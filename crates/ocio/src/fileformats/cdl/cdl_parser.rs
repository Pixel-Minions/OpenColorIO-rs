// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The parser of CDL, CCC and CC files.
//!
//! Port of `src/OpenColorIO/fileformats/cdl/CDLParser.h` and `CDLParser.cpp` (@ v2.5.2).
//!
//! Upstream's `CDLParser::Impl` is expat's user data, and its static handlers reach it through
//! a pointer. Here the parser's state is shared (`Rc<RefCell<State>>`) between the parser and
//! the closures that are expat's handlers; a handler's exception is its `Err`, which ends
//! `XML_Parse` and is returned as is, as upstream's C++ exceptions propagate.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use ocio_formats::expat::xmlparse::{ParseError, Parser, XmlError, XmlStatus, xml_error_string};
use ocio_formats::fileformats::xmlutils::xml_reader_helper::{
    ElementRcPtr, XmlReaderDescriptionElt, XmlReaderDummyElt, XmlReaderElementStack,
    XmlReaderSaturationElt, XmlReaderSopValueElt,
};
use ocio_formats::fileformats::xmlutils::xml_reader_utils::{
    CDL_TAG_COLOR_CORRECTION, TAG_DESCRIPTION, TAG_OFFSET, TAG_POWER, TAG_SATNODE, TAG_SATNODEALT,
    TAG_SATURATION, TAG_SLOPE, TAG_SOPNODE, find_sub_string,
};
use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::format_metadata::FormatMetadataImpl;
use ocio_ops::{Exception, Result};

use super::cdl_reader_helper::{
    CdlParsingInfo, CdlParsingInfoRcPtr, CdlReaderColorCorrectionCollectionElt,
    CdlReaderColorCorrectionElt, CdlReaderColorDecisionElt, CdlReaderColorDecisionListElt,
    CdlReaderSatNodeCcElt, CdlReaderSopNodeCcElt, CdlTransformRcPtr, CdlTransformVec,
};
use crate::fileformats::input_stream::InputStream;
use crate::transforms::cdl_transform::{METADATA_INPUT_DESCRIPTION, METADATA_VIEWING_DESCRIPTION};

/// `CDL_TAG_COLOR_DECISION_LIST` (CDLParser.h:50 @ v2.5.2).
pub const CDL_TAG_COLOR_DECISION_LIST: &[u8] = b"ColorDecisionList";
/// `CDL_TAG_COLOR_CORRECTION_COLLECTION` (CDLParser.h:51 @ v2.5.2).
pub const CDL_TAG_COLOR_CORRECTION_COLLECTION: &[u8] = b"ColorCorrectionCollection";
/// `CDL_TAG_COLOR_DECISION` (CDLParser.h:52 @ v2.5.2).
pub const CDL_TAG_COLOR_DECISION: &[u8] = b"ColorDecision";

/// `CDLTransformMap` (src/OpenColorIO/transforms/CDLTransform.h:22 @ v2.5.2): the transforms
/// by id, ordered as a `std::map<std::string, ...>` orders them.
pub type CdlTransformMap = BTreeMap<Vec<u8>, CdlTransformRcPtr>;

/// `s` up to its first null, as a `const char *` reads it.
fn c_str(s: &[u8]) -> &[u8] {
    s.iter().position(|&b| b == 0).map_or(s, |n| &s[..n])
}

/// The schema of the file, which picks the start element handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Schema {
    /// `StartElementHandlerCDL`.
    Cdl,
    /// `StartElementHandlerCCC`.
    Ccc,
    /// `StartElementHandlerCC`.
    Cc,
}

/// The data members of `CDLParser::Impl` but the expat parser.
///
/// Port of `CDLParser::Impl` (CDLParser.cpp:17-160 @ v2.5.2).
#[derive(Debug)]
struct State {
    /// `m_elms`.
    elms: XmlReaderElementStack,
    /// `m_parsingInfo`.
    parsing_info: Option<CdlParsingInfoRcPtr>,
    /// `m_lineNumber`.
    line_number: u32,
    /// `m_fileName`.
    file_name: Vec<u8>,
    /// `m_isCC`.
    is_cc: bool,
    /// `m_isCCC`.
    is_ccc: bool,
}

type StateRc = Rc<RefCell<State>>;

