// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Houdini `.lut` format: a 1D LUT (type `C`), a 3D LUT (red fastest), or a 3D LUT with a
//! 1D prelut. A port of `src/OpenColorIO/fileformats/FileFormatHDL.cpp` @ v2.5.2.
//!
//! ```text
//! Version     3
//! Format      any
//! Type        3D+1D
//! From        0.005478 14.080103
//! To          0 1
//! Black       0
//! White       1
//! Length      2 10
//! LUT:
//! Pre {
//!     0.994922
//!     ...
//! }
//! 3D {
//!     0.093776 0.093776 0.093776
//!     ...
//! }
//! ```

use std::any::Any;
use std::collections::BTreeMap;
use std::ffi::c_ulong;
use std::sync::Arc;

use ocio_ops::cfmt::{Crt, FloatField, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::image_desc::PackedImageDesc;
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{BitDepth, TransformDirection, combine_transform_directions};
use ocio_ops::ops::lut1d::Lut1DOpData;
use ocio_ops::ops::lut1d::lut1d_op::{
    create_lut1d_op, generate_identity_lut1d, generate_linear_scale_lut1d,
};
use ocio_ops::ops::lut3d::lut3d_op::{Lut3DOrder, create_lut3d_op, generate_identity_lut3d};
use ocio_ops::ops::lut3d::lut3d_op_data::{Interpolation, Lut3DOpData, MAX_3D_LUT_LENGTH};
use ocio_ops::ops::matrix::matrix_op::create_min_max_op_f32;
use ocio_ops::parse_utils::{string_to_float, string_to_int};
use ocio_ops::utils::number_utils::{Errc, Flavor, from_chars_f32};
use ocio_ops::utils::string_utils::{lower, split_by_white_spaces, trim};

use crate::baker::Baker;
use crate::baking_utils::{
    LinuxResize, bake_buffer, cube_entries, cube_values, get_input_to_shaper_processor,
    get_input_to_target_processor, get_shaper_range, get_shaper_to_target_processor,
};
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

/// The header's lines: each line's first word, lower-cased, and the words after it.
///
/// Port of `StringToStringVecMap` (FileFormatHDL.cpp:51 @ v2.5.2).
type StringToStringVecMap = BTreeMap<Vec<u8>, Vec<Vec<u8>>>;

/// The LUT blocks' values, by block name.
///
/// Port of `StringToFloatVecMap` (FileFormatHDL.cpp:52 @ v2.5.2).
type StringToFloatVecMap = BTreeMap<Vec<u8>, Vec<f32>>;

/// Reads the header lines (lower-cased, split into words, the first word the key) up to the
/// `LUT:` line; a key seen twice keeps its last line.
///
/// Port of `readHeaders` (FileFormatHDL.cpp:54-78 @ v2.5.2).
fn read_headers(headers: &mut StringToStringVecMap, istream: &mut InputStream) {
    let mut line = Vec::new();
    while nextline(istream, &mut line) {
        // Remove trailing/leading whitespace, lower-case and split into words
        let mut chunks = split_by_white_spaces(&lower(trim(&line)));

        // Skip empty lines
        if chunks.is_empty() {
            continue;
        }

        // Stop looking for headers at the "LUT:" line
        if chunks[0] == b"lut:" {
            break;
        }

        // Use first index as key, and remove it from the value
        let key = chunks.remove(0);
        headers.insert(key, chunks);
    }
}

/// The values of the header line `key`: `'<key>' line not found` without one, and
/// `Incorrect number of chunks (...)` with fewer than `min_vals` or more than `max_vals`.
///
/// Port of `findHeaderItem` (FileFormatHDL.cpp:84-132 @ v2.5.2), both overloads.
fn find_header_item(
    headers: &StringToStringVecMap,
    key: &str,
    min_vals: usize,
    max_vals: usize,
) -> Result<Vec<Vec<u8>>> {
    // Error if key is not found
    let Some(value) = headers.get(key.as_bytes()) else {
        return Err(Exception::new(format!("'{key}' line not found")));
    };

    // Error if incorrect number of values is found
    if value.len() < min_vals || value.len() > max_vals {
        let mut os = format!(
            "Incorrect number of chunks ({}) after '{key}' line, expected ",
            value.len()
        );
        if min_vals == max_vals {
            os.push_str(&min_vals.to_string());
        } else {
            os.push_str(&format!("between {min_vals} and {max_vals}"));
        }
        return Err(Exception::new(os));
    }

    Ok(value.clone())
}

/// Reads the LUT blocks after the header, word by word: a lone `{` opens the `3d` block, a
/// word then `{` opens the block of that (lower-cased) name, `}` closes it, and the words in
/// between are its values.
///
/// Port of `readLuts` (FileFormatHDL.cpp:140-220 @ v2.5.2).
fn read_luts(istream: &mut InputStream, lut_values: &mut StringToFloatVecMap) -> Result<()> {
    // State variables
    let mut inlut = false;
    let mut lutname: Vec<u8> = Vec::new();

    let mut word = Vec::new();

    loop {
        istream.extract_word(&mut word);
        if istream.fail() {
            break;
        }
        if !inlut {
            if word == b"{" {
                // Lone "{" is for a 3D
                inlut = true;
                lutname = b"3d".to_vec();
            } else {
                // Named LUT, e.g "Pre {"
                inlut = true;
                lutname = lower(&word);

                // Ensure next word is "{"
                let mut nextword = Vec::new();
                istream.extract_word(&mut nextword);
                if nextword != b"{" {
                    return Err(Exception::new(
                        [
                            b"Malformed LUT - Unknown word '".as_slice(),
                            &word,
                            b"' after LUT name '",
                            &nextword,
                            b"'",
                        ]
                        .concat(),
                    ));
                }
            }
        } else if word == b"}" {
            // end of LUT
            inlut = false;
            lutname.clear();
        } else {
            let mut v = 0.0f32;
            let result = from_chars_f32(Flavor::NATIVE, &word, word.len(), &mut v);

            if result.ec == Errc::Ok {
                // Cap per-LUT entry count: max is Max3DLUTLength^3 * 3 for a 3D LUT.
                let max = MAX_3D_LUT_LENGTH as usize;
                let values = lut_values.entry(lutname.clone()).or_default();
                if values.len() > max * max * max * 3 {
                    return Err(Exception::new(
                        [b"Too many values in ".as_slice(), &lutname, b" LUT block"].concat(),
                    ));
                }
                values.push(v);
            } else {
                // stdtod endptr still contained stuff, meaning an invalid float value
                return Err(Exception::new(
                    [
                        b"Invalid float value in ".as_slice(),
                        &lutname,
                        b" LUT, '",
                        &word,
                        b"'",
                    ]
                    .concat(),
                ));
            }
        }
    }
    Ok(())
}

/// A Houdini LUT file's header values and LUTs.
///
/// Port of `CachedFileHDL` (FileFormatHDL.cpp:226-268 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct CachedFileHdl {
    /// `hdlversion`.
    pub hdlversion: Vec<u8>,
    /// `hdlformat`.
    pub hdlformat: Vec<u8>,
    /// `hdltype`.
    pub hdltype: Vec<u8>,
    /// `from_min`.
    pub from_min: f32,
    /// `from_max`.
    pub from_max: f32,
    /// `to_min`.
    pub to_min: f32,
    /// `to_max`.
    pub to_max: f32,
    /// `hdlblack`.
    pub hdlblack: f32,
    /// `hdlwhite`.
    pub hdlwhite: f32,
    /// `lut1D`.
    pub lut1d: Option<Lut1DOpData>,
    /// `lut3D`.
    pub lut3d: Option<Lut3DOpData>,
}

