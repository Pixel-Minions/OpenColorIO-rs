// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! S1 with strings that are not UTF-8 (review finding F1).
//!
//! OCIO's strings are C byte strings: the wheel accepts bytes for every string parameter, and
//! yaml-cpp writes them as they are in plain scalars and decodes them leniently in quoted and
//! literal ones (emitterutils.cpp:90-153). An overlong `C0 80` decodes to U+0000, which a
//! literal block writes raw, and OCIO writes `ostream << out.c_str()` (OCIOYaml.cpp:5447), so
//! the text, and the config cache ID that hashes it (Config.cpp:5264-5271), stop there.
//!
//! `serialize()` fails in Python on text that is not UTF-8, so the oracle writes
//! `serialize(fileName)` (a `std::ofstream` in text mode: CRLF on Windows) and reports
//! `getCacheID()` (`ocio_oracle/text.py` `serialize_built_config_to_file`). The test reads the
//! same config with a placeholder in place of the bytes, writes it again with the bytes put
//! back (`common::ocio_writer`), and compares the file's bytes and the cache ID.

mod common;

use common::ocio_writer::OcioWriter;
use common::yaml_tree;
use ocio_testkit::{Oracle, assert_bytes_eq, assert_text_eq, fixtures};
use serde_json::{Value, json};

/// Port of `CacheIDHash` (HashUtils.cpp:20-30 @ v2.5.2): XXH3-128 of the bytes, printed as its
/// low then its high 64 bits in zero-padded lowercase hex.
fn cache_id_hash(bytes: &[u8]) -> String {
    let hash = xxhash_rust::xxh3::xxh3_128(bytes);
    format!("{:016x}{:016x}", hash as u64, (hash >> 64) as u64)
}

/// `Config::getCacheID()` for a config without file references (Config.cpp:5250-5311): the
/// hash of `serialize()`, a colon, and the hash of the empty file-reference list.
fn config_cache_id(serialized: &[u8]) -> String {
    format!("{}:{}", cache_id_hash(serialized), cache_id_hash(b""))
}

