// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `None` as the `self` of a method (U-134, `crates/ocio-py/src/self_kind.rs`) against the
//! wheel:
//! - the table of unnamed overloads is exactly the wheel's: every method overload of its
//!   classes whose signature names no parameter (`self`, `arg0`, ...), constructors aside,
//!   by class path, name and index; the parameters of those whose `self` is a pointer or a
//!   holder are of the types the dispatcher checks (`str` and classes);
//! - a probe per kind, each in its own wheel process (the oracle's py_eval): a pointer's and a
//!   holder's `self` (with no other argument, a `str`, a class instance) kills the wheel's
//!   process, where the port raises its ValueError; a reference's, a named overload's, an
//!   overload whose other argument doesn't load, and an overloaded function's zero-argument
//!   overload (no second pass) raise the wheel's TypeError, which the port's matches;
//! - every listed overload the port binds: `None` and arguments of its types give the port's
//!   ValueError for a pointer or a holder, pybind11's TypeError for a reference.
#![cfg(feature = "python")]

mod common;

use std::collections::BTreeSet;

use common::script::{Expect, Script, compare, i, n, none, r, s};
use ocio_py::self_kind::{SelfKind, UNNAMED};
use ocio_testkit::Oracle;
use ocio_testkit::oracle_values::bytes;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::{Value, json};

fn text(v: &Value) -> String {
    String::from_utf8(bytes(v)).unwrap()
}

/// An unnamed overload of the wheel: class path, name, index, parameter types after `self`.
type Unnamed = (String, String, usize, Vec<String>);

/// `text` split at the commas outside brackets.
fn split_params(text: &str) -> Vec<String> {
    let (mut out, mut cur, mut depth) = (Vec::new(), String::new(), 0i32);
    for c in text.chars() {
        match c {
            '[' | '(' | '<' | '{' => depth += 1,
            ']' | ')' | '>' | '}' => depth -= 1,
            ',' if depth == 0 => {
                out.push(std::mem::take(&mut cur).trim().to_string());
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

/// The method overloads of the classes under `members` (recursively) whose signatures name no
/// parameter.
fn unnamed(members: &Value, prefix: &str, out: &mut Vec<Unnamed>) {
    for m in members.as_array().unwrap() {
        let name = text(&m["name"]);
        if m["kind"] == "class" {
            if m.get("entries").is_some() {
                continue;
            }
            let path = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}.{name}")
            };
            unnamed(&m["members"], &path, out);
            continue;
        }
        if prefix.is_empty() || m["kind"] != "instancemethod" || name == "__init__" {
            continue;
        }
        // pybind11's own `_pybind11_conduit_v1_` has no docstring: its `self` is a handle.
        if m["doc"].is_null() {
            assert_eq!(name, "_pybind11_conduit_v1_", "{prefix}");
            continue;
        }
        let doc = text(&m["doc"]);
        let mut k = 0;
        for line in doc.lines() {
            let rest = match line.split_once(". ") {
                Some((num, rest)) if num.chars().all(|c| c.is_ascii_digit()) => rest,
                _ => line,
            };
            let Some(sig) = rest.strip_prefix(&format!("{name}(")) else {
                continue;
            };
            if sig.starts_with("*args, **kwargs)") {
                continue;
            }
            let end = sig.rfind(") -> ").unwrap_or_else(|| panic!("{line}"));
            let params = split_params(&sig[..end]);
            assert!(params[0].starts_with("self"), "{prefix}.{name}: {line}");
            let rest: Vec<&String> = params.iter().skip(1).collect();
            let is_unnamed = rest
                .iter()
                .enumerate()
                .all(|(j, p)| p.split(':').next() == Some(&format!("arg{j}")));
            if is_unnamed {
                let types = rest
                    .iter()
                    .map(|p| p.split_once(": ").unwrap().1.to_string())
                    .collect();
                out.push((prefix.to_string(), name.clone(), k, types));
            }
            k += 1;
        }
    }
}

fn wheel_unnamed() -> Vec<Unnamed> {
    let report = common::wheel_surface();
    let mut out = Vec::new();
    unnamed(&report["module"]["members"], "", &mut out);
    out
}

/// The table is the wheel's unnamed overloads, exactly; the parameters of the pointers and
/// holders of the classes the port binds are of the types the dispatcher loads (`str` and
/// classes; a later card binding a class adds its types to `dispatch.rs`'s `rest_loads`).
#[test]
fn the_table_is_the_wheels_unnamed_overloads() {
    let wheel = wheel_unnamed();
    let bound = bound_classes();
    let theirs: BTreeSet<(String, String, usize)> = wheel
        .iter()
        .map(|(c, n, k, _)| (c.clone(), n.clone(), *k))
        .collect();
    let ours: BTreeSet<(String, String, usize)> = UNNAMED
        .iter()
        .map(|(c, n, k, _)| (c.to_string(), n.to_string(), usize::from(*k)))
        .collect();
    assert_eq!(ours.len(), UNNAMED.len(), "duplicate entries");
    let missing: Vec<_> = theirs.difference(&ours).collect();
    let extra: Vec<_> = ours.difference(&theirs).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "missing {missing:?}\nextra {extra:?}"
    );
    assert!(ours.len() > 700, "{}", ours.len());
    let (mut pointers, mut holders) = (0, 0);
    for (c, n, k, kind) in UNNAMED {
        let types = &wheel
            .iter()
            .find(|(wc, wn, wk, _)| wc == c && wn == n && *wk == usize::from(*k))
            .unwrap()
            .3;
        if matches!(kind, SelfKind::Pointer | SelfKind::Holder) && bound.contains(c) {
            for t in types {
                assert!(
                    t == "str" || t.starts_with("PyOpenColorIO.PyOpenColorIO."),
                    "{c}.{n}: {t}"
                );
            }
            pointers += usize::from(*kind == SelfKind::Pointer);
            holders += usize::from(*kind == SelfKind::Holder);
        }
    }
    assert!(pointers > 140 && holders > 50, "{pointers} {holders}");
}

/// The class paths of the table the port's module binds.
fn bound_classes() -> BTreeSet<&'static str> {
    common::start_python();
    Python::attach(|py| {
        let ocio = py.import("PyOpenColorIO").unwrap().into_any();
        UNNAMED
            .iter()
            .map(|e| e.0)
            .filter(|c| {
                let mut target = ocio.clone();
                for part in c.split('.') {
                    match target.getattr(part) {
                        Ok(t) => target = t,
                        Err(_) => return false,
                    }
                }
                // A declared class the port doesn't bind yet has only bindPyTypes' members.
                target.hasattr("__repr__").unwrap()
                    && UNNAMED.iter().any(|(uc, un, _, _)| {
                        uc == c && target.getattr("__dict__").unwrap().contains(*un).unwrap()
                    })
            })
            .collect()
    })
}