impl Default for CachedFileHdl {
    fn default() -> Self {
        CachedFileHdl {
            hdlversion: b"unknown".to_vec(),
            hdlformat: b"unknown".to_vec(),
            hdltype: b"unknown".to_vec(),
            from_min: 0.0,
            from_max: 1.0,
            to_min: 0.0,
            to_max: 1.0,
            hdlblack: 0.0,
            hdlwhite: 1.0,
            lut1d: None,
            lut3d: None,
        }
    }
}

impl CachedFileHdl {
    /// A 1D LUT of `values`, the same on each channel.
    ///
    /// Port of `CachedFileHDL::setLUT1D` (FileFormatHDL.cpp:237-254 @ v2.5.2).
    fn set_lut1d(&mut self, values: &[f32], interp: Interpolation) -> Result<()> {
        let lut_size = values.len() as c_ulong;
        let mut lut1d = Lut1DOpData::new(lut_size)?;
        if Lut1DOpData::is_valid_interpolation(interp) {
            lut1d.set_interpolation(interp);
        }
        lut1d.set_file_output_bit_depth(BitDepth::F32);

        let lut_array = lut1d.get_array_mut().get_values_mut();
        for (i, &v) in values.iter().enumerate() {
            lut_array[3 * i] = v;
            lut_array[3 * i + 1] = v;
            lut_array[3 * i + 2] = v;
        }
        self.lut1d = Some(lut1d);
        Ok(())
    }
}

