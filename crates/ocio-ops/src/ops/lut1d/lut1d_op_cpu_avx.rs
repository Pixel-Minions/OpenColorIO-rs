// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The AVX kernel of the 1D LUT, `linear1D<BIT_DEPTH_F32, outBD>`
//! (src/OpenColorIO/ops/lut1d/Lut1DOpCPU_AVX.cpp:15-165 @ v2.5.2), as exact scalar code: 8
//! pixels per block instead of SSE2's 4, `_mm256_floor_ps` instead of a truncation (the same
//! for the clamped, non-negative values), a multiply and an add (no FMA), and F16C for half
//! output. The lanes never mix (`lut1d_op_cpu_sse2` explains why), so each value is one call of
//! [`apply_lut_avx`], and each stored value one call of [`store`].

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
