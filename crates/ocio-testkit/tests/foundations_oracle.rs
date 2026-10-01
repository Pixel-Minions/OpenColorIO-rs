// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's FormatMetadata and logging commands (`oracle/ocio_oracle/foundations.py`) against
//! the wheel itself: their refusals, the same replies on every run and in any batch (the
//! commands that change the logging state restore it), clean exits of the processes
//! `logging_environment` starts (a logging function left installed at exit crashes the
//! wheel), and the one composed transform `format_metadata_combine` relies on.
//!
//! The port is compared with these commands in `ocio-ops` (`tests/format_metadata_oracle.rs`,
//! `tests/logging_oracle.rs`).

use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use serde_json::{Value, json};

fn call(cmd: &'static str, args: Value) -> BatchCall<'static> {
    BatchCall {
        cmd,
        args,
        blobs: Vec::new(),
    }
}

/// A config whose LogTransform has an unknown key, which the wheel logs as a warning when it
/// loads the config: a probe of the logging level and function a command leaves behind.
const WARNING_CONFIG: &str = "ocio_profile_version: 2\n\nroles:\n  default: raw\n\n\
     colorspaces:\n  - !<ColorSpace>\n    name: raw\n    \
     from_scene_reference: !<LogTransform> {base: 2, unknown_key: 1}\n";

fn environment_case(env: Option<&str>, custom_function: bool) -> Value {
    json!({
        "env": env,
        "custom_function": custom_function,
        "set_level": "LOGGING_LEVEL_NONE",
        "messages": [["LOGGING_LEVEL_WARNING", "w1\nw2"], ["LOGGING_LEVEL_DEBUG", "d1"]],
        "messages_after_reset": [["LOGGING_LEVEL_WARNING", "after reset"]],
    })
}

/// One call of each command, then a config load that logs a warning.
fn calls() -> Vec<BatchCall<'static>> {
    vec![
        call(
            "format_metadata_ops",
            json!({"scenarios": [[
                {"op": "add_attribute", "path": [], "name": "b", "value": "1"},
                {"op": "add_child_element", "path": [], "name": "A", "value": "v"},
                {"op": "set_element_value", "path": [], "value": "root value"},
                {"op": "get_child_element", "path": [], "index": 1},
            ]]}),
        ),
        call(
            "format_metadata_combine",
            json!({"cases": [{
                "first": {"attributes": [["name", "a"]], "children": [["L", "l"]]},
                "second": {"attributes": [["NAME", "b"]], "children": [["R", "r"]]},
            }]}),
        ),
        call(
            "log_message",
            json!({
                "settings": ["LOGGING_LEVEL_NONE", "LOGGING_LEVEL_DEBUG"],
                "levels": ["LOGGING_LEVEL_WARNING", "LOGGING_LEVEL_UNKNOWN"],
                "messages": ["one\ntwo  ", ""],
            }),
        ),
        call(
            "logging_level_strings",
            json!({"from_string": ["Debug", "2", "x"], "to_string": ["LOGGING_LEVEL_UNKNOWN"]}),
        ),
        call(
            "logging_environment",
            json!({"cases": [environment_case(Some("bogus"), true), environment_case(Some("debug"), false)]}),
        ),
        call(
            "config_serialize",
            json!({"config": {"yaml": WARNING_CONFIG}}),
        ),
    ]
}

#[test]
fn replies_are_the_same_on_every_run_and_in_any_batch() {
    let calls = calls();
    let runs = [
        Oracle::get().batch(&calls, false),
        Oracle::get().batch(&calls, false),
    ];
    for (i, call) in calls.iter().enumerate() {
        let alone = Oracle::get().call_uncached(call.cmd, call.args.clone(), &call.blobs);
        for run in &runs {
            let batched = run[i]
                .as_ref()
                .unwrap_or_else(|e| panic!("call {i} ({}): {e}", call.cmd));
            assert_eq!(batched.result, alone.result, "call {i} ({})", call.cmd);
            assert_eq!(batched.blobs, alone.blobs, "call {i} ({})", call.cmd);
        }
    }
    // The warning config logs its warning at the default level after the other commands,
    // so they restored the level and the logging function.
    let serialized = runs[0][calls.len() - 1].as_ref().unwrap();
    assert_eq!(
        serialized.result["log"].as_array().map(Vec::len),
        Some(1),
        "{}",
        serialized.result
    );
}

