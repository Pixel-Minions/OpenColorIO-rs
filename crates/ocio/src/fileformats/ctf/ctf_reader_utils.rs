// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The CLF/CTF names: elements, attributes, interpolations and grading styles.
//!
//! Port of `src/OpenColorIO/fileformats/ctf/CTFReaderUtils.h` and `CTFReaderUtils.cpp` @
//! v2.5.2.

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::open_color_types::{GradingStyle, Interpolation, TransformDirection};
use ocio_ops::platform::strcasecmp;
use ocio_ops::{Exception, Result};

/// `INTERPOLATION_1D_LINEAR` (CTFReaderUtils.cpp:15 @ v2.5.2).
const INTERPOLATION_1D_LINEAR: &[u8] = b"linear";

/// `INTERPOLATION_3D_LINEAR` (CTFReaderUtils.cpp:17 @ v2.5.2).
const INTERPOLATION_3D_LINEAR: &[u8] = b"trilinear";
/// `INTERPOLATION_3D_TETRAHEDRAL` (CTFReaderUtils.cpp:18 @ v2.5.2).
const INTERPOLATION_3D_TETRAHEDRAL: &[u8] = b"tetrahedral";

/// Whether two C strings are equal ignoring ASCII case: `0 == Platform::Strcasecmp(a, b)`.
fn equal_ignoring_case(a: &[u8], b: &[u8]) -> bool {
    strcasecmp(a, b).is_eq()
}

/// The 1D LUT interpolation a CLF names: only "linear" (ignoring case). `str` is a C string
/// (`None` for upstream's null).
///
/// Port of `GetInterpolation1D` (CTFReaderUtils.cpp:21-36 @ v2.5.2).
pub fn get_interpolation_1d(str: Option<&[u8]>) -> Result<Interpolation> {
    if let Some(str) = str.filter(|s| !s.is_empty()) {
        if equal_ignoring_case(str, INTERPOLATION_1D_LINEAR) {
            return Ok(Interpolation::Linear);
        }

        let mut oss = OStringStream::new(Crt::NATIVE);
        oss.put_str("1D LUT interpolation not recongnized: '");
        oss.put_c_str(str);
        oss.put_str("'.");
        return Err(Exception::new(oss.into_bytes()));
    }

    Err(Exception::new("1D LUT missing interpolation value."))
}

/// The name a CLF gives a 1D LUT interpolation: "linear" for linear and best, none for the
/// others (CLF v3 only has "linear").
///
/// Port of `GetInterpolation1DName` (CTFReaderUtils.cpp:38-57 @ v2.5.2).
pub fn get_interpolation_1d_name(interp: Interpolation) -> Option<&'static [u8]> {
    match interp {
        Interpolation::Linear | Interpolation::Best => Some(INTERPOLATION_1D_LINEAR),

        // Note: In CLF v3, some options were removed and the only legal Lut1D value is now "linear".
        Interpolation::Cubic
        | Interpolation::Default
        | Interpolation::Nearest
        | Interpolation::Tetrahedral
        | Interpolation::Unknown => None,
    }
}

/// The 3D LUT interpolation a CLF names: "trilinear" or "tetrahedral" (ignoring case). `str`
/// is a C string (`None` for upstream's null).
///
/// Port of `GetInterpolation3D` (CTFReaderUtils.cpp:59-78 @ v2.5.2).
pub fn get_interpolation_3d(str: Option<&[u8]>) -> Result<Interpolation> {
    if let Some(str) = str.filter(|s| !s.is_empty()) {
        if equal_ignoring_case(str, INTERPOLATION_3D_LINEAR) {
            return Ok(Interpolation::Linear);
        } else if equal_ignoring_case(str, INTERPOLATION_3D_TETRAHEDRAL) {
            return Ok(Interpolation::Tetrahedral);
        }

        let mut oss = OStringStream::new(Crt::NATIVE);
        oss.put_str("3D LUT interpolation not recongnized: '");
        oss.put_c_str(str);
        oss.put_str("'.");
        return Err(Exception::new(oss.into_bytes()));
    }

    Err(Exception::new("3D LUT missing interpolation value."))
}

