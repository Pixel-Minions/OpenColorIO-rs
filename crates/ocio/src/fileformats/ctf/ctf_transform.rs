// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The CLF/CTF versions and the transform the CTF reader builds: its format metadata (id,
//! name, inverseOf and `xmlns:*` attributes, Id element, descriptions and descriptors), info
//! metadata and ops, to and from the format metadata of a group.
//!
//! Port of `src/OpenColorIO/fileformats/ctf/CTFTransform.h` and `CTFTransform.cpp:1-586` @
//! v2.6.0, the reader's half; the writers (`TransformWriter` and the op writers,
//! `GetMinimumVersion`) are in `ctf_writer.rs`.

use std::cmp::Ordering;
use std::fmt;
use std::sync::Arc;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::format_metadata::{
    Elements, FormatMetadataImpl, METADATA_DESCRIPTION, METADATA_ID, METADATA_ID_ELEMENT,
    METADATA_INFO, METADATA_INPUT_DESCRIPTOR, METADATA_NAME, METADATA_OUTPUT_DESCRIPTOR,
};
use ocio_ops::op::OpVec;
use ocio_ops::op_data::OpDataRcPtr;
use ocio_ops::open_color_types::BitDepth;
use ocio_ops::platform::strcasecmp;
use ocio_ops::utils::cscan::{ScanArg, ScanCrt, sscanf};
use ocio_ops::utils::string_utils::{c_str, starts_with};
use ocio_ops::{Exception, Result};

use super::ctf_reader_utils::{ATTR_INVERSE_OF, ATTR_LANGUAGE};

/// The namespace of SMPTE ST 2136-1 CLF, which its ProcessList's `xmlns` holds.
///
/// Port of `SMPTE_XMLNS_URL` (CTFTransform.cpp:42 @ v2.6.0).
pub const SMPTE_XMLNS_URL: &[u8] = b"http://www.smpte-ra.org/ns/2136-1/2024";

/// The SMPTE ST 2136-1 version string a `compCLFversion` may hold.
///
/// The literal of `CTFVersion::CTFVersion` (CTFTransform.cpp:58-62 @ v2.6.0).
const SMPTE_CLF_VERSION: &[u8] = b"ST2136-1:2024";

/// The version strings a [`CtfVersion`] accepts besides MAJOR[.MINOR[.REVISION]], which is
/// always accepted.
///
/// Port of `CTFVersion::StringFormat` (CTFTransform.h:25-30 @ v2.6.0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StringFormat {
    /// `VERSION_NUMERIC`: numeric version is always accepted.
    VersionNumeric = 0,
    /// `VERSION_SMPTE_XMLNS`: the SMPTE namespace, [`SMPTE_XMLNS_URL`].
    VersionSmpteXmlns = 1 << 1,
    /// `VERSION_SMPTE_CLF`: the SMPTE version, `ST2136-1:2024`.
    VersionSmpteClf = 1 << 2,
}

impl StringFormat {
    /// The enumerator's value, which upstream tests bit by bit.
    fn bits(self) -> u32 {
        self as u32
    }
}

/// A CLF or CTF version: MAJOR[.MINOR[.REVISION]], or a SMPTE ST 2136-1 version string, which
/// counts as 3.0 and is written as it was read.
///
/// `Clone` is upstream's copy constructor and `operator=`; comparisons are by the numbers
/// only (`operator==`, `operator<`, ...), whatever the version string.
///
/// Port of `CTFVersion` (CTFTransform.h:22-99, CTFTransform.cpp:44-191 @ v2.6.0).
#[derive(Debug, Clone, Default)]
pub struct CtfVersion {
    /// `m_major`: CTF and Academy/ASC CLF use the numeric version system.
    major: u32,
    /// `m_minor`.
    minor: u32,
    /// `m_revision`.
    revision: u32,
    /// `m_version_string`: SMPTE CLF uses a non-numeric xmlns version system.
    version_string: Vec<u8>,
}

