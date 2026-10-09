// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Resolve `.cube` format: a 1D LUT, a 3D LUT (red fastest), or a 1D shaper LUT then a 3D
//! LUT, each with an optional input range. A port of
//! `src/OpenColorIO/fileformats/FileFormatResolveCube.cpp` @ v2.5.2; its `bake` comes with the
//! baker (WP 4.8).
//!
//! ```text
//! # Comments, before the data only.
//! LUT_1D_SIZE 6
//! LUT_1D_INPUT_RANGE 0.0 1.0
//! LUT_3D_SIZE 3
//! LUT_3D_INPUT_RANGE 0.0 1.0
//! 1.0 1.0 1.0
//! ...
//! ```
//!
//! The 1D LUT's lines come first, then the 3D LUT's.

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
use ocio_ops::ops::matrix::matrix_op::create_min_max_op_f32;
use ocio_ops::parse_utils::{string_to_float, string_to_int, string_vec_to_float_vec};
use ocio_ops::utils::string_utils::{lower, split_by_white_spaces, starts_with_char, trim};

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

/// A Resolve `.cube` file's LUTs and their input ranges.
///
/// Port of `LocalCachedFile` (FileFormatResolveCube.cpp:179-192 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct LocalCachedFile {
    /// `lut1D`.
    pub lut1d: Option<Lut1DOpData>,
    /// `range1d_min`.
    pub range1d_min: f32,
    /// `range1d_max`.
    pub range1d_max: f32,
    /// `lut3D`.
    pub lut3d: Option<Lut3DOpData>,
    /// `range3d_min`.
    pub range3d_min: f32,
    /// `range3d_max`.
    pub range3d_max: f32,
}

impl Default for LocalCachedFile {
    fn default() -> Self {
        LocalCachedFile {
            lut1d: None,
            range1d_min: 0.0,
            range1d_max: 1.0,
            lut3d: None,
            range3d_min: 0.0,
            range3d_max: 1.0,
        }
    }
}

impl CachedFile for LocalCachedFile {}

/// The Resolve `.cube` format.
///
/// Port of `LocalFileFormat` (FileFormatResolveCube.cpp:198-226 @ v2.5.2).
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalFileFormat;

