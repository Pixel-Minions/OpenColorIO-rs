// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The FixedFunction op's data: a style, which names a fixed algorithm and its direction, and
//! the parameters of the styles that take some.
//!
//! Port of `FixedFunctionOpData` (src/OpenColorIO/ops/fixedfunction/FixedFunctionOpData.h and
//! FixedFunctionOpData.cpp @ v2.5.2).

use crate::exception::{Exception, Result};
use crate::open_color_types::{FixedFunctionStyle, TransformDirection, UNIMPLEMENTED_GAMUTMAP};
use crate::platform::strcasecmp;

/// The styles of the op: each algorithm in each direction. The discriminants are upstream's.
///
/// Port of `FixedFunctionOpData::Style`
/// (src/OpenColorIO/ops/fixedfunction/FixedFunctionOpData.h:28-72 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FixedFunctionOpStyle {
    /// `ACES_RED_MOD_03_FWD`: red modifier (ACES 0.3/0.7).
    AcesRedMod03Fwd = 0,
    /// `ACES_RED_MOD_03_INV`: red modifier inverse (ACES 0.3/0.7).
    AcesRedMod03Inv,
    /// `ACES_RED_MOD_10_FWD`: red modifier (ACES 1.0).
    AcesRedMod10Fwd,
    /// `ACES_RED_MOD_10_INV`: red modifier inverse (ACES 1.0).
    AcesRedMod10Inv,
    /// `ACES_GLOW_03_FWD`: glow function (ACES 0.3/0.7).
    AcesGlow03Fwd,
    /// `ACES_GLOW_03_INV`: glow function inverse (ACES 0.3/0.7).
    AcesGlow03Inv,
    /// `ACES_GLOW_10_FWD`: glow function (ACES 1.0).
    AcesGlow10Fwd,
    /// `ACES_GLOW_10_INV`: glow function inverse (ACES 1.0).
    AcesGlow10Inv,
    /// `ACES_DARK_TO_DIM_10_FWD`: dark to dim surround correction (ACES 1.0).
    AcesDarkToDim10Fwd,
    /// `ACES_DARK_TO_DIM_10_INV`: dim to dark surround correction (ACES 1.0).
    AcesDarkToDim10Inv,
    /// `ACES_GAMUT_COMP_13_FWD`: parametric gamut compression (ACES 1.3).
    AcesGamutComp13Fwd,
    /// `ACES_GAMUT_COMP_13_INV`: parametric gamut compression inverse (ACES 1.3).
    AcesGamutComp13Inv,
    /// `REC2100_SURROUND_FWD`: Rec.2100 surround correction (one gamma parameter).
    Rec2100SurroundFwd,
    /// `REC2100_SURROUND_INV`: Rec.2100 surround correction inverse (one gamma parameter).
    Rec2100SurroundInv,
    /// `RGB_TO_HSV`: classic RGB to HSV function.
    RgbToHsv,
    /// `HSV_TO_RGB`: classic HSV to RGB function.
    HsvToRgb,
    /// `XYZ_TO_xyY`: CIE XYZ to 1931 xy chromaticity coordinates.
    XyzToXyy,
    /// `xyY_TO_XYZ`: inverse of the above.
    XyyToXyz,
    /// `XYZ_TO_uvY`: CIE XYZ to 1976 u'v' chromaticity coordinates.
    XyzToUvy,
    /// `uvY_TO_XYZ`: inverse of the above.
    UvyToXyz,
    /// `XYZ_TO_LUV`: CIE XYZ to 1976 CIELUV colour space (D65 white).
    XyzToLuv,
    /// `LUV_TO_XYZ`: inverse of the above.
    LuvToXyz,
    /// `LIN_TO_PQ`: linear to Perceptual Quantizer curve.
    LinToPq,
    /// `PQ_TO_LIN`: inverse of the above.
    PqToLin,
    /// `LIN_TO_GAMMA_LOG`: curve with gamma and log segments (10 parameters).
    LinToGammaLog,
    /// `GAMMA_LOG_TO_LIN`: inverse of the above.
    GammaLogToLin,
    /// `LIN_TO_DOUBLE_LOG`: curve with two log affine and one linear segment (13 parameters).
    LinToDoubleLog,
    /// `DOUBLE_LOG_TO_LIN`: inverse of the above.
    DoubleLogToLin,
    /// `ACES_OUTPUT_TRANSFORM_20_FWD`: ACES2 output transform.
    AcesOutputTransform20Fwd,
    /// `ACES_OUTPUT_TRANSFORM_20_INV`: ACES2 output transform (inverse).
    AcesOutputTransform20Inv,
    /// `ACES_RGB_TO_JMh_20`: ACES2 RGB to JMh.
    AcesRgbToJmh20,
    /// `ACES_JMh_TO_RGB_20`: ACES2 JMh to RGB.
    AcesJmhToRgb20,
    /// `ACES_TONESCALE_COMPRESS_20_FWD`: ACES2 tonescale and chroma compression.
    AcesTonescaleCompress20Fwd,
    /// `ACES_TONESCALE_COMPRESS_20_INV`: ACES2 tonescale and chroma compression (inverse).
    AcesTonescaleCompress20Inv,
    /// `ACES_GAMUT_COMPRESS_20_FWD`: ACES2 gamut compression.
    AcesGamutCompress20Fwd,
    /// `ACES_GAMUT_COMPRESS_20_INV`: ACES2 gamut compression (inverse).
    AcesGamutCompress20Inv,
    /// `RGB_TO_HSY_LIN`: RGB to HSY (hue, saturation, luminance) for linear spaces.
    RgbToHsyLin,
    /// `RGB_TO_HSY_LOG`: RGB to HSY (hue, saturation, luma) for log spaces.
    RgbToHsyLog,
    /// `RGB_TO_HSY_VID`: RGB to HSY (hue, saturation, luma) for video spaces.
    RgbToHsyVid,
    /// `HSY_LIN_TO_RGB`: HSY (hue, saturation, luminance) to RGB for linear spaces.
    HsyLinToRgb,
    /// `HSY_LOG_TO_RGB`: HSY (hue, saturation, luma) to RGB for log spaces.
    HsyLogToRgb,
    /// `HSY_VID_TO_RGB`: HSY (hue, saturation, luma) to RGB for video spaces.
    HsyVidToRgb,
}

