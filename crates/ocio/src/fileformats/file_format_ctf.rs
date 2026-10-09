// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Academy/ASC Common LUT Format (CLF) and the Autodesk Color Transform Format (CTF):
//! their reader. A port of `src/OpenColorIO/fileformats/FileFormatCTF.cpp` @ v2.5.2, as far
//! as WP 4.5b: the format infos, the check that a file is a CTF/CLF, and the parser
//! (`XMLParserHelper`: the line loop, expat's callbacks, the elements by tag, parent and
//! version, the errors). The operators' readers (4.5c-4.5i), `buildFileOps` (4.5h), the
//! baker (WP 4.8) and the writer (WP 4.6) come with their chunks.
//!
//! Upstream's `XMLParserHelper` is expat's user data, and its static handlers reach it
//! through a pointer. Here its state is shared (`Rc<RefCell<State>>`) between the helper and
//! the closures that are expat's handlers; a handler's exception is its `Err`, which ends
//! `XML_Parse` and is returned as is, as upstream's C++ exceptions propagate.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use ocio_formats::expat::xmlparse::{ParseError, Parser, XmlError, XmlStatus, xml_error_string};
use ocio_formats::fileformats::xmlutils::xml_reader_helper::{
    ElementRcPtr, XmlReaderDescriptionElt, XmlReaderDummyElt, XmlReaderElementStack,
};
use ocio_formats::fileformats::xmlutils::xml_reader_utils::{
    TAG_DESCRIPTION, TAG_OFFSET, TAG_POWER, TAG_SATNODE, TAG_SATNODEALT, TAG_SATURATION, TAG_SLOPE,
    TAG_SOPNODE, find_sub_string,
};
use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::format_metadata::{METADATA_INPUT_DESCRIPTOR, METADATA_OUTPUT_DESCRIPTOR};
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::TransformDirection;
use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;
use ocio_ops::utils::pystring::os_path;
use ocio_ops::utils::string_utils::{c_str, lower};

use crate::config::Config;
use crate::context::Context;
use crate::fileformats::ctf::ctf_reader_helper::{
    CtfReaderDescriptorElt, CtfReaderKind, CtfReaderMetadataElt, CtfReaderOpType,
    CtfReaderTransformElt, CtfReaderTransformRc, Descriptor, equal_ignoring_case, get_reader,
};
use crate::fileformats::ctf::ctf_reader_utils::{
    TAG_ACES, TAG_ACES_PARAMS, TAG_ARRAY, TAG_CDL, TAG_CURVE_CTRL_PNTS, TAG_CURVE_SLOPES,
    TAG_DYNAMIC_PARAMETER, TAG_EC_PARAMS, TAG_EXPONENT, TAG_EXPONENT_PARAMS, TAG_EXPOSURE_CONTRAST,
    TAG_FIXED_FUNCTION, TAG_FUNCTION, TAG_GAMMA, TAG_GAMMA_PARAMS, TAG_HUE_CURVE,
    TAG_HUE_CURVE_HUE_FX, TAG_HUE_CURVE_HUE_HUE, TAG_HUE_CURVE_HUE_LUM, TAG_HUE_CURVE_HUE_SAT,
    TAG_HUE_CURVE_LUM_LUM, TAG_HUE_CURVE_LUM_SAT, TAG_HUE_CURVE_SAT_LUM, TAG_HUE_CURVE_SAT_SAT,
    TAG_INDEX_MAP, TAG_INFO, TAG_INVLUT1D, TAG_INVLUT3D, TAG_LOG, TAG_LOG_PARAMS, TAG_LUT1D,
    TAG_LUT3D, TAG_MATRIX, TAG_MAX_IN_VALUE, TAG_MAX_OUT_VALUE, TAG_MIN_IN_VALUE,
    TAG_MIN_OUT_VALUE, TAG_PRIMARY, TAG_PRIMARY_BRIGHTNESS, TAG_PRIMARY_CLAMP,
    TAG_PRIMARY_CONTRAST, TAG_PRIMARY_EXPOSURE, TAG_PRIMARY_GAIN, TAG_PRIMARY_GAMMA,
    TAG_PRIMARY_LIFT, TAG_PRIMARY_OFFSET, TAG_PRIMARY_PIVOT, TAG_PRIMARY_SATURATION,
    TAG_PROCESS_LIST, TAG_RANGE, TAG_REFERENCE, TAG_RGB_CURVE, TAG_RGB_CURVE_BLUE,
    TAG_RGB_CURVE_GREEN, TAG_RGB_CURVE_MASTER, TAG_RGB_CURVE_RED, TAG_TONE, TAG_TONE_BLACKS,
    TAG_TONE_HIGHLIGHTS, TAG_TONE_MIDTONES, TAG_TONE_SCONTRAST, TAG_TONE_SHADOWS, TAG_TONE_WHITES,
};
use crate::fileformats::ctf::ctf_transform::CtfReaderTransformPtr;
use crate::fileformats::input_stream::InputStream;
use crate::transforms::cdl_transform::{METADATA_INPUT_DESCRIPTION, METADATA_VIEWING_DESCRIPTION};
use crate::transforms::file_format::{
    CachedFile, CachedFileRcPtr, FILEFORMAT_CLF, FILEFORMAT_CTF, FileFormat,
    FormatBakeCapabilities, FormatCapabilities, FormatInfo,
};
use crate::transforms::file_transform::FileTransform;
use crate::transforms::group_transform::GroupTransform;

