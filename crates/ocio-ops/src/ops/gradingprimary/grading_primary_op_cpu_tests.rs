// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of tests/cpu/ops/gradingprimary/GradingPrimaryOpCPU_tests.cpp @ v2.5.2. The renderers
//! take the `OCIO_USE_SSE2` branches, as every x86-64 wheel does.

use super::*;
use crate::ops::gradingprimary::grading_primary::{GradingPrimary, GradingRgbm};
use ocio_testkit::upstream::check_close;

/// Port of `ValidateImage` (GradingPrimaryOpCPU_tests.cpp:14-40 @ v2.5.2), `OCIO_USE_SSE2`'s
/// tolerance, 1e-4; a NaN expected value isn't tested in SSE mode.
fn validate_image(expected: &[f32], res: &[f32]) {
    const ERROR: f32 = 1e-4;
    assert_eq!(expected.len(), res.len());
    for (&e, &r) in expected.iter().zip(res) {
        if e.is_nan() {
            // Do not test nan in SSE mode.
        } else if e != r {
            check_close(e, r, ERROR);
        }
    }
}

/// `op->apply(in, res, numPixels)`: the renderer on a copy of `input`.
fn apply(op: &Arc<dyn CpuOp>, input: &[f32]) -> Vec<f32> {
    let mut res = input.to_vec();
    op.apply(&mut res);
    res
}

/// `TS1` (GradingPrimaryOpCPU_tests.cpp:147-172 @ v2.5.2).
mod ts1 {
    use super::*;
    pub(super) const STYLE: GradingStyle = GradingStyle::Log;
    pub(super) const BRIGHTNESS: GradingRgbm = GradingRgbm {
        red: -10.,
        green: 45.,
        blue: -5.,
        master: 50.,
    };
    pub(super) const CONTRAST: GradingRgbm = GradingRgbm {
        red: 0.9,
        green: 1.4,
        blue: 0.7,
        master: 0.75,
    };
    pub(super) const GAMMA: GradingRgbm = GradingRgbm {
        red: 1.1,
        green: 0.7,
        blue: 1.05,
        master: 1.15,
    };
    pub(super) const PIVOT: f64 = -0.3;
    pub(super) const SATURATION: f64 = 1.21;
    pub(super) const CLAMP_BLACK: f64 = -0.05;
    pub(super) const CLAMP_WHITE: f64 = 1.50;
    pub(super) const PIVOT_BLACK: f64 = 0.05;
    pub(super) const PIVOT_WHITE: f64 = 0.9;
    pub(super) const INPUT_32F: [f32; 8] = [0.1, 0.9, 1.2, 1.0, -0.4, 0.2, 1.2, 0.5];
    pub(super) const EXPECTED_32F: [f32; 8] = [
        0.23327083,
        1.77384381,
        0.86027701,
        1.0,
        -0.10117631,
        0.79016840,
        1.02051931,
        0.5,
    ];
    pub(super) const EXPECTED_CLAMP_32F: [f32; 8] = [
        0.23327083,
        1.50000000,
        0.86027701,
        1.0,
        -0.05000000,
        0.79016840,
        1.02051931,
        0.5,
    ];
    pub(super) const EXPECTED_WBPIVOT_32F: [f32; 8] = [
        0.21137053,
        1.82456972,
        0.83339811,
        1.0,
        -0.16370305,
        0.81365125,
        0.99945772,
        0.5,
    ];
}

