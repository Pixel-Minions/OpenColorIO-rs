// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `ocio::ViewingRules` against the wheel's, through the oracle's `config_calls`: the same
//! calls on a new `ViewingRules` on both sides (rules inserted, refused and removed, their
//! color spaces, encodings and custom keys), each call's result or exception compared byte for
//! byte, then the rules' `repr()` (upstream's `operator<<`) and every getter of every rule.

use ocio::ViewingRules;
use ocio_testkit::Oracle;
use ocio_testkit::oracle_values::{bytes_arg, hex, log};
use serde_json::{Value, json};

/// The port's side of a call: its outcome, as the oracle writes a call's.
type PortCall = Box<dyn Fn(&mut ViewingRules) -> Value>;

struct Step {
    call: Value,
    port: PortCall,
}

fn step(call: Value, port: impl Fn(&mut ViewingRules) -> Value + 'static) -> Step {
    Step {
        call,
        port: Box::new(port),
    }
}

fn arg(s: &[u8]) -> Value {
    bytes_arg(s)
}

/// A string as the binding returns it: `{"bytes"}`, or `{"undecodable"}` where it isn't UTF-8.
fn text(s: &[u8]) -> Value {
    match std::str::from_utf8(s) {
        Ok(_) => bytes_arg(s),
        Err(_) => json!({ "undecodable": hex(s) }),
    }
}

fn text_out(s: &[u8]) -> Value {
    match std::str::from_utf8(s) {
        Ok(_) => json!({"result": bytes_arg(s)}),
        Err(_) => json!({"undecodable": hex(s)}),
    }
}

fn exception(e: &ocio::Exception) -> Value {
    json!({"type": "Exception", "message": bytes_arg(e.what())})
}

fn error_out(e: ocio::Exception) -> Value {
    match std::str::from_utf8(e.what()) {
        Ok(_) => json!({ "exception": exception(&e) }),
        Err(_) => json!({"undecodable": hex(e.what())}),
    }
}

fn unit_out(r: ocio::Result<()>) -> Value {
    match r {
        Ok(()) => json!({"result": null}),
        Err(e) => error_out(e),
    }
}

fn value_out(r: ocio::Result<Value>) -> Value {
    match r {
        Ok(v) => json!({ "result": v }),
        Err(e) => error_out(e),
    }
}

fn bytes_result(r: ocio::Result<Vec<u8>>) -> Value {
    match r {
        Ok(v) => text_out(&v),
        Err(e) => error_out(e),
    }
}

type Count = fn(&ViewingRules, usize) -> ocio::Result<usize>;
type Element = for<'a> fn(&'a ViewingRules, usize, usize) -> ocio::Result<Option<&'a [u8]>>;

/// A rule's color spaces or encodings as the binding's iterator gives them, read element by
/// element (`len()`, then `[i]`): a rule index past the end raises in `len()`.
fn tokens_out(r: &ViewingRules, rule: usize, count: Count, element: Element) -> Value {
    let n = match count(r, rule) {
        Ok(n) => n,
        Err(e) => return json!({"result": {"exception": exception(&e)}}),
    };
    let items: Vec<Value> = (0..n)
        .map(|i| match element(r, rule, i) {
            Ok(Some(t)) => text(t),
            Ok(None) => Value::Null,
            Err(e) => json!({ "exception": exception(&e) }),
        })
        .collect();
    json!({ "result": items })
}

// ---------------------------------------------------------------------------------------------
// Calls

fn insert(index: usize, name: &[u8]) -> Step {
    let n = name.to_vec();
    step(
        json!({"call": "insertRule", "args": [index, arg(&n)]}),
        move |r| unit_out(r.insert_rule(index, &n)),
    )
}

fn add_color_space(index: usize, cs: &[u8]) -> Step {
    let v = cs.to_vec();
    step(
        json!({"call": "addColorSpace", "args": [index, arg(&v)]}),
        move |r| unit_out(r.add_color_space(index, &v)),
    )
}

fn add_encoding(index: usize, enc: &[u8]) -> Step {
    let v = enc.to_vec();
    step(
        json!({"call": "addEncoding", "args": [index, arg(&v)]}),
        move |r| unit_out(r.add_encoding(index, &v)),
    )
}

