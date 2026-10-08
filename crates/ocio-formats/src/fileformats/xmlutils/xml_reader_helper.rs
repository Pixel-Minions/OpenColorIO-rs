// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The XML readers' elements: the base classes the CDL and CTF readers derive their
//! elements from, the dummy and description elements, the CDL's SOP and saturation values,
//! and the element stack.
//!
//! Port of `src/OpenColorIO/fileformats/xmlutils/XMLReaderHelper.h` and `XMLReaderHelper.cpp`
//! (@ v2.5.2).
//!
//! **The class hierarchy.** Upstream's elements are C++ classes held by `shared_ptr`, which
//! the readers cast with `dynamic_pointer_cast`. Here an element is an
//! [`ElementRcPtr`] (`Rc<RefCell<dyn XmlReaderElement>>`):
//! - [`XmlReaderElement`] holds the virtual functions of `XmlReaderElement`, and the casts to
//!   the abstract classes the readers use as optional methods (`as_container_mut`,
//!   `as_plain_mut`, `as_sop_node_base_mut`, `as_sat_node_base_mut`); casts to concrete
//!   classes go through `as_any`;
//! - [`XmlReaderContainerElt`] and [`XmlReaderPlainElt`] add `appendMetadata` and
//!   `setRawData`;
//! - the data members of the base classes are structs the concrete elements hold:
//!   [`XmlReaderElementBase`], [`XmlReaderPlainEltBase`], [`XmlReaderComplexEltBase`],
//!   [`XmlReaderSopNodeBase`].
//!
//! Upstream throws `Exception`s; here functions return `Result`, and `throwMessage` builds
//! the exception the caller returns.

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::format_metadata::FormatMetadataImpl;
use ocio_ops::logging::log_warning;
use ocio_ops::ops::cdl::cdl_op_data::{CdlOpData, ChannelParams};
use ocio_ops::{Exception, Result};

use super::xml_reader_utils::{
    TAG_OFFSET, TAG_POWER, TAG_SATURATION, TAG_SLOPE, XmlNumber, get_numbers, trim, truncate_string,
};

/// `METADATA_SOP_DESCRIPTION` (src/OpenColorIO/transforms/CDLTransform.h:19 @ v2.5.2).
pub const METADATA_SOP_DESCRIPTION: &[u8] = b"SOPDescription";
/// `METADATA_SAT_DESCRIPTION` (src/OpenColorIO/transforms/CDLTransform.h:20 @ v2.5.2).
pub const METADATA_SAT_DESCRIPTION: &[u8] = b"SATDescription";

/// A shared element: `ElementRcPtr` (XMLReaderHelper.h:119).
pub type ElementRcPtr = Rc<RefCell<dyn XmlReaderElement>>;
/// A shared container element: `ContainerEltRcPtr` (XMLReaderHelper.h:146). Its
/// `as_container_mut` is `Some`.
pub type ContainerEltRcPtr = Rc<RefCell<dyn XmlReaderElement>>;
/// A shared CDL op data: `CDLOpDataRcPtr`, which the CDL readers' elements share.
pub type CdlOpDataRcPtr = Rc<RefCell<CdlOpData>>;

/// The data members of `XmlReaderElement`, and its non-virtual functions.
///
/// Port of `XmlReaderElement` (XMLReaderHelper.h:20-117, XMLReaderHelper.cpp:13-56 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct XmlReaderElementBase {
    name: Vec<u8>,
    xml_line_number: u32,
    xml_file: Vec<u8>,
}

impl XmlReaderElementBase {
    /// Port of `XmlReaderElement::XmlReaderElement` (XMLReaderHelper.cpp:14-21 @ v2.5.2).
    pub fn new(name: &[u8], xml_line_number: u32, xml_file: &[u8]) -> XmlReaderElementBase {
        XmlReaderElementBase {
            name: name.to_vec(),
            xml_line_number,
            xml_file: xml_file.to_vec(),
        }
    }

    /// Port of `XmlReaderElement::getName` (XMLReaderHelper.h:38-41 @ v2.5.2).
    pub fn get_name(&self) -> &[u8] {
        &self.name
    }

    /// Port of `XmlReaderElement::getXmlLineNumber` (XMLReaderHelper.h:45-48 @ v2.5.2).
    pub fn get_xml_line_number(&self) -> u32 {
        self.xml_line_number
    }

