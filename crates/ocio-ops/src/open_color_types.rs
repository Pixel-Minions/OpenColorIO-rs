// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The public enums the ops need, from `include/OpenColorIO/OpenColorTypes.h` @ v2.5.2, and
//! their helpers from `src/OpenColorIO/ParseUtils.cpp`.
//!
//! They live in `ocio-ops` because op data uses them; the public `ocio` crate re-exports them.
//! So far: `LoggingLevel`, `TransformDirection`, `NegativeStyle`, `DynamicPropertyType`, `BitDepth`,
//! `ChannelOrdering`, `Allocation`, `FixedFunctionStyle` and `OptimizationFlags`.

use core::ffi::c_ulong;
use std::ops::{BitAnd, BitOr};

use crate::exception::{Exception, Result};
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

/// The direction's name in configs and cache IDs: `forward` or `inverse`.
///
/// Port of `TransformDirectionToString` (src/OpenColorIO/ParseUtils.cpp:133-138 @ v2.5.2).
pub fn transform_direction_to_string(dir: TransformDirection) -> &'static str {
    if dir == TransformDirection::Forward {
        return "forward";
    }
    // TRANSFORM_DIR_INVERSE
    "inverse"
}

/// The style of a CDL: the ASC v1.2 specification, which clamps, or no clamping. The default
/// for reading .cc/.ccc/.cdl files, config YAML and the `CDLTransform` is no-clamp; the CLF
/// format's default is ASC.
///
/// Port of `CDLStyle` (include/OpenColorIO/OpenColorTypes.h:533-546 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CdlStyle {
    /// `CDL_ASC`: ASC CDL specification v1.2.
    Asc = 0,
    /// `CDL_NO_CLAMP`: a CDL that does not clamp.
    NoClamp,
}

impl CdlStyle {
    /// `CDL_TRANSFORM_DEFAULT`, which is `CDL_NO_CLAMP`.
    pub const TRANSFORM_DEFAULT: CdlStyle = CdlStyle::NoClamp;
}

/// The style's name: `asc` or `noClamp`.
///
/// Port of `CDLStyleToString` (src/OpenColorIO/ParseUtils.cpp:314-319 @ v2.5.2). Its fallback
/// for a value outside the enum can't happen.
pub fn cdl_style_to_string(style: CdlStyle) -> &'static str {
    match style {
        CdlStyle::Asc => "asc",
        CdlStyle::NoClamp => "noClamp",
    }
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

/// The negative style's name: `clamp`, `mirror`, `pass_thru` or `linear`.
///
/// Port of `NegativeStyleToString` (src/OpenColorIO/ParseUtils.cpp:496-513 @ v2.5.2), with the
/// `NEGATIVE_STYLE_*` names. Its "Unknown exponent style" for a value outside the enum can't
/// happen.
pub fn negative_style_to_string(style: NegativeStyle) -> &'static str {
    match style {
        NegativeStyle::Clamp => "clamp",
        NegativeStyle::Mirror => "mirror",
        NegativeStyle::PassThru => "pass_thru",
        NegativeStyle::Linear => "linear",
    }
}

