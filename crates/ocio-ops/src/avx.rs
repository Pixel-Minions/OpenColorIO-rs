// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The F16C half-float conversions that `AVX.h`, `AVX2.h` and `AVX512.h` use
//! (src/OpenColorIO/AVX.h:267-306, AVX2.h:237-276 and AVX512.h:319-393 @ v2.5.2), as exact scalar
//! code for one lane, plus wrappers around the real instructions to prove them.
//!
//! The kernels call `_mm256_cvtps_ph(x, 0)` / `_mm512_cvtps_ph(x, 0)` (rounding to nearest
//! even) and `_mm256_cvtph_ps` / `_mm512_cvtph_ps`. F16C rounds like IEEE 754, keeps the top
//! 10 mantissa bits of a NaN and makes it quiet. The `half` crate's software conversions
//! (`from_f32_const`, `to_f32_const`) implement exactly that; the tests prove it against the
//! hardware for every input.
//!
//! `unsafe` is allowed in this module for SIMD intrinsics only.
#![allow(unsafe_code)]

/// One lane of `_mm256_cvtps_ph(a, 0)`: float to half bits, rounding to nearest even.
#[inline]
pub fn f16c_cvtps_ph(a: f32) -> u16 {
    half::f16::from_f32_const(a).to_bits()
}

/// One lane of `_mm256_cvtph_ps(a)`: half bits to float. NaNs come out quiet.
#[inline]
pub fn f16c_cvtph_ps(h: u16) -> f32 {
    half::f16::from_bits(h).to_f32_const()
}

/// One value of `AVXRGBAPack<BD>::Load` (src/OpenColorIO/AVX.h:94-332 @ v2.5.2): as
/// [`crate::sse2::rgba_pack_load`], with F16C for halves.
pub fn avx_rgba_pack_load(depth: crate::sse2::PackDepth, raw: u32) -> f32 {
    crate::sse2::rgba_pack_load(depth, raw, crate::sse2::HalfConversion::F16c)
}

/// One value of `AVXRGBAPack<BD>::Store` (src/OpenColorIO/AVX.h:94-332 @ v2.5.2): as
/// [`crate::sse2::rgba_pack_store`], with F16C for halves.
pub fn avx_rgba_pack_store(depth: crate::sse2::PackDepth, value: f32) -> u32 {
    crate::sse2::rgba_pack_store(depth, value, crate::sse2::HalfConversion::F16c)
}

/// The F16C instructions themselves, to prove the scalar lanes against the hardware.
#[cfg(target_arch = "x86_64")]
pub mod hardware {
    use core::arch::x86_64::*;

    /// Whether this CPU and OS support the F16C and AVX instructions used here.
    pub fn available() -> bool {
        std::arch::is_x86_feature_detected!("avx") && std::arch::is_x86_feature_detected!("f16c")
    }

    /// Converts floats to half bits with `_mm256_cvtps_ph(x, 0)`. Panics if F16C is missing.
    pub fn cvtps_ph(src: &[f32], dst: &mut [u16]) {
        assert!(available(), "the CPU has no F16C");
        assert_eq!(src.len(), dst.len());
        // SAFETY: AVX and F16C are available (checked above).
        unsafe { cvtps_ph_f16c(src, dst) }
    }

    /// Converts half bits to floats with `_mm256_cvtph_ps`. Panics if F16C is missing.
    pub fn cvtph_ps(src: &[u16], dst: &mut [f32]) {
        assert!(available(), "the CPU has no F16C");
        assert_eq!(src.len(), dst.len());
        // SAFETY: AVX and F16C are available (checked above).
        unsafe { cvtph_ps_f16c(src, dst) }
    }

    #[target_feature(enable = "avx,f16c")]
    unsafe fn cvtps_ph_f16c(src: &[f32], dst: &mut [u16]) {
        for (s, d) in src.chunks(8).zip(dst.chunks_mut(8)) {
            let mut lanes = [0f32; 8];
            lanes[..s.len()].copy_from_slice(s);
            let mut halves = [0u16; 8];
            // SAFETY: `lanes` holds 8 floats and `halves` 8 halves, as the load and store need.
            unsafe {
                let v = _mm256_loadu_ps(lanes.as_ptr());
                let h = _mm256_cvtps_ph::<0>(v);
                _mm_storeu_si128(halves.as_mut_ptr().cast::<__m128i>(), h);
            }
            d.copy_from_slice(&halves[..d.len()]);
        }
    }

    #[target_feature(enable = "avx,f16c")]
    unsafe fn cvtph_ps_f16c(src: &[u16], dst: &mut [f32]) {
        for (s, d) in src.chunks(8).zip(dst.chunks_mut(8)) {
            let mut halves = [0u16; 8];
            halves[..s.len()].copy_from_slice(s);
            let mut lanes = [0f32; 8];
            // SAFETY: `halves` holds 8 halves and `lanes` 8 floats, as the load and store need.
            unsafe {
                let h = _mm_loadu_si128(halves.as_ptr().cast::<__m128i>());
                let v = _mm256_cvtph_ps(h);
                _mm256_storeu_ps(lanes.as_mut_ptr(), v);
            }
            d.copy_from_slice(&lanes[..d.len()]);
        }
    }
}

#[cfg(test)]
#[path = "avx_tests.rs"]
mod simd_tests;