impl State {
    /// The parse's error: "Error parsing <root> (<file>). Error is: <error>. At line (<n>)".
    ///
    /// Port of `CDLParser::Impl::throwMessage` (CDLParser.cpp:218-239 @ v2.5.2).
    fn throw_message(&self, error: &[u8]) -> Exception {
        let mut os = OStringStream::new(Crt::NATIVE);
        os.put_str("Error parsing ");
        if self.is_cc {
            os.put_bytes(CDL_TAG_COLOR_CORRECTION);
        } else if self.is_ccc {
            os.put_bytes(CDL_TAG_COLOR_CORRECTION_COLLECTION);
        } else {
            os.put_bytes(CDL_TAG_COLOR_DECISION_LIST);
        }
        os.put_str(" (");
        os.put_c_str(&self.file_name);
        os.put_str("). ");
        os.put_str("Error is: ");
        os.put_c_str(error);
        os.put_str(". At line (");
        os.put_u32(self.line_number);
        os.put_str(")");
        Exception::new(os.into_bytes())
    }

    /// Port of `CDLParser::Impl::getXmlLocation` (CDLParser.cpp:350-353 @ v2.5.2).
    fn get_xml_location(&self) -> u32 {
        self.line_number
    }

    /// Port of `CDLParser::Impl::getXmlFilename` (CDLParser.cpp:355-359 @ v2.5.2).
    fn get_xml_filename(&self) -> &[u8] {
        if self.file_name.is_empty() {
            b"File name not specified"
        } else {
            &self.file_name
        }
    }

    /// Port of `CDLParser::Impl::reset` (CDLParser.cpp:361-373 @ v2.5.2).
    fn reset(&mut self) {
        if let Some(parsing_info) = &self.parsing_info {
            parsing_info.borrow_mut().transforms.clear();
        }

        self.elms.clear();

        self.line_number = 0;
        self.is_cc = false;
        self.is_ccc = false;
    }

    /// A dummy element for `name`, under the last element.
    ///
    /// Port of `CDLParser::Impl::createDummyElement` (CDLParser.cpp:375-383 @ v2.5.2).
    fn create_dummy_element(&self, name: &[u8], msg: &[u8]) -> ElementRcPtr {
        Rc::new(RefCell::new(XmlReaderDummyElt::new(
            name,
            self.get_back_element(),
            self.get_xml_location(),
            self.get_xml_filename(),
            Some(msg),
        )))
    }

    /// Port of `CDLParser::Impl::getBackElement` (CDLParser.cpp:395-398 @ v2.5.2).
    fn get_back_element(&self) -> Option<ElementRcPtr> {
        self.elms.back()
    }

    /// Whether the last element is a `T`.
    ///
    /// Port of `CDLParser::Impl::isBackElementInstanceOf` (CDLParser.cpp:400-404 @ v2.5.2).
    fn is_back_element_instance_of<T: 'static>(&self) -> bool {
        self.get_back_element()
            .is_some_and(|back| back.borrow().as_any().is::<T>())
    }

    /// The parent of an element `createElement` makes: the last element if it is a container
    /// (`dynamic_pointer_cast<XmlReaderContainerElt>`), else none.
    ///
    /// Port of the parent of `CDLParser::Impl::createElement` (CDLParser.cpp:385-393 @ v2.5.2).
    fn create_parent(&self) -> Option<ElementRcPtr> {
        self.get_back_element()
            .filter(|back| back.borrow().is_container())
    }

    /// [`State::create_parent`] where the parser checked the last element is a container.
    fn back_container(&self) -> ElementRcPtr {
        self.create_parent().expect("an element under a container")
    }
}

/// Check if a description tag matches the required schema.
///
/// Port of `CDLParser::Impl::IsValidDescriptionTag` (CDLParser.cpp:406-421 @ v2.5.2).
fn is_valid_description_tag(current_id: &[u8], parent_id: &[u8]) -> bool {
    let curr_id = c_str(current_id);
    let par_id = c_str(parent_id);

    let is_desc = curr_id == TAG_DESCRIPTION;
    let is_input_viewing_desc =
        curr_id == METADATA_INPUT_DESCRIPTION || curr_id == METADATA_VIEWING_DESCRIPTION;
    let is_sop_sat = par_id == TAG_SOPNODE || par_id == TAG_SATNODE || par_id == TAG_SATNODEALT;

    is_desc || (is_input_viewing_desc && !is_sop_sat)
}

