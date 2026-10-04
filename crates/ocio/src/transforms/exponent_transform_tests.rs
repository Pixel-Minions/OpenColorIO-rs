// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the exponent transform: `tests/cpu/transforms/ExponentTransform_tests.cpp` @ v2.5.2,
//! and `ExponentOp create_transform` (tests/cpu/ops/exponent/ExponentOp_tests.cpp @ v2.5.2),
//! which tests `CreateExponentTransform` and needed the transform. The text, the validation,
//! the equality and the ops built are compared with the wheel's in
//! `tests/exponent_transform_oracle.rs`.

use std::sync::Arc;

use ocio_ops::format_metadata::METADATA_ID;
use ocio_ops::op_data::OpData;
use ocio_ops::ops::exponent::exponent_op::create_exponent_op_from_values;
use ocio_ops::ops::gamma::GammaStyle;
use ocio_testkit::upstream::{check_close, check_throw_what};

use super::*;
use crate::transform::Transform;

/// Port of `OCIO_ADD_TEST(ExponentTransform, basic)` @ v2.5.2.
#[test]
fn basic() {
    let mut exp = ExponentTransform::new();
    assert_eq!(exp.direction(), TransformDirection::Forward);

    exp.set_direction(TransformDirection::Inverse);
    assert_eq!(exp.direction(), TransformDirection::Inverse);

    let mut val4 = exp.value();
    assert_eq!(val4[0], 1.);
    assert_eq!(val4[1], 1.);
    assert_eq!(val4[2], 1.);
    assert_eq!(val4[3], 1.);

    val4[1] = 2.;
    exp.set_value(&val4);
    let val4 = exp.value();
    assert_eq!(val4[0], 1.);
    assert_eq!(val4[1], 2.);
    assert_eq!(val4[2], 1.);
    assert_eq!(val4[3], 1.);
}

/// `CheckValues` (ExponentTransform_tests.cpp:33-43 @ v2.5.2).
fn check_values(v1: &[f64; 4], v2: &[f64; 4]) {
    let err_threshold = f64::from(1e-8f32);

    check_close(v1[0], v2[0], err_threshold);
    check_close(v1[1], v2[1], err_threshold);
    check_close(v1[2], v2[2], err_threshold);
    check_close(v1[3], v2[3], err_threshold);
}

/// Port of `OCIO_ADD_TEST(ExponentTransform, double)` @ v2.5.2.
#[test]
fn double() {
    let mut exp = ExponentTransform::new();
    assert_eq!(exp.direction(), TransformDirection::Forward);

    let mut val4 = exp.value();
    check_values(&val4, &[1., 1., 1., 1.]);

    val4[1] = 2.1234567;
    exp.set_value(&val4);
    let val4 = exp.value();
    check_values(&val4, &[1., 2.1234567, 1., 1.]);
}

/// The data of the only op of `ops`.
fn only(ops: &OpVec) -> &OpData {
    assert_eq!(ops.len(), 1);
    ops[0].data()
}

