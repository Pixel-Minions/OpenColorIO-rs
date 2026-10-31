// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The environment of the oracle's processes: an allowlist (`ALLOWED_VARIABLES`,
//! `oracle/ocio_oracle/child_env.py`), never the caller's whole environment, which OCIO would
//! print (a context that loads the whole environment lists every variable in its repr).
//! - the oracle's Python sees only the allowed variables, what the test sets
//!   (`PYTHONDONTWRITEBYTECODE`) and what the interpreter adds itself;
//! - the processes the oracle starts get the allowlisted environment: `child_env.py` gives the
//!   oracle's allowed variables, plus the deliberate ones; the oracle's sources start
//!   processes only with `subprocess.run`, from the modules listed, each call with an `env`
//!   of its own (never `os.environ` or `None`).

use ocio_testkit::Oracle;
use ocio_testkit::oracle::ALLOWED_VARIABLES;
use ocio_testkit::paths;

/// What the interpreter adds to its own environment: uv's launcher on Windows
/// (`PYTHONHOME`, `UV_INTERNAL__PYTHONHOME`), and Python's C-locale coercion on Linux
/// (`LC_CTYPE`, PEP 538).
#[cfg(windows)]
const ADDED_BY_THE_INTERPRETER: &[&str] = &["PYTHONHOME", "UV_INTERNAL__PYTHONHOME"];
#[cfg(not(windows))]
const ADDED_BY_THE_INTERPRETER: &[&str] = &["LC_CTYPE"];

/// The names of `os.environ` in the oracle's Python, and of `child_env.allowed_environment`
/// with a deliberate variable, as `<where>\t<name>` lines. Names only: values are never
/// printed.
const NAMES: &str = r#"
import os
from ocio_oracle import child_env
for name in sorted(os.environ):
    print("oracle", name, sep="\t")
for name in sorted(child_env.allowed_environment({"OCIO_DELIBERATE": "1"})):
    print("child", name, sep="\t")
"#;

fn names(lines: &[String], place: &str) -> Vec<String> {
    lines
        .iter()
        .filter_map(|l| l.strip_prefix(&format!("{place}\t")))
        .map(str::to_string)
        .collect()
}

/// The oracle's Python sees the allowed variables the caller has, `PYTHONDONTWRITEBYTECODE`
/// and the interpreter's own; none of the caller's others (this test's process has `PATH` and
/// Cargo's variables). Its children get the allowed ones and the deliberate ones only.
#[test]
fn the_oracle_and_its_children_see_only_allowed_variables() {
    assert!(std::env::var_os("PATH").is_some() && std::env::var_os("CARGO").is_some());
    let lines = Oracle::get().run_script(NAMES, &[]);
    let oracle = names(&lines, "oracle");
    let child = names(&lines, "child");

    let caller_allowed: Vec<String> = std::env::vars_os()
        .map(|(k, _)| k.to_string_lossy().into_owned())
        .map(|k| {
            if cfg!(windows) {
                k.to_ascii_uppercase()
            } else {
                k
            }
        })
        .filter(|k| ALLOWED_VARIABLES.contains(&k.as_str()))
        .collect();
    for name in &oracle {
        let upper = if cfg!(windows) {
            name.to_ascii_uppercase()
        } else {
            name.clone()
        };
        assert!(
            ALLOWED_VARIABLES.contains(&upper.as_str())
                || upper == "PYTHONDONTWRITEBYTECODE"
                || ADDED_BY_THE_INTERPRETER.contains(&upper.as_str()),
            "the oracle sees {name}: {oracle:?}"
        );
    }
    for name in &caller_allowed {
        assert!(
            oracle.iter().any(|o| o.eq_ignore_ascii_case(name)),
            "{name} didn't reach the oracle: {oracle:?}"
        );
    }
    assert!(oracle.iter().any(|o| o == "PYTHONDONTWRITEBYTECODE"));

    let mut expected: Vec<String> = caller_allowed.clone();
    expected.push("OCIO_DELIBERATE".into());
    expected.sort();
    let mut child = child;
    child.sort();
    assert_eq!(child, expected);
}

/// The modules of the oracle that start processes, each with `subprocess.run`.
const STARTS_PROCESSES: [&str; 11] = [
    "env_api.py",
    "file_rules_api.py",
    "foundations.py",
    "legacy_gpu.py",
    "native_build.py",
    "numerics_lut.py",
    "processor_cache.py",
    "py_eval_api.py",
    "py_suite_api.py",
    "py_surface_api.py",
    "tools_api.py",
];

/// The other ways Python starts a process, which the oracle's sources never use.
const OTHER_WAYS: [&str; 14] = [
    "subprocess.Popen",
    "subprocess.call",
    "subprocess.check_call",
    "subprocess.check_output",
    "subprocess.getoutput",
    "subprocess.getstatusoutput",
    "from subprocess import",
    "import subprocess as",
    "os.system",
    "os.popen",
    "os.exec",
    "os.spawn",
    "os.posix_spawn",
    "multiprocessing",
];

/// The arguments of the call whose text starts at `start` in `source`: up to its closing
/// parenthesis.
fn call_at(source: &str, start: usize) -> &str {
    let mut depth = 0;
    for (i, c) in source[start..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return &source[start..start + i];
                }
            }
            _ => {}
        }
    }
    panic!("an unclosed call at {start}")
}

/// The oracle's sources start processes only with `subprocess.run`, only in the modules
/// listed, and every such call passes an environment of its own (`env=` that isn't the
/// oracle's `os.environ`, a copy of it, or `None`, which inherits it): no process the oracle
/// starts can inherit more than the allowlist.
#[test]
fn every_child_process_of_the_oracle_gets_an_environment() {
    let dir = paths::oracle_dir().join("ocio_oracle");
    let mut starting = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap().flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "py") {
            continue;
        }
        let file = path.file_name().unwrap().to_string_lossy().into_owned();
        let source = std::fs::read_to_string(&path).unwrap();
        for way in OTHER_WAYS {
            assert!(!source.contains(way), "{file}: {way}");
        }
        let calls: Vec<&str> = source
            .match_indices("subprocess.run(")
            .map(|(start, _)| call_at(&source, start))
            .collect();
        if calls.is_empty() {
            continue;
        }
        starting.push(file.clone());
        for call in calls {
            let env = call
                .split_once("env=")
                .unwrap_or_else(|| panic!("{file}: a subprocess.run without env=: {call}"))
                .1;
            for inherited in ["os.environ", "None", "dict(os", "{**os"] {
                assert!(
                    !env.trim_start().starts_with(inherited),
                    "{file}: env={inherited}...: {call}"
                );
            }
        }
    }
    starting.sort();
    assert_eq!(starting, STARTS_PROCESSES);
}
