// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The ICC profile format (`.icc`, `.icm`, `.pf`): matrix/TRC monitor profiles, converted
//! into a matrix and a gamma or a 1D LUT. A port of
//! `src/OpenColorIO/fileformats/FileFormatICC.cpp` @ v2.5.2, over the SampleICC subset of
//! `icc_profile_reader.rs`.
//!
//! ICC color management is the de facto standard in areas such as printing and OS-level color
//! management. ICC profiles are a widely used method of storing color information for computer
//! displays and that is the main purpose of this format reader. The "matrix/TRC" model for a
//! monitor is parsed and converted into an OCIO compatible form. Other types of ICC profiles
//! are not currently supported in this reader.

use std::any::Any;
use std::sync::Arc;

use ocio_ops::exception::{Exception, Result};
use ocio_ops::logging::log_warning;
use ocio_ops::math_utils::std_powf;
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{BitDepth, TransformDirection, combine_transform_directions};
use ocio_ops::ops::gamma::gamma_op::create_gamma_op;
use ocio_ops::ops::gamma::gamma_op_data::{GammaOpData, GammaStyle};
use ocio_ops::ops::lut1d::Lut1DOpData;
use ocio_ops::ops::lut1d::lut1d_op::create_lut1d_op;
use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;
use ocio_ops::ops::matrix::matrix_op::create_matrix_op_from_m44;
use ocio_ops::utils::pystring::os_path;

use crate::config::Config;
use crate::context::Context;
use crate::fileformats::file_format_utils::{handle_lut1d, log_warning_interpolation_not_used};
use crate::fileformats::icc::icc_profile_reader::{
    IC_MAGIC_NUMBER, IC_SIG_BLUE_COLORANT_TAG, IC_SIG_BLUE_TRC_TAG, IC_SIG_GREEN_COLORANT_TAG,
    IC_SIG_GREEN_TRC_TAG, IC_SIG_PROFILE_DESCRIPTION_ML_TAG, IC_SIG_PROFILE_DESCRIPTION_TAG,
    IC_SIG_RED_COLORANT_TAG, IC_SIG_RED_TRC_TAG, IcS15Fixed16Number, IccContent, IccTagElement,
    IccTypeReader, ic_f_to_d, read8, read16, read32, read32_signed, read64,
};
use crate::fileformats::input_stream::{InputStream, OpenMode};
use crate::transforms::file_format::{
    CachedFile, CachedFileRcPtr, FileFormat, FormatBakeCapabilities, FormatCapabilities, FormatInfo,
};
use crate::transforms::file_transform::FileTransform;

/// An ICC profile's description, matrix, and gamma or LUT.
///
/// Port of `LocalCachedFile` (FileFormatICC.cpp:36-53 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct LocalCachedFile {
    /// `mProfileDescription`.
    pub profile_description: Vec<u8>,
    /// `mMatrix44`.
    pub matrix44: [f64; 16],
    /// `mGammaRGB`.
    pub gamma_rgb: [f32; 4],
    /// `lut`.
    pub lut: Option<Lut1DOpData>,
}

impl Default for LocalCachedFile {
    /// Port of `LocalCachedFile`'s initializers: a zero matrix, a gamma of 1 then 0s (only
    /// `mGammaRGB[0]` is initialized to 1), no LUT.
    fn default() -> Self {
        LocalCachedFile {
            profile_description: Vec::new(),
            matrix44: [0.0; 16],
            gamma_rgb: [1.0, 0.0, 0.0, 0.0],
            lut: None,
        }
    }
}

impl CachedFile for LocalCachedFile {}

/// The ICC profile format.
///
/// Port of `LocalFileFormat` (FileFormatICC.cpp:57-96 @ v2.5.2).
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalFileFormat;

/// `Error parsing .icc file (<file>).  <error>`.
///
/// Port of `LocalFileFormat::ThrowErrorMessage` (FileFormatICC.cpp:117-127 @ v2.5.2).
fn throw_error_message(error: &[u8], file_name: &[u8]) -> Exception {
    Exception::new(
        [
            b"Error parsing .icc file (".as_slice(),
            file_name,
            b").  ",
            error,
        ]
        .concat(),
    )
}

