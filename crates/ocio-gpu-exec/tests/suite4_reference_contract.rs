// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Regression for the Suite4 CPU reference stream's property ownership.
//! Expected pixels come from independent fresh wheel processors for each state.
#![cfg(target_os = "linux")]
#![forbid(unsafe_code)]

use ocio_gpu_exec::gpu_unit_test::GpuUnitTest;
use ocio_testkit::Oracle;
use ocio_testkit::oracle::{BatchCall, f32_to_bytes};
use serde_json::json;

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