/// Port of `OCIO_ADD_TEST(GradingPrimaryOpCPU, log)` @ v2.5.2.
#[test]
#[allow(clippy::excessive_precision)]
fn log() {
    let mut gd = GradingPrimaryOpData::new(ts1::STYLE);

    // Test in forward direction.

    let mut gdp = GradingPrimary::new(ts1::STYLE);
    gdp.brightness = ts1::BRIGHTNESS;
    gdp.contrast = ts1::CONTRAST;
    gdp.gamma = ts1::GAMMA;
    gdp.pivot = ts1::PIVOT;
    gdp.saturation = ts1::SATURATION;

    gd.set_value(&gdp).unwrap();
    gd.get_dynamic_property_internal().make_dynamic();

    let op = get_grading_primary_cpu_renderer(&gd).unwrap();
    let res = apply(&op, &ts1::INPUT_32F);
    validate_image(&ts1::EXPECTED_32F, &res);

    // The CPUOp has a copy of gd.  Get the dynamic property ptr in order to change the value
    // for the apply.
    let dp = op
        .get_dynamic_property(DynamicPropertyType::GradingPrimary)
        .unwrap();
    let dpgp = dp.as_grading_primary().unwrap();

    gdp.clamp_black = ts1::CLAMP_BLACK;
    gdp.clamp_white = ts1::CLAMP_WHITE;

    dpgp.set_value(&gdp).unwrap();
    let res = apply(&op, &ts1::INPUT_32F);
    validate_image(&ts1::EXPECTED_CLAMP_32F, &res);

    gdp.clamp_black = -100.;
    gdp.clamp_white = 100.;
    gdp.pivot_black = ts1::PIVOT_BLACK;
    gdp.pivot_white = ts1::PIVOT_WHITE;

    dpgp.set_value(&gdp).unwrap();
    let res = apply(&op, &ts1::INPUT_32F);
    validate_image(&ts1::EXPECTED_WBPIVOT_32F, &res);

    // Test in inverse direction.

    gd.set_direction(TransformDirection::Inverse);
    let mut gdp = GradingPrimary::new(ts1::STYLE);
    gdp.brightness = ts1::BRIGHTNESS;
    gdp.contrast = ts1::CONTRAST;
    gdp.gamma = ts1::GAMMA;
    gdp.pivot = ts1::PIVOT;
    gdp.saturation = ts1::SATURATION;

    gd.set_value(&gdp).unwrap();
    let op = get_grading_primary_cpu_renderer(&gd).unwrap();
    let res = apply(&op, &ts1::EXPECTED_32F);
    validate_image(&ts1::INPUT_32F, &res);

    let dp = op
        .get_dynamic_property(DynamicPropertyType::GradingPrimary)
        .unwrap();
    let dpgp = dp.as_grading_primary().unwrap();

    // Clamping prevents full inversion. Skip.

    gdp.pivot_black = ts1::PIVOT_BLACK;
    gdp.pivot_white = ts1::PIVOT_WHITE;

    dpgp.set_value(&gdp).unwrap();
    let res = apply(&op, &ts1::EXPECTED_WBPIVOT_32F);
    validate_image(&ts1::INPUT_32F, &res);
}

/// The renderer's class, as `typeid(c).name()` names it.
fn class_of(op: &Arc<dyn CpuOp>) -> String {
    let name = format!("{op:?}");
    name.split([' ', '{']).next().unwrap_or("").to_string()
}

/// Port of `OCIO_ADD_TEST(GradingPrimaryOpCPU, identity)` @ v2.5.2.
#[test]
fn identity() {
    const QNAN: f32 = f32::NAN;
    const INF: f32 = f32::INFINITY;
    #[rustfmt::skip]
    let image: [f32; 4 * 9] = [
        -0.50, -0.25, 0.50, 0.0,
         0.75,  1.00, 1.25, 1.0,
         1.25,  1.50, 1.75, 0.0,
         QNAN,  QNAN, QNAN, 0.0,
         0.0,   0.0,  0.0,  QNAN,
         INF,   INF,  INF,  0.0,
         0.0,   0.0,  0.0,  INF,
        -INF,  -INF, -INF,  0.0,
         0.0,   0.0,  0.0, -INF,
    ];

    #[rustfmt::skip]
    let expected: [f32; 4 * 9] = [
        -0.50, -0.25, 0.50, 0.0,
         0.75,  1.00, 1.25, 1.0,
         1.25,  1.50, 1.75, 0.0,
         QNAN,  QNAN, QNAN, 0.0,
         0.0,   0.0,  0.0,  QNAN,
         INF,   INF,  INF,  0.0,
         0.0,   0.0,  0.0,  INF,
        -INF,  -INF, -INF,  0.0,
         0.0,   0.0,  0.0, -INF,
    ];

    let mut gd = GradingPrimaryOpData::new(GradingStyle::Log);
    let op = get_grading_primary_cpu_renderer(&gd).unwrap();
    // Check that the right OpCPU is created. Check that class name contains LogFwd.
    assert!(class_of(&op).contains("LogFwd"), "{}", class_of(&op));
    validate_image(&expected, &apply(&op, &image));

    gd.set_direction(TransformDirection::Inverse);
    let op = get_grading_primary_cpu_renderer(&gd).unwrap();
    // Check that the right OpCPU is created. Check that class name contains LogRev.
    assert!(class_of(&op).contains("LogRev"), "{}", class_of(&op));
    validate_image(&expected, &apply(&op, &image));

    let mut gd = GradingPrimaryOpData::new(GradingStyle::Lin);
    let op = get_grading_primary_cpu_renderer(&gd).unwrap();
    // Check that the right OpCPU is created. Check that class name contains LinFwd.
    assert!(class_of(&op).contains("LinFwd"), "{}", class_of(&op));
    validate_image(&expected, &apply(&op, &image));

    gd.set_direction(TransformDirection::Inverse);
    let op = get_grading_primary_cpu_renderer(&gd).unwrap();
    // Check that the right OpCPU is created. Check that class name contains LinRev.
    assert!(class_of(&op).contains("LinRev"), "{}", class_of(&op));
    validate_image(&expected, &apply(&op, &image));

    let mut gd = GradingPrimaryOpData::new(GradingStyle::Video);
    let op = get_grading_primary_cpu_renderer(&gd).unwrap();
    // Check that the right OpCPU is created. Check that class name contains VidFwd.
    assert!(class_of(&op).contains("VidFwd"), "{}", class_of(&op));
    validate_image(&expected, &apply(&op, &image));

    gd.set_direction(TransformDirection::Inverse);
    let op = get_grading_primary_cpu_renderer(&gd).unwrap();
    // Check that the right OpCPU is created. Check that class name contains VidRev.
    assert!(class_of(&op).contains("VidRev"), "{}", class_of(&op));
    validate_image(&expected, &apply(&op, &image));
}