/// A CTF/CLF file's transform.
///
/// Port of `LocalCachedFile` (FileFormatCTF.cpp:102-113 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct LocalCachedFile {
    /// `m_transform`.
    pub transform: CtfReaderTransformPtr,
    /// `m_filePath`.
    pub file_path: Vec<u8>,
}

impl CachedFile for LocalCachedFile {}

/// The CLF and CTF formats.
///
/// Port of `LocalFileFormat` (FileFormatCTF.cpp:117-145 @ v2.5.2).
#[derive(Debug, Clone, Copy, Default)]
pub struct LocalFileFormat;

impl LocalFileFormat {
    /// Reads a CTF or CLF file: its ProcessList's transform. A file whose first 5 KiB of
    /// lines don't hold `<ProcessList` is not one.
    ///
    /// Port of `LocalFileFormat::read` (FileFormatCTF.cpp:1195-1216 @ v2.5.2).
    pub fn read_file(
        &self,
        istream: &mut InputStream,
        file_path: &[u8],
    ) -> Result<LocalCachedFile> {
        if !is_loadable_ctf(istream) {
            let mut oss = OStringStream::new(Crt::NATIVE);
            oss.put_str("Parsing error: '");
            oss.put_bytes(file_path);
            oss.put_str("' is not a CTF/CLF file.");
            return Err(Exception::new(oss.into_bytes()));
        }

        let mut parser = XmlParserHelper::new(file_path);
        parser.parse(istream)?;

        // Keep transform.
        let transform = parser
            .get_transform()
            .expect("a parse that succeeds has a transform");
        let transform = transform.borrow().clone();
        Ok(LocalCachedFile {
            transform: Arc::new(transform),
            file_path: file_path.to_vec(),
        })
    }
}

impl FileFormat for LocalFileFormat {
    /// The CLF and CTF infos. Upstream sets the CTF info's bake capabilities on the CLF info
    /// a second time (`info.bake_capabilities = ...` after `info2.capabilities`), so the CTF
    /// info has none (I-180).
    ///
    /// Port of `LocalFileFormat::getFormatInfo` (FileFormatCTF.cpp:147-170 @ v2.5.2).
    fn format_info(&self) -> Vec<FormatInfo> {
        let all_bakes = FormatBakeCapabilities::LUT_3D
            | FormatBakeCapabilities::LUT_1D
            | FormatBakeCapabilities::LUT_1D_3D;
        let capabilities =
            FormatCapabilities::READ | FormatCapabilities::BAKE | FormatCapabilities::WRITE;
        let info = FormatInfo::new(FILEFORMAT_CLF, "clf", capabilities, all_bakes);
        let info2 = FormatInfo::new(
            FILEFORMAT_CTF,
            "ctf",
            capabilities,
            FormatBakeCapabilities::NONE,
        );
        vec![info, info2]
    }

    /// Port of `LocalFileFormat::read` (FileFormatCTF.cpp:1195-1216 @ v2.5.2).
    fn read(
        &self,
        istream: &mut InputStream,
        file_name: &[u8],
        _interp: Interpolation,
    ) -> Result<CachedFileRcPtr> {
        Ok(Arc::new(self.read_file(istream, file_name)?))
    }

    /// The writer comes with WP 4.6.
    fn write(
        &self,
        _config: &Config,
        _context: &Context,
        _group: &GroupTransform,
        format_name: &[u8],
        _ostream: &mut OStringStream,
    ) -> Result<()> {
        Err(Exception::new(
            [
                b"The writer of the '".as_slice(),
                format_name,
                b"' file format is not ported yet.",
            ]
            .concat(),
        ))
    }

    /// The ops of a CTF/CLF come with WP 4.5h.
    fn build_file_ops(
        &self,
        _ops: &mut OpVec,
        _config: &Config,
        _context: &Context,
        _cached_file: &CachedFileRcPtr,
        _file_transform: &FileTransform,
        _dir: TransformDirection,
    ) -> Result<()> {
        Err(Exception::new(
            "The ops of a CTF/CLF file are not ported yet (WP 4.5h).",
        ))
    }
}

/// Whether the first 5 KiB of lines of `istream` hold `<ProcessList`; the stream moves
/// back to where it was (a failed stream doesn't move: `seekg` fails).
///
/// Port of `isLoadableCTF` (FileFormatCTF.cpp:1166-1191 @ v2.5.2). The readers get a fresh
/// stream, at its start, where `tellg` is 0.
fn is_loadable_ctf(istream: &mut InputStream) -> bool {
    const LIMIT: u32 = 5 * 1024; // 5 kilobytes.
    const PATTERN: &[u8] = b"<ProcessList";
    let mut found_pattern = false;
    let mut size_processed: u32 = 0;
    // `char line[limit + 1];` (FileFormatCTF.cpp:1174): `getline` stores up to `limit - 1`
    // bytes and a NUL in it.
    let mut line = vec![0u8; LIMIT as usize + 1];

    // Error: file is not found, or cannot read.
    if istream.good() {
        // Find ProcessList tag at beginning of file.
        while istream.good() && !found_pattern && (size_processed < LIMIT) {
            istream.getline_buf(&mut line, LIMIT as usize);
            let line = c_str(&line);
            if line.windows(PATTERN.len()).any(|w| w == PATTERN) {
                found_pattern = true;
            }
            size_processed = size_processed.wrapping_add(line.len() as u32);
        }
    }

    // Restore pos in stream.
    istream.seekg_begin();
    found_pattern
}

