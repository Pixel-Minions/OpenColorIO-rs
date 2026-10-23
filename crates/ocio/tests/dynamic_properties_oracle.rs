// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The processors' dynamic properties of every type against the wheel, end to end: who shares
//! a property with whom, through `Config::getProcessor`, `Processor`, `CPUProcessor` and
//! `GpuShaderDesc` (WP 5.6a).
//!
//! - `sharing_matches_the_wheel` (`object_calls`): per kind of transform, one script of calls
//!   on both sides. The kinds: each dynamic property type (exposure, contrast, gamma, grading
//!   primary, RGB curve, tone and hue curve); two ops of one type in a group (exposure,
//!   contrast, gamma, primary and tone, each type with its own warning, and a description
//!   that refuses the second property); and several properties in one description (the order
//!   of its list). The script: the processor's property, which its optimized processors
//!   share; CPU processors and GPU descriptions, which take a copy when they are made; the
//!   CPU processor cache, which hands out a made CPU processor again unless the config's flags
//!   don't share dynamic properties (`PROCESSOR_CACHE_SHARE_DYN_PROPERTIES`); the config's
//!   cache fallback, which hands out a cached processor of the same cache ID, with the values
//!   set on it; `OPTIMIZATION_NO_DYNAMIC_PROPERTIES` alone, on the optimized, CPU and GPU
//!   processors; and the properties a description holds, counted and read by index. Each
//!   call's result (processors as `processor_ops` writes them, the group of their ops
//!   included, CPU and GPU processors by their getters, property values, pixels by their
//!   bits), exception and log.
//! - `uniforms_and_pixels_follow_their_own_property` (`dynamic_apply`): a processor's CPU
//!   processor and GPU description, each property taken from the processor, the CPU processor
//!   and the description, set in turn, with the CPU processor's pixels and the description's
//!   uniforms after each set.
//!
//! Every value is an input: the expected values are the wheel's.

mod common;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use common::transforms::{
    compare_processor, dump_grading_hue_curve, dump_grading_primary, dump_grading_rgb_curve,
    dump_grading_tone, dumped_enum, grading_style_name,
};
use ocio::{
    BitDepth, Config, DynamicPropertyType, ExposureContrastStyle, ExposureContrastTransform,
    GradingBSplineCurve, GradingControlPoint, GradingHueCurve, GradingHueCurveTransform,
    GradingPrimary, GradingPrimaryTransform, GradingRgbCurve, GradingRgbCurveTransform,
    GradingRgbm, GradingRgbmsw, GradingStyle, GradingTone, GradingToneTransform, GroupTransform,
    HueCurveType, OptimizationFlags, Processor, ProcessorCacheFlags, Transform,
};
use ocio_gpu::gpu_processor::GpuProcessor;
use ocio_gpu::gpu_shader::UniformData;
use ocio_gpu::{GpuLanguage, GpuShaderDesc};
use ocio_ops::cpu_processor::CpuProcessor;
use ocio_ops::dynamic_property::DynamicPropertyRcPtr;
use ocio_ops::image_desc::PackedImageDesc;
use ocio_ops::logging::{reset_to_default_logging_function, set_logging_function};
use ocio_testkit::Oracle;
use ocio_testkit::oracle_values::{bytes, log};
use ocio_testkit::processor_ops::{Dumped, ProcessorDump};
use ocio_testkit::transform_text::f64_spec;
use serde_json::{Value, json};

/// The tests of this file swap the logging function: one at a time.
static SERIAL: Mutex<()> = Mutex::new(());

/// What `f` logs on the port's side, each message with its line feed.
fn capture<T>(f: impl FnOnce() -> T) -> (T, Vec<Vec<u8>>) {
    let messages = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&messages);
    set_logging_function(Some(Arc::new(move |m: &[u8]| {
        sink.lock().unwrap().push(m.to_vec());
    })))
    .unwrap();
    let out = f();
    reset_to_default_logging_function();
    let log = messages.lock().unwrap().clone();
    (out, log)
}

// ---------------------------------------------------------------------------------------------
// Values

/// A value of a dynamic property.
#[derive(Debug, Clone)]
enum DynValue {
    Double(f64),
    Primary(GradingPrimary),
    RgbCurve(GradingRgbCurve),
    Tone(GradingTone),
    HueCurve(GradingHueCurve),
}

fn rgbm_spec(v: &GradingRgbm) -> Value {
    json!({"object": {"class": "GradingRGBM",
        "args": [f64_spec(v.red), f64_spec(v.green), f64_spec(v.blue), f64_spec(v.master)]}})
}

fn rgbmsw_spec(v: &GradingRgbmsw) -> Value {
    json!({"object": {"class": "GradingRGBMSW",
        "args": [f64_spec(v.red), f64_spec(v.green), f64_spec(v.blue), f64_spec(v.master),
            f64_spec(v.start), f64_spec(v.width)]}})
}

/// A curve as an object spec: `GradingBSplineCurve([x0, y0, x1, y1, ...])`, of the hue curve
/// type named `hue_type` when there is one.
fn curve_spec(c: &GradingBSplineCurve, hue_type: Option<&str>) -> Value {
    let values: Vec<Value> = c
        .control_points()
        .iter()
        .flat_map(|p| [f64_spec(f64::from(p.x)), f64_spec(f64::from(p.y))])
        .collect();
    assert!(c.slopes_are_default());
    match hue_type {
        Some(t) => json!({"object": {"class": "GradingBSplineCurve",
            "args": [values, {"enum": t}]}}),
        None => json!({"object": {"class": "GradingBSplineCurve", "args": [values]}}),
    }
}

impl DynValue {
    /// The value as an object spec (`{"f64": bits}` for a double), the grading values of
    /// `style`.
    fn spec(&self, style: GradingStyle) -> Value {
        let style_arg = json!({"enum": grading_style_name(style)});
        match self {
            DynValue::Double(v) => f64_spec(*v),
            DynValue::Primary(v) => json!({"object": {"class": "GradingPrimary",
            "args": [style_arg],
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
            ]}}),
            DynValue::RgbCurve(v) => {
                let c = |t| curve_spec(v.curve(t).unwrap(), None);
                use ocio::RgbCurveType::{Blue, Green, Master, Red};
                json!({"object": {"class": "GradingRGBCurve",
                    "args": [c(Red), c(Green), c(Blue), c(Master)]}})
            }
            DynValue::Tone(v) => json!({"object": {"class": "GradingTone",
            "args": [style_arg],
            "attrs": [
                ["blacks", rgbmsw_spec(&v.blacks)],
                ["shadows", rgbmsw_spec(&v.shadows)],
                ["midtones", rgbmsw_spec(&v.midtones)],
                ["highlights", rgbmsw_spec(&v.highlights)],
                ["whites", rgbmsw_spec(&v.whites)],
                ["scontrast", f64_spec(v.scontrast)],
            ]}}),
            DynValue::HueCurve(v) => {
                let args: Vec<Value> = HUE_CURVES
                    .iter()
                    .map(|&(t, name)| curve_spec(v.curve(t).unwrap(), Some(name)))
                    .collect();
                json!({"object": {"class": "GradingHueCurve", "args": args}})
            }
        }
    }

    /// The property's setter in PyOpenColorIO.
    fn setter(&self) -> &'static str {
        match self {
            DynValue::Double(_) => "setDouble",
            DynValue::Primary(_) => "setGradingPrimary",
            DynValue::RgbCurve(_) => "setGradingRGBCurve",
            DynValue::Tone(_) => "setGradingTone",
            DynValue::HueCurve(_) => "setGradingHueCurve",
        }
    }

    /// Sets the port's property to the value.
    fn set(&self, prop: &DynamicPropertyRcPtr) -> ocio::Result<()> {
        match self {
            DynValue::Double(v) => {
                prop.as_double()?.set_value(*v);
                Ok(())
            }
            DynValue::Primary(v) => prop.as_grading_primary()?.set_value(v),
            DynValue::RgbCurve(v) => prop.as_grading_rgb_curve()?.set_value(v),
            DynValue::Tone(v) => prop.as_grading_tone()?.set_value(v),
            DynValue::HueCurve(v) => prop.as_grading_hue_curve()?.set_value(v),
        }
    }
}

