// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the log camera transform: `tests/cpu/transforms/LogCameraTransform_tests.cpp` @
//! v2.5.2. The text, the validation, the equality and the ops built are compared with the
//! wheel's in `tests/log_transform_oracle.rs`.

use ocio_ops::op::OpVec;

use super::*;
use crate::transform::Transform;
use crate::transforms::group_transform::GroupTransform;
use crate::transforms::log_transform::{build_log_op, create_log_transform};

/// `AllEqual` (LogCameraTransform_tests.cpp:15-18 @ v2.5.2).
fn all_equal(values: &[f64; 3]) -> bool {
    values[0] == values[1] && values[0] == values[2]
}

/// Port of `OCIO_ADD_TEST(LogCameraTransform, camera)` @ v2.5.2.
#[test]
fn camera() {
    let mut log = LogCameraTransform::new(&[0.2, 0.2, 0.2]);

    let values = log.lin_side_break_value();
    assert!(all_equal(&values));
    assert_eq!(values[0], 0.2);

    assert!(log.linear_slope_value().is_none());

    log.set_linear_slope_value(&[1., 1., 1.]).unwrap();
    let values = log.linear_slope_value().unwrap();
    assert!(all_equal(&values));
    assert_eq!(values[0], 1.);

    log.unset_linear_slope_value();
    assert!(log.linear_slope_value().is_none());

    log.set_lin_side_break_value(&[0.01, 0.02, 0.03]);
    let values = log.lin_side_break_value();
    assert_eq!(values[0], 0.01);
    assert_eq!(values[1], 0.02);
    assert_eq!(values[2], 0.03);
    assert!(log.linear_slope_value().is_none());

    log.set_linear_slope_value(&[1., 1.1, 1.2]).unwrap();
    let values = log.linear_slope_value().unwrap();
    assert_eq!(values[0], 1.);
    assert_eq!(values[1], 1.1);
    assert_eq!(values[2], 1.2);

    let mut ops = OpVec::new();

    // Convert to op and back to transform.
    build_log_op(&mut ops, log.data(), TransformDirection::Forward).unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].get_info(), "<LogOp>");

    let mut group = GroupTransform::new();
    create_log_transform(&mut group, &ops[0]).unwrap();

    assert_eq!(group.num_transforms(), 1);
    let transform = group.transform(0).unwrap();
    let Transform::LogCamera(l_transform) = transform else {
        panic!("not a LogCameraTransform: {transform:?}");
    };
    assert!(l_transform.equals(&log));
}
