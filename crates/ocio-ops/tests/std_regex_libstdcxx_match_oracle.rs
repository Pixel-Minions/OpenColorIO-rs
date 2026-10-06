// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Matching with `std::regex` as the Linux wheel does ([`Library::Libstdcxx`]), against the
//! wheel: a file rule's regular expression matches a path with `regex_match` (FileRules.cpp:
//! 488-494 @ v2.5.2), so the oracle's `file_rules_match` (O3.4), given a regex rule before the
//! default rule, reports rule 0 for each path the expression matches whole and rule 1 (the
//! default) for the others. The port matches the same path with the same expression.
//!
//! The expressions: hand cases for each departure of libstdc++ from ECMAScript (back
//! references to groups that matched nothing, captures kept across iterations, empty
//! iterations, `\cX`, `\uNNNN`, signed ranges, `^` and `$`), generated expressions over the
//! parser's alphabet, bracket expressions against every byte, and expressions of groups,
//! alternations, quantifiers and back references (all pinned by a digest); then the port's own
//! limits (U-54).
//!
//! Only on Linux: the Windows wheel matches with MSVC's STL.
#![cfg(target_os = "linux")]

use ocio_ops::std_regex::{ErrorType, Library, Regex, regex_match};
use ocio_testkit::Oracle;
use ocio_testkit::fixtures::sha256_hex;
use serde_json::{Value, json};

fn hex(b: &[u8]) -> String {
    b.iter().map(|c| format!("{c:02x}")).collect()
}

/// Hand cases, separated by white space.
const HAND: &str = r#"
a ab a|b a|ab ab|a (a|ab)(c|bcd) a* a+ a? a*? a+? a?? a{2} a{2,} a{1,3} a{1,3}? a{0} (a)*
(a|b)* (a|b)+?b (a*)* (a*)+ (a?)* (a|)*b (?:a|b)*a ((a)|b)* (a*b*)* (ab|a)*b (a)\1 (a*)\1
(a|b)\1 (a*)b\1 ((a)b)\2 (a)|\1b (?=a)a (?=a)b (?!a). (?!a)b* a(?=b) (?=(a))\1a (?!(a))\1
\ba a\b \Ba a\B \b \B ^a a$ ^$ ^ $ . .* .+ a.b [ab] [^ab] (a)+\1 (a{2})* (.*)\1 (.+)\1 (.)\1*
a** a*+ a+* a?* a*?? a{2}{2} (a|b){2}{2} (a*)*\1 (a|)+\1 (b*)(a\1)* (a)*\1 (?:(a)|b)*\1
((a)|b)+\2 (a*)+b\1 (a)?\1b (a)??\1 (a|())*b ()* (()|a)* (a|(b))*\2 (?:a*)*b (?:a?)*?b
(?:a|b)*?\b (a*?)*\1 \ca \cb \c( \x61 \u0061 \u0161 [\x80-\x7f] [\x80-\xff] [a-b-c] [--a]
[!--] [\d-] [-\d] []a] [^] ] } [[.a.]] [[.hyphen.]] [[=a=]] [[:w:]] \0a a{0,2}b (?:a|b){1,3}?
"#;

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
        b"\x01",
        b"\x7f",
        b"\x80",
        b"\xe9",
        b"\xff",
        b"a\xffb",
        b"=",
        b"-",
        b"]",
        b"}",
        b"(",
    ] {
        out.push(extra.to_vec());
    }
    out
}

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) as usize
    }

    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.next() % items.len()]
    }
}

/// Expressions over the parser's alphabet that the port compiles.
fn generated() -> Vec<String> {
    const ALPHABET: &[u8] = b"ab()[]|*+?{}12,.^$\\-:=!dwsSbB";
    let mut rng = Lcg(77);
    let mut out = Vec::new();
    while out.len() < 1500 {
        let len = 1 + rng.next() % 8;
        let s: Vec<u8> = (0..len)
            .map(|_| ALPHABET[rng.next() % ALPHABET.len()])
            .collect();
        let s = String::from_utf8(s).unwrap();
        if Regex::new(s.as_bytes(), Library::Libstdcxx).is_ok() {
            out.push(s);
        }
    }
    out
}

