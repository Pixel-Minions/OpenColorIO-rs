// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! CPU feature detection: a port of `CPUInfo` (src/OpenColorIO/CPUInfo.cpp, CPUInfo.h and
//! CPUInfoConfig.h.in @ v2.5.2).
//!
//! OCIO picks its SIMD kernels at runtime from these flags. Each `has*()` answer combines a
//! runtime CPU flag with the build's compile-time `OCIO_USE_*` switch ([`BuildConfig`]), exactly
//! as the `x86_check_flags` macro does (CPUInfo.h:36-37 @ v2.5.2).
//!
//! **The build being matched.** The official x86-64 wheels are built with every `OCIO_USE_*`
//! switch on ([`BuildConfig::X86_64_WHEEL`]). Evidence: the wheels' `ociocpuinfo` app, which
//! compiles CPUInfo.cpp against the library's generated `CPUInfoConfig.h`, prints `1` for every
//! `has*()` line on a CPU with all the features (spike S4, `docs/spikes/s4.md`).
//!
//! **No EPYC 9V45 quirk.** Upstream `main` blocks AVX-512 on AMD EPYC 9V45 (commit `c2bd98f7`,
//! PR #2324, July 2026). That change is not in v2.5.2, and the 2.5.2 wheels' binaries contain no
//! "EPYC" string, so this port does not block it either.
//!
//! `unsafe` is allowed in this module only for the `cpuid` and `xgetbv` instructions.
#![allow(unsafe_code)]

use std::borrow::Cow;
use std::sync::OnceLock;

/// SSE2 functions. Port of `X86_CPU_FLAG_SSE2` (src/OpenColorIO/CPUInfo.h:14 @ v2.5.2).
pub const X86_CPU_FLAG_SSE2: u32 = 1 << 0;
/// SSE2 supported but usually not faster than MMX/SSE. Port of `X86_CPU_FLAG_SSE2_SLOW`
/// (src/OpenColorIO/CPUInfo.h:15 @ v2.5.2).
pub const X86_CPU_FLAG_SSE2_SLOW: u32 = 1 << 1;
/// Prescott SSE3 functions. Port of `X86_CPU_FLAG_SSE3` (src/OpenColorIO/CPUInfo.h:17 @ v2.5.2).
pub const X86_CPU_FLAG_SSE3: u32 = 1 << 2;
/// SSE3 supported but usually not faster. Port of `X86_CPU_FLAG_SSE3_SLOW`
/// (src/OpenColorIO/CPUInfo.h:18 @ v2.5.2).
pub const X86_CPU_FLAG_SSE3_SLOW: u32 = 1 << 3;
/// Conroe SSSE3 functions. Port of `X86_CPU_FLAG_SSSE3` (src/OpenColorIO/CPUInfo.h:20 @ v2.5.2).
pub const X86_CPU_FLAG_SSSE3: u32 = 1 << 4;
/// SSSE3 supported but usually not faster than SSE2. Port of `X86_CPU_FLAG_SSSE3_SLOW`
/// (src/OpenColorIO/CPUInfo.h:21 @ v2.5.2).
pub const X86_CPU_FLAG_SSSE3_SLOW: u32 = 1 << 5;
/// Penryn SSE4.1 functions. Port of `X86_CPU_FLAG_SSE4` (src/OpenColorIO/CPUInfo.h:23 @ v2.5.2).
pub const X86_CPU_FLAG_SSE4: u32 = 1 << 6;
/// Nehalem SSE4.2 functions. Port of `X86_CPU_FLAG_SSE42` (src/OpenColorIO/CPUInfo.h:24 @ v2.5.2).
pub const X86_CPU_FLAG_SSE42: u32 = 1 << 7;
/// AVX functions, with OS support. Port of `X86_CPU_FLAG_AVX` (src/OpenColorIO/CPUInfo.h:26 @ v2.5.2).
pub const X86_CPU_FLAG_AVX: u32 = 1 << 8;
/// AVX supported but slow with YMM registers (Bulldozer, Jaguar). Port of `X86_CPU_FLAG_AVX_SLOW`
/// (src/OpenColorIO/CPUInfo.h:27 @ v2.5.2).
pub const X86_CPU_FLAG_AVX_SLOW: u32 = 1 << 9;
/// AVX2 functions, with OS support. Port of `X86_CPU_FLAG_AVX2`
/// (src/OpenColorIO/CPUInfo.h:29 @ v2.5.2).
pub const X86_CPU_FLAG_AVX2: u32 = 1 << 10;
/// The CPU has slow gathers. Port of `X86_CPU_FLAG_AVX2_SLOWGATHER`
/// (src/OpenColorIO/CPUInfo.h:30 @ v2.5.2).
pub const X86_CPU_FLAG_AVX2_SLOWGATHER: u32 = 1 << 11;
/// AVX-512 (F, DQ, CD, BW and VL), with OS support. Port of `X86_CPU_FLAG_AVX512`
/// (src/OpenColorIO/CPUInfo.h:32 @ v2.5.2).
pub const X86_CPU_FLAG_AVX512: u32 = 1 << 12;
/// F16C half-float conversions. Port of `X86_CPU_FLAG_F16C` (src/OpenColorIO/CPUInfo.h:34 @ v2.5.2).
pub const X86_CPU_FLAG_F16C: u32 = 1 << 13;

