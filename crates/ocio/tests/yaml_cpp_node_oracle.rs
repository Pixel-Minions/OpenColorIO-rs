// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The port of yaml-cpp's nodes against the wheel's, through the first error a config load
//! reports.
//!
//! OCIO starts reading a config with its root node's `ocio_profile_version` (`load(const
//! YAML::Node&, ConfigRcPtr&, const char*)`, src/OpenColorIO/OCIOYaml.cpp:4398-4448 @ v2.5.2):
//! - `node["ocio_profile_version"]` on the root: on a scalar root this throws yaml-cpp's
//!   `BadSubscript`, at the root's mark;
//! - a root without the key (a sequence, a null, a map without it) has no version: the
//!   error names the root's line and tag ("At line 3, '?' parsing failed: ...");
//! - the version reads as a string (`load(const YAML::Node&, std::string&)`, OCIOYaml.cpp:
//!   98-112): a collection throws yaml-cpp's `TypedBadConversion` at its mark, wrapped with
//!   the version node's line and tag, and a null reads as `null`;
//! - a version that `std::stoi` can't read is printed in the error, whole. Every version here
//!   starts with an `x`, so it never reads.
//!
//! `OCIOYaml::Read` (OCIOYaml.cpp:5419-5437) wraps the error: "Error: Loading the OCIO
//! profile failed. ". OCIO's `Exception` keeps a C string, up to the first NUL.
//!
//! So the root's type, mark and tag, the map lookup by key, the version node's mark and tag,
//! and its scalar's text, as the port's node API gives them, make the message the wheel must
//! report. The documents are hand-written scalars of every style, and random documents of
//! YAML fragments.

use ocio::yaml_cpp::exceptions::Result;
use ocio::yaml_cpp::node::{Node, NodeType};
use ocio::yaml_cpp::parse::load;
use ocio_testkit::fixtures::sha256_hex;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::{Oracle, assert_text_eq};
use serde_json::json;

const PREFIX: &[u8] = b"Error: Loading the OCIO profile failed. ";

/// "At line N, '<tag>' " with the node's line counted from 1, as OCIO writes it.
fn at_line(node: &Node) -> Result<Vec<u8>> {
    let mut out = format!("At line {}, '", node.mark()?.line.wrapping_add(1)).into_bytes();
    out.extend_from_slice(node.tag()?);
    out.extend_from_slice(b"' ");
    Ok(out)
}

/// The message the start of OCIO's config loading gives for a document the port loads.
fn ocio_message(root: &Node) -> Result<Vec<u8>> {
    let version_node = match root.get("ocio_profile_version") {
        Ok(node) => node,
        // a yaml-cpp exception, wrapped by OCIOYaml::Read only
        Err(e) => return Ok([PREFIX, &e.what()].concat()),
    };
    let mut version = Vec::new();
    if version_node.is_defined() {
        match version_node.as_::<Vec<u8>>() {
            Ok(v) => version = v,
            Err(e) => {
                let mut out = PREFIX.to_vec();
                out.extend(at_line(&version_node)?);
                out.extend_from_slice(b"parsing string failed with: ");
                out.extend(e.what());
                return Ok(out);
            }
        }
    }
    let mut out = PREFIX.to_vec();
    out.extend(at_line(root)?);
    out.extend_from_slice(
        b"parsing failed: The specified OCIO configuration file <null> does not appear to have a \
          valid version ",
    );
    out.extend_from_slice(if version.is_empty() {
        b"<null>"
    } else {
        &version
    });
    out.push(b'.');
    Ok(out)
}

