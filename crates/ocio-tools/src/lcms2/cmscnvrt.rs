// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/cmscnvrt.c` @ lcms2.17: linking profiles, with the ICC intents.
//!
//! The tool asks for the perceptual intent, and the black point detection for the relative
//! colorimetric one. So `SearchIntent` always finds `DefaultICCintents`, and the absolute intent
//! and the black-preserving ones aren't reached.

use super::cmserr::cms_signal_error;
use super::cmsio0::Profile;
use super::cmsio1::{cms_read_devicelink_lut, cms_read_input_lut, cms_read_output_lut};
use super::cmslut::*;
use super::cmsmtrx::{Mat3, Vec3, cms_mat3_identity};
use super::cmssamp::{cms_detect_black_point, cms_detect_destination_black_point};
use super::cmswtpnt::cms_d50_xyz;
use super::types::*;

/// Port of `ComputeBlackPointCompensation` (src/cmscnvrt.c:168-200 @ lcms2.17).
fn compute_black_point_compensation(
    black_point_in: &CieXyz,
    black_point_out: &CieXyz,
    m: &mut Mat3,
    off: &mut Vec3,
) {
    // Now we need to compute a matrix plus an offset m and of such of
    // [m]*bpin + off = bpout
    // [m]*D50  + off = D50
    //
    // This is a linear scaling in the form ax+b, where
    // a = (bpout - D50) / (bpin - D50)
    // b = - D50* (bpout - bpin) / (bpin - D50)

    let d50 = cms_d50_xyz();

    let tx = black_point_in.x - d50.x;
    let ty = black_point_in.y - d50.y;
    let tz = black_point_in.z - d50.z;

    let ax = (black_point_out.x - d50.x) / tx;
    let ay = (black_point_out.y - d50.y) / ty;
    let az = (black_point_out.z - d50.z) / tz;

    let bx = -d50.x * (black_point_out.x - black_point_in.x) / tx;
    let by = -d50.y * (black_point_out.y - black_point_in.y) / ty;
    let bz = -d50.z * (black_point_out.z - black_point_in.z) / tz;

    *m = [[ax, 0.0, 0.0], [0.0, ay, 0.0], [0.0, 0.0, az]];
    *off = [bx, by, bz];
}

/// Port of `IsEmptyLayer` (src/cmscnvrt.c:328-348 @ lcms2.17).
fn is_empty_layer(m: &Mat3, off: &Vec3) -> bool {
    let mut diff = 0f64;

    let ident = cms_mat3_identity();

    for i in 0..3 {
        for j in 0..3 {
            diff += (m[i][j] - ident[i][j]).abs();
        }
    }

    for item in off {
        diff += item.abs();
    }

    diff < 0.002
}

/// Port of `ComputeConversion` (src/cmscnvrt.c:352-416 @ lcms2.17).
fn compute_conversion(
    i: usize,
    profiles: &mut [&mut Profile],
    intent: u32,
    bpc: bool,
    m: &mut Mat3,
    off: &mut Vec3,
) {
    // m  and off are set to identity and this is detected latter on
    *m = cms_mat3_identity();
    *off = [0.0, 0.0, 0.0];

    // Rest of intents may apply BPC.

    if bpc {
        let mut black_point_in = CieXyz::default();
        let mut black_point_out = CieXyz::default();

        cms_detect_black_point(&mut black_point_in, profiles[i - 1], intent);
        cms_detect_destination_black_point(&mut black_point_out, profiles[i], intent);

        // If black points are equal, then do nothing
        if black_point_in.x != black_point_out.x
            || black_point_in.y != black_point_out.y
            || black_point_in.z != black_point_out.z
        {
            compute_black_point_compensation(&black_point_in, &black_point_out, m, off);
        }
    }

    // Offset should be adjusted because the encoding. We encode XYZ normalized to 0..1.0,
    // to do that, we divide by MAX_ENCODEABLE_XZY. The conversion stage goes XYZ -> XYZ so
    // we have first to convert from encoded to XYZ and then convert back to encoded.
    // y = Mx + Off
    // x = x'c
    // y = M x'c + Off
    // y = y'c; y' = y / c
    // y' = (Mx'c + Off) /c = Mx' + (Off / c)

    for item in off.iter_mut() {
        *item /= MAX_ENCODEABLE_XYZ;
    }
}

