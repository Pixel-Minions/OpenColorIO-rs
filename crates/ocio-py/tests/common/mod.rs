// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Running the same Python against the port's module and the wheel's.
//!
//! - The port: this test binary embeds the interpreter of target/py-port (the gate's test-py
//!   step and `cargo xtask py-check` build it with ocio-py's `python` feature against it) and
//!   registers the module (`ocio_py::embed`), so no build of the extension is needed.
//! - The wheel: the oracle.
//!
//! Both sides run scripts of steps through the oracle's interpreter of them
//! (`oracle/ocio_oracle/py_eval_steps.py`, O6.2), which reports values, exceptions and
//! docstrings the same way on both.
#![allow(dead_code)]

pub(crate) mod processors;
pub(crate) mod script;

use std::sync::Once;

use ocio_testkit::{Oracle, paths};
use pyo3::prelude::*;
use pyo3::types::PyList;
use serde_json::{Value, json};

static START: Once = Once::new();

/// Starts the embedded interpreter once, with the module registered, the oracle's step
/// interpreter importable, and target/py-port's site-packages (NumPy) on its path.
pub(crate) fn start_python() {
    START.call_once(|| {
        ocio_py::embed::append_to_inittab();
        Python::initialize();
        Python::attach(|py| {
            let path = py
                .import("sys")
                .unwrap()
                .getattr("path")
                .unwrap()
                .cast_into::<PyList>()
                .unwrap();
            // A plain path (not Windows' verbatim \\?\ form, which canonicalize gives).
            let dir = paths::oracle_dir().join("ocio_oracle");
            let dir = dir.to_string_lossy();
            let dir = dir.strip_prefix(r"\\?\").unwrap_or(&dir).to_string();
            path.insert(0, dir).unwrap();
            if let Some(site) = site_packages() {
                path.append(site).unwrap();
            }
        });
    });
}

/// target/py-port's site-packages, from the interpreter PyO3 built against (PYO3_PYTHON).
fn site_packages() -> Option<String> {
    let python = std::env::var_os("PYO3_PYTHON")?;
    let out = std::process::Command::new(python)
        .args([
            "-I",
            "-c",
            "import sysconfig; print(sysconfig.get_path('purelib'))",
        ])
        .output()
        .ok()?;
    Some(String::from_utf8(out.stdout).ok()?.trim().to_string())
}

/// The report of `script` ({"steps": [...]}) against the port's module, as
/// `py_eval_steps.run` makes it, without its "interpreter" flags (an embedded interpreter's).
pub(crate) fn port(script: &Value) -> Value {
    start_python();
    Python::attach(|py| {
        let json = py.import("json").unwrap();
        let steps = py.import("py_eval_steps").unwrap();
        let script = json.call_method1("loads", (script.to_string(),)).unwrap();
        let report = steps
            .call_method1("run", (script,))
            .unwrap_or_else(|e| panic!("{e}"));
        let text: String = json
            .call_method1("dumps", (report,))
            .unwrap()
            .extract()
            .unwrap();
        let mut report: Value = serde_json::from_str(&text).unwrap();
        report.as_object_mut().unwrap().remove("interpreter");
        report
    })
}

/// The wheel's reports of `scripts`, through the oracle's py_eval, without their "interpreter"
/// flags.
pub(crate) fn wheel(scripts: &[Value]) -> Vec<Value> {
    let result = Oracle::get()
        .call("py_eval", json!({ "scripts": scripts }), &[])
        .result;
    result["scripts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let mut r = r.clone();
            assert!(r.get("crashed").is_none(), "{r}");
            r.as_object_mut().unwrap().remove("interpreter");
            r
        })
        .collect()
}

/// The object an expression of the step interpreter makes, in the embedded interpreter: the
/// last step's result of the steps `steps` ({"eval": ...} or {"call": ...}).
pub(crate) fn port_object<'py>(py: Python<'py>, steps: &Value) -> Bound<'py, PyAny> {
    start_python();
    let json = py.import("json").unwrap();
    let interpreter = py.import("py_eval_steps").unwrap();
    let ocio = py.import("PyOpenColorIO").unwrap();
    let script = interpreter
        .getattr("_Script")
        .unwrap()
        .call1((ocio,))
        .unwrap();
    let mut last = py.None().into_bound(py);
    for step in steps.as_array().unwrap() {
        let step = json.call_method1("loads", (step.to_string(),)).unwrap();
        let kind = script.call_method1("check", (&step,)).unwrap();
        last = script
            .call_method1("step", (&step, kind))
            .unwrap_or_else(|e| panic!("{e}"));
        script
            .getattr("results")
            .unwrap()
            .call_method1("append", ((true, &last),))
            .unwrap();
    }
    last
}

