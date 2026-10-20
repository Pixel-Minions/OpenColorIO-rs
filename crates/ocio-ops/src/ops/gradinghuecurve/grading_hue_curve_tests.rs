// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of tests/cpu/ops/gradinghuecurve/GradingHueCurve_tests.cpp @ v2.5.2.
//!
//! Upstream changes curves through shared pointers and checks that the copies `Create` made
//! don't change; the curves are values here, so the copies are clones.

use super::*;
use crate::dynamic_property::DynamicPropertyGradingHueCurveImpl;
use crate::open_color_types::BSplineType;
use ocio_testkit::upstream::check_throw_what;

fn points(xy: &[(f32, f32)]) -> Vec<GradingControlPoint> {
    xy.iter()
        .map(|&(x, y)| GradingControlPoint::new(x, y))
        .collect()
}

/// Port of `OCIO_ADD_TEST(GradingHueCurve, basic)` @ v2.5.2.
#[test]
fn basic() {
    let mut curve = GradingBSplineCurve::with_points_and_hue_curve_type(
        &points(&[(0., 0.), (0.2, 0.2), (0.5, 0.7), (1., 1.)]),
        HueCurveType::HueHue,
    );
    assert_eq!(0.2f32, curve.control_point(1).unwrap().y);
    curve.control_point_mut(1).unwrap().y = 0.3;
    assert_eq!(0.3f32, curve.control_point(1).unwrap().y);
    let curve_hh = curve.clone();
    let curve_hs = GradingBSplineCurve::with_hue_curve_type(4, HueCurveType::HueSat);
    let curve_hl = GradingBSplineCurve::with_hue_curve_type(3, HueCurveType::HueLum);
    let curve_ls = GradingBSplineCurve::with_hue_curve_type(2, HueCurveType::LumSat);
    let curve_ss = GradingBSplineCurve::with_hue_curve_type(2, HueCurveType::SatSat);
    let curve_ll = GradingBSplineCurve::with_hue_curve_type(2, HueCurveType::LumLum);
    let curve_sl = GradingBSplineCurve::with_hue_curve_type(2, HueCurveType::SatLum);
    let curve_hfx = GradingBSplineCurve::with_hue_curve_type(2, HueCurveType::HueFx);

    // The Create function takes 8 curves and creates new curves that are copies of the 8
    // parameters. First try to create one using curveHH instead of curveSS.
    check_throw_what(
        GradingHueCurve::with_curves(
            &curve_hh, &curve_hs, &curve_hl, &curve_ls, &curve_hh, &curve_ll, &curve_sl, &curve_hfx,
        ),
        "GradingHueCurve validation failed: 'sat_sat' curve is of the wrong BSplineType.",
    );

    // Now create a valid one.
    let mut hue_curve = GradingHueCurve::with_curves(
        &curve_hh, &curve_hs, &curve_hl, &curve_ls, &curve_ss, &curve_ll, &curve_sl, &curve_hfx,
    )
    .unwrap();
    assert!(hue_curve.validate().is_ok());

    for c in HueCurveType::CURVES {
        assert!(hue_curve.curve(c).is_ok());
    }
    check_throw_what(
        hue_curve.curve(HueCurveType::NumCurves),
        "The HueCurveType provided is illegal",
    );

    // Validate that the Create function made copies of its curve arguments.
    assert_eq!(
        0.3f32,
        hue_curve
            .curve(HueCurveType::HueHue)
            .unwrap()
            .control_point(1)
            .unwrap()
            .y
    );
    curve.control_point_mut(1).unwrap().y = 0.4;
    assert_eq!(0.4f32, curve.control_point(1).unwrap().y);
    assert_eq!(
        0.3f32,
        hue_curve
            .curve(HueCurveType::HueHue)
            .unwrap()
            .control_point(1)
            .unwrap()
            .y
    );

    // Set the type to the wrong BSpline type and re-validate.
    hue_curve
        .curve_mut(HueCurveType::HueHue)
        .unwrap()
        .set_spline_type(BSplineType::DiagonalBSpline);
    check_throw_what(
        hue_curve.validate(),
        "GradingHueCurve validation failed: 'hue_hue' curve is of the wrong BSplineType.",
    );
    // Turn on drawCurveOnly mode and verify that any spline type is now allowed.
    hue_curve.set_draw_curve_only(true);
    assert!(hue_curve.validate().is_ok());

    // Test default curves.
    let mut hue_curve_lin = GradingHueCurve::new(GradingStyle::Lin);
    let hue_curve_log = GradingHueCurve::new(GradingStyle::Log);
    let hue_curve_video = GradingHueCurve::new(GradingStyle::Video);
    assert!(hue_curve_log == hue_curve_video);
    assert!(hue_curve_log != hue_curve_lin);

    let log = |c| hue_curve_log.curve(c).unwrap();
    assert!(log(HueCurveType::HueLum) == log(HueCurveType::HueSat));
    assert!(log(HueCurveType::SatSat) == log(HueCurveType::LumLum));
    assert!(log(HueCurveType::LumSat) == log(HueCurveType::SatLum));
    assert!(log(HueCurveType::HueHue) != log(HueCurveType::HueSat));
    let ll = log(HueCurveType::LumLum);
    assert_eq!(3, ll.num_control_points());
    assert_eq!(0.0f32, ll.control_point(0).unwrap().x);
    assert_eq!(0.0f32, ll.control_point(0).unwrap().y);
    assert_eq!(0.5f32, ll.control_point(1).unwrap().x);
    assert_eq!(0.5f32, ll.control_point(1).unwrap().y);
    assert_eq!(1.0f32, ll.control_point(2).unwrap().x);
    assert_eq!(1.0f32, ll.control_point(2).unwrap().y);

    let lin = |c| hue_curve_lin.curve(c).unwrap().clone();
    assert!(lin(HueCurveType::HueLum) == lin(HueCurveType::HueSat));
    assert!(lin(HueCurveType::SatSat) != lin(HueCurveType::LumLum));
    assert!(lin(HueCurveType::LumSat) != lin(HueCurveType::SatLum));
    assert!(lin(HueCurveType::HueHue) != lin(HueCurveType::HueSat));
    let ll = lin(HueCurveType::LumLum);
    assert_eq!(3, ll.num_control_points());
    assert_eq!(-7.0f32, ll.control_point(0).unwrap().x);
    assert_eq!(-7.0f32, ll.control_point(0).unwrap().y);
    assert_eq!(0.0f32, ll.control_point(1).unwrap().x);
    assert_eq!(0.0f32, ll.control_point(1).unwrap().y);
    assert_eq!(7.0f32, ll.control_point(2).unwrap().x);
    assert_eq!(7.0f32, ll.control_point(2).unwrap().y);

    assert!(!hue_curve_lin.get_draw_curve_only());
    hue_curve_lin.set_draw_curve_only(true);
    assert!(hue_curve_lin.get_draw_curve_only());

    // Validate that the other Create function made copies of its arguments.
    let hue_curve_lin_copy = hue_curve_lin.clone();
    // Use operator== to compare the contents of the curves.
    assert!(hue_curve_lin == hue_curve_lin_copy);
    assert!(hue_curve_lin_copy.get_draw_curve_only());

    // Test op<<.
    assert_eq!(
        "<hue_hue=<control_points=[<x=0, y=0><x=0.166667, y=0.166667>\
         <x=0.333333, y=0.333333><x=0.5, y=0.5><x=0.666667, y=0.666667><x=0.833333, y=0.833333>]>, \
         hue_sat=<control_points=[<x=0, y=1><x=0.166667, y=1><x=0.333333, y=1><x=0.5, y=1>\
         <x=0.666667, y=1><x=0.833333, y=1>]>, \
         hue_lum=<control_points=[<x=0, y=1><x=0.166667, y=1><x=0.333333, y=1><x=0.5, y=1>\
         <x=0.666667, y=1><x=0.833333, y=1>]>, \
         lum_sat=<control_points=[<x=-7, y=1><x=0, y=1><x=7, y=1>]>, \
         sat_sat=<control_points=[<x=0, y=0><x=0.5, y=0.5><x=1, y=1>]>, \
         lum_lum=<control_points=[<x=-7, y=-7><x=0, y=0><x=7, y=7>]>, \
         sat_lum=<control_points=[<x=0, y=1><x=0.5, y=1><x=1, y=1>]>, \
         hue_fx=<control_points=[<x=0, y=0><x=0.166667, y=0><x=0.333333, y=0><x=0.5, y=0>\
         <x=0.666667, y=0><x=0.833333, y=0>]>>",
        hue_curve_lin.to_string()
    );
}

