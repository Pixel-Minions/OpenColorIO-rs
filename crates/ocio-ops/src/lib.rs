// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Op data, CPU renderers and the optimizer: a port of `src/OpenColorIO/ops/`,
//! `Op.cpp`, `OpOptimizers.cpp`, `CPUInfo.cpp`, `BitDepthUtils` and `ImagePacking`
//! from OpenColorIO 2.5.2.
//!
//! `unsafe` is denied here and allowed only in the SIMD modules (PLAN.md D8).
#![deny(unsafe_code)]

pub mod avx;
pub mod avx2;
pub mod avx512;
pub mod cpu_info;
pub mod imath_half;
pub mod ops;
pub mod sse2;
