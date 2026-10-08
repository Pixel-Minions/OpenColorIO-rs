// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The file transform: a port of the class in `src/OpenColorIO/transforms/FileTransform.cpp`
//! @ v2.5.2, and of its format queries (`GetNumFormats`, `GetFormatNameByIndex`,
//! `GetFormatExtensionByIndex`, `IsFormatExtensionSupported`) over the format registry
//! (`file_format.rs`), and of the loading of its file: `getLutData`, `LoadFileUncached`, the
//! file cache (`GetCachedFileAndFormat`, `ClearFileTransformCaches`) and
//! `BuildFileTransformOps`. Its `CollectContextVariables` comes with WP 3.2a.

use std::fmt;
use std::sync::{Arc, LazyLock, Mutex};

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::logging::{is_debug_logging_enabled, log_debug};
use ocio_ops::op::OpVec;
use ocio_ops::op_data::OpData;
use ocio_ops::open_color_types::{
    CdlStyle, TransformDirection, cdl_style_to_string, interpolation_to_string,
    transform_direction_to_string,
};
use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;
use ocio_ops::ops::noop::{NoOpKind, create_file_no_op};
use ocio_ops::platform::strcasecmp;
use ocio_ops::utils::pystring::{self, os_path};
use ocio_ops::utils::string_utils::c_str;

use crate::caching::GenericCache;
use crate::config::Config;
use crate::context::Context;
use crate::fileformats::input_stream::{InputStream, OpenMode};
use crate::transform::{put_c_str, validate_direction};
use crate::transforms::file_format::{
    CachedFileRcPtr, FileFormat, FormatCapabilities, FormatInfo, FormatRegistry,
};

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

/// The stream of the LUT file at `filepath`, opened in `mode`.
///
/// Port of `getLutData` (FileTransform.cpp:186-221 @ v2.5.2), without its `ConfigIOProxy`
/// branch, which comes with the config's proxy (WP 3.10c): the file system's stream, whose
/// `failbit` is set when the file can't be opened.
pub(crate) fn get_lut_data(_config: &Config, filepath: &[u8], mode: OpenMode) -> InputStream {
    // Default behavior. Return file stream.
    InputStream::open_file(filepath, mode)
}

/// A context with `context`'s search path, working directory and I/O proxy, and no variable.
fn context_like(context: &Context) -> Context {
    let mut ctx = Context::new();
    ctx.set_search_path(context.search_path());
    ctx.set_working_dir(context.working_dir());
    ctx.set_config_io_proxy(context.config_io_proxy().cloned());
    ctx
}

/// Whether the file transform's path, its search through the search paths, or its CCC ID uses
/// context variables; adds those it uses to `used_context_vars`. A file that can't be found
/// counts as using them (with every variable of the search paths).
///
/// Port of `CollectContextVariables(const Config &, const Context &, const FileTransform &,
/// ContextRcPtr &)` (FileTransform.cpp:224-307 @ v2.5.2).
pub(crate) fn collect_context_variables(
    _config: &Config,
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
    let mut ctx_filename = context_like(context);

    let resolved_string = context.resolve_string_var_with_used(src, &mut ctx_filename);
    if c_str(&resolved_string) != src {
        found_context_vars = true;
        used_context_vars.add_string_vars(&ctx_filename);
    }

    // We want to determine if any context vars are needed to resolve the filename. Currently,
    // resolveFileLocation returns all usedContextVars in the search_path, regardless of
    // whether they are needed for the given file. The work-around is to compare the resolved
    // location with and without using the environment -- if they are the same, it means the
    // environment was not used. So we create an empty context for this purpose.
    let empty_context = context_like(context);

    // Used to collect the context variables needed to resolve the search_path. Note that this
    // may contain some variables that are not actually used.
    let mut ctx_filepath = context_like(context);

    let same_location = context
        .resolve_file_location_with_used(&resolved_string, &mut ctx_filepath)
        .and_then(|resolved_filename| {
            let other = empty_context.resolve_file_location(&resolved_string)?;
            Ok(c_str(&resolved_filename) == c_str(&other))
        });
    match same_location {
        Ok(true) => {}
        Ok(false) => {
            found_context_vars = true;
            used_context_vars.add_string_vars(&ctx_filepath);
        }
        Err(_) => {
            // It could throw if the file does not exist. That's not the mandate of the method
            // to report that kind of problem. To be safe, it returns true i.e. there is a
            // context variable.
            found_context_vars = true;
            used_context_vars.add_string_vars(&ctx_filepath);
        }
    }

    // Check if the CCCID is using a context variable and add it to the context if that's the
    // case.
    let mut ctx_cccid = Context::new();
    let cccid = c_str(tr.ccc_id());
    let resolved_cccid = context.resolve_string_var_with_used(cccid, &mut ctx_cccid);
    if c_str(&resolved_cccid) != cccid {
        found_context_vars = true;
        used_context_vars.add_string_vars(&ctx_cccid);
    }

    found_context_vars
}

