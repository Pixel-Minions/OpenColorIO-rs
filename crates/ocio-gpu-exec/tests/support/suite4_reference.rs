// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Uncached per-file wheel streams for upstream's grading GPU cases and retests.
//! Finite companions replay the same CPU bindings and updates over the complete
//! padded input, changing only non-finite input components to zero.

use ocio_gpu_exec::gpu_unit_test::{GpuUnitTest, ReferenceRun};
use ocio_testkit::Oracle;
use ocio_testkit::gpu::GpuShaderReply;
use ocio_testkit::oracle::{BatchCall, Response, f32_to_bytes};
use ocio_testkit::upstream::gpu::{ENTRIES, HEIGHT, WIDTH, comparison_dimensions};
use serde_json::{Value, json};

/// Ordered upstream input descriptions; updates[0] is empty. Later entries are
/// dynamic_apply operations before that frame. Bindings name the CPU processor's
/// own properties, which the wheel decouples from the parent Processor.
pub(super) struct Plan {
    pub(super) test: GpuUnitTest,
    pub(super) transforms: Vec<Value>,
    pub(super) bindings: Vec<&'static str>,
    pub(super) updates: Vec<Vec<Value>>,
}

fn finite_parameters(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(_) => true,
        Value::Number(n) => n.as_f64().is_some_and(f64::is_finite),
        Value::String(s) => !matches!(
            s.to_ascii_lowercase().as_str(),
            "nan" | "inf" | "+inf" | "-inf" | "infinity" | "+infinity" | "-infinity"
        ),
        Value::Array(a) => a.iter().all(finite_parameters),
        // oracle/spec.py:154-155 decodes this existing input tag as f64 bits.
        Value::Object(m) if m.contains_key("f64") => {
            m.len() == 1
                && m["f64"]
                    .as_u64()
                    .is_some_and(|bits| f64::from_bits(bits).is_finite())
        }
        Value::Object(m) => m.iter().all(|(key, value)| {
            !key.to_ascii_lowercase().contains("bits") && finite_parameters(value)
        }),
    }
}

/// All original streams, complete finite streams and shader extractions are
/// independent requests to the actual wheel in one uncached per-file batch.
pub(super) fn batch(plans: &[Plan]) -> Vec<Vec<ReferenceRun>> {
    assert!(!plans.is_empty());
    let prepared: Vec<_> = plans.iter().map(|plan| prepare(plan, false)).collect();
    let finite: Vec<_> = plans
        .iter()
        .map(|plan| {
            plan.test
                .values
                .input_values
                .iter()
                .any(|v| !v.is_finite())
                .then(|| {
                    assert!(plan.transforms.iter().all(finite_parameters));
                    assert!(plan.updates.iter().flatten().all(finite_parameters));
                    prepare(plan, true)
                })
        })
        .collect();
    let gpu_streams: Vec<_> = plans
        .iter()
        .map(|plan| (plan.test.num_retests() != 0).then(|| prepare_gpu(plan)))
        .collect();
    let mut calls = Vec::new();
    for (((plan, original), companion), gpu_stream) in
        plans.iter().zip(&prepared).zip(&finite).zip(&gpu_streams)
    {
        calls.push(BatchCall {
            cmd: "dynamic_apply",
            args: original.args.clone(),
            blobs: vec![original.input.as_slice()],
        });
        if let Some(companion) = companion {
            calls.push(BatchCall {
                cmd: "dynamic_apply",
                args: companion.args.clone(),
                blobs: vec![companion.input.as_slice()],
            });
        }
        if let Some(gpu_stream) = gpu_stream {
            calls.push(BatchCall {
                cmd: "dynamic_apply",
                args: gpu_stream.args.clone(),
                blobs: Vec::new(),
            });
        }
        for transform in &plan.transforms {
            calls.push(BatchCall {
                cmd: "gpu_shader",
                args: json!({"config":{"create":{"processor_cache":"off"}},
                    "transform":transform, "shader":plan.test.shader_settings.json()}),
                blobs: Vec::new(),
            });
        }
    }
    let answers = Oracle::get().batch(&calls, false);
    assert_eq!(answers.len(), calls.len());
    let mut answers = answers.into_iter();
    let mut all = Vec::new();
    for (((plan, original), companion), gpu_prepared) in
        plans.iter().zip(&prepared).zip(&finite).zip(&gpu_streams)
    {
        let cpu = cpu_frames(
            &answers.next().unwrap().expect("actual wheel CPU stream"),
            plan,
            original,
        );
        let finite_cpu = companion.as_ref().map(|prepared| {
            cpu_frames(
                &answers
                    .next()
                    .unwrap()
                    .expect("actual complete finite CPU stream"),
                plan,
                prepared,
            )
        });
        let gpu_stream = gpu_prepared.as_ref().map(|prepared| {
            let response = answers
                .next()
                .unwrap()
                .expect("actual reused wheel GPU descriptor");
            validate_gpu_stream(&response, plan, prepared);
            (prepared, response)
        });
        let mut original_gpu_result = None;
        let mut out = Vec::new();
        for (frame, pixels) in cpu.into_iter().enumerate() {
            let gpu = GpuShaderReply::from_response(
                answers.next().unwrap().expect("actual wheel shader"),
            );
            assert!(
                gpu.raised().is_none(),
                "{}/{}: {:?}",
                plan.test.group,
                plan.test.name,
                gpu.raised()
            );
            assert!(gpu.shader.is_some());
            assert_eq!(gpu.result["log"], json!([]));
            assert_eq!(gpu.shader().dynamic_properties.len(), plan.bindings.len());
            if frame == 0 {
                original_gpu_result = Some(gpu.result.clone());
            }
            let gpu = if let Some((prepared, stream)) = &gpu_stream {
                gpu_frame(
                    plan,
                    prepared,
                    stream,
                    frame,
                    original_gpu_result.as_ref().unwrap(),
                    &gpu,
                )
            } else {
                gpu
            };
            out.push(ReferenceRun {
                cpu: pixels,
                finite_cpu: finite_cpu.as_ref().map(|frames| frames[frame].clone()),
                gpu,
                shader_settings: plan.test.shader_settings.clone(),
            });
        }
        assert_eq!(out.len(), plan.test.num_retests() + 1);
        all.push(out);
    }
    assert!(answers.next().is_none());
    assert_eq!(all.len(), plans.len());
    all
}

