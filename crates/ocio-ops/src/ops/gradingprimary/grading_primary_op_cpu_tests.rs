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
