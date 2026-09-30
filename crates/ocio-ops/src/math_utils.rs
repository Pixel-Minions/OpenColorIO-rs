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
//! - The matrix and vector math of `MathUtils.cpp` (`GetM44Inverse`, `GetM44M44Product`,
//!   `GetMxbCombine`, ...), in the source's operand order. That order is a deliberate choice:
//!   nothing in 2.5.2 calls these functions but `IsM44Identity`, and the Windows wheel
//!   doesn't contain them. The Linux wheel's unused copies commute some operations (listed
//!   above the functions), which would only change which NaN comes out where two NaNs meet.
//!
//! Port of parts of `src/OpenColorIO/MathUtils.h`, `src/OpenColorIO/MathUtils.cpp` and
//! `src/OpenColorIO/BitDepthUtils.h` @ v2.5.2.

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
/// conversion to `float` (to nearest, ties to even), so it counts as 0 exactly when
/// |v| <= 2.5 * 2^-149, about 3.50e-45: a LogAffine slope of 2.1e-45 is refused as 0, and one
/// of 3.6e-45 is accepted.
///
/// Port of `IsScalarEqualToZero<T>` (src/OpenColorIO/MathUtils.cpp:17-27 @ v2.5.2).
#[inline]
pub fn is_scalar_equal_to_zero<T: MathFloat>(v: T) -> bool {
    !floats_differ(0.0, v.to_f32(), 2, false)
}

/// `IsScalarEqualToOne(v)`: `v`, converted to `float`, is within 2 ULPs of 1. A `double`
/// counts as 1 exactly in [1 - 2.5 * 2^-24, 1 + 2.5 * 2^-23], about [1 - 1.49e-7,
/// 1 + 2.98e-7]: 1 + 2.98e-7 is one and 1 + 2.99e-7 isn't; 1 - 1.49e-7 is one and
/// 1 - 1.50e-7 isn't.
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
/// `GetHalfNormMin()` is just below 2^-14, so the values in [6.10351562e-05, 2^-14), which
/// aren't normal halves, are kept. No `float` lies in that band: only `double` values, such
/// as literals, can fall in it.
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

/// Imath's `half::isNan()`: the exponent is 31 and the mantissa is not 0.
///
/// Port of `half::isNan` (src/Imath/half.h:837-841, with `half::mantissa` and
/// `half::exponent` at :801-811 @ Imath v3.2.1).
#[inline]
fn half_is_nan(h: u16) -> bool {
    (h >> 10) & 0x001F == 31 && h & 0x03FF != 0
}

/// Imath's `half::isInfinity()`: the exponent is 31 and the mantissa is 0.
///
/// Port of `half::isInfinity` (src/Imath/half.h:843-847, with `half::mantissa` and
/// `half::exponent` at :801-811 @ Imath v3.2.1).
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

// ---------------------------------------------------------------------------------------------
// 4x4 matrices (row-major) and 4-vectors.
//
// OpenColorIO 2.5.2 calls none of these outside its tests, except `IsM44Identity`
// (src/OpenColorIO/OCIOYaml.cpp:3071 @ v2.5.2): the matrix op has its own `double` math
// (`MatrixOpData::MatrixArray::inverse`, `MatrixOpData::compose`). The Windows wheel doesn't
// contain them (MSVC's linker drops unreferenced functions). The Linux wheel keeps unexported
// copies, and GCC commutes some of their operations (addresses in libOpenColorIO.so):
// - `GetM44Inverse`: both products of `d10_21` (0x2cbf43, 0x2cbf51) and the three additions
//   of `det` (0x2cc126, 0x2cc137, 0x2cc146);
// - `GetM44M44Product`: the last addition of each entry, `p3 + ((p0 + p1) + p2)` (0x2cc578);
// - the copies of `GetM44V4Product` it inlined: the first addition and the last product of
//   each entry in `GetMxbCombine` (0x2cc718, 0x2cc75f), and the products of the last entry in
//   `GetMxbInverse` (0x2cc88c).
// Operand order only decides which NaN comes out where two NaNs meet. Nothing in 2.5.2 calls
// these functions and the Windows wheel has none of them, so no output of either wheel depends
// on their NaNs: the port deliberately keeps the source's order, pinned with `sse_add` and
// `sse_mul`, rather than the Linux copies'. None of the copies use an FMA.

