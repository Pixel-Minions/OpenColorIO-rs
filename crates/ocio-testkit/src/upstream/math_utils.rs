// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `FloatsDiffer` (src/OpenColorIO/MathUtils.cpp:355-529 @ v2.5.2), the ULP comparison upstream's
//! SIMD tests check with (`OCIO_CHECK_ASSERT_MESSAGE(!OCIO::FloatsDiffer(...))`).

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

/// Port of `FloatsDiffer` (src/OpenColorIO/MathUtils.cpp:455-529 @ v2.5.2): whether `actual`
/// differs from `expected` by more than `tolerance` ULPs. Any NaN equals any NaN; infinities
/// must match in sign; `-0.0` equals `0.0`.
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
