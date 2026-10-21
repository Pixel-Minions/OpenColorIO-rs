// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! pybind11's constructors and class identity against the wheel, through the same step
//! scripts on both sides (on `ViewType`, which no other test of this binary changes):
//! - a class parameter (and `self`) is checked by type, not by name: after
//!   `OCIO.ViewType.__module__ = ...`, its values still compare and construct;
//! - `__init__` and `__setstate__` are constructors: on an instance constructed already they
//!   change nothing and load no argument; without an instance of the class as `self`, they
//!   raise "__init__(self, ...) called with invalid or missing `self` argument"; their
//!   refusal is the constructor's text, named by the function;
//! - pybind11's metatype refuses an instance a Python subclass's `__init__` didn't construct,
//!   and "No constructor defined!" names a Python subclass by its name;
//! - the instance size and weak reference offset, and the module of methods and functions;
//! - the function record type (its dictionary, flags and size) and pickling: a module
//!   function's record reduces to its module, read by name when called, others' aren't
//!   pickleable.
//!
//! And, without the wheel (whose behaviour is undefined there, U-86, U-87, U-98, U-99): an
//! instance no constructor constructed, `pybind11_object` itself, a function record made,
//! initialized or failing to pickle from Python, and a keyword name pybind11 can't write in
//! an error, raise the port's errors.
#![cfg(feature = "python")]

mod common;

use common::{call, name, port, text, wheel};
use pyo3::prelude::*;
use serde_json::{Value, json};

fn int(v: &str) -> Value {
    json!({ "int": v })
}

fn none() -> Value {
    json!({ "none": null })
}

fn setattr(target: Value, attr: &str, value: Value) -> Value {
    json!({ "setattr": target, "name": attr, "value": value })
}

/// `type(name, (base,), {"__module__": "probe", "__init__": init})`, without `__init__` when
/// `init` is None. (`__module__` given: by default it is the module that calls `type`, which
/// the oracle runs as `__main__` and the port imports as `py_eval_steps`.)
fn subclass(n: &str, base: &str, init: Option<&str>) -> Value {
    let mut ns: Vec<Value> = vec![json!([text("__module__"), text("probe")])];
    ns.extend(init.map(|i| json!([text("__init__"), name(i)])));
    call(
        name("type"),
        json!([text(n), { "tuple": [name(base)] }, { "dict": ns }]),
    )
}