/// The bytes a `std::ofstream` opened in text mode writes for `text`: on Windows every `\n`
/// becomes `\r\n` (the MSVC runtime's text mode); elsewhere nothing changes.
fn text_mode(text: &[u8]) -> Vec<u8> {
    if cfg!(windows) {
        let mut out = Vec::with_capacity(text.len() + text.len() / 16);
        for &b in text {
            if b == b'\n' {
                out.push(b'\r');
            }
            out.push(b);
        }
        out
    } else {
        text.to_vec()
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The config of one case: the string `v` in one place of a small config.
fn slot_spec(slot: &str, v: Value) -> Value {
    let cs0 = json!({"name": "cs0"});
    match slot {
        "family" => json!({"colorspaces": [{"name": "cs0", "family": v}]}),
        "cs_description" => json!({"colorspaces": [{"name": "cs0", "description": v}]}),
        "config_description" => json!({"description": v, "colorspaces": [cs0]}),
        "interchange" => {
            json!({"colorspaces": [{"name": "cs0", "interchange": [["amf_transform_ids", v]]}]})
        }
        "view_description" => json!({
            "colorspaces": [cs0],
            "displays": [["d", [{"name": "v", "colorspace": "cs0", "description": v}]]],
        }),
        "env_key" => json!({"environment": [[v, "x"]], "colorspaces": [cs0]}),
        "env_value" => json!({"environment": [["k", v]], "colorspaces": [cs0]}),
        "rule_custom_key" => json!({
            "colorspaces": [cs0],
            "file_rules": [{"name": "r", "colorspace": "cs0", "pattern": "*", "extension": "*",
                            "custom": [[v, "x"]]}],
        }),
        "display" => json!({
            "colorspaces": [cs0],
            "displays": [[v, [{"name": "v", "colorspace": "cs0"}]]],
        }),
        "name" => json!({"name": v, "colorspaces": [cs0]}),
        other => panic!("unknown slot {other}"),
    }
}

/// The byte strings, by the place they go.
const CASES: &[(&str, &[u8])] = &[
    // Plain scalars pass bytes through; quoted ones decode them (U+FFFD for bad bytes).
    ("family", b"caf\xe9"),
    ("family", b"- caf\xe9"),
    ("family", b"a\x80b"),
    ("family", b"- \xc0\x80x"),
    ("family", b"a\xed\xa0\x80b"),
    ("name", b"\xff\xfe"),
    // Literal blocks decode too; an overlong NUL ends the text there.
    ("cs_description", b"a\xff\xfeb\nc\xc0\x80d"),
    ("cs_description", b"x\n\xc0\x80A"),
    ("cs_description", b"a\n\xe6\x97"),
    ("cs_description", b"a\n\xed\xa0\x80b"),
    ("cs_description", b"a\n\xf8\x88\x80\x80b"),
    ("cs_description", b"a\n\xe9t\xe9"),
    ("config_description", b"caf\xe9\nx\xc0\x80y"),
    ("interchange", b"one\n\xe6\x97"),
    ("cs_description", b"k: \xff"),
    // Double quotes in a flow map; keys.
    ("view_description", b"a\n\xff"),
    ("view_description", b"\xc0\x80"),
    ("env_key", b"K\xff"),
    ("env_value", b"\xc0\x80"),
    ("rule_custom_key", b"k\xfe"),
    ("rule_custom_key", &[b'k'; 1030]),
    ("display", b"d\xe9"),
];

/// Writes the config of `slot` holding `bytes` and compares with the wheel's file and
/// cache ID. Returns the wheel's cache ID.
fn check(slot: &str, bytes: &[u8]) -> String {
    let label = format!("{slot} {bytes:x?}");
    let placeholder = "phxq0000";
    let oracle = Oracle::get();

    let text = oracle.call(
        "serialize_built_config",
        json!({"spec": slot_spec(slot, json!(placeholder))}),
        &[],
    );
    assert!(
        text.result.get("exception").is_none(),
        "{label}: {}",
        text.result
    );
    let file = oracle.call(
        "serialize_built_config_to_file",
        json!({"spec": slot_spec(slot, json!({"hex": hex(bytes)}))}),
        &[],
    );
    assert!(
        file.result.get("exception").is_none(),
        "{label}: {}",
        file.result
    );

    let tree = yaml_tree::parse(text.blob_text(0));
    let mut writer = OcioWriter::new();
    writer.substitutions = vec![(placeholder.to_string(), bytes.to_vec())];
    writer.save_config(&tree);
    assert_eq!(writer.substituted.len(), 1, "{label}: placeholder put back");

    let written = writer.out.c_str();
    assert_bytes_eq(&label, &file.blobs[0], &text_mode(written));
    let cache_id = file.result["cache_id"].as_str().expect("a cache ID");
    assert_eq!(config_cache_id(written), cache_id, "{label}: cache ID");
    cache_id.to_string()
}

#[test]
fn byte_strings_serialize_like_the_wheel() {
    for &(slot, bytes) in CASES {
        check(slot, bytes);
    }
}

/// The overlong NUL ends the text the cache ID hashes, so two configs that differ only after
/// it share a cache ID, in the wheel and in the port.
#[test]
fn text_after_an_overlong_nul_is_lost() {
    let a = check("cs_description", b"x\n\xc0\x80A");
    let b = check("cs_description", b"x\n\xc0\x80B");
    assert_eq!(a, b);
}

/// The cache ID port against the wheel's own: the built-in configs' serialize() text and
/// cache ID fixtures.
#[test]
fn cache_ids_of_the_builtin_configs() {
    let paths: Vec<String> = fixtures::list("builtin_configs/")
        .into_iter()
        .filter(|p| p.ends_with("/cache_id.txt"))
        .collect();
    assert_eq!(paths.len(), 8, "the eight built-in configs: {paths:?}");
    for path in paths {
        let text = fixtures::read(&path.replace("/cache_id.txt", "/serialize.ocio"));
        assert_text_eq(&path, &fixtures::read_text(&path), &config_cache_id(&text));
    }
}
