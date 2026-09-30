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
        let batched = Oracle::get().batch(&calls, cache);
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
