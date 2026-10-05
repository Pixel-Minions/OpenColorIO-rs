// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Log helpers: the CTF and CLF log styles and their parameters, converted to the op's
//! (`ConvertLogParameters`), and the camera style's linear segment: its slope, the break on
//! the log side, and its offset.
//!
//! Port of `src/OpenColorIO/ops/log/LogUtils.h` and `LogUtils.cpp` @ v2.5.2.
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
//!
//! The additions and multiplications use [`sse_add`]/[`sse_mul`], so that two NaN operands
//! give the same NaN in every build, in the order the wheels' machine code uses. That is
//! upstream's source order except at the sites whose comments cite the wheels' addresses.
//! Where the two compilers chose different orders and only NaN parameters can reach the site,
//! the port keeps the source order (waiver W0002).

use super::log_op_data::{
    LIN_SIDE_BREAK, LIN_SIDE_OFFSET, LIN_SIDE_SLOPE, LINEAR_SLOPE, LOG_SIDE_OFFSET, LOG_SIDE_SLOPE,
    Params,
};
use crate::cfmt::{Crt, OStringStream};
use crate::exception::{Exception, Result};
use crate::math_utils::{sse_add, sse_mul, std_min};
use crate::open_color_types::TransformDirection;
use crate::platform::strcasecmp;

/// The CTF log styles.
///
/// Port of `LogUtil::LogStyle` (src/OpenColorIO/ops/log/LogUtils.h:17-28 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LogStyle {
    /// `LOG10`: base-10 logarithm.
    Log10 = 0,
    /// `LOG2`: base-2 logarithm.
    Log2,
    /// `ANTI_LOG10`: base-10 anti-logarithm (power).
    AntiLog10,
    /// `ANTI_LOG2`: base-2 anti-logarithm (power).
    AntiLog2,
    /// `LOG_TO_LIN`: Cineon (or similar) log media to scene-linear or video.
    LogToLin,
    /// `LIN_TO_LOG`: scene-linear or video to Cineon (or similar) log media.
    LinToLog,
    /// `CAMERA_LOG_TO_LIN`: log to lin with a linear section near black.
    CameraLogToLin,
    /// `CAMERA_LIN_TO_LOG`: lin to log with a linear section near black.
    CameraLinToLog,
}

/// The styles' names in CLF and CTF files (src/OpenColorIO/ops/log/LogUtils.h:30-38 @ v2.5.2).
pub const LOG2_STR: &str = "log2";
/// See [`LOG2_STR`].
pub const LOG10_STR: &str = "log10";
/// See [`LOG2_STR`].
pub const ANTI_LOG2_STR: &str = "antiLog2";
/// See [`LOG2_STR`].
pub const ANTI_LOG10_STR: &str = "antiLog10";
/// See [`LOG2_STR`].
pub const LIN_TO_LOG_STR: &str = "linToLog";
/// See [`LOG2_STR`].
pub const LOG_TO_LIN_STR: &str = "logToLin";
/// See [`LOG2_STR`].
pub const CAMERA_LIN_TO_LOG_STR: &str = "cameraLinToLog";
/// See [`LOG2_STR`].
pub const CAMERA_LOG_TO_LIN_STR: &str = "cameraLogToLin";

/// What `std::stringstream ss(initial); ss << written;` holds: the stream starts writing at the
/// beginning of its initial text, so `written` overwrites it, and the rest of it stays after
/// (docs/improvements.md, I-52).
fn overwritten(initial: &str, written: &str) -> Vec<u8> {
    let mut bytes = initial.as_bytes().to_vec();
    let written = written.as_bytes();
    let n = written.len().min(bytes.len());
    bytes[..n].copy_from_slice(&written[..n]);
    bytes.extend_from_slice(&written[n..]);
    // A multi-byte character cut in two stays cut: the message is bytes, as upstream's.
    bytes
}

