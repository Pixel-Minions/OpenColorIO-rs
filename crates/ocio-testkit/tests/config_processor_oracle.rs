// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's `config_processor` command (`oracle/ocio_oracle/config_processors.py`),
//! against the wheel itself: each part reports exactly what its own command reports of the
//! same processor (`processor_ops`, `cpu_apply`, `image_apply`, `gpu_shader`), every overload
//! and its context variant reach the library, the context's calls reach the processor, and the
//! requests it can't run exactly are refused.

use ocio_testkit::Oracle;
use ocio_testkit::oracle::{BatchCall, Response, f32_to_bytes};
use ocio_testkit::oracle_values::bytes;
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
        bytes(&all["processor"]["cache_id"]),
        r[1].result["processor"]["cache_id"]
            .as_str()
            .unwrap()
            .as_bytes()
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
        assert!(
            result["processor"]["cache_id"]["bytes"].is_string(),
            "{a}: {result}"
        );
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

/// A config whose file transform takes its LUT from a context variable, with two LUTs.
fn lut_request(context: Option<Value>, overload: Value) -> Value {
    let lut =
        |top: &str| format!("Version 1\nFrom 0.0 1.0\nLength 2\nComponents 1\n{{\n0\n{top}\n}}\n");
    let mut args = json!({
        "config": {"yaml": "ocio_profile_version: 2\nsearch_path: .\nroles:\n  default: lin\n\
            colorspaces:\n  - !<ColorSpace>\n    name: lin\n  - !<ColorSpace>\n    name: lut\n\
            \x20   from_scene_reference: !<FileTransform> {src: lut_$LUT.spi1d}\n"},
        "env": {"LUT": "a"},
        "files": {"lut_a.spi1d": lut("1"), "lut_b.spi1d": lut("0.5")},
        "overload": overload,
        "cpu": [{"blob": 0}],
    });
    if let Some(context) = context {
        args["context"] = context;
    }
    args
}

/// The context passed reaches getProcessor: a variable it sets picks another LUT. It is a copy
/// of the config's current context, which it leaves alone.
#[test]
fn the_context_reaches_the_processor() {
    let input = pixels();
    let names = json!({"names": ["lin", "lut"]});
    let set_b = json!({"calls": [{"call": "setStringVar", "args": ["LUT", "b"]}]});
    let set_a = json!({"calls": [{"call": "setStringVar", "args": ["LUT", "a"]}]});
    let args = [
        lut_request(None, names.clone()),
        lut_request(Some(set_b), names.clone()),
        lut_request(Some(set_a), names.clone()),
        lut_request(
            Some(json!({"base": "new", "calls": [
            {"call": "setStringVar", "args": ["LUT", "b"]},
            {"call": "setSearchPath", "args": ["."]}]})),
            names,
        ),
    ];
    let calls: Vec<BatchCall<'_>> = args
        .iter()
        .map(|a| BatchCall {
            cmd: "config_processor",
            args: a.clone(),
            blobs: vec![&input],
        })
        .collect();
    let r = batch(&calls);
    let pixels_of = |i: usize| {
        let index = r[i].result["cpu"][0]["pixels"]
            .as_u64()
            .unwrap_or_else(|| panic!("{}", r[i].result));
        let index = usize::try_from(index).unwrap();
        r[i].blobs[index].clone()
    };
    assert_ne!(pixels_of(0), pixels_of(1), "{}", r[1].result);
    assert_eq!(pixels_of(0), pixels_of(2));
    assert_eq!(pixels_of(1), pixels_of(3));
    assert_ne!(
        r[0].result["processor"]["cache_id"],
        r[1].result["processor"]["cache_id"]
    );
    for reply in &r[1..3] {
        assert_eq!(
            reply.result["config_context_cache_id"], r[0].result["config_context_cache_id"],
            "{}",
            reply.result
        );
    }
}

/// The direction an overload is given reaches getProcessor.
#[test]
fn directions_reach_the_processor() {
    let input = pixels();
    let mut args = Vec::new();
    for dir in ["TRANSFORM_DIR_FORWARD", "TRANSFORM_DIR_INVERSE"] {
        let exponent = json!({"class": "ExponentTransform", "args": {"value": [2, 2, 2, 1]}});
        for overload in [
            json!({"display_view": ["lin", "disp", "view", dir]}),
            json!({"named_transform": ["nt", dir]}),
            json!({"named_transform_name": ["nt", dir]}),
            json!({"transform_direction": [exponent, dir]}),
        ] {
            args.push(json!({"config": {"yaml": YAML}, "overload": overload,
                             "cpu": [{"blob": 0}]}));
        }
    }
    let calls: Vec<BatchCall<'_>> = args
        .iter()
        .map(|a| BatchCall {
            cmd: "config_processor",
            args: a.clone(),
            blobs: vec![&input],
        })
        .collect();
    let r = batch(&calls);
    for i in 0..4 {
        assert_ne!(
            r[i].result["processor"]["cache_id"],
            r[i + 4].result["processor"]["cache_id"],
            "{}",
            args[i]
        );
    }
}

/// The optimization flags of the ops and gpu parts reach the optimizer: what they report is
/// what processor_ops and gpu_shader report with the same flags, and the two settings differ.
#[test]
fn optimization_flags_reach_the_parts() {
    let two = json!({"class": "GroupTransform", "children": [
        {"class": "MatrixTransform", "args": {"offset": [0.25, 0, 0, 0]}},
        {"class": "MatrixTransform", "args": {"offset": [0.5, 0, 0, 0]}}]});
    let mut args = Vec::new();
    for flags in ["OPTIMIZATION_NONE", "OPTIMIZATION_DEFAULT"] {
        args.push((
            "config_processor",
            json!({
            "config": "raw", "overload": {"transform": [two]},
            "ops": {"optimization": flags},
            "gpu": {"optimization": flags, "shader": {"language": "GPU_LANGUAGE_GLSL_4_0"}}}),
        ));
        args.push((
            "processor_ops",
            json!({"transform": two, "optimization": flags}),
        ));
        args.push((
            "gpu_shader",
            json!({"transform": two, "optimization": flags,
                                        "shader": {"language": "GPU_LANGUAGE_GLSL_4_0"}}),
        ));
    }
    let calls: Vec<BatchCall<'_>> = args
        .iter()
        .map(|(cmd, a)| BatchCall {
            cmd,
            args: a.clone(),
            blobs: vec![],
        })
        .collect();
    let r = batch(&calls);
    for k in [0, 3] {
        assert_eq!(
            r[k].result["ops"]["optimized"],
            r[k + 1].result["optimized"]
        );
        assert_eq!(
            r[k].result["gpu"]["gpu_cache_id"],
            r[k + 2].result["gpu_cache_id"]
        );
    }
    assert_ne!(
        r[0].result["ops"]["optimized"],
        r[3].result["ops"]["optimized"]
    );
    assert_ne!(
        r[0].result["gpu"]["gpu_cache_id"],
        r[3].result["gpu"]["gpu_cache_id"]
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
        json!({"config": yaml, "overload": {"names": ["lin", "log"]},
               "context": {"calls": [{"call": "setStringVar", "args": [{"f64": -1}, "x"]}]}}),
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
