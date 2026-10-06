// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Compiling `std::regex` as the Windows wheel does ([`Library::Msvc`]), against the wheel: a
//! file rule's regular expression is compiled when a config loads (FileRules.cpp:263-282
//! `ValidateRegularExpression` @ v2.5.2), so a config with the expression as a rule, loaded
//! through the oracle's `config_serialize`, either loads or fails with
//! "File rules: invalid regular expression '<expression>': '<what()>'.". The port compiles the
//! same expression: it must succeed exactly when the wheel loads, and otherwise fail with the
//! same `what()`.
//!
//! The expressions: one per path through the parser (each error, each escape, bracket
//! expressions, groups, quantifiers, the limit of 1000 capture groups), every expression of up
//! to two characters from the parser's alphabet, and a sample of longer ones (pinned by a
//! digest).
//!
//! Only on Windows: the Linux wheel compiles with libstdc++ (a later chunk).
#![cfg(windows)]

use ocio_ops::std_regex::{ErrorType, Library, Regex};
use ocio_testkit::Oracle;
use ocio_testkit::fixtures::sha256_hex;
use ocio_testkit::oracle::BatchCall;
use serde_json::json;

/// A config whose first file rule has the regular expression `pattern` (text, no control
/// characters), quoted for YAML.
fn yaml(pattern: &str) -> String {
    let quoted = pattern.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        "ocio_profile_version: 2\nroles:\n  default: raw\nfile_rules:\n  - !<Rule> {{name: r, \
         colorspace: raw, regex: \"{quoted}\"}}\n  - !<Rule> {{name: Default, colorspace: \
         raw}}\ncolorspaces:\n  - !<ColorSpace>\n    name: raw\n"
    )
}

/// One expression per path through the parser.
const HAND: &[&str] = &[
    // errors
    "[a",
    "(a",
    "a)",
    "*a",
    "a{2,1}",
    "\\",
    "a{",
    "(?<a>b)",
    "[b-a]",
    "\\1",
    "a**",
    "x{99999999999}",
    "[[:foo:]]",
    "a{,2}",
    "+",
    "?",
    "a|*",
    "\\c",
    "\\x1",
    "\\u12",
    "(?:a",
    "a{2",
    "[\\d-z]",
    "\\b*",
    "^*",
    "$*",
    "a{2}{3}",
    "(?!a)*",
    "[a-\\d]",
    "\\01",
    "[[:alpha:]",
    "x{1,99999999999}",
    "\\p{L}",
    "a++",
    "(a)\\2",
    "(?",
    "(?x)",
    "}",
    "]",
    "a}",
    "a]",
    "[]a]",
    "[[=a",
    "[[==]]",
    "[[..]]",
    "[[.a",
    "[[:a",
    "[[:]]",
    "[[=a.]]",
    "[[.a=]]",
    "[[:alpha=]]",
    "\\c1",
    "\\c_",
    "\\xGG",
    "\\x4",
    "\\u00F",
    "\\u0100",
    "\\uFFFF",
    "(a\\1)",
    "\\1(a)",
    "a{2147483648}",
    "a{0,2147483648}",
    "a{2147483647}",
    "a{1,2147483647}",
    "(?=",
    "(?!",
    "(",
    ")",
    "[",
    "{",
    "{1}",
    "a{1}{",
    "a{1,2}{3}",
    "\\k<a>",
    "[a-]",
    "[-a]",
    "[\\d-]",
    "[-\\d]",
    "[\\w-\\d]",
    "[a-\\]",
    "[\\0]",
    "[\\1]",
    "[\\00]",
    "[\\b-a]",
    "[a-\\b]",
    "[\\x41-\\x40]",
    "[\\cA-\\cB]",
    "[[.a.]-b]",
    "[a-[.b.]]",
    "[[:alpha:]-z]",
    "[[=a=]-z]", // valid
    "(?=a)b",
    "\\k",
    "()",
    "[]",
    "[^]",
    "\\0",
    "[[.a.]]",
    "[[=a=]]",
    "a{1,2}?",
    "[\\b]",
    "\\B",
    "a{0}",
    "[[:alpha:]]",
    "[[:ALPHA:]]",
    "[[:w:]]",
    "[[:W:]]",
    "[[:d:]]",
    "[[:s:]]",
    "[[:Blank:]]",
    "[[:xdigit:]]",
    "[[:print:]]",
    "[[:graph:]]",
    "[[:punct:]]",
    "[[:cntrl:]]",
    "[[:upper:]]",
    "[[:lower:]]",
    "[[:alnum:]]",
    "[[:space:]]",
    "[[:digit:]]",
    "[[.ab.]]",
    "[[=ab=]]",
    "\\cA",
    "\\cz",
    "\\x41",
    "\\xff",
    "\\u00ff",
    "\\u00FF",
    "\\f\\n\\r\\t\\v",
    "\\d\\D\\s\\S\\w\\W",
    "[\\d\\D\\s\\S\\w\\W]",
    "\\a",
    "\\e",
    "\\/",
    "\\-",
    "\\.",
    "(a)\\1",
    "(a)(b)\\2\\1",
    "((a)\\2)",
    "(?:a)\\1",
    "a|",
    "|a",
    "|",
    "||",
    "(|)",
    "(a|b|)",
    "a{1,}",
    "a{1,}?",
    "a*?",
    "a+?",
    "a??",
    "(?:)",
    "(?=)",
    "(?!)",
    "^$",
    "\\b\\B",
    "a\\b",
    ".*\\.(exr|EXR)$",
    "[^/]*/[^/]*",
    "a{0,0}",
    "a{3,3}",
    "x{1000}",
    "(a)(b)(c)(d)(e)(f)(g)(h)(i)(j)\\10",
    "(a)\\10",
    "[\\]]",
    "[\\[]",
    "[a\\-z]",
    "[\\^a]",
    "[a^]",
    "[\\\\]",
    "\\\\",
    "a\\",
    "[\\",
    "[a-z",
    "[a-",
    "(?:",
    "(a|",
    "a{1,2,3}",
    "a{ 1}",
    "a{1 }",
    "a{-1}",
    "a{+1}",
];

