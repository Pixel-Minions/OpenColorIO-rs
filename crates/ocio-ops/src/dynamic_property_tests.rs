// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of the double-property tests of `tests/cpu/DynamicProperty_tests.cpp` @ v2.5.2. The
//! others need the grading properties (Phase 5) or a processor that reads CTF files.
//!
//! Upstream's `OCIO_REQUIRE_ASSERT(dp)` checks that `std::make_shared` gave a pointer; an
//! `Arc` always holds one.

use super::*;
use ocio_testkit::upstream::check_equal;

/// Port of `OCIO_ADD_TEST(DynamicPropertyImpl, basic)` @ v2.5.2.
#[test]
fn basic() {
    let dp: DynamicPropertyDoubleImplRcPtr = Arc::new(DynamicPropertyDoubleImpl::new(
        DynamicPropertyType::Exposure,
        1.0,
        false,
    ));
    check_equal(dp.get_value(), 1.0);
    dp.set_value(2.0);
    check_equal(dp.get_value(), 2.0);

    let dp_impl: DynamicPropertyDoubleImplRcPtr = Arc::new(DynamicPropertyDoubleImpl::new(
        DynamicPropertyType::Exposure,
        1.0,
        false,
    ));
    assert!(!dp_impl.is_dynamic());
    check_equal(dp_impl.get_value(), 1.0);

    dp_impl.make_dynamic();
    assert!(dp_impl.is_dynamic());
    dp_impl.set_value(2.0);
    check_equal(dp_impl.get_value(), 2.0);
}

/// Port of `OCIO_ADD_TEST(DynamicPropertyImpl, equal_double)` @ v2.5.2.
#[test]
fn equal_double() {
    let dp_impl0: DynamicPropertyDoubleImplRcPtr = Arc::new(DynamicPropertyDoubleImpl::new(
        DynamicPropertyType::Exposure,
        1.0,
        false,
    ));
    let dp0 = DynamicPropertyRcPtr::from(dp_impl0.clone());

    let dp_impl1: DynamicPropertyDoubleImplRcPtr = Arc::new(DynamicPropertyDoubleImpl::new(
        DynamicPropertyType::Exposure,
        1.0,
        false,
    ));
    let dp1 = DynamicPropertyRcPtr::from(dp_impl1.clone());

    // Both not dynamic, same value.
    assert!(dp0 == dp1);

    // Both not dynamic, diff values.
    dp_impl0.set_value(2.0);
    assert!(!(dp0 == dp1));

    // Same value.
    dp_impl1.set_value(2.0);
    assert!(dp0 == dp1);

    // One dynamic, not the other, same value.
    dp_impl0.make_dynamic();
    assert!(!(dp0 == dp1));

    // Both dynamic, same value. Equality is used to optimized, so if values are dynamic they
    // might or not be the same, but they are considered different so that they are not
    // optimized.
    dp_impl1.make_dynamic();
    assert!(!(dp0 == dp1));

    // Both dynamic, different values.
    dp_impl1.set_value(3.0);
    assert!(!(dp0 == dp1));
}

use crate::ops::gradingrgbcurve::grading_b_spline_curve::{
    GradingBSplineCurve, GradingControlPoint,
};
use crate::ops::gradingrgbcurve::grading_rgb_curve::GradingRgbCurve;
use ocio_testkit::upstream::{check_close, check_throw_what};

fn curve_of(xy: &[(f32, f32)]) -> GradingBSplineCurve {
    let points: Vec<GradingControlPoint> = xy
        .iter()
        .map(|&(x, y)| GradingControlPoint::new(x, y))
        .collect();
    GradingBSplineCurve::with_points(&points)
}