/// The style a name gives, ignoring ASCII case: "Unknown Log style: ..." for any other, as
/// upstream's stream writes it over its first characters (I-52), and "Missing Log style." for
/// none or an empty one.
///
/// Port of `LogUtil::ConvertStringToStyle` (src/OpenColorIO/ops/log/LogUtils.cpp:18-63 @
/// v2.5.2).
pub fn convert_string_to_style(str: Option<&str>) -> Result<LogStyle> {
    if let Some(str) = str
        && !str.is_empty()
    {
        let is = |name: &str| strcasecmp(str, name) == std::cmp::Ordering::Equal;
        return if is(LOG10_STR) {
            Ok(LogStyle::Log10)
        } else if is(LOG2_STR) {
            Ok(LogStyle::Log2)
        } else if is(ANTI_LOG10_STR) {
            Ok(LogStyle::AntiLog10)
        } else if is(ANTI_LOG2_STR) {
            Ok(LogStyle::AntiLog2)
        } else if is(LOG_TO_LIN_STR) {
            Ok(LogStyle::LogToLin)
        } else if is(LIN_TO_LOG_STR) {
            Ok(LogStyle::LinToLog)
        } else if is(CAMERA_LOG_TO_LIN_STR) {
            Ok(LogStyle::CameraLogToLin)
        } else if is(CAMERA_LIN_TO_LOG_STR) {
            Ok(LogStyle::CameraLinToLog)
        } else {
            // `std::stringstream ss("Unknown Log style: '"); ss << str << "'.";`
            Err(Exception::new(overwritten(
                "Unknown Log style: '",
                &format!("{str}'."),
            )))
        };
    }

    Err(Exception::new("Missing Log style."))
}

/// The style's name in CLF and CTF files. (Upstream's error for a value outside the enum
/// can't happen with a Rust enum.)
///
/// Port of `LogUtil::ConvertStyleToString` (src/OpenColorIO/ops/log/LogUtils.cpp:65-91 @
/// v2.5.2).
pub fn convert_style_to_string(style: LogStyle) -> &'static str {
    match style {
        LogStyle::Log10 => LOG10_STR,
        LogStyle::Log2 => LOG2_STR,
        LogStyle::AntiLog10 => ANTI_LOG10_STR,
        LogStyle::AntiLog2 => ANTI_LOG2_STR,
        LogStyle::LogToLin => LOG_TO_LIN_STR,
        LogStyle::LinToLog => LIN_TO_LOG_STR,
        LogStyle::CameraLogToLin => CAMERA_LOG_TO_LIN_STR,
        LogStyle::CameraLinToLog => CAMERA_LIN_TO_LOG_STR,
    }
}

/// The kind of file the CTF parameters come from.
///
/// Port of `LogUtil::CTFParams::Type` (src/OpenColorIO/ops/log/LogUtils.h:51-56 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CtfParamsType {
    /// `UNKNOWN`.
    Unknown,
    /// `CINEON`.
    Cineon,
    /// `CLF`.
    Clf,
}

/// A channel of the CTF parameters.
///
/// Port of `LogUtil::CTFParams::Channels` (src/OpenColorIO/ops/log/LogUtils.h:59-64 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CtfChannel {
    /// `red`.
    Red = 0,
    /// `green`.
    Green,
    /// `blue`.
    Blue,
}

/// The index of each legacy CTF parameter.
///
/// Port of `LogUtil::CTFParams::Values` (src/OpenColorIO/ops/log/LogUtils.h:66-73 @ v2.5.2).
pub mod ctf_values {
    /// `gamma`.
    pub const GAMMA: usize = 0;
    /// `refWhite`.
    pub const REF_WHITE: usize = 1;
    /// `refBlack`.
    pub const REF_BLACK: usize = 2;
    /// `highlight`.
    pub const HIGHLIGHT: usize = 3;
    /// `shadow`.
    pub const SHADOW: usize = 4;
}

