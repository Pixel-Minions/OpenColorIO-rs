// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the Matrix op: the one test of `tests/cpu/ops/matrix/MatrixOp_tests.cpp` @ v2.5.2
//! that needs only these creation functions (the others use `CreateScaleOp`, `CreateFitOp` and
//! the other factories of chunk 1.3m3), and the op's behaviors, with the messages upstream's
//! tests expect (`throw_combine`, `throw_validate`). `tests/matrix_op_oracle.rs` checks the
//! cache IDs and the combinations against the wheel.

use super::*;
use crate::op::OpVec;
use crate::ops::matrix::matrix_op_data::Offsets;
use crate::ops::noop::create_file_no_op;

/// The matrix data of a Matrix op.
fn matrix(op: &Op) -> &MatrixOpData {
    match &**op.data() {
        OpData::Matrix(data) => data,
        other => panic!("not a Matrix op: {other:?}"),
    }
}

/// A matrix with offsets: upstream's `m1` and `v1` (MatrixOp_tests.cpp:368-373 @ v2.5.2).
fn m1() -> MatrixOpData {
    let mut mat = MatrixOpData::new();
    mat.set_rgba(&[
        1.1, 0.2, 0.3, 0.4, 0.5, 1.6, 0.7, 0.8, 0.2, 0.1, 1.1, 0.2, 0.3, 0.4, 0.5, 1.6,
    ]);
    mat.set_offsets(Offsets::new(-0.5, -0.25, 0.25, 0.0));
    mat
}

/// Port of `OCIO_ADD_TEST(MatrixOffsetOp, removing_red_green)` @ v2.5.2.
#[test]
fn removing_red_green() {
    let mut mat = MatrixOpData::new();
    mat.set_array_value(0, 0.0);
    mat.set_array_value(5, 0.0);
    let mut ops = OpVec::new();
    create_matrix_op(&mut ops, mat, TransformDirection::Forward);
    assert_eq!(ops.len(), 1);
    ops[0].validate().unwrap();
    ops[0].finalize().unwrap();

    const NB_PIXELS: usize = 6;
    let src: [f32; NB_PIXELS * 4] = [
        0.1004, 0.201, 0.303, 0.408, //
        -0.1008, -0.207, 0.502, 0.123422, //
        1.0090, 1.009, 1.044, 1.001, //
        1.1, 1.2, 1.3, 1.0, //
        1.4, 1.5, 1.6, 0.0, //
        1.7, 1.8, 1.9, 1.0,
    ];

    let mut tmp = src;

    ops[0].apply(&mut tmp).unwrap();

    // As upstream's loop, which steps through the first NB_PIXELS values 4 at a time.
    for idx in (0..NB_PIXELS).step_by(4) {
        assert_eq!(0.0f32, tmp[idx]);
        assert_eq!(0.0f32, tmp[idx + 1]);
        assert_eq!(src[idx + 2], tmp[idx + 2]);
        assert_eq!(src[idx + 3], tmp[idx + 3]);
    }
}

#[test]
fn create_matrix_op_combines_the_directions() {
    let mut ops = OpVec::new();
    create_matrix_op(&mut ops, m1(), TransformDirection::Forward);
    create_matrix_op(&mut ops, m1(), TransformDirection::Inverse);
    let mut inverse_data = m1();
    inverse_data.set_direction(TransformDirection::Inverse);
    create_matrix_op(&mut ops, inverse_data, TransformDirection::Inverse);

    let directions: Vec<_> = ops.iter().map(|op| matrix(op).get_direction()).collect();
    assert_eq!(
        directions,
        [
            TransformDirection::Forward,
            TransformDirection::Inverse,
            TransformDirection::Forward
        ]
    );
    for op in ops.iter() {
        assert_eq!(op.get_info(), "<MatrixOffsetOp>");
        assert!(op.is_same_type(&ops[0]));
        assert!(!op.is_inverse(&ops[0]));
        assert!(op.has_channel_crosstalk());
        assert!(!op.is_no_op().unwrap());
    }
}

