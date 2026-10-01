// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the optimizer's core. Upstream's tests (tests/cpu/OpOptimizers_tests.cpp @ v2.5.2)
//! build their ops with `CreateScaleOp`, `CreateMatrixOffsetOp` (chunk 1.3m3) and the other
//! families' factories; they come with those. `tests/op_optimizers_oracle.rs` checks the
//! optimized lists of Matrix ops against the wheel; the debug log's text needs the oracle's
//! debug-log command, which comes in its own chunk.

use super::*;
use crate::open_color_types::TransformDirection;
use crate::ops::matrix::MatrixOpData;
use crate::ops::matrix::matrix_op::create_matrix_op;
use crate::ops::noop::{create_file_no_op, create_look_no_op};

/// A scale op: a diagonal matrix.
fn scale(ops: &mut OpVec, s: f64, dir: TransformDirection) {
    create_matrix_op(ops, MatrixOpData::create_diagonal_matrix(s), dir);
}

#[test]
fn none_only_removes_the_no_op_types() {
    let mut ops = OpVec::new();
    create_file_no_op(&mut ops, b"file");
    scale(&mut ops, 1.0, TransformDirection::Forward);
    create_look_no_op(&mut ops, b"look");
    scale(&mut ops, 2.0, TransformDirection::Forward);
    scale(&mut ops, 2.0, TransformDirection::Forward);
    ops.finalize().unwrap();

    ops.optimize(OptimizationFlags::NONE).unwrap();
    // The identity and both scales stay.
    assert_eq!(ops.len(), 3);
    assert!(ops.iter().all(|op| op.get_info() == "<MatrixOffsetOp>"));

    // With the default level, the identity goes, and the scales combine.
    ops.optimize(OptimizationFlags::DEFAULT).unwrap();
    assert_eq!(ops.len(), 1);
}

#[test]
fn matrices_combine_only_with_their_flag() {
    let mut ops = OpVec::new();
    scale(&mut ops, 2.0, TransformDirection::Forward);
    scale(&mut ops, 4.0, TransformDirection::Forward);
    scale(&mut ops, 0.5, TransformDirection::Forward);
    ops.finalize().unwrap();

    let mut without = ops.clone();
    without
        .optimize(
            OptimizationFlags::LOSSLESS & OptimizationFlags(!OptimizationFlags::COMP_MATRIX.0),
        )
        .unwrap();
    assert_eq!(without.len(), 3);

    let mut with = ops.clone();
    with.optimize(OptimizationFlags::COMP_MATRIX).unwrap();
    assert_eq!(with.len(), 1);
}

#[test]
fn a_matrix_and_its_inverse_leave_nothing() {
    // Matrix pairs aren't removed as inverses: they combine into an identity, which isn't
    // kept.
    let mut ops = OpVec::new();
    scale(&mut ops, 2.0, TransformDirection::Forward);
    scale(&mut ops, 3.0, TransformDirection::Forward);
    scale(&mut ops, 3.0, TransformDirection::Inverse);
    scale(&mut ops, 2.0, TransformDirection::Inverse);
    ops.finalize().unwrap();
    ops.optimize(OptimizationFlags::COMP_MATRIX).unwrap();
    assert!(ops.is_empty());
}

#[test]
fn a_prefix_of_matrices_is_not_baked() {
    let mut ops = OpVec::new();
    scale(&mut ops, 2.0, TransformDirection::Forward);
    ops.finalize().unwrap();
    for in_bd in [
        BitDepth::Uint8,
        BitDepth::Uint10,
        BitDepth::Uint12,
        BitDepth::Uint16,
        BitDepth::F16,
        BitDepth::F32,
    ] {
        ops.optimize_for_bitdepth(in_bd, BitDepth::Uint8, OptimizationFlags::ALL)
            .unwrap();
        assert_eq!(ops.len(), 1);
    }
    // An unsupported bit depth is refused, as `IsFloatBitDepth` does.
    assert!(
        ops.optimize_for_bitdepth(BitDepth::Uint14, BitDepth::F32, OptimizationFlags::ALL)
            .is_err()
    );
}
