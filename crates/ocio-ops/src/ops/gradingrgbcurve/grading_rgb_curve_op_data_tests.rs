// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of tests/cpu/ops/gradingrgbcurve/GradingRGBCurveOpData_tests.cpp @ v2.5.2.

use super::*;
use crate::open_color_types::{BSplineType, DynamicPropertyType};
use crate::ops::gradingrgbcurve::grading_b_spline_curve::GradingControlPoint;

fn points(xy: &[(f32, f32)]) -> Vec<GradingControlPoint> {
    xy.iter()
        .map(|&(x, y)| GradingControlPoint::new(x, y))
        .collect()
}

/// Port of `OCIO_ADD_TEST(GradingRGBCurveOpData, accessors)` @ v2.5.2.
#[test]
fn accessors() {
    // Create GradingRGBCurveOpData and check values. Changes them and check.
    let mut gc = GradingRgbCurveOpData::new(GradingStyle::Log);

    let expected = "log forward \
        <red=<control_points=[<x=0, y=0><x=0.5, y=0.5><x=1, y=1>]>, \
        green=<control_points=[<x=0, y=0><x=0.5, y=0.5><x=1, y=1>]>, \
        blue=<control_points=[<x=0, y=0><x=0.5, y=0.5><x=1, y=1>]>, \
        master=<control_points=[<x=0, y=0><x=0.5, y=0.5><x=1, y=1>]>>";
    assert_eq!(gc.get_cache_id(), expected.as_bytes());

    assert_eq!(gc.get_style(), GradingStyle::Log);
    let curves = gc.get_value();
    assert!(curves.is_identity());
    assert!(gc.is_identity());
    assert!(gc.is_no_op());
    assert!(!gc.has_channel_crosstalk());
    assert!(!gc.get_bypass_lin_to_log());

    gc.set_style(GradingStyle::Lin);
    assert_eq!(gc.get_style(), GradingStyle::Lin);
    gc.set_bypass_lin_to_log(true);
    assert!(gc.get_bypass_lin_to_log());

    // Get dynamic property as a generic dynamic property and as a type one and verify they are
    // the same and can be made dynamic.
    assert!(!gc.is_dynamic());
    let dp = gc.get_dynamic_property();
    assert_eq!(dp.get_type(), DynamicPropertyType::GradingRgbCurve);
    let dp_impl = gc.get_dynamic_property_internal();
    let DynamicPropertyRcPtr::GradingRgbCurve(dp_rgb) = &dp else {
        panic!("not an RGB curve property");
    };
    assert!(Arc::ptr_eq(dp_rgb, &dp_impl));
    assert!(!dp_impl.is_dynamic());
    dp_impl.make_dynamic();
    assert!(gc.is_dynamic());

    assert_eq!(gc.get_direction(), TransformDirection::Forward);
    gc.set_direction(TransformDirection::Inverse);
    assert_eq!(gc.get_direction(), TransformDirection::Inverse);

    // Test operator==.
    let mut gc1 = GradingRgbCurveOpData::new(GradingStyle::Lin);
    let mut gc2 = GradingRgbCurveOpData::new(GradingStyle::Lin);

    assert!(gc1 == gc2);
    gc1.set_direction(TransformDirection::Inverse);
    assert!(!(gc1 == gc2));
    gc2.set_direction(TransformDirection::Inverse);
    assert!(gc1 == gc2);

    gc1.set_style(GradingStyle::Log);
    assert!(!(gc1 == gc2));
    gc2.set_style(GradingStyle::Log);
    assert!(gc1 == gc2);

    let add_point = |gc: &GradingRgbCurveOpData| {
        let mut v = gc.get_value();
        let red = v.curve_mut(RgbCurveType::Red).unwrap();
        red.set_num_control_points(4);
        let last = *red.control_point(2).unwrap();
        red.control_point_mut(3).unwrap().x = last.x + 1.0;
        red.control_point_mut(3).unwrap().y = last.y + 0.5;
        gc.set_value(&v).unwrap();
    };
    add_point(&gc1);
    assert!(!(gc1 == gc2));
    add_point(&gc2);
    assert!(gc1 == gc2);

    gc1.set_slope(RgbCurveType::Blue, 2, 0.9).unwrap();
    assert_eq!(gc1.get_slope(RgbCurveType::Blue, 2).unwrap(), 0.9f32);
    assert!(gc1.slopes_are_default(RgbCurveType::Green).unwrap());
    assert!(!gc1.slopes_are_default(RgbCurveType::Blue).unwrap());

    assert!(!gc1.is_identity());
    assert!(!gc1.has_channel_crosstalk());

    // Check isInverse.

    // Make two equal non-identity ops, invert one.
    let mut gc3 = GradingRgbCurveOpData::new(GradingStyle::Lin);
    let mut v3 = gc3.get_value();
    let spline = v3.curve_mut(RgbCurveType::Red).unwrap();
    spline.set_num_control_points(2);
    *spline.control_point_mut(0).unwrap() = GradingControlPoint::new(0.0, 2.0);
    *spline.control_point_mut(1).unwrap() = GradingControlPoint::new(0.9, 2.0);
    gc3.set_value(&v3).unwrap();
    assert!(!gc3.is_identity());
    let gcptr3 = gc3.clone();
    gc3.set_direction(TransformDirection::Inverse);
    // They start as inverses.
    assert!(gc3.is_inverse(&gcptr3));

    // Change value of one: no longer an inverse.
    let add_y = |v: &mut GradingRgbCurve, dy: f32| {
        v.curve_mut(RgbCurveType::Red)
            .unwrap()
            .control_point_mut(1)
            .unwrap()
            .y += dy;
    };
    add_y(&mut v3, 0.25);
    gc3.set_value(&v3).unwrap();
    assert!(!gc3.is_inverse(&gcptr3));
    // Restore value.
    add_y(&mut v3, -0.25);
    gc3.set_value(&v3).unwrap();
    assert!(gc3.is_inverse(&gcptr3));

    // Change slope of one: no longer an inverse.
    gc3.set_slope(RgbCurveType::Blue, 2, 0.9).unwrap();
    assert!(!gc3.is_inverse(&gcptr3));
    // Restore value.
    gc3.set_slope(RgbCurveType::Blue, 2, 0.0).unwrap();
    assert!(gc3.is_inverse(&gcptr3));

    // Change direction: no longer an inverse.
    gc3.set_direction(TransformDirection::Forward);
    assert!(!gc3.is_inverse(&gcptr3));
}