struct Prepared {
    input: Vec<u8>,
    args: Value,
    apply_steps: Vec<usize>,
    steps: usize,
}

/// Validate the original reviewed stream contract equally for both input sets.
fn cpu_frames(cpu: &Response, plan: &Plan, prepared: &Prepared) -> Vec<Vec<f32>> {
    let test = &plan.test;
    assert!(
        cpu.result.get("exception").is_none(),
        "{}/{}: {}",
        test.group,
        test.name,
        cpu.result
    );
    assert_eq!(cpu.result["log"], json!([]));
    let results = cpu.result["steps"].as_array().unwrap();
    assert_eq!(results.len(), prepared.steps);
    for (index, step) in results.iter().enumerate() {
        assert!(
            step.get("exception").is_none(),
            "{}/{}: {step}",
            test.group,
            test.name
        );
        assert_eq!(step["log"], json!([]));
        assert!(step["result"].get("unbound").is_none());
        if index < plan.bindings.len() {
            assert_eq!(step["has"], json!(true));
        }
    }
    let mut frames = Vec::new();
    for &step in &prepared.apply_steps {
        let buffers = results[step]["result"]["buffers"].as_array().unwrap();
        assert_eq!(buffers.len(), 2);
        let source = buffers[0].as_u64().unwrap() as usize;
        let output = buffers[1].as_u64().unwrap() as usize;
        assert_eq!(
            cpu.blobs[source], prepared.input,
            "wheel source remains unchanged between frames"
        );
        assert_eq!(cpu.blobs[output].len(), prepared.input.len());
        frames.push(cpu.blob_f32(output));
    }
    assert_eq!(frames.len(), plan.transforms.len());
    frames
}

fn prepare(plan: &Plan, finite: bool) -> Prepared {
    let test = &plan.test;
    assert!(!plan.transforms.is_empty());
    assert_eq!(plan.transforms.len(), plan.updates.len());
    assert!(plan.updates[0].is_empty());
    assert_eq!(plan.transforms.len(), test.num_retests() + 1);
    let (input, width, height) = if finite {
        assert_eq!(test.values.input_values.len(), ENTRIES);
        let values: Vec<_> = test
            .values
            .input_values
            .iter()
            .map(|&v| if v.is_finite() { v } else { 0.0 })
            .collect();
        (f32_to_bytes(&values), WIDTH, HEIGHT)
    } else {
        let entries = test.comparison_entries().unwrap();
        let (width, height) = comparison_dimensions(test.values.original_input_value_size).unwrap();
        assert_eq!(width * height * 4, entries);
        (
            f32_to_bytes(&test.values.input_values[..entries]),
            width,
            height,
        )
    };
    let mut steps = Vec::new();
    for &binding in &plan.bindings {
        steps.push(json!({"get":binding,"from":"cpu","as":binding}));
    }
    let mut apply_steps = Vec::new();
    for updates in &plan.updates {
        steps.extend(updates.iter().cloned());
        apply_steps.push(steps.len());
        steps.push(json!({"apply":[0,1]}));
    }
    let args = json!({"config":{"create":{"processor_cache":"off"}},
        "transform":plan.transforms[0],
        "buffers":[{"blob":0},{"size":input.len(),"fill":[0]}],
        "images":[{"kind":"packed","data":{"buffer":0},"width":width,"height":height,"num_channels":4},
            {"kind":"packed","data":{"buffer":1},"width":width,"height":height,"num_channels":4}],
        "steps":steps});
    Prepared {
        input,
        args,
        apply_steps,
        steps: steps.len(),
    }
}

