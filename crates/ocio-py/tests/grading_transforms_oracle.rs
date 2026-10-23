// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The transform classes of chunk 6.7b against the wheel (`ExposureContrastTransform`,
//! `GradingPrimaryTransform`, `GradingToneTransform`, `GradingRGBCurveTransform`,
//! `GradingHueCurveTransform`): each class's surface (py_surface_walk.py's `_class` against the
//! wheel's py_surface), and their behaviour through the same step scripts on both sides: the
//! base class's methods, each constructor (positional, keyword, refused), every getter and
//! setter with values whose bits differ, styles outside the enumerators (their message written
//! over its start, I-262), the dynamic states, `equals`, `validate` and `repr`.
#![cfg(feature = "python")]

mod common;

use common::script::{
    Expect, Script, b, base_methods, compare, d, f, i, l, n, none, r, ra, s, special,
};

/// Each class has the wheel's surface, member for member.
fn surface(classes: &[&str]) {
    let report = common::wheel_surface();
    for c in classes {
        let theirs = common::wheel_member(&report, c);
        let ours = common::port_member(c);
        assert!(
            theirs["members"].as_array().is_some_and(|m| m.len() > 5),
            "{c}: {theirs}"
        );
        assert_eq!(ours, theirs, "{c}");
    }
}

#[test]
fn exposure_contrast_has_the_wheels_surface() {
    surface(&["ExposureContrastTransform"]);
}

const EC_ALL: [&str; 14] = [
    "getStyle",
    "getExposure",
    "isExposureDynamic",
    "getContrast",
    "isContrastDynamic",
    "getGamma",
    "isGammaDynamic",
    "getPivot",
    "getLogExposureStep",
    "getLogMidGray",
    "getDirection",
    "getTransformType",
    "validate",
    "__repr__",
];

/// The format metadata of `obj`: the same object each time, named through it, copied with the
/// transform.
fn metadata(sc: &mut Script, obj: usize) {
    let md = sc.call(ra(obj, "getFormatMetadata"), vec![]);
    let md2 = sc.call(ra(obj, "getFormatMetadata"), vec![]);
    sc.call(n("operator.is_"), vec![r(md), r(md2)]);
    sc.call(ra(md, "setName"), vec![s("grading")]);
    sc.call(ra(md2, "getName"), vec![]);
    let copy = sc.call(n("copy.deepcopy"), vec![r(obj)]);
    let md3 = sc.call(ra(copy, "getFormatMetadata"), vec![]);
    sc.call(ra(md3, "getName"), vec![]);
}

