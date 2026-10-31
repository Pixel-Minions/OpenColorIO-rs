// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! What the translated files share of `include/lcms2.h`, `include/lcms2_plugin.h` and
//! `src/lcms2_internal.h` @ lcms2.17: signatures, pixel format macros, flags, limits and the
//! inline fixed-point helpers. The signatures at the end are generated from the header, one for
//! one.

/// `cmsMAXCHANNELS` (include/lcms2.h:674 @ lcms2.17).
pub const CMS_MAX_CHANNELS: usize = 16;
/// `MAX_INPUT_DIMENSIONS` (include/lcms2_plugin.h:290 @ lcms2.17).
pub const MAX_INPUT_DIMENSIONS: usize = 15;
/// `MAX_STAGE_CHANNELS` (src/lcms2_internal.h:78 @ lcms2.17).
pub const MAX_STAGE_CHANNELS: usize = 128;
/// `MAX_TABLE_TAG` (src/lcms2_internal.h:802 @ lcms2.17).
pub const MAX_TABLE_TAG: usize = 100;
/// `MAX_ENCODEABLE_XYZ` (src/lcms2_internal.h:71 @ lcms2.17).
pub const MAX_ENCODEABLE_XYZ: f64 = 1.0 + 32767.0 / 32768.0;
/// `MATRIX_DET_TOLERANCE` (src/lcms2_internal.h:144 @ lcms2.17).
pub const MATRIX_DET_TOLERANCE: f64 = 0.0001;

pub const CMS_D50X: f64 = 0.9642;
pub const CMS_D50Y: f64 = 1.0;
pub const CMS_D50Z: f64 = 0.8249;
pub const CMS_PERCEPTUAL_BLACK_X: f64 = 0.00336;
pub const CMS_PERCEPTUAL_BLACK_Y: f64 = 0.0034731;
pub const CMS_PERCEPTUAL_BLACK_Z: f64 = 0.00287;

pub const CMS_MAGIC_NUMBER: u32 = 0x61637370;
pub const LCMS_SIGNATURE: u32 = 0x6c636d73;
/// `cmsCorbisBrokenXYZtype` (src/cmstypes.c:40 @ lcms2.17).
pub const CMS_CORBIS_BROKEN_XYZ_TYPE: u32 = 0x17A505B8;
/// `cmsMonacoBrokenCurveType` (src/cmstypes.c:41 @ lcms2.17).
pub const CMS_MONACO_BROKEN_CURVE_TYPE: u32 = 0x9478ee00;

pub const CMS_ERROR_FILE: u32 = 1;
pub const CMS_ERROR_RANGE: u32 = 2;
pub const CMS_ERROR_NULL: u32 = 4;
pub const CMS_ERROR_WRITE: u32 = 7;
pub const CMS_ERROR_UNKNOWN_EXTENSION: u32 = 8;
pub const CMS_ERROR_COLORSPACE_CHECK: u32 = 9;
pub const CMS_ERROR_BAD_SIGNATURE: u32 = 11;
pub const CMS_ERROR_CORRUPTION_DETECTED: u32 = 12;
pub const CMS_ERROR_NOT_SUITABLE: u32 = 13;

pub const INTENT_PERCEPTUAL: u32 = 0;
pub const INTENT_RELATIVE_COLORIMETRIC: u32 = 1;
pub const INTENT_SATURATION: u32 = 2;
pub const INTENT_ABSOLUTE_COLORIMETRIC: u32 = 3;

pub const CMS_LERP_FLAGS_16BITS: u32 = 0x0000;
pub const CMS_LERP_FLAGS_FLOAT: u32 = 0x0001;
pub const CMS_LERP_FLAGS_TRILINEAR: u32 = 0x0100;

/// `SAMPLER_INSPECT` (include/lcms2.h:1322 @ lcms2.17).
pub const SAMPLER_INSPECT: u32 = 0x01000000;