#[test]
fn finalize_makes_an_inverse_op_forward() {
    let mut ops = OpVec::new();
    create_matrix_op(&mut ops, m1(), TransformDirection::Inverse);
    let mut inverse = m1();
    inverse.set_direction(TransformDirection::Inverse);

    // Before: the renderer needs a forward matrix.
    let error = ops[0].get_cpu_op(false).unwrap_err();
    assert!(error.message().contains("Op::finalize has to be called"));

    let before = ops[0].clone();
    ops[0].finalize().unwrap();
    assert!(*matrix(&ops[0]) == inverse.get_as_forward().unwrap());
    // New data: the op shared before keeps the inverse.
    assert_eq!(matrix(&before).get_direction(), TransformDirection::Inverse);
    assert!(ops[0].get_cpu_op(false).unwrap().is_some());

    // A forward op keeps its data.
    let mut forward = OpVec::new();
    create_matrix_op(&mut forward, m1(), TransformDirection::Forward);
    let shared = forward[0].clone();
    forward[0].finalize().unwrap();
    assert!(Arc::ptr_eq(forward[0].data(), shared.data()));
}

/// The validation of an inverse op checks that the matrix can be inverted, as upstream's
/// `throw_validate` test does (MatrixOp_tests.cpp:622-630 @ v2.5.2) with a scale of 0.
#[test]
fn an_inverse_op_needs_an_invertible_matrix() {
    let mut mat = MatrixOpData::create_diagonal_matrix(1.0);
    mat.set_array_value(0, 0.0);
    mat.set_array_value(5, 1.3);
    mat.set_array_value(10, 0.3);
    let mut ops = OpVec::new();
    create_matrix_op(&mut ops, mat, TransformDirection::Inverse);
    let error = ops[0].validate().unwrap_err();
    assert!(
        error
            .message()
            .contains("Singular Matrix can't be inverted")
    );
}

/// The scenarios of upstream's `throw_combine` test (MatrixOp_tests.cpp:632-691 @ v2.5.2),
/// with the matrices that its `CreateOffsetOp` and `CreateScaleOp` make, and its messages.
#[test]
fn combining_needs_two_forward_matrix_ops() {
    let mut offset = MatrixOpData::new();
    offset.set_offsets(Offsets::new(1.1, -1.3, 0.3, 0.0));
    let mut scale_no_inv = MatrixOpData::create_diagonal_matrix(1.0);
    for (k, v) in [(0, 1.1), (5, 0.0), (10, 0.3), (15, 0.0)] {
        scale_no_inv.set_array_value(k, v);
    }

    // Combining with a different op.
    let mut ops = OpVec::new();
    create_matrix_op(&mut ops, offset.clone(), TransformDirection::Forward);
    create_file_no_op(&mut ops, b"NoOp");
    assert!(!ops[0].can_combine_with(&ops[1]).unwrap());
    let mut combined_ops = OpVec::new();
    let error = ops[0].combine_with(&mut combined_ops, &ops[1]).unwrap_err();
    assert!(
        error
            .message()
            .contains("MatrixOffsetOp: canCombineWith must be checked before calling combineWith")
    );

    // An inverse op on either side, before finalize.
    for (first, second) in [
        (TransformDirection::Forward, TransformDirection::Inverse),
        (TransformDirection::Inverse, TransformDirection::Forward),
        (TransformDirection::Inverse, TransformDirection::Inverse),
    ] {
        for (a, b) in [(&offset, &scale_no_inv), (&scale_no_inv, &offset)] {
            let mut ops = OpVec::new();
            create_matrix_op(&mut ops, a.clone(), first);
            create_matrix_op(&mut ops, b.clone(), second);
            let error = ops[0].can_combine_with(&ops[1]).unwrap_err();
            assert!(error.message().contains("Op::finalize has to be called"));
            let error = ops[0].combine_with(&mut OpVec::new(), &ops[1]).unwrap_err();
            assert!(error.message().contains("Op::finalize has to be called"));
        }
    }
}