impl CachedFile for CachedFileHdl {}

/// The Houdini LUT format.
///
/// Port of `LocalFileFormat` (FileFormatHDL.cpp:271-293 @ v2.5.2).
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalFileFormat;

/// `StringToFloat` of a header value: the value, or `None` where it isn't one.
fn header_float(s: &[u8]) -> Option<f32> {
    string_to_float(s)
}

impl LocalFileFormat {
    /// Reads a Houdini LUT file: the header (`Version`, `Format`, `Type`, `From`, `To`,
    /// `Black`, `White`, `Length`) up to `LUT:`, then the LUT blocks the type needs.
    ///
    /// Port of `LocalFileFormat::read` (FileFormatHDL.cpp:307-588 @ v2.5.2). A `3D+1D` file
    /// whose `Length` line gives one size has no prelut size, where upstream reads past the
    /// sizes it read (docs/improvements.md, U-71): an error.
    pub fn read_file(
        &self,
        istream: &mut InputStream,
        interp: Interpolation,
    ) -> Result<CachedFileHdl> {
        // this shouldn't happen
        if istream.fail() {
            return Err(Exception::new(
                "file stream empty when trying to read Houdini LUT",
            ));
        }

        let mut cached_file = CachedFileHdl::default();

        let mut lut3d_ptr: Option<Lut3DOpData> = None;

        // Parse headers into key-value pairs
        let mut header_chunks = StringToStringVecMap::new();

        // Read headers, ending after the "LUT:" line
        read_headers(&mut header_chunks, istream);

        // "Version 3" - format version (currently one version number per LUT type)
        let value = find_header_item(&header_chunks, "version", 1, 1)?;
        cached_file.hdlversion = value[0].clone();

        // "Format any" - bit depth of image the LUT should be applied to (this is basically
        // ignored)
        let value = find_header_item(&header_chunks, "format", 1, 1)?;
        cached_file.hdlformat = value[0].clone();

        // "Type 3d" - type of LUT
        {
            let value = find_header_item(&header_chunks, "type", 1, 1)?;

            cached_file.hdltype = value[0].clone();
        }

        // "From 0.0 1.0" - range of input values
        {
            let value = find_header_item(&header_chunks, "from", 2, 2)?;

            let (Some(from_min), Some(from_max)) =
                (header_float(&value[0]), header_float(&value[1]))
            else {
                return Err(Exception::new(
                    [
                        b"Invalid float value(s) on 'From' line, '".as_slice(),
                        &value[0],
                        b"' and '",
                        &value[1],
                        b"'",
                    ]
                    .concat(),
                ));
            };
            cached_file.from_min = from_min;
            cached_file.from_max = from_max;
        }

        // "To 0.0 1.0" - range of values in LUT (e.g "0 255" to specify values as 8-bit
        // numbers, usually "0 1")
        {
            let value = find_header_item(&header_chunks, "to", 2, 2)?;

            let (Some(to_min), Some(to_max)) = (header_float(&value[0]), header_float(&value[1]))
            else {
                return Err(Exception::new(
                    [
                        b"Invalid float value(s) on 'To' line, '".as_slice(),
                        &value[0],
                        b"' and '",
                        &value[1],
                        b"'",
                    ]
                    .concat(),
                ));
            };
            cached_file.to_min = to_min;
            cached_file.to_max = to_max;
        }

        // "Black 0" and "White 1" - obsolete options, should be 0 and 1

        {
            let value = find_header_item(&header_chunks, "black", 1, 1)?;

            let Some(black) = header_float(&value[0]) else {
                return Err(Exception::new(
                    [
                        b"Invalid float value on 'Black' line, '".as_slice(),
                        &value[0],
                        b"'",
                    ]
                    .concat(),
                ));
            };
            cached_file.hdlblack = black;
        }

        {
            let value = find_header_item(&header_chunks, "white", 1, 1)?;

            let Some(white) = header_float(&value[0]) else {
                return Err(Exception::new(
                    [
                        b"Invalid float value on 'White' line, '".as_slice(),
                        &value[0],
                        b"'",
                    ]
                    .concat(),
                ));
            };
            cached_file.hdlwhite = white;
        }

        // Verify type is valid and supported - used to handle length sensibly, and checking
        // the LUT later
        {
            let ltype = &cached_file.hdltype;
            if ltype != b"3d" && ltype != b"3d+1d" && ltype != b"c" {
                return Err(Exception::new(
                    [b"Unsupported Houdini LUT type: '".as_slice(), ltype, b"'"].concat(),
                ));
            }
        }

        // "Length 2" or "Length 2 5" - either "[cube size]", or "[cube size] [prelut size]"
        let mut size_3d: i32 = -1;
        let mut size_prelut: i32 = -1;
        let mut size_1d: i32 = -1;

        {
            let mut lut_sizes: Vec<i32> = Vec::new();

            let value = find_header_item(&header_chunks, "length", 1, 2)?;
            for v in &value {
                let mut tmpsize: i32 = -1;
                if !string_to_int(&mut tmpsize, v, false) {
                    return Err(Exception::new(
                        [
                            b"Invalid integer on 'Length' line: '".as_slice(),
                            &value[0],
                            b"'",
                        ]
                        .concat(),
                    ));
                }
                lut_sizes.push(tmpsize);
            }

            if cached_file.hdltype == b"3d" || cached_file.hdltype == b"3d+1d" {
                // Set cube size
                size_3d = lut_sizes[0];

                if size_3d < 2 || i64::from(size_3d) > i64::from(MAX_3D_LUT_LENGTH) {
                    return Err(Exception::new(format!(
                        "3D LUT cube size must be between 2 and {MAX_3D_LUT_LENGTH}, found: \
                         {size_3d}"
                    )));
                }

                let mut lut3d = Lut3DOpData::new(lut_sizes[0] as c_ulong)?;
                if Lut3DOpData::is_valid_interpolation(interp) {
                    lut3d.set_interpolation(interp);
                }
                lut3d.set_file_output_bit_depth(BitDepth::F32);
                lut3d_ptr = Some(lut3d);
            }

            if cached_file.hdltype == b"c" {
                size_1d = lut_sizes[0];
                if size_1d < 2 || i64::from(size_1d) > MAX_1D_LUT_LENGTH as i64 {
                    return Err(Exception::new(format!(
                        "1D LUT size must be between 2 and {MAX_1D_LUT_LENGTH}, found: {size_1d}"
                    )));
                }
            }

            if cached_file.hdltype == b"3d+1d" {
                // `lut_sizes[1]`: upstream reads past the sizes when the line gives one
                // (U-71).
                let Some(&prelut) = lut_sizes.get(1) else {
                    return Err(Exception::new(
                        "A 3D+1D Houdini LUT needs a prelut size on the 'Length' line, after \
                         the cube size (the port's error where upstream reads past the sizes \
                         it read).",
                    ));
                };
                size_prelut = prelut;
                if size_prelut < 2 || i64::from(size_prelut) > MAX_1D_LUT_LENGTH as i64 {
                    return Err(Exception::new(format!(
                        "Prelut size must be between 2 and {MAX_1D_LUT_LENGTH}, found: \
                         {size_prelut}"
                    )));
                }
            }
        }

        // Read stuff after "LUT:"
        let mut lut_data = StringToFloatVecMap::new();
        read_luts(istream, &mut lut_data)?;

        if cached_file.hdltype == b"3d+1d" {
            // Read prelut, and bind onto cachedFile
            let Some(pre) = lut_data.get(b"pre".as_slice()) else {
                return Err(Exception::new("3D+1D LUT should contain Pre{} LUT section"));
            };

            if size_prelut != pre.len() as i32 {
                return Err(Exception::new(format!(
                    "Pre{{}} LUT was {} values long, expected {size_prelut} values",
                    pre.len()
                )));
            }

            cached_file.set_lut1d(pre, interp)?;
        }

        if cached_file.hdltype == b"3d" || cached_file.hdltype == b"3d+1d" {
            // Bind 3D LUT to lut3d_ptr, along with some slightly-elabourate error messages

            let Some(cube) = lut_data.get(b"3d".as_slice()) else {
                return Err(Exception::new("3D LUT section not found"));
            };

            let size_3d_cubed = size_3d * size_3d * size_3d;

            if size_3d_cubed * 3 != cube.len() as i32 {
                let foundsize = cube.len() as i32;
                let foundlines = foundsize / 3;

                return Err(Exception::new(format!(
                    "3D LUT contains incorrect number of values. Contained {foundsize} values \
                     ({foundlines} lines), expected {} values ({size_3d_cubed} lines)",
                    size_3d_cubed * 3
                )));
            }

            let mut lut3d = lut3d_ptr.take().expect("the 3D LUT of a 3D type");
            lut3d.set_array_from_red_fastest_order(cube)?;

            // Bind to cachedFile
            cached_file.lut3d = Some(lut3d);
        }

        if cached_file.hdltype == b"c" {
            // Bind simple 1D RGB LUT
            let Some(rgb) = lut_data.get(b"rgb".as_slice()) else {
                return Err(Exception::new("3D+1D LUT should contain Pre{} LUT section"));
            };

            if size_1d != rgb.len() as i32 {
                return Err(Exception::new(format!(
                    "RGB{{}} LUT was {} values long, expected {size_1d} values",
                    rgb.len()
                )));
            }

            cached_file.set_lut1d(rgb, interp)?;
        }

        Ok(cached_file)
    }
}

