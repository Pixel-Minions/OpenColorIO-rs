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

/// One lane of `_mm256_blendv_ps(a, b, mask)`: `b` where the mask's sign bit is set, `a`
/// otherwise. Also `blendv_avx` (src/OpenColorIO/ops/lut3d/Lut3DOpCPU_AVX.cpp:61-73 @ v2.5.2),
/// whose inline-assembly form for GCC 12.0-12.2 is the same `vblendvps` instruction.
#[inline]
pub fn avx_blendv_ps(a: f32, b: f32, mask: u32) -> f32 {
    if mask & 0x8000_0000 != 0 { b } else { a }
}

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

/// The real `AVXRGBAPack`s: eight pixels, 32 values. The AVX2 kernels store through them too:
/// `AVX2RGBAPack`'s `Store` (src/OpenColorIO/AVX2.h:70-300 @ v2.5.2) gives the same values with
/// wider integer instructions. The kernels only load F32.
#[cfg(target_arch = "x86_64")]
pub(crate) mod packs {
    use core::arch::x86_64::*;

    /// Port of `avx_movelh_ps` (src/OpenColorIO/AVX.h:22-25 @ v2.5.2).
    #[inline]
    #[target_feature(enable = "avx")]
    fn movelh_ps(a: __m256, b: __m256) -> __m256 {
        _mm256_castpd_ps(_mm256_unpacklo_pd(_mm256_castps_pd(a), _mm256_castps_pd(b)))
    }

    /// Port of `avx_movehl_ps` (src/OpenColorIO/AVX.h:27-31 @ v2.5.2).
    #[inline]
    #[target_feature(enable = "avx")]
    fn movehl_ps(a: __m256, b: __m256) -> __m256 {
        // NOTE: this is a and b are reversed to match sse2 movhlps which is different than unpckhpd
        _mm256_castpd_ps(_mm256_unpackhi_pd(_mm256_castps_pd(b), _mm256_castps_pd(a)))
    }

    /// Port of `avx_clamp` (src/OpenColorIO/AVX.h:33-37 @ v2.5.2).
    #[inline]
    #[target_feature(enable = "avx")]
    fn clamp(value: __m256, max_value: __m256) -> __m256 {
        _mm256_min_ps(_mm256_max_ps(value, _mm256_setzero_ps()), max_value)
    }

    /// Port of `avxRGBATranspose_4x4_4x4` (src/OpenColorIO/AVX.h:39-62 @ v2.5.2).
    #[inline]
    #[target_feature(enable = "avx")]
    pub(crate) fn transpose(rows: [__m256; 4]) -> [__m256; 4] {
        let tmp0 = _mm256_unpacklo_ps(rows[0], rows[1]);
        let tmp2 = _mm256_unpacklo_ps(rows[2], rows[3]);
        let tmp1 = _mm256_unpackhi_ps(rows[0], rows[1]);
        let tmp3 = _mm256_unpackhi_ps(rows[2], rows[3]);
        [
            movelh_ps(tmp0, tmp2),
            movehl_ps(tmp2, tmp0),
            movelh_ps(tmp1, tmp3),
            movehl_ps(tmp3, tmp1),
        ]
    }

    /// Port of `AVXRGBAPack<BIT_DEPTH_F32>::Load` (src/OpenColorIO/AVX.h:312-321 @ v2.5.2).
    #[inline]
    #[target_feature(enable = "avx")]
    pub(crate) fn load_f32(input: &[f32; 32]) -> [__m256; 4] {
        // SAFETY: `input` holds the 32 floats these read, at any alignment.
        transpose(std::array::from_fn(|i| unsafe {
            _mm256_loadu_ps(input.as_ptr().add(8 * i))
        }))
    }

    /// Port of `AVXRGBAPack<BIT_DEPTH_F32>::Store` (src/OpenColorIO/AVX.h:323-332 @ v2.5.2).
    #[inline]
    #[target_feature(enable = "avx")]
    pub(crate) fn store_f32(out: &mut [f32; 32], rgba: [__m256; 4]) {
        for (i, row) in transpose(rgba).into_iter().enumerate() {
            // SAFETY: `out` holds the 32 floats these write, at any alignment.
            unsafe { _mm256_storeu_ps(out.as_mut_ptr().add(8 * i), row) };
        }
    }

    /// The clamped and rounded rows of the integer `Store`s.
    #[inline]
    #[target_feature(enable = "avx")]
    fn rounded_rows(rgba: [__m256; 4], max_value: f32) -> [__m256i; 4] {
        let max_value = _mm256_set1_ps(max_value);
        // NOTE note using cvtps which will round based on MXCSR register defaults to _MM_ROUND_NEAREST
        transpose(rgba).map(|row| _mm256_cvtps_epi32(clamp(row, max_value)))
    }

