// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Ported `tests/cpu/ops/log/LogOpCPU_tests.cpp` @ v2.5.2.
//!
//! The wheel is built with `OCIO_USE_SSE2`, so the ports keep the `#if OCIO_USE_SSE2` branches.
//! `log2lin_test` and `lin2log_test` need `LogUtil::ConvertLogParameters` (WP 1.3l1) and are not
//! ported yet.

use super::*;
use crate::open_color_types::TransformDirection;
use ocio_testkit::upstream::{check_close, equal_with_safe_rel_error};

const QNAN: f32 = f32::NAN;
const INF: f32 = f32::INFINITY;

/// The input image of the log and anti-log tests (tests/cpu/ops/log/LogOpCPU_tests.cpp:17-24
/// @ v2.5.2).
const RGBA_IMAGE: [f32; 32] = [
    0.0367126f32,
    0.5f32,
    1.0f32,
    0.0f32, //
    0.2f32,
    0.0f32,
    0.99f32,
    128.0f32, //
    QNAN,
    QNAN,
    QNAN,
    0.0f32, //
    0.0f32,
    0.0f32,
    0.0f32,
    QNAN, //
    INF,
    INF,
    INF,
    0.0f32, //
    0.0f32,
    0.0f32,
    0.0f32,
    INF, //
    -INF,
    -INF,
    -INF,
    0.0f32, //
    0.0f32,
    0.0f32,
    0.0f32,
    -INF,
];

/// Applies the renderer the way the C++ tests do (`pRenderer->apply(rgbaImage, rgba, n)`), in
/// place on a copy.
fn apply(renderer: &dyn CpuOp, image: &[f32]) -> Vec<f32> {
    let mut rgba = image.to_vec();
    renderer.apply(&mut rgba);
    rgba
}

/// `TestLog` (tests/cpu/ops/log/LogOpCPU_tests.cpp:15-99 @ v2.5.2).
fn test_log(log_base: f32) {
    let log_op = LogOpData::new(f64::from(log_base), TransformDirection::Forward);

    let renderer = get_log_renderer(&log_op, true);
    let rgba = apply(renderer.as_ref(), &RGBA_IMAGE);

    let min_value = f32::MIN_POSITIVE;

    // LogOpCPU implementation uses optimized logarithm approximation cannot use strict
    // comparison.
    let error = 5e-5f32;

    for i in 0..8 {
        let is_alpha = i % 4 == 3;

        let result = rgba[i];
        let mut expected = RGBA_IMAGE[i];
        if !is_alpha {
            expected = std_max(min_value, expected).ln() / log_base.ln();
        }

        check_close(result, expected, error);
    }

    let res_min = min_value.ln() / log_base.ln();

    // Evaluating output for input rgbaImage[8-11] = {qnan, qnan, qnan, 0.}.
    check_close(rgba[8], res_min, error);
    assert_eq!(rgba[11], 0.0f32);

    // Evaluating output for input rgbaImage[12-15] = {0., 0., 0., qnan.}.
    check_close(rgba[12], res_min, error);
    assert!(rgba[15].is_nan());

    // SSE implementation of sseLog2 & sseExp2 do not behave like CPU.
    // Evaluating output for input rgbaImage[16-19] = {inf, inf, inf, 0.}.
    if log_base == 10.0f32 {
        check_close(rgba[16], 38.53184509f32, error);
    } else if log_base == 2.0f32 {
        check_close(rgba[16], 128.0000153f32, error);
    }
    assert_eq!(rgba[19], 0.0f32);

    // Evaluating output for input rgbaImage[20-23] = {0., 0., 0., inf}.
    check_close(rgba[20], res_min, error);
    assert_eq!(rgba[23], INF);

    // Evaluating output for input rgbaImage[24-27] = {-inf, -inf, -inf, 0.}.
    check_close(rgba[24], res_min, error);
    assert_eq!(rgba[27], 0.0f32);

    // Evaluating output for input rgbaImage[28-31] = {0., 0.,  0., -inf}.
    check_close(rgba[28], res_min, error);
    assert_eq!(rgba[31], -INF);
}

/// Port of `OCIO_ADD_TEST(LogOpCPU, log_test)` @ v2.5.2.
#[test]
fn log_test() {
    // Log base 10 case, no scaling.
    test_log(10.0f32);

    // Log base 2 case, no scaling.
    test_log(2.0f32);
}