impl CtfVersion {
    /// Port of `CTFVersion::CTFVersion(unsigned int, unsigned int, unsigned int)`
    /// (CTFTransform.h:39-44 @ v2.6.0).
    pub const fn new(major: u32, minor: u32, revision: u32) -> CtfVersion {
        CtfVersion {
            major,
            minor,
            revision,
            version_string: Vec::new(),
        }
    }

    /// Reads `version_string`: a SMPTE version string `accepted_format` allows (compared
    /// ignoring ASCII case, and kept as it is), which counts as 3.0, or one to three numbers
    /// separated by dots (MAJOR[.MINOR[.REVISION]]); the missing ones are 0.
    ///
    /// Port of `CTFVersion::CTFVersion(const std::string &, StringFormat)`
    /// (CTFTransform.cpp:44-118 @ v2.6.0). Its `::isdigit` sees only the ASCII digits as
    /// digits here: the string comes from expat, as UTF-8, whose first byte past ASCII is a
    /// lead byte (0xC2 to 0xF4), which neither wheel's runtime takes for a digit. Its
    /// `sscanf("%d.%d.%d")` stores each number's low 32 bits, as both runtimes do for numbers
    /// past `int` (docs/improvements.md, I-160).
    pub fn read(version_string: &[u8], accepted_format: StringFormat) -> Result<CtfVersion> {
        let accepted = accepted_format.bits();
        let xmlns = StringFormat::VersionSmpteXmlns.bits();
        let clf = StringFormat::VersionSmpteClf.bits();

        // Parse the version string to see if that matches the SMPTE namespace/version
        // patterns. If so store the version string and consider equivalent to v3.0.
        if accepted & (xmlns | clf) != 0 {
            let mut res = false;
            if accepted & xmlns != 0 {
                res = strcasecmp(c_str(version_string), SMPTE_XMLNS_URL).is_eq();
            }

            if !res && accepted & clf != 0 {
                res = strcasecmp(c_str(version_string), SMPTE_CLF_VERSION).is_eq();
            }

            if res {
                let mut version = CtfVersion::new(3, 0, 0);
                version.version_string = version_string.to_vec();
                return Ok(version);
            }
        }

        // For non-SMPTE namespace versions, parse as MAJOR[.MINOR[.REVISION]]
        let mut num_dot: u32 = 0;
        let mut num_int: u32 = 0;
        let mut can_be_dot = false;
        let mut it = 0usize;
        while it < version_string.len() {
            let c = version_string[it];
            if c.is_ascii_digit() {
                num_int = num_dot + 1;
                can_be_dot = true;
                it += 1;
            } else if c == b'.' && can_be_dot {
                can_be_dot = false;
                num_dot += 1;
                it += 1;
            } else {
                break;
            }
        }
        if version_string.is_empty()
            || it != version_string.len()
            || num_int == 0
            || num_int > 3
            || num_int == num_dot
        {
            let mut os = OStringStream::new(Crt::NATIVE);
            os.put_str("'");
            os.put_bytes(version_string);
            os.put_str("' is not a valid version. Expecting ");
            if accepted & clf != 0 {
                os.put_str("'ST2136-1:2024' or ");
            }
            if accepted & xmlns != 0 {
                os.put_str("'");
                os.put_bytes(SMPTE_XMLNS_URL);
                os.put_str("' or ");
            }
            os.put_str("MAJOR[.MINOR[.REVISION]] ");
            return Err(Exception::new(c_str(&os.into_bytes()).to_vec()));
        }

        let mut major: i32 = 0;
        let mut minor: i32 = 0;
        let mut revision: i32 = 0;

        sscanf(
            ScanCrt::NATIVE,
            c_str(version_string),
            b"%d.%d.%d",
            &mut [
                ScanArg::Int(&mut major),
                ScanArg::Int(&mut minor),
                ScanArg::Int(&mut revision),
            ],
        );

        Ok(CtfVersion::new(major as u32, minor as u32, revision as u32))
    }

