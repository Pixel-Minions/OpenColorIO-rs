// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The CDL renderers' real SSE2 kernels, for blocks of finite pixels and finite coefficients
//! (`crate::sse2::blocks`, and `super::cdl_op_cpu` for the rest).
//!
//! They follow `CDLRendererFwdSSE<CLAMP>::apply` and `CDLRendererRevSSE<CLAMP>::apply`
//! (src/OpenColorIO/ops/cdl/CDLOpCPU.cpp:346-376, 409-440 @ v2.5.2), the renderers that the
//! default optimization level picks. The renderers with `powf` stay as they are: they spend
//! their time in the math library, as the wheel does.
//!
//! # Why no operand order shows here
//!
//! With finite pixels and coefficients, a NaN starts as x86's default NaN: an overflow makes an
//! infinity, and the saturation subtracts or sums infinities, or multiplies one by alpha's weight
//! of 0. Nothing turns that NaN into another NaN before two meet: there is no `xor`, `abs` or
//! other change of sign in these kernels, `ssePower` gives 0 or a number, never a NaN, for an
//! input that isn't one and for finite exponents, and its mask makes a NaN 0. `sseSelect` only
//! moves bits. So every sum and product that meets two NaNs meets the same bits.
//!
//! `unsafe` here only calls this file's `#[target_feature(enable = "sse2")]` functions: SSE2 is
//! part of x86-64, which is the only architecture that compiles this file.
#![allow(unsafe_code)]

use core::arch::x86_64::*;

use crate::sse::simd::{sse_power, sse_select};
use crate::sse2::blocks::{load, store_rgb};

/// Port of `ApplyClamp<bool>(__m128 &)` (CDLOpCPU.cpp:120-133 @ v2.5.2).
#[inline]
#[target_feature(enable = "sse2")]
fn apply_clamp<const CLAMP: bool>(pix: __m128) -> __m128 {
    if CLAMP {
        _mm_min_ps(_mm_max_ps(pix, _mm_setzero_ps()), _mm_set1_ps(1.0))
    } else {
        pix
    }
}

/// Port of `ApplyPower<bool>(__m128 &, const __m128 &)` (CDLOpCPU.cpp:135-155 @ v2.5.2).
#[inline]
#[target_feature(enable = "sse2")]
fn apply_power<const CLAMP: bool>(pix: __m128, power: __m128) -> __m128 {
    if CLAMP {
        sse_power(apply_clamp::<true>(pix), power)
    } else {
        let neg_mask = _mm_cmplt_ps(pix, _mm_setzero_ps());
        sse_select(neg_mask, pix, sse_power(pix, power))
    }
}

/// Port of `ApplySaturation(__m128 &, const __m128)` (CDLOpCPU.cpp:157-171 @ v2.5.2).
#[inline]
#[target_feature(enable = "sse2")]
fn apply_saturation(pix: __m128, saturation: __m128) -> __m128 {
    let luma_weights = _mm_setr_ps(0.2126, 0.7152, 0.0722, 0.0);

    // Compute luma: dot product of pixel values and the luma weights
    let luma = _mm_mul_ps(pix, luma_weights);

    // luma = [ x+y , y+x , z+w , w+z ]
    let luma = _mm_add_ps(luma, _mm_shuffle_ps::<0b10_11_00_01>(luma, luma));

    // luma = [ x+y+z+w , y+x+w+z , z+w+x+y , w+z+y+x ]
    let luma = _mm_add_ps(luma, _mm_shuffle_ps::<0b01_00_11_10>(luma, luma));

    // Apply saturation
    _mm_add_ps(luma, _mm_mul_ps(saturation, _mm_sub_ps(pix, luma)))
}

/// `CDLRendererFwdSSE<CLAMP>::apply`'s loop on `block`, in place.
#[target_feature(enable = "sse2")]
fn fwd<const CLAMP: bool>(
    block: &mut [[f32; 4]],
    slope: &[f32; 4],
    offset: &[f32; 4],
    power: &[f32; 4],
    saturation: f32,
) {
    let (slope, offset, power) = (load(slope), load(offset), load(power));
    let saturation = _mm_set1_ps(saturation);
    for px in block {
        let pix = _mm_add_ps(_mm_mul_ps(load(px), slope), offset);
        let pix = apply_power::<CLAMP>(pix, power);
        let pix = apply_saturation(pix, saturation);
        store_rgb(px, apply_clamp::<CLAMP>(pix));
    }
}

/// `CDLRendererRevSSE<CLAMP>::apply`'s loop on `block`, in place.
#[target_feature(enable = "sse2")]
fn rev<const CLAMP: bool>(
    block: &mut [[f32; 4]],
    slope: &[f32; 4],
    offset: &[f32; 4],
    power: &[f32; 4],
    saturation: f32,
) {
    let (slope, offset, power) = (load(slope), load(offset), load(power));
    let saturation = _mm_set1_ps(saturation);
    for px in block {
        let pix = apply_clamp::<CLAMP>(load(px));
        let pix = apply_saturation(pix, saturation);
        let pix = apply_power::<CLAMP>(pix, power);
        let pix = _mm_mul_ps(_mm_add_ps(pix, offset), slope);
        store_rgb(px, apply_clamp::<CLAMP>(pix));
    }
}

/// [`fwd`] on `block`.
pub(super) fn render_fwd<const CLAMP: bool>(
    block: &mut [[f32; 4]],
    slope: &[f32; 4],
    offset: &[f32; 4],
    power: &[f32; 4],
    saturation: f32,
) {
    // SAFETY: x86-64 has SSE2.
    unsafe { fwd::<CLAMP>(block, slope, offset, power, saturation) }
}

/// [`rev`] on `block`.
pub(super) fn render_rev<const CLAMP: bool>(
    block: &mut [[f32; 4]],
    slope: &[f32; 4],
    offset: &[f32; 4],
    power: &[f32; 4],
    saturation: f32,
) {
    // SAFETY: x86-64 has SSE2.
    unsafe { rev::<CLAMP>(block, slope, offset, power, saturation) }
}
