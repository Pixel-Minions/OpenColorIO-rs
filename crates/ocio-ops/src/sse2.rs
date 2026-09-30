// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The SSE2 helpers of `SSE2.h` (src/OpenColorIO/SSE2.h @ v2.5.2), as exact scalar code for one
//! SIMD lane: the software half-float conversions that OCIO's SSE2 kernels use on x86
//! (`!OCIO_USE_SSE2NEON`). The SSE min/max and conversion lanes are
//! [`crate::math_utils::sse_min`], [`sse_max`](crate::math_utils::sse_max) and
//! [`sse_cvttps_epi32`](crate::math_utils::sse_cvttps_epi32).
//!
//! Each `__m128` operation works on its four lanes independently, so one lane reproduces the
//! kernel's arithmetic exactly. The [`intrinsics`] module transcribes the same functions with
//! the real SSE2 instructions, to prove the scalar lanes against the hardware.
//!
//! `unsafe` is allowed in this module for SIMD intrinsics only.
#![allow(unsafe_code)]

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

/// The same functions written with SSE2 intrinsics, instruction for instruction as in
/// `SSE2.h`. They exist to prove the scalar lanes above against the hardware.
#[cfg(target_arch = "x86_64")]
pub mod intrinsics {
    use core::arch::x86_64::*;

    /// Port of `sse2_blendv` (src/OpenColorIO/SSE2.h:106-109 @ v2.5.2).
    #[inline]
    #[target_feature(enable = "sse2")]
    fn sse2_blendv(a: __m128i, b: __m128i, mask: __m128i) -> __m128i {
        _mm_xor_si128(_mm_and_si128(_mm_xor_si128(a, b), mask), a)
    }

    /// Port of `sse2_cvtps_ph` (src/OpenColorIO/SSE2.h:111-153 @ v2.5.2).
    #[target_feature(enable = "sse2")]
    fn sse2_cvtps_ph(a: __m128) -> __m128i {
        let mut x = _mm_castps_si128(a);

        let x_sgn = _mm_and_si128(x, _mm_set1_epi32(0x8000_0000u32 as i32));
        let mut x_exp = _mm_and_si128(x, _mm_set1_epi32(0x7f80_0000));

        let magic1 = _mm_castsi128_ps(_mm_set1_epi32(0x7780_0000)); // 0x1.0p+112f
        let magic2 = _mm_castsi128_ps(_mm_set1_epi32(0x0880_0000)); // 0x1.0p-110f

        // sse2 doesn't have _mm_max_epu32, but _mm_max_ps works.
        let exp_max = _mm_set1_epi32(0x3880_0000);
        x_exp = _mm_castps_si128(_mm_max_ps(
            _mm_castsi128_ps(x_exp),
            _mm_castsi128_ps(exp_max),
        )); // max(e, -14)
        x_exp = _mm_add_epi32(x_exp, _mm_set1_epi32(15 << 23)); // e += 15
        x = _mm_and_si128(x, _mm_set1_epi32(0x7fff_ffff)); // Discard sign

        let mut f = _mm_castsi128_ps(x);
        let magicf = _mm_castsi128_ps(x_exp);

        // If 15 < e then inf, otherwise e += 2
        f = _mm_mul_ps(_mm_mul_ps(f, magic1), magic2);
        f = _mm_add_ps(f, magicf);

        let u = _mm_castps_si128(f);

        let h_exp = _mm_and_si128(_mm_srli_epi32::<13>(u), _mm_set1_epi32(0x7c00));
        let mut h_sig = _mm_and_si128(u, _mm_set1_epi32(0x0fff));

        // blend in nan values only if present
        let nan_mask = _mm_cmpgt_epi32(x, _mm_set1_epi32(0x7f80_0000));
        if _mm_movemask_epi8(nan_mask) != 0 {
            let mut nan = _mm_and_si128(_mm_srli_epi32::<13>(x), _mm_set1_epi32(0x03ff));
            nan = _mm_or_si128(_mm_set1_epi32(0x0200), nan);
            h_sig = sse2_blendv(h_sig, nan, nan_mask);
        }

        let mut ph = _mm_add_epi32(_mm_srli_epi32::<16>(x_sgn), _mm_add_epi32(h_exp, h_sig));

        // pack u16 values into lower 64 bits
        ph = _mm_shufflehi_epi16::<{ 1 << 6 | 1 << 4 | 2 << 2 }>(ph);
        ph = _mm_shufflelo_epi16::<{ 1 << 6 | 1 << 4 | 2 << 2 }>(ph);
        _mm_shuffle_epi32::<{ 3 << 6 | 3 << 4 | 2 << 2 }>(ph)
    }

