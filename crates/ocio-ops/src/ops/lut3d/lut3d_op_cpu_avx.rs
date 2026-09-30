// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The AVX tetrahedral kernel, `applyTetrahedralAVX`
//! (src/OpenColorIO/ops/lut3d/Lut3DOpCPU_AVX.cpp:15-323 @ v2.5.2), as exact scalar code.
//!
//! As in the SSE2 kernel, the lanes never mix, so each pixel is one call of
//! [`interp_tetrahedral_avx`]. Differences from SSE2: `_mm256_floor_ps` instead of truncation
//! (equal here, since the inputs are clamped to `[0, lutmax]`), and `_mm256_blendv_ps`. The
//! multiply-adds are still a multiply and an add (`fmadd_ps_avx`, no FMA).
//!
//! No NaN reaches this arithmetic, so plain `+` and `*` are exact (CLAUDE.md, "NaN operand
//! order"): the inputs are clamped (NaN becomes 0), the LUT values are sanitized, and the
//! weights are in `[0, 1]`. The sums can overflow to an infinity, but never become NaN.

use crate::avx::avx_blendv_ps;
use crate::math_utils::{sse_cvttps_epi32, sse_max, sse_min};
use crate::sse2::{mm_andnot_ps, mm_cmpgt_ps};

/// Port of `Lut3DContextAVX` (src/OpenColorIO/ops/lut3d/Lut3DOpCPU_AVX.cpp:15-20 @ v2.5.2).
struct Lut3DContextAvx<'a> {
    lut: &'a [f32],
    lutmax: f32,
    lutsize: f32,
    lutsize2: f32,
}

/// One lane of `fmadd_ps_avx` (src/OpenColorIO/ops/lut3d/Lut3DOpCPU_AVX.cpp:56-59 @ v2.5.2):
/// `_mm256_add_ps(_mm256_mul_ps(a, b), c)`, two roundings.
fn fmadd_ps_avx(a: f32, b: f32, c: f32) -> f32 {
    a * b + c
}

/// One lane of `gather_rgb_avx` (src/OpenColorIO/ops/lut3d/Lut3DOpCPU_AVX.cpp:42-54 @ v2.5.2).
fn gather_rgb_avx(lut: &[f32], index: i32) -> [f32; 3] {
    let at = index as usize;
    [lut[at], lut[at + 1], lut[at + 2]]
}

/// One lane of `interp_tetrahedral_avx`
/// (src/OpenColorIO/ops/lut3d/Lut3DOpCPU_AVX.cpp:75-222 @ v2.5.2), for scaled and clamped
/// `r`, `g` and `b`. Returns the red, green and blue results.
fn interp_tetrahedral_avx(ctx: &Lut3DContextAvx<'_>, r: f32, g: f32, b: f32) -> [f32; 3] {
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

    let gt_r = mm_cmpgt_ps(d_r, d_g); // r>g
    let gt_g = mm_cmpgt_ps(d_g, d_b); // g>b
    let gt_b = mm_cmpgt_ps(d_b, d_r); // b>r

    // r> !b>r && r>g
    let mask = mm_andnot_ps(gt_b, gt_r);
    let mut cxxxa = avx_blendv_ps(prev_r, next_r, mask);

    // r< !r>g && b>r
    let mask = mm_andnot_ps(gt_r, gt_b);
    let mut cxxxb = avx_blendv_ps(next_r, prev_r, mask);

    // g> !r>g && g>b
    let mask = mm_andnot_ps(gt_r, gt_g);
    cxxxa += avx_blendv_ps(prev_g, next_g, mask);

    // g< !g>b && r>g
    let mask = mm_andnot_ps(gt_g, gt_r);
    cxxxb += avx_blendv_ps(next_g, prev_g, mask);

    // b> !g>b && b>r
    let mask = mm_andnot_ps(gt_g, gt_b);
    cxxxa += avx_blendv_ps(prev_b, next_b, mask);

    // b< !b>r && g>b
    let mask = mm_andnot_ps(gt_b, gt_g);
    cxxxb += avx_blendv_ps(next_b, prev_b, mask);

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

    let sample = gather_rgb_avx(ctx.lut, c000_idx);

    // (1-x0) * c000
    let v = one_f - x0;
    let mut result = sample.map(|s| s * v);

    let sample = gather_rgb_avx(ctx.lut, cxxxa_idx);

    // (x0-x1) * cxxxa
    let v = x0 - x1;
    result = std::array::from_fn(|c| fmadd_ps_avx(v, sample[c], result[c]));

    let sample = gather_rgb_avx(ctx.lut, cxxxb_idx);

    // (x1-x2) * cxxxb
    let v = x1 - x2;
    result = std::array::from_fn(|c| fmadd_ps_avx(v, sample[c], result[c]));

    let sample = gather_rgb_avx(ctx.lut, c111_idx);

    // x2 * c111
    std::array::from_fn(|c| fmadd_ps_avx(x2, sample[c], result[c]))
}

/// Port of `applyTetrahedralAVX` and `applyTetrahedralAVXFunc<F32, F32>`
/// (src/OpenColorIO/ops/lut3d/Lut3DOpCPU_AVX.cpp:224-323 @ v2.5.2). `lut3d` holds 4 floats per
/// entry; alpha passes through unchanged.
pub(crate) fn apply_tetrahedral_avx(
    lut3d: &[f32],
    dim: i32,
    src: &[f32],
    dst: &mut [f32],
    total_pixel_count: i32,
) {
    let lutmax = dim as f32 - 1.0;
    let scale = lutmax;
    let zero = 0.0f32;

    let ctx = Lut3DContextAvx {
        lut: lut3d,
        lutmax,
        lutsize: dim as f32 * 4.0,
        lutsize2: dim as f32 * dim as f32 * 4.0,
    };

    let pixels = total_pixel_count as usize;
    for (inp, out) in src
        .as_chunks::<4>()
        .0
        .iter()
        .zip(dst.as_chunks_mut::<4>().0.iter_mut())
        .take(pixels)
    {
        // scale and clamp values
        let [r, g, b] =
            [inp[0], inp[1], inp[2]].map(|v| sse_min(sse_max(v * scale, zero), ctx.lutmax));

        let c = interp_tetrahedral_avx(&ctx, r, g, b);

        out[0] = c[0];
        out[1] = c[1];
        out[2] = c[2];
        out[3] = inp[3];
    }
}
