// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Ported `tests/cpu/ops/log/LogOpCPU_tests.cpp` @ v2.5.2.
//!
//! The wheel is built with `OCIO_USE_SSE2`, so the ports keep the `#if OCIO_USE_SSE2` branches.

use super::*;
use crate::math_utils::std_min;
use crate::open_color_types::TransformDirection;
use crate::ops::log::log_op_data::Params;
use crate::ops::log::log_utils::{
    CtfChannel, CtfParams, LogStyle, convert_log_parameters, ctf_values, get_log_direction,
};
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

    let renderer = get_log_renderer(&log_op, true).unwrap();
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

    let renderer = get_log_renderer(&log_op, true).unwrap();
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

/// The legacy parameters of upstream's log2lin and lin2log tests, per channel: gamma, refWhite,
/// refBlack, highlight, shadow (tests/cpu/ops/log/LogOpCPU_tests.cpp:221-241, 357-377 @ v2.5.2).
fn legacy_params(style: LogStyle) -> CtfParams {
    let mut params = CtfParams::default();
    for (channel, values) in [
        (CtfChannel::Red, [0.5, 685., 93., 0.8, 0.0004]),
        (CtfChannel::Green, [0.6, 684., 94., 0.9, 0.0005]),
        (CtfChannel::Blue, [0.65, 683., 95., 1.0, 0.0003]),
    ] {
        let p = params.get_mut(channel);
        p[ctf_values::GAMMA] = values[0];
        p[ctf_values::REF_WHITE] = values[1];
        p[ctf_values::REF_BLACK] = values[2];
        p[ctf_values::HIGHLIGHT] = values[3];
        p[ctf_values::SHADOW] = values[4];
    }
    params.style = style;
    params
}

/// The op data upstream's tests build from legacy parameters: `GetLogDirection`,
/// `ConvertLogParameters`, then `LogOpData(base, paramsR, paramsG, paramsB, dir)`.
fn legacy_log_op(params: &CtfParams) -> LogOpData {
    let (mut params_r, mut params_g, mut params_b) = (Params::new(), Params::new(), Params::new());
    let mut base = 1.0;
    let dir = get_log_direction(params.style);
    convert_log_parameters(
        params,
        &mut base,
        &mut params_r,
        &mut params_g,
        &mut params_b,
    )
    .unwrap();
    LogOpData::from_channel_params(base, params_r, params_g, params_b, dir).unwrap()
}

/// `ComputeLog2LinEval` (tests/cpu/ops/log/LogOpCPU_tests.cpp:185-206 @ v2.5.2).
fn compute_log2lin_eval(input: f32, params: &[f64]) -> f32 {
    let range = 0.002f32 * 1023.0f32;

    let gamma = params[0] as f32;
    let ref_white = params[1] as f32 / 1023.0f32;
    let ref_black = params[2] as f32 / 1023.0f32;
    let highlight = params[3] as f32;
    let shadow = params[4] as f32;

    let mult_factor = range / gamma;

    let mut tmp_value = (ref_black - ref_white) * mult_factor;
    tmp_value = std_min(tmp_value, -0.0001f32);

    let gain = (highlight - shadow) / (1.0f32 - 10.0f32.powf(tmp_value));
    let offset = gain - (highlight - shadow);

    10.0f32.powf((input - ref_white) * mult_factor) * gain - offset + shadow
}

/// The channel's legacy parameters for index `i` of a pixel, or none for alpha (the tests'
/// `noParam`).
fn channel_params(params: &CtfParams, i: usize) -> &[f64] {
    match i % 4 {
        0 => params.get(CtfChannel::Red),
        1 => params.get(CtfChannel::Green),
        2 => params.get(CtfChannel::Blue),
        _ => &[],
    }
}

