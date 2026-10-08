// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The spi1d format: a 1D LUT of 1 to 3 components over a `From` range. A port of
//! `src/OpenColorIO/fileformats/FileFormatSpi1D.cpp` @ v2.5.2; its `bake` comes with the baker
//! (WP 4.8).
//!
//! ```text
//! Version 1
//! From -7.5 3.7555555555555555
//! Components 1
//! Length 4096
//! {
//!         0.031525943963232252
//!         0.045645604561056156
//!         ...
//! }
//! ```

use std::any::Any;
use std::ffi::c_ulong;
use std::sync::Arc;

use ocio_ops::exception::{Exception, Result};
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{BitDepth, TransformDirection, combine_transform_directions};
use ocio_ops::ops::lut1d::Lut1DOpData;
use ocio_ops::ops::lut1d::lut1d_op::create_lut1d_op;
use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;
use ocio_ops::ops::matrix::matrix_op::create_min_max_op;
use ocio_ops::platform::strcasecmp;
use ocio_ops::utils::cscan::{ScanArg, ScanCrt, sscanf};
use ocio_ops::utils::number_utils::{Errc, Flavor, from_chars_f32};
use ocio_ops::utils::string_utils::{c_str, starts_with, starts_with_char, trim};

use crate::config::Config;
use crate::context::Context;
use crate::fileformats::file_format_utils::{handle_lut1d, log_warning_interpolation_not_used};
use crate::fileformats::input_stream::InputStream;
use crate::lut_limits::MAX_1D_LUT_LENGTH;
use crate::transforms::file_format::{
    CachedFile, CachedFileRcPtr, FileFormat, FormatBakeCapabilities, FormatCapabilities, FormatInfo,
};
use crate::transforms::file_transform::FileTransform;

/// A spi1d file's LUT and range.
///
/// Port of `LocalCachedFile` (FileFormatSpi1D.cpp:38-48 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct LocalCachedFile {
    /// `lut`.
    pub lut: Lut1DOpData,
    /// `from_min`.
    pub from_min: f32,
    /// `from_max`.
    pub from_max: f32,
}

impl CachedFile for LocalCachedFile {}

/// The spi1d format.
///
/// Port of `LocalFileFormat` (FileFormatSpi1D.cpp:52-80 @ v2.5.2).
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalFileFormat;

/// The longest line the reader reads, its NUL included.
const MAX_LINE_SIZE: usize = 4096;

/// `At line <line>: <error> (<line content>)`, the line and its content where known (-1 for
/// none).
///
/// Port of `LocalFileFormat::ThrowErrorMessage` (FileFormatSpi1D.cpp:440-455 @ v2.5.2).
fn throw_error_message(error: &[u8], line: i32, line_content: &[u8]) -> Exception {
    let mut os = Vec::new();
    if -1 != line {
        os.extend_from_slice(format!("At line {line}: ").as_bytes());
    }
    os.extend_from_slice(error);
    if -1 != line && !line_content.is_empty() {
        os.extend_from_slice(b" (");
        os.extend_from_slice(line_content);
        os.extend_from_slice(b")");
    }
    Exception::new(os)
}

/// `NumberUtils::from_chars(s, s + 64, value)` on a 64-byte buffer: whether it parsed.
fn parse_float(s: &[u8; 64], value: &mut f32) -> bool {
    from_chars_f32(Flavor::NATIVE, s, 64, value).ec == Errc::Ok
}

