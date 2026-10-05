// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The FixedFunction op family: a port of `src/OpenColorIO/ops/fixedfunction/` @ v2.5.2.

pub mod fixed_function_op;
pub mod fixed_function_op_cpu;
pub mod fixed_function_op_data;

pub use fixed_function_op_data::FixedFunctionOpStyle;