/// Port of `OCIO_ADD_TEST(GradingRGBCurveOpData, validate)` @ v2.5.2. Upstream makes the last
/// curve with `HUE_FX`, whose spline type is `PERIODIC_0_B_SPLINE`
/// (`GradingHueCurve::GetBSplineTypeForHueCurveType`, src/OpenColorIO/ops/gradinghuecurve/
/// GradingHueCurve.cpp:254-277 @ v2.5.2, Phase 5).
#[test]
fn validate() {
    // Default is valid.
    let gc = GradingRgbCurveOpData::new(GradingStyle::Log);
    gc.validate().unwrap();

    let all =
        |curve: &GradingBSplineCurve| GradingRgbCurve::with_curves(curve, curve, curve, curve);
    let message = |r: Result<()>| r.unwrap_err().message().to_string();

    // Curves with a single control point are not valid.
    let curve = GradingBSplineCurve::new(1);
    let text = message(gc.set_value(&all(&curve)));
    assert!(
        text.contains("There must be at least 2 control points."),
        "{text}"
    );

    // Curve x coordinates have to increase.
    let mut curve = GradingBSplineCurve::with_points(&points(&[
        (0.0, 0.0),
        (0.7, 0.3),
        (0.5, 0.7),
        (1.0, 1.0),
    ]));
    let text = message(gc.set_value(&all(&curve)));
    assert!(
        text.contains(
            "has a x coordinate '0.5' that is less than previous control point x coordinate '0.7'."
        ),
        "{text}"
    );

    // Fix the curve x coordinate.
    curve.control_point_mut(1).unwrap().x = 0.3;
    gc.set_value(&all(&curve)).unwrap();
    gc.validate().unwrap();

    // Curve y coordinates have to increase.
    let curve = GradingBSplineCurve::with_points(&points(&[
        (0.0, 0.0),
        (0.3, 0.3),
        (0.5, 0.27),
        (1.0, 1.0),
    ]));
    let text = message(gc.set_value(&all(&curve)));
    assert!(
        text.contains(
            "point at index 2 has a y coordinate '0.27' that is less than previous control point \
             y coordinate '0.3'."
        ),
        "{text}"
    );

    // Curve must use the proper spline type.
    let curve = GradingBSplineCurve::with_points_and_spline_type(
        &points(&[(0.0, 0.0), (0.9, 0.0)]),
        BSplineType::Periodic0BSpline,
    );
    let text = message(gc.set_value(&all(&curve)));
    assert!(
        text.contains("validation failed: 'red' curve is of the wrong BSplineType."),
        "{text}"
    );
}
