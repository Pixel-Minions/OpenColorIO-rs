// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Regression for the Suite4 CPU reference stream's property ownership.
//! Expected pixels come from independent fresh wheel processors for each state.
#![cfg(target_os = "linux")]
#![forbid(unsafe_code)]

use ocio_gpu_exec::gpu_unit_test::GpuUnitTest;
use ocio_testkit::Oracle;
use ocio_testkit::gpu::GpuShaderReply;
use ocio_testkit::oracle::{BatchCall, f32_to_bytes};
use serde_json::{Value, json};

#[path = "support/suite4_reference.rs"]
mod reference;

#[test]
fn cpu_stream_retests_and_full_finite_companions_match_fresh_wheel_processors() {
    let mut test = GpuUnitTest::new("Infrastructure", "dynamic CPU stream ownership");
    // This contract only asks for references; GPU callback behavior is checked
    // separately by the upstream retest cases and existing harness controls.
    test.add_retest(|_, _| {});
    test.prepare_values().unwrap();
    let original = f32_to_bytes(&test.values.input_values);
    let finite = f32_to_bytes(
        &test
            .values
            .input_values
            .iter()
            .map(|&v| if v.is_finite() { v } else { 0.0 })
            .collect::<Vec<_>>(),
    );
    let transforms: Vec<_> = [0.0_f64, 1.0]
        .into_iter()
        .map(|exposure| {
            json!({"class":"ExposureContrastTransform", "calls":[
            ["setExposure", {"f64":exposure.to_bits()}], ["makeExposureDynamic"]]})
        })
        .collect();
    let plan = reference::Plan {
        test,
        transforms: transforms.clone(),
        bindings: vec!["DYNAMIC_PROPERTY_EXPOSURE"],
        updates: vec![
            vec![],
            vec![json!({"set":"DYNAMIC_PROPERTY_EXPOSURE", "value":{"f64":1.0_f64.to_bits()}})],
        ],
    };
    let references = reference::batch(&[plan]).pop().unwrap();
    assert_eq!(references.len(), 2);
    let mut calls = Vec::new();
    for transform in transforms {
        for input in [&original, &finite] {
            calls.push(BatchCall {
                cmd: "cpu_apply",
                args: json!({"config":{"create":{"processor_cache":"off"}},"transform":transform}),
                blobs: vec![input.as_slice()],
            });
        }
    }
    let answers = Oracle::get().batch(&calls, false);
    assert_eq!(answers.len(), 4);
    let mut answers = answers.into_iter();
    for reference in &references {
        for actual in [
            &reference.cpu,
            reference
                .finite_cpu
                .as_ref()
                .expect("full finite companion"),
        ] {
            let expected = answers
                .next()
                .unwrap()
                .expect("independent fresh wheel CPU");
            assert!(
                expected.result.get("exception").is_none(),
                "{}",
                expected.result
            );
            assert_eq!(expected.result["log"], json!([]));
            assert_eq!(expected.blobs.len(), 1);
            assert_eq!(f32_to_bytes(actual), expected.blobs[0]);
        }
    }
    assert!(answers.next().is_none());
    assert_ne!(
        f32_to_bytes(&references[0].cpu),
        f32_to_bytes(&references[1].cpu)
    );
    assert_ne!(
        f32_to_bytes(references[0].finite_cpu.as_ref().unwrap()),
        f32_to_bytes(references[1].finite_cpu.as_ref().unwrap())
    );
}

fn metadata_plan(class: &str, binding: &'static str, values: &[Value]) -> reference::Plan {
    let mut test = GpuUnitTest::new("Infrastructure", "reused native descriptor metadata");
    for _ in &values[1..] {
        test.add_retest(|_, _| {});
    }
    test.prepare_values().unwrap();
    let transforms = values
        .iter()
        .map(|value| {
            json!({"class":class,
        "args":{"style":{"enum":"GRADING_LOG"}}, "calls":[["makeDynamic"],["setValue",value]]})
        })
        .collect();
    let updates = values
        .iter()
        .enumerate()
        .map(|(frame, value)| {
            if frame == 0 {
                vec![]
            } else {
                vec![json!({"set":binding,"value":value})]
            }
        })
        .collect();
    reference::Plan {
        test,
        transforms,
        bindings: vec![binding],
        updates,
    }
}

fn fresh_shader(transform: Value, settings: Value) -> GpuShaderReply {
    let args = json!({"config":{"create":{"processor_cache":"off"}}, "transform":transform, "shader":settings});
    let answers = Oracle::get().batch(
        &[BatchCall {
            cmd: "gpu_shader",
            args,
            blobs: Vec::new(),
        }],
        false,
    );
    assert_eq!(answers.len(), 1);
    let response = GpuShaderReply::from_response(answers.into_iter().next().unwrap().unwrap());
    assert!(response.raised().is_none());
    assert!(response.shader.is_some());
    assert_eq!(response.result["log"], json!([]));
    response
}

#[test]
fn retest_flags_belong_to_the_original_gpu_processor() {
    let default = json!({"object":{"class":"GradingPrimary", "args":[{"enum":"GRADING_LOG"}]}});
    let initial = json!({"object":{"class":"GradingPrimary", "args":[{"enum":"GRADING_LOG"}],
        "attrs":[["saturation",1.21]]}});
    let plan = metadata_plan(
        "GradingPrimaryTransform",
        "DYNAMIC_PROPERTY_GRADING_PRIMARY",
        &[initial, default],
    );
    let fresh = fresh_shader(plan.transforms[1].clone(), plan.test.shader_settings.json());
    let frames = reference::batch(&[plan]).pop().unwrap();
    assert_eq!(frames.len(), 2);
    let original = &frames[0].gpu.result["gpu_processor"];
    let retained = &frames[1].gpu.result["gpu_processor"];
    assert_eq!(original, retained);
    assert_ne!(
        retained["hasChannelCrosstalk"],
        fresh.result["gpu_processor"]["hasChannelCrosstalk"]
    );
    assert_eq!(
        frames[1].gpu.shader().dynamic_properties,
        fresh.shader().dynamic_properties
    );
}

#[test]
fn tone_identity_reset_uses_the_reused_native_descriptors_uniform_values() {
    // BLACK_LOG inputs, GradingToneOp_test.cpp @ v2.5.2; expected metadata is live.
    let default = json!({"object":{"class":"GradingTone", "args":[{"enum":"GRADING_LOG"}]}});
    let initial = json!({"object":{"class":"GradingTone", "args":[{"enum":"GRADING_LOG"}],
        "attrs":[["blacks",{"object":{"class":"GradingRGBMSW", "args":[0.3,1.0,1.9,0.6,0.8,0.9]}}]]}});
    let plan = metadata_plan(
        "GradingToneTransform",
        "DYNAMIC_PROPERTY_GRADING_TONE",
        &[initial, default],
    );
    let fresh = fresh_shader(plan.transforms[1].clone(), plan.test.shader_settings.json());
    let frames = reference::batch(&[plan]).pop().unwrap();
    assert_eq!(frames.len(), 2);
    let before = frames[0].gpu.shader();
    let reset = frames[1].gpu.shader();
    let uniform = |shader: &ocio_testkit::gpu::Shader| {
        shader
            .uniforms
            .iter()
            .find(|u| u.name.ends_with("blacksStart"))
            .unwrap()
            .value
            .clone()
    };
    assert_eq!(uniform(before), uniform(reset));
    assert_ne!(uniform(reset), uniform(fresh.shader()));
    assert_eq!(reset.text, fresh.shader().text);
    assert_eq!(reset.dynamic_properties, fresh.shader().dynamic_properties);
}
