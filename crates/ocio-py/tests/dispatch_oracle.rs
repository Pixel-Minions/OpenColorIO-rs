// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! pybind11's dispatch and casters (chunk 6.4a), against the wheel:
//! - the module functions bound so far, called with valid and invalid arguments (positional,
//!   keyword, missing, extra, of the wrong types, `None`), report what the wheel's report: the
//!   same values, the same exceptions with the same `TypeError` texts, the same `__doc__` and
//!   `__text_signature__`;
//! - each caster accepts exactly the values the wheel accepts for a parameter of its C++ type
//!   (a wheel function with one such parameter, refusing with "incompatible function
//!   arguments"), over values of every kind: booleans, integers in and out of range, floats,
//!   NumPy scalars and arrays, strings, bytes, `None`, sequences, sets, iterators;
//! - an overloaded call tries each overload without conversions first, then again with them:
//!   a function of the dispatcher with the overloads `(int)` and `(Sequence[float])` chooses,
//!   for each value, the overload the wheel's `GradingBSplineCurve(value)` does (the first and
//!   fourth of its five; the others take two arguments). The class chunks keep this as their
//!   first check of the dispatcher;
//! - the container casters (`std::vector<float>` through that function, `std::array<double,
//!   4>` through `MatrixTransform.setOffset`) read sequences and iterables as the wheel's: the
//!   same `len()` and item reads in the same order, stopping at the first item refused, the
//!   errors those reads and the C++ runtime's reservation raise (I-236), generators, `map`,
//!   `zip`, views and sets taken by their types.
#![cfg(feature = "python")]

mod common;

use common::{call, name, port, port_object, text as s, wheel};
use ocio_py::dispatch::{CULong, Cast, CharPtr, StdString};
use pyo3::prelude::*;
use serde_json::{Value, json};

fn int(v: &str) -> Value {
    json!({ "int": v })
}

/// Calls of the module's functions, each a step of one script.
fn calls() -> Vec<Value> {
    let functions = [
        "ClearAllCaches",
        "GetVersion",
        "GetVersionHex",
        "GetEnvVariable",
        "SetEnvVariable",
        "UnsetEnvVariable",
        "IsEnvVariablePresent",
    ];
    let mut steps = Vec::new();
    for f in functions {
        steps.push(json!({ "eval": name(&format!("OCIO.{f}.__doc__")) }));
        steps.push(json!({ "eval": name(&format!("OCIO.{f}.__text_signature__")) }));
        steps.push(json!({ "eval": name(&format!("OCIO.{f}.__name__")) }));
        // No arguments, one too many, an unknown keyword.
        steps.push(call(name(&format!("OCIO.{f}")), json!([])));
        steps.push(call(
            name(&format!("OCIO.{f}")),
            json!([s("A"), s("B"), s("C")]),
        ));
        steps.push(json!({ "call": name(&format!("OCIO.{f}")), "kwargs": [["nme", s("x")]] }));
    }
    let var = || s("OCIO_RS_DISPATCH_PROBE");
    let set = |args: Value, kwargs: Value| json!({ "call": name("OCIO.SetEnvVariable"), "args": args, "kwargs": kwargs });
    let get = || call(name("OCIO.GetEnvVariable"), json!([var()]));
    steps.extend([
        set(json!([var()]), json!([["value", s("by keyword")]])),
        get(),
        // An empty name (None, "", one that starts with a NUL) gives the value the last call
        // gave (I-224).
        call(name("OCIO.GetEnvVariable"), json!([{ "none": null }])),
        call(name("OCIO.GetEnvVariable"), json!([s("")])),
        call(name("OCIO.GetEnvVariable"), json!([{ "str": "0078" }])),
        set(json!([]), json!([["value", s("both")], ["name", var()]])),
        get(),
        set(json!([var(), { "none": null }]), json!([])),
        get(),
        call(name("OCIO.IsEnvVariablePresent"), json!([var()])),
        set(json!([var(), { "bytes": "6279746573" }]), json!([])),
        get(),
        set(json!([var(), { "str": "61006263" }]), json!([])),
        get(),
        set(json!([var(), { "str": "eda080" }]), json!([])),
        set(json!([var(), int("1")]), json!([])),
        set(json!([var()]), json!([["value", int("1")]])),
        set(json!([var(), s("x")]), json!([["name", var()]])),
        set(
            json!([int("1"), int("2"), int("3")]),
            json!([["x", int("4")], ["y", s("z")]]),
        ),
        set(json!([{ "none": null }, s("x")]), json!([])),
        call(name("OCIO.IsEnvVariablePresent"), json!([{ "none": null }])),
        call(name("OCIO.UnsetEnvVariable"), json!([{ "none": null }])),
        call(name("OCIO.UnsetEnvVariable"), json!([var()])),
        call(name("OCIO.IsEnvVariablePresent"), json!([var()])),
        call(name("OCIO.GetVersion"), json!([])),
        call(name("OCIO.GetVersionHex"), json!([])),
        call(name("OCIO.ClearAllCaches"), json!([])),
        json!({ "call": name("OCIO.GetVersion"), "kwargs": [["a", int("1")]] }),
    ]);
    // A bytearray, made by a step of its own.
    steps.push(call(name("bytearray"), json!([{ "bytes": "6261" }])));
    let made = steps.len() - 1;
    steps.push(set(json!([var(), { "ref": made }]), json!([])));
    steps.push(get());
    // Arguments whose repr() is a lone surrogate, holds a NUL, or raises, in "Invoked with"
    // (positional, keyword, a constructor's); a keyword name with a NUL (verifier re-check 5).
    for repr in [
        ev(json!({ "str": "78eda080" })),
        ev(json!({ "str": "610062" })),
        ev(json!({ "list": [] })),
    ] {
        let method = if repr["eval"].get("list").is_some() {
            "pop"
        } else {
            "__str__"
        };
        let base = steps.len();
        for step in instance(
            vec![repr],
            "R",
            "object",
            vec![("__repr__", attr(0, method))],
            json!([]),
        ) {
            steps.push(shift_refs(&step, base));
        }
        let r = json!({ "ref": steps.len() - 1 });
        steps.push(call(name("OCIO.GetEnvVariable"), json!([r])));
        steps.push(json!({ "call": name("OCIO.GetEnvVariable"), "args": [int("1")], "kwargs": [["x", r]] }));
        steps.push(call(name("OCIO.TransformDirection"), json!([r])));
    }
    steps.push(json!({ "call": name("OCIO.GetEnvVariable"), "kwargs": [["a\u{0}b", int("1")]] }));
    steps.push(
        json!({ "call": name("OCIO.TransformDirection"), "kwargs": [["a\u{0}b", int("1")]] }),
    );
    steps
}

