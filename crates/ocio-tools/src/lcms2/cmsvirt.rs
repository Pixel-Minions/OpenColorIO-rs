// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/cmsvirt.c` @ lcms2.17: the built-in profiles `ocioicc.cpp` and the black point
//! detection make.

use super::cmsgamma::{ToneCurve, cms_build_parametric_tone_curve};
use super::cmsio0::*;
use super::cmslut::*;
use super::cmsmtrx::Mat3;
use super::cmsnamed::{Mlu, cms_mlu_set_ascii};
use super::cmspcs::{cms_xy_y_2_xyz, cms_xyz_2_xy_y};
use super::cmswtpnt::{cms_adaptation_matrix, cms_build_rgb2xyz_transfer_matrix, cms_d50_xyz};
use super::types::*;

/// Port of `SetTextTags` (src/cmsvirt.c:32-59 @ lcms2.17). `cmsMLUsetWide` of ASCII text gives what
/// `cmsMLUsetASCII` gives.
fn set_text_tags(profile: &mut Profile, description: &[u8]) -> bool {
    let mut description_mlu = Mlu::default();
    let mut copyright_mlu = Mlu::default();

    if !cms_mlu_set_ascii(&mut description_mlu, b"en", b"US", description) {
        return false;
    }
    if !cms_mlu_set_ascii(
        &mut copyright_mlu,
        b"en",
        b"US",
        b"No copyright, use freely",
    ) {
        return false;
    }

    if !cms_write_tag(
        profile,
        SIG_PROFILE_DESCRIPTION_TAG,
        TagData::Mlu(description_mlu),
    ) {
        return false;
    }
    cms_write_tag(profile, SIG_COPYRIGHT_TAG, TagData::Mlu(copyright_mlu))
}

/// Port of `cmsCreateRGBProfileTHR` (src/cmsvirt.c:101-215 @ lcms2.17). `transfer_function` holds one
/// curve for the three channels: the only way it is called with curves. So the second and third
/// are links to the first, as C finds when it compares the pointers.
///
/// `_cmsAdaptationMatrix` isn't checked in C, and leaves `CHAD` as it was if it fails. That takes
/// a white point the tool's temperatures can't make: `Err` isn't for that but for its `NULL`.
pub fn cms_create_rgb_profile_thr(
    white_point: Option<&CieXyY>,
    primaries: Option<&CieXyYTriple>,
    transfer_function: Option<&ToneCurve>,
) -> Option<Profile> {
    let mut icc = cms_create_profile_placeholder();

    cms_set_profile_version(&mut icc, 4.4);

    icc.device_class = SIG_DISPLAY_CLASS;
    icc.color_space = SIG_RGB_DATA;
    icc.pcs = SIG_XYZ_DATA;

    icc.rendering_intent = INTENT_PERCEPTUAL;

    // Implement profile using following tags:
    //
    //  1 cmsSigProfileDescriptionTag
    //  2 cmsSigMediaWhitePointTag
    //  3 cmsSigRedColorantTag
    //  4 cmsSigGreenColorantTag
    //  5 cmsSigBlueColorantTag
    //  6 cmsSigRedTRCTag
    //  7 cmsSigGreenTRCTag
    //  8 cmsSigBlueTRCTag
    //  9 Chromatic adaptation Tag
    // This conforms a standard RGB DisplayProfile as says ICC, and then I add (As per addendum II)
    // 10 cmsSigChromaticityTag

    if !set_text_tags(&mut icc, b"RGB built-in") {
        return None;
    }

    if let Some(white_point) = white_point {
        if !cms_write_tag(
            &mut icc,
            SIG_MEDIA_WHITE_POINT_TAG,
            TagData::Xyz(cms_d50_xyz()),
        ) {
            return None;
        }

        let white_point_xyz = cms_xy_y_2_xyz(white_point);
        let chad: Mat3 = cms_adaptation_matrix(&white_point_xyz, &cms_d50_xyz())?;

        // This is a V4 tag, but many CMM does read and understand it no matter which version
        let flat = chad.iter().flatten().copied().collect();
        if !cms_write_tag(
            &mut icc,
            SIG_CHROMATIC_ADAPTATION_TAG,
            TagData::S15Fixed16(flat),
        ) {
            return None;
        }
    }

    if let (Some(white_point), Some(primaries)) = (white_point, primaries) {
        let max_white = CieXyY {
            x: white_point.x,
            y: white_point.y,
            big_y: 1.0,
        };

        let m_colorants = cms_build_rgb2xyz_transfer_matrix(&max_white, primaries)?;

        let colorant = |i: usize| {
            TagData::Xyz(CieXyz {
                x: m_colorants[0][i],
                y: m_colorants[1][i],
                z: m_colorants[2][i],
            })
        };

        if !cms_write_tag(&mut icc, SIG_RED_COLORANT_TAG, colorant(0)) {
            return None;
        }
        if !cms_write_tag(&mut icc, SIG_BLUE_COLORANT_TAG, colorant(2)) {
            return None;
        }
        if !cms_write_tag(&mut icc, SIG_GREEN_COLORANT_TAG, colorant(1)) {
            return None;
        }
    }

    if let Some(curve) = transfer_function {
        // Tries to minimize space. Thanks to Richard Hughes for this nice idea
        if !cms_write_tag(&mut icc, SIG_RED_TRC_TAG, TagData::Curve(curve.clone())) {
            return None;
        }

        if !cms_link_tag(&mut icc, SIG_GREEN_TRC_TAG, SIG_RED_TRC_TAG) {
            return None;
        }

        if !cms_link_tag(&mut icc, SIG_BLUE_TRC_TAG, SIG_RED_TRC_TAG) {
            return None;
        }
    }

    if let Some(primaries) = primaries
        && !cms_write_tag(
            &mut icc,
            SIG_CHROMATICITY_TAG,
            TagData::Chromaticity(*primaries),
        )
    {
        return None;
    }

    Some(icc)
}

