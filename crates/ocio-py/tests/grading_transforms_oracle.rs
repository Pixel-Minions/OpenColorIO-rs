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

use common::script::{Expect, Script, b, base_methods, compare, f, i, n, none, r, ra, s, special};

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