fn remove_color_space(index: usize, cs: usize) -> Step {
    step(
        json!({"call": "removeColorSpace", "args": [index, cs]}),
        move |r| unit_out(r.remove_color_space(index, cs)),
    )
}

fn remove_encoding(index: usize, enc: usize) -> Step {
    step(
        json!({"call": "removeEncoding", "args": [index, enc]}),
        move |r| unit_out(r.remove_encoding(index, enc)),
    )
}

fn set_custom_key(index: usize, key: &[u8], value: &[u8]) -> Step {
    let (k, v) = (key.to_vec(), value.to_vec());
    step(
        json!({"call": "setCustomKey", "args": [index, arg(&k), arg(&v)]}),
        move |r| unit_out(r.set_custom_key(index, &k, &v)),
    )
}

fn remove_rule(index: usize) -> Step {
    step(json!({"call": "removeRule", "args": [index]}), move |r| {
        unit_out(r.remove_rule(index))
    })
}

/// The rules' `repr()`, their number, every getter of every rule (and of the indices past
/// them), and `getIndexForRule` of each probe name.
fn getters(names: &[&[u8]]) -> Vec<Step> {
    let mut out = vec![
        step(json!({"call": "__repr__"}), |r| text_out(&r.to_bytes())),
        step(
            json!({"call": "getNumEntries"}),
            |r| json!({"result": r.num_entries()}),
        ),
    ];
    // The rules' number varies: the getters ask up to a fixed index, the ones past the end
    // raising.
    for i in 0..5usize {
        out.push(step(json!({"call": "getName", "args": [i]}), move |r| {
            bytes_result(r.name(i).map(<[u8]>::to_vec))
        }));
        out.push(step(
            json!({"call": "getColorSpaces", "args": [i]}),
            move |r| {
                tokens_out(
                    r,
                    i,
                    ViewingRules::num_color_spaces,
                    ViewingRules::color_space,
                )
            },
        ));
        out.push(step(
            json!({"call": "getEncodings", "args": [i]}),
            move |r| tokens_out(r, i, ViewingRules::num_encodings, ViewingRules::encoding),
        ));
        out.push(step(
            json!({"call": "getNumCustomKeys", "args": [i]}),
            move |r| value_out(r.num_custom_keys(i).map(|n| json!(n))),
        ));
        for k in 0..3usize {
            out.push(step(
                json!({"call": "getCustomKeyName", "args": [i, k]}),
                move |r| bytes_result(r.custom_key_name(i, k).map(<[u8]>::to_vec)),
            ));
            out.push(step(
                json!({"call": "getCustomKeyValue", "args": [i, k]}),
                move |r| bytes_result(r.custom_key_value(i, k).map(<[u8]>::to_vec)),
            ));
        }
    }
    for name in names {
        let n = name.to_vec();
        out.push(step(
            json!({"call": "getIndexForRule", "args": [arg(name)]}),
            move |r| value_out(r.index_for_rule(&n).map(|i| json!(i))),
        ));
    }
    out
}

