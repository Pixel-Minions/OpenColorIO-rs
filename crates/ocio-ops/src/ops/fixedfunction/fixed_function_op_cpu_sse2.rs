// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The FixedFunction renderers' real SSE2 kernels, for blocks of finite pixels
//! (`crate::sse2::blocks`, and `super::fixed_function_op_cpu` for the rest): the fast-math PQ
//! curves, `Renderer_PQ_TO_LIN_SSE<true>` and `Renderer_LIN_TO_PQ_SSE<true>`
//! (src/OpenColorIO/ops/fixedfunction/FixedFunctionOpCPU.cpp:2143-2172, 2198-2227 @ v2.5.2). Alpha
//! isn't written.
//!
//! # Why no operand order shows here
//!
//! No lane meets another, the constants are finite, and two NaNs never meet in a lane. To PQ,
//! `ssePower` of a finite value is finite here, so the ratio has no NaN. From PQ, `nom / denom` can
//! be the default NaN only (`0 / 0`, at the curve's pole), and `ssePower` then makes it 0.
//!
//! `unsafe` here only calls this file's `#[target_feature(enable = "sse2")]` functions: SSE2 is
//! part of x86-64, which is the only architecture that compiles this file.
#![allow(unsafe_code)]

use core::arch::x86_64::*;

use crate::sse::simd::sse_power;
use crate::sse2::blocks::{load, store_rgb};

/// `_mm_or_ps(_mm_and_ps(abs_rgb_mask, value), _mm_andnot_ps(abs_rgb_mask, v))` on the lanes
/// that get stored.
#[inline]
#[target_feature(enable = "sse2")]
fn with_sign_of(value: __m128, v: __m128) -> __m128 {
    let sign = _mm_set1_ps(-0.0);
    _mm_or_ps(_mm_andnot_ps(sign, value), _mm_and_ps(sign, v))
}

/// The curve's constants, as `float`.
#[derive(Debug, Clone, Copy)]
pub(super) struct Pq {
    pub(super) m1: f32,
    pub(super) m2: f32,
    pub(super) c1: f32,
    pub(super) c2: f32,
    pub(super) c3: f32,
}

/// The loop of `Renderer_LIN_TO_PQ_SSE<true>::apply` on `block`, in place.
pub(super) fn lin_to_pq(block: &mut [[f32; 4]], pq: &Pq) {
    #[target_feature(enable = "sse2")]
    fn kernel(block: &mut [[f32; 4]], pq: &Pq) {
        let (vm1, vm2) = (_mm_set1_ps(pq.m1), _mm_set1_ps(pq.m2));
        let (vc1, vc2, vc3) = (_mm_set1_ps(pq.c1), _mm_set1_ps(pq.c2), _mm_set1_ps(pq.c3));
        let scale = _mm_set1_ps(0.01);
        let one = _mm_set1_ps(1.0);
        for px in block {
            let v = load(px);

            // Clear sign bits of RGB.
            let vabs = _mm_andnot_ps(_mm_set1_ps(-0.0), v);
            // Input is in nits/100, convert to [0,1], where 1 is 10000 nits.
            let l = _mm_mul_ps(scale, vabs);
            let y = sse_power(l, vm1);
            let ratpoly = _mm_div_ps(
                _mm_add_ps(vc1, _mm_mul_ps(vc2, y)),
                _mm_add_ps(one, _mm_mul_ps(vc3, y)),
            );
            let n = sse_power(ratpoly, vm2);

            // Restore sign bits.
            store_rgb(px, with_sign_of(n, v));
        }
    }
    // SAFETY: x86-64 has SSE2.
    unsafe { kernel(block, pq) }
}

/// The loop of `Renderer_PQ_TO_LIN_SSE<true>::apply` on `block`, in place.
pub(super) fn pq_to_lin(block: &mut [[f32; 4]], pq: &Pq) {
    #[target_feature(enable = "sse2")]
    fn kernel(block: &mut [[f32; 4]], pq: &Pq) {
        let vm1_inv = _mm_set1_ps(1.0 / pq.m1);
        let vm2_inv = _mm_set1_ps(1.0 / pq.m2);
        let (vc1, vc2, vc3) = (_mm_set1_ps(pq.c1), _mm_set1_ps(pq.c2), _mm_set1_ps(pq.c3));
        let hundred = _mm_set1_ps(100.0);
        for px in block {
            let v = load(px);

            // Clear sign bits of RGB.
            let vabs = _mm_andnot_ps(_mm_set1_ps(-0.0), v);
            let x = sse_power(vabs, vm2_inv);
            let nom = _mm_max_ps(_mm_setzero_ps(), _mm_sub_ps(x, vc1));
            let denom = _mm_sub_ps(vc2, _mm_mul_ps(vc3, x));

            // Output scale is 1.0 = 10000 nits, we map it to make 1.0 = 100 nits.
            let nits100 = _mm_mul_ps(hundred, sse_power(_mm_div_ps(nom, denom), vm1_inv));

            // Restore the sign bits.
            store_rgb(px, with_sign_of(nits100, v));
        }
    }
    // SAFETY: x86-64 has SSE2.
    unsafe { kernel(block, pq) }
}