/// Hand-written documents: each scalar style, folding and chomping, escapes, tags, anchors and
/// aliases, roots of each type, and lines and columns of every kind.
const CASES: &[&str] = &[
    // the root
    "",
    "~",
    "null",
    "x",
    "  x",
    "\n\n  x y",
    "- a\n- b",
    "[a, b]",
    "{a: b}",
    "a: b",
    "!foo {a: b}",
    "--- !bar\na: b",
    "!!map {a: b}",
    "&a {b: c}",
    "\n\n# comment\n\na: b\n",
    "!<tag:x> [a]",
    "!t ~",
    "!t",
    "\"x\"",
    "'x'",
    "|\n x",
    // the version: plain scalars
    "ocio_profile_version: x",
    "ocio_profile_version: x.1",
    "ocio_profile_version: x  y  z  ",
    "ocio_profile_version: x\n  continued\n\n  after a blank\n",
    "ocio_profile_version: x # comment",
    "ocio_profile_version: x#not a comment",
    "ocio_profile_version: x:y",
    "ocio_profile_version: x: y",
    "ocio_profile_version:\n  x\n",
    "ocio_profile_version: ~",
    "ocio_profile_version:",
    "ocio_profile_version: null",
    "ocio_profile_version: !!str null",
    "ocio_profile_version: !foo x",
    "ocio_profile_version: !<v> x",
    "%TAG !e! tag:e,1:\n---\nocio_profile_version: !e!t x",
    "ocio_profile_version: &a x",
    "a: &a x\nocio_profile_version: *a",
    "ocio_profile_version: x\nocio_profile_version: xb",
    "ocio_profile_version: x\t",
    "ocio_profile_version: x\r\n",
    "ocio_profile_version: x\ry",
    // quoted scalars
    "ocio_profile_version: 'x''s'",
    "ocio_profile_version: 'x\n  folded\n\n  lines '",
    "ocio_profile_version: \"x\\ty\\\\z\\\"q\\/\\a\\b\\e\\f\\v\\r\"",
    "ocio_profile_version: \"x\\x41\\u00e9\\U0001F600\"",
    "ocio_profile_version: \"x\\\n  y\"",
    "ocio_profile_version: \"x \\\n  y\"",
    "ocio_profile_version: \"x\n\n   y  \"",
    "ocio_profile_version: \"x\\0y\"",
    "ocio_profile_version: \"x  \"",
    "ocio_profile_version: \"\"",
    "ocio_profile_version: ''",
    // block scalars
    "ocio_profile_version: |\n  x\n  y\n",
    "ocio_profile_version: |-\n  x\n  y\n\n",
    "ocio_profile_version: |+\n  x\n  y\n\n",
    "ocio_profile_version: >\n  x\n  y\n\n  z\n",
    "ocio_profile_version: >-\n  x\n   more\n  y\n",
    "ocio_profile_version: >+\n  x\n\n",
    "ocio_profile_version: |2\n   x\n  y\n",
    "ocio_profile_version: >\n\n  x\n",
    "ocio_profile_version: | # comment\n  x\n",
    "ocio_profile_version: |\n  x\n\ttab\n",
    // collections as the version
    "ocio_profile_version: [x]",
    "ocio_profile_version: {x: y}",
    "ocio_profile_version:\n  - x\n",
    "\n  ocio_profile_version:\n    x: y\n",
    "[{ocio_profile_version: x}]",
    "{a: b, ocio_profile_version: x}",
    "? ocio_profile_version\n: x\n",
    "? [ocio_profile_version]\n: x\n",
    "\"ocio_profile_version\": x",
    "'ocio_profile_version' : x",
    // the stream
    "ocio_profile_version: x\u{0}0y",
    "ocio_profile_version: x\u{0}ny",
    "ocio_profile_version:\u{4}x",
    "\u{feff}ocio_profile_version: x",
    "ocio_profile_version: x\u{e9}",
    // Aliases are the anchored node, and an anchor names its collection from the collection's
    // start: a collection can hold itself, as an element, a value or a key.
    "a: &a [*a]\nocio_profile_version: x",
    "a: &a\n  - b\n  - *a\nocio_profile_version: x",
    "a: &a {k: *a}\nocio_profile_version: x",
    "a: &a {*a : 1}\nocio_profile_version: x",
    "&r {k: *r, ocio_profile_version: x}",
    "&r {*r : 1, ocio_profile_version: x}",
    "ocio_profile_version: &v [*v]",
    "ocio_profile_version: &v {*v : *v}",
    // Nested aliases, 9 to the power of 12 nodes if they were copied.
    "l: [&a0 [x, x, x, x, x, x, x, x, x], &a1 [*a0, *a0, *a0, *a0, *a0, *a0, *a0, *a0, *a0], \
     &a2 [*a1, *a1, *a1, *a1, *a1, *a1, *a1, *a1, *a1], \
     &a3 [*a2, *a2, *a2, *a2, *a2, *a2, *a2, *a2, *a2], \
     &a4 [*a3, *a3, *a3, *a3, *a3, *a3, *a3, *a3, *a3], \
     &a5 [*a4, *a4, *a4, *a4, *a4, *a4, *a4, *a4, *a4], \
     &a6 [*a5, *a5, *a5, *a5, *a5, *a5, *a5, *a5, *a5], \
     &a7 [*a6, *a6, *a6, *a6, *a6, *a6, *a6, *a6, *a6], \
     &a8 [*a7, *a7, *a7, *a7, *a7, *a7, *a7, *a7, *a7], \
     &a9 [*a8, *a8, *a8, *a8, *a8, *a8, *a8, *a8, *a8], \
     &a10 [*a9, *a9, *a9, *a9, *a9, *a9, *a9, *a9, *a9], \
     &a11 [*a10, *a10, *a10, *a10, *a10, *a10, *a10, *a10, *a10]]\n\
     ocio_profile_version: *a11",
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

/// Random documents of YAML fragments, many of them with a version.
fn random_fragments(rng: &mut Rng, count: usize) -> Vec<String> {
    const FRAGMENTS: &[&str] = &[
        "ocio_profile_version: ",
        "ocio_profile_version: ",
        "ocio_profile_version:",
        "x",
        "xy z",
        "- ",
        "? ",
        ": ",
        "a: ",
        "[",
        "]",
        "{",
        "}",
        ", ",
        "\n",
        "\n  ",
        "  ",
        "&x ",
        "*x",
        "!t ",
        "!<u> ",
        "!!str ",
        "\"xq\\n\"",
        "'x''s'",
        "|\n  x\n",
        ">-\n  x\n\n  y\n",
        "# c\n",
        "---\n",
        "~",
        "\t",
        "\"",
    ];
    (0..count)
        .map(|_| {
            let len = 1 + rng.below(10);
            (0..len)
                .map(|_| FRAGMENTS[rng.below(FRAGMENTS.len())])
                .collect()
        })
        .collect()
}

/// Random documents with a version of every scalar style: plain, single and double quoted,
/// literal and folded, after random lines and before random ones.
fn random_versions(rng: &mut Rng, count: usize) -> Vec<String> {
    fn pick<'a>(rng: &mut Rng, items: &[&'a str]) -> &'a str {
        items[rng.below(items.len())]
    }
    fn repeat(rng: &mut Rng, max: usize, items: &[&str]) -> String {
        let len = rng.below(max + 1);
        (0..len).map(|_| pick(rng, items)).collect()
    }
    const PLAIN: &[&str] = &[
        "a", "b", " ", "1", ".", ":", "#", "-", "'", "\"", "\\", "\t", "\n  ", "\n\n  ",
    ];
    const SINGLE: &[&str] = &["a", " ", "''", "\n  ", "\n\n  ", "\\", "\"", "#", ": "];
    const DOUBLE: &[&str] = &[
        "a", " ", "\\n", "\\t", "\\\\", "\\\"", "\\x41", "\\u00e9", "\n  ", "\\\n  ", "'", "#",
        "  \n  ",
    ];
    const BLOCK_LINES: &[&str] = &["\n  a", "\n  b c", "\n", "\n   more", "\n    more", "\n  "];
    (0..count)
        .map(|_| {
            let prefix = repeat(rng, 3, &["# c\n", "a: b\n", "\n", "b:\n  - c\n", "  \n"]);
            let separator = pick(rng, &[" ", "  ", "\n  ", " # c\n  ", "\t"]);
            let value = match rng.below(5) {
                0 => format!("x{}", repeat(rng, 8, PLAIN)),
                1 => format!("'x{}'", repeat(rng, 8, SINGLE)),
                2 => format!("\"x{}\"", repeat(rng, 8, DOUBLE)),
                _ => {
                    let header = format!(
                        "{}{}{}",
                        pick(rng, &["|", ">"]),
                        pick(rng, &["", "-", "+", "2", "-2", "+1"]),
                        pick(rng, &["", " # c"])
                    );
                    format!("{header}\n  x{}", repeat(rng, 4, BLOCK_LINES))
                }
            };
            let suffix = pick(rng, &["", "\n", "\nz: 1", "\n\n", "\n# end", "\n  "]);
            format!("{prefix}ocio_profile_version:{separator}{value}{suffix}")
        })
        .collect()
}

