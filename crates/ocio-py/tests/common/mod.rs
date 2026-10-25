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

use std::sync::{Condvar, Mutex, Once, OnceLock};
use std::time::{Duration, Instant};

use ocio_testkit::{Oracle, paths};
use pyo3::prelude::*;
use pyo3::types::PyList;
use serde_json::{Value, json};

static START: Once = Once::new();

/// The port's module as py_surface_walk.py's `surface()` reports it (its `"module"`), walked
/// once, right after the module's import and before any script runs (`start_python`).
static PORT_SURFACE: OnceLock<Value> = OnceLock::new();

/// Starts the embedded interpreter once, with the module registered, the oracle's step
/// interpreter importable, and target/py-port's site-packages (NumPy) on its path; walks the
/// module's surface ([`port_surface`]).
///
/// Call it without holding the GIL (outside `Python::attach`, or inside `py.detach`): a thread
/// that waits here for another thread's start while holding the GIL deadlocks with it.
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
            let ocio = py.import("PyOpenColorIO").unwrap();
            PORT_SURFACE.set(walk_module(py, &ocio)).unwrap();
        });
    });
}

/// py_surface_walk.py's `_module` of the port's module, as its `surface()` starts the walk
/// (`seen` holding the module). The wheel's py_surface walks a module no script has used, in a
/// process of its own; scripts change what a class holds (`copy.copy` makes CPython's
/// `copyreg` add `__slotnames__` to the class, reading `__annotations__` adds it), so the
/// port's module is walked before any script runs, once.
fn walk_module(py: Python<'_>, module: &Bound<'_, PyModule>) -> Value {
    let walk = py.import("py_surface_walk").unwrap();
    let seen = py
        .eval(c"lambda m: {id(m): 'PyOpenColorIO'}", None, None)
        .unwrap()
        .call1((module,))
        .unwrap();
    let report = walk
        .call_method1("_module", (module, "PyOpenColorIO", seen))
        .unwrap_or_else(|e| panic!("{e}"));
    let text: String = py
        .import("json")
        .unwrap()
        .call_method1("dumps", (report,))
        .unwrap()
        .extract()
        .unwrap();
    serde_json::from_str(&text).unwrap()
}

/// The port's module as py_surface_walk.py reports it (the `"module"` of its `surface()`),
/// walked when the interpreter started, before any script ran.
pub(crate) fn port_surface() -> &'static Value {
    start_python();
    PORT_SURFACE.get().unwrap()
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

/// Whether the [`Turn`] is held.
static TURN: Mutex<bool> = Mutex::new(false);
static TURN_FREE: Condvar = Condvar::new();

/// How long [`turn`] waits for another test's turn to end (a turn takes seconds).
const TURN_LIMIT: Duration = Duration::from_secs(600);

/// The turn of one Python run of the port's library at a time in this test binary, held until
/// it drops ([`turn`]).
///
/// The wheel runs each script in a process of its own (the oracle's py_eval); the port's
/// scripts share their test binary's process, whose tests run on threads of their own. What a
/// script changes or reads of the process's state would meet another test's run meanwhile: the
/// logging function (`py_eval_steps.run` sets it for its script, and a message another test
/// logs meanwhile, such as the first current config's "Color management disabled.", would be
/// the log of this script's step), the logging level, the current config, and the library's
/// file cache (another test's `ClearAllCaches` makes a processor read its LUT again).
#[must_use]
pub(crate) struct Turn(());

impl Drop for Turn {
    fn drop(&mut self) {
        *TURN
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = false;
        TURN_FREE.notify_one();
    }
}

/// Waits for the [`Turn`] and takes it. It waits with the GIL released when this thread holds
/// it, since the turn's holder may need it. [`port`] runs each script in its turn; a test whose
/// own Python code changes or reads the process's state that scripts share (the logging level,
/// the current config, the file cache) holds it too. Never on a thread that holds the turn
/// already: it would wait for itself.
pub(crate) fn turn() -> Turn {
    start_python();
    let wait = || {
        let deadline = Instant::now() + TURN_LIMIT;
        let mut held = TURN
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while *held {
            let left = deadline.saturating_duration_since(Instant::now());
            assert!(
                !left.is_zero(),
                "no turn within {TURN_LIMIT:?}: another test's Python run never ended"
            );
            held = TURN_FREE
                .wait_timeout(held, left)
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0;
        }
        *held = true;
        Turn(())
    };
    Python::attach(|py| py.detach(wait))
}

/// The report of `script` ({"steps": [...]}) against the port's module, as
/// `py_eval_steps.run` makes it, without its "interpreter" flags (an embedded interpreter's).
/// It runs in its [`turn`].
pub(crate) fn port(script: &Value) -> Value {
    let _turn = turn();
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
    // The caller holds the GIL: waiting for another thread's start (which runs Python, the
    // module's walk) while holding it would deadlock.
    py.detach(start_python);
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

/// The port's entry for `OCIO.<n>` among the module's members, from the walk of the module
/// made before any script ran ([`port_surface`]), as the wheel's ([`wheel_member`]).
pub(crate) fn port_member(n: &str) -> Value {
    port_surface()["members"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| String::from_utf8(ocio_testkit::oracle_values::bytes(&e["name"])).unwrap() == n)
        .unwrap_or_else(|| panic!("{n} is not a member of the port's module"))
        .clone()
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
