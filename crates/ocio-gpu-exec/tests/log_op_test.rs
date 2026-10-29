// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `tests/gpu/LogOp_test.cpp` @ v2.5.2: all twenty-two cases, four legacy.
//! Uses the verified SSE2 thresholds and expf(1.0f) platform input from log_cases.
//! Linux retains camera NaN inputs; upstream disables them only under __APPLE__.
#![cfg(target_os = "linux")]
#![forbid(unsafe_code)]

use std::sync::OnceLock;

use ocio::{LogAffineTransform, LogCameraTransform, LogTransform, Transform, TransformDirection};
use ocio_gpu_exec::gpu_unit_test::{GpuUnitTest, ReferenceRun};
use ocio_testkit::gpu_cases::{LogCase, log_cases};
use ocio_testkit::upstream::gpu::input_digest;
use serde_json::json;

#[path = "support/suite1_reference.rs"]
mod reference;

// Independent input-only construction, with source-selected NaN/infinity rows.
const INPUT_DIGEST_NO_INF: &str =
    "d9deebabc1ead056fe845e0b25e81bb194504127aa9ab6d2615f4080ce0a7642";
const INPUT_DIGEST_NEITHER: &str =
    "ea88cf946d9444bfa680a9b6ca485dba98a6362fe9f286d503d218dd6b75be94";

