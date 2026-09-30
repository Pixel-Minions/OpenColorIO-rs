// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! OCIO's fast-math approximations, as exact scalar code.
//!
//! Port of `src/OpenColorIO/SSE.h` @ v2.5.2. With the default optimization level
//! (`OPTIMIZATION_FAST_LOG_EXP_POW` set), OCIO's Log, Gamma, CDL and PQ renderers call these
//! polynomial approximations instead of the math library (`logf`, `powf`, ...).
//!
//! Each `__m128` function of `SSE.h` computes four independent lanes. The port computes one
//! lane with the same operations in the same order: `f32` multiplies and adds that are never
//! fused (the wheel's SSE2 translation units contain no FMA), the same bit manipulations, the
//! same compare-and-select masks and the same float↔int conversions. So each function returns
//! the bits one lane of the C++ returns, including for NaN, infinities and subnormals.
//!
//! Rust's `+` and `*` are enough here, although LLVM may swap their operands: `sseLog2` never
//! returns NaN, and wherever else two NaNs meet in an addition or a multiplication, they are
//! copies of one input NaN (quieted or not), so either order gives the same bits. (The
//! renderers that call these functions use `math_utils::sse_add`/`sse_mul`, where a NaN pixel
//! can meet a different NaN coefficient.)
//!
//! The functions with a `_scalar` suffix port the separate scalar overloads of `SSE.h`
//! (`sseAtan(float)`, `sseAtan2(float, float)`, `sseSinCos(float, ...)`), whose control flow
//! differs from the four-lane versions.
//!
//! The constants are `(float)` casts of double literals: round to nearest, ties to even, as
//! both compilers do. The ones the library uses were read back from both builds of the
//! wheel and match bit for bit (see `docs/spikes/s2-s5.md`). `sseAtan`, `sseAtan2`, `sseCos`,
//! `sseSin` and `sseSinCos` are not called anywhere in OCIO 2.5.2's library, only by its
//! unit tests.

use crate::math_utils::{sse_cvtps_epi32, sse_cvttps_epi32};

/// `EXP_MASK`: the exponent bits of an `f32`.
pub const EXP_MASK: i32 = 0x7F80_0000;
/// `EXP_BIAS`: the `f32` exponent bias.
pub const EXP_BIAS: i32 = 127;
/// `EXP_SHIFT`: the position of the `f32` exponent.
pub const EXP_SHIFT: u32 = 23;
/// `SIGN_SHIFT`: the position of the `f32` sign.
pub const SIGN_SHIFT: u32 = 31;

/// `EONE`.
pub const EONE: f32 = 1.0;
/// `EZERO`.
pub const EZERO: f32 = 0.0;
/// `ENEG126`.
pub const ENEG126: f32 = -126.0;
/// `EPOS128`.
pub const EPOS128: f32 = 128.0;
/// `EPOSINF`.
pub const EPOSINF: f32 = f32::INFINITY;

// Coefficients of Chebyshev (minimax) degree 5 polynomial approximation to log2() over the
// range [1.0, 2.0[ (SSE.h:158-165).
/// `PNLOG5`.
pub const PNLOG5: f32 = 4.487361286440374006195e-2_f64 as f32;
/// `PNLOG4`.
pub const PNLOG4: f32 = -4.165637071209677112635e-1_f64 as f32;
/// `PNLOG3`.
pub const PNLOG3: f32 = 1.631148826119436277100_f64 as f32;
/// `PNLOG2`.
pub const PNLOG2: f32 = -3.550793018041176193407_f64 as f32;
/// `PNLOG1`.
pub const PNLOG1: f32 = 5.091710879305474367557_f64 as f32;
/// `PNLOG0`.
pub const PNLOG0: f32 = -2.800364054395965731506_f64 as f32;

// Coefficients of Chebyshev (minimax) degree 4 polynomial approximation to exp2() over the
// range [0.0, 1.0[ (SSE.h:167-173).
/// `PNEXP4`.
pub const PNEXP4: f32 = 1.353416792833547468620e-2_f64 as f32;
/// `PNEXP3`.
pub const PNEXP3: f32 = 5.201146058412685018921e-2_f64 as f32;
/// `PNEXP2`.
pub const PNEXP2: f32 = 2.414427569091865207710e-1_f64 as f32;
/// `PNEXP1`.
pub const PNEXP1: f32 = 6.930038344665415134202e-1_f64 as f32;
/// `PNEXP0`.
pub const PNEXP0: f32 = 1.000002593370603213644_f64 as f32;

