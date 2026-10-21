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
//!   first check of the dispatcher.
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

/// The port's overloaded function: `(size: int)` and `(points: Sequence[float])`, as
/// `GradingBSplineCurve.__init__`'s first and fourth overloads. Each returns the arguments it
/// took, as the wheel's call to make the same curve: `[n]` or `[[x, ...]]`.
static PROBE: ocio_py::dispatch::Function = ocio_py::dispatch::Function {
    name: "probe",
    constructor: false,
    method: false,
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
                let Some(points) = <Vec<f64> as Cast>::try_load(&a[0], convert)? else {
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