/// Port of `OCIO_ADD_TEST(DynamicPropertyImpl, equal_grading_rgb_curve)` @ v2.5.2. Upstream
/// keeps a pointer to the curve it changes; the curve is a value here, changed in place.
#[test]
fn equal_grading_rgb_curve() {
    let mut curve = GradingBSplineCurve::new(2);
    let rgb_curve = GradingRgbCurve::with_curves(&curve, &curve, &curve, &curve);

    let dp_impl0 = Arc::new(DynamicPropertyGradingRgbCurveImpl::new(&rgb_curve, false).unwrap());
    let dp0 = DynamicPropertyRcPtr::from(dp_impl0.clone());

    let dp_impl1 = Arc::new(DynamicPropertyGradingRgbCurveImpl::new(&rgb_curve, false).unwrap());
    let dp1 = DynamicPropertyRcPtr::from(dp_impl1.clone());

    // Both not dynamic, same value.
    assert!(dp0 == dp1);

    // Both not dynamic, diff values.
    curve.set_num_control_points(3);
    let rgb_curve1 = GradingRgbCurve::with_curves(&curve, &curve, &curve, &curve);

    dp_impl0.set_value(&rgb_curve1).unwrap();
    assert!(!(dp0 == dp1));

    // Same value.
    dp_impl1.set_value(&rgb_curve1).unwrap();
    assert!(dp0 == dp1);

    // One dynamic, not the other, same value.
    dp_impl0.make_dynamic();
    assert!(!(dp0 == dp1));

    // Both dynamic, same value.
    dp_impl1.make_dynamic();
    assert!(!(dp0 == dp1));

    // Both dynamic, different values.
    dp_impl1.set_value(&rgb_curve).unwrap();
    assert!(!(dp0 == dp1));

    // Different value types.
    let dp_impl_double: DynamicPropertyDoubleImplRcPtr = Arc::new(DynamicPropertyDoubleImpl::new(
        DynamicPropertyType::Exposure,
        1.0,
        true,
    ));
    assert!(!(dp0 == DynamicPropertyRcPtr::from(dp_impl_double)));
}

/// The 11-point curve of the knots and coefficients test.
fn curve11() -> GradingBSplineCurve {
    curve_of(&[
        (0.0, 10.0),
        (2.0, 10.0),
        (3.0, 10.0),
        (5.0, 10.0),
        (6.0, 10.0),
        (8.0, 10.0),
        (9.0, 10.5),
        (11.0, 15.0),
        (12.0, 50.0),
        (14.0, 60.0),
        (15.0, 85.0),
    ])
}