// The styles' names in CTF files (FixedFunctionOpData.cpp:43-84 @ v2.5.2).
const ACES_RED_MOD_03_FWD_STR: &str = "RedMod03Fwd";
const ACES_RED_MOD_03_REV_STR: &str = "RedMod03Rev";
const ACES_RED_MOD_10_FWD_STR: &str = "RedMod10Fwd";
const ACES_RED_MOD_10_REV_STR: &str = "RedMod10Rev";
const ACES_GLOW_03_FWD_STR: &str = "Glow03Fwd";
const ACES_GLOW_03_REV_STR: &str = "Glow03Rev";
const ACES_GLOW_10_FWD_STR: &str = "Glow10Fwd";
const ACES_GLOW_10_REV_STR: &str = "Glow10Rev";
const ACES_DARK_TO_DIM_10_STR: &str = "DarkToDim10";
const ACES_DIM_TO_DARK_10_STR: &str = "DimToDark10";
const ACES_GAMUT_COMP_13_FWD_STR: &str = "GamutComp13Fwd";
const ACES_GAMUT_COMP_13_REV_STR: &str = "GamutComp13Rev";
const ACES_OUTPUT_TRANSFORM_20_FWD_STR: &str = "ACESOutputTransform20Fwd";
const ACES_OUTPUT_TRANSFORM_20_INV_STR: &str = "ACESOutputTransform20Inv";
const ACES_RGB_TO_JMH_20_STR: &str = "RGB_TO_JMh_20";
const ACES_JMH_TO_RGB_20_STR: &str = "JMh_TO_RGB_20";
const ACES_TONESCALE_COMPRESS_20_FWD_STR: &str = "ToneScaleCompress20Fwd";
const ACES_TONESCALE_COMPRESS_20_INV_STR: &str = "ToneScaleCompress20Inv";
const ACES_GAMUT_COMPRESS_20_FWD_STR: &str = "GamutCompress20Fwd";
const ACES_GAMUT_COMPRESS_20_INV_STR: &str = "GamutCompress20Inv";
/// Old name for `Rec2100SurroundFwd`.
const SURROUND_STR: &str = "Surround";
const REC_2100_SURROUND_FWD_STR: &str = "Rec2100SurroundFwd";
const REC_2100_SURROUND_REV_STR: &str = "Rec2100SurroundRev";
const RGB_TO_HSV_STR: &str = "RGB_TO_HSV";
const HSV_TO_RGB_STR: &str = "HSV_TO_RGB";
const XYZ_TO_XYY_STR: &str = "XYZ_TO_xyY";
const XYY_TO_XYZ_STR: &str = "xyY_TO_XYZ";
const XYZ_TO_UVY_STR: &str = "XYZ_TO_uvY";
const UVY_TO_XYZ_STR: &str = "uvY_TO_XYZ";
const XYZ_TO_LUV_STR: &str = "XYZ_TO_LUV";
const LUV_TO_XYZ_STR: &str = "LUV_TO_XYZ";
const LIN_TO_PQ_STR: &str = "Lin_TO_PQ";
const PQ_TO_LIN_STR: &str = "PQ_TO_Lin";
const LIN_TO_GAMMA_LOG_STR: &str = "Lin_TO_GammaLog";
const GAMMA_LOG_TO_LIN_STR: &str = "GammaLog_TO_Lin";
const LIN_TO_DOUBLE_LOG_STR: &str = "Lin_TO_DoubleLog";
const DOUBLE_LOG_TO_LIN_STR: &str = "DoubleLog_TO_Lin";
const RGB_TO_HSY_LIN_STR: &str = "RGB_TO_HSY_LIN";
const RGB_TO_HSY_LOG_STR: &str = "RGB_TO_HSY_LOG";
const RGB_TO_HSY_VID_STR: &str = "RGB_TO_HSY_VID";
const HSY_LOG_TO_RGB_STR: &str = "HSY_LOG_TO_RGB";
const HSY_LIN_TO_RGB_STR: &str = "HSY_LIN_TO_RGB";
const HSY_VID_TO_RGB_STR: &str = "HSY_VID_TO_RGB";

