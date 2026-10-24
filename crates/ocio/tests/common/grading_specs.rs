// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The port's ExposureContrast and grading transforms and grading values, built from the
//! oracle's specs (`oracle/ocio_oracle/spec.py`) the way the binding builds them: the specs of
//! the `grading_text` fixtures (`oracle/ocio_oracle/grading_text.py`).
//!
//! Only the forms those specs take are read; anything else panics, so an input can't be read
//! differently from the wheel without the test failing. Each step cites the binding's code it
//! follows (src/bindings/python @ v2.5.2).

use ocio::{
    BSplineType, ExposureContrastStyle, ExposureContrastTransform, GradingBSplineCurve,
    GradingControlPoint, GradingHueCurve, GradingHueCurveTransform, GradingPrimary,
    GradingPrimaryTransform, GradingRgbCurve, GradingRgbCurveTransform, GradingRgbm, GradingRgbmsw,
    GradingStyle, GradingTone, GradingToneTransform, HsyTransformStyle, HueCurveType, RgbCurveType,
    Transform, TransformDirection,
};
use serde_json::{Map, Value};

/// A grading value of the binding's classes.
#[derive(Debug, Clone)]
pub(crate) enum Object {
    Rgbm(GradingRgbm),
    Rgbmsw(GradingRgbmsw),
    ControlPoint(GradingControlPoint),
    BSpline(GradingBSplineCurve),
    /// A `GradingPrimary` and the style it was made for, which its `validate` takes.
    Primary(GradingPrimary, GradingStyle),
    Tone(GradingTone),
    RgbCurve(GradingRgbCurve),
    HueCurve(GradingHueCurve),
}

impl Object {
    /// `repr()`: upstream's `operator<<` (PyUtils.h `defRepr`).
    pub(crate) fn repr(&self) -> String {
        match self {
            Object::Rgbm(v) => v.to_string(),
            Object::Rgbmsw(v) => v.to_string(),
            Object::ControlPoint(v) => v.to_string(),
            Object::BSpline(v) => v.to_string(),
            Object::Primary(v, _) => v.to_string(),
            Object::Tone(v) => v.to_string(),
            Object::RgbCurve(v) => v.to_string(),
            Object::HueCurve(v) => v.to_string(),
        }
    }

    /// `validate()`, `None` where the binding's class has none (PyGradingData.cpp): a
    /// `GradingPrimary` validates for the style it was made for (as the fixtures call it).
    pub(crate) fn validate(&self) -> Option<ocio::Result<()>> {
        match self {
            Object::Rgbm(_) | Object::Rgbmsw(_) | Object::ControlPoint(_) => None,
            Object::BSpline(v) => Some(v.validate()),
            Object::Primary(v, style) => Some(v.validate(*style)),
            Object::Tone(v) => Some(v.validate()),
            Object::RgbCurve(v) => Some(v.validate()),
            Object::HueCurve(v) => Some(v.validate()),
        }
    }
}

/// A double of a value spec: `{"f64": bits}`, or a JSON number.
pub(crate) fn f64_of(v: &Value) -> f64 {
    if let Some(n) = v.as_f64() {
        return n;
    }
    let bits = v
        .get("f64")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("a double is {{\"f64\": bits}} or a number, not {v}"));
    f64::from_bits(bits)
}

/// A float argument: pybind11 converts the double to a C float (`static_cast<float>`).
fn f32_of(v: &Value) -> f32 {
    f64_of(v) as f32
}

fn enum_name(v: &Value) -> &str {
    v.get("enum")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("an enum is {{\"enum\": name}}, not {v}"))
}

fn usize_of(v: &Value) -> usize {
    v.as_u64()
        .unwrap_or_else(|| panic!("a size or an index, not {v}")) as usize
}

pub(crate) fn grading_style(v: &Value) -> GradingStyle {
    match enum_name(v) {
        "GRADING_LOG" => GradingStyle::Log,
        "GRADING_LIN" => GradingStyle::Lin,
        "GRADING_VIDEO" => GradingStyle::Video,
        other => panic!("not a grading style: {other}"),
    }
}

fn direction(v: &Value) -> TransformDirection {
    match enum_name(v) {
        "TRANSFORM_DIR_FORWARD" => TransformDirection::Forward,
        "TRANSFORM_DIR_INVERSE" => TransformDirection::Inverse,
        other => panic!("not a direction: {other}"),
    }
}

