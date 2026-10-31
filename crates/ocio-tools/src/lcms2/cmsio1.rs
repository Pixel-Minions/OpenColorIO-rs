// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/cmsio1.c` @ lcms2.17: reading a profile's pipelines, version by version.
//!
//! A gray profile never gets this far: `cmsCreateExtendedTransform` refuses its colorspace first
//! (`TYPE_RGB_16`). A named color profile doesn't either, so `BuildGrayInputMatrixPipeline`,
//! `BuildGrayOutputPipeline` and the named color branches aren't ported.

use super::cmsgamma::{ToneCurve, cms_reverse_tone_curve};
use super::cmsintrp::cms_set_interpolation_routine;
use super::cmsio0::{Profile, TagData, cms_get_tag_true_type, cms_is_tag, cms_read_tag};
use super::cmslut::*;
use super::cmsmtrx::{Mat3, cms_mat3_inverse};
use super::types::*;

/// `Device2PCS16` (src/cmsio1.c:32-35 @ lcms2.17).
const DEVICE2PCS16: [u32; 4] = [
    SIG_A_TO_B0_TAG,
    SIG_A_TO_B1_TAG,
    SIG_A_TO_B2_TAG,
    SIG_A_TO_B1_TAG,
];
/// `Device2PCSFloat` (src/cmsio1.c:37-40 @ lcms2.17).
const DEVICE2PCS_FLOAT: [u32; 4] = [
    SIG_D_TO_B0_TAG,
    SIG_D_TO_B1_TAG,
    SIG_D_TO_B2_TAG,
    SIG_D_TO_B3_TAG,
];
/// `PCS2Device16` (src/cmsio1.c:42-45 @ lcms2.17).
const PCS2DEVICE16: [u32; 4] = [
    SIG_B_TO_A0_TAG,
    SIG_B_TO_A1_TAG,
    SIG_B_TO_A2_TAG,
    SIG_B_TO_A1_TAG,
];
/// `PCS2DeviceFloat` (src/cmsio1.c:47-50 @ lcms2.17).
const PCS2DEVICE_FLOAT: [u32; 4] = [
    SIG_B_TO_D0_TAG,
    SIG_B_TO_D1_TAG,
    SIG_B_TO_D2_TAG,
    SIG_B_TO_D3_TAG,
];

/// `InpAdj` (src/cmsio1.c:54 @ lcms2.17).
const INP_ADJ: f64 = 1.0 / MAX_ENCODEABLE_XYZ;
/// `OutpAdj` (src/cmsio1.c:55 @ lcms2.17).
const OUTP_ADJ: f64 = MAX_ENCODEABLE_XYZ;

/// `cmsReadTag` of a tag of type `cmsCIEXYZ`.
fn read_xyz_tag(profile: &mut Profile, sig: u32) -> Option<CieXyz> {
    match cms_read_tag(profile, sig)?.as_ref() {
        TagData::Xyz(xyz) => Some(*xyz),
        _ => None,
    }
}

/// `cmsReadTag` of a tag of type `cmsToneCurve`.
fn read_curve_tag(profile: &mut Profile, sig: u32) -> Option<ToneCurve> {
    match cms_read_tag(profile, sig)?.as_ref() {
        TagData::Curve(curve) => Some(curve.clone()),
        _ => None,
    }
}

/// `cmsPipelineDup((cmsPipeline*) cmsReadTag(hProfile, sig))`.
fn read_pipeline_tag(profile: &mut Profile, sig: u32) -> Option<Pipeline> {
    match cms_read_tag(profile, sig)?.as_ref() {
        TagData::Pipeline(lut) => Some(lut.clone()),
        _ => None,
    }
}

/// Port of `ReadICCMatrixRGB2XYZ` (src/cmsio1.c:132-151 @ lcms2.17).
fn read_icc_matrix_rgb2xyz(profile: &mut Profile) -> Option<Mat3> {
    let red = read_xyz_tag(profile, SIG_RED_COLORANT_TAG);
    let green = read_xyz_tag(profile, SIG_GREEN_COLORANT_TAG);
    let blue = read_xyz_tag(profile, SIG_BLUE_COLORANT_TAG);

    let (red, green, blue) = (red?, green?, blue?);

    Some([
        [red.x, green.x, blue.x],
        [red.y, green.y, blue.y],
        [red.z, green.z, blue.z],
    ])
}

