// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The cases of the sweeps through the API (`api_formats_oracle.rs`) for the ExposureContrast
//! and grading transforms, as [`Calls`] specs the port reads with `common::grading_specs`: per
//! style, the defaults and typical values, static and dynamic; and for each dynamic case, the
//! values its dynamic properties are set to in turn ([`DynSet`]), one of them refused by the
//! property's setter where the type validates.
//!
//! The values are inputs; the typical hue curves are upstream's serialization curves
//! (tests/cpu/transforms/GradingHueCurveTransform_tests.cpp @ v2.5.2).

use ocio::{
    DynamicPropertyType, GradingBSplineCurve, GradingControlPoint, GradingHueCurve, GradingPrimary,
    GradingRgbCurve, GradingRgbm, GradingRgbmsw, GradingStyle, GradingTone, HueCurveType,
    RgbCurveType,
};
use ocio_testkit::battery::params::{Case, RGB};
use serde_json::{Value, json};

use super::api::{Arg, Calls};
use super::api_cases::{Cases, RgbCurve, rgb_curve};
use super::grading_specs::{
    f64_spec, hue_curve_spec, primary_spec, rgb_curve_spec, style_spec, tone_spec,
};

use GradingStyle::{Lin, Log, Video};

/// A set of a dynamic property: its type, and the value, as a spec (`{"f64": bits}` for a
/// double, `{"object": spec}` for a grading value).
#[derive(Debug, Clone)]
pub(crate) struct DynSet {
    pub(crate) ty: DynamicPropertyType,
    pub(crate) value: Value,
}

/// Cases, and per case the sets of its dynamic properties, in order (none for a case that
/// isn't dynamic).
pub(crate) struct DynCases {
    pub(crate) cases: Cases,
    pub(crate) sets: Vec<Vec<DynSet>>,
}

impl DynCases {
    fn new() -> DynCases {
        DynCases {
            cases: Cases {
                cases: Vec::new(),
                bases: Vec::new(),
            },
            sets: Vec::new(),
        }
    }

    fn push(&mut self, label: String, calls: Calls, sets: Vec<DynSet>) {
        self.cases.cases.push(Case::new(label, calls));
        self.sets.push(sets);
    }
}

const STYLES: [GradingStyle; 3] = [Log, Lin, Video];

fn double(ty: DynamicPropertyType, v: f64) -> DynSet {
    DynSet {
        ty,
        value: f64_spec(v),
    }
}

/// An `ExposureContrastTransform` of `style` with `values` (exposure, contrast, gamma, pivot,
/// log exposure step, log mid gray), the properties `dynamic` names made dynamic.
fn ec_calls(style: &str, values: [f64; 6], dynamic: [bool; 3]) -> Calls {
    let mut calls = Calls::new("ExposureContrastTransform")
        .enumerated("setStyle", style)
        .scalar("setExposure", values[0], RGB)
        .scalar("setContrast", values[1], RGB)
        .scalar("setGamma", values[2], RGB)
        .scalar("setPivot", values[3], RGB)
        .scalar("setLogExposureStep", values[4], RGB)
        .scalar("setLogMidGray", values[5], RGB);
    for (on, name) in dynamic.into_iter().zip([
        "makeExposureDynamic",
        "makeContrastDynamic",
        "makeGammaDynamic",
    ]) {
        if on {
            calls = calls.call(name, Vec::new());
        }
    }
    calls
}

