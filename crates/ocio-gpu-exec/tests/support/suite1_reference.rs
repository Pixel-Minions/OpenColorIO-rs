// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Live per-file wheel batch for the GPU suite, following MatrixOp's reviewed runner.
//! These helpers only transport actual CPU/shader results. GpuUnitTest owns the
//! exact wheel-shader/same-context comparison and upstream numerical checks.

use ocio_gpu_exec::gpu_unit_test::{GpuUnitTest, ReferenceRun};
use ocio_testkit::Oracle;
use ocio_testkit::gpu::GpuShaderReply;
use ocio_testkit::oracle::{BatchCall, f32_to_bytes};
use serde_json::{Value, json};

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
        Value::Object(m) => m.iter().all(|(key, value)| {
            !key.to_ascii_lowercase().contains("bits") && finite_parameters(value)
        }),
    }
}

pub(super) fn batch(tests: &[GpuUnitTest], processors: &[Value]) -> Vec<ReferenceRun> {
    assert!(!tests.is_empty());
    assert_eq!(tests.len(), processors.len());
    let entries: Vec<_> = tests
        .iter()
        .map(|test| test.comparison_entries().unwrap())
        .collect();
    let inputs: Vec<_> = tests
        .iter()
        .zip(&entries)
        .map(|(test, &entries)| f32_to_bytes(&test.values.input_values[..entries]))
        .collect();
    let finite_inputs: Vec<_> = tests
        .iter()
        .zip(processors)
        .map(|(test, processor)| {
            if test.values.input_values.iter().all(|v| v.is_finite()) {
                return None;
            }
            // Class approval covers non-finite image data, not NaN parameters.
            assert!(
                finite_parameters(processor),
                "unverified companion parameters"
            );
            Some(f32_to_bytes(
                &test
                    .values
                    .input_values
                    .iter()
                    .map(|&v| if v.is_finite() { v } else { 0.0 })
                    .collect::<Vec<_>>(),
            ))
        })
        .collect();
    let mut calls = Vec::new();
    for (((test, processor), input), finite) in tests
        .iter()
        .zip(processors)
        .zip(&inputs)
        .zip(&finite_inputs)
    {
        calls.push(BatchCall {
            cmd: "cpu_apply",
            args: processor.clone(),
            blobs: vec![input.as_slice()],
        });
        let mut gpu = processor.clone();
        gpu["shader"] = test.shader_settings.json();
        if test.legacy_shader {
            gpu["edge_len"] = json!(test.legacy_shader_lut_edge);
        }
        calls.push(BatchCall {
            cmd: if test.legacy_shader {
                "legacy_gpu_shader"
            } else {
                "gpu_shader"
            },
            args: gpu,
            blobs: Vec::new(),
        });
        if let Some(finite) = finite {
            calls.push(BatchCall {
                cmd: "cpu_apply",
                args: processor.clone(),
                blobs: vec![finite.as_slice()],
            });
        }
    }
    // Explicitly bypass response caching, as every pixel reference must be live.
    let answers = Oracle::get().batch(&calls, false);
    assert_eq!(
        answers.len(),
        tests.len() * 2 + finite_inputs.iter().filter(|v| v.is_some()).count()
    );
    let mut answers = answers.into_iter();
    let mut out = Vec::new();
    for ((test, &entries), finite_input) in tests.iter().zip(&entries).zip(&finite_inputs) {
        let cpu = answers
            .next()
            .expect("CPU response")
            .expect("live wheel CPU request");
        assert!(
            cpu.result.get("exception").is_none(),
            "{}/{}: {}",
            test.group,
            test.name,
            cpu.result
        );
        assert_eq!(
            cpu.result["log"],
            json!([]),
            "{}/{}: {}",
            test.group,
            test.name,
            cpu.result
        );
        assert_eq!(cpu.blobs.len(), 1, "{}/{}", test.group, test.name);
        assert_eq!(
            cpu.blobs[0].len(),
            entries * 4,
            "{}/{}",
            test.group,
            test.name
        );
        let gpu = GpuShaderReply::from_response(
            answers
                .next()
                .expect("GPU response")
                .expect("live wheel GPU request"),
        );
        assert!(
            gpu.raised().is_none(),
            "{}/{}: {:?}",
            test.group,
            test.name,
            gpu.raised()
        );
        assert!(
            gpu.shader.is_some(),
            "{}/{}: {}",
            test.group,
            test.name,
            gpu.result
        );
        assert_eq!(
            gpu.result["log"],
            json!([]),
            "{}/{}: {}",
            test.group,
            test.name,
            gpu.result
        );
        assert!(
            gpu.shader().dynamic_properties.is_empty(),
            "{}/{}",
            test.group,
            test.name
        );
        let finite_cpu = finite_input.as_ref().map(|input| {
            let answer = answers
                .next()
                .expect("finite CPU response")
                .expect("live wheel finite CPU request");
            assert!(
                answer.result.get("exception").is_none(),
                "{}",
                answer.result
            );
            assert_eq!(answer.result["log"], json!([]));
            assert_eq!(answer.blobs.len(), 1);
            assert_eq!(answer.blobs[0].len(), input.len());
            assert_eq!(input.len(), ocio_testkit::upstream::gpu::ENTRIES * 4);
            answer.blob_f32(0)
        });
        out.push(ReferenceRun {
            cpu: cpu.blob_f32(0),
            finite_cpu,
            gpu,
            shader_settings: test.shader_settings.clone(),
        });
    }
    assert!(answers.next().is_none());
    assert_eq!(out.len(), tests.len());
    out
}