    /// Port of `sse2_cvtph_ps` (src/OpenColorIO/SSE2.h:155-192 @ v2.5.2).
    #[target_feature(enable = "sse2")]
    fn sse2_cvtph_ps(a: __m128i) -> __m128 {
        let magic = _mm_castsi128_ps(_mm_set1_epi32((254 - 15) << 23));
        let was_infnan = _mm_castsi128_ps(_mm_set1_epi32((127 + 16) << 23));

        // the values to unpack are in the lower 64 bits
        let a = _mm_unpacklo_epi16(a, a);

        // extract sign
        let sign = _mm_castsi128_ps(_mm_slli_epi32::<16>(_mm_and_si128(
            a,
            _mm_set1_epi32(0x8000),
        )));

        // extract exponent/mantissa bits
        let mut o = _mm_castsi128_ps(_mm_slli_epi32::<13>(_mm_and_si128(
            a,
            _mm_set1_epi32(0x7fff),
        )));

        // magic multiply
        o = _mm_mul_ps(o, magic);

        // blend in inf/nan values only if present
        let mut mask = _mm_castps_si128(_mm_cmpge_ps(o, was_infnan));
        if _mm_movemask_epi8(mask) != 0 {
            let mut ou = _mm_castps_si128(o);
            let ou_nan = _mm_or_si128(ou, _mm_set1_epi32(0x01ff << 22));
            let ou_inf = _mm_or_si128(ou, _mm_set1_epi32(0x00ff << 23));

            // blend in nans
            ou = sse2_blendv(ou, ou_nan, mask);

            // blend in infinities
            mask = _mm_cmpeq_epi32(_mm_castps_si128(o), _mm_castps_si128(was_infnan));
            o = _mm_castsi128_ps(sse2_blendv(ou, ou_inf, mask));
        }

        _mm_or_ps(o, sign)
    }

    /// [`sse2_cvtps_ph`] on four floats: the four halves in the low 64 bits, in order.
    pub fn cvtps_ph_x4(values: [f32; 4]) -> [u16; 4] {
        // SAFETY: SSE2 is part of the x86-64 baseline.
        unsafe { cvtps_ph_x4_sse2(values) }
    }

    #[target_feature(enable = "sse2")]
    fn cvtps_ph_x4_sse2(values: [f32; 4]) -> [u16; 4] {
        let ph = sse2_cvtps_ph(_mm_setr_ps(values[0], values[1], values[2], values[3]));
        let low = _mm_cvtsi128_si64(ph) as u64;
        [
            low as u16,
            (low >> 16) as u16,
            (low >> 32) as u16,
            (low >> 48) as u16,
        ]
    }

    /// [`sse2_cvtph_ps`] on four halves.
    pub fn cvtph_ps_x4(halves: [u16; 4]) -> [f32; 4] {
        // SAFETY: SSE2 is part of the x86-64 baseline.
        unsafe { cvtph_ps_x4_sse2(halves) }
    }

    #[target_feature(enable = "sse2")]
    fn cvtph_ps_x4_sse2(halves: [u16; 4]) -> [f32; 4] {
        let low = u64::from(halves[0])
            | u64::from(halves[1]) << 16
            | u64::from(halves[2]) << 32
            | u64::from(halves[3]) << 48;
        let o = sse2_cvtph_ps(_mm_cvtsi64_si128(low as i64));
        let mut out = [0f32; 4];
        // SAFETY: `out` has room for the four floats `_mm_storeu_ps` writes.
        unsafe { _mm_storeu_ps(out.as_mut_ptr(), o) };
        out
    }
}
