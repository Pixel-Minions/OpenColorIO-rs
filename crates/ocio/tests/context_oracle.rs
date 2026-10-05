// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `ocio::Context` against the wheel's, through the oracle's `context_calls`: the same
//! operations on both, then everything the context says compared byte for byte: its cache ID,
//! `operator<<` (repr), search paths, working directory, environment mode and variables; the
//! strings and file locations it resolves, with the variables they used, and the exceptions
//! (`ExceptionMissingFile` and its message) where a file isn't found. Files live in a directory
//! of this checkout's target directory, given to both by its absolute path; the environment is
//! the same on both (the oracle's holds exactly the request's variables).

use std::sync::Arc;

use ocio::Context;
use ocio_ops::open_color_types::EnvironmentMode;
use ocio_ops::platform::{EnvProvider, MapEnv, set_env_provider};
use ocio_testkit::Oracle;
use ocio_testkit::oracle_values::{bytes, result, result_bytes};
use ocio_testkit::paths::target_dir;
use serde_json::{Value, json};

/// One operation, on the port's context and as a `context_calls` call.
#[derive(Clone)]
enum Op {
    SetSearchPath(Vec<u8>),
    AddSearchPath(Vec<u8>),
    ClearSearchPaths,
    SetWorkingDir(Vec<u8>),
    SetEnvironmentMode(EnvironmentMode),
    LoadEnvironment,
    SetStringVar(Vec<u8>, Option<Vec<u8>>),
    ClearStringVars,
    /// `copy.deepcopy`: createEditableCopy, which the next operations act on.
    Copy,
    ResolveStringVar(Vec<u8>),
    ResolveFileLocation(Vec<u8>),
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn text(bytes: &[u8]) -> Value {
    json!({"bytes": hex(bytes)})
}

fn mode_name(mode: EnvironmentMode) -> &'static str {
    match mode {
        EnvironmentMode::Unknown => "ENV_ENVIRONMENT_UNKNOWN",
        EnvironmentMode::LoadPredefined => "ENV_ENVIRONMENT_LOAD_PREDEFINED",
        EnvironmentMode::LoadAll => "ENV_ENVIRONMENT_LOAD_ALL",
    }
}

/// The wheel's calls for the operations: after each, a check of the context's state; for each
/// resolution, a new used context and its variables.
fn calls(ops: &[Op]) -> Vec<Value> {
    let mut out = Vec::new();
    let mut on = "context".to_string();
    let mut copies = 0;
    for op in ops {
        match op {
            Op::SetSearchPath(p) => {
                out.push(json!({"call": "setSearchPath", "on": on, "args": [text(p)]}))
            }
            Op::AddSearchPath(p) => {
                out.push(json!({"call": "addSearchPath", "on": on, "args": [text(p)]}))
            }
            Op::ClearSearchPaths => out.push(json!({"call": "clearSearchPaths", "on": on})),
            Op::SetWorkingDir(p) => {
                out.push(json!({"call": "setWorkingDir", "on": on, "args": [text(p)]}))
            }
            Op::SetEnvironmentMode(m) => out.push(json!({"call": "setEnvironmentMode",
                "on": on, "args": [{"enum": mode_name(*m)}]})),
            Op::LoadEnvironment => out.push(json!({"call": "loadEnvironment", "on": on})),
            Op::SetStringVar(n, v) => out.push(json!({"call": "setStringVar", "on": on,
                "args": [text(n), v.as_ref().map_or(Value::Null, |v| text(v))]})),
            Op::ClearStringVars => out.push(json!({"call": "clearStringVars", "on": on})),
            Op::Copy => {
                copies += 1;
                let name = format!("copy{copies}");
                out.push(json!({"copy": on, "as": name}));
                on = name;
            }
            Op::ResolveStringVar(s) => {
                out.push(json!({"new": "Context", "as": "used"}));
                out.push(json!({"call": "resolveStringVar", "on": on,
                                "args": [text(s), {"ref": "used"}]}));
                out.push(json!({"call": "getStringVars", "on": "used"}));
            }
            Op::ResolveFileLocation(s) => {
                out.push(json!({"new": "Context", "as": "used"}));
                out.push(json!({"call": "resolveFileLocation", "on": on,
                                "args": [text(s), {"ref": "used"}]}));
                out.push(json!({"call": "getStringVars", "on": "used"}));
            }
        }
        for getter in [
            "getCacheID",
            "__repr__",
            "getSearchPath",
            "getSearchPaths",
            "getWorkingDir",
            "getEnvironmentMode",
            "getStringVars",
        ] {
            out.push(json!({"call": getter, "on": on}));
        }
    }
    out
}

/// The result of a wheel call as bytes (text, or the bytes it couldn't decode).
fn wheel_text(v: &Value) -> Vec<u8> {
    result_bytes(v)
}

fn wheel_pairs(v: &Value) -> Vec<(Vec<u8>, Vec<u8>)> {
    result(v)
        .as_array()
        .unwrap_or_else(|| panic!("{v}"))
        .iter()
        .map(|p| (bytes(&p[0]), bytes(&p[1])))
        .collect()
}