/// `TS2` (GradingPrimaryOpCPU_tests.cpp:250-270 @ v2.5.2).
mod ts2 {
    use super::*;
    pub(super) const STYLE: GradingStyle = GradingStyle::Lin;
    pub(super) const EXPOSURE: GradingRgbm = GradingRgbm {
        red: 0.5,
        green: -0.2,
        blue: 0.4,
        master: -0.25,
    };
    pub(super) const OFFSET: GradingRgbm = GradingRgbm {
        red: -0.03,
        green: 0.02,
        blue: 0.1,
        master: -0.1,
    };
    pub(super) const CONTRAST: GradingRgbm = GradingRgbm {
        red: 0.9,
        green: 1.4,
        blue: 0.7,
        master: 0.75,
    };
    pub(super) const PIVOT: f64 = 0.5;
    pub(super) const SATURATION: f64 = 1.33;
    pub(super) const CLAMP_BLACK: f64 = -0.40;
    pub(super) const CLAMP_WHITE: f64 = 1.05;
    pub(super) const INPUT_32F: [f32; 8] = [0.1, 0.9, 1.2, 1.0, -0.1, 0.9, 3.2, 0.5];
    pub(super) const EXPECTED_32F: [f32; 8] = [
        -0.24746465,
        0.67575505,
        0.64940625,
        1.0,
        -0.50871492,
        0.68002410,
        1.19721858,
        0.5,
    ];
    pub(super) const EXPECTED_CLAMP_32F: [f32; 8] = [
        -0.24746465,
        0.67575505,
        0.64940625,
        1.0,
        -0.40000000,
        0.68002410,
        1.05000000,
        0.5,
    ];
}

/// Port of `OCIO_ADD_TEST(GradingPrimaryOpCPU, lin)` @ v2.5.2.
#[test]
#[allow(clippy::excessive_precision)]
fn lin() {
    let mut gd = GradingPrimaryOpData::new(ts2::STYLE);

    // Test in forward direction.

    let mut gdp = GradingPrimary::new(ts2::STYLE);
    gdp.exposure = ts2::EXPOSURE;
    gdp.offset = ts2::OFFSET;
    gdp.contrast = ts2::CONTRAST;
    gdp.pivot = ts2::PIVOT;
    gdp.saturation = ts2::SATURATION;

    gd.set_value(&gdp).unwrap();

    let op = get_grading_primary_cpu_renderer(&gd).unwrap();
    validate_image(&ts2::EXPECTED_32F, &apply(&op, &ts2::INPUT_32F));

    gdp.clamp_black = ts2::CLAMP_BLACK;
    gdp.clamp_white = ts2::CLAMP_WHITE;

    gd.set_value(&gdp).unwrap();
    validate_image(&ts2::EXPECTED_CLAMP_32F, &apply(&op, &ts2::INPUT_32F));

    // Test in inverse direction.

    gd.set_direction(TransformDirection::Inverse);
    gdp.clamp_black = -100.;
    gdp.clamp_white = 100.;

    gd.set_value(&gdp).unwrap();
    let op = get_grading_primary_cpu_renderer(&gd).unwrap();
    validate_image(&ts2::INPUT_32F, &apply(&op, &ts2::EXPECTED_32F));
}

