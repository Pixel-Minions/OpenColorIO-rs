// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! pybind11's enums (chunks 6.2a and 6.2b), against the wheel, on `TransformDirection`:
//! - its surface as the oracle's walk reports it (py_surface_walk.py's `_class`, run on the
//!   port's class in the embedded interpreter, against the wheel's py_surface): metatype, MRO,
//!   bases, docstring, every member in order with its kind and docstrings, the entries, and
//!   the values exported into the module;
//! - its behaviour, through the same scripts on both sides: repr, str, name, value, int,
//!   index, hash, equality with ints, floats, None, strings and other enums, construction from
//!   ints (in range or not) and its refusals, `__members__`, `__entries`, `__getstate__` and
//!   `__setstate__`, copies, the class's and an instance's docstrings;
//! - the arithmetic enums' comparisons and bitwise operators (on ints, strings `int()` parses
//!   or doesn't, None, floats, other enums, reflected), and construction from values at the
//!   edges of `int`, `unsigned int`, `unsigned long` (whose width the platform's wheel sets)
//!   and `unsigned char` (`GpuShaderCreator.TextureDimensions`). py_types_oracle.rs compares every enum's surface.
//!
//! Left out until the owner decides on the C-API module (`pybind11.rs`): the texts CPython
//! writes with a class's C name (`'<' not supported between instances of ...`, an unknown
//! attribute's AttributeError).
#![cfg(feature = "python")]

mod common;

use common::{call, name, port, text, wheel};
use ocio_testkit::Oracle;
use pyo3::prelude::*;
use serde_json::{Value, json};

/// The wheel's py_surface entry named `n` among the module's members.
fn wheel_member(report: &Value, n: &str) -> Value {
    report["module"]["members"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| String::from_utf8(ocio_testkit::oracle_values::bytes(&e["name"])).unwrap() == n)
        .unwrap_or_else(|| panic!("{n}"))
        .clone()
}

/// The port's entry for `OCIO.<n>`, as py_surface_walk.py reports a class (`_class`) or a value
/// (`_value`), with the name the module's walk gives it.
fn port_member(n: &str) -> Value {
    common::start_python();
    Python::attach(|py| {
        let walk = py.import("py_surface_walk").unwrap();
        let ocio = py.import("PyOpenColorIO").unwrap();
        let v = ocio.getattr(n).unwrap();
        let entry = if v.is_instance_of::<pyo3::types::PyType>() {
            let seen = pyo3::types::PyDict::new(py);
            walk.call_method1("_class", (v, format!("PyOpenColorIO.{n}"), seen))
                .unwrap()
        } else {
            walk.call_method1("_value", (v,)).unwrap()
        };
        entry
            .set_item("name", walk.call_method1("_text", (n,)).unwrap())
            .unwrap();
        let text: String = py
            .import("json")
            .unwrap()
            .call_method1("dumps", (entry,))
            .unwrap()
            .extract()
            .unwrap();
        serde_json::from_str(&text).unwrap()
    })
}

/// The class, its members and the exported values report as the wheel's.
#[test]
fn the_enum_class_has_the_wheels_surface() {
    let report = Oracle::get().call("py_surface", json!({}), &[]).result;
    for n in [
        "TransformDirection",
        "TRANSFORM_DIR_FORWARD",
        "TRANSFORM_DIR_INVERSE",
    ] {
        let theirs = wheel_member(&report, n);
        let ours = port_member(n);
        assert_eq!(ours, theirs, "{n}");
    }
}

fn int(v: &str) -> Value {
    json!({ "int": v })
}