/// `Error parsing Resolve .cube file (<file>).  [At line (<n>): '<line>'.  ]<error>`.
///
/// Port of `LocalFileFormat::ThrowErrorMessage` (FileFormatResolveCube.cpp:228-245 @ v2.5.2).
fn throw_error_message(
    error: &[u8],
    file_name: &[u8],
    line: i32,
    line_content: &[u8],
) -> Exception {
    let mut os = b"Error parsing Resolve .cube file (".to_vec();
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

impl LocalFileFormat {
    /// Reads a Resolve `.cube` file: the tags and the comments, then the entries, three values
    /// a line, the 1D LUT's first; a comment after the first entry, a `TITLE` or a
    /// `LUT_2D_SIZE` is an error.
    ///
    /// Port of `LocalFileFormat::read` (FileFormatResolveCube.cpp:259-521 @ v2.5.2).
    pub fn read_file(
        &self,
        istream: &mut InputStream,
        file_name: &[u8],
        interp: Interpolation,
    ) -> Result<LocalCachedFile> {
        // this shouldn't happen
        if istream.fail() {
            return Err(Exception::new(
                "File stream empty when trying to read Resolve .cube lut",
            ));
        }

        // Parse the file
        let mut raw1d: Vec<f32> = Vec::new();
        let mut raw3d: Vec<f32> = Vec::new();

        let mut size3d: i32 = 0;
        let mut size1d: i32 = 0;

        let mut has1d = false;
        let mut has3d = false;

        let mut range1d_min = 0.0f32;
        let mut range1d_max = 1.0f32;

        let mut range3d_min = 0.0f32;
        let mut range3d_max = 1.0f32;

        {
            let mut line: Vec<u8> = Vec::new();
            let mut tmpfloats: Vec<f32> = Vec::new();
            let mut line_number: i32 = 0;
            let mut header_complete = false;
            let mut triplet_number: i32 = 0;

            while nextline(istream, &mut line) {
                line_number += 1;

                // All lines starting with '#' are comments
                if starts_with_char(&line, b'#') {
                    if header_complete {
                        return Err(throw_error_message(
                            b"Comments not allowed after header.",
                            file_name,
                            line_number,
                            &line,
                        ));
                    } else {
                        continue;
                    }
                }

                // Strip, lowercase, and split the line
                let parts = split_by_white_spaces(&lower(trim(&line)));
                if parts.is_empty() {
                    continue;
                }

                let tag = lower(&parts[0]);
                if tag == b"title" {
                    return Err(throw_error_message(
                        b"Unsupported tag: 'TITLE'.",
                        file_name,
                        line_number,
                        &line,
                    ));
                } else if tag == b"lut_1d_size" {
                    if parts.len() != 2 || !string_to_int(&mut size1d, &parts[1], false) {
                        return Err(throw_error_message(
                            b"Malformed LUT_1D_SIZE tag.",
                            file_name,
                            line_number,
                            &line,
                        ));
                    }

                    if size1d < 2 || i64::from(size1d) > MAX_1D_LUT_LENGTH as i64 {
                        return Err(throw_error_message(
                            format!("LUT_1D_SIZE must be between 2 and {MAX_1D_LUT_LENGTH}.")
                                .as_bytes(),
                            file_name,
                            line_number,
                            &line,
                        ));
                    }
                    has1d = true;
                } else if tag == b"lut_2d_size" {
                    return Err(throw_error_message(
                        b"Unsupported tag: 'LUT_2D_SIZE'.",
                        file_name,
                        line_number,
                        &line,
                    ));
                } else if tag == b"lut_3d_size" {
                    if parts.len() != 2 || !string_to_int(&mut size3d, &parts[1], false) {
                        return Err(throw_error_message(
                            b"Malformed LUT_3D_SIZE tag.",
                            file_name,
                            line_number,
                            &line,
                        ));
                    }

                    if size3d < 2 || i64::from(size3d) > i64::from(MAX_3D_LUT_LENGTH) {
                        return Err(throw_error_message(
                            format!("LUT_3D_SIZE must be between 2 and {MAX_3D_LUT_LENGTH}.")
                                .as_bytes(),
                            file_name,
                            line_number,
                            &line,
                        ));
                    }
                    has3d = true;
                } else if tag == b"lut_1d_input_range" {
                    if parts.len() != 3 || !read_range(&parts, &mut range1d_min, &mut range1d_max) {
                        return Err(throw_error_message(
                            b"Malformed LUT_1D_INPUT_RANGE tag.",
                            file_name,
                            line_number,
                            &line,
                        ));
                    }
                } else if tag == b"lut_3d_input_range" {
                    if parts.len() != 3 || !read_range(&parts, &mut range3d_min, &mut range3d_max) {
                        return Err(throw_error_message(
                            b"Malformed LUT_3D_INPUT_RANGE tag.",
                            file_name,
                            line_number,
                            &line,
                        ));
                    }
                } else {
                    header_complete = true;

                    // It must be a float triple!
                    if !string_vec_to_float_vec(&mut tmpfloats, &parts) || tmpfloats.len() != 3 {
                        return Err(throw_error_message(
                            b"Malformed color triples specified.",
                            file_name,
                            line_number,
                            &line,
                        ));
                    }

                    for &value in &tmpfloats[..3] {
                        if has1d && triplet_number < size1d {
                            raw1d.push(value);
                        } else {
                            let max = MAX_3D_LUT_LENGTH as usize;
                            if raw3d.len() > max * max * max * 3 {
                                return Err(throw_error_message(
                                    b"Too many 3D LUT entries.",
                                    file_name,
                                    line_number,
                                    &line,
                                ));
                            }
                            raw3d.push(value);
                        }
                    }

                    triplet_number += 1;
                }
            }
        }

        // Interpret the parsed data, validate lut sizes

        let mut cached_file = LocalCachedFile::default();

        if has1d {
            if size1d != (raw1d.len() / 3) as i32 {
                return Err(throw_error_message(
                    format!(
                        "Incorrect number of lut1d entries. Found {}, expected {size1d}.",
                        raw1d.len() / 3
                    )
                    .as_bytes(),
                    file_name,
                    -1,
                    b"",
                ));
            }

            // Reformat 1D data
            if size1d > 0 {
                let mut lut1d = Lut1DOpData::new(size1d as c_ulong)?;
                if Lut1DOpData::is_valid_interpolation(interp) {
                    lut1d.set_interpolation(interp);
                }

                lut1d.set_file_output_bit_depth(BitDepth::F32);

                cached_file.range1d_min = range1d_min;
                cached_file.range1d_max = range1d_max;

                let lut_array = lut1d.get_array_mut().get_values_mut();
                lut_array[..raw1d.len()].copy_from_slice(&raw1d);
                cached_file.lut1d = Some(lut1d);
            }
        }
        if has3d {
            if size3d * size3d * size3d != (raw3d.len() / 3) as i32 {
                return Err(throw_error_message(
                    format!(
                        "Incorrect number of lut3d entries. Found {}, expected {}.",
                        raw3d.len() / 3,
                        size3d * size3d * size3d
                    )
                    .as_bytes(),
                    file_name,
                    -1,
                    b"",
                ));
            }

            // Reformat 3D data
            cached_file.range3d_min = range3d_min;
            cached_file.range3d_max = range3d_max;

            let mut lut3d = Lut3DOpData::new(size3d as c_ulong)?;
            if Lut3DOpData::is_valid_interpolation(interp) {
                lut3d.set_interpolation(interp);
            }
            lut3d.set_file_output_bit_depth(BitDepth::F32);
            lut3d.set_array_from_red_fastest_order(&raw3d)?;
            cached_file.lut3d = Some(lut3d);
        }
        if !has1d && !has3d {
            return Err(throw_error_message(
                b"Lut type (1D/3D) unspecified.",
                file_name,
                -1,
                b"",
            ));
        }

        Ok(cached_file)
    }
}

/// `StringToFloat(&min, parts[1]) && StringToFloat(&max, parts[2])`: each written where it
/// reads, the second read only after the first.
fn read_range(parts: &[Vec<u8>], min: &mut f32, max: &mut f32) -> bool {
    let Some(v) = string_to_float(&parts[1]) else {
        return false;
    };
    *min = v;
    let Some(v) = string_to_float(&parts[2]) else {
        return false;
    };
    *max = v;
    true
}

impl FileFormat for LocalFileFormat {
    /// Port of `LocalFileFormat::getFormatInfo` (FileFormatResolveCube.cpp:247-257 @ v2.5.2).
    fn format_info(&self) -> Vec<FormatInfo> {
        vec![FormatInfo::new(
            "resolve_cube",
            "cube",
            FormatCapabilities::READ | FormatCapabilities::BAKE,
            FormatBakeCapabilities::LUT_3D
                | FormatBakeCapabilities::LUT_1D
                | FormatBakeCapabilities::LUT_1D_3D,
        )]
    }

    /// Port of `LocalFileFormat::read` (FileFormatResolveCube.cpp:259-521 @ v2.5.2).
    fn read(
        &self,
        istream: &mut InputStream,
        file_name: &[u8],
        interp: Interpolation,
    ) -> Result<CachedFileRcPtr> {
        Ok(Arc::new(self.read_file(istream, file_name, interp)?))
    }

    /// Appends, forward, each LUT after the min/max op of its range, the 1D one first; inverse,
    /// each LUT then its range's inverse min/max op, the 3D one first.
    ///
    /// Port of `LocalFileFormat::buildFileOps` (FileFormatResolveCube.cpp:744-806 @ v2.5.2).
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
                    "Cannot build Resolve .cube Op. Invalid cache type.",
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

        match new_dir {
            TransformDirection::Forward => {
                if let Some(lut1d) = lut1d {
                    create_min_max_op_f32(
                        ops,
                        cached_file.range1d_min,
                        cached_file.range1d_max,
                        new_dir,
                    )?;
                    create_lut1d_op(ops, lut1d, new_dir);
                }
                if let Some(lut3d) = lut3d {
                    create_min_max_op_f32(
                        ops,
                        cached_file.range3d_min,
                        cached_file.range3d_max,
                        new_dir,
                    )?;
                    create_lut3d_op(ops, lut3d, new_dir);
                }
            }
            TransformDirection::Inverse => {
                if let Some(lut3d) = lut3d {
                    create_lut3d_op(ops, lut3d, new_dir);
                    create_min_max_op_f32(
                        ops,
                        cached_file.range3d_min,
                        cached_file.range3d_max,
                        new_dir,
                    )?;
                }
                if let Some(lut1d) = lut1d {
                    create_lut1d_op(ops, lut1d, new_dir);
                    create_min_max_op_f32(
                        ops,
                        cached_file.range1d_min,
                        cached_file.range1d_max,
                        new_dir,
                    )?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "file_format_resolve_cube_tests.rs"]
mod tests;
