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

/// The real `AVX512RGBAPack`s: sixteen pixels, 64 values. The kernels only load F32, and they
/// leave the last pixels of a row to the exact lanes, so `LoadMasked` and `StoreMasked` have
/// no counterpart here.
#[cfg(target_arch = "x86_64")]
pub(crate) mod packs {
    use core::arch::x86_64::*;

    /// Port of `av512_clamp` (src/OpenColorIO/AVX512.h:24-28 @ v2.5.2).
    #[inline]
    #[target_feature(enable = "avx512f")]
    fn clamp(value: __m512, max_value: __m512) -> __m512 {
        _mm512_min_ps(_mm512_max_ps(value, _mm512_setzero_ps()), max_value)
    }

    /// Port of `avx512_movelh_ps` (src/OpenColorIO/AVX512.h:30-33 @ v2.5.2).
    #[inline]
    #[target_feature(enable = "avx512f")]
    fn movelh_ps(a: __m512, b: __m512) -> __m512 {
        _mm512_castpd_ps(_mm512_unpacklo_pd(_mm512_castps_pd(a), _mm512_castps_pd(b)))
    }

    /// Port of `avx512_movehl_ps` (src/OpenColorIO/AVX512.h:35-39 @ v2.5.2).
    #[inline]
    #[target_feature(enable = "avx512f")]
    fn movehl_ps(a: __m512, b: __m512) -> __m512 {
        // NOTE: this is a and b are reversed to match sse2 movhlps which is different than unpckhpd
        _mm512_castpd_ps(_mm512_unpackhi_pd(_mm512_castps_pd(b), _mm512_castps_pd(a)))
    }

    /// Port of `avx512RGBATranspose_4x4_4x4_4x4_4x4` (src/OpenColorIO/AVX512.h:42-76 @ v2.5.2).
    #[inline]
    #[target_feature(enable = "avx512f")]
    pub(crate) fn transpose(rows: [__m512; 4]) -> [__m512; 4] {
        let tmp0 = _mm512_unpacklo_ps(rows[0], rows[1]);
        let tmp2 = _mm512_unpacklo_ps(rows[2], rows[3]);
        let tmp1 = _mm512_unpackhi_ps(rows[0], rows[1]);
        let tmp3 = _mm512_unpackhi_ps(rows[2], rows[3]);
        [
            movelh_ps(tmp0, tmp2),
            movehl_ps(tmp2, tmp0),
            movelh_ps(tmp1, tmp3),
            movehl_ps(tmp3, tmp1),
        ]
    }

    /// Port of `AVX512RGBAPack<BIT_DEPTH_F32>::Load` (src/OpenColorIO/AVX512.h:398-408 @ v2.5.2).
    #[inline]
    #[target_feature(enable = "avx512f")]
    pub(crate) fn load_f32(input: &[f32; 64]) -> [__m512; 4] {
        // SAFETY: `input` holds the 64 floats these read, at any alignment.
        transpose(std::array::from_fn(|i| unsafe {
            _mm512_loadu_ps(input.as_ptr().add(16 * i))
        }))
    }

    /// Port of `AVX512RGBAPack<BIT_DEPTH_F32>::Store` (src/OpenColorIO/AVX512.h:431-442 @
    /// v2.5.2), with a store mask: `rgba` is written where `mask` has a bit set.
    #[inline]
    #[target_feature(enable = "avx512f")]
    pub(crate) fn store_f32(out: &mut [f32; 64], rgba: [__m512; 4], mask: __mmask16) {
        for (i, row) in transpose(rgba).into_iter().enumerate() {
            // SAFETY: `out` holds the 64 floats these write, at any alignment.
            unsafe { _mm512_mask_storeu_ps(out.as_mut_ptr().add(16 * i), mask, row) };
        }
    }

    /// The clamped and rounded rows of the integer `Store`s.
    #[inline]
    #[target_feature(enable = "avx512f")]
    fn rounded_rows([r, g, b, a]: [__m512; 4], max_value: f32) -> [__m512i; 4] {
        let max_value = _mm512_set1_ps(max_value);
        let clamped = [r, g, b, a].map(|channel| clamp(channel, max_value));
        transpose(clamped).map(|row| _mm512_cvtps_epi32(row))
    }

    /// Port of `AVX512RGBAPack<BIT_DEPTH_UINT8>::Store` (src/OpenColorIO/AVX512.h:112-132 @
    /// v2.5.2).
    #[inline]
    #[target_feature(enable = "avx512f")]
    pub(crate) fn store_u8(out: &mut [u8; 64], rgba: [__m512; 4]) {
        for (i, row) in rounded_rows(rgba, 255.0).into_iter().enumerate() {
            // SAFETY: `out` holds the 64 bytes these write, at any alignment.
            unsafe {
                _mm512_mask_cvtepi32_storeu_epi8(out.as_mut_ptr().add(16 * i).cast(), 0xffff, row);
            }
        }
    }

    /// Port of `AVX512RGBAPack16<BD>::Store` (src/OpenColorIO/AVX512.h:206-224 @ v2.5.2), where
    /// `max_value` is `BitDepthInfo<BD>::maxValue`.
    #[inline]
    #[target_feature(enable = "avx512f")]
    pub(crate) fn store_u16(out: &mut [u16; 64], rgba: [__m512; 4], max_value: f32) {
        for (i, row) in rounded_rows(rgba, max_value).into_iter().enumerate() {
            // SAFETY: `out` holds the 64 values these write, at any alignment.
            unsafe {
                _mm512_mask_cvtepi32_storeu_epi16(out.as_mut_ptr().add(16 * i).cast(), 0xffff, row);
            }
        }
    }

    /// Port of `AVX512RGBAPack<BIT_DEPTH_F16>::Store` (src/OpenColorIO/AVX512.h:360-371 @ v2.5.2).
    #[inline]
    #[target_feature(enable = "avx512f")]
    pub(crate) fn store_f16(out: &mut [half::f16; 64], rgba: [__m512; 4]) {
        let [rgba0, rgba1, rgba2, rgba3] = transpose(rgba);

        let first = _mm512_inserti64x4::<1>(
            _mm512_castsi256_si512(_mm512_cvtps_ph::<0>(rgba0)),
            _mm512_cvtps_ph::<0>(rgba1),
        );
        let second = _mm512_inserti64x4::<1>(
            _mm512_castsi256_si512(_mm512_cvtps_ph::<0>(rgba2)),
            _mm512_cvtps_ph::<0>(rgba3),
        );

        // SAFETY: `out` holds the 64 values these write, at any alignment.
        unsafe {
            _mm512_storeu_si512(out.as_mut_ptr().cast(), first);
            _mm512_storeu_si512(out.as_mut_ptr().add(32).cast(), second);
        }
    }
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