/// The hue curve types in the order `GradingHueCurve`'s constructor takes them, with their
/// names in PyOpenColorIO.
const HUE_CURVES: [(HueCurveType, &str); 8] = [
    (HueCurveType::HueHue, "HUE_HUE"),
    (HueCurveType::HueSat, "HUE_SAT"),
    (HueCurveType::HueLum, "HUE_LUM"),
    (HueCurveType::LumSat, "LUM_SAT"),
    (HueCurveType::SatSat, "SAT_SAT"),
    (HueCurveType::LumLum, "LUM_LUM"),
    (HueCurveType::SatLum, "SAT_LUM"),
    (HueCurveType::HueFx, "HUE_FX"),
];

/// The getter of a property type in PyOpenColorIO.
fn getter(ty: DynamicPropertyType) -> &'static str {
    match ty {
        DynamicPropertyType::Exposure
        | DynamicPropertyType::Contrast
        | DynamicPropertyType::Gamma => "getDouble",
        DynamicPropertyType::GradingPrimary => "getGradingPrimary",
        DynamicPropertyType::GradingRgbCurve => "getGradingRGBCurve",
        DynamicPropertyType::GradingTone => "getGradingTone",
        DynamicPropertyType::GradingHueCurve => "getGradingHueCurve",
    }
}

/// The name of a property type in PyOpenColorIO.
fn type_name(ty: DynamicPropertyType) -> &'static str {
    match ty {
        DynamicPropertyType::Exposure => "DYNAMIC_PROPERTY_EXPOSURE",
        DynamicPropertyType::Contrast => "DYNAMIC_PROPERTY_CONTRAST",
        DynamicPropertyType::Gamma => "DYNAMIC_PROPERTY_GAMMA",
        DynamicPropertyType::GradingPrimary => "DYNAMIC_PROPERTY_GRADING_PRIMARY",
        DynamicPropertyType::GradingRgbCurve => "DYNAMIC_PROPERTY_GRADING_RGBCURVE",
        DynamicPropertyType::GradingTone => "DYNAMIC_PROPERTY_GRADING_TONE",
        DynamicPropertyType::GradingHueCurve => "DYNAMIC_PROPERTY_GRADING_HUECURVE",
    }
}

/// The port's property value of type `ty`, as `checks.dump` writes the getter's result.
fn read(prop: &DynamicPropertyRcPtr, ty: DynamicPropertyType) -> ocio::Result<Dumped> {
    Ok(match ty {
        DynamicPropertyType::Exposure
        | DynamicPropertyType::Contrast
        | DynamicPropertyType::Gamma => Dumped::F64(prop.as_double()?.get_value()),
        DynamicPropertyType::GradingPrimary => {
            dump_grading_primary(&prop.as_grading_primary()?.get_value())
        }
        DynamicPropertyType::GradingRgbCurve => {
            dump_grading_rgb_curve(&prop.as_grading_rgb_curve()?.get_value())
        }
        DynamicPropertyType::GradingTone => dump_grading_tone(&prop.as_grading_tone()?.get_value()),
        DynamicPropertyType::GradingHueCurve => {
            dump_grading_hue_curve(&prop.as_grading_hue_curve()?.get_value())
        }
    })
}

// ---------------------------------------------------------------------------------------------
// Cases

/// A transform with a dynamic property of type `ty`: the wheel's spec and the port's
/// transform, built from an initial value.
#[derive(Debug, Clone)]
struct Kind {
    label: &'static str,
    ty: DynamicPropertyType,
    style: GradingStyle,
    /// The transform of an initial value: `transform(&values[0])` is the case's transform,
    /// `transform(&values[3])` another of the same cache ID, for the cache fallback.
    transform: fn(&DynValue) -> (Value, Transform),
    /// Four values: the transform's, two to set, and the other transform's.
    values: [DynValue; 4],
    /// Two ops with a dynamic property of the same type: the processor keeps the first one's,
    /// and the GPU description takes the first one's and refuses the second one's.
    duplicates: bool,
    /// How many dynamic properties the GPU description holds after extracting: the properties
    /// the script reads one by one, after comparing the wheel's own count.
    gpu_props: usize,
}

/// An exposure and contrast transform of `style` with the value `v` set for `ty`, and the
/// properties flagged in `dynamic` (exposure, contrast, gamma) made dynamic.
fn ec_build(
    style: ExposureContrastStyle,
    style_name: &str,
    ty: DynamicPropertyType,
    v: f64,
    dynamic: [bool; 3],
) -> (Value, Transform) {
    let mut t = ExposureContrastTransform::new();
    t.set_style(style);
    t.set_pivot(0.18);
    let mut calls = vec![
        json!(["setStyle", {"enum": style_name}]),
        json!(["setPivot", f64_spec(0.18)]),
    ];
    match ty {
        DynamicPropertyType::Exposure => {
            t.set_exposure(v);
            calls.push(json!(["setExposure", f64_spec(v)]));
        }
        DynamicPropertyType::Contrast => {
            t.set_contrast(v);
            calls.push(json!(["setContrast", f64_spec(v)]));
        }
        _ => {
            t.set_gamma(v);
            calls.push(json!(["setGamma", f64_spec(v)]));
        }
    }
    if dynamic[0] {
        t.make_exposure_dynamic();
        calls.push(json!(["makeExposureDynamic"]));
    }
    if dynamic[1] {
        t.make_contrast_dynamic();
        calls.push(json!(["makeContrastDynamic"]));
    }
    if dynamic[2] {
        t.make_gamma_dynamic();
        calls.push(json!(["makeGammaDynamic"]));
    }
    (
        json!({"class": "ExposureContrastTransform", "calls": calls}),
        Transform::from(t),
    )
}

/// An exposure and contrast transform whose one property `ty` is dynamic.
fn ec_transform(
    style: ExposureContrastStyle,
    style_name: &str,
    ty: DynamicPropertyType,
    v: f64,
) -> (Value, Transform) {
    let dynamic = [
        ty == DynamicPropertyType::Exposure,
        ty == DynamicPropertyType::Contrast,
        ty == DynamicPropertyType::Gamma,
    ];
    ec_build(style, style_name, ty, v, dynamic)
}

/// The transforms in a group, in order.
fn group(parts: Vec<(Value, Transform)>) -> (Value, Transform) {
    let mut g = GroupTransform::new();
    let mut specs = Vec::new();
    for (spec, t) in parts {
        specs.push(spec);
        g.append_transform(t);
    }
    (
        json!({"class": "GroupTransform", "children": specs}),
        Transform::from(g),
    )
}

