// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Discreet `.3dl` format (Flame and Lustre): an optional 1D shaper LUT and a 3D LUT of
//! integer code values, blue fastest. A port of
//! `src/OpenColorIO/fileformats/FileFormat3DL.cpp` @ v2.5.2.
//!
//! Lines that start with text or `#` are skipped; a line of more than 3 integers is the
//! shaper LUT; every line of 3 integers is a 3D LUT entry, and the cube's size comes from
//! their count. The bit depths come from the largest values.
//!
//! ```text
//! #Tokens required by applications - do not edit
//! 3DMESH
//! Mesh 4 12
//! 0 64 128 192 256 320 384 448 512 576 640 704 768 832 896 960 1023
//! 0 17 17
//! 0 0 88
//! ...
//! LUT8
//! gamma 1.0
//! ```

use std::any::Any;
use std::ffi::c_ulong;
use std::sync::Arc;

use ocio_ops::bit_depth_utils::get_bit_depth_max_value;
use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::image_desc::PackedImageDesc;
use ocio_ops::math_utils::std_pow;
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{BitDepth, TransformDirection, combine_transform_directions};
use ocio_ops::ops::lut1d::Lut1DOpData;
use ocio_ops::ops::lut1d::lut1d_op::{create_lut1d_op, generate_identity_lut1d};
use ocio_ops::ops::lut3d::lut3d_op::{
    Lut3DOrder, create_lut3d_op, generate_identity_lut3d, get_3d_lut_edge_len_from_num_pixels,
};
use ocio_ops::ops::lut3d::lut3d_op_data::{Interpolation, Lut3DOpData, MAX_3D_LUT_LENGTH};
use ocio_ops::parse_utils::string_vec_to_int_vec;
use ocio_ops::utils::string_utils::{c_str, split_by_white_spaces, starts_with_char, trim};

use crate::baker::Baker;
use crate::baking_utils::get_input_to_target_processor;
use crate::config::Config;
use crate::context::Context;
use crate::fileformats::file_format_utils::{
    handle_lut1d, handle_lut3d, log_warning_interpolation_not_used,
};
use crate::fileformats::input_stream::InputStream;
use crate::transforms::file_format::{
    CachedFile, CachedFileRcPtr, FileFormat, FormatBakeCapabilities, FormatCapabilities, FormatInfo,
};
use crate::transforms::file_transform::FileTransform;

/// A `.3dl` file's shaper LUT (when it isn't an identity) and 3D LUT.
///
/// Port of `LocalCachedFile` (FileFormat3DL.cpp:81-89 @ v2.5.2).
#[derive(Debug, Clone, Default)]
pub struct LocalCachedFile {
    /// `lut1D`.
    pub lut1d: Option<Lut1DOpData>,
    /// `lut3D`.
    pub lut3d: Option<Lut3DOpData>,
}

impl CachedFile for LocalCachedFile {}

/// The `.3dl` format.
///
/// Port of `LocalFileFormat` (FileFormat3DL.cpp:95-118 @ v2.5.2).
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalFileFormat;

/// The bit depth (8, 10, 12 or 16) whose code values most likely hold a largest value of
/// `testval`, allowing for a 2x overshoot; -1 for a negative value.
///
/// Port of `GetLikelyLutBitDepth` (FileFormat3DL.cpp:141-165 @ v2.5.2).
pub(crate) fn get_likely_lut_bit_depth(testval: i32) -> i32 {
    const MIN_BIT_DEPTH: i32 = 8;
    const MAX_BIT_DEPTH: i32 = 16;

    if testval < 0 {
        return -1;
    }

    // Only test even bit depths
    let mut bit_depth = MIN_BIT_DEPTH;
    while bit_depth <= MAX_BIT_DEPTH {
        let maxcode = std_pow(2.0, f64::from(bit_depth)) as i32;
        let adjusted_max = maxcode * 2 - 1;
        if testval <= adjusted_max {
            // Since 14-bit scaling is not used in practice, if the max is more than 8192,
            // they are likely 16-bit values.
            if bit_depth == 14 {
                bit_depth = 16;
            }

            return bit_depth;
        }
        bit_depth += 2;
    }

    MAX_BIT_DEPTH
}

