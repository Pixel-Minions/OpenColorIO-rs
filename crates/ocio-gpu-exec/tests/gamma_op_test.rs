// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `tests/gpu/GammaOp_test.cpp` @ v2.5.2: fourteen verified cases, two legacy.
//! SSE2 thresholds are upstream's selected branch for both pinned wheels.
//! Needs the actual reviewed/committed create.major_version oracle option.
#![cfg(target_os = "linux")]
#![forbid(unsafe_code)]

use std::sync::OnceLock;

use ocio::{
    Config, ExponentTransform, ExponentWithLinearTransform, NegativeStyle, Transform,
    TransformDirection,
};
use ocio_gpu_exec::gpu_unit_test::{GpuUnitTest, ReferenceRun};
use ocio_testkit::gpu_cases::{GammaCase, GammaTransform, gamma_cases};
use ocio_testkit::upstream::gpu::input_digest;
use serde_json::{Value, json};

#[path = "support/suite1_reference.rs"]
mod reference;

// All four upstream special-row variants, independently built from inputs only.
const INPUT_DIGEST_BOTH: &str = "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af";
const INPUT_DIGEST_NO_NAN: &str =
    "d42def2ca316f1777ee68a7b78242178bd7a67b6d7cd1188f9ac79ac7ea3d489";
const INPUT_DIGEST_NO_INF: &str =
    "d9deebabc1ead056fe845e0b25e81bb194504127aa9ab6d2615f4080ce0a7642";
const INPUT_DIGEST_NEITHER: &str =
    "ea88cf946d9444bfa680a9b6ca485dba98a6362fe9f286d503d218dd6b75be94";

fn input_test(case: &GammaCase) -> GpuUnitTest {
    let mut test = GpuUnitTest::new(case.group, case.name);
    test.comparison.error_threshold = case.error_threshold;
    test.test_nan = case.test_nan;
    test.test_infinity = case.test_infinity;
    test.legacy_shader = case.legacy_shader;
    test.prepare_values().unwrap();
    let expected = match (case.test_nan, case.test_infinity) {
        (true, true) => INPUT_DIGEST_BOTH,
        (false, true) => INPUT_DIGEST_NO_NAN,
        (true, false) => INPUT_DIGEST_NO_INF,
        (false, false) => INPUT_DIGEST_NEITHER,
    };
    assert_eq!(
        input_digest(&test.values.input_values),
        expected,
        "{}/{}",
        case.group,
        case.name
    );
    assert_eq!(test.num_retests(), 0);
    test
}

fn processor_spec(case: &GammaCase) -> Value {
    // AddExponent sets major 1/2; AddExponentWithLinear never sets it. Default
    // processor caching is ON for both config/transform overloads upstream.
    let create = match case.transform {
        GammaTransform::Exponent { version, .. } => json!({"major_version":version}),
        GammaTransform::ExponentWithLinear { .. } => json!({}),
    };
    json!({"config":{"create":create}, "transform":case.transform()})
}

fn references() -> &'static [ReferenceRun] {
    static REFERENCES: OnceLock<Vec<ReferenceRun>> = OnceLock::new();
    REFERENCES.get_or_init(|| {
        let cases = gamma_cases();
        assert_eq!(cases.len(), 14);
        assert_eq!(cases.iter().filter(|case| case.legacy_shader).count(), 2);
        let tests: Vec<_> = cases.iter().map(input_test).collect();
        let processors: Vec<_> = cases.iter().map(processor_spec).collect();
        reference::batch(&tests, &processors)
    })
}

