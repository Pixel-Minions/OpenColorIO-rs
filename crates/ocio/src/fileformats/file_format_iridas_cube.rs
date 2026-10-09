// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Iridas `.cube` format: a 1D or a 3D LUT (red fastest), with an optional domain. A port
//! of `src/OpenColorIO/fileformats/FileFormatIridasCube.cpp` @ v2.5.2; its `bake` comes with
//! the baker (WP 4.8).
//!
//! ```text
//! # comments start with '#'
//! TITLE "title"
//! LUT_3D_SIZE 2
//! DOMAIN_MIN 0.0 0.0 0.0
//! DOMAIN_MAX 1.0 1.0 1.0
//! 0.0 0.0 0.0
//! 1.0 0.0 0.0
//! ...
//! ```
//!
//! The tags are read in any case; `LUT_1D_SIZE` or `LUT_3D_SIZE` (not both) gives the LUT's
//! size, and the first line that is no tag starts the entries, three values per line.

use std::any::Any;
use std::ffi::c_ulong;
use std::sync::Arc;

use ocio_ops::exception::{Exception, Result};
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{BitDepth, TransformDirection, combine_transform_directions};
use ocio_ops::ops::lut1d::Lut1DOpData;
use ocio_ops::ops::lut1d::lut1d_op::create_lut1d_op;
use ocio_ops::ops::lut3d::lut3d_op::create_lut3d_op;
use ocio_ops::ops::lut3d::lut3d_op_data::{Interpolation, Lut3DOpData, MAX_3D_LUT_LENGTH};
use ocio_ops::ops::matrix::matrix_op::create_min_max_op;
use ocio_ops::utils::cscan::{ScanArg, ScanCrt, sscanf};
use ocio_ops::utils::number_utils::{Errc, Flavor, from_chars_f32};
use ocio_ops::utils::string_utils::{left_trim, lower, starts_with, starts_with_char, trim};

use crate::config::Config;
use crate::context::Context;
use crate::fileformats::file_format_utils::{
    handle_lut1d, handle_lut3d, log_warning_interpolation_not_used,
};
use crate::fileformats::input_stream::InputStream;
use crate::lut_limits::MAX_1D_LUT_LENGTH;
use crate::parse_utils::nextline;
use crate::transforms::file_format::{
    CachedFile, CachedFileRcPtr, FileFormat, FormatBakeCapabilities, FormatCapabilities, FormatInfo,
};
use crate::transforms::file_transform::FileTransform;

/// An Iridas `.cube` file's LUT (1D or 3D) and domain.
///
/// Port of `LocalCachedFile` (FileFormatIridasCube.cpp:74-84 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct LocalCachedFile {
    /// `lut1D`.
    pub lut1d: Option<Lut1DOpData>,
    /// `lut3D`.
    pub lut3d: Option<Lut3DOpData>,
    /// `domain_min`.
    pub domain_min: [f32; 3],
    /// `domain_max`.
    pub domain_max: [f32; 3],
}

impl Default for LocalCachedFile {
    fn default() -> Self {
        LocalCachedFile {
            lut1d: None,
            lut3d: None,
            domain_min: [0.0, 0.0, 0.0],
            domain_max: [1.0, 1.0, 1.0],
        }
    }
}

impl CachedFile for LocalCachedFile {}

/// The Iridas `.cube` format.
///
/// Port of `LocalFileFormat` (FileFormatIridasCube.cpp:90-119 @ v2.5.2).
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalFileFormat;

/// `Error parsing Iridas .cube file (<file>).  [At line (<n>): '<line>'.  ]<error>`.
///
/// Port of `LocalFileFormat::ThrowErrorMessage` (FileFormatIridasCube.cpp:121-138 @ v2.5.2).
fn throw_error_message(
    error: &[u8],
    file_name: &[u8],
    line: i32,
    line_content: &[u8],
) -> Exception {
    let mut os = b"Error parsing Iridas .cube file (".to_vec();
    os.extend_from_slice(file_name);
    os.extend_from_slice(b").  ");
    if -1 != line {
        os.extend_from_slice(format!("At line ({line}): '").as_bytes());
        os.extend_from_slice(line_content);
        os.extend_from_slice(b"'.  ");
    }
    os.extend_from_slice(error);
    Exception::new(os)
}