/// The profile's header and tag table, validated, and its description (the `dscm` tag, else
/// the `desc` tag, else none).
///
/// Port of `LocalFileFormat::ReadInfo` (FileFormatICC.cpp:129-252 @ v2.5.2).
pub fn read_info(
    istream: &mut InputStream,
    file_name: &[u8],
    icc: &mut IccContent,
) -> Result<LocalCachedFile> {
    istream.seekg(0);
    let h = &mut icc.header;
    let header_read = istream.good()
        && read32(istream, &mut h.size)
        && read32(istream, &mut h.cmm_id)
        && read32(istream, &mut h.version)
        && read32(istream, &mut h.device_class)
        && read32(istream, &mut h.color_space)
        && read32(istream, &mut h.pcs)
        && read16(istream, &mut h.date.year)
        && read16(istream, &mut h.date.month)
        && read16(istream, &mut h.date.day)
        && read16(istream, &mut h.date.hours)
        && read16(istream, &mut h.date.minutes)
        && read16(istream, &mut h.date.seconds)
        && read32(istream, &mut h.magic)
        && read32(istream, &mut h.platform)
        && read32(istream, &mut h.flags)
        && read32(istream, &mut h.manufacturer)
        && read32(istream, &mut h.model)
        && read64(istream, &mut h.attributes)
        && read32(istream, &mut h.rendering_intent)
        && read32_signed(istream, &mut h.illuminant.x)
        && read32_signed(istream, &mut h.illuminant.y)
        && read32_signed(istream, &mut h.illuminant.z)
        && read32(istream, &mut h.creator)
        && read8(istream, &mut h.profile_id, 16) == 16
        && read8(istream, &mut h.reserved, 28) == 28;
    if !header_read {
        return Err(throw_error_message(b"Error loading header.", file_name));
    }

    if icc.header.magic != IC_MAGIC_NUMBER {
        return Err(throw_error_message(b"Wrong magic number.", file_name));
    }

    let mut count = 0u32;
    if !read32(istream, &mut count) {
        return Err(throw_error_message(
            b"Error loading number of tags.",
            file_name,
        ));
    }

    const MAX_NUM_ICC_TAGS: u32 = 100;
    if count > MAX_NUM_ICC_TAGS {
        return Err(throw_error_message(
            b"Too many tags in ICC profile.",
            file_name,
        ));
    }
    icc.tags = vec![IccTagElement::default(); count as usize];

    // Read Tag offset table.
    for tag in icc.tags.iter_mut() {
        let info = &mut tag.tag_info;
        if !read32(istream, &mut info.sig)
            || !read32(istream, &mut info.offset)
            || !read32(istream, &mut info.size)
        {
            return Err(throw_error_message(
                b"Error loading tag offset table from header.",
                file_name,
            ));
        }
    }

    // Validate
    if let Err(error) = icc.validate() {
        return Err(throw_error_message(&error, file_name));
    }

    let mut cached_file = LocalCachedFile::default();

    // Get the profile description.
    {
        // First try the Apple private 'dscm' tag, which tends to have more accurate
        // descriptions in Apple profiles. Fall back to the standard 'desc' tag if 'dscm' is
        // not present.
        let mut reader = icc
            .load_tag(istream, IC_SIG_PROFILE_DESCRIPTION_ML_TAG)
            .cloned();
        if reader.is_none() {
            reader = icc
                .load_tag(istream, IC_SIG_PROFILE_DESCRIPTION_TAG)
                .cloned();
        }

        match reader {
            // The tags are missing.
            None => cached_file.profile_description = Vec::new(),
            Some(IccTypeReader::TextDescription(text)) => cached_file.profile_description = text,
            // The profile description implementation is a list of localized unicode strings.
            // But the OCIO implementation only returns the english string.
            Some(IccTypeReader::MultiLocalizedUnicode(text)) => {
                cached_file.profile_description = text;
            }
            Some(_) => {
                return Err(throw_error_message(
                    b"The 'desc' (or 'dcsm') reader is missing.",
                    file_name,
                ));
            }
        }
    }

    Ok(cached_file)
}

