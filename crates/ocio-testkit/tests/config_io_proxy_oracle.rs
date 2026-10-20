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

/// The archive the wheel makes of `CONFIG` with `luts/a.spi1d` and `luts/b.spi1d`, and the
/// `ocioz` command's report of its entries.
fn archive() -> (Vec<u8>, Value) {
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
    (
        response.blobs[index].clone(),
        response.result["entries"].clone(),
    )
}

/// A zip archive of `files`, each stored (PKWARE APPNOTE 4.3.7, 4.3.12, 4.3.16).
fn stored_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    let crc32 = |data: &[u8]| {
        let mut crc = u32::MAX;
        for &b in data {
            crc ^= u32::from(b);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xedb8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    };
    let (mut out, mut central) = (Vec::new(), Vec::new());
    for (name, data) in files {
        let offset = out.len() as u32;
        let (crc, size) = (crc32(data), data.len() as u32);
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        for v in [20u16, 0, 0, 0, 0x5a21] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for v in [crc, size, size] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for v in [name.len() as u16, 0] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        for v in [20u16, 20, 0, 0, 0, 0x5a21] {
            central.extend_from_slice(&v.to_le_bytes());
        }
        for v in [crc, size, size] {
            central.extend_from_slice(&v.to_le_bytes());
        }
        for v in [name.len() as u16, 0, 0, 0, 0] {
            central.extend_from_slice(&v.to_le_bytes());
        }
        for v in [0u32, offset] {
            central.extend_from_slice(&v.to_le_bytes());
        }
        central.extend_from_slice(name.as_bytes());
    }
    let cd_offset = out.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    let n = files.len() as u16;
    for v in [0u16, 0, n, n] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for v in [central.len() as u32, cd_offset] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&[0, 0]);
    out
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
    let (archive, entries) = archive();
    let (result, blobs) = proxy_calls(
        &archive,
        json!([
            {"getLutData": "luts/b.spi1d"},
            {"getLutData": {"bytes": b"luts/a.spi1d".iter().map(|b| format!("{b:02x}")).collect::<String>()}},
            {"getLutData": "luts/c.spi1d"},
            {"getConfigData": null},
            {"getFastLutFileHash": "luts/a.spi1d"},
            {"getFastLutFileHash": "luts/b.spi1d"},
            {"getFastLutFileHash": "luts/c.spi1d"},
        ]),
    );
    assert_eq!(result["proxy"], json!(true), "{result}");
    let calls = result["calls"]
        .as_array()
        .unwrap_or_else(|| panic!("{result}"));
    assert_eq!(calls.len(), 7, "{result}");
    let data = |i: usize| -> &[u8] {
        let index = calls[i]["data"]
            .as_u64()
            .unwrap_or_else(|| panic!("call {i}: {result}"));
        &blobs[index as usize]
    };
    assert_eq!(data(0), LUT_B.as_bytes());
    assert_eq!(data(1), LUT_A.as_bytes());
    // A file the archive doesn't hold: no bytes (OCIOZArchive.cpp:417-427, 586-600 @ v2.5.2).
    assert!(data(2).is_empty(), "{result}");
    // The archive holds the config as the wheel serializes it.
    let serialized =
        Oracle::get().call("config_serialize", json!({"config": {"yaml": CONFIG}}), &[]);
    assert_eq!(bytes(&calls[3]["text"]), serialized.blobs[0], "{result}");
    // A file's hash is its entry's name and CRC in decimal (OCIOZArchive.cpp:565-571 @
    // v2.5.2), as the `ocioz` command reports the entry; a missing file's is empty.
    let entries = entries.as_array().unwrap_or_else(|| panic!("{entries:?}"));
    for (call, name) in [(4, &b"luts/a.spi1d"[..]), (5, &b"luts/b.spi1d"[..])] {
        let entry = entries
            .iter()
            .find(|e| bytes(&e["name"]) == name)
            .unwrap_or_else(|| panic!("{entries:?}"));
        let mut hash = name.to_vec();
        hash.extend_from_slice(entry["crc"].as_u64().unwrap().to_string().as_bytes());
        assert_eq!(bytes(&calls[call]["text"]), hash, "{result}");
    }
    assert_eq!(bytes(&calls[6]["text"]), b"", "{result}");
    assert_eq!(blobs.len(), 3, "{result}");
}

/// A string the binding can't return, the text of a config that isn't UTF-8 (a description
/// of `caf\xe9`), is reported by its bytes; the calls after it go on.
#[test]
fn a_config_that_isnt_utf8_is_reported_by_its_bytes() {
    let mut config = CONFIG.as_bytes().to_vec();
    let at = config.windows(5).position(|w| w == b"raw}\n").unwrap();
    config.splice(
        at + 3..at + 3,
        b", description: \"caf\xe9\"".iter().copied(),
    );
    let archive = stored_zip(&[("config.ocio", &config), ("luts/a.spi1d", LUT_A.as_bytes())]);
    let (result, blobs) = proxy_calls(
        &archive,
        json!([{"getConfigData": null}, {"getLutData": "luts/a.spi1d"}]),
    );
    let calls = &result["calls"];
    let hex: String = config.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(
        calls[0]["exception"],
        json!({"type": "UnicodeDecodeError", "undecodable": hex}),
        "{result}"
    );
    let index = calls[1]["data"]
        .as_u64()
        .unwrap_or_else(|| panic!("{result}"));
    assert_eq!(blobs[index as usize], LUT_A.as_bytes());
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