    /// [`read`](Self::read) of a numeric version only: `CTFVersion(versionString)`, whose
    /// format defaults to `VERSION_NUMERIC` (CTFTransform.h:33 @ v2.6.0).
    pub fn read_version(version_string: &[u8]) -> Result<CtfVersion> {
        Self::read(version_string, StringFormat::VersionNumeric)
    }

    /// The bytes `operator<<` writes: the version string when there is one; else MAJOR, then
    /// .MINOR when the minor or the revision isn't 0, then .REVISION when the revision isn't
    /// 0.
    ///
    /// Port of `operator<<(std::ostream &, const CTFVersion &)` (CTFTransform.h:69-89 @
    /// v2.6.0).
    pub fn to_bytes(&self) -> Vec<u8> {
        if !self.version_string.is_empty() {
            return self.version_string.clone();
        }
        let mut s = self.major.to_string();
        if self.minor != 0 || self.revision != 0 {
            s.push('.');
            s.push_str(&self.minor.to_string());
            if self.revision != 0 {
                s.push('.');
                s.push_str(&self.revision.to_string());
            }
        }
        s.into_bytes()
    }
}

impl PartialEq for CtfVersion {
    /// Port of `CTFVersion::operator==` (CTFTransform.cpp:132-139 @ v2.6.0): the numbers only.
    fn eq(&self, rhs: &CtfVersion) -> bool {
        self.major == rhs.major && self.minor == rhs.minor && self.revision == rhs.revision
    }
}

impl Eq for CtfVersion {}

impl PartialOrd for CtfVersion {
    fn partial_cmp(&self, rhs: &CtfVersion) -> Option<Ordering> {
        Some(self.cmp(rhs))
    }
}

impl Ord for CtfVersion {
    /// By major, then minor, then revision.
    ///
    /// Port of `CTFVersion::operator<`, `>`, `<=` and `>=` (CTFTransform.cpp:141-191 @ v2.6.0).
    fn cmp(&self, rhs: &CtfVersion) -> Ordering {
        (self.major, self.minor, self.revision).cmp(&(rhs.major, rhs.minor, rhs.revision))
    }
}

impl fmt::Display for CtfVersion {
    /// [`CtfVersion::to_bytes`] as text. The version string is UTF-8 from expat, or a
    /// literal; a byte that isn't is shown as U+FFFD (only `to_bytes` keeps it).
    ///
    /// Port of `operator<<(std::ostream &, const CTFVersion &)` (CTFTransform.h:69-89 @
    /// v2.6.0).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&String::from_utf8_lossy(&self.to_bytes()))
    }
}

// Process List Versions (CTFTransform.h:101-145 @ v2.6.0).

