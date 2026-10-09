// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The file transform: a port of the class in `src/OpenColorIO/transforms/FileTransform.cpp`
//! @ v2.5.2. The format registry (`GetNumFormats`, `GetFormatNameByIndex`,
//! `GetFormatExtensionByIndex`, `IsFormatExtensionSupported`), the file loading and
//! `BuildFileTransformOps` come with the file formats (WP 4.1).

use std::fmt;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::open_color_types::{
    CdlStyle, TransformDirection, cdl_style_to_string, interpolation_to_string,
    transform_direction_to_string,
};
use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;
use ocio_ops::utils::string_utils::c_str;

use crate::context::Context;
use crate::transform::{put_c_str, validate_direction};

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

/// A new context with the search path, working directory and I/O proxy of `context`, and no
/// variables (`Context::Create` and its three setters).
fn context_with_paths_of(context: &Context) -> Context {
    let mut ctx = Context::new();
    ctx.set_search_path(context.search_path());
    ctx.set_working_dir(context.working_dir());
    ctx.set_config_io_proxy(context.config_io_proxy().cloned());
    ctx
}

/// Whether `tr` uses context variables, which it adds to `used_context_vars`: those its source
/// resolves with; those of the search path, when the file's location depends on them (or when
/// it can't be found: upstream then counts them, to be safe); and those of its CCC ID.
///
/// Port of `CollectContextVariables(const Config &, const Context &, const FileTransform &,
/// ContextRcPtr &)` (src/OpenColorIO/transforms/FileTransform.cpp:224-298 @ v2.5.2).
pub(crate) fn collect_file_transform_context_variables(
    context: &Context,
    tr: &FileTransform,
    used_context_vars: &mut Context,
) -> bool {
    let src = c_str(tr.src());

    if src.is_empty() {
        return false;
    }

    let mut found_context_vars = false;

    // Used to collect the context variables needed to resolve the src string itself (not
    // involving the search_path yet).
    let mut ctx_filename = context_with_paths_of(context);

    let resolved_string = context.resolve_string_var_with_used(src, &mut ctx_filename);
    if resolved_string.as_slice() != src {
        found_context_vars = true;
        used_context_vars.add_string_vars(&ctx_filename);
    }

    // We want to determine if any context vars are needed to resolve the filename. Currently,
    // resolveFileLocation returns all usedContextVars in the search_path, regardless of whether
    // they are needed for the given file.  The work-around is to compare the resolved location
    // with and without using the environment -- if they are the same, it means the environment
    // was not used. So we create an empty context for this purpose.

    let empty_context = context_with_paths_of(context);

    // Used to collect the context variables needed to resolve the search_path. Note that this
    // may contain some variables that are not actually used.
    let mut ctx_filepath = context_with_paths_of(context);

    // TODO: resolveFileLocation() tests the file existence (i.e. uses FileExists()) which is
    // useless here, and it could potentially add some performance penalty.
    let compared = context
        .resolve_file_location_with_used(&resolved_string, &mut ctx_filepath)
        .and_then(|resolved_filename| {
            Ok(resolved_filename != empty_context.resolve_file_location(&resolved_string)?)
        });
    // It could fail if the file does not exist. That's not the mandate of the method to report
    // that kind of problem.  To be safe, it returns true i.e. there is a context variable.
    if compared.unwrap_or(true) {
        found_context_vars = true;
        used_context_vars.add_string_vars(&ctx_filepath);
    }

    // Check if the CCCID is using a context variable and add it to the context if that's the
    // case.
    let mut ctx_cccid = Context::new();
    let cccid = c_str(tr.ccc_id());
    let resolved_cccid = context.resolve_string_var_with_used(cccid, &mut ctx_cccid);
    if resolved_cccid.as_slice() != cccid {
        found_context_vars = true;
        used_context_vars.add_string_vars(&ctx_cccid);
    }

    found_context_vars
}

#[cfg(test)]
#[path = "file_transform_tests.rs"]
mod tests;