#[test]
fn config_load_errors_match_the_wheel() {
    let mut rng = Rng(0x0DDB_A11C_0FFE_E123);
    let mut docs: Vec<String> = CASES.iter().map(|s| s.to_string()).collect();
    docs.extend(random_fragments(&mut rng, 4000));
    docs.extend(random_versions(&mut rng, 4000));

    // The generated documents are fixed: a change to the generator must be deliberate.
    let all: Vec<u8> = docs
        .iter()
        .flat_map(|d| [d.as_bytes(), b"\x01"].concat())
        .collect();
    assert_eq!(
        sha256_hex(&all),
        "12e94a2261a1813cd9f06c1c6816c5e49ad255d7c8ae6c6cfc10d0127aa423e6",
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

    let (mut loaded, mut versions, mut roots) = (0, 0, [0usize; 5]);
    for (yaml, response) in docs.iter().zip(responses) {
        let response = response.unwrap_or_else(|e| panic!("{e}"));
        let label = format!("document {yaml:?}");
        // Documents the parser refuses are the parser test's.
        let Ok(root) = load(yaml.as_bytes()) else {
            continue;
        };
        loaded += 1;
        roots[root.node_type().unwrap() as usize] += 1;
        if root
            .get("ocio_profile_version")
            .is_ok_and(|v| v.is_defined())
        {
            versions += 1;
        }
        let mut expected = ocio_message(&root).unwrap_or_else(|e| panic!("{label}: {e}"));
        if let Some(nul) = expected.iter().position(|&c| c == 0) {
            expected.truncate(nul);
        }
        let wheel = response
            .result
            .get("exception")
            .map(|e| e["message"].as_str().expect("a message").to_string())
            .unwrap_or_else(|| panic!("{label}: the wheel loaded it"));
        assert_text_eq(&label, &wheel, &String::from_utf8_lossy(&expected));
    }
    // Every root type and many versions are checked.
    assert!(
        loaded > 4000 && versions > 3000,
        "loaded {loaded}, versions {versions}"
    );
    assert!(
        roots[NodeType::Null as usize] > 0
            && roots[NodeType::Scalar as usize] > 0
            && roots[NodeType::Sequence as usize] > 0
            && roots[NodeType::Map as usize] > 0,
        "{roots:?}"
    );
}
