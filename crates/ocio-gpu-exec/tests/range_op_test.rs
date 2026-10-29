// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `tests/gpu/RangeOp_test.cpp` @ v2.5.2: all eight cases from the verified table.
//! Actual uncached wheel CPU/shader references; same-renderer shader identity
//! remains bitwise, and only upstream's 1e-6 comparison threshold is used.
#![cfg(target_os = "linux")]
#![forbid(unsafe_code)]

use std::sync::OnceLock;

use ocio::{RangeStyle, RangeTransform, Transform};
use ocio_gpu_exec::gpu_unit_test::{GpuUnitTest, ReferenceRun};
use ocio_testkit::gpu_cases::{RangeCase, range_cases};
use ocio_testkit::upstream::gpu::input_digest;
use serde_json::json;

#[path = "support/suite1_reference.rs"]
mod reference;

// Independent input-only construction: input-digests.py, default wide/NaN/Inf.
const INPUT_DIGEST: &str = "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af";

fn input_test(case: &RangeCase) -> GpuUnitTest {
    let mut test = GpuUnitTest::new("RangeOp", case.name);
    test.comparison.error_threshold = case.error_threshold;
    // Explicit setTestWideRange(true) calls equal GPUUnitTest's default.
    test.test_wide_range = true;
    test.prepare_values().unwrap();
    assert_eq!(
        input_digest(&test.values.input_values),
        INPUT_DIGEST,
        "{}",
        case.name
    );
    assert_eq!(test.num_retests(), 0);
    test
}

fn references() -> &'static [ReferenceRun] {
    static REFERENCES: OnceLock<Vec<ReferenceRun>> = OnceLock::new();
    REFERENCES.get_or_init(|| {
        let cases = range_cases();
        assert_eq!(cases.len(), 8);
        let tests: Vec<_> = cases.iter().map(input_test).collect();
        assert_eq!(tests.iter().filter(|test| test.legacy_shader).count(), 0);
        let processors: Vec<_> = cases
            .iter()
            .map(|case| {
                json!({
                    "config":{"create":{"processor_cache":"off"}}, "transform":case.transform()
                })
            })
            .collect();
        reference::batch(&tests, &processors)
    })
}

/// Port of the test setup in `RangeOp_test.cpp:17-119` @ v2.5.2.
fn run_case(index: usize, name: &str) {
    let cases = range_cases();
    let case = &cases[index];
    assert_eq!(case.name, name);
    let mut range = RangeTransform::new();
    if case.no_clamp {
        range.set_style(RangeStyle::NoClamp);
    }
    // The existing table retains each upstream float literal promoted to double.
    if let Some(value) = case.min_in {
        range.set_min_in_value(value);
    }
    if let Some(value) = case.max_in {
        range.set_max_in_value(value);
    }
    if let Some(value) = case.min_out {
        range.set_min_out_value(value);
    }
    if let Some(value) = case.max_out {
        range.set_max_out_value(value);
    }
    let mut test = input_test(case);
    test.set_processor_transform(&Transform::from(range))
        .unwrap();
    assert!(
        test.run(std::slice::from_ref(&references()[index]))
            .unwrap()
    );
}

/// Port of `OCIO_ADD_GPU_TEST(RangeOp, scale_with_low_and_high_clippings)` @ v2.5.2.
#[test]
fn scale_with_low_and_high_clippings() {
    run_case(0, "scale_with_low_and_high_clippings");
}
/// Port of `OCIO_ADD_GPU_TEST(RangeOp, scale_with_low_clipping)` @ v2.5.2.
#[test]
fn scale_with_low_clipping() {
    run_case(1, "scale_with_low_clipping");
}
/// Port of `OCIO_ADD_GPU_TEST(RangeOp, scale_with_high_clipping)` @ v2.5.2.
#[test]
fn scale_with_high_clipping() {
    run_case(2, "scale_with_high_clipping");
}
/// Port of `OCIO_ADD_GPU_TEST(RangeOp, scale_with_low_and_high_clippings_2)` @ v2.5.2.
#[test]
fn scale_with_low_and_high_clippings_2() {
    run_case(3, "scale_with_low_and_high_clippings_2");
}
/// Port of `OCIO_ADD_GPU_TEST(RangeOp, arbitrary_1)` @ v2.5.2.
#[test]
fn arbitrary_1() {
    run_case(4, "arbitrary_1");
}
/// Port of `OCIO_ADD_GPU_TEST(RangeOp, arbitrary_1_no_clamp)` @ v2.5.2.
#[test]
fn arbitrary_1_no_clamp() {
    run_case(5, "arbitrary_1_no_clamp");
}
/// Port of `OCIO_ADD_GPU_TEST(RangeOp, arbitrary_2)` @ v2.5.2.
#[test]
fn arbitrary_2() {
    run_case(6, "arbitrary_2");
}
/// Port of `OCIO_ADD_GPU_TEST(RangeOp, arbitrary_2_no_clamp)` @ v2.5.2.
#[test]
fn arbitrary_2_no_clamp() {
    run_case(7, "arbitrary_2_no_clamp");
}
