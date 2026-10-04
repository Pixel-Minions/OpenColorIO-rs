// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the exponent with linear segment transform:
//! `tests/cpu/transforms/ExponentWithLinearTransform_tests.cpp` @ v2.5.2, and `GammaOp
//! create_transform` (tests/cpu/ops/gamma/GammaOp_tests.cpp @ v2.5.2), which tests
//! `CreateGammaTransform` and needed both exponent transforms. The text, the validation, the
//! equality and the ops built are compared with the wheel's in
//! `tests/exponent_transform_oracle.rs`.

use ocio_ops::ops::gamma::GammaStyle;
use ocio_testkit::upstream::{check_close, check_throw_what};

use super::*;
use crate::transform::Transform;

/// `CheckValues` (ExponentWithLinearTransform_tests.cpp:14-23 @ v2.5.2).
fn check_values(v1: &[f64; 4], v2: &[f64; 4]) {
    let err_threshold = f64::from(1e-8f32);

    check_close(v1[0], v2[0], err_threshold);
    check_close(v1[1], v2[1], err_threshold);
    check_close(v1[2], v2[2], err_threshold);
    check_close(v1[3], v2[3], err_threshold);
}

/// Port of `OCIO_ADD_TEST(ExponentWithLinearTransform, basic)` @ v2.5.2.
#[test]
fn basic() {
    let mut exp = ExponentWithLinearTransform::new();
    assert_eq!(exp.direction(), TransformDirection::Forward);

    exp.set_direction(TransformDirection::Inverse);
    assert_eq!(exp.direction(), TransformDirection::Inverse);

    let mut val4 = exp.gamma();
    check_values(&val4, &[1., 1., 1., 1.]);

    val4[1] = 2.1234567;
    exp.set_gamma(&val4);
    let val4 = exp.gamma();
    check_values(&val4, &[1., 2.1234567, 1., 1.]);

    let mut val4 = exp.offset();
    check_values(&val4, &[0., 0., 0., 0.]);

    val4[1] = 0.1234567;
    exp.set_offset(&val4);
    let val4 = exp.offset();
    check_values(&val4, &[0., 0.1234567, 0., 0.]);

    assert_eq!(exp.negative_style(), NegativeStyle::Linear);
    exp.set_negative_style(NegativeStyle::Mirror).unwrap();
    assert_eq!(exp.negative_style(), NegativeStyle::Mirror);
    check_throw_what(
        exp.set_negative_style(NegativeStyle::PassThru),
        "Pass thru negative extrapolation is not valid for MonCurve",
    );
    check_throw_what(
        exp.set_negative_style(NegativeStyle::Clamp),
        "Clamp negative extrapolation is not valid",
    );
    exp.set_negative_style(NegativeStyle::Linear).unwrap();
    assert_eq!(exp.negative_style(), NegativeStyle::Linear);
}

