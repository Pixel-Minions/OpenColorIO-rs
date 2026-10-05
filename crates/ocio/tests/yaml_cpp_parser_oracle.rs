// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The port of yaml-cpp's parser against the wheel's, through the error a config load
//! reports.
//!
//! OCIO loads a config with `YAML::Load` before it reads anything, and wraps whatever yaml-cpp
//! throws: "Error: Loading the OCIO profile failed. " and the exception's `what()`
//! (`OCIOYaml::Read`, src/OpenColorIO/OCIOYaml.cpp:5419-5437 @ v2.5.2), cut at the first NUL
//! as OCIO's `Exception` takes a C string. So for each document the oracle loads
//! (`config_serialize`):
//! - where the port's parser throws, the wheel reports that `what()`: the message and the
//!   line and column;
//! - where it doesn't, the wheel's error isn't a `ParserException`: it comes from OCIO's own
//!   checks, or from yaml-cpp's node API ("operator[] call on a scalar" on a scalar
//!   document).
//!
//! `YAML::Load` parses the first document only, as here. The documents are hand-written ones
//! for each of the parser's errors and quirks, and generated ones: random characters, and
//! random sequences of YAML fragments.

use ocio::yaml_cpp::event_handler::{Anchor, EmitterStyle, EventHandler};
use ocio::yaml_cpp::mark::Mark;
use ocio::yaml_cpp::parser::Parser;
use ocio_testkit::fixtures::sha256_hex;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::{Oracle, assert_text_eq};
use serde_json::json;

/// An event handler that ignores the events.
struct NullHandler;

impl EventHandler for NullHandler {
    fn on_document_start(&mut self, _: Mark) {}
    fn on_document_end(&mut self) {}
    fn on_null(&mut self, _: Mark, _: Anchor) {}
    fn on_alias(&mut self, _: Mark, _: Anchor) {}
    fn on_scalar(&mut self, _: Mark, _: &[u8], _: Anchor, _: &[u8]) {}
    fn on_sequence_start(&mut self, _: Mark, _: &[u8], _: Anchor, _: EmitterStyle) {}
    fn on_sequence_end(&mut self) {}
    fn on_map_start(&mut self, _: Mark, _: &[u8], _: Anchor, _: EmitterStyle) {}
    fn on_map_end(&mut self) {}
}

/// The `what()` of the port's parser on the first document, if it throws.
fn port_error(yaml: &[u8]) -> Option<Vec<u8>> {
    let mut parser = Parser::new(yaml);
    match parser.handle_next_document(&mut NullHandler) {
        Ok(_) => None,
        Err(e) => Some(e.what()),
    }
}

const PREFIX: &str = "Error: Loading the OCIO profile failed. ";

/// Hand-written documents: each error the scanner and the parser report, the `%YAML`
/// version's number reading, and the stream's end-of-input and NUL characters.
const CASES: &[&str] = &[
    // flow collections
    "a: [b",
    "{a: b",
    "[a, b}",
    "{a: b]",
    "]",
    "a: }",
    "[a b c] d",
    "{a: b, c d, : e}",
    "[a: b, c: d, : e]",
    "[? a : b, [c]: d]",
    "{\"a\":b, 'c':d}",
    "[a,\n b]: c",
    // block collections
    "- a\nb",
    "a: b\n- c",
    "a:\n  - b\n - c",
    "a: - b",
    "- a: b\n  c: d\n e: f",
    "? a\n? b\n: c",
    "a: b: c",
    "a:\n\tb: c",
    "a: b\n\t- c",
    "- - - a\n  - b\n- c",
    "? - a\n  - b\n: - c",
    ": a",
    "a\n: b",
    // anchors, aliases and tags
    "&",
    "*",
    "&a[ b",
    "*a",
    "&a a: *a",
    "- &a b\n- *a\n- *b",
    "&a &b c",
    "!a !b c",
    "!<a",
    "!<a b>",
    "!a!",
    "!a,b!c d",
    "!! a",
    "! a",
    "!<tag:yaml.org,2002:str> a",
    "*a: b",
    // scalars
    "\"a",
    "'a",
    "\"a\\qb\"",
    "\"\\xZZ\"",
    "\"\\uD800\"",
    "\"\\U00110000\"",
    "\"\\U0010FFFF\\u00e9\\x41\"",
    "'it''s'",
    "\"a\n---\nb\"",
    "|0\n a",
    "| x",
    "|2-\n  a\n   b\n",
    ">+\n a\n\n b\n\n",
    "a:\n  b\n\tc",
    "- |\n  a\n \tb",
    "`a",
    "@a",
    "%a",
    // documents and directives
    "---\na\n...\n---\nb",
    "--- a\n--- b",
    "...\na",
    "%YAML 1.2\n---\na",
    "%YAML 1.2 3\n---\na",
    "%YAML 2.0\n---\na",
    "%YAML 1.2\n%YAML 1.1\n---\na",
    "%YAML x\n---\na",
    "%YAML 1\n---\na",
    "%YAML 1.\n---\na",
    "%YAML 01.2\n---\na",
    "%YAML +1.2\n---\na",
    "%YAML -1.2\n---\na",
    "%YAML 1.-2\n---\na",
    "%YAML 1.+2\n---\na",
    "%YAML 1..2\n---\na",
    "%YAML 1.2.3\n---\na",
    "%YAML 1.2x\n---\na",
    "%YAML 1,2\n---\na",
    "%YAML \u{b}1.2\n---\na",
    "%YAML 1.\u{b}2\n---\na",
    "%YAML 1.2\u{c}\n---\na",
    "%YAML 2147483647.0\n---\na",
    "%YAML 2147483648.0\n---\na",
    "%YAML -2147483649.0\n---\na",
    "%YAML 99999999999999999999.0\n---\na",
    "%YAML 1.99999999999\n---\na",
    "%YAML 0000000000000000000000000000000000000000001.2\n---\na",
    "%TAG ! a\n%TAG ! b\n---\na",
    "%TAG !\n---\na",
    "%TAG !e! tag:e,1:\n---\n!e!x a",
    "%FOO bar\n---\na",
    // the stream: Stream::eof() and NUL characters
    "a\u{4}b: c",
    "\u{4}a: b",
    "a: [b\u{4}]",
    "ab\u{0}cd",
    "a: b\u{0}0c",
    "a: b\u{0}",
    "\"a\u{0}b\"",
    "\u{0}a",
    "|\n a\u{0}nb",
    // encodings
    "\u{feff}a: b",
    "\u{feff}\u{feff}a: b",
];

