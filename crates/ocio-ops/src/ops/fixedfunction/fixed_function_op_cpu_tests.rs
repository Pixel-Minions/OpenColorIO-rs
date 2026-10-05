// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Ported `tests/cpu/ops/fixedfunction/FixedFunctionOpCPU_tests.cpp` @ v2.5.2: the ACES 1.x
//! styles (chunk 2.3b), the Rec.2100 surround, HSV and CIE styles (2.3c1), the HSY styles
//! (2.3c2). The other styles' tests come with their renderers.

use super::*;
use crate::ops::fixedfunction::fixed_function_op_data::Params;
use FixedFunctionOpStyle::*;
use ocio_testkit::upstream::{check_throw_what, equal_with_safe_rel_error};

/// Applies `fn_data`'s renderer to `input_32f` in place and checks every value against
/// `expected_32f` with a relative error that turns absolute below 1.
///
/// Port of `ApplyFixedFunction` (tests/cpu/ops/fixedfunction/FixedFunctionOpCPU_tests.cpp:
/// 17-55 @ v2.5.2).
#[track_caller]
fn apply_fixed_function(
    input_32f: &mut [f32],
    expected_32f: &[f32],
    fn_data: &FixedFunctionOpData,
    error_threshold: f32,
    fast_log_exp_pow: bool,
) {
    let op = get_fixed_function_cpu_renderer(fn_data, fast_log_exp_pow).unwrap();
    op.apply(input_32f);

    for (idx, (&value, &expected)) in input_32f.iter().zip(expected_32f).enumerate() {
        // Using rel error with a large minExpected value of 1 will transition from absolute
        // error for expected values < 1 and relative error for values > 1.
        assert!(
            equal_with_safe_rel_error(value, expected, error_threshold, 1.0),
            "Index: {idx} - Values: {value} expected: {expected}"
        );
    }
}

/// The data of a style without parameters.
fn data(style: FixedFunctionOpStyle) -> FixedFunctionOpData {
    FixedFunctionOpData::new(style).unwrap()
}

/// Port of `OCIO_ADD_TEST(FixedFunctionOpCPU, aces_red_mod_03)` @ v2.5.2.
#[test]
fn aces_red_mod_03() {
    let input_32f: [f32; 16] = [
        0.90, 0.05, 0.22, 0.5, //
        0.97, 0.097, 0.0097, 1.0, //
        0.89, 0.15, 0.56, 0.0, //
        -1.0, -0.001, 1.2, 0.0,
    ];

    let mut output_32f = input_32f;

    let expected_32f: [f32; 16] = [
        0.79670035, 0.05, 0.19934007, 0.5, //
        0.83517569, 0.08474324, 0.0097, 1.0, //
        0.87166744, 0.15, 0.54984271, 0.0, //
        -1.0, -0.001, 1.2, 0.0,
    ];

    apply_fixed_function(
        &mut output_32f,
        &expected_32f,
        &data(AcesRedMod03Fwd),
        1e-7,
        false,
    );
    apply_fixed_function(
        &mut output_32f,
        &input_32f,
        &data(AcesRedMod03Inv),
        1e-7,
        false,
    );
}

/// Port of `OCIO_ADD_TEST(FixedFunctionOpCPU, aces_red_mod_10)` @ v2.5.2.
#[test]
fn aces_red_mod_10() {
    let input_32f: [f32; 16] = [
        0.90, 0.05, 0.22, 0.5, //
        0.97, 0.097, 0.0097, 1.0, //
        0.89, 0.15, 0.56, 0.0, //
        -1.0, -0.001, 1.2, 0.0,
    ];

    let mut output_32f = input_32f;

    let expected_32f: [f32; 16] = [
        0.77148211, 0.05, 0.22, 0.5, //
        0.80705338, 0.097, 0.0097, 1.0, //
        0.85730940, 0.15, 0.56, 0.0, //
        -1.0, -0.001, 1.2, 0.0,
    ];

    apply_fixed_function(
        &mut output_32f,
        &expected_32f,
        &data(AcesRedMod10Fwd),
        1e-7,
        false,
    );

    let mut adjusted_input_32f = input_32f;

    // Note: There is a known issue in ACES 1.0 where the red modifier inverse algorithm is
    // not quite exact. Hence the aim values here aren't quite the same as the input.
    adjusted_input_32f[0] = 0.89146208;
    adjusted_input_32f[4] = 0.96750682;
    adjusted_input_32f[8] = 0.88518190;

    apply_fixed_function(
        &mut output_32f,
        &adjusted_input_32f,
        &data(AcesRedMod10Inv),
        1e-7,
        false,
    );
}

