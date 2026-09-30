// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `Oracle::batch` against single calls: many calls in one oracle process return exactly what
//! each call returns on its own, results and pixels, refusals included.

use ocio_testkit::oracle::{BatchCall, bytes_to_f32, f32_to_bytes};
use ocio_testkit::probe::{specials, to_rgba_cycled};
use ocio_testkit::{Oracle, assert_f32_bits_eq};
use serde_json::{Value, json};

fn log(base: f64, fast_math: bool) -> Value {
    let mut args = json!({"transform": {"class": "LogTransform", "args": {"base": base}}});
    if !fast_math {
        args["optimization"] = json!(["OPTIMIZATION_LOSSLESS"]);
    }
    args
}

#[test]
fn batched_calls_return_what_single_calls_return() {
    let probe = f32_to_bytes(&to_rgba_cycled(&specials()));
    let other = f32_to_bytes(&[0.25, 0.5, f32::from_bits(0xff80_0001), 1.0]);
    let nan_base = json!({
        "config": {"yaml": "ocio_profile_version: 2.1\nroles:\n  default: raw\n\
            colorspaces:\n  - !<ColorSpace>\n    name: raw\n  - !<ColorSpace>\n    name: cs\n\
            \x20   from_scene_reference: !<LogTransform> {base: .nan}\n"},
        "src": "raw",
        "dst": "cs",
    });
    // (arguments, input): shared and distinct inputs; base 1 is refused.
    let calls = [
        (log(2.0, true), &probe),
        (log(10.0, false), &probe),
        (nan_base, &other),
        (log(1.0, true), &probe),
        (log(3.7, true), &other),
    ];
    let calls: Vec<BatchCall<'_>> = calls
        .iter()
        .map(|(args, blob)| BatchCall {
            cmd: "cpu_apply",
            args: args.clone(),
            blobs: vec![blob.as_slice()],
        })
        .collect();
    for cache in [false, true] {
        let batched: Vec<_> = Oracle::get()
            .batch(&calls, cache)
            .into_iter()
            .map(|r| r.unwrap_or_else(|e| panic!("{e}")))
            .collect();
        assert_eq!(batched.len(), calls.len());
        for (call, batched) in calls.iter().zip(&batched) {
            let single = Oracle::get().call(call.cmd, call.args.clone(), &call.blobs);
            assert_eq!(batched.result, single.result, "{}", call.args);
            assert_eq!(batched.blobs.len(), single.blobs.len(), "{}", call.args);
            for (b, s) in batched.blobs.iter().zip(&single.blobs) {
                assert_f32_bits_eq(&call.args.to_string(), &bytes_to_f32(s), &bytes_to_f32(b));
            }
        }
        assert!(batched[3].result.get("exception").is_some());
        assert!(batched[2].result.get("exception").is_none());
    }
}

/// A call that raises, here on a transform class that doesn't exist, reports its traceback
/// and index in its own entry; the calls around it still run.
#[test]
fn a_failing_call_reports_its_error_and_the_others_still_run() {
    let pixels = f32_to_bytes(&[0.25, 0.5, 1.0, 1.0]);
    let args = json!({"calls": [
        {"cmd": "cpu_apply", "args": log(2.0, true), "blobs": [0]},
        {"cmd": "cpu_apply", "args": {"transform": {"class": "NoSuchTransform"}}, "blobs": [0]},
        {"cmd": "cpu_apply", "args": log(10.0, true), "blobs": [0]},
    ]});
    let response = Oracle::get().call_uncached("batch", args, &[&pixels]);
    let entries = response.result.as_array().expect("a list of entries");
    assert_eq!(entries.len(), 3);
    assert!(
        entries[0]["result"].get("exception").is_none(),
        "{}",
        entries[0]
    );
    assert_eq!(entries[1]["call"], 1);
    let error = entries[1]["error"].as_str().expect("an error");
    assert!(error.contains("NoSuchTransform"), "{error}");
    assert!(
        entries[2]["result"].get("exception").is_none(),
        "{}",
        entries[2]
    );
    assert_eq!(response.blobs.len(), 2);
}

/// cpu_apply says where OCIO raised: while loading the config (a YAML number yaml-cpp can't
/// parse), while building the transform (the Python bindings' constructors validate it), or in
/// getProcessor (which validates the ops of a config's transform).
#[test]
fn cpu_apply_reports_the_stage_of_an_exception() {
    let yaml = |transform: &str| {
        json!({
            "config": {"yaml": format!(
                "ocio_profile_version: 2.1\nroles:\n  default: raw\ncolorspaces:\n  - !<ColorSpace>\n    \
                 name: raw\n  - !<ColorSpace>\n    name: cs\n    from_scene_reference: {transform}\n"
            )},
            "src": "raw",
            "dst": "cs",
        })
    };
    let pixels = f32_to_bytes(&[0.25, 0.5, 1.0, 1.0]);
    for (args, stage) in [
        (yaml("!<LogTransform> {base: inf}"), "config"),
        (log(1.0, true), "transform"),
        (yaml("!<LogTransform> {base: 1}"), "processor"),
    ] {
        let response = Oracle::get().call("cpu_apply", args.clone(), &[&pixels]);
        assert!(
            response.result.get("exception").is_some(),
            "{args}: {}",
            response.result
        );
        assert_eq!(
            response.result["stage"], stage,
            "{args}: {}",
            response.result
        );
    }
    let accepted = Oracle::get().call("cpu_apply", log(2.0, true), &[&pixels]);
    assert!(
        accepted.result.get("stage").is_none(),
        "{}",
        accepted.result
    );
}

/// `Oracle::batch` returns the failing call's error, with its index, and the other calls'
/// responses.
#[test]
fn batch_returns_the_error_of_the_failing_call_only() {
    let pixels = f32_to_bytes(&[0.25, 0.5, 1.0, 1.0]);
    let bad = json!({"transform": {"class": "NoSuchTransform"}});
    let calls: Vec<BatchCall<'_>> = [log(2.0, true), bad, log(10.0, true)]
        .into_iter()
        .map(|args| BatchCall {
            cmd: "cpu_apply",
            args,
            blobs: vec![pixels.as_slice()],
        })
        .collect();
    let results = Oracle::get().batch(&calls, false);
    assert!(results[0].is_ok() && results[2].is_ok());
    let error = results[1].as_ref().expect_err("the bad call fails");
    assert!(
        error.contains("call 1 of 3") && error.contains("NoSuchTransform"),
        "{error}"
    );
}
