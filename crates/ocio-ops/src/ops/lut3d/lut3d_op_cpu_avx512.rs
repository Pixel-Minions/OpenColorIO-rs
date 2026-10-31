// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The AVX-512 tetrahedral kernel, `applyTetrahedralAVX512`
//! (src/OpenColorIO/ops/lut3d/Lut3DOpCPU_AVX512.cpp:16-255 @ v2.5.2), as exact scalar code.
//!
//! The lanes never mix (the last block uses masked loads and stores instead of a padded
//! buffer), so each pixel is one call of [`interp_tetrahedral_avx512`]. Compares produce mask
//! bits, blends select by mask bit, and the three multiply-adds are FMA instructions
//! (`_mm512_fmadd_ps`, one rounding), ported with `f32::mul_add`.
//!
//! No NaN reaches this arithmetic, so plain `+` and `*` are exact (CLAUDE.md, "NaN operand
//! order"): the inputs are clamped (NaN becomes 0), the LUT values are sanitized, and the
//! weights are in `[0, 1]`. The sums can overflow to an infinity, but never become NaN.

#![allow(unsafe_code)]

use crate::avx2::mm256_fmadd_ps as mm512_fmadd_ps;
use crate::avx512::{mm512_cmp_gt_oq, mm512_kandn, mm512_mask_blend_ps};
use crate::math_utils::{sse_cvttps_epi32, sse_max, sse_min};

/// Port of `Lut3DContextAVX512` (src/OpenColorIO/ops/lut3d/Lut3DOpCPU_AVX512.cpp:16-21 @ v2.5.2).
struct Lut3DContextAvx512<'a> {
    lut: &'a [f32],
    lutmax: f32,
    lutsize: f32,
    lutsize2: f32,
}

/// One lane of `gather_rgb_avx512` (src/OpenColorIO/ops/lut3d/Lut3DOpCPU_AVX512.cpp:27-30 @ v2.5.2):
/// three `_mm512_i32gather_ps` with a scale of 4 bytes.
fn gather_rgb_avx512(lut: &[f32], index: i32) -> [f32; 3] {
    let at = index as usize;
    [lut[at], lut[at + 1], lut[at + 2]]
}

/// One lane of `interp_tetrahedral_avx512`
/// (src/OpenColorIO/ops/lut3d/Lut3DOpCPU_AVX512.cpp:32-174 @ v2.5.2), for scaled and clamped
/// `r`, `g` and `b`. Returns the red, green and blue results.
fn interp_tetrahedral_avx512(ctx: &Lut3DContextAvx512<'_>, r: f32, g: f32, b: f32) -> [f32; 3] {
    let lut_max = ctx.lutmax;
    let lutsize = ctx.lutsize;
    let lutsize2 = ctx.lutsize2;

    let one_f = 1.0f32;
    let four_f = 4.0f32;

    let mut prev_r = r.floor();
    let mut prev_g = g.floor();
    let mut prev_b = b.floor();

    // rgb delta values
    let d_r = r - prev_r;
    let d_g = g - prev_g;
    let d_b = b - prev_b;

    let mut next_r = sse_min(lut_max, prev_r + one_f);
    let mut next_g = sse_min(lut_max, prev_g + one_f);
    let mut next_b = sse_min(lut_max, prev_b + one_f);

    // prescale indices
    prev_r *= lutsize2;
    next_r *= lutsize2;

    prev_g *= lutsize;
    next_g *= lutsize;

    prev_b *= four_f;
    next_b *= four_f;

    let gt_r = mm512_cmp_gt_oq(d_r, d_g); // r>g
    let gt_g = mm512_cmp_gt_oq(d_g, d_b); // g>b
    let gt_b = mm512_cmp_gt_oq(d_b, d_r); // b>r

    // r> !b>r && r>g
    let mask = mm512_kandn(gt_b, gt_r);
    let mut cxxxa = mm512_mask_blend_ps(mask, prev_r, next_r);

    // r< !r>g && b>r
    let mask = mm512_kandn(gt_r, gt_b);
    let mut cxxxb = mm512_mask_blend_ps(mask, next_r, prev_r);

    // g> !r>g && g>b
    let mask = mm512_kandn(gt_r, gt_g);
    cxxxa += mm512_mask_blend_ps(mask, prev_g, next_g);

    // g< !g>b && r>g
    let mask = mm512_kandn(gt_g, gt_r);
    cxxxb += mm512_mask_blend_ps(mask, next_g, prev_g);

    // b> !g>b && b>r
    let mask = mm512_kandn(gt_g, gt_b);
    cxxxa += mm512_mask_blend_ps(mask, prev_b, next_b);

    // b< !b>r && g>b
    let mask = mm512_kandn(gt_b, gt_g);
    cxxxb += mm512_mask_blend_ps(mask, next_b, prev_b);

    let c000 = (prev_r + prev_g) + prev_b;
    let c111 = (next_r + next_g) + next_b;

    // sort delta r,g,b x0 >= x1 >= x2
    let rg_min = sse_min(d_r, d_g);
    let rg_max = sse_max(d_r, d_g);

    let x2 = sse_min(rg_min, d_b);
    let mid = sse_max(rg_min, d_b);

    let x0 = sse_max(rg_max, d_b);
    let x1 = sse_min(rg_max, mid);

    // convert indices to int
    let c000_idx = sse_cvttps_epi32(c000);
    let cxxxa_idx = sse_cvttps_epi32(cxxxa);
    let cxxxb_idx = sse_cvttps_epi32(cxxxb);
    let c111_idx = sse_cvttps_epi32(c111);

    let sample = gather_rgb_avx512(ctx.lut, c000_idx);

    // (1-x0) * c000
    let v = one_f - x0;
    let mut result = sample.map(|s| s * v);

    let sample = gather_rgb_avx512(ctx.lut, cxxxa_idx);

    // (x0-x1) * cxxxa
    let v = x0 - x1;
    result = std::array::from_fn(|c| mm512_fmadd_ps(v, sample[c], result[c]));

    let sample = gather_rgb_avx512(ctx.lut, cxxxb_idx);

    // (x1-x2) * cxxxb
    let v = x1 - x2;
    result = std::array::from_fn(|c| mm512_fmadd_ps(v, sample[c], result[c]));

    let sample = gather_rgb_avx512(ctx.lut, c111_idx);

    // x2 * c111
    std::array::from_fn(|c| mm512_fmadd_ps(x2, sample[c], result[c]))
}