/// `std::lround(float)`, the C runtime's `lroundf`: the nearest integer, halves away from
/// zero, as a C `long` (32 bits on Windows, 64 on Linux). Out of range, each library returns
/// its own value: the UCRT 0 for a NaN or a value past 2^31 in magnitude, and `LONG_MIN` for
/// 2^31 (it wraps); glibc the conversion instruction's "integer indefinite", `LONG_MIN`, for a
/// NaN or a value of 2^63 or more in magnitude. Checked against the platform's `lroundf`.
pub(crate) fn lround(v: f32) -> i64 {
    const TWO_31: f32 = 2_147_483_648.0;
    const TWO_63: f32 = 9_223_372_036_854_775_808.0;
    let r = v.round();
    if cfg!(windows) {
        if r.is_nan() || r < -TWO_31 || r > TWO_31 {
            0
        } else {
            i64::from(r as i64 as i32)
        }
    } else if r.is_nan() || r <= -TWO_63 || r >= TWO_63 {
        i64::MIN
    } else {
        r as i64
    }
}

/// `v` rounded to a `bitdepth`-bit code value, as a float.
///
/// Port of `QuantizeF` (FileFormatICC.cpp:298-302 @ v2.5.2): `std::lround(v * maxVal) /
/// maxVal`, the `long` converted to `float`.
fn quantize_f(v: f32, bitdepth: u8) -> f32 {
    let max_val = std_powf(2.0, f32::from(bitdepth)) - 1.0;
    lround(v * max_val) as f32 / max_val
}

/// The parameters as upstream prints them in its messages: each `icFtoD` value through
/// `std::ostream` (6 significant digits), separated by spaces.
fn params_text(params: &[IcS15Fixed16Number]) -> Vec<u8> {
    use ocio_ops::cfmt::{Crt, OStringStream};
    let mut oss = OStringStream::new(Crt::NATIVE);
    for (i, &p) in params.iter().enumerate() {
        if i != 0 {
            oss.put_str(" ");
        }
        oss.put_f32(ic_f_to_d(p));
    }
    oss.into_bytes()
}

