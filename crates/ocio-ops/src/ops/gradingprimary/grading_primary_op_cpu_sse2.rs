// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The GradingPrimary renderers' real SSE2 kernels, for blocks of finite pixels and finite
//! coefficients ([`Coefficients::fast_path`], `crate::sse2::blocks`, and
//! `super::grading_primary_op_cpu` for the rest).
//!
//! They follow the `OCIO_USE_SSE2` branches of the renderers' `apply`
//! (src/OpenColorIO/ops/gradingprimary/GradingPrimaryOpCPU.cpp:259-1297 @ v2.5.2), whose four
//! loops per `apply` are the kernels' two `const` parameters. The log and video styles differ
//! in their coefficients only: the pivot of the contrast (the slope) is the black pivot for
//! video.
//!
//! `unsafe` here only calls this file's `#[target_feature(enable = "sse2")]` functions: SSE2 is
//! part of x86-64, which is the only architecture that compiles this file.
#![allow(unsafe_code)]

use core::arch::x86_64::*;

use crate::sse::simd::sse_power;
use crate::sse2::blocks::{load, store_rgb};

/// What an `apply` reads, for the kernels.
#[derive(Debug, Clone, Copy)]
pub(super) struct Coefficients {
    /// `brightness`, `offset`; the inverses in the reverse styles.
    pub(super) add: [f32; 4],
    /// `contrast` (log), `exposure` (linear), `slope` (video); the inverses in the reverse styles.
    pub(super) scale: [f32; 4],
    /// `gamma` (log, video), `contrast` (linear); the inverses in the reverse styles.
    pub(super) power: [f32; 4],
    /// `pivot` (log, linear), `blackPivot` (video).
    pub(super) pivot: f32,
    /// `saturation`, or its inverse in the reverse styles.
    pub(super) saturation: f32,
    /// `blackPivot` (log, video).
    pub(super) black_pivot: f32,
    /// `whitePivot` (log, video).
    pub(super) white_pivot: f32,
    /// `blackClamp`.
    pub(super) black_clamp: f32,
    /// `whiteClamp`.
    pub(super) white_clamp: f32,
    /// Whether the renderer runs `ApplyLinContrast` (the linear style).
    pub(super) lin_contrast: bool,
}

impl Coefficients {
    /// Whether finite pixels can take the kernels (`docs/cards/phase8.md`): every coefficient
    /// but the clamps, whose defaults are infinite and which only compare, is finite, and
    /// the pivot of `ApplyLinContrast`, if the style runs it, isn't 0.
    ///
    /// The only NaNs the kernels can then make are x86's default NaN, from infinities that an
    /// overflow makes, so no operand order can show. The steps that keep a sign (`ApplyGamma`,
    /// `ApplyLinContrast`) put it on `ssePower`'s result, which isn't NaN here: it gets a
    /// NaN's sign, not a NaN. In `ApplyLinContrast` the multiplication comes before the sign:
    /// `Inf * 0` with a pivot of 0 would make a NaN of the other sign, hence the condition.
    pub(super) fn fast_path(&self) -> bool {
        let finite = [self.add, self.scale, self.power]
            .iter()
            .flatten()
            .chain(&[
                self.pivot,
                self.saturation,
                self.black_pivot,
                self.white_pivot,
            ])
            .fold(true, |finite, v| finite & v.is_finite());
        finite && (!self.lin_contrast || self.pivot != 0.0)
    }
}

/// The kernels' registers.
#[derive(Clone, Copy)]
struct Registers {
    add: __m128,
    scale: __m128,
    power: __m128,
    pivot: __m128,
    saturation: __m128,
    black_pivot: __m128,
    white_pivot: __m128,
    black_clamp: __m128,
    white_clamp: __m128,
}

impl Registers {
    #[inline]
    #[target_feature(enable = "sse2")]
    fn new(c: &Coefficients) -> Self {
        Registers {
            add: load(&c.add),
            scale: load(&c.scale),
            power: load(&c.power),
            pivot: _mm_set1_ps(c.pivot),
            saturation: _mm_set1_ps(c.saturation),
            black_pivot: _mm_set1_ps(c.black_pivot),
            white_pivot: _mm_set1_ps(c.white_pivot),
            black_clamp: _mm_set1_ps(c.black_clamp),
            white_clamp: _mm_set1_ps(c.white_clamp),
        }
    }
}

