// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The F16C half-float conversions that `AVX.h`, `AVX2.h` and `AVX512.h` use
//! (src/OpenColorIO/AVX.h:267-306, AVX2.h:237-276 and AVX512.h:319-393 @ v2.5.2), as exact scalar
//! code for one lane.
//!
//! The kernels call `_mm256_cvtps_ph(x, 0)` / `_mm512_cvtps_ph(x, 0)` (rounding to nearest
//! even) and `_mm256_cvtph_ps` / `_mm512_cvtph_ps`. F16C rounds like IEEE 754, keeps the top
//! 10 mantissa bits of a NaN and makes it quiet. The `half` crate's software conversions
//! (`from_f32_const`, `to_f32_const`) implement exactly that.

/// One lane of `_mm256_cvtps_ph(a, 0)`: float to half bits, rounding to nearest even.
#[inline]
pub fn f16c_cvtps_ph(a: f32) -> u16 {
    half::f16::from_f32_const(a).to_bits()
}

/// One lane of `_mm256_cvtph_ps(a)`: half bits to float. NaNs come out quiet.
#[inline]
pub fn f16c_cvtph_ps(h: u16) -> f32 {
    half::f16::from_bits(h).to_f32_const()
}