/// Bracket expressions over pieces of every kind.
fn brackets() -> Vec<String> {
    let pieces = [
        "a",
        "b",
        "z",
        "-",
        "]",
        "^",
        "[",
        ":",
        ".",
        "=",
        "\\d",
        "\\w",
        "\\s",
        "\\D",
        "\\b",
        "\\-",
        "\\]",
        "\\x41",
        "\\cA",
        "\\u0062",
        "[:alpha:]",
        "[:digit:]",
        "[.a.]",
        "[.hyphen.]",
        "[=a=]",
        "[.space.]",
        "!",
        "~",
        "0",
        "9",
        "\\0",
        "\\n",
        "\\t",
        "\\x7f",
        "\\x80",
        "\\xff",
    ];
    let mut rng = Lcg(7);
    let mut out = Vec::new();
    while out.len() < 600 {
        let n = rng.next() % 6;
        let body: String = (0..n).map(|_| rng.pick(&pieces)).collect();
        let negated = if rng.next().is_multiple_of(3) {
            "^"
        } else {
            ""
        };
        let tail = rng.pick(&["]", "]", "]", "]a", "]*", "]]"]);
        let s = format!("[{negated}{body}{tail}");
        if Regex::new(s.as_bytes(), Library::Libstdcxx).is_ok() {
            out.push(s);
        }
    }
    out
}

/// Expressions of groups, alternations, quantifiers and back references. A quantified group
/// holds no quantified group itself, which keeps the backtracking (exponential in both the
/// wheel and the port) small.
fn groups() -> Vec<String> {
    let atoms = [
        "a",
        "b",
        ".",
        "(a)",
        "(b)",
        "(a|b)",
        "(a|)",
        "()",
        "(?:a|b)",
        "(?:ab)",
        "(?=a)",
        "(?!b)",
        "(?=(a))",
        "[ab]",
        "\\1",
        "\\2",
        "\\3",
        "((a)|b)",
        "(a|(b))",
        "(?:(a)|(b))",
        "\\b",
        "\\B",
        "^",
        "$",
    ];
    let quantifiers = [
        "", "", "", "*", "+", "?", "*?", "+?", "??", "{2}", "{0,2}", "{1,3}", "{2,}", "{0}", "**",
        "*+", "{1}{2}", "?*", "{0,1}?", "{2,3}?",
    ];
    let mut rng = Lcg(23);
    let mut out = Vec::new();
    while out.len() < 1500 {
        let n = 1 + rng.next() % 4;
        let mut s = String::new();
        for _ in 0..n {
            s += rng.pick(&atoms);
            s += rng.pick(&quantifiers);
            if rng.next().is_multiple_of(7) {
                s += "|";
            }
        }
        if Regex::new(s.as_bytes(), Library::Libstdcxx).is_ok() {
            out.push(s);
        }
    }
    out
}

/// The generated expressions can't change unnoticed.
#[test]
fn the_generated_expressions_are_pinned() {
    let mut all = generated();
    all.extend(brackets());
    all.extend(groups());
    assert_eq!(
        sha256_hex(all.join("\n").as_bytes()),
        "5748331d7665997b0ce1a7a19c445d55a297de3b7d8f61d2ce4f86ea4ca6cc56"
    );
}

