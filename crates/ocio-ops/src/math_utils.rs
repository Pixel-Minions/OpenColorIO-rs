// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Scalar helpers with the exact semantics of the C++ that OpenColorIO 2.5.2 is built from.
//!
//! - C++ `std::min`, `std::max` and OCIO's `Clamp` (`MathUtils.h`). They treat NaN
//!   differently from Rust's `f32::min`/`f32::max`: the comparison is false, so the other
//!   operand is returned.
//! - One lane of the SSE min/max and float→int conversions that OCIO's SIMD code uses.
//!   `_mm_max_ps`/`_mm_min_ps` return the *second* operand when either is NaN, and the
//!   conversions return the "integer indefinite" value `0x80000000` for NaN and out-of-range
//!   inputs.
//! - The scalar float→integer casts of `BitDepthUtils.h`: add 0.5, clamp, truncate.
//! - The bit helpers of `MathUtils.h` (`FloatAsInt`, `IntAsFloat`, `AddULP`).
//!
//! Port of parts of `src/OpenColorIO/MathUtils.h` and `src/OpenColorIO/BitDepthUtils.h`
//! @ v2.5.2.

/// C++ `std::min(a, b)`: `(b < a) ? b : a`. Returns `a` when the comparison is false, so a
/// NaN in `a` is returned and a NaN in `b` is not.
///
/// Port of `std::min` as libstdc++ and the MSVC STL implement it (both return
/// `(b < a) ? b : a`), used throughout OCIO 2.5.2.
#[inline]
pub fn std_min<T: PartialOrd>(a: T, b: T) -> T {
    if b < a { b } else { a }
}

/// C++ `std::max(a, b)`: `(a < b) ? b : a`. Returns `a` when the comparison is false, so a
/// NaN in `a` is returned and a NaN in `b` is not.
///
/// Port of `std::max` as libstdc++ and the MSVC STL implement it (both return
/// `(a < b) ? b : a`), used throughout OCIO 2.5.2.
#[inline]
pub fn std_max<T: PartialOrd>(a: T, b: T) -> T {
    if a < b { b } else { a }
}

/// OCIO's `Clamp(a, min, max)`: `std::min(std::max(min, a), max)`. NaN becomes `min`; `max`
/// is not validated against `min`.
///
/// Port of `Clamp` (src/OpenColorIO/MathUtils.h:58-67 @ v2.5.2).
#[inline]
pub fn clamp<T: PartialOrd>(a: T, min: T, max: T) -> T {
    std_min(std_max(min, a), max)
}

/// One lane of `_mm_max_ps(a, b)` (x86 `MAXPS`): `a > b ? a : b`. When either value is NaN,
/// or both are zeros of either sign, the second operand `b` is returned.
///
/// OCIO relies on this to filter a NaN in the first operand (`src/OpenColorIO/SSE.h:46-55`
/// @ v2.5.2).
#[inline]
pub fn sse_max(a: f32, b: f32) -> f32 {
    if a > b { a } else { b }
}

/// One lane of `_mm_min_ps(a, b)` (x86 `MINPS`): `a < b ? a : b`. When either value is NaN,
/// or both are zeros of either sign, the second operand `b` is returned.
#[inline]
pub fn sse_min(a: f32, b: f32) -> f32 {
    if a < b { a } else { b }
}

/// The value x86 float→int32 conversions return for NaN and out-of-range inputs (Intel's
/// "integer indefinite").
pub const INTEGER_INDEFINITE: i32 = i32::MIN;

/// One lane of `_mm_cvttps_epi32` (x86 `CVTTPS2DQ`): truncation toward zero. NaN and values
/// outside the `i32` range give [`INTEGER_INDEFINITE`].
///
/// Rust's `as i32` saturates and maps NaN to 0, so it cannot be used directly.
#[inline]
pub fn sse_cvttps_epi32(x: f32) -> i32 {
    // Every f32 in [-2^31, 2^31) truncates to a representable i32.
    if (-2_147_483_648.0..2_147_483_648.0).contains(&x) {
        x as i32
    } else {
        INTEGER_INDEFINITE
    }
}

