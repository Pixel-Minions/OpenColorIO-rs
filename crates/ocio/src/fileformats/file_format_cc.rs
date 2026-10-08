// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The CC format: one ASC CDL `ColorCorrection`. A port of
//! `src/OpenColorIO/fileformats/FileFormatCC.cpp` @ v2.5.2.

use std::any::Any;
use std::sync::Arc;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{CdlStyle, TransformDirection, combine_transform_directions};
use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;

use crate::config::Config;
use crate::context::Context;
use crate::fileformats::cdl::cdl_parser::CdlParser;
use crate::fileformats::cdl::cdl_reader_helper::CdlTransformRcPtr;
use crate::fileformats::cdl::cdl_writer;
use crate::fileformats::input_stream::InputStream;
use crate::transform::Transform;
use crate::transforms::cdl_transform::{CdlTransform, build_cdl_op};
use crate::transforms::file_format::{
    CachedFile, CachedFileRcPtr, FILEFORMAT_COLOR_CORRECTION, FileFormat, FormatBakeCapabilities,
    FormatCapabilities, FormatInfo,
};
use crate::transforms::file_transform::FileTransform;
use crate::transforms::group_transform::GroupTransform;
use ocio_formats::fileformats::xmlutils::xml_writer_utils::XmlFormatter;

/// A CC file's transform.
///
/// Port of `LocalCachedFile` (FileFormatCC.cpp:19-37 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct LocalCachedFile {
    /// `m_transform`.
    pub transform: CdlTransformRcPtr,
}

impl CachedFile for LocalCachedFile {
    /// A group of the file's CDL. Upstream's group shares the cached transform; the port's
    /// holds a copy (I-174).
    ///
    /// Port of `LocalCachedFile::getCDLGroup` (FileFormatCC.cpp:29-34 @ v2.5.2).
    fn get_cdl_group(&self) -> Result<GroupTransform> {
        let mut group = GroupTransform::new();
        group.append_transform((*self.transform).clone().into());
        Ok(group)
    }
}

/// The CC format.
///
/// Port of `LocalFileFormat` (FileFormatCC.cpp:43-67 @ v2.5.2).
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalFileFormat;

impl LocalFileFormat {
    /// Reads a CC file: its ColorCorrection, through the CDL parser.
    ///
    /// Port of `LocalFileFormat::read` (FileFormatCC.cpp:81-107 @ v2.5.2).
    pub fn read_file(
        &self,
        istream: &mut InputStream,
        file_name: &[u8],
    ) -> Result<LocalCachedFile> {
        let mut parser = CdlParser::new(file_name);
        let transform = match parser
            .parse(istream)
            .and_then(|()| parser.get_cdl_transform())
        {
            Ok(transform) => transform,
            Err(e) => {
                let mut os = OStringStream::new(Crt::NATIVE);
                os.put_str("Error parsing .cc file. ");
                os.put_str("Does not appear to contain a valid ASC CDL XML:");
                os.put_c_str(e.what());
                return Err(Exception::new(os.into_bytes()));
            }
        };
        if !parser.is_cc() {
            let mut os = OStringStream::new(Crt::NATIVE);
            os.put_str("File '");
            os.put_bytes(file_name);
            os.put_str("' is not a .cc file.");
            return Err(Exception::new(os.into_bytes()));
        }

        Ok(LocalCachedFile { transform })
    }
}

impl FileFormat for LocalFileFormat {
    /// Port of `LocalFileFormat::getFormatInfo` (FileFormatCC.cpp:69-76 @ v2.5.2).
    fn format_info(&self) -> Vec<FormatInfo> {
        vec![FormatInfo::new(
            FILEFORMAT_COLOR_CORRECTION,
            "cc",
            FormatCapabilities::READ | FormatCapabilities::WRITE,
            FormatBakeCapabilities::NONE,
        )]
    }

    /// Port of `LocalFileFormat::read` (FileFormatCC.cpp:81-107 @ v2.5.2).
    fn read(
        &self,
        istream: &mut InputStream,
        file_name: &[u8],
        _interp: Interpolation,
    ) -> Result<CachedFileRcPtr> {
        Ok(Arc::new(self.read_file(istream, file_name)?))
    }

    /// Writes the group's one CDL as a `ColorCorrection`.
    ///
    /// Port of `LocalFileFormat::write` (FileFormatCC.cpp:111-129 @ v2.5.2).
    fn write(
        &self,
        _config: &Config,
        _context: &Context,
        group: &GroupTransform,
        _format_name: &[u8],
        ostream: &mut OStringStream,
    ) -> Result<()> {
        if group.num_transforms() != 1 {
            return Err(Exception::new("CDL write: there should be a single CDL."));
        }
        let Ok(Transform::Cdl(cdl)) = group.transform(0) else {
            return Err(Exception::new("CDL write: only CDL can be written."));
        };

        let mut fmt = XmlFormatter::new(ostream);
        cdl_writer::write(&mut fmt, cdl);
        Ok(())
    }

    /// Appends the CDL's ops, with the file transform's CDL style if not the default, in the
    /// direction `dir` combined with the transform's.
    ///
    /// Port of `LocalFileFormat::buildFileOps` (FileFormatCC.cpp:129-159 @ v2.5.2).
    fn build_file_ops(
        &self,
        ops: &mut OpVec,
        config: &Config,
        _context: &Context,
        untyped_cached_file: &CachedFileRcPtr,
        file_transform: &FileTransform,
        dir: TransformDirection,
    ) -> Result<()> {
        let any: &dyn Any = untyped_cached_file.as_ref();
        let Some(cached_file) = any.downcast_ref::<LocalCachedFile>() else {
            // This should never happen.
            return Err(Exception::new("Cannot build .cc Op. Invalid cache type."));
        };

        let new_dir = combine_transform_directions(dir, file_transform.direction());

        let file_cdl_style = file_transform.cdl_style();
        if file_cdl_style != CdlStyle::TRANSFORM_DEFAULT {
            let mut cdl: CdlTransform = (*cached_file.transform).clone();
            cdl.set_style(file_cdl_style);
            return build_cdl_op(ops, config, &cdl, new_dir);
        }

        build_cdl_op(ops, config, &cached_file.transform, new_dir)
    }
}

#[cfg(test)]
#[path = "file_format_cc_tests.rs"]
mod tests;