/// The data members of `XMLParserHelper` but the expat parser.
///
/// Port of `XMLParserHelper` (FileFormatCTF.cpp:172-1164 @ v2.5.2).
#[derive(Debug)]
struct State {
    /// `m_lineNumber`.
    line_number: u32,
    /// `m_fileName`.
    file_name: Vec<u8>,
    /// `m_isCLF`.
    is_clf: bool,
    /// `m_elms`: the parsing stack.
    elms: XmlReaderElementStack,
    /// `m_transform`.
    transform: Option<CtfReaderTransformRc>,
}

type StateRc = Rc<RefCell<State>>;

impl State {
    /// The parse's error: "Error parsing CTF/CLF file (<file>). Error is: <error>. At line
    /// (<n>)".
    ///
    /// Port of `XMLParserHelper::throwMessage` (FileFormatCTF.cpp:325-333 @ v2.5.2).
    fn throw_message(&self, error: &[u8]) -> Exception {
        let mut os = OStringStream::new(Crt::NATIVE);
        os.put_str("Error parsing CTF/CLF file (");
        os.put_c_str(&self.file_name);
        os.put_str("). ");
        os.put_str("Error is: ");
        os.put_c_str(error);
        os.put_str(". At line (");
        os.put_u32(self.line_number);
        os.put_str(")");
        Exception::new(os.into_bytes())
    }

    /// Port of `XMLParserHelper::getXmLineNumber` (FileFormatCTF.cpp:1142-1145 @ v2.5.2).
    fn get_xm_line_number(&self) -> u32 {
        self.line_number
    }

    /// Port of `XMLParserHelper::getXmlFilename` (FileFormatCTF.cpp:1147-1150 @ v2.5.2).
    fn get_xml_filename(&self) -> &[u8] {
        &self.file_name
    }

    /// A dummy element for `name` under `parent`, logging `msg`.
    fn dummy(&self, name: &[u8], parent: Option<ElementRcPtr>, msg: Option<&[u8]>) -> ElementRcPtr {
        Rc::new(RefCell::new(XmlReaderDummyElt::new(
            name,
            parent,
            self.get_xm_line_number(),
            self.get_xml_filename(),
            msg,
        )))
    }

    /// Pushes the reader of an operator of `op_type` named `xml_tag`, which must be a child
    /// of the ProcessList (else a dummy is pushed); a version that doesn't support the
    /// operator is an error.
    ///
    /// Port of `XMLParserHelper::AddOpReader` (FileFormatCTF.cpp:283-323 @ v2.5.2). The
    /// operators' readers come with WP 4.5c-4.5i; until then, a supported operator is
    /// refused as not ported yet.
    fn add_op_reader(&mut self, op_type: CtfReaderOpType, xml_tag: &[u8]) -> Result<()> {
        if self.elms.size() != 1 {
            let mut ss = OStringStream::new(Crt::NATIVE);
            ss.put_str("The ");
            ss.put_c_str(xml_tag);
            ss.put_str("'s parent can only be a Transform");

            let dummy = self.dummy(xml_tag, self.elms.back(), Some(ss.str()));
            self.elms.push_back(dummy);
            return Ok(());
        }

        let elt = self.elms.back().expect("one element");
        let (version, clf_version, is_clf) = {
            let elt = elt.borrow();
            let t = elt
                .as_any()
                .downcast_ref::<CtfReaderTransformElt>()
                .expect("the only element under an operator is the ProcessList's");
            (t.get_version(), t.get_clf_version(), t.is_clf())
        };
        let Some(kind) = get_reader(op_type, version, is_clf) else {
            let mut ss = OStringStream::new(Crt::NATIVE);
            if is_clf {
                ss.put_str("CLF file version '");
                ss.put_bytes(clf_version.to_string().as_bytes());
            } else {
                ss.put_str("CTF file version '");
                ss.put_bytes(version.to_string().as_bytes());
            }
            ss.put_str("' does not support operator '");
            ss.put_c_str(xml_tag);
            ss.put_str("'");
            return Err(self.throw_message(ss.str()));
        };

        Err(self.throw_message(&not_ported(kind, xml_tag)))
    }
}

/// The port's error for an operator whose reader isn't ported yet.
fn not_ported(kind: CtfReaderKind, xml_tag: &[u8]) -> Vec<u8> {
    [
        b"The reader of the operator '".as_slice(),
        xml_tag,
        format!("' ({kind:?}) is not ported yet").as_bytes(),
    ]
    .concat()
}

/// Determines if the element name is supported in the current context: `name` is `tag`
/// (ignoring case), which sets `recognized_name`, and the parent is named `parent_name`, or
/// any parent if it is empty.
///
/// Port of `XMLParserHelper::SupportedElement(const char *, ElementRcPtr &, const char *,
/// const char *, bool &)` (FileFormatCTF.cpp:336-359 @ v2.5.2).
fn supported_element(
    name: &[u8],
    parent: &Option<ElementRcPtr>,
    tag: &[u8],
    parent_name: &[u8],
    recognized_name: &mut bool,
) -> bool {
    if !name.is_empty() && !tag.is_empty() && equal_ignoring_case(name, tag) {
        *recognized_name = true;

        if parent_name.is_empty()
            || parent
                .as_ref()
                .is_some_and(|p| equal_ignoring_case(p.borrow().get_name(), parent_name))
        {
            return true;
        }
    }

    false
}