    /// Port of `avx_pack_u8` (src/OpenColorIO/AVX.h:72-85 @ v2.5.2).
    #[inline]
    #[target_feature(enable = "avx")]
    fn pack_u8(a: __m256i, b: __m256i) -> __m128i {
        let lo0_shuf = _mm_setr_epi8(0, 4, 8, 12, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1);
        let lo1_shuf = _mm_setr_epi8(-1, -1, -1, -1, 0, 4, 8, 12, -1, -1, -1, -1, -1, -1, -1, -1);
        let hi0_shuf = _mm_setr_epi8(-1, -1, -1, -1, -1, -1, -1, -1, 0, 4, 8, 12, -1, -1, -1, -1);
        let hi1_shuf = _mm_setr_epi8(-1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, 0, 4, 8, 12);

        let lo0 = _mm_shuffle_epi8(_mm256_castsi256_si128(a), lo0_shuf);
        let lo1 = _mm_shuffle_epi8(_mm256_extractf128_si256::<1>(a), lo1_shuf);
        let hi0 = _mm_shuffle_epi8(_mm256_castsi256_si128(b), hi0_shuf);
        let hi1 = _mm_shuffle_epi8(_mm256_extractf128_si256::<1>(b), hi1_shuf);

        _mm_or_si128(_mm_or_si128(_mm_or_si128(lo0, lo1), hi0), hi1)
    }

    /// Port of `AVXRGBAPack<BIT_DEPTH_UINT8>::Store` (src/OpenColorIO/AVX.h:114-139 @ v2.5.2).
    #[inline]
    #[target_feature(enable = "avx")]
    pub(crate) fn store_u8(out: &mut [u8; 32], rgba: [__m256; 4]) {
        let [rgba01, rgba23, rgba45, rgba67] = rounded_rows(rgba, 255.0);

        let lo = pack_u8(rgba01, rgba23);
        let hi = pack_u8(rgba45, rgba67);

        let rgba = _mm256_insertf128_si256::<1>(_mm256_castsi128_si256(lo), hi);
        // SAFETY: `out` holds the 32 bytes this writes, at any alignment.
        unsafe { _mm256_storeu_si256(out.as_mut_ptr().cast(), rgba) };
    }

    /// Port of `avx_pack_u16` (src/OpenColorIO/AVX.h:150-164 @ v2.5.2).
    #[inline]
    #[target_feature(enable = "avx")]
    fn pack_u16(a: __m256i) -> __m128i {
        let lo_pack_shuffle16 =
            _mm_setr_epi8(0, 1, 4, 5, 8, 9, 12, 13, -1, -1, -1, -1, -1, -1, -1, -1);
        let hi_pack_shuffle16 =
            _mm_setr_epi8(-1, -1, -1, -1, -1, -1, -1, -1, 0, 1, 4, 5, 8, 9, 12, 13);

        let hi = _mm256_extractf128_si256::<1>(a);
        let lo = _mm256_castsi256_si128(a);

        let lo = _mm_shuffle_epi8(lo, lo_pack_shuffle16);
        let hi = _mm_shuffle_epi8(hi, hi_pack_shuffle16);

        _mm_or_si128(lo, hi)
    }

    /// Port of `AVXRGBAPack16<BD>::Store` (src/OpenColorIO/AVX.h:184-217 @ v2.5.2), where
    /// `max_value` is `BitDepthInfo<BD>::maxValue`.
    #[inline]
    #[target_feature(enable = "avx")]
    pub(crate) fn store_u16(out: &mut [u16; 32], rgba: [__m256; 4], max_value: f32) {
        let [rgba01, rgba23, rgba45, rgba67] = rounded_rows(rgba, max_value);

        let lo = pack_u16(rgba01);
        let hi = pack_u16(rgba23);
        let first = _mm256_insertf128_si256::<1>(_mm256_castsi128_si256(lo), hi);

        let lo = pack_u16(rgba45);
        let hi = pack_u16(rgba67);
        let second = _mm256_insertf128_si256::<1>(_mm256_castsi128_si256(lo), hi);

        // SAFETY: `out` holds the 32 values these write, at any alignment.
        unsafe {
            _mm256_storeu_si256(out.as_mut_ptr().cast(), first);
            _mm256_storeu_si256(out.as_mut_ptr().add(16).cast(), second);
        }
    }

    /// Port of `AVXRGBAPack<BIT_DEPTH_F16>::Store` (src/OpenColorIO/AVX.h:284-303 @ v2.5.2).
    #[inline]
    #[target_feature(enable = "avx,f16c")]
    pub(crate) fn store_f16(out: &mut [half::f16; 32], rgba: [__m256; 4]) {
        let [rgba0, rgba1, rgba2, rgba3] = transpose(rgba);

        let rgba00_03 = _mm256_cvtps_ph::<0>(rgba0);
        let rgba04_07 = _mm256_cvtps_ph::<0>(rgba1);
        let rgba08_11 = _mm256_cvtps_ph::<0>(rgba2);
        let rgba12_16 = _mm256_cvtps_ph::<0>(rgba3);

        let first = _mm256_insertf128_si256::<1>(_mm256_castsi128_si256(rgba00_03), rgba04_07);
        let second = _mm256_insertf128_si256::<1>(_mm256_castsi128_si256(rgba08_11), rgba12_16);

        // SAFETY: `out` holds the 32 values these write, at any alignment.
        unsafe {
            _mm256_storeu_si256(out.as_mut_ptr().cast(), first);
            _mm256_storeu_si256(out.as_mut_ptr().add(16).cast(), second);
        }
    }
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
