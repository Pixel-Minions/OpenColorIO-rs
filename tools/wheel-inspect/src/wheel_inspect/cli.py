# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Command line: `uv run --project tools/wheel-inspect wheel-inspect <command>`."""

from __future__ import annotations

import argparse
import importlib.metadata
import math
import re
import struct
import sys

from . import names, wheels
from .elf import ElfImage
from .image import Function, Image, Insn, rip_target
from .pe import PeImage

PLATFORMS = ("windows", "linux")
_images: dict[str, Image] = {}


def log(msg: str) -> None:
    print(msg, file=sys.stderr)


def image(platform: str) -> Image:
    """The platform's library from the pinned wheel, loaded and named (cached)."""
    if platform not in _images:
        w = wheels.locate(platform, log)
        if platform == "windows":
            img = PeImage(w.library.path)
            _images[platform] = img
            img.naming = names.name_windows(img, image("linux"))
        else:
            _images[platform] = ElfImage(w.library.path)
    return _images[platform]


def selected(args) -> list[str]:
    if getattr(args, "win", False):
        return ["windows"]
    if getattr(args, "linux", False):
        return ["linux"]
    return list(PLATFORMS)


def versions() -> str:
    v = importlib.metadata.version
    return (
        f"capstone {v('capstone')}, cpp-demangle {v('cpp-demangle')}, pyelftools {v('pyelftools')}"
    )


def header(img: Image) -> str:
    return f"== {img.platform}: {img.path.name} (sha256 {img.sha256[:16]}...)"


# -- formatting -----------------------------------------------------------------------------


def _f32(bits: int) -> str:
    v = struct.unpack("<f", struct.pack("<I", bits))[0]
    if math.isnan(v):
        return f"nan({bits:#010x})"
    for p in range(6, 10):
        s = f"{v:.{p}g}"
        if struct.pack("<f", float(s)) == struct.pack("<f", v):
            return s + "f"
    return repr(v)


def _f64(bits: int) -> str:
    v = struct.unpack("<d", struct.pack("<Q", bits))[0]
    return f"nan({bits:#018x})" if math.isnan(v) else repr(v)


_INT_SIMD = re.compile(r"^v?p(and|andn|or|xor|add|sub|cmp|min|max|shuf|broadcast)|^v?movdq[au]")


def constant(img: Image, insn: Insn, addr: int) -> str:
    """The value a RIP-relative operand reads from read-only data, by the instruction's type."""
    sec = img.section_of(addr)
    if sec is None or sec.name not in (".rdata", ".rodata", "_RDATA"):
        return ""
    m = insn.mnemonic
    b = img.read(addr, 16)
    if len(b) < 16:
        return ""
    if m.endswith("ss") or m == "vbroadcastss":
        return "= " + _f32(struct.unpack_from("<I", b)[0])
    if m.endswith("sd") or m == "vbroadcastsd":
        return "= " + _f64(struct.unpack_from("<Q", b)[0])
    if m.endswith("ps"):
        return "= {" + ", ".join(_f32(x) for x in struct.unpack("<4I", b)) + "}"
    if m.endswith("pd"):
        return "= {" + ", ".join(_f64(x) for x in struct.unpack("<2Q", b)) + "}"
    if _INT_SIMD.search(m):
        return "= {" + ", ".join(f"{x:#x}" for x in struct.unpack("<4I", b)) + "}"
    return ""


def annotation(img: Image, insn: Insn, f: Function | None) -> str:
    target, imp = img.resolve_call(insn)
    if imp is not None:
        return f"{imp.display} ({imp.library})" if imp.library else imp.display
    if target is not None:
        if f is not None and (
            f.start <= target < f.end or any(a <= target < b for a, b in f.fragments)
        ):
            return ""
        return img.name_of(target) or ""
    addr = rip_target(insn)
    if addr is None:
        return ""
    if addr in img.slots:
        return f"{addr:#x} &{img.slots[addr].display}"
    parts = [f"{addr:#x}", img.name_of(addr) or "", constant(img, insn, addr)]
    return " ".join(p for p in parts if p)