    /// The file's name, or "File name not specified".
    ///
    /// Port of `XmlReaderElement::getXmlFile` (XMLReaderHelper.cpp:27-31 @ v2.5.2).
    pub fn get_xml_file(&self) -> &[u8] {
        if self.xml_file.is_empty() {
            b"File name not specified"
        } else {
            &self.xml_file
        }
    }

    /// Port of `XmlReaderElement::setContext` (XMLReaderHelper.cpp:33-40 @ v2.5.2).
    pub fn set_context(&mut self, name: &[u8], xml_line_number: u32, xml_file: &[u8]) {
        self.name = name.to_vec();
        self.xml_line_number = xml_line_number;
        self.xml_file = xml_file.to_vec();
    }

    /// The exception `throwMessage` throws: "At line N: error".
    ///
    /// Port of `XmlReaderElement::throwMessage` (XMLReaderHelper.cpp:42-48 @ v2.5.2).
    pub fn throw_message(&self, error: &[u8]) -> Exception {
        let mut os = OStringStream::new(Crt::NATIVE);
        os.put_str("At line ");
        os.put_u32(self.get_xml_line_number());
        os.put_str(": ");
        os.put_c_str(error);
        Exception::new(os.into_bytes())
    }

    /// Logs "file(line): Unrecognized attribute 'param' of 'name'.".
    ///
    /// Port of `XmlReaderElement::logParameterWarning` (XMLReaderHelper.cpp:50-56 @ v2.5.2).
    pub fn log_parameter_warning(&self, param: &[u8]) {
        let mut oss = OStringStream::new(Crt::NATIVE);
        oss.put_c_str(self.get_xml_file());
        oss.put_str("(");
        oss.put_u32(self.get_xml_line_number());
        oss.put_str("): ");
        oss.put_str("Unrecognized attribute '");
        oss.put_c_str(param);
        oss.put_str("' of '");
        oss.put_bytes(self.get_name());
        oss.put_str("'.");
        log_warning(c_str(oss.str()));
    }

    /// The single number of the attribute `name`'s value `attrib`.
    ///
    /// Port of `XmlReaderElement::parseScalarAttribute` (XMLReaderHelper.h:72-103 @ v2.5.2).
    pub fn parse_scalar_attribute<T: XmlNumber>(&self, name: &[u8], attrib: &[u8]) -> Result<T> {
        let attrib = c_str(attrib);
        let len = attrib.len();

        let data = match get_numbers::<T>(attrib, len) {
            Ok(data) => data,
            Err(ce) => {
                let mut oss = OStringStream::new(Crt::NATIVE);
                oss.put_str("For parameter: '");
                oss.put_c_str(name);
                oss.put_str("'. ");
                oss.put_c_str(ce.what());
                return Err(self.throw_message(oss.str()));
            }
        };

        if data.len() != 1 {
            let mut oss = OStringStream::new(Crt::NATIVE);
            oss.put_str("For parameter: '");
            oss.put_c_str(name);
            oss.put_str("'. ");
            oss.put_str("Expecting 1 value, found ");
            oss.put_u64(data.len() as u64);
            oss.put_str(" values.");
            return Err(self.throw_message(oss.str()));
        }

        Ok(data[0])
    }
}

/// `s` up to its first null, as a `const char *` reads it.
fn c_str(s: &[u8]) -> &[u8] {
    s.iter().position(|&b| b == 0).map_or(s, |n| &s[..n])
}

/// An element of the XML readers.
///
/// Port of the virtual functions of `XmlReaderElement` (XMLReaderHelper.h:20-117 @ v2.5.2),
/// and of the casts the readers make.
pub trait XmlReaderElement: Any {
    /// The base class's data.
    fn element(&self) -> &XmlReaderElementBase;
    /// The base class's data.
    fn element_mut(&mut self) -> &mut XmlReaderElementBase;

    /// Start the parsing of the element, with its attributes' names and values alternately.
    fn start(&mut self, atts: &[&[u8]]) -> Result<()>;

    /// End the parsing of the element.
    fn end(&mut self) -> Result<()>;

    /// Is it a container which means if it can hold other elements.
    fn is_container(&self) -> bool;

    /// `getIdentifier`.
    fn get_identifier(&self) -> &[u8];

    /// `getTypeName`.
    fn get_type_name(&self) -> &[u8];

