// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The `GradingToneTransform` against the wheel (`common::transforms`):
//! - its text (`repr()`, `str()`): the direction, the style, the values (`GradingTone`'s
//!   `operator<<`) with 6 significant digits (16 after a MatrixTransform in the same group,
//!   I-73), and whether it is dynamic; its validation;
//! - `setValue`'s refusals of values outside the bounds, by their messages;
//! - the raw config's processor of each, in both directions: `BuildGradingToneOp`, the op's
//!   cache ID (in the processor's), `isDynamic`, and `CreateGradingToneTransform` through
//!   `createGroupTransform()`, every getter, the values included; and the optimized
//!   processors.
//!
//! The binding has no `equals()` for this class. The transforms are built as the binding's
//! constructor builds them, `GradingToneTransform(style, dynamic, dir)`, then `setValue`
//! (src/bindings/python/transforms/PyGradingToneTransform.cpp @ v2.5.2).

mod common;

use common::transforms::{
    Case, check_optimized_processors, check_processors, check_text, direction_spec,
    grading_style_name, group, setter_errors, special_doubles,
};
use ocio::{
    GradingRgbmsw, GradingStyle, GradingTone, GradingToneTransform, MatrixTransform,
    TransformDirection,
};
use ocio_testkit::battery::BitDepth as Depth;
use ocio_testkit::transform_text::f64_spec;
use serde_json::{Value, json};

use GradingStyle::{Lin, Log, Video};
use TransformDirection::{Forward, Inverse};

/// A refused case: its label, its spec, and the port's refusal.
type Refused = (String, Value, ocio::Result<()>);

/// A `GradingRGBMSW` as an object spec.
fn rgbmsw_spec(v: &GradingRgbmsw) -> Value {
    json!({"object": {"class": "GradingRGBMSW",
        "args": [f64_spec(v.red), f64_spec(v.green), f64_spec(v.blue), f64_spec(v.master),
            f64_spec(v.start), f64_spec(v.width)]}})
}

/// A `GradingTone` of `style` as an object spec: `GradingTone(style)` and each field set.
fn tone_spec(style: GradingStyle, v: &GradingTone) -> Value {
    json!({"object": {"class": "GradingTone",
    "args": [{"enum": grading_style_name(style)}],
    "attrs": [
        ["blacks", rgbmsw_spec(&v.blacks)],
        ["shadows", rgbmsw_spec(&v.shadows)],
        ["midtones", rgbmsw_spec(&v.midtones)],
        ["highlights", rgbmsw_spec(&v.highlights)],
        ["whites", rgbmsw_spec(&v.whites)],
        ["scontrast", f64_spec(v.scontrast)],
    ]}})
}

