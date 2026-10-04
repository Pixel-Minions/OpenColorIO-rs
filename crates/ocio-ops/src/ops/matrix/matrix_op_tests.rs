// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the Matrix op: `tests/cpu/ops/matrix/MatrixOp_tests.cpp` @ v2.5.2, but for
//! `create_transform` (it needs `CreateMatrixTransform` and `BuildMatrixOp`: in
//! crates/ocio/src/transforms/matrix_transform_tests.rs); and the op's behaviors.
//! `tests/matrix_op_oracle.rs` checks the cache IDs and the combinations against the wheel, and
//! `tests/matrix_factories_oracle.rs` the factories.

use ocio_testkit::upstream::{check_close, check_throw_what, equal_with_safe_rel_error};

use super::*;
use crate::op::OpVec;
use crate::ops::log::log_op::create_log_op_from_parameters;
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
    let cloned = ops[0].clone_op().unwrap();
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
    // The renderers read the 16 values too.
    let renderer_error = |r: Result<_>| match r {
        Ok(_) => panic!("a renderer for an unvalidated 3x3 matrix"),
        Err(e) => crate::exception::Exception::message(&e).to_string(),
    };
    assert_eq!(renderer_error(ops[0].get_cpu_op(false)), message);
    assert_eq!(renderer_error(ops[0].get_cpu_op(true)), message);
    let mut pixel = [0.5f32, 0.25, 0.125, 1.0];
    assert_eq!(ops[0].apply(&mut pixel).unwrap_err().message(), message);
    let mut out = [0.0f32; 4];
    assert_eq!(
        ops[0].apply_in_out(&pixel, &mut out).unwrap_err().message(),
        message
    );

    ops[0].validate().unwrap();
    assert!(ops[0].is_no_op().unwrap());
    assert!(!ops[0].get_cache_id().unwrap().is_empty());
    assert!(ops[0].get_cpu_op(false).unwrap().is_some());
}

/// A 3x3 matrix with offsets, before validation: `isIdentity` returns false on the offsets
/// before `hasAlpha` reads past the 9 values (MatrixOpData.cpp:534-539 @ v2.5.2), so the
/// query answers (docs/improvements.md, U-16), and so does `isNoOp`.
#[test]
fn an_unvalidated_3x3_matrix_with_offsets_is_not_an_identity() {
    let mut data = MatrixOpData::new();
    data.get_array_mut().resize(3, 3);
    data.get_array_mut()
        .get_values_mut()
        .copy_from_slice(&[1., 0., 0., 0., 1., 0., 0., 0., 1.]);
    data.set_offset_value(0, 0.5).unwrap();
    assert!(data.has_alpha().is_err());
    assert!(!data.is_identity().unwrap());
    assert!(!data.is_no_op().unwrap());

    let mut ops = OpVec::new();
    create_matrix_op(&mut ops, data, TransformDirection::Forward);
    assert!(!ops[0].is_identity().unwrap());
    assert!(!ops[0].is_no_op().unwrap());
}

/// `OCIO_CHECK_CLOSE(expected[i], tmp[i], error)` for every value, in `float`.
#[track_caller]
fn check_all_close(expected: &[f32], tmp: &[f32], error: f32) {
    for (&e, &t) in expected.iter().zip(tmp) {
        check_close(e, t, error);
    }
}

/// `OCIO_CHECK_CLOSE(dst[idx], tmp[idx], error)` with a `double` `dst`: in `double`.
#[track_caller]
fn check_all_close_f64(expected: &[f64], tmp: &[f32], error: f32) {
    for (&e, &t) in expected.iter().zip(tmp) {
        check_close(e, f64::from(t), f64::from(error));
    }
}

/// `OCIO_CHECK_ASSERT(OCIO::EqualWithSafeRelError((float)dst[idx], (float)tmp[idx], error,
/// 1.0f))` for every value.
#[track_caller]
fn check_all_safe_rel(expected: &[f32], tmp: &[f32], error: f32) {
    for (&e, &t) in expected.iter().zip(tmp) {
        assert!(equal_with_safe_rel_error(e, t, error, 1.0), "{e} {t}");
    }
}

