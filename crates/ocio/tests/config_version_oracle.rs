// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `Config::set_major_version` against the wheel (the oracle's `config_major_version`): on the
//! raw config, the major and minor versions before and after each version, supported or
//! refused, and a refused version's message.

use std::sync::Arc;

use ocio::Config;
use ocio_testkit::Oracle;
use serde_json::{Value, json};

/// The major and minor versions, as the command reports them.
fn versions(config: &Config) -> Value {
    json!([config.major_version(), config.minor_version()])
}

#[test]
fn set_major_version_matches_the_wheel() {
    let tried: [u32; 7] = [1, 2, 0, 3, 20_000, u32::MAX, 1];
    let response = Oracle::get().call("config_major_version", json!({"versions": tried}), &[]);
    let wheel = response.result["versions"]
        .as_array()
        .unwrap_or_else(|| panic!("{}", response.result));
    assert_eq!(wheel.len(), tried.len());

    let mut failures = Vec::new();
    for (&version, wheel) in tried.iter().zip(wheel) {
        let mut raw = Config::create_raw();
        let config = Arc::get_mut(&mut raw).expect("a config of its own");
        let before = versions(config);
        let result = config.set_major_version(version);
        let mut port = json!({"before": before, "after": versions(config)});
        if let Err(e) = result {
            port["exception"] = json!({"type": "Exception", "message": e.message()});
        }
        if &port != wheel {
            failures.push(format!("{version}: wheel {wheel}, port {port}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
