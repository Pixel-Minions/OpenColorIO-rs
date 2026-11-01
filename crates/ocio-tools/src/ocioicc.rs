// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/apps/ociobakelut/ocioicc.cpp` @ v2.5.2: writes an ICC profile with the
//! translation of lcms2 in [`crate::lcms2`].
//!
//! Where the wheel's tool leaves its memory the port can't follow it: [`Stop`] says so, and the
//! tool ends with an error (docs/improvements.md, U-651 to U-653).

use ocio_ops::cpu_processor::CpuProcessor;

use crate::lcms2::cmserr::{cms_set_log_error_handler, out_of_bounds};
use crate::lcms2::cmsgamma::cms_build_gamma;
use crate::lcms2::cmsio0::*;
use crate::lcms2::cmslut::*;
use crate::lcms2::cmsnamed::{Mlu, cms_mlu_set_ascii};
use crate::lcms2::cmsvirt::*;
use crate::lcms2::cmswtpnt::cms_white_point_from_temp;
use crate::lcms2::cmsxform::*;
use crate::lcms2::types::*;
use crate::stdio::{cerr, cout, cout_endl};

/// Why `SaveICCProfileToFile` can't go on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    /// `cmsWhitePointFromTemp` failed, and the tool would write a `chad` tag out of a white point
    /// nobody set (U-651).
    WhitePoint,
    /// A transform couldn't be created, and the sampler calls `cmsDoTransform` on a null pointer
    /// (U-652).
    NullTransform,
    /// An interpolator reads outside its table (U-653).
    OutOfBounds,
    /// `applyRGB` threw.
    Processor,
}

impl Stop {
    /// What the tool reports instead.
    pub fn message(self) -> &'static str {
        match self {
            Stop::WhitePoint => {
                "ERROR: the whitepoint must be between 4000 and 25000 to write an ICC profile."
            }
            Stop::NullTransform => {
                "ERROR: no transform between the display profile and Lab: the ICC profile can't \
                 be written."
            }
            Stop::OutOfBounds => {
                "ERROR: the display profile's table is read past its end: the ICC profile can't be written."
            }
            Stop::Processor => "ERROR: the CPU processor failed.",
        }
    }
}

/// Port of `ErrorHandler` (src/apps/ociobakelut/ocioicc.cpp:25-29 @ v2.5.2).
fn error_handler(_error_code: u32, text: &[u8]) {
    cerr(b"OCIO Error: ");
    cerr(text);
    cerr(b"\n");
}

/// Port of `Add3GammaCurves` (src/apps/ociobakelut/ocioicc.cpp:39-48 @ v2.5.2).
fn add_3_gamma_curves(lut: &mut Pipeline, curve: f64) {
    let stage = cms_build_gamma(curve).and_then(|id| {
        let id3 = [id.clone(), id.clone(), id];
        cms_stage_alloc_tone_curves(3, Some(&id3))
    });
    cms_pipeline_insert_stage(lut, StageLoc::AtEnd, stage);
}

/// Port of `AddIdentityMatrix` (src/apps/ociobakelut/ocioicc.cpp:50-58 @ v2.5.2).
fn add_identity_matrix(lut: &mut Pipeline) {
    const IDENTITY: [f64; 12] = [
        1.0, 0.0, 0.0, //
        0.0, 1.0, 0.0, //
        0.0, 0.0, 1.0, //
        0.0, 0.0, 0.0,
    ];
    cms_pipeline_insert_stage(
        lut,
        StageLoc::AtEnd,
        cms_stage_alloc_matrix(3, 3, &IDENTITY, None),
    );
}

/// `(cmsUInt16Number)std::max(std::min(pix * 65535.f, 65535.f), 0.f)`: a NaN gets past both
/// comparisons, and `cvttss2si` makes it `0x80000000`, of which the cast keeps 0.
fn sample_to_16(pix: f32) -> u16 {
    let scaled = pix * 65535.0f32;
    let low = if 65535.0f32 < scaled {
        65535.0f32
    } else {
        scaled
    };
    let high = if low < 0.0f32 { 0.0f32 } else { low };
    if high.is_nan() { 0 } else { high as i32 as u16 }
}

/// Port of `Display2PCS_Sampler16` (src/apps/ociobakelut/ocioicc.cpp:60-73 @ v2.5.2).
fn display2pcs_sampler16(
    in_: &[u16],
    out: &mut [u16],
    processor: &CpuProcessor,
    to_pcs16: Option<&Transform>,
    stop: &mut Option<Stop>,
) -> bool {
    let mut pix = [
        in_[0] as f32 / 65535.0f32,
        in_[1] as f32 / 65535.0f32,
        in_[2] as f32 / 65535.0f32,
    ];
    if processor.apply_rgb(&mut pix).is_err() {
        *stop = Some(Stop::Processor);
        return false;
    }
    let rgb = [
        sample_to_16(pix[0]),
        sample_to_16(pix[1]),
        sample_to_16(pix[2]),
    ];
    let Some(to_pcs16) = to_pcs16 else {
        *stop = Some(Stop::NullTransform);
        return false;
    };
    let mut lab = [0u16; 3];
    cms_do_transform(to_pcs16, &rgb, &mut lab);
    out[..3].copy_from_slice(&lab);
    if out_of_bounds() {
        *stop = Some(Stop::OutOfBounds);
        return false;
    }
    true
}