/// Checks a parametric curve: its type and parameter count, a power law and a linear segment
/// that don't decrease, no discontinuity going down, no negative argument to the power; and
/// logs warnings for a curve that doesn't reach (1, 1), isn't continuous or isn't smooth.
///
/// Port of `LocalFileFormat::ValidateParametricCurve` (FileFormatICC.cpp:258-470 @ v2.5.2).
// Upstream's branches, as written: some log the same warning.
#[allow(clippy::if_same_then_else)]
fn validate_parametric_curve(
    curve_type: u16,
    params: &[IcS15Fixed16Number],
    file_name: &[u8],
) -> Result<()> {
    let num_params = params.len();
    let throw_para_error = |msg: &[u8]| -> Exception {
        let oss = [
            b"Error parsing ICC Parametric Curve (with arguments ".as_slice(),
            &params_text(params),
            b"): ",
            msg,
        ]
        .concat();
        throw_error_message(&oss, file_name)
    };
    let log_para_warning = |msg: &[u8]| {
        let oss = [
            b"Parsing .icc file (".as_slice(),
            file_name,
            b").  ICC Parametric Curve (with arguments ",
            &params_text(params),
            b"): ",
            msg,
        ]
        .concat();
        log_warning(oss);
    };
    let p = |i: usize| ic_f_to_d(params[i]);

    // Expected number of arguments
    let expected: u16 = match curve_type {
        0 => 1,
        1 => 3,
        2 => 4,
        3 => 5,
        4 => 7,
        _ => return Err(throw_para_error(b"Unknown parametric curve type.")),
    };

    if num_params != usize::from(expected) {
        return Err(throw_para_error(
            format!("Expecting {expected}param(s).").as_bytes(),
        ));
    }

    // Monotonically non-decreasing (flat segments permitted)
    let g = p(0);

    // Forces the power law to be monotonically non-decreasing.
    if f64::from(g) <= 0.0 {
        return Err(throw_para_error(
            b"Expecting monotonically non-decreasing power-law.",
        ));
    }

    // Forces the argument to the power law to be an increasing function.
    if curve_type != 0 {
        let a = p(1);
        if f64::from(a) <= 0.0 {
            return Err(throw_para_error(
                b"Expecting strictly increasing argument to power-law.",
            ));
        }
    }

    // Forces the linear segment to be flat or increasing.
    if curve_type == 3 || curve_type == 4 {
        let c = p(3);
        if f64::from(c) < 0.0 {
            return Err(throw_para_error(
                b"Expecting flat or increasing linear segment.",
            ));
        }
    }

    // Look for negative discontinuity at the linear segment / power law boundary.
    if curve_type == 3 {
        let (a, b, c, d) = (p(1), p(2), p(3), p(4));
        let lin_segment_break = quantize_f(c * d, 10);
        let power_law_break = quantize_f((a * d + b).powf(g), 10);
        if lin_segment_break > power_law_break {
            return Err(throw_para_error(
                b"Expecting no negative discontinuity at linear segment boundary.",
            ));
        }
    } else if curve_type == 4 {
        let (a, b, c, d, e, f) = (p(1), p(2), p(3), p(4), p(5), p(6));
        let lin_segment_break = quantize_f(c * d + f, 10);
        let power_law_break = quantize_f((a * d + b).powf(g) + e, 10);
        if lin_segment_break > power_law_break {
            return Err(throw_para_error(
                b"Expecting no negative discontinuity at linear segment boundary.",
            ));
        }
    }

    // No complex / imaginary numbers
    if curve_type == 3 || curve_type == 4 {
        let (a, b, d) = (p(1), p(2), p(4));
        if (a * d + b) < 0.0 {
            return Err(throw_para_error(
                b"Expecting no negative arguments to the power law.",
            ));
        }
    }

    // Boundary warnings
    if curve_type == 1 || curve_type == 2 {
        let (a, b) = (p(1), p(2));
        let c = if curve_type == 2 { p(3) } else { 0.0 };

        // Breakpoint is at x = -b/a, assuming a > 0, b should be negative so that the
        // breakpoint occurs at positive x values.
        if f64::from(b) >= 0.0 {
            log_para_warning(b"Expecting b < 0 for linear segment to occur at positive x values.");
        }

        if curve_type == 1 && quantize_f(a + b, 8) != 1.0 {
            log_para_warning(b"Curve does not reach maximum at (1,1).");
        } else if curve_type == 2 && quantize_f((a + b).powf(g) + c, 8) != 1.0 {
            log_para_warning(b"Curve does not reach maximum at (1,1).");
        }
    }

    // Continuity warnings
    // Note that type 0, 1, 2 are continuous by definition.
    if curve_type == 3 || curve_type == 4 {
        let (a, b, c, d) = (p(1), p(2), p(3), p(4));
        let e = if curve_type == 4 { p(5) } else { 0.0 };
        let f = if curve_type == 4 { p(6) } else { 0.0 };

        if curve_type == 3 && quantize_f(c * d, 8) != quantize_f((a * d + b).powf(g), 8) {
            log_para_warning(b"Curve is not continuous.");
        } else if curve_type == 4
            && quantize_f(c * d + f, 8) != quantize_f((a * d + b).powf(g) + e, 8)
        {
            log_para_warning(b"Curve is not continuous.");
        }
    }

    // Smoothness warnings
    // Note that type 0 is smooth by definition.
    if curve_type == 1 || curve_type == 2 {
        let (a, b) = (p(1), p(2));
        if g <= 1.0 && -b / a > 0.0 {
            log_para_warning(b"Curve is not smooth (first derivative).");
        }
    } else if curve_type == 3 || curve_type == 4 {
        let (a, b, c, d) = (p(1), p(2), p(3), p(4));
        if quantize_f(c, 8) != quantize_f(a * g * (a * d + b).powf(g - 1.0), 8) {
            log_para_warning(b"Curve is not smooth (first derivative).");
        }
    }
    Ok(())
}

