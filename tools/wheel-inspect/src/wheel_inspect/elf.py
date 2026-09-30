# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""The Linux library, `libOpenColorIO.so` (GCC, x86-64). Its full symbol table is present, so
functions and data have names; imports carry their glibc symbol versions (`log2@GLIBC_2.2.5`).
"""

from __future__ import annotations

import bisect
import io
import re
from pathlib import Path

import cpp_demangle
from elftools.elf.elffile import ELFFile
from elftools.elf.relocation import RelocationSection
from elftools.elf.sections import SymbolTableSection

from .image import Function, Image, Import, Section, rip_target

SHF_ALLOC, SHF_EXECINSTR = 0x2, 0x4


_SPECIAL = [
    (re.compile(r"\{vtable\((.*)\)\}"), "vtable for {}"),
    (re.compile(r"\{vtt\((.*)\)\}"), "VTT for {}"),
    (
        re.compile(r"\{virtual override thunk\(\{offset\(-?\d+\)\}, (.*)\)\}"),
        "non-virtual thunk to {}",
    ),
]


def demangle(name: str) -> str:
    """Itanium demangling (the Rust cpp_demangle crate), with c++filt's wording for vtables
    and thunks. Names it cannot read stay mangled."""
    if not name.startswith("_Z"):
        return name
    try:
        d = cpp_demangle.demangle(name)
    except Exception:
        return name
    for rx, fmt in _SPECIAL:
        m = rx.fullmatch(d)
        if m:
            return fmt.format(m.group(1))
    return d


class ElfImage(Image):
    platform = "linux"

    def __init__(self, path: Path):
        super().__init__(path)
        elf = ELFFile(io.BytesIO(self.data))
        for s in elf.iter_sections():
            if s["sh_flags"] & SHF_ALLOC and s["sh_addr"]:
                raw = 0 if s["sh_type"] == "SHT_NOBITS" else s["sh_size"]
                self.sections.append(
                    Section(
                        s.name,
                        s["sh_addr"],
                        s["sh_size"],
                        s["sh_offset"],
                        raw,
                        bool(s["sh_flags"] & SHF_EXECINSTR),
                    )
                )
        self._read_symbols(elf)
        self._read_imports(elf)
        self._read_relocations(elf)
        self._index_functions()
        self._find_plt()

    def _read_symbols(self, elf: ELFFile) -> None:
        by_range: dict[tuple[int, int], Function] = {}
        data: dict[int, tuple[int, str]] = {}
        for name in (".symtab", ".dynsym"):
            sec = elf.get_section_by_name(name)
            if not isinstance(sec, SymbolTableSection):
                continue
            for sym in sec.iter_symbols():
                if sym["st_shndx"] == "SHN_UNDEF" or not sym.name:
                    continue
                kind, value, size = sym["st_info"]["type"], sym["st_value"], sym["st_size"]
                if kind in ("STT_FUNC", "STT_GNU_IFUNC") and size:
                    f = by_range.setdefault(
                        (value, value + size), Function(value, value + size, source="symbol")
                    )
                    d = demangle(sym.name)
                    if d not in f.names:
                        f.names.append(d)
                elif kind == "STT_OBJECT" and size:
                    data.setdefault(value, (size, demangle(sym.name)))
        self.functions = list(by_range.values())
        for f in self.functions:
            f.names.sort(key=lambda n: (n.startswith("_Z"), len(n), n))
        self._data = sorted((a, a + size, name) for a, (size, name) in data.items())
        self._data_starts = [d[0] for d in self._data]

    def _read_imports(self, elf: ELFFile) -> None:
        dynsym = elf.get_section_by_name(".dynsym")
        versym = elf.get_section_by_name(".gnu.version")
        verneed = elf.get_section_by_name(".gnu.version_r")
        versions: dict[int, tuple[str, str]] = {}  # version index -> (library, version)
        if verneed is not None:
            for need, auxes in verneed.iter_versions():
                for aux in auxes:
                    versions[aux["vna_other"]] = (need.name, aux.name)
        self._dynsym_imports: dict[int, Import] = {}
        for i, sym in enumerate(dynsym.iter_symbols()):
            if sym["st_shndx"] != "SHN_UNDEF" or not sym.name:
                continue
            index = versym.get_symbol(i)["ndx"] if versym is not None else 0
            library, version = versions.get(index if isinstance(index, int) else 0, ("", ""))
            imp = Import(sym.name, library, version)
            self.imports.append(imp)
            self._dynsym_imports[i] = imp

    def _read_relocations(self, elf: ELFFile) -> None:
        """GOT slots of the imports, and the pointers the loader writes into data (vtables)."""
        self.pointers: dict[int, int | str] = {}  # address -> target address, or import name
        dynsym = elf.get_section_by_name(".dynsym")
        for sec in elf.iter_sections():
            if not isinstance(sec, RelocationSection) or not sec.is_RELA():
                continue
            for r in sec.iter_relocations():
                kind, offset, sym_index = r["r_info_type"], r["r_offset"], r["r_info_sym"]
                if kind == 8:  # R_X86_64_RELATIVE
                    self.pointers[offset] = r["r_addend"]
                elif kind in (1, 6, 7):  # R_X86_64_64, GLOB_DAT, JUMP_SLOT
                    imp = self._dynsym_imports.get(sym_index)
                    if imp is not None:
                        self.pointers[offset] = imp.name
                        if kind != 1:  # a GOT slot, which code calls through or loads
                            self.slots[offset] = imp
                            if kind == 7 or not imp.slot:
                                imp.slot = offset
                    else:
                        sym = dynsym.get_symbol(sym_index)
                        self.pointers[offset] = sym["st_value"] + r["r_addend"]

    def _find_plt(self) -> None:
        """Each PLT entry (16 bytes) jumps through the GOT slot of one import."""
        for s in self.sections:
            if not s.name.startswith(".plt"):
                continue
            for insn in self.disasm(s.va, s.va + s.size):
                slot = rip_target(insn) if insn.mnemonic in ("jmp", "bnd jmp") else None
                if slot in self.slots:
                    entry = s.va + (insn.address - s.va) // 16 * 16
                    imp = self.slots[slot]
                    self.stubs[entry] = imp
                    imp.stubs.append(entry)

    def name_of(self, va: int) -> str | None:
        name = super().name_of(va)
        if name is not None:
            return name
        i = bisect.bisect_right(self._data_starts, va) - 1
        if i >= 0 and self._data[i][0] <= va < self._data[i][1]:
            start, _, n = self._data[i]
            return n if va == start else f"{n}+{va - start:#x}"
        return None

    def vtables(self) -> dict[str, list[int | str | None]]:
        """Each class's primary vtable: `vtable for X` -> its virtual functions in slot order:
        addresses, import names (`__cxa_pure_virtual`), or None for an empty slot (GCC leaves
        both destructor slots of an abstract class empty)."""
        out: dict[str, list[int | str | None]] = {}
        for start, end, name in self._data:
            if not name.startswith("vtable for "):
                continue
            entries: list[int | str | None] = []
            # {offset to top, typeinfo pointer, virtual functions...}; the primary group ends
            # where the next group begins: its offset-to-top, then a pointer to a typeinfo.
            for a in range(start + 16, end, 8):
                target = self.pointers.get(a)
                if target is None:
                    after = self.pointers.get(a + 8)
                    next_group = isinstance(after, int) and self.function_at(after) is None
                    if int.from_bytes(self.read(a, 8), "little") or next_group:
                        break
                    entries.append(None)
                elif isinstance(target, int) and self.function_at(target) is None:
                    break
                else:
                    entries.append(target)
            out[name[len("vtable for ") :]] = entries
        return out
