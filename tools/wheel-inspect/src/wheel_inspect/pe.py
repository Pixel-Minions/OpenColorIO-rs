# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""The Windows library, `OpenColorIO_2_5.dll` (MSVC, x64). It has no symbols, so functions
come from other places:

- the exception directory (`.pdata`): the bounds of every function that has unwind data
  (every function that calls another one or uses the stack);
- the export table: the public API;
- MSVC's RTTI: each polymorphic class's type descriptor leads to its complete object
  locator and then to its vtable, whose entries are the class's virtual functions. The
  Linux build names those entries (see `names.py`);
- import thunks (`jmp [iat]`), found by decoding the code.
"""

from __future__ import annotations

import bisect
import re
import struct
from dataclasses import dataclass, field
from pathlib import Path

from .image import Function, Image, Import, Section, rip_target


def _u16(d: bytes, o: int) -> int:
    return struct.unpack_from("<H", d, o)[0]


def _u32(d: bytes, o: int) -> int:
    return struct.unpack_from("<I", d, o)[0]


def _u64(d: bytes, o: int) -> int:
    return struct.unpack_from("<Q", d, o)[0]


UNW_FLAG_CHAININFO = 0x4


@dataclass
class RttiClass:
    """A polymorphic class, from its MSVC type descriptor."""

    raw: str  # the decorated type name, e.g. .?AVCameraLog2LinRenderer@OpenColorIO_v2_5@@
    name: str  # e.g. OpenColorIO_v2_5::CameraLog2LinRenderer
    descriptor: int  # address of the type descriptor
    # (address of the vtable, offset of this subobject in the class, entries), primary first.
    vtables: list[tuple[int, int, list[int]]] = field(default_factory=list)


class PeImage(Image):
    platform = "windows"

    def __init__(self, path: Path):
        super().__init__(path)
        d = self.data
        pe = _u32(d, 0x3C)
        if d[pe : pe + 4] != b"PE\0\0":
            raise ValueError(f"{path}: not a PE file")
        nsec, optsz = _u16(d, pe + 6), _u16(d, pe + 20)
        opt = pe + 24
        if _u16(d, opt) != 0x20B:
            raise ValueError(f"{path}: not PE32+")
        self.image_base = _u64(d, opt + 24)
        sh = opt + optsz
        for i in range(nsec):
            o = sh + 40 * i
            name = d[o : o + 8].rstrip(b"\0").decode("ascii", "replace")
            vsize, va, rsize, rptr = struct.unpack_from("<IIII", d, o + 8)
            chars = _u32(d, o + 36)
            self.sections.append(
                Section(
                    name,
                    self.image_base + va,
                    vsize,
                    rptr,
                    min(rsize, vsize),
                    bool(chars & 0x20000000),
                )
            )

        def directory(i: int) -> tuple[int, int]:
            return _u32(d, opt + 112 + 8 * i), _u32(d, opt + 112 + 8 * i + 4)

        self._read_imports(*directory(1))
        self._read_exports(*directory(0))
        self._read_pdata(*directory(3))
        self._index_functions()
        self._find_thunks()
        self.classes = self._read_rtti()

    def rva(self, rva: int, n: int) -> bytes:
        return self.read(self.image_base + rva, n)

    def _cstr(self, rva: int) -> str:
        b = self.rva(rva, 512)
        return b[: b.index(b"\0")].decode("ascii", "replace")

    def _read_imports(self, rva: int, size: int) -> None:
        while size:
            ilt, _, _, name_rva, iat = struct.unpack("<IIIII", self.rva(rva, 20))
            if name_rva == 0:
                break
            library = self._cstr(name_rva)
            lookup = ilt or iat
            for k in range(1 << 16):
                entry = _u64(self.rva(lookup + 8 * k, 8), 0)
                if entry == 0:
                    break
                name = f"#{entry & 0xFFFF}" if entry >> 63 else self._cstr((entry & 0x7FFFFFFF) + 2)
                imp = Import(name, library, slot=self.image_base + iat + 8 * k)
                self.imports.append(imp)
                self.slots[imp.slot] = imp
            rva += 20

    def _read_exports(self, rva: int, size: int) -> None:
        self.exports: dict[str, int] = {}
        if not size:
            return
        e = self.rva(rva, 40)
        _, nnames, afun, anames, aords = struct.unpack_from("<IIIII", e, 20)
        for k in range(nnames):
            name = self._cstr(_u32(self.rva(anames + 4 * k, 4), 0))
            ordinal = _u16(self.rva(aords + 2 * k, 2), 0)
            target = _u32(self.rva(afun + 4 * ordinal, 4), 0)
            if not (rva <= target < rva + size):  # skip forwarders
                self.exports[name] = self.image_base + target

    def _read_pdata(self, rva: int, size: int) -> None:
        """One function per primary RUNTIME_FUNCTION; chained entries (UNW_FLAG_CHAININFO,
        code MSVC moved out of the function's main range) become its fragments."""
        table = self.rva(rva, size)
        entries = [struct.unpack_from("<III", table, 12 * k) for k in range(size // 12)]
        primary: dict[int, Function] = {}
        chained: list[tuple[int, int, int]] = []
        for begin, end, unwind in entries:
            if begin == 0:
                continue
            info = self.rva(unwind & ~1, 4)
            flags = info[0] >> 3
            if flags & UNW_FLAG_CHAININFO:
                ncodes = info[2]
                parent = _u32(self.rva((unwind & ~1) + 4 + 2 * (ncodes + (ncodes & 1)), 4), 0)
                chained.append((begin, end, parent))
            else:
                f = Function(self.image_base + begin, self.image_base + end, source="pdata")
                primary[begin] = f
                self.functions.append(f)
        parent_of = {begin: parent for begin, _, parent in chained}
        for begin, end, parent in chained:
            for _ in range(16):  # a chain can go through several entries
                if parent in primary or parent not in parent_of:
                    break
                parent = parent_of[parent]
            owner = primary.get(parent)
            if owner is not None:
                owner.fragments.append((self.image_base + begin, self.image_base + end))
            else:
                self.functions.append(
                    Function(self.image_base + begin, self.image_base + end, source="pdata")
                )

    def _find_thunks(self) -> None:
        """Import thunks are `jmp qword ptr [rip + iat]` instructions outside any function."""
        for insn in self.sweep():
            if insn.mnemonic == "jmp" and insn.op_str.startswith("qword ptr [rip"):
                slot = rip_target(insn)
                if slot in self.slots and Image.function_at(self, insn.address) is None:
                    self.stubs[insn.address] = self.slots[slot]
                    self.slots[slot].stubs.append(insn.address)

    def leaf_from(self, va: int) -> Function:
        """A function without unwind data (a leaf, or a thunk) that starts at `va`. MSVC pads
        between functions with int3, so it ends at the first int3 (or the next known function)."""
        i = bisect.bisect_right(self._starts, va)
        sec = self.section_of(va)
        hi = min(self._starts[i] if i < len(self._starts) else sec.va + sec.size, va + 0x2000)
        end = va + 1
        for insn in self.disasm(va, hi):
            if insn.mnemonic == "int3":
                break
            end = insn.address + insn.size
        return Function(va, end, source="leaf")

    def function_at(self, va: int) -> Function | None:
        f = super().function_at(va)
        sec = self.section_of(va)
        if f is not None or sec is None or not sec.executable:
            return f
        # A leaf function: it starts after the int3 padding that precedes `va`.
        i = bisect.bisect_right(self._starts, va) - 1
        lo = max([sec.va] + [e for _, e, _ in self._ranges[max(0, i - 7) : i + 1] if e <= va])
        start = lo
        for insn in self.disasm(lo, va + 1):
            if insn.mnemonic == "int3":
                start = insn.address + insn.size
        return self.leaf_from(start) if start <= va else None

    # -- RTTI ------------------------------------------------------------------------------

    def _read_rtti(self) -> list[RttiClass]:
        data = next((s for s in self.sections if s.name == ".data"), None)
        rdata = next((s for s in self.sections if s.name == ".rdata"), None)
        text = next(s for s in self.sections if s.name == ".text")
        if data is None or rdata is None:
            return []
        # Type descriptors (in .data): {vftable pointer, spare, name}.
        blob = self.data[data.offset : data.offset + data.raw_size]
        descriptors: dict[int, str] = {}
        for m in re.finditer(rb"\.\?A[VU][\x21-\x7e]+?@@\0", blob):
            descriptors[data.va + m.start() - 16 - self.image_base] = m.group()[:-1].decode()
        # Complete object locators (in .rdata): {signature 1, offset, cdOffset,
        # type descriptor RVA, class hierarchy RVA, own RVA}.
        rblob = self.data[rdata.offset : rdata.offset + rdata.raw_size]
        base_rva = rdata.va - self.image_base
        locators: dict[int, tuple[int, int]] = {}  # COL address -> (descriptor RVA, offset)
        for m in re.finditer(rb"\x01\x00\x00\x00", rblob):
            o = m.start()
            if o % 4 or o + 24 > len(rblob):
                continue
            _, offset, _, td, _, own = struct.unpack_from("<IIIIII", rblob, o)
            if own == base_rva + o and td in descriptors:
                locators[self.image_base + base_rva + o] = (td, offset)
        # A vtable is preceded by a pointer to its locator.
        by_td: dict[int, RttiClass] = {}
        for o in range(0, len(rblob) - 15, 8):
            col = _u64(rblob, o)
            if col not in locators:
                continue
            td, offset = locators[col]
            start = rdata.va + o + 8
            entries = []
            while len(b := self.read(start + 8 * len(entries), 8)) == 8:
                p = _u64(b, 0)
                if not (text.va <= p < text.va + text.size):
                    break
                entries.append(p)
            raw = descriptors[td]
            cls = by_td.setdefault(td, RttiClass(raw, demangle_type(raw), self.image_base + td))
            cls.vtables.append((start, offset, entries))
        for cls in by_td.values():
            cls.vtables.sort(key=lambda v: (v[1], v[0]))
            self.labels[cls.vtables[0][0]] = f"{cls.name}::`vftable'"
        return sorted(by_td.values(), key=lambda c: c.name)


def demangle_type(raw: str) -> str:
    """An RTTI type name (`.?AVName@Scope@@`) as C++ (`Scope::Name`). Covers what OCIO's
    classes use: namespaces, anonymous namespaces, templates with integer and simple type
    arguments, and name back-references. Anything else is returned as it is."""
    try:
        p = _MsvcName(raw)
        if raw[:4] not in (".?AV", ".?AU"):
            return raw
        p.i = 4
        name = p.qualified([])
        return name if p.i == len(raw) else raw
    except (IndexError, ValueError):
        return raw


_BUILTIN = {
    "C": "signed char",
    "D": "char",
    "E": "unsigned char",
    "F": "short",
    "G": "unsigned short",
    "H": "int",
    "I": "unsigned int",
    "J": "long",
    "K": "unsigned long",
    "M": "float",
    "N": "double",
    "O": "long double",
    "X": "void",
    "_N": "bool",
    "_J": "__int64",
    "_K": "unsigned __int64",
    "_W": "wchar_t",
}


class _MsvcName:
    """A small recursive-descent reader of MSVC's decorated names."""

    def __init__(self, s: str):
        self.s, self.i = s, 0

    def take(self, n: int = 1) -> str:
        t = self.s[self.i : self.i + n]
        if len(t) < n:
            raise IndexError
        self.i += n
        return t

    def ident(self) -> str:
        j = self.s.index("@", self.i)
        t, self.i = self.s[self.i : j], j + 1
        return t

    def qualified(self, names: list[str]) -> str:
        """Name fragments, innermost first, up to the terminating `@`."""
        parts = []
        while self.s[self.i] != "@":
            parts.append(self.fragment(names))
        self.i += 1
        return "::".join(reversed(parts))

    def fragment(self, names: list[str]) -> str:
        c = self.s[self.i]
        if c.isdigit():
            self.i += 1
            return names[int(c)]
        if self.s.startswith("?$", self.i):
            self.i += 2
            inner = [self.ident()]  # a template has its own back-reference table
            args = []
            while self.s[self.i] != "@":
                args.append(self.template_arg(inner))
            self.i += 1
            t = f"{inner[0]}<{', '.join(args)}>"
        elif self.s.startswith("?A0x", self.i):
            self.ident()
            t = "(anonymous namespace)"
        elif c == "?":
            raise ValueError("unsupported name fragment")
        else:
            t = self.ident()
        if len(names) < 10:
            names.append(t)
        return t

    def template_arg(self, names: list[str]) -> str:
        if self.s.startswith("$0", self.i):
            self.i += 2
            return str(self.number())
        return self.type(names)

    def number(self) -> int:
        neg = self.s[self.i] == "?"
        self.i += neg
        c = self.take()
        if c.isdigit():
            v = int(c) + 1
        else:
            v = 0
            while c != "@":
                if not "A" <= c <= "P":
                    raise ValueError("bad number")
                v = v * 16 + ord(c) - ord("A")
                c = self.take()
        return -v if neg else v

    def type(self, names: list[str]) -> str:
        c = self.take()
        if c == "_":
            c += self.take()
        if c in _BUILTIN:
            return _BUILTIN[c]
        if c in ("V", "U"):
            return self.qualified(names)
        if c == "W":
            self.take()  # the enum's underlying type
            return self.qualified(names)
        raise ValueError(f"unsupported type code {c}")
