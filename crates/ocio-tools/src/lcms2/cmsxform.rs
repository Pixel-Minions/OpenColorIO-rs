// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/cmsxform.c` @ lcms2.17: creating and running the transforms of a profile bake.
//!
//! All of them carry `cmsFLAGS_NOOPTIMIZE|cmsFLAGS_NOCACHE`, and there are no plug-ins, no gamut
//! check and no proofing. So the workers are `PrecalculatedXFORM` for the sampler's 16-bit
//! transforms and `FloatXFORM` for the black point detection's, which end in `TYPE_Lab_DBL`.

use super::cmscnvrt::cms_link_profiles;
use super::cmserr::cms_signal_error;
use super::cmsio0::{Profile, cms_is_tag, cms_read_tag};
use super::cmslut::*;
use super::cmsopt::cms_optimize_pipeline;
use super::cmspack::*;
use super::cmspcs::{cms_channels_of, cms_channels_of_color_space, cms_lcms_color_space};
use super::types::*;

/// `_cmsTRANSFORM`.
#[derive(Debug)]
pub struct Transform {
    lut: Pipeline,
    input_format: u32,
    output_format: u32,
}

/// Port of `AllocEmptyTransform` (src/cmsxform.c:855-1014 @ lcms2.17). Every format here has a stock
/// formatter, so it can't fail.
fn alloc_empty_transform(mut lut: Pipeline, input_format: u32, output_format: u32) -> Transform {
    // Not suitable for the transform plug-in, let's check  the pipeline plug-in
    cms_optimize_pipeline(&mut lut);

    Transform {
        lut,
        input_format,
        output_format,
    }
}

/// Port of `GetXFormColorSpaces` (src/cmsxform.c:1016-1066 @ lcms2.17).
fn get_xform_color_spaces(profiles: &[&mut Profile]) -> (u32, u32) {
    let n_profiles = profiles.len();

    let mut input = profiles[0].color_space;
    let mut post_color_space = input;

    for (i, profile) in profiles.iter().enumerate() {
        let is_input = post_color_space != SIG_XYZ_DATA && post_color_space != SIG_LAB_DATA;

        let cls = profile.device_class;

        let (color_space_in, color_space_out) = if cls == SIG_NAMED_COLOR_CLASS {
            (
                SIG_1COLOR_DATA,
                if n_profiles > 1 {
                    profile.pcs
                } else {
                    profile.color_space
                },
            )
        } else if is_input || cls == SIG_LINK_CLASS {
            (profile.color_space, profile.pcs)
        } else {
            (profile.pcs, profile.color_space)
        };

        if i == 0 {
            input = color_space_in;
        }

        post_color_space = color_space_out;
    }

    (input, post_color_space)
}

/// Port of `IsProperColorSpace` (src/cmsxform.c:1069-1082 @ lcms2.17).
fn is_proper_color_space(check: u32, dw_format: u32) -> bool {
    let space1 = t_colorspace(dw_format);
    let space2 = cms_lcms_color_space(check);

    if space1 == PT_ANY {
        return t_channels(dw_format) == cms_channels_of(check);
    }
    if space1 == space2 {
        return true;
    }

    if space1 == PT_LAB_V2 && space2 == PT_LAB {
        return true;
    }
    if space1 == PT_LAB && space2 == PT_LAB_V2 {
        return true;
    }

    false
}

/// `(cmsCIEXYZ*) cmsReadTag(hProfiles[n], cmsSigMediaWhitePointTag)`, whose value `SetWhitePoint` (src/cmsxform.c:1102-1118 @ lcms2.17) keeps in fields nothing reads. What is left is what reading
/// does: its errors, and the tag staying in memory.
fn read_colorants_and_white(profiles: &mut [&mut Profile]) {
    let last = profiles.len() - 1;

    // Take white points
    cms_read_tag(profiles[0], SIG_MEDIA_WHITE_POINT_TAG);
    cms_read_tag(profiles[last], SIG_MEDIA_WHITE_POINT_TAG);

    // Try to read input and output colorant table
    if cms_is_tag(profiles[0], SIG_COLORANT_TABLE_TAG) {
        // Input table can only come in this way.
        cms_read_tag(profiles[0], SIG_COLORANT_TABLE_TAG);
    }

    // Output is a little bit more complex.
    if profiles[last].device_class == SIG_LINK_CLASS {
        // This tag may exist only on devicelink profiles.
        if cms_is_tag(profiles[last], SIG_COLORANT_TABLE_OUT_TAG) {
            // It may be NULL if error
            cms_read_tag(profiles[last], SIG_COLORANT_TABLE_OUT_TAG);
        }
    } else if cms_is_tag(profiles[last], SIG_COLORANT_TABLE_TAG) {
        cms_read_tag(profiles[last], SIG_COLORANT_TABLE_TAG);
    }
}