/// Port of `OCIO_ADD_TEST(FixedFunctionOpCPU, aces_glow_03)` @ v2.5.2.
#[test]
fn aces_glow_03() {
    let input_32f: [f32; 16] = [
        0.11, 0.02, 0.0, 0.5, // YC = 0.10
        0.01, 0.02, 0.03, 1.0, // YC = 0.03
        0.11, 0.91, 0.01, 0.0, // YC = 0.84
        -1.0, -0.001, 1.2, 0.0,
    ];

    let mut output_32f = input_32f;

    let expected_32f: [f32; 16] = [
        0.11392101, 0.02071291, 0.0, 0.5, //
        0.01070833, 0.02141666, 0.03212499, 1.0, //
        0.10999999, 0.91000002, 0.00999999, 0.0, //
        -1.0, -0.001, 1.2, 0.0,
    ];

    apply_fixed_function(
        &mut output_32f,
        &expected_32f,
        &data(AcesGlow03Fwd),
        1e-7,
        false,
    );
    apply_fixed_function(
        &mut output_32f,
        &input_32f,
        &data(AcesGlow03Inv),
        1e-7,
        false,
    );
}

/// Port of `OCIO_ADD_TEST(FixedFunctionOpCPU, aces_glow_10)` @ v2.5.2.
#[test]
fn aces_glow_10() {
    let input_32f: [f32; 16] = [
        0.11, 0.02, 0.0, 0.5, // YC = 0.10
        0.01, 0.02, 0.03, 1.0, // YC = 0.03
        0.11, 0.91, 0.01, 0.0, // YC = 0.84
        -1.0, -0.001, 1.2, 0.0,
    ];

    let mut output_32f = input_32f;

    let expected_32f: [f32; 16] = [
        0.11154121, 0.02028021, 0.0, 0.5, //
        0.01047222, 0.02094444, 0.03141666, 1.0, //
        0.10999999, 0.91000002, 0.00999999, 0.0, //
        -1.0, -0.001, 1.2, 0.0,
    ];

    apply_fixed_function(
        &mut output_32f,
        &expected_32f,
        &data(AcesGlow10Fwd),
        1e-7,
        false,
    );
    apply_fixed_function(
        &mut output_32f,
        &input_32f,
        &data(AcesGlow10Inv),
        1e-7,
        false,
    );
}

/// Port of `OCIO_ADD_TEST(FixedFunctionOpCPU, aces_dark_to_dim_10)` @ v2.5.2.
#[test]
fn aces_dark_to_dim_10() {
    let input_32f: [f32; 16] = [
        0.11, 0.02, 0.04, 0.5, //
        0.71, 0.51, 0.92, 1.0, //
        0.43, 0.82, 0.71, 0.0, //
        -0.3, 0.5, 1.2, 0.0,
    ];

    let mut output_32f = input_32f;

    let expected_32f: [f32; 16] = [
        0.11661188,
        0.02120216,
        0.04240432,
        0.5, //
        0.71719729,
        0.51516991,
        0.92932611,
        1.0, //
        0.43281638,
        0.82537078,
        0.71465027,
        0.0, //
        -0.30653429,
        0.51089048,
        1.22613716,
        0.0,
    ];

    apply_fixed_function(
        &mut output_32f,
        &expected_32f,
        &data(AcesDarkToDim10Fwd),
        1e-7,
        false,
    );
    apply_fixed_function(
        &mut output_32f,
        &input_32f,
        &data(AcesDarkToDim10Inv),
        1e-7,
        false,
    );
}

