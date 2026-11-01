// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The SSE2 helpers of `SSE2.h` (src/OpenColorIO/SSE2.h @ v2.5.2), as exact scalar code for one
//! SIMD lane: the compare and mask lanes the kernels use, `sse2_clamp`, the software half-float
//! conversions that OCIO's SSE2 kernels use on x86 (`!OCIO_USE_SSE2NEON`), and the per-value
//! semantics of the RGBA packs. The SSE min/max and conversion lanes are
//! [`crate::math_utils::sse_min`], [`sse_max`](crate::math_utils::sse_max) and
//! [`sse_cvttps_epi32`](crate::math_utils::sse_cvttps_epi32).
//!
//! Each `__m128` operation works on its four lanes independently, so one lane reproduces the
//! kernel's arithmetic exactly. The [`intrinsics`] module transcribes the same functions with
//! the real SSE2 instructions, to prove the scalar lanes against the hardware.
//!
//! The [`blocks`] module holds what the real SIMD fast paths share (`docs/cards/phase8.md`).
//!
//! `unsafe` is allowed in this module for SIMD intrinsics only.
#![allow(unsafe_code)]

use crate::math_utils::{sse_cvtps_epi32, sse_max, sse_min};

/// One lane of `_mm_cmpgt_ps(a, b)` (and of `_CMP_GT_OQ` compares): all ones when `a > b`,
/// zero otherwise, including when either value is NaN.
#[inline]
pub fn mm_cmpgt_ps(a: f32, b: f32) -> u32 {
    if a > b { u32::MAX } else { 0 }
}

/// One lane of `_mm_andnot_ps(a, b)` on mask bits: `!a & b`.
#[inline]
pub fn mm_andnot_ps(a: u32, b: u32) -> u32 {
    !a & b
}

/// One lane of `sse2_clamp` (src/OpenColorIO/SSE2.h:85-89 @ v2.5.2): NaN becomes 0.
#[inline]
pub fn sse2_clamp(value: f32, max_value: f32) -> f32 {
    sse_min(sse_max(value, 0.0), max_value)
}

/// One lane of `sse2_blendv` (src/OpenColorIO/SSE2.h:106-109 @ v2.5.2):
/// `((a ^ b) & mask) ^ a`.
#[inline]
pub fn sse2_blendv(a: u32, b: u32, mask: u32) -> u32 {
    ((a ^ b) & mask) ^ a
}

/// One lane of `sse2_cvtps_ph` (src/OpenColorIO/SSE2.h:111-153 @ v2.5.2): float to half bits,
/// rounding to nearest even through a float addition.
///
/// The final shuffles move each lane's low 16 bits into the low 64 bits of the register and
/// leave zeros above them, so one lane's result is the low 16 bits of `ph`.
pub fn sse2_cvtps_ph(a: f32) -> u16 {
    let x = a.to_bits();

    let x_sgn = x & 0x8000_0000;
    let x_exp = x & 0x7f80_0000;

    let magic1 = f32::from_bits(0x7780_0000); // 0x1.0p+112f
    let magic2 = f32::from_bits(0x0880_0000); // 0x1.0p-110f

    // SSE2 has no _mm_max_epu32, but _mm_max_ps works.
    let exp_max = 0x3880_0000u32;
    let x_exp = sse_max(f32::from_bits(x_exp), f32::from_bits(exp_max)).to_bits(); // max(e, -14)
    let x_exp = x_exp.wrapping_add(15 << 23); // e += 15
    let x = x & 0x7fff_ffff; // Discard the sign.

    let mut f = f32::from_bits(x);
    let magicf = f32::from_bits(x_exp);

    // If 15 < e then inf, otherwise e += 2.
    f = (f * magic1) * magic2;
    f += magicf;

    let u = f.to_bits();

    let h_exp = (u >> 13) & 0x7c00;
    let mut h_sig = u & 0x0fff;

    // Blend in NaN values (only if present, which does not change the result).
    // _mm_cmpgt_epi32 compares signed 32-bit integers; `x` has no sign bit here.
    let nan_mask = if (x as i32) > 0x7f80_0000 {
        u32::MAX
    } else {
        0
    };
    if nan_mask != 0 {
        let nan = ((x >> 13) & 0x03ff) | 0x0200;
        h_sig = sse2_blendv(h_sig, nan, nan_mask);
    }

    let ph = (x_sgn >> 16).wrapping_add(h_exp.wrapping_add(h_sig));
    ph as u16
}

