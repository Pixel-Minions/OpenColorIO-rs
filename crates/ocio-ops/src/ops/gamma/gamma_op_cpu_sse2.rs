// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Gamma renderers' real SSE2 kernels, for blocks of finite pixels and finite coefficients
//! (`crate::sse2::blocks`, and `super::gamma_op_cpu` for the rest).
//!
//! They follow the `...OpCPUSSE::apply` loops of src/OpenColorIO/ops/gamma/GammaOpCPU.cpp @
//! v2.5.2, operation for operation. The renderers write every channel, alpha included.
//!
//! # Why no operand order shows here
//!
//! No lane meets another, and in a lane no two NaNs can meet. With finite pixels and
//! coefficients, each sum and product has a coefficient as its other operand, which is finite.
//! `ssePower` makes a NaN 0 and gives no NaN of its own for finite exponents. The mirror styles
//! OR a sign onto their result last, so what that does to a NaN meets nothing afterwards. So
//! the renderers' operand orders, which only matter for a NaN coefficient, can't show.
//!
//! `unsafe` here only calls this file's `#[target_feature(enable = "sse2")]` functions: SSE2 is
//! part of x86-64, which is the only architecture that compiles this file.
#![allow(unsafe_code)]

use core::arch::x86_64::*;

use super::gamma_op_utils::RendererParams;
use crate::sse::simd::sse_power;
use crate::sse2::blocks::{load, store};

/// A register of each moncurve coefficient, one lane per channel.
#[derive(Clone, Copy)]
struct Moncurve {
    gamma: __m128,
    offset: __m128,
    break_pnt: __m128,
    slope: __m128,
    scale: __m128,
}

impl Moncurve {
    #[inline]
    #[target_feature(enable = "sse2")]
    fn new(p: &[RendererParams; 4]) -> Self {
        Moncurve {
            gamma: load(&p.each_ref().map(|c| c.gamma)),
            offset: load(&p.each_ref().map(|c| c.offset)),
            break_pnt: load(&p.each_ref().map(|c| c.break_pnt)),
            slope: load(&p.each_ref().map(|c| c.slope)),
            scale: load(&p.each_ref().map(|c| c.scale)),
        }
    }
}

/// `sign_pix` and `abs_pix`.
#[inline]
#[target_feature(enable = "sse2")]
fn sign_and_abs(pixel: __m128) -> (__m128, __m128) {
    let sign = _mm_set1_ps(-0.0);
    (_mm_and_ps(pixel, sign), _mm_andnot_ps(sign, pixel))
}

/// `(a & flag) | (b & !flag)`.
#[inline]
#[target_feature(enable = "sse2")]
fn select(flag: __m128, a: __m128, b: __m128) -> __m128 {
    _mm_or_ps(_mm_and_ps(flag, a), _mm_andnot_ps(flag, b))
}

macro_rules! kernel {
    ($(#[$doc:meta])* $name:ident ( $block:ident; $($arg:ident : $ty:ty),* ) $inner:ident $body:block) => {
        #[target_feature(enable = "sse2")]
        fn $inner($block: &mut [[f32; 4]], $($arg: $ty),*) $body

        $(#[$doc])*
        pub(super) fn $name(block: &mut [[f32; 4]], $($arg: $ty),*) {
            // SAFETY: x86-64 has SSE2.
            unsafe { $inner(block, $($arg),*) }
        }
    };
}

kernel!(
    /// The loop of `GammaBasicOpCPUSSE::apply` (GammaOpCPU.cpp:321-339 @ v2.5.2).
    basic(block; gamma: &[f32; 4]) basic_loop {
        let gamma = load(gamma);
        for px in block {
            store(px, sse_power(load(px), gamma));
        }
    }
);

kernel!(
    /// The loop of `GammaBasicMirrorOpCPUSSE::apply` (GammaOpCPU.cpp:370-391 @ v2.5.2).
    basic_mirror(block; gamma: &[f32; 4]) basic_mirror_loop {
        let gamma = load(gamma);
        for px in block {
            let (sign_pix, abs_pix) = sign_and_abs(load(px));
            store(px, _mm_or_ps(sign_pix, sse_power(abs_pix, gamma)));
        }
    }
);

kernel!(
    /// The loop of `GammaBasicPassThruOpCPUSSE::apply` (GammaOpCPU.cpp:422-447 @ v2.5.2).
    basic_pass_thru(block; gamma: &[f32; 4]) basic_pass_thru_loop {
        let gamma = load(gamma);
        for px in block {
            let pixel = load(px);
            let data = sse_power(pixel, gamma);
            let flag = _mm_cmpgt_ps(pixel, _mm_setzero_ps());
            store(px, select(flag, data, pixel));
        }
    }
);

kernel!(
    /// The loop of `GammaMoncurveOpCPUFwdSSE::apply` (GammaOpCPU.cpp:484-522 @ v2.5.2).
    moncurve_fwd(block; params: &[RendererParams; 4]) moncurve_fwd_loop {
        let m = Moncurve::new(params);
        for px in block {
            let pixel = load(px);
            let data = _mm_add_ps(_mm_mul_ps(pixel, m.scale), m.offset);
            let data = sse_power(data, m.gamma);
            let flag = _mm_cmpgt_ps(pixel, m.break_pnt);
            store(px, select(flag, data, _mm_mul_ps(pixel, m.slope)));
        }
    }
);

kernel!(
    /// The loop of `GammaMoncurveOpCPURevSSE::apply` (GammaOpCPU.cpp:573-611 @ v2.5.2).
    moncurve_rev(block; params: &[RendererParams; 4]) moncurve_rev_loop {
        let m = Moncurve::new(params);
        for px in block {
            let pixel = load(px);
            let data = sse_power(pixel, m.gamma);
            let data = _mm_sub_ps(_mm_mul_ps(data, m.scale), m.offset);
            let flag = _mm_cmpgt_ps(pixel, m.break_pnt);
            store(px, select(flag, data, _mm_mul_ps(pixel, m.slope)));
        }
    }
);

kernel!(
    /// The loop of `GammaMoncurveMirrorOpCPUFwdSSE::apply` (GammaOpCPU.cpp:662-704 @ v2.5.2).
    moncurve_mirror_fwd(block; params: &[RendererParams; 4]) moncurve_mirror_fwd_loop {
        let m = Moncurve::new(params);
        for px in block {
            let (sign_pix, abs_pix) = sign_and_abs(load(px));
            let data = _mm_add_ps(_mm_mul_ps(abs_pix, m.scale), m.offset);
            let data = sse_power(data, m.gamma);
            let flag = _mm_cmpgt_ps(abs_pix, m.break_pnt);
            let data = select(flag, data, _mm_mul_ps(abs_pix, m.slope));
            store(px, _mm_or_ps(sign_pix, data));
        }
    }
);

kernel!(
    /// The loop of `GammaMoncurveMirrorOpCPURevSSE::apply` (GammaOpCPU.cpp:758-800 @ v2.5.2).
    moncurve_mirror_rev(block; params: &[RendererParams; 4]) moncurve_mirror_rev_loop {
        let m = Moncurve::new(params);
        for px in block {
            let (sign_pix, abs_pix) = sign_and_abs(load(px));
            let data = sse_power(abs_pix, m.gamma);
            let data = _mm_sub_ps(_mm_mul_ps(data, m.scale), m.offset);
            let flag = _mm_cmpgt_ps(abs_pix, m.break_pnt);
            let data = select(flag, data, _mm_mul_ps(abs_pix, m.slope));
            store(px, _mm_or_ps(sign_pix, data));
        }
    }
);