fn double(v: &DynValue) -> f64 {
    match v {
        DynValue::Double(v) => *v,
        other => panic!("not a double: {other:?}"),
    }
}

fn exposure_transform(v: &DynValue) -> (Value, Transform) {
    ec_transform(
        ExposureContrastStyle::Linear,
        "EXPOSURE_CONTRAST_LINEAR",
        DynamicPropertyType::Exposure,
        double(v),
    )
}

fn contrast_transform(v: &DynValue) -> (Value, Transform) {
    ec_transform(
        ExposureContrastStyle::Video,
        "EXPOSURE_CONTRAST_VIDEO",
        DynamicPropertyType::Contrast,
        double(v),
    )
}

fn gamma_transform(v: &DynValue) -> (Value, Transform) {
    ec_transform(
        ExposureContrastStyle::Logarithmic,
        "EXPOSURE_CONTRAST_LOGARITHMIC",
        DynamicPropertyType::Gamma,
        double(v),
    )
}

/// Two dynamic exposures in a group: the first one's property is the processor's, and the
/// processor logs that the property can only be there once.
fn two_exposures_transform(v: &DynValue) -> (Value, Transform) {
    group(vec![
        exposure_transform(v),
        exposure_transform(&DynValue::Double(0.25)),
    ])
}

/// Two dynamic contrasts in a group.
fn two_contrasts_transform(v: &DynValue) -> (Value, Transform) {
    group(vec![
        contrast_transform(v),
        contrast_transform(&DynValue::Double(1.3)),
    ])
}

/// Two dynamic gammas in a group.
fn two_gammas_transform(v: &DynValue) -> (Value, Transform) {
    group(vec![
        gamma_transform(v),
        gamma_transform(&DynValue::Double(1.3)),
    ])
}

/// Two dynamic grading primaries in a group.
fn two_primaries_transform(v: &DynValue) -> (Value, Transform) {
    group(vec![
        primary_transform(v),
        primary_transform(&primary(GradingStyle::Log, 3.0)),
    ])
}

/// Two dynamic grading tones in a group.
fn two_tones_transform(v: &DynValue) -> (Value, Transform) {
    group(vec![
        tone_transform(v),
        tone_transform(&tone(GradingStyle::Video, 3.0)),
    ])
}

/// One exposure and contrast transform with its three properties dynamic, the exposure set:
/// the GPU description holds three properties.
fn exposure_contrast_gamma_transform(v: &DynValue) -> (Value, Transform) {
    ec_build(
        ExposureContrastStyle::Logarithmic,
        "EXPOSURE_CONTRAST_LOGARITHMIC",
        DynamicPropertyType::Exposure,
        double(v),
        [true; 3],
    )
}

/// A dynamic exposure in one transform and a dynamic contrast in the next.
fn exposure_then_contrast_transform(v: &DynValue) -> (Value, Transform) {
    group(vec![
        exposure_transform(v),
        contrast_transform(&DynValue::Double(1.3)),
    ])
}

/// A grading transform's spec: the class, made dynamic, with the value set.
fn grading_spec(class: &str, style: GradingStyle, value: &DynValue) -> Value {
    json!({"class": class,
        "args": {"style": {"enum": grading_style_name(style)}, "dynamic": true,
            "dir": {"enum": "TRANSFORM_DIR_FORWARD"}},
        "calls": [["setValue", value.spec(style)]]})
}

fn primary_transform(v: &DynValue) -> (Value, Transform) {
    let DynValue::Primary(p) = v else {
        panic!("not a primary: {v:?}")
    };
    let mut t = GradingPrimaryTransform::new(GradingStyle::Log);
    t.make_dynamic();
    t.set_value(p).unwrap();
    (
        grading_spec("GradingPrimaryTransform", GradingStyle::Log, v),
        Transform::from(t),
    )
}

fn rgb_curve_transform(v: &DynValue) -> (Value, Transform) {
    let DynValue::RgbCurve(c) = v else {
        panic!("not an RGB curve: {v:?}")
    };
    let mut t = GradingRgbCurveTransform::new(GradingStyle::Lin);
    t.make_dynamic();
    t.set_value(c).unwrap();
    (
        grading_spec("GradingRGBCurveTransform", GradingStyle::Lin, v),
        Transform::from(t),
    )
}

fn tone_transform(v: &DynValue) -> (Value, Transform) {
    let DynValue::Tone(tone) = v else {
        panic!("not a tone: {v:?}")
    };
    let mut t = GradingToneTransform::new(GradingStyle::Video);
    t.make_dynamic();
    t.set_value(tone).unwrap();
    (
        grading_spec("GradingToneTransform", GradingStyle::Video, v),
        Transform::from(t),
    )
}

fn hue_curve_transform(v: &DynValue) -> (Value, Transform) {
    let DynValue::HueCurve(c) = v else {
        panic!("not a hue curve: {v:?}")
    };
    let mut t = GradingHueCurveTransform::new(GradingStyle::Log);
    t.make_dynamic();
    t.set_value(c).unwrap();
    (
        grading_spec("GradingHueCurveTransform", GradingStyle::Log, v),
        Transform::from(t),
    )
}

/// A GradingPrimary of `style` with a few fields moved by `k`.
fn primary(style: GradingStyle, k: f64) -> DynValue {
    let mut v = GradingPrimary::new(style);
    v.brightness = GradingRgbm::new(0.1 * k, -0.05 * k, 0.02, 0.03 * k);
    v.contrast = GradingRgbm::new(1.0 + 0.1 * k, 0.9, 1.05, 1.0 + 0.02 * k);
    v.gamma = GradingRgbm::new(1.1, 1.0 - 0.05 * k, 0.95, 1.0);
    v.saturation = 1.0 + 0.1 * k;
    DynValue::Primary(v)
}

/// An RGB curve whose red and master curves bend by `k`.
fn rgb_curve(k: f32) -> DynValue {
    let mut v = GradingRgbCurve::new(GradingStyle::Lin);
    let bent = |y: f32| {
        GradingBSplineCurve::with_points(&[
            GradingControlPoint::new(-1.0, -1.0),
            GradingControlPoint::new(0.25, y),
            GradingControlPoint::new(1.0, 1.0),
            GradingControlPoint::new(4.0, 4.5),
        ])
    };
    *v.curve_mut(ocio::RgbCurveType::Red).unwrap() = bent(0.25 + 0.1 * k);
    *v.curve_mut(ocio::RgbCurveType::Master).unwrap() = bent(0.25 - 0.05 * k);
    DynValue::RgbCurve(v)
}

/// A GradingTone of `style` with a few fields moved by `k`.
fn tone(style: GradingStyle, k: f64) -> DynValue {
    let mut v = GradingTone::new(style);
    v.midtones.red = 1.0 + 0.2 * k;
    v.midtones.master = 1.0 - 0.1 * k;
    v.shadows.green = 1.0 + 0.15 * k;
    v.highlights.master = 1.0 - 0.1 * k;
    v.scontrast = 1.0 + 0.1 * k;
    DynValue::Tone(v)
}