/// Port of `OCIO_ADD_TEST(MatrixOffsetOp, scale)` @ v2.5.2.
#[test]
fn scale() {
    let error = 1e-6f32;

    let mut ops = OpVec::new();
    let scale = [1.1, 1.3, 0.3, -1.0];
    create_scale_op(&mut ops, &scale, TransformDirection::Forward);
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].get_info(), "<MatrixOffsetOp>");

    create_scale_op(&mut ops, &scale, TransformDirection::Inverse);
    assert_eq!(ops.len(), 2);
    ops.finalize().unwrap();

    let cache_id = ops[0].get_cache_id().unwrap();
    assert!(!cache_id.is_empty());

    let src: [f32; 12] = [
        0.1004, 0.2, 0.3, 0.4, //
        -0.1008, -0.2, 5.001, 0.1234, //
        1.0090, 1.0, 1.0, 1.0,
    ];

    let dst: [f32; 12] = [
        0.11044, 0.26, 0.090, -0.4, //
        -0.11088, -0.26, 1.5003, -0.1234, //
        1.10990, 1.30, 0.300, -1.0,
    ];

    let mut tmp = src;

    ops[0].apply(&mut tmp).unwrap();
    check_all_close(&dst, &tmp, error);

    ops[1].apply(&mut tmp).unwrap();
    check_all_close(&src, &tmp, error);
}

/// Port of `OCIO_ADD_TEST(MatrixOffsetOp, offset)` @ v2.5.2.
#[test]
fn offset() {
    let error = 1e-6f32;

    let mut ops = OpVec::new();
    let offset = [1.1, -1.3, 0.3, -1.0];
    create_offset_op(&mut ops, &offset, TransformDirection::Forward);
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].get_info(), "<MatrixOffsetOp>");

    create_offset_op(&mut ops, &offset, TransformDirection::Inverse);
    assert_eq!(ops.len(), 2);
    ops.finalize().unwrap();

    let src: [f32; 12] = [
        0.1004, 0.2, 0.3, 0.4, //
        -0.1008, -0.2, 5.01, 0.1234, //
        1.0090, 1.0, 1.0, 1.0,
    ];

    let dst: [f32; 12] = [
        1.2004, -1.1, 0.60, -0.6, //
        0.9992, -1.5, 5.31, -0.8766, //
        2.1090, -0.3, 1.30, 0.0,
    ];

    let mut tmp = src;

    ops[0].apply(&mut tmp).unwrap();
    check_all_close(&dst, &tmp, error);

    ops[1].apply(&mut tmp).unwrap();
    check_all_close(&src, &tmp, error);
}

/// The matrix of upstream's `matrix`, `arbitrary` and `combining` tests.
const M1: [f64; 16] = [
    1.1, 0.2, 0.3, 0.4, //
    0.5, 1.6, 0.7, 0.8, //
    0.2, 0.1, 1.1, 0.2, //
    0.3, 0.4, 0.5, 1.6,
];

/// Port of `OCIO_ADD_TEST(MatrixOffsetOp, matrix)` @ v2.5.2.
#[test]
fn matrix_op() {
    let error = 1e-6f32;

    let mut ops = OpVec::new();
    create_matrix_op_from_m44(&mut ops, &M1, TransformDirection::Forward);
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].get_info(), "<MatrixOffsetOp>");

    create_matrix_op_from_m44(&mut ops, &M1, TransformDirection::Inverse);
    assert_eq!(ops.len(), 2);
    ops.finalize().unwrap();

    let src: [f32; 12] = [
        0.1004, 0.201, 0.303, 0.408, //
        -0.1008, -0.207, 5.002, 0.123422, //
        1.0090, 1.009, 1.044, 1.001,
    ];

    // `const double dst[]` initialized from `float` literals, then cast to `float`.
    let dst: [f32; 12] = [
        0.40474, 0.91030, 0.45508, 0.914820, //
        1.3976888, 3.2185376, 5.4860244, 2.5854352, //
        2.02530, 3.65050, 1.65130, 2.829900,
    ];

    let mut tmp = src;

    ops[0].apply(&mut tmp).unwrap();
    check_all_safe_rel(&dst, &tmp, error);

    ops[1].apply(&mut tmp).unwrap();
    check_all_safe_rel(&src, &tmp, error);
}