/// `NumberUtils::from_chars(s, s + 64, value)` on a 64-byte buffer: whether it parsed.
fn parse_float(s: &[u8; 64], value: &mut f32) -> bool {
    from_chars_f32(Flavor::NATIVE, s, 64, value).ec == Errc::Ok
}

/// Scans `line` for `<tag> %63s %63s %63s %c`: the three values, when exactly three scan.
fn scan_triple(line: &[u8], format: &[u8]) -> Option<[[u8; 64]; 3]> {
    let mut r = [0u8; 64];
    let mut g = [0u8; 64];
    let mut b = [0u8; 64];
    let mut end_tok = 0u8;
    let scanned = sscanf(
        ScanCrt::NATIVE,
        line,
        format,
        &mut [
            ScanArg::Str(&mut r),
            ScanArg::Str(&mut g),
            ScanArg::Str(&mut b),
            ScanArg::Char(&mut end_tok),
        ],
    );
    (scanned == 3).then_some([r, g, b])
}

/// Scans `line` for `<tag> %d %c`: the size, when exactly one value scans.
fn scan_size(line: &[u8], format: &[u8], size: &mut i32) -> bool {
    let mut end_tok = 0u8;
    sscanf(
        ScanCrt::NATIVE,
        line,
        format,
        &mut [ScanArg::Int(size), ScanArg::Char(&mut end_tok)],
    ) == 1
}