fn exposure_contrast_script() -> Script {
    let mut sc = Script::new();
    let e = sc.call(n("OCIO.ExposureContrastTransform"), vec![]);
    sc.eval(r(e), &EC_ALL);
    base_methods(&mut sc, e, true);
    metadata(&mut sc, e);
    // Each value with special bits, then an ordinary one; an int; refused types.
    for (k, (get, set)) in [
        ("getExposure", "setExposure"),
        ("getContrast", "setContrast"),
        ("getGamma", "setGamma"),
        ("getPivot", "setPivot"),
        ("getLogExposureStep", "setLogExposureStep"),
        ("getLogMidGray", "setLogMidGray"),
    ]
    .into_iter()
    .enumerate()
    {
        for v in [special(k), special(k + 3), special(k + 6), i(2)] {
            sc.call(ra(e, set), vec![v]);
            sc.call(ra(e, get), vec![]);
        }
        sc.repr(r(e));
        sc.raises(ra(e, set), vec![s("1")]);
        sc.raises(ra(e, set), vec![none()]);
        sc.raises(ra(e, set), vec![]);
        sc.call(ra(e, set), vec![f(0.5)]);
    }
    sc.call_kw(
        ra(e, "setLogMidGray"),
        vec![],
        vec![("logMidGray", f(0.25))],
        Expect::Value,
    );
    sc.call_kw(
        ra(e, "setExposure"),
        vec![],
        vec![("value", f(0.25))],
        Expect::Raised,
    );
    // The styles, in each direction.
    for dir in ["OCIO.TRANSFORM_DIR_INVERSE", "OCIO.TRANSFORM_DIR_FORWARD"] {
        sc.call(ra(e, "setDirection"), vec![n(dir)]);
        for st in [
            "EXPOSURE_CONTRAST_LINEAR",
            "EXPOSURE_CONTRAST_VIDEO",
            "EXPOSURE_CONTRAST_LOGARITHMIC",
        ] {
            sc.call(ra(e, "setStyle"), vec![n(&format!("OCIO.{st}"))]);
            sc.eval(r(e), &["getStyle", "getDirection", "__repr__"]);
        }
    }
    for v in [3, 7, 12345, 2147483647] {
        let st = sc.call(n("OCIO.ExposureContrastStyle"), vec![i(v)]);
        sc.raises(ra(e, "setStyle"), vec![r(st)]);
        sc.eval(r(e), &["getStyle", "__repr__"]);
        sc.call_kw(
            n("OCIO.ExposureContrastTransform"),
            vec![],
            vec![("style", r(st))],
            Expect::Raised,
        );
    }
    sc.raises(ra(e, "setStyle"), vec![i(0)]);
    sc.raises(ra(e, "setStyle"), vec![none()]);
    // The dynamic states.
    for v in ["Exposure", "Contrast", "Gamma"] {
        sc.call(ra(e, &format!("make{v}Dynamic")), vec![]);
        sc.eval(r(e), &[&format!("is{v}Dynamic"), "__repr__"]);
    }
    let other = sc.call(n("OCIO.ExposureContrastTransform"), vec![]);
    sc.call(ra(e, "equals"), vec![r(e)]);
    sc.call(ra(e, "equals"), vec![r(other)]);
    for v in ["Exposure", "Contrast", "Gamma"] {
        sc.call(ra(e, &format!("make{v}NonDynamic")), vec![]);
        sc.eval(r(e), &[&format!("is{v}Dynamic"), "__repr__"]);
    }
    sc.call(ra(e, "setGamma"), vec![special(2)]);
    sc.call(ra(e, "equals"), vec![r(e)]);
    sc.call(ra(e, "validate"), vec![]);
    sc.call(ra(e, "setGamma"), vec![f(-1.0)]);
    sc.call(ra(e, "validate"), vec![]);
    sc.raises(ra(e, "equals"), vec![none()]);
    let m = sc.call(n("OCIO.MatrixTransform"), vec![]);
    sc.raises(ra(e, "equals"), vec![r(m)]);
    // The constructors.
    let full = sc.call(
        n("OCIO.ExposureContrastTransform"),
        vec![
            n("OCIO.EXPOSURE_CONTRAST_VIDEO"),
            f(1.5),
            f(1.2),
            f(0.8),
            f(0.2),
            f(0.1),
            f(0.5),
            b(true),
            b(false),
            b(true),
            n("OCIO.TRANSFORM_DIR_INVERSE"),
        ],
    );
    sc.eval(r(full), &EC_ALL);
    let kw = sc.call_kw(
        n("OCIO.ExposureContrastTransform"),
        vec![],
        vec![
            ("direction", n("OCIO.TRANSFORM_DIR_INVERSE")),
            ("dynamicContrast", b(true)),
            ("logMidGray", i(1)),
            ("style", n("OCIO.EXPOSURE_CONTRAST_LOGARITHMIC")),
        ],
        Expect::Value,
    );
    sc.eval(r(kw), &EC_ALL);
    let kw = sc.call_kw(
        n("OCIO.ExposureContrastTransform"),
        vec![],
        vec![("gamma", f(0.0)), ("dynamicGamma", none())],
        Expect::Value,
    );
    sc.eval(r(kw), &EC_ALL);
    let odd = sc.call(n("OCIO.TransformDirection"), vec![i(5)]);
    let kw = sc.call_kw(
        n("OCIO.ExposureContrastTransform"),
        vec![],
        vec![("direction", r(odd))],
        Expect::Value,
    );
    sc.eval(r(kw), &EC_ALL);
    for args in [
        vec![i(0)],
        vec![n("OCIO.EXPOSURE_CONTRAST_VIDEO"), s("1")],
        vec![
            n("OCIO.EXPOSURE_CONTRAST_VIDEO"),
            f(0.0),
            f(0.0),
            f(0.0),
            f(0.0),
            f(0.0),
            f(0.0),
            s("x"),
        ],
    ] {
        sc.raises(n("OCIO.ExposureContrastTransform"), args);
    }
    sc.call_kw(
        n("OCIO.ExposureContrastTransform"),
        vec![],
        vec![("exposures", f(1.0))],
        Expect::Raised,
    );
    sc
}

/// The class behaves as the wheel's.
#[test]
fn exposure_contrast_behaves_as_the_wheels() {
    compare(&[exposure_contrast_script()]);
}

// ---------------------------------------------------------------------------------------------
// GradingPrimaryTransform, GradingToneTransform

#[test]
fn primary_and_tone_have_the_wheels_surface() {
    surface(&["GradingPrimaryTransform", "GradingToneTransform"]);
}

const VALUE_ALL: [&str; 7] = [
    "getStyle",
    "getValue",
    "isDynamic",
    "getDirection",
    "getTransformType",
    "validate",
    "__repr__",
];