/// [`supported_element`] for any of `tags`.
///
/// Port of `XMLParserHelper::SupportedElement(const char *, ElementRcPtr &, const
/// std::vector<const char *> &, const char *, bool &)` (FileFormatCTF.cpp:361-392 @ v2.5.2).
fn supported_element_tags(
    name: &[u8],
    parent: &Option<ElementRcPtr>,
    tags: &[&[u8]],
    parent_name: &[u8],
    recognized_name: &mut bool,
) -> bool {
    if !name.is_empty() {
        // If found name in the tag list, test the parent name.
        if tags.iter().any(|tag| equal_ignoring_case(name, tag)) {
            *recognized_name = true;

            if parent_name.is_empty()
                || parent
                    .as_ref()
                    .is_some_and(|p| equal_ignoring_case(p.borrow().get_name(), parent_name))
            {
                return true;
            }
        }
    }

    false
}

/// [`supported_element`] under a parent named any of `parent_names`: a parent is required
/// for the name to be recognized.
///
/// Port of `XMLParserHelper::SupportedElement(const char *, ElementRcPtr &, const char *,
/// const std::vector<const char *> &, bool &)` (FileFormatCTF.cpp:394-419 @ v2.5.2).
fn supported_element_parents(
    name: &[u8],
    parent: &Option<ElementRcPtr>,
    tag: &[u8],
    parent_names: &[&[u8]],
    recognized_name: &mut bool,
) -> bool {
    if let Some(parent) = parent
        && !name.is_empty()
        && !tag.is_empty()
        && equal_ignoring_case(name, tag)
    {
        *recognized_name = true;

        let parent = parent.borrow();
        return parent_names
            .iter()
            .any(|parent_name| equal_ignoring_case(parent.get_name(), parent_name));
    }

    false
}

/// The sub-elements of a Range.
const RANGE_SUB_ELEMENTS: [&[u8]; 4] = [
    TAG_MIN_IN_VALUE,
    TAG_MAX_IN_VALUE,
    TAG_MIN_OUT_VALUE,
    TAG_MAX_OUT_VALUE,
];

/// The sub-elements of a SOPNode.
const SOP_SUB_ELEMENTS: [&[u8]; 3] = [TAG_SLOPE, TAG_OFFSET, TAG_POWER];

/// The sub-elements of a GradingPrimary.
const GRADING_PRIMARY_SUB_ELEMENTS: [&[u8]; 10] = [
    TAG_PRIMARY_BRIGHTNESS,
    TAG_PRIMARY_CLAMP,
    TAG_PRIMARY_CONTRAST,
    TAG_PRIMARY_EXPOSURE,
    TAG_PRIMARY_GAIN,
    TAG_PRIMARY_GAMMA,
    TAG_PRIMARY_LIFT,
    TAG_PRIMARY_OFFSET,
    TAG_PRIMARY_PIVOT,
    TAG_PRIMARY_SATURATION,
];

/// The sub-elements of a GradingTone.
const GRADING_TONE_SUB_ELEMENTS: [&[u8]; 6] = [
    TAG_TONE_BLACKS,
    TAG_TONE_SHADOWS,
    TAG_TONE_MIDTONES,
    TAG_TONE_HIGHLIGHTS,
    TAG_TONE_WHITES,
    TAG_TONE_SCONTRAST,
];

/// The sub-elements of a GradingRGBCurve.
const GRADING_RGB_CURVE_SUB_ELEMENTS: [&[u8]; 4] = [
    TAG_RGB_CURVE_BLUE,
    TAG_RGB_CURVE_GREEN,
    TAG_RGB_CURVE_MASTER,
    TAG_RGB_CURVE_RED,
];

/// The sub-elements of a GradingHueCurve.
const GRADING_HUE_CURVE_SUB_ELEMENTS: [&[u8]; 8] = [
    TAG_HUE_CURVE_HUE_HUE,
    TAG_HUE_CURVE_HUE_SAT,
    TAG_HUE_CURVE_HUE_LUM,
    TAG_HUE_CURVE_LUM_SAT,
    TAG_HUE_CURVE_SAT_SAT,
    TAG_HUE_CURVE_LUM_LUM,
    TAG_HUE_CURVE_SAT_LUM,
    TAG_HUE_CURVE_HUE_FX,
];

/// The elements of operators whose readers come with WP 4.5c-4.5i (upstream's class
/// `class`): their parent is the reader of an operator, which this chunk never makes, so
/// they are refused as not ported.
fn operator_part_not_ported(state: &State, class: &str) -> Exception {
    state.throw_message(format!("The CTF reader's {class} is not ported yet").as_bytes())
}

/// Whether `elt` is an operator's reader (`dynamic_pointer_cast<CTFReaderOpElt>`): none is
/// ported yet (WP 4.5c-4.5i).
fn is_op_elt(_elt: &ElementRcPtr) -> bool {
    false
}

/// Start the parsing of one element.
///
/// Port of `XMLParserHelper::StartElementHandler` (FileFormatCTF.cpp:422-991 @ v2.5.2).
/// expat never gives a null or empty name, which upstream checks.
fn start_element_handler(state: &StateRc, name: &[u8], atts: &[&[u8]]) -> Result<()> {
    let back = {
        let mut st = state.borrow_mut();
        let st = &mut *st;
        push_element(st, name)?;
        st.elms.back().expect("an element was pushed")
    };
    back.borrow_mut().start(atts)
}

