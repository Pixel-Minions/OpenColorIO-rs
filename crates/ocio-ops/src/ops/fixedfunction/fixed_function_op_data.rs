// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The FixedFunction op's data: a style, which names a fixed algorithm and its direction, and
//! the parameters of the styles that take some.
//!
//! Port of `FixedFunctionOpData` (src/OpenColorIO/ops/fixedfunction/FixedFunctionOpData.h and
//! FixedFunctionOpData.cpp @ v2.5.2).

use crate::cfmt::{Crt, OStringStream};
use crate::exception::{Exception, Result};
use crate::format_metadata::{FormatMetadataImpl, METADATA_ID, METADATA_NAME};
use crate::op_data::OpDataType;
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

/// The names `GetStyle` recognizes, in its order of tests (FixedFunctionOpData.cpp:191-361 @
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
    /// (src/OpenColorIO/ops/fixedfunction/FixedFunctionOpData.cpp:92-186 @ v2.5.2). Its
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
    /// (src/OpenColorIO/ops/fixedfunction/FixedFunctionOpData.cpp:189-368 @ v2.5.2).
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
    /// (src/OpenColorIO/ops/fixedfunction/FixedFunctionOpData.cpp:371-492 @ v2.5.2). Its
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
    /// (src/OpenColorIO/ops/fixedfunction/FixedFunctionOpData.cpp:495-589 @ v2.5.2). Its
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

/// The parameters of a style: none for most, 1 for the Rec.2100 surround and the ACES 2.0
/// tone scale, 7 for the ACES 1.3 gamut compression, 8, 9, 10 or 13 for others.
///
/// Port of `FixedFunctionOpData::Params` (src/OpenColorIO/ops/fixedfunction/
/// FixedFunctionOpData.h:81 @ v2.5.2).
pub type Params = Vec<f64>;

/// The cache ID's precision (`DefaultValues::FLOAT_DECIMALS`, FixedFunctionOpData.cpp:38-41 @
/// v2.5.2).
const FLOAT_DECIMALS: i64 = 7;

/// The error of a query that would read a Rec.2100 surround's missing parameter upstream
/// (`docs/improvements.md` U-31).
pub const SHORT_PARAMS: &str = "FixedFunctionOp: the style has fewer parameters than it \
     uses: upstream reads past them.";

/// `ss << value` on a default `std::stringstream`: 6 significant digits.
fn put(ss: &mut OStringStream, value: f64) {
    ss.put_f64(value);
}

/// Refuses `val` outside `[low, high]`; a NaN passes.
///
/// Port of `check_param_bounds` (src/OpenColorIO/ops/fixedfunction/FixedFunctionOpData.cpp:
/// 17-25 @ v2.5.2).
fn check_param_bounds(name: &str, val: f64, low: f64, high: f64) -> Result<()> {
    if val < low || val > high {
        let mut ss = OStringStream::new(Crt::NATIVE);
        ss.put_str("Parameter ");
        put(&mut ss, val);
        ss.put_str(" (");
        ss.put_str(name);
        ss.put_str(") is outside valid range [");
        put(&mut ss, low);
        ss.put_str(",");
        put(&mut ss, high);
        ss.put_str("]");
        return Err(Exception::new(ss.into_bytes()));
    }
    Ok(())
}

/// Refuses a `val` with a fractional part, NaN included (`floor(NaN) != NaN`).
///
/// Port of `check_param_no_frac` (src/OpenColorIO/ops/fixedfunction/FixedFunctionOpData.cpp:
/// 27-35 @ v2.5.2).
fn check_param_no_frac(name: &str, val: f64) -> Result<()> {
    if val.floor() != val {
        let mut ss = OStringStream::new(Crt::NATIVE);
        ss.put_str("Parameter ");
        put(&mut ss, val);
        ss.put_str(" (");
        ss.put_str(name);
        ss.put_str(") cannot include any fractional component");
        return Err(Exception::new(ss.into_bytes()));
    }
    Ok(())
}

