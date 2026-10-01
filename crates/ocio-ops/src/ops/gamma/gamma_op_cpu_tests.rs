// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Ported `tests/cpu/ops/gamma/GammaOpCPU_tests.cpp` @ v2.5.2.
//!
//! The wheel is built with `OCIO_USE_SSE2`, so the ports keep the `#if OCIO_USE_SSE2` expected
//! values, and render with `getCPUOp(true)`.
//!
//! Each upstream test builds the op, finalizes it, optimizes the list with
//! `OPTIMIZATION_DEFAULT` and requires one op left. The optimizer is WP 1.6a; for one Gamma op
//! that is neither a no-op nor an identity it keeps the op unchanged (`RemoveNoOps`,
//! `ReplaceIdentityOps`; there is no pair to combine or cancel), so the ports check those two
//! conditions and render the op data directly.

use super::*;
use crate::ops::gamma::gamma_op_data::Params;
use ocio_testkit::upstream::equal_with_safe_rel_error;

const QNAN: f32 = f32::NAN;
const INF: f32 = f32::INFINITY;

/// `ApplyGamma` (tests/cpu/ops/gamma/GammaOpCPU_tests.cpp:18-53 @ v2.5.2): renders `image` in
/// place and compares with `result`: NaN must stay NaN; otherwise a relative error with a
/// minimum expected value of 1 (so an absolute error below 1).
#[track_caller]
fn apply_gamma(op: &GammaOpData, image: &mut [f32], result: &[f32], error_threshold: f32) {
    // ops.finalize(); ops.optimize(OPTIMIZATION_DEFAULT); OCIO_REQUIRE_EQUAL(ops.size(), 1).
    assert!(!op.is_no_op().unwrap() && !op.is_identity().unwrap());

    let cpu = get_gamma_renderer(op, true).unwrap();
    cpu.apply(image);

    for (idx, (&value, &expected)) in image.iter().zip(result).enumerate() {
        if expected.is_nan() {
            assert!(
                value.is_nan(),
                "Index: {idx} - Values: {value} and: {expected}"
            );
            continue;
        }
        // Using rel error with a large minExpected value of 1 will transition from absolute
        // error for expected values < 1 and relative error for values > 1.
        assert!(
            equal_with_safe_rel_error(value, expected, error_threshold, 1.0f32),
            "Index: {idx} - Values: {value:.9} and: {expected:.9} - Threshold: {error_threshold}"
        );
    }
}

fn gamma_data(style: GammaStyle, r: Params, g: Params, b: Params, a: Params) -> GammaOpData {
    GammaOpData::new(style, r, g, b, a)
}

/// The input of the clamping and moncurve tests (GammaOpCPU_tests.cpp:65-72 @ v2.5.2).
const INPUT_7: [f32; 28] = [
    -1.0f32, -0.75f32, -0.25f32, 0.0f32, //
    -0.0025f32, 0.0f32, 0.00005f32, 0.5f32, //
    0.0005f32, 0.005f32, 0.05f32, 0.75f32, //
    0.25f32, 0.5f32, 0.75f32, 1.0f32, //
    0.80f32, 0.95f32, 1.0f32, 1.5f32, //
    1.005f32, 1.05f32, 1.5f32, -0.25f32, //
    -INF, INF, QNAN, 0.0f32,
];

/// The input of the mirror and pass-thru basic tests (GammaOpCPU_tests.cpp:195-204 @ v2.5.2).
const INPUT_9: [f32; 36] = [
    0.0005f32, 0.005f32, 0.05f32, 0.75f32, //
    -0.0005f32, -0.005f32, -0.05f32, -0.75f32, //
    0.25f32, 0.5f32, 0.75f32, 1.0f32, //
    -0.25f32, -0.5f32, -0.75f32, -1.0f32, //
    0.80f32, 0.95f32, 1.0f32, 1.5f32, //
    -0.80f32, -0.95f32, -1.0f32, -1.5f32, //
    1.005f32, 1.05f32, 1.5f32, 0.25f32, //
    -1.005f32, -1.05f32, -1.5f32, -0.25f32, //
    -INF, INF, QNAN, 0.0f32,
];

