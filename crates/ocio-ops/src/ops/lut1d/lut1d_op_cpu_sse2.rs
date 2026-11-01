// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The SSE2 kernel of the 1D LUT, `linear1D<BIT_DEPTH_F32, outBD>`
//! (src/OpenColorIO/ops/lut1d/Lut1DOpCPU_SSE2.cpp:15-178 @ v2.5.2), as exact scalar code.
//!
//! The kernel's lanes never mix: the RGBA packs only move bits (for F32 input), each gather
//! reads its own lane's index, and the zero-padded last block adds lanes whose results are
//! discarded. So each value is one call of [`apply_lut_sse2`], which follows the C++ operation
//! for operation, and each stored value one call of [`store`].
//!
//! No NaN reaches the arithmetic as an operand: the input is clamped first (NaN becomes 0), and
//! the LUT values are sanitized. `n - p` can overflow to an infinity; on a node, where `d` is
//! 0, its product with `d` is the default NaN, and so is the result.

#![allow(unsafe_code)]

use crate::math_utils::{sse_cvttps_epi32, sse_max, sse_min};
use crate::sse2::{PackDepth, sse2_rgba_pack_store};

/// One lane of `fmadd_ps_sse2` (Lut1DOpCPU_SSE2.cpp:25-34 @ v2.5.2) without SSE2NEON:
/// `_mm_add_ps(_mm_mul_ps(a, b), c)`, two roundings.
fn fmadd_ps_sse2(a: f32, b: f32, c: f32) -> f32 {
    a * b + c
}

/// One lane of `floor_ps_sse2` (Lut1DOpCPU_SSE2.cpp:36-44 @ v2.5.2) without SSE2NEON:
/// truncation, `_mm_cvtepi32_ps(_mm_cvttps_epi32(v))`.
fn floor_ps_sse2(v: f32) -> f32 {
    sse_cvttps_epi32(v) as f32
}

/// One lane of `apply_lut_sse2` (Lut1DOpCPU_SSE2.cpp:47-76 @ v2.5.2): `v` through the table
/// `lut`, with `scale` the domain's scale and `lut_max` its last index.
pub(crate) fn apply_lut_sse2(lut: &[f32], v: f32, scale: f32, lut_max: f32) -> f32 {
    let zero = 0.0f32;
    let one_f = 1.0f32;

    let scaled = v * scale;

    // clamp, max first, NAN set to zero
    let x = sse_min(sse_max(scaled, zero), lut_max);
    let prev_f = floor_ps_sse2(x);
    let d = x - prev_f;
    let next_f = sse_min(prev_f + one_f, lut_max);

    let prev_i = sse_cvttps_epi32(prev_f);
    let next_i = sse_cvttps_epi32(next_f);

    // `i32gather_ps_sse2`: the indices are stored as `uint32_t`.
    let p = lut[prev_i as u32 as usize];
    let n = lut[next_i as u32 as usize];

    // lerp: a + (b - a) * t;
    fmadd_ps_sse2(n - p, d, p)
}

/// One value of `SSE2RGBAPack<outBD>::Store` (src/OpenColorIO/SSE2.h:198-401 @ v2.5.2), as
/// the raw bits of the output channel type.
pub(crate) fn store(depth: PackDepth, value: f32) -> u32 {
    sse2_rgba_pack_store(depth, value)
}

/// The real SSE2 kernel (`docs/cards/phase8.md`). Its lanes never mix, so it renders whole
/// groups of 4 pixels, and the exact lanes above render the rest. No NaN reaches its
/// arithmetic, so no operand order can show: the clamp makes a NaN 0, and the only NaN after it
/// is the default NaN of `Inf * 0` on a node.
#[cfg(target_arch = "x86_64")]
pub(super) mod simd {
    use core::arch::x86_64::*;

    use super::super::lut1d_op_cpu::KernelLut;
    use crate::op::PixelsMut;
    use crate::sse2::PackDepth;
    use crate::sse2::packs::{load_f32, store_f16, store_f32, store_u8, store_u16};

    /// `i32gather_ps_sse2` (Lut1DOpCPU_SSE2.cpp:17-23 @ v2.5.2).
    ///
    /// # Safety
    ///
    /// Each of `indices` is a valid index of `lut`.
    #[inline]
    #[target_feature(enable = "sse2")]
    unsafe fn gather(lut: &[f32], indices: __m128i) -> __m128 {
        let mut at = [0u32; 4];
        // SAFETY: `at` holds the sixteen bytes this writes, at any alignment.
        unsafe { _mm_storeu_si128(at.as_mut_ptr().cast(), indices) };
        // SAFETY: the caller's.
        let values = at.map(|i| unsafe { *lut.get_unchecked(i as usize) });
        // SAFETY: `values` holds the four floats this reads, at any alignment.
        unsafe { _mm_loadu_ps(values.as_ptr()) }
    }