/// Handle the ColorDecisionList element.
///
/// Port of `CDLParser::Impl::HandleColorDecisionListStartElement` (CDLParser.cpp:496-526 @
/// v2.5.2).
fn handle_color_decision_list_start_element(state: &mut State, name: &[u8]) -> bool {
    if name != CDL_TAG_COLOR_DECISION_LIST {
        return false;
    }
    let elt: ElementRcPtr = if state
        .parsing_info
        .as_ref()
        .is_none_or(|info| info.borrow().transforms.is_empty())
    {
        let elt = CdlReaderColorDecisionListElt::new(
            name,
            state.get_xml_location(),
            state.get_xml_filename(),
        );
        // Bind the reader's CDLTransformList to the one in the ColorDecisionList element.
        state.parsing_info = Some(elt.get_cdl_parsing_info().clone());
        Rc::new(RefCell::new(elt))
    } else {
        state.create_dummy_element(name, b": The ColorDecisionList already exists")
    };
    state.elms.push_back(elt);
    true
}

/// Handle the ColorDecision element, if parsing a CDL.
///
/// Port of `CDLParser::Impl::HandleColorDecisionStartElement` (CDLParser.cpp:528-549 @
/// v2.5.2).
fn handle_color_decision_start_element(state: &mut State, name: &[u8]) -> bool {
    if name != CDL_TAG_COLOR_DECISION {
        return false;
    }
    let elt: ElementRcPtr = if state.is_back_element_instance_of::<CdlReaderColorDecisionListElt>()
    {
        Rc::new(RefCell::new(CdlReaderColorDecisionElt::new(
            name,
            state.back_container(),
            state.get_xml_location(),
            state.get_xml_filename(),
        )))
    } else {
        state.create_dummy_element(name, b": ColorDecision must be under a ColorDecisionList")
    };
    state.elms.push_back(elt);
    true
}

/// Handle the ColorCorrectionCollection element.
///
/// Port of `CDLParser::Impl::HandleColorCorrectionCollectionStartElement`
/// (CDLParser.cpp:551-580 @ v2.5.2).
fn handle_color_correction_collection_start_element(state: &mut State, name: &[u8]) -> bool {
    if name != CDL_TAG_COLOR_CORRECTION_COLLECTION {
        return false;
    }
    let elt: ElementRcPtr = if state
        .parsing_info
        .as_ref()
        .is_none_or(|info| info.borrow().transforms.is_empty())
    {
        let elt = CdlReaderColorCorrectionCollectionElt::new(
            name,
            state.get_xml_location(),
            state.get_xml_filename(),
        );
        // Bind the reader's CDLTransformList to the one in the ColorCorrectionCollection
        // element.
        state.parsing_info = Some(elt.get_cdl_parsing_info().clone());
        Rc::new(RefCell::new(elt))
    } else {
        state.create_dummy_element(name, b": The ColorCorrectionCollection already exists")
    };
    state.elms.push_back(elt);
    true
}

/// The error message of a ColorCorrection out of place.
const COLOR_CORRECTION_MISPLACED: &[u8] = b": ColorCorrection must be under a ColorDecision \
(CDL), ColorCorrectionCollection (CCC), or must be the root element (CC)";

/// Handle the ColorCorrection element in the CDL schema.
///
/// Port of `CDLParser::Impl::HandleColorCorrectionCDLStartElement` (CDLParser.cpp:582-619 @
/// v2.5.2).
fn handle_color_correction_cdl_start_element(state: &mut State, name: &[u8]) -> bool {
    if name != CDL_TAG_COLOR_CORRECTION {
        return false;
    }
    // If parsing a CDL, make sure the ColorCorrection is under a ColorDecision element.
    let elt: ElementRcPtr = if state.is_back_element_instance_of::<CdlReaderColorDecisionElt>() {
        let parent = state.back_container();
        let mut cc_elt = CdlReaderColorCorrectionElt::new(
            name,
            Some(parent.clone()),
            state.get_xml_location(),
            state.get_xml_filename(),
        );

        // Bind the ColorCorrection element's CDLTransformList to the one in the
        // ColorDecisionList element.
        let cd_elt = parent.borrow();
        let cd_elt = cd_elt
            .as_any()
            .downcast_ref::<CdlReaderColorDecisionElt>()
            .expect("checked above");
        let cdl_elt = cd_elt.get_parent().borrow();
        let cdl_elt = cdl_elt
            .as_any()
            .downcast_ref::<CdlReaderColorDecisionListElt>()
            .expect("a ColorDecision is made only under a ColorDecisionList");
        cc_elt.set_cdl_parsing_info(cdl_elt.get_cdl_parsing_info());
        Rc::new(RefCell::new(cc_elt))
    } else {
        state.create_dummy_element(name, COLOR_CORRECTION_MISPLACED)
    };
    state.elms.push_back(elt);
    true
}

