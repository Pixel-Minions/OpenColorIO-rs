// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The CLF/CTF versions and the transform the CTF reader builds: its id, name, descriptors,
//! descriptions, info metadata and ops, to and from the format metadata of a group.
//!
//! Port of `src/OpenColorIO/fileformats/ctf/CTFTransform.h` and `CTFTransform.cpp:1-505` @
//! v2.5.2, so far the reader's half; the writers (`TransformWriter` and the op writers,
//! `GetMinimumVersion`) come with WP 4.6.

use std::cmp::Ordering;
use std::fmt;
use std::sync::Arc;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::format_metadata::{
    Elements, FormatMetadataImpl, METADATA_DESCRIPTION, METADATA_ID, METADATA_INFO,
    METADATA_INPUT_DESCRIPTOR, METADATA_NAME, METADATA_OUTPUT_DESCRIPTOR,
};
use ocio_ops::op::OpVec;
use ocio_ops::op_data::OpDataRcPtr;
use ocio_ops::open_color_types::BitDepth;
use ocio_ops::platform::strcasecmp;
use ocio_ops::utils::cscan::{ScanArg, ScanCrt, sscanf};
use ocio_ops::utils::string_utils::c_str;
use ocio_ops::{Exception, Result};

use super::ctf_reader_utils::ATTR_INVERSE_OF;

/// A CLF or CTF version: MAJOR[.MINOR[.REVISION]].
///
/// Port of `CTFVersion` (CTFTransform.h:22-84, CTFTransform.cpp:42-161 @ v2.5.2).
#[derive(Debug, Clone, Copy, Default)]
pub struct CtfVersion {
    /// `m_major`.
    major: u32,
    /// `m_minor`.
    minor: u32,
    /// `m_revision`.
    revision: u32,
}

impl CtfVersion {
    /// Port of `CTFVersion::CTFVersion(unsigned int, unsigned int, unsigned int)`
    /// (CTFTransform.h:35-40 @ v2.5.2).
    pub const fn new(major: u32, minor: u32, revision: u32) -> CtfVersion {
        CtfVersion {
            major,
            minor,
            revision,
        }
    }

    /// Reads `version_string`, which must be one to three numbers separated by dots
    /// (MAJOR[.MINOR[.REVISION]]); the missing ones are 0.
    ///
    /// Port of `CTFVersion::ReadVersion` (CTFTransform.cpp:42-89 @ v2.5.2). Its `::isdigit`
    /// sees only the ASCII digits as digits here: the string comes from expat, as UTF-8, whose
    /// first byte past ASCII is a lead byte (0xC2 to 0xF4), which neither wheel's runtime
    /// takes for a digit. Its `sscanf("%d.%d.%d")` stores each number's low 32 bits, as both
    /// runtimes do for numbers past `int` (docs/improvements.md, I-160).
    pub fn read_version(version_string: &[u8]) -> Result<CtfVersion> {
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
            os.put_str("' is not a valid version. ");
            os.put_str("Expecting MAJOR[.MINOR[.REVISION]] ");
            return Err(Exception::new(os.into_bytes()));
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
}

impl PartialEq for CtfVersion {
    /// Port of `CTFVersion::operator==` (CTFTransform.cpp:102-109 @ v2.5.2).
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
    /// Port of `CTFVersion::operator<`, `>`, `<=` and `>=` (CTFTransform.cpp:111-161 @ v2.5.2).
    fn cmp(&self, rhs: &CtfVersion) -> Ordering {
        (self.major, self.minor, self.revision).cmp(&(rhs.major, rhs.minor, rhs.revision))
    }
}

impl fmt::Display for CtfVersion {
    /// MAJOR, then .MINOR when the minor or the revision isn't 0, then .REVISION when the
    /// revision isn't 0.
    ///
    /// Port of `operator<<(std::ostream &, const CTFVersion &)` (CTFTransform.h:65-78 @
    /// v2.5.2).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.major)?;
        if self.minor != 0 || self.revision != 0 {
            write!(f, ".{}", self.minor)?;
            if self.revision != 0 {
                write!(f, ".{}", self.revision)?;
            }
        }
        Ok(())
    }
}

// Process List Versions (CTFTransform.h:86-127 @ v2.5.2).

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
/// The latest version.
pub const CTF_PROCESS_LIST_VERSION: CtfVersion = CTF_PROCESS_LIST_VERSION_2_5;