/// A Log element's parameters as a CTF or CLF file gives them: its style, and for the legacy
/// styles `[gamma, refWhite, refBlack, highlight, shadow]` per channel.
///
/// Port of `LogUtil::CTFParams` (src/OpenColorIO/ops/log/LogUtils.h:43-111 @ v2.5.2).
#[derive(Debug, Clone, PartialEq)]
pub struct CtfParams {
    /// `m_style`.
    pub style: LogStyle,
    /// `m_params`: red, green, blue.
    pub params: [Vec<f64>; 3],
    /// `m_type`.
    type_: CtfParamsType,
}

impl Default for CtfParams {
    fn default() -> Self {
        CtfParams {
            style: LogStyle::Log10,
            params: [vec![0.; 5], vec![0.; 5], vec![0.; 5]],
            type_: CtfParamsType::Unknown,
        }
    }
}

impl CtfParams {
    /// Port of `CTFParams::get` (src/OpenColorIO/ops/log/LogUtils.h:75-83 @ v2.5.2).
    pub fn get(&self, c: CtfChannel) -> &Vec<f64> {
        &self.params[c as usize]
    }

    /// Port of `CTFParams::get` (src/OpenColorIO/ops/log/LogUtils.h:75-78 @ v2.5.2).
    pub fn get_mut(&mut self, c: CtfChannel) -> &mut Vec<f64> {
        &mut self.params[c as usize]
    }

    /// Sets the type the first time; afterwards, only the same type is accepted.
    ///
    /// Port of `CTFParams::setType` (src/OpenColorIO/ops/log/LogUtils.h:91-102 @ v2.5.2).
    pub fn set_type(&mut self, type_: CtfParamsType) -> bool {
        if self.type_ == CtfParamsType::Unknown {
            self.type_ = type_;
        } else if type_ != self.type_ {
            return false;
        }
        true
    }

    /// Port of `CTFParams::getType` (src/OpenColorIO/ops/log/LogUtils.h:104-107 @ v2.5.2).
    pub fn get_type(&self) -> CtfParamsType {
        self.type_
    }
}

/// The op's parameters of one channel from the legacy CTF ones: base 10, in `double`.
///
/// Port of `LogUtil::ConvertFromCTFToOCIO` (src/OpenColorIO/ops/log/LogUtils.cpp:94-122 @
/// v2.5.2).
fn convert_from_ctf_to_ocio(ctf_params: &[f64], ocio_params: &mut Params) {
    // Base is 10.0.
    const RANGE: f64 = 0.002 * 1023.0;

    let gamma = ctf_params[ctf_values::GAMMA];
    let ref_white = ctf_params[ctf_values::REF_WHITE] / 1023.0;
    let ref_black = ctf_params[ctf_values::REF_BLACK] / 1023.0;
    let highlight = ctf_params[ctf_values::HIGHLIGHT];
    let shadow = ctf_params[ctf_values::SHADOW];

    let mult_factor = RANGE / gamma;

    let mut tmp_value = (ref_black - ref_white) * mult_factor;
    // The exact clamp value is not critical. RefBlack and RefWhite are never very close to one
    // another in practice. We just need to avoid a div by 0 in the gain calculation.
    tmp_value = std_min(tmp_value, -0.0001);

    let gain = (highlight - shadow) / (1. - 10.0f64.powf(tmp_value));
    let offset = gain - (highlight - shadow);

    ocio_params[LOG_SIDE_SLOPE] = 1. / mult_factor;
    ocio_params[LIN_SIDE_SLOPE] = 1. / gain;
    ocio_params[LIN_SIDE_OFFSET] = (offset - shadow) / gain;
    ocio_params[LOG_SIDE_OFFSET] = ref_white;
}

