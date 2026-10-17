// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Nuke `.vf` format: a 3D LUT, blue fastest, and an optional global matrix. A port of
//! `src/OpenColorIO/fileformats/FileFormatVF.cpp` @ v2.5.2.
//!
//! ```text
//! #Inventor V2.1 ascii
//! grid_size 2 2 2
//! global_transform 1 0 0 0  0 1 0 0  0 0 1 0  0 0 0 1
//! data
//! 0 0 0
//! 0 0 2
//! ...
//! ```

use std::any::Any;
use std::ffi::c_ulong;
use std::sync::Arc;

use ocio_ops::exception::{Exception, Result};
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{BitDepth, TransformDirection, combine_transform_directions};
use ocio_ops::ops::lut3d::lut3d_op::create_lut3d_op;
use ocio_ops::ops::lut3d::lut3d_op_data::{Interpolation, Lut3DOpData, MAX_3D_LUT_LENGTH};
use ocio_ops::ops::matrix::matrix_op::create_matrix_op_from_m44;
use ocio_ops::parse_utils::{string_to_int, string_vec_to_float_vec};
use ocio_ops::utils::string_utils::{
    lower, split_by_white_spaces, starts_with, starts_with_char, trim,
};

use crate::config::Config;
use crate::context::Context;
use crate::fileformats::file_format_utils::{handle_lut3d, log_warning_interpolation_not_used};
use crate::fileformats::input_stream::InputStream;
use crate::parse_utils::nextline;
use crate::transforms::file_format::{
    CachedFile, CachedFileRcPtr, FileFormat, FormatBakeCapabilities, FormatCapabilities, FormatInfo,
};
use crate::transforms::file_transform::FileTransform;

/// A `.vf` file's 3D LUT and global matrix.
///
/// Port of `LocalCachedFile` (FileFormatVF.cpp:23-32 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct LocalCachedFile {
    /// `lut3D`.
    pub lut3d: Option<Lut3DOpData>,
    /// `m44`.
    pub m44: [f64; 16],
    /// `useMatrix`.
    pub use_matrix: bool,
}

impl CachedFile for LocalCachedFile {}

/// The Nuke `.vf` format.
///
/// Port of `LocalFileFormat` (FileFormatVF.cpp:38-62 @ v2.5.2).
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalFileFormat;