/// Port of `OCIO_ADD_TEST(GammaOpCPU, apply_basic_style_fwd)` @ v2.5.2.
#[test]
fn apply_basic_style_fwd() {
    let error_threshold = 1e-7f32;
    let mut input_32f = INPUT_7;

    // Including a gamma of 1.0 because OCIO v1 did not clamp negatives in that case.
    // In OCIO v2, the behavior does *not* depend on the gamma.
    let gamma_vals = [1.2, 2.12, 1., 1.05];

    let expected_32f: [f32; 28] = [
        0.0f32,
        0.0f32,
        0.0f32,
        0.0f32, //
        0.0f32,
        0.0f32,
        0.00005f32,
        0.48297336f32, //
        0.00010933f32,
        0.00001323f32,
        0.0499999f32,
        0.73928129f32, //
        0.18946611f32,
        0.23005184f32,
        0.7499921f32,
        1.00001204f32, //
        0.76507961f32,
        0.89695119f32,
        1.0000116f32,
        1.53070319f32, //
        1.00601125f32,
        1.10895324f32,
        1.4999843f32,
        0.0f32, //
        0.0f32,
        INF,
        0.0f32,
        0.0f32,
    ];

    let gamma = gamma_data(
        GammaStyle::BasicFwd,
        vec![gamma_vals[0]],
        vec![gamma_vals[1]],
        vec![gamma_vals[2]],
        vec![gamma_vals[3]],
    );

    apply_gamma(&gamma, &mut input_32f, &expected_32f, error_threshold);
}

/// Port of `OCIO_ADD_TEST(GammaOpCPU, apply_basic_style_rev)` @ v2.5.2.
#[test]
fn apply_basic_style_rev() {
    let error_threshold = 1e-7f32;
    let mut input_32f = INPUT_7;

    let gamma_vals = [1.2, 2.12, 1.123, 1.05];

    let expected_32f: [f32; 28] = [
        0.0f32,
        0.0f32,
        0.0f32,
        0.0f32, //
        0.0f32,
        0.0f32,
        0.00014792f32,
        0.51678240f32, //
        0.00177476f32,
        0.08215060f32,
        0.06941742f32,
        0.76033723f32, //
        0.31498342f32,
        0.72111737f32,
        0.77400052f32,
        1.00001109f32, //
        0.83031141f32,
        0.97609287f32,
        1.00001061f32,
        1.47130167f32, //
        1.00417137f32,
        1.02327621f32,
        1.43483067f32,
        0.0f32, //
        0.0f32,
        1.49761057e+18f32,
        0.0f32,
        0.0f32,
    ];

    let gamma = gamma_data(
        GammaStyle::BasicRev,
        vec![gamma_vals[0]],
        vec![gamma_vals[1]],
        vec![gamma_vals[2]],
        vec![gamma_vals[3]],
    );

    apply_gamma(&gamma, &mut input_32f, &expected_32f, error_threshold);
}

/// Port of `OCIO_ADD_TEST(GammaOpCPU, apply_basic_mirror_style_fwd)` @ v2.5.2.
#[test]
fn apply_basic_mirror_style_fwd() {
    let error_threshold = 1e-7f32;
    let mut input_32f = INPUT_9;

    let gamma_vals = [1.2, 2.12, 1.123, 1.05];

    let expected_32f: [f32; 36] = [
        0.00010933f32,
        0.00001323f32,
        0.03458935f32,
        0.73928129f32, //
        -0.00010933f32,
        -0.00001323f32,
        -0.03458935f32,
        -0.73928129f32, //
        0.18946611f32,
        0.23005184f32,
        0.72391760f32,
        1.00001204f32, //
        -0.18946611f32,
        -0.23005184f32,
        -0.72391760f32,
        -1.00001204f32, //
        0.76507961f32,
        0.89695119f32,
        1.00001264f32,
        1.53070319f32, //
        -0.76507961f32,
        -0.89695119f32,
        -1.00001264f32,
        -1.53070319f32, //
        1.00601125f32,
        1.10895324f32,
        1.57668686f32,
        0.23326106f32, //
        -1.00601125f32,
        -1.10895324f32,
        -1.57668686f32,
        -0.23326106f32, //
        -INF,
        INF,
        0.0f32,
        0.0f32,
    ];

    let gamma = gamma_data(
        GammaStyle::BasicMirrorFwd,
        vec![gamma_vals[0]],
        vec![gamma_vals[1]],
        vec![gamma_vals[2]],
        vec![gamma_vals[3]],
    );

    apply_gamma(&gamma, &mut input_32f, &expected_32f, error_threshold);
}