/// The nine values of a `cmsMAT3`, as C casts it to `cmsFloat64Number*`.
fn flat(m: &Mat3) -> [f64; 9] {
    [
        m[0][0], m[0][1], m[0][2], m[1][0], m[1][1], m[1][2], m[2][0], m[2][1], m[2][2],
    ]
}

/// Port of `BuildRGBInputMatrixShaper` (src/cmsio1.c:209-259 @ lcms2.17).
fn build_rgb_input_matrix_shaper(profile: &mut Profile) -> Option<Pipeline> {
    let mut mat = read_icc_matrix_rgb2xyz(profile)?;

    // XYZ PCS in encoded in 1.15 format, and the matrix output comes in 0..0xffff range, so
    // we need to adjust the output by a factor of (0x10000/0xffff) to put data in
    // a 1.16 range, and then a >> 1 to obtain 1.15. The total factor is (65536.0)/(65535.0*2)

    for row in &mut mat {
        for item in row {
            *item *= INP_ADJ;
        }
    }

    let shapes = [
        read_curve_tag(profile, SIG_RED_TRC_TAG),
        read_curve_tag(profile, SIG_GREEN_TRC_TAG),
        read_curve_tag(profile, SIG_BLUE_TRC_TAG),
    ];
    let [Some(red), Some(green), Some(blue)] = shapes else {
        return None;
    };
    let shapes = [red, green, blue];

    let mut lut = cms_pipeline_alloc(3, 3)?;

    if !cms_pipeline_insert_stage(
        &mut lut,
        StageLoc::AtEnd,
        cms_stage_alloc_tone_curves(3, Some(&shapes)),
    ) || !cms_pipeline_insert_stage(
        &mut lut,
        StageLoc::AtEnd,
        cms_stage_alloc_matrix(3, 3, &flat(&mat), None),
    ) {
        return None;
    }

    // Note that it is certainly possible a single profile would have a LUT based
    // tag for output working in lab and a matrix-shaper for the fallback cases.
    // This is not allowed by the spec, but this code is tolerant to those cases
    if profile.pcs == SIG_LAB_DATA
        && !cms_pipeline_insert_stage(&mut lut, StageLoc::AtEnd, Some(cms_stage_alloc_xyz2lab()))
    {
        return None;
    }

    Some(lut)
}

/// The stage a float tag starts or ends with for a space, if any: `_cmsReadFloatInputTag` and its
/// like normalize Lab and XYZ. `to` is the direction in from the 0..1 encoding.
fn normalization(space: u32, to: bool) -> Option<Option<Stage>> {
    match (space, to) {
        (SIG_LAB_DATA, true) => Some(cms_stage_normalize_to_lab_float()),
        (SIG_XYZ_DATA, true) => Some(cms_stage_normalize_to_xyz_float()),
        (SIG_LAB_DATA, false) => Some(cms_stage_normalize_from_lab_float()),
        (SIG_XYZ_DATA, false) => Some(cms_stage_normalize_from_xyz_float()),
        _ => None,
    }
}

/// Port of `_cmsReadFloatInputTag` (src/cmsio1.c:264-303 @ lcms2.17) and of
/// `_cmsReadFloatDevicelinkTag` (src/cmsio1.c:663-701 @ lcms2.17), which do the same.
fn cms_read_float_input_tag(profile: &mut Profile, tag_float: u32) -> Option<Pipeline> {
    let mut lut = read_pipeline_tag(profile, tag_float)?;
    let spc = profile.color_space;
    let pcs = profile.pcs;

    // input and output of transform are in lcms 0..1 encoding.  If XYZ or Lab spaces are used,
    //  these need to be normalized into the appropriate ranges (Lab = 100,0,0, XYZ=1.0,1.0,1.0)
    if let Some(stage) = normalization(spc, true)
        && !cms_pipeline_insert_stage(&mut lut, StageLoc::AtBegin, stage)
    {
        return None;
    }

    if let Some(stage) = normalization(pcs, false)
        && !cms_pipeline_insert_stage(&mut lut, StageLoc::AtEnd, stage)
    {
        return None;
    }

    Some(lut)
}