/// `ESIGN_MASK`: the sign bit (SSE.h:341).
pub const ESIGN_MASK: u32 = 0x8000_0000;
/// `EABS_MASK`: everything but the sign bit (SSE.h:342).
pub const EABS_MASK: u32 = 0x7fff_ffff;

/// `E_PI` (SSE.h:344).
pub const E_PI: f32 = 3.14159265358979323846_f64 as f32;
/// `E_PI_2` (SSE.h:345).
pub const E_PI_2: f32 = 1.57079632679489661923_f64 as f32;
/// `E_1_PI` (SSE.h:537).
pub const E_1_PI: f32 = 0.31830988618379067153776752674503_f64 as f32;

// Rational polynomial coefficients for the arc tangent approximation (SSE.h:370-377, and the
// same values in the scalar overload at SSE.h:427-434).
const PN_ATAN_A1: f32 = 48.70107004404898384_f64 as f32;
const PN_ATAN_A2: f32 = 49.5326263772254345_f64 as f32;
const PN_ATAN_A3: f32 = 9.40604244231624_f64 as f32;
const PN_ATAN_B1: f32 = 48.70107004404996166_f64 as f32;
const PN_ATAN_B2: f32 = 65.7663163908956299_f64 as f32;
const PN_ATAN_B3: f32 = 21.587934067020262_f64 as f32;

// Chebyshev polynomial coefficients for the cosine approximation (SSE.h:549-555).
const PN_COS_C1: f32 = 0.999999953464_f64 as f32;
const PN_COS_C2: f32 = -0.499999053455_f64 as f32;
const PN_COS_C3: f32 = 0.0416635846769_f64 as f32;
const PN_COS_C4: f32 = -0.0013853704264_f64 as f32;
const PN_COS_C5: f32 = 0.00002315393167_f64 as f32;

/// A lane mask as the SSE compares produce it: all ones when true, zero when false.
#[inline]
fn lane_mask(condition: bool) -> u32 {
    if condition { u32::MAX } else { 0 }
}

/// One lane of `isNegativeSpecial(x)`: an all-ones mask when the sign bit is set (so for
/// -0, -NaN and -Inf too), else zero. `_mm_srai_epi32(x, 31)` is an arithmetic shift.
///
/// Port of `isNegativeSpecial` (src/OpenColorIO/SSE.h:110-116 @ v2.5.2).
#[inline]
pub fn is_negative_special(x: f32) -> u32 {
    ((x.to_bits() as i32) >> SIGN_SHIFT) as u32
}

/// One lane of `sseSelect(mask, arg_true, arg_false)`:
/// `((arg_true XOR arg_false) AND mask) XOR arg_false`, bit by bit.
///
/// Port of `sseSelect` (src/OpenColorIO/SSE.h:118-156 @ v2.5.2).
#[inline]
pub fn sse_select(mask: u32, arg_true: f32, arg_false: f32) -> f32 {
    let f = arg_false.to_bits();
    f32::from_bits(f ^ ((arg_true.to_bits() ^ f) & mask))
}

/// One lane of `sseLog2(x)`: `exponent + P(mantissa)`, where the mantissa is `x`'s mantissa
/// bits with a 1.0 exponent (keeping `x`'s sign bit) and `P` is the degree-5 polynomial.
///
/// About 15 good bits of mantissa for positive normal `x`. Other inputs follow the bit
/// manipulation: `sse_log2(0.0)` is close to -127, and infinities and NaNs give values near
/// 128.
///
/// Port of `sseLog2` (src/OpenColorIO/SSE.h:179-228 @ v2.5.2).
#[inline]
pub fn sse_log2(x: f32) -> f32 {
    // mantissa = (~EMASK & x) | EONE
    let mantissa = f32::from_bits((!(EXP_MASK as u32) & x.to_bits()) | EONE.to_bits());

    let log2 = ((((PNLOG5 * mantissa + PNLOG4) * mantissa + PNLOG3) * mantissa + PNLOG2)
        * mantissa
        + PNLOG1)
        * mantissa
        + PNLOG0;

    // exponent = ((x & EMASK) >> EXP_SHIFT) - EBIAS, with a logical shift.
    let exponent = (((x.to_bits() & EXP_MASK as u32) >> EXP_SHIFT) as i32).wrapping_sub(EXP_BIAS);

    // _mm_cvtepi32_ps(exponent): exact for these small integers.
    log2 + exponent as f32
}