/// The steps of the behaviour script.
fn steps() -> Vec<Value> {
    let e = |n: &str| name(&format!("OCIO.TransformDirection.{n}"));
    let fwd = || e("TRANSFORM_DIR_FORWARD");
    let inv = || e("TRANSFORM_DIR_INVERSE");
    let op = |f: &str, a: Value, b: Value| call(name(&format!("operator.{f}")), json!([a, b]));
    vec![
        json!({ "eval": inv(), "getters": ["name", "value", "__int__", "__index__", "__hash__", "__getstate__", "__repr__", "__str__", "__doc__"] }),
        json!({ "eval": name("OCIO.TransformDirection.__doc__") }),
        json!({ "eval": name("OCIO.TransformDirection.__name__") }),
        json!({ "eval": name("OCIO.TransformDirection.__qualname__") }),
        json!({ "eval": name("OCIO.TransformDirection.__module__") }),
        call(name("OCIO.TransformDirection"), json!([int("1")])),
        call(name("OCIO.TransformDirection"), json!([int("5")])),
        call(name("OCIO.TransformDirection"), json!([int("-2147483649")])),
        json!({ "call": name("OCIO.TransformDirection"), "kwargs": [["value", int("0")]] }),
        call(name("OCIO.TransformDirection"), json!([])),
        call(name("OCIO.TransformDirection"), json!([{ "str": "78" }])),
        call(
            name("OCIO.TransformDirection"),
            json!([{ "float": "0x3ff0000000000000" }]),
        ),
        call(name("OCIO.TransformDirection"), json!([fwd()])),
        op("eq", inv(), int("1")),
        op("eq", inv(), json!({ "float": "0x3ff0000000000000" })),
        op("eq", inv(), json!({ "bool": true })),
        op("eq", inv(), json!({ "none": null })),
        op("ne", inv(), json!({ "none": null })),
        op("eq", inv(), fwd()),
        op("ne", inv(), fwd()),
        op("eq", inv(), json!({ "str": "78" })),
        op("eq", int("1"), inv()),
        op("eq", inv(), inv()),
        json!({ "eval": name("OCIO.TransformDirection.__members__") }),
        json!({ "eval": name("OCIO.TransformDirection.__entries") }),
        call(name("copy.copy"), json!([inv()])),
        call(name("copy.deepcopy"), json!([inv()])),
        call(name("OCIO.TransformDirection.__getstate__"), json!([fwd()])),
        call(name("OCIO.TransformDirection.__int__"), json!([int("1")])),
        call(name("OCIO.TransformDirection.__repr__"), json!([int("1")])),
        json!({ "call": name("OCIO.TransformDirection.__eq__"), "args": [inv()], "kwargs": [["other", int("1")]] }),
        call(
            name("OCIO.TransformDirection.__init__"),
            json!([inv(), int("0")]),
        ),
        json!({ "eval": inv(), "getters": ["value"] }),
        call(
            name("OCIO.TransformDirection.__init__"),
            json!([inv(), int("1")]),
        ),
        call(
            name("OCIO.TransformDirection.__setstate__"),
            json!([inv(), int("1")]),
        ),
        call(name("hash"), json!([inv()])),
        call(name("int"), json!([inv()])),
        call(name("bool"), json!([fwd()])),
        call(
            name("isinstance"),
            json!([inv(), name("OCIO.TransformDirection")]),
        ),
        json!({ "eval": name("OCIO.TRANSFORM_DIR_FORWARD") }),
        op("is_", name("OCIO.TRANSFORM_DIR_FORWARD"), fwd()),
        json!({ "eval": name("OCIO.TransformDirection.name.__doc__") }),
        json!({ "eval": name("OCIO.TransformDirection.value.__doc__") }),
        json!({ "eval": name("OCIO.TransformDirection.__init__.__doc__") }),
    ]
}

/// The enum behaves as the wheel's.
#[test]
fn the_enum_behaves_as_the_wheels() {
    let script = json!({ "steps": steps() });
    let ours = port(&script);
    let theirs = wheel(std::slice::from_ref(&script)).remove(0);
    let (ours, theirs) = (
        ours["steps"].as_array().unwrap(),
        theirs["steps"].as_array().unwrap(),
    );
    for (i, (a, b)) in ours.iter().zip(theirs).enumerate() {
        assert_eq!(a, b, "step {i}: {}", script["steps"][i]);
    }
    assert_eq!(ours.len(), theirs.len());
}

