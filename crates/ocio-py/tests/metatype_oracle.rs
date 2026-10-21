// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! pybind11's metatype `pybind11_type` and descriptor `pybind11_static_property` against the
//! wheel's (p6-core re-check round 3), through the same step scripts on both sides. The wheel's
//! methods of both are the slot wrappers of their C slots; the port's are Python functions
//! (docs/deviations.md, D-7) that check their arguments as the slot wrappers do: a `self` of
//! the wrapper's class (by its type's MRO, named by its type's C name), no keyword arguments
//! but `__call__`'s, the slot's arguments (CPython's texts, with `PyArg_UnpackTuple`'s leading
//! space); with the wrappers' docstrings, `__text_signature__`, `__objclass__`, names. Then
//! what the slots do: the metatype's lookup clears an error hashing the name, the static
//! property gets with the owner (the third argument), sets and deletes on the class (the
//! instance when it is a type by its type's MRO, not by its `__class__`).
//!
//! `d7_lists_what_still_differs` checks each difference D-7 lists, against the wheel; and,
//! without the wheel (whose heap is corrupted, U-100), replacing or deleting an enum's `value`
//! property raises the port's error.
#![cfg(feature = "python")]

mod common;

use common::{call, name, port, text, wheel};
use pyo3::prelude::*;
use serde_json::{Value, json};

fn int(v: &str) -> Value {
    json!({ "int": v })
}

fn r(i: usize) -> Value {
    json!({ "ref": i })
}

fn attr(i: usize, a: &str) -> Value {
    json!({ "ref": i, "attr": a })
}

fn kwcall(target: Value, args: Value, kwargs: Value) -> Value {
    json!({ "call": target, "args": args, "kwargs": kwargs })
}

/// `type(name, bases, {"__module__": "probe", k: v})`. (`__module__` given: by default it is
/// the module that calls `type`, which the oracle runs as `__main__` and the port imports as
/// `py_eval_steps`.)
fn new_type(n: &str, bases: Vec<Value>, ns: Vec<(&str, Value)>) -> Value {
    let ns: Vec<Value> = std::iter::once(("__module__", text("probe")))
        .chain(ns)
        .map(|(k, v)| json!([text(k), v]))
        .collect();
    call(
        name("type"),
        json!([text(n), { "tuple": bases }, { "dict": ns }]),
    )
}