/// Port of `OCIO_ADD_TEST(FixedFunctionOpCPU, aces_gamut_map_13)` @ v2.5.2.
#[test]
fn aces_gamut_map_13() {
    // Test dataset consist of ACEScg values:
    // - Common camera color space primaries
    // - ColorChecker 24 values as per SMPTE 2065-1
    #[rustfmt::skip]
    let input_32f: [f32; 39 * 4] = [
        // ALEXA Wide Gamut
         0.96663409472,   0.04819045216,   0.00719300006,  0.0,
         0.11554181576,   1.18493819237,  -0.06659350544,  0.0,
        -0.08217582852,  -0.23312863708,   1.05940067768,  0.0,
        // BMD Wide Gamut
         0.92980366945,   0.03025679290,  -0.02240031771,  0.0,
         0.12437260151,   1.19238424301,  -0.08014731854,  0.0,
        -0.05417707562,  -0.22264070809,   1.10254764557,  0.0,
        // Cinema Gamut
         1.10869872570,  -0.05317572504,  -0.00306261564,  0.0,
         0.00142395718,   1.31239914894,  -0.22332298756,  0.0,
        -0.11012268066,  -0.25922337174,   1.22638559341,  0.0,
        // REDWideGamutRGB
         1.14983725548,  -0.02548932098,  -0.06720325351,  0.0,
        -0.06796986610,   1.30455482006,  -0.31973674893,  0.0,
        -0.08186896890,  -0.27906489372,   1.38694024086,  0.0,
        // S-Gamut3
         1.08979821205,  -0.03117186762,  -0.00326358480,  0.0,
        -0.03276504576,   1.18293666840,  -0.00156985107,  0.0,
        -0.05703317001,  -0.15176482499,   1.00483345985,  0.0,
        // Venice S-Gamut3
         1.15183949471,  -0.04052511975,  -0.01231821068,  0.0,
        -0.11769985408,   1.20661473274,   0.00725125661,  0.0,
        -0.03413961083,  -0.16608965397,   1.00506699085,  0.0,
        // V-Gamut
         1.04839742184,  -0.02998665348,  -0.00313943392,  0.0,
         0.01196120959,   1.14840388298,  -0.00963746291,  0.0,
        -0.06036021933,  -0.11841656268,   1.01277709007,  0.0,
        // CC24 hue selective patch
         0.13911968470,   0.08746965975,   0.05927771702,  0.0,
         0.45410454273,   0.32112336159,   0.23821924627,  0.0,
         0.15262818336,   0.19457373023,   0.31270095706,  0.0,
         0.11231111735,   0.14410330355,   0.06487321854,  0.0,
         0.24113640189,   0.22817260027,   0.40912008286,  0.0,
         0.27200737596,   0.47832396626,   0.40502992272,  0.0,
         0.49412208796,   0.23219805956,   0.05947655812,  0.0,
         0.09734666348,   0.10917002708,   0.33662334085,  0.0,
         0.37841814756,   0.12591768801,   0.12897071242,  0.0,
         0.09104857594,   0.05404697359,   0.13533248007,  0.0,
         0.38014721870,   0.47619381547,   0.10615456849,  0.0,
         0.60210841894,   0.38621774316,   0.08225912601,  0.0,
         0.05051656812,   0.05367648974,   0.27239432931,  0.0,
         0.14276765287,   0.28139206767,   0.09023084491,  0.0,
         0.28782477975,   0.06140174344,   0.05256444961,  0.0,
         0.70791155100,   0.58026152849,   0.09300658852,  0.0,
         0.35456034541,   0.12329842150,   0.27530980110,  0.0,
         0.08374430984,   0.22774916887,   0.35839819908,  0.0,
    ];

    let mut output_32f = input_32f;

    // Above values are passed through ctlrender and the CTL implementation (1), using
    // openEXR 32bits as the i/o image format. For more details, see
    // https://gist.github.com/remia/380d972fa568493d570f2ba298b3f23a
    // (1) urn:ampas:aces:transformId:v1.5:LMT.Academy.GamutCompress.a1.3.0
    //     Note: AP0 to / from AP1 conversions have been disabled
    #[rustfmt::skip]
    let expected_32f: [f32; 39 * 4] = [
        // ALEXA Wide Gamut
        0.96663409472,  0.08610087633,  0.04698687792,  0.0,
        0.13048231602,  1.18493819237,  0.03576064110,  0.0,
        0.02295053005,  0.00768482685,  1.05940067768,  0.0,
        // BMD Wide Gamut
        0.92980366945,  0.07499730587,  0.03567957878,  0.0,
        0.13714194298,  1.19238424301,  0.03311228752,  0.0,
        0.03551459312,  0.01163744926,  1.10254764557,  0.0,
        // Cinema Gamut
        1.10869872570,  0.05432271957,  0.04990577698,  0.0,
        0.07070028782,  1.31239914894,  0.01541912556,  0.0,
        0.02140641212,  0.01080632210,  1.22638559341,  0.0,
        // REDWideGamutRGB
        1.14983725548,  0.06666719913,  0.03411936760,  0.0,
        0.04051816463,  1.30455482006,  0.00601124763,  0.0,
        0.03941023350,  0.01482784748,  1.38694024086,  0.0,
        // S-Gamut3
        1.08979821205,  0.06064450741,  0.04896950722,  0.0,
        0.04843533039,  1.18293666840,  0.05382478237,  0.0,
        0.02941548824,  0.02107459307,  1.00483345985,  0.0,
        // Venice S-Gamut3
        1.15183949471,  0.06142425537,  0.04885411263,  0.0,
        0.01795542240,  1.20661473274,  0.05802130699,  0.0,
        0.03851079941,  0.01796829700,  1.00506699085,  0.0,
        // V-Gamut
        1.04839742184,  0.05834102631,  0.04710924625,  0.0,
        0.06705272198,  1.14840388298,  0.04955554008,  0.0,
        0.02856093645,  0.02944415808,  1.01277709007,  0.0,
        // CC24 hue selective patch
        0.13911968470,  0.08746965975,  0.05927771330,  0.0,
        0.45410454273,  0.32112336159,  0.23821924627,  0.0,
        0.15262818336,  0.19457373023,  0.31270095706,  0.0,
        0.11231111735,  0.14410330355,  0.06487321109,  0.0,
        0.24113640189,  0.22817260027,  0.40912008286,  0.0,
        0.27200737596,  0.47832396626,  0.40502992272,  0.0,
        0.49412208796,  0.23219805956,  0.05947655439,  0.0,
        0.09734666348,  0.10917001963,  0.33662334085,  0.0,
        0.37841814756,  0.12591767311,  0.12897071242,  0.0,
        0.09104857594,  0.05404697359,  0.13533248007,  0.0,
        0.38014721870,  0.47619381547,  0.10615456104,  0.0,
        0.60210841894,  0.38621774316,  0.08225911856,  0.0,
        0.05051657557,  0.05367648602,  0.27239432931,  0.0,
        0.14276765287,  0.28139206767,  0.09023086727,  0.0,
        0.28782477975,  0.06140173972,  0.05256444216,  0.0,
        0.70791155100,  0.58026152849,  0.09300661087,  0.0,
        0.35456034541,  0.12329842150,  0.27530980110,  0.0,
        0.08374431729,  0.22774916887,  0.35839819908,  0.0,
    ];

    let params: Params = vec![1.147, 1.264, 1.312, 0.815, 0.803, 0.880, 1.2];

    let fwd = FixedFunctionOpData::with_params(AcesGamutComp13Fwd, params.clone()).unwrap();
    apply_fixed_function(&mut output_32f, &expected_32f, &fwd, 1e-6, false);

    let inv = FixedFunctionOpData::with_params(AcesGamutComp13Inv, params).unwrap();
    apply_fixed_function(&mut output_32f, &input_32f, &inv, 1e-6, false);
}

