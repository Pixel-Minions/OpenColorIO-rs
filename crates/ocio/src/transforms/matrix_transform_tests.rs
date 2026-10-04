// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the matrix transform: `tests/cpu/transforms/MatrixTransform_tests.cpp` @ v2.5.2,
//! and `MatrixOffsetOp create_transform` (tests/cpu/ops/matrix/MatrixOp_tests.cpp @ v2.5.2),
//! which tests `CreateMatrixTransform` and `BuildMatrixOp` and needed the transform. The text,
//! the validation, the equality, the static functions and the ops built are compared with the
//! wheel's in `tests/transform_oracle.rs`.

use ocio_ops::op_data::OpData;
use ocio_ops::open_color_types::{BitDepth, TransformDirection};
use ocio_ops::ops::matrix::matrix_op::create_matrix_op;

use super::*;
use crate::transform::Transform;

/// Port of `OCIO_ADD_TEST(MatrixTransform, basic)` @ v2.5.2.
#[test]
fn basic() {
    let mut matrix = MatrixTransform::new();
    assert_eq!(matrix.direction(), TransformDirection::Forward);

    let mut m44 = matrix.matrix();
    let mut offset4 = matrix.offset();

    assert_eq!(m44[0], 1.0);
    assert_eq!(m44[1], 0.0);
    assert_eq!(m44[2], 0.0);
    assert_eq!(m44[3], 0.0);

    assert_eq!(m44[4], 0.0);
    assert_eq!(m44[5], 1.0);
    assert_eq!(m44[6], 0.0);
    assert_eq!(m44[7], 0.0);

    assert_eq!(m44[8], 0.0);
    assert_eq!(m44[9], 0.0);
    assert_eq!(m44[10], 1.0);
    assert_eq!(m44[11], 0.0);

    assert_eq!(m44[12], 0.0);
    assert_eq!(m44[13], 0.0);
    assert_eq!(m44[14], 0.0);
    assert_eq!(m44[15], 1.0);

    assert_eq!(offset4[0], 0.0);
    assert_eq!(offset4[1], 0.0);
    assert_eq!(offset4[2], 0.0);
    assert_eq!(offset4[3], 0.0);

    m44[0] = 1.0;
    m44[1] = 1.01;
    m44[2] = 1.02;
    m44[3] = 1.03;

    m44[4] = 1.04;
    m44[5] = 1.05;
    m44[6] = 1.06;
    m44[7] = 1.07;

    m44[8] = 1.08;
    m44[9] = 1.09;
    m44[10] = 1.10;
    m44[11] = 1.11;

    m44[12] = 1.12;
    m44[13] = 1.13;
    m44[14] = 1.14;
    m44[15] = 1.15;

    offset4[0] = 1.0;
    offset4[1] = 1.1;
    offset4[2] = 1.2;
    offset4[3] = 1.3;

    matrix.set_matrix(&m44);
    matrix.set_offset(&offset4);

    let m44r = matrix.matrix();
    let offset4r = matrix.offset();

    for i in 0..16 {
        assert_eq!(m44r[i], m44[i]);
    }

    assert_eq!(offset4r[0], 1.0);
    assert_eq!(offset4r[1], 1.1);
    assert_eq!(offset4r[2], 1.2);
    assert_eq!(offset4r[3], 1.3);

    assert_eq!(matrix.file_input_bit_depth(), BitDepth::Unknown);
    assert_eq!(matrix.file_output_bit_depth(), BitDepth::Unknown);

    matrix.set_file_input_bit_depth(BitDepth::Uint8);
    matrix.set_file_output_bit_depth(BitDepth::Uint10);

    assert_eq!(matrix.file_input_bit_depth(), BitDepth::Uint8);
    assert_eq!(matrix.file_output_bit_depth(), BitDepth::Uint10);

    // File bit-depth does not affect values.
    let m44r = matrix.matrix();
    let offset4r = matrix.offset();

    for i in 0..16 {
        assert_eq!(m44r[i], m44[i]);
    }

    assert_eq!(offset4r[0], 1.0);
    assert_eq!(offset4r[1], 1.1);
    assert_eq!(offset4r[2], 1.2);
    assert_eq!(offset4r[3], 1.3);

    assert_eq!(matrix.direction(), TransformDirection::Forward);
    matrix.set_direction(TransformDirection::Inverse);
    assert_eq!(matrix.direction(), TransformDirection::Inverse);

    assert_eq!(matrix.file_input_bit_depth(), BitDepth::Uint8);
    assert_eq!(matrix.file_output_bit_depth(), BitDepth::Uint10);
}

