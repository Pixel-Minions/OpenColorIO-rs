// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Ported `tests/cpu/SSE_tests.cpp` @ v2.5.2, and a lane-by-lane cross-check of the scalar port
//! against `SSE.h`'s four-lane code executed by the CPU.
//!
//! Upstream's `sse2_*` tests evaluate a four-lane function on `_mm_set1_ps(x)` and check every
//! lane. The port computes one lane; the four lanes of the C++ are the same computation on the
//! same value, so each check is made once.

use super::*;
use crate::math_utils::add_ulp;
use ocio_testkit::upstream::{are_all_close, check_close, equal_with_abs_error};

// ---------------------------------------------------------------------------------------------
// Helpers of SSE_tests.cpp.

/// `IsInfinity` (tests/cpu/SSE_tests.cpp:52-58 @ v2.5.2): either infinity.
fn is_infinity(x: f32) -> bool {
    x.is_infinite()
}

/// `CheckFloat` (tests/cpu/SSE_tests.cpp:60-75 @ v2.5.2): infinities of either sign match each
/// other, NaNs match each other, anything else must be within 2^-precision (absolute).
#[track_caller]
fn check_float(operation: &str, expected: f32, actual: f32, precision: u32) {
    if (is_infinity(expected) && is_infinity(actual)) || (expected.is_nan() && actual.is_nan()) {
        return;
    }
    let rtol = 2.0f32.powf(-(precision as f32));
    assert!(
        equal_with_abs_error(expected, actual, rtol),
        "Output differs on {operation} : expected: {expected:e} != actual: {actual:e}"
    );
}

// ---------------------------------------------------------------------------------------------
// Ported upstream tests.

/// Port of `OCIO_ADD_TEST(SSE, sse2_log2_test)` @ v2.5.2.
#[test]
fn sse2_log2_test() {
    let values: [f32; 8] = [
        1e-010f32, 0.1f32, 0.5f32, 1.0f32, 11.0f32, 112.0f32, 2425.0f32, 2e015f32,
    ];

    // The sse approx should have about 15 good digits of mantissa.
    let rtol = 2.0f32.powf(-14.0f32);

    for v in values {
        let cpu_result = v.ln() / 2.0f32.ln();
        let sse_result = sse_log2(v);
        check_close(cpu_result, sse_result, rtol);
    }
}

/// `CheckPower` (tests/cpu/SSE_tests.cpp:88-102 @ v2.5.2).
#[track_caller]
fn check_power(base: f32, exponent: f32) {
    let cpu_result = base.powf(exponent);
    let sse_result = sse_power(base, exponent);
    check_float(
        &format!("power({base} , {exponent})"),
        cpu_result,
        sse_result,
        12,
    );
}

/// Port of `OCIO_ADD_TEST(SSE, sse2_power_test)` @ v2.5.2.
#[test]
fn sse2_power_test() {
    let values: [f32; 8] = [
        1e-010f32, 0.1f32, 0.5f32, 1.0f32, 0.7f32, 0.112f32, 0.2425f32, 0.3f32,
    ];
    for v in values {
        check_power(v, 10.0f32);
    }
}

/// Port of `OCIO_ADD_TEST(SSE, sse2_exp2_test)` @ v2.5.2.
#[test]
fn sse2_exp2_test() {
    let ulp_tolerance: u32 = 50;

    let values: [f32; 24] = [
        1e-5f32, 1e-10f32, 1e-15f32, 1e-20f32, //
        0.005f32, 0.1f32, 0.5f32, 1.0f32, //
        0.67f32, 0.112f32, 0.2425f32, 0.33f32, //
        1.5f32, 3.2f32, 7.11f32, 13.23f32, //
        27.001f32, 32.513f32, 44.999f32, 56.191f32, //
        61.0019f32, 77.7f32, 83.654f32, 98.989f32,
    ];

    // Check positive test values.
    for v in values {
        let expected = 2.0f32.powf(v);
        let result = sse_exp2(v);
        assert!(
            are_all_close(&[result], expected, ulp_tolerance),
            "exp2({v}): result {result:e}, expected {expected:e}"
        );
    }

    // Check negative test values.
    for v in values {
        let expected = 2.0f32.powf(-v);
        let result = sse_exp2(-v);
        assert!(
            are_all_close(&[result], expected, ulp_tolerance),
            "exp2({}): result {result:e}, expected {expected:e}",
            -v
        );
    }

    //
    // Check for edge cases
    //

    // log2_max_float should be exactly 128.0f.
    let log2_max_float = ((f32::MAX as f64).ln() / 2.0f64.ln()) as f32;

    // log2_min_float should be exactly -126.0f.
    let log2_min_float = ((f32::MIN_POSITIVE as f64).ln() / 2.0f64.ln()) as f32;

    // Check the log2_max_float and log2_min_float limits.
    assert!(is_infinity(sse_exp2(log2_max_float)));
    assert!(sse_exp2(log2_min_float) == 0.0f32);

    // The valid domain of exp2 is actually reduced by one ULP. Verify that the
    // log2_max_float and log2_min_float limits, contracted by one ULP, return valid
    // representable floating-point numbers.
    //
    // Note: We want log2_min_float_inside_one_ulp to be -125.9999..., but since addULP ignores
    // the sign and just modifies the mantissa, we actually need to subtract one.
    let log2_max_float_inside_one_ulp = add_ulp(log2_max_float, -1);
    let log2_min_float_inside_one_ulp = add_ulp(log2_min_float, -1);
    {
        // The result should be a large number, but not infinity. Create a tight bound for the
        // large number based on the log2_max_float limit.
        let large_threshold = 2.0f64.powf(add_ulp(log2_max_float, -2) as f64) as f32;

        let r = sse_exp2(log2_max_float_inside_one_ulp);
        assert!(r > large_threshold && r < f32::INFINITY, "{r:e}");

        // The result should be a small number, but not zero. Create a tight bound for the
        // small number based on the log2_min_float limit.
        let small_threshold = 2.0f64.powf(add_ulp(log2_min_float, -2) as f64) as f32;

        let r = sse_exp2(log2_min_float_inside_one_ulp);
        assert!(r > 0.0f32 && r < small_threshold, "{r:e}");
    }

    // Verify that the log2_max_float and log2_min_float limits, expanded by one ULP, still
    // return Infinity and zero, respectively.
    //
    // Note: As above, it is perhaps counter-intuitive, but we want to make
    // log2_min_float_outside_one_ulp just slightly more negative than -126 and so need to
    // increment the mantissa.
    let log2_max_float_outside_one_ulp = add_ulp(log2_max_float, 1);
    let log2_min_float_outside_one_ulp = add_ulp(log2_min_float, 1);
    {
        assert!(is_infinity(sse_exp2(log2_max_float_outside_one_ulp)));
        assert!(sse_exp2(log2_min_float_outside_one_ulp) == 0.0f32);
    }
}

