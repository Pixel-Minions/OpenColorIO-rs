// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The tests of `ocio-ops`'s upstream test files that read files (WP 4.10a).
//!
//! Upstream's op and optimizer tests build ops from test files (`BuildOpsTest`,
//! `GetFileTransformProcessor`, tests/cpu/UnitTestUtils.cpp @ v2.5.2), which needs the file
//! readers and the file transform of this crate. `ocio-ops` can't depend on this crate, so
//! these tests live here, one module per upstream file, next to the readers they need; the
//! tests of the same upstream files that don't read files stay in `ocio-ops`.

mod lut1d_op_cpu_tests;
mod lut1d_op_data_tests;
mod lut1d_op_tests;
mod lut3d_op_data_tests;
mod lut3d_op_tests;
mod op_optimizers_tests;
