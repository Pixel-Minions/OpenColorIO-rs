// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

use super::*;
use ocio_testkit::upstream::{check_close, check_equal};

/// Port of `OCIO_ADD_TEST(MathUtils, clamp)` @ v2.5.2.
#[test]
fn clamp_test() {
    assert_eq!(-1.0f32, clamp(f32::NAN, -1.0f32, 1.0f32));

    assert_eq!(10.0f32, clamp(f32::INFINITY, 5.0f32, 10.0f32));
    assert_eq!(5.0f32, clamp(-f32::INFINITY, 5.0f32, 10.0f32));

    assert_eq!(0.0000005f32, clamp(0.0000005f32, 0.0f32, 1.0f32));
    assert_eq!(0.0f32, clamp(-0.0000005f32, 0.0f32, 1.0f32));
    assert_eq!(1.0f32, clamp(1.0000005f32, 0.0f32, 1.0f32));
}

/// `std::max`/`std::min` return their first argument when the comparison is false, which is
/// how C++ code filters or keeps NaN depending on the argument order.
#[test]
fn std_min_max_keep_the_first_argument_on_false_comparisons() {
    let nan = f32::from_bits(0x7fc0_1234);
    assert_eq!(std_max(nan, 1.0f32).to_bits(), nan.to_bits());
    assert_eq!(std_max(1.0f32, nan).to_bits(), 1.0f32.to_bits());
    assert_eq!(std_min(nan, 1.0f32).to_bits(), nan.to_bits());
    assert_eq!(std_min(1.0f32, nan).to_bits(), 1.0f32.to_bits());
    // Equal values of either sign: the first argument.
    assert_eq!(std_max(-0.0f32, 0.0f32).to_bits(), (-0.0f32).to_bits());
    assert_eq!(std_min(0.0f32, -0.0f32).to_bits(), 0.0f32.to_bits());
}

/// `AddULP` wraps like the C++ unsigned arithmetic and ignores the sign.
#[test]
fn add_ulp_moves_the_bits() {
    let one = 1.0f32;
    assert_eq!(float_as_int(add_ulp(one, 1)), float_as_int(one) + 1);
    assert_eq!(float_as_int(add_ulp(one, -1)), float_as_int(one) - 1);
    assert_eq!(float_as_int(add_ulp(-one, 1)), float_as_int(-one) + 1);
    assert_eq!(float_as_int(add_ulp(int_as_float(u32::MAX), 1)), 0);
}

/// Port of `OCIO_ADD_TEST(MathUtils, is_scalar_equal_to_zero)` @ v2.5.2.
#[test]
fn is_scalar_equal_to_zero_test() {
    check_equal(is_scalar_equal_to_zero(0.0f32), true);
    check_equal(is_scalar_equal_to_zero(-0.0f32), true);

    check_equal(is_scalar_equal_to_zero(-1.072883670794056e-09f32), false);
    check_equal(is_scalar_equal_to_zero(1.072883670794056e-09f32), false);

    check_equal(is_scalar_equal_to_zero(-1.072883670794056e-03f32), false);
    check_equal(is_scalar_equal_to_zero(1.072883670794056e-03f32), false);

    check_equal(is_scalar_equal_to_zero(-1.072883670794056e-01f32), false);
    check_equal(is_scalar_equal_to_zero(1.072883670794056e-01f32), false);
}

/// `GetMxbResult(vout, m, x, v)`: `m x + v` (tests/cpu/MathUtils_tests.cpp:14-21 @ v2.5.2).
fn get_mxb_result(m: &[f32; 16], x: &[f32; 4], v: &[f32; 4]) -> [f32; 4] {
    let vout = get_m44_v4_product(m, x);
    get_v4_sum(&vout, v)
}

/// Port of `OCIO_ADD_TEST(MathUtils, get_m44_inverse)` @ v2.5.2.
#[test]
fn get_m44_inverse_test() {
    // This is a degenerate matrix, and shouldn't be invertible.
    #[rustfmt::skip]
    let m: [f32; 16] = [0.3, 0.3, 0.3, 0.0,
                        0.3, 0.3, 0.3, 0.0,
                        0.3, 0.3, 0.3, 0.0,
                        0.0, 0.0, 0.0, 1.0];

    let invertsuccess = get_m44_inverse(&m).is_some();
    check_equal(invertsuccess, false);
}

/// Port of `OCIO_ADD_TEST(MathUtils, m44_m44_product)` @ v2.5.2.
#[test]
fn m44_m44_product() {
    #[rustfmt::skip]
    let m1: [f32; 16] = [1.0, 2.0, 0.0, 0.0,
                         0.0, 1.0, 1.0, 0.0,
                         1.0, 0.0, 1.0, 0.0,
                         0.0, 1.0, 3.0, 1.0];
    #[rustfmt::skip]
    let m2: [f32; 16] = [1.0, 1.0, 0.0, 0.0,
                         0.0, 1.0, 0.0, 0.0,
                         0.0, 0.0, 1.0, 0.0,
                         2.0, 0.0, 0.0, 1.0];
    let mout = get_m44_m44_product(&m1, &m2);

    #[rustfmt::skip]
    let mcorrect: [f32; 16] = [1.0, 3.0, 0.0, 0.0,
                               0.0, 1.0, 1.0, 0.0,
                               1.0, 1.0, 1.0, 0.0,
                               2.0, 1.0, 3.0, 1.0];

    for i in 0..16 {
        check_equal(mout[i], mcorrect[i]);
    }
}

