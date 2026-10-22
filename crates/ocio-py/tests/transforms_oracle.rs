// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The transform classes of chunk 6.3a against the wheel (`Transform`, `MatrixTransform`,
//! `RangeTransform`, `AllocationTransform`, `GroupTransform` and its iterators):
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
//!   enumerators (I-261);
//! - the group: its children shared with the Python objects given (identity and changes), in
//!   order, a copy sharing them, its iterators (`len`, indexing, iteration to `StopIteration`),
//!   the write formats, `write` to a string in each format and its errors, null children and a
//!   group holding itself where the wheel survives them, a child's validation error;
//! - a style outside its enumerators given to the classes that convert it at once (the
//!   exponent, fixed function and CDL classes of chunks 6.3b and 6.3c): the wheel's error,
//!   written over its own start (I-262); the constructors of `AllocationTransform` and
//!   `FileTransform` given such a value, whose validation errors come first.
//!
//! And, without the wheel (which runs the same steps without an error): an enum value outside
//! its enumerators given to a setter that stores it is the port's error (the port's library
//! can't hold it, U-112); so are a null child and a group holding
//! itself where the wheel dereferences or recurses (U-110, U-111).
#![cfg(feature = "python")]

mod common;

use common::script::{
    Expect, Script, b, base_methods, by, compare, d, f, fl, i, l, n, none, r, ra, s, special, t,
};
use serde_json::Value;