/// One lane of `sse2_cvtph_ps` (src/OpenColorIO/SSE2.h:155-192 @ v2.5.2): half bits to float.
/// NaNs come out quiet (bit 22 set).
pub fn sse2_cvtph_ps(h: u16) -> f32 {
    let magic = f32::from_bits((254 - 15) << 23);
    let was_infnan = f32::from_bits((127 + 16) << 23);

    // _mm_unpacklo_epi16(a, a) puts the half in both 16-bit halves of the lane.
    let a = u32::from(h) | (u32::from(h) << 16);

    // Extract the sign.
    let sign = (a & 0x8000) << 16;

    // Extract the exponent and mantissa bits.
    let mut o = f32::from_bits((a & 0x7fff) << 13);

    // Magic multiply.
    o *= magic;

    // Blend in infinities and NaNs (only if present, which does not change the result).
    // _mm_cmpge_ps is an ordered compare; `o` is never NaN here.
    let mask = if o >= was_infnan { u32::MAX } else { 0 };
    if mask != 0 {
        let ou = o.to_bits();
        let ou_nan = ou | (0x01ff << 22);
        let ou_inf = ou | (0x00ff << 23);

        // Blend in NaNs.
        let ou = sse2_blendv(ou, ou_nan, mask);

        // Blend in infinities.
        let inf_mask = if o.to_bits() == was_infnan.to_bits() {
            u32::MAX
        } else {
            0
        };
        o = f32::from_bits(sse2_blendv(ou, ou_inf, inf_mask));
    }

    f32::from_bits(o.to_bits() | sign)
}

/// A channel storage type of the RGBA packs: the `BitDepth` template argument of
/// `SSE2RGBAPack<BD>` and its AVX, AVX2 and AVX-512 counterparts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackDepth {
    /// `BIT_DEPTH_UINT8`: `uint8_t`
    Uint8,
    /// `BIT_DEPTH_UINT10`: `uint16_t`
    Uint10,
    /// `BIT_DEPTH_UINT12`: `uint16_t`
    Uint12,
    /// `BIT_DEPTH_UINT16`: `uint16_t`
    Uint16,
    /// `BIT_DEPTH_F16`: half bits
    F16,
    /// `BIT_DEPTH_F32`: float bits
    F32,
}

impl PackDepth {
    /// `BitDepthInfo<BD>::maxValue` (src/OpenColorIO/BitDepthUtils.h:28-62 @ v2.5.2).
    pub fn max_value(self) -> u32 {
        match self {
            PackDepth::Uint8 => 255,
            PackDepth::Uint10 => 1023,
            PackDepth::Uint12 => 4095,
            PackDepth::Uint16 => 65535,
            PackDepth::F16 | PackDepth::F32 => 1,
        }
    }

    /// `BitDepthInfo<BD>::isFloat`.
    pub fn is_float(self) -> bool {
        matches!(self, PackDepth::F16 | PackDepth::F32)
    }
}

/// The half-float conversion a pack uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HalfConversion {
    /// `sse2_cvtph_ps` / `sse2_cvtps_ph` (SSE2 packs, without SSE2NEON).
    Sse2,
    /// F16C (`_mm256_cvtph_ps`, `_mm256_cvtps_ph(x, 0)` and their AVX-512 forms).
    F16c,
}

/// One value of an RGBA pack's `Load`: a stored channel value (`raw`, the integer, or the bits
/// of a half or float) as the float the kernels compute with.
///
/// The packs load 4 (SSE2), 8 (AVX, AVX2) or 16 (AVX-512) pixels and transpose them into R, G, B
/// and A registers; the transposes only move values, so each value converts on its own:
/// integers exactly (`_mm_cvtepi32_ps` of the zero-extended value), halves with `half`, floats
/// unchanged.
pub fn rgba_pack_load(depth: PackDepth, raw: u32, half: HalfConversion) -> f32 {
    match depth {
        PackDepth::Uint8 | PackDepth::Uint10 | PackDepth::Uint12 | PackDepth::Uint16 => raw as f32,
        PackDepth::F16 => match half {
            HalfConversion::Sse2 => sse2_cvtph_ps(raw as u16),
            HalfConversion::F16c => crate::avx::f16c_cvtph_ps(raw as u16),
        },
        PackDepth::F32 => f32::from_bits(raw),
    }
}

/// One value of an RGBA pack's `Store`: the inverse of [`rgba_pack_load`]. Integers are clamped
/// to `[0, maxValue]` (NaN becomes 0), then rounded to nearest even by `_mm_cvtps_epi32` with the
/// default MXCSR, then narrowed; halves use `half`; floats are stored unchanged.
pub fn rgba_pack_store(depth: PackDepth, value: f32, half: HalfConversion) -> u32 {
    match depth {
        PackDepth::Uint8 | PackDepth::Uint10 | PackDepth::Uint12 | PackDepth::Uint16 => {
            sse_cvtps_epi32(sse2_clamp(value, depth.max_value() as f32)) as u32
        }
        PackDepth::F16 => u32::from(match half {
            HalfConversion::Sse2 => sse2_cvtps_ph(value),
            HalfConversion::F16c => crate::avx::f16c_cvtps_ph(value),
        }),
        PackDepth::F32 => value.to_bits(),
    }
}