pub const PT_ANY: u32 = 0;

pub const CMS_FLAGS_NOCACHE: u32 = 0x0040;
pub const CMS_FLAGS_NOOPTIMIZE: u32 = 0x0100;

/// `T_COLORSPACE` (include/lcms2.h:712 @ lcms2.17).
pub const fn t_colorspace(s: u32) -> u32 {
    (s >> 16) & 31
}

/// `T_CHANNELS` (include/lcms2.h:719 @ lcms2.17).
pub const fn t_channels(c: u32) -> u32 {
    (c >> 3) & 15
}

/// `TYPE_RGB_16` (include/lcms2.h:777 @ lcms2.17).
pub const TYPE_RGB_16: u32 = (PT_RGB << 16) | (3 << 3) | 2;
/// `TYPE_LabV2_16` (include/lcms2.h:896 @ lcms2.17).
pub const TYPE_LAB_V2_16: u32 = (PT_LAB_V2 << 16) | (3 << 3) | 2;
/// `TYPE_Lab_DBL` (include/lcms2.h:954 @ lcms2.17).
pub const TYPE_LAB_DBL: u32 = (1 << 22) | (PT_LAB << 16) | (3 << 3);

pub const PT_GRAY: u32 = 3;
pub const PT_RGB: u32 = 4;
pub const PT_CMY: u32 = 5;
pub const PT_CMYK: u32 = 6;
pub const PT_YCBCR: u32 = 7;
pub const PT_YUV: u32 = 8;
pub const PT_XYZ: u32 = 9;
pub const PT_LAB: u32 = 10;
pub const PT_YUVK: u32 = 11;
pub const PT_HSV: u32 = 12;
pub const PT_HLS: u32 = 13;
pub const PT_YXY: u32 = 14;
pub const PT_MCH1: u32 = 15;
pub const PT_LAB_V2: u32 = 30;

/// `cmsCIEXYZ`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CieXyz {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// `cmsCIExyY`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CieXyY {
    pub x: f64,
    pub y: f64,
    pub big_y: f64,
}

/// `cmsCIExyYTRIPLE`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CieXyYTriple {
    pub red: CieXyY,
    pub green: CieXyY,
    pub blue: CieXyY,
}

/// `cmsCIELab`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CieLab {
    pub l: f64,
    pub a: f64,
    pub b: f64,
}

/// Port of `FROM_8_TO_16` (src/lcms2_internal.h:97 @ lcms2.17).
pub const fn from_8_to_16(rgb: u8) -> u16 {
    ((rgb as u16) << 8) | rgb as u16
}

/// Port of `_cmsALIGNLONG` (src/lcms2_internal.h:57 @ lcms2.17).
pub const fn cms_align_long(x: u32) -> u32 {
    x.wrapping_add(3) & !3
}

/// `FIXED_TO_INT` (src/lcms2_internal.h:147 @ lcms2.17).
#[inline]
pub const fn fixed_to_int(x: i32) -> i32 {
    x >> 16
}

/// `FIXED_REST_TO_INT` (src/lcms2_internal.h:148 @ lcms2.17).
#[inline]
pub const fn fixed_rest_to_int(x: i32) -> i32 {
    ((x as u32) & 0xFFFF) as i32
}

/// `ROUND_FIXED_TO_INT` (src/lcms2_internal.h:149 @ lcms2.17). The sum wraps, as the wheels'
/// `add` does.
#[inline]
pub const fn round_fixed_to_int(x: i32) -> i32 {
    x.wrapping_add(0x8000) >> 16
}

/// Port of `_cmsToFixedDomain` (src/lcms2_internal.h:151 @ lcms2.17). The sums wrap, as the
/// wheels' `add` does.
#[inline]
pub const fn cms_to_fixed_domain(a: i32) -> i32 {
    a.wrapping_add(a.wrapping_add(0x7fff) / 0xffff)
}

