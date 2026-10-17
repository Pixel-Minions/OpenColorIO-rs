// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Pandora `.mga` and `.m3d` formats: a 3D LUT of integer code values, blue fastest. A
//! port of `src/OpenColorIO/fileformats/FileFormatPandora.cpp` @ v2.5.2.
//!
//! ```text
//! channel 3d
//! in 8
//! out 256
//! format lut
//! values red green blue
//! 0 0     0   0
//! 1 0     0 255
//! ...
//! ```

use std::any::Any;
use std::ffi::c_ulong;
use std::sync::Arc;

use ocio_ops::bit_depth_utils::get_bitdepth_from_max_value;
use ocio_ops::exception::{Exception, Result};
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{TransformDirection, combine_transform_directions};
use ocio_ops::ops::lut3d::lut3d_op::{create_lut3d_op, get_3d_lut_edge_len_from_num_pixels};
use ocio_ops::ops::lut3d::lut3d_op_data::{Interpolation, Lut3DOpData, MAX_3D_LUT_LENGTH};
use ocio_ops::parse_utils::{string_to_int, string_vec_to_int_vec};
use ocio_ops::utils::string_utils::{lower, split_by_white_spaces, starts_with_char, trim};

use crate::config::Config;
use crate::context::Context;
use crate::fileformats::file_format_utils::{handle_lut3d, log_warning_interpolation_not_used};
use crate::fileformats::input_stream::InputStream;
use crate::parse_utils::nextline;
use crate::transforms::file_format::{
    CachedFile, CachedFileRcPtr, FileFormat, FormatBakeCapabilities, FormatCapabilities, FormatInfo,
};
use crate::transforms::file_transform::FileTransform;

/// A Pandora file's 3D LUT.
///
/// Port of `LocalCachedFile` (FileFormatPandora.cpp:23-30 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct LocalCachedFile {
    /// `lut3D`.
    pub lut3d: Lut3DOpData,
}

impl CachedFile for LocalCachedFile {}

/// The Pandora formats.
///
/// Port of `LocalFileFormat` (FileFormatPandora.cpp:36-61 @ v2.5.2).
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalFileFormat;

