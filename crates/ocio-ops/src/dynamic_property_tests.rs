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