/// The steps common to `GradingPrimaryTransform` (`class`, its value class `value`) and
/// `GradingToneTransform`: constructors, styles, values (copies both ways), the dynamic state.
fn value_transform_script(class: &str, value: &str, invalid: fn(&mut Script, usize)) -> Script {
    let mut sc = Script::new();
    let cls = format!("OCIO.{class}");
    let val = format!("OCIO.{value}");
    let t = sc.call(n(&cls), vec![]);
    sc.eval(r(t), &VALUE_ALL);
    base_methods(&mut sc, t, true);
    metadata(&mut sc, t);
    // getValue: a copy each time; changing it changes nothing.
    let v1 = sc.call(ra(t, "getValue"), vec![]);
    let v2 = sc.call(ra(t, "getValue"), vec![]);
    sc.call(n("operator.is_"), vec![r(v1), r(v2)]);
    invalid(&mut sc, v1);
    sc.repr(r(t));
    sc.raises(ra(t, "setValue"), vec![r(v1)]);
    sc.repr(r(t));
    sc.raises(ra(t, "setValue"), vec![none()]);
    sc.raises(ra(t, "setValue"), vec![i(0)]);
    // setValue: a copy; changing the object given after changes nothing.
    for style in ["GRADING_LIN", "GRADING_VIDEO", "GRADING_LOG"] {
        let v = sc.call(n(&val), vec![n(&format!("OCIO.{style}"))]);
        sc.call(ra(t, "setValue"), vec![r(v)]);
        sc.eval(r(t), &["getValue", "__repr__"]);
        invalid(&mut sc, v);
        sc.repr(r(t));
    }
    // The styles: each, then the default values of the new one.
    for style in ["GRADING_LIN", "GRADING_VIDEO", "GRADING_LOG", "GRADING_LOG"] {
        sc.call(ra(t, "setStyle"), vec![n(&format!("OCIO.{style}"))]);
        sc.eval(r(t), &["getStyle", "getValue", "__repr__"]);
    }
    sc.raises(ra(t, "setStyle"), vec![i(0)]);
    sc.raises(ra(t, "setStyle"), vec![none()]);
    // The dynamic state.
    sc.call(ra(t, "makeDynamic"), vec![]);
    sc.eval(r(t), &["isDynamic", "__repr__"]);
    sc.call(ra(t, "makeNonDynamic"), vec![]);
    sc.eval(r(t), &["isDynamic", "__repr__"]);
    // The constructors.
    let v = sc.call(n(&val), vec![n("OCIO.GRADING_LIN")]);
    let full = sc.call(
        n(&cls),
        vec![
            r(v),
            n("OCIO.GRADING_LIN"),
            b(true),
            n("OCIO.TRANSFORM_DIR_INVERSE"),
        ],
    );
    sc.eval(r(full), &VALUE_ALL);
    let kw = sc.call_kw(
        n(&cls),
        vec![],
        vec![
            ("dir", n("OCIO.TRANSFORM_DIR_INVERSE")),
            ("values", r(v)),
            ("style", n("OCIO.GRADING_VIDEO")),
        ],
        Expect::Value,
    );
    sc.eval(r(kw), &VALUE_ALL);
    for style in ["GRADING_LIN", "GRADING_VIDEO"] {
        let s2 = sc.call(n(&cls), vec![n(&format!("OCIO.{style}"))]);
        sc.eval(r(s2), &VALUE_ALL);
        let s3 = sc.call(n(&cls), vec![n(&format!("OCIO.{style}")), b(true)]);
        sc.eval(r(s3), &VALUE_ALL);
    }
    let kw = sc.call_kw(
        n(&cls),
        vec![],
        vec![("dynamic", i(1)), ("dir", n("OCIO.TRANSFORM_DIR_INVERSE"))],
        Expect::Value,
    );
    sc.eval(r(kw), &VALUE_ALL);
    let odd = sc.call(n("OCIO.TransformDirection"), vec![i(5)]);
    let kw = sc.call_kw(n(&cls), vec![], vec![("dir", r(odd))], Expect::Raised);
    let _ = kw;
    invalid(&mut sc, v);
    sc.raises(n(&cls), vec![r(v)]);
    sc.raises(n(&cls), vec![r(v), n("OCIO.GRADING_LOG")]);
    for args in [
        vec![none()],
        vec![i(0)],
        vec![n("OCIO.GRADING_LOG"), s("x")],
        vec![n("OCIO.TRANSFORM_DIR_FORWARD")],
    ] {
        sc.raises(n(&cls), args);
    }
    sc.call_kw(
        n(&cls),
        vec![],
        vec![("direction", n("OCIO.TRANSFORM_DIR_FORWARD"))],
        Expect::Raised,
    );
    sc
}

/// A `GradingPrimary` made invalid for every style: gamma and contrast 0.
fn invalid_primary(sc: &mut Script, v: usize) {
    let zero = sc.call(n("OCIO.GradingRGBM"), vec![f(0.0), f(1.0), f(1.0), f(1.0)]);
    sc.step(
        serde_json::json!({ "setattr": r(v), "name": "gamma", "value": r(zero) }),
        Expect::Value,
    );
    sc.step(
        serde_json::json!({ "setattr": r(v), "name": "contrast", "value": r(zero) }),
        Expect::Value,
    );
}

/// A `GradingTone` made invalid: its s-contrast out of range.
fn invalid_tone(sc: &mut Script, v: usize) {
    sc.step(
        serde_json::json!({ "setattr": r(v), "name": "scontrast", "value": f(3.0) }),
        Expect::Value,
    );
}

/// The classes behave as the wheel's.
#[test]
fn primary_and_tone_behave_as_the_wheels() {
    compare(&[
        value_transform_script("GradingPrimaryTransform", "GradingPrimary", invalid_primary),
        value_transform_script("GradingToneTransform", "GradingTone", invalid_tone),
    ]);
}

