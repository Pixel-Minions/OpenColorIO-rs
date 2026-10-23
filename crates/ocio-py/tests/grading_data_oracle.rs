// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The grading values of chunk 6.7a against the wheel (`GradingRGBM`, `GradingPrimary`,
//! `GradingRGBMSW`, `GradingTone`, `GradingControlPoint`): each class's surface (its
//! `def_readwrite` properties, whose getters and setters are nameless methods, and
//! `GradingPrimary`'s static properties), and their behaviour through the same step scripts on
//! both sides: every constructor (positional, keyword, refused), every member read and written
//! with values whose bits differ (a `float` member rounds them), the members of `GradingRGBM` and
//! `GradingRGBMSW` type, which are references into their owner (a change through one shows in
//! the owner, and the same object comes back while it lives), `==` and `!=` (NaNs, other types:
//! `NotImplemented`), `hash`, `validate` and `repr`, and a `GradingStyle` outside the
//! enumerators.
#![cfg(feature = "python")]

mod common;

use common::script::{Expect, Script, b, compare, f, fb, i, l, n, none, r, ra, s, special};
use serde_json::{Value, json};

const CLASSES: [&str; 5] = [
    "GradingRGBM",
    "GradingPrimary",
    "GradingRGBMSW",
    "GradingTone",
    "GradingControlPoint",
];

