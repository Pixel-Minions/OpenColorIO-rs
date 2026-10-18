// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The spi3d format: a 3D LUT, one line per entry, in any order. A port of
//! `src/OpenColorIO/fileformats/FileFormatSpi3D.cpp` @ v2.5.2.
//!
//! ```text
//! SPILUT 1.0
//! 3 3
//! 32 32 32
//! 0 0 0 0.0132509 0.0158522 0.0156622
//! 0 0 1 0.0136178 0.018843 0.033921
//! 0 0 2 0.0136487 0.0240918 0.0563014
//! 0 0 3 0.015706 0.0303061 0.0774135
//! ```
//!
//! Entries can be in any order; after the expected number of entries is found, the file can
//! contain anything.

use std::any::Any;
use std::ffi::c_ulong;
use std::sync::Arc;

use ocio_ops::cfmt::{Crt, FloatField, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::image_desc::PackedImageDesc;
use ocio_ops::math_utils::std_max;
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{BitDepth, TransformDirection, combine_transform_directions};
use ocio_ops::ops::lut3d::lut3d_op::{
    Lut3DOrder, create_lut3d_op, generate_identity_lut3d, get_lut3d_index_blue_fast,
};
use ocio_ops::ops::lut3d::lut3d_op_data::{Interpolation, Lut3DOpData, MAX_3D_LUT_LENGTH};
use ocio_ops::utils::cscan::{ScanArg, ScanCrt, sscanf};
use ocio_ops::utils::number_utils::{Errc, Flavor, from_chars_f32};
use ocio_ops::utils::string_utils::{c_str, lower, starts_with};

use crate::baker::Baker;
use crate::baking_utils::{LinuxResize, bake_buffer, cube_values, get_input_to_target_processor};
use crate::config::Config;
use crate::context::Context;
use crate::fileformats::file_format_utils::{handle_lut3d, log_warning_interpolation_not_used};
use crate::fileformats::input_stream::InputStream;
use crate::transforms::file_format::{
    CachedFile, CachedFileRcPtr, FileFormat, FormatBakeCapabilities, FormatCapabilities, FormatInfo,
};
use crate::transforms::file_transform::FileTransform;

/// A spi3d file's LUT.
///
/// Port of `LocalCachedFile` (FileFormatSpi3D.cpp:37-44 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct LocalCachedFile {
    /// `lut`.
    pub lut: Lut3DOpData,
}

impl CachedFile for LocalCachedFile {}

/// The spi3d format.
///
/// Port of `LocalFileFormat` (FileFormatSpi3D.cpp:48-70 @ v2.5.2).
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalFileFormat;

/// The longest line the reader reads, its NUL included.
const MAX_LINE_SIZE: usize = 4096;

/// `Error parsing .spi3d file (<file>). <error>`.
fn error(file_name: &[u8], separator: &[u8], text: &[u8]) -> Exception {
    Exception::new(
        [
            b"Error parsing .spi3d file (".as_slice(),
            file_name,
            b").",
            separator,
            text,
        ]
        .concat(),
    )
}

/// `NumberUtils::from_chars(s, s + 64, value)` on a 64-byte buffer: whether it parsed.
fn parse_float(s: &[u8; 64], value: &mut f32) -> bool {
    from_chars_f32(Flavor::NATIVE, s, 64, value).ec == Errc::Ok
}