/// One lane of `sseExp2(x)`: `2^floor(x) * P(x - floor(x))`, with `P` the degree-4
/// polynomial; 0 when `x < -126` and +Inf when `x >= 128`.
///
/// `floor(x)` is computed as the C++ does: a truncating conversion, minus 1 when
/// `!(0 <= x)` (so also for NaN, and for negative integers, which makes the fraction 1).
/// Out-of-range and NaN inputs make the integer wrong, but those lanes are overridden by the
/// range checks or stay NaN through the polynomial.
///
/// Port of `sseExp2` (src/OpenColorIO/SSE.h:230-307 @ v2.5.2).
#[inline]
pub fn sse_exp2(x: f32) -> f32 {
    // _mm_cmpnle_ps(EZERO, x) is all ones (-1 as an integer) when NOT(0 <= x).
    let adjust: i32 = if EZERO <= x { 0 } else { -1 };
    let floor_x = sse_cvttps_epi32(x).wrapping_add(adjust);

    // exp2(floor_x), by moving floor_x into the exponent bits.
    let zf = f32::from_bits((floor_x.wrapping_add(EXP_BIAS) as u32) << EXP_SHIFT);

    let iexp = floor_x as f32; // _mm_cvtepi32_ps: round to nearest, ties to even.
    let fraction = x - iexp;

    let mexp = (((PNEXP4 * fraction + PNEXP3) * fraction + PNEXP2) * fraction + PNEXP1) * fraction
        + PNEXP0;

    let mut exp2 = zf * mexp;

    // Underflow: _mm_andnot_ps(_mm_cmplt_ps(x, ENEG126), exp2) clears every bit.
    if x < ENEG126 {
        exp2 = f32::from_bits(0);
    }

    // Overflow: sseSelect(_mm_cmpge_ps(x, EPOS128), EPOSINF, exp2).
    sse_select(lane_mask(x >= EPOS128), EPOSINF, exp2)
}

/// One lane of `ssePower(x, exp)`: `sseExp2(exp * sseLog2(x))`, with every lane whose base
/// is not greater than zero (including NaN) forced to +0.
///
/// Port of `ssePower` (src/OpenColorIO/SSE.h:309-339 @ v2.5.2).
#[inline]
pub fn sse_power(x: f32, exp: f32) -> f32 {
    let log2 = sse_log2(x);

    // _mm_mul_ps(exp, values)
    let values = sse_exp2(exp * log2);

    // Handle values where base is smaller or equal than zero:
    // _mm_and_ps(values, _mm_cmpgt_ps(x, EZERO)).
    f32::from_bits(values.to_bits() & lane_mask(x > EZERO))
}

/// One lane of the four-lane `sseAtan(x)`: a rational approximation over [0, 1] after the
/// reductions `atan(x) = -atan(-x)` and `atan(x) = PI/2 - atan(1/x)`, applied with masks.
/// About 14 good bits of mantissa.
///
/// Port of `sseAtan(const __m128)` (src/OpenColorIO/SSE.h:347-422 @ v2.5.2).
#[inline]
pub fn sse_atan(x: f32) -> f32 {
    // Apply identity atan(x) = -atan(-x) to reduce domain to [0, Inf).
    let sign_x = x.to_bits() & ESIGN_MASK;
    let abs_x = f32::from_bits(x.to_bits() & EABS_MASK);

    // Apply identity atan(x) = PI/2 - atan(1/x) to reduce domain to [0,1].
    let inv_mask = lane_mask(abs_x > EONE);
    let inv_abs_x = EONE / abs_x;
    let norm_x = sse_select(inv_mask, inv_abs_x, abs_x);

    // Compute atan using a normalized input.
    let norm_x2 = norm_x * norm_x;

    let num = ((norm_x2 * PN_ATAN_A3 + PN_ATAN_A2) * norm_x2 + PN_ATAN_A1) * norm_x;

    let denom = ((norm_x2 + PN_ATAN_B3) * norm_x2 + PN_ATAN_B2) * norm_x2 + PN_ATAN_B1;

    let mut res = num / denom;

    // If the input was inverted during domain reduction, correct the result by subtracting
    // it from PI/2.
    res = sse_select(inv_mask, E_PI_2 - res, res);

    // If the input was negated during domain reduction, correct the result by negating it
    // again: _mm_or_ps(sign_x, res).
    f32::from_bits(sign_x | res.to_bits())
}

