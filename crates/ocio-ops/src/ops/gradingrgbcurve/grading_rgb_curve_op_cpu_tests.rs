// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of tests/cpu/ops/gradingrgbcurve/GradingRGBCurveOpCPU_tests.cpp @ v2.5.2.
//!
//! Upstream's `op->apply(in, out, n)` between two buffers is [`CpuOp::apply_bit_depth`] here,
//! and `op->apply(buf, buf, n)` is [`CpuOp::apply`]. Its check of the renderer's class name
//! (`typeid(c).name()`) is a check of the port's renderer type's name, which contains the same
//! words (`CurveLinearFwdOp`, `CurveRevOp`, ...).

use super::*;
use crate::ops::gradingrgbcurve::grading_b_spline_curve::{
    GradingBSplineCurve, GradingControlPoint,
};
use ocio_testkit::upstream::check_close;

/// Port of the tests' `ValidateImage` with `OCIO_USE_SSE2`, as the wheels are built: a
/// tolerance of 5e-4, and NaNs not checked.
#[track_caller]
fn validate_image(expected: &[f32], res: &[f32]) {
    const ERROR: f32 = 5e-4;
    assert_eq!(expected.len(), res.len());
    for (&e, &r) in expected.iter().zip(res) {
        if e.is_nan() {
            // Do not test nan in SSE mode.
        } else if e != r {
            check_close(e, r, ERROR);
        }
    }
}

/// `op->apply(input, res, n)`: between two buffers.
fn apply(op: &Arc<dyn CpuOp>, input: &[f32]) -> Vec<f32> {
    let mut res = vec![0.0; input.len()];
    op.apply_bit_depth(Pixels::F32(input), PixelsMut::F32(&mut res));
    res
}

fn curve(xy: &[(f32, f32)]) -> GradingBSplineCurve {
    let points: Vec<GradingControlPoint> = xy
        .iter()
        .map(|&(x, y)| GradingControlPoint::new(x, y))
        .collect();
    GradingBSplineCurve::with_points(&points)
}

/// The renderer of `gc`, whose type's name contains `name`.
#[track_caller]
fn renderer(gc: &GradingRgbCurveOpData, name: &str) -> Arc<dyn CpuOp> {
    let op = get_grading_rgb_curve_cpu_renderer(gc).unwrap();
    let type_name = format!("{op:?}");
    assert!(type_name.contains(name), "{type_name} has no {name}");
    op
}

/// Port of `OCIO_ADD_TEST(GradingRGBCurveOpCPU, identity)` @ v2.5.2.
#[test]
fn identity() {
    let qnan = f32::NAN;
    let inf = f32::INFINITY;
    #[rustfmt::skip]
    let image = [
        -0.50, -0.25, 0.50, 0.0,
         0.75,  1.00, 1.25, 1.0,
         1.25,  1.50, 1.75, 0.0,
         qnan,  qnan, qnan, 0.0,
         0.0,   0.0,  0.0,  qnan,
         inf,   inf,  inf,  0.0,
         0.0,   0.0,  0.0,  inf,
        -inf,  -inf, -inf,  0.0,
         0.0,   0.0,  0.0, -inf,
    ];
    let expected = image;

    let mut gc = GradingRgbCurveOpData::new(GradingStyle::Lin);
    // Check that the right OpCPU is created. Check that class name contains CurveLinearFwdOp.
    let op = renderer(&gc, "CurveLinearFwdOp");
    validate_image(&expected, &apply(&op, &image));

    gc.set_direction(TransformDirection::Inverse);
    let op = renderer(&gc, "CurveLinearRevOp");
    validate_image(&expected, &apply(&op, &image));

    // If BypassLinToLog is true, a Curve*Op renderer rather than a CurveLinear*Op renderer will
    // be used.
    gc.set_bypass_lin_to_log(true);
    let op = renderer(&gc, "CurveRevOp");
    validate_image(&expected, &apply(&op, &image));

    gc.set_direction(TransformDirection::Forward);
    let op = renderer(&gc, "CurveFwdOp");
    validate_image(&expected, &apply(&op, &image));

    let mut gc = GradingRgbCurveOpData::new(GradingStyle::Video);
    let op = renderer(&gc, "CurveFwdOp");
    validate_image(&expected, &apply(&op, &image));

    gc.set_direction(TransformDirection::Inverse);
    let op = renderer(&gc, "CurveRevOp");
    validate_image(&expected, &apply(&op, &image));

    // BypassLinToLog is ignored when style is not GRADING_LIN, still creating a CurveRevOp
    // renderer.
    gc.set_bypass_lin_to_log(true);
    let op = renderer(&gc, "CurveRevOp");
    validate_image(&expected, &apply(&op, &image));
}