/// Port of `OCIO_ADD_TEST(MathUtils, m44_v4_product)` @ v2.5.2.
#[test]
fn m44_v4_product() {
    #[rustfmt::skip]
    let m: [f32; 16] = [1.0, 2.0, 0.0, 0.0,
                        0.0, 1.0, 1.0, 0.0,
                        1.0, 0.0, 1.0, 0.0,
                        0.0, 1.0, 3.0, 1.0];
    let v: [f32; 4] = [1.0, 2.0, 3.0, 4.0];
    let vout = get_m44_v4_product(&m, &v);

    let vcorrect: [f32; 4] = [5.0, 5.0, 4.0, 15.0];

    for i in 0..4 {
        check_equal(vout[i], vcorrect[i]);
    }
}

/// Port of `OCIO_ADD_TEST(MathUtils, v4_add)` @ v2.5.2.
#[test]
fn v4_add() {
    let v1: [f32; 4] = [1.0, 2.0, 3.0, 4.0];
    let v2: [f32; 4] = [3.0, 1.0, 4.0, 1.0];
    let vout = get_v4_sum(&v1, &v2);

    let vcorrect: [f32; 4] = [4.0, 3.0, 7.0, 5.0];

    for i in 0..4 {
        check_equal(vout[i], vcorrect[i]);
    }
}

/// Port of `OCIO_ADD_TEST(MathUtils, mxb_eval)` @ v2.5.2.
#[test]
fn mxb_eval() {
    #[rustfmt::skip]
    let m: [f32; 16] = [1.0, 2.0, 0.0, 0.0,
                        0.0, 1.0, 1.0, 0.0,
                        1.0, 0.0, 1.0, 0.0,
                        0.0, 1.0, 3.0, 1.0];
    let x: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
    let v: [f32; 4] = [1.0, 2.0, 3.0, 4.0];
    let vout = get_mxb_result(&m, &x, &v);

    let vcorrect: [f32; 4] = [4.0, 4.0, 5.0, 9.0];

    for i in 0..4 {
        check_equal(vout[i], vcorrect[i]);
    }
}

/// Port of `OCIO_ADD_TEST(MathUtils, combine_two_mxb)` @ v2.5.2.
///
/// `OCIO_CHECK_CLOSE(x, y, 1e-3)` subtracts the floats and compares with a `double`; the port's
/// `check_close` takes one type, so the last block widens the floats to `double` first. That is
/// equivalent: the two results are within a factor of 2 of each other, so their `float`
/// difference is already exact (Sterbenz's lemma).
#[test]
fn combine_two_mxb() {
    #[rustfmt::skip]
    let m1: [f32; 16] = [1.0, 0.0, 2.0, 0.0,
                         2.0, 1.0, 0.0, 1.0,
                         0.0, 1.0, 2.0, 0.0,
                         1.0, 0.0, 0.0, 1.0];
    let v1: [f32; 4] = [1.0, 2.0, 3.0, 4.0];
    #[rustfmt::skip]
    let m2: [f32; 16] = [2.0, 1.0, 0.0, 0.0,
                         0.0, 1.0, 0.0, 0.0,
                         1.0, 0.0, 3.0, 0.0,
                         1.0, 1.0, 1.0, 1.0];
    let v2: [f32; 4] = [0.0, 2.0, 1.0, 0.0];
    let tolerance = 1e-9f32;

    {
        let x: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

        // Combine two mx+b operations, and apply to test point
        let (mout, vout) = get_mxb_combine(&m1, &v1, &m2, &v2);
        let vcombined = get_mxb_result(&mout, &x, &vout);

        // Sequentially apply the two mx+b operations.
        let vout = get_mxb_result(&m1, &x, &v1);
        let vout = get_mxb_result(&m2, &vout, &v2);

        // Compare outputs
        for i in 0..4 {
            check_close(vcombined[i], vout[i], tolerance);
        }
    }

    {
        let x: [f32; 4] = [6.0, 0.5, -2.0, -0.1];

        let (mout, vout) = get_mxb_combine(&m1, &v1, &m2, &v2);
        let vcombined = get_mxb_result(&mout, &x, &vout);

        let vout = get_mxb_result(&m1, &x, &v1);
        let vout = get_mxb_result(&m2, &vout, &v2);

        for i in 0..4 {
            check_close(vcombined[i], vout[i], tolerance);
        }
    }

    {
        let x: [f32; 4] = [26.0, -0.5, 0.005, 12.1];

        let (mout, vout) = get_mxb_combine(&m1, &v1, &m2, &v2);
        let vcombined = get_mxb_result(&mout, &x, &vout);

        let vout = get_mxb_result(&m1, &x, &v1);
        let vout = get_mxb_result(&m2, &vout, &v2);

        // We pick a not so small tolerance, as we're dealing with
        // large numbers, and the error for CHECK_CLOSE is absolute.
        for i in 0..4 {
            check_close(f64::from(vcombined[i]), f64::from(vout[i]), 1e-3);
        }
    }
}

