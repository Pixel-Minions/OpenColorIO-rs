// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the range transform: `tests/cpu/transforms/RangeTransform_tests.cpp` @ v2.5.2, and
//! `RangeOp create_transform` and `RangeTransform no_clamp_converts_to_matrix`
//! (tests/cpu/ops/range/RangeOp_tests.cpp @ v2.5.2), which test `CreateRangeTransform` and
//! `BuildRangeOp` and needed the transform. The text, the validation, the equality and the ops
//! built are compared with the wheel's in `tests/range_transform_oracle.rs`.

use ocio_ops::op_data::{OpData, OpDataType};
use ocio_ops::open_color_types::{BitDepth, TransformDirection};
use ocio_ops::ops::matrix::MatrixOpData;
use ocio_ops::ops::range::range_op::create_range_op;

use super::*;
use crate::transform::Transform;

/// Port of `OCIO_ADD_TEST(RangeTransform, basic)` @ v2.5.2.
#[test]
fn basic() {
    let mut range = RangeTransform::new();
    assert_eq!(range.direction(), TransformDirection::Forward);
    assert_eq!(range.style(), RangeStyle::Clamp);
    assert!(!range.has_min_in_value());
    assert!(!range.has_max_in_value());
    assert!(!range.has_min_out_value());
    assert!(!range.has_max_out_value());

    range.set_direction(TransformDirection::Inverse);
    assert_eq!(range.direction(), TransformDirection::Inverse);

    range.set_style(RangeStyle::NoClamp);
    assert_eq!(range.style(), RangeStyle::NoClamp);

    range.set_min_in_value(-0.5);
    assert_eq!(range.min_in_value(), -0.5);
    assert!(range.has_min_in_value());

    let mut range2 = RangeTransform::new();
    range2.set_direction(TransformDirection::Inverse);
    range2.set_min_in_value(-0.5);
    range2.set_style(RangeStyle::NoClamp);
    assert!(range2.equals(&range));

    range2.set_direction(TransformDirection::Forward);
    range2.set_min_in_value(-1.5);
    range2.set_max_in_value(-0.5);
    range2.set_min_out_value(1.5);
    range2.set_max_out_value(4.5);

    assert_eq!(range2.file_input_bit_depth(), BitDepth::Unknown);
    assert_eq!(range2.file_output_bit_depth(), BitDepth::Unknown);

    range2.set_file_input_bit_depth(BitDepth::Uint8);
    range2.set_file_output_bit_depth(BitDepth::Uint10);

    assert_eq!(range2.file_input_bit_depth(), BitDepth::Uint8);
    assert_eq!(range2.file_output_bit_depth(), BitDepth::Uint10);

    assert_eq!(range2.min_in_value(), -1.5);
    assert_eq!(range2.max_in_value(), -0.5);
    assert_eq!(range2.min_out_value(), 1.5);
    assert_eq!(range2.max_out_value(), 4.5);

    range2.unset_min_in_value();

    // (Note that the transform would not validate at this point.)

    assert!(!range2.has_min_in_value());
    assert_eq!(range2.max_in_value(), -0.5);
    assert_eq!(range2.min_out_value(), 1.5);
    assert_eq!(range2.max_out_value(), 4.5);

    range2.set_min_in_value(f64::from(-1.5f32));
    assert_eq!(range2.min_in_value(), -1.5);
    assert_eq!(range2.max_in_value(), -0.5);
    assert_eq!(range2.min_out_value(), 1.5);
    assert_eq!(range2.max_out_value(), 4.5);

    assert!(range2.has_min_in_value());
    assert!(range2.has_max_in_value());
    assert!(range2.has_min_out_value());
    assert!(range2.has_max_out_value());

    range2.unset_min_in_value();
    assert!(!range2.has_min_in_value());
    assert!(range2.has_max_in_value());
    assert!(range2.has_min_out_value());
    assert!(range2.has_max_out_value());

    range2.unset_max_in_value();
    assert!(!range2.has_min_in_value());
    assert!(!range2.has_max_in_value());
    assert!(range2.has_min_out_value());
    assert!(range2.has_max_out_value());

    range2.unset_min_out_value();
    assert!(!range2.has_min_in_value());
    assert!(!range2.has_max_in_value());
    assert!(!range2.has_min_out_value());
    assert!(range2.has_max_out_value());

    range2.unset_max_out_value();
    assert!(!range2.has_min_in_value());
    assert!(!range2.has_max_in_value());
    assert!(!range2.has_min_out_value());
    assert!(!range2.has_max_out_value());
}