/// Port of `OCIO_ADD_TEST(MatrixOffsetOp, arbitrary)` @ v2.5.2.
#[test]
fn arbitrary() {
    let error = 1e-6f32;

    let offset = [-0.5, -0.25, 0.25, 0.1];

    let mut ops = OpVec::new();
    create_matrix_offset_op(&mut ops, &M1, &offset, TransformDirection::Forward);
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].get_info(), "<MatrixOffsetOp>");

    create_matrix_offset_op(&mut ops, &M1, &offset, TransformDirection::Inverse);
    assert_eq!(ops.len(), 2);
    ops.finalize().unwrap();

    let src: [f32; 12] = [
        0.1004, 0.201, 0.303, 0.408, //
        -0.1008, -0.207, 5.02, 0.123422, //
        1.0090, 1.009, 1.044, 1.001,
    ];

    let dst: [f32; 12] = [
        -0.09526, 0.660300, 0.70508, 1.014820, //
        0.9030888, 2.9811376, 5.7558244, 2.6944352, //
        1.52530, 3.400500, 1.90130, 2.929900,
    ];

    let mut tmp = src;

    ops[0].apply(&mut tmp).unwrap();
    check_all_safe_rel(&dst, &tmp, error);

    ops[1].apply(&mut tmp).unwrap();
    check_all_safe_rel(&src, &tmp, error);

    let op_info0 = ops[0].get_info();
    assert!(!op_info0.is_empty());

    let op_info1 = ops[1].get_info();
    assert_eq!(op_info0, op_info1);

    let cloned_op = ops[1].clone_op().unwrap();
    let cache_id = ops[1].get_cache_id().unwrap();
    let cache_id_cloned = cloned_op.get_cache_id().unwrap();

    assert!(!cache_id_cloned.is_empty());
    assert_eq!(cache_id_cloned, cache_id);
}

/// Port of `OCIO_ADD_TEST(MatrixOffsetOp, create_fit_op)` @ v2.5.2.
#[test]
fn create_fit_op_test() {
    let error = 1e-6f32;

    let oldmin4 = [0.0, 1.0, 1.0, 4.0];
    let oldmax4 = [1.0, 3.0, 4.0, 8.0];
    let newmin4 = [0.0, 2.0, 0.0, 4.0];
    let newmax4 = [1.0, 6.0, 9.0, 20.0];

    let mut ops = OpVec::new();
    create_fit_op(
        &mut ops,
        &oldmin4,
        &oldmax4,
        &newmin4,
        &newmax4,
        TransformDirection::Forward,
    )
    .unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].get_info(), "<MatrixOffsetOp>");

    create_fit_op(
        &mut ops,
        &oldmin4,
        &oldmax4,
        &newmin4,
        &newmax4,
        TransformDirection::Inverse,
    )
    .unwrap();
    assert_eq!(ops.len(), 2);
    ops.finalize().unwrap();

    let src: [f32; 12] = [
        0.1004, 0.201, 0.303, 0.408, //
        -0.10, -2.10, 0.5, 1.0, //
        42.0, 1.0, -1.11, -0.001,
    ];

    // `const double dst[]` initialized from `float` literals.
    let dst: [f64; 12] = [
        0.1004f32, 0.402, -2.091, -10.368, //
        -0.10, -4.20, -1.50, -8.0, //
        42.0, 2.0, -6.33, -12.004,
    ]
    .map(f64::from);

    let mut tmp = src;

    ops[0].apply(&mut tmp).unwrap();
    check_all_close_f64(&dst, &tmp, error);

    ops[1].apply(&mut tmp).unwrap();
    check_all_close(&src, &tmp, error);
}

/// Port of `OCIO_ADD_TEST(MatrixOffsetOp, create_saturation_op)` @ v2.5.2.
#[test]
fn create_saturation_op_test() {
    let error = 1e-6f32;
    let sat = 0.9;
    let luma_coef3 = [1.0, 0.5, 0.1];

    let mut ops = OpVec::new();
    create_saturation_op(&mut ops, sat, &luma_coef3, TransformDirection::Forward);

    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].get_info(), "<MatrixOffsetOp>");

    create_saturation_op(&mut ops, sat, &luma_coef3, TransformDirection::Inverse);
    assert_eq!(ops.len(), 2);
    ops.finalize().unwrap();

    let src: [f32; 12] = [
        0.1004, 0.201, 0.303, 0.408, //
        -0.10, -2.1, 0.5, 1.0, //
        42.0, 1.0, -1.11, -0.001,
    ];

    let dst: [f64; 12] = [
        0.11348f32, 0.20402, 0.29582, 0.408, //
        -0.2, -2.0, 0.34, 1.0, //
        42.0389, 5.1389, 3.2399, -0.001,
    ]
    .map(f64::from);

    let mut tmp = src;

    ops[0].apply(&mut tmp).unwrap();
    check_all_close_f64(&dst, &tmp, error);

    ops[1].apply(&mut tmp).unwrap();
    check_all_close(&src, &tmp, 10.0 * error);
}

