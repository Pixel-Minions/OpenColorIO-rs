# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""`wheel-inspect selftest`: reproduce the spikes' findings from the pinned wheels' machine
code. Any mismatch fails. The expectations are the spikes' reports, quoted with their source.

Both libraries come from the pinned wheels on every platform, so the whole test runs on
Windows and in Rocky Linux 9 alike; `--win` or `--linux` keeps one library's checks.
"""

from __future__ import annotations

import re
from collections import Counter

from . import known, names, wheels
from .image import rip_target

# Where two NaNs can meet, the operand order each wheel compiled (docs/spikes/s2-s5.md, "Two
# NaN operands"): (function, upstream source, {platform: (cited address, instructions from it)}).
OPERAND_ORDER = [
    (
        "CameraLog2LinRenderer::apply",
        "LogOpCPU.cpp:787 `m_linsinv[i] * (in[i] + m_minuslino[i])`; both: (in + minuslino) * linsinv",
        {
            "windows": (
                0x180210D18,
                ["addss xmm1, dword ptr [rbx + 0xbc]", "mulss xmm1, dword ptr [rbx + 0xb0]"],
            ),
            "linux": (
                0x3F26C0,
                ["addss xmm0, dword ptr [rbx + 0xbc]", "mulss xmm0, dword ptr [rbx + 0xb0]"],
            ),
        },
    ),
    (
        "LogUtil::GetLinearOffset",
        "LogUtils.cpp:285 `linearSlope * linBreak`; both: linBreak * linearSlope",
        {"windows": (0x18021AB8C, ["mulss xmm0, xmm1"]), "linux": (0x40185F, ["mulss xmm0, xmm2"])},
    ),
    (
        "LogUtil::GetLinearSlope",
        "LogUtils.cpp:264-266 `(...) * log(base)`; both: log(base) * (...)",
        {
            "windows": (0x18021ABF1, ["mulsd xmm0, xmm6"]),
            "linux": (0x40178B, ["mulsd xmm0, qword ptr [rsp]"]),
        },
    ),
    (
        "GammaMoncurveMirrorOpCPUFwd::apply",
        "GammaOpCPU.cpp:728-731 `pixel * scale`; both: scale * pixel",
        {
            "windows": (0x1801BDEB4, ["mulss xmm0, xmm11"]),
            "linux": (0x384EA5, ["mulss xmm0, xmm10"]),
        },
    ),
]

# LogUtil::GetLogSideBreak (LogUtils.cpp:270-281): its scalar arithmetic and calls, in order
# (s2-s5.md, "Windows and Linux differences" 1). Linux: log2 in double, `lsb *= q` in double
# as q * lsb, then `lsb += logOffset` in float as logOffset + lsb. Windows: all float.
LOG_SIDE_BREAK = {
    "linux": (
        0x4017B0,
        [
            "mulsd xmm1, qword ptr [rbx + 0x20]",
            "addsd xmm0, xmm1",
            "call log2@GLIBC_2.2.5",
            "call log2@GLIBC_2.2.5",
            "divsd xmm1, xmm0",
            "mulsd xmm1, xmm0",
            "addss xmm0, xmm1",
        ],
    ),
    "windows": (
        0x18021AC60,
        [
            "mulsd xmm0, qword ptr [rbx + 0x10]",
            "addsd xmm0, qword ptr [rbx + 0x18]",
            "call log2f",
            "call log2f",
            "divss xmm6, xmm0",
            "mulss xmm7, xmm6",
            "addss xmm7, xmm1",
        ],
    ),
}

FMA = re.compile(r"vfn?m(add|sub)(132|213|231)(ps|ss)")  # the spikes' pattern (s4.md)
FMA_ANY = re.compile(r"vf(n?m(add|sub)|maddsub|msubadd)(132|213|231)(ps|ss|pd|sd)")
F16C = re.compile(r"vcvt(ph2ps|ps2ph)")


class Report:
    def __init__(self) -> None:
        self.failed = 0
        self.passed = 0

    def section(self, title: str) -> None:
        print(f"\n{title}")

    def check(self, ok: bool, what: str, got: object = None) -> None:
        print(f"  {'PASS' if ok else 'FAIL'}  {what}")
        if not ok and got is not None:
            print(f"        got: {got}")
        self.failed += not ok
        self.passed += bool(ok)


def text(insn) -> str:
    return f"{insn.mnemonic} {insn.op_str}".strip()


def skeleton(img, f) -> list[str]:
    """A function's scalar float arithmetic and calls, in address order."""
    out = []
    for insn in img.function_insns(f):
        if re.fullmatch(r"(add|sub|mul|div)(ss|sd)", insn.mnemonic):
            out.append(text(insn))
        elif insn.mnemonic == "call":
            _, imp = img.resolve_call(insn)
            out.append(f"call {imp.display if imp else insn.op_str}")
    return out


