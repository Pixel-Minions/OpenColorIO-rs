# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Names for the Windows DLL's code, which has no symbols, and name matching for `find`.

Virtual functions: both builds compile the same classes, so a class's vtable has the same
slots in both, except that GCC puts two entries (complete and deleting) where MSVC has one
(the scalar deleting destructor). Slot k of a Windows RTTI vtable is therefore named after
the Linux symbol in the corresponding slot of the same class's vtable. Classes whose vtables
do not line up, or which have overloaded virtual functions (MSVC groups overloads), are left
unnamed.

Exports are named from their decorated names; `known.py` names what neither gives.
"""

from __future__ import annotations

import re
from dataclasses import dataclass

from . import known
from .elf import ElfImage
from .image import Function, Image, Import
from .pe import PeImage, _MsvcName


def strip_params(demangled: str) -> str:
    """The qualified name of a demangled function: no return type, no parameters."""
    s = re.sub(r"( \[clone [^\]]*\])+$", "", demangled)
    s = re.sub(r"( const| volatile| &&| &| noexcept)+$", "", s)
    if s.endswith(")"):
        depth = 0
        for i in range(len(s) - 1, -1, -1):
            depth += {")": 1, "(": -1}.get(s[i], 0)
            if depth == 0:
                s = s[:i]
                break
    depth = 0
    for i in range(len(s) - 1, -1, -1):
        c = s[i]
        depth += {">": 1, ")": 1, "<": -1, "(": -1}.get(c, 0)
        if c == " " and depth == 0 and not s[:i].endswith("operator"):
            return s[i + 1 :]
    return s


def strip_templates(name: str) -> str:
    out, depth = [], 0
    for c in name:
        if c == "<":
            depth += 1
        elif c == ">":
            depth -= 1
        elif depth == 0:
            out.append(c)
    return "".join(out)


def canonical(name: str) -> str:
    """A spelling both builds share: `(Type)1` and `true` become `1`, no spaces."""
    name = re.sub(r"\((?:[\w:]+ )*[\w:]+\)(-?\d+)", r"\1", name)
    name = re.sub(r"\btrue\b", "1", re.sub(r"\bfalse\b", "0", name))
    return name.replace(" ", "")


def matches(query: str, demangled: str) -> bool:
    """`query` names the function: equal to its qualified name, or a `::`-suffix of it, with
    or without template arguments (`CameraLog2LinRenderer::apply`, `GetLogSideBreak`). A query
    with a parameter list compares the parameters too."""
    q = canonical(query)
    # Parentheses in `(anonymous namespace)` or in template arguments are not parameters.
    if "(" in strip_templates(query).replace("(anonymous namespace)", ""):
        candidates = [canonical(demangled)]
    else:
        full = canonical(strip_params(demangled))
        candidates = [full, strip_templates(full)]
    return any(name == q or name.endswith("::" + q) for name in candidates)


def msvc_function_name(decorated: str) -> str | None:
    """`?getCacheID@Config@OpenColorIO_v2_5@@QEBAPEBDXZ` to `OpenColorIO_v2_5::Config::getCacheID`
    (constructors and destructors too); other special names give None."""
    try:
        if decorated.startswith(("??0", "??1")):
            p = _MsvcName(decorated)
            p.i = 3
            cls = p.qualified([])
            short = strip_templates(cls).rsplit("::", 1)[-1]
            return f"{cls}::{'~' if decorated[2] == '1' else ''}{short}"
        if decorated.startswith("?") and not decorated.startswith("??"):
            p = _MsvcName(decorated)
            p.i = 1
            return p.qualified([])
    except (IndexError, ValueError):
        pass
    return None


@dataclass
class VirtualSlot:
    cls: str  # the class, as the Windows RTTI names it
    vtable: int  # Windows vtable address
    slot: int  # Windows slot
    linux_slot: int
    linux_class: str  # the Linux class whose vtable named it: the same, or another instantiation


def _msvc_slots(names: list[str | None]) -> list[tuple[int, str | None]] | None:
    """A GCC primary vtable's entries as MSVC lays them out: (GCC slot, name) per MSVC slot.
    GCC's complete and deleting destructors (or two empty slots) are one MSVC slot. MSVC
    groups a virtual function's overloads in its own order, so they stay unnamed; if a group
    is not contiguous, the other slots may move too, and the class gives None."""
    slots: list[tuple[int, str | None]] = []
    for k, n in enumerate(names):
        prev = slots[-1][1] if slots else ""
        if (n is None and prev is None) or (n is not None and "::~" in n and prev == n):
            continue
        slots.append((k, n))
    methods = [
        strip_params(n).rsplit("::", 1)[-1] if n and not n.startswith("__cxa_") else None
        for _, n in slots
    ]
    for m in {m for m in methods if m is not None and methods.count(m) > 1}:
        at = [i for i, x in enumerate(methods) if x == m]
        if at[-1] - at[0] + 1 != len(at):
            return None
        for i in at:
            slots[i] = (slots[i][0], None)
    return slots


def name_windows(win: PeImage, linux: ElfImage | None) -> dict:
    """Names the DLL's exports, curated functions and (with the Linux image) virtual functions.
    Returns counts for `locate`."""
    stats = {"exports": 0, "known": 0, "classes_named": 0, "classes_unmatched": 0, "slots": 0}
    new: list[Function] = []
    by_start = {f.start: f for f in win.functions}

    def function(va: int) -> Function:
        f = by_start.get(va)
        if f is None:
            f = by_start[va] = win.leaf_from(va)
            new.append(f)
        return f

    win.slot_of: dict[int, list[VirtualSlot]] = {}
    if linux is not None:
        lin: dict[str, tuple[str, list] | None] = {}
        templates: dict[str, list[tuple[str, list]]] = {}
        for cls, entries in sorted(linux.vtables().items()):
            key = canonical(cls)
            lin[key] = None if key in lin else (cls, entries)  # None: several classes
            if "<" in key:
                templates.setdefault(strip_templates(key), []).append((cls, entries))

        def slots_of(entries: list) -> list | None:
            return _msvc_slots([linux.name_of(e) if isinstance(e, int) else e for e in entries])

        for c in win.classes:
            if not c.vtables or c.vtables[0][1] != 0:
                continue
            vtable, _, wentries = c.vtables[0]
            match = lin.get(canonical(c.name))
            sibling = False
            if match is None and "<" in c.name:
                # An instantiation only MSVC compiled (e.g. the SVML PQ renderers): another
                # instantiation of the same template with the same slots names its own methods.
                for cls, entries in templates.get(strip_templates(canonical(c.name)), []):
                    s = slots_of(entries)
                    if s is not None and len(s) == len(wentries):
                        match, sibling = (cls, entries), True
                        break
            if match is None:
                continue
            lcls, entries = match
            slots = slots_of(entries)
            # With multiple inheritance, GCC appends the overriders of the other bases'
            # functions to the primary vtable; MSVC keeps them in the other vtables only.
            if slots is None or (len(wentries) < len(slots) and len(c.vtables) == 1):
                stats["classes_unmatched"] += 1
                continue
            stats["classes_named"] += 1
            for wslot, ((lslot, n), va) in enumerate(zip(slots, wentries, strict=False)):
                if n is None or n.startswith("__cxa_"):
                    continue
                if sibling:
                    if not n.startswith(lcls + "::"):
                        continue  # inherited: named through the class that defines it
                    n = c.name + n[len(lcls) :]
                f = function(va)
                if n not in f.names:
                    f.names.append(n)
                win.slot_of.setdefault(va, []).append(
                    VirtualSlot(c.name, vtable, wslot, lslot, lcls)
                )
                stats["slots"] += 1
    for decorated, va in sorted(win.exports.items()):
        name = msvc_function_name(decorated)
        sec = win.section_of(va)
        if name and sec is not None and sec.executable:
            f = function(va)
            if name not in f.names:
                f.names.append(name)
                stats["exports"] += 1
    win.known_names = {}
    if win.sha256 == known.WINDOWS_SHA256:
        for va, name, kind, why in known.WINDOWS:
            f = function(va)
            f.names.insert(0, name)
            win.known_names[va] = why
            stats["known"] += 1
            if kind == "svml-entry":
                # Listed with the imports, so `imports` finds its call sites.
                imp = Import(name, "SVML, linked statically", stubs=[va], stub_label="SVML")
                win.imports.append(imp)
                win.stubs[va] = imp
    win.functions.extend(new)
    win._index_functions()
    return stats


def fingerprint(img: Image, f: Function) -> list[str]:
    """The imports a function calls, in address order, as double-precision libm names
    (`log2f` and `log2` both give `log2`), so the two builds can be compared."""
    out = []
    for insn in img.function_insns(f):
        if insn.mnemonic in ("call", "jmp"):
            _, imp = img.resolve_call(insn)
            if imp is not None:
                out.append(_libm_base(imp.name))
    return out


def same_imports(win: Image, linux: Image, lf: Function) -> tuple[list[str], list[Function]]:
    """The Windows functions (with unwind data) that call the same imports as the Linux
    function `lf`: a way to find a function the DLL does not name, if it calls any."""
    want = fingerprint(linux, lf)
    if not want:
        return want, []
    if not hasattr(win, "_fingerprints"):
        win._fingerprints = {
            f.start: fingerprint(win, f) for f in win.functions if f.source == "pdata"
        }
    return want, [
        f for f in win.functions if f.source == "pdata" and win._fingerprints[f.start] == want
    ]


_LIBM_F = re.compile(
    r"^(a?(sin|cos|tan)h?|atan2|exp2?|expm1|log(2|10|1p)?|pow|sqrt|cbrt|floor|ceil|round|fmod|modf|frexp|ldexp|hypot)f$"
)


def _libm_base(name: str) -> str:
    return name[:-1] if _LIBM_F.match(name) else name