/// Port of `OCIO_ADD_TEST(MathUtils, mxb_invert)` @ v2.5.2.
#[test]
fn mxb_invert() {
    {
        #[rustfmt::skip]
        let m: [f32; 16] = [1.0, 2.0, 0.0, 0.0,
                            0.0, 1.0, 1.0, 0.0,
                            1.0, 0.0, 1.0, 0.0,
                            0.0, 1.0, 3.0, 1.0];
        let x: [f32; 4] = [1.0, 0.5, -1.0, 60.0];
        let v: [f32; 4] = [1.0, 2.0, 3.0, 4.0];

        let vresult = get_mxb_result(&m, &x, &v);
        let inverse = get_mxb_inverse(&m, &v);
        let invertsuccess = inverse.is_some();
        check_equal(invertsuccess, true);
        let (mout, vout) = inverse.expect("invertible");

        let vresult = get_mxb_result(&mout, &vresult, &vout);

        let tolerance = 1e-9f32;
        for i in 0..4 {
            check_close(vresult[i], x[i], tolerance);
        }
    }

    {
        #[rustfmt::skip]
        let m: [f32; 16] = [0.3, 0.3, 0.3, 0.0,
                            0.3, 0.3, 0.3, 0.0,
                            0.3, 0.3, 0.3, 0.0,
                            0.0, 0.0, 0.0, 1.0];
        let v: [f32; 4] = [0.0, 0.0, 0.0, 0.0];

        let invertsuccess = get_mxb_inverse(&m, &v).is_some();
        check_equal(invertsuccess, false);
    }
}

// ---------------------------------------------------------------------------------------------
// The infrastructure of upstream's FloatsDiffer tests (tests/cpu/MathUtils_tests.cpp:245-605
// @ v2.5.2).

const KEEP_DENORMS: bool = false;
const COMPRESS_DENORMS: bool = true;

const POSINF: f32 = f32::INFINITY;
const NEGINF: f32 = f32::NEG_INFINITY;
/// `std::numeric_limits<float>::quiet_NaN()`: 0x7fc00000 in both standard libraries.
const QNAN: f32 = f32::NAN;
/// `std::numeric_limits<float>::signaling_NaN()`: `__builtin_nansf("1")` in the MSVC STL,
/// `__builtin_nansf("")` in libstdc++. `FloatsDiffer` treats every NaN alike.
const SNAN: f32 = if cfg!(windows) {
    f32::from_bits(0x7f80_0001)
} else {
    f32::from_bits(0x7fa0_0000)
};

const POSMAXFLOAT: f32 = f32::MAX;
const NEGMAXFLOAT: f32 = -f32::MAX;

/// `std::numeric_limits<float>::min()`: the smallest positive normal float.
const POSMINFLOAT: f32 = f32::MIN_POSITIVE;
const NEGMINFLOAT: f32 = -f32::MIN_POSITIVE;

const ZERO: f32 = 0.0;
const NEGZERO: f32 = -0.0;

const POSONE: f32 = 1.0;
const NEGONE: f32 = -1.0;

const POSRANDOM: f32 = 12.345;
const NEGRANDOM: f32 = -12.345;

/// Upstream's `tol`.
const TOL: i32 = 8;

/// A reference value moved by 1, 4, 7, 8, 9 and 16 ULPs: the variables that upstream's
/// `DECLARE_FLOAT_PLUS_ULP` and `DECLARE_FLOAT_MINUS_ULP` declare, named `<prefix><ulps>`.
struct Ulps {
    u1: f32,
    u4: f32,
    u7: f32,
    u8: f32,
    u9: f32,
    u16: f32,
}

/// `DECLARE_FLOAT_PLUS_ULP(prefix, reference, 1, 4, 7, 8, 9, 16)`.
fn plus(reference: f32) -> Ulps {
    Ulps {
        u1: add_ulp(reference, 1),
        u4: add_ulp(reference, 4),
        u7: add_ulp(reference, 7),
        u8: add_ulp(reference, 8),
        u9: add_ulp(reference, 9),
        u16: add_ulp(reference, 16),
    }
}

/// `DECLARE_FLOAT_MINUS_ULP(prefix, reference, 1, 4, 7, 8, 9, 16)`.
fn minus(reference: f32) -> Ulps {
    Ulps {
        u1: add_ulp(reference, -1),
        u4: add_ulp(reference, -4),
        u7: add_ulp(reference, -7),
        u8: add_ulp(reference, -8),
        u9: add_ulp(reference, -9),
        u16: add_ulp(reference, -16),
    }
}

/// The variables upstream declares with `DECLARE_FLOAT_RANGE_ULP` and `DECLARE_FLOAT_PLUS_ULP`
/// (tests/cpu/MathUtils_tests.cpp:306-334 @ v2.5.2).
struct Values {
    posinf_p: Ulps,
    posinf_m: Ulps,
    neginf_p: Ulps,
    neginf_m: Ulps,
    posmaxfloat_p: Ulps,
    posmaxfloat_m: Ulps,
    negmaxfloat_p: Ulps,
    negmaxfloat_m: Ulps,
    posminfloat_p: Ulps,
    posminfloat_m: Ulps,
    negminfloat_p: Ulps,
    negminfloat_m: Ulps,
    zero_p: Ulps,
    negzero_p: Ulps,
    posone_p: Ulps,
    posone_m: Ulps,
    negone_p: Ulps,
    negone_m: Ulps,
    posrandom_p: Ulps,
    posrandom_m: Ulps,
    negrandom_p: Ulps,
    negrandom_m: Ulps,
}

fn values() -> Values {
    Values {
        posinf_p: plus(POSINF),
        posinf_m: minus(POSINF),
        neginf_p: plus(NEGINF),
        neginf_m: minus(NEGINF),
        posmaxfloat_p: plus(POSMAXFLOAT),
        posmaxfloat_m: minus(POSMAXFLOAT),
        negmaxfloat_p: plus(NEGMAXFLOAT),
        negmaxfloat_m: minus(NEGMAXFLOAT),
        posminfloat_p: plus(POSMINFLOAT),
        posminfloat_m: minus(POSMINFLOAT),
        negminfloat_p: plus(NEGMINFLOAT),
        negminfloat_m: minus(NEGMINFLOAT),
        zero_p: plus(ZERO),
        negzero_p: plus(NEGZERO),
        posone_p: plus(POSONE),
        posone_m: minus(POSONE),
        negone_p: plus(NEGONE),
        negone_m: minus(NEGONE),
        posrandom_p: plus(POSRANDOM),
        posrandom_m: minus(POSRANDOM),
        negrandom_p: plus(NEGRANDOM),
        negrandom_m: minus(NEGRANDOM),
    }
}