/// A style outside the enumerators, which the wheel's grading transforms store (their `repr`
/// then kills the wheel's process), is the port's error (U-112): `setStyle` and each
/// constructor's `style`; a constructor's values are validated first (with that style, as the
/// wheel's), so their error comes first. The wheel runs the same steps without an error, its
/// getter giving the number back.
#[test]
fn a_grading_style_outside_the_enumerators_is_the_ports_error() {
    let mut sc = Script::new();
    let mut refused = Vec::new();
    let mut getters = Vec::new();
    let st = sc.call(n("OCIO.GradingStyle"), vec![i(7)]);
    for class in [
        "GradingPrimaryTransform",
        "GradingToneTransform",
        "GradingRGBCurveTransform",
        "GradingHueCurveTransform",
    ] {
        let t = sc.call(n(&format!("OCIO.{class}")), vec![]);
        refused.push(sc.call(ra(t, "setStyle"), vec![r(st)]));
        getters.push(sc.call(ra(t, "getStyle"), vec![]));
        // The constructor through `min`'s key, so that no step's result is the transform,
        // whose repr would kill the wheel's process.
        refused.push(sc.call_kw(
            n("min"),
            vec![l(vec![r(st)])],
            vec![("key", n(&format!("OCIO.{class}")))],
            Expect::Value,
        ));
    }
    let theirs = common::wheel(&[sc.json()]);
    let theirs = theirs[0]["steps"].as_array().unwrap();
    let ours = common::port(&sc.json());
    let ours = ours["steps"].as_array().unwrap();
    assert_eq!(theirs.len(), sc.steps.len());
    assert_eq!(ours.len(), sc.steps.len());
    assert_eq!(refused.len(), 8);
    for (k, (step, wheel_step)) in ours.iter().zip(theirs).enumerate() {
        assert!(wheel_step.get("value").is_some(), "step {k}: {wheel_step}");
        if refused.contains(&k) {
            assert_eq!(step["raised"]["type"], "builtins.ValueError", "step {k}");
            let message = String::from_utf8(ocio_testkit::oracle_values::bytes(
                &step["raised"]["message"],
            ))
            .unwrap();
            assert_eq!(
                message,
                "PyOpenColorIO.PyOpenColorIO.GradingStyle(7): the wheel stores an enum value \
                 outside its enumerators, which the port's library can't hold \
                 (docs/improvements.md, U-112)",
                "step {k}"
            );
        } else if !getters.contains(&k) {
            // The getter after the refused setter: the wheel's number, the port's unchanged
            // style.
            assert!(step.get("value").is_some(), "step {k}: {step}");
        }
    }
    // The getters: the number in the wheel.
    assert_eq!(getters.len(), 4);
    for k in getters {
        let repr = String::from_utf8(ocio_testkit::oracle_values::bytes(
            &theirs[k]["value"]["repr"],
        ))
        .unwrap();
        assert_eq!(repr, "<GradingStyle.???: 7>", "step {k}");
    }
    // Valid values and the style: the port's error (the wheel's transform can't be shown).
    for (class, value) in [
        ("GradingPrimaryTransform", "GradingPrimary"),
        ("GradingToneTransform", "GradingTone"),
        ("GradingRGBCurveTransform", "GradingRGBCurve"),
        ("GradingHueCurveTransform", "GradingHueCurve"),
    ] {
        let mut sc = Script::new();
        let st = sc.call(n("OCIO.GradingStyle"), vec![i(7)]);
        let v = sc.call(n(&format!("OCIO.{value}")), vec![n("OCIO.GRADING_VIDEO")]);
        sc.call(n(&format!("OCIO.{class}")), vec![r(v), r(st)]);
        let ours = common::port(&sc.json());
        let step = &ours["steps"][2];
        assert_eq!(
            step["raised"]["type"], "builtins.ValueError",
            "{class}: {step}"
        );
        let message = String::from_utf8(ocio_testkit::oracle_values::bytes(
            &step["raised"]["message"],
        ))
        .unwrap();
        assert!(
            message.ends_with("(docs/improvements.md, U-112)"),
            "{class}: {message}"
        );
    }
    // Invalid values: the values' error, as the wheel's.
    let mut sc = Script::new();
    let st = sc.call(n("OCIO.GradingStyle"), vec![i(7)]);
    let v = sc.call(n("OCIO.GradingPrimary"), vec![n("OCIO.GRADING_LOG")]);
    invalid_primary(&mut sc, v);
    sc.raises(n("OCIO.GradingPrimaryTransform"), vec![r(v), r(st)]);
    let v = sc.call(n("OCIO.GradingTone"), vec![n("OCIO.GRADING_LOG")]);
    invalid_tone(&mut sc, v);
    sc.raises(n("OCIO.GradingToneTransform"), vec![r(v), r(st)]);
    compare(&[sc]);
}

// ---------------------------------------------------------------------------------------------
// GradingRGBCurveTransform, GradingHueCurveTransform

#[test]
fn curve_transforms_have_the_wheels_surface() {
    surface(&["GradingRGBCurveTransform", "GradingHueCurveTransform"]);
}

const CURVE_ALL: [&str; 6] = [
    "getStyle",
    "isDynamic",
    "getDirection",
    "getTransformType",
    "validate",
    "__repr__",
];

