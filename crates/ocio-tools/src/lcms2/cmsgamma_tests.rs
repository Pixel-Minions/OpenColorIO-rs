// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! lcms2's own tests of tone curves: `testbed/testcms2.c` @ lcms2.17.
//!
//! `cmsBuildTabulatedToneCurveFloat` isn't reached, so the float table tests aren't here.

use ocio_testkit::upstream::equal_with_abs_error;

use super::*;

/// `FIXED_PRECISION_15_16` (testbed/testcms2.c:832 @ lcms2.17).
const FIXED_PRECISION_15_16: f64 = 1.0 / 65535.0;

/// `FLOAT_PRECISSION` (testbed/testcms2.c:838 @ lcms2.17).
const FLOAT_PRECISSION: f64 = 0.00001;

/// `cmsEstimateGamma` (src/cmsgamma.c:1449-1492 @ lcms2.17), which only these tests use.
fn cms_estimate_gamma(t: &ToneCurve, precision: f64) -> f64 {
    let (mut sum, mut sum2, mut n) = (0f64, 0f64, 0f64);

    // Excluding endpoints
    for i in 1..(MAX_NODES_IN_CURVE - 1) {
        let x = i as f64 / (MAX_NODES_IN_CURVE - 1) as f64;
        let y = cms_eval_tone_curve_float(t, x as f32) as f64;

        // Avoid 7% on lower part to prevent
        // artifacts due to linear ramps

        if y > 0.0 && y < 1.0 && x > 0.07 {
            let gamma = y.ln() / x.ln();
            sum += gamma;
            sum2 += gamma * gamma;
            n += 1.0;
        }
    }

    // We need enough valid samples
    if n <= 1.0 {
        return -1.0;
    }

    // Take a look on SD to see if gamma isn't exponential at all
    let std = ((n * sum2 - sum * sum) / (n * (n - 1.0))).sqrt();

    if std > precision {
        return -1.0;
    }

    sum / n // The mean
}

/// `MAX_NODES_IN_CURVE` (src/cmsgamma.c:42 @ lcms2.17).
const MAX_NODES_IN_CURVE: i32 = 4097;

/// Port of `CheckGammaEstimation` (testbed/testcms2.c:2391-2399 @ lcms2.17).
fn check_gamma_estimation(c: &ToneCurve, g: f64) {
    let est = cms_estimate_gamma(c, 0.001);
    assert!(
        equal_with_abs_error(est, g, 0.001),
        "gamma estimation: {est}"
    );
}

/// Port of `CheckGammaCreation16` (testbed/testcms2.c:2401-2422 @ lcms2.17).
#[test]
fn check_gamma_creation16() {
    let lin_gamma = cms_build_gamma(1.0).unwrap();
    for i in 0..0xffff {
        let in_ = i as u16;
        assert_eq!(cms_eval_tone_curve16(&lin_gamma, in_), in_);
    }
    check_gamma_estimation(&lin_gamma, 1.0);
}

/// Port of `CheckGammaCreationFlt` (testbed/testcms2.c:2426-2446 @ lcms2.17).
#[test]
fn check_gamma_creation_flt() {
    let lin_gamma = cms_build_gamma(1.0).unwrap();
    for i in 0..0xffff {
        let in_ = (i as f64 / 65535.0) as f32;
        let out = cms_eval_tone_curve_float(&lin_gamma, in_);
        assert!(equal_with_abs_error(in_ as f64, out as f64, 1.0 / 65535.0));
    }
    check_gamma_estimation(&lin_gamma, 1.0);
}

/// Port of `CheckGammaFloat` (testbed/testcms2.c:2451-2474 @ lcms2.17). The C test prints the
/// error it measures, and fails only on the estimate.
fn check_gamma_float(g: f64) {
    let curve = cms_build_gamma(g).unwrap();
    check_gamma_estimation(&curve, g);
}

/// Port of `CheckGamma18` (testbed/testcms2.c:2477-2480 @ lcms2.17).
#[test]
fn check_gamma18() {
    check_gamma_float(1.8);
}

/// Port of `CheckGamma22` (testbed/testcms2.c:2482-2485 @ lcms2.17).
#[test]
fn check_gamma22() {
    check_gamma_float(2.2);
}

/// Port of `CheckGamma30` (testbed/testcms2.c:2487-2490 @ lcms2.17).
#[test]
fn check_gamma30() {
    check_gamma_float(3.0);
}

/// Port of `CheckGammaWordTable` (testbed/testcms2.c:2548-2580 @ lcms2.17). As
/// [`check_gamma_float`].
fn check_gamma_word_table(g: f64) {
    let values: Vec<u16> = (0..=1024)
        .map(|i| {
            let in_ = (i as f64 / 1024.0) as f32;
            ((in_ as f64).powf(g) * 65535.0 + 0.5).floor() as u16
        })
        .collect();

    let curve = cms_build_tabulated_tone_curve16(1025, Some(&values)).unwrap();

    check_gamma_estimation(&curve, g);
}

