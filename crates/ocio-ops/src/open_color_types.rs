// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The public enums the ops need, from `include/OpenColorIO/OpenColorTypes.h` @ v2.5.2, and
//! their helpers from `src/OpenColorIO/ParseUtils.cpp`.
//!
//! They live in `ocio-ops` because op data uses them; the public `ocio` crate re-exports them.
//! So far: `LoggingLevel`, `TransformDirection`, `NegativeStyle`, `DynamicPropertyType`, `BitDepth`,
//! `ChannelOrdering` and `Allocation`.

use crate::utils::string_utils::lower_c_str;

/// How much OCIO logs (`crate::logging`). The discriminants are upstream's, and levels
/// compare by them: `Unknown` (255) logs everything, as `Debug` does.
///
/// Port of `LoggingLevel` (include/OpenColorIO/OpenColorTypes.h:288-297 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LoggingLevel {
    /// `LOGGING_LEVEL_NONE`.
    None = 0,
    /// `LOGGING_LEVEL_WARNING`: warnings and errors.
    Warning = 1,
    /// `LOGGING_LEVEL_INFO`: and information.
    Info = 2,
    /// `LOGGING_LEVEL_DEBUG`: and debugging messages.
    Debug = 3,
    /// `LOGGING_LEVEL_UNKNOWN`.
    Unknown = 255,
}

impl LoggingLevel {
    /// `LOGGING_LEVEL_DEFAULT`.
    pub const DEFAULT: LoggingLevel = LoggingLevel::Info;
}

/// The level's name: `none`, `warning`, `info`, `debug` or `unknown`.
///
/// Port of `LoggingLevelToString` (src/OpenColorIO/ParseUtils.cpp:113-120 @ v2.5.2).
pub fn logging_level_to_string(level: LoggingLevel) -> &'static str {
    match level {
        LoggingLevel::None => "none",
        LoggingLevel::Warning => "warning",
        LoggingLevel::Info => "info",
        LoggingLevel::Debug => "debug",
        LoggingLevel::Unknown => "unknown",
    }
}

/// The level named `s` (`none`, `warning`, `info` or `debug`, in any ASCII case, or `0` to `3`),
/// or `Unknown`. `None` is a null pointer, which gives `Unknown`.
///
/// Port of `LoggingLevelFromString` (src/OpenColorIO/ParseUtils.cpp:122-131 @ v2.5.2).
pub fn logging_level_from_string(s: Option<&[u8]>) -> LoggingLevel {
    let s = lower_c_str(s);
    match s.as_slice() {
        b"0" | b"none" => LoggingLevel::None,
        b"1" | b"warning" => LoggingLevel::Warning,
        b"2" | b"info" => LoggingLevel::Info,
        b"3" | b"debug" => LoggingLevel::Debug,
        _ => LoggingLevel::Unknown,
    }
}

/// Port of `TransformDirection` (include/OpenColorIO/OpenColorTypes.h:355-359 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TransformDirection {
    /// `TRANSFORM_DIR_FORWARD`.
    #[default]
    Forward = 0,
    /// `TRANSFORM_DIR_INVERSE`.
    Inverse,
}

/// The other direction.
///
/// Port of `GetInverseTransformDirection` (src/OpenColorIO/ParseUtils.cpp:164-169 @ v2.5.2).
pub fn get_inverse_transform_direction(dir: TransformDirection) -> TransformDirection {
    if dir == TransformDirection::Forward {
        return TransformDirection::Inverse;
    }
    // TRANSFORM_DIR_INVERSE
    TransformDirection::Forward
}

/// Forward when both directions agree, inverse otherwise.
///
/// Port of `CombineTransformDirections` (src/OpenColorIO/ParseUtils.cpp:153-162 @ v2.5.2).
pub fn combine_transform_directions(
    d1: TransformDirection,
    d2: TransformDirection,
) -> TransformDirection {
    if d1 == TransformDirection::Forward && d2 == TransformDirection::Forward {
        return TransformDirection::Forward;
    }

    if d1 == TransformDirection::Inverse && d2 == TransformDirection::Inverse {
        return TransformDirection::Forward;
    }

    TransformDirection::Inverse
}

/// How an exponent or curve handles negative values.
///
/// Port of `NegativeStyle` (include/OpenColorIO/OpenColorTypes.h:552-558 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NegativeStyle {
    /// `NEGATIVE_CLAMP`: clamp negative values.
    Clamp = 0,
    /// `NEGATIVE_MIRROR`: the positive curve is rotated 180 degrees around the origin.
    Mirror,
    /// `NEGATIVE_PASS_THRU`: negative values are passed through unchanged.
    PassThru,
    /// `NEGATIVE_LINEAR`: linearly extrapolate the curve for negative values.
    Linear,
}