/// Text as the step interpreter writes it: {"bytes": hex of its UTF-8}.
pub(crate) fn text(s: &str) -> Value {
    json!({ "str": ocio_testkit::oracle_values::hex(s.as_bytes()) })
}

/// A dotted name of the step interpreter.
pub(crate) fn name(n: &str) -> Value {
    json!({ "name": n })
}

/// A call step.
pub(crate) fn call(target: Value, args: Value) -> Value {
    json!({ "call": target, "args": args })
}

/// The wheel's py_surface report.
pub(crate) fn wheel_surface() -> Value {
    Oracle::get().call("py_surface", json!({}), &[]).result
}

/// The wheel's py_surface entry named `n` among the module's members.
pub(crate) fn wheel_member(report: &Value, n: &str) -> Value {
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
pub(crate) fn port_member(n: &str) -> Value {
    start_python();
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

/// `report` with each spelling of the directory `dir` in its text (the bytes of its
/// {"bytes": hex} values) written as "$FILES", as the oracle's with_files writes the wheel's: as
/// given, without Windows' verbatim prefix, and with "/" and with doubled backslashes for "\\".
pub(crate) fn restore_dir(report: &Value, dir: &std::path::Path) -> Value {
    let given = dir.to_string_lossy().to_string();
    let plain = given.strip_prefix(r"\\?\").unwrap_or(&given).to_string();
    let mut spellings = vec![given.clone(), plain.clone()];
    if cfg!(windows) {
        for p in [&given, &plain] {
            spellings.push(p.replace('\\', "/"));
            spellings.push(p.replace('\\', "\\\\"));
        }
    }
    spellings.sort_by_key(|s| std::cmp::Reverse(s.len()));
    spellings.dedup();
    fn walk(v: &Value, spellings: &[String]) -> Value {
        match v {
            Value::Array(items) => Value::Array(items.iter().map(|i| walk(i, spellings)).collect()),
            Value::Object(o) if o.len() == 1 && o.get("bytes").is_some_and(Value::is_string) => {
                let mut text = ocio_testkit::oracle_values::bytes(v);
                for s in spellings {
                    let mut out = Vec::new();
                    let mut rest = &text[..];
                    while let Some(at) = rest.windows(s.len()).position(|w| w == s.as_bytes()) {
                        out.extend_from_slice(&rest[..at]);
                        out.extend_from_slice(b"$FILES");
                        rest = &rest[at + s.len()..];
                    }
                    out.extend_from_slice(rest);
                    text = out;
                }
                json!({ "bytes": ocio_testkit::oracle_values::hex(&text) })
            }
            Value::Object(o) => Value::Object(
                o.iter()
                    .map(|(k, v)| (k.clone(), walk(v, spellings)))
                    .collect(),
            ),
            other => other.clone(),
        }
    }
    walk(report, &spellings)
}

/// The wheel's reports of `scripts`, each run in a directory holding `files` (relative path,
/// bytes) as its working directory (the oracle's with_files), without their "interpreter"
/// flags.
pub(crate) fn wheel_with_files(scripts: &[Value], files: &[(&str, Vec<u8>)]) -> Vec<Value> {
    let files: serde_json::Map<String, Value> = files
        .iter()
        .map(|(name, bytes)| {
            (
                (*name).to_string(),
                json!({ "bytes": ocio_testkit::oracle_values::hex(bytes) }),
            )
        })
        .collect();
    let result = Oracle::get()
        .call(
            "with_files",
            json!({ "files": files, "command": "py_eval", "args": { "scripts": scripts } }),
            &[],
        )
        .result;
    result["scripts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let mut r = r.clone();
            assert!(r.get("crashed").is_none(), "{r}");
            r.as_object_mut().unwrap().remove("interpreter");
            r
        })
        .collect()
}