def ranges(f: Function) -> list[tuple[int, int]]:
    """The function's address ranges, adjacent ones merged."""
    out: list[tuple[int, int]] = []
    for a, b in sorted([(f.start, f.end), *f.fragments]):
        if out and out[-1][1] == a:
            out[-1] = (out[-1][0], b)
        else:
            out.append((a, b))
    return out


def describe(img: Image, f: Function) -> str:
    rs = ranges(f)
    where = " ".join(f"{a:#x}-{b:#x}" for a, b in rs)
    how = {
        "symbol": "symbol",
        "pdata": ".pdata",
        "leaf": "no unwind data; ends at int3 padding",
    }.get(f.source, f.source)
    why = getattr(img, "known_names", {}).get(f.start)
    if why:
        how += "; named by known.py"
    if f.fragments:
        how += f", {len(f.fragments)} chained entries"
    slots = getattr(img, "slot_of", {}).get(f.start)
    if slots:
        s = slots[0]
        how += f"; vtable {s.vtable:#x} of {s.cls}, slot {s.slot} (Linux slot {s.linux_slot}"
        how += (
            ")"
            if names.canonical(s.linux_class) == names.canonical(s.cls)
            else f" of {s.linux_class})"
        )
    return f"{where}  {f.name}  [{how}]"


def print_function(img: Image, f: Function, args, mark: int | None = None) -> None:
    print(describe(img, f))
    for extra in f.names[1:4]:
        print(f"    also: {extra}")
    if len(f.names) > 4:
        print(f"    also: ... {len(f.names) - 4} more names (identical code folded by the linker)")
    for a, b in ranges(f):
        insns = img.disasm(a, b, att=args.att)
        if mark is not None and args.around is not None:
            k = next((i for i, x in enumerate(insns) if x.address >= mark), 0)
            insns = insns[max(0, k - args.around) : k + args.around + 1]
        for insn in insns:
            prefix = ">" if insn.address == mark else " "
            raw = f"{img.read(insn.address, insn.size).hex():<24}" if args.bytes else ""
            text = f"{prefix} {insn.address:x}:  {raw}{insn.mnemonic:<8} {insn.op_str}".rstrip()
            ann = annotation(img, insn, f)
            print(f"{text:<64}; {ann}" if ann else text)


# -- commands -------------------------------------------------------------------------------


def cmd_locate(args) -> int:
    print(f"oracle/uv.lock pins {wheels.PACKAGE}=={wheels.VERSION}; tools: {versions()}")
    for platform in PLATFORMS:
        w = wheels.locate(platform, log)
        print(f"{platform}: {w.locked.filename}")
        print(f"  wheel    {w.wheel}")
        print(f"           sha256 {w.locked.sha256}, {w.locked.size} bytes: matches oracle/uv.lock")
        for kind, m in (("library", w.library), ("module", w.module)):
            print(f"  {kind:<8} {m.path}")
            print(f"           sha256 {m.sha256}, {m.size} bytes: matches the wheel's RECORD")
            if m.installed is not None:
                same = "identical" if m.installed_identical else "DIFFERENT from the pinned wheel"
                print(f"           oracle environment copy {m.installed}: {same}")
    if args.names:
        img = image("windows")
        n = img.naming
        print(
            f"windows names: {n['exports']} exports, {n['classes_named']} RTTI classes matched to "
            f"Linux vtables ({n['slots']} slots; vtables that did not line up: {n['classes_unmatched']}), "
            f"{n['known']} from known.py"
        )
    return 0


def find(img: Image, query: str) -> list[Function]:
    return [f for f in img.functions if any(names.matches(query, n) for n in f.names)]