/// Checks one channel's legacy CTF parameters: 5 of them, `gamma > 0.01` (a `float` 0.01),
/// `refWhite > refBlack` and `highlight > shadow`, with upstream's messages.
///
/// Port of `LogUtil::ValidateLegacyParams` (src/OpenColorIO/ops/log/LogUtils.cpp:124-168 @
/// v2.5.2).
fn validate_legacy_params(ctf_params: &[f64]) -> Result<()> {
    // Params vector is [ gamma, refWhite, refBlack, highlight, shadow ].
    const EXPECTED_SIZE: usize = 5;
    if ctf_params.len() != EXPECTED_SIZE {
        return Err(Exception::new("Log: Expecting 5 parameters."));
    }

    let gamma = ctf_params[ctf_values::GAMMA];
    let ref_white = ctf_params[ctf_values::REF_WHITE];
    let ref_black = ctf_params[ctf_values::REF_BLACK];
    let highlight = ctf_params[ctf_values::HIGHLIGHT];
    let shadow = ctf_params[ctf_values::SHADOW];

    let message = |parts: &[(&str, Option<f64>)]| {
        let mut oss = OStringStream::new(Crt::NATIVE);
        for (text, value) in parts {
            oss.put_str(text);
            if let Some(value) = value {
                oss.put_f64(*value);
            }
        }
        Exception::new(oss.into_bytes())
    };

    // gamma > 0.01.
    let valid = gamma > f64::from(0.01f32);
    if !valid {
        return Err(message(&[
            ("Log: Invalid gamma value '", Some(gamma)),
            ("', gamma should be greater than 0.01.", None),
        ]));
    }

    // refWhite > refBlack.
    let valid = ref_white > ref_black;
    if !valid {
        return Err(message(&[
            ("Log: Invalid refWhite '", Some(ref_white)),
            ("' and refBlack '", Some(ref_black)),
            ("', refWhite should be greater than refBlack.", None),
        ]));
    }

    // highlight > shadow.
    let valid = highlight > shadow;
    if !valid {
        return Err(message(&[
            ("Log: Invalid highlight '", Some(highlight)),
            ("' and shadow '", Some(shadow)),
            ("', highlight should be greater than shadow.", None),
        ]));
    }
    Ok(())
}

/// The op's base and parameters for a CTF or CLF Log element: the plain logarithms and
/// anti-logarithms keep the default parameters (base 10 or 2), the legacy styles convert their
/// parameters (base 10), after checking them; the camera styles leave them to their reader.
///
/// Port of `LogUtil::ConvertLogParameters` (src/OpenColorIO/ops/log/LogUtils.cpp:170-235 @
/// v2.5.2).
pub fn convert_log_parameters(
    ctf_params: &CtfParams,
    base: &mut f64,
    red_params: &mut Params,
    green_params: &mut Params,
    blue_params: &mut Params,
) -> Result<()> {
    for p in [&mut *red_params, &mut *green_params, &mut *blue_params] {
        p.resize(4, 0.0);
        p[LOG_SIDE_SLOPE] = 1.;
        p[LIN_SIDE_SLOPE] = 1.;
        p[LIN_SIDE_OFFSET] = 0.;
        p[LOG_SIDE_OFFSET] = 0.;
    }

    match ctf_params.style {
        // out = log(in) / log(10); keep default values.
        LogStyle::Log10 => *base = 10.0,
        // out = log(in) / log(2); keep default values but base.
        LogStyle::Log2 => *base = 2.,
        // out = pow(10, in); keep default values but direction.
        LogStyle::AntiLog10 => *base = 10.0,
        // out = pow(2, in); keep default values but direction and base.
        LogStyle::AntiLog2 => *base = 2.,
        // LIN_TO_LOG: out = k3 * log(m3 * in + b3) / log(base3) + kb3, base and direction to
        // default values; LOG_TO_LIN: out = ( pow(base3, (in - kb3) / k3) - b3 ) / m3, base to
        // default values.
        LogStyle::LinToLog | LogStyle::LogToLin => {
            *base = 10.0;
            validate_legacy_params(ctf_params.get(CtfChannel::Red))?;
            validate_legacy_params(ctf_params.get(CtfChannel::Green))?;
            validate_legacy_params(ctf_params.get(CtfChannel::Blue))?;
            convert_from_ctf_to_ocio(ctf_params.get(CtfChannel::Red), red_params);
            convert_from_ctf_to_ocio(ctf_params.get(CtfChannel::Green), green_params);
            convert_from_ctf_to_ocio(ctf_params.get(CtfChannel::Blue), blue_params);
        }
        // Should not be used for new style.
        LogStyle::CameraLinToLog | LogStyle::CameraLogToLin => {}
    }
    Ok(())
}