/// The element `name` starts: pushes its reader.
///
/// Port of the body of `XMLParserHelper::StartElementHandler` (FileFormatCTF.cpp:479-988 @
/// v2.5.2), but its last line, the element's `start`.
fn push_element(st: &mut State, name: &[u8]) -> Result<()> {
    if let Some(back) = st.elms.back() {
        // Check if we are still processing a metadata structure.
        if back.borrow().as_any().is::<CtfReaderMetadataElt>() {
            let elt = CtfReaderMetadataElt::new(name, back.clone(), st.line_number, &st.file_name)?;
            st.elms.push_back(Rc::new(RefCell::new(elt)));
            return Ok(());
        }
    }

    // Handle the ProcessList element or its children (the ops).
    if equal_ignoring_case(name, TAG_PROCESS_LIST) {
        if st.transform.is_some() {
            let front = st
                .elms
                .front()
                .filter(|front| front.borrow().as_any().is::<CtfReaderTransformElt>());
            let dummy = st.dummy(name, front, Some(b"The Transform already exists"));
            st.elms.push_back(dummy);
        } else {
            let elt = CtfReaderTransformElt::new(
                name,
                st.get_xm_line_number(),
                st.get_xml_filename(),
                st.is_clf,
            );
            st.transform = Some(elt.get_transform().clone());
            st.elms.push_back(Rc::new(RefCell::new(elt)));
        }
        return Ok(());
    }

    // Handle all Ops.
    let p_elt = st.elms.back();

    // Will allow to give better error feedback to the user if the element name is not
    // handled. If any case recognizes the name, but the element is not in the correct
    // context (under the correct parent), then the recognizedName boolean will be true.
    let mut recognized_name = false;
    let r = &mut recognized_name;

    // For each possible element name, test against a tag name and a current parent name to
    // determine if the element should be handled.
    let op_type = if supported_element(name, &p_elt, TAG_ACES, TAG_PROCESS_LIST, r) {
        Some(CtfReaderOpType::Aces)
    } else if supported_element(name, &p_elt, TAG_CDL, TAG_PROCESS_LIST, r) {
        Some(CtfReaderOpType::Cdl)
    } else if supported_element(name, &p_elt, TAG_EXPOSURE_CONTRAST, TAG_PROCESS_LIST, r) {
        Some(CtfReaderOpType::ExposureContrast)
    } else if supported_element(name, &p_elt, TAG_FIXED_FUNCTION, TAG_PROCESS_LIST, r) {
        Some(CtfReaderOpType::FixedFunction)
    } else if supported_element(name, &p_elt, TAG_FUNCTION, TAG_PROCESS_LIST, r) {
        Some(CtfReaderOpType::Function)
    } else if supported_element(name, &p_elt, TAG_GAMMA, TAG_PROCESS_LIST, r)
        || supported_element(name, &p_elt, TAG_EXPONENT, TAG_PROCESS_LIST, r)
    {
        Some(CtfReaderOpType::Gamma)
    } else if supported_element(name, &p_elt, TAG_PRIMARY, TAG_PROCESS_LIST, r) {
        Some(CtfReaderOpType::GradingPrimary)
    } else if supported_element(name, &p_elt, TAG_RGB_CURVE, TAG_PROCESS_LIST, r) {
        Some(CtfReaderOpType::GradingRgbCurve)
    } else if supported_element(name, &p_elt, TAG_HUE_CURVE, TAG_PROCESS_LIST, r) {
        Some(CtfReaderOpType::GradingHueCurve)
    } else if supported_element(name, &p_elt, TAG_TONE, TAG_PROCESS_LIST, r) {
        Some(CtfReaderOpType::GradingTone)
    } else if supported_element(name, &p_elt, TAG_INVLUT1D, TAG_PROCESS_LIST, r) {
        Some(CtfReaderOpType::InvLut1D)
    } else if supported_element(name, &p_elt, TAG_INVLUT3D, TAG_PROCESS_LIST, r) {
        Some(CtfReaderOpType::InvLut3D)
    } else if supported_element(name, &p_elt, TAG_LOG, TAG_PROCESS_LIST, r) {
        Some(CtfReaderOpType::Log)
    } else if supported_element(name, &p_elt, TAG_LUT1D, TAG_PROCESS_LIST, r) {
        Some(CtfReaderOpType::Lut1D)
    } else if supported_element(name, &p_elt, TAG_LUT3D, TAG_PROCESS_LIST, r) {
        Some(CtfReaderOpType::Lut3D)
    } else if supported_element(name, &p_elt, TAG_MATRIX, TAG_PROCESS_LIST, r) {
        Some(CtfReaderOpType::Matrix)
    } else if supported_element(name, &p_elt, TAG_RANGE, TAG_PROCESS_LIST, r) {
        Some(CtfReaderOpType::Range)
    } else if supported_element(name, &p_elt, TAG_REFERENCE, TAG_PROCESS_LIST, r) {
        Some(CtfReaderOpType::Reference)
    } else {
        None
    };
    // TODO: handle other ops from syncolor.
    if let Some(op_type) = op_type {
        return st.add_op_reader(op_type, name);
    }

    // Handle other elements that are transform-level metadata or parts of ops.
    let p_container = p_elt.clone().filter(|p| p.borrow().is_container());
    let Some(p_container) = p_container else {
        let dummy = st.dummy(name, p_elt, None);
        st.elms.push_back(dummy);
        return Ok(());
    };

    let elt: ElementRcPtr = if supported_element(name, &p_elt, TAG_ACES_PARAMS, TAG_ACES, r) {
        return Err(operator_part_not_ported(st, "CTFReaderACESParamsElt"));
    } else if supported_element(name, &p_elt, TAG_ARRAY, TAG_LUT1D, r)
        || supported_element(name, &p_elt, TAG_ARRAY, TAG_INVLUT1D, r)
        || supported_element(name, &p_elt, TAG_ARRAY, TAG_LUT3D, r)
        || supported_element(name, &p_elt, TAG_ARRAY, TAG_INVLUT3D, r)
        || supported_element(name, &p_elt, TAG_ARRAY, TAG_MATRIX, r)
    {
        return Err(operator_part_not_ported(st, "CTFReaderArrayElt"));
    } else if supported_element(name, &p_elt, TAG_DESCRIPTION, b"", r)
        || supported_element(name, &p_elt, METADATA_INPUT_DESCRIPTION, TAG_CDL, r)
        || supported_element(name, &p_elt, METADATA_VIEWING_DESCRIPTION, TAG_CDL, r)
    {
        Rc::new(RefCell::new(XmlReaderDescriptionElt::new(
            name,
            p_container,
            st.get_xm_line_number(),
            st.get_xml_filename(),
        )))
    }
    // Dynamic Property is valid under any operator parent. First test if the tag is
    // supported to set the recognizedName accordingly, without testing for parents. Test for
    // the parent type prior to testing the name.
    else if supported_element(name, &p_elt, TAG_DYNAMIC_PARAMETER, b"", r)
        && is_op_elt(&p_container)
    {
        return Err(operator_part_not_ported(st, "CTFReaderDynamicParamElt"));
    } else if supported_element(name, &p_elt, TAG_EC_PARAMS, TAG_EXPOSURE_CONTRAST, r) {
        return Err(operator_part_not_ported(st, "CTFReaderECParamsElt"));
    } else if supported_element(name, &p_elt, TAG_GAMMA_PARAMS, TAG_GAMMA, r)
        || supported_element(name, &p_elt, TAG_EXPONENT_PARAMS, TAG_EXPONENT, r)
    {
        return Err(operator_part_not_ported(st, "CTFReaderGammaParamsElt"));
    } else if supported_element(name, &p_elt, TAG_INDEX_MAP, TAG_LUT1D, r)
        || supported_element(name, &p_elt, TAG_INDEX_MAP, TAG_LUT3D, r)
    {
        return Err(operator_part_not_ported(st, "CTFReaderIndexMapElt"));
    } else if supported_element(name, &p_elt, TAG_INFO, TAG_PROCESS_LIST, r) {
        Rc::new(RefCell::new(CtfReaderMetadataElt::new_info(
            name,
            p_container,
            st.get_xm_line_number(),
            st.get_xml_filename(),
        )?))
    } else if supported_element(name, &p_elt, METADATA_INPUT_DESCRIPTOR, TAG_PROCESS_LIST, r) {
        Rc::new(RefCell::new(CtfReaderDescriptorElt::new(
            name,
            p_container,
            st.get_xm_line_number(),
            st.get_xml_filename(),
            Descriptor::Input,
        )))
    } else if supported_element(name, &p_elt, TAG_LOG_PARAMS, TAG_LOG, r) {
        return Err(operator_part_not_ported(st, "CTFReaderLogParamsElt"));
    } else if supported_element(
        name,
        &p_elt,
        METADATA_OUTPUT_DESCRIPTOR,
        TAG_PROCESS_LIST,
        r,
    ) {
        Rc::new(RefCell::new(CtfReaderDescriptorElt::new(
            name,
            p_container,
            st.get_xm_line_number(),
            st.get_xml_filename(),
            Descriptor::Output,
        )))
    } else if supported_element_tags(name, &p_elt, &RANGE_SUB_ELEMENTS, TAG_RANGE, r) {
        return Err(operator_part_not_ported(st, "CTFReaderRangeValueElt"));
    } else if supported_element(name, &p_elt, TAG_SATNODE, TAG_CDL, r)
        || supported_element(name, &p_elt, TAG_SATNODEALT, TAG_CDL, r)
    {
        return Err(operator_part_not_ported(st, "CTFReaderSatNodeElt"));
    } else if supported_element(name, &p_elt, TAG_SATURATION, TAG_SATNODE, r) {
        return Err(operator_part_not_ported(st, "XmlReaderSaturationElt"));
    } else if supported_element(name, &p_elt, TAG_SOPNODE, TAG_CDL, r) {
        return Err(operator_part_not_ported(st, "CTFReaderSOPNodeElt"));
    } else if supported_element_tags(name, &p_elt, &SOP_SUB_ELEMENTS, TAG_SOPNODE, r) {
        return Err(operator_part_not_ported(st, "XmlReaderSOPValueElt"));
    } else if supported_element_tags(name, &p_elt, &GRADING_PRIMARY_SUB_ELEMENTS, TAG_PRIMARY, r) {
        return Err(operator_part_not_ported(
            st,
            "CTFReaderGradingPrimaryParamElt",
        ));
    } else if supported_element_tags(
        name,
        &p_elt,
        &GRADING_RGB_CURVE_SUB_ELEMENTS,
        TAG_RGB_CURVE,
        r,
    ) || supported_element_tags(
        name,
        &p_elt,
        &GRADING_HUE_CURVE_SUB_ELEMENTS,
        TAG_HUE_CURVE,
        r,
    ) {
        return Err(operator_part_not_ported(st, "CTFReaderGradingCurveElt"));
    } else if supported_element_parents(
        name,
        &p_elt,
        TAG_CURVE_CTRL_PNTS,
        &GRADING_RGB_CURVE_SUB_ELEMENTS,
        r,
    ) || supported_element_parents(
        name,
        &p_elt,
        TAG_CURVE_CTRL_PNTS,
        &GRADING_HUE_CURVE_SUB_ELEMENTS,
        r,
    ) {
        return Err(operator_part_not_ported(
            st,
            "CTFReaderGradingCurvePointsElt",
        ));
    } else if supported_element_parents(
        name,
        &p_elt,
        TAG_CURVE_SLOPES,
        &GRADING_RGB_CURVE_SUB_ELEMENTS,
        r,
    ) || supported_element_parents(
        name,
        &p_elt,
        TAG_CURVE_SLOPES,
        &GRADING_HUE_CURVE_SUB_ELEMENTS,
        r,
    ) {
        return Err(operator_part_not_ported(
            st,
            "CTFReaderGradingCurveSlopesElt",
        ));
    } else if supported_element_tags(name, &p_elt, &GRADING_TONE_SUB_ELEMENTS, TAG_TONE, r) {
        return Err(operator_part_not_ported(st, "CTFReaderGradingToneParamElt"));
    } else if recognized_name {
        let mut oss = OStringStream::new(Crt::NATIVE);
        oss.put_str("'");
        oss.put_c_str(name);
        oss.put_str("' not allowed in this element");
        st.dummy(name, st.elms.back(), Some(oss.str()))
    } else {
        st.dummy(name, st.elms.back(), Some(b"Unknown element"))
    };
    st.elms.push_back(elt);
    Ok(())
}

