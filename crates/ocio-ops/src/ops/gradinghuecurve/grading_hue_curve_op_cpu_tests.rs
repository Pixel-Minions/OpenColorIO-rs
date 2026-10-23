// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of tests/cpu/ops/gradinghuecurve/GradingHueCurveOpCPU_tests.cpp @ v2.5.2.
//!
//! Upstream changes the op data through a shared pointer after making a renderer, then asks for
//! a new renderer; here the data is changed in place and a new renderer made the same way.

use super::*;
use crate::ops::gradinghuecurve::grading_hue_curve::GradingHueCurve;
use crate::ops::gradingrgbcurve::grading_b_spline_curve::{
    GradingBSplineCurve, GradingControlPoint,
};
use ocio_testkit::upstream::check_close;

/// Port of `ValidateImage` (GradingHueCurveOpCPU_tests.cpp:14-32 @ v2.5.2): a NaN expected
/// value wants a NaN, any other is equal or within 2e-5.
fn validate_image(expected: &[f32], res: &[f32]) {
    const ERROR: f32 = 2e-5;
    assert_eq!(expected.len(), res.len());
    for (&e, &r) in expected.iter().zip(res) {
        if e.is_nan() {
            assert!(r.is_nan(), "expected a NaN, got {r}");
        } else if e != r {
            check_close(e, r, ERROR);
        }
    }
}

/// `op->apply(in, res, numPixels)`: the renderer on a copy of `input`.
fn apply(op: &Arc<dyn CpuOp>, input: &[f32]) -> Vec<f32> {
    let mut res = input.to_vec();
    op.apply(&mut res);
    res
}

/// The renderer's class name, as `typeid(c).name()` gives upstream's: its `Debug` text.
fn class_name(op: &Arc<dyn CpuOp>) -> String {
    format!("{op:?}")
}

fn curve(xy: &[(f32, f32)], c: HueCurveType) -> GradingBSplineCurve {
    let points: Vec<GradingControlPoint> = xy
        .iter()
        .map(|&(x, y)| GradingControlPoint::new(x, y))
        .collect();
    GradingBSplineCurve::with_points_and_hue_curve_type(&points, c)
}

/// The op data of the eight curves, `GradingHueCurveOpData(style, hh, hs, hl, ls, ss, ll, sl,
/// hfx)`.
#[allow(clippy::too_many_arguments)]
fn op_data(
    style: GradingStyle,
    hh: &GradingBSplineCurve,
    hs: &GradingBSplineCurve,
    hl: &GradingBSplineCurve,
    ls: &GradingBSplineCurve,
    ss: &GradingBSplineCurve,
    ll: &GradingBSplineCurve,
    sl: &GradingBSplineCurve,
    hfx: &GradingBSplineCurve,
) -> GradingHueCurveOpData {
    GradingHueCurveOpData::with_curves(style, hh, hs, hl, ls, ss, ll, sl, hfx).unwrap()
}

/// Port of `OCIO_ADD_TEST(GradingHueCurveOpCPU, identity)` @ v2.5.2.
#[test]
fn identity() {
    let qnan = f32::NAN;
    let inf = f32::INFINITY;
    #[rustfmt::skip]
    let image: [f32; 36] = [
        -0.50, -0.25, 0.50, 0.0,
        0.75, 1.00, 1.25, 1.0,
        1.25, 1.50, 1.75, 0.0,
        0.0, 0.0, 0.0, qnan,
        0.0, 0.0, 0.0, inf,
        0.0, 0.0, 0.0, -inf,
        0.0, 0.0, 0.0, 0.0,
        0.0, 0.0, 0.0, 0.0,
        0.0, 0.0, 0.0, 0.0,
    ];
    let expected = image;

    // TODO: Nan/Inf handling

    let mut gc = GradingHueCurveOpData::new(GradingStyle::Lin);
    let op = get_grading_hue_curve_cpu_renderer(&gc).unwrap();

    // Check that the right OpCPU is created. Check that class name contains CurveFwdOp.
    assert!(class_name(&op).contains("CurveFwdOp"));
    validate_image(&expected, &apply(&op, &image));

    gc.set_direction(TransformDirection::Inverse);
    let op = get_grading_hue_curve_cpu_renderer(&gc).unwrap();

    // Check that the right OpCPU is created. Check that class name contains CurveRevOp.
    assert!(class_name(&op).contains("CurveRevOp"));
    validate_image(&expected, &apply(&op, &image));
}