/// `ExposureContrastTransform`: per style, the defaults, typical values, and typical values
/// with every property dynamic, set in turn (an infinite exposure and a zero contrast among
/// them), or with the exposure alone dynamic.
pub(crate) fn exposure_contrast() -> DynCases {
    use DynamicPropertyType::{Contrast, Exposure, Gamma};
    let defaults = [0.0, 1.0, 1.0, 0.18, 0.088, 0.435];
    let typical = [0.5, 1.2, 0.9, 0.18, 0.1, 0.4];
    let mut out = DynCases::new();
    for style in [
        "EXPOSURE_CONTRAST_LINEAR",
        "EXPOSURE_CONTRAST_VIDEO",
        "EXPOSURE_CONTRAST_LOGARITHMIC",
    ] {
        out.push(
            format!("{style} defaults"),
            ec_calls(style, defaults, [false; 3]),
            Vec::new(),
        );
        out.push(
            format!("{style} typical"),
            ec_calls(style, typical, [false; 3]),
            Vec::new(),
        );
        out.push(
            format!("{style} all dynamic"),
            ec_calls(style, typical, [true; 3]),
            vec![
                double(Exposure, 1.25),
                double(Contrast, 0.7),
                double(Gamma, 1.4),
                double(Exposure, -3.5),
                double(Contrast, 0.0),
                double(Exposure, f64::INFINITY),
            ],
        );
        out.push(
            format!("{style} exposure dynamic"),
            ec_calls(style, typical, [true, false, false]),
            vec![double(Exposure, -0.75), double(Exposure, 2.5)],
        );
    }
    out
}

/// A grading transform of `class` and `style` with `value` (a spec), dynamic or not.
fn grading_calls(class: &'static str, style: GradingStyle, dynamic: bool, value: Value) -> Calls {
    Calls::new(class)
        .arg_fixed("style", style_spec(style))
        .arg_fixed("dynamic", json!(dynamic))
        .call("setValue", vec![Arg::Fixed(value)])
}

/// Per style, a grading class's defaults and `typical` values, static and dynamic, the
/// dynamic one set to each of `sets` in turn.
fn grading_cases(
    class: &'static str,
    ty: DynamicPropertyType,
    spec: impl Fn(GradingStyle) -> (Value, Value, Vec<Value>),
) -> DynCases {
    let mut out = DynCases::new();
    for style in STYLES {
        let (defaults, typical, sets) = spec(style);
        out.push(
            format!("{style:?} defaults"),
            grading_calls(class, style, false, defaults.clone()),
            Vec::new(),
        );
        out.push(
            format!("{style:?} typical"),
            grading_calls(class, style, false, typical.clone()),
            Vec::new(),
        );
        out.push(
            format!("{style:?} dynamic"),
            grading_calls(class, style, true, typical),
            sets.into_iter().map(|value| DynSet { ty, value }).collect(),
        );
    }
    out
}

/// Typical primary values of `style`, every field away from its default, scaled by `k`.
fn primary_values(style: GradingStyle, k: f64) -> GradingPrimary {
    let mut v = GradingPrimary::new(style);
    v.brightness = GradingRgbm::new(0.1 * k, -0.2 * k, 0.05, 0.02);
    v.contrast = GradingRgbm::new(1.1, 0.9 * k, 1.2, 1.05);
    v.gamma = GradingRgbm::new(1.3, 0.8, 1.0 * k, 1.1);
    v.offset = GradingRgbm::new(-0.01, 0.02 * k, 0.0, 0.005);
    v.exposure = GradingRgbm::new(0.5 * k, -0.25, 1.0, 0.1);
    v.lift = GradingRgbm::new(0.02, -0.01 * k, 0.03, 0.0);
    v.gain = GradingRgbm::new(1.2 * k, 0.95, 1.05, 1.0);
    v.saturation = 1.3 * k;
    v.pivot = 0.123_456_789;
    v.pivot_black = 0.05;
    v.pivot_white = 0.95;
    v.clamp_black = -0.1;
    v.clamp_white = 1.2 * k;
    v
}