/// Port of `GetOCIOBitdepth` (FileFormat3DL.cpp:167-183 @ v2.5.2).
fn get_ocio_bitdepth(bitdepth: i32) -> BitDepth {
    match bitdepth {
        8 => BitDepth::Uint8,
        10 => BitDepth::Uint10,
        12 => BitDepth::Uint12,
        16 => BitDepth::Uint16,
        _ => BitDepth::Unknown,
    }
}

/// Whether a shaper LUT is an identity within 2 code values. The shaper part of the format
/// was never properly documented and its use in the industry is inconsistent, hence the
/// loose tolerance.
///
/// Port of `IsIdentity` (FileFormat3DL.cpp:217-231 @ v2.5.2).
fn is_identity(rawshaper: &[i32], out_bit_depth: BitDepth) -> Result<bool> {
    let dim = rawshaper.len() as u32;
    let step_value = get_bit_depth_max_value(out_bit_depth)? as f32 / (dim as f32 - 1.0f32);

    for (i, &value) in rawshaper.iter().enumerate() {
        if ((i as u32) as f32 * step_value - value as f32).abs() >= 2.0f32 {
            return Ok(false);
        }
    }
    Ok(true)
}

/// `std::max(a, b)` on `int`s.
fn std_max(a: i32, b: i32) -> i32 {
    if a < b { b } else { a }
}

/// `std::vector<float>(n)` of a negative `int` `n`, whose `size_t` is past `max_size()`: the
/// `std::length_error` of the C++ library the wheel uses, "vector too long" in MSVC's STL and
/// "cannot create std::vector larger than max_size()" in libstdc++ (docs/improvements.md,
/// I-200).
const VECTOR_CONSTRUCTOR_TOO_LONG: &str = if cfg!(target_os = "windows") {
    "vector too long"
} else {
    "cannot create std::vector larger than max_size()"
};

/// 65 -> 6, 33 -> 5, 17 -> 4: `log2(size - 1)` in `float`, truncated.
///
/// Port of `CubeDimensionLenToLustreBitDepth` (FileFormat3DL.cpp:487-495 @ v2.5.2).
fn cube_dimension_len_to_lustre_bit_depth(size: i32) -> i32 {
    let logval = ((size - 1) as f32).ln() / std::hint::black_box(2.0f32).ln();
    logval as i32
}

/// `2^bit_depth - 1`, through libm's `pow`.
///
/// Port of `GetMaxValueFromIntegerBitDepth` (FileFormat3DL.cpp:185-188 @ v2.5.2).
fn get_max_value_from_integer_bit_depth(bit_depth: i32) -> i32 {
    std_pow(2.0, f64::from(bit_depth)) as i32 - 1
}

/// `val` clamped to [0, 1], scaled and rounded half away from zero: a NaN gives 0.
///
/// Port of `GetClampedIntFromNormFloat` (FileFormat3DL.cpp:190-194 @ v2.5.2).
fn get_clamped_int_from_norm_float(val: f32, scale: f32) -> i32 {
    let val = std_min_f32(std_max_f32(0.0f32, val), 1.0f32) * scale;
    val.round() as i32
}

/// `std::max(a, b)` on `float`s: `(a < b) ? b : a`.
fn std_max_f32(a: f32, b: f32) -> f32 {
    if a < b { b } else { a }
}

/// `std::min(a, b)` on `float`s: `(b < a) ? b : a`.
fn std_min_f32(a: f32, b: f32) -> f32 {
    if b < a { b } else { a }
}