/// `IsM44Identity(m44)`: the diagonal entries [are equal to one](is_scalar_equal_to_one) and the
/// others [to zero](is_scalar_equal_to_zero), each converted to `float` and compared within 2
/// ULPs.
///
/// Port of `IsM44Identity<T>` (src/OpenColorIO/MathUtils.cpp:162-191 @ v2.5.2).
pub fn is_m44_identity<T: MathFloat>(m44: &[T; 16]) -> bool {
    for j in 0..4 {
        for i in 0..4 {
            let index = 4 * j + i;
            if i == j {
                if !is_scalar_equal_to_one(m44[index]) {
                    return false;
                }
            } else if !is_scalar_equal_to_zero(m44[index]) {
                return false;
            }
        }
    }
    true
}

/// `GetM44Inverse(inverse_out, m)`: the inverse of a 4x4 matrix, from its cofactors, computed in
/// `double` and converted back to `float`. `None` where upstream returns false (leaving
/// `inverse_out` alone): the determinant, converted to `float`, [is equal to
/// zero](is_scalar_equal_to_zero). The test is absolute, so a matrix whose determinant is at
/// most 2.5 * 2^-149 (about 3.50e-45) in magnitude counts as singular however well conditioned
/// it is, and a singular matrix whose determinant rounds away from 0 is inverted into huge
/// values. A NaN determinant is not zero: the inverse is all NaN.
///
/// Port of `GetM44Inverse` (src/OpenColorIO/MathUtils.cpp:193-261 @ v2.5.2).
pub fn get_m44_inverse(m_: &[f32; 16]) -> Option<[f32; 16]> {
    let m: [f64; 16] = m_.map(f64::from);
    let mul = sse_mul::<f64>;
    let add = sse_add::<f64>;

    let d10_21 = mul(m[4], m[9]) - mul(m[5], m[8]);
    let d10_22 = mul(m[4], m[10]) - mul(m[6], m[8]);
    let d10_23 = mul(m[4], m[11]) - mul(m[7], m[8]);
    let d11_22 = mul(m[5], m[10]) - mul(m[6], m[9]);
    let d11_23 = mul(m[5], m[11]) - mul(m[7], m[9]);
    let d12_23 = mul(m[6], m[11]) - mul(m[7], m[10]);

    let a00 = add(mul(m[13], d12_23) - mul(m[14], d11_23), mul(m[15], d11_22));
    let a10 = mul(m[14], d10_23) - mul(m[15], d10_22) - mul(m[12], d12_23);
    let a20 = add(mul(m[12], d11_23) - mul(m[13], d10_23), mul(m[15], d10_21));
    let a30 = mul(m[13], d10_22) - mul(m[14], d10_21) - mul(m[12], d11_22);

    let det = add(
        add(add(mul(a00, m[0]), mul(a10, m[1])), mul(a20, m[2])),
        mul(a30, m[3]),
    );

    if is_scalar_equal_to_zero(det as f32) {
        return None;
    }

    let det = 1.0 / det;

    let d00_31 = mul(m[0], m[13]) - mul(m[1], m[12]);
    let d00_32 = mul(m[0], m[14]) - mul(m[2], m[12]);
    let d00_33 = mul(m[0], m[15]) - mul(m[3], m[12]);
    let d01_32 = mul(m[1], m[14]) - mul(m[2], m[13]);
    let d01_33 = mul(m[1], m[15]) - mul(m[3], m[13]);
    let d02_33 = mul(m[2], m[15]) - mul(m[3], m[14]);

    let a01 = add(mul(m[9], d02_33) - mul(m[10], d01_33), mul(m[11], d01_32));
    let a11 = mul(m[10], d00_33) - mul(m[11], d00_32) - mul(m[8], d02_33);
    let a21 = add(mul(m[8], d01_33) - mul(m[9], d00_33), mul(m[11], d00_31));
    let a31 = mul(m[9], d00_32) - mul(m[10], d00_31) - mul(m[8], d01_32);

    let a02 = mul(m[6], d01_33) - mul(m[7], d01_32) - mul(m[5], d02_33);
    let a12 = add(mul(m[4], d02_33) - mul(m[6], d00_33), mul(m[7], d00_32));
    let a22 = mul(m[5], d00_33) - mul(m[7], d00_31) - mul(m[4], d01_33);
    let a32 = add(mul(m[4], d01_32) - mul(m[5], d00_32), mul(m[6], d00_31));

    let a03 = mul(m[2], d11_23) - mul(m[3], d11_22) - mul(m[1], d12_23);
    let a13 = add(mul(m[0], d12_23) - mul(m[2], d10_23), mul(m[3], d10_22));
    let a23 = mul(m[1], d10_23) - mul(m[3], d10_21) - mul(m[0], d11_23);
    let a33 = add(mul(m[0], d11_22) - mul(m[1], d10_22), mul(m[2], d10_21));

    Some(
        [
            a00, a01, a02, a03, a10, a11, a12, a13, a20, a21, a22, a23, a30, a31, a32, a33,
        ]
        .map(|a| mul(a, det) as f32),
    )
}