/// Port of `OCIO_ADD_TEST(GammaOp, create_transform)` @ v2.5.2.
#[test]
fn create_transform() {
    let direction = TransformDirection::Forward;

    let red = vec![2., 0.2];
    let green = vec![3., 0.3];
    let blue = vec![4., 0.4];
    let alpha = vec![2.5, 0.25];

    let mut gamma = GammaOpData::new(
        GammaStyle::MoncurveFwd,
        red.clone(),
        green.clone(),
        blue.clone(),
        alpha.clone(),
    );

    gamma
        .get_format_metadata_mut()
        .add_attribute(Some(b"name"), Some(b"test"))
        .unwrap();

    let mut ops = OpVec::new();
    create_gamma_op(&mut ops, gamma, direction);
    assert_eq!(ops.len(), 1);

    let mut group = GroupTransform::new();

    create_gamma_transform(&mut group, &ops[0]).unwrap();
    assert_eq!(group.num_transforms(), 1);
    let transform = group.transform(0).unwrap();
    let Transform::ExponentWithLinear(g_transform) = transform else {
        panic!("not an ExponentWithLinearTransform: {transform:?}");
    };

    assert_eq!(g_transform.negative_style(), NegativeStyle::Linear);

    let metadata = g_transform.format_metadata();
    assert_eq!(metadata.get_num_attributes(), 1);
    assert_eq!(metadata.get_attribute_name(0), b"name");
    assert_eq!(metadata.get_attribute_value(0), b"test");

    assert_eq!(g_transform.direction(), direction);
    let gval = g_transform.gamma();
    assert_eq!(gval[0], red[0]);
    assert_eq!(gval[1], green[0]);
    assert_eq!(gval[2], blue[0]);
    assert_eq!(gval[3], alpha[0]);

    let oval = g_transform.offset();
    assert_eq!(oval[0], red[1]);
    assert_eq!(oval[1], green[1]);
    assert_eq!(oval[2], blue[1]);
    assert_eq!(oval[3], alpha[1]);

    let red0 = vec![2.];
    let green0 = vec![3.];
    let blue0 = vec![4.];
    let alpha0 = vec![2.5];

    let mut gamma0 = GammaOpData::new(
        GammaStyle::BasicRev,
        red0.clone(),
        green0.clone(),
        blue0.clone(),
        alpha0.clone(),
    );

    gamma0
        .get_format_metadata_mut()
        .add_attribute(Some(b"name"), Some(b"test"))
        .unwrap();

    create_gamma_op(&mut ops, gamma0, direction);
    assert_eq!(ops.len(), 2);

    create_gamma_transform(&mut group, &ops[1]).unwrap();
    assert_eq!(group.num_transforms(), 2);
    let transform0 = group.transform(1).unwrap();
    let Transform::Exponent(e_transform) = transform0 else {
        panic!("not an ExponentTransform: {transform0:?}");
    };

    let metadata0 = e_transform.format_metadata();
    assert_eq!(metadata0.get_num_attributes(), 1);
    assert_eq!(metadata0.get_attribute_name(0), b"name");
    assert_eq!(metadata0.get_attribute_value(0), b"test");

    assert_eq!(e_transform.direction(), TransformDirection::Inverse);
    let exp_val = e_transform.value();
    assert_eq!(exp_val[0], red0[0]);
    assert_eq!(exp_val[1], green0[0]);
    assert_eq!(exp_val[2], blue0[0]);
    assert_eq!(exp_val[3], alpha0[0]);

    let gamma1 = GammaOpData::new(GammaStyle::MoncurveMirrorFwd, red, green, blue, alpha);

    create_gamma_op(&mut ops, gamma1, direction);
    assert_eq!(ops.len(), 3);

    create_gamma_transform(&mut group, &ops[2]).unwrap();
    assert_eq!(group.num_transforms(), 3);
    let transform1 = group.transform(2).unwrap();
    let Transform::ExponentWithLinear(el_transform) = transform1 else {
        panic!("not an ExponentWithLinearTransform: {transform1:?}");
    };
    assert_eq!(el_transform.negative_style(), NegativeStyle::Mirror);
}

/// `CreateGammaTransform` refuses an op of another type (which `CreateTransform`'s dispatch
/// never passes it) and adds nothing.
#[test]
fn create_gamma_transform_needs_a_gamma_op() {
    let mut ops = OpVec::new();
    ocio_ops::ops::noop::create_file_no_op(&mut ops, b"f");
    let mut group = GroupTransform::new();
    assert!(create_gamma_transform(&mut group, &ops[0]).is_err());
    assert_eq!(group.num_transforms(), 0);
}

/// `BuildExponentWithLinearOp` validates the data itself: an offset missing adds no op, with
/// the data's error, the one `validate` reports after its prefix.
#[test]
fn build_exponent_with_linear_op_validates_the_data() {
    let mut exp = ExponentWithLinearTransform::new();
    exp.data.set_green_params(vec![2.0]);
    let validated = exp.validate().unwrap_err();

    let mut ops = OpVec::new();
    let built =
        build_exponent_with_linear_op(&mut ops, &exp, TransformDirection::Forward).unwrap_err();
    assert_eq!(
        format!(
            "ExponentWithLinearTransform validation failed: {}",
            built.message()
        ),
        validated.message()
    );
    assert_eq!(ops.len(), 0);
}
