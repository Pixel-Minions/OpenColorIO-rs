// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The SSE2 tetrahedral kernel, `applyTetrahedralSSE2`
//! (src/OpenColorIO/ops/lut3d/Lut3DOpCPU_SSE2.cpp:14-317 @ v2.5.2), as exact scalar code.
//!
//! The kernel's lanes never mix: the RGBA transposes only move bits, each gather reads its own
//! lane's index, and the zero-padded last block adds lanes whose results are discarded. So each
//! pixel is one call of [`interp_tetrahedral_sse2`], which follows the C++ operation for
//! operation. Multiply-adds are a multiply and an add (`fmadd_ps_sse2`, no FMA).
//!
//! No NaN reaches this arithmetic, so plain `+` and `*` are exact (CLAUDE.md, "NaN operand
//! order"): the inputs are clamped (NaN becomes 0), the LUT values are sanitized, and the
//! weights are in `[0, 1]`. The sums can overflow to an infinity, but never become NaN.

#![allow(unsafe_code)]

use crate::math_utils::{sse_cvttps_epi32, sse_max, sse_min};
use crate::sse2::{mm_andnot_ps, mm_cmpgt_ps, sse2_blendv};

/// Port of `Lut3DContextSSE2` (src/OpenColorIO/ops/lut3d/Lut3DOpCPU_SSE2.cpp:14-19 @ v2.5.2).
struct Lut3DContextSse2<'a> {
    lut: &'a [f32],
    lutmax: f32,
    lutsize: f32,
    lutsize2: f32,
}

/// One lane of `floor_ps_sse2` (src/OpenColorIO/ops/lut3d/Lut3DOpCPU_SSE2.cpp:39-47 @ v2.5.2):
/// truncation, `_mm_cvtepi32_ps(_mm_cvttps_epi32(v))`.
fn floor_ps_sse2(v: f32) -> f32 {
    sse_cvttps_epi32(v) as f32
}

/// One lane of `blendv_ps_sse2` (src/OpenColorIO/ops/lut3d/Lut3DOpCPU_SSE2.cpp:49-56 @ v2.5.2).
fn blendv_ps_sse2(a: f32, b: f32, mask: u32) -> f32 {
    f32::from_bits(sse2_blendv(a.to_bits(), b.to_bits(), mask))
}

/// One lane of `fmadd_ps_sse2` (src/OpenColorIO/ops/lut3d/Lut3DOpCPU_SSE2.cpp:58-67 @ v2.5.2):
/// `_mm_add_ps(_mm_mul_ps(a, b), c)`, two roundings.
fn fmadd_ps_sse2(a: f32, b: f32, c: f32) -> f32 {
    a * b + c
}

/// One lane of `gather_rgb_sse2` (src/OpenColorIO/ops/lut3d/Lut3DOpCPU_SSE2.cpp:25-37 @ v2.5.2).
fn gather_rgb_sse2(lut: &[f32], index: i32) -> [f32; 3] {
    let at = index as usize;
    [lut[at], lut[at + 1], lut[at + 2]]
}

