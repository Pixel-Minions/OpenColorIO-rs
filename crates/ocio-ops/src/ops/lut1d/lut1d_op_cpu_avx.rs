// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The AVX kernel of the 1D LUT, `linear1D<BIT_DEPTH_F32, outBD>`
//! (src/OpenColorIO/ops/lut1d/Lut1DOpCPU_AVX.cpp:15-165 @ v2.5.2), as exact scalar code: 8
//! pixels per block instead of SSE2's 4, `_mm256_floor_ps` instead of a truncation (the same
//! for the clamped, non-negative values), a multiply and an add (no FMA), and F16C for half
//! output. The lanes never mix (`lut1d_op_cpu_sse2` explains why), so each value is one call of
//! [`apply_lut_avx`], and each stored value one call of [`store`].

#![allow(unsafe_code)]

use crate::avx::avx_rgba_pack_store;
use crate::math_utils::{sse_cvttps_epi32, sse_max, sse_min};
use crate::sse2::PackDepth;

/// One lane of `fmadd_ps_avx` (Lut1DOpCPU_AVX.cpp:29-32 @ v2.5.2):
/// `_mm256_add_ps(_mm256_mul_ps(a, b), c)`, two roundings.
fn fmadd_ps_avx(a: f32, b: f32, c: f32) -> f32 {
    a * b + c
}

/// One lane of `apply_lut_avx` (Lut1DOpCPU_AVX.cpp:34-63 @ v2.5.2): `v` through the table
/// `lut`, with `scale` the domain's scale and `lut_max` its last index.
pub(crate) fn apply_lut_avx(lut: &[f32], v: f32, scale: f32, lut_max: f32) -> f32 {
    let zero = 0.0f32;
    let one_f = 1.0f32;

    let scaled = v * scale;

    // clamp, max first, NAN set to zero
    let x = sse_min(sse_max(scaled, zero), lut_max);
    let prev_f = x.floor();
    let d = x - prev_f;
    let next_f = sse_min(prev_f + one_f, lut_max);

    let prev_i = sse_cvttps_epi32(prev_f);
    let next_i = sse_cvttps_epi32(next_f);

    // `i32gather_ps_avx`: the indices are stored as `uint32_t`.
    let p = lut[prev_i as u32 as usize];
    let n = lut[next_i as u32 as usize];

    // lerp: a + (b - a) * t;
    fmadd_ps_avx(n - p, d, p)
}

/// One value of `AVXRGBAPack<outBD>::Store` (src/OpenColorIO/AVX.h:94-332 @ v2.5.2), as the raw
/// bits of the output channel type.
pub(crate) fn store(depth: PackDepth, value: f32) -> u32 {
    avx_rgba_pack_store(depth, value)
}

/// The real AVX kernel (`docs/cards/phase8.md`). Its lanes never mix, so it renders whole
/// groups of 8 pixels, and the exact lanes above render the rest. No NaN reaches its
/// arithmetic, so no operand order can show: the clamp makes a NaN 0, and the only NaN after it
/// is the default NaN of `Inf * 0` on a node.
#[cfg(target_arch = "x86_64")]
pub(super) mod simd {
    use core::arch::x86_64::*;

    use super::super::lut1d_op_cpu::KernelLut;
    use crate::avx::packs::{load_f32, store_f16, store_f32, store_u8, store_u16};
    use crate::op::PixelsMut;
    use crate::sse2::PackDepth;

    /// `i32gather_ps_avx` (Lut1DOpCPU_AVX.cpp:17-27 @ v2.5.2).
    ///
    /// # Safety
    ///
    /// Each of `indices` is a valid index of `lut`.
    #[inline]
    #[target_feature(enable = "avx")]
    unsafe fn gather(lut: &[f32], indices: __m256i) -> __m256 {
        let mut at = [0u32; 8];
        // SAFETY: `at` holds the 32 bytes this writes, at any alignment.
        unsafe { _mm256_storeu_si256(at.as_mut_ptr().cast(), indices) };
        // SAFETY: the caller's.
        let values = at.map(|i| unsafe { *lut.get_unchecked(i as usize) });
        // SAFETY: `values` holds the eight floats this reads, at any alignment.
        unsafe { _mm256_loadu_ps(values.as_ptr()) }
    }