/// The algorithms of a `FixedFunctionTransform`. The discriminants are upstream's.
///
/// Port of `FixedFunctionStyle` (include/OpenColorIO/OpenColorTypes.h:499-524 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FixedFunctionStyle {
    /// `FIXED_FUNCTION_ACES_RED_MOD_03`: red modifier (ACES 0.3/0.7).
    #[doc(alias = "FIXED_FUNCTION_ACES_RED_MOD_03")]
    AcesRedMod03 = 0,
    /// `FIXED_FUNCTION_ACES_RED_MOD_10`: red modifier (ACES 1.0).
    #[doc(alias = "FIXED_FUNCTION_ACES_RED_MOD_10")]
    AcesRedMod10,
    /// `FIXED_FUNCTION_ACES_GLOW_03`: glow function (ACES 0.3/0.7).
    #[doc(alias = "FIXED_FUNCTION_ACES_GLOW_03")]
    AcesGlow03,
    /// `FIXED_FUNCTION_ACES_GLOW_10`: glow function (ACES 1.0).
    #[doc(alias = "FIXED_FUNCTION_ACES_GLOW_10")]
    AcesGlow10,
    /// `FIXED_FUNCTION_ACES_DARK_TO_DIM_10`: dark to dim surround correction (ACES 1.0).
    #[doc(alias = "FIXED_FUNCTION_ACES_DARK_TO_DIM_10")]
    AcesDarkToDim10,
    /// `FIXED_FUNCTION_REC2100_SURROUND`: Rec.2100 surround correction (takes one double for
    /// the gamma param).
    #[doc(alias = "FIXED_FUNCTION_REC2100_SURROUND")]
    Rec2100Surround,
    /// `FIXED_FUNCTION_RGB_TO_HSV`: classic RGB to HSV function.
    #[doc(alias = "FIXED_FUNCTION_RGB_TO_HSV")]
    RgbToHsv,
    /// `FIXED_FUNCTION_XYZ_TO_xyY`: CIE XYZ to 1931 xy chromaticity coordinates.
    #[doc(alias = "FIXED_FUNCTION_XYZ_TO_xyY")]
    XyzToXyy,
    /// `FIXED_FUNCTION_XYZ_TO_uvY`: CIE XYZ to 1976 u'v' chromaticity coordinates.
    #[doc(alias = "FIXED_FUNCTION_XYZ_TO_uvY")]
    XyzToUvy,
    /// `FIXED_FUNCTION_XYZ_TO_LUV`: CIE XYZ to 1976 CIELUV colour space (D65 white).
    #[doc(alias = "FIXED_FUNCTION_XYZ_TO_LUV")]
    XyzToLuv,
    /// `FIXED_FUNCTION_ACES_GAMUTMAP_02`: ACES 0.2 gamut clamping algorithm, not implemented
    /// upstream: refused.
    #[doc(alias = "FIXED_FUNCTION_ACES_GAMUTMAP_02")]
    AcesGamutMap02,
    /// `FIXED_FUNCTION_ACES_GAMUTMAP_07`: ACES 0.7 gamut clamping algorithm, not implemented
    /// upstream: refused.
    #[doc(alias = "FIXED_FUNCTION_ACES_GAMUTMAP_07")]
    AcesGamutMap07,
    /// `FIXED_FUNCTION_ACES_GAMUT_COMP_13`: ACES 1.3 parametric gamut compression (expects
    /// ACEScg values).
    #[doc(alias = "FIXED_FUNCTION_ACES_GAMUT_COMP_13")]
    AcesGamutComp13,
    /// `FIXED_FUNCTION_LIN_TO_PQ`: SMPTE ST-2084 OETF, scaled with 100 nits at 1.0 (negative
    /// values mirrored).
    #[doc(alias = "FIXED_FUNCTION_LIN_TO_PQ")]
    LinToPq,
    /// `FIXED_FUNCTION_LIN_TO_GAMMA_LOG`: parametrized gamma and log segments with mirroring.
    #[doc(alias = "FIXED_FUNCTION_LIN_TO_GAMMA_LOG")]
    LinToGammaLog,
    /// `FIXED_FUNCTION_LIN_TO_DOUBLE_LOG`: two parameterized LogAffineTransforms with a middle
    /// linear segment.
    #[doc(alias = "FIXED_FUNCTION_LIN_TO_DOUBLE_LOG")]
    LinToDoubleLog,
    /// `FIXED_FUNCTION_ACES_OUTPUT_TRANSFORM_20`: ACES 2.0 display rendering.
    #[doc(alias = "FIXED_FUNCTION_ACES_OUTPUT_TRANSFORM_20")]
    AcesOutputTransform20,
    /// `FIXED_FUNCTION_ACES_RGB_TO_JMH_20`: ACES 2.0 RGB to JMh.
    #[doc(alias = "FIXED_FUNCTION_ACES_RGB_TO_JMH_20")]
    AcesRgbToJmh20,
    /// `FIXED_FUNCTION_ACES_TONESCALE_COMPRESS_20`: ACES 2.0 tonescale and chroma compression.
    #[doc(alias = "FIXED_FUNCTION_ACES_TONESCALE_COMPRESS_20")]
    AcesTonescaleCompress20,
    /// `FIXED_FUNCTION_ACES_GAMUT_COMPRESS_20`: ACES 2.0 gamut compression.
    #[doc(alias = "FIXED_FUNCTION_ACES_GAMUT_COMPRESS_20")]
    AcesGamutCompress20,
    /// `FIXED_FUNCTION_RGB_TO_HSY_LIN`: RGB to HSY (hue, saturation, luminance) for linear
    /// spaces.
    #[doc(alias = "FIXED_FUNCTION_RGB_TO_HSY_LIN")]
    RgbToHsyLin,
    /// `FIXED_FUNCTION_RGB_TO_HSY_LOG`: RGB to HSY (hue, saturation, luma) for log spaces.
    #[doc(alias = "FIXED_FUNCTION_RGB_TO_HSY_LOG")]
    RgbToHsyLog,
    /// `FIXED_FUNCTION_RGB_TO_HSY_VID`: RGB to HSY (hue, saturation, luma) for video spaces.
    #[doc(alias = "FIXED_FUNCTION_RGB_TO_HSY_VID")]
    RgbToHsyVid,
}

