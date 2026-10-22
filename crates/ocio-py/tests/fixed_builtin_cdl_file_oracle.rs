// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The classes of chunk 6.3c against the wheel (`FixedFunctionTransform`, `BuiltinTransform`,
//! `BuiltinTransformRegistry` and its iterators, `CDLTransform`, `FileTransform` and its
//! iterator): each class's surface (py_surface_walk.py's `_class` against the wheel's
//! py_surface), and their behaviour through the same step scripts on both sides: constructors
//! (every overload, positional, keyword, refused), every getter and setter with values whose
//! bits differ, the styles and their errors, the parameters, `equals`, `validate`, `repr`, the
//! registry's lookups ignoring case and its `KeyError`, the iterators (`len`, indexing, a
//! negative index, iteration to `StopIteration`), the format lists, and a direction outside the
//! enumerators (I-261: the fixed function and the CDL flip; the others keep it).
//!
//! And, without the wheel (U-112): a style or an interpolation outside the enumerators is the
//! port's error.
#![cfg(feature = "python")]

mod common;

use common::script::{
    Expect, Script, b, base_methods, by, compare, f, fl, i, l, n, none, r, ra, s, special, t,
};

const CLASSES: [&str; 5] = [
    "FixedFunctionTransform",
    "BuiltinTransform",
    "BuiltinTransformRegistry",
    "CDLTransform",
    "FileTransform",
];

/// Each class has the wheel's surface, member for member.
#[test]
fn the_classes_have_the_wheels_surface() {
    let report = common::wheel_surface();
    for c in CLASSES {
        let theirs = common::wheel_member(&report, c);
        let ours = common::port_member(c);
        assert!(
            theirs["members"].as_array().is_some_and(|m| m.len() > 5),
            "{c}: {theirs}"
        );
        assert_eq!(ours, theirs, "{c}");
    }
}

/// A direction outside the enumerators, set twice, then validated (I-261).
fn odd_direction(sc: &mut Script, obj: usize, validates: bool) {
    let bad = sc.call(n("OCIO.TransformDirection"), vec![i(5)]);
    for _ in 0..2 {
        sc.call(ra(obj, "setDirection"), vec![r(bad)]);
        sc.eval(r(obj), &["getDirection", "__repr__"]);
    }
    let expect = if validates {
        Expect::Value
    } else {
        Expect::Raised
    };
    sc.step(
        serde_json::json!({ "call": ra(obj, "validate"), "args": [] }),
        expect,
    );
    sc.call(
        ra(obj, "setDirection"),
        vec![n("OCIO.TRANSFORM_DIR_FORWARD")],
    );
}

/// Every fixed-function style, with the parameters each needs.
const FF_STYLES: [(&str, &[f64]); 23] = [
    ("FIXED_FUNCTION_ACES_RED_MOD_03", &[]),
    ("FIXED_FUNCTION_ACES_RED_MOD_10", &[]),
    ("FIXED_FUNCTION_ACES_GLOW_03", &[]),
    ("FIXED_FUNCTION_ACES_GLOW_10", &[]),
    ("FIXED_FUNCTION_ACES_DARK_TO_DIM_10", &[]),
    ("FIXED_FUNCTION_REC2100_SURROUND", &[0.78]),
    ("FIXED_FUNCTION_RGB_TO_HSV", &[]),
    ("FIXED_FUNCTION_XYZ_TO_xyY", &[]),
    ("FIXED_FUNCTION_XYZ_TO_uvY", &[]),
    ("FIXED_FUNCTION_XYZ_TO_LUV", &[]),
    ("FIXED_FUNCTION_ACES_GAMUTMAP_02", &[]),
    ("FIXED_FUNCTION_ACES_GAMUTMAP_07", &[]),
    (
        "FIXED_FUNCTION_ACES_GAMUT_COMP_13",
        &[1.147, 1.264, 1.312, 0.815, 0.803, 0.88, 1.2],
    ),
    ("FIXED_FUNCTION_LIN_TO_PQ", &[]),
    (
        "FIXED_FUNCTION_LIN_TO_GAMMA_LOG",
        &[0.0, 0.18, 2.0, 0.2, 0.2, 0.3, 0.2, 0.01, 0.01, 0.1],
    ),
    (
        "FIXED_FUNCTION_LIN_TO_DOUBLE_LOG",
        &[
            10.0, 0.25, 0.5, -1.0, 0.0, -1.0, 0.0, 1.0, 1.0, 1.0, 0.5, 0.25, 0.5,
        ],
    ),
    (
        "FIXED_FUNCTION_ACES_OUTPUT_TRANSFORM_20",
        &[100.0, 0.64, 0.33, 0.30, 0.60, 0.15, 0.06, 0.3127, 0.3290],
    ),
    (
        "FIXED_FUNCTION_ACES_RGB_TO_JMH_20",
        &[0.64, 0.33, 0.30, 0.60, 0.15, 0.06, 0.3127, 0.3290],
    ),
    ("FIXED_FUNCTION_ACES_TONESCALE_COMPRESS_20", &[100.0]),
    (
        "FIXED_FUNCTION_ACES_GAMUT_COMPRESS_20",
        &[100.0, 0.64, 0.33, 0.30, 0.60, 0.15, 0.06, 0.3127, 0.3290],
    ),
    ("FIXED_FUNCTION_RGB_TO_HSY_LIN", &[]),
    ("FIXED_FUNCTION_RGB_TO_HSY_LOG", &[]),
    ("FIXED_FUNCTION_RGB_TO_HSY_VID", &[]),
];

