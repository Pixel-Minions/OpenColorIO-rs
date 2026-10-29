// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Rejection controls exercise the actual harness and uncached wheel responses.
//! Fixture parameters are RangeOp_test.cpp's arbitrary_1_no_clamp @ v2.5.2.
#![cfg(target_os = "linux")]
#![forbid(unsafe_code)]

use ocio::{MatrixTransform, RangeStyle, RangeTransform, Transform};
use ocio_gpu_exec::gpu_unit_test::{GpuUnitTest, ReferenceRun};
use ocio_testkit::gpu::GpuLanguage;
use ocio_testkit::gpu_cases::{matrix_cases, range_cases};
use ocio_testkit::upstream::gpu::ENTRIES;
use serde_json::json;
use std::sync::OnceLock;

#[path = "support/suite1_reference.rs"]
mod reference;

fn fixture() -> GpuUnitTest {
    let case = range_cases()
        .into_iter()
        .find(|c| c.name == "arbitrary_1_no_clamp")
        .unwrap();
    let mut range = RangeTransform::new();
    range.set_style(RangeStyle::NoClamp);
    range.set_min_in_value(case.min_in.unwrap());
    range.set_max_in_value(case.max_in.unwrap());
    range.set_min_out_value(case.min_out.unwrap());
    range.set_max_out_value(case.max_out.unwrap());
    let mut test = GpuUnitTest::new("Infrastructure", "nonfinite rejection fixture");
    test.comparison.error_threshold = case.error_threshold;
    test.set_processor_transform(&Transform::from(range))
        .unwrap();
    test.prepare_values().unwrap();
    test
}

fn actual() -> &'static ReferenceRun {
    static VALUE: OnceLock<ReferenceRun> = OnceLock::new();
    VALUE.get_or_init(|| {
        let case = range_cases()
            .into_iter()
            .find(|c| c.name == "arbitrary_1_no_clamp")
            .unwrap();
        let processor =
            json!({"config":{"create":{"processor_cache":"off"}},"transform":case.transform()});
        reference::batch(&[fixture()], &[processor]).pop().unwrap()
    })
}

#[test]
fn live_baseline_reaches_the_class_with_a_complete_finite_companion() {
    assert!(fixture().run(std::slice::from_ref(actual())).unwrap());
}

#[test]
fn live_reference_error_on_a_finite_footprint_is_rejected() {
    let mut altered = actual().clone();
    altered.cpu[ENTRIES - 1] += 1.0;
    assert!(
        fixture()
            .run(&[altered])
            .unwrap_err()
            .to_string()
            .contains("finite-footprint")
    );
}

#[test]
fn missing_and_corrupted_live_finite_companions_are_rejected() {
    let mut absent = actual().clone();
    absent.finite_cpu = None;
    assert!(
        fixture()
            .run(&[absent])
            .unwrap_err()
            .to_string()
            .contains("missing complete finite companion")
    );
    let mut altered = actual().clone();
    altered.finite_cpu.as_mut().unwrap()[ENTRIES - 1] += 1.0;
    assert!(
        fixture()
            .run(&[altered])
            .unwrap_err()
            .to_string()
            .contains("finite companion comparison")
    );
}

#[test]
fn mismatched_actual_shader_settings_are_rejected() {
    let mut altered = actual().clone();
    altered.shader_settings.language = Some(GpuLanguage::Glsl13);
    assert!(
        fixture()
            .run(&[altered])
            .unwrap_err()
            .to_string()
            .contains("shader settings differ")
    );
}

#[test]
fn mismatched_actual_wheel_shader_text_cannot_use_the_class() {
    let mut altered = actual().clone();
    altered
        .gpu
        .shader
        .as_mut()
        .unwrap()
        .text
        .push_str("\n// changed reference shader text\n");
    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| fixture().run(&[altered])));
    assert!(result.is_err() || result.unwrap().is_err());
}

#[test]
fn registered_matrix_tuples_remain_mandatory_during_transition() {
    let case = matrix_cases()[0];
    let make = || {
        let mut matrix = MatrixTransform::new();
        matrix.set_matrix(&case.matrix.unwrap());
        let mut test = GpuUnitTest::new("MatrixOps", case.name);
        test.comparison.error_threshold = case.error_threshold;
        test.legacy_shader = case.legacy_shader;
        test.set_processor_transform(&Transform::from(matrix))
            .unwrap();
        test.prepare_values().unwrap();
        test
    };
    let processor =
        json!({"config":{"create":{"processor_cache":"off"}},"transform":case.transform()});
    let mut value = reference::batch(&[make()], &[processor]).pop().unwrap();
    assert!(make().run(std::slice::from_ref(&value)).unwrap());
    // A changed recorded CPU tuple still lies in a non-finite footprint and
    // would satisfy the new class alone. The retained signed rule must reject it.
    value.cpu[16] = f32::NEG_INFINITY;
    assert!(
        make()
            .run(&[value])
            .unwrap_err()
            .to_string()
            .contains("Retained legacy GPU rule")
    );
}
