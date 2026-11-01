// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/cmssamp.c` @ lcms2.17: black point detection.
//!
//! What is reached: `_cmsLinkProfiles` forces compensation only for v4 profiles in the perceptual
//! intent, and an RGB one, or a built-in Lab, is the only kind that gets there. For a v4 profile
//! `cmsDetectDestinationBlackPoint` stops at its first test, so its Adobe algorithm, the round
//! trip transform and the curve fitting aren't reached. `cmsDetectBlackPoint` is asked about the
//! perceptual intent only: `BlackPointUsingPerceptualBlack` needs a CMYK output profile.

use super::cmsio0::Profile;
use super::cmsio1::{cms_is_intent_supported_as_input, cms_is_matrix_shaper};
use super::cmspack::cms_formatter_for_color_space_of_profile;
use super::cmspcs::{cms_end_points_by_space_rgb_black, cms_lab_2_xyz};
use super::cmsvirt::cms_create_lab2_profile_thr;
use super::cmsxform::{cms_create_transform, cms_do_transform_to_lab_dbl};
use super::types::*;

/// `BlackPoint -> X = BlackPoint ->Y = BlackPoint -> Z = 0.0`.
fn zero(black_point: &mut CieXyz) {
    black_point.x = 0.0;
    black_point.y = 0.0;
    black_point.z = 0.0;
}

/// Port of `BlackPointAsDarkerColorant` (src/cmssamp.c:63-142 @ lcms2.17), for an RGB profile.
fn black_point_as_darker_colorant(
    input: &mut Profile,
    intent: u32,
    black_point: &mut CieXyz,
) -> bool {
    // If the profile does not support input direction, assume Black point 0
    if !cms_is_intent_supported_as_input(input, intent) {
        zero(black_point);
        return false;
    }

    // Create a formatter which has n channels and no floating point
    let format = cms_formatter_for_color_space_of_profile(input, 2);

    // Try to get black by using black colorant

    // This function returns darker colorant in 16 bits for several spaces
    let black = cms_end_points_by_space_rgb_black();

    // Lab will be used as the output space, but lab2 will avoid recursion
    let Some(mut lab_profile) = cms_create_lab2_profile_thr() else {
        zero(black_point);
        return false;
    };

    // Create the transform
    let flags = CMS_FLAGS_NOOPTIMIZE | CMS_FLAGS_NOCACHE;
    let xform = cms_create_transform(input, format, &mut lab_profile, TYPE_LAB_DBL, intent, flags);

    let Some(xform) = xform else {
        // Something went wrong. Get rid of open resources and return zero as black
        zero(black_point);
        return false;
    };

    // Convert black to Lab
    let mut lab = cms_do_transform_to_lab_dbl(&xform, &black);

    // Force it to be neutral, check for inconsistencies
    lab.a = 0.0;
    lab.b = 0.0;
    if lab.l > 50.0 || lab.l < 0.0 {
        lab.l = 0.0;
    }

    // Convert from Lab (which is now clipped) to XYZ.
    *black_point = cms_lab_2_xyz(&lab);

    true
}

/// Port of `cmsDetectBlackPoint` (src/cmssamp.c:191-274 @ lcms2.17). `CMS_USE_PROFILE_BLACK_POINT_TAG`
/// isn't defined.
pub fn cms_detect_black_point(
    black_point: &mut CieXyz,
    profile: &mut Profile,
    intent: u32,
) -> bool {
    // Make sure the device class is adequate
    let dev_class = profile.device_class;
    if dev_class == SIG_LINK_CLASS
        || dev_class == SIG_ABSTRACT_CLASS
        || dev_class == SIG_NAMED_COLOR_CLASS
    {
        zero(black_point);
        return false;
    }

    // Make sure intent is adequate
    if intent != INTENT_PERCEPTUAL
        && intent != INTENT_RELATIVE_COLORIMETRIC
        && intent != INTENT_SATURATION
    {
        zero(black_point);
        return false;
    }

    // v4 + perceptual & saturation intents does have its own black point, and it is
    // well specified enough to use it. Black point tag is deprecated in V4.
    if profile.version >= 0x4000000 && (intent == INTENT_PERCEPTUAL || intent == INTENT_SATURATION)
    {
        // Matrix shaper share MRC & perceptual intents
        if cms_is_matrix_shaper(profile) {
            return black_point_as_darker_colorant(
                profile,
                INTENT_RELATIVE_COLORIMETRIC,
                black_point,
            );
        }

        // Get Perceptual black out of v4 profiles. That is fixed for perceptual & saturation
        // intents
        black_point.x = CMS_PERCEPTUAL_BLACK_X;
        black_point.y = CMS_PERCEPTUAL_BLACK_Y;
        black_point.z = CMS_PERCEPTUAL_BLACK_Z;

        return true;
    }

    // That is about v2 profiles.

    // Nope, compute BP using current intent.
    black_point_as_darker_colorant(profile, intent, black_point)
}

/// Port of `cmsDetectDestinationBlackPoint` (src/cmssamp.c:352-552 @ lcms2.17), up to the end of its
/// v4 test: the only profiles that reach it with compensation on are a built-in Lab, which is
/// abstract, and a v4 one.
pub fn cms_detect_destination_black_point(
    black_point: &mut CieXyz,
    profile: &mut Profile,
    intent: u32,
) -> bool {
    // Make sure the device class is adequate
    let dev_class = profile.device_class;
    if dev_class == SIG_LINK_CLASS
        || dev_class == SIG_ABSTRACT_CLASS
        || dev_class == SIG_NAMED_COLOR_CLASS
    {
        zero(black_point);
        return false;
    }

    // Make sure intent is adequate
    if intent != INTENT_PERCEPTUAL
        && intent != INTENT_RELATIVE_COLORIMETRIC
        && intent != INTENT_SATURATION
    {
        zero(black_point);
        return false;
    }

    // v4 + perceptual & saturation intents does have its own black point, and it is
    // well specified enough to use it. Black point tag is deprecated in V4.

    // Matrix shaper share MRC & perceptual intents
    if cms_is_matrix_shaper(profile) {
        return black_point_as_darker_colorant(profile, INTENT_RELATIVE_COLORIMETRIC, black_point);
    }

    // Get Perceptual black out of v4 profiles. That is fixed for perceptual & saturation intents
    black_point.x = CMS_PERCEPTUAL_BLACK_X;
    black_point.y = CMS_PERCEPTUAL_BLACK_Y;
    black_point.z = CMS_PERCEPTUAL_BLACK_Z;
    true
}