fn fixed_function_script() -> Script {
    let mut sc = Script::new();
    let all = ["getStyle", "getParams", "getDirection", "__repr__"];
    for (name, params) in FF_STYLES {
        // Upstream doesn't implement the two gamut maps.
        let implemented = !name.contains("GAMUTMAP");
        let outcome = |ok: bool| if ok { Expect::Value } else { Expect::Raised };
        let style = n(&format!("OCIO.{name}"));
        sc.step(
            serde_json::json!({ "call": n("OCIO.FixedFunctionTransform"), "args": [style.clone()], "getters": all }),
            outcome(implemented && params.is_empty()),
        );
        sc.step(
            serde_json::json!({ "call": n("OCIO.FixedFunctionTransform"), "args": [style.clone(), fl(params)], "getters": all }),
            outcome(implemented),
        );
        sc.step(
            serde_json::json!({ "call": n("OCIO.FixedFunctionTransform"), "args": [style, fl(params), n("OCIO.TRANSFORM_DIR_INVERSE")], "getters": all }),
            outcome(implemented),
        );
    }
    let ff = sc.call(
        n("OCIO.FixedFunctionTransform"),
        vec![n("OCIO.FIXED_FUNCTION_ACES_RED_MOD_03")],
    );
    base_methods(&mut sc, ff, true);
    // Styles that make an inverse transform forward (I-80), the parameters' checks.
    sc.call(
        ra(ff, "setDirection"),
        vec![n("OCIO.TRANSFORM_DIR_INVERSE")],
    );
    for (style, _) in FF_STYLES {
        sc.step(
            serde_json::json!({ "call": ra(ff, "setStyle"), "args": [n(&format!("OCIO.{style}"))] }),
            if style.ends_with("GAMUTMAP_02") || style.ends_with("GAMUTMAP_07") {
                Expect::Raised
            } else {
                Expect::Value
            },
        );
        sc.eval(r(ff), &all);
    }
    sc.call(
        ra(ff, "setStyle"),
        vec![n("OCIO.FIXED_FUNCTION_REC2100_SURROUND")],
    );
    sc.raises(ra(ff, "validate"), vec![]);
    sc.call(ra(ff, "setParams"), vec![l(vec![special(1), special(2)])]);
    sc.eval(r(ff), &all);
    sc.raises(ra(ff, "validate"), vec![]);
    sc.call(ra(ff, "setParams"), vec![t(vec![i(1)])]);
    sc.call(ra(ff, "validate"), vec![]);
    sc.raises(ra(ff, "setParams"), vec![none()]);
    sc.raises(ra(ff, "setStyle"), vec![i(0)]);
    odd_direction(&mut sc, ff, true);
    sc.raises(n("OCIO.FixedFunctionTransform"), vec![]);
    sc.raises(n("OCIO.FixedFunctionTransform"), vec![none()]);
    sc.raises(n("OCIO.FixedFunctionTransform"), vec![i(1)]);
    sc.call_kw(
        n("OCIO.FixedFunctionTransform"),
        vec![],
        vec![
            ("params", fl(&[0.5])),
            ("style", n("OCIO.FIXED_FUNCTION_REC2100_SURROUND")),
        ],
        Expect::Value,
    );
    let a = sc.call(
        n("OCIO.FixedFunctionTransform"),
        vec![n("OCIO.FIXED_FUNCTION_ACES_GLOW_10")],
    );
    let b2 = sc.call(
        n("OCIO.FixedFunctionTransform"),
        vec![n("OCIO.FIXED_FUNCTION_ACES_GLOW_10")],
    );
    sc.call(ra(a, "equals"), vec![r(b2)]);
    sc.call(ra(a, "equals"), vec![r(ff)]);
    sc.raises(ra(a, "equals"), vec![none()]);
    sc
}