/// One value of `SSE2RGBAPack<BD>::Load` (src/OpenColorIO/SSE2.h:198-401 @ v2.5.2).
pub fn sse2_rgba_pack_load(depth: PackDepth, raw: u32) -> f32 {
    rgba_pack_load(depth, raw, HalfConversion::Sse2)
}

/// One value of `SSE2RGBAPack<BD>::Store` (src/OpenColorIO/SSE2.h:198-401 @ v2.5.2).
pub fn sse2_rgba_pack_store(depth: PackDepth, value: f32) -> u32 {
    rgba_pack_store(depth, value, HalfConversion::Sse2)
}

/// One x86 scalar instruction each, for `math_utils::sse_add`, `sse_sub` and `sse_mul`.
///
/// An instruction gives the first operand's NaN where two meet, as C++'s `addss` does, without
/// the test of a NaN that the same rule takes in Rust: LLVM's `fadd` and `fmul` are commutative,
/// so `+` and `*` don't say which NaN comes out.
#[cfg(target_arch = "x86_64")]
pub mod scalar {
    use core::arch::asm;

    macro_rules! instruction {
        ($name:ident, $ty:ty, $text:literal) => {
            /// The instruction with `a` as its first source operand.
            #[inline(always)]
            pub fn $name(a: $ty, b: $ty) -> $ty {
                let out: $ty;
                // SAFETY: the instruction reads two registers and writes the first, and x86-64
                // has SSE2.
                unsafe {
                    asm!(
                        $text,
                        inlateout(xmm_reg) a => out,
                        in(xmm_reg) b,
                        options(pure, nomem, nostack, preserves_flags)
                    );
                }
                out
            }
        };
    }

    instruction!(addss, f32, "addss {0}, {1}");
    instruction!(subss, f32, "subss {0}, {1}");
    instruction!(mulss, f32, "mulss {0}, {1}");
    instruction!(addsd, f64, "addsd {0}, {1}");
    instruction!(subsd, f64, "subsd {0}, {1}");
    instruction!(mulsd, f64, "mulsd {0}, {1}");
}

/// The x86 packed instructions with their operand order fixed, which Rust's `_mm_add_ps` and
/// `_mm_mul_ps`, LLVM's commutative `fadd` and `fmul`, don't fix. With NaN operands the result
/// is the instruction's, as in the wheels' loops of the same instructions.
#[cfg(target_arch = "x86_64")]
pub mod packed {
    use core::arch::asm;
    use core::arch::x86_64::__m128;

    macro_rules! instruction {
        ($name:ident, $text:literal) => {
            /// The instruction with `a` as its first source operand.
            #[inline(always)]
            pub fn $name(a: __m128, b: __m128) -> __m128 {
                let out: __m128;
                // SAFETY: the instruction reads two registers and writes the first, and x86-64
                // has SSE.
                unsafe {
                    asm!(
                        $text,
                        inlateout(xmm_reg) a => out,
                        in(xmm_reg) b,
                        options(pure, nomem, nostack, preserves_flags)
                    );
                }
                out
            }
        };
    }

    instruction!(addps, "addps {0}, {1}");
    instruction!(mulps, "mulps {0}, {1}");
}

/// What the renderers' real SIMD fast paths share (`docs/cards/phase8.md`).
///
/// Rust's `_mm_add_ps` and `_mm_mul_ps` are LLVM's commutative `fadd` and `fmul`, so they don't
/// fix which NaN comes out where two meet, as x86's `addps` and C++'s intrinsics do. A renderer
/// therefore keeps its exact lanes, and sends a block of pixels to its fast path only when
/// every value in the block is finite. The renderer's own coefficients must be finite too.
/// Then the only NaN its arithmetic can make is x86's default NaN, and which of two default
/// NaNs comes out doesn't change a bit.
///
/// A block holding a NaN or an infinity takes the exact lanes.
#[cfg(target_arch = "x86_64")]
pub mod blocks {
    use core::arch::x86_64::*;

    /// The pixels of a block. One special value sends all of its block to the exact lanes.
    pub const BLOCK_PIXELS: usize = 32;

    /// Aligns the code that follows to 32 bytes. A loop of a few cycles a pixel that crosses a
    /// boundary of the CPU's fetch windows, which changes from build to build, can take a third
    /// longer. It runs once before such a loop.
    #[inline(always)]
    pub fn align_loop() {
        // SAFETY: the directive only pads the code with no-ops.
        unsafe { core::arch::asm!(".p2align 5", options(nomem, nostack, preserves_flags)) };
    }

    /// A pixel as an SSE register.
    #[inline]
    pub fn load(pixel: &[f32; 4]) -> __m128 {
        // SAFETY: `pixel` holds the four floats `_mm_loadu_ps` reads, at any alignment, and
        // x86-64 has SSE.
        unsafe { _mm_loadu_ps(pixel.as_ptr()) }
    }

