// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Upstream's check macros (tests/testutils/UnitTest.h @ v2.5.2), for ported upstream tests.
//!
//! Upstream's checks record a failure and let the test go on; these panic at the first failure,
//! on the same condition.

use std::fmt::Debug;
use std::ops::Sub;

/// A number `OCIO_CHECK_CLOSE` can compare: it has `std::abs`, `-` and `<`.
pub trait Close: Copy + PartialOrd + Debug + Sub<Output = Self> {
    /// `std::abs`
    fn abs(self) -> Self;
}

impl Close for f32 {
    fn abs(self) -> f32 {
        f32::abs(self)
    }
}

impl Close for f64 {
    fn abs(self) -> f64 {
        f64::abs(self)
    }
}

/// Port of `OCIO_CHECK_CLOSE(x, y, tol)` (tests/testutils/UnitTest.h:194-212 @ v2.5.2): passes
/// when `std::abs(x - y) < tol`, so it fails when either value is NaN.
#[track_caller]
pub fn check_close<T: Close>(x: T, y: T, tol: T) {
    if (x - y).abs() < tol {
        return;
    }
    panic!("FAILED: abs(x - y) < tol\n\tvalues were '{x:?}', '{y:?}' and '{tol:?}'");
}

/// Port of `OCIO_CHECK_EQUAL(x, y)` (tests/testutils/UnitTest.h:133-147 @ v2.5.2): passes when
/// `x == y`, with the type's own `==` (for floats, `-0.0 == 0.0` and NaN never equals).
#[track_caller]
pub fn check_equal<T: PartialEq + Debug>(x: T, y: T) {
    if x == y {
        return;
    }
    panic!("FAILED: x == y\n\tvalues were '{x:?}' and '{y:?}'");
}