    /// Port of `apply_lut_avx` (Lut1DOpCPU_AVX.cpp:35-61 @ v2.5.2), for a table of more than
    /// `lut_max` values.
    #[inline]
    #[target_feature(enable = "avx")]
    fn apply_lut_avx(lut: &[f32], v: __m256, scale: __m256, lut_max: __m256) -> __m256 {
        let zero = _mm256_setzero_ps();
        let one_f = _mm256_set1_ps(1.0);

        let scaled = _mm256_mul_ps(v, scale);

        // clamp, max first, NAN set to zero
        let x = _mm256_min_ps(_mm256_max_ps(scaled, zero), lut_max);
        let prev_f = _mm256_floor_ps(x);
        let d = _mm256_sub_ps(x, prev_f);
        let next_f = _mm256_min_ps(_mm256_add_ps(prev_f, one_f), lut_max);

        let prev_i = _mm256_cvttps_epi32(prev_f);
        let next_i = _mm256_cvttps_epi32(next_f);

        // SAFETY: both are integers from 0 to `lut_max`, as `x` is.
        let (p, n) = unsafe { (gather(lut, prev_i), gather(lut, next_i)) };

        // lerp: a + (b - a) * t;
        _mm256_add_ps(_mm256_mul_ps(_mm256_sub_ps(n, p), d), p)
    }

    /// Renders red, green and blue of each group of pixels in `rgba`, in place. The transposes
    /// only move alpha's bits. Returns the number of values rendered.
    #[target_feature(enable = "avx")]
    fn in_place(lut: &KernelLut<'_>, rgba: &mut [f32]) -> usize {
        let scale = _mm256_set1_ps(lut.scale);
        let lut_max = _mm256_set1_ps(lut.lut_max);
        let (groups, _) = rgba.as_chunks_mut::<32>();
        for group in groups.iter_mut() {
            let [r, g, b, a] = load_f32(group);
            let r = apply_lut_avx(lut.r, r, scale, lut_max);
            let g = apply_lut_avx(lut.g, g, scale, lut_max);
            let b = apply_lut_avx(lut.b, b, scale, lut_max);
            store_f32(group, [r, g, b, a]);
        }
        groups.len() * 32
    }

    /// The loop of `linear1D<BIT_DEPTH_F32, outBD>` (Lut1DOpCPU_AVX.cpp:76-92 @ v2.5.2) with
    /// `inBD != outBD`, for `store`, the pack of `outBD`.
    #[inline]
    #[target_feature(enable = "avx")]
    fn convert<T>(
        lut: &KernelLut<'_>,
        input: &[f32],
        output: &mut [T],
        store: impl Fn(&mut [T; 32], [__m256; 4]),
    ) -> usize {
        let scale = _mm256_set1_ps(lut.scale);
        let lut_max = _mm256_set1_ps(lut.lut_max);
        let alpha_scale = _mm256_set1_ps(lut.alpha_scale);
        let (groups, _) = input.as_chunks::<32>();
        let (outputs, _) = output.as_chunks_mut::<32>();
        for (group, out) in groups.iter().zip(outputs) {
            let [r, g, b, a] = load_f32(group);
            let r = apply_lut_avx(lut.r, r, scale, lut_max);
            let g = apply_lut_avx(lut.g, g, scale, lut_max);
            let b = apply_lut_avx(lut.b, b, scale, lut_max);
            let a = _mm256_mul_ps(a, alpha_scale);
            store(out, [r, g, b, a]);
        }
        groups.len() * 32
    }

    #[target_feature(enable = "avx,f16c")]
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
            PixelsMut::F32(_) => 0,
        }
    }

    #[target_feature(enable = "avx")]
    fn to_integers(
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
            PixelsMut::F16(_) | PixelsMut::F32(_) => 0,
        }
    }

    /// Whether this CPU runs this kernel's instructions. A renderer can be built for another
    /// CPU's kernel: its exact lanes then render the row.
    pub(in super::super) fn runs_here() -> bool {
        std::arch::is_x86_feature_detected!("avx")
    }

    /// See [`in_place`]: nothing where the CPU can't run it.
    pub(in super::super) fn render_in_place(lut: &KernelLut<'_>, rgba: &mut [f32]) -> usize {
        if !runs_here() {
            return 0;
        }
        // SAFETY: `runs_here`.
        unsafe { in_place(lut, rgba) }
    }

    /// Renders each group of `input` to `output`, of the bit depth `depth`, other than F32.
    /// Returns the number of values rendered, none where the CPU can't run it.
    pub(in super::super) fn render(
        lut: &KernelLut<'_>,
        depth: PackDepth,
        input: &[f32],
        output: PixelsMut<'_>,
    ) -> usize {
        if !runs_here() {
            return 0;
        }
        if std::arch::is_x86_feature_detected!("f16c") {
            // SAFETY: `runs_here`, and the CPU has F16C.
            return unsafe { to_output(lut, depth, input, output) };
        }
        // SAFETY: `runs_here`.
        unsafe { to_integers(lut, depth, input, output) }
    }
}