fn ec_style(v: &Value) -> ExposureContrastStyle {
    match enum_name(v) {
        "EXPOSURE_CONTRAST_LINEAR" => ExposureContrastStyle::Linear,
        "EXPOSURE_CONTRAST_VIDEO" => ExposureContrastStyle::Video,
        "EXPOSURE_CONTRAST_LOGARITHMIC" => ExposureContrastStyle::Logarithmic,
        other => panic!("not an exposure contrast style: {other}"),
    }
}

/// A hue curve type by its enum name.
pub(crate) fn hue_curve_type(name: &str) -> HueCurveType {
    match name {
        "HUE_HUE" => HueCurveType::HueHue,
        "HUE_SAT" => HueCurveType::HueSat,
        "HUE_LUM" => HueCurveType::HueLum,
        "LUM_SAT" => HueCurveType::LumSat,
        "SAT_SAT" => HueCurveType::SatSat,
        "LUM_LUM" => HueCurveType::LumLum,
        "SAT_LUM" => HueCurveType::SatLum,
        "HUE_FX" => HueCurveType::HueFx,
        other => panic!("not a hue curve type: {other}"),
    }
}

fn spline_type(name: &str) -> BSplineType {
    match name {
        "B_SPLINE" => BSplineType::BSpline,
        "DIAGONAL_B_SPLINE" => BSplineType::DiagonalBSpline,
        "HUE_HUE_B_SPLINE" => BSplineType::HueHueBSpline,
        "PERIODIC_1_B_SPLINE" => BSplineType::Periodic1BSpline,
        "PERIODIC_0_B_SPLINE" => BSplineType::Periodic0BSpline,
        "HORIZONTAL1_B_SPLINE" => BSplineType::Horizontal1BSpline,
        other => panic!("not a spline type: {other}"),
    }
}

fn rgb_curve_type(name: &str) -> RgbCurveType {
    match name {
        "RGB_RED" | "red" => RgbCurveType::Red,
        "RGB_GREEN" | "green" => RgbCurveType::Green,
        "RGB_BLUE" | "blue" => RgbCurveType::Blue,
        "RGB_MASTER" | "master" => RgbCurveType::Master,
        other => panic!("not an RGB curve: {other}"),
    }
}

/// A spec's keys, which must be among `keys`.
fn check_keys(spec: &Map<String, Value>, keys: &[&str]) {
    for key in spec.keys() {
        assert!(
            keys.contains(&key.as_str()),
            "unexpected key {key} in {spec:?}"
        );
    }
}

fn list(v: &Value) -> &[Value] {
    v.as_array()
        .unwrap_or_else(|| panic!("a list, not {v}"))
        .as_slice()
}

/// The calls of a spec, each `[method, args...]`.
fn calls(spec: &Map<String, Value>) -> Vec<(&str, &[Value])> {
    spec.get("calls").map_or_else(Vec::new, |calls| {
        list(calls)
            .iter()
            .map(|call| {
                let call = list(call);
                let name = call[0]
                    .as_str()
                    .unwrap_or_else(|| panic!("a method name: {call:?}"));
                (name, &call[1..])
            })
            .collect()
    })
}

/// The object of an `{"object": spec}` value.
fn object_value(v: &Value) -> ocio::Result<Object> {
    object(
        v.get("object")
            .unwrap_or_else(|| panic!("an object value: {v}")),
    )
}

/// `GradingBSplineCurve(values)` and `GradingBSplineCurve(values, curveType)`: a curve of
/// `values.len() / 2` points, each point's x and y from the values (PyGradingData.cpp:273-318).
fn curve_from_values(values: &[Value], curve_type: Option<HueCurveType>) -> ocio::Result<Object> {
    let size = values.len();
    if size < 4 {
        return Err(ocio::Exception::new(
            "GradingBSpline needs at least 4 values.",
        ));
    }
    if !size.is_multiple_of(2) {
        return Err(ocio::Exception::new(
            "GradingBSpline needs an even number of values.",
        ));
    }
    let num_points = size / 2;
    let mut curve = match curve_type {
        None => GradingBSplineCurve::new(num_points),
        Some(t) => GradingBSplineCurve::with_hue_curve_type(num_points, t),
    };
    for p in 0..num_points {
        let point = curve.control_point_mut(p)?;
        point.x = f32_of(&values[2 * p]);
        point.y = f32_of(&values[2 * p + 1]);
    }
    Ok(Object::BSpline(curve))
}