def cmd_find(args) -> int:
    status = 1
    for platform in selected(args):
        img = image(platform)
        print(header(img))
        found = find(img, args.name)
        for f in found[: args.limit]:
            print("  " + describe(img, f))
            for extra in f.names[1:3]:
                print(f"      also: {extra}")
        if len(found) > args.limit:
            print(f"  ... {len(found) - args.limit} more")
        if found:
            status = 0
            continue
        needle = args.name.lower()
        near = [f for f in img.functions if any(needle in n.lower() for n in f.names)]
        if near:
            print(f"  no function is named {args.name}; names containing it:")
            for f in near[: args.limit]:
                print("    " + describe(img, f))
        else:
            print(f"  no function is named {args.name}")
        if platform == "windows":
            candidates(img, args)
    return status


def candidates(win: Image, args) -> None:
    """For a function the DLL does not name: Windows functions that call the same imports as
    the Linux function (float and double libm functions counted as one)."""
    linux = image("linux")
    for lf in find(linux, args.name)[:3]:
        want, hits = names.same_imports(win, linux, lf)
        if not want:
            print(f"  the Linux {lf.name} calls no import, so there is no fingerprint to compare")
            continue
        print(
            f"  Windows functions that call the same imports as the Linux function ({' '.join(want)}):"
        )
        for f in hits[: args.limit]:
            print("    " + describe(win, f))
        if not hits:
            print("    none")


def cmd_disasm(args) -> int:
    target = args.target
    address = int(target, 16) if re.fullmatch(r"0x[0-9a-fA-F]+", target) else None
    status = 0
    printed = False
    for platform in selected(args):
        img = image(platform)
        if address is not None:
            if img.section_of(address) is None:
                continue
            f = img.function_at(address)
            print(header(img) + f"  [{versions()}]")
            if f is None:
                print(f"  {address:#x} is in no known function; decoding 64 bytes from it")
                f = Function(address, address + 64, source="raw bytes")
            print_function(img, f, args, mark=address)
            printed = True
            continue
        found = find(img, target)
        print(header(img) + f"  [{versions()}]")
        if len(found) == 1 or (found and args.all):
            for f in found:
                print_function(img, f, args)
        elif not found:
            print(f"  no function is named {target}; try `wheel-inspect find`")
            status = 1
        else:
            print(f"  {len(found)} functions match; use a longer name, an address, or --all:")
            for f in found[:20]:
                print("    " + describe(img, f))
            status = 1
        printed = True
    if not printed:
        print(f"{target} is in neither library")
        status = 1
    return status


def cmd_imports(args) -> int:
    for platform in selected(args):
        img = image(platform)
        print(header(img))
        if args.grep:
            rx = re.compile(args.grep)
            chosen = [i for i in img.imports if rx.search(i.name)]
        elif args.all:
            chosen = list(img.imports)
        else:
            chosen = [
                i
                for i in img.imports
                if i.library.startswith("api-ms-win-crt-math")
                or i.library == "libm.so.6"
                or i.stub_label == "SVML"
            ]
            print("  math library and SVML (--all for every import):")
        for imp in sorted(chosen, key=lambda i: (i.library, i.name)):
            s = img.call_sites().get(id(imp), [])
            funcs: dict[int, list[tuple[int, str]]] = {}
            for a, kind in s:
                f = img.function_at(a)
                funcs.setdefault(f.start if f else a, []).append((a, kind))
            stubs = " ".join(f"{a:#x}" for a in imp.stubs)
            if imp.slot:
                label = "PLT" if platform == "linux" else "thunk"
                where = f"slot {imp.slot:#x}" + (f", {label} {stubs}" if stubs else "")
            else:
                where = f"at {stubs}"
            print(
                f"  {imp.display:<28} {imp.library:<32} {where}: {len(s)} sites in {len(funcs)} functions"
            )
            if args.grep:
                for start in sorted(funcs):
                    f = img.function_at(start)
                    label = f.name if f else f"{start:#x}"
                    at = " ".join(
                        f"{a:x}" + ("" if k.startswith("call") else f"({k})")
                        for a, k in funcs[start]
                    )
                    print(f"      {start:#x}  {label}")
                    print(f"          {len(funcs[start])}: {at}")
    return 0


