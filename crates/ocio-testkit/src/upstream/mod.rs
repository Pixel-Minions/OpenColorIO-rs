// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Upstream's own test checks, for **ported upstream tests only** (CLAUDE.md, rule 3).
//!
//! Upstream's C++ tests compare some results with tolerances. A ported test keeps upstream's
//! check and upstream's tolerance, through the functions here; each one mirrors one upstream
//! macro or helper and cites it. Nothing else in the port may compare with a tolerance:
//! oracle checks are exact (`crate::compare`).
//!
//! Upstream's `OCIO_CHECK_*` macros record a failure and let the test continue; these
//! functions panic at the first failure instead, which fails the same tests.

use std::fmt::{Debug, Display, LowerExp};
use std::ops::{Div, Neg, Sub};

/// The float types upstream's checks are instantiated with.
pub trait UpstreamFloat:
    Copy + PartialOrd + Debug + LowerExp + Sub<Output = Self> + Div<Output = Self> + Neg<Output = Self>
{
    /// `std::abs`.
    fn abs(self) -> Self;
    /// `std::isnan`.
    fn is_nan(self) -> bool;
    /// `0` of this type.
    fn zero() -> Self;
}

impl UpstreamFloat for f32 {
    fn abs(self) -> Self {
        f32::abs(self)
    }
    fn is_nan(self) -> bool {
        f32::is_nan(self)
    }
    fn zero() -> Self {
        0.0
    }
}

impl UpstreamFloat for f64 {
    fn abs(self) -> Self {
        f64::abs(self)
    }
    fn is_nan(self) -> bool {
        f64::is_nan(self)
    }
    fn zero() -> Self {
        0.0
    }
}

/// `OCIO_CHECK_CLOSE(x, y, tol)`: passes when `std::abs(x - y) < tol`. A NaN fails.
///
/// Port of `OCIO_CHECK_CLOSE` / `OCIO_CHECK_CLOSE_FROM`
/// (tests/testutils/UnitTest.h:194-212 @ v2.5.2).
#[track_caller]
pub fn check_close<T: UpstreamFloat>(x: T, y: T, tol: T) {
    // Upstream passes when the comparison is true; NaN makes it false.
    let passes = (x - y).abs() < tol;
    if !passes {
        panic!("OCIO_CHECK_CLOSE failed: abs({x:e} - {y:e}) < {tol:e}");
    }
}

/// `OCIO_CHECK_CLOSE(x, y, tol)` with `float` values and a `double` tolerance: `std::abs(x -
/// y)` is computed in `float`, then promoted to `double` for the comparison. A NaN fails.
///
/// Port of `OCIO_CHECK_CLOSE` / `OCIO_CHECK_CLOSE_FROM`
/// (tests/testutils/UnitTest.h:194-212 @ v2.5.2).
#[track_caller]
pub fn check_close_f32_f64(x: f32, y: f32, tol: f64) {
    let passes = f64::from((x - y).abs()) < tol;
    if !passes {
        panic!("OCIO_CHECK_CLOSE failed: abs({x:e} - {y:e}) < {tol:e}");
    }
}

/// `EqualWithAbsError(x1, x2, e)`: `((x1 > x2) ? x1 - x2 : x2 - x1) <= e`.
///
/// Port of `EqualWithAbsError` (src/OpenColorIO/MathUtils.h:21-45 @ v2.5.2), which upstream's
/// tests use as a check.
pub fn equal_with_abs_error<T: UpstreamFloat>(x1: T, x2: T, e: T) -> bool {
    (if x1 > x2 { x1 - x2 } else { x2 - x1 }) <= e
}

/// `EqualWithSafeRelError(value, expected, eps, minExpected)`: true when the values are equal
/// (including infinities), both NaN, or their difference divided by
/// `max(|expected|, minExpected)` is at most `eps`.
///
/// Port of `EqualWithSafeRelError` (tests/cpu/UnitTestUtils.h:74-100 @ v2.5.2).
pub fn equal_with_safe_rel_error<T: UpstreamFloat>(
    value: T,
    expected: T,
    eps: T,
    min_expected: T,
) -> bool {
    // If value and expected are infinity, return true.
    if value == expected {
        return true;
    }
    if value.is_nan() && expected.is_nan() {
        return true;
    }
    let zero = T::zero();
    let div = if expected > zero {
        if expected < min_expected {
            min_expected
        } else {
            expected
        }
    } else if -expected < min_expected {
        min_expected
    } else {
        -expected
    };

    let err = (if value > expected {
        value - expected
    } else {
        expected - value
    }) / div;

    err <= eps
}

/// `GetULPDifference(a, b)`: `abs((int)(FloatAsInt(a) - FloatAsInt(b)))`, the distance between
/// the bit patterns. The unsigned difference wraps and the `int` cast reinterprets it, as in
/// C++; `abs(INT_MIN)` stays `INT_MIN`, which the `unsigned` result reads as 2^31.
///
/// Port of `GetULPDifference` (tests/cpu/SSE_tests.cpp:123-126 @ v2.5.2).
pub fn ulp_difference(a: f32, b: f32) -> u32 {
    (a.to_bits().wrapping_sub(b.to_bits()) as i32).wrapping_abs() as u32
}