impl LocalFileFormat {
    /// Reads a spi3d file: `SPILUT` (in any case) on the first line, a line it skips, the
    /// three sizes, then an entry per line (`r g b R G B`) until every entry is set, skipping
    /// the lines that don't scan as an entry.
    ///
    /// Port of `LocalFileFormat::read` (FileFormatSpi3D.cpp:82-265 @ v2.5.2).
    pub fn read_file(
        &self,
        istream: &mut InputStream,
        file_name: &[u8],
        interp: Interpolation,
    ) -> Result<LocalCachedFile> {
        let mut line_buffer = [0u8; MAX_LINE_SIZE];

        // Read header information
        istream.getline_buf(&mut line_buffer, MAX_LINE_SIZE);
        if !starts_with(&lower(c_str(&line_buffer)), b"spilut") {
            return Err(error(
                file_name,
                b"  ",
                &[
                    b"LUT does not appear to be valid spilut format. Expected 'SPILUT'.  Found: '"
                        .as_slice(),
                    c_str(&line_buffer),
                    b"'.",
                ]
                .concat(),
            ));
        }

        // TODO: Assert 2nd line is 3 3
        istream.getline_buf(&mut line_buffer, MAX_LINE_SIZE);

        // Get LUT Size
        let (mut r_size, mut g_size, mut b_size) = (0i32, 0i32, 0i32);
        istream.getline_buf(&mut line_buffer, MAX_LINE_SIZE);
        if 3 != sscanf(
            ScanCrt::NATIVE,
            &line_buffer,
            b"%d %d %d",
            &mut [
                ScanArg::Int(&mut r_size),
                ScanArg::Int(&mut g_size),
                ScanArg::Int(&mut b_size),
            ],
        ) {
            return Err(error(
                file_name,
                b" ",
                &[
                    b"Error while reading LUT size. Found: '".as_slice(),
                    c_str(&line_buffer),
                    b"'.",
                ]
                .concat(),
            ));
        }

        // TODO: Support nonuniformly sized LUTs.
        if r_size != g_size || r_size != b_size {
            return Err(error(
                file_name,
                b" ",
                &[
                    b"LUT size should be the same for all components. Found: '".as_slice(),
                    c_str(&line_buffer),
                    b"'.",
                ]
                .concat(),
            ));
        }

        if r_size < 2 || i64::from(r_size) > i64::from(MAX_3D_LUT_LENGTH) {
            return Err(error(
                file_name,
                b" ",
                &[
                    format!("LUT size must be between 2 and {MAX_3D_LUT_LENGTH}. Found: '")
                        .as_bytes(),
                    c_str(&line_buffer),
                    b"'.",
                ]
                .concat(),
            ));
        }

        let mut lut3d = Lut3DOpData::new(r_size as c_ulong)?;
        if Lut3DOpData::is_valid_interpolation(interp) {
            lut3d.set_interpolation(interp);
        }
        lut3d.set_file_output_bit_depth(BitDepth::F32);

        // Parse table
        let (mut r_index, mut g_index, mut b_index) = (0i32, 0i32, 0i32);
        let (mut red_value, mut green_value, mut blue_value) = (f32::NAN, f32::NAN, f32::NAN);

        let mut entries_remaining = r_size * g_size * b_size;
        let num_val = lut3d.get_array().get_num_values();
        let mut index_defined = vec![false; num_val as usize];
        while istream.good() && entries_remaining > 0 {
            istream.getline_buf(&mut line_buffer, MAX_LINE_SIZE);

            let mut red_value_s = [0u8; 64];
            let mut green_value_s = [0u8; 64];
            let mut blue_value_s = [0u8; 64];

            if sscanf(
                ScanCrt::NATIVE,
                &line_buffer,
                b"%d %d %d %63s %63s %63s",
                &mut [
                    ScanArg::Int(&mut r_index),
                    ScanArg::Int(&mut g_index),
                    ScanArg::Int(&mut b_index),
                    ScanArg::Str(&mut red_value_s),
                    ScanArg::Str(&mut green_value_s),
                    ScanArg::Str(&mut blue_value_s),
                ],
            ) == 6
            {
                let red_ok = parse_float(&red_value_s, &mut red_value);
                let green_ok = parse_float(&green_value_s, &mut green_value);
                let blue_ok = parse_float(&blue_value_s, &mut blue_value);

                if !red_ok || !green_ok || !blue_ok {
                    return Err(error(
                        file_name,
                        b" ",
                        &[
                            b"Data is invalid. A color value is specified (".as_slice(),
                            c_str(&red_value_s),
                            b" ",
                            c_str(&green_value_s),
                            b" ",
                            c_str(&blue_value_s),
                            b") that cannot be parsed as a floating-point triplet.",
                        ]
                        .concat(),
                    ));
                }

                let mut invalid_index = false;
                let mut index = 0i32;
                if r_index < 0
                    || r_index >= r_size
                    || g_index < 0
                    || g_index >= g_size
                    || b_index < 0
                    || b_index >= b_size
                {
                    invalid_index = true;
                } else {
                    index = get_lut3d_index_blue_fast(
                        r_index, g_index, b_index, r_size, g_size, b_size,
                    );
                    if index < 0 || index >= num_val as i32 {
                        invalid_index = true;
                    }
                }

                if invalid_index {
                    return Err(error(
                        file_name,
                        b" ",
                        format!(
                            "Data is invalid. A LUT entry is specified ({r_index} {g_index} \
                             {b_index}) that falls outside of the cube."
                        )
                        .as_bytes(),
                    ));
                }

                let index = index as usize;
                let lut_array = lut3d.get_array_mut().get_values_mut();
                lut_array[index] = red_value;
                lut_array[index + 1] = green_value;
                lut_array[index + 2] = blue_value;
                if !index_defined[index] {
                    entries_remaining -= 1;
                    index_defined[index] = true;
                } else {
                    return Err(error(
                        file_name,
                        b" ",
                        format!(
                            "Data is invalid. A LUT entry is specified multiple times \
                             ({r_index} {g_index} {b_index})."
                        )
                        .as_bytes(),
                    ));
                }
            }
        }

        // Have we fully populated the table?
        if entries_remaining > 0 {
            return Err(error(file_name, b" ", b"Not enough entries found."));
        }

        Ok(LocalCachedFile { lut: lut3d })
    }
}