/// `CopyGradingBSpline(to, from)` (PyGradingData.cpp:22-33): the curve properties' setters.
fn copy_curve(to: &mut GradingBSplineCurve, from: &GradingBSplineCurve) -> ocio::Result<()> {
    to.set_spline_type(from.spline_type());
    let num_points = from.num_control_points();
    to.set_num_control_points(num_points);
    for pt in 0..num_points {
        *to.control_point_mut(pt)? = *from.control_point(pt)?;
        to.set_slope(pt, from.slope(pt)?)?;
    }
    Ok(())
}

fn bspline_value(v: &Value) -> ocio::Result<GradingBSplineCurve> {
    match object_value(v)? {
        Object::BSpline(c) => Ok(c),
        other => panic!("a GradingBSplineCurve, not {other:?}"),
    }
}

/// The value object of an object spec.
pub(crate) fn object(spec: &Value) -> ocio::Result<Object> {
    let spec = spec
        .as_object()
        .unwrap_or_else(|| panic!("an object spec: {spec}"));
    check_keys(spec, &["class", "args", "attrs", "calls"]);
    let class = spec["class"].as_str().expect("a class");
    let args = spec.get("args");
    let positional = || args.map_or(&[][..], list);
    let attrs: Vec<(&str, &Value)> = spec.get("attrs").map_or_else(Vec::new, |attrs| {
        list(attrs)
            .iter()
            .map(|a| (list(a)[0].as_str().expect("an attribute name"), &list(a)[1]))
            .collect()
    });
    let mut obj = match class {
        // py::init<>() and py::init<double, double, double, double>() (PyGradingData.cpp:79-84).
        "GradingRGBM" => match positional() {
            [] => Object::Rgbm(GradingRgbm::default()),
            [r, g, b, m] => {
                Object::Rgbm(GradingRgbm::new(f64_of(r), f64_of(g), f64_of(b), f64_of(m)))
            }
            other => panic!("GradingRGBM{other:?}"),
        },
        // py::init<>(), (6 doubles) and (start, width) (162-169).
        "GradingRGBMSW" => match positional() {
            [] => Object::Rgbmsw(GradingRgbmsw::default()),
            [s, w] => Object::Rgbmsw(GradingRgbmsw::with_start_width(f64_of(s), f64_of(w))),
            [r, g, b, m, s, w] => Object::Rgbmsw(GradingRgbmsw::new(
                f64_of(r),
                f64_of(g),
                f64_of(b),
                f64_of(m),
                f64_of(s),
                f64_of(w),
            )),
            other => panic!("GradingRGBMSW{other:?}"),
        },
        // py::init<>() and py::init<float, float>() (227-233).
        "GradingControlPoint" => match positional() {
            [] => Object::ControlPoint(GradingControlPoint::default()),
            [x, y] => Object::ControlPoint(GradingControlPoint::new(f32_of(x), f32_of(y))),
            other => panic!("GradingControlPoint{other:?}"),
        },
        "GradingBSplineCurve" => match args {
            // (size, splinetype), by keyword (265-272).
            Some(Value::Object(kw)) => {
                check_keys(kw, &["size", "splinetype"]);
                Object::BSpline(GradingBSplineCurve::with_spline_type(
                    usize_of(&kw["size"]),
                    spline_type(enum_name(&kw["splinetype"])),
                ))
            }
            _ => match positional() {
                // (size) and (size, huecurvetype) (251-264).
                [size] if size.is_u64() => {
                    Object::BSpline(GradingBSplineCurve::new(usize_of(size)))
                }
                [size, t] if size.is_u64() => {
                    Object::BSpline(GradingBSplineCurve::with_hue_curve_type(
                        usize_of(size),
                        hue_curve_type(enum_name(t)),
                    ))
                }
                [values] => curve_from_values(list(values), None)?,
                [values, t] => curve_from_values(list(values), Some(hue_curve_type(enum_name(t))))?,
                other => panic!("GradingBSplineCurve{other:?}"),
            },
        },
        // GradingRGBCurve(red=, green=, blue=, master=), every curve given (402-414).
        "GradingRGBCurve" if matches!(args, Some(Value::Object(_))) => {
            let kw = args.and_then(Value::as_object).unwrap();
            check_keys(kw, &["red", "green", "blue", "master"]);
            let [r, g, b, m] = ["red", "green", "blue", "master"].map(|k| {
                bspline_value(
                    kw.get(k)
                        .unwrap_or_else(|| panic!("GradingRGBCurve needs {k}")),
                )
            });
            Object::RgbCurve(GradingRgbCurve::with_curves(&r?, &g?, &b?, &m?))
        }
        // GradingHueCurve(hue_hue, ..., hue_fx), positional (479-492).
        "GradingHueCurve" if positional().len() == 8 => {
            let c: Vec<GradingBSplineCurve> = positional()
                .iter()
                .map(bspline_value)
                .collect::<ocio::Result<_>>()?;
            Object::HueCurve(GradingHueCurve::with_curves(
                &c[0], &c[1], &c[2], &c[3], &c[4], &c[5], &c[6], &c[7],
            )?)
        }
        // py::init<GradingStyle>() (106, 196); the curves' (style) constructors (396-401,
        // 473-478).
        "GradingPrimary" | "GradingTone" | "GradingRGBCurve" | "GradingHueCurve" => {
            let [style] = positional() else {
                panic!("{class}{:?}", positional())
            };
            let style = grading_style(style);
            match class {
                "GradingPrimary" => Object::Primary(GradingPrimary::new(style), style),
                "GradingTone" => Object::Tone(GradingTone::new(style)),
                "GradingRGBCurve" => Object::RgbCurve(GradingRgbCurve::new(style)),
                _ => Object::HueCurve(GradingHueCurve::new(style)),
            }
        }
        other => panic!("not a grading value class: {other}"),
    };
    for (name, value) in attrs {
        set_attr(&mut obj, name, value)?;
    }
    for (name, args) in calls(spec) {
        match (&mut obj, name, args) {
            // setSlopes (PyGradingData.cpp:353-365).
            (Object::BSpline(curve), "setSlopes", [slopes]) => {
                let slopes = list(slopes);
                let num_points = curve.num_control_points();
                if slopes.len() != num_points {
                    return Err(ocio::Exception::new(
                        "Length of slopes vector must match number of control points.",
                    ));
                }
                for (pt, s) in slopes.iter().enumerate() {
                    curve.set_slope(pt, f32_of(s))?;
                }
            }
            (obj, name, args) => panic!("{obj:?}.{name}{args:?}"),
        }
    }
    Ok(obj)
}