/// Port of `_cmsQuickFloor` (src/lcms2_internal.h:159-176 @ lcms2.17): `CMS_DONT_USE_FAST_FLOOR`
/// isn't defined in the wheels' builds, so this is the magic-number version. It reads the low
/// half of the sum, which is what its union does on a little-endian machine.
#[inline]
pub fn cms_quick_floor(val: f64) -> i32 {
    const MAGIC: f64 = 68719476736.0 * 1.5;
    let temp = val + MAGIC;
    ((temp.to_bits() as u32) as i32) >> 16
}

/// Port of `_cmsQuickFloorWord` (src/lcms2_internal.h:179-182 @ lcms2.17).
#[inline]
pub fn cms_quick_floor_word(d: f64) -> u16 {
    (cms_quick_floor(d - 32767.0) as u16).wrapping_add(32767)
}

/// Port of `_cmsQuickSaturateWord` (src/lcms2_internal.h:185-191 @ lcms2.17).
#[inline]
pub fn cms_quick_saturate_word(d: f64) -> u16 {
    let d = d + 0.5;
    if d <= 0.0 {
        return 0;
    }
    if d >= 65535.0 {
        return 0xffff;
    }
    cms_quick_floor_word(d)
}

/// `(int) d` on x86-64: `cvttsd2si` with a 32-bit destination, which gives `0x80000000` for
/// anything that doesn't fit, where Rust's `as` saturates.
#[inline]
pub fn cvttsd2si(d: f64) -> i32 {
    if d > -2147483649.0 && d < 2147483648.0 {
        d as i32
    } else {
        i32::MIN
    }
}

// The enums of include/lcms2.h, generated from it one for one.

