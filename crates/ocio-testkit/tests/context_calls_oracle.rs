// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's `context_calls` command (`oracle/ocio_oracle/context_api.py`), against the
//! wheel itself: each context source, file resolution against the request's files with a used
//! context, the request's environment, and the requests it refuses.

use ocio_testkit::Oracle;
use serde_json::{Value, json};

fn call(args: Value) -> Value {
    Oracle::get().call("context_calls", args, &[]).result
}

fn calls(response: &Value) -> &Vec<Value> {
    assert_eq!(response["context"], Value::Null, "{response}");
    response["calls"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
}

/// A search path finds a file the request wrote, the used context records the variable that
/// led there, and a file that isn't there raises ExceptionMissingFile.
#[test]
fn files_resolve_with_the_used_context() {
    let response = call(json!({
        "context": {"new": {"searchPaths": ["$SHOT/luts"],
                            "stringVars": {"map": [["SHOT", "s01"]]}}},
        "files": {"s01/luts/a.spi1d": "x"},
        "calls": [
            {"new": "Context", "as": "used"},
            {"call": "resolveFileLocation", "args": ["a.spi1d", {"ref": "used"}]},
            {"call": "getStringVars", "on": "used"},
            {"call": "resolveFileLocation", "args": ["missing.spi1d"]},
        ],
    }));
    let c = calls(&response);
    let found = c[1]["result"]
        .as_str()
        .unwrap_or_else(|| panic!("{response}"));
    assert!(found.ends_with("a.spi1d"), "{found}");
    assert_eq!(c[2]["result"], json!([["SHOT", "s01"]]));
    assert_eq!(
        c[3]["exception"]["type"], "ExceptionMissingFile",
        "{response}"
    );
}

/// A context made new loads nothing; loadEnvironment updates its predefined variables from the
/// request's environment, which holds exactly the request's variables.
#[test]
fn the_environment_is_the_requests() {
    let response = call(json!({
        "context": {"new": {"stringVars": {"map": [["V", "default"]]}}},
        "env": {"V": "from env", "OTHER": "y"},
        "calls": [
            {"call": "getStringVars"},
            {"call": "loadEnvironment"},
            {"call": "getStringVars"},
            {"call": "IsEnvVariablePresent", "on": "OCIO", "args": ["PATH"]},
        ],
    }));
    let c = calls(&response);
    assert_eq!(c[0]["result"], json!([["V", "default"]]));
    assert_eq!(c[2]["result"], json!([["V", "from env"]]));
    assert_eq!(c[3]["result"], json!(false));
}

/// A config's context is its current one, and the config is reachable as "config".
#[test]
fn a_configs_context() {
    let response = call(json!({
        "context": {"config": {"yaml": "ocio_profile_version: 2\nsearch_path: a:b\nroles:\n  \
            default: raw\ncolorspaces:\n  - !<ColorSpace>\n    name: raw\n"}},
        "calls": [
            {"call": "getSearchPaths"},
            {"call": "getCacheID", "on": "config", "args": [{"ref": "context"}]},
            {"dump": "context"},
        ],
    }));
    let c = calls(&response);
    assert_eq!(c[0]["result"], json!(["a", "b"]));
    assert!(c[1]["result"].is_string(), "{response}");
    assert_eq!(c[2]["result"]["class"], "Context");
    assert!(c[2]["result"]["repr"].is_string());
    let failed =
        call(json!({"context": {"config": {"yaml": "x: 1"}}, "calls": [{"dump": "context"}]}));
    assert_eq!(
        failed["context"]["exception"]["type"], "Exception",
        "{failed}"
    );
    assert_eq!(failed["calls"], json!([]));
}

/// Requests it can't run exactly are refused.
#[test]
fn bad_requests_are_refused() {
    for args in [
        json!({"context": "new", "config": "raw"}),
        json!({"context": "old"}),
        json!({"context": {"new": ["x"]}}),
        json!({"context": {"new": {"noSuchArgument": 1}}}),
        json!({"context": "new", "calls": [{"call": "resolveStringVar", "args": [1]}]}),
        json!({"context": "new", "env": {"OCIO_LOGGING_LEVEL": "info"}}),
    ] {
        assert!(
            Oracle::get()
                .try_call("context_calls", args.clone(), &[])
                .is_err(),
            "{args}"
        );
    }
}

/// The module's string conversions of the enums are reachable on "OCIO", their exceptions
/// reported.
#[test]
fn the_string_conversions_are_reachable() {
    let response = call(json!({
        "context": "new",
        "calls": [
            {"call": "EnvironmentModeToString", "on": "OCIO",
             "args": [{"enum": "ENV_ENVIRONMENT_LOAD_ALL"}]},
            {"call": "EnvironmentModeFromString", "on": "OCIO", "args": ["LoadAll"]},
            {"call": "BoolFromString", "on": "OCIO", "args": ["yes"]},
            {"call": "TransformDirectionFromString", "on": "OCIO", "args": ["sideways"]},
        ],
    }));
    let c = calls(&response);
    assert!(c[0]["result"].is_string(), "{response}");
    assert!(c[1]["result"]["enum"].is_string(), "{response}");
    assert!(c[2]["result"].is_boolean(), "{response}");
    assert_eq!(c[3]["exception"]["type"], "Exception", "{response}");
}
