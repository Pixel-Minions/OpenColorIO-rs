// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! OCIO's environment functions against the wheel's, through the oracle's `context_env_calls`
//! (each request in a process of its own): SetEnvVariable and UnsetEnvVariable with each
//! platform's rules (a name holding `=`, an empty value, a name starting with `=`, case, bytes
//! that aren't UTF-8, Windows' folding of names, the length limit), then what GetEnvVariable,
//! IsEnvVariablePresent and a context's loadEnvironment (in LOAD_ALL mode) read. The port's
//! environment starts as the oracle's process does: empty, then the request's variables set
//! one by one.

use std::sync::Arc;

use ocio::Context;
use ocio_ops::open_color_types::EnvironmentMode;
use ocio_ops::platform::{
    MapEnv, get_env_variable, is_env_variable_present, set_env_provider, set_env_variable, setenv,
    unset_env_variable,
};
use ocio_testkit::Oracle;
use ocio_testkit::oracle_values::{bytes, hex, result};
use serde_json::{Value, json};

/// One operation, on the port and as a call.
enum Op {
    Set(Vec<u8>, Vec<u8>),
    Unset(Vec<u8>),
    Get(Vec<u8>),
    Present(Vec<u8>),
    /// A new context in LOAD_ALL mode loads the environment; its variables.
    Load,
}

fn arg(s: &[u8]) -> Value {
    json!({"bytes": hex(s)})
}

fn calls(ops: &[Op]) -> Vec<Value> {
    let mut out = Vec::new();
    for op in ops {
        match op {
            Op::Set(n, v) => {
                out.push(json!({"call": "SetEnvVariable", "on": "OCIO", "args": [arg(n), arg(v)]}))
            }
            Op::Unset(n) => {
                out.push(json!({"call": "UnsetEnvVariable", "on": "OCIO", "args": [arg(n)]}))
            }
            Op::Get(n) => {
                out.push(json!({"call": "GetEnvVariable", "on": "OCIO", "args": [arg(n)]}))
            }
            Op::Present(n) => {
                out.push(json!({"call": "IsEnvVariablePresent", "on": "OCIO", "args": [arg(n)]}))
            }
            Op::Load => {
                out.push(json!({"new": "Context", "as": "loaded"}));
                out.push(json!({"call": "setEnvironmentMode", "on": "loaded",
                    "args": [{"enum": "ENV_ENVIRONMENT_LOAD_ALL"}]}));
                out.push(json!({"call": "loadEnvironment", "on": "loaded"}));
                out.push(json!({"call": "getStringVars", "on": "loaded"}));
            }
        }
    }
    out
}

/// A string result, or the bytes of one the binding couldn't decode.
fn text_out(call: &Value) -> Vec<u8> {
    match call.get("undecodable") {
        Some(Value::String(h)) => (0..h.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&h[i..i + 2], 16).expect("hex"))
            .collect(),
        _ => bytes(result(call)),
    }
}

/// The context's variables against the wheel's list: a pair `(name, value)`, or, where the
/// binding couldn't decode the pair, `{"undecodable": hex}` with the first string it couldn't
/// decode (the name, else the value).
fn assert_pairs(port: &[(Vec<u8>, Vec<u8>)], call: &Value, what: &str) {
    let wheel = result(call).as_array().unwrap_or_else(|| panic!("{call}"));
    assert_eq!(port.len(), wheel.len(), "{what}");
    for ((name, value), w) in port.iter().zip(wheel) {
        match w.get("undecodable") {
            Some(Value::String(h)) => {
                let undecodable = if std::str::from_utf8(name).is_ok() {
                    value
                } else {
                    name
                };
                assert_eq!(hex(undecodable), *h, "{what}");
            }
            _ => assert_eq!(
                (name.clone(), value.clone()),
                (bytes(&w[0]), bytes(&w[1])),
                "{what}"
            ),
        }
    }
}

/// Runs the operations on both, in an environment of `env`, and compares each read.
fn check(name: &str, env: &[(&str, &str)], ops: &[Op]) {
    let env_json: serde_json::Map<String, Value> =
        env.iter().map(|(k, v)| (k.to_string(), json!(v))).collect();
    let response = Oracle::get()
        .call(
            "context_env_calls",
            json!({"context": "new", "env": env_json, "calls": calls(ops)}),
            &[],
        )
        .result;

    set_env_provider(Some(Arc::new(MapEnv::default())));
    for (n, v) in env {
        setenv(n, v).expect("a request's variable");
    }
    let mut port_failed = None;
    let mut w = response["calls"].as_array().map(|c| c.iter());
    for (i, op) in ops.iter().enumerate() {
        let what = format!("{name}, operation {i}: {response}");
        let mut next = || {
            w.as_mut()
                .and_then(Iterator::next)
                .unwrap_or_else(|| panic!("{what}"))
                .clone()
        };
        match op {
            Op::Set(n, v) => {
                if let Err(e) = set_env_variable(n, Some(v)) {
                    port_failed = Some(e);
                    break;
                }
                next();
            }
            Op::Unset(n) => {
                if let Err(e) = unset_env_variable(n) {
                    port_failed = Some(e);
                    break;
                }
                next();
            }
            Op::Get(n) => assert_eq!(get_env_variable(n), text_out(&next()), "{what}"),
            Op::Present(n) => {
                assert_eq!(
                    json!(is_env_variable_present(n)),
                    *result(&next()),
                    "{what}"
                )
            }
            Op::Load => {
                let mut context = Context::new();
                context.set_environment_mode(EnvironmentMode::LoadAll);
                context.load_environment();
                next();
                next();
                next();
                let port: Vec<(Vec<u8>, Vec<u8>)> = (0..context.num_string_vars())
                    .map(|i| {
                        (
                            context.string_var_name_by_index(i).to_vec(),
                            context.string_var_by_index(i).to_vec(),
                        )
                    })
                    .collect();
                assert_pairs(&port, &next(), &what);
            }
        }
    }
    // Where the wheel's process ended, the port returned an error, and only there.
    assert_eq!(
        port_failed.is_some(),
        response.get("crashed").is_some(),
        "{name}: {port_failed:?} {response}"
    );
    set_env_provider(None);
}

