// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Matrix op family: a port of `src/OpenColorIO/ops/matrix/` @ v2.5.2.

pub mod matrix_op;
pub mod matrix_op_cpu;
#[cfg(target_arch = "x86_64")]
mod matrix_op_cpu_sse2;
pub mod matrix_op_data;

pub use matrix_op_data::{MatrixArray, MatrixOpData, Offsets};