/// C++ `std::max(a, b)`: `(a < b) ? b : a`.
fn std_max(a: f32, b: f32) -> f32 {
    if a < b { b } else { a }
}

/// C++ `std::min(a, b)`: `(b < a) ? b : a`.
fn std_min(a: f32, b: f32) -> f32 {
    if b < a { b } else { a }
}

/// A parametric curve of type 1 to 4 at `v`, both clamped to [0, 1].
///
/// Port of `LocalFileFormat::ApplyParametricCurve` (FileFormatICC.cpp:474-533 @ v2.5.2).
fn apply_parametric_curve(v: f32, curve_type: u16, params: &[IcS15Fixed16Number]) -> f32 {
    let mut v = std_min(std_max(0.0, v), 1.0);
    let p = |i: usize| ic_f_to_d(params[i]);

    if curve_type == 1 {
        // y = (ax+b)^g  (x >= -b/a)
        // y = 0         (x < -b/a)
        let (g, a, b) = (p(0), p(1), p(2));
        v = if v >= (-b / a) {
            (a * v + b).powf(g)
        } else {
            0.0
        };
    } else if curve_type == 2 {
        // y = (ax+b)^g + c  (x >= -b/a)
        // y = c             (x < -b/a)
        let (g, a, b, c) = (p(0), p(1), p(2), p(3));
        v = if v >= (-b / a) {
            (a * v + b).powf(g) + c
        } else {
            c
        };
    } else if curve_type == 3 {
        // y = (ax+b)^g  (x >= d)
        // y = cx        (x < d)
        let (g, a, b, c, d) = (p(0), p(1), p(2), p(3), p(4));
        v = if v >= d { (a * v + b).powf(g) } else { c * v };
    } else if curve_type == 4 {
        // y = (ax+b)^g + e  (x >= d)
        // y = cx+f          (x < d)
        let (g, a, b, c, d, e, f) = (p(0), p(1), p(2), p(3), p(4), p(5), p(6));
        v = if v >= d {
            (a * v + b).powf(g) + e
        } else {
            c * v + f
        };
    }

    std_min(std_max(0.0, v), 1.0)
}