/// Handle the ColorCorrection element in the CCC schema.
///
/// Port of `CDLParser::Impl::HandleColorCorrectionCCCStartElement` (CDLParser.cpp:621-655 @
/// v2.5.2).
fn handle_color_correction_ccc_start_element(state: &mut State, name: &[u8]) -> bool {
    if name != CDL_TAG_COLOR_CORRECTION {
        return false;
    }
    // If parsing a CCC, make sure the ColorCorrection is under a ColorCorrectionCollection
    // element.
    let elt: ElementRcPtr =
        if state.is_back_element_instance_of::<CdlReaderColorCorrectionCollectionElt>() {
            let parent = state.back_container();
            let mut cc_elt = CdlReaderColorCorrectionElt::new(
                name,
                Some(parent.clone()),
                state.get_xml_location(),
                state.get_xml_filename(),
            );

            // Bind the ColorCorrection element's CDLTransformList to the one in the
            // ColorCorrectionCollection element.
            let ccc_elt = parent.borrow();
            let ccc_elt = ccc_elt
                .as_any()
                .downcast_ref::<CdlReaderColorCorrectionCollectionElt>()
                .expect("checked above");
            cc_elt.set_cdl_parsing_info(ccc_elt.get_cdl_parsing_info());
            Rc::new(RefCell::new(cc_elt))
        } else {
            state.create_dummy_element(name, COLOR_CORRECTION_MISPLACED)
        };
    state.elms.push_back(elt);
    true
}

/// Handle the ColorCorrection element in the CC schema.
///
/// Port of `CDLParser::Impl::HandleColorCorrectionCCStartElement` (CDLParser.cpp:657-691 @
/// v2.5.2).
fn handle_color_correction_cc_start_element(state: &mut State, name: &[u8]) -> bool {
    if name != CDL_TAG_COLOR_CORRECTION {
        return false;
    }
    // If parsing a CC, make sure it is the only CDLTransform.
    let elt: ElementRcPtr = if state
        .parsing_info
        .as_ref()
        .is_none_or(|info| info.borrow().transforms.is_empty())
    {
        // The root has no parent: upstream's `createElement` passes a null one.
        let mut cc_elt = CdlReaderColorCorrectionElt::new(
            name,
            state.create_parent(),
            state.get_xml_location(),
            state.get_xml_filename(),
        );

        // Bind the ColorCorrection element's CDLTransformList to the one explicitly created
        // by the reader.
        let parsing_info = state
            .parsing_info
            .clone()
            .expect("a CC parse makes its parsing info first");
        cc_elt.set_cdl_parsing_info(&parsing_info);
        Rc::new(RefCell::new(cc_elt))
    } else {
        state.create_dummy_element(name, COLOR_CORRECTION_MISPLACED)
    };
    state.elms.push_back(elt);
    true
}

/// Handle the start of a SOPNode element.
///
/// Port of `CDLParser::Impl::HandleSOPNodeStartElement` (CDLParser.cpp:693-714 @ v2.5.2).
fn handle_sop_node_start_element(state: &mut State, name: &[u8]) -> bool {
    if name != TAG_SOPNODE {
        return false;
    }
    let elt: ElementRcPtr = if state.is_back_element_instance_of::<CdlReaderColorCorrectionElt>() {
        Rc::new(RefCell::new(CdlReaderSopNodeCcElt::new(
            name,
            state.back_container(),
            state.get_xml_location(),
            state.get_xml_filename(),
        )))
    } else {
        state.create_dummy_element(name, b": SOPNode must be under a ColorCorrection")
    };
    state.elms.push_back(elt);
    true
}

/// Handle the start of a SatNode element.
///
/// Port of `CDLParser::Impl::HandleSatNodeStartElement` (CDLParser.cpp:716-738 @ v2.5.2).
fn handle_sat_node_start_element(state: &mut State, name: &[u8]) -> bool {
    if name != TAG_SATNODE && name != TAG_SATNODEALT {
        return false;
    }
    let elt: ElementRcPtr = if state.is_back_element_instance_of::<CdlReaderColorCorrectionElt>() {
        Rc::new(RefCell::new(CdlReaderSatNodeCcElt::new(
            name,
            state.back_container(),
            state.get_xml_location(),
            state.get_xml_filename(),
        )))
    } else {
        state.create_dummy_element(name, b": SatNode must be under a ColorCorrection")
    };
    state.elms.push_back(elt);
    true
}