/// Port of `OCIO_ADD_TEST(RangeOp, create_transform)` @ v2.5.2.
#[test]
fn create_transform() {
    let direction = TransformDirection::Inverse;

    let mut range = RangeOpData::with_direction(0.1, 0.9, 0.2, 0.7, direction).unwrap();

    range
        .get_format_metadata_mut()
        .add_attribute(Some(b"name"), Some(b"test"))
        .unwrap();

    range.set_file_input_bit_depth(BitDepth::Uint10);
    range.set_file_output_bit_depth(BitDepth::Uint8);

    let mut ops = OpVec::new();
    create_range_op(&mut ops, range, TransformDirection::Forward).unwrap();
    assert_eq!(ops.len(), 1);

    let mut group = GroupTransform::new();

    let op = &ops[0];

    create_range_transform(&mut group, op).unwrap();
    assert_eq!(group.num_transforms(), 1);
    let transform = group.transform(0).unwrap();
    let Transform::Range(r_transform) = transform else {
        panic!("not a RangeTransform: {transform:?}");
    };
    assert_eq!(r_transform.file_input_bit_depth(), BitDepth::Uint10);
    assert_eq!(r_transform.file_output_bit_depth(), BitDepth::Uint8);

    let metadata = r_transform.format_metadata();
    assert_eq!(metadata.get_num_attributes(), 1);
    assert_eq!(metadata.get_attribute_name(0), b"name");
    assert_eq!(metadata.get_attribute_value(0), b"test");

    assert_eq!(r_transform.direction(), direction);

    assert_eq!(0.1, r_transform.min_in_value());
    assert_eq!(0.9, r_transform.max_in_value());
    assert_eq!(0.2, r_transform.min_out_value());
    assert_eq!(0.7, r_transform.max_out_value());
}

/// The range data of a Range op.
fn range_data(op: &Op) -> &RangeOpData {
    match &**op.data() {
        OpData::Range(data) => data,
        other => panic!("not a Range op: {other:?}"),
    }
}

/// The matrix data of a Matrix op.
fn matrix_data(op: &Op) -> &MatrixOpData {
    match &**op.data() {
        OpData::Matrix(data) => data,
        other => panic!("not a Matrix op: {other:?}"),
    }
}

