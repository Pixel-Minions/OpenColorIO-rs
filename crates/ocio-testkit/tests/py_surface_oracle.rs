// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's `py_surface` command (`oracle/ocio_oracle/py_surface_api.py`, which runs
//! `py_surface_walk.py`, O6.1), against the wheel itself:
//! - a script of its own walks the wheel's compiled module in the oracle's environment
//!   (`Oracle::run_script`, independently of the command) and prints one line per fact (each
//!   name in order with its kind, docstrings, enum entries and values, property accessors and
//!   their docstrings, static methods' functions, class hierarchies, modules and qualified
//!   names, submodules); the command's report, flattened the same way, must hold the same
//!   lines in the same order;
//! - every entry has exactly the keys its kind documents, and no text of the report holds an
//!   address or a path of the installation;
//! - the facts the binding's sources decide are pinned (exceptions, an enum with large
//!   values, the three submodules and their functions, a static method, read-write and
//!   read-only properties);
//! - bad requests are refused.

use std::collections::BTreeSet;

use ocio_testkit::Oracle;
use ocio_testkit::oracle_values::bytes;
use serde_json::{Value, json};

/// The independent walk: `path<TAB>fact<TAB>value` lines, the module's names then each class's
/// and submodule's, recursively, in dictionary order; text as UTF-8 hex.
const WALK: &str = r#"
import PyOpenColorIO as P
import PyOpenColorIO.PyOpenColorIO as M

def h(s):
    return "-" if s is None else s.encode("utf-8").hex()

def q(c):
    return c.__module__ + "." + c.__qualname__

done = {id(M): "PyOpenColorIO"}
RUNTIME = ["__builtins__", "__loader__", "__spec__", "__file__", "__cached__", "__path__",
           "__package__", "__name__", "__doc__"]
SKIP_CLASS = ["__doc__", "__module__", "__dict__", "__weakref__", "__qualname__"]

def walk_module(m, p):
    print(p, "module", h(m.__name__), sep="\t")
    print(p, "doc", h(m.__doc__), sep="\t")
    print(p, "runtime", ",".join(n for n in m.__dict__ if n in RUNTIME), sep="\t")
    walk(m.__dict__, p, RUNTIME)

def walk(ns, path, skip):
    for name in list(ns.keys()):
        if name in skip:
            continue
        v = ns[name]
        p = path + "." + name
        if type(v) is type(M) or isinstance(v, type):
            if id(v) in done:
                print(p, "class_ref", done[id(v)], sep="\t")
                continue
            done[id(v)] = p
            if type(v) is type(M):
                walk_module(v, p)
                continue
            print(p, "class", q(type(v)), sep="\t")
            print(p, "module", h(v.__module__), sep="\t")
            print(p, "qualname", h(v.__qualname__), sep="\t")
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
            if v.fget is not None:
                print(p, "getter_doc", h(v.fget.__doc__), sep="\t")
            if v.fset is not None:
                print(p, "setter_doc", h(v.fset.__doc__), sep="\t")
        elif isinstance(v, (staticmethod, classmethod)):
            print(p, type(v).__name__, h(v.__func__.__doc__), sep="\t")
            print(p, "function_kind", type(v.__func__).__name__, sep="\t")
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
            if "__members__" in type(v).__dict__:
                print(p, "enum_value", str(int(v)), sep="\t")

walk_module(M, "PyOpenColorIO")
for name, v in P.__dict__.items():
    if name not in M.__dict__ and name not in RUNTIME:
        print("package." + name, "type", q(type(v)), sep="\t")
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

fn strings(v: &Value) -> Vec<&str> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|b| b.as_str().unwrap())
        .collect()
}

/// A module entry's lines.
fn flatten_module(e: &Value, p: &str, out: &mut Vec<String>) {
    out.push(format!("{p}\tmodule\t{}", h(&e["module_name"])));
    out.push(format!("{p}\tdoc\t{}", h(&e["doc"])));
    out.push(format!(
        "{p}\truntime\t{}",
        strings(&e["runtime_names"]).join(",")
    ));
    flatten(&e["members"], p, out);
}