/// Port of `AddExponent` and `AddExponentWithLinear`
/// (`GammaOp_test.cpp:21-58` @ v2.5.2), using their real config/transform overload.
fn run_case(index: usize, group: &str, name: &str) {
    let cases = gamma_cases();
    let case = &cases[index];
    assert_eq!((case.group, case.name), (group, name));
    let direction = if case.inverse {
        TransformDirection::Inverse
    } else {
        TransformDirection::Forward
    };
    let style = match case.negative_style {
        "NEGATIVE_CLAMP" => NegativeStyle::Clamp,
        "NEGATIVE_MIRROR" => NegativeStyle::Mirror,
        "NEGATIVE_PASS_THRU" => NegativeStyle::PassThru,
        "NEGATIVE_LINEAR" => NegativeStyle::Linear,
        other => panic!("unexpected upstream negative style {other}"),
    };
    let (transform, version): (Transform, Option<u32>) = match case.transform {
        GammaTransform::Exponent { value, version } => {
            let mut exp = ExponentTransform::new();
            exp.set_negative_style(style).unwrap();
            exp.set_direction(direction);
            exp.set_value(&value);
            (exp.into(), Some(version))
        }
        GammaTransform::ExponentWithLinear { gamma, offset } => {
            let mut exp = ExponentWithLinearTransform::new();
            exp.set_direction(direction);
            exp.set_gamma(&gamma);
            exp.set_offset(&offset);
            exp.set_negative_style(style).unwrap();
            (exp.into(), None)
        }
    };
    let mut config = Config::new().unwrap();
    // setMajorVersion selects the actual last supported minor: 1.0 or 2.5.
    if let Some(version) = version {
        config.set_major_version(version).unwrap();
    }
    let mut test = input_test(case);
    test.set_processor_config(&config, &transform).unwrap();
    assert!(
        test.run(std::slice::from_ref(&references()[index]))
            .unwrap()
    );
}

/// Port of `OCIO_ADD_GPU_TEST(ExponentOp, legacy_shader_v1)` @ v2.5.2.
#[test]
fn exponent_legacy_shader_v1() {
    run_case(0, "ExponentOp", "legacy_shader_v1");
}
/// Port of `OCIO_ADD_GPU_TEST(ExponentOp, forward_v1)` @ v2.5.2.
#[test]
fn exponent_forward_v1() {
    run_case(1, "ExponentOp", "forward_v1");
}
/// Port of `OCIO_ADD_GPU_TEST(ExponentOp, forward)` @ v2.5.2.
#[test]
fn exponent_forward() {
    run_case(2, "ExponentOp", "forward");
}
/// Port of `OCIO_ADD_GPU_TEST(ExponentOp, forward_mirror)` @ v2.5.2.
#[test]
fn exponent_forward_mirror() {
    run_case(3, "ExponentOp", "forward_mirror");
}
/// Port of `OCIO_ADD_GPU_TEST(ExponentOp, forward_pass_thru)` @ v2.5.2.
#[test]
fn exponent_forward_pass_thru() {
    run_case(4, "ExponentOp", "forward_pass_thru");
}
/// Port of `OCIO_ADD_GPU_TEST(ExponentOp, inverse_legacy_shader_v1)` @ v2.5.2.
#[test]
fn exponent_inverse_legacy_shader_v1() {
    run_case(5, "ExponentOp", "inverse_legacy_shader_v1");
}
/// Port of `OCIO_ADD_GPU_TEST(ExponentOp, inverse_v1)` @ v2.5.2.
#[test]
fn exponent_inverse_v1() {
    run_case(6, "ExponentOp", "inverse_v1");
}
/// Port of `OCIO_ADD_GPU_TEST(ExponentOp, inverse)` @ v2.5.2.
#[test]
fn exponent_inverse() {
    run_case(7, "ExponentOp", "inverse");
}
/// Port of `OCIO_ADD_GPU_TEST(ExponentOp, inverse_mirror)` @ v2.5.2.
#[test]
fn exponent_inverse_mirror() {
    run_case(8, "ExponentOp", "inverse_mirror");
}
/// Port of `OCIO_ADD_GPU_TEST(ExponentOp, inverse_pass_thru)` @ v2.5.2.
#[test]
fn exponent_inverse_pass_thru() {
    run_case(9, "ExponentOp", "inverse_pass_thru");
}
/// Port of `OCIO_ADD_GPU_TEST(ExponentWithLinearOp, forward)` @ v2.5.2.
#[test]
fn exponent_with_linear_forward() {
    run_case(10, "ExponentWithLinearOp", "forward");
}
/// Port of `OCIO_ADD_GPU_TEST(ExponentWithLinearOp, mirror_forward)` @ v2.5.2.
#[test]
fn exponent_with_linear_mirror_forward() {
    run_case(11, "ExponentWithLinearOp", "mirror_forward");
}
/// Port of `OCIO_ADD_GPU_TEST(ExponentWithLinearOp, inverse)` @ v2.5.2.
#[test]
fn exponent_with_linear_inverse() {
    run_case(12, "ExponentWithLinearOp", "inverse");
}
/// Port of `OCIO_ADD_GPU_TEST(ExponentWithLinearOp, mirror_inverse)` @ v2.5.2.
#[test]
fn exponent_with_linear_mirror_inverse() {
    run_case(13, "ExponentWithLinearOp", "mirror_inverse");
}