fn rgbm_value(v: &Value) -> ocio::Result<GradingRgbm> {
    match object_value(v)? {
        Object::Rgbm(x) => Ok(x),
        other => panic!("a GradingRGBM, not {other:?}"),
    }
}

fn rgbmsw_value(v: &Value) -> ocio::Result<GradingRgbmsw> {
    match object_value(v)? {
        Object::Rgbmsw(x) => Ok(x),
        other => panic!("a GradingRGBMSW, not {other:?}"),
    }
}

/// An attribute set: the fields (`def_readwrite`) of `GradingPrimary` and `GradingTone`, and
/// the curves' properties, which copy the curve in (PyGradingData.cpp:121-145, 211-222,
/// 429-468, 511-590).
fn set_attr(obj: &mut Object, name: &str, value: &Value) -> ocio::Result<()> {
    match obj {
        Object::Primary(p, _) => match name {
            "brightness" => p.brightness = rgbm_value(value)?,
            "contrast" => p.contrast = rgbm_value(value)?,
            "gamma" => p.gamma = rgbm_value(value)?,
            "offset" => p.offset = rgbm_value(value)?,
            "exposure" => p.exposure = rgbm_value(value)?,
            "lift" => p.lift = rgbm_value(value)?,
            "gain" => p.gain = rgbm_value(value)?,
            "saturation" => p.saturation = f64_of(value),
            "pivot" => p.pivot = f64_of(value),
            "pivotBlack" => p.pivot_black = f64_of(value),
            "pivotWhite" => p.pivot_white = f64_of(value),
            "clampBlack" => p.clamp_black = f64_of(value),
            "clampWhite" => p.clamp_white = f64_of(value),
            other => panic!("GradingPrimary.{other}"),
        },
        Object::Tone(t) => match name {
            "blacks" => t.blacks = rgbmsw_value(value)?,
            "shadows" => t.shadows = rgbmsw_value(value)?,
            "midtones" => t.midtones = rgbmsw_value(value)?,
            "highlights" => t.highlights = rgbmsw_value(value)?,
            "whites" => t.whites = rgbmsw_value(value)?,
            "scontrast" => t.scontrast = f64_of(value),
            other => panic!("GradingTone.{other}"),
        },
        Object::RgbCurve(c) => {
            let from = bspline_value(value)?;
            copy_curve(c.curve_mut(rgb_curve_type(name))?, &from)?;
        }
        Object::HueCurve(c) => {
            let from = bspline_value(value)?;
            copy_curve(
                c.curve_mut(hue_curve_type(&name.to_ascii_uppercase()))?,
                &from,
            )?;
        }
        other => panic!("{other:?}.{name} = {value}"),
    }
    Ok(())
}