/// The error for the two styles upstream doesn't implement, `FIXED_FUNCTION_ACES_GAMUTMAP_02`
/// and `_07` (src/OpenColorIO/ParseUtils.cpp:377-381, ops/fixedfunction/FixedFunctionOpData.cpp:
/// 500-506 @ v2.5.2).
pub const UNIMPLEMENTED_GAMUTMAP: &str = "Unimplemented fixed function types: \
     FIXED_FUNCTION_ACES_GAMUTMAP_02, FIXED_FUNCTION_ACES_GAMUTMAP_07.";

/// The style's name: `ACES_RedMod03`, ..., `ACES2_OutputTransform`, ..., `RGB_TO_HSY_VID`. The
/// two styles upstream doesn't implement are refused ([`UNIMPLEMENTED_GAMUTMAP`]).
///
/// Port of `FixedFunctionStyleToString` (src/OpenColorIO/ParseUtils.cpp:354-388 @ v2.5.2). Its
/// "Unknown Fixed FunctionOp style" for a value outside the enum can't happen.
pub fn fixed_function_style_to_string(style: FixedFunctionStyle) -> Result<&'static str> {
    use FixedFunctionStyle::*;
    Ok(match style {
        AcesRedMod03 => "ACES_RedMod03",
        AcesRedMod10 => "ACES_RedMod10",
        AcesGlow03 => "ACES_Glow03",
        AcesGlow10 => "ACES_Glow10",
        AcesDarkToDim10 => "ACES_DarkToDim10",
        AcesGamutComp13 => "ACES_GamutComp13",
        AcesOutputTransform20 => "ACES2_OutputTransform",
        AcesRgbToJmh20 => "ACES2_RGB_TO_JMh",
        AcesTonescaleCompress20 => "ACES2_TonescaleCompress",
        AcesGamutCompress20 => "ACES2_GamutCompress",
        Rec2100Surround => "REC2100_Surround",
        RgbToHsv => "RGB_TO_HSV",
        XyzToXyy => "XYZ_TO_xyY",
        XyzToUvy => "XYZ_TO_uvY",
        XyzToLuv => "XYZ_TO_LUV",
        LinToPq => "Lin_TO_PQ",
        LinToGammaLog => "Lin_TO_GammaLog",
        LinToDoubleLog => "Lin_TO_DoubleLog",
        RgbToHsyLin => "RGB_TO_HSY_LIN",
        RgbToHsyLog => "RGB_TO_HSY_LOG",
        RgbToHsyVid => "RGB_TO_HSY_VID",
        AcesGamutMap02 | AcesGamutMap07 => {
            return Err(Exception::new(UNIMPLEMENTED_GAMUTMAP));
        }
    })
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