/// `GetM44M44Product(mout, m1, m2)`: the product `m1 m2`, in `float`. Entry `(r, c)` is
/// `((m1[r][0]*m2[0][c] + m1[r][1]*m2[1][c]) + m1[r][2]*m2[2][c]) + m1[r][3]*m2[3][c]`.
///
/// Port of `GetM44M44Product` (src/OpenColorIO/MathUtils.cpp:263-286 @ v2.5.2).
pub fn get_m44_m44_product(m1: &[f32; 16], m2: &[f32; 16]) -> [f32; 16] {
    std::array::from_fn(|i| {
        let (r, c) = (i / 4 * 4, i % 4);
        let sum = sse_add(sse_mul(m1[r], m2[c]), sse_mul(m1[r + 1], m2[4 + c]));
        let sum = sse_add(sum, sse_mul(m1[r + 2], m2[8 + c]));
        sse_add(sum, sse_mul(m1[r + 3], m2[12 + c]))
    })
}

/// `GetM44V4Product(vout, m, v)`: the product `m v`, in `float`. Entry `r` is
/// `((m[r][0]*v[0] + m[r][1]*v[1]) + m[r][2]*v[2]) + m[r][3]*v[3]`.
///
/// Port of `GetM44V4Product` (src/OpenColorIO/MathUtils.cpp:291-300 @ v2.5.2), a function of an
/// anonymous namespace that upstream's tests include.
pub fn get_m44_v4_product(m: &[f32; 16], v: &[f32; 4]) -> [f32; 4] {
    std::array::from_fn(|i| {
        let r = 4 * i;
        let sum = sse_add(sse_mul(m[r], v[0]), sse_mul(m[r + 1], v[1]));
        let sum = sse_add(sum, sse_mul(m[r + 2], v[2]));
        sse_add(sum, sse_mul(m[r + 3], v[3]))
    })
}

/// `GetV4Sum(vout, v1, v2)`: `v1 + v2`, in `float`.
///
/// Port of `GetV4Sum` (src/OpenColorIO/MathUtils.cpp:302-308 @ v2.5.2), a function of an
/// anonymous namespace that upstream's tests include.
pub fn get_v4_sum(v1: &[f32; 4], v2: &[f32; 4]) -> [f32; 4] {
    std::array::from_fn(|i| sse_add(v1[i], v2[i]))
}

/// `GetMxbCombine(mout, vout, m1, v1, m2, v2)`: the single `mout x + vout` that equals
/// `m2 (m1 x + v1) + v2`: `mout = m2 m1` and `vout = m2 v1 + v2`.
///
/// Port of `GetMxbCombine` (src/OpenColorIO/MathUtils.cpp:312-332 @ v2.5.2).
pub fn get_mxb_combine(
    m1: &[f32; 16],
    v1: &[f32; 4],
    m2: &[f32; 16],
    v2: &[f32; 4],
) -> ([f32; 16], [f32; 4]) {
    let mout = get_m44_m44_product(m2, m1);
    let vout = get_m44_v4_product(m2, v1);
    let vout = get_v4_sum(&vout, v2);
    (mout, vout)
}

/// `GetMxbInverse(mout, vout, m, v)`: the inverse of `m x + v`, `mout x + vout` with
/// `mout = m^-1` ([`get_m44_inverse`]) and `vout = mout (-v)`. `None` where upstream returns
/// false, leaving `mout` and `vout` alone: `m` is singular.
///
/// Port of `GetMxbInverse` (src/OpenColorIO/MathUtils.cpp:334-351 @ v2.5.2).
pub fn get_mxb_inverse(m: &[f32; 16], v: &[f32; 4]) -> Option<([f32; 16], [f32; 4])> {
    let mout = get_m44_inverse(m)?;
    let v = v.map(|x| -x);
    let vout = get_m44_v4_product(&mout, &v);
    Some((mout, vout))
}

#[cfg(test)]
#[path = "math_utils_tests.rs"]
mod tests;