/// `AreAllClose(sseResult, reference, ulp_tolerance)`: every lane is at most `ulp_tolerance`
/// ULPs from `reference` ([`ulp_difference`]).
///
/// Port of `AreAllClose` (tests/cpu/SSE_tests.cpp:157-168 @ v2.5.2), which checks the four
/// lanes of an `__m128`; a port passes the lanes it computes.
pub fn are_all_close(lanes: &[f32], reference: f32, ulp_tolerance: u32) -> bool {
    lanes
        .iter()
        .all(|&lane| ulp_difference(lane, reference) <= ulp_tolerance)
}

/// `OCIO_CHECK_EQUAL(x, y)`: passes when `x == y`, with the type's own `==` (for floats,
/// `-0.0 == 0.0` and a NaN never equals).
///
/// Port of `OCIO_CHECK_EQUAL` / `OCIO_CHECK_EQUAL_FROM`
/// (tests/testutils/UnitTest.h:133-147 @ v2.5.2).
#[track_caller]
pub fn check_equal<T: PartialEq + Debug>(x: T, y: T) {
    if x != y {
        panic!("OCIO_CHECK_EQUAL failed: {x:?} == {y:?}");
    }
}

/// `OCIO_CHECK_THROW_WHAT(S, E, W)`: `S` fails with an error whose message contains `W`. An
/// empty message, or an empty `W`, fails the check.
///
/// `result` is what the statement returned, and its error type stands for `E`. Upstream
/// catches `E const &`, so a check on `OCIO::Exception` also accepts an
/// `OCIO::ExceptionMissingFile`; a port of a check on the subclass must check the error's
/// kind too.
///
/// Port of `OCIO_CHECK_THROW_WHAT` (tests/testutils/UnitTest.h:234-250 @ v2.5.2).
#[track_caller]
pub fn check_throw_what<T: Debug, E: Display>(result: Result<T, E>, what: &str) {
    match result {
        Ok(value) => panic!("OCIO_CHECK_THROW_WHAT failed: no error was raised, got {value:?}"),
        Err(e) => {
            let message = e.to_string();
            if what.is_empty() || message.is_empty() || !message.contains(what) {
                panic!(
                    "OCIO_CHECK_THROW_WHAT failed: the error \"{message}\" was raised. Expecting \
                     to contain \"{what}\""
                );
            }
        }
    }
}

/// Port of `FloatForCompare` (src/OpenColorIO/MathUtils.cpp:397-400 @ v2.5.2): maps a float's
/// bits to an ordered integer, keeping denormals.
fn float_for_compare(float_bits: u32) -> u32 {
    if float_bits < 0x8000_0000 {
        0x8000_0000u32.wrapping_add(float_bits)
    } else {
        0x8000_0000u32.wrapping_sub(float_bits & 0x7FFF_FFFF)
    }
}

/// Port of `FloatForCompareCompressDenorms` (src/OpenColorIO/MathUtils.cpp:433-444 @ v2.5.2):
/// maps a float's bits to an ordered integer, with denormals equal to zero.
fn float_for_compare_compress_denorms(float_bits: u32) -> u32 {
    let absi = float_bits & 0x7FFF_FFFF;
    if absi < 0x0080_0000 {
        0x8000_0000
    } else if float_bits < 0x8000_0000 {
        0x7F80_0001u32.wrapping_add(float_bits)
    } else {
        0x807F_FFFFu32.wrapping_sub(absi)
    }
}

/// Port of `ExtractFloatComponents` (src/OpenColorIO/MathUtils.cpp:446-453 @ v2.5.2):
/// `(sign, exponent, mantissa)`.
fn extract_float_components(float_bits: u32) -> (u32, u32, u32) {
    let mantissa = float_bits & 0x007F_FFFF;
    let sign_exp = float_bits >> 23;
    (sign_exp >> 8, sign_exp & 0xFF, mantissa)
}