/// One probe: a script whose last step calls with `None` as `self`.
struct Probe {
    script: Script,
    /// The wheel's process dies.
    dies: bool,
}

fn probe(setup: impl FnOnce(&mut Script) -> Vec<Value>, target: &str, dies: bool) -> Probe {
    let mut sc = Script::new();
    let mut args = vec![none()];
    args.extend(setup(&mut sc));
    let expect = Expect::Raised;
    sc.step(json!({ "call": n(target), "args": args }), expect);
    Probe { script: sc, dies }
}

fn probes() -> Vec<Probe> {
    vec![
        // A pointer: no other argument; a `const char *`.
        probe(|_| vec![], "OCIO.Context.getSearchPath", true),
        probe(|_| vec![], "OCIO.ColorSpace.getName", true),
        probe(|_| vec![s("a")], "OCIO.PyConfigIOProxy.getLutData", true),
        // A holder: no other argument (defRepr); a holder; a `const char *`.
        probe(|_| vec![], "OCIO.Context.__repr__", true),
        probe(
            |sc| vec![r(sc.call(n("OCIO.ColorSpaceSet"), vec![]))],
            "OCIO.ColorSpaceSet.__eq__",
            true,
        ),
        probe(|_| vec![s("out.ocioz")], "OCIO.Config.archive", true),
        // A reference.
        probe(|_| vec![], "OCIO.vector_of_uint8_t.__len__", false),
        probe(
            |_| vec![],
            "OCIO.Context.StringVarNameIterator.__len__",
            false,
        ),
        probe(
            |_| vec![i(0)],
            "OCIO.Config.ColorSpaceIterator.__getitem__",
            false,
        ),
        // Named: `self` refuses None.
        probe(|_| vec![s("x")], "OCIO.Context.setSearchPath", false),
        // The other argument doesn't load.
        probe(|_| vec![i(1)], "OCIO.PyConfigIOProxy.getLutData", false),
        // An overloaded function's zero-argument unnamed overload: no second pass.
        probe(|_| vec![], "OCIO.Config.getColorSpaces", false),
    ]
}

