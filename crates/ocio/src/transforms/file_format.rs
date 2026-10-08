// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The file formats and their registry: a port of `FileFormat`, `FormatInfo`, the capability
//! flags and `FormatRegistry` (src/OpenColorIO/transforms/FileTransform.h:20-187,
//! FileTransform.cpp:315-594 @ v2.5.2).
//!
//! The registry holds upstream's 19 formats in upstream's order, so the lists of names and
//! extensions, and the order in which a file's formats are tried, are upstream's from the
//! start. A format whose reader isn't ported yet is [`NotPortedFormat`], which holds its
//! `getFormatInfo` and refuses to read; each format's chunk replaces it (WP 4.2-4.9).

use std::any::Any;
use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use ocio_ops::exception::{Exception, Result};
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::TransformDirection;
use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;
use ocio_ops::utils::string_utils::{c_str, lower};

use crate::config::Config;
use crate::context::Context;
use crate::fileformats::input_stream::InputStream;
use crate::transforms::file_transform::FileTransform;

/// `FILEFORMAT_CLF`.
pub const FILEFORMAT_CLF: &str = "Academy/ASC Common LUT Format";
/// `FILEFORMAT_CTF`.
pub const FILEFORMAT_CTF: &str = "Color Transform Format";
/// `FILEFORMAT_COLOR_CORRECTION`.
pub const FILEFORMAT_COLOR_CORRECTION: &str = "ColorCorrection";
/// `FILEFORMAT_COLOR_CORRECTION_COLLECTION`.
pub const FILEFORMAT_COLOR_CORRECTION_COLLECTION: &str = "ColorCorrectionCollection";
/// `FILEFORMAT_COLOR_DECISION_LIST`.
pub const FILEFORMAT_COLOR_DECISION_LIST: &str = "ColorDecisionList";

/// What a format can do: `FormatCapabilityFlags`, a bit set.
///
/// Port of `FormatCapabilityFlags` (FileTransform.h:35-41 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatCapabilities(pub u32);

impl FormatCapabilities {
    /// `FORMAT_CAPABILITY_NONE`.
    pub const NONE: FormatCapabilities = FormatCapabilities(0);
    /// `FORMAT_CAPABILITY_READ`.
    pub const READ: FormatCapabilities = FormatCapabilities(1);
    /// `FORMAT_CAPABILITY_BAKE`.
    pub const BAKE: FormatCapabilities = FormatCapabilities(2);
    /// `FORMAT_CAPABILITY_WRITE`.
    pub const WRITE: FormatCapabilities = FormatCapabilities(4);

    /// Whether every flag of `other` is set.
    pub fn contains(self, other: FormatCapabilities) -> bool {
        self.0 & other.0 == other.0
    }
}

impl std::ops::BitOr for FormatCapabilities {
    type Output = FormatCapabilities;
    fn bitor(self, rhs: FormatCapabilities) -> FormatCapabilities {
        FormatCapabilities(self.0 | rhs.0)
    }
}

/// The kinds of LUT a format bakes: `FormatBakeFlags`, a bit set.
///
/// Port of `FormatBakeFlags` (FileTransform.h:43-49 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatBakeCapabilities(pub u32);

impl FormatBakeCapabilities {
    /// `FORMAT_BAKE_CAPABILITY_NONE`.
    pub const NONE: FormatBakeCapabilities = FormatBakeCapabilities(0);
    /// `FORMAT_BAKE_CAPABILITY_3DLUT`.
    pub const LUT_3D: FormatBakeCapabilities = FormatBakeCapabilities(1);
    /// `FORMAT_BAKE_CAPABILITY_1DLUT`.
    pub const LUT_1D: FormatBakeCapabilities = FormatBakeCapabilities(2);
    /// `FORMAT_BAKE_CAPABILITY_1D_3D_LUT`.
    pub const LUT_1D_3D: FormatBakeCapabilities = FormatBakeCapabilities(4);
}

impl std::ops::BitOr for FormatBakeCapabilities {
    type Output = FormatBakeCapabilities;
    fn bitor(self, rhs: FormatBakeCapabilities) -> FormatBakeCapabilities {
        FormatBakeCapabilities(self.0 | rhs.0)
    }
}

/// A format's name, extension and capabilities. A format can declare several (a name per
/// extension).
///
/// Port of `FormatInfo` (FileTransform.h:51-62 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatInfo {
    /// `name`, globally unique (ignoring case).
    pub name: &'static str,
    /// `extension`, lower case, not unique.
    pub extension: &'static str,
    /// `capabilities`.
    pub capabilities: FormatCapabilities,
    /// `bake_capabilities`.
    pub bake_capabilities: FormatBakeCapabilities,
}