/// Version 1.0 initial Autodesk version for InfoElt (CTFTransform.h:130-131 @ v2.5.2).
pub const CTF_INFO_ELEMENT_VERSION_1_0: f32 = 1.0;
/// Version 2.0 Flame 2017 Ext1 InfoElt (CTFTransform.h:133-134 @ v2.5.2).
pub const CTF_INFO_ELEMENT_VERSION_2_0: f32 = 2.0;
/// The latest InfoElt version (CTFTransform.h:139 @ v2.5.2).
pub const CTF_INFO_ELEMENT_VERSION: f32 = CTF_INFO_ELEMENT_VERSION_2_0;

/// `ConstOpDataVec`: the op data of a transform.
pub type ConstOpDataVec = Vec<OpDataRcPtr>;

/// What the CTF reader builds from a ProcessList (and the writer writes): its attributes,
/// descriptors, descriptions and info, its ops, and its versions.
///
/// Port of `CTFReaderTransform` (CTFTransform.h:142-261, CTFTransform.cpp:163-208 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct CtfReaderTransform {
    /// `m_id`.
    id: Vec<u8>,
    /// `m_name`.
    name: Vec<u8>,
    /// `m_inverseOfId`.
    inverse_of_id: Vec<u8>,
    /// `m_inDescriptor`.
    in_descriptor: Vec<u8>,
    /// `m_outDescriptor`.
    out_descriptor: Vec<u8>,
    /// `m_infoMetadata`.
    info_metadata: FormatMetadataImpl,
    /// `m_ops`.
    ops: ConstOpDataVec,
    /// `m_descriptions`.
    descriptions: Vec<Vec<u8>>,
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
    /// Port of `CTFReaderTransform::CTFReaderTransform()` (CTFTransform.cpp:163-168 @ v2.5.2).
    pub fn new() -> CtfReaderTransform {
        CtfReaderTransform {
            id: Vec::new(),
            name: Vec::new(),
            inverse_of_id: Vec::new(),
            in_descriptor: Vec::new(),
            out_descriptor: Vec::new(),
            info_metadata: FormatMetadataImpl::new(METADATA_INFO, b"").expect("a non-empty name"),
            ops: ConstOpDataVec::new(),
            descriptions: Vec::new(),
            version: CTF_PROCESS_LIST_VERSION,
            version_clf: CtfVersion::new(0, 0, 0),
            prev_out_bd: BitDepth::Unknown,
        }
    }

    /// A transform of `ops`' data, with what the CLF keeps of `metadata`
    /// ([`from_metadata`](Self::from_metadata)).
    ///
    /// Port of `CTFReaderTransform::CTFReaderTransform(const OpRcPtrVec &, const
    /// FormatMetadataImpl &)` (CTFTransform.cpp:170-183 @ v2.5.2).
    pub fn from_ops(ops: &OpVec, metadata: &FormatMetadataImpl) -> Result<CtfReaderTransform> {
        let mut transform = CtfReaderTransform::new();
        transform.from_metadata(metadata)?;

        for op in ops.iter() {
            let op_data = op.data().clone();
            transform.ops.push(op_data);
        }
        Ok(transform)
    }

    /// Port of `CTFReaderTransform::getID` (CTFTransform.h:152-155 @ v2.5.2).
    pub fn get_id(&self) -> &[u8] {
        &self.id
    }

    /// Port of `CTFReaderTransform::setID` (CTFTransform.h:156-159 @ v2.5.2): a C string.
    pub fn set_id(&mut self, id: &[u8]) {
        self.id = c_str(id).to_vec();
    }

    /// Port of `CTFReaderTransform::getName` (CTFTransform.h:160-163 @ v2.5.2).
    pub fn get_name(&self) -> &[u8] {
        &self.name
    }

    /// Port of `CTFReaderTransform::setName` (CTFTransform.h:164-167 @ v2.5.2): a C string.
    pub fn set_name(&mut self, name: &[u8]) {
        self.name = c_str(name).to_vec();
    }

    /// Port of `CTFReaderTransform::getInverseOfId` (CTFTransform.h:168-171 @ v2.5.2).
    pub fn get_inverse_of_id(&self) -> &[u8] {
        &self.inverse_of_id
    }

    /// Port of `CTFReaderTransform::setInverseOfId` (CTFTransform.h:172-175 @ v2.5.2): a C
    /// string.
    pub fn set_inverse_of_id(&mut self, id: &[u8]) {
        self.inverse_of_id = c_str(id).to_vec();
    }

    /// Port of `CTFReaderTransform::getInfoMetadata` (CTFTransform.h:180-183 @ v2.5.2).
    pub fn get_info_metadata(&self) -> &FormatMetadataImpl {
        &self.info_metadata
    }

    /// Port of `CTFReaderTransform::getInfoMetadata` (CTFTransform.h:176-179 @ v2.5.2).
    pub fn get_info_metadata_mut(&mut self) -> &mut FormatMetadataImpl {
        &mut self.info_metadata
    }

    /// Port of `CTFReaderTransform::getOps` (CTFTransform.h:184-187 @ v2.5.2).
    pub fn get_ops(&self) -> &ConstOpDataVec {
        &self.ops
    }

    /// Port of `CTFReaderTransform::getOps` (CTFTransform.h:188-191 @ v2.5.2).
    pub fn get_ops_mut(&mut self) -> &mut ConstOpDataVec {
        &mut self.ops
    }

    /// Port of `CTFReaderTransform::getDescriptions` (CTFTransform.h:192-195 @ v2.5.2).
    pub fn get_descriptions(&self) -> &[Vec<u8>] {
        &self.descriptions
    }

    /// Port of `CTFReaderTransform::getDescriptions` (CTFTransform.h:196-199 @ v2.5.2).
    pub fn get_descriptions_mut(&mut self) -> &mut Vec<Vec<u8>> {
        &mut self.descriptions
    }

    /// Port of `CTFReaderTransform::getInputDescriptor` (CTFTransform.h:201-204 @ v2.5.2).
    pub fn get_input_descriptor(&self) -> &[u8] {
        &self.in_descriptor
    }

    /// Port of `CTFReaderTransform::setInputDescriptor` (CTFTransform.h:206-209 @ v2.5.2).
    pub fn set_input_descriptor(&mut self, input: &[u8]) {
        self.in_descriptor = input.to_vec();
    }

    /// Port of `CTFReaderTransform::getOutputDescriptor` (CTFTransform.h:211-214 @ v2.5.2).
    pub fn get_output_descriptor(&self) -> &[u8] {
        &self.out_descriptor
    }

    /// Port of `CTFReaderTransform::setOutputDescriptor` (CTFTransform.h:216-219 @ v2.5.2).
    pub fn set_output_descriptor(&mut self, out: &[u8]) {
        self.out_descriptor = out.to_vec();
    }

    /// Port of `CTFReaderTransform::setCTFVersion` (CTFTransform.cpp:185-188 @ v2.5.2).
    pub fn set_ctf_version(&mut self, ver: CtfVersion) {
        self.version = ver;
    }

    /// Port of `CTFReaderTransform::setCLFVersion` (CTFTransform.cpp:190-193 @ v2.5.2).
    pub fn set_clf_version(&mut self, ver: CtfVersion) {
        self.version_clf = ver;
    }

    /// Port of `CTFReaderTransform::getCTFVersion` (CTFTransform.cpp:195-198 @ v2.5.2).
    pub fn get_ctf_version(&self) -> CtfVersion {
        self.version
    }

    /// Port of `CTFReaderTransform::getCLFVersion` (CTFTransform.cpp:200-203 @ v2.5.2).
    pub fn get_clf_version(&self) -> CtfVersion {
        self.version_clf
    }

    /// Whether the file was a CLF: its CLF version isn't 0.
    ///
    /// Port of `CTFReaderTransform::isCLF` (CTFTransform.cpp:205-208 @ v2.5.2).
    pub fn is_clf(&self) -> bool {
        self.version_clf != CtfVersion::new(0, 0, 0)
    }

    /// Port of `CTFReaderTransform::setPreviousOutBitDepth` (CTFTransform.h:232-235 @ v2.5.2).
    pub fn set_previous_out_bit_depth(&mut self, out: BitDepth) {
        self.prev_out_bd = out;
    }

    /// Port of `CTFReaderTransform::getPreviousOutBitDepth` (CTFTransform.h:236-239 @ v2.5.2).
    pub fn get_previous_out_bit_depth(&self) -> BitDepth {
        self.prev_out_bd
    }

    /// Copies what the CLF spec keeps of `metadata`, the top level metadata of a ProcessList
    /// (the metadata of the process nodes is in their op data):
    /// - the attributes "name", "id" and "inverseOf"; other attributes are ignored;
    /// - the value of the first "InputDescriptor" and of the last "OutputDescriptor", without
    ///   their attributes or children;
    /// - the values of all "Description" children, without their attributes or children;
    /// - the "Info" children, merged into one with all their attributes and children;
    /// - any other children or attributes are ignored.
    ///
    /// Names compare ignoring case, but the Info children merge only when named exactly
    /// "Info" (`FormatMetadataImpl::combine`): another case fails.
    ///
    /// Port of `CTFReaderTransform::fromMetadata` (CTFTransform.cpp:430-464 @ v2.5.2).
    pub fn from_metadata(&mut self, metadata: &FormatMetadataImpl) -> Result<()> {
        // Name & id handled as attributes of the root metadata.
        self.name = metadata
            .get_attribute_value_string(Some(METADATA_NAME))
            .to_vec();
        self.id = metadata
            .get_attribute_value_string(Some(METADATA_ID))
            .to_vec();
        self.inverse_of_id = metadata
            .get_attribute_value_string(Some(ATTR_INVERSE_OF))
            .to_vec();

        // Preserve first InputDescriptor, last OutputDescriptor, and all Descriptions.
        self.in_descriptor = c_str(get_first_element_value(
            metadata.get_children_elements(),
            METADATA_INPUT_DESCRIPTOR,
        ))
        .to_vec();
        self.out_descriptor = c_str(get_last_element_value(
            metadata.get_children_elements(),
            METADATA_OUTPUT_DESCRIPTOR,
        ))
        .to_vec();
        get_elements_values(
            metadata.get_children_elements(),
            METADATA_DESCRIPTION,
            &mut self.descriptions,
        );

        // Combine all Info elements.
        for elt in metadata.get_children_elements() {
            if strcasecmp(elt.get_element_name(), METADATA_INFO).is_eq() {
                self.info_metadata.combine(elt)?;
            }
        }
        Ok(())
    }

    /// Adds the transform's ProcessList information to `metadata`: the non-empty name, id
    /// and inverseOf attributes, the non-empty descriptors, the descriptions, and the info
    /// when it has attributes, children or a value.
    ///
    /// Port of `CTFReaderTransform::toMetadata` (CTFTransform.cpp:485-504 @ v2.5.2), with
    /// `AddNonEmptyElement` and `AddNonEmptyAttribute` (466-483).
    pub fn to_metadata(&self, metadata: &mut FormatMetadataImpl) -> Result<()> {
        // Put CTF processList information into the FormatMetadata.
        add_non_empty_attribute(metadata, METADATA_NAME, self.get_name())?;
        add_non_empty_attribute(metadata, METADATA_ID, self.get_id())?;
        add_non_empty_attribute(metadata, ATTR_INVERSE_OF, self.get_inverse_of_id())?;

        add_non_empty_element(
            metadata,
            METADATA_INPUT_DESCRIPTOR,
            self.get_input_descriptor(),
        )?;
        add_non_empty_element(
            metadata,
            METADATA_OUTPUT_DESCRIPTOR,
            self.get_output_descriptor(),
        )?;
        for desc in &self.descriptions {
            metadata.add_child_element(Some(METADATA_DESCRIPTION), Some(desc))?;
        }
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

/// `typedef OCIO_SHARED_PTR<CTFReaderTransform> CTFReaderTransformPtr` (CTFTransform.h:263).
pub type CtfReaderTransformPtr = Arc<CtfReaderTransform>;

/// Adds the value of each element named `name` (ignoring case) to `values`.
///
/// Port of `GetElementsValues` (CTFTransform.cpp:210-221 @ v2.5.2). The values are C strings
/// (`getElementValue`).
pub fn get_elements_values(elements: &Elements, name: &[u8], values: &mut Vec<Vec<u8>>) {
    for element in elements {
        if strcasecmp(name, element.get_element_name()).is_eq() {
            values.push(c_str(element.get_element_value()).to_vec());
        }
    }
}

/// The value of the first element named `name` (ignoring case), or "".
///
/// Port of `GetFirstElementValue` (CTFTransform.cpp:405-415 @ v2.5.2).
fn get_first_element_value<'a>(elements: &'a Elements, name: &[u8]) -> &'a [u8] {
    for it in elements {
        if strcasecmp(name, it.get_element_name()).is_eq() {
            return it.get_element_value();
        }
    }
    b""
}

/// The value of the last element named `name` (ignoring case), or "".
///
/// Port of `GetLastElementValue` (CTFTransform.cpp:417-427 @ v2.5.2).
fn get_last_element_value<'a>(elements: &'a Elements, name: &[u8]) -> &'a [u8] {
    for it in elements.iter().rev() {
        if strcasecmp(name, it.get_element_name()).is_eq() {
            return it.get_element_value();
        }
    }
    b""
}

/// Port of `AddNonEmptyElement` (CTFTransform.cpp:468-474 @ v2.5.2).
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

/// Port of `AddNonEmptyAttribute` (CTFTransform.cpp:476-482 @ v2.5.2).
fn add_non_empty_attribute(
    metadata: &mut FormatMetadataImpl,
    name: &[u8],
    value: &[u8],
) -> Result<()> {
    if !value.is_empty() {
        metadata.add_attribute(Some(name), Some(value))?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "ctf_transform_tests.rs"]
mod tests;