/// The tangent test values of the atan tests (tests/cpu/SSE_tests.cpp:302-314 @ v2.5.2).
fn atan_values() -> [f32; 24] {
    let tan_pi_3 = 1.7320508075688772935274463415059_f64 as f32;
    let tan_pi_4 = 1.0_f64 as f32;
    let tan_pi_6 = 0.57735026918962576450914878050196_f64 as f32;
    let tan_pi_12 = 0.26794919243112270647255365849413_f64 as f32;
    [
        0.0f32, 1e-20f32, 1e-10f32, 1e-5f32, //
        0.005f32, 0.1f32, 0.5f32, 1.0f32, //
        tan_pi_3, tan_pi_4, tan_pi_6, tan_pi_12, //
        1.5f32, 3.2f32, 7.11f32, 13.23f32, //
        27.001f32, 32.513f32, 44.999f32, 56.191f32, //
        61.0019f32, 77.7f32, 83.654f32, 98.989f32,
    ]
}

/// The test values of the atan2 tests, for both `x` and `y`
/// (tests/cpu/SSE_tests.cpp:380-402 @ v2.5.2).
fn atan2_values() -> [f32; 24] {
    let tan_pi_3 = 1.7320508075688772935274463415059_f64 as f32;
    let tan_pi_4 = 1.0_f64 as f32;
    let tan_pi_6 = 0.57735026918962576450914878050196_f64 as f32;
    let tan_pi_12 = 0.26794919243112270647255365849413_f64 as f32;
    [
        0.0f32, 1e-20f32, 1e-15f32, 1e-10f32, //
        0.005f32, 0.1f32, 0.5f32, 1.0f32, //
        tan_pi_3, tan_pi_4, tan_pi_6, tan_pi_12, //
        1.5f32, 3.2f32, 7.11f32, 13.23f32, //
        27.001f32, 32.513f32, 44.999f32, 56.191f32, //
        61.0019f32, 77.7f32, 83.654f32, 98.989f32,
    ]
}

const SIGN_VALUES: [f32; 2] = [-1.0f32, 1.0f32];

/// Port of `OCIO_ADD_TEST(SSE, sse2_atan_test)` @ v2.5.2.
#[test]
fn sse2_atan_test() {
    for s in SIGN_VALUES {
        for v in atan_values() {
            let x = s * v;
            let expected = x.atan();
            check_float(&format!("atan({x})"), expected, sse_atan(x), 14);
        }
    }
}

/// Port of `OCIO_ADD_TEST(SSE, scalar_atan_test)` @ v2.5.2.
#[test]
fn scalar_atan_test() {
    for s in SIGN_VALUES {
        for v in atan_values() {
            let x = s * v;
            let expected = x.atan();
            check_float(&format!("atan({x})"), expected, sse_atan_scalar(x), 14);
        }
    }
}

/// Port of `OCIO_ADD_TEST(SSE, sse2_atan2_test)` @ v2.5.2.
#[test]
fn sse2_atan2_test() {
    for si in SIGN_VALUES {
        for vx in atan2_values() {
            let x = si * vx;
            for sj in SIGN_VALUES {
                for vy in atan2_values() {
                    let y = sj * vy;
                    let expected = y.atan2(x);
                    check_float(&format!("atan2({y} , {x})"), expected, sse_atan2(y, x), 14);
                }
            }
        }
    }
}