/// Port of `CheckGamma18TableWord` (testbed/testcms2.c:2583-2586 @ lcms2.17).
#[test]
fn check_gamma18_table_word() {
    check_gamma_word_table(1.8);
}

/// Port of `CheckGamma22TableWord` (testbed/testcms2.c:2588-2591 @ lcms2.17).
#[test]
fn check_gamma22_table_word() {
    check_gamma_word_table(2.2);
}

/// Port of `CheckGamma30TableWord` (testbed/testcms2.c:2593-2596 @ lcms2.17).
#[test]
fn check_gamma30_table_word() {
    check_gamma_word_table(3.0);
}

/// Port of `Gamma` (testbed/testcms2.c @ lcms2.17).
fn gamma(x: f32, params: &[f64]) -> f32 {
    (x as f64).powf(params[0]) as f32
}

/// Port of `CIE122` (testbed/testcms2.c @ lcms2.17).
fn cie122(x: f32, params: &[f64]) -> f32 {
    let x = x as f64;
    let val = if x >= -params[2] / params[1] {
        let e = params[1] * x + params[2];
        if e > 0.0 { e.powf(params[0]) } else { 0.0 }
    } else {
        0.0
    };
    val as f32
}

/// Port of `IEC61966_3` (testbed/testcms2.c @ lcms2.17).
fn iec61966_3(x: f32, params: &[f64]) -> f32 {
    let x = x as f64;
    let val = if x >= -params[2] / params[1] {
        let e = params[1] * x + params[2];
        if e > 0.0 {
            e.powf(params[0]) + params[3]
        } else {
            0.0
        }
    } else {
        params[3]
    };
    val as f32
}

/// Port of `IEC61966_21` (testbed/testcms2.c @ lcms2.17).
fn iec61966_21(x: f32, params: &[f64]) -> f32 {
    let x = x as f64;
    let val = if x >= params[4] {
        let e = params[1] * x + params[2];
        if e > 0.0 { e.powf(params[0]) } else { 0.0 }
    } else {
        x * params[3]
    };
    val as f32
}

/// Port of `param_5` (testbed/testcms2.c @ lcms2.17).
fn param_5(x: f32, params: &[f64]) -> f32 {
    let x = x as f64;
    // Y = (aX + b)^Gamma + e | X >= d
    // Y = cX + f             | else
    let val = if x >= params[4] {
        let e = params[1] * x + params[2];
        if e > 0.0 {
            e.powf(params[0]) + params[5]
        } else {
            0.0
        }
    } else {
        x * params[3] + params[6]
    };
    val as f32
}

/// Port of `param_6` (testbed/testcms2.c @ lcms2.17).
fn param_6(x: f32, params: &[f64]) -> f32 {
    let e = params[1] * x as f64 + params[2];
    let val = if e > 0.0 {
        e.powf(params[0]) + params[3]
    } else {
        0.0
    };
    val as f32
}

/// Port of `param_7` (testbed/testcms2.c @ lcms2.17).
fn param_7(x: f32, params: &[f64]) -> f32 {
    let val = params[1] * (params[2] * (x as f64).powf(params[0]) + params[3]).log10() + params[4];
    val as f32
}

/// Port of `param_8` (testbed/testcms2.c @ lcms2.17).
fn param_8(x: f32, params: &[f64]) -> f32 {
    let val = params[0] * params[1].powf(params[2] * x as f64 + params[3]) + params[4];
    val as f32
}

/// Port of `CheckSingleParametric` (testbed/testcms2.c @ lcms2.17). The reverses of types 6 to 8
/// aren't reached in the tool, so those go forward only.
fn check_single_parametric(name: &str, f: fn(f32, &[f64]) -> f32, type_: i32, params: &[f64]) {
    let tc = cms_build_parametric_tone_curve(type_, params).unwrap();
    let tc_1 = cms_build_parametric_tone_curve(-type_, params).unwrap();

    for i in 0..=1000 {
        let x = i as f32 / 1000.0;

        let y_fn = f(x, params);
        let y_param = cms_eval_tone_curve_float(&tc, x);

        assert!(
            equal_with_abs_error(y_fn as f64, y_param as f64, FIXED_PRECISION_15_16),
            "{name}: {x}"
        );

        if type_ <= 5 {
            let x_param = cms_eval_tone_curve_float(&tc_1, y_param);
            let y_param2 = f(x_param, params);
            assert!(
                equal_with_abs_error(y_fn as f64, y_param2 as f64, FIXED_PRECISION_15_16),
                "inverse {name}: {x}"
            );
        }
    }
}

