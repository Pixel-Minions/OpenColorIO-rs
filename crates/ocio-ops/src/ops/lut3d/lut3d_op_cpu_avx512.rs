// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The AVX-512 tetrahedral kernel, `applyTetrahedralAVX512`
//! (src/OpenColorIO/ops/lut3d/Lut3DOpCPU_AVX512.cpp:16-255 @ v2.5.2), as exact scalar code.
//!
//! The lanes never mix (the last block uses masked loads and stores instead of a padded
//! buffer), so each pixel is one call of [`interp_tetrahedral_avx512`]. Compares produce mask
//! bits, blends select by mask bit, and the three multiply-adds are FMA instructions
//! (`_mm512_fmadd_ps`, one rounding), ported with `f32::mul_add`.

use crate::avx2::mm256_fmadd_ps as mm512_fmadd_ps;
use crate::avx512::{mm512_cmp_gt_oq, mm512_kandn, mm512_mask_blend_ps};
use crate::sse2::{mm_cvttps_epi32, mm_max_ps, mm_min_ps};

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

    let mut next_r = mm_min_ps(lut_max, prev_r + one_f);
    let mut next_g = mm_min_ps(lut_max, prev_g + one_f);
    let mut next_b = mm_min_ps(lut_max, prev_b + one_f);

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
    let rg_min = mm_min_ps(d_r, d_g);
    let rg_max = mm_max_ps(d_r, d_g);

    let x2 = mm_min_ps(rg_min, d_b);
    let mid = mm_max_ps(rg_min, d_b);

    let x0 = mm_max_ps(rg_max, d_b);
    let x1 = mm_min_ps(rg_max, mid);

    // convert indices to int
    let c000_idx = mm_cvttps_epi32(c000);
    let cxxxa_idx = mm_cvttps_epi32(cxxxa);
    let cxxxb_idx = mm_cvttps_epi32(cxxxb);
    let c111_idx = mm_cvttps_epi32(c111);

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
/// (src/OpenColorIO/ops/lut3d/Lut3DOpCPU_AVX512.cpp:176-255 @ v2.5.2). `lut3d` holds 4 floats
/// per entry; alpha passes through unchanged.
pub(crate) fn apply_tetrahedral_avx512(
    lut3d: &[f32],
    dim: i32,
    src: &[f32],
    dst: &mut [f32],
    num_pixels: i32,
) {
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
    for (inp, out) in src
        .as_chunks::<4>()
        .0
        .iter()
        .zip(dst.as_chunks_mut::<4>().0.iter_mut())
        .take(pixels)
    {
        // scale and clamp values
        let [r, g, b] =
            [inp[0], inp[1], inp[2]].map(|v| mm_min_ps(mm_max_ps(v * scale, zero), ctx.lutmax));

        let c = interp_tetrahedral_avx512(&ctx, r, g, b);

        out[0] = c[0];
        out[1] = c[1];
        out[2] = c[2];
        out[3] = inp[3];
    }
}
