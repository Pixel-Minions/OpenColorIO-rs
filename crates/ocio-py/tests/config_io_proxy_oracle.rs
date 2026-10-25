// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `PyConfigIOProxy` (chunk 6.6a) against the wheel, called from Python: the class's surface
//! (py_types_oracle.rs compares it member for member) and its behaviour through the same step
//! scripts on both sides:
//! - the base class's pure virtual functions (pybind11's RuntimeError);
//! - Python subclasses made with `type()` (the step interpreter defines no functions, so the
//!   overrides are callables of the builtins: `str`, `bytes`, `list`, the vector class, ...),
//!   whose overrides the base class's functions call (`PYBIND11_OVERRIDE_PURE`): their
//!   arguments (text, bytes that aren't UTF-8, `None`), their results cast to the C++ types
//!   (a `str`, `bytes` or `bytearray` for text; a vector, or a `list` or `bytearray` converted,
//!   for the LUT data; pybind11's `cast_error` otherwise), their errors passed through;
//! - pybind11's lookup of an override: an attribute that is a C function (`len`) counts as none,
//!   and the type is then remembered as having none (a later override on the type isn't
//!   seen, one on a type made after is), an instance's own attribute, an attribute Python
//!   can't call (pybind11's TypeError);
//! - the methods' refusals (another class's instance, keywords on unnamed parameters).
#![cfg(feature = "python")]

mod common;

use common::script::{Expect, Script, by, compare, d, i, n, none, r, ra, s, t};
use serde_json::Value;

/// A subclass of `PyConfigIOProxy` named `name`, its class attributes `attrs`, and an instance
/// of it: their steps.
fn subclass(sc: &mut Script, name: &str, attrs: Vec<(&str, Value)>) -> (usize, usize) {
    let mut items = vec![(s("__module__"), s("probe"))];
    items.extend(attrs.into_iter().map(|(k, v)| (s(k), v)));
    let cls = sc.call(
        n("type"),
        vec![s(name), t(vec![n("OCIO.PyConfigIOProxy")]), d(items)],
    );
    let obj = sc.call(r(cls), vec![]);
    (cls, obj)
}

/// The base class's function `f` on `obj`, with `args`.
fn base(f: &str) -> Value {
    n(&format!("OCIO.PyConfigIOProxy.{f}"))
}