/// The compile-time `OCIO_USE_*` switches of an OCIO build. Port of the switches in
/// `CPUInfoConfig.h.in` (src/OpenColorIO/CPUInfoConfig.h.in:27-53 @ v2.5.2), which CMake sets
/// from `CMakeLists.txt:180-275` and `share/cmake/utils/CheckSupportX86SIMD.cmake` @ v2.5.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildConfig {
    /// `OCIO_USE_SSE2`
    pub use_sse2: bool,
    /// `OCIO_USE_SSE3`
    pub use_sse3: bool,
    /// `OCIO_USE_SSSE3`
    pub use_ssse3: bool,
    /// `OCIO_USE_SSE4`
    pub use_sse4: bool,
    /// `OCIO_USE_SSE42`
    pub use_sse42: bool,
    /// `OCIO_USE_AVX`
    pub use_avx: bool,
    /// `OCIO_USE_AVX2`
    pub use_avx2: bool,
    /// `OCIO_USE_AVX512`
    pub use_avx512: bool,
    /// `OCIO_USE_F16C`
    pub use_f16c: bool,
}

impl BuildConfig {
    /// The official x86-64 wheels (Windows and manylinux): every switch on.
    pub const X86_64_WHEEL: BuildConfig = BuildConfig {
        use_sse2: true,
        use_sse3: true,
        use_ssse3: true,
        use_sse4: true,
        use_sse42: true,
        use_avx: true,
        use_avx2: true,
        use_avx512: true,
        use_f16c: true,
    };

    /// A build without SIMD (`OCIO_USE_SIMD=OFF`, or an architecture without SSE support).
    pub const NO_SIMD: BuildConfig = BuildConfig {
        use_sse2: false,
        use_sse3: false,
        use_ssse3: false,
        use_sse4: false,
        use_sse42: false,
        use_avx: false,
        use_avx2: false,
        use_avx512: false,
        use_f16c: false,
    };

    /// The build this port matches on the current target. Only x86-64 is a reference platform
    /// (PLAN.md D11); other architectures get [`BuildConfig::NO_SIMD`] until they are verified.
    ///
    /// That is right for the Linux aarch64 wheel as CMake configures it (`aarch64` matches
    /// neither `ARM64|arm64` nor an x86 name, so SIMD is off), but not for the macOS arm64 wheel,
    /// which turns `OCIO_USE_SSE2` on through SSE2NEON. The SSE2NEON variants of the SSE2 code are
    /// not ported.
    pub const CURRENT: BuildConfig = if cfg!(target_arch = "x86_64") {
        BuildConfig::X86_64_WHEEL
    } else {
        BuildConfig::NO_SIMD
    };
}

/// What OCIO knows about the CPU. Port of `struct CPUInfo` (src/OpenColorIO/CPUInfo.h:39-76 @ v2.5.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuInfo {
    /// `X86_CPU_FLAG_*` bits.
    pub flags: u32,
    /// CPU family, as CPUInfo.cpp:91 computes it.
    pub family: i32,
    /// CPU model, as CPUInfo.cpp:92 computes it.
    pub model: i32,
    name: [u8; 65],
    vendor: [u8; 13],
    build: BuildConfig,
}