/// The direction of a style: the logarithms forward, the anti-logarithms inverse.
///
/// Port of `LogUtil::GetLogDirection` (src/OpenColorIO/ops/log/LogUtils.cpp:237-253 @ v2.5.2).
pub fn get_log_direction(style: LogStyle) -> TransformDirection {
    match style {
        LogStyle::Log10 | LogStyle::Log2 | LogStyle::LinToLog | LogStyle::CameraLinToLog => {
            TransformDirection::Forward
        }
        LogStyle::AntiLog10
        | LogStyle::AntiLog2
        | LogStyle::LogToLin
        | LogStyle::CameraLogToLin => TransformDirection::Inverse,
    }
}

/// `linSlope * linBreak + linOffset`, in `double`: the argument of the logarithm at the break.
fn log_argument_at_break(params: &Params) -> f64 {
    sse_add(
        sse_mul(params[LIN_SIDE_SLOPE], params[LIN_SIDE_BREAK]),
        params[LIN_SIDE_OFFSET],
    )
}

/// The slope of the camera style's linear segment: `LINEAR_SLOPE` if set, otherwise the
/// slope of the log curve at the break, computed in `double`, as each platform's wheel
/// computes it ([`get_linear_slope_msvc`] on Windows, [`get_linear_slope_libstdcxx`] on
/// Linux; the crate does not build for other targets).
///
/// Port of `LogUtil::GetLinearSlope` (src/OpenColorIO/ops/log/LogUtils.cpp:255-268 @ v2.5.2).
pub fn get_linear_slope(params: &Params, base: f64) -> f32 {
    #[cfg(target_os = "windows")]
    {
        get_linear_slope_msvc(params, base)
    }
    #[cfg(target_os = "linux")]
    {
        get_linear_slope_libstdcxx(params, base)
    }
}

/// `GetLinearSlope` as MSVC compiles it: the numerator is `linSlope * logSlope` (the Windows
/// wheel at 0x18021abde: `movaps xmm7, xmm6` holds `linSlope`, then `mulsd xmm7, [rbx]`
/// multiplies by `logSlope`), so where both are NaN, the linear side slope's NaN is kept (I-70).
///
/// Port of `LogUtil::GetLinearSlope` (src/OpenColorIO/ops/log/LogUtils.cpp:255-268 @ v2.5.2),
/// Windows wheel.
pub fn get_linear_slope_msvc(params: &Params, base: f64) -> f32 {
    linear_slope_with(params, base, |log_slope, lin_slope| {
        sse_mul(lin_slope, log_slope)
    })
}

/// `GetLinearSlope` as GCC compiles it: the numerator is `logSlope * linSlope`, the source's
/// order (the Linux wheel at 0x40176d: `mulsd xmm1, xmm2`, `xmm1` holding `logSlope`), so where
/// both are NaN, the log side slope's NaN is kept (I-70).
///
/// Port of `LogUtil::GetLinearSlope` (src/OpenColorIO/ops/log/LogUtils.cpp:255-268 @ v2.5.2),
/// Linux wheel.
pub fn get_linear_slope_libstdcxx(params: &Params, base: f64) -> f32 {
    linear_slope_with(params, base, |log_slope, lin_slope| {
        sse_mul(log_slope, lin_slope)
    })
}