/// `getErrorMessage*` (tests/cpu/MathUtils_tests.cpp:346-361, 415-428, 482-494 @ v2.5.2).
fn message(a: f32, b: f32, expected: &str, compress_denorms: bool) -> String {
    format!(
        "The values {a:e} ({:#x}) and {b:e} ({:#x}) are expected to be {expected} {}",
        float_as_int(a),
        float_as_int(b),
        if compress_denorms {
            "(when compressing denormalized numbers)."
        } else {
            "(when keeping denormalized numbers)."
        }
    )
}

/// `checkFloatsAreDifferent(ref, tolerance, compressDenorms, a, ...)`: each value differs from
/// `reference`, both ways (tests/cpu/MathUtils_tests.cpp:363-408 @ v2.5.2).
#[track_caller]
fn check_floats_are_different(
    reference: f32,
    tolerance: i32,
    compress_denorms: bool,
    values: &[f32],
) {
    for &a in values {
        let what = format!("DIFFERENT within a tolerance of {tolerance} ULPs");
        assert!(
            floats_differ(reference, a, tolerance, compress_denorms),
            "{}",
            message(reference, a, &what, compress_denorms)
        );
        assert!(
            floats_differ(a, reference, tolerance, compress_denorms),
            "{}",
            message(a, reference, &what, compress_denorms)
        );
    }
}

/// `checkFloatsAreClose(ref, tolerance, compressDenorms, a, ...)`: no value differs from
/// `reference`, either way (tests/cpu/MathUtils_tests.cpp:430-475 @ v2.5.2).
#[track_caller]
fn check_floats_are_close(reference: f32, tolerance: i32, compress_denorms: bool, values: &[f32]) {
    for &a in values {
        let what = format!("CLOSE within a tolerance of {tolerance} ULPs");
        assert!(
            !floats_differ(reference, a, tolerance, compress_denorms),
            "{}",
            message(reference, a, &what, compress_denorms)
        );
        assert!(
            !floats_differ(a, reference, tolerance, compress_denorms),
            "{}",
            message(a, reference, &what, compress_denorms)
        );
    }
}

/// `checkFloatsAreEqual(ref, compressDenorms, a, ...)`: no value differs from `reference` with
/// a tolerance of 0, either way (tests/cpu/MathUtils_tests.cpp:496-510 @ v2.5.2).
#[track_caller]
fn check_floats_are_equal(reference: f32, compress_denorms: bool, values: &[f32]) {
    for &a in values {
        assert!(
            !floats_differ(reference, a, 0, compress_denorms),
            "{}",
            message(reference, a, "EQUAL", compress_denorms)
        );
        assert!(
            !floats_differ(a, reference, 0, compress_denorms),
            "{}",
            message(a, reference, "EQUAL", compress_denorms)
        );
    }
}

