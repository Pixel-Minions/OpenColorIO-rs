// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `ocio::FileRules` against the wheel's, through the oracle's `config_calls`: the same calls on
//! a new `FileRules` on both sides (rules inserted, refused, changed, moved and removed, their
//! custom keys), each call's result or exception compared byte for byte, then the rules'
//! `repr()` (upstream's `operator<<`) and every getter of every rule.
//!
//! The glob rules' patterns and extensions are turned into regular expressions, sanitized
//! with `regex_replace` and compiled by each wheel's `std::regex`; an expression that doesn't
//! compile gives a message with the sanitized expression, which shows what `regex_replace`
//! made of it.

use ocio::FileRules;
use ocio_testkit::Oracle;
use ocio_testkit::oracle_values::{bytes_arg, hex, log};
use serde_json::{Value, json};

/// The port's side of a call: its outcome, as the oracle writes a call's.
type PortCall = Box<dyn Fn(&mut FileRules) -> Value>;

struct Step {
    call: Value,
    port: PortCall,
}

fn step(call: Value, port: impl Fn(&mut FileRules) -> Value + 'static) -> Step {
    Step {
        call,
        port: Box::new(port),
    }
}

fn arg(s: &[u8]) -> Value {
    bytes_arg(s)
}

fn text_out(s: &[u8]) -> Value {
    match std::str::from_utf8(s) {
        Ok(_) => json!({"result": bytes_arg(s)}),
        Err(_) => json!({"undecodable": hex(s)}),
    }
}