/// Version 1.2 2012 initial Autodesk version.
pub const CTF_PROCESS_LIST_VERSION_1_2: CtfVersion = CtfVersion::new(1, 2, 0);
/// Version 1.3 2012-12 revised matrix.
pub const CTF_PROCESS_LIST_VERSION_1_3: CtfVersion = CtfVersion::new(1, 3, 0);
/// Version 1.4 2013-07 adds ACES v0.2.
pub const CTF_PROCESS_LIST_VERSION_1_4: CtfVersion = CtfVersion::new(1, 4, 0);
/// Version 1.5 2014-01 adds ACES v0.7.
pub const CTF_PROCESS_LIST_VERSION_1_5: CtfVersion = CtfVersion::new(1, 5, 0);
/// Version 1.6 2014-05 adds invLut3D.
pub const CTF_PROCESS_LIST_VERSION_1_6: CtfVersion = CtfVersion::new(1, 6, 0);
/// Version 1.7 2015-01 adds 'invert' flag to referenceOp and to the transform, adds 1.0
/// styles to ACES op, adds CLF v2 support (IndexMap, alt. Range, CDL styles).
pub const CTF_PROCESS_LIST_VERSION_1_7: CtfVersion = CtfVersion::new(1, 7, 0);
/// Version 1.8 2017-10 adds Function op as a valid element in CTF files.
pub const CTF_PROCESS_LIST_VERSION_1_8: CtfVersion = CtfVersion::new(1, 8, 0);
/// Version 2.0 sync with OCIO and incorporate CLF v3 changes.
pub const CTF_PROCESS_LIST_VERSION_2_0: CtfVersion = CtfVersion::new(2, 0, 0);
/// Version 2.1 2021-08 adds the 'FIXED_FUNCTION_ACES_GAMUT_COMP_13' style to FixedFunctionOp.
pub const CTF_PROCESS_LIST_VERSION_2_1: CtfVersion = CtfVersion::new(2, 1, 0);
/// Version 2.4 2024-08 adds the ACES 2 related FixedFunctionOps and the LIN_TO_PQ,
/// LIN_TO_GAMMA_LOG, AND LIN_TO_DOUBLE_LOG FixedFunctionOps.
pub const CTF_PROCESS_LIST_VERSION_2_4: CtfVersion = CtfVersion::new(2, 4, 0);
/// Version 2.5 2025-08 adds the GradingHueCurve.
pub const CTF_PROCESS_LIST_VERSION_2_5: CtfVersion = CtfVersion::new(2, 5, 0);
/// Version 2.6 2026-08 adds the FixedFunction FIXED_FUNCTION_ACES_RGB_TO_HMJ_20.
pub const CTF_PROCESS_LIST_VERSION_2_6: CtfVersion = CtfVersion::new(2, 6, 0);
/// The latest version.
pub const CTF_PROCESS_LIST_VERSION: CtfVersion = CTF_PROCESS_LIST_VERSION_2_6;

/// Version 1.0 initial Autodesk version for InfoElt (CTFTransform.h:148-149 @ v2.6.0).
pub const CTF_INFO_ELEMENT_VERSION_1_0: f32 = 1.0;
/// Version 2.0 Flame 2017 Ext1 InfoElt (CTFTransform.h:151-152 @ v2.6.0).
pub const CTF_INFO_ELEMENT_VERSION_2_0: f32 = 2.0;
/// The latest InfoElt version (CTFTransform.h:157 @ v2.6.0).
pub const CTF_INFO_ELEMENT_VERSION: f32 = CTF_INFO_ELEMENT_VERSION_2_0;

/// `ConstOpDataVec`: the op data of a transform.
pub type ConstOpDataVec = Vec<OpDataRcPtr>;

/// What the CTF reader builds from a ProcessList (and the writer writes): its format
/// metadata (attributes, Id element, descriptions and descriptors), its info, its ops, and
/// its versions.
///
/// Port of `CTFReaderTransform` (CTFTransform.h:160-246, CTFTransform.cpp:193-238 @ v2.6.0).
#[derive(Debug, Clone)]
pub struct CtfReaderTransform {
    /// `m_infoMetadata`.
    info_metadata: FormatMetadataImpl,
    /// `m_formatMetadata`: a top-level element (`FormatMetadataImpl()`).
    format_metadata: FormatMetadataImpl,
    /// `m_ops`.
    ops: ConstOpDataVec,
    /// `m_version`: CTF version used even for CLF files. CLF versions <= 2.0 are interpreted
    /// as CTF version 1.7.
    version: CtfVersion,
    /// `m_versionCLF`: original CLF version (for reference).
    version_clf: CtfVersion,
    /// `m_prevOutBD`: track bit-depth of ops when loading a CTFTransform.
    prev_out_bd: BitDepth,
}

impl Default for CtfReaderTransform {
    fn default() -> Self {
        Self::new()
    }
}

impl CtfReaderTransform {
    /// An empty transform of the latest CTF version.
    ///
    /// Port of `CTFReaderTransform::CTFReaderTransform()` (CTFTransform.cpp:193-198 @ v2.6.0).
    pub fn new() -> CtfReaderTransform {
        CtfReaderTransform {
            info_metadata: FormatMetadataImpl::new(METADATA_INFO, b"").expect("a non-empty name"),
            format_metadata: FormatMetadataImpl::root(),
            ops: ConstOpDataVec::new(),
            version: CTF_PROCESS_LIST_VERSION,
            version_clf: CtfVersion::new(0, 0, 0),
            prev_out_bd: BitDepth::Unknown,
        }
    }

