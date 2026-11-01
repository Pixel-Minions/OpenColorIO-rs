// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Log renderers' real SSE2 kernels, for blocks of finite pixels and finite coefficients
//! (`crate::sse2::blocks`, and `super::log_op_cpu` for the rest).
//!
//! They follow the `...RendererSSE::apply` loops of src/OpenColorIO/ops/log/LogOpCPU.cpp @
//! v2.5.2, operation for operation. Those load 0 in the alpha lane and restore alpha after the
//! store; here the alpha lane holds the pixel's alpha, its result is discarded, and alpha isn't
//! written.
//!
//! # Why no operand order shows here
//!
//! With finite pixels and coefficients, a NaN starts as x86's default NaN: an overflow makes an
//! infinity, and an infinity meets another or a coefficient of 0. These kernels have no `xor`,
//! `abs` or other change of sign: the camera styles' `flag` selects one of two values whole,
//! `_mm_max_ps` makes a NaN `minValue` before `sseLog2`, whose bit manipulation would otherwise
//! read a NaN's sign and payload, and `sseExp2` leaves a NaN the default NaN (its only NaN
//! operand is `fraction`, and its masks test `x`, for which every comparison is false). So
//! each sum and product that meets two NaNs meets the same bits.
//!
//! `unsafe` here only calls this file's `#[target_feature(enable = "sse2")]` functions: SSE2 is
//! part of x86-64, which is the only architecture that compiles this file.
#![allow(unsafe_code)]

use core::arch::x86_64::*;

use crate::sse::simd::{sse_exp2, sse_log2};
use crate::sse2::blocks::{Pair, load, store_rgb};

/// `_mm_set_ps(0.0f, a[2], a[1], a[0])`.
#[inline]
#[target_feature(enable = "sse2")]
fn rgb(a: &[f32; 3]) -> __m128 {
    _mm_setr_ps(a[0], a[1], a[2], 0.0)
}

/// `(pixel & flag) | (lin & !flag)`.
#[inline]
#[target_feature(enable = "sse2")]
fn select(flag: __m128, pixel: __m128, lin: __m128) -> __m128 {
    _mm_or_ps(_mm_and_ps(flag, pixel), _mm_andnot_ps(flag, lin))
}