/// The scalar `sseAtan(v)`: the same approximation as [`sse_atan`], with branches instead of
/// masks.
///
/// Port of `sseAtan(const float)` (src/OpenColorIO/SSE.h:424-478 @ v2.5.2).
#[inline]
pub fn sse_atan_scalar(v: f32) -> f32 {
    const F_PI_2: f32 = 1.57079632679489661923_f64 as f32;

    let mut inv = false;
    let mut neg = false;
    let mut x = v;

    // Apply identity atan(x) = -atan(-x) to reduce domain to [0, Inf).
    if x < 0.0f32 {
        x = -x;
        neg = true;
    }

    // Apply identity atan(x) = PI/2 - atan(1/x) to reduce domain to [0,1].
    if x > 1.0f32 {
        x = 1.0f32 / x;
        inv = true;
    }

    // Compute atan using a normalized input.
    let x2 = x * x;

    let num = x * (PN_ATAN_A1 + x2 * (PN_ATAN_A2 + x2 * PN_ATAN_A3));
    let denom = PN_ATAN_B1 + x2 * (PN_ATAN_B2 + x2 * (x2 + PN_ATAN_B3));
    let mut res = num / denom;

    // If the input was inverted during domain reduction, correct the result by subtracting
    // it from PI/2.
    if inv {
        res = F_PI_2 - res;
    }

    // If the input was negated during domain reduction, correct the result by negating it
    // again.
    if neg {
        res = -res;
    }

    res
}

/// One lane of the four-lane `sseAtan2(y, x)`: `sseAtan(y / x)`, zeroed when both inputs are
/// zero, plus `±PI` (the sign of `y`) when `x`'s sign bit is set.
///
/// Port of `sseAtan2(const __m128, const __m128)` (src/OpenColorIO/SSE.h:480-511 @ v2.5.2).
#[inline]
pub fn sse_atan2(y: f32, x: f32) -> f32 {
    let mut res = sse_atan(y / x);

    // Fix for x=0 and y=0: _mm_cmpneq_ps is true for unordered values.
    let zero_mask = lane_mask(x != EZERO) | lane_mask(y != EZERO);
    res = f32::from_bits(res.to_bits() & zero_mask);

    // Adjust quadrants 2 and 3 based on the sign of the arguments.
    let neg_x = is_negative_special(x);
    let sign_y = y.to_bits() & ESIGN_MASK;

    res + f32::from_bits((sign_y | E_PI.to_bits()) & neg_x)
}

/// The scalar `sseAtan2(y, x)`: the same algorithm as [`sse_atan2`], with branches, and
/// adding `±PI` only when `x`'s sign bit is set.
///
/// Port of `sseAtan2(const float, const float)` (src/OpenColorIO/SSE.h:513-535 @ v2.5.2).
#[inline]
pub fn sse_atan2_scalar(y: f32, x: f32) -> f32 {
    const SIGN_MASK: u32 = 0x8000_0000;
    const F_PI: f32 = 3.14159265358979323846_f64 as f32;

    // Fix for x=0 and y=0.
    let mut res = if x == 0.0f32 && y == 0.0f32 {
        0.0f32
    } else {
        sse_atan_scalar(y / x)
    };

    // Adjust quadrants 2 and 3 based on the sign of the arguments.
    let neg_x = (x.to_bits() & SIGN_MASK) != 0;
    let neg_y = (y.to_bits() & SIGN_MASK) != 0;

    if neg_x {
        res += if neg_y { -F_PI } else { F_PI };
    }
    res
}

/// The intermediate results of `__sse_cos__`, shared by the cosine and sine-cosine
/// approximations.
#[derive(Debug, Clone, Copy)]
struct CosParts {
    /// The cosine of the angle.
    cos_x: f32,
    /// The input angle reduced to [-pi/2, pi/2].
    xr: f32,
    /// `xr * xr`.
    xr2: f32,
    /// The sign bit that was flipped because the angle was in quadrant 2 or 3.
    flip_sign_cos_x: u32,
}

