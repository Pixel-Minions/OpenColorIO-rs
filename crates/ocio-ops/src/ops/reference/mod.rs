// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! References: CLF/CTF process nodes that stand for the ops of another file. A port of
//! `src/OpenColorIO/ops/reference/` @ v2.5.2.

pub mod reference_op_data;

pub use reference_op_data::{ReferenceOpData, ReferenceStyle};
