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

/// `in * m_scale`, on `block` in place.
pub(super) fn scale(block: &mut [[f32; 4]], scale: &[f32; 4]) {
    #[target_feature(enable = "sse2")]
    fn kernel(block: &mut [[f32; 4]], scale: &[f32; 4]) {
        let scale = load(scale);
        for pixel in block {
            store(pixel, _mm_mul_ps(load(pixel), scale));
        }
    }
    // SAFETY: x86-64 has SSE2.
    unsafe { kernel(block, scale) }
}

/// `in * m_scale + m_offset`, on `block` in place.
pub(super) fn scale_with_offset(block: &mut [[f32; 4]], scale: &[f32; 4], offset: &[f32; 4]) {
    #[target_feature(enable = "sse2")]
    fn kernel(block: &mut [[f32; 4]], scale: &[f32; 4], offset: &[f32; 4]) {
        let scale = load(scale);
        let offset = load(offset);
        for pixel in block {
            store(pixel, _mm_add_ps(_mm_mul_ps(load(pixel), scale), offset));
        }
    }
    // SAFETY: x86-64 has SSE2.
    unsafe { kernel(block, scale, offset) }
}

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

/// The pixels of `block`, in place, times the matrix of the four `columns`.
pub(super) fn matrix(block: &mut [[f32; 4]], columns: &[[f32; 4]; 4]) {
    #[target_feature(enable = "sse2")]
    fn kernel(block: &mut [[f32; 4]], columns: &[[f32; 4]; 4]) {
        let m = columns.each_ref().map(load);
        for pixel in block {
            store(pixel, times(load(pixel), &m));
        }
    }
    // SAFETY: x86-64 has SSE2.
    unsafe { kernel(block, columns) }
}

/// The pixels of `block`, in place, times the matrix of the four `columns`, plus `offset`.
pub(super) fn matrix_with_offset(
    block: &mut [[f32; 4]],
    columns: &[[f32; 4]; 4],
    offset: &[f32; 4],
) {
    #[target_feature(enable = "sse2")]
    fn kernel(block: &mut [[f32; 4]], columns: &[[f32; 4]; 4], offset: &[f32; 4]) {
        let m = columns.each_ref().map(load);
        let offset = load(offset);
        for pixel in block {
            store(pixel, _mm_add_ps(times(load(pixel), &m), offset));
        }
    }
    // SAFETY: x86-64 has SSE2.
    unsafe { kernel(block, columns, offset) }
}