/// End the parsing of one element.
///
/// Port of `XMLParserHelper::EndElementHandler` (FileFormatCTF.cpp:994-1058 @ v2.5.2). The
/// stack is never empty here (each start pushed an element); upstream's `back` of an empty
/// one would be undefined.
fn end_element_handler(state: &StateRc, name: &[u8]) -> Result<()> {
    let elt = {
        let mut st = state.borrow_mut();

        // Is the expected element present?
        let Some(elt) = st.elms.back() else {
            return Err(st.throw_message(b"CTF/CLF parsing error: Tag is missing."));
        };

        // Is it the expected element?
        if elt.borrow().get_name() != name {
            let mut ss = OStringStream::new(Crt::NATIVE);
            ss.put_str("CTF/CLF parsing error: Tag '");
            ss.put_c_str(name);
            ss.put_str("' is missing.");
            return Err(st.throw_message(ss.str()));
        }

        let (is_dummy, is_container) = {
            let e = elt.borrow();
            (e.is_dummy(), e.is_container())
        };
        if is_dummy || is_container {
            st.elms.pop_back();
        } else {
            // Is it a plain element?
            let plain_parent = elt
                .borrow_mut()
                .as_plain_mut()
                .map(|plain| plain.get_parent().clone());
            let Some(plain_parent) = plain_parent else {
                let mut ss = OStringStream::new(Crt::NATIVE);
                ss.put_str("CTF/CLF parsing error: Attribute end '");
                ss.put_c_str(name);
                ss.put_str("' is illegal.");
                return Err(st.throw_message(ss.str()));
            };

            st.elms.pop_back();

            let parent = st.elms.back();

            // Is it at the right location in the stack?
            let right = parent.is_some_and(|parent| {
                parent.borrow().is_container() && Rc::ptr_eq(&parent, &plain_parent)
            });
            if !right {
                let mut ss = OStringStream::new(Crt::NATIVE);
                ss.put_str("CTF/CLF parsing error: Tag '");
                ss.put_c_str(name);
                ss.put_str("'.");
                return Err(st.throw_message(ss.str()));
            }
        }
        elt
    };

    elt.borrow_mut().end()
}