/// Port of `applyTetrahedralAVX512` and `applyTetrahedralAVX512Func<F32, F32>`
/// (src/OpenColorIO/ops/lut3d/Lut3DOpCPU_AVX512.cpp:176-255 @ v2.5.2), in place. `lut3d` holds
/// 4 floats per entry. Alpha is never written, so it keeps every bit (see "Alpha" in
/// [`super::lut3d_op_cpu`]).
pub(crate) fn apply_tetrahedral_lanes(lut3d: &[f32], dim: i32, rgba: &mut [f32], num_pixels: i32) {
    let lutmax = dim as f32 - 1.0;
    let scale = lutmax;
    let zero = 0.0f32;

    let ctx = Lut3DContextAvx512 {
        lut: lut3d,
        lutmax,
        lutsize: dim as f32 * 4.0,
        lutsize2: dim as f32 * dim as f32 * 4.0,
    };

    let pixels = num_pixels as usize;
    for px in rgba.as_chunks_mut::<4>().0.iter_mut().take(pixels) {
        // scale and clamp values
        let [r, g, b] =
            [px[0], px[1], px[2]].map(|v| sse_min(sse_max(v * scale, zero), ctx.lutmax));

        let c = interp_tetrahedral_avx512(&ctx, r, g, b);

        px[0] = c[0];
        px[1] = c[1];
        px[2] = c[2];
    }
}

/// The real kernel on whole groups of pixels, where it can run, and the lanes on the rest.
pub(crate) fn apply_tetrahedral_avx512(lut3d: &[f32], dim: i32, rgba: &mut [f32], num_pixels: i32) {
    let values = 4 * num_pixels as usize;
    #[cfg(target_arch = "x86_64")]
    let done = simd::tetrahedral(lut3d, dim, &mut rgba[..values]);
    #[cfg(not(target_arch = "x86_64"))]
    let done = 0;
    apply_tetrahedral_lanes(lut3d, dim, &mut rgba[done..], ((values - done) / 4) as i32);
}

/// The real AVX-512 kernel (`docs/cards/phase8.md`). Its lanes never mix, so it renders whole
/// groups of 16 pixels, and the exact lanes above render the rest. No operand order can show:
/// the clamp makes a NaN pixel 0, the table holds no NaN or infinity (`sanitize_float`), and the
/// weights are in `[0, 1]`. So a product is finite, a sum can only overflow, and the only NaN
/// left is the default NaN of an infinity minus an infinity, which nothing changes after.
#[cfg(target_arch = "x86_64")]
pub(super) mod simd {
    use core::arch::x86_64::*;