/// What differs between the two curve transforms in their scripts.
struct CurveClass {
    class: &'static str,
    value: &'static str,
    /// The curve the scripts change, and its curve type's name.
    curve: &'static str,
    curve_type: &'static str,
    /// The curve type enum.
    curve_enum: &'static str,
    /// Points that make that curve valid and different from its default, then invalid.
    valid: [f64; 6],
    invalid: [f64; 6],
    /// The flag's getter and setter (`BypassLinToLog`, `RGBToHSY`) and two values of it.
    flag: (&'static str, &'static str, &'static str, &'static str),
}

const RGB: CurveClass = CurveClass {
    class: "GradingRGBCurveTransform",
    value: "GradingRGBCurve",
    curve: "red",
    curve_type: "RGB_RED",
    curve_enum: "RGBCurveType",
    valid: [0.0, 0.1, 0.5, 0.6, 1.0, 0.9],
    invalid: [0.0, 0.1, 0.5, 0.6, 0.4, 0.9],
    flag: ("getBypassLinToLog", "setBypassLinToLog", "True", "False"),
};

const HUE: CurveClass = CurveClass {
    class: "GradingHueCurveTransform",
    value: "GradingHueCurve",
    curve: "lum_sat",
    curve_type: "LUM_SAT",
    curve_enum: "HueCurveType",
    valid: [0.0, 1.2, 0.5, 0.8, 1.0, 1.0],
    invalid: [0.0, 1.2, 0.5, 0.8, 0.4, 1.0],
    flag: (
        "getRGBToHSY",
        "setRGBToHSY",
        "OCIO.HSY_TRANSFORM_NONE",
        "OCIO.HSY_TRANSFORM_1",
    ),
};

/// A flag value: `True`/`False`, or a name.
fn flag_value(v: &str) -> serde_json::Value {
    match v {
        "True" => b(true),
        "False" => b(false),
        name => n(name),
    }
}

/// `curve` of the value object `v` set to the points `p` through its control point iterator
/// (the curve has three points).
fn set_points(sc: &mut Script, v: usize, curve: &str, p: &[f64; 6]) {
    let it = sc.call(ra(v, &format!("{curve}.getControlPoints")), vec![]);
    for k in 0..3 {
        let point = sc.call(
            n("OCIO.GradingControlPoint"),
            vec![f(p[2 * k]), f(p[2 * k + 1])],
        );
        sc.call(ra(it, "__setitem__"), vec![i(k as i128), r(point)]);
    }
}