def run(image, platforms: list[str]) -> int:
    from .cli import find

    r = Report()
    imgs = {p: image(p) for p in platforms}

    r.section("The pinned wheels (oracle/uv.lock)")
    for p in platforms:
        w = wheels.locate(p)
        r.check(
            True,
            f"{p}: {w.locked.filename}: {w.library.path.name} verified (sha256 {w.library.sha256[:16]}...)",
        )
        if w.library.installed is not None:
            r.check(
                bool(w.library.installed_identical),
                f"{p}: the oracle environment's copy is the same file",
            )
        else:
            print(f"  skip  {p}: this machine's target directory has no oracle environment for it")
    if "windows" in imgs:
        r.check(
            imgs["windows"].sha256 == known.WINDOWS_SHA256, "windows: known.py describes this DLL"
        )

    r.section("Operand order where two NaNs can meet (s2-s5.md, `Two NaN operands`)")
    for name, source, sites in OPERAND_ORDER:
        print(f"  {name}: {source}")
        for p, img in imgs.items():
            addr, want = sites[p]
            found = find(img, name)
            f = img.function_at(addr)
            r.check(
                len(found) == 1 and f is found[0],
                f"{p}: `find {name}` gives the one function holding {addr:#x}",
                [hex(g.start) for g in found],
            )
            got = [text(i) for i in img.disasm(addr, addr + 16 * len(want))[: len(want)]]
            r.check(got == want, f"{p}: {addr:#x}: {'; '.join(want)}", got)

    r.section(
        "GetLogSideBreak: float on Windows, double on Linux (s2-s5.md, `Windows and Linux differences` 1)"
    )
    for p, img in imgs.items():
        addr, want = LOG_SIDE_BREAK[p]
        found = find(img, "LogUtil::GetLogSideBreak")
        ok = len(found) == 1 and found[0].start == addr
        r.check(
            ok, f"{p}: GetLogSideBreak is the function at {addr:#x}", [hex(g.start) for g in found]
        )
        if ok:
            got = skeleton(img, found[0])
            r.check(got == want, f"{p}: arithmetic and calls: {'; '.join(want)}", got)
    if set(imgs) == {"windows", "linux"}:
        lf = find(imgs["linux"], "LogUtil::GetLogSideBreak")[0]
        want, found = names.same_imports(imgs["windows"], imgs["linux"], lf)
        hits = [f.start for f in found]
        r.check(
            hits == [LOG_SIDE_BREAK["windows"][0]],
            f"windows: the only function calling what the Linux one calls ({' '.join(want)}) is {LOG_SIDE_BREAK['windows'][0]:#x}",
            [hex(h) for h in hits],
        )

    r.section("FMA and F16C instructions (s4.md, `Disassembly of both wheels`)")
    for p, img in imgs.items():
        counts: Counter = Counter()
        where: Counter = Counter()
        for insn in img.sweep():
            for label, rx in (("fma", FMA), ("fma-any", FMA_ANY), ("f16c", F16C)):
                if rx.fullmatch(insn.mnemonic):
                    counts[label] += 1
                    f = img.function_at(insn.address)
                    fn = names.strip_params(f.name) if f else ""
                    kernel = (
                        "linear1D"
                        if "::linear1D<" in fn
                        else "tetrahedral"
                        if "applyTetrahedralAVX" in fn
                        else "other"
                    )
                    where[(label, kernel)] += 1
        r.check(counts["fma"] == 108, f"{p}: 108 FMA instructions ({FMA.pattern})", counts["fma"])
        r.check(
            counts["fma-any"] == 108,
            f"{p}: no other FMA form, none in double precision",
            counts["fma-any"],
        )
        r.check(counts["f16c"] == 24, f"{p}: 24 F16C instructions ({F16C.pattern})", counts["f16c"])
        if p == "linux":
            r.check(
                (where[("fma", "linear1D")], where[("fma", "tetrahedral")]) == (72, 36),
                "linux: 72 FMA in the Lut1D kernels (linear1D), 36 in applyTetrahedralAVX2Func/AVX512Func",
                dict(where),
            )
            r.check(
                where[("f16c", "linear1D")] == 24,
                "linux: all 24 F16C in the Lut1D kernels",
                dict(where),
            )

    r.section("Math library and SVML (s2-s5.md, S5 and `Windows and Linux differences` 3)")
    if "windows" in imgs:
        win = imgs["windows"]
        entry = [text(i) for i in win.disasm(0x18034B0B0, 0x18034B0B5)]
        r.check(
            entry == ["jmp 0x18034ba20"],
            "windows: __vdecl_powf4 (0x18034b0b0) is `jmp __sse2_powf4` (0x18034ba20)",
            entry,
        )
        svml = next(i for i in win.imports if i.name == "__vdecl_powf4")
        per = Counter(win.function_at(a).name for a, _ in win.call_sites().get(id(svml), []))
        want = {
            "OpenColorIO_v2_5::Renderer_LIN_TO_PQ_SSE<0>::apply(void const*, void*, long) const": 10,
            "OpenColorIO_v2_5::Renderer_PQ_TO_LIN_SSE<0>::apply(void const*, void*, long) const": 10,
        }
        r.check(
            dict(per) == want,
            "windows: 20 calls to it, 10 in each PQ SSE renderer without fast power",
            dict(per),
        )
        rdata = next(s for s in win.sections if s.name == "_RDATA")
        readers = set()
        for insn in win.sweep():
            t = rip_target(insn)
            if t is not None and rdata.va <= t < rdata.va + rdata.size:
                readers.add(win.function_at(insn.address).start)
        r.check(
            readers == {0x18034BA20},
            "windows: __sse2_powf4 is the only function reading _RDATA",
            [hex(a) for a in readers],
        )
    if "linux" in imgs:
        lin = imgs["linux"]
        versions = {i.name: (i.version, i.library) for i in lin.imports}
        want_v = {n: ("GLIBC_2.2.5", "libm.so.6") for n in ("log2", "log", "pow")}
        want_v |= {
            n: ("GLIBC_2.27", "libm.so.6") for n in ("log2f", "exp2f", "powf", "logf", "expf")
        }
        got_v = {n: versions.get(n) for n in want_v}
        r.check(
            got_v == want_v,
            "linux: log2, log, pow @GLIBC_2.2.5; log2f, exp2f, powf, logf, expf @GLIBC_2.27 (libm.so.6)",
            got_v,
        )
        powf = next(i for i in lin.imports if i.name == "powf")
        per = Counter(lin.function_at(a).name for a, _ in lin.call_sites().get(id(powf), []))
        pq = {n: c for n, c in per.items() if "PQ" in n}
        want = {
            "OpenColorIO_v2_5::Renderer_LIN_TO_PQ<float>::apply(void const*, void*, long) const": 2,
            "OpenColorIO_v2_5::Renderer_PQ_TO_LIN<float>::apply(void const*, void*, long) const": 2,
        }
        r.check(pq == want, "linux: PQ without fast power is the scalar renderer, calling powf", pq)

    if "windows" in imgs:
        r.section("known.py")
        win = imgs["windows"]
        for va, name, kind, _ in known.WINDOWS:
            f = win.function_at(va)
            r.check(
                f is not None and f.start == va and f.name == name,
                f"windows: {va:#x} is {name.split('(')[0]} ({kind})",
            )

    print(f"\n{r.passed} passed, {r.failed} failed")
    return 1 if r.failed else 0