/// Port of `OCIO_ADD_TEST(DynamicPropertyImpl, grading_rgb_curve_knots_coefs)` @ v2.5.2.
#[test]
fn grading_rgb_curve_knots_coefs() {
    let curve11 = curve11();
    // Identity curve.
    let curve = curve_of(&[(0.0, 0.0), (1.0, 1.0)]);

    // 1 curve with 11 control points used for green.
    let curves = GradingRgbCurve::with_curves(&curve, &curve11, &curve, &curve);

    let dp = Arc::new(DynamicPropertyGradingRgbCurveImpl::new(&curves, false).unwrap());
    let state = dp.state().clone();
    let coefs_offsets = &state.knots_coefs.coefs_offsets;
    let knots_offsets = &state.knots_coefs.knots_offsets;
    check_equal(-1, coefs_offsets[0]); // Offset for red
    check_equal(0, coefs_offsets[1]); // Count for red
    check_equal(0, coefs_offsets[2]); // Offset for green
    check_equal(45, coefs_offsets[3]); // Count for green
    check_equal(-1, coefs_offsets[4]); // Offset for blue
    check_equal(0, coefs_offsets[5]); // Count for blue
    check_equal(-1, coefs_offsets[6]); // Offset for master
    check_equal(0, coefs_offsets[7]); // Count for master
    check_equal(-1, knots_offsets[0]); // Offset for red
    check_equal(0, knots_offsets[1]); // Count for red
    check_equal(0, knots_offsets[2]); // Offset for green
    check_equal(16, knots_offsets[3]); // Count for green
    check_equal(-1, knots_offsets[4]); // Offset for blue
    check_equal(0, knots_offsets[5]); // Count for blue
    check_equal(-1, knots_offsets[6]); // Offset for master
    check_equal(0, knots_offsets[7]); // Count for master
    check_equal(45, dp.get_num_coefs());
    check_equal(16, dp.get_num_knots());

    let coefs = &state.knots_coefs.coefs;
    let knots = &state.knots_coefs.knots;

    const ERROR: f32 = 1e-6;
    let expected_coefs: [f32; 45] = [
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.337645531,
        2.74714088,
        0.081863299,
        643.661987,
        17.7471409,
        -37.0891609,
        -5.69135284,
        3.83422971,
        59.0043716,
        1.69310224,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.499999881,
        1.92619848,
        2.25,
        30.9619350,
        48.7090759,
        11.6199141,
        0.237208843,
        7.90566826,
        24.9999962,
        10.0,
        10.0,
        10.0,
        10.0,
        10.0,
        10.0,
        10.1851053,
        10.5,
        14.6296263,
        15.0,
        34.9177551,
        50.0,
        55.9285622,
        60.0,
        62.3833008,
    ];
    for (k, &expected) in expected_coefs.iter().enumerate() {
        check_close(expected, coefs[k], ERROR);
    }
    let expected_knots: [f32; 16] = [
        0.0, 2.0, 3.0, 5.0, 6.0, 8.0, 8.74042130, 9.0, 10.9776964, 11.0, 11.5, 12.0, 13.0, 14.0,
        14.1448565, 15.0,
    ];
    for (k, &expected) in expected_knots.iter().enumerate() {
        check_close(expected, knots[k], ERROR);
    }

    // Using the 11 control points curve twice.
    let curves = GradingRgbCurve::with_curves(&curve11, &curve, &curve11, &curve);

    let dp2 = Arc::new(DynamicPropertyGradingRgbCurveImpl::new(&curves, false).unwrap());
    let state2 = dp2.state().clone();
    let coefs_offsets = &state2.knots_coefs.coefs_offsets;
    check_equal(0, coefs_offsets[0]); // Offset for red
    check_equal(45, coefs_offsets[1]); // Count for red
    check_equal(-1, coefs_offsets[2]); // Offset for green
    check_equal(0, coefs_offsets[3]); // Count for green
    check_equal(45, coefs_offsets[4]); // Offset for blue
    check_equal(45, coefs_offsets[5]); // Count for blue
    check_equal(-1, coefs_offsets[6]); // Offset for master
    check_equal(0, coefs_offsets[7]); // Count for master
    check_equal(90, dp2.get_num_coefs());
    check_equal(32, dp2.get_num_knots());

    let coefs2 = &state2.knots_coefs.coefs;
    let knots2 = &state2.knots_coefs.knots;
    for c in 0..45 {
        check_equal(coefs[c], coefs2[c]);
        check_equal(coefs[c], coefs2[45 + c]);
    }
    for k in 0..16 {
        check_equal(knots[k], knots2[k]);
        check_equal(knots[k], knots2[16 + k]);
    }

    // Verify that pointer does not change when setting data.
    let dp_pointer = Arc::as_ptr(&dp);
    dp.set_value(&curves).unwrap();
    check_equal(dp2.get_num_coefs(), dp.get_num_coefs());
    check_equal(dp2.get_num_knots(), dp.get_num_knots());
    check_equal(dp_pointer, Arc::as_ptr(&dp));
}