/// Port of `ApplyContrast` (GradingPrimaryOpCPU.cpp:130-133 @ v2.5.2).
#[inline]
#[target_feature(enable = "sse2")]
fn apply_contrast(pix: __m128, contrast: __m128, pivot: __m128) -> __m128 {
    _mm_add_ps(_mm_mul_ps(_mm_sub_ps(pix, pivot), contrast), pivot)
}

/// Port of `ApplyLinContrast` (GradingPrimaryOpCPU.cpp:135-142 @ v2.5.2).
#[inline]
#[target_feature(enable = "sse2")]
fn apply_lin_contrast(pix: __m128, contrast: __m128, pivot: __m128) -> __m128 {
    let pix = _mm_div_ps(pix, pivot);
    let sign_pix = _mm_and_ps(pix, _mm_set1_ps(-0.0));
    let abs_pix = _mm_andnot_ps(_mm_set1_ps(-0.0), pix);
    let pix = _mm_mul_ps(sse_power(abs_pix, contrast), pivot);
    _mm_xor_ps(pix, sign_pix)
}

/// Port of `ApplyGamma` (GradingPrimaryOpCPU.cpp:144-153 @ v2.5.2).
#[inline]
#[target_feature(enable = "sse2")]
fn apply_gamma(pix: __m128, gamma: __m128, black_pivot: __m128, white_pivot: __m128) -> __m128 {
    let pix = _mm_sub_ps(pix, black_pivot);
    let sign_pix = _mm_and_ps(pix, _mm_set1_ps(-0.0));
    let abs_pix = _mm_andnot_ps(_mm_set1_ps(-0.0), pix);
    let range = _mm_sub_ps(white_pivot, black_pivot);
    let pix = _mm_div_ps(abs_pix, range);
    let pix = sse_power(pix, gamma);
    _mm_add_ps(_mm_mul_ps(_mm_xor_ps(pix, sign_pix), range), black_pivot)
}

/// Port of `ApplySaturation` (GradingPrimaryOpCPU.cpp:155-170 @ v2.5.2).
#[inline]
#[target_feature(enable = "sse2")]
fn apply_saturation(pix: __m128, saturation: __m128) -> __m128 {
    let luma_weights = _mm_setr_ps(0.2126, 0.7152, 0.0722, 0.0);

    // Compute luma: dot product of pixel values and the luma weights.
    let luma = _mm_mul_ps(pix, luma_weights);

    // luma = [ x+y , y+x , z+w , w+z ]
    let luma = _mm_add_ps(luma, _mm_shuffle_ps::<0b10_11_00_01>(luma, luma));

    // luma = [ x+y+z+w , y+x+w+z , z+w+x+y , w+z+y+x ]
    let luma = _mm_add_ps(luma, _mm_shuffle_ps::<0b01_00_11_10>(luma, luma));

    // Apply saturation
    _mm_add_ps(luma, _mm_mul_ps(saturation, _mm_sub_ps(pix, luma)))
}

/// Port of `ApplyClamp` (GradingPrimaryOpCPU.cpp:172-175 @ v2.5.2).
#[inline]
#[target_feature(enable = "sse2")]
fn apply_clamp(pix: __m128, black_clamp: __m128, white_clamp: __m128) -> __m128 {
    _mm_min_ps(_mm_max_ps(pix, black_clamp), white_clamp)
}

/// The log and video styles forward: the loops of `GradingPrimaryLogFwdOpCPU::apply`
/// (GradingPrimaryOpCPU.cpp:289-418 @ v2.5.2) and `GradingPrimaryVidFwdOpCPU::apply` (983-1114).
#[target_feature(enable = "sse2")]
fn log_fwd<const SATURATION: bool, const GAMMA: bool>(block: &mut [[f32; 4]], c: &Coefficients) {
    let r = Registers::new(c);
    for px in block {
        let mut pixel = _mm_add_ps(load(px), r.add);
        pixel = apply_contrast(pixel, r.scale, r.pivot);
        if GAMMA {
            pixel = apply_gamma(pixel, r.power, r.black_pivot, r.white_pivot);
        }
        if SATURATION {
            pixel = apply_saturation(pixel, r.saturation);
        }
        store_rgb(px, apply_clamp(pixel, r.black_clamp, r.white_clamp));
    }
}

