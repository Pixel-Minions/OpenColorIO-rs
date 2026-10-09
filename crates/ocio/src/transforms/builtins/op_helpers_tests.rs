// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `Interpolate1D`'s test: `OCIO_ADD_TEST(Builtins, interpolate)`
//! (tests/cpu/transforms/BuiltinTransform_tests.cpp:309-326 @ v2.5.2).

use super::*;
use ocio_testkit::upstream::equal_with_safe_rel_error;

/// Port of `ValidateValues(T act, T aim, int lineNo)` (BuiltinTransform_tests.cpp:120-158 @
/// v2.5.2): a relative error, absolute below 1, within 1e-7.
#[track_caller]
fn validate_values(act: f64, aim: f64) {
    assert!(
        equal_with_safe_rel_error(act, aim, 1e-7, 1.0),
        " - Values: {act} expected: {aim}"
    );
}

/// Port of `OCIO_ADD_TEST(Builtins, interpolate)` @ v2.5.2.
#[test]
fn interpolate() {
    // Test the non-uniform 1D linear interpolation helper function.

    const LUT_SIZE: usize = 4;
    #[rustfmt::skip]
    const LUT_VALUES: [f64; LUT_SIZE * 2] = [
        0.,    1.0,
        0.50,  2.0,
        0.75,  2.5,
        1.,    3.,
    ];

    validate_values(interpolate_1d(LUT_SIZE, &LUT_VALUES, -1.).unwrap(), 1.);
    validate_values(interpolate_1d(LUT_SIZE, &LUT_VALUES, 0.).unwrap(), 1.);
    validate_values(interpolate_1d(LUT_SIZE, &LUT_VALUES, 0.1).unwrap(), 1.2);
    validate_values(interpolate_1d(LUT_SIZE, &LUT_VALUES, 0.5).unwrap(), 2.);
    validate_values(interpolate_1d(LUT_SIZE, &LUT_VALUES, 0.99).unwrap(), 2.98);
    validate_values(interpolate_1d(LUT_SIZE, &LUT_VALUES, 2.).unwrap(), 3.);
}

/// A NaN input matches no interval: upstream's "Invalid interpolation value.".
#[test]
fn interpolating_a_nan_is_refused() {
    let lut_values = [0., 1.0, 1., 3.];
    assert_eq!(
        interpolate_1d(2, &lut_values, f64::NAN)
            .unwrap_err()
            .message(),
        "Invalid interpolation value."
    );
}

/// A generator's error comes out of the half-domain LUT's builder, as upstream's throw does,
/// and no op is appended; without one, the builder appends its LUT.
#[test]
fn a_half_lut_generator_error_appends_nothing() {
    let mut ops = OpVec::new();
    let error = try_create_half_lut(&mut ops, |value| {
        if value > 1.0 {
            interpolate_1d(2, &[0., 1.0, 1., 3.], f64::NAN).map(|v| v as f32)
        } else {
            Ok(value as f32)
        }
    })
    .unwrap_err();
    assert_eq!(error.message(), "Invalid interpolation value.");
    assert!(ops.is_empty());

    try_create_half_lut(&mut ops, |value| Ok(value as f32)).unwrap();
    assert_eq!(ops.len(), 1);
}