/// Port of `OCIO_ADD_TEST(FixedFunctionOpCPU, rec2100_surround)` @ v2.5.2.
#[test]
fn rec2100_surround() {
    let input_32f: [f32; 20] = [
        8.4e-5, 2.4e-5, 1.4e-4, 0.1, //
        0.11, 0.02, 0.04, 0.5, //
        0.71, 0.51, 0.81, 1.0, //
        0.43, 0.82, 0.71, 0.0, //
        -1.00, -0.001, 1.2, 0.0,
    ];
    {
        let params: Params = vec![0.78];

        let mut output_32f = input_32f;

        let expected_32f: [f32; 20] = [
            0.000637205163,
            0.000182058618,
            0.001062008605,
            0.1, //
            0.21779590,
            0.03959925,
            0.07919850,
            0.5, //
            0.80029451,
            0.57485944,
            0.91301214,
            1.0, //
            0.46350446,
            0.88389223,
            0.76532131,
            0.0, //
            -1.43735918,
            -0.00143735918,
            1.72483102,
            0.0,
        ];

        // Forward transform -- input to expected.
        let func_data =
            FixedFunctionOpData::with_params(Rec2100SurroundFwd, params.clone()).unwrap();
        apply_fixed_function(&mut output_32f, &expected_32f, &func_data, 4e-7, false);

        // Inverse transform -- output back to original.
        let func_data_inv = FixedFunctionOpData::with_params(Rec2100SurroundInv, params).unwrap();
        apply_fixed_function(&mut output_32f, &input_32f, &func_data_inv, 3e-7, false);
    }
    {
        let params: Params = vec![1.2];

        let mut output_32f = input_32f;

        let expected_32f: [f32; 20] = [
            1.331310281667e-05,
            3.803743661907e-06,
            2.218850469446e-05,
            0.1, //
            0.059115925805,
            0.010748350146,
            0.021496700293,
            0.5, //
            0.636785774786,
            0.457409500198,
            0.726473912080,
            1.0, //
            0.401647721515,
            0.765932864285,
            0.663185772735,
            0.0, //
            -7.190495367684e-01,
            -7.190495367684e-04,
            8.628594441221e-01,
            0.0,
        ];

        // Forward transform -- input to expected.
        let func_data =
            FixedFunctionOpData::with_params(Rec2100SurroundFwd, params.clone()).unwrap();
        apply_fixed_function(&mut output_32f, &expected_32f, &func_data, 2e-7, false);

        // Inverse transform -- output back to original.
        let func_data_inv = FixedFunctionOpData::with_params(Rec2100SurroundInv, params).unwrap();
        apply_fixed_function(&mut output_32f, &input_32f, &func_data_inv, 2e-7, false);
    }
}

