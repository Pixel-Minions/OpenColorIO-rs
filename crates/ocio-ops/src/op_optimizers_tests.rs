// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the optimizer's core. Upstream's tests (tests/cpu/OpOptimizers_tests.cpp @ v2.5.2)
//! build their ops with `CreateScaleOp`, `CreateMatrixOffsetOp` (chunk 1.3m3) and the other
//! families' factories; they come with those. Until then the optimizer is checked against the
//! wheel: `tests/op_optimizers_oracle.rs` (the optimized lists of Matrix ops, for every level
//! and several flags alone), and `tests/cpu_processor_oracle.rs` (the debug log, the pass cap,
//! the no-op types, the refused bit depths).