/// One lane of `_mm_cvtps_epi32` (x86 `CVTPS2DQ`) with the default MXCSR rounding mode:
/// round to nearest, ties to even. NaN and values outside the `i32` range give
/// [`INTEGER_INDEFINITE`].
#[inline]
pub fn sse_cvtps_epi32(x: f32) -> i32 {
    // Every f32 in [-2^31, 2^31) rounds to a representable i32: the f32 values nearest to
    // 2^31 are already integers.
    if (-2_147_483_648.0..2_147_483_648.0).contains(&x) {
        x.round_ties_even() as i32
    } else {
        INTEGER_INDEFINITE
    }
}

/// The shared body of the integer `Converter<BD>::CastValue` specializations:
/// `v = value + 0.5f`, then `CLAMP(v, 0.0f, maxValue)` (whose result is a float, since the
/// comparisons promote the unsigned `maxValue` to float), then the cast to the integer type.
///
/// The cast truncates toward zero. A NaN reaches the cast unclamped (both `CLAMP`
/// comparisons are false); converting it is undefined behaviour in C++, and x86-64 compilers
/// emit a 32-bit `cvttss2si`, which gives [`INTEGER_INDEFINITE`]. The callers keep its low
/// 8 or 16 bits, which are 0.
///
/// Port of `CLAMP` and the `CastValue` bodies (src/OpenColorIO/BitDepthUtils.h:79-140
/// @ v2.5.2).
#[inline]
fn cast_value_uint(value: f32, max_value: u16) -> i32 {
    let v = value + 0.5f32;
    let max = f32::from(max_value);
    let clamped = if v > max {
        max
    } else if 0.0f32 > v {
        0.0f32
    } else {
        v
    };
    sse_cvttps_epi32(clamped)
}

/// `Converter<BIT_DEPTH_UINT8>::CastValue`: add 0.5, clamp to [0, 255], truncate.
///
/// Port of `Converter<BIT_DEPTH_UINT8>::CastValue` (src/OpenColorIO/BitDepthUtils.h:90-101
/// @ v2.5.2).
#[inline]
pub fn cast_value_uint8(value: f32) -> u8 {
    cast_value_uint(value, 255) as u8
}

/// `Converter<BIT_DEPTH_UINT10>::CastValue`: add 0.5, clamp to [0, 1023], truncate.
///
/// Port of `Converter<BIT_DEPTH_UINT10>::CastValue` (src/OpenColorIO/BitDepthUtils.h:103-114
/// @ v2.5.2).
#[inline]
pub fn cast_value_uint10(value: f32) -> u16 {
    cast_value_uint(value, 1023) as u16
}

/// `Converter<BIT_DEPTH_UINT12>::CastValue`: add 0.5, clamp to [0, 4095], truncate.
///
/// Port of `Converter<BIT_DEPTH_UINT12>::CastValue` (src/OpenColorIO/BitDepthUtils.h:116-127
/// @ v2.5.2).
#[inline]
pub fn cast_value_uint12(value: f32) -> u16 {
    cast_value_uint(value, 4095) as u16
}

/// `Converter<BIT_DEPTH_UINT16>::CastValue`: add 0.5, clamp to [0, 65535], truncate.
///
/// Port of `Converter<BIT_DEPTH_UINT16>::CastValue` (src/OpenColorIO/BitDepthUtils.h:129-140
/// @ v2.5.2).
#[inline]
pub fn cast_value_uint16(value: f32) -> u16 {
    cast_value_uint(value, 65535) as u16
}

/// The bits of a float, as an unsigned integer.
///
/// Port of `FloatAsInt` (src/OpenColorIO/MathUtils.h:149-164 @ v2.5.2).
#[inline]
pub fn float_as_int(x: f32) -> u32 {
    x.to_bits()
}

/// The float with the given bits.
///
/// Port of `IntAsFloat` (src/OpenColorIO/MathUtils.h:166-181 @ v2.5.2).
#[inline]
pub fn int_as_float(x: u32) -> f32 {
    f32::from_bits(x)
}

/// Adds `ulp` to the bits of `f` (so the sign is ignored: a positive `ulp` moves a negative
/// float away from zero). The unsigned addition wraps, as in C++.
///
/// Port of `AddULP` (src/OpenColorIO/MathUtils.h:183-193 @ v2.5.2).
#[inline]
pub fn add_ulp(f: f32, ulp: i32) -> f32 {
    int_as_float(float_as_int(f).wrapping_add_signed(ulp))
}

#[cfg(test)]
#[path = "math_utils_tests.rs"]
mod tests;