/// The module functions bound so far answer every call as the wheel's: values, exceptions and
/// their texts, docstrings, signatures.
#[test]
fn module_functions_answer_as_the_wheels() {
    let script = json!({ "steps": calls() });
    let ours = port(&script);
    let theirs = wheel(std::slice::from_ref(&script)).remove(0);
    let (ours, theirs) = (&ours["steps"], &theirs["steps"]);
    for (i, (a, b)) in ours
        .as_array()
        .unwrap()
        .iter()
        .zip(theirs.as_array().unwrap())
        .enumerate()
    {
        assert_eq!(a, b, "step {i}: {}", script["steps"][i]);
    }
    assert_eq!(
        ours.as_array().unwrap().len(),
        theirs.as_array().unwrap().len()
    );
}

/// Values of every kind, each as steps of the step interpreter whose last result is the value.
fn values() -> Vec<Vec<Value>> {
    let f = |bits: u64| json!({ "float": format!("0x{bits:016x}") });
    let list = |items: Vec<Value>| json!({ "list": items });
    let np = |t: &str, v: Value| vec![call(name(&format!("numpy.{t}")), json!([v]))];
    let one = |v: Value| vec![json!({ "eval": v })];
    let four = || list(vec![int("1"), int("2"), int("3"), int("4")]);
    vec![
        one(json!({ "bool": true })),
        one(json!({ "bool": false })),
        one(int("0")),
        one(int("1")),
        one(int("-1")),
        one(int("2147483647")),
        one(int("2147483648")),
        one(int("-2147483649")),
        one(int("4294967295")),
        one(int("4294967296")),
        one(int("9223372036854775807")),
        one(int("9223372036854775808")),
        one(int("18446744073709551616")),
        one(f(1.0f64.to_bits())),
        one(f(1.5f64.to_bits())),
        one(f(f64::INFINITY.to_bits())),
        np("int64", int("3")),
        np("int32", int("-3")),
        np("uint8", int("3")),
        np("float32", f(1.5f64.to_bits())),
        np("float64", f(1.5f64.to_bits())),
        np("bool_", json!({ "bool": true })),
        one(s("1")),
        one(s("")),
        one(json!({ "bytes": "78" })),
        // Four bytes: a sequence of four ints, which pybind11's sequence casters refuse as
        // bytes (verifier re-check round 3, C6).
        one(json!({ "bytes": "61626364" })),
        vec![call(name("bytearray"), json!([{ "bytes": "78" }]))],
        one(json!({ "none": null })),
        vec![call(name("complex"), json!([int("1"), int("2")]))],
        one(four()),
        one(json!({ "tuple": [int("1"), int("2"), int("3"), int("4")] })),
        one(list(vec![
            f(1.0f64.to_bits()),
            int("2"),
            json!({ "bool": true }),
            int("4"),
        ])),
        one(list(vec![int("1"), int("2"), int("3")])),
        one(list(vec![int("1"), int("2"), int("3"), s("4")])),
        one(list(vec![four(), four(), four(), four()])),
        one(s("abcd")),
        vec![call(name("set"), json!([four()]))],
        vec![call(name("frozenset"), json!([four()]))],
        vec![call(name("range"), json!([int("4")]))],
        vec![call(name("iter"), json!([four()]))],
        vec![
            json!({ "eval": { "dict": [[int("1"), int("1")], [int("2"), int("2")], [int("3"), int("3")], [int("4"), int("4")]] } }),
        ],
        vec![
            json!({ "eval": { "dict": [[int("1"), int("1")], [int("2"), int("2")], [int("3"), int("3")], [int("4"), int("4")]] } }),
            call(json!({ "ref": 0, "attr": "keys" }), json!([])),
        ],
        vec![call(name("bytearray"), json!([int("4")]))],
        vec![call(name("numpy.array"), json!([four()]))],
        // A class named "generator" (PyGen_Check is a type check), one named "map" (pybind11
        // compares that name), each without and with items.
        named_instance("generator", None),
        named_instance("map", None),
        named_instance("generator", Some(four())),
        named_instance("map", Some(four())),
        // A mappingproxy: __getitem__ without a sequence slot.
        one(name("int.__dict__")),
        one(list(vec![s("a"), s("b"), s("c"), s("d")])),
        vec![call(name("set"), json!([list(vec![s("a")])]))],
        vec![call(name("zip"), json!([four()]))],
        vec![call(name("reversed"), json!([four()]))],
        vec![call(
            name("numpy.zeros"),
            json!([json!({ "tuple": [int("2"), int("2")] })]),
        )],
        // The type's own number slots (re-check round 4): `nb_bool` for the bool caster,
        // `nb_index`, `nb_int` and `nb_float` for PyNumber_Check, which an attribute lookup
        // can find on the metaclass, or on a static type's base without the slot (numpy.bytes_
        // and numpy.str_ share bytes' and str's number methods, generic's `__bool__`,
        // `__int__` and `__float__` in their MRO); a Python subclass of them has the slots.
        metaclass_instance("__bool__"),
        metaclass_instance("__index__"),
        metaclass_instance("__int__"),
        metaclass_instance("__float__"),
        vec![call(name("numpy.bytes_"), json!([{ "bytes": "61626364" }]))],
        vec![call(name("numpy.bytes_"), json!([{ "bytes": "3132" }]))],
        vec![call(name("numpy.str_"), json!([s("12")]))],
        vec![call(name("numpy.str_"), json!([s("x")]))],
        subclass_instance("numpy.bytes_", vec![], json!({ "bytes": "3132" })),
        subclass_instance("numpy.str_", vec![], s("12")),
        vec![call(name("numpy.void"), json!([{ "bytes": "6162" }]))],
        vec![call(name("numpy.datetime64"), json!([s("2020-01-01")]))],
        vec![call(name("numpy.timedelta64"), json!([int("3")]))],
        vec![call(name("numpy.complex64"), json!([int("1")]))],
        vec![call(name("numpy.float16"), json!([f(1.5f64.to_bits())]))],
        vec![call(name("numpy.array"), json!([f(1.5f64.to_bits())]))],
        // Classes with the slots, from a callable that isn't a function (called without
        // `self`), or `None`.
        subclass_instance("object", vec![[s("__bool__"), name("bool")]], json!(null)),
        subclass_instance(
            "object",
            vec![[s("__bool__"), json!({ "none": null })]],
            json!(null),
        ),
        subclass_instance("object", vec![[s("__index__"), name("int")]], json!(null)),
        subclass_instance("object", vec![[s("__int__"), name("int")]], json!(null)),
        subclass_instance("object", vec![[s("__float__"), name("float")]], json!(null)),
        subclass_instance("object", vec![[s("__len__"), name("int")]], json!(null)),
        // `nb_float` alone makes PyNumber_Check true, and then `int(obj)` parses a `str` or
        // `bytes` subclass's text.
        subclass_instance("str", vec![[s("__float__"), name("float")]], s("12")),
        subclass_instance(
            "bytes",
            vec![[s("__float__"), name("float")]],
            json!({ "bytes": "3132" }),
        ),
        // Built-in containers and their subclasses (the verifier's re-check round 4).
        vec![call(name("bytearray"), json!([{ "bytes": "61626364" }]))],
        vec![call(name("bytes"), json!([four()]))],
        vec![
            call(name("numpy.array"), json!([four()])),
            call(json!({ "ref": 0, "attr": "tolist" }), json!([])),
        ],
        subclass_instance("bytes", vec![], json!({ "bytes": "61626364" })),
        subclass_instance("bytes", vec![], json!({ "bytes": "3132" })),
        subclass_instance("str", vec![], s("abcd")),
        subclass_instance("str", vec![], s("12")),
        subclass_instance("tuple", vec![], four()),
        subclass_instance(
            "dict",
            vec![],
            json!({ "dict": [[int("1"), int("1")], [int("2"), int("2")], [int("3"), int("3")], [int("4"), int("4")]] }),
        ),
        subclass_instance("int", vec![], int("1")),
        subclass_instance("float", vec![], f(1.5f64.to_bits())),
        subclass_instance("complex", vec![], int("1")),
        subclass_instance("bytearray", vec![], int("4")),
    ]
}