impl FileFormat for LocalFileFormat {
    /// Port of `LocalFileFormat::getFormatInfo` (FileFormatHDL.cpp:295-305 @ v2.5.2).
    fn format_info(&self) -> Vec<FormatInfo> {
        vec![FormatInfo::new(
            "houdini",
            "lut",
            FormatCapabilities::READ | FormatCapabilities::BAKE,
            FormatBakeCapabilities::LUT_3D
                | FormatBakeCapabilities::LUT_1D
                | FormatBakeCapabilities::LUT_1D_3D,
        )]
    }

    /// Port of `LocalFileFormat::read` (FileFormatHDL.cpp:307-588 @ v2.5.2).
    fn read(
        &self,
        istream: &mut InputStream,
        _file_name: &[u8],
        interp: Interpolation,
    ) -> Result<CachedFileRcPtr> {
        Ok(Arc::new(self.read_file(istream, interp)?))
    }

    /// Bakes `baker`'s transform: a 1D LUT (1024 entries by default) where it has no channel
    /// crosstalk, else a 3D LUT (64 per side by default), after a prelut (1024 by default) when
    /// there is a shaper space.
    ///
    /// Port of `LocalFileFormat::bake` (FileFormatHDL.cpp:590-823 @ v2.5.2).
    fn bake(&self, baker: &Baker, format_name: &[u8], ostream: &mut OStringStream) -> Result<()> {
        if format_name != b"houdini" {
            let mut os = OStringStream::new(Crt::NATIVE);
            os.put_str("Unknown hdl format name, '");
            os.put_bytes(format_name);
            os.put_str("'.");
            return Err(Exception::new(os.into_bytes()));
        }

        // setup the floating point precision
        ostream.float_field = FloatField::Fixed;
        ostream.precision = 6;

        // Default sizes
        const DEFAULT_SHAPER_SIZE: i32 = 1024;
        // MPlay produces bad results with 32^3 cube (in a way
        // that looks more quantised than even "nearest"
        // interpolation in OCIOFileTransform)
        const DEFAULT_CUBE_SIZE: i32 = 64;
        const DEFAULT_1D_SIZE: i32 = 1024;

        // Get configured sizes
        let mut cube_size = baker.cube_size();
        let mut shaper_size = baker.shaper_size();
        let mut oned_size = baker.cube_size();

        // Check defaults and cube size
        if cube_size == -1 {
            cube_size = DEFAULT_CUBE_SIZE;
        }

        // ..and same for shaper size
        if shaper_size == -1 {
            shaper_size = DEFAULT_SHAPER_SIZE;
        }

        // ..and finally, for the 1D LUT size
        if oned_size == -1 {
            oned_size = DEFAULT_1D_SIZE;
        }

        // Version numbers
        const HDL_1D: i32 = 1; // 1D LUT version number
        const HDL_3D: i32 = 2; // 3D LUT version number
        const HDL_3D1D: i32 = 3; // 3D LUT with 1D prelut

        // Get spaces from baker
        let shaper_space = baker.shaper_space();

        // Determine required LUT type
        let input_to_target = get_input_to_target_processor(baker)?;

        let required_lut = if input_to_target.has_channel_crosstalk() {
            if shaper_space.is_empty() {
                // Has crosstalk, but no prelut, so need 3D LUT
                HDL_3D
            } else {
                // Crosstalk with shaper-space
                HDL_3D1D
            }
        } else {
            // No crosstalk
            HDL_1D
        };

        // Make prelut
        let mut prelut_data: Vec<f32> = Vec::new();

        let mut from_in_start = 0.0f32;
        let mut from_in_end = 1.0f32;

        if required_lut == HDL_3D1D {
            (from_in_start, from_in_end) = get_shaper_range(baker)?;

            // Generate the identity prelut values, then apply the transform.
            // Prelut is linearly sampled from fromInStart to fromInEnd
            prelut_data = bake_buffer(i128::from(shaper_size) * 3, LinuxResize::Checked)?;
            generate_linear_scale_lut1d(
                &mut prelut_data,
                shaper_size,
                3,
                from_in_start,
                from_in_end,
            );

            let mut prelut_img =
                PackedImageDesc::new(&mut prelut_data[..], shaper_size as usize, 1, 3)?;
            let input_to_shaper = get_input_to_shaper_processor(baker)?;
            input_to_shaper.apply(&mut prelut_img)?;
        }

        // TODO: Do same "auto prelut" input-space allocation as FileFormatCSP?

        // Make 3D LUT
        let mut cube_data: Vec<f32> = Vec::new();
        if required_lut == HDL_3D || required_lut == HDL_3D1D {
            cube_data = bake_buffer(cube_values(cube_size), LinuxResize::Checked)?;

            generate_identity_lut3d(&mut cube_data, cube_size, 3, Lut3DOrder::FastRed)?;
            let mut cube_img =
                PackedImageDesc::new(&mut cube_data[..], cube_entries(cube_size), 1, 3)?;

            let cube_proc = if required_lut == HDL_3D1D {
                get_shaper_to_target_processor(baker)?
            } else {
                // No prelut, so cube goes from input-to-target
                input_to_target.clone()
            };

            cube_proc.apply(&mut cube_img)?;
        }

        // Make 1D LUT
        let mut oned_data: Vec<f32> = Vec::new();

        if required_lut == HDL_1D {
            oned_data = bake_buffer(i128::from(oned_size) * 3, LinuxResize::Checked)?;

            if !shaper_space.is_empty() {
                (from_in_start, from_in_end) = get_shaper_range(baker)?;
                generate_linear_scale_lut1d(
                    &mut oned_data,
                    oned_size,
                    3,
                    from_in_start,
                    from_in_end,
                );
            } else {
                generate_identity_lut1d(&mut oned_data, oned_size, 3);
            }

            let mut oned_img = PackedImageDesc::new(&mut oned_data[..], oned_size as usize, 1, 3)?;
            input_to_target.apply(&mut oned_img)?;
        }

        // Write the file contents
        ostream.put_str("Version\t\t");
        ostream.put_i32(required_lut);
        ostream.put_str("\n");
        ostream.put_str("Format\t\t");
        ostream.put_str("any");
        ostream.put_str("\n");

        ostream.put_str("Type\t\t");
        if required_lut == HDL_1D {
            ostream.put_str("RGB");
        }
        if required_lut == HDL_3D {
            ostream.put_str("3D");
        }
        if required_lut == HDL_3D1D {
            ostream.put_str("3D+1D");
        }
        ostream.put_str("\n");

        ostream.put_str("From\t\t");
        ostream.put_f32(from_in_start);
        ostream.put_str(" ");
        ostream.put_f32(from_in_end);
        ostream.put_str("\n");
        ostream.put_str("To\t\t");
        ostream.put_f32(0.0f32);
        ostream.put_str(" ");
        ostream.put_f32(1.0f32);
        ostream.put_str("\n");
        ostream.put_str("Black\t\t");
        ostream.put_f32(0.0f32);
        ostream.put_str("\n");
        ostream.put_str("White\t\t");
        ostream.put_f32(1.0f32);
        ostream.put_str("\n");

        if required_lut == HDL_3D1D {
            ostream.put_str("Length\t\t");
            ostream.put_i32(cube_size);
            ostream.put_str(" ");
            ostream.put_i32(shaper_size);
            ostream.put_str("\n");
        }
        if required_lut == HDL_3D {
            ostream.put_str("Length\t\t");
            ostream.put_i32(cube_size);
            ostream.put_str("\n");
        }
        if required_lut == HDL_1D {
            ostream.put_str("Length\t\t");
            ostream.put_i32(oned_size);
            ostream.put_str("\n");
        }

        ostream.put_str("LUT:\n");

        // Write prelut
        if required_lut == HDL_3D1D {
            ostream.put_str("Pre {\n");
            for i in 0..shaper_size as usize {
                // Grab green channel from RGB prelut
                ostream.put_str("\t");
                ostream.put_f32(prelut_data[i * 3 + 1]);
                ostream.put_str("\n");
            }
            ostream.put_str("}\n");
        }

        // Write "3D {" part of output of 3D+1D LUT
        if required_lut == HDL_3D1D {
            ostream.put_str("3D {\n");
        }

        // Write the slightly-different "{" without line for the 3D-only LUT
        if required_lut == HDL_3D {
            ostream.put_str(" {\n");
        }

        // Write the cube data after the "{"
        if required_lut == HDL_3D || required_lut == HDL_3D1D {
            for i in 0..cube_entries(cube_size) {
                // TODO: Original baker code clamped values to
                // 1.0, was this necessary/desirable?

                ostream.put_str("\t");
                ostream.put_f32(cube_data[3 * i]);
                ostream.put_str(" ");
                ostream.put_f32(cube_data[3 * i + 1]);
                ostream.put_str(" ");
                ostream.put_f32(cube_data[3 * i + 2]);
                ostream.put_str("\n");
            }

            // Write closing "}"
            ostream.put_str(" }\n");
        }

        // Write out channels for 1D LUT
        if required_lut == HDL_1D {
            for (name, c) in ["R {\n", "G {\n", "B {\n"].iter().zip(0..3) {
                ostream.put_str(name);
                for i in 0..oned_size as usize {
                    ostream.put_str("\t");
                    ostream.put_f32(oned_data[i * 3 + c]);
                    ostream.put_str("\n");
                }
                ostream.put_str("}\n");
            }
        }
        Ok(())
    }