/// A probe per kind, each in its own wheel process: the pointers and holders kill it, where
/// the port raises U-134's ValueError; the others raise the wheel's TypeError, as the port.
#[test]
fn a_probe_per_kind_against_the_wheel() {
    let probes = probes();
    let scripts: Vec<Value> = probes.iter().map(|p| p.script.json()).collect();
    let theirs = Oracle::get()
        .call("py_eval", json!({ "scripts": scripts }), &[])
        .result;
    let theirs = theirs["scripts"].as_array().unwrap();
    assert_eq!(theirs.len(), probes.len());
    let mut alive = Vec::new();
    for (p, t) in probes.iter().zip(theirs) {
        let step = p.script.steps.last().unwrap();
        if p.dies {
            let code = &t["crashed"]["returncode"];
            assert!(
                code.as_i64().is_some_and(|c| c != 0),
                "{step}: the wheel's process lives: {t}"
            );
            let ours = common::port(&p.script.json());
            let last = ours["steps"].as_array().unwrap().last().unwrap().clone();
            assert_eq!(
                last["raised"]["type"], "builtins.ValueError",
                "{step}: {last}"
            );
            assert!(
                text(&last["raised"]["message"]).contains("docs/improvements.md, U-134)"),
                "{step}: {last}"
            );
        } else {
            assert!(t.get("crashed").is_none(), "{step}: {t}");
            let raised = &t["steps"].as_array().unwrap().last().unwrap()["raised"];
            assert_eq!(raised["type"], "builtins.TypeError", "{step}: {t}");
            alive.push(Script {
                steps: p.script.steps.clone(),
                expect: p.script.expect.clone(),
                ..Script::default()
            });
        }
    }
    assert_eq!(alive.len(), 6);
    compare(&alive);
}

/// The values the port's sweep passes for parameters of these types, or none.
fn argument(ty: &str) -> Option<&'static str> {
    match ty {
        "str" => Some("'a'"),
        "typing.SupportsInt" => Some("0"),
        "PyOpenColorIO.PyOpenColorIO.ColorSpaceSet" => Some("OCIO.ColorSpaceSet()"),
        "PyOpenColorIO.PyOpenColorIO.vector_of_uint8_t" => Some("OCIO.vector_of_uint8_t()"),
        _ => None,
    }
}

/// Every listed overload the port binds, called with `None` and arguments of its types: the
/// port's ValueError for a pointer or a holder (but where an earlier overload of the function
/// takes the call), pybind11's TypeError for a reference.
#[test]
fn every_bound_overload_with_none_as_self() {
    let wheel = wheel_unnamed();
    common::start_python();
    let (mut errors, mut refused, mut skipped) = (0, 0, 0);
    Python::attach(|py| {
        let ocio = py.import("PyOpenColorIO").unwrap();
        for (c, name, k, kind) in UNNAMED {
            let mut target = ocio.clone().into_any();
            let mut bound = true;
            for part in c.split('.') {
                match target.getattr(part) {
                    Ok(t) => target = t,
                    Err(_) => bound = false,
                }
                if !bound {
                    break;
                }
            }
            if !bound || !target.getattr("__dict__").unwrap().contains(*name).unwrap() {
                continue;
            }
            let types = &wheel
                .iter()
                .find(|(wc, wn, wk, _)| wc == c && wn == name && *wk == usize::from(*k))
                .unwrap()
                .3;
            let args: Option<Vec<&str>> = types.iter().map(|t| argument(t)).collect();
            let Some(args) = args else {
                skipped += 1;
                continue;
            };
            // Only the overloads a call with these arguments reaches: the first whose types
            // they are (pybind11 tries the overloads in order).
            let first = wheel
                .iter()
                .filter(|(wc, wn, _, _)| wc == c && wn == name)
                .map(|(_, _, wk, t)| (*wk, t))
                .min_by_key(|(wk, _)| *wk)
                .unwrap();
            if first.0 != usize::from(*k) && first.1 == types {
                skipped += 1;
                continue;
            }
            let mut call_args = vec!["None"];
            call_args.extend(args);
            let code = format!("OCIO.{c}.{name}({})", call_args.join(", "));
            let globals = PyDict::new(py);
            globals.set_item("OCIO", &ocio).unwrap();
            let src = std::ffi::CString::new(format!("r = {code}")).unwrap();
            let outcome = py.run(&src, Some(&globals), None);
            match kind {
                SelfKind::Pointer | SelfKind::Holder => {
                    let err = outcome.expect_err(&code);
                    let t = err.to_string();
                    if t.starts_with("TypeError") && t.contains("incompatible") {
                        // An overloaded function whose zero-argument overload gets no second
                        // pass (`getColorSpaces()`): pybind11 never loads None there.
                        assert!(types.is_empty(), "{code}: {t}");
                        refused += 1;
                    } else {
                        assert!(
                            t.starts_with("ValueError") && t.contains("U-134"),
                            "{code}: {t}"
                        );
                        errors += 1;
                    }
                }
                SelfKind::Reference => {
                    // pybind11's TypeError; an operator's NotImplemented.
                    match outcome {
                        Ok(()) => {
                            let v = globals.get_item("r").unwrap().unwrap();
                            assert!(
                                name.starts_with("__") && v.is(py.NotImplemented()),
                                "{code}"
                            );
                        }
                        Err(err) => {
                            let t = err.to_string();
                            assert!(t.starts_with("TypeError"), "{code}: {t}");
                        }
                    }
                    refused += 1;
                }
            }
        }
    });
    // The classes bound so far (p6-config): later cards add theirs.
    assert!(errors >= 231, "{errors}");
    assert!(refused >= 231, "{refused}");
    assert!(skipped <= 2, "{skipped}");
}