/// The script, `module` naming the extension module (the wheel's `OCIO.PyOpenColorIO`, the
/// embedded port's `OCIO` itself), whose name is `module_name`: set to the wheel's while the
/// script runs, so that records reduce to the same text.
fn steps(module: &str, module_name: &str) -> Vec<Value> {
    let shared = || name("OCIO.VIEW_SHARED");
    let class = || name("OCIO.ViewType");
    let mut s = vec![
        setattr(
            name(module),
            "__name__",
            text("PyOpenColorIO.PyOpenColorIO"),
        ),
        // Identity, not names.
        setattr(class(), "__module__", text("PyOpenColorIO")),
        call(name("operator.eq"), json!([shared(), int("0")])),
        call(class(), json!([int("1")])),
        setattr(class(), "__module__", text("Foo")),
        call(name("operator.eq"), json!([shared(), int("0")])),
        call(class(), json!([int("1")])),
        setattr(class(), "__qualname__", text("Bar")),
        call(class(), json!([int("1")])),
        call(name("repr"), json!([shared()])),
        setattr(class(), "__qualname__", text("ViewType")),
        setattr(class(), "__module__", text("PyOpenColorIO.PyOpenColorIO")),
        // Constructors on a constructed instance.
        call(name("OCIO.VIEW_SHARED.__setstate__"), json!([int("1")])),
        json!({ "eval": shared(), "getters": ["value"] }),
        call(name("OCIO.VIEW_SHARED.__init__"), json!([text("x")])),
        call(name("OCIO.VIEW_SHARED.__init__"), json!([int("1")])),
        json!({ "eval": shared(), "getters": ["value"] }),
        // Without `self`.
        call(name("OCIO.ViewType.__init__"), json!([int("1"), int("1")])),
        call(name("OCIO.ViewType.__init__"), json!([])),
        call(name("OCIO.ViewType.__init__"), json!([none()])),
        call(
            name("OCIO.ViewType.__setstate__"),
            json!([int("1"), int("1")]),
        ),
        call(name("OCIO.ViewType.__setstate__"), json!([])),
        call(
            name("OCIO.ViewType.__init__"),
            json!([name("OCIO.TRANSFORM_DIR_FORWARD"), int("1")]),
        ),
        json!({ "call": name("OCIO.ViewType.__init__"), "kwargs": [["self", shared()], ["value", int("1")]] }),
    ];
    // An instance no constructor constructed yet, then refused or constructed by a
    // constructor. Of a subclass with `object.__repr__` and no `__members__`: reporting an
    // unconstructed enum reads its value, undefined in the wheel (U-86).
    let fresh_class = s.len();
    s.push(call(
        name("type"),
        json!([text("F"), { "tuple": [class()] }, { "dict": [[text("__module__"), text("probe")], [text("__repr__"), name("object.__repr__")]] }]),
    ));
    for (ctor, args) in [
        ("__setstate__", vec![text("x"), none(), int("1"), int("0")]),
        ("__init__", vec![text("x"), int("1"), int("0")]),
    ] {
        let fresh = s.len();
        s.push(call(
            json!({ "ref": fresh_class, "attr": "__new__" }),
            json!([{ "ref": fresh_class }]),
        ));
        for arg in args {
            s.push(call(json!({ "ref": fresh, "attr": ctor }), json!([arg])));
        }
        s.push(call(name("int"), json!([{ "ref": fresh }])));
    }
    // pybind11's metatype and Python subclasses.
    for (n, base, init, args) in [
        (
            "P",
            "OCIO.GpuShaderCreator",
            Some("object.__init__"),
            json!([]),
        ),
        ("Q", "OCIO.GpuShaderCreator", None, json!([])),
        (
            "D",
            "OCIO.GpuShaderDesc",
            Some("object.__init__"),
            json!([]),
        ),
        ("R", "OCIO.ViewType", Some("object.__init__"), json!([])),
        ("S", "OCIO.ViewType", None, json!([int("1")])),
    ] {
        let i = s.len();
        s.push(subclass(n, base, init));
        s.push(call(json!({ "ref": i }), args));
    }
    s.push(call(name("OCIO.Processor"), json!([])));
    // Layout and modules.
    for n in ["ViewType", "GpuShaderCreator", "GpuShaderDesc", "Processor"] {
        s.push(json!({ "eval": name(&format!("OCIO.{n}")), "getters": ["__basicsize__", "__weakrefoffset__", "__dictoffset__", "__itemsize__"] }));
    }
    s.push(call(name("OCIO.VIEW_SHARED.__sizeof__"), json!([])));
    s.push(json!({ "eval": name("OCIO.ViewType.__repr__.__module__") }));
    s.push(json!({ "eval": name("OCIO.ViewType.__init__.__module__") }));
    s.push(json!({ "eval": name("OCIO.GetEnvVariable.__module__") }));
    s.push(json!({ "eval": name("OCIO.BitDepthToString.__module__") }));
    // Functions: their `__self__` is pybind11's function record, whose class (named with
    // pybind11's ABI tag, per platform) CPython writes in `__qualname__` and the repr.
    for f in [
        "OCIO.GetEnvVariable",
        "OCIO.BitDepthToString",
        "OCIO.ViewType.__repr__",
        "OCIO.ViewType.__eq__",
        "OCIO.ViewType.__init__",
        "OCIO.ViewType._pybind11_conduit_v1_",
        "OCIO.ViewType.name.fget",
        "OCIO.ViewType.value.fget",
        "OCIO.ViewType.__members__.fget",
    ] {
        s.push(json!({ "eval": name(f), "getters": ["__qualname__", "__name__", "__module__", "__self__", "__text_signature__"] }));
        s.push(json!({ "eval": name(&format!("{f}.__self__.__class__")), "getters": ["__module__", "__qualname__", "__name__", "__doc__"] }));
        s.push(call(
            name("hasattr"),
            json!([name(&format!("{f}.__self__")), text("dispatch_call")]),
        ));
        // Pickling (verifier re-check 3): a module function's record reduces to the module,
        // any other's raises "<repr> is not pickleable.", whatever the arguments.
        for args in [json!([int("2")]), json!([]), json!([text("x"), none()])] {
            s.push(call(name(&format!("{f}.__self__.__reduce_ex__")), args));
        }
        s.push(json!({ "call": name(&format!("{f}.__self__.__reduce_ex__")), "kwargs": [["protocol", int("4")]] }));
        s.push(call(name(&format!("{f}.__self__.__reduce__")), json!([])));
        s.push(call(name(&format!("{f}.__reduce_ex__")), json!([int("2")])));
    }
    // The record type: its dictionary, flags and size, its __new__ refusing another type,
    // copy of a record (through __reduce_ex__: the module).
    for a in [
        "__flags__",
        "__basicsize__",
        "__itemsize__",
        "__weakrefoffset__",
        "__doc__",
    ] {
        s.push(json!({ "eval": name(&format!("OCIO.GetVersion.__self__.__class__.{a}")) }));
    }
    s.push(call(
        name("list"),
        json!([name("OCIO.GetVersion.__self__.__class__.__dict__")]),
    ));
    for a in ["__new__", "__init__", "__reduce_ex__"] {
        s.push(json!({ "eval": name(&format!("OCIO.GetVersion.__self__.__class__.{a}")), "getters": ["__text_signature__", "__doc__", "__qualname__"] }));
    }
    s.push(call(
        name("OCIO.GetVersion.__self__.__class__.__new__"),
        json!([name("int")]),
    ));
    s.push(call(
        name("copy.deepcopy"),
        json!([name("OCIO.GetVersion")]),
    ));
    // The module's name is read when reducing: __module__, else __name__, else none (not
    // pickleable). (One that isn't a str aborts the wheel: U-98.)
    let reduce = || {
        call(
            name("OCIO.GetVersion.__self__.__reduce_ex__"),
            json!([int("2")]),
        )
    };
    s.push(setattr(name(module), "__name__", text("renamed")));
    s.push(reduce());
    s.push(setattr(name(module), "__module__", text("moduled")));
    s.push(reduce());
    s.push(json!({ "delattr": name(module), "name": "__module__" }));
    s.push(json!({ "delattr": name(module), "name": "__name__" }));
    s.push(reduce());
    s.push(setattr(name(module), "__name__", text(module_name)));
    s
}

