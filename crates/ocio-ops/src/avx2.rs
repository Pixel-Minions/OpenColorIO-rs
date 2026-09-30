// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! AVX2 lane helpers (src/OpenColorIO/AVX2.h and the AVX2 kernels @ v2.5.2), as exact scalar code
//! for one SIMD lane, plus wrappers around the real instructions to prove them.
//!
//! The AVX2 and AVX-512 kernels are the only places in OCIO 2.5.2 that use FMA intrinsics
//! (`_mm256_fmadd_ps`, `_mm512_fmadd_ps`). One lane of either is `f32::mul_add`: a multiply and
//! an add with a single rounding.
//!
//! `unsafe` is allowed in this module for SIMD intrinsics only.
#![allow(unsafe_code)]

/// One lane of `_mm256_fmadd_ps(a, b, c)` (and `_mm512_fmadd_ps`): `a * b + c`, rounded once.
#[inline]
pub fn mm256_fmadd_ps(a: f32, b: f32, c: f32) -> f32 {
    a.mul_add(b, c)
}

/// One value of `AVX2RGBAPack<BD>::Load` (src/OpenColorIO/AVX2.h:70-300 @ v2.5.2): as
/// [`crate::sse2::rgba_pack_load`], with F16C for halves.
pub fn avx2_rgba_pack_load(depth: crate::sse2::PackDepth, raw: u32) -> f32 {
    crate::sse2::rgba_pack_load(depth, raw, crate::sse2::HalfConversion::F16c)
}

/// One value of `AVX2RGBAPack<BD>::Store` (src/OpenColorIO/AVX2.h:70-300 @ v2.5.2): as
/// [`crate::sse2::rgba_pack_store`], with F16C for halves.
pub fn avx2_rgba_pack_store(depth: crate::sse2::PackDepth, value: f32) -> u32 {
    crate::sse2::rgba_pack_store(depth, value, crate::sse2::HalfConversion::F16c)
}

/// The FMA instruction itself, to prove [`mm256_fmadd_ps`] against the hardware.
#[cfg(target_arch = "x86_64")]
pub mod hardware {
    use core::arch::x86_64::*;

    /// Whether this CPU and OS support AVX and FMA.
    pub fn available() -> bool {
        std::arch::is_x86_feature_detected!("avx") && std::arch::is_x86_feature_detected!("fma")
    }

    /// `out[i] = a[i] * b[i] + c[i]` with `_mm256_fmadd_ps`. Panics if FMA is missing.
    pub fn fmadd_ps(a: &[f32], b: &[f32], c: &[f32], out: &mut [f32]) {
        assert!(available(), "the CPU has no FMA");
        assert!(a.len() == b.len() && b.len() == c.len() && c.len() == out.len());
        // SAFETY: AVX and FMA are available (checked above).
        unsafe { fmadd_ps_fma(a, b, c, out) }
    }

    #[target_feature(enable = "avx,fma")]
    unsafe fn fmadd_ps_fma(a: &[f32], b: &[f32], c: &[f32], out: &mut [f32]) {
        for (i, o) in out.chunks_mut(8).enumerate() {
            let lanes = |s: &[f32]| {
                let mut v = [0f32; 8];
                let part = &s[8 * i..8 * i + o.len()];
                v[..part.len()].copy_from_slice(part);
                v
            };
            let (va, vb, vc) = (lanes(a), lanes(b), lanes(c));
            let mut r = [0f32; 8];
            // SAFETY: each array holds the 8 floats the loads and the store need.
            unsafe {
                let v = _mm256_fmadd_ps(
                    _mm256_loadu_ps(va.as_ptr()),
                    _mm256_loadu_ps(vb.as_ptr()),
                    _mm256_loadu_ps(vc.as_ptr()),
                );
                _mm256_storeu_ps(r.as_mut_ptr(), v);
            }
            o.copy_from_slice(&r[..o.len()]);
        }
    }
}

#[cfg(all(test, target_arch = "x86_64"))]
mod tests {
    use super::*;

    /// `f32::mul_add` (which Rust lowers to the platform's `fmaf` when FMA is not a compile-time
    /// target feature) equals the FMA instruction, bit for bit, on random bit patterns and on
    /// operands built to cancel, overflow and round at the edges.
    ///
    /// The operands are never NaN. With NaN operands the two pick different NaNs to return:
    /// the instruction returns its first NaN operand in encoding order (which depends on the
    /// register allocation of the 132/213/231 forms), while `fmaf` differs by platform. OCIO's
    /// FMA kernels never see a NaN: the LUT is sanitized, the weights are in `[0, 1]` and the
    /// sums can only overflow to infinity.
    #[test]
    fn mul_add_matches_fma_instruction() {
        if !hardware::available() {
            println!("skipped: this CPU has no FMA");
            return;
        }
        let mut rng = ocio_testkit::probe::Rng::new(0x464d_4121);
        let mut a = Vec::new();
        let mut b = Vec::new();
        let mut c = Vec::new();
        for _ in 0..(1 << 20) {
            a.push(rng.any_bits());
            b.push(rng.any_bits());
            c.push(rng.any_bits());
        }
        for _ in 0..(1 << 20) {
            // Products near c, so the single rounding matters.
            let x = rng.uniform(-2.0, 2.0);
            let y = rng.uniform(-2.0, 2.0);
            a.push(x);
            b.push(y);
            c.push(-(x * y) + rng.uniform(-1e-6, 1e-6));
        }
        let specials = ocio_testkit::probe::special_values();
        for &x in &specials {
            for &y in &specials {
                for &z in &[0.0, -0.0, 1.0, -f32::MAX, f32::MAX, f32::INFINITY] {
                    a.push(x);
                    b.push(y);
                    c.push(z);
                }
            }
        }
        // No NaN operands (see above). Invalid operations such as Inf * 0 stay in.
        let keep: Vec<usize> = (0..a.len())
            .filter(|&i| !(a[i].is_nan() || b[i].is_nan() || c[i].is_nan()))
            .collect();
        let (a, b, c): (Vec<f32>, Vec<f32>, Vec<f32>) = (
            keep.iter().map(|&i| a[i]).collect(),
            keep.iter().map(|&i| b[i]).collect(),
            keep.iter().map(|&i| c[i]).collect(),
        );

        let mut expected = vec![0f32; a.len()];
        hardware::fmadd_ps(&a, &b, &c, &mut expected);
        let actual: Vec<f32> = (0..a.len())
            .map(|i| mm256_fmadd_ps(a[i], b[i], c[i]))
            .collect();
        ocio_testkit::assert_f32_bits_eq("f32::mul_add vs _mm256_fmadd_ps", &expected, &actual);
    }
}

#[cfg(test)]
#[path = "avx2_tests.rs"]
mod simd_tests;