impl LocalFileFormat {
    /// Reads a `.cube` file: the tags (comments and blank lines skipped) up to the first line
    /// that is none, then the entries, three values a line, to the end of the file.
    ///
    /// Port of `LocalFileFormat::read` (FileFormatIridasCube.cpp:150-460 @ v2.5.2).
    pub fn read_file(
        &self,
        istream: &mut InputStream,
        file_name: &[u8],
        interp: Interpolation,
    ) -> Result<LocalCachedFile> {
        // this shouldn't happen
        if istream.fail() {
            return Err(Exception::new(
                "File stream empty when trying to read Iridas .cube LUT",
            ));
        }

        // Parse the file
        let mut raw: Vec<f32> = Vec::new();

        let mut size3d: i32 = 0;
        let mut size1d: i32 = 0;

        let mut in1d = false;
        let mut in3d = false;

        let mut domain_min = [0.0f32, 0.0, 0.0];
        let mut domain_max = [1.0f32, 1.0, 1.0];

        {
            let mut line: Vec<u8> = Vec::new();
            let mut line_number: i32 = 0;
            let mut entries_started = false;

            while !entries_started && nextline(istream, &mut line) {
                line_number += 1;
                // All lines starting with '#' are comments
                if starts_with_char(&line, b'#') {
                    continue;
                }

                line = lower(trim(&line));

                if line.is_empty() {
                    continue;
                }

                if starts_with(&line, b"title") {
                    // Optional, and currently unhandled
                } else if starts_with(&line, b"lut_1d_size") {
                    if !scan_size(&line, b"lut_1d_size %d %c", &mut size1d) {
                        return Err(throw_error_message(
                            b"Malformed 'LUT_1D_SIZE' tag.",
                            file_name,
                            line_number,
                            &line,
                        ));
                    }

                    if size1d < 2 || i64::from(size1d) > MAX_1D_LUT_LENGTH as i64 {
                        return Err(throw_error_message(
                            format!("'LUT_1D_SIZE' must be between 2 and {MAX_1D_LUT_LENGTH}.")
                                .as_bytes(),
                            file_name,
                            line_number,
                            &line,
                        ));
                    }

                    in1d = true;
                } else if starts_with(&line, b"lut_2d_size") {
                    return Err(throw_error_message(
                        b"Unsupported tag: 'LUT_2D_SIZE'.",
                        file_name,
                        line_number,
                        &line,
                    ));
                } else if starts_with(&line, b"lut_3d_size") {
                    if !scan_size(&line, b"lut_3d_size %d %c", &mut size3d) {
                        return Err(throw_error_message(
                            b"Malformed 'LUT_3D_SIZE' tag.",
                            file_name,
                            line_number,
                            &line,
                        ));
                    }

                    if size3d < 2 || i64::from(size3d) > i64::from(MAX_3D_LUT_LENGTH) {
                        return Err(throw_error_message(
                            format!("'LUT_3D_SIZE' must be between 2 and {MAX_3D_LUT_LENGTH}.")
                                .as_bytes(),
                            file_name,
                            line_number,
                            &line,
                        ));
                    }

                    in3d = true;
                } else if starts_with(&line, b"domain_min") {
                    let Some([r, g, b]) = scan_triple(&line, b"domain_min %63s %63s %63s %c")
                    else {
                        return Err(throw_error_message(
                            b"Malformed 'DOMAIN_MIN' tag.",
                            file_name,
                            line_number,
                            &line,
                        ));
                    };
                    let r_ok = parse_float(&r, &mut domain_min[0]);
                    let g_ok = parse_float(&g, &mut domain_min[1]);
                    let b_ok = parse_float(&b, &mut domain_min[2]);
                    if !r_ok || !g_ok || !b_ok {
                        return Err(throw_error_message(
                            b"Invalid 'DOMAIN_MIN' Tag",
                            file_name,
                            line_number,
                            &line,
                        ));
                    }
                } else if starts_with(&line, b"domain_max") {
                    let Some([r, g, b]) = scan_triple(&line, b"domain_max %63s %63s %63s %c")
                    else {
                        return Err(throw_error_message(
                            b"Malformed 'DOMAIN_MAX' tag.",
                            file_name,
                            line_number,
                            &line,
                        ));
                    };
                    let r_ok = parse_float(&r, &mut domain_max[0]);
                    let g_ok = parse_float(&g, &mut domain_max[1]);
                    let b_ok = parse_float(&b, &mut domain_max[2]);
                    if !r_ok || !g_ok || !b_ok {
                        return Err(throw_error_message(
                            b"Invalid 'DOMAIN_MAX' Tag",
                            file_name,
                            line_number,
                            &line,
                        ));
                    }
                } else {
                    entries_started = true;
                }
            }

            // `do { ... } while (nextline(istream, line));`, whose `continue` goes to the
            // condition.
            loop {
                line = left_trim(&line).to_vec();

                // All lines starting with '#' are comments
                let skip = starts_with_char(&line, b'#') || line.is_empty();

                if !skip {
                    let Some([r_s, g_s, b_s]) = scan_triple(&line, b"%63s %63s %63s %c") else {
                        // It must be a float triple!
                        return Err(throw_error_message(
                            b"Malformed color triples specified.",
                            file_name,
                            line_number,
                            &line,
                        ));
                    };
                    let (mut r, mut g, mut b) = (f32::NAN, f32::NAN, f32::NAN);

                    let r_ok = parse_float(&r_s, &mut r);
                    let g_ok = parse_float(&g_s, &mut g);
                    let b_ok = parse_float(&b_s, &mut b);

                    if !r_ok || !g_ok || !b_ok {
                        return Err(throw_error_message(
                            b"Invalid color triples",
                            file_name,
                            line_number,
                            &line,
                        ));
                    }

                    raw.push(r);
                    raw.push(g);
                    raw.push(b);

                    line_number += 1;
                }

                if !nextline(istream, &mut line) {
                    break;
                }
            }
        }

        // Interpret the parsed data, validate LUT sizes.

        let mut cached_file = LocalCachedFile::default();

        if in1d {
            if size1d != (raw.len() / 3) as i32 {
                return Err(throw_error_message(
                    format!(
                        "Incorrect number of lut1d entries. Found {}, expected {size1d}.",
                        raw.len() / 3
                    )
                    .as_bytes(),
                    file_name,
                    -1,
                    b"",
                ));
            }

            // Reformat 1D data
            if size1d > 0 {
                cached_file.domain_min = domain_min;
                cached_file.domain_max = domain_max;

                let lut_lenght = size1d as c_ulong;
                let mut lut1d = Lut1DOpData::new(lut_lenght)?;
                if Lut1DOpData::is_valid_interpolation(interp) {
                    lut1d.set_interpolation(interp);
                }

                lut1d.set_file_output_bit_depth(BitDepth::F32);

                let lut_array = lut1d.get_array_mut().get_values_mut();

                let num_vals = lut_array.len();
                lut_array.copy_from_slice(&raw[..num_vals]);
                cached_file.lut1d = Some(lut1d);
            }
        } else if in3d {
            if size3d * size3d * size3d != (raw.len() / 3) as i32 {
                return Err(throw_error_message(
                    format!(
                        "Incorrect number of 3D LUT entries. Found {}, expected {}.",
                        raw.len() / 3,
                        size3d * size3d * size3d
                    )
                    .as_bytes(),
                    file_name,
                    -1,
                    b"",
                ));
            }

            // Reformat 3D data
            cached_file.domain_min = domain_min;
            cached_file.domain_max = domain_max;
            let mut lut3d = Lut3DOpData::new(size3d as c_ulong)?;
            if Lut3DOpData::is_valid_interpolation(interp) {
                lut3d.set_interpolation(interp);
            }
            lut3d.set_file_output_bit_depth(BitDepth::F32);
            lut3d.set_array_from_red_fastest_order(&raw)?;
            cached_file.lut3d = Some(lut3d);
        } else {
            return Err(throw_error_message(
                b"LUT type (1D/3D) unspecified.",
                file_name,
                -1,
                b"",
            ));
        }

        Ok(cached_file)
    }
}