/// The steps, and the indices of those the wheel must refuse.
fn steps() -> (Vec<Value>, Vec<usize>) {
    let mut s: Vec<Value> = Vec::new();
    let mut refused = Vec::new();
    let push = |s: &mut Vec<Value>, step: Value| {
        s.push(step);
        s.len() - 1
    };
    let td = || name("OCIO.TransformDirection");
    let fwd = || name("OCIO.TRANSFORM_DIR_FORWARD");
    // The metatype, the static property `__doc__` and its type.
    let m = push(&mut s, call(name("type"), json!([td()])));
    let sp = push(
        &mut s,
        call(
            name("operator.getitem"),
            json!([name("OCIO.TransformDirection.__dict__"), text("__doc__")]),
        ),
    );
    let spt = push(&mut s, call(name("type"), json!([r(sp)])));
    // `property` (not among the step interpreter's names): the type of an enum's `value`.
    let value = push(
        &mut s,
        call(
            name("operator.getitem"),
            json!([name("OCIO.TransformDirection.__dict__"), text("value")]),
        ),
    );
    let property = push(&mut s, call(name("type"), json!([r(value)])));
    // Classes: a plain one; a str whose hash raises (`abs` called without an argument); a
    // metatype whose `__name__` lies, and a class of it; a class whose name is longer than
    // CPython writes; an instance whose `__class__` claims `type`.
    let p = push(&mut s, new_type("P", vec![], vec![]));
    let bad_hash = push(
        &mut s,
        new_type("S", vec![name("str")], vec![("__hash__", name("abs"))]),
    );
    let liar_meta = push(
        &mut s,
        new_type("N", vec![name("type")], vec![("__name__", text("zz"))]),
    );
    let lying_class = push(
        &mut s,
        call(
            r(liar_meta),
            json!([text("X"), { "tuple": [] }, { "dict": [[text("__module__"), text("probe")]] }]),
        ),
    );
    let long = push(&mut s, new_type(&"a".repeat(150), vec![], vec![]));
    let long_instance = push(&mut s, call(r(long), json!([])));
    let claims_type = push(
        &mut s,
        call(
            name("operator.attrgetter"),
            json!([text("__init_subclass__.__self__.__class__")]),
        ),
    );
    let class_property = push(&mut s, call(r(property), json!([r(claims_type)])));
    let liar = push(
        &mut s,
        new_type("Liar", vec![], vec![("__class__", r(class_property))]),
    );
    let liar_instance = push(&mut s, call(r(liar), json!([])));
    s.push(call(
        name("isinstance"),
        json!([r(liar_instance), name("type")]),
    ));
    let s_value = push(&mut s, call(r(bad_hash), json!([text("value")])));
    let s_init = push(&mut s, call(r(bad_hash), json!([text("__init__")])));
    let s_new = push(&mut s, call(r(bad_hash), json!([text("qq")])));
    let s_nope = push(&mut s, call(r(bad_hash), json!([text("nope")])));
    let plain_property = push(&mut s, call(r(property), json!([])));
    let empty = || json!({ "tuple": [] });
    let list1 = || json!({ "list": [int("1")] });

    // The slot wrappers: names, docstrings, signatures, classes.
    for (owner, names) in [
        (
            m,
            &["__call__", "__getattribute__", "__setattr__", "__delattr__"][..],
        ),
        (spt, &["__get__", "__set__", "__delete__"][..]),
    ] {
        for n in names {
            // (Each attribute alone: the wrapper itself is D-7's.)
            for a in [
                "__doc__",
                "__text_signature__",
                "__name__",
                "__qualname__",
                "__objclass__",
            ] {
                s.push(call(name("getattr"), json!([attr(owner, n), text(a)])));
            }
        }
    }
    // Refused calls, then accepted ones (`refusing` marks the steps the wheel must refuse).
    let mut refusing = |s: &mut Vec<Value>, step: Value| {
        s.push(step);
        refused.push(s.len() - 1);
    };
    let mc = || attr(m, "__call__");
    let mg = || attr(m, "__getattribute__");
    let ms = || attr(m, "__setattr__");
    let md = || attr(m, "__delattr__");
    for args in [
        json!([name("int"), int("5")]),
        json!([]),
        json!([int("3")]),
        json!([r(p)]),
        json!([{ "bool": true }]),
        json!([name("OCIO.GetVersion")]),
        json!([r(sp)]),
        json!([fwd()]),
        json!([r(m), text("x"), empty(), { "dict": [] }]),
        json!([r(lying_class)]),
        json!([r(long_instance)]),
        json!([r(liar_instance)]),
    ] {
        refusing(&mut s, call(mc(), args));
    }
    refusing(
        &mut s,
        kwcall(mc(), json!([td(), int("1")]), json!([["k", int("2")]])),
    );
    for args in [
        json!([r(p), text("x")]),
        json!([]),
        json!([td()]),
        json!([td(), text("value"), int("1")]),
        json!([td(), int("5")]),
        json!([td(), list1()]),
        json!([td(), r(s_value)]),
        json!([td(), r(s_init)]),
        json!([r(liar_instance), text("x")]),
    ] {
        refusing(&mut s, call(mg(), args));
    }
    refusing(
        &mut s,
        kwcall(mg(), json!([td()]), json!([["name", text("value")]])),
    );
    refusing(
        &mut s,
        kwcall(mg(), json!([td(), text("value")]), json!([["k", int("1")]])),
    );
    refusing(
        &mut s,
        kwcall(mg(), json!([int("3")]), json!([["k", int("1")]])),
    );
    for args in [
        json!([r(p), text("x"), int("1")]),
        json!([]),
        json!([td(), text("x")]),
        json!([td(), text("x"), int("1"), int("2")]),
        json!([td(), list1(), int("2")]),
        json!([td(), int("5"), int("2")]),
    ] {
        refusing(&mut s, call(ms(), args));
    }
    refusing(
        &mut s,
        kwcall(
            ms(),
            json!([td(), text("x"), int("1")]),
            json!([["k", int("2")]]),
        ),
    );
    for args in [
        json!([r(p), text("x")]),
        json!([]),
        json!([td()]),
        json!([td(), text("x"), text("y")]),
        json!([td(), text("zzz")]),
        json!([td(), list1()]),
        json!([td(), r(s_nope)]),
    ] {
        refusing(&mut s, call(md(), args));
    }
    refusing(
        &mut s,
        kwcall(md(), json!([td(), text("x")]), json!([["k", int("1")]])),
    );
    let sg = || attr(spt, "__get__");
    let ss = || attr(spt, "__set__");
    let sd = || attr(spt, "__delete__");
    for args in [
        json!([r(sp), { "none": null }, { "none": null }]),
        json!([r(sp)]),
        json!([r(sp), { "none": null }, td(), int("4")]),
        json!([]),
        json!([int("5"), td()]),
        json!([r(plain_property), td()]),
        json!([r(sp), td(), int("5")]),
        json!([r(liar_instance), { "none": null }, td()]),
    ] {
        refusing(&mut s, call(sg(), args));
    }
    refusing(
        &mut s,
        kwcall(sg(), json!([r(sp), td()]), json!([["k", int("1")]])),
    );
    for args in [
        json!([r(sp), td()]),
        json!([r(sp), td(), int("1"), int("2")]),
        json!([r(sp), td(), int("1")]),
        json!([int("3"), td(), int("1")]),
        json!([]),
        json!([r(sp), fwd(), int("1")]),
        json!([r(sp), { "none": null }, int("1")]),
        json!([r(sp), r(liar_instance), int("1")]),
    ] {
        refusing(&mut s, call(ss(), args));
    }
    refusing(
        &mut s,
        kwcall(
            ss(),
            json!([r(sp), td(), int("1")]),
            json!([["k", int("2")]]),
        ),
    );
    for args in [
        json!([r(sp)]),
        json!([r(sp), td()]),
        json!([r(sp), td(), int("2")]),
        json!([int("3"), int("4")]),
        json!([]),
        json!([r(sp), fwd()]),
        json!([r(sp), { "none": null }]),
        json!([r(sp), r(liar_instance)]),
    ] {
        refusing(&mut s, call(sd(), args));
    }
    refusing(
        &mut s,
        kwcall(sd(), json!([r(sp), td()]), json!([["k", int("2")]])),
    );
    // Accepted: the slots called directly, and through Python's syntax.
    s.push(call(mc(), json!([td(), int("1")])));
    s.push(kwcall(mc(), json!([td()]), json!([["value", int("1")]])));
    s.push(call(mg(), json!([td(), text("value")])));
    s.push(kwcall(mg(), json!([td(), text("value")]), json!([])));
    s.push(call(mg(), json!([td(), text("__init__")])));
    s.push(call(mg(), json!([td(), text("__repr__")])));
    s.push(call(ms(), json!([td(), r(s_new), int("1")])));
    s.push(json!({ "eval": name("OCIO.TransformDirection.qq") }));
    s.push(call(md(), json!([td(), text("qq")])));
    s.push(call(name("hasattr"), json!([td(), text("qq")])));
    s.push(call(sg(), json!([r(sp), fwd()])));
    s.push(call(sg(), json!([r(sp), { "none": null }, td()])));
    s.push(call(sg(), json!([r(sp), fwd(), td()])));
    s.push(call(
        name("getattr"),
        json!([name("OCIO.TRANSFORM_DIR_INVERSE"), text("__doc__")]),
    ));
    s.push(json!({ "eval": name("OCIO.TransformDirection.__members__") }));
    s.push(json!({ "setattr": td(), "name": "zz", "value": int("3") }));
    s.push(json!({ "eval": name("OCIO.TransformDirection.zz") }));
    s.push(json!({ "delattr": td(), "name": "zz" }));
    // A static property can't be set (no setter), but is replaced by another one, and
    // deleted, through the metatype.
    refusing(
        &mut s,
        json!({ "setattr": td(), "name": "__doc__", "value": text("x") }),
    );
    let members = push(
        &mut s,
        call(
            name("operator.getitem"),
            json!([name("OCIO.Interpolation.__dict__"), text("__members__")]),
        ),
    );
    s.push(json!({ "setattr": td(), "name": "__members__", "value": r(members) }));
    s.push(json!({ "eval": name("OCIO.TransformDirection.__members__") }));
    s.push(json!({ "delattr": td(), "name": "__doc__" }));
    s.push(json!({ "eval": name("OCIO.TransformDirection.__doc__") }));
    (s, refused)
}