fn builtin_script() -> Script {
    let mut sc = Script::new();
    let all = ["getStyle", "getDescription", "getDirection", "__repr__"];
    let bt = sc.call(n("OCIO.BuiltinTransform"), vec![]);
    sc.eval(r(bt), &all);
    base_methods(&mut sc, bt, true);
    for style in [
        "acescct_to_aces2065-1",
        "ACEScct_to_ACES2065-1",
        "UTILITY - ACES-AP0_to_CIE-XYZ-D65_BFD",
        "IDENTITY\u{0}junk",
    ] {
        sc.call(ra(bt, "setStyle"), vec![s(style)]);
        sc.eval(r(bt), &all);
    }
    sc.raises(ra(bt, "setStyle"), vec![s("nope")]);
    sc.raises(ra(bt, "setStyle"), vec![s("")]);
    sc.raises(ra(bt, "setStyle"), vec![none()]);
    sc.call(ra(bt, "setStyle"), vec![by(b"identity")]);
    sc.eval(r(bt), &all);
    odd_direction(&mut sc, bt, false);
    let b2 = sc.call(
        n("OCIO.BuiltinTransform"),
        vec![s("ACEScct_to_ACES2065-1"), n("OCIO.TRANSFORM_DIR_INVERSE")],
    );
    sc.eval(r(b2), &all);
    let b3 = sc.call_kw(
        n("OCIO.BuiltinTransform"),
        vec![],
        vec![("style", s(""))],
        Expect::Value,
    );
    sc.eval(r(b3), &all);
    sc.call_kw(
        n("OCIO.BuiltinTransform"),
        vec![],
        vec![("style", s("nope"))],
        Expect::Raised,
    );
    sc.raises(n("OCIO.BuiltinTransform"), vec![none()]);
    sc.raises(n("OCIO.BuiltinTransform"), vec![i(1)]);
    sc
}

fn registry_script() -> Script {
    let mut sc = Script::new();
    let reg = sc.call(n("OCIO.BuiltinTransformRegistry"), vec![]);
    sc.call(n("len"), vec![r(reg)]);
    sc.call(n("OCIO.BuiltinTransformRegistry.__len__"), vec![r(reg)]);
    for style in [
        "IDENTITY",
        "identity",
        "ACEScct_to_ACES2065-1",
        "UTILITY - ACES-AP0_to_CIE-XYZ-D65_BFD",
    ] {
        sc.call(ra(reg, "__getitem__"), vec![s(style)]);
        sc.call(ra(reg, "__contains__"), vec![s(style)]);
    }
    for style in ["nope", "IDENTITY\u{0}", "caf\u{e9}"] {
        sc.raises(ra(reg, "__getitem__"), vec![s(style)]);
        sc.call(ra(reg, "__contains__"), vec![s(style)]);
    }
    sc.raises(ra(reg, "__getitem__"), vec![by(b"\xff\xfe")]);
    sc.call(ra(reg, "__contains__"), vec![by(b"identity")]);
    sc.raises(ra(reg, "__getitem__"), vec![i(0)]);
    sc.raises(ra(reg, "__contains__"), vec![i(0)]);
    sc.raises(ra(reg, "__contains__"), vec![none()]);
    // The style iterator.
    let it = sc.call(n("iter"), vec![r(reg)]);
    sc.call(n("type"), vec![r(it)]);
    sc.call(n("iter"), vec![r(it)]);
    sc.call(n("len"), vec![r(it)]);
    for k in [0, 1, 10] {
        sc.call(ra(it, "__getitem__"), vec![i(k)]);
    }
    sc.raises(ra(it, "__getitem__"), vec![i(-1)]);
    sc.raises(ra(it, "__getitem__"), vec![i(100_000)]);
    sc.call(n("next"), vec![r(it)]);
    sc.call(n("list"), vec![r(it)]);
    sc.raises(n("next"), vec![r(it)]);
    sc.call(n("list"), vec![r(reg)]);
    // The (style, description) iterator.
    let bi = sc.call(ra(reg, "getBuiltins"), vec![]);
    sc.call(n("type"), vec![r(bi)]);
    sc.call(n("len"), vec![r(bi)]);
    for k in [0, 7] {
        sc.call(ra(bi, "__getitem__"), vec![i(k)]);
    }
    sc.raises(ra(bi, "__getitem__"), vec![i(-1)]);
    sc.call(n("list"), vec![r(bi)]);
    sc.raises(n("next"), vec![r(bi)]);
    sc.raises(n("OCIO.BuiltinTransformRegistry"), vec![i(1)]);
    sc.raises(n("OCIO.BuiltinTransformRegistry.BuiltinIterator"), vec![]);
    let reg2 = sc.call(n("OCIO.BuiltinTransformRegistry"), vec![]);
    sc.call(n("operator.is_"), vec![r(reg), r(reg2)]);
    sc
}