fn port_pairs(c: &Context) -> Vec<(Vec<u8>, Vec<u8>)> {
    (0..c.num_string_vars())
        .map(|i| {
            (
                c.string_var_name_by_index(i).to_vec(),
                c.string_var_by_index(i).to_vec(),
            )
        })
        .collect()
}

/// Runs the operations on both and compares everything after each.
fn check(name: &str, env: &[(&str, &str)], ops: &[Op]) {
    let env_json: serde_json::Map<String, Value> =
        env.iter().map(|(k, v)| (k.to_string(), json!(v))).collect();
    let response = Oracle::get().call(
        "context_calls",
        json!({"context": "new", "env": env_json, "calls": calls(ops)}),
        &[],
    );
    assert_eq!(response.result["context"], Value::Null, "{name}");
    let wheel = response.result["calls"].as_array().unwrap();
    let mut w = wheel.iter();

    // The oracle sets the request's variables in its process one by one, with the platform's
    // rules: on Windows a variable set to "" is removed. So are the port's.
    let port_env = MapEnv::default();
    for (name, value) in env {
        port_env.set_var(name.as_bytes(), value.as_bytes());
    }
    set_env_provider(Some(Arc::new(port_env)));
    let mut context = Context::new();
    for (i, op) in ops.iter().enumerate() {
        let what = format!("{name}, operation {i}");
        match op {
            Op::SetSearchPath(p) => context.set_search_path(p),
            Op::AddSearchPath(p) => context.add_search_path(p),
            Op::ClearSearchPaths => context.clear_search_paths(),
            Op::SetWorkingDir(p) => context.set_working_dir(p),
            Op::SetEnvironmentMode(m) => context.set_environment_mode(*m),
            Op::LoadEnvironment => context.load_environment(),
            Op::SetStringVar(n, v) => context.set_string_var(n, v.as_deref()),
            Op::ClearStringVars => context.clear_string_vars(),
            Op::Copy => context = context.clone(),
            Op::ResolveStringVar(s) => {
                let mut used = Context::new();
                let r = context.resolve_string_var_with_used(s, &mut used);
                w.next();
                assert_eq!(r, wheel_text(w.next().unwrap()), "{what}");
                assert_eq!(port_pairs(&used), wheel_pairs(w.next().unwrap()), "{what}");
            }
            Op::ResolveFileLocation(s) => {
                let mut used = Context::new();
                let r = context.resolve_file_location_with_used(s, &mut used);
                w.next();
                let call = w.next().unwrap();
                match r {
                    Ok(path) => assert_eq!(path, wheel_text(call), "{what}: {call}"),
                    Err(e) => {
                        let exc = &call["exception"];
                        assert_eq!(
                            exc["type"],
                            if e.is_missing_file() {
                                "ExceptionMissingFile"
                            } else {
                                "Exception"
                            },
                            "{what}: {call}"
                        );
                        assert_eq!(e.what(), bytes(&exc["message"]), "{what}");
                    }
                }
                assert_eq!(port_pairs(&used), wheel_pairs(w.next().unwrap()), "{what}");
            }
        }
        if matches!(
            op,
            Op::SetStringVar(..)
                | Op::ClearStringVars
                | Op::Copy
                | Op::LoadEnvironment
                | Op::SetSearchPath(_)
                | Op::AddSearchPath(_)
                | Op::ClearSearchPaths
                | Op::SetWorkingDir(_)
                | Op::SetEnvironmentMode(_)
        ) {
            // The calls of the operation itself.
            w.next();
        }
        assert_eq!(
            context.cache_id().as_bytes(),
            wheel_text(w.next().unwrap()),
            "{what}: cache ID"
        );
        assert_eq!(
            context.to_bytes(),
            wheel_text(w.next().unwrap()),
            "{what}: repr"
        );
        assert_eq!(
            context.search_path(),
            wheel_text(w.next().unwrap()),
            "{what}"
        );
        let paths: Vec<Vec<u8>> = (0..context.num_search_paths())
            .map(|i| context.search_path_with_index(i).to_vec())
            .collect();
        let wheel_paths: Vec<Vec<u8>> = result(w.next().unwrap())
            .as_array()
            .unwrap()
            .iter()
            .map(bytes)
            .collect();
        assert_eq!(paths, wheel_paths, "{what}");
        assert_eq!(
            context.working_dir(),
            wheel_text(w.next().unwrap()),
            "{what}"
        );
        assert_eq!(
            json!({"enum": mode_name(context.environment_mode())}),
            w.next().unwrap()["result"],
            "{what}"
        );
        assert_eq!(
            port_pairs(&context),
            wheel_pairs(w.next().unwrap()),
            "{what}"
        );
    }
    set_env_provider(None);
}

fn b(s: &str) -> Vec<u8> {
    s.as_bytes().to_vec()
}