/// The metatype's and the static property's methods as the wheel's slot wrappers.
#[test]
fn metatype_and_static_property_slots_as_the_wheels() {
    let (steps, refused) = steps();
    let script = json!({ "steps": steps });
    let ours = port(&script);
    let theirs = wheel(std::slice::from_ref(&script)).remove(0);
    let (ours, theirs) = (
        ours["steps"].as_array().unwrap(),
        theirs["steps"].as_array().unwrap(),
    );
    assert_eq!(ours.len(), steps.len());
    assert_eq!(theirs.len(), steps.len());
    for (i, (a, b)) in ours.iter().zip(theirs).enumerate() {
        assert_eq!(a, b, "step {i}: {}", script["steps"][i]);
    }
    // Not vacuous: the wheel refuses each refused step, and runs every other.
    assert_eq!(refused.len(), 68);
    for (i, result) in theirs.iter().enumerate() {
        assert_eq!(
            result.get("raised").is_some(),
            refused.contains(&i),
            "step {i}: {}: {result}",
            script["steps"][i]
        );
    }
}

/// The value of a step script's result `i` (its repr's text, or the exception's message), as
/// UTF-8.
fn shown(report: &Value, i: usize) -> String {
    let step = &report["steps"][i];
    let hex = step["value"]["repr"]["bytes"]
        .as_str()
        .or_else(|| step["raised"]["message"]["bytes"].as_str())
        .unwrap_or_else(|| panic!("step {i}: {step}"));
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|k| u8::from_str_radix(&hex[k..k + 2], 16).unwrap())
        .collect();
    String::from_utf8(bytes).unwrap()
}

