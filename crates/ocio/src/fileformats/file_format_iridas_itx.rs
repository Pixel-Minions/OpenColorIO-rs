// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Iridas `.itx` format: a 3D LUT, red fastest. A port of
//! `src/OpenColorIO/fileformats/FileFormatIridasItx.cpp` @ v2.5.2.
//!
//! ```text
//! LUT_3D_SIZE M
//! 0.0 0.0 0.0
//! 1.0 0.0 0.0
//! ...
//! ```
//!
//! Lines before `LUT_3D_SIZE` are skipped; every line after it is an entry of three values.

use std::any::Any;
use std::ffi::c_ulong;
use std::sync::Arc;

use ocio_ops::cfmt::{Crt, FloatField, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::image_desc::PackedImageDesc;
use ocio_ops::math_utils::std_max;
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{BitDepth, TransformDirection, combine_transform_directions};
use ocio_ops::ops::lut3d::lut3d_op::{Lut3DOrder, create_lut3d_op, generate_identity_lut3d};
use ocio_ops::ops::lut3d::lut3d_op_data::{Interpolation, Lut3DOpData, MAX_3D_LUT_LENGTH};
use ocio_ops::parse_utils::{string_to_int, string_vec_to_float_vec};
use ocio_ops::utils::string_utils::{lower, split_by_white_spaces, starts_with_char, trim};

use crate::baker::Baker;
use crate::baking_utils::{LinuxResize, bake_buffer, cube_values, get_input_to_target_processor};
use crate::config::Config;
use crate::context::Context;
use crate::fileformats::file_format_utils::{handle_lut3d, log_warning_interpolation_not_used};
use crate::fileformats::input_stream::InputStream;
use crate::parse_utils::nextline;
use crate::transforms::file_format::{
    CachedFile, CachedFileRcPtr, FileFormat, FormatBakeCapabilities, FormatCapabilities, FormatInfo,
};
use crate::transforms::file_transform::FileTransform;

/// An Iridas `.itx` file's 3D LUT.
///
/// Port of `LocalCachedFile` (FileFormatIridasItx.cpp:50-57 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct LocalCachedFile {
    /// `lut3D`.
    pub lut3d: Lut3DOpData,
}

impl CachedFile for LocalCachedFile {}

/// The Iridas `.itx` format.
///
/// Port of `LocalFileFormat` (FileFormatIridasItx.cpp:63-92 @ v2.5.2).
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalFileFormat;

/// `Error parsing Iridas .itx file (<file>).  [At line (<n>): '<line>'.  ]<error>`.
///
/// Port of `LocalFileFormat::ThrowErrorMessage` (FileFormatIridasItx.cpp:94-111 @ v2.5.2).
fn throw_error_message(
    error: &[u8],
    file_name: &[u8],
    line: i32,
    line_content: &[u8],
) -> Exception {
    let mut os = b"Error parsing Iridas .itx file (".to_vec();
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
    /// Reads an `.itx` file: comments and lines skipped up to `LUT_3D_SIZE`, then the entries,
    /// three values a line.
    ///
    /// Port of `LocalFileFormat::read` (FileFormatIridasItx.cpp:123-249 @ v2.5.2).
    pub fn read_file(
        &self,
        istream: &mut InputStream,
        file_name: &[u8],
        interp: Interpolation,
    ) -> Result<LocalCachedFile> {
        // this shouldn't happen
        if istream.fail() {
            return Err(Exception::new(
                "File stream empty when trying to read Iridas .itx LUT",
            ));
        }

        // Parse the file
        let mut raw: Vec<f32> = Vec::new();

        let mut size3d: i32 = 0;
        let mut in3d = false;

        {
            let mut line: Vec<u8> = Vec::new();
            let mut tmpfloats: Vec<f32> = Vec::new();
            let mut line_number: i32 = 0;

            while nextline(istream, &mut line) {
                line_number += 1;
                // All lines starting with '#' are comments
                if starts_with_char(&line, b'#') {
                    continue;
                }

                // Strip, lowercase, and split the line
                let parts = split_by_white_spaces(&lower(trim(&line)));
                if parts.is_empty() {
                    continue;
                }

                if lower(&parts[0]) == b"lut_3d_size" {
                    let mut size = 0i32;

                    if parts.len() != 2 || !string_to_int(&mut size, &parts[1], false) {
                        return Err(throw_error_message(
                            b"Malformed LUT_3D_SIZE tag.",
                            file_name,
                            line_number,
                            &line,
                        ));
                    }
                    if size < 2 || i64::from(size) > i64::from(MAX_3D_LUT_LENGTH) {
                        return Err(throw_error_message(
                            format!("LUT_3D_SIZE must be between 2 and {MAX_3D_LUT_LENGTH}.")
                                .as_bytes(),
                            file_name,
                            line_number,
                            &line,
                        ));
                    }
                    size3d = size;

                    in3d = true;
                } else if in3d {
                    // It must be a float triple!

                    if !string_vec_to_float_vec(&mut tmpfloats, &parts) || tmpfloats.len() != 3 {
                        return Err(throw_error_message(
                            b"Malformed color triples specified.",
                            file_name,
                            line_number,
                            &line,
                        ));
                    }

                    let max = MAX_3D_LUT_LENGTH as usize;
                    if raw.len() > max * max * max * 3 {
                        return Err(throw_error_message(
                            b"Too many 3D LUT entries.",
                            file_name,
                            line_number,
                            &line,
                        ));
                    }

                    raw.extend_from_slice(&tmpfloats[..3]);
                }
            }
        }

        // Interpret the parsed data, validate LUT sizes
        if in3d {
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
            let mut lut3d = Lut3DOpData::new(size3d as c_ulong)?;
            if Lut3DOpData::is_valid_interpolation(interp) {
                lut3d.set_interpolation(interp);
            }

            lut3d.set_file_output_bit_depth(BitDepth::F32);
            lut3d.set_array_from_red_fastest_order(&raw)?;
            Ok(LocalCachedFile { lut3d })
        } else {
            Err(throw_error_message(b"No 3D LUT found.", file_name, -1, b""))
        }
    }
}

