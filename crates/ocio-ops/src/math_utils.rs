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
//! - x86 addition and multiplication with a fixed operand order ([`sse_add`], [`sse_mul`]):
//!   when both operands are NaN, the result is the *first* one.
//! - The scalar float→integer casts of `BitDepthUtils.h`: add 0.5, clamp, truncate.
//! - The bit helpers of `MathUtils.h` (`FloatAsInt`, `IntAsFloat`, `AddULP`).
//! - The other scalar helpers of `MathUtils.h` and `MathUtils.cpp`: the tolerance tests
//!   (`EqualWithAbsError`, `FloatsDiffer`, `IsScalarEqualToZero`, ...), `SanitizeFloat`,
//!   `lerpf` and the half helpers.
//!
//! Port of parts of `src/OpenColorIO/MathUtils.h` and `src/OpenColorIO/BitDepthUtils.h`
//! @ v2.5.2.

use std::ops::{Add, Mul};

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

/// The float types of x86 SSE arithmetic: `f32` (`ADDSS`, `ADDPS`, `MULSS`, `MULPS`) and `f64`
/// (`ADDSD`, `MULSD`), which x86-64 compilers also emit for scalar `float` and `double` code.
pub trait SseFloat: Copy + Add<Output = Self> + Mul<Output = Self> {
    /// Whether the value is a NaN.
    fn is_nan(self) -> bool;
    /// The value with the quiet bit set: what the instructions return for a NaN operand.
    fn quieted(self) -> Self;
}

impl SseFloat for f32 {
    #[inline]
    fn is_nan(self) -> bool {
        f32::is_nan(self)
    }
    #[inline]
    fn quieted(self) -> f32 {
        f32::from_bits(self.to_bits() | 0x0040_0000)
    }
}

impl SseFloat for f64 {
    #[inline]
    fn is_nan(self) -> bool {
        f64::is_nan(self)
    }
    #[inline]
    fn quieted(self) -> f64 {
        f64::from_bits(self.to_bits() | 0x0008_0000_0000_0000)
    }
}

/// `a + b` as x86 computes it with `a` as the first source operand: one lane of
/// `_mm_add_ps(a, b)` (`ADDPS`), or the `ADDSS`/`ADDSD` of a scalar C++ `a + b`. When both
/// values are NaN, the result is `a`, quieted; when only one is, it is that one, quieted
/// (Intel SDM Vol. 1, 4.8.3.5, Table 4-7).
///
/// Rust's `a + b` leaves the choice between two NaN operands to LLVM, which may swap the
/// operands of an addition, and does so differently in debug and release builds. The port
/// writes the operand order of upstream's source with this function wherever two NaNs can
/// meet. Subtraction and division need no helper: their operands cannot be swapped.
#[inline]
pub fn sse_add<T: SseFloat>(a: T, b: T) -> T {
    if a.is_nan() { a.quieted() } else { a + b }
}

