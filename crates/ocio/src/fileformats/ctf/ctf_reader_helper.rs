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
use std::cell::RefCell;
use std::rc::Rc;

use ocio_formats::fileformats::xmlutils::xml_reader_helper::{
    ContainerEltRcPtr, XmlReaderComplexEltBase, XmlReaderContainerElt, XmlReaderElement,
    XmlReaderElementBase, XmlReaderPlainElt, XmlReaderPlainEltBase,
};
use ocio_formats::fileformats::xmlutils::xml_reader_utils::{ATTR_ID, ATTR_NAME};
use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::format_metadata::FormatMetadataImpl;
use ocio_ops::platform::strcasecmp;
use ocio_ops::utils::cscan::{ScanArg, ScanCrt, sscanf};
use ocio_ops::utils::string_utils::c_str;
use ocio_ops::{Exception, Result};

use super::ctf_reader_utils::{
    ATTR_COMP_CLF_VERSION, ATTR_INVERSE_OF, ATTR_VERSION, TAG_PROCESS_LIST,
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