/// Handle of strings within an element.
///
/// Port of `XMLParserHelper::CharacterDataHandler` (FileFormatCTF.cpp:1061-1140 @ v2.5.2).
/// expat gives character data only inside the root element, so the stack is never empty
/// here.
fn character_data_handler(state: &StateRc, s: &[u8]) -> Result<()> {
    let len = s.len();
    if len == 0 {
        return Ok(());
    }
    let (elt, location) = {
        let st = state.borrow();
        if s[0] == 0 {
            return Err(st.throw_message(b"CTF/CLF parsing error: attribute illegal. "));
        }
        // Parsing a single new line. This is valid.
        if len == 1 && s[0] == b'\n' {
            return Ok(());
        }

        let Some(elt) = st.elms.back() else {
            let mut oss = OStringStream::new(Crt::NATIVE);
            oss.put_str("CTF/CLF parsing error: missing end tag '");
            oss.put_c_str(s);
            oss.put_str("'.");
            return Err(st.throw_message(oss.str()));
        };
        (elt, st.get_xm_line_number())
    };

    let is_description = elt.borrow().as_any().is::<XmlReaderDescriptionElt>();
    if is_description {
        let mut elt = elt.borrow_mut();
        let desc_elt = elt
            .as_plain_mut()
            .expect("a description is a plain element");
        return desc_elt.set_raw_data(s, location);
    }

    // Strip white spaces.
    let (start, end) = find_sub_string(s, len);

    if end > 0 {
        // CTFReaderMetadataElt is a special element: it is used to process container
        // elements, but it is also used to process the terminal/plain elements.
        let mut e = elt.borrow_mut();
        if let Some(metadata_elt) = e.as_any_mut().downcast_mut::<CtfReaderMetadataElt>() {
            return metadata_elt.set_raw_data(&s[start..end], location);
        }

        let illegal = || {
            let mut oss = OStringStream::new(Crt::NATIVE);
            oss.put_str("CTF/CLF parsing error: attribute illegal '");
            oss.put_c_str(s);
            oss.put_str("'.");
            state.borrow().throw_message(oss.str())
        };
        if e.is_container() {
            return Err(illegal());
        }

        let Some(plain_elt) = e.as_plain_mut() else {
            return Err(illegal());
        };
        plain_elt.set_raw_data(&s[start..end], location)?;
    }
    Ok(())
}