/// The wheel's outcome of each pattern (a regex rule) on each path: matched or not.
fn wheel(cases: &[(String, Vec<Vec<u8>>)]) -> Vec<Vec<Option<bool>>> {
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
    for chunk in requests.chunks(100) {
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
                    .map(|p| p.get("rule").map(|rule| *rule == 0))
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
        let re = Regex::new(pattern.as_bytes(), Library::Libstdcxx).expect("compiles");
        for (path, wheel) in paths.iter().zip(wheel) {
            let port = regex_match(path, &re).ok();
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
        HAND.split_whitespace()
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

#[test]
fn bracket_expressions_match_as_in_the_wheel() {
    let mut paths: Vec<Vec<u8>> = (1..=255u8).map(|c| vec![c]).collect();
    paths.extend([
        b"".to_vec(),
        b"aa".to_vec(),
        b"a]".to_vec(),
        b"]a".to_vec(),
        b"-a".to_vec(),
    ]);
    check(brackets().into_iter().map(|p| (p, paths.clone())).collect());
}

#[test]
fn groups_and_back_references_match_as_in_the_wheel() {
    let mut paths = vec![Vec::new()];
    let mut frontier = vec![Vec::new()];
    for _ in 0..5 {
        let mut next = Vec::new();
        for s in &frontier {
            for &c in b"ab" {
                let mut t: Vec<u8> = s.clone();
                t.push(c);
                next.push(t);
            }
        }
        paths.extend(next.iter().cloned());
        frontier = next;
    }
    check(groups().into_iter().map(|p| (p, paths.clone())).collect());
}

/// A lookahead is an expression of its own that starts where it stands: there `^` matches,
/// and `\b`/`\B` see no character before it (the p3-regex verifier's cases).
#[test]
fn lookaheads_start_their_own_expression() {
    let patterns = [
        r"a(?=^b)b",
        r"a(?=\bb)b",
        r"ab(?=^)",
        r"a(?=(?:^|x)b)b",
        r"a(?=\Bb)b",
        r"a(?!^b)b",
        r"a(?=\B)b",
        r".(?=\b)",
        r"(?=^a)a",
        r"a(?=(?=^b))b",
        r"a(?=b(?=^))b",
        r"a(?=b\b)b",
        r"_(?=\b)",
        r"a(?=$)",
        r"a(?!$)b",
        r"(?:a(?=^))*",
        r"a(?=.^)",
    ];
    let paths: Vec<Vec<u8>> = ["ab", "a", "b", "", "a_", "ab_", "aab", "_"]
        .iter()
        .map(|p| p.as_bytes().to_vec())
        .collect();
    check(
        patterns
            .iter()
            .map(|p| (p.to_string(), paths.clone()))
            .collect(),
    );
}

/// A brace copies its body, and each copy's loops count their entries at a position apart;
/// `*`, `+` and `?` don't copy (the p3-regex verifier's cases).
#[test]
fn brace_copies_count_their_loops_apart() {
    let bodies = [
        r"(?:(?:()|()|())?){3}",
        r"(?:(?:()|()|())*){2}",
        r"(?:(?:()|()|())*){2,}",
        r"(?:(?:()|()|())*){1,2}",
        r"(?:(?:()|()|())*){1,2}?",
        r"(?:(?:()|()|())*){3,}",
        r"(?:(?:()|()|())*?){2}",
        r"(?:(?:()|()|())??){3}",
        r"(?:(?:()|()|()){0,1}){3}",
        r"(?:(?:()|()|())+){2}",
        r"(?:(?:()|()|())*)+",
        r"(?:(?:()|()|())*)*",
        r"(?:()|())*",
        r"(?:()|()|())*",
        r"(?:()|()|()){0,3}",
    ];
    let tails = [r"\1\2\3", r"\1\2", r"\1", r"\3", ""];
    let paths = vec![b"".to_vec(), b"a".to_vec()];
    let mut cases = Vec::new();
    for body in bodies {
        for tail in tails {
            let pattern = format!("{body}{tail}");
            if Regex::new(pattern.as_bytes(), Library::Libstdcxx).is_ok() {
                cases.push((pattern, paths.clone()));
            }
        }
    }
    check(cases);
}

/// Inputs the p3-regex verifier's mutants needed: classes, escapes and anchors.
#[test]
fn classes_escapes_and_anchors_match_as_in_the_wheel() {
    let cases: [(&str, &[&[u8]]); 6] = [
        ("[[:blank:]]", &[b"\t", b" ", b"a", b"\x0b"]),
        ("[[:print:]]", &[b" ", b"\t", b"~", b"\x7f"]),
        (r"\v", &[b"\x0b", b"v"]),
        (r"\0?a", &[b"0a", b"a"]),
        ("a\n^b", &[b"a\nb"]),
        (r"_\b", &[b"_"]),
    ];
    check(
        cases
            .iter()
            .map(|(p, paths)| (p.to_string(), paths.iter().map(|x| x.to_vec()).collect()))
            .collect(),
    );
}

/// Paths of 8,192 bytes match as in the wheel; longer ones, and matches whose recursion would
/// go past 100,000 levels, the port refuses (U-54).
#[test]
fn the_ports_limits() {
    let long = vec![b'a'; 8192];
    let cases = vec![
        ("(a|b)*".to_string(), vec![long.clone()]),
        (".*".to_string(), vec![long.clone()]),
        ("a*?".to_string(), vec![long.clone()]),
    ];
    check(cases);

    let re = Regex::new(b".*", Library::Libstdcxx).unwrap();
    let error = regex_match(&vec![b'a'; 8193], &re).unwrap_err();
    assert_eq!(
        (error.code(), error.what()),
        (ErrorType::Stack, "regex_error")
    );

    let deep = format!("{}a{}*", "(".repeat(200), ")".repeat(200));
    let re = Regex::new(deep.as_bytes(), Library::Libstdcxx).unwrap();
    let error = regex_match(&long, &re).unwrap_err();
    assert_eq!(
        (error.code(), error.what()),
        (ErrorType::Stack, "regex_error")
    );
}
