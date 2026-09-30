// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Op data, CPU renderers and the optimizer: a port of `src/OpenColorIO/ops/`,
//! `Op.cpp`, `OpOptimizers.cpp`, `CPUInfo.cpp`, `BitDepthUtils` and `ImagePacking`
//! from OpenColorIO 2.5.2.
//!
//! `unsafe` is denied here and allowed only in the SIMD modules (PLAN.md D8).
//!
//! The crate builds only for the targets whose wheels it has been verified against:
//! `x86_64-pc-windows-msvc` and `x86_64-unknown-linux-gnu` (PLAN.md D11). The two wheels were
//! built differently, and the port does what each one does (D12): where they differ, the
//! Windows variant is selected with `target_os = "windows"` and the Linux one (GCC,
//! libstdc++, glibc) with `target_os = "linux"`. The wheel of any other target (macOS, ARM,
//! musl, MinGW) may be built differently, and Rust's `std` calls another math library there.
#![deny(unsafe_code)]

#[cfg(not(all(
    target_arch = "x86_64",
    any(
        all(target_os = "windows", target_env = "msvc"),
        all(target_os = "linux", target_env = "gnu")
    )
)))]
compile_error!(
    "ocio-ops reproduces the OpenColorIO 2.5.2 wheels for x86_64-pc-windows-msvc and \
     x86_64-unknown-linux-gnu only (PLAN.md D11): the wheel behavior of this target has not \
     been verified"
);

pub mod exception;
pub mod hash_utils;
pub mod math_utils;
pub mod op;
pub mod open_color_types;
pub mod ops;
pub mod platform;
pub mod sse;

pub use exception::{Exception, ExceptionKind, Result};
