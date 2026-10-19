// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's `config_io_proxy` command (`oracle/ocio_oracle/config_io_proxy_api.py`),
//! against the wheel and the files it is given: a config read from an archive the wheel made
//! has a proxy, whose `getLutData` gives each archived LUT file's bytes, in the order of the
//! calls, for paths as text or bytes, and data or an exception for a file the archive doesn't
//! hold; `getConfigData` gives the config as the wheel serializes it, and
//! `getFastLutFileHash` a different text per file; a config
//! read from a file has none; a config the wheel can't read reports its stage; the requests it
//! can't read are refused.

use ocio_testkit::Oracle;
use ocio_testkit::oracle_values::bytes;
use serde_json::{Value, json};

const CONFIG: &str = "ocio_profile_version: 2\nsearch_path: luts\nroles: {default: raw}\n\
colorspaces:\n  - !<ColorSpace> {name: raw}\n  - !<ColorSpace> {name: b, \
from_scene_reference: !<FileTransform> {src: a.spi1d}}\n";

const LUT_A: &str = "Version 1\nFrom 0.0 1.0\nLength 2\nComponents 1\n{\n0.0\n1.0\n}\n";
const LUT_B: &str = "Version 1\nFrom 0.0 1.0\nLength 2\nComponents 1\n{\n0.25\n0.5\n}\n";

/// The archive the wheel makes of `CONFIG` with `luts/a.spi1d` and `luts/b.spi1d`.
fn archive() -> Vec<u8> {
    let response = Oracle::get().call(
        "with_files",
        json!({"files": {"config.ocio": CONFIG, "luts/a.spi1d": LUT_A, "luts/b.spi1d": LUT_B},
               "command": "ocioz",
               "args": {"archive": {"config": {"file": "$FILES/config.ocio"}}}}),
        &[],
    );
    let index = response.result["archive"]
        .as_u64()
        .unwrap_or_else(|| panic!("{}", response.result)) as usize;
    response.blobs[index].clone()
}

/// The wheel's `config_io_proxy` of the config read from `archive`, with `calls`.
fn proxy_calls(archive: &[u8], calls: Value) -> (Value, Vec<Vec<u8>>) {
    let response = Oracle::get().call(
        "with_files",
        json!({"files": {"a.ocioz": {"blob": 0}}, "file_blobs": 1, "command": "config_io_proxy",
               "args": {"config": {"file": "$FILES/a.ocioz"}, "calls": calls}}),
        &[archive],
    );
    (response.result, response.blobs)
}

#[test]
fn an_archived_config_has_a_proxy_that_gives_its_files() {
    let archive = archive();
    let (result, blobs) = proxy_calls(
        &archive,
        json!([
            {"getLutData": "luts/b.spi1d"},
            {"getLutData": {"bytes": b"luts/a.spi1d".iter().map(|b| format!("{b:02x}")).collect::<String>()}},
            {"getLutData": "luts/c.spi1d"},
            {"getConfigData": null},
            {"getFastLutFileHash": "luts/a.spi1d"},
            {"getFastLutFileHash": "luts/b.spi1d"},
        ]),
    );
    assert_eq!(result["proxy"], json!(true), "{result}");
    let calls = result["calls"]
        .as_array()
        .unwrap_or_else(|| panic!("{result}"));
    assert_eq!(calls.len(), 6, "{result}");
    let data = |i: usize| -> &[u8] {
        let index = calls[i]["data"]
            .as_u64()
            .unwrap_or_else(|| panic!("call {i}: {result}"));
        &blobs[index as usize]
    };
    assert_eq!(data(0), LUT_B.as_bytes());
    assert_eq!(data(1), LUT_A.as_bytes());
    // A file the archive doesn't hold: whatever the wheel gives, data or an exception.
    assert!(
        calls[2]["data"].is_u64() || calls[2]["exception"]["message"].is_string(),
        "{result}"
    );
    // The archive holds the config as the wheel serializes it.
    let serialized =
        Oracle::get().call("config_serialize", json!({"config": {"yaml": CONFIG}}), &[]);
    assert_eq!(bytes(&calls[3]["text"]), serialized.blobs[0], "{result}");
    let (hash_a, hash_b) = (bytes(&calls[4]["text"]), bytes(&calls[5]["text"]));
    assert!(!hash_a.is_empty() && hash_a != hash_b, "{result}");
    assert_eq!(
        blobs.len(),
        2 + usize::from(calls[2]["data"].is_u64()),
        "{result}"
    );
}

#[test]
fn a_config_read_from_a_file_has_no_proxy() {
    let response = Oracle::get().call(
        "with_files",
        json!({"files": {"config.ocio": CONFIG, "luts/a.spi1d": LUT_A},
               "command": "config_io_proxy",
               "args": {"config": {"file": "$FILES/config.ocio"},
                        "calls": [{"getLutData": "luts/a.spi1d"}]}}),
        &[],
    );
    assert_eq!(
        response.result["proxy"],
        json!(false),
        "{}",
        response.result
    );
    assert!(
        response.result.get("calls").is_none(),
        "{}",
        response.result
    );
    assert!(response.blobs.is_empty());
}

#[test]
fn a_config_the_wheel_cant_read_reports_its_stage() {
    let response = Oracle::get().call(
        "with_files",
        json!({"files": {"a.ocioz": {"bytes": "00"}}, "command": "config_io_proxy",
               "args": {"config": {"file": "$FILES/a.ocioz"}, "calls": []}}),
        &[],
    );
    let result = &response.result;
    assert_eq!(result["stage"], json!("config"), "{result}");
    assert!(result["exception"]["message"].is_string(), "{result}");
    assert!(result.get("proxy").is_none(), "{result}");
}

#[test]
fn unreadable_requests_are_refused() {
    for args in [
        json!({"config": null, "calls": [], "extra": 1}),
        json!({"config": null}),
        json!({"config": null, "calls": {"getConfigData": null}}),
        json!({"config": null, "calls": [{"getLutDatum": "a"}]}),
        json!({"config": null, "calls": [{"getLutData": null}]}),
        json!({"config": null, "calls": [{"getLutData": 1}]}),
        json!({"config": null, "calls": [{"getConfigData": "a"}]}),
        json!({"config": null, "calls": [{"getLutData": "a", "getConfigData": null}]}),
        json!({"config": null, "calls": ["getConfigData"]}),
    ] {
        assert!(
            Oracle::get()
                .try_call("config_io_proxy", args.clone(), &[])
                .is_err(),
            "{args}"
        );
    }
    // The same request with readable calls is answered: a raw config has no proxy.
    let response = Oracle::get().call(
        "config_io_proxy",
        json!({"config": null, "calls": [{"getLutData": "a"}, {"getConfigData": null}]}),
        &[],
    );
    assert_eq!(
        response.result["proxy"],
        json!(false),
        "{}",
        response.result
    );
}
