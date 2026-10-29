// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Direct coverage of GPUUnitTest's dynamic retests and the additional
//! same-context wheel-render restoration path. CPU answers are the actual wheel.
#![cfg(target_os = "linux")]
#![forbid(unsafe_code)]

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use ocio::{DynamicPropertyType, ExposureContrastStyle, ExposureContrastTransform, Transform};
use ocio_gpu::GpuShaderDesc;
use ocio_gpu_exec::gpu_unit_test::{GpuUnitTest, ReferenceRun};
use ocio_gpu_exec::{Components, HeadlessApp, UniformUpload};
use ocio_testkit::gpu::GpuShaderReply;
use ocio_testkit::oracle::{BatchCall, f32_to_bytes};
use ocio_testkit::upstream::gpu::input_digest;
use ocio_testkit::{Oracle, assert_f32_bits_eq};
use serde_json::json;

const DYNAMIC_INPUT_DIGEST: &str =
    "ea88cf946d9444bfa680a9b6ca485dba98a6362fe9f286d503d218dd6b75be94";
const RECOVERY_INPUT_DIGEST: &str =
    "ce005188fc76c62ccdbe1fbbb0d1e0c472f8e1b0ff4270775036d9eee7e1f9f2";

#[test]
fn retests_reach_the_actual_extracted_and_decoupled_gpu_property() {
    let mut transform = ExposureContrastTransform::new();
    transform.set_style(ExposureContrastStyle::Linear);
    transform.set_exposure(0.0);
    transform.make_exposure_dynamic();
    let mut test = GpuUnitTest::new("Infrastructure", "extracted GPU property");
    // This additional authored fixture checks callback/CPU behavior on the
    // default-generated finite ramp. Full default NaN/Inf GPU parity at all
    // three exposure states is retained separately in wheel_shader's tests.
    test.test_nan = false;
    test.test_infinity = false;
    test.set_processor_transform(&Transform::from(transform))
        .unwrap();
    test.prepare_values().unwrap();
    assert_eq!(
        input_digest(&test.values.input_values),
        DYNAMIC_INPUT_DIGEST
    );
    let entries = test.comparison_entries().unwrap();
    let input = f32_to_bytes(&test.values.input_values[..entries]);
    let settings = test.shader_settings.clone();
    let mut calls = Vec::new();
    for exposure in [0.0, 1.0, 2.0] {
        let args = json!({"config":{"create":{"processor_cache":"off"}},
            "transform":{"class":"ExposureContrastTransform","calls":[
                ["setStyle",{"enum":"EXPOSURE_CONTRAST_LINEAR"}],
                ["setExposure",exposure],["makeExposureDynamic"]]}});
        calls.push(BatchCall {
            cmd: "cpu_apply",
            args: args.clone(),
            blobs: vec![input.as_slice()],
        });
        let mut gpu = args;
        gpu["shader"] = settings.json();
        calls.push(BatchCall {
            cmd: "gpu_shader",
            args: gpu,
            blobs: Vec::new(),
        });
    }
    let answers = Oracle::get().batch(&calls, false);
    assert_eq!(answers.len(), 6);
    let mut answers = answers.into_iter();
    let mut references = Vec::new();
    for _ in 0..3 {
        let cpu = answers.next().unwrap().expect("actual wheel CPU");
        assert!(cpu.result.get("exception").is_none(), "{}", cpu.result);
        assert_eq!(cpu.blobs.len(), 1);
        assert_eq!(cpu.blobs[0].len(), entries * 4);
        let gpu = GpuShaderReply::from_response(answers.next().unwrap().expect("actual wheel GPU"));
        assert!(gpu.raised().is_none(), "{:?}", gpu.raised());
        assert!(gpu.shader.is_some());
        assert_eq!(gpu.shader().dynamic_properties.len(), 1);
        references.push(ReferenceRun {
            cpu: cpu.blob_f32(0),
            finite_cpu: None,
            gpu,
            shader_settings: settings.clone(),
        });
    }
    assert!(answers.next().is_none());

    let callbacks = Arc::new(AtomicU32::new(0));
    let first_gpu = Arc::new(Mutex::new(None));
    for exposure in [1.0_f64, 2.0] {
        let callbacks = callbacks.clone();
        let first_gpu = first_gpu.clone();
        test.add_retest(move |desc, processor| {
            // Mirrors ECOp_test.cpp:163-185: wait for extraction, then fetch
            // the actual shader's property rather than the processor's copy.
            let cpu = processor
                .dynamic_property(DynamicPropertyType::Exposure)
                .unwrap()
                .as_double()
                .unwrap();
            let gpu = desc
                .dynamic_property_by_type(DynamicPropertyType::Exposure)
                .unwrap()
                .as_double()
                .unwrap();
            assert!(!Arc::ptr_eq(&cpu, &gpu));
            assert_eq!(cpu.get_value().to_bits(), 0.0_f64.to_bits());
            let mut first = first_gpu.lock().unwrap();
            if let Some(previous) = first.as_ref() {
                assert!(
                    Arc::ptr_eq(previous, &gpu),
                    "the extracted property survives every retest"
                );
            } else {
                *first = Some(gpu.clone());
            }
            // Change GPU only: a fallback to the processor's property would
            // leave the shader at exposure zero and fail the actual wheel pixels.
            gpu.set_value(exposure);
            callbacks.fetch_add(1, Ordering::SeqCst);
        });
    }
    assert_eq!(test.num_retests(), 2);
    assert!(test.run(&references).unwrap());
    assert_eq!(callbacks.load(Ordering::SeqCst), 2);
    let retained = test
        .shader_desc()
        .unwrap()
        .dynamic_property_by_type(DynamicPropertyType::Exposure)
        .unwrap()
        .as_double()
        .unwrap();
    assert!(Arc::ptr_eq(
        first_gpu.lock().unwrap().as_ref().unwrap(),
        &retained
    ));
    assert_eq!(retained.get_value().to_bits(), 2.0_f64.to_bits());
}

