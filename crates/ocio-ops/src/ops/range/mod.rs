// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Range op family: a port of `src/OpenColorIO/ops/range/` @ v2.5.2.

pub mod range_op;
pub mod range_op_cpu;
pub mod range_op_data;

pub use range_op_data::RangeOpData;