fn cdl_script() -> Script {
    let mut sc = Script::new();
    let all = [
        "getSlope",
        "getOffset",
        "getPower",
        "getSOP",
        "getSat",
        "getSatLumaCoefs",
        "getID",
        "getFirstSOPDescription",
        "getStyle",
        "getDirection",
        "__repr__",
    ];
    let c = sc.call(n("OCIO.CDLTransform"), vec![]);
    sc.eval(r(c), &all);
    base_methods(&mut sc, c, true);
    for (k, v) in ["Slope", "Offset", "Power"].iter().enumerate() {
        sc.call(
            ra(c, &format!("set{v}")),
            vec![l((k..k + 3).map(special).collect())],
        );
        sc.eval(r(c), &all);
        sc.raises(ra(c, &format!("set{v}")), vec![fl(&[1.0; 4])]);
    }
    sc.raises(ra(c, "validate"), vec![]);
    sc.call(ra(c, "setSOP"), vec![l((0..9).map(special).collect())]);
    sc.eval(r(c), &all);
    sc.call(
        ra(c, "setSOP"),
        vec![fl(&[1.1, 1.2, 1.3, 0.1, 0.2, 0.3, 0.9, 1.0, 1.1])],
    );
    sc.call(ra(c, "validate"), vec![]);
    sc.raises(ra(c, "setSOP"), vec![fl(&[1.0; 8])]);
    for sat in [special(2), special(0), f(0.8), i(2)] {
        sc.call(ra(c, "setSat"), vec![sat]);
        sc.eval(r(c), &all);
    }
    sc.call(ra(c, "validate"), vec![]);
    sc.call(ra(c, "setSat"), vec![f(-1.0)]);
    sc.raises(ra(c, "validate"), vec![]);
    sc.call(ra(c, "setSat"), vec![f(1.0)]);
    for id in ["shot_01", "", "a\u{0}b", "caf\u{e9}"] {
        sc.call(ra(c, "setID"), vec![s(id)]);
        sc.call(ra(c, "setFirstSOPDescription"), vec![s(id)]);
        sc.eval(r(c), &all);
    }
    sc.call(ra(c, "setID"), vec![by(b"\xff")]);
    sc.raises(ra(c, "getID"), vec![]);
    sc.raises(ra(c, "setID"), vec![none()]);
    sc.raises(ra(c, "setFirstSOPDescription"), vec![none()]);
    sc.call(ra(c, "setStyle"), vec![n("OCIO.CDL_ASC")]);
    sc.eval(r(c), &all);
    sc.call(ra(c, "setStyle"), vec![n("OCIO.CDL_NO_CLAMP")]);
    sc.raises(ra(c, "setStyle"), vec![i(1)]);
    odd_direction(&mut sc, c, true);
    // Constructors.
    let c2 = sc.call(
        n("OCIO.CDLTransform"),
        vec![n("OCIO.TRANSFORM_DIR_INVERSE")],
    );
    sc.eval(r(c2), &all);
    let c3 = sc.call(
        n("OCIO.CDLTransform"),
        vec![
            fl(&[1.1, 1.2, 1.3]),
            fl(&[0.1, 0.2, 0.3]),
            fl(&[0.9, 1.0, 1.1]),
            f(0.8),
            s("myid"),
            s("my description"),
            n("OCIO.TRANSFORM_DIR_INVERSE"),
        ],
    );
    sc.eval(r(c3), &all);
    let c4 = sc.call_kw(
        n("OCIO.CDLTransform"),
        vec![],
        vec![("sat", i(2)), ("id", s("x")), ("power", fl(&[2.0; 3]))],
        Expect::Value,
    );
    sc.eval(r(c4), &all);
    let cut = sc.call_kw(
        n("OCIO.CDLTransform"),
        vec![],
        vec![("id", s("a\u{0}b")), ("description", s("d\u{0}e"))],
        Expect::Value,
    );
    sc.eval(r(cut), &all);
    sc.call_kw(
        n("OCIO.CDLTransform"),
        vec![],
        vec![("direction", n("OCIO.TRANSFORM_DIR_INVERSE"))],
        Expect::Value,
    );
    sc.call_kw(
        n("OCIO.CDLTransform"),
        vec![],
        vec![("power", fl(&[-1.0; 3]))],
        Expect::Raised,
    );
    sc.raises(n("OCIO.CDLTransform"), vec![none()]);
    sc.raises(n("OCIO.CDLTransform"), vec![fl(&[1.0; 3]), none()]);
    sc.call(ra(c3, "equals"), vec![r(c3)]);
    sc.call(ra(c3, "equals"), vec![r(c4)]);
    let c5 = sc.call(n("copy.deepcopy"), vec![r(c3)]);
    sc.call(ra(c3, "equals"), vec![r(c5)]);
    sc.call(ra(c5, "setID"), vec![s("other")]);
    sc.call(ra(c3, "equals"), vec![r(c5)]);
    // The file functions' refusals (the files' contents: cdl_files_oracle.rs).
    sc.raises(n("OCIO.CDLTransform.CreateFromFile"), vec![s(""), s("")]);
    sc.raises(n("OCIO.CDLTransform.CreateFromFile"), vec![none(), s("")]);
    sc.raises(
        n("OCIO.CDLTransform.CreateFromFile"),
        vec![s("missing.cc"), none()],
    );
    sc.raises(n("OCIO.CDLTransform.CreateGroupFromFile"), vec![s("")]);
    sc.raises(n("OCIO.CDLTransform.CreateGroupFromFile"), vec![none()]);
    sc
}