/// `FloatsDiffer(expected, actual, tolerance, compressDenorms)`: whether `actual` differs from
/// `expected` by more than `tolerance` ULPs. Any NaN equals any NaN; infinities must match in
/// sign; `-0.0` equals `0.0`. Upstream's SIMD tests check with it
/// (`OCIO_CHECK_ASSERT_MESSAGE(!OCIO::FloatsDiffer(...))`).
///
/// Port of `FloatsDiffer` (src/OpenColorIO/MathUtils.cpp:455-529 @ v2.5.2).
pub fn floats_differ(expected: f32, actual: f32, tolerance: i32, compress_denorms: bool) -> bool {
    let expected_bits = expected.to_bits();
    let actual_bits = actual.to_bits();

    let (es, ee, em) = extract_float_components(expected_bits);
    let (as_, ae, am) = extract_float_components(actual_bits);

    let is_expected_special = ee == 0xFF;
    let is_actual_special = ae == 0xFF;
    if is_expected_special {
        // expected is a special float (-/+Inf or NaN)
        if is_actual_special {
            // Comparing special floats
            let is_expected_inf = em == 0;
            let is_actual_inf = am == 0;
            if is_expected_inf {
                // Comparing -/+Inf with -/+Inf, or -/+Inf with NaN.
                return if is_actual_inf { es != as_ } else { true };
            }
            // Comparing NaN with a special float.
            return is_actual_inf;
        }
        // Comparing a special float with a regular float.
        return true;
    } else if is_actual_special {
        // Comparing a regular float with a special float.
        return true;
    }

    // Comparing regular floats.
    let (expected_comp, actual_comp) = if compress_denorms {
        (
            float_for_compare_compress_denorms(expected_bits),
            float_for_compare_compress_denorms(actual_bits),
        )
    } else {
        (
            float_for_compare(expected_bits),
            float_for_compare(actual_bits),
        )
    };

    let diff = expected_comp.abs_diff(actual_comp);
    diff > tolerance as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats_differ_counts_ulps() {
        let one = 1.0f32;
        let next = f32::from_bits(one.to_bits() + 1);
        assert!(!floats_differ(one, one, 0, false));
        assert!(floats_differ(one, next, 0, false));
        assert!(!floats_differ(one, next, 1, false));
        // Signed zeros compare equal; NaNs equal each other but nothing else.
        assert!(!floats_differ(0.0, -0.0, 0, false));
        assert!(!floats_differ(f32::NAN, -f32::NAN, 0, false));
        assert!(floats_differ(f32::NAN, f32::INFINITY, 0, false));
        assert!(floats_differ(f32::INFINITY, f32::NEG_INFINITY, 0, false));
        // With compressDenorms, a denormal equals zero.
        assert!(floats_differ(0.0, f32::from_bits(1), 0, false));
        assert!(!floats_differ(0.0, f32::from_bits(1), 0, true));
    }

    #[test]
    fn check_equal_uses_eq() {
        check_equal(0.0f32, -0.0f32);
        let nan = std::panic::catch_unwind(|| check_equal(f32::NAN, f32::NAN));
        assert!(nan.is_err(), "a NaN never equals");
    }

    #[test]
    fn ulp_difference_counts_bit_patterns() {
        let one = 1.0f32;
        let next = f32::from_bits(one.to_bits() + 1);
        assert_eq!(ulp_difference(one, one), 0);
        assert_eq!(ulp_difference(one, next), 1);
        assert_eq!(ulp_difference(next, one), 1);
        // -0 and +0 differ by the sign bit alone: 2^31, the `abs(INT_MIN)` case.
        assert_eq!(ulp_difference(-0.0f32, 0.0f32), 1 << 31);
        assert!(are_all_close(&[one, next], one, 1));
        assert!(!are_all_close(&[one, next], one, 0));
    }

    #[test]
    fn check_close_is_strict() {
        check_close(1.0f32, 1.5f32, 0.6f32);
        let failed = std::panic::catch_unwind(|| check_close(1.0f32, 1.5f32, 0.5f32));
        assert!(failed.is_err(), "the bound is exclusive, as in `<`");
        let nan = std::panic::catch_unwind(|| check_close(f32::NAN, 1.0f32, 1.0f32));
        assert!(nan.is_err(), "NaN fails, as the comparison is false");
    }

    #[test]
    fn check_close_f32_f64_compares_in_double() {
        // 1e-3f32 is 1.00000005e-3 in double: below 1.0000001e-3, not below 1e-3.
        check_close_f32_f64(1.0e-3, 0.0, 1.0000001e-3);
        let failed = std::panic::catch_unwind(|| check_close_f32_f64(1.0e-3, 0.0, 1.0e-3));
        assert!(
            failed.is_err(),
            "the float difference is compared in double"
        );
        let nan = std::panic::catch_unwind(|| check_close_f32_f64(f32::NAN, 1.0, 1.0));
        assert!(nan.is_err(), "NaN fails, as the comparison is false");
    }

    #[test]
    fn abs_error_is_inclusive() {
        assert!(equal_with_abs_error(1.0f64, 1.5f64, 0.5f64));
        assert!(!equal_with_abs_error(1.0f64, 1.5f64, 0.25f64));
        assert!(!equal_with_abs_error(f32::NAN, 1.0f32, 1.0f32));
    }

    #[test]
    fn safe_rel_error() {
        assert!(equal_with_safe_rel_error(
            f32::INFINITY,
            f32::INFINITY,
            0.0f32,
            1.0f32
        ));
        assert!(equal_with_safe_rel_error(
            f32::NAN,
            -f32::NAN,
            0.0f32,
            1.0f32
        ));
        // Below minExpected the error is absolute, above it relative.
        assert!(equal_with_safe_rel_error(0.5f32, 0.25f32, 0.25f32, 1.0f32));
        assert!(!equal_with_safe_rel_error(0.5f32, 0.25f32, 0.2f32, 1.0f32));
        assert!(equal_with_safe_rel_error(12.0f32, 10.0f32, 0.2f32, 1.0f32));
        assert!(!equal_with_safe_rel_error(12.0f32, 10.0f32, 0.1f32, 1.0f32));
        assert!(!equal_with_safe_rel_error(f32::NAN, 1.0f32, 1.0f32, 1.0f32));
    }
}