/// A small deterministic generator (xorshift64*), so the generated documents never change.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Random documents of characters that matter to the scanner.
fn random_characters(rng: &mut Rng, count: usize) -> Vec<String> {
    const ALPHABET: &[u8] = b"-?:,[]{}#&*!|>'\"%@`<> \n\n\t\rab0.~\\";
    (0..count)
        .map(|_| {
            let len = 1 + rng.below(24);
            (0..len)
                .map(|_| char::from(ALPHABET[rng.below(ALPHABET.len())]))
                .collect()
        })
        .collect()
}

/// Random documents of YAML fragments.
fn random_fragments(rng: &mut Rng, count: usize) -> Vec<String> {
    const FRAGMENTS: &[&str] = &[
        "- ",
        "? ",
        ": ",
        ":",
        "a",
        "b: ",
        "c",
        "[",
        "]",
        "{",
        "}",
        ", ",
        ",",
        "\n",
        "\n  ",
        "  ",
        " ",
        "&x ",
        "*x",
        "!t ",
        "!<u> ",
        "!!str ",
        "!e!s ",
        "\"q\"",
        "\"q\\n\"",
        "'s'",
        "'s''t'",
        "|\n",
        "|-\n",
        ">\n",
        ">+2\n",
        "# c\n",
        "---\n",
        "--- ",
        "...\n",
        "\t",
        "%YAML 1.2\n",
        "%TAG !e! tag:e,1:\n",
        "~",
        "null",
        "\"",
        "'",
        "-",
        "?",
        "\\",
    ];
    (0..count)
        .map(|_| {
            let len = 1 + rng.below(12);
            (0..len)
                .map(|_| FRAGMENTS[rng.below(FRAGMENTS.len())])
                .collect()
        })
        .collect()
}

#[test]
fn parser_errors_match_the_wheel() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut docs: Vec<String> = CASES.iter().map(|s| s.to_string()).collect();
    docs.extend(random_characters(&mut rng, 3000));
    docs.extend(random_fragments(&mut rng, 4000));
    // A deep document: "bad file" at depth 500.
    docs.push("[".repeat(600));
    docs.push(format!("{}a{}", "- ".repeat(498), ""));
    docs.push(format!("{}a{}", "{a: ".repeat(300), "}".repeat(300)));

    // The generated documents are fixed: a change to the generator must be deliberate.
    let all: Vec<u8> = docs
        .iter()
        .flat_map(|d| [d.as_bytes(), b"\x01"].concat())
        .collect();
    assert_eq!(
        sha256_hex(&all),
        "21d2ea6b14879fd799d3c28a97431666a7357a282b59244669acfe7a78e170d6",
        "the generated documents changed"
    );

    let calls: Vec<BatchCall<'_>> = docs
        .iter()
        .map(|yaml| BatchCall {
            cmd: "config_serialize",
            args: json!({"config": {"yaml": yaml}}),
            blobs: Vec::new(),
        })
        .collect();
    let responses = Oracle::get().batch(&calls, true);

    let (mut thrown, mut parsed) = (0, 0);
    for (yaml, response) in docs.iter().zip(responses) {
        let response = response.unwrap_or_else(|e| panic!("{e}"));
        let wheel = response
            .result
            .get("exception")
            .map(|e| e["message"].as_str().expect("a message").to_string());
        let label = format!("document {yaml:?}");
        match port_error(yaml.as_bytes()) {
            Some(what) => {
                thrown += 1;
                // OCIO's Exception keeps the C string: up to the first NUL.
                let what = match what.iter().position(|&c| c == 0) {
                    Some(nul) => &what[..nul],
                    None => &what[..],
                };
                let expected = wheel.unwrap_or_else(|| panic!("{label}: the wheel loaded it"));
                let actual = format!("{PREFIX}{}", String::from_utf8_lossy(what));
                assert_text_eq(&label, &expected, &actual);
            }
            None => {
                parsed += 1;
                if let Some(wheel) = wheel
                    && let Some(rest) = wheel.strip_prefix(PREFIX)
                    && rest.starts_with("yaml-cpp: error at line ")
                {
                    assert!(
                        rest.ends_with(
                            ": operator[] call on a scalar (key: \"ocio_profile_version\")"
                        ),
                        "{label}: the port parsed it; the wheel threw {wheel:?}"
                    );
                }
            }
        }
    }
    // Both outcomes are well represented.
    assert!(
        thrown > 1000 && parsed > 1000,
        "thrown {thrown}, parsed {parsed}"
    );
}