/// The parser's alphabet: its meta characters and the letters and digits that escapes read.
const ALPHABET: &[u8] = b"a1()[]{}*+?|\\^$.,-:=!bBcdDsSwWxu0";

/// A small deterministic generator (an LCG), so the sample is the same on every run.
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

fn generated() -> Vec<String> {
    let mut out = Vec::new();
    for &a in ALPHABET {
        out.push(String::from_utf8(vec![a]).unwrap());
        for &b in ALPHABET {
            out.push(String::from_utf8(vec![a, b]).unwrap());
        }
    }
    let mut rng = Lcg(2024);
    for _ in 0..3000 {
        let len = 3 + rng.next() % 6;
        let s: Vec<u8> = (0..len)
            .map(|_| ALPHABET[rng.next() % ALPHABET.len()])
            .collect();
        out.push(String::from_utf8(s).unwrap());
    }
    out
}

fn patterns() -> Vec<String> {
    let mut out: Vec<String> = HAND.iter().map(|s| s.to_string()).collect();
    out.extend(generated());
    // The capture group limit: 999 groups compile, 1000 don't.
    out.push("()".repeat(998));
    out.push("()".repeat(999));
    out.push(format!("{}(a)", "()".repeat(998)));
    out.push("()".repeat(1000));
    out.push(r"[a-\xff]".to_string());
    out.push(r"[\x80-\xff]".to_string());
    out
}

/// The generated expressions can't change unnoticed.
#[test]
fn the_generated_expressions_are_pinned() {
    let joined = generated().join("\0");
    let digest = sha256_hex(joined.as_bytes());
    assert_eq!(
        digest,
        "d84b761cfbcd7fbd6fdb5c02356bdd8b05e3ee773c37ae637999233a98ab411f"
    );
}

#[test]
fn compiling_matches_the_wheel() {
    let patterns = patterns();
    let yamls: Vec<String> = patterns.iter().map(|p| yaml(p)).collect();
    let calls: Vec<BatchCall<'_>> = yamls
        .iter()
        .map(|y| BatchCall {
            cmd: "config_serialize",
            args: json!({"config": {"yaml": y}}),
            blobs: Vec::new(),
        })
        .collect();
    let mut failures = Vec::new();
    for (pattern, response) in patterns.iter().zip(Oracle::get().batch(&calls, true)) {
        let response = response.unwrap_or_else(|e| panic!("{pattern:?}: {e}"));
        let wheel = response.result.get("exception").map(|e| {
            e["message"]
                .as_str()
                .unwrap_or_else(|| panic!("{e}"))
                .to_string()
        });
        let port = Regex::new(pattern.as_bytes(), Library::Msvc);
        match (wheel, port) {
            (None, Ok(_)) => {}
            (Some(message), Err(error)) => {
                let expected = format!(
                    "invalid regular expression '{pattern}': '{}'.",
                    error.what()
                );
                if !message.contains(&expected) {
                    failures.push(format!(
                        "{pattern:?}: wheel {message:?}, port {:?}",
                        error.what()
                    ));
                }
            }
            (wheel, port) => failures.push(format!(
                "{pattern:?}: wheel {wheel:?}, port {:?}",
                port.map(|_| ()).map_err(|e| e.what())
            )),
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} expressions differ:\n{}",
        failures.len(),
        patterns.len(),
        failures.join("\n")
    );
}

/// Groups nested 5000 deep compile as in the wheel; deeper, where the wheel's recursion comes
/// close to overflowing its stack (about 7,800 to 8,400 levels, docs/improvements.md U-53),
/// the port refuses them with `error_stack`.
#[test]
fn deep_nesting() {
    let nested = |open: &str, depth: usize| format!("{}a{}", open.repeat(depth), ")".repeat(depth));
    let patterns: Vec<String> = ["(?:", "(?=", "(?!"]
        .iter()
        .map(|open| nested(open, 5000))
        .chain([format!(
            "{}{}a{}",
            "(".repeat(990),
            "(?:".repeat(4010),
            ")".repeat(5000)
        )])
        .collect();
    let yamls: Vec<String> = patterns.iter().map(|p| yaml(p)).collect();
    let calls: Vec<BatchCall<'_>> = yamls
        .iter()
        .map(|y| BatchCall {
            cmd: "config_serialize",
            args: json!({"config": {"yaml": y}}),
            blobs: Vec::new(),
        })
        .collect();
    for (pattern, response) in patterns.iter().zip(Oracle::get().batch(&calls, true)) {
        let response = response.unwrap_or_else(|e| panic!("{e}"));
        assert!(
            response.result.get("exception").is_none(),
            "{}",
            response.result
        );
        assert!(Regex::new(pattern.as_bytes(), Library::Msvc).is_ok());
    }

    for open in ["(?:", "(?=", "(?!", "("] {
        let error = Regex::new(nested(open, 5001).as_bytes(), Library::Msvc).unwrap_err();
        assert_eq!(error.code(), ErrorType::Stack, "{open}");
    }
}