impl LocalFileFormat {
    /// Reads a spi1d file: the header lines (`Version`, `From`, `Components`, `Length`) up to
    /// one starting with `{`, then a line per entry up to `}`.
    ///
    /// Port of `LocalFileFormat::read` (FileFormatSpi1D.cpp:93-311 @ v2.5.2).
    pub fn read_file(
        &self,
        istream: &mut InputStream,
        interp: Interpolation,
    ) -> Result<LocalCachedFile> {
        // Parse Header Info.
        let mut lut_size: i32 = -1;
        let mut from_min: f32 = 0.0;
        let mut from_max: f32 = 1.0;
        let mut version: i32 = -1;
        let mut components: i32 = -1;

        let mut line_buffer = Vec::new();
        let mut current_line: i32 = 0;

        // PARSE HEADER INFO
        {
            loop {
                istream.getline(&mut line_buffer, MAX_LINE_SIZE);
                current_line += 1;
                let header_line = c_str(&line_buffer);

                if starts_with(header_line, b"Version") {
                    // " " in format means any number of spaces (white space,
                    // new line, tab) including 0 of them.
                    // "Version1" is valid.
                    let scanned = sscanf(
                        ScanCrt::NATIVE,
                        header_line,
                        b"Version %d",
                        &mut [ScanArg::Int(&mut version)],
                    );
                    if scanned != 1 {
                        return Err(throw_error_message(
                            b"Invalid 'Version' Tag",
                            current_line,
                            header_line,
                        ));
                    } else if version != 1 {
                        return Err(throw_error_message(
                            b"Only format version 1 supported",
                            current_line,
                            header_line,
                        ));
                    }
                } else if starts_with(header_line, b"From") {
                    let mut from_min_s = [0u8; 64];
                    let mut from_max_s = [0u8; 64];
                    let scanned = sscanf(
                        ScanCrt::NATIVE,
                        header_line,
                        b"From %63s %63s",
                        &mut [ScanArg::Str(&mut from_min_s), ScanArg::Str(&mut from_max_s)],
                    );
                    if scanned != 2 {
                        return Err(throw_error_message(
                            b"Invalid 'From' Tag",
                            current_line,
                            header_line,
                        ));
                    }
                    let from_min_ok = parse_float(&from_min_s, &mut from_min);
                    let from_max_ok = parse_float(&from_max_s, &mut from_max);

                    if !from_min_ok || !from_max_ok {
                        return Err(throw_error_message(
                            b"Invalid 'From' Tag",
                            current_line,
                            header_line,
                        ));
                    }
                } else if starts_with(header_line, b"Components") {
                    let scanned = sscanf(
                        ScanCrt::NATIVE,
                        header_line,
                        b"Components %d",
                        &mut [ScanArg::Int(&mut components)],
                    );
                    if scanned != 1 {
                        return Err(throw_error_message(
                            b"Invalid 'Components' Tag",
                            current_line,
                            header_line,
                        ));
                    }
                } else if starts_with(header_line, b"Length") {
                    let scanned = sscanf(
                        ScanCrt::NATIVE,
                        header_line,
                        b"Length %d",
                        &mut [ScanArg::Int(&mut lut_size)],
                    );
                    if scanned != 1 {
                        return Err(throw_error_message(
                            b"Invalid 'Length' Tag",
                            current_line,
                            header_line,
                        ));
                    }
                }

                if !(istream.good() && !starts_with_char(header_line, b'{')) {
                    break;
                }
            }
        }

        if version == -1 {
            return Err(throw_error_message(
                b"Could not find 'Version' Tag",
                -1,
                b"",
            ));
        }
        if lut_size == -1 {
            return Err(throw_error_message(b"Could not find 'Length' Tag", -1, b""));
        }
        if lut_size < 2 || i64::from(lut_size) > MAX_1D_LUT_LENGTH as i64 {
            return Err(throw_error_message(
                format!("'Length' must be between 2 and {MAX_1D_LUT_LENGTH}").as_bytes(),
                -1,
                b"",
            ));
        }
        if components == -1 {
            return Err(throw_error_message(
                b"Could not find 'Components' Tag",
                -1,
                b"",
            ));
        }
        if !(0..=3).contains(&components) {
            return Err(throw_error_message(b"Components must be [1,2,3]", -1, b""));
        }

        let mut lut1d = Lut1DOpData::new(lut_size as c_ulong)?;
        if Lut1DOpData::is_valid_interpolation(interp) {
            lut1d.set_interpolation(interp);
        }

        lut1d.set_file_output_bit_depth(BitDepth::F32);
        {
            let lut_array = lut1d.get_array_mut().get_values_mut();
            let mut i: usize = 0;

            istream.getline(&mut line_buffer, MAX_LINE_SIZE);
            current_line += 1;

            let mut line_count: i32 = 0;

            let mut values: Vec<f32> = Vec::new();

            while istream.good() {
                let line = trim(c_str(&line_buffer));
                if strcasecmp(line, b"}").is_eq() {
                    break;
                }

                if !line.is_empty() {
                    values.clear();

                    let mut input_lut = [[0u8; 64]; 4];
                    let [a, b, c, d] = &mut input_lut;
                    let scanned = sscanf(
                        ScanCrt::NATIVE,
                        &line_buffer,
                        b"%63s %63s %63s %63s",
                        &mut [
                            ScanArg::Str(a),
                            ScanArg::Str(b),
                            ScanArg::Str(c),
                            ScanArg::Str(d),
                        ],
                    );
                    if scanned != components {
                        return Err(throw_error_message(
                            b"Malformed LUT line",
                            current_line,
                            line,
                        ));
                    }

                    if line_count >= lut_size {
                        return Err(throw_error_message(
                            b"Too many entries found",
                            current_line,
                            b"",
                        ));
                    }

                    values.resize(components as usize, 0.0);

                    for (k, value) in values.iter_mut().enumerate() {
                        let mut v = f32::NAN;
                        if !parse_float(&input_lut[k], &mut v) {
                            return Err(throw_error_message(
                                b"Malformed LUT line",
                                current_line,
                                line,
                            ));
                        }

                        *value = v;
                    }

                    // If 1 component is specified, use x1 x1 x1.
                    if components == 1 {
                        lut_array[i] = values[0];
                        lut_array[i + 1] = values[0];
                        lut_array[i + 2] = values[0];
                        i += 3;
                        line_count += 1;
                    }
                    // If 2 components are specified, use x1 x2 0.0.
                    else if components == 2 {
                        lut_array[i] = values[0];
                        lut_array[i + 1] = values[1];
                        lut_array[i + 2] = 0.0;
                        i += 3;
                        line_count += 1;
                    }
                    // If 3 component is specified, use x1 x2 x3.
                    else if components == 3 {
                        lut_array[i] = values[0];
                        lut_array[i + 1] = values[1];
                        lut_array[i + 2] = values[2];
                        i += 3;
                        line_count += 1;
                    }

                    // No other case, components is in [1..3].
                }

                istream.getline(&mut line_buffer, MAX_LINE_SIZE);
                current_line += 1;
            }

            if line_count != lut_size {
                return Err(throw_error_message(
                    b"Not enough entries found",
                    current_line,
                    b"",
                ));
            }
        }

        Ok(LocalCachedFile {
            lut: lut1d,
            from_min,
            from_max,
        })
    }
}

