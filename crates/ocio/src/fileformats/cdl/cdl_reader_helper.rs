// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The elements of the CDL, CCC and CC readers.
//!
//! Port of `src/OpenColorIO/fileformats/cdl/CDLReaderHelper.h` and `CDLReaderHelper.cpp`
//! (@ v2.5.2), on the element classes of `ocio_formats::fileformats::xmlutils`.

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use ocio_formats::fileformats::xmlutils::xml_reader_helper::{
    CdlOpDataRcPtr, ContainerEltRcPtr, XmlReaderComplexEltBase, XmlReaderContainerElt,
    XmlReaderElement, XmlReaderElementBase, XmlReaderSatNodeBaseElt, XmlReaderSopNodeBase,
    XmlReaderSopNodeBaseElt, sat_node_append_metadata,
};
use ocio_formats::fileformats::xmlutils::xml_reader_utils::ATTR_ID;
use ocio_ops::Result;
use ocio_ops::format_metadata::FormatMetadataImpl;
use ocio_ops::ops::cdl::cdl_op_data::CdlOpData;

use crate::transforms::cdl_transform::CdlTransform;

/// A shared CDL transform: `CDLTransformImplRcPtr`.
pub type CdlTransformRcPtr = Arc<CdlTransform>;
/// `CDLTransformVec` (src/OpenColorIO/transforms/CDLTransform.h:23 @ v2.5.2).
pub type CdlTransformVec = Vec<CdlTransformRcPtr>;

/// What a parse found: the transforms and the root's metadata.
///
/// Port of `CDLParsingInfo` (CDLReaderHelper.h:14-24 @ v2.5.2).
#[derive(Debug, Default)]
pub struct CdlParsingInfo {
    /// `m_transforms`.
    pub transforms: CdlTransformVec,
    /// `m_metadata`.
    pub metadata: FormatMetadataImpl,
}

/// A shared [`CdlParsingInfo`]: `CDLParsingInfoRcPtr`.
pub type CdlParsingInfoRcPtr = Rc<RefCell<CdlParsingInfo>>;

/// The `as_any` and `as_any_mut` of an element.
macro_rules! any_impls {
    () => {
        fn as_any(&self) -> &dyn Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
    };
}

/// The root of a CDL: a `ColorDecisionList`.
///
/// Port of `CDLReaderColorDecisionListElt` (CDLReaderHelper.h:27-70 @ v2.5.2).
#[derive(Debug)]
pub struct CdlReaderColorDecisionListElt {
    element: XmlReaderElementBase,
    parsing_info: CdlParsingInfoRcPtr,
}

impl CdlReaderColorDecisionListElt {
    /// Port of `CDLReaderColorDecisionListElt::CDLReaderColorDecisionListElt`
    /// (CDLReaderHelper.h:30-36 @ v2.5.2).
    pub fn new(name: &[u8], xml_line_number: u32, xml_file: &[u8]) -> Self {
        CdlReaderColorDecisionListElt {
            element: XmlReaderElementBase::new(name, xml_line_number, xml_file),
            parsing_info: Rc::new(RefCell::new(CdlParsingInfo::default())),
        }
    }

    /// Port of `CDLReaderColorDecisionListElt::getCDLParsingInfo` (CDLReaderHelper.h:52-55 @
    /// v2.5.2).
    pub fn get_cdl_parsing_info(&self) -> &CdlParsingInfoRcPtr {
        &self.parsing_info
    }
}

impl XmlReaderElement for CdlReaderColorDecisionListElt {
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
        self.get_name()
    }
    fn get_type_name(&self) -> &[u8] {
        self.get_name()
    }
    fn as_container_mut(&mut self) -> Option<&mut dyn XmlReaderContainerElt> {
        Some(self)
    }
    any_impls!();
}

impl XmlReaderContainerElt for CdlReaderColorDecisionListElt {
    /// Port of `CDLReaderColorDecisionListElt::appendMetadata` (CDLReaderHelper.h:57-60 @
    /// v2.5.2).
    fn append_metadata(&mut self, name: &[u8], value: &[u8]) -> Result<()> {
        self.parsing_info
            .borrow_mut()
            .metadata
            .add_child_element(Some(name), Some(value))
    }
}

/// A `ColorDecision` of a CDL.
///
/// Port of `CDLReaderColorDecisionElt` (CDLReaderHelper.h:72-105 @ v2.5.2).
#[derive(Debug)]
pub struct CdlReaderColorDecisionElt {
    complex: XmlReaderComplexEltBase,
    metadata: FormatMetadataImpl,
}