fn file_script() -> Script {
    let mut sc = Script::new();
    let all = [
        "getSrc",
        "getCCCId",
        "getCDLStyle",
        "getInterpolation",
        "getDirection",
        "__repr__",
    ];
    let ft = sc.call(n("OCIO.FileTransform"), vec![]);
    sc.eval(r(ft), &all);
    base_methods(&mut sc, ft, false);
    for src in ["lut.cube", "a\u{0}b.spi1d", "caf\u{e9}.cc", ""] {
        sc.call(ra(ft, "setSrc"), vec![s(src)]);
        sc.call(ra(ft, "setCCCId"), vec![s(src)]);
        sc.eval(r(ft), &all);
    }
    sc.raises(ra(ft, "setSrc"), vec![none()]);
    sc.raises(ra(ft, "setCCCId"), vec![none()]);
    sc.call(ra(ft, "setSrc"), vec![s("lut.cube")]);
    sc.call(ra(ft, "validate"), vec![]);
    for interp in [
        "INTERP_UNKNOWN",
        "INTERP_NEAREST",
        "INTERP_LINEAR",
        "INTERP_TETRAHEDRAL",
        "INTERP_CUBIC",
        "INTERP_BEST",
        "INTERP_DEFAULT",
    ] {
        sc.call(
            ra(ft, "setInterpolation"),
            vec![n(&format!("OCIO.{interp}"))],
        );
        sc.eval(r(ft), &all);
    }
    for style in ["CDL_ASC", "CDL_NO_CLAMP"] {
        sc.call(ra(ft, "setCDLStyle"), vec![n(&format!("OCIO.{style}"))]);
        sc.eval(r(ft), &all);
    }
    sc.raises(ra(ft, "setCDLStyle"), vec![i(0)]);
    odd_direction(&mut sc, ft, false);
    let f2 = sc.call(
        n("OCIO.FileTransform"),
        vec![
            s("lut.cube"),
            s("id"),
            n("OCIO.INTERP_TETRAHEDRAL"),
            n("OCIO.TRANSFORM_DIR_INVERSE"),
        ],
    );
    sc.eval(r(f2), &all);
    let f3 = sc.call_kw(
        n("OCIO.FileTransform"),
        vec![],
        vec![("cccId", s("x")), ("src", s("y.cc"))],
        Expect::Value,
    );
    sc.eval(r(f3), &all);
    sc.call_kw(
        n("OCIO.FileTransform"),
        vec![],
        vec![("cccId", s("x"))],
        Expect::Raised,
    );
    sc.raises(n("OCIO.FileTransform"), vec![none()]);
    // The formats.
    let fi = sc.call(n("OCIO.FileTransform.getFormats"), vec![]);
    sc.call(n("type"), vec![r(fi)]);
    sc.call(n("len"), vec![r(fi)]);
    for k in [0, 5, 23, -1] {
        sc.call(ra(fi, "__getitem__"), vec![i(k)]);
    }
    sc.raises(ra(fi, "__getitem__"), vec![i(1000)]);
    sc.call(n("list"), vec![r(fi)]);
    sc.raises(n("next"), vec![r(fi)]);
    for ext in ["cube", ".CUBE", "clf", ".", "", "nope", "ccc"] {
        sc.call(
            n("OCIO.FileTransform.IsFormatExtensionSupported"),
            vec![s(ext)],
        );
    }
    sc.call(
        n("OCIO.FileTransform.IsFormatExtensionSupported"),
        vec![none()],
    );
    sc.raises(
        n("OCIO.FileTransform.IsFormatExtensionSupported"),
        vec![b(true)],
    );
    sc
}