impl FileFormat for LocalFileFormat {
    /// Port of `LocalFileFormat::getFormatInfo` (FileFormatIridasItx.cpp:113-121 @ v2.5.2).
    fn format_info(&self) -> Vec<FormatInfo> {
        vec![FormatInfo::new(
            "iridas_itx",
            "itx",
            FormatCapabilities::READ | FormatCapabilities::BAKE,
            FormatBakeCapabilities::LUT_3D,
        )]
    }

    /// Port of `LocalFileFormat::read` (FileFormatIridasItx.cpp:123-249 @ v2.5.2).
    fn read(
        &self,
        istream: &mut InputStream,
        file_name: &[u8],
        interp: Interpolation,
    ) -> Result<CachedFileRcPtr> {
        Ok(Arc::new(self.read_file(istream, file_name, interp)?))
    }

    /// Bakes `baker`'s transform into a 3D LUT of the cube size (64 by default, 2 at least),
    /// red fastest, without shaper or metadata.
    ///
    /// Port of `LocalFileFormat::bake` (FileFormatIridasItx.cpp:251-301 @ v2.5.2).
    fn bake(&self, baker: &Baker, format_name: &[u8], ostream: &mut OStringStream) -> Result<()> {
        let default_cube_size: i32 = 64;

        if format_name != b"iridas_itx" {
            let mut os = OStringStream::new(Crt::NATIVE);
            os.put_str("Unknown 3dl format name, '");
            os.put_bytes(format_name);
            os.put_str("'.");
            return Err(Exception::new(os.into_bytes()));
        }

        let mut cube_size = baker.cube_size();
        if cube_size == -1 {
            cube_size = default_cube_size;
        }
        cube_size = std_max(2, cube_size); // smallest cube is 2x2x2

        let mut cube_data = bake_buffer(cube_values(cube_size), LinuxResize::Unchecked)?;
        let num_entries = cube_size * cube_size * cube_size;
        generate_identity_lut3d(&mut cube_data, cube_size, 3, Lut3DOrder::FastRed)?;
        let mut cube_img = PackedImageDesc::new(&mut cube_data[..], num_entries as usize, 1, 3)?;

        // Apply our conversion from the input space to the output space.
        let input_to_target = get_input_to_target_processor(baker)?;
        input_to_target.apply(&mut cube_img)?;

        // Write out the file.
        // For for maximum compatibility with other apps, we will
        // not utilize the shaper or output any metadata

        ostream.put_str("LUT_3D_SIZE ");
        ostream.put_i32(cube_size);
        ostream.put_str("\n");
        if cube_size < 2 {
            return Err(Exception::new("Internal cube size exception."));
        }

        // Set to a fixed 6 decimal precision
        ostream.float_field = FloatField::Fixed;
        ostream.precision = 6;
        for i in 0..num_entries as usize {
            let r = cube_data[3 * i];
            let g = cube_data[3 * i + 1];
            let b = cube_data[3 * i + 2];
            ostream.put_f32(r);
            ostream.put_str(" ");
            ostream.put_f32(g);
            ostream.put_str(" ");
            ostream.put_f32(b);
            ostream.put_str("\n");
        }
        ostream.put_str("\n");
        Ok(())
    }

    /// Appends the file's 3D LUT, with the file transform's interpolation where valid for it.
    ///
    /// Port of `LocalFileFormat::buildFileOps` (FileFormatIridasItx.cpp:304-335 @ v2.5.2).
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
                "Cannot build Iridas .itx Op. Invalid cache type.",
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
#[path = "file_format_iridas_itx_tests.rs"]
mod tests;
