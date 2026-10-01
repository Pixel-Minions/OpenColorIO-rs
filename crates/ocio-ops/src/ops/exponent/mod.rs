// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Exponent op: its data, its CPU renderer and the op (`src/OpenColorIO/ops/exponent/` @
//! v2.5.2).

pub mod exponent_op;
pub mod exponent_op_cpu;
pub mod exponent_op_data;

pub use exponent_op_data::ExponentOpData;
