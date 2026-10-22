// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of tests/cpu/ops/gradinghuecurve/GradingHueCurveOpData_tests.cpp @ v2.5.2.
//!
//! Upstream changes the curves through shared pointers into a copy (`createEditableCopy`)
//! before it sets them; the curves are values here, changed through `curve_mut`.

use super::*;
use crate::open_color_types::DynamicPropertyType;
use crate::ops::gradingrgbcurve::grading_b_spline_curve::GradingControlPoint;
use ocio_testkit::upstream::check_throw_what;

fn points(xy: &[(f32, f32)]) -> Vec<GradingControlPoint> {
    xy.iter()
        .map(|&(x, y)| GradingControlPoint::new(x, y))
        .collect()
}

/// Port of `OCIO_ADD_TEST(GradingHueCurveOpData, accessors)` @ v2.5.2.
#[test]
fn accessors() {
    // Create GradingHueCurveOpData and check values. Change them and check.

    let mut gc = GradingHueCurveOpData::new(GradingStyle::Log);

    let expected = "log forward \
        <hue_hue=<control_points=[<x=0, y=0><x=0.1666667, y=0.1666667><x=0.3333333, y=0.3333333><x=0.5, y=0.5><x=0.6666667, y=0.6666667><x=0.8333333, y=0.8333333>]>, \
        hue_sat=<control_points=[<x=0, y=1><x=0.1666667, y=1><x=0.3333333, y=1><x=0.5, y=1><x=0.6666667, y=1><x=0.8333333, y=1>]>, \
        hue_lum=<control_points=[<x=0, y=1><x=0.1666667, y=1><x=0.3333333, y=1><x=0.5, y=1><x=0.6666667, y=1><x=0.8333333, y=1>]>, \
        lum_sat=<control_points=[<x=0, y=1><x=0.5, y=1><x=1, y=1>]>, \
        sat_sat=<control_points=[<x=0, y=0><x=0.5, y=0.5><x=1, y=1>]>, \
        lum_lum=<control_points=[<x=0, y=0><x=0.5, y=0.5><x=1, y=1>]>, \
        sat_lum=<control_points=[<x=0, y=1><x=0.5, y=1><x=1, y=1>]>, \
        hue_fx=<control_points=[<x=0, y=0><x=0.1666667, y=0><x=0.3333333, y=0><x=0.5, y=0><x=0.6666667, y=0><x=0.8333333, y=0>]>>";
    assert_eq!(String::from_utf8_lossy(&gc.get_cache_id()), expected);

    assert_eq!(gc.get_style(), GradingStyle::Log);
    let curves = gc.get_value();
    assert!(curves.is_identity());
    assert!(gc.is_identity());
    assert!(gc.is_no_op());
    assert!(gc.has_channel_crosstalk());
    assert_eq!(gc.get_rgb_to_hsy(), HsyTransformStyle::Transform1);

    gc.set_style(GradingStyle::Lin);
    assert_eq!(gc.get_style(), GradingStyle::Lin);
    gc.set_rgb_to_hsy(HsyTransformStyle::None);
    assert_eq!(gc.get_rgb_to_hsy(), HsyTransformStyle::None);

    // Get dynamic property as a generic dynamic property and as a typed one and verify they
    // are the same and can be made dynamic.

    assert!(!gc.is_dynamic());
    let dp = gc.get_dynamic_property();
    assert_eq!(dp.get_type(), DynamicPropertyType::GradingHueCurve);
    let dp_impl = gc.get_dynamic_property_internal();
    let DynamicPropertyRcPtr::GradingHueCurve(dp_typed) = &dp else {
        panic!("a hue curve property");
    };
    assert!(Arc::ptr_eq(dp_typed, &dp_impl));
    assert!(!dp_impl.is_dynamic());
    dp_impl.make_dynamic();
    assert!(gc.is_dynamic());

    assert_eq!(gc.get_direction(), TransformDirection::Forward);
    gc.set_direction(TransformDirection::Inverse);
    assert_eq!(gc.get_direction(), TransformDirection::Inverse);

    // Test operator==.

    let mut gc1 = GradingHueCurveOpData::new(GradingStyle::Lin);
    let mut gc2 = GradingHueCurveOpData::new(GradingStyle::Lin);

    assert!(gc1 == gc2);
    gc1.set_direction(TransformDirection::Inverse);
    assert!(!(gc1 == gc2));
    gc2.set_direction(TransformDirection::Inverse);
    assert!(gc1 == gc2);

    gc1.set_style(GradingStyle::Log);
    assert!(!(gc1 == gc2));
    gc2.set_style(GradingStyle::Log);
    assert!(gc1 == gc2);

    let mut v1 = gc1.get_value();
    {
        let hue_hue = v1.curve_mut(HueCurveType::HueHue).unwrap();
        hue_hue.set_num_control_points(4);
        let p2 = *hue_hue.control_point(2).unwrap();
        hue_hue.control_point_mut(3).unwrap().x = p2.x + 0.25;
        hue_hue.control_point_mut(3).unwrap().y = p2.y + 0.5;
    }
    gc1.set_value(&v1).unwrap();
    assert!(!(gc1 == gc2));
    let mut v2 = gc2.get_value();
    {
        let hue_hue2 = v2.curve_mut(HueCurveType::HueHue).unwrap();
        hue_hue2.set_num_control_points(4);
        let p2 = *hue_hue2.control_point(2).unwrap();
        hue_hue2.control_point_mut(3).unwrap().x = p2.x + 0.25;
        hue_hue2.control_point_mut(3).unwrap().y = p2.y + 0.5;
    }
    gc2.set_value(&v2).unwrap();
    assert!(gc1 == gc2);

    gc1.set_slope(HueCurveType::HueSat, 2, 0.9).unwrap();
    assert!(!(gc1 == gc2));
    assert_eq!(gc1.get_slope(HueCurveType::HueSat, 2).unwrap(), 0.9f32);
    assert!(gc1.slopes_are_default(HueCurveType::HueLum).unwrap());
    assert!(!gc1.slopes_are_default(HueCurveType::HueSat).unwrap());

    assert!(!gc1.is_identity());
    assert!(gc1.has_channel_crosstalk());

    // Check isInverse.

    // Make two equal non-identity ops, invert one.
    let mut gc3 = GradingHueCurveOpData::new(GradingStyle::Lin);
    let mut v3 = gc3.get_value();
    {
        let spline = v3.curve_mut(HueCurveType::HueLum).unwrap();
        spline.set_num_control_points(2);
        *spline.control_point_mut(0).unwrap() = GradingControlPoint::new(0.0, 2.0);
        *spline.control_point_mut(1).unwrap() = GradingControlPoint::new(0.9, 2.0);
    }
    gc3.set_value(&v3).unwrap();
    assert!(!gc3.is_identity());
    // Need a shared pointer for the parameter.
    let gcptr3 = gc3.clone();
    gc3.set_direction(TransformDirection::Inverse);
    // They start as inverses.
    assert!(gc3.is_inverse(&gcptr3));

    // Change value of one: no longer an inverse.
    v3.curve_mut(HueCurveType::HueLum)
        .unwrap()
        .control_point_mut(1)
        .unwrap()
        .y += 0.25;
    gc3.set_value(&v3).unwrap();
    assert!(!gc3.is_inverse(&gcptr3));
    // Restore value.
    v3.curve_mut(HueCurveType::HueLum)
        .unwrap()
        .control_point_mut(1)
        .unwrap()
        .y -= 0.25;
    gc3.set_value(&v3).unwrap();
    assert!(gc3.is_inverse(&gcptr3));

    // Change slope of one: no longer an inverse.
    gc3.set_slope(HueCurveType::HueSat, 2, 0.9).unwrap();
    assert!(!gc3.is_inverse(&gcptr3));
    // Restore value.
    gc3.set_slope(HueCurveType::HueSat, 2, 0.0).unwrap();
    assert!(gc3.is_inverse(&gcptr3));

    // Change direction: no longer an inverse.
    gc3.set_direction(TransformDirection::Forward);
    assert!(!gc3.is_inverse(&gcptr3));
}

