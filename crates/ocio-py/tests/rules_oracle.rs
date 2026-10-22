// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `FileRules` and `ViewingRules` with its iterators (chunk 6.3d) against the wheel:
//! - each class's surface, and the module's `DEFAULT_RULE_NAME` and
//!   `FILE_PATH_SEARCH_RULE_NAME`;
//! - through the same step scripts on both sides: every method, valid and refused (rule
//!   indices outside the rules, the default rule's limits, names twice, invalid globs and
//!   regular expressions, custom keys), the `size_t` casters (a negative index, `2**64 - 1`,
//!   `2**64`, a `bool`, a `float`), `None` for each `const char *` (the empty string upstream;
//!   `getIndexForRule`'s comparison error), copies and `repr`; the viewing rules' iterators,
//!   whose index is cast to `int` (`2**31` reads a null color space, `2**32` the first);
//!
//! and, without the wheel, whose process dies writing the null name into its message, the
//! port's error for `getIndexForRule(None)` on viewing rules without a rule (U-123).
#![cfg(feature = "python")]

mod common;

use common::script::{Expect, Script, b, by, compare, d, f, i, l, n, none, r, ra, s};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::json;

/// Each class has the wheel's surface, member for member; so do the constants.
#[test]
fn the_classes_and_constants_have_the_wheels_surface() {
    let report = common::wheel_surface();
    for c in ["FileRules", "ViewingRules"] {
        let theirs = common::wheel_member(&report, c);
        let ours = common::port_member(c);
        assert!(
            theirs["members"].as_array().is_some_and(|m| m.len() > 15),
            "{c}: {theirs}"
        );
        assert_eq!(ours, theirs, "{c}");
    }
    for c in ["DEFAULT_RULE_NAME", "FILE_PATH_SEARCH_RULE_NAME"] {
        let theirs = common::wheel_member(&report, c);
        assert_eq!(common::port_member(c), theirs, "{c}");
    }
}

/// A step whose outcome the test expects, by whether it raises.
fn step(sc: &mut Script, target: serde_json::Value, args: Vec<serde_json::Value>, raises: bool) {
    let expect = if raises {
        Expect::Raised
    } else {
        Expect::Value
    };
    sc.step(json!({ "call": target, "args": args }), expect);
}

/// The getters of every rule of `fr` (a step's result), each rule by index up to `count`.
fn file_rules_state(sc: &mut Script, fr: usize, count: i128) {
    sc.eval(r(fr), &["getNumEntries", "isDefault", "__repr__"]);
    for k in 0..count {
        for g in [
            "getName",
            "getPattern",
            "getExtension",
            "getRegex",
            "getColorSpace",
            "getNumCustomKeys",
        ] {
            sc.call(ra(fr, g), vec![i(k)]);
        }
    }
}

