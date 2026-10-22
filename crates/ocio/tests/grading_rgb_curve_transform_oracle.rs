// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The `GradingRGBCurveTransform` against the wheel (`common::transforms`):
//! - its text (`repr()`, `str()`): the direction, the style, the curves (`GradingRGBCurve`'s
//!   `operator<<`, the slopes only when they aren't the defaults) with 6 significant digits
//!   (16 after a MatrixTransform in the same group, I-73), the lin-to-log bypass and whether it
//!   is dynamic; its validation;
//! - `setValue`'s and `setSlope`'s refusals, by their messages;
//! - the raw config's processor of each, in both directions: `BuildGradingRGBCurveOp`, the
//!   op's cache ID (in the processor's), `isDynamic`, and `CreateGradingRGBCurveTransform`
//!   through `createGroupTransform()`, every getter, the curves included; and the optimized
//!   processors.
//!
//! The binding has no `equals()` for this class. The transforms are built as the binding's
//! constructor builds them, `GradingRGBCurveTransform(style, dynamic, dir)`, then the setters
//! (src/bindings/python/transforms/PyGradingRGBCurveTransform.cpp @ v2.5.2); the curves as
//! `GradingBSplineCurve(values)`, with `setSlopes` (src/bindings/python/PyGradingData.cpp).

mod common;

use common::transforms::{
    Case, check_optimized_processors, check_processors, check_text, direction_spec,
    grading_style_name, group, setter_errors,
};
use ocio::{
    GradingBSplineCurve, GradingControlPoint, GradingRgbCurve, GradingRgbCurveTransform,
    GradingStyle, MatrixTransform, RgbCurveType, TransformDirection,
};
use ocio_testkit::battery::BitDepth as Depth;
use ocio_testkit::transform_text::f64_spec;
use serde_json::{Value, json};

use GradingStyle::{Lin, Log, Video};
use TransformDirection::{Forward, Inverse};

/// A refused case: its label, its spec, and the port's refusal.
type Refused = (String, Value, ocio::Result<()>);

/// A B-spline curve through `points`, with `slopes` when there are some.
fn curve(points: &[(f32, f32)], slopes: &[f32]) -> GradingBSplineCurve {
    let pts: Vec<GradingControlPoint> = points
        .iter()
        .map(|&(x, y)| GradingControlPoint::new(x, y))
        .collect();
    let mut c = GradingBSplineCurve::with_points(&pts);
    for (i, &s) in slopes.iter().enumerate() {
        c.set_slope(i, s).expect("a slope per point");
    }
    c
}

/// A curve as an object spec: `GradingBSplineCurve([x0, y0, x1, y1, ...])`, then `setSlopes`
/// unless the slopes are the defaults.
fn curve_spec(c: &GradingBSplineCurve) -> Value {
    let values: Vec<Value> = c
        .control_points()
        .iter()
        .flat_map(|p| [f64_spec(f64::from(p.x)), f64_spec(f64::from(p.y))])
        .collect();
    // The binding takes at least two points as values; fewer, by their number, all at (0, 0).
    let mut spec = if c.num_control_points() < 2 {
        assert!(c.control_points().iter().all(|p| p.x == 0.0 && p.y == 0.0));
        json!({"class": "GradingBSplineCurve", "args": [c.num_control_points()]})
    } else {
        json!({"class": "GradingBSplineCurve", "args": [values]})
    };
    if !c.slopes_are_default() {
        let slopes: Vec<Value> = c.slopes().iter().map(|&s| f64_spec(f64::from(s))).collect();
        spec["calls"] = json!([["setSlopes", slopes]]);
    }
    json!({"object": spec})
}

/// Curves as an object spec: `GradingRGBCurve(red, green, blue, master)`.
fn curves_spec(v: &GradingRgbCurve) -> Value {
    let c = |t| curve_spec(v.curve(t).unwrap());
    json!({"object": {"class": "GradingRGBCurve",
        "args": [c(RgbCurveType::Red), c(RgbCurveType::Green), c(RgbCurveType::Blue),
            c(RgbCurveType::Master)]}})
}