/// The hue restoration a 1D LUT applies.
///
/// Port of `enum Lut1DHueAdjust` (include/OpenColorIO/OpenColorTypes.h:441-447 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lut1DHueAdjust {
    /// `HUE_NONE`: no adjustment.
    None = 0,
    /// `HUE_DW3`: the algorithm of the ACES Output Transforms through v0.7.
    Dw3,
    /// `HUE_WYPN`: Weighted Yellow Power Norm, not implemented upstream.
    Wypn,
}

/// Interpolation algorithms.
///
/// Port of `enum Interpolation` (include/OpenColorIO/OpenColorTypes.h:410-420 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Interpolation {
    /// `INTERP_UNKNOWN`
    Unknown = 0,
    /// `INTERP_NEAREST`: nearest neighbor.
    Nearest = 1,
    /// `INTERP_LINEAR`: linear interpolation (trilinear for Lut3D).
    Linear = 2,
    /// `INTERP_TETRAHEDRAL`: tetrahedral interpolation (Lut3D only).
    Tetrahedral = 3,
    /// `INTERP_CUBIC`: cubic interpolation (not supported).
    Cubic = 4,
    /// `INTERP_DEFAULT`: the default interpolation type.
    Default = 254,
    /// `INTERP_BEST`: the 'best' suitable interpolation type.
    Best = 255,
}

/// The interpolation's name: `nearest`, `linear`, `tetrahedral`, `best`, `default`, `cubic`,
/// or `unknown`.
///
/// Port of `InterpolationToString` (src/OpenColorIO/ParseUtils.cpp:234-244 @ v2.5.2).
pub fn interpolation_to_string(interp: Interpolation) -> &'static str {
    match interp {
        Interpolation::Nearest => "nearest",
        Interpolation::Linear => "linear",
        Interpolation::Tetrahedral => "tetrahedral",
        Interpolation::Best => "best",
        Interpolation::Default => "default",
        // INTERP_CUBIC is not implemented yet, but the string may be useful for error messages.
        Interpolation::Cubic => "cubic",
        Interpolation::Unknown => "unknown",
    }
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

/// Which environment variables a context loads: those of the config's `environment:`
/// section only (`LoadPredefined`), or all of them (`LoadAll`).
///
/// Port of `EnvironmentMode` (include/OpenColorIO/OpenColorTypes.h:484-489 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EnvironmentMode {
    /// `ENV_ENVIRONMENT_UNKNOWN`.
    Unknown = 0,
    /// `ENV_ENVIRONMENT_LOAD_PREDEFINED`: only load vars in the config's environment section.
    LoadPredefined,
    /// `ENV_ENVIRONMENT_LOAD_ALL`: load all env. vars.
    LoadAll,
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

/// Which optimizations a processor applies: a set of flags, combined with `|` and queried with
/// [`has_flag`](Self::has_flag). The number underneath is upstream's enum's type, a C
/// `unsigned long` (32 bits on Windows, 64 on Linux), which the CPU processor's cache ID prints.
///
/// Port of `OptimizationFlags` (include/OpenColorIO/OpenColorTypes.h:634-722 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OptimizationFlags(pub c_ulong);

impl OptimizationFlags {
    /// `OPTIMIZATION_NONE`: do not optimize.
    pub const NONE: OptimizationFlags = OptimizationFlags(0x0000_0000);

    /// `OPTIMIZATION_IDENTITY`: replace identity ops (other than gamma).
    pub const IDENTITY: OptimizationFlags = OptimizationFlags(0x0000_0001);

    /// `OPTIMIZATION_IDENTITY_GAMMA`: replace identity gamma ops.
    pub const IDENTITY_GAMMA: OptimizationFlags = OptimizationFlags(0x0000_0002);