/// `FileRules`.
fn file_rules_script() -> Script {
    let mut sc = Script::new();
    let fr = sc.call(n("OCIO.FileRules"), vec![]);
    file_rules_state(&mut sc, fr, 1);
    sc.raises(n("OCIO.FileRules"), vec![i(1)]);
    // Inserting.
    sc.call(
        ra(fr, "insertRule"),
        vec![i(0), s("  glob rule "), s("cs1"), s("*abc*"), s("ex*")],
    );
    sc.call(
        ra(fr, "insertRule"),
        vec![i(1), s("regex"), s("cs2"), s(".*\\.dpx$")],
    );
    sc.call_kw(
        ra(fr, "insertRule"),
        vec![],
        vec![
            ("ruleIndex", i(0)),
            ("name", by(b"kw")),
            ("colorSpace", s("cs3")),
            ("regex", s("x")),
        ],
        Expect::Value,
    );
    sc.call(ra(fr, "insertPathSearchRule"), vec![i(3)]);
    file_rules_state(&mut sc, fr, 5);
    for (args, raises) in [
        (vec![i(0), s("GLOB RULE"), s("cs"), s("*"), s("*")], true),
        (vec![i(0), s("default"), s("cs"), s("*"), s("*")], true),
        (vec![i(0), s(""), s("cs"), s("*"), s("*")], true),
        (vec![i(0), none(), s("cs"), s("*"), s("*")], true),
        (vec![i(9), s("far"), s("cs"), s("*"), s("*")], true),
        (vec![i(0), s("nocs"), s(""), s("*"), s("*")], true),
        (vec![i(0), s("nopat"), s("cs"), none(), s("*")], true),
        (vec![i(0), s("noext"), s("cs"), s("*"), s("")], true),
        (vec![i(0), s("badglob"), s("cs"), s("[a"), s("*")], true),
        (vec![i(0), s("badregex"), s("cs"), s("(")], true),
        (vec![i(0), s("noregex"), s("cs"), none()], true),
        (
            vec![i(0), s("ColorSpaceNamePathSearch"), s(""), s("")],
            true,
        ),
        (vec![i(-1), s("neg"), s("cs"), s("x")], true),
        (vec![i(4294967296), s("big"), s("cs"), s("*"), s("*")], true),
        (
            vec![i(18446744073709551615), s("max"), s("cs"), s("x")],
            true,
        ),
        (vec![f(1.0), s("float"), s("cs"), s("x")], true),
        (vec![b(true), s("bool"), s("cs"), s("x")], false),
        (vec![i(0), s("x\0nul"), s("cs\0x"), s("y\0z")], false),
    ] {
        step(&mut sc, ra(fr, "insertRule"), args, raises);
    }
    file_rules_state(&mut sc, fr, 7);
    // Setters, the default rule's limits.
    for (setter, k, v, raises) in [
        ("setPattern", 2, s("*.exr"), false),
        ("setExtension", 2, s("EXR"), false),
        ("setRegex", 2, s("^a$"), false),
        ("setPattern", 2, s(""), true),
        ("setExtension", 2, none(), true),
        ("setRegex", 2, s("["), true),
        ("setRegex", 2, none(), true),
        ("setColorSpace", 2, s("lin"), false),
        ("setColorSpace", 2, none(), true),
        ("setPattern", 6, s("*"), true),
        ("setPattern", 6, none(), true),
        ("setColorSpace", 6, s("raw"), false),
        ("setColorSpace", 5, s("raw"), true),
        ("setColorSpace", 5, none(), false),
        ("setPattern", 5, s("*"), true),
        ("setPattern", 5, s(""), false),
        ("setRegex", 99, s("a"), true),
        ("setPattern", 18446744073709551615, s("a"), true),
    ] {
        step(&mut sc, ra(fr, setter), vec![i(k), v], raises);
    }
    step(
        &mut sc,
        ra(fr, "setPattern"),
        vec![i(18446744073709551616), s("a")],
        true,
    );
    file_rules_state(&mut sc, fr, 7);
    // Custom keys.
    sc.call(ra(fr, "setCustomKey"), vec![i(2), s("k2"), s("v2")]);
    sc.call(ra(fr, "setCustomKey"), vec![i(2), s("k1"), s("v1")]);
    sc.call(ra(fr, "setCustomKey"), vec![i(6), s("dk"), s("dv")]);
    sc.raises(ra(fr, "setCustomKey"), vec![i(2), s(""), s("v")]);
    sc.raises(ra(fr, "setCustomKey"), vec![i(2), none(), s("v")]);
    sc.raises(ra(fr, "setCustomKey"), vec![i(9), s("k"), s("v")]);
    for k in [0, 1] {
        sc.call(ra(fr, "getCustomKeyName"), vec![i(2), i(k)]);
        sc.call(ra(fr, "getCustomKeyValue"), vec![i(2), i(k)]);
    }
    sc.raises(ra(fr, "getCustomKeyName"), vec![i(2), i(2)]);
    sc.raises(ra(fr, "getCustomKeyValue"), vec![i(2), i(5)]);
    sc.raises(ra(fr, "getCustomKeyName"), vec![i(9), i(0)]);
    sc.call(ra(fr, "setCustomKey"), vec![i(2), s("k2"), none()]);
    sc.call(ra(fr, "setCustomKey"), vec![i(2), s("k1"), s("")]);
    sc.call(ra(fr, "getNumCustomKeys"), vec![i(2)]);
    sc.repr(r(fr));
    // Names.
    for name in [
        "glob rule",
        "GLOB RULE",
        "Default",
        "colorspacenamepathsearch",
        "nope",
        "",
    ] {
        step(
            &mut sc,
            ra(fr, "getIndexForRule"),
            vec![s(name)],
            ["nope", ""].contains(&name),
        );
    }
    sc.raises(ra(fr, "getIndexForRule"), vec![none()]);
    sc.raises(ra(fr, "getIndexForRule"), vec![by(b"\xff")]);
    // Priorities and removal.
    for (m, k, raises) in [
        ("increaseRulePriority", 2, false),
        ("increaseRulePriority", 0, true),
        ("decreaseRulePriority", 0, false),
        ("decreaseRulePriority", 5, true),
        ("increaseRulePriority", 6, true),
        ("decreaseRulePriority", 6, true),
        ("removeRule", 6, true),
        ("removeRule", 7, true),
        ("removeRule", 1, false),
    ] {
        step(&mut sc, ra(fr, m), vec![i(k)], raises);
    }
    file_rules_state(&mut sc, fr, 6);
    sc.call(ra(fr, "setDefaultRuleColorSpace"), vec![s("dcs")]);
    sc.raises(ra(fr, "setDefaultRuleColorSpace"), vec![none()]);
    sc.raises(ra(fr, "setDefaultRuleColorSpace"), vec![s("")]);
    // Copies.
    let copy = sc.call(n("copy.deepcopy"), vec![r(fr)]);
    sc.call(n("operator.is_"), vec![r(copy), r(fr)]);
    sc.call(ra(copy, "removeRule"), vec![i(0)]);
    sc.eval(r(copy), &["getNumEntries", "__repr__"]);
    sc.eval(r(fr), &["getNumEntries"]);
    sc.raises(ra(fr, "__deepcopy__"), vec![l(vec![])]);
    let copy2 = sc.call(ra(fr, "__deepcopy__"), vec![d(vec![])]);
    sc.call(n("type"), vec![r(copy2)]);
    sc.eval(n("OCIO.DEFAULT_RULE_NAME"), &[]);
    sc.eval(n("OCIO.FILE_PATH_SEARCH_RULE_NAME"), &[]);
    sc
}

