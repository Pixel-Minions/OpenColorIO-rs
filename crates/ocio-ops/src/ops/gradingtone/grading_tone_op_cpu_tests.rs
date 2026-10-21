// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of tests/cpu/ops/gradingtone/GradingToneOpCPU_tests.cpp @ v2.5.2.

use super::*;
use crate::ops::gradingtone::grading_tone::GradingRgbmsw;
use ocio_testkit::upstream::check_close;

/// Port of `ValidateImage` (GradingToneOpCPU_tests.cpp:14-44 @ v2.5.2), `OCIO_USE_SSE2`'s
/// tolerance, 2e-4, relative above 1; a NaN expected value isn't tested in SSE mode.
fn validate_image(expected: &[f32], res: &[f32]) {
    const ERROR: f32 = 2e-4;
    assert_eq!(expected.len(), res.len());
    for (&e, &r) in expected.iter().zip(res) {
        if e.is_nan() {
            // Do not test nan in SSE mode.
        } else if e != r {
            // `std::abs(expected) < 1.f ? 1.f : std::abs(expected)`, for a value that isn't NaN.
            let error_factor = e.abs().max(1.0);
            check_close(e, r, ERROR * error_factor);
        }
    }
}

/// `op->apply(in, res, numPixels)`: the renderer on a copy of `input`.
fn apply(op: &Arc<dyn CpuOp>, input: &[f32]) -> Vec<f32> {
    let mut res = input.to_vec();
    op.apply(&mut res);
    res
}

/// The forward then the inverse renderer of `gtd`, made dynamic as upstream's tests do, on
/// `input` and `expected`.
fn check_both_ways(style: GradingStyle, gtd: &GradingTone, input: &[f32], expected: &[f32]) {
    let mut gt = GradingToneOpData::new(style);

    // Test in forward direction.
    gt.set_value(gtd).unwrap();
    gt.get_dynamic_property_internal().make_dynamic();
    let op = get_grading_tone_cpu_renderer(&gt).unwrap();
    validate_image(expected, &apply(&op, input));

    // Test in inverse direction.
    gt.set_direction(TransformDirection::Inverse);
    gt.set_value(gtd).unwrap();
    let op = get_grading_tone_cpu_renderer(&gt).unwrap();
    validate_image(input, &apply(&op, expected));
}

/// The renderer's class name, as `typeid(c).name()` gives upstream's: its `Debug` text.
fn class_name(op: &Arc<dyn CpuOp>) -> String {
    let text = format!("{op:?}");
    text.split_whitespace().next().unwrap().to_string()
}

/// Port of `OCIO_ADD_TEST(GradingToneOpCPU, identity)` @ v2.5.2.
#[test]
fn identity() {
    let qnan = f32::NAN;
    let inf = f32::INFINITY;
    // inf is clamp, so inverse would fail.
    #[rustfmt::skip]
    let image: [f32; 32] = [
        -0.50, -0.25, 0.50, 0.0,
        0.75, 1.00, 1.25, 1.0,
        65000., 1.50, -65000., 0.0,
        qnan, qnan, qnan, 0.0,
        0.0, 0.0, 0.0, qnan,
        0.0, 0.0, 0.0, inf,
        -inf, -inf, -inf, 0.0,
        0.0, 0.0, 0.0, -inf,
    ];
    let expected = image;

    // Check that the right OpCPU is created: its class name contains the expected one.
    for (style, fwd, rev) in [
        (
            GradingStyle::Log,
            "GradingToneFwdOpCpu",
            "GradingToneRevOpCpu",
        ),
        (
            GradingStyle::Lin,
            "GradingToneLinearFwdOpCpu",
            "GradingToneLinearRevOpCpu",
        ),
        (
            GradingStyle::Video,
            "GradingToneFwdOpCpu",
            "GradingToneRevOpCpu",
        ),
    ] {
        let mut gd = GradingToneOpData::new(style);
        let op = get_grading_tone_cpu_renderer(&gd).unwrap();
        assert!(class_name(&op).contains(fwd), "{}", class_name(&op));
        validate_image(&expected, &apply(&op, &image));

        gd.set_direction(TransformDirection::Inverse);
        let op = get_grading_tone_cpu_renderer(&gd).unwrap();
        assert!(class_name(&op).contains(rev), "{}", class_name(&op));
        validate_image(&image, &apply(&op, &expected));
    }
}