/// `TestAntiLog` (tests/cpu/ops/log/LogOpCPU_tests.cpp:110-174 @ v2.5.2).
fn test_anti_log(log_base: f32) {
    let log_op = LogOpData::new(f64::from(log_base), TransformDirection::Inverse);

    let renderer = get_log_renderer(&log_op, true);
    let rgba = apply(renderer.as_ref(), &RGBA_IMAGE);

    // Relative error tolerance for the log2 approximation.
    let rtol = 2.0f32.powf(-14.0f32);

    for i in 0..8 {
        let is_alpha = i % 4 == 3;

        let result = rgba[i];
        let mut expected = RGBA_IMAGE[i];
        if !is_alpha {
            expected = log_base.powf(expected);
        }

        // LogOpCPU implementation uses optimized logarithm approximation cannot use strict
        // comparison.
        assert!(
            equal_with_safe_rel_error(result, expected, rtol, 1.0f32),
            "[{i}] {result:e} vs {expected:e}"
        );
    }

    // Evaluating output for input rgbaImage[8-11] = {qnan, qnan, qnan, 0.}.
    assert!(rgba[8].is_nan());
    assert_eq!(rgba[11], 0.0f32);

    // Evaluating output for input rgbaImage[12-15] = {0., 0., 0., qnan.}.
    check_close(rgba[12], 1.0f32, rtol);
    assert!(rgba[15].is_nan());

    // Evaluating output for input rgbaImage[16-19] = {inf, inf, inf, 0.}.
    assert_eq!(rgba[16], INF);
    assert_eq!(rgba[19], 0.0f32);

    // Evaluating output for input rgbaImage[20-23] = {0., 0., 0., inf}.
    check_close(rgba[20], 1.0f32, rtol);
    assert_eq!(rgba[23], INF);

    // Evaluating output for input rgbaImage[24-27] = {-inf, -inf, -inf, 0.}.
    assert_eq!(rgba[24], 0.0f32);
    assert_eq!(rgba[27], 0.0f32);

    // Evaluating output for input rgbaImage[28-31] = {0., 0.,  0., -inf}.
    check_close(rgba[28], 1.0f32, rtol);
    assert_eq!(rgba[31], -INF);
}

/// Port of `OCIO_ADD_TEST(LogOpCPU, anti_log_test)` @ v2.5.2.
#[test]
fn anti_log_test() {
    // Anti-Log base 10 case, no scaling.
    test_anti_log(10.0f32);

    // Anti-Log base 2 case, no scaling.
    test_anti_log(2.0f32);
}