/// `ViewingRules` and its iterators.
fn viewing_rules_script() -> Script {
    let mut sc = Script::new();
    let vr = sc.call(n("OCIO.ViewingRules"), vec![]);
    sc.eval(r(vr), &["getNumEntries", "__repr__"]);
    sc.raises(ra(vr, "getName"), vec![i(0)]);
    sc.raises(ra(vr, "getIndexForRule"), vec![s("x")]);
    for (args, raises) in [
        (vec![i(0), s(" first ")], false),
        (vec![i(1), s("second")], false),
        (vec![i(0), s("zeroth")], false),
        (vec![i(9), s("far")], true),
        (vec![i(0), s("FIRST")], true),
        (vec![i(0), s("")], true),
        (vec![i(0), none()], true),
        (vec![i(-1), s("neg")], true),
        (vec![i(3), s("x\0y")], false),
    ] {
        step(&mut sc, ra(vr, "insertRule"), args, raises);
    }
    sc.eval(r(vr), &["getNumEntries", "__repr__"]);
    for name in ["first", "SECOND", "x", "nope"] {
        step(
            &mut sc,
            ra(vr, "getIndexForRule"),
            vec![s(name)],
            name == "nope",
        );
    }
    sc.raises(ra(vr, "getIndexForRule"), vec![none()]);
    // Color spaces and encodings.
    for (m, args, raises) in [
        ("addColorSpace", vec![i(1), s("cs1")], false),
        ("addColorSpace", vec![i(1), s("cs2")], false),
        ("addColorSpace", vec![i(1), s("cs1")], false),
        ("addColorSpace", vec![i(1), s("")], true),
        ("addColorSpace", vec![i(1), none()], true),
        ("addColorSpace", vec![i(7), s("cs")], true),
        ("addEncoding", vec![i(1), s("enc")], true),
        ("addEncoding", vec![i(2), s("enc1")], false),
        ("addEncoding", vec![i(2), s("enc2")], false),
        ("addColorSpace", vec![i(2), s("cs")], true),
        ("addEncoding", vec![i(2), none()], true),
    ] {
        step(&mut sc, ra(vr, m), args, raises);
    }
    sc.repr(r(vr));
    let css = sc.call(ra(vr, "getColorSpaces"), vec![i(1)]);
    let encs = sc.call(ra(vr, "getEncodings"), vec![i(2)]);
    sc.call(n("type"), vec![r(css)]);
    sc.call(n("type"), vec![r(encs)]);
    sc.call(n("len"), vec![r(css)]);
    sc.call(n("len"), vec![r(encs)]);
    for (k, raises) in [
        (0, false),
        (1, false),
        (2, true),
        (2147483648, false),
        (4294967296, false),
        (4294967297, false),
        (18446744073709551615, false),
    ] {
        step(&mut sc, ra(css, "__getitem__"), vec![i(k)], raises);
        step(&mut sc, ra(encs, "__getitem__"), vec![i(k)], raises);
    }
    sc.raises(ra(css, "__getitem__"), vec![i(-1)]);
    sc.call(n("list"), vec![r(css)]);
    sc.raises(n("next"), vec![r(css)]);
    sc.call(n("next"), vec![r(encs)]);
    let it2 = sc.call(ra(encs, "__iter__"), vec![]);
    sc.call(n("operator.is_"), vec![r(it2), r(encs)]);
    // An iterator of a rule that isn't there: made, then its errors.
    let gone = sc.call(ra(vr, "getColorSpaces"), vec![i(42)]);
    sc.raises(n("len"), vec![r(gone)]);
    sc.raises(ra(gone, "__getitem__"), vec![i(0)]);
    sc.raises(n("next"), vec![r(gone)]);
    // Removing.
    for (m, args, raises) in [
        ("removeColorSpace", vec![i(1), i(0)], false),
        ("removeColorSpace", vec![i(1), i(5)], true),
        ("removeColorSpace", vec![i(1), i(4294967296)], false),
        ("removeEncoding", vec![i(2), i(1)], false),
        ("removeEncoding", vec![i(2), i(2147483648)], false),
        ("removeEncoding", vec![i(9), i(0)], true),
    ] {
        step(&mut sc, ra(vr, m), args, raises);
    }
    sc.call(n("len"), vec![r(css)]);
    sc.call(n("list"), vec![r(encs)]);
    sc.repr(r(vr));
    // Custom keys.
    sc.call(ra(vr, "setCustomKey"), vec![i(0), s("b"), s("2")]);
    sc.call(ra(vr, "setCustomKey"), vec![i(0), s("a"), s("1")]);
    sc.raises(ra(vr, "setCustomKey"), vec![i(0), none(), s("1")]);
    sc.raises(ra(vr, "setCustomKey"), vec![i(8), s("k"), s("1")]);
    sc.call(ra(vr, "getNumCustomKeys"), vec![i(0)]);
    for k in [0, 1] {
        sc.call(ra(vr, "getCustomKeyName"), vec![i(0), i(k)]);
        sc.call(ra(vr, "getCustomKeyValue"), vec![i(0), i(k)]);
    }
    sc.raises(ra(vr, "getCustomKeyName"), vec![i(0), i(2)]);
    sc.raises(ra(vr, "getCustomKeyValue"), vec![i(5), i(0)]);
    sc.call(ra(vr, "setCustomKey"), vec![i(0), s("a"), none()]);
    sc.repr(r(vr));
    sc.call(ra(vr, "removeRule"), vec![i(0)]);
    sc.raises(ra(vr, "removeRule"), vec![i(9)]);
    sc.eval(r(vr), &["getNumEntries", "__repr__"]);
    sc.call(ra(vr, "getName"), vec![i(0)]);
    // Copies.
    let copy = sc.call(n("copy.deepcopy"), vec![r(vr)]);
    sc.call(ra(copy, "removeRule"), vec![i(0)]);
    sc.eval(r(copy), &["getNumEntries", "__repr__"]);
    sc.eval(r(vr), &["getNumEntries"]);
    sc.raises(ra(vr, "__deepcopy__"), vec![none()]);
    // One rule: the null name compared with it.
    let one = sc.call(n("OCIO.ViewingRules"), vec![]);
    sc.call(ra(one, "insertRule"), vec![i(0), s("only")]);
    sc.raises(ra(one, "getIndexForRule"), vec![none()]);
    sc
}

#[test]
fn the_rules_behave_as_the_wheels() {
    compare(&[file_rules_script(), viewing_rules_script()]);
}

/// `getIndexForRule(None)` on viewing rules without a rule: the wheel writes the null name into
/// its message and its process dies; the port's error (U-123). With a rule, the comparison's
/// error, as the wheel's (`the_rules_behave_as_the_wheels`).
#[test]
fn a_null_rule_name_without_rules_is_the_ports_error() {
    common::start_python();
    Python::attach(|py| {
        let globals = PyDict::new(py);
        let run = |code: &str| {
            let code = std::ffi::CString::new(code).unwrap();
            py.run(&code, Some(&globals), None)
        };
        run("import PyOpenColorIO as OCIO\nvr = OCIO.ViewingRules()").unwrap();
        let err = run("vr.getIndexForRule(None)").expect_err("refused");
        let text = err.to_string();
        assert!(
            text.starts_with("ValueError") && text.contains("docs/improvements.md, U-123"),
            "{text}"
        );
        run("vr.insertRule(0, 'a')\nassert vr.getIndexForRule('A') == 0").unwrap();
    });
}