/// Handle the start of a terminal/leaf element: Description, InputDescription,
/// ViewingDescription, Slope, Offset, Power and Saturation.
///
/// Port of `CDLParser::Impl::HandleTerminalStartElement` (CDLParser.cpp:740-807 @ v2.5.2).
fn handle_terminal_start_element(state: &mut State, name: &[u8]) -> bool {
    let container = state
        .get_back_element()
        .filter(|back| back.borrow().is_container());
    let Some(container) = container else {
        let dummy = state.create_dummy_element(name, b"Internal error");
        state.elms.push_back(dummy);
        return true;
    };
    let container_id = container.borrow().get_identifier().to_vec();

    // Handle Description, InputDescription and ViewingDescription elements at their
    // appropriate parent container.
    if is_valid_description_tag(name, &container_id) {
        let desc_elt = XmlReaderDescriptionElt::new(
            name,
            container,
            state.get_xml_location(),
            state.get_xml_filename(),
        );
        state.elms.push_back(Rc::new(RefCell::new(desc_elt)));
        return true;
    }
    // Handle Slope, Offset and Power elements.
    else if name == TAG_SLOPE || name == TAG_OFFSET || name == TAG_POWER {
        let elt: ElementRcPtr = if state.is_back_element_instance_of::<CdlReaderSopNodeCcElt>() {
            Rc::new(RefCell::new(XmlReaderSopValueElt::new(
                name,
                container,
                state.get_xml_location(),
                state.get_xml_filename(),
            )))
        } else {
            state.create_dummy_element(name, b": Slope, Offset or Power tags must be under SOPNode")
        };
        state.elms.push_back(elt);
        return true;
    }
    // Handle Saturation element.
    else if name == TAG_SATURATION {
        let elt: ElementRcPtr = if state.is_back_element_instance_of::<CdlReaderSatNodeCcElt>() {
            Rc::new(RefCell::new(XmlReaderSaturationElt::new(
                name,
                container,
                state.get_xml_location(),
                state.get_xml_filename(),
            )))
        } else {
            state.create_dummy_element(name, b": Saturation tags must be under SatNode")
        };
        state.elms.push_back(elt);
        return true;
    }
    false
}

/// Default handler for the start of an unknown element.
///
/// Port of `CDLParser::Impl::HandleUnknownStartElement` (CDLParser.cpp:809-815 @ v2.5.2).
fn handle_unknown_start_element(state: &mut State, name: &[u8]) -> bool {
    let dummy = state.create_dummy_element(name, b": Unknown element");
    state.elms.push_back(dummy);
    true
}

/// Start the parsing of one element in `schema`.
///
/// Port of `CDLParser::Impl::StartElementHandlerCDL`, `StartElementHandlerCCC` and
/// `StartElementHandlerCC` (CDLParser.cpp:423-478 @ v2.5.2). `IsValidStartElement`
/// (CDLParser.cpp:480-494) checks a null or empty name, which expat never gives.
fn start_element_handler(
    state: &StateRc,
    schema: Schema,
    name: &[u8],
    atts: &[&[u8]],
) -> Result<()> {
    let back = {
        let mut state = state.borrow_mut();
        let state = &mut *state;
        let handled = match schema {
            Schema::Cdl => {
                handle_color_decision_list_start_element(state, name)
                    || handle_color_decision_start_element(state, name)
                    || handle_color_correction_cdl_start_element(state, name)
                    || handle_sop_node_start_element(state, name)
                    || handle_sat_node_start_element(state, name)
                    || handle_terminal_start_element(state, name)
                    || handle_unknown_start_element(state, name)
            }
            Schema::Ccc => {
                handle_color_correction_collection_start_element(state, name)
                    || handle_color_correction_ccc_start_element(state, name)
                    || handle_sop_node_start_element(state, name)
                    || handle_sat_node_start_element(state, name)
                    || handle_terminal_start_element(state, name)
                    || handle_unknown_start_element(state, name)
            }
            Schema::Cc => {
                handle_color_correction_cc_start_element(state, name)
                    || handle_sop_node_start_element(state, name)
                    || handle_sat_node_start_element(state, name)
                    || handle_terminal_start_element(state, name)
                    || handle_unknown_start_element(state, name)
            }
        };
        if !handled {
            return Ok(());
        }
        state.elms.back().expect("an element was pushed")
    };
    back.borrow_mut().start(atts)
}

