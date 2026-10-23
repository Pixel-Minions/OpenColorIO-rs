// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The `GradingPrimaryTransform` against the wheel (`common::transforms`):
//! - its text (`repr()`, `str()`): the direction, the style, the values (`GradingPrimary`'s
//!   `operator<<`, the clamps only when they clamp) with 6 significant digits (16 after a
//!   MatrixTransform in the same group, I-73), and whether it is dynamic; its validation;
//! - `setValue`'s refusals of values that aren't valid for the style, by their messages;
//! - the raw config's processor of each, in both directions: `BuildGradingPrimaryOp`, the op's
//!   cache ID (in the processor's), `isDynamic`, and `CreateGradingPrimaryTransform` through
//!   `createGroupTransform()`, every getter, the values included; and the optimized
//!   processors.
//!
//! The binding has no `equals()` for this class. The transforms are built as the binding's
//! constructor builds them, `GradingPrimaryTransform(style, dynamic, dir)`, then `setValue`
//! and the other setters (src/bindings/python/transforms/PyGradingPrimaryTransform.cpp @
//! v2.5.2).

mod common;

use common::transforms::{
    Case, check_optimized_processors, check_processors, check_text, direction_spec,
    grading_style_name, group, setter_errors, special_doubles,
};
use ocio::{
    GradingPrimary, GradingPrimaryTransform, GradingRgbm, GradingStyle, MatrixTransform,
    TransformDirection,
};
use ocio_testkit::battery::BitDepth as Depth;
use ocio_testkit::transform_text::f64_spec;
use serde_json::{Value, json};

use GradingStyle::{Lin, Log, Video};
use TransformDirection::{Forward, Inverse};

/// A refused case: its label, its spec, and the port's refusal.
type Refused = (String, Value, ocio::Result<()>);

/// A `GradingRGBM` as an object spec.
fn rgbm_spec(v: &GradingRgbm) -> Value {
    json!({"object": {"class": "GradingRGBM",
        "args": [f64_spec(v.red), f64_spec(v.green), f64_spec(v.blue), f64_spec(v.master)]}})
}

/// A `GradingPrimary` of `style` as an object spec: `GradingPrimary(style)` and each field
/// set.
fn primary_spec(style: GradingStyle, v: &GradingPrimary) -> Value {
    json!({"object": {"class": "GradingPrimary",
    "args": [{"enum": grading_style_name(style)}],
    "attrs": [
        ["brightness", rgbm_spec(&v.brightness)],
        ["contrast", rgbm_spec(&v.contrast)],
        ["gamma", rgbm_spec(&v.gamma)],
        ["offset", rgbm_spec(&v.offset)],
        ["exposure", rgbm_spec(&v.exposure)],
        ["lift", rgbm_spec(&v.lift)],
        ["gain", rgbm_spec(&v.gain)],
        ["saturation", f64_spec(v.saturation)],
        ["pivot", f64_spec(v.pivot)],
        ["pivotBlack", f64_spec(v.pivot_black)],
        ["pivotWhite", f64_spec(v.pivot_white)],
        ["clampBlack", f64_spec(v.clamp_black)],
        ["clampWhite", f64_spec(v.clamp_white)],
    ]}})
}

