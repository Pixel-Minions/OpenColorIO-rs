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