impl LocalFileFormat {
    /// Reads an ICC profile: its description, its matrix (the red, green and blue colorant
    /// tags), and its TRC: one parametric curve type on the three channels (type 0 as a gamma,
    /// types 1 to 4 as a 1024-entry LUT), or curves of one length (one entry as a u8.8 gamma,
    /// more as a 16-bit LUT).
    ///
    /// Port of `LocalFileFormat::read` (FileFormatICC.cpp:537-743 @ v2.5.2).
    pub fn read_file(
        &self,
        istream: &mut InputStream,
        file_name: &[u8],
    ) -> Result<LocalCachedFile> {
        let mut icc = IccContent::default();
        let mut cached_file = read_info(istream, file_name, &mut icc)?;

        // Matrix part of the Matrix/TRC Model
        {
            let xyz = |icc: &mut IccContent, istream: &mut InputStream, sig| match icc
                .load_tag(istream, sig)
            {
                Some(IccTypeReader::XyzArray(xyz)) => Some(*xyz),
                _ => None,
            };
            let red = xyz(&mut icc, istream, IC_SIG_RED_COLORANT_TAG);
            let green = xyz(&mut icc, istream, IC_SIG_GREEN_COLORANT_TAG);
            let blue = xyz(&mut icc, istream, IC_SIG_BLUE_COLORANT_TAG);

            let (Some(red), Some(green), Some(blue)) = (red, green, blue) else {
                return Err(throw_error_message(
                    b"Illegal matrix tag in ICC profile.",
                    file_name,
                ));
            };

            let m = &mut cached_file.matrix44;
            m[0] = f64::from(red.x) / 65536.0;
            m[1] = f64::from(green.x) / 65536.0;
            m[2] = f64::from(blue.x) / 65536.0;
            m[3] = 0.0;

            m[4] = f64::from(red.y) / 65536.0;
            m[5] = f64::from(green.y) / 65536.0;
            m[6] = f64::from(blue.y) / 65536.0;
            m[7] = 0.0;

            m[8] = f64::from(red.z) / 65536.0;
            m[9] = f64::from(green.z) / 65536.0;
            m[10] = f64::from(blue.z) / 65536.0;
            m[11] = 0.0;

            m[12] = 0.0;
            m[13] = 0.0;
            m[14] = 0.0;
            m[15] = 1.0;
        }

        // Extract the "B" Curve part of the Matrix/TRC Model
        let red_trc = icc.load_tag(istream, IC_SIG_RED_TRC_TAG).cloned();
        let green_trc = icc.load_tag(istream, IC_SIG_GREEN_TRC_TAG).cloned();
        let blue_trc = icc.load_tag(istream, IC_SIG_BLUE_TRC_TAG).cloned();
        let (Some(red_trc), Some(green_trc), Some(blue_trc)) = (red_trc, green_trc, blue_trc)
        else {
            return Err(throw_error_message(
                b"Illegal curve tag in ICC profile.",
                file_name,
            ));
        };

        let same_type: &[u8] = b"All curves in the ICC profile must be of the same type.";
        if red_trc.is_parametric_curve() {
            if !green_trc.is_parametric_curve() || !blue_trc.is_parametric_curve() {
                return Err(throw_error_message(same_type, file_name));
            }

            let parametric = |r: &IccTypeReader| match r {
                IccTypeReader::ParametricCurve {
                    function_type,
                    params,
                } => (*function_type, params.clone()),
                _ => unreachable!("a parametric curve's reader"),
            };
            let (red_type, red) = parametric(&red_trc);
            let (green_type, green) = parametric(&green_trc);
            let (blue_type, blue) = parametric(&blue_trc);

            // Red, Green and blue curves must be of the same function type.
            if red_type != green_type || red_type != blue_type {
                return Err(throw_error_message(same_type, file_name));
            }

            validate_parametric_curve(red_type, &red, file_name)?;
            validate_parametric_curve(green_type, &green, file_name)?;
            validate_parametric_curve(blue_type, &blue, file_name)?;

            // Handle type 0 with a GammaOp.
            if red_type == 0 {
                if red.len() != 1 || green.len() != 1 || blue.len() != 1 {
                    return Err(throw_error_message(
                        b"Expecting 1 param in parametric curve tag (type 0) of ICC profile.",
                        file_name,
                    ));
                }

                cached_file.gamma_rgb[0] = ic_f_to_d(red[0]);
                cached_file.gamma_rgb[1] = ic_f_to_d(green[0]);
                cached_file.gamma_rgb[2] = ic_f_to_d(blue[0]);
                cached_file.gamma_rgb[3] = 1.0;
            }
            // Handle type 1-4 with a 1DLUTOp.
            else {
                const LUT_LENGTH: std::ffi::c_ulong = 1024;
                let mut lut = Lut1DOpData::new(LUT_LENGTH)?;
                lut.set_file_output_bit_depth(BitDepth::F32);

                let lut_data = lut.get_array_mut().get_values_mut();
                for i in 0..LUT_LENGTH {
                    let v = i as f32 / (LUT_LENGTH as f32 - 1.0);
                    let i = i as usize;
                    lut_data[i * 3] = apply_parametric_curve(v, red_type, &red);
                    lut_data[i * 3 + 1] = apply_parametric_curve(v, green_type, &green);
                    lut_data[i * 3 + 2] = apply_parametric_curve(v, blue_type, &blue);
                }
                cached_file.lut = Some(lut);
            }
        } else {
            if green_trc.is_parametric_curve() || blue_trc.is_parametric_curve() {
                return Err(throw_error_message(same_type, file_name));
            }
            let curve = |r: &IccTypeReader| match r {
                IccTypeReader::Curve(c) => Some(c.clone()),
                _ => None,
            };
            let (Some(rc), Some(gc), Some(bc)) =
                (curve(&red_trc), curve(&green_trc), curve(&blue_trc))
            else {
                return Err(throw_error_message(same_type, file_name));
            };

            let curve_size = rc.len();
            if gc.len() != curve_size || bc.len() != curve_size {
                return Err(throw_error_message(
                    b"All curves in the ICC profile must be of the same length.",
                    file_name,
                ));
            }

            if 0 == curve_size {
                return Err(throw_error_message(
                    b"Curves with no values in ICC profile.",
                    file_name,
                ));
            } else if 1 == curve_size {
                // The curve value shall be interpreted as a gamma value.
                //
                // In this case, the 16-bit curve value is to be interpreted as an unsigned
                // fixed-point 8.8 number. (But we want to multiply by 65535 to undo the
                // normalization applied by SampleICC)
                cached_file.gamma_rgb[0] = rc[0] * 65535.0f32 / 256.0f32;
                cached_file.gamma_rgb[1] = gc[0] * 65535.0f32 / 256.0f32;
                cached_file.gamma_rgb[2] = bc[0] * 65535.0f32 / 256.0f32;
                cached_file.gamma_rgb[3] = 1.0;
            } else {
                // The LUT stored in the profile takes gamma-corrected values and linearizes
                // them. The entries are encoded as 16-bit ints that may be normalized by 65535
                // to interpret them as [0,1]. The LUT will be inverted to convert
                // output-linear values into values that may be sent to the display.
                let mut lut = Lut1DOpData::new(curve_size as std::ffi::c_ulong)?;
                let lut_data = lut.get_array_mut().get_values_mut();
                for i in 0..curve_size {
                    lut_data[i * 3] = rc[i];
                    lut_data[i * 3 + 1] = gc[i];
                    lut_data[i * 3 + 2] = bc[i];
                }

                // Set the file bit-depth based on what is in the ICC profile (even though
                // SampleICC has normalized the values).
                lut.set_file_output_bit_depth(BitDepth::Uint16);
                cached_file.lut = Some(lut);
            }
        }

        Ok(cached_file)
    }
}

