// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyTypes` (chunk 6.2b) against the wheel, through the oracle's surface walk
//! (py_surface_walk.py's `_members`, run on the port's module in the embedded interpreter,
//! against the wheel's py_surface):
//! - every name of the port's module is one of the wheel's module, in the wheel's order (no
//!   `__all__`, which would make the package's `import *` drop the rest);
//! - the enums and their exported values, the string constants, the exceptions and the
//!   module's functions (the string conversions among them) report as the wheel's, entry for
//!   entry;
//! - the classes `bindPyTypes` declares have the wheel's metatype, module, name, docstring,
//!   bases and MRO, and the members a declaration gives them: `_pybind11_conduit_v1_`, and
//!   `__init__`, the slot wrapper that raises "No constructor defined!" (the wheel's, for a
//!   class that never gets a constructor; until its card binds one, for the others), and then
//!   the members bound so far (GpuShaderCreator's enums and their values), as the wheel's and
//!   in its order.

#![cfg(feature = "python")]

mod common;

use ocio_testkit::Oracle;
use ocio_testkit::oracle_values::bytes;
use pyo3::prelude::*;
use serde_json::{Value, json};

/// The declared classes whose cards bind them completely: each compares with the wheel's entry
/// for entry (transforms_oracle.rs checks them in detail).
const COMPLETE: [&str; 35] = [
    "Transform",
    "GroupTransform",
    "MatrixTransform",
    "RangeTransform",
    "AllocationTransform",
    "ExponentTransform",
    "ExponentWithLinearTransform",
    "LogTransform",
    "LogAffineTransform",
    "LogCameraTransform",
    "FixedFunctionTransform",
    "BuiltinTransform",
    "BuiltinTransformRegistry",
    "CDLTransform",
    "FileTransform",
    "FormatMetadata",
    "Context",
    "ProcessorMetadata",
    "FileRules",
    "ViewingRules",
    "ColorSpace",
    "ColorSpaceSet",
    "Look",
    "NamedTransform",
    "ViewTransform",
    "ImageDesc",
    "PackedImageDesc",
    "PlanarImageDesc",
    "CPUProcessor",
    "Lut1DTransform",
    "Lut3DTransform",
    "ColorSpaceTransform",
    "DisplayViewTransform",
    "LookTransform",
    "BuiltinConfigRegistry",
];

/// The classes a binding makes itself rather than bindPyTypes' declarations (bindPyConfigIOProxy's
/// `py::bind_vector`): each compares with the wheel's entry for entry.
const OWN_CLASSES: [&str; 2] = ["vector_of_uint8_t", "PyConfigIOProxy"];

/// The declared classes bound but for members a later card binds: each compares with the
/// wheel's entry for entry but for those members, which are exactly these (chunk 6.6a: the
/// I/O proxy; chunk 6.8a: `ConfigUtils`).
const PARTIAL: [(&str, &[&str]); 1] = [(
    "Config",
    &[
        "CreateFromConfigIOProxy",
        "getConfigIOProxy",
        "setConfigIOProxy",
        "IdentifyBuiltinColorSpace",
        "IdentifyInterchangeSpace",
        "GetProcessorToBuiltinColorSpace",
        "GetProcessorFromBuiltinColorSpace",
        "GetProcessorFromConfigs",
    ],
)];

/// The stand-ins `common::start_python` adds to the port's module until WP 6.6 binds them.
const STAND_INS: [&str; 3] = [
    "SetLoggingFunction",
    "ResetToDefaultLoggingFunction",
    "ResetComputeHashFunction",
];

fn entry_name(e: &Value) -> String {
    String::from_utf8(bytes(&e["name"])).unwrap()
}

/// The wheel's module members, as py_surface reports them.
fn wheel_members() -> Vec<Value> {
    let report = Oracle::get().call("py_surface", json!({}), &[]).result;
    report["module"]["members"].as_array().unwrap().clone()
}

/// The port's module members, as py_surface_walk.py's `_members` reports them, without the
/// stand-ins.
fn port_members() -> Vec<Value> {
    common::start_python();
    Python::attach(|py| {
        let walk = py.import("py_surface_walk").unwrap();
        let module = py.import("PyOpenColorIO").unwrap();
        // The walk's `seen`, keyed by id(), as py_surface_walk.surface() starts it.
        let seen = py
            .eval(c"lambda m: {id(m): 'PyOpenColorIO'}", None, None)
            .unwrap()
            .call1((&module,))
            .unwrap();
        let namespace = py
            .eval(
                c"lambda m, skip: {k: v for k, v in vars(m).items() if k not in skip}",
                None,
                None,
            )
            .unwrap()
            .call1((&module, STAND_INS.to_vec()))
            .unwrap();
        let skip = walk.getattr("RUNTIME_NAMES").unwrap();
        let skip = py
            .import("builtins")
            .unwrap()
            .getattr("set")
            .unwrap()
            .call1((skip,))
            .unwrap();
        let members = walk
            .call_method1("_members", (namespace, "PyOpenColorIO", seen, skip))
            .unwrap();
        let text: String = py
            .import("json")
            .unwrap()
            .call_method1("dumps", (members,))
            .unwrap()
            .extract()
            .unwrap();
        serde_json::from_str::<Value>(&text)
            .unwrap()
            .as_array()
            .unwrap()
            .clone()
    })
}

