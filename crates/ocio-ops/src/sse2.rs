// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The SSE2 helpers of `SSE2.h` (src/OpenColorIO/SSE2.h @ v2.5.2), as exact scalar code for one
//! SIMD lane: the software half-float conversions that OCIO's SSE2 kernels use on x86
//! (`!OCIO_USE_SSE2NEON`). The SSE min/max and conversion lanes are
//! [`crate::math_utils::sse_min`], [`sse_max`](crate::math_utils::sse_max) and
//! [`sse_cvttps_epi32`](crate::math_utils::sse_cvttps_epi32).
//!
//! Each `__m128` operation works on its four lanes independently, so one lane reproduces the
//! kernel's arithmetic exactly.

use crate::math_utils::sse_max;

/// One lane of `sse2_blendv` (src/OpenColorIO/SSE2.h:106-109 @ v2.5.2):
/// `((a ^ b) & mask) ^ a`.
#[inline]
pub fn sse2_blendv(a: u32, b: u32, mask: u32) -> u32 {
    ((a ^ b) & mask) ^ a
}

/// One lane of `sse2_cvtps_ph` (src/OpenColorIO/SSE2.h:111-153 @ v2.5.2): float to half bits,
/// rounding to nearest even through a float addition.
///
/// The final shuffles move each lane's low 16 bits into the low 64 bits of the register and
/// leave zeros above them, so one lane's result is the low 16 bits of `ph`.
pub fn sse2_cvtps_ph(a: f32) -> u16 {
    let x = a.to_bits();

    let x_sgn = x & 0x8000_0000;
    let x_exp = x & 0x7f80_0000;

    let magic1 = f32::from_bits(0x7780_0000); // 0x1.0p+112f
    let magic2 = f32::from_bits(0x0880_0000); // 0x1.0p-110f

    // SSE2 has no _mm_max_epu32, but _mm_max_ps works.
    let exp_max = 0x3880_0000u32;
    let x_exp = sse_max(f32::from_bits(x_exp), f32::from_bits(exp_max)).to_bits(); // max(e, -14)
    let x_exp = x_exp.wrapping_add(15 << 23); // e += 15
    let x = x & 0x7fff_ffff; // Discard the sign.

    let mut f = f32::from_bits(x);
    let magicf = f32::from_bits(x_exp);

    // If 15 < e then inf, otherwise e += 2.
    f = (f * magic1) * magic2;
    f += magicf;

    let u = f.to_bits();

    let h_exp = (u >> 13) & 0x7c00;
    let mut h_sig = u & 0x0fff;

    // Blend in NaN values (only if present, which does not change the result).
    // _mm_cmpgt_epi32 compares signed 32-bit integers; `x` has no sign bit here.
    let nan_mask = if (x as i32) > 0x7f80_0000 {
        u32::MAX
    } else {
        0
    };
    if nan_mask != 0 {
        let nan = ((x >> 13) & 0x03ff) | 0x0200;
        h_sig = sse2_blendv(h_sig, nan, nan_mask);
    }

    let ph = (x_sgn >> 16).wrapping_add(h_exp.wrapping_add(h_sig));
    ph as u16
}

/// One lane of `sse2_cvtph_ps` (src/OpenColorIO/SSE2.h:155-192 @ v2.5.2): half bits to float.
/// NaNs come out quiet (bit 22 set).
pub fn sse2_cvtph_ps(h: u16) -> f32 {
    let magic = f32::from_bits((254 - 15) << 23);
    let was_infnan = f32::from_bits((127 + 16) << 23);

    // _mm_unpacklo_epi16(a, a) puts the half in both 16-bit halves of the lane.
    let a = u32::from(h) | (u32::from(h) << 16);

    // Extract the sign.
    let sign = (a & 0x8000) << 16;

    // Extract the exponent and mantissa bits.
    let mut o = f32::from_bits((a & 0x7fff) << 13);

    // Magic multiply.
    o *= magic;

    // Blend in infinities and NaNs (only if present, which does not change the result).
    // _mm_cmpge_ps is an ordered compare; `o` is never NaN here.
    let mask = if o >= was_infnan { u32::MAX } else { 0 };
    if mask != 0 {
        let ou = o.to_bits();
        let ou_nan = ou | (0x01ff << 22);
        let ou_inf = ou | (0x00ff << 23);

        // Blend in NaNs.
        let ou = sse2_blendv(ou, ou_nan, mask);

        // Blend in infinities.
        let inf_mask = if o.to_bits() == was_infnan.to_bits() {
            u32::MAX
        } else {
            0
        };
        o = f32::from_bits(sse2_blendv(ou, ou_inf, inf_mask));
    }

    f32::from_bits(o.to_bits() | sign)
}
