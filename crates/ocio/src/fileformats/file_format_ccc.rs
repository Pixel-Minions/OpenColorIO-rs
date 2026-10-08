// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The CCC format: an ASC CDL `ColorCorrectionCollection`. A port of
//! `src/OpenColorIO/fileformats/FileFormatCCC.cpp` @ v2.5.2.

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
use ocio_ops::open_color_types::{CdlStyle, TransformDirection, combine_transform_directions};
use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;
use ocio_ops::parse_utils::string_to_int;

use crate::config::Config;
use crate::context::Context;
use crate::fileformats::cdl::cdl_parser::{
    CDL_TAG_COLOR_CORRECTION_COLLECTION, CdlParser, CdlTransformMap,
};
use crate::fileformats::cdl::cdl_reader_helper::CdlTransformVec;
use crate::fileformats::cdl::cdl_writer;
use crate::fileformats::input_stream::InputStream;
use crate::transform::Transform;
use crate::transforms::cdl_transform::{CdlTransform, build_cdl_op};
use crate::transforms::cdl_transform::{METADATA_INPUT_DESCRIPTION, METADATA_VIEWING_DESCRIPTION};
use crate::transforms::file_format::{
    CachedFile, CachedFileRcPtr, FILEFORMAT_COLOR_CORRECTION_COLLECTION, FileFormat,
    FormatBakeCapabilities, FormatCapabilities, FormatInfo,
};
use crate::transforms::file_transform::FileTransform;
use crate::transforms::group_transform::GroupTransform;

/// A CCC file's transforms.
///
/// Port of `LocalCachedFile` (FileFormatCCC.cpp:23-49 @ v2.5.2).
#[derive(Debug, Clone, Default)]
pub struct LocalCachedFile {
    /// `m_transformMap`: the transforms with an id, by id.
    pub transform_map: CdlTransformMap,
    /// `m_transformVec`: the transforms in the file's order.
    pub transform_vec: CdlTransformVec,
    /// `m_metadata`: the descriptive element children of `<ColorCorrectionCollection>`.
    /// Descriptive elements of SOPNode and SatNode are stored in the transforms.
    pub metadata: FormatMetadataImpl,
}

impl CachedFile for LocalCachedFile {
    /// A group of the file's CDLs, in the file's order, with the file's metadata. Upstream's
    /// group shares the cached transforms; the port's holds copies (I-174).
    ///
    /// Port of `LocalCachedFile::getCDLGroup` (FileFormatCCC.cpp:32-41 @ v2.5.2).
    fn get_cdl_group(&self) -> Result<GroupTransform> {
        let mut group = GroupTransform::new();
        for cdl in &self.transform_vec {
            group.append_transform((**cdl).clone().into());
        }
        *group.format_metadata_mut() = self.metadata.clone();
        Ok(group)
    }
}

/// The CCC format.
///
/// Port of `LocalFileFormat` (FileFormatCCC.cpp:53-77 @ v2.5.2).
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalFileFormat;

impl LocalFileFormat {
    /// Reads a CCC file (or any file of the CDL parser): its color corrections.
    ///
    /// Port of `LocalFileFormat::read` (FileFormatCCC.cpp:91-105 @ v2.5.2).
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
    /// Port of `LocalFileFormat::getFormatInfo` (FileFormatCCC.cpp:79-86 @ v2.5.2).
    fn format_info(&self) -> Vec<FormatInfo> {
        vec![FormatInfo::new(
            FILEFORMAT_COLOR_CORRECTION_COLLECTION,
            "ccc",
            FormatCapabilities::READ | FormatCapabilities::WRITE,
            FormatBakeCapabilities::NONE,
        )]
    }

    /// Port of `LocalFileFormat::read` (FileFormatCCC.cpp:91-105 @ v2.5.2).
    fn read(
        &self,
        istream: &mut InputStream,
        file_name: &[u8],
        _interp: Interpolation,
    ) -> Result<CachedFileRcPtr> {
        Ok(Arc::new(self.read_file(istream, file_name)?))
    }