#[test]
fn contexts_match_the_wheel() {
    // The files the contexts look for.
    let root = target_dir().join("context_oracle_files");
    for file in [
        "luts/a.spi1d",
        "shots/s01/luts/b.spi1d",
        "shots/s01/c.cube",
        "d.clf",
    ] {
        let path = root.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"x").unwrap();
    }
    let root_text = root.to_str().unwrap().to_string();
    let root_text = root_text
        .strip_prefix(r"\\?\")
        .unwrap_or(&root_text)
        .to_string();
    let dir = |rel: &str| b(&format!("{root_text}/{rel}"));

    check(
        "setters",
        &[],
        &[
            Op::SetSearchPath(b("a:b::c")),
            Op::AddSearchPath(b("")),
            Op::AddSearchPath(b("d")),
            Op::SetWorkingDir(b("/w d")),
            Op::SetStringVar(b("NAME"), Some(b("v"))),
            Op::SetStringVar(b("LONGER_NAME"), Some(b("w"))),
            Op::SetStringVar(b("A"), Some(b("x"))),
            Op::SetStringVar(b("NAME"), Some(b("v"))),
            Op::SetStringVar(b(""), Some(b("ignored"))),
            Op::SetStringVar(b("A"), None),
            Op::SetEnvironmentMode(EnvironmentMode::LoadAll),
            Op::SetEnvironmentMode(EnvironmentMode::Unknown),
            Op::ClearSearchPaths,
            Op::ClearStringVars,
        ],
    );

    check(
        "resolve strings",
        &[],
        &[
            Op::SetStringVar(b("TEST1"), Some(b("foo.bar"))),
            Op::SetStringVar(b("TEST1NG"), Some(b("bar.foo"))),
            Op::SetStringVar(b("FOO_foo.bar"), Some(b("cheese"))),
            Op::ResolveStringVar(b(
                "/a/b/${TEST1}/${TEST1NG}/%TEST1%/$TEST1NG/${FOO_${TEST1}}/",
            )),
            Op::ResolveStringVar(b(
                "/a/b/${TEST1}/${TEST1NG}/%TEST1%/$TEST1NG/${FOO_${TEST1}}/",
            )),
            Op::ResolveStringVar(b("$MISSING %TEST1 TEST1% ${TEST1")),
            Op::ResolveStringVar(b("")),
            Op::SetStringVar(b("LOOP"), Some(b("a$LOOP"))),
            Op::ResolveStringVar(b("x$LOOPy")),
        ],
    );

    check(
        "environment",
        &[
            ("SHOT", "s01"),
            ("Other", "o"),
            ("EMPTY", ""),
            ("EQ", "1=2=3"),
        ],
        &[
            Op::SetStringVar(b("SHOT"), Some(b("default"))),
            Op::SetStringVar(b("NOT_IN_ENV"), Some(b("kept"))),
            Op::LoadEnvironment,
            Op::SetEnvironmentMode(EnvironmentMode::LoadAll),
            Op::LoadEnvironment,
            Op::Copy,
            Op::LoadEnvironment,
        ],
    );

    check(
        "files",
        &[],
        &[
            Op::SetWorkingDir(dir("")),
            Op::ResolveFileLocation(b("d.clf")),
            Op::ResolveFileLocation(b("missing.clf")),
            Op::AddSearchPath(b("luts")),
            Op::AddSearchPath(b("shots/$SHOT/luts/ ")),
            Op::AddSearchPath(dir("shots/$SHOT")),
            Op::SetStringVar(b("SHOT"), Some(b("s01"))),
            Op::ResolveFileLocation(b("a.spi1d")),
            Op::ResolveFileLocation(b("b.spi1d")),
            Op::ResolveFileLocation(b("b.spi1d")),
            Op::ResolveFileLocation(b("c.cube")),
            Op::ResolveFileLocation(b("missing.spi1d")),
            Op::SetStringVar(b("FILE"), Some(b("a.spi1d"))),
            Op::ResolveFileLocation(b("$FILE")),
            Op::ResolveFileLocation(dir("d.clf")),
            Op::ResolveFileLocation(dir("./luts/../d.clf")),
            Op::ResolveFileLocation(dir("missing.clf")),
            Op::ResolveFileLocation(b("")),
        ],
    );

    // Windows' verbatim and device paths, which `_wstat` opens, and a drive letter alone, which
    // it refuses; on Linux, names like any other.
    const VERBATIM: &str = "\\\\?\\";
    const DEVICE: &str = "\\\\.\\";
    let sep = if cfg!(windows) { "\\" } else { "/" };
    check(
        "drives and verbatim paths",
        &[],
        &[
            Op::ResolveFileLocation(b(&format!("{VERBATIM}{root_text}{sep}d.clf"))),
            Op::ResolveFileLocation(b(&format!("{VERBATIM}{root_text}{sep}missing.clf"))),
            Op::ResolveFileLocation(b(&format!("{DEVICE}{root_text}{sep}d.clf"))),
            Op::ResolveFileLocation(b("C:")),
            Op::ResolveFileLocation(b("C:.")),
            Op::SetWorkingDir(dir("")),
            Op::ResolveFileLocation(b("C:")),
            Op::ResolveFileLocation(b("nul")),
        ],
    );
}
