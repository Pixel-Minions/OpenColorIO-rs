// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Reading the values the Phase 3 oracle commands write (`oracle/ocio_oracle/config_api.py`):
//! strings as `{"bytes": hex}`, floats as `{"f64": bits}`, a call's outcome as `{"result"}`,
//! `{"exception": {"type", "message"}}` or `{"undecodable": hex}`, and the log as a list of
//! `{"bytes": hex}` messages.

use serde_json::{Value, json};

/// `bytes` as lowercase hex, as the commands take and write bytes.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// `{"bytes": hex}`: a string argument given as its bytes.
pub fn bytes_arg(bytes: &[u8]) -> Value {
    json!({ "bytes": hex(bytes) })
}

/// The bytes of a `{"bytes": hex}` value. Panics on anything else.
#[track_caller]
pub fn bytes(value: &Value) -> Vec<u8> {
    let text = value["bytes"]
        .as_str()
        .unwrap_or_else(|| panic!("not {{\"bytes\": hex}}: {value}"));
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex"))
        .collect()
}

/// The text of a `{"bytes": hex}` value, what isn't UTF-8 replaced by U+FFFD: for messages in
/// assertions.
#[track_caller]
pub fn text(value: &Value) -> String {
    String::from_utf8_lossy(&bytes(value)).into_owned()
}

/// The messages of a log, `[{"bytes": hex}, ...]`, each with its line feed.
#[track_caller]
pub fn log(value: &Value) -> Vec<Vec<u8>> {
    value
        .as_array()
        .unwrap_or_else(|| panic!("not a log: {value}"))
        .iter()
        .map(bytes)
        .collect()
}

/// A call's result (`{"result": v}`). Panics when the call raised or couldn't be decoded.
#[track_caller]
pub fn result(call: &Value) -> &Value {
    call.get("result")
        .unwrap_or_else(|| panic!("the call didn't return: {call}"))
}

/// The bytes of a call's string result.
#[track_caller]
pub fn result_bytes(call: &Value) -> Vec<u8> {
    bytes(result(call))
}

/// The message of a call's exception, as bytes, with its type.
#[track_caller]
pub fn exception(call: &Value) -> (String, Vec<u8>) {
    let e = &call["exception"];
    let kind = e["type"]
        .as_str()
        .unwrap_or_else(|| panic!("the call didn't raise: {call}"))
        .to_string();
    (kind, bytes(&e["message"]))
}