/// `checkFloatsDenormInvariant(compressDenorms)` (tests/cpu/MathUtils_tests.cpp:512-603 @ v2.5.2).
///
/// Four of its calls pass `compressDenorms` as the tolerance and `tol` as the flag (lines 517,
/// 520, 558 and 573). They are ported as upstream wrote them, with C++'s conversions: the flag
/// becomes a tolerance of 0 or 1, and `tol` a flag of `true`.
fn check_floats_denorm_invariant(compress_denorms: bool) {
    let v = values();
    let swapped_tolerance = i32::from(compress_denorms);
    let swapped_flag = TOL != 0;

    check_floats_are_equal(POSINF, compress_denorms, &[POSINF]);
    check_floats_are_different(
        POSINF,
        swapped_tolerance,
        swapped_flag,
        &[NEGINF, QNAN, SNAN],
    );

    check_floats_are_equal(NEGINF, compress_denorms, &[NEGINF]);
    check_floats_are_different(NEGINF, swapped_tolerance, swapped_flag, &[QNAN, SNAN]);

    check_floats_are_equal(QNAN, compress_denorms, &[QNAN, SNAN]);
    check_floats_are_equal(SNAN, compress_denorms, &[SNAN]);

    // Check positive infinity limits
    //
    let p = &v.posinf_p;
    check_floats_are_different(
        POSINF,
        TOL,
        compress_denorms,
        &[p.u1, p.u4, p.u7, p.u8, p.u9, p.u16],
    );
    let m = &v.posinf_m;
    check_floats_are_different(
        POSINF,
        TOL,
        compress_denorms,
        &[m.u1, m.u4, m.u7, m.u8, m.u9, m.u16],
    );

    // Check negative infinity limits
    //
    let p = &v.neginf_p;
    check_floats_are_different(
        NEGINF,
        TOL,
        compress_denorms,
        &[p.u1, p.u4, p.u7, p.u8, p.u9, p.u16],
    );
    let m = &v.neginf_m;
    check_floats_are_different(
        NEGINF,
        TOL,
        compress_denorms,
        &[m.u1, m.u4, m.u7, m.u8, m.u9, m.u16],
    );

    // Check positive maximum float
    //
    check_floats_are_equal(POSMAXFLOAT, compress_denorms, &[v.posinf_m.u1]);
    check_floats_are_equal(v.posmaxfloat_p.u1, compress_denorms, &[POSINF]);

    let p = &v.posmaxfloat_p;
    check_floats_are_different(
        POSMAXFLOAT,
        TOL,
        compress_denorms,
        &[p.u1, p.u4, p.u7, p.u8, p.u9, p.u16],
    );

    let m = &v.posmaxfloat_m;
    check_floats_are_close(
        POSMAXFLOAT,
        TOL,
        compress_denorms,
        &[m.u1, m.u4, m.u7, m.u8],
    );

    check_floats_are_different(POSMAXFLOAT, swapped_tolerance, swapped_flag, &[m.u9, m.u16]);

    // Check negative maximum float
    //
    check_floats_are_equal(NEGMAXFLOAT, compress_denorms, &[v.neginf_m.u1]);
    check_floats_are_equal(v.negmaxfloat_p.u1, compress_denorms, &[NEGINF]);

    let p = &v.negmaxfloat_p;
    check_floats_are_different(
        NEGMAXFLOAT,
        TOL,
        compress_denorms,
        &[p.u1, p.u4, p.u7, p.u8, p.u9, p.u16],
    );

    let m = &v.negmaxfloat_m;
    check_floats_are_close(
        NEGMAXFLOAT,
        TOL,
        compress_denorms,
        &[m.u1, m.u4, m.u7, m.u8],
    );

    check_floats_are_different(NEGMAXFLOAT, swapped_tolerance, swapped_flag, &[m.u9, m.u16]);

    // Check zero and negative zero equality
    check_floats_are_equal(ZERO, compress_denorms, &[NEGZERO]);

    // Check positive and negative one
    let (p, m) = (&v.posone_p, &v.posone_m);
    check_floats_are_different(POSONE, TOL, compress_denorms, &[m.u16, m.u9]);
    check_floats_are_close(POSONE, TOL, compress_denorms, &[m.u8, m.u7, m.u4, m.u1]);
    check_floats_are_close(POSONE, TOL, compress_denorms, &[p.u1, p.u4, p.u7, p.u8]);
    check_floats_are_different(POSONE, TOL, compress_denorms, &[p.u9, p.u16]);

    let (p, m) = (&v.negone_p, &v.negone_m);
    check_floats_are_different(NEGONE, TOL, compress_denorms, &[m.u16, m.u9]);
    check_floats_are_close(NEGONE, TOL, compress_denorms, &[m.u8, m.u7, m.u4, m.u1]);
    check_floats_are_close(NEGONE, TOL, compress_denorms, &[p.u1, p.u4, p.u7, p.u8]);
    check_floats_are_different(NEGONE, TOL, compress_denorms, &[p.u9, p.u16]);

    // Check positive and negative random value
    let (p, m) = (&v.posrandom_p, &v.posrandom_m);
    check_floats_are_different(POSRANDOM, TOL, compress_denorms, &[m.u16, m.u9]);
    check_floats_are_close(POSRANDOM, TOL, compress_denorms, &[m.u8, m.u7, m.u4, m.u1]);
    check_floats_are_close(POSRANDOM, TOL, compress_denorms, &[p.u1, p.u4, p.u7, p.u8]);
    check_floats_are_different(POSRANDOM, TOL, compress_denorms, &[p.u9, p.u16]);

    let (p, m) = (&v.negrandom_p, &v.negrandom_m);
    check_floats_are_different(NEGRANDOM, TOL, compress_denorms, &[m.u16, m.u9]);
    check_floats_are_close(NEGRANDOM, TOL, compress_denorms, &[m.u8, m.u7, m.u4, m.u1]);
    check_floats_are_close(NEGRANDOM, TOL, compress_denorms, &[p.u1, p.u4, p.u7, p.u8]);
    check_floats_are_different(NEGRANDOM, TOL, compress_denorms, &[p.u9, p.u16]);
}