    /// Is it a dummy element? Only [`XmlReaderDummyElt`] will return true.
    fn is_dummy(&self) -> bool {
        false
    }

    /// `dynamic_cast<XmlReaderContainerElt *>`.
    fn as_container_mut(&mut self) -> Option<&mut dyn XmlReaderContainerElt> {
        None
    }

    /// `dynamic_cast<XmlReaderPlainElt *>`.
    fn as_plain_mut(&mut self) -> Option<&mut dyn XmlReaderPlainElt> {
        None
    }

    /// `dynamic_cast<XmlReaderSOPNodeBaseElt *>`.
    fn as_sop_node_base_mut(&mut self) -> Option<&mut dyn XmlReaderSopNodeBaseElt> {
        None
    }

    /// `dynamic_cast<XmlReaderSatNodeBaseElt *>`.
    fn as_sat_node_base_mut(&mut self) -> Option<&mut dyn XmlReaderSatNodeBaseElt> {
        None
    }

    /// For casts to concrete classes.
    fn as_any(&self) -> &dyn Any;
    /// For casts to concrete classes.
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// `getName`.
    fn get_name(&self) -> &[u8] {
        self.element().get_name()
    }

    /// `getXmlLineNumber`.
    fn get_xml_line_number(&self) -> u32 {
        self.element().get_xml_line_number()
    }

    /// `getXmlFile`.
    fn get_xml_file(&self) -> &[u8] {
        self.element().get_xml_file()
    }

    /// `throwMessage`.
    fn throw_message(&self, error: &[u8]) -> Exception {
        self.element().throw_message(error)
    }
}

impl std::fmt::Debug for dyn XmlReaderElement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("XmlReaderElement")
            .field("name", &String::from_utf8_lossy(self.get_name()))
            .field("line", &self.get_xml_line_number())
            .finish_non_exhaustive()
    }
}

/// An element that could contain sub-elements.
///
/// Port of `XmlReaderContainerElt` (XMLReaderHelper.h:121-144 @ v2.5.2): its
/// `isContainer` is true.
pub trait XmlReaderContainerElt: XmlReaderElement {
    /// `appendMetadata`.
    fn append_metadata(&mut self, name: &[u8], value: &[u8]) -> Result<()>;
}

/// A basic element, which holds text.
///
/// Port of `XmlReaderPlainElt` (XMLReaderHelper.h:148-190 @ v2.5.2): its `isContainer` is
/// false, and its identifier and type name are its name.
pub trait XmlReaderPlainElt: XmlReaderElement {
    /// `setRawData`.
    fn set_raw_data(&mut self, s: &[u8], xml_line: u32) -> Result<()>;

    /// `getParent`.
    fn get_parent(&self) -> &ContainerEltRcPtr;
}

/// The data members of `XmlReaderPlainElt`.
///
/// Port of `XmlReaderPlainElt` (XMLReaderHelper.h:148-190 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct XmlReaderPlainEltBase {
    /// The base class.
    pub element: XmlReaderElementBase,
    /// The element's parent.
    parent: ContainerEltRcPtr,
}

impl XmlReaderPlainEltBase {
    /// Port of `XmlReaderPlainElt::XmlReaderPlainElt` (XMLReaderHelper.h:151-159 @ v2.5.2).
    pub fn new(
        name: &[u8],
        parent: ContainerEltRcPtr,
        xml_line_number: u32,
        xml_file: &[u8],
    ) -> XmlReaderPlainEltBase {
        XmlReaderPlainEltBase {
            element: XmlReaderElementBase::new(name, xml_line_number, xml_file),
            parent,
        }
    }

    /// Port of `XmlReaderPlainElt::getParent` (XMLReaderHelper.h:174-177 @ v2.5.2).
    pub fn get_parent(&self) -> &ContainerEltRcPtr {
        &self.parent
    }
}

/// The data members of `XmlReaderComplexElt`, a container nested in another.
///
/// Port of `XmlReaderComplexElt` (XMLReaderHelper.h:306-344 @ v2.5.2): its identifier and
/// type name are its name, and its `appendMetadata` does nothing.
#[derive(Debug, Clone)]
pub struct XmlReaderComplexEltBase {
    /// The base class.
    pub element: XmlReaderElementBase,
    /// The parent: `None` for a root, which upstream's readers make with a null parent.
    parent: Option<ContainerEltRcPtr>,
}