/// Builds a message on a default `std::stringstream`, from text and doubles.
fn message(parts: &[Part<'_>]) -> Exception {
    let mut ss = OStringStream::new(Crt::NATIVE);
    for part in parts {
        match *part {
            Part::Text(text) => ss.put_str(text),
            Part::Value(value) => put(&mut ss, value),
        }
    }
    Exception::new(ss.into_bytes())
}

/// A piece of a [`message`].
enum Part<'a> {
    Text(&'a str),
    Value(f64),
}

/// The FixedFunction op's data.
///
/// `==` is upstream's `operator==` ([`equals`](Self::equals)): the style and the parameters,
/// not the metadata.
///
/// Port of `FixedFunctionOpData` (src/OpenColorIO/ops/fixedfunction/FixedFunctionOpData.h:
/// 23-119 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct FixedFunctionOpData {
    /// The `OpData` base's `m_metadata`.
    metadata: FormatMetadataImpl,
    /// `m_style`.
    style: FixedFunctionOpStyle,
    /// `m_params`.
    params: Params,
}

impl FixedFunctionOpData {
    /// The style without parameters, validated: a style that takes some is refused.
    ///
    /// Port of `FixedFunctionOpData(Style)` (src/OpenColorIO/ops/fixedfunction/
    /// FixedFunctionOpData.cpp:591-596 @ v2.5.2).
    pub fn new(style: FixedFunctionOpStyle) -> Result<Self> {
        Self::with_params(style, Params::new())
    }

    /// The style with its parameters, validated.
    ///
    /// Port of `FixedFunctionOpData(Style, const Params &)` (src/OpenColorIO/ops/fixedfunction/
    /// FixedFunctionOpData.cpp:598-604 @ v2.5.2).
    pub fn with_params(style: FixedFunctionOpStyle, params: Params) -> Result<Self> {
        let data = FixedFunctionOpData {
            metadata: FormatMetadataImpl::default(),
            style,
            params,
        };
        data.validate()?;
        Ok(data)
    }

    /// A copy made with the validating constructor, so invalid data is refused, then given the
    /// metadata.
    ///
    /// Port of `FixedFunctionOpData::clone` (src/OpenColorIO/ops/fixedfunction/
    /// FixedFunctionOpData.cpp:610-615 @ v2.5.2).
    pub fn try_clone(&self) -> Result<Self> {
        let mut clone = Self::with_params(self.style, self.params.clone())?;
        clone.metadata = self.metadata.clone();
        Ok(clone)
    }

