// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The port of yaml-cpp's parser parses the deepest documents yaml-cpp allows on a thread of
//! 1 MiB (a program's main thread on Windows), and refuses one level deeper with yaml-cpp's
//! "bad file", as the wheel does.
//!
//! `SingleDocParser` recurses once per nested node up to its `DepthGuard<500>`: 498 nodes
//! nested in the document's root parse, 499 don't. The port parses each document on a thread
//! of its own, sized from the stack the deepest documents take at opt-level 0 (where frames
//! are largest); this test passes there too. Each shape of nesting takes its own path through
//! the parser: flow and block sequences, flow maps by their values and by their keys, block
//! maps, and compact maps in flow sequences (two nodes a level).
//!
//! The wheel loads each document as a config (`config_calls`), and wraps what yaml-cpp throws
//! in "Error: Loading the OCIO profile failed. " (`OCIOYaml::Read`, src/OpenColorIO/
//! OCIOYaml.cpp:5419-5437 @ v2.5.2): where the port's parser throws, the wheel reports the
//! same `what()`; where it doesn't, the wheel's error comes from OCIO's own checks, not
//! from the parser.

use ocio::yaml_cpp::parse::load;
use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::oracle_values::{bytes, hex};
use serde_json::{Value, json};

const PREFIX: &[u8] = b"Error: Loading the OCIO profile failed. ";

/// A document of `n` nodes nested in the root, in each shape.
fn documents(n: usize) -> Vec<Vec<u8>> {
    let mut block_map = String::new();
    for i in 0..n {
        block_map.push_str(&" ".repeat(i));
        block_map.push_str("a:\n");
    }
    block_map.push_str(&" ".repeat(n));
    block_map.push_str("x\n");
    vec![
        format!("{}x{}", "[".repeat(n), "]".repeat(n)).into_bytes(),
        format!("{}x{}", "{a: ".repeat(n), "}".repeat(n)).into_bytes(),
        format!("{}x{}", "{? ".repeat(n), "}".repeat(n)).into_bytes(),
        // Two nodes a level: (n - 1) / 2 levels are 497 or 498 nodes deep, for n 498 or 499.
        format!("{}x{}", "[a: ".repeat((n - 1) / 2), "]".repeat((n - 1) / 2)).into_bytes(),
        format!("{}x\n", "- ".repeat(n)).into_bytes(),
        block_map.into_bytes(),
    ]
}

/// The wheel's error making a config of `doc`, as bytes.
fn wheel_error(config: &Value) -> Vec<u8> {
    match config.get("undecodable") {
        Some(h) => bytes(&json!({ "bytes": h })),
        None => bytes(&config["exception"]["message"]),
    }
}

#[test]
fn the_deepest_documents_parse_on_a_small_stack() {
    let mut docs = documents(498);
    docs.extend(documents(499));

    let inputs = docs.clone();
    let port: Vec<Option<Vec<u8>>> = std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(move || {
            inputs
                .iter()
                .map(|doc| load(doc).err().map(|e| e.what()))
                .collect()
        })
        .unwrap()
        .join()
        .unwrap();

    let calls: Vec<BatchCall<'_>> = docs
        .iter()
        .map(|doc| BatchCall {
            cmd: "config_calls",
            args: json!({"config": {"yaml": {"bytes": hex(doc)}}}),
            blobs: Vec::new(),
        })
        .collect();
    let wheel = Oracle::get().batch(&calls, true);

    for ((doc, port), wheel) in docs.iter().zip(&port).zip(wheel) {
        let config = &wheel.expect("config_calls").result["config"];
        let label = String::from_utf8_lossy(&doc[..doc.len().min(12)]).into_owned();
        assert!(!config.is_null(), "{label}: the wheel loads it");
        let wheel = wheel_error(config);
        match port {
            Some(what) => {
                assert_eq!(
                    String::from_utf8_lossy(&wheel),
                    String::from_utf8_lossy(&[PREFIX, what].concat()),
                    "{label}"
                );
            }
            None => assert!(
                !wheel.ends_with(b"bad file"),
                "{label}: the wheel's parser refuses it: {}",
                String::from_utf8_lossy(&wheel)
            ),
        }
    }
    // The deepest documents parse; the flow and block collections one deeper don't.
    assert!(port[..6].iter().all(Option::is_none), "{port:?}");
    assert!(port[6..9].iter().all(Option::is_some), "{port:?}");
    assert!(port[10..].iter().all(Option::is_some), "{port:?}");
}