/// Runs `steps` on a new `ViewingRules` on both sides, each followed by the getters; `None` is
/// a copy (`copy.deepcopy`: `createEditableCopy`) that the steps after it act on.
fn check(label: &str, steps: Vec<Option<Step>>, names: &[&[u8]]) {
    let mut sequence: Vec<Option<Step>> = getters(names).into_iter().map(Some).collect();
    for s in steps {
        sequence.push(s);
        sequence.extend(getters(names).into_iter().map(Some));
    }
    let mut calls = vec![json!({"new": "ViewingRules", "as": "vr"})];
    let mut on = "vr".to_string();
    for s in &sequence {
        match s {
            Some(s) => {
                let mut call = s.call.clone();
                call["on"] = json!(on);
                calls.push(call);
            }
            None => {
                calls.push(json!({"copy": on, "as": "copy"}));
                on = "copy".to_string();
            }
        }
    }
    let response = Oracle::get().call(
        "config_calls",
        json!({"config": "new", "calls": calls}),
        &[],
    );
    let wheel = &response.result;
    assert_eq!(wheel["config"], Value::Null, "{label}: {wheel}");
    let results = wheel["calls"].as_array().expect("the calls' results");
    assert_eq!(results.len(), sequence.len() + 1, "{label}");
    assert!(
        results[0].get("result").is_some(),
        "{label}: {}",
        results[0]
    );

    let mut rules = ViewingRules::new();
    let mut failures = Vec::new();
    for (i, (s, w)) in sequence.iter().zip(&results[1..]).enumerate() {
        assert!(log(&w["log"]).is_empty(), "{label}, call {i}: {w}");
        let mut w = w.clone();
        w.as_object_mut().expect("a call's outcome").remove("log");
        match s {
            Some(s) => {
                let port = (s.port)(&mut rules);
                if port != w {
                    failures.push(format!(
                        "{label}, call {i} {}: wheel {w}, port {port}",
                        s.call
                    ));
                }
            }
            None => rules = rules.clone(),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// ---------------------------------------------------------------------------------------------
// Cases

/// Rules inserted and refused (names trimmed, cut at a NUL, repeated in another case, empty,
/// positions), their color spaces and encodings (repeated in another case or with spaces,
/// blank, conflicting, removed), custom keys, removals and copies.
#[test]
fn rules_match_the_wheel() {
    let steps = vec![
        Some(insert(0, b"r0")),
        Some(insert(1, b"  r1  ")),
        Some(insert(3, b"far")),
        Some(insert(0, b"R0")),
        Some(insert(0, b"   ")),
        Some(insert(0, b"")),
        Some(insert(2, b"r2\0tail")),
        Some(insert(1, b"\xe9")),
        Some(add_color_space(0, b"cs0")),
        Some(add_color_space(0, b" CS0 ")),
        Some(add_color_space(0, b"  cs1\t")),
        Some(add_color_space(0, b"   ")),
        Some(add_color_space(0, b"")),
        Some(add_color_space(0, b"cs2\0x")),
        Some(add_color_space(9, b"cs")),
        Some(add_encoding(0, b"log")),
        Some(add_encoding(2, b"log")),
        Some(add_encoding(2, b"LOG")),
        Some(add_encoding(2, b"video ")),
        Some(add_encoding(2, b"")),
        Some(add_color_space(2, b"cs")),
        Some(add_encoding(1, b"\xff")),
        Some(set_custom_key(0, b"k2", b"v2")),
        Some(set_custom_key(0, b"k1", b"v1")),
        Some(set_custom_key(0, b"k2", b"")),
        Some(set_custom_key(0, b"", b"v")),
        Some(set_custom_key(3, b"k\0x", b"v\0y")),
        Some(set_custom_key(7, b"key", b"value")),
        None,
        Some(remove_color_space(0, 5)),
        Some(remove_color_space(0, 1)),
        Some(remove_encoding(2, 0)),
        Some(remove_encoding(2, 4)),
        Some(remove_encoding(0, 0)),
        Some(remove_rule(1)),
        Some(remove_rule(9)),
    ];
    check(
        "rules",
        steps,
        &[b"r0", b"R1", b" r1", b"r2", b"r2\0x", b"missing", b""],
    );
}

/// Color space and encoding indices past 2^31: upstream cuts them to an `int` before it checks
/// them (docs/improvements.md I-135), so 2^32 is index 0, and an index whose cut is negative
/// passes the check and removes nothing.
#[test]
fn large_indices_match_the_wheel() {
    const TWO_32: usize = 1 << 32;
    let steps = vec![
        Some(insert(0, b"cs")),
        Some(insert(1, b"enc")),
        Some(add_color_space(0, b"a")),
        Some(add_color_space(0, b"b")),
        Some(add_color_space(0, b"c")),
        Some(add_encoding(1, b"x")),
        Some(add_encoding(1, b"y")),
        Some(remove_color_space(0, 1 << 31)),
        Some(remove_color_space(0, TWO_32 - 1)),
        Some(remove_color_space(0, TWO_32 + 3)),
        Some(remove_color_space(0, TWO_32 + 1)),
        Some(remove_color_space(0, TWO_32)),
        Some(remove_encoding(1, (1 << 31) + 5)),
        Some(remove_encoding(1, TWO_32 + 2)),
        Some(remove_encoding(1, TWO_32 + 1)),
        Some(remove_color_space(5, TWO_32)),
        Some(remove_rule(TWO_32)),
    ];
    check("large indices", steps, &[]);
}