/// Port of `OCIO_ADD_TEST(RangeTransform, no_clamp_converts_to_matrix)` @ v2.5.2.
#[test]
fn no_clamp_converts_to_matrix() {
    let mut ops = OpVec::new();

    let mut range = RangeTransform::new();
    assert_eq!(range.direction(), TransformDirection::Forward);
    range.set_max_in_value(1.);
    range.set_max_out_value(1.);
    assert_eq!(range.style(), RangeStyle::Clamp);
    assert!(!range.has_min_in_value());
    assert!(range.has_max_in_value());
    assert!(!range.has_min_out_value());
    assert!(range.has_max_out_value());

    build_range_op(&mut ops, &range, TransformDirection::Forward).unwrap();
    assert_eq!(ops.len(), 1);
    let op0 = &ops[0];
    assert_eq!(op0.data().get_type(), OpDataType::Range);
    assert!(!op0.is_no_op().unwrap());
    ops.clear();

    range.set_min_in_value(0.0);
    range.set_max_in_value(0.5);
    range.set_min_out_value(0.5);
    range.set_max_out_value(1.5);

    // Test the resulting Range Op

    build_range_op(&mut ops, &range, TransformDirection::Forward).unwrap();

    assert_eq!(ops.len(), 1);
    let op0 = ops[0].clone_op();
    assert_eq!(op0.data().get_type(), OpDataType::Range);

    let range_data = range_data(&op0);

    assert_eq!(range_data.get_min_in_value(), range.min_in_value());
    assert_eq!(range_data.get_max_in_value(), range.max_in_value());
    assert_eq!(range_data.get_min_out_value(), range.min_out_value());
    assert_eq!(range_data.get_max_out_value(), range.max_out_value());

    // Test the resulting Matrix Op

    range.set_style(RangeStyle::NoClamp);

    build_range_op(&mut ops, &range, TransformDirection::Forward).unwrap();

    assert_eq!(ops.len(), 2);
    let op1 = &ops[1];
    assert_eq!(op1.data().get_type(), OpDataType::Matrix);

    let matrix = matrix_data(op1);

    assert_eq!(matrix.get_offset_value(0).unwrap(), range_data.get_offset());
    assert_eq!(matrix.get_direction(), TransformDirection::Forward);

    assert_eq!(matrix.get_offset_value(0).unwrap(), 0.5);
    assert_eq!(matrix.get_offset_value(1).unwrap(), 0.5);
    assert_eq!(matrix.get_offset_value(2).unwrap(), 0.5);
    assert_eq!(matrix.get_offset_value(3).unwrap(), 0.0);

    assert!(matrix.is_diagonal());

    assert_eq!(matrix.get_array()[0], range_data.get_scale());

    assert_eq!(matrix.get_array()[0], 2.0);
    assert_eq!(matrix.get_array()[5], 2.0);
    assert_eq!(matrix.get_array()[10], 2.0);
    assert_eq!(matrix.get_array()[15], 1.0);

    // Range is forward, build an inverse.
    build_range_op(&mut ops, &range, TransformDirection::Inverse).unwrap();

    assert_eq!(ops.len(), 3);
    let op2 = &ops[2];
    assert_eq!(op2.data().get_type(), OpDataType::Matrix);

    let matrix = matrix_data(op2);
    assert_eq!(matrix.get_direction(), TransformDirection::Inverse);

    assert_eq!(matrix.get_offset_value(0).unwrap(), 0.5);
    assert_eq!(matrix.get_offset_value(1).unwrap(), 0.5);
    assert_eq!(matrix.get_offset_value(2).unwrap(), 0.5);
    assert_eq!(matrix.get_offset_value(3).unwrap(), 0.0);

    assert!(matrix.is_diagonal());

    assert_eq!(matrix.get_array()[0], 2.0);
    assert_eq!(matrix.get_array()[5], 2.0);
    assert_eq!(matrix.get_array()[10], 2.0);
    assert_eq!(matrix.get_array()[15], 1.0);

    // Range is inverse, build a forward.
    range.set_direction(TransformDirection::Inverse);
    build_range_op(&mut ops, &range, TransformDirection::Forward).unwrap();

    assert_eq!(ops.len(), 4);
    let op3 = &ops[3];
    assert_eq!(op3.data().get_type(), OpDataType::Matrix);

    let matrix = matrix_data(op3);
    assert_eq!(matrix.get_direction(), TransformDirection::Forward);

    assert_eq!(matrix.get_offset_value(0).unwrap(), -0.25);
    assert_eq!(matrix.get_offset_value(1).unwrap(), -0.25);
    assert_eq!(matrix.get_offset_value(2).unwrap(), -0.25);
    assert_eq!(matrix.get_offset_value(3).unwrap(), 0.0);

    assert!(matrix.is_diagonal());

    assert_eq!(matrix.get_array()[0], 1.0 / 2.0);
    assert_eq!(matrix.get_array()[5], 1.0 / 2.0);
    assert_eq!(matrix.get_array()[10], 1.0 / 2.0);
    assert_eq!(matrix.get_array()[15], 1.0);
}

/// `CreateRangeTransform` refuses an op of another type (which `CreateTransform`'s dispatch
/// never passes it) and adds nothing.
#[test]
fn create_range_transform_needs_a_range_op() {
    let mut ops = OpVec::new();
    ocio_ops::ops::noop::create_file_no_op(&mut ops, b"f");
    let mut group = GroupTransform::new();
    assert!(create_range_transform(&mut group, &ops[0]).is_err());
    assert_eq!(group.num_transforms(), 0);
}

/// `BuildRangeOp` validates the data itself: a range without bounds adds no op, with the
/// data's error, the one `validate` reports after its prefix.
#[test]
fn build_range_op_validates_the_data() {
    let range = RangeTransform::new();
    let validated = range.validate().unwrap_err();

    let mut ops = OpVec::new();
    let built = build_range_op(&mut ops, &range, TransformDirection::Forward).unwrap_err();
    assert_eq!(
        format!("RangeTransform validation failed: {}", built.message()),
        validated.message()
    );
    assert_eq!(ops.len(), 0);
}