    /// A transform of `ops`' data, with what the CLF keeps of `metadata`
    /// ([`from_metadata`](Self::from_metadata)).
    ///
    /// Port of `CTFReaderTransform::CTFReaderTransform(const OpRcPtrVec &, const
    /// FormatMetadataImpl &)` (CTFTransform.cpp:200-213 @ v2.6.0).
    pub fn from_ops(ops: &OpVec, metadata: &FormatMetadataImpl) -> Result<CtfReaderTransform> {
        let mut transform = CtfReaderTransform::new();
        transform.from_metadata(metadata)?;

        for op in ops.iter() {
            let op_data = op.data().clone();
            transform.ops.push(op_data);
        }
        Ok(transform)
    }

    /// The format metadata's "id" attribute (found ignoring ASCII case), or "".
    ///
    /// Port of `CTFReaderTransform::getID` (CTFTransform.h:170-173 @ v2.6.0).
    pub fn get_id(&self) -> &[u8] {
        self.format_metadata
            .get_attribute_value_string(Some(METADATA_ID))
    }

    /// Sets the format metadata's "id" attribute: a C string.
    ///
    /// Port of `CTFReaderTransform::setID` (CTFTransform.h:174-177 @ v2.6.0).
    pub fn set_id(&mut self, id: &[u8]) -> Result<()> {
        self.format_metadata
            .add_attribute(Some(METADATA_ID), Some(id))
    }

    /// The format metadata's "name" attribute (found ignoring ASCII case), or "".
    ///
    /// Port of `CTFReaderTransform::getName` (CTFTransform.h:179-182 @ v2.6.0).
    pub fn get_name(&self) -> &[u8] {
        self.format_metadata
            .get_attribute_value_string(Some(METADATA_NAME))
    }

    /// Sets the format metadata's "name" attribute: a C string.
    ///
    /// Port of `CTFReaderTransform::setName` (CTFTransform.h:183-186 @ v2.6.0).
    pub fn set_name(&mut self, name: &[u8]) -> Result<()> {
        self.format_metadata
            .add_attribute(Some(METADATA_NAME), Some(name))
    }

    /// Port of `CTFReaderTransform::getInfoMetadata` (CTFTransform.h:192-195 @ v2.6.0).
    pub fn get_info_metadata(&self) -> &FormatMetadataImpl {
        &self.info_metadata
    }

    /// Port of `CTFReaderTransform::getInfoMetadata` (CTFTransform.h:188-191 @ v2.6.0).
    pub fn get_info_metadata_mut(&mut self) -> &mut FormatMetadataImpl {
        &mut self.info_metadata
    }

    /// Port of `CTFReaderTransform::getFormatMetadata` (CTFTransform.h:201-204 @ v2.6.0).
    pub fn get_format_metadata(&self) -> &FormatMetadataImpl {
        &self.format_metadata
    }

    /// Port of `CTFReaderTransform::getFormatMetadata` (CTFTransform.h:197-200 @ v2.6.0).
    pub fn get_format_metadata_mut(&mut self) -> &mut FormatMetadataImpl {
        &mut self.format_metadata
    }

    /// Port of `CTFReaderTransform::getOpDataVec` (CTFTransform.h:206-209 @ v2.6.0).
    pub fn get_op_data_vec(&self) -> &ConstOpDataVec {
        &self.ops
    }

    /// Port of `CTFReaderTransform::getOpDataVec` (CTFTransform.h:210-213 @ v2.6.0).
    pub fn get_op_data_vec_mut(&mut self) -> &mut ConstOpDataVec {
        &mut self.ops
    }

    /// Port of `CTFReaderTransform::setCTFVersion` (CTFTransform.cpp:215-218 @ v2.6.0).
    pub fn set_ctf_version(&mut self, ver: &CtfVersion) {
        self.version = ver.clone();
    }