/// `TS3` (GradingPrimaryOpCPU_tests.cpp:316-341 @ v2.5.2).
mod ts3 {
    use super::*;
    pub(super) const STYLE: GradingStyle = GradingStyle::Video;
    pub(super) const LIFT: GradingRgbm = GradingRgbm {
        red: 0.05,
        green: -0.04,
        blue: 0.02,
        master: 0.05,
    };
    pub(super) const GAMMA: GradingRgbm = GradingRgbm {
        red: 0.9,
        green: 1.4,
        blue: 0.7,
        master: 0.75,
    };
    pub(super) const GAIN: GradingRgbm = GradingRgbm {
        red: 1.2,
        green: 1.1,
        blue: 1.25,
        master: 0.8,
    };
    pub(super) const OFFSET: GradingRgbm = GradingRgbm {
        red: -0.03,
        green: 0.02,
        blue: 0.1,
        master: -0.1,
    };
    pub(super) const SATURATION: f64 = 1.2;
    pub(super) const CLAMP_BLACK: f64 = -0.15;
    pub(super) const CLAMP_WHITE: f64 = 1.50;
    pub(super) const PIVOT_BLACK: f64 = 0.05;
    pub(super) const PIVOT_WHITE: f64 = 0.9;
    pub(super) const INPUT_32F: [f32; 8] = [0.1, 0.9, 1.2, 1.0, -0.1, 0.9, 1.2, 0.5];
    pub(super) const EXPECTED_32F: [f32; 8] = [
        -0.10667760,
        0.75643484,
        1.53729499,
        1.0,
        -0.17148458,
        0.75881552,
        1.53967567,
        0.5,
    ];
    pub(super) const EXPECTED_CLAMP_32F: [f32; 8] = [
        -0.10667760,
        0.75643484,
        1.50000000,
        1.0,
        -0.15000000,
        0.75881552,
        1.50000000,
        0.5,
    ];
    pub(super) const EXPECTED_WBPIVOT_32F: [f32; 8] = [
        -0.06553329,
        0.74984638,
        1.67741281,
        1.0,
        -0.14759934,
        0.75286107,
        1.68042750,
        0.5,
    ];
}

/// Port of `OCIO_ADD_TEST(GradingPrimaryOpCPU, video)` @ v2.5.2.
#[test]
#[allow(clippy::excessive_precision)]
fn video() {
    let mut gd = GradingPrimaryOpData::new(ts3::STYLE);

    // Test in forward direction.

    let mut gdp = GradingPrimary::new(ts3::STYLE);
    gdp.lift = ts3::LIFT;
    gdp.gamma = ts3::GAMMA;
    gdp.gain = ts3::GAIN;
    gdp.offset = ts3::OFFSET;
    gdp.saturation = ts3::SATURATION;

    gd.set_value(&gdp).unwrap();

    let op = get_grading_primary_cpu_renderer(&gd).unwrap();
    validate_image(&ts3::EXPECTED_32F, &apply(&op, &ts3::INPUT_32F));

    gdp.clamp_black = ts3::CLAMP_BLACK;
    gdp.clamp_white = ts3::CLAMP_WHITE;

    gd.set_value(&gdp).unwrap();
    validate_image(&ts3::EXPECTED_CLAMP_32F, &apply(&op, &ts3::INPUT_32F));

    gdp.clamp_black = -100.;
    gdp.clamp_white = 100.;
    gdp.pivot_black = ts3::PIVOT_BLACK;
    gdp.pivot_white = ts3::PIVOT_WHITE;

    gd.set_value(&gdp).unwrap();
    validate_image(&ts3::EXPECTED_WBPIVOT_32F, &apply(&op, &ts3::INPUT_32F));

    // Test in inverse direction.

    gd.set_direction(TransformDirection::Inverse);
    let mut gdp = GradingPrimary::new(ts3::STYLE);
    gdp.lift = ts3::LIFT;
    gdp.gamma = ts3::GAMMA;
    gdp.gain = ts3::GAIN;
    gdp.offset = ts3::OFFSET;
    gdp.saturation = ts3::SATURATION;

    gd.set_value(&gdp).unwrap();
    let op = get_grading_primary_cpu_renderer(&gd).unwrap();
    validate_image(&ts3::INPUT_32F, &apply(&op, &ts3::EXPECTED_32F));

    // Clamping prevents full inversion. Skip.

    gdp.pivot_black = ts3::PIVOT_BLACK;
    gdp.pivot_white = ts3::PIVOT_WHITE;

    gd.set_value(&gdp).unwrap();
    validate_image(&ts3::INPUT_32F, &apply(&op, &ts3::EXPECTED_WBPIVOT_32F));
}