/// Port of `OCIO_ADD_TEST(GradingRGBCurve, max_ctrl_pnts)` @ v2.5.2. Upstream's test is in
/// tests/cpu/ops/gradingrgbcurve/GradingRGBCurve_tests.cpp; it needs this property.
#[test]
fn max_ctrl_pnts() {
    let xy = [
        (0.0, 10.0),
        (2.0, 10.0),
        (3.0, 10.0),
        (5.0, 10.0),
        (6.0, 10.0),
        (8.0, 10.0),
        (9.0, 10.5),
        (11.0, 15.0),
        (12.0, 50.0),
        (14.0, 60.0),
        (15.0, 85.0),
        (16.0, 86.0),
        (17.0, 87.0),
        (18.0, 88.0),
        (19.0, 89.0),
        (20.0, 90.0),
        (21.0, 91.0),
        (22.0, 92.0),
        (23.0, 93.0),
        (24.0, 94.0),
        (25.0, 95.0),
        (26.0, 96.0),
        (27.0, 97.0),
        (28.0, 98.0),
        (29.0, 99.0),
        (30.0, 100.0),
    ];
    let curve = curve_of(&xy);
    let rgb_curve = GradingRgbCurve::with_curves(&curve, &curve, &curve, &curve);

    check_throw_what(
        DynamicPropertyGradingRgbCurveImpl::new(&rgb_curve, false),
        "RGB curve: maximum number of control points reached",
    );
}

/// Port of `OCIO_ADD_TEST(DynamicPropertyImpl, equal_grading_primary)` @ v2.5.2.
#[test]
fn equal_grading_primary() {
    use crate::ops::gradingprimary::grading_primary::GradingRgbm;

    let mut gplog = GradingPrimary::new(GradingStyle::Log);
    gplog.brightness = GradingRgbm::new(-10., 45., -5., 50.);
    gplog.contrast = GradingRgbm::new(0.9, 1.4, 0.7, 0.75);
    gplog.gamma = GradingRgbm::new(1.1, 0.7, 1.05, 1.15);
    gplog.saturation = 1.21;
    gplog.pivot = -0.3;
    gplog.pivot_black = 0.05;
    gplog.pivot_white = 0.9;
    gplog.clamp_black = -0.05;
    gplog.clamp_white = 1.5;

    let dp_impl0 = Arc::new(DynamicPropertyGradingPrimaryImpl::new(
        GradingStyle::Log,
        TransformDirection::Forward,
        &gplog,
        false,
    ));
    let dp0 = DynamicPropertyRcPtr::from(Arc::clone(&dp_impl0));

    let dp_impl1 = Arc::new(DynamicPropertyGradingPrimaryImpl::new(
        GradingStyle::Log,
        TransformDirection::Forward,
        &gplog,
        false,
    ));
    let dp1 = DynamicPropertyRcPtr::from(Arc::clone(&dp_impl1));

    // Both not dynamic, same value.
    assert!(dp0 == dp1);

    // Both not dynamic, diff values.
    gplog.clamp_white = 1.4;

    dp_impl0.set_value(&gplog).unwrap();
    assert!(!(dp0 == dp1));

    // Same value.
    dp_impl1.set_value(&gplog).unwrap();
    assert!(dp0 == dp1);

    // One dynamic, not the other, same value.
    dp_impl0.make_dynamic();
    assert!(!(dp0 == dp1));

    // Both dynamic, same value.
    dp_impl1.make_dynamic();
    assert!(!(dp0 == dp1));

    // Both dynamic, different values.
    gplog.clamp_white = 1.3;
    dp_impl1.set_value(&gplog).unwrap();
    assert!(!(dp0 == dp1));

    // Different value types.
    let dp_impl_double: DynamicPropertyDoubleImplRcPtr = Arc::new(DynamicPropertyDoubleImpl::new(
        DynamicPropertyType::Exposure,
        1.0,
        true,
    ));

    assert!(!(dp0 == DynamicPropertyRcPtr::from(dp_impl_double)));
}

