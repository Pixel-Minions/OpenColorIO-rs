// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The CDL format: an ASC CDL `ColorDecisionList`. A port of
//! `src/OpenColorIO/fileformats/FileFormatCDL.cpp` @ v2.5.2.

use std::any::Any;
use std::sync::Arc;

use ocio_formats::fileformats::xmlutils::xml_reader_utils::TAG_DESCRIPTION;
use ocio_formats::fileformats::xmlutils::xml_writer_utils::{
    Attributes, XmlFormatter, XmlScopeIndent,
};
use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::format_metadata::FormatMetadataImpl;
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::TransformDirection;
use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;

use crate::config::Config;
use crate::context::Context;
use crate::fileformats::cdl::cdl_parser::{
    CDL_TAG_COLOR_DECISION, CDL_TAG_COLOR_DECISION_LIST, CdlParser, CdlTransformMap,
};
use crate::fileformats::cdl::cdl_reader_helper::CdlTransformVec;
use crate::fileformats::cdl::cdl_writer;
use crate::fileformats::file_format_ccc::build_cdl_file_ops;
use crate::fileformats::input_stream::InputStream;
use crate::transform::Transform;
use crate::transforms::cdl_transform::{METADATA_INPUT_DESCRIPTION, METADATA_VIEWING_DESCRIPTION};
use crate::transforms::file_format::{
    CachedFile, CachedFileRcPtr, FILEFORMAT_COLOR_DECISION_LIST, FileFormat,
    FormatBakeCapabilities, FormatCapabilities, FormatInfo,
};
use crate::transforms::file_transform::FileTransform;
use crate::transforms::group_transform::GroupTransform;

/// A CDL file's transforms.
///
/// Port of `LocalCachedFile` (FileFormatCDL.cpp:46-72 @ v2.5.2).
#[derive(Debug, Clone, Default)]
pub struct LocalCachedFile {
    /// `m_transformMap`: the transforms with an id, by id.
    pub transform_map: CdlTransformMap,
    /// `m_transformVec`: the transforms in the file's order.
    pub transform_vec: CdlTransformVec,
    /// `m_metadata`: the descriptive element children of `<ColorDecisonList>`. Descriptive
    /// elements of SOPNode and SatNode are stored in the transforms.
    pub metadata: FormatMetadataImpl,
}

impl CachedFile for LocalCachedFile {
    /// A group of the file's CDLs, in the file's order, with the file's metadata. Upstream's
    /// group shares the cached transforms; the port's holds copies (I-174).
    ///
    /// Port of `LocalCachedFile::getCDLGroup` (FileFormatCDL.cpp:55-64 @ v2.5.2).
    fn get_cdl_group(&self) -> Result<GroupTransform> {
        let mut group = GroupTransform::new();
        for cdl in &self.transform_vec {
            group.append_transform((**cdl).clone().into());
        }
        *group.format_metadata_mut() = self.metadata.clone();
        Ok(group)
    }
}

/// The CDL format.
///
/// Port of `LocalFileFormat` (FileFormatCDL.cpp:76-100 @ v2.5.2).
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalFileFormat;

impl LocalFileFormat {
    /// Reads a CDL file (or any file of the CDL parser): its color corrections.
    ///
    /// Port of `LocalFileFormat::read` (FileFormatCDL.cpp:114-128 @ v2.5.2).
    pub fn read_file(
        &self,
        istream: &mut InputStream,
        file_name: &[u8],
    ) -> Result<LocalCachedFile> {
        let mut parser = CdlParser::new(file_name);
        parser.parse(istream)?;

        let mut cached_file = LocalCachedFile::default();

        parser.get_cdl_transforms(
            &mut cached_file.transform_map,
            &mut cached_file.transform_vec,
            &mut cached_file.metadata,
        )?;

        Ok(cached_file)
    }
}

impl FileFormat for LocalFileFormat {
    /// Port of `LocalFileFormat::getFormatInfo` (FileFormatCDL.cpp:102-109 @ v2.5.2).
    fn format_info(&self) -> Vec<FormatInfo> {
        vec![FormatInfo::new(
            FILEFORMAT_COLOR_DECISION_LIST,
            "cdl",
            FormatCapabilities::READ | FormatCapabilities::WRITE,
            FormatBakeCapabilities::NONE,
        )]
    }

