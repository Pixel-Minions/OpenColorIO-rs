# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""What the Windows and Linux loaders share: sections, functions, imports and names."""

from __future__ import annotations

import bisect
import hashlib
import itertools
import re
from dataclasses import dataclass, field
from pathlib import Path

import capstone


@dataclass
class Section:
    name: str
    va: int
    size: int
    offset: int
    raw_size: int
    executable: bool


@dataclass
class Function:
    start: int
    end: int
    # Every name this code has; the first is the one displayed. Empty for an unnamed
    # Windows function, which is shown as `sub_<address>`.
    names: list[str] = field(default_factory=list)
    # Where the bounds come from: "symbol" (ELF), "pdata" (Windows unwind table), "leaf"
    # (Windows code without unwind data, bounded by int3 padding).
    source: str = ""
    # Windows: further address ranges of the same function (chained unwind entries).
    fragments: list[tuple[int, int]] = field(default_factory=list)

    @property
    def name(self) -> str:
        return self.names[0] if self.names else f"sub_{self.start:x}"


@dataclass
class Import:
    name: str
    library: str
    version: str = ""  # Linux symbol version, e.g. GLIBC_2.2.5
    slot: int = 0  # the IAT or GOT slot the loader fills in
    stubs: list[int] = field(default_factory=list)  # import thunks / PLT entries
    stub_label: str = ""  # how `name_of` describes a stub: PLT, thunk, SVML

    @property
    def display(self) -> str:
        return f"{self.name}@{self.version}" if self.version else self.name


@dataclass
class Insn:
    address: int
    size: int
    mnemonic: str
    op_str: str


_RIP = re.compile(r"\[rip ([+-]) 0x([0-9a-f]+)\]")


def rip_target(insn: Insn) -> int | None:
    """The absolute address of a RIP-relative memory operand."""
    m = _RIP.search(insn.op_str)
    if not m:
        return None
    disp = int(m.group(2), 16)
    return insn.address + insn.size + (disp if m.group(1) == "+" else -disp)


def branch_target(insn: Insn) -> int | None:
    """The target of a direct call or jump (`call 0x1234`)."""
    if (insn.mnemonic == "call" or insn.mnemonic.startswith("j")) and insn.op_str.startswith("0x"):
        return int(insn.op_str, 16)
    return None


def new_capstone(att: bool = False) -> capstone.Cs:
    md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_64)
    md.skipdata = True  # undecodable bytes (data inside .text) become `.byte`, as in objdump
    if att:
        md.syntax = capstone.CS_OPT_SYNTAX_ATT
    return md