/// Port of `OCIO_ADD_TEST(MatrixTransform, equals)` @ v2.5.2.
#[test]
fn equals() {
    let mut matrix1 = MatrixTransform::new();
    let matrix2 = MatrixTransform::new();

    assert!(matrix1.equals(&matrix2));

    matrix1.set_direction(TransformDirection::Inverse);
    assert!(!matrix1.equals(&matrix2));
    matrix1.set_direction(TransformDirection::Forward);

    let mut m44 = matrix1.matrix();
    let mut offset4 = matrix1.offset();
    m44[0] = 1.0 + 1e-6;
    matrix1.set_matrix(&m44);
    assert!(!matrix1.equals(&matrix2));
    m44[0] = 1.0;
    matrix1.set_matrix(&m44);
    assert!(matrix1.equals(&matrix2));

    offset4[0] = 1e-6;
    matrix1.set_offset(&offset4);
    assert!(!matrix1.equals(&matrix2));
}

/// The matrix data of a Matrix op.
fn matrix_data(op: &Op) -> &MatrixOpData {
    match &**op.data() {
        OpData::Matrix(data) => data,
        other => panic!("not a Matrix op: {other:?}"),
    }
}

/// Port of `OCIO_ADD_TEST(MatrixOffsetOp, create_transform)` @ v2.5.2.
#[test]
fn create_transform() {
    let mut mat = MatrixOpData::new();
    mat.get_format_metadata_mut()
        .add_attribute(Some(b"name"), Some(b"test"))
        .unwrap();

    let offset: [f64; 4] = [1., 2., 3., 4.];
    mat.get_offsets_mut().set_rgba(&offset);
    mat.get_array_mut().get_values_mut().copy_from_slice(&[
        1.1, 0.2, 0.3, 0.4, //
        0.5, 1.6, 0.7, 0.8, //
        0.2, 0.1, 1.1, 0.2, //
        0.3, 0.4, 0.5, 1.6,
    ]);

    let mut ops = OpVec::new();
    let direction = TransformDirection::Forward;
    create_matrix_op(&mut ops, mat.clone(), direction);
    assert_eq!(ops.len(), 1);

    let mut group = GroupTransform::new();

    let op = &ops[0];

    create_matrix_transform(&mut group, op).unwrap();
    assert_eq!(group.num_transforms(), 1);
    let transform = group.transform(0).unwrap();
    let Transform::Matrix(m_transform) = transform else {
        panic!("not a MatrixTransform: {transform:?}");
    };

    let metadata = m_transform.format_metadata();
    assert_eq!(metadata.get_num_attributes(), 1);
    assert_eq!(metadata.get_attribute_name(0), b"name");
    assert_eq!(metadata.get_attribute_value(0), b"test");

    assert_eq!(m_transform.direction(), direction);
    let oval = m_transform.offset();
    assert_eq!(oval[0], offset[0]);
    assert_eq!(oval[1], offset[1]);
    assert_eq!(oval[2], offset[2]);
    assert_eq!(oval[3], offset[3]);
    let mval = m_transform.matrix();
    for (i, value) in mval.iter().enumerate() {
        assert_eq!(*value, mat.get_array()[i]);
    }

    let mut ops_back = OpVec::new();
    build_matrix_op(&mut ops_back, m_transform, TransformDirection::Forward).unwrap();
    build_matrix_op(&mut ops_back, m_transform, TransformDirection::Inverse).unwrap();
    assert_eq!(ops_back.len(), 2);

    let m0 = matrix_data(&ops_back[0]);
    let m1 = matrix_data(&ops_back[1]);
    assert_eq!(m0.get_direction(), TransformDirection::Forward);
    assert_eq!(m1.get_direction(), TransformDirection::Inverse);
    assert!(m0.get_array().equals(mat.get_array()));
    assert!(m1.get_array().equals(mat.get_array()));
    assert!(m0.get_offsets() == mat.get_offsets());
    assert!(m1.get_offsets() == mat.get_offsets());
}

/// `CreateMatrixTransform` refuses an op of another type (which `CreateTransform`'s dispatch
/// never passes it) and adds nothing.
#[test]
fn create_matrix_transform_needs_a_matrix_op() {
    let mut ops = OpVec::new();
    ocio_ops::ops::noop::create_file_no_op(&mut ops, b"f");
    let mut group = GroupTransform::new();
    assert!(create_matrix_transform(&mut group, &ops[0]).is_err());
    assert_eq!(group.num_transforms(), 0);
}

/// `BuildMatrixOp` validates the data itself: an inverse matrix that can't be inverted adds no
/// op, with the data's error, the one `validate` reports after its prefix.
#[test]
fn build_matrix_op_validates_the_data() {
    let mut matrix = MatrixTransform::new();
    matrix.set_matrix(&[0.0; 16]);
    matrix.set_direction(TransformDirection::Inverse);
    let validated = matrix.validate().unwrap_err();

    let mut ops = OpVec::new();
    let built = build_matrix_op(&mut ops, &matrix, TransformDirection::Forward).unwrap_err();
    assert_eq!(
        format!("MatrixTransform validation failed: {}", built.message()),
        validated.message()
    );
    assert_eq!(ops.len(), 0);
}