/// The name a CLF gives a 3D LUT interpolation: "trilinear" for linear, "tetrahedral" for
/// tetrahedral and best, none for the others.
///
/// Port of `GetInterpolation3DName` (CTFReaderUtils.cpp:80-98 @ v2.5.2).
pub fn get_interpolation_3d_name(interp: Interpolation) -> Option<&'static [u8]> {
    match interp {
        Interpolation::Linear => Some(INTERPOLATION_3D_LINEAR),
        Interpolation::Tetrahedral | Interpolation::Best => Some(INTERPOLATION_3D_TETRAHEDRAL),

        Interpolation::Default
        | Interpolation::Nearest
        | Interpolation::Cubic
        | Interpolation::Unknown => None,
    }
}

/// `GRADING_STYLE_LOG_FWD` (CTFReaderUtils.cpp:102 @ v2.5.2).
const GRADING_STYLE_LOG_FWD: &[u8] = b"log";
/// `GRADING_STYLE_LIN_FWD` (CTFReaderUtils.cpp:103 @ v2.5.2).
const GRADING_STYLE_LIN_FWD: &[u8] = b"linear";
/// `GRADING_STYLE_VIDEO_FWD` (CTFReaderUtils.cpp:104 @ v2.5.2).
const GRADING_STYLE_VIDEO_FWD: &[u8] = b"video";
/// `GRADING_STYLE_LOG_REV` (CTFReaderUtils.cpp:105 @ v2.5.2).
const GRADING_STYLE_LOG_REV: &[u8] = b"logRev";
/// `GRADING_STYLE_LIN_REV` (CTFReaderUtils.cpp:106 @ v2.5.2).
const GRADING_STYLE_LIN_REV: &[u8] = b"linearRev";
/// `GRADING_STYLE_VIDEO_REV` (CTFReaderUtils.cpp:107 @ v2.5.2).
const GRADING_STYLE_VIDEO_REV: &[u8] = b"videoRev";

/// The grading style and direction a CTF names (ignoring case). `str` is a C string (`None`
/// for upstream's null).
///
/// Port of `ConvertStringToGradingStyleAndDir` (CTFReaderUtils.cpp:110-157 @ v2.5.2).
pub fn convert_string_to_grading_style_and_dir(
    str: Option<&[u8]>,
) -> Result<(GradingStyle, TransformDirection)> {
    if let Some(str) = str.filter(|s| !s.is_empty()) {
        let found = if equal_ignoring_case(str, GRADING_STYLE_LOG_FWD) {
            (GradingStyle::Log, TransformDirection::Forward)
        } else if equal_ignoring_case(str, GRADING_STYLE_LOG_REV) {
            (GradingStyle::Log, TransformDirection::Inverse)
        } else if equal_ignoring_case(str, GRADING_STYLE_LIN_FWD) {
            (GradingStyle::Lin, TransformDirection::Forward)
        } else if equal_ignoring_case(str, GRADING_STYLE_LIN_REV) {
            (GradingStyle::Lin, TransformDirection::Inverse)
        } else if equal_ignoring_case(str, GRADING_STYLE_VIDEO_FWD) {
            (GradingStyle::Video, TransformDirection::Forward)
        } else if equal_ignoring_case(str, GRADING_STYLE_VIDEO_REV) {
            (GradingStyle::Video, TransformDirection::Inverse)
        } else {
            let mut os = OStringStream::new(Crt::NATIVE);
            os.put_str("Unknown grading style: '");
            os.put_c_str(str);
            os.put_str("'.");

            return Err(Exception::new(os.into_bytes()));
        };
        return Ok(found);
    }

    Err(Exception::new("Missing grading style."))
}