class Image:
    """A loaded library. Addresses are virtual addresses as the spikes cite them: Windows with
    the DLL's image base (0x180000000), Linux as the ELF file's (the .so's own addresses)."""

    platform = ""

    def __init__(self, path: Path):
        self.path = Path(path)
        self.data = self.path.read_bytes()
        self.sha256 = hashlib.sha256(self.data).hexdigest()
        self.sections: list[Section] = []
        self.functions: list[Function] = []
        self.imports: list[Import] = []
        self.slots: dict[int, Import] = {}  # IAT / GOT slot -> import
        self.stubs: dict[int, Import] = {}  # thunk / PLT entry -> import
        self.labels: dict[int, str] = {}  # named data (vtables, ...)
        self._ranges: list[tuple[int, int, Function]] = []
        self._starts: list[int] = []
        self._sweep: list[Insn] | None = None
        self._index: dict[int, int] | None = None
        self._call_sites: dict[int, list[tuple[int, str]]] | None = None

    # -- addresses -------------------------------------------------------------------------

    def section_of(self, va: int) -> Section | None:
        for s in self.sections:
            if s.va <= va < s.va + s.size:
                return s
        return None

    def read(self, va: int, n: int) -> bytes:
        s = self.section_of(va)
        if s is None:
            return b""
        off = va - s.va
        if off >= s.raw_size:
            return b"\0" * n  # uninitialized data
        return self.data[s.offset + off : s.offset + min(off + n, s.raw_size)]

    def _index_functions(self) -> None:
        self.functions.sort(key=lambda f: (f.start, -f.end))
        ranges = [(f.start, f.end, f) for f in self.functions]
        ranges += [(a, b, f) for f in self.functions for a, b in f.fragments]
        ranges.sort(key=lambda r: (r[0], -r[1]))
        self._ranges = ranges
        self._starts = [r[0] for r in ranges]

    def function_at(self, va: int) -> Function | None:
        """The function containing `va` (in its main range or one of its fragments)."""
        i = bisect.bisect_right(self._starts, va) - 1
        # Symbols can alias or nest, so look at a few ranges that start before `va`.
        for start, end, f in reversed(self._ranges[max(0, i - 7) : i + 1]):
            if start <= va < end:
                return f
        return None

    def name_of(self, va: int) -> str | None:
        """A name for an address: an import stub, a function (+offset) or a label."""
        if va in self.stubs:
            imp = self.stubs[va]
            label = imp.stub_label or ("PLT" if self.platform == "linux" else "thunk")
            return f"{imp.display} ({label})"
        if va in self.labels:
            return self.labels[va]
        f = self.function_at(va)
        if f is not None:
            return f.name if va == f.start else f"{f.name}+{va - f.start:#x}"
        return None

    # -- code ------------------------------------------------------------------------------

    def disasm(self, start: int, end: int, att: bool = False) -> list[Insn]:
        md = new_capstone(att)
        return [Insn(*t) for t in md.disasm_lite(self.read(start, end - start), start)]

    def sweep(self) -> list[Insn]:
        """Every instruction of the executable sections, decoded linearly but restarted at
        each known function start, so data inside .text cannot shift the code after it."""
        if self._sweep is None:
            md = new_capstone()
            out: list[Insn] = []
            starts = {a for f in self.functions for a in (f.start, *(r[0] for r in f.fragments))}
            for s in self.sections:
                if not s.executable:
                    continue
                cuts = sorted(
                    {s.va, s.va + s.size} | {a for a in starts if s.va < a < s.va + s.size}
                )
                for a, b in itertools.pairwise(cuts):
                    out.extend(Insn(*t) for t in md.disasm_lite(self.read(a, b - a), a))
            self._sweep = out
            self._index = {insn.address: i for i, insn in enumerate(out)}
        return self._sweep

    def function_insns(self, f: Function) -> list[Insn]:
        """The instructions of a function: its main range, then its fragments."""
        self.sweep()
        out: list[Insn] = []
        for a, b in [(f.start, f.end), *sorted(f.fragments)]:
            i = self._index.get(a)
            if i is None:  # not a boundary of the sweep (a function found later)
                out.extend(self.disasm(a, b))
                continue
            while i < len(self._sweep) and self._sweep[i].address < b:
                out.append(self._sweep[i])
                i += 1
        return out

    def insn_at(self, va: int) -> Insn | None:
        self.sweep()
        i = self._index.get(va)
        return None if i is None else self._sweep[i]

    def resolve_call(self, insn: Insn) -> tuple[int | None, Import | None]:
        """Where a call or jump goes: (target address, import it reaches, if any)."""
        t = branch_target(insn)
        if t is not None:
            return t, self.reached_import(t)
        if insn.mnemonic in ("call", "jmp") and insn.op_str.startswith("qword ptr [rip"):
            return None, self.slots.get(rip_target(insn))
        return None, None

    def call_sites(self) -> dict[int, list[tuple[int, str]]]:
        """Every call, jump or pointer load that reaches an import, by `id(import)`: a list of
        (address, mnemonic, or "ref" for a load of its IAT or GOT slot)."""
        if self._call_sites is None:
            sites: dict[int, list[tuple[int, str]]] = {}
            for insn in self.sweep():
                if insn.address in self.stubs or self.section_of(insn.address).name.startswith(
                    ".plt"
                ):
                    continue
                target, imp = self.resolve_call(insn)
                kind = insn.mnemonic
                if imp is None and target is None:
                    slot = rip_target(insn)
                    imp, kind = (self.slots.get(slot) if slot is not None else None), "ref"
                if imp is not None:
                    sites.setdefault(id(imp), []).append((insn.address, kind))
            self._call_sites = sites
        return self._call_sites

    def reached_import(self, va: int, depth: int = 3) -> Import | None:
        """The import a call target reaches through thunks (`jmp [iat]`, `jmp 0x...`)."""
        if va in self.stubs:
            return self.stubs[va]
        if depth == 0:
            return None
        insn = self.insn_at(va)
        if insn is None or insn.mnemonic != "jmp":
            return None
        t = branch_target(insn)
        if t is not None:
            return self.reached_import(t, depth - 1)
        slot = rip_target(insn)
        return self.slots.get(slot) if slot is not None else None