    /// Port of `LocalFileFormat::read` (FileFormatCDL.cpp:114-128 @ v2.5.2).
    fn read(
        &self,
        istream: &mut InputStream,
        file_name: &[u8],
        _interp: Interpolation,
    ) -> Result<CachedFileRcPtr> {
        Ok(Arc::new(self.read_file(istream, file_name)?))
    }

    /// Writes the group's CDLs, which must be at least one, under a `ColorDecisionList` with the
    /// group's descriptions.
    ///
    /// Port of `LocalFileFormat::write` (FileFormatCDL.cpp:130-184 @ v2.5.2).
    fn write(
        &self,
        _config: &Config,
        _context: &Context,
        group: &GroupTransform,
        format_name: &[u8],
        ostream: &mut OStringStream,
    ) -> Result<()> {
        let num_cdl = group.num_transforms();
        if num_cdl == 0 {
            let mut os = OStringStream::new(Crt::NATIVE);
            os.put_str("Write to ");
            os.put_bytes(format_name);
            os.put_str(": there should be at least one CDL.");
            return Err(Exception::new(os.into_bytes()));
        }
        for i in 0..num_cdl {
            if !matches!(group.transform(i), Ok(Transform::Cdl(_))) {
                let mut os = OStringStream::new(Crt::NATIVE);
                os.put_str("Write to ");
                os.put_bytes(format_name);
                os.put_str(": only CDL can be written.");
                return Err(Exception::new(os.into_bytes()));
            }
        }

        let mut fmt = XmlFormatter::new(ostream);
        let attributes: Attributes = vec![(b"xmlns".to_vec(), b"urn:ASC:CDL:v1.01".to_vec())];
        fmt.write_start_tag_with(CDL_TAG_COLOR_DECISION_LIST, &attributes);
        {
            let mut scope_indent = XmlScopeIndent::new(&mut fmt);

            let mut main_desc = Vec::new();
            let mut input_desc = Vec::new();
            let mut viewing_desc = Vec::new();
            let mut sop_desc = Vec::new();
            let mut sat_desc = Vec::new();
            let metadata = group.format_metadata();
            cdl_writer::extract_cdl_metadata(
                metadata,
                &mut main_desc,
                &mut input_desc,
                &mut viewing_desc,
                &mut sop_desc,
                &mut sat_desc,
            );
            cdl_writer::write_strings(&mut scope_indent, TAG_DESCRIPTION, &main_desc);
            cdl_writer::write_strings(&mut scope_indent, METADATA_INPUT_DESCRIPTION, &input_desc);
            cdl_writer::write_strings(
                &mut scope_indent,
                METADATA_VIEWING_DESCRIPTION,
                &viewing_desc,
            );

            for i in 0..num_cdl {
                scope_indent.write_start_tag(CDL_TAG_COLOR_DECISION);
                {
                    let mut scope_indent = XmlScopeIndent::new(&mut scope_indent);
                    let Ok(Transform::Cdl(cdl)) = group.transform(i) else {
                        unreachable!("checked above");
                    };
                    cdl_writer::write(&mut scope_indent, cdl);
                }
                scope_indent.write_end_tag(CDL_TAG_COLOR_DECISION);
            }
        }
        fmt.write_end_tag(CDL_TAG_COLOR_DECISION_LIST);
        Ok(())
    }

    /// Appends the ops of the color correction the file transform's cccid names, resolved in
    /// `context`: by id, else by index (an empty cccid is index 0).
    ///
    /// Port of `LocalFileFormat::buildFileOps` (FileFormatCDL.cpp:186-283 @ v2.5.2).
    fn build_file_ops(
        &self,
        ops: &mut OpVec,
        config: &Config,
        context: &Context,
        untyped_cached_file: &CachedFileRcPtr,
        file_transform: &FileTransform,
        dir: TransformDirection,
    ) -> Result<()> {
        let any: &dyn Any = untyped_cached_file.as_ref();
        let Some(cached_file) = any.downcast_ref::<LocalCachedFile>() else {
            // This should never happen.
            return Err(Exception::new("Cannot build .cdl Op. Invalid cache type."));
        };

        build_cdl_file_ops(
            ops,
            config,
            context,
            &cached_file.transform_map,
            &cached_file.transform_vec,
            file_transform,
            dir,
            "cdl",
        )
    }
}

#[cfg(test)]
#[path = "file_format_cdl_tests.rs"]
mod tests;