/// `GradingPrimaryTransform`: its dynamic property set to other typical values, the
/// defaults, values `GradingPrimary::validate` refuses (a gamma and a contrast below 0.01),
/// then large gains and exposures.
pub(crate) fn primary() -> DynCases {
    grading_cases(
        "GradingPrimaryTransform",
        DynamicPropertyType::GradingPrimary,
        |style| {
            let spec = |v: &GradingPrimary| primary_spec(style, v);
            let mut refused = GradingPrimary::new(style);
            refused.gamma = GradingRgbm::new(0.001, 1.0, 1.0, 1.0);
            refused.contrast = GradingRgbm::new(1.0, 0.001, 1.0, 1.0);
            let mut large = primary_values(style, 1.0);
            large.gain = GradingRgbm::new(1e6, 1.0, 1e-6, 1.0);
            large.exposure = GradingRgbm::new(20.0, -20.0, 0.0, 1.0);
            large.clamp_white = GradingPrimary::NO_CLAMP_WHITE;
            large.clamp_black = GradingPrimary::NO_CLAMP_BLACK;
            (
                spec(&GradingPrimary::new(style)),
                spec(&primary_values(style, 1.0)),
                vec![
                    spec(&primary_values(style, 1.5)),
                    spec(&GradingPrimary::new(style)),
                    spec(&refused),
                    spec(&large),
                ],
            )
        },
    )
}

/// Typical tone values of `style` (those of crates/ocio/tests/grading_tone_transform_oracle.rs),
/// the s-contrast `s`.
fn tone_values(style: GradingStyle, s: f64) -> GradingTone {
    let mut v = GradingTone::new(style);
    v.blacks = GradingRgbmsw::new(0.1, 0.123_456_78, 0.9, 1.0, 0.1, 0.9);
    v.shadows = GradingRgbmsw::new(1.0, 1.1, 1.1111, 1.1, 0.9, 0.1);
    v.midtones = GradingRgbmsw::new(0.85, 0.98, 1.0, 1.11, 0.1, 0.9);
    v.highlights = GradingRgbmsw::new(1.1, 1.1111, 1.0, 1.2, 0.15, 1.1);
    v.whites = GradingRgbmsw::new(0.95, 0.96, 0.95, 1.1, 0.1, 0.9);
    v.scontrast = s;
    v
}

/// `GradingToneTransform`: its dynamic property set to other values, the defaults, and an
/// s-contrast `GradingTone::validate` refuses.
pub(crate) fn tone() -> DynCases {
    grading_cases(
        "GradingToneTransform",
        DynamicPropertyType::GradingTone,
        |style| {
            let spec = |v: &GradingTone| tone_spec(style, v);
            let mut other = tone_values(style, 1.6);
            other.midtones = GradingRgbmsw::new(1.3, 0.7, 1.0, 0.9, 0.4, 0.6);
            other.whites = GradingRgbmsw::new(1.5, 1.4, 1.3, 1.2, 0.5, 1.5);
            (
                spec(&GradingTone::new(style)),
                spec(&tone_values(style, 1.1)),
                vec![
                    spec(&other),
                    spec(&GradingTone::new(style)),
                    spec(&tone_values(style, 2.5)),
                    spec(&tone_values(style, 0.5)),
                ],
            )
        },
    )
}

/// A curve of hue curve type `t` through `points`, with `slopes` when there are some.
fn hue_bspline(points: &[(f32, f32)], slopes: &[f32], t: HueCurveType) -> GradingBSplineCurve {
    let pts: Vec<GradingControlPoint> = points
        .iter()
        .map(|&(x, y)| GradingControlPoint::new(x, y))
        .collect();
    let mut curve = GradingBSplineCurve::with_points_and_hue_curve_type(&pts, t);
    for (i, &s) in slopes.iter().enumerate() {
        curve.set_slope(i, s).expect("a slope per point");
    }
    curve
}

