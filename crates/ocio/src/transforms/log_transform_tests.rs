// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the log transform: `tests/cpu/transforms/LogTransform_tests.cpp` @ v2.5.2, and `LogOp
//! create_transform` (tests/cpu/ops/log/LogOp_tests.cpp @ v2.5.2), which tests
//! `CreateLogTransform` and needed the transforms. The text, the validation, the equality and the
//! ops built are compared with the wheel's in `tests/log_transform_oracle.rs`.

use crate::transform::Transform;

use super::*;

/// Port of `OCIO_ADD_TEST(LogTransform, basic)` @ v2.5.2.
#[test]
fn basic() {
    let mut log = LogTransform::new();

    assert_eq!(log.base(), f64::from(2.0f32));
    assert_eq!(log.direction(), TransformDirection::Forward);

    log.set_direction(TransformDirection::Inverse);
    assert_eq!(log.direction(), TransformDirection::Inverse);

    log.set_base(f64::from(10.0f32));
    assert_eq!(log.base(), f64::from(10.0f32));

    let mut ops = OpVec::new();

    // Convert to op.
    build_log_op(&mut ops, log.data(), TransformDirection::Forward).unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].get_info(), "<LogOp>");

    // Convert back to transform.
    let mut group = GroupTransform::new();
    create_log_transform(&mut group, &ops[0]).unwrap();

    assert_eq!(group.num_transforms(), 1);
    let transform = group.transform(0).unwrap();
    assert!(matches!(transform, Transform::Log(_)), "{transform:?}");
}

/// Port of `OCIO_ADD_TEST(LogOp, create_transform)` @ v2.5.2. Upstream's forward Log op shares
/// the caller's data, so after each `setValue` its test reads the changed data back through the
/// first op; the port's ops own their data (`Op`'s docs), so the test reads it through the op
/// built after the change, which holds the same data.
#[test]
fn create_transform() {
    let direction = TransformDirection::Forward;

    let base = 1.0;
    let log_slope = [1.5, 1.6, 1.7];
    let lin_slope = [1.1, 1.2, 1.3];
    let lin_offset = [1.0, 2.0, 3.0];
    let log_offset = [10.0, 20.0, 30.0];

    let mut log = LogOpData::with_parameters(
        base,
        &log_slope,
        &log_offset,
        &lin_slope,
        &lin_offset,
        direction,
    );

    let metadata_source = log.get_format_metadata_mut();
    metadata_source
        .add_attribute(Some(b"name"), Some(b"test"))
        .unwrap();

    let mut ops = OpVec::new();
    create_log_op(&mut ops, log.clone(), direction).unwrap();
    assert_eq!(ops.len(), 1);

    let mut group = GroupTransform::new();

    create_log_transform(&mut group, &ops[0]).unwrap();
    assert_eq!(group.num_transforms(), 1);
    let transform = group.transform(0).unwrap();
    let Transform::LogAffine(l_transform) = transform else {
        panic!("not a LogAffineTransform: {transform:?}");
    };

    let metadata = l_transform.format_metadata();
    assert_eq!(metadata.get_num_attributes(), 1);
    assert_eq!(metadata.get_attribute_name(0), b"name");
    assert_eq!(metadata.get_attribute_value(0), b"test");

    assert_eq!(l_transform.direction(), direction);
    assert_eq!(l_transform.base(), base);
    let values = l_transform.log_side_slope_value();
    assert_eq!(values[0], log_slope[0]);
    assert_eq!(values[1], log_slope[1]);
    assert_eq!(values[2], log_slope[2]);
    let values = l_transform.log_side_offset_value();
    assert_eq!(values[0], log_offset[0]);
    assert_eq!(values[1], log_offset[1]);
    assert_eq!(values[2], log_offset[2]);
    let values = l_transform.lin_side_slope_value();
    assert_eq!(values[0], lin_slope[0]);
    assert_eq!(values[1], lin_slope[1]);
    assert_eq!(values[2], lin_slope[2]);
    let values = l_transform.lin_side_offset_value();
    assert_eq!(values[0], lin_offset[0]);
    assert_eq!(values[1], lin_offset[1]);
    assert_eq!(values[2], lin_offset[2]);

    let lin_break = [0.5, 0.4, 0.3];
    log.set_value(LogAffineParameter::LinSideBreak, &lin_break)
        .unwrap();

    create_log_op(&mut ops, log.clone(), direction).unwrap();
    assert_eq!(ops.len(), 2);

    let mut group1 = GroupTransform::new();

    create_log_transform(&mut group1, &ops[1]).unwrap();
    assert_eq!(group1.num_transforms(), 1);
    let transform1 = group1.transform(0).unwrap();
    let Transform::LogCamera(l_transform1) = transform1 else {
        panic!("not a LogCameraTransform: {transform1:?}");
    };
    let values = l_transform1.lin_side_break_value();
    assert_eq!(values[0], lin_break[0]);
    assert_eq!(values[1], lin_break[1]);
    assert_eq!(values[2], lin_break[2]);
    assert!(l_transform1.linear_slope_value().is_none());

    let linear_slope = [0.9, 1.0, 1.1];
    log.set_value(LogAffineParameter::LinearSlope, &linear_slope)
        .unwrap();

    create_log_op(&mut ops, log, direction).unwrap();
    assert_eq!(ops.len(), 3);

    let mut group2 = GroupTransform::new();

    create_log_transform(&mut group2, &ops[2]).unwrap();
    assert_eq!(group2.num_transforms(), 1);
    let transform2 = group2.transform(0).unwrap();
    let Transform::LogCamera(l_transform2) = transform2 else {
        panic!("not a LogCameraTransform: {transform2:?}");
    };
    let values = l_transform1.lin_side_break_value();
    assert_eq!(values[0], lin_break[0]);
    assert_eq!(values[1], lin_break[1]);
    assert_eq!(values[2], lin_break[2]);
    let values = l_transform2.linear_slope_value().unwrap();
    assert_eq!(values[0], linear_slope[0]);
    assert_eq!(values[1], linear_slope[1]);
    assert_eq!(values[2], linear_slope[2]);
}

/// `CreateLogTransform` refuses an op of another type (which `CreateTransform`'s dispatch never
/// passes it) and adds nothing.
#[test]
fn create_log_transform_needs_a_log_op() {
    let mut ops = OpVec::new();
    ocio_ops::ops::noop::create_file_no_op(&mut ops, b"f");
    let mut group = GroupTransform::new();
    assert!(create_log_transform(&mut group, &ops[0]).is_err());
    assert_eq!(group.num_transforms(), 0);
}

/// `BuildLogOp` validates the data itself: a base of 1 adds no op, with the data's error, the one
/// `validate` reports after its prefix.
#[test]
fn build_log_op_validates_the_data() {
    let mut log = LogTransform::new();
    log.set_base(1.0);
    let validated = log.validate().unwrap_err();

    let mut ops = OpVec::new();
    let built = build_log_op(&mut ops, log.data(), TransformDirection::Forward).unwrap_err();
    assert_eq!(
        format!("LogTransform validation failed: {}", built.message()),
        validated.message()
    );
    assert_eq!(ops.len(), 0);
}