/// Port of `OCIO_ADD_TEST(LogOpCPU, cameralin2log_test)` @ v2.5.2.
#[test]
fn cameralin2log_test() {
    const NUM_PIXELS: usize = 3;
    let rgba_image: [f32; 4 * NUM_PIXELS] = [
        -0.1f32, 0.0f32, 0.01f32, 0.0f32, //
        0.08f32, 0.16f32, 1.16f32, 0.0f32, //
        -INF, INF, QNAN, 0.0f32,
    ];

    // logSideSlope = 0.2
    // logSideOffset = 0.6
    // linSideSlope = 1.1
    // linSideOffset = 0.05
    // linSideBreak = 0.1
    // linearSlope = 1.2

    let mut params: Params = vec![0.2, 0.6, 1.1, 0.05, 0.1, 1.2];
    let base = 2.0;
    let dir = TransformDirection::Forward;
    let log_op =
        LogOpData::from_channel_params(base, params.clone(), params.clone(), params.clone(), dir)
            .unwrap();

    let renderer = get_log_renderer(&log_op, true);
    let rgba = apply(renderer.as_ref(), &rgba_image);

    let error = 1e-6f32;

    // Evaluating output for input rgbaImage[0-2] = { -0.1f, 0.f, 0.01f, ... }.
    check_close(rgba[0], -0.168771237955f32, error);
    check_close(rgba[1], -0.048771237955f32, error);
    check_close(rgba[2], -0.036771237955f32, error);

    // Evaluating output for input rgbaImage[4-6] = { 0.08f, 0.16f, 1.16f, ... }.
    check_close(rgba[4], 0.047228762045f32, error);
    check_close(rgba[5], 0.170878935551f32, 10.0f32 * error);
    check_close(rgba[6], 0.68141615509f32, error);

    // Evaluating output for input rgbaImage[8-10] = { -inf, inf, qnan, ... }.
    assert_eq!(rgba[8], -INF);
    check_close(rgba[9], 26.2f32, 10.0f32 * error);
    assert!(rgba[10].is_nan());

    // Set linearSlope to default.
    params.pop();
    let lognols =
        LogOpData::from_channel_params(base, params.clone(), params.clone(), params.clone(), dir)
            .unwrap();

    let renderer_no_ls = get_log_renderer(&lognols, true);
    let rgba_nols = apply(renderer_no_ls.as_ref(), &rgba_image);

    // Evaluating output for input rgbaImage[0-2] = { -0.1f, 0.f, 0.01f, ... }.
    check_close(rgba_nols[0], -0.325512374199f32, error);
    check_close(rgba_nols[1], -0.127141806077f32, error);
    check_close(rgba_nols[2], -0.107304749265f32, error);

    // Evaluating output for input rgbaImage[4-6] = { 0.08f, 0.16f, 1.16f, ... }.
    check_close(rgba_nols[4], 0.031554648421f32, error);
    check_close(rgba_nols[5], 0.170878935551f32, 10.0f32 * error);
    check_close(rgba_nols[6], 0.68141615509f32, error);

    // Evaluating output for input rgbaImage[8-10] = { -inf, inf, qnan, ... }.
    assert_eq!(rgba_nols[8], -INF);
    check_close(rgba_nols[9], 26.2f32, 10.0f32 * error);
    assert!(rgba_nols[10].is_nan());

    // Don't use break.
    params.pop();
    let lognobreak =
        LogOpData::from_channel_params(base, params.clone(), params.clone(), params, dir).unwrap();

    let renderer_no_break = get_log_renderer(&lognobreak, true);
    let rgba_nobreak = apply(renderer_no_break.as_ref(), &rgba_image);

    let error2 = 1e-5f32;

    // Evaluating output for input rgbaImage[0-2] = { -0.1f, 0.f, 0.01f, ... }.
    check_close(rgba_nobreak[0], -24.6f32, error2);
    check_close(rgba_nobreak[1], -0.264385618977f32, error2);
    check_close(rgba_nobreak[2], -0.20700938942f32, error2);

    // Evaluating output for input rgbaImage[4-6] = { 0.08f, 0.16f, 1.16f, ... }.
    check_close(rgba_nobreak[4], 0.028548034423f32, error2);
    check_close(rgba_nobreak[5], 0.170878935551f32, error2);
    // Upstream's expected value here is a double literal, so the check is in double.
    check_close(
        f64::from(rgba_nobreak[6]),
        0.68141615509_f64,
        f64::from(error2),
    );

    // Evaluating output for input rgbaImage[8-10] = { -inf, inf, qnan, ... }.
    check_close(rgba_nobreak[8], -24.6f32, error2);
    check_close(rgba_nobreak[9], 26.2f32, error2);
    check_close(rgba_nobreak[10], -24.6f32, error2);
}

/// Port of `OCIO_ADD_TEST(LogOpCPU, cameralog2lin_test)` @ v2.5.2.
#[test]
fn cameralog2lin_test() {
    // Inverse of previous test.
    let rgba_image: [f32; 12] = [
        -0.168771237955f32,
        -0.048771237955f32,
        -0.036771237955f32,
        0.0f32, //
        0.047228762045f32,
        0.170878935551f32,
        0.68141615509f32,
        0.0f32, //
        -INF,
        INF,
        QNAN,
        0.0f32,
    ];

    let params: Params = vec![0.2, 0.6, 1.1, 0.05, 0.1, 1.2];
    let base = 2.0;
    let dir = TransformDirection::Inverse;
    let log_op =
        LogOpData::from_channel_params(base, params.clone(), params.clone(), params, dir).unwrap();

    let renderer = get_log_renderer(&log_op, true);
    let rgba = apply(renderer.as_ref(), &rgba_image);

    let error = 1e-6f32;

    // Evaluating output for input rgbaImage[0-2] =
    // { -0.168771237955f, -0.048771237955f, -0.036771237955f, ... }.
    check_close(rgba[0], -0.1f32, error);
    check_close(rgba[1], 0.0f32, error);
    check_close(rgba[2], 0.01f32, error);

    // Evaluating output for input rgbaImage[4-6] =
    // { 0.047228762045f, 0.170878935551f, 0.68141615509f, ... }.
    check_close(rgba[4], 0.08f32, error);
    check_close(rgba[5], 0.16f32, error);
    check_close(rgba[6], 1.16f32, 10.0f32 * error);

    // Evaluating output for input rgbaImage[8-10] = { -inf, inf, qnan, ... }.
    assert_eq!(rgba[8], -INF);
    assert_eq!(rgba[9], INF);
    assert!(rgba[10].is_nan());
}
