// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's `config_calls` command (`oracle/ocio_oracle/config_api.py`), against the wheel
//! itself: the environment of a request is exactly the one it gives and the process's own comes
//! back after it; files land in the request's directory, which is the working directory; strings
//! the binding can't decode come back as their bytes; exceptions are reported per call; the dump
//! reaches every getter; and the requests it can't run exactly are refused.

use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use serde_json::{Value, json};

/// A config of one color space, with no `environment:` section (so its context loads the whole
/// environment) and the extra lines given.
fn yaml(extra: &str) -> String {
    format!(
        "ocio_profile_version: 2\nroles:\n  default: raw\ncolorspaces:\n  - !<ColorSpace>\n    \
         name: raw\n{extra}"
    )
}

fn call(args: Value) -> Value {
    Oracle::get().call("config_calls", args, &[]).result
}

/// The result of each call, panicking on an exception.
fn results(response: &Value) -> Vec<Value> {
    assert_eq!(response["config"], Value::Null, "{response}");
    response["calls"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
        .iter()
        .map(|c| {
            c.get("result")
                .unwrap_or_else(|| panic!("{c} raised"))
                .clone()
        })
        .collect()
}

/// The context of a config that loads the whole environment holds exactly the request's
/// variables: none of the oracle's own reach it.
#[test]
fn the_environment_is_exactly_the_requests() {
    let response = call(json!({
        "config": {"yaml": yaml("")},
        "env": {"OCIO_RS_PROBE_B": "2", "OCIO_RS_PROBE_A": "one"},
        "calls": [
            {"call": "getCurrentContext", "as": "context"},
            {"call": "getStringVars", "on": "context"},
            {"call": "getEnvironmentMode"},
            {"call": "IsEnvVariablePresent", "on": "OCIO", "args": ["PATH"]},
            {"call": "GetEnvVariable", "on": "OCIO", "args": ["OCIO_RS_PROBE_A"]},
        ],
    }));
    let r = results(&response);
    let mut vars: Vec<Value> = r[1].as_array().unwrap().clone();
    vars.sort_by_key(Value::to_string);
    assert_eq!(
        vars,
        [
            json!(["OCIO_RS_PROBE_A", "one"]),
            json!(["OCIO_RS_PROBE_B", "2"])
        ]
    );
    assert_eq!(r[2], json!({"enum": "ENV_ENVIRONMENT_LOAD_ALL"}));
    assert_eq!(r[3], json!(false));
    assert_eq!(r[4], json!("one"));
}

/// In one oracle process, a request's variables are gone in the next request, and the
/// process's own are back once a request ends: what a later command of a batch sees doesn't
/// depend on the earlier ones.
#[test]
fn the_process_environment_comes_back() {
    let with = json!({"config": "raw", "env": {"OCIO_RS_PROBE": "x"}, "calls": []});
    let probe = json!({"config": "raw", "calls": [
        {"call": "IsEnvVariablePresent", "on": "OCIO", "args": ["OCIO_RS_PROBE"]},
    ]});
    let calls = [
        BatchCall {
            cmd: "config_calls",
            args: with,
            blobs: vec![],
        },
        BatchCall {
            cmd: "config_calls",
            args: probe,
            blobs: vec![],
        },
        BatchCall {
            cmd: "transform_text",
            args: json!({"transforms": []}),
            blobs: vec![],
        },
    ];
    let out: Vec<_> = Oracle::get()
        .batch(&calls, false)
        .into_iter()
        .map(|r| r.unwrap_or_else(|e| panic!("{e}")))
        .collect();
    assert_eq!(results(&out[1].result), [json!(false)]);
}

/// Files land in a new directory, the request's working directory: a relative path finds
/// them, and the config's working directory is the file's directory under it.
#[test]
fn files_land_in_the_working_directory() {
    let response = call(json!({
        "config": {"file": "sub/config.ocio"},
        "files": {"sub/config.ocio": yaml("")},
        "calls": [{"call": "getWorkingDir"}],
    }));
    let dir = response["dir"].as_str().unwrap();
    let working = results(&response)[0].as_str().unwrap().to_owned();
    assert!(working.starts_with(dir), "{working} is not under {dir}");
    assert!(working.ends_with("sub"), "{working}");
}

/// Bytes go to the library as they are, and a string the binding can't decode comes back as
/// its bytes, from a getter and from a repr().
#[test]
fn undecodable_strings_come_back_as_bytes() {
    let response = call(json!({
        "config": {"yaml": yaml("")},
        "calls": [
            {"call": "getColorSpace", "args": ["raw"], "as": "cs"},
            {"call": "setFamily", "on": "cs", "args": [{"bytes": "61ff62"}]},
            {"call": "getFamily", "on": "cs"},
            {"call": "getColorSpace", "args": ["raw"]},
        ],
    }));
    let calls = response["calls"].as_array().unwrap();
    assert_eq!(
        calls[2]["exception"]["type"], "UnicodeDecodeError",
        "{response}"
    );
    assert_eq!(calls[2]["exception"]["bytes"], "61ff62", "{response}");
    let repr = &calls[3]["result"]["repr"]["exception"];
    assert!(
        repr["bytes"].as_str().unwrap().contains("61ff62"),
        "{response}"
    );
}

/// What the library raises is reported per call with its log, and nothing is stored for it;
/// a source that raises reports the exception and runs no call.
#[test]
fn exceptions_are_reported_per_call() {
    let response = call(json!({
        "config": "raw",
        "calls": [
            {"call": "getProcessor", "args": ["nope", "raw"], "as": "p"},
            {"call": "getMajorVersion"},
        ],
    }));
    let calls = response["calls"].as_array().unwrap();
    assert_eq!(calls[0]["exception"]["type"], "Exception", "{response}");
    assert!(calls[0]["log"].is_array());
    assert!(calls[1]["result"].is_number());
    let failed = call(json!({"config": {"yaml": "not: a config"}, "calls": [{"dump": "config"}]}));
    assert_eq!(
        failed["config"]["exception"]["type"], "Exception",
        "{failed}"
    );
    assert_eq!(failed["calls"], json!([]));
}

/// The dump of a built-in config reaches every getter but the two that take what no config
/// holds (a file path, a processor's arguments), and lists them.
#[test]
fn the_dump_reaches_every_getter() {
    let response = call(json!({
        "config": {"builtin": "studio-config-v4.0.0_aces-v2.0_ocio-v2.5"},
        "calls": [{"dump": "config"}],
    }));
    let dump = &results(&response)[0];
    assert_eq!(
        dump["not_dumped"],
        json!(["getColorSpaceFromFilepath", "getProcessor"])
    );
    for key in [
        "getters",
        "color_space_names",
        "color_spaces",
        "categories",
        "roles",
        "lookups",
        "environment",
        "displays",
        "shared_views",
        "virtual_display",
        "looks",
        "view_transforms",
        "default_scene_to_display",
        "named_transform_names",
        "named_transforms",
        "current_context",
        "file_rules",
        "viewing_rules",
        "serialize",
        "validate",
    ] {
        assert!(dump.get(key).is_some(), "the dump has no {key}");
    }
    assert!(!dump["color_spaces"].as_array().unwrap().is_empty());
    let space = &dump["color_spaces"][0]["dump"];
    assert_eq!(space["class"], "ColorSpace");
    assert_eq!(space["uncalled"], json!([]));
    assert!(
        dump["serialize"]
            .as_str()
            .unwrap()
            .starts_with("ocio_profile_version")
    );
}

/// Objects made by a call are stored and reached by later calls, constructors and copies
/// included, with values of every kind.
#[test]
fn objects_are_stored_and_reached() {
    let response = call(json!({
        "config": "raw",
        "calls": [
            {"new": "Context", "kwargs": {"stringVars": {"map": [["V", "x"]]},
                                          "environmentMode": {"enum": "ENV_ENVIRONMENT_LOAD_PREDEFINED"}},
             "as": "context"},
            {"copy": "context", "as": "copy"},
            {"call": "setStringVar", "on": "copy", "args": ["V", "y"]},
            {"call": "resolveStringVar", "on": "context", "args": ["$V"]},
            {"call": "resolveStringVar", "on": "copy", "args": ["${V}"]},
            {"new": "ColorSpace", "kwargs": {"name": "made"}, "as": "cs"},
            {"call": "setTransform", "on": "cs", "args": [
                {"transform": {"class": "MatrixTransform"}}, {"enum": "COLORSPACE_DIR_TO_REFERENCE"}]},
            {"call": "addColorSpace", "args": [{"ref": "cs"}]},
            {"call": "getColorSpace", "args": ["made"], "as": "found"},
            {"call": "getTransform", "on": "found", "args": [{"enum": "COLORSPACE_DIR_TO_REFERENCE"}]},
            {"call": "setDefaultLumaCoefs", "args": [[{"f64": 0}, {"f64": 0x3ff0_0000_0000_0000u64}, 0.5]]},
            {"call": "getDefaultLumaCoefs"},
        ],
    }));
    let r = results(&response);
    assert_eq!(r[3], "x");
    assert_eq!(r[4], "y");
    assert_eq!(r[9]["class"], "MatrixTransform");
    assert!(r[9]["repr"].is_string());
    assert_eq!(
        r[11],
        json!([{"f64": 0}, {"f64": 0x3ff0_0000_0000_0000u64}, {"f64": 0x3fe0_0000_0000_0000u64}])
    );
}

/// Requests it can't run exactly are refused.
#[test]
fn bad_requests_are_refused() {
    for args in [
        json!({"config": "raw", "unknown": 1}),
        json!({"config": "nope"}),
        json!({"config": "raw", "calls": [{"frobnicate": 1}]}),
        json!({"config": "raw", "calls": [{"call": "getName", "extra": 1}]}),
        json!({"config": "raw", "calls": [{"call": "_pybind11_conduit_v1_"}]}),
        json!({"config": "raw", "calls": [{"call": "noSuchMethod"}]}),
        json!({"config": "raw", "calls": [{"call": "SetCurrentConfig", "on": "OCIO",
                                           "args": [{"ref": "config"}]}]}),
        json!({"config": "raw", "calls": [{"call": "getName", "on": "missing"}]}),
        json!({"config": "raw", "calls": [{"call": "setName", "args": [1]}]}),
        json!({"config": "raw", "calls": [{"call": "getName", "args": [{"what": 1}]}]}),
        json!({"config": "raw", "env": {"OCIO_LOGGING_LEVEL": "debug"}}),
        json!({"config": "raw", "env": ["x"]}),
        json!({"config": "raw", "files": {"../escape.ocio": "x"}}),
        json!({"config": "raw", "files": {"/abs.ocio": "x"}}),
    ] {
        assert!(
            Oracle::get()
                .try_call("config_calls", args.clone(), &[])
                .is_err(),
            "{args}"
        );
    }
}