#[test]
fn environment_processes_exit_cleanly() {
    let mut cases = Vec::new();
    for env in [None, Some(""), Some("debug"), Some("bogus"), Some("none")] {
        for custom_function in [false, true] {
            cases.push(environment_case(env, custom_function));
        }
    }
    let response = Oracle::get().call("logging_environment", json!({ "cases": cases }), &[]);
    let results = response.result.as_array().expect("a list");
    assert_eq!(results.len(), cases.len());
    for (case, result) in cases.iter().zip(results) {
        assert_eq!(result["returncode"], 0, "{case}: {result}");
        assert!(result["report"].is_object(), "{case}: {result}");
        let stderr = result["stderr"].as_u64().expect("a blob index") as usize;
        assert!(stderr < response.blobs.len(), "{case}: {result}");
    }
}

#[test]
fn combine_leaves_one_composed_matrix() {
    let response = Oracle::get().call(
        "format_metadata_combine",
        json!({"cases": [
            {"first": {"attributes": [], "children": []}, "second": {"attributes": [], "children": []}},
            {"first": {"attributes": [["a", "1"]], "children": [["A", "x"]]},
             "second": {"attributes": [["A", "2"]], "children": [["B", "y"]]}},
        ]}),
        &[],
    );
    for case in response.result.as_array().expect("a list") {
        let transforms = case["transforms"].as_array().expect("transforms");
        assert_eq!(transforms.len(), 1, "{case}");
        assert_eq!(transforms[0]["class"], "MatrixTransform", "{case}");
    }
}

#[test]
fn requests_with_unknown_keys_or_names_are_refused() {
    let refused: Vec<(&'static str, Value, &str)> = vec![
        (
            "format_metadata_ops",
            json!({"scenarios": [], "scenario": []}),
            "unknown keys ['scenario']",
        ),
        (
            "format_metadata_ops",
            json!({"scenarios": [[{"op": "add_attr", "path": [], "name": "a", "value": "b"}]]}),
            "unknown operation",
        ),
        (
            "format_metadata_ops",
            json!({"scenarios": [[{"op": "set_name", "path": [], "id": "a"}]]}),
            "missing keys ['name'], unknown keys ['id']",
        ),
        (
            "format_metadata_ops",
            json!({"scenarios": [[{"op": "get_child_element", "path": [], "index": "0"}]]}),
            "index must be an integer",
        ),
        (
            "format_metadata_combine",
            json!({"cases": [{"first": {"attributes": [], "children": []},
                              "second": {"attributes": [], "child": []}}]}),
            "unknown keys ['child']",
        ),
        (
            "log_message",
            json!({"settings": [], "levels": [], "messages": [], "message": []}),
            "unknown keys ['message']",
        ),
        (
            "log_message",
            json!({"settings": ["LOGGING_LEVEL_LOUD"], "levels": [], "messages": []}),
            "unknown logging level 'LOGGING_LEVEL_LOUD'",
        ),
        (
            "logging_level_strings",
            json!({"from_string": [1], "to_string": []}),
            "must be a string",
        ),
        (
            "logging_environment",
            json!({"cases": [{"env": null, "custom_function": false, "set_level": null,
                              "messages": [], "messages_after_reset": [], "mode": "custom"}]}),
            "unknown keys ['mode']",
        ),
        (
            "logging_environment",
            json!({"cases": [{"env": 3, "custom_function": false, "set_level": null,
                              "messages": [], "messages_after_reset": []}]}),
            "must be a string",
        ),
        // Shapes: a string where a list is expected, a two-character string where a pair is,
        // and a bool or a string as an index.
        (
            "format_metadata_ops",
            json!({"scenarios": "ab"}),
            "scenarios must be a list",
        ),
        (
            "format_metadata_ops",
            json!({"scenarios": [[{"op": "clear", "path": "0"}]]}),
            "path must be a list",
        ),
        (
            "format_metadata_ops",
            json!({"scenarios": [[{"op": "clear", "path": [true]}]]}),
            "must be an integer, not True",
        ),
        (
            "format_metadata_ops",
            json!({"scenarios": [[{"op": "get_child_element", "path": [], "index": true}]]}),
            "must be an integer, not True",
        ),
        (
            "format_metadata_combine",
            json!({"cases": "ab"}),
            "cases must be a list",
        ),
        (
            "format_metadata_combine",
            json!({"cases": [{"first": {"attributes": ["ab"], "children": []},
                              "second": {"attributes": [], "children": []}}]}),
            "must be a list of two items",
        ),
        (
            "format_metadata_combine",
            json!({"cases": [{"first": {"attributes": [], "children": "ab"},
                              "second": {"attributes": [], "children": []}}]}),
            "children must be a list",
        ),
        (
            "log_message",
            json!({"settings": "LOGGING_LEVEL_INFO", "levels": [], "messages": []}),
            "settings must be a list",
        ),
        (
            "log_message",
            json!({"settings": [], "levels": [], "messages": "ab"}),
            "messages must be a list",
        ),
        (
            "logging_level_strings",
            json!({"from_string": "info", "to_string": []}),
            "from_string must be a list",
        ),
        (
            "logging_environment",
            json!({"cases": "ab"}),
            "cases must be a list",
        ),
        (
            "logging_environment",
            json!({"cases": [{"env": null, "custom_function": false, "set_level": null,
                              "messages": ["ab"], "messages_after_reset": []}]}),
            "must be a list of two items",
        ),
    ];
    let calls: Vec<BatchCall<'_>> = refused
        .iter()
        .map(|(cmd, args, _)| call(cmd, args.clone()))
        .collect();
    for ((cmd, args, fragment), result) in refused.iter().zip(Oracle::get().batch(&calls, false)) {
        match result {
            Ok(response) => panic!("{cmd} {args} was accepted: {}", response.result),
            Err(error) => assert!(error.contains(fragment), "{cmd} {args}: {error}"),
        }
    }
}