/// Port of `OCIO_ADD_TEST(FixedFunctionOpCPU, RGB_TO_HSV)` @ v2.5.2.
#[test]
fn rgb_to_hsv() {
    #[rustfmt::skip]
    let hsv_frame: Vec<f32> = vec![
         3./12.,  0.80,  2.50,  0.50,      // val > 1
        11./12.,  1.20,  2.50,  1.00,      // sat > 1
        15./24.,  0.80, -2.00,  0.25,      // val < 0
        19./24.,  1.50, -0.40,  0.25,      // sat > 1, val < 0
       -89./24.,  0.50,  0.40,  2.00,      // under-range hue
        81./24.,  1.50, -0.40, -0.25,      // over-range hue, sat > 1, val < 0
        81./24., -0.50,  0.40,  0.00,      // sat < 0
         0.5000,  2.50,  0.04,  0.00,      // sat > 2
    ];

    #[rustfmt::skip]
    let rgb_frame: Vec<f32> = vec![
        1.500,   2.500,   0.500,   0.50,
        3.125,  -0.625,   1.250,   1.00,
       -5./3., -4./3., -1./3.,  0.25,
        0.100,  -0.800,   0.400,   0.25,
        0.250,   0.400,   0.200,   2.00,
       -0.800,   0.400,  -0.500,  -0.25,
        0.400,   0.400,   0.400,   0.00,
       -39.96,   40.00,   40.00,   0.00,
    ];

    let data_fwd = data(RgbToHsv);

    let num_rgb = 4; // only the first 4 are relevant for RGB --> HSV
    let mut img = rgb_frame.clone();
    apply_fixed_function(
        &mut img[..num_rgb * 4],
        &hsv_frame[..num_rgb * 4],
        &data_fwd,
        1e-6,
        false,
    );

    let data_f_inv = data(HsvToRgb);

    let num_hsv = 7; // not using the last one as it requires a looser tolerance
    let mut img = hsv_frame.clone();
    apply_fixed_function(
        &mut img[..num_hsv * 4],
        &rgb_frame[..num_hsv * 4],
        &data_f_inv,
        1e-6,
        false,
    );
}