    /// Writes an SSE register to a pixel.
    #[inline]
    pub fn store(pixel: &mut [f32; 4], value: __m128) {
        // SAFETY: `pixel` holds the four floats `_mm_storeu_ps` writes, at any alignment, and
        // x86-64 has SSE.
        unsafe { _mm_storeu_ps(pixel.as_mut_ptr(), value) }
    }

    /// Writes red, green and blue of a register to a pixel. Alpha isn't written.
    #[inline]
    pub fn store_rgb(pixel: &mut [f32; 4], value: __m128) {
        let mut lanes = [0.0f32; 4];
        // SAFETY: `lanes` holds the four floats this writes, at any alignment, and x86-64 has SSE.
        unsafe { _mm_storeu_ps(lanes.as_mut_ptr(), value) };
        pixel[..3].copy_from_slice(&lanes[..3]);
    }

    /// Whether every value of `pixels` is finite: its exponent bits aren't all ones.
    #[inline]
    pub fn block_is_finite(pixels: &[[f32; 4]]) -> bool {
        // A few pixels, such as `applyRGBA`'s one, don't pay for the wider registers.
        if pixels.len() >= 4 && std::arch::is_x86_feature_detected!("avx2") {
            // SAFETY: the CPU has AVX2.
            return unsafe { crate::avx2::block_is_finite(pixels) };
        }
        block_is_finite_sse2(pixels)
    }

    /// [`block_is_finite`] for every x86-64 CPU, one pixel at a time.
    #[inline]
    pub fn block_is_finite_sse2(pixels: &[[f32; 4]]) -> bool {
        #[inline]
        #[target_feature(enable = "sse2")]
        fn check(pixels: &[[f32; 4]]) -> bool {
            // The exponent is in the upper 16 bits of each value, where 0x7f80 is the largest
            // signed value the mask leaves, so one maximum keeps it (the lower 16 are 0).
            let exponent = _mm_set1_epi32(0x7f80_0000);
            let mut largest = _mm_setzero_si128();
            for pixel in pixels {
                let bits = _mm_and_si128(_mm_castps_si128(load(pixel)), exponent);
                largest = _mm_max_epi16(largest, bits);
            }
            _mm_movemask_epi8(_mm_cmpeq_epi32(largest, exponent)) == 0
        }
        // SAFETY: x86-64 has SSE2.
        unsafe { check(pixels) }
    }

    /// The OR of the first three values of every pixel of `pixels`.
    pub fn rgb_bits(pixels: &[[u16; 4]]) -> u16 {
        #[target_feature(enable = "sse2")]
        fn scan(pixels: &[[u16; 4]]) -> u16 {
            let mut bits = _mm_setzero_si128();
            for pixel in pixels {
                // SAFETY: `pixel` holds the 8 bytes this reads, at any alignment.
                let value = unsafe { _mm_loadl_epi64(pixel.as_ptr().cast()) };
                bits = _mm_or_si128(bits, value);
            }
            let rgba = _mm_cvtsi128_si64(bits) as u64;
            (rgba | (rgba >> 16) | (rgba >> 32)) as u16
        }
        // SAFETY: x86-64 has SSE2.
        unsafe { scan(pixels) }
    }

    /// A pixel of a loop between two buffers, for a kernel written for a pixel in place.
    #[derive(Debug)]
    pub struct Pair<'a> {
        /// The input.
        pub from: &'a [f32; 4],
        /// The output.
        pub to: &'a mut [f32; 4],
    }

    impl Pair<'_> {
        /// The input as a register.
        #[inline]
        pub fn load(&self) -> __m128 {
            load(self.from)
        }

        /// Writes red, green and blue of `value`, and the input's alpha, a finite value whose
        /// copy has nothing to quiet.
        #[inline]
        pub fn store_rgb(&mut self, value: __m128) {
            store_rgb(self.to, value);
            self.to[3] = self.from[3];
        }
    }

    /// [`for_each_block`] from `input` to `output`, the same number of pixels: `fast` renders a
    /// finite block from one to the other, and `exact` a copy of another block, in place. The
    /// C++ renders the first op of a chain between two rows, without that copy.
    #[inline]
    pub fn for_each_block_to(
        input: &[[f32; 4]],
        output: &mut [[f32; 4]],
        mut fast: impl FnMut(&[[f32; 4]], &mut [[f32; 4]]),
        mut exact: impl FnMut(&mut [[f32; 4]], usize),
    ) {
        let blocks = input
            .chunks(BLOCK_PIXELS)
            .zip(output.chunks_mut(BLOCK_PIXELS));
        for (index, (from, to)) in blocks.enumerate() {
            if block_is_finite(from) {
                fast(from, to);
            } else {
                to.copy_from_slice(from);
                exact(to, index * BLOCK_PIXELS);
            }
        }
    }

    /// Cuts `pixels` into blocks: `fast` renders the finite ones, `exact` the others, with the
    /// index in `pixels` of the block's first pixel.
    #[inline]
    pub fn for_each_block(
        pixels: &mut [[f32; 4]],
        mut fast: impl FnMut(&mut [[f32; 4]]),
        mut exact: impl FnMut(&mut [[f32; 4]], usize),
    ) {
        for (index, block) in pixels.chunks_mut(BLOCK_PIXELS).enumerate() {
            if block_is_finite(block) {
                fast(block);
            } else {
                exact(block, index * BLOCK_PIXELS);
            }
        }
    }
}

