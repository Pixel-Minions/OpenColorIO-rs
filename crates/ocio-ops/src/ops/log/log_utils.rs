// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The camera-style Log helpers: the linear segment's slope, the break on the log side, and
//! the linear segment's offset.
//!
//! Port of the renderer helpers of `src/OpenColorIO/ops/log/LogUtils.cpp` @ v2.5.2
//! (`GetLinearSlope`, `GetLogSideBreak`, `GetLinearOffset`). The CTF parameter conversion
//! (`ConvertLogParameters`) comes with WP 1.3l1: its validation messages print doubles with
//! C++ stream formatting.
//!
//! # `GetLogSideBreak` differs between the Windows and Linux wheels
//!
//! `GetLogSideBreak` calls `log2` on `float` arguments. Which overload that is depends on the
//! C++ standard library, and the two wheels differ:
//! - MSVC's `<cmath>` declares the `float` overloads in the global namespace, so the Windows
//!   wheel calls `log2f` and computes the whole expression in `float`.
//! - `LogUtils.cpp` does not include the libstdc++ `<math.h>` wrapper (unlike `LogOpCPU.cpp`,
//!   which gets it through `MathUtils.h`), so with GCC the unqualified `log2` is the C
//!   library's `double log2(double)`: the Linux wheel computes the logarithms and the
//!   multiplication in `double`.
//!
//! Both were read from the wheels' machine code (`docs/spikes/s2-s5.md`). The port does what
//! each platform's wheel does (PLAN.md D12).

use super::log_op_data::{
    LIN_SIDE_BREAK, LIN_SIDE_OFFSET, LIN_SIDE_SLOPE, LINEAR_SLOPE, LOG_SIDE_OFFSET, LOG_SIDE_SLOPE,
    Params,
};

/// The slope of the camera style's linear segment: `LINEAR_SLOPE` if set, otherwise the
/// slope of the log curve at the break, computed in `double`.
///
/// Port of `LogUtil::GetLinearSlope` (src/OpenColorIO/ops/log/LogUtils.cpp:255-268 @ v2.5.2).
pub fn get_linear_slope(params: &Params, base: f64) -> f32 {
    // If value is defined, use it, else compute value.
    if params.len() > LINEAR_SLOPE {
        params[LINEAR_SLOPE] as f32
    } else {
        (params[LOG_SIDE_SLOPE] * params[LIN_SIDE_SLOPE]
            / ((params[LIN_SIDE_SLOPE] * params[LIN_SIDE_BREAK] + params[LIN_SIDE_OFFSET])
                * base.ln())) as f32
    }
}

/// The break on the log side: the log curve at `LIN_SIDE_BREAK`, as each platform's wheel
/// computes it ([`get_log_side_break_msvc`] on Windows, [`get_log_side_break_libstdcxx`]
/// elsewhere).
///
/// Port of `LogUtil::GetLogSideBreak` (src/OpenColorIO/ops/log/LogUtils.cpp:270-281 @ v2.5.2).
pub fn get_log_side_break(params: &Params, base: f64) -> f32 {
    #[cfg(windows)]
    {
        get_log_side_break_msvc(params, base)
    }
    #[cfg(not(windows))]
    {
        get_log_side_break_libstdcxx(params, base)
    }
}

/// `GetLogSideBreak` as MSVC compiles it: `log2` is `log2f`, and every step is in `float`.
///
/// ```text
/// float logSideBreak = log2f((float)(linSlope * linBreak + linOffset));
/// logSideBreak *= (float)logSlope / log2f((float)base);
/// logSideBreak += (float)logOffset;
/// ```
///
/// Port of `LogUtil::GetLogSideBreak` (src/OpenColorIO/ops/log/LogUtils.cpp:270-281 @ v2.5.2),
/// Windows wheel.
pub fn get_log_side_break_msvc(params: &Params, base: f64) -> f32 {
    let mut log_side_break =
        ((params[LIN_SIDE_SLOPE] * params[LIN_SIDE_BREAK] + params[LIN_SIDE_OFFSET]) as f32).log2();
    log_side_break *= params[LOG_SIDE_SLOPE] as f32 / (base as f32).log2();
    log_side_break += params[LOG_SIDE_OFFSET] as f32;
    log_side_break
}

/// `GetLogSideBreak` as GCC and libstdc++ compile it: `log2` is `double log2(double)`, so the
/// `float` arguments are promoted, the quotient is a `double`, and `*=` multiplies in `double`
/// before rounding back to `float`.
///
/// ```text
/// float logSideBreak = (float)log2((double)(float)(linSlope * linBreak + linOffset));
/// logSideBreak = (float)((double)logSideBreak * ((double)(float)logSlope / log2((double)(float)base)));
/// logSideBreak += (float)logOffset;
/// ```
///
/// Port of `LogUtil::GetLogSideBreak` (src/OpenColorIO/ops/log/LogUtils.cpp:270-281 @ v2.5.2),
/// Linux wheel.
pub fn get_log_side_break_libstdcxx(params: &Params, base: f64) -> f32 {
    let lin = (params[LIN_SIDE_SLOPE] * params[LIN_SIDE_BREAK] + params[LIN_SIDE_OFFSET]) as f32;
    let mut log_side_break = log2_glibc_2_2_5(f64::from(lin)) as f32;
    let factor =
        f64::from(params[LOG_SIDE_SLOPE] as f32) / log2_glibc_2_2_5(f64::from(base as f32));
    log_side_break = (f64::from(log_side_break) * factor) as f32;
    log_side_break += params[LOG_SIDE_OFFSET] as f32;
    log_side_break
}

/// `log2` as the Linux wheel links it: `log2@GLIBC_2.2.5` (the wheel was built against a
/// glibc older than 2.29), while Rust links the current `log2@GLIBC_2.29`.
///
/// Both run the same `__ieee754_log2`. They differ only for `x < 0`: the old symbol is the
/// SVID/XOPEN compatibility wrapper `__log2_compat` (glibc `math/w_log2_compat.c`), which
/// returns `__kernel_standard(x, x, 49)`, that is `NAN`, a *positive* quiet NaN; the new one
/// returns the x86 default NaN, which is negative. (`log2(±0)` is `-Inf` in both.) The sign
/// is visible in the camera renderers when `linSlope * linBreak + linOffset < 0`.
fn log2_glibc_2_2_5(x: f64) -> f64 {
    if x < 0.0 {
        f64::from_bits(0x7ff8_0000_0000_0000)
    } else {
        x.log2()
    }
}

/// The offset of the camera style's linear segment, in `float`:
/// `logSideBreak - linearSlope * (float)linSideBreak`.
///
/// Port of `LogUtil::GetLinearOffset` (src/OpenColorIO/ops/log/LogUtils.cpp:283-286 @ v2.5.2).
pub fn get_linear_offset(params: &Params, linear_slope: f32, log_side_break: f32) -> f32 {
    log_side_break - linear_slope * params[LIN_SIDE_BREAK] as f32
}