/// An instance of a class made by a metaclass (a `type` subclass) that has the special method
/// `slot` (an int, never called): the class itself has no such slot.
fn metaclass_instance(slot: &str) -> Vec<Value> {
    vec![
        call(
            name("type"),
            json!([s("M"), { "tuple": [name("type")] }, { "dict": [[s(slot), int("0")]] }]),
        ),
        call(
            json!({ "ref": 0 }),
            json!([s("C"), { "tuple": [] }, { "dict": [[s("__module__"), s("probe")]] }]),
        ),
        call(json!({ "ref": 1 }), json!([])),
    ]
}

/// An instance of a Python subclass of `base` (named by `name`), with the class attributes
/// `ns`, made from `arg` (none for null).
fn subclass_instance(base: &str, ns: Vec<[Value; 2]>, arg: Value) -> Vec<Value> {
    let mut ns: Vec<Value> = ns.into_iter().map(|kv| json!(kv)).collect();
    ns.push(json!([s("__module__"), s("probe")]));
    let args = if arg.is_null() {
        json!([])
    } else {
        json!([arg])
    };
    vec![
        call(
            name("type"),
            json!([s("X"), { "tuple": [name(base)] }, { "dict": ns }]),
        ),
        call(json!({ "ref": 0 }), args),
    ]
}