/// Port of `OCIO_ADD_TEST(MatrixOffsetOp, create_min_max_op)` @ v2.5.2.
#[test]
fn create_min_max_op_test() {
    let error = 1e-6f32;

    let min3 = [1.0, 2.0, 3.0];
    let max3 = [2.0, 4.0, 6.0];

    let mut ops = OpVec::new();
    create_min_max_op(&mut ops, &min3, &max3, TransformDirection::Forward).unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].get_info(), "<MatrixOffsetOp>");
    ops.finalize().unwrap();

    let src: [f32; 20] = [
        1.0, 2.0, 3.0, 1.0, //
        1.5, 2.5, 3.15, 1.0, //
        0.0, 0.0, 0.0, 1.0, //
        3.0, 5.0, 6.3, 1.0, //
        2.0, 4.0, 6.0, 1.0,
    ];

    let dst: [f64; 20] = [
        0.0f32, 0.0, 0.0, 1.0, //
        0.5, 0.25, 0.05, 1.0, //
        -1.0, -1.0, -1.0, 1.0, //
        2.0, 1.5, 1.1, 1.0, //
        1.0, 1.0, 1.0, 1.0,
    ]
    .map(f64::from);

    let mut tmp = src;

    ops[0].apply(&mut tmp).unwrap();
    check_all_close_f64(&dst, &tmp, error);
}

/// The other matrix and offsets of upstream's `combining` test.
const M2: [f64; 16] = [
    1.1, -0.1, -0.1, 0.0, //
    0.1, 0.9, -0.2, 0.0, //
    0.05, 0.0, 1.1, 0.0, //
    0.0, 0.0, 0.0, 1.0,
];
const V1: [f64; 4] = [-0.5, -0.25, 0.25, 0.0];
const V2: [f64; 4] = [-0.2, -0.1, -0.1, -0.2];
const SOURCE: [f32; 12] = [
    0.1, 0.2, 0.3, 0.4, //
    -0.1, -0.2, 50.0, 123.4, //
    1.0, 1.0, 1.0, 1.0,
];

/// Each source pixel through `first` then `second`, and through `combined`: the same within
/// `error`, as upstream's `combining` test checks.
#[track_caller]
fn check_combination(first: &Op, second: &Op, combined: &Op, error: f32) {
    for test in 0..3 {
        let mut tmp: [f32; 4] = SOURCE[4 * test..4 * test + 4].try_into().unwrap();
        first.apply(&mut tmp).unwrap();
        second.apply(&mut tmp).unwrap();

        let mut tmp2: [f32; 4] = SOURCE[4 * test..4 * test + 4].try_into().unwrap();
        combined.apply(&mut tmp2).unwrap();

        for i in 0..4 {
            check_close(tmp2[i], tmp[i], error);
        }
    }
}