#[test]
fn combining_appends_the_composition_unless_it_is_a_no_op() {
    // A matrix then its inverse: the composition is the identity, and no op is appended.
    let mut ops = OpVec::new();
    create_matrix_op(
        &mut ops,
        MatrixOpData::create_diagonal_matrix(2.0),
        TransformDirection::Forward,
    );
    create_matrix_op(
        &mut ops,
        MatrixOpData::create_diagonal_matrix(2.0),
        TransformDirection::Inverse,
    );
    ops.finalize().unwrap();
    assert!(ops[0].can_combine_with(&ops[1]).unwrap());
    let mut combined = OpVec::new();
    ops[0].combine_with(&mut combined, &ops[1]).unwrap();
    assert!(combined.is_empty());

    // Two others: one op, forward, holding the composition.
    let mut ops = OpVec::new();
    create_matrix_op(&mut ops, m1(), TransformDirection::Forward);
    create_matrix_op(
        &mut ops,
        MatrixOpData::create_diagonal_matrix(2.0),
        TransformDirection::Forward,
    );
    let mut combined = OpVec::new();
    ops[0].combine_with(&mut combined, &ops[1]).unwrap();
    assert_eq!(combined.len(), 1);
    let expected = m1()
        .compose(&MatrixOpData::create_diagonal_matrix(2.0))
        .unwrap();
    assert!(*matrix(&combined[0]) == expected);
    assert_eq!(
        matrix(&combined[0]).get_direction(),
        TransformDirection::Forward
    );
}

#[test]
fn the_identity_op_and_replacement() {
    let mut ops = OpVec::new();
    create_identity_matrix_op(&mut ops);
    assert_eq!(ops.len(), 1);
    assert!(ops[0].is_no_op().unwrap());
    assert!(ops[0].is_identity().unwrap());
    assert!(*matrix(&ops[0]) == MatrixOpData::create_diagonal_matrix(1.0));

    // The replacement of any op the optimizer finds to be an identity is an identity matrix
    // op, for the types so far.
    let mut file = OpVec::new();
    create_file_no_op(&mut file, b"path");
    for op in [&ops[0], &file[0]] {
        let replacement = op.get_identity_replacement().unwrap();
        assert!(*matrix(&replacement) == MatrixOpData::new());
        assert_eq!(
            matrix(&replacement).get_direction(),
            TransformDirection::Forward
        );
    }
}

#[test]
fn clone_op_copies_the_data_and_metadata() {
    let mut data = m1();
    data.set_name(b"mat1");
    let mut ops = OpVec::new();
    create_matrix_op(&mut ops, data, TransformDirection::Forward);
    let cloned = ops[0].clone_op();
    assert!(!Arc::ptr_eq(cloned.data(), ops[0].data()));
    assert!(*matrix(&cloned) == *matrix(&ops[0]));
    assert_eq!(cloned.data().get_name(), b"mat1");
}

#[test]
fn op_vec_from_matrix_data_in_both_directions() {
    let data: crate::op_data::OpDataRcPtr = Arc::new(OpData::Matrix(m1()));
    let mut ops = OpVec::new();
    crate::op::create_op_vec_from_op_data(&mut ops, &data, TransformDirection::Forward).unwrap();
    crate::op::create_op_vec_from_op_data(&mut ops, &data, TransformDirection::Inverse).unwrap();
    assert_eq!(ops.len(), 2);
    // Copies of the data.
    assert!(!Arc::ptr_eq(ops[0].data(), &data));
    assert_eq!(matrix(&ops[0]).get_direction(), TransformDirection::Forward);
    assert_eq!(matrix(&ops[1]).get_direction(), TransformDirection::Inverse);

    // The inverse of a list: the ops in reverse order, each inverted.
    let inverted = ops.invert().unwrap();
    assert_eq!(
        matrix(&inverted[0]).get_direction(),
        TransformDirection::Forward
    );
    assert_eq!(
        matrix(&inverted[1]).get_direction(),
        TransformDirection::Inverse
    );
}