/// Port of `OCIO_ADD_TEST(GammaOpCPU, apply_basic_mirror_style_rev)` @ v2.5.2.
#[test]
fn apply_basic_mirror_style_rev() {
    let error_threshold = 1e-7f32;
    let mut input_32f = INPUT_9;

    let gamma_vals = [1.2, 2.12, 1.123, 1.05];

    let expected_32f: [f32; 36] = [
        0.00177476f32,
        0.08215060f32,
        0.06941742f32,
        0.76033723f32, //
        -0.00177476f32,
        -0.08215060f32,
        -0.06941742f32,
        -0.76033723f32, //
        0.31498342f32,
        0.72111737f32,
        0.77400052f32,
        1.00001109f32, //
        -0.31498342f32,
        -0.72111737f32,
        -0.77400052f32,
        -1.00001109f32, //
        0.83031141f32,
        0.97609287f32,
        1.00001061f32,
        1.47130167f32, //
        -0.83031141f32,
        -0.97609287f32,
        -1.00001061f32,
        -1.47130167f32, //
        1.00417137f32,
        1.02327621f32,
        1.43483067f32,
        0.26706201f32, //
        -1.00417137f32,
        -1.02327621f32,
        -1.43483067f32,
        -0.26706201f32, //
        -1.28786104e+32f32,
        1.49761057e+18f32,
        0.0f32,
        0.0f32,
    ];

    let gamma = gamma_data(
        GammaStyle::BasicMirrorRev,
        vec![gamma_vals[0]],
        vec![gamma_vals[1]],
        vec![gamma_vals[2]],
        vec![gamma_vals[3]],
    );

    apply_gamma(&gamma, &mut input_32f, &expected_32f, error_threshold);
}

/// Port of `OCIO_ADD_TEST(GammaOpCPU, apply_basic_pass_thru_style_fwd)` @ v2.5.2.
#[test]
fn apply_basic_pass_thru_style_fwd() {
    let error_threshold = 1e-7f32;
    let mut input_32f = INPUT_9;

    let gamma_vals = [1.2, 2.12, 1.123, 1.05];

    let i = &INPUT_9;
    let expected_32f: [f32; 36] = [
        0.00010933f32,
        0.00001323f32,
        0.03458935f32,
        0.73928129f32, //
        i[4],
        i[5],
        i[6],
        i[7], //
        0.18946611f32,
        0.23005184f32,
        0.72391760f32,
        1.00001204f32, //
        i[12],
        i[13],
        i[14],
        i[15], //
        0.76507961f32,
        0.89695119f32,
        1.00001264f32,
        1.53070319f32, //
        i[20],
        i[21],
        i[22],
        i[23], //
        1.00601125f32,
        1.10895324f32,
        1.57668686f32,
        0.23326106f32, //
        i[28],
        i[29],
        i[30],
        i[31], //
        -INF,
        INF,
        QNAN,
        0.0f32,
    ];

    let gamma = gamma_data(
        GammaStyle::BasicPassThruFwd,
        vec![gamma_vals[0]],
        vec![gamma_vals[1]],
        vec![gamma_vals[2]],
        vec![gamma_vals[3]],
    );

    apply_gamma(&gamma, &mut input_32f, &expected_32f, error_threshold);
}

/// Port of `OCIO_ADD_TEST(GammaOpCPU, apply_basic_pass_thru_style_rev)` @ v2.5.2.
#[test]
fn apply_basic_pass_thru_style_rev() {
    let error_threshold = 1e-7f32;
    let mut input_32f = INPUT_9;

    let gamma_vals = [1.2, 2.12, 1.123, 1.05];

    let i = &INPUT_9;
    let expected_32f: [f32; 36] = [
        0.00177476f32,
        0.08215060f32,
        0.06941742f32,
        0.76033723f32, //
        i[4],
        i[5],
        i[6],
        i[7], //
        0.31498342f32,
        0.72111737f32,
        0.77400052f32,
        1.00001109f32, //
        i[12],
        i[13],
        i[14],
        i[15], //
        0.83031141f32,
        0.97609287f32,
        1.00001061f32,
        1.47130167f32, //
        i[20],
        i[21],
        i[22],
        i[23], //
        1.00417137f32,
        1.02327621f32,
        1.43483067f32,
        0.26706201f32, //
        i[28],
        i[29],
        i[30],
        i[31], //
        -INF,
        1.49761057e+18f32,
        QNAN,
        0.0f32,
    ];

    let gamma = gamma_data(
        GammaStyle::BasicPassThruRev,
        vec![gamma_vals[0]],
        vec![gamma_vals[1]],
        vec![gamma_vals[2]],
        vec![gamma_vals[3]],
    );

    apply_gamma(&gamma, &mut input_32f, &expected_32f, error_threshold);
}

