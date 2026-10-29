// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `tests/gpu/MatrixOp_test.cpp` @ v2.5.2: all ten cases, including eight legacy.
//! Wheel CPU expectations are one uncached batch; shaders are actual ordinary or
//! native-legacy wheel outputs, rendered bit-for-bit on the same GL context.
#![cfg(target_os = "linux")]
#![forbid(unsafe_code)]

use std::sync::OnceLock;

use ocio::{MatrixTransform, Transform, TransformDirection};
use ocio_gpu_exec::gpu_unit_test::{GpuUnitTest, ReferenceRun};
use ocio_testkit::Oracle;
use ocio_testkit::gpu::GpuShaderReply;
use ocio_testkit::gpu_cases::matrix_cases;
use ocio_testkit::oracle::{BatchCall, f32_to_bytes};
use ocio_testkit::upstream::gpu::input_digest;
use serde_json::json;

// Input-only digest: f32 NaN/Inf channel rows followed by min + step * float(i),
// GPUUnitTest.cpp:225-301. Independently constructed with explicit f32 rounding
// and SHA256 before compiling/running this source. No port output is an expected value.
const INPUT_DIGEST: &str = "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af";

fn references() -> &'static [ReferenceRun] {
    static REFERENCES: OnceLock<Vec<ReferenceRun>> = OnceLock::new();
    REFERENCES.get_or_init(|| {
        let cases = matrix_cases();
        assert_eq!(cases.len(), 10);
        assert_eq!(cases.iter().filter(|case| case.legacy_shader).count(), 8);
        let mut template = GpuUnitTest::new("MatrixOps", "input");
        template.prepare_values().unwrap();
        assert_eq!(input_digest(&template.values.input_values), INPUT_DIGEST);
        let entries = template.comparison_entries().unwrap();
        let input = f32_to_bytes(&template.values.input_values[..entries]);
        let settings = template.shader_settings.clone();
        let mut calls = Vec::new();
        for case in &cases {
            let processor = json!({"config":{"create":{"processor_cache":"off"}}, "transform":case.transform()});
            calls.push(BatchCall { cmd: "cpu_apply", args: processor.clone(), blobs: vec![input.as_slice()] });
            let mut gpu = processor;
            gpu["shader"] = settings.json();
            if case.legacy_shader { gpu["edge_len"] = json!(32); }
            calls.push(BatchCall {
                cmd: if case.legacy_shader { "legacy_gpu_shader" } else { "gpu_shader" },
                args: gpu, blobs: Vec::new(),
            });
        }
        let answers = Oracle::get().batch(&calls, false);
        assert_eq!(answers.len(), 2 * cases.len());
        let mut answers = answers.into_iter();
        let mut out = Vec::new();
        for case in &cases {
            let cpu = answers.next().expect("CPU response").expect("wheel CPU request");
            assert!(cpu.result.get("exception").is_none(), "{}: {}", case.name, cpu.result);
            assert_eq!(cpu.blobs.len(), 1, "{}", case.name);
            assert_eq!(cpu.blobs[0].len(), entries * 4, "{}", case.name);
            let gpu = GpuShaderReply::from_response(answers.next().expect("GPU response").expect("wheel GPU request"));
            assert!(gpu.raised().is_none(), "{}: {:?}", case.name, gpu.raised());
            assert!(gpu.shader.is_some(), "{}: {}", case.name, gpu.result);
            assert!(gpu.shader().dynamic_properties.is_empty());
            out.push(ReferenceRun { cpu: cpu.blob_f32(0), gpu, shader_settings: settings.clone() });
        }
        assert!(answers.next().is_none());
        out
    })
}

/// Port of AddMatrixTest (`MatrixOp_test.cpp:16-35` @ v2.5.2).
fn run_case(index: usize) {
    let cases = matrix_cases();
    let case = cases[index];
    let mut matrix = MatrixTransform::new();
    matrix.set_direction(if case.inverse {
        TransformDirection::Inverse
    } else {
        TransformDirection::Forward
    });
    if let Some(values) = case.matrix {
        matrix.set_matrix(&values);
    }
    if let Some(values) = case.offset {
        matrix.set_offset(&values);
    }
    let mut test = GpuUnitTest::new("MatrixOps", case.name);
    test.comparison.error_threshold = case.error_threshold;
    test.set_processor_transform(&Transform::from(matrix))
        .unwrap();
    test.legacy_shader = case.legacy_shader;
    test.prepare_values().unwrap();
    assert_eq!(input_digest(&test.values.input_values), INPUT_DIGEST);
    assert_eq!(test.num_retests(), 0);
    assert!(
        test.run(std::slice::from_ref(&references()[index]))
            .unwrap()
    );
}

/// Port of `OCIO_ADD_GPU_TEST(MatrixOps, matrix)` @ v2.5.2.
#[test]
fn matrix() {
    run_case(0);
}
/// Port of `OCIO_ADD_GPU_TEST(MatrixOps, scale)` @ v2.5.2.
#[test]
fn scale() {
    run_case(1);
}
/// Port of `OCIO_ADD_GPU_TEST(MatrixOps, offset)` @ v2.5.2.
#[test]
fn offset() {
    run_case(2);
}
/// Port of `OCIO_ADD_GPU_TEST(MatrixOps, matrix_offset)` @ v2.5.2.
#[test]
fn matrix_offset() {
    run_case(3);
}
/// Port of `OCIO_ADD_GPU_TEST(MatrixOps, matrix_inverse)` @ v2.5.2.
#[test]
fn matrix_inverse() {
    run_case(4);
}
/// Port of `OCIO_ADD_GPU_TEST(MatrixOps, scale_inverse)` @ v2.5.2.
#[test]
fn scale_inverse() {
    run_case(5);
}
/// Port of `OCIO_ADD_GPU_TEST(MatrixOps, offset_inverse)` @ v2.5.2.
#[test]
fn offset_inverse() {
    run_case(6);
}
/// Port of `OCIO_ADD_GPU_TEST(MatrixOps, matrix_offset_inverse)` @ v2.5.2.
#[test]
fn matrix_offset_inverse() {
    run_case(7);
}
/// Port of `OCIO_ADD_GPU_TEST(MatrixOps, matrix_offset_generic_shader)` @ v2.5.2.
#[test]
fn matrix_offset_generic_shader() {
    run_case(8);
}
/// Port of `OCIO_ADD_GPU_TEST(MatrixOps, matrix_offset_inverse_generic_shader)` @ v2.5.2.
#[test]
fn matrix_offset_inverse_generic_shader() {
    run_case(9);
}
