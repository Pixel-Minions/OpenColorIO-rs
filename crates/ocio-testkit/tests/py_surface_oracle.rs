// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's `py_surface` command (`oracle/ocio_oracle/py_surface_api.py`, O6.1), against
//! the wheel itself: a script of its own walks the wheel's compiled module in the oracle's
//! environment (`Oracle::run_script`, independently of the command) and prints one line per
//! fact (each name in order with its kind, docstrings, enum entries, property accessors,
//! class hierarchies, values); the command's report, flattened the same way, must hold the
//! same lines in the same order. Bad requests are refused.

use ocio_testkit::Oracle;
use ocio_testkit::oracle_values::bytes;
use serde_json::{Value, json};

/// The independent walk: `path<TAB>fact<TAB>value` lines, the module's names then each class's,
/// recursively, in dictionary order; text as UTF-8 hex.
const WALK: &str = r#"
import PyOpenColorIO as P
import PyOpenColorIO.PyOpenColorIO as M

def h(s):
    return "-" if s is None else s.encode("utf-8").hex()

def q(c):
    return c.__module__ + "." + c.__qualname__

done = {}
SKIP_MODULE = {"__builtins__", "__loader__", "__spec__", "__file__", "__cached__", "__path__",
               "__package__", "__name__", "__doc__"}
SKIP_CLASS = {"__doc__", "__module__", "__dict__", "__weakref__", "__qualname__"}

def walk(ns, path, skip):
    for name in list(ns.keys()):
        if name in skip:
            continue
        v = ns[name]
        p = path + "." + name
        if isinstance(v, type):
            if id(v) in done:
                print(p, "class_ref", done[id(v)], sep="\t")
                continue
            done[id(v)] = p
            print(p, "class", q(type(v)), sep="\t")
            print(p, "doc", h(v.__doc__), sep="\t")
            print(p, "mro", ",".join(q(b) for b in v.__mro__), sep="\t")
            print(p, "bases", ",".join(q(b) for b in v.__bases__), sep="\t")
            if "__members__" in v.__dict__:
                for n, e in v.__members__.items():
                    print(p, "entry", n + "=" + str(int(e)), sep="\t")
            walk(v.__dict__, p, SKIP_CLASS)
        elif isinstance(v, property):
            print(p, type(v).__name__, h(v.__doc__), sep="\t")
            print(p, "accessors", "".join(
                "1" if f is not None else "0" for f in (v.fget, v.fset, v.fdel)), sep="\t")
        elif isinstance(v, (staticmethod, classmethod)):
            print(p, type(v).__name__, h(v.__func__.__doc__), sep="\t")
        elif callable(v):
            print(p, type(v).__name__, h(v.__doc__), sep="\t")
        elif isinstance(v, str):
            print(p, "str", h(v), sep="\t")
        elif type(v).__name__ == "PyCapsule":
            print(p, "capsule", "", sep="\t")
        elif type(v) is int:
            print(p, "int", str(v), sep="\t")
        else:
            print(p, "value", q(type(v)) + "=" + h(repr(v)), sep="\t")

walk(M.__dict__, "PyOpenColorIO", SKIP_MODULE)
for name, v in P.__dict__.items():
    if name not in M.__dict__ and name not in SKIP_MODULE:
        print("package." + name, "type", q(type(v)), sep="\t")
print("module", "name", h(M.__name__), sep="\t")
print("module", "doc", h(M.__doc__), sep="\t")
"#;

/// Text from the command: hex, or "-" for null.
fn h(v: &Value) -> String {
    if v.is_null() {
        "-".to_string()
    } else {
        ocio_testkit::oracle_values::hex(&bytes(v))
    }
}

fn name(entry: &Value) -> String {
    String::from_utf8(bytes(&entry["name"])).unwrap()
}

/// The command's report as the script's lines.
fn flatten(members: &Value, path: &str, out: &mut Vec<String>) {
    for e in members.as_array().unwrap() {
        let p = format!("{path}.{}", name(e));
        let kind = e["kind"].as_str().unwrap();
        let line = |fact: &str, value: String| format!("{p}\t{fact}\t{value}");
        match kind {
            "class_ref" => out.push(line("class_ref", e["target"].as_str().unwrap().into())),
            "class" => {
                out.push(line("class", e["metatype"].as_str().unwrap().into()));
                out.push(line("doc", h(&e["doc"])));
                let list = |k: &str| {
                    e[k].as_array()
                        .unwrap()
                        .iter()
                        .map(|b| b.as_str().unwrap())
                        .collect::<Vec<_>>()
                        .join(",")
                };
                out.push(line("mro", list("mro")));
                out.push(line("bases", list("bases")));
                if let Some(entries) = e.get("entries") {
                    for entry in entries.as_array().unwrap() {
                        out.push(line(
                            "entry",
                            format!(
                                "{}={}",
                                entry[0].as_str().unwrap(),
                                entry[1].as_str().unwrap()
                            ),
                        ));
                    }
                }
                flatten(&e["members"], &p, out);
            }
            "property" | "pybind11_static_property" => {
                out.push(line(kind, h(&e["doc"])));
                let flag = |k: &str| if e[k].as_bool().unwrap() { "1" } else { "0" };
                out.push(line(
                    "accessors",
                    format!("{}{}{}", flag("getter"), flag("setter"), flag("deleter")),
                ));
            }
            "value" => {
                let ty = e["type"].as_str().unwrap();
                if let Some(text) = e.get("text") {
                    out.push(line("str", h(text)));
                } else if let Some(i) = e.get("int") {
                    out.push(line("int", i.as_str().unwrap().into()));
                } else if ty == "builtins.PyCapsule" {
                    out.push(line("capsule", String::new()));
                } else {
                    out.push(line("value", format!("{ty}={}", h(&e["repr"]))));
                }
            }
            _ => out.push(line(kind, h(&e["doc"]))),
        }
    }
}