/// A 3x3 matrix in an op, as a CLF or CTF file gives one: validating the op makes it 4x4 in the
/// op's data, as upstream's `const` `MatrixArray::validate` does through a `const_cast`
/// (MatrixOpData.cpp:413-436 @ v2.5.2), with the 3x3 values as its RGB part and alpha passed
/// through (`expandFrom3x3To4x4`, MatrixOpData.cpp:365-372). So finalizing the ops, and a CPU
/// processor of them, work on 4 by 4 values; before, the processor indexed past the 9 values.
#[test]
fn validating_a_3x3_matrix_op_makes_it_4x4() {
    use crate::cpu_processor::CpuProcessor;
    use crate::open_color_types::{BitDepth, OptimizationFlags};

    let values3 = [1.1, 0.2, 0.3, 0.4, 1.5, 0.6, 0.7, 0.8, 1.9];
    let mut data = MatrixOpData::new();
    data.get_array_mut().resize(3, 3);
    data.get_array_mut()
        .get_values_mut()
        .copy_from_slice(&values3);
    let mut ops = OpVec::new();
    create_matrix_op(&mut ops, data, TransformDirection::Forward);
    let shared = ops[0].clone();

    // The processor validates a copy of the ops.
    let cpu =
        CpuProcessor::new(&ops, BitDepth::F32, BitDepth::F32, OptimizationFlags::NONE).unwrap();

    ops.finalize().unwrap();
    let expanded = matrix(&ops[0]).get_array();
    assert_eq!(expanded.get_length(), 4);
    assert_eq!(
        expanded.get_values()[..],
        [
            1.1, 0.2, 0.3, 0.0, //
            0.4, 1.5, 0.6, 0.0, //
            0.7, 0.8, 1.9, 0.0, //
            0.0, 0.0, 0.0, 1.0,
        ]
    );
    // An op sharing the data before keeps its 3x3 copy.
    assert_eq!(matrix(&shared).get_array().get_length(), 3);

    let mut ops4 = OpVec::new();
    let mut data4 = MatrixOpData::new();
    data4
        .get_array_mut()
        .get_values_mut()
        .copy_from_slice(expanded.get_values());
    create_matrix_op(&mut ops4, data4, TransformDirection::Forward);
    let cpu4 =
        CpuProcessor::new(&ops4, BitDepth::F32, BitDepth::F32, OptimizationFlags::NONE).unwrap();
    assert_eq!(cpu.get_cache_id(), cpu4.get_cache_id());
}

/// A 3x3 matrix before validation (docs/improvements.md, U-16): the queries that upstream
/// answers by reading past its 9 values return an error, on the data and on its op; once the
/// op is validated, they answer.
#[test]
fn queries_of_an_unvalidated_3x3_matrix_are_errors() {
    let mut data = MatrixOpData::new();
    data.get_array_mut().resize(3, 3);
    data.get_array_mut()
        .get_values_mut()
        .copy_from_slice(&[1., 0., 0., 0., 1., 0., 0., 0., 1.]);
    let message = "Matrix: a 3x3 matrix has to be validated before this query: upstream reads \
                   past its 9 values.";
    assert_eq!(data.has_alpha().unwrap_err().message(), message);
    assert_eq!(data.is_identity().unwrap_err().message(), message);
    assert_eq!(data.is_no_op().unwrap_err().message(), message);
    assert_eq!(data.get_cache_id().unwrap_err().message(), message);
    // The queries that read only the 3x3 positions answer.
    assert!(data.is_diagonal());
    assert!(data.is_unity_diagonal());

    let mut ops = OpVec::new();
    create_matrix_op(&mut ops, data, TransformDirection::Forward);
    assert_eq!(ops[0].is_no_op().unwrap_err().message(), message);
    assert_eq!(ops[0].is_identity().unwrap_err().message(), message);
    assert_eq!(ops[0].get_cache_id().unwrap_err().message(), message);
    assert_eq!(ops.is_no_op().unwrap_err().message(), message);
    assert_eq!(ops.get_cache_id().unwrap_err().message(), message);
    assert_eq!(
        crate::op::serialize_op_vec(&ops, 0).unwrap_err().message(),
        message
    );

    ops[0].validate().unwrap();
    assert!(ops[0].is_no_op().unwrap());
    assert!(!ops[0].get_cache_id().unwrap().is_empty());
}
