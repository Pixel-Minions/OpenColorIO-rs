// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Op data, CPU renderers and the optimizer: a port of `src/OpenColorIO/ops/`,
//! `Op.cpp`, `OpOptimizers.cpp`, `CPUInfo.cpp`, `BitDepthUtils` and `ImagePacking`
//! from OpenColorIO 2.5.2.
//!
//! `unsafe` is denied here and allowed only in the SIMD modules (PLAN.md D8).
//!
//! The crate builds only for the targets whose wheels it has been verified against:
//! `x86_64-pc-windows-msvc` and `x86_64-unknown-linux-gnu` (PLAN.md D11), and the few targets
//! that `cfg` cannot tell apart from the latter (see the `compile_error!`). The two wheels were
//! built differently, and the port does what each one does (D12): where they differ, the
//! Windows variant is selected with `target_os = "windows"` and the Linux one (GCC,
//! libstdc++, glibc) with `target_os = "linux"`. The wheel of any other target (macOS, ARM,
//! musl, MinGW) may be built differently, and Rust's `std` calls another math library there.
#![deny(unsafe_code)]

// The Windows arm accepts `x86_64-pc-windows-msvc` alone: `target_vendor = "pc"` rules out
// the UWP and Windows 7 targets. The Linux arm accepts `x86_64-unknown-linux-gnu`, and also
// the targets that `cfg` cannot tell apart from it, which have the same glibc ABI: its
// sanitizer variants (`x86_64-unknown-linux-gnuasan`, `-gnumsan`, `-gnutsan`) and
// `x86_64-oe-linux-gnu` (OpenEmbedded). `target_pointer_width` and `target_abi` rule out
// `x86_64-unknown-linux-gnux32`.
#[cfg(not(all(
    target_arch = "x86_64",
    any(
        all(target_os = "windows", target_env = "msvc", target_vendor = "pc"),
        all(
            target_os = "linux",
            target_env = "gnu",
            target_vendor = "unknown",
            target_pointer_width = "64",
            target_abi = ""
        )
    )
)))]
compile_error!(
    "ocio-ops reproduces the OpenColorIO 2.5.2 wheels for x86_64-pc-windows-msvc and \
     x86_64-unknown-linux-gnu only (PLAN.md D11): the wheel behavior of this target has not \
     been verified"
);

pub mod avx;
pub mod avx2;
pub mod avx512;
pub mod cpu_info;
pub mod exception;
pub mod hash_utils;
pub mod imath_half;
pub mod math_utils;
pub mod op;
pub mod open_color_types;
pub mod ops;
pub mod platform;
pub mod sse;
pub mod sse2;

pub use exception::{Exception, ExceptionKind, Result};
