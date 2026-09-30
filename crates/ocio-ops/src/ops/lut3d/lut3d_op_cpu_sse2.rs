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
pub(crate) fn apply_tetrahedral_sse2(
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