def cmd_scan(args) -> int:
    rx = re.compile(args.regex)
    for platform in selected(args):
        img = image(platform)
        print(header(img))
        per: dict = {}
        total = 0
        for insn in img.sweep():
            if rx.fullmatch(insn.mnemonic):
                f = img.function_at(insn.address)
                per.setdefault(f.start if f else None, []).append(insn)
                total += 1
        print(f"  {total} instructions match {args.regex!r}, in {len(per)} functions")
        for start in sorted(per, key=lambda s: -1 if s is None else s):
            f = img.function_at(start) if start is not None else None
            name = f.name if f else "(outside any known function)"
            print(f"  {len(per[start]):6}  {start or 0:#x}  {name}")
            if args.list:
                for insn in per[start]:
                    print(f"            {insn.address:x}:  {insn.mnemonic:<8} {insn.op_str}")
    return 0


def cmd_selftest(args) -> int:
    from . import selftest

    return selftest.run(image, selected(args))


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(
        prog="wheel-inspect",
        description="Read the machine code of the pinned opencolorio==2.5.2 wheel (docs/wheel-inspect.md).",
    )
    sub = parser.add_subparsers(dest="cmd", required=True)

    def platform_flags(p):
        g = p.add_mutually_exclusive_group()
        g.add_argument("--win", action="store_true", help="only the Windows DLL")
        g.add_argument("--linux", action="store_true", help="only the Linux .so")

    p = sub.add_parser(
        "locate", help="the pinned wheels and their libraries, verified against oracle/uv.lock"
    )
    p.add_argument("--names", action="store_true", help="also count the names the Windows DLL gets")
    p = sub.add_parser("find", help="find a function by name in both builds")
    p.add_argument(
        "name",
        help="a C++ name or a ::-suffix of one: GetLogSideBreak, CameraLog2LinRenderer::apply",
    )
    p.add_argument("--limit", type=int, default=20)
    platform_flags(p)
    p = sub.add_parser(
        "disasm", help="disassemble a function (by name, or the one containing an address)"
    )
    p.add_argument("target", help="a function name, or an address such as 0x180210d18")
    p.add_argument("--att", action="store_true", help="AT&T syntax, as objdump prints by default")
    p.add_argument("--bytes", action="store_true", help="show the instruction bytes")
    p.add_argument(
        "--around", type=int, metavar="N", help="with an address: only N instructions either side"
    )
    p.add_argument("--all", action="store_true", help="print every function that matches")
    platform_flags(p)
    p = sub.add_parser("imports", help="math library and SVML imports, and where they are called")
    p.add_argument(
        "--grep", metavar="REGEX", help="imports whose name matches, with their call sites"
    )
    p.add_argument("--all", action="store_true", help="every import, not only the math library")
    platform_flags(p)
    p = sub.add_parser("scan", help="count instructions by mnemonic, per function (FMA, F16C, ...)")
    p.add_argument(
        "regex", help=r"a regular expression for the whole mnemonic, e.g. 'vfn?m(add|sub)\d+[ps]s'"
    )
    p.add_argument("--list", action="store_true", help="print each instruction")
    platform_flags(p)
    p = sub.add_parser("selftest", help="reproduce the spikes' findings from the wheels")
    platform_flags(p)

    args = parser.parse_args(argv)
    commands = {
        "locate": cmd_locate,
        "find": cmd_find,
        "disasm": cmd_disasm,
        "imports": cmd_imports,
        "scan": cmd_scan,
        "selftest": cmd_selftest,
    }
    try:
        return commands[args.cmd](args)
    except wheels.WheelError as e:
        print(f"wheel-inspect: {e}", file=sys.stderr)
        return 2
    except BrokenPipeError:
        return 0


if __name__ == "__main__":
    sys.exit(main())