/// The transform of a transform spec, or the binding's refusal of a call.
pub(crate) fn transform(spec: &Value) -> ocio::Result<Transform> {
    let spec = spec
        .as_object()
        .unwrap_or_else(|| panic!("a transform spec: {spec}"));
    check_keys(spec, &["class", "args", "calls"]);
    let class = spec["class"].as_str().expect("a class");
    if class == "ExposureContrastTransform" {
        return exposure_contrast(spec);
    }
    // `cls(style, dynamic, dir)`: Create(style), setStyle(style), makeDynamic() when
    // dynamic, setDirection(dir), validate() (PyGrading*Transform.cpp:35-46); and
    // `cls(values, style, dynamic, dir)`: the same with setValue(values) after setStyle
    // (17-34). Arguments left out take the defaults: style log, not dynamic, forward.
    let args = spec["args"].as_object().expect("keyword arguments");
    check_keys(args, &["values", "style", "dynamic", "dir"]);
    let style = args.get("style").map_or(GradingStyle::Log, grading_style);
    let dynamic = args
        .get("dynamic")
        .is_some_and(|d| d.as_bool().expect("dynamic"));
    let dir = args
        .get("dir")
        .map_or(TransformDirection::Forward, direction);
    let values = args.get("values").map(object_value).transpose()?;
    let calls = calls(spec);
    macro_rules! construct {
        ($t:ident, $variant:ident) => {{
            let mut t = $t::new(style);
            t.set_style(style);
            match &values {
                None => {}
                Some(Object::$variant(v, ..)) => t.set_value(v)?,
                Some(other) => panic!("{class}: values {other:?}"),
            }
            if dynamic {
                t.make_dynamic();
            }
            t.set_direction(dir);
            t.validate()?;
            t
        }};
    }
    let unknown = |call: (&str, &[Value])| -> ! { panic!("{class}.{}{:?}", call.0, call.1) };
    Ok(match class {
        "GradingPrimaryTransform" => {
            let mut t = construct!(GradingPrimaryTransform, Primary);
            for call in calls {
                match call {
                    ("setValue", [v]) => match object_value(v)? {
                        Object::Primary(p, _) => t.set_value(&p)?,
                        other => panic!("setValue({other:?})"),
                    },
                    ("setDirection", [d]) => t.set_direction(direction(d)),
                    ("makeDynamic", []) => t.make_dynamic(),
                    call => unknown(call),
                }
            }
            Transform::from(t)
        }
        "GradingToneTransform" => {
            let mut t = construct!(GradingToneTransform, Tone);
            for call in calls {
                match call {
                    ("setValue", [v]) => match object_value(v)? {
                        Object::Tone(p) => t.set_value(&p)?,
                        other => panic!("setValue({other:?})"),
                    },
                    ("setDirection", [d]) => t.set_direction(direction(d)),
                    ("makeDynamic", []) => t.make_dynamic(),
                    call => unknown(call),
                }
            }
            Transform::from(t)
        }
        "GradingRGBCurveTransform" => {
            let mut t = construct!(GradingRgbCurveTransform, RgbCurve);
            for call in calls {
                match call {
                    ("setValue", [v]) => match object_value(v)? {
                        Object::RgbCurve(p) => t.set_value(&p)?,
                        other => panic!("setValue({other:?})"),
                    },
                    ("setBypassLinToLog", [b]) => {
                        t.set_bypass_lin_to_log(b.as_bool().expect("a bool"));
                    }
                    ("setSlope", [c, i, s]) => {
                        t.set_slope(rgb_curve_type(enum_name(c)), usize_of(i), f32_of(s))?;
                    }
                    ("setDirection", [d]) => t.set_direction(direction(d)),
                    ("makeDynamic", []) => t.make_dynamic(),
                    call => unknown(call),
                }
            }
            Transform::from(t)
        }
        "GradingHueCurveTransform" => {
            let mut t = construct!(GradingHueCurveTransform, HueCurve);
            for call in calls {
                match call {
                    ("setValue", [v]) => match object_value(v)? {
                        Object::HueCurve(p) => t.set_value(&p)?,
                        other => panic!("setValue({other:?})"),
                    },
                    ("setRGBToHSY", [s]) => t.set_rgb_to_hsy(match enum_name(s) {
                        "HSY_TRANSFORM_NONE" => HsyTransformStyle::None,
                        "HSY_TRANSFORM_1" => HsyTransformStyle::Transform1,
                        other => panic!("not an HSY style: {other}"),
                    }),
                    ("setSlope", [c, i, s]) => {
                        t.set_slope(hue_curve_type(enum_name(c)), usize_of(i), f32_of(s))?;
                    }
                    ("setDirection", [d]) => t.set_direction(direction(d)),
                    ("makeDynamic", []) => t.make_dynamic(),
                    call => unknown(call),
                }
            }
            Transform::from(t)
        }
        other => panic!("not an ExposureContrast or grading transform: {other}"),
    })
}