fn curve_transform_script(c: &CurveClass) -> Script {
    let mut sc = Script::new();
    let cls = format!("OCIO.{}", c.class);
    let t = sc.call(n(&cls), vec![]);
    sc.eval(r(t), &CURVE_ALL);
    base_methods(&mut sc, t, true);
    metadata(&mut sc, t);
    // getValue: the transform's own curves, the same object each time.
    let v = sc.call(ra(t, "getValue"), vec![]);
    let v2 = sc.call(ra(t, "getValue"), vec![]);
    sc.call(n("operator.is_"), vec![r(v), r(v2)]);
    let before = sc.call(n("copy.deepcopy"), vec![r(t)]);
    // Changed through it: the transform shows it, unvalidated.
    set_points(&mut sc, v, c.curve, &c.valid);
    sc.eval(r(t), &CURVE_ALL);
    let copy = sc.call(n("copy.deepcopy"), vec![r(t)]);
    sc.repr(r(before));
    let group = sc.call(n("OCIO.GroupTransform"), vec![]);
    sc.call(ra(group, "appendTransform"), vec![r(t)]);
    set_points(&mut sc, v, c.curve, &c.invalid);
    sc.repr(r(t));
    sc.repr(r(group));
    sc.raises(ra(t, "validate"), vec![]);
    sc.raises(ra(group, "validate"), vec![]);
    sc.repr(r(copy));
    let curve = format!("OCIO.{}", c.curve_type);
    sc.call(ra(t, "getSlope"), vec![n(&curve), i(1)]);
    sc.call(ra(t, "slopesAreDefault"), vec![n(&curve)]);
    // A slope the copy refuses (it doesn't validate): the curves stay the object's.
    sc.raises(ra(t, "setSlope"), vec![n(&curve), i(1), f(0.5)]);
    sc.raises(ra(t, "setSlope"), vec![n(&curve), i(7), f(0.5)]);
    let v3 = sc.call(ra(t, "getValue"), vec![]);
    sc.call(n("operator.is_"), vec![r(v), r(v3)]);
    // A value refused: the same.
    let bad = sc.call(n(&format!("OCIO.{}", c.value)), vec![n("OCIO.GRADING_LOG")]);
    set_points(&mut sc, bad, c.curve, &c.invalid);
    sc.raises(ra(t, "setValue"), vec![r(bad)]);
    let v4 = sc.call(ra(t, "getValue"), vec![]);
    sc.call(n("operator.is_"), vec![r(v), r(v4)]);
    // Made valid again; a slope set: a copy of the curves replaces them.
    set_points(&mut sc, v, c.curve, &c.valid);
    sc.call(ra(t, "validate"), vec![]);
    sc.call(ra(t, "setSlope"), vec![n(&curve), i(1), f(0.5)]);
    sc.eval(r(t), &CURVE_ALL);
    let v5 = sc.call(ra(t, "getValue"), vec![]);
    sc.call(n("operator.is_"), vec![r(v), r(v5)]);
    set_points(&mut sc, v, c.curve, &c.invalid);
    sc.call(ra(t, "validate"), vec![]);
    sc.call(ra(t, "getSlope"), vec![n(&curve), i(1)]);
    sc.call(ra(t, "slopesAreDefault"), vec![n(&curve)]);
    // setValue: a copy of the value given, the object given not the transform's.
    let w = sc.call(n(&format!("OCIO.{}", c.value)), vec![n("OCIO.GRADING_LOG")]);
    set_points(&mut sc, w, c.curve, &c.valid);
    sc.call(ra(t, "setValue"), vec![r(w)]);
    let v6 = sc.call(ra(t, "getValue"), vec![]);
    sc.call(n("operator.is_"), vec![r(w), r(v6)]);
    set_points(&mut sc, w, c.curve, &c.invalid);
    sc.call(ra(t, "validate"), vec![]);
    sc.call(ra(t, "setValue"), vec![r(v6)]);
    let v7 = sc.call(ra(t, "getValue"), vec![]);
    sc.call(n("operator.is_"), vec![r(v6), r(v7)]);
    // The same style keeps the curves; another resets them.
    sc.call(ra(t, "setStyle"), vec![n("OCIO.GRADING_LOG")]);
    let v8 = sc.call(ra(t, "getValue"), vec![]);
    sc.call(n("operator.is_"), vec![r(v7), r(v8)]);
    sc.call(ra(t, "setStyle"), vec![n("OCIO.GRADING_LIN")]);
    let v9 = sc.call(ra(t, "getValue"), vec![]);
    sc.call(n("operator.is_"), vec![r(v8), r(v9)]);
    sc.eval(r(t), &CURVE_ALL);
    for style in ["GRADING_VIDEO", "GRADING_LOG"] {
        sc.call(ra(t, "setStyle"), vec![n(&format!("OCIO.{style}"))]);
        sc.eval(r(t), &CURVE_ALL);
    }
    // Curve types outside the curves, and the casters.
    let odd = sc.call(n(&format!("OCIO.{}", c.curve_enum)), vec![i(9)]);
    sc.raises(ra(t, "getSlope"), vec![r(odd), i(0)]);
    sc.raises(ra(t, "setSlope"), vec![r(odd), i(0), f(1.0)]);
    sc.raises(ra(t, "slopesAreDefault"), vec![r(odd)]);
    sc.raises(ra(t, "getSlope"), vec![i(0), i(0)]);
    sc.raises(ra(t, "getSlope"), vec![n(&curve), i(-1)]);
    sc.raises(ra(t, "getSlope"), vec![n(&curve), i(3)]);
    sc.call_kw(
        ra(t, "getSlope"),
        vec![],
        vec![
            ("index", i(0)),
            (
                if c.class == RGB.class {
                    "channel"
                } else {
                    "curve"
                },
                n(&curve),
            ),
        ],
        Expect::Value,
    );
    // The flag.
    let (get, set, on, off) = c.flag;
    sc.call(ra(t, get), vec![]);
    sc.call(ra(t, set), vec![flag_value(on)]);
    sc.eval(r(t), &[get, "__repr__"]);
    sc.call(ra(t, set), vec![flag_value(off)]);
    sc.eval(r(t), &[get, "__repr__"]);
    sc.call(ra(t, "makeDynamic"), vec![]);
    sc.eval(r(t), &CURVE_ALL);
    sc.call(ra(t, "makeNonDynamic"), vec![]);
    // The constructors (of values made valid again).
    set_points(&mut sc, w, c.curve, &c.valid);
    set_points(&mut sc, v, c.curve, &c.valid);
    let full = sc.call(
        n(&cls),
        vec![
            r(w),
            n("OCIO.GRADING_LIN"),
            b(true),
            n("OCIO.TRANSFORM_DIR_INVERSE"),
        ],
    );
    sc.eval(r(full), &CURVE_ALL);
    let fv = sc.call(ra(full, "getValue"), vec![]);
    sc.call(n("operator.is_"), vec![r(fv), r(w)]);
    let kw = sc.call_kw(
        n(&cls),
        vec![],
        vec![("dir", n("OCIO.TRANSFORM_DIR_INVERSE")), ("values", r(v))],
        Expect::Value,
    );
    sc.eval(r(kw), &CURVE_ALL);
    let kw = sc.call_kw(
        n(&cls),
        vec![],
        vec![("style", n("OCIO.GRADING_VIDEO")), ("dynamic", b(true))],
        Expect::Value,
    );
    sc.eval(r(kw), &CURVE_ALL);
    let bad = sc.call(n(&format!("OCIO.{}", c.value)), vec![n("OCIO.GRADING_LOG")]);
    set_points(&mut sc, bad, c.curve, &c.invalid);
    sc.raises(n(&cls), vec![r(bad)]);
    for args in [vec![i(0)], vec![n("OCIO.GRADING_LOG"), s("x")], vec![r(t)]] {
        sc.raises(n(&cls), args);
    }
    sc.raises(ra(t, "setValue"), vec![i(0)]);
    sc
}