struct GpuFrameSteps {
    uniforms: usize,
    properties: Vec<usize>,
}
struct GpuPrepared {
    args: Value,
    frames: Vec<GpuFrameSteps>,
    steps: usize,
}

/// Retain the actual native descriptor through all upstream updates. Its unused
/// Tone coefficients may retain history when an identity reset bypasses them.
fn prepare_gpu(plan: &Plan) -> GpuPrepared {
    assert!(!plan.bindings.is_empty());
    let mut steps = Vec::new();
    for &binding in &plan.bindings {
        steps.push(json!({"get":binding,"from":"gpu","as":binding}));
    }
    let mut frames = Vec::new();
    for updates in &plan.updates {
        steps.extend(updates.iter().cloned());
        let uniforms = steps.len();
        steps.push(json!({"uniforms":true}));
        let mut properties = Vec::new();
        for &binding in &plan.bindings {
            properties.push(steps.len());
            steps.push(json!({"read":binding}));
        }
        frames.push(GpuFrameSteps {
            uniforms,
            properties,
        });
    }
    let args = json!({"config":{"create":{"processor_cache":"off"}},
        "transform":plan.transforms[0], "gpu":{"shader":plan.test.shader_settings.json()}, "steps":steps});
    GpuPrepared {
        args,
        frames,
        steps: steps.len(),
    }
}

fn validate_gpu_stream(response: &Response, plan: &Plan, prepared: &GpuPrepared) {
    assert!(
        response.result.get("exception").is_none(),
        "{}",
        response.result
    );
    assert_eq!(response.result["log"], json!([]));
    assert!(response.result["shader"].is_object());
    let steps = response.result["steps"].as_array().unwrap();
    assert_eq!(steps.len(), prepared.steps);
    assert_eq!(prepared.frames.len(), plan.transforms.len());
    for (index, step) in steps.iter().enumerate() {
        assert!(
            step.get("exception").is_none(),
            "{}/{}: {step}",
            plan.test.group,
            plan.test.name
        );
        assert_eq!(step["log"], json!([]));
        assert!(step["result"].get("unbound").is_none());
        if index < plan.bindings.len() {
            assert_eq!(step["has"], json!(true));
        }
    }
}

/// Transport actual native observations, never calculated uniform expectations.
/// The original GPU processor flags belong to the retained processor. Fresh
/// per-state extraction still checks code/resources/layout and property values.
fn gpu_frame(
    plan: &Plan,
    prepared: &GpuPrepared,
    response: &Response,
    frame: usize,
    original: &Value,
    fresh: &GpuShaderReply,
) -> GpuShaderReply {
    assert_eq!(response.result["gpu_cache_id"], original["gpu_cache_id"]);
    assert_eq!(
        response.result["processor_cache_id"],
        original["processor_cache_id"]
    );
    let observations = response.result["steps"].as_array().unwrap();
    let indices = &prepared.frames[frame];
    let mut result = original.clone();
    result["shader"] = response.result["shader"].clone();
    result["shader"]["uniforms"] = observations[indices.uniforms]["result"].clone();
    assert!(result["shader"]["uniforms"].is_array());
    let properties = result["shader"]["dynamic_properties"]
        .as_array_mut()
        .unwrap();
    assert_eq!(properties.len(), plan.bindings.len());
    for property in properties {
        let matching: Vec<_> = indices
            .properties
            .iter()
            .map(|&index| &observations[index]["result"])
            .filter(|value| value["type"] == property["type"])
            .collect();
        assert_eq!(matching.len(), 1);
        *property = matching[0].clone();
    }
    let actual = GpuShaderReply::from_response(Response {
        result,
        blobs: response.blobs.clone(),
    });
    let a = actual.shader();
    let b = fresh.shader();
    if frame == 0 {
        assert_eq!(a, b, "initial independent wheel extractions");
    }
    assert_eq!(a.cache_id, b.cache_id);
    assert_eq!(a.text, b.text);
    assert_eq!(a.textures, b.textures);
    assert_eq!(a.textures_3d, b.textures_3d);
    assert_eq!(a.getters, b.getters);
    assert_eq!(a.dynamic_properties, b.dynamic_properties);
    assert_eq!(a.uniforms.len(), b.uniforms.len());
    for (a, b) in a.uniforms.iter().zip(&b.uniforms) {
        assert_eq!(
            (&a.name, &a.kind, a.buffer_offset),
            (&b.name, &b.kind, b.buffer_offset)
        );
    }
    actual
}

#[cfg(test)]
mod controls {
    use super::*;

    #[test]
    fn encoded_nonfinite_parameters_cannot_enter_the_companion_scope() {
        // Input-policy controls; no expected OCIO or GPU output is supplied.
        for value in [0.0_f64, -0.0, f64::MAX] {
            assert!(finite_parameters(&json!({"f64": value.to_bits()})));
        }
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(!finite_parameters(&json!({"f64": value.to_bits()})));
        }
        assert!(!finite_parameters(&json!({"f64": true})));
        assert!(!finite_parameters(&json!({"f64": 0, "extra": true})));
    }
}