/// The command's entries as the script's lines.
fn flatten(members: &Value, path: &str, out: &mut Vec<String>) {
    for e in members.as_array().unwrap() {
        let p = format!("{path}.{}", name(e));
        let kind = e["kind"].as_str().unwrap();
        let mut line = |fact: &str, value: String| out.push(format!("{p}\t{fact}\t{value}"));
        match kind {
            "class_ref" => line("class_ref", e["target"].as_str().unwrap().into()),
            "module" => flatten_module(e, &p, out),
            "class" => {
                line("class", e["metatype"].as_str().unwrap().into());
                line("module", h(&e["module"]));
                line("qualname", h(&e["qualname"]));
                line("doc", h(&e["doc"]));
                line("mro", strings(&e["mro"]).join(","));
                line("bases", strings(&e["bases"]).join(","));
                if let Some(entries) = e.get("entries") {
                    for entry in entries.as_array().unwrap() {
                        let (n, v) = (entry[0].as_str().unwrap(), entry[1].as_str().unwrap());
                        line("entry", format!("{n}={v}"));
                    }
                }
                flatten(&e["members"], &p, out);
            }
            "property" | "pybind11_static_property" => {
                line(kind, h(&e["doc"]));
                let flag = |k: &str| if e[k].as_bool().unwrap() { "1" } else { "0" };
                line(
                    "accessors",
                    format!("{}{}{}", flag("getter"), flag("setter"), flag("deleter")),
                );
                for k in ["getter_doc", "setter_doc"] {
                    if let Some(doc) = e.get(k) {
                        line(k, h(doc));
                    }
                }
            }
            "staticmethod" | "classmethod" => {
                line(kind, h(&e["doc"]));
                line("function_kind", e["function_kind"].as_str().unwrap().into());
            }
            "value" => {
                let ty = e["type"].as_str().unwrap();
                if let Some(text) = e.get("text") {
                    line("str", h(text));
                } else if let Some(i) = e.get("int") {
                    line("int", i.as_str().unwrap().into());
                } else if ty == "builtins.PyCapsule" {
                    line("capsule", String::new());
                } else {
                    line("value", format!("{ty}={}", h(&e["repr"])));
                    if let Some(v) = e.get("enum_value") {
                        line("enum_value", v.as_str().unwrap().into());
                    }
                }
            }
            _ => line(kind, h(&e["doc"])),
        }
    }
}

fn surface() -> Value {
    Oracle::get().call("py_surface", json!({}), &[]).result
}

/// Finds the entry named `n` in a list of entries.
fn find(list: &Value, n: &str) -> Value {
    list.as_array()
        .unwrap()
        .iter()
        .find(|e| name(e) == n)
        .unwrap_or_else(|| panic!("{n} not reported"))
        .clone()
}

/// The command reports every name of the module, of its submodules and of each class, in
/// order, with its kind, its docstrings, its enum entries and values, its property accessors
/// and their docstrings, its static methods' functions, its class's metatype, module,
/// qualified name, bases and MRO, and its value: exactly what an independent walk of the
/// wheel prints.
#[test]
fn reports_the_whole_module_as_an_independent_walk_sees_it() {
    let report = surface();
    let mut ours = Vec::new();
    flatten_module(&report["module"], "PyOpenColorIO", &mut ours);
    for e in report["package"].as_array().unwrap() {
        ours.push(format!(
            "package.{}\ttype\t{}",
            name(e),
            e["type"].as_str().unwrap()
        ));
    }

    let walked = Oracle::get().run_script(WALK, &[]);
    assert!(walked.len() > 4_000, "{} lines", walked.len());
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

/// The keys py_surface_api.py documents for an entry of `kind` (besides "name" and "kind"),
/// and those it may add.
fn schema(kind: &str, e: &Value) -> (Vec<&'static str>, Vec<&'static str>) {
    match kind {
        "module" => (
            vec!["module_name", "doc", "runtime_names", "members"],
            vec![],
        ),
        "class" => (
            vec![
                "metatype", "module", "qualname", "doc", "bases", "mro", "members",
            ],
            vec!["entries"],
        ),
        "class_ref" => (vec!["target"], vec![]),
        "property" | "pybind11_static_property" => (
            vec!["doc", "getter", "setter", "deleter"],
            vec!["getter_doc", "setter_doc"],
        ),
        "staticmethod" | "classmethod" => (vec!["function_kind", "doc"], vec![]),
        "value" => {
            let ty = e["type"].as_str().unwrap();
            if ty == "builtins.str" {
                (vec!["type", "text"], vec![])
            } else if ty == "builtins.int" {
                (vec!["type", "int"], vec![])
            } else if ty == "builtins.PyCapsule" {
                (vec!["type"], vec![])
            } else {
                (vec!["type", "repr"], vec!["enum_value"])
            }
        }
        _ => (vec!["doc"], vec![]),
    }
}

/// Checks every entry under `e` against [`schema`]; counts kinds in `kinds`.
fn check_schema(e: &Value, path: &str, kinds: &mut BTreeSet<String>) {
    let kind = e["kind"].as_str().unwrap();
    kinds.insert(kind.to_string());
    let (required, optional) = schema(kind, e);
    let keys: BTreeSet<&str> = e.as_object().unwrap().keys().map(String::as_str).collect();
    for k in &required {
        assert!(keys.contains(k), "{path}: {kind} without {k}: {e}");
    }
    for k in &keys {
        assert!(
            ["name", "kind"].contains(k) || required.contains(k) || optional.contains(k),
            "{path}: {kind} with {k}"
        );
    }
    if kind == "property" || kind == "pybind11_static_property" {
        for (accessor, doc) in [("getter", "getter_doc"), ("setter", "setter_doc")] {
            assert_eq!(e[accessor].as_bool().unwrap(), keys.contains(doc), "{path}");
        }
    }
    if let Some(members) = e.get("members") {
        for m in members.as_array().unwrap() {
            check_schema(m, &format!("{path}.{}", name(m)), kinds);
        }
    }
}

/// Every text of `v`: the strings and the decoded `{"bytes": hex}` values.
fn texts(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) => out.push(s.clone()),
        Value::Array(items) => items.iter().for_each(|x| texts(x, out)),
        Value::Object(map) if map.len() == 1 && map.contains_key("bytes") => {
            out.push(String::from_utf8(bytes(v)).unwrap());
        }
        Value::Object(map) => map.values().for_each(|x| texts(x, out)),
        _ => {}
    }
}