/// Port of `OCIO_ADD_TEST(MatrixOffsetOp, combining)` @ v2.5.2.
#[test]
fn combining() {
    use crate::format_metadata::{METADATA_ID, METADATA_NAME};
    use crate::open_color_types::OptimizationFlags;

    let error = 1e-4f32;

    {
        let mut ops = OpVec::new();

        let mut mat1 = MatrixOpData::new();
        mat1.set_rgba(&M1);
        mat1.set_rgba_offsets(&V1);
        let md = mat1.get_format_metadata_mut();
        md.add_attribute(Some(METADATA_NAME), Some(b"mat1"))
            .unwrap();
        md.add_attribute(Some(b"Attrib"), Some(b"1")).unwrap();
        create_matrix_op(&mut ops, mat1.clone(), TransformDirection::Forward);

        let mut mat2 = MatrixOpData::new();
        mat2.set_rgba(&M2);
        mat2.set_rgba_offsets(&V2);
        let md = mat2.get_format_metadata_mut();
        md.add_attribute(Some(METADATA_ID), Some(b"ID2")).unwrap();
        md.add_attribute(Some(b"Attrib"), Some(b"2")).unwrap();
        create_matrix_op(&mut ops, mat2.clone(), TransformDirection::Forward);
        assert_eq!(ops.len(), 2);

        ops.finalize().unwrap();

        let mut combined = OpVec::new();
        ops[0].combine_with(&mut combined, &ops[1]).unwrap();
        assert_eq!(combined.len(), 1);
        combined.finalize().unwrap();

        let combined_data = combined[0].data();

        // Check metadata of combined op.
        assert_eq!(combined_data.get_name(), b"mat1");
        assert_eq!(combined_data.get_id(), b"ID2");
        // 3 attributes: name, id, Attrib.
        let metadata = combined_data.get_format_metadata();
        assert_eq!(metadata.get_num_attributes(), 3);
        assert_eq!(metadata.get_attribute_name(1), b"Attrib");
        assert_eq!(metadata.get_attribute_value(1), b"1 + 2");

        let cache_id_combined = combined[0].get_cache_id().unwrap();
        assert!(!cache_id_combined.is_empty());

        check_combination(&ops[0], &ops[1], &combined[0], error);

        // Now try the same thing but use FinalizeOpVec to call combineWith.
        let mut ops = OpVec::new();
        create_matrix_op(&mut ops, mat1, TransformDirection::Forward);
        create_matrix_op(&mut ops, mat2, TransformDirection::Forward);
        assert_eq!(ops.len(), 2);
        let op0 = ops[0].clone();
        let op1 = ops[1].clone();

        ops.finalize().unwrap();
        ops.optimize(OptimizationFlags::DEFAULT).unwrap();
        assert_eq!(ops.len(), 1);

        let cache_id_optimized = ops[0].get_cache_id().unwrap();
        assert!(!cache_id_optimized.is_empty());

        assert_eq!(cache_id_combined, cache_id_optimized);

        check_combination(&op0, &op1, &ops[0], error);
    }

    for (dir1, dir2) in [
        (TransformDirection::Forward, TransformDirection::Inverse),
        (TransformDirection::Inverse, TransformDirection::Forward),
        (TransformDirection::Inverse, TransformDirection::Inverse),
    ] {
        let mut ops = OpVec::new();
        create_matrix_offset_op(&mut ops, &M1, &V1, dir1);
        create_matrix_offset_op(&mut ops, &M2, &V2, dir2);
        assert_eq!(ops.len(), 2);
        ops.finalize().unwrap();

        let mut combined = OpVec::new();
        ops[0].combine_with(&mut combined, &ops[1]).unwrap();
        assert_eq!(combined.len(), 1);
        combined.validate().unwrap();

        check_combination(&ops[0], &ops[1], &combined[0], error);
    }

    {
        let mut ops = OpVec::new();
        let offset = [1.1, -1.3, 0.3, 0.0];
        let offset_inv = [-1.1, 1.3, -0.3, 0.0];
        create_offset_op(&mut ops, &offset, TransformDirection::Forward);
        assert_eq!(ops.len(), 1);
        create_offset_op(&mut ops, &offset, TransformDirection::Inverse);
        assert_eq!(ops.len(), 2);
        create_offset_op(&mut ops, &offset_inv, TransformDirection::Forward);
        assert_eq!(ops.len(), 3);

        ops.finalize().unwrap();

        // Combining offset (FWD) and offset (INV) becomes an identity and is optimized out.

        let mut combined = OpVec::new();
        ops[0].combine_with(&mut combined, &ops[1]).unwrap();
        assert_eq!(combined.len(), 0);
        let mut combined = OpVec::new();

        // Combining offset (FWD) and offsetInv (FWD) becomes an identity and is optimized
        // out.

        ops[0].combine_with(&mut combined, &ops[2]).unwrap();
        assert_eq!(combined.len(), 0);
    }
}