/// End the parsing of one element.
///
/// Port of `CDLParser::Impl::EndElementHandler` (CDLParser.cpp:817-879 @ v2.5.2).
fn end_element_handler(state: &StateRc, name: &[u8]) -> Result<()> {
    let elt = {
        let mut state = state.borrow_mut();

        // Is the expected element present?
        let Some(elt) = state.get_back_element() else {
            return Err(state.throw_message(b"Missing element"));
        };

        // Is it the expected element?
        if elt.borrow().get_name() != name {
            let mut os = OStringStream::new(Crt::NATIVE);
            os.put_str("Unexpected element (");
            os.put_c_str(name);
            os.put_str("). ");
            os.put_str("Expecting (");
            os.put_bytes(elt.borrow().get_name());
            os.put_str("). ");
            return Err(state.throw_message(os.str()));
        }

        state.elms.pop_back();

        let (is_container, is_dummy) = {
            let e = elt.borrow();
            (e.is_container(), e.is_dummy())
        };
        if !is_container && !is_dummy {
            // Is it a plain element?
            let plain_parent = elt
                .borrow_mut()
                .as_plain_mut()
                .map(|plain| plain.get_parent().clone());
            let Some(plain_parent) = plain_parent else {
                let mut os = OStringStream::new(Crt::NATIVE);
                os.put_str("Unexpected attribute (");
                os.put_c_str(name);
                os.put_str(")");
                return Err(state.throw_message(os.str()));
            };

            let parent = state.get_back_element();

            // Is it at the right location in the stack?
            let right = parent.is_some_and(|parent| {
                parent.borrow().is_container() && Rc::ptr_eq(&parent, &plain_parent)
            });
            if !right {
                let mut os = OStringStream::new(Crt::NATIVE);
                os.put_str("Parsing error (");
                os.put_c_str(name);
                os.put_str(")");
                return Err(state.throw_message(os.str()));
            }
        }
        elt
    };

    elt.borrow_mut().end()
}

/// Handle of strings within an element.
///
/// Port of `CDLParser::Impl::CharacterDataHandler` (CDLParser.cpp:881-954 @ v2.5.2).
fn character_data_handler(state: &StateRc, s: &[u8]) -> Result<()> {
    let len = s.len();
    if len == 0 {
        return Ok(());
    }
    let elt = {
        let state = state.borrow();
        if s[0] == 0 {
            return Err(state.throw_message(b"Empty attribute data"));
        }
        // Parsing a single new line. This is valid.
        if len == 1 && s[0] == b'\n' {
            return Ok(());
        }

        if state.elms.empty() {
            return Err(state.throw_message(b"Unexpected character data before root element"));
        }

        state.elms.back().expect("not empty")
    };
    let location = state.borrow().get_xml_location();

    let is_description = elt.borrow().as_any().is::<XmlReaderDescriptionElt>();
    if is_description {
        // For description we keep the all the text.
        let mut elt = elt.borrow_mut();
        let desc_elt = elt
            .as_plain_mut()
            .expect("a description is a plain element");
        return desc_elt.set_raw_data(s, location);
    }

    // Ignore white-spaces.
    let (start, end) = find_sub_string(s, len);

    if end > 0 {
        let illegal = || {
            let mut os = OStringStream::new(Crt::NATIVE);
            os.put_str("Illegal attribute (");
            os.put_bytes(s);
            os.put_str(")");
            state.borrow().throw_message(os.str())
        };
        if elt.borrow().is_container() {
            return Err(illegal());
        }
        let mut elt = elt.borrow_mut();
        let Some(plain_elt) = elt.as_plain_mut() else {
            return Err(illegal());
        };
        plain_elt.set_raw_data(&s[start..end], location)?;
    }
    Ok(())
}

/// Find root element tag at beginning of the file.
///
/// Port of `FindRootElement` (CDLParser.cpp:286-291 @ v2.5.2): `strstr` on the header.
fn find_root_element(header: &[u8], tag: &[u8]) -> bool {
    let header = c_str(header);
    let pattern = [b"<".as_slice(), tag].concat();
    header.windows(pattern.len()).any(|w| w == pattern)
}

/// The parser of a CDL, CCC or CC file.
///
/// Port of `CDLParser` (CDLParser.h:19-48, CDLParser.cpp:956-1019 @ v2.5.2) and of its
/// `Impl`.
pub struct CdlParser {
    state: StateRc,
    /// `m_parser`.
    parser: Parser<'static, Exception>,
}

impl std::fmt::Debug for CdlParser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CdlParser")
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