fn input_test(case: &LogCase) -> GpuUnitTest {
    let mut test = GpuUnitTest::new(case.group, case.name);
    test.comparison.error_threshold = case.error_threshold;
    test.comparison.relative = case.relative_comparison;
    test.test_nan = case.test_nan;
    test.test_infinity = case.test_infinity;
    test.legacy_shader = case.legacy_shader;
    test.prepare_values().unwrap();
    assert!(!case.test_infinity);
    let expected = if case.test_nan {
        INPUT_DIGEST_NO_INF
    } else {
        INPUT_DIGEST_NEITHER
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

fn references() -> &'static [ReferenceRun] {
    static REFERENCES: OnceLock<Vec<ReferenceRun>> = OnceLock::new();
    REFERENCES.get_or_init(|| {
        let cases = log_cases();
        assert_eq!(cases.len(), 22);
        assert_eq!(cases.iter().filter(|case| case.legacy_shader).count(), 4);
        let tests: Vec<_> = cases.iter().map(input_test).collect();
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

/// Port of `AddLogTest` and the affine/camera setup blocks
/// (`LogOp_test.cpp:26-353` @ v2.5.2), in their original setter order.
fn run_case(index: usize, group: &str, name: &str) {
    let cases = log_cases();
    let case = &cases[index];
    assert_eq!((case.group, case.name), (group, name));
    let direction = if case.inverse {
        TransformDirection::Inverse
    } else {
        TransformDirection::Forward
    };
    let p = &case.params;
    let transform: Transform = match case.group {
        "LogTransform" => {
            let mut log = LogTransform::new();
            log.set_direction(direction);
            log.set_base(case.base.expect("AddLogTest base"));
            log.into()
        }
        "LogAffineTransform" => {
            let mut log = LogAffineTransform::new();
            log.set_direction(direction);
            if let Some(base) = case.base {
                log.set_base(base);
            }
            if let Some(values) = p.log_side_slope {
                log.set_log_side_slope_value(&values);
            }
            if let Some(values) = p.log_side_offset {
                log.set_log_side_offset_value(&values);
            }
            if let Some(values) = p.lin_side_slope {
                log.set_lin_side_slope_value(&values);
            }
            if let Some(values) = p.lin_side_offset {
                log.set_lin_side_offset_value(&values);
            }
            assert!(p.linear_slope.is_none());
            assert!(case.lin_side_break.is_none());
            log.into()
        }
        "LogCameraTransform" => {
            let mut log =
                LogCameraTransform::new(&case.lin_side_break.expect("camera constructor break"));
            log.set_direction(direction);
            if let Some(base) = case.base {
                log.set_base(base);
            }
            if let Some(values) = p.log_side_slope {
                log.set_log_side_slope_value(&values);
            }
            if let Some(values) = p.log_side_offset {
                log.set_log_side_offset_value(&values);
            }
            if let Some(values) = p.lin_side_slope {
                log.set_lin_side_slope_value(&values);
            }
            if let Some(values) = p.lin_side_offset {
                log.set_lin_side_offset_value(&values);
            }
            if let Some(values) = p.linear_slope {
                log.set_linear_slope_value(&values).unwrap();
            }
            log.into()
        }
        other => panic!("unexpected upstream log class {other}"),
    };
    let mut test = input_test(case);
    test.set_processor_transform(&transform).unwrap();
    assert!(
        test.run(std::slice::from_ref(&references()[index]))
            .unwrap()
    );
}

/// Port of `OCIO_ADD_GPU_TEST(LogTransform, LogBase_10_legacy)` @ v2.5.2.
#[test]
fn log_base_10_legacy() {
    run_case(0, "LogTransform", "LogBase_10_legacy");
}
/// Port of `OCIO_ADD_GPU_TEST(LogTransform, LogBase_10_legacy_inverse)` @ v2.5.2.
#[test]
fn log_base_10_legacy_inverse() {
    run_case(1, "LogTransform", "LogBase_10_legacy_inverse");
}
/// Port of `OCIO_ADD_GPU_TEST(LogTransform, LogBase_10_generic_shader)` @ v2.5.2.
#[test]
fn log_base_10_generic_shader() {
    run_case(2, "LogTransform", "LogBase_10_generic_shader");
}
/// Port of `OCIO_ADD_GPU_TEST(LogTransform, LogBase_10_inverse_generic_shader)` @ v2.5.2.
#[test]
fn log_base_10_inverse_generic_shader() {
    run_case(3, "LogTransform", "LogBase_10_inverse_generic_shader");
}
/// Port of `OCIO_ADD_GPU_TEST(LogTransform, LogBase_euler_legacy)` @ v2.5.2.
#[test]
fn log_base_euler_legacy() {
    run_case(4, "LogTransform", "LogBase_euler_legacy");
}
/// Port of `OCIO_ADD_GPU_TEST(LogTransform, LogBase_euler_legacy_inverse)` @ v2.5.2.
#[test]
fn log_base_euler_legacy_inverse() {
    run_case(5, "LogTransform", "LogBase_euler_legacy_inverse");
}
/// Port of `OCIO_ADD_GPU_TEST(LogTransform, LogBase_euler_generic_shader)` @ v2.5.2.
#[test]
fn log_base_euler_generic_shader() {
    run_case(6, "LogTransform", "LogBase_euler_generic_shader");
}
/// Port of `OCIO_ADD_GPU_TEST(LogTransform, LogBase_euler_inverse_generic_shader)` @ v2.5.2.
#[test]
fn log_base_euler_inverse_generic_shader() {
    run_case(7, "LogTransform", "LogBase_euler_inverse_generic_shader");
}
/// Port of `OCIO_ADD_GPU_TEST(LogAffineTransform, base)` @ v2.5.2.
#[test]
fn affine_base() {
    run_case(8, "LogAffineTransform", "base");
}
/// Port of `OCIO_ADD_GPU_TEST(LogAffineTransform, base_inverse)` @ v2.5.2.
#[test]
fn affine_base_inverse() {
    run_case(9, "LogAffineTransform", "base_inverse");
}
/// Port of `OCIO_ADD_GPU_TEST(LogAffineTransform, linSideSlope)` @ v2.5.2.
#[test]
fn affine_lin_side_slope() {
    run_case(10, "LogAffineTransform", "linSideSlope");
}
/// Port of `OCIO_ADD_GPU_TEST(LogAffineTransform, linSideSlope_inverse)` @ v2.5.2.
#[test]
fn affine_lin_side_slope_inverse() {
    run_case(11, "LogAffineTransform", "linSideSlope_inverse");
}
/// Port of `OCIO_ADD_GPU_TEST(LogAffineTransform, linSideOffset)` @ v2.5.2.
#[test]
fn affine_lin_side_offset() {
    run_case(12, "LogAffineTransform", "linSideOffset");
}
/// Port of `OCIO_ADD_GPU_TEST(LogAffineTransform, linSideOffset_inverse)` @ v2.5.2.
#[test]
fn affine_lin_side_offset_inverse() {
    run_case(13, "LogAffineTransform", "linSideOffset_inverse");
}
/// Port of `OCIO_ADD_GPU_TEST(LogAffineTransform, logSideSlope)` @ v2.5.2.
#[test]
fn affine_log_side_slope() {
    run_case(14, "LogAffineTransform", "logSideSlope");
}
/// Port of `OCIO_ADD_GPU_TEST(LogAffineTransform, logSideSlope_inverse)` @ v2.5.2.
#[test]
fn affine_log_side_slope_inverse() {
    run_case(15, "LogAffineTransform", "logSideSlope_inverse");
}
/// Port of `OCIO_ADD_GPU_TEST(LogAffineTransform, logSideOffset)` @ v2.5.2.
#[test]
fn affine_log_side_offset() {
    run_case(16, "LogAffineTransform", "logSideOffset");
}
/// Port of `OCIO_ADD_GPU_TEST(LogAffineTransform, logSideOffset_inverse)` @ v2.5.2.
#[test]
fn affine_log_side_offset_inverse() {
    run_case(17, "LogAffineTransform", "logSideOffset_inverse");
}
/// Port of `OCIO_ADD_GPU_TEST(LogAffineTransform, lin2log)` @ v2.5.2.
#[test]
fn affine_lin2log() {
    run_case(18, "LogAffineTransform", "lin2log");
}
/// Port of `OCIO_ADD_GPU_TEST(LogAffineTransform, log2lin)` @ v2.5.2.
#[test]
fn affine_log2lin() {
    run_case(19, "LogAffineTransform", "log2lin");
}
/// Port of `OCIO_ADD_GPU_TEST(LogCameraTransform, camera_lin2log)` @ v2.5.2.
#[test]
fn camera_lin2log() {
    run_case(20, "LogCameraTransform", "camera_lin2log");
}
/// Port of `OCIO_ADD_GPU_TEST(LogCameraTransform, camera_log2lin)` @ v2.5.2.
#[test]
fn camera_log2lin() {
    run_case(21, "LogCameraTransform", "camera_log2lin");
}