/// Port of `OCIO_ADD_TEST(GradingToneOpCPU, log_midtones)` @ v2.5.2 (`TS1`, 150-164).
#[test]
fn log_midtones() {
    const STYLE: GradingStyle = GradingStyle::Log;
    // These are {R, G, B, master, center, width};
    let midtones = GradingRgbmsw::new(0.3, 1.0, 1.8, 1.2, 0.47, 0.6);
    let input_32f: [f32; 12] = [0.1, -0.4, 0.9, 1.0, 0.3, 0.6, 0.7, 0.5, 0.8, 2.2, 0.5, 0.0];
    let expected_32f: [f32; 12] = [
        0.09440361,
        -0.40000000,
        0.90645507,
        1.0,
        0.23564218,
        0.62838000,
        0.76080927,
        0.5,
        0.78783701,
        2.20000000,
        0.67159981,
        0.0,
    ];
    let mut gtd = GradingTone::new(STYLE);
    gtd.midtones = midtones;
    check_both_ways(STYLE, &gtd, &input_32f, &expected_32f);
}

/// Port of `OCIO_ADD_TEST(GradingToneOpCPU, log_highlights)` @ v2.5.2 (`TS2`, 196-210).
#[test]
fn log_highlights() {
    const STYLE: GradingStyle = GradingStyle::Log;
    // These are {R, G, B, master, start, pivot};
    let highlights = GradingRgbmsw::new(0.3, 1.0, 1.8, 1.4, -0.1, 0.9);
    let input_32f: [f32; 12] = [
        0.8, 0.2, -0.05, 1.0, -0.4, 0.7, 0.8, 0.5, 0.5, 1.0, 2.2, 0.0,
    ];
    let expected_32f: [f32; 12] = [
        0.75833820,
        0.21800000,
        -0.04847980,
        1.0,
        -0.40000000,
        0.75600000,
        0.88018560,
        0.5,
        0.46114011,
        0.96000000,
        1.05600000,
        0.0,
    ];
    let mut gtd = GradingTone::new(STYLE);
    gtd.highlights = highlights;
    check_both_ways(STYLE, &gtd, &input_32f, &expected_32f);
}

/// Port of `OCIO_ADD_TEST(GradingToneOpCPU, video_shadows)` @ v2.5.2 (`TS3`, 243-257).
#[test]
fn video_shadows() {
    const STYLE: GradingStyle = GradingStyle::Video;
    // These are {R, G, B, master, start, pivot};
    let shadows = GradingRgbmsw::new(0.3, 1., 1.79, 0.6, 0.8, -0.1);
    let input_32f: [f32; 12] = [
        -0.05, -0.3, -0.05, 1.0, 0.20, 0.2, 0.10, 0.5, 0.50, 1.2, 0.40, 0.0,
    ];
    let expected_32f: [f32; 12] = [
        -0.08903600,
        -0.22000000,
        -0.0101064,
        1.0,
        0.04235000,
        0.14000000,
        0.158287734,
        0.5,
        0.44006111,
        1.20000000,
        0.426106364,
        0.0,
    ];
    let mut gtd = GradingTone::new(STYLE);
    gtd.shadows = shadows;
    check_both_ways(STYLE, &gtd, &input_32f, &expected_32f);
}

/// Port of `OCIO_ADD_TEST(GradingToneOpCPU, video_white_details)` @ v2.5.2 (`TS4`, 290-304).
#[test]
fn video_white_details() {
    const STYLE: GradingStyle = GradingStyle::Video;
    // These are {R, G, B, master, start, width};
    let white_detail = GradingRgbmsw::new(0.3, 1., 1.9, 0.6, -0.2, 1.4);
    let input_32f: [f32; 12] = [0.9, -0.4, 0.8, 1.0, 1.2, 0.8, 1.0, 0.5, 8.0, 4.0, 2.0, 0.0];
    let expected_32f: [f32; 12] = [
        0.50664196,
        -0.40000000,
        0.85713846,
        1.0,
        0.59170000,
        0.65714286,
        1.11661389,
        0.5,
        1.85000000,
        2.60000000,
        17.73099488,
        0.0,
    ];
    let mut gtd = GradingTone::new(STYLE);
    gtd.whites = white_detail;
    check_both_ways(STYLE, &gtd, &input_32f, &expected_32f);
}

