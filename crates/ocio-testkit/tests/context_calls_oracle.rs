// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's `context_calls` command (`oracle/ocio_oracle/context_api.py`), against the
//! wheel itself: each context source and its arguments, file resolution against the request's
//! files with a used context, the request's environment, the module's string conversions, and
//! the requests it refuses.

use ocio_testkit::Oracle;
use ocio_testkit::oracle_values::{bytes, exception, result, result_bytes};
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

fn pairs(list: &Value) -> Vec<(Vec<u8>, Vec<u8>)> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|p| (bytes(&p[0]), bytes(&p[1])))
        .collect()
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
    assert!(result_bytes(&c[1]).ends_with(b"a.spi1d"), "{response}");
    assert_eq!(pairs(result(&c[2])), [(b"SHOT".to_vec(), b"s01".to_vec())]);
    assert_eq!(exception(&c[3]).0, "ExceptionMissingFile", "{response}");
}

/// Each of the constructor's arguments reaches the context.
#[test]
fn the_constructor_takes_every_argument() {
    let response = call(json!({
        "context": {"new": {"workingDir": "/work", "searchPaths": ["a", "b"],
                            "stringVars": {"map": [["V", "x"]]},
                            "environmentMode": {"enum": "ENV_ENVIRONMENT_LOAD_ALL"}}},
        "calls": [
            {"call": "getWorkingDir"},
            {"call": "getSearchPath"},
            {"call": "getStringVars"},
            {"call": "getEnvironmentMode"},
        ],
    }));
    let c = calls(&response);
    assert_eq!(result_bytes(&c[0]), b"/work");
    assert_eq!(result_bytes(&c[1]), b"a:b");
    assert_eq!(pairs(result(&c[2])), [(b"V".to_vec(), b"x".to_vec())]);
    assert_eq!(*result(&c[3]), json!({"enum": "ENV_ENVIRONMENT_LOAD_ALL"}));
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
    assert_eq!(pairs(result(&c[0])), [(b"V".to_vec(), b"default".to_vec())]);
    assert_eq!(
        pairs(result(&c[2])),
        [(b"V".to_vec(), b"from env".to_vec())]
    );
    assert_eq!(*result(&c[3]), json!(false));
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
    let paths: Vec<Vec<u8>> = result(&c[0])
        .as_array()
        .unwrap()
        .iter()
        .map(bytes)
        .collect();
    assert_eq!(paths, [b"a".to_vec(), b"b".to_vec()]);
    assert!(!result_bytes(&c[1]).is_empty(), "{response}");
    assert_eq!(result(&c[2])["class"], "Context");
    assert!(bytes(&result(&c[2])["repr"]).starts_with(b"<Context"));
    let failed =
        call(json!({"context": {"config": {"yaml": "x: 1"}}, "calls": [{"dump": "context"}]}));
    assert_eq!(
        failed["context"]["exception"]["type"], "Exception",
        "{failed}"
    );
    assert_eq!(failed["calls"], json!([]));
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
    assert_eq!(result_bytes(&c[0]), b"loadall");
    assert_eq!(*result(&c[1]), json!({"enum": "ENV_ENVIRONMENT_LOAD_ALL"}));
    assert_eq!(*result(&c[2]), json!(true));
    assert_eq!(exception(&c[3]).0, "Exception", "{response}");
}

/// Requests it can't run exactly are refused.
#[test]
fn bad_requests_are_refused() {
    for args in [
        json!({"context": "new", "config": "raw"}),
        json!({"context": "old"}),
        json!({"context": {"new": ["x"]}}),
        json!({"context": {"new": {"noSuchArgument": 1}}}),
        json!({"context": {"new": {"stringVars": {"map": "x"}}}}),
        json!({"context": {"new": {"workingDir": {"f64": 0}}}}),
        json!({"context": {"config": "nope"}}),
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