/// Port of `OCIO_ADD_TEST(MathUtils, float_diff_keep_denorms_test)` @ v2.5.2.
#[test]
fn float_diff_keep_denorms_test() {
    check_floats_denorm_invariant(KEEP_DENORMS);
    let v = values();
    let (zp, nzp) = (&v.zero_p, &v.negzero_p);

    // Check positive minimum float
    //
    let (p, m) = (&v.posminfloat_p, &v.posminfloat_m);
    check_floats_are_different(POSMINFLOAT, TOL, KEEP_DENORMS, &[m.u16, m.u9]);
    check_floats_are_close(POSMINFLOAT, TOL, KEEP_DENORMS, &[m.u8, m.u7, m.u4, m.u1]);
    check_floats_are_close(POSMINFLOAT, TOL, KEEP_DENORMS, &[p.u1, p.u4, p.u7, p.u8]);
    check_floats_are_different(POSMINFLOAT, TOL, KEEP_DENORMS, &[p.u9, p.u16]);

    // Check negative minimum float
    //
    let (p, m) = (&v.negminfloat_p, &v.negminfloat_m);
    check_floats_are_different(NEGMINFLOAT, TOL, KEEP_DENORMS, &[m.u16, m.u9]);
    check_floats_are_close(NEGMINFLOAT, TOL, KEEP_DENORMS, &[m.u8, m.u7, m.u4, m.u1]);
    check_floats_are_close(NEGMINFLOAT, TOL, KEEP_DENORMS, &[p.u1, p.u4, p.u7, p.u8]);
    check_floats_are_different(NEGMINFLOAT, TOL, KEEP_DENORMS, &[p.u9, p.u16]);

    // Compare zero and positive denorms
    check_floats_are_close(ZERO, TOL, KEEP_DENORMS, &[zp.u1, zp.u4, zp.u7, zp.u8]);
    check_floats_are_different(ZERO, TOL, KEEP_DENORMS, &[zp.u9, zp.u16]);

    // Compare zero and negative denorms
    check_floats_are_close(ZERO, TOL, KEEP_DENORMS, &[nzp.u1, nzp.u4, nzp.u7, nzp.u8]);
    check_floats_are_different(ZERO, TOL, KEEP_DENORMS, &[nzp.u9, nzp.u16]);

    // Compare negative zero and positive denorms
    check_floats_are_close(NEGZERO, TOL, KEEP_DENORMS, &[zp.u1, zp.u4, zp.u7, zp.u8]);
    check_floats_are_different(NEGZERO, TOL, KEEP_DENORMS, &[zp.u9, zp.u16]);

    // Compare negative zero and negative denorms
    check_floats_are_close(
        NEGZERO,
        TOL,
        KEEP_DENORMS,
        &[nzp.u1, nzp.u4, nzp.u7, nzp.u8],
    );
    check_floats_are_different(NEGZERO, TOL, KEEP_DENORMS, &[nzp.u9, nzp.u16]);

    // Compare positive denorms and negative denorms
    check_floats_are_close(zp.u1, TOL, KEEP_DENORMS, &[nzp.u1, nzp.u4, nzp.u7]);
    check_floats_are_different(zp.u1, TOL, KEEP_DENORMS, &[nzp.u8, nzp.u9, nzp.u16]);

    check_floats_are_close(zp.u4, TOL, KEEP_DENORMS, &[nzp.u1, nzp.u4]);
    check_floats_are_different(zp.u4, TOL, KEEP_DENORMS, &[nzp.u7, nzp.u8, nzp.u9, nzp.u16]);

    check_floats_are_different(
        zp.u9,
        TOL,
        KEEP_DENORMS,
        &[nzp.u1, nzp.u4, nzp.u7, nzp.u8, nzp.u9, nzp.u16],
    );

    check_floats_are_close(nzp.u1, TOL, KEEP_DENORMS, &[zp.u1, zp.u4, zp.u7]);
    check_floats_are_different(nzp.u1, TOL, KEEP_DENORMS, &[zp.u8, zp.u9, zp.u16]);

    check_floats_are_close(nzp.u4, TOL, KEEP_DENORMS, &[zp.u1, zp.u4]);
    check_floats_are_different(nzp.u4, TOL, KEEP_DENORMS, &[zp.u7, zp.u8, zp.u9, zp.u16]);

    check_floats_are_different(
        nzp.u9,
        TOL,
        KEEP_DENORMS,
        &[zp.u1, zp.u4, zp.u7, zp.u8, zp.u9, zp.u16],
    );

    // Compare negative and positive minimum floats
    //
    // Note: The float-point values being compared are expected to be different because there is
    //       the full set of denormalized values between zero and -/+MIN_FLOAT when denormalized
    //       values are kept.
    check_floats_are_different(
        POSMINFLOAT,
        TOL,
        KEEP_DENORMS,
        &[zp.u1, zp.u4, zp.u7, zp.u8, zp.u9, zp.u16],
    );

    check_floats_are_different(
        POSMINFLOAT,
        TOL,
        KEEP_DENORMS,
        &[nzp.u1, nzp.u4, nzp.u7, nzp.u8, nzp.u9, nzp.u16],
    );

    let (p, m) = (&v.negminfloat_p, &v.negminfloat_m);
    check_floats_are_different(
        POSMINFLOAT,
        TOL,
        KEEP_DENORMS,
        &[p.u1, p.u4, p.u7, p.u8, p.u9, p.u16],
    );

    check_floats_are_different(
        POSMINFLOAT,
        TOL,
        KEEP_DENORMS,
        &[m.u1, m.u4, m.u7, m.u8, m.u9, m.u16],
    );

    check_floats_are_different(
        NEGMINFLOAT,
        TOL,
        KEEP_DENORMS,
        &[zp.u1, zp.u4, zp.u7, zp.u8, zp.u9, zp.u16],
    );

    check_floats_are_different(
        NEGMINFLOAT,
        TOL,
        KEEP_DENORMS,
        &[nzp.u1, nzp.u4, nzp.u7, nzp.u8, nzp.u9, nzp.u16],
    );

    let (p, m) = (&v.posminfloat_p, &v.posminfloat_m);
    check_floats_are_different(
        NEGMINFLOAT,
        TOL,
        KEEP_DENORMS,
        &[p.u1, p.u4, p.u7, p.u8, p.u9, p.u16],
    );

    check_floats_are_different(
        NEGMINFLOAT,
        TOL,
        KEEP_DENORMS,
        &[m.u1, m.u4, m.u7, m.u8, m.u9, m.u16],
    );
}

