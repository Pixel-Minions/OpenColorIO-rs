// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The `GradingHueCurveTransform` against the wheel (`common::transforms`):
//! - its text (`repr()`, `str()`): the direction, the style, the eight curves (`GradingHueCurve`'s
//!   `operator<<`, the slopes only when they aren't the defaults) with 6 significant digits
//!   (16 after a MatrixTransform in the same group, I-73), `hsy_transform=none` and whether it
//!   is dynamic; its validation;
//! - `setValue`'s and `setSlope`'s refusals, by their messages;
//! - the raw config's processor of each, in both directions: `BuildGradingHueCurveOp`, the
//!   op's cache ID (in the processor's), `isDynamic`, and `CreateGradingHueCurveTransform`
//!   through `createGroupTransform()`, every getter, the curves included; and the optimized
//!   processors.
//!
//! The binding has no `equals()` for this class. The transforms are built as the binding's
//! constructor builds them, `GradingHueCurveTransform(style, dynamic, dir)`, then the setters
//! (src/bindings/python/transforms/PyGradingHueCurveTransform.cpp @ v2.5.2); the curves as
//! `GradingBSplineCurve(values, curveType)`, with `setSlopes`, and the values as
//! `GradingHueCurve(hue_hue, ...)` (src/bindings/python/PyGradingData.cpp).

mod common;

use common::transforms::{
    Case, check_optimized_processors, check_processors, check_text, direction_spec,
    grading_style_name, group, setter_errors,
};
use ocio::{
    GradingBSplineCurve, GradingControlPoint, GradingHueCurve, GradingHueCurveTransform,
    GradingStyle, HsyTransformStyle, HueCurveType, MatrixTransform, TransformDirection,
};
use ocio_testkit::battery::BitDepth as Depth;
use ocio_testkit::transform_text::f64_spec;
use serde_json::{Value, json};

use GradingStyle::{Lin, Log, Video};
use TransformDirection::{Forward, Inverse};

/// A refused case: its label, its spec, and the port's refusal.
type Refused = (String, Value, ocio::Result<()>);

/// The curve types, in order, with their names in PyOpenColorIO and as the curves' properties.
const CURVES: [(HueCurveType, &str, &str); 8] = [
    (HueCurveType::HueHue, "HUE_HUE", "hue_hue"),
    (HueCurveType::HueSat, "HUE_SAT", "hue_sat"),
    (HueCurveType::HueLum, "HUE_LUM", "hue_lum"),
    (HueCurveType::LumSat, "LUM_SAT", "lum_sat"),
    (HueCurveType::SatSat, "SAT_SAT", "sat_sat"),
    (HueCurveType::LumLum, "LUM_LUM", "lum_lum"),
    (HueCurveType::SatLum, "SAT_LUM", "sat_lum"),
    (HueCurveType::HueFx, "HUE_FX", "hue_fx"),
];

/// A curve of `c`'s spline type through `points`, with `slopes` when there are some.
fn curve(points: &[(f32, f32)], slopes: &[f32], c: HueCurveType) -> GradingBSplineCurve {
    let pts: Vec<GradingControlPoint> = points
        .iter()
        .map(|&(x, y)| GradingControlPoint::new(x, y))
        .collect();
    let mut curve = GradingBSplineCurve::with_points_and_hue_curve_type(&pts, c);
    for (i, &s) in slopes.iter().enumerate() {
        curve.set_slope(i, s).expect("a slope per point");
    }
    curve
}

/// A curve of the curve type named `name` as an object spec:
/// `GradingBSplineCurve([x0, y0, ...], curveType)`, then `setSlopes` unless the slopes are the
/// defaults.
fn curve_spec(c: &GradingBSplineCurve, name: &str) -> Value {
    let values: Vec<Value> = c
        .control_points()
        .iter()
        .flat_map(|p| [f64_spec(f64::from(p.x)), f64_spec(f64::from(p.y))])
        .collect();
    let mut spec = json!({"class": "GradingBSplineCurve", "args": [values, {"enum": name}]});
    if !c.slopes_are_default() {
        let slopes: Vec<Value> = c.slopes().iter().map(|&s| f64_spec(f64::from(s))).collect();
        spec["calls"] = json!([["setSlopes", slopes]]);
    }
    json!({"object": spec})
}