fn b(s: &str) -> Vec<u8> {
    s.as_bytes().to_vec()
}

#[test]
fn environment_functions_match_the_wheel() {
    check(
        "equal signs",
        &[],
        &[
            Op::Set(b("A=B"), b("C")),
            Op::Get(b("A")),
            Op::Get(b("A=B")),
            Op::Present(b("A")),
            Op::Unset(b("Q=R")),
            Op::Get(b("Q")),
            Op::Present(b("Q")),
            Op::Set(b("=X"), b("1")),
            Op::Get(b("=X")),
            Op::Present(b("=X")),
            Op::Set(b("E="), b("")),
            Op::Get(b("E")),
            Op::Present(b("E")),
            Op::Set(b("F"), b("")),
            Op::Present(b("F")),
            Op::Load,
        ],
    );
    check(
        "case",
        &[("SHOT", "a")],
        &[
            Op::Get(b("shot")),
            Op::Present(b("Shot")),
            Op::Set(b("Shot"), b("b")),
            Op::Get(b("SHOT")),
            Op::Load,
            Op::Set(b("\u{e9}t\u{e9}"), b("1")),
            Op::Get(b("\u{c9}T\u{c9}")),
            Op::Present(b("\u{c9}T\u{c9}")),
            Op::Set(b("\u{c9}T\u{c9}"), b("2")),
            Op::Get(b("\u{e9}t\u{e9}")),
            Op::Load,
            Op::Unset(b("\u{c9}T\u{c9}")),
            Op::Present(b("\u{e9}t\u{e9}")),
            Op::Load,
        ],
    );
    // Code units where Windows' folding and Unicode's simple uppercase mapping differ: the
    // dotless i, the micro sign and the long s, which Windows doesn't map; a Greek letter with
    // iota subscript, which it maps to its titlecase form; and a titlecase letter.
    check(
        "folding",
        &[],
        &[
            Op::Set(b("\u{131}"), b("1")),
            Op::Present(b("I")),
            Op::Set(b("\u{b5}"), b("2")),
            Op::Present(b("\u{39c}")),
            Op::Set(b("\u{17f}"), b("3")),
            Op::Present(b("S")),
            Op::Set(b("x\u{1f80}"), b("4")),
            Op::Get(b("X\u{1f88}")),
            Op::Present(b("X\u{1f08}\u{399}")),
            Op::Set(b("\u{1c5}"), b("5")),
            Op::Present(b("\u{1c4}")),
            Op::Get(b("\u{1c5}")),
            Op::Set(b("\u{3c3}"), b("6")),
            Op::Get(b("\u{3a3}")),
            Op::Present(b("\u{3c2}")),
            Op::Load,
        ],
    );
    check(
        "bytes",
        &[],
        &[
            Op::Set(b("V"), b"a\xffb".to_vec()),
            Op::Get(b("V")),
            Op::Set(b"N\xff".to_vec(), b("1")),
            Op::Get(b"N\xfe".to_vec()),
            Op::Get(b"N\xff".to_vec()),
            Op::Set(b("W"), b"\xed\xa0\x80".to_vec()),
            Op::Get(b("W")),
            Op::Load,
        ],
    );
    check(
        "order",
        &[],
        &[
            Op::Set(b("ZZZ"), b("1")),
            Op::Set(b("AAA"), b("2")),
            Op::Set(b("MMM"), b("3")),
            Op::Set(b("aaa"), b("4")),
            Op::Unset(b("ZZZ")),
            Op::Load,
            Op::Get(b("AAA")),
        ],
    );
    check(
        "a value just short of the limit",
        &[],
        &[Op::Set(b("L"), b(&"x".repeat(32_766))), Op::Present(b("L"))],
    );
    check(
        "a value at the limit",
        &[],
        &[Op::Set(b("L"), b(&"x".repeat(32_767))), Op::Present(b("L"))],
    );
    check(
        "a name at the limit",
        &[],
        &[Op::Unset(b(&"N".repeat(32_767))), Op::Present(b("N"))],
    );
}
