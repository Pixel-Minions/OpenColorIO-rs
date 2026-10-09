// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The CLF/CTF reader's elements: the ProcessList, its metadata, Info and descriptors, and
//! the versioned choice of each operator's reader.
//!
//! Port of `src/OpenColorIO/fileformats/ctf/CTFReaderHelper.h` and `CTFReaderHelper.cpp`
//! (@ v2.5.2), as far as WP 4.5b: the operators' readers come with 4.5c-4.5i, and until then
//! [`get_reader`] names the reader a file asks for.
//!
//! **Sharing.** Upstream's parser, ProcessList element and operator elements share one
//! `CTFReaderTransform` (`CTFReaderTransformPtr`, a `shared_ptr`); here the parse shares a
//! [`CtfReaderTransformRc`] (`Rc<RefCell<...>>`), which the reader hands on as a
//! [`CtfReaderTransformPtr`] once the file is read. An element's `getIdentifier` returns a
//! reference upstream; here the elements keep a copy of what they set.

use std::any::Any;
use std::cell::{RefCell, RefMut};
use std::ffi::c_ulong;
use std::rc::Rc;
use std::sync::Arc;

use ocio_formats::fileformats::xmlutils::xml_reader_helper::{
    ContainerEltRcPtr, XmlReaderComplexEltBase, XmlReaderContainerElt, XmlReaderElement,
    XmlReaderElementBase, XmlReaderPlainElt, XmlReaderPlainEltBase,
};
use ocio_formats::fileformats::xmlutils::xml_reader_utils::{
    ATTR_ID, ATTR_NAME, find_next_token_start, get_next_number, get_numbers, truncate_string,
};
use ocio_ops::bit_depth_utils::get_bit_depth_max_value;
use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::format_metadata::FormatMetadataImpl;
use ocio_ops::logging::{is_debug_logging_enabled, log_debug};
use ocio_ops::op_data::{OpData, OpDataType};
use ocio_ops::open_color_types::{BitDepth, bit_depth_to_string};
use ocio_ops::ops::matrix::matrix_op_data::MatrixOpData;
use ocio_ops::ops::range::range_op_data::RangeOpData;
use ocio_ops::platform::strcasecmp;
use ocio_ops::utils::cscan::{ScanArg, ScanCrt, sscanf};
use ocio_ops::utils::string_utils::{c_str, lower};
use ocio_ops::{Exception, Result};

use super::ctf_reader_utils::{
    ATTR_BITDEPTH_IN, ATTR_BITDEPTH_OUT, ATTR_BYPASS, ATTR_COMP_CLF_VERSION, ATTR_DIMENSION,
    ATTR_INVERSE_OF, ATTR_STYLE, ATTR_VERSION, TAG_MAX_IN_VALUE, TAG_MAX_OUT_VALUE,
    TAG_MIN_IN_VALUE, TAG_MIN_OUT_VALUE, TAG_PROCESS_LIST,
};
use super::ctf_transform::{
    CTF_INFO_ELEMENT_VERSION, CTF_PROCESS_LIST_VERSION, CTF_PROCESS_LIST_VERSION_1_2,
    CTF_PROCESS_LIST_VERSION_1_3, CTF_PROCESS_LIST_VERSION_1_4, CTF_PROCESS_LIST_VERSION_1_5,
    CTF_PROCESS_LIST_VERSION_1_6, CTF_PROCESS_LIST_VERSION_1_7, CTF_PROCESS_LIST_VERSION_1_8,
    CTF_PROCESS_LIST_VERSION_2_0, CTF_PROCESS_LIST_VERSION_2_5, CtfReaderTransform, CtfVersion,
};

/// The transform a parse fills, which the parser and the elements share.
pub type CtfReaderTransformRc = Rc<RefCell<CtfReaderTransform>>;

/// Whether two C strings are equal ignoring ASCII case: `0 == Platform::Strcasecmp(a, b)`.
pub(crate) fn equal_ignoring_case(a: &[u8], b: &[u8]) -> bool {
    strcasecmp(c_str(a), c_str(b)).is_eq()
}

/// The ProcessList element: the transform's attributes and versions.
///
/// Port of `CTFReaderTransformElt` (CTFReaderHelper.h:32-77, CTFReaderHelper.cpp:19-261 @
/// v2.5.2).
#[derive(Debug)]
pub struct CtfReaderTransformElt {
    element: XmlReaderElementBase,
    /// `m_transform`: the associated Transform.
    transform: CtfReaderTransformRc,
    /// `m_isCLF`: is it a clf file? Or is a clf parser requested.
    is_clf: bool,
    /// The transform's id, which `getIdentifier` returns.
    id: Vec<u8>,
}

impl CtfReaderTransformElt {
    /// Port of `CTFReaderTransformElt::CTFReaderTransformElt` (CTFReaderHelper.cpp:19-27 @
    /// v2.5.2).
    pub fn new(
        name: &[u8],
        xml_line_number: u32,
        xml_file: &[u8],
        is_clf: bool,
    ) -> CtfReaderTransformElt {
        CtfReaderTransformElt {
            element: XmlReaderElementBase::new(name, xml_line_number, xml_file),
            transform: Rc::new(RefCell::new(CtfReaderTransform::new())),
            is_clf,
            id: Vec::new(),
        }
    }

    /// The exception of `ThrowM(*this, ...)`: the parts streamed, then `throwMessage`.
    ///
    /// Port of `ThrowM` (CTFReaderHelper.cpp:34-56 @ v2.5.2).
    fn throw_m(&self, parts: &[&[u8]]) -> Exception {
        self.throw_message(&parts.concat())
    }

    /// Get its parent which is a Transform.
    ///
    /// Port of `CTFReaderTransformElt::getTransform` (CTFReaderHelper.cpp:223-226 @ v2.5.2).
    pub fn get_transform(&self) -> &CtfReaderTransformRc {
        &self.transform
    }

    /// Set the current transform CTF version.
    ///
    /// Port of `CTFReaderTransformElt::setVersion` (CTFReaderHelper.cpp:234-241 @ v2.5.2).
    pub fn set_version(&mut self, ver: CtfVersion) -> Result<()> {
        if CTF_PROCESS_LIST_VERSION < ver {
            return Err(self.throw_m(&[
                b"Unsupported transform file version '",
                ver.to_string().as_bytes(),
                b"' supplied.",
            ]));
        }
        self.transform.borrow_mut().set_ctf_version(ver);
        Ok(())
    }

    /// Port of `CTFReaderTransformElt::setCLFVersion` (CTFReaderHelper.cpp:243-246 @ v2.5.2).
    pub fn set_clf_version(&mut self, ver: CtfVersion) {
        self.transform.borrow_mut().set_clf_version(ver);
    }

    /// Get current xml transform version.
    ///
    /// Port of `CTFReaderTransformElt::getVersion` (CTFReaderHelper.cpp:248-251 @ v2.5.2).
    pub fn get_version(&self) -> CtfVersion {
        self.transform.borrow().get_ctf_version()
    }

    /// Port of `CTFReaderTransformElt::getCLFVersion` (CTFReaderHelper.cpp:253-256 @ v2.5.2).
    pub fn get_clf_version(&self) -> CtfVersion {
        self.transform.borrow().get_clf_version()
    }

    /// Whether the transform is a CLF's: its CLF version (not the parser's request).
    ///
    /// Port of `CTFReaderTransformElt::isCLF` (CTFReaderHelper.cpp:258-261 @ v2.5.2).
    pub fn is_clf(&self) -> bool {
        self.transform.borrow().is_clf()
    }
}

impl XmlReaderElement for CtfReaderTransformElt {
    fn element(&self) -> &XmlReaderElementBase {
        &self.element
    }
    fn element_mut(&mut self) -> &mut XmlReaderElementBase {
        &mut self.element
    }

