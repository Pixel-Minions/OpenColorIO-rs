// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The transform classes of chunk 6.3a against the wheel (`Transform`, `MatrixTransform`,
//! `RangeTransform`, `AllocationTransform`):
//! - each class's surface as the oracle's walk reports it (py_surface_walk.py's `_class`, run on
//!   the port's class in the embedded interpreter, against the wheel's py_surface): metatype,
//!   MRO, docstring, every member in order with its kind and docstring (pybind11's signatures:
//!   parameter names, types and defaults);
//! - their behaviour, through the same step scripts on both sides: constructors (each overload,
//!   positional and keyword, their refusals), every getter and setter with values whose bits
//!   differ (signed zeros, NaN payloads, infinities, extremes), the casters' refusals (a list of
//!   the wrong length, `None`, a string, a float for an `int`), `equals`, the static functions
//!   and their errors, `validate`, `repr`, `__deepcopy__`, `getFormatMetadata`'s identity, the
//!   base class's methods on each class, a Python subclass, and a direction outside the
//!   enumerators (I-261).
//!
//! And, without the wheel (whose behaviour the port can't hold, U-112): an enum value outside
//! its enumerators given to a setter is the port's error.
#![cfg(feature = "python")]

mod common;

use common::script::{
    Expect, Script, b, base_methods, compare, d, f, fl, i, l, n, none, r, ra, s, special, t,
};
use serde_json::Value;