/// Port of `_cmsReadInputLUT` (src/cmsio1.c:309-401 @ lcms2.17).
pub fn cms_read_input_lut(profile: &mut Profile, intent: u32) -> Option<Pipeline> {
    if intent <= INTENT_ABSOLUTE_COLORIMETRIC {
        let mut tag16 = DEVICE2PCS16[intent as usize];
        let tag_float = DEVICE2PCS_FLOAT[intent as usize];

        if cms_is_tag(profile, tag_float) {
            // Float tag takes precedence

            // Floating point LUT are always V4, but the encoding range is no
            // longer 0..1.0, so we need to add an stage depending on the color space
            return cms_read_float_input_tag(profile, tag_float);
        }

        // Revert to perceptual if no tag is found
        if !cms_is_tag(profile, tag16) {
            tag16 = DEVICE2PCS16[0];
        }

        if cms_is_tag(profile, tag16) {
            // Is there any LUT-Based table?

            // Check profile version and LUT type. Do the necessary adjustments if needed

            // First read the tag, and the profile owns the Lut, so we need to copy it
            let mut lut = read_pipeline_tag(profile, tag16)?;

            // After reading it, we have now info about the original type
            let original_type = cms_get_tag_true_type(profile, tag16);

            // We need to adjust data only for Lab16 on output
            if original_type != SIG_LUT16_TYPE || profile.pcs != SIG_LAB_DATA {
                return Some(lut);
            }

            // If the input is Lab, add also a conversion at the begin
            if profile.color_space == SIG_LAB_DATA
                && !cms_pipeline_insert_stage(
                    &mut lut,
                    StageLoc::AtBegin,
                    cms_stage_alloc_lab_v4_to_v2(),
                )
            {
                return None;
            }

            // Add a matrix for conversion V2 to V4 Lab PCS
            if !cms_pipeline_insert_stage(&mut lut, StageLoc::AtEnd, cms_stage_alloc_lab_v2_to_v4())
            {
                return None;
            }

            return Some(lut);
        }
    }

    // Lut was not found, try to create a matrix-shaper

    // Not gray, create a normal matrix-shaper
    build_rgb_input_matrix_shaper(profile)
}

/// Port of `BuildRGBOutputMatrixShaper` (src/cmsio1.c:452-513 @ lcms2.17).
fn build_rgb_output_matrix_shaper(profile: &mut Profile) -> Option<Pipeline> {
    let mat = read_icc_matrix_rgb2xyz(profile)?;

    let mut inv = cms_mat3_inverse(&mat)?;

    // XYZ PCS in encoded in 1.15 format, and the matrix input should come in 0..0xffff range, so
    // we need to adjust the input by a << 1 to obtain a 1.16 fixed and then by a factor of
    // (0xffff/0x10000) to put data in 0..0xffff range. Total factor is (2.0*65535.0)/65536.0;

    for row in &mut inv {
        for item in row {
            *item *= OUTP_ADJ;
        }
    }

    let shapes = [
        read_curve_tag(profile, SIG_RED_TRC_TAG),
        read_curve_tag(profile, SIG_GREEN_TRC_TAG),
        read_curve_tag(profile, SIG_BLUE_TRC_TAG),
    ];
    let [Some(red), Some(green), Some(blue)] = shapes else {
        return None;
    };

    let inv_shapes = [
        cms_reverse_tone_curve(&red),
        cms_reverse_tone_curve(&green),
        cms_reverse_tone_curve(&blue),
    ];
    let [Some(inv_red), Some(inv_green), Some(inv_blue)] = inv_shapes else {
        return None;
    };
    let inv_shapes = [inv_red, inv_green, inv_blue];

    let mut lut = cms_pipeline_alloc(3, 3)?;

    // Note that it is certainly possible a single profile would have a LUT based
    // tag for output working in lab and a matrix-shaper for the fallback cases.
    // This is not allowed by the spec, but this code is tolerant to those cases
    if profile.pcs == SIG_LAB_DATA
        && !cms_pipeline_insert_stage(&mut lut, StageLoc::AtEnd, Some(cms_stage_alloc_lab2xyz()))
    {
        return None;
    }

    if !cms_pipeline_insert_stage(
        &mut lut,
        StageLoc::AtEnd,
        cms_stage_alloc_matrix(3, 3, &flat(&inv), None),
    ) || !cms_pipeline_insert_stage(
        &mut lut,
        StageLoc::AtEnd,
        cms_stage_alloc_tone_curves(3, Some(&inv_shapes)),
    ) {
        return None;
    }

    Some(lut)
}