/// Port of `OCIO_ADD_TEST(GradingHueCurve, curves)` @ v2.5.2.
#[test]
fn curves() {
    let mut curves = GradingHueCurve::new(GradingStyle::Video);
    assert!(curves.is_identity());
    // Use the mutable curve accessor to modify one of the spline curves.
    assert_eq!(
        curves
            .curve(HueCurveType::HueSat)
            .unwrap()
            .num_control_points(),
        6
    );
    // For this spline type, all y values must be 1. to be an identity.
    assert!(curves.is_identity());
    {
        let spline = curves.curve_mut(HueCurveType::HueSat).unwrap();
        spline.control_point_mut(3).unwrap().x = 0.9;
        spline.control_point_mut(3).unwrap().y = 1.1;
    }
    assert!(!curves.is_identity());
    curves
        .curve_mut(HueCurveType::HueSat)
        .unwrap()
        .control_point_mut(3)
        .unwrap()
        .y = 1.;
    assert!(curves.is_identity());
    curves
        .curve_mut(HueCurveType::HueSat)
        .unwrap()
        .set_num_control_points(4);
    assert_eq!(
        4,
        curves
            .curve(HueCurveType::HueSat)
            .unwrap()
            .num_control_points()
    );

    // Changing the pointer does not change the curves: a new curve value is another value.
    let _new_spline = GradingBSplineCurve::with_points(&points(&[(0., 0.), (1., 2.)]));
    assert_eq!(
        curves
            .curve(HueCurveType::HueSat)
            .unwrap()
            .num_control_points(),
        4
    );
}

/// Port of `OCIO_ADD_TEST(GradingHueCurve, max_ctrl_pnts)` @ v2.5.2.
#[test]
fn max_ctrl_pnts() {
    let mut hue_curve = GradingHueCurve::new(GradingStyle::Video);
    for c in HueCurveType::CURVES {
        // Use the mutable curve accessor to modify the curves.
        hue_curve.curve_mut(c).unwrap().set_num_control_points(28);
    }

    check_throw_what(
        DynamicPropertyGradingHueCurveImpl::new(&hue_curve, false),
        "Hue curve: maximum number of control points reached",
    );
}
