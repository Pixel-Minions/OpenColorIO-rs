// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The real SSE2 forms of the linear style's conversion, `LinLog` and `LogLin`
//! (src/OpenColorIO/ops/gradingrgbcurve/GradingRGBCurveOpCPU.cpp:179-235 @ v2.5.2), for blocks of
//! finite pixels (`crate::sse2::blocks`, and `super::grading_rgb_curve_op_cpu` for the rest).
//!
//! # Why no operand order shows here
//!
//! No lane meets another. In a lane, each branch meets constants only, so no two NaNs can meet,
//! and the select moves bits. A finite pixel gives no NaN in `LinLog`: a sum and a product of
//! finite values, and `sseLog2`, which gives a number for any input. In `LogLin` a NaN can
//! arrive from the curves; it is the default NaN, and it takes the linear branch.
//!
//! `unsafe` here only calls this file's `#[target_feature(enable = "sse2")]` functions: SSE2 is
//! part of x86-64, which is the only architecture that compiles this file.
#![allow(unsafe_code)]

use core::arch::x86_64::*;

use super::grading_rgb_curve_op_cpu::log_lin as k;
use crate::sse::simd::{sse_log2, sse_power};
use crate::sse2::blocks::{load, store};

/// Port of `LinLog` (GradingRGBCurveOpCPU.cpp:179-207 @ v2.5.2), on all four lanes.
#[inline]
#[target_feature(enable = "sse2")]
fn lin_log(pix: __m128) -> __m128 {
    let flag = _mm_cmpgt_ps(pix, _mm_set1_ps(k::XBRK));

    let pix_lin = _mm_mul_ps(pix, _mm_set1_ps(k::GAIN));
    let pix_lin = _mm_add_ps(pix_lin, _mm_set1_ps(k::OFFS));

    let pix = _mm_add_ps(pix, _mm_set1_ps(k::SHIFT));
    let pix = _mm_mul_ps(pix, _mm_set1_ps(k::M));
    let pix = sse_log2(pix);

    _mm_or_ps(_mm_and_ps(flag, pix), _mm_andnot_ps(flag, pix_lin))
}

/// Port of `LogLin` (GradingRGBCurveOpCPU.cpp:209-235 @ v2.5.2), on all four lanes.
#[inline]
#[target_feature(enable = "sse2")]
fn log_lin(pix: __m128) -> __m128 {
    let flag = _mm_cmpgt_ps(pix, _mm_set1_ps(k::YBRK));

    let pix_lin = _mm_sub_ps(pix, _mm_set1_ps(k::OFFS));
    let pix_lin = _mm_mul_ps(pix_lin, _mm_set1_ps(k::GAIN_INV));

    let pix = sse_power(_mm_set1_ps(k::POWER), pix);
    let pix = _mm_mul_ps(pix, _mm_set1_ps(k::SHIFT_018));
    let pix = _mm_sub_ps(pix, _mm_set1_ps(k::SHIFT));

    _mm_or_ps(_mm_and_ps(flag, pix), _mm_andnot_ps(flag, pix_lin))
}

/// [`lin_log`] on a pixel, in place.
#[inline]
pub(super) fn lin_log_pixel(px: &mut [f32; 4]) {
    // SAFETY: x86-64 has SSE2.
    store(px, unsafe { lin_log(load(px)) });
}

/// [`log_lin`] on a pixel, in place.
#[inline]
pub(super) fn log_lin_pixel(px: &mut [f32; 4]) {
    // SAFETY: x86-64 has SSE2.
    store(px, unsafe { log_lin(load(px)) });
}
