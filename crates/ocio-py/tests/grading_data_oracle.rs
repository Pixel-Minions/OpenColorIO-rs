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

use common::script::{Expect, Script, b, compare, f, fb, i, l, n, none, r, ra, s, special, t};
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

/// What the accessors of the property `prop` (`OCIO.<class>.<name>`) are: their type,
/// `__module__`, `__name__`, `__qualname__`, `__self__`'s type and `__doc__` (a
/// `def_readwrite`'s are methods with the module's scope, a `def_property`'s plain functions
/// without one, a static property's getter a function without one).
fn accessors(sc: &mut Script, prop: &str) {
    sc.eval(n(&format!("{prop}.__doc__")), &[]);
    sc.call(n("type"), vec![n(prop)]);
    for a in ["fget", "fset", "fdel"] {
        let f = sc.eval(n(&format!("{prop}.{a}")), &[]);
        sc.call(n("type"), vec![r(f)]);
        for attr in [
            "__module__",
            "__name__",
            "__qualname__",
            "__doc__",
            "__self__",
        ] {
            sc.call(n("getattr"), vec![r(f), s(attr), none()]);
        }
    }
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
    accessors(&mut sc, "OCIO.GradingRGBM.red");
    accessors(&mut sc, "OCIO.GradingPrimary.gain");
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
    // The dictionary's repr holds what earlier scripts added to the class in this process
    // (`copy.copy` makes `copyreg` add `__slotnames__`), where the wheel runs each script in a
    // process of its own: the class's own members are py_surface's (the surface test).
    sc.unrepr(dict);
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

// ---------------------------------------------------------------------------------------------
// The curves

const CURVE_CLASSES: [&str; 3] = ["GradingBSplineCurve", "GradingRGBCurve", "GradingHueCurve"];

/// Each curve class (the B-spline curve with its nested iterator class) has the wheel's
/// surface, member for member: the default curves of `GradingRGBCurve`'s constructor in its
/// signature, the curve properties' nameless functions, `GradingHueCurve`'s two `__repr__`.
#[test]
fn the_curves_have_the_wheels_surface() {
    let report = common::wheel_surface();
    for c in CURVE_CLASSES {
        let theirs = common::wheel_member(&report, c);
        let ours = common::port_member(c);
        assert!(
            theirs["members"].as_array().is_some_and(|m| m.len() > 8),
            "{c}: {theirs}"
        );
        assert_eq!(ours, theirs, "{c}");
    }
}

/// `[x0, y0, x1, y1, ...]` as a list of floats.
fn points(values: &[f64]) -> Value {
    l(values.iter().map(|&v| f(v)).collect())
}

/// The control points of `curve` through its iterator: `len`, each item, the items past the
/// end, the iteration.
fn walk_points(sc: &mut Script, curve: usize) -> usize {
    let it = sc.call(ra(curve, "getControlPoints"), vec![]);
    sc.call(n("len"), vec![r(it)]);
    sc.call(n("list"), vec![r(it)]);
    sc.call(n("iter"), vec![r(it)]);
    sc.raises(n("next"), vec![r(it)]);
    sc.call(ra(curve, "getSlopes"), vec![]);
    sc.call(ra(curve, "slopesAreDefault"), vec![]);
    sc.call(ra(curve, "getSplineType"), vec![]);
    sc.repr(r(curve));
    it
}

fn bspline_script() -> Script {
    let mut sc = Script::new();
    // The constructors.
    let c3 = sc.call(n("OCIO.GradingBSplineCurve"), vec![i(3)]);
    walk_points(&mut sc, c3);
    let c0 = sc.call(n("OCIO.GradingBSplineCurve"), vec![i(0)]);
    walk_points(&mut sc, c0);
    sc.raises(ra(c0, "validate"), vec![]);
    for t in [
        "HUE_HUE", "HUE_SAT", "HUE_LUM", "LUM_SAT", "SAT_SAT", "LUM_LUM", "SAT_LUM", "HUE_FX",
    ] {
        let c = sc.call(
            n("OCIO.GradingBSplineCurve"),
            vec![i(2), n(&format!("OCIO.{t}"))],
        );
        sc.call(ra(c, "getSplineType"), vec![]);
        let v = sc.call(
            n("OCIO.GradingBSplineCurve"),
            vec![points(&[0.0, 0.5, 0.5, 0.75]), n(&format!("OCIO.{t}"))],
        );
        sc.call(ra(v, "getSplineType"), vec![]);
        sc.repr(r(v));
    }
    let odd = sc.call(n("OCIO.HueCurveType"), vec![i(9)]);
    let c = sc.call(n("OCIO.GradingBSplineCurve"), vec![i(2), r(odd)]);
    sc.call(ra(c, "getSplineType"), vec![]);
    let c = sc.call(
        n("OCIO.GradingBSplineCurve"),
        vec![points(&[0.0, 0.0, 1.0, 1.0]), r(odd)],
    );
    sc.call(ra(c, "getSplineType"), vec![]);
    for t in [
        "B_SPLINE",
        "DIAGONAL_B_SPLINE",
        "HUE_HUE_B_SPLINE",
        "PERIODIC_1_B_SPLINE",
        "PERIODIC_0_B_SPLINE",
        "HORIZONTAL1_B_SPLINE",
    ] {
        let c = sc.call(
            n("OCIO.GradingBSplineCurve"),
            vec![i(4), n(&format!("OCIO.{t}"))],
        );
        sc.call(ra(c, "getSplineType"), vec![]);
    }
    let kw = sc.call_kw(
        n("OCIO.GradingBSplineCurve"),
        vec![],
        vec![("splinetype", n("OCIO.DIAGONAL_B_SPLINE")), ("size", i(2))],
        Expect::Value,
    );
    sc.call(ra(kw, "getSplineType"), vec![]);
    let kw = sc.call_kw(
        n("OCIO.GradingBSplineCurve"),
        vec![i(2)],
        vec![("huecurvetype", n("OCIO.HUE_FX"))],
        Expect::Value,
    );
    sc.call(ra(kw, "getSplineType"), vec![]);
    // Values: floats, ints, a tuple, a float's rounding to float.
    let v = sc.call(
        n("OCIO.GradingBSplineCurve"),
        vec![l(vec![i(0), f(0.1), f(1.0 / 3.0), f(1e300), i(1), i(1)])],
    );
    walk_points(&mut sc, v);
    let v = sc.call(
        n("OCIO.GradingBSplineCurve"),
        vec![t(vec![f(0.0), f(0.0), f(1.0), f(2.0)])],
    );
    walk_points(&mut sc, v);
    for values in [vec![], vec![0.0, 1.0, 2.0], vec![0.0, 1.0, 2.0, 3.0, 4.0]] {
        sc.raises(n("OCIO.GradingBSplineCurve"), vec![points(&values)]);
        sc.raises(
            n("OCIO.GradingBSplineCurve"),
            vec![points(&values), n("OCIO.HUE_SAT")],
        );
    }
    for args in [
        vec![],
        vec![i(-1)],
        vec![f(3.0)],
        vec![s("3")],
        vec![none()],
        vec![l(vec![s("a"), i(1), i(2), i(3)])],
        vec![i(3), i(1)],
        vec![i(3), n("OCIO.GRADING_LOG")],
        vec![points(&[0.0, 0.0, 1.0, 1.0]), n("OCIO.B_SPLINE")],
        vec![i(1), i(2), i(3)],
    ] {
        sc.raises(n("OCIO.GradingBSplineCurve"), args);
    }
    sc.call_kw(
        n("OCIO.GradingBSplineCurve"),
        vec![],
        vec![("values", points(&[0.0, 0.0, 1.0, 1.0]))],
        Expect::Raised,
    );
    // Sizes the C++ runtime refuses: more than max_size() (ValueError), more than memory
    // holds (MemoryError).
    // 2**59 points: more than the address space (MemoryError on both); 2**60: libstdc++'s
    // max_size() + 1 (ValueError on Linux, MemoryError on Windows); 2**61 and 2**63: ValueError
    // on both. On Windows, MSVC's max_size() of the points (8 bytes) is 2**61 - 1, whose
    // bytes and alignment overflow (MemoryError "bad array new length"), as 2**61 - 5's do;
    // 2**61 - 6's don't ("bad allocation"); on Linux the three are over max_size()
    // (ValueError). (A size the allocator grants but memory can't hold, such as 2**40 on
    // Linux, depends on the machine: I-366.)
    let mut sizes = Vec::new();
    for k in [59, 60, 61, 63] {
        sizes.push(sc.call(n("pow"), vec![i(2), i(k)]));
    }
    for below in [1, 5, 6] {
        sizes.push(sc.call(n("operator.sub"), vec![r(sizes[2]), i(below)]));
    }
    for size in sizes {
        sc.raises(n("OCIO.GradingBSplineCurve"), vec![r(size)]);
        sc.raises(
            n("OCIO.GradingBSplineCurve"),
            vec![r(size), n("OCIO.HUE_SAT")],
        );
        sc.raises(
            n("OCIO.GradingBSplineCurve"),
            vec![r(size), n("OCIO.DIAGONAL_B_SPLINE")],
        );
        sc.raises(ra(c3, "setNumControlPoints"), vec![r(size)]);
    }
    sc.repr(r(c3));
    // The iterator: items are copies; set through it; indices out of range.
    let it = sc.call(ra(c3, "getControlPoints"), vec![]);
    let p = sc.call(n("OCIO.GradingControlPoint"), vec![f(0.25), f(0.5)]);
    sc.step(
        json!({ "setitem": r(it), "index": i(1), "value": r(p) }),
        Expect::Value,
    );
    sc.repr(r(c3));
    let item = sc.step(
        json!({ "call": ra(it, "__getitem__"), "args": [i(1)] }),
        Expect::Value,
    );
    let item2 = sc.call(ra(it, "__getitem__"), vec![i(1)]);
    sc.call(n("operator.is_"), vec![r(item), r(item2)]);
    setattr(&mut sc, r(item), "x", f(0.75), Expect::Value);
    sc.repr(r(c3));
    let it2 = sc.call(ra(c3, "getControlPoints"), vec![]);
    sc.call(n("operator.is_"), vec![r(it), r(it2)]);
    sc.call(ra(it, "__setitem__"), vec![i(2), r(p)]);
    for k in [i(3), i(-1), i(-4)] {
        sc.raises(ra(it, "__getitem__"), vec![k.clone()]);
        sc.raises(ra(it, "__setitem__"), vec![k, r(p)]);
    }
    let over = sc.call(n("pow"), vec![i(2), i(31)]);
    sc.raises(ra(it, "__getitem__"), vec![r(over)]);
    sc.raises(ra(it, "__getitem__"), vec![f(1.0)]);
    sc.raises(ra(it, "__setitem__"), vec![i(1), none()]);
    sc.raises(ra(it, "__setitem__"), vec![i(1), r(c3)]);
    let rgbm = sc.call(n("OCIO.GradingRGBM"), vec![]);
    sc.raises(ra(it, "__setitem__"), vec![i(1), r(rgbm)]);
    // An iterator partly walked, then the curve resized.
    let it3 = sc.call(ra(c3, "getControlPoints"), vec![]);
    sc.call(n("next"), vec![r(it3)]);
    sc.call(ra(c3, "setNumControlPoints"), vec![i(5)]);
    sc.call(n("list"), vec![r(it3)]);
    sc.call(n("len"), vec![r(it)]);
    sc.repr(r(c3));
    sc.call(ra(c3, "setNumControlPoints"), vec![i(2)]);
    sc.call(n("list"), vec![r(it)]);
    sc.raises(ra(c3, "setNumControlPoints"), vec![i(-1)]);
    sc.raises(ra(c3, "setNumControlPoints"), vec![]);
    // Slopes.
    sc.call(ra(c3, "setSlopes"), vec![l(vec![f(0.1), i(2)])]);
    walk_points(&mut sc, c3);
    sc.call(ra(c3, "setSlopes"), vec![l(vec![f(-0.0), f(0.0)])]);
    sc.call(ra(c3, "slopesAreDefault"), vec![]);
    sc.call(
        ra(c3, "setSlopes"),
        vec![l(vec![fb(0x7ff8_0000_0000_1234), f(1e300)])],
    );
    walk_points(&mut sc, c3);
    for slopes in [vec![], vec![f(1.0)], vec![f(1.0); 3]] {
        sc.raises(ra(c3, "setSlopes"), vec![l(slopes)]);
    }
    sc.raises(ra(c3, "setSlopes"), vec![none()]);
    sc.raises(ra(c3, "setSlopes"), vec![l(vec![s("a"), f(1.0)])]);
    // validate: too few points, x decreasing, the spline types' own checks.
    let bs = sc.call(
        n("OCIO.GradingBSplineCurve"),
        vec![points(&[0.0, 0.0, 0.1, 0.5, 0.4, 0.6, 0.6, 0.7, 1.0, 1.0])],
    );
    sc.call(ra(bs, "validate"), vec![]);
    let it = sc.call(ra(bs, "getControlPoints"), vec![]);
    let p = sc.call(n("OCIO.GradingControlPoint"), vec![f(0.5), f(1.0)]);
    sc.call(ra(it, "__setitem__"), vec![i(4), r(p)]);
    sc.raises(ra(bs, "validate"), vec![]);
    for t in [
        "DIAGONAL_B_SPLINE",
        "HUE_HUE_B_SPLINE",
        "PERIODIC_1_B_SPLINE",
        "PERIODIC_0_B_SPLINE",
        "HORIZONTAL1_B_SPLINE",
    ] {
        let c = sc.call(
            n("OCIO.GradingBSplineCurve"),
            vec![points(&[0.0, 0.5, 0.2, 0.2, 1.5, 0.7])],
        );
        sc.call(ra(c, "setSplineType"), vec![n(&format!("OCIO.{t}"))]);
        // The diagonal spline refuses y decreasing, the hue-hue one x past 1.
        let expect = if t == "DIAGONAL_B_SPLINE" || t == "HUE_HUE_B_SPLINE" {
            Expect::Raised
        } else {
            Expect::Value
        };
        sc.step(json!({ "call": ra(c, "validate"), "args": [] }), expect);
        sc.call(ra(c, "getSplineType"), vec![]);
    }
    let per = sc.call(
        n("OCIO.GradingBSplineCurve"),
        vec![points(&[0.0, 0.5, 1.0, 0.5]), n("OCIO.HUE_SAT")],
    );
    sc.raises(ra(per, "validate"), vec![]);
    sc.raises(ra(c3, "setSplineType"), vec![i(1)]);
    sc.raises(ra(c3, "setSplineType"), vec![none()]);
    // Comparisons.
    let same = sc.call(
        n("OCIO.GradingBSplineCurve"),
        vec![points(&[0.0, 0.0, 0.1, 0.5, 0.4, 0.6, 0.6, 0.7, 1.0, 1.0])],
    );
    let bs2 = sc.call(
        n("OCIO.GradingBSplineCurve"),
        vec![points(&[0.0, 0.0, 0.1, 0.5, 0.4, 0.6, 0.6, 0.7, 1.0, 1.0])],
    );
    let other = sc.call(
        n("OCIO.GradingBSplineCurve"),
        vec![
            points(&[0.0, 0.0, 0.1, 0.5, 0.4, 0.6, 0.6, 0.7, 1.0, 1.0]),
            n("OCIO.LUM_SAT"),
        ],
    );
    comparisons(&mut sc, bs2, same, other, r(rgbm));
    sc.call(ra(same, "setSlopes"), vec![l(vec![f(0.0); 5])]);
    sc.call(ra(bs2, "__eq__"), vec![r(same)]);
    sc.call(ra(same, "setSlopes"), vec![l(vec![f(0.5); 5])]);
    sc.call(ra(bs2, "__eq__"), vec![r(same)]);
    sc.call(ra(bs2, "__eq__"), vec![r(c3)]);
    let nan = sc.call(
        n("OCIO.GradingBSplineCurve"),
        vec![l(vec![fb(0x7ff8_0000_0000_0000), f(0.0), f(1.0), f(1.0)])],
    );
    sc.call(ra(nan, "__eq__"), vec![r(nan)]);
    sc
}

fn rgb_curve_script() -> Script {
    let mut sc = Script::new();
    let curves = ["red", "green", "blue", "master"];
    // No `__repr__`: the class has none (pybind11's default, with an address, which the
    // report masks in the value's repr but not in a returned `str`).
    let mut all: Vec<&str> = curves.to_vec();
    all.push("isIdentity");
    for style in ["GRADING_LOG", "GRADING_LIN", "GRADING_VIDEO"] {
        let c = sc.call(n("OCIO.GradingRGBCurve"), vec![n(&format!("OCIO.{style}"))]);
        sc.eval(r(c), &all);
        sc.call(ra(c, "validate"), vec![]);
    }
    let odd = odd_style(&mut sc);
    let c = sc.call(n("OCIO.GradingRGBCurve"), vec![r(odd)]);
    sc.eval(r(c), &all);
    let c = sc.call_kw(
        n("OCIO.GradingRGBCurve"),
        vec![],
        vec![("style", n("OCIO.GRADING_LIN"))],
        Expect::Value,
    );
    sc.eval(r(c), &all);
    // The curves' constructor: its defaults, keywords, copies of the curves given.
    let d = sc.call(n("OCIO.GradingRGBCurve"), vec![]);
    sc.eval(r(d), &all);
    let b1 = sc.call(
        n("OCIO.GradingBSplineCurve"),
        vec![points(&[0.0, 0.0, 0.1, 0.5, 0.4, 0.6, 0.6, 0.7, 1.0, 1.0])],
    );
    let b2 = sc.call(
        n("OCIO.GradingBSplineCurve"),
        vec![points(&[0.1, 0.5, 0.4, 0.6, 0.6, 0.7, 1.0, 1.1])],
    );
    let full = sc.call(n("OCIO.GradingRGBCurve"), vec![r(b1), r(b2), r(b1), r(b2)]);
    sc.eval(r(full), &all);
    sc.call(ra(full, "validate"), vec![]);
    let red = sc.eval(ra(full, "red"), &[]);
    sc.call(n("operator.is_"), vec![r(red), r(b1)]);
    let blue = sc.eval(ra(full, "blue"), &[]);
    sc.call(n("operator.is_"), vec![r(red), r(blue)]);
    sc.call(ra(red, "__eq__"), vec![r(b1)]);
    let kw = sc.call_kw(
        n("OCIO.GradingRGBCurve"),
        vec![],
        vec![("master", r(b2)), ("green", r(b1))],
        Expect::Value,
    );
    sc.eval(r(kw), &all);
    for args in [
        vec![none()],
        vec![r(b1), none()],
        vec![r(b1), r(b1), r(b1), none()],
    ] {
        sc.raises(n("OCIO.GradingRGBCurve"), args);
    }
    sc.call_kw(
        n("OCIO.GradingRGBCurve"),
        vec![],
        vec![("blue", none())],
        Expect::Raised,
    );
    for args in [
        vec![i(0)],
        vec![n("OCIO.TRANSFORM_DIR_FORWARD")],
        vec![r(b1); 5],
        vec![n("OCIO.GRADING_LOG"), r(b1)],
    ] {
        sc.raises(n("OCIO.GradingRGBCurve"), args);
    }
    // The properties: the very curves, the same object while it lives; set: a copy in.
    let log = sc.call(n("OCIO.GradingRGBCurve"), vec![n("OCIO.GRADING_LOG")]);
    let r1 = sc.eval(ra(log, "red"), &[]);
    let r2 = sc.eval(ra(log, "red"), &[]);
    let g1 = sc.eval(ra(log, "green"), &[]);
    let it = sc.call(ra(r1, "getControlPoints"), vec![]);
    let p = sc.call(n("OCIO.GradingControlPoint"), vec![f(0.4), f(0.5)]);
    sc.call(ra(it, "__setitem__"), vec![i(1), r(p)]);
    sc.eval(r(log), &["isIdentity"]);
    sc.call(ra(log, "validate"), vec![]);
    sc.call(ra(r2, "getSlopes"), vec![]);
    setattr(&mut sc, r(log), "green", r(b2), Expect::Value);
    sc.eval(r(g1), &["__repr__", "getSplineType"]);
    sc.eval(ra(log, "green"), &[]);
    sc.call(ra(b2, "setNumControlPoints"), vec![i(2)]);
    sc.eval(r(g1), &["__repr__"]);
    let hue = sc.call(
        n("OCIO.GradingBSplineCurve"),
        vec![points(&[0.0, 0.5, 0.5, 0.5, 0.9, 0.5]), n("OCIO.HUE_SAT")],
    );
    sc.call(ra(hue, "setSlopes"), vec![l(vec![f(0.25), f(0.5), f(0.0)])]);
    setattr(&mut sc, r(log), "blue", r(hue), Expect::Value);
    sc.eval(ra(log, "blue"), &["__repr__", "getSplineType", "getSlopes"]);
    sc.raises(ra(log, "validate"), vec![]);
    // A curve set to itself, and to another of the same object.
    setattr(&mut sc, r(log), "master", ra(log, "master"), Expect::Value);
    setattr(&mut sc, r(log), "master", ra(log, "blue"), Expect::Value);
    sc.eval(r(log), &all);
    for name in curves {
        setattr(&mut sc, r(log), name, i(1), Expect::Raised);
        let rgbm = sc.call(n("OCIO.GradingRGBM"), vec![]);
        setattr(&mut sc, r(log), name, r(rgbm), Expect::Raised);
        delattr(&mut sc, r(log), name);
        let prop = format!("OCIO.GradingRGBCurve.{name}");
        sc.call(n(&format!("{prop}.fget")), vec![r(log)]);
        sc.call(n(&format!("{prop}.fset")), vec![r(log), r(b1)]);
        sc.raises(n(&format!("{prop}.fget")), vec![r(b1)]);
        sc.raises(n(&format!("{prop}.fset")), vec![r(b1), r(b1)]);
        sc.raises(n(&format!("{prop}.fget")), vec![]);
    }
    sc.eval(r(log), &all);
    // Comparisons.
    let lin1 = sc.call(n("OCIO.GradingRGBCurve"), vec![n("OCIO.GRADING_LIN")]);
    let lin2 = sc.call(n("OCIO.GradingRGBCurve"), vec![n("OCIO.GRADING_LIN")]);
    let hc = sc.call(n("OCIO.GradingHueCurve"), vec![n("OCIO.GRADING_LIN")]);
    comparisons(&mut sc, lin1, lin2, log, r(hc));
    let it = sc.call(ra(lin1, "red.getControlPoints"), vec![]);
    sc.call(ra(it, "__setitem__"), vec![i(1), r(p)]);
    sc.call(ra(lin1, "__eq__"), vec![r(lin2)]);
    sc.eval(r(lin1), &["isIdentity"]);
    sc.raises(ra(lin1, "validate"), vec![i(1)]);
    accessors(&mut sc, "OCIO.GradingRGBCurve.red");
    accessors(&mut sc, "OCIO.GradingHueCurve.hue_fx");
    let dict = sc.eval(n("OCIO.GradingPrimary.__dict__"), &[]);
    // As in primary_script: the dictionary's repr depends on earlier scripts of this process.
    sc.unrepr(dict);
    let prop = sc.call(ra(dict, "__getitem__"), vec![s("NoClampWhite")]);
    sc.call(n("type"), vec![r(prop)]);
    sc.call(
        n("getattr"),
        vec![ra(prop, "fget"), s("__module__"), none()],
    );
    sc.call(
        n("getattr"),
        vec![ra(prop, "fget"), s("__qualname__"), none()],
    );
    sc.call(n("type"), vec![ra(prop, "fget")]);
    sc
}

fn hue_curve_script() -> Script {
    let mut sc = Script::new();
    let curves = [
        "hue_hue", "hue_sat", "hue_lum", "lum_sat", "sat_sat", "lum_lum", "sat_lum", "hue_fx",
    ];
    let mut all: Vec<&str> = curves.to_vec();
    all.extend(["isIdentity", "getDrawCurveOnly", "__repr__"]);
    for style in ["GRADING_LOG", "GRADING_LIN", "GRADING_VIDEO"] {
        let c = sc.call(n("OCIO.GradingHueCurve"), vec![n(&format!("OCIO.{style}"))]);
        sc.eval(r(c), &all);
        sc.call(ra(c, "validate"), vec![]);
    }
    let odd = odd_style(&mut sc);
    let c = sc.call(n("OCIO.GradingHueCurve"), vec![r(odd)]);
    sc.eval(r(c), &all);
    let c = sc.call_kw(
        n("OCIO.GradingHueCurve"),
        vec![],
        vec![("style", n("OCIO.GRADING_VIDEO"))],
        Expect::Value,
    );
    sc.eval(r(c), &all);
    for args in [vec![], vec![i(0)], vec![none()]] {
        sc.raises(n("OCIO.GradingHueCurve"), args);
    }
    // The curves' constructor: copies, validated.
    let mut parts = Vec::new();
    for t in [
        "HUE_HUE", "HUE_SAT", "HUE_LUM", "LUM_SAT", "SAT_SAT", "LUM_LUM", "SAT_LUM", "HUE_FX",
    ] {
        parts.push(sc.call(
            n("OCIO.GradingBSplineCurve"),
            vec![i(5), n(&format!("OCIO.{t}"))],
        ));
    }
    let ls = sc.call(
        n("OCIO.GradingBSplineCurve"),
        vec![points(&[0.1, 0.8, 0.9, 0.7]), n("OCIO.LUM_SAT")],
    );
    parts[3] = ls;
    let args: Vec<Value> = parts.iter().map(|&p| r(p)).collect();
    let full = sc.call(n("OCIO.GradingHueCurve"), args.clone());
    sc.eval(r(full), &all);
    sc.call(ra(full, "validate"), vec![]);
    let lum_sat = sc.eval(ra(full, "lum_sat"), &[]);
    sc.call(n("operator.is_"), vec![r(lum_sat), r(ls)]);
    sc.call(ra(lum_sat, "__eq__"), vec![r(ls)]);
    let mut wrong = args.clone();
    wrong.swap(0, 1);
    sc.raises(n("OCIO.GradingHueCurve"), wrong);
    let bad = sc.call(
        n("OCIO.GradingBSplineCurve"),
        vec![points(&[0.5, 0.0, 0.1, 1.0]), n("OCIO.LUM_SAT")],
    );
    let mut invalid = args.clone();
    invalid[3] = r(bad);
    sc.raises(n("OCIO.GradingHueCurve"), invalid);
    let mut short = args.clone();
    short.pop();
    sc.raises(n("OCIO.GradingHueCurve"), short);
    let mut typed = args.clone();
    typed[7] = i(1);
    sc.raises(n("OCIO.GradingHueCurve"), typed);
    sc.call_kw(
        n("OCIO.GradingHueCurve"),
        args[..7].to_vec(),
        vec![("arg7", args[7].clone())],
        Expect::Raised,
    );
    // The properties.
    let log = sc.call(n("OCIO.GradingHueCurve"), vec![n("OCIO.GRADING_LOG")]);
    let h1 = sc.eval(ra(log, "sat_lum"), &[]);
    let h2 = sc.eval(ra(log, "sat_lum"), &[]);
    sc.call(n("operator.is_"), vec![r(h1), r(h2)]);
    let it = sc.call(ra(h1, "getControlPoints"), vec![]);
    let p = sc.call(n("OCIO.GradingControlPoint"), vec![f(0.4), f(0.8)]);
    sc.call(ra(it, "__setitem__"), vec![i(1), r(p)]);
    sc.eval(r(log), &["isIdentity", "__repr__"]);
    sc.call(ra(log, "validate"), vec![]);
    // A curve of another spline type set in: the spline type and the slopes come along.
    setattr(&mut sc, r(log), "hue_sat", r(ls), Expect::Value);
    sc.eval(ra(log, "hue_sat"), &["getSplineType", "__repr__"]);
    sc.raises(ra(log, "validate"), vec![]);
    sc.call(ra(log, "setDrawCurveOnly"), vec![b(true)]);
    sc.call(ra(log, "validate"), vec![]);
    let slopes = sc.call(ra(ls, "getSlopes"), vec![]);
    sc.call(ra(ls, "setSlopes"), vec![l(vec![f(0.0), f(0.5)])]);
    setattr(&mut sc, r(log), "hue_sat", r(ls), Expect::Value);
    sc.eval(
        ra(log, "hue_sat"),
        &["getSlopes", "slopesAreDefault", "__repr__"],
    );
    sc.eval(r(slopes), &[]);
    for name in curves {
        setattr(&mut sc, r(log), name, s("x"), Expect::Raised);
        delattr(&mut sc, r(log), name);
        let prop = format!("OCIO.GradingHueCurve.{name}");
        sc.call(n(&format!("{prop}.fget")), vec![r(log)]);
        sc.raises(n(&format!("{prop}.fget")), vec![r(ls)]);
    }
    // The draw-curve-only flag: bool's conversions (None is false).
    for v in [b(false), i(2), none(), f(0.5)] {
        sc.call(ra(log, "setDrawCurveOnly"), vec![v]);
        sc.call(ra(log, "getDrawCurveOnly"), vec![]);
    }
    // A str has no nb_bool slot.
    sc.raises(ra(log, "setDrawCurveOnly"), vec![s("")]);
    sc.raises(ra(log, "setDrawCurveOnly"), vec![]);
    sc.call_kw(
        ra(log, "setDrawCurveOnly"),
        vec![],
        vec![("drawcurveonly", b(true))],
        Expect::Value,
    );
    sc.repr(r(log));
    // Comparisons, the flag included.
    let lin1 = sc.call(n("OCIO.GradingHueCurve"), vec![n("OCIO.GRADING_LIN")]);
    let lin2 = sc.call(n("OCIO.GradingHueCurve"), vec![n("OCIO.GRADING_LIN")]);
    let rgb = sc.call(n("OCIO.GradingRGBCurve"), vec![n("OCIO.GRADING_LIN")]);
    comparisons(&mut sc, lin1, lin2, log, r(rgb));
    sc.call(ra(lin1, "setDrawCurveOnly"), vec![b(true)]);
    sc.call(ra(lin1, "__eq__"), vec![r(lin2)]);
    sc.call(ra(lin1, "__ne__"), vec![r(lin2)]);
    sc.call(n("OCIO.GradingHueCurve.__repr__"), vec![r(lin1)]);
    sc.raises(n("OCIO.GradingHueCurve.__repr__"), vec![none()]);
    sc.raises(n("OCIO.GradingHueCurve.__repr__"), vec![r(rgb)]);
    sc
}

/// The curves behave as the wheel's.
#[test]
fn the_curves_behave_as_the_wheels() {
    compare(&[bspline_script(), rgb_curve_script(), hue_curve_script()]);
}

/// A spline type outside the enumerators, which the wheel's curve stores and the port's library
/// can't hold, is the port's error (U-112): the constructor's and `setSplineType`'s. The wheel
/// runs the same steps without an error, and its getter then gives the number back.
#[test]
fn a_spline_type_outside_the_enumerators_is_the_ports_error() {
    let mut sc = Script::new();
    let t = sc.call(n("OCIO.BSplineType"), vec![i(9)]);
    let c = sc.call(n("OCIO.GradingBSplineCurve"), vec![i(3)]);
    let refused = [
        sc.call(n("OCIO.GradingBSplineCurve"), vec![i(3), r(t)]),
        sc.call(ra(c, "setSplineType"), vec![r(t)]),
    ];
    let theirs = common::wheel(&[sc.json()]);
    let theirs = theirs[0]["steps"].as_array().unwrap();
    let ours = common::port(&sc.json());
    let ours = ours["steps"].as_array().unwrap();
    assert_eq!(theirs.len(), sc.steps.len());
    assert_eq!(ours.len(), sc.steps.len());
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
                "PyOpenColorIO.PyOpenColorIO.BSplineType(9): the wheel stores an enum value \
                 outside its enumerators, which the port's library can't hold \
                 (docs/improvements.md, U-112)"
            );
        } else {
            assert!(step.get("value").is_some(), "step {k}: {step}");
        }
    }
    // The wheel stored the number.
    let mut sc2 = Script::new();
    let t = sc2.call(n("OCIO.BSplineType"), vec![i(9)]);
    let c = sc2.call(n("OCIO.GradingBSplineCurve"), vec![i(3), r(t)]);
    sc2.call(ra(c, "getSplineType"), vec![]);
    let theirs = common::wheel(&[sc2.json()]);
    let repr = String::from_utf8(ocio_testkit::oracle_values::bytes(
        &theirs[0]["steps"][2]["value"]["repr"],
    ))
    .unwrap();
    assert_eq!(repr, "<BSplineType.???: 9>");
}

