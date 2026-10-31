// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The AVX-512 kernel of the 1D LUT, `linear1D<BIT_DEPTH_F32, outBD>`
//! (src/OpenColorIO/ops/lut1d/Lut1DOpCPU_AVX512.cpp:15-116 @ v2.5.2), as exact scalar code: as
//! the AVX2 kernel, 16 pixels per block, the last block loaded and stored masked
//! (`LoadMasked`, `StoreMasked`) rather than through zero-padded buffers. The masked-off lanes'
//! results are never stored, so each value is one call of [`apply_lut_avx512`] and each stored
//! value one call of [`store`].

#![allow(unsafe_code)]

use crate::avx2::mm256_fmadd_ps;
use crate::avx512::avx512_rgba_pack_store;
use crate::math_utils::{sse_cvttps_epi32, sse_max, sse_min};
use crate::sse2::PackDepth;

/// One lane of `apply_lut_avx512` (Lut1DOpCPU_AVX512.cpp:18-41 @ v2.5.2): `v` through the table
/// `lut`, with `scale` the domain's scale and `lut_max` its last index. `_mm512_fmadd_ps` is
/// one rounding, as `_mm256_fmadd_ps`.
pub(crate) fn apply_lut_avx512(lut: &[f32], v: f32, scale: f32, lut_max: f32) -> f32 {
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

    // `_mm512_i32gather_ps(index, lut, sizeof(float))`: signed 32-bit indices, non-negative
    // here.
    let p = lut[prev_i as usize];
    let n = lut[next_i as usize];

    // lerp: a + (b - a) * t;
    mm256_fmadd_ps(n - p, d, p)
}

/// One value of `AVX512RGBAPack<outBD>::Store` and `StoreMasked`
/// (src/OpenColorIO/AVX512.h:80-461 @ v2.5.2), as the raw bits of the output channel type.
pub(crate) fn store(depth: PackDepth, value: f32) -> u32 {
    avx512_rgba_pack_store(depth, value)
}

/// The real AVX-512 kernel (`docs/cards/phase8.md`). Its lanes never mix, so it renders whole
/// groups of 16 pixels, and the exact lanes above render the rest. No NaN reaches its
/// arithmetic, so no operand order can show: the clamp makes a NaN 0, and the only NaN after it
/// is the default NaN of `Inf * 0` on a node.
#[cfg(target_arch = "x86_64")]
pub(super) mod simd {
    use core::arch::x86_64::*;

    use super::super::lut1d_op_cpu::KernelLut;
    use crate::avx512::packs::{load_f32, store_f16, store_f32, store_u8, store_u16};
    use crate::op::PixelsMut;
    use crate::sse2::PackDepth;

    /// `_mm512_i32gather_ps(indices, lut, sizeof(float))`.
    ///
    /// # Safety
    ///
    /// Each of `indices` is a valid index of `lut`.
    #[inline]
    #[target_feature(enable = "avx512f")]
    unsafe fn gather(lut: &[f32], indices: __m512i) -> __m512 {
        // SAFETY: the caller's.
        unsafe { _mm512_i32gather_ps::<4>(indices, lut.as_ptr()) }
    }

    /// Port of `apply_lut_avx512` (Lut1DOpCPU_AVX512.cpp:18-41 @ v2.5.2), for a table of more
    /// than `lut_max` values.
    #[inline]
    #[target_feature(enable = "avx512f")]
    fn apply_lut_avx512(lut: &[f32], v: __m512, scale: __m512, lut_max: __m512) -> __m512 {
        let zero = _mm512_setzero_ps();
        let one_f = _mm512_set1_ps(1.0);

        let scaled = _mm512_mul_ps(v, scale);

        // clamp, max first, NAN set to zero
        let x = _mm512_min_ps(_mm512_max_ps(scaled, zero), lut_max);
        // `_mm512_floor_ps`: to the integer below, keeping the exceptions quiet.
        let prev_f = _mm512_roundscale_ps::<0x09>(x);
        let d = _mm512_sub_ps(x, prev_f);
        let next_f = _mm512_min_ps(_mm512_add_ps(prev_f, one_f), lut_max);

        let prev_i = _mm512_cvttps_epi32(prev_f);
        let next_i = _mm512_cvttps_epi32(next_f);

        // SAFETY: both are integers from 0 to `lut_max`, as `x` is.
        let (p, n) = unsafe { (gather(lut, prev_i), gather(lut, next_i)) };

        // lerp: a + (b - a) * t;
        _mm512_fmadd_ps(_mm512_sub_ps(n, p), d, p)
    }

    /// Renders red, green and blue of each group of pixels in `rgba`, in place: the store's
    /// mask leaves alpha unwritten. Returns the number of values rendered.
    #[target_feature(enable = "avx512f")]
    fn in_place(lut: &KernelLut<'_>, rgba: &mut [f32]) -> usize {
        let scale = _mm512_set1_ps(lut.scale);
        let lut_max = _mm512_set1_ps(lut.lut_max);
        let (groups, _) = rgba.as_chunks_mut::<64>();
        for group in groups.iter_mut() {
            let [r, g, b, a] = load_f32(group);
            let r = apply_lut_avx512(lut.r, r, scale, lut_max);
            let g = apply_lut_avx512(lut.g, g, scale, lut_max);
            let b = apply_lut_avx512(lut.b, b, scale, lut_max);
            store_f32(group, [r, g, b, a], 0x7777);
        }
        groups.len() * 64
    }

    /// The loop of `linear1D<BIT_DEPTH_F32, outBD>` (Lut1DOpCPU_AVX512.cpp:64-78 @ v2.5.2) with
    /// `inBD != outBD`, for `store`, the pack of `outBD`.
    #[inline]
    #[target_feature(enable = "avx512f")]
    fn convert<T>(
        lut: &KernelLut<'_>,
        input: &[f32],
        output: &mut [T],
        store: impl Fn(&mut [T; 64], [__m512; 4]),
    ) -> usize {
        let scale = _mm512_set1_ps(lut.scale);
        let lut_max = _mm512_set1_ps(lut.lut_max);
        let alpha_scale = _mm512_set1_ps(lut.alpha_scale);
        let (groups, _) = input.as_chunks::<64>();
        let (outputs, _) = output.as_chunks_mut::<64>();
        for (group, out) in groups.iter().zip(outputs) {
            let [r, g, b, a] = load_f32(group);
            let r = apply_lut_avx512(lut.r, r, scale, lut_max);
            let g = apply_lut_avx512(lut.g, g, scale, lut_max);
            let b = apply_lut_avx512(lut.b, b, scale, lut_max);
            let a = _mm512_mul_ps(a, alpha_scale);
            store(out, [r, g, b, a]);
        }
        groups.len() * 64
    }

    #[target_feature(enable = "avx512f")]
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

    /// Whether this CPU runs this kernel's instructions. A renderer can be built for another
    /// CPU's kernel: its exact lanes then render the row.
    pub(in super::super) fn runs_here() -> bool {
        std::arch::is_x86_feature_detected!("avx512f")
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
        // SAFETY: `runs_here`.
        unsafe { to_output(lut, depth, input, output) }
    }
}
