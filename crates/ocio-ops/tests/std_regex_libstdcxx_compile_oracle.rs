// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Compiling `std::regex` as the Linux wheel does ([`Library::Libstdcxx`]), against the wheel:
//! a file rule's regular expression is compiled when a config loads (FileRules.cpp:263-282
//! `ValidateRegularExpression` @ v2.5.2), so a config with the expression as a rule, loaded
//! through the oracle's `config_serialize`, either loads or fails with
//! "File rules: invalid regular expression '<expression>': '<what()>'.". The port compiles the
//! same expression: it must succeed exactly when the wheel loads, and otherwise fail with the
//! same `what()`.
//!
//! The expressions: hand cases for each rule of the parser, every expression of up to two
//! characters from its alphabet, a sample of longer ones, and the NFA state limit
//! (`_GLIBCXX_REGEX_STATE_LIMIT`): for constructs of each kind and for generated expressions,
//! the longest `a{N}` after them the port accepts and the next, alone and before expressions
//! that are refused for another reason (which error the wheel reports first). The generated
//! expressions are pinned by a digest. Then the port's own limits (U-54).
//!
//! Only on Linux; YAML text can't carry bytes past 0x7F, which the match test covers.
#![cfg(target_os = "linux")]

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

/// Hand cases, separated by white space: quantifiers on quantifiers and on assertions, braces,
/// bracket expressions (ranges, classes, equivalence classes, collating elements, escapes),
/// escapes, back references and groups.
const HAND: &str = r#"
a** a*+ a+* a?* a*? a*?? a??+ a{2}* a*{2} a{2}? a{2}?? a{2}{3} a*?* a+?+
^* $* \b* \B+ (?=a)* (?!a)+ (?:a)* ()* (a)* (?=a){2} \b{2} ^{0} $?
* +a ? {1} a|* (*) (|*) (?:*) (?=*)
{ a{ a{1 a{1, a{1,2 a{,} a{,2} a{1,2,3} a{a} a{1a} a{_1} a{2,1} a{0} a{0,0} a{99999999999}
a{2147483647} a{2147483648} a{4294967296} a{1,99999999999} {1}a a} } a{1}} a{1,}
[ [a [] []a] [^] [^ [a-] [-a] [a-z [a- [b-a] [a-a] [\d-z] [a-\d] [\d-] [-\d] [[:alpha:]]
[[:alpha:] [[:alpha]] [[:foo:]] [[: [[ [[a [[a] [[=a=]] [[=ab=]] [[==]] [[.a.]] [[.ab.]]
[[..]] [[...]] [[===]] [[.space.]] [[.hyphen.]] [\]] [\b] [\0] [\1] [\x41] [\c] [\cA]
[\u0041] [\ [a\ [a-b-c] [a-[.b.]] [[.a.]-b] [[:alpha:]-z] [z-[:alpha:]] [\w-\d] [--a] [!--]
[a\-z] [[:ALPHA:]] [[:w:]] [[:W:]] [[:d:]] [[:s:]] [[:blank:]] [[=a]] [[.a]] [[:a]] [[=]]
[[.]] [a[] [a[b] [[a-z]] [^-] [\\] [\-] [\B] [\D-z] [\k] [[.].]] [[=].]] [[:].]]
\ a\ \0 \00 \01 \08 \1 (a)\1 (a)\2 (a\1) (a)\10 \c \cA \c1 \c( \c\\ \x \x1 \x41 \xG1 \u
\u004 \u0041 \u00ff \u0100 \uFFFF \k \e \a \f\n\r\t\v \/ \- \b \B \d\D\s\S\w\W \p{L}
\99999999999 \0a (a)(b)\2\1 ((a)\2) (?:a)\1 (a)|\1 \10
( ) (a a) () (? (?: (?:a (?= (?! (?<a>b) (?<=a) (?x) (|) | || a| |a (?:|a) ((a) (a))
(?:a)) a(?:b
"#;

/// The names a collating element or an equivalence class may try: the POSIX names, their
/// usual aliases, and others.
const NAMES: &str = r#"
NUL SOH STX ETX EOT ENQ ACK BEL alert BS backspace HT tab LF newline VT vertical-tab FF
form-feed CR carriage-return SO SI DLE DC1 DC2 DC3 DC4 NAK SYN ETB CAN EM SUB ESC IS4 FS IS3
GS IS2 RS IS1 US space exclamation-mark quotation-mark number-sign dollar-sign percent-sign
ampersand apostrophe left-parenthesis right-parenthesis asterisk plus-sign comma hyphen
hyphen-minus period full-stop slash solidus zero one two three four five six seven eight nine
colon semicolon less-than-sign equals-sign greater-than-sign question-mark commercial-at
left-square-bracket backslash reverse-solidus right-square-bracket circumflex
circumflex-accent underscore low-line grave-accent left-brace left-curly-bracket
vertical-line right-brace right-curly-bracket tilde DEL A a Z z SPACE Space nul del 0 ! - ~
"#;

/// The parser's alphabet.
const ALPHABET: &[u8] = b"a1()[]{}*+?|\\^$.,-:=!bBcdDsSwWxu0";

/// A small deterministic generator (an LCG), so the samples are the same on every run.
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

/// Every expression of up to two characters of the alphabet, and 3,000 longer ones.
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

/// Constructs of each kind, whose NFA states the state-limit cases measure.
const CONSTRUCTS: &str = r#"
b bb bbb b| b|b| (b) (bb) (?:b) (?:bb) ((b)) (?:(b)) () (?:) (?:b|c) (?:b|c|d) (?:b|) (?:|b)
(?:|) (b|c) b* b+ b? b*? b+? b?? b{2} b{3} b{0} b{1} b{0,1} b{0,2} b{0,3} b{1,2} b{1,3}
b{2,3} b{2,} b{3,} b{1,} b{0,} b** b*+ b{2}{2} b{2}{3} (?:bb){2} (?:bb){3} (b){2} (b){3}
(?:b|c){2} (?:b|c){3} (?:b|c)* (?:b|c)? (b)* (b)? (bb)* [b] [bc] . \d \b \B ^ $ (?=b)
(?!b) (?=bb) (?=b|c) (b)\1 (b)\1\1 (?:b*)* (?:b*)+ (?:b*){2} (b*)* b*b* b*c*d*
(?:b{2}){2} (?:b?){2} (?:b{0,2}){2} (?:b*){0,2} b{0,2}? (?:b|c|d|e) (?:(b)|c) (?:b(c)d)
((b)(c)) (){2,} (b){2,} (?:b){2,} (b|c){2,} (){0,} (b){1,} (){2} ()+ (b|c)+ (?:b*){2,}
(?:b{2}){2,} (?:bc){2,} (?:b?){2,} (?:b|){2,} (?:^){2,}
"#;

/// Random expressions over the constructs, nested.
fn random_expressions() -> Vec<String> {
    fn atom(rng: &mut Lcg, depth: u32) -> String {
        if depth > 2 || rng.next().is_multiple_of(3) {
            return rng
                .pick(&["b", "c", ".", "[bc]", "\\d", "\\b", "^", "$", "(?:)", "()"])
                .to_string();
        }
        let inner = expression(rng, depth + 1);
        let open = rng.pick(&["(", "(?:", "(?=", "(?!"]);
        format!("{open}{inner})")
    }
    fn term(rng: &mut Lcg, depth: u32) -> String {
        let a = atom(rng, depth);
        if a == "^" || a == "$" || a == "\\b" || a.starts_with("(?=") || a.starts_with("(?!") {
            return a;
        }
        let quantifiers = [
            "", "", "*", "+", "?", "*?", "{2}", "{0}", "{1,3}", "{2,}", "{0,2}", "{3}?", "**",
            "{2}{2}", "?+", "{1,2}*",
        ];
        a + rng.pick(&quantifiers)
    }
    fn expression(rng: &mut Lcg, depth: u32) -> String {
        let alternatives = [1, 1, 1, 2, 3][rng.next() % 5];
        (0..alternatives)
            .map(|_| {
                (0..rng.next() % 4)
                    .map(|_| term(rng, depth))
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("|")
    }
    let mut rng = Lcg(99);
    (0..300)
        .map(|_| format!("(?:{})", expression(&mut rng, 0)))
        .collect()
}

/// The largest `n` such that the port compiles `prefix a{n} suffix`, by bisection.
fn port_boundary(prefix: &str, suffix: &str) -> u64 {
    let compiles = |n: u64| {
        Regex::new(
            format!("{prefix}a{{{n}}}{suffix}").as_bytes(),
            Library::Libstdcxx,
        )
        .is_ok()
    };
    let (mut lo, mut hi) = (0u64, 100_001u64);
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        if compiles(mid) { lo = mid } else { hi = mid }
    }
    lo
}

/// The state-limit cases: `e a{n}` at the port's boundary and past it, for each construct and
/// random expression; and around the boundary before tails and after heads that are refused
/// for other reasons.
fn state_limit_patterns() -> Vec<String> {
    let mut out = Vec::new();
    let mut expressions: Vec<String> = CONSTRUCTS.split_whitespace().map(str::to_string).collect();
    expressions.push(String::new());
    expressions.extend(random_expressions());
    for e in &expressions {
        let n = port_boundary(e, "");
        out.push(format!("{e}a{{{n}}}"));
        out.push(format!("{e}a{{{}}}", n + 1));
    }
    let tails = [
        "(", "\\", "[", "[a", ")", "{", "*{", "|(", "(?:", "(?=", "(?=a", "\\1", "b(", "b*[",
        "b?(", "|", "{2,1}", "\\x", "(?<", "{2", "{2,", "{2,3", "{2x", "{2,x", "{x", "{,", "{0}(",
        "((", "(a)(", "+(", "?(", "?{",
    ];
    let heads = ["", "(", "(?:", "b|", "(?=", "(a)"];
    let mut rng = Lcg(5);
    for e in expressions.iter().step_by(3) {
        let n = port_boundary(e, "");
        for _ in 0..6 {
            let k = (rng.next() % 10) as u64;
            let head = rng.pick(&heads);
            let tail = rng.pick(&tails);
            out.push(format!("{head}{e}a{{{}}}{tail}", (n + 4).saturating_sub(k)));
        }
    }
    out
}

fn patterns() -> Vec<String> {
    let mut out: Vec<String> = HAND.split_whitespace().map(str::to_string).collect();
    for name in NAMES.split_whitespace() {
        out.push(format!("[[.{name}.]]"));
        out.push(format!("[[={name}=]]"));
        out.push(format!("[[:{name}:]]"));
    }
    out.extend(generated());
    out.extend(state_limit_patterns());
    out
}

/// The generated expressions can't change unnoticed.
#[test]
fn the_generated_expressions_are_pinned() {
    let mut all = generated();
    all.extend(random_expressions());
    let joined = all.join("\0");
    assert_eq!(
        sha256_hex(joined.as_bytes()),
        "ce0ce17113b06806f1df5664cec583d8c09f96ec769f7df47f759389205e8d65"
    );
}

/// Each expression compiles in the port exactly when the wheel loads it, and otherwise fails
/// with the wheel's `what()`.
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
        let port = Regex::new(pattern.as_bytes(), Library::Libstdcxx);
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
        failures
            .iter()
            .take(60)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Groups nested 5,000 deep and alternatives of 50,000 terms compile as in the wheel; deeper or
/// longer, which the wheel's recursion takes close to overflowing its stack (about 14,000
/// levels and 75,000 terms, docs/improvements.md U-54), the port refuses.
#[test]
fn the_ports_limits() {
    let nested = |open: &str, depth: usize| format!("{}a{}", open.repeat(depth), ")".repeat(depth));
    let patterns = [
        nested("(?:", 5000),
        nested("(", 5000),
        nested("(?=", 5000),
        "a".repeat(50_000),
    ];
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
        assert!(Regex::new(pattern.as_bytes(), Library::Libstdcxx).is_ok());
    }

    for open in ["(?:", "(", "(?="] {
        let error = Regex::new(nested(open, 5001).as_bytes(), Library::Libstdcxx).unwrap_err();
        assert_eq!(
            (error.code(), error.what()),
            (ErrorType::Stack, "regex_error"),
            "{open}"
        );
    }
    let error = Regex::new("a".repeat(50_001).as_bytes(), Library::Libstdcxx).unwrap_err();
    assert_eq!(error.code(), ErrorType::Space);
}