/// Port of `CheckParametricToneCurves` (testbed/testcms2.c:3019-3112 @ lcms2.17).
#[test]
fn check_parametric_tone_curves() {
    // 1) X = Y ^ Gamma
    check_single_parametric("Gamma", gamma, 1, &[2.2]);

    // 2) CIE 122-1966
    // Y = (aX + b)^Gamma  | X >= -b/a
    // Y = 0               | else
    check_single_parametric("CIE122-1966", cie122, 2, &[2.2, 1.5, -0.5]);

    // 3) IEC 61966-3
    // Y = (aX + b)^Gamma | X <= -b/a
    // Y = c              | else
    check_single_parametric("IEC 61966-3", iec61966_3, 3, &[2.2, 1.5, -0.5, 0.3]);

    // 4) IEC 61966-2.1 (sRGB)
    // Y = (aX + b)^Gamma | X >= d
    // Y = cX             | X < d
    check_single_parametric(
        "IEC 61966-2.1",
        iec61966_21,
        4,
        &[2.4, 1. / 1.055, 0.055 / 1.055, 1. / 12.92, 0.04045],
    );

    // 5) Y = (aX + b)^Gamma + e | X >= d
    // Y = cX + f             | else
    check_single_parametric("param_5", param_5, 5, &[2.2, 0.7, 0.2, 0.3, 0.1, 0.5, 0.2]);

    // 6) Y = (aX + b) ^ Gamma + c
    check_single_parametric("param_6", param_6, 6, &[2.2, 0.7, 0.2, 0.3]);

    // 7) Y = a * log (b * X^Gamma + c) + d
    check_single_parametric("param_7", param_7, 7, &[2.2, 0.9, 0.9, 0.02, 0.1]);

    // 8) Y = a * b ^ (c*X+d) + e
    check_single_parametric("param_8", param_8, 8, &[0.9, 0.9, 1.02, 0.1, 0.2]);
}

/// `StraightLine` (testbed/testcms2.c:7821-7824 @ lcms2.17).
fn straight_line(x: f32) -> f32 {
    (0.1 + 0.9 * x as f64) as f32
}

/// Port of `CheckFloatSegments` (testbed/testcms2.c:7866-7914 @ lcms2.17), with `TestCurve`.
#[test]
fn check_float_segments() {
    const K_NUM_POINTS: usize = 10;

    // build a segmented curve with a sampled section...
    let line = |x0: f32, x1: f32| {
        let mut segment = CurveSegment {
            x0,
            x1,
            type_: 6,
            ..Default::default()
        };
        // Y = (a * X + b) ^ Gamma + c
        segment.params[..4].copy_from_slice(&[1.0, 0.9f32 as f64, 0.0, 0.1f32 as f64]);
        segment
    };

    // From zero to 1
    let y: Vec<f32> = (0..K_NUM_POINTS)
        .map(|i| {
            let x = (0.1 + (i as f32 / (K_NUM_POINTS - 1) as f32) as f64 * 0.8) as f32;
            straight_line(x)
        })
        .collect();
    let sampled = CurveSegment {
        x0: 0.1,
        x1: 0.9,
        type_: 0,
        n_grid_points: K_NUM_POINTS as u32,
        sampled_points: Some(y),
        ..Default::default()
    };

    // from 1 to +infinity
    let segments = [line(-1e22, 0.1), sampled, line(0.9, 1e22)];

    let curve = cms_build_segmented_tone_curve(&segments).unwrap();

    // TestCurve
    for i in 0..K_NUM_POINTS * 3 {
        let x = i as f32 / (K_NUM_POINTS * 3 - 1) as f32;
        let expected = straight_line(x);
        let out = cms_eval_tone_curve_float(&curve, x);
        assert!(
            equal_with_abs_error(expected as f64, out as f64, FLOAT_PRECISSION),
            "{x}"
        );
    }
}

/// Port of `CheckReverseDegenerated` (testbed/testcms2.c @ lcms2.17), the reverse of a table with
/// flat runs, on the path every `curv` table of a profile takes.
#[test]
fn check_reverse_degenerated() {
    let tab = [
        0u16, 0, 0, 0, 0, 0x5555, 0x6666, 0x7777, 0x8888, 0x9999, 0xffff, 0xffff, 0xffff, 0xffff,
        0xffff, 0xffff,
    ];
    let p = cms_build_tabulated_tone_curve16(16, Some(&tab)).unwrap();
    let g = cms_reverse_tone_curve(&p).unwrap();

    // CheckFToneCurvePoint
    let check = |point: u16, value: i32| {
        let result = cms_eval_tone_curve16(&g, point) as i32;
        assert!(equal_with_abs_error(value as f64, result as f64, 1.0));
    };

    // Now let's check some points
    check(0x5555, 0x5555);
    check(0x7777, 0x7777);

    // First point for zero
    check(0x0000, 0x4444);

    // Last point
    check(0xFFFF, 0xFFFF);
}
