// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of tests/cpu/ops/gradingrgbcurve/GradingRGBCurve_tests.cpp @ v2.5.2 (`max_ctrl_pnts`,
//! which needs the dynamic property, is with it in `dynamic_property_tests.rs`).

use super::*;

fn points(xy: &[(f32, f32)]) -> Vec<GradingControlPoint> {
    xy.iter()
        .map(|&(x, y)| GradingControlPoint::new(x, y))
        .collect()
}

/// Port of `OCIO_ADD_TEST(GradingRGBCurve, basic)` @ v2.5.2. Upstream changes a curve through
/// a shared pointer and checks that the copies `Create` made don't change; the curves are
/// values here, so the copies are clones.
#[test]
fn basic() {
    let mut curve = GradingBSplineCurve::with_points(&points(&[
        (0.0, 0.0),
        (0.2, 0.2),
        (0.5, 0.7),
        (1.0, 1.0),
    ]));
    assert_eq!(0.2f32, curve.control_point(1).unwrap().y);
    curve.control_point_mut(1).unwrap().y = 0.3;
    assert_eq!(0.3f32, curve.control_point(1).unwrap().y);
    let curve_g = GradingBSplineCurve::new(4);
    let curve_b = GradingBSplineCurve::new(3);
    let curve_m = GradingBSplineCurve::new(2);
    // The Create function takes 4 curves and creates new curves that are copies of the 4
    // parameters.
    let rgb_curve = GradingRgbCurve::with_curves(&curve, &curve_g, &curve_b, &curve_m);
    for c in RgbCurveType::CURVES {
        assert!(rgb_curve.curve(c).is_ok());
    }
    assert_eq!(
        rgb_curve
            .curve(RgbCurveType::NumCurves)
            .unwrap_err()
            .message(),
        "Invalid curve."
    );
    let copied_curve = rgb_curve.curve(RgbCurveType::Red).unwrap();
    assert_eq!(0.3f32, copied_curve.control_point(1).unwrap().y);
    curve.control_point_mut(1).unwrap().y = 0.4;
    assert_eq!(0.3f32, copied_curve.control_point(1).unwrap().y);

    // Test default curves.
    let rgb_curve_lin = GradingRgbCurve::new(GradingStyle::Lin);
    let rgb_curve_log = GradingRgbCurve::new(GradingStyle::Log);
    let rgb_curve_video = GradingRgbCurve::new(GradingStyle::Video);
    assert!(rgb_curve_log == rgb_curve_video);
    assert!(rgb_curve_log != rgb_curve_lin);
    let log = |c| rgb_curve_log.curve(c).unwrap();
    assert!(log(RgbCurveType::Red) == log(RgbCurveType::Green));
    assert!(log(RgbCurveType::Red) == log(RgbCurveType::Blue));
    assert!(log(RgbCurveType::Red) == log(RgbCurveType::Master));
    let red = log(RgbCurveType::Red);
    assert_eq!(3, red.num_control_points());
    assert_eq!(0.0f32, red.control_point(0).unwrap().x);
    assert_eq!(0.0f32, red.control_point(0).unwrap().y);
    assert_eq!(0.5f32, red.control_point(1).unwrap().x);
    assert_eq!(0.5f32, red.control_point(1).unwrap().y);
    assert_eq!(1.0f32, red.control_point(2).unwrap().x);
    assert_eq!(1.0f32, red.control_point(2).unwrap().y);

    let lin = |c| rgb_curve_lin.curve(c).unwrap();
    assert!(lin(RgbCurveType::Red) == lin(RgbCurveType::Green));
    assert!(lin(RgbCurveType::Red) == lin(RgbCurveType::Blue));
    assert!(lin(RgbCurveType::Red) == lin(RgbCurveType::Master));
    let red = lin(RgbCurveType::Red);
    assert_eq!(3, red.num_control_points());
    assert_eq!(-7.0f32, red.control_point(0).unwrap().x);
    assert_eq!(-7.0f32, red.control_point(0).unwrap().y);
    assert_eq!(0.0f32, red.control_point(1).unwrap().x);
    assert_eq!(0.0f32, red.control_point(1).unwrap().y);
    assert_eq!(7.0f32, red.control_point(2).unwrap().x);
    assert_eq!(7.0f32, red.control_point(2).unwrap().y);

    let rgb_curve_lin_copy = rgb_curve_lin.clone();
    assert!(rgb_curve_lin == rgb_curve_lin_copy);

    assert_eq!(
        rgb_curve_lin.to_string(),
        "<red=<control_points=[<x=-7, y=-7><x=0, y=0><x=7, y=7>]>, \
         green=<control_points=[<x=-7, y=-7><x=0, y=0><x=7, y=7>]>, \
         blue=<control_points=[<x=-7, y=-7><x=0, y=0><x=7, y=7>]>, \
         master=<control_points=[<x=-7, y=-7><x=0, y=0><x=7, y=7>]>>"
    );
}

/// Port of `OCIO_ADD_TEST(GradingRGBCurve, curves)` @ v2.5.2.
#[test]
fn curves() {
    let mut curves = GradingRgbCurve::new(GradingStyle::Video);
    assert!(curves.is_identity());
    // Use non const curve accessor to modify one of the spline of the curves.
    let spline = curves.curve_mut(RgbCurveType::Green).unwrap();
    spline.set_num_control_points(4);
    spline.control_point_mut(3).unwrap().x = 1.1;
    spline.control_point_mut(3).unwrap().y = 2.0;
    assert!(!curves.is_identity());
    let spline = curves.curve_mut(RgbCurveType::Green).unwrap();
    spline.control_point_mut(3).unwrap().x = 2.0;
    assert!(curves.is_identity());
    assert_eq!(
        curves
            .curve(RgbCurveType::Green)
            .unwrap()
            .num_control_points(),
        4
    );

    // Changing the pointer does not change the curves.
    let mut spline = curves.curve(RgbCurveType::Green).unwrap().clone();
    assert_eq!(spline.num_control_points(), 4);
    spline = GradingBSplineCurve::with_points(&points(&[(0.0, 0.0), (1.0, 2.0)]));
    assert_eq!(spline.num_control_points(), 2);
    assert_eq!(
        curves
            .curve(RgbCurveType::Green)
            .unwrap()
            .num_control_points(),
        4
    );
}