/// A hue curve of the log style whose sat-sat and hue-sat curves move by `k`.
fn hue_curve(k: f32) -> DynValue {
    let mut v = GradingHueCurve::new(GradingStyle::Log);
    *v.curve_mut(HueCurveType::SatSat).unwrap() =
        GradingBSplineCurve::with_points_and_hue_curve_type(
            &[
                GradingControlPoint::new(0.0, 0.0),
                GradingControlPoint::new(0.5, 0.5 + 0.1 * k),
                GradingControlPoint::new(1.0, 1.0),
            ],
            HueCurveType::SatSat,
        );
    *v.curve_mut(HueCurveType::HueSat).unwrap() =
        GradingBSplineCurve::with_points_and_hue_curve_type(
            &[
                GradingControlPoint::new(0.0, 1.0),
                GradingControlPoint::new(0.3, 1.0 + 0.2 * k),
                GradingControlPoint::new(0.7, 1.0 - 0.1 * k),
            ],
            HueCurveType::HueSat,
        );
    DynValue::HueCurve(v)
}

fn kinds() -> Vec<Kind> {
    use DynValue::Double;
    let log = GradingStyle::Log;
    let video = GradingStyle::Video;
    vec![
        Kind {
            label: "exposure",
            ty: DynamicPropertyType::Exposure,
            style: log,
            transform: exposure_transform,
            values: [Double(0.5), Double(1.25), Double(-0.75), Double(2.0)],
            duplicates: false,
            gpu_props: 1,
        },
        Kind {
            label: "contrast",
            ty: DynamicPropertyType::Contrast,
            style: log,
            transform: contrast_transform,
            values: [Double(1.2), Double(0.8), Double(1.5), Double(0.6)],
            duplicates: false,
            gpu_props: 1,
        },
        Kind {
            label: "gamma",
            ty: DynamicPropertyType::Gamma,
            style: log,
            transform: gamma_transform,
            values: [Double(1.1), Double(0.9), Double(1.3), Double(0.7)],
            duplicates: false,
            gpu_props: 1,
        },
        Kind {
            label: "two exposures",
            ty: DynamicPropertyType::Exposure,
            style: log,
            transform: two_exposures_transform,
            values: [Double(0.5), Double(1.25), Double(-0.75), Double(2.0)],
            duplicates: true,
            gpu_props: 1,
        },
        // The other types in pairs: each has its own place in `validate_dynamic_properties`
        // and its own warning.
        Kind {
            label: "two contrasts",
            ty: DynamicPropertyType::Contrast,
            style: log,
            transform: two_contrasts_transform,
            values: [Double(1.2), Double(0.8), Double(1.5), Double(0.6)],
            duplicates: true,
            gpu_props: 1,
        },
        Kind {
            label: "two gammas",
            ty: DynamicPropertyType::Gamma,
            style: log,
            transform: two_gammas_transform,
            values: [Double(1.1), Double(0.9), Double(1.3), Double(0.7)],
            duplicates: true,
            gpu_props: 1,
        },
        Kind {
            label: "grading primary",
            ty: DynamicPropertyType::GradingPrimary,
            style: log,
            transform: primary_transform,
            values: [
                primary(log, 1.0),
                primary(log, 2.0),
                primary(log, -1.0),
                primary(log, 0.5),
            ],
            duplicates: false,
            gpu_props: 1,
        },
        Kind {
            label: "two grading primaries",
            ty: DynamicPropertyType::GradingPrimary,
            style: log,
            transform: two_primaries_transform,
            values: [
                primary(log, 1.0),
                primary(log, 2.0),
                primary(log, -1.0),
                primary(log, 0.5),
            ],
            duplicates: true,
            gpu_props: 1,
        },
        Kind {
            label: "grading RGB curve",
            ty: DynamicPropertyType::GradingRgbCurve,
            style: GradingStyle::Lin,
            transform: rgb_curve_transform,
            values: [
                rgb_curve(1.0),
                rgb_curve(2.0),
                rgb_curve(-1.0),
                rgb_curve(0.5),
            ],
            duplicates: false,
            gpu_props: 1,
        },
        Kind {
            label: "grading tone",
            ty: DynamicPropertyType::GradingTone,
            style: video,
            transform: tone_transform,
            values: [
                tone(video, 1.0),
                tone(video, 2.0),
                tone(video, -1.0),
                tone(video, 0.5),
            ],
            duplicates: false,
            gpu_props: 1,
        },
        Kind {
            label: "two grading tones",
            ty: DynamicPropertyType::GradingTone,
            style: video,
            transform: two_tones_transform,
            values: [
                tone(video, 1.0),
                tone(video, 2.0),
                tone(video, -1.0),
                tone(video, 0.5),
            ],
            duplicates: true,
            gpu_props: 1,
        },
        Kind {
            label: "grading hue curve",
            ty: DynamicPropertyType::GradingHueCurve,
            style: log,
            transform: hue_curve_transform,
            values: [
                hue_curve(1.0),
                hue_curve(2.0),
                hue_curve(-1.0),
                hue_curve(0.5),
            ],
            duplicates: false,
            gpu_props: 1,
        },
        // Several properties in one description: the order of its list.
        Kind {
            label: "exposure, contrast and gamma of one op",
            ty: DynamicPropertyType::Exposure,
            style: log,
            transform: exposure_contrast_gamma_transform,
            values: [Double(0.5), Double(1.25), Double(-0.75), Double(2.0)],
            duplicates: false,
            gpu_props: 3,
        },
        Kind {
            label: "an exposure op and a contrast op",
            ty: DynamicPropertyType::Exposure,
            style: log,
            transform: exposure_then_contrast_transform,
            values: [Double(0.5), Double(1.25), Double(-0.75), Double(2.0)],
            duplicates: false,
            gpu_props: 2,
        },
    ]
}

/// The pixels each apply step processes: RGBA, alpha passed through.
const PIXELS: [f32; 12] = [
    0.5, 0.4, 0.2, 1.0, //
    0.18, 0.18, 0.18, 0.5, //
    -0.1, 1.5, 0.9, 0.0,
];

// ---------------------------------------------------------------------------------------------
// The script

