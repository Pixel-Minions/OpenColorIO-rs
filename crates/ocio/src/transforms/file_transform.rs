// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The file transform: a port of the class in `src/OpenColorIO/transforms/FileTransform.cpp`
//! @ v2.5.2, and of its format queries (`GetNumFormats`, `GetFormatNameByIndex`,
//! `GetFormatExtensionByIndex`, `IsFormatExtensionSupported`) over the format registry
//! (`file_format.rs`). The file loading and `BuildFileTransformOps` come with WP 4.1b-c, its
//! `CollectContextVariables` with WP 3.2a.

use std::fmt;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::open_color_types::{
    CdlStyle, TransformDirection, cdl_style_to_string, interpolation_to_string,
    transform_direction_to_string,
};
use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;
use ocio_ops::utils::string_utils::c_str;

use crate::transform::{put_c_str, validate_direction};
use crate::transforms::file_format::{FormatCapabilities, FormatInfo, FormatRegistry};

/// A transform read from a file (a LUT, a CDL, a CLF or CTF, ...), by path: relative paths are
/// found through the config's search path. For the CDL formats, the CCC ID picks the CDL and
/// the CDL style its clamping; for LUTs, the interpolation.
///
/// A copy is upstream's `createEditableCopy`. Upstream gives the class no `equals`.
///
/// Port of `FileTransform` and its `Impl` (include/OpenColorIO/OpenColorTransforms.h:
/// 1109-1178, src/OpenColorIO/transforms/FileTransform.cpp:28-144 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct FileTransform {
    /// `m_dir`.
    dir: TransformDirection,
    /// `m_interp`.
    interp: Interpolation,
    /// `m_src`.
    src: Vec<u8>,
    /// `m_cccid`.
    cccid: Vec<u8>,
    /// `m_cdlStyle`.
    cdl_style: CdlStyle,
}

impl Default for FileTransform {
    fn default() -> Self {
        Self::new()
    }
}

impl FileTransform {
    /// A forward transform without a file, with the default interpolation and CDL style.
    ///
    /// Port of `FileTransform::Create` and `Impl` (FileTransform.cpp:28-59 @ v2.5.2).
    #[doc(alias = "Create")]
    pub fn new() -> FileTransform {
        FileTransform {
            dir: TransformDirection::Forward,
            interp: Interpolation::Default,
            src: Vec::new(),
            cccid: Vec::new(),
            cdl_style: CdlStyle::TRANSFORM_DEFAULT,
        }
    }

    /// Port of `FileTransform::getDirection` (FileTransform.cpp:74-77 @ v2.5.2).
    #[doc(alias = "getDirection")]
    pub fn direction(&self) -> TransformDirection {
        self.dir
    }

    /// Port of `FileTransform::setDirection` (FileTransform.cpp:79-82 @ v2.5.2).
    #[doc(alias = "setDirection")]
    pub fn set_direction(&mut self, dir: TransformDirection) {
        self.dir = dir;
    }

    /// Checks the direction ("FileTransform validation failed: " and its error), then that the
    /// path is set. The interpolation isn't checked: version 1 configs use `unknown`.
    ///
    /// Port of `FileTransform::validate` (FileTransform.cpp:84-104 @ v2.5.2).
    pub fn validate(&self) -> Result<()> {
        if let Err(ex) = validate_direction(self.dir) {
            return Err(Exception::new(format!(
                "FileTransform validation failed: {}",
                ex.message()
            )));
        }

        if self.src.is_empty() {
            return Err(Exception::new("FileTransform: empty file path"));
        }

        // NB: Not validating interpolation since v1 configs such as the spi examples use
        // interpolation=unknown.  So that is a legal usage, even if it makes no sense.
        Ok(())
    }

    /// The file's path.
    ///
    /// Port of `FileTransform::getSrc` (FileTransform.cpp:106-109 @ v2.5.2).
    #[doc(alias = "getSrc")]
    pub fn src(&self) -> &[u8] {
        &self.src
    }

    /// Sets the file's path, up to its first NUL (a C string).
    ///
    /// Port of `FileTransform::setSrc` (FileTransform.cpp:111-114 @ v2.5.2). Its null pointer,
    /// which sets an empty path, is an empty slice here.
    #[doc(alias = "setSrc")]
    pub fn set_src(&mut self, src: impl AsRef<[u8]>) {
        self.src = c_str(src.as_ref()).to_vec();
    }

    /// The CCC ID: the ID of a CDL in the file, or its index as a string; empty for the first.
    ///
    /// Port of `FileTransform::getCCCId` (FileTransform.cpp:116-119 @ v2.5.2).
    #[doc(alias = "getCCCId")]
    pub fn ccc_id(&self) -> &[u8] {
        &self.cccid
    }