/// Each difference docs/deviations.md's D-7 lists, as the wheel and the port show it: the
/// wheel's values are CPython's (slot wrappers, C names, pybind11's layout), the port's what D-7
/// says instead.
#[test]
fn d7_lists_what_still_differs() {
    let mut s: Vec<Value> = Vec::new();
    let m = 0;
    s.push(call(name("type"), json!([name("OCIO.TransformDirection")])));
    let sp = 1;
    s.push(call(
        name("operator.getitem"),
        json!([name("OCIO.TransformDirection.__dict__"), text("__doc__")]),
    ));
    let spt = 2;
    s.push(call(name("type"), json!([r(sp)])));
    // [step, the wheel's, the port's]
    let mut differ: Vec<(usize, &str, &str)> = Vec::new();
    let mut add = |s: &mut Vec<Value>, step: Value, theirs: &'static str, ours: &'static str| {
        s.push(step);
        differ.push((s.len() - 1, theirs, ours));
    };
    for n in ["__call__", "__getattribute__", "__setattr__", "__delattr__"] {
        let d = call(
            name("operator.getitem"),
            json!([attr(m, "__dict__"), text(n)]),
        );
        s.push(d);
        let i = s.len() - 1;
        add(
            &mut s,
            call(name("type"), json!([r(i)])),
            "<class 'wrapper_descriptor'>",
            "<class 'function'>",
        );
    }
    for n in ["__get__", "__set__", "__delete__"] {
        s.push(call(
            name("operator.getitem"),
            json!([attr(spt, "__dict__"), text(n)]),
        ));
        let i = s.len() - 1;
        add(
            &mut s,
            call(name("type"), json!([r(i)])),
            "<class 'wrapper_descriptor'>",
            "<class 'function'>",
        );
    }
    add(
        &mut s,
        call(
            name("type"),
            json!([name("OCIO.TransformDirection.__call__")]),
        ),
        "<class 'method-wrapper'>",
        "<class 'method'>",
    );
    add(
        &mut s,
        call(
            name("hasattr"),
            json!([attr(m, "__call__"), text("__module__")]),
        ),
        "False",
        "True",
    );
    add(
        &mut s,
        call(
            name("hasattr"),
            json!([attr(m, "__call__"), text("__code__")]),
        ),
        "False",
        "True",
    );
    // type's own __setattr__ and __delattr__ on a class of the module: CPython's check that
    // no C slot is jumped over refuses them in the wheel.
    add(
        &mut s,
        call(
            name("type.__setattr__"),
            json!([name("OCIO.TransformDirection"), text("x"), int("1")]),
        ),
        "can't apply this __setattr__ to pybind11_type object",
        "None",
    );
    add(
        &mut s,
        call(
            name("type.__delattr__"),
            json!([name("OCIO.TransformDirection"), text("x")]),
        ),
        "can't apply this __delattr__ to pybind11_type object",
        "None",
    );
    // pybind11_object: its metatype, its C name.
    add(
        &mut s,
        call(
            name("type"),
            json!([name("OCIO.TransformDirection.__base__")]),
        ),
        "<class 'pybind11_builtins.pybind11_type'>",
        "<class 'type'>",
    );
    add(
        &mut s,
        call(
            name("OCIO.TransformDirection.__new__"),
            json!([name("int")]),
        ),
        "pybind11_object.__new__(int): int is not a subtype of pybind11_object",
        "pybind11_builtins.pybind11_object.__new__(int): int is not a subtype of \
         pybind11_builtins.pybind11_object",
    );
    add(
        &mut s,
        call(
            name("repr"),
            json!([name("OCIO.TransformDirection.__base__.__init__")]),
        ),
        "\"<slot wrapper '__init__' of 'pybind11_object' objects>\"",
        "\"<slot wrapper '__init__' of 'pybind11_builtins.pybind11_object' objects>\"",
    );
    add(
        &mut s,
        call(
            name("OCIO.TransformDirection.__base__.__init__"),
            json!([name("object")]),
        ),
        "descriptor '__init__' requires a 'pybind11_object' object but received a 'type'",
        "descriptor '__init__' requires a 'pybind11_builtins.pybind11_object' object but \
         received a 'type'",
    );
    // CPython's layout check of a `__bases__` assignment names the C name too (re-check round
    // 4). It fails in both, so the class is left as it was.
    add(
        &mut s,
        json!({
            "setattr": name("OCIO.TransformDirection"),
            "name": "__bases__",
            "value": { "tuple": [name("object")] },
        }),
        "__bases__ assignment: 'object' object layout differs from 'pybind11_object'",
        "__bases__ assignment: 'object' object layout differs from \
         'pybind11_builtins.pybind11_object'",
    );
    // The weak reference offset, of the module's classes and Python classes deriving from
    // them (with `__slots__`, with several bases).
    add(
        &mut s,
        json!({ "eval": name("OCIO.TransformDirection.__weakrefoffset__") }),
        "40",
        "48",
    );
    let slots = vec![json!([text("__slots__"), { "tuple": [text("a")] }])];
    for (n, bases, ns) in [
        ("Q", vec![name("OCIO.TransformDirection")], slots),
        (
            "R",
            vec![name("OCIO.TransformDirection"), name("OCIO.Interpolation")],
            vec![],
        ),
    ] {
        s.push(call(
            name("type"),
            json!([text(n), { "tuple": bases }, { "dict": ns }]),
        ));
        let i = s.len() - 1;
        add(
            &mut s,
            json!({ "eval": attr(i, "__weakrefoffset__") }),
            "40",
            "48",
        );
    }
    let script = json!({ "steps": s });
    let ours = port(&script);
    let theirs = wheel(std::slice::from_ref(&script)).remove(0);
    assert_eq!(differ.len(), 20);
    for (i, w, p) in differ {
        assert_eq!(
            shown(&theirs, i),
            w,
            "wheel, step {i}: {}",
            script["steps"][i]
        );
        assert_eq!(shown(&ours, i), p, "port, step {i}: {}", script["steps"][i]);
    }
}