impl CdlParser {
    /// A parser of the file `xml_file`, which names it in messages.
    ///
    /// Port of `CDLParser::CDLParser` and `CDLParser::Impl::Impl` (CDLParser.cpp:162-169,
    /// 956-959 @ v2.5.2).
    pub fn new(xml_file: &[u8]) -> CdlParser {
        CdlParser {
            state: Rc::new(RefCell::new(State {
                elms: XmlReaderElementStack::new(),
                parsing_info: None,
                line_number: 0,
                file_name: xml_file.to_vec(),
                is_cc: false,
                is_ccc: false,
            })),
            parser: Parser::new(),
        }
    }

    /// Reads up to 5 KiB of lines of `istream`, each followed by a space, then rewinds it.
    ///
    /// Port of `CDLParser::Impl::loadHeader` (CDLParser.cpp:177-195 @ v2.5.2).
    fn load_header(istream: &mut InputStream) -> Vec<u8> {
        const LIMIT: u32 = 5 * 1024; // 5 kilobytes.
        let mut line = vec![0u8; LIMIT as usize + 1];

        let mut header = Vec::new();
        let mut size_processed: u32 = 0;
        while istream.good() && (size_processed < LIMIT) {
            istream.getline_buf(&mut line, LIMIT as usize);
            let line = c_str(&line);
            header.extend_from_slice(line);
            header.push(b' ');
            size_processed = size_processed.wrapping_add(line.len() as u32);
        }

        istream.clear();
        istream.seekg_begin();

        header
    }

    /// Use the buffer string to detect the input schema and initialize the related element
    /// handlers.
    ///
    /// Port of `CDLParser::Impl::initializeHandlers` (CDLParser.cpp:293-325 @ v2.5.2).
    fn initialize_handlers(&mut self, buffer: &[u8]) -> Result<()> {
        let schema = if find_root_element(buffer, CDL_TAG_COLOR_DECISION_LIST) {
            Schema::Cdl
        } else if find_root_element(buffer, CDL_TAG_COLOR_CORRECTION_COLLECTION) {
            self.state.borrow_mut().is_ccc = true;
            Schema::Ccc
        } else if find_root_element(buffer, CDL_TAG_COLOR_CORRECTION) {
            let mut state = self.state.borrow_mut();
            state.is_cc = true;
            // If parsing a CC, initialize the TransformList explicitly.
            state.parsing_info = Some(Rc::new(RefCell::new(CdlParsingInfo::default())));
            Schema::Cc
        } else {
            // The character data handler is set first upstream; no data reaches it.
            return Err(self.state.borrow().throw_message(b"Missing CDL tag"));
        };

        let state = self.state.clone();
        self.parser
            .set_character_data_handler(Some(Box::new(move |s| character_data_handler(&state, s))));
        let start_state = self.state.clone();
        let end_state = self.state.clone();
        self.parser.set_element_handler(
            Some(Box::new(move |name, atts| {
                start_element_handler(&start_state, schema, name, atts)
            })),
            Some(Box::new(move |name| end_element_handler(&end_state, name))),
        );
        Ok(())
    }

    /// Parse a line.
    ///
    /// Port of `CDLParser::Impl::parse(const std::string &, bool)` (CDLParser.cpp:241-279 @
    /// v2.5.2).
    fn parse_line(&mut self, buffer: &[u8], last_line: bool) -> Result<()> {
        let status = match self.parser.parse(buffer, last_line) {
            Ok(status) => status,
            Err(ParseError::Handler(e)) => return Err(e),
        };
        if status == XmlStatus::Error {
            let expat_error_code = self.parser.error_code();
            let state = self.state.borrow();
            if expat_error_code == XmlError::TagMismatch {
                if let Some(back) = state.elms.back() {
                    // It could be an Op or an Attribute.
                    let mut error = b"XML parsing error (no closing tag for '".to_vec();
                    error.extend_from_slice(c_str(back.borrow().get_name()));
                    error.extend_from_slice(b"'). ");
                    return Err(state.throw_message(&error));
                } else {
                    // Completely lost, something went wrong, but nothing detected with the
                    // stack.
                    return Err(
                        state.throw_message(b"XML parsing error (unbalanced element tags). ")
                    );
                }
            } else {
                let mut error = b"XML parsing error: ".to_vec();
                error.extend_from_slice(
                    xml_error_string(self.parser.error_code())
                        .unwrap_or("")
                        .as_bytes(),
                );
                return Err(state.throw_message(&error));
            }
        }
        Ok(())
    }