/// Forward from `input` to `expected`, then inverse back, between two buffers.
#[track_caller]
fn round_trip(mut gc: GradingRgbCurveOpData, input: &[f32], expected: &[f32]) {
    // Test in forward direction.
    let op = get_grading_rgb_curve_cpu_renderer(&gc).unwrap();
    validate_image(expected, &apply(&op, input));

    // Test in inverse direction.
    gc.set_direction(TransformDirection::Inverse);
    let op = get_grading_rgb_curve_cpu_renderer(&gc).unwrap();
    validate_image(input, &apply(&op, expected));
}

/// Port of `OCIO_ADD_TEST(GradingRGBCurveOpCPU, log)` @ v2.5.2.
#[test]
fn log() {
    let r = curve(&[(0.1, 0.15), (0.55, 0.45), (0.9, 1.1)]);
    let g = curve(&[(0.1, 0.15), (0.55, 0.35), (0.9, 1.1)]);
    let b = curve(&[(0.1, 0.15), (0.55, 0.85), (0.9, 1.1)]);
    let m = curve(&[(-0.1, 0.1), (1.1, 1.3)]);
    let gc = GradingRgbCurveOpData::with_curves(GradingStyle::Log, &r, &g, &b, &m).unwrap();

    #[rustfmt::skip]
    let input_32f = [
        -0.2, 0.2, 0.5, 0.0,
         0.8, 1.0, 2.0, 0.5,
    ];
    #[rustfmt::skip]
    let expected_32f = [
        0.25306581, 0.35779659, 0.98416632, 0.0,
        1.09451043, 1.54596428, 1.78067802, 0.5,
    ];
    round_trip(gc, &input_32f, &expected_32f);
}

/// Port of `OCIO_ADD_TEST(GradingRGBCurveOpCPU, log_partial_identity)` @ v2.5.2.
#[test]
fn log_partial_identity() {
    let r = curve(&[(0.1, 0.1), (0.9, 0.9)]);
    let g = curve(&[(0.1, 0.15), (0.55, 0.35), (0.9, 1.1)]);
    let b = curve(&[(0.0, 0.0), (0.5, 0.5), (1.0, 1.0)]);
    let m = curve(&[(0.1, 0.1), (1.1, 1.1)]);
    let gc = GradingRgbCurveOpData::with_curves(GradingStyle::Log, &r, &g, &b, &m).unwrap();

    #[rustfmt::skip]
    let input_32f = [
        -0.2, 0.2, 0.5, 0.0,
         0.8, 1.0, 2.0, 0.5,
    ];
    #[rustfmt::skip]
    let expected_32f = [
        -0.2, 0.15779659, 0.5, 0.0,
         0.8, 1.34596419, 2.0, 0.5,
    ];
    round_trip(gc, &input_32f, &expected_32f);
}

/// Port of `OCIO_ADD_TEST(GradingRGBCurveOpCPU, monotonic)` @ v2.5.2.
#[test]
fn monotonic() {
    let r = curve(&[
        (0.0, 0.0),
        (0.785, 0.231),
        (0.809, 0.631),
        (0.948, 0.704),
        (1.0, 1.0),
    ]);
    let g = curve(&[(-0.1, -0.1), (1.1, 1.1)]);
    let b = curve(&[(-0.1, -0.1), (1.1, 1.1)]);
    let m = curve(&[(-0.1, -0.1), (1.1, 1.1)]);
    let gc = GradingRgbCurveOpData::with_curves(GradingStyle::Log, &r, &g, &b, &m).unwrap();

    #[rustfmt::skip]
    let input_32f = [
        0.8, 0.2, 0.5, 0.0,
        0.9, 1.0, 2.0, 0.5,
    ];
    #[rustfmt::skip]
    let expected_32f = [
        0.52230538, 0.2, 0.5, 0.0,
        0.68079938, 1.0, 2.0, 0.5,
    ];
    round_trip(gc, &input_32f, &expected_32f);
}

