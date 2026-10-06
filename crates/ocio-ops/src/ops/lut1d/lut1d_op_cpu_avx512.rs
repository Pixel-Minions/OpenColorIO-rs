// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The AVX-512 kernel of the 1D LUT, `linear1D<BIT_DEPTH_F32, outBD>`
//! (src/OpenColorIO/ops/lut1d/Lut1DOpCPU_AVX512.cpp:15-116 @ v2.5.2), as exact scalar code: as
//! the AVX2 kernel, 16 pixels per block, the last block loaded and stored masked
//! (`LoadMasked`, `StoreMasked`) rather than through zero-padded buffers. The masked-off lanes'
//! results are never stored, so each value is one call of [`apply_lut_avx512`] and each stored
//! value one call of [`store`].

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