/// Port of `cmsCreateExtendedTransform` (src/cmsxform.c:1121-1276 @ lcms2.17).
fn cms_create_extended_transform(
    profiles: &mut [&mut Profile],
    bpc: &mut [bool],
    intents: &[u32],
    input_format: u32,
    output_format: u32,
) -> Option<Transform> {
    // Mark entry/exit spaces
    let (entry_color_space, exit_color_space) = get_xform_color_spaces(profiles);

    // Check if proper colorspaces
    if !is_proper_color_space(entry_color_space, input_format) {
        cms_signal_error(
            CMS_ERROR_COLORSPACE_CHECK,
            b"Wrong input color space on transform",
        );
        return None;
    }

    if !is_proper_color_space(exit_color_space, output_format) {
        cms_signal_error(
            CMS_ERROR_COLORSPACE_CHECK,
            b"Wrong output color space on transform",
        );
        return None;
    }

    // Create a pipeline with all transformations
    let Some(lut) = cms_link_profiles(intents, profiles, bpc) else {
        cms_signal_error(CMS_ERROR_NOT_SUITABLE, b"Couldn't link the profiles");
        return None;
    };

    // Check channel count
    if cms_channels_of_color_space(entry_color_space) != lut.input_channels as i32
        || cms_channels_of_color_space(exit_color_space) != lut.output_channels as i32
    {
        cms_signal_error(
            CMS_ERROR_NOT_SUITABLE,
            b"Channel count doesn't match. Profile is corrupted",
        );
        return None;
    }

    // All seems ok
    let xform = alloc_empty_transform(lut, input_format, output_format);

    read_colorants_and_white(profiles);

    Some(xform)
}

/// Port of `cmsCreateTransform` (src/cmsxform.c:1348-1356 @ lcms2.17), through
/// `cmsCreateTransformTHR` and `cmsCreateMultiprofileTransformTHR` (src/cmsxform.c:1279-1305 @ lcms2.17).
/// The flags don't ask for black point compensation.
pub fn cms_create_transform(
    input: &mut Profile,
    input_format: u32,
    output: &mut Profile,
    output_format: u32,
    intent: u32,
    _dw_flags: u32,
) -> Option<Transform> {
    let mut bpc = [false, false];
    let intents = [intent, intent];
    let mut profiles = [input, output];

    cms_create_extended_transform(
        &mut profiles,
        &mut bpc,
        &intents,
        input_format,
        output_format,
    )
}

/// Port of `cmsDoTransform` (src/cmsxform.c:192-207 @ lcms2.17) for one pixel, with the worker
/// `PrecalculatedXFORM` (src/cmsxform.c:402-438 @ lcms2.17). `TYPE_RGB_16` to `TYPE_LabV2_16` and back.
pub fn cms_do_transform(transform: &Transform, input: &[u16; 3], output: &mut [u16; 3]) {
    let mut w_in = [0u16; CMS_MAX_CHANNELS];
    let mut w_out = [0u16; CMS_MAX_CHANNELS];

    if transform.input_format == TYPE_LAB_V2_16 {
        unroll_lab_v2_16(&mut w_in, input);
    } else {
        unroll_3_words(&mut w_in, input);
    }

    cms_pipeline_eval16(&w_in, &mut w_out, &transform.lut);

    if transform.output_format == TYPE_LAB_V2_16 {
        pack_lab_v2_16(&w_out, output);
    } else {
        pack_3_words(&w_out, output);
    }
}

/// Port of `cmsDoTransform` (src/cmsxform.c:192-207 @ lcms2.17) for one pixel, with the worker
/// `FloatXFORM` (src/cmsxform.c:257-322 @ lcms2.17). The black point detection's: 16-bit input, to
/// `TYPE_Lab_DBL`.
pub fn cms_do_transform_to_lab_dbl(transform: &Transform, input: &[u16; 3]) -> CieLab {
    let mut f_in = [0f32; CMS_MAX_CHANNELS];
    let mut f_out = [0f32; CMS_MAX_CHANNELS];

    unroll_16_to_float(&mut f_in, input);

    // No gamut check at all
    cms_pipeline_eval_float(&f_in, &mut f_out, &transform.lut);

    pack_lab_double_from_float(&f_out)
}

/// Port of `cmsCreateTransform` (src/cmsxform.c:1348-1356 @ lcms2.17) with a null output profile: a chain
/// of one profile, the built-in Lab one, which can only end in the check of its colorspaces.
pub fn cms_create_lab_only_transform(
    input: &mut Profile,
    input_format: u32,
    output_format: u32,
) -> Option<Transform> {
    let mut bpc = [false];
    let intents = [INTENT_PERCEPTUAL];
    let mut profiles = [input];

    cms_create_extended_transform(
        &mut profiles,
        &mut bpc,
        &intents,
        input_format,
        output_format,
    )
}