/// `ExposureContrastTransform()` (`Create`, PyExposureContrastTransform.cpp:17), then the
/// spec's setters.
fn exposure_contrast(spec: &Map<String, Value>) -> ocio::Result<Transform> {
    assert!(
        spec.get("args").is_none(),
        "the default constructor: {spec:?}"
    );
    let mut t = ExposureContrastTransform::new();
    for call in calls(spec) {
        match call {
            ("setStyle", [s]) => t.set_style(ec_style(s)),
            ("setExposure", [v]) => t.set_exposure(f64_of(v)),
            ("setContrast", [v]) => t.set_contrast(f64_of(v)),
            ("setGamma", [v]) => t.set_gamma(f64_of(v)),
            ("setPivot", [v]) => t.set_pivot(f64_of(v)),
            ("setLogExposureStep", [v]) => t.set_log_exposure_step(f64_of(v)),
            ("setLogMidGray", [v]) => t.set_log_mid_gray(f64_of(v)),
            ("makeExposureDynamic", []) => t.make_exposure_dynamic(),
            ("makeContrastDynamic", []) => t.make_contrast_dynamic(),
            ("makeGammaDynamic", []) => t.make_gamma_dynamic(),
            ("setDirection", [d]) => t.set_direction(direction(d)),
            (name, args) => panic!("ExposureContrastTransform.{name}{args:?}"),
        }
    }
    Ok(Transform::from(t))
}

// The other way: specs of the port's values, in the forms read above.

/// A double as a value spec, by its bits.
pub(crate) fn f64_spec(v: f64) -> Value {
    serde_json::json!({"f64": v.to_bits()})
}

/// A grading style as a value spec.
pub(crate) fn style_spec(style: GradingStyle) -> Value {
    serde_json::json!({"enum": match style {
        GradingStyle::Log => "GRADING_LOG",
        GradingStyle::Lin => "GRADING_LIN",
        GradingStyle::Video => "GRADING_VIDEO",
    }})
}

/// The hue curve types in `GradingHueCurve`'s order, with their enum names.
pub(crate) const HUE_CURVES: [(HueCurveType, &str); 8] = [
    (HueCurveType::HueHue, "HUE_HUE"),
    (HueCurveType::HueSat, "HUE_SAT"),
    (HueCurveType::HueLum, "HUE_LUM"),
    (HueCurveType::LumSat, "LUM_SAT"),
    (HueCurveType::SatSat, "SAT_SAT"),
    (HueCurveType::LumLum, "LUM_LUM"),
    (HueCurveType::SatLum, "SAT_LUM"),
    (HueCurveType::HueFx, "HUE_FX"),
];

fn rgbm_spec(v: &GradingRgbm) -> Value {
    serde_json::json!({"object": {"class": "GradingRGBM",
        "args": [f64_spec(v.red), f64_spec(v.green), f64_spec(v.blue), f64_spec(v.master)]}})
}