/// `cmsStageAllocMatrix(ContextID, 3, 3, m_as_dbl, off_as_dbl)` of `AddConversion`.
fn conversion_matrix(m: &Mat3, off: &Vec3) -> Option<Stage> {
    let flat: Vec<f64> = m.iter().flatten().copied().collect();
    cms_stage_alloc_matrix(3, 3, &flat, Some(off))
}

/// Port of `AddConversion` (src/cmscnvrt.c:420-487 @ lcms2.17).
fn add_conversion(result: &mut Pipeline, in_pcs: u32, out_pcs: u32, m: &Mat3, off: &Vec3) -> bool {
    // Handle PCS mismatches. A specialized stage is added to the LUT in such case
    match in_pcs {
        // Input profile operates in XYZ
        SIG_XYZ_DATA => match out_pcs {
            // XYZ -> XYZ
            SIG_XYZ_DATA => {
                if !is_empty_layer(m, off)
                    && !cms_pipeline_insert_stage(
                        result,
                        StageLoc::AtEnd,
                        conversion_matrix(m, off),
                    )
                {
                    return false;
                }
            }

            // XYZ -> Lab
            SIG_LAB_DATA => {
                if !is_empty_layer(m, off)
                    && !cms_pipeline_insert_stage(
                        result,
                        StageLoc::AtEnd,
                        conversion_matrix(m, off),
                    )
                {
                    return false;
                }
                if !cms_pipeline_insert_stage(
                    result,
                    StageLoc::AtEnd,
                    Some(cms_stage_alloc_xyz2lab()),
                ) {
                    return false;
                }
            }

            _ => return false, // Colorspace mismatch
        },

        // Input profile operates in Lab
        SIG_LAB_DATA => match out_pcs {
            // Lab -> XYZ
            SIG_XYZ_DATA => {
                if !cms_pipeline_insert_stage(
                    result,
                    StageLoc::AtEnd,
                    Some(cms_stage_alloc_lab2xyz()),
                ) {
                    return false;
                }
                if !is_empty_layer(m, off)
                    && !cms_pipeline_insert_stage(
                        result,
                        StageLoc::AtEnd,
                        conversion_matrix(m, off),
                    )
                {
                    return false;
                }
            }

            // Lab -> Lab
            SIG_LAB_DATA => {
                if !is_empty_layer(m, off)
                    && (!cms_pipeline_insert_stage(
                        result,
                        StageLoc::AtEnd,
                        Some(cms_stage_alloc_lab2xyz()),
                    ) || !cms_pipeline_insert_stage(
                        result,
                        StageLoc::AtEnd,
                        conversion_matrix(m, off),
                    ) || !cms_pipeline_insert_stage(
                        result,
                        StageLoc::AtEnd,
                        Some(cms_stage_alloc_xyz2lab()),
                    ))
                {
                    return false;
                }
            }

            _ => return false, // Mismatch
        },

        // On colorspaces other than PCS, check for same space
        _ => {
            if in_pcs != out_pcs {
                return false;
            }
        }
    }

    true
}

/// Port of `ColorSpaceIsCompatible` (src/cmscnvrt.c:491-506 @ lcms2.17).
fn color_space_is_compatible(a: u32, b: u32) -> bool {
    // If they are same, they are compatible.
    if a == b {
        return true;
    }

    // Check for MCH4 substitution of CMYK
    if a == SIG_4COLOR_DATA && b == SIG_CMYK_DATA {
        return true;
    }
    if a == SIG_CMYK_DATA && b == SIG_4COLOR_DATA {
        return true;
    }

    // Check for XYZ/Lab. Those spaces are interchangeable as they can be computed one from other.
    if a == SIG_XYZ_DATA && b == SIG_LAB_DATA {
        return true;
    }
    if a == SIG_LAB_DATA && b == SIG_XYZ_DATA {
        return true;
    }

    false
}