/// The names `GetStyle` recognizes, in its order of tests (FixedFunctionOpData.cpp:197-370 @
/// v2.5.2).
const STYLE_NAMES: [(&str, FixedFunctionOpStyle); 43] = {
    use FixedFunctionOpStyle::*;
    [
        (ACES_RED_MOD_03_FWD_STR, AcesRedMod03Fwd),
        (ACES_RED_MOD_03_REV_STR, AcesRedMod03Inv),
        (ACES_RED_MOD_10_FWD_STR, AcesRedMod10Fwd),
        (ACES_RED_MOD_10_REV_STR, AcesRedMod10Inv),
        (ACES_GLOW_03_FWD_STR, AcesGlow03Fwd),
        (ACES_GLOW_03_REV_STR, AcesGlow03Inv),
        (ACES_GLOW_10_FWD_STR, AcesGlow10Fwd),
        (ACES_GLOW_10_REV_STR, AcesGlow10Inv),
        (ACES_DARK_TO_DIM_10_STR, AcesDarkToDim10Fwd),
        (ACES_DIM_TO_DARK_10_STR, AcesDarkToDim10Inv),
        (ACES_GAMUT_COMP_13_FWD_STR, AcesGamutComp13Fwd),
        (ACES_GAMUT_COMP_13_REV_STR, AcesGamutComp13Inv),
        (ACES_OUTPUT_TRANSFORM_20_FWD_STR, AcesOutputTransform20Fwd),
        (ACES_OUTPUT_TRANSFORM_20_INV_STR, AcesOutputTransform20Inv),
        (ACES_RGB_TO_JMH_20_STR, AcesRgbToJmh20),
        (ACES_JMH_TO_RGB_20_STR, AcesJmhToRgb20),
        (
            ACES_TONESCALE_COMPRESS_20_FWD_STR,
            AcesTonescaleCompress20Fwd,
        ),
        (
            ACES_TONESCALE_COMPRESS_20_INV_STR,
            AcesTonescaleCompress20Inv,
        ),
        (ACES_GAMUT_COMPRESS_20_FWD_STR, AcesGamutCompress20Fwd),
        (ACES_GAMUT_COMPRESS_20_INV_STR, AcesGamutCompress20Inv),
        (SURROUND_STR, Rec2100SurroundFwd),
        (REC_2100_SURROUND_FWD_STR, Rec2100SurroundFwd),
        (REC_2100_SURROUND_REV_STR, Rec2100SurroundInv),
        (RGB_TO_HSV_STR, RgbToHsv),
        (HSV_TO_RGB_STR, HsvToRgb),
        (XYZ_TO_XYY_STR, XyzToXyy),
        (XYY_TO_XYZ_STR, XyyToXyz),
        (XYZ_TO_UVY_STR, XyzToUvy),
        (UVY_TO_XYZ_STR, UvyToXyz),
        (XYZ_TO_LUV_STR, XyzToLuv),
        (LUV_TO_XYZ_STR, LuvToXyz),
        (LIN_TO_PQ_STR, LinToPq),
        (PQ_TO_LIN_STR, PqToLin),
        (LIN_TO_GAMMA_LOG_STR, LinToGammaLog),
        (GAMMA_LOG_TO_LIN_STR, GammaLogToLin),
        (LIN_TO_DOUBLE_LOG_STR, LinToDoubleLog),
        (DOUBLE_LOG_TO_LIN_STR, DoubleLogToLin),
        (RGB_TO_HSY_LIN_STR, RgbToHsyLin),
        (RGB_TO_HSY_LOG_STR, RgbToHsyLog),
        (RGB_TO_HSY_VID_STR, RgbToHsyVid),
        (HSY_LOG_TO_RGB_STR, HsyLogToRgb),
        (HSY_LIN_TO_RGB_STR, HsyLinToRgb),
        (HSY_VID_TO_RGB_STR, HsyVidToRgb),
    ]
};

