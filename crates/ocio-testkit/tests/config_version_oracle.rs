// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's `config_major_version` command (`oracle/ocio_oracle/config_version.py`),
//! against the wheel itself: each version is set on a config of its own (the earlier calls
//! leave no trace), the versions before and after the call come back for each, with what a
//! refused version raised; and the arguments it doesn't take are refused.

use ocio_testkit::Oracle;
use serde_json::{Value, json};

/// The command's entries for `versions` on the raw config.
fn entries(versions: Value) -> Vec<Value> {
    let response = Oracle::get().call("config_major_version", json!({"versions": versions}), &[]);
    response.result["versions"]
        .as_array()
        .unwrap_or_else(|| panic!("{}", response.result))
        .clone()
}

/// Each version gets a new config: the same version set twice in a row, or after another,
/// starts from the raw config's versions each time.
#[test]
fn each_version_starts_from_a_new_config() {
    let all = entries(json!([1, 2, 1, 1]));
    assert_eq!(all.len(), 4);
    for entry in &all {
        assert_eq!(entry["before"], all[0]["before"], "{entry}");
    }
    assert_eq!(all[0]["after"], all[2]["after"]);
    assert_eq!(all[2]["after"], all[3]["after"]);
}

/// A refused version comes back with the exception and the versions after the call.
#[test]
fn a_refused_version_reports_the_exception() {
    let all = entries(json!([0, 4_294_967_295u32]));
    for entry in &all {
        assert_eq!(entry["exception"]["type"], "Exception", "{entry}");
        assert!(entry["exception"]["message"].is_string(), "{entry}");
        assert!(entry["after"].is_array(), "{entry}");
    }
}

/// An unknown key, versions that aren't a list, and a version that isn't an unsigned int
/// are refused.
#[test]
fn bad_arguments_are_refused() {
    for args in [
        json!({"version": [1]}),
        json!({"versions": 1}),
        json!({"versions": [true]}),
        json!({"versions": [-1]}),
        json!({"versions": [4_294_967_296u64]}),
        json!({"versions": [1.0]}),
    ] {
        assert!(
            Oracle::get()
                .try_call("config_major_version", args.clone(), &[])
                .is_err(),
            "{args}"
        );
    }
}
