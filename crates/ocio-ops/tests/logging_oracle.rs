// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The lines the port's logging function receives, against the wheel's.
//!
//! The wheel logs a warning for each unknown key of a transform in a config: "Unknown key in
//! LogTransform: '<key>'." (`LogUnknownKeyWarning`, src/OpenColorIO/OCIOYaml.cpp:248-257,
//! called at 2918-2921 @ v2.5.2). A YAML key can hold any character, so these keys make
//! messages with line breaks, carriage returns, empty lines, spaces around the breaks, a
//! non-ASCII character and a NUL. The oracle loads such a config (`config_serialize`) and
//! returns what its logging function received (`captured_log`); the port logs the same
//! messages, and its logging function must receive the same lines, byte for byte.
//!
//! The wheel runs at the default level (the oracle never passes `OCIO_LOGGING_LEVEL`), and
//! so does this test.

use std::sync::{Arc, Mutex};

use ocio_ops::logging::{log_warning, reset_to_default_logging_function, set_logging_function};
use ocio_testkit::Oracle;
use serde_json::json;

/// The unknown keys, in the order the transform lists them. Their order and bytes are the
/// test's input.
const KEYS: &[&str] = &[
    "a\nb",
    "c\r\nd",
    "e\n\nf",
    "g \n h",
    "\n",
    "tab\there",
    "\u{e9}t\u{e9}",
    "x\0y",
];

/// `key` as a YAML double-quoted scalar.
fn yaml_quoted(key: &str) -> String {
    let mut out = String::from("\"");
    for c in key.chars() {
        match c {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A config whose one transform has every key of [`KEYS`].
fn config_yaml() -> String {
    let keys: Vec<String> = KEYS
        .iter()
        .map(|key| format!("{}: 1", yaml_quoted(key)))
        .collect();
    format!(
        "ocio_profile_version: 2\n\
         \n\
         roles:\n  default: raw\n\
         \n\
         colorspaces:\n\
         \x20 - !<ColorSpace>\n\
         \x20   name: raw\n\
         \x20   from_scene_reference: !<LogTransform> {{base: 2, {}}}\n",
        keys.join(", ")
    )
}

#[test]
fn unknown_key_warnings_match_the_wheel() {
    let response = Oracle::get().call(
        "config_serialize",
        json!({ "config": { "yaml": config_yaml() } }),
        &[],
    );
    assert!(
        response.result.get("exception").is_none(),
        "the wheel refused the config: {}",
        response.result
    );
    let wheel: Vec<Vec<u8>> = response.result["log"]
        .as_array()
        .expect("a log")
        .iter()
        .map(|line| line.as_str().expect("a line").as_bytes().to_vec())
        .collect();

    let lines = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    let sink = Arc::clone(&lines);
    set_logging_function(Some(Arc::new(move |line: &[u8]| {
        sink.lock().unwrap().push(line.to_vec());
    })))
    .unwrap();
    for key in KEYS {
        log_warning(format!("Unknown key in LogTransform: '{key}'."));
    }
    reset_to_default_logging_function();

    let port = lines.lock().unwrap().clone();
    assert_eq!(
        port,
        wheel,
        "port:\n{:#?}\nwheel:\n{:#?}",
        port.iter()
            .map(|l| String::from_utf8_lossy(l))
            .collect::<Vec<_>>(),
        wheel
            .iter()
            .map(|l| String::from_utf8_lossy(l))
            .collect::<Vec<_>>()
    );
}
