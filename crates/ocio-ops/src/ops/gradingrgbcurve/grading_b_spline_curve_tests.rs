// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of tests/cpu/ops/gradingrgbcurve/GradingBSplineCurve_tests.cpp @ v2.5.2.

use super::*;

fn points(xy: &[(f32, f32)]) -> Vec<GradingControlPoint> {
    xy.iter()
        .map(|&(x, y)| GradingControlPoint::new(x, y))
        .collect()
}

/// Port of `OCIO_ADD_TEST(GradingBSplineCurve, basic)` @ v2.5.2.
#[test]
fn basic() {
    let mut curve = GradingBSplineCurve::new(3);
    assert_eq!(3, curve.num_control_points());
    assert_eq!(0.0f32, curve.control_point(0).unwrap().x);
    assert_eq!(0.0f32, curve.control_point(0).unwrap().y);

    curve.control_point_mut(1).unwrap().x = 0.5;
    curve.control_point_mut(1).unwrap().y = 0.4;
    curve.control_point_mut(2).unwrap().x = 1.0;
    curve.control_point_mut(2).unwrap().y = 0.9;
    assert_eq!(0.5f32, curve.control_point(1).unwrap().x);
    assert_eq!(0.4f32, curve.control_point(1).unwrap().y);
    assert_eq!(1.0f32, curve.control_point(2).unwrap().x);
    assert_eq!(0.9f32, curve.control_point(2).unwrap().y);

    assert!(curve.slopes_are_default());
    curve.set_slope(2, 0.9).unwrap();
    assert_eq!(0.9f32, curve.slope(2).unwrap());
    assert!(!curve.slopes_are_default());

    curve.set_num_control_points(4);
    assert_eq!(4, curve.num_control_points());
    assert_eq!(0.0f32, curve.control_point(3).unwrap().x);
    assert_eq!(0.0f32, curve.control_point(3).unwrap().y);

    let mut curve = GradingBSplineCurve::with_points(&points(&[
        (0.0, 0.0),
        (0.2, 0.3),
        (0.5, 0.7),
        (1.0, 1.0),
    ]));
    assert_eq!(4, curve.num_control_points());
    assert_eq!(0.0f32, curve.control_point(0).unwrap().x);
    assert_eq!(0.0f32, curve.control_point(0).unwrap().y);
    assert_eq!(0.2f32, curve.control_point(1).unwrap().x);
    assert_eq!(0.3f32, curve.control_point(1).unwrap().y);
    assert_eq!(0.5f32, curve.control_point(2).unwrap().x);
    assert_eq!(0.7f32, curve.control_point(2).unwrap().y);
    assert_eq!(1.0f32, curve.control_point(3).unwrap().x);
    assert_eq!(1.0f32, curve.control_point(3).unwrap().y);

    assert_eq!(
        curve.control_point(42).unwrap_err().message(),
        "There are '4' control points. '42' is out of bounds."
    );
    assert_eq!(
        curve.set_slope(42, 0.2).unwrap_err().message(),
        "There are '4' control points. '42' is out of bounds."
    );
    assert!(curve.control_point_mut(42).is_err());

    assert_eq!(
        curve.to_string(),
        "<control_points=[<x=0, y=0><x=0.2, y=0.3><x=0.5, y=0.7><x=1, y=1>]>"
    );
}

/// Port of `OCIO_ADD_TEST(GradingBSplineCurve, validate)` @ v2.5.2.
#[test]
fn validate() {
    let curve = GradingBSplineCurve::new(1);
    assert_eq!(
        curve.validate().unwrap_err().message(),
        "There must be at least 2 control points."
    );
    let mut curve = GradingBSplineCurve::with_points(&points(&[
        (0.0, 0.0),
        (0.7, 0.3),
        (0.5, 0.7),
        (1.0, 1.0),
    ]));
    let message = curve.validate().unwrap_err().message().to_string();
    assert!(
        message.contains(
            "has a x coordinate '0.5' that is less than previous control point x coordinate '0.7'."
        ),
        "{message}"
    );

    curve.control_point_mut(1).unwrap().x = 0.3;
    curve.validate().unwrap();
}

/// Port of `OCIO_ADD_TEST(GradingBSplineCurve, equals)` @ v2.5.2.
#[test]
fn equals() {
    let xy = [(0.0, 0.0), (0.2, 0.3), (0.5, 0.7), (1.0, 1.0)];
    // Identical curves.
    let curve1 = GradingBSplineCurve::with_points(&points(&xy));
    let mut curve2 = GradingBSplineCurve::with_points(&points(&xy));
    assert!(curve1 == curve2);

    // Curve has different spline type.
    let curve3 = GradingBSplineCurve::with_points_and_spline_type(
        &points(&xy),
        BSplineType::DiagonalBSpline,
    );
    assert!(!(curve1 == curve3));

    // Curve has different slopes.
    curve2.set_slope(3, 0.9).unwrap();
    curve2.validate().unwrap();
    assert!(!(curve1 == curve2));

    // Curve has a different control point value.
    let mut curve4 = GradingBSplineCurve::with_points(&points(&xy));
    assert!(curve1 == curve4);
    curve4.control_point_mut(2).unwrap().y = 0.9;
    assert!(!(curve1 == curve4));
}