fn description(declaration: &str, body: &str) -> GpuShaderDesc {
    let mut desc = GpuShaderDesc::default();
    desc.create_shader_text(
        declaration,
        b"",
        b"",
        b"vec4 OCIOMain(vec4 inPixel)\n{\n",
        body,
        b"\n}\n",
    );
    desc
}

#[test]
fn reference_success_and_failure_restore_program_without_extra_getter_calls() {
    let input = [0.25_f32, 0.5, 0.75, 1.0].repeat(32);
    assert_eq!(input_digest(&input), RECOVERY_INPUT_DIGEST);
    let calls = Arc::new(AtomicU32::new(0));
    let observed = calls.clone();
    let mut original = description("uniform float value;", "return vec4(value);");
    original
        .add_uniform_double(
            "value",
            Arc::new(move || {
                observed.fetch_add(1, Ordering::SeqCst);
                0.25
            }),
        )
        .unwrap();
    let mut app = HeadlessApp::new(8, 4).unwrap();
    app.init_image(8, 4, Components::Rgba, &input).unwrap();
    app.create_gl_buffers().unwrap();
    app.reshape(8, 4).unwrap();
    app.set_shader(Arc::new(original)).unwrap();
    calls.store(0, Ordering::SeqCst);
    app.redisplay().unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let uploads = app.uniform_uploads();
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "inspection never invokes a getter"
    );
    assert_eq!(uploads.len(), 1);
    let UniformUpload::Double(value) = &uploads[0] else {
        panic!("double upload")
    };
    assert_eq!(value.to_bits(), 0.25_f64.to_bits());
    assert_f32_bits_eq(
        "original platform uniform",
        &[0.25; 128],
        &app.read_image().unwrap(),
    );

    let reference = description("", "return inPixel;");
    let pixels = app.render_reference_shader(Arc::new(reference)).unwrap();
    assert_f32_bits_eq("reference platform identity", &input, &pixels);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "successful restoration does not upload again"
    );
    app.redisplay().unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_f32_bits_eq(
        "program restored after success",
        &[0.25; 128],
        &app.read_image().unwrap(),
    );

    let broken = description("", "invalid GLSL token sequence;");
    let error = app.render_reference_shader(Arc::new(broken)).unwrap_err();
    assert!(
        error
            .what()
            .starts_with(b"OCIO Shader program compilation failed: ")
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "failed reference restoration does not upload again"
    );
    app.redisplay().unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert_f32_bits_eq(
        "program restored after failure",
        &[0.25; 128],
        &app.read_image().unwrap(),
    );
}