    /// Checks the number of parameters the style takes and, for some styles, their values.
    ///
    /// Port of `FixedFunctionOpData::validate` (src/OpenColorIO/ops/fixedfunction/
    /// FixedFunctionOpData.cpp:617-840 @ v2.5.2).
    pub fn validate(&self) -> Result<()> {
        use FixedFunctionOpStyle::*;
        use Part::{Text, Value};
        let p = &self.params;
        match self.style {
            AcesGamutComp13Fwd | AcesGamutComp13Inv => {
                self.check_size(7, "seven parameters")?;

                // Clamped to the smallest increment above 1 in half float precision for
                // numerical stability.
                const LIM_LOW_BOUND: f64 = 1.001;
                const LIM_HI_BOUND: f64 = 65504.0;
                check_param_bounds("lim_cyan", p[0], LIM_LOW_BOUND, LIM_HI_BOUND)?;
                check_param_bounds("lim_magenta", p[1], LIM_LOW_BOUND, LIM_HI_BOUND)?;
                check_param_bounds("lim_yellow", p[2], LIM_LOW_BOUND, LIM_HI_BOUND)?;

                const THR_LOW_BOUND: f64 = 0.0;
                // Clamped to the smallest increment below 1 in half float precision for
                // numerical stability.
                const THR_HI_BOUND: f64 = 0.9995;
                check_param_bounds("thr_cyan", p[3], THR_LOW_BOUND, THR_HI_BOUND)?;
                check_param_bounds("thr_magenta", p[4], THR_LOW_BOUND, THR_HI_BOUND)?;
                check_param_bounds("thr_yellow", p[5], THR_LOW_BOUND, THR_HI_BOUND)?;

                const PWR_LOW_BOUND: f64 = 1.0;
                const PWR_HI_BOUND: f64 = 65504.0;
                check_param_bounds("power", p[6], PWR_LOW_BOUND, PWR_HI_BOUND)?;
            }
            AcesOutputTransform20Fwd | AcesOutputTransform20Inv => {
                self.check_size(9, "9 parameters")?;
                check_param_bounds("peak_luminance", p[0], 1.0, 10000.0)?;
                check_param_no_frac("peak_luminance", p[0])?;
            }
            AcesRgbToJmh20 | AcesJmhToRgb20 => {
                self.check_size(8, "8 parameters")?;
            }
            AcesTonescaleCompress20Fwd | AcesTonescaleCompress20Inv => {
                self.check_size(1, "1 parameters")?;
                check_param_bounds("peak_luminance", p[0], 1.0, 10000.0)?;
                check_param_no_frac("peak_luminance", p[0])?;
            }
            AcesGamutCompress20Fwd | AcesGamutCompress20Inv => {
                self.check_size(9, "9 parameters")?;
                check_param_bounds("peak_luminance", p[0], 1.0, 10000.0)?;
                check_param_no_frac("peak_luminance", p[0])?;
            }
            Rec2100SurroundFwd | Rec2100SurroundInv => {
                self.check_size(1, "one parameter")?;

                let p = p[0];
                let low_bound = 0.01;
                let hi_bound = 100.;

                if p < low_bound {
                    return Err(message(&[
                        Text("Parameter "),
                        Value(p),
                        Text(" is less than lower bound "),
                        Value(low_bound),
                    ]));
                } else if p > hi_bound {
                    return Err(message(&[
                        Text("Parameter "),
                        Value(p),
                        Text(" is greater than upper bound "),
                        Value(hi_bound),
                    ]));
                }
            }
            DoubleLogToLin | LinToDoubleLog => {
                self.check_size(13, "13 parameters")?;

                let base = p[0];
                let break1 = p[1];
                let break2 = p[2];
                // Upstream's TODO: add additional checks on the remaining params.

                // Check log base.
                if base <= 0.0 {
                    return Err(message(&[
                        Text("Log base "),
                        Value(base),
                        Text(" is not greater than zero."),
                    ]));
                }

                // Check break point order.
                if break1 > break2 {
                    return Err(message(&[
                        Text("First break point "),
                        Value(break1),
                        Text(" is larger than the second break point "),
                        Value(break2),
                        Text("."),
                    ]));
                }
            }
            LinToGammaLog | GammaLogToLin => {
                self.check_size(10, "10 parameters")?;

                let mirror_pt = p[0];
                let break_pt = p[1];
                let gamma_seg_power = p[2];
                // Upstream's TODO: add additional checks on the remaining params.
                let log_seg_base = p[5];

                // Check log base.
                if log_seg_base <= 0.0 {
                    return Err(message(&[
                        Text("Log base "),
                        Value(log_seg_base),
                        Text(" is not greater than zero."),
                    ]));
                }

                // Check mirror and break point order.
                if mirror_pt >= break_pt {
                    return Err(message(&[
                        Text("Mirror point "),
                        Value(mirror_pt),
                        Text(" is not smaller than the break point "),
                        Value(break_pt),
                        Text("."),
                    ]));
                }

                // Check gamma.
                if gamma_seg_power == 0.0 {
                    return Err(Exception::new("Gamma power is zero."));
                }
            }
            AcesRedMod03Fwd | AcesRedMod03Inv | AcesRedMod10Fwd | AcesRedMod10Inv
            | AcesGlow03Fwd | AcesGlow03Inv | AcesGlow10Fwd | AcesGlow10Inv
            | AcesDarkToDim10Fwd | AcesDarkToDim10Inv | RgbToHsv | HsvToRgb | XyzToXyy
            | XyyToXyz | XyzToUvy | UvyToXyz | XyzToLuv | LuvToXyz | LinToPq | PqToLin
            | RgbToHsyLin | RgbToHsyLog | RgbToHsyVid | HsyLinToRgb | HsyLogToRgb | HsyVidToRgb => {
                self.check_size(0, "zero parameters")?;
            }
        }
        Ok(())
    }

    /// "The style '<detailed name>' must have <what> but <size> found." unless the style has
    /// `size` parameters (FixedFunctionOpData.cpp:621-628 @ v2.5.2, and the other styles'
    /// copies).
    fn check_size(&self, size: usize, what: &str) -> Result<()> {
        if self.params.len() != size {
            let mut ss = OStringStream::new(Crt::NATIVE);
            ss.put_str("The style '");
            ss.put_str(self.style.to_str(true));
            ss.put_str("' must have ");
            ss.put_str(what);
            ss.put_str(" but ");
            ss.put_u64(self.params.len() as u64);
            ss.put_str(" found.");
            return Err(Exception::new(ss.into_bytes()));
        }
        Ok(())
    }

