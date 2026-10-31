// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The ExposureContrast renderers' real SSE2 kernels, for blocks of finite pixels and finite
//! coefficients (`crate::sse2::blocks`, and `super::exposure_contrast_op_cpu` for the rest).
//!
//! They compute what the `OCIO_USE_SSE2` branches of the renderers' `apply` compute
//! (src/OpenColorIO/ops/exposurecontrast/ExposureContrastOpCPU.cpp @ v2.5.2). The C++ scales by
//! the exposure in a scalar loop, where the contrast is 1; a register does it here. Alpha isn't
//! written.
//!
//! # Why no operand order shows here
//!
//! No lane meets another, and each sum and product has a finite coefficient as its other
//! operand, so no two NaNs can meet. `ssePower` makes a NaN 0 and gives no NaN of its own for a
//! finite exponent.
//!
//! `unsafe` here only calls this file's `#[target_feature(enable = "sse2")]` functions: SSE2 is
//! part of x86-64, which is the only architecture that compiles this file.
#![allow(unsafe_code)]

use core::arch::x86_64::*;

use crate::sse::simd::sse_power;
use crate::sse2::blocks::{Pair, load, store_rgb};

/// `in * factor` on `block`, in place.
pub(super) fn scale(block: &mut [[f32; 4]], factor: f32) {
    #[target_feature(enable = "sse2")]
    fn kernel(block: &mut [[f32; 4]], factor: f32) {
        let factor = _mm_set1_ps(factor);
        for px in block {
            store_rgb(px, _mm_mul_ps(load(px), factor));
        }
    }
    // SAFETY: x86-64 has SSE2.
    unsafe { kernel(block, factor) }
}

/// `_mm_mul_ps(ssePower(_mm_mul_ps(data, scale), power), factor)` on `block`, in place.
pub(super) fn power(block: &mut [[f32; 4]], scale: f32, power: f32, factor: f32) {
    #[target_feature(enable = "sse2")]
    fn kernel(block: &mut [[f32; 4]], scale: f32, power: f32, factor: f32) {
        let (scale, power, factor) = (_mm_set1_ps(scale), _mm_set1_ps(power), _mm_set1_ps(factor));
        for px in block {
            let data = sse_power(_mm_mul_ps(load(px), scale), power);
            store_rgb(px, _mm_mul_ps(data, factor));
        }
    }
    // SAFETY: x86-64 has SSE2.
    unsafe { kernel(block, scale, power, factor) }
}

/// `in * contrast + offset` on `block`, in place.
pub(super) fn linear(block: &mut [[f32; 4]], contrast: f32, offset: f32) {
    #[target_feature(enable = "sse2")]
    fn kernel(block: &mut [[f32; 4]], contrast: f32, offset: f32) {
        let (contrast, offset) = (_mm_set1_ps(contrast), _mm_set1_ps(offset));
        for px in block {
            store_rgb(px, _mm_add_ps(_mm_mul_ps(load(px), contrast), offset));
        }
    }
    // SAFETY: x86-64 has SSE2.
    unsafe { kernel(block, contrast, offset) }
}

/// [`linear`] from `input` to `output`.
pub(super) fn linear_to(input: &[[f32; 4]], output: &mut [[f32; 4]], contrast: f32, offset: f32) {
    #[target_feature(enable = "sse2")]
    fn kernel(input: &[[f32; 4]], output: &mut [[f32; 4]], contrast: f32, offset: f32) {
        let (contrast, offset) = (_mm_set1_ps(contrast), _mm_set1_ps(offset));
        for (from, to) in input.iter().zip(output) {
            let mut pair = Pair { from, to };
            let value = _mm_add_ps(_mm_mul_ps(pair.load(), contrast), offset);
            pair.store_rgb(value);
        }
    }
    // SAFETY: x86-64 has SSE2.
    unsafe { kernel(input, output, contrast, offset) }
}