/// A GradingPrimary property's set validates the value for the property's style, and a
/// refused value leaves the property as it was; a set, the direction and the style (which
/// resets the values to its defaults) leave the pre-render form a property built in the end
/// state has; and a copy holds the same state. The wiring only: no expected number comes from
/// the port, and the pre-render values themselves are the wheel's dynamic uniforms
/// (`tests/grading_pre_render_oracle.rs`).
#[test]
fn grading_primary_property_state() {
    let built = |style, dir, value: &GradingPrimary| {
        DynamicPropertyGradingPrimaryImpl::new(style, dir, value, true).get_computed_value()
    };
    let dp = DynamicPropertyGradingPrimaryImpl::new(
        GradingStyle::Lin,
        TransformDirection::Forward,
        &GradingPrimary::new(GradingStyle::Lin),
        true,
    );
    let before = dp.get_computed_value();
    let mut bad = GradingPrimary::new(GradingStyle::Lin);
    bad.contrast.red = 0.;
    assert!(dp.set_value(&bad).is_err());
    assert_eq!(dp.get_value(), GradingPrimary::new(GradingStyle::Lin));
    assert_eq!(dp.get_computed_value(), before);

    let mut value = GradingPrimary::new(GradingStyle::Lin);
    value.exposure.master = 1.;
    dp.set_value(&value).unwrap();
    let forward = built(GradingStyle::Lin, TransformDirection::Forward, &value);
    assert_eq!(dp.get_computed_value(), forward);
    assert_ne!(dp.get_computed_value(), before);

    dp.set_direction(TransformDirection::Inverse);
    assert_eq!(dp.get_direction(), TransformDirection::Inverse);
    let inverse = built(GradingStyle::Lin, TransformDirection::Inverse, &value);
    assert_eq!(dp.get_computed_value(), inverse);
    assert_ne!(inverse, forward);

    let copy = dp.create_editable_copy();
    assert_eq!(copy.get_computed_value(), inverse);
    assert_eq!(copy.get_value(), value);
    assert!(copy.is_dynamic());

    // The log style's update leaves the linear style's fields as they were, as upstream's
    // does: only the log style's own compare.
    dp.set_style(GradingStyle::Log);
    assert_eq!(dp.get_value(), GradingPrimary::new(GradingStyle::Log));
    let log_fields = |pr: &GradingPrimaryPreRender| {
        (
            *pr.get_brightness(),
            *pr.get_contrast(),
            *pr.get_gamma(),
            pr.get_pivot(),
            pr.get_local_bypass(),
        )
    };
    assert_eq!(
        log_fields(&dp.get_computed_value()),
        log_fields(&built(
            GradingStyle::Log,
            TransformDirection::Inverse,
            &GradingPrimary::new(GradingStyle::Log)
        ))
    );
}

/// A GradingTone property's set validates the value, and a refused value leaves the property
/// as it was; a set and the style (which resets the values to its defaults) leave the
/// pre-render form a property built in the end state has; a copy holds the same state; and
/// equality follows the dynamic state as for the other grading properties. The wiring only, as
/// for GradingPrimary above.
#[test]
fn grading_tone_property_state() {
    let built = |value: &GradingTone, style| {
        DynamicPropertyGradingToneImpl::new(value, style, false).get_computed_value()
    };
    let dp = Arc::new(DynamicPropertyGradingToneImpl::new(
        &GradingTone::new(GradingStyle::Lin),
        GradingStyle::Lin,
        false,
    ));
    assert!(dp.get_local_bypass());
    let before = dp.get_computed_value();
    let mut bad = GradingTone::new(GradingStyle::Lin);
    bad.scontrast = 2.5;
    assert!(dp.set_value(&bad).is_err());
    assert_eq!(dp.get_value(), GradingTone::new(GradingStyle::Lin));
    assert_eq!(dp.get_computed_value(), before);

    let mut value = GradingTone::new(GradingStyle::Lin);
    value.midtones.master = 1.4;
    value.scontrast = 1.3;
    dp.set_value(&value).unwrap();
    let lin = built(&value, GradingStyle::Lin);
    assert_eq!(dp.get_computed_value(), lin);
    assert_ne!(lin, before);
    assert!(!dp.get_local_bypass());

    let copy = dp.create_editable_copy();
    assert_eq!(copy.get_computed_value(), lin);
    assert_eq!(copy.get_value(), value);
    let (dp_any, copy_any) = (
        DynamicPropertyRcPtr::from(Arc::clone(&dp)),
        DynamicPropertyRcPtr::from(Arc::clone(&copy)),
    );
    assert!(dp_any == copy_any);
    copy.make_dynamic();
    assert!(!(dp_any == copy_any));

    // An identity value only sets the bypass and leaves the other constants as they were, as
    // upstream's update does: the style's own constants (top, bottom, pivot) compare.
    dp.set_style(GradingStyle::Video);
    assert_eq!(dp.get_value(), GradingTone::new(GradingStyle::Video));
    assert!(dp.get_local_bypass());
    let style_fields = |pr: &GradingTonePreRender| (pr.top, pr.top_sc, pr.bottom, pr.pivot);
    assert_eq!(
        style_fields(&dp.get_computed_value()),
        style_fields(&built(
            &GradingTone::new(GradingStyle::Video),
            GradingStyle::Video
        ))
    );
}