fn proxy_script() -> Script {
    let mut sc = Script::new();
    // The base class: every function pure virtual.
    let p = sc.call(n("OCIO.PyConfigIOProxy"), vec![]);
    sc.raises(ra(p, "getConfigData"), vec![]);
    sc.raises(ra(p, "getLutData"), vec![s("a")]);
    sc.raises(ra(p, "getFastLutFileHash"), vec![s("a")]);
    sc.call(n("isinstance"), vec![r(p), n("OCIO.PyConfigIOProxy")]);
    // Overrides of each function, through the base class's function and directly.
    let (_, a) = subclass(
        &mut sc,
        "A",
        vec![
            ("getConfigData", n("str")),
            ("getFastLutFileHash", n("str")),
            ("getLutData", n("list")),
        ],
    );
    sc.call(base("getConfigData"), vec![r(a)]);
    sc.call(ra(a, "getConfigData"), vec![]);
    for arg in [s("abc"), s("caf\u{e9}"), s("a\u{0}b"), none()] {
        sc.call(base("getFastLutFileHash"), vec![r(a), arg]);
    }
    sc.raises(base("getFastLutFileHash"), vec![r(a), by(b"\xff")]);
    sc.call(base("getFastLutFileHash"), vec![r(a), by(b"bytes")]);
    let empty = sc.call(base("getLutData"), vec![r(a), s("")]);
    sc.call(n("type"), vec![r(empty)]);
    sc.call(n("list"), vec![r(empty)]);
    sc.raises(base("getLutData"), vec![r(a), s("ab")]);
    // The vector class itself, `bytearray` (whose error goes through), `bytes` (no conversion).
    let (_, b) = subclass(
        &mut sc,
        "B",
        vec![
            ("getLutData", n("OCIO.vector_of_uint8_t")),
            ("getConfigData", n("bytes")),
            ("getFastLutFileHash", n("bytearray")),
        ],
    );
    let v = sc.call(base("getLutData"), vec![r(b), s("")]);
    sc.call(n("list"), vec![r(v)]);
    sc.raises(base("getLutData"), vec![r(b), s("x")]);
    sc.call(base("getConfigData"), vec![r(b)]);
    sc.raises(base("getFastLutFileHash"), vec![r(b), s("x")]);
    let (_, c) = subclass(
        &mut sc,
        "C",
        vec![
            ("getLutData", n("bytearray")),
            ("getConfigData", n("bytearray")),
            ("getFastLutFileHash", n("bytes")),
        ],
    );
    sc.raises(base("getLutData"), vec![r(c), s("x")]);
    sc.call(base("getConfigData"), vec![r(c)]);
    sc.raises(base("getFastLutFileHash"), vec![r(c), s("x")]);
    // Results the casters refuse: an int, a list for text, a tuple and bytes for the data.
    let (_, e) = subclass(
        &mut sc,
        "E",
        vec![
            ("getConfigData", n("int")),
            ("getFastLutFileHash", n("list")),
            ("getLutData", n("tuple")),
        ],
    );
    sc.raises(base("getConfigData"), vec![r(e)]);
    sc.raises(base("getFastLutFileHash"), vec![r(e), s("x")]);
    sc.raises(base("getLutData"), vec![r(e), s("")]);
    let (_, e2) = subclass(&mut sc, "E2", vec![("getLutData", n("bytes"))]);
    sc.raises(base("getLutData"), vec![r(e2), s("")]);
    // A C function counts as no override, and the type is remembered: an override set on the
    // type later isn't seen through the base class (Python's own lookup sees it).
    let (f_cls, f) = subclass(&mut sc, "F", vec![("getConfigData", n("len"))]);
    sc.raises(base("getConfigData"), vec![r(f)]);
    let (g_cls, g) = subclass(&mut sc, "G", vec![]);
    sc.raises(base("getConfigData"), vec![r(g)]);
    sc.call(n("setattr"), vec![r(g_cls), s("getConfigData"), n("str")]);
    sc.raises(base("getConfigData"), vec![r(g)]);
    sc.call(ra(g, "getConfigData"), vec![]);
    sc.call(n("setattr"), vec![r(f_cls), s("getConfigData"), n("str")]);
    sc.raises(base("getConfigData"), vec![r(f)]);
    // A type whose override is set before the first call; an instance's own attribute.
    let (h_cls, h) = subclass(&mut sc, "H", vec![]);
    sc.call(n("setattr"), vec![r(h_cls), s("getConfigData"), n("str")]);
    sc.call(base("getConfigData"), vec![r(h)]);
    let (_, k) = subclass(&mut sc, "K", vec![]);
    sc.call(n("setattr"), vec![r(k), s("getFastLutFileHash"), n("str")]);
    sc.call(base("getFastLutFileHash"), vec![r(k), s("own")]);
    sc.raises(base("getConfigData"), vec![r(k)]);
    // An attribute whose lookup on the instance raises (a descriptor of another class): the
    // error cleared, no override.
    let (_, q) = subclass(&mut sc, "Q", vec![("getConfigData", n("str.upper"))]);
    sc.raises(base("getConfigData"), vec![r(q)]);
    sc.raises(ra(q, "getConfigData"), vec![]);
    // An attribute Python can't call.
    let (_, m) = subclass(&mut sc, "M", vec![("getConfigData", i(5))]);
    sc.raises(base("getConfigData"), vec![r(m)]);
    let (_, m2) = subclass(&mut sc, "M2", vec![("getLutData", s("text"))]);
    sc.raises(base("getLutData"), vec![r(m2), s("")]);
    // Refused calls.
    sc.raises(base("getConfigData"), vec![i(5)]);
    sc.raises(base("getLutData"), vec![r(a)]);
    sc.raises(base("getLutData"), vec![r(a), i(1)]);
    sc.call_kw(
        base("getLutData"),
        vec![r(a)],
        vec![("arg0", s("x"))],
        Expect::Raised,
    );
    sc.raises(n("OCIO.PyConfigIOProxy"), vec![i(1)]);
    sc
}

/// The class behaves as the wheel's.
#[test]
fn the_class_behaves_as_the_wheels() {
    compare(&[proxy_script()]);
}