/// Port of `OCIO_ADD_TEST(MatrixOffsetOp, throw_create)` @ v2.5.2.
#[test]
fn throw_create() {
    let mut ops = OpVec::new();

    // FitOp can't be created when old min and max are equal.
    let oldmin4 = [1.0, 0.0, 0.0, 0.0];
    let oldmax4 = [1.0, 2.0, 3.0, 4.0];
    let newmin4 = [0.0, 0.0, 0.0, 0.0];
    let newmax4 = [1.0, 4.0, 9.0, 16.0];

    check_throw_what(
        create_fit_op(
            &mut ops,
            &oldmin4,
            &oldmax4,
            &newmin4,
            &newmax4,
            TransformDirection::Forward,
        ),
        "Cannot create Fit operator. Max value equals min value",
    );
}

/// Port of `OCIO_ADD_TEST(MatrixOffsetOp, throw_validate)` @ v2.5.2.
#[test]
fn throw_validate() {
    // Matrix that can't be inverted can't be used in inverse direction.
    let mut ops = OpVec::new();
    let scale = [0.0, 1.3, 0.3, 1.0];
    create_scale_op(&mut ops, &scale, TransformDirection::Inverse);

    check_throw_what(ops[0].validate(), "Singular Matrix can't be inverted");
}

/// Port of `OCIO_ADD_TEST(MatrixOffsetOp, throw_combine)` @ v2.5.2.
#[test]
fn throw_combine() {
    let mut ops = OpVec::new();

    // Combining with different op.
    let offset = [1.1, -1.3, 0.3, 0.0];
    create_offset_op(&mut ops, &offset, TransformDirection::Forward);
    create_file_no_op(&mut ops, b"NoOp");

    assert_eq!(ops.len(), 2);

    assert!(!ops[0].can_combine_with(&ops[1]).unwrap());
    let mut combined_ops = OpVec::new();
    check_throw_what(
        ops[0].combine_with(&mut combined_ops, &ops[1]),
        "MatrixOffsetOp: canCombineWith must be checked before calling combineWith",
    );

    let scale_no_inv = [1.1, 0.0, 0.3, 0.0];
    type Create = fn(&mut OpVec, &[f64; 4], TransformDirection);
    let offset_op: Create = create_offset_op;
    let scale_op: Create = create_scale_op;
    for ((first, a, dir_a), (second, b, dir_b)) in [
        // Combining forward with inverse that can't be inverted.
        (
            (offset_op, offset, TransformDirection::Forward),
            (scale_op, scale_no_inv, TransformDirection::Inverse),
        ),
        // Combining inverse that can't be inverted with forward.
        (
            (scale_op, scale_no_inv, TransformDirection::Inverse),
            (offset_op, offset, TransformDirection::Forward),
        ),
        // Combining inverse with inverse that can't be inverted.
        (
            (offset_op, offset, TransformDirection::Inverse),
            (scale_op, scale_no_inv, TransformDirection::Inverse),
        ),
        // Combining inverse that can't be inverted with inverse.
        (
            (scale_op, scale_no_inv, TransformDirection::Inverse),
            (offset_op, offset, TransformDirection::Inverse),
        ),
    ] {
        let mut ops = OpVec::new();
        first(&mut ops, &a, dir_a);
        second(&mut ops, &b, dir_b);
        assert_eq!(ops.len(), 2);

        check_throw_what(
            ops[0].can_combine_with(&ops[1]),
            "Op::finalize has to be called",
        );
    }
}

/// Port of `OCIO_ADD_TEST(MatrixOffsetOp, no_op)` @ v2.5.2.
#[test]
fn no_op() {
    use crate::open_color_types::OptimizationFlags;

    /// Each factory call, then `finalize` and the default optimization: no op is left.
    fn leaves_nothing(create: impl Fn(&mut OpVec)) {
        let mut ops = OpVec::new();
        create(&mut ops);
        assert_eq!(ops.len(), 1);
        ops.finalize().unwrap();
        ops.optimize(OptimizationFlags::DEFAULT).unwrap();
        assert_eq!(ops.len(), 0);
    }

    let offset = [0.0, 0.0, 0.0, 0.0];
    let scale = [1.0, 1.0, 1.0, 1.0];
    let matrix = [
        1.0, 0.0, 0.0, 0.0, //
        0.0, 1.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ];
    let oldmin4 = [0.0, 0.0, 0.0, 0.0];
    let oldmax4 = [1.0, 2.0, 3.0, 4.0];
    let sat = 1.0;
    let luma_coef3 = [1.0, 1.0, 1.0];

    for dir in [TransformDirection::Forward, TransformDirection::Inverse] {
        leaves_nothing(|ops| create_offset_op(ops, &offset, dir));
        leaves_nothing(|ops| create_scale_op(ops, &scale, dir));
        leaves_nothing(|ops| create_matrix_op_from_m44(ops, &matrix, dir));
        leaves_nothing(|ops| create_matrix_offset_op(ops, &matrix, &offset, dir));
        leaves_nothing(|ops| {
            create_fit_op(ops, &oldmin4, &oldmax4, &oldmin4, &oldmax4, dir).unwrap();
        });
        leaves_nothing(|ops| create_saturation_op(ops, sat, &luma_coef3, dir));
    }

    let mut ops = OpVec::new();
    create_identity_matrix_op(&mut ops);
    assert_eq!(ops.len(), 1);
    assert!(ops[0].is_no_op().unwrap());
    ops[0].validate().unwrap();
    ops[0].finalize().unwrap();
    assert!(ops[0].is_no_op().unwrap());
}