/// Port of `OCIO_ADD_TEST(LogOpCPU, log2lin_test)` @ v2.5.2.
#[test]
fn log2lin_test() {
    let params = legacy_params(LogStyle::LogToLin);
    let log_op = legacy_log_op(&params);

    let renderer = get_log_renderer(&log_op, true).unwrap();
    let rgba = apply(renderer.as_ref(), &RGBA_IMAGE);

    // Relative error tolerance for the log2 approximation.
    let rtol = 2.0f32.powf(-14.0f32);

    for i in 0..8 {
        let is_alpha = i % 4 == 3;

        let result = rgba[i];
        let mut expected = RGBA_IMAGE[i];

        if !is_alpha {
            expected = compute_log2lin_eval(expected, channel_params(&params, i));
        }

        // LogOpCPU implementation uses optimized logarithm approximation cannot use strict
        // comparison.
        assert!(
            equal_with_safe_rel_error(result, expected, rtol, 1.0f32),
            "[{i}] {result:e} vs {expected:e}"
        );
    }

    let red_p = params.get(CtfChannel::Red);
    let res0 = compute_log2lin_eval(0.0f32, red_p);

    // Evaluating output for input rgbaImage[8-11] = {qnan, qnan, qnan, 0.}.
    assert!(rgba[8].is_nan());
    assert_eq!(rgba[11], 0.0f32);

    // Evaluating output for input rgbaImage[12-15] = {0., 0., 0., qnan.}.
    check_close(rgba[12], res0, rtol);
    assert!(rgba[15].is_nan());

    // Evaluating output for input rgbaImage[16-19] = {inf, inf, inf, 0.}.
    assert_eq!(rgba[16], INF);
    assert_eq!(rgba[19], 0.0f32);

    // Evaluating output for input rgbaImage[20-23] = {0., 0., 0., inf}.
    check_close(rgba[20], res0, rtol);
    assert_eq!(rgba[23], INF);

    // Evaluating output for input rgbaImage[24-27] = {-inf, -inf, -inf, 0.}.
    check_close(rgba[24], compute_log2lin_eval(-INF, red_p), rtol);
    assert_eq!(rgba[27], 0.0f32);

    // Evaluating output for input rgbaImage[28-31] = {0., 0.,  0., -inf}.
    check_close(rgba[28], res0, rtol);
    assert_eq!(rgba[31], -INF);
}

/// `ComputeLin2LogEval` (tests/cpu/ops/log/LogOpCPU_tests.cpp:317-340 @ v2.5.2).
fn compute_lin2log_eval(mut input: f32, params: &[f64]) -> f32 {
    let min_value = f32::MIN_POSITIVE;

    let gamma = params[0] as f32;
    let ref_white = params[1] as f32 / 1023.0f32;
    let ref_black = params[2] as f32 / 1023.0f32;
    let highlight = params[3] as f32;
    let shadow = params[4] as f32;

    let range = 0.002f32 * 1023.0f32;
    let mult_factor = range / gamma;

    let mut tmp_value = (ref_black - ref_white) * mult_factor;
    tmp_value = std_min(tmp_value, -0.0001f32);

    let gain = (highlight - shadow) / (1.0f32 - 10.0f32.powf(tmp_value));
    let offset = gain - (highlight - shadow);

    input = (input - shadow + offset) / gain;
    std_max(min_value, input).log10() / mult_factor + ref_white
}

/// Port of `OCIO_ADD_TEST(LogOpCPU, lin2log_test)` @ v2.5.2.
#[test]
fn lin2log_test() {
    let params = legacy_params(LogStyle::LinToLog);
    let log_op = legacy_log_op(&params);

    let renderer = get_log_renderer(&log_op, true).unwrap();
    let rgba = apply(renderer.as_ref(), &RGBA_IMAGE);

    let error = 1e-4f32;
    for i in 0..8 {
        let is_alpha = i % 4 == 3;

        let result = rgba[i];
        let mut expected = RGBA_IMAGE[i];

        if !is_alpha {
            expected = compute_lin2log_eval(expected, channel_params(&params, i));
        }

        // LogOpCPU implementation uses optimized logarithm approximation cannot use strict
        // comparison.
        check_close(result, expected, error);
    }

    let red_p = params.get(CtfChannel::Red);
    let res0 = compute_lin2log_eval(0.0f32, red_p);
    let res_min = compute_lin2log_eval(-100.0f32, red_p);

    // Evaluating output for input rgbaImage[8-11] = {qnan, qnan, qnan, 0.}.
    check_close(rgba[8], res_min, error);
    assert_eq!(rgba[11], 0.0f32);

    // Evaluating output for input rgbaImage[12-15] = {0., 0., 0., qnan.}.
    check_close(rgba[12], res0, error);
    assert!(rgba[15].is_nan());

    // Evaluating output for input rgbaImage[16-19] = {inf, inf, inf, 0.}.
    check_close(rgba[16], 10.08598328f32, error);
    assert_eq!(rgba[19], 0.0f32);

    // Evaluating output for input rgbaImage[20-23] = {0., 0., 0., inf}.
    check_close(rgba[20], res0, error);
    assert_eq!(rgba[23], INF);

    // Evaluating output for input rgbaImage[24-27] = {-inf, -inf, -inf, 0.}.
    check_close(rgba[24], res_min, error);
    assert_eq!(rgba[27], 0.0f32);

    // Evaluating output for input rgbaImage[28-31] = {0., 0.,  0., -inf}.
    check_close(rgba[28], res0, error);
    assert_eq!(rgba[31], -INF);
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

    let renderer = get_log_renderer(&log_op, true).unwrap();
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

    let renderer_no_ls = get_log_renderer(&lognols, true).unwrap();
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

    let renderer_no_break = get_log_renderer(&lognobreak, true).unwrap();
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

    let renderer = get_log_renderer(&log_op, true).unwrap();
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