/// An instance of a class named `n` (an `object`, or a `list` of `items`).
fn named_instance(n: &str, items: Option<Value>) -> Vec<Value> {
    let base = if items.is_some() { "list" } else { "object" };
    vec![
        call(
            name("type"),
            json!([s(n), { "tuple": [name(base)] }, { "dict": [[s("__module__"), s("probe")]] }]),
        ),
        call(
            json!({ "ref": 0 }),
            json!(items.into_iter().collect::<Vec<_>>()),
        ),
    ]
}

/// For each value, whether the wheel's `target` (a callable, made by `setup`) refuses it as an
/// argument ("incompatible function arguments"), or accepts it (whatever the call does next).
fn wheel_accepts(setup: Value, target: Value, before: Vec<Value>, after: Vec<Value>) -> Vec<bool> {
    let values = values();
    let mut steps = vec![setup];
    let mut calls = Vec::new();
    for v in &values {
        let base = steps.len();
        for step in v {
            steps.push(shift_refs(step, base));
        }
        let mut args = before.clone();
        args.push(json!({ "ref": steps.len() - 1 }));
        args.extend(after.clone());
        calls.push(steps.len());
        steps.push(json!({ "call": target.clone(), "args": args }));
    }
    let report = wheel(&[json!({ "steps": steps })]).remove(0);
    calls
        .into_iter()
        .map(|i| {
            let step = &report["steps"][i];
            let refused = step["raised"]["type"] == "builtins.TypeError"
                && String::from_utf8(ocio_testkit::oracle_values::bytes(
                    &step["raised"]["message"],
                ))
                .unwrap()
                .contains("incompatible function arguments");
            !refused
        })
        .collect()
}

/// For each value, whether the wheel's `target` refuses it as the keyword argument `keyword`
/// ("incompatible ... arguments"), or accepts it.
fn wheel_accepts_keyword(target: Value, keyword: &str) -> Vec<bool> {
    let values = values();
    let mut steps = vec![];
    let mut calls = Vec::new();
    for v in &values {
        let base = steps.len();
        for step in v {
            steps.push(shift_refs(step, base));
        }
        calls.push(steps.len());
        steps.push(
            json!({ "call": target.clone(), "kwargs": [[keyword, { "ref": steps.len() - 1 }]] }),
        );
    }
    let report = wheel(&[json!({ "steps": steps })]).remove(0);
    calls
        .into_iter()
        .map(|i| {
            let step = &report["steps"][i];
            let refused = step["raised"]["type"] == "builtins.TypeError"
                && String::from_utf8(ocio_testkit::oracle_values::bytes(
                    &step["raised"]["message"],
                ))
                .unwrap()
                .contains("incompatible");
            !refused
        })
        .collect()
}

/// `step` with its references to earlier steps of its value moved past `base` steps.
fn shift_refs(step: &Value, base: usize) -> Value {
    match step {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| {
                    let v = if k == "ref" {
                        json!(v.as_u64().unwrap() as usize + base)
                    } else {
                        shift_refs(v, base)
                    };
                    (k.clone(), v)
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(|v| shift_refs(v, base)).collect()),
        other => other.clone(),
    }
}

/// For each value, whether the port's caster `T` loads it (with conversions, as a function
/// with one overload loads its arguments).
fn port_accepts<T: for<'py> Cast<'py>>() -> Vec<bool> {
    Python::attach(|py| {
        values()
            .iter()
            // A Python error the caster lets through isn't the dispatcher's refusal.
            .map(|v| !matches!(T::try_load(&port_object(py, &json!(v)), true), Ok(None)))
            .collect()
    })
}

fn check<T: for<'py> Cast<'py>>(caster: &str, accepted: Vec<bool>) {
    let ours = port_accepts::<T>();
    let values = values();
    let differences: Vec<String> = values
        .iter()
        .zip(ours.iter().zip(&accepted))
        .filter(|(_, (a, b))| a != b)
        .map(|(v, (a, b))| format!("{}: port {a}, wheel {b}", json!(v)))
        .collect();
    assert!(
        differences.is_empty(),
        "{caster}:\n{}",
        differences.join("\n")
    );
}

/// The port's overloaded function: `(size: int)` and `(points: std::vector<float>)`, as
/// `GradingBSplineCurve.__init__`'s first and fourth overloads. Each returns the arguments it
/// took, as the wheel's call to make the same curve: `[n]` or `[[x, ...]]`.
static PROBE: ocio_py::dispatch::Function = ocio_py::dispatch::Function {
    name: "probe",
    constructor: false,
    method: false,
    operator: false,
    overloads: &[
        ocio_py::dispatch::Overload {
            params: &[ocio_py::dispatch::Param::new("size", "typing.SupportsInt")],
            ret: "list",
            call: |py, _, a, convert| {
                let Some(n) = <i32 as Cast>::try_load(&a[0], convert)? else {
                    return Ok(None);
                };
                Ok(Some(pyo3::types::PyList::new(py, [n])?.into_any().unbind()))
            },
            pos_only: 0,
        },
        ocio_py::dispatch::Overload {
            params: &[ocio_py::dispatch::Param::new(
                "points",
                "collections.abc.Sequence[typing.SupportsFloat]",
            )],
            ret: "list",
            call: |py, _, a, convert| {
                let Some(points) = <Vec<f32> as Cast>::try_load(&a[0], convert)? else {
                    return Ok(None);
                };
                Ok(Some(
                    pyo3::types::PyList::new(py, [pyo3::types::PyList::new(py, points)?])?
                        .into_any()
                        .unbind(),
                ))
            },
            pos_only: 0,
        },
    ],
};