/// Reads `filepath` with the first format that can: the formats of its extension in
/// registration order, then every other format in registration order. Each read opens the
/// file anew, in the format's mode.
///
/// Port of `LoadFileUncached` (FileTransform.cpp:595-776 @ v2.5.2). Upstream catches every
/// `std::exception` of a read; the port's readers return their errors.
fn load_file_uncached(
    filepath: &[u8],
    interp: Interpolation,
    config: &Config,
) -> Result<(&'static dyn FileFormat, CachedFileRcPtr)> {
    log_debug([b"**\nOpening ".as_slice(), filepath].concat());

    // Try the initial format.
    let mut primary_error_text = b"\n".to_vec(); // Add a separator for the first reader error.

    let (_root, extension) = os_path::splitext(filepath);
    // remove the leading '.'
    let extension = pystring::replace(&extension, b".", b"", 1);

    let format_registry = FormatRegistry::instance();

    let possible_formats = format_registry.file_formats_for_extension(&extension);
    for &try_format in &possible_formats {
        match open_and_read(try_format, filepath, interp, config) {
            Ok(cached_file) => {
                if is_debug_logging_enabled() {
                    log_debug(format!("    Loaded primary format {}\n", try_format.name()));
                }
                return Ok((try_format, cached_file));
            }
            Err(e) => {
                primary_error_text.extend_from_slice(b"    '");
                primary_error_text.extend_from_slice(try_format.name().as_bytes());
                primary_error_text.extend_from_slice(b"' failed with: ");
                primary_error_text.extend_from_slice(e.what());

                if is_debug_logging_enabled() {
                    log_debug(
                        [
                            b"    Failed primary format ".as_slice(),
                            try_format.name().as_bytes(),
                            b":  ",
                            e.what(),
                        ]
                        .concat(),
                    );
                }
            }
        }
    }

    // If this fails, try all other formats
    for findex in 0..format_registry.num_raw_formats() {
        let alt_format = format_registry
            .raw_format_by_index(findex)
            .expect("an index of the registry");

        // Do not try primary formats twice.
        if possible_formats
            .iter()
            .any(|&f| std::ptr::addr_eq(f, alt_format))
        {
            continue;
        }

        match open_and_read(alt_format, filepath, interp, config) {
            Ok(cached_file) => {
                if is_debug_logging_enabled() {
                    log_debug(format!("    Loaded alt format {}", alt_format.name()));
                }
                return Ok((alt_format, cached_file));
            }
            Err(e) => {
                if is_debug_logging_enabled() {
                    log_debug(
                        [
                            b"    Failed alt format ".as_slice(),
                            alt_format.name().as_bytes(),
                            b":  ",
                            e.what(),
                        ]
                        .concat(),
                    );
                }
            }
        }
    }

    // No formats succeeded. Error out with a sensible message.
    let mut os = Vec::new();
    os.extend_from_slice(b"The specified transform file '");
    os.extend_from_slice(filepath);
    os.extend_from_slice(b"' could not be loaded.\n");
    os.extend_from_slice(b"All formats have been tried. ");

    if is_debug_logging_enabled() {
        os.extend_from_slice(b"(Refer to debug log for errors from all formats.) ");
    } else {
        os.extend_from_slice(b"(Enable debug log for errors from all formats.) ");
    }

    if !possible_formats.is_empty() {
        if possible_formats.len() == 1 {
            os.extend_from_slice(b"The format for the file's extension gave the error:\n");
        } else {
            os.extend_from_slice(b"The formats for the file's extension gave the errors:\n");
        }
        os.extend_from_slice(&primary_error_text);
    }

    Err(Exception::new(os))
}

/// Opens `filepath` in `format`'s mode and reads it with `format`: one try of
/// [`load_file_uncached`] (FileTransform.cpp:629-647 and 698-716 @ v2.5.2, whose two
/// messages are the same text).
fn open_and_read(
    format: &dyn FileFormat,
    filepath: &[u8],
    interp: Interpolation,
    config: &Config,
) -> Result<CachedFileRcPtr> {
    let mode = if format.is_binary() {
        OpenMode::Binary
    } else {
        OpenMode::Text
    };
    let mut stream = get_lut_data(config, filepath, mode);
    if !stream.good() {
        return Err(Exception::new(
            [
                b"The specified FileTransform srcfile, '".as_slice(),
                filepath,
                b"', could not be opened. Please confirm the file exists with appropriate read \
                  permissions.",
            ]
            .concat(),
        ));
    }
    format.read(&mut stream, filepath, interp)
}

/// One file's load, done once: its format and what it read, or the text of its error.
///
/// Port of `FileCacheResult` (FileTransform.cpp:782-794 @ v2.5.2); its mutex is the `Mutex`
/// around it.
#[derive(Default)]
struct FileCacheResult {
    format: Option<&'static dyn FileFormat>,
    ready: bool,
    error: bool,
    cached_file: Option<CachedFileRcPtr>,
    exception_text: Vec<u8>,
}