/// The real SSE2 form of `BitDepthCast<BIT_DEPTH_F32, outBD>` for the integer depths: the values
/// of `Converter<BD>::CastValue` (src/OpenColorIO/BitDepthUtils.h:95-139 @ v2.5.2), four at a
/// time.
///
/// `CastValue` adds 0.5, clamps with two comparisons and truncates. `_mm_max_ps(v, 0)` and
/// `_mm_min_ps(v, max)` give the same integer: a NaN stays a NaN in `CastValue`, whose 32-bit
/// conversion gives `0x80000000`, of which the low bits kept are 0, and it becomes 0 here. A
/// value has no other NaN to meet, so no operand order shows.
#[cfg(target_arch = "x86_64")]
pub mod casts {
    use core::arch::x86_64::*;

    /// `(int)CLAMP(value * scale + 0.5f, 0.0f, max)` on four values.
    #[inline]
    #[target_feature(enable = "sse2")]
    fn cast(value: __m128, scale: __m128, max: __m128) -> __m128i {
        let v = _mm_add_ps(_mm_mul_ps(value, scale), _mm_set1_ps(0.5));
        _mm_cvttps_epi32(_mm_min_ps(_mm_max_ps(v, _mm_setzero_ps()), max))
    }

    /// The whole groups of sixteen values of `input`, cast and stored in `output`: the number of
    /// values done.
    #[target_feature(enable = "sse2")]
    fn to_u8(input: &[f32], output: &mut [u8], scale: f32) -> usize {
        let (scale, max) = (_mm_set1_ps(scale), _mm_set1_ps(255.0));
        let (groups, _) = input.as_chunks::<16>();
        let (outputs, _) = output.as_chunks_mut::<16>();
        for (group, out) in groups.iter().zip(outputs) {
            // SAFETY: `group` holds the sixteen floats these read, at any alignment.
            let v: [__m128; 4] =
                std::array::from_fn(|i| unsafe { _mm_loadu_ps(group.as_ptr().add(4 * i)) });
            let [a, b, c, d] = v.map(|v| cast(v, scale, max));
            let packed = _mm_packus_epi16(_mm_packs_epi32(a, b), _mm_packs_epi32(c, d));
            // SAFETY: `out` holds the sixteen bytes this writes, at any alignment.
            unsafe { _mm_storeu_si128(out.as_mut_ptr().cast(), packed) };
        }
        groups.len() * 16
    }

    /// As [`to_u8`], to 16 bits, for a `max_value` of at most 65535.
    #[target_feature(enable = "sse2")]
    fn to_u16(input: &[f32], output: &mut [u16], scale: f32, max_value: f32) -> usize {
        let (scale, max) = (_mm_set1_ps(scale), _mm_set1_ps(max_value));
        let (groups, _) = input.as_chunks::<16>();
        let (outputs, _) = output.as_chunks_mut::<16>();
        for (group, out) in groups.iter().zip(outputs) {
            // SAFETY: `group` holds the sixteen floats these read, at any alignment.
            let v: [__m128; 4] =
                std::array::from_fn(|i| unsafe { _mm_loadu_ps(group.as_ptr().add(4 * i)) });
            // The low 16 bits as signed values, as `_mm_packs_epi32` keeps those whole.
            let [a, b, c, d] =
                v.map(|v| _mm_srai_epi32::<16>(_mm_slli_epi32::<16>(cast(v, scale, max))));
            // SAFETY: `out` holds the sixteen values these write, at any alignment.
            unsafe {
                _mm_storeu_si128(out.as_mut_ptr().cast(), _mm_packs_epi32(a, b));
                _mm_storeu_si128(out.as_mut_ptr().add(8).cast(), _mm_packs_epi32(c, d));
            }
        }
        groups.len() * 16
    }

    /// [`to_u8`] for every x86-64 CPU.
    pub fn cast_to_u8(input: &[f32], output: &mut [u8], scale: f32) -> usize {
        // SAFETY: x86-64 has SSE2.
        unsafe { to_u8(input, output, scale) }
    }