/// Port of `PCS2Display_Sampler16` (src/apps/ociobakelut/ocioicc.cpp:75-82 @ v2.5.2).
fn pcs2display_sampler16(
    in_: &[u16],
    out: &mut [u16],
    from_pcs16: Option<&Transform>,
    stop: &mut Option<Stop>,
) -> bool {
    let Some(from_pcs16) = from_pcs16 else {
        *stop = Some(Stop::NullTransform);
        return false;
    };
    let mut rgb = [0u16; 3];
    cms_do_transform(from_pcs16, &[in_[0], in_[1], in_[2]], &mut rgb);
    // we don't have a reverse Lab -> Display transform
    out[..3].copy_from_slice(&rgb);
    if out_of_bounds() {
        *stop = Some(Stop::OutOfBounds);
        return false;
    }
    true
}

/// `SaveICCProfileToFile`'s calls that take its display profile, which can be null.
fn create_with_display(
    display: Option<&mut Profile>,
    lab: &mut Profile,
    display_first: bool,
) -> Option<Transform> {
    let flags = CMS_FLAGS_NOOPTIMIZE | CMS_FLAGS_NOCACHE;
    match (display, display_first) {
        (Some(display), true) => cms_create_transform(
            display,
            TYPE_RGB_16,
            lab,
            TYPE_LAB_V2_16,
            INTENT_PERCEPTUAL,
            flags,
        ),
        (Some(display), false) => cms_create_transform(
            lab,
            TYPE_LAB_V2_16,
            display,
            TYPE_RGB_16,
            INTENT_PERCEPTUAL,
            flags,
        ),
        // GetXFormColorSpaces (src/cmsxform.c:930 @ lcms2.17) stops at a null first profile.
        (None, true) => {
            crate::lcms2::cmserr::cms_signal_error(
                CMS_ERROR_NULL,
                b"NULL input profiles on transform",
            );
            None
        }
        // cmsCreateTransformTHR makes a one-profile chain of a null second profile.
        (None, false) => cms_create_lab_only_transform(lab, TYPE_LAB_V2_16, TYPE_RGB_16),
    }
}