    /// `OPTIMIZATION_PAIR_IDENTITY_CDL`: replace a pair of CDL ops where one is the inverse
    /// of the other.
    pub const PAIR_IDENTITY_CDL: OptimizationFlags = OptimizationFlags(0x0000_0040);

    /// `OPTIMIZATION_PAIR_IDENTITY_EXPOSURE_CONTRAST`: likewise for exposure-contrast ops.
    pub const PAIR_IDENTITY_EXPOSURE_CONTRAST: OptimizationFlags = OptimizationFlags(0x0000_0080);

    /// `OPTIMIZATION_PAIR_IDENTITY_FIXED_FUNCTION`: likewise for fixed-function ops.
    pub const PAIR_IDENTITY_FIXED_FUNCTION: OptimizationFlags = OptimizationFlags(0x0000_0100);

    /// `OPTIMIZATION_PAIR_IDENTITY_GAMMA`: likewise for gamma ops.
    pub const PAIR_IDENTITY_GAMMA: OptimizationFlags = OptimizationFlags(0x0000_0200);

    /// `OPTIMIZATION_PAIR_IDENTITY_LUT1D`: likewise for 1D LUTs.
    pub const PAIR_IDENTITY_LUT1D: OptimizationFlags = OptimizationFlags(0x0000_0400);

    /// `OPTIMIZATION_PAIR_IDENTITY_LUT3D`: likewise for 3D LUTs.
    pub const PAIR_IDENTITY_LUT3D: OptimizationFlags = OptimizationFlags(0x0000_0800);

    /// `OPTIMIZATION_PAIR_IDENTITY_LOG`: likewise for log ops.
    pub const PAIR_IDENTITY_LOG: OptimizationFlags = OptimizationFlags(0x0000_1000);

    /// `OPTIMIZATION_PAIR_IDENTITY_GRADING`: likewise for grading ops.
    pub const PAIR_IDENTITY_GRADING: OptimizationFlags = OptimizationFlags(0x0000_2000);

    /// `OPTIMIZATION_COMP_EXPONENT`: compose a pair of exponent ops into a single op.
    pub const COMP_EXPONENT: OptimizationFlags = OptimizationFlags(0x0004_0000);

    /// `OPTIMIZATION_COMP_GAMMA`: likewise for gamma ops.
    pub const COMP_GAMMA: OptimizationFlags = OptimizationFlags(0x0008_0000);

    /// `OPTIMIZATION_COMP_MATRIX`: likewise for matrix ops.
    pub const COMP_MATRIX: OptimizationFlags = OptimizationFlags(0x0010_0000);

    /// `OPTIMIZATION_COMP_LUT1D`: likewise for 1D LUTs.
    pub const COMP_LUT1D: OptimizationFlags = OptimizationFlags(0x0020_0000);

    /// `OPTIMIZATION_COMP_LUT3D`: likewise for 3D LUTs.
    pub const COMP_LUT3D: OptimizationFlags = OptimizationFlags(0x0040_0000);

    /// `OPTIMIZATION_COMP_RANGE`: likewise for range ops.
    pub const COMP_RANGE: OptimizationFlags = OptimizationFlags(0x0080_0000);

    /// `OPTIMIZATION_COMP_SEPARABLE_PREFIX`: for integer and half bit depths only, replace
    /// separable ops (ops without channel crosstalk) by a single 1D LUT of the input bit
    /// depth's domain.
    pub const COMP_SEPARABLE_PREFIX: OptimizationFlags = OptimizationFlags(0x0100_0000);

    /// `OPTIMIZATION_LUT_INV_FAST`: evaluate inverse 1D and 3D LUTs with a forward LUT (faster
    /// but less accurate; GPU evaluations always do).
    pub const LUT_INV_FAST: OptimizationFlags = OptimizationFlags(0x0200_0000);