    /// The type of the data: `FixedFunctionType`.
    ///
    /// Port of `FixedFunctionOpData::getType` (src/OpenColorIO/ops/fixedfunction/
    /// FixedFunctionOpData.h:92 @ v2.5.2).
    pub fn get_type(&self) -> OpDataType {
        OpDataType::FixedFunction
    }

    /// Never a no-op.
    ///
    /// Port of `FixedFunctionOpData::isNoOp` (src/OpenColorIO/ops/fixedfunction/
    /// FixedFunctionOpData.h:94 @ v2.5.2).
    pub fn is_no_op(&self) -> bool {
        false
    }

    /// Never an identity.
    ///
    /// Port of `FixedFunctionOpData::isIdentity` (src/OpenColorIO/ops/fixedfunction/
    /// FixedFunctionOpData.h:95 @ v2.5.2).
    pub fn is_identity(&self) -> bool {
        false
    }

    /// Always mixes channels.
    ///
    /// Port of `FixedFunctionOpData::hasChannelCrosstalk` (src/OpenColorIO/ops/fixedfunction/
    /// FixedFunctionOpData.h:96 @ v2.5.2).
    pub fn has_channel_crosstalk(&self) -> bool {
        true
    }

    /// Whether `r` undoes this data: a Rec.2100 surround of the same style whose parameter is
    /// the reciprocal of this one's, or else data equal to this one's
    /// [`inverse`](Self::inverse), whose errors it raises.
    ///
    /// [`SHORT_PARAMS`] where the Rec.2100 surround's comparison would read a missing
    /// parameter upstream (`docs/improvements.md` U-31).
    ///
    /// Port of `FixedFunctionOpData::isInverse` (src/OpenColorIO/ops/fixedfunction/
    /// FixedFunctionOpData.cpp:842-854 @ v2.5.2).
    pub fn is_inverse(&self, r: &FixedFunctionOpData) -> Result<bool> {
        let this_style = self.style;
        if (FixedFunctionOpStyle::Rec2100SurroundFwd == this_style
            || FixedFunctionOpStyle::Rec2100SurroundInv == this_style)
            && this_style == r.style
        {
            // Check for the case where the styles are the same but the parameter is
            // inverted.
            let (Some(&p), Some(&q)) = (self.params.first(), r.params.first()) else {
                return Err(Exception::new(SHORT_PARAMS));
            };
            return Ok(p == 1. / q);
        }
        Ok(r.equals(&self.inverse()?))
    }