impl LocalFileFormat {
    /// Reads a `.3dl` file: the lines of integers (the others skipped), then the shaper LUT
    /// and the 3D LUT they make, their bit depths inferred from their largest values.
    ///
    /// Port of `LocalFileFormat::read` (FileFormat3DL.cpp:236-485 @ v2.5.2).
    pub fn read_file(
        &self,
        istream: &mut InputStream,
        interp: Interpolation,
    ) -> Result<LocalCachedFile> {
        let mut rawshaper: Vec<i32> = Vec::new();
        let mut raw3d: Vec<i32> = Vec::new();

        let mut lut3dmax: i32 = 0;

        // Parse the file 3D LUT data to an int array.
        {
            const MAX_LINE_SIZE: usize = 4096;
            let mut line_buffer = [0u8; MAX_LINE_SIZE];

            let mut tmp_data: Vec<i32> = Vec::new();

            let mut line_number: i32 = 0;

            while istream.good() {
                istream.getline_buf(&mut line_buffer, MAX_LINE_SIZE);
                line_number += 1;
                let line = c_str(&line_buffer);

                // Strip and split the line.
                let line_parts = split_by_white_spaces(trim(line));

                if line_parts.is_empty() {
                    continue;
                }
                if starts_with_char(&line_parts[0], b'#') {
                    continue;
                }
                if starts_with_char(&line_parts[0], b'<') {
                    // Format error: reject files that could be formatted as xml.
                    return Err(Exception::new(
                        [
                            b"Error parsing .3dl file. Not expecting a line starting with \"<\"."
                                .as_slice(),
                            format!("Line ({line_number}): '").as_bytes(),
                            line,
                            b"'.",
                        ]
                        .concat(),
                    ));
                }

                // If we haven't found a list of ints, continue.
                if !string_vec_to_int_vec(&mut tmp_data, &line_parts) {
                    // Some keywords are valid (3DMESH, mesh, gamma, LUT*) but others could be
                    // format error. To preserve v1 behavior, don't reject them.
                    continue;
                }

                // If we've found more than 3 ints, and dont have a shaper LUT yet, we've got
                // it!
                if tmp_data.len() > 3 {
                    if rawshaper.is_empty() {
                        rawshaper.extend_from_slice(&tmp_data);
                    } else {
                        // Format error, more than 1 shaper LUT.
                        return Err(Exception::new(
                            [
                                b"Error parsing .3dl file. Appears to contain more than 1 shaper \
                                  LUT."
                                    .as_slice(),
                                format!("Line ({line_number}): '").as_bytes(),
                                line,
                                b"'.",
                            ]
                            .concat(),
                        ));
                    }
                }
                // If we've found 3 ints, add it to our 3D LUT.
                else if tmp_data.len() == 3 {
                    let max = MAX_3D_LUT_LENGTH as usize;
                    if raw3d.len() > max * max * max * 3 {
                        return Err(Exception::new(
                            "Error parsing .3dl file. Too many 3D LUT entries found.",
                        ));
                    }
                    raw3d.extend_from_slice(&tmp_data[..3]);
                    // Find the maximum shaper LUT value to infer bit-depth.
                    lut3dmax = std_max(lut3dmax, tmp_data[0]);
                    lut3dmax = std_max(lut3dmax, tmp_data[1]);
                    lut3dmax = std_max(lut3dmax, tmp_data[2]);
                } else {
                    // Format error, line with 1 or 2 int.
                    return Err(Exception::new(
                        [
                            b"Error parsing .3dl file. Invalid line with less than 3 values."
                                .as_slice(),
                            format!("Line ({line_number}): '").as_bytes(),
                            line,
                            b"'.",
                        ]
                        .concat(),
                    ));
                }
            }
        }

        if raw3d.is_empty() && rawshaper.is_empty() {
            return Err(Exception::new(
                "Error parsing .3dl file. Does not appear to contain a valid shaper LUT or a 3D \
                 LUT.",
            ));
        }

        let mut cached_file = LocalCachedFile::default();

        // If all we're doing to parse the format is to read in sets of 3 numbers, it's possible
        // that other formats will accidentally be able to be read mistakenly as .3dl files. We
        // can exclude a huge segment of these mis-reads by screening for files that use float
        // representations. I.e., if the MAX value of the LUT is a small number (such as
        // <128.0) it's likely not an integer format, and thus not a likely 3DL file.
        const FORMAT3DL_CODEVALUE_LOWEST_PLAUSIBLE_MAXINT: i32 = 128;

        // Interpret the shaper LUT
        if !rawshaper.is_empty() {
            // Find the maximum shaper LUT value to infer bit-depth.
            let shapermax = rawshaper.iter().fold(0, |m, &i| std_max(m, i));

            if shapermax < FORMAT3DL_CODEVALUE_LOWEST_PLAUSIBLE_MAXINT {
                return Err(Exception::new(format!(
                    "Error parsing .3dl file. The maximum shaper LUT value, {shapermax}, is \
                     unreasonably low. This LUT is probably not a .3dl file, but instead a \
                     related format that shares a similar structure."
                )));
            }

            let shaperbitdepth = get_likely_lut_bit_depth(shapermax);
            if shaperbitdepth < 0 {
                return Err(Exception::new(format!(
                    "Error parsing .3dl file. The maximum shaper LUT value, {shapermax}, does not \
                     correspond to any likely bit depth. Please confirm source file is valid."
                )));
            }

            let out1d_bd = get_ocio_bitdepth(shaperbitdepth);

            if out1d_bd == BitDepth::Unknown {
                return Err(Exception::new(
                    "Error parsing .3dl file. The shaper LUT bit depth is not known. Please \
                     confirm source file is valid.",
                ));
            }

            if !is_identity(&rawshaper, out1d_bd)? {
                let length = rawshaper.len() as c_ulong;
                let mut lut1d = Lut1DOpData::new(length)?;
                if Lut1DOpData::is_valid_interpolation(interp) {
                    lut1d.set_interpolation(interp);
                }
                lut1d.set_file_output_bit_depth(out1d_bd);

                let scale = get_bit_depth_max_value(out1d_bd)? as f32;
                let lut_array = lut1d.get_array_mut().get_values_mut();
                let mut p = 0usize;
                for &value in &rawshaper {
                    for _ in 0..3 {
                        lut_array[p] = value as f32 / scale;
                        p += 1;
                    }
                }
                cached_file.lut1d = Some(lut1d);
            }
        }

        // Interpret the parsed data.
        if !raw3d.is_empty() {
            // lut3dmax has been stored while reading values.
            if lut3dmax < FORMAT3DL_CODEVALUE_LOWEST_PLAUSIBLE_MAXINT {
                return Err(Exception::new(format!(
                    "Error parsing .3dl file.The maximum 3D LUT value, {lut3dmax}, is \
                     unreasonably low. This LUT is probably not a .3dl file, but instead a \
                     related format that shares a similar structure."
                )));
            }

            let lut3dbitdepth = get_likely_lut_bit_depth(lut3dmax);
            if lut3dbitdepth < 0 {
                return Err(Exception::new(format!(
                    "Error parsing .3dl file.The maximum 3D LUT value, {lut3dmax}, does not \
                     correspond to any likely bit depth. Please confirm source file is valid."
                )));
            }

            // Interpret the int array as a 3D LUT
            let lut_edge_len = get_3d_lut_edge_len_from_num_pixels(raw3d.len() as i32 / 3)?;

            // The 3dl format stores the LUT entries in blue-fastest order, which is the same
            // order used by Lut3DOpData, so no transposition of LUT entries is needed in this
            // case.

            let out3d_bd = get_ocio_bitdepth(lut3dbitdepth);

            let mut lut3d = Lut3DOpData::new(lut_edge_len as c_ulong)?;
            if Lut3DOpData::is_valid_interpolation(interp) {
                lut3d.set_interpolation(interp);
            }
            lut3d.set_file_output_bit_depth(out3d_bd);

            let scale = get_bit_depth_max_value(out3d_bd)? as f32;
            let lut_array = lut3d.get_array_mut().get_values_mut();
            for (i, &value) in raw3d.iter().enumerate() {
                lut_array[i] = value as f32 / scale;
            }
            cached_file.lut3d = Some(lut3d);
        }
        Ok(cached_file)
    }
}

