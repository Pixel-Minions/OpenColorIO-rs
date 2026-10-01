// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's `processor_debug_log` command (`oracle/ocio_oracle/debug_log.py`) against the
//! wheel itself:
//! - the optimizer's lists of ops before and after come through, each op's line carrying the
//!   cache ID that the CPU processor's own cache ID ends with, an independent read of the
//!   wheel (`OpRcPtrVec::optimize`, src/OpenColorIO/OpOptimizers.cpp:620-627, 735-751 @
//!   v2.5.2; `SerializeOpVec`, Op.cpp:473-489);
//! - the flags reach `getOptimizedCPUProcessor`: without optimization the second list says one
//!   pass (OpOptimizers.cpp:638-651);
//! - the command restores the logging level and the logging function it found, so a
//!   `cpu_apply` after it in the batch logs nothing at the default level;
//! - every error path reports its stage, with the messages logged so far; and its refusals.

use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use serde_json::{Value, json};

/// Runs each call of `args` in one batch, `cpu_apply` included; each must reach the command.
fn run(calls: &[(&'static str, Value)]) -> Vec<Value> {
    let pixel = [0u8; 16];
    let calls: Vec<BatchCall<'_>> = calls
        .iter()
        .map(|(cmd, args)| BatchCall {
            cmd,
            args: args.clone(),
            blobs: if *cmd == "cpu_apply" {
                vec![&pixel[..]]
            } else {
                vec![]
            },
        })
        .collect();
    Oracle::get()
        .batch(&calls, false)
        .into_iter()
        .map(|r| r.unwrap_or_else(|e| panic!("{e}")).result)
        .collect()
}

/// A group of two offsets, which the optimizer combines into one Matrix op.
fn two_offsets() -> Value {
    let offset = json!({"class": "MatrixTransform", "args": {"offset": [0.1, 0.2, 0.3, 0.0]}});
    json!({"class": "GroupTransform", "children": [offset.clone(), offset]})
}

/// The messages' text, without the `[OpenColorIO Debug]: ` prefix each line has.
fn lines(messages: &Value) -> Vec<String> {
    messages
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            let m = m.as_str().unwrap();
            m.strip_prefix("[OpenColorIO Debug]: ")
                .unwrap_or_else(|| panic!("not a debug message: {m:?}"))
                .to_string()
        })
        .collect()
}

#[test]
fn the_optimizers_lists_come_through() {
    let [result, unoptimized] = &run(&[
        (
            "processor_debug_log",
            json!({"transform": two_offsets(), "in_bitdepth": "BIT_DEPTH_UINT8"}),
        ),
        (
            "processor_debug_log",
            json!({"transform": two_offsets(), "optimization": "OPTIMIZATION_NONE"}),
        ),
    ])[..] else {
        unreachable!()
    };

    let lines_ = lines(&result["cpu_processor"]);
    let start = lines_
        .iter()
        .position(|l| l == "Optimizing Op Vec...\n")
        .unwrap_or_else(|| panic!("no list before: {lines_:?}"));
    assert_eq!(lines_[start - 1], "**\n");
    assert!(lines_[start + 1].starts_with("    Op 0: <MatrixOffsetOp> <MatrixOffsetOp "));
    assert!(lines_[start + 2].starts_with("    Op 1: <MatrixOffsetOp> <MatrixOffsetOp "));
    let after = lines_
        .iter()
        .position(|l| l.starts_with("Optimized 2->1, "))
        .unwrap_or_else(|| panic!("no list after: {lines_:?}"));
    assert!(lines_[after].contains(" 1 ops combined, "), "{lines_:?}");
    // The one op left is the CPU processor's.
    let op = lines_[after + 1]
        .strip_prefix("    Op 0: <MatrixOffsetOp> ")
        .unwrap()
        .trim_end();
    let cache_id = result["cpu_cache_id"].as_str().unwrap();
    assert!(cache_id.starts_with("CPU Processor: from 8ui to 32f "));
    assert!(
        cache_id.ends_with(&format!(" ops:  {op}")),
        "{cache_id:?} {op:?}"
    );
    assert_eq!(result["level"], "info");

    // Without optimization: one pass, two ops kept.
    let lines_ = lines(&unoptimized["cpu_processor"]);
    assert!(
        lines_
            .iter()
            .any(|l| l == "Optimized 2->2, 1 pass, 0 no-op types removed\n"),
        "{lines_:?}"
    );
}

#[test]
fn the_level_and_the_logging_function_are_restored() {
    let results = run(&[
        ("processor_debug_log", json!({"transform": two_offsets()})),
        ("processor_debug_log", json!({"transform": two_offsets()})),
        ("cpu_apply", json!({"transform": two_offsets()})),
    ]);
    assert_eq!(results[0]["level"], "info");
    assert_eq!(results[1]["level"], "info");
    assert_eq!(results[2]["log"], json!([]), "{}", results[2]);
}

#[test]
fn errors_report_their_stage_and_the_messages_so_far() {
    let singular = json!({"class": "MatrixTransform",
        "args": {"matrix": vec![0.0; 16], "direction": {"enum": "TRANSFORM_DIR_INVERSE"}}});
    let results = run(&[
        // The binding's constructor validates the transform.
        ("processor_debug_log", json!({"transform": singular})),
        // No such color space.
        (
            "processor_debug_log",
            json!({"src": "nowhere", "dst": "nowhere either"}),
        ),
        // A bit depth the CPU processor refuses, with an op to optimize.
        (
            "processor_debug_log",
            json!({"transform": two_offsets(), "in_bitdepth": "BIT_DEPTH_UINT14"}),
        ),
    ]);
    let stages: Vec<&str> = results
        .iter()
        .map(|r| r["stage"].as_str().unwrap_or_else(|| panic!("{r}")))
        .collect();
    assert_eq!(stages, ["transform", "processor", "cpu_processor"]);
    for result in &results {
        assert!(result["exception"]["message"].is_string(), "{result}");
        assert_eq!(result["level"], "info");
    }
    // Building the processor logged before the CPU processor raised.
    assert!(!lines(&results[2]["processor"]).is_empty());
    assert!(results[2].get("cpu_cache_id").is_none());
}

#[test]
fn requests_it_refuses() {
    for args in [
        json!({"transform": two_offsets(), "flags": "OPTIMIZATION_NONE"}),
        json!({"transform": two_offsets(), "in_bitdepth": "8ui"}),
    ] {
        let calls = [BatchCall {
            cmd: "processor_debug_log",
            args: args.clone(),
            blobs: vec![],
        }];
        let response = Oracle::get().batch(&calls, false).remove(0);
        assert!(response.is_err(), "{args} was accepted");
    }
}