/// Each class has the wheel's surface, member for member.
#[test]
fn the_values_have_the_wheels_surface() {
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

/// `setattr(target, name, value)`.
fn setattr(sc: &mut Script, target: Value, name: &str, value: Value, expect: Expect) -> usize {
    sc.step(
        json!({ "setattr": target, "name": name, "value": value }),
        expect,
    )
}

/// `delattr(target, name)`, which raises.
fn delattr(sc: &mut Script, target: Value, name: &str) -> usize {
    sc.step(json!({ "delattr": target, "name": name }), Expect::Raised)
}

/// `OCIO.GradingStyle(7)`, outside the enumerators.
fn odd_style(sc: &mut Script) -> usize {
    sc.call(n("OCIO.GradingStyle"), vec![i(7)])
}

/// The comparisons of `x` with `same` (an equal value), `other` (a different one), and values of
/// other types.
fn comparisons(sc: &mut Script, x: usize, same: usize, other: usize, foreign: Value) {
    for (op, expect) in [("__eq__", Expect::Value), ("__ne__", Expect::Value)] {
        for arg in [r(x), r(same), r(other), none(), i(5), foreign.clone()] {
            sc.step(json!({ "call": ra(x, op), "args": [arg] }), expect);
        }
    }
    for arg in [r(x), r(same), r(other), none(), i(5), foreign] {
        sc.call(n("operator.eq"), vec![r(x), arg.clone()]);
        sc.call(n("operator.ne"), vec![r(x), arg]);
    }
    sc.raises(n("hash"), vec![r(x)]);
    sc.raises(n("copy.copy"), vec![r(x)]);
    sc.raises(n("copy.deepcopy"), vec![r(x)]);
}

/// Each number member `names` of `obj`: read, written with special values (read back), with an
/// `int` and a `bool`; refused for a `str`, `None` and no value; not deletable; and through its
/// property's getter and setter called directly.
fn number_members(sc: &mut Script, class: &str, obj: usize, names: &[&str], foreign: Value) {
    for (k, name) in names.iter().enumerate() {
        sc.eval(ra(obj, name), &[]);
        for v in [special(k), special(k + 2), special(k + 5), i(3), b(true)] {
            setattr(sc, r(obj), name, v, Expect::Value);
            sc.eval(ra(obj, name), &[]);
        }
        let np = sc.call(n("numpy.float32"), vec![f(0.5)]);
        setattr(sc, r(obj), name, r(np), Expect::Value);
        sc.eval(ra(obj, name), &[]);
        setattr(sc, r(obj), name, s("1"), Expect::Raised);
        setattr(sc, r(obj), name, none(), Expect::Raised);
        delattr(sc, r(obj), name);
        let prop = format!("OCIO.{class}.{name}");
        sc.call(n(&format!("{prop}.fget")), vec![r(obj)]);
        sc.call(n(&format!("{prop}.fset")), vec![r(obj), f(0.25)]);
        sc.eval(ra(obj, name), &[]);
        sc.raises(n(&format!("{prop}.fget")), vec![none()]);
        sc.raises(n(&format!("{prop}.fget")), vec![foreign.clone()]);
        sc.raises(n(&format!("{prop}.fset")), vec![r(obj)]);
        sc.raises(n(&format!("{prop}.fset")), vec![r(obj), f(1.0), f(2.0)]);
    }
    sc.repr(r(obj));
}

fn rgbm_script() -> Script {
    let mut sc = Script::new();
    let all = ["red", "green", "blue", "master", "__repr__"];
    let x = sc.call(n("OCIO.GradingRGBM"), vec![]);
    sc.eval(r(x), &all);
    let y = sc.call(
        n("OCIO.GradingRGBM"),
        vec![f(1.0), i(2), b(true), fb(0x7ff8_0000_0000_1234)],
    );
    sc.eval(r(y), &all);
    let z = sc.call_kw(
        n("OCIO.GradingRGBM"),
        vec![],
        vec![
            ("blue", f(1.0)),
            ("green", f(2.0)),
            ("master", f(3.0)),
            ("red", f(4.0)),
        ],
        Expect::Value,
    );
    sc.eval(r(z), &all);
    sc.call_kw(
        n("OCIO.GradingRGBM"),
        vec![],
        vec![("master", f(0.3)), ("red", f(0.4))],
        Expect::Raised,
    );
    for args in [
        vec![i(0)],
        vec![i(0), i(0)],
        vec![i(0), i(0), i(0)],
        vec![s("1"), i(0), i(0), i(0)],
        vec![none(), i(0), i(0), i(0)],
        vec![i(0); 5],
    ] {
        sc.raises(n("OCIO.GradingRGBM"), args);
    }
    sc.call(
        n("OCIO.GradingRGBM.__init__"),
        vec![r(x), f(9.0), f(9.0), f(9.0), f(9.0)],
    );
    sc.eval(r(x), &all);
    let same = sc.call(n("OCIO.GradingRGBM"), vec![]);
    let msw = sc.call(n("OCIO.GradingRGBMSW"), vec![]);
    comparisons(&mut sc, x, same, z, r(msw));
    let nan = sc.call(n("OCIO.GradingRGBM"), vec![fb(0x7ff8_0000_0000_0001); 4]);
    sc.call(ra(nan, "__eq__"), vec![r(nan)]);
    sc.call(ra(nan, "__ne__"), vec![r(nan)]);
    number_members(
        &mut sc,
        "GradingRGBM",
        x,
        &["red", "green", "blue", "master"],
        r(msw),
    );
    sc.raises(n("OCIO.GradingRGBM.__repr__"), vec![none()]);
    sc.raises(n("OCIO.GradingRGBM.__repr__"), vec![r(msw)]);
    // A subclass of Python's.
    sc
}

fn primary_script() -> Script {
    let mut sc = Script::new();
    let members = [
        "brightness",
        "contrast",
        "gamma",
        "offset",
        "exposure",
        "lift",
        "gain",
        "pivot",
        "saturation",
        "clampWhite",
        "clampBlack",
        "pivotWhite",
        "pivotBlack",
        "NoClampBlack",
        "NoClampWhite",
        "__repr__",
    ];
    let log = sc.call(n("OCIO.GradingPrimary"), vec![n("OCIO.GRADING_LOG")]);
    sc.eval(r(log), &members);
    let lin = sc.call(n("OCIO.GradingPrimary"), vec![n("OCIO.GRADING_LIN")]);
    sc.eval(r(lin), &members);
    let video = sc.call(n("OCIO.GradingPrimary"), vec![n("OCIO.GRADING_VIDEO")]);
    sc.eval(r(video), &members);
    let odd = odd_style(&mut sc);
    let o = sc.call(n("OCIO.GradingPrimary"), vec![r(odd)]);
    sc.eval(r(o), &members);
    for args in [
        vec![],
        vec![i(0)],
        vec![none()],
        vec![n("OCIO.TRANSFORM_DIR_FORWARD")],
        vec![n("OCIO.GRADING_LOG"), n("OCIO.GRADING_LOG")],
    ] {
        sc.raises(n("OCIO.GradingPrimary"), args);
    }
    sc.call_kw(
        n("OCIO.GradingPrimary"),
        vec![],
        vec![("arg0", n("OCIO.GRADING_LOG"))],
        Expect::Raised,
    );
    // validate, for each style: the defaults pass; a gamma, a contrast, pivots and clamps out
    // of their ranges fail where the style checks them.
    for style in ["OCIO.GRADING_LOG", "OCIO.GRADING_LIN", "OCIO.GRADING_VIDEO"] {
        sc.call(ra(log, "validate"), vec![n(style)]);
    }
    sc.call(ra(log, "validate"), vec![r(odd)]);
    sc.raises(ra(log, "validate"), vec![]);
    sc.raises(ra(log, "validate"), vec![i(0)]);
    let bad = sc.call(n("OCIO.GradingPrimary"), vec![n("OCIO.GRADING_LOG")]);
    let gamma = sc.call(n("OCIO.GradingRGBM"), vec![f(1.0), f(0.0), f(1.0), f(1.0)]);
    setattr(&mut sc, r(bad), "gamma", r(gamma), Expect::Value);
    sc.raises(ra(bad, "validate"), vec![n("OCIO.GRADING_LOG")]);
    sc.call(ra(bad, "validate"), vec![n("OCIO.GRADING_LIN")]);
    sc.raises(ra(bad, "validate"), vec![n("OCIO.GRADING_VIDEO")]);
    sc.raises(ra(bad, "validate"), vec![r(odd)]);
    setattr(&mut sc, r(bad), "contrast", r(gamma), Expect::Value);
    sc.raises(ra(bad, "validate"), vec![n("OCIO.GRADING_LIN")]);
    let bad2 = sc.call(n("OCIO.GradingPrimary"), vec![n("OCIO.GRADING_LIN")]);
    setattr(&mut sc, r(bad2), "pivotWhite", f(0.0), Expect::Value);
    sc.raises(ra(bad2, "validate"), vec![n("OCIO.GRADING_LIN")]);
    setattr(&mut sc, r(bad2), "pivotWhite", f(1.0), Expect::Value);
    setattr(&mut sc, r(bad2), "clampBlack", f(2.0), Expect::Value);
    setattr(&mut sc, r(bad2), "clampWhite", f(1.0), Expect::Value);
    sc.raises(ra(bad2, "validate"), vec![n("OCIO.GRADING_LOG")]);
    sc.repr(r(bad2));
    // The members of GradingRGBM type: references into the primary.
    let bright = sc.eval(ra(log, "brightness"), &["red", "__repr__"]);
    let bright2 = sc.eval(ra(log, "brightness"), &[]);
    let contrast = sc.eval(ra(log, "contrast"), &[]);
    setattr(&mut sc, r(bright), "red", f(5.0), Expect::Value);
    sc.eval(ra(log, "brightness.red"), &[]);
    sc.repr(r(log));
    let rgbm = sc.call(n("OCIO.GradingRGBM"), vec![f(1.0), f(2.0), f(3.0), f(4.0)]);
    setattr(&mut sc, r(log), "brightness", r(rgbm), Expect::Value);
    sc.eval(r(bright), &["red", "green", "blue", "master"]);
    sc.eval(r(bright2), &["__repr__"]);
    setattr(&mut sc, r(rgbm), "red", f(7.0), Expect::Value);
    sc.eval(ra(log, "brightness.red"), &[]);
    // One member assigned another: a copy.
    setattr(&mut sc, r(lin), "gamma", r(contrast), Expect::Value);
    sc.repr(r(lin));
    sc.eval(ra(lin, "gamma"), &[]);
    setattr(&mut sc, r(contrast), "blue", f(-1.0), Expect::Value);
    sc.eval(ra(lin, "gamma.blue"), &[]);
    sc.eval(ra(log, "contrast.blue"), &[]);
    setattr(&mut sc, r(log), "gain", r(contrast), Expect::Value);
    sc.eval(ra(log, "gain"), &["__repr__"]);
    // A view assigned to its own member.
    setattr(&mut sc, r(log), "contrast", r(contrast), Expect::Value);
    sc.eval(ra(log, "contrast"), &["__repr__"]);
    for name in ["brightness", "gain"] {
        setattr(&mut sc, r(log), name, none(), Expect::Raised);
        setattr(&mut sc, r(log), name, f(1.0), Expect::Raised);
        let msw = sc.call(n("OCIO.GradingRGBMSW"), vec![]);
        setattr(&mut sc, r(log), name, r(msw), Expect::Raised);
        delattr(&mut sc, r(log), name);
        let prop = format!("OCIO.GradingPrimary.{name}");
        sc.call(n(&format!("{prop}.fget")), vec![r(log)]);
        sc.raises(n(&format!("{prop}.fget")), vec![none()]);
        sc.raises(n(&format!("{prop}.fset")), vec![r(log), none()]);
    }
    let tone = sc.call(n("OCIO.GradingTone"), vec![n("OCIO.GRADING_LOG")]);
    number_members(
        &mut sc,
        "GradingPrimary",
        video,
        &[
            "pivot",
            "saturation",
            "clampWhite",
            "clampBlack",
            "pivotWhite",
            "pivotBlack",
        ],
        r(tone),
    );
    // The static properties: through the class and an instance, read-only.
    sc.eval(n("OCIO.GradingPrimary.NoClampBlack"), &[]);
    sc.eval(n("OCIO.GradingPrimary.NoClampWhite"), &[]);
    sc.eval(ra(video, "NoClampWhite"), &[]);
    setattr(
        &mut sc,
        n("OCIO.GradingPrimary"),
        "NoClampBlack",
        f(3.0),
        Expect::Raised,
    );
    setattr(&mut sc, r(video), "NoClampBlack", f(3.0), Expect::Raised);
    let dict = sc.eval(n("OCIO.GradingPrimary.__dict__"), &[]);
    let prop = sc.call(ra(dict, "__getitem__"), vec![s("NoClampBlack")]);
    sc.call(ra(prop, "fget"), vec![i(5)]);
    sc.call(ra(prop, "fget"), vec![none()]);
    sc.raises(ra(prop, "fget"), vec![]);
    // Comparisons: equal values, a NaN, other types.
    let same = sc.call(n("OCIO.GradingPrimary"), vec![n("OCIO.GRADING_LIN")]);
    let lin2 = sc.call(n("OCIO.GradingPrimary"), vec![n("OCIO.GRADING_LIN")]);
    comparisons(&mut sc, lin2, same, log, r(tone));
    setattr(
        &mut sc,
        r(lin2),
        "pivot",
        fb(0x7ff8_0000_0000_0002),
        Expect::Value,
    );
    sc.call(ra(lin2, "__eq__"), vec![r(lin2)]);
    sc.call(ra(lin2, "__ne__"), vec![r(lin2)]);
    sc.raises(n("OCIO.GradingPrimary.__repr__"), vec![none()]);
    sc
}

fn tone_script() -> Script {
    let mut sc = Script::new();
    let members = [
        "blacks",
        "whites",
        "shadows",
        "highlights",
        "midtones",
        "scontrast",
        "__repr__",
    ];
    let rgbmsw = [
        "red", "green", "blue", "master", "start", "width", "__repr__",
    ];
    let x = sc.call(n("OCIO.GradingRGBMSW"), vec![]);
    sc.eval(r(x), &rgbmsw);
    let y = sc.call(
        n("OCIO.GradingRGBMSW"),
        vec![
            f(1.0),
            i(2),
            f(3.0),
            f(4.0),
            fb(0xfff0_0000_0000_0000),
            f(-0.0),
        ],
    );
    sc.eval(r(y), &rgbmsw);
    let z = sc.call(n("OCIO.GradingRGBMSW"), vec![f(2.0), i(3)]);
    sc.eval(r(z), &rgbmsw);
    let kw = sc.call_kw(
        n("OCIO.GradingRGBMSW"),
        vec![],
        vec![("width", f(3.0)), ("start", f(4.0))],
        Expect::Value,
    );
    sc.eval(r(kw), &rgbmsw);
    let kw6 = sc.call_kw(
        n("OCIO.GradingRGBMSW"),
        vec![],
        vec![
            ("blue", f(1.0)),
            ("master", f(2.0)),
            ("green", f(3.0)),
            ("start", f(4.0)),
            ("width", f(5.0)),
            ("red", f(6.0)),
        ],
        Expect::Value,
    );
    sc.eval(r(kw6), &rgbmsw);
    sc.call_kw(
        n("OCIO.GradingRGBMSW"),
        vec![],
        vec![("green", f(3.0)), ("start", f(4.0))],
        Expect::Raised,
    );
    sc.call_kw(
        n("OCIO.GradingRGBMSW"),
        vec![f(1.0)],
        vec![("width", f(4.0))],
        Expect::Value,
    );
    for args in [
        vec![i(0)],
        vec![i(0); 3],
        vec![i(0); 5],
        vec![i(0); 7],
        vec![s("a"), i(0)],
    ] {
        sc.raises(n("OCIO.GradingRGBMSW"), args);
    }
    let same = sc.call(n("OCIO.GradingRGBMSW"), vec![]);
    let rgbm = sc.call(n("OCIO.GradingRGBM"), vec![]);
    comparisons(&mut sc, x, same, y, r(rgbm));
    number_members(
        &mut sc,
        "GradingRGBMSW",
        x,
        &["red", "green", "blue", "master", "start", "width"],
        r(rgbm),
    );
    // GradingTone.
    let log = sc.call(n("OCIO.GradingTone"), vec![n("OCIO.GRADING_LOG")]);
    sc.eval(r(log), &members);
    let lin = sc.call(n("OCIO.GradingTone"), vec![n("OCIO.GRADING_LIN")]);
    sc.eval(r(lin), &members);
    let video = sc.call(n("OCIO.GradingTone"), vec![n("OCIO.GRADING_VIDEO")]);
    sc.eval(r(video), &members);
    let odd = odd_style(&mut sc);
    let o = sc.call(n("OCIO.GradingTone"), vec![r(odd)]);
    sc.eval(r(o), &members);
    for args in [
        vec![],
        vec![i(0)],
        vec![none()],
        vec![n("OCIO.TRANSFORM_DIR_FORWARD")],
    ] {
        sc.raises(n("OCIO.GradingTone"), args);
    }
    for t in [log, lin, video, o] {
        sc.call(ra(t, "validate"), vec![]);
    }
    sc.raises(ra(log, "validate"), vec![n("OCIO.GRADING_LOG")]);
    // Out of range: each member in turn.
    for (name, value) in [
        ("blacks", 0.05),
        ("whites", 1.95),
        ("shadows", 0.1),
        ("highlights", 1.9),
        ("midtones", 2.0),
    ] {
        let t = sc.call(n("OCIO.GradingTone"), vec![n("OCIO.GRADING_LOG")]);
        let m = sc.eval(ra(t, name), &[]);
        setattr(&mut sc, r(m), "green", f(value), Expect::Value);
        sc.raises(ra(t, "validate"), vec![]);
        setattr(&mut sc, r(m), "green", f(1.0), Expect::Value);
        setattr(&mut sc, r(m), "width", f(0.0), Expect::Value);
        let expect = if name != "shadows" {
            Expect::Raised
        } else {
            Expect::Value
        };
        sc.step(json!({ "call": ra(t, "validate"), "args": [] }), expect);
    }
    let t = sc.call(n("OCIO.GradingTone"), vec![n("OCIO.GRADING_LIN")]);
    setattr(&mut sc, r(t), "scontrast", f(2.5), Expect::Value);
    sc.raises(ra(t, "validate"), vec![]);
    // The members of GradingRGBMSW type: references into the tone.
    let blacks = sc.eval(ra(log, "blacks"), &["start", "__repr__"]);
    let blacks2 = sc.eval(ra(log, "blacks"), &[]);
    let whites = sc.eval(ra(log, "whites"), &[]);
    setattr(&mut sc, r(blacks), "start", f(0.1), Expect::Value);
    sc.eval(ra(log, "blacks.start"), &[]);
    sc.repr(r(log));
    setattr(&mut sc, r(log), "blacks", r(kw6), Expect::Value);
    sc.eval(r(blacks2), &rgbmsw);
    setattr(&mut sc, r(lin), "midtones", r(whites), Expect::Value);
    sc.repr(r(lin));
    setattr(&mut sc, r(whites), "red", f(-2.0), Expect::Value);
    sc.eval(ra(lin, "midtones.red"), &[]);
    sc.eval(ra(log, "whites.red"), &[]);
    for name in ["blacks", "midtones"] {
        setattr(&mut sc, r(log), name, none(), Expect::Raised);
        setattr(&mut sc, r(log), name, r(rgbm), Expect::Raised);
        delattr(&mut sc, r(log), name);
        sc.raises(n(&format!("OCIO.GradingTone.{name}.fget")), vec![r(rgbm)]);
    }
    let same_tone = sc.call(n("OCIO.GradingTone"), vec![n("OCIO.GRADING_VIDEO")]);
    let primary = sc.call(n("OCIO.GradingPrimary"), vec![n("OCIO.GRADING_LOG")]);
    comparisons(&mut sc, video, same_tone, log, r(primary));
    number_members(&mut sc, "GradingTone", video, &["scontrast"], r(primary));
    sc.raises(n("OCIO.GradingTone.__repr__"), vec![none()]);
    sc
}

fn control_point_script() -> Script {
    let mut sc = Script::new();
    let all = ["x", "y", "__repr__"];
    let p = sc.call(n("OCIO.GradingControlPoint"), vec![]);
    sc.eval(r(p), &all);
    // A float member: the double rounded to float (a NaN's payload, an overflow).
    for (x, y) in [
        (f(0.1), f(1.0 / 3.0)),
        (f(1e300), f(-1e300)),
        (fb(0x7ff8_0000_0000_1234), fb(0x7ff0_0000_0000_0001)),
        (fb(0xfff8_0000_2000_0000), f(-0.0)),
        (f(1e-46), f(3.4028235e38)),
    ] {
        let q = sc.call(n("OCIO.GradingControlPoint"), vec![x, y]);
        sc.eval(r(q), &all);
    }
    let big = sc.call(n("pow"), vec![i(2), i(200)]);
    let q = sc.call(n("OCIO.GradingControlPoint"), vec![f(0.5), r(big)]);
    sc.eval(r(q), &all);
    for kw in [
        vec![("y", f(2.0))],
        vec![("x", f(2.0))],
        vec![("y", f(0.6)), ("x", f(0.4))],
    ] {
        let q = sc.call_kw(n("OCIO.GradingControlPoint"), vec![], kw, Expect::Value);
        sc.eval(r(q), &all);
    }
    let q = sc.call(n("OCIO.GradingControlPoint"), vec![i(1)]);
    sc.eval(r(q), &all);
    for args in [vec![s("1")], vec![none()], vec![i(0); 3], vec![l(vec![])]] {
        sc.raises(n("OCIO.GradingControlPoint"), args);
    }
    sc.call_kw(
        n("OCIO.GradingControlPoint"),
        vec![],
        vec![("z", f(1.0))],
        Expect::Raised,
    );
    let same = sc.call(n("OCIO.GradingControlPoint"), vec![]);
    let other = sc.call(n("OCIO.GradingControlPoint"), vec![f(1.0), f(1.0)]);
    let rgbm = sc.call(n("OCIO.GradingRGBM"), vec![]);
    comparisons(&mut sc, p, same, other, r(rgbm));
    let nan = sc.call(
        n("OCIO.GradingControlPoint"),
        vec![fb(0x7ff8_0000_0000_0000), f(0.0)],
    );
    sc.call(ra(nan, "__eq__"), vec![r(nan)]);
    number_members(&mut sc, "GradingControlPoint", p, &["x", "y"], r(rgbm));
    sc.raises(n("OCIO.GradingControlPoint.__repr__"), vec![none()]);
    sc
}

/// The values behave as the wheel's.
#[test]
fn the_values_behave_as_the_wheels() {
    compare(&[
        rgbm_script(),
        primary_script(),
        tone_script(),
        control_point_script(),
    ]);
}