pub const SIG_CHROMATICITY_TYPE: u32 = 0x6368726D;
pub const SIG_CICP_TYPE: u32 = 0x63696370;
pub const SIG_COLORANT_ORDER_TYPE: u32 = 0x636C726F;
pub const SIG_COLORANT_TABLE_TYPE: u32 = 0x636C7274;
pub const SIG_CRD_INFO_TYPE: u32 = 0x63726469;
pub const SIG_CURVE_TYPE: u32 = 0x63757276;
pub const SIG_DATA_TYPE: u32 = 0x64617461;
pub const SIG_DICT_TYPE: u32 = 0x64696374;
pub const SIG_DATE_TIME_TYPE: u32 = 0x6474696D;
pub const SIG_DEVICE_SETTINGS_TYPE: u32 = 0x64657673;
pub const SIG_LUT16_TYPE: u32 = 0x6D667432;
pub const SIG_LUT8_TYPE: u32 = 0x6D667431;
pub const SIG_LUT_ATO_B_TYPE: u32 = 0x6D414220;
pub const SIG_LUT_BTO_A_TYPE: u32 = 0x6D424120;
pub const SIG_MEASUREMENT_TYPE: u32 = 0x6D656173;
pub const SIG_MULTI_LOCALIZED_UNICODE_TYPE: u32 = 0x6D6C7563;
pub const SIG_MULTI_PROCESS_ELEMENT_TYPE: u32 = 0x6D706574;
pub const SIG_NAMED_COLOR_TYPE: u32 = 0x6E636F6C;
pub const SIG_NAMED_COLOR2_TYPE: u32 = 0x6E636C32;
pub const SIG_PARAMETRIC_CURVE_TYPE: u32 = 0x70617261;
pub const SIG_PROFILE_SEQUENCE_DESC_TYPE: u32 = 0x70736571;
pub const SIG_PROFILE_SEQUENCE_ID_TYPE: u32 = 0x70736964;
pub const SIG_RESPONSE_CURVE_SET16_TYPE: u32 = 0x72637332;
pub const SIG_S15_FIXED16_ARRAY_TYPE: u32 = 0x73663332;
pub const SIG_SCREENING_TYPE: u32 = 0x7363726E;
pub const SIG_SIGNATURE_TYPE: u32 = 0x73696720;
pub const SIG_TEXT_TYPE: u32 = 0x74657874;
pub const SIG_TEXT_DESCRIPTION_TYPE: u32 = 0x64657363;
pub const SIG_U16_FIXED16_ARRAY_TYPE: u32 = 0x75663332;
pub const SIG_UCR_BG_TYPE: u32 = 0x62666420;
pub const SIG_U_INT16_ARRAY_TYPE: u32 = 0x75693136;
pub const SIG_U_INT32_ARRAY_TYPE: u32 = 0x75693332;
pub const SIG_U_INT64_ARRAY_TYPE: u32 = 0x75693634;
pub const SIG_U_INT8_ARRAY_TYPE: u32 = 0x75693038;
pub const SIG_VCGT_TYPE: u32 = 0x76636774;
pub const SIG_VIEWING_CONDITIONS_TYPE: u32 = 0x76696577;
pub const SIG_XYZ_TYPE: u32 = 0x58595A20;
pub const SIG_MHC2_TYPE: u32 = 0x4D484332;
pub const SIG_A_TO_B0_TAG: u32 = 0x41324230;
pub const SIG_A_TO_B1_TAG: u32 = 0x41324231;
pub const SIG_A_TO_B2_TAG: u32 = 0x41324232;
pub const SIG_BLUE_COLORANT_TAG: u32 = 0x6258595A;
pub const SIG_BLUE_MATRIX_COLUMN_TAG: u32 = 0x6258595A;
pub const SIG_BLUE_TRC_TAG: u32 = 0x62545243;
pub const SIG_B_TO_A0_TAG: u32 = 0x42324130;
pub const SIG_B_TO_A1_TAG: u32 = 0x42324131;
pub const SIG_B_TO_A2_TAG: u32 = 0x42324132;
pub const SIG_CALIBRATION_DATE_TIME_TAG: u32 = 0x63616C74;
pub const SIG_CHAR_TARGET_TAG: u32 = 0x74617267;
pub const SIG_CHROMATIC_ADAPTATION_TAG: u32 = 0x63686164;
pub const SIG_CHROMATICITY_TAG: u32 = 0x6368726D;
pub const SIG_COLORANT_ORDER_TAG: u32 = 0x636C726F;
pub const SIG_COLORANT_TABLE_TAG: u32 = 0x636C7274;
pub const SIG_COLORANT_TABLE_OUT_TAG: u32 = 0x636C6F74;
pub const SIG_COLORIMETRIC_INTENT_IMAGE_STATE_TAG: u32 = 0x63696973;
pub const SIG_COPYRIGHT_TAG: u32 = 0x63707274;
pub const SIG_CRD_INFO_TAG: u32 = 0x63726469;
pub const SIG_DATA_TAG: u32 = 0x64617461;
pub const SIG_DATE_TIME_TAG: u32 = 0x6474696D;
pub const SIG_DEVICE_MFG_DESC_TAG: u32 = 0x646D6E64;
pub const SIG_DEVICE_MODEL_DESC_TAG: u32 = 0x646D6464;
pub const SIG_DEVICE_SETTINGS_TAG: u32 = 0x64657673;
pub const SIG_D_TO_B0_TAG: u32 = 0x44324230;
pub const SIG_D_TO_B1_TAG: u32 = 0x44324231;
pub const SIG_D_TO_B2_TAG: u32 = 0x44324232;
pub const SIG_D_TO_B3_TAG: u32 = 0x44324233;
pub const SIG_B_TO_D0_TAG: u32 = 0x42324430;
pub const SIG_B_TO_D1_TAG: u32 = 0x42324431;
pub const SIG_B_TO_D2_TAG: u32 = 0x42324432;
pub const SIG_B_TO_D3_TAG: u32 = 0x42324433;
pub const SIG_GAMUT_TAG: u32 = 0x67616D74;
pub const SIG_GRAY_TRC_TAG: u32 = 0x6B545243;
pub const SIG_GREEN_COLORANT_TAG: u32 = 0x6758595A;
pub const SIG_GREEN_MATRIX_COLUMN_TAG: u32 = 0x6758595A;
pub const SIG_GREEN_TRC_TAG: u32 = 0x67545243;
pub const SIG_LUMINANCE_TAG: u32 = 0x6C756D69;
pub const SIG_MEASUREMENT_TAG: u32 = 0x6D656173;
pub const SIG_MEDIA_BLACK_POINT_TAG: u32 = 0x626B7074;
pub const SIG_MEDIA_WHITE_POINT_TAG: u32 = 0x77747074;
pub const SIG_NAMED_COLOR_TAG: u32 = 0x6E636F6C;
pub const SIG_NAMED_COLOR2_TAG: u32 = 0x6E636C32;
pub const SIG_OUTPUT_RESPONSE_TAG: u32 = 0x72657370;
pub const SIG_PERCEPTUAL_RENDERING_INTENT_GAMUT_TAG: u32 = 0x72696730;
pub const SIG_PREVIEW0_TAG: u32 = 0x70726530;
pub const SIG_PREVIEW1_TAG: u32 = 0x70726531;
pub const SIG_PREVIEW2_TAG: u32 = 0x70726532;
pub const SIG_PROFILE_DESCRIPTION_TAG: u32 = 0x64657363;
pub const SIG_PROFILE_DESCRIPTION_ML_TAG: u32 = 0x6473636D;
pub const SIG_PROFILE_SEQUENCE_DESC_TAG: u32 = 0x70736571;
pub const SIG_PROFILE_SEQUENCE_ID_TAG: u32 = 0x70736964;
pub const SIG_PS2_CRD0_TAG: u32 = 0x70736430;
pub const SIG_PS2_CRD1_TAG: u32 = 0x70736431;
pub const SIG_PS2_CRD2_TAG: u32 = 0x70736432;
pub const SIG_PS2_CRD3_TAG: u32 = 0x70736433;
pub const SIG_PS2_CSA_TAG: u32 = 0x70733273;
pub const SIG_PS2_RENDERING_INTENT_TAG: u32 = 0x70733269;
pub const SIG_RED_COLORANT_TAG: u32 = 0x7258595A;
pub const SIG_RED_MATRIX_COLUMN_TAG: u32 = 0x7258595A;
pub const SIG_RED_TRC_TAG: u32 = 0x72545243;
pub const SIG_SATURATION_RENDERING_INTENT_GAMUT_TAG: u32 = 0x72696732;
pub const SIG_SCREENING_DESC_TAG: u32 = 0x73637264;
pub const SIG_SCREENING_TAG: u32 = 0x7363726E;
pub const SIG_TECHNOLOGY_TAG: u32 = 0x74656368;
pub const SIG_UCR_BG_TAG: u32 = 0x62666420;
pub const SIG_VIEWING_COND_DESC_TAG: u32 = 0x76756564;
pub const SIG_VIEWING_CONDITIONS_TAG: u32 = 0x76696577;
pub const SIG_VCGT_TAG: u32 = 0x76636774;
pub const SIG_META_TAG: u32 = 0x6D657461;
pub const SIG_CICP_TAG: u32 = 0x63696370;
pub const SIG_ARGYLL_ARTS_TAG: u32 = 0x61727473;
pub const SIG_MHC2_TAG: u32 = 0x4D484332;
pub const SIG_DIGITAL_CAMERA: u32 = 0x6463616D;
pub const SIG_FILM_SCANNER: u32 = 0x6673636E;
pub const SIG_REFLECTIVE_SCANNER: u32 = 0x7273636E;
pub const SIG_INK_JET_PRINTER: u32 = 0x696A6574;
pub const SIG_THERMAL_WAX_PRINTER: u32 = 0x74776178;
pub const SIG_ELECTROPHOTOGRAPHIC_PRINTER: u32 = 0x6570686F;
pub const SIG_ELECTROSTATIC_PRINTER: u32 = 0x65737461;
pub const SIG_DYE_SUBLIMATION_PRINTER: u32 = 0x64737562;
pub const SIG_PHOTOGRAPHIC_PAPER_PRINTER: u32 = 0x7270686F;
pub const SIG_FILM_WRITER: u32 = 0x6670726E;
pub const SIG_VIDEO_MONITOR: u32 = 0x7669646D;
pub const SIG_VIDEO_CAMERA: u32 = 0x76696463;
pub const SIG_PROJECTION_TELEVISION: u32 = 0x706A7476;
pub const SIG_CRT_DISPLAY: u32 = 0x43525420;
pub const SIG_PM_DISPLAY: u32 = 0x504D4420;
pub const SIG_AM_DISPLAY: u32 = 0x414D4420;
pub const SIG_PHOTO_CD: u32 = 0x4B504344;
pub const SIG_PHOTO_IMAGE_SETTER: u32 = 0x696D6773;
pub const SIG_GRAVURE: u32 = 0x67726176;
pub const SIG_OFFSET_LITHOGRAPHY: u32 = 0x6F666673;
pub const SIG_SILKSCREEN: u32 = 0x73696C6B;
pub const SIG_FLEXOGRAPHY: u32 = 0x666C6578;
pub const SIG_MOTION_PICTURE_FILM_SCANNER: u32 = 0x6D706673;
pub const SIG_MOTION_PICTURE_FILM_RECORDER: u32 = 0x6D706672;
pub const SIG_DIGITAL_MOTION_PICTURE_CAMERA: u32 = 0x646D7063;
pub const SIG_DIGITAL_CINEMA_PROJECTOR: u32 = 0x64636A70;
pub const SIG_XYZ_DATA: u32 = 0x58595A20;
pub const SIG_LAB_DATA: u32 = 0x4C616220;
pub const SIG_LUV_DATA: u32 = 0x4C757620;
pub const SIG_Y_CB_CR_DATA: u32 = 0x59436272;
pub const SIG_YXY_DATA: u32 = 0x59787920;
pub const SIG_RGB_DATA: u32 = 0x52474220;
pub const SIG_GRAY_DATA: u32 = 0x47524159;
pub const SIG_HSV_DATA: u32 = 0x48535620;
pub const SIG_HLS_DATA: u32 = 0x484C5320;
pub const SIG_CMYK_DATA: u32 = 0x434D594B;
pub const SIG_CMY_DATA: u32 = 0x434D5920;
pub const SIG_MCH1_DATA: u32 = 0x4D434831;
pub const SIG_MCH2_DATA: u32 = 0x4D434832;
pub const SIG_MCH3_DATA: u32 = 0x4D434833;
pub const SIG_MCH4_DATA: u32 = 0x4D434834;
pub const SIG_MCH5_DATA: u32 = 0x4D434835;
pub const SIG_MCH6_DATA: u32 = 0x4D434836;
pub const SIG_MCH7_DATA: u32 = 0x4D434837;
pub const SIG_MCH8_DATA: u32 = 0x4D434838;
pub const SIG_MCH9_DATA: u32 = 0x4D434839;
pub const SIG_MCHA_DATA: u32 = 0x4D434841;
pub const SIG_MCHB_DATA: u32 = 0x4D434842;
pub const SIG_MCHC_DATA: u32 = 0x4D434843;
pub const SIG_MCHD_DATA: u32 = 0x4D434844;
pub const SIG_MCHE_DATA: u32 = 0x4D434845;
pub const SIG_MCHF_DATA: u32 = 0x4D434846;
pub const SIG_NAMED_DATA: u32 = 0x6E6D636C;
pub const SIG_1COLOR_DATA: u32 = 0x31434C52;
pub const SIG_2COLOR_DATA: u32 = 0x32434C52;
pub const SIG_3COLOR_DATA: u32 = 0x33434C52;
pub const SIG_4COLOR_DATA: u32 = 0x34434C52;
pub const SIG_5COLOR_DATA: u32 = 0x35434C52;
pub const SIG_6COLOR_DATA: u32 = 0x36434C52;
pub const SIG_7COLOR_DATA: u32 = 0x37434C52;
pub const SIG_8COLOR_DATA: u32 = 0x38434C52;
pub const SIG_9COLOR_DATA: u32 = 0x39434C52;
pub const SIG_10COLOR_DATA: u32 = 0x41434C52;
pub const SIG_11COLOR_DATA: u32 = 0x42434C52;
pub const SIG_12COLOR_DATA: u32 = 0x43434C52;
pub const SIG_13COLOR_DATA: u32 = 0x44434C52;
pub const SIG_14COLOR_DATA: u32 = 0x45434C52;
pub const SIG_15COLOR_DATA: u32 = 0x46434C52;
pub const SIG_LUV_K_DATA: u32 = 0x4C75764B;
pub const SIG_INPUT_CLASS: u32 = 0x73636E72;
pub const SIG_DISPLAY_CLASS: u32 = 0x6D6E7472;
pub const SIG_OUTPUT_CLASS: u32 = 0x70727472;
pub const SIG_LINK_CLASS: u32 = 0x6C696E6B;
pub const SIG_ABSTRACT_CLASS: u32 = 0x61627374;
pub const SIG_COLOR_SPACE_CLASS: u32 = 0x73706163;
pub const SIG_NAMED_COLOR_CLASS: u32 = 0x6E6D636C;
pub const SIG_MACINTOSH: u32 = 0x4150504C;
pub const SIG_MICROSOFT: u32 = 0x4D534654;
pub const SIG_SOLARIS: u32 = 0x53554E57;
pub const SIG_SGI: u32 = 0x53474920;
pub const SIG_TALIGENT: u32 = 0x54474E54;
pub const SIG_UNICES: u32 = 0x2A6E6978;
pub const SIG_CURVE_SET_ELEM_TYPE: u32 = 0x63767374;
pub const SIG_MATRIX_ELEM_TYPE: u32 = 0x6D617466;
pub const SIG_C_LUT_ELEM_TYPE: u32 = 0x636C7574;
pub const SIG_B_ACS_ELEM_TYPE: u32 = 0x62414353;
pub const SIG_E_ACS_ELEM_TYPE: u32 = 0x65414353;
pub const SIG_XYZ2_LAB_ELEM_TYPE: u32 = 0x6C327820;
pub const SIG_LAB2_XYZ_ELEM_TYPE: u32 = 0x78326C20;
pub const SIG_NAMED_COLOR_ELEM_TYPE: u32 = 0x6E636C20;
pub const SIG_LAB_V2TO_V4: u32 = 0x32203420;
pub const SIG_LAB_V4TO_V2: u32 = 0x34203220;
pub const SIG_IDENTITY_ELEM_TYPE: u32 = 0x69646E20;
pub const SIG_LAB2_FLOAT_PCS: u32 = 0x64326C20;
pub const SIG_FLOAT_PCS2_LAB: u32 = 0x6C326420;
pub const SIG_XYZ2_FLOAT_PCS: u32 = 0x64327820;
pub const SIG_FLOAT_PCS2_XYZ: u32 = 0x78326420;
pub const SIG_CLIP_NEGATIVES_ELEM_TYPE: u32 = 0x636C7020;
pub const SIG_FORMULA_CURVE_SEG: u32 = 0x70617266;
pub const SIG_SAMPLED_CURVE_SEG: u32 = 0x73616D66;
pub const SIG_SEGMENTED_CURVE: u32 = 0x63757266;

#[cfg(test)]
#[path = "types_tests.rs"]
mod tests;
