// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The grading curve value types (`GradingBSplineCurve`, `GradingRGBCurve`) against the wheel,
//! which builds them from object specs (`oracle/ocio_oracle/spec.py`, chunk O2.2) and hands
//! them to a `GradingRGBCurveTransform`:
//! - their text: the transform's `repr()` holds the curves' `operator<<`
//!   (src/OpenColorIO/transforms/GradingRGBCurveTransform.cpp:155-190 @ v2.5.2), control points
//!   and slopes as C++ `float`s with 6 significant digits;
//! - their validation: the binding's constructor sets the curves through the dynamic property,
//!   which validates them (`DynamicPropertyGradingRGBCurveImpl::setValue`,
//!   src/OpenColorIO/DynamicProperty.cpp:216-223), so the wheel raises
//!   `GradingRGBCurve::validate`'s message;
//! - whether they are identities: the processor's `createGroupTransform()` gives the curves
//!   back, and the dump calls `GradingRGBCurve.isIdentity()`.

use ocio_ops::open_color_types::{BSplineType, GradingStyle, RgbCurveType};
use ocio_ops::ops::gradingrgbcurve::grading_b_spline_curve::{
    GradingBSplineCurve, GradingControlPoint,
};
use ocio_ops::ops::gradingrgbcurve::grading_rgb_curve::GradingRgbCurve;
use ocio_testkit::assert_text_eq;
use ocio_testkit::processor_ops::ProcessorOpsRequest;
use ocio_testkit::transform_text::{Built, TransformTextRequest, f64_spec};
use serde_json::{Value, json};

/// A float as a spec value, by its bits.
fn f32_spec(v: f32) -> Value {
    f64_spec(f64::from(v))
}

/// A curve's object spec: the binding's constructor from a list of control points, then the
/// slopes (when any is set) and the spline type (when it isn't `B_SPLINE`).
fn curve_spec(curve: &GradingBSplineCurve) -> Value {
    let points = curve.control_points();
    // The binding's constructor takes at least 2 points: fewer are cut down from 2 copies.
    let given = if points.len() < 2 {
        vec![points[0]; 2]
    } else {
        points.to_vec()
    };
    let values: Vec<Value> = given
        .iter()
        .flat_map(|p| [f32_spec(p.x), f32_spec(p.y)])
        .collect();
    let mut calls = Vec::new();
    if points.len() < 2 {
        calls.push(json!(["setNumControlPoints", points.len()]));
    }
    if curve.slopes().iter().any(|s| s.to_bits() != 0) {
        let slopes: Vec<Value> = curve.slopes().iter().map(|&s| f32_spec(s)).collect();
        calls.push(json!(["setSlopes", slopes]));
    }
    let spline_type = match curve.spline_type() {
        BSplineType::BSpline => None,
        BSplineType::DiagonalBSpline => Some("DIAGONAL_B_SPLINE"),
        BSplineType::HueHueBSpline => Some("HUE_HUE_B_SPLINE"),
        BSplineType::Periodic1BSpline => Some("PERIODIC_1_B_SPLINE"),
        BSplineType::Periodic0BSpline => Some("PERIODIC_0_B_SPLINE"),
        BSplineType::Horizontal1BSpline => Some("HORIZONTAL1_B_SPLINE"),
    };
    if let Some(name) = spline_type {
        calls.push(json!(["setSplineType", {"enum": name}]));
    }
    json!({"object": {"class": "GradingBSplineCurve", "args": [values], "calls": calls}})
}

/// A `GradingRGBCurveTransform` of the four curves, log style.
fn transform_spec(curves: &GradingRgbCurve) -> Value {
    let curve = |c| curve_spec(curves.curve(c).unwrap());
    json!({"class": "GradingRGBCurveTransform", "args": {
        "values": {"object": {"class": "GradingRGBCurve", "args": {
            "red": curve(RgbCurveType::Red),
            "green": curve(RgbCurveType::Green),
            "blue": curve(RgbCurveType::Blue),
            "master": curve(RgbCurveType::Master),
        }}},
        "style": {"enum": "GRADING_LOG"},
    }})
}

fn curve(xy: &[(f32, f32)]) -> GradingBSplineCurve {
    let points: Vec<GradingControlPoint> = xy
        .iter()
        .map(|&(x, y)| GradingControlPoint::new(x, y))
        .collect();
    GradingBSplineCurve::with_points(&points)
}

fn with_slopes(mut c: GradingBSplineCurve, slopes: &[f32]) -> GradingBSplineCurve {
    for (i, &s) in slopes.iter().enumerate() {
        c.set_slope(i, s).unwrap();
    }
    c
}