impl CdlReaderColorDecisionElt {
    /// Port of `CDLReaderColorDecisionElt::CDLReaderColorDecisionElt` (CDLReaderHelper.h:75-82
    /// @ v2.5.2).
    pub fn new(
        name: &[u8],
        parent: ContainerEltRcPtr,
        xml_line_number: u32,
        xml_file: &[u8],
    ) -> Self {
        CdlReaderColorDecisionElt {
            complex: XmlReaderComplexEltBase::new(name, Some(parent), xml_line_number, xml_file),
            metadata: FormatMetadataImpl::default(),
        }
    }

    /// `getParent`: the parser makes a ColorDecision only under a ColorDecisionList.
    pub fn get_parent(&self) -> &ContainerEltRcPtr {
        self.complex
            .get_parent()
            .expect("a ColorDecision has a parent")
    }
}

impl XmlReaderElement for CdlReaderColorDecisionElt {
    fn element(&self) -> &XmlReaderElementBase {
        &self.complex.element
    }
    fn element_mut(&mut self) -> &mut XmlReaderElementBase {
        &mut self.complex.element
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
        self.get_name()
    }
    fn get_type_name(&self) -> &[u8] {
        self.get_name()
    }
    fn as_container_mut(&mut self) -> Option<&mut dyn XmlReaderContainerElt> {
        Some(self)
    }
    any_impls!();
}

impl XmlReaderContainerElt for CdlReaderColorDecisionElt {
    /// Port of `CDLReaderColorDecisionElt::appendMetadata` (CDLReaderHelper.h:92-95 @
    /// v2.5.2).
    fn append_metadata(&mut self, name: &[u8], value: &[u8]) -> Result<()> {
        self.metadata.add_child_element(Some(name), Some(value))
    }
}

/// The root of a CCC: a `ColorCorrectionCollection`.
///
/// Port of `CDLReaderColorCorrectionCollectionElt` (CDLReaderHelper.h:107-154 @ v2.5.2).
#[derive(Debug)]
pub struct CdlReaderColorCorrectionCollectionElt {
    element: XmlReaderElementBase,
    parsing_info: CdlParsingInfoRcPtr,
}

impl CdlReaderColorCorrectionCollectionElt {
    /// Port of `CDLReaderColorCorrectionCollectionElt::CDLReaderColorCorrectionCollectionElt`
    /// (CDLReaderHelper.h:110-116 @ v2.5.2).
    pub fn new(name: &[u8], xml_line_number: u32, xml_file: &[u8]) -> Self {
        CdlReaderColorCorrectionCollectionElt {
            element: XmlReaderElementBase::new(name, xml_line_number, xml_file),
            parsing_info: Rc::new(RefCell::new(CdlParsingInfo::default())),
        }
    }

    /// Port of `CDLReaderColorCorrectionCollectionElt::getCDLParsingInfo`
    /// (CDLReaderHelper.h:136-139 @ v2.5.2).
    pub fn get_cdl_parsing_info(&self) -> &CdlParsingInfoRcPtr {
        &self.parsing_info
    }
}

impl XmlReaderElement for CdlReaderColorCorrectionCollectionElt {
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
        self.get_name()
    }
    fn get_type_name(&self) -> &[u8] {
        self.get_name()
    }
    fn as_container_mut(&mut self) -> Option<&mut dyn XmlReaderContainerElt> {
        Some(self)
    }
    any_impls!();
}

impl XmlReaderContainerElt for CdlReaderColorCorrectionCollectionElt {
    /// Port of `CDLReaderColorCorrectionCollectionElt::appendMetadata`
    /// (CDLReaderHelper.h:141-144 @ v2.5.2).
    fn append_metadata(&mut self, name: &[u8], value: &[u8]) -> Result<()> {
        self.parsing_info
            .borrow_mut()
            .metadata
            .add_child_element(Some(name), Some(value))
    }
}

/// A `ColorCorrection`: one CDL transform.
///
/// Port of `CDLReaderColorCorrectionElt` (CDLReaderHelper.h:156-177, CDLReaderHelper.cpp:10-85
/// @ v2.5.2).
#[derive(Debug)]
pub struct CdlReaderColorCorrectionElt {
    complex: XmlReaderComplexEltBase,
    parsing_info: Option<CdlParsingInfoRcPtr>,
    transform_data: CdlOpDataRcPtr,
}

impl CdlReaderColorCorrectionElt {
    /// Port of `CDLReaderColorCorrectionElt::CDLReaderColorCorrectionElt`
    /// (CDLReaderHelper.cpp:10-18 @ v2.5.2).
    pub fn new(
        name: &[u8],
        parent: Option<ContainerEltRcPtr>,
        xml_location: u32,
        xml_file: &[u8],
    ) -> Self {
        CdlReaderColorCorrectionElt {
            complex: XmlReaderComplexEltBase::new(name, parent, xml_location, xml_file),
            parsing_info: None,
            transform_data: Rc::new(RefCell::new(CdlOpData::new_default())),
        }
    }