impl CpuInfo {
    /// The detected CPU, for the build this port matches. Port of `CPUInfo::instance`
    /// (src/OpenColorIO/CPUInfo.cpp:226-230 @ v2.5.2).
    pub fn instance() -> &'static CpuInfo {
        static INSTANCE: OnceLock<CpuInfo> = OnceLock::new();
        INSTANCE.get_or_init(|| CpuInfo::detect(BuildConfig::CURRENT))
    }

    /// Detects this machine's CPU, for a build with the switches `build`.
    pub fn detect(build: BuildConfig) -> CpuInfo {
        detect(build)
    }

    /// A copy with `flags` replaced, the way upstream's test runner forces a SIMD mode
    /// (`cpu.flags = flags`, tests/cpu/UnitTestMain.cpp:104-153 @ v2.5.2).
    ///
    /// Test-only (`docs/architecture.md`, "Forced profiles"): renderers built from a forced
    /// `CpuInfo` run a numeric profile this machine would not pick. It is public, but hidden, only
    /// so the crate's integration tests can reach it; nothing else may call it.
    #[doc(hidden)]
    pub fn with_flags(&self, flags: u32) -> CpuInfo {
        CpuInfo {
            flags,
            ..self.clone()
        }
    }

    /// A copy for a build with the switches `build`.
    ///
    /// Test-only, like [`CpuInfo::with_flags`]: it selects the code paths of another build, such
    /// as the trilinear renderer of a build without SSE2.
    #[doc(hidden)]
    pub fn with_build(&self, build: BuildConfig) -> CpuInfo {
        CpuInfo {
            build,
            ..self.clone()
        }
    }

    /// The build switches this CPU info answers for.
    pub fn build(&self) -> BuildConfig {
        self.build
    }

    /// Decodes x86 CPUID results the way OCIO does. Port of `CPUInfo::CPUInfo`
    /// (src/OpenColorIO/CPUInfo.cpp:72-184 @ v2.5.2), for x86-64.
    ///
    /// `cpuid(leaf)` returns `[eax, ebx, ecx, edx]` for sub-leaf 0. `xgetbv()` returns XCR0; it
    /// is called only where upstream calls it (CPUID reports OSXSAVE and AVX).
    pub fn from_cpuid(
        mut cpuid: impl FnMut(u32) -> [u32; 4],
        mut xgetbv: impl FnMut() -> u64,
        build: BuildConfig,
    ) -> CpuInfo {
        const EAX: usize = 0;
        const EBX: usize = 1;
        const ECX: usize = 2;
        const EDX: usize = 3;

        let mut flags = 0u32;
        let mut family = 0i32;
        let mut model = 0i32;
        let mut name = [0u8; 65];
        let mut vendor = [0u8; 13];

        let info = cpuid(0);
        let max_std_level = info[EAX];
        vendor[0..4].copy_from_slice(&info[EBX].to_le_bytes());
        vendor[4..8].copy_from_slice(&info[EDX].to_le_bytes());
        vendor[8..12].copy_from_slice(&info[ECX].to_le_bytes());

        let mut xcr: i64 = 0;

        if max_std_level >= 1 {
            let info = cpuid(1);
            family = (((info[EAX] >> 8) & 0xf) + ((info[EAX] >> 20) & 0xff)) as i32;
            model = (((info[EAX] >> 4) & 0xf) + ((info[EAX] >> 12) & 0xf0)) as i32;

            if info[EDX] & (1 << 26) != 0 {
                flags |= X86_CPU_FLAG_SSE2;
            }
            if info[ECX] & 1 != 0 {
                flags |= X86_CPU_FLAG_SSE3;
            }
            if info[ECX] & 0x0000_0200 != 0 {
                flags |= X86_CPU_FLAG_SSSE3;
            }
            if info[ECX] & 0x0008_0000 != 0 {
                flags |= X86_CPU_FLAG_SSE4;
            }
            if info[ECX] & 0x0010_0000 != 0 {
                flags |= X86_CPU_FLAG_SSE42;
            }

            // Check OSXSAVE and AVX bits.
            if info[ECX] & 0x1800_0000 == 0x1800_0000 {
                xcr = xgetbv() as i64;
                if xcr & 0x6 == 0x6 {
                    flags |= X86_CPU_FLAG_AVX;

                    if info[ECX] & 0x2000_0000 != 0 {
                        flags |= X86_CPU_FLAG_F16C;
                    }
                }
            }
        }

        if max_std_level >= 7 {
            let info = cpuid(7);

            if flags & X86_CPU_FLAG_AVX != 0 && info[EBX] & 0x0000_0020 != 0 {
                flags |= X86_CPU_FLAG_AVX2;
            }

            // OPMASK/ZMM state.
            if xcr & 0xe0 == 0xe0
                && flags & X86_CPU_FLAG_AVX2 != 0
                && info[EBX] & 0xd003_0000 == 0xd003_0000
            {
                flags |= X86_CPU_FLAG_AVX512;
            }
        }

        let max_ext_level = cpuid(0x8000_0000)[EAX];

        if max_ext_level >= 0x8000_0001 {
            let info = cpuid(0x8000_0001);
            // strncmp(vendor, "AuthenticAMD", 12)
            if &vendor[..12] == b"AuthenticAMD" {
                // Athlon64, some Opteron, and some Sempron processors.
                if flags & X86_CPU_FLAG_SSE2 != 0 && info[ECX] & 0x0000_0040 == 0 {
                    flags |= X86_CPU_FLAG_SSE2_SLOW;
                }

                // Bulldozer and Jaguar based CPUs.
                if (family == 0x15 || family == 0x16) && flags & X86_CPU_FLAG_AVX != 0 {
                    flags |= X86_CPU_FLAG_AVX_SLOW;
                }

                // Zen 3 and earlier have slow gather.
                if family <= 0x19 && flags & X86_CPU_FLAG_AVX2 != 0 {
                    flags |= X86_CPU_FLAG_AVX2_SLOWGATHER;
                }
            }
        }

        if &vendor[..12] == b"GenuineIntel" {
            if family == 6 && (model == 9 || model == 13 || model == 14) {
                if flags & X86_CPU_FLAG_SSE2 != 0 {
                    flags |= X86_CPU_FLAG_SSE2_SLOW;
                }
                if flags & X86_CPU_FLAG_SSE3 != 0 {
                    flags |= X86_CPU_FLAG_SSE3_SLOW;
                }
            }

            // Conroe has a slow shuffle unit.
            if flags & X86_CPU_FLAG_SSSE3 != 0
                && flags & X86_CPU_FLAG_SSE4 == 0
                && family == 6
                && model < 23
            {
                flags |= X86_CPU_FLAG_SSSE3_SLOW;
            }

            // Haswell has slow gather.
            if flags & X86_CPU_FLAG_AVX2 != 0 && family == 6 && model < 70 {
                flags |= X86_CPU_FLAG_AVX2_SLOWGATHER;
            }
        }

        // Get the CPU brand string.
        for index in 0..3u32 {
            let regs = cpuid(0x8000_0002 + index);
            for (reg, value) in regs.iter().enumerate() {
                let at = 16 * index as usize + 4 * reg;
                name[at..at + 4].copy_from_slice(&value.to_le_bytes());
            }
        }

        CpuInfo {
            flags,
            family,
            model,
            name,
            vendor,
            build,
        }
    }

    /// The brand string. Port of `CPUInfo::getName` (src/OpenColorIO/CPUInfo.h:51 @ v2.5.2).
    ///
    /// C++ returns the raw bytes up to the first NUL; this decodes them as UTF-8, replacing
    /// invalid sequences. [`CpuInfo::name_bytes`] returns the raw bytes.
    pub fn name(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(c_string(&self.name))
    }

    /// The brand string's bytes up to the first NUL, exactly as `CPUInfo::getName` returns them.
    pub fn name_bytes(&self) -> &[u8] {
        c_string(&self.name)
    }

    /// The vendor string. Port of `CPUInfo::getVendor` (src/OpenColorIO/CPUInfo.h:52 @ v2.5.2).
    ///
    /// C++ returns the raw bytes up to the first NUL; this decodes them as UTF-8, replacing
    /// invalid sequences. [`CpuInfo::vendor_bytes`] returns the raw bytes.
    pub fn vendor(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(c_string(&self.vendor))
    }

    /// The vendor string's bytes up to the first NUL, exactly as `CPUInfo::getVendor` returns
    /// them.
    pub fn vendor_bytes(&self) -> &[u8] {
        c_string(&self.vendor)
    }

    /// Port of `CPUInfo::hasSSE2` (src/OpenColorIO/CPUInfo.h:54 @ v2.5.2).
    pub fn has_sse2(&self) -> bool {
        self.build.use_sse2 && self.flags & X86_CPU_FLAG_SSE2 != 0
    }

    /// Port of `CPUInfo::SSE2Slow` (src/OpenColorIO/CPUInfo.h:55 @ v2.5.2).
    pub fn sse2_slow(&self) -> bool {
        self.build.use_sse2 && self.flags & X86_CPU_FLAG_SSE2_SLOW != 0
    }

    /// Port of `CPUInfo::hasSSE3` (src/OpenColorIO/CPUInfo.h:57 @ v2.5.2).
    pub fn has_sse3(&self) -> bool {
        self.build.use_sse3 && self.flags & X86_CPU_FLAG_SSE3 != 0
    }

    /// Port of `CPUInfo::SSE3Slow` (src/OpenColorIO/CPUInfo.h:58 @ v2.5.2).
    pub fn sse3_slow(&self) -> bool {
        self.build.use_sse3 && self.flags & X86_CPU_FLAG_SSE3_SLOW != 0
    }

    /// Port of `CPUInfo::hasSSSE3` (src/OpenColorIO/CPUInfo.h:60 @ v2.5.2).
    pub fn has_ssse3(&self) -> bool {
        self.build.use_ssse3 && self.flags & X86_CPU_FLAG_SSSE3 != 0
    }

    /// Port of `CPUInfo::SSSE3Slow` (src/OpenColorIO/CPUInfo.h:61 @ v2.5.2).
    pub fn ssse3_slow(&self) -> bool {
        self.build.use_ssse3 && self.flags & X86_CPU_FLAG_SSSE3_SLOW != 0
    }

    /// Port of `CPUInfo::hasSSE4` (src/OpenColorIO/CPUInfo.h:63 @ v2.5.2).
    pub fn has_sse4(&self) -> bool {
        self.build.use_sse4 && self.flags & X86_CPU_FLAG_SSE4 != 0
    }

    /// Port of `CPUInfo::hasSSE42` (src/OpenColorIO/CPUInfo.h:64 @ v2.5.2).
    pub fn has_sse42(&self) -> bool {
        self.build.use_sse42 && self.flags & X86_CPU_FLAG_SSE42 != 0
    }

    /// Port of `CPUInfo::hasAVX` (src/OpenColorIO/CPUInfo.h:66 @ v2.5.2).
    pub fn has_avx(&self) -> bool {
        self.build.use_avx && self.flags & X86_CPU_FLAG_AVX != 0
    }

    /// Port of `CPUInfo::AVXSlow` (src/OpenColorIO/CPUInfo.h:67 @ v2.5.2).
    pub fn avx_slow(&self) -> bool {
        self.build.use_avx && self.flags & X86_CPU_FLAG_AVX_SLOW != 0
    }

    /// Port of `CPUInfo::hasAVX2` (src/OpenColorIO/CPUInfo.h:69 @ v2.5.2).
    pub fn has_avx2(&self) -> bool {
        self.build.use_avx2 && self.flags & X86_CPU_FLAG_AVX2 != 0
    }

    /// Port of `CPUInfo::AVX2SlowGather` (src/OpenColorIO/CPUInfo.h:70 @ v2.5.2).
    pub fn avx2_slow_gather(&self) -> bool {
        self.build.use_avx2 && self.flags & X86_CPU_FLAG_AVX2_SLOWGATHER != 0
    }

    /// Port of `CPUInfo::hasAVX512` (src/OpenColorIO/CPUInfo.h:72 @ v2.5.2).
    pub fn has_avx512(&self) -> bool {
        self.build.use_avx512 && self.flags & X86_CPU_FLAG_AVX512 != 0
    }

    /// Port of `CPUInfo::hasF16C` (src/OpenColorIO/CPUInfo.h:74 @ v2.5.2).
    pub fn has_f16c(&self) -> bool {
        self.build.use_f16c && self.flags & X86_CPU_FLAG_F16C != 0
    }
}