    /// Swaps the style's direction. The data is assumed to be validated.
    ///
    /// Port of `FixedFunctionOpData::invert` (src/OpenColorIO/ops/fixedfunction/
    /// FixedFunctionOpData.cpp:856-1088 @ v2.5.2).
    fn invert(&mut self) {
        use FixedFunctionOpStyle::*;
        let inverse = match self.style {
            AcesRedMod03Fwd => AcesRedMod03Inv,
            AcesRedMod03Inv => AcesRedMod03Fwd,
            AcesRedMod10Fwd => AcesRedMod10Inv,
            AcesRedMod10Inv => AcesRedMod10Fwd,
            AcesGlow03Fwd => AcesGlow03Inv,
            AcesGlow03Inv => AcesGlow03Fwd,
            AcesGlow10Fwd => AcesGlow10Inv,
            AcesGlow10Inv => AcesGlow10Fwd,
            AcesDarkToDim10Fwd => AcesDarkToDim10Inv,
            AcesDarkToDim10Inv => AcesDarkToDim10Fwd,
            AcesGamutComp13Fwd => AcesGamutComp13Inv,
            AcesGamutComp13Inv => AcesGamutComp13Fwd,
            AcesOutputTransform20Fwd => AcesOutputTransform20Inv,
            AcesOutputTransform20Inv => AcesOutputTransform20Fwd,
            AcesRgbToJmh20 => AcesJmhToRgb20,
            AcesJmhToRgb20 => AcesRgbToJmh20,
            AcesTonescaleCompress20Fwd => AcesTonescaleCompress20Inv,
            AcesTonescaleCompress20Inv => AcesTonescaleCompress20Fwd,
            AcesGamutCompress20Fwd => AcesGamutCompress20Inv,
            AcesGamutCompress20Inv => AcesGamutCompress20Fwd,
            Rec2100SurroundFwd => Rec2100SurroundInv,
            Rec2100SurroundInv => Rec2100SurroundFwd,
            RgbToHsv => HsvToRgb,
            HsvToRgb => RgbToHsv,
            RgbToHsyLog => HsyLogToRgb,
            HsyLogToRgb => RgbToHsyLog,
            RgbToHsyLin => HsyLinToRgb,
            HsyLinToRgb => RgbToHsyLin,
            RgbToHsyVid => HsyVidToRgb,
            HsyVidToRgb => RgbToHsyVid,
            XyzToXyy => XyyToXyz,
            XyyToXyz => XyzToXyy,
            XyzToUvy => UvyToXyz,
            UvyToXyz => XyzToUvy,
            XyzToLuv => LuvToXyz,
            LuvToXyz => XyzToLuv,
            LinToPq => PqToLin,
            PqToLin => LinToPq,
            LinToGammaLog => GammaLogToLin,
            GammaLogToLin => LinToGammaLog,
            LinToDoubleLog => DoubleLogToLin,
            DoubleLogToLin => LinToDoubleLog,
        };
        self.set_style(inverse);

        // Note that any existing metadata could become stale at this point but trying to
        // update it is also challenging since inverse() is sometimes called even during the
        // creation of new ops.
    }

    /// A copy ([`try_clone`](Self::try_clone), whose errors it raises) with the direction
    /// swapped.
    ///
    /// Port of `FixedFunctionOpData::inverse` (src/OpenColorIO/ops/fixedfunction/
    /// FixedFunctionOpData.cpp:1090-1095 @ v2.5.2).
    pub fn inverse(&self) -> Result<FixedFunctionOpData> {
        let mut func = self.try_clone()?;
        func.invert();
        Ok(func)
    }

    /// Port of `FixedFunctionOpData::getStyle` (src/OpenColorIO/ops/fixedfunction/
    /// FixedFunctionOpData.h:103 @ v2.5.2).
    pub fn style(&self) -> FixedFunctionOpStyle {
        self.style
    }

    /// Port of `FixedFunctionOpData::setStyle` (src/OpenColorIO/ops/fixedfunction/
    /// FixedFunctionOpData.h:104 @ v2.5.2). It doesn't validate.
    pub fn set_style(&mut self, style: FixedFunctionOpStyle) {
        self.style = style;
    }

    /// The direction the style encodes.
    ///
    /// Port of `FixedFunctionOpData::getDirection` (src/OpenColorIO/ops/fixedfunction/
    /// FixedFunctionOpData.cpp:1098-1149 @ v2.5.2).
    pub fn direction(&self) -> TransformDirection {
        use FixedFunctionOpStyle::*;
        match self.style {
            AcesRedMod03Fwd
            | AcesRedMod10Fwd
            | AcesGlow03Fwd
            | AcesGlow10Fwd
            | AcesDarkToDim10Fwd
            | AcesGamutComp13Fwd
            | AcesOutputTransform20Fwd
            | AcesRgbToJmh20
            | AcesTonescaleCompress20Fwd
            | AcesGamutCompress20Fwd
            | Rec2100SurroundFwd
            | RgbToHsv
            | RgbToHsyLog
            | RgbToHsyLin
            | RgbToHsyVid
            | XyzToXyy
            | XyzToUvy
            | XyzToLuv
            | LinToPq
            | LinToGammaLog
            | LinToDoubleLog => TransformDirection::Forward,

            AcesRedMod03Inv
            | AcesRedMod10Inv
            | AcesGlow03Inv
            | AcesGlow10Inv
            | AcesDarkToDim10Inv
            | AcesGamutComp13Inv
            | AcesOutputTransform20Inv
            | AcesJmhToRgb20
            | AcesTonescaleCompress20Inv
            | AcesGamutCompress20Inv
            | Rec2100SurroundInv
            | HsvToRgb
            | HsyLogToRgb
            | HsyLinToRgb
            | HsyVidToRgb
            | XyyToXyz
            | UvyToXyz
            | LuvToXyz
            | PqToLin
            | GammaLogToLin
            | DoubleLogToLin => TransformDirection::Inverse,
        }
    }

