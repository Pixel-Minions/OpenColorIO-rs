// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's `config_processor` command (`oracle/ocio_oracle/config_processors.py`),
//! against the wheel itself: each part reports exactly what its own command reports of the
//! same processor (`processor_ops`, `cpu_apply`, `image_apply`, `gpu_shader`), every overload
//! and its context variant reach the library, the context's calls reach the processor, and the
//! requests it can't run exactly are refused.

use ocio_testkit::Oracle;
use ocio_testkit::oracle::{BatchCall, Response, f32_to_bytes};
use serde_json::{Value, json};

/// A config with two color spaces, a display with a view, and a named transform.
const YAML: &str = "ocio_profile_version: 2\n\
roles:\n  default: lin\n\
displays:\n  disp:\n    - !<View> {name: view, colorspace: log}\n\
colorspaces:\n\
  - !<ColorSpace>\n    name: lin\n\
  - !<ColorSpace>\n    name: log\n    from_scene_reference: !<LogTransform> {base: 2}\n\
named_transforms:\n\
  - !<NamedTransform>\n    name: nt\n    transform: !<ExponentTransform> {value: [2, 2, 2, 1]}\n";

fn pixels() -> Vec<u8> {
    f32_to_bytes(&[0.0, 0.25, 0.5, 1.0, 1.5, -0.5, 2.0, 0.75])
}