/// The classes this chunk binds completely.
const CLASSES: [&str; 4] = [
    "Transform",
    "MatrixTransform",
    "RangeTransform",
    "AllocationTransform",
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

/// `MatrixTransform`.
fn matrix_script() -> Script {
    let mut sc = Script::new();
    let m = sc.call(n("OCIO.MatrixTransform"), vec![]);
    sc.eval(
        r(m),
        &[
            "getMatrix",
            "getOffset",
            "getFileInputBitDepth",
            "getFileOutputBitDepth",
        ],
    );
    base_methods(&mut sc, m, true);
    // Every value's bits.
    let m44: Vec<Value> = (0..16).map(special).collect();
    sc.call(ra(m, "setMatrix"), vec![l(m44.clone())]);
    sc.call(ra(m, "getMatrix"), vec![]);
    sc.repr(r(m));
    sc.call(ra(m, "setOffset"), vec![t((4..8).map(special).collect())]);
    sc.call(ra(m, "getOffset"), vec![]);
    sc.repr(r(m));
    sc.call(ra(m, "validate"), vec![]);
    // Casters: ints, a tuple, wrong lengths, None, strings.
    sc.call(
        ra(m, "setOffset"),
        vec![l(vec![i(1), i(-2), b(true), f(0.5)])],
    );
    sc.call(ra(m, "getOffset"), vec![]);
    sc.raises(ra(m, "setOffset"), vec![fl(&[1.0, 2.0, 3.0])]);
    sc.raises(ra(m, "setOffset"), vec![fl(&[1.0, 2.0, 3.0, 4.0, 5.0])]);
    sc.raises(ra(m, "setOffset"), vec![none()]);
    sc.raises(ra(m, "setOffset"), vec![s("abcd")]);
    sc.raises(
        ra(m, "setOffset"),
        vec![l(vec![s("a"), f(1.0), f(1.0), f(1.0)])],
    );
    sc.raises(ra(m, "setMatrix"), vec![fl(&[0.0; 15])]);
    sc.call(ra(m, "getMatrix"), vec![]);
    // Constructors.
    let id: Vec<f64> = vec![
        2.0, 0.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 0.0, 4.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    let m2 = sc.call(
        n("OCIO.MatrixTransform"),
        vec![
            fl(&id),
            fl(&[0.1, 0.2, 0.3, 0.4]),
            n("OCIO.TRANSFORM_DIR_INVERSE"),
        ],
    );
    sc.eval(
        r(m2),
        &["getMatrix", "getOffset", "getDirection", "__repr__"],
    );
    let m3 = sc.call_kw(
        n("OCIO.MatrixTransform"),
        vec![],
        vec![
            ("direction", n("OCIO.TRANSFORM_DIR_INVERSE")),
            ("offset", fl(&[1.0, 2.0, 3.0, 4.0])),
        ],
        Expect::Value,
    );
    sc.eval(r(m3), &["getMatrix", "getOffset", "getDirection"]);
    sc.call_kw(
        n("OCIO.MatrixTransform"),
        vec![],
        vec![
            ("matrix", fl(&[1.0; 16])),
            ("direction", n("OCIO.TRANSFORM_DIR_INVERSE")),
        ],
        Expect::Raised,
    );
    sc.raises(n("OCIO.MatrixTransform"), vec![i(1)]);
    sc.raises(n("OCIO.MatrixTransform"), vec![none()]);
    sc.call_kw(
        n("OCIO.MatrixTransform"),
        vec![],
        vec![("nope", i(1))],
        Expect::Raised,
    );
    sc.raises(
        n("OCIO.MatrixTransform"),
        vec![
            fl(&id),
            fl(&[0.0; 4]),
            n("OCIO.TRANSFORM_DIR_FORWARD"),
            i(1),
        ],
    );
    // equals.
    sc.call(ra(m2, "equals"), vec![r(m2)]);
    sc.call(ra(m2, "equals"), vec![r(m3)]);
    let m4 = sc.call(n("copy.deepcopy"), vec![r(m2)]);
    sc.call(ra(m2, "equals"), vec![r(m4)]);
    sc.call(
        ra(m4, "setFileInputBitDepth"),
        vec![n("OCIO.BIT_DEPTH_UINT10")],
    );
    sc.call(ra(m2, "equals"), vec![r(m4)]);
    sc.call(ra(m, "equals"), vec![r(m)]);
    sc.raises(ra(m2, "equals"), vec![none()]);
    sc.raises(ra(m2, "equals"), vec![n("OCIO.RangeTransform")]);
    let range = sc.call(n("OCIO.RangeTransform"), vec![]);
    sc.raises(ra(m2, "equals"), vec![r(range)]);
    sc.raises(n("OCIO.MatrixTransform.getMatrix"), vec![r(range)]);
    // Bit depths.
    for bd in [
        "BIT_DEPTH_UINT8",
        "BIT_DEPTH_UINT10",
        "BIT_DEPTH_UINT12",
        "BIT_DEPTH_UINT14",
        "BIT_DEPTH_UINT16",
        "BIT_DEPTH_UINT32",
        "BIT_DEPTH_F16",
        "BIT_DEPTH_F32",
        "BIT_DEPTH_UNKNOWN",
    ] {
        sc.call(
            ra(m3, "setFileInputBitDepth"),
            vec![n(&format!("OCIO.{bd}"))],
        );
        sc.call(
            ra(m3, "setFileOutputBitDepth"),
            vec![n(&format!("OCIO.{bd}"))],
        );
        sc.eval(
            r(m3),
            &["getFileInputBitDepth", "getFileOutputBitDepth", "__repr__"],
        );
    }
    sc.raises(ra(m3, "setFileInputBitDepth"), vec![i(8)]);
    // Static functions.
    let fit = sc.call(n("OCIO.MatrixTransform.Fit"), vec![]);
    sc.eval(r(fit), &["getMatrix", "getOffset", "__repr__"]);
    let fit = sc.call(
        n("OCIO.MatrixTransform.Fit"),
        vec![
            fl(&[0.0, 0.1, -1.0, 0.0]),
            fl(&[1.0, 0.9, 1.0, 3.0]),
            fl(&[0.25, 0.0, 0.0, 0.0]),
            fl(&[0.75, 2.0, 1e-300, -1.0]),
        ],
    );
    sc.eval(r(fit), &["getMatrix", "getOffset"]);
    sc.call_kw(
        n("OCIO.MatrixTransform.Fit"),
        vec![],
        vec![
            ("newMax", fl(&[2.0, 2.0, 2.0, 2.0])),
            ("oldMin", fl(&[0.5, 0.5, 0.5, 0.5])),
        ],
        Expect::Value,
    );
    sc.raises(
        n("OCIO.MatrixTransform.Fit"),
        vec![fl(&[0.0; 4]), fl(&[1.0, 1.0, 0.0, 1.0])],
    );
    sc.call(
        n("OCIO.MatrixTransform.Fit"),
        vec![fl(&[0.0; 4]), fl(&[1.0, 1.0, 1.0, f64::NAN])],
    );
    let ident = sc.call(n("OCIO.MatrixTransform.Identity"), vec![]);
    sc.eval(r(ident), &["getMatrix", "getOffset", "__repr__"]);
    sc.raises(n("OCIO.MatrixTransform.Identity"), vec![i(1)]);
    for (sat, luma) in [
        (0.5, [0.2126, 0.7152, 0.0722]),
        (0.0, [0.3, 0.6, 0.1]),
        (-2.5, [1e-10, 0.0, -0.0]),
        (f64::INFINITY, [1.0, 1.0, 1.0]),
    ] {
        let sat = sc.call(n("OCIO.MatrixTransform.Sat"), vec![f(sat), fl(&luma)]);
        sc.eval(r(sat), &["getMatrix", "getOffset"]);
    }
    sc.call(
        n("OCIO.MatrixTransform.Sat"),
        vec![i(2), l(vec![i(1), i(0), i(0)])],
    );
    sc.raises(n("OCIO.MatrixTransform.Sat"), vec![f(0.5)]);
    sc.raises(n("OCIO.MatrixTransform.Sat"), vec![f(0.5), fl(&[1.0, 2.0])]);
    sc.call(
        n("OCIO.MatrixTransform.Scale"),
        vec![fl(&[2.0, -0.0, 1e300, 0.5])],
    );
    let scale = sc.call(
        n("OCIO.MatrixTransform.Scale"),
        vec![fl(&[2.0, -0.0, 1e300, 0.5])],
    );
    sc.eval(r(scale), &["getMatrix", "getOffset"]);
    for hot in [
        [1, 1, 1, 1],
        [0, 0, 0, 1],
        [1, 0, 0, 0],
        [0, 1, 1, 0],
        [0, 0, 0, 0],
        [5, -1, 0, 0],
    ] {
        let v = sc.call(
            n("OCIO.MatrixTransform.View"),
            vec![
                l(hot.iter().map(|&h| i(h)).collect()),
                fl(&[0.2126, 0.7152, 0.0722]),
            ],
        );
        sc.eval(r(v), &["getMatrix", "getOffset"]);
    }
    sc.call(
        n("OCIO.MatrixTransform.View"),
        vec![l(vec![i(1), i(1), i(0), i(0)]), fl(&[0.5, -0.5, 0.0])],
    );
    sc.raises(
        n("OCIO.MatrixTransform.View"),
        vec![fl(&[1.0, 1.0, 0.0, 0.0]), fl(&[0.2126, 0.7152, 0.0722])],
    );
    sc.raises(
        n("OCIO.MatrixTransform.View"),
        vec![
            l(vec![i(1 << 40), i(0), i(0), i(0)]),
            fl(&[0.2126, 0.7152, 0.0722]),
        ],
    );
    // A direction outside the enumerators (I-261).
    let bad = sc.call(n("OCIO.TransformDirection"), vec![i(5)]);
    sc.call(ra(m3, "setDirection"), vec![r(bad)]);
    sc.call(ra(m3, "getDirection"), vec![]);
    sc.repr(r(m3));
    sc.raises(ra(m3, "validate"), vec![]);
    sc.call_kw(
        n("OCIO.MatrixTransform"),
        vec![],
        vec![("direction", r(bad))],
        Expect::Raised,
    );
    // getFormatMetadata: the same object while it lives.
    let md = sc.call(ra(m, "getFormatMetadata"), vec![]);
    let md2 = sc.call(ra(m, "getFormatMetadata"), vec![]);
    // FormatMetadata's repr comes with its class (6.3d).
    sc.unrepr(md);
    sc.unrepr(md2);
    sc.call(n("operator.is_"), vec![r(md), r(md2)]);
    sc.call(n("type"), vec![r(md)]);
    // The base class has no constructor; a Python subclass.
    sc.raises(n("OCIO.Transform"), vec![]);
    let sub = sc.call(
        n("type"),
        vec![
            s("M"),
            t(vec![n("OCIO.MatrixTransform")]),
            d(vec![(s("__module__"), s("probe"))]),
        ],
    );
    let obj = sc.call(r(sub), vec![]);
    sc.eval(r(obj), &["getMatrix", "getTransformType", "__repr__"]);
    sc.call(n("isinstance"), vec![r(obj), n("OCIO.Transform")]);
    sc.call(ra(m2, "equals"), vec![r(obj)]);
    let copy = sc.call(n("copy.deepcopy"), vec![r(obj)]);
    sc.call(n("type"), vec![r(copy)]);
    sc
}

/// `RangeTransform`.
fn range_script() -> Script {
    let mut sc = Script::new();
    let rt = sc.call(n("OCIO.RangeTransform"), vec![]);
    let all = [
        "getStyle",
        "getMinInValue",
        "getMaxInValue",
        "getMinOutValue",
        "getMaxOutValue",
        "hasMinInValue",
        "hasMaxInValue",
        "hasMinOutValue",
        "hasMaxOutValue",
        "getFileInputBitDepth",
        "getFileOutputBitDepth",
        "__repr__",
    ];
    sc.eval(r(rt), &all);
    base_methods(&mut sc, rt, false);
    for (k, v) in ["MinIn", "MaxIn", "MinOut", "MaxOut"].iter().enumerate() {
        sc.call(ra(rt, &format!("set{v}Value")), vec![special(k + 1)]);
        sc.eval(r(rt), &all);
        sc.call(
            ra(rt, &format!("set{v}Value")),
            vec![f(0.1 * (k as f64 + 1.0))],
        );
    }
    sc.eval(r(rt), &all);
    sc.call(ra(rt, "validate"), vec![]);
    sc.call(ra(rt, "setStyle"), vec![n("OCIO.RANGE_NO_CLAMP")]);
    sc.eval(r(rt), &all);
    sc.call(ra(rt, "unsetMaxInValue"), vec![]);
    sc.eval(r(rt), &all);
    sc.raises(ra(rt, "validate"), vec![]);
    sc.call(ra(rt, "unsetMaxOutValue"), vec![]);
    sc.raises(ra(rt, "validate"), vec![]);
    sc.call(ra(rt, "setStyle"), vec![n("OCIO.RANGE_CLAMP")]);
    sc.raises(ra(rt, "validate"), vec![]);
    sc.eval(r(rt), &all);
    sc.call(ra(rt, "unsetMinInValue"), vec![]);
    sc.call(ra(rt, "unsetMinOutValue"), vec![]);
    sc.eval(r(rt), &all);
    sc.call(ra(rt, "setMinInValue"), vec![i(3)]);
    sc.raises(ra(rt, "setMinInValue"), vec![s("1")]);
    sc.raises(ra(rt, "setMinInValue"), vec![none()]);
    sc.raises(ra(rt, "setStyle"), vec![i(1)]);
    sc.raises(ra(rt, "validate"), vec![]);
    sc.call(ra(rt, "setMaxInValue"), vec![f(1.0)]);
    sc.raises(ra(rt, "validate"), vec![]);
    // Constructors.
    let r2 = sc.call(
        n("OCIO.RangeTransform"),
        vec![f(0.1), f(1.1), f(0.2), f(1.4)],
    );
    sc.eval(r(r2), &all);
    let r3 = sc.call_kw(
        n("OCIO.RangeTransform"),
        vec![],
        vec![("maxOutValue", f(1.0)), ("maxInValue", i(1))],
        Expect::Value,
    );
    sc.eval(r(r3), &all);
    sc.call_kw(
        n("OCIO.RangeTransform"),
        vec![],
        vec![("maxOutValue", f(2.0))],
        Expect::Raised,
    );
    sc.call(
        n("OCIO.RangeTransform"),
        vec![f(0.0), f(1.0), f(1.0), f(1.0)],
    );
    sc.raises(
        n("OCIO.RangeTransform"),
        vec![f(1.0), f(0.0), f(0.0), f(1.0)],
    );
    sc.raises(n("OCIO.RangeTransform"), vec![s("0")]);
    let r4 = sc.call(
        n("OCIO.RangeTransform"),
        vec![
            f(0.0),
            f(1.0),
            f(0.5),
            f(1.5),
            n("OCIO.TRANSFORM_DIR_INVERSE"),
        ],
    );
    sc.eval(r(r4), &all);
    sc.call(ra(r2, "equals"), vec![r(r2)]);
    sc.call(ra(r2, "equals"), vec![r(r3)]);
    let r5 = sc.call(n("copy.deepcopy"), vec![r(r2)]);
    sc.call(ra(r2, "equals"), vec![r(r5)]);
    sc.raises(ra(r2, "equals"), vec![none()]);
    let nan = sc.call(n("OCIO.RangeTransform"), vec![]);
    sc.call(ra(nan, "equals"), vec![r(nan)]);
    let nan2 = sc.call(n("OCIO.RangeTransform"), vec![]);
    sc.call(ra(nan, "equals"), vec![r(nan2)]);
    sc.call(
        ra(r2, "setFileInputBitDepth"),
        vec![n("OCIO.BIT_DEPTH_F16")],
    );
    sc.call(
        ra(r2, "setFileOutputBitDepth"),
        vec![n("OCIO.BIT_DEPTH_UINT12")],
    );
    sc.eval(r(r2), &all);
    sc.call(ra(r2, "equals"), vec![r(r5)]);
    let bad = sc.call(n("OCIO.TransformDirection"), vec![i(7)]);
    sc.call(ra(r2, "setDirection"), vec![r(bad)]);
    sc.raises(ra(r2, "validate"), vec![]);
    sc.eval(r(r2), &all);
    sc
}

/// `AllocationTransform`.
fn allocation_script() -> Script {
    let mut sc = Script::new();
    let a = sc.call(n("OCIO.AllocationTransform"), vec![]);
    let all = ["getAllocation", "getVars", "getDirection", "__repr__"];
    sc.eval(r(a), &all);
    base_methods(&mut sc, a, true);
    for vars in [
        vec![f(0.0), f(1.0)],
        vec![f(-8.0), f(5.0), f(0.1)],
        vec![special(3), special(2), special(5)],
        vec![f(1e300), f(-1e-300)],
        vec![i(1), i(2)],
    ] {
        sc.call(ra(a, "setVars"), vec![l(vars)]);
        sc.eval(r(a), &all);
    }
    sc.raises(ra(a, "setVars"), vec![fl(&[1.0])]);
    sc.raises(ra(a, "setVars"), vec![fl(&[])]);
    sc.raises(ra(a, "setVars"), vec![fl(&[1.0, 2.0, 3.0, 4.0])]);
    sc.raises(ra(a, "setVars"), vec![none()]);
    sc.raises(ra(a, "setVars"), vec![s("ab")]);
    sc.call(ra(a, "setVars"), vec![t(vec![f(0.5), f(0.25)])]);
    sc.eval(r(a), &all);
    for alloc in ["ALLOCATION_LG2", "ALLOCATION_UNKNOWN", "ALLOCATION_UNIFORM"] {
        sc.call(ra(a, "setAllocation"), vec![n(&format!("OCIO.{alloc}"))]);
        sc.eval(r(a), &all);
        sc.step(
            serde_json::json!({ "call": ra(a, "validate"), "args": [] }),
            if alloc == "ALLOCATION_UNKNOWN" {
                Expect::Raised
            } else {
                Expect::Value
            },
        );
    }
    sc.raises(ra(a, "setAllocation"), vec![i(1)]);
    sc.call(ra(a, "setVars"), vec![fl(&[0.0, 1.0, 2.0])]);
    sc.raises(ra(a, "validate"), vec![]);
    sc.call(ra(a, "setAllocation"), vec![n("OCIO.ALLOCATION_LG2")]);
    sc.call(ra(a, "validate"), vec![]);
    // Constructors.
    let a2 = sc.call(
        n("OCIO.AllocationTransform"),
        vec![n("OCIO.ALLOCATION_LG2"), fl(&[-8.0, 5.0, 0.00390625])],
    );
    sc.eval(r(a2), &all);
    let a3 = sc.call_kw(
        n("OCIO.AllocationTransform"),
        vec![],
        vec![
            ("vars", fl(&[])),
            ("direction", n("OCIO.TRANSFORM_DIR_INVERSE")),
        ],
        Expect::Value,
    );
    sc.eval(r(a3), &all);
    sc.call_kw(
        n("OCIO.AllocationTransform"),
        vec![],
        vec![("vars", fl(&[1.0]))],
        Expect::Raised,
    );
    sc.call_kw(
        n("OCIO.AllocationTransform"),
        vec![],
        vec![("allocation", n("OCIO.ALLOCATION_UNKNOWN"))],
        Expect::Raised,
    );
    sc.call_kw(
        n("OCIO.AllocationTransform"),
        vec![],
        vec![("vars", fl(&[0.0, 1.0, 2.0]))],
        Expect::Raised,
    );
    sc.raises(n("OCIO.AllocationTransform"), vec![i(1)]);
    sc
}

/// The classes behave as the wheel's.
#[test]
fn the_classes_behave_as_the_wheels() {
    compare(&[matrix_script(), range_script(), allocation_script()]);
}

/// An enum value outside its enumerators, which the wheel's setters store and the port's
/// library can't hold, is the port's error (U-112).
#[test]
fn an_enum_value_outside_the_enumerators_is_the_ports_error() {
    let mut sc = Script::new();
    let m = sc.call(n("OCIO.MatrixTransform"), vec![]);
    let bd = sc.call(n("OCIO.BitDepth"), vec![i(42)]);
    sc.raises(ra(m, "setFileInputBitDepth"), vec![r(bd)]);
    sc.raises(ra(m, "setFileOutputBitDepth"), vec![r(bd)]);
    let rt = sc.call(n("OCIO.RangeTransform"), vec![]);
    let st = sc.call(n("OCIO.RangeStyle"), vec![i(9)]);
    sc.raises(ra(rt, "setStyle"), vec![r(st)]);
    let a = sc.call(n("OCIO.AllocationTransform"), vec![]);
    let al = sc.call(n("OCIO.Allocation"), vec![i(7)]);
    sc.raises(ra(a, "setAllocation"), vec![r(al)]);
    sc.call_kw(
        n("OCIO.AllocationTransform"),
        vec![],
        vec![("allocation", r(al)), ("vars", fl(&[0.0, 1.0]))],
        Expect::Raised,
    );
    let ours = common::port(&sc.json());
    let steps = ours["steps"].as_array().unwrap();
    assert_eq!(steps.len(), sc.steps.len());
    for (k, step) in steps.iter().enumerate() {
        if sc.expect[k] == Expect::Raised {
            let message = String::from_utf8(ocio_testkit::oracle_values::bytes(
                &step["raised"]["message"],
            ))
            .unwrap();
            assert!(message.contains("U-112"), "step {k}: {step}");
            assert_eq!(step["raised"]["type"], "builtins.ValueError", "step {k}");
        } else {
            assert!(step.get("value").is_some(), "step {k}: {step}");
        }
    }
}