    /// [`to_u16`] for every x86-64 CPU.
    pub fn cast_to_u16(input: &[f32], output: &mut [u16], scale: f32, max_value: f32) -> usize {
        // SAFETY: x86-64 has SSE2.
        unsafe { to_u16(input, output, scale, max_value) }
    }
}

/// The real `SSE2RGBAPack`s: four pixels, sixteen values. The kernels only load F32.
#[cfg(target_arch = "x86_64")]
pub(crate) mod packs {
    use core::arch::x86_64::*;

    use super::intrinsics::sse2_cvtps_ph;

    /// Port of `sse2_clamp` (src/OpenColorIO/SSE2.h:85-89 @ v2.5.2).
    #[inline]
    #[target_feature(enable = "sse2")]
    fn clamp(value: __m128, max_value: __m128) -> __m128 {
        _mm_min_ps(_mm_max_ps(value, _mm_setzero_ps()), max_value)
    }

    /// Port of `sse2RGBATranspose_4x4` (src/OpenColorIO/SSE2.h:91-104 @ v2.5.2).
    #[inline]
    #[target_feature(enable = "sse2")]
    pub(crate) fn transpose(rows: [__m128; 4]) -> [__m128; 4] {
        let tmp0 = _mm_unpacklo_ps(rows[0], rows[1]);
        let tmp2 = _mm_unpacklo_ps(rows[2], rows[3]);
        let tmp1 = _mm_unpackhi_ps(rows[0], rows[1]);
        let tmp3 = _mm_unpackhi_ps(rows[2], rows[3]);
        [
            _mm_movelh_ps(tmp0, tmp2),
            _mm_movehl_ps(tmp2, tmp0), // Note movhlps swaps b with a which is different than unpckhpd
            _mm_movelh_ps(tmp1, tmp3),
            _mm_movehl_ps(tmp3, tmp1),
        ]
    }

    /// Port of `SSE2RGBAPack<BIT_DEPTH_F32>::Load` (src/OpenColorIO/SSE2.h:375-384 @ v2.5.2).
    #[inline]
    #[target_feature(enable = "sse2")]
    pub(crate) fn load_f32(input: &[f32; 16]) -> [__m128; 4] {
        // SAFETY: `input` holds the sixteen floats these read, at any alignment.
        transpose(std::array::from_fn(|i| unsafe {
            _mm_loadu_ps(input.as_ptr().add(4 * i))
        }))
    }

    /// Port of `SSE2RGBAPack<BIT_DEPTH_F32>::Store` (src/OpenColorIO/SSE2.h:386-397 @ v2.5.2).
    #[inline]
    #[target_feature(enable = "sse2")]
    pub(crate) fn store_f32(out: &mut [f32; 16], rgba: [__m128; 4]) {
        for (i, row) in transpose(rgba).into_iter().enumerate() {
            // SAFETY: `out` holds the sixteen floats these write, at any alignment.
            unsafe { _mm_storeu_ps(out.as_mut_ptr().add(4 * i), row) };
        }
    }

    /// Port of `SSE2RGBAPack<BIT_DEPTH_UINT8>::Store` (src/OpenColorIO/SSE2.h:222-238 @ v2.5.2).
    #[inline]
    #[target_feature(enable = "sse2")]
    pub(crate) fn store_u8(out: &mut [u8; 16], [r, g, b, a]: [__m128; 4]) {
        let max_value = _mm_set1_ps(255.0);

        // NOTE note using cvtps which will round based on MXCSR register defaults to _MM_ROUND_NEAREST
        let r = _mm_cvtps_epi32(clamp(r, max_value));
        let g = _mm_cvtps_epi32(clamp(g, max_value));
        let b = _mm_cvtps_epi32(clamp(b, max_value));
        let a = _mm_cvtps_epi32(clamp(a, max_value));

        let rgba = _mm_or_si128(r, _mm_slli_si128::<1>(g));
        let rgba = _mm_or_si128(rgba, _mm_slli_si128::<2>(b));
        let rgba = _mm_or_si128(rgba, _mm_slli_si128::<3>(a));

        // SAFETY: `out` holds the sixteen bytes this writes, at any alignment.
        unsafe { _mm_storeu_si128(out.as_mut_ptr().cast(), rgba) };
    }

