// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Op data, CPU renderers and the optimizer: a port of `src/OpenColorIO/ops/`,
//! `Op.cpp`, `OpOptimizers.cpp`, `CPUInfo.cpp`, `BitDepthUtils` and `ImagePacking`
//! from OpenColorIO 2.5.2.
//!
//! `unsafe` is denied here and allowed only in the SIMD modules (PLAN.md D8).
#![deny(unsafe_code)]