/// `Error parsing Pandora LUT file (<file>).  [At line (<n>): '<line>'.  ]<error>`.
///
/// Port of `LocalFileFormat::ThrowErrorMessage` (FileFormatPandora.cpp:63-80 @ v2.5.2).
fn throw_error_message(
    error: &[u8],
    file_name: &[u8],
    line: i32,
    line_content: &[u8],
) -> Exception {
    let mut os = b"Error parsing Pandora LUT file (".to_vec();
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
    /// Reads a Pandora file: the tags (`channel 3d`, `in`, `out`, `format lut`, `values red
    /// green blue`), then a line of an index and 3 integers per entry.
    ///
    /// Port of `LocalFileFormat::read` (FileFormatPandora.cpp:97-291 @ v2.5.2).
    pub fn read_file(
        &self,
        istream: &mut InputStream,
        file_name: &[u8],
        interp: Interpolation,
    ) -> Result<LocalCachedFile> {
        // this shouldn't happen
        if istream.fail() {
            return Err(Exception::new(
                "File stream empty when trying to read Pandora LUT",
            ));
        }

        // Validate the file type
        let mut line: Vec<u8> = Vec::new();

        // Parse the file
        let mut lut_edge_len: i32 = 0;
        let mut output_bit_depth_max_value: i32 = 0;
        let mut raw3d: Vec<i32> = Vec::new();

        {
            let mut tmpints: Vec<i32> = Vec::new();
            let mut in_lut3d = false;
            let mut line_number: i32 = 0;

            while nextline(istream, &mut line) {
                line_number += 1;

                // Strip, lowercase, and split the line
                let parts = split_by_white_spaces(&lower(trim(&line)));
                if parts.is_empty() {
                    continue;
                }

                // Skip all lines starting with '#'
                if starts_with_char(&parts[0], b'#') {
                    continue;
                }

                if parts[0] == b"channel" {
                    if parts.len() != 2 || lower(&parts[1]) != b"3d" {
                        return Err(throw_error_message(
                            b"Only 3D LUTs are currently supported (channel: 3d).",
                            file_name,
                            line_number,
                            &line,
                        ));
                    }
                } else if parts[0] == b"in" {
                    let mut inval: i32 = 0;
                    if parts.len() != 2 || !string_to_int(&mut inval, &parts[1], false) {
                        return Err(throw_error_message(
                            b"Malformed 'in' tag.",
                            file_name,
                            line_number,
                            &line,
                        ));
                    }
                    let max = i64::from(MAX_3D_LUT_LENGTH);
                    if inval < 8 || i64::from(inval) > max * max * max {
                        return Err(throw_error_message(
                            format!(
                                "'in' value must be between 8 and {} ({MAX_3D_LUT_LENGTH}^3).",
                                max * max * max
                            )
                            .as_bytes(),
                            file_name,
                            line_number,
                            &line,
                        ));
                    }
                    lut_edge_len = get_3d_lut_edge_len_from_num_pixels(inval)?;
                } else if parts[0] == b"out" {
                    if parts.len() != 2
                        || !string_to_int(&mut output_bit_depth_max_value, &parts[1], false)
                    {
                        return Err(throw_error_message(
                            b"Malformed 'out' tag.",
                            file_name,
                            line_number,
                            &line,
                        ));
                    }
                } else if parts[0] == b"format" {
                    if parts.len() != 2 || lower(&parts[1]) != b"lut" {
                        return Err(throw_error_message(
                            b"Only LUTs are currently supported (format: lut).",
                            file_name,
                            line_number,
                            &line,
                        ));
                    }
                } else if parts[0] == b"values" {
                    if parts.len() != 4
                        || lower(&parts[1]) != b"red"
                        || lower(&parts[2]) != b"green"
                        || lower(&parts[3]) != b"blue"
                    {
                        return Err(throw_error_message(
                            b"Only rgb LUTs are currently supported (values: red green blue).",
                            file_name,
                            line_number,
                            &line,
                        ));
                    }

                    in_lut3d = true;
                } else if in_lut3d {
                    if !string_vec_to_int_vec(&mut tmpints, &parts) || tmpints.len() != 4 {
                        return Err(throw_error_message(
                            b"Expected to find 4 integers.",
                            file_name,
                            line_number,
                            &line,
                        ));
                    }

                    let max = MAX_3D_LUT_LENGTH as usize;
                    if raw3d.len() > max * max * max * 3 {
                        return Err(throw_error_message(
                            b"Too many 3D LUT entries.",
                            file_name,
                            line_number,
                            &line,
                        ));
                    }

                    raw3d.extend_from_slice(&tmpints[1..4]);
                }
            }
        }

        // Interpret the parsed data, validate LUT sizes
        if lut_edge_len * lut_edge_len * lut_edge_len != (raw3d.len() / 3) as i32 {
            return Err(throw_error_message(
                format!(
                    "Incorrect number of 3D LUT entries. Found {}, expected {}.",
                    raw3d.len() / 3,
                    lut_edge_len * lut_edge_len * lut_edge_len
                )
                .as_bytes(),
                file_name,
                -1,
                b"",
            ));
        }

        if lut_edge_len * lut_edge_len * lut_edge_len == 0 {
            return Err(throw_error_message(
                b"No 3D LUT entries found.",
                file_name,
                -1,
                b"",
            ));
        }

        if output_bit_depth_max_value <= 0 {
            return Err(throw_error_message(
                b"A valid 'out' tag was not found.",
                file_name,
                -1,
                b"",
            ));
        }

        // Copy raw3d vector into LutOpData object.
        let mut lut3d = Lut3DOpData::new(lut_edge_len as c_ulong)?;
        if Lut3DOpData::is_valid_interpolation(interp) {
            lut3d.set_interpolation(interp);
        }

        let file_bd = get_bitdepth_from_max_value(output_bit_depth_max_value as u32);
        lut3d.set_file_output_bit_depth(file_bd);

        let scale = 1.0f32 / (output_bit_depth_max_value as f32 - 1.0f32);

        // lutArray and LUT in file are blue fastest.
        let lut_array = lut3d.get_array_mut().get_values_mut();
        for (value, &raw) in lut_array.iter_mut().zip(&raw3d) {
            *value = raw as f32 * scale;
        }

        Ok(LocalCachedFile { lut3d })
    }
}

impl FileFormat for LocalFileFormat {
    /// Port of `LocalFileFormat::getFormatInfo` (FileFormatPandora.cpp:82-95 @ v2.5.2).
    fn format_info(&self) -> Vec<FormatInfo> {
        vec![
            FormatInfo::new(
                "pandora_mga",
                "mga",
                FormatCapabilities::READ,
                FormatBakeCapabilities::NONE,
            ),
            FormatInfo::new(
                "pandora_m3d",
                "m3d",
                FormatCapabilities::READ,
                FormatBakeCapabilities::NONE,
            ),
        ]
    }

    /// Port of `LocalFileFormat::read` (FileFormatPandora.cpp:97-291 @ v2.5.2).
    fn read(
        &self,
        istream: &mut InputStream,
        file_name: &[u8],
        interp: Interpolation,
    ) -> Result<CachedFileRcPtr> {
        Ok(Arc::new(self.read_file(istream, file_name, interp)?))
    }

    /// Appends the file's 3D LUT, with the file transform's interpolation where valid for it.
    ///
    /// Port of `LocalFileFormat::buildFileOps` (FileFormatPandora.cpp:293-324 @ v2.5.2).
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
            return Err(Exception::new(
                "Cannot build Pandora LUT. Invalid cache type.",
            ));
        };

        let new_dir = combine_transform_directions(dir, file_transform.direction());

        let file_interp = file_transform.interpolation();

        let mut file_interp_used = false;
        let lut3d = handle_lut3d(Some(&cached_file.lut3d), file_interp, &mut file_interp_used)
            .expect("the file's LUT");

        if !file_interp_used {
            log_warning_interpolation_not_used(file_interp, file_transform);
        }

        create_lut3d_op(ops, lut3d, new_dir);
        Ok(())
    }
}

#[cfg(test)]
#[path = "file_format_pandora_tests.rs"]
mod tests;