/// `Error parsing Nuke .vf file (<file>).  [At line (<n>): '<line>'.  ]<error>`.
///
/// Port of `LocalFileFormat::ThrowErrorMessage` (FileFormatVF.cpp:64-81 @ v2.5.2).
fn throw_error_message(
    error: &[u8],
    file_name: &[u8],
    line: i32,
    line_content: &[u8],
) -> Exception {
    let mut os = b"Error parsing Nuke .vf file (".to_vec();
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
    /// Reads a `.vf` file: `#Inventor` on its first line, the tags (`grid_size`,
    /// `global_transform`) up to `data`, then the lines of 3 values after it.
    ///
    /// Port of `LocalFileFormat::read` (FileFormatVF.cpp:92-265 @ v2.5.2).
    pub fn read_file(
        &self,
        istream: &mut InputStream,
        file_name: &[u8],
        interp: Interpolation,
    ) -> Result<LocalCachedFile> {
        // this shouldn't happen
        if istream.fail() {
            return Err(Exception::new(
                "File stream empty when trying to read .vf LUT",
            ));
        }

        // Validate the file type
        let mut line: Vec<u8> = Vec::new();
        let mut line_number: i32 = 1;
        if !nextline(istream, &mut line) || !starts_with(&lower(&line), b"#inventor") {
            return Err(throw_error_message(
                b"Expecting '#Inventor V2.1 ascii'.",
                file_name,
                line_number,
                &line,
            ));
        }

        // Parse the file
        let mut raw3d: Vec<f32> = Vec::new();
        let mut size3d: [i32; 3] = [0, 0, 0];
        let mut global_transform: Vec<f32> = Vec::new();
        {
            let mut tmpfloats: Vec<f32> = Vec::new();
            let mut in3d = false;

            while nextline(istream, &mut line) {
                line_number += 1;

                // Strip, lowercase, and split the line
                let mut parts = split_by_white_spaces(&lower(trim(&line)));

                if parts.is_empty() {
                    continue;
                }

                if starts_with_char(&parts[0], b'#') {
                    continue;
                }

                if !in3d {
                    if parts[0] == b"grid_size" {
                        if parts.len() != 4
                            || !string_to_int(&mut size3d[0], &parts[1], false)
                            || !string_to_int(&mut size3d[1], &parts[2], false)
                            || !string_to_int(&mut size3d[2], &parts[3], false)
                        {
                            return Err(throw_error_message(
                                b"Malformed grid_size tag.",
                                file_name,
                                line_number,
                                &line,
                            ));
                        }

                        // TODO: Support nununiformly sized LUTs.
                        if size3d[0] != size3d[1] || size3d[0] != size3d[2] {
                            return Err(throw_error_message(
                                format!(
                                    "Only equal grid size LUTs are supported. Found grid size: \
                                     {} x {} x {}.",
                                    size3d[0], size3d[1], size3d[2]
                                )
                                .as_bytes(),
                                file_name,
                                line_number,
                                &line,
                            ));
                        }

                        if size3d[0] < 2 || i64::from(size3d[0]) > i64::from(MAX_3D_LUT_LENGTH) {
                            return Err(throw_error_message(
                                format!("Grid size must be between 2 and {MAX_3D_LUT_LENGTH}.")
                                    .as_bytes(),
                                file_name,
                                line_number,
                                &line,
                            ));
                        }
                    } else if parts[0] == b"global_transform" {
                        if parts.len() != 17 {
                            return Err(throw_error_message(
                                b"Malformed global_transform tag. 16 floats expected.",
                                file_name,
                                line_number,
                                &line,
                            ));
                        }

                        parts.remove(0); // Drop the 1st entry. (the tag)
                        if !string_vec_to_float_vec(&mut global_transform, &parts)
                            || global_transform.len() != 16
                        {
                            return Err(throw_error_message(
                                b"Malformed global_transform tag. Could not convert to float \
                                  array.",
                                file_name,
                                line_number,
                                &line,
                            ));
                        }
                    }
                    // TODO: element_size (aka scale3)
                    // TODO: world_origin (aka translate3)
                    else if parts[0] == b"data" {
                        in3d = true;
                    }
                } else if string_vec_to_float_vec(&mut tmpfloats, &parts) && tmpfloats.len() == 3 {
                    let max = MAX_3D_LUT_LENGTH as usize;
                    if raw3d.len() > max * max * max * 3 {
                        return Err(throw_error_message(
                            b"Too many 3D LUT entries.",
                            file_name,
                            line_number,
                            &line,
                        ));
                    }
                    raw3d.extend_from_slice(&tmpfloats);
                }
            }
        }

        // Interpret the parsed data, validate LUT sizes
        if size3d[0] * size3d[1] * size3d[2] != (raw3d.len() / 3) as i32 {
            return Err(throw_error_message(
                format!(
                    "Incorrect number of 3D LUT entries. Found {}, expected {}.",
                    raw3d.len() / 3,
                    size3d[0] * size3d[1] * size3d[2]
                )
                .as_bytes(),
                file_name,
                -1,
                b"",
            ));
        }

        if size3d[0] * size3d[1] * size3d[2] == 0 {
            return Err(throw_error_message(
                b"No 3D LUT entries found.",
                file_name,
                -1,
                b"",
            ));
        }

        let mut m44 = [0.0f64; 16];
        let mut use_matrix = false;

        // Setup the global matrix. (Nuke pre-scales this by the 3D LUT size, so we must undo
        // that here)
        if global_transform.len() == 16 {
            for i in 0..4 {
                global_transform[4 * i] *= size3d[0] as f32;
                global_transform[4 * i + 1] *= size3d[1] as f32;
                global_transform[4 * i + 2] *= size3d[2] as f32;
                m44[4 * i] = f64::from(global_transform[4 * i]);
                m44[4 * i + 1] = f64::from(global_transform[4 * i + 1]);
                m44[4 * i + 2] = f64::from(global_transform[4 * i + 2]);
                m44[4 * i + 3] = f64::from(global_transform[4 * i + 3]);
            }
            use_matrix = true;
        }

        // Copy raw3d vector into LutOpData object.
        let mut lut3d = Lut3DOpData::new(size3d[0] as c_ulong)?;
        if Lut3DOpData::is_valid_interpolation(interp) {
            lut3d.set_interpolation(interp);
        }
        lut3d.set_file_output_bit_depth(BitDepth::F32);

        // LUT in file is blue fastest.
        *lut3d.get_array_mut().get_values_mut() = raw3d;

        Ok(LocalCachedFile {
            lut3d: Some(lut3d),
            m44,
            use_matrix,
        })
    }
}

impl FileFormat for LocalFileFormat {
    /// Port of `LocalFileFormat::getFormatInfo` (FileFormatVF.cpp:83-90 @ v2.5.2).
    fn format_info(&self) -> Vec<FormatInfo> {
        vec![FormatInfo::new(
            "nukevf",
            "vf",
            FormatCapabilities::READ,
            FormatBakeCapabilities::NONE,
        )]
    }

    /// Port of `LocalFileFormat::read` (FileFormatVF.cpp:92-265 @ v2.5.2).
    fn read(
        &self,
        istream: &mut InputStream,
        file_name: &[u8],
        interp: Interpolation,
    ) -> Result<CachedFileRcPtr> {
        Ok(Arc::new(self.read_file(istream, file_name, interp)?))
    }

    /// Appends, forward, the global matrix then the 3D LUT; inverse, the 3D LUT then the
    /// matrix.
    ///
    /// Port of `LocalFileFormat::buildFileOps` (FileFormatVF.cpp:267-327 @ v2.5.2).
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
            return Err(Exception::new("Cannot build .vf Op. Invalid cache type."));
        };

        let new_dir = combine_transform_directions(dir, file_transform.direction());

        let file_interp = file_transform.interpolation();

        let mut lut3d = None;

        if cached_file.lut3d.is_some() {
            let mut file_interp_used = false;
            lut3d = handle_lut3d(
                cached_file.lut3d.as_ref(),
                file_interp,
                &mut file_interp_used,
            );

            if !file_interp_used {
                log_warning_interpolation_not_used(file_interp, file_transform);
            }
        }

        match new_dir {
            TransformDirection::Forward => {
                if cached_file.use_matrix {
                    create_matrix_op_from_m44(ops, &cached_file.m44, new_dir);
                }

                if let Some(lut3d) = lut3d {
                    create_lut3d_op(ops, lut3d, new_dir);
                }
            }
            TransformDirection::Inverse => {
                if let Some(lut3d) = lut3d {
                    create_lut3d_op(ops, lut3d, new_dir);
                }

                if cached_file.use_matrix {
                    create_matrix_op_from_m44(ops, &cached_file.m44, new_dir);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "file_format_vf_tests.rs"]
mod tests;