/// Port of `ChangeInterpolationToTrilinear` (src/cmsio1.c:517-534 @ lcms2.17).
fn change_interpolation_to_trilinear(lut: &mut Pipeline) {
    for stage in &mut lut.elements {
        if stage.type_ == SIG_C_LUT_ELEM_TYPE
            && let StageData::Clut(clut) = &mut stage.data
        {
            clut.params.dw_flags |= CMS_LERP_FLAGS_TRILINEAR;
            cms_set_interpolation_routine(&mut clut.params);
        }
    }
}

/// Port of `_cmsReadFloatOutputTag` (src/cmsio1.c:538-579 @ lcms2.17).
fn cms_read_float_output_tag(profile: &mut Profile, tag_float: u32) -> Option<Pipeline> {
    let mut lut = read_pipeline_tag(profile, tag_float)?;
    let pcs = profile.pcs;
    let data_space = profile.color_space;

    // If PCS is Lab or XYZ, the floating point tag is accepting data in the space encoding,
    // and since the formatter has already accommodated to 0..1.0, we should undo this change
    if let Some(stage) = normalization(pcs, true)
        && !cms_pipeline_insert_stage(&mut lut, StageLoc::AtBegin, stage)
    {
        return None;
    }

    // the output can be Lab or XYZ, in which case normalisation is needed on the end of the
    // pipeline
    if let Some(stage) = normalization(data_space, false)
        && !cms_pipeline_insert_stage(&mut lut, StageLoc::AtEnd, stage)
    {
        return None;
    }

    Some(lut)
}

/// Port of `_cmsReadOutputLUT` (src/cmsio1.c:582-658 @ lcms2.17).
pub fn cms_read_output_lut(profile: &mut Profile, intent: u32) -> Option<Pipeline> {
    if intent <= INTENT_ABSOLUTE_COLORIMETRIC {
        let mut tag16 = PCS2DEVICE16[intent as usize];
        let tag_float = PCS2DEVICE_FLOAT[intent as usize];

        if cms_is_tag(profile, tag_float) {
            // Float tag takes precedence

            // Floating point LUT are always V4
            return cms_read_float_output_tag(profile, tag_float);
        }

        // Revert to perceptual if no tag is found
        if !cms_is_tag(profile, tag16) {
            tag16 = PCS2DEVICE16[0];
        }

        if cms_is_tag(profile, tag16) {
            // Is there any LUT-Based table?

            // Check profile version and LUT type. Do the necessary adjustments if needed

            // First read the tag, and the profile owns the Lut, so we need to copy it
            let mut lut = read_pipeline_tag(profile, tag16)?;

            // After reading it, we have info about the original type
            let original_type = cms_get_tag_true_type(profile, tag16);

            // Now it is time for a controversial stuff. I found that for 3D LUTS using
            // Lab used as indexer space,  trilinear interpolation should be used
            if profile.pcs == SIG_LAB_DATA {
                change_interpolation_to_trilinear(&mut lut);
            }

            // We need to adjust data only for Lab and Lut16 type
            if original_type != SIG_LUT16_TYPE || profile.pcs != SIG_LAB_DATA {
                return Some(lut);
            }

            // Add a matrix for conversion V4 to V2 Lab PCS
            if !cms_pipeline_insert_stage(
                &mut lut,
                StageLoc::AtBegin,
                cms_stage_alloc_lab_v4_to_v2(),
            ) {
                return None;
            }

            // If the output is Lab, add also a conversion at the end
            if profile.color_space == SIG_LAB_DATA
                && !cms_pipeline_insert_stage(
                    &mut lut,
                    StageLoc::AtEnd,
                    cms_stage_alloc_lab_v2_to_v4(),
                )
            {
                return None;
            }

            return Some(lut);
        }
    }

    // Lut not found, try to create a matrix-shaper

    // Not gray, create a normal matrix-shaper, which only operates in XYZ space
    build_rgb_output_matrix_shaper(profile)
}