impl FormatInfo {
    /// An info.
    pub(crate) const fn new(
        name: &'static str,
        extension: &'static str,
        capabilities: FormatCapabilities,
        bake_capabilities: FormatBakeCapabilities,
    ) -> FormatInfo {
        FormatInfo {
            name,
            extension,
            capabilities,
            bake_capabilities,
        }
    }
}

/// What a format read from a file, which the format's `build_file_ops` turns into ops (it
/// downcasts it to its own type). The file cache keeps it.
///
/// Port of `CachedFile` (FileTransform.h:23-33 @ v2.5.2); `getCDLGroup` comes with the CDL
/// formats (WP 4.4).
pub trait CachedFile: Any + Send + Sync {}

/// A shared [`CachedFile`].
pub type CachedFileRcPtr = Arc<dyn CachedFile>;

/// A file format: its infos, reading, and (as their chunks port them) baking and writing.
///
/// Port of `FileFormat` (FileTransform.h:66-106, FileTransform.cpp:556-593 @ v2.5.2), so far
/// `getFormatInfo`, `read`, `buildFileOps`, `isBinary` and `getName`; `bake` and `write` come
/// with the baker (WP 4.8) and the writers.
pub trait FileFormat: Send + Sync {
    /// Port of `FileFormat::getFormatInfo`.
    fn format_info(&self) -> Vec<FormatInfo>;

    /// Reads the file `stream` holds. `original_file_name` is the file's path, which some
    /// readers parse; it may be empty when unknown.
    ///
    /// Port of `FileFormat::read` (FileTransform.h:74-79 @ v2.5.2).
    fn read(
        &self,
        stream: &mut InputStream,
        original_file_name: &[u8],
        interp: Interpolation,
    ) -> Result<CachedFileRcPtr>;

    /// Appends the ops of `cached_file`, which this format read, for `file_transform` in the
    /// direction `dir`.
    ///
    /// Port of `FileFormat::buildFileOps` (FileTransform.h:90-95 @ v2.5.2).
    fn build_file_ops(
        &self,
        ops: &mut OpVec,
        config: &Config,
        context: &Context,
        cached_file: &CachedFileRcPtr,
        file_transform: &FileTransform,
        dir: TransformDirection,
    ) -> Result<()>;

    /// Whether the format reads its files in binary rather than text mode.
    ///
    /// Port of `FileFormat::isBinary` (FileTransform.h:98-101 @ v2.5.2).
    fn is_binary(&self) -> bool {
        false
    }

    /// The name of the format's first info, or "Unknown Format" without one.
    ///
    /// Port of `FileFormat::getName` (FileTransform.cpp:561-570 @ v2.5.2).
    fn name(&self) -> &'static str {
        self.format_info()
            .first()
            .map_or("Unknown Format", |info| info.name)
    }
}

/// A format whose reader isn't ported yet: its infos as upstream's `getFormatInfo` declares
/// them, and whether it reads in binary.
#[derive(Debug, Clone)]
pub struct NotPortedFormat {
    infos: Vec<FormatInfo>,
    binary: bool,
}

impl NotPortedFormat {
    /// The port's error for reading a file of this format.
    pub fn not_ported(&self) -> Exception {
        Exception::new(format!(
            "The reader of the '{}' file format is not ported yet.",
            self.name()
        ))
    }
}

impl FileFormat for NotPortedFormat {
    fn format_info(&self) -> Vec<FormatInfo> {
        self.infos.clone()
    }

    /// Refuses: the reader isn't ported yet. The loader takes it for a failed read and tries
    /// the next format.
    fn read(&self, _: &mut InputStream, _: &[u8], _: Interpolation) -> Result<CachedFileRcPtr> {
        Err(self.not_ported())
    }

    /// Refuses, as [`read`](Self::read) never gives a file to build.
    fn build_file_ops(
        &self,
        _: &mut OpVec,
        _: &Config,
        _: &Context,
        _: &CachedFileRcPtr,
        _: &FileTransform,
        _: TransformDirection,
    ) -> Result<()> {
        Err(self.not_ported())
    }

    fn is_binary(&self) -> bool {
        self.binary
    }
}

