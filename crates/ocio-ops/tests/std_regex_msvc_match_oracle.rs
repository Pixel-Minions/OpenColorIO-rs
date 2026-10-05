// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Matching with `std::regex` as the Windows wheel does ([`Library::Msvc`]), against the wheel:
//! a file rule's regular expression matches a path with `regex_match` (FileRules.cpp:488-494
//! @ v2.5.2), so the oracle's `file_rules_match` (O3.4), given a regex rule before the default
//! rule, reports rule 0 for each path the expression matches whole and rule 1 (the default)
//! for the others, or the `regex_error` the matcher threw. The port matches the same path with
//! the same expression.
//!
//! The expressions: hand cases for each kind of node (alternation, repetitions greedy and
//! not, simple and nested loops, groups, back references, assertions, word boundaries,
//! anchors, bracket expressions, collating elements), and a sample of generated ones (pinned
//! by a digest), each against paths over a small alphabet and some odd ones; and the matcher's
//! limits (`error_stack` past 600 nested matches, `error_complexity` past ten million steps).
//!
//! Only on Windows: the Linux wheel matches with libstdc++ (a later chunk).
#![cfg(windows)]

use ocio_ops::std_regex::{Library, Regex, regex_match};
use ocio_testkit::Oracle;
use ocio_testkit::fixtures::sha256_hex;
use ocio_testkit::oracle_values::{bytes, hex};
use serde_json::{Value, json};

const HAND: &[&str] = &[
    "a",
    "ab",
    "a|b",
    "a|ab",
    "ab|a",
    "(a|ab)(c|bcd)",
    "a*",
    "a+",
    "a?",
    "a*?",
    "a+?",
    "a??",
    "a{2}",
    "a{2,}",
    "a{1,3}",
    "a{1,3}?",
    "a{0}",
    "a{0,0}",
    "(a)*",
    "(a|b)*",
    "(a|b)+?b",
    "(a*)*",
    "(a*)+",
    "(a?)*",
    "(a|)*b",
    "(?:a|b)*a",
    "((a)|b)*",
    "(a*b*)*",
    "(ab|a)*b",
    "(a)\\1",
    "(a*)\\1",
    "(a|b)\\1",
    "(a*)b\\1",
    "((a)b)\\2",
    "(a)|\\1b",
    "(?=a)a",
    "(?=a)b",
    "(?!a).",
    "(?!a)b*",
    "a(?=b)",
    "(?=(a))\\1a",
    "(?!(a))\\1",
    "\\ba",
    "a\\b",
    "\\Ba",
    "a\\B",
    "\\b",
    "\\B",
    "^a",
    "a$",
    "^$",
    "^",
    "$",
    ".",
    ".*",
    ".+",
    "a.b",
    "[ab]",
    "[^ab]",
    "[a-c]",
    "[^a-c]*",
    "[\\d]",
    "[\\D]",
    "\\d+",
    "\\D",
    "\\s",
    "\\S",
    "\\w+",
    "\\W",
    "[[:alpha:]]+",
    "[[:digit:]]",
    "[[:space:]]",
    "[[:blank:]]",
    "[[:print:]]",
    "[[:graph:]]",
    "[[:punct:]]",
    "[[:cntrl:]]",
    "[[:upper:]]",
    "[[:lower:]]",
    "[[:xdigit:]]",
    "[[:w:]]",
    "[[=a=]]",
    "[[=A=]]",
    "[[.a.]]",
    "[[.a.]]b",
    "b[[.a.]]",
    "[[.ab.]]",
    "a[[.ab.]]",
    "[^[.a.]]",
    "\\x61",
    "\\u0062",
    "\\cJ",
    "\\n",
    "\\t",
    "a\\.b",
    "\\0",
    "[\\0a]",
    "(a)(b)?",
    "(a(b)?)+",
    "(a|(b))+",
    "((a)|(b))+\\3",
    "(a)+\\1",
    "(a{2})*",
    "(?:a{2})*b",
    "(ab)*?b",
    "a{2,3}?b",
    "(a|b){2}",
    "(a|b){1,2}?a",
    "(.*)\\1",
    "(.+)\\1",
    "(.)\\1*",
    ".*\\.(exr|EXR)$",
    "[^/]*/[^/]*",
    "(?:)",
    "()",
    "(|a)",
    "(a|)",
    "||",
    "a||b",
    "(a)|b",
    "(?:a)\\b",
];

/// The paths: every string over `ab` up to four characters, and some others.
fn paths() -> Vec<Vec<u8>> {
    let mut out: Vec<Vec<u8>> = vec![Vec::new()];
    let mut frontier: Vec<Vec<u8>> = vec![Vec::new()];
    for _ in 0..4 {
        let mut next = Vec::new();
        for s in &frontier {
            for &c in b"ab" {
                let mut t = s.clone();
                t.push(c);
                next.push(t);
            }
        }
        out.extend(next.iter().cloned());
        frontier = next;
    }
    for extra in [
        &b"c"[..],
        b"abcd",
        b"bcd",
        b"abc",
        b"A",
        b"aA",
        b"1",
        b"a1",
        b"_",
        b"a_b",
        b"a b",
        b" ",
        b"\t",
        b"\n",
        b"a\nb",
        b"\na",
        b"a\n",
        b"\r",
        b"a.b",
        b"a/b",
        b"x.exr",
        b"x.EXR",
        b"dir/x.exr",
        b"\x01",
        b"\x7f",
        b"\x80",
        b"\xe9",
        b"\xff",
        b"a\xffb",
        b"=",
        b"-",
    ] {
        out.push(extra.to_vec());
    }
    out
}