/// Port of `_cmsReadDevicelinkLUT` (src/cmsio1.c:705-801 @ lcms2.17).
pub fn cms_read_devicelink_lut(profile: &mut Profile, intent: u32) -> Option<Pipeline> {
    if intent > INTENT_ABSOLUTE_COLORIMETRIC {
        return None;
    }

    let mut tag16 = DEVICE2PCS16[intent as usize];
    let tag_float = DEVICE2PCS_FLOAT[intent as usize];

    if cms_is_tag(profile, tag_float) {
        // Float tag takes precedence

        // Floating point LUT are always V
        return cms_read_float_input_tag(profile, tag_float);
    }

    let tag_float = DEVICE2PCS_FLOAT[0];
    if cms_is_tag(profile, tag_float) {
        return read_pipeline_tag(profile, tag_float);
    }

    if !cms_is_tag(profile, tag16) {
        // Is there any LUT-Based table?

        tag16 = DEVICE2PCS16[0];
        if !cms_is_tag(profile, tag16) {
            return None;
        }
    }

    // Check profile version and LUT type. Do the necessary adjustments if needed

    // Read the tag, and the profile owns the Lut, so we need to copy it
    let mut lut = read_pipeline_tag(profile, tag16)?;

    // Now it is time for a controversial stuff. I found that for 3D LUTS using
    // Lab used as indexer space,  trilinear interpolation should be used
    if profile.pcs == SIG_LAB_DATA {
        change_interpolation_to_trilinear(&mut lut);
    }

    // After reading it, we have info about the original type
    let original_type = cms_get_tag_true_type(profile, tag16);

    // We need to adjust data for Lab16 on output
    if original_type != SIG_LUT16_TYPE {
        return Some(lut);
    }

    // Here it is possible to get Lab on both sides

    if profile.color_space == SIG_LAB_DATA
        && !cms_pipeline_insert_stage(&mut lut, StageLoc::AtBegin, cms_stage_alloc_lab_v4_to_v2())
    {
        return None;
    }

    if profile.pcs == SIG_LAB_DATA
        && !cms_pipeline_insert_stage(&mut lut, StageLoc::AtEnd, cms_stage_alloc_lab_v2_to_v4())
    {
        return None;
    }

    Some(lut)
}

/// Port of `cmsIsMatrixShaper` (src/cmsio1.c:806-827 @ lcms2.17).
pub fn cms_is_matrix_shaper(profile: &Profile) -> bool {
    match profile.color_space {
        SIG_GRAY_DATA => cms_is_tag(profile, SIG_GRAY_TRC_TAG),

        SIG_RGB_DATA => {
            cms_is_tag(profile, SIG_RED_COLORANT_TAG)
                && cms_is_tag(profile, SIG_GREEN_COLORANT_TAG)
                && cms_is_tag(profile, SIG_BLUE_COLORANT_TAG)
                && cms_is_tag(profile, SIG_RED_TRC_TAG)
                && cms_is_tag(profile, SIG_GREEN_TRC_TAG)
                && cms_is_tag(profile, SIG_BLUE_TRC_TAG)
        }

        _ => false,
    }
}

/// Port of `cmsIsCLUT` (src/cmsio1.c:830-860 @ lcms2.17), with `LCMS_USED_AS_INPUT`: the only way it
/// is asked. A devicelink never gets to it.
fn cms_is_clut_as_input(profile: &Profile, intent: u32) -> bool {
    // Extended intents are not strictly CLUT-based
    if intent > INTENT_ABSOLUTE_COLORIMETRIC {
        return false;
    }

    cms_is_tag(profile, DEVICE2PCS16[intent as usize])
}

/// Port of `cmsIsIntentSupported` (src/cmsio1.c:864-876 @ lcms2.17), with `LCMS_USED_AS_INPUT`.
pub fn cms_is_intent_supported_as_input(profile: &Profile, intent: u32) -> bool {
    if cms_is_clut_as_input(profile, intent) {
        return true;
    }

    // Is there any matrix-shaper? If so, the intent is supported. This is a bit odd, since V2
    // matrix shaper does not fully support relative colorimetric because they cannot deal with
    // non-zero black points, but many profiles claims that, and this is certainly not true for
    // V4 profiles. Lets answer "yes" no matter the accuracy would be less than optimal in rel.col
    // and v2 case.

    cms_is_matrix_shaper(profile)
}