    /// Port of `SSE2RGBAPack16<BD>::Store` (src/OpenColorIO/SSE2.h:260-278 @ v2.5.2), where
    /// `max_value` is `BitDepthInfo<BD>::maxValue`.
    #[inline]
    #[target_feature(enable = "sse2")]
    pub(crate) fn store_u16(out: &mut [u16; 16], [r, g, b, a]: [__m128; 4], max_value: f32) {
        let max_value = _mm_set1_ps(max_value);

        let rrrr = _mm_cvtps_epi32(clamp(r, max_value));
        let gggg = _mm_cvtps_epi32(clamp(g, max_value));
        let bbbb = _mm_cvtps_epi32(clamp(b, max_value));
        let aaaa = _mm_cvtps_epi32(clamp(a, max_value));

        let rgrg_rgrg = _mm_or_si128(rrrr, _mm_slli_si128::<2>(gggg));
        let baba_baba = _mm_or_si128(bbbb, _mm_slli_si128::<2>(aaaa));

        let rgba_00_01 = _mm_unpacklo_epi32(rgrg_rgrg, baba_baba);
        let rgba_02_03 = _mm_unpackhi_epi32(rgrg_rgrg, baba_baba);

        // SAFETY: `out` holds the sixteen values these write, at any alignment.
        unsafe {
            _mm_storeu_si128(out.as_mut_ptr().cast(), rgba_00_01);
            _mm_storeu_si128(out.as_mut_ptr().add(8).cast(), rgba_02_03);
        }
    }

    /// Port of `SSE2RGBAPack<BIT_DEPTH_F16>::Store` (src/OpenColorIO/SSE2.h:344-370 @ v2.5.2),
    /// its branch without SSE2NEON.
    #[inline]
    #[target_feature(enable = "sse2")]
    pub(crate) fn store_f16(out: &mut [half::f16; 16], rgba: [__m128; 4]) {
        let [rgba0, rgba1, rgba2, rgba3] = transpose(rgba);

        let rgba00_01 = sse2_cvtps_ph(rgba0);
        let rgba02_03 = sse2_cvtps_ph(rgba1);
        let rgba04_05 = sse2_cvtps_ph(rgba2);
        let rgba06_07 = sse2_cvtps_ph(rgba3);

        let low = _mm_xor_si128(rgba00_01, _mm_shuffle_epi32::<0b01_00_11_10>(rgba02_03));
        let high = _mm_xor_si128(rgba04_05, _mm_shuffle_epi32::<0b01_00_11_10>(rgba06_07));

        // SAFETY: `out` holds the sixteen values these write, at any alignment.
        unsafe {
            _mm_storeu_si128(out.as_mut_ptr().cast(), low);
            _mm_storeu_si128(out.as_mut_ptr().add(8).cast(), high);
        }
    }
}

/// The same functions written with SSE2 intrinsics, instruction for instruction as in
/// `SSE2.h`. They exist to prove the scalar lanes above against the hardware, and the real
/// pack of halves ([`packs`]) uses them.
#[cfg(target_arch = "x86_64")]
pub mod intrinsics {
    use core::arch::x86_64::*;

    /// Port of `sse2_blendv` (src/OpenColorIO/SSE2.h:106-109 @ v2.5.2).
    #[inline]
    #[target_feature(enable = "sse2")]
    fn sse2_blendv(a: __m128i, b: __m128i, mask: __m128i) -> __m128i {
        _mm_xor_si128(_mm_and_si128(_mm_xor_si128(a, b), mask), a)
    }

    /// Port of `sse2_cvtps_ph` (src/OpenColorIO/SSE2.h:111-153 @ v2.5.2).
    #[inline]
    #[target_feature(enable = "sse2")]
    pub(crate) fn sse2_cvtps_ph(a: __m128) -> __m128i {
        let mut x = _mm_castps_si128(a);

        let x_sgn = _mm_and_si128(x, _mm_set1_epi32(0x8000_0000u32 as i32));
        let mut x_exp = _mm_and_si128(x, _mm_set1_epi32(0x7f80_0000));

        let magic1 = _mm_castsi128_ps(_mm_set1_epi32(0x7780_0000)); // 0x1.0p+112f
        let magic2 = _mm_castsi128_ps(_mm_set1_epi32(0x0880_0000)); // 0x1.0p-110f

        // sse2 doesn't have _mm_max_epu32, but _mm_max_ps works.
        let exp_max = _mm_set1_epi32(0x3880_0000);
        x_exp = _mm_castps_si128(_mm_max_ps(
            _mm_castsi128_ps(x_exp),
            _mm_castsi128_ps(exp_max),
        )); // max(e, -14)
        x_exp = _mm_add_epi32(x_exp, _mm_set1_epi32(15 << 23)); // e += 15
        x = _mm_and_si128(x, _mm_set1_epi32(0x7fff_ffff)); // Discard sign

        let mut f = _mm_castsi128_ps(x);
        let magicf = _mm_castsi128_ps(x_exp);

        // If 15 < e then inf, otherwise e += 2
        f = _mm_mul_ps(_mm_mul_ps(f, magic1), magic2);
        f = _mm_add_ps(f, magicf);

        let u = _mm_castps_si128(f);

        let h_exp = _mm_and_si128(_mm_srli_epi32::<13>(u), _mm_set1_epi32(0x7c00));
        let mut h_sig = _mm_and_si128(u, _mm_set1_epi32(0x0fff));

        // blend in nan values only if present
        let nan_mask = _mm_cmpgt_epi32(x, _mm_set1_epi32(0x7f80_0000));
        if _mm_movemask_epi8(nan_mask) != 0 {
            let mut nan = _mm_and_si128(_mm_srli_epi32::<13>(x), _mm_set1_epi32(0x03ff));
            nan = _mm_or_si128(_mm_set1_epi32(0x0200), nan);
            h_sig = sse2_blendv(h_sig, nan, nan_mask);
        }

        let mut ph = _mm_add_epi32(_mm_srli_epi32::<16>(x_sgn), _mm_add_epi32(h_exp, h_sig));

        // pack u16 values into lower 64 bits
        ph = _mm_shufflehi_epi16::<{ 1 << 6 | 1 << 4 | 2 << 2 }>(ph);
        ph = _mm_shufflelo_epi16::<{ 1 << 6 | 1 << 4 | 2 << 2 }>(ph);
        _mm_shuffle_epi32::<{ 3 << 6 | 3 << 4 | 2 << 2 }>(ph)
    }

