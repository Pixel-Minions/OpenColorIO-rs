// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! AVX-512 lane helpers (src/OpenColorIO/AVX512.h and the AVX-512 kernels @ v2.5.2), as exact
//! scalar code for one SIMD lane. AVX-512 compares produce mask bits (`__mmask16`), one per
//! lane, instead of all-ones lanes.
//!
//! One lane of `_mm512_fmadd_ps` is [`crate::avx2::mm256_fmadd_ps`] (`f32::mul_add`); the tests
//! prove it against the AVX-512 instruction too.
//!
//! `unsafe` is allowed in this module for SIMD intrinsics only.
#![allow(unsafe_code)]

/// One lane of `_mm512_cmp_ps_mask(a, b, _CMP_GT_OQ)`: `a > b`, false when either is NaN.
#[inline]
pub fn mm512_cmp_gt_oq(a: f32, b: f32) -> bool {
    a > b
}

/// One lane of `_mm512_kandn(a, b)`: `!a & b`.
#[inline]
pub fn mm512_kandn(a: bool, b: bool) -> bool {
    !a & b
}

/// One lane of `_mm512_mask_blend_ps(k, a, b)`: `b` where the mask bit is set, `a` otherwise.
#[inline]
pub fn mm512_mask_blend_ps(k: bool, a: f32, b: f32) -> f32 {
    if k { b } else { a }
}

/// One value of `AVX512RGBAPack<BD>::Load` (src/OpenColorIO/AVX512.h:80-461 @ v2.5.2): as
/// [`crate::sse2::rgba_pack_load`], with F16C for halves.
pub fn avx512_rgba_pack_load(depth: crate::sse2::PackDepth, raw: u32) -> f32 {
    crate::sse2::rgba_pack_load(depth, raw, crate::sse2::HalfConversion::F16c)
}

/// One value of `AVX512RGBAPack<BD>::Store` (src/OpenColorIO/AVX512.h:80-461 @ v2.5.2): as
/// [`crate::sse2::rgba_pack_store`], with F16C for halves.
pub fn avx512_rgba_pack_store(depth: crate::sse2::PackDepth, value: f32) -> u32 {
    crate::sse2::rgba_pack_store(depth, value, crate::sse2::HalfConversion::F16c)
}

/// One value of `AVX512RGBAPack<BD>::LoadMasked` (src/OpenColorIO/AVX512.h:80-461 @ v2.5.2):
/// value `index` of a 16-pixel block, of which the first `pixel_count` pixels are loaded; the
/// masked-off lanes load zero bits, which convert to `+0.0`.
pub fn avx512_rgba_pack_load_masked(
    depth: crate::sse2::PackDepth,
    raw: u32,
    index: usize,
    pixel_count: usize,
) -> f32 {
    let raw = if index < 4 * pixel_count { raw } else { 0 };
    avx512_rgba_pack_load(depth, raw)
}

/// Whether `AVX512RGBAPack<BD>::StoreMasked` (src/OpenColorIO/AVX512.h:80-461 @ v2.5.2) writes
/// value `index` of a 16-pixel block when storing `pixel_count` pixels.
pub fn avx512_rgba_pack_store_masked_writes(index: usize, pixel_count: usize) -> bool {
    index < 4 * pixel_count
}

#[cfg(all(test, target_arch = "x86_64"))]
mod tests {
    use core::arch::x86_64::*;

    use super::*;
    use crate::avx2::mm256_fmadd_ps;

    #[target_feature(enable = "avx512f")]
    fn fmadd_and_blend_avx512(
        a: &[f32; 16],
        b: &[f32; 16],
        c: &[f32; 16],
    ) -> ([f32; 16], [f32; 16]) {
        let mut fma = [0f32; 16];
        let mut blend = [0f32; 16];
        // SAFETY: each array holds the 16 floats the loads and stores need.
        unsafe {
            let (va, vb, vc) = (
                _mm512_loadu_ps(a.as_ptr()),
                _mm512_loadu_ps(b.as_ptr()),
                _mm512_loadu_ps(c.as_ptr()),
            );
            _mm512_storeu_ps(fma.as_mut_ptr(), _mm512_fmadd_ps(va, vb, vc));
            let k = _mm512_kandn(
                _mm512_cmp_ps_mask::<_CMP_GT_OQ>(vb, vc),
                _mm512_cmp_ps_mask::<_CMP_GT_OQ>(va, vb),
            );
            _mm512_storeu_ps(blend.as_mut_ptr(), _mm512_mask_blend_ps(k, va, vc));
        }
        (fma, blend)
    }

    /// The FMA, compare, `kandn` and blend lanes equal the AVX-512 instructions on random values.
    #[test]
    fn lanes_match_avx512_instructions() {
        if !std::arch::is_x86_feature_detected!("avx512f") {
            println!("skipped: this CPU has no AVX-512F");
            return;
        }
        let mut rng = ocio_testkit::probe::Rng::new(0x4156_5835);
        let mut expected = Vec::new();
        let mut actual = Vec::new();
        for round in 0..(1 << 16) {
            let mut a = [0f32; 16];
            let mut b = [0f32; 16];
            let mut c = [0f32; 16];
            for i in 0..16 {
                if round % 2 == 0 {
                    (a[i], b[i], c[i]) = (rng.finite_bits(), rng.finite_bits(), rng.finite_bits());
                } else {
                    (a[i], b[i], c[i]) = (
                        rng.uniform(0.0, 1.0),
                        rng.uniform(0.0, 1.0),
                        rng.uniform(0.0, 1.0),
                    );
                }
            }
            // SAFETY: AVX-512F is available (checked above).
            let (fma, blend) = unsafe { fmadd_and_blend_avx512(&a, &b, &c) };
            for i in 0..16 {
                expected.push(fma[i]);
                actual.push(mm256_fmadd_ps(a[i], b[i], c[i]));
                let k = mm512_kandn(mm512_cmp_gt_oq(b[i], c[i]), mm512_cmp_gt_oq(a[i], b[i]));
                expected.push(blend[i]);
                actual.push(mm512_mask_blend_ps(k, a[i], c[i]));
            }
        }
        ocio_testkit::assert_f32_bits_eq("AVX-512 lanes", &expected, &actual);
    }
}

#[cfg(test)]
#[path = "avx512_tests.rs"]
mod simd_tests;