/// Constructors, identity and layout as the wheel's.
#[test]
fn constructors_and_identity_as_the_wheels() {
    let script = json!({ "steps": steps("OCIO", "PyOpenColorIO") });
    let ours = port(&script);
    let theirs = wheel(&[json!({
        "steps": steps("OCIO.PyOpenColorIO", "PyOpenColorIO.PyOpenColorIO")
    })])
    .remove(0);
    let (ours, theirs) = (
        ours["steps"].as_array().unwrap(),
        theirs["steps"].as_array().unwrap(),
    );
    for (i, (a, b)) in ours.iter().zip(theirs).enumerate() {
        assert_eq!(a, b, "step {i}: {}", script["steps"][i]);
    }
    assert_eq!(ours.len(), theirs.len());
}

/// The errors the port raises where the wheel's behaviour is undefined: reading an instance
/// no constructor constructed (U-86), constructing `pybind11_object` itself, which
/// `__reduce__()` does through copyreg (U-87), a keyword name pybind11 fails to write in an
/// error (U-99).
#[test]
fn undefined_behaviour_raises_the_ports_errors() {
    common::start_python();
    Python::attach(|py| {
        let ocio = py.import("PyOpenColorIO").unwrap();
        let cls = ocio.getattr("ViewType").unwrap();
        let fresh = cls.call_method1("__new__", (&cls,)).unwrap();
        let never = "PyOpenColorIO.PyOpenColorIO.ViewType: the instance was never constructed \
                     (no __init__ or __setstate__ gave it a value; the wheel reads uninitialized \
                     memory)";
        for attempt in [
            fresh.getattr("value"),
            fresh.call_method0("__int__"),
            fresh.call_method0("__index__"),
            fresh.call_method0("__hash__"),
            fresh.call_method0("__getstate__"),
            fresh.repr().map(|r| r.into_any()),
            fresh
                .eq(0)
                .map(|b| pyo3::types::PyBool::new(py, b).to_owned().into_any()),
            ocio.getattr("AllocationToString").unwrap().call1((ocio
                .getattr("Allocation")
                .unwrap()
                .call_method1("__new__", (ocio.getattr("Allocation").unwrap(),))
                .unwrap(),)),
        ] {
            let e = attempt.unwrap_err();
            assert!(
                e.is_instance_of::<pyo3::exceptions::PyRuntimeError>(py),
                "{e}"
            );
            let message = e.value(py).to_string();
            assert!(
                message == never || message == never.replace("ViewType", "Allocation"),
                "{message}"
            );
        }
        let base = cls.getattr("__mro__").unwrap().get_item(1).unwrap();
        let no_instances = "pybind11_builtins.pybind11_object: pybind11's base class has no \
                            instances (the wheel's process aborts)";
        for attempt in [
            base.call0(),
            ocio.getattr("VIEW_SHARED")
                .unwrap()
                .call_method0("__reduce__"),
        ] {
            let e = attempt.unwrap_err();
            assert!(e.is_instance_of::<pyo3::exceptions::PyTypeError>(py), "{e}");
            assert_eq!(e.value(py).to_string(), no_instances);
        }
        // A function record made or initialized from Python (U-98).
        let record = ocio
            .getattr("GetVersion")
            .unwrap()
            .getattr("__self__")
            .unwrap();
        let record_type = record.get_type();
        let c_name = format!(
            "{}.{}",
            record_type.module().unwrap(),
            record_type.name().unwrap()
        );
        for (attempt, verb) in [
            (record_type.call0(), "made"),
            (record_type.call_method1("__new__", (&record_type,)), "made"),
            (
                record.call_method0("__init__").map(|r| r.into_any()),
                "initialized",
            ),
            (record.call_method1("__init__", (1, 2)), "initialized"),
        ] {
            let e = attempt.unwrap_err();
            assert!(e.is_instance_of::<pyo3::exceptions::PyTypeError>(py), "{e}");
            assert_eq!(
                e.value(py).to_string(),
                format!(
                    "{c_name}: pybind11's function records are {verb} by pybind11 only (the \
                     wheel's process aborts; docs/improvements.md, U-98)"
                )
            );
        }
        // Pickling a module function's record when its module's name isn't a str, or without
        // `eval` among the builtins (U-98).
        let module = ocio.clone();
        let reduce = || record.call_method1("__reduce_ex__", (2,));
        let pickling = format!(
            "{c_name}: pickling a function record raised an error, which aborts the wheel's \
             process (docs/improvements.md, U-98)"
        );
        let original = module.getattr("__name__").unwrap();
        module.setattr("__name__", 5).unwrap();
        let attempt = reduce();
        module.setattr("__name__", &original).unwrap();
        let builtins = py.import("builtins").unwrap();
        let eval = builtins.getattr("eval").unwrap();
        builtins.delattr("eval").unwrap();
        let without_eval = reduce();
        builtins.setattr("eval", eval).unwrap();
        for (attempt, kind) in [(attempt, "TypeError"), (without_eval, "KeyError")] {
            let e = attempt.unwrap_err();
            assert!(e.is_instance_of::<pyo3::exceptions::PyTypeError>(py), "{e}");
            assert_eq!(e.value(py).to_string(), pickling);
            let cause = e.cause(py).unwrap();
            assert_eq!(cause.get_type(py).name().unwrap().to_string(), kind);
        }
        // A keyword whose name isn't UTF-8, in the error of a call no overload takes (U-99).
        let kwargs = pyo3::types::PyDict::new(py);
        kwargs
            .set_item(
                pyo3::types::PyString::from_encoded_object(
                    &pyo3::types::PyBytes::new(py, b"\xed\xa0\x80"),
                    Some(c"utf-8"),
                    Some(c"surrogatepass"),
                )
                .unwrap(),
                1,
            )
            .unwrap();
        for f in ["GetEnvVariable", "TransformDirection"] {
            let e = ocio
                .getattr(f)
                .unwrap()
                .call((), Some(&kwargs))
                .unwrap_err();
            assert!(e.is_instance_of::<pyo3::exceptions::PyTypeError>(py), "{e}");
            let name = if f == "GetEnvVariable" { f } else { "__init__" };
            assert_eq!(
                e.value(py).to_string(),
                format!(
                    "{name}(): a keyword argument's name pybind11 can't write in its error (the \
                     wheel's process aborts; docs/improvements.md, U-99)"
                )
            );
        }
    });
}