/// Each overload's choice is the wheel's: for each value, the port's dispatcher picks an
/// overload (or none), and the wheel's GradingBSplineCurve(value) makes the curve the wheel
/// makes from the arguments that overload took (or refuses the value the same way).
#[test]
fn overloads_are_tried_without_conversions_first() {
    let four = || json!({ "list": [f(1.0), f(2.0), f(3.0), f(4.0)] });
    let with_index = |base: &str, items: Value| {
        vec![
            call(
                name("type"),
                json!([s("V"), { "tuple": [name(base)] }, { "dict": [[s("__module__"), s("probe")], [s("__index__"), name("numpy.size")]] }]),
            ),
            call(json!({ "ref": 0 }), json!([items])),
        ]
    };
    let values: Vec<Vec<Value>> = vec![
        vec![json!({ "eval": int("3") })],
        vec![json!({ "eval": { "bool": true } })],
        vec![call(name("numpy.int64"), json!([int("3")]))],
        vec![call(name("numpy.float32"), json!([f(3.0)]))],
        vec![json!({ "eval": f(3.5) })],
        vec![json!({ "eval": four() })],
        vec![json!({ "eval": { "list": [int("1"), int("2"), int("3"), int("4")] } })],
        vec![json!({ "eval": { "tuple": [f(1.0), f(2.0), f(3.0), f(4.0)] } })],
        vec![call(name("set"), json!([four()]))],
        vec![call(name("numpy.array"), json!([four()]))],
        // A list (or tuple) with __index__: a sequence of floats without conversions, an int
        // only with them.
        with_index("list", four()),
        with_index("tuple", four()),
        // An object with __index__ only: an int with conversions.
        vec![
            call(
                name("type"),
                json!([s("W"), { "tuple": [name("object")] }, { "dict": [[s("__module__"), s("probe")], [s("__index__"), name("numpy.size")]] }]),
            ),
            call(json!({ "ref": 0 }), json!([])),
        ],
        vec![json!({ "eval": s("abcd") })],
        vec![json!({ "eval": { "none": null } })],
    ];
    // The port's choices: the arguments of the overload it took, or None (refused).
    common::start_python();
    let chosen: Vec<Option<Value>> = Python::attach(|py| {
        values
            .iter()
            .map(|v| {
                let obj = port_object(py, &json!(v));
                let args = pyo3::types::PyTuple::new(py, [obj]).unwrap();
                match ocio_py::dispatch::dispatch(py, &PROBE, &args, None) {
                    Ok(taken) => {
                        let text: String = py
                            .import("json")
                            .unwrap()
                            .call_method1("dumps", (taken,))
                            .unwrap()
                            .extract()
                            .unwrap();
                        Some(serde_json::from_str(&text).unwrap())
                    }
                    Err(_) => None,
                }
            })
            .collect()
    });
    // The wheel: GradingBSplineCurve(value), then from the arguments the port's overload took.
    let mut steps = Vec::new();
    let mut pairs = Vec::new();
    for (v, taken) in values.iter().zip(&chosen) {
        let base = steps.len();
        for step in v {
            steps.push(shift_refs(step, base));
        }
        let made = steps.len();
        steps.push(call(
            name("OCIO.GradingBSplineCurve"),
            json!([{ "ref": made - 1 }]),
        ));
        let reference = taken.as_ref().map(|args| {
            let args: Vec<Value> = args
                .as_array()
                .unwrap()
                .iter()
                .map(|a| match a {
                    Value::Number(n) => int(&n.to_string()),
                    Value::Array(xs) => json!({ "list": xs.iter().map(|x| f(x.as_f64().unwrap())).collect::<Vec<_>>() }),
                    other => panic!("{other}"),
                })
                .collect();
            steps.push(call(name("OCIO.GradingBSplineCurve"), json!(args)));
            steps.len() - 1
        });
        pairs.push((made, reference));
    }
    let report = wheel(&[json!({ "steps": steps })]).remove(0);
    for ((made, reference), v) in pairs.into_iter().zip(&values) {
        let theirs = &report["steps"][made];
        match reference {
            Some(r) => assert_eq!(theirs, &report["steps"][r], "{}", json!(v)),
            None => {
                let message = String::from_utf8(ocio_testkit::oracle_values::bytes(
                    &theirs["raised"]["message"],
                ))
                .unwrap_or_default();
                assert!(
                    message.contains("incompatible constructor arguments"),
                    "{}: the port refused, the wheel: {theirs}",
                    json!(v)
                );
            }
        }
    }
}

/// Steps that make an instance of a class `class` (of `base`), whose members are values of the
/// steps `objects` makes first (`{"ref": i, "attr": ...}` of them), called with `args`.
fn instance(
    objects: Vec<Value>,
    class: &str,
    base: &str,
    members: Vec<(&str, Value)>,
    args: Value,
) -> Vec<Value> {
    let mut steps = objects;
    let mut dict = vec![json!([s("__module__"), s("probe")])];
    dict.extend(members.into_iter().map(|(k, v)| json!([s(k), v])));
    steps.push(call(
        name("type"),
        json!([s(class), { "tuple": [name(base)] }, { "dict": dict }]),
    ));
    steps.push(call(json!({ "ref": steps.len() - 1 }), args));
    steps
}

/// The attribute `attr` of step `i`'s result.
fn attr(i: usize, attr: &str) -> Value {
    json!({ "ref": i, "attr": attr })
}

fn ev(v: Value) -> Value {
    json!({ "eval": v })
}

fn floats(xs: &[f64]) -> Value {
    json!({ "list": xs.iter().map(|&x| f(x)).collect::<Vec<_>>() })
}

