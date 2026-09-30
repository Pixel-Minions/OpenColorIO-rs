// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

use super::*;

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