fn surface() -> Value {
    Oracle::get().call("py_surface", json!({}), &[]).result
}

/// The command reports every name of the module and of each class, in order, with its kind,
/// its docstrings, its enum entries, its property accessors, its class's metatype, bases and
/// MRO, and its value: exactly what an independent walk of the wheel prints.
#[test]
fn reports_the_whole_module_as_an_independent_walk_sees_it() {
    let report = surface();
    let mut ours = Vec::new();
    flatten(&report["module"]["members"], "PyOpenColorIO", &mut ours);
    for e in report["package"].as_array().unwrap() {
        ours.push(format!(
            "package.{}\ttype\t{}",
            name(e),
            e["type"].as_str().unwrap()
        ));
    }
    ours.push(format!("module\tname\t{}", h(&report["module"]["name"])));
    ours.push(format!("module\tdoc\t{}", h(&report["module"]["doc"])));

    let walked = Oracle::get().run_script(WALK, &[]);
    assert!(walked.len() > 2_000, "{} lines", walked.len());
    let first_difference = ours.iter().zip(&walked).position(|(a, b)| a != b);
    assert!(
        first_difference.is_none() && ours.len() == walked.len(),
        "first difference at line {first_difference:?} of {} reported, {} walked: {:?} vs {:?}",
        ours.len(),
        walked.len(),
        first_difference.map(|i| &ours[i]),
        first_difference.map(|i| &walked[i]),
    );
}

/// The report's shape where the binding's sources decide it: both exception classes
/// (`PyOpenColorIO.cpp:15-22`, pybind11's `register_exception`, whose default base is
/// Python's `Exception`, so `ExceptionMissingFile` doesn't derive from `OCIO.Exception`), an enum and its values exported at module level (`PyTypes.cpp`,
/// `export_values`), a static factory (`PyConfig.cpp`, `def_static("CreateRaw", ...)`) and a
/// read-write property (`PyGradingData.cpp`, `def_readwrite("red", ...)`).
#[test]
fn reports_classes_enums_static_methods_and_properties() {
    let report = surface();
    let members = report["module"]["members"].as_array().unwrap();
    let find = |list: &[Value], n: &str| -> Value {
        list.iter()
            .find(|e| name(e) == n)
            .unwrap_or_else(|| panic!("{n} not reported"))
            .clone()
    };

    for cls in ["Exception", "ExceptionMissingFile"] {
        let e = find(members, cls);
        assert_eq!(e["kind"], "class");
        let mro: Vec<&str> = e["mro"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(mro[0], format!("PyOpenColorIO.PyOpenColorIO.{cls}"));
        assert_eq!(
            &mro[1..],
            [
                "builtins.Exception",
                "builtins.BaseException",
                "builtins.object"
            ]
        );
    }

    let direction = find(members, "TransformDirection");
    assert_eq!(
        direction["entries"],
        json!([
            ["TRANSFORM_DIR_FORWARD", "0"],
            ["TRANSFORM_DIR_INVERSE", "1"]
        ])
    );
    for (n, value) in [
        ("TRANSFORM_DIR_FORWARD", "0"),
        ("TRANSFORM_DIR_INVERSE", "1"),
    ] {
        let exported = find(members, n);
        assert_eq!(exported["kind"], "value");
        assert_eq!(
            exported["type"],
            "PyOpenColorIO.PyOpenColorIO.TransformDirection"
        );
        assert_eq!(exported["enum_value"], value);
    }

    let config = find(members, "Config");
    let create_raw = find(config["members"].as_array().unwrap(), "CreateRaw");
    assert_eq!(create_raw["kind"], "staticmethod");
    assert!(!create_raw["doc"].is_null());

    let rgbm = find(members, "GradingRGBM");
    let red = find(rgbm["members"].as_array().unwrap(), "red");
    assert_eq!(red["kind"], "property");
    assert_eq!(
        (&red["getter"], &red["setter"], &red["deleter"]),
        (&json!(true), &json!(true), &json!(false))
    );
}

/// Requests it can't read are refused.
#[test]
fn bad_requests_are_refused() {
    for args in [json!({"extra": 1}), json!({"module": "os"})] {
        assert!(
            Oracle::get()
                .try_call("py_surface", args.clone(), &[])
                .is_err(),
            "{args}"
        );
    }
}