/// Curves as an object spec: `GradingHueCurve(hue_hue, hue_sat, ...)`, positional (the
/// binding names the arguments `arg0` to `arg7`).
fn curves_spec(v: &GradingHueCurve) -> Value {
    let args: Vec<Value> = CURVES
        .iter()
        .map(|&(t, name, _)| curve_spec(v.curve(t).unwrap(), name))
        .collect();
    json!({"object": {"class": "GradingHueCurve", "args": args}})
}

/// A transform of `style`, dynamic or not, in the direction `dir`, without the RGB to HSY
/// conversion when `no_hsy`, given `values` (when there are some): the spec, and the port's
/// transform or `setValue`'s refusal.
fn build(
    style: GradingStyle,
    dynamic: bool,
    dir: TransformDirection,
    no_hsy: bool,
    values: Option<&GradingHueCurve>,
) -> (Value, ocio::Result<GradingHueCurveTransform>) {
    let mut calls = Vec::new();
    let mut port = GradingHueCurveTransform::new(style);
    if dynamic {
        port.make_dynamic();
    }
    port.set_direction(dir);
    if no_hsy {
        calls.push(json!(["setRGBToHSY", {"enum": "HSY_TRANSFORM_NONE"}]));
        port.set_rgb_to_hsy(HsyTransformStyle::None);
    }
    let mut result = Ok(());
    if let Some(values) = values {
        calls.push(json!(["setValue", curves_spec(values)]));
        result = port.set_value(values);
    }
    let spec = json!({"class": "GradingHueCurveTransform",
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
    no_hsy: bool,
    values: Option<&GradingHueCurve>,
) -> Case {
    let label = format!("{style:?} dynamic {dynamic} {dir:?} no_hsy {no_hsy} {values:?}");
    let (spec, port) = build(style, dynamic, dir, no_hsy, values);
    Case::new(label, spec, port.expect("valid curves"))
}

/// Typical curves (upstream's serialization curves, GradingHueCurveTransform_tests.cpp @
/// v2.5.2), with slopes on the sat-sat one.
fn typical() -> GradingHueCurve {
    let hh = curve(
        &[
            (0.1, -0.05),
            (0.2, 0.23),
            (0.5, 0.25),
            (0.8, 0.7),
            (0.85, 0.8),
            (0.95, 0.9),
        ],
        &[],
        HueCurveType::HueHue,
    );
    let hs = curve(
        &[
            (0.0, 1.2),
            (0.1, 1.2),
            (0.4, 0.7),
            (0.6, 0.3),
            (0.8, 0.5),
            (0.9, 0.8),
        ],
        &[],
        HueCurveType::HueSat,
    );
    let hl = curve(
        &[(0.1, 1.4), (0.2, 1.4), (0.4, 0.7), (0.6, 0.5), (0.8, 0.8)],
        &[],
        HueCurveType::HueLum,
    );
    let ls = curve(
        &[(0.0, 1.0), (0.5, 1.5), (1.0, 0.9), (1.1, 1.1)],
        &[],
        HueCurveType::LumSat,
    );
    let ss = curve(
        &[(0.0, 0.05), (0.5, 0.8), (1.0, 1.05)],
        &[1.2, 0.8, 0.4],
        HueCurveType::SatSat,
    );
    let ll = curve(
        &[(0.0, -0.0005), (0.5, 0.3), (1.0, 0.9)],
        &[],
        HueCurveType::LumLum,
    );
    let sl = curve(
        &[(0.05, 1.1), (0.3, 1.0), (1.2, 0.9)],
        &[],
        HueCurveType::SatLum,
    );
    let hfx = curve(
        &[
            (-0.15, 0.1),
            (0.0, -0.05),
            (0.2, -0.1),
            (0.4, 0.3),
            (0.6, 0.25),
            (0.8, 0.2),
            (0.9, 0.05),
            (1.1, -0.07),
        ],
        &[],
        HueCurveType::HueFx,
    );
    GradingHueCurve::with_curves(&hh, &hs, &hl, &ls, &ss, &ll, &sl, &hfx).unwrap()
}

/// The style's default curves with curve `c` replaced: by curves of special and many-digit
/// coordinates and slopes, and of invalid ones (x decreasing, a hue-hue curve outside [0, 1],
/// a y decreasing in a diagonal curve).
fn odd_curves() -> Vec<(GradingStyle, GradingHueCurve)> {
    let replaced = |style: GradingStyle, c: HueCurveType, new: GradingBSplineCurve| {
        let mut v = GradingHueCurve::new(style);
        *v.curve_mut(c).unwrap() = new;
        (style, v)
    };
    vec![
        replaced(
            Log,
            HueCurveType::HueSat,
            curve(
                &[(0.0, 1.0), (0.123_456_79, 0.987_654_3), (0.5, 1.0)],
                &[],
                HueCurveType::HueSat,
            ),
        ),
        replaced(
            Lin,
            HueCurveType::LumLum,
            curve(
                &[(-1e-30, -0.0), (1e-30, 1e-30), (2.0, 3.4e38)],
                &[],
                HueCurveType::LumLum,
            ),
        ),
        replaced(
            Video,
            HueCurveType::SatLum,
            curve(&[(0.0, f32::NAN), (1.0, 1.0)], &[], HueCurveType::SatLum),
        ),
        replaced(
            Log,
            HueCurveType::HueFx,
            curve(
                &[(0.0, f32::INFINITY), (0.5, 0.0)],
                &[],
                HueCurveType::HueFx,
            ),
        ),
        replaced(
            Lin,
            HueCurveType::LumSat,
            curve(
                &[(0.0, 1.0), (0.5, 0.2), (0.2, 0.7), (1.0, 1.0)],
                &[],
                HueCurveType::LumSat,
            ),
        ),
        replaced(
            Video,
            HueCurveType::HueHue,
            curve(&[(0.0, 0.0), (1.5, 1.0)], &[], HueCurveType::HueHue),
        ),
        replaced(
            Log,
            HueCurveType::SatSat,
            curve(
                &[(0.0, 0.6), (0.5, 0.5), (1.0, 1.0)],
                &[],
                HueCurveType::SatSat,
            ),
        ),
        replaced(
            Lin,
            HueCurveType::SatSat,
            curve(
                &[(0.0, 0.0), (0.5, 0.5), (1.0, 1.0)],
                &[f32::NAN, 1.0, -0.0],
                HueCurveType::SatSat,
            ),
        ),
        replaced(
            Video,
            HueCurveType::HueLum,
            curve(
                &[(0.1, 1.4), (0.2, 1.4), (0.4, 0.7)],
                &[1e30, -1e30, 1.0],
                HueCurveType::HueLum,
            ),
        ),
    ]
}

/// Every style, in both directions, dynamic or not, with or without the RGB to HSY
/// conversion, with its default and typical curves; and [`odd_curves`]: the ones the port
/// accepts, as text cases, and those it refuses.
fn value_cases() -> (Vec<Case>, Vec<Refused>) {
    let mut cases = Vec::new();
    let mut refused = Vec::new();
    for style in [Log, Lin, Video] {
        for dir in [Forward, Inverse] {
            for dynamic in [false, true] {
                cases.push(case(style, dynamic, dir, dynamic, None));
                cases.push(case(style, dynamic, dir, false, Some(&typical())));
            }
        }
    }
    for (style, values) in odd_curves() {
        let (spec, port) = build(style, false, Forward, false, Some(&values));
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
    let gc = case(Video, true, Forward, false, Some(&odd_curves()[0].1));
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
    for (index, c, name) in [
        (6usize, HueCurveType::HueHue, "HUE_HUE"),
        (100, HueCurveType::SatLum, "SAT_LUM"),
    ] {
        let mut port = GradingHueCurveTransform::new(Log);
        let result = port.set_slope(c, index, 1.0);
        refused.push((
            format!("setSlope {c:?} {index}"),
            json!({"class": "GradingHueCurveTransform",
                "args": {"style": {"enum": "GRADING_LOG"}},
                "calls": [["setSlope", {"enum": name}, index, f64_spec(1.0)]]}),
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
/// directions, with and without the RGB to HSY conversion; and a transform and its inverse in
/// a group.
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
        cases.push(case(style, false, Forward, false, Some(&odd_curves()[0].1)));
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
