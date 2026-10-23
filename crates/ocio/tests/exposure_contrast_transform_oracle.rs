// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The `ExposureContrastTransform` against the wheel (`common::transforms`):
//! - its text (`repr()`, `str()`): the direction, the style, the six values with 6 significant
//!   digits (16 after a MatrixTransform in the same group, I-73), and which values are dynamic;
//!   its validation; `equals()` between transforms that differ in each part, the dynamic
//!   states and NaN values included;
//! - the raw config's processor of each, in both directions: `BuildExposureContrastOp`, the
//!   op's cache ID (in the processor's: a dynamic value isn't in it), `isDynamic`, and
//!   `CreateExposureContrastTransform` through `createGroupTransform()`, every getter, the
//!   dynamic states included; and the optimized processors, which drop the identities that
//!   aren't dynamic and keep the dynamic ones.
//!
//! The transforms are built with the setters, in the order each case gives, as the binding's
//! constructor and setters build them
//! (src/bindings/python/transforms/PyExposureContrastTransform.cpp @ v2.5.2).

mod common;

use common::transforms::{
    Case, check_optimized_processors, check_processors, check_text, direction_spec,
    exposure_contrast_style_name, group, special_doubles,
};
use ocio::{ExposureContrastStyle, ExposureContrastTransform, MatrixTransform, TransformDirection};
use ocio_testkit::battery::BitDepth as Depth;
use ocio_testkit::transform_text::f64_spec;
use serde_json::{Value, json};

use ExposureContrastStyle::{Linear, Logarithmic, Video};
use TransformDirection::{Forward, Inverse};

/// A setter call, applied to the port's transform and written as the spec's call.
#[derive(Debug, Clone, Copy)]
enum Setter {
    Style(ExposureContrastStyle),
    Dir(TransformDirection),
    Exposure(f64),
    Contrast(f64),
    Gamma(f64),
    Pivot(f64),
    LogExposureStep(f64),
    LogMidGray(f64),
    /// `makeExposureDynamic` (true) or `makeExposureNonDynamic` (false).
    DynExposure(bool),
    /// `makeContrastDynamic` or `makeContrastNonDynamic`.
    DynContrast(bool),
    /// `makeGammaDynamic` or `makeGammaNonDynamic`.
    DynGamma(bool),
}

use Setter::*;

impl Setter {
    /// Applies the step to the port's transform.
    fn apply(self, t: &mut ExposureContrastTransform) {
        match self {
            Style(s) => t.set_style(s),
            Dir(d) => t.set_direction(d),
            Exposure(v) => t.set_exposure(v),
            Contrast(v) => t.set_contrast(v),
            Gamma(v) => t.set_gamma(v),
            Pivot(v) => t.set_pivot(v),
            LogExposureStep(v) => t.set_log_exposure_step(v),
            LogMidGray(v) => t.set_log_mid_gray(v),
            DynExposure(true) => t.make_exposure_dynamic(),
            DynExposure(false) => t.make_exposure_non_dynamic(),
            DynContrast(true) => t.make_contrast_dynamic(),
            DynContrast(false) => t.make_contrast_non_dynamic(),
            DynGamma(true) => t.make_gamma_dynamic(),
            DynGamma(false) => t.make_gamma_non_dynamic(),
        }
    }

    /// The step as a spec's call.
    fn call(self) -> Value {
        match self {
            Style(s) => json!(["setStyle", {"enum": exposure_contrast_style_name(s)}]),
            Dir(d) => json!(["setDirection", direction_spec(d)]),
            Exposure(v) => json!(["setExposure", f64_spec(v)]),
            Contrast(v) => json!(["setContrast", f64_spec(v)]),
            Gamma(v) => json!(["setGamma", f64_spec(v)]),
            Pivot(v) => json!(["setPivot", f64_spec(v)]),
            LogExposureStep(v) => json!(["setLogExposureStep", f64_spec(v)]),
            LogMidGray(v) => json!(["setLogMidGray", f64_spec(v)]),
            DynExposure(true) => json!(["makeExposureDynamic"]),
            DynExposure(false) => json!(["makeExposureNonDynamic"]),
            DynContrast(true) => json!(["makeContrastDynamic"]),
            DynContrast(false) => json!(["makeContrastNonDynamic"]),
            DynGamma(true) => json!(["makeGammaDynamic"]),
            DynGamma(false) => json!(["makeGammaNonDynamic"]),
        }
    }
}

/// A transform made by `ExposureContrastTransform()` and the steps, in order.
fn case(steps: &[Setter]) -> Case {
    let mut port = ExposureContrastTransform::new();
    for step in steps {
        step.apply(&mut port);
    }
    Case::new(
        format!("{steps:?}"),
        json!({"class": "ExposureContrastTransform",
            "calls": steps.iter().map(|s| s.call()).collect::<Vec<_>>()}),
        port,
    )
}