/// The name a CTF gives a grading style in a direction. Upstream's error for a style outside
/// the enum can't happen.
///
/// Port of `ConvertGradingStyleAndDirToString` (CTFReaderUtils.cpp:159-175 @ v2.5.2).
pub fn convert_grading_style_and_dir_to_string(
    style: GradingStyle,
    dir: TransformDirection,
) -> &'static [u8] {
    let fwd = dir == TransformDirection::Forward;
    match style {
        GradingStyle::Log => {
            if fwd {
                GRADING_STYLE_LOG_FWD
            } else {
                GRADING_STYLE_LOG_REV
            }
        }
        GradingStyle::Lin => {
            if fwd {
                GRADING_STYLE_LIN_FWD
            } else {
                GRADING_STYLE_LIN_REV
            }
        }
        GradingStyle::Video => {
            if fwd {
                GRADING_STYLE_VIDEO_FWD
            } else {
                GRADING_STYLE_VIDEO_REV
            }
        }
    }
}

// The element names (CTFReaderUtils.h:22-92 @ v2.5.2).

/// `TAG_ACES`.
pub const TAG_ACES: &[u8] = b"ACES";
/// `TAG_ACES_PARAMS`.
pub const TAG_ACES_PARAMS: &[u8] = b"ACESParams";
/// `TAG_ARRAY`.
pub const TAG_ARRAY: &[u8] = b"Array";
/// `TAG_CDL`.
pub const TAG_CDL: &[u8] = b"ASC_CDL";
/// `TAG_CURVE_CTRL_PNTS`.
pub const TAG_CURVE_CTRL_PNTS: &[u8] = b"ControlPoints";
/// `TAG_CURVE_SLOPES`.
pub const TAG_CURVE_SLOPES: &[u8] = b"Slopes";
/// `TAG_DYN_PROP_CONTRAST`.
pub const TAG_DYN_PROP_CONTRAST: &[u8] = b"CONTRAST";
/// `TAG_DYN_PROP_EXPOSURE`.
pub const TAG_DYN_PROP_EXPOSURE: &[u8] = b"EXPOSURE";
/// `TAG_DYN_PROP_GAMMA`.
pub const TAG_DYN_PROP_GAMMA: &[u8] = b"GAMMA";
/// `TAG_DYN_PROP_PRIMARY`.
pub const TAG_DYN_PROP_PRIMARY: &[u8] = b"PRIMARY";
/// `TAG_DYN_PROP_HUECURVE`.
pub const TAG_DYN_PROP_HUECURVE: &[u8] = b"HUE_CURVE";
/// `TAG_DYN_PROP_RGBCURVE`.
pub const TAG_DYN_PROP_RGBCURVE: &[u8] = b"RGB_CURVE";
/// `TAG_DYN_PROP_TONE`.
pub const TAG_DYN_PROP_TONE: &[u8] = b"TONE";
/// `TAG_DYN_PROP_LOOK`.
pub const TAG_DYN_PROP_LOOK: &[u8] = b"LOOK_SWITCH";
/// `TAG_DYNAMIC_PARAMETER`.
pub const TAG_DYNAMIC_PARAMETER: &[u8] = b"DynamicParameter";
/// `TAG_EXPONENT`.
pub const TAG_EXPONENT: &[u8] = b"Exponent";
/// `TAG_EXPONENT_PARAMS`.
pub const TAG_EXPONENT_PARAMS: &[u8] = b"ExponentParams";
/// `TAG_EXPOSURE_CONTRAST`.
pub const TAG_EXPOSURE_CONTRAST: &[u8] = b"ExposureContrast";
/// `TAG_EC_PARAMS`.
pub const TAG_EC_PARAMS: &[u8] = b"ECParams";
/// `TAG_FIXED_FUNCTION`.
pub const TAG_FIXED_FUNCTION: &[u8] = b"FixedFunction";
/// `TAG_FUNCTION`.
pub const TAG_FUNCTION: &[u8] = b"Function";
/// `TAG_GAMMA`.
pub const TAG_GAMMA: &[u8] = b"Gamma";
/// `TAG_GAMMA_PARAMS`.
pub const TAG_GAMMA_PARAMS: &[u8] = b"GammaParams";
/// `TAG_INDEX_MAP`.
pub const TAG_INDEX_MAP: &[u8] = b"IndexMap";
/// `TAG_INFO`.
pub const TAG_INFO: &[u8] = b"Info";
/// `TAG_INVLUT1D`.
pub const TAG_INVLUT1D: &[u8] = b"InverseLUT1D";
/// `TAG_INVLUT3D`.
pub const TAG_INVLUT3D: &[u8] = b"InverseLUT3D";
/// `TAG_LOG`.
pub const TAG_LOG: &[u8] = b"Log";
/// `TAG_LOG_PARAMS`.
pub const TAG_LOG_PARAMS: &[u8] = b"LogParams";
/// `TAG_LUT1D`.
pub const TAG_LUT1D: &[u8] = b"LUT1D";
/// `TAG_LUT3D`.
pub const TAG_LUT3D: &[u8] = b"LUT3D";
/// `TAG_MATRIX`.
pub const TAG_MATRIX: &[u8] = b"Matrix";
/// `TAG_MAX_IN_VALUE`.
pub const TAG_MAX_IN_VALUE: &[u8] = b"maxInValue";
/// `TAG_MAX_OUT_VALUE`.
pub const TAG_MAX_OUT_VALUE: &[u8] = b"maxOutValue";
/// `TAG_MIN_IN_VALUE`.
pub const TAG_MIN_IN_VALUE: &[u8] = b"minInValue";
/// `TAG_MIN_OUT_VALUE`.
pub const TAG_MIN_OUT_VALUE: &[u8] = b"minOutValue";
/// `TAG_PRIMARY`.
pub const TAG_PRIMARY: &[u8] = b"GradingPrimary";
/// `TAG_PRIMARY_BRIGHTNESS`.
pub const TAG_PRIMARY_BRIGHTNESS: &[u8] = b"Brightness";
/// `TAG_PRIMARY_CLAMP`.
pub const TAG_PRIMARY_CLAMP: &[u8] = b"Clamp";
/// `TAG_PRIMARY_CONTRAST`.
pub const TAG_PRIMARY_CONTRAST: &[u8] = b"Contrast";
/// `TAG_PRIMARY_EXPOSURE`.
pub const TAG_PRIMARY_EXPOSURE: &[u8] = b"Exposure";
/// `TAG_PRIMARY_GAIN`.
pub const TAG_PRIMARY_GAIN: &[u8] = b"Gain";
/// `TAG_PRIMARY_GAMMA`.
pub const TAG_PRIMARY_GAMMA: &[u8] = b"Gamma";
/// `TAG_PRIMARY_LIFT`.
pub const TAG_PRIMARY_LIFT: &[u8] = b"Lift";
/// `TAG_PRIMARY_OFFSET`.
pub const TAG_PRIMARY_OFFSET: &[u8] = b"Offset";
/// `TAG_PRIMARY_PIVOT`.
pub const TAG_PRIMARY_PIVOT: &[u8] = b"Pivot";
/// `TAG_PRIMARY_SATURATION`.
pub const TAG_PRIMARY_SATURATION: &[u8] = b"Saturation";
/// `TAG_PROCESS_LIST`.
pub const TAG_PROCESS_LIST: &[u8] = b"ProcessList";
/// `TAG_RANGE`.
pub const TAG_RANGE: &[u8] = b"Range";
/// `TAG_REFERENCE`.
pub const TAG_REFERENCE: &[u8] = b"Reference";
/// `TAG_HUE_CURVE`.
pub const TAG_HUE_CURVE: &[u8] = b"GradingHueCurve";
/// `TAG_HUE_CURVE_HUE_HUE`.
pub const TAG_HUE_CURVE_HUE_HUE: &[u8] = b"HueHue";
/// `TAG_HUE_CURVE_HUE_SAT`.
pub const TAG_HUE_CURVE_HUE_SAT: &[u8] = b"HueSat";
/// `TAG_HUE_CURVE_HUE_LUM`.
pub const TAG_HUE_CURVE_HUE_LUM: &[u8] = b"HueLum";
/// `TAG_HUE_CURVE_LUM_SAT`.
pub const TAG_HUE_CURVE_LUM_SAT: &[u8] = b"LumSat";
/// `TAG_HUE_CURVE_SAT_SAT`.
pub const TAG_HUE_CURVE_SAT_SAT: &[u8] = b"SatSat";
/// `TAG_HUE_CURVE_LUM_LUM`.
pub const TAG_HUE_CURVE_LUM_LUM: &[u8] = b"LumLum";
/// `TAG_HUE_CURVE_SAT_LUM`.
pub const TAG_HUE_CURVE_SAT_LUM: &[u8] = b"SatLum";
/// `TAG_HUE_CURVE_HUE_FX`.
pub const TAG_HUE_CURVE_HUE_FX: &[u8] = b"HueFx";
/// `TAG_RGB_CURVE`.
pub const TAG_RGB_CURVE: &[u8] = b"GradingRGBCurve";
/// `TAG_RGB_CURVE_BLUE`.
pub const TAG_RGB_CURVE_BLUE: &[u8] = b"Blue";
/// `TAG_RGB_CURVE_GREEN`.
pub const TAG_RGB_CURVE_GREEN: &[u8] = b"Green";
/// `TAG_RGB_CURVE_MASTER`.
pub const TAG_RGB_CURVE_MASTER: &[u8] = b"Master";
/// `TAG_RGB_CURVE_RED`.
pub const TAG_RGB_CURVE_RED: &[u8] = b"Red";
/// `TAG_TONE`.
pub const TAG_TONE: &[u8] = b"GradingTone";
/// `TAG_TONE_BLACKS`.
pub const TAG_TONE_BLACKS: &[u8] = b"Blacks";
/// `TAG_TONE_HIGHLIGHTS`.
pub const TAG_TONE_HIGHLIGHTS: &[u8] = b"Highlights";
/// `TAG_TONE_MIDTONES`.
pub const TAG_TONE_MIDTONES: &[u8] = b"Midtones";
/// `TAG_TONE_SCONTRAST`.
pub const TAG_TONE_SCONTRAST: &[u8] = b"SContrast";
/// `TAG_TONE_SHADOWS`.
pub const TAG_TONE_SHADOWS: &[u8] = b"Shadows";
/// `TAG_TONE_WHITES`.
pub const TAG_TONE_WHITES: &[u8] = b"Whites";