/// Port of `OCIO_ADD_TEST(MathUtils, float_diff_compress_denorms_test)` @ v2.5.2.
#[test]
fn float_diff_compress_denorms_test() {
    check_floats_denorm_invariant(COMPRESS_DENORMS);
    let v = values();
    let (zp, nzp) = (&v.zero_p, &v.negzero_p);

    // Check positive minimum float
    //
    // Note: posminfloat_m* are mapped to zero when compressing denormalized values.
    let (p, m) = (&v.posminfloat_p, &v.posminfloat_m);
    check_floats_are_close(
        POSMINFLOAT,
        TOL,
        COMPRESS_DENORMS,
        &[m.u16, m.u9, m.u8, m.u7, m.u4, m.u1],
    );
    check_floats_are_close(
        POSMINFLOAT,
        TOL,
        COMPRESS_DENORMS,
        &[p.u1, p.u4, p.u7, p.u8],
    );
    check_floats_are_different(POSMINFLOAT, TOL, COMPRESS_DENORMS, &[p.u9, p.u16]);

    // Check negative minimum float
    //
    // Note: negminfloat_m* are mapped to zero when compressing denormalized values.
    let (p, m) = (&v.negminfloat_p, &v.negminfloat_m);
    check_floats_are_close(
        NEGMINFLOAT,
        TOL,
        COMPRESS_DENORMS,
        &[m.u16, m.u9, m.u8, m.u7, m.u4, m.u1],
    );
    check_floats_are_close(
        NEGMINFLOAT,
        TOL,
        COMPRESS_DENORMS,
        &[p.u1, p.u4, p.u7, p.u8],
    );
    check_floats_are_different(NEGMINFLOAT, TOL, COMPRESS_DENORMS, &[p.u9, p.u16]);

    let all_zp = [zp.u1, zp.u4, zp.u7, zp.u8, zp.u9, zp.u16];
    let all_nzp = [nzp.u1, nzp.u4, nzp.u7, nzp.u8, nzp.u9, nzp.u16];

    // Compare zero and positive denorms
    //
    // Note: zero_p* are mapped to zero when compressing denormalized values.
    check_floats_are_close(ZERO, TOL, COMPRESS_DENORMS, &all_zp);

    // Compare zero and negative denorms
    //
    // Note: negzero_p* are mapped to zero when compressing denormalized values.
    check_floats_are_close(ZERO, TOL, COMPRESS_DENORMS, &all_nzp);

    // Compare negative zero and positive denorms
    //
    // Note: zero_p* are mapped to zero when compressing denormalized values.
    check_floats_are_close(NEGZERO, TOL, COMPRESS_DENORMS, &all_zp);

    // Compare negative zero and negative denorms
    //
    // Note: negzero_p* are mapped to zero when compressing denormalized values.
    check_floats_are_close(NEGZERO, TOL, COMPRESS_DENORMS, &all_nzp);

    // Compare positive denorms and negative denorms
    //
    // Note: negzero_p* are mapped to zero when compressing denormalized values.
    check_floats_are_close(zp.u1, TOL, COMPRESS_DENORMS, &all_nzp);

    check_floats_are_close(zp.u4, TOL, COMPRESS_DENORMS, &all_nzp);

    check_floats_are_close(zp.u9, TOL, COMPRESS_DENORMS, &all_nzp);

    check_floats_are_close(nzp.u1, TOL, COMPRESS_DENORMS, &all_zp);

    check_floats_are_close(nzp.u4, TOL, COMPRESS_DENORMS, &all_zp);

    check_floats_are_close(nzp.u9, TOL, COMPRESS_DENORMS, &all_zp);

    // Compare negative and positive minimum floats
    //
    // Note: When compressing denorms, the mapped floating-point values ordering used for
    //       comparison becomes: ... , negminfloat , zero , posminfloat , ..., so the
    //       difference between negminfloat and posminfloat actually becomes 2 ULPs.
    //       Denormalized values like, zero_p*, negzero_p*, posminfloat_m*, negminfloat_m*
    //       are all mapped to zero.
    check_floats_are_close(ZERO, 1, COMPRESS_DENORMS, &[NEGMINFLOAT]);
    check_floats_are_close(ZERO, 1, COMPRESS_DENORMS, &[POSMINFLOAT]);
    check_floats_are_close(POSMINFLOAT, 2, COMPRESS_DENORMS, &[NEGMINFLOAT]);

    check_floats_are_close(POSMINFLOAT, TOL, COMPRESS_DENORMS, &all_nzp);

    let (p, m) = (&v.negminfloat_p, &v.negminfloat_m);
    check_floats_are_close(POSMINFLOAT, TOL, COMPRESS_DENORMS, &[p.u1, p.u4]);
    check_floats_are_different(
        POSMINFLOAT,
        TOL,
        COMPRESS_DENORMS,
        &[p.u7, p.u8, p.u9, p.u16],
    );

    check_floats_are_close(
        POSMINFLOAT,
        TOL,
        COMPRESS_DENORMS,
        &[m.u1, m.u4, m.u7, m.u8, m.u9, m.u16],
    );

    check_floats_are_close(NEGMINFLOAT, TOL, COMPRESS_DENORMS, &all_zp);

    check_floats_are_close(NEGMINFLOAT, TOL, COMPRESS_DENORMS, &all_nzp);

    let (p, m) = (&v.posminfloat_p, &v.posminfloat_m);
    check_floats_are_close(NEGMINFLOAT, TOL, COMPRESS_DENORMS, &[p.u1, p.u4]);
    check_floats_are_different(
        NEGMINFLOAT,
        TOL,
        COMPRESS_DENORMS,
        &[p.u7, p.u8, p.u9, p.u16],
    );

    check_floats_are_close(
        NEGMINFLOAT,
        TOL,
        COMPRESS_DENORMS,
        &[m.u1, m.u4, m.u7, m.u8, m.u9, m.u16],
    );
}

/// Port of `OCIO_ADD_TEST(MathUtils, half_bits_test)` @ v2.5.2.
#[test]
fn half_bits_test() {
    // Validation.
    assert_eq!(0.5f32, convert_half_bits_to_float(0x3800));

    // Preserve negatives.
    assert_eq!(-1.0f32, convert_half_bits_to_float(0xbc00));

    // Preserve values > 1.
    assert_eq!(1024.0f32, convert_half_bits_to_float(0x6400));
}