    /// Validate if the parsed file or buffer was successful.
    ///
    /// Port of `CDLParser::Impl::validateParsing` (CDLParser.cpp:327-348 @ v2.5.2). Upstream
    /// dereferences a null parsing info when the CDL or CCC root element never started (U-75);
    /// the port refuses the file. Its check of null transforms finds none.
    fn validate_parsing(&self) -> Result<()> {
        let state = self.state.borrow();
        if let Some(back) = state.elms.back() {
            let mut error = b"CDL parsing error (no closing tag for '".to_vec();
            error.extend_from_slice(c_str(back.borrow().get_name()));
            error.extend_from_slice(b")");
            return Err(state.throw_message(&error));
        }

        if state.parsing_info.is_none() {
            return Err(missing_root_error(&state));
        }
        Ok(())
    }

    /// Parse a CDL stream.
    ///
    /// Port of `CDLParser::parse` and `CDLParser::Impl::parse(std::istream &)`
    /// (CDLParser.cpp:197-216, 967-970 @ v2.5.2).
    pub fn parse(&mut self, istream: &mut InputStream) -> Result<()> {
        self.state.borrow_mut().reset();

        let header = CdlParser::load_header(istream);
        self.initialize_handlers(&header)?;

        let mut line = Vec::new();
        self.state.borrow_mut().line_number = 0;
        while istream.good() {
            istream.getline(&mut line);
            line.push(b'\n');
            self.state.borrow_mut().line_number += 1;

            self.parse_line(&line, !istream.good())?;
        }

        self.validate_parsing()
    }

    /// The parse's transforms, in the file's order, and by id, with the root's metadata.
    ///
    /// Port of `CDLParser::getCDLTransforms` (CDLParser.cpp:972-999 @ v2.5.2).
    pub fn get_cdl_transforms(
        &self,
        transform_map: &mut CdlTransformMap,
        transform_vec: &mut CdlTransformVec,
        metadata: &mut FormatMetadataImpl,
    ) -> Result<()> {
        let state = self.state.borrow();
        let Some(parsing_info) = &state.parsing_info else {
            return Err(missing_root_error(&state));
        };
        let parsing_info = parsing_info.borrow();
        for transform in &parsing_info.transforms {
            transform_vec.push(transform.clone());

            let id = transform.id();
            if !id.is_empty() {
                if transform_map.contains_key(id) {
                    let mut os = OStringStream::new(Crt::NATIVE);
                    os.put_str("Error loading ccc xml. ");
                    os.put_str("Duplicate elements with '");
                    os.put_bytes(id);
                    os.put_str("' found. ");
                    os.put_str("If id is specified, it must be unique.");
                    return Err(Exception::new(os.into_bytes()));
                }

                transform_map.insert(id.to_vec(), transform.clone());
            }
        }
        *metadata = parsing_info.metadata.clone();
        Ok(())
    }

    /// The parse's first transform.
    ///
    /// Port of `CDLParser::getCDLTransform` (CDLParser.cpp:1001-1009 @ v2.5.2).
    pub fn get_cdl_transform(&self) -> Result<CdlTransformRcPtr> {
        let state = self.state.borrow();
        let Some(parsing_info) = &state.parsing_info else {
            return Err(missing_root_error(&state));
        };
        let parsing_info = parsing_info.borrow();
        match parsing_info.transforms.first() {
            Some(transform) => Ok(transform.clone()),
            None => Err(Exception::new("No transform found.")),
        }
    }

    /// Port of `CDLParser::isCC` (CDLParser.cpp:1011-1014 @ v2.5.2).
    pub fn is_cc(&self) -> bool {
        self.state.borrow().is_cc
    }

    /// Port of `CDLParser::isCCC` (CDLParser.cpp:1016-1019 @ v2.5.2).
    pub fn is_ccc(&self) -> bool {
        self.state.borrow().is_ccc
    }
}

/// The port's error for a CDL or CCC whose header names the root element but whose root
/// element never started, where upstream dereferences a null pointer (U-75).
fn missing_root_error(state: &State) -> Exception {
    let root: &[u8] = if state.is_ccc {
        CDL_TAG_COLOR_CORRECTION_COLLECTION
    } else {
        CDL_TAG_COLOR_DECISION_LIST
    };
    let mut error = b"CDL parsing error: the root element '".to_vec();
    error.extend_from_slice(root);
    error.extend_from_slice(b"' is missing");
    state.throw_message(&error)
}

#[cfg(test)]
#[path = "cdl_parser_tests.rs"]
mod tests;