    /// `OPTIMIZATION_FAST_LOG_EXP_POW`: in SSE mode, the CPU processor uses faster
    /// approximations of log, exp and pow.
    pub const FAST_LOG_EXP_POW: OptimizationFlags = OptimizationFlags(0x0400_0000);

    /// `OPTIMIZATION_SIMPLIFY_OPS`: break certain ops down into simpler ones where possible,
    /// such as a CDL into a matrix.
    pub const SIMPLIFY_OPS: OptimizationFlags = OptimizationFlags(0x0800_0000);

    /// `OPTIMIZATION_NO_DYNAMIC_PROPERTIES`: turn off the dynamic control of the ops that
    /// offer it after finalization (e.g. exposure-contrast).
    pub const NO_DYNAMIC_PROPERTIES: OptimizationFlags = OptimizationFlags(0x1000_0000);

    /// `OPTIMIZATION_ALL`: apply all possible optimizations.
    pub const ALL: OptimizationFlags = OptimizationFlags(0xFFFF_FFFF);

    // The following groupings of flags are provided as a convenient way to select an overall
    // optimization level.

    /// `OPTIMIZATION_LOSSLESS`: the identities, the inverse pairs, the compositions of
    /// exponents, gammas, matrices and ranges, and the simpler ops.
    pub const LOSSLESS: OptimizationFlags = OptimizationFlags(
        Self::IDENTITY.0
            | Self::IDENTITY_GAMMA.0
            | Self::PAIR_IDENTITY_CDL.0
            | Self::PAIR_IDENTITY_EXPOSURE_CONTRAST.0
            | Self::PAIR_IDENTITY_FIXED_FUNCTION.0
            | Self::PAIR_IDENTITY_GAMMA.0
            | Self::PAIR_IDENTITY_GRADING.0
            | Self::PAIR_IDENTITY_LOG.0
            | Self::PAIR_IDENTITY_LUT1D.0
            | Self::PAIR_IDENTITY_LUT3D.0
            | Self::COMP_EXPONENT.0
            | Self::COMP_GAMMA.0
            | Self::COMP_MATRIX.0
            | Self::COMP_RANGE.0
            | Self::SIMPLIFY_OPS.0,
    );

    /// `OPTIMIZATION_VERY_GOOD`: lossless, and the compositions of 1D LUTs, the fast inverse
    /// LUTs, the fast log, exp and pow, and the separable prefix.
    pub const VERY_GOOD: OptimizationFlags = OptimizationFlags(
        Self::LOSSLESS.0
            | Self::COMP_LUT1D.0
            | Self::LUT_INV_FAST.0
            | Self::FAST_LOG_EXP_POW.0
            | Self::COMP_SEPARABLE_PREFIX.0,
    );

    /// `OPTIMIZATION_GOOD`: very good, and the compositions of 3D LUTs.
    pub const GOOD: OptimizationFlags = OptimizationFlags(Self::VERY_GOOD.0 | Self::COMP_LUT3D.0);

    /// `OPTIMIZATION_DRAFT`: for quite lossy optimizations, all of them.
    pub const DRAFT: OptimizationFlags = Self::ALL;

    /// `OPTIMIZATION_DEFAULT`: very good.
    pub const DEFAULT: OptimizationFlags = Self::VERY_GOOD;

    /// Whether every flag of `query_flag` is set.
    ///
    /// Port of `HasFlag` (src/OpenColorIO/Op.h:418-421 @ v2.5.2).
    pub fn has_flag(self, query_flag: OptimizationFlags) -> bool {
        (self & query_flag) == query_flag
    }
}

impl BitOr for OptimizationFlags {
    type Output = OptimizationFlags;

    /// The flags of both.
    fn bitor(self, rhs: OptimizationFlags) -> OptimizationFlags {
        OptimizationFlags(self.0 | rhs.0)
    }
}

impl BitAnd for OptimizationFlags {
    type Output = OptimizationFlags;

    /// The flags set in both.
    fn bitand(self, rhs: OptimizationFlags) -> OptimizationFlags {
        OptimizationFlags(self.0 & rhs.0)
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