/// The bytes of a NUL-terminated C string, as `const char *` getters return it.
fn c_string(bytes: &[u8]) -> &[u8] {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    &bytes[..end]
}

/// This machine's CPUID, decoded by [`CpuInfo::from_cpuid`].
#[cfg(target_arch = "x86_64")]
fn detect(build: BuildConfig) -> CpuInfo {
    use core::arch::x86_64::{__cpuid_count, _xgetbv};

    CpuInfo::from_cpuid(
        |leaf| {
            // CPUInfo.cpp:49-68 sets ECX (the sub-leaf) to 0.
            let r = __cpuid_count(leaf, 0);
            [r.eax, r.ebx, r.ecx, r.edx]
        },
        || {
            // SAFETY: `from_cpuid` calls this only when CPUID reports OSXSAVE, i.e. the OS has
            // enabled XSAVE and XGETBV is available, as upstream does (CPUInfo.cpp:110-112).
            unsafe { _xgetbv(0) }
        },
        build,
    )
}

/// Port of the ARM64 `CPUInfo::CPUInfo` (src/OpenColorIO/CPUInfo.cpp:186-211 @ v2.5.2).
#[cfg(target_arch = "aarch64")]
fn detect(build: BuildConfig) -> CpuInfo {
    let vendor: &[u8] = if cfg!(target_os = "macos") {
        b"Apple"
    } else {
        b"ARM"
    };
    let mut info = unknown_cpu(b"ARM", vendor);
    // SSE2NEON supports SSE, SSE2, SSE3, SSSE3, SSE4.1 and SSE4.2, but no AVX.
    if build.use_sse2 {
        info.flags = X86_CPU_FLAG_SSE2
            | X86_CPU_FLAG_SSE3
            | X86_CPU_FLAG_SSSE3
            | X86_CPU_FLAG_SSE4
            | X86_CPU_FLAG_SSE42;
    }
    info.build = build;
    info
}

