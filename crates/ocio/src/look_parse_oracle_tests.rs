// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! LookParse against the wheel, through the oracle's `config_calls`: `SplitStringEnvStyle`
//! through `Config.setActiveDisplays` and `getActiveDisplays` (which split the same way), and
//! the parsed options and their text through the message of a look transform whose looks all
//! fail. Written by the p3-model-objects verifier (its killing tests).

use super::*;
use ocio_testkit::Oracle;
use ocio_testkit::oracle_values::{bytes, bytes_arg, exception};
use serde_json::{Value, json};

fn raw(v: &Value) -> Vec<u8> {
    if v.get("bytes").is_some() {
        bytes(v)
    } else {
        let h = v["undecodable"].as_str().unwrap();
        (0..h.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap())
            .collect()
    }
}

fn inputs() -> Vec<Vec<u8>> {
    let alphabet: &[u8] = b"a,:\" b";
    let mut out: Vec<Vec<u8>> = vec![Vec::new()];
    let mut frontier: Vec<Vec<u8>> = vec![Vec::new()];
    for _ in 0..5 {
        let mut next = Vec::new();
        for s in &frontier {
            for &c in alphabet {
                let mut t = s.clone();
                t.push(c);
                next.push(t);
            }
        }
        out.extend(next.iter().cloned());
        frontier = next;
    }
    for extra in [
        &b"\t\"a,b\"\x01 , c"[..],
        b"\x01:\x02",
        b"\"\"",
        b"\"",
        b"a\"b\"c,d",
        b"\"a:b\":c",
        b"x,\"y:z\",",
        b" \"  q  \" ",
        b"\xa0a\xa0,b",
    ] {
        out.push(extra.to_vec());
    }
    out
}

#[test]
fn split_string_env_style_matches_the_wheel() {
    let all = inputs();
    let mut failures = Vec::new();
    for chunk in all.chunks(3000) {
        let mut calls = Vec::new();
        for s in chunk {
            calls.push(json!({"call": "setActiveDisplays", "args": [bytes_arg(s)]}));
            calls.push(json!({"call": "getActiveDisplays"}));
        }
        let response = Oracle::get()
            .call(
                "config_calls",
                json!({"config": "new", "calls": calls}),
                &[],
            )
            .result;
        let results = response["calls"].as_array().unwrap();
        for (k, s) in chunk.iter().enumerate() {
            let set = &results[2 * k];
            let get = &results[2 * k + 1];
            let port = split_string_env_style(s);
            match (set.get("exception"), port) {
                (Some(_), Err(e)) => {
                    let w = exception(set).1;
                    if w != e.what() {
                        failures.push(format!(
                            "{:?}: wheel err {:?} port err {:?}",
                            String::from_utf8_lossy(s),
                            String::from_utf8_lossy(&w),
                            String::from_utf8_lossy(e.what())
                        ));
                    }
                }
                (None, Ok(mut v)) => {
                    if v.len() == 1 && v[0].is_empty() {
                        v.clear();
                    }
                    let wheel: Vec<Vec<u8>> = get["result"]
                        .as_array()
                        .unwrap_or_else(|| panic!("{get}"))
                        .iter()
                        .map(raw)
                        .collect();
                    if wheel != v {
                        failures.push(format!(
                            "{:?}: wheel {:?} port {:?}",
                            String::from_utf8_lossy(s),
                            wheel
                                .iter()
                                .map(|x| String::from_utf8_lossy(x).into_owned())
                                .collect::<Vec<_>>(),
                            v.iter()
                                .map(|x| String::from_utf8_lossy(x).into_owned())
                                .collect::<Vec<_>>()
                        ));
                    }
                }
                (w, p) => failures.push(format!(
                    "{:?}: wheel {:?} port {:?}",
                    String::from_utf8_lossy(s),
                    w,
                    p.map_err(|e| String::from_utf8_lossy(e.what()).into_owned())
                )),
            }
        }
    }
    println!("checked {} inputs", all.len());
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures
            .iter()
            .take(40)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Token::parse and the serializers against the wheel: a LookTransform whose options all fail
/// on a missing file reports each option as `(serialized) message`, joined by `  ...  `.
#[test]
fn serialize_matches_the_wheel() {
    let yaml = "ocio_profile_version: 2\nroles: {default: raw}\ncolorspaces:\n  - !<ColorSpace> {name: raw}\nlooks:\n  - !<Look> {name: a, process_space: raw, transform: !<FileTransform> {src: missing_a.spi1d}}\n  - !<Look> {name: b, process_space: raw, transform: !<FileTransform> {src: missing_b.spi1d}}\n";
    let inputs: [&[u8]; 7] = [
        b"++a, --b | +b",
        b"a,-b|-a,b",
        b"--a,++b|b",
        b"a:-b | -a",
        b"\"a\", -b | b",
        b"a,,b|b",
        b"-a|--b|+++a",
    ];
    let mut calls = Vec::new();
    for s in inputs {
        calls.push(json!({"call": "getProcessor", "args": [{"transform": {"class": "LookTransform",
            "calls": [["setSrc", "raw"], ["setDst", "raw"], ["setLooks", String::from_utf8(s.to_vec()).unwrap()]]}}]}));
    }
    let response = Oracle::get()
        .call(
            "config_calls",
            json!({"config": {"yaml": yaml}, "calls": calls}),
            &[],
        )
        .result;
    let results = response["calls"].as_array().unwrap();
    let mut failures = Vec::new();
    for (k, s) in inputs.iter().enumerate() {
        let msg = exception(&results[k]).1;
        let msg = String::from_utf8(msg).unwrap();
        let wheel: Vec<String> = msg
            .split("  ...  ")
            .map(|seg| {
                let end = seg
                    .find(") The specified file")
                    .unwrap_or_else(|| panic!("{msg}"));
                seg[1..end].to_string()
            })
            .collect();
        let mut r = LookParseResult::default();
        r.parse(s).unwrap();
        let port: Vec<String> = r
            .options()
            .iter()
            .map(|o| {
                let mut v = Vec::new();
                LookParseResult::serialize(&mut v, o);
                String::from_utf8(v).unwrap()
            })
            .collect();
        println!(
            "{:?}: wheel {wheel:?} port {port:?}",
            String::from_utf8_lossy(s)
        );
        if wheel != port {
            failures.push(format!(
                "{:?}: wheel {wheel:?} port {port:?}",
                String::from_utf8_lossy(s)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