/// Port of `OCIO_ADD_TEST(SSE, scalar_atan2_test)` @ v2.5.2.
#[test]
fn scalar_atan2_test() {
    for si in SIGN_VALUES {
        for vx in atan2_values() {
            let x = si * vx;
            for sj in SIGN_VALUES {
                for vy in atan2_values() {
                    let y = sj * vy;
                    let expected = y.atan2(x);
                    let result = sse_atan2_scalar(y, x);
                    check_float(&format!("atan2({y} , {x})"), expected, result, 14);
                }
            }
        }
    }
}

/// The angles of the cos, sin and sincos tests (tests/cpu/SSE_tests.cpp:494-511 @ v2.5.2).
fn angle_values() -> [f32; 28] {
    let three_pi = 9.4247779607693797153879301498385_f64 as f32;
    let two_pi = 6.283185307179586476925286766559_f64 as f32;
    let pi = 3.1415926535897932384626433832795_f64 as f32;
    let pi_2 = 1.5707963267948966192313216916398_f64 as f32;
    let pi_3 = 1.0471975511965977461542144610932_f64 as f32;
    let pi_4 = 0.78539816339744830961566084581988_f64 as f32;
    let pi_6 = 0.52359877559829887307710723054658_f64 as f32;
    let pi_12 = 0.26179938779914943653855361527329_f64 as f32;
    [
        0.0f32, 1e-20f32, 1e-10f32, 1e-5f32, //
        0.005f32, 0.1f32, 0.5f32, 1.0f32, //
        0.67f32, 0.112f32, 0.2425f32, 0.33f32, //
        pi, pi_2, pi_3, pi_4, //
        pi_6, pi_12, two_pi, three_pi, //
        27.001f32, 32.513f32, 44.999f32, 56.191f32, //
        61.0019f32, 77.7f32, 83.654f32, 98.989f32,
    ]
}

/// Port of `OCIO_ADD_TEST(SSE, sse2_cos_test)` @ v2.5.2.
#[test]
fn sse2_cos_test() {
    for s in SIGN_VALUES {
        for v in angle_values() {
            let x = s * v;
            check_float(&format!("cos({x})"), x.cos(), sse_cos(x), 16);
        }
    }
}

/// Port of `OCIO_ADD_TEST(SSE, sse2_sin_test)` @ v2.5.2.
#[test]
fn sse2_sin_test() {
    for s in SIGN_VALUES {
        for v in angle_values() {
            let x = s * v;
            check_float(&format!("sin({x})"), x.sin(), sse_sin(x), 16);
        }
    }
}

/// Port of `OCIO_ADD_TEST(SSE, sse2_sin_cos_test)` @ v2.5.2.
#[test]
fn sse2_sin_cos_test() {
    for s in SIGN_VALUES {
        for v in angle_values() {
            let x = s * v;
            let (sin_x, cos_x) = sse_sin_cos(x);
            check_float(&format!("sincos({x})"), x.sin(), sin_x, 16);
            check_float(&format!("sincos({x})"), x.cos(), cos_x, 16);
        }
    }
}