/// Port of `OCIO_ADD_TEST(ExponentTransform, build_ops)` @ v2.5.2. Upstream starts from
/// `Config::Create()`; only the major version matters to `BuildExponentOp`, so this starts from
/// the raw config, the one the port has.
#[test]
fn build_ops() {
    let mut exp = ExponentTransform::new();
    let id = b"sample exponent";
    exp.format_metadata_mut()
        .add_attribute(Some(METADATA_ID), Some(id))
        .unwrap();
    assert_eq!(exp.direction(), TransformDirection::Forward);
    assert_eq!(exp.negative_style(), NegativeStyle::Clamp);

    // With v1 config, exponent transform is converted to ExponentOp that does not handle
    // negative styles.
    let mut config = Config::create_raw();
    Arc::get_mut(&mut config)
        .unwrap()
        .set_major_version(1)
        .unwrap();
    {
        let mut ops = OpVec::new();
        build_exponent_op(&mut ops, &config, &exp, TransformDirection::Forward).unwrap();
        let OpData::Exponent(data) = only(&ops) else {
            panic!("not an Exponent op");
        };
        // In v1 identity exponent is considered a No-op and will be removed (losing the clamp).
        assert!(data.is_no_op());
        assert_eq!(&id[..], data.get_id());
    }
    exp.set_negative_style(NegativeStyle::Mirror).unwrap();
    {
        let mut ops = OpVec::new();
        build_exponent_op(&mut ops, &config, &exp, TransformDirection::Forward).unwrap();
        // Style is ignored, still getting a simple ExponentOpData.
        let OpData::Exponent(data) = only(&ops) else {
            panic!("not an Exponent op");
        };
        assert!(data.is_no_op());
    }

    // With v2 config, exponent transform is converted to GammaOp that handles negative styles.
    Arc::get_mut(&mut config)
        .unwrap()
        .set_major_version(2)
        .unwrap();
    exp.set_negative_style(NegativeStyle::Clamp).unwrap();
    {
        let mut ops = OpVec::new();
        build_exponent_op(&mut ops, &config, &exp, TransformDirection::Forward).unwrap();
        let OpData::Gamma(data) = only(&ops) else {
            panic!("not a Gamma op");
        };
        assert_eq!(data.style(), GammaStyle::BasicFwd);
        assert!(data.is_identity().unwrap());
        // With v2 config clamping is preserved.
        assert!(!data.is_no_op().unwrap());
        assert_eq!(&id[..], data.get_id());
    }
    exp.set_negative_style(NegativeStyle::Mirror).unwrap();
    {
        let mut ops = OpVec::new();
        build_exponent_op(&mut ops, &config, &exp, TransformDirection::Forward).unwrap();
        let OpData::Gamma(data) = only(&ops) else {
            panic!("not a Gamma op");
        };
        assert_eq!(data.style(), GammaStyle::BasicMirrorFwd);
        assert!(data.is_identity().unwrap());
        assert!(data.is_no_op().unwrap());
    }
    exp.set_negative_style(NegativeStyle::PassThru).unwrap();
    {
        let mut ops = OpVec::new();
        build_exponent_op(&mut ops, &config, &exp, TransformDirection::Forward).unwrap();
        let OpData::Gamma(data) = only(&ops) else {
            panic!("not a Gamma op");
        };
        assert_eq!(data.style(), GammaStyle::BasicPassThruFwd);
        assert!(data.is_identity().unwrap());
        assert!(data.is_no_op().unwrap());
    }

    check_throw_what(
        exp.set_negative_style(NegativeStyle::Linear),
        "Linear negative extrapolation is not valid for basic exponent style",
    );
}

/// Port of `OCIO_ADD_TEST(ExponentOp, create_transform)` @ v2.5.2.
#[test]
fn create_transform() {
    let exp: [f64; 4] = [2.0, 2.1, 3.0, 3.1];
    let mut ops = OpVec::new();
    create_exponent_op_from_values(&mut ops, &exp, TransformDirection::Forward).unwrap();
    let op = &ops[0];

    let mut group = GroupTransform::new();
    create_exponent_transform(&mut group, op).unwrap();
    assert_eq!(group.num_transforms(), 1);
    let transform = group.transform(0).unwrap();
    let Transform::Exponent(exp_transform) = transform else {
        panic!("not an ExponentTransform: {transform:?}");
    };

    assert_eq!(exp_transform.direction(), TransformDirection::Forward);
    let exp_val = exp_transform.value();
    assert_eq!(exp_val[0], exp[0]);
    assert_eq!(exp_val[1], exp[1]);
    assert_eq!(exp_val[2], exp[2]);
    assert_eq!(exp_val[3], exp[3]);
}

/// `CreateExponentTransform` refuses an op of another type (which `CreateTransform`'s dispatch
/// never passes it) and adds nothing.
#[test]
fn create_exponent_transform_needs_an_exponent_op() {
    let mut ops = OpVec::new();
    ocio_ops::ops::noop::create_file_no_op(&mut ops, b"f");
    let mut group = GroupTransform::new();
    assert!(create_exponent_transform(&mut group, &ops[0]).is_err());
    assert_eq!(group.num_transforms(), 0);
}

/// `BuildExponentOp` validates the data itself in a version 2 config: an exponent with a
/// parameter missing adds no op, with the data's error, the one `validate` reports after its
/// prefix.
#[test]
fn build_exponent_op_validates_the_data() {
    let mut exp = ExponentTransform::new();
    exp.data.set_red_params(Vec::new());
    let validated = exp.validate().unwrap_err();

    let config = Config::create_raw();
    let mut ops = OpVec::new();
    let built =
        build_exponent_op(&mut ops, &config, &exp, TransformDirection::Forward).unwrap_err();
    assert_eq!(
        format!("ExponentTransform validation failed: {}", built.message()),
        validated.message()
    );
    assert_eq!(ops.len(), 0);
}