/// Port of `OCIO_ADD_TEST(GradingHueCurveOpCPU, log_identity)` @ v2.5.2.
#[test]
fn log_identity() {
    use HueCurveType::*;
    // Identity curves (for log or video) that are different from the default curves.
    let hh = curve(
        &[
            (0.0, 0.0),
            (0.1, 0.1),
            (0.2, 0.2),
            (0.4, 0.4),
            (0.6, 0.6),
            (0.8, 0.8),
        ],
        HueHue,
    );
    let hs = curve(
        &[
            (0.0, 1.0),
            (0.1, 1.0),
            (0.2, 1.0),
            (0.4, 1.0),
            (0.6, 1.0),
            (0.8, 1.0),
        ],
        HueSat,
    );
    let hl = curve(
        &[
            (0.0, 1.0),
            (0.1, 1.0),
            (0.2, 1.0),
            (0.4, 1.0),
            (0.6, 1.0),
            (0.8, 1.0),
        ],
        HueLum,
    );
    let ls = curve(&[(0.0, 1.0), (1.0, 1.0)], LumSat);
    let ss = curve(&[(0.0, 0.0), (0.25, 0.25), (1.0, 1.0)], SatSat);
    let ll = curve(&[(0.0, 0.0), (0.25, 0.25), (0.5, 0.5), (1.0, 1.0)], LumLum);
    let sl = curve(&[(0.0, 1.0), (0.25, 1.0), (0.5, 1.0), (1.0, 1.0)], SatLum);
    let hfx = curve(
        &[
            (0.0, 0.0),
            (0.1, 0.0),
            (0.2, 0.0),
            (0.4, 0.0),
            (0.6, 0.0),
            (0.8, 0.0),
        ],
        HueFx,
    );

    let mut gc = op_data(GradingStyle::Log, &hh, &hs, &hl, &ls, &ss, &ll, &sl, &hfx);
    let op = get_grading_hue_curve_cpu_renderer(&gc).unwrap();

    #[rustfmt::skip]
    let input_32f: [f32; 8] = [
        -0.2, 0.2, 0.5, 0.0,
        0.8, 1.0, 2.0, 0.5,
    ];
    let expected_32f = input_32f;

    // Test in forward direction.
    validate_image(&expected_32f, &apply(&op, &input_32f));

    // Test in inverse direction.
    gc.set_direction(TransformDirection::Inverse);
    let op = get_grading_hue_curve_cpu_renderer(&gc).unwrap();
    validate_image(&input_32f, &apply(&op, &expected_32f));
}

/// Port of `OCIO_ADD_TEST(GradingHueCurveOpCPU, hh_hfx_curves)` @ v2.5.2.
#[test]
fn hh_hfx_curves() {
    use HueCurveType::*;
    // Validate that the hue curves round-trip.

    // Identity curves.
    let hs = curve(&[(0.0, 1.0), (0.9, 1.0)], HueSat);
    let hl = curve(&[(0.0, 1.0), (0.9, 1.0)], HueLum);
    let ls = curve(&[(0.0, 1.0), (0.9, 1.0)], LumSat);
    let ss = curve(&[(0.0, 0.0), (0.9, 0.9)], SatSat);
    let ll = curve(&[(0.0, 0.0), (0.9, 0.9)], LumLum);
    let sl = curve(&[(0.0, 1.0), (0.9, 1.0)], SatLum);

    // Set hh and hfx to non-identities.
    let hh = curve(
        &[
            (0.05, 0.15),
            (0.2, 0.3),
            (0.35, 0.4),
            (0.45, 0.45),
            (0.6, 0.7),
            (0.8, 0.85),
        ],
        HueHue,
    );
    let hfx = curve(
        &[
            (0.2, 0.05),
            (0.4, -0.09),
            (0.6, -0.2),
            (0.8, 0.05),
            (0.99, -0.02),
        ],
        HueFx,
    );

    let mut gc = op_data(GradingStyle::Log, &hh, &hs, &hl, &ls, &ss, &ll, &sl, &hfx);
    let op = get_grading_hue_curve_cpu_renderer(&gc).unwrap();

    #[rustfmt::skip]
    let input_32f: [f32; 16] = [
        0.1, 0.5, 0.7, 0.0,
        0.6, 0.9, 0.8, 0.5,
        0.4, 0.35, 0.3, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ];

    #[rustfmt::skip]
    let expected_32f: [f32; 16] = [
        0.3984785676, 0.3790940642, 1.0187726020, 0.0,
        0.6117081642, 0.8883015513, 0.8814064860, 0.5,
        0.3847683966, 0.3567464352, 0.2780219615, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ];

    // Test in forward direction.
    let mut res = apply(&op, &input_32f);
    validate_image(&expected_32f, &res);

    // Test in inverse direction.
    gc.set_direction(TransformDirection::Inverse);
    let op = get_grading_hue_curve_cpu_renderer(&gc).unwrap();
    op.apply(&mut res);
    validate_image(&input_32f, &res);
}