/// Port of `OCIO_ADD_TEST(GammaOpCPU, apply_moncurve_style_fwd)` @ v2.5.2.
#[test]
fn apply_moncurve_style_fwd() {
    let error_threshold = 1e-7f32;
    let mut input_32f = INPUT_7;

    let expected_32f: [f32; 28] = [
        -0.07738016f32,
        -0.33144456f32,
        -0.25f32,
        0.0f32, //
        -0.00019345f32,
        0.0f32,
        0.00005f32,
        0.49101364f32, //
        0.00003869f32,
        0.00220963f32,
        0.05f32,
        0.73652046f32, //
        0.05087645f32,
        0.30550804f32,
        0.75f32,
        1.00001871f32, //
        0.60383129f32,
        0.91060406f32,
        1.0f32,
        1.63147723f32, //
        1.01142657f32,
        1.09394502f32,
        1.499984f32,
        -0.24550682f32, //
        -INF,
        INF,
        QNAN,
        0.0f32,
    ];

    let gamma = gamma_data(
        GammaStyle::MoncurveFwd,
        vec![2.4, 0.055],
        vec![2.2, 0.2],
        vec![1.0, 0.0],
        vec![1.8, 0.6],
    );

    apply_gamma(&gamma, &mut input_32f, &expected_32f, error_threshold);
}

/// Port of `OCIO_ADD_TEST(GammaOpCPU, apply_moncurve_style_rev)` @ v2.5.2.
#[test]
fn apply_moncurve_style_rev() {
    let error_threshold = 1e-6f32;
    let mut input_32f = INPUT_7;

    let expected_32f: [f32; 28] = [
        -6.18606853f32,
        -1.69711625f32,
        -0.25f32,
        0.0f32, //
        -0.01546517f32,
        0.0f32,
        0.00005f32,
        0.50915080f32, //
        0.00309303f32,
        0.01131410f32,
        0.05f32,
        0.76366448f32, //
        0.51735591f32,
        0.67569005f32,
        0.75f32,
        1.00001215f32, //
        0.90233862f32,
        0.97234255f32,
        1.0f32,
        1.40423023f32, //
        1.00229334f32,
        1.02690458f32,
        1.499984f32,
        -0.25457540f32, //
        -INF,
        3.92334474e+17f32,
        QNAN,
        0.0f32,
    ];

    let gamma = gamma_data(
        GammaStyle::MoncurveRev,
        vec![2.4, 0.1],
        vec![2.2, 0.2],
        vec![1.0, 0.0],
        vec![1.8, 0.6],
    );

    apply_gamma(&gamma, &mut input_32f, &expected_32f, error_threshold);
}

/// Port of `OCIO_ADD_TEST(GammaOpCPU, apply_moncurve_mirror_style_fwd)` @ v2.5.2.
#[test]
fn apply_moncurve_mirror_style_fwd() {
    let error_threshold = 1e-7f32;
    let mut input_32f: [f32; 36] = [
        0.0005f32, 0.005f32, 0.05f32, 0.75f32, //
        -0.0005f32, -0.005f32, -0.05f32, -0.75f32, //
        0.25f32, 0.5f32, 0.75f32, 1.0f32, //
        -0.25f32, -0.5f32, -0.75f32, -1.0f32, //
        0.80f32, 0.95f32, 1.0f32, 1.5f32, //
        -0.80f32, -0.95f32, -1.0f32, -1.5f32, //
        1.005f32, 1.05f32, 1.5f32, 1.0f32, //
        -1.005f32, -1.05f32, -1.5f32, -1.0f32, //
        -INF, INF, QNAN, 0.0f32,
    ];

    let expected_32f: [f32; 36] = [
        0.00003869f32,
        0.00220963f32,
        0.04081632f32,
        0.73652046f32, //
        -0.00003869f32,
        -0.00220963f32,
        -0.04081632f32,
        -0.73652046f32, //
        0.05087645f32,
        0.30550804f32,
        0.67475068f32,
        1.00001871f32, //
        -0.05087645f32,
        -0.30550804f32,
        -0.67475068f32,
        -1.00001871f32, //
        0.60383129f32,
        0.91060406f32,
        1.00002050f32,
        1.63147723f32, //
        -0.60383129f32,
        -0.91060406f32,
        -1.00002050f32,
        -1.63147723f32, //
        1.01142657f32,
        1.09394502f32,
        1.84183871f32,
        1.00001871f32, //
        -1.01142657f32,
        -1.09394502f32,
        -1.84183871f32,
        -1.00001871f32, //
        -INF,
        INF,
        QNAN,
        0.0f32,
    ];

    let gamma = gamma_data(
        GammaStyle::MoncurveMirrorFwd,
        vec![2.4, 0.055],
        vec![2.2, 0.2],
        vec![2.0, 0.4],
        vec![1.8, 0.6],
    );

    apply_gamma(&gamma, &mut input_32f, &expected_32f, error_threshold);
}