/// Port of `OCIO_ADD_TEST(MathUtils, halfs_differ_test)` @ v2.5.2.
#[test]
fn halfs_differ_test() {
    let bits = half::f16::from_bits;
    let pos_inf = bits(31744); // +inf
    let neg_inf = bits(64512); // -inf
    let pos_nan = bits(31745); // +nan
    let neg_nan = bits(64513); // -nan
    let pos_max = bits(31743); // +HALF_MAX
    let neg_max = bits(64511); // -HALF_MAX
    let pos_zero = bits(0); // +0
    let neg_zero = bits(32768); // -0
    let pos_small = bits(4); // +small
    let neg_small = bits(32772); // -small
    let pos_1 = bits(15360); //
    let pos_2 = bits(15365); //
    let neg_1 = bits(50000); //
    let neg_2 = bits(50005); //

    let tol = 10;

    assert!(halfs_differ(pos_inf, neg_inf, tol));
    assert!(halfs_differ(pos_inf, pos_nan, tol));
    assert!(halfs_differ(neg_inf, neg_nan, tol));
    assert!(halfs_differ(pos_max, pos_inf, tol));
    assert!(halfs_differ(neg_max, neg_inf, tol));
    assert!(halfs_differ(pos_1, neg_1, tol));
    assert!(halfs_differ(pos_2, pos_1, 0));
    assert!(halfs_differ(neg_2, neg_1, 0));

    assert!(!halfs_differ(pos_zero, neg_zero, 0));
    assert!(!halfs_differ(pos_small, neg_small, tol));
    assert!(!halfs_differ(pos_2, pos_1, tol));
    assert!(!halfs_differ(neg_2, neg_1, tol));
}

/// The exponents of 2 that the built-ins' LUTs raise (the ACEScc curve over its 4096 entries,
/// the Apple Log curve over every half code), and a spread of others.
fn pow2_exponents() -> Vec<f64> {
    let mut out = Vec::new();
    for i in 0..4096u32 {
        let in_ = f64::from(i) / 4095. * (1.50 - -0.36) + -0.36;
        out.push(in_ * 17.52 - 9.72);
    }
    for bits in 0..=u16::MAX {
        let v = f64::from(crate::imath_half::half_to_float(bits));
        if v.is_finite() {
            out.push((v - 0.69336945) / 0.08550479);
        }
    }
    for i in -2000..2000 {
        out.push(f64::from(i) * 0.0137);
    }
    out
}

/// [`std_pow`] and [`std_powf`] are libm's `pow` and `powf` in every build, including where
/// `exp2` gives other bits: the C runtime's `pow` is the reference (`ocio_testkit::crt`).
#[test]
fn std_pow_is_the_c_runtime_pow() {
    use ocio_testkit::crt::{exp2_c, exp2f_c, pow_c, powf_c};

    let mut exp2_differs = 0;
    for x in pow2_exponents() {
        let pow = pow_c(2.0, x);
        assert_eq!(std_pow(2.0, x).to_bits(), pow.to_bits(), "pow(2, {x:e})");
        if exp2_c(x).to_bits() != pow.to_bits() {
            exp2_differs += 1;
        }

        let xf = x as f32;
        let powf = powf_c(2.0, xf);
        assert_eq!(
            std_powf(2.0, xf).to_bits(),
            powf.to_bits(),
            "powf(2, {xf:e})"
        );
        if exp2f_c(xf).to_bits() != powf.to_bits() {
            exp2_differs += 1;
        }
    }
    // Both C runtimes have inputs where they differ, so the test tells `pow` from `exp2`.
    assert!(exp2_differs > 0, "exp2 equals pow on every input");
}

/// The places where a constant base 2 would let LLVM call `exp2` instead of `pow`: a literal 2
/// before `.powf(`, or as the first argument of `f32::powf`/`f64::powf`. Source files of the
/// workspace's crates use [`std_pow`] or [`std_powf`] instead (comments aside).
#[test]
fn no_powf_of_a_literal_2() {
    fn is_two(token: &str) -> bool {
        let t = token
            .trim_end_matches("f64")
            .trim_end_matches("f32")
            .trim_end_matches('_');
        !t.is_empty() && t.parse::<f64>() == Ok(2.0)
    }
    fn scan(dir: &std::path::Path, found: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                if path.file_name().is_some_and(|n| n != "target") {
                    scan(&path, found);
                }
                continue;
            }
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            for (n, line) in text.lines().enumerate() {
                if line.trim_start().starts_with("//") {
                    continue;
                }
                let method = [".", "powf("].concat();
                let mut rest = line;
                while let Some(at) = rest.find(&method) {
                    let receiver: String = rest[..at]
                        .chars()
                        .rev()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '_')
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect();
                    if is_two(&receiver) {
                        found.push(format!("{}:{}: {line}", path.display(), n + 1));
                    }
                    rest = &rest[at + method.len()..];
                }
                for ty in ["f32", "f64"] {
                    let call = [ty, "::", "powf("].concat();
                    if let Some(at) = line.find(&call) {
                        let arg: String = line[at + call.len()..]
                            .trim_start()
                            .chars()
                            .take_while(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '_')
                            .collect();
                        if is_two(&arg) {
                            found.push(format!("{}:{}: {line}", path.display(), n + 1));
                        }
                    }
                }
            }
        }
    }
    let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut found = Vec::new();
    scan(&crates, &mut found);
    assert!(
        found.is_empty(),
        "use std_pow or std_powf:\n{}",
        found.join("\n")
    );
}