/// A transform of `style`, dynamic or not, in the direction `dir`, given `values` (when there
/// are some): the spec, and the port's transform or `setValue`'s refusal.
fn build(
    style: GradingStyle,
    dynamic: bool,
    dir: TransformDirection,
    values: Option<&GradingPrimary>,
) -> (Value, ocio::Result<GradingPrimaryTransform>) {
    let mut calls = Vec::new();
    let mut port = GradingPrimaryTransform::new(style);
    if dynamic {
        port.make_dynamic();
    }
    port.set_direction(dir);
    let mut result = Ok(());
    if let Some(values) = values {
        calls.push(json!(["setValue", primary_spec(style, values)]));
        result = port.set_value(values);
    }
    let spec = json!({"class": "GradingPrimaryTransform",
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
    values: Option<&GradingPrimary>,
) -> Case {
    let label = format!("{style:?} dynamic {dynamic} {dir:?} {values:?}");
    let (spec, port) = build(style, dynamic, dir, values);
    Case::new(label, spec, port.expect("valid values"))
}

/// Typical values of `style`, every field away from its default.
fn typical(style: GradingStyle) -> GradingPrimary {
    let mut v = GradingPrimary::new(style);
    v.brightness = GradingRgbm::new(0.1, -0.2, 0.05, 0.02);
    v.contrast = GradingRgbm::new(1.1, 0.9, 1.2, 1.05);
    v.gamma = GradingRgbm::new(1.3, 0.8, 1.0, 1.1);
    v.offset = GradingRgbm::new(-0.01, 0.02, 0.0, 0.005);
    v.exposure = GradingRgbm::new(0.5, -0.25, 1.0, 0.1);
    v.lift = GradingRgbm::new(0.02, -0.01, 0.03, 0.0);
    v.gain = GradingRgbm::new(1.2, 0.95, 1.05, 1.0);
    v.saturation = 1.3;
    v.pivot = 0.123_456_789;
    v.pivot_black = 0.05;
    v.pivot_white = 0.95;
    v.clamp_black = -0.1;
    v.clamp_white = 1.2;
    v
}

/// Every style, in both directions, dynamic or not, with its defaults and typical values; each
/// field of each style set to each special double, the valid ones here and the refused ones in
/// [`near_bound_values`]: the values the port accepts, as text cases, and those it refuses.
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
    type Setter = fn(&mut GradingPrimary, f64);
    let setters: [(&str, Setter); 12] = [
        ("brightness.red", |p, v| p.brightness.red = v),
        ("contrast.green", |p, v| p.contrast.green = v),
        ("gamma.blue", |p, v| p.gamma.blue = v),
        ("gamma.master", |p, v| p.gamma.master = v),
        ("offset.master", |p, v| p.offset.master = v),
        ("exposure.red", |p, v| p.exposure.red = v),
        ("lift.green", |p, v| p.lift.green = v),
        ("gain.blue", |p, v| p.gain.blue = v),
        ("saturation", |p, v| p.saturation = v),
        ("pivot", |p, v| p.pivot = v),
        ("pivotBlack", |p, v| p.pivot_black = v),
        ("clampWhite", |p, v| p.clamp_white = v),
    ];
    let mut k = 0;
    for v in special_doubles() {
        for (name, set) in setters {
            let style = [Log, Lin, Video][k % 3];
            let dir = [Forward, Inverse][k / 3 % 2];
            k += 1;
            let mut values = GradingPrimary::new(style);
            set(&mut values, v);
            let (spec, port) = build(style, false, dir, Some(&values));
            let label = format!("{style:?} {dir:?} {name} = {v:?}");
            match port {
                Ok(port) => cases.push(Case::new(label, spec, port)),
                Err(e) => refused.push((label, spec, Err(e))),
            }
        }
    }
    for (label, spec, port) in near_bound_values() {
        match port {
            Ok(port) => cases.push(Case::new(label, spec, port)),
            Err(e) => refused.push((label, spec, Err(e))),
        }
    }
    (cases, refused)
}

/// Values at and past the bounds of each style's checks: gamma (log and video) or contrast
/// (linear) below 0.01, the white pivot not above the black one, the black clamp above the
/// white one; each style has them all, so it refuses some and accepts others.
fn near_bound_values() -> Vec<(String, Value, ocio::Result<GradingPrimaryTransform>)> {
    let mut out = Vec::new();
    for style in [Log, Lin, Video] {
        let mut bad = Vec::new();
        let mut v = GradingPrimary::new(style);
        v.gamma.green = 0.00999;
        bad.push(v);
        let mut v = GradingPrimary::new(style);
        v.gamma.master = 0.009999;
        bad.push(v);
        let mut v = GradingPrimary::new(style);
        v.gamma.red = 0.009998;
        bad.push(v);
        let mut v = GradingPrimary::new(style);
        v.contrast.blue = 0.0099;
        bad.push(v);
        let mut v = GradingPrimary::new(style);
        v.contrast = GradingRgbm::new(-1.0, 2.0, 3.0, 0.0);
        bad.push(v);
        let mut v = GradingPrimary::new(style);
        v.pivot_white = 0.5;
        v.pivot_black = 0.49;
        bad.push(v);
        let mut v = GradingPrimary::new(style);
        v.pivot_white = 0.5;
        v.pivot_black = 0.490_001;
        bad.push(v);
        let mut v = GradingPrimary::new(style);
        v.clamp_black = 1.0;
        v.clamp_white = 0.999;
        bad.push(v);
        let mut v = GradingPrimary::new(style);
        v.gamma = GradingRgbm::new(0.001, 0.002, 0.003, 0.004);
        v.contrast = GradingRgbm::new(0.001, 0.002, 0.003, 0.004);
        v.pivot_black = 2.0;
        v.clamp_black = 3.0;
        v.clamp_white = 2.0;
        bad.push(v);
        for v in bad {
            let (spec, port) = build(style, false, Forward, Some(&v));
            out.push((format!("{style:?} {v:?}"), spec, port));
        }
    }
    out
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
    v.saturation = 1.0 / 3.0;
    let gp = case(Video, true, Forward, Some(&v));
    cases.push(group(
        "gp after a matrix",
        Forward,
        &[matrix.clone(), gp.clone()],
    ));
    cases.push(group("gp before a matrix", Inverse, &[gp, matrix]));
    assert!(cases.len() > 300, "{} cases", cases.len());
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
        v.pivot = 0.123_456_789_012;
        v.saturation = 1.000_000_01;
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