/// The classes behave as the wheel's.
#[test]
fn curve_transforms_behave_as_the_wheels() {
    compare(&[curve_transform_script(&RGB), curve_transform_script(&HUE)]);
}

/// A copy of a curve transform (`createEditableCopy`) sets its curves again with the property's
/// `setValue` (`GradingRGBCurveOpData.cpp:65`, `GradingHueCurveOpData.cpp:73` @ v2.5.2): curves
/// changed in place to curves that don't validate are refused, with the curves' own text, by
/// `deepcopy`, by the config objects' transform setters and constructors (which keep a copy, and
/// keep the transform they had), and by the copy of a config object holding such a transform
/// (changed through its getter); a group shares its children, so its copy succeeds.
fn curve_copy_script(c: &CurveClass) -> Script {
    let mut sc = Script::new();
    let cls = format!("OCIO.{}", c.class);
    let t = sc.call(n(&cls), vec![]);
    let v = sc.call(ra(t, "getValue"), vec![]);
    set_points(&mut sc, v, c.curve, &c.invalid);
    sc.raises(n("copy.deepcopy"), vec![r(t)]);
    sc.raises(ra(t, "__deepcopy__"), vec![d(vec![])]);
    // Color space: the setter keeps the transform it had.
    let kept = sc.call_kw(
        n(&cls),
        vec![],
        vec![("style", n("OCIO.GRADING_LIN"))],
        Expect::Value,
    );
    let cs = sc.call(n("OCIO.ColorSpace"), vec![]);
    let to_ref = n("OCIO.COLORSPACE_DIR_TO_REFERENCE");
    let from_ref = n("OCIO.COLORSPACE_DIR_FROM_REFERENCE");
    sc.call(ra(cs, "setTransform"), vec![r(kept), to_ref.clone()]);
    sc.raises(ra(cs, "setTransform"), vec![r(t), to_ref.clone()]);
    sc.raises(ra(cs, "setTransform"), vec![r(t), from_ref.clone()]);
    let held = sc.call(ra(cs, "getTransform"), vec![to_ref.clone()]);
    sc.repr(r(held));
    sc.call(ra(cs, "getTransform"), vec![from_ref.clone()]);
    for kw in ["toReference", "fromReference"] {
        sc.call_kw(
            n("OCIO.ColorSpace"),
            vec![],
            vec![(kw, r(t))],
            Expect::Raised,
        );
    }
    // Look, named transform, view transform.
    let look = sc.call(n("OCIO.Look"), vec![]);
    sc.raises(ra(look, "setTransform"), vec![r(t)]);
    sc.raises(ra(look, "setInverseTransform"), vec![r(t)]);
    sc.call(ra(look, "getTransform"), vec![]);
    for kw in ["transform", "inverseTransform"] {
        sc.call_kw(n("OCIO.Look"), vec![], vec![(kw, r(t))], Expect::Raised);
    }
    let nt = sc.call(n("OCIO.NamedTransform"), vec![]);
    for dir in ["OCIO.TRANSFORM_DIR_FORWARD", "OCIO.TRANSFORM_DIR_INVERSE"] {
        sc.raises(ra(nt, "setTransform"), vec![r(t), n(dir)]);
        sc.call(ra(nt, "getTransform"), vec![n(dir)]);
    }
    for kw in ["forwardTransform", "inverseTransform"] {
        sc.call_kw(
            n("OCIO.NamedTransform"),
            vec![],
            vec![(kw, r(t))],
            Expect::Raised,
        );
    }
    let vt = sc.call(n("OCIO.ViewTransform"), vec![]);
    for dir in [
        "OCIO.VIEWTRANSFORM_DIR_TO_REFERENCE",
        "OCIO.VIEWTRANSFORM_DIR_FROM_REFERENCE",
    ] {
        sc.raises(ra(vt, "setTransform"), vec![r(t), n(dir)]);
        sc.call(ra(vt, "getTransform"), vec![n(dir)]);
    }
    for kw in ["toReference", "fromReference"] {
        sc.call_kw(
            n("OCIO.ViewTransform"),
            vec![],
            vec![(kw, r(t))],
            Expect::Raised,
        );
    }
    // A group shares its children: its copy doesn't copy them.
    let group = sc.call(n("OCIO.GroupTransform"), vec![]);
    sc.call(ra(group, "appendTransform"), vec![r(t)]);
    let gcopy = sc.call(n("copy.deepcopy"), vec![r(group)]);
    sc.repr(r(gcopy));
    // A config object's copy copies its transforms: curves changed through its getter.
    let held_value = sc.call(ra(held, "getValue"), vec![]);
    set_points(&mut sc, held_value, c.curve, &c.invalid);
    sc.repr(r(cs));
    sc.raises(n("copy.deepcopy"), vec![r(cs)]);
    sc.call(ra(look, "setTransform"), vec![r(kept)]);
    let lheld = sc.call(ra(look, "getTransform"), vec![]);
    let lvalue = sc.call(ra(lheld, "getValue"), vec![]);
    set_points(&mut sc, lvalue, c.curve, &c.invalid);
    sc.raises(n("copy.deepcopy"), vec![r(look)]);
    // Made valid again: the copies succeed, with the curves.
    set_points(&mut sc, v, c.curve, &c.valid);
    let copy = sc.call(n("copy.deepcopy"), vec![r(t)]);
    sc.eval(r(copy), &CURVE_ALL);
    sc.call(ra(cs, "setTransform"), vec![r(t), from_ref.clone()]);
    let got = sc.call(ra(cs, "getTransform"), vec![from_ref]);
    sc.repr(r(got));
    sc
}