/// The curves of upstream's `log_all_curves` (GradingHueCurveOpCPU_tests.cpp:209-233 @
/// v2.5.2): `[hh, hs, hl, ls, ss, ll, sl, hfx]`.
fn all_curves() -> [GradingBSplineCurve; 8] {
    use HueCurveType::*;
    [
        curve(
            &[
                (0.05, 0.15),
                (0.2, 0.3),
                (0.35, 0.4),
                (0.45, 0.45),
                (0.6, 0.7),
                (0.8, 0.85),
            ],
            HueHue,
        ),
        curve(
            &[
                (-0.1, 1.2),
                (0.2, 0.7),
                (0.4, 1.5),
                (0.5, 0.5),
                (0.6, 1.4),
                (0.8, 0.7),
            ],
            HueSat,
        ),
        curve(
            &[(0.1, 1.5), (0.2, 0.7), (0.4, 1.4), (0.5, 0.8), (0.8, 0.5)],
            HueLum,
        ),
        curve(&[(0.05, 1.5), (0.5, 0.9), (1.1, 1.4)], LumSat),
        curve(&[(0.0, 0.1), (0.5, 0.45), (1.0, 1.1)], SatSat),
        curve(
            &[(-0.02, -0.04), (0.2, 0.1), (0.8, 0.95), (1.1, 1.2)],
            LumLum,
        ),
        curve(&[(0.0, 1.2), (0.6, 0.8), (0.9, 1.1)], SatLum),
        curve(
            &[
                (0.2, 0.05),
                (0.4, -0.09),
                (0.6, -0.2),
                (0.8, 0.05),
                (0.99, -0.02),
            ],
            HueFx,
        ),
    ]
}

/// Port of `OCIO_ADD_TEST(GradingHueCurveOpCPU_1, log_all_curves)` @ v2.5.2.
#[test]
fn log_all_curves() {
    // All curves are non-identities.
    let [hh, hs, hl, ls, ss, ll, sl, hfx] = all_curves();

    let mut gc = op_data(GradingStyle::Log, &hh, &hs, &hl, &ls, &ss, &ll, &sl, &hfx);
    let op = get_grading_hue_curve_cpu_renderer(&gc).unwrap();

    #[rustfmt::skip]
    let input_32f: [f32; 20] = [
        0.1, 0.5, 0.7, 0.0,
        0.6, 0.9, 0.8, 0.5,
        0.4, 0.35, 0.3, 0.0,
        0.4, -0.2, -0.05, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ];

    #[rustfmt::skip]
    let expected_32f: [f32; 20] = [
        0.651269494808, 0.630018105394, 1.331314732772, 0.0,
        0.787401154155, 1.286561695129, 1.274118545611, 0.5,
        0.317389674917, 0.297787779440, 0.242718507572, 0.0,
        0.830653473122, 0.449246419743, -0.173027078802, 0.0,
        0.004989255546, -0.033773428950, -0.019725339077, 1.0,
    ];

    // Test in forward direction.
    let mut res = apply(&op, &input_32f);
    validate_image(&expected_32f, &res);

    // Test in inverse direction.
    gc.set_direction(TransformDirection::Inverse);
    let op = get_grading_hue_curve_cpu_renderer(&gc).unwrap();
    op.apply(&mut res);
    validate_image(&input_32f, &res);
}