    /// Port of `CDLReaderColorCorrectionElt::getCDL` (CDLReaderHelper.h:168 @ v2.5.2).
    pub fn get_cdl(&self) -> &CdlOpDataRcPtr {
        &self.transform_data
    }

    /// Port of `CDLReaderColorCorrectionElt::setCDLParsingInfo` (CDLReaderHelper.cpp:74-77 @
    /// v2.5.2).
    pub fn set_cdl_parsing_info(&mut self, parsing_info: &CdlParsingInfoRcPtr) {
        self.parsing_info = Some(parsing_info.clone());
    }

    /// `getParent`: `None` for the root of a CC.
    pub fn get_parent(&self) -> Option<&ContainerEltRcPtr> {
        self.complex.get_parent()
    }
}

impl XmlReaderElement for CdlReaderColorCorrectionElt {
    fn element(&self) -> &XmlReaderElementBase {
        &self.complex.element
    }
    fn element_mut(&mut self) -> &mut XmlReaderElementBase {
        &mut self.complex.element
    }

    /// Takes the `id` attribute.
    ///
    /// Port of `CDLReaderColorCorrectionElt::start` (CDLReaderHelper.cpp:20-40 @ v2.5.2):
    /// expat gives every attribute a value, so its "Missing attribute value for id" error is
    /// never thrown.
    fn start(&mut self, atts: &[&[u8]]) -> Result<()> {
        for pair in atts.chunks(2) {
            if pair[0] == ATTR_ID {
                // Note: eXpat automatically replaces escaped characters with their original
                // values.
                self.transform_data.borrow_mut().set_id(pair[1]);
            }
        }
        Ok(())
    }

    /// Makes the CDL transform of the element's data, validated, and adds it to the parse's.
    ///
    /// Port of `CDLReaderColorCorrectionElt::end` (CDLReaderHelper.cpp:42-72 @ v2.5.2).
    fn end(&mut self) -> Result<()> {
        let mut transform = CdlTransform::new();

        let data = self.transform_data.borrow();
        let mut vec9 = [0.0f64; 9];
        let slopes = data.get_slope_params().get_rgb();
        vec9[0] = slopes[0];
        vec9[1] = slopes[1];
        vec9[2] = slopes[2];

        let offsets = data.get_offset_params().get_rgb();
        vec9[3] = offsets[0];
        vec9[4] = offsets[1];
        vec9[5] = offsets[2];

        let powers = data.get_power_params().get_rgb();
        vec9[6] = powers[0];
        vec9[7] = powers[1];
        vec9[8] = powers[2];
        transform.set_sop(&vec9);

        transform.set_sat(data.get_saturation());

        *transform.format_metadata_mut() = data.get_format_metadata().clone();

        transform.validate()?;

        self.parsing_info
            .as_ref()
            .expect("the parser sets the parsing info of a ColorCorrection")
            .borrow_mut()
            .transforms
            .push(Arc::new(transform));
        Ok(())
    }

    fn is_container(&self) -> bool {
        true
    }
    fn get_identifier(&self) -> &[u8] {
        self.get_name()
    }
    fn get_type_name(&self) -> &[u8] {
        self.get_name()
    }
    fn as_container_mut(&mut self) -> Option<&mut dyn XmlReaderContainerElt> {
        Some(self)
    }
    any_impls!();
}

impl XmlReaderContainerElt for CdlReaderColorCorrectionElt {
    /// Keeps description as metadata with supplied name.
    ///
    /// Port of `CDLReaderColorCorrectionElt::appendMetadata` (CDLReaderHelper.cpp:79-84 @
    /// v2.5.2).
    fn append_metadata(&mut self, name: &[u8], value: &[u8]) -> Result<()> {
        let item = FormatMetadataImpl::new(name, value)?;
        self.transform_data
            .borrow_mut()
            .get_format_metadata_mut()
            .get_children_elements_mut()
            .push(item);
        Ok(())
    }
}

/// The CDL of the `ColorCorrection` `parent`: `static_cast<CDLReaderColorCorrectionElt *>(
/// getParent().get())->getCDL()`. The parser makes SOP and Sat nodes only in a
/// `ColorCorrection`.
fn parent_cdl(parent: Option<&ContainerEltRcPtr>) -> CdlOpDataRcPtr {
    parent
        .expect("a SOP or Sat node has a parent")
        .borrow()
        .as_any()
        .downcast_ref::<CdlReaderColorCorrectionElt>()
        .expect("the parent of a SOP or Sat node is a ColorCorrection")
        .get_cdl()
        .clone()
}