/// Runs `logging_environment` in a Python process whose environment has `PYTHON*` variables
/// that change what Python writes to stderr, then again without them, and prints whether each
/// case's stderr bytes and report are the same.
const ISOLATED_CHILD: &str = r#"
import json, os, sys
from ocio_oracle.foundations import logging_environment
cases = json.loads(sys.argv[1])
plain, plain_err = logging_environment({"cases": cases}, [])
os.environ["PYTHONVERBOSE"] = "1"
os.environ["PYTHONWARNINGS"] = "error"
os.environ["PYTHONDEVMODE"] = "1"
noisy, noisy_err = logging_environment({"cases": cases}, [])
for a, b, ea, eb in zip(plain, noisy, plain_err, noisy_err):
    print(json.dumps({"same_stderr": ea == eb, "same_report": a["report"] == b["report"],
                      "returncodes": [a["returncode"], b["returncode"]]}))
"#;

/// The process `logging_environment` starts ignores the `PYTHON*` variables of the oracle's
/// environment (`python -I`): `PYTHONVERBOSE`, `PYTHONWARNINGS` and `PYTHONDEVMODE` change
/// neither its stderr bytes nor its report.
#[test]
fn the_environment_process_ignores_python_variables() {
    let cases = vec![
        environment_case(None, false),
        environment_case(Some("debug"), false),
        environment_case(Some("bogus"), true),
    ];
    let lines = Oracle::get().run_script(
        ISOLATED_CHILD,
        &[serde_json::to_string(&cases).expect("JSON")],
    );
    assert_eq!(lines.len(), cases.len(), "{lines:?}");
    for (case, line) in cases.iter().zip(&lines) {
        let result: Value = serde_json::from_str(line).expect("the script's JSON");
        assert_eq!(
            result,
            json!({"same_stderr": true, "same_report": true, "returncodes": [0, 0]}),
            "{case}"
        );
    }
}
