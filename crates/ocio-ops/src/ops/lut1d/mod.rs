// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Lut1D op family: a port of `src/OpenColorIO/ops/lut1d/` @ v2.5.2: the op data, the op,
//! and the forward CPU renderers but their SIMD kernels.

pub mod lut1d_op;
pub mod lut1d_op_cpu;
pub mod lut1d_op_data;

pub use lut1d_op_data::Lut1DOpData;
