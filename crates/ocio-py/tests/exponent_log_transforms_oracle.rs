// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The transform classes of chunk 6.3b against the wheel (`ExponentTransform`,
//! `ExponentWithLinearTransform`, `LogTransform`, `LogAffineTransform`, `LogCameraTransform`):
//! each class's surface (py_surface_walk.py's `_class` against the wheel's py_surface), and
//! their behaviour through the same step scripts on both sides: the base class's methods, each
//! constructor (positional, keyword, refused), every getter and setter with values whose bits
//! differ, the negative styles each class takes or refuses, `equals`, `validate`, `repr`, a
//! direction outside the enumerators (which flips the exponent classes, I-261), and the camera
//! log's linear slope (unset: three quiet NaNs).
#![cfg(feature = "python")]

mod common;

use common::script::{
    Expect, Script, base_methods, compare, f, fl, i, l, n, none, r, ra, s, special, t,
};

const CLASSES: [&str; 5] = [
    "ExponentTransform",
    "ExponentWithLinearTransform",
    "LogTransform",
    "LogAffineTransform",
    "LogCameraTransform",
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

const STYLES: [&str; 4] = [
    "NEGATIVE_CLAMP",
    "NEGATIVE_MIRROR",
    "NEGATIVE_PASS_THRU",
    "NEGATIVE_LINEAR",
];

/// The negative styles on `obj`, and the ones `refused` it refuses.
fn negative_styles(sc: &mut Script, obj: usize, refused: &[&str]) {
    for st in STYLES {
        let expect = if refused.contains(&st) {
            Expect::Raised
        } else {
            Expect::Value
        };
        sc.step(
            serde_json::json!({ "call": ra(obj, "setNegativeStyle"), "args": [n(&format!("OCIO.{st}"))] }),
            expect,
        );
        sc.eval(r(obj), &["getNegativeStyle", "__repr__"]);
    }
    sc.raises(ra(obj, "setNegativeStyle"), vec![i(0)]);
    sc.raises(ra(obj, "setNegativeStyle"), vec![none()]);
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

fn exponent_script() -> Script {
    let mut sc = Script::new();
    let e = sc.call(n("OCIO.ExponentTransform"), vec![]);
    let all = ["getValue", "getNegativeStyle", "getDirection", "__repr__"];
    sc.eval(r(e), &all);
    base_methods(&mut sc, e, true);
    sc.call(ra(e, "setValue"), vec![l((0..4).map(special).collect())]);
    sc.eval(r(e), &all);
    sc.raises(ra(e, "validate"), vec![]);
    sc.call(
        ra(e, "setValue"),
        vec![t(vec![f(2.2), i(2), f(1.8), f(0.5)])],
    );
    sc.call(ra(e, "validate"), vec![]);
    sc.raises(ra(e, "setValue"), vec![fl(&[1.0, 2.0, 3.0])]);
    sc.raises(ra(e, "setValue"), vec![none()]);
    negative_styles(&mut sc, e, &["NEGATIVE_LINEAR"]);
    odd_direction(&mut sc, e, true);
    let e2 = sc.call(
        n("OCIO.ExponentTransform"),
        vec![
            fl(&[2.2, 2.4, 2.6, 1.0]),
            n("OCIO.NEGATIVE_MIRROR"),
            n("OCIO.TRANSFORM_DIR_INVERSE"),
        ],
    );
    sc.eval(r(e2), &all);
    let e3 = sc.call_kw(
        n("OCIO.ExponentTransform"),
        vec![],
        vec![
            ("negativeStyle", n("OCIO.NEGATIVE_PASS_THRU")),
            ("value", fl(&[1.5; 4])),
        ],
        Expect::Value,
    );
    sc.eval(r(e3), &all);
    sc.call_kw(
        n("OCIO.ExponentTransform"),
        vec![],
        vec![("negativeStyle", n("OCIO.NEGATIVE_LINEAR"))],
        Expect::Raised,
    );
    sc.call_kw(
        n("OCIO.ExponentTransform"),
        vec![],
        vec![("value", fl(&[0.0; 4]))],
        Expect::Raised,
    );
    sc.raises(n("OCIO.ExponentTransform"), vec![f(1.0)]);
    sc.call(ra(e2, "equals"), vec![r(e2)]);
    sc.call(ra(e2, "equals"), vec![r(e3)]);
    let e4 = sc.call(n("copy.deepcopy"), vec![r(e2)]);
    sc.call(ra(e2, "equals"), vec![r(e4)]);
    sc.raises(ra(e2, "equals"), vec![none()]);
    sc
}

fn exponent_with_linear_script() -> Script {
    let mut sc = Script::new();
    let e = sc.call(n("OCIO.ExponentWithLinearTransform"), vec![]);
    let all = [
        "getGamma",
        "getOffset",
        "getNegativeStyle",
        "getDirection",
        "__repr__",
    ];
    sc.eval(r(e), &all);
    base_methods(&mut sc, e, true);
    sc.call(ra(e, "setGamma"), vec![l((0..4).map(special).collect())]);
    sc.call(ra(e, "setOffset"), vec![l((4..8).map(special).collect())]);
    sc.eval(r(e), &all);
    sc.raises(ra(e, "validate"), vec![]);
    sc.call(ra(e, "setGamma"), vec![fl(&[2.4, 2.4, 2.4, 1.0])]);
    sc.call(ra(e, "setOffset"), vec![fl(&[0.055, 0.055, 0.055, 0.0])]);
    sc.call(ra(e, "validate"), vec![]);
    sc.raises(ra(e, "setOffset"), vec![fl(&[0.1; 5])]);
    negative_styles(&mut sc, e, &["NEGATIVE_CLAMP", "NEGATIVE_PASS_THRU"]);
    odd_direction(&mut sc, e, true);
    let e2 = sc.call(
        n("OCIO.ExponentWithLinearTransform"),
        vec![
            fl(&[2.2, 2.4, 2.6, 1.0]),
            fl(&[0.1, 0.2, 0.3, 0.0]),
            n("OCIO.NEGATIVE_MIRROR"),
            n("OCIO.TRANSFORM_DIR_INVERSE"),
        ],
    );
    sc.eval(r(e2), &all);
    sc.call_kw(
        n("OCIO.ExponentWithLinearTransform"),
        vec![],
        vec![("negativeStyle", n("OCIO.NEGATIVE_CLAMP"))],
        Expect::Raised,
    );
    sc.call_kw(
        n("OCIO.ExponentWithLinearTransform"),
        vec![],
        vec![("offset", fl(&[-0.5; 4]))],
        Expect::Raised,
    );
    sc.call(ra(e2, "equals"), vec![r(e2)]);
    sc.call(ra(e2, "equals"), vec![r(e)]);
    sc.raises(ra(e2, "equals"), vec![r(e2), r(e2)]);
    sc
}

fn log_script() -> Script {
    let mut sc = Script::new();
    let lg = sc.call(n("OCIO.LogTransform"), vec![]);
    let all = ["getBase", "getDirection", "__repr__"];
    sc.eval(r(lg), &all);
    base_methods(&mut sc, lg, true);
    for k in 0..8 {
        sc.call(ra(lg, "setBase"), vec![special(k)]);
        sc.eval(r(lg), &all);
    }
    for base in [1.0, 0.0, -2.0, 10.0] {
        sc.call(ra(lg, "setBase"), vec![f(base)]);
        let expect = if base > 0.0 && base != 1.0 {
            Expect::Value
        } else {
            Expect::Raised
        };
        sc.step(
            serde_json::json!({ "call": ra(lg, "validate"), "args": [] }),
            expect,
        );
    }
    sc.raises(ra(lg, "setBase"), vec![s("2")]);
    odd_direction(&mut sc, lg, false);
    let l2 = sc.call(
        n("OCIO.LogTransform"),
        vec![i(10), n("OCIO.TRANSFORM_DIR_INVERSE")],
    );
    sc.eval(r(l2), &all);
    sc.call_kw(
        n("OCIO.LogTransform"),
        vec![],
        vec![("base", f(1.0))],
        Expect::Raised,
    );
    sc.call(ra(l2, "equals"), vec![r(l2)]);
    sc.call(ra(l2, "equals"), vec![r(lg)]);
    sc
}

fn log_affine_script() -> Script {
    let mut sc = Script::new();
    let la = sc.call(n("OCIO.LogAffineTransform"), vec![]);
    let all = [
        "getBase",
        "getLogSideSlopeValue",
        "getLogSideOffsetValue",
        "getLinSideSlopeValue",
        "getLinSideOffsetValue",
        "getDirection",
        "__repr__",
    ];
    sc.eval(r(la), &all);
    base_methods(&mut sc, la, true);
    for (k, v) in [
        "LogSideSlope",
        "LogSideOffset",
        "LinSideSlope",
        "LinSideOffset",
    ]
    .iter()
    .enumerate()
    {
        sc.call(
            ra(la, &format!("set{v}Value")),
            vec![l((k..k + 3).map(special).collect())],
        );
        sc.eval(r(la), &all);
        sc.raises(ra(la, &format!("set{v}Value")), vec![fl(&[1.0; 4])]);
    }
    sc.raises(ra(la, "validate"), vec![]);
    for v in ["LogSideSlope", "LinSideSlope"] {
        sc.call(ra(la, &format!("set{v}Value")), vec![fl(&[0.5, 1.5, 2.5])]);
    }
    for v in ["LogSideOffset", "LinSideOffset"] {
        sc.call(ra(la, &format!("set{v}Value")), vec![fl(&[0.1, -0.2, 0.3])]);
    }
    sc.call(ra(la, "setBase"), vec![f(10.0)]);
    sc.call(ra(la, "validate"), vec![]);
    sc.eval(r(la), &all);
    odd_direction(&mut sc, la, false);
    let l2 = sc.call(
        n("OCIO.LogAffineTransform"),
        vec![
            fl(&[0.5; 3]),
            fl(&[0.1; 3]),
            fl(&[2.0; 3]),
            fl(&[0.2; 3]),
            n("OCIO.TRANSFORM_DIR_INVERSE"),
        ],
    );
    sc.eval(r(l2), &all);
    let l3 = sc.call_kw(
        n("OCIO.LogAffineTransform"),
        vec![],
        vec![
            ("linSideOffset", fl(&[0.5; 3])),
            ("logSideSlope", fl(&[2.0; 3])),
        ],
        Expect::Value,
    );
    sc.eval(r(l3), &all);
    sc.call_kw(
        n("OCIO.LogAffineTransform"),
        vec![],
        vec![("logSideSlope", fl(&[0.0; 3]))],
        Expect::Raised,
    );
    sc.call(ra(l2, "equals"), vec![r(l2)]);
    sc.call(ra(l2, "equals"), vec![r(l3)]);
    sc
}

fn log_camera_script() -> Script {
    let mut sc = Script::new();
    let lc = sc.call(n("OCIO.LogCameraTransform"), vec![fl(&[0.1, 0.2, 0.3])]);
    let all = [
        "getBase",
        "getLogSideSlopeValue",
        "getLogSideOffsetValue",
        "getLinSideSlopeValue",
        "getLinSideOffsetValue",
        "getLinSideBreakValue",
        "getLinearSlopeValue",
        "isLinearSlopeValueSet",
        "getDirection",
        "__repr__",
    ];
    sc.eval(r(lc), &all);
    base_methods(&mut sc, lc, true);
    for (k, v) in [
        "LogSideSlope",
        "LogSideOffset",
        "LinSideSlope",
        "LinSideOffset",
        "LinSideBreak",
        "LinearSlope",
    ]
    .iter()
    .enumerate()
    {
        sc.call(
            ra(lc, &format!("set{v}Value")),
            vec![l((k..k + 3).map(special).collect())],
        );
        sc.eval(r(lc), &all);
    }
    sc.call(ra(lc, "unsetLinearSlopeValue"), vec![]);
    sc.eval(r(lc), &all);
    sc.raises(ra(lc, "setLinearSlopeValue"), vec![fl(&[1.0; 2])]);
    odd_direction(&mut sc, lc, false);
    // The constructor: the break required, the linear slope of 0 or 3 values, no validation.
    sc.raises(n("OCIO.LogCameraTransform"), vec![]);
    sc.raises(n("OCIO.LogCameraTransform"), vec![none()]);
    let l2 = sc.call_kw(
        n("OCIO.LogCameraTransform"),
        vec![fl(&[0.1; 3])],
        vec![("linearSlope", fl(&[1.0, 2.0, 3.0])), ("base", f(10.0))],
        Expect::Value,
    );
    sc.eval(r(l2), &all);
    sc.call_kw(
        n("OCIO.LogCameraTransform"),
        vec![fl(&[0.1; 3])],
        vec![("linearSlope", fl(&[1.0, 2.0]))],
        Expect::Raised,
    );
    let unvalidated = sc.call_kw(
        n("OCIO.LogCameraTransform"),
        vec![fl(&[0.1; 3])],
        vec![("base", f(1.0))],
        Expect::Value,
    );
    sc.raises(ra(unvalidated, "validate"), vec![]);
    let l3 = sc.call(
        n("OCIO.LogCameraTransform"),
        vec![
            fl(&[0.1; 3]),
            f(2.0),
            fl(&[0.5; 3]),
            fl(&[0.1; 3]),
            fl(&[2.0; 3]),
            fl(&[0.2; 3]),
            l(vec![]),
            n("OCIO.TRANSFORM_DIR_INVERSE"),
        ],
    );
    sc.eval(r(l3), &all);
    sc.call(ra(l3, "equals"), vec![r(l3)]);
    sc.call(ra(l3, "equals"), vec![r(l2)]);
    sc.eval(
        n("OCIO.LogCameraTransform.getLinearSlopeValue.__doc__"),
        &[],
    );
    sc
}

/// The classes behave as the wheel's.
#[test]
fn the_classes_behave_as_the_wheels() {
    compare(&[
        exponent_script(),
        exponent_with_linear_script(),
        log_script(),
        log_affine_script(),
        log_camera_script(),
    ]);
}

/// A negative style outside the enumerators is the port's error (U-112).
#[test]
fn a_negative_style_outside_the_enumerators_is_the_ports_error() {
    let mut sc = Script::new();
    let style = sc.call(n("OCIO.NegativeStyle"), vec![i(9)]);
    let mut refused = Vec::new();
    for c in ["OCIO.ExponentTransform", "OCIO.ExponentWithLinearTransform"] {
        let obj = sc.call(n(c), vec![]);
        refused.push(sc.raises(ra(obj, "setNegativeStyle"), vec![r(style)]));
        refused.push(sc.call_kw(
            n(c),
            vec![],
            vec![("negativeStyle", r(style))],
            Expect::Raised,
        ));
    }
    let ours = common::port(&sc.json());
    let steps = ours["steps"].as_array().unwrap();
    assert_eq!(steps.len(), sc.steps.len());
    for k in refused {
        let message = String::from_utf8(ocio_testkit::oracle_values::bytes(
            &steps[k]["raised"]["message"],
        ))
        .unwrap();
        assert!(message.contains("U-112"), "step {k}: {}", steps[k]);
    }
}
