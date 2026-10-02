// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Lut1D op family: a port of `src/OpenColorIO/ops/lut1d/` @ v2.5.2, the forward parts
//! the optimizer's separable-prefix bake needs.

pub mod lut1d_op_data;

pub use lut1d_op_data::Lut1DOpData;