/// Upstream's serialization curves (GradingHueCurveTransform_tests.cpp @ v2.5.2), with slopes
/// on the sat-sat one.
fn hue_typical() -> GradingHueCurve {
    use HueCurveType::*;
    GradingHueCurve::with_curves(
        &hue_bspline(
            &[
                (0.1, -0.05),
                (0.2, 0.23),
                (0.5, 0.25),
                (0.8, 0.7),
                (0.85, 0.8),
                (0.95, 0.9),
            ],
            &[],
            HueHue,
        ),
        &hue_bspline(
            &[
                (0.0, 1.2),
                (0.1, 1.2),
                (0.4, 0.7),
                (0.6, 0.3),
                (0.8, 0.5),
                (0.9, 0.8),
            ],
            &[],
            HueSat,
        ),
        &hue_bspline(
            &[(0.1, 1.4), (0.2, 1.4), (0.4, 0.7), (0.6, 0.5), (0.8, 0.8)],
            &[],
            HueLum,
        ),
        &hue_bspline(
            &[(0.0, 1.0), (0.5, 1.5), (1.0, 0.9), (1.1, 1.1)],
            &[],
            LumSat,
        ),
        &hue_bspline(
            &[(0.0, 0.05), (0.5, 0.8), (1.0, 1.05)],
            &[1.2, 0.8, 0.4],
            SatSat,
        ),
        &hue_bspline(&[(0.0, -0.0005), (0.5, 0.3), (1.0, 0.9)], &[], LumLum),
        &hue_bspline(&[(0.05, 1.1), (0.3, 1.0), (1.2, 0.9)], &[], SatLum),
        &hue_bspline(
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
            HueFx,
        ),
    )
    .expect("valid curves")
}

/// `GradingHueCurveTransform`: its dynamic property set to the defaults, to the typical curves
/// with another hue-sat curve, and to curves whose hue-lum x coordinates decrease, which
/// `GradingBSplineCurve::validate` refuses.
pub(crate) fn hue_curve() -> DynCases {
    grading_cases(
        "GradingHueCurveTransform",
        DynamicPropertyType::GradingHueCurve,
        |style| {
            let mut other = hue_typical();
            *other.curve_mut(HueCurveType::HueSat).unwrap() = hue_bspline(
                &[(0.0, 0.8), (0.3, 1.3), (0.7, 0.6), (0.9, 1.0)],
                &[],
                HueCurveType::HueSat,
            );
            let mut refused = hue_typical();
            *refused.curve_mut(HueCurveType::HueLum).unwrap() = hue_bspline(
                &[(0.5, 1.0), (0.2, 1.2), (0.8, 0.9)],
                &[],
                HueCurveType::HueLum,
            );
            (
                hue_curve_spec(&GradingHueCurve::new(style)),
                hue_curve_spec(&hue_typical()),
                vec![
                    hue_curve_spec(&GradingHueCurve::new(style)),
                    hue_curve_spec(&other),
                    hue_curve_spec(&refused),
                    hue_curve_spec(&hue_typical()),
                ],
            )
        },
    )
}

/// `GradingRGBCurveTransform`: the dynamic cases of `api_cases::rgb_curve`, their property
/// set to the style's defaults, to the other cases' curves, and to curves whose master x
/// coordinates decrease, which `GradingBSplineCurve::validate` refuses.
pub(crate) fn rgb_curve_dynamic() -> DynCases {
    let all: Vec<(&str, RgbCurve)> = rgb_curve();
    let mut out = DynCases::new();
    let mut refused = GradingRgbCurve::new(Log);
    *refused.curve_mut(RgbCurveType::Master).unwrap() = GradingBSplineCurve::with_points(&[
        GradingControlPoint::new(0.0, 0.0),
        GradingControlPoint::new(0.7, 0.5),
        GradingControlPoint::new(0.4, 1.0),
    ]);
    for (label, c) in all.iter().filter(|(_, c)| c.is_dynamic()) {
        let mut sets = vec![rgb_curve_spec(&GradingRgbCurve::new(c.style()))];
        sets.extend(
            all.iter()
                .filter(|(_, o)| !o.is_dynamic())
                .map(|(_, o)| o.value_spec()),
        );
        sets.push(rgb_curve_spec(&refused));
        sets.push(c.value_spec());
        out.push(
            label.to_string(),
            c.calls(),
            sets.into_iter()
                .map(|value| DynSet {
                    ty: DynamicPropertyType::GradingRgbCurve,
                    value,
                })
                .collect(),
        );
    }
    out
}