    /// Sets the CCC ID, up to its first NUL.
    ///
    /// Port of `FileTransform::setCCCId` (FileTransform.cpp:121-124 @ v2.5.2).
    #[doc(alias = "setCCCId")]
    pub fn set_ccc_id(&mut self, cccid: impl AsRef<[u8]>) {
        self.cccid = c_str(cccid.as_ref()).to_vec();
    }

    /// The clamping of the CDL formats' transforms.
    ///
    /// Port of `FileTransform::getCDLStyle` (FileTransform.cpp:126-129 @ v2.5.2).
    #[doc(alias = "getCDLStyle")]
    pub fn cdl_style(&self) -> CdlStyle {
        self.cdl_style
    }

    /// Port of `FileTransform::setCDLStyle` (FileTransform.cpp:131-134 @ v2.5.2).
    #[doc(alias = "setCDLStyle")]
    pub fn set_cdl_style(&mut self, style: CdlStyle) {
        self.cdl_style = style;
    }

    /// The interpolation the LUT formats are asked to use.
    ///
    /// Port of `FileTransform::getInterpolation` (FileTransform.cpp:136-139 @ v2.5.2).
    #[doc(alias = "getInterpolation")]
    pub fn interpolation(&self) -> Interpolation {
        self.interp
    }

    /// Port of `FileTransform::setInterpolation` (FileTransform.cpp:141-144 @ v2.5.2).
    #[doc(alias = "setInterpolation")]
    pub fn set_interpolation(&mut self, interp: Interpolation) {
        self.interp = interp;
    }

    /// Writes the transform's text to `os`: the CCC ID only when it is set, the CDL style only
    /// when it isn't the default.
    ///
    /// Port of `operator<<(std::ostream &, const FileTransform &)` (FileTransform.cpp:166-185 @
    /// v2.5.2).
    pub(crate) fn write_text(&self, os: &mut OStringStream) {
        os.put_str("<FileTransform ");
        os.put_str("direction=");
        os.put_str(transform_direction_to_string(self.direction()));
        os.put_str(", interpolation=");
        os.put_str(interpolation_to_string(self.interpolation()));
        os.put_str(", src=");
        put_c_str(os, self.src());
        let cccid = self.ccc_id();
        if !cccid.is_empty() {
            os.put_str(", cccid=");
            put_c_str(os, self.ccc_id());
        }
        let cdl_style = self.cdl_style();
        if cdl_style != CdlStyle::TRANSFORM_DEFAULT {
            os.put_str(", cdl_style=");
            os.put_str(cdl_style_to_string(cdl_style));
        }
        os.put_str(">");
    }
}

impl FileTransform {
    /// The number of format names a file transform reads.
    ///
    /// Port of `FileTransform::GetNumFormats` (FileTransform.cpp:146-149 @ v2.5.2).
    #[doc(alias = "GetNumFormats")]
    pub fn num_formats() -> i32 {
        FormatRegistry::instance().num_formats(FormatCapabilities::READ)
    }

    /// The name of the read format at `index`; `""` outside the list.
    ///
    /// Port of `FileTransform::GetFormatNameByIndex` (FileTransform.cpp:151-154 @ v2.5.2).
    #[doc(alias = "GetFormatNameByIndex")]
    pub fn format_name_by_index(index: i32) -> &'static [u8] {
        FormatRegistry::instance()
            .format_name_by_index(FormatCapabilities::READ, index)
            .as_bytes()
    }

    /// The extension of the read format at `index`; `""` outside the list.
    ///
    /// Port of `FileTransform::GetFormatExtensionByIndex` (FileTransform.cpp:156-159 @
    /// v2.5.2).
    #[doc(alias = "GetFormatExtensionByIndex")]
    pub fn format_extension_by_index(index: i32) -> &'static [u8] {
        FormatRegistry::instance()
            .format_extension_by_index(FormatCapabilities::READ, index)
            .as_bytes()
    }

    /// Whether a format reads files of `extension`, ignoring case and a leading `.`.
    ///
    /// Port of `FileTransform::IsFormatExtensionSupported` (FileTransform.cpp:161-164 @
    /// v2.5.2).
    #[doc(alias = "IsFormatExtensionSupported")]
    pub fn is_format_extension_supported(extension: impl AsRef<[u8]>) -> bool {
        FormatRegistry::instance().is_format_extension_supported(extension)
    }

    /// Every format's names, extensions and capabilities, in the registry's order (owner
    /// decision P4-5).
    pub fn formats() -> Vec<FormatInfo> {
        FormatRegistry::instance().format_infos()
    }
}

impl fmt::Display for FileTransform {
    /// `<FileTransform direction=<dir>, interpolation=<interp>, src=<src>[, cccid=<id>][,
    /// cdl_style=<style>]>`.
    ///
    /// Port of `operator<<(std::ostream &, const FileTransform &)` (FileTransform.cpp:166-185 @
    /// v2.5.2), on a new stream.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_text(&mut os);
        f.write_str(&os.to_string_lossy())
    }
}

#[cfg(test)]
#[path = "file_transform_tests.rs"]
mod tests;