/// The steps of the arithmetic enums' script: `LoggingLevel` and `BitDepth` (`int`),
/// `OptimizationFlags` (`unsigned long`, 32 bits in the Windows wheel and 64 in the Linux one),
/// and `ProcessorCacheFlags` (`unsigned int`, not arithmetic).
fn arithmetic_steps() -> Vec<Value> {
    let v = |e: &str, n: &str| name(&format!("OCIO.{e}.{n}"));
    let info = || v("LoggingLevel", "LOGGING_LEVEL_INFO");
    let f32 = || v("BitDepth", "BIT_DEPTH_F32");
    let all = || v("OptimizationFlags", "OPTIMIZATION_ALL");
    let lossless = || v("OptimizationFlags", "OPTIMIZATION_LOSSLESS");
    let op = |f: &str, a: Value, b: Value| call(name(&format!("operator.{f}")), json!([a, b]));
    let mut steps = vec![];
    for f in ["lt", "gt", "le", "ge", "and_", "or_", "xor", "eq", "ne"] {
        steps.push(op(f, info(), int("2")));
        steps.push(op(f, info(), int("3")));
        steps.push(op(f, int("1"), info()));
        steps.push(op(f, info(), f32()));
        steps.push(op(f, info(), text("33")));
        steps.push(op(f, info(), text("x")));
        steps.push(op(f, info(), json!({ "none": null })));
        steps.push(op(f, info(), json!({ "float": "0x4000000000000000" })));
        steps.push(op(f, all(), lossless()));
        steps.push(op(f, int("-1"), all()));
    }
    // `~` on a non-arithmetic enum raises with the class's C name: left out with the other such
    // texts (above).
    for target in [info(), all()] {
        steps.push(call(name("operator.invert"), json!([target])));
    }
    let c = |e: &str, arg: Value| call(name(&format!("OCIO.{e}")), json!([arg]));
    for e in [
        "OptimizationFlags",
        "ProcessorCacheFlags",
        "LoggingLevel",
        "GpuShaderCreator.TextureDimensions",
    ] {
        for n in [
            "-1",
            "255",
            "256",
            "4294967295",
            "4294967296",
            "18446744073709551615",
            "18446744073709551616",
            "2147483648",
        ] {
            steps.push(c(e, int(n)));
        }
        steps.push(c(e, json!({ "bool": true })));
    }
    steps.push(json!({ "eval": all(), "getters": ["name", "value", "__int__", "__hash__", "__repr__", "__str__"] }));
    steps.push(json!({ "eval": name("OCIO.OptimizationFlags.__lt__.__doc__") }));
    steps.push(json!({ "eval": name("OCIO.OptimizationFlags.__rand__.__doc__") }));
    steps.push(json!({ "eval": name("OCIO.OptimizationFlags.__invert__.__doc__") }));
    steps.push(json!({ "eval": name("OCIO.OptimizationFlags.__init__.__doc__") }));
    steps.push(json!({ "eval": name("OCIO.ProcessorCacheFlags.__init__.__doc__") }));
    steps.push(call(name("OCIO.OptimizationFlags.__lt__"), json!([all()])));
    steps.push(json!({ "call": name("OCIO.BitDepth.__and__"), "args": [f32()], "kwargs": [["other", int("1")]] }));
    steps
}

/// The arithmetic enums and the unsigned ones behave as the wheel's.
#[test]
fn arithmetic_and_unsigned_enums_behave_as_the_wheels() {
    let script = json!({ "steps": arithmetic_steps() });
    let ours = port(&script);
    let theirs = wheel(std::slice::from_ref(&script)).remove(0);
    let (ours, theirs) = (
        ours["steps"].as_array().unwrap(),
        theirs["steps"].as_array().unwrap(),
    );
    for (i, (a, b)) in ours.iter().zip(theirs).enumerate() {
        assert_eq!(a, b, "step {i}: {}", script["steps"][i]);
    }
    assert_eq!(ours.len(), theirs.len());
}