/// Port of `OCIO_ADD_TEST(FixedFunctionOpCPU, RGB_TO_HSY_LIN)` @ v2.5.2.
#[test]
fn rgb_to_hsy_lin() {
    #[rustfmt::skip]
    let hsy_frame: Vec<f32> = vec![
         0.470554752,    9.12594033,   0.0326650218,  0.,   // hsy alpha == 1
         0.75,           0.22196741,   0.38596,       0.,
         0.08333333,     0.12976444,   0.034974,      0.,
         0.333333333333, 0.606036032,  0.0056680,     1.,   // hsy mid alpha
         0.241666666667, 0.8372990325, 0.0034440,     1.,
         0.734693877551, 0.752099600,  0.0005572,     0.,   // hsy alpha == 0
         0.96296296,     9.7034,      -0.1862,        0.,
         0.730158730159, 0.811517000, -0.0009310,     0.,
    ];

    #[rustfmt::skip]
    let rgb_frame: Vec<f32> = vec![
        -0.075290,  0.078996, -0.108397, 0.,
         0.3,       0.4,       0.5,      0.,
         0.05,      0.03,      0.04,     0.,
         0.01,      0.01,     -0.05,     1.,
         0.05,     -0.005,    -0.05,     1.,
        -0.048,     0.01,      0.05,     0.,
         0.3,      -0.4,       0.5,      0.,
        -0.055,     0.01,      0.05,     0.,
    ];

    let mut img = rgb_frame.clone();
    apply_fixed_function(&mut img, &hsy_frame, &data(RgbToHsyLin), 1e-6, false);

    let mut img = hsy_frame.clone();
    apply_fixed_function(&mut img, &rgb_frame, &data(HsyLinToRgb), 1e-6, false);
}

/// Port of `OCIO_ADD_TEST(FixedFunctionOpCPU, RGB_TO_HSY_LOG)` @ v2.5.2.
#[test]
fn rgb_to_hsy_log() {
    let hsy_frame: Vec<f32> = vec![
        14563.0 / 65535.,
        64392.0 / 65535.,
        20899.0 / 65535.,
        32007.0 / 65535.,
        50061.0 / 65535.,
        64310.0 / 65535.,
        6328.0 / 65535.,
        65535.0 / 65535.,
    ];

    let rgb_frame: Vec<f32> = vec![
        1800.0 / 4095.,
        1200.0 / 4095.,
        900.0 / 4095.,
        2000.0 / 4095.,
        40.0 / 4095.,
        440.0 / 4095.,
        1000.0 / 4095.,
        4095.0 / 4095.,
    ];

    let mut img = rgb_frame.clone();
    apply_fixed_function(&mut img, &hsy_frame, &data(RgbToHsyLog), 1e-5, false);

    let mut img = hsy_frame.clone();
    apply_fixed_function(&mut img, &rgb_frame, &data(HsyLogToRgb), 1e-5, false);
}

/// Port of `OCIO_ADD_TEST(FixedFunctionOpCPU, RGB_TO_HSY_VID)` @ v2.5.2.
#[test]
fn rgb_to_hsy_vid() {
    let hsy_frame: Vec<f32> = vec![
        0.54190051555634,
        1.0851333141327,
        0.55111348628998,
        0.48840048909187,
        0.54262113571167,
        1.4824789762497,
        1.1281162500381,
        1.,
    ];

    let rgb_frame: Vec<f32> = vec![
        0.12152557820082,
        0.70731294155121,
        0.26879417896271,
        0.48840048909187,
        0.53938156366348,
        1.3418402671814,
        0.74459171295166,
        1.,
    ];

    let mut img = rgb_frame.clone();
    apply_fixed_function(&mut img, &hsy_frame, &data(RgbToHsyVid), 1e-6, false);

    let mut img = hsy_frame.clone();
    apply_fixed_function(&mut img, &rgb_frame, &data(HsyVidToRgb), 1e-6, false);
}

/// Port of `OCIO_ADD_TEST(FixedFunctionOpCPU, XYZ_TO_xyY)` @ v2.5.2.
#[test]
fn xyz_to_xyy() {
    let input_frame: Vec<f32> = vec![
        3600.0 / 4095.0,
        250.0 / 4095.0,
        900.0 / 4095.0,
        2000.0 / 4095.0,
        400.0 / 4095.0,
        3000.0 / 4095.0,
        4000.0 / 4095.0,
        4095.0 / 4095.0,
    ];

    let output_frame: Vec<f32> = vec![
        49669.0 / 65535.0,
        3449.0 / 65535.0,
        4001.0 / 65535.0,
        32007.0 / 65535.0,
        3542.0 / 65535.0,
        26568.0 / 65535.0,
        48011.0 / 65535.0,
        65535.0 / 65535.0,
    ];

    let mut img = input_frame.clone();
    apply_fixed_function(&mut img, &output_frame, &data(XyzToXyy), 1e-5, false);

    let mut img = output_frame.clone();
    apply_fixed_function(&mut img, &input_frame, &data(XyyToXyz), 1e-4, false);
}