/// The log and video styles in reverse: the loops of `GradingPrimaryLogRevOpCPU::apply`
/// (GradingPrimaryOpCPU.cpp:467-592 @ v2.5.2) and `GradingPrimaryVidRevOpCPU::apply` (1162-1284).
#[target_feature(enable = "sse2")]
fn log_rev<const SATURATION: bool, const GAMMA: bool>(block: &mut [[f32; 4]], c: &Coefficients) {
    let r = Registers::new(c);
    for px in block {
        let mut pixel = apply_clamp(load(px), r.black_clamp, r.white_clamp);
        if SATURATION {
            pixel = apply_saturation(pixel, r.saturation);
        }
        if GAMMA {
            pixel = apply_gamma(pixel, r.power, r.black_pivot, r.white_pivot);
        }
        pixel = apply_contrast(pixel, r.scale, r.pivot);
        store_rgb(px, _mm_add_ps(pixel, r.add));
    }
}

/// The linear style forward: the loops of `GradingPrimaryLinFwdOpCPU::apply`
/// (GradingPrimaryOpCPU.cpp:636-766 @ v2.5.2).
#[target_feature(enable = "sse2")]
fn lin_fwd<const SATURATION: bool, const CONTRAST: bool>(block: &mut [[f32; 4]], c: &Coefficients) {
    let r = Registers::new(c);
    for px in block {
        let mut pixel = _mm_mul_ps(_mm_add_ps(load(px), r.add), r.scale);
        if CONTRAST {
            pixel = apply_lin_contrast(pixel, r.power, r.pivot);
        }
        if SATURATION {
            pixel = apply_saturation(pixel, r.saturation);
        }
        store_rgb(px, apply_clamp(pixel, r.black_clamp, r.white_clamp));
    }
}

/// The linear style in reverse: the loops of `GradingPrimaryLinRevOpCPU::apply`
/// (GradingPrimaryOpCPU.cpp:810-934 @ v2.5.2).
#[target_feature(enable = "sse2")]
fn lin_rev<const SATURATION: bool, const CONTRAST: bool>(block: &mut [[f32; 4]], c: &Coefficients) {
    let r = Registers::new(c);
    for px in block {
        let mut pixel = apply_clamp(load(px), r.black_clamp, r.white_clamp);
        if SATURATION {
            pixel = apply_saturation(pixel, r.saturation);
        }
        if CONTRAST {
            pixel = apply_lin_contrast(pixel, r.power, r.pivot);
        }
        store_rgb(px, _mm_add_ps(_mm_mul_ps(pixel, r.scale), r.add));
    }
}

macro_rules! kernel {
    ($(#[$doc:meta])* $name:ident, $kernel:ident) => {
        $(#[$doc])*
        pub(super) fn $name(
            block: &mut [[f32; 4]],
            c: &Coefficients,
            saturation: bool,
            power: bool,
        ) {
            // SAFETY: x86-64 has SSE2.
            unsafe {
                match (saturation, power) {
                    (false, false) => $kernel::<false, false>(block, c),
                    (false, true) => $kernel::<false, true>(block, c),
                    (true, false) => $kernel::<true, false>(block, c),
                    (true, true) => $kernel::<true, true>(block, c),
                }
            }
        }
    };
}

kernel!(
    /// The forward log and video styles on `block`, in place.
    render_log_fwd,
    log_fwd
);
kernel!(
    /// The reverse log and video styles on `block`, in place.
    render_log_rev,
    log_rev
);
kernel!(
    /// The forward linear style on `block`, in place.
    render_lin_fwd,
    lin_fwd
);
kernel!(
    /// The reverse linear style on `block`, in place.
    render_lin_rev,
    lin_rev
);
