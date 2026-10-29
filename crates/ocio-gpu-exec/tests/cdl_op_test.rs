// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `tests/gpu/CDLOp_test.cpp` @ v2.5.2: all fifteen cases, including two legacy.
//! The verified table retains each source threshold, range and special setting.
//! Needs the actual reviewed/committed create.major_version oracle option.
#![cfg(target_os = "linux")]
#![forbid(unsafe_code)]

use std::sync::OnceLock;

use ocio::{CdlStyle, CdlTransform, Config, Transform, TransformDirection};
use ocio_gpu_exec::gpu_unit_test::{GpuUnitTest, ReferenceRun};
use ocio_testkit::gpu_cases::{CdlCase, cdl_cases};
use ocio_testkit::upstream::gpu::input_digest;
use serde_json::{Value, json};

#[path = "support/suite1_reference.rs"]
mod reference;

// Independent input-only construction, including source Data_3's [0,1] ramp.
const INPUT_DIGEST_BOTH: &str = "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af";
const INPUT_DIGEST_NO_NAN: &str =
    "d42def2ca316f1777ee68a7b78242178bd7a67b6d7cd1188f9ac79ac7ea3d489";
const INPUT_DIGEST_NEITHER: &str =
    "ea88cf946d9444bfa680a9b6ca485dba98a6362fe9f286d503d218dd6b75be94";
const INPUT_DIGEST_UNIT_RANGE: &str =
    "394608a63bca0842914575c2b977347e75db40ea3f5f085a2b788687effaf4d2";

fn input_test(case: &CdlCase) -> GpuUnitTest {
    let mut test = GpuUnitTest::new("CDLOp", case.name);
    test.test_wide_range = case.test_wide_range;
    test.comparison.relative = false;
    test.comparison.error_threshold = case.error_threshold;
    test.legacy_shader = case.legacy_shader;
    test.test_nan = case.test_nan;
    test.test_infinity = case.test_infinity;
    test.prepare_values().unwrap();
    let expected = match (case.test_wide_range, case.test_nan, case.test_infinity) {
        (true, true, true) => INPUT_DIGEST_BOTH,
        (true, false, true) => INPUT_DIGEST_NO_NAN,
        (true, false, false) => INPUT_DIGEST_NEITHER,
        (false, false, false) => INPUT_DIGEST_UNIT_RANGE,
        other => panic!("unexpected upstream CDL inputs {other:?}"),
    };
    assert_eq!(
        input_digest(&test.values.input_values),
        expected,
        "{}",
        case.name
    );
    assert_eq!(test.num_retests(), 0);
    test
}

fn processor_spec(case: &CdlCase) -> Value {
    // Only v1 cases call setMajorVersion; v2 cases use Config::Create defaults.
    // All use the config/transform overload, retaining default cache ON.
    let create = match case.version {
        1 => json!({"major_version":1}),
        2 => json!({}),
        other => panic!("unexpected upstream CDL config version {other}"),
    };
    json!({"config":{"create":create}, "transform":case.transform()})
}

fn references() -> &'static [ReferenceRun] {
    static REFERENCES: OnceLock<Vec<ReferenceRun>> = OnceLock::new();
    REFERENCES.get_or_init(|| {
        let cases = cdl_cases();
        assert_eq!(cases.len(), 15);
        assert_eq!(cases.iter().filter(|case| case.legacy_shader).count(), 2);
        let tests: Vec<_> = cases.iter().map(input_test).collect();
        let processors: Vec<_> = cases.iter().map(processor_spec).collect();
        reference::batch(&tests, &processors)
    })
}

/// Port of the CDL test setups (`CDLOp_test.cpp:27-352` @ v2.5.2).
fn run_case(index: usize, name: &str) {
    let cases = cdl_cases();
    let case = &cases[index];
    assert_eq!(case.name, name);
    let mut cdl = CdlTransform::new();
    if let Some(style) = case.style {
        cdl.set_style(match style {
            "CDL_ASC" => CdlStyle::Asc,
            "CDL_NO_CLAMP" => CdlStyle::NoClamp,
            other => panic!("unexpected upstream CDL style {other}"),
        });
    }
    cdl.set_direction(if case.inverse {
        TransformDirection::Inverse
    } else {
        TransformDirection::Forward
    });
    cdl.set_slope(&case.params.slope);
    cdl.set_offset(&case.params.offset);
    cdl.set_power(&case.params.power);
    if let Some(saturation) = case.params.sat {
        cdl.set_sat(saturation);
    }
    let mut config = Config::new().unwrap();
    if case.version == 1 {
        config.set_major_version(1).unwrap();
    }
    let mut test = input_test(case);
    test.set_processor_config(&config, &Transform::from(cdl))
        .unwrap();
    assert!(
        test.run(std::slice::from_ref(&references()[index]))
            .unwrap()
    );
}