/// One lane of `__sse_cos__(x, ...)`: reduces `x` to [-pi/2, pi/2] with a rounding conversion
/// of `x / pi`, evaluates the degree-4 polynomial in `xr^2`, and flips the sign for odd
/// half-turns.
///
/// Port of `__sse_cos__` (src/OpenColorIO/SSE.h:539-584 @ v2.5.2).
#[inline]
fn sse_cos_parts(x: f32) -> CosParts {
    // Reduce to [-pi/2, pi/2]. _mm_cvtps_epi32 rounds to nearest, ties to even.
    let cycles = sse_cvtps_epi32(x * E_1_PI);

    let xr = x - cycles as f32 * E_PI;
    let xr2 = xr * xr;

    let cos_x =
        (((PN_COS_C5 * xr2 + PN_COS_C4) * xr2 + PN_COS_C3) * xr2 + PN_COS_C2) * xr2 + PN_COS_C1;

    // If cycles is odd, then the angle is in either quadrant 2 or 3. In these case we need to
    // invert the sign of the result: _mm_slli_epi32(cycles, 31).
    let flip_sign_cos_x = (cycles as u32) << 31;
    CosParts {
        cos_x: f32::from_bits(cos_x.to_bits() ^ flip_sign_cos_x),
        xr,
        xr2,
        flip_sign_cos_x,
    }
}

/// One lane of `sseCos(x)`. About 17 good bits of mantissa.
///
/// Port of `sseCos` (src/OpenColorIO/SSE.h:586-600 @ v2.5.2).
#[inline]
pub fn sse_cos(x: f32) -> f32 {
    sse_cos_parts(x).cos_x
}

/// One lane of `sseSin(x)`: `sseCos(PI/2 - x)`.
///
/// Port of `sseSin` (src/OpenColorIO/SSE.h:602-612 @ v2.5.2).
#[inline]
pub fn sse_sin(x: f32) -> f32 {
    sse_cos(E_PI_2 - x)
}

/// One lane of the four-lane `sseSinCos(x, sin_x, cos_x)`, returned as `(sin_x, cos_x)`: the
/// cosine as in [`sse_cos`], and the sine as `sqrt(1 - cos^2)` (or `|xr|` when the reduced
/// angle is below 2^-7) with its sign fixed from the quadrant.
///
/// Port of `sseSinCos(const __m128, __m128&, __m128&)`
/// (src/OpenColorIO/SSE.h:614-643 @ v2.5.2).
#[inline]
pub fn sse_sin_cos(x: f32) -> (f32, f32) {
    // Using a threshold of 2^-7 for the reduced angle seems to provide a fairly decent
    // precision (16 bits) to the final result.
    const SINE_THRESHOLD_SQUARED: f32 = 0.00006103515625_f64 as f32;

    let CosParts {
        cos_x,
        xr,
        xr2,
        flip_sign_cos_x,
    } = sse_cos_parts(x);

    // When cos(x) becomes too close to 1, the sin(x) evaluation contains too much error.
    // However, in this case, sin(x) ~ x, and we can use xr to approximate sin(x) instead.
    let mut sin_x2 = EONE - cos_x * cos_x;
    sin_x2 = sse_select(lane_mask(xr2 > SINE_THRESHOLD_SQUARED), sin_x2, xr2);
    let sin_x = sin_x2.sqrt();

    // Flip the sign of sin(x) if the angle was in quadrants 3 or 4.
    let xr_sign = xr.to_bits() & ESIGN_MASK;
    let flip_sign_sin_x = flip_sign_cos_x ^ xr_sign;
    (f32::from_bits(sin_x.to_bits() ^ flip_sign_sin_x), cos_x)
}

/// The scalar `sseSinCos(x, sin_x, cos_x)`, returned as `(sin_x, cos_x)`: two lanes of
/// `sseCos`, one on `x` and one on `PI/2 - x` (computed in scalar `float`). Unlike
/// [`sse_sin_cos`], the sine is a phased cosine.
///
/// Port of `sseSinCos(const float, float&, float&)` (src/OpenColorIO/SSE.h:645-658 @ v2.5.2).
#[inline]
pub fn sse_sin_cos_scalar(x: f32) -> (f32, f32) {
    const F_PI_2: f32 = 1.57079632679489661923_f64 as f32;

    // __m128 sc = _mm_setr_ps(x, F_PI_2-x, 0, 0); __m128 res = sseCos(sc);
    let cos_x = sse_cos(x);
    let sin_x = sse_cos(F_PI_2 - x);
    (sin_x, cos_x)
}

#[cfg(test)]
#[allow(unsafe_code)] // The tests compare lanes with the SSE2 instructions (core::arch).
#[path = "sse_tests.rs"]
mod tests;