/// Port of `OCIO_ADD_TEST(MatrixOffsetOp, is_same_type)` @ v2.5.2.
#[test]
fn is_same_type() {
    let sat = 0.9;
    let luma_coef3 = [1.0, 0.5, 0.1];
    let scale = [1.1, 1.3, 0.3, 1.0];
    let base = 10.0;
    let log_slope = [0.18, 0.5, 0.3];
    let lin_slope = [2.0, 4.0, 8.0];
    let lin_offset = [0.1, 0.1, 0.1];
    let log_offset = [1.0, 1.0, 1.0];

    // Create saturation, scale and log.
    let mut ops = OpVec::new();
    create_saturation_op(&mut ops, sat, &luma_coef3, TransformDirection::Forward);
    assert_eq!(ops.len(), 1);
    create_scale_op(&mut ops, &scale, TransformDirection::Forward);
    assert_eq!(ops.len(), 2);
    create_log_op_from_parameters(
        &mut ops,
        base,
        &log_slope,
        &log_offset,
        &lin_slope,
        &lin_offset,
        TransformDirection::Forward,
    );
    assert_eq!(ops.len(), 3);
    let op0 = &ops[0];
    let op1 = &ops[1];
    let op2 = &ops[2];

    // saturation and scale are MatrixOffset operators, log is not.
    assert!(ops[0].is_same_type(op1));
    assert!(ops[1].is_same_type(op0));
    assert!(!ops[0].is_same_type(op2));
    assert!(!ops[2].is_same_type(op0));
    assert!(!ops[1].is_same_type(op2));
    assert!(!ops[2].is_same_type(op1));
}

/// Port of `OCIO_ADD_TEST(MatrixOffsetOp, has_channel_crosstalk)` @ v2.5.2.
#[test]
fn has_channel_crosstalk() {
    let scale = [1.1, 1.3, 0.3, 1.0];
    let sat = 0.9;
    let luma_coef3 = [1.0, 0.5, 0.1];

    let mut ops = OpVec::new();
    create_scale_op(&mut ops, &scale, TransformDirection::Forward);
    assert_eq!(ops.len(), 1);
    ops[0].validate().unwrap();
    create_saturation_op(&mut ops, sat, &luma_coef3, TransformDirection::Forward);
    assert_eq!(ops.len(), 2);
    ops[1].validate().unwrap();

    assert!(!ops[0].has_channel_crosstalk());
    assert!(ops[1].has_channel_crosstalk());
}

/// `CreateIdentityMatrixOp(ops, direction)` makes an identity Matrix op in the direction it is
/// given (src/OpenColorIO/ops/matrix/MatrixOp.cpp:282-294 @ v2.5.2).
///
/// Hand-derived: nothing upstream calls this overload, neither the library nor its tests, so
/// no output of the wheel shows it and no upstream test checks it. The expected values are what
/// the code above says: the direction passed, and an identity matrix, which `isNoOp` finds.
#[test]
fn the_identity_op_in_a_direction() {
    let mut ops = OpVec::new();
    create_identity_matrix_op_with_direction(&mut ops, TransformDirection::Inverse);
    assert_eq!(ops.len(), 1);
    assert_eq!(matrix(&ops[0]).get_direction(), TransformDirection::Inverse);
    assert!(ops[0].is_no_op().unwrap());
}