/// The `SOPNode` element in the CDL/CCC/CC schemas.
///
/// Port of `CDLReaderSOPNodeCCElt` (CDLReaderHelper.h:179-195 @ v2.5.2).
#[derive(Debug)]
pub struct CdlReaderSopNodeCcElt {
    sop: XmlReaderSopNodeBase,
}

impl CdlReaderSopNodeCcElt {
    /// Port of `CDLReaderSOPNodeCCElt::CDLReaderSOPNodeCCElt` (CDLReaderHelper.h:183-189 @
    /// v2.5.2).
    pub fn new(name: &[u8], parent: ContainerEltRcPtr, xml_location: u32, xml_file: &[u8]) -> Self {
        CdlReaderSopNodeCcElt {
            sop: XmlReaderSopNodeBase::new(name, parent, xml_location, xml_file),
        }
    }
}

impl XmlReaderElement for CdlReaderSopNodeCcElt {
    fn element(&self) -> &XmlReaderElementBase {
        &self.sop.complex.element
    }
    fn element_mut(&mut self) -> &mut XmlReaderElementBase {
        &mut self.sop.complex.element
    }
    fn start(&mut self, _atts: &[&[u8]]) -> Result<()> {
        self.sop.start()
    }
    fn end(&mut self) -> Result<()> {
        self.sop.end()
    }
    fn is_container(&self) -> bool {
        true
    }
    fn get_identifier(&self) -> &[u8] {
        self.get_name()
    }
    fn get_type_name(&self) -> &[u8] {
        self.get_name()
    }
    fn as_container_mut(&mut self) -> Option<&mut dyn XmlReaderContainerElt> {
        Some(self)
    }
    fn as_sop_node_base_mut(&mut self) -> Option<&mut dyn XmlReaderSopNodeBaseElt> {
        Some(self)
    }
    any_impls!();
}

impl XmlReaderContainerElt for CdlReaderSopNodeCcElt {
    fn append_metadata(&mut self, name: &[u8], value: &[u8]) -> Result<()> {
        let cdl = self.get_cdl();
        XmlReaderSopNodeBase::append_metadata(&cdl, name, value)
    }
}

impl XmlReaderSopNodeBaseElt for CdlReaderSopNodeCcElt {
    /// Port of `CDLReaderSOPNodeCCElt::getCDL` (CDLReaderHelper.h:191-194 @ v2.5.2).
    fn get_cdl(&self) -> CdlOpDataRcPtr {
        parent_cdl(self.sop.complex.get_parent())
    }
    fn sop_node(&mut self) -> &mut XmlReaderSopNodeBase {
        &mut self.sop
    }
}

/// The `SatNode` element in the CDL/CCC/CC schemas.
///
/// Port of `CDLReaderSatNodeCCElt` (CDLReaderHelper.h:197-213 @ v2.5.2), on
/// `XmlReaderSatNodeBaseElt` (whose `start` and `end` do nothing).
#[derive(Debug)]
pub struct CdlReaderSatNodeCcElt {
    complex: XmlReaderComplexEltBase,
}

impl CdlReaderSatNodeCcElt {
    /// Port of `CDLReaderSatNodeCCElt::CDLReaderSatNodeCCElt` (CDLReaderHelper.h:201-207 @
    /// v2.5.2).
    pub fn new(
        name: &[u8],
        parent: ContainerEltRcPtr,
        xml_line_number: u32,
        xml_file: &[u8],
    ) -> Self {
        CdlReaderSatNodeCcElt {
            complex: XmlReaderComplexEltBase::new(name, Some(parent), xml_line_number, xml_file),
        }
    }
}

impl XmlReaderElement for CdlReaderSatNodeCcElt {
    fn element(&self) -> &XmlReaderElementBase {
        &self.complex.element
    }
    fn element_mut(&mut self) -> &mut XmlReaderElementBase {
        &mut self.complex.element
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
        self.get_name()
    }
    fn get_type_name(&self) -> &[u8] {
        self.get_name()
    }
    fn as_container_mut(&mut self) -> Option<&mut dyn XmlReaderContainerElt> {
        Some(self)
    }
    fn as_sat_node_base_mut(&mut self) -> Option<&mut dyn XmlReaderSatNodeBaseElt> {
        Some(self)
    }
    any_impls!();
}

impl XmlReaderContainerElt for CdlReaderSatNodeCcElt {
    fn append_metadata(&mut self, name: &[u8], value: &[u8]) -> Result<()> {
        let cdl = self.get_cdl();
        sat_node_append_metadata(&cdl, name, value)
    }
}

impl XmlReaderSatNodeBaseElt for CdlReaderSatNodeCcElt {
    /// Port of `CDLReaderSatNodeCCElt::getCDL` (CDLReaderHelper.h:209-212 @ v2.5.2).
    fn get_cdl(&self) -> CdlOpDataRcPtr {
        parent_cdl(self.complex.get_parent())
    }
}