/// Replacing or deleting an enum's `value` property, through its metatype, raises the port's
/// error (U-100); a Python subclass's own `value`, and the property itself put back, don't.
#[test]
fn replacing_an_enum_value_property_raises_the_ports_error() {
    common::start_python();
    Python::attach(|py| {
        let ocio = py.import("PyOpenColorIO").unwrap();
        let td = ocio.getattr("TransformDirection").unwrap();
        let interp = ocio.getattr("Interpolation").unwrap();
        let metatype = td.get_type();
        let builtins = py.import("builtins").unwrap();
        let property = td.getattr("__dict__").unwrap().get_item("value").unwrap();
        let other = interp
            .getattr("__dict__")
            .unwrap()
            .get_item("value")
            .unwrap();
        let error = |c: &str| {
            format!(
                "PyOpenColorIO.PyOpenColorIO.{c}: replacing or deleting an enum's `value` \
                 property frees its getter, which corrupts the wheel's heap \
                 (docs/improvements.md, U-100)"
            )
        };
        let s_value = builtins
            .getattr("type")
            .unwrap()
            .call1((
                "S",
                (builtins.getattr("str").unwrap(),),
                pyo3::types::PyDict::new(py),
            ))
            .unwrap()
            .call1(("value",))
            .unwrap();
        for (attempt, c) in [
            (td.setattr("value", 3), "TransformDirection"),
            (td.delattr("value"), "TransformDirection"),
            (td.setattr("value", &other), "TransformDirection"),
            (
                metatype
                    .getattr("__setattr__")
                    .unwrap()
                    .call1((&td, "value", 3))
                    .map(|_| ()),
                "TransformDirection",
            ),
            (
                metatype
                    .getattr("__delattr__")
                    .unwrap()
                    .call1((&td, "value"))
                    .map(|_| ()),
                "TransformDirection",
            ),
            (
                builtins
                    .getattr("setattr")
                    .unwrap()
                    .call1((&td, &s_value, 3))
                    .map(|_| ()),
                "TransformDirection",
            ),
            (interp.setattr("value", 3), "Interpolation"),
            (
                ocio.getattr("BitDepth").unwrap().delattr("value"),
                "BitDepth",
            ),
        ] {
            let e = attempt.unwrap_err();
            assert!(
                e.is_instance_of::<pyo3::exceptions::PyRuntimeError>(py),
                "{e}"
            );
            assert_eq!(e.value(py).to_string(), error(c));
        }
        // Still there, and still the getter.
        assert!(
            td.getattr("__dict__")
                .unwrap()
                .get_item("value")
                .unwrap()
                .is(&property)
        );
        let one = td.call1((1,)).unwrap();
        assert_eq!(one.getattr("value").unwrap().extract::<i64>().unwrap(), 1);
        // The property itself put back; another attribute; a Python subclass's own `value`.
        td.setattr("value", &property).unwrap();
        td.setattr("other", &property).unwrap();
        td.delattr("other").unwrap();
        let sub = builtins
            .getattr("type")
            .unwrap()
            .call1(("Q", (&td,), pyo3::types::PyDict::new(py)))
            .unwrap();
        sub.setattr("value", 3).unwrap();
        assert_eq!(sub.getattr("value").unwrap().extract::<i64>().unwrap(), 3);
        sub.delattr("value").unwrap();
        assert!(sub.getattr("value").unwrap().is(&property));
    });
}