/// `GetLinearSlope` with the numerator's product `numerator(logSlope, linSlope)`.
fn linear_slope_with(params: &Params, base: f64, numerator: fn(f64, f64) -> f64) -> f32 {
    // If value is defined, use it, else compute value.
    if params.len() > LINEAR_SLOPE {
        params[LINEAR_SLOPE] as f32
    } else {
        // logSlope * linSlope / ((linSlope * linBreak + linOffset) * log(base)). Both wheels
        // multiply `log(base) * (...)` (Windows at 0x18021abf1, Linux at 0x40178b) and divide
        // the numerator by it.
        (numerator(params[LOG_SIDE_SLOPE], params[LIN_SIDE_SLOPE])
            / sse_mul(base.ln(), log_argument_at_break(params))) as f32
    }
}

/// The break on the log side: the log curve at `LIN_SIDE_BREAK`, as each platform's wheel
/// computes it ([`get_log_side_break_msvc`] on Windows, [`get_log_side_break_libstdcxx`] on
/// Linux; the crate does not build for other targets).
///
/// Port of `LogUtil::GetLogSideBreak` (src/OpenColorIO/ops/log/LogUtils.cpp:270-281 @ v2.5.2).
pub fn get_log_side_break(params: &Params, base: f64) -> f32 {
    #[cfg(target_os = "windows")]
    {
        get_log_side_break_msvc(params, base)
    }
    #[cfg(target_os = "linux")]
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
    let mut log_side_break = (log_argument_at_break(params) as f32).log2();
    log_side_break = sse_mul(
        log_side_break,
        params[LOG_SIDE_SLOPE] as f32 / (base as f32).log2(),
    );
    sse_add(log_side_break, params[LOG_SIDE_OFFSET] as f32)
}

/// `GetLogSideBreak` as GCC and libstdc++ compile it: `log2` is `double log2(double)`, so the
/// `float` arguments are promoted, the quotient is a `double`, and `*=` multiplies in `double`
/// before rounding back to `float`.
///
/// ```text
/// float logSideBreak = (float)log2((double)(float)(linSlope * linBreak + linOffset));
/// double factor = (double)(float)logSlope / log2((double)(float)base);
/// logSideBreak = (float)(factor * (double)logSideBreak);
/// logSideBreak = (float)logOffset + logSideBreak;
/// ```
///
/// GCC swapped the operands of `*=` and `+=` (the Linux wheel's `LogUtil::GetLogSideBreak` at
/// 0x4017b0). It matters when two NaNs meet: the glibc `log2` below returns a positive NaN for
/// a negative argument, and finite parameters that overflow make the factor a negative NaN.
///
/// Port of `LogUtil::GetLogSideBreak` (src/OpenColorIO/ops/log/LogUtils.cpp:270-281 @ v2.5.2),
/// Linux wheel.
pub fn get_log_side_break_libstdcxx(params: &Params, base: f64) -> f32 {
    let lin = log_argument_at_break(params) as f32;
    let mut log_side_break = log2_glibc_2_2_5(f64::from(lin)) as f32;
    let factor =
        f64::from(params[LOG_SIDE_SLOPE] as f32) / log2_glibc_2_2_5(f64::from(base as f32));
    log_side_break = sse_mul(factor, f64::from(log_side_break)) as f32;
    sse_add(params[LOG_SIDE_OFFSET] as f32, log_side_break)
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
/// `logSideBreak - linearSlope * (float)linSideBreak`. Both wheels multiply
/// `(float)linSideBreak * linearSlope` (Windows at 0x18021ab8c, Linux at 0x40185f).
///
/// Port of `LogUtil::GetLinearOffset` (src/OpenColorIO/ops/log/LogUtils.cpp:283-286 @ v2.5.2).
pub fn get_linear_offset(params: &Params, linear_slope: f32, log_side_break: f32) -> f32 {
    log_side_break - sse_mul(params[LIN_SIDE_BREAK] as f32, linear_slope)
}

#[cfg(test)]
#[path = "log_utils_tests.rs"]
mod tests;