    /// Port of `sse2_cvtph_ps` (src/OpenColorIO/SSE2.h:155-192 @ v2.5.2).
    #[target_feature(enable = "sse2")]
    fn sse2_cvtph_ps(a: __m128i) -> __m128 {
        let magic = _mm_castsi128_ps(_mm_set1_epi32((254 - 15) << 23));
        let was_infnan = _mm_castsi128_ps(_mm_set1_epi32((127 + 16) << 23));

        // the values to unpack are in the lower 64 bits
        let a = _mm_unpacklo_epi16(a, a);

        // extract sign
        let sign = _mm_castsi128_ps(_mm_slli_epi32::<16>(_mm_and_si128(
            a,
            _mm_set1_epi32(0x8000),
        )));

        // extract exponent/mantissa bits
        let mut o = _mm_castsi128_ps(_mm_slli_epi32::<13>(_mm_and_si128(
            a,
            _mm_set1_epi32(0x7fff),
        )));

        // magic multiply
        o = _mm_mul_ps(o, magic);

        // blend in inf/nan values only if present
        let mut mask = _mm_castps_si128(_mm_cmpge_ps(o, was_infnan));
        if _mm_movemask_epi8(mask) != 0 {
            let mut ou = _mm_castps_si128(o);
            let ou_nan = _mm_or_si128(ou, _mm_set1_epi32(0x01ff << 22));
            let ou_inf = _mm_or_si128(ou, _mm_set1_epi32(0x00ff << 23));

            // blend in nans
            ou = sse2_blendv(ou, ou_nan, mask);

            // blend in infinities
            mask = _mm_cmpeq_epi32(_mm_castps_si128(o), _mm_castps_si128(was_infnan));
            o = _mm_castsi128_ps(sse2_blendv(ou, ou_inf, mask));
        }

        _mm_or_ps(o, sign)
    }

    /// [`sse2_cvtps_ph`] on four floats: the four halves in the low 64 bits, in order.
    pub fn cvtps_ph_x4(values: [f32; 4]) -> [u16; 4] {
        // SAFETY: SSE2 is part of the x86-64 baseline.
        unsafe { cvtps_ph_x4_sse2(values) }
    }

    #[target_feature(enable = "sse2")]
    fn cvtps_ph_x4_sse2(values: [f32; 4]) -> [u16; 4] {
        let ph = sse2_cvtps_ph(_mm_setr_ps(values[0], values[1], values[2], values[3]));
        let low = _mm_cvtsi128_si64(ph) as u64;
        [
            low as u16,
            (low >> 16) as u16,
            (low >> 32) as u16,
            (low >> 48) as u16,
        ]
    }

    /// [`sse2_cvtph_ps`] on four halves.
    pub fn cvtph_ps_x4(halves: [u16; 4]) -> [f32; 4] {
        // SAFETY: SSE2 is part of the x86-64 baseline.
        unsafe { cvtph_ps_x4_sse2(halves) }
    }

    #[target_feature(enable = "sse2")]
    fn cvtph_ps_x4_sse2(halves: [u16; 4]) -> [f32; 4] {
        let low = u64::from(halves[0])
            | u64::from(halves[1]) << 16
            | u64::from(halves[2]) << 32
            | u64::from(halves[3]) << 48;
        let o = sse2_cvtph_ps(_mm_cvtsi64_si128(low as i64));
        let mut out = [0f32; 4];
        // SAFETY: `out` has room for the four floats `_mm_storeu_ps` writes.
        unsafe { _mm_storeu_ps(out.as_mut_ptr(), o) };
        out
    }
}

#[cfg(test)]
#[path = "sse2_tests.rs"]
pub(crate) mod tests;