impl FileFormat for LocalFileFormat {
    /// Port of `LocalFileFormat::getFormatInfo` (FileFormat3DL.cpp:196-208 @ v2.5.2): the
    /// "lustre" info is a copy of the "flame" one.
    fn format_info(&self) -> Vec<FormatInfo> {
        let info = FormatInfo::new(
            "flame",
            "3dl",
            FormatCapabilities::READ | FormatCapabilities::BAKE,
            FormatBakeCapabilities::LUT_3D,
        );
        let mut info2 = info;
        info2.name = "lustre";
        vec![info, info2]
    }

    /// Port of `LocalFileFormat::read` (FileFormat3DL.cpp:236-485 @ v2.5.2).
    fn read(
        &self,
        istream: &mut InputStream,
        _file_name: &[u8],
        interp: Interpolation,
    ) -> Result<CachedFileRcPtr> {
        Ok(Arc::new(self.read_file(istream, interp)?))
    }

    /// Bakes `baker`'s transform into a cube of 12-bit code values, blue fastest, after an
    /// identity shaper of 10-bit code values: 17 per side by default as flame, 33 as lustre,
    /// whose file also has a mesh header and a footer.
    ///
    /// Port of `LocalFileFormat::bake` (FileFormat3DL.cpp:497-584 @ v2.5.2).
    fn bake(&self, baker: &Baker, format_name: &[u8], ostream: &mut OStringStream) -> Result<()> {
        let shaper_bit_depth = 10;
        let cube_bit_depth = 12;

        // NOTE: This code is very old, Lustre and Flame have long been able
        //       to support much larger cube sizes.  Furthermore there is no
        //       need to use the legacy 3dl format since CLF/CTF is supported.
        let default_cube_size = if format_name == b"lustre" {
            33
        } else if format_name == b"flame" {
            17
        } else {
            let mut os = OStringStream::new(Crt::NATIVE);
            os.put_str("Unknown 3dl format name, '");
            os.put_bytes(format_name);
            os.put_str("'.");
            return Err(Exception::new(os.into_bytes()));
        };

        let mut cube_size = baker.cube_size();
        if cube_size == -1 {
            cube_size = default_cube_size;
        }
        cube_size = std_max(2, cube_size); // smallest cube is 2x2x2

        let mut shaper_size = baker.shaper_size();
        if shaper_size == -1 {
            shaper_size = cube_size;
        }

        let num_entries = cube_size * cube_size * cube_size;
        let mut cube_data = vec![0.0f32; (num_entries * 3) as usize];
        generate_identity_lut3d(&mut cube_data, cube_size, 3, Lut3DOrder::FastBlue)?;
        let mut cube_img = PackedImageDesc::new(&mut cube_data[..], num_entries as usize, 1, 3)?;
        let input_to_target = get_input_to_target_processor(baker)?;
        input_to_target.apply(&mut cube_img)?;

        // Write out the file.
        // For for maximum compatibility with other apps, we will
        // not utilize the shaper or output any metadata.

        if format_name == b"lustre" {
            let mesh_input_bit_depth = cube_dimension_len_to_lustre_bit_depth(cube_size);
            ostream.put_str("3DMESH\n");
            ostream.put_str("Mesh ");
            ostream.put_i32(mesh_input_bit_depth);
            ostream.put_str(" ");
            ostream.put_i32(cube_bit_depth);
            ostream.put_str("\n");
        }

        // `std::vector<float> shaperData(shaperSize)`: a negative size is past max_size().
        let Ok(shaper_len) = usize::try_from(shaper_size) else {
            return Err(Exception::length_error(VECTOR_CONSTRUCTOR_TOO_LONG));
        };
        let mut shaper_data = vec![0.0f32; shaper_len];
        generate_identity_lut1d(&mut shaper_data, shaper_size, 1);

        let shaper_scale = get_max_value_from_integer_bit_depth(shaper_bit_depth) as f32;

        for (i, &value) in shaper_data.iter().enumerate() {
            if i != 0 {
                ostream.put_str(" ");
            }
            let val = get_clamped_int_from_norm_float(value, shaper_scale);
            ostream.put_i32(val);
        }
        ostream.put_str("\n");

        // Write out the 3D Cube.
        let cube_scale = get_max_value_from_integer_bit_depth(cube_bit_depth) as f32;

        for i in 0..num_entries as usize {
            let r = get_clamped_int_from_norm_float(cube_data[3 * i], cube_scale);
            let g = get_clamped_int_from_norm_float(cube_data[3 * i + 1], cube_scale);
            let b = get_clamped_int_from_norm_float(cube_data[3 * i + 2], cube_scale);
            ostream.put_i32(r);
            ostream.put_str(" ");
            ostream.put_i32(g);
            ostream.put_str(" ");
            ostream.put_i32(b);
            ostream.put_str("\n");
        }
        ostream.put_str("\n");

        if format_name == b"lustre" {
            ostream.put_str("LUT8\n");
            ostream.put_str("gamma 1.0\n");
        }
        Ok(())
    }

    /// Appends, forward, the shaper LUT then the 3D LUT; inverse, the 3D LUT then the shaper.
    ///
    /// Port of `LocalFileFormat::buildFileOps` (FileFormat3DL.cpp:586-643 @ v2.5.2).
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
                return Err(Exception::new("Cannot build .3dl Op. Invalid cache type."));
            }
        };

        let new_dir = combine_transform_directions(dir, file_transform.direction());

        // If the FileTransform specifies an interpolation, and it is valid, use it. If value
        // can't be used for a type of LUT use DEFAULT. It logs a warning if a specified value
        // cannot be used by any LUT in the file. FileTransform interpolation defaults to
        // INTERP_DEFAULT.
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
#[path = "file_format_3dl_tests.rs"]
mod tests;