/// A transform of `style`, dynamic or not, in the direction `dir`, with the lin-to-log bypass,
/// given `values` (when there are some): the spec, and the port's transform or `setValue`'s
/// refusal.
fn build(
    style: GradingStyle,
    dynamic: bool,
    dir: TransformDirection,
    bypass: bool,
    values: Option<&GradingRgbCurve>,
) -> (Value, ocio::Result<GradingRgbCurveTransform>) {
    let mut calls = Vec::new();
    let mut port = GradingRgbCurveTransform::new(style);
    if dynamic {
        port.make_dynamic();
    }
    port.set_direction(dir);
    if bypass {
        calls.push(json!(["setBypassLinToLog", true]));
        port.set_bypass_lin_to_log(true);
    }
    let mut result = Ok(());
    if let Some(values) = values {
        calls.push(json!(["setValue", curves_spec(values)]));
        result = port.set_value(values);
    }
    let spec = json!({"class": "GradingRGBCurveTransform",
        "args": {"style": {"enum": grading_style_name(style)}, "dynamic": dynamic,
            "dir": direction_spec(dir)},
        "calls": calls});
    (spec, result.map(|()| port))
}

/// A case of a transform the port builds.
fn case(
    style: GradingStyle,
    dynamic: bool,
    dir: TransformDirection,
    bypass: bool,
    values: Option<&GradingRgbCurve>,
) -> Case {
    let label = format!("{style:?} dynamic {dynamic} {dir:?} bypass {bypass} {values:?}");
    let (spec, port) = build(style, dynamic, dir, bypass, values);
    Case::new(label, spec, port.expect("valid curves"))
}

/// Typical curves (upstream's processor_several_transforms curves,
/// GradingRGBCurveTransform_tests.cpp @ v2.5.2), with slopes on the blue one.
fn typical() -> GradingRgbCurve {
    let c1 = curve(&[(0.0, 0.0), (0.2, 0.2), (0.5, 0.7), (1.0, 1.0)], &[]);
    let c2 = curve(&[(0.0, 0.5), (0.3, 0.7), (0.5, 1.1), (1.0, 1.5)], &[]);
    let c3 = curve(
        &[
            (0.0, -0.5),
            (0.2, -0.4),
            (0.3, 0.1),
            (0.5, 0.4),
            (0.7, 0.9),
            (1.0, 1.1),
        ],
        &[0.0, 1.0, 1.1, 0.9, 0.5, 2.0],
    );
    let c4 = curve(&[(-1.0, 0.0), (0.2, 0.2), (0.8, 0.8), (2.0, 1.0)], &[]);
    GradingRgbCurve::with_curves(&c1, &c2, &c3, &c4)
}

/// Curves of special and many-digit coordinates, and of invalid ones (x decreasing, too few
/// points), each in the master curve. Not a NaN x coordinate: the wheel reads past the control
/// points fitting it (docs/improvements.md, U-35), see
/// [`a_nan_x_coordinate_is_refused`].
fn odd_curves() -> Vec<GradingRgbCurve> {
    let ident = curve(&[(0.0, 0.0), (1.0, 1.0)], &[]);
    let masters = [
        curve(&[(0.0, 0.0), (0.123_456_79, 0.987_654_3), (1.0, 1.0)], &[]),
        curve(&[(-1e-30, -0.0), (1e-30, 3.4e38), (2.0, 3.4e38)], &[]),
        curve(&[(0.0, f32::NAN), (1.0, 1.0)], &[]),
        curve(&[(0.0, f32::INFINITY), (1.0, 1.0)], &[]),
        curve(&[(f32::NEG_INFINITY, 0.0), (1.0, 1.0)], &[]),
        curve(&[(0.0, 0.0), (0.5, 0.2), (0.2, 0.7), (1.0, 1.0)], &[]),
        curve(&[(0.0, 0.0)], &[]),
        curve(&[(0.0, 0.0), (0.0, 1.0), (1.0, 1.0)], &[]),
        curve(&[(0.0, 0.0), (0.5, 0.5), (1.0, 1.0)], &[f32::NAN, 1.0, 1.0]),
        curve(&[(0.0, 0.0), (0.5, 0.5), (1.0, 1.0)], &[-0.0, 0.0, -0.0]),
        curve(&[(0.0, 0.0), (0.5, 0.5), (1.0, 1.0)], &[1e30, -1e30, 1.0]),
    ];
    masters
        .iter()
        .map(|m| GradingRgbCurve::with_curves(&ident, &ident, &ident, m))
        .collect()
}