/// Port of `OCIO_ADD_TEST(SSE, scalar_sin_cos_test)` @ v2.5.2.
#[test]
fn scalar_sin_cos_test() {
    for s in SIGN_VALUES {
        for v in angle_values() {
            let x = s * v;
            let (sin_x, cos_x) = sse_sin_cos_scalar(x);
            check_float(&format!("sincos({x})"), x.sin(), sin_x, 16);
            check_float(&format!("sincos({x})"), x.cos(), cos_x, 16);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Cross-check against SSE.h's four-lane code, executed by the CPU.
//
// `reference` transliterates SSE.h's `__m128` functions call for call into `core::arch`
// intrinsics. It exercises what the scalar port emulates by hand: the conversions' rounding and
// out-of-range results, the compare masks, the selects and the bit tricks, four different
// values at a time. It is a cross-check of the emulation, not an oracle: the wheel is the
// oracle, and the Log and Gamma oracle tests check sseLog2, sseExp2 and ssePower against it.

#[cfg(target_arch = "x86_64")]
mod reference {
    use std::arch::x86_64::*;

    use super::super::{
        E_1_PI, E_PI, E_PI_2, PN_ATAN_A1, PN_ATAN_A2, PN_ATAN_A3, PN_ATAN_B1, PN_ATAN_B2,
        PN_ATAN_B3, PN_COS_C1, PN_COS_C2, PN_COS_C3, PN_COS_C4, PN_COS_C5, PNEXP0, PNEXP1, PNEXP2,
        PNEXP3, PNEXP4, PNLOG0, PNLOG1, PNLOG2, PNLOG3, PNLOG4, PNLOG5,
    };

    #[target_feature(enable = "sse2")]
    fn emask() -> __m128i {
        _mm_set1_epi32(0x7F80_0000)
    }

    #[target_feature(enable = "sse2")]
    fn ebias() -> __m128i {
        _mm_set1_epi32(127)
    }

    #[target_feature(enable = "sse2")]
    fn eone() -> __m128 {
        _mm_set1_ps(1.0)
    }

    #[target_feature(enable = "sse2")]
    fn ezero() -> __m128 {
        _mm_set1_ps(0.0)
    }

    #[target_feature(enable = "sse2")]
    fn sign_mask() -> __m128 {
        _mm_castsi128_ps(_mm_set1_epi32(0x8000_0000_u32 as i32))
    }

    #[target_feature(enable = "sse2")]
    fn abs_mask() -> __m128 {
        _mm_castsi128_ps(_mm_set1_epi32(0x7fff_ffff))
    }

    #[target_feature(enable = "sse2")]
    fn is_negative_special(x: __m128) -> __m128 {
        _mm_castsi128_ps(_mm_srai_epi32::<31>(_mm_castps_si128(x)))
    }

    #[target_feature(enable = "sse2")]
    fn sse_select(mask: __m128, arg_true: __m128, arg_false: __m128) -> __m128 {
        _mm_xor_ps(arg_false, _mm_and_ps(mask, _mm_xor_ps(arg_true, arg_false)))
    }

    #[target_feature(enable = "sse2")]
    pub(super) fn sse_log2(x: __m128) -> __m128 {
        let mantissa = _mm_or_ps(_mm_andnot_ps(_mm_castsi128_ps(emask()), x), eone());

        let mut log2 = _mm_add_ps(
            _mm_mul_ps(
                _mm_add_ps(
                    _mm_mul_ps(
                        _mm_add_ps(
                            _mm_mul_ps(
                                _mm_add_ps(
                                    _mm_mul_ps(
                                        _mm_add_ps(
                                            _mm_mul_ps(_mm_set1_ps(PNLOG5), mantissa),
                                            _mm_set1_ps(PNLOG4),
                                        ),
                                        mantissa,
                                    ),
                                    _mm_set1_ps(PNLOG3),
                                ),
                                mantissa,
                            ),
                            _mm_set1_ps(PNLOG2),
                        ),
                        mantissa,
                    ),
                    _mm_set1_ps(PNLOG1),
                ),
                mantissa,
            ),
            _mm_set1_ps(PNLOG0),
        );

        let exponent = _mm_sub_epi32(
            _mm_srli_epi32::<23>(_mm_and_si128(_mm_castps_si128(x), emask())),
            ebias(),
        );

        log2 = _mm_add_ps(log2, _mm_cvtepi32_ps(exponent));
        log2
    }

    #[target_feature(enable = "sse2")]
    pub(super) fn sse_exp2(x: __m128) -> __m128 {
        let floor_x = _mm_add_epi32(
            _mm_cvttps_epi32(x),
            _mm_castps_si128(_mm_cmpnle_ps(ezero(), x)),
        );

        let zf = _mm_castsi128_ps(_mm_slli_epi32::<23>(_mm_add_epi32(floor_x, ebias())));

        let iexp = _mm_cvtepi32_ps(floor_x);
        let fraction = _mm_sub_ps(x, iexp);

        let mexp = _mm_add_ps(
            _mm_mul_ps(
                _mm_add_ps(
                    _mm_mul_ps(
                        _mm_add_ps(
                            _mm_mul_ps(
                                _mm_add_ps(
                                    _mm_mul_ps(_mm_set1_ps(PNEXP4), fraction),
                                    _mm_set1_ps(PNEXP3),
                                ),
                                fraction,
                            ),
                            _mm_set1_ps(PNEXP2),
                        ),
                        fraction,
                    ),
                    _mm_set1_ps(PNEXP1),
                ),
                fraction,
            ),
            _mm_set1_ps(PNEXP0),
        );

        let mut exp2 = _mm_mul_ps(zf, mexp);
        exp2 = _mm_andnot_ps(_mm_cmplt_ps(x, _mm_set1_ps(-126.0)), exp2);
        exp2 = sse_select(
            _mm_cmpge_ps(x, _mm_set1_ps(128.0)),
            _mm_set1_ps(f32::INFINITY),
            exp2,
        );
        exp2
    }

    #[target_feature(enable = "sse2")]
    pub(super) fn sse_power(x: __m128, exp: __m128) -> __m128 {
        let mut values = sse_log2(x);
        values = _mm_mul_ps(exp, values);
        values = sse_exp2(values);
        _mm_and_ps(values, _mm_cmpgt_ps(x, ezero()))
    }

    #[target_feature(enable = "sse2")]
    pub(super) fn sse_atan(x: __m128) -> __m128 {
        let sign_x = _mm_and_ps(x, sign_mask());
        let abs_x = _mm_and_ps(x, abs_mask());

        let inv_mask = _mm_cmpgt_ps(abs_x, eone());
        let inv_abs_x = _mm_div_ps(eone(), abs_x);
        let norm_x = sse_select(inv_mask, inv_abs_x, abs_x);

        let norm_x2 = _mm_mul_ps(norm_x, norm_x);

        let num = _mm_mul_ps(
            _mm_add_ps(
                _mm_mul_ps(
                    _mm_add_ps(
                        _mm_mul_ps(norm_x2, _mm_set1_ps(PN_ATAN_A3)),
                        _mm_set1_ps(PN_ATAN_A2),
                    ),
                    norm_x2,
                ),
                _mm_set1_ps(PN_ATAN_A1),
            ),
            norm_x,
        );

        let denom = _mm_add_ps(
            _mm_mul_ps(
                _mm_add_ps(
                    _mm_mul_ps(_mm_add_ps(norm_x2, _mm_set1_ps(PN_ATAN_B3)), norm_x2),
                    _mm_set1_ps(PN_ATAN_B2),
                ),
                norm_x2,
            ),
            _mm_set1_ps(PN_ATAN_B1),
        );

        let mut res = _mm_div_ps(num, denom);
        res = sse_select(inv_mask, _mm_sub_ps(_mm_set1_ps(E_PI_2), res), res);
        _mm_or_ps(sign_x, res)
    }

    #[target_feature(enable = "sse2")]
    pub(super) fn sse_atan2(y: __m128, x: __m128) -> __m128 {
        let mut res = sse_atan(_mm_div_ps(y, x));

        let zero_mask = _mm_or_ps(_mm_cmpneq_ps(x, ezero()), _mm_cmpneq_ps(y, ezero()));
        res = _mm_and_ps(res, zero_mask);

        let neg_x = is_negative_special(x);
        let sign_y = _mm_and_ps(y, sign_mask());

        _mm_add_ps(res, _mm_and_ps(_mm_or_ps(sign_y, _mm_set1_ps(E_PI)), neg_x))
    }

    /// `__sse_cos__`: (cos_x, xr, xr2, flip_sign_cos_x).
    #[target_feature(enable = "sse2")]
    fn sse_cos_parts(x: __m128) -> (__m128, __m128, __m128, __m128) {
        let cycles = _mm_cvtps_epi32(_mm_mul_ps(x, _mm_set1_ps(E_1_PI)));

        let xr = _mm_sub_ps(x, _mm_mul_ps(_mm_cvtepi32_ps(cycles), _mm_set1_ps(E_PI)));
        let xr2 = _mm_mul_ps(xr, xr);

        let mut cos_x = _mm_add_ps(
            _mm_mul_ps(
                _mm_add_ps(
                    _mm_mul_ps(
                        _mm_add_ps(
                            _mm_mul_ps(
                                _mm_add_ps(
                                    _mm_mul_ps(_mm_set1_ps(PN_COS_C5), xr2),
                                    _mm_set1_ps(PN_COS_C4),
                                ),
                                xr2,
                            ),
                            _mm_set1_ps(PN_COS_C3),
                        ),
                        xr2,
                    ),
                    _mm_set1_ps(PN_COS_C2),
                ),
                xr2,
            ),
            _mm_set1_ps(PN_COS_C1),
        );

        let flip_sign_cos_x = _mm_castsi128_ps(_mm_slli_epi32::<31>(cycles));
        cos_x = _mm_xor_ps(cos_x, flip_sign_cos_x);
        (cos_x, xr, xr2, flip_sign_cos_x)
    }

    #[target_feature(enable = "sse2")]
    pub(super) fn sse_cos(x: __m128) -> __m128 {
        sse_cos_parts(x).0
    }

    #[target_feature(enable = "sse2")]
    pub(super) fn sse_sin(x: __m128) -> __m128 {
        sse_cos(_mm_sub_ps(_mm_set1_ps(E_PI_2), x))
    }

    /// `sseSinCos`: (sin_x, cos_x).
    #[target_feature(enable = "sse2")]
    pub(super) fn sse_sin_cos(x: __m128) -> (__m128, __m128) {
        let (cos_x, xr, xr2, flip_sign_cos_x) = sse_cos_parts(x);
        let mut sin_x2 = _mm_sub_ps(eone(), _mm_mul_ps(cos_x, cos_x));
        sin_x2 = sse_select(
            _mm_cmpgt_ps(xr2, _mm_set1_ps(0.00006103515625)),
            sin_x2,
            xr2,
        );
        let mut sin_x = _mm_sqrt_ps(sin_x2);
        let xr_sign = _mm_and_ps(xr, sign_mask());
        let flip_sign_sin_x = _mm_xor_ps(flip_sign_cos_x, xr_sign);
        sin_x = _mm_xor_ps(sin_x, flip_sign_sin_x);
        (sin_x, cos_x)
    }

    #[target_feature(enable = "sse2")]
    pub(super) fn load(v: [f32; 4]) -> __m128 {
        _mm_setr_ps(v[0], v[1], v[2], v[3])
    }

    pub(super) fn store(v: __m128) -> [f32; 4] {
        let mut out = [0.0f32; 4];
        // SAFETY: `out` is 16 writable bytes; `_mm_storeu_ps` has no alignment requirement and
        // SSE is part of the x86-64 baseline.
        unsafe { _mm_storeu_ps(out.as_mut_ptr(), v) };
        out
    }
}

/// Inputs for the cross-check: every half, the specials, seeded random bit patterns (NaNs of
/// both signs with payloads, infinities, subnormals) and seeded values in the ranges the
/// functions reduce to.
#[cfg(target_arch = "x86_64")]
fn cross_check_inputs() -> Vec<f32> {
    use ocio_testkit::probe::{Rng, all_half_values, special_values, uniform};
    let mut v = all_half_values();
    v.extend(special_values());
    let mut rng = Rng::new(0x5ee5_0001);
    v.extend((0..400_000).map(|_| rng.any_bits()));
    v.extend(uniform(0x5ee5_0002, 200_000, -200.0, 200.0));
    v.extend(uniform(0x5ee5_0003, 100_000, -2.0, 2.0));
    // Around the sseExp2 range limits and the i32 conversion limits.
    for edge in [-126.0f32, 128.0f32, 2_147_483_648.0f32, -2_147_483_648.0f32] {
        for d in -8..=8 {
            v.push(add_ulp(edge, d));
        }
    }
    v
}

#[cfg(target_arch = "x86_64")]
#[track_caller]
fn assert_lanes(name: &str, inputs: &[f32; 4], reference: [f32; 4], port: [f32; 4]) {
    for lane in 0..4 {
        assert_eq!(
            port[lane].to_bits(),
            reference[lane].to_bits(),
            "{name} lane {lane}: input {:e} ({:#010x}), SSE {:e} ({:#010x}), port {:e} ({:#010x})",
            inputs[lane],
            inputs[lane].to_bits(),
            reference[lane],
            reference[lane].to_bits(),
            port[lane],
            port[lane].to_bits()
        );
    }
}

/// Each lane of the scalar port equals the corresponding lane of `SSE.h`'s four-lane code
/// (transliterated into `core::arch` intrinsics), bit for bit, over every probe input.
#[cfg(target_arch = "x86_64")]
#[test]
fn lanes_match_the_four_lane_code() {
    let inputs = cross_check_inputs();
    // Exponents for ssePower: the gammas upstream's tests and typical configs use, plus random.
    let mut rng = ocio_testkit::probe::Rng::new(0x5ee5_0004);
    for (i, chunk) in inputs.as_chunks::<4>().0.iter().enumerate() {
        let x: [f32; 4] = [chunk[0], chunk[1], chunk[2], chunk[3]];
        let y: [f32; 4] = [chunk[3], chunk[0], chunk[1], chunk[2]];
        let exponents: [f32; 4] = [
            [1.0f32, 2.2f32, 1.0f32 / 2.4f32, 0.45f32][i % 4],
            rng.uniform(0.01, 100.0),
            rng.uniform(-4.0, 4.0),
            rng.any_bits(),
        ];
        // SAFETY (all `unsafe` blocks below): the reference functions only need SSE2, which is
        // part of the x86-64 baseline.
        let (vx, vy, ve) = unsafe {
            (
                reference::load(x),
                reference::load(y),
                reference::load(exponents),
            )
        };

        let r = reference::store(unsafe { reference::sse_log2(vx) });
        assert_lanes("sseLog2", &x, r, x.map(sse_log2));

        let r = reference::store(unsafe { reference::sse_exp2(vx) });
        assert_lanes("sseExp2", &x, r, x.map(sse_exp2));

        let r = reference::store(unsafe { reference::sse_power(vx, ve) });
        let port = [0, 1, 2, 3].map(|l| sse_power(x[l], exponents[l]));
        assert_lanes("ssePower", &x, r, port);

        let r = reference::store(unsafe { reference::sse_atan(vx) });
        assert_lanes("sseAtan", &x, r, x.map(sse_atan));

        let r = reference::store(unsafe { reference::sse_atan2(vy, vx) });
        let port = [0, 1, 2, 3].map(|l| sse_atan2(y[l], x[l]));
        assert_lanes("sseAtan2", &x, r, port);

        let r = reference::store(unsafe { reference::sse_cos(vx) });
        assert_lanes("sseCos", &x, r, x.map(sse_cos));

        let r = reference::store(unsafe { reference::sse_sin(vx) });
        assert_lanes("sseSin", &x, r, x.map(sse_sin));

        let (rs, rc) = unsafe { reference::sse_sin_cos(vx) };
        let port = x.map(sse_sin_cos);
        assert_lanes("sseSinCos sin", &x, reference::store(rs), port.map(|p| p.0));
        assert_lanes("sseSinCos cos", &x, reference::store(rc), port.map(|p| p.1));
    }
}

/// The scalar `sseSinCos` is two lanes of `sseCos`: `(x, PI/2 - x)`.
#[cfg(target_arch = "x86_64")]
#[test]
fn scalar_sin_cos_matches_two_cos_lanes() {
    const F_PI_2: f32 = 1.57079632679489661923_f64 as f32;
    for x in cross_check_inputs() {
        let lanes = [x, F_PI_2 - x, 0.0, 0.0];
        // SAFETY: SSE2 is part of the x86-64 baseline.
        let r = reference::store(unsafe { reference::sse_cos(reference::load(lanes)) });
        let (sin_x, cos_x) = sse_sin_cos_scalar(x);
        assert_eq!(cos_x.to_bits(), r[0].to_bits(), "cos({x:e})");
        assert_eq!(sin_x.to_bits(), r[1].to_bits(), "sin({x:e})");
    }
}

/// The lane helpers of `math_utils` match the x86 instructions they stand for, bit for bit:
/// `MAXPS`, `MINPS`, `CVTPS2DQ` and `CVTTPS2DQ`.
#[cfg(target_arch = "x86_64")]
#[test]
fn math_utils_lane_helpers_match_the_instructions() {
    use crate::math_utils::{sse_cvtps_epi32, sse_cvttps_epi32, sse_max, sse_min};
    use std::arch::x86_64::_mm_min_ps;
    use std::arch::x86_64::{_mm_castsi128_ps, _mm_cvtps_epi32, _mm_cvttps_epi32, _mm_max_ps};

    let inputs = cross_check_inputs();
    for chunk in inputs.as_chunks::<4>().0 {
        let a: [f32; 4] = [chunk[0], chunk[1], chunk[2], chunk[3]];
        let b: [f32; 4] = [chunk[1], chunk[0], chunk[3], chunk[3]];
        // SAFETY: SSE2 is part of the x86-64 baseline.
        let (va, vb) = unsafe { (reference::load(a), reference::load(b)) };

        let r = reference::store(unsafe { _mm_max_ps(va, vb) });
        let port = [0, 1, 2, 3].map(|l| sse_max(a[l], b[l]));
        assert_lanes("_mm_max_ps", &a, r, port);

        let r = reference::store(unsafe { _mm_min_ps(va, vb) });
        let port = [0, 1, 2, 3].map(|l| sse_min(a[l], b[l]));
        assert_lanes("_mm_min_ps", &a, r, port);

        // The conversions, compared as integers through a bit cast.
        let r = reference::store(unsafe { _mm_castsi128_ps(_mm_cvtps_epi32(va)) });
        let port = a.map(|v| f32::from_bits(sse_cvtps_epi32(v) as u32));
        assert_lanes("_mm_cvtps_epi32", &a, r, port);

        let r = reference::store(unsafe { _mm_castsi128_ps(_mm_cvttps_epi32(va)) });
        let port = a.map(|v| f32::from_bits(sse_cvttps_epi32(v) as u32));
        assert_lanes("_mm_cvttps_epi32", &a, r, port);
    }
}

/// The x86 additions and multiplications, run through `asm!`: in Intel syntax,
/// `addps x, y` computes `x + y` with `x` as the first source operand. (The `core::arch`
/// intrinsics `_mm_add_ps`/`_mm_mul_ps` are plain additions and multiplications to LLVM, which
/// may swap their operands: the behaviour `sse_add`/`sse_mul` exist to avoid.)
#[cfg(target_arch = "x86_64")]
mod instructions {
    use std::arch::asm;
    use std::arch::x86_64::__m128;

    macro_rules! instruction {
        ($name:ident, $mnemonic:literal, $t:ty) => {
            pub(super) fn $name(a: $t, b: $t) -> $t {
                let mut x = a;
                // SAFETY: the instruction only reads and writes the two registers, and SSE2 is
                // part of the x86-64 baseline.
                unsafe {
                    asm!(
                        concat!($mnemonic, " {x}, {y}"),
                        x = inout(xmm_reg) x,
                        y = in(xmm_reg) b,
                        options(pure, nomem, nostack, preserves_flags)
                    );
                }
                x
            }
        };
    }

    instruction!(addps, "addps", __m128);
    instruction!(mulps, "mulps", __m128);
    instruction!(addss, "addss", f32);
    instruction!(mulss, "mulss", f32);
    instruction!(addsd, "addsd", f64);
    instruction!(mulsd, "mulsd", f64);
}

/// `f32` operands for the NaN pairs: NaNs of both signs, quiet and signalling, with and
/// without payloads, then the other special values.
#[cfg(target_arch = "x86_64")]
const NAN_PAIR_VALUES_F32: [u32; 23] = [
    0x7fc0_0000, // quiet NaN
    0xffc0_0000, // the x86 default NaN
    0x7fc1_2345,
    0xffc1_2345,
    0x7f80_0001, // signalling NaNs
    0xff80_0001,
    0x7fbf_ffff,
    0xffbf_ffff,
    0x7fff_ffff,
    0xffff_ffff,
    0x7f80_0000, // infinities
    0xff80_0000,
    0x0000_0000, // zeros
    0x8000_0000,
    0x3f80_0000, // ±1
    0xbf80_0000,
    0x7f7f_ffff, // ±FLT_MAX
    0xff7f_ffff,
    0x0080_0000, // FLT_MIN
    0x0000_0001, // subnormals
    0x8000_0001,
    0x3dcc_cccd, // 0.1
    0xc060_0000, // -3.5
];

/// `f64` operands for the NaN pairs, as [`NAN_PAIR_VALUES_F32`].
#[cfg(target_arch = "x86_64")]
const NAN_PAIR_VALUES_F64: [u64; 23] = [
    0x7ff8_0000_0000_0000, // quiet NaN
    0xfff8_0000_0000_0000, // the x86 default NaN
    0x7ff8_1234_5678_9abc,
    0xfff8_1234_5678_9abc,
    0x7ff0_0000_0000_0001, // signalling NaNs
    0xfff0_0000_0000_0001,
    0x7ff7_ffff_ffff_ffff,
    0xfff7_ffff_ffff_ffff,
    0x7fff_ffff_ffff_ffff,
    0xffff_ffff_ffff_ffff,
    0x7ff0_0000_0000_0000, // infinities
    0xfff0_0000_0000_0000,
    0x0000_0000_0000_0000, // zeros
    0x8000_0000_0000_0000,
    0x3ff0_0000_0000_0000, // ±1
    0xbff0_0000_0000_0000,
    0x7fef_ffff_ffff_ffff, // ±DBL_MAX
    0xffef_ffff_ffff_ffff,
    0x0010_0000_0000_0000, // DBL_MIN
    0x0000_0000_0000_0001, // subnormals
    0x8000_0000_0000_0001,
    0x3fb9_9999_9999_999a, // 0.1
    0xc00c_0000_0000_0000, // -3.5
];

#[cfg(target_arch = "x86_64")]
#[track_caller]
fn assert_same_bits(name: &str, a: u64, b: u64, instruction: u64, port: u64) {
    assert_eq!(
        port, instruction,
        "{name}({a:#x}, {b:#x}): instruction {instruction:#x}, port {port:#x}"
    );
}

/// `math_utils::sse_add` and `sse_mul` match `ADDPS`/`MULPS` and `ADDSS`/`MULSS` on `f32`, and
/// `ADDSD`/`MULSD` on `f64`, bit for bit: for every ordered pair of the NaN and special values
/// (for two NaNs, the result is the first operand's NaN, quieted), and for pairs of the
/// cross-check inputs.
#[cfg(target_arch = "x86_64")]
#[test]
fn math_utils_arithmetic_helpers_match_the_instructions() {
    use crate::math_utils::{sse_add, sse_mul};
    use std::hint::black_box;

    let specials = NAN_PAIR_VALUES_F32.map(f32::from_bits);
    let mut pairs: Vec<(f32, f32)> = specials
        .iter()
        .flat_map(|&a| specials.map(|b| (a, b)))
        .collect();
    let inputs = cross_check_inputs();
    pairs.extend(
        inputs
            .iter()
            .zip(inputs.iter().rev())
            .map(|(&a, &b)| (a, b)),
    );

    for chunk in pairs.as_chunks::<4>().0 {
        let a = chunk.map(|p| p.0);
        let b = chunk.map(|p| p.1);
        // SAFETY: SSE2 is part of the x86-64 baseline.
        let (va, vb) = unsafe { (reference::load(a), reference::load(b)) };
        let sums = reference::store(instructions::addps(va, vb));
        let products = reference::store(instructions::mulps(va, vb));
        for l in 0..4 {
            let (x, y) = (u64::from(a[l].to_bits()), u64::from(b[l].to_bits()));
            let port = sse_add(black_box(a[l]), black_box(b[l]));
            assert_same_bits(
                "addps",
                x,
                y,
                u64::from(sums[l].to_bits()),
                u64::from(port.to_bits()),
            );
            let port = sse_mul(black_box(a[l]), black_box(b[l]));
            assert_same_bits(
                "mulps",
                x,
                y,
                u64::from(products[l].to_bits()),
                u64::from(port.to_bits()),
            );
        }
    }
    for &(a, b) in &pairs {
        let (x, y) = (u64::from(a.to_bits()), u64::from(b.to_bits()));
        let port = sse_add(black_box(a), black_box(b));
        let sum = instructions::addss(a, b);
        assert_same_bits(
            "addss",
            x,
            y,
            u64::from(sum.to_bits()),
            u64::from(port.to_bits()),
        );
        let port = sse_mul(black_box(a), black_box(b));
        let product = instructions::mulss(a, b);
        assert_same_bits(
            "mulss",
            x,
            y,
            u64::from(product.to_bits()),
            u64::from(port.to_bits()),
        );
    }

    let specials = NAN_PAIR_VALUES_F64.map(f64::from_bits);
    let mut pairs: Vec<(f64, f64)> = specials
        .iter()
        .flat_map(|&a| specials.map(|b| (a, b)))
        .collect();
    let mut rng = ocio_testkit::probe::Rng::new(0x5ee5_0005);
    pairs.extend((0..200_000).map(|_| {
        (
            f64::from_bits(rng.next_u64()),
            f64::from_bits(rng.next_u64()),
        )
    }));
    for &(a, b) in &pairs {
        let (x, y) = (a.to_bits(), b.to_bits());
        let port = sse_add(black_box(a), black_box(b));
        assert_same_bits(
            "addsd",
            x,
            y,
            instructions::addsd(a, b).to_bits(),
            port.to_bits(),
        );
        let port = sse_mul(black_box(a), black_box(b));
        assert_same_bits(
            "mulsd",
            x,
            y,
            instructions::mulsd(a, b).to_bits(),
            port.to_bits(),
        );
    }
}