/// Copies of transforms holding curves that don't validate are refused as the wheel's.
#[test]
fn copies_set_the_curves_again() {
    compare(&[curve_copy_script(&RGB), curve_copy_script(&HUE)]);
}

/// An `HSYTransformStyle` outside the enumerators, which the wheel's hue curve transform stores
/// (its getter giving the number back), is the port's error (U-112).
#[test]
fn an_hsy_style_outside_the_enumerators_is_the_ports_error() {
    let mut sc = Script::new();
    let st = sc.call(n("OCIO.HSYTransformStyle"), vec![i(7)]);
    let t = sc.call(n("OCIO.GradingHueCurveTransform"), vec![]);
    let refused = sc.call(ra(t, "setRGBToHSY"), vec![r(st)]);
    let getter = sc.call(ra(t, "getRGBToHSY"), vec![]);
    let theirs = common::wheel(&[sc.json()]);
    let theirs = theirs[0]["steps"].as_array().unwrap();
    let ours = common::port(&sc.json());
    let ours = ours["steps"].as_array().unwrap();
    assert_eq!(theirs.len(), 4);
    assert!(
        theirs.iter().all(|s| s.get("value").is_some()),
        "{theirs:?}"
    );
    let repr = String::from_utf8(ocio_testkit::oracle_values::bytes(
        &theirs[getter]["value"]["repr"],
    ))
    .unwrap();
    assert_eq!(repr, "<HSYTransformStyle.???: 7>");
    assert_eq!(ours[refused]["raised"]["type"], "builtins.ValueError");
    let message = String::from_utf8(ocio_testkit::oracle_values::bytes(
        &ours[refused]["raised"]["message"],
    ))
    .unwrap();
    assert_eq!(
        message,
        "PyOpenColorIO.PyOpenColorIO.HSYTransformStyle(7): the wheel stores an enum value \
         outside its enumerators, which the port's library can't hold \
         (docs/improvements.md, U-112)"
    );
}

/// `None` for a curve transform's values (`setValue`, the constructor), which the wheel
/// dereferences (its process dies), is the port's error (U-167). Each script runs in a process
/// of its own in the wheel, which dies at its last step, and only there.
#[test]
fn null_values_are_the_ports_error() {
    let mut scripts = Vec::new();
    for (class, value) in [
        ("GradingRGBCurveTransform", "GradingRGBCurve"),
        ("GradingHueCurveTransform", "GradingHueCurve"),
    ] {
        let mut sc = Script::new();
        let t = sc.call(n(&format!("OCIO.{class}")), vec![]);
        sc.call(ra(t, "setValue"), vec![none()]);
        scripts.push((sc, value));
        let mut sc = Script::new();
        sc.call(
            n(&format!("OCIO.{class}")),
            vec![none(), n("OCIO.GRADING_LOG")],
        );
        scripts.push((sc, value));
    }
    let json: Vec<serde_json::Value> = scripts.iter().map(|(sc, _)| sc.json()).collect();
    let result = ocio_testkit::Oracle::get()
        .call("py_eval", serde_json::json!({ "scripts": json }), &[])
        .result;
    let reports = result["scripts"].as_array().unwrap();
    assert_eq!(reports.len(), 4);
    let mut shortened = Vec::new();
    for (k, report) in reports.iter().enumerate() {
        let code = report["crashed"]["returncode"].as_i64();
        assert!(code.is_some_and(|c| c != 0), "script {k}: {report}");
        let mut steps = scripts[k].0.json();
        steps["steps"].as_array_mut().unwrap().pop();
        if !steps["steps"].as_array().unwrap().is_empty() {
            shortened.push(steps);
        }
    }
    for report in common::wheel(&shortened) {
        for step in report["steps"].as_array().unwrap() {
            assert!(step.get("value").is_some(), "{step}");
        }
    }
    for (sc, value) in &scripts {
        let ours = common::port(&sc.json());
        let steps = ours["steps"].as_array().unwrap();
        let last = steps.last().unwrap();
        for step in &steps[..steps.len() - 1] {
            assert!(step.get("value").is_some(), "{step}");
        }
        assert_eq!(last["raised"]["type"], "builtins.ValueError", "{last}");
        let message = String::from_utf8(ocio_testkit::oracle_values::bytes(
            &last["raised"]["message"],
        ))
        .unwrap();
        assert_eq!(
            message,
            format!(
                "PyOpenColorIO.PyOpenColorIO.{value}: None for a curve, a null pointer the wheel \
                 dereferences (its process dies; docs/improvements.md, U-167)"
            )
        );
    }
}