fn ints(xs: &[&str]) -> Value {
    json!({ "list": xs.iter().map(|x| int(x)).collect::<Vec<_>>() })
}

/// An object whose `len()` is `n` (`n.__index__`, called without `self`) and whose items are
/// 1.0 to 4.0.
fn sized(n: &str) -> Vec<Value> {
    instance(
        vec![ev(int(n)), ev(floats(&[1.0, 2.0, 3.0, 4.0]))],
        "Sized",
        "object",
        vec![
            ("__len__", attr(0, "__index__")),
            ("__getitem__", attr(1, "__getitem__")),
        ],
        json!([]),
    )
}

/// An object whose successive `len()`s are `lens` (popped from their end) and whose item `i`
/// is `i` as a float, for `i` up to 7.
fn stateful(lens: &[&str]) -> Vec<Value> {
    instance(
        vec![
            ev(ints(lens)),
            ev(floats(&[0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0])),
        ],
        "Stateful",
        "object",
        vec![
            ("__len__", attr(0, "pop")),
            ("__getitem__", attr(1, "__getitem__")),
        ],
        json!([]),
    )
}

/// The sequences and iterables of the container casters' checks (verifier re-check 1).
fn sequences() -> Vec<Vec<Value>> {
    let four = || floats(&[1.0, 2.0, 3.0, 4.0]);
    vec![
        vec![ev(four())],
        vec![ev(floats(&[1.0, 2.0, 3.0]))],
        // __getitem__ without __len__: a sequence whose len() raises.
        instance(
            vec![ev(four())],
            "N",
            "object",
            vec![("__getitem__", attr(0, "__getitem__"))],
            json!([]),
        ),
        // len() 2, item 0 'x' (refused), item 1 a KeyError: refused before reading item 1.
        instance(
            vec![
                ev(ints(&["0", "0"])),
                ev(json!({ "dict": [[int("0"), s("x")]] })),
            ],
            "E",
            "object",
            vec![
                ("__len__", attr(0, "__len__")),
                ("__getitem__", attr(1, "__getitem__")),
            ],
            json!([]),
        ),
        // Items that raise an error whose text names 'Sequence': it goes through.
        instance(
            vec![
                ev(ints(&["0", "0", "0", "0"])),
                call(name("operator.methodcaller"), json!([s("'Sequence'")])),
            ],
            "T",
            "object",
            vec![
                ("__len__", attr(0, "__len__")),
                ("__getitem__", json!({ "ref": 1 })),
            ],
            json!([]),
        ),
        // list subclasses: __getitem__ 7.0, __iter__ 9.0, __len__ 2.
        instance(
            vec![ev(floats(&[7.0, 7.0, 7.0, 7.0]))],
            "G",
            "list",
            vec![("__getitem__", attr(0, "__getitem__"))],
            json!([four()]),
        ),
        instance(
            vec![ev(floats(&[9.0, 9.0, 9.0, 9.0]))],
            "L",
            "list",
            vec![("__iter__", attr(0, "__iter__"))],
            json!([four()]),
        ),
        instance(
            vec![ev(ints(&["0", "0"]))],
            "K",
            "list",
            vec![("__len__", attr(0, "__len__"))],
            json!([four()]),
        ),
        // len()s the C++ runtime can't reserve (I-236), and one that doesn't fit Py_ssize_t.
        sized("4611686018427387904"),
        sized("4611686018427387903"),
        sized("4611686018427387892"),
        sized("2305843009213693952"),
        sized("1099511627776"),
        sized("9223372036854775808"),
        // Successive len()s 6, 4, 4 / 4, then an IndexError.
        stateful(&["6", "4", "4"]),
        stateful(&["4"]),
        stateful(&["4", "4611686018427387904", "4"]),
        // A real generator (numpy's), map, zip, a dict's keys, a set, a bytearray.
        vec![call(
            name("numpy._core.shape_base._block_dispatcher"),
            json!([four()]),
        )],
        // `map`, which the step interpreter's names don't list, through the copy module's
        // builtins.
        vec![
            ev(name("copy.__builtins__")),
            call(attr(0, "get"), json!([s("map")])),
            call(
                json!({ "ref": 1 }),
                json!([name("float"), ints(&["1", "2", "3", "4"])]),
            ),
        ],
        vec![call(name("zip"), json!([]))],
        vec![
            ev(
                json!({ "dict": [[f(1.0), int("0")], [f(2.0), int("0")], [f(3.0), int("0")], [f(4.0), int("0")]] }),
            ),
            call(attr(0, "keys"), json!([])),
        ],
        vec![call(name("set"), json!([four()]))],
        // A set with __index__ (3): not taken as a sequence without conversions, so the
        // `(size)` overload takes it in the second pass.
        instance(
            vec![ev(int("3"))],
            "S",
            "set",
            vec![("__index__", attr(0, "__index__"))],
            json!([four()]),
        ),
        vec![call(name("bytearray"), json!([{ "bytes": "01020304" }]))],
        // An instance of a class named map (pybind11 compares the C name), one named x.map, and
        // a class whose metaclass is named map (a class is never taken by its name).
        instance(
            vec![ev(four())],
            "map",
            "object",
            vec![("__iter__", attr(0, "__iter__"))],
            json!([]),
        ),
        instance(
            vec![ev(four())],
            "x.map",
            "object",
            vec![("__iter__", attr(0, "__iter__"))],
            json!([]),
        ),
        vec![
            call(
                name("type"),
                json!([s("map"), { "tuple": [name("type")] }, { "dict": [[s("__module__"), s("probe")]] }]),
            ),
            call(
                json!({ "ref": 0 }),
                json!([s("X"), { "tuple": [] }, { "dict": [] }]),
            ),
        ],
    ]
}

