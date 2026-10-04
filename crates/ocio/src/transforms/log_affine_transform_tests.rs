// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the log affine transform: `tests/cpu/transforms/LogAffineTransform_tests.cpp` @
//! v2.5.2. The text, the validation, the equality and the ops built are compared with the
//! wheel's in `tests/log_transform_oracle.rs`.

use ocio_ops::op::OpVec;

use super::*;
use crate::transform::Transform;
use crate::transforms::group_transform::GroupTransform;
use crate::transforms::log_transform::{build_log_op, create_log_transform};

/// `AllEqual` (LogAffineTransform_tests.cpp:15-18 @ v2.5.2).
fn all_equal(values: &[f64; 3]) -> bool {
    values[0] == values[1] && values[0] == values[2]
}

/// Port of `OCIO_ADD_TEST(LogAffineTransform, basic)` @ v2.5.2.
#[test]
fn basic() {
    let mut log = LogAffineTransform::new();

    let base = log.base();
    assert_eq!(base, 2.0);
    let values = log.lin_side_offset_value();
    assert!(all_equal(&values));
    assert_eq!(values[0], 0.0);
    let values = log.lin_side_slope_value();
    assert!(all_equal(&values));
    assert_eq!(values[0], 1.0);
    let values = log.log_side_offset_value();
    assert!(all_equal(&values));
    assert_eq!(values[0], 0.0);
    let values = log.log_side_slope_value();
    assert!(all_equal(&values));
    assert_eq!(values[0], 1.0);
    assert_eq!(log.direction(), TransformDirection::Forward);

    let mut ops = OpVec::new();

    // Convert to op.
    build_log_op(&mut ops, log.data(), TransformDirection::Forward).unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].get_info(), "<LogOp>");

    let mut group = GroupTransform::new();
    // Convert back to transform.
    create_log_transform(&mut group, &ops[0]).unwrap();

    assert_eq!(group.num_transforms(), 1);
    let transform = group.transform(0).unwrap();
    // Affine parameters are identity, so it comes back as a simple log.
    assert!(matches!(transform, Transform::Log(_)), "{transform:?}");

    log.set_direction(TransformDirection::Inverse);
    assert_eq!(log.direction(), TransformDirection::Inverse);

    log.set_base(3.0);
    log.base();
    assert_eq!(log.base(), 3.0);

    log.set_lin_side_offset_value(&[0.1, 0.2, 0.3]);
    let values = log.lin_side_offset_value();
    assert_eq!(values[0], 0.1);
    assert_eq!(values[1], 0.2);
    assert_eq!(values[2], 0.3);

    log.set_lin_side_slope_value(&[1.1, 1.2, 1.3]);
    let values = log.lin_side_slope_value();
    assert_eq!(values[0], 1.1);
    assert_eq!(values[1], 1.2);
    assert_eq!(values[2], 1.3);

    log.set_log_side_offset_value(&[0.4, 0.5, 0.6]);
    let values = log.log_side_offset_value();
    assert_eq!(values[0], 0.4);
    assert_eq!(values[1], 0.5);
    assert_eq!(values[2], 0.6);

    log.set_log_side_slope_value(&[1.4, 1.5, 1.6]);
    let values = log.log_side_slope_value();
    assert_eq!(values[0], 1.4);
    assert_eq!(values[1], 1.5);
    assert_eq!(values[2], 1.6);

    // Convert to op and back to transform.
    build_log_op(&mut ops, log.data(), TransformDirection::Forward).unwrap();
    assert_eq!(ops.len(), 2);
    assert_eq!(ops[1].get_info(), "<LogOp>");

    create_log_transform(&mut group, &ops[1]).unwrap();

    assert_eq!(group.num_transforms(), 2);
    let transform2 = group.transform(1).unwrap();
    let Transform::LogAffine(l_transform2) = transform2 else {
        panic!("not a LogAffineTransform: {transform2:?}");
    };
    assert!(l_transform2.equals(&log));
}
