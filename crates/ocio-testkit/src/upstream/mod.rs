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

use std::fmt::{Debug, LowerExp};
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_close_is_strict() {
        check_close(1.0f32, 1.5f32, 0.6f32);
        let failed = std::panic::catch_unwind(|| check_close(1.0f32, 1.5f32, 0.5f32));
        assert!(failed.is_err(), "the bound is exclusive, as in `<`");
        let nan = std::panic::catch_unwind(|| check_close(f32::NAN, 1.0f32, 1.0f32));
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
