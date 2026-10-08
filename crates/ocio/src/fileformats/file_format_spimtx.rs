// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The spimtx format: a 3x4 matrix of 12 floats, the offsets in 16-bit code values. A port of
//! `src/OpenColorIO/fileformats/FileFormatSpiMtx.cpp` @ v2.5.2.

use std::any::Any;
use std::sync::Arc;

use ocio_ops::exception::{Exception, Result};
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{TransformDirection, combine_transform_directions};
use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;
use ocio_ops::ops::matrix::matrix_op::create_matrix_offset_op;
use ocio_ops::parse_utils::string_vec_to_float_vec;
use ocio_ops::utils::string_utils::{split_by_white_spaces, trim};

use crate::config::Config;
use crate::context::Context;
use crate::fileformats::input_stream::InputStream;
use crate::transforms::file_format::{
    CachedFile, CachedFileRcPtr, FileFormat, FormatBakeCapabilities, FormatCapabilities, FormatInfo,
};
use crate::transforms::file_transform::FileTransform;

/// A spimtx file's matrix and offsets.
///
/// Port of `LocalCachedFile` (FileFormatSpiMtx.cpp:21-34 @ v2.5.2).
#[derive(Debug, Clone, PartialEq)]
pub struct LocalCachedFile {
    /// `m44`.
    pub m44: [f64; 16],
    /// `offset4`.
    pub offset4: [f64; 4],
}

impl CachedFile for LocalCachedFile {}

/// The spimtx format.
///
/// Port of `LocalFileFormat` (FileFormatSpiMtx.cpp:38-55 @ v2.5.2).
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalFileFormat;

/// The largest file the reader reads: a valid file holds 12 numbers.
const MAX_FILE_SIZE: usize = 1024;

/// The error of a file that isn't a spimtx file.
fn parse_error(file_name: &[u8], what: &[u8]) -> Exception {
    Exception::new(
        [
            b"Error parsing .spimtx file (".as_slice(),
            file_name,
            b"). ",
            what,
        ]
        .concat(),
    )
}

impl LocalFileFormat {
    /// Reads a spimtx file: the whole stream, which must end within 1024 bytes, then 12 floats
    /// separated by white space.
    ///
    /// Port of `LocalFileFormat::read` (FileFormatSpiMtx.cpp:66-140 @ v2.5.2).
    pub fn read_file(
        &self,
        istream: &mut InputStream,
        file_name: &[u8],
    ) -> Result<LocalCachedFile> {
        // Read the entire file (capped: a valid spimtx file contains exactly 12 floats).
        let mut file_buf = [0u8; MAX_FILE_SIZE];
        istream.read(&mut file_buf, MAX_FILE_SIZE);
        let bytes_read = istream.gcount();
        if !istream.eof() {
            return Err(parse_error(
                file_name,
                b"File is too large to be a valid .spimtx file.",
            ));
        }

        // Turn it into parts
        let line_parts = split_by_white_spaces(trim(&file_buf[..bytes_read]));

        if line_parts.len() != 12 {
            return Err(parse_error(
                file_name,
                format!(
                    "File must contain 12 float entries. {} found.",
                    line_parts.len()
                )
                .as_bytes(),
            ));
        }

        // Turn the parts into floats
        let mut float_array = Vec::new();
        if !string_vec_to_float_vec(&mut float_array, &line_parts) {
            return Err(parse_error(
                file_name,
                b"File must contain all float entries. ",
            ));
        }

        // Put the bits in the right place
        let f = |i: usize| f64::from(float_array[i]);
        Ok(LocalCachedFile {
            m44: [
                f(0),
                f(1),
                f(2),
                0.0,
                f(4),
                f(5),
                f(6),
                0.0,
                f(8),
                f(9),
                f(10),
                0.0,
                0.0,
                0.0,
                0.0,
                1.0,
            ],
            offset4: [f(3) / 65535.0, f(7) / 65535.0, f(11) / 65535.0, 0.0],
        })
    }
}

impl FileFormat for LocalFileFormat {
    /// Port of `LocalFileFormat::getFormatInfo` (FileFormatSpiMtx.cpp:57-64 @ v2.5.2).
    fn format_info(&self) -> Vec<FormatInfo> {
        vec![FormatInfo::new(
            "spimtx",
            "spimtx",
            FormatCapabilities::READ,
            FormatBakeCapabilities::NONE,
        )]
    }

    /// Port of `LocalFileFormat::read` (FileFormatSpiMtx.cpp:66-140 @ v2.5.2).
    fn read(
        &self,
        istream: &mut InputStream,
        file_name: &[u8],
        _interp: Interpolation,
    ) -> Result<CachedFileRcPtr> {
        Ok(Arc::new(self.read_file(istream, file_name)?))
    }

    /// Appends the matrix op, in the direction `dir` combined with the transform's.
    ///
    /// Port of `LocalFileFormat::buildFileOps` (FileFormatSpiMtx.cpp:142-160 @ v2.5.2).
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
            return Err(Exception::new(
                "Cannot build SpiMtx Ops. Invalid cache type.",
            ));
        };

        let new_dir = combine_transform_directions(dir, file_transform.direction());

        create_matrix_offset_op(ops, &cached_file.m44, &cached_file.offset4, new_dir);
        Ok(())
    }
}

#[cfg(test)]
#[path = "file_format_spimtx_tests.rs"]
mod tests;