    /// Inverts the style when the direction differs.
    ///
    /// Port of `FixedFunctionOpData::setDirection` (src/OpenColorIO/ops/fixedfunction/
    /// FixedFunctionOpData.cpp:1151-1157 @ v2.5.2).
    pub fn set_direction(&mut self, dir: TransformDirection) {
        if self.direction() != dir {
            self.invert();
        }
    }

    /// Port of `FixedFunctionOpData::setParams` (src/OpenColorIO/ops/fixedfunction/
    /// FixedFunctionOpData.h:109 @ v2.5.2). It doesn't validate.
    pub fn set_params(&mut self, params: Params) {
        self.params = params;
    }

    /// Port of `FixedFunctionOpData::getParams` (src/OpenColorIO/ops/fixedfunction/
    /// FixedFunctionOpData.h:110 @ v2.5.2).
    pub fn params(&self) -> &Params {
        &self.params
    }

    /// Whether `other` has the same style and parameters (a NaN parameter is never equal).
    /// The metadata is ignored.
    ///
    /// Port of `FixedFunctionOpData::equals` (src/OpenColorIO/ops/fixedfunction/
    /// FixedFunctionOpData.cpp:1159-1166 @ v2.5.2), after `OpData::equals`, which compares the
    /// types.
    pub fn equals(&self, other: &FixedFunctionOpData) -> bool {
        self.style == other.style && self.params == other.params
    }

    /// The ID (followed by a space) if there is one, the style's detailed name, then each
    /// parameter after a space, with 7 significant digits.
    ///
    /// Port of `FixedFunctionOpData::getCacheID` (src/OpenColorIO/ops/fixedfunction/
    /// FixedFunctionOpData.cpp:1168-1188 @ v2.5.2).
    pub fn get_cache_id(&self) -> Vec<u8> {
        let mut cache_id = Vec::new();
        if !self.get_id().is_empty() {
            cache_id.extend_from_slice(self.get_id());
            cache_id.push(b' ');
        }

        let mut stream = OStringStream::new(Crt::NATIVE);
        stream.precision = FLOAT_DECIMALS;

        stream.put_str(self.style.to_str(true));

        for &param in &self.params {
            stream.put_str(" ");
            stream.put_f64(param);
        }

        cache_id.extend_from_slice(&stream.into_bytes());
        cache_id
    }

    /// Port of `OpData::getFormatMetadata() const` (src/OpenColorIO/Op.h:164 @ v2.5.2).
    pub fn get_format_metadata(&self) -> &FormatMetadataImpl {
        &self.metadata
    }

    /// Port of `OpData::getFormatMetadata()` (src/OpenColorIO/Op.h:163 @ v2.5.2).
    pub fn get_format_metadata_mut(&mut self) -> &mut FormatMetadataImpl {
        &mut self.metadata
    }

    /// Port of `OpData::getID` (src/OpenColorIO/Op.cpp:81-84 @ v2.5.2).
    pub fn get_id(&self) -> &[u8] {
        self.metadata.get_attribute_value_string(Some(METADATA_ID))
    }

    /// Port of `OpData::setID` (src/OpenColorIO/Op.cpp:86-89 @ v2.5.2).
    pub fn set_id(&mut self, id: &[u8]) {
        self.metadata.set_id(Some(id));
    }

    /// Port of `OpData::getName` (src/OpenColorIO/Op.cpp:91-94 @ v2.5.2).
    pub fn get_name(&self) -> &[u8] {
        self.metadata
            .get_attribute_value_string(Some(METADATA_NAME))
    }
}

/// Port of `operator==(const FixedFunctionOpData &, const FixedFunctionOpData &)`
/// (src/OpenColorIO/ops/fixedfunction/FixedFunctionOpData.cpp:1190-1193 @ v2.5.2).
impl PartialEq for FixedFunctionOpData {
    fn eq(&self, other: &Self) -> bool {
        self.equals(other)
    }
}

#[cfg(test)]
#[path = "fixed_function_op_data_tests.rs"]
mod tests;