/// Generates a kernel's two loops, on a block in place and from one buffer to another: its body
/// reads and writes `$px` with `load` and `store_rgb`, which the second loop defines for a
/// [`Pair`].
macro_rules! kernel {
    (
        $(#[$doc:meta])*
        $name:ident, $name_to:ident ($($arg:ident: $ty:ty),*) { $($setup:tt)* } $px:ident => $body:tt
    ) => {
        $(#[$doc])*
        pub(super) fn $name(block: &mut [[f32; 4]], $($arg: $ty),*) {
            #[target_feature(enable = "sse2")]
            fn run(block: &mut [[f32; 4]], $($arg: $ty),*) {
                $($setup)*
                for $px in block $body
            }
            // SAFETY: x86-64 has SSE2.
            unsafe { run(block, $($arg),*) }
        }

        $(#[$doc])*
        pub(super) fn $name_to(input: &[[f32; 4]], output: &mut [[f32; 4]], $($arg: $ty),*) {
            #[target_feature(enable = "sse2")]
            fn run(input: &[[f32; 4]], output: &mut [[f32; 4]], $($arg: $ty),*) {
                #[inline]
                fn load(px: &Pair<'_>) -> __m128 {
                    px.load()
                }
                #[inline]
                fn store_rgb(px: &mut Pair<'_>, value: __m128) {
                    px.store_rgb(value);
                }
                $($setup)*
                for (from, to) in input.iter().zip(output) {
                    let mut pair = Pair { from, to };
                    let $px = &mut pair;
                    $body
                }
            }
            // SAFETY: x86-64 has SSE2.
            unsafe { run(input, output, $($arg),*) }
        }
    };
}

kernel!(
    /// The loop of `LogRendererSSE::apply` (LogOpCPU.cpp:421-451 @ v2.5.2).
    log, log_to(min_value: f32, log_scale: f32) {
        let min_value = _mm_set1_ps(min_value);
        let log_scale = _mm_set1_ps(log_scale);
    } px => {
            let pixel = _mm_max_ps(load(px), min_value);
            store_rgb(px, _mm_mul_ps(sse_log2(pixel), log_scale));
    }
);

kernel!(
    /// The loop of `AntiLogRendererSSE::apply` (LogOpCPU.cpp:490-520 @ v2.5.2).
    anti_log, anti_log_to(log2_base: f32) {
        let log2_base = _mm_set1_ps(log2_base);
    } px => {
            store_rgb(px, sse_exp2(_mm_mul_ps(load(px), log2_base)));
    }
);

kernel!(
    /// The loop of `Log2LinRendererSSE::apply` (LogOpCPU.cpp:581-620 @ v2.5.2).
    log2lin, log2lin_to(kinv: &[f32; 3], minuskb: &[f32; 3], minusb: &[f32; 3], minv: &[f32; 3]) {
        let (kinv, minuskb, minusb, minv) = (rgb(kinv), rgb(minuskb), rgb(minusb), rgb(minv));
    } px => {
            let pixel = _mm_add_ps(load(px), minuskb);
            let pixel = _mm_mul_ps(pixel, kinv);
            let pixel = sse_exp2(pixel);
            let pixel = _mm_add_ps(pixel, minusb);
            store_rgb(px, _mm_mul_ps(pixel, minv));
    }
);

kernel!(
    /// The loop of `Lin2LogRendererSSE::apply` (LogOpCPU.cpp:682-720 @ v2.5.2).
    lin2log, lin2log_to(min_value: f32, m: &[f32; 3], b: &[f32; 3], klog: &[f32; 3], kb: &[f32; 3]) {
        let min_value = _mm_set1_ps(min_value);
        let (m, b, klog, kb) = (rgb(m), rgb(b), rgb(klog), rgb(kb));
    } px => {
            let pixel = _mm_mul_ps(load(px), m);
            let pixel = _mm_add_ps(pixel, b);
            let pixel = _mm_max_ps(pixel, min_value);
            let pixel = sse_log2(pixel);
            let pixel = _mm_mul_ps(pixel, klog);
            store_rgb(px, _mm_add_ps(pixel, kb));
    }
);

/// `CameraLog2LinRendererSSE`'s coefficients.
#[derive(Debug, Clone, Copy)]
pub(super) struct CameraLog2Lin<'a> {
    pub(super) kinv: &'a [f32; 3],
    pub(super) minuskb: &'a [f32; 3],
    pub(super) minusb: &'a [f32; 3],
    pub(super) minv: &'a [f32; 3],
    pub(super) break_pnt: &'a [f32; 3],
    pub(super) linoinv: &'a [f32; 3],
    pub(super) linsinv: &'a [f32; 3],
}

kernel!(
    /// The loop of `CameraLog2LinRendererSSE::apply` (LogOpCPU.cpp:810-859 @ v2.5.2).
    camera_log2lin, camera_log2lin_to(c: &CameraLog2Lin<'_>) {
        let (kinv, minuskb, minusb, minv) =
            (rgb(c.kinv), rgb(c.minuskb), rgb(c.minusb), rgb(c.minv));
        let (break_pnt, linoinv, linsinv) = (rgb(c.break_pnt), rgb(c.linoinv), rgb(c.linsinv));
    } px => {
            let pixel = load(px);
            let flag = _mm_cmpgt_ps(pixel, break_pnt);

            let pixel_lin = _mm_mul_ps(_mm_add_ps(pixel, linoinv), linsinv);

            let pixel = _mm_add_ps(pixel, minuskb);
            let pixel = _mm_mul_ps(pixel, kinv);
            let pixel = sse_exp2(pixel);
            let pixel = _mm_add_ps(pixel, minusb);
            let pixel = _mm_mul_ps(pixel, minv);

            store_rgb(px, select(flag, pixel, pixel_lin));
    }
);

/// `CameraLin2LogRendererSSE`'s coefficients.
#[derive(Debug, Clone, Copy)]
pub(super) struct CameraLin2Log<'a> {
    pub(super) m: &'a [f32; 3],
    pub(super) b: &'a [f32; 3],
    pub(super) klog: &'a [f32; 3],
    pub(super) kb: &'a [f32; 3],
    pub(super) lins: &'a [f32; 3],
    pub(super) lino: &'a [f32; 3],
    pub(super) break_pnt: &'a [f32; 3],
}

kernel!(
    /// The loop of `CameraLin2LogRendererSSE::apply` (LogOpCPU.cpp:928-981 @ v2.5.2).
    camera_lin2log, camera_lin2log_to(min_value: f32, c: &CameraLin2Log<'_>) {
        let min_value = _mm_set1_ps(min_value);
        let (m, b, klog, kb) = (rgb(c.m), rgb(c.b), rgb(c.klog), rgb(c.kb));
        let (lins, lino, break_pnt) = (rgb(c.lins), rgb(c.lino), rgb(c.break_pnt));
    } px => {
            let pixel = load(px);
            let flag = _mm_cmpgt_ps(pixel, break_pnt);

            let pixel_lin = _mm_add_ps(_mm_mul_ps(pixel, lins), lino);

            let pixel = _mm_mul_ps(pixel, m);
            let pixel = _mm_add_ps(pixel, b);
            let pixel = _mm_max_ps(pixel, min_value);
            let pixel = sse_log2(pixel);
            let pixel = _mm_mul_ps(pixel, klog);
            let pixel = _mm_add_ps(pixel, kb);

            store_rgb(px, select(flag, pixel, pixel_lin));
    }
);