/// Port of upstream's `checkKnotsAndCoefs` helper (tests/cpu/DynamicProperty_tests.cpp:
/// 451-478 @ v2.5.2): curve `set`'s knots and coefficients, within upstream's tolerances.
fn check_knots_and_coefs(
    dp: &DynamicPropertyGradingHueCurveImpl,
    set: usize,
    true_knots: &[f32],
    true_coefs_a: &[f32],
    true_coefs_b: &[f32],
    true_coefs_c: &[f32],
) {
    use ocio_testkit::upstream::check_close_f32_f64;
    let state = dp.state();
    let kc = &state.knots_coefs;
    let num_knots = kc.knots_offsets[set * 2 + 1] as usize;
    let knots_offset = kc.knots_offsets[set * 2] as usize;
    for (i, true_knot) in true_knots.iter().enumerate().take(num_knots) {
        check_close_f32_f64(kc.knots[knots_offset + i], *true_knot, 1e-6);
    }
    let num_coef_sets = (kc.coefs_offsets[set * 2 + 1] / 3) as usize;
    let offset = kc.coefs_offsets[set * 2] as usize;
    for i in 0..num_coef_sets {
        check_close_f32_f64(kc.coefs[offset + i], true_coefs_a[i], 3e-4);
        check_close_f32_f64(kc.coefs[offset + num_coef_sets + i], true_coefs_b[i], 1e-5);
        check_close_f32_f64(
            kc.coefs[offset + 2 * num_coef_sets + i],
            true_coefs_c[i],
            1e-5,
        );
    }
}

fn hue_points(xy: &[(f32, f32)]) -> Vec<GradingControlPoint> {
    xy.iter()
        .map(|&(x, y)| GradingControlPoint::new(x, y))
        .collect()
}