impl FileFormat for LocalFileFormat {
    /// Port of `LocalFileFormat::getFormatInfo` (FileFormatSpi3D.cpp:72-80 @ v2.5.2).
    fn format_info(&self) -> Vec<FormatInfo> {
        vec![FormatInfo::new(
            "spi3d",
            "spi3d",
            FormatCapabilities::READ | FormatCapabilities::BAKE,
            FormatBakeCapabilities::LUT_3D,
        )]
    }

    /// Port of `LocalFileFormat::read` (FileFormatSpi3D.cpp:82-265 @ v2.5.2).
    fn read(
        &self,
        istream: &mut InputStream,
        file_name: &[u8],
        interp: Interpolation,
    ) -> Result<CachedFileRcPtr> {
        Ok(Arc::new(self.read_file(istream, file_name, interp)?))
    }

    /// Bakes `baker`'s transform into a 3D LUT of the cube size (32 by default, 2 at least),
    /// one line per entry, blue fastest.
    ///
    /// Port of `LocalFileFormat::bake` (FileFormatSpi3D.cpp:267-311 @ v2.5.2).
    fn bake(&self, baker: &Baker, format_name: &[u8], ostream: &mut OStringStream) -> Result<()> {
        const DEFAULT_CUBE_SIZE: i32 = 32;

        if format_name != b"spi3d" {
            let mut os = OStringStream::new(Crt::NATIVE);
            os.put_str("Unknown spi format name, '");
            os.put_bytes(format_name);
            os.put_str("'.");
            return Err(Exception::new(os.into_bytes()));
        }

        let mut cube_size = baker.cube_size();
        if cube_size == -1 {
            cube_size = DEFAULT_CUBE_SIZE;
        }
        cube_size = std_max(2, cube_size); // smallest cube is 2x2x2

        let mut cube_data = bake_buffer(cube_values(cube_size), LinuxResize::Unchecked)?;
        let num_entries = cube_size * cube_size * cube_size;
        generate_identity_lut3d(&mut cube_data, cube_size, 3, Lut3DOrder::FastBlue)?;
        let mut cube_img = PackedImageDesc::new(&mut cube_data[..], num_entries as usize, 1, 3)?;
        let input_to_target = get_input_to_target_processor(baker)?;
        input_to_target.apply(&mut cube_img)?;

        ostream.put_str("SPILUT 1.0\n");
        ostream.put_str("3 3\n");
        ostream.put_i32(cube_size);
        ostream.put_str(" ");
        ostream.put_i32(cube_size);
        ostream.put_str(" ");
        ostream.put_i32(cube_size);
        ostream.put_str("\n");

        // Set to a fixed 6 decimal precision
        ostream.float_field = FloatField::Fixed;
        ostream.precision = 6;
        for i in 0..num_entries {
            ostream.put_i32(((i / cube_size) / cube_size) % cube_size);
            ostream.put_str(" ");
            ostream.put_i32((i / cube_size) % cube_size);
            ostream.put_str(" ");
            ostream.put_i32(i % cube_size);
            ostream.put_str(" ");
            ostream.put_f32(cube_data[3 * i as usize]);
            ostream.put_str(" ");
            ostream.put_f32(cube_data[3 * i as usize + 1]);
            ostream.put_str(" ");
            ostream.put_f32(cube_data[3 * i as usize + 2]);
            ostream.put_str("\n");
        }
        Ok(())
    }

    /// Appends the file's 3D LUT, with the file transform's interpolation where valid for it.
    ///
    /// Port of `LocalFileFormat::buildFileOps` (FileFormatSpi3D.cpp:313-342 @ v2.5.2).
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
            return Err(Exception::new("Cannot build Spi3D Op. Invalid cache type."));
        };

        let new_dir = combine_transform_directions(dir, file_transform.direction());

        let file_interp = file_transform.interpolation();

        let mut file_interp_used = false;
        let lut = handle_lut3d(Some(&cached_file.lut), file_interp, &mut file_interp_used)
            .expect("the file's LUT");

        if !file_interp_used {
            log_warning_interpolation_not_used(file_interp, file_transform);
        }

        create_lut3d_op(ops, lut, new_dir);
        Ok(())
    }
}

#[cfg(test)]
#[path = "file_format_spi3d_tests.rs"]
mod tests;