    /// Port of `CTFReaderTransform::setCLFVersion` (CTFTransform.cpp:220-223 @ v2.6.0).
    pub fn set_clf_version(&mut self, ver: &CtfVersion) {
        self.version_clf = ver.clone();
    }

    /// Port of `CTFReaderTransform::getCTFVersion` (CTFTransform.cpp:225-228 @ v2.6.0).
    pub fn get_ctf_version(&self) -> &CtfVersion {
        &self.version
    }

    /// Port of `CTFReaderTransform::getCLFVersion` (CTFTransform.cpp:230-233 @ v2.6.0).
    pub fn get_clf_version(&self) -> &CtfVersion {
        &self.version_clf
    }

    /// Whether the file was a CLF: its CLF version isn't 0.
    ///
    /// Port of `CTFReaderTransform::isCLF` (CTFTransform.cpp:235-238 @ v2.6.0).
    pub fn is_clf(&self) -> bool {
        self.version_clf != CtfVersion::new(0, 0, 0)
    }

    /// Port of `CTFReaderTransform::setPreviousOutBitDepth` (CTFTransform.h:221-224 @ v2.6.0).
    pub fn set_previous_out_bit_depth(&mut self, out: BitDepth) {
        self.prev_out_bd = out;
    }

    /// Port of `CTFReaderTransform::getPreviousOutBitDepth` (CTFTransform.h:225-228 @ v2.6.0).
    pub fn get_previous_out_bit_depth(&self) -> BitDepth {
        self.prev_out_bd
    }

    /// Copies what the CLF spec keeps of `metadata`, the top level metadata of a ProcessList
    /// (the metadata of the process nodes is in their op data), into the format metadata:
    /// - the non-empty attributes "id", "name" and "inverseOf", in that order, then every
    ///   non-empty "xmlns:*" attribute (the prefix compared exactly); other attributes are
    ///   ignored;
    /// - the value of the last "Id" child, if not empty;
    /// - the "Description", then "InputDescriptor", then "OutputDescriptor" children, each
    ///   with its name and value and its non-empty "language" attribute;
    /// - the "Info" children, merged into one with all their attributes and children;
    /// - any other children or attributes are ignored.
    ///
    /// Names compare ignoring case, but the Info children merge only when named exactly
    /// "Info" (`FormatMetadataImpl::combine`): another case fails.
    ///
    /// Port of `CTFReaderTransform::fromMetadata` (CTFTransform.cpp:471-533 @ v2.6.0).
    pub fn from_metadata(&mut self, metadata: &FormatMetadataImpl) -> Result<()> {
        copy_process_list_metadata(&mut self.format_metadata, metadata)?;

        // Combine all Info elements.
        for elt in metadata.get_children_elements() {
            if strcasecmp(elt.get_element_name(), METADATA_INFO).is_eq() {
                self.info_metadata.combine(elt)?;
            }
        }
        Ok(())
    }

    /// Adds the transform's ProcessList information to `metadata`, as
    /// [`from_metadata`](Self::from_metadata) reads it, then the info when it has
    /// attributes, children or a value.
    ///
    /// Port of `CTFReaderTransform::toMetadata` (CTFTransform.cpp:537-586 @ v2.6.0).
    pub fn to_metadata(&self, metadata: &mut FormatMetadataImpl) -> Result<()> {
        // Put CTF processList information into the FormatMetadata.
        copy_process_list_metadata(metadata, &self.format_metadata)?;

        // Info Metadata.
        let info_value = self.info_metadata.get_element_value();
        if self.info_metadata.get_num_attributes() != 0
            || self.info_metadata.get_num_children_elements() != 0
            || !info_value.is_empty()
        {
            metadata
                .get_children_elements_mut()
                .push(self.info_metadata.clone());
        }
        Ok(())
    }
}