/// One lane of `interp_tetrahedral_sse2`
/// (src/OpenColorIO/ops/lut3d/Lut3DOpCPU_SSE2.cpp:69-216 @ v2.5.2), for scaled and clamped
/// `r`, `g` and `b`. Returns the red, green and blue results.
fn interp_tetrahedral_sse2(ctx: &Lut3DContextSse2<'_>, r: f32, g: f32, b: f32) -> [f32; 3] {
    let lut_max = ctx.lutmax;
    let lutsize = ctx.lutsize;
    let lutsize2 = ctx.lutsize2;

    let one_f = 1.0f32;
    let four_f = 4.0f32;

    let mut prev_r = floor_ps_sse2(r);
    let mut prev_g = floor_ps_sse2(g);
    let mut prev_b = floor_ps_sse2(b);

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

    let gt_r = mm_cmpgt_ps(d_r, d_g); // r>g
    let gt_g = mm_cmpgt_ps(d_g, d_b); // g>b
    let gt_b = mm_cmpgt_ps(d_b, d_r); // b>r

    // r> !b>r && r>g
    let mask = mm_andnot_ps(gt_b, gt_r);
    let mut cxxxa = blendv_ps_sse2(prev_r, next_r, mask);

    // r< !r>g && b>r
    let mask = mm_andnot_ps(gt_r, gt_b);
    let mut cxxxb = blendv_ps_sse2(next_r, prev_r, mask);

    // g> !r>g && g>b
    let mask = mm_andnot_ps(gt_r, gt_g);
    cxxxa += blendv_ps_sse2(prev_g, next_g, mask);

    // g< !g>b && r>g
    let mask = mm_andnot_ps(gt_g, gt_r);
    cxxxb += blendv_ps_sse2(next_g, prev_g, mask);

    // b> !g>b && b>r
    let mask = mm_andnot_ps(gt_g, gt_b);
    cxxxa += blendv_ps_sse2(prev_b, next_b, mask);

    // b< !b>r && g>b
    let mask = mm_andnot_ps(gt_b, gt_g);
    cxxxb += blendv_ps_sse2(next_b, prev_b, mask);

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

    let sample = gather_rgb_sse2(ctx.lut, c000_idx);

    // (1-x0) * c000
    let v = one_f - x0;
    let mut result = sample.map(|s| s * v);

    let sample = gather_rgb_sse2(ctx.lut, cxxxa_idx);

    // (x0-x1) * cxxxa
    let v = x0 - x1;
    result = std::array::from_fn(|c| fmadd_ps_sse2(v, sample[c], result[c]));

    let sample = gather_rgb_sse2(ctx.lut, cxxxb_idx);

    // (x1-x2) * cxxxb
    let v = x1 - x2;
    result = std::array::from_fn(|c| fmadd_ps_sse2(v, sample[c], result[c]));

    let sample = gather_rgb_sse2(ctx.lut, c111_idx);

    // x2 * c111
    std::array::from_fn(|c| fmadd_ps_sse2(x2, sample[c], result[c]))
}

/// Port of `applyTetrahedralSSE2` and `applyTetrahedralSSE2Func<F32, F32>`
/// (src/OpenColorIO/ops/lut3d/Lut3DOpCPU_SSE2.cpp:218-317 @ v2.5.2), in place. `lut3d` holds 4
/// floats per entry. Alpha is never written, so it keeps every bit (see "Alpha" in
/// [`super::lut3d_op_cpu`]).
pub(crate) fn apply_tetrahedral_lanes(
    lut3d: &[f32],
    dim: i32,
    rgba: &mut [f32],
    total_pixel_count: i32,
) {
    let lutmax = dim as f32 - 1.0;
    let scale = lutmax;
    let zero = 0.0f32;

    let ctx = Lut3DContextSse2 {
        lut: lut3d,
        lutmax,
        lutsize: dim as f32 * 4.0,
        lutsize2: dim as f32 * dim as f32 * 4.0,
    };

    let pixels = total_pixel_count as usize;
    for px in rgba.as_chunks_mut::<4>().0.iter_mut().take(pixels) {
        // scale and clamp values
        let [r, g, b] =
            [px[0], px[1], px[2]].map(|v| sse_min(sse_max(v * scale, zero), ctx.lutmax));

        let c = interp_tetrahedral_sse2(&ctx, r, g, b);

        px[0] = c[0];
        px[1] = c[1];
        px[2] = c[2];
    }
}

/// The real kernel on whole groups of pixels, where it can run, and the lanes on the rest.
pub(crate) fn apply_tetrahedral_sse2(lut3d: &[f32], dim: i32, rgba: &mut [f32], num_pixels: i32) {
    let values = 4 * num_pixels as usize;
    #[cfg(target_arch = "x86_64")]
    let done = simd::tetrahedral(lut3d, dim, &mut rgba[..values]);
    #[cfg(not(target_arch = "x86_64"))]
    let done = 0;
    apply_tetrahedral_lanes(lut3d, dim, &mut rgba[done..], ((values - done) / 4) as i32);
}

/// The real SSE2 kernel (`docs/cards/phase8.md`). Its lanes never mix, so it renders whole
/// groups of 4 pixels, and the exact lanes above render the rest. No operand order can show:
/// the clamp makes a NaN pixel 0, the table holds no NaN or infinity (`sanitize_float`), and the
/// weights are in `[0, 1]`. So a product is finite, a sum can only overflow, and the only NaN
/// left is the default NaN of an infinity minus an infinity, which nothing changes after.
#[cfg(target_arch = "x86_64")]
pub(super) mod simd {
    use core::arch::x86_64::*;