/// The classes this chunk binds completely.
const CLASSES: [&str; 5] = [
    "Transform",
    "GroupTransform",
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

/// `GroupTransform`, its iterators and `write` (to a string).
fn group_script() -> Script {
    let mut sc = Script::new();
    // The current config `write` takes, the same on both sides.
    sc.call(n("OCIO.UnsetEnvVariable"), vec![s("OCIO")]);
    let g = sc.call(n("OCIO.GroupTransform"), vec![]);
    sc.eval(r(g), &["__len__", "getDirection", "__repr__"]);
    base_methods(&mut sc, g, true);
    // Children: shared, in order.
    let m = sc.call(n("OCIO.MatrixTransform"), vec![]);
    let rt = sc.call(
        n("OCIO.RangeTransform"),
        vec![f(0.1), f(1.1), f(0.2), f(1.4)],
    );
    sc.call(ra(g, "appendTransform"), vec![r(m)]);
    sc.call(ra(g, "prependTransform"), vec![r(rt)]);
    sc.call_kw(
        ra(g, "appendTransform"),
        vec![],
        vec![("transform", r(m))],
        Expect::Value,
    );
    sc.call(n("len"), vec![r(g)]);
    sc.call(ra(g, "__getitem__"), vec![i(0)]);
    sc.call(ra(g, "__getitem__"), vec![i(1)]);
    sc.call(ra(g, "__getitem__"), vec![i(2)]);
    sc.raises(ra(g, "__getitem__"), vec![i(3)]);
    sc.raises(ra(g, "__getitem__"), vec![i(-1)]);
    sc.raises(ra(g, "__getitem__"), vec![f(1.0)]);
    sc.raises(ra(g, "__getitem__"), vec![i(1 << 40)]);
    sc.call(ra(m, "setOffset"), vec![fl(&[0.1, 0.2, 0.3, 0.4])]);
    sc.repr(r(g));
    sc.call(
        ra(rt, "setDirection"),
        vec![n("OCIO.TRANSFORM_DIR_INVERSE")],
    );
    sc.repr(r(g));
    sc.raises(ra(g, "appendTransform"), vec![i(1)]);
    sc.raises(ra(g, "appendTransform"), vec![]);
    sc.raises(ra(g, "appendTransform"), vec![n("OCIO.MatrixTransform")]);
    // Iterators.
    let it = sc.call(n("iter"), vec![r(g)]);
    sc.call(n("type"), vec![r(it)]);
    sc.call(n("iter"), vec![r(it)]);
    sc.call(n("len"), vec![r(it)]);
    sc.call(ra(it, "__getitem__"), vec![i(2)]);
    sc.raises(ra(it, "__getitem__"), vec![i(3)]);
    sc.raises(ra(it, "__getitem__"), vec![i(-1)]);
    for _ in 0..3 {
        sc.call(n("next"), vec![r(it)]);
    }
    sc.raises(n("next"), vec![r(it)]);
    sc.raises(n("next"), vec![r(it)]);
    sc.call(ra(g, "appendTransform"), vec![r(rt)]);
    sc.call(n("next"), vec![r(it)]);
    sc.call(n("len"), vec![r(it)]);
    sc.call(n("list"), vec![r(g)]);
    sc.raises(n("OCIO.GroupTransform.TransformIterator"), vec![]);
    sc.raises(
        n("OCIO.GroupTransform.TransformIterator.__len__"),
        vec![r(g)],
    );
    // A copy shares the children.
    let copy = sc.call(n("copy.deepcopy"), vec![r(g)]);
    sc.call(ra(copy, "__getitem__"), vec![i(1)]);
    sc.call(ra(copy, "appendTransform"), vec![r(m)]);
    sc.call(n("len"), vec![r(copy)]);
    sc.call(n("len"), vec![r(g)]);
    // Constructors.
    let g2 = sc.call(
        n("OCIO.GroupTransform"),
        vec![l(vec![r(m), r(rt)]), n("OCIO.TRANSFORM_DIR_INVERSE")],
    );
    sc.eval(r(g2), &["__len__", "getDirection", "__repr__"]);
    sc.call(ra(g2, "__getitem__"), vec![i(0)]);
    let g3 = sc.call_kw(
        n("OCIO.GroupTransform"),
        vec![],
        vec![
            ("direction", n("OCIO.TRANSFORM_DIR_INVERSE")),
            ("transforms", t(vec![r(rt), r(g2)])),
        ],
        Expect::Value,
    );
    sc.eval(r(g3), &["__len__", "getDirection", "__repr__"]);
    sc.call(n("OCIO.GroupTransform"), vec![l(vec![])]);
    sc.raises(n("OCIO.GroupTransform"), vec![none()]);
    sc.raises(n("OCIO.GroupTransform"), vec![i(1)]);
    sc.raises(n("OCIO.GroupTransform"), vec![l(vec![i(1)])]);
    sc.raises(n("OCIO.GroupTransform"), vec![s("ab")]);
    sc.raises(n("OCIO.GroupTransform"), vec![r(m)]);
    sc.raises(n("OCIO.GroupTransform"), vec![l(vec![r(m)]), none()]);
    let listed = sc.call(n("iter"), vec![l(vec![r(m)])]);
    sc.raises(n("OCIO.GroupTransform"), vec![r(listed)]);
    let set = sc.call(n("set"), vec![l(vec![r(m)])]);
    let from_set = sc.call(n("OCIO.GroupTransform"), vec![r(set)]);
    sc.call(ra(from_set, "__getitem__"), vec![i(0)]);
    // A child's validation error, the group's own direction.
    let bad = sc.call(n("OCIO.RangeTransform"), vec![]);
    sc.call(ra(g2, "appendTransform"), vec![r(bad)]);
    sc.raises(ra(g2, "validate"), vec![]);
    let dir5 = sc.call(n("OCIO.TransformDirection"), vec![i(5)]);
    let g4 = sc.call(n("OCIO.GroupTransform"), vec![l(vec![r(m)])]);
    sc.call(ra(g4, "setDirection"), vec![r(dir5)]);
    sc.raises(ra(g4, "validate"), vec![]);
    sc.call(ra(g4, "getDirection"), vec![]);
    sc.repr(r(g4));
    sc.call(
        ra(g4, "setDirection"),
        vec![n("OCIO.TRANSFORM_DIR_FORWARD")],
    );
    let m5 = sc.call(n("OCIO.MatrixTransform"), vec![]);
    sc.call(ra(g4, "appendTransform"), vec![r(m5)]);
    sc.call(ra(m5, "setDirection"), vec![r(dir5)]);
    sc.raises(ra(g4, "validate"), vec![]);
    sc.call_kw(
        n("OCIO.GroupTransform"),
        vec![],
        vec![("transforms", l(vec![r(m5)]))],
        Expect::Raised,
    );
    // A null child: counted, read and iterated (U-110 covers what dereferences it).
    let holes = sc.call(n("OCIO.GroupTransform"), vec![]);
    sc.call(ra(holes, "appendTransform"), vec![none()]);
    sc.call(ra(holes, "appendTransform"), vec![r(m)]);
    sc.call(ra(holes, "prependTransform"), vec![none()]);
    sc.call(n("len"), vec![r(holes)]);
    sc.call(ra(holes, "__getitem__"), vec![i(0)]);
    sc.call(n("list"), vec![r(holes)]);
    // A copy, whose repr (which the step interpreter takes) dereferences its null children
    // (U-110): the Windows wheel raises std::bad_typeid's RuntimeError there, the Linux one
    // reads through the null pointer.
    if cfg!(windows) {
        let copy = sc.call(n("copy.deepcopy"), vec![r(holes)]);
        sc.unrepr(copy);
    }
    // A group that holds itself: what doesn't recurse through it (U-111 covers what does).
    let cyc = sc.call(n("OCIO.GroupTransform"), vec![]);
    sc.call(ra(cyc, "appendTransform"), vec![r(cyc)]);
    sc.call(n("len"), vec![r(cyc)]);
    // A child that fails validation before a null child or the cycle: its error.
    let bad_holes = sc.call(n("OCIO.GroupTransform"), vec![]);
    sc.call(ra(bad_holes, "appendTransform"), vec![r(bad)]);
    sc.raises(ra(bad_holes, "validate"), vec![]);
    sc.call(ra(bad_holes, "appendTransform"), vec![none()]);
    sc.call(ra(bad_holes, "appendTransform"), vec![r(bad_holes)]);
    sc.raises(ra(bad_holes, "validate"), vec![]);
    // getFormatMetadata: the same object while it lives.
    let md = sc.call(ra(g, "getFormatMetadata"), vec![]);
    let md2 = sc.call(ra(g, "getFormatMetadata"), vec![]);
    sc.unrepr(md);
    sc.unrepr(md2);
    // The write formats.
    let wf = sc.call(n("OCIO.GroupTransform.GetWriteFormats"), vec![]);
    sc.call(n("type"), vec![r(wf)]);
    sc.call(n("len"), vec![r(wf)]);
    for k in [0, 1, 2, 3, 4, -1, -100] {
        sc.call(ra(wf, "__getitem__"), vec![i(k)]);
    }
    sc.raises(ra(wf, "__getitem__"), vec![i(5)]);
    sc.call(n("iter"), vec![r(wf)]);
    sc.call(n("list"), vec![r(wf)]);
    sc.raises(n("next"), vec![r(wf)]);
    let wf2 = sc.call(n("OCIO.GroupTransform.GetWriteFormats"), vec![]);
    sc.call(n("next"), vec![r(wf2)]);
    sc.call(n("operator.is_"), vec![r(wf), r(wf2)]);
    sc.raises(n("OCIO.GroupTransform.GetWriteFormats"), vec![i(1)]);
    sc.eval(n("OCIO.GroupTransform.GetWriteFormats.__doc__"), &[]);
    // write, to a string, with the current config.
    let mt = sc.call(n("OCIO.MatrixTransform"), vec![]);
    sc.call(ra(mt, "setOffset"), vec![fl(&[0.1, 0.2, 0.3, 0.0])]);
    let w = sc.call(n("OCIO.GroupTransform"), vec![l(vec![r(mt)])]);
    let first = sc.call(ra(w, "write"), vec![s("Color Transform Format")]);
    sc.unlog(first);
    sc.call(ra(w, "write"), vec![s("Academy/ASC Common LUT Format")]);
    sc.call(ra(w, "write"), vec![s("color transform format")]);
    for f in [
        "ColorDecisionList",
        "ColorCorrection",
        "ColorCorrectionCollection",
        "nope",
        "",
    ] {
        sc.raises(ra(w, "write"), vec![s(f)]);
    }
    sc.call(ra(w, "write"), vec![by(b"Color Transform Format")]);
    sc.call_kw(
        ra(w, "write"),
        vec![],
        vec![
            ("formatName", s("Color Transform Format")),
            ("config", none()),
        ],
        Expect::Value,
    );
    sc.raises(ra(w, "write"), vec![none()]);
    sc.raises(ra(w, "write"), vec![]);
    sc.raises(ra(w, "write"), vec![s("Color Transform Format"), i(1)]);
    sc.raises(ra(w, "write"), vec![s("Color Transform Format"), r(m)]);
    sc.call(ra(w, "write"), vec![s("Color Transform Format\u{0}x")]);
    sc.call(ra(w, "setDirection"), vec![n("OCIO.TRANSFORM_DIR_INVERSE")]);
    sc.call(ra(w, "write"), vec![s("Color Transform Format")]);
    // Null children and a group that holds itself, as the writers see them; an empty group.
    sc.call(ra(holes, "write"), vec![s("Color Transform Format")]);
    sc.raises(ra(holes, "write"), vec![s("Academy/ASC Common LUT Format")]);
    for f in [
        "ColorDecisionList",
        "ColorCorrection",
        "ColorCorrectionCollection",
    ] {
        sc.raises(ra(holes, "write"), vec![s(f)]);
        sc.raises(ra(cyc, "write"), vec![s(f)]);
    }
    sc.raises(ra(cyc, "write"), vec![s("nope")]);
    let empty = sc.call(n("OCIO.GroupTransform"), vec![]);
    sc.call(ra(empty, "write"), vec![s("Color Transform Format")]);
    sc.raises(ra(empty, "write"), vec![s("ColorDecisionList")]);
    sc.raises(ra(empty, "write"), vec![s("ColorCorrection")]);
    sc.raises(ra(g3, "write"), vec![s("Color Transform Format")]);
    // A Python subclass, as a child.
    let sub = sc.call(
        n("type"),
        vec![
            s("G"),
            t(vec![n("OCIO.GroupTransform")]),
            d(vec![(s("__module__"), s("probe"))]),
        ],
    );
    let obj = sc.call(r(sub), vec![l(vec![r(m)])]);
    sc.eval(r(obj), &["__len__", "__repr__"]);
    sc.call(ra(g, "appendTransform"), vec![r(obj)]);
    sc.call(ra(g, "__getitem__"), vec![i(4)]);
    sc
}

/// The classes behave as the wheel's.
#[test]
fn the_classes_behave_as_the_wheels() {
    compare(&[
        matrix_script(),
        range_script(),
        allocation_script(),
        group_script(),
    ]);
}

/// `common`, numbers outside an enum's values that both wheels make an enum of, and the first
/// `n` of those only this platform's wheel makes one of (I-225): the `int`'s most negative,
/// -12345, -10 and -1 in the Windows wheel; in the Linux one, the `unsigned int`'s that are
/// the same bits.
fn with_platforms(common: &[i128], n: usize) -> Vec<i128> {
    let negative = [-2147483648_i128, -12345, -10, -1];
    let mut values = common.to_vec();
    values.extend(negative.iter().take(n).map(
        |&v| {
            if cfg!(windows) { v } else { v + (1_i128 << 32) }
        },
    ));
    values
}

/// An enum value outside its enumerators, which the wheel's setters store and the port's
/// library can't hold, is the port's error (U-112): each setter that stores one, and
/// `FileTransform`'s constructor once its validation passed. The wheel runs the same steps
/// without an error, so each of them does store the value there.
#[test]
fn an_enum_value_outside_the_enumerators_is_the_ports_error() {
    let mut sc = Script::new();
    let mut refused = Vec::new();
    let m = sc.call(n("OCIO.MatrixTransform"), vec![]);
    let bd = sc.call(n("OCIO.BitDepth"), vec![i(42)]);
    refused.push(sc.call(ra(m, "setFileInputBitDepth"), vec![r(bd)]));
    refused.push(sc.call(ra(m, "setFileOutputBitDepth"), vec![r(bd)]));
    let rt = sc.call(n("OCIO.RangeTransform"), vec![]);
    refused.push(sc.call(ra(rt, "setFileInputBitDepth"), vec![r(bd)]));
    refused.push(sc.call(ra(rt, "setFileOutputBitDepth"), vec![r(bd)]));
    let st = sc.call(n("OCIO.RangeStyle"), vec![i(9)]);
    refused.push(sc.call(ra(rt, "setStyle"), vec![r(st)]));
    let a = sc.call(n("OCIO.AllocationTransform"), vec![]);
    let al = sc.call(n("OCIO.Allocation"), vec![i(7)]);
    refused.push(sc.call(ra(a, "setAllocation"), vec![r(al)]));
    let file = sc.call(n("OCIO.FileTransform"), vec![]);
    for v in with_platforms(&[5, 99, 253, 256, 2147483647], 1) {
        let interp = sc.call(n("OCIO.Interpolation"), vec![i(v)]);
        refused.push(sc.call(ra(file, "setInterpolation"), vec![r(interp)]));
        refused.push(sc.call_kw(
            n("OCIO.FileTransform"),
            vec![s("a.clf")],
            vec![("interpolation", r(interp))],
            Expect::Value,
        ));
    }
    for v in with_platforms(&[2, 9, 2147483647], 1) {
        let style = sc.call(n("OCIO.CDLStyle"), vec![i(v)]);
        refused.push(sc.call(ra(file, "setCDLStyle"), vec![r(style)]));
    }
    assert_eq!(refused.len(), 22);
    assert!(sc.expect.iter().all(|e| *e == Expect::Value));
    let theirs = common::wheel(&[sc.json()]);
    let theirs = theirs[0]["steps"].as_array().unwrap();
    let ours = common::port(&sc.json());
    let ours = ours["steps"].as_array().unwrap();
    assert_eq!(theirs.len(), sc.steps.len());
    assert_eq!(ours.len(), sc.steps.len());
    for (k, (step, wheel_step)) in ours.iter().zip(theirs).enumerate() {
        assert!(wheel_step.get("value").is_some(), "step {k}: {wheel_step}");
        if refused.contains(&k) {
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

/// A style outside its enumerators that the wheel converts at once, and so never stores,
/// raises the wheel's error, whose number the C++ stream writes over the start of the message
/// (I-262): the exponent classes' negative style (constructor and `setNegativeStyle`), the
/// fixed function's style (constructor and `setStyle`) and the CDL's (`setStyle`), with
/// numbers of each width up to the `int`'s extremes, the object unchanged after. And the
/// constructors that store one validate it as the wheel does: `AllocationTransform`'s
/// validation refuses it, after the errors of its vars and its direction;
/// `FileTransform`'s errors come before the port's refusal (U-112).
#[test]
fn a_style_outside_the_enumerators_raises_the_wheels_error() {
    const INT_MAX: i128 = 2147483647;
    let mut sc = Script::new();
    let odd_dir = sc.call(n("OCIO.TransformDirection"), vec![i(5)]);
    let inverse = n("OCIO.TRANSFORM_DIR_INVERSE");
    for v in with_platforms(&[4, 9, 10, 99, 255, 1000000, INT_MAX], 4) {
        let ns = sc.call(n("OCIO.NegativeStyle"), vec![i(v)]);
        for class in ["ExponentTransform", "ExponentWithLinearTransform"] {
            let class = format!("OCIO.{class}");
            sc.call_kw(
                n(&class),
                vec![],
                vec![("negativeStyle", r(ns))],
                Expect::Raised,
            );
            sc.call_kw(
                n(&class),
                vec![],
                vec![("negativeStyle", r(ns)), ("direction", r(odd_dir))],
                Expect::Raised,
            );
            let obj = sc.call(n(&class), vec![]);
            sc.raises(ra(obj, "setNegativeStyle"), vec![r(ns)]);
            sc.call(ra(obj, "setDirection"), vec![inverse.clone()]);
            sc.raises(ra(obj, "setNegativeStyle"), vec![r(ns)]);
            sc.eval(r(obj), &["getNegativeStyle", "getDirection", "__repr__"]);
        }
    }
    for v in with_platforms(&[23, 99, 100, 255, 1000000, INT_MAX], 4) {
        let style = sc.call(n("OCIO.FixedFunctionStyle"), vec![i(v)]);
        sc.raises(n("OCIO.FixedFunctionTransform"), vec![r(style)]);
        sc.call_kw(
            n("OCIO.FixedFunctionTransform"),
            vec![r(style)],
            vec![("params", fl(&[1.0, 2.0])), ("direction", r(odd_dir))],
            Expect::Raised,
        );
        let obj = sc.call(
            n("OCIO.FixedFunctionTransform"),
            vec![n("OCIO.FIXED_FUNCTION_ACES_GLOW_03")],
        );
        sc.raises(ra(obj, "setStyle"), vec![r(style)]);
        sc.call(ra(obj, "setDirection"), vec![inverse.clone()]);
        sc.raises(ra(obj, "setStyle"), vec![r(style)]);
        sc.eval(r(obj), &["getStyle", "getDirection", "__repr__"]);
    }
    for v in with_platforms(&[2, 9, 100, 1000000, INT_MAX], 4) {
        let style = sc.call(n("OCIO.CDLStyle"), vec![i(v)]);
        let obj = sc.call(n("OCIO.CDLTransform"), vec![]);
        sc.raises(ra(obj, "setStyle"), vec![r(style)]);
        sc.call(ra(obj, "setDirection"), vec![inverse.clone()]);
        sc.raises(ra(obj, "setStyle"), vec![r(style)]);
        sc.eval(r(obj), &["getStyle", "getDirection", "__repr__"]);
    }
    for v in with_platforms(&[3, 7, INT_MAX], 4) {
        let al = sc.call(n("OCIO.Allocation"), vec![i(v)]);
        sc.raises(n("OCIO.AllocationTransform"), vec![r(al)]);
        for vars in [
            &[1.0][..],
            &[0.0, 1.0],
            &[0.0, 1.0, 2.0],
            &[0.0, 1.0, 2.0, 3.0],
        ] {
            sc.call_kw(
                n("OCIO.AllocationTransform"),
                vec![r(al)],
                vec![("vars", fl(vars))],
                Expect::Raised,
            );
            sc.call_kw(
                n("OCIO.AllocationTransform"),
                vec![r(al)],
                vec![("vars", fl(vars)), ("direction", r(odd_dir))],
                Expect::Raised,
            );
        }
        sc.call_kw(
            n("OCIO.AllocationTransform"),
            vec![],
            vec![("allocation", r(al)), ("direction", r(odd_dir))],
            Expect::Raised,
        );
    }
    for v in with_platforms(&[5, 99, 253, 256, INT_MAX], 4) {
        let interp = sc.call(n("OCIO.Interpolation"), vec![i(v)]);
        sc.call_kw(
            n("OCIO.FileTransform"),
            vec![],
            vec![("interpolation", r(interp))],
            Expect::Raised,
        );
        sc.call_kw(
            n("OCIO.FileTransform"),
            vec![s("")],
            vec![("interpolation", r(interp)), ("direction", r(odd_dir))],
            Expect::Raised,
        );
        sc.call_kw(
            n("OCIO.FileTransform"),
            vec![s("a.clf"), s("id")],
            vec![("interpolation", r(interp)), ("direction", r(odd_dir))],
            Expect::Raised,
        );
    }
    let raised = sc.expect.iter().filter(|e| **e == Expect::Raised).count();
    assert_eq!(raised, 2 * 11 * 4 + 10 * 4 + 9 * 2 + 7 * 10 + 9 * 3);
    compare(&[sc]);
}

/// What the wheel can't survive is the port's error: a null child where upstream dereferences
/// it (U-110) and a group that holds itself where upstream recurses through it (U-111).
#[test]
fn null_children_and_cycles_are_the_ports_errors() {
    let mut sc = Script::new();
    let raised = |sc: &mut Script, target, args| sc.raises(target, args);
    let mut expect_u = Vec::new();
    expect_u.push((
        raised(&mut sc, n("OCIO.GroupTransform"), vec![l(vec![none()])]),
        "U-110",
    ));
    let m = sc.call(n("OCIO.MatrixTransform"), vec![]);
    let holes = sc.call(n("OCIO.GroupTransform"), vec![l(vec![r(m)])]);
    sc.call(ra(holes, "appendTransform"), vec![none()]);
    expect_u.push((raised(&mut sc, ra(holes, "validate"), vec![]), "U-110"));
    expect_u.push((raised(&mut sc, ra(holes, "__repr__"), vec![]), "U-110"));
    let outer = sc.call(n("OCIO.GroupTransform"), vec![]);
    sc.call(ra(outer, "appendTransform"), vec![r(holes)]);
    expect_u.push((raised(&mut sc, ra(outer, "validate"), vec![]), "U-110"));
    expect_u.push((raised(&mut sc, ra(outer, "__repr__"), vec![]), "U-110"));
    let cyc = sc.call(n("OCIO.GroupTransform"), vec![l(vec![r(m)])]);
    sc.call(ra(cyc, "appendTransform"), vec![r(cyc)]);
    expect_u.push((raised(&mut sc, ra(cyc, "validate"), vec![]), "U-111"));
    expect_u.push((raised(&mut sc, ra(cyc, "__repr__"), vec![]), "U-111"));
    for f in ["Color Transform Format", "academy/asc common lut format"] {
        expect_u.push((raised(&mut sc, ra(cyc, "write"), vec![s(f)]), "U-111"));
    }
    let deep = sc.call(n("OCIO.GroupTransform"), vec![]);
    sc.call(ra(deep, "appendTransform"), vec![r(outer)]);
    sc.call(ra(outer, "appendTransform"), vec![r(deep)]);
    expect_u.push((
        raised(
            &mut sc,
            ra(deep, "write"),
            vec![s("Color Transform Format")],
        ),
        "U-111",
    ));
    let ours = common::port(&sc.json());
    let steps = ours["steps"].as_array().unwrap();
    assert_eq!(steps.len(), sc.steps.len());
    for (k, step) in steps.iter().enumerate() {
        match expect_u.iter().find(|(i, _)| *i == k) {
            Some((_, u)) => {
                let message = String::from_utf8(ocio_testkit::oracle_values::bytes(
                    &step["raised"]["message"],
                ))
                .unwrap();
                assert!(message.contains(u), "step {k}: {step}");
                assert_eq!(step["raised"]["type"], "builtins.RuntimeError", "step {k}");
            }
            None => assert!(step.get("value").is_some(), "step {k}: {step}"),
        }
    }
    assert_eq!(expect_u.len(), 10);
}