/// Port of `OCIO_ADD_GPU_TEST(CDLOp, clamp_fwd_v1_legacy_shader)` @ v2.5.2.
#[test]
fn clamp_fwd_v1_legacy_shader() {
    run_case(0, "clamp_fwd_v1_legacy_shader");
}
/// Port of `OCIO_ADD_GPU_TEST(CDLOp, clamp_fwd_v1)` @ v2.5.2.
#[test]
fn clamp_fwd_v1() {
    run_case(1, "clamp_fwd_v1");
}
/// Port of `OCIO_ADD_GPU_TEST(CDLOp, clamp_fwd_v2)` @ v2.5.2.
#[test]
fn clamp_fwd_v2() {
    run_case(2, "clamp_fwd_v2");
}
/// Port of `OCIO_ADD_GPU_TEST(CDLOp, clamp_fwd_no_clamp_v2)` @ v2.5.2.
#[test]
fn clamp_fwd_no_clamp_v2() {
    run_case(3, "clamp_fwd_no_clamp_v2");
}
/// Port of `OCIO_ADD_GPU_TEST(CDLOp, clamp_inv_v2)` @ v2.5.2.
#[test]
fn clamp_inv_v2() {
    run_case(4, "clamp_inv_v2");
}
/// Port of `OCIO_ADD_GPU_TEST(CDLOp, clamp_inv_no_clamp_v2)` @ v2.5.2.
#[test]
fn clamp_inv_no_clamp_v2() {
    run_case(5, "clamp_inv_no_clamp_v2");
}
/// Port of `OCIO_ADD_GPU_TEST(CDLOp, clamp_fwd_v1_legacy_shader_Data_2)` @ v2.5.2.
#[test]
fn clamp_fwd_v1_legacy_shader_data_2() {
    run_case(6, "clamp_fwd_v1_legacy_shader_Data_2");
}
/// Port of `OCIO_ADD_GPU_TEST(CDLOp, clamp_fwd_v1_Data_2)` @ v2.5.2.
#[test]
fn clamp_fwd_v1_data_2() {
    run_case(7, "clamp_fwd_v1_Data_2");
}
/// Port of `OCIO_ADD_GPU_TEST(CDLOp, clamp_fwd_v2_Data_2)` @ v2.5.2.
#[test]
fn clamp_fwd_v2_data_2() {
    run_case(8, "clamp_fwd_v2_Data_2");
}
/// Port of `OCIO_ADD_GPU_TEST(CDLOp, clamp_inv_v2_Data_2)` @ v2.5.2.
#[test]
fn clamp_inv_v2_data_2() {
    run_case(9, "clamp_inv_v2_Data_2");
}
/// Port of `OCIO_ADD_GPU_TEST(CDLOp, clamp_fwd_no_clamp_v2_Data_2)` @ v2.5.2.
#[test]
fn clamp_fwd_no_clamp_v2_data_2() {
    run_case(10, "clamp_fwd_no_clamp_v2_Data_2");
}
/// Port of `OCIO_ADD_GPU_TEST(CDLOp, clamp_inv_no_clamp_v2_Data_2)` @ v2.5.2.
#[test]
fn clamp_inv_no_clamp_v2_data_2() {
    run_case(11, "clamp_inv_no_clamp_v2_Data_2");
}
/// Port of `OCIO_ADD_GPU_TEST(CDLOp, clamp_fwd_v2_Data_3)` @ v2.5.2.
#[test]
fn clamp_fwd_v2_data_3() {
    run_case(12, "clamp_fwd_v2_Data_3");
}
/// Port of `OCIO_ADD_GPU_TEST(CDLOp, clamp_fwd_no_clamp_v2_Data_3)` @ v2.5.2.
#[test]
fn clamp_fwd_no_clamp_v2_data_3() {
    run_case(13, "clamp_fwd_no_clamp_v2_Data_3");
}
/// Port of `OCIO_ADD_GPU_TEST(CDLOp, clamp_inv_no_clamp_v2_Data_3)` @ v2.5.2.
#[test]
fn clamp_inv_no_clamp_v2_data_3() {
    run_case(14, "clamp_inv_no_clamp_v2_Data_3");
}