/// Port of `cmsCreateLab2ProfileTHR` (src/cmsvirt.c:473-510 @ lcms2.17), with the null white point, the
/// only one it gets.
pub fn cms_create_lab2_profile_thr() -> Option<Profile> {
    let white_point = cms_xyz_2_xy_y(&cms_d50_xyz());
    let mut profile = cms_create_rgb_profile_thr(Some(&white_point), None, None)?;

    cms_set_profile_version(&mut profile, 2.1);
    profile.device_class = SIG_ABSTRACT_CLASS;
    profile.color_space = SIG_LAB_DATA;
    profile.pcs = SIG_LAB_DATA;

    if !set_text_tags(&mut profile, b"Lab identity built-in") {
        return None;
    }

    // An identity LUT is all we need
    let mut lut = cms_pipeline_alloc(3, 3)?;

    if !cms_pipeline_insert_stage(
        &mut lut,
        StageLoc::AtBegin,
        cms_stage_alloc_identity_clut(3),
    ) {
        return None;
    }

    if !cms_write_tag(&mut profile, SIG_A_TO_B0_TAG, TagData::Pipeline(lut)) {
        return None;
    }

    Some(profile)
}

/// Port of `cmsCreateLab4ProfileTHR` (src/cmsvirt.c:520-557 @ lcms2.17). A null white point is D50.
pub fn cms_create_lab4_profile_thr(white_point: Option<&CieXyY>) -> Option<Profile> {
    let d50 = cms_xyz_2_xy_y(&cms_d50_xyz());
    let mut profile = cms_create_rgb_profile_thr(Some(white_point.unwrap_or(&d50)), None, None)?;

    cms_set_profile_version(&mut profile, 4.4);
    profile.device_class = SIG_ABSTRACT_CLASS;
    profile.color_space = SIG_LAB_DATA;
    profile.pcs = SIG_LAB_DATA;

    if !set_text_tags(&mut profile, b"Lab identity built-in") {
        return None;
    }

    // An empty LUTs is all we need
    let mut lut = cms_pipeline_alloc(3, 3)?;

    if !cms_pipeline_insert_stage(
        &mut lut,
        StageLoc::AtBegin,
        cms_stage_alloc_identity_curves(3),
    ) {
        return None;
    }

    if !cms_write_tag(&mut profile, SIG_A_TO_B0_TAG, TagData::Pipeline(lut)) {
        return None;
    }

    Some(profile)
}

/// Port of `Build_sRGBGamma` (src/cmsvirt.c:627-639 @ lcms2.17).
fn build_srgb_gamma() -> Option<ToneCurve> {
    let parameters = [2.4, 1. / 1.055, 0.055 / 1.055, 1. / 12.92, 0.04045];

    cms_build_parametric_tone_curve(4, &parameters)
}

/// Port of `cmsCreate_sRGBProfileTHR` (src/cmsvirt.c:642-667 @ lcms2.17).
pub fn cms_create_srgb_profile_thr() -> Option<Profile> {
    let d65 = CieXyY {
        x: 0.3127,
        y: 0.3290,
        big_y: 1.0,
    };
    let rec709_primaries = CieXyYTriple {
        red: CieXyY {
            x: 0.6400,
            y: 0.3300,
            big_y: 1.0,
        },
        green: CieXyY {
            x: 0.3000,
            y: 0.6000,
            big_y: 1.0,
        },
        blue: CieXyY {
            x: 0.1500,
            y: 0.0600,
            big_y: 1.0,
        },
    };

    let gamma22 = build_srgb_gamma()?;

    let mut hsrgb =
        cms_create_rgb_profile_thr(Some(&d65), Some(&rec709_primaries), Some(&gamma22))?;

    if !set_text_tags(&mut hsrgb, b"sRGB built-in") {
        return None;
    }

    Some(hsrgb)
}