/// The curves of the `lin_bypass` and `lin` tests.
fn lin_curves(style: GradingStyle) -> GradingRgbCurveOpData {
    let rgb = curve(&[(-6.0, -8.0), (-2.0, -5.0), (2.0, 4.0), (5.0, 6.0)]);
    let m = curve(&[(0.0, 0.0), (0.5, 0.5), (1.0, 1.0)]);
    GradingRgbCurveOpData::with_curves(style, &rgb, &rgb, &rgb, &m).unwrap()
}

/// Port of `OCIO_ADD_TEST(GradingRGBCurveOpCPU, lin_bypass)` @ v2.5.2.
#[test]
fn lin_bypass() {
    let mut gc = lin_curves(GradingStyle::Lin);
    gc.set_bypass_lin_to_log(true);

    #[rustfmt::skip]
    let input_32f = [
        -8.0, -3.0, -1.0, 0.0,
         1.0,  2.5,  4.0, 0.5,
    ];
    #[rustfmt::skip]
    let expected_32f = [
        -8.50508935, -6.37181915, -3.01264257, 0.0,
         1.95205522,  4.76796850,  5.76796850, 0.5,
    ];
    round_trip(gc, &input_32f, &expected_32f);
}

/// Port of `OCIO_ADD_TEST(GradingRGBCurveOpCPU, lin)` @ v2.5.2.
#[test]
fn lin() {
    let gc = lin_curves(GradingStyle::Lin);

    #[rustfmt::skip]
    let input_32f = [
        -0.003, 0.02, 0.09, 0.0,
         0.360, 1.00, 3.00, 0.5,
    ];
    #[rustfmt::skip]
    let expected_32f = [
        -4.20784139e-03, 1.26825221e-03, 2.23983977e-02, 0.0,
         6.96706128e-01, 4.79411018e+00, 9.95152432e+00, 0.5,
    ];
    round_trip(gc, &input_32f, &expected_32f);
}

/// Port of `OCIO_ADD_TEST(GradingRGBCurveOpCPU, slopes)` @ v2.5.2.
#[test]
fn slopes() {
    let mut curve_m = curve(&[
        (-5.26017743, -4.0),
        (-3.75502745, -3.57868829),
        (-2.24987747, -1.82131329),
        (-0.74472749, 0.68124124),
        (1.06145248, 2.87457742),
        (2.86763245, 3.83406206),
        (4.67381243, 4.0),
    ]);
    let slopes = [
        0.0, 0.55982688, 1.77532247, 1.55, 0.8787017, 0.18374463, 0.0,
    ];
    for (i, &slope) in slopes.iter().enumerate() {
        curve_m.set_slope(i, slope).unwrap();
    }
    curve_m.validate().unwrap();

    let identity = curve(&[(0.0, 0.0), (1.0, 1.0)]);
    let mut gc = GradingRgbCurveOpData::with_curves(
        GradingStyle::Log,
        &identity,
        &identity,
        &identity,
        &curve_m,
    )
    .unwrap();
    let op = get_grading_rgb_curve_cpu_renderer(&gc).unwrap();

    #[rustfmt::skip]
    let mut input_32f = [
        -3.0, -1.0, 1.0, 0.5,
        -7.0,  0.0, 7.0, 1.0,
    ];
    // Test that the slopes were used (the values are significantly different without slopes).
    #[rustfmt::skip]
    let expected_32f = [
        -2.92582282, 0.28069129, 2.81987724, 0.5,
        -4.0,        1.73250193, 4.0,        1.0,
    ];
    op.apply(&mut input_32f);
    validate_image(&expected_32f, &input_32f);

    // Test in inverse direction.
    #[rustfmt::skip]
    let mut rev_input_32f = [
        -2.92582282, 0.28069129, 2.81987724, 0.5,
        -7.0,        1.73250193, 7.0,        1.0,
    ];
    #[rustfmt::skip]
    let rev_expected_32f = [
        -3.0,        -1.0, 1.0,        0.5,
        -5.26017743,  0.0, 4.67381243, 1.0,
    ];
    gc.set_direction(TransformDirection::Inverse);
    let op = get_grading_rgb_curve_cpu_renderer(&gc).unwrap();
    op.apply(&mut rev_input_32f);
    validate_image(&rev_expected_32f, &rev_input_32f);
}