impl FileFormat for LocalFileFormat {
    /// Port of `LocalFileFormat::getFormatInfo` (FileFormatSpi1D.cpp:82-90 @ v2.5.2).
    fn format_info(&self) -> Vec<FormatInfo> {
        vec![FormatInfo::new(
            "spi1d",
            "spi1d",
            FormatCapabilities::READ | FormatCapabilities::BAKE,
            FormatBakeCapabilities::LUT_1D,
        )]
    }

    /// Port of `LocalFileFormat::read` (FileFormatSpi1D.cpp:93-311 @ v2.5.2).
    fn read(
        &self,
        istream: &mut InputStream,
        _file_name: &[u8],
        interp: Interpolation,
    ) -> Result<CachedFileRcPtr> {
        Ok(Arc::new(self.read_file(istream, interp)?))
    }

    /// Appends the range's op and the LUT's, in the direction `dir` combined with the
    /// transform's (the LUT first, inverted, for inverse); the transform's interpolation
    /// applies when the LUT allows it, else a warning.
    ///
    /// Port of `LocalFileFormat::buildFileOps` (FileFormatSpi1D.cpp:390-438 @ v2.5.2).
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
        let Some(cached_file) = any.downcast_ref::<LocalCachedFile>() else {
            // This should never happen.
            return Err(Exception::new("Cannot build Spi1D Op. Invalid cache type."));
        };

        let new_dir = combine_transform_directions(dir, file_transform.direction());

        let from_min = f64::from(cached_file.from_min);
        let from_max = f64::from(cached_file.from_max);
        let min = [from_min, from_min, from_min];
        let max = [from_max, from_max, from_max];

        let file_interp = file_transform.interpolation();
        let mut file_interp_used = false;
        let lut = handle_lut1d(Some(&cached_file.lut), file_interp, &mut file_interp_used)
            .expect("the file's LUT");

        if !file_interp_used {
            log_warning_interpolation_not_used(file_interp, file_transform);
        }

        match new_dir {
            TransformDirection::Forward => {
                create_min_max_op(ops, &min, &max, TransformDirection::Forward)?;
                create_lut1d_op(ops, lut, TransformDirection::Forward);
            }
            TransformDirection::Inverse => {
                create_lut1d_op(ops, lut, TransformDirection::Inverse);
                create_min_max_op(ops, &min, &max, TransformDirection::Inverse)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "file_format_spi1d_tests.rs"]
mod tests;