/// A transform of `style`, dynamic or not, in the direction `dir`, given `values` (when there
/// are some): the spec, and the port's transform or `setValue`'s refusal.
fn build(
    style: GradingStyle,
    dynamic: bool,
    dir: TransformDirection,
    values: Option<&GradingTone>,
) -> (Value, ocio::Result<GradingToneTransform>) {
    let mut calls = Vec::new();
    let mut port = GradingToneTransform::new(style);
    if dynamic {
        port.make_dynamic();
    }
    port.set_direction(dir);
    let mut result = Ok(());
    if let Some(values) = values {
        calls.push(json!(["setValue", tone_spec(style, values)]));
        result = port.set_value(values);
    }
    let spec = json!({"class": "GradingToneTransform",
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
    values: Option<&GradingTone>,
) -> Case {
    let label = format!("{style:?} dynamic {dynamic} {dir:?} {values:?}");
    let (spec, port) = build(style, dynamic, dir, values);
    Case::new(label, spec, port.expect("valid values"))
}

/// Typical values of `style`, every field away from its default (upstream's
/// grading_tone_serialization values, Config_tests.cpp @ v2.5.2).
fn typical(style: GradingStyle) -> GradingTone {
    let mut v = GradingTone::new(style);
    v.blacks = GradingRgbmsw::new(0.1, 0.123_456_78, 0.9, 1.0, 0.1, 0.9);
    v.shadows = GradingRgbmsw::new(1.0, 1.1, 1.1111, 1.1, 0.9, 0.1);
    v.midtones = GradingRgbmsw::new(0.85, 0.98, 1.0, 1.11, 0.1, 0.9);
    v.highlights = GradingRgbmsw::new(1.1, 1.1111, 1.0, 1.2, 0.15, 1.1);
    v.whites = GradingRgbmsw::new(0.95, 0.96, 0.95, 1.1, 0.1, 0.9);
    v.scontrast = 1.1;
    v
}

/// The values at and past each bound of `GradingTone::validate`: the colours of each range
/// between 0.01 and 1.99 (with a tolerance), its width (or pivot) and the s-contrast; each
/// style has them all, so it refuses some and accepts others.
fn near_bound_values() -> Vec<(String, Value, ocio::Result<GradingToneTransform>)> {
    let mut out = Vec::new();
    type Setter = fn(&mut GradingTone, f64);
    let setters: [(&str, Setter); 14] = [
        ("blacks.red", |t, v| t.blacks.red = v),
        ("blacks.width", |t, v| t.blacks.width = v),
        ("shadows.green", |t, v| t.shadows.green = v),
        ("shadows.width", |t, v| t.shadows.width = v),
        ("shadows.start", |t, v| t.shadows.start = v),
        ("midtones.blue", |t, v| t.midtones.blue = v),
        ("midtones.width", |t, v| t.midtones.width = v),
        ("highlights.master", |t, v| t.highlights.master = v),
        ("highlights.width", |t, v| t.highlights.width = v),
        ("highlights.start", |t, v| t.highlights.start = v),
        ("whites.red", |t, v| t.whites.red = v),
        ("whites.width", |t, v| t.whites.width = v),
        ("whites.start", |t, v| t.whites.start = v),
        ("scontrast", |t, v| t.scontrast = v),
    ];
    let values = [
        0.0, 0.009, 0.0099, 0.009_999, 0.01, 1.9, 1.900_001, 1.91, 1.99, 2.0, -1.0, 0.5,
    ];
    for (k, (name, set)) in setters.into_iter().enumerate() {
        for v in values {
            let style = [Log, Lin, Video][k % 3];
            let mut tone = GradingTone::new(style);
            set(&mut tone, v);
            let (spec, port) = build(style, false, Forward, Some(&tone));
            out.push((format!("{style:?} {name} = {v:?}"), spec, port));
        }
    }
    out
}

/// Every style, in both directions, dynamic or not, with its defaults and typical values; each
/// field set to each special double; and [`near_bound_values`]: the values the port accepts,
/// as text cases, and those it refuses.
fn value_cases() -> (Vec<Case>, Vec<Refused>) {
    let mut cases = Vec::new();
    let mut refused = Vec::new();
    for style in [Log, Lin, Video] {
        for dir in [Forward, Inverse] {
            for dynamic in [false, true] {
                cases.push(case(style, dynamic, dir, None));
                cases.push(case(style, dynamic, dir, Some(&typical(style))));
            }
        }
    }
    type Setter = fn(&mut GradingTone, f64);
    let setters: [(&str, Setter); 7] = [
        ("blacks.start", |t, v| t.blacks.start = v),
        ("shadows.red", |t, v| t.shadows.red = v),
        ("midtones.start", |t, v| t.midtones.start = v),
        ("midtones.width", |t, v| t.midtones.width = v),
        ("highlights.blue", |t, v| t.highlights.blue = v),
        ("whites.master", |t, v| t.whites.master = v),
        ("scontrast", |t, v| t.scontrast = v),
    ];
    let mut k = 0;
    let mut sorted = Vec::new();
    for v in special_doubles() {
        for (name, set) in setters {
            let style = [Log, Lin, Video][k % 3];
            let dir = [Forward, Inverse][k / 3 % 2];
            k += 1;
            let mut values = GradingTone::new(style);
            set(&mut values, v);
            let (spec, port) = build(style, false, dir, Some(&values));
            sorted.push((format!("{style:?} {dir:?} {name} = {v:?}"), spec, port));
        }
    }
    sorted.extend(near_bound_values());
    for (label, spec, port) in sorted {
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
    let mut v = typical(Video);
    v.scontrast = 1.0 / 3.0 + 1.0;
    let gt = case(Video, true, Forward, Some(&v));
    cases.push(group(
        "gt after a matrix",
        Forward,
        &[matrix.clone(), gt.clone()],
    ));
    cases.push(group("gt before a matrix", Inverse, &[gt, matrix]));
    assert!(cases.len() > 200, "{} cases", cases.len());
    check_text(&cases, &[]);
}

#[test]
fn set_value_refusals_match_the_wheel() {
    // The values the port refuses (`setter_errors` fails on one the wheel accepts); those it
    // accepts are `text_and_validation_match_the_wheel`'s, which fails on one the wheel
    // refuses.
    let (_, refused) = value_cases();
    assert!(refused.len() > 40, "{} refused cases", refused.len());
    let checks: Vec<(&str, Value, ocio::Result<()>)> = refused
        .iter()
        .map(|(label, spec, port)| (label.as_str(), spec.clone(), port.clone()))
        .collect();
    setter_errors(&checks);
}

/// The processors' cases: each style's defaults and typical values, dynamic or not, in both
/// directions; and a transform and its inverse in a group, which the optimizer removes when
/// neither is dynamic.
fn processor_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for style in [Log, Lin, Video] {
        for dir in [Forward, Inverse] {
            for dynamic in [false, true] {
                cases.push(case(style, dynamic, dir, None));
                cases.push(case(style, dynamic, dir, Some(&typical(style))));
            }
        }
        let mut v = typical(style);
        v.scontrast = 1.000_000_01;
        v.midtones.start = 0.123_456_789_012;
        cases.push(case(style, false, Forward, Some(&v)));
        for dynamic in [false, true] {
            let fwd = case(style, dynamic, Forward, Some(&typical(style)));
            let inv = case(style, dynamic, Inverse, Some(&typical(style)));
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
