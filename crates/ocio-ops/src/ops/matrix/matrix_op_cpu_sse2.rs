// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Matrix renderers' real SSE2 kernels, for blocks of finite pixels and finite coefficients
//! (`crate::sse2::blocks`, and `super::matrix_op_cpu` for the rest).
//!
//! The matrix kernels follow the SSE2 branches of `MatrixWithOffsetRenderer::apply` and
//! `MatrixRenderer::apply` (src/OpenColorIO/ops/matrix/MatrixOpCPU.cpp:209-288, 323-396 @
//! v2.5.2) operation for operation. The scale kernels compute what the plain C++ of
//! `ScaleRenderer::apply` and `ScaleWithOffsetRenderer::apply` (MatrixOpCPU.cpp:90-105, 125-140)
//! computes for four channels, with one multiply and one add per pixel.
//!
//! `unsafe` here only calls this file's `#[target_feature(enable = "sse2")]` functions: SSE2 is
//! part of x86-64, which is the only architecture that compiles this file.
#![allow(unsafe_code)]

use core::arch::x86_64::*;

use crate::sse2::blocks::{load, store};

/// `(m0 * r + m1 * g) + (m2 * b + m3 * a)`.
#[inline]
#[target_feature(enable = "sse2")]
fn times(pixel: __m128, m: &[__m128; 4]) -> __m128 {
    let r = _mm_shuffle_ps::<0x00>(pixel, pixel);
    let g = _mm_shuffle_ps::<0x55>(pixel, pixel);
    let b = _mm_shuffle_ps::<0xaa>(pixel, pixel);
    let a = _mm_shuffle_ps::<0xff>(pixel, pixel);

    let rm0 = _mm_mul_ps(m[0], r);
    let gm1 = _mm_mul_ps(m[1], g);
    let bm2 = _mm_mul_ps(m[2], b);
    let am3 = _mm_mul_ps(m[3], a);

    _mm_add_ps(_mm_add_ps(rm0, gm1), _mm_add_ps(bm2, am3))
}

/// `in * m_scale`.
#[derive(Debug, Clone, Copy)]
pub(super) struct Scale<'a>(pub(super) &'a [f32; 4]);

/// `in * m_scale + m_offset`.
#[derive(Debug, Clone, Copy)]
pub(super) struct ScaleWithOffset<'a>(pub(super) &'a [f32; 4], pub(super) &'a [f32; 4]);

/// The pixel times the matrix of the four columns.
#[derive(Debug, Clone, Copy)]
pub(super) struct Matrix<'a>(pub(super) &'a [[f32; 4]; 4]);

/// The pixel times the matrix of the four columns, plus the offsets.
#[derive(Debug, Clone, Copy)]
pub(super) struct MatrixWithOffset<'a>(pub(super) &'a [[f32; 4]; 4], pub(super) &'a [f32; 4]);

/// Generates a kernel's two loops, in place and between two buffers.
macro_rules! kernel {
    ($name:ident, $self:ident, ($($setup:tt)*), $pixel:ident => $body:expr) => {
        impl $name<'_> {
            /// The kernel on `block`, in place.
            pub(super) fn render(&self, block: &mut [[f32; 4]]) {
                #[target_feature(enable = "sse2")]
                fn run($self: &$name<'_>, block: &mut [[f32; 4]]) {
                    $($setup)*
                    for pixel in block {
                        let $pixel = load(pixel);
                        store(pixel, $body);
                    }
                }
                // SAFETY: x86-64 has SSE2.
                unsafe { run(self, block) }
            }

            /// The kernel from `input` to `output`.
            pub(super) fn render_to(&self, input: &[[f32; 4]], output: &mut [[f32; 4]]) {
                #[target_feature(enable = "sse2")]
                fn run($self: &$name<'_>, input: &[[f32; 4]], output: &mut [[f32; 4]]) {
                    $($setup)*
                    for (from, to) in input.iter().zip(output) {
                        let $pixel = load(from);
                        store(to, $body);
                    }
                }
                // SAFETY: x86-64 has SSE2.
                unsafe { run(self, input, output) }
            }
        }
    };
}

kernel!(Scale, this, (let scale = load(this.0);), pixel => _mm_mul_ps(pixel, scale));

kernel!(
    ScaleWithOffset,
    this,
    (let scale = load(this.0); let offset = load(this.1);),
    pixel => _mm_add_ps(_mm_mul_ps(pixel, scale), offset)
);

kernel!(
    Matrix,
    this,
    (let m = this.0.each_ref().map(load);),
    pixel => times(pixel, &m)
);

kernel!(
    MatrixWithOffset,
    this,
    (let m = this.0.each_ref().map(load); let offset = load(this.1);),
    pixel => _mm_add_ps(times(pixel, &m), offset)
);