/// The Bradford matrix that converts a D50 XYZ to a D65 XYZ.
///
/// Port of `D50_to_D65_m44` (FileFormatICC.cpp:779-784 @ v2.5.2).
const D50_TO_D65_M44: [f64; 16] = [
    0.955509474537,
    -0.023074829492,
    0.063312392987,
    0.0,
    -0.028327238868,
    1.00994465504,
    0.021055592145,
    0.0,
    0.012329273379,
    -0.020536209966,
    1.33072998567,
    0.0,
    0.0,
    0.0,
    0.0,
    1.0,
];

/// A gamma op of `style` with the file's gammas, `mGammaRGB` as `double`s.
fn gamma_data(cached_file: &LocalCachedFile, style: GammaStyle) -> GammaOpData {
    let g = |i: usize| vec![f64::from(cached_file.gamma_rgb[i])];
    GammaOpData::new(style, g(0), g(1), g(2), g(3))
}

impl FileFormat for LocalFileFormat {
    /// Port of `LocalFileFormat::getFormatInfo` (FileFormatICC.cpp:98-115 @ v2.5.2): the three
    /// infos share one `FormatInfo`, its name and extension changed.
    fn format_info(&self) -> Vec<FormatInfo> {
        vec![
            FormatInfo::new(
                "International Color Consortium profile",
                "icc",
                FormatCapabilities::READ,
                FormatBakeCapabilities::NONE,
            ),
            // .icm and .pf file extensions are also fine.
            FormatInfo::new(
                "Image Color Matching profile",
                "icm",
                FormatCapabilities::READ,
                FormatBakeCapabilities::NONE,
            ),
            FormatInfo::new(
                "ICC profile",
                "pf",
                FormatCapabilities::READ,
                FormatBakeCapabilities::NONE,
            ),
        ]
    }

    /// Port of `LocalFileFormat::isBinary` (FileFormatICC.cpp:80-83 @ v2.5.2).
    fn is_binary(&self) -> bool {
        true
    }

    /// Port of `LocalFileFormat::read` (FileFormatICC.cpp:537-743 @ v2.5.2).
    fn read(
        &self,
        istream: &mut InputStream,
        file_name: &[u8],
        _interp: Interpolation,
    ) -> Result<CachedFileRcPtr> {
        Ok(Arc::new(self.read_file(istream, file_name)?))
    }