/// What a call did: the value, a refusal ("incompatible ... arguments"), or another exception
/// (its type's qualified name and message).
#[derive(Debug, PartialEq)]
enum Outcome {
    Value(Value),
    Refused,
    Raised(String, String),
}

/// The port's outcome of `r`: `Err` with "incompatible" in a TypeError is a refusal.
fn port_outcome(py: Python<'_>, r: PyResult<Option<Value>>) -> Outcome {
    match r {
        Ok(Some(v)) => Outcome::Value(v),
        Ok(None) => Outcome::Refused,
        Err(e) => {
            let ty = e.get_type(py);
            let qualified = format!("{}.{}", ty.module().unwrap(), ty.qualname().unwrap());
            let message = e.value(py).str().unwrap().to_string();
            if qualified == "builtins.TypeError" && message.contains("incompatible") {
                Outcome::Refused
            } else {
                Outcome::Raised(qualified, message)
            }
        }
    }
}

/// The wheel's outcome of a step's result: a value (whatever `value` makes of it), a refusal,
/// or another exception.
fn wheel_outcome(step: &Value, value: impl Fn(&Value) -> Value) -> Outcome {
    match step.get("raised") {
        None => Outcome::Value(value(&step["value"])),
        Some(raised) => {
            let ty = raised["type"].as_str().unwrap().to_string();
            let message =
                String::from_utf8(ocio_testkit::oracle_values::bytes(&raised["message"])).unwrap();
            if ty == "builtins.TypeError" && message.contains("incompatible") {
                Outcome::Refused
            } else {
                Outcome::Raised(ty, message)
            }
        }
    }
}

/// `std::vector<float>` loads each value as the wheel's does: the dispatcher's choice for
/// `GradingBSplineCurve(value)` (its `(size)` and `(points)` overloads), what the sequence
/// gives, and the errors a sequence raises (its `len()`, its items, the C++ runtime's
/// reservation), in the order pybind11 reads it.
#[test]
fn vectors_load_sequences_as_the_wheels() {
    let values = sequences();
    common::start_python();
    let ours: Vec<Outcome> = Python::attach(|py| {
        values
            .iter()
            .map(|v| {
                let obj = port_object(py, &json!(v));
                let args = pyo3::types::PyTuple::new(py, [obj]).unwrap();
                let r = ocio_py::dispatch::dispatch(py, &PROBE, &args, None).map(|taken| {
                    let text: String = py
                        .import("json")
                        .unwrap()
                        .call_method1("dumps", (taken,))
                        .unwrap()
                        .extract()
                        .unwrap();
                    Some(serde_json::from_str::<Value>(&text).unwrap())
                });
                port_outcome(py, r)
            })
            .collect()
    });
    // The wheel: GradingBSplineCurve(value); and, where the port took an overload, the curve
    // the wheel makes from the arguments the port took.
    let mut steps = Vec::new();
    let mut pairs = Vec::new();
    for (v, our) in values.iter().zip(&ours) {
        let base = steps.len();
        for step in v {
            steps.push(shift_refs(step, base));
        }
        let made = steps.len();
        steps.push(call(
            name("OCIO.GradingBSplineCurve"),
            json!([{ "ref": made - 1 }]),
        ));
        let reference = match our {
            Outcome::Value(args) => {
                let args: Vec<Value> = args
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|a| match a {
                        Value::Number(n) => int(&n.to_string()),
                        Value::Array(xs) => {
                            json!({ "list": xs.iter().map(|x| f(x.as_f64().unwrap())).collect::<Vec<_>>() })
                        }
                        other => panic!("{other}"),
                    })
                    .collect();
                steps.push(call(name("OCIO.GradingBSplineCurve"), json!(args)));
                Some(steps.len() - 1)
            }
            _ => None,
        };
        pairs.push((made, reference));
    }
    let report = wheel(&[json!({ "steps": steps })]).remove(0);
    let mut differences = Vec::new();
    let mut kinds = [0; 3];
    for (((made, reference), v), our) in pairs.into_iter().zip(&values).zip(&ours) {
        let theirs = &report["steps"][made];
        let same = match (our, reference) {
            // The curve (or error) the wheel makes from the port's arguments.
            (Outcome::Value(_), Some(r)) => theirs == &report["steps"][r],
            _ => wheel_outcome(theirs, Value::clone) == *our,
        };
        kinds[match our {
            Outcome::Value(_) => 0,
            Outcome::Refused => 1,
            Outcome::Raised(..) => 2,
        }] += 1;
        if !same {
            differences.push(format!("{}:\n  port {our:?}\n  wheel {theirs}", json!(v)));
        }
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));
    // Each kind of outcome is met.
    assert!(kinds.iter().all(|&k| k >= 3), "{kinds:?}");
}

