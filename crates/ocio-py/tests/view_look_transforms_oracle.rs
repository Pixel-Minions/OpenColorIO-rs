// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The transform classes of chunk 6.3g against the wheel (`ColorSpaceTransform`,
//! `DisplayViewTransform`, `LookTransform`): each class's surface (py_surface_walk.py's
//! `_class` against the wheel's py_surface), and their behaviour through the same step scripts
//! on both sides: both constructors (positional, keyword, every default, refused arguments, the
//! validation errors of the second), every getter and setter (text with a NUL, non-ASCII text,
//! the empty string, `None` where the setter takes a null pointer and where it refuses it,
//! booleans and what the `bool` caster refuses), `validate` and its errors, `repr`, the base
//! class's methods, and a direction outside the enumerators.
#![cfg(feature = "python")]

mod common;

use common::script::{Expect, Script, b, base_methods, by, compare, f, i, l, n, none, r, ra, s};

const CLASSES: [&str; 3] = [
    "ColorSpaceTransform",
    "DisplayViewTransform",
    "LookTransform",
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

/// Text values whose bytes a getter must give back (or cut at a NUL).
const TEXTS: [&str; 4] = ["lin", "a\u{0}b", "caf\u{e9} \u{1f600}", ""];

/// A direction outside the enumerators, set, read and validated (the class keeps the value).
fn odd_direction(sc: &mut Script, obj: usize, validates: bool) {
    let bad = sc.call(n("OCIO.TransformDirection"), vec![i(5)]);
    sc.call(ra(obj, "setDirection"), vec![r(bad)]);
    sc.eval(r(obj), &["getDirection", "__repr__"]);
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

/// The `bool` caster on a setter: `True`, `False`, then what it refuses without conversions
/// that it takes with them, and what it refuses.
fn bools(sc: &mut Script, obj: usize, setter: &str, getter: &str) {
    for v in [b(false), b(true)] {
        sc.call(ra(obj, setter), vec![v]);
        sc.call(ra(obj, getter), vec![]);
    }
    // With conversions, the caster takes what has `__bool__`, and None as False.
    for v in [i(0), i(2), f(1.0), f(-0.0), none(), b(true)] {
        sc.call(ra(obj, setter), vec![v]);
        sc.call(ra(obj, getter), vec![]);
    }
    sc.raises(ra(obj, setter), vec![l(vec![])]);
    sc.raises(ra(obj, setter), vec![s("True")]);
    sc.raises(ra(obj, setter), vec![]);
    sc.call(ra(obj, getter), vec![]);
}

/// A text setter on every [`TEXTS`] value, then its refusals: `None` (`refuses_none`, else a
/// null pointer, stored as the empty string), bytes, an int.
fn texts(sc: &mut Script, obj: usize, setter: &str, all: &[&str], refuses_none: bool) {
    for text in TEXTS {
        sc.call(ra(obj, setter), vec![s(text)]);
        sc.eval(r(obj), all);
    }
    sc.call(ra(obj, setter), vec![s("x")]);
    if refuses_none {
        sc.raises(ra(obj, setter), vec![none()]);
    } else {
        sc.call(ra(obj, setter), vec![none()]);
    }
    sc.eval(r(obj), all);
    sc.call(ra(obj, setter), vec![by(b"by\xfftes")]);
    sc.eval(r(obj), all);
    sc.raises(ra(obj, setter), vec![i(3)]);
}

fn color_space_script() -> Script {
    let mut sc = Script::new();
    let all = [
        "getSrc",
        "getDst",
        "getDataBypass",
        "getDirection",
        "__repr__",
    ];
    let t = sc.call(n("OCIO.ColorSpaceTransform"), vec![]);
    sc.eval(r(t), &all);
    base_methods(&mut sc, t, false);
    texts(&mut sc, t, "setSrc", &all, true);
    texts(&mut sc, t, "setDst", &all, true);
    sc.call(ra(t, "setDst"), vec![s("")]);
    sc.call(ra(t, "setSrc"), vec![s("a")]);
    sc.raises(ra(t, "validate"), vec![]);
    sc.call(ra(t, "setDst"), vec![s("b")]);
    sc.call(ra(t, "validate"), vec![]);
    bools(&mut sc, t, "setDataBypass", "getDataBypass");
    odd_direction(&mut sc, t, false);
    // The second constructor, positional, keyword, its defaults and its validation.
    let t2 = sc.call(
        n("OCIO.ColorSpaceTransform"),
        vec![
            s("src"),
            s("dst"),
            n("OCIO.TRANSFORM_DIR_INVERSE"),
            b(false),
        ],
    );
    sc.eval(r(t2), &all);
    let t3 = sc.call_kw(
        n("OCIO.ColorSpaceTransform"),
        vec![],
        vec![("dst", s("d\u{0}x")), ("src", s("s"))],
        Expect::Value,
    );
    sc.eval(r(t3), &all);
    for kwargs in [
        vec![("src", s("s"))],
        vec![("dst", s("d"))],
        vec![("src", s("")), ("dst", s("d"))],
        vec![("src", s("\u{0}")), ("dst", s("d"))],
        vec![("src", s("s")), ("dst", s("d")), ("direction", i(0))],
        vec![("src", s("s")), ("dst", s("d")), ("bogus", i(1))],
        vec![("src", none()), ("dst", s("d"))],
    ] {
        sc.call_kw(
            n("OCIO.ColorSpaceTransform"),
            vec![],
            kwargs,
            Expect::Raised,
        );
    }
    let bad = sc.call(n("OCIO.TransformDirection"), vec![i(7)]);
    sc.raises(n("OCIO.ColorSpaceTransform"), vec![s("s"), s("d"), r(bad)]);
    sc.raises(n("OCIO.ColorSpaceTransform"), vec![none()]);
    sc
}

fn display_view_script() -> Script {
    let mut sc = Script::new();
    let all = [
        "getSrc",
        "getDisplay",
        "getView",
        "getLooksBypass",
        "getDataBypass",
        "getDirection",
        "__repr__",
    ];
    let t = sc.call(n("OCIO.DisplayViewTransform"), vec![]);
    sc.eval(r(t), &all);
    base_methods(&mut sc, t, false);
    texts(&mut sc, t, "setSrc", &all, false);
    texts(&mut sc, t, "setDisplay", &all, false);
    texts(&mut sc, t, "setView", &all, false);
    sc.call(ra(t, "setDisplay"), vec![s("")]);
    sc.call(ra(t, "setView"), vec![s("")]);
    sc.call(ra(t, "setSrc"), vec![s("a")]);
    sc.raises(ra(t, "validate"), vec![]);
    sc.call(ra(t, "setDisplay"), vec![s("b")]);
    sc.raises(ra(t, "validate"), vec![]);
    sc.call(ra(t, "setView"), vec![s("c")]);
    sc.call(ra(t, "validate"), vec![]);
    bools(&mut sc, t, "setLooksBypass", "getLooksBypass");
    bools(&mut sc, t, "setDataBypass", "getDataBypass");
    odd_direction(&mut sc, t, false);
    let t2 = sc.call(
        n("OCIO.DisplayViewTransform"),
        vec![
            s("src"),
            s("disp"),
            s("view"),
            b(true),
            b(false),
            n("OCIO.TRANSFORM_DIR_INVERSE"),
        ],
    );
    sc.eval(r(t2), &all);
    let t3 = sc.call_kw(
        n("OCIO.DisplayViewTransform"),
        vec![],
        vec![("view", s("v")), ("display", s("d")), ("src", s("s"))],
        Expect::Value,
    );
    sc.eval(r(t3), &all);
    for kwargs in [
        vec![("src", s("s"))],
        vec![("src", s("s")), ("display", s("d"))],
        vec![("display", s("d")), ("view", s("v"))],
        vec![("src", s("s")), ("display", s("d")), ("view", none())],
        vec![
            ("src", s("s")),
            ("display", s("d")),
            ("view", s("v")),
            ("looksBypass", s("no")),
        ],
    ] {
        sc.call_kw(
            n("OCIO.DisplayViewTransform"),
            vec![],
            kwargs,
            Expect::Raised,
        );
    }
    sc
}

fn look_script() -> Script {
    let mut sc = Script::new();
    let all = [
        "getSrc",
        "getDst",
        "getLooks",
        "getSkipColorSpaceConversion",
        "getDirection",
        "__repr__",
    ];
    let t = sc.call(n("OCIO.LookTransform"), vec![]);
    sc.eval(r(t), &all);
    base_methods(&mut sc, t, false);
    texts(&mut sc, t, "setSrc", &all, true);
    texts(&mut sc, t, "setDst", &all, true);
    texts(&mut sc, t, "setLooks", &all, true);
    sc.call(ra(t, "setDst"), vec![s("")]);
    sc.call(ra(t, "setSrc"), vec![s("a")]);
    sc.raises(ra(t, "validate"), vec![]);
    sc.call(ra(t, "setDst"), vec![s("b")]);
    sc.call(ra(t, "validate"), vec![]);
    sc.call(ra(t, "setLooks"), vec![s("+look1, -look2")]);
    sc.call(ra(t, "validate"), vec![]);
    bools(
        &mut sc,
        t,
        "setSkipColorSpaceConversion",
        "getSkipColorSpaceConversion",
    );
    odd_direction(&mut sc, t, false);
    let t2 = sc.call(
        n("OCIO.LookTransform"),
        vec![
            s("src"),
            s("dst"),
            s("look"),
            b(true),
            n("OCIO.TRANSFORM_DIR_INVERSE"),
        ],
    );
    sc.eval(r(t2), &all);
    let t3 = sc.call_kw(
        n("OCIO.LookTransform"),
        vec![],
        vec![("dst", s("d")), ("src", s("s"))],
        Expect::Value,
    );
    sc.eval(r(t3), &all);
    let t4 = sc.call(n("OCIO.LookTransform"), vec![s("s"), s("d")]);
    sc.eval(r(t4), &all);
    for kwargs in [
        vec![("src", s("s"))],
        vec![("src", s("")), ("dst", s("d"))],
        vec![("src", s("s")), ("dst", s(""))],
        vec![("src", none()), ("dst", s("d"))],
        vec![("src", s("s")), ("dst", s("d")), ("looks", none())],
        vec![
            ("src", s("s")),
            ("dst", s("d")),
            ("skipColorSpaceConversion", s("no")),
        ],
    ] {
        sc.call_kw(n("OCIO.LookTransform"), vec![], kwargs, Expect::Raised);
    }
    sc.raises(n("OCIO.LookTransform"), vec![s("s")]);
    sc
}

/// The classes behave as the wheel's.
#[test]
fn the_classes_behave_as_the_wheels() {
    compare(&[color_space_script(), display_view_script(), look_script()]);
}