/// Port of `OCIO_ADD_TEST(GradingToneOpCPU, log_black_details)` @ v2.5.2 (`TS5`, 337-351).
#[test]
fn log_black_details() {
    const STYLE: GradingStyle = GradingStyle::Log;
    // These are {R, G, B, master, start, width};
    let black_detail = GradingRgbmsw::new(0.3, 1., 1.9, 0.6, 0.8, 0.9);
    let input_32f: [f32; 12] = [
        -0.05, -0.5, -0.20, 1.0, 0.05, 0.0, -0.05, 0.5, 0.40, 1.2, 0.40, 0.0,
    ];
    let expected_32f: [f32; 12] = [
        -0.88574485,
        -0.99166667,
        0.23906196,
        1.0,
        -0.50105701,
        -0.16583916,
        0.25926968,
        0.5,
        0.30488108,
        1.20000000,
        0.45937302,
        0.0,
    ];
    let mut gtd = GradingTone::new(STYLE);
    gtd.blacks = black_detail;
    check_both_ways(STYLE, &gtd, &input_32f, &expected_32f);
}

/// Port of `OCIO_ADD_TEST(GradingToneOpCPU, log_scontrast)` @ v2.5.2 (`TS6`, 384-409).
#[test]
fn log_scontrast() {
    const STYLE: GradingStyle = GradingStyle::Log;
    const SCONTRAST: f64 = 1.8;
    let input_32f: [f32; 12] = [
        0.15, 0.3, 0.42, 1.0, -0.1, 0.6, 1.2, 0.5, 0.8, 0.0, 1.0, 0.0,
    ];
    let expected_32f: [f32; 12] = [
        0.05250000,
        0.15283050,
        0.45714286,
        1.0,
        -0.03500000,
        0.83910667,
        1.07000000,
        0.5,
        0.93000000,
        0.00000000,
        1.00000000,
        0.0,
    ];
    const SCONTRAST2: f64 = 0.3;
    let input2_32f: [f32; 12] = [
        0.04, 0.3, 0.15, 1.0, -0.1, 0.6, 1.2, 0.5, 0.8, 0.0, 1.0, 0.0,
    ];
    let expected2_32f: [f32; 12] = [
        0.08050314,
        0.35031250,
        0.26213396,
        1.0,
        -0.20125786,
        0.49937500,
        1.40251572,
        0.5,
        0.63561388,
        0.00000000,
        1.00000000,
        0.0,
    ];

    let mut gt = GradingToneOpData::new(STYLE);

    // Test in forward direction.
    let mut gtd = GradingTone::new(STYLE);
    gtd.scontrast = SCONTRAST;
    gt.set_value(&gtd).unwrap();
    gt.get_dynamic_property_internal().make_dynamic();
    let op = get_grading_tone_cpu_renderer(&gt).unwrap();
    validate_image(&expected_32f, &apply(&op, &input_32f));

    // Test with second value.
    gtd.scontrast = SCONTRAST2;
    gt.set_value(&gtd).unwrap();
    let op = get_grading_tone_cpu_renderer(&gt).unwrap();
    validate_image(&expected2_32f, &apply(&op, &input2_32f));

    // Test in inverse direction with second value.
    gt.set_direction(TransformDirection::Inverse);
    gt.set_value(&gtd).unwrap();
    let op = get_grading_tone_cpu_renderer(&gt).unwrap();
    validate_image(&input2_32f, &apply(&op, &expected2_32f));

    // Test inverse with first value.
    gtd.scontrast = SCONTRAST;
    gt.set_value(&gtd).unwrap();
    let op = get_grading_tone_cpu_renderer(&gt).unwrap();
    validate_image(&input_32f, &apply(&op, &expected_32f));
}

/// Port of `OCIO_ADD_TEST(GradingToneOpCPU, lin_midtones)` @ v2.5.2 (`TS7`, 459-474).
#[test]
fn lin_midtones() {
    const STYLE: GradingStyle = GradingStyle::Lin;
    // These are {R, G, B, master, center, width};
    let midtones = GradingRgbmsw::new(0.3, 1.4, 1.8, 1., 1., 8.);
    let input_32f: [f32; 12] = [
        0.1, -0.1, 0.90, 1.0, 0.3, 0.6, 0.70, 0.5, 0.8, 1.5, 0.05, 0.0,
    ];
    let expected_32f: [f32; 12] = [
        0.04102994,
        -0.10000000,
        3.07542735,
        1.0,
        0.08530666,
        1.19569300,
        2.65221218,
        0.5,
        0.26080380,
        2.37429354,
        0.08896667,
        0.0,
    ];
    let mut gtd = GradingTone::new(STYLE);
    gtd.midtones = midtones;
    check_both_ways(STYLE, &gtd, &input_32f, &expected_32f);
}