/// Port of the unknown-processor `CPUInfo::CPUInfo` (src/OpenColorIO/CPUInfo.cpp:213-222 @ v2.5.2).
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
fn detect(build: BuildConfig) -> CpuInfo {
    let mut info = unknown_cpu(b"Unknown", b"Unknown");
    info.build = build;
    info
}

#[cfg(not(target_arch = "x86_64"))]
fn unknown_cpu(name: &[u8], vendor: &[u8]) -> CpuInfo {
    let mut info = CpuInfo {
        flags: 0,
        family: 0,
        model: 0,
        name: [0; 65],
        vendor: [0; 13],
        build: BuildConfig::NO_SIMD,
    };
    info.name[..name.len()].copy_from_slice(name);
    info.vendor[..vendor.len()].copy_from_slice(vendor);
    info
}

#[cfg(all(test, target_arch = "x86_64"))]
mod tests {
    use super::*;

    /// The CPUID bits OCIO decodes agree with the Rust standard library's own CPUID and XCR0
    /// decoding. (The slow-CPU quirks and the brand string are compared with the wheel's
    /// `ociocpuinfo`, which needs the `cpu_info` oracle command.)
    #[test]
    fn feature_bits_agree_with_std_detection() {
        use std::arch::is_x86_feature_detected as has;

        let cpu = CpuInfo::detect(BuildConfig::X86_64_WHEEL);
        let flag = |f: u32| cpu.flags & f != 0;
        assert_eq!(flag(X86_CPU_FLAG_SSE2), has!("sse2"));
        assert_eq!(flag(X86_CPU_FLAG_SSE3), has!("sse3"));
        assert_eq!(flag(X86_CPU_FLAG_SSSE3), has!("ssse3"));
        assert_eq!(flag(X86_CPU_FLAG_SSE4), has!("sse4.1"));
        assert_eq!(flag(X86_CPU_FLAG_SSE42), has!("sse4.2"));
        assert_eq!(flag(X86_CPU_FLAG_AVX), has!("avx"));
        assert_eq!(flag(X86_CPU_FLAG_F16C), has!("f16c"));
        assert_eq!(flag(X86_CPU_FLAG_AVX2), has!("avx2"));
        assert_eq!(
            flag(X86_CPU_FLAG_AVX512),
            has!("avx2")
                && has!("avx512f")
                && has!("avx512dq")
                && has!("avx512cd")
                && has!("avx512bw")
                && has!("avx512vl")
        );
    }
}
