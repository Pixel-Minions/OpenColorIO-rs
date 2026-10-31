// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! lcms2's own tests of the fixed-point numbers: `testbed/testcms2.c` @ lcms2.17.

use ocio_testkit::upstream::equal_with_abs_error;

use super::*;
use crate::lcms2::types::{CMS_D50X, CMS_D50Y, CMS_D50Z};

/// `FIXED_PRECISION_15_16` (testbed/testcms2.c:832 @ lcms2.17).
const FIXED_PRECISION_15_16: f64 = 1.0 / 65535.0;

/// `TestSingleFixed15_16` (testbed/testcms2.c:893-901 @ lcms2.17).
fn test_single_fixed_15_16(d: f64) -> bool {
    let f = cms_double_to_15_fixed16(d);
    let round_trip = cms_15_fixed16_to_double(f);
    equal_with_abs_error(d, round_trip, FIXED_PRECISION_15_16)
}

/// Port of `CheckFixedPoint15_16` (testbed/testcms2.c:904-920 @ lcms2.17).
#[test]
fn check_fixed_point_15_16() {
    assert!(test_single_fixed_15_16(1.0));
    assert!(test_single_fixed_15_16(2.0));
    assert!(test_single_fixed_15_16(1.23456));
    assert!(test_single_fixed_15_16(0.99999));
    assert!(test_single_fixed_15_16(0.1234567890123456789099999));
    assert!(test_single_fixed_15_16(-1.0));
    assert!(test_single_fixed_15_16(-2.0));
    assert!(test_single_fixed_15_16(-1.23456));
    assert!(test_single_fixed_15_16(-1.1234567890123456789099999));
    assert!(test_single_fixed_15_16(32767.1234567890123456789099999));
    assert!(test_single_fixed_15_16(-32767.1234567890123456789099999));
}

/// Port of `CheckD50Roundtrip` (testbed/testcms2.c:946-986 @ lcms2.17).
#[test]
fn check_d50_roundtrip() {
    let euclidean = |[x, y, z]: [f64; 3]| {
        let round_trip = |v: f64| cms_15_fixed16_to_double(cms_double_to_15_fixed16(v));
        let (dx, dy, dz) = (
            (x - round_trip(x)).abs(),
            (y - round_trip(y)).abs(),
            (z - round_trip(z)).abs(),
        );
        (dx * dx + dy * dy + dz * dz).sqrt()
    };
    assert!(euclidean([CMS_D50X, CMS_D50Y, CMS_D50Z]) <= 1E-5);
    assert!(euclidean([0.96420288, 1.0, 0.82490540]) <= 1E-5);
}