/// Port of `DefaultICCintents` (src/cmscnvrt.c:510-651 @ lcms2.17), with `dwFlags` that don't include
/// `cmsFLAGS_NONEGATIVES`.
fn default_icc_intents(
    the_intents: &[u32],
    profiles: &mut [&mut Profile],
    bpc: &[bool],
) -> Option<Pipeline> {
    let n_profiles = profiles.len();

    // For safety
    if n_profiles == 0 {
        return None;
    }

    // Allocate an empty LUT for holding the result. 0 as channel count means 'undefined'
    let mut result = cms_pipeline_alloc(0, 0)?;

    let mut current_color_space = profiles[0].color_space;
    let mut m = cms_mat3_identity();
    let mut off: Vec3 = [0.0; 3];

    for i in 0..n_profiles {
        let class_sig = profiles[i].device_class;
        let is_device_link = class_sig == SIG_LINK_CLASS || class_sig == SIG_ABSTRACT_CLASS;

        // First profile is used as input unless devicelink or abstract
        let is_input = if i == 0 && !is_device_link {
            true
        } else {
            // Else use profile in the input direction if current space is not PCS
            current_color_space != SIG_XYZ_DATA && current_color_space != SIG_LAB_DATA
        };

        let intent = the_intents[i];

        let (color_space_in, color_space_out) = if is_input || is_device_link {
            (profiles[i].color_space, profiles[i].pcs)
        } else {
            (profiles[i].pcs, profiles[i].color_space)
        };

        if !color_space_is_compatible(color_space_in, current_color_space) {
            cms_signal_error(CMS_ERROR_COLORSPACE_CHECK, b"ColorSpace mismatch");
            return None;
        }

        let lut;

        // If devicelink is found, then no custom intent is allowed and we can
        // read the LUT to be applied. Settings don't apply here.
        if is_device_link {
            // Get the involved LUT from the profile
            lut = cms_read_devicelink_lut(profiles[i], intent)?;

            // What about abstract profiles?
            if class_sig == SIG_ABSTRACT_CLASS && i > 0 {
                compute_conversion(i, profiles, intent, bpc[i], &mut m, &mut off);
            } else {
                m = cms_mat3_identity();
                off = [0.0; 3];
            }

            if !add_conversion(&mut result, current_color_space, color_space_in, &m, &off) {
                return None;
            }
        } else if is_input {
            // Input direction means non-pcs connection, so proceed like devicelinks
            lut = cms_read_input_lut(profiles[i], intent)?;
        } else {
            // Output direction means PCS connection. Intent may apply here
            lut = cms_read_output_lut(profiles[i], intent)?;

            compute_conversion(i, profiles, intent, bpc[i], &mut m, &mut off);
            if !add_conversion(&mut result, current_color_space, color_space_in, &m, &off) {
                return None;
            }
        }

        // Concatenate to the output LUT
        if !cms_pipeline_cat(&mut result, &lut) {
            return None;
        }

        // Update current space
        current_color_space = color_space_out;
    }

    Some(result)
}

/// Port of `_cmsLinkProfiles` (src/cmscnvrt.c:1102-1150 @ lcms2.17).
pub fn cms_link_profiles(
    the_intents: &[u32],
    profiles: &mut [&mut Profile],
    bpc: &mut [bool],
) -> Option<Pipeline> {
    for i in 0..profiles.len() {
        // Check if black point is really needed or allowed. Note that
        // following Adobe's document:
        // BPC does not apply to devicelink profiles, nor to abs colorimetric,
        // and applies always on V4 perceptual and saturation.

        if the_intents[i] == INTENT_ABSOLUTE_COLORIMETRIC {
            bpc[i] = false;
        }

        if the_intents[i] == INTENT_PERCEPTUAL || the_intents[i] == INTENT_SATURATION {
            // Force BPC for V4 profiles in perceptual and saturation
            if profiles[i].version >= 0x4000000 {
                bpc[i] = true;
            }
        }
    }

    // Call the handler
    default_icc_intents(the_intents, profiles, bpc)
}