/// Whether `t` holds a Windows drive path (`C:\`, `d:/`), a letter and a colon not preceded
/// by a letter or a digit (`ocio://` is a scheme).
fn has_drive_path(t: &str) -> bool {
    let b = t.as_bytes();
    (0..b.len().saturating_sub(2)).any(|i| {
        b[i].is_ascii_alphabetic()
            && b[i + 1] == b':'
            && (b[i + 2] == b'\\' || b[i + 2] == b'/')
            && (i == 0 || !b[i - 1].is_ascii_alphanumeric())
    })
}

/// Every entry holds exactly the keys its kind documents, every kind of entry appears, and no
/// text of the report holds an address or a path of the installation.
#[test]
fn reports_each_entry_with_its_documented_keys_and_no_address_or_path() {
    let report = surface();
    let top: BTreeSet<&str> = report
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(top, BTreeSet::from(["module", "package"]));
    let mut module = report["module"].clone();
    module["name"] = json!({"bytes": ""});
    let mut kinds = BTreeSet::new();
    check_schema(&module, "PyOpenColorIO", &mut kinds);
    // ("class_ref" doesn't appear: the wheel reaches no class twice.)
    for kind in [
        "module",
        "class",
        "property",
        "pybind11_static_property",
        "staticmethod",
        "instancemethod",
        "builtin_function_or_method",
        "value",
    ] {
        assert!(kinds.contains(kind), "no {kind} in {kinds:?}");
    }
    for p in report["package"].as_array().unwrap() {
        let keys: BTreeSet<&str> = p.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys, BTreeSet::from(["name", "type"]));
    }

    let mut all = Vec::new();
    texts(&report, &mut all);
    assert!(all.len() > 4_000, "{}", all.len());
    for t in &all {
        assert!(!t.contains("0x"), "an address: {t:?}");
        assert!(
            !t.contains("site-packages") && !t.contains("/work/") && !has_drive_path(t),
            "a path: {t:?}"
        );
    }
    assert!(has_drive_path("C:\\x") && has_drive_path("(d:/x") && !has_drive_path("ocio://"));
}

