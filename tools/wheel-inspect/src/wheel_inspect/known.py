# SPDX-License-Identifier: BSD-3-Clause
# Copyright Contributors to the OpenColorIO Project.
"""Names for code in the pinned Windows DLL that its exports and RTTI do not give.

The DLL has no symbols, so these were identified by hand; each entry says how. The addresses
hold only for the DLL whose SHA-256 is `WINDOWS_SHA256` (the one `oracle/uv.lock` pins); the
tool ignores this table for any other file. `wheel-inspect selftest` checks every entry.

To add one: identify the function (usually from the Linux build's symbol, the imports it
calls and its arithmetic), give its start address, and say how
you know. Kinds: "function", "svml-entry" (an SVML entry point; `imports` lists its call
sites), "svml-kernel".
"""

WINDOWS_SHA256 = "f75646b44d33f947e5fc08ddf2fe524f3dd70817c6864dae74c4e0e4f40cd192"

# (address, name, kind, how it was identified)
WINDOWS = [
    # LogUtils.cpp's three helpers are adjacent in the DLL. The camera log renderers call them
    # (LogOpCPU.cpp:731-739); they are not virtual and not exported.
    (
        0x18021AB80,
        "OpenColorIO_v2_5::LogUtil::GetLinearOffset(std::vector<double, std::allocator<double> > const&, float, float)",
        "function",
        "A leaf just before GetLinearSlope computing `logSideBreak - (float)params[4] * "
        "linearSlope` (LogUtils.cpp:283-286); docs/spikes/s2-s5.md cites 0x18021ab8c in it.",
    ),
    (
        0x18021ABA0,
        "OpenColorIO_v2_5::LogUtil::GetLinearSlope(std::vector<double, std::allocator<double> > const&, double)",
        "function",
        "One of 4 functions that call `log` once, as the Linux one does (`find GetLinearSlope`); "
        "the only one returning (float)params[5] when params.size() > 5 (LogUtils.cpp:255-268).",
    ),
    (
        0x18021AC60,
        "OpenColorIO_v2_5::LogUtil::GetLogSideBreak(std::vector<double, std::allocator<double> > const&, double)",
        "function",
        "The only function that calls log2f twice, as the Linux one calls log2 twice (`find "
        "GetLogSideBreak` before this entry); then divss, mulss, addss (LogUtils.cpp:270-281).",
    ),
    (
        0x18034B0B0,
        "__vdecl_powf4",
        "svml-entry",
        "What `_mm_pow_ps` compiles to (FixedFunctionOpCPU.cpp:2139, 2194; S5 in "
        "docs/spikes/s2-s5.md). In MSVC 14.44's msvcrt.lib (svml_spowf4_dispatch.obj) it is a "
        "single `jmp __sse2_powf4`, as here.",
    ),
    (
        0x18034BA20,
        "__sse2_powf4",
        "svml-kernel",
        "MSVC 14.44.35211's svml_spowf4_sse2.obj: a program linked with `cl /O2 /MD` has the "
        "same 594 instructions (addresses aside) and the same constants (the DLL's _RDATA "
        "section). It is the only function that reads _RDATA.",
    ),
]
