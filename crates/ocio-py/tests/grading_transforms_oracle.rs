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
    Expect, Script, b, base_methods, compare, f, i, l, n, none, r, ra, s, special,
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
    for class in ["GradingPrimaryTransform", "GradingToneTransform"] {
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
    assert_eq!(refused.len(), 4);
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
    assert_eq!(getters.len(), 2);
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