/// The report's facts where the binding's sources decide them.
#[test]
fn reports_what_the_binding_defines() {
    let report = surface();
    let members = &report["module"]["members"];

    // PyOpenColorIO.cpp:15-28: both exceptions, through register_exception, whose default
    // base is Python's Exception (so ExceptionMissingFile doesn't derive from OCIO's).
    for cls in ["Exception", "ExceptionMissingFile"] {
        let e = find(members, cls);
        assert_eq!(e["kind"], "class");
        let mro = strings(&e["mro"]);
        assert_eq!(mro[0], format!("PyOpenColorIO.PyOpenColorIO.{cls}"));
        assert_eq!(
            &mro[1..],
            [
                "builtins.Exception",
                "builtins.BaseException",
                "builtins.object"
            ]
        );
        assert_eq!(String::from_utf8(bytes(&e["qualname"])).unwrap(), cls);
        assert_eq!(
            String::from_utf8(bytes(&e["module"])).unwrap(),
            "PyOpenColorIO.PyOpenColorIO"
        );
    }

    // PyTypes.cpp:446-466, py::enum_<Interpolation> with export_values; the values are
    // OpenColorTypes.h:412-419's.
    let interpolation = find(members, "Interpolation");
    assert_eq!(
        interpolation["entries"],
        json!([
            ["INTERP_UNKNOWN", "0"],
            ["INTERP_NEAREST", "1"],
            ["INTERP_LINEAR", "2"],
            ["INTERP_TETRAHEDRAL", "3"],
            ["INTERP_CUBIC", "4"],
            ["INTERP_DEFAULT", "254"],
            ["INTERP_BEST", "255"]
        ])
    );
    for (n, value) in [
        ("INTERP_LINEAR", "2"),
        ("INTERP_DEFAULT", "254"),
        ("INTERP_BEST", "255"),
    ] {
        let exported = find(members, n);
        assert_eq!(exported["kind"], "value");
        assert_eq!(
            exported["type"],
            "PyOpenColorIO.PyOpenColorIO.Interpolation"
        );
        assert_eq!(exported["enum_value"], value);
    }

    // The submodules (def_submodule) and their functions, in definition order:
    // apphelpers/PyColorSpaceHelpers.cpp:33-34, PyDisplayViewHelpers.cpp:11-73,
    // PyMergeConfigs.cpp:14 and 216-224.
    for (module, functions) in [
        ("ColorSpaceHelpers", &["AddColorSpace"][..]),
        (
            "DisplayViewHelpers",
            &[
                "GetProcessor",
                "GetIdentityProcessor",
                "AddDisplayView",
                "RemoveDisplayView",
            ][..],
        ),
        (
            "ConfigMergingHelpers",
            &["MergeConfigs", "MergeColorSpace"][..],
        ),
    ] {
        let e = find(members, module);
        assert_eq!(e["kind"], "module", "{module}");
        assert_eq!(
            String::from_utf8(bytes(&e["module_name"])).unwrap(),
            format!("PyOpenColorIO.PyOpenColorIO.{module}")
        );
        let names: Vec<String> = e["members"].as_array().unwrap().iter().map(name).collect();
        assert_eq!(names, functions, "{module}");
        for f in e["members"].as_array().unwrap() {
            assert_eq!(f["kind"], "builtin_function_or_method");
            let doc = String::from_utf8(bytes(&f["doc"])).unwrap();
            assert!(doc.starts_with(&format!("{}(", name(f))), "{doc}");
        }
    }

    // PyConfig.cpp:199, def_static("CreateRaw", ...).
    let create_raw = find(&find(members, "Config")["members"], "CreateRaw");
    assert_eq!(create_raw["kind"], "staticmethod");
    assert_eq!(create_raw["function_kind"], "builtin_function_or_method");
    assert!(!create_raw["doc"].is_null());

    // PyGradingData.cpp:94, def_readwrite("red", ...): both accessors, with pybind11's
    // signature lines.
    let red = find(&find(members, "GradingRGBM")["members"], "red");
    assert_eq!(red["kind"], "property");
    assert_eq!(
        (&red["getter"], &red["setter"], &red["deleter"]),
        (&json!(true), &json!(true), &json!(false))
    );
    for k in ["getter_doc", "setter_doc"] {
        let doc = String::from_utf8(bytes(&red[k])).unwrap();
        assert!(
            doc.contains("self: PyOpenColorIO.PyOpenColorIO.GradingRGBM"),
            "{doc}"
        );
    }

    // PyGpuShaderDesc.cpp:66-68 and 253, the class Texture nested in GpuShaderDesc and its
    // def_readonly("width", ...): a getter only.
    let texture = find(&find(members, "GpuShaderDesc")["members"], "Texture");
    assert_eq!(
        String::from_utf8(bytes(&texture["qualname"])).unwrap(),
        "GpuShaderDesc.Texture"
    );
    let width = find(&texture["members"], "width");
    assert_eq!(
        (&width["getter"], &width["setter"]),
        (&json!(true), &json!(false))
    );
    assert!(width.get("getter_doc").is_some() && width.get("setter_doc").is_none());
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