/// One call, on both sides. Names bind processors, CPU and GPU processors, shader descriptions
/// and properties.
#[derive(Debug, Clone)]
enum Step {
    /// `config.getProcessor(transform of values[i])`.
    Processor(&'static str, usize),
    /// `from.getOptimizedProcessor(flags)`.
    Optimized(&'static str, &'static str, &'static str),
    /// The processor `name` written out again.
    DumpProcessor(&'static str),
    /// `from.getDefaultCPUProcessor()`, or `getOptimizedCPUProcessor(flags)`.
    Cpu(&'static str, &'static str, Option<&'static str>),
    /// `from.getDefaultGPUProcessor()`.
    Gpu(&'static str, &'static str),
    /// `from.getOptimizedGPUProcessor(flags)`.
    OptGpu(&'static str, &'static str, &'static str),
    /// `GpuShaderDesc.CreateShaderDesc()`.
    Desc(&'static str),
    /// `gpu.extractGpuShaderInfo(desc)`.
    Extract(&'static str, &'static str),
    /// `desc.getDynamicProperties()`: the iterator, which the binding writes out by the
    /// properties' getters, and those of the other types raise.
    DescProps(&'static str, &'static str),
    /// `len(iterator)`.
    Len(&'static str),
    /// `iterator[i]`.
    Item(&'static str, &'static str, usize),
    /// `property.getType()`.
    Type(&'static str),
    /// `from.hasDynamicProperty(type)`.
    Has(&'static str),
    /// `from.getDynamicProperty(type)`.
    Get(&'static str, &'static str),
    /// The property's getter of its type.
    Read(&'static str),
    /// The property's setter of its type, with values[i].
    Set(&'static str, usize),
    /// `cpu.applyRGBA(PIXELS)`.
    Apply(&'static str),
    /// `config.setProcessorCacheFlags(flags)`.
    CacheFlags(&'static str),
    /// `config.clearProcessorCache()`.
    ClearCache,
}

fn script(kind: &Kind) -> Vec<Step> {
    use Step::*;
    let mut steps = vec![
        // The processor's property.
        Processor("p", 0),
        Has("p"),
        Get("dp", "p"),
        Read("dp"),
        // A CPU processor takes a copy.
        Cpu("c1", "p", None),
        Has("c1"),
        Get("dc1", "c1"),
        Read("dc1"),
        Apply("c1"),
        Set("dp", 1),
        Read("dp"),
        Read("dc1"),
        Apply("c1"),
        DumpProcessor("p"),
        // The cache hands out the CPU processor already made (shared, the default flags); a
        // new one takes the processor's value now.
        Cpu("c2", "p", None),
        Get("dc2", "c2"),
        Read("dc2"),
        Cpu("c3", "p", Some("OPTIMIZATION_LOSSLESS")),
        Get("dc3", "c3"),
        Read("dc3"),
        Apply("c3"),
        // An optimized processor shares the processor's property.
        Optimized("o", "p", "OPTIMIZATION_DEFAULT"),
        Get("do", "o"),
        Read("do"),
        Set("do", 2),
        Read("dp"),
        DumpProcessor("p"),
        DumpProcessor("o"),
        Cpu("c4", "o", None),
        Apply("c4"),
        // The CPU processor's own property.
        Set("dc1", 2),
        Read("dc1"),
        Apply("c1"),
        Read("dp"),
        // Without dynamic properties.
        Cpu("c5", "p", Some("OPTIMIZATION_ALL")),
        Has("c5"),
        Get("dc5", "c5"),
        Apply("c5"),
        // A GPU description takes a copy.
        Gpu("g", "p"),
        Desc("d"),
        Extract("g", "d"),
        // The description's list: its length, and the type of each property in order.
        DescProps("d", "l"),
        Len("l"),
    ];
    const ITEMS: [&str; 3] = ["item0", "item1", "item2"];
    for (i, name) in ITEMS.iter().enumerate().take(kind.gpu_props) {
        steps.push(Item(name, "l", i));
        steps.push(Type(name));
    }
    steps.extend([
        Has("d"),
        Get("dd", "d"),
        Read("dd"),
        Set("dp", 1),
        Read("dd"),
        Set("dd", 0),
        Read("dd"),
        Read("dp"),
        // The config's cache: the processor of the same transform is the cached one, with the
        // value set on it.
        Processor("p2", 0),
        Get("dp2", "p2"),
        Read("dp2"),
        // Enabled, without sharing dynamic properties: a new processor, whose CPU processors
        // are made afresh each time, with the processor's value then.
        CacheFlags("PROCESSOR_CACHE_ENABLED"),
        ClearCache,
        Processor("q", 0),
        Get("dq", "q"),
        Cpu("q1", "q", None),
        Set("dq", 1),
        Cpu("q2", "q", None),
        Get("dq2", "q2"),
        Read("dq2"),
        Apply("q1"),
        Apply("q2"),
        // The fallback: another transform of the same cache ID gives the cached processor, the
        // values set on it included.
        CacheFlags("PROCESSOR_CACHE_DEFAULT"),
        Processor("r", 3),
        Get("dr", "r"),
        Read("dr"),
        DumpProcessor("r"),
        Set("dq", 2),
        Read("dr"),
        // The cache off.
        CacheFlags("PROCESSOR_CACHE_OFF"),
        Processor("s", 0),
        Get("ds", "s"),
        Read("ds"),
        Cpu("s1", "s", None),
        Set("ds", 2),
        Cpu("s2", "s", None),
        Apply("s1"),
        Apply("s2"),
        // OPTIMIZATION_NO_DYNAMIC_PROPERTIES alone drops the dynamic properties from the
        // optimized processor, the CPU processor and the GPU description, and other flags
        // don't. A property that isn't there raises on each of them in its own words.
        CacheFlags("PROCESSOR_CACHE_DEFAULT"),
        Processor("n", 0),
        Optimized("on", "n", "OPTIMIZATION_NO_DYNAMIC_PROPERTIES"),
        Has("on"),
        Get("don", "on"),
        Optimized("on2", "n", "OPTIMIZATION_NONE"),
        Has("on2"),
        Get("don2", "on2"),
        Read("don2"),
        Cpu("cn", "n", Some("OPTIMIZATION_NO_DYNAMIC_PROPERTIES")),
        Has("cn"),
        Get("dcn", "cn"),
        Apply("cn"),
        OptGpu("gn", "n", "OPTIMIZATION_NO_DYNAMIC_PROPERTIES"),
        Desc("dn"),
        Extract("gn", "dn"),
        Has("dn"),
        Get("ddn", "dn"),
        OptGpu("gv", "n", "OPTIMIZATION_VERY_GOOD"),
        Desc("dv"),
        Extract("gv", "dv"),
        Has("dv"),
        Get("ddv", "dv"),
        Read("ddv"),
        Cpu("cv", "n", Some("OPTIMIZATION_VERY_GOOD")),
        Has("cv"),
        Get("dcv", "cv"),
        Read("dcv"),
        Apply("cv"),
    ]);
    steps
}

/// The wheel's calls of the script.
fn wheel_calls(kind: &Kind, steps: &[Step]) -> Vec<Value> {
    let ty = json!({"enum": type_name(kind.ty)});
    let pixels: Vec<Value> = PIXELS.iter().map(|&v| f64_spec(f64::from(v))).collect();
    steps
        .iter()
        .map(|step| match step {
            Step::Processor(name, i) => {
                let (spec, _) = (kind.transform)(&kind.values[*i]);
                json!({"call": "getProcessor", "args": [{"transform": spec}], "as": name})
            }
            Step::Optimized(name, from, flags) => json!({"call": "getOptimizedProcessor",
                "on": from, "args": [{"enum": flags}], "as": name}),
            Step::DumpProcessor(name) => json!({"dump": name}),
            Step::Cpu(name, from, None) => {
                json!({"call": "getDefaultCPUProcessor", "on": from, "as": name})
            }
            Step::Cpu(name, from, Some(flags)) => json!({"call": "getOptimizedCPUProcessor",
                "on": from, "args": [{"enum": flags}], "as": name}),
            Step::Gpu(name, from) => {
                json!({"call": "getDefaultGPUProcessor", "on": from, "as": name})
            }
            Step::OptGpu(name, from, flags) => json!({"call": "getOptimizedGPUProcessor",
                "on": from, "args": [{"enum": flags}], "as": name}),
            Step::Desc(name) => {
                json!({"call": "CreateShaderDesc", "on": "GpuShaderDesc", "as": name})
            }
            Step::Extract(gpu, desc) => json!({"call": "extractGpuShaderInfo", "on": gpu,
                "args": [{"ref": desc}]}),
            Step::DescProps(desc, list) => {
                json!({"call": "getDynamicProperties", "on": desc, "as": list})
            }
            Step::Len(list) => json!({"call": "__len__", "on": list}),
            Step::Item(name, list, i) => {
                json!({"call": "__getitem__", "on": list, "args": [i], "as": name})
            }
            Step::Type(prop) => json!({"call": "getType", "on": prop}),
            Step::Has(from) => json!({"call": "hasDynamicProperty", "on": from, "args": [ty]}),
            Step::Get(name, from) => {
                json!({"call": "getDynamicProperty", "on": from, "args": [ty], "as": name})
            }
            Step::Read(prop) => json!({"call": getter(kind.ty), "on": prop}),
            Step::Set(prop, i) => json!({"call": kind.values[*i].setter(), "on": prop,
                "args": [kind.values[*i].spec(kind.style)]}),
            Step::Apply(cpu) => json!({"call": "applyRGBA", "on": cpu, "args": [pixels]}),
            Step::CacheFlags(flags) => {
                json!({"call": "setProcessorCacheFlags", "args": [{"enum": flags}]})
            }
            Step::ClearCache => json!({"call": "clearProcessorCache"}),
        })
        .collect()
}

/// The port's objects, by name.
enum Obj {
    Proc(Arc<Processor>),
    Cpu(Arc<CpuProcessor>),
    Gpu(Arc<GpuProcessor>),
    Desc(Box<GpuShaderDesc>),
    Prop(DynamicPropertyRcPtr),
    /// The iterator of a description's properties: the description it reads.
    List(&'static str),
}

/// What a step gave on the port's side.
enum Outcome {
    /// A call that returns nothing.
    Unit,
    /// A processor, compared with its dump.
    Processor(Arc<Processor>),
    /// A CPU or GPU processor's getters, or a property, compared getter by getter.
    Getters(Vec<(&'static str, Dumped)>),
    /// A getDynamicProperty that returned: the wheel's dump of the property raises in its
    /// getters of the other types.
    Property,
    /// A value, compared with the wheel's written value.
    Value(Dumped),
}

fn flags(name: &str) -> OptimizationFlags {
    match name {
        "OPTIMIZATION_DEFAULT" => OptimizationFlags::DEFAULT,
        "OPTIMIZATION_LOSSLESS" => OptimizationFlags::LOSSLESS,
        "OPTIMIZATION_ALL" => OptimizationFlags::ALL,
        "OPTIMIZATION_NONE" => OptimizationFlags::NONE,
        "OPTIMIZATION_NO_DYNAMIC_PROPERTIES" => OptimizationFlags::NO_DYNAMIC_PROPERTIES,
        "OPTIMIZATION_VERY_GOOD" => OptimizationFlags::VERY_GOOD,
        other => panic!("flags {other}"),
    }
}

fn cache_flags(name: &str) -> ProcessorCacheFlags {
    match name {
        "PROCESSOR_CACHE_OFF" => ProcessorCacheFlags::OFF,
        "PROCESSOR_CACHE_ENABLED" => ProcessorCacheFlags::ENABLED,
        "PROCESSOR_CACHE_DEFAULT" => ProcessorCacheFlags::DEFAULT,
        other => panic!("cache flags {other}"),
    }
}

fn bit_depth_name(depth: BitDepth) -> &'static str {
    match depth {
        BitDepth::F32 => "BIT_DEPTH_F32",
        other => panic!("bit depth {other:?}"),
    }
}

/// The port's step.
fn port_step(
    kind: &Kind,
    step: &Step,
    config: &Config,
    objects: &mut BTreeMap<&'static str, Obj>,
) -> ocio::Result<Outcome> {
    let processor = |objects: &BTreeMap<&str, Obj>, name: &str| match &objects[name] {
        Obj::Proc(p) => Arc::clone(p),
        _ => panic!("{name} isn't a processor"),
    };
    let cpu = |objects: &BTreeMap<&str, Obj>, name: &str| match &objects[name] {
        Obj::Cpu(c) => Arc::clone(c),
        _ => panic!("{name} isn't a CPU processor"),
    };
    let prop = |objects: &BTreeMap<&str, Obj>, name: &str| match &objects[name] {
        Obj::Prop(p) => p.clone(),
        _ => panic!("{name} isn't a property"),
    };
    Ok(match step {
        Step::Processor(name, i) => {
            let (_, transform) = (kind.transform)(&kind.values[*i]);
            let p = config.processor(&transform)?;
            objects.insert(name, Obj::Proc(Arc::clone(&p)));
            Outcome::Processor(p)
        }
        Step::Optimized(name, from, f) => {
            let o = processor(objects, from).optimized_processor(flags(f))?;
            objects.insert(name, Obj::Proc(Arc::clone(&o)));
            Outcome::Processor(o)
        }
        Step::DumpProcessor(name) => Outcome::Processor(processor(objects, name)),
        Step::Cpu(name, from, f) => {
            let p = processor(objects, from);
            let c = match f {
                None => p.default_cpu_processor()?,
                Some(f) => p.optimized_cpu_processor(flags(f))?,
            };
            let getters = vec![
                (
                    "getCacheID",
                    Dumped::Str(String::from_utf8_lossy(c.get_cache_id()).into_owned()),
                ),
                (
                    "hasChannelCrosstalk",
                    Dumped::Bool(c.has_channel_crosstalk()),
                ),
                ("isDynamic", Dumped::Bool(c.is_dynamic())),
                ("isIdentity", Dumped::Bool(c.is_identity())),
                ("isNoOp", Dumped::Bool(c.is_no_op())),
                (
                    "getInputBitDepth",
                    dumped_enum(bit_depth_name(c.get_input_bit_depth())),
                ),
                (
                    "getOutputBitDepth",
                    dumped_enum(bit_depth_name(c.get_output_bit_depth())),
                ),
            ];
            objects.insert(name, Obj::Cpu(c));
            Outcome::Getters(getters)
        }
        Step::Gpu(name, from) => {
            let g = processor(objects, from).default_gpu_processor()?;
            let getters = vec![
                (
                    "getCacheID",
                    Dumped::Str(String::from_utf8_lossy(g.get_cache_id()).into_owned()),
                ),
                (
                    "hasChannelCrosstalk",
                    Dumped::Bool(g.has_channel_crosstalk()),
                ),
                ("isNoOp", Dumped::Bool(g.is_no_op())),
            ];
            objects.insert(name, Obj::Gpu(g));
            Outcome::Getters(getters)
        }
        Step::OptGpu(name, from, f) => {
            let g = processor(objects, from).optimized_gpu_processor(flags(f))?;
            let getters = vec![
                (
                    "getCacheID",
                    Dumped::Str(String::from_utf8_lossy(g.get_cache_id()).into_owned()),
                ),
                (
                    "hasChannelCrosstalk",
                    Dumped::Bool(g.has_channel_crosstalk()),
                ),
                ("isNoOp", Dumped::Bool(g.is_no_op())),
            ];
            objects.insert(name, Obj::Gpu(g));
            Outcome::Getters(getters)
        }
        Step::Desc(name) => {
            // `CreateShaderDesc()`: the defaults, GLSL 1.2 among them.
            let d = GpuShaderDesc::default();
            let getters = vec![
                (
                    "getCacheID",
                    Dumped::Str(String::from_utf8_lossy(&d.cache_id()).into_owned()),
                ),
                ("getDynamicProperties", Dumped::List(Vec::new())),
            ];
            objects.insert(name, Obj::Desc(Box::new(d)));
            Outcome::Getters(getters)
        }
        Step::Extract(gpu, desc) => {
            let Obj::Gpu(g) = &objects[gpu] else {
                panic!("{gpu} isn't a GPU processor")
            };
            let g = Arc::clone(g);
            let Some(Obj::Desc(d)) = objects.get_mut(desc) else {
                panic!("{desc} isn't a shader description")
            };
            g.extract_gpu_shader_info(d)?;
            Outcome::Unit
        }
        Step::DescProps(desc, list) => {
            assert!(
                matches!(objects[desc], Obj::Desc(_)),
                "{desc} isn't a shader description"
            );
            objects.insert(list, Obj::List(desc));
            Outcome::Property
        }
        Step::Len(list) => {
            let Obj::List(desc) = &objects[list] else {
                panic!("{list} isn't a property list")
            };
            let Obj::Desc(d) = &objects[desc] else {
                panic!("{desc} isn't a shader description")
            };
            Outcome::Value(Dumped::Int(i64::from(d.num_dynamic_properties())))
        }
        Step::Item(name, list, i) => {
            let Obj::List(desc) = &objects[list] else {
                panic!("{list} isn't a property list")
            };
            let Obj::Desc(d) = &objects[desc] else {
                panic!("{desc} isn't a shader description")
            };
            let got = d.dynamic_property(*i as u32);
            objects.remove(name);
            objects.insert(name, Obj::Prop(got?));
            Outcome::Property
        }
        Step::Type(name) => Outcome::Value(dumped_enum(type_name(prop(objects, name).get_type()))),
        Step::Has(from) => Outcome::Value(Dumped::Bool(match &objects[from] {
            Obj::Proc(p) => p.has_dynamic_property(kind.ty),
            Obj::Cpu(c) => c.has_dynamic_property(kind.ty),
            Obj::Desc(d) => d.has_dynamic_property(kind.ty),
            _ => panic!("{from} has no properties"),
        })),
        Step::Get(name, from) => {
            let got = match &objects[from] {
                Obj::Proc(p) => p.dynamic_property(kind.ty),
                Obj::Cpu(c) => c.get_dynamic_property(kind.ty),
                Obj::Desc(d) => d.dynamic_property_by_type(kind.ty),
                _ => panic!("{from} has no properties"),
            };
            objects.remove(name);
            let got = got?;
            objects.insert(name, Obj::Prop(got));
            Outcome::Property
        }
        Step::Read(name) => Outcome::Value(read(&prop(objects, name), kind.ty)?),
        Step::Set(name, i) => {
            kind.values[*i].set(&prop(objects, name))?;
            Outcome::Unit
        }
        Step::Apply(name) => {
            let c = cpu(objects, name);
            let mut pixels = PIXELS;
            let width = pixels.len() / 4;
            let mut img = PackedImageDesc::new(&mut pixels[..], width, 1, 4)?;
            c.apply(&mut img)?;
            Outcome::Value(Dumped::List(
                pixels.iter().map(|&v| Dumped::F64(f64::from(v))).collect(),
            ))
        }
        Step::CacheFlags(f) => {
            config.set_processor_cache_flags(cache_flags(f));
            Outcome::Unit
        }
        Step::ClearCache => {
            config.clear_processor_cache();
            Outcome::Unit
        }
    })
}

/// The wheel's entry against the port's outcome: `None` when they agree.
fn compare(entry: &Value, port: &ocio::Result<Outcome>, blobs: &[Vec<u8>]) -> Option<String> {
    let raised = entry.get("exception");
    let stage = entry.get("stage").and_then(Value::as_str);
    match port {
        Err(e) => match (raised, stage) {
            (Some(raised), Some("call")) => {
                let message = bytes(&raised["message"]);
                (message != e.what()).then(|| {
                    format!(
                        "wheel raised {:?}, port {:?}",
                        String::from_utf8_lossy(&message),
                        e.to_string()
                    )
                })
            }
            _ => Some(format!("port raised {e:?}, wheel {entry}")),
        },
        Ok(Outcome::Property) => {
            // The wheel writes the property out through its getters, and the getters of the
            // other types raise: the call returned all the same.
            (stage != Some("write") && entry.get("result").is_none())
                .then(|| format!("the port got the property, wheel {entry}"))
        }
        Ok(_) if raised.is_some() => Some(format!("the port returned, wheel {entry}")),
        Ok(Outcome::Unit) => {
            (entry.get("result") != Some(&Value::Null)).then(|| format!("wheel {entry}"))
        }
        Ok(Outcome::Value(value)) => {
            let wheel = Dumped::parse(&entry["result"], blobs);
            (&wheel != value).then(|| format!("wheel {wheel:?}\n    port  {value:?}"))
        }
        Ok(Outcome::Getters(getters)) => {
            let wheel = Dumped::parse(&entry["result"], blobs);
            let Dumped::Object(wheel) = wheel else {
                return Some(format!("wheel {entry}"));
            };
            getters
                .iter()
                .find(|(name, value)| wheel.getters.get(*name) != Some(value))
                .map(|(name, value)| {
                    format!(
                        "{name}: wheel {:?}, port {value:?}",
                        wheel.getters.get(*name)
                    )
                })
        }
        Ok(Outcome::Processor(p)) => {
            let result = &entry["result"];
            if result["class"] != "Processor" {
                return Some(format!("wheel {entry}"));
            }
            let wheel = ProcessorDump::parse(&result["processor"], blobs);
            let group = match p.create_group_transform() {
                Ok(group) => group,
                Err(e) => return Some(format!("port createGroupTransform: {e}")),
            };
            compare_processor(&wheel, p, &group).err()
        }
    }
}

/// Per dynamic property type, the script's calls on the wheel's and the port's objects.
#[test]
fn sharing_matches_the_wheel() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let mut failures = Vec::new();
    let mut compared = 0;
    let mut expected = 0;
    for kind in kinds() {
        let steps = script(&kind);
        expected += steps.len();
        let response = Oracle::get().call(
            "object_calls",
            json!({"config": "raw", "calls": wheel_calls(&kind, &steps)}),
            &[],
        );
        let result = &response.result;
        assert_eq!(result["config"], Value::Null, "{}: {result}", kind.label);
        let entries = result["calls"].as_array().expect("the calls");
        assert_eq!(entries.len(), steps.len(), "{}: {result}", kind.label);

        let config = Config::create_raw().unwrap();
        let mut objects = BTreeMap::new();
        for (i, (step, entry)) in steps.iter().zip(entries).enumerate() {
            let (outcome, port_log) = capture(|| port_step(&kind, step, &config, &mut objects));
            compared += 1;
            if matches!(step, Step::Len(_)) {
                // The script reads every property of the description, one by one.
                assert_eq!(entry["result"], json!(kind.gpu_props), "{}", kind.label);
            }
            if let Some(diff) = compare(entry, &outcome, &response.blobs) {
                failures.push(format!("{} step {i} {step:?}: {diff}", kind.label));
            }
            let wheel_log = log(&entry["log"]);
            if wheel_log != port_log {
                failures.push(format!(
                    "{} step {i} {step:?}: log: wheel {:?}, port {:?}",
                    kind.label,
                    wheel_log
                        .iter()
                        .map(|m| String::from_utf8_lossy(m))
                        .collect::<Vec<_>>(),
                    port_log
                        .iter()
                        .map(|m| String::from_utf8_lossy(m))
                        .collect::<Vec<_>>()
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert_eq!(compared, expected);
}

// ---------------------------------------------------------------------------------------------
// dynamic_apply: the CPU processor's pixels and the description's uniforms

/// The uniforms of the port's description, as `dynamic_apply` writes them.
fn port_uniforms(desc: &GpuShaderDesc) -> Vec<Value> {
    desc.uniforms()
        .iter()
        .map(|u| {
            let (kind, value) = match u.data() {
                UniformData::Double(get) => ("UNIFORM_DOUBLE", json!(get().to_bits())),
                UniformData::Bool(get) => ("UNIFORM_BOOL", json!(get())),
                UniformData::Float3(get) => {
                    ("UNIFORM_FLOAT3", json!(get().map(f32::to_bits).to_vec()))
                }
                UniformData::VectorFloat { size, values } => (
                    "UNIFORM_VECTOR_FLOAT",
                    json!(
                        values()[..size() as usize]
                            .iter()
                            .map(|v| v.to_bits())
                            .collect::<Vec<_>>()
                    ),
                ),
                UniformData::VectorInt { size, values } => (
                    "UNIFORM_VECTOR_INT",
                    json!(values()[..size() as usize].to_vec()),
                ),
            };
            json!({"name": String::from_utf8(u.name().to_vec()).unwrap(), "type": kind,
                "buffer_offset": u.buffer_offset(), "value": value})
        })
        .collect()
}

/// Per dynamic property type, the processor's, the CPU processor's and the GPU description's
/// properties set in turn, with the pixels and the uniforms after each set: a set reaches only
/// the CPU processor or the description it was taken from.
#[test]
fn uniforms_and_pixels_follow_their_own_property() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let mut failures = Vec::new();
    let mut compared = 0;
    let bytes_in: Vec<u8> = PIXELS.iter().flat_map(|v| v.to_ne_bytes()).collect();
    for kind in kinds() {
        let ty = type_name(kind.ty);
        let value = |i: usize| kind.values[i].spec(kind.style);
        // The sources in order, each set to values[1], then values[2] (after the apply and the
        // uniforms of the first set).
        let sources = ["processor", "cpu", "gpu"];
        let mut steps = vec![
            json!({"get": ty, "from": "processor", "as": "processor"}),
            json!({"get": ty, "from": "cpu", "as": "cpu"}),
            json!({"get": ty, "from": "gpu", "as": "gpu"}),
            json!({"apply": [0, 1]}),
            json!({"uniforms": true}),
        ];
        for source in sources {
            for i in [1, 2] {
                steps.push(json!({"set": source, "value": value(i)}));
                steps.push(json!({"read": source}));
                steps.push(json!({"apply": [0, 1]}));
                steps.push(json!({"uniforms": true}));
            }
        }
        let (spec, transform) = (kind.transform)(&kind.values[0]);
        let response = Oracle::get().call(
            "dynamic_apply",
            json!({
                "transform": spec,
                "gpu": {"shader": {"language": "GPU_LANGUAGE_GLSL_4_0"}},
                "buffers": [{"blob": 0}, {"size": bytes_in.len(), "fill": [0]}],
                "images": [
                    {"kind": "packed", "data": {"buffer": 0}, "width": 3, "height": 1,
                     "num_channels": 4},
                    {"kind": "packed", "data": {"buffer": 1}, "width": 3, "height": 1,
                     "num_channels": 4},
                ],
                "steps": steps,
            }),
            &[&bytes_in],
        );
        let result = &response.result;

        // The port: the raw config's processor, its default CPU processor and default GPU
        // processor, and a description in GLSL 4.0.
        let config = Config::create_raw().unwrap();
        let processor = config.processor(&transform).unwrap();
        let cpu = processor.default_cpu_processor().unwrap();
        let gpu = processor.default_gpu_processor().unwrap();
        let mut desc = GpuShaderDesc::new(GpuLanguage::Glsl4_0);
        let extracted = gpu.extract_gpu_shader_info(&mut desc);

        // Two ops of one dynamic property type: the description takes the first one's
        // property, and refuses the second one's.
        if kind.duplicates {
            assert_eq!(result["stage"], "extract", "{}: {result}", kind.label);
            let message = result["exception"]["message"].as_str().expect("a message");
            assert_eq!(extracted.unwrap_err().to_string(), message);
            compared += 1;
            continue;
        }
        assert!(
            result.get("exception").is_none(),
            "{}: {result}",
            kind.label
        );
        let wheel_steps = result["steps"].as_array().expect("the steps");
        assert_eq!(wheel_steps.len(), steps.len(), "{}: {result}", kind.label);
        extracted.unwrap();
        let props = [
            processor.dynamic_property(kind.ty).unwrap(),
            cpu.get_dynamic_property(kind.ty).unwrap(),
            desc.dynamic_property_by_type(kind.ty).unwrap(),
        ];
        for step in &wheel_steps[..3] {
            assert_eq!(step["has"], json!(true), "{}: {step}", kind.label);
            assert!(step.get("exception").is_none(), "{}: {step}", kind.label);
        }

        let mut k = 3;
        let check_apply_and_uniforms = |k: usize, label: &str| -> Vec<String> {
            let mut found = Vec::new();
            let mut pixels = PIXELS;
            let src = PackedImageDesc::new(&mut pixels[..], 3, 1, 4).unwrap();
            let mut out = [0f32; 12];
            let mut dst = PackedImageDesc::new(&mut out[..], 3, 1, 4).unwrap();
            cpu.apply_src_dst(&src, &mut dst).unwrap();
            let step = &wheel_steps[k];
            let index = step["result"]["buffers"][1]
                .as_u64()
                .unwrap_or_else(|| panic!("{step}")) as usize;
            let wheel: Vec<u32> = response.blobs[index]
                .as_chunks::<4>()
                .0
                .iter()
                .map(|b| u32::from_ne_bytes(*b))
                .collect();
            let port: Vec<u32> = out.iter().map(|v| v.to_bits()).collect();
            assert_eq!(wheel.len(), 12);
            if wheel != port {
                found.push(format!("{label}: pixels: wheel {wheel:x?}, port {port:x?}"));
            }
            let wheel = &wheel_steps[k + 1]["result"];
            let port = Value::from(port_uniforms(&desc));
            assert!(!port.as_array().unwrap().is_empty(), "{label}");
            if *wheel != port {
                found.push(format!("{label}: uniforms: wheel {wheel}, port {port}"));
            }
            found
        };
        failures.extend(check_apply_and_uniforms(
            k,
            &format!("{} first", kind.label),
        ));
        compared += 2;
        k += 2;
        for (s, source) in sources.iter().enumerate() {
            for i in [1, 2] {
                let label = format!("{} {source} set {i}", kind.label);
                let set = &wheel_steps[k];
                kind.values[i].set(&props[s]).unwrap();
                assert!(set.get("exception").is_none(), "{label}: {set}");
                let read_step = &wheel_steps[k + 1];
                let wheel = Dumped::parse(&read_step["result"]["value"], &response.blobs);
                let port = read(&props[s], kind.ty).unwrap();
                compared += 1;
                if wheel != port {
                    failures.push(format!("{label}: read: wheel {wheel:?}, port {port:?}"));
                }
                k += 2;
                failures.extend(check_apply_and_uniforms(k, &label));
                compared += 2;
                k += 2;
            }
        }
        assert_eq!(k, steps.len(), "{}", kind.label);
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    // The kinds with two ops of one type compare the refusal only.
    let duplicates = kinds().iter().filter(|k| k.duplicates).count();
    assert_eq!(
        compared,
        (kinds().len() - duplicates) * (2 + 3 * 2 * 3) + duplicates
    );
}