const ALPHABET: &[u8] = b"ab()[]|*+?{}12,.^$\\-:=!dwsSbB";

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) as usize
    }
}

/// Generated expressions that the port compiles (the compile test checks the wheel agrees).
fn generated() -> Vec<String> {
    let mut rng = Lcg(77);
    let mut out = Vec::new();
    while out.len() < 1500 {
        let len = 1 + rng.next() % 8;
        let s: Vec<u8> = (0..len)
            .map(|_| ALPHABET[rng.next() % ALPHABET.len()])
            .collect();
        let s = String::from_utf8(s).unwrap();
        if Regex::new(s.as_bytes(), Library::Msvc).is_ok() {
            out.push(s);
        }
    }
    out
}

/// The generated expressions can't change unnoticed.
#[test]
fn the_generated_expressions_are_pinned() {
    let joined = generated().join("\n");
    assert_eq!(
        sha256_hex(joined.as_bytes()),
        "0c65a1a8365428ab4f366b313412924a9133b3a76b6b43dfacef56257c0ac684"
    );
}

/// What the wheel reports for each path: matched by the rule, or not, or the exception.
#[derive(Debug, PartialEq)]
enum Outcome {
    Match(bool),
    Error(Vec<u8>),
}

/// The wheel's outcome of `pattern` (a regex rule) on each path.
fn wheel(cases: &[(String, Vec<Vec<u8>>)]) -> Vec<Vec<Outcome>> {
    let requests: Vec<Value> = cases
        .iter()
        .map(|(pattern, paths)| {
            json!({
                "rules": [{"name": "r", "colorspace": "raw",
                           "regex": {"bytes": hex(pattern.as_bytes())}}],
                "paths": paths.iter().map(|p| json!({"bytes": hex(p)})).collect::<Vec<_>>(),
            })
        })
        .collect();
    let mut out = Vec::new();
    for chunk in requests.chunks(200) {
        let response = Oracle::get()
            .call("file_rules_match", json!({"cases": chunk}), &[])
            .result;
        for case in response["cases"].as_array().expect("the cases") {
            assert_eq!(case["inserted"][0], Value::Null, "{case}");
            out.push(
                case["paths"]
                    .as_array()
                    .expect("the paths")
                    .iter()
                    .map(|p| match p.get("rule") {
                        Some(rule) => Outcome::Match(rule == 0),
                        None => Outcome::Error(bytes(&p["exception"]["message"])),
                    })
                    .collect(),
            );
        }
    }
    out
}

fn check(cases: Vec<(String, Vec<Vec<u8>>)>) {
    let wheel = wheel(&cases);
    let mut failures = Vec::new();
    for ((pattern, paths), wheel) in cases.iter().zip(wheel) {
        let re = Regex::new(pattern.as_bytes(), Library::Msvc).expect("compiles");
        for (path, wheel) in paths.iter().zip(wheel) {
            let port = match regex_match(path, &re) {
                Ok(m) => Outcome::Match(m),
                Err(e) => Outcome::Error(e.what().as_bytes().to_vec()),
            };
            if port != wheel {
                failures.push(format!(
                    "{pattern:?} on {:?}: wheel {wheel:?}, port {port:?}",
                    String::from_utf8_lossy(path)
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} differ:\n{}",
        failures.len(),
        failures
            .iter()
            .take(60)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn hand_expressions_match_as_in_the_wheel() {
    let paths = paths();
    check(
        HAND.iter()
            .map(|p| (p.to_string(), paths.clone()))
            .collect(),
    );
}

#[test]
fn generated_expressions_match_as_in_the_wheel() {
    let paths = paths();
    check(
        generated()
            .into_iter()
            .map(|p| (p, paths.clone()))
            .collect(),
    );
}

/// Past 600 nested matches MSVC's matcher throws `error_stack` (a loop with an alternation
/// recurses once per iteration); past ten million steps `error_complexity`.
#[test]
fn the_matchers_limits_are_the_wheels() {
    let long = |n: usize| vec![b'a'; n];
    let cases = vec![
        (
            "(a|b)*".to_string(),
            vec![
                long(10),
                long(250),
                long(298),
                long(299),
                long(300),
                long(301),
                long(1000),
            ],
        ),
        ("(?:a|b)*c".to_string(), vec![long(200), long(400)]),
        ("(a*)*b".to_string(), vec![long(10), long(20), long(25)]),
        ("(a|aa)*b".to_string(), vec![long(20), long(30)]),
        ("((a)\\2|b)*".to_string(), vec![long(100), long(300)]),
    ];
    check(cases);
}