/// Valid curves whose numbers print in every form `%g` has: integers, fractions of 6 and
/// more digits, exponents, negative zero, the extremes, and slopes (default, set, and -0.0,
/// which counts as default).
fn valid_curves() -> Vec<GradingRgbCurve> {
    let identity = curve(&[(0.0, 0.0), (1.0, 1.0)]);
    let odd = curve(&[
        (-f32::MAX, -f32::MAX),
        (-1234567.0, -0.0),
        (-0.0, 0.0),
        (1e-7, 1.0 / 3.0),
        (0.1, 0.40000001),
        (123456.7, 1e10),
        (f32::MAX, f32::MAX),
    ]);
    let tiny = curve(&[
        (f32::from_bits(1), f32::MIN_POSITIVE),
        (1e-30, 2e-5),
        (0.000123456, 9.99999e-5),
    ]);
    let sloped = with_slopes(
        curve(&[(0.0, 0.1), (0.5, 0.6), (1.0, 1.2)]),
        &[1.0, -0.0, 2.5e-8],
    );
    let negative_zero_slopes = with_slopes(identity.clone(), &[-0.0, -0.0]);
    vec![
        GradingRgbCurve::with_curves(&identity, &identity, &identity, &identity),
        GradingRgbCurve::with_curves(&odd, &tiny, &sloped, &negative_zero_slopes),
        GradingRgbCurve::with_curves(&sloped, &odd, &identity, &tiny),
        GradingRgbCurve::new(GradingStyle::Lin),
    ]
}

/// The curves' text is the wheel's: the transform's `repr()` holds it after `values=`.
#[test]
fn curves_print_as_the_wheel_prints_them() {
    let cases = valid_curves();
    let reply = TransformTextRequest {
        transforms: cases.iter().map(transform_spec).collect(),
        pairs: Vec::new(),
    }
    .run();
    for (curves, built) in cases.iter().zip(&reply.transforms) {
        let repr = &built.text().repr;
        let values = repr
            .split_once(", values=")
            .and_then(|(_, rest)| rest.strip_suffix('>'))
            .unwrap_or_else(|| panic!("no values in {repr}"));
        assert_text_eq("the curves' text", values, &curves.to_string());
    }
}

/// Curves that don't validate: the wheel raises `GradingRGBCurve::validate`'s message, which
/// the port's validation gives, byte for byte.
#[test]
fn invalid_curves_raise_the_wheels_messages() {
    let identity = curve(&[(0.0, 0.0), (1.0, 1.0)]);
    let one_point = curve(&[(0.5, 0.5)]);
    let x_decreases = curve(&[(0.0, 0.0), (0.30000001, 0.3), (0.1, 0.7), (1.0, 1.0)]);
    let x_below_max = curve(&[(f32::NEG_INFINITY, 0.0), (1.0, 1.0)]);
    let y_decreases = curve(&[(0.0, 0.0), (0.3, 1e-7), (0.5, -1e-7), (1.0, 1.0)]);
    let mut diagonal = identity.clone();
    diagonal.set_spline_type(BSplineType::DiagonalBSpline);
    let mut periodic = curve(&[(0.0, 0.0), (1.0, 0.5)]);
    periodic.set_spline_type(BSplineType::Periodic1BSpline);
    let i = &identity;
    let cases = [
        GradingRgbCurve::with_curves(&one_point, i, i, i),
        GradingRgbCurve::with_curves(i, &x_decreases, i, i),
        GradingRgbCurve::with_curves(i, i, &x_below_max, i),
        GradingRgbCurve::with_curves(i, i, i, &y_decreases),
        GradingRgbCurve::with_curves(&diagonal, i, i, i),
        GradingRgbCurve::with_curves(i, i, &periodic, i),
    ];
    let reply = TransformTextRequest {
        transforms: cases.iter().map(transform_spec).collect(),
        pairs: Vec::new(),
    }
    .run();
    for (curves, built) in cases.iter().zip(&reply.transforms) {
        let Built::Raised(wheel) = built else {
            panic!("the wheel accepts {curves}");
        };
        let port = curves.validate().expect_err("the port refuses it");
        assert_text_eq("the validation's message", &wheel.message, port.message());
    }
    for curves in valid_curves() {
        curves.validate().unwrap();
    }
}

/// Whether the curves are identities, as the wheel's `GradingRGBCurve.isIdentity()` says of
/// the curves the processor's group gives back.
#[test]
fn identities_are_the_wheels() {
    let identity = curve(&[(0.0, 0.0), (1.0, 1.0)]);
    let mut cases = valid_curves();
    let shifted = curve(&[(0.0, 0.0), (1.0, 1.0000001)]);
    let sloped = with_slopes(identity.clone(), &[1.0, 1.0]);
    let i = &identity;
    cases.push(GradingRgbCurve::with_curves(i, i, i, &shifted));
    cases.push(GradingRgbCurve::with_curves(i, &sloped, i, i));
    for curves in &cases {
        let mut request = ProcessorOpsRequest::new(json!({ "transform": transform_spec(curves) }));
        request.optimization = Some(json!("OPTIMIZATION_NONE"));
        let reply = request.run();
        let group = &reply.processor().group;
        let value = group.children[0].getter("getValue").object();
        let wheel = matches!(
            value.getter("isIdentity"),
            ocio_testkit::processor_ops::Dumped::Bool(true)
        );
        assert_eq!(curves.is_identity(), wheel, "{curves}");
    }
}