/// The 19 formats in upstream's registration order, each with its `getFormatInfo`.
///
/// Port of `FormatRegistry::FormatRegistry` (FileTransform.cpp:331-352 @ v2.5.2) and of each
/// format's `LocalFileFormat::getFormatInfo` and `isBinary` (src/OpenColorIO/fileformats/ @
/// v2.5.2, the lines below).
fn create_formats() -> Vec<Box<dyn FileFormat>> {
    use FormatBakeCapabilities as B;
    use FormatCapabilities as C;
    let read_bake = C::READ | C::BAKE;
    let read_write = C::READ | C::WRITE;
    let all_bakes = B::LUT_3D | B::LUT_1D | B::LUT_1D_3D;
    let stub = |infos: Vec<FormatInfo>| -> Box<dyn FileFormat> {
        Box::new(NotPortedFormat {
            infos,
            binary: false,
        })
    };
    vec![
        // FileFormat3DL.cpp:196-208: "lustre" is a copy of the "flame" info.
        stub(vec![
            FormatInfo::new("flame", "3dl", read_bake, B::LUT_3D),
            FormatInfo::new("lustre", "3dl", read_bake, B::LUT_3D),
        ]),
        // FileFormatCC.cpp:69-76.
        Box::new(crate::fileformats::file_format_cc::LocalFileFormat),
        // FileFormatCCC.cpp:79-86.
        stub(vec![FormatInfo::new(
            FILEFORMAT_COLOR_CORRECTION_COLLECTION,
            "ccc",
            read_write,
            B::NONE,
        )]),
        // FileFormatCDL.cpp:102-109.
        stub(vec![FormatInfo::new(
            FILEFORMAT_COLOR_DECISION_LIST,
            "cdl",
            read_write,
            B::NONE,
        )]),
        // FileFormatCTF.cpp:147-170. Upstream sets the bake capabilities of the CTF info on the
        // CLF one a second time (`info.bake_capabilities = ...` after `info2.capabilities`), so
        // the CTF info has none. No caller sees it: the baker reads a format's first info
        // (Baker.cpp:249-251), the CLF one.
        stub(vec![
            FormatInfo::new(
                FILEFORMAT_CLF,
                "clf",
                C::READ | C::BAKE | C::WRITE,
                all_bakes,
            ),
            FormatInfo::new(FILEFORMAT_CTF, "ctf", C::READ | C::BAKE | C::WRITE, B::NONE),
        ]),
        // FileFormatCSP.cpp:351-359.
        stub(vec![FormatInfo::new(
            "cinespace",
            "csp",
            read_bake,
            B::LUT_3D | B::LUT_1D_3D,
        )]),
        // FileFormatDiscreet1DL.cpp:675-682.
        stub(vec![FormatInfo::new(
            "Discreet 1D LUT",
            "lut",
            C::READ,
            B::NONE,
        )]),
        // FileFormatHDL.cpp:295-305.
        stub(vec![FormatInfo::new(
            "houdini", "lut", read_bake, all_bakes,
        )]),
        // FileFormatICC.cpp:81-84, 98-115: binary; the three infos share one `FormatInfo`, its
        // name and extension changed.
        Box::new(NotPortedFormat {
            infos: vec![
                FormatInfo::new(
                    "International Color Consortium profile",
                    "icc",
                    C::READ,
                    B::NONE,
                ),
                FormatInfo::new("Image Color Matching profile", "icm", C::READ, B::NONE),
                FormatInfo::new("ICC profile", "pf", C::READ, B::NONE),
            ],
            binary: true,
        }),
        // FileFormatIridasCube.cpp:140-148.
        stub(vec![FormatInfo::new(
            "iridas_cube",
            "cube",
            read_bake,
            B::LUT_3D,
        )]),
        // FileFormatIridasItx.cpp:113-121.
        stub(vec![FormatInfo::new(
            "iridas_itx",
            "itx",
            read_bake,
            B::LUT_3D,
        )]),
        // FileFormatIridasLook.cpp:514-521.
        stub(vec![FormatInfo::new(
            "iridas_look",
            "look",
            C::READ,
            B::NONE,
        )]),
        // FileFormatPandora.cpp:82-95.
        stub(vec![
            FormatInfo::new("pandora_mga", "mga", C::READ, B::NONE),
            FormatInfo::new("pandora_m3d", "m3d", C::READ, B::NONE),
        ]),
        // FileFormatResolveCube.cpp:247-257.
        stub(vec![FormatInfo::new(
            "resolve_cube",
            "cube",
            read_bake,
            all_bakes,
        )]),
        // FileFormatSpi1D.cpp:82-90.
        stub(vec![FormatInfo::new(
            "spi1d",
            "spi1d",
            read_bake,
            B::LUT_1D,
        )]),
        // FileFormatSpi3D.cpp:72-80.
        stub(vec![FormatInfo::new(
            "spi3d",
            "spi3d",
            read_bake,
            B::LUT_3D,
        )]),
        // FileFormatSpiMtx.cpp:57-64.
        Box::new(crate::fileformats::file_format_spimtx::LocalFileFormat),
        // FileFormatTruelight.cpp:79-87.
        stub(vec![FormatInfo::new(
            "truelight",
            "cub",
            read_bake,
            B::LUT_3D,
        )]),
        // FileFormatVF.cpp:83-90.
        stub(vec![FormatInfo::new("nukevf", "vf", C::READ, B::NONE)]),
    ]
}