fn batch(calls: &[BatchCall<'_>]) -> Vec<Response> {
    Oracle::get()
        .batch(calls, true)
        .into_iter()
        .enumerate()
        .map(|(i, r)| r.unwrap_or_else(|e| panic!("call {i}: {e}")))
        .collect()
}

/// Each part of a `names` processor is what its own command reports of `src`/`dst` on the
/// same config, blobs included.
#[test]
fn each_part_is_its_commands_report() {
    let input = pixels();
    let processor = json!({"config": {"yaml": YAML}, "src": "lin", "dst": "log"});
    let image = json!({"buffers": [{"blob": 0}],
                       "images": [{"kind": "packed", "data": {"buffer": 0}, "width": 2,
                                   "height": 1, "num_channels": 4}],
                       "apply": [0]});
    let combined = json!({
        "config": {"yaml": YAML},
        "overload": {"names": ["lin", "log"]},
        "ops": {"optimization": "OPTIMIZATION_LOSSLESS"},
        "cpu": [{"blob": 0}, {"blob": 0, "out_bitdepth": "BIT_DEPTH_UINT16"}],
        "image": image,
        "gpu": {"shader": {"language": "GPU_LANGUAGE_HLSL_DX11"}},
    });
    let mut ops = processor.clone();
    ops["optimization"] = json!("OPTIMIZATION_LOSSLESS");
    let mut cpu16 = processor.clone();
    cpu16["out_bitdepth"] = json!("BIT_DEPTH_UINT16");
    let mut image_args = image.clone();
    for (k, v) in processor.as_object().unwrap() {
        image_args[k] = v.clone();
    }
    let mut gpu = processor.clone();
    gpu["shader"] = json!({"language": "GPU_LANGUAGE_HLSL_DX11"});
    let calls = [
        BatchCall {
            cmd: "config_processor",
            args: combined,
            blobs: vec![&input],
        },
        BatchCall {
            cmd: "processor_ops",
            args: ops,
            blobs: vec![],
        },
        BatchCall {
            cmd: "cpu_apply",
            args: processor.clone(),
            blobs: vec![&input],
        },
        BatchCall {
            cmd: "cpu_apply",
            args: cpu16,
            blobs: vec![&input],
        },
        BatchCall {
            cmd: "image_apply",
            args: image_args,
            blobs: vec![&input],
        },
        BatchCall {
            cmd: "gpu_shader",
            args: gpu,
            blobs: vec![],
        },
    ];
    let r = batch(&calls);
    let all = &r[0].result;
    assert_eq!(all["config"], Value::Null, "{all}");
    let blob = |i: &Value| &r[0].blobs[usize::try_from(i.as_u64().unwrap()).unwrap()];
    // processor_ops
    assert_eq!(all["ops"]["processor"], r[1].result["processor"]);
    assert_eq!(all["ops"]["optimized"], r[1].result["optimized"]);
    assert_eq!(
        all["processor"]["cache_id"],
        r[1].result["processor"]["cache_id"]
    );
    // cpu_apply
    for (part, single) in all["cpu"].as_array().unwrap().iter().zip(&r[2..4]) {
        assert_eq!(part["cpu_cache_id"], single.result["cpu_cache_id"]);
        assert_eq!(*blob(&part["pixels"]), single.blobs[0]);
    }
    // image_apply
    let image = &all["image"];
    for key in [
        "images",
        "processor_cache_id",
        "cpu_cache_id",
        "cpu_processor",
    ] {
        assert_eq!(image[key], r[4].result[key], "{key}");
    }
    assert_eq!(*blob(&image["buffers"][0]), r[4].blobs[0]);
    // gpu_shader
    let shader = &all["gpu"]["shader"];
    assert_eq!(all["gpu"]["gpu_cache_id"], r[5].result["gpu_cache_id"]);
    assert_eq!(*blob(&shader["text"]), r[5].blobs[0]);
    let mut expected = r[5].result["shader"].clone();
    expected["text"] = shader["text"].clone();
    assert_eq!(*shader, expected);
}

/// Every overload and its context variant reaches the library; the context's calls run on a
/// copy of the current context and are reported.
#[test]
fn every_overload_reaches_the_library() {
    let input = pixels();
    let forward = json!("TRANSFORM_DIR_FORWARD");
    let exponent = json!({"class": "ExponentTransform", "args": {"value": [2, 2, 2, 1]}});
    let overloads = [
        json!({"color_spaces": ["lin", "log"]}),
        json!({"names": ["lin", "log"]}),
        json!({"display_view": ["lin", "disp", "view", forward]}),
        json!({"named_transform": ["nt", forward]}),
        json!({"named_transform_name": ["nt", forward]}),
        json!({"transform_direction": [exponent, forward]}),
    ];
    let mut args = Vec::new();
    for overload in &overloads {
        for context in [
            None,
            Some(json!({"calls": [{"call": "setStringVar",
                                                       "args": ["V", "x"]}]})),
        ] {
            let mut a = json!({"config": {"yaml": YAML}, "overload": overload,
                               "cpu": [{"blob": 0}]});
            if let Some(context) = context {
                a["context"] = context;
            }
            args.push(a);
        }
    }
    args.push(
        json!({"config": {"yaml": YAML}, "overload": {"transform": [exponent]},
                     "cpu": [{"blob": 0}]}),
    );
    let calls: Vec<BatchCall<'_>> = args
        .iter()
        .map(|a| BatchCall {
            cmd: "config_processor",
            args: a.clone(),
            blobs: vec![&input],
        })
        .collect();
    let replies = batch(&calls);
    for (a, reply) in args.iter().zip(&replies) {
        let result = &reply.result;
        assert!(result["processor"]["cache_id"].is_string(), "{a}: {result}");
        assert!(result["cpu"][0]["pixels"].is_u64(), "{a}: {result}");
        if a.get("context").is_some() {
            assert_eq!(
                result["context"],
                json!([{"result": null, "log": []}]),
                "{a}"
            );
        }
    }
    // A color space getColorSpace doesn't find reaches getProcessor as None.
    let missing = Oracle::get().call(
        "config_processor",
        json!({"config": {"yaml": YAML}, "overload": {"color_spaces": ["nope", "log"]}}),
        &[],
    );
    assert_eq!(
        missing.result["processor"]["exception"]["type"], "Exception",
        "{}",
        missing.result
    );
}

/// Requests it can't run exactly are refused.
#[test]
fn bad_requests_are_refused() {
    let yaml = json!({"yaml": YAML});
    for args in [
        json!({"config": yaml, "overload": {"names": ["lin"]}}),
        json!({"config": yaml, "overload": {"nope": []}}),
        json!({"config": yaml, "overload": {"names": ["lin", "log"], "transform": []}}),
        json!({"config": yaml, "overload": {"transform": [{"class": "MatrixTransform"}]},
               "context": {}}),
        json!({"config": yaml, "overload": {"display_view": ["lin", "disp", "view", "fwd"]}}),
        json!({"config": yaml, "overload": {"names": ["lin", "log"]}, "cpu": {"blob": 0}}),
        json!({"config": yaml, "overload": {"names": ["lin", "log"]}, "cpu": [{"blob": 3}]}),
        json!({"config": yaml, "overload": {"names": ["lin", "log"]},
               "cpu": [{"blob": 0, "out_bitdepth": "BIT_DEPTH_9"}]}),
        json!({"config": yaml, "overload": {"names": ["lin", "log"]}, "ops": {"x": 1}}),
        json!({"config": yaml, "overload": {"names": ["lin", "log"]},
               "context": {"base": "other"}}),
        json!({"config": yaml, "overload": {"names": ["lin", "log"]}, "other": 1}),
    ] {
        assert!(
            Oracle::get()
                .try_call("config_processor", args.clone(), &[&pixels()])
                .is_err(),
            "{args}"
        );
    }
}