/// `a * b` as x86 computes it with `a` as the first source operand: one lane of
/// `_mm_mul_ps(a, b)` (`MULPS`), or the `MULSS`/`MULSD` of a scalar C++ `a * b`. The NaN
/// rules and the reason for this function are those of [`sse_add`].
#[inline]
pub fn sse_mul<T: SseFloat>(a: T, b: T) -> T {
    if a.is_nan() { a.quieted() } else { a * b }
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

/// The float types `MathUtils`' templates are instantiated with: `float` and `double`.
pub trait MathFloat:
    SseFloat + PartialOrd + std::ops::Sub<Output = Self> + std::ops::Neg<Output = Self>
{
    /// `0` of the type.
    const ZERO: Self;
    /// `(float)v`: the value converted to `float`, rounded to nearest (an `f32` is unchanged).
    fn to_f32(self) -> f32;
}

impl MathFloat for f32 {
    const ZERO: f32 = 0.0;
    #[inline]
    fn to_f32(self) -> f32 {
        self
    }
}

impl MathFloat for f64 {
    const ZERO: f64 = 0.0;
    #[inline]
    fn to_f32(self) -> f32 {
        self as f32
    }
}

/// `IsNan(val)`: `std::isnan`.
///
/// Port of `IsNan` (src/OpenColorIO/MathUtils.h:17-18 @ v2.5.2).
#[inline]
pub fn is_nan<T: SseFloat>(val: T) -> bool {
    val.is_nan()
}

/// `EqualWithAbsError(x1, x2, e)`: `((x1 > x2) ? x1 - x2 : x2 - x1) <= e`. False when a value
/// is NaN.
///
/// Port of `EqualWithAbsError` (src/OpenColorIO/MathUtils.h:41-45 @ v2.5.2).
#[inline]
pub fn equal_with_abs_error<T: MathFloat>(x1: T, x2: T, e: T) -> bool {
    (if x1 > x2 { x1 - x2 } else { x2 - x1 }) <= e
}

/// `EqualWithRelError(x1, x2, e)`: `((x1 > x2) ? x1 - x2 : x2 - x1) <= e * ((x1 > 0) ? x1 :
/// -x1)`, the error relative to `x1` alone. False when a value is NaN.
///
/// Port of `EqualWithRelError` (src/OpenColorIO/MathUtils.h:47-51 @ v2.5.2).
#[inline]
pub fn equal_with_rel_error<T: MathFloat>(x1: T, x2: T, e: T) -> bool {
    (if x1 > x2 { x1 - x2 } else { x2 - x1 }) <= e * (if x1 > T::ZERO { x1 } else { -x1 })
}

/// `lerpf(a, b, z)`: `(b - a) * z + a` in `float`, unfused.
///
/// The C++ is inline, so the compiled code of each caller decides the operand order where two
/// NaNs meet, and the port of each caller checks its own. This function keeps the source's
/// order.
///
/// Port of `lerpf` (src/OpenColorIO/MathUtils.h:53-56 @ v2.5.2).
#[inline]
pub fn lerpf(a: f32, b: f32, z: f32) -> f32 {
    sse_add(sse_mul(b - a, z), a)
}

/// `SanitizeFloat(f)`: -Inf becomes `-FLT_MAX`, +Inf becomes `FLT_MAX`, NaN becomes 0; every
/// other value is unchanged.
///
/// Port of `SanitizeFloat` (src/OpenColorIO/MathUtils.cpp:145-160 @ v2.5.2).
#[inline]
pub fn sanitize_float(f: f32) -> f32 {
    if f == f32::NEG_INFINITY {
        -f32::MAX
    } else if f == f32::INFINITY {
        f32::MAX
    } else if f.is_nan() {
        0.0
    } else {
        f
    }
}

/// `IsScalarEqualToZero(v)`: `v`, converted to `float`, is within 2 ULPs of 0 (keeping
/// denormals): 0, -0 and the four denormals nearest to 0. A `double` is compared after its
/// conversion to `float`, so any `double` smaller than half the smallest `float` denormal
/// counts as 0.
///
/// Port of `IsScalarEqualToZero<T>` (src/OpenColorIO/MathUtils.cpp:17-27 @ v2.5.2).
#[inline]
pub fn is_scalar_equal_to_zero<T: MathFloat>(v: T) -> bool {
    !floats_differ(0.0, v.to_f32(), 2, false)
}

/// `IsScalarEqualToOne(v)`: `v`, converted to `float`, is within 2 ULPs of 1.
///
/// Port of `IsScalarEqualToOne<T>` (src/OpenColorIO/MathUtils.cpp:29-39 @ v2.5.2).
#[inline]
pub fn is_scalar_equal_to_one<T: MathFloat>(v: T) -> bool {
    !floats_differ(1.0, v.to_f32(), 2, false)
}

/// `IsVecEqualToZero(v, size)`: every value is [`is_scalar_equal_to_zero`].
///
/// Port of `IsVecEqualToZero<T>` (src/OpenColorIO/MathUtils.cpp:41-55 @ v2.5.2).
pub fn is_vec_equal_to_zero<T: MathFloat>(v: &[T]) -> bool {
    v.iter().all(|&x| is_scalar_equal_to_zero(x))
}

/// `IsVecEqualToOne(v, size)`: every value is [`is_scalar_equal_to_one`].
///
/// Port of `IsVecEqualToOne<T>` (src/OpenColorIO/MathUtils.cpp:57-71 @ v2.5.2).
pub fn is_vec_equal_to_one<T: MathFloat>(v: &[T]) -> bool {
    v.iter().all(|&x| is_scalar_equal_to_one(x))
}

/// `VecContainsZero(v, size)`: some value is [`is_scalar_equal_to_zero`].
///
/// Port of `VecContainsZero` (src/OpenColorIO/MathUtils.cpp:100-107 @ v2.5.2).
pub fn vec_contains_zero(v: &[f32]) -> bool {
    v.iter().any(|&x| is_scalar_equal_to_zero(x))
}

/// `VecContainsOne(v, size)`: some value is [`is_scalar_equal_to_one`].
///
/// Port of `VecContainsOne` (src/OpenColorIO/MathUtils.cpp:109-116 @ v2.5.2).
pub fn vec_contains_one(v: &[f32]) -> bool {
    v.iter().any(|&x| is_scalar_equal_to_one(x))
}

/// `VecsEqualWithRelError(v1, size1, v2, size2, e)`: the same length, and every pair of values
/// [`equal_with_rel_error`], relative to the value of `v1`.
///
/// Port of `VecsEqualWithRelError<T>` (src/OpenColorIO/MathUtils.cpp:73-92 @ v2.5.2).
pub fn vecs_equal_with_rel_error<T: MathFloat>(v1: &[T], v2: &[T], e: T) -> bool {
    v1.len() == v2.len()
        && v1
            .iter()
            .zip(v2)
            .all(|(&a, &b)| equal_with_rel_error(a, b, e))
}

/// `GetHalfMax()`: the largest positive half, 65504.
///
/// Port of `GetHalfMax` (src/OpenColorIO/MathUtils.h:100-103 @ v2.5.2).
#[inline]
pub fn get_half_max() -> f64 {
    65504.0
}

/// `GetHalfMin()`: upstream's literal for the smallest positive half, `5.96046448e-08`. It is
/// the `double` nearest to that decimal, not 2^-24 (`5.9604644775390625e-08`) itself.
///
/// Port of `GetHalfMin` (src/OpenColorIO/MathUtils.h:105-108 @ v2.5.2).
#[inline]
pub fn get_half_min() -> f64 {
    5.96046448e-08
}

/// `GetHalfNormMin()`: upstream's literal for the smallest positive normal half,
/// `6.10351562e-05`. It is the `double` nearest to that decimal, just below 2^-14
/// (`6.103515625e-05`).
///
/// Port of `GetHalfNormMin` (src/OpenColorIO/MathUtils.h:110-113 @ v2.5.2).
#[inline]
pub fn get_half_norm_min() -> f64 {
    6.10351562e-05
}

/// `ClampToNormHalf(val)`: values below `-GetHalfMax()` become `-GetHalfMax()`, values above
/// `GetHalfMax()` become `GetHalfMax()`, and values strictly between `-GetHalfNormMin()` and
/// `GetHalfNormMin()` become `+0.0` (`-0.0` too). NaN is returned unchanged.
///
/// Port of `ClampToNormHalf` (src/OpenColorIO/MathUtils.cpp:118-136 @ v2.5.2).
pub fn clamp_to_norm_half(val: f64) -> f64 {
    if val < -get_half_max() {
        return -get_half_max();
    }
    if val > -get_half_norm_min() && val < get_half_norm_min() {
        return 0.0;
    }
    if val > get_half_max() {
        return get_half_max();
    }
    val
}

/// `ConvertHalfBitsToFloat(val)`: the half with these bits, as a float, converted by Imath.
///
/// Port of `ConvertHalfBitsToFloat` (src/OpenColorIO/MathUtils.cpp:138-143 @ v2.5.2).
#[inline]
pub fn convert_half_bits_to_float(val: u16) -> f32 {
    crate::imath_half::half_to_float(val)
}

/// `GetSafeScalarInverse(v, defaultValue)`: `defaultValue` when `v` [is equal to
/// zero](is_scalar_equal_to_zero), else `1.0f / v`. Upstream's `defaultValue` defaults to 1.
///
/// Port of `GetSafeScalarInverse` (src/OpenColorIO/MathUtils.h:122 and
/// src/OpenColorIO/MathUtils.cpp:94-98 @ v2.5.2).
#[inline]
pub fn get_safe_scalar_inverse(v: f32, default_value: f32) -> f32 {
    if is_scalar_equal_to_zero(v) {
        return default_value;
    }
    1.0 / v
}

/// `FloatForCompare(floatBits)`: maps a float's bits to an ordered integer, keeping denormals
/// (the table at src/OpenColorIO/MathUtils.cpp:353-396). The C++ computes in `unsigned` and
/// returns `int`; its callers store the result back in an `unsigned`.
///
/// Port of `FloatForCompare` (src/OpenColorIO/MathUtils.cpp:397-400 @ v2.5.2).
#[inline]
fn float_for_compare(float_bits: u32) -> u32 {
    if float_bits < 0x8000_0000 {
        0x8000_0000u32.wrapping_add(float_bits)
    } else {
        0x8000_0000u32.wrapping_sub(float_bits & 0x7FFF_FFFF)
    }
}

/// `FloatForCompareCompressDenorms(floatBits)`: maps a float's bits to an ordered integer,
/// with every denormal equal to zero (the table at src/OpenColorIO/MathUtils.cpp:402-432).
///
/// Port of `FloatForCompareCompressDenorms` (src/OpenColorIO/MathUtils.cpp:433-444 @ v2.5.2).
#[inline]
fn float_for_compare_compress_denorms(float_bits: u32) -> u32 {
    let absi = float_bits & 0x7FFF_FFFF;
    if absi < 0x0080_0000 {
        0x8000_0000
    } else if float_bits < 0x8000_0000 {
        0x7F80_0001u32.wrapping_add(float_bits)
    } else {
        0x807F_FFFFu32.wrapping_sub(absi)
    }
}

/// `ExtractFloatComponents(floatBits, sign, exponent, mantissa)`, returning
/// `(sign, exponent, mantissa)`.
///
/// Port of `ExtractFloatComponents` (src/OpenColorIO/MathUtils.cpp:446-453 @ v2.5.2).
#[inline]
fn extract_float_components(float_bits: u32) -> (u32, u32, u32) {
    let mantissa = float_bits & 0x007F_FFFF;
    let sign_exp = float_bits >> 23;
    (sign_exp >> 8, sign_exp & 0xFF, mantissa)
}

/// `FloatsDiffer(expected, actual, tolerance, compressDenorms)`: whether the values differ by
/// more than `tolerance` ULPs, on the ordered integer scale of [`float_for_compare`] (or, with
/// `compress_denorms`, of [`float_for_compare_compress_denorms`]). `-0` equals `0`. Every NaN
/// equals every NaN; an infinity equals only itself; a special value differs from every
/// regular one. A negative `tolerance` is converted to `unsigned`, as in C++, so nothing
/// differs.
///
/// Port of `FloatsDiffer` (src/OpenColorIO/MathUtils.cpp:455-529 @ v2.5.2).
pub fn floats_differ(expected: f32, actual: f32, tolerance: i32, compress_denorms: bool) -> bool {
    let expected_bits = float_as_int(expected);
    let actual_bits = float_as_int(actual);

    let (es, ee, em) = extract_float_components(expected_bits);
    let (as_, ae, am) = extract_float_components(actual_bits);

    let is_expected_special = ee == 0xFF;
    let is_actual_special = ae == 0xFF;
    if is_expected_special {
        // expected is a special float (-/+Inf or NaN).
        if is_actual_special {
            // Comparing special floats.
            let is_expected_inf = em == 0;
            let is_actual_inf = am == 0;
            if is_expected_inf {
                // Comparing -/+Inf with -/+Inf, or -/+Inf with NaN.
                return if is_actual_inf { es != as_ } else { true };
            }
            // Comparing NaN with a special float.
            return is_actual_inf;
        }
        // Comparing a special float with a regular float.
        return true;
    } else if is_actual_special {
        // Comparing a regular float with a special float.
        return true;
    }

    // Comparing regular floats.
    let (expected_comp, actual_comp) = if compress_denorms {
        (
            float_for_compare_compress_denorms(expected_bits),
            float_for_compare_compress_denorms(actual_bits),
        )
    } else {
        (
            float_for_compare(expected_bits),
            float_for_compare(actual_bits),
        )
    };

    // (expectedBitsComp > actualBitsComp) ? (expectedBitsComp - actualBitsComp)
    //                                     : (actualBitsComp - expectedBitsComp)
    let diff = expected_comp.abs_diff(actual_comp);
    diff > tolerance as u32
}

/// Imath's `half::isNan()`: the exponent is 31 and the mantissa is not 0 (Imath 3.2.1,
/// src/Imath/half.h).
#[inline]
fn half_is_nan(h: u16) -> bool {
    (h >> 10) & 0x001F == 31 && h & 0x03FF != 0
}

/// Imath's `half::isInfinity()`: the exponent is 31 and the mantissa is 0 (Imath 3.2.1,
/// src/Imath/half.h).
#[inline]
fn half_is_infinity(h: u16) -> bool {
    (h >> 10) & 0x001F == 31 && h & 0x03FF == 0
}

/// `HalfForCompare(h)`: maps a half's bits to an ordered integer, with +0 and -0 both 32768.
/// The test is `< 32767`, so 0x7FFF (a NaN, which the caller handles first) is mapped as if it
/// were negative.
///
/// Port of `HalfForCompare` (src/OpenColorIO/MathUtils.cpp:531-537 @ v2.5.2).
#[inline]
fn half_for_compare(h: u16) -> i32 {
    let raw_half = i32::from(h);
    if raw_half < 32767 {
        raw_half + 32768
    } else {
        2 * 32768 - raw_half
    }
}

/// `HalfsDiffer(expected, actual, tolerance)`: whether two halves differ by more than
/// `tolerance` steps of their bits, +0 and -0 being equal. A NaN equals only a NaN, and an
/// infinity only itself.
///
/// Port of `HalfsDiffer` (src/OpenColorIO/MathUtils.cpp:539-571 @ v2.5.2).
pub fn halfs_differ(expected: half::f16, actual: half::f16, tolerance: i32) -> bool {
    let (expected, actual) = (expected.to_bits(), actual.to_bits());
    let aim_bits = half_for_compare(expected);
    let val_bits = half_for_compare(actual);

    if half_is_nan(expected) {
        return !half_is_nan(actual);
    } else if half_is_nan(actual) {
        return !half_is_nan(expected);
    } else if half_is_infinity(expected) || half_is_infinity(actual) {
        // Upstream's two branches, one per operand, return the same.
        return aim_bits != val_bits;
    } else if (val_bits - aim_bits).abs() > tolerance {
        return true;
    }
    false
}

#[cfg(test)]
#[path = "math_utils_tests.rs"]
mod tests;