impl FileFormat for LocalFileFormat {
    /// Port of `LocalFileFormat::getFormatInfo` (FileFormatIridasCube.cpp:140-148 @ v2.5.2).
    fn format_info(&self) -> Vec<FormatInfo> {
        vec![FormatInfo::new(
            "iridas_cube",
            "cube",
            FormatCapabilities::READ | FormatCapabilities::BAKE,
            FormatBakeCapabilities::LUT_3D,
        )]
    }

    /// Port of `LocalFileFormat::read` (FileFormatIridasCube.cpp:150-460 @ v2.5.2).
    fn read(
        &self,
        istream: &mut InputStream,
        file_name: &[u8],
        interp: Interpolation,
    ) -> Result<CachedFileRcPtr> {
        Ok(Arc::new(self.read_file(istream, file_name, interp)?))
    }

    /// Appends, forward, the domain's min/max op then the LUT (the 1D one first); inverse, the
    /// LUT (the 3D one first) then the inverse min/max op.
    ///
    /// Port of `LocalFileFormat::buildFileOps` (FileFormatIridasCube.cpp:515-578 @ v2.5.2).
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
        let cached_file = match any.downcast_ref::<LocalCachedFile>() {
            Some(file) if file.lut1d.is_some() || file.lut3d.is_some() => file,
            _ => {
                return Err(Exception::new(
                    "Cannot build Iridas .cube Op. Invalid cache type.",
                ));
            }
        };

        let new_dir = combine_transform_directions(dir, file_transform.direction());

        let file_interp = file_transform.interpolation();

        let mut file_interp_used = false;
        let lut1d = handle_lut1d(
            cached_file.lut1d.as_ref(),
            file_interp,
            &mut file_interp_used,
        );
        let lut3d = handle_lut3d(
            cached_file.lut3d.as_ref(),
            file_interp,
            &mut file_interp_used,
        );

        if !file_interp_used {
            log_warning_interpolation_not_used(file_interp, file_transform);
        }

        let dmin = cached_file.domain_min.map(f64::from);
        let dmax = cached_file.domain_max.map(f64::from);

        match new_dir {
            TransformDirection::Forward => {
                create_min_max_op(ops, &dmin, &dmax, new_dir)?;
                if let Some(lut1d) = lut1d {
                    create_lut1d_op(ops, lut1d, new_dir);
                } else if let Some(lut3d) = lut3d {
                    create_lut3d_op(ops, lut3d, new_dir);
                }
            }
            TransformDirection::Inverse => {
                if let Some(lut3d) = lut3d {
                    create_lut3d_op(ops, lut3d, new_dir);
                } else if let Some(lut1d) = lut1d {
                    create_lut1d_op(ops, lut1d, new_dir);
                }
                create_min_max_op(ops, &dmin, &dmax, new_dir)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "file_format_iridas_cube_tests.rs"]
mod tests;