/// Every name of the port's module is the wheel's, in the wheel's order.
#[test]
fn the_module_names_are_the_wheels_in_its_order() {
    let theirs: Vec<String> = wheel_members().iter().map(entry_name).collect();
    let ours: Vec<String> = port_members().iter().map(entry_name).collect();
    for n in &ours {
        assert!(theirs.contains(n), "{n} is not the wheel's");
    }
    let in_order: Vec<&String> = theirs.iter().filter(|n| ours.contains(n)).collect();
    assert_eq!(ours.iter().collect::<Vec<_>>(), in_order);
    // bindPyTypes' names are all there: 63 classes, 31 enums, 34 string constants.
    for n in [
        "Baker",
        "ConfigMerger",
        "ProcessorCacheFlags",
        "OCIO_BUILTIN_URI_PREFIX",
    ] {
        assert!(ours.iter().any(|o| o == n), "{n}");
    }
}

/// The enums, their values, the constants and the functions report as the wheel's; the
/// declared classes as a declaration makes them.
#[test]
fn the_module_members_report_as_the_wheels() {
    let theirs = wheel_members();
    let ours = port_members();
    let (mut enums, mut classes, mut others, mut bound, mut complete) = (0, 0, 0, 0, 0);
    let mut own = 0;
    let mut partial = 0;
    for our in &ours {
        let n = entry_name(our);
        let their = theirs.iter().find(|t| entry_name(t) == n).unwrap();
        let is_enum = their.get("entries").is_some();
        let declared = their["metatype"] == "pybind11_builtins.pybind11_type" && !is_enum;
        if OWN_CLASSES.contains(&n.as_str()) {
            assert_eq!(our, their, "{n}");
            own += 1;
            continue;
        }
        if their["kind"] != "class" || !declared {
            assert_eq!(our, their, "{n}");
            if is_enum {
                enums += 1;
            } else {
                others += 1;
            }
            continue;
        }
        classes += 1;
        if let Some((_, missing)) = PARTIAL.iter().find(|(c, _)| *c == n) {
            let without = |v: &Value| {
                let mut v = v.clone();
                let members = v["members"].as_array_mut().unwrap();
                members.retain(|m| !missing.contains(&entry_name(m).as_str()));
                v
            };
            let their_count = their["members"].as_array().unwrap().len();
            let our_count = our["members"].as_array().unwrap().len();
            assert_eq!(our_count + missing.len(), their_count, "{n}");
            assert_eq!(*our, without(their), "{n}");
            partial += 1;
            continue;
        }
        if COMPLETE.contains(&n.as_str()) {
            assert_eq!(our, their, "{n}");
            complete += 1;
            continue;
        }
        let strip = |v: &Value| {
            let mut v = v.clone();
            v.as_object_mut().unwrap().remove("members");
            v
        };
        assert_eq!(strip(our), strip(their), "{n}");
        let (our_members, their_members) = (
            our["members"].as_array().unwrap(),
            their["members"].as_array().unwrap(),
        );
        // The declaration's members first, then those bound so far (GpuShaderCreator's
        // enums and their values), each the wheel's, in the wheel's order.
        assert!(our_members.len() >= 2, "{n}");
        assert_eq!(entry_name(&our_members[0]), "__init__", "{n}");
        assert_eq!(entry_name(&their_members[0]), "__init__", "{n}");
        if their_members[0]["kind"] == "wrapper_descriptor" {
            assert_eq!(our_members[0], their_members[0], "{n}");
        } else {
            assert_eq!(our_members[0]["kind"], "wrapper_descriptor", "{n}");
        }
        let in_order: Vec<&Value> = their_members[1..]
            .iter()
            .filter(|t| our_members.iter().any(|o| entry_name(o) == entry_name(t)))
            .collect();
        assert_eq!(our_members[1..].iter().collect::<Vec<_>>(), in_order, "{n}");
        assert_eq!(entry_name(&our_members[1]), "_pybind11_conduit_v1_", "{n}");
        bound += our_members.len() - 2;
    }
    assert_eq!(classes, 63);
    assert_eq!(complete, COMPLETE.len());
    assert_eq!(own, OWN_CLASSES.len());
    assert_eq!(partial, PARTIAL.len());
    assert_eq!(enums, 31);
    // The constants, the exported values and the functions of bindPyTypes and
    // PyOpenColorIO.cpp.
    assert!(others >= 34 + 31 + 32, "{others}");
    // GpuShaderCreator's enums and their values.
    assert_eq!(bound, 6);
}

/// A class without a constructor refuses to be constructed, as the wheel's does.
#[test]
fn a_declared_class_has_no_constructor() {
    let script = json!({ "steps": [
        common::call(common::name("OCIO.Processor"), json!([])),
        common::call(common::name("OCIO.CPUProcessor"), json!([{ "int": "1" }])),
        json!({ "call": common::name("OCIO.GPUProcessor"), "kwargs": [["a", { "int": "1" }]] }),
        json!({ "eval": common::name("OCIO.Processor.__init__.__doc__") }),
        // The class's own slot wrapper, named by its C name.
        common::call(
            common::name("repr"),
            json!([common::name("OCIO.Processor.__init__")]),
        ),
        common::call(
            common::name("repr"),
            json!([common::name("OCIO.GpuShaderDesc.__init__")]),
        ),
        json!({ "eval": common::name("OCIO.Processor.nope") }),
        common::call(common::name("type"), json!([common::name("OCIO.Processor")])),
    ]});
    let ours = common::port(&script);
    let theirs = common::wheel(std::slice::from_ref(&script)).remove(0);
    assert_eq!(ours, theirs);
}
