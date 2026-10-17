// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Truelight `.cub` format: an optional 1D shaper (`InputLUT`) and a 3D LUT (`Cube`, red
//! fastest), after `# Truelight Cube` and `#` tags. A port of
//! `src/OpenColorIO/fileformats/FileFormatTruelight.cpp` @ v2.5.2.
//!
//! ```text
//! # Truelight Cube v2.0
//! # iDims 3
//! # oDims 3
//! # width 3 3 3
//! # lutLength 5
//! # InputLUT
//!  0.000000 0.000000 0.000000
//!  ...
//! # Cube
//!  0.000000 0.000000 0.000000
//!  ...
//! # end
//! ```
//!
//! The shaper's values run from 0 to the cube's size minus 1, and are scaled to [0, 1].
//! Integer encodings of the shaper are not supported.

use std::any::Any;
use std::ffi::c_ulong;
use std::sync::Arc;

use ocio_ops::cfmt::{FloatField, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::image_desc::PackedImageDesc;
use ocio_ops::math_utils::std_max;
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{BitDepth, TransformDirection, combine_transform_directions};
use ocio_ops::ops::lut1d::Lut1DOpData;
use ocio_ops::ops::lut1d::lut1d_op::create_lut1d_op;
use ocio_ops::ops::lut3d::lut3d_op::{Lut3DOrder, create_lut3d_op, generate_identity_lut3d};
use ocio_ops::ops::lut3d::lut3d_op_data::{Interpolation, Lut3DOpData, MAX_3D_LUT_LENGTH};
use ocio_ops::parse_utils::{string_to_int, string_vec_to_float_vec};
use ocio_ops::utils::string_utils::{
    lower, split_by_white_spaces, starts_with, starts_with_char, trim,
};

use crate::baker::Baker;
use crate::baking_utils::{LinuxResize, bake_buffer, cube_values, get_input_to_target_processor};
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

/// A `.cub` file's shaper and 3D LUT.
///
/// Port of `LocalCachedFile` (FileFormatTruelight.cpp:41-49 @ v2.5.2).
#[derive(Debug, Clone, Default)]
pub struct LocalCachedFile {
    /// `lut1D`.
    pub lut1d: Option<Lut1DOpData>,
    /// `lut3D`.
    pub lut3d: Option<Lut3DOpData>,
}

impl CachedFile for LocalCachedFile {}

/// The Truelight `.cub` format.
///
/// Port of `LocalFileFormat` (FileFormatTruelight.cpp:54-77 @ v2.5.2).
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalFileFormat;

impl LocalFileFormat {
    /// Reads a `.cub` file: `# Truelight Cube` on its first line, then the `#` tags (`width`,
    /// `lutLength`, `InputLUT`, `Cube`, `end`, in any case) and the lines of 3 values of the
    /// block they open, up to `# end`.
    ///
    /// Port of `LocalFileFormat::read` (FileFormatTruelight.cpp:89-293 @ v2.5.2).
    pub fn read_file(
        &self,
        istream: &mut InputStream,
        interp: Interpolation,
    ) -> Result<LocalCachedFile> {
        // this shouldn't happen
        if istream.fail() {
            return Err(Exception::new(
                "File stream empty when trying to read Truelight .cub LUT",
            ));
        }

        // Validate the file type
        let mut line = Vec::new();
        if !nextline(istream, &mut line) || !starts_with(&lower(&line), b"# truelight cube") {
            return Err(Exception::new(
                "LUT doesn't seem to be a Truelight .cub LUT.",
            ));
        }

        // Parse the file
        let mut raw1d: Vec<f32> = Vec::new();
        let mut raw3d: Vec<f32> = Vec::new();
        let mut size3d: [i32; 3] = [0, 0, 0];
        let mut size1d: i32 = 0;
        {
            let mut tmpfloats: Vec<f32> = Vec::new();

            let mut in1d = false;
            let mut in3d = false;

            while nextline(istream, &mut line) {
                // Strip, lowercase, and split the line
                let parts = split_by_white_spaces(&lower(trim(&line)));

                if parts.is_empty() {
                    continue;
                }

                // Parse header metadata (which starts with #)
                if starts_with_char(&parts[0], b'#') {
                    if parts.len() < 2 {
                        continue;
                    }

                    if parts[1] == b"width" {
                        if parts.len() != 5
                            || !string_to_int(&mut size3d[0], &parts[2], false)
                            || !string_to_int(&mut size3d[1], &parts[3], false)
                            || !string_to_int(&mut size3d[2], &parts[4], false)
                        {
                            return Err(Exception::new(
                                "Malformed width tag in Truelight .cub LUT.",
                            ));
                        }

                        if size3d[0] != size3d[1] || size3d[0] != size3d[2] {
                            return Err(Exception::new(format!(
                                "Truelight .cub LUT. Only equal grid size LUTs are supported. \
                                 Found grid size: {} x {} x {}.",
                                size3d[0], size3d[1], size3d[2]
                            )));
                        }

                        if size3d[0] < 2 || i64::from(size3d[0]) > i64::from(MAX_3D_LUT_LENGTH) {
                            return Err(Exception::new(format!(
                                "Truelight .cub LUT grid size must be between 2 and \
                                 {MAX_3D_LUT_LENGTH}."
                            )));
                        }
                    } else if parts[1] == b"lutlength" {
                        if parts.len() != 3 || !string_to_int(&mut size1d, &parts[2], false) {
                            return Err(Exception::new(
                                "Malformed lutlength tag in Truelight .cub LUT.",
                            ));
                        }
                        if size1d < 2 || i64::from(size1d) > MAX_1D_LUT_LENGTH as i64 {
                            return Err(Exception::new(format!(
                                "Truelight .cub LUT lutlength must be between 2 and \
                                 {MAX_1D_LUT_LENGTH}."
                            )));
                        }
                    } else if parts[1] == b"inputlut" {
                        in1d = true;
                        in3d = false;
                    } else if parts[1] == b"cube" {
                        in3d = true;
                        in1d = false;
                    } else if parts[1] == b"end" {
                        // If we hit the end tag, don't bother searching further in the file.
                        break;
                    }
                }

                if (in1d || in3d)
                    && string_vec_to_float_vec(&mut tmpfloats, &parts)
                    && tmpfloats.len() == 3
                {
                    if in1d {
                        if raw1d.len() > MAX_1D_LUT_LENGTH as usize * 3 {
                            return Err(Exception::new(
                                "Too many 1D LUT entries in Truelight .cub LUT.",
                            ));
                        }
                        raw1d.extend_from_slice(&tmpfloats);
                    } else if in3d {
                        let max = MAX_3D_LUT_LENGTH as usize;
                        if raw3d.len() > max * max * max * 3 {
                            return Err(Exception::new(
                                "Too many 3D LUT entries in Truelight .cub LUT.",
                            ));
                        }
                        raw3d.extend_from_slice(&tmpfloats);
                    }
                }
            }
        }

        // Interpret the parsed data, validate LUT sizes

        if size1d != (raw1d.len() / 3) as i32 {
            return Err(Exception::new(format!(
                "Parse error in Truelight .cub LUT. Incorrect number of lut1d entries. Found {}, \
                 expected {size1d}.",
                raw1d.len() / 3
            )));
        }

        if size3d[0] * size3d[1] * size3d[2] != (raw3d.len() / 3) as i32 {
            return Err(Exception::new(format!(
                "Parse error in Truelight .cub LUT. Incorrect number of 3D LUT entries. Found {}, \
                 expected {}.",
                raw3d.len() / 3,
                size3d[0] * size3d[1] * size3d[2]
            )));
        }

        let mut cached_file = LocalCachedFile::default();

        let has3d = size3d[0] * size3d[1] * size3d[2] > 0;

        // Reformat 1D data
        if size1d > 0 {
            let mut lut1d = Lut1DOpData::new(size1d as c_ulong)?;
            if Lut1DOpData::is_valid_interpolation(interp) {
                lut1d.set_interpolation(interp);
            }
            lut1d.set_file_output_bit_depth(BitDepth::F32);

            // Determine the scale factor for the 1D LUT. Example: the inputlut feeding a
            // 6x6x6 3D LUT should be scaled from 0.0-5.0. Beware: Nuke Truelight Writer (at
            // least 6.3 and before) is busted and does this scaling incorrectly.

            let mut descale = 1.0f32;
            if has3d {
                descale = 1.0f32 / (size3d[0] - 1) as f32;
            }

            let lut_array = lut1d.get_array_mut().get_values_mut();
            for (value, &raw) in lut_array.iter_mut().zip(&raw1d) {
                *value = raw * descale;
            }
            cached_file.lut1d = Some(lut1d);
        }

        if has3d {
            // Reformat 3D data
            let mut lut3d = Lut3DOpData::new(size3d[0] as c_ulong)?;
            if Lut3DOpData::is_valid_interpolation(interp) {
                lut3d.set_interpolation(interp);
            }
            lut3d.set_file_output_bit_depth(BitDepth::F32);
            lut3d.set_array_from_red_fastest_order(&raw3d)?;
            cached_file.lut3d = Some(lut3d);
        }

        Ok(cached_file)
    }
}

impl FileFormat for LocalFileFormat {
    /// Port of `LocalFileFormat::getFormatInfo` (FileFormatTruelight.cpp:79-87 @ v2.5.2).
    fn format_info(&self) -> Vec<FormatInfo> {
        vec![FormatInfo::new(
            "truelight",
            "cub",
            FormatCapabilities::READ | FormatCapabilities::BAKE,
            FormatBakeCapabilities::LUT_3D,
        )]
    }

    /// Port of `LocalFileFormat::read` (FileFormatTruelight.cpp:89-293 @ v2.5.2).
    fn read(
        &self,
        istream: &mut InputStream,
        _file_name: &[u8],
        interp: Interpolation,
    ) -> Result<CachedFileRcPtr> {
        Ok(Arc::new(self.read_file(istream, interp)?))
    }

    /// Bakes `baker`'s transform into a cube (32 per side by default, 2 at least), red
    /// fastest, after an identity shaper of the shaper size (1024 by default, 2 at least) over
    /// the cube's indices. The format name isn't checked.
    ///
    /// Port of `LocalFileFormat::bake` (FileFormatTruelight.cpp:296-355 @ v2.5.2).
    fn bake(&self, baker: &Baker, _format_name: &[u8], ostream: &mut OStringStream) -> Result<()> {
        const DEFAULT_CUBE_SIZE: i32 = 32;
        const DEFAULT_SHAPER_SIZE: i32 = 1024;

        let mut cube_size = baker.cube_size();
        if cube_size == -1 {
            cube_size = DEFAULT_CUBE_SIZE;
        }
        cube_size = std_max(2, cube_size); // smallest cube is 2x2x2

        let mut cube_data = bake_buffer(cube_values(cube_size), LinuxResize::Unchecked)?;
        let num_entries = cube_size * cube_size * cube_size;
        generate_identity_lut3d(&mut cube_data, cube_size, 3, Lut3DOrder::FastRed)?;
        let mut cube_img = PackedImageDesc::new(&mut cube_data[..], num_entries as usize, 1, 3)?;

        // Apply processor to LUT data
        let input_to_target = get_input_to_target_processor(baker)?;
        input_to_target.apply(&mut cube_img)?;

        let mut shaper_size = baker.shaper_size();
        if shaper_size == -1 {
            shaper_size = DEFAULT_SHAPER_SIZE;
        }
        shaper_size = std_max(2, shaper_size); // smallest shaper is 2x2x2

        // Write the header
        ostream.put_str("# Truelight Cube v2.0\n");
        ostream.put_str("# lutLength ");
        ostream.put_i32(shaper_size);
        ostream.put_str("\n");
        ostream.put_str("# iDims     3\n");
        ostream.put_str("# oDims     3\n");
        ostream.put_str("# width     ");
        ostream.put_i32(cube_size);
        ostream.put_str(" ");
        ostream.put_i32(cube_size);
        ostream.put_str(" ");
        ostream.put_i32(cube_size);
        ostream.put_str("\n");
        ostream.put_str("\n");

        // Write the shaper LUT
        // (We are just going to use a unity LUT)
        ostream.put_str("# InputLUT\n");
        ostream.precision = 6;
        ostream.float_field = FloatField::Fixed;
        let triple = |ostream: &mut OStringStream, v: f32| {
            ostream.put_f32(v);
            ostream.put_str(" ");
            ostream.put_f32(v);
            ostream.put_str(" ");
            ostream.put_f32(v);
            ostream.put_str("\n");
        };
        for i in 0..shaper_size - 1 {
            let v = (i as f32 / (shaper_size - 1) as f32) * (cube_size - 1) as f32;
            triple(ostream, v);
        }
        let v = (cube_size - 1) as f32;
        triple(ostream, v); // ensure that the last value is spot on
        ostream.put_str("\n");

        // Write the cube
        ostream.put_str("# Cube\n");
        for i in 0..num_entries as usize {
            ostream.put_f32(cube_data[3 * i]);
            ostream.put_str(" ");
            ostream.put_f32(cube_data[3 * i + 1]);
            ostream.put_str(" ");
            ostream.put_f32(cube_data[3 * i + 2]);
            ostream.put_str("\n");
        }

        ostream.put_str("# end\n");
        Ok(())
    }

    /// Appends, forward, the shaper then the 3D LUT; inverse, the 3D LUT then the shaper.
    ///
    /// Port of `LocalFileFormat::buildFileOps` (FileFormatTruelight.cpp:357-413 @ v2.5.2).
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
                    "Cannot build Truelight .cub Op. Invalid cache type.",
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
                    create_lut1d_op(ops, lut1d, new_dir);
                }

                if let Some(lut3d) = lut3d {
                    create_lut3d_op(ops, lut3d, new_dir);
                }
            }
            TransformDirection::Inverse => {
                if let Some(lut3d) = lut3d {
                    create_lut3d_op(ops, lut3d, new_dir);
                }

                if let Some(lut1d) = lut1d {
                    create_lut1d_op(ops, lut1d, new_dir);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "file_format_truelight_tests.rs"]
mod tests;