    /// Port of `apply_lut_sse2` (Lut1DOpCPU_SSE2.cpp:47-76 @ v2.5.2), for a table of more than
    /// `lut_max` values.
    #[inline]
    #[target_feature(enable = "sse2")]
    fn apply_lut_sse2(lut: &[f32], v: __m128, scale: __m128, lut_max: __m128) -> __m128 {
        let zero = _mm_setzero_ps();
        let one_f = _mm_set1_ps(1.0);

        let scaled = _mm_mul_ps(v, scale);

        // clamp, max first, NAN set to zero
        let x = _mm_min_ps(_mm_max_ps(scaled, zero), lut_max);
        // `floor_ps_sse2`: truncation.
        let prev_f = _mm_cvtepi32_ps(_mm_cvttps_epi32(x));
        let d = _mm_sub_ps(x, prev_f);
        let next_f = _mm_min_ps(_mm_add_ps(prev_f, one_f), lut_max);

        let prev_i = _mm_cvttps_epi32(prev_f);
        let next_i = _mm_cvttps_epi32(next_f);

        // SAFETY: both are integers from 0 to `lut_max`, as `x` is.
        let (p, n) = unsafe { (gather(lut, prev_i), gather(lut, next_i)) };

        // lerp: a + (b - a) * t;
        _mm_add_ps(_mm_mul_ps(_mm_sub_ps(n, p), d), p)
    }

    /// Renders red, green and blue of each group of pixels in `rgba`, in place. The transposes
    /// only move alpha's bits. Returns the number of values rendered.
    #[target_feature(enable = "sse2")]
    fn in_place(lut: &KernelLut<'_>, rgba: &mut [f32]) -> usize {
        let scale = _mm_set1_ps(lut.scale);
        let lut_max = _mm_set1_ps(lut.lut_max);
        let (groups, _) = rgba.as_chunks_mut::<16>();
        for group in groups.iter_mut() {
            let [r, g, b, a] = load_f32(group);
            let r = apply_lut_sse2(lut.r, r, scale, lut_max);
            let g = apply_lut_sse2(lut.g, g, scale, lut_max);
            let b = apply_lut_sse2(lut.b, b, scale, lut_max);
            store_f32(group, [r, g, b, a]);
        }
        groups.len() * 16
    }

    /// The loop of `linear1D<BIT_DEPTH_F32, outBD>` (Lut1DOpCPU_SSE2.cpp:91-108 @ v2.5.2) with
    /// `inBD != outBD`, for `store`, the pack of `outBD`.
    #[inline]
    #[target_feature(enable = "sse2")]
    fn convert<T>(
        lut: &KernelLut<'_>,
        input: &[f32],
        output: &mut [T],
        store: impl Fn(&mut [T; 16], [__m128; 4]),
    ) -> usize {
        let scale = _mm_set1_ps(lut.scale);
        let lut_max = _mm_set1_ps(lut.lut_max);
        let alpha_scale = _mm_set1_ps(lut.alpha_scale);
        let (groups, _) = input.as_chunks::<16>();
        let (outputs, _) = output.as_chunks_mut::<16>();
        for (group, out) in groups.iter().zip(outputs) {
            let [r, g, b, a] = load_f32(group);
            let r = apply_lut_sse2(lut.r, r, scale, lut_max);
            let g = apply_lut_sse2(lut.g, g, scale, lut_max);
            let b = apply_lut_sse2(lut.b, b, scale, lut_max);
            let a = _mm_mul_ps(a, alpha_scale);
            store(out, [r, g, b, a]);
        }
        groups.len() * 16
    }

    /// From F32 to F32, where alpha moves through the packs (`inBD == outBD`).
    #[target_feature(enable = "sse2")]
    fn between(lut: &KernelLut<'_>, input: &[f32], output: &mut [f32]) -> usize {
        let scale = _mm_set1_ps(lut.scale);
        let lut_max = _mm_set1_ps(lut.lut_max);
        let (groups, _) = input.as_chunks::<16>();
        let (outputs, _) = output.as_chunks_mut::<16>();
        for (group, out) in groups.iter().zip(outputs) {
            let [r, g, b, a] = load_f32(group);
            let r = apply_lut_sse2(lut.r, r, scale, lut_max);
            let g = apply_lut_sse2(lut.g, g, scale, lut_max);
            let b = apply_lut_sse2(lut.b, b, scale, lut_max);
            store_f32(out, [r, g, b, a]);
        }
        groups.len() * 16
    }

    #[target_feature(enable = "sse2")]
    fn to_output(
        lut: &KernelLut<'_>,
        depth: PackDepth,
        input: &[f32],
        output: PixelsMut<'_>,
    ) -> usize {
        let max_value = depth.max_value() as f32;
        match output {
            PixelsMut::U8(out) => convert(lut, input, out, |o, rgba| store_u8(o, rgba)),
            PixelsMut::U16(out) => {
                convert(lut, input, out, |o, rgba| store_u16(o, rgba, max_value))
            }
            PixelsMut::F16(out) => convert(lut, input, out, |o, rgba| store_f16(o, rgba)),
            PixelsMut::F32(out) => between(lut, input, out),
        }
    }

    /// See [`in_place`].
    pub(in super::super) fn render_in_place(lut: &KernelLut<'_>, rgba: &mut [f32]) -> usize {
        // SAFETY: x86-64 has SSE2.
        unsafe { in_place(lut, rgba) }
    }

    /// Renders each group of `input` to `output`, of the bit depth `depth`.
    /// Returns the number of values rendered.
    pub(in super::super) fn render(
        lut: &KernelLut<'_>,
        depth: PackDepth,
        input: &[f32],
        output: PixelsMut<'_>,
    ) -> usize {
        // SAFETY: x86-64 has SSE2.
        unsafe { to_output(lut, depth, input, output) }
    }
}