    use crate::sse2::blocks::store_rgb;
    use crate::sse2::packs::{load_f32, store_f32, transpose};

    /// The context of `interp_tetrahedral_sse2`, in registers.
    struct Context<'a> {
        lut: &'a [f32],
        lutmax: __m128,
        lutsize: __m128,
        lutsize2: __m128,
    }

    /// Whether the kernels can index `lut3d`, four floats per entry, with float arithmetic: the
    /// largest index, `4 * (dim^3 - 1)`, stays exact up to the grid sizes OCIO allows (129).
    pub(in super::super) fn fits(lut3d: &[f32], dim: i32) -> bool {
        (1..=129).contains(&dim) && lut3d.len() == 4 * (dim * dim * dim) as usize
    }

    /// `gather_rgb_sse2` (Lut3DOpCPU_SSE2.cpp:25-37 @ v2.5.2): the red, green and blue at each
    /// index. The transpose is the C++'s, whose alpha it doesn't keep.
    ///
    /// # Safety
    ///
    /// Each of `indices` has four floats of `lut` from it.
    #[inline]
    #[target_feature(enable = "sse2")]
    unsafe fn gather_rgb(lut: &[f32], indices: __m128i) -> [__m128; 3] {
        let mut at = [0u32; 4];
        // SAFETY: `at` holds the sixteen bytes this writes, at any alignment.
        unsafe { _mm_storeu_si128(at.as_mut_ptr().cast(), indices) };
        // SAFETY: the caller's.
        let rows = at.map(|i| unsafe { _mm_loadu_ps(lut.as_ptr().add(i as usize)) });
        let [r, g, b, _] = transpose(rows);
        [r, g, b]
    }

    /// Port of `floor_ps_sse2` (Lut3DOpCPU_SSE2.cpp:39-47 @ v2.5.2).
    #[inline]
    #[target_feature(enable = "sse2")]
    fn floor_ps(v: __m128) -> __m128 {
        _mm_cvtepi32_ps(_mm_cvttps_epi32(v))
    }

    /// Port of `blendv_ps_sse2` (Lut3DOpCPU_SSE2.cpp:49-56 @ v2.5.2).
    #[inline]
    #[target_feature(enable = "sse2")]
    fn blendv_ps(a: __m128, b: __m128, mask: __m128) -> __m128 {
        _mm_xor_ps(_mm_and_ps(_mm_xor_ps(a, b), mask), a)
    }

    /// Port of `fmadd_ps_sse2` (Lut3DOpCPU_SSE2.cpp:58-67 @ v2.5.2).
    #[inline]
    #[target_feature(enable = "sse2")]
    fn fmadd_ps(a: __m128, b: __m128, c: __m128) -> __m128 {
        _mm_add_ps(_mm_mul_ps(a, b), c)
    }

    /// Port of `interp_tetrahedral_sse2` (Lut3DOpCPU_SSE2.cpp:69-216 @ v2.5.2), for scaled and
    /// clamped `r`, `g` and `b`.
    #[inline]
    #[target_feature(enable = "sse2")]
    fn interp_tetrahedral(ctx: &Context<'_>, r: __m128, g: __m128, b: __m128) -> [__m128; 3] {
        let one_f = _mm_set1_ps(1.0);
        let four_f = _mm_set1_ps(4.0);

        let prev_r = floor_ps(r);
        let prev_g = floor_ps(g);
        let prev_b = floor_ps(b);

        // rgb delta values
        let d_r = _mm_sub_ps(r, prev_r);
        let d_g = _mm_sub_ps(g, prev_g);
        let d_b = _mm_sub_ps(b, prev_b);

        let next_r = _mm_min_ps(ctx.lutmax, _mm_add_ps(prev_r, one_f));
        let next_g = _mm_min_ps(ctx.lutmax, _mm_add_ps(prev_g, one_f));
        let next_b = _mm_min_ps(ctx.lutmax, _mm_add_ps(prev_b, one_f));

        // prescale indices
        let prev_r = _mm_mul_ps(prev_r, ctx.lutsize2);
        let next_r = _mm_mul_ps(next_r, ctx.lutsize2);

        let prev_g = _mm_mul_ps(prev_g, ctx.lutsize);
        let next_g = _mm_mul_ps(next_g, ctx.lutsize);

        let prev_b = _mm_mul_ps(prev_b, four_f);
        let next_b = _mm_mul_ps(next_b, four_f);

        let gt_r = _mm_cmpgt_ps(d_r, d_g); // r>g
        let gt_g = _mm_cmpgt_ps(d_g, d_b); // g>b
        let gt_b = _mm_cmpgt_ps(d_b, d_r); // b>r

        // r> !b>r && r>g
        let mask = _mm_andnot_ps(gt_b, gt_r);
        let cxxxa = blendv_ps(prev_r, next_r, mask);

        // r< !r>g && b>r
        let mask = _mm_andnot_ps(gt_r, gt_b);
        let cxxxb = blendv_ps(next_r, prev_r, mask);

        // g> !r>g && g>b
        let mask = _mm_andnot_ps(gt_r, gt_g);
        let cxxxa = _mm_add_ps(cxxxa, blendv_ps(prev_g, next_g, mask));

        // g< !g>b && r>g
        let mask = _mm_andnot_ps(gt_g, gt_r);
        let cxxxb = _mm_add_ps(cxxxb, blendv_ps(next_g, prev_g, mask));

        // b> !g>b && b>r
        let mask = _mm_andnot_ps(gt_g, gt_b);
        let cxxxa = _mm_add_ps(cxxxa, blendv_ps(prev_b, next_b, mask));

        // b< !b>r && g>b
        let mask = _mm_andnot_ps(gt_b, gt_g);
        let cxxxb = _mm_add_ps(cxxxb, blendv_ps(next_b, prev_b, mask));

        let c000 = _mm_add_ps(_mm_add_ps(prev_r, prev_g), prev_b);
        let c111 = _mm_add_ps(_mm_add_ps(next_r, next_g), next_b);

        // sort delta r,g,b x0 >= x1 >= x2
        let rg_min = _mm_min_ps(d_r, d_g);
        let rg_max = _mm_max_ps(d_r, d_g);

        let x2 = _mm_min_ps(rg_min, d_b);
        let mid = _mm_max_ps(rg_min, d_b);

        let x0 = _mm_max_ps(rg_max, d_b);
        let x1 = _mm_min_ps(rg_max, mid);

        // convert indices to int
        let c000_idx = _mm_cvttps_epi32(c000);
        let cxxxa_idx = _mm_cvttps_epi32(cxxxa);
        let cxxxb_idx = _mm_cvttps_epi32(cxxxb);
        let c111_idx = _mm_cvttps_epi32(c111);

        // SAFETY: `fits`, and the inputs are clamped to `[0, lutmax]`: each index is an entry's.
        let sample = unsafe { gather_rgb(ctx.lut, c000_idx) };

        // (1-x0) * c000
        let v = _mm_sub_ps(one_f, x0);
        let mut result = sample.map(|s| _mm_mul_ps(s, v));

        // SAFETY: as above.
        let sample = unsafe { gather_rgb(ctx.lut, cxxxa_idx) };

        // (x0-x1) * cxxxa
        let v = _mm_sub_ps(x0, x1);
        result = std::array::from_fn(|c| fmadd_ps(v, sample[c], result[c]));

        // SAFETY: as above.
        let sample = unsafe { gather_rgb(ctx.lut, cxxxb_idx) };

        // (x1-x2) * cxxxb
        let v = _mm_sub_ps(x1, x2);
        result = std::array::from_fn(|c| fmadd_ps(v, sample[c], result[c]));

        // SAFETY: as above.
        let sample = unsafe { gather_rgb(ctx.lut, c111_idx) };

        // x2 * c111
        std::array::from_fn(|c| fmadd_ps(x2, sample[c], result[c]))
    }

    /// The loop of `applyTetrahedralSSE2Func<F32, F32>` (Lut3DOpCPU_SSE2.cpp:252-278 @ v2.5.2).
    /// Alpha's bits only move through the transposes. Returns the number of values rendered.
    #[target_feature(enable = "sse2")]
    fn tetrahedral_loop(lut3d: &[f32], dim: i32, rgba: &mut [f32]) -> usize {
        let lutmax = dim as f32 - 1.0;
        let scale = _mm_set1_ps(lutmax);
        let zero = _mm_setzero_ps();
        let ctx = Context {
            lut: lut3d,
            lutmax: _mm_set1_ps(lutmax),
            lutsize: _mm_set1_ps(dim as f32 * 4.0),
            lutsize2: _mm_set1_ps(dim as f32 * dim as f32 * 4.0),
        };
        let (groups, _) = rgba.as_chunks_mut::<16>();
        for group in groups.iter_mut() {
            let [r, g, b, a] = load_f32(group);

            // scale and clamp values
            let r = _mm_min_ps(_mm_max_ps(_mm_mul_ps(r, scale), zero), ctx.lutmax);
            let g = _mm_min_ps(_mm_max_ps(_mm_mul_ps(g, scale), zero), ctx.lutmax);
            let b = _mm_min_ps(_mm_max_ps(_mm_mul_ps(b, scale), zero), ctx.lutmax);

            let [r, g, b] = interp_tetrahedral(&ctx, r, g, b);

            store_f32(group, [r, g, b, a]);
        }
        groups.len() * 16
    }

    /// See [`tetrahedral_loop`]; nothing where the table doesn't [`fits`].
    pub(in super::super) fn tetrahedral(lut3d: &[f32], dim: i32, rgba: &mut [f32]) -> usize {
        if !fits(lut3d, dim) {
            return 0;
        }
        // SAFETY: x86-64 has SSE2.
        unsafe { tetrahedral_loop(lut3d, dim, rgba) }
    }

    /// Port of `GetLut3DIndices` (Lut3DOpCPU.cpp:204-250 @ v2.5.2) with one size for the axes.
    #[inline]
    #[target_feature(enable = "sse2")]
    fn lut3d_indices(idx_r: __m128i, idx_g: __m128i, idx_b: __m128i, dim: __m128i) -> __m128i {
        let r02 = _mm_mul_epu32(dim, idx_r);
        let r13 = _mm_mul_epu32(dim, _mm_srli_si128::<4>(idx_r));

        let r02 = _mm_add_epi32(idx_g, r02);
        let r13 = _mm_add_epi32(_mm_srli_si128::<4>(idx_g), r13);

        let r02 = _mm_mul_epu32(dim, r02);
        let r13 = _mm_mul_epu32(dim, r13);

        let r02 = _mm_add_epi32(idx_b, r02);
        let r13 = _mm_add_epi32(_mm_srli_si128::<4>(idx_b), r13);

        let r = _mm_unpacklo_epi32(
            _mm_shuffle_epi32::<0b00_00_10_00>(r02),
            _mm_shuffle_epi32::<0b00_00_10_00>(r13),
        );

        _mm_slli_epi32::<2>(r)
    }

    /// Port of `LookupNearest4` (Lut3DOpCPU.cpp:252-264 @ v2.5.2).
    ///
    /// # Safety
    ///
    /// The indices are of a grid of size `dim` that `lut` [`fits`].
    #[inline]
    #[target_feature(enable = "sse2")]
    unsafe fn lookup_nearest4(
        lut: &[f32],
        r: __m128i,
        g: __m128i,
        b: __m128i,
        dim: __m128i,
    ) -> [__m128; 4] {
        let offsets = lut3d_indices(r, g, b, dim);
        let mut at = [0u32; 4];
        // SAFETY: `at` holds the sixteen bytes this writes, at any alignment.
        unsafe { _mm_storeu_si128(at.as_mut_ptr().cast(), offsets) };
        // SAFETY: the caller's: each offset is that of an entry's four floats.
        at.map(|i| unsafe { _mm_loadu_ps(lut.as_ptr().add(i as usize)) })
    }

    /// `a * (1 - w) + b * w`.
    #[inline]
    #[target_feature(enable = "sse2")]
    fn lerp(a: __m128, b: __m128, one_minus_w: __m128, w: __m128) -> __m128 {
        _mm_add_ps(_mm_mul_ps(a, one_minus_w), _mm_mul_ps(b, w))
    }

    /// The `OCIO_USE_SSE2` loop of `Lut3DRenderer::apply` (Lut3DOpCPU.cpp:640-739 @ v2.5.2).
    #[target_feature(enable = "sse2")]
    fn trilinear_loop(lut: &[f32], grid: i32, step: f32, rgba: &mut [f32]) {
        let step = _mm_set1_ps(step);
        let max_idx = _mm_set1_ps((grid - 1) as f32);
        let dim = _mm_set1_epi32(grid);
        let one = _mm_set1_ps(1.0);

        for px in rgba.as_chunks_mut::<4>().0 {
            // SAFETY: `px` holds the four floats this reads, at any alignment.
            let data = unsafe { _mm_loadu_ps(px.as_ptr()) };

            let idx = _mm_mul_ps(data, step);

            let idx = _mm_max_ps(idx, _mm_setzero_ps()); // NaNs become 0
            let idx = _mm_min_ps(idx, max_idx);

            // lowIdxInt32 = floor(idx), with lowIdx in [0, maxIdx]
            let low_idx_int32 = _mm_cvttps_epi32(idx);
            let low_idx = _mm_cvtepi32_ps(low_idx_int32);

            // highIdxInt32 = ceil(idx), with highIdx in [1, maxIdx]
            let high_idx_int32 = _mm_sub_epi32(
                low_idx_int32,
                _mm_castps_si128(_mm_cmplt_ps(low_idx, max_idx)),
            );

            let delta = _mm_sub_ps(idx, low_idx);

            // lh01 = {L0, H0, L1, H1}
            // lh23 = {L2, H2, L3, H3}, L3 and H3 are not used
            let lh01 = _mm_unpacklo_epi32(low_idx_int32, high_idx_int32);
            let lh23 = _mm_unpackhi_epi32(low_idx_int32, high_idx_int32);

            let idx_r_l0 = _mm_shuffle_epi32::<0b00_00_00_00>(lh01);
            let idx_r_h0 = _mm_shuffle_epi32::<0b01_01_01_01>(lh01);
            let idx_g = _mm_unpackhi_epi32(lh01, lh01);
            let idx_b = _mm_unpacklo_epi64(lh23, lh23);

            // Lookup 8 corners of cube
            // SAFETY: the table [`fits`], and the indices are in `[0, maxIdx]`.
            let [v0, v1, v2, v3] = unsafe { lookup_nearest4(lut, idx_r_l0, idx_g, idx_b, dim) };
            // SAFETY: as above.
            let [v4, v5, v6, v7] = unsafe { lookup_nearest4(lut, idx_r_h0, idx_g, idx_b, dim) };

            // Perform the trilinear interpolation
            let wr = _mm_shuffle_ps::<0b00_00_00_00>(delta, delta);
            let wg = _mm_shuffle_ps::<0b01_01_01_01>(delta, delta);
            let wb = _mm_shuffle_ps::<0b10_10_10_10>(delta, delta);

            let one_minus_wr = _mm_sub_ps(one, wr);
            let one_minus_wg = _mm_sub_ps(one, wg);
            let one_minus_wb = _mm_sub_ps(one, wb);

            // Compute linear interpolation along the blue axis
            let blue1 = lerp(v0, v1, one_minus_wb, wb);
            let blue2 = lerp(v2, v3, one_minus_wb, wb);
            let blue3 = lerp(v4, v5, one_minus_wb, wb);
            let blue4 = lerp(v6, v7, one_minus_wb, wb);

            // Compute linear interpolation along the green axis
            let green1 = lerp(blue1, blue2, one_minus_wg, wg);
            let green2 = lerp(blue3, blue4, one_minus_wg, wg);

            // Compute linear interpolation along the red axis
            store_rgb(px, lerp(green1, green2, one_minus_wr, wr));
        }
    }

    /// Renders `rgba` in place, if the table [`fits`]: whether it did.
    pub(in super::super) fn trilinear(lut: &[f32], dim: i32, step: f32, rgba: &mut [f32]) -> bool {
        if !fits(lut, dim) {
            return false;
        }
        // SAFETY: x86-64 has SSE2.
        unsafe { trilinear_loop(lut, dim, step, rgba) };
        true
    }
}