/// Port of `SaveICCProfileToFile` (src/apps/ociobakelut/ocioicc.cpp:86-224 @ v2.5.2).
#[allow(clippy::too_many_arguments)]
pub fn save_icc_profile_to_file(
    outputfile: &[u8],
    processor: &CpuProcessor,
    cubesize: i32,
    whitepointtemp: i32,
    displayicc: &[u8],
    description: &[u8],
    copyright: &[u8],
    verbose: bool,
) -> Result<(), Stop> {
    // Create the ICC Profile

    // Setup the Error Handler
    cms_set_log_error_handler(error_handler);

    // D65 white point
    // Without one the wheel goes on with memory nobody set, which only reaches the `chad` tags,
    // and only the output's is ever looked at. So the port goes on, too, with what it prints, and
    // stops where the output would be written (U-651).
    let white_point = cms_white_point_from_temp(whitepointtemp as f64);

    // LAB PCS
    let mut lab_profile =
        cms_create_lab4_profile_thr(white_point.as_ref()).ok_or(Stop::WhitePoint)?;

    // Display (OCIO sRGB cube -> LAB)
    let mut display_profile = if !displayicc.is_empty() {
        cms_open_profile_from_file(displayicc)
    } else {
        cms_create_srgb_profile_thr()
    };

    // Create an empty RGB Profile
    let mut h_profile =
        cms_create_rgb_profile_thr(white_point.as_ref(), None, None).ok_or(Stop::WhitePoint)?;

    if verbose {
        cout(b"[OpenColorIO INFO]: Setting up Profile: ");
        cout(outputfile);
        cout(b"\n");
    }

    // Added Header fields
    cms_set_profile_version(&mut h_profile, 4.2);
    h_profile.device_class = SIG_DISPLAY_CLASS;
    h_profile.color_space = SIG_RGB_DATA;
    h_profile.pcs = SIG_LAB_DATA;
    h_profile.rendering_intent = INTENT_PERCEPTUAL;

    //
    let mut description_mlu = Mlu::default();
    let mut copyright_mlu = Mlu::default();
    cms_mlu_set_ascii(&mut description_mlu, b"en", b"US", description);
    cms_mlu_set_ascii(&mut copyright_mlu, b"en", b"US", copyright);
    cms_write_tag(
        &mut h_profile,
        SIG_PROFILE_DESCRIPTION_TAG,
        TagData::Mlu(description_mlu),
    );
    cms_write_tag(
        &mut h_profile,
        SIG_COPYRIGHT_TAG,
        TagData::Mlu(copyright_mlu),
    );

    // 16Bit
    let to_pcs16 = create_with_display(display_profile.as_mut(), &mut lab_profile, true);
    let from_pcs16 = create_with_display(display_profile.as_mut(), &mut lab_profile, false);

    //
    // AToB0Tag - Device to PCS (16-bit) intent of 0 (perceptual)
    //
    // cmsSigCurveSetElemType
    // `- cmsSigCLutElemType
    //  `- cmsSigCurveSetElemType
    //   `- cmsSigMatrixElemType
    //    `- cmsSigCurveSetElemType
    //

    if verbose {
        cout(b"[OpenColorIO INFO]: Adding AToB0Tag\n");
    }
    let mut a_to_b0_tag = cms_pipeline_alloc(3, 3).ok_or(Stop::Processor)?;

    add_3_gamma_curves(&mut a_to_b0_tag, 1.0); // cmsSigCurveSetElemType

    // cmsSigCLutElemType
    let mut a_to_b0_clut = cms_stage_alloc_clut16bit(cubesize as u32, 3, 3, None);

    if verbose {
        cout(b"[OpenColorIO INFO]: Sampling AToB0 CLUT from Display to Lab\n");
    }
    let mut stop = None;
    cms_stage_sample_clut16bit(
        a_to_b0_clut.as_mut(),
        |in_, out| display2pcs_sampler16(in_, out, processor, to_pcs16.as_ref(), &mut stop),
        0,
    );
    if let Some(stop) = stop {
        return Err(stop);
    }
    cms_pipeline_insert_stage(&mut a_to_b0_tag, StageLoc::AtEnd, a_to_b0_clut);

    add_3_gamma_curves(&mut a_to_b0_tag, 1.0); // cmsSigCurveSetElemType
    add_identity_matrix(&mut a_to_b0_tag); // cmsSigMatrixElemType
    add_3_gamma_curves(&mut a_to_b0_tag, 1.0); // cmsSigCurveSetElemType

    // Add AToB0Tag
    cms_write_tag(
        &mut h_profile,
        SIG_A_TO_B0_TAG,
        TagData::Pipeline(a_to_b0_tag),
    );

    //
    // BToA0Tag - PCS to Device space (16-bit) intent of 0 (perceptual)
    //
    // cmsSigCurveSetElemType
    // `- cmsSigMatrixElemType
    //  `- cmsSigCurveSetElemType
    //   `- cmsSigCLutElemType
    //    `- cmsSigCurveSetElemType
    //
    if verbose {
        cout(b"[OpenColorIO INFO]: Adding BToA0Tag\n");
    }
    let mut b_to_a0_tag = cms_pipeline_alloc(3, 3).ok_or(Stop::Processor)?;

    add_3_gamma_curves(&mut b_to_a0_tag, 1.0); // cmsSigCurveSetElemType
    add_identity_matrix(&mut b_to_a0_tag); // cmsSigMatrixElemType
    add_3_gamma_curves(&mut b_to_a0_tag, 1.0); // cmsSigCurveSetElemType

    // cmsSigCLutElemType
    let mut b_to_a0_clut = cms_stage_alloc_clut16bit(cubesize as u32, 3, 3, None);
    if verbose {
        cout(b"[OpenColorIO INFO]: Sampling BToA0 CLUT from Lab to Display\n");
    }
    let mut stop = None;
    cms_stage_sample_clut16bit(
        b_to_a0_clut.as_mut(),
        |in_, out| pcs2display_sampler16(in_, out, from_pcs16.as_ref(), &mut stop),
        0,
    );
    if let Some(stop) = stop {
        return Err(stop);
    }
    cms_pipeline_insert_stage(&mut b_to_a0_tag, StageLoc::AtEnd, b_to_a0_clut);

    add_3_gamma_curves(&mut b_to_a0_tag, 1.0); // cmsSigCurveSetElemType

    // Add BToA0Tag
    cms_write_tag(
        &mut h_profile,
        SIG_B_TO_A0_TAG,
        TagData::Pipeline(b_to_a0_tag),
    );

    //
    // D2Bx - Device to PCS (float) (Not Yet Impl)
    //

    //
    // B2Dx - PCS to Device (float) (Not Yet Impl)
    //

    //
    // Write
    //
    if verbose {
        cout(b"[OpenColorIO INFO]: Writing ");
        cout(outputfile);
        cout_endl();
    }
    if white_point.is_none() {
        // What the wheel's save would report about the tags, on no file.
        cms_save_profile_to_io_handler(&mut h_profile, &mut IoHandler::null());
        return Err(Stop::WhitePoint);
    }
    cms_save_profile_to_file(&mut h_profile, outputfile);

    if verbose {
        cout(b"[OpenColorIO INFO]: Finished\n");
    }

    Ok(())
}