/// The format registry: the formats by name (lower case), by extension (in registration
/// order), and the names and extensions that read, bake and write, in registration order.
///
/// Port of `FormatRegistry` (FileTransform.h:113-155, FileTransform.cpp:315-554 @ v2.5.2).
pub struct FormatRegistry {
    /// `m_rawFormats`.
    raw_formats: Vec<Box<dyn FileFormat>>,
    /// `m_formatsByName`: indices into `raw_formats`.
    formats_by_name: BTreeMap<Vec<u8>, usize>,
    /// `m_formatsByExtension`: indices into `raw_formats`.
    formats_by_extension: BTreeMap<Vec<u8>, Vec<usize>>,
    /// `m_readFormatNames` and `m_readFormatExtensions`.
    read: Vec<(&'static str, &'static str)>,
    /// `m_bakeFormatNames` and `m_bakeFormatExtensions`.
    bake: Vec<(&'static str, &'static str)>,
    /// `m_writeFormatNames` and `m_writeFormatExtensions`.
    write: Vec<(&'static str, &'static str)>,
}

impl std::fmt::Debug for FormatRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FormatRegistry")
            .field("read", &self.read)
            .finish_non_exhaustive()
    }
}

impl FormatRegistry {
    /// The registry, made on first use.
    ///
    /// Port of `FormatRegistry::GetInstance` (FileTransform.cpp:319-329 @ v2.5.2).
    pub fn instance() -> &'static FormatRegistry {
        static REGISTRY: OnceLock<FormatRegistry> = OnceLock::new();
        REGISTRY.get_or_init(|| {
            let mut registry = FormatRegistry {
                raw_formats: Vec::new(),
                formats_by_name: BTreeMap::new(),
                formats_by_extension: BTreeMap::new(),
                read: Vec::new(),
                bake: Vec::new(),
                write: Vec::new(),
            };
            for format in create_formats() {
                registry
                    .register_file_format(format)
                    .expect("upstream's formats register");
            }
            registry
        })
    }

    /// The format a name (ignoring case) belongs to.
    ///
    /// Port of `FormatRegistry::getFileFormatByName` (FileTransform.cpp:358-365 @ v2.5.2).
    pub fn file_format_by_name(&self, name: impl AsRef<[u8]>) -> Option<&dyn FileFormat> {
        self.formats_by_name
            .get(&lower(name.as_ref()))
            .map(|&i| self.raw_formats[i].as_ref())
    }

    /// The formats of an extension (ignoring case), in registration order: the formats a file
    /// of that extension tries first.
    ///
    /// Port of `FormatRegistry::getFileFormatForExtension` (FileTransform.cpp:367-375 @
    /// v2.5.2).
    pub fn file_formats_for_extension(&self, extension: impl AsRef<[u8]>) -> Vec<&dyn FileFormat> {
        self.formats_by_extension
            .get(&lower(extension.as_ref()))
            .map(|v| v.iter().map(|&i| self.raw_formats[i].as_ref()).collect())
            .unwrap_or_default()
    }

    /// Port of `FormatRegistry::registerFileFormat` (FileTransform.cpp:377-433 @ v2.5.2).
    fn register_file_format(&mut self, format: Box<dyn FileFormat>) -> Result<()> {
        let infos = format.format_info();
        if infos.is_empty() {
            return Err(Exception::new(
                "FileFormat Registry error. A file format did not provide the required format \
                 info.",
            ));
        }
        let index = self.raw_formats.len();
        for info in &infos {
            if info.capabilities == FormatCapabilities::NONE {
                return Err(Exception::new(
                    "FileFormat Registry error. A file format does not define either reading or \
                     writing.",
                ));
            }
            if self.file_format_by_name(info.name).is_some() {
                return Err(Exception::new(format!(
                    "Cannot register multiple file formats named, '{}'.",
                    info.name
                )));
            }
            self.formats_by_name
                .insert(lower(info.name.as_bytes()), index);
            // Upstream keys the extension as given (they are lower case).
            self.formats_by_extension
                .entry(info.extension.as_bytes().to_vec())
                .or_default()
                .push(index);
            let entry = (info.name, info.extension);
            if info.capabilities.contains(FormatCapabilities::READ) {
                self.read.push(entry);
            }
            if info.capabilities.contains(FormatCapabilities::BAKE) {
                self.bake.push(entry);
            }
            if info.capabilities.contains(FormatCapabilities::WRITE) {
                self.write.push(entry);
            }
        }
        self.raw_formats.push(format);
        Ok(())
    }

    /// Port of `FormatRegistry::getNumRawFormats` (FileTransform.cpp:435-438 @ v2.5.2).
    pub fn num_raw_formats(&self) -> i32 {
        self.raw_formats.len() as i32
    }

    /// The format at `index` in registration order.
    ///
    /// Port of `FormatRegistry::getRawFormatByIndex` (FileTransform.cpp:440-448 @ v2.5.2).
    pub fn raw_format_by_index(&self, index: i32) -> Option<&dyn FileFormat> {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.raw_formats.get(i))
            .map(Box::as_ref)
    }

    /// The (name, extension) list of a capability: one of `READ`, `BAKE` or `WRITE`, else
    /// none.
    fn list(&self, capability: FormatCapabilities) -> &[(&'static str, &'static str)] {
        match capability {
            FormatCapabilities::READ => &self.read,
            FormatCapabilities::BAKE => &self.bake,
            FormatCapabilities::WRITE => &self.write,
            _ => &[],
        }
    }

    /// The number of format names with the capability.
    ///
    /// Port of `FormatRegistry::getNumFormats` (FileTransform.cpp:450-465 @ v2.5.2).
    pub fn num_formats(&self, capability: FormatCapabilities) -> i32 {
        self.list(capability).len() as i32
    }

    /// The name at `index` of the capability's list; `""` outside it.
    ///
    /// Port of `FormatRegistry::getFormatNameByIndex` (FileTransform.cpp:467-494 @ v2.5.2).
    pub fn format_name_by_index(&self, capability: FormatCapabilities, index: i32) -> &'static str {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.list(capability).get(i))
            .map_or("", |e| e.0)
    }

    /// The extension at `index` of the capability's list; `""` outside it.
    ///
    /// Port of `FormatRegistry::getFormatExtensionByIndex` (FileTransform.cpp:496-526 @
    /// v2.5.2).
    pub fn format_extension_by_index(
        &self,
        capability: FormatCapabilities,
        index: i32,
    ) -> &'static str {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.list(capability).get(i))
            .map_or("", |e| e.1)
    }

    /// Whether a format reads files of `extension` (as a C string), ignoring case and a
    /// leading `.`; `""` and `"."` are not.
    ///
    /// Port of `FormatRegistry::isFormatExtensionSupported` (FileTransform.cpp:528-552 @
    /// v2.5.2).
    pub fn is_format_extension_supported(&self, extension: impl AsRef<[u8]>) -> bool {
        let extension = c_str(extension.as_ref());
        // Early return false with an input of just the dot or invalid pointer.
        if extension.is_empty() || extension == b"." {
            return false;
        }
        // If dot is present at the start, ignore that dot.
        let extension = extension.strip_prefix(b".").unwrap_or(extension);
        self.formats_by_extension.contains_key(&lower(extension))
    }

    /// Every info of every format, in registration order.
    pub fn format_infos(&self) -> Vec<FormatInfo> {
        self.raw_formats
            .iter()
            .flat_map(|f| f.format_info())
            .collect()
    }
}

/// The error of baking in a format that doesn't bake.
///
/// Port of `FileFormat::bake` (FileTransform.cpp:572-579 @ v2.5.2).
pub fn bake_not_supported(format_name: &[u8]) -> Exception {
    Exception::new(
        [
            b"Format '".as_slice(),
            format_name,
            b"' does not support baking.",
        ]
        .concat(),
    )
}

/// The error of writing in a format that doesn't write.
///
/// Port of `FileFormat::write` (FileTransform.cpp:581-593 @ v2.5.2).
pub fn write_not_supported(format_name: &[u8]) -> Exception {
    Exception::new(
        [
            b"Format '".as_slice(),
            format_name,
            b"' does not support writing.",
        ]
        .concat(),
    )
}