fn error_out(e: ocio::Exception) -> Value {
    match std::str::from_utf8(e.what()) {
        Ok(_) => json!({"exception": {"type": "Exception", "message": bytes_arg(e.what())}}),
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

// ---------------------------------------------------------------------------------------------
// Calls

fn insert_glob(index: usize, name: &[u8], cs: &[u8], pattern: &[u8], extension: &[u8]) -> Step {
    let (n, c, p, e) = (
        name.to_vec(),
        cs.to_vec(),
        pattern.to_vec(),
        extension.to_vec(),
    );
    step(
        json!({"call": "insertRule", "args": [index, arg(&n), arg(&c), arg(&p), arg(&e)]}),
        move |r| unit_out(r.insert_rule(index, &n, &c, &p, &e)),
    )
}

fn insert_regex(index: usize, name: &[u8], cs: &[u8], regex: &[u8]) -> Step {
    let (n, c, x) = (name.to_vec(), cs.to_vec(), regex.to_vec());
    step(
        json!({"call": "insertRule", "args": [index, arg(&n), arg(&c), arg(&x)]}),
        move |r| unit_out(r.insert_regex_rule(index, &n, &c, &x)),
    )
}

fn insert_path_search(index: usize) -> Step {
    step(
        json!({"call": "insertPathSearchRule", "args": [index]}),
        move |r| unit_out(r.insert_path_search_rule(index)),
    )
}

type Setter = fn(&mut FileRules, usize, &[u8]) -> ocio::Result<()>;

fn set(name: &str, index: usize, value: &[u8], port: Setter) -> Step {
    let v = value.to_vec();
    step(json!({"call": name, "args": [index, arg(&v)]}), move |r| {
        unit_out(port(r, index, &v))
    })
}

fn set_pattern(index: usize, v: &[u8]) -> Step {
    set("setPattern", index, v, |r, i, v| r.set_pattern(i, v))
}

fn set_extension(index: usize, v: &[u8]) -> Step {
    set("setExtension", index, v, |r, i, v| r.set_extension(i, v))
}

fn set_regex(index: usize, v: &[u8]) -> Step {
    set("setRegex", index, v, |r, i, v| r.set_regex(i, v))
}

fn set_color_space(index: usize, v: &[u8]) -> Step {
    set("setColorSpace", index, v, |r, i, v| r.set_color_space(i, v))
}

fn set_custom_key(index: usize, key: &[u8], value: &[u8]) -> Step {
    let (k, v) = (key.to_vec(), value.to_vec());
    step(
        json!({"call": "setCustomKey", "args": [index, arg(&k), arg(&v)]}),
        move |r| unit_out(r.set_custom_key(index, &k, &v)),
    )
}

fn set_default_rule_color_space(v: &[u8]) -> Step {
    let v = v.to_vec();
    step(
        json!({"call": "setDefaultRuleColorSpace", "args": [arg(&v)]}),
        move |r| unit_out(r.set_default_rule_color_space(&v)),
    )
}

type Mover = fn(&mut FileRules, usize) -> ocio::Result<()>;

fn index_call(name: &str, index: usize, port: Mover) -> Step {
    step(json!({"call": name, "args": [index]}), move |r| {
        unit_out(port(r, index))
    })
}

fn remove_rule(index: usize) -> Step {
    index_call("removeRule", index, |r, i| r.remove_rule(i))
}

fn increase(index: usize) -> Step {
    index_call("increaseRulePriority", index, |r, i| {
        r.increase_rule_priority(i)
    })
}

fn decrease(index: usize) -> Step {
    index_call("decreaseRulePriority", index, |r, i| {
        r.decrease_rule_priority(i)
    })
}

/// The rules' `repr()`, their number, `isDefault`, every getter of every rule (and of the
/// index past them), and `getIndexForRule` of each probe name.
fn getters(names: &[&[u8]]) -> Vec<Step> {
    let mut out = vec![
        step(json!({"call": "__repr__"}), |r| text_out(&r.to_bytes())),
        step(
            json!({"call": "getNumEntries"}),
            |r| json!({"result": r.num_entries()}),
        ),
        step(
            json!({"call": "isDefault"}),
            |r| json!({"result": r.is_default()}),
        ),
    ];
    // The rules' number varies: the getters ask up to a fixed index, the ones past the end
    // raising.
    for i in 0..6usize {
        type Getter = fn(&FileRules, usize) -> ocio::Result<Vec<u8>>;
        let getters: [(&str, Getter); 5] = [
            ("getName", |r, i| r.name(i).map(<[u8]>::to_vec)),
            ("getPattern", |r, i| r.pattern(i).map(<[u8]>::to_vec)),
            ("getExtension", |r, i| r.extension(i).map(<[u8]>::to_vec)),
            ("getRegex", |r, i| r.regex(i).map(<[u8]>::to_vec)),
            ("getColorSpace", |r, i| r.color_space(i)),
        ];
        for (name, getter) in getters {
            out.push(step(json!({"call": name, "args": [i]}), move |r| {
                bytes_result(getter(r, i))
            }));
        }
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

/// Runs `steps` on a new `FileRules` on both sides, each followed by the getters; `None` is a
/// copy (`copy.deepcopy`: `createEditableCopy`) that the steps after it act on.
fn check(label: &str, steps: Vec<Option<Step>>, names: &[&[u8]]) {
    let mut sequence: Vec<Option<Step>> = getters(names).into_iter().map(Some).collect();
    for s in steps {
        sequence.push(s);
        sequence.extend(getters(names).into_iter().map(Some));
    }
    let mut calls = vec![json!({"new": "FileRules", "as": "fr"})];
    let mut on = "fr".to_string();
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

    let mut rules = FileRules::new();
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

/// Rules inserted and refused (names, positions, color spaces, the reserved names in any case),
/// their setters on each kind of rule, custom keys, moves and removals, copies.
#[test]
fn rules_match_the_wheel() {
    let steps = vec![
        Some(insert_glob(0, b"exr", b"lin", b"*", b"exr")),
        Some(insert_glob(0, b"  spaced  ", b"cs", b"a?b", b"ext")),
        Some(insert_regex(1, b"rx", b"cs2", b".*\\.tif")),
        Some(insert_path_search(1)),
        Some(insert_path_search(0)),
        Some(insert_glob(0, b"EXR", b"x", b"*", b"*")),
        Some(insert_glob(0, b"default", b"x", b"*", b"*")),
        Some(insert_glob(0, b"", b"x", b"*", b"*")),
        Some(insert_glob(9, b"far", b"x", b"*", b"*")),
        Some(insert_glob(0, b"nocs", b"", b"*", b"*")),
        Some(insert_glob(0, b"nopat", b"x", b"", b"*")),
        Some(insert_glob(0, b"noext", b"x", b"*", b"")),
        Some(insert_regex(0, b"norx", b"x", b"")),
        Some(insert_regex(0, b"badrx", b"x", b"a(")),
        Some(insert_regex(0, b"colorspacenamepathsearch", b"", b"")),
        Some(set_pattern(0, b"*.ab")),
        Some(set_pattern(1, b"x")),
        Some(set_pattern(4, b"x")),
        Some(set_pattern(5, b"x")),
        Some(set_pattern(3, b"[")),
        Some(set_extension(0, b"Tif")),
        Some(set_extension(3, b"")),
        Some(set_extension(2, b"x")),
        Some(set_regex(0, b"^a.*$")),
        Some(set_regex(2, b"x")),
        Some(set_regex(3, b"")),
        Some(set_regex(3, b"(")),
        Some(set_color_space(2, b"cs")),
        Some(set_color_space(2, b"")),
        Some(set_color_space(4, b"")),
        Some(set_color_space(4, b"raw")),
        Some(set_custom_key(0, b"k2", b"v2")),
        Some(set_custom_key(0, b"k1", b"v1")),
        Some(set_custom_key(0, b"k2", b"")),
        Some(set_custom_key(0, b"", b"v")),
        Some(set_custom_key(4, b"key", b"value")),
        Some(set_custom_key(7, b"key", b"value")),
        Some(set_default_rule_color_space(b"")),
        Some(set_default_rule_color_space(b"raw")),
        None,
        Some(increase(0)),
        Some(increase(1)),
        Some(decrease(3)),
        Some(decrease(2)),
        Some(increase(4)),
        Some(remove_rule(4)),
        Some(remove_rule(1)),
        Some(remove_rule(9)),
        Some(set_default_rule_color_space(b"DEFAULT")),
    ];
    check(
        "rules",
        steps,
        &[
            b"exr",
            b"EXR",
            b"spaced",
            b"Default",
            b"colorspacenamepathsearch",
            b"missing",
            b"",
        ],
    );
}

/// The default rules, and back to them.
#[test]
fn default_rules_match_the_wheel() {
    let steps = vec![
        Some(set_custom_key(0, b"k", b"v")),
        Some(set_custom_key(0, b"k", b"")),
        Some(set_default_rule_color_space(b"Default")),
        Some(set_default_rule_color_space(b"other")),
        Some(set_default_rule_color_space(b"default")),
        Some(insert_glob(0, b"r", b"x", b"*", b"*")),
        Some(remove_rule(0)),
    ];
    check("default", steps, &[b"default"]);
}

/// Globs turned into regular expressions: their escapes, sets and refusals, and the
/// expressions `regex_replace` sanitizes, shown by the ones that don't compile (an escaped
/// closing parenthesis), on both libraries' terms.
#[test]
fn globs_match_the_wheel() {
    let patterns: &[&[u8]] = &[
        b"a.b",
        b"a+^${}()|",
        b"[abc]",
        b"[!abc]",
        b"[a-z]*",
        b"[]",
        b"[!]",
        b"[a",
        b"a]",
        b"[a[b]",
        b"[a.b]",
        b"[a\\.b]",
        b"[+^${}()|\\]",
        b"**",
        b"*?",
        b"?*",
        b"*?*",
        b"a**b\\",
        b"**\\",
        b"*.**\\",
        b"\n.*\\",
        b"x\n.**\\",
        b"\\..*\\",
        b"\xe9*",
    ];
    let extensions: &[&[u8]] = &[
        b"*", b"exr", b"EXR", b"e?r", b"[eE]xr", b"**", b"\xe9", b"a\\",
    ];
    let mut steps = vec![Some(insert_glob(0, b"g", b"cs", b"*", b"*"))];
    for p in patterns {
        steps.push(Some(set_pattern(0, p)));
    }
    steps.push(Some(set_pattern(0, b"*")));
    for e in extensions {
        steps.push(Some(set_extension(0, e)));
    }
    for (p, e) in [(&b"**\\"[..], &b"**"[..]), (b"a", b"**\\"), (b"", b"")] {
        steps.push(Some(insert_glob(0, b"g2", b"cs", p, e)));
    }
    check("globs", steps, &[]);
}
