// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of tests/cpu/ops/exposurecontrast/ExposureContrastOp_tests.cpp @ v2.5.2, but
//! `ExposureContrastOp create_transform`, which needs the `ExposureContrastTransform` (5.5a).
//!
//! Upstream's `CreateExposureContrastOp` shares the caller's data with a forward op; here the
//! op owns the data the caller passes, a copy where the test goes on using its own.

use super::*;

/// The op data of `op`.
fn ec_data(op: &Op) -> &ExposureContrastOpData {
    let OpData::ExposureContrast(data) = &**op.data() else {
        panic!("not an ExposureContrast op");
    };
    data
}

/// Port of `OCIO_ADD_TEST(ExposureContrastOp, create)` @ v2.5.2.
#[test]
fn create() {
    let direction = TransformDirection::Forward;
    let data = ExposureContrastOpData::default();
    let mut ops = OpVec::new();

    // Make it dynamic so that it is not a no op.
    data.get_exposure_property().make_dynamic();
    create_exposure_contrast_op(&mut ops, data.clone(), direction);
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].get_info(), "<ExposureContrastOp>");

    let data = data.clone();
    data.get_contrast_property().make_dynamic();
    create_exposure_contrast_op(&mut ops, data, direction);
    assert_eq!(ops.len(), 2);
    let op1 = ops[1].clone();
    assert!(ops[0].is_same_type(&op1));
}

/// Port of `OCIO_ADD_TEST(ExposureContrastOp, inverse)` @ v2.5.2.
#[test]
fn inverse() {
    let mut direction = TransformDirection::Forward;
    let mut data = ExposureContrastOpData::default();

    data.set_exposure(1.2);
    data.set_pivot(0.5);

    let mut ops = OpVec::new();
    create_exposure_contrast_op(&mut ops, data.clone(), direction);
    assert_eq!(ops.len(), 1);

    let data = data.clone();
    direction = TransformDirection::Inverse;
    create_exposure_contrast_op(&mut ops, data.clone(), direction);
    assert_eq!(ops.len(), 2);

    let op0 = ops[0].clone();
    let op1 = ops[1].clone();
    assert!(op0.is_inverse(&op1));
    assert!(op1.is_inverse(&op0));

    // Create op2 similar to op1 with different exposure.
    let data = data.clone();
    data.set_exposure(1.3);
    create_exposure_contrast_op(&mut ops, data.clone(), direction);
    assert_eq!(ops.len(), 3);

    let op2 = ops[2].clone();
    // As exposure from E/C is not dynamic and exposure is different,
    // op1 and op2 are different.
    assert!(!Arc::ptr_eq(op1.data(), op2.data()));
    assert!(!op0.is_inverse(&op2));

    // With dynamic property.
    let data = data.clone();
    data.get_exposure_property().make_dynamic();
    let dp3 = data.get_exposure_property();
    create_exposure_contrast_op(&mut ops, data.clone(), direction);
    assert_eq!(ops.len(), 4);
    let op3 = ops[3].clone();

    let data = data.clone();
    let dp4 = data.get_exposure_property();

    direction = TransformDirection::Forward;
    // The forward op holds this data (upstream's shares it), with its property `dp4`.
    create_exposure_contrast_op(&mut ops, data, direction);
    assert_eq!(ops.len(), 5);
    assert!(Arc::ptr_eq(&ec_data(&ops[4]).get_exposure_property(), &dp4));

    // Exposure dynamic, same value, opposite direction.
    assert!(!ops[4].is_inverse(&op3));
    assert!(!ops[4].is_inverse(&op1));
    assert!(!ops[4].is_inverse(&op0));

    // When dynamic property is enabled they are never equal.
    dp4.set_value(-1.);
    assert!(dp3.get_value() != dp4.get_value());
    assert!(!ops[4].is_inverse(&op3));
    dp3.set_value(-1.);
    assert!(dp3.get_value() == dp4.get_value());
    assert!(!ops[4].is_inverse(&op3));
}