// The attribute names (CTFReaderUtils.h:94-142 @ v2.5.2).

/// `ATTR_ALIAS`.
pub const ATTR_ALIAS: &[u8] = b"alias";
/// `ATTR_BASE`.
pub const ATTR_BASE: &[u8] = b"base";
/// `ATTR_BASE_PATH`.
pub const ATTR_BASE_PATH: &[u8] = b"basePath";
/// `ATTR_BITDEPTH_IN`.
pub const ATTR_BITDEPTH_IN: &[u8] = b"inBitDepth";
/// `ATTR_BITDEPTH_OUT`.
pub const ATTR_BITDEPTH_OUT: &[u8] = b"outBitDepth";
/// `ATTR_BYPASS`.
pub const ATTR_BYPASS: &[u8] = b"bypass";
/// `ATTR_BYPASS_LIN_TO_LOG`.
pub const ATTR_BYPASS_LIN_TO_LOG: &[u8] = b"bypassLinToLog";
/// `ATTR_RGB_TO_HSY`.
pub const ATTR_RGB_TO_HSY: &[u8] = b"hsyTransform";
/// `ATTR_CENTER`.
pub const ATTR_CENTER: &[u8] = b"center";
/// `ATTR_CHAN`.
pub const ATTR_CHAN: &[u8] = b"channel";
/// `ATTR_COMP_CLF_VERSION`.
pub const ATTR_COMP_CLF_VERSION: &[u8] = b"compCLFversion";
/// `ATTR_CONTRAST`.
pub const ATTR_CONTRAST: &[u8] = b"contrast";
/// `ATTR_DIMENSION`.
pub const ATTR_DIMENSION: &[u8] = b"dim";
/// `ATTR_DIRECTION`.
pub const ATTR_DIRECTION: &[u8] = b"dir";
/// `ATTR_EXPONENT`.
pub const ATTR_EXPONENT: &[u8] = b"exponent";
/// `ATTR_EXPOSURE`.
pub const ATTR_EXPOSURE: &[u8] = b"exposure";
/// `ATTR_GAMMA`.
pub const ATTR_GAMMA: &[u8] = b"gamma";
/// `ATTR_HALF_DOMAIN`.
pub const ATTR_HALF_DOMAIN: &[u8] = b"halfDomain";
/// `ATTR_HIGHLIGHT`.
pub const ATTR_HIGHLIGHT: &[u8] = b"highlight";
/// `ATTR_HUE_ADJUST`.
pub const ATTR_HUE_ADJUST: &[u8] = b"hueAdjust";
/// `ATTR_INTERPOLATION`.
pub const ATTR_INTERPOLATION: &[u8] = b"interpolation";
/// `ATTR_INVERSE_OF`.
pub const ATTR_INVERSE_OF: &[u8] = b"inverseOf";
/// `ATTR_IS_INVERTED`.
pub const ATTR_IS_INVERTED: &[u8] = b"inverted";
/// `ATTR_LINEARSLOPE`.
pub const ATTR_LINEARSLOPE: &[u8] = b"linearSlope";
/// `ATTR_LINSIDEBREAK`.
pub const ATTR_LINSIDEBREAK: &[u8] = b"linSideBreak";
/// `ATTR_LINSIDESLOPE`.
pub const ATTR_LINSIDESLOPE: &[u8] = b"linSideSlope";
/// `ATTR_LINSIDEOFFSET`.
pub const ATTR_LINSIDEOFFSET: &[u8] = b"linSideOffset";
/// `ATTR_LOGEXPOSURESTEP`.
pub const ATTR_LOGEXPOSURESTEP: &[u8] = b"logExposureStep";
/// `ATTR_LOGMIDGRAY`.
pub const ATTR_LOGMIDGRAY: &[u8] = b"logMidGray";
/// `ATTR_LOGSIDESLOPE`.
pub const ATTR_LOGSIDESLOPE: &[u8] = b"logSideSlope";
/// `ATTR_LOGSIDEOFFSET`.
pub const ATTR_LOGSIDEOFFSET: &[u8] = b"logSideOffset";
/// `ATTR_MASTER`.
pub const ATTR_MASTER: &[u8] = b"master";
/// `ATTR_OFFSET`.
pub const ATTR_OFFSET: &[u8] = b"offset";
/// `ATTR_PARAM`.
pub const ATTR_PARAM: &[u8] = b"param";
/// `ATTR_PARAMS`.
pub const ATTR_PARAMS: &[u8] = b"params";
/// `ATTR_PATH`.
pub const ATTR_PATH: &[u8] = b"path";
/// `ATTR_PIVOT`.
pub const ATTR_PIVOT: &[u8] = b"pivot";
/// `ATTR_PRIMARY_BLACK`.
pub const ATTR_PRIMARY_BLACK: &[u8] = b"black";
/// `ATTR_PRIMARY_CONTRAST`.
pub const ATTR_PRIMARY_CONTRAST: &[u8] = b"contrast";
/// `ATTR_PRIMARY_WHITE`.
pub const ATTR_PRIMARY_WHITE: &[u8] = b"white";
/// `ATTR_RAW_HALFS`.
pub const ATTR_RAW_HALFS: &[u8] = b"rawHalfs";
/// `ATTR_REFBLACK`.
pub const ATTR_REFBLACK: &[u8] = b"refBlack";
/// `ATTR_REFWHITE`.
pub const ATTR_REFWHITE: &[u8] = b"refWhite";
/// `ATTR_RGB`.
pub const ATTR_RGB: &[u8] = b"rgb";
/// `ATTR_SHADOW`.
pub const ATTR_SHADOW: &[u8] = b"shadow";
/// `ATTR_START`.
pub const ATTR_START: &[u8] = b"start";
/// `ATTR_STYLE`.
pub const ATTR_STYLE: &[u8] = b"style";
/// `ATTR_VERSION`.
pub const ATTR_VERSION: &[u8] = b"version";
/// `ATTR_WIDTH`.
pub const ATTR_WIDTH: &[u8] = b"width";