    /// Writes the group's CDLs, which must be at least one, under a `ColorCorrectionCollection` with the
    /// group's descriptions.
    ///
    /// Port of `LocalFileFormat::write` (FileFormatCCC.cpp:107-156 @ v2.5.2).
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
        fmt.write_start_tag_with(CDL_TAG_COLOR_CORRECTION_COLLECTION, &attributes);
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
                let Ok(Transform::Cdl(cdl)) = group.transform(i) else {
                    unreachable!("checked above");
                };
                cdl_writer::write(&mut scope_indent, cdl);
            }
        }
        fmt.write_end_tag(CDL_TAG_COLOR_CORRECTION_COLLECTION);
        Ok(())
    }

    /// Appends the ops of the color correction the file transform's cccid names, resolved in
    /// `context`: by id, else by index (an empty cccid is index 0).
    ///
    /// Port of `LocalFileFormat::buildFileOps` (FileFormatCCC.cpp:158-252 @ v2.5.2).
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
            return Err(Exception::new("Cannot build .ccc Op. Invalid cache type."));
        };

        build_cdl_file_ops(
            ops,
            config,
            context,
            &cached_file.transform_map,
            &cached_file.transform_vec,
            file_transform,
            dir,
            "ccc",
        )
    }
}

/// The ops of the color correction of a CCC's or CDL's transforms that the file transform's
/// cccid names: `buildFileOps` of both formats, whose texts differ only in the extension
/// `ext` they name.
///
/// Port of `LocalFileFormat::buildFileOps` (FileFormatCCC.cpp:175-251, and
/// FileFormatCDL.cpp:204-282 @ v2.5.2), past the cast of the cached file.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_cdl_file_ops(
    ops: &mut OpVec,
    config: &Config,
    context: &Context,
    transform_map: &CdlTransformMap,
    transform_vec: &CdlTransformVec,
    file_transform: &FileTransform,
    dir: TransformDirection,
    ext: &str,
) -> Result<()> {
    let new_dir = combine_transform_directions(dir, file_transform.direction());

    // Below this point, we should throw ExceptionMissingFile on
    // errors rather than Exception
    // This is because we've verified that the ccc file is valid,
    // at now we're only querying whether the specified cccid can
    // be found.
    //
    // Using ExceptionMissingFile enables the missing looks fallback
    // mechanism to function properly.
    // At the time ExceptionMissingFile was named, we errently assumed
    // a 1:1 relationship between files and color corrections, which is
    // not true for .ccc files.
    //
    // In a future OCIO release, it may be more appropriate to
    // rename ExceptionMissingFile -> ExceptionMissingCorrection.
    // But either way, it's what we should throw below.

    let cccid = file_transform.ccc_id().to_vec();
    let cccid = context.resolve_string_var(&cccid);

    let mut success = false;

    let file_cdl_style = file_transform.cdl_style();

    // Try to parse the cccid as a string id
    if let Some(cdl) = transform_map.get(&cccid) {
        success = true;
        build_styled_cdl_op(ops, config, cdl, file_cdl_style, new_dir)?;
    }

    // Try to parse the cccid as an integer index
    // We want to be strict, so fail if leftover chars in the parse.
    if !success {
        // Use 0 for empty string.
        let mut cccindex: i32 = 0;
        if cccid.is_empty() || string_to_int(&mut cccindex, &cccid, true) {
            let maxindex = (transform_vec.len() as i32) - 1;
            if cccindex < 0 || cccindex > maxindex {
                let mut os = OStringStream::new(Crt::NATIVE);
                os.put_str("The specified cccindex ");
                os.put_i32(cccindex);
                os.put_str(" is outside the valid range for this file [0,");
                os.put_i32(maxindex);
                os.put_str("]");
                return Err(Exception::missing_file(os.into_bytes()));
            }

            success = true;
            build_styled_cdl_op(
                ops,
                config,
                &transform_vec[cccindex as usize],
                file_cdl_style,
                new_dir,
            )?;
        }
    }

    if !success {
        let mut os = OStringStream::new(Crt::NATIVE);
        os.put_str("You must specify a valid cccid to load from the ");
        os.put_str(ext);
        os.put_str(" file");
        os.put_str(" (either by name or index). id='");
        os.put_bytes(&cccid);
        os.put_str("' ");
        os.put_str("is not found in the file, and is not parsable as an ");
        os.put_str("integer index.");
        return Err(Exception::missing_file(os.into_bytes()));
    }
    Ok(())
}

/// `BuildCDLOp` of `cdl`, of a copy with the file transform's style when that isn't the
/// default (FileFormatCCC.cpp:204-211 @ v2.5.2).
fn build_styled_cdl_op(
    ops: &mut OpVec,
    config: &Config,
    cdl: &CdlTransform,
    file_cdl_style: CdlStyle,
    new_dir: TransformDirection,
) -> Result<()> {
    if file_cdl_style != CdlStyle::TRANSFORM_DEFAULT {
        let mut cdl = cdl.clone();
        cdl.set_style(file_cdl_style);
        return build_cdl_op(ops, config, &cdl, new_dir);
    }
    build_cdl_op(ops, config, cdl, new_dir)
}

#[cfg(test)]
#[path = "file_format_ccc_tests.rs"]
pub(crate) mod tests;
