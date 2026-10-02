// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Gamma op family: a port of `src/OpenColorIO/ops/gamma/` @ v2.5.2.

pub mod gamma_op;
pub mod gamma_op_cpu;
pub mod gamma_op_data;
pub mod gamma_op_utils;

pub use gamma_op_data::{GammaOpData, GammaStyle};