/// Every style, in both directions, dynamic or not, with or without the bypass, with its
/// default and typical curves; and [`odd_curves`]: the ones the port accepts, as text cases,
/// and those it refuses.
fn value_cases() -> (Vec<Case>, Vec<Refused>) {
    let mut cases = Vec::new();
    let mut refused = Vec::new();
    for style in [Log, Lin, Video] {
        for dir in [Forward, Inverse] {
            for dynamic in [false, true] {
                cases.push(case(style, dynamic, dir, style == Lin, None));
                cases.push(case(style, dynamic, dir, false, Some(&typical())));
            }
        }
    }
    for (k, values) in odd_curves().iter().enumerate() {
        let style = [Log, Lin, Video][k % 3];
        let (spec, port) = build(style, false, Forward, false, Some(values));
        let label = format!("{style:?} {values:?}");
        match port {
            Ok(port) => cases.push(Case::new(label, spec, port)),
            Err(e) => refused.push((label, spec, Err(e))),
        }
    }
    (cases, refused)
}

#[test]
fn text_and_validation_match_the_wheel() {
    let (mut cases, _) = value_cases();
    let matrix = Case::new(
        "matrix",
        json!({"class": "MatrixTransform"}),
        MatrixTransform::new(),
    );
    let gc = case(Video, true, Forward, false, Some(&odd_curves()[0]));
    cases.push(group(
        "gc after a matrix",
        Forward,
        &[matrix.clone(), gc.clone()],
    ));
    cases.push(group("gc before a matrix", Inverse, &[gc, matrix]));
    assert!(cases.len() > 30, "{} cases", cases.len());
    check_text(&cases, &[]);
}

#[test]
fn set_value_and_set_slope_refusals_match_the_wheel() {
    // The curves the port refuses (`setter_errors` fails on one the wheel accepts); those it
    // accepts are `text_and_validation_match_the_wheel`'s, which fails on one the wheel
    // refuses.
    let (_, mut refused) = value_cases();
    assert!(refused.len() >= 3, "{} refused cases", refused.len());
    // `setSlope` of a point that doesn't exist.
    for (index, c) in [(3usize, RgbCurveType::Red), (100, RgbCurveType::Master)] {
        let mut port = GradingRgbCurveTransform::new(Log);
        let result = port.set_slope(c, index, 1.0);
        let curve_name = match c {
            RgbCurveType::Red => "RGB_RED",
            _ => "RGB_MASTER",
        };
        refused.push((
            format!("setSlope {c:?} {index}"),
            json!({"class": "GradingRGBCurveTransform",
                "args": {"style": {"enum": "GRADING_LOG"}},
                "calls": [["setSlope", {"enum": curve_name}, index, f64_spec(1.0)]]}),
            result,
        ));
    }
    let checks: Vec<(&str, Value, ocio::Result<()>)> = refused
        .iter()
        .map(|(label, spec, port)| (label.as_str(), spec.clone(), port.clone()))
        .collect();
    setter_errors(&checks);
}

/// The processors' cases: each style's defaults and typical curves, dynamic or not, in both
/// directions, the linear style with and without the bypass; and a transform and its inverse
/// in a group.
fn processor_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for style in [Log, Lin, Video] {
        for dir in [Forward, Inverse] {
            for dynamic in [false, true] {
                cases.push(case(style, dynamic, dir, false, None));
                cases.push(case(style, dynamic, dir, false, Some(&typical())));
            }
        }
        cases.push(case(style, false, Forward, true, Some(&typical())));
        cases.push(case(style, false, Forward, false, Some(&odd_curves()[0])));
        for dynamic in [false, true] {
            let fwd = case(style, dynamic, Forward, false, Some(&typical()));
            let inv = case(style, dynamic, Inverse, false, Some(&typical()));
            cases.push(group(
                &format!("{style:?} (dynamic {dynamic}) and its inverse"),
                Forward,
                &[fwd, inv],
            ));
        }
    }
    cases
}

#[test]
fn processors_match_the_wheel() {
    check_processors(&processor_cases());
}

#[test]
fn optimized_processors_match_the_wheel() {
    let depths = [(Depth::F32, Depth::F32), (Depth::Uint8, Depth::Uint16)];
    check_optimized_processors(&processor_cases(), &depths);
}

/// A NaN x coordinate makes the wheel read past the control points as it fits the curve
/// (docs/improvements.md, U-35): the port refuses the curves instead, with its own message.
#[test]
fn a_nan_x_coordinate_is_refused() {
    let ident = curve(&[(0.0, 0.0), (1.0, 1.0)], &[]);
    let nan_x = curve(&[(f32::NAN, 0.0), (1.0, 1.0)], &[]);
    let curves = GradingRgbCurve::with_curves(&ident, &ident, &ident, &nan_x);
    let mut port = GradingRgbCurveTransform::new(Log);
    let e = port.set_value(&curves).unwrap_err();
    assert_eq!(
        e.message(),
        "RGB curve: fitting the curve would read past its control points."
    );
}