    /// Appends the profile's ops: forward (CIE XYZ to monitor code values), the inverse of
    /// the Bradford D50 to D65 matrix, the inverse of the profile's matrix, then the inverse
    /// of its LUT or a reversed mirrored gamma; inverse, the LUT or a mirrored gamma, the
    /// matrix, the Bradford matrix.
    ///
    /// Port of `LocalFileFormat::buildFileOps` (FileFormatICC.cpp:745-881 @ v2.5.2).
    fn build_file_ops(
        &self,
        ops: &mut OpVec,
        _config: &Config,
        _context: &Context,
        untyped_cached_file: &CachedFileRcPtr,
        file_transform: &FileTransform,
        dir: TransformDirection,
    ) -> Result<()> {
        let any: &dyn Any = untyped_cached_file.as_ref();
        // This should never happen.
        let Some(cached_file) = any.downcast_ref::<LocalCachedFile>() else {
            return Err(Exception::new("Cannot build Op. Invalid cache type."));
        };

        let new_dir = combine_transform_directions(dir, file_transform.direction());

        let file_interp = file_transform.interpolation();

        let mut lut = None;
        if cached_file.lut.is_some() {
            let mut file_interp_used = false;
            lut = handle_lut1d(cached_file.lut.as_ref(), file_interp, &mut file_interp_used);

            if !file_interp_used {
                log_warning_interpolation_not_used(file_interp, file_transform);
            }
        }

        match new_dir {
            TransformDirection::Inverse => {
                // Monitor code value to CIE XYZ.
                match lut {
                    Some(lut) => create_lut1d_op(ops, lut, TransformDirection::Forward),
                    None => create_gamma_op(
                        ops,
                        gamma_data(cached_file, GammaStyle::BasicMirrorFwd),
                        TransformDirection::Forward,
                    ),
                }

                create_matrix_op_from_m44(ops, &cached_file.matrix44, TransformDirection::Forward);

                create_matrix_op_from_m44(ops, &D50_TO_D65_M44, TransformDirection::Forward);
            }
            TransformDirection::Forward => {
                // CIE XYZ to monitor code value.

                create_matrix_op_from_m44(ops, &D50_TO_D65_M44, TransformDirection::Inverse);

                // The ICC profile tags form a matrix that converts RGB to CIE XYZ. Invert
                // since we are building a PCS -> device transform.
                create_matrix_op_from_m44(ops, &cached_file.matrix44, TransformDirection::Inverse);

                // The LUT / gamma stored in the ICC profile works in the gamma->linear
                // direction.
                match lut {
                    Some(lut) => create_lut1d_op(ops, lut, TransformDirection::Inverse),
                    None => create_gamma_op(
                        ops,
                        gamma_data(cached_file, GammaStyle::BasicMirrorRev),
                        TransformDirection::Forward,
                    ),
                }
            }
        }
        Ok(())
    }
}

/// The description of the ICC profile at `icc_profile_filepath`, or the file's name when it
/// has none or an empty one.
///
/// Port of `GetProfileDescriptionFromICCProfile` (FileFormatICC.cpp:888-913 @ v2.5.2).
pub fn get_profile_description_from_icc_profile(icc_profile_filepath: &[u8]) -> Result<Vec<u8>> {
    let mut filestream = InputStream::open_file(icc_profile_filepath, OpenMode::Binary);
    if !filestream.good() {
        return Err(Exception::new(
            [
                b"The specified file '".as_slice(),
                icc_profile_filepath,
                b"' could not be opened. Please confirm the file exists with appropriate read \
                  permissions.",
            ]
            .concat(),
        ));
    }

    let mut icc = IccContent::default();
    let file = read_info(&mut filestream, icc_profile_filepath, &mut icc)?;

    let mut desc = file.profile_description;
    if desc.is_empty() {
        // Fallback to the filename if the description is missing or empty.
        let (_path, filename) = os_path::split(icc_profile_filepath);
        desc = filename;
    }

    Ok(desc)
}

#[cfg(test)]
#[path = "file_format_icc_tests.rs"]
mod tests;