/// Typical values of each style, set before or after the direction, with each combination of
/// dynamic values.
fn typical_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for style in [Linear, Video, Logarithmic] {
        for dir in [Forward, Inverse] {
            for dynamic in 0..8 {
                let mut steps = vec![
                    Dir(dir),
                    Style(style),
                    Exposure(1.1),
                    Contrast(0.5),
                    Gamma(1.5),
                    Pivot(0.2),
                ];
                steps.push(DynExposure(dynamic & 1 != 0));
                steps.push(DynContrast(dynamic & 2 != 0));
                steps.push(DynGamma(dynamic & 4 != 0));
                cases.push(case(&steps));
            }
            // The style set before the direction.
            cases.push(case(&[
                Style(style),
                Dir(dir),
                Exposure(-1.4),
                LogExposureStep(0.1),
                LogMidGray(0.5),
            ]));
        }
    }
    // Dynamic, then not dynamic again; dynamic made twice.
    cases.push(case(&[
        DynExposure(true),
        DynExposure(false),
        Exposure(0.5),
    ]));
    cases.push(case(&[DynGamma(true), DynGamma(true), DynContrast(true)]));
    cases
}

/// Each value set to each special double, in a transform of each style with its other values
/// typical.
fn special_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for (k, v) in special_doubles().into_iter().enumerate() {
        let style = [Linear, Video, Logarithmic][k % 3];
        let dir = [Forward, Inverse][k % 2];
        for setter in [
            Exposure,
            Contrast,
            Gamma,
            Pivot,
            LogExposureStep,
            LogMidGray,
        ] {
            cases.push(case(&[Style(style), Dir(dir), setter(v)]));
        }
        cases.push(case(&[Style(style), Exposure(v), DynExposure(true)]));
    }
    cases
}

/// An ExposureContrastTransform after a MatrixTransform in a group, which prints it with 16
/// significant digits (I-73), and before one.
fn group_cases() -> Vec<Case> {
    let matrix = Case::new(
        "matrix",
        json!({"class": "MatrixTransform"}),
        MatrixTransform::new(),
    );
    let ec = case(&[
        Style(Video),
        Exposure(1.0 / 3.0),
        Contrast(0.123_456_789_012_345_67),
        Pivot(0.18),
        DynGamma(true),
    ]);
    vec![
        group("ec after a matrix", Forward, &[matrix.clone(), ec.clone()]),
        group("ec before a matrix", Inverse, &[ec, matrix]),
    ]
}

#[test]
fn text_validation_and_equality_match_the_wheel() {
    let mut cases = typical_cases();
    cases.extend(special_cases());
    cases.extend(group_cases());
    // Each transform with itself and its neighbours; every pair of the typical ones of the
    // linear style (they differ in direction and dynamic states).
    let mut pairs = Vec::new();
    for i in 0..cases.len() {
        pairs.push((i, i));
        if i + 1 < cases.len() {
            pairs.push((i, i + 1));
        }
    }
    for i in 0..18 {
        for j in 0..18 {
            pairs.push((i, j));
        }
    }
    // Only the ExposureContrastTransforms have `equals`.
    pairs.retain(|&(i, j)| {
        cases[i].spec["class"] == "ExposureContrastTransform"
            && cases[j].spec["class"] == "ExposureContrastTransform"
    });
    assert!(pairs.len() > 500, "{} pairs", pairs.len());
    check_text(&cases, &pairs);
}

/// The processors' cases: the typical ones, the identities (dynamic or not), values with more
/// digits than the cache ID's 7 significant ones, and pairs of a transform and its inverse,
/// which the optimizer removes when neither is dynamic.
fn processor_cases() -> Vec<Case> {
    let mut cases = typical_cases();
    cases.push(case(&[]));
    cases.push(case(&[Dir(Inverse)]));
    cases.push(case(&[DynExposure(true)]));
    cases.push(case(&[
        Style(Logarithmic),
        DynContrast(true),
        DynGamma(true),
    ]));
    cases.push(case(&[
        Style(Logarithmic),
        Exposure(1.234_567_89),
        Contrast(0.987_654_321),
        Gamma(1.000_000_01),
        Pivot(0.180_000_001),
        LogExposureStep(0.088_000_000_1),
        LogMidGray(0.435_000_1),
    ]));
    for style in [Linear, Video, Logarithmic] {
        for dynamic in [false, true] {
            let steps = [
                Style(style),
                Exposure(0.7),
                Gamma(1.2),
                DynExposure(dynamic),
            ];
            let fwd = case(&steps);
            let mut inv_steps = steps.to_vec();
            inv_steps.push(Dir(Inverse));
            let inv = case(&inv_steps);
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