/// Port of `OCIO_ADD_TEST(DynamicPropertyImpl, grading_hue_curve_knots_coefs)` @ v2.5.2.
#[test]
fn grading_hue_curve_knots_coefs() {
    use crate::open_color_types::HueCurveType as H;
    let curve = |xy: &[(f32, f32)], t| {
        GradingBSplineCurve::with_points_and_hue_curve_type(&hue_points(xy), t)
    };
    let hh = curve(
        &[
            (0.1, 0.05),
            (0.2, 0.3),
            (0.5, 0.4),
            (0.8, 0.7),
            (0.9, 0.75),
            (1.0, 0.9),
        ],
        H::HueHue,
    );
    let hs = curve(
        &[
            (-0.15, 1.25),
            (0., 0.8),
            (0.2, 0.9),
            (0.4, 1.8),
            (0.6, 1.4),
            (0.8, 1.3),
            (0.9, 1.1),
            (1.1, 0.7),
        ],
        H::HueSat,
    );
    let hl = curve(
        &[
            (0., 0.),
            (0.22, 0.077),
            (0.36, 0.092),
            (0.51, 0.27),
            (0.67, 0.),
            (0.83, 0.),
        ],
        H::HueLum,
    );
    // The rest are identities, but not the default curves.
    let ls = curve(&[(0., 1.), (1., 1.)], H::LumSat);
    let ss = curve(&[(0., 0.), (0.25, 0.25), (1., 1.)], H::SatSat);
    let ll = curve(&[(0., 0.), (0.25, 0.25), (0.5, 0.5), (1., 1.)], H::LumLum);
    let sl = curve(&[(0., 1.), (0.25, 1.), (0.5, 1.), (1., 1.)], H::SatLum);
    let hfx = curve(
        &[
            (0., 0.),
            (0.1, 0.),
            (0.2, 0.),
            (0.4, 0.),
            (0.6, 0.),
            (0.8, 0.),
        ],
        H::HueFx,
    );

    let mut curves = GradingHueCurve::with_curves(&hh, &hs, &hl, &ls, &ss, &ll, &sl, &hfx).unwrap();

    {
        // Fit the polynomials.
        let dp = DynamicPropertyGradingHueCurveImpl::new(&curves, false).unwrap();

        assert_eq!(46, dp.get_num_knots());
        assert_eq!(129, dp.get_num_coefs());

        // These are offset0, count0, offset1, count1, offset2, count2, ...
        let true_knots_offsets = [0, 15, 15, 19, 34, 12, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0];
        let true_coefs_offsets = [0, 42, 42, 54, 96, 33, -1, 0, -1, 0, -1, 0, -1, 0, -1, 0];
        assert_eq!(16, DynamicPropertyGradingHueCurveImpl::NUM_OFFSET_VALUES);
        let state = dp.state();
        assert_eq!(state.knots_coefs.knots_offsets, true_knots_offsets);
        assert_eq!(state.knots_coefs.coefs_offsets, true_coefs_offsets);
    }

    // Repeat the test in DrawCurveOnly mode. This will yield identity knots and coefs for the
    // curves that are identities.
    curves.set_draw_curve_only(true);

    let dp = DynamicPropertyGradingHueCurveImpl::new(&curves, false).unwrap();

    assert_eq!(56, dp.get_num_knots());
    assert_eq!(144, dp.get_num_coefs());

    let true_knots_offsets = [0, 15, 15, 19, 34, 12, 46, 2, 48, 2, 50, 2, 52, 2, 54, 2];
    let true_coefs_offsets = [
        0, 42, 42, 54, 96, 33, 129, 3, 132, 3, 135, 3, 138, 3, 141, 3,
    ];
    {
        let state = dp.state();
        assert_eq!(state.knots_coefs.knots_offsets, true_knots_offsets);
        assert_eq!(state.knots_coefs.coefs_offsets, true_coefs_offsets);
    }

    // Hue-Hue
    check_knots_and_coefs(
        &dp,
        0,
        &[
            -0.1,
            -0.06928571,
            0.0,
            0.05642857,
            0.1,
            0.17549634,
            0.2,
            0.33714286,
            0.5,
            0.62499860,
            0.8,
            0.85261905,
            0.9,
            0.93071429,
            1.,
        ],
        &[
            15.95930233,
            -1.66237113,
            -1.44778481,
            6.17827869,
            10.39930009,
            -58.70626575,
            -1.54375789,
            1.03834397,
            3.7077401,
            -2.12344738,
            -3.54260935,
            4.81365159,
            15.95930233,
            -1.66237113,
        ],
        &[
            0.75, 1.73035714, 1.5, 1.33660714, 1.875, 3.44521825, 0.56818182, 0.14475108,
            0.48295455, 1.40987919, 0.66666667, 0.29384921, 0.75, 1.73035714,
        ],
        &[
            -0.25,
            -0.2119088,
            -0.1,
            -0.01996716,
            0.05,
            0.25082851,
            0.3,
            0.34888683,
            0.4,
            0.51830078,
            0.7,
            0.72527072,
            0.75,
            0.7880912,
        ],
    );
    // Hue-Sat
    check_knots_and_coefs(
        &dp,
        1,
        &[
            -0.1,
            -0.03071429,
            0.,
            0.0625,
            0.1,
            0.13333333,
            0.2,
            0.34913793,
            0.4,
            0.46896552,
            0.6,
            0.69,
            0.8,
            0.82770833,
            0.85,
            0.86535714,
            0.9,
            0.96928571,
            1.,
        ],
        &[
            -3.32474227,
            31.91860465,
            3.5,
            14.16666667,
            32.30769231,
            4.61538462,
            13.9662072,
            -68.17470665,
            -25.2,
            10.21052632,
            2.92592593,
            -1.78787879,
            -5.32581454,
            -12.07165109,
            -63.8372093,
            6.64948454,
            -3.32474227,
            31.91860465,
        ],
        &[
            -3.,
            -3.46071429,
            -1.5,
            -1.0625,
            0.,
            2.15384615,
            2.76923077,
            6.93501326,
            0.,
            -3.47586207,
            -0.8,
            -0.27333333,
            -0.66666667,
            -0.96180556,
            -1.5,
            -3.46071429,
            -3.,
            -3.46071429,
        ],
        &[
            1.1, 0.8761824, 0.8, 0.71992187, 0.7, 0.73589744, 0.9, 1.62363544, 1.8, 1.68014269,
            1.4, 1.3517, 1.3, 1.27743887, 1.25, 1.2119088, 1.1, 0.8761824,
        ],
    );
    // Hue-Lum: test for the "Adjust slopes that are not shape-preserving" path in
    // EstimateHueSlopes.
    check_knots_and_coefs(
        &dp,
        2,
        &[
            -0.17, 0., 0.07049104, 0.22, 0.29691485, 0.36, 0.435, 0.51, 0.59, 0.67, 0.83, 1.,
        ],
        &[
            0.,
            4.21997107,
            -1.47264319,
            -0.70657119,
            1.10402357,
            13.97025263,
            -15.20489902,
            -21.09375,
            21.09375,
            0.,
            0.,
        ],
        &[
            0., 0., 0.59494032, 0.15459362, 0.04590198, 0.18519696, 2.28073485, 0., -3.375, 0., 0.,
        ],
        &[
            0., 0., 0.02096898, 0.077, 0.08471054, 0.092, 0.18447244, 0.27, 0.135, 0., 0.,
        ],
    );
    // Horizontal identities: Lum-Sat, Sat-Lum, Hue-Fx.
    check_knots_and_coefs(&dp, 3, &[0., 1.], &[0.], &[0.], &[1.]);
    check_knots_and_coefs(&dp, 6, &[0., 1.], &[0.], &[0.], &[1.]);
    check_knots_and_coefs(&dp, 7, &[0., 1.], &[0.], &[0.], &[0.]);
    // Diagonal identities: Sat-Sat, Lum-Lum.
    check_knots_and_coefs(&dp, 4, &[0., 1.], &[0.], &[1.], &[0.]);
    check_knots_and_coefs(&dp, 5, &[0., 1.], &[0.], &[1.], &[0.]);
}

/// Port of `OCIO_ADD_TEST(DynamicPropertyImpl, get_as)` @ v2.5.2.
#[test]
fn get_as() {
    let mut gplog = GradingPrimary::new(GradingStyle::Log);
    gplog.saturation = 1.21;

    let dp_impl0 = Arc::new(DynamicPropertyGradingPrimaryImpl::new(
        GradingStyle::Log,
        TransformDirection::Forward,
        &gplog,
        false,
    ));

    let dp0 = DynamicPropertyRcPtr::from(Arc::clone(&dp_impl0));
    check_throw_what(dp0.as_double(), "Dynamic property value is not a double");
    check_throw_what(
        dp0.as_grading_tone(),
        "Dynamic property value is not a grading tone",
    );
    let as_primary = dp0.as_grading_primary().unwrap();

    assert_eq!(as_primary.get_value(), gplog);
    gplog.pivot = 0.12;
    as_primary.set_value(&gplog).unwrap();
    assert_eq!(dp_impl0.get_value(), gplog);
}