/// Port of `OCIO_ADD_TEST(GradingHueCurveOpCPU, lin_all_curves)` @ v2.5.2.
#[test]
fn lin_all_curves() {
    use HueCurveType::*;
    // All curves are non-identities.
    let [hh, hs, hl, _, ss, _, sl, hfx] = all_curves();
    // Adjust these two, relative to previous test, to work in f-stops.
    let ls = curve(
        &[
            (-6.0, 0.9),
            (-3.0, 0.8),
            (0.0, 1.2),
            (2.0, 1.0),
            (4.0, 0.6),
            (6.0, 0.55),
        ],
        LumSat,
    );
    let ll = curve(
        &[(-8.0, -7.0), (-2.0, -3.0), (2.0, 3.5), (8.0, 7.0)],
        LumLum,
    );

    let mut gc = op_data(GradingStyle::Lin, &hh, &hs, &hl, &ls, &ss, &ll, &sl, &hfx);
    let op = get_grading_hue_curve_cpu_renderer(&gc).unwrap();

    #[rustfmt::skip]
    let input_32f: [f32; 20] = [
        0.1, 0.5, 0.7, 0.0,
        0.6, 0.9, 0.8, 0.5,
        2.4, 2.35, 2.3, 0.0,
        0.4, 0.2, -0.05, 0.0,
        0.0, 0.0, 0.0, 1.0,
    ];

    #[rustfmt::skip]
    let expected_32f: [f32; 20] = [
        0.527229344453, 0.490778616791, 1.693653961874, 0.0,
        1.253512415394, 2.240034381083, 2.215442212203, 0.5,
        6.983003751281, 6.772174817271, 6.179875164501, 0.0,
        0.527554073346, 0.360480028655, -0.135388205576, 0.0,
        0.011308048228, -0.001711436982, 0.003006990049, 1.0,
    ];

    // Test in forward direction.
    let mut res = apply(&op, &input_32f);
    validate_image(&expected_32f, &res);

    // Test in inverse direction.
    gc.set_direction(TransformDirection::Inverse);
    let op = get_grading_hue_curve_cpu_renderer(&gc).unwrap();
    op.apply(&mut res);
    validate_image(&input_32f, &res);
}

/// Port of `OCIO_ADD_TEST(GradingHueCurveOpCPU, draw_curve_only)` @ v2.5.2.
#[test]
fn draw_curve_only() {
    let mut gc = GradingHueCurveOpData::new(GradingStyle::Log);

    let mut val: GradingHueCurve = gc.get_value();
    *val.curve_mut(HueCurveType::HueSat)
        .unwrap()
        .control_point_mut(1)
        .unwrap() = GradingControlPoint::new(0.15, 1.4);
    assert!(!val.is_identity());

    // Enable drawCurveOnly mode. This should only evaluate the HUE-SAT spline for use in a
    // user interface.
    val.set_draw_curve_only(true);
    gc.set_value(&val).unwrap();

    let op = get_grading_hue_curve_cpu_renderer(&gc).unwrap();

    #[rustfmt::skip]
    let input_32f: [f32; 8] = [
        -0.2, 0.15, 0.15, 0.0,
        0.15, 1.0, 2.0, 0.5,
    ];

    #[rustfmt::skip]
    let expected_32f: [f32; 8] = [
        1.0, 1.4, 1.4, 0.0,
        1.4, 1.0, 1.0, 0.5,
    ];

    // Test in forward direction.
    validate_image(&expected_32f, &apply(&op, &input_32f));

    // Test in inverse direction, which should be the same as the forward direction since the
    // direction is ignored for DrawCurveOnly mode.
    gc.set_direction(TransformDirection::Inverse);
    let op = get_grading_hue_curve_cpu_renderer(&gc).unwrap();
    validate_image(&expected_32f, &apply(&op, &input_32f));
}

/// Port of `OCIO_ADD_TEST(GradingHueCurveOpCPU, bypass_rgbtohsy)` @ v2.5.2.
#[test]
fn bypass_rgbtohsy() {
    let mut gc = GradingHueCurveOpData::new(GradingStyle::Log);

    gc.set_rgb_to_hsy(HsyTransformStyle::None);

    let mut val = gc.get_value();
    *val.curve_mut(HueCurveType::SatSat)
        .unwrap()
        .control_point_mut(1)
        .unwrap() = GradingControlPoint::new(0.4, 0.8);
    assert!(!val.is_identity());
    gc.set_value(&val).unwrap();

    let op = get_grading_hue_curve_cpu_renderer(&gc).unwrap();

    #[rustfmt::skip]
    let input_32f: [f32; 8] = [
        0.2, 0.2, -0.1, 0.0,
        0.1, 0.4, 2.0, 0.5,
    ];

    // Only the green channel gets processed, using the sat-sat curve.
    #[rustfmt::skip]
    let expected_32f: [f32; 8] = [
        0.2, 0.4475418, -0.1, 0.0,
        0.1, 0.8000000, 2.0, 0.5,
    ];

    // Test in forward direction.
    let mut res = apply(&op, &input_32f);
    validate_image(&expected_32f, &res);

    // Test in inverse direction.
    gc.set_direction(TransformDirection::Inverse);
    let op = get_grading_hue_curve_cpu_renderer(&gc).unwrap();
    op.apply(&mut res);
    validate_image(&input_32f, &res);
}