/// The CTF/CLF parser: a file's ProcessList, line by line through expat.
///
/// Port of `XMLParserHelper` (FileFormatCTF.cpp:172-1164 @ v2.5.2).
struct XmlParserHelper {
    state: StateRc,
    /// `m_parser`.
    parser: Parser<'static, Exception>,
}

impl XmlParserHelper {
    /// A parser of the file `file_name`, a CLF if its extension is `.clf` (ignoring case).
    ///
    /// Port of `XMLParserHelper::XMLParserHelper` (FileFormatCTF.cpp:178-189 @ v2.5.2).
    fn new(file_name: &[u8]) -> XmlParserHelper {
        let (_root, extension) = os_path::splitext(file_name);
        let state = Rc::new(RefCell::new(State {
            line_number: 0,
            file_name: file_name.to_vec(),
            is_clf: lower(&extension) == b".clf",
            elms: XmlReaderElementStack::new(),
            transform: None,
        }));
        let mut parser = Parser::new();
        let start_state = state.clone();
        let end_state = state.clone();
        parser.set_element_handler(
            Some(Box::new(move |name, atts| {
                start_element_handler(&start_state, name, atts)
            })),
            Some(Box::new(move |name| end_element_handler(&end_state, name))),
        );
        let data_state = state.clone();
        parser.set_character_data_handler(Some(Box::new(move |s| {
            character_data_handler(&data_state, s)
        })));
        XmlParserHelper { state, parser }
    }

    /// Parses the stream, a line at a time, then checks every element closed and the
    /// transform has an operator.
    ///
    /// Port of `XMLParserHelper::Parse(std::istream &)` (FileFormatCTF.cpp:196-236 @
    /// v2.5.2).
    fn parse(&mut self, istream: &mut InputStream) -> Result<()> {
        let mut line = Vec::new();
        self.state.borrow_mut().line_number = 0;
        while istream.good() {
            istream.getline(&mut line);
            // Add back the newline character. Parsing will copy the line in a buffer up to
            // the length of the line without the null termination. Our code will be called
            // back to parse the buffer into a number. Code uses strtod. The buffer has to be
            // delimited so that strtod does not access it after its length.
            line.push(b'\n');
            self.state.borrow_mut().line_number += 1;

            self.parse_line(&line, !istream.good())?;
        }

        let st = self.state.borrow();
        if let Some(back) = st.elms.back() {
            let mut error = b"CTF/CLF parsing error (no closing tag for '".to_vec();
            error.extend_from_slice(c_str(back.borrow().get_name()));
            error.extend_from_slice(b") ");
            return Err(st.throw_message(&error));
        }

        let Some(transform) = &st.transform else {
            return Err(st.throw_message(b"CTF/CLF parsing error: Invalid transform."));
        };

        if transform.borrow().get_ops().is_empty() {
            return Err(st.throw_message(b"CTF/CLF parsing error: No color operator in file."));
        }
        Ok(())
    }

    /// Parses a line.
    ///
    /// Port of `XMLParserHelper::Parse(const std::string &, bool)` (FileFormatCTF.cpp:238-274
    /// @ v2.5.2).
    fn parse_line(&mut self, buffer: &[u8], last_line: bool) -> Result<()> {
        let status = match self.parser.parse(buffer, last_line) {
            Ok(status) => status,
            Err(ParseError::Handler(e)) => return Err(e),
        };
        if status == XmlStatus::Error {
            let expat_error_code = self.parser.error_code();
            let st = self.state.borrow();
            if expat_error_code == XmlError::TagMismatch {
                if let Some(back) = st.elms.back() {
                    // It could be an Op or an Attribute.
                    let mut error = b"CTF/CLF parsing error (no closing tag for '".to_vec();
                    error.extend_from_slice(c_str(back.borrow().get_name()));
                    error.extend_from_slice(b"').");
                    return Err(st.throw_message(&error));
                } else {
                    // Completely lost, something went wrong, but nothing detected with the
                    // stack.
                    return Err(
                        st.throw_message(b"CTF/CLF parsing error (unbalanced element tags).")
                    );
                }
            } else {
                let mut error = b"CTF/CLF parsing error: ".to_vec();
                error.extend_from_slice(
                    xml_error_string(self.parser.error_code())
                        .unwrap_or("")
                        .as_bytes(),
                );
                return Err(st.throw_message(&error));
            }
        }
        Ok(())
    }

    /// Port of `XMLParserHelper::getTransform` (FileFormatCTF.cpp:276-279 @ v2.5.2).
    fn get_transform(&self) -> Option<CtfReaderTransformRc> {
        self.state.borrow().transform.clone()
    }
}

#[cfg(test)]
#[path = "file_format_ctf_tests.rs"]
mod tests;
