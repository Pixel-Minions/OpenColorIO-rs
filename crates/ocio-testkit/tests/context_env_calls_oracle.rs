// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's `context_env_calls` command (`oracle/ocio_oracle/env_api.py`), against the
//! wheel itself: each request runs in a process of its own, where SetEnvVariable and
//! UnsetEnvVariable reach the wheel and change what GetEnvVariable and a context's
//! loadEnvironment read; the oracle's own process never sees the change; a process that dies
//! is reported; and bad requests are refused.

use ocio_testkit::Oracle;
use ocio_testkit::oracle_values::{bytes, result, result_bytes};
use serde_json::{Value, json};

fn call(args: Value) -> Value {
    Oracle::get().call("context_env_calls", args, &[]).result
}

fn calls(response: &Value) -> &Vec<Value> {
    assert_eq!(response["context"], Value::Null, "{response}");
    response["calls"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
}

/// A variable set through OCIO reads back through OCIO and loads into a context; unset, it is
/// gone. The request's own environment is there too.
#[test]
fn set_and_unset_reach_the_wheel() {
    let response = call(json!({
        "context": {"new": {"environmentMode": {"enum": "ENV_ENVIRONMENT_LOAD_ALL"}}},
        "env": {"GIVEN": "g"},
        "calls": [
            {"call": "SetEnvVariable", "on": "OCIO", "args": ["OCIO_RS_SET", "v"]},
            {"call": "GetEnvVariable", "on": "OCIO", "args": ["OCIO_RS_SET"]},
            {"call": "loadEnvironment"},
            {"call": "__getitem__", "args": ["OCIO_RS_SET"]},
            {"call": "__getitem__", "args": ["GIVEN"]},
            {"call": "UnsetEnvVariable", "on": "OCIO", "args": ["OCIO_RS_SET"]},
            {"call": "IsEnvVariablePresent", "on": "OCIO", "args": ["OCIO_RS_SET"]},
        ],
    }));
    let c = calls(&response);
    assert_eq!(result_bytes(&c[1]), b"v", "{response}");
    assert_eq!(result_bytes(&c[3]), b"v", "{response}");
    assert_eq!(result_bytes(&c[4]), b"g", "{response}");
    assert_eq!(*result(&c[6]), json!(false), "{response}");
}

/// The request ran in another process, which kept the change to itself: the oracle's own
/// process (context_calls, then a second context_env_calls) doesn't see the variable.
#[test]
fn the_change_stays_in_its_process() {
    let set = call(json!({
        "context": "new",
        "calls": [{"call": "SetEnvVariable", "on": "OCIO", "args": ["OCIO_RS_KEPT", "v"]}],
    }));
    let probe = json!({
        "context": "new",
        "calls": [{"call": "IsEnvVariablePresent", "on": "OCIO", "args": ["OCIO_RS_KEPT"]}],
    });
    let own = Oracle::get()
        .call("context_calls", probe.clone(), &[])
        .result;
    assert_eq!(*result(&calls(&own)[0]), json!(false), "{own}");
    let other = call(probe);
    assert_eq!(*result(&calls(&other)[0]), json!(false), "{other}");
    assert!(
        set["pid"].is_u64() && other["pid"].is_u64(),
        "{set} {other}"
    );
    assert_ne!(set["pid"], other["pid"]);
}

/// A process that dies is reported, not the oracle's end: on Windows, the wheel's
/// SetEnvVariable stops the process for a value of 32,767 UTF-16 units (_wputenv_s's parameter
/// validation). Linux's setenv takes it.
#[test]
fn a_dead_process_is_reported() {
    let long = "x".repeat(32_767);
    let response = call(json!({
        "context": "new",
        "calls": [{"call": "SetEnvVariable", "on": "OCIO", "args": ["OCIO_RS_LONG", long]}],
    }));
    if cfg!(windows) {
        let stderr = bytes(&response["crashed"]["stderr"]);
        assert!(response["crashed"]["returncode"].is_i64(), "{response}");
        // The wheel stopped it, not a Python error.
        assert!(
            !String::from_utf8_lossy(&stderr).contains("Traceback"),
            "{response}"
        );
    } else {
        assert_eq!(response["crashed"], Value::Null, "{response}");
    }
}

/// A request that context_calls refuses fails the command here too.
#[test]
fn bad_requests_are_refused() {
    let refused = Oracle::get().try_call(
        "context_env_calls",
        json!({"context": "new", "calls": [{"call": "SetCurrentConfig", "on": "OCIO"}]}),
        &[],
    );
    assert!(refused.is_err());
}
