// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The AVX2 kernel of the 1D LUT, `linear1D<BIT_DEPTH_F32, outBD>`
//! (src/OpenColorIO/ops/lut1d/Lut1DOpCPU_AVX2.cpp:15-143 @ v2.5.2), as exact scalar code: as
//! the AVX kernel, with hardware gathers (`_mm256_i32gather_ps`) and the interpolation as one
//! fused multiply-add (`_mm256_fmadd_ps`, one rounding: `f32::mul_add`). The lanes never mix
//! (`lut1d_op_cpu_sse2` explains why), so each value is one call of [`apply_lut_avx2`], and each
//! stored value one call of [`store`].
//!
//! The multiply-add's operands are never NaN (`lut1d_op_cpu_sse2`); `Inf * 0` on a node gives
//! the default NaN, which `f32::mul_add` returns as the instruction does
//! (`crate::avx2`'s test).

use crate::avx2::{avx2_rgba_pack_store, mm256_fmadd_ps};
use crate::math_utils::{sse_cvttps_epi32, sse_max, sse_min};
use crate::sse2::PackDepth;

/// One lane of `apply_lut_avx2` (Lut1DOpCPU_AVX2.cpp:18-41 @ v2.5.2): `v` through the table
/// `lut`, with `scale` the domain's scale and `lut_max` its last index.
pub(crate) fn apply_lut_avx2(lut: &[f32], v: f32, scale: f32, lut_max: f32) -> f32 {
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

    // `_mm256_i32gather_ps(lut, index, sizeof(float))`: signed 32-bit indices, non-negative
    // here.
    let p = lut[prev_i as usize];
    let n = lut[next_i as usize];

    // lerp: a + (b - a) * t;
    mm256_fmadd_ps(n - p, d, p)
}

/// One value of `AVX2RGBAPack<outBD>::Store` (src/OpenColorIO/AVX2.h:70-300 @ v2.5.2), as the
/// raw bits of the output channel type.
pub(crate) fn store(depth: PackDepth, value: f32) -> u32 {
    avx2_rgba_pack_store(depth, value)
}