    /// The ProcessList's attributes: `id` (required), `name`, `inverseOf`, and either
    /// `version` (CTF) or `compCLFversion` (CLF, translated to a CTF version); `xmlns` is
    /// ignored and other attributes are logged.
    ///
    /// Port of `CTFReaderTransformElt::start` (CTFReaderHelper.cpp:58-212 @ v2.5.2).
    fn start(&mut self, atts: &[&[u8]]) -> Result<()> {
        let mut is_id_found = false;
        let mut is_version_found = false;
        let mut is_clf_version_found = false;
        let mut requested_version = CtfVersion::new(0, 0, 0);
        let mut requested_clf_version = CtfVersion::new(0, 0, 0);

        let mut i = 0;
        while i < atts.len() {
            let (att, value) = (atts[i], atts[i + 1]);
            if equal_ignoring_case(ATTR_ID, att) {
                if value.is_empty() {
                    return Err(
                        self.throw_message(b"Required attribute 'id' does not have a value.")
                    );
                }

                self.transform.borrow_mut().set_id(value);
                self.id = value.to_vec();
                is_id_found = true;
            } else if equal_ignoring_case(ATTR_NAME, att) {
                if value.is_empty() {
                    return Err(self.throw_message(
                        b"If the attribute 'name' is present, it must have a value.",
                    ));
                }

                self.transform.borrow_mut().set_name(value);
            } else if equal_ignoring_case(ATTR_INVERSE_OF, att) {
                if value.is_empty() {
                    return Err(self.throw_message(
                        b"If the attribute 'inverseOf' is present, it must have a value.",
                    ));
                }

                self.transform.borrow_mut().set_inverse_of_id(value);
            } else if equal_ignoring_case(ATTR_VERSION, att) {
                if is_clf_version_found {
                    return Err(self
                        .throw_message(b"'compCLFversion' and 'Version' cannot both be present."));
                }
                if is_version_found {
                    return Err(self.throw_message(b"'Version' can only be there once."));
                }

                if value.is_empty() {
                    return Err(self.throw_message(
                        b"If the attribute 'version' is present, it must have a value.",
                    ));
                }

                requested_version = match CtfVersion::read_version(value) {
                    Ok(version) => version,
                    Err(ce) => return Err(self.throw_message(ce.what())),
                };

                is_version_found = true;
            } else if equal_ignoring_case(ATTR_COMP_CLF_VERSION, att) {
                if is_clf_version_found {
                    return Err(self.throw_message(b"'compCLFversion' can only be there once."));
                }
                if is_version_found {
                    return Err(self
                        .throw_message(b"'compCLFversion' and 'Version' cannot be both present."));
                }

                if value.is_empty() {
                    return Err(self.throw_message(
                        b"Required attribute 'compCLFversion' does not have a value.",
                    ));
                }

                requested_clf_version = match CtfVersion::read_version(value) {
                    Ok(version) => version,
                    Err(ce) => return Err(self.throw_message(ce.what())),
                };

                // Translate to CTF.

                let max_clf = CtfVersion::new(3, 0, 0);
                if max_clf < requested_clf_version {
                    return Err(self.throw_m(&[
                        b"Unsupported transform file version '",
                        value,
                        b"' supplied.",
                    ]));
                }
                // We currently interpret CLF versions <= 2.0 as CTF version 1.7.
                if requested_clf_version <= CtfVersion::new(2, 0, 0) {
                    requested_version = CTF_PROCESS_LIST_VERSION_1_7;
                } else {
                    requested_version = CTF_PROCESS_LIST_VERSION_2_0;
                }

                is_version_found = true;
                is_clf_version_found = true;
                // Handle as CLF.
                self.is_clf = true;
            } else if equal_ignoring_case(b"xmlns", att) {
                // Ignore.
            } else {
                self.element.log_parameter_warning(att);
            }

            i += 2;
        }

        // Check mandatory elements.
        if !is_id_found {
            return Err(self.throw_message(b"Required attribute 'id' is missing."));
        }

        // Transform file format with no version means that the CTF format is 1.2.
        if !is_version_found {
            if self.is_clf && !is_clf_version_found {
                return Err(self.throw_message(b"Required attribute 'compCLFversion' is missing."));
            }
            self.set_version(CTF_PROCESS_LIST_VERSION_1_2)?;
        } else {
            self.set_version(requested_version)?;
            if self.is_clf {
                self.set_clf_version(requested_clf_version);
            }
        }
        Ok(())
    }

    /// Port of `CTFReaderTransformElt::end` (CTFReaderHelper.cpp:214-216 @ v2.5.2).
    fn end(&mut self) -> Result<()> {
        Ok(())
    }

    fn is_container(&self) -> bool {
        true
    }

    /// The transform's id.
    ///
    /// Port of `CTFReaderTransformElt::getIdentifier` (CTFReaderHelper.cpp:29-32 @ v2.5.2).
    fn get_identifier(&self) -> &[u8] {
        &self.id
    }

    /// Port of `CTFReaderTransformElt::getTypeName` (CTFReaderHelper.cpp:228-232 @ v2.5.2).
    fn get_type_name(&self) -> &[u8] {
        TAG_PROCESS_LIST
    }