/// Port of `OCIO_ADD_TEST(GradingHueCurveOpData, validate)` @ v2.5.2.
#[test]
fn validate() {
    // Default is valid.
    let gc = GradingHueCurveOpData::new(GradingStyle::Log);
    assert!(gc.validate().is_ok());

    let create = |curve: &GradingBSplineCurve| {
        GradingHueCurve::with_curves(curve, curve, curve, curve, curve, curve, curve, curve)
    };

    // Curves with a single control point are not valid.
    let curve = GradingBSplineCurve::new(1);
    check_throw_what(create(&curve), "There must be at least 2 control points.");

    // A periodic curve may not have only two control points that wrap to the same point.
    let curve = GradingBSplineCurve::with_points_and_hue_curve_type(
        &points(&[(0.0, 0.0), (1.0, 0.0)]),
        HueCurveType::HueFx,
    );
    check_throw_what(
        create(&curve),
        "The periodic spline x coordinates may not wrap to the same value.",
    );

    // Curve x coordinates have to increase.
    let curve = GradingBSplineCurve::with_points(&points(&[
        (0.0, 0.0),
        (0.7, 0.3),
        (0.5, 0.7),
        (1.0, 1.0),
    ]));
    check_throw_what(
        create(&curve),
        "has a x coordinate '0.5' that is less than previous control point x coordinate '0.7'.",
    );

    // A hue-hue curve must have x-coordinates in [0,1].
    let mut curve = GradingBSplineCurve::with_points_and_hue_curve_type(
        &points(&[(0.1, 0.05), (1.1, 1.05)]),
        HueCurveType::HueHue,
    );
    check_throw_what(
        create(&curve),
        "The HUE-HUE spline may not have x coordinates greater than one.",
    );

    // Fix the curve x coordinate.
    curve.control_point_mut(1).unwrap().x = 1.0;
    // But the curves are not all of the correct BSplineType.
    check_throw_what(
        create(&curve),
        "GradingHueCurve validation failed: 'hue_sat' curve is of the wrong BSplineType.",
    );

    // Curve y coordinates have to increase.
    let curve = GradingBSplineCurve::with_points(&points(&[
        (0.0, 0.0),
        (0.3, 0.3),
        (0.5, 0.27),
        (1.0, 1.0),
    ]));
    check_throw_what(
        create(&curve),
        "point at index 2 has a y coordinate '0.27' that is less than previous control point y \
         coordinate '0.3'.",
    );

    // Curve y coordinates have to increase. For hue-hue this includes comparing the wrapped
    // last point to the first point. In this case, 1.1 at the end is equivalent to 0.1 at the
    // start.
    let curve = GradingBSplineCurve::with_points_and_hue_curve_type(
        &points(&[(0.1, 0.05), (1.0, 1.1)]),
        HueCurveType::HueHue,
    );
    check_throw_what(
        create(&curve),
        "Control point at index 0 has a y coordinate '0.05' that is less than previous control \
         point y coordinate '0.1'.",
    );
}