/// The loads of files, by path: each entry has its own lock, so a slow load doesn't block the
/// lookups of other files (loads of the same file wait for each other).
///
/// Port of `g_fileCache` (FileTransform.cpp:798-799 @ v2.5.2). Upstream makes it when the
/// library loads, and reads `OCIO_DISABLE_ALL_CACHES` then; the port makes it on first use.
static FILE_CACHE: LazyLock<GenericCache<Vec<u8>, Arc<Mutex<FileCacheResult>>>> =
    LazyLock::new(GenericCache::new);

/// The format and contents of the file at `filepath`, loaded once per path while the file
/// cache is enabled (a failed load fails again with the same text), and at every call
/// otherwise.
///
/// Port of `GetCachedFileAndFormat` (FileTransform.cpp:801-888 @ v2.5.2). Its `catch (...)`,
/// for an exception that isn't a `std::exception`, has nothing to catch here.
pub(crate) fn get_cached_file_and_format(
    filepath: &[u8],
    interp: Interpolation,
    config: &Config,
) -> Result<(&'static dyn FileFormat, CachedFileRcPtr)> {
    // Load the file cache ptr from the global map
    let result = match FILE_CACHE.lock() {
        Some(mut entries) => entries
            .entries()
            .entry(filepath.to_vec())
            .or_default()
            .clone(),
        None => Arc::default(),
    };

    // If this file has already been loaded, return the result immediately.
    let mut result = result.lock().unwrap_or_else(|e| e.into_inner());
    if !result.ready {
        result.ready = true;
        result.error = false;

        match load_file_uncached(filepath, interp, config) {
            Ok((format, cached_file)) => {
                result.format = Some(format);
                result.cached_file = Some(cached_file);
            }
            Err(e) => {
                result.error = true;
                result.exception_text = e.what().to_vec();
            }
        }
    }

    if result.error {
        return Err(Exception::new(result.exception_text.clone()));
    }
    // Upstream's checks that a load without an error gave a format and a file
    // (FileTransform.cpp:870-887) can't fail: a load gives both or an error.
    match (result.format, &result.cached_file) {
        (Some(format), Some(cached_file)) => Ok((format, cached_file.clone())),
        _ => unreachable!("a load without an error gives a format and a file"),
    }
}

/// Forgets every file load.
///
/// Port of `ClearFileTransformCaches` (FileTransform.cpp:890-893 @ v2.5.2).
pub fn clear_file_transform_caches() {
    FILE_CACHE.clear();
}

/// Appends the ops of the file `file_transform` names, in the direction `dir`: a `FileNoOp`
/// for the file, then its format's ops. A file that is still being loaded further up (one
/// that references itself, directly or not) is an error.
///
/// Port of `BuildFileTransformOps` (FileTransform.cpp:895-970 @ v2.5.2).
pub(crate) fn build_file_transform_ops(
    ops: &mut OpVec,
    config: &Config,
    context: &Context,
    file_transform: &FileTransform,
    dir: TransformDirection,
) -> Result<()> {
    let src = file_transform.src();
    if src.is_empty() {
        return Err(Exception::new("The transform file has not been specified."));
    }

    let filepath = context.resolve_file_location(src)?;

    // Verify the recursion is valid, FileNoOp is added for each file.
    for op in ops.iter() {
        if let OpData::NoOp(data) = &**op.data()
            && let NoOpKind::File(file_data) = data.kind()
        {
            // Error if file is still being loaded and is the same as the
            // one about to be loaded.
            if !file_data.get_complete() && strcasecmp(file_data.get_path(), &filepath).is_eq() {
                return Err(Exception::new(
                    [
                        b"Reference to: ".as_slice(),
                        &filepath,
                        b" is creating a recursion.",
                    ]
                    .concat(),
                ));
            }
        }
    }

    let (format, cached_file) =
        get_cached_file_and_format(&filepath, file_transform.interpolation(), config)?;

    // Add FileNoOp and keep track of it.
    create_file_no_op(ops, &filepath);
    let file_no_op = ops.last().expect("the file no-op").data().clone();

    // CTF implementation of FileFormat::buildFileOps might call
    // BuildFileTransformOps for References.
    if let Err(e) = format.build_file_ops(ops, config, context, &cached_file, file_transform, dir) {
        return Err(Exception::new(
            [
                b"The transform file: ".as_slice(),
                &filepath,
                b" failed while building ops with this error: ",
                e.what(),
            ]
            .concat(),
        ));
    }

    // File has been loaded completely. It may now be referenced again.
    if let OpData::NoOp(data) = &*file_no_op
        && let NoOpKind::File(file_data) = data.kind()
    {
        file_data.set_complete();
    }
    Ok(())
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