/// The classes behave as the wheel's.
#[test]
fn the_classes_behave_as_the_wheels() {
    compare(&[
        fixed_function_script(),
        builtin_script(),
        registry_script(),
        cdl_script(),
        file_script(),
    ]);
}

/// A style or an interpolation outside the enumerators is the port's error (U-112).
#[test]
fn a_style_outside_the_enumerators_is_the_ports_error() {
    let mut sc = Script::new();
    let mut refused = Vec::new();
    let ffs = sc.call(n("OCIO.FixedFunctionStyle"), vec![i(99)]);
    let ff = sc.call(
        n("OCIO.FixedFunctionTransform"),
        vec![n("OCIO.FIXED_FUNCTION_ACES_RED_MOD_03")],
    );
    refused.push(sc.raises(ra(ff, "setStyle"), vec![r(ffs)]));
    refused.push(sc.raises(n("OCIO.FixedFunctionTransform"), vec![r(ffs)]));
    let cs = sc.call(n("OCIO.CDLStyle"), vec![i(7)]);
    let c = sc.call(n("OCIO.CDLTransform"), vec![]);
    refused.push(sc.raises(ra(c, "setStyle"), vec![r(cs)]));
    let ft = sc.call(n("OCIO.FileTransform"), vec![]);
    refused.push(sc.raises(ra(ft, "setCDLStyle"), vec![r(cs)]));
    let interp = sc.call(n("OCIO.Interpolation"), vec![i(9)]);
    refused.push(sc.raises(ra(ft, "setInterpolation"), vec![r(interp)]));
    refused.push(sc.call_kw(
        n("OCIO.FileTransform"),
        vec![s("x.cube")],
        vec![("interpolation", r(interp))],
        Expect::Raised,
    ));
    let ours = common::port(&sc.json());
    let steps = ours["steps"].as_array().unwrap();
    assert_eq!(steps.len(), sc.steps.len());
    assert_eq!(refused.len(), 6);
    for (k, step) in steps.iter().enumerate() {
        if refused.contains(&k) {
            let message = String::from_utf8(ocio_testkit::oracle_values::bytes(
                &step["raised"]["message"],
            ))
            .unwrap();
            assert!(message.contains("U-112"), "step {k}: {step}");
        } else {
            assert!(step.get("value").is_some(), "step {k}: {step}");
        }
    }
}