impl FixedFunctionOpStyle {
    /// The style's name in CTF files (`RedMod03Fwd`, ...), or with `detailed`, a more verbose
    /// one for messages and cache IDs (`ACES_RedMod03 (Forward)`, ...). Only the ACES styles and
    /// the Rec.2100 surround have a detailed name; the others give their CTF name either way.
    ///
    /// Port of `FixedFunctionOpData::ConvertStyleToString`
    /// (src/OpenColorIO/ops/fixedfunction/FixedFunctionOpData.cpp:90-191 @ v2.5.2). Its
    /// "Unknown FixedFunction style" for a value outside the enum can't happen.
    pub fn to_str(self, detailed: bool) -> &'static str {
        use FixedFunctionOpStyle::*;
        let (verbose, name) = match self {
            AcesRedMod03Fwd => ("ACES_RedMod03 (Forward)", ACES_RED_MOD_03_FWD_STR),
            AcesRedMod03Inv => ("ACES_RedMod03 (Inverse)", ACES_RED_MOD_03_REV_STR),
            AcesRedMod10Fwd => ("ACES_RedMod10 (Forward)", ACES_RED_MOD_10_FWD_STR),
            AcesRedMod10Inv => ("ACES_RedMod10 (Inverse)", ACES_RED_MOD_10_REV_STR),
            AcesGlow03Fwd => ("ACES_Glow03 (Forward)", ACES_GLOW_03_FWD_STR),
            AcesGlow03Inv => ("ACES_Glow03 (Inverse)", ACES_GLOW_03_REV_STR),
            AcesGlow10Fwd => ("ACES_Glow10 (Forward)", ACES_GLOW_10_FWD_STR),
            AcesGlow10Inv => ("ACES_Glow10 (Inverse)", ACES_GLOW_10_REV_STR),
            AcesDarkToDim10Fwd => ("ACES_DarkToDim10 (Forward)", ACES_DARK_TO_DIM_10_STR),
            AcesDarkToDim10Inv => ("ACES_DarkToDim10 (Inverse)", ACES_DIM_TO_DARK_10_STR),
            AcesGamutComp13Fwd => ("ACES_GamutComp13 (Forward)", ACES_GAMUT_COMP_13_FWD_STR),
            AcesGamutComp13Inv => ("ACES_GamutComp13 (Inverse)", ACES_GAMUT_COMP_13_REV_STR),
            AcesOutputTransform20Fwd => (
                "ACES_OutputTransform20 (Forward)",
                ACES_OUTPUT_TRANSFORM_20_FWD_STR,
            ),
            AcesOutputTransform20Inv => (
                "ACES_OutputTransform20 (Inverse)",
                ACES_OUTPUT_TRANSFORM_20_INV_STR,
            ),
            AcesRgbToJmh20 => (ACES_RGB_TO_JMH_20_STR, ACES_RGB_TO_JMH_20_STR),
            AcesJmhToRgb20 => (ACES_JMH_TO_RGB_20_STR, ACES_JMH_TO_RGB_20_STR),
            AcesTonescaleCompress20Fwd => (
                "ACES_ToneScaleCompress20 (Forward)",
                ACES_TONESCALE_COMPRESS_20_FWD_STR,
            ),
            AcesTonescaleCompress20Inv => (
                "ACES_ToneScaleCompress20 (Inverse)",
                ACES_TONESCALE_COMPRESS_20_INV_STR,
            ),
            AcesGamutCompress20Fwd => (
                "ACES_GamutCompress20 (Forward)",
                ACES_GAMUT_COMPRESS_20_FWD_STR,
            ),
            AcesGamutCompress20Inv => (
                "ACES_GamutCompress20 (Inverse)",
                ACES_GAMUT_COMPRESS_20_INV_STR,
            ),
            Rec2100SurroundFwd => ("REC2100_Surround (Forward)", REC_2100_SURROUND_FWD_STR),
            Rec2100SurroundInv => ("REC2100_Surround (Inverse)", REC_2100_SURROUND_REV_STR),
            RgbToHsv => (RGB_TO_HSV_STR, RGB_TO_HSV_STR),
            HsvToRgb => (HSV_TO_RGB_STR, HSV_TO_RGB_STR),
            XyzToXyy => (XYZ_TO_XYY_STR, XYZ_TO_XYY_STR),
            XyyToXyz => (XYY_TO_XYZ_STR, XYY_TO_XYZ_STR),
            XyzToUvy => (XYZ_TO_UVY_STR, XYZ_TO_UVY_STR),
            UvyToXyz => (UVY_TO_XYZ_STR, UVY_TO_XYZ_STR),
            XyzToLuv => (XYZ_TO_LUV_STR, XYZ_TO_LUV_STR),
            LuvToXyz => (LUV_TO_XYZ_STR, LUV_TO_XYZ_STR),
            LinToPq => (LIN_TO_PQ_STR, LIN_TO_PQ_STR),
            PqToLin => (PQ_TO_LIN_STR, PQ_TO_LIN_STR),
            LinToGammaLog => (LIN_TO_GAMMA_LOG_STR, LIN_TO_GAMMA_LOG_STR),
            GammaLogToLin => (GAMMA_LOG_TO_LIN_STR, GAMMA_LOG_TO_LIN_STR),
            LinToDoubleLog => (LIN_TO_DOUBLE_LOG_STR, LIN_TO_DOUBLE_LOG_STR),
            DoubleLogToLin => (DOUBLE_LOG_TO_LIN_STR, DOUBLE_LOG_TO_LIN_STR),
            RgbToHsyLin => (RGB_TO_HSY_LIN_STR, RGB_TO_HSY_LIN_STR),
            RgbToHsyLog => (RGB_TO_HSY_LOG_STR, RGB_TO_HSY_LOG_STR),
            RgbToHsyVid => (RGB_TO_HSY_VID_STR, RGB_TO_HSY_VID_STR),
            HsyLogToRgb => (HSY_LOG_TO_RGB_STR, HSY_LOG_TO_RGB_STR),
            HsyLinToRgb => (HSY_LIN_TO_RGB_STR, HSY_LIN_TO_RGB_STR),
            HsyVidToRgb => (HSY_VID_TO_RGB_STR, HSY_VID_TO_RGB_STR),
        };
        if detailed { verbose } else { name }
    }

    /// The style a CTF name gives, ignoring ASCII case (`Platform::Strcasecmp`, deviation
    /// D-4); `Surround` is an old name of `Rec2100SurroundFwd`. `None` is a null pointer. The
    /// name ends at its first NUL, as upstream's C string does.
    ///
    /// A null name is refused as an empty one is, where upstream appends the null pointer to
    /// its message, which is undefined behaviour (`docs/improvements.md` U-30).
    ///
    /// Port of `FixedFunctionOpData::GetStyle`
    /// (src/OpenColorIO/ops/fixedfunction/FixedFunctionOpData.cpp:194-376 @ v2.5.2).
    pub fn from_name(name: Option<&str>) -> Result<FixedFunctionOpStyle> {
        let name = name.map_or("", |s| s.split('\0').next().unwrap_or(""));
        if !name.is_empty() {
            for (style_name, style) in STYLE_NAMES {
                if strcasecmp(name, style_name).is_eq() {
                    return Ok(style);
                }
            }
        }
        Err(Exception::new(format!(
            "Unknown FixedFunction style: {name}"
        )))
    }

    /// The op style of a transform style in a direction. The styles of RGB to HSV and of XYZ to
    /// xyY, u'v'Y and LUV ignore the direction: they are always forward. The two styles
    /// upstream doesn't implement are refused ([`UNIMPLEMENTED_GAMUTMAP`]).
    ///
    /// Port of `FixedFunctionOpData::ConvertStyle(FixedFunctionStyle, TransformDirection)`
    /// (src/OpenColorIO/ops/fixedfunction/FixedFunctionOpData.cpp:378-525 @ v2.5.2). Its
    /// "Unknown FixedFunction transform style" for a value outside the enum can't happen.
    pub fn from_transform_style(
        style: FixedFunctionStyle,
        dir: TransformDirection,
    ) -> Result<FixedFunctionOpStyle> {
        use FixedFunctionOpStyle::*;
        let is_forward = dir == TransformDirection::Forward;
        let pick = |fwd, inv| if is_forward { fwd } else { inv };

        Ok(match style {
            FixedFunctionStyle::AcesRedMod03 => pick(AcesRedMod03Fwd, AcesRedMod03Inv),
            FixedFunctionStyle::AcesRedMod10 => pick(AcesRedMod10Fwd, AcesRedMod10Inv),
            FixedFunctionStyle::AcesGlow03 => pick(AcesGlow03Fwd, AcesGlow03Inv),
            FixedFunctionStyle::AcesGlow10 => pick(AcesGlow10Fwd, AcesGlow10Inv),
            FixedFunctionStyle::AcesDarkToDim10 => pick(AcesDarkToDim10Fwd, AcesDarkToDim10Inv),
            FixedFunctionStyle::AcesGamutComp13 => pick(AcesGamutComp13Fwd, AcesGamutComp13Inv),
            FixedFunctionStyle::AcesOutputTransform20 => {
                pick(AcesOutputTransform20Fwd, AcesOutputTransform20Inv)
            }
            FixedFunctionStyle::AcesRgbToJmh20 => pick(AcesRgbToJmh20, AcesJmhToRgb20),
            FixedFunctionStyle::AcesTonescaleCompress20 => {
                pick(AcesTonescaleCompress20Fwd, AcesTonescaleCompress20Inv)
            }
            FixedFunctionStyle::AcesGamutCompress20 => {
                pick(AcesGamutCompress20Fwd, AcesGamutCompress20Inv)
            }
            FixedFunctionStyle::Rec2100Surround => pick(Rec2100SurroundFwd, Rec2100SurroundInv),
            FixedFunctionStyle::RgbToHsv => RgbToHsv,
            FixedFunctionStyle::RgbToHsyLin => pick(RgbToHsyLin, HsyLinToRgb),
            FixedFunctionStyle::RgbToHsyLog => pick(RgbToHsyLog, HsyLogToRgb),
            FixedFunctionStyle::RgbToHsyVid => pick(RgbToHsyVid, HsyVidToRgb),
            FixedFunctionStyle::XyzToXyy => XyzToXyy,
            FixedFunctionStyle::XyzToUvy => XyzToUvy,
            FixedFunctionStyle::XyzToLuv => XyzToLuv,
            FixedFunctionStyle::AcesGamutMap02 | FixedFunctionStyle::AcesGamutMap07 => {
                return Err(Exception::new(UNIMPLEMENTED_GAMUTMAP));
            }
            FixedFunctionStyle::LinToPq => pick(LinToPq, PqToLin),
            FixedFunctionStyle::LinToGammaLog => pick(LinToGammaLog, GammaLogToLin),
            FixedFunctionStyle::LinToDoubleLog => pick(LinToDoubleLog, DoubleLogToLin),
        })
    }

    /// The transform style of the op style, in either direction.
    ///
    /// Port of `FixedFunctionOpData::ConvertStyle(Style)`
    /// (src/OpenColorIO/ops/fixedfunction/FixedFunctionOpData.cpp:527-589 @ v2.5.2). Its
    /// "Unknown FixedFunction style" for a value outside the enum can't happen.
    pub fn to_transform_style(self) -> FixedFunctionStyle {
        use FixedFunctionOpStyle::*;
        match self {
            AcesRedMod03Fwd | AcesRedMod03Inv => FixedFunctionStyle::AcesRedMod03,
            AcesRedMod10Fwd | AcesRedMod10Inv => FixedFunctionStyle::AcesRedMod10,
            AcesGlow03Fwd | AcesGlow03Inv => FixedFunctionStyle::AcesGlow03,
            AcesGlow10Fwd | AcesGlow10Inv => FixedFunctionStyle::AcesGlow10,
            AcesDarkToDim10Fwd | AcesDarkToDim10Inv => FixedFunctionStyle::AcesDarkToDim10,
            AcesGamutComp13Fwd | AcesGamutComp13Inv => FixedFunctionStyle::AcesGamutComp13,
            AcesOutputTransform20Fwd | AcesOutputTransform20Inv => {
                FixedFunctionStyle::AcesOutputTransform20
            }
            AcesRgbToJmh20 | AcesJmhToRgb20 => FixedFunctionStyle::AcesRgbToJmh20,
            AcesTonescaleCompress20Fwd | AcesTonescaleCompress20Inv => {
                FixedFunctionStyle::AcesTonescaleCompress20
            }
            AcesGamutCompress20Fwd | AcesGamutCompress20Inv => {
                FixedFunctionStyle::AcesGamutCompress20
            }
            Rec2100SurroundFwd | Rec2100SurroundInv => FixedFunctionStyle::Rec2100Surround,
            RgbToHsv | HsvToRgb => FixedFunctionStyle::RgbToHsv,
            XyzToXyy | XyyToXyz => FixedFunctionStyle::XyzToXyy,
            XyzToUvy | UvyToXyz => FixedFunctionStyle::XyzToUvy,
            XyzToLuv | LuvToXyz => FixedFunctionStyle::XyzToLuv,
            LinToPq | PqToLin => FixedFunctionStyle::LinToPq,
            LinToGammaLog | GammaLogToLin => FixedFunctionStyle::LinToGammaLog,
            LinToDoubleLog | DoubleLogToLin => FixedFunctionStyle::LinToDoubleLog,
            RgbToHsyLin | HsyLinToRgb => FixedFunctionStyle::RgbToHsyLin,
            RgbToHsyLog | HsyLogToRgb => FixedFunctionStyle::RgbToHsyLog,
            RgbToHsyVid | HsyVidToRgb => FixedFunctionStyle::RgbToHsyVid,
        }
    }
}