/// Port of `OCIO_ADD_TEST(FixedFunctionOpCPU, XYZ_TO_uvY)` @ v2.5.2.
#[test]
fn xyz_to_uvy() {
    let input_frame: Vec<f32> = vec![
        3600.0 / 4095.0,
        350.0 / 4095.0,
        1900.0 / 4095.0,
        2000.0 / 4095.0,
        400.0 / 4095.0,
        3000.0 / 4095.0,
        4000.0 / 4095.0,
        4095.0 / 4095.0,
    ];

    let output_frame: Vec<f32> = vec![
        64859.0 / 65535.0,
        14188.0 / 65535.0,
        5601.0 / 65535.0,
        32007.0 / 65535.0,
        1827.0 / 65535.0,
        30827.0 / 65535.0,
        48011.0 / 65535.0,
        65535.0 / 65535.0,
    ];

    let mut img = input_frame.clone();
    apply_fixed_function(&mut img, &output_frame, &data(XyzToUvy), 1e-5, false);

    let mut img = output_frame.clone();
    apply_fixed_function(&mut img, &input_frame, &data(UvyToXyz), 1e-4, false);
}

/// Port of `OCIO_ADD_TEST(FixedFunctionOpCPU, XYZ_TO_LUV)` @ v2.5.2.
#[test]
fn xyz_to_luv() {
    let input_frame: Vec<f32> = vec![
        3600.0 / 4095.0,
        3500.0 / 4095.0,
        1900.0 / 4095.0,
        2000.0 / 4095.0,
        50.0 / 4095.0,
        30.0 / 4095.0,
        19.0 / 4095.0,
        4095.0 / 4095.0, // below the L* break
    ];

    let output_frame: Vec<f32> = vec![
        61659.0 / 65535.0,
        28199.0 / 65535.0,
        33176.0 / 65535.0,
        32007.0 / 65535.0,
        4337.0 / 65535.0,
        9090.0 / 65535.0,
        926.0 / 65535.0,
        65535.0 / 65535.0,
    ];

    let mut img = input_frame.clone();
    apply_fixed_function(&mut img, &output_frame, &data(XyzToLuv), 1e-5, false);

    let mut img = output_frame.clone();
    apply_fixed_function(&mut img, &input_frame, &data(LuvToXyz), 1e-5, false);
}

/// U-31: the gamut compression's renderer reads seven parameters and the Rec.2100 surround's
/// one, which upstream does without a check; the port refuses data with fewer (set after the
/// validating constructor). The styles whose renderers come later are refused, in both
/// fast-math settings.
#[test]
fn short_params_and_unported_styles_are_refused() {
    let params: Params = vec![1.147, 1.264, 1.312, 0.815, 0.803, 0.880, 1.2];
    for style in [AcesGamutComp13Fwd, AcesGamutComp13Inv] {
        let mut data = FixedFunctionOpData::with_params(style, params.clone()).unwrap();
        data.set_params(params[..6].to_vec());
        check_throw_what(
            get_fixed_function_cpu_renderer(&data, false).map(|_| ()),
            SHORT_PARAMS,
        );
    }
    for style in [Rec2100SurroundFwd, Rec2100SurroundInv] {
        let mut data = FixedFunctionOpData::with_params(style, vec![0.78]).unwrap();
        data.set_params(Vec::new());
        check_throw_what(
            get_fixed_function_cpu_renderer(&data, false).map(|_| ()),
            SHORT_PARAMS,
        );
    }
    for fast in [false, true] {
        let data = FixedFunctionOpData::new(LinToPq).unwrap();
        check_throw_what(
            get_fixed_function_cpu_renderer(&data, fast).map(|_| ()),
            "the CPU renderer of the style 'Lin_TO_PQ' is not ported yet",
        );
    }
}
