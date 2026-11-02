// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Input values/serialization already exercised by dynamic_properties_oracle.
//! Private reuse for legacy post-extraction checks; no SDK output expectation.

#![allow(dead_code)]
use super::common::transforms::{
    dump_grading_hue_curve, dump_grading_primary, dump_grading_rgb_curve, dump_grading_tone,
    grading_style_name,
};
use ocio::{
    DynamicPropertyType, GradingBSplineCurve, GradingControlPoint, GradingHueCurve, GradingPrimary,
    GradingRgbCurve, GradingRgbm, GradingRgbmsw, GradingStyle, GradingTone, HueCurveType,
};
use ocio_ops::dynamic_property::DynamicPropertyRcPtr;
use ocio_testkit::processor_ops::Dumped;
use ocio_testkit::transform_text::f64_spec;
use serde_json::{Value, json};

#[derive(Debug, Clone)]
pub(super) enum DynValue {
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
    pub(super) fn spec(&self, style: GradingStyle) -> Value {
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
    pub(super) fn set(&self, prop: &DynamicPropertyRcPtr) -> ocio::Result<()> {
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

pub(super) fn type_name(ty: DynamicPropertyType) -> &'static str {
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
pub(super) fn read(prop: &DynamicPropertyRcPtr, ty: DynamicPropertyType) -> ocio::Result<Dumped> {
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

/// A GradingPrimary of `style` with a few fields moved by `k`.
pub(super) fn primary(style: GradingStyle, k: f64) -> DynValue {
    let mut v = GradingPrimary::new(style);
    v.brightness = GradingRgbm::new(0.1 * k, -0.05 * k, 0.02, 0.03 * k);
    v.contrast = GradingRgbm::new(1.0 + 0.1 * k, 0.9, 1.05, 1.0 + 0.02 * k);
    v.gamma = GradingRgbm::new(1.1, 1.0 - 0.05 * k, 0.95, 1.0);
    v.saturation = 1.0 + 0.1 * k;
    DynValue::Primary(v)
}

/// An RGB curve whose red and master curves bend by `k`.
pub(super) fn rgb_curve(k: f32) -> DynValue {
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
pub(super) fn tone(style: GradingStyle, k: f64) -> DynValue {
    let mut v = GradingTone::new(style);
    v.midtones.red = 1.0 + 0.2 * k;
    v.midtones.master = 1.0 - 0.1 * k;
    v.shadows.green = 1.0 + 0.15 * k;
    v.highlights.master = 1.0 - 0.1 * k;
    v.scontrast = 1.0 + 0.1 * k;
    DynValue::Tone(v)
}

/// A hue curve of the log style whose sat-sat and hue-sat curves move by `k`.
pub(super) fn hue_curve(k: f32) -> DynValue {
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