impl XmlReaderComplexEltBase {
    /// Port of `XmlReaderComplexElt::XmlReaderComplexElt` (XMLReaderHelper.h:309-316 @
    /// v2.5.2).
    pub fn new(
        name: &[u8],
        parent: Option<ContainerEltRcPtr>,
        xml_line_number: u32,
        xml_file: &[u8],
    ) -> XmlReaderComplexEltBase {
        XmlReaderComplexEltBase {
            element: XmlReaderElementBase::new(name, xml_line_number, xml_file),
            parent,
        }
    }

    /// Port of `XmlReaderComplexElt::getParent` (XMLReaderHelper.h:322-325 @ v2.5.2).
    pub fn get_parent(&self) -> Option<&ContainerEltRcPtr> {
        self.parent.as_ref()
    }
}

/// Dummy to address unexpected parent for the DummyElt.
///
/// Port of `XmlReaderDummyElt::DummyParent` (XMLReaderHelper.h:197-232,
/// XMLReaderHelper.cpp:60-64 @ v2.5.2).
#[derive(Debug)]
struct DummyParent {
    element: XmlReaderElementBase,
}

impl DummyParent {
    fn new(parent: &Option<ElementRcPtr>) -> DummyParent {
        let element = match parent {
            Some(p) => {
                let p = p.borrow();
                XmlReaderElementBase::new(p.get_name(), p.get_xml_line_number(), p.get_xml_file())
            }
            None => XmlReaderElementBase::new(b"", 0, b""),
        };
        DummyParent { element }
    }
}