/// A case of [`a_null_curve_is_the_ports_error`]: its name, the steps it adds (the last's
/// index), the class its error names.
type Case = (&'static str, fn(&mut Script) -> usize, &'static str);

/// `None` for a curve where the wheel dereferences the null pointer (its process dies) is the
/// port's error (U-167): a curve property read or set on `None`, a property set to `None`, and
/// `None` among the hue curve's eight curves. Each script runs in a process of its own in the
/// wheel, which dies at its last step (and only there: the same script without that step
/// succeeds).
#[test]
fn a_null_curve_is_the_ports_error() {
    let cases: [Case; 6] = [
        (
            "GradingRGBCurve.red getter",
            |sc| sc.call(n("OCIO.GradingRGBCurve.red.fget"), vec![none()]),
            "PyOpenColorIO.PyOpenColorIO.GradingRGBCurve",
        ),
        (
            "GradingHueCurve.hue_fx getter",
            |sc| sc.call(n("OCIO.GradingHueCurve.hue_fx.fget"), vec![none()]),
            "PyOpenColorIO.PyOpenColorIO.GradingHueCurve",
        ),
        (
            "GradingRGBCurve.master setter of None",
            |sc| {
                let b = sc.call(n("OCIO.GradingBSplineCurve"), vec![i(3)]);
                sc.call(n("OCIO.GradingRGBCurve.master.fset"), vec![none(), r(b)])
            },
            "PyOpenColorIO.PyOpenColorIO.GradingRGBCurve",
        ),
        (
            "GradingRGBCurve.green = None",
            |sc| {
                let c = sc.call(n("OCIO.GradingRGBCurve"), vec![n("OCIO.GRADING_LOG")]);
                setattr(sc, r(c), "green", none(), Expect::Value)
            },
            "PyOpenColorIO.PyOpenColorIO.GradingBSplineCurve",
        ),
        (
            "GradingHueCurve.lum_sat = None",
            |sc| {
                let c = sc.call(n("OCIO.GradingHueCurve"), vec![n("OCIO.GRADING_LOG")]);
                setattr(sc, r(c), "lum_sat", none(), Expect::Value)
            },
            "PyOpenColorIO.PyOpenColorIO.GradingBSplineCurve",
        ),
        (
            "GradingHueCurve of a None",
            |sc| {
                let b = sc.call(n("OCIO.GradingBSplineCurve"), vec![i(3)]);
                let mut args = vec![r(b); 7];
                args.insert(2, none());
                sc.call(n("OCIO.GradingHueCurve"), args)
            },
            "PyOpenColorIO.PyOpenColorIO.GradingHueCurve",
        ),
    ];
    let mut scripts = Vec::new();
    let mut lasts = Vec::new();
    for (_, make, _) in &cases {
        let mut sc = Script::new();
        lasts.push(make(&mut sc));
        scripts.push(sc);
    }
    // The wheel: each process dies; without its last step, each succeeds.
    let json: Vec<Value> = scripts.iter().map(Script::json).collect();
    let result = ocio_testkit::Oracle::get()
        .call("py_eval", json!({ "scripts": json }), &[])
        .result;
    let reports = result["scripts"].as_array().unwrap();
    assert_eq!(reports.len(), cases.len());
    let mut shortened = Vec::new();
    for (k, report) in reports.iter().enumerate() {
        let code = report["crashed"]["returncode"].as_i64();
        assert!(code.is_some_and(|c| c != 0), "{}: {report}", cases[k].0);
        let mut steps = scripts[k].json();
        steps["steps"].as_array_mut().unwrap().pop();
        shortened.push(steps);
    }
    let short = common::wheel(&shortened);
    for (k, report) in short.iter().enumerate() {
        for step in report["steps"].as_array().unwrap() {
            assert!(step.get("value").is_some(), "{}: {step}", cases[k].0);
        }
    }
    // The port: its error, at the last step only.
    for (k, sc) in scripts.iter().enumerate() {
        let ours = common::port(&sc.json());
        let steps = ours["steps"].as_array().unwrap();
        assert_eq!(steps.len(), lasts[k] + 1);
        for step in &steps[..lasts[k]] {
            assert!(step.get("value").is_some(), "{}: {step}", cases[k].0);
        }
        let last = &steps[lasts[k]];
        assert_eq!(
            last["raised"]["type"], "builtins.ValueError",
            "{}",
            cases[k].0
        );
        let message = String::from_utf8(ocio_testkit::oracle_values::bytes(
            &last["raised"]["message"],
        ))
        .unwrap();
        assert_eq!(
            message,
            format!(
                "{}: None for a curve, a null pointer the wheel dereferences (its process \
                 dies; docs/improvements.md, U-167)",
                cases[k].2
            ),
            "{}",
            cases[k].0
        );
    }
}
