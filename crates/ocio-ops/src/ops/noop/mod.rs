// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The no-op family: ops that leave pixels alone and carry information along an op list. A
//! port of `src/OpenColorIO/ops/noop/` @ v2.5.2.

pub mod no_ops;

pub use no_ops::{FileNoOpData, NoOpData, NoOpKind, create_file_no_op, create_look_no_op};