/// The part `fromMetadata` and `toMetadata` share, from `source` to `dest`: the attributes,
/// the Id element, and the descriptions and descriptors.
///
/// Port of `CTFReaderTransform::fromMetadata` (CTFTransform.cpp:487-522 @ v2.6.0) and of the
/// same lines of `CTFReaderTransform::toMetadata` (CTFTransform.cpp:541-577), whose code is
/// the same but for the direction.
fn copy_process_list_metadata(
    dest: &mut FormatMetadataImpl,
    source: &FormatMetadataImpl,
) -> Result<()> {
    // Attributes
    copy_non_empty_attribute(dest, source, METADATA_ID)?;
    copy_non_empty_attribute(dest, source, METADATA_NAME)?;
    copy_non_empty_attribute(dest, source, ATTR_INVERSE_OF)?;

    // all "xmlns:*" attributes needs to be copied too.
    for i in 0..source.get_num_attributes() {
        let attr_name = source.get_attribute_name(i).to_vec();
        if starts_with(&attr_name, b"xmlns:") {
            copy_non_empty_attribute(dest, source, &attr_name)?;
        }
    }

    // Id Element
    let id = get_last_element_value(source.get_children_elements(), METADATA_ID_ELEMENT).to_vec();
    add_non_empty_element(dest, METADATA_ID_ELEMENT, &id)?;

    // Description, input and output descriptors
    for element_name in [
        METADATA_DESCRIPTION,
        METADATA_INPUT_DESCRIPTOR,
        METADATA_OUTPUT_DESCRIPTOR,
    ] {
        for desc in source.get_children_elements_named(element_name) {
            let mut new_el =
                FormatMetadataImpl::new(desc.get_element_name(), c_str(desc.get_element_value()))?;
            let lang = desc.get_attribute_value_string(Some(ATTR_LANGUAGE));
            if !lang.is_empty() {
                new_el.add_attribute(Some(ATTR_LANGUAGE), Some(lang))?;
            }
            dest.get_children_elements_mut().push(new_el);
        }
    }
    Ok(())
}

/// `typedef OCIO_SHARED_PTR<CTFReaderTransform> CTFReaderTransformPtr` (CTFTransform.h:248).
pub type CtfReaderTransformPtr = Arc<CtfReaderTransform>;

/// Adds the value of each element named `name` (ignoring case) to `values`.
///
/// Port of `GetElementsValues` (CTFTransform.cpp:240-251 @ v2.6.0). The values are C strings
/// (`getElementValue`).
pub fn get_elements_values(elements: &Elements, name: &[u8], values: &mut Vec<Vec<u8>>) {
    for element in elements {
        if strcasecmp(name, element.get_element_name()).is_eq() {
            values.push(c_str(element.get_element_value()).to_vec());
        }
    }
}

/// The value of the last element named `name` (ignoring case), or "".
///
/// Port of `GetLastElementValue` (CTFTransform.cpp:440-450 @ v2.6.0).
fn get_last_element_value<'a>(elements: &'a Elements, name: &[u8]) -> &'a [u8] {
    for it in elements.iter().rev() {
        if strcasecmp(name, it.get_element_name()).is_eq() {
            return it.get_element_value();
        }
    }
    b""
}

/// Adds `source`'s attribute `attrname` (found ignoring case) to `dest` under that name,
/// when its value isn't empty.
///
/// Port of `CopyNonEmptyAttribute` (CTFTransform.cpp:452-459 @ v2.6.0).
fn copy_non_empty_attribute(
    dest: &mut FormatMetadataImpl,
    source: &FormatMetadataImpl,
    attrname: &[u8],
) -> Result<()> {
    let value = source.get_attribute_value_string(Some(attrname));
    if !value.is_empty() {
        dest.add_attribute(Some(attrname), Some(value))?;
    }
    Ok(())
}

/// Port of `AddNonEmptyElement` (CTFTransform.cpp:461-467 @ v2.6.0).
fn add_non_empty_element(
    metadata: &mut FormatMetadataImpl,
    name: &[u8],
    value: &[u8],
) -> Result<()> {
    if !value.is_empty() {
        metadata.add_child_element(Some(name), Some(value))?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "ctf_transform_tests.rs"]
mod tests;