fn rgbmsw_spec(v: &GradingRgbmsw) -> Value {
    serde_json::json!({"object": {"class": "GradingRGBMSW", "args": [
        f64_spec(v.red), f64_spec(v.green), f64_spec(v.blue), f64_spec(v.master),
        f64_spec(v.start), f64_spec(v.width)]}})
}

/// `GradingPrimary(style)` with every field set.
pub(crate) fn primary_spec(style: GradingStyle, v: &GradingPrimary) -> Value {
    serde_json::json!({"object": {"class": "GradingPrimary", "args": [style_spec(style)],
    "attrs": [
        ["brightness", rgbm_spec(&v.brightness)], ["contrast", rgbm_spec(&v.contrast)],
        ["gamma", rgbm_spec(&v.gamma)], ["offset", rgbm_spec(&v.offset)],
        ["exposure", rgbm_spec(&v.exposure)], ["lift", rgbm_spec(&v.lift)],
        ["gain", rgbm_spec(&v.gain)], ["saturation", f64_spec(v.saturation)],
        ["pivot", f64_spec(v.pivot)], ["pivotBlack", f64_spec(v.pivot_black)],
        ["pivotWhite", f64_spec(v.pivot_white)], ["clampBlack", f64_spec(v.clamp_black)],
        ["clampWhite", f64_spec(v.clamp_white)],
    ]}})
}

/// `GradingTone(style)` with every field set.
pub(crate) fn tone_spec(style: GradingStyle, v: &GradingTone) -> Value {
    serde_json::json!({"object": {"class": "GradingTone", "args": [style_spec(style)],
    "attrs": [
        ["blacks", rgbmsw_spec(&v.blacks)], ["shadows", rgbmsw_spec(&v.shadows)],
        ["midtones", rgbmsw_spec(&v.midtones)], ["highlights", rgbmsw_spec(&v.highlights)],
        ["whites", rgbmsw_spec(&v.whites)], ["scontrast", f64_spec(v.scontrast)],
    ]}})
}

/// `GradingBSplineCurve([x0, y0, ...])`, with the hue curve type `hue` when there is one, then
/// `setSlopes` unless the slopes are the defaults.
pub(crate) fn curve_spec(c: &GradingBSplineCurve, hue: Option<&str>) -> Value {
    let values: Vec<Value> = c
        .control_points()
        .iter()
        .flat_map(|p| [f64_spec(f64::from(p.x)), f64_spec(f64::from(p.y))])
        .collect();
    let mut args = vec![Value::Array(values)];
    if let Some(name) = hue {
        args.push(serde_json::json!({"enum": name}));
    }
    let mut spec = serde_json::json!({"class": "GradingBSplineCurve", "args": args});
    if !c.slopes_are_default() {
        let slopes: Vec<Value> = c.slopes().iter().map(|&s| f64_spec(f64::from(s))).collect();
        spec["calls"] = serde_json::json!([["setSlopes", slopes]]);
    }
    serde_json::json!({"object": spec})
}

/// `GradingRGBCurve(red=, green=, blue=, master=)`.
pub(crate) fn rgb_curve_spec(v: &GradingRgbCurve) -> Value {
    let c = |t: RgbCurveType| curve_spec(v.curve(t).unwrap(), None);
    serde_json::json!({"object": {"class": "GradingRGBCurve", "args": {
        "red": c(RgbCurveType::Red), "green": c(RgbCurveType::Green),
        "blue": c(RgbCurveType::Blue), "master": c(RgbCurveType::Master)}}})
}

/// `GradingHueCurve(hue_hue, ..., hue_fx)`, each curve of its type's spline.
pub(crate) fn hue_curve_spec(v: &GradingHueCurve) -> Value {
    assert!(
        !v.get_draw_curve_only(),
        "the binding's constructor draws no curve only"
    );
    let args: Vec<Value> = HUE_CURVES
        .iter()
        .map(|&(t, name)| {
            let c = v.curve(t).unwrap();
            assert_eq!(
                c.spline_type(),
                t.b_spline_type(),
                "{name}: its own spline type"
            );
            curve_spec(c, Some(name))
        })
        .collect();
    serde_json::json!({"object": {"class": "GradingHueCurve", "args": args}})
}
