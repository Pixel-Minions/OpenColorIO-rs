// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Matrix renderers' real SSE2 kernels (`super::matrix_op_cpu`).
//!
//! The matrix kernels are the loops of the SSE2 branches of `MatrixWithOffsetRenderer::apply` and
//! `MatrixRenderer::apply` (src/OpenColorIO/ops/matrix/MatrixOpCPU.cpp:209-288, 323-396 @
//! v2.5.2), the scale kernels the loops the wheels compiled `ScaleRenderer::apply` and
//! `ScaleWithOffsetRenderer::apply` (MatrixOpCPU.cpp:90-105, 125-140) into. Each operation is the
//! instruction (`crate::sse2::packed`), with the operand order of the wheel's machine code, so
//! any NaN comes out as in the wheel (`super::matrix_op_cpu`, "The wheels' operand orders").
//!
//! `unsafe` here only calls this file's `#[target_feature(enable = "sse2")]` functions: SSE2 is
//! part of x86-64, which is the only architecture that compiles this file.
#![allow(unsafe_code)]

use core::arch::x86_64::*;

use crate::sse2::blocks::{align_loop, load, store};
use crate::sse2::packed::{addps, mulps};

/// `(m0 * r + m1 * g) + (m2 * b + m3 * a)`, in the orders of the wheel's loop (module docs).
///
/// `REST`: the orders of MSVC's loop that finishes a row. GCC has one loop.
#[inline]
#[target_feature(enable = "sse2")]
fn times<const REST: bool>(pixel: __m128, m: &[__m128; 4]) -> __m128 {
    let r = _mm_shuffle_ps::<0x00>(pixel, pixel);
    let g = _mm_shuffle_ps::<0x55>(pixel, pixel);
    let b = _mm_shuffle_ps::<0xaa>(pixel, pixel);
    let a = _mm_shuffle_ps::<0xff>(pixel, pixel);

    let rm0 = mulps(r, m[0]);
    let gm1 = mulps(g, m[1]);
    let bm2 = mulps(b, m[2]);
    let am3 = mulps(a, m[3]);

    if REST {
        addps(addps(gm1, rm0), addps(am3, bm2))
    } else if cfg!(target_os = "windows") {
        addps(addps(am3, bm2), addps(gm1, rm0))
    } else {
        addps(addps(bm2, am3), addps(rm0, gm1))
    }
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

/// Generates a kernel's two loops, in place and between two buffers, `REST` as in [`times`].
macro_rules! kernel {
    ($name:ident, $self:ident, $rest:ident, ($($setup:tt)*), $pixel:ident => $body:expr) => {
        impl $name<'_> {
            /// The kernel on `block`, in place, with the orders of the loop of whole groups of
            /// four pixels.
            pub(super) fn render(&self, block: &mut [[f32; 4]]) {
                // SAFETY: x86-64 has SSE2.
                unsafe { Self::run::<false>(self, block) }
            }

            /// The kernel from `input` to `output`, as in [`Self::render`].
            pub(super) fn render_to(&self, input: &[[f32; 4]], output: &mut [[f32; 4]]) {
                // SAFETY: x86-64 has SSE2.
                unsafe { Self::run_to::<false>(self, input, output) }
            }

            #[target_feature(enable = "sse2")]
            fn run_to<const $rest: bool>(
                $self: &$name<'_>,
                input: &[[f32; 4]],
                output: &mut [[f32; 4]],
            ) {
                $($setup)*
                align_loop();
                for (from, to) in input.iter().zip(output) {
                    let $pixel = load(from);
                    store(to, $body);
                }
            }

            #[target_feature(enable = "sse2")]
            fn run<const $rest: bool>($self: &$name<'_>, block: &mut [[f32; 4]]) {
                $($setup)*
                align_loop();
                for pixel in block {
                    let $pixel = load(pixel);
                    store(pixel, $body);
                }
            }
        }
    };
}

kernel!(Scale, this, REST, (let scale = load(this.0);), pixel => mulps(pixel, scale));

kernel!(
    ScaleWithOffset,
    this,
    REST,
    (let scale = load(this.0); let offset = load(this.1);),
    pixel => addps(mulps(pixel, scale), offset)
);

kernel!(
    Matrix,
    this,
    REST,
    (let m = this.0.each_ref().map(load);),
    pixel => times::<REST>(pixel, &m)
);

kernel!(
    MatrixWithOffset,
    this,
    REST,
    (let m = this.0.each_ref().map(load); let offset = load(this.1);),
    pixel => addps(times::<REST>(pixel, &m), offset)
);

impl Scale<'_> {
    /// The pixels of the loop that finishes the row: `lanes`, for MSVC's order on blue.
    pub(super) fn render_rest(&self, pixels: &mut [[f32; 4]], lanes: impl FnOnce(&mut [[f32; 4]])) {
        lanes(pixels);
    }
}

impl ScaleWithOffset<'_> {
    /// The pixels of the loop that finishes the row: `lanes`, for MSVC's order on blue.
    pub(super) fn render_rest(&self, pixels: &mut [[f32; 4]], lanes: impl FnOnce(&mut [[f32; 4]])) {
        lanes(pixels);
    }
}

impl Matrix<'_> {
    /// The pixels of the loop that finishes the row.
    pub(super) fn render_rest(
        &self,
        pixels: &mut [[f32; 4]],
        _lanes: impl FnOnce(&mut [[f32; 4]]),
    ) {
        // SAFETY: x86-64 has SSE2.
        unsafe { Self::run::<true>(self, pixels) }
    }
}

impl MatrixWithOffset<'_> {
    /// The pixels of the loop that finishes the row.
    pub(super) fn render_rest(
        &self,
        pixels: &mut [[f32; 4]],
        _lanes: impl FnOnce(&mut [[f32; 4]]),
    ) {
        // SAFETY: x86-64 has SSE2.
        unsafe { Self::run::<true>(self, pixels) }
    }
}
