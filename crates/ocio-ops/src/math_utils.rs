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
//! - The bit helpers of `MathUtils.h` (`FloatAsInt`, `IntAsFloat`, `AddULP`).
//!
//! Port of parts of `src/OpenColorIO/MathUtils.h` @ v2.5.2. The scalar float→integer casts of
//! `BitDepthUtils.h` are in [`crate::bit_depth_utils`].

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