    use super::super::lut3d_op_cpu_sse2::simd::fits;
    use crate::avx512::packs::{load_f32, store_f32};

    /// The context of `interp_tetrahedral_avx512`, in registers.
    struct Context<'a> {
        lut: &'a [f32],
        lutmax: __m512,
        lutsize: __m512,
        lutsize2: __m512,
    }

    /// `gather_rgb_avx512` (Lut3DOpCPU_AVX512.cpp:28-31 @ v2.5.2).
    ///
    /// # Safety
    ///
    /// Each of `indices` has three floats of `lut` from it.
    #[inline]
    #[target_feature(enable = "avx512f")]
    unsafe fn gather_rgb(lut: &[f32], indices: __m512i) -> [__m512; 3] {
        let at = lut.as_ptr();
        // SAFETY: the caller's.
        unsafe {
            [
                _mm512_i32gather_ps::<4>(indices, at),
                _mm512_i32gather_ps::<4>(indices, at.add(1)),
                _mm512_i32gather_ps::<4>(indices, at.add(2)),
            ]
        }
    }

    /// `_mm512_floor_ps`, keeping the exceptions quiet.
    #[inline]
    #[target_feature(enable = "avx512f")]
    fn floor_ps(v: __m512) -> __m512 {
        _mm512_roundscale_ps::<0x09>(v)
    }

    /// Port of `interp_tetrahedral_avx512` (Lut3DOpCPU_AVX512.cpp:33-175 @ v2.5.2), for scaled
    /// and clamped `r`, `g` and `b`.
    #[inline]
    #[target_feature(enable = "avx512f")]
    fn interp_tetrahedral(ctx: &Context<'_>, r: __m512, g: __m512, b: __m512) -> [__m512; 3] {
        let one_f = _mm512_set1_ps(1.0);
        let four_f = _mm512_set1_ps(4.0);

        let prev_r = floor_ps(r);
        let prev_g = floor_ps(g);
        let prev_b = floor_ps(b);

        // rgb delta values
        let d_r = _mm512_sub_ps(r, prev_r);
        let d_g = _mm512_sub_ps(g, prev_g);
        let d_b = _mm512_sub_ps(b, prev_b);

        let next_r = _mm512_min_ps(ctx.lutmax, _mm512_add_ps(prev_r, one_f));
        let next_g = _mm512_min_ps(ctx.lutmax, _mm512_add_ps(prev_g, one_f));
        let next_b = _mm512_min_ps(ctx.lutmax, _mm512_add_ps(prev_b, one_f));

        // prescale indices
        let prev_r = _mm512_mul_ps(prev_r, ctx.lutsize2);
        let next_r = _mm512_mul_ps(next_r, ctx.lutsize2);

        let prev_g = _mm512_mul_ps(prev_g, ctx.lutsize);
        let next_g = _mm512_mul_ps(next_g, ctx.lutsize);

        let prev_b = _mm512_mul_ps(prev_b, four_f);
        let next_b = _mm512_mul_ps(next_b, four_f);

        let gt_r = _mm512_cmp_ps_mask::<_CMP_GT_OQ>(d_r, d_g); // r>g
        let gt_g = _mm512_cmp_ps_mask::<_CMP_GT_OQ>(d_g, d_b); // g>b
        let gt_b = _mm512_cmp_ps_mask::<_CMP_GT_OQ>(d_b, d_r); // b>r

        // r> !b>r && r>g
        let mask = _mm512_kandn(gt_b, gt_r);
        let cxxxa = _mm512_mask_blend_ps(mask, prev_r, next_r);

        // r< !r>g && b>r
        let mask = _mm512_kandn(gt_r, gt_b);
        let cxxxb = _mm512_mask_blend_ps(mask, next_r, prev_r);

        // g> !r>g && g>b
        let mask = _mm512_kandn(gt_r, gt_g);
        let cxxxa = _mm512_add_ps(cxxxa, _mm512_mask_blend_ps(mask, prev_g, next_g));

        // g< !g>b && r>g
        let mask = _mm512_kandn(gt_g, gt_r);
        let cxxxb = _mm512_add_ps(cxxxb, _mm512_mask_blend_ps(mask, next_g, prev_g));

        // b> !g>b && b>r
        let mask = _mm512_kandn(gt_g, gt_b);
        let cxxxa = _mm512_add_ps(cxxxa, _mm512_mask_blend_ps(mask, prev_b, next_b));

        // b< !b>r && g>b
        let mask = _mm512_kandn(gt_b, gt_g);
        let cxxxb = _mm512_add_ps(cxxxb, _mm512_mask_blend_ps(mask, next_b, prev_b));

        let c000 = _mm512_add_ps(_mm512_add_ps(prev_r, prev_g), prev_b);
        let c111 = _mm512_add_ps(_mm512_add_ps(next_r, next_g), next_b);

        // sort delta r,g,b x0 >= x1 >= x2
        let rg_min = _mm512_min_ps(d_r, d_g);
        let rg_max = _mm512_max_ps(d_r, d_g);

        let x2 = _mm512_min_ps(rg_min, d_b);
        let mid = _mm512_max_ps(rg_min, d_b);

        let x0 = _mm512_max_ps(rg_max, d_b);
        let x1 = _mm512_min_ps(rg_max, mid);

        // convert indices to int
        let c000_idx = _mm512_cvttps_epi32(c000);
        let cxxxa_idx = _mm512_cvttps_epi32(cxxxa);
        let cxxxb_idx = _mm512_cvttps_epi32(cxxxb);
        let c111_idx = _mm512_cvttps_epi32(c111);

        // SAFETY: `fits`, and the inputs are clamped to `[0, lutmax]`: each index is an entry's.
        let sample = unsafe { gather_rgb(ctx.lut, c000_idx) };

        // (1-x0) * c000
        let v = _mm512_sub_ps(one_f, x0);
        let mut result = sample.map(|s| _mm512_mul_ps(s, v));

        // SAFETY: as above.
        let sample = unsafe { gather_rgb(ctx.lut, cxxxa_idx) };

        // (x0-x1) * cxxxa
        let v = _mm512_sub_ps(x0, x1);
        result = std::array::from_fn(|c| _mm512_fmadd_ps(v, sample[c], result[c]));

        // SAFETY: as above.
        let sample = unsafe { gather_rgb(ctx.lut, cxxxb_idx) };

        // (x1-x2) * cxxxb
        let v = _mm512_sub_ps(x1, x2);
        result = std::array::from_fn(|c| _mm512_fmadd_ps(v, sample[c], result[c]));

        // SAFETY: as above.
        let sample = unsafe { gather_rgb(ctx.lut, c111_idx) };

        // x2 * c111
        std::array::from_fn(|c| _mm512_fmadd_ps(x2, sample[c], result[c]))
    }

    /// The main loop of `applyTetrahedralAVX512Func<F32, F32>` (Lut3DOpCPU_AVX512.cpp:198-219 @
    /// v2.5.2). The store's mask leaves alpha unwritten. Returns the number of values rendered.
    #[target_feature(enable = "avx512f")]
    fn tetrahedral_loop(lut3d: &[f32], dim: i32, rgba: &mut [f32]) -> usize {
        let lutmax = dim as f32 - 1.0;
        let scale = _mm512_set1_ps(lutmax);
        let zero = _mm512_setzero_ps();
        let ctx = Context {
            lut: lut3d,
            lutmax: _mm512_set1_ps(lutmax),
            lutsize: _mm512_set1_ps(dim as f32 * 4.0),
            lutsize2: _mm512_set1_ps(dim as f32 * dim as f32 * 4.0),
        };
        let (groups, _) = rgba.as_chunks_mut::<64>();
        for group in groups.iter_mut() {
            let [r, g, b, a] = load_f32(group);

            // scale and clamp values
            let r = _mm512_min_ps(_mm512_max_ps(_mm512_mul_ps(r, scale), zero), ctx.lutmax);
            let g = _mm512_min_ps(_mm512_max_ps(_mm512_mul_ps(g, scale), zero), ctx.lutmax);
            let b = _mm512_min_ps(_mm512_max_ps(_mm512_mul_ps(b, scale), zero), ctx.lutmax);

            let [r, g, b] = interp_tetrahedral(&ctx, r, g, b);

            store_f32(group, [r, g, b, a], 0x7777);
        }
        groups.len() * 64
    }

    /// See [`tetrahedral_loop`]; nothing where the CPU can't run it or the table doesn't fit.
    pub(in super::super) fn tetrahedral(lut3d: &[f32], dim: i32, rgba: &mut [f32]) -> usize {
        if !std::arch::is_x86_feature_detected!("avx512f") || !fits(lut3d, dim) {
            return 0;
        }
        // SAFETY: the CPU has AVX-512 F.
        unsafe { tetrahedral_loop(lut3d, dim, rgba) }
    }
}