/// What a dynamic property holds: a double for the first three, a grading value for the
/// others.
///
/// Port of `DynamicPropertyType` (include/OpenColorIO/OpenColorTypes.h:568-578 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DynamicPropertyType {
    /// `DYNAMIC_PROPERTY_EXPOSURE`: image exposure (a double).
    Exposure = 0,
    /// `DYNAMIC_PROPERTY_CONTRAST`: image contrast (a double).
    Contrast,
    /// `DYNAMIC_PROPERTY_GAMMA`: image gamma (a double).
    Gamma,
    /// `DYNAMIC_PROPERTY_GRADING_PRIMARY`: used by `GradingPrimaryTransform`.
    GradingPrimary,
    /// `DYNAMIC_PROPERTY_GRADING_RGBCURVE`: used by `GradingRGBCurveTransform`.
    GradingRgbCurve,
    /// `DYNAMIC_PROPERTY_GRADING_TONE`: used by `GradingToneTransform`.
    GradingTone,
    /// `DYNAMIC_PROPERTY_GRADING_HUECURVE`: used by `GradingHueCurveTransform`.
    GradingHueCurve,
}

/// The bit depth of a color space, or of the images a CPU processor reads and writes. The
/// processor supports only `Uint8`, `Uint10`, `Uint12`, `Uint16`, `F16` and `F32`; the other
/// enumerators exist for upstream's API and are rejected where a supported one is needed.
///
/// The discriminants are upstream's enumerator values.
///
/// Port of `BitDepth` (include/OpenColorIO/OpenColorTypes.h:422-439 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BitDepth {
    /// `BIT_DEPTH_UNKNOWN`.
    Unknown = 0,
    /// `BIT_DEPTH_UINT8`.
    Uint8,
    /// `BIT_DEPTH_UINT10`.
    Uint10,
    /// `BIT_DEPTH_UINT12`.
    Uint12,
    /// `BIT_DEPTH_UINT14`.
    Uint14,
    /// `BIT_DEPTH_UINT16`.
    Uint16,
    /// `BIT_DEPTH_UINT32`: here for historical reasons, but not supported.
    Uint32,
    /// `BIT_DEPTH_F16`.
    F16,
    /// `BIT_DEPTH_F32`.
    F32,
}

/// The order of the channels in the pixels of a packed image.
///
/// Port of `ChannelOrdering` (include/OpenColorIO/OpenColorTypes.h:450-457 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChannelOrdering {
    /// `CHANNEL_ORDERING_RGBA`.
    Rgba = 0,
    /// `CHANNEL_ORDERING_BGRA`.
    Bgra,
    /// `CHANNEL_ORDERING_ABGR`.
    Abgr,
    /// `CHANNEL_ORDERING_RGB`.
    Rgb,
    /// `CHANNEL_ORDERING_BGR`.
    Bgr,
}

/// The bit depth's name in configs and error messages: `8ui`, `10ui`, `12ui`, `14ui`, `16ui`,
/// `32ui`, `16f`, `32f`, or `unknown`.
///
/// Port of `BitDepthToString` (src/OpenColorIO/ParseUtils.cpp:171-182 @ v2.5.2).
pub fn bit_depth_to_string(bit_depth: BitDepth) -> &'static str {
    match bit_depth {
        BitDepth::Uint8 => "8ui",
        BitDepth::Uint10 => "10ui",
        BitDepth::Uint12 => "12ui",
        BitDepth::Uint14 => "14ui",
        BitDepth::Uint16 => "16ui",
        BitDepth::Uint32 => "32ui",
        BitDepth::F16 => "16f",
        BitDepth::F32 => "32f",
        BitDepth::Unknown => "unknown",
    }
}

/// How a color space's values are spread over the range the GPU's legacy 3D LUT samples:
/// uniformly, or on a log2 scale.
///
/// Port of `Allocation` (include/OpenColorIO/OpenColorTypes.h:459-463 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Allocation {
    /// `ALLOCATION_UNKNOWN`.
    Unknown = 0,
    /// `ALLOCATION_UNIFORM`.
    Uniform,
    /// `ALLOCATION_LG2`.
    Lg2,
}

/// The allocation's name in configs: `uniform`, `lg2` or `unknown`.
///
/// Port of `AllocationToString` (src/OpenColorIO/ParseUtils.cpp:218-223 @ v2.5.2).
pub fn allocation_to_string(allocation: Allocation) -> &'static str {
    match allocation {
        Allocation::Uniform => "uniform",
        Allocation::Lg2 => "lg2",
        Allocation::Unknown => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use TransformDirection::{Forward, Inverse};

    #[test]
    fn directions() {
        assert_eq!(get_inverse_transform_direction(Forward), Inverse);
        assert_eq!(get_inverse_transform_direction(Inverse), Forward);
        assert_eq!(combine_transform_directions(Forward, Forward), Forward);
        assert_eq!(combine_transform_directions(Inverse, Inverse), Forward);
        assert_eq!(combine_transform_directions(Forward, Inverse), Inverse);
        assert_eq!(combine_transform_directions(Inverse, Forward), Inverse);
    }
}
