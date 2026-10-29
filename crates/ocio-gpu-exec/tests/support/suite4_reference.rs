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
    let mut calls = Vec::new();
    for ((plan, original), companion) in plans.iter().zip(&prepared).zip(&finite) {
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
    for ((plan, original), companion) in plans.iter().zip(&prepared).zip(&finite) {
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