/// Port of `OCIO_ADD_TEST(GammaOpCPU, apply_moncurve_mirror_style_rev)` @ v2.5.2.
#[test]
fn apply_moncurve_mirror_style_rev() {
    let error_threshold = 1e-6f32;
    let mut input_32f: [f32; 36] = [
        0.0005f32, 0.005f32, 0.05f32, 0.75f32, //
        -0.0005f32, -0.005f32, -0.05f32, -0.75f32, //
        0.25f32, 0.5f32, 0.75f32, 1.0f32, //
        -0.25f32, -0.5f32, -0.75f32, -1.0f32, //
        0.80f32, 0.95f32, 1.0f32, 0.75f32, //
        -0.80f32, -0.95f32, -1.0f32, -0.75f32, //
        1.005f32, 1.05f32, 1.5f32, 1.0f32, //
        -1.005f32, -1.05f32, -1.5f32, -1.0f32, //
        -INF, INF, QNAN, 0.0f32,
    ];

    let expected_32f: [f32; 36] = [
        0.00309303f32,
        0.01131410f32,
        0.06125000f32,
        0.76366448f32, //
        -0.00309303f32,
        -0.01131410f32,
        -0.06125000f32,
        -0.76366448f32, //
        0.51735591f32,
        0.67569005f32,
        0.81243133f32,
        1.00001215f32, //
        -0.51735591f32,
        -0.67569005f32,
        -0.81243133f32,
        -1.00001215f32, //
        0.90233862f32,
        0.97234255f32,
        1.00000989f32,
        0.76366448f32, //
        -0.90233862f32,
        -0.97234255f32,
        -1.00000989f32,
        -0.76366448f32, //
        1.00229334f32,
        1.02690458f32,
        1.31464004f32,
        1.00001215f32, //
        -1.00229334f32,
        -1.02690458f32,
        -1.31464004f32,
        -1.00001215f32, //
        -1.24832838e+16f32,
        3.92334474e+17f32,
        QNAN,
        0.0f32,
    ];

    let gamma = gamma_data(
        GammaStyle::MoncurveMirrorRev,
        vec![2.4, 0.1],
        vec![2.2, 0.2],
        vec![2.0, 0.4],
        vec![1.8, 0.6],
    );

    apply_gamma(&gamma, &mut input_32f, &expected_32f, error_threshold);
}

/// Where upstream's renderers would read past a channel's parameters, `get_gamma_renderer`
/// returns an error instead (`docs/improvements.md` U-24): a basic style reads one parameter
/// per channel, a moncurve style two.
#[test]
fn short_parameters_are_errors() {
    use crate::ops::gamma::gamma_op_data::SHORT_PARAMS;
    let all = |p: Params| [p.clone(), p.clone(), p.clone(), p];
    for (style, short, enough) in [
        (GammaStyle::BasicFwd, vec![], vec![2.]),
        (GammaStyle::BasicMirrorRev, vec![], vec![2., 0.5]),
        (GammaStyle::MoncurveFwd, vec![2.], vec![2., 0.1]),
        (GammaStyle::MoncurveMirrorRev, vec![], vec![2., 0.1, 7.]),
    ] {
        for channel in 0..4 {
            let mut p = all(enough.clone());
            p[channel] = short.clone();
            let [r, g, b, a] = p;
            let gamma = gamma_data(style, r, g, b, a);
            for fast in [false, true] {
                let Err(err) = get_gamma_renderer(&gamma, fast) else {
                    panic!("an error")
                };
                assert_eq!(err.message(), SHORT_PARAMS);
            }
        }
        let [r, g, b, a] = all(enough);
        let gamma = gamma_data(style, r, g, b, a);
        for fast in [false, true] {
            assert!(get_gamma_renderer(&gamma, fast).is_ok());
        }
    }
}