    /// Appends, forward, a type `C` file's min/max op and 1D LUT, a `3D` file's 3D LUT, a
    /// `3D+1D` file's min/max op, prelut and 3D LUT; inverse, the same in reverse order.
    ///
    /// Port of `LocalFileFormat::buildFileOps` (FileFormatHDL.cpp:825-906 @ v2.5.2).
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
        let cached_file = match any.downcast_ref::<CachedFileHdl>() {
            Some(file) if file.lut1d.is_some() || file.lut3d.is_some() => file,
            _ => {
                return Err(Exception::new(
                    "Cannot build Houdini Op. Invalid cache type.",
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

        let hdltype = cached_file.hdltype.as_slice();
        let (from_min, from_max) = (cached_file.from_min, cached_file.from_max);
        let lut1d = || lut1d.clone().expect("the file's 1D LUT");
        let lut3d = || lut3d.clone().expect("the file's 3D LUT");
        match new_dir {
            TransformDirection::Forward => {
                if hdltype == b"c" {
                    create_min_max_op_f32(ops, from_min, from_max, new_dir)?;
                    create_lut1d_op(ops, lut1d(), new_dir);
                } else if hdltype == b"3d" {
                    create_lut3d_op(ops, lut3d(), new_dir);
                } else if hdltype == b"3d+1d" {
                    create_min_max_op_f32(ops, from_min, from_max, new_dir)?;
                    create_lut1d_op(ops, lut1d(), new_dir);
                    create_lut3d_op(ops, lut3d(), new_dir);
                } else {
                    return Err(Exception::new(
                        "Unhandled hdltype while creating forward ops",
                    ));
                }
            }
            TransformDirection::Inverse => {
                if hdltype == b"c" {
                    create_lut1d_op(ops, lut1d(), new_dir);
                    create_min_max_op_f32(ops, from_min, from_max, new_dir)?;
                } else if hdltype == b"3d" {
                    create_lut3d_op(ops, lut3d(), new_dir);
                } else if hdltype == b"3d+1d" {
                    create_lut3d_op(ops, lut3d(), new_dir);
                    create_lut1d_op(ops, lut1d(), new_dir);
                    create_min_max_op_f32(ops, from_min, from_max, new_dir)?;
                } else {
                    return Err(Exception::new(
                        "Unhandled hdltype while creating reverse ops",
                    ));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "file_format_hdl_tests.rs"]
mod tests;