/// `std::array<double, 4>` (`MatrixTransform.setOffset`, one overload) loads each value as the
/// wheel's does: the array taken (as `getOffset` gives it back), refused, or the error raised.
#[test]
fn arrays_load_sequences_as_the_wheels() {
    let values = sequences();
    common::start_python();
    let ours: Vec<Outcome> = Python::attach(|py| {
        values
            .iter()
            .map(|v| {
                let obj = port_object(py, &json!(v));
                let r = <[f64; 4] as Cast>::try_load(&obj, true).map(|a| {
                    a.map(|a| {
                        json!(
                            a.iter()
                                .map(|x| format!("0x{:016x}", x.to_bits()))
                                .collect::<Vec<_>>()
                        )
                    })
                });
                port_outcome(py, r)
            })
            .collect()
    });
    let mut steps = vec![call(name("OCIO.MatrixTransform"), json!([]))];
    let mut calls = Vec::new();
    for v in &values {
        let base = steps.len();
        for step in v {
            steps.push(shift_refs(step, base));
        }
        let set = steps.len();
        steps.push(call(attr(0, "setOffset"), json!([{ "ref": set - 1 }])));
        steps.push(call(attr(0, "getOffset"), json!([])));
        calls.push(set);
    }
    let report = wheel(&[json!({ "steps": steps })]).remove(0);
    let mut differences = Vec::new();
    let mut kinds = [0; 3];
    for ((set, v), our) in calls.into_iter().zip(&values).zip(&ours) {
        let theirs = wheel_outcome(&report["steps"][set], |_| {
            let offset = &report["steps"][set + 1]["value"]["items"];
            json!(
                offset
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x["float"].clone())
                    .collect::<Vec<_>>()
            )
        });
        kinds[match our {
            Outcome::Value(_) => 0,
            Outcome::Refused => 1,
            Outcome::Raised(..) => 2,
        }] += 1;
        if theirs != *our {
            differences.push(format!("{}:\n  port {our:?}\n  wheel {theirs:?}", json!(v)));
        }
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));
    assert!(kinds.iter().all(|&k| k >= 3), "{kinds:?}");
}

/// pybind11 takes an iterable by its type's whole C name (`tp_name`), not by `__name__`, its
/// last dotted part: an instance of a class whose C name is `PyOpenColorIO.PyOpenColorIO.map`
/// isn't a `map` (no class of the wheel's has such a name, so this runs on the port only).
#[test]
fn iterables_are_taken_by_their_whole_c_name() {
    common::start_python();
    Python::attach(|py| {
        let cls = ocio_py::pybind11::new_class(py, "map", "map", &[], &[]).unwrap();
        assert_eq!(cls.name().unwrap().to_string(), "map");
        let obj = cls.call_method1("__new__", (&cls,)).unwrap();
        assert!(matches!(<Vec<f32> as Cast>::try_load(&obj, true), Ok(None)));
    });
}

/// The array caster's reads the wheel leaves undefined (U-96, U-97) are the port's errors.
#[test]
fn arrays_refuse_what_the_wheel_reads_undefined() {
    common::start_python();
    Python::attach(|py| {
        for (lens, text) in [(&["5", "4"][..], "U-96"), (&["2", "4", "4"][..], "U-97")] {
            let obj = port_object(py, &json!(stateful(lens)));
            let e = <[f64; 4] as Cast>::try_load(&obj, true).unwrap_err();
            assert!(
                e.is_instance_of::<pyo3::exceptions::PyRuntimeError>(py),
                "{e}"
            );
            assert!(e.to_string().contains(text), "{e}");
        }
    });
}

/// A float literal of the step interpreter.
fn f(x: f64) -> Value {
    json!({ "float": format!("0x{:016x}", x.to_bits()) })
}

/// Each caster accepts what the wheel accepts for a parameter of its C++ type.
#[test]
fn casters_accept_what_the_wheel_accepts() {
    let new = |class: &str| call(name(&format!("OCIO.{class}")), json!([]));
    let method = |m: &str| json!({ "ref": 0, "attr": m });
    // int: Baker::setCubeSize(int).
    check::<i32>(
        "int",
        wheel_accepts(new("Baker"), method("setCubeSize"), vec![], vec![]),
    );
    // unsigned long: Lut1DTransform::setLength(unsigned long).
    check::<CULong>(
        "unsigned long",
        wheel_accepts(new("Lut1DTransform"), method("setLength"), vec![], vec![]),
    );
    // double: ExposureContrastTransform::setExposure(double).
    check::<f64>(
        "double",
        wheel_accepts(
            new("ExposureContrastTransform"),
            method("setExposure"),
            vec![],
            vec![],
        ),
    );
    // bool: Lut1DTransform::setInputHalfDomain(bool).
    check::<bool>(
        "bool",
        wheel_accepts(
            new("Lut1DTransform"),
            method("setInputHalfDomain"),
            vec![],
            vec![],
        ),
    );
    // const char *: SetEnvVariable's value.
    check::<CharPtr>(
        "const char *",
        wheel_accepts(
            new("Baker"),
            name("OCIO.SetEnvVariable"),
            vec![s("OCIO_RS_CASTER_PROBE")],
            vec![],
        ),
    );
    // std::string: Config.CreateFromStream(const std::string &).
    check::<StdString>(
        "std::string",
        wheel_accepts(
            new("Baker"),
            name("OCIO.Config.CreateFromStream"),
            vec![],
            vec![],
        ),
    );
    // std::vector<std::string>: Context(searchPaths=...), the constructor's second overload
    // (the first takes no argument), tried without then with conversions.
    check::<Vec<StdString>>(
        "std::vector<std::string>",
        wheel_accepts_keyword(name("OCIO.Context"), "searchPaths"),
    );
    // std::array<double, 4>: MatrixTransform::setOffset.
    check::<[f64; 4]>(
        "std::array<double, 4>",
        wheel_accepts(new("MatrixTransform"), method("setOffset"), vec![], vec![]),
    );
}