    fn as_container_mut(&mut self) -> Option<&mut dyn XmlReaderContainerElt> {
        Some(self)
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl XmlReaderContainerElt for CtfReaderTransformElt {
    /// A description of the ProcessList.
    ///
    /// Port of `CTFReaderTransformElt::appendMetadata` (CTFReaderHelper.cpp:218-221 @
    /// v2.5.2).
    fn append_metadata(&mut self, _name: &[u8], value: &[u8]) -> Result<()> {
        self.transform
            .borrow_mut()
            .get_descriptions_mut()
            .push(value.to_vec());
        Ok(())
    }
}

/// A metadata element: an `Info` element of the ProcessList (`info`), or any element inside
/// one, which keeps its name, attributes, text and children as a [`FormatMetadataImpl`].
///
/// Port of `CTFReaderMetadataElt` and `CTFReaderInfoElt` (CTFReaderHelper.h:79-118,
/// CTFReaderHelper.cpp:675-798 @ v2.5.2). Upstream's `CTFReaderInfoElt` derives from
/// `CTFReaderMetadataElt`: both are this type, which the parser's casts find.
#[derive(Debug)]
pub struct CtfReaderMetadataElt {
    complex: XmlReaderComplexEltBase,
    /// `m_metadata`.
    metadata: FormatMetadataImpl,
    /// Whether the element is a `CTFReaderInfoElt`.
    info: bool,
}

impl CtfReaderMetadataElt {
    /// Port of `CTFReaderMetadataElt::CTFReaderMetadataElt` (CTFReaderHelper.cpp:675-682 @
    /// v2.5.2).
    pub fn new(
        name: &[u8],
        parent: ContainerEltRcPtr,
        xml_line_number: u32,
        xml_file: &[u8],
    ) -> Result<CtfReaderMetadataElt> {
        Ok(CtfReaderMetadataElt {
            complex: XmlReaderComplexEltBase::new(name, Some(parent), xml_line_number, xml_file),
            metadata: FormatMetadataImpl::new(name, b"")?,
            info: false,
        })
    }

    /// An `Info` element.
    ///
    /// Port of `CTFReaderInfoElt::CTFReaderInfoElt` (CTFReaderHelper.cpp:723-729 @ v2.5.2).
    pub fn new_info(
        name: &[u8],
        parent: ContainerEltRcPtr,
        xml_line_number: u32,
        xml_file: &[u8],
    ) -> Result<CtfReaderMetadataElt> {
        let mut elt = CtfReaderMetadataElt::new(name, parent, xml_line_number, xml_file)?;
        elt.info = true;
        Ok(elt)
    }

    /// Appends the text to the element's value.
    ///
    /// Port of `CTFReaderMetadataElt::setRawData` (CTFReaderHelper.cpp:715-719 @ v2.5.2).
    pub fn set_raw_data(&mut self, s: &[u8], _xml_line: u32) -> Result<()> {
        let new_value = [self.metadata.get_element_value(), s].concat();
        self.metadata.set_element_value(Some(c_str(&new_value)))
    }

    /// Port of `CTFReaderMetadataElt::getMetadata` (CTFReaderHelper.h:98 @ v2.5.2).
    pub fn get_metadata(&self) -> &FormatMetadataImpl {
        &self.metadata
    }

    /// Port of `CTFReaderMetadataElt::getMetadata` (CTFReaderHelper.h:98 @ v2.5.2).
    pub fn get_metadata_mut(&mut self) -> &mut FormatMetadataImpl {
        &mut self.metadata
    }

    /// The attributes, each with a value, as the element's.
    ///
    /// Port of `CTFReaderMetadataElt::start` (CTFReaderHelper.cpp:688-699 @ v2.5.2).
    fn metadata_start(&mut self, atts: &[&[u8]]) -> Result<()> {
        let mut i = 0;
        while i < atts.len() && !atts[i].is_empty() {
            if !atts[i + 1].is_empty() {
                self.metadata
                    .add_attribute(Some(atts[i]), Some(atts[i + 1]))?;
            }
            i += 2;
        }
        Ok(())
    }
}

/// Checks the version of an `Info` element, its first attribute if named `version`: a
/// number (read as `%d`) whose integer part is at most the supported version's.
///
/// Port of `validateInfoElementVersion` (CTFReaderHelper.cpp:737-779 @ v2.5.2).
fn validate_info_element_version(
    version_attr: Option<&[u8]>,
    version_value: Option<&[u8]>,
) -> Result<()> {
    // There are 3 rules for an <Info> element version attribute to be valid :
    //
    // 1- Not exist. No version means version 1.0. It will always be valid.
    // 2- Be of the following format : MAJOR.MINOR ( i.e '3.0' )
    // 3- The major version should be equal or smaller then the current major version.
    //
    // Note: The minor version is not taken into account when validating the version. The
    // minor version is only for tracking purposes.
    // Note: <Info> is not part of CLF.
    let Some(version_attr) = version_attr.filter(|a| !a.is_empty()) else {
        return Ok(());
    };
    if !equal_ignoring_case(ATTR_VERSION, version_attr) {
        return Ok(());
    }
    let Some(version_value) = version_value.filter(|v| !v.is_empty()) else {
        return Err(Exception::new(
            "CTF reader. Invalid Info element version attribute.",
        ));
    };

    let mut fver = CTF_INFO_ELEMENT_VERSION as i32;
    if 0 == sscanf(
        ScanCrt::NATIVE,
        c_str(version_value),
        b"%d",
        &mut [ScanArg::Int(&mut fver)],
    ) {
        let mut oss = OStringStream::new(Crt::NATIVE);
        oss.put_str("CTF reader. Invalid Info element version attribute: ");
        oss.put_c_str(version_value);
        oss.put_str(" .");
        return Err(Exception::new(oss.into_bytes()));
    }

    // Always compare with 'ints' so we do not include minor versions in the test. An info
    // version of 3.9 should be supported in a SynColor with the current version of 3.0.
    if fver > CTF_INFO_ELEMENT_VERSION as i32 {
        let mut oss = OStringStream::new(Crt::NATIVE);
        oss.put_str("CTF reader. Unsupported Info element version attribute: ");
        oss.put_c_str(version_value);
        oss.put_str(" .");
        return Err(Exception::new(oss.into_bytes()));
    }
    Ok(())
}

impl XmlReaderElement for CtfReaderMetadataElt {
    fn element(&self) -> &XmlReaderElementBase {
        &self.complex.element
    }
    fn element_mut(&mut self) -> &mut XmlReaderElementBase {
        &mut self.complex.element
    }

    /// An `Info` element checks its version first.
    ///
    /// Port of `CTFReaderMetadataElt::start` and `CTFReaderInfoElt::start`
    /// (CTFReaderHelper.cpp:688-699, 781-789 @ v2.5.2).
    fn start(&mut self, atts: &[&[u8]]) -> Result<()> {
        if self.info {
            // Validate the version number. The version should be the first attribute.
            // Might throw.
            validate_info_element_version(atts.first().copied(), atts.get(1).copied())?;
        }

        // Let the base class store the attributes in the FormatMetadataImpl element.
        self.metadata_start(atts)
    }

    /// A metadata element adds itself to its parent's children; an `Info` element becomes
    /// the transform's info metadata.
    ///
    /// Port of `CTFReaderMetadataElt::end` and `CTFReaderInfoElt::end`
    /// (CTFReaderHelper.cpp:701-708, 791-798 @ v2.5.2).
    fn end(&mut self) -> Result<()> {
        let parent = self
            .complex
            .get_parent()
            .expect("a metadata element has a parent")
            .clone();
        let mut parent = parent.borrow_mut();
        if self.info {
            if let Some(transform_elt) = parent.as_any_mut().downcast_mut::<CtfReaderTransformElt>()
            {
                *transform_elt
                    .get_transform()
                    .borrow_mut()
                    .get_info_metadata_mut() = self.metadata.clone();
            }
        } else if let Some(metadata_elt) =
            parent.as_any_mut().downcast_mut::<CtfReaderMetadataElt>()
        {
            metadata_elt
                .get_metadata_mut()
                .get_children_elements_mut()
                .push(self.metadata.clone());
        }
        Ok(())
    }

    fn is_container(&self) -> bool {
        true
    }

    /// Port of `CTFReaderMetadataElt::getIdentifier` (CTFReaderHelper.cpp:710-713 @ v2.5.2).
    fn get_identifier(&self) -> &[u8] {
        self.get_name()
    }

    /// Port of `XmlReaderComplexElt::getTypeName` (XMLReaderHelper.h:337-340 @ v2.5.2).
    fn get_type_name(&self) -> &[u8] {
        self.get_name()
    }

    fn as_container_mut(&mut self) -> Option<&mut dyn XmlReaderContainerElt> {
        Some(self)
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl XmlReaderContainerElt for CtfReaderMetadataElt {
    /// Port of `XmlReaderComplexElt::appendMetadata` (XMLReaderHelper.h:342-345 @ v2.5.2).
    fn append_metadata(&mut self, _name: &[u8], _value: &[u8]) -> Result<()> {
        Ok(())
    }
}

/// Which descriptor of the transform a [`CtfReaderDescriptorElt`] reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Descriptor {
    /// `CTFReaderInputDescriptorElt`.
    Input,
    /// `CTFReaderOutputDescriptorElt`.
    Output,
}

/// The ProcessList's `InputDescriptor` or `OutputDescriptor`: its text is appended to the
/// transform's.
///
/// Port of `CTFReaderInputDescriptorElt` and `CTFReaderOutputDescriptorElt`
/// (CTFReaderHelper.h:120-190 @ v2.5.2).
#[derive(Debug)]
pub struct CtfReaderDescriptorElt {
    plain: XmlReaderPlainEltBase,
    descriptor: Descriptor,
}

impl CtfReaderDescriptorElt {
    /// Port of the constructors of `CTFReaderInputDescriptorElt` and
    /// `CTFReaderOutputDescriptorElt` (CTFReaderHelper.h:124-130, 160-166 @ v2.5.2).
    pub fn new(
        name: &[u8],
        parent: ContainerEltRcPtr,
        xml_line_number: u32,
        xml_file: &[u8],
        descriptor: Descriptor,
    ) -> CtfReaderDescriptorElt {
        CtfReaderDescriptorElt {
            plain: XmlReaderPlainEltBase::new(name, parent, xml_line_number, xml_file),
            descriptor,
        }
    }
}

impl XmlReaderElement for CtfReaderDescriptorElt {
    fn element(&self) -> &XmlReaderElementBase {
        &self.plain.element
    }
    fn element_mut(&mut self) -> &mut XmlReaderElementBase {
        &mut self.plain.element
    }
    fn start(&mut self, _atts: &[&[u8]]) -> Result<()> {
        Ok(())
    }
    fn end(&mut self) -> Result<()> {
        Ok(())
    }
    fn is_container(&self) -> bool {
        false
    }
    fn get_identifier(&self) -> &[u8] {
        self.get_name()
    }
    fn get_type_name(&self) -> &[u8] {
        self.get_name()
    }
    fn as_plain_mut(&mut self) -> Option<&mut dyn XmlReaderPlainElt> {
        Some(self)
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl XmlReaderPlainElt for CtfReaderDescriptorElt {
    /// Port of `CTFReaderInputDescriptorElt::setRawData` and
    /// `CTFReaderOutputDescriptorElt::setRawData` (CTFReaderHelper.h:144-153, 180-189 @
    /// v2.5.2): the parent is the ProcessList's element (the parser makes a descriptor only
    /// under it).
    fn set_raw_data(&mut self, s: &[u8], _xml_line: u32) -> Result<()> {
        let parent = self.plain.get_parent().borrow();
        let transform_elt = parent
            .as_any()
            .downcast_ref::<CtfReaderTransformElt>()
            .expect("a descriptor is under the ProcessList");
        let mut transform = transform_elt.get_transform().borrow_mut();
        match self.descriptor {
            Descriptor::Input => {
                let value = [transform.get_input_descriptor(), s].concat();
                transform.set_input_descriptor(&value);
            }
            Descriptor::Output => {
                let value = [transform.get_output_descriptor(), s].concat();
                transform.set_output_descriptor(&value);
            }
        }
        Ok(())
    }
    fn get_parent(&self) -> &ContainerEltRcPtr {
        self.plain.get_parent()
    }
}

/// The operators the CLF/CTF reader knows.
///
/// Port of `CTFReaderOpElt::Type` (CTFReaderHelper.h:312-336 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CtfReaderOpType {
    /// `CDLType`.
    Cdl,
    /// `Lut1DType`.
    Lut1D,
    /// `Lut3DType`.
    Lut3D,
    /// `MatrixType`.
    Matrix,
    /// `RangeType`.
    Range,
    /// `ACESType`.
    Aces,
    /// `ExposureContrastType`.
    ExposureContrast,
    /// `FixedFunctionType`.
    FixedFunction,
    /// `FunctionType`.
    Function,
    /// `GammaType`.
    Gamma,
    /// `GradingPrimaryType`.
    GradingPrimary,
    /// `GradingRGBCurveType`.
    GradingRgbCurve,
    /// `GradingHueCurveType`.
    GradingHueCurve,
    /// `GradingToneType`.
    GradingTone,
    /// `InvLut1DType`.
    InvLut1D,
    /// `InvLut3DType`.
    InvLut3D,
    /// `LogType`.
    Log,
    /// `ReferenceType`.
    Reference,
}

/// The reader classes of the operators, one per version range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CtfReaderKind {
    /// `CTFReaderACESElt`.
    Aces,
    /// `CTFReaderCDLElt`.
    Cdl,
    /// `CTFReaderExposureContrastElt`.
    ExposureContrast,
    /// `CTFReaderFixedFunctionElt`.
    FixedFunction,
    /// `CTFReaderFunctionElt`.
    Function,
    /// `CTFReaderGammaElt`.
    Gamma,
    /// `CTFReaderGammaElt_1_5`.
    Gamma1_5,
    /// `CTFReaderGammaElt_CTF_2_0`.
    GammaCtf2_0,
    /// `CTFReaderGammaElt_CLF_3_0`.
    GammaClf3_0,
    /// `CTFReaderGradingHueCurveElt`.
    GradingHueCurve,
    /// `CTFReaderGradingPrimaryElt`.
    GradingPrimary,
    /// `CTFReaderGradingRGBCurveElt`.
    GradingRgbCurve,
    /// `CTFReaderGradingToneElt`.
    GradingTone,
    /// `CTFReaderInvLut1DElt`.
    InvLut1D,
    /// `CTFReaderInvLut3DElt`.
    InvLut3D,
    /// `CTFReaderLogElt`.
    Log,
    /// `CTFReaderLogElt_2_0`.
    Log2_0,
    /// `CTFReaderLut1DElt`.
    Lut1D,
    /// `CTFReaderLut1DElt_1_4`.
    Lut1D1_4,
    /// `CTFReaderLut1DElt_1_7`.
    Lut1D1_7,
    /// `CTFReaderLut3DElt`.
    Lut3D,
    /// `CTFReaderLut3DElt_1_7`.
    Lut3D1_7,
    /// `CTFReaderMatrixElt`.
    Matrix,
    /// `CTFReaderMatrixElt_1_3`.
    Matrix1_3,
    /// `CTFReaderRangeElt`.
    Range,
    /// `CTFReaderRangeElt_1_7`.
    Range1_7,
    /// `CTFReaderReferenceElt`.
    Reference,
}

/// The reader of an operator of `op_type` in a file of `version` (a CLF's if `is_clf`), or
/// none if that version doesn't support it.
///
/// Port of `CTFReaderOpElt::GetReader` (CTFReaderHelper.cpp:1020-1183 @ v2.5.2) and of the
/// macros it uses (`ADD_READER_FOR_VERSIONS_UP_TO`, `ADD_DEFAULT_READER`,
/// `ADD_READER_FOR_VERSIONS_STARTING_AT`, `ADD_READER_FOR_VERSIONS_BETWEEN`,
/// CTFReaderHelper.cpp:997-1018): the first range that holds `version` picks the reader.
pub fn get_reader(
    op_type: CtfReaderOpType,
    version: CtfVersion,
    is_clf: bool,
) -> Option<CtfReaderKind> {
    use CtfReaderKind as K;
    // `ADD_READER_FOR_VERSIONS_UP_TO`.
    let up_to = |vers: CtfVersion| version <= vers;
    // `ADD_DEFAULT_READER`.
    let default = || version <= CTF_PROCESS_LIST_VERSION;
    // `ADD_READER_FOR_VERSIONS_STARTING_AT`.
    let starting_at = |vers: CtfVersion| version >= vers && version <= CTF_PROCESS_LIST_VERSION;
    // `ADD_READER_FOR_VERSIONS_BETWEEN`.
    let between = |vers1: CtfVersion, vers2: CtfVersion| version >= vers1 && version <= vers2;

    match op_type {
        CtfReaderOpType::Aces => starting_at(CTF_PROCESS_LIST_VERSION_1_5).then_some(K::Aces),
        CtfReaderOpType::Cdl => {
            // Note: CLF style name support was not added until version 1.7, but no point
            // creating a separate version just for that.
            starting_at(CTF_PROCESS_LIST_VERSION_1_3).then_some(K::Cdl)
        }
        CtfReaderOpType::ExposureContrast => (!is_clf && default()).then_some(K::ExposureContrast),
        CtfReaderOpType::FixedFunction => {
            (!is_clf && starting_at(CTF_PROCESS_LIST_VERSION_2_0)).then_some(K::FixedFunction)
        }
        CtfReaderOpType::Function => {
            (!is_clf && starting_at(CTF_PROCESS_LIST_VERSION_1_6)).then_some(K::Function)
        }
        CtfReaderOpType::Gamma => {
            if !is_clf {
                // If the version is 1.4 or less, then use GammaElt. This reader forces the
                // alpha transformation to be the identity.
                if up_to(CTF_PROCESS_LIST_VERSION_1_4) {
                    Some(K::Gamma)
                }
                // CTF 1.5 and onwards handles alpha component.
                else if up_to(CTF_PROCESS_LIST_VERSION_1_8) {
                    Some(K::Gamma1_5)
                }
                // CTF v2 and CLF v3 add styles and changes names.
                else {
                    starting_at(CTF_PROCESS_LIST_VERSION_2_0).then_some(K::GammaCtf2_0)
                }
            } else {
                // Introduced with CLF v3. CLF does not handle alpha component.
                starting_at(CTF_PROCESS_LIST_VERSION_2_0).then_some(K::GammaClf3_0)
            }
        }
        CtfReaderOpType::GradingHueCurve => {
            (!is_clf && starting_at(CTF_PROCESS_LIST_VERSION_2_5)).then_some(K::GradingHueCurve)
        }
        CtfReaderOpType::GradingPrimary => {
            (!is_clf && starting_at(CTF_PROCESS_LIST_VERSION_2_0)).then_some(K::GradingPrimary)
        }
        CtfReaderOpType::GradingRgbCurve => {
            (!is_clf && starting_at(CTF_PROCESS_LIST_VERSION_2_0)).then_some(K::GradingRgbCurve)
        }
        CtfReaderOpType::GradingTone => {
            (!is_clf && starting_at(CTF_PROCESS_LIST_VERSION_2_0)).then_some(K::GradingTone)
        }
        CtfReaderOpType::InvLut1D => {
            (!is_clf && starting_at(CTF_PROCESS_LIST_VERSION_1_3)).then_some(K::InvLut1D)
        }
        CtfReaderOpType::InvLut3D => {
            (!is_clf && starting_at(CTF_PROCESS_LIST_VERSION_1_6)).then_some(K::InvLut3D)
        }
        CtfReaderOpType::Log => {
            if !is_clf && between(CTF_PROCESS_LIST_VERSION_1_3, CTF_PROCESS_LIST_VERSION_1_8) {
                Some(K::Log)
            }
            // CLF v3 (CTF v2) adds log support and adds new camera styles.
            else {
                starting_at(CTF_PROCESS_LIST_VERSION_2_0).then_some(K::Log2_0)
            }
        }
        CtfReaderOpType::Lut1D => {
            if up_to(CTF_PROCESS_LIST_VERSION_1_3) {
                Some(K::Lut1D)
            }
            // Adding hue_adjust attribute.
            else if up_to(CTF_PROCESS_LIST_VERSION_1_4) {
                Some(K::Lut1D1_4)
            }
            // Adding basic IndexMap element.
            else {
                default().then_some(K::Lut1D1_7)
            }
        }
        CtfReaderOpType::Lut3D => {
            if up_to(CTF_PROCESS_LIST_VERSION_1_6) {
                Some(K::Lut3D)
            }
            // Adding basic IndexMap element.
            else {
                default().then_some(K::Lut3D1_7)
            }
        }
        CtfReaderOpType::Matrix => {
            // If the version is 1.2 or less, then use MatrixElt.
            if up_to(CTF_PROCESS_LIST_VERSION_1_2) {
                Some(K::Matrix)
            }
            // If the version is 1.3 or more, then use MatrixElt_1_3.
            else {
                default().then_some(K::Matrix1_3)
            }
        }
        CtfReaderOpType::Range => {
            if up_to(CTF_PROCESS_LIST_VERSION_1_6) {
                Some(K::Range)
            }
            // Adding noClamp style.
            else {
                default().then_some(K::Range1_7)
            }
        }
        CtfReaderOpType::Reference => default().then_some(K::Reference),
    }
}

/// An `unsigned long` (32 bits on Windows, 64 on Linux) as a `u64`, as `operator<<` streams it.
// `unsigned long` is already `u64` on Linux.
#[allow(clippy::useless_conversion)]
fn wide(v: c_ulong) -> u64 {
    u64::from(v)
}

/// The bit depth a CLF names: `8i`, `10i`, `12i`, `16i`, `16f` or `32f`, ignoring case.
///
/// Port of `CTFReaderOpElt::GetBitDepth` (CTFReaderHelper.cpp:1185-1195 @ v2.5.2).
fn get_bit_depth(str_bd: &[u8]) -> BitDepth {
    let str = lower(c_str(str_bd));
    match str.as_slice() {
        b"8i" => BitDepth::Uint8,
        b"10i" => BitDepth::Uint10,
        b"12i" => BitDepth::Uint12,
        b"16i" => BitDepth::Uint16,
        b"16f" => BitDepth::F16,
        b"32f" => BitDepth::F32,
        _ => BitDepth::Unknown,
    }
}

/// The reader of an operator: the op data it fills, its bit depths, and what its class
/// keeps.
///
/// Port of `CTFReaderOpElt` (CTFReaderHelper.h:308-381, CTFReaderHelper.cpp:802-957 @ v2.5.2)
/// and of its subclasses, one per [`CtfReaderKind`]; the subclasses' virtual functions are
/// matches on the kind. Upstream's op elements append their op data to the transform's ops
/// when they start and fill it in place; here an element fills its own copy, which it
/// writes to the transform's ops when it ends.
#[derive(Debug)]
pub struct CtfReaderOpElt {
    element: XmlReaderElementBase,
    /// `m_transform`: the parent.
    transform: Option<CtfReaderTransformRc>,
    /// `m_inBitDepth`.
    in_bit_depth: BitDepth,
    /// `m_outBitDepth`.
    out_bit_depth: BitDepth,
    /// The subclass.
    kind: CtfReaderKind,
    /// The op data the subclass holds (`m_matrix`, `m_range`, ...).
    op: OpData,
    /// Where `start` appended the op to the transform's ops.
    op_index: usize,
    /// `CTFArrayMgt::m_completed`.
    array_completed: bool,
    /// `CTFReaderRangeElt_1_7::m_isNoClamp`.
    is_no_clamp: bool,
}

impl CtfReaderOpElt {
    /// The reader of `kind`, if ported (WP 4.5c-4.5i port the others).
    ///
    /// Port of the constructors of the `CTFReaderOpElt` subclasses: `CTFReaderMatrixElt`
    /// (CTFReaderHelper.cpp:4561-4566) and `CTFReaderRangeElt` (4858-4862) @ v2.5.2.
    pub fn create(kind: CtfReaderKind) -> Option<CtfReaderOpElt> {
        let op = match kind {
            CtfReaderKind::Matrix | CtfReaderKind::Matrix1_3 => OpData::Matrix(MatrixOpData::new()),
            CtfReaderKind::Range | CtfReaderKind::Range1_7 => OpData::Range(RangeOpData::new()),
            _ => return None,
        };
        Some(CtfReaderOpElt {
            element: XmlReaderElementBase::new(b"", 0, b""),
            transform: None,
            in_bit_depth: BitDepth::Unknown,
            out_bit_depth: BitDepth::Unknown,
            kind,
            op,
            op_index: 0,
            array_completed: false,
            is_no_clamp: false,
        })
    }

    /// Port of `CTFReaderOpElt::setContext` (CTFReaderHelper.cpp:811-824 @ v2.5.2).
    pub fn set_context(
        &mut self,
        name: &[u8],
        transform: &Option<CtfReaderTransformRc>,
        xml_line_number: u32,
        xml_file: &[u8],
    ) -> Result<()> {
        self.element.set_context(name, xml_line_number, xml_file);

        self.transform = transform.clone();

        if self.transform.is_none() {
            return Err(self.throw_message(b"ProcessList tag missing."));
        }
        Ok(())
    }

    /// The reader's class.
    pub fn get_kind(&self) -> CtfReaderKind {
        self.kind
    }

    /// Port of `CTFReaderOpElt::getTransform` (CTFReaderHelper.h:359-362 @ v2.5.2).
    pub fn get_transform(&self) -> &CtfReaderTransformRc {
        self.transform.as_ref().expect("set_context set it")
    }

    /// The op data as the reader has filled it so far.
    ///
    /// Port of `CTFReaderOpElt::getOp` (CTFReaderHelper.h:349 @ v2.5.2).
    pub fn get_op(&self) -> &OpData {
        &self.op
    }

    /// The exception of `ThrowM(*this, ...)`.
    ///
    /// Port of `ThrowM` (CTFReaderHelper.cpp:34-56 @ v2.5.2).
    fn throw_m(&self, parts: &[&[u8]]) -> Exception {
        self.throw_message(&parts.concat())
    }

    /// The matrix of a Matrix reader.
    fn matrix_mut(&mut self) -> &mut MatrixOpData {
        match &mut self.op {
            OpData::Matrix(matrix) => matrix,
            _ => panic!("not a Matrix reader"),
        }
    }

    /// The matrix of a Matrix reader.
    fn matrix(&self) -> &MatrixOpData {
        match &self.op {
            OpData::Matrix(matrix) => matrix,
            _ => panic!("not a Matrix reader"),
        }
    }

    /// The range of a Range reader.
    ///
    /// Port of `CTFReaderRangeElt::getRange` (CTFReaderHelper.h:1124-1127 @ v2.5.2).
    pub fn range_mut(&mut self) -> &mut RangeOpData {
        match &mut self.op {
            OpData::Range(range) => range,
            _ => panic!("not a Range reader"),
        }
    }

    /// Whether this is a Range reader (`dynamic_cast<CTFReaderRangeElt *>`).
    pub fn is_range(&self) -> bool {
        matches!(self.kind, CtfReaderKind::Range | CtfReaderKind::Range1_7)
    }

    /// The operator's start: its op appended to the transform's ops; its id, name and bit
    /// depths (both required, and the input one matching the previous operator's output);
    /// the unrecognized attributes logged.
    ///
    /// Port of `CTFReaderOpElt::start` (CTFReaderHelper.cpp:837-924 @ v2.5.2).
    fn op_start(&mut self, atts: &[&[u8]]) -> Result<()> {
        let mut dbg = OStringStream::new(Crt::NATIVE);
        dbg.put_c_str(self.get_xml_file());
        dbg.put_str("(");
        dbg.put_u32(self.get_xml_line_number());
        dbg.put_str("): ");
        dbg.put_str("Parsing '");
        dbg.put_bytes(self.get_name());
        dbg.put_str("'.");
        log_debug(dbg.str());

        // Add a pointer to an empty op of the appropriate child class to the end of the opvec.
        // No data is copied since the parameters of the op have not been filled in yet.
        self.op_index = {
            let mut transform = self.get_transform().borrow_mut();
            let ops = transform.get_ops_mut();
            ops.push(Arc::new(self.op.clone()));
            ops.len() - 1
        };

        const NO_BIT_DEPTH: u32 = 0x00;
        const INPUT_BIT_DEPTH: u32 = 0x01;
        const OUTPUT_BIT_DEPTH: u32 = 0x02;

        let mut bit_depth_found = NO_BIT_DEPTH;

        let mut i = 0;
        while i < atts.len() {
            let (att, value) = (atts[i], atts[i + 1]);
            if equal_ignoring_case(ATTR_ID, att) {
                self.op.set_id(value);
            } else if equal_ignoring_case(ATTR_NAME, att) {
                self.op.set_name(value);
            } else if equal_ignoring_case(ATTR_BITDEPTH_IN, att) {
                let bitdepth = get_bit_depth(value);
                if bitdepth == BitDepth::Unknown {
                    return Err(self.throw_m(&[b"inBitDepth unknown value (", value, b")."]));
                }
                self.in_bit_depth = bitdepth;
                bit_depth_found |= INPUT_BIT_DEPTH;
            } else if equal_ignoring_case(ATTR_BITDEPTH_OUT, att) {
                let bitdepth = get_bit_depth(value);
                if bitdepth == BitDepth::Unknown {
                    return Err(self.throw_m(&[b"outBitDepth unknown value (", value, b")."]));
                }
                self.out_bit_depth = bitdepth;
                bit_depth_found |= OUTPUT_BIT_DEPTH;
            }

            i += 2;
        }

        // Check mandatory attributes.
        if (bit_depth_found & INPUT_BIT_DEPTH) == NO_BIT_DEPTH {
            return Err(self.throw_message(b"inBitDepth is missing."));
        } else if (bit_depth_found & OUTPUT_BIT_DEPTH) == NO_BIT_DEPTH {
            return Err(self.throw_message(b"outBitDepth is missing."));
        }

        // Ensure bit-depths consistency between ops in the file.

        let prev_out_bd = self.get_transform().borrow().get_previous_out_bit_depth();
        self.get_transform()
            .borrow_mut()
            .set_previous_out_bit_depth(self.out_bit_depth);
        // Unknown for first op.
        if prev_out_bd != BitDepth::Unknown && prev_out_bd != self.in_bit_depth {
            let in_bd = bit_depth_to_string(self.in_bit_depth);
            let prev_bd = bit_depth_to_string(prev_out_bd);
            let mut os = OStringStream::new(Crt::NATIVE);
            os.put_str("Bit-depth mismatch between ops. Previous op output ");
            os.put_str("bit-depth is: '");
            os.put_str(prev_bd);
            os.put_str("' and this op input bit-depth is '");
            os.put_str(in_bd);
            os.put_str("'. ");
            return Err(self.throw_message(os.str()));
        }

        self.validate_xml_parameters(atts);
        Ok(())
    }

    /// Logs the attributes the reader doesn't know.
    ///
    /// Port of `CTFReaderOpElt::validateXmlParameters` (CTFReaderHelper.cpp:935-947 @ v2.5.2).
    fn validate_xml_parameters(&self, atts: &[&[u8]]) {
        let mut i = 0;
        while i < atts.len() {
            if !self.is_op_parameter_valid(atts[i]) {
                self.element.log_parameter_warning(atts[i]);
            }

            i += 2;
        }
    }

    /// The attributes of the operator: `id`, `name`, `inBitDepth`, `outBitDepth`, `bypass`
    /// (CTF only), and the subclass's.
    ///
    /// Port of `CTFReaderOpElt::isOpParameterValid` (CTFReaderHelper.cpp:949-957) and
    /// `CTFReaderRangeElt_1_7::isOpParameterValid` (4917-4921) @ v2.5.2.
    fn is_op_parameter_valid(&self, att: &[u8]) -> bool {
        let base = equal_ignoring_case(ATTR_ID, att)
            || equal_ignoring_case(ATTR_NAME, att)
            || equal_ignoring_case(ATTR_BITDEPTH_IN, att)
            || equal_ignoring_case(ATTR_BITDEPTH_OUT, att)
            // Allow bypass attribute in CTF.
            || (equal_ignoring_case(ATTR_BYPASS, att) && !self.get_transform().borrow().is_clf());
        match self.kind {
            CtfReaderKind::Range1_7 => base || equal_ignoring_case(ATTR_STYLE, att),
            _ => base,
        }
    }

    /// Whether the reader holds an array (`dynamic_cast<CTFArrayMgt *>`).
    pub fn is_array_mgt(&self) -> bool {
        matches!(self.kind, CtfReaderKind::Matrix | CtfReaderKind::Matrix1_3)
    }

    /// Port of `CTFArrayMgt::isCompleted` (CTFReaderHelper.h:233-236 @ v2.5.2).
    pub fn is_completed(&self) -> bool {
        self.array_completed
    }

    /// Port of `CTFArrayMgt::setCompleted` (CTFReaderHelper.h:228-231 @ v2.5.2).
    fn set_completed(&mut self, status: bool) {
        self.array_completed = status;
    }

    /// Sizes the array for the dimensions `dims`: whether the reader takes them (upstream
    /// returns the array, or null).
    ///
    /// Port of `CTFReaderMatrixElt::updateDimension` (CTFReaderHelper.cpp:4590-4616) and
    /// `CTFReaderMatrixElt_1_3::updateDimension` (4684-4743) @ v2.5.2.
    pub fn update_dimension(&mut self, dims: &[u32]) -> bool {
        match self.kind {
            CtfReaderKind::Matrix => {
                // Only 3 3 3 and 4 4 3 are handled here (ctf version 1.2 or less).
                // Other formats are covered by CTFReaderMatrixElt_1_3::updateDimension.
                if dims.len() != 3 {
                    return false;
                }

                let num_color_components = dims[2];
                let size = dims[0];

                if size != dims[1] || num_color_components != 3 {
                    return false;
                }

                if !(3..=4).contains(&size) {
                    return false;
                }

                self.matrix_mut()
                    .get_array_mut()
                    .resize(size as c_ulong, num_color_components as c_ulong);
                true
            }
            CtfReaderKind::Matrix1_3 => {
                // In CLF v3, the committee dropped the third dimension value since with all
                // currently supported cases it is the same as the first value. There will
                // probably be a lot of files that use the old syntax with the new version
                // number or vice versa, so we try to be forgiving and handle either two or
                // three values (for both CLF and CTF). We decided to allow this even for old
                // files back to CTF 1.3.
                let size = dims.len();
                if size != 3 && size != 2 {
                    return false;
                }

                if !((dims[0] == 3 && dims[1] == 3)
                    || (dims[0] == 3 && dims[1] == 4)
                    || (dims[0] == 4 && dims[1] == 4)
                    || (dims[0] == 4 && dims[1] == 5))
                {
                    return false;
                }

                if size == 3 && dims[0] != dims[2] {
                    return false;
                }

                if is_debug_logging_enabled() {
                    let message: Option<&str> = if self.get_transform().borrow().get_ctf_version()
                        < CTF_PROCESS_LIST_VERSION_2_0
                    {
                        (size == 2).then_some(
                            "Matrix array dimension should have 3 numbers for CTF before version 2.",
                        )
                    } else {
                        (size == 3).then_some(
                            "Matrix array dimension should have 2 numbers for CTF from version 2.",
                        )
                    };
                    if let Some(message) = message {
                        let mut oss = OStringStream::new(Crt::NATIVE);
                        oss.put_c_str(self.get_xml_file());
                        oss.put_str("(");
                        oss.put_u32(self.get_xml_line_number());
                        oss.put_str("): ");
                        oss.put_str(message);
                        log_debug(oss.str());
                    }
                }

                self.matrix_mut()
                    .get_array_mut()
                    .resize(dims[1] as c_ulong, dims[0] as c_ulong);
                true
            }
            _ => false,
        }
    }

    /// The number of values of the array `update_dimension` sized: `ArrayBase::getNumValues`.
    pub fn array_num_values(&self) -> c_ulong {
        self.matrix().get_array().get_num_values()
    }

    /// The array's length: `ArrayBase::getLength`.
    pub fn array_length(&self) -> c_ulong {
        self.matrix().get_array().get_length()
    }

    /// The array's number of color components: `ArrayBase::getNumColorComponents`.
    pub fn array_num_color_components(&self) -> c_ulong {
        self.matrix().get_array().get_num_color_components()
    }

    /// Sets the array's value at `index`: `ArrayBase::setDoubleValue`.
    pub fn array_set_double_value(&mut self, index: c_ulong, value: f64) {
        self.matrix_mut()
            .get_array_mut()
            .set_double_value(index, value);
    }

    /// Finalizes the array, `position` values read.
    ///
    /// Port of `CTFReaderMatrixElt::endArray` (CTFReaderHelper.cpp:4618-4634) and
    /// `CTFReaderMatrixElt_1_3::endArray` (4745-4854) @ v2.5.2.
    pub fn end_array(&mut self, position: u32) -> Result<()> {
        match self.kind {
            CtfReaderKind::Matrix => {
                let array = self.matrix().get_array();
                if array.get_num_values() != position as c_ulong {
                    let mut arg = OStringStream::new(Crt::NATIVE);
                    arg.put_str("Expected ");
                    arg.put_u64(wide(array.get_length()));
                    arg.put_str("x");
                    arg.put_u64(wide(array.get_length()));
                    arg.put_str(" Array values, found ");
                    arg.put_u32(position);

                    return Err(Exception::new(arg.into_bytes()));
                }

                // Array parsing is done.
                self.set_completed(true);

                self.convert_1_2_to_latest()
            }
            CtfReaderKind::Matrix1_3 => self.end_array_1_3(position),
            _ => Ok(()),
        }
    }

    /// Helper method to convert Matrix data from 1.2 to latest.
    ///
    /// Port of `CTFReaderMatrixElt::convert_1_2_to_Latest` (CTFReaderHelper.cpp:4636-4682 @
    /// v2.5.2).
    fn convert_1_2_to_latest(&mut self) -> Result<()> {
        if CTF_PROCESS_LIST_VERSION_1_2 < CTF_PROCESS_LIST_VERSION {
            let length = self.matrix().get_array().get_length();
            if length == 3 {
                let offsets = [0.0f64, 0.0, 0.0, 0.0];
                self.matrix_mut().set_rgba_offsets(&offsets);
            } else if length == 4 {
                let matrix = self.matrix_mut();
                let old_v = matrix.get_array().get_values().clone();

                // Extract offsets.
                matrix.set_offset_value(0, old_v[3])?;
                matrix.set_offset_value(1, old_v[7])?;
                matrix.set_offset_value(2, old_v[11])?;
                matrix.set_offset_value(3, 0.0)?;

                let array = matrix.get_array_mut();
                array.resize(3, 3);

                let v = array.get_values_mut();
                v[0] = old_v[0];
                v[1] = old_v[1];
                v[2] = old_v[2];

                v[3] = old_v[4];
                v[4] = old_v[5];
                v[5] = old_v[6];

                v[6] = old_v[8];
                v[7] = old_v[9];
                v[8] = old_v[10];
            } else {
                let mut arg = OStringStream::new(Crt::NATIVE);
                arg.put_str("MatrixElt: Expecting array dimension to be 3 or 4. Got: ");
                arg.put_u64(wide(length));
                arg.put_str(".");

                return Err(Exception::new(arg.into_bytes()));
            }
        }
        Ok(())
    }

    /// Port of `CTFReaderMatrixElt_1_3::endArray` (CTFReaderHelper.cpp:4745-4854 @ v2.5.2).
    fn end_array_1_3(&mut self, position: u32) -> Result<()> {
        let (length, num_color_components) = {
            let array = self.matrix().get_array();
            (array.get_length(), array.get_num_color_components())
        };
        let found = |expected: &[u8]| -> Vec<u8> {
            [
                b"Expected ".as_slice(),
                expected,
                b" Array values, found ",
                position.to_string().as_bytes(),
                b".",
            ]
            .concat()
        };

        // Note: Version 1.3 of Matrix Op supports 4 xml formats:
        //       1) 4x5x4, matrix with alpha and offsets
        //       2) 4x4x4, matrix with alpha and no offsets
        //       3) 3x4x3, matrix only with offsets and no alpha
        //       4) 3x3x3, matrix with no alpha and no offsets
        // Note: As of CLF v3 we drop the third digit, since it is redundant.

        if length == 3 && num_color_components == 3 {
            if position != 9 {
                return Err(self.throw_message(&found(b"3x3")));
            }
        } else if length == 4 {
            if num_color_components == 3 {
                if position != 12 {
                    return Err(self.throw_message(&found(b"3x4")));
                }

                let matrix = self.matrix_mut();
                let values = matrix.get_array().get_values().clone();

                matrix.set_offset_value(0, values[3])?;
                matrix.set_offset_value(1, values[7])?;
                matrix.set_offset_value(2, values[11])?;
                matrix.set_offset_value(3, 0.0)?;

                let old_v = values;

                let array = matrix.get_array_mut();
                array.set_length(3);

                let v = array.get_values_mut();
                v[0] = old_v[0];
                v[1] = old_v[1];
                v[2] = old_v[2];

                v[3] = old_v[4];
                v[4] = old_v[5];
                v[5] = old_v[6];

                v[6] = old_v[8];
                v[7] = old_v[9];
                v[8] = old_v[10];
            } else {
                if position != 16 {
                    return Err(self.throw_message(&found(b"4x4")));
                }

                let offsets = [0.0f64, 0., 0., 0.];
                self.matrix_mut().set_rgba_offsets(&offsets);
            }
        } else {
            if position != 20 {
                return Err(self.throw_message(&found(b"4x5")));
            }

            let matrix = self.matrix_mut();
            let values = matrix.get_array().get_values().clone();

            matrix.set_offset_value(0, values[4])?;
            matrix.set_offset_value(1, values[9])?;
            matrix.set_offset_value(2, values[14])?;
            matrix.set_offset_value(3, values[19])?;

            let old_v = values;

            let array = matrix.get_array_mut();
            array.resize(4, 4);

            let v = array.get_values_mut();
            v[0] = old_v[0];
            v[1] = old_v[1];
            v[2] = old_v[2];
            v[3] = old_v[3];

            v[4] = old_v[5];
            v[5] = old_v[6];
            v[6] = old_v[7];
            v[7] = old_v[8];

            v[8] = old_v[10];
            v[9] = old_v[11];
            v[10] = old_v[12];
            v[11] = old_v[13];

            v[12] = old_v[15];
            v[13] = old_v[16];
            v[14] = old_v[17];
            v[15] = old_v[18];
        }

        // Array parsing is done.

        self.set_completed(true);
        Ok(())
    }

    /// The matrix's scaling to the file's bit depths, then its check.
    ///
    /// Port of `CTFReaderMatrixElt::end` (CTFReaderHelper.cpp:4572-4583 @ v2.5.2).
    fn matrix_end(&mut self) -> Result<()> {
        let in_scale = get_bit_depth_max_value(self.in_bit_depth)?;
        let out_scale = 1.0 / get_bit_depth_max_value(self.out_bit_depth)?;
        let (in_bd, out_bd) = (self.in_bit_depth, self.out_bit_depth);
        let matrix = self.matrix_mut();
        matrix.scale(in_scale, out_scale);

        matrix.set_file_input_bit_depth(in_bd);
        matrix.set_file_output_bit_depth(out_bd);

        // Validate the end result.
        matrix.validate()
    }

    /// The range's normalization to the file's bit depths, then its check.
    ///
    /// Port of `CTFReaderRangeElt::end` (CTFReaderHelper.cpp:4868-4879 @ v2.5.2).
    fn range_end(&mut self) -> Result<()> {
        let (in_bd, out_bd) = (self.in_bit_depth, self.out_bit_depth);
        let range = self.range_mut();
        range.set_file_input_bit_depth(in_bd);
        range.set_file_output_bit_depth(out_bd);

        range.normalize()?;

        // Validate the end result.
        range.validate()
    }

    /// Writes the reader's op to the transform's ops, where `start` appended it.
    fn commit_op(&self) {
        let mut transform = self.get_transform().borrow_mut();
        transform.get_ops_mut()[self.op_index] = Arc::new(self.op.clone());
    }
}

impl XmlReaderElement for CtfReaderOpElt {
    fn element(&self) -> &XmlReaderElementBase {
        &self.element
    }
    fn element_mut(&mut self) -> &mut XmlReaderElementBase {
        &mut self.element
    }

    /// Port of `CTFReaderOpElt::start` (CTFReaderHelper.cpp:837-924) and
    /// `CTFReaderRangeElt_1_7::start` (4897-4915) @ v2.5.2.
    fn start(&mut self, atts: &[&[u8]]) -> Result<()> {
        self.op_start(atts)?;
        if self.kind == CtfReaderKind::Range1_7 {
            self.is_no_clamp = false;

            let mut i = 0;
            while i < atts.len() {
                // TODO: Refactor to use a base class attribute checking function instead of
                //       looping twice around the attribute list.
                if equal_ignoring_case(ATTR_STYLE, atts[i]) {
                    self.is_no_clamp = equal_ignoring_case(b"noClamp", atts[i + 1]);
                }

                i += 2;
            }
        }
        Ok(())
    }

    /// Port of `CTFReaderOpElt::end` (CTFReaderHelper.cpp:926-928), `CTFReaderMatrixElt::end`
    /// (4572-4583), `CTFReaderRangeElt::end` (4868-4879) and `CTFReaderRangeElt_1_7::end`
    /// (4923-4943) @ v2.5.2.
    fn end(&mut self) -> Result<()> {
        match self.kind {
            CtfReaderKind::Matrix | CtfReaderKind::Matrix1_3 => self.matrix_end()?,
            CtfReaderKind::Range | CtfReaderKind::Range1_7 => self.range_end()?,
            _ => {}
        }
        self.commit_op();

        // Adding support for the noClamp style introduced in the CLF spec. We convert our
        // RangeOp (which always clamps) into an equivalent MatrixOp.
        if self.kind == CtfReaderKind::Range1_7 && self.is_no_clamp {
            let OpData::Range(range) = &self.op else {
                panic!("a Range reader");
            };
            let p_mtx = Arc::new(OpData::Matrix(range.convert_to_matrix()?));

            // This code assumes that the current Range is at the end of the opList. In other
            // words, that this Op's end() method will be called before any other Op's start().
            let mut transform = self.get_transform().borrow_mut();
            let ops = transform.get_ops_mut();
            let len = ops.len();
            let pos = len - 1;

            // Replace the range appended to m_transform in OpElt::start with the matrix.
            ops[pos] = p_mtx;
        }
        Ok(())
    }

    fn is_container(&self) -> bool {
        true
    }

    /// Port of `CTFReaderOpElt::getIdentifier` (CTFReaderHelper.cpp:826-829 @ v2.5.2).
    fn get_identifier(&self) -> &[u8] {
        self.op.get_id()
    }

    /// Port of `CTFReaderOpElt::getTypeName` (CTFReaderHelper.cpp:930-933 @ v2.5.2).
    fn get_type_name(&self) -> &[u8] {
        self.get_name()
    }

    fn as_container_mut(&mut self) -> Option<&mut dyn XmlReaderContainerElt> {
        Some(self)
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl XmlReaderContainerElt for CtfReaderOpElt {
    /// A description of the operator, as a child of its metadata.
    ///
    /// Port of `CTFReaderOpElt::appendMetadata` (CTFReaderHelper.cpp:831-835 @ v2.5.2).
    fn append_metadata(&mut self, name: &[u8], value: &[u8]) -> Result<()> {
        let item = FormatMetadataImpl::new(name, value)?;
        self.op
            .get_format_metadata_mut()
            .get_children_elements_mut()
            .push(item);
        Ok(())
    }
}

/// The parent of a plain element of an operator: its reader.
fn op_parent(parent: &ContainerEltRcPtr) -> RefMut<'_, CtfReaderOpElt> {
    RefMut::map(parent.borrow_mut(), |p| {
        p.as_any_mut()
            .downcast_mut::<CtfReaderOpElt>()
            .expect("the parent is an operator's reader")
    })
}

/// An operator's array: its dimensions, then its values.
///
/// Port of `CTFReaderArrayElt` (CTFReaderHelper.h:192-217, CTFReaderHelper.cpp:265-418 @
/// v2.5.2). Upstream keeps a pointer to the array the parent's `updateDimension` returns;
/// here the parent holds it, and the element whether it was given one.
#[derive(Debug)]
pub struct CtfReaderArrayElt {
    plain: XmlReaderPlainEltBase,
    /// The parent's type name, which `getTypeName` returns.
    type_name: Vec<u8>,
    /// Whether the parent sized its array (`m_array` not null).
    has_array: bool,
    /// `m_position`: the current position to fill.
    position: u32,
}

impl CtfReaderArrayElt {
    /// Port of `CTFReaderArrayElt::CTFReaderArrayElt` (CTFReaderHelper.cpp:265-273 @ v2.5.2).
    pub fn new(
        name: &[u8],
        parent: ContainerEltRcPtr,
        xml_line_number: u32,
        xml_file: &[u8],
    ) -> CtfReaderArrayElt {
        let type_name = parent.borrow().get_type_name().to_vec();
        CtfReaderArrayElt {
            plain: XmlReaderPlainEltBase::new(name, parent, xml_line_number, xml_file),
            type_name,
            has_array: false,
            position: 0,
        }
    }

    /// The exception of `ThrowM(*this, ...)`.
    fn throw_m(&self, parts: &[&[u8]]) -> Exception {
        self.throw_message(&parts.concat())
    }
}

impl XmlReaderElement for CtfReaderArrayElt {
    fn element(&self) -> &XmlReaderElementBase {
        &self.plain.element
    }
    fn element_mut(&mut self) -> &mut XmlReaderElementBase {
        &mut self.plain.element
    }

    /// The `dim` attribute (required): the parent sizes its array.
    ///
    /// Port of `CTFReaderArrayElt::start` (CTFReaderHelper.cpp:280-343 @ v2.5.2).
    fn start(&mut self, atts: &[&[u8]]) -> Result<()> {
        let mut is_dim_found = false;

        let mut i = 0;
        while i < atts.len() && !atts[i].is_empty() {
            if equal_ignoring_case(ATTR_DIMENSION, atts[i]) {
                is_dim_found = true;

                let dim_str = c_str(atts[i + 1]);
                let len = dim_str.len();

                let illegal = |this: &Self| {
                    this.throw_m(&[
                        b"Illegal '",
                        this.get_type_name(),
                        b"' array dimensions ",
                        &truncate_string(dim_str, len),
                        b".",
                    ])
                };

                let dims = match get_numbers::<u32>(dim_str, len) {
                    Ok(dims) => dims,
                    Err(_ce) => return Err(illegal(self)),
                };

                let parent = self.plain.get_parent().clone();
                let mut parent = parent.borrow_mut();
                let p_arr = parent
                    .as_any_mut()
                    .downcast_mut::<CtfReaderOpElt>()
                    .filter(|op| op.is_array_mgt());
                let Some(p_arr) = p_arr else {
                    return Err(self.throw_m(&[
                        b"Parsing issue while parsing array dimensions of '",
                        self.get_type_name(),
                        b"' (",
                        &truncate_string(dim_str, len),
                        b").",
                    ]));
                };
                let max = if dims.is_empty() { 0 } else { dims.len() - 1 };
                if max == 0 {
                    return Err(illegal(self));
                }

                self.has_array = p_arr.update_dimension(&dims);
                if !self.has_array {
                    return Err(self.throw_m(&[
                        b"'",
                        self.get_type_name(),
                        b"' Illegal array dimensions ",
                        &truncate_string(dim_str, len),
                        b".",
                    ]));
                }
            } else {
                self.plain.element.log_parameter_warning(atts[i]);
            }

            i += 2;
        }

        // Check mandatory elements.
        if !is_dim_found {
            return Err(self.throw_message(b"Missing 'dim' attribute."));
        }

        self.position = 0;
        Ok(())
    }

    /// The parent finalizes its array.
    ///
    /// Port of `CTFReaderArrayElt::end` (CTFReaderHelper.cpp:345-353 @ v2.5.2).
    fn end(&mut self) -> Result<()> {
        // A known element (e.g. an array) in a dummy element, no need to validate it.
        if self.plain.get_parent().borrow().is_dummy() {
            return Ok(());
        }

        op_parent(self.plain.get_parent()).end_array(self.position)
    }

    fn is_container(&self) -> bool {
        false
    }
    fn get_identifier(&self) -> &[u8] {
        self.get_name()
    }

    /// The parent's type name.
    ///
    /// Port of `CTFReaderArrayElt::getTypeName` (CTFReaderHelper.cpp:415-418 @ v2.5.2).
    fn get_type_name(&self) -> &[u8] {
        &self.type_name
    }

    fn as_plain_mut(&mut self) -> Option<&mut dyn XmlReaderPlainElt> {
        Some(self)
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl XmlReaderPlainElt for CtfReaderArrayElt {
    /// The values, up to the number the array holds.
    ///
    /// Port of `CTFReaderArrayElt::setRawData` (CTFReaderHelper.cpp:355-413 @ v2.5.2).
    fn set_raw_data(&mut self, s: &[u8], _xml_line: u32) -> Result<()> {
        let len = s.len();
        assert!(self.has_array, "an array element sized its array");
        let parent = self.plain.get_parent().clone();
        let mut p = op_parent(&parent);
        let max_values = p.array_num_values();

        // using GetNextNumber here instead of GetNumbers to leverage the loop needed here to
        // process each value from the strings. This function is the most used when reading in
        // large transforms.

        let mut pos = find_next_token_start(s, len, 0);
        while pos != len {
            let mut data = 0.0f64;

            if get_next_number(s, len, &mut pos, &mut data).is_err() {
                return Err(self.throw_m(&[
                    b"Illegal values '",
                    &truncate_string(s, len),
                    b"' in array of ",
                    self.get_type_name(),
                    b".",
                ]));
            }

            if (self.position as c_ulong) < max_values {
                p.array_set_double_value(self.position as c_ulong, data);
                self.position += 1;
            } else {
                let mut arg = OStringStream::new(Crt::NATIVE);
                let length = wide(p.array_length());
                let components = wide(p.array_num_color_components());
                match p.get_op().get_type() {
                    OpDataType::Lut1D => {
                        arg.put_u64(length);
                        arg.put_str("x");
                        arg.put_u64(components);
                    }
                    OpDataType::Lut3D => {
                        arg.put_u64(length);
                        arg.put_str("x");
                        arg.put_u64(length);
                        arg.put_str("x");
                        arg.put_u64(length);
                        arg.put_str("x");
                        arg.put_u64(components);
                    }
                    // Matrix
                    _ => {
                        arg.put_u64(length);
                        arg.put_str("x");
                        arg.put_u64(length);
                    }
                }

                return Err(self.throw_m(&[
                    b"Expected ",
                    arg.str(),
                    b" Array, found too many values in array of '",
                    self.get_type_name(),
                    b"'.",
                ]));
            }
        }
        Ok(())
    }
    fn get_parent(&self) -> &ContainerEltRcPtr {
        self.plain.get_parent()
    }
}

/// A value of a Range: `minInValue`, `maxInValue`, `minOutValue` or `maxOutValue`.
///
/// Port of `CTFReaderRangeValueElt` (CTFReaderHelper.h:1151-1166, CTFReaderHelper.cpp:
/// 4947-5010 @ v2.5.2).
#[derive(Debug)]
pub struct CtfReaderRangeValueElt {
    plain: XmlReaderPlainEltBase,
}

impl CtfReaderRangeValueElt {
    /// Port of `CTFReaderRangeValueElt::CTFReaderRangeValueElt` (CTFReaderHelper.cpp:
    /// 4947-4953 @ v2.5.2).
    pub fn new(
        name: &[u8],
        parent: ContainerEltRcPtr,
        xml_line_number: u32,
        xml_file: &[u8],
    ) -> CtfReaderRangeValueElt {
        CtfReaderRangeValueElt {
            plain: XmlReaderPlainEltBase::new(name, parent, xml_line_number, xml_file),
        }
    }
}

impl XmlReaderElement for CtfReaderRangeValueElt {
    fn element(&self) -> &XmlReaderElementBase {
        &self.plain.element
    }
    fn element_mut(&mut self) -> &mut XmlReaderElementBase {
        &mut self.plain.element
    }

    /// Every attribute is logged.
    ///
    /// Port of `CTFReaderRangeValueElt::start` (CTFReaderHelper.cpp:4959-4967 @ v2.5.2).
    fn start(&mut self, atts: &[&[u8]]) -> Result<()> {
        let mut i = 0;
        while i < atts.len() {
            self.plain.element.log_parameter_warning(atts[i]);
            i += 2;
        }
        Ok(())
    }

    /// Port of `CTFReaderRangeValueElt::end` (CTFReaderHelper.cpp:4969-4971 @ v2.5.2).
    fn end(&mut self) -> Result<()> {
        Ok(())
    }

    fn is_container(&self) -> bool {
        false
    }
    fn get_identifier(&self) -> &[u8] {
        self.get_name()
    }
    fn get_type_name(&self) -> &[u8] {
        self.get_name()
    }
    fn as_plain_mut(&mut self) -> Option<&mut dyn XmlReaderPlainElt> {
        Some(self)
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl XmlReaderPlainElt for CtfReaderRangeValueElt {
    /// The value: one number.
    ///
    /// Port of `CTFReaderRangeValueElt::setRawData` (CTFReaderHelper.cpp:4973-5010 @ v2.5.2).
    fn set_raw_data(&mut self, s: &[u8], _xml_line: u32) -> Result<()> {
        let len = s.len();
        let data = match get_numbers::<f64>(s, len) {
            Ok(data) => data,
            Err(ce) => {
                return Err(self.throw_message(
                    &[
                        b"Illegal '".as_slice(),
                        self.get_type_name(),
                        b"' values ",
                        &truncate_string(s, len),
                        b" [",
                        c_str(ce.what()),
                        b"]",
                    ]
                    .concat(),
                ));
            }
        };

        if data.len() != 1 {
            return Err(self.throw_message(b"Range element: non-single value."));
        }

        let parent = self.plain.get_parent().clone();
        let mut p_range = op_parent(&parent);
        let name = self.get_name();
        if equal_ignoring_case(name, TAG_MIN_IN_VALUE) {
            p_range.range_mut().set_min_in_value(data[0]);
        } else if equal_ignoring_case(name, TAG_MAX_IN_VALUE) {
            p_range.range_mut().set_max_in_value(data[0]);
        } else if equal_ignoring_case(name, TAG_MIN_OUT_VALUE) {
            p_range.range_mut().set_min_out_value(data[0]);
        } else if equal_ignoring_case(name, TAG_MAX_OUT_VALUE) {
            p_range.range_mut().set_max_out_value(data[0]);
        }
        Ok(())
    }
    fn get_parent(&self) -> &ContainerEltRcPtr {
        self.plain.get_parent()
    }
}