impl XmlReaderElement for DummyParent {
    fn element(&self) -> &XmlReaderElementBase {
        &self.element
    }
    fn element_mut(&mut self) -> &mut XmlReaderElementBase {
        &mut self.element
    }
    fn start(&mut self, _atts: &[&[u8]]) -> Result<()> {
        Ok(())
    }
    fn end(&mut self) -> Result<()> {
        Ok(())
    }
    fn is_container(&self) -> bool {
        true
    }
    fn get_identifier(&self) -> &[u8] {
        b"Unknown"
    }
    fn get_type_name(&self) -> &[u8] {
        self.get_identifier()
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

impl XmlReaderContainerElt for DummyParent {
    fn append_metadata(&mut self, _name: &[u8], _value: &[u8]) -> Result<()> {
        Ok(())
    }
}

/// Dummy to address unknown Element. When parsing meets some unrecognized data, a dummy is
/// used to continue parsing.
///
/// Port of `XmlReaderDummyElt` (XMLReaderHelper.h:192-268, XMLReaderHelper.cpp:66-92 @
/// v2.5.2).
#[derive(Debug)]
pub struct XmlReaderDummyElt {
    plain: XmlReaderPlainEltBase,
    raw_data: Vec<Vec<u8>>,
}

impl XmlReaderDummyElt {
    /// A dummy for the element `name` under `parent`, logging that it is unrecognized (with
    /// `msg`, if any).
    ///
    /// Port of `XmlReaderDummyElt::XmlReaderDummyElt` (XMLReaderHelper.cpp:66-86 @ v2.5.2).
    pub fn new(
        name: &[u8],
        parent: Option<ElementRcPtr>,
        xml_line_number: u32,
        xml_file: &[u8],
        msg: Option<&[u8]>,
    ) -> XmlReaderDummyElt {
        let dummy_parent: ContainerEltRcPtr = Rc::new(RefCell::new(DummyParent::new(&parent)));
        let elt = XmlReaderDummyElt {
            plain: XmlReaderPlainEltBase::new(name, dummy_parent, xml_line_number, xml_file),
            raw_data: Vec::new(),
        };
        let mut oss = OStringStream::new(Crt::NATIVE);
        oss.put_c_str(elt.get_xml_file());
        oss.put_str("(");
        oss.put_u32(elt.get_xml_line_number());
        oss.put_str("): ");
        oss.put_str("Unrecognized element '");
        oss.put_bytes(elt.get_name());
        oss.put_str("' where its parent is '");
        {
            let parent = elt.plain.get_parent().borrow();
            oss.put_c_str(parent.get_name());
            oss.put_str("' (");
            oss.put_u32(parent.get_xml_line_number());
        }
        oss.put_str(")");
        if let Some(msg) = msg {
            oss.put_str(": ");
            oss.put_c_str(msg);
        }
        oss.put_str(".");

        log_warning(oss.str());
        elt
    }

    /// The text the element held.
    pub fn raw_data(&self) -> &[Vec<u8>] {
        &self.raw_data
    }
}

impl XmlReaderElement for XmlReaderDummyElt {
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
    /// Port of `XmlReaderDummyElt::getIdentifier` (XMLReaderHelper.cpp:88-92 @ v2.5.2).
    fn get_identifier(&self) -> &[u8] {
        b""
    }
    fn get_type_name(&self) -> &[u8] {
        self.get_name()
    }
    fn is_dummy(&self) -> bool {
        true
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

impl XmlReaderPlainElt for XmlReaderDummyElt {
    fn set_raw_data(&mut self, s: &[u8], _xml_line: u32) -> Result<()> {
        self.raw_data.push(s.to_vec());
        Ok(())
    }
    fn get_parent(&self) -> &ContainerEltRcPtr {
        self.plain.get_parent()
    }
}

/// The description's element: its text goes to its parent's metadata.
///
/// Port of `XmlReaderDescriptionElt` (XMLReaderHelper.h:270-304, XMLReaderHelper.cpp:96-104
/// @ v2.5.2).
#[derive(Debug)]
pub struct XmlReaderDescriptionElt {
    plain: XmlReaderPlainEltBase,
    description: Vec<u8>,
    changed: bool,
}

impl XmlReaderDescriptionElt {
    /// Port of `XmlReaderDescriptionElt::XmlReaderDescriptionElt` (XMLReaderHelper.h:274-282
    /// @ v2.5.2).
    pub fn new(
        name: &[u8],
        parent: ContainerEltRcPtr,
        xml_location: u32,
        xml_file: &[u8],
    ) -> XmlReaderDescriptionElt {
        XmlReaderDescriptionElt {
            plain: XmlReaderPlainEltBase::new(name, parent, xml_location, xml_file),
            description: Vec::new(),
            changed: false,
        }
    }
}

impl XmlReaderElement for XmlReaderDescriptionElt {
    fn element(&self) -> &XmlReaderElementBase {
        &self.plain.element
    }
    fn element_mut(&mut self) -> &mut XmlReaderElementBase {
        &mut self.plain.element
    }
    fn start(&mut self, _atts: &[&[u8]]) -> Result<()> {
        self.description.clear();
        self.changed = false;
        Ok(())
    }
    /// Port of `XmlReaderDescriptionElt::end` (XMLReaderHelper.cpp:96-104 @ v2.5.2).
    fn end(&mut self) -> Result<()> {
        if self.changed {
            // Note: eXpat automatically replaces escaped characters with their original
            // values.
            let mut parent = self.plain.get_parent().borrow_mut();
            parent
                .as_container_mut()
                .expect("a plain element's parent is a container")
                .append_metadata(self.plain.element.get_name(), &self.description)?;
        }
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

impl XmlReaderPlainElt for XmlReaderDescriptionElt {
    fn set_raw_data(&mut self, s: &[u8], _xml_line: u32) -> Result<()> {
        // Keep adding to the string.
        self.description.extend_from_slice(s);
        self.changed = true;
        Ok(())
    }
    fn get_parent(&self) -> &ContainerEltRcPtr {
        self.plain.get_parent()
    }
}

/// The SOP node's base class, which its slope, offset and power elements fill.
///
/// Port of the virtual functions of `XmlReaderSOPNodeBaseElt` (XMLReaderHelper.h:346-406 @
/// v2.5.2) its children call.
pub trait XmlReaderSopNodeBaseElt {
    /// `getCDL`.
    fn get_cdl(&self) -> CdlOpDataRcPtr;
    /// The node's [`XmlReaderSopNodeBase`].
    fn sop_node(&mut self) -> &mut XmlReaderSopNodeBase;
}

/// The data members of `XmlReaderSOPNodeBaseElt`, and its non-virtual functions.
///
/// Port of `XmlReaderSOPNodeBaseElt` (XMLReaderHelper.h:346-406 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct XmlReaderSopNodeBase {
    /// The base class.
    pub complex: XmlReaderComplexEltBase,
    is_slope_init: bool,
    is_offset_init: bool,
    is_power_init: bool,
}

impl XmlReaderSopNodeBase {
    /// Port of `XmlReaderSOPNodeBaseElt::XmlReaderSOPNodeBaseElt` (XMLReaderHelper.h:349-359
    /// @ v2.5.2).
    pub fn new(
        name: &[u8],
        parent: ContainerEltRcPtr,
        xml_line_number: u32,
        xml_file: &[u8],
    ) -> XmlReaderSopNodeBase {
        XmlReaderSopNodeBase {
            complex: XmlReaderComplexEltBase::new(name, Some(parent), xml_line_number, xml_file),
            is_slope_init: false,
            is_offset_init: false,
            is_power_init: false,
        }
    }

    /// Port of `XmlReaderSOPNodeBaseElt::start` (XMLReaderHelper.h:361-364 @ v2.5.2).
    pub fn start(&mut self) -> Result<()> {
        self.is_slope_init = false;
        self.is_offset_init = false;
        self.is_power_init = false;
        Ok(())
    }

    /// Port of `XmlReaderSOPNodeBaseElt::end` (XMLReaderHelper.h:366-382 @ v2.5.2).
    pub fn end(&mut self) -> Result<()> {
        if !self.is_slope_init {
            return Err(self
                .complex
                .element
                .throw_message(b"Required node 'Slope' is missing. "));
        }
        if !self.is_offset_init {
            return Err(self
                .complex
                .element
                .throw_message(b"Required node 'Offset' is missing. "));
        }
        if !self.is_power_init {
            return Err(self
                .complex
                .element
                .throw_message(b"Required node 'Power' is missing. "));
        }
        Ok(())
    }

    /// Port of `XmlReaderSOPNodeBaseElt::setIsSlopeInit` (XMLReaderHelper.h:386 @ v2.5.2).
    pub fn set_is_slope_init(&mut self, status: bool) {
        self.is_slope_init = status;
    }

    /// Port of `XmlReaderSOPNodeBaseElt::setIsOffsetInit` (XMLReaderHelper.h:387 @ v2.5.2).
    pub fn set_is_offset_init(&mut self, status: bool) {
        self.is_offset_init = status;
    }

    /// Port of `XmlReaderSOPNodeBaseElt::setIsPowerInit` (XMLReaderHelper.h:388 @ v2.5.2).
    pub fn set_is_power_init(&mut self, status: bool) {
        self.is_power_init = status;
    }

    /// Adds the description `value` to the CDL's metadata (the name is ignored).
    ///
    /// Port of `XmlReaderSOPNodeBaseElt::appendMetadata` (XMLReaderHelper.h:390-395 @
    /// v2.5.2).
    pub fn append_metadata(cdl: &CdlOpDataRcPtr, _name: &[u8], value: &[u8]) -> Result<()> {
        // Add description to parent and override name.
        let item = FormatMetadataImpl::new(METADATA_SOP_DESCRIPTION, value)?;
        cdl.borrow_mut()
            .get_format_metadata_mut()
            .get_children_elements_mut()
            .push(item);
        Ok(())
    }
}

/// The SatNode's base class, which its saturation element fills.
///
/// Port of the virtual functions of `XmlReaderSatNodeBaseElt` (XMLReaderHelper.h:432-464 @
/// v2.5.2) its children call.
pub trait XmlReaderSatNodeBaseElt {
    /// `getCDL`.
    fn get_cdl(&self) -> CdlOpDataRcPtr;
}

/// Adds the description `value` to the CDL's metadata (the name is ignored).
///
/// Port of `XmlReaderSatNodeBaseElt::appendMetadata` (XMLReaderHelper.h:453-458 @ v2.5.2).
/// `XmlReaderSatNodeBaseElt`'s `start` and `end` do nothing, and its data members are
/// [`XmlReaderComplexEltBase`]'s.
pub fn sat_node_append_metadata(cdl: &CdlOpDataRcPtr, _name: &[u8], value: &[u8]) -> Result<()> {
    // Add description to parent and override name.
    let item = FormatMetadataImpl::new(METADATA_SAT_DESCRIPTION, value)?;
    cdl.borrow_mut()
        .get_format_metadata_mut()
        .get_children_elements_mut()
        .push(item);
    Ok(())
}

/// The numbers of an element's text, or the "Illegal values" error.
fn content_numbers(element: &XmlReaderElementBase, content_data: &mut Vec<u8>) -> Result<Vec<f64>> {
    trim(content_data);

    match get_numbers::<f64>(content_data, content_data.len()) {
        Ok(data) => Ok(data),
        Err(_) => {
            let s = truncate_string(content_data, content_data.len());
            let mut oss = OStringStream::new(Crt::NATIVE);
            oss.put_str("Illegal values '");
            oss.put_bytes(&s);
            oss.put_str("' in ");
            oss.put_c_str(element.get_name());
            Err(element.throw_message(oss.str()))
        }
    }
}

/// The slope, offset and power elements.
///
/// Port of `XmlReaderSOPValueElt` (XMLReaderHelper.h:408-430, XMLReaderHelper.cpp:153-224 @
/// v2.5.2).
#[derive(Debug)]
pub struct XmlReaderSopValueElt {
    plain: XmlReaderPlainEltBase,
    content_data: Vec<u8>,
}

impl XmlReaderSopValueElt {
    /// Port of `XmlReaderSOPValueElt::XmlReaderSOPValueElt` (XMLReaderHelper.cpp:153-160 @
    /// v2.5.2).
    pub fn new(
        name: &[u8],
        parent: ContainerEltRcPtr,
        xml_line_number: u32,
        xml_file: &[u8],
    ) -> XmlReaderSopValueElt {
        XmlReaderSopValueElt {
            plain: XmlReaderPlainEltBase::new(name, parent, xml_line_number, xml_file),
            content_data: Vec::new(),
        }
    }
}

impl XmlReaderElement for XmlReaderSopValueElt {
    fn element(&self) -> &XmlReaderElementBase {
        &self.plain.element
    }
    fn element_mut(&mut self) -> &mut XmlReaderElementBase {
        &mut self.plain.element
    }
    /// Port of `XmlReaderSOPValueElt::start` (XMLReaderHelper.cpp:166-169 @ v2.5.2).
    fn start(&mut self, _atts: &[&[u8]]) -> Result<()> {
        self.content_data.clear();
        Ok(())
    }
    /// Port of `XmlReaderSOPValueElt::end` (XMLReaderHelper.cpp:171-217 @ v2.5.2).
    fn end(&mut self) -> Result<()> {
        let data = content_numbers(&self.plain.element, &mut self.content_data)?;

        if data.len() != 3 {
            return Err(self.throw_message(b"SOPNode: 3 values required."));
        }

        let mut parent = self.plain.get_parent().borrow_mut();
        // The readers make a SOP value element only in a SOP node.
        let sop_node_elt = parent
            .as_sop_node_base_mut()
            .expect("the parent of a SOP value is a SOP node");
        let cdl = sop_node_elt.get_cdl();
        let params = ChannelParams::new(data[0], data[1], data[2]);

        let name = self.plain.element.get_name();
        if name == TAG_SLOPE {
            cdl.borrow_mut().set_slope_params(params);
            sop_node_elt.sop_node().set_is_slope_init(true);
        } else if name == TAG_OFFSET {
            cdl.borrow_mut().set_offset_params(params);
            sop_node_elt.sop_node().set_is_offset_init(true);
        } else if name == TAG_POWER {
            cdl.borrow_mut().set_power_params(params);
            sop_node_elt.sop_node().set_is_power_init(true);
        }
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

impl XmlReaderPlainElt for XmlReaderSopValueElt {
    /// Port of `XmlReaderSOPValueElt::setRawData` (XMLReaderHelper.cpp:219-224 @ v2.5.2).
    fn set_raw_data(&mut self, s: &[u8], _xml_line: u32) -> Result<()> {
        self.content_data.extend_from_slice(s);
        self.content_data.push(b' ');
        Ok(())
    }
    fn get_parent(&self) -> &ContainerEltRcPtr {
        self.plain.get_parent()
    }
}

/// The CDL Saturation element.
///
/// Port of `XmlReaderSaturationElt` (XMLReaderHelper.h:466-488, XMLReaderHelper.cpp:228-282 @
/// v2.5.2).
#[derive(Debug)]
pub struct XmlReaderSaturationElt {
    plain: XmlReaderPlainEltBase,
    content_data: Vec<u8>,
}

impl XmlReaderSaturationElt {
    /// Port of `XmlReaderSaturationElt::XmlReaderSaturationElt` (XMLReaderHelper.cpp:228-234
    /// @ v2.5.2).
    pub fn new(
        name: &[u8],
        parent: ContainerEltRcPtr,
        xml_line_number: u32,
        xml_file: &[u8],
    ) -> XmlReaderSaturationElt {
        XmlReaderSaturationElt {
            plain: XmlReaderPlainEltBase::new(name, parent, xml_line_number, xml_file),
            content_data: Vec::new(),
        }
    }
}

impl XmlReaderElement for XmlReaderSaturationElt {
    fn element(&self) -> &XmlReaderElementBase {
        &self.plain.element
    }
    fn element_mut(&mut self) -> &mut XmlReaderElementBase {
        &mut self.plain.element
    }
    /// Port of `XmlReaderSaturationElt::start` (XMLReaderHelper.cpp:240-243 @ v2.5.2).
    fn start(&mut self, _atts: &[&[u8]]) -> Result<()> {
        self.content_data.clear();
        Ok(())
    }
    /// Port of `XmlReaderSaturationElt::end` (XMLReaderHelper.cpp:245-275 @ v2.5.2).
    fn end(&mut self) -> Result<()> {
        let data = content_numbers(&self.plain.element, &mut self.content_data)?;

        if data.len() != 1 {
            return Err(self.throw_message(b"SatNode: non-single value. "));
        }

        let mut parent = self.plain.get_parent().borrow_mut();
        // The readers make a saturation element only in a SatNode.
        let sat_node_elt = parent
            .as_sat_node_base_mut()
            .expect("the parent of a saturation is a SatNode");
        let cdl = sat_node_elt.get_cdl();

        if self.plain.element.get_name() == TAG_SATURATION {
            cdl.borrow_mut().set_saturation(data[0]);
        }
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

impl XmlReaderPlainElt for XmlReaderSaturationElt {
    /// Port of `XmlReaderSaturationElt::setRawData` (XMLReaderHelper.cpp:277-280 @ v2.5.2).
    fn set_raw_data(&mut self, s: &[u8], _xml_line: u32) -> Result<()> {
        self.content_data.extend_from_slice(s);
        self.content_data.push(b' ');
        Ok(())
    }
    fn get_parent(&self) -> &ContainerEltRcPtr {
        self.plain.get_parent()
    }
}

/// Stack of elements.
///
/// Port of `XmlReaderElementStack` (XMLReaderHelper.h:490-519, XMLReaderHelper.cpp:108-149 @
/// v2.5.2). `back` and `front` of an empty stack are undefined in C++; here they are `None`.
#[derive(Debug, Default)]
pub struct XmlReaderElementStack {
    elms: Vec<ElementRcPtr>,
}

impl XmlReaderElementStack {
    /// Port of `XmlReaderElementStack::XmlReaderElementStack` (XMLReaderHelper.cpp:108-110 @
    /// v2.5.2).
    pub fn new() -> XmlReaderElementStack {
        XmlReaderElementStack { elms: Vec::new() }
    }

    /// Port of `XmlReaderElementStack::empty` (XMLReaderHelper.cpp:122-125 @ v2.5.2).
    pub fn empty(&self) -> bool {
        self.elms.is_empty()
    }

    /// Port of `XmlReaderElementStack::size` (XMLReaderHelper.cpp:117-120 @ v2.5.2).
    pub fn size(&self) -> u32 {
        self.elms.len() as u32
    }

    /// Port of `XmlReaderElementStack::push_back` (XMLReaderHelper.cpp:127-130 @ v2.5.2).
    pub fn push_back(&mut self, elt: ElementRcPtr) {
        self.elms.push(elt);
    }

    /// Port of `XmlReaderElementStack::pop_back` (XMLReaderHelper.cpp:132-135 @ v2.5.2).
    pub fn pop_back(&mut self) {
        self.elms.pop();
    }

    /// Port of `XmlReaderElementStack::back` (XMLReaderHelper.cpp:137-140 @ v2.5.2).
    pub fn back(&self) -> Option<ElementRcPtr> {
        self.elms.last().cloned()
    }

    /// Port of `XmlReaderElementStack::front` (XMLReaderHelper.cpp:142-145 @ v2.5.2).
    pub fn front(&self) -> Option<ElementRcPtr> {
        self.elms.first().cloned()
    }

    /// Port of `XmlReaderElementStack::clear` (XMLReaderHelper.cpp:147-150 @ v2.5.2).
    pub fn clear(&mut self) {
        self.elms.clear();
    }
}
