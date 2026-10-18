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
    CdlOpDataRcPtr, ContainerEltRcPtr, XmlReaderComplexEltBase, XmlReaderContainerElt,
    XmlReaderElement, XmlReaderElementBase, XmlReaderPlainElt, XmlReaderPlainEltBase,
    XmlReaderSatNodeBaseElt, XmlReaderSopNodeBase, XmlReaderSopNodeBaseElt,
    sat_node_append_metadata,
};
use ocio_formats::fileformats::xmlutils::xml_reader_utils::{
    ATTR_ID, ATTR_NAME, find_delim, find_next_token_start, get_next_number, get_numbers,
    is_number_delimiter, is_space, parse_number, truncate_string,
};
use ocio_ops::bit_depth_utils::get_bit_depth_max_value;
use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::format_metadata::FormatMetadataImpl;
use ocio_ops::logging::{is_debug_logging_enabled, log_debug, log_warning};
use ocio_ops::math_utils::{convert_half_bits_to_float, sse_cvttps_epi32};
use ocio_ops::op_data::{OpData, OpDataType};
use ocio_ops::open_color_types::{
    BitDepth, GradingStyle, Interpolation, Lut1DHueAdjust, RgbCurveType, TransformDirection,
    bit_depth_to_string,
};
use ocio_ops::ops::cdl::cdl_op_data::{CdlOpData, CdlOpStyle};
use ocio_ops::ops::fixedfunction::fixed_function_op_data::{
    FixedFunctionOpData, FixedFunctionOpStyle,
};
use ocio_ops::ops::gamma::gamma_op_data::{GammaOpData, GammaStyle};
use ocio_ops::ops::gradingrgbcurve::grading_rgb_curve::GradingRgbCurve;
use ocio_ops::ops::gradingrgbcurve::grading_rgb_curve_op_data::GradingRgbCurveOpData;
use ocio_ops::ops::log::log_op_data::{
    LIN_SIDE_OFFSET, LIN_SIDE_SLOPE, LOG_SIDE_OFFSET, LOG_SIDE_SLOPE, LogOpData,
};
use ocio_ops::ops::log::log_utils::{
    CtfParams, CtfParamsType, LogStyle, convert_log_parameters,
    convert_string_to_style as log_convert_string_to_style,
    convert_style_to_string as log_convert_style_to_string, ctf_values, get_log_direction,
};
use ocio_ops::ops::lut1d::lut1d_op_data::Lut1DOpData;
use ocio_ops::ops::lut3d::lut3d_op_data::Lut3DOpData;
use ocio_ops::ops::matrix::matrix_op_data::MatrixOpData;
use ocio_ops::ops::range::range_op_data::RangeOpData;
use ocio_ops::ops::reference::reference_op_data::ReferenceOpData;
use ocio_ops::platform::strcasecmp;
use ocio_ops::utils::cscan::{ScanArg, ScanCrt, sscanf};
use ocio_ops::utils::string_utils::{c_str, lower};
use ocio_ops::{Exception, Result};

use super::ctf_reader_utils::{
    ATTR_ALIAS, ATTR_BASE_PATH, ATTR_HALF_DOMAIN, ATTR_HUE_ADJUST, ATTR_INTERPOLATION,
    ATTR_IS_INVERTED, ATTR_PARAM, ATTR_PATH, ATTR_RAW_HALFS, TAG_DYN_PROP_CONTRAST,
    TAG_DYN_PROP_EXPOSURE, TAG_DYN_PROP_GAMMA, TAG_DYN_PROP_HUECURVE, TAG_DYN_PROP_LOOK,
    TAG_DYN_PROP_PRIMARY, TAG_DYN_PROP_RGBCURVE, TAG_DYN_PROP_TONE, TAG_HUE_CURVE_HUE_FX,
    TAG_HUE_CURVE_HUE_HUE, TAG_HUE_CURVE_HUE_LUM, TAG_HUE_CURVE_HUE_SAT, TAG_HUE_CURVE_LUM_LUM,
    TAG_HUE_CURVE_LUM_SAT, TAG_HUE_CURVE_SAT_LUM, TAG_HUE_CURVE_SAT_SAT, TAG_RGB_CURVE_BLUE,
    TAG_RGB_CURVE_GREEN, TAG_RGB_CURVE_MASTER, TAG_RGB_CURVE_RED,
    convert_string_to_grading_style_and_dir, get_interpolation_1d, get_interpolation_3d,
};
use super::ctf_reader_utils::{
    ATTR_BASE, ATTR_BITDEPTH_IN, ATTR_BITDEPTH_OUT, ATTR_BYPASS, ATTR_BYPASS_LIN_TO_LOG, ATTR_CHAN,
    ATTR_COMP_CLF_VERSION, ATTR_DIMENSION, ATTR_EXPONENT, ATTR_GAMMA, ATTR_HIGHLIGHT,
    ATTR_INVERSE_OF, ATTR_LINEARSLOPE, ATTR_LINSIDEBREAK, ATTR_LINSIDEOFFSET, ATTR_LINSIDESLOPE,
    ATTR_LOGSIDEOFFSET, ATTR_LOGSIDESLOPE, ATTR_OFFSET, ATTR_PARAMS, ATTR_REFBLACK, ATTR_REFWHITE,
    ATTR_SHADOW, ATTR_STYLE, ATTR_VERSION, TAG_MAX_IN_VALUE, TAG_MAX_OUT_VALUE, TAG_MIN_IN_VALUE,
    TAG_MIN_OUT_VALUE, TAG_PROCESS_LIST,
};
use super::ctf_transform::{
    CTF_INFO_ELEMENT_VERSION, CTF_PROCESS_LIST_VERSION, CTF_PROCESS_LIST_VERSION_1_2,
    CTF_PROCESS_LIST_VERSION_1_3, CTF_PROCESS_LIST_VERSION_1_4, CTF_PROCESS_LIST_VERSION_1_5,
    CTF_PROCESS_LIST_VERSION_1_6, CTF_PROCESS_LIST_VERSION_1_7, CTF_PROCESS_LIST_VERSION_1_8,
    CTF_PROCESS_LIST_VERSION_2_0, CTF_PROCESS_LIST_VERSION_2_5, CtfReaderTransform, CtfVersion,
};
use super::index_mapping::IndexMapping;
use crate::lut_limits::{MAX_1D_LUT_LENGTH, MAX_3D_LUT_LENGTH};

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

/// An `unsigned long` (32 bits on Windows, 64 on Linux) passed as an `unsigned int`: its low
/// 32 bits.
// `unsigned long` is already `u32` on Windows.
#[allow(clippy::unnecessary_cast)]
fn narrow(v: c_ulong) -> u32 {
    v as u32
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
    /// `CTFReaderCDLElt::m_cdl`, which the SOPNode and SatNode elements share: the op data of
    /// a CDL reader.
    cdl: Option<CdlOpDataRcPtr>,
    /// `CTFReaderLogElt::m_ctfParams`.
    ctf_params: CtfParams,
    /// `CTFReaderLogElt::m_baseSet`.
    base_set: bool,
    /// The op's id, which `getIdentifier` returns.
    id: Vec<u8>,
    /// `CTFReaderLut1DElt::m_indexMapping`.
    index_mapping: IndexMapping,
    /// `CTFIndexMapMgt::m_completed`.
    index_map_completed: bool,
    /// `CTFReaderGradingRGBCurveElt::m_loadingRGBCurve`: the curves the curve elements fill,
    /// from the style's defaults.
    loading_rgb_curve: Option<GradingRgbCurve>,
}

impl CtfReaderOpElt {
    /// The reader of `kind`, if ported (WP 4.5c-4.5i port the others).
    ///
    /// Port of the constructors of the `CTFReaderOpElt` subclasses: `CTFReaderACESElt`
    /// (CTFReaderHelper.cpp:1199-1203), `CTFReaderCDLElt` (1327-1332),
    /// `CTFReaderFixedFunctionElt` (1417-1421), `CTFReaderFunctionElt` (1496-1500),
    /// `CTFReaderGammaElt` (1862-1865), `CTFReaderGradingRGBCurveElt` (2580-2583),
    /// `CTFReaderReferenceElt` (5014-5018),
    /// `CTFReaderInvLut1DElt`
    /// (3245-3248), `CTFReaderInvLut3DElt` (3433-3436), `CTFReaderLogElt` (3545-3549),
    /// `CTFReaderLut1DElt` (4085-4092), `CTFReaderLut3DElt` (4389-4396),
    /// `CTFReaderMatrixElt` (4561-4566) and `CTFReaderRangeElt` (4858-4862) @ v2.5.2.
    pub fn create(kind: CtfReaderKind) -> Option<CtfReaderOpElt> {
        let op = match kind {
            CtfReaderKind::Matrix | CtfReaderKind::Matrix1_3 => OpData::Matrix(MatrixOpData::new()),
            CtfReaderKind::Range | CtfReaderKind::Range1_7 => OpData::Range(RangeOpData::new()),
            // CDL op is already initialized to identity.
            CtfReaderKind::Cdl => OpData::Cdl(CdlOpData::new_default()),
            CtfReaderKind::Gamma
            | CtfReaderKind::Gamma1_5
            | CtfReaderKind::GammaCtf2_0
            | CtfReaderKind::GammaClf3_0 => OpData::Gamma(GammaOpData::default()),
            CtfReaderKind::Log | CtfReaderKind::Log2_0 => {
                OpData::Log(LogOpData::new(2.0, TransformDirection::Forward))
            }
            CtfReaderKind::Lut1D | CtfReaderKind::Lut1D1_4 | CtfReaderKind::Lut1D1_7 => {
                OpData::Lut1D(Lut1DOpData::new(2).expect("a LUT of 2 entries"))
            }
            CtfReaderKind::InvLut1D => OpData::Lut1D(
                Lut1DOpData::with_direction(2, TransformDirection::Inverse)
                    .expect("a LUT of 2 entries"),
            ),
            CtfReaderKind::Lut3D | CtfReaderKind::Lut3D1_7 => {
                OpData::Lut3D(Lut3DOpData::new(2).expect("a grid of 2"))
            }
            CtfReaderKind::InvLut3D => OpData::Lut3D(
                Lut3DOpData::with_direction(2, TransformDirection::Inverse).expect("a grid of 2"),
            ),
            CtfReaderKind::GradingRgbCurve => {
                OpData::GradingRgbCurve(GradingRgbCurveOpData::new(GradingStyle::Log))
            }
            CtfReaderKind::Reference => OpData::Reference(ReferenceOpData::new()),
            CtfReaderKind::Aces | CtfReaderKind::FixedFunction | CtfReaderKind::Function => {
                OpData::FixedFunction(
                    FixedFunctionOpData::new(FixedFunctionOpStyle::AcesRedMod03Fwd)
                        .expect("a style without parameters"),
                )
            }
            _ => return None,
        };
        let cdl = match &op {
            OpData::Cdl(cdl) => Some(Rc::new(RefCell::new(cdl.clone()))),
            _ => None,
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
            cdl,
            ctf_params: CtfParams::default(),
            base_set: false,
            id: Vec::new(),
            index_mapping: IndexMapping::new(0),
            index_map_completed: false,
            loading_rgb_curve: None,
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
                self.id = c_str(value).to_vec();
                match &self.cdl {
                    Some(cdl) => cdl.borrow_mut().set_id(value),
                    None => self.op.set_id(value),
                }
            } else if equal_ignoring_case(ATTR_NAME, att) {
                match &self.cdl {
                    Some(cdl) => cdl
                        .borrow_mut()
                        .get_format_metadata_mut()
                        .set_name(Some(value)),
                    None => self.op.set_name(value),
                }
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
    /// Port of `CTFReaderOpElt::isOpParameterValid` (CTFReaderHelper.cpp:949-957),
    /// `CTFReaderCDLElt::isOpParameterValid` (1361-1365), `CTFReaderGammaElt::isOpParameterValid`
    /// (1918-1922), `CTFReaderLogElt::isOpParameterValid` (3582-3586) and
    /// `CTFReaderInvLut1DElt::isOpParameterValid` (3322-3329),
    /// `CTFReaderLut1DElt::isOpParameterValid` (4149-4155),
    /// `CTFReaderLut1DElt_1_4::isOpParameterValid` (4344-4351),
    /// `CTFReaderACESElt::isOpParameterValid` (1241-1245),
    /// `CTFReaderFixedFunctionElt::isOpParameterValid` (1475-1480),
    /// `CTFReaderFunctionElt::isOpParameterValid` (1537-1541),
    /// `CTFReaderGradingRGBCurveElt::isOpParameterValid` (2585-2590),
    /// `CTFReaderReferenceElt::isOpParameterValid` (5095-5102),
    /// `CTFReaderInvLut3DElt::isOpParameterValid` (3471-3475),
    /// `CTFReaderLut3DElt::isOpParameterValid` (4431-4435) and
    /// `CTFReaderRangeElt_1_7::isOpParameterValid` (4917-4921) @ v2.5.2.
    fn is_op_parameter_valid(&self, att: &[u8]) -> bool {
        let base = equal_ignoring_case(ATTR_ID, att)
            || equal_ignoring_case(ATTR_NAME, att)
            || equal_ignoring_case(ATTR_BITDEPTH_IN, att)
            || equal_ignoring_case(ATTR_BITDEPTH_OUT, att)
            // Allow bypass attribute in CTF.
            || (equal_ignoring_case(ATTR_BYPASS, att) && !self.get_transform().borrow().is_clf());
        match self.kind {
            CtfReaderKind::Range1_7
            | CtfReaderKind::Cdl
            | CtfReaderKind::Gamma
            | CtfReaderKind::Gamma1_5
            | CtfReaderKind::GammaCtf2_0
            | CtfReaderKind::GammaClf3_0
            | CtfReaderKind::Log
            | CtfReaderKind::Log2_0
            | CtfReaderKind::Aces
            | CtfReaderKind::Function => base || equal_ignoring_case(ATTR_STYLE, att),
            CtfReaderKind::FixedFunction => {
                base || equal_ignoring_case(ATTR_STYLE, att)
                    || equal_ignoring_case(ATTR_PARAMS, att)
            }
            CtfReaderKind::GradingRgbCurve => {
                base || equal_ignoring_case(ATTR_STYLE, att)
                    || equal_ignoring_case(ATTR_BYPASS_LIN_TO_LOG, att)
            }
            CtfReaderKind::Reference => {
                base || equal_ignoring_case(ATTR_PATH, att)
                    || equal_ignoring_case(ATTR_BASE_PATH, att)
                    || equal_ignoring_case(ATTR_ALIAS, att)
                    || equal_ignoring_case(ATTR_IS_INVERTED, att)
            }
            CtfReaderKind::Lut1D => {
                base || equal_ignoring_case(ATTR_INTERPOLATION, att)
                    || equal_ignoring_case(ATTR_HALF_DOMAIN, att)
                    || equal_ignoring_case(ATTR_RAW_HALFS, att)
            }
            CtfReaderKind::Lut1D1_4 | CtfReaderKind::Lut1D1_7 | CtfReaderKind::InvLut1D => {
                base || equal_ignoring_case(ATTR_INTERPOLATION, att)
                    || equal_ignoring_case(ATTR_HALF_DOMAIN, att)
                    || equal_ignoring_case(ATTR_RAW_HALFS, att)
                    || equal_ignoring_case(ATTR_HUE_ADJUST, att)
            }
            CtfReaderKind::Lut3D | CtfReaderKind::Lut3D1_7 | CtfReaderKind::InvLut3D => {
                base || equal_ignoring_case(ATTR_INTERPOLATION, att)
            }
            _ => base,
        }
    }

    /// Whether the reader holds an array (`dynamic_cast<CTFArrayMgt *>`).
    pub fn is_array_mgt(&self) -> bool {
        matches!(
            self.kind,
            CtfReaderKind::Matrix
                | CtfReaderKind::Matrix1_3
                | CtfReaderKind::Lut1D
                | CtfReaderKind::Lut1D1_4
                | CtfReaderKind::Lut1D1_7
                | CtfReaderKind::InvLut1D
                | CtfReaderKind::Lut3D
                | CtfReaderKind::Lut3D1_7
                | CtfReaderKind::InvLut3D
        )
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
    /// Port of `CTFReaderInvLut1DElt::updateDimension` (CTFReaderHelper.cpp:3353-3376),
    /// `CTFReaderLut1DElt::updateDimension` (4176-4199), `CTFReaderInvLut3DElt::updateDimension`
    /// (3496-3519), `CTFReaderLut3DElt::updateDimension` (4453-4477),
    /// `CTFReaderMatrixElt::updateDimension`
    /// (4590-4616) and `CTFReaderMatrixElt_1_3::updateDimension` (4684-4743) @ v2.5.2.
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
            CtfReaderKind::Lut1D
            | CtfReaderKind::Lut1D1_4
            | CtfReaderKind::Lut1D1_7
            | CtfReaderKind::InvLut1D => {
                if dims.len() != 2 {
                    return false;
                }

                let max = if dims.is_empty() { 0 } else { dims.len() - 1 };
                let num_color_components = dims[max];

                if dims[1] != 3 && dims[1] != 1 {
                    return false;
                }

                if dims[0] < 2 || c_ulong::from(dims[0]) > MAX_1D_LUT_LENGTH {
                    return false;
                }

                self.lut1d_mut()
                    .get_array_mut()
                    .resize(c_ulong::from(dims[0]), c_ulong::from(num_color_components))
                    .expect("a length the reader allows");
                true
            }
            CtfReaderKind::Lut3D | CtfReaderKind::Lut3D1_7 | CtfReaderKind::InvLut3D => {
                if dims.len() != 4 {
                    return false;
                }

                let max = if dims.is_empty() { 0 } else { dims.len() - 1 };
                let num_color_components = dims[max];

                if dims[3] != 3 || dims[1] != dims[0] || dims[2] != dims[0] {
                    return false;
                }

                if dims[0] < 2 || c_ulong::from(dims[0]) > MAX_3D_LUT_LENGTH {
                    return false;
                }

                self.lut3d_mut()
                    .get_array_mut()
                    .resize(c_ulong::from(dims[0]), c_ulong::from(num_color_components))
                    .expect("a length the reader allows");
                true
            }
            _ => false,
        }
    }

    /// The number of values of the array `update_dimension` sized: `ArrayBase::getNumValues`.
    pub fn array_num_values(&self) -> c_ulong {
        match &self.op {
            OpData::Lut1D(lut) => lut.get_array().get_num_values(),
            OpData::Lut3D(lut) => lut.get_array().get_num_values(),
            _ => self.matrix().get_array().get_num_values(),
        }
    }

    /// The array's length: `ArrayBase::getLength`.
    pub fn array_length(&self) -> c_ulong {
        match &self.op {
            OpData::Lut1D(lut) => lut.get_array().get_length(),
            OpData::Lut3D(lut) => lut.get_array().get_length(),
            _ => self.matrix().get_array().get_length(),
        }
    }

    /// The array's number of color components: `ArrayBase::getNumColorComponents`.
    pub fn array_num_color_components(&self) -> c_ulong {
        match &self.op {
            OpData::Lut1D(lut) => lut.get_array().get_num_color_components(),
            OpData::Lut3D(lut) => lut.get_array().get_num_color_components(),
            _ => self.matrix().get_array().get_num_color_components(),
        }
    }

    /// Sets the array's value at `index`: `ArrayBase::setDoubleValue`, a `float` for a LUT
    /// (OpArray.h:73-76 @ v2.5.2).
    pub fn array_set_double_value(&mut self, index: c_ulong, value: f64) {
        match &mut self.op {
            OpData::Lut1D(lut) => {
                lut.get_array_mut().get_values_mut()[index as usize] = value as f32
            }
            OpData::Lut3D(lut) => {
                lut.get_array_mut().get_values_mut()[index as usize] = value as f32
            }
            _ => self
                .matrix_mut()
                .get_array_mut()
                .set_double_value(index, value),
        }
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
            CtfReaderKind::Lut1D
            | CtfReaderKind::Lut1D1_4
            | CtfReaderKind::Lut1D1_7
            | CtfReaderKind::InvLut1D => self.lut1d_end_array(position),
            CtfReaderKind::Lut3D | CtfReaderKind::Lut3D1_7 | CtfReaderKind::InvLut3D => {
                self.lut3d_end_array(position)
            }
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
    fn commit_op(&mut self) {
        if let Some(cdl) = &self.cdl {
            self.op = OpData::Cdl(cdl.borrow().clone());
        }
        let mut transform = self.get_transform().borrow_mut();
        transform.get_ops_mut()[self.op_index] = Arc::new(self.op.clone());
    }
}

impl CtfReaderOpElt {
    /// The CDL of a CDL reader, which its SOPNode and SatNode elements fill.
    ///
    /// Port of `CTFReaderCDLElt::getCDL` (CTFReaderHelper.cpp:1380-1383 @ v2.5.2).
    pub fn get_cdl(&self) -> &CdlOpDataRcPtr {
        self.cdl.as_ref().expect("a CDL reader")
    }

    /// The style (required by CLF v3, which defaults it to ASC; unknown styles are errors).
    ///
    /// Port of `CTFReaderCDLElt::start` (CTFReaderHelper.cpp:1338-1359 @ v2.5.2), after
    /// `CTFReaderOpElt::start`.
    fn cdl_start(&mut self, atts: &[&[u8]]) -> Result<()> {
        let mut is_style_found = false;
        let mut i = 0;
        while i < atts.len() {
            if equal_ignoring_case(ATTR_STYLE, atts[i]) {
                // Unrecognized CDL styles will throw an exception.
                let style = CdlOpData::get_style_from_name(Some(utf8(atts[i + 1])))?;
                self.get_cdl().borrow_mut().set_style(style);
                is_style_found = true;
            }
            i += 2;
        }

        // Although the OCIO default for CDL style is no-clamp, the default specified in the
        // CLF v3 spec is ASC.
        if !is_style_found {
            self.get_cdl().borrow_mut().set_style(CdlOpStyle::V1_2Fwd);
        }
        Ok(())
    }

    /// The gamma of a Gamma reader.
    ///
    /// Port of `CTFReaderGammaElt::getGamma` (CTFReaderHelper.h:532 @ v2.5.2).
    pub fn gamma_mut(&mut self) -> &mut GammaOpData {
        match &mut self.op {
            OpData::Gamma(gamma) => gamma,
            _ => panic!("not a Gamma reader"),
        }
    }

    /// Whether this is a Gamma reader (`dynamic_cast<CTFReaderGammaElt *>`).
    pub fn is_gamma(&self) -> bool {
        matches!(
            self.kind,
            CtfReaderKind::Gamma
                | CtfReaderKind::Gamma1_5
                | CtfReaderKind::GammaCtf2_0
                | CtfReaderKind::GammaClf3_0
        )
    }

    /// Whether the reader's version handles the style: the basic and moncurve ones before
    /// CTF 2.0, all from CTF 2.0 and CLF 3.0.
    ///
    /// Port of `CTFReaderGammaElt::isValid` (CTFReaderHelper.cpp:1953-1971) and
    /// `CTFReaderGammaElt_CTF_2_0::isValid` (1984-2001) @ v2.5.2.
    fn gamma_is_valid(&self, style: GammaStyle) -> bool {
        match self.kind {
            CtfReaderKind::GammaCtf2_0 | CtfReaderKind::GammaClf3_0 => true,
            _ => matches!(
                style,
                GammaStyle::BasicFwd
                    | GammaStyle::BasicRev
                    | GammaStyle::MoncurveFwd
                    | GammaStyle::MoncurveRev
            ),
        }
    }

    /// Whether the GammaParams elements of this reader take the alpha channel: the reader
    /// `createGammaParamsElt` makes is `CTFReaderGammaParamsElt_1_5` for the CTF 1.5 and 2.0
    /// readers, `CTFReaderGammaParamsElt` for the others.
    ///
    /// Port of `CTFReaderGammaElt::createGammaParamsElt` (CTFReaderHelper.cpp:1944-1951),
    /// `CTFReaderGammaElt_1_5::createGammaParamsElt` (1973-1982) and
    /// `CTFReaderGammaElt_CLF_3_0::createGammaParamsElt` (2003-2012) @ v2.5.2.
    pub fn gamma_params_allow_alpha(&self) -> bool {
        matches!(
            self.kind,
            CtfReaderKind::Gamma1_5 | CtfReaderKind::GammaCtf2_0
        )
    }

    /// The style (required, and handled by the reader's version), with its identity
    /// parameters for all channels.
    ///
    /// Port of `CTFReaderGammaElt::start` (CTFReaderHelper.cpp:1871-1916 @ v2.5.2), after
    /// `CTFReaderOpElt::start`.
    fn gamma_start(&mut self, atts: &[&[u8]]) -> Result<()> {
        let mut is_style_found = false;

        let mut i = 0;
        while i < atts.len() {
            if equal_ignoring_case(ATTR_STYLE, atts[i]) {
                // We need a valid style to parse the parameters. This will throw on
                // unrecognized styles.
                let style = GammaOpData::convert_string_to_style(Some(utf8(atts[i + 1])))?;
                if !self.gamma_is_valid(style) {
                    let mut oss = OStringStream::new(Crt::NATIVE);
                    oss.put_str("Style not handled: '");
                    oss.put_c_str(atts[i + 1]);
                    oss.put_str("' for ");

                    let transform = self.get_transform().borrow();
                    if transform.is_clf() {
                        oss.put_str("CLF file version '");
                        oss.put_bytes(transform.get_clf_version().to_string().as_bytes());
                    } else {
                        oss.put_str("CTF file version '");
                        oss.put_bytes(transform.get_ctf_version().to_string().as_bytes());
                    }
                    oss.put_str("'.");

                    return Err(self.throw_message(oss.str()));
                }
                let gamma = self.gamma_mut();
                gamma.set_style(style);
                is_style_found = true;

                // Set default parameters for all channels.
                let params = GammaOpData::identity_parameters(gamma.style());
                gamma.set_params(&params);
            }

            i += 2;
        }
        if !is_style_found {
            return Err(self.throw_message(b"Missing parameter 'style'."));
        }
        Ok(())
    }

    /// The parameters' check.
    ///
    /// Port of `CTFReaderGammaElt::end` (CTFReaderHelper.cpp:1924-1937 @ v2.5.2), after
    /// `CTFReaderOpElt::end`.
    fn gamma_end(&mut self) -> Result<()> {
        // Validate the end result.
        if let Err(ce) = self.gamma_mut().validate() {
            return Err(self.throw_m(&[b"Invalid parameters: ", c_str(ce.what()), b"."]));
        }
        Ok(())
    }

    /// The log of a Log reader.
    ///
    /// Port of `CTFReaderLogElt::getLog` (CTFReaderHelper.h:897 @ v2.5.2).
    fn log_mut(&mut self) -> &mut LogOpData {
        match &mut self.op {
            OpData::Log(log) => log,
            _ => panic!("not a Log reader"),
        }
    }

    /// Whether this is a Log reader (`dynamic_cast<CTFReaderLogElt *>`).
    pub fn is_log(&self) -> bool {
        matches!(self.kind, CtfReaderKind::Log | CtfReaderKind::Log2_0)
    }

    /// Whether the LogParams elements of this reader are CTF 2.0's: the reader
    /// `createLogParamsElt` makes is `CTFReaderLogParamsElt_2_0` for `CTFReaderLogElt_2_0`.
    ///
    /// Port of `CTFReaderLogElt::createLogParamsElt` (CTFReaderHelper.cpp:3655-3662) and
    /// `CTFReaderLogElt_2_0::createLogParamsElt` (3719-3726) @ v2.5.2.
    pub fn log_params_are_2_0(&self) -> bool {
        self.kind == CtfReaderKind::Log2_0
    }

    /// The parameters as the file gives them.
    ///
    /// Port of `CTFReaderLogElt::getCTFParams` (CTFReaderHelper.h:899-900 @ v2.5.2).
    pub fn get_ctf_params(&self) -> &CtfParams {
        &self.ctf_params
    }

    /// Port of `CTFReaderLogElt::getCTFParams` (CTFReaderHelper.h:899-900 @ v2.5.2).
    pub fn get_ctf_params_mut(&mut self) -> &mut CtfParams {
        &mut self.ctf_params
    }

    /// Port of `CTFReaderLogElt::isBaseSet` (CTFReaderHelper.h:903 @ v2.5.2).
    fn is_base_set(&self) -> bool {
        self.base_set
    }

    /// Sets the log's base, which all the channels must share.
    ///
    /// Port of `CTFReaderLogElt::setBase` (CTFReaderHelper.cpp:3629-3645 @ v2.5.2).
    pub fn set_base(&mut self, base: f64) -> Result<()> {
        if self.base_set {
            let cur_base = self.log_mut().base();
            if cur_base != base {
                let mut oss = OStringStream::new(Crt::NATIVE);
                oss.put_str("Log base has to be the same on all components: ");
                oss.put_str("Current base: ");
                oss.put_f64(cur_base);
                oss.put_str(", new base: ");
                oss.put_f64(base);
                oss.put_str(".");
                return Err(self.throw_message(oss.str()));
            }
        } else {
            self.base_set = true;
            self.log_mut().set_base(base);
        }
        Ok(())
    }

    /// The style (required).
    ///
    /// Port of `CTFReaderLogElt::start` (CTFReaderHelper.cpp:3555-3580 @ v2.5.2), after
    /// `CTFReaderOpElt::start`.
    fn log_start(&mut self, atts: &[&[u8]]) -> Result<()> {
        let mut is_style_found = false;
        let mut i = 0;
        while i < atts.len() {
            if equal_ignoring_case(ATTR_STYLE, atts[i]) {
                match log_convert_string_to_style(Some(utf8(atts[i + 1]))) {
                    Ok(style) => self.ctf_params.style = style,
                    Err(_) => {
                        return Err(self.throw_m(&[
                            b"Required attribute 'style' '",
                            c_str(atts[i + 1]),
                            b"' is invalid.",
                        ]));
                    }
                }
                is_style_found = true;
            }
            i += 2;
        }

        if !is_style_found {
            return Err(
                self.throw_message(b"CTF/CLF Log parsing. Required attribute 'style' is missing.")
            );
        }
        Ok(())
    }

    /// The log of a CTF before 2.0: its base, direction and parameters from the style and
    /// the legacy parameters, then its check.
    ///
    /// Port of `CTFReaderLogElt::end` (CTFReaderHelper.cpp:3588-3622 @ v2.5.2), after
    /// `CTFReaderOpElt::end`.
    fn log_end(&mut self) -> Result<()> {
        // When style is log2, log10 (or anti), there are no params.
        let mut base = 2.0;
        let mut r_params = Vec::new();
        let mut g_params = Vec::new();
        let mut b_params = Vec::new();
        let dir = get_log_direction(self.ctf_params.style);

        // This handles all log styles.
        if let Err(ce) = convert_log_parameters(
            &self.ctf_params,
            &mut base,
            &mut r_params,
            &mut g_params,
            &mut b_params,
        ) {
            return Err(self.throw_m(&[b"Parameters are not valid: '", c_str(ce.what()), b"'."]));
        }

        let log = self.log_mut();
        log.set_base(base);
        log.set_direction(dir);
        log.set_red_params(r_params);
        log.set_green_params(g_params);
        log.set_blue_params(b_params);

        self.log_validate()
    }

    /// The log's check.
    fn log_validate(&mut self) -> Result<()> {
        // Validate the end result.
        if let Err(ce) = self.log_mut().validate() {
            return Err(self.throw_m(&[b"Log is not valid: '", c_str(ce.what()), b"'."]));
        }
        Ok(())
    }

    /// The log of CTF 2.0 and CLF 3.0: its direction from the style; the legacy parameters
    /// converted, or the base of the plain logarithms if no LogParams set it; then its check.
    ///
    /// Port of `CTFReaderLogElt_2_0::end` (CTFReaderHelper.cpp:3664-3717 @ v2.5.2), after
    /// `CTFReaderOpElt::end`.
    fn log_end_2_0(&mut self) -> Result<()> {
        let dir = get_log_direction(self.ctf_params.style);
        self.log_mut().set_direction(dir);

        if self.ctf_params.get_type() == CtfParamsType::Cineon {
            let mut base = 2.0;
            let mut r_params = Vec::new();
            let mut g_params = Vec::new();
            let mut b_params = Vec::new();

            // This handles all log styles.
            if let Err(ce) = convert_log_parameters(
                &self.ctf_params,
                &mut base,
                &mut r_params,
                &mut g_params,
                &mut b_params,
            ) {
                return Err(self.throw_m(&[
                    b"Parameters are not valid: '",
                    c_str(ce.what()),
                    b"'.",
                ]));
            }

            let log = self.log_mut();
            log.set_base(base);
            log.set_red_params(r_params);
            log.set_green_params(g_params);
            log.set_blue_params(b_params);
        } else if !self.is_base_set() {
            let style = self.ctf_params.style;
            if style == LogStyle::Log2 || style == LogStyle::AntiLog2 {
                self.set_base(2.0)?;
            } else if style == LogStyle::Log10 || style == LogStyle::AntiLog10 {
                self.set_base(10.0)?;
            }
        }

        self.log_validate()
    }
}

/// An attribute's value, which expat gives in UTF-8, as the `&str` the style parsers take.
fn utf8(value: &[u8]) -> &str {
    std::str::from_utf8(value).expect("expat gives UTF-8")
}

impl CtfReaderOpElt {
    /// The LUT of a Lut1D or InverseLUT1D reader.
    fn lut1d_mut(&mut self) -> &mut Lut1DOpData {
        match &mut self.op {
            OpData::Lut1D(lut) => lut,
            _ => panic!("not a Lut1D reader"),
        }
    }

    /// The LUT of a Lut1D or InverseLUT1D reader.
    fn lut1d(&self) -> &Lut1DOpData {
        match &self.op {
            OpData::Lut1D(lut) => lut,
            _ => panic!("not a Lut1D reader"),
        }
    }

    /// Whether this is a Lut1D reader of any version.
    fn is_lut1d(&self) -> bool {
        matches!(
            self.kind,
            CtfReaderKind::Lut1D | CtfReaderKind::Lut1D1_4 | CtfReaderKind::Lut1D1_7
        )
    }

    /// Whether the reader holds an IndexMap (`dynamic_cast<CTFIndexMapMgt *>`).
    pub fn is_index_map_mgt(&self) -> bool {
        self.is_lut1d() || matches!(self.kind, CtfReaderKind::Lut3D | CtfReaderKind::Lut3D1_7)
    }

    /// Port of `CTFIndexMapMgt::isCompletedIM` (CTFReaderHelper.h:290-293 @ v2.5.2).
    pub fn is_completed_im(&self) -> bool {
        self.index_map_completed
    }

    /// Sizes the IndexMap for the dimensions `dims`: whether the reader takes them (upstream
    /// returns the map, or null).
    ///
    /// Port of `CTFReaderLut1DElt::updateDimensionIM` (CTFReaderHelper.cpp:4248-4264) and
    /// `CTFReaderLut3DElt::updateDimensionIM` (4494-4510) @ v2.5.2.
    pub fn update_dimension_im(&mut self, dims: &[u32]) -> bool {
        if dims.len() != 1 {
            return false;
        }

        let num_components = dims[0];

        let max_length = if self.is_lut1d() {
            MAX_1D_LUT_LENGTH
        } else {
            MAX_3D_LUT_LENGTH
        };
        if dims[0] < 2 || c_ulong::from(dims[0]) > max_length {
            return false;
        }

        self.index_mapping.resize(num_components as usize);
        true
    }

    /// The IndexMap's dimension: `IndexMapping::getDimension`.
    pub fn index_map_dimension(&self) -> usize {
        self.index_mapping.get_dimension()
    }

    /// Sets the IndexMap's pair at `index`: `IndexMapping::setPair`.
    pub fn index_map_set_pair(&mut self, index: usize, first: f32, second: f32) -> Result<()> {
        self.index_mapping.set_pair(index, first, second)
    }

    /// Finalizes the IndexMap, `position` pairs read.
    ///
    /// Port of `CTFReaderLut1DElt::endIndexMap` (CTFReaderHelper.cpp:4266-4276) and
    /// `CTFReaderLut3DElt::endIndexMap` (4512-4522) @ v2.5.2.
    pub fn end_index_map(&mut self, position: u32) -> Result<()> {
        if self.index_mapping.get_dimension() != position as usize {
            return Err(self.throw_m(&[
                b"Expected ",
                self.index_mapping.get_dimension().to_string().as_bytes(),
                b" IndexMap values, found ",
                position.to_string().as_bytes(),
                b".",
            ]));
        }

        self.index_mapping.validate()?;
        self.index_map_completed = true;
        Ok(())
    }

    /// The interpolation (optional), half domain, raw halfs and, from CTF 1.4, hue adjust of
    /// a Lut1D or an InverseLUT1D.
    ///
    /// Port of `CTFReaderLut1DElt::start` (CTFReaderHelper.cpp:4098-4147),
    /// `CTFReaderLut1DElt_1_4::start` (4280-4342) and `CTFReaderInvLut1DElt::start`
    /// (3254-3320) @ v2.5.2, after `CTFReaderOpElt::start`.
    fn lut1d_start(&mut self, atts: &[&[u8]]) -> Result<()> {
        let inverse = self.kind == CtfReaderKind::InvLut1D;
        let hue_adjust = self.kind != CtfReaderKind::Lut1D;

        // The interpolation attribute is optional in CLF/CTF. The INTERP_DEFAULT enum
        // indicates that the value was not specified in the file. When writing, this means no
        // interpolation attribute will be added.
        self.lut1d_mut().set_interpolation(Interpolation::Default);

        // The errors of an illegal value, the reader's own and the inverse's.
        let illegal = |this: &Self, att: &[u8], value: &[u8]| {
            if inverse {
                this.throw_m(&[
                    b"Unknown ",
                    att,
                    b" value: '",
                    value,
                    b"' while parsing InvLut1D.",
                ])
            } else {
                this.throw_m(&[
                    b"Illegal '",
                    att,
                    b"' attribute '",
                    value,
                    b"' while parsing Lut1D.",
                ])
            }
        };

        let mut i = 0;
        while i < atts.len() {
            let (att, value) = (atts[i], atts[i + 1]);
            if equal_ignoring_case(ATTR_INTERPOLATION, att) {
                match get_interpolation_1d(Some(value)) {
                    Ok(interp) => self.lut1d_mut().set_interpolation(interp),
                    Err(e) => return Err(self.throw_message(e.what())),
                }
            }

            if equal_ignoring_case(ATTR_HALF_DOMAIN, att) {
                if !equal_ignoring_case(b"true", value) {
                    return Err(illegal(self, b"halfDomain", c_str(value)));
                }

                self.lut1d_mut().set_input_half_domain(true);
            }

            if equal_ignoring_case(ATTR_RAW_HALFS, att) {
                if !equal_ignoring_case(b"true", value) {
                    return Err(illegal(self, b"rawHalfs", c_str(value)));
                }

                self.lut1d_mut().set_output_raw_halfs(true);
            }

            // This was added in v1.4.
            if hue_adjust && equal_ignoring_case(ATTR_HUE_ADJUST, att) {
                if !equal_ignoring_case(b"dw3", value) {
                    return Err(illegal(self, b"hueAdjust", c_str(value)));
                }

                self.lut1d_mut().set_hue_adjust(Lut1DHueAdjust::Dw3)?;
            }

            i += 2;
        }
        Ok(())
    }

    /// The array scaled from the file's output bit depth (the input one for an inverse),
    /// that bit depth recorded, the LUT validated; for CTF 1.7 and later, a 2-entry IndexMap
    /// becomes a Range inserted before the LUT.
    ///
    /// Port of `CTFReaderLut1DElt::end` (CTFReaderHelper.cpp:4157-4169),
    /// `CTFReaderLut1DElt_1_7::end` (4355-4385) and `CTFReaderInvLut1DElt::end` (3331-3346)
    /// @ v2.5.2, after `CTFReaderOpElt::end`.
    fn lut1d_end(&mut self) -> Result<()> {
        // Array values for inverse LUTs are scaled based on the inBitDepth (rather than
        // outBD), normalize them.
        let bit_depth = if self.kind == CtfReaderKind::InvLut1D {
            self.in_bit_depth
        } else {
            self.out_bit_depth
        };
        let scale = 1.0f32 / get_bit_depth_max_value(bit_depth)? as f32;
        let lut = self.lut1d_mut();
        lut.scale(scale);

        // Record the original array scaling present in the file. This is used by a heuristic
        // involved with LUT inversion. The bit-depth of ops is typically changed after the
        // file is read, hence the need to store it now.
        lut.set_file_output_bit_depth(bit_depth);
        lut.validate()
    }

    /// The Range a 2-entry IndexMap makes, inserted before the LUT in the transform's ops.
    ///
    /// Port of the IndexMap part of `CTFReaderLut1DElt_1_7::end` (CTFReaderHelper.cpp:
    /// 4365-4384) and `CTFReaderLut3DElt_1_7::end` (4537-4556) @ v2.5.2.
    fn insert_index_map_range(&mut self) -> Result<()> {
        // The LUT renderers do not currently support an indexMap, however for compliance
        // with the CLF spec we want to support the case of a single 2-entry indexMap by
        // converting it into a RangeOp which we insert before the LUT.
        if self.is_completed_im() {
            // This will throw if the indexMap is not length 2.
            let p_rng = range_from_index_mapping(
                &self.index_mapping,
                narrow(self.array_length()),
                self.in_bit_depth,
            )?;

            // Insert the range before the LUT that was appended to m_transform in
            // OpElt::start. This code assumes that the current LUT is at the end of the
            // opList. In other words, that this LUT's end() method will be called before any
            // other Op's start().
            let mut transform = self.get_transform().borrow_mut();
            let ops = transform.get_ops_mut();
            let len = ops.len();
            let pos = len - 1;
            ops.insert(pos, Arc::new(OpData::Range(p_rng)));
        }
        Ok(())
    }

    /// Finalizes a Lut1D's or an InverseLUT1D's array, `position` values read: raw halfs
    /// converted, a single channel spread to the three.
    ///
    /// Port of `CTFReaderLut1DElt::endArray` (CTFReaderHelper.cpp:4201-4246) and
    /// `CTFReaderInvLut1DElt::endArray` (3378-3429) @ v2.5.2. The raw halfs' `(unsigned
    /// short)` cast of a `float` is undefined outside `[0, 65536)`; both wheels compile it as
    /// `cvttss2si` to a 32-bit `int`, whose low 16 bits they keep (I-181).
    fn lut1d_end_array(&mut self, position: u32) -> Result<()> {
        let inverse = self.kind == CtfReaderKind::InvLut1D;
        let raw_halfs = self.lut1d().is_output_raw_halfs();
        let array = self.lut1d_mut().get_array_mut();

        // Convert half bits to float values if needed.
        if raw_halfs {
            let max_values = array.get_num_values() as usize;
            for i in 0..max_values {
                let bits = sse_cvttps_epi32(array.get_values()[i]) as u16;
                array.get_values_mut()[i] = convert_half_bits_to_float(bits);
            }
        }

        if array.get_num_values() != c_ulong::from(position) {
            let num_color_components = array.get_num_color_components();
            let max_color_components = array.get_max_color_components();

            let dimensions = array.get_length();

            if num_color_components != 1 || c_ulong::from(position) != dimensions {
                let message = [
                    b"Expected ".as_slice(),
                    dimensions.to_string().as_bytes(),
                    b"x",
                    num_color_components.to_string().as_bytes(),
                    b" Array values, found ",
                    position.to_string().as_bytes(),
                    b".",
                ]
                .concat();
                return Err(if inverse {
                    self.throw_message(&message)
                } else {
                    self.throw_m(&[&message])
                });
            }

            // Convert a 1D LUT to a 3by1D LUT (duplicate values from the Red to the Green and
            // Blue).
            let num_luts = max_color_components as usize;

            // TODO: Should improve Lut1DOp so that the copy is unnecessary.
            let values = array.get_values_mut();
            for i in (0..dimensions as usize).rev() {
                for j in 0..num_luts {
                    values[(i * num_luts) + j] = values[i];
                }
            }
        }

        self.lut1d().get_array().validate()?;

        self.set_completed(true);
        Ok(())
    }
}

impl CtfReaderOpElt {
    /// The LUT of a Lut3D or InverseLUT3D reader.
    fn lut3d_mut(&mut self) -> &mut Lut3DOpData {
        match &mut self.op {
            OpData::Lut3D(lut) => lut,
            _ => panic!("not a Lut3D reader"),
        }
    }

    /// The interpolation (optional) of a Lut3D or an InverseLUT3D.
    ///
    /// Port of `CTFReaderLut3DElt::start` (CTFReaderHelper.cpp:4402-4429) and
    /// `CTFReaderInvLut3DElt::start` (3442-3469) @ v2.5.2, after `CTFReaderOpElt::start`.
    fn lut3d_start(&mut self, atts: &[&[u8]]) -> Result<()> {
        // The interpolation attribute is optional in CLF/CTF. The INTERP_DEFAULT enum
        // indicates that the value was not specified in the file. When writing, this means no
        // interpolation attribute will be added.
        self.lut3d_mut().set_interpolation(Interpolation::Default);

        let mut i = 0;
        while i < atts.len() {
            if equal_ignoring_case(ATTR_INTERPOLATION, atts[i]) {
                match get_interpolation_3d(Some(atts[i + 1])) {
                    Ok(interp) => self.lut3d_mut().set_interpolation(interp),
                    Err(e) => return Err(self.throw_message(e.what())),
                }
            }

            i += 2;
        }
        Ok(())
    }

    /// The array scaled from the file's output bit depth (the input one for an inverse),
    /// that bit depth recorded, the LUT validated.
    ///
    /// Port of `CTFReaderLut3DElt::end` (CTFReaderHelper.cpp:4437-4446),
    /// `CTFReaderLut3DElt_1_7::end` (4527-4557) and `CTFReaderInvLut3DElt::end` (3477-3489)
    /// @ v2.5.2, after `CTFReaderOpElt::end`.
    fn lut3d_end(&mut self) -> Result<()> {
        // Array values for inverse LUTs are scaled based on the inBitDepth (rather than
        // outBD), normalize them.
        let bit_depth = if self.kind == CtfReaderKind::InvLut3D {
            self.in_bit_depth
        } else {
            self.out_bit_depth
        };
        let scale = 1.0f32 / get_bit_depth_max_value(bit_depth)? as f32;
        let lut = self.lut3d_mut();
        lut.scale(scale);

        // For an inverse LUT, it's the input bit-depth that describes the array scaling.
        lut.set_file_output_bit_depth(bit_depth);
        lut.validate()
    }

    /// Finalizes a Lut3D's or an InverseLUT3D's array, `position` values read (blue
    /// fastest).
    ///
    /// Port of `CTFReaderLut3DElt::endArray` (CTFReaderHelper.cpp:4479-4492) and
    /// `CTFReaderInvLut3DElt::endArray` (3521-3541) @ v2.5.2.
    fn lut3d_end_array(&mut self, position: u32) -> Result<()> {
        // NB: A CLF/CTF Lut3D Array stores the elements in blue-fastest order.
        let array = self.lut3d_mut().get_array_mut();
        if array.get_num_values() != c_ulong::from(position) {
            let len = array.get_length().to_string();
            let message = [
                b"Expected ".as_slice(),
                len.as_bytes(),
                b"x",
                len.as_bytes(),
                b"x",
                len.as_bytes(),
                b"x",
                array.get_num_color_components().to_string().as_bytes(),
                b" Array values, found ",
                position.to_string().as_bytes(),
                b".",
            ]
            .concat();
            return Err(self.throw_message(&message));
        }

        array.validate()?;
        self.set_completed(true);
        Ok(())
    }
}

impl CtfReaderOpElt {
    /// The fixed function of an ACES, FixedFunction or Function reader.
    ///
    /// Port of `CTFReaderACESElt::getFixedFunction` (CTFReaderHelper.h:394-397 @ v2.5.2).
    pub fn fixed_function_mut(&mut self) -> &mut FixedFunctionOpData {
        match &mut self.op {
            OpData::FixedFunction(ff) => ff,
            _ => panic!("not a fixed function reader"),
        }
    }

    /// The style (required; unknown styles are errors) and, for a FixedFunction, its
    /// parameters.
    ///
    /// Port of `CTFReaderACESElt::start` (CTFReaderHelper.cpp:1209-1239),
    /// `CTFReaderFixedFunctionElt::start` (1427-1473) and `CTFReaderFunctionElt::start`
    /// (1506-1534) @ v2.5.2, after `CTFReaderOpElt::start`.
    fn fixed_function_start(&mut self, atts: &[&[u8]]) -> Result<()> {
        let mut is_style_found = false;

        let mut i = 0;
        while i < atts.len() {
            if equal_ignoring_case(ATTR_STYLE, atts[i]) {
                // We need a valid style to parse the parameters. This will throw on
                // unrecognized styles.
                match FixedFunctionOpStyle::from_name(Some(utf8(atts[i + 1]))) {
                    Ok(style) => {
                        self.fixed_function_mut().set_style(style);
                        is_style_found = true;
                    }
                    Err(ce) => return Err(self.throw_message(c_str(ce.what()))),
                }
            } else if self.kind == CtfReaderKind::FixedFunction
                && equal_ignoring_case(ATTR_PARAMS, atts[i])
            {
                let params_str = atts[i + 1];
                let len = params_str.len();
                let data = match get_numbers::<f64>(params_str, len) {
                    Ok(data) => data,
                    Err(_ce) => {
                        return Err(self.throw_m(&[
                            b"Illegal '",
                            self.get_type_name(),
                            b"' params ",
                            &truncate_string(params_str, len),
                            b".",
                        ]));
                    }
                };
                self.fixed_function_mut().set_params(data);
            }

            i += 2;
        }
        if !is_style_found {
            return Err(self.throw_message(if self.kind == CtfReaderKind::Aces {
                b"style parameter for FixedFunction is missing."
            } else {
                b"Style parameter for FixedFunction is missing."
            }));
        }
        Ok(())
    }
}

impl CtfReaderOpElt {
    /// The op data of a GradingRGBCurve reader.
    ///
    /// Port of `CTFReaderGradingRGBCurveElt::getGradingRGBCurve` (CTFReaderHelper.h:686-689 @
    /// v2.5.2).
    fn grading_rgb_curve_mut(&mut self) -> &mut GradingRgbCurveOpData {
        match &mut self.op {
            OpData::GradingRgbCurve(curve) => curve,
            _ => panic!("not a GradingRGBCurve reader"),
        }
    }

    /// The curves the curve elements of a GradingRGBCurve reader fill.
    ///
    /// Port of `CTFReaderGradingRGBCurveElt::getLoadingRGBCurve` (CTFReaderHelper.h:692 @
    /// v2.5.2). The reader's `start` set them, or failed.
    pub fn get_loading_rgb_curve_mut(&mut self) -> &mut GradingRgbCurve {
        self.loading_rgb_curve
            .as_mut()
            .expect("the GradingRGBCurve's start set its curves")
    }

    /// Whether this is a GradingRGBCurve reader (`dynamic_cast<CTFReaderGradingRGBCurveElt
    /// *>`).
    pub fn is_grading_rgb_curve(&self) -> bool {
        self.kind == CtfReaderKind::GradingRgbCurve
    }

    /// The style and direction (required), which give the curves their defaults, and
    /// `bypassLinToLog` (only `true`, ignoring case).
    ///
    /// Port of `CTFReaderGradingRGBCurveElt::start` (CTFReaderHelper.cpp:2592-2640 @ v2.5.2),
    /// after `CTFReaderOpElt::start`.
    fn grading_rgb_curve_start(&mut self, atts: &[&[u8]]) -> Result<()> {
        let mut is_style_found = false;

        let mut i = 0;
        while i < atts.len() {
            if equal_ignoring_case(ATTR_STYLE, atts[i]) {
                match convert_string_to_grading_style_and_dir(Some(atts[i + 1])) {
                    Ok((style, dir)) => {
                        let curve = self.grading_rgb_curve_mut();
                        curve.set_style(style);
                        curve.set_direction(dir);

                        // Initialize loading curve with corresponding style.
                        self.loading_rgb_curve = Some(GradingRgbCurve::new(style));
                    }
                    Err(_) => {
                        return Err(self.throw_m(&[
                            b"Required attribute 'style' '",
                            c_str(atts[i + 1]),
                            b"' is invalid.",
                        ]));
                    }
                }
                is_style_found = true;
            } else if equal_ignoring_case(ATTR_BYPASS_LIN_TO_LOG, atts[i]) {
                if !equal_ignoring_case(b"true", atts[i + 1]) {
                    let mut oss = OStringStream::new(Crt::NATIVE);
                    oss.put_str("Unknown bypassLinToLog value: '");
                    oss.put_c_str(atts[i + 1]);
                    oss.put_str("' while parsing RGBCurve.");
                    return Err(self.throw_message(oss.str()));
                }

                self.grading_rgb_curve_mut().set_bypass_lin_to_log(true);
            }

            i += 2;
        }

        if !is_style_found {
            return Err(self.throw_m(&[b"Required attribute 'style' is missing."]));
        }
        Ok(())
    }

    /// The loaded curves set (which validates them), then validated.
    ///
    /// Port of `CTFReaderGradingRGBCurveElt::end` (CTFReaderHelper.cpp:2642-2650 @ v2.5.2),
    /// after `CTFReaderOpElt::end`.
    fn grading_rgb_curve_end(&mut self) -> Result<()> {
        // Set the loaded data.
        let loading = self.get_loading_rgb_curve_mut().clone();
        let curve = self.grading_rgb_curve_mut();
        curve.set_value(&loading)?;
        // Validate the end result.
        curve.validate()
    }
}

impl CtfReaderOpElt {
    /// The reference of a Reference reader.
    ///
    /// Port of `CTFReaderReferenceElt::getReference` (CTFReaderHelper.h:1180-1183 @ v2.5.2).
    fn reference_mut(&mut self) -> &mut ReferenceOpData {
        match &mut self.op {
            OpData::Reference(reference) => reference,
            _ => panic!("not a Reference reader"),
        }
    }

    /// The path (required) or the alias (not `currentMonitor`, and without a path or a
    /// basePath, which is ignored), and the direction (`inverted="true"`, ignoring case).
    ///
    /// Port of `CTFReaderReferenceElt::start` (CTFReaderHelper.cpp:5024-5093 @ v2.5.2), after
    /// `CTFReaderOpElt::start`.
    fn reference_start(&mut self, atts: &[&[u8]]) -> Result<()> {
        let mut alias: Vec<u8> = Vec::new();
        let mut path: Vec<u8> = Vec::new();
        let mut base_path_found = false;

        let mut i = 0;
        while i < atts.len() {
            if equal_ignoring_case(ATTR_PATH, atts[i]) {
                path = c_str(atts[i + 1]).to_vec();
            } else if equal_ignoring_case(ATTR_BASE_PATH, atts[i]) {
                // Ignore BasePath for now: BasePath could be used to point to a specific
                // folder, but for OCIO all folders have to be reacheable through the Context.
                // All paths might be absolute or relative.
                base_path_found = true;
            } else if equal_ignoring_case(ATTR_ALIAS, atts[i]) {
                // Most alias operators may be ignored, with the exception of currentMonitor.
                // Since we cannot apply that transform here, we need to throw.
                alias = c_str(atts[i + 1]).to_vec();
                if equal_ignoring_case(&alias, b"currentMonitor") {
                    return Err(self.throw_message(b"The 'currentMonitor' alias is not supported."));
                }
            } else if equal_ignoring_case(ATTR_IS_INVERTED, atts[i])
                && equal_ignoring_case(b"true", atts[i + 1])
            {
                self.reference_mut()
                    .set_direction(TransformDirection::Inverse);
            }

            i += 2;
        }
        if !alias.is_empty() {
            if !path.is_empty() {
                return Err(self.throw_message(
                    b"alias & path attributes for Reference should not be both defined.",
                ));
            }

            if base_path_found {
                return Err(self.throw_message(
                    b"alias & basepath attributes for Reference should not be both defined.",
                ));
            }

            self.reference_mut().set_alias(&alias);
        } else {
            if path.is_empty() {
                return Err(self.throw_message(b"path attribute for Reference is missing."));
            }

            self.reference_mut().set_path(&path);
        }
        Ok(())
    }
}

/// The Range of a CTF LUT's 2-entry IndexMap: its first pair's halves, scaled to the LUT's
/// input bit depth and to its `len` entries, as the minimums, the second pair's as the
/// maximums.
///
/// Port of `RangeOpData::RangeOpData(const IndexMapping &, unsigned int, BitDepth)`
/// (src/OpenColorIO/ops/range/RangeOpData.cpp:67-103 @ v2.5.2), which needs the CTF reader's
/// `IndexMapping`.
fn range_from_index_mapping(
    p_im: &IndexMapping,
    len: u32,
    bitdepth: BitDepth,
) -> Result<RangeOpData> {
    let mut range = RangeOpData::new();
    range.set_file_input_bit_depth(bitdepth);
    range.set_file_output_bit_depth(bitdepth);

    if p_im.get_dimension() != 2 {
        return Err(Exception::new(
            "CTF/CLF parsing error. Only two entry IndexMaps are supported.",
        ));
    }

    let scale_in = 1.0 / get_bit_depth_max_value(bitdepth)?;

    let (first, second) = p_im.get_pair(0)?;

    // The first half of the pair is scaled to the LUT's input bit depth.
    range.set_min_in_value(f64::from(first) * scale_in);
    // The second half is scaled to the number of entries in the LUT.
    range.set_min_out_value(f64::from(second) / f64::from(len.wrapping_sub(1)));

    // Note: The CLF spec does not say how to handle out-of-range values. E.g., a user could
    // specify an index longer than the LUT length. For now, we are not preventing this (no
    // harm is done since those values are already clipped safely on input to the LUT
    // renderers).

    let (first, second) = p_im.get_pair(1)?;
    range.set_max_in_value(f64::from(first) * scale_in);
    range.set_max_out_value(f64::from(second) / f64::from(len.wrapping_sub(1)));

    range.validate()?;
    Ok(range)
}

impl XmlReaderElement for CtfReaderOpElt {
    fn element(&self) -> &XmlReaderElementBase {
        &self.element
    }
    fn element_mut(&mut self) -> &mut XmlReaderElementBase {
        &mut self.element
    }

    /// Port of `CTFReaderOpElt::start` (CTFReaderHelper.cpp:837-924), `CTFReaderACESElt::start`
    /// (1209-1239), `CTFReaderCDLElt::start` (1338-1359), `CTFReaderFixedFunctionElt::start`
    /// (1427-1473), `CTFReaderFunctionElt::start` (1506-1534),
    /// `CTFReaderGradingRGBCurveElt::start` (2592-2640), `CTFReaderReferenceElt::start`
    /// (5024-5093), `CTFReaderGammaElt::start` (1871-1916), `CTFReaderLogElt::start`
    /// (3555-3580) and `CTFReaderRangeElt_1_7::start` (4897-4915) @ v2.5.2.
    fn start(&mut self, atts: &[&[u8]]) -> Result<()> {
        self.op_start(atts)?;
        match self.kind {
            CtfReaderKind::Cdl => return self.cdl_start(atts),
            CtfReaderKind::Gamma
            | CtfReaderKind::Gamma1_5
            | CtfReaderKind::GammaCtf2_0
            | CtfReaderKind::GammaClf3_0 => return self.gamma_start(atts),
            CtfReaderKind::Log | CtfReaderKind::Log2_0 => return self.log_start(atts),
            CtfReaderKind::Lut1D
            | CtfReaderKind::Lut1D1_4
            | CtfReaderKind::Lut1D1_7
            | CtfReaderKind::InvLut1D => return self.lut1d_start(atts),
            CtfReaderKind::Lut3D | CtfReaderKind::Lut3D1_7 | CtfReaderKind::InvLut3D => {
                return self.lut3d_start(atts);
            }
            CtfReaderKind::Aces | CtfReaderKind::FixedFunction | CtfReaderKind::Function => {
                return self.fixed_function_start(atts);
            }
            CtfReaderKind::GradingRgbCurve => return self.grading_rgb_curve_start(atts),
            CtfReaderKind::Reference => return self.reference_start(atts),
            _ => {}
        }
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

    /// Port of `CTFReaderOpElt::end` (CTFReaderHelper.cpp:926-928), `CTFReaderACESElt::end`
    /// (1247-1252), `CTFReaderCDLElt::end` (1367-1373), `CTFReaderFixedFunctionElt::end`
    /// (1482-1487), `CTFReaderFunctionElt::end` (1543-1548),
    /// `CTFReaderGradingRGBCurveElt::end` (2642-2650), `CTFReaderReferenceElt::end`
    /// (5104-5109), `CTFReaderGammaElt::end` (1924-1937), `CTFReaderLogElt::end` (3588-3622),
    /// `CTFReaderLogElt_2_0::end` (3664-3717), `CTFReaderMatrixElt::end` (4572-4583),
    /// `CTFReaderRangeElt::end` (4868-4879) and `CTFReaderRangeElt_1_7::end` (4923-4943) @
    /// v2.5.2.
    fn end(&mut self) -> Result<()> {
        match self.kind {
            CtfReaderKind::Matrix | CtfReaderKind::Matrix1_3 => self.matrix_end()?,
            CtfReaderKind::Range | CtfReaderKind::Range1_7 => self.range_end()?,
            CtfReaderKind::Cdl => {
                // Validate the end result.
                self.get_cdl().borrow().validate()?;
            }
            CtfReaderKind::Gamma
            | CtfReaderKind::Gamma1_5
            | CtfReaderKind::GammaCtf2_0
            | CtfReaderKind::GammaClf3_0 => self.gamma_end()?,
            CtfReaderKind::Log => self.log_end()?,
            CtfReaderKind::Log2_0 => self.log_end_2_0()?,
            CtfReaderKind::Lut1D
            | CtfReaderKind::Lut1D1_4
            | CtfReaderKind::Lut1D1_7
            | CtfReaderKind::InvLut1D => self.lut1d_end()?,
            CtfReaderKind::Lut3D | CtfReaderKind::Lut3D1_7 | CtfReaderKind::InvLut3D => {
                self.lut3d_end()?
            }
            CtfReaderKind::Aces | CtfReaderKind::FixedFunction | CtfReaderKind::Function => {
                self.fixed_function_mut().validate()?
            }
            CtfReaderKind::GradingRgbCurve => self.grading_rgb_curve_end()?,
            CtfReaderKind::Reference => self.reference_mut().validate(),
            _ => {}
        }
        self.commit_op();
        if self.kind == CtfReaderKind::Lut1D1_7 || self.kind == CtfReaderKind::Lut3D1_7 {
            self.insert_index_map_range()?;
        }

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
        &self.id
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
        match &self.cdl {
            Some(cdl) => cdl
                .borrow_mut()
                .get_format_metadata_mut()
                .get_children_elements_mut()
                .push(item),
            None => self
                .op
                .get_format_metadata_mut()
                .get_children_elements_mut()
                .push(item),
        }
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

/// The parameters of a Gamma (`GammaParams`) or an Exponent (`ExponentParams`): a channel
/// (optional) and the style's gamma and offset.
///
/// Port of `CTFReaderGammaParamsElt` and `CTFReaderGammaParamsElt_1_5` (CTFReaderHelper.h:
/// 585-615, CTFReaderHelper.cpp:2016-2200 @ v2.5.2): `alpha` is the second's channel `A`.
#[derive(Debug)]
pub struct CtfReaderGammaParamsElt {
    plain: XmlReaderPlainEltBase,
    /// Whether the channel `A` is allowed (`CTFReaderGammaParamsElt_1_5`).
    alpha: bool,
}

impl CtfReaderGammaParamsElt {
    /// Port of `CTFReaderGammaParamsElt::CTFReaderGammaParamsElt` (CTFReaderHelper.cpp:
    /// 2016-2022) and `CTFReaderGammaParamsElt_1_5::CTFReaderGammaParamsElt_1_5` (2174-2180)
    /// @ v2.5.2.
    pub fn new(
        name: &[u8],
        parent: ContainerEltRcPtr,
        xml_line_number: u32,
        xml_file: &[u8],
        alpha: bool,
    ) -> CtfReaderGammaParamsElt {
        CtfReaderGammaParamsElt {
            plain: XmlReaderPlainEltBase::new(name, parent, xml_line_number, xml_file),
            alpha,
        }
    }

    /// The channel `name` names: `R`, `G`, `B` (0 to 2), and `A` (3) from CTF 1.5; -1 for
    /// another.
    ///
    /// Port of `CTFReaderGammaParamsElt::getChannelNumber` (CTFReaderHelper.cpp:2154-2172) and
    /// `CTFReaderGammaParamsElt_1_5::getChannelNumber` (2186-2200) @ v2.5.2.
    fn get_channel_number(&self, name: &[u8]) -> i32 {
        if self.alpha && equal_ignoring_case(b"A", name) {
            return 3;
        }
        // Version prior to 1.3 only supports R, G and B channels.
        if equal_ignoring_case(b"R", name) {
            0
        } else if equal_ignoring_case(b"G", name) {
            1
        } else if equal_ignoring_case(b"B", name) {
            2
        } else {
            -1
        }
    }
}

impl XmlReaderElement for CtfReaderGammaParamsElt {
    fn element(&self) -> &XmlReaderElementBase {
        &self.plain.element
    }
    fn element_mut(&mut self) -> &mut XmlReaderElementBase {
        &mut self.plain.element
    }

    /// The channel, gamma (or exponent) and offset, checked against the parent's style, set
    /// on its channel or all.
    ///
    /// Port of `CTFReaderGammaParamsElt::start` (CTFReaderHelper.cpp:2028-2144 @ v2.5.2).
    fn start(&mut self, atts: &[&[u8]]) -> Result<()> {
        // Attributes we want to extract.
        let mut chan: i32 = -1;
        let mut gamma = f64::NAN;
        let mut offset = f64::NAN;

        // Try extracting the attributes.
        let mut i = 0;
        while i < atts.len() {
            let (att, value) = (atts[i], atts[i + 1]);
            if equal_ignoring_case(ATTR_CHAN, att) {
                chan = self.get_channel_number(value);

                // Chan is optional but, if present, must be legal.
                if chan == -1 {
                    return Err(self.throw_message(
                        &[b"Invalid channel: ".as_slice(), c_str(value), b"."].concat(),
                    ));
                }
            } else if equal_ignoring_case(ATTR_GAMMA, att)
                || equal_ignoring_case(ATTR_EXPONENT, att)
            {
                gamma = self
                    .plain
                    .element
                    .parse_scalar_attribute::<f64>(att, value)?;
            } else if equal_ignoring_case(ATTR_OFFSET, att) {
                offset = self
                    .plain
                    .element
                    .parse_scalar_attribute::<f64>(att, value)?;
            } else {
                self.plain.element.log_parameter_warning(att);
            }

            i += 2;
        }

        // Validate the attributes are appropriate for the gamma style and set the parameters
        // (numeric validation is done by GammaOp::validate).

        let parent = self.plain.get_parent().clone();
        let mut p_gamma = op_parent(&parent);

        let mut params = Vec::new();

        let style = p_gamma.gamma_mut().style();
        let style_name = GammaOpData::convert_style_to_string(style).as_bytes();
        let missing_gamma = || {
            self.throw_message(
                &[
                    b"Missing required gamma parameter for style: ".as_slice(),
                    style_name,
                    b".",
                ]
                .concat(),
            )
        };
        if style.is_basic() {
            if gamma.is_nan() {
                return Err(missing_gamma());
            }
            params.push(gamma);

            if !offset.is_nan() {
                return Err(self.throw_message(
                    &[
                        b"Illegal offset parameter for style: ".as_slice(),
                        style_name,
                        b".",
                    ]
                    .concat(),
                ));
            }
        } else {
            if gamma.is_nan() {
                return Err(missing_gamma());
            }
            params.push(gamma);

            if offset.is_nan() {
                return Err(self.throw_message(
                    &[
                        b"Missing required offset parameter for style: ".as_slice(),
                        style_name,
                        b".",
                    ]
                    .concat(),
                ));
            }
            params.push(offset);
        }

        // Assign the parameters to the object.
        let gamma_op = p_gamma.gamma_mut();
        match chan {
            -1 => gamma_op.set_params(&params),
            0 => gamma_op.set_red_params(params),
            1 => gamma_op.set_green_params(params),
            2 => gamma_op.set_blue_params(params),
            3 => gamma_op.set_alpha_params(params),
            _ => {}
        }
        Ok(())
    }

    /// Port of `CTFReaderGammaParamsElt::end` (CTFReaderHelper.cpp:2146-2148 @ v2.5.2).
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

impl XmlReaderPlainElt for CtfReaderGammaParamsElt {
    /// The text is ignored.
    ///
    /// Port of `CTFReaderGammaParamsElt::setRawData` (CTFReaderHelper.cpp:2150-2152 @
    /// v2.5.2).
    fn set_raw_data(&mut self, _s: &[u8], _xml_line: u32) -> Result<()> {
        Ok(())
    }
    fn get_parent(&self) -> &ContainerEltRcPtr {
        self.plain.get_parent()
    }
}

/// The parameters of a Log (`LogParams`): a channel (optional) and the legacy (Cineon)
/// parameters, or from CTF 2.0 and CLF 3.0 the OCIO ones.
///
/// Port of `CTFReaderLogParamsElt` and `CTFReaderLogParamsElt_2_0` (CTFReaderHelper.h:
/// 933-967, CTFReaderHelper.cpp:3728-4081 @ v2.5.2): `v2_0` is the second.
#[derive(Debug)]
pub struct CtfReaderLogParamsElt {
    plain: XmlReaderPlainEltBase,
    /// Whether this is a `CTFReaderLogParamsElt_2_0`.
    v2_0: bool,
}

/// The legacy (Cineon) parameters of a LogParams element: gamma, refWhite, refBlack,
/// highlight and shadow.
#[derive(Debug, Clone, Copy)]
struct Cineon {
    gamma: f64,
    ref_white: f64,
    ref_black: f64,
    highlight: f64,
    shadow: f64,
}

impl CtfReaderLogParamsElt {
    /// Port of `CTFReaderLogParamsElt::CTFReaderLogParamsElt` (CTFReaderHelper.cpp:
    /// 3728-3734 @ v2.5.2).
    pub fn new(
        name: &[u8],
        parent: ContainerEltRcPtr,
        xml_line_number: u32,
        xml_file: &[u8],
        v2_0: bool,
    ) -> CtfReaderLogParamsElt {
        CtfReaderLogParamsElt {
            plain: XmlReaderPlainEltBase::new(name, parent, xml_line_number, xml_file),
            v2_0,
        }
    }

    /// Reads the attribute `att` if it is a legacy parameter: whether it is one.
    ///
    /// Port of `CTFReaderLogParamsElt::parseCineon` (CTFReaderHelper.cpp:3740-3770 @ v2.5.2).
    fn parse_cineon(&self, att: &[u8], value: &[u8], cineon: &mut Cineon) -> Result<bool> {
        let element = &self.plain.element;
        if equal_ignoring_case(ATTR_GAMMA, att) {
            cineon.gamma = element.parse_scalar_attribute::<f64>(att, value)?;
            return Ok(true);
        } else if equal_ignoring_case(ATTR_REFWHITE, att) {
            cineon.ref_white = element.parse_scalar_attribute::<f64>(att, value)?;
            return Ok(true);
        } else if equal_ignoring_case(ATTR_REFBLACK, att) {
            cineon.ref_black = element.parse_scalar_attribute::<f64>(att, value)?;
            return Ok(true);
        } else if equal_ignoring_case(ATTR_HIGHLIGHT, att) {
            cineon.highlight = element.parse_scalar_attribute::<f64>(att, value)?;
            return Ok(true);
        } else if equal_ignoring_case(ATTR_SHADOW, att) {
            cineon.shadow = element.parse_scalar_attribute::<f64>(att, value)?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Sets the legacy parameters of the channel `chan` (-1 for all), each required.
    ///
    /// Port of `CTFReaderLogParamsElt::setCineon` (CTFReaderHelper.cpp:3772-3832 @ v2.5.2).
    fn set_cineon(&self, legacy_params: &mut CtfParams, chan: i32, cineon: Cineon) -> Result<()> {
        // Validate the attributes are appropriate for the log style and set the parameters
        // (numeric validation is done by LogOpData::validate).

        // Legacy parameters are set on the CTFReaderLogElt and will be transferred to the op
        // on the end() call.
        let mut ctf_values = vec![0.0; 5];

        let missing = |att: &[u8]| {
            self.throw_message(
                &[b"Required attribute '".as_slice(), att, b"' is missing."].concat(),
            )
        };

        if cineon.gamma.is_nan() {
            return Err(missing(ATTR_GAMMA));
        }
        ctf_values[ctf_values::GAMMA] = cineon.gamma;

        if cineon.ref_white.is_nan() {
            return Err(missing(ATTR_REFWHITE));
        }
        ctf_values[ctf_values::REF_WHITE] = cineon.ref_white;

        if cineon.ref_black.is_nan() {
            return Err(missing(ATTR_REFBLACK));
        }
        ctf_values[ctf_values::REF_BLACK] = cineon.ref_black;

        if cineon.highlight.is_nan() {
            return Err(missing(ATTR_HIGHLIGHT));
        }
        ctf_values[ctf_values::HIGHLIGHT] = cineon.highlight;

        if cineon.shadow.is_nan() {
            return Err(missing(ATTR_SHADOW));
        }
        ctf_values[ctf_values::SHADOW] = cineon.shadow;

        // Assign the parameters to the object.

        match chan {
            -1 => {
                legacy_params.params[0] = ctf_values.clone();
                legacy_params.params[1] = ctf_values.clone();
                legacy_params.params[2] = ctf_values;
            }
            0 => legacy_params.params[0] = ctf_values,
            1 => legacy_params.params[1] = ctf_values,
            2 => legacy_params.params[2] = ctf_values,
            _ => {}
        }
        Ok(())
    }

    /// The channel: `R`, `G` or `B` (0 to 2), else an error.
    fn channel(&self, value: &[u8]) -> Result<i32> {
        if equal_ignoring_case(b"R", value) {
            Ok(0)
        } else if equal_ignoring_case(b"G", value) {
            Ok(1)
        } else if equal_ignoring_case(b"B", value) {
            Ok(2)
        }
        // Chan is optional but, if present, must be legal.
        else {
            let mut arg = OStringStream::new(Crt::NATIVE);
            arg.put_str("Illegal channel attribute value '");
            arg.put_c_str(value);
            arg.put_str("'.");

            Err(self.throw_message(arg.str()))
        }
    }

    /// The legacy parameters of a CTF before 2.0.
    ///
    /// Port of `CTFReaderLogParamsElt::start` (CTFReaderHelper.cpp:3834-3888 @ v2.5.2).
    fn start_legacy(&mut self, atts: &[&[u8]]) -> Result<()> {
        let parent = self.plain.get_parent().clone();
        let mut p_log_elt = op_parent(&parent);

        // Attributes we want to extract.

        let mut chan: i32 = -1;

        // Legacy Log/Lin parameters:
        let mut cineon = Cineon {
            gamma: f64::NAN,
            ref_white: f64::NAN,
            ref_black: f64::NAN,
            highlight: f64::NAN,
            shadow: f64::NAN,
        };

        // Try extracting the attributes.
        let mut i = 0;
        while i < atts.len() {
            let (att, value) = (atts[i], atts[i + 1]);
            if equal_ignoring_case(ATTR_CHAN, att) {
                chan = self.channel(value)?;
            } else if !self.parse_cineon(att, value, &mut cineon)? {
                self.plain.element.log_parameter_warning(att);
            }

            i += 2;
        }

        self.set_cineon(p_log_elt.get_ctf_params_mut(), chan, cineon)
    }

    /// The parameters of CTF 2.0 and CLF 3.0: the OCIO ones (the camera styles' break and
    /// linear slope included), or the legacy ones in a CTF of a non-camera style, not mixed.
    ///
    /// Port of `CTFReaderLogParamsElt_2_0::start` (CTFReaderHelper.cpp:3898-4081 @ v2.5.2).
    fn start_2_0(&mut self, atts: &[&[u8]]) -> Result<()> {
        let parent = self.plain.get_parent().clone();
        let mut p_log_elt = op_parent(&parent);
        let style = p_log_elt.get_ctf_params().style;

        let camera_style = style == LogStyle::CameraLinToLog || style == LogStyle::CameraLogToLin;

        let allow_cineon = !camera_style && !p_log_elt.get_transform().borrow().is_clf();

        // Attributes we want to extract.

        let mut chan: i32 = -1;

        // Log/Lin parameters (CTF v2, CLF v3 and onward):
        let mut lin_side_slope = f64::NAN;
        let mut lin_side_offset = f64::NAN;
        let mut log_side_slope = f64::NAN;
        let mut log_side_offset = f64::NAN;
        let mut base = f64::NAN;
        let mut lin_side_break = f64::NAN;
        let mut linear_slope = f64::NAN;

        // Legacy Log/Lin parameters (if allowed for CTF v2 onward:
        let mut cineon = Cineon {
            gamma: f64::NAN,
            ref_white: f64::NAN,
            ref_black: f64::NAN,
            highlight: f64::NAN,
            shadow: f64::NAN,
        };

        // Try extracting the attributes.
        let mut i = 0;
        let mut valid_type = true;
        while i < atts.len() {
            let (att, value) = (atts[i], atts[i + 1]);
            let element = &self.plain.element;
            if equal_ignoring_case(ATTR_CHAN, att) {
                chan = self.channel(value)?;
            } else if equal_ignoring_case(ATTR_LINSIDESLOPE, att) {
                lin_side_slope = element.parse_scalar_attribute::<f64>(att, value)?;
                valid_type = p_log_elt.get_ctf_params_mut().set_type(CtfParamsType::Clf);
            } else if equal_ignoring_case(ATTR_LINSIDEOFFSET, att) {
                lin_side_offset = element.parse_scalar_attribute::<f64>(att, value)?;
                valid_type = p_log_elt.get_ctf_params_mut().set_type(CtfParamsType::Clf);
            } else if equal_ignoring_case(ATTR_LOGSIDESLOPE, att) {
                log_side_slope = element.parse_scalar_attribute::<f64>(att, value)?;
                valid_type = p_log_elt.get_ctf_params_mut().set_type(CtfParamsType::Clf);
            } else if equal_ignoring_case(ATTR_LOGSIDEOFFSET, att) {
                log_side_offset = element.parse_scalar_attribute::<f64>(att, value)?;
                valid_type = p_log_elt.get_ctf_params_mut().set_type(CtfParamsType::Clf);
            } else if equal_ignoring_case(ATTR_BASE, att) {
                base = element.parse_scalar_attribute::<f64>(att, value)?;
                valid_type = p_log_elt.get_ctf_params_mut().set_type(CtfParamsType::Clf);
            } else if equal_ignoring_case(ATTR_LINEARSLOPE, att) {
                linear_slope = element.parse_scalar_attribute::<f64>(att, value)?;
                valid_type = p_log_elt.get_ctf_params_mut().set_type(CtfParamsType::Clf);
            } else if equal_ignoring_case(ATTR_LINSIDEBREAK, att) {
                lin_side_break = element.parse_scalar_attribute::<f64>(att, value)?;
                valid_type = p_log_elt.get_ctf_params_mut().set_type(CtfParamsType::Clf);
            } else if allow_cineon && self.parse_cineon(att, value, &mut cineon)? {
                valid_type = p_log_elt
                    .get_ctf_params_mut()
                    .set_type(CtfParamsType::Cineon);
            } else {
                element.log_parameter_warning(att);
            }

            if !valid_type {
                return Err(
                    self.throw_message(b"CLF type and Cineon types parameters can not be mixed.")
                );
            }

            i += 2;
        }

        if p_log_elt.get_ctf_params().get_type() == CtfParamsType::Cineon {
            return self.set_cineon(p_log_elt.get_ctf_params_mut(), chan, cineon);
        }

        // Validate the attributes are appropriate for the log style and set the parameters
        // (numeric validation is done by LogOpData::validate).

        // New parameters are set directly on the op.
        let mut new_params = vec![0.0; 4];

        new_params[LIN_SIDE_SLOPE] = if lin_side_slope.is_nan() {
            1.0
        } else {
            lin_side_slope
        };
        new_params[LIN_SIDE_OFFSET] = if lin_side_offset.is_nan() {
            0.0
        } else {
            lin_side_offset
        };
        new_params[LOG_SIDE_SLOPE] = if log_side_slope.is_nan() {
            1.0
        } else {
            log_side_slope
        };
        new_params[LOG_SIDE_OFFSET] = if log_side_offset.is_nan() {
            0.0
        } else {
            log_side_offset
        };

        if !base.is_nan() {
            p_log_elt.set_base(base)?;
        }

        // The LogOpData does not have a style member, so some validation needs to happen
        // here.

        let camera_styles = |message: &[u8], att: &[u8], end: &[u8]| {
            self.throw_message(
                &[
                    b"Parameter '".as_slice(),
                    att,
                    message,
                    log_convert_style_to_string(LogStyle::CameraLogToLin).as_bytes(),
                    b"' or '",
                    log_convert_style_to_string(LogStyle::CameraLinToLog).as_bytes(),
                    end,
                ]
                .concat(),
            )
        };

        if !lin_side_break.is_nan() {
            if !camera_style {
                return Err(camera_styles(
                    b"' is only allowed for style '",
                    ATTR_LINSIDEBREAK,
                    b"'.",
                ));
            }
            new_params.push(lin_side_break);
        } else if camera_style {
            return Err(camera_styles(
                b"' should be defined for style '",
                ATTR_LINSIDEBREAK,
                b"'. ",
            ));
        }

        if !linear_slope.is_nan() {
            if !camera_style {
                return Err(camera_styles(
                    b"' is only allowed for style '",
                    ATTR_LINEARSLOPE,
                    b"'. ",
                ));
            }
            new_params.push(linear_slope);
        }

        let log_op = p_log_elt.log_mut();

        match chan {
            -1 => {
                log_op.set_red_params(new_params.clone());
                log_op.set_green_params(new_params.clone());
                log_op.set_blue_params(new_params);
            }
            0 => log_op.set_red_params(new_params),
            1 => log_op.set_green_params(new_params),
            2 => log_op.set_blue_params(new_params),
            _ => {}
        }
        Ok(())
    }
}

impl XmlReaderElement for CtfReaderLogParamsElt {
    fn element(&self) -> &XmlReaderElementBase {
        &self.plain.element
    }
    fn element_mut(&mut self) -> &mut XmlReaderElementBase {
        &mut self.plain.element
    }

    /// Port of `CTFReaderLogParamsElt::start` (CTFReaderHelper.cpp:3834-3888) and
    /// `CTFReaderLogParamsElt_2_0::start` (3898-4081) @ v2.5.2.
    fn start(&mut self, atts: &[&[u8]]) -> Result<()> {
        if self.v2_0 {
            self.start_2_0(atts)
        } else {
            self.start_legacy(atts)
        }
    }

    /// Port of `CTFReaderLogParamsElt::end` (CTFReaderHelper.cpp:3890-3892 @ v2.5.2).
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

impl XmlReaderPlainElt for CtfReaderLogParamsElt {
    /// The text is ignored.
    ///
    /// Port of `CTFReaderLogParamsElt::setRawData` (CTFReaderHelper.cpp:3894-3896 @ v2.5.2).
    fn set_raw_data(&mut self, _s: &[u8], _xml_line: u32) -> Result<()> {
        Ok(())
    }
    fn get_parent(&self) -> &ContainerEltRcPtr {
        self.plain.get_parent()
    }
}

/// A dynamic parameter of an operator: makes the property `param` names dynamic.
///
/// Port of `CTFReaderDynamicParamElt` (CTFReaderHelper.h:1231-1244, CTFReaderHelper.cpp:
/// 1556-1695 @ v2.5.2).
#[derive(Debug)]
pub struct CtfReaderDynamicParamElt {
    plain: XmlReaderPlainEltBase,
}

impl CtfReaderDynamicParamElt {
    /// Port of `CTFReaderDynamicParamElt::CTFReaderDynamicParamElt` (CTFReaderHelper.cpp:
    /// 1556-1562 @ v2.5.2).
    pub fn new(
        name: &[u8],
        parent: ContainerEltRcPtr,
        xml_line_number: u32,
        xml_file: &[u8],
    ) -> CtfReaderDynamicParamElt {
        CtfReaderDynamicParamElt {
            plain: XmlReaderPlainEltBase::new(name, parent, xml_line_number, xml_file),
        }
    }

    /// The error of a dynamic parameter the parent doesn't have.
    fn not_supported(&self, param: &[u8], container: &[u8]) -> Exception {
        self.throw_message(
            &[
                b"Dynamic parameter '".as_slice(),
                c_str(param),
                b"' is not supported in '",
                c_str(container),
                b"'.",
            ]
            .concat(),
        )
    }
}

impl XmlReaderElement for CtfReaderDynamicParamElt {
    fn element(&self) -> &XmlReaderElementBase {
        &self.plain.element
    }
    fn element_mut(&mut self) -> &mut XmlReaderElementBase {
        &mut self.plain.element
    }

    /// The property `param` names made dynamic, if the parent has it; `LOOK_SWITCH` is
    /// ignored with a warning, and another name is an error.
    ///
    /// Port of `CTFReaderDynamicParamElt::start` (CTFReaderHelper.cpp:1568-1695 @ v2.5.2). The
    /// ExposureContrast, GradingPrimary, GradingHueCurve and GradingTone readers come with WP
    /// 4.5i: until then no such parent is made, and their branches refuse the parameter as
    /// not ported.
    fn start(&mut self, atts: &[&[u8]]) -> Result<()> {
        let container = self.plain.get_parent().clone();
        let (kind, container_name) = {
            let container = container.borrow();
            let kind = container
                .as_any()
                .downcast_ref::<CtfReaderOpElt>()
                .map(CtfReaderOpElt::get_kind);
            (kind, container.get_name().to_vec())
        };

        // Try extracting the attributes.
        let mut i = 0;
        while i < atts.len() {
            if equal_ignoring_case(ATTR_PARAM, atts[i]) {
                let param = atts[i + 1];
                let parent_kind = if equal_ignoring_case(TAG_DYN_PROP_EXPOSURE, param)
                    || equal_ignoring_case(TAG_DYN_PROP_CONTRAST, param)
                    || equal_ignoring_case(TAG_DYN_PROP_GAMMA, param)
                {
                    Some(CtfReaderKind::ExposureContrast)
                } else if equal_ignoring_case(TAG_DYN_PROP_PRIMARY, param) {
                    Some(CtfReaderKind::GradingPrimary)
                } else if equal_ignoring_case(TAG_DYN_PROP_RGBCURVE, param) {
                    Some(CtfReaderKind::GradingRgbCurve)
                } else if equal_ignoring_case(TAG_DYN_PROP_HUECURVE, param) {
                    Some(CtfReaderKind::GradingHueCurve)
                } else if equal_ignoring_case(TAG_DYN_PROP_TONE, param) {
                    Some(CtfReaderKind::GradingTone)
                } else {
                    None
                };
                if let Some(parent_kind) = parent_kind {
                    if kind != Some(parent_kind) {
                        return Err(self.not_supported(param, &container_name));
                    }
                    if parent_kind != CtfReaderKind::GradingRgbCurve {
                        return Err(self.throw_message(
                            format!("The CTF reader's {parent_kind:?} reader is not ported yet")
                                .as_bytes(),
                        ));
                    }
                    op_parent(&container)
                        .grading_rgb_curve_mut()
                        .get_dynamic_property_internal()
                        .make_dynamic();
                } else if equal_ignoring_case(TAG_DYN_PROP_LOOK, param) {
                    // The LOOK_SWITCH was used in CTF files from SynColor but is not supported
                    // yet in OCIO. Ignoring it is similar to setting LOOK_SWITCH to 'on' but
                    // this is harmless since the defaultLook is also ignored.
                    let mut dbg = OStringStream::new(Crt::NATIVE);
                    dbg.put_c_str(self.get_xml_file());
                    dbg.put_str("(");
                    dbg.put_u32(self.get_xml_line_number());
                    dbg.put_str("): ");
                    dbg.put_str("Dynamic parameter '");
                    dbg.put_c_str(param);
                    dbg.put_str("' on '");
                    dbg.put_bytes(&container_name);
                    dbg.put_str("' is ignored.");
                    log_warning(c_str(dbg.str()));
                } else {
                    return Err(self.throw_message(
                        &[
                            b"Dynamic parameter '".as_slice(),
                            c_str(param),
                            b"' is not valid in '",
                            c_str(&container_name),
                            b"'.",
                        ]
                        .concat(),
                    ));
                }
            }

            i += 2;
        }
        Ok(())
    }

    /// Port of `CTFReaderDynamicParamElt::end` (CTFReaderHelper.h:1241 @ v2.5.2).
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

impl XmlReaderPlainElt for CtfReaderDynamicParamElt {
    /// Port of `CTFReaderDynamicParamElt::setRawData` (CTFReaderHelper.h:1243 @ v2.5.2).
    fn set_raw_data(&mut self, _s: &[u8], _xml_line: u32) -> Result<()> {
        Ok(())
    }
    fn get_parent(&self) -> &ContainerEltRcPtr {
        self.plain.get_parent()
    }
}

/// The parameter of an ACES (a deprecated FixedFunction): the gamma of the Rec.2100 surround
/// styles.
///
/// Port of `CTFReaderACESParamsElt` (CTFReaderHelper.h:406-421, CTFReaderHelper.cpp:1259-1323
/// @ v2.5.2).
#[derive(Debug)]
pub struct CtfReaderAcesParamsElt {
    plain: XmlReaderPlainEltBase,
}

impl CtfReaderAcesParamsElt {
    /// Port of `CTFReaderACESParamsElt::CTFReaderACESParamsElt` (CTFReaderHelper.cpp:
    /// 1259-1266 @ v2.5.2).
    pub fn new(
        name: &[u8],
        parent: ContainerEltRcPtr,
        xml_line_number: u32,
        xml_file: &[u8],
    ) -> CtfReaderAcesParamsElt {
        CtfReaderAcesParamsElt {
            plain: XmlReaderPlainEltBase::new(name, parent, xml_line_number, xml_file),
        }
    }
}

impl XmlReaderElement for CtfReaderAcesParamsElt {
    fn element(&self) -> &XmlReaderElementBase {
        &self.plain.element
    }
    fn element_mut(&mut self) -> &mut XmlReaderElementBase {
        &mut self.plain.element
    }

    /// The gamma, which only the Rec.2100 surround styles take, once.
    ///
    /// Port of `CTFReaderACESParamsElt::start` (CTFReaderHelper.cpp:1272-1323 @ v2.5.2). Its
    /// message for a missing gamma lacks a space after the attribute's name, as upstream's
    /// does (`docs/improvements.md` I-187).
    fn start(&mut self, atts: &[&[u8]]) -> Result<()> {
        // Attributes we want to extract.
        let mut gamma = f64::NAN;

        // Try extracting the attributes.
        let mut i = 0;
        while i < atts.len() {
            if equal_ignoring_case(ATTR_GAMMA, atts[i]) {
                gamma = self
                    .plain
                    .element
                    .parse_scalar_attribute::<f64>(atts[i], atts[i + 1])?;
            } else {
                self.plain.element.log_parameter_warning(atts[i]);
            }

            i += 2;
        }

        let parent = self.plain.get_parent().clone();
        let mut p_fixed_function = op_parent(&parent);

        let style = p_fixed_function.fixed_function_mut().style();
        let style_name = style.to_str(false).as_bytes();
        if style == FixedFunctionOpStyle::Rec2100SurroundFwd
            || style == FixedFunctionOpStyle::Rec2100SurroundInv
        {
            if !p_fixed_function.fixed_function_mut().params().is_empty() {
                return Err(self.throw_message(
                    &[
                        b"ACES FixedFunction element with style ".as_slice(),
                        style_name,
                        b" expects only 1 gamma parameter.",
                    ]
                    .concat(),
                ));
            }
            if gamma.is_nan() {
                return Err(self.throw_message(
                    &[
                        b"Missing required parameter ".as_slice(),
                        ATTR_GAMMA,
                        b"for ACES FixedFunction element with style ",
                        style_name,
                        b".",
                    ]
                    .concat(),
                ));
            }
            // Assign the parameters to the object.
            p_fixed_function
                .fixed_function_mut()
                .set_params(vec![gamma]);
        } else {
            return Err(self.throw_message(
                &[
                    b"ACES FixedFunction element with style ".as_slice(),
                    style_name,
                    b" does not take any parameter.",
                ]
                .concat(),
            ));
        }
        Ok(())
    }

    /// Port of `CTFReaderACESParamsElt::end` (CTFReaderHelper.h:418 @ v2.5.2).
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

impl XmlReaderPlainElt for CtfReaderAcesParamsElt {
    /// Port of `CTFReaderACESParamsElt::setRawData` (CTFReaderHelper.h:420 @ v2.5.2).
    fn set_raw_data(&mut self, _s: &[u8], _xml_line: u32) -> Result<()> {
        Ok(())
    }
    fn get_parent(&self) -> &ContainerEltRcPtr {
        self.plain.get_parent()
    }
}

/// Whether a grading curve's element names an RGB curve rather than a hue curve.
///
/// Port of `IsRGBCurveType` (CTFReaderHelper.cpp:2753-2776 @ v2.5.2).
fn is_rgb_curve_type(name: &[u8]) -> Result<bool> {
    if equal_ignoring_case(TAG_RGB_CURVE_RED, name)
        || equal_ignoring_case(TAG_RGB_CURVE_GREEN, name)
        || equal_ignoring_case(TAG_RGB_CURVE_BLUE, name)
        || equal_ignoring_case(TAG_RGB_CURVE_MASTER, name)
    {
        return Ok(true);
    } else if equal_ignoring_case(TAG_HUE_CURVE_HUE_HUE, name)
        || equal_ignoring_case(TAG_HUE_CURVE_HUE_SAT, name)
        || equal_ignoring_case(TAG_HUE_CURVE_HUE_LUM, name)
        || equal_ignoring_case(TAG_HUE_CURVE_LUM_SAT, name)
        || equal_ignoring_case(TAG_HUE_CURVE_SAT_SAT, name)
        || equal_ignoring_case(TAG_HUE_CURVE_LUM_LUM, name)
        || equal_ignoring_case(TAG_HUE_CURVE_SAT_LUM, name)
        || equal_ignoring_case(TAG_HUE_CURVE_HUE_FX, name)
    {
        return Ok(false);
    }
    Err(illegal_grading_curve_name(name))
}

/// The error of a name that isn't a grading curve's.
fn illegal_grading_curve_name(name: &[u8]) -> Exception {
    let mut err = OStringStream::new(Crt::NATIVE);
    err.put_str("Illegal grading curve name '");
    err.put_bytes(name);
    err.put_str("'.");
    Exception::new(c_str(err.str()))
}

/// The RGB curve an element names.
///
/// Port of `GetRGBCurveType` (CTFReaderHelper.cpp:2778-2799 @ v2.5.2).
fn get_rgb_curve_type(name: &[u8]) -> Result<RgbCurveType> {
    if equal_ignoring_case(TAG_RGB_CURVE_RED, name) {
        return Ok(RgbCurveType::Red);
    } else if equal_ignoring_case(TAG_RGB_CURVE_GREEN, name) {
        return Ok(RgbCurveType::Green);
    } else if equal_ignoring_case(TAG_RGB_CURVE_BLUE, name) {
        return Ok(RgbCurveType::Blue);
    } else if equal_ignoring_case(TAG_RGB_CURVE_MASTER, name) {
        return Ok(RgbCurveType::Master);
    }
    Err(illegal_grading_curve_name(name))
}

/// A curve of a GradingRGBCurve (`Red`, `Green`, `Blue` or `Master`) or of a GradingHueCurve:
/// its control points and slopes.
///
/// Port of `CTFReaderGradingCurveElt` (CTFReaderHelper.h:731-746, CTFReaderHelper.cpp:
/// 2738-2867 @ v2.5.2). Upstream keeps a pointer to the curve in the parent's loading
/// curves; here the element keeps which curve it is, and the parent holds it.
#[derive(Debug)]
pub struct CtfReaderGradingCurveElt {
    complex: XmlReaderComplexEltBase,
    /// `m_curve`: which curve of the parent's.
    curve: Option<RgbCurveType>,
}

impl CtfReaderGradingCurveElt {
    /// Port of `CTFReaderGradingCurveElt::CTFReaderGradingCurveElt` (CTFReaderHelper.cpp:
    /// 2738-2744 @ v2.5.2).
    pub fn new(
        name: &[u8],
        parent: ContainerEltRcPtr,
        xml_line_number: u32,
        xml_file: &[u8],
    ) -> CtfReaderGradingCurveElt {
        CtfReaderGradingCurveElt {
            complex: XmlReaderComplexEltBase::new(name, Some(parent), xml_line_number, xml_file),
            curve: None,
        }
    }

    /// The parent's reader and which of its curves this is.
    ///
    /// Port of `CTFReaderGradingCurveElt::getCurve` (CTFReaderHelper.h:743 @ v2.5.2).
    fn get_curve(&self) -> (&ContainerEltRcPtr, RgbCurveType) {
        (
            self.complex
                .get_parent()
                .expect("a grading curve is under its operator"),
            self.curve.expect("the curve's start found it"),
        )
    }
}

impl XmlReaderElement for CtfReaderGradingCurveElt {
    fn element(&self) -> &XmlReaderElementBase {
        &self.complex.element
    }
    fn element_mut(&mut self) -> &mut XmlReaderElementBase {
        &mut self.complex.element
    }

    /// Finds the parent's curve the name gives.
    ///
    /// Port of `CTFReaderGradingCurveElt::start` (CTFReaderHelper.cpp:2842-2863 @ v2.5.2).
    /// The hue curves' branch, whose parent is the GradingHueCurve reader, comes with WP
    /// 4.5i: until then no such parent is made.
    fn start(&mut self, _atts: &[&[u8]]) -> Result<()> {
        let found = (|| -> Result<Option<RgbCurveType>> {
            if is_rgb_curve_type(self.get_name())? {
                let ty = get_rgb_curve_type(self.get_name())?;
                let parent = self
                    .complex
                    .get_parent()
                    .expect("a grading curve is under its operator");
                let is_rgb_curve = parent
                    .borrow()
                    .as_any()
                    .downcast_ref::<CtfReaderOpElt>()
                    .is_some_and(CtfReaderOpElt::is_grading_rgb_curve);
                assert!(is_rgb_curve, "the parent is a GradingRGBCurve reader");
                Ok(Some(ty))
            } else {
                Ok(None)
            }
        })();
        match found {
            Ok(Some(ty)) => {
                self.curve = Some(ty);
                Ok(())
            }
            Ok(None) => Err(self
                .throw_message(b"The CTF reader's CTFReaderGradingHueCurveElt is not ported yet")),
            Err(ce) => Err(self.throw_message(c_str(ce.what()))),
        }
    }

    /// Port of `CTFReaderGradingCurveElt::end` (CTFReaderHelper.cpp:2865-2867 @ v2.5.2).
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
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl XmlReaderContainerElt for CtfReaderGradingCurveElt {
    /// Port of `XmlReaderComplexElt::appendMetadata` (XMLReaderHelper.h:342-344 @ v2.5.2).
    fn append_metadata(&mut self, _name: &[u8], _value: &[u8]) -> Result<()> {
        Ok(())
    }
}

/// The curve of the parent, a grading curve: its operator's loading curves and which of them.
fn parent_curve(parent: &ContainerEltRcPtr) -> (ContainerEltRcPtr, RgbCurveType) {
    let parent = parent.borrow();
    let curve = parent
        .as_any()
        .downcast_ref::<CtfReaderGradingCurveElt>()
        .expect("the parent is a grading curve");
    let (op, ty) = curve.get_curve();
    (op.clone(), ty)
}

/// The values of a grading curve's plain element, appended: the numbers of `s`.
///
/// Port of `CTFReaderGradingCurvePointsElt::setRawData` (CTFReaderHelper.cpp:2906-2920) and
/// `CTFReaderGradingCurveSlopesElt::setRawData` (2956-2970) @ v2.5.2.
fn append_curve_values(elt: &dyn XmlReaderElement, data: &mut Vec<f32>, s: &[u8]) -> Result<()> {
    let len = s.len();
    let values = match get_numbers::<f32>(s, len) {
        Ok(values) => values,
        Err(ce) => {
            return Err(elt.throw_message(
                &[
                    b"Illegal '".as_slice(),
                    elt.get_type_name(),
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
    data.extend_from_slice(&values);
    Ok(())
}

/// The control points of a grading curve: x and y pairs.
///
/// Port of `CTFReaderGradingCurvePointsElt` (CTFReaderHelper.h:748-764, CTFReaderHelper.cpp:
/// 2871-2920 @ v2.5.2).
#[derive(Debug)]
pub struct CtfReaderGradingCurvePointsElt {
    plain: XmlReaderPlainEltBase,
    /// `m_data`.
    data: Vec<f32>,
}

impl CtfReaderGradingCurvePointsElt {
    /// Port of `CTFReaderGradingCurvePointsElt::CTFReaderGradingCurvePointsElt`
    /// (CTFReaderHelper.cpp:2871-2877 @ v2.5.2).
    pub fn new(
        name: &[u8],
        parent: ContainerEltRcPtr,
        xml_line_number: u32,
        xml_file: &[u8],
    ) -> CtfReaderGradingCurvePointsElt {
        CtfReaderGradingCurvePointsElt {
            plain: XmlReaderPlainEltBase::new(name, parent, xml_line_number, xml_file),
            data: Vec::new(),
        }
    }
}

impl XmlReaderElement for CtfReaderGradingCurvePointsElt {
    fn element(&self) -> &XmlReaderElementBase {
        &self.plain.element
    }
    fn element_mut(&mut self) -> &mut XmlReaderElementBase {
        &mut self.plain.element
    }

    /// Port of `CTFReaderGradingCurvePointsElt::start` (CTFReaderHelper.cpp:2883-2885 @
    /// v2.5.2).
    fn start(&mut self, _atts: &[&[u8]]) -> Result<()> {
        Ok(())
    }

    /// The curve's control points: the values in pairs.
    ///
    /// Port of `CTFReaderGradingCurvePointsElt::end` (CTFReaderHelper.cpp:2887-2904 @
    /// v2.5.2).
    fn end(&mut self) -> Result<()> {
        if !self.data.len().is_multiple_of(2) {
            return Err(self.throw_message(b"Control points element: odd number of values."));
        }

        let (op, ty) = parent_curve(self.plain.get_parent());
        let mut p_op = op_parent(&op);
        let num_pts = self.data.len() / 2;
        let curve = p_op
            .get_loading_rgb_curve_mut()
            .curve_mut(ty)
            .expect("an RGB curve");
        curve.set_num_control_points(num_pts);
        for p in 0..num_pts {
            let ct_pt = curve
                .control_point_mut(p)
                .expect("the curve has that many points");
            ct_pt.x = self.data[2 * p];
            ct_pt.y = self.data[2 * p + 1];
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

impl XmlReaderPlainElt for CtfReaderGradingCurvePointsElt {
    /// Port of `CTFReaderGradingCurvePointsElt::setRawData` (CTFReaderHelper.cpp:2906-2920 @
    /// v2.5.2).
    fn set_raw_data(&mut self, s: &[u8], _xml_line: u32) -> Result<()> {
        let mut data = std::mem::take(&mut self.data);
        let result = append_curve_values(self, &mut data, s);
        self.data = data;
        result
    }
    fn get_parent(&self) -> &ContainerEltRcPtr {
        self.plain.get_parent()
    }
}

/// The slopes of a grading curve, one per control point.
///
/// Port of `CTFReaderGradingCurveSlopesElt` (CTFReaderHelper.h:766-782, CTFReaderHelper.cpp:
/// 2924-2970 @ v2.5.2).
#[derive(Debug)]
pub struct CtfReaderGradingCurveSlopesElt {
    plain: XmlReaderPlainEltBase,
    /// `m_data`.
    data: Vec<f32>,
}

impl CtfReaderGradingCurveSlopesElt {
    /// Port of `CTFReaderGradingCurveSlopesElt::CTFReaderGradingCurveSlopesElt`
    /// (CTFReaderHelper.cpp:2924-2930 @ v2.5.2).
    pub fn new(
        name: &[u8],
        parent: ContainerEltRcPtr,
        xml_line_number: u32,
        xml_file: &[u8],
    ) -> CtfReaderGradingCurveSlopesElt {
        CtfReaderGradingCurveSlopesElt {
            plain: XmlReaderPlainEltBase::new(name, parent, xml_line_number, xml_file),
            data: Vec::new(),
        }
    }
}

impl XmlReaderElement for CtfReaderGradingCurveSlopesElt {
    fn element(&self) -> &XmlReaderElementBase {
        &self.plain.element
    }
    fn element_mut(&mut self) -> &mut XmlReaderElementBase {
        &mut self.plain.element
    }

    /// Port of `CTFReaderGradingCurveSlopesElt::start` (CTFReaderHelper.cpp:2936-2938 @
    /// v2.5.2).
    fn start(&mut self, _atts: &[&[u8]]) -> Result<()> {
        Ok(())
    }

    /// The curve's slopes, as many as its control points so far.
    ///
    /// Port of `CTFReaderGradingCurveSlopesElt::end` (CTFReaderHelper.cpp:2940-2954 @
    /// v2.5.2).
    fn end(&mut self) -> Result<()> {
        let (op, ty) = parent_curve(self.plain.get_parent());
        let mut p_op = op_parent(&op);
        let num_vals = self.data.len();
        let curve = p_op
            .get_loading_rgb_curve_mut()
            .curve_mut(ty)
            .expect("an RGB curve");
        let num_ct_pnts = curve.num_control_points();
        if num_vals != num_ct_pnts {
            return Err(
                self.throw_message(b"Number of slopes must match number of control points.")
            );
        }
        for i in 0..num_vals {
            curve
                .set_slope(i, self.data[i])
                .expect("as many slopes as control points");
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

impl XmlReaderPlainElt for CtfReaderGradingCurveSlopesElt {
    /// Port of `CTFReaderGradingCurveSlopesElt::setRawData` (CTFReaderHelper.cpp:2956-2970 @
    /// v2.5.2).
    fn set_raw_data(&mut self, s: &[u8], _xml_line: u32) -> Result<()> {
        let mut data = std::mem::take(&mut self.data);
        let result = append_curve_values(self, &mut data, s);
        self.data = data;
        result
    }
    fn get_parent(&self) -> &ContainerEltRcPtr {
        self.plain.get_parent()
    }
}

/// The CDL of the parent, an ASC_CDL reader.
fn parent_cdl(parent: &ContainerEltRcPtr) -> CdlOpDataRcPtr {
    parent
        .borrow()
        .as_any()
        .downcast_ref::<CtfReaderOpElt>()
        .expect("the parent is an ASC_CDL reader")
        .get_cdl()
        .clone()
}

/// The SatNode of an ASC_CDL.
///
/// Port of `CTFReaderSatNodeElt` (CTFReaderHelper.h:444-456, CTFReaderHelper.cpp:1387-1398 @
/// v2.5.2), on `XmlReaderSatNodeBaseElt` (whose `start` and `end` do nothing).
#[derive(Debug)]
pub struct CtfReaderSatNodeElt {
    complex: XmlReaderComplexEltBase,
}

impl CtfReaderSatNodeElt {
    /// Port of `CTFReaderSatNodeElt::CTFReaderSatNodeElt` (CTFReaderHelper.cpp:1387-1393 @
    /// v2.5.2).
    pub fn new(
        name: &[u8],
        parent: ContainerEltRcPtr,
        xml_line_number: u32,
        xml_file: &[u8],
    ) -> CtfReaderSatNodeElt {
        CtfReaderSatNodeElt {
            complex: XmlReaderComplexEltBase::new(name, Some(parent), xml_line_number, xml_file),
        }
    }
}

impl XmlReaderElement for CtfReaderSatNodeElt {
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
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl XmlReaderContainerElt for CtfReaderSatNodeElt {
    fn append_metadata(&mut self, name: &[u8], value: &[u8]) -> Result<()> {
        let cdl = self.get_cdl();
        sat_node_append_metadata(&cdl, name, value)
    }
}

impl XmlReaderSatNodeBaseElt for CtfReaderSatNodeElt {
    /// Port of `CTFReaderSatNodeElt::getCDL` (CTFReaderHelper.cpp:1395-1398 @ v2.5.2).
    fn get_cdl(&self) -> CdlOpDataRcPtr {
        parent_cdl(
            self.complex
                .get_parent()
                .expect("a SatNode is under an ASC_CDL"),
        )
    }
}

/// The SOPNode of an ASC_CDL.
///
/// Port of `CTFReaderSOPNodeElt` (CTFReaderHelper.h:458-469, CTFReaderHelper.cpp:1402-1413 @
/// v2.5.2), on `XmlReaderSOPNodeBaseElt`.
#[derive(Debug)]
pub struct CtfReaderSopNodeElt {
    sop: XmlReaderSopNodeBase,
}

impl CtfReaderSopNodeElt {
    /// Port of `CTFReaderSOPNodeElt::CTFReaderSOPNodeElt` (CTFReaderHelper.cpp:1402-1408 @
    /// v2.5.2).
    pub fn new(
        name: &[u8],
        parent: ContainerEltRcPtr,
        xml_line_number: u32,
        xml_file: &[u8],
    ) -> CtfReaderSopNodeElt {
        CtfReaderSopNodeElt {
            sop: XmlReaderSopNodeBase::new(name, parent, xml_line_number, xml_file),
        }
    }
}

impl XmlReaderElement for CtfReaderSopNodeElt {
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
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl XmlReaderContainerElt for CtfReaderSopNodeElt {
    fn append_metadata(&mut self, name: &[u8], value: &[u8]) -> Result<()> {
        let cdl = self.get_cdl();
        XmlReaderSopNodeBase::append_metadata(&cdl, name, value)
    }
}

impl XmlReaderSopNodeBaseElt for CtfReaderSopNodeElt {
    /// Port of `CTFReaderSOPNodeElt::getCDL` (CTFReaderHelper.cpp:1410-1413 @ v2.5.2).
    fn get_cdl(&self) -> CdlOpDataRcPtr {
        parent_cdl(
            self.sop
                .complex
                .get_parent()
                .expect("a SOPNode is under an ASC_CDL"),
        )
    }
    fn sop_node(&mut self) -> &mut XmlReaderSopNodeBase {
        &mut self.sop
    }
}

/// Like `FindDelim` but looks for whitespace or an at sign (for IndexMap).
///
/// Port of `FindIndexDelim` (CTFReaderHelper.cpp:539-554 @ v2.5.2).
fn find_index_delim(s: &[u8], len: usize, mut pos: usize) -> usize {
    let at = |i: usize| s.get(i).copied().unwrap_or(0);
    while !is_space(at(pos)) && at(pos) != b'@' {
        pos += 1;

        if pos >= len {
            return len;
        }
    }

    pos
}

/// Like `FindNextTokenStart` but also skips at signs.
///
/// Port of `FindNextTokenStart_IndexMap` (CTFReaderHelper.cpp:557-577 @ v2.5.2).
fn find_next_token_start_index_map(s: &[u8], len: usize, mut pos: usize) -> usize {
    let at = |i: usize| s.get(i).copied().unwrap_or(0);
    if pos == len {
        return pos;
    }

    while is_number_delimiter(at(pos)) || at(pos) == b'@' {
        pos += 1;

        if pos >= len {
            return len;
        }
    }

    pos
}

/// The next pair of IndexMap numbers of `s` (its first `len` bytes) from `*pos`, like
/// `64.5@0` or `1 @2`; `*pos` moves to the next token, or `len`.
///
/// Port of `GetNextIndexPair` (CTFReaderHelper.cpp:590-629 @ v2.5.2).
fn get_next_index_pair(
    s: &[u8],
    len: usize,
    pos: &mut usize,
    num1: &mut f32,
    num2: &mut f32,
) -> Result<()> {
    // s might not be null terminated. Set pos to how much leading white space there is.
    *pos = find_next_token_start(s, len, *pos);

    if *pos != len {
        // Note, the len may here include the @ at the end of the string for ParseNumber (e.g.
        // "10.5@") but it does not cause the sscanf to fail. Set pos to advance over the
        // numbers we just parsed. Note that we stop either at white space or an ampersand.
        let mut end_pos = find_index_delim(s, len, *pos);
        if end_pos == len {
            let mut oss = OStringStream::new(Crt::NATIVE);
            oss.put_str("GetNextIndexPair: First number of a pair is the end of the string '");
            oss.put_bytes(&truncate_string(s, len));
            oss.put_str("'.");
            return Err(Exception::new(oss.into_bytes()));
        }

        // Extract a number at pos.
        parse_number(s, *pos, end_pos, num1)?;

        // Set pos to the start of the next number, advancing over white space or an @.
        *pos = find_next_token_start_index_map(s, len, end_pos);

        end_pos = find_delim(s, len, *pos);

        // Extract the other half of the index pair. Set pos to advance over the numbers we
        // just parsed.
        parse_number(s, *pos, end_pos, num2)?;

        *pos = end_pos;
        if *pos != len {
            *pos = find_next_token_start(s, len, *pos);
        }
    }
    Ok(())
}

/// A LUT's IndexMap: its dimension, then its pairs.
///
/// Port of `CTFReaderIndexMapElt` (CTFReaderHelper.h:248-272, CTFReaderHelper.cpp:433-534,
/// 632-671 @ v2.5.2). Upstream keeps a pointer to the map the parent's `updateDimensionIM`
/// returns; here the parent holds it.
#[derive(Debug)]
pub struct CtfReaderIndexMapElt {
    plain: XmlReaderPlainEltBase,
    /// The parent's type name, which `getTypeName` returns.
    type_name: Vec<u8>,
    /// `m_position`: the current position to fill.
    position: u32,
}

impl CtfReaderIndexMapElt {
    /// Port of `CTFReaderIndexMapElt::CTFReaderIndexMapElt` (CTFReaderHelper.cpp:433-441 @
    /// v2.5.2).
    pub fn new(
        name: &[u8],
        parent: ContainerEltRcPtr,
        xml_line_number: u32,
        xml_file: &[u8],
    ) -> CtfReaderIndexMapElt {
        let type_name = parent.borrow().get_type_name().to_vec();
        CtfReaderIndexMapElt {
            plain: XmlReaderPlainEltBase::new(name, parent, xml_line_number, xml_file),
            type_name,
            position: 0,
        }
    }

    /// The exception of `ThrowM(*this, ...)`.
    fn throw_m(&self, parts: &[&[u8]]) -> Exception {
        self.throw_message(&parts.concat())
    }
}

impl XmlReaderElement for CtfReaderIndexMapElt {
    fn element(&self) -> &XmlReaderElementBase {
        &self.plain.element
    }
    fn element_mut(&mut self) -> &mut XmlReaderElementBase {
        &mut self.plain.element
    }

    /// The `dim` attribute (required): the parent sizes its IndexMap.
    ///
    /// Port of `CTFReaderIndexMapElt::start` (CTFReaderHelper.cpp:448-510 @ v2.5.2).
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
                        b"' IndexMap dimensions ",
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
                    .filter(|op| op.is_index_map_mgt());
                let Some(p_arr) = p_arr else {
                    return Err(illegal(self));
                };
                if dims.len() != 1 {
                    return Err(illegal(self));
                }

                if !p_arr.update_dimension_im(&dims) {
                    return Err(illegal(self));
                }
            } else {
                self.plain.element.log_parameter_warning(atts[i]);
            }

            i += 2;
        }

        // Check mandatory elements
        if !is_dim_found {
            return Err(self.throw_message(b"Required attribute 'dim' is missing."));
        }

        self.position = 0;
        Ok(())
    }

    /// The parent finalizes its IndexMap before CTF 2.0 (CLF 3); from it, the element is
    /// logged as not valid.
    ///
    /// Port of `CTFReaderIndexMapElt::end` (CTFReaderHelper.cpp:512-534 @ v2.5.2).
    fn end(&mut self) -> Result<()> {
        // A known element (e.g. an IndexMap) in a dummy element, no need to validate it.
        if self.plain.get_parent().borrow().is_dummy() {
            return Ok(());
        }

        let parent = self.plain.get_parent().clone();
        let mut p_op_elt = op_parent(&parent);
        if p_op_elt.get_transform().borrow().get_ctf_version() < CTF_PROCESS_LIST_VERSION_2_0 {
            p_op_elt.end_index_map(self.position)
        } else {
            let mut dbg = OStringStream::new(Crt::NATIVE);
            dbg.put_c_str(self.get_xml_file());
            dbg.put_str("(");
            dbg.put_u32(self.get_xml_line_number());
            dbg.put_str("): ");
            dbg.put_str("Element '");
            dbg.put_bytes(self.get_name());
            dbg.put_str("' is not valid since CLF 3 (or CTF 2).");
            log_warning(dbg.str());
            Ok(())
        }
    }

    fn is_container(&self) -> bool {
        false
    }
    fn get_identifier(&self) -> &[u8] {
        self.get_name()
    }

    /// The parent's type name.
    ///
    /// Port of `CTFReaderIndexMapElt::getTypeName` (CTFReaderHelper.cpp:668-671 @ v2.5.2).
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

impl XmlReaderPlainElt for CtfReaderIndexMapElt {
    /// The pairs, up to the IndexMap's dimension.
    ///
    /// Port of `CTFReaderIndexMapElt::setRawData` (CTFReaderHelper.cpp:632-666 @ v2.5.2).
    fn set_raw_data(&mut self, s: &[u8], _xml_line: u32) -> Result<()> {
        let len = s.len();
        let parent = self.plain.get_parent().clone();
        let mut p = op_parent(&parent);
        let max_values = p.index_map_dimension();

        let mut pos = find_next_token_start(s, len, 0);
        while pos != len {
            let mut data1 = 0.0f32;
            let mut data2 = 0.0f32;

            if get_next_index_pair(s, len, &mut pos, &mut data1, &mut data2).is_err() {
                return Err(self.throw_m(&[
                    b"Illegal values '",
                    &truncate_string(s, len),
                    b"' in '",
                    self.get_type_name(),
                    b"' IndexMap.",
                ]));
            }

            if (self.position as usize) < max_values {
                p.index_map_set_pair(self.position as usize, data1, data2)?;
                self.position += 1;
            } else {
                return Err(self.throw_m(&[
                    b"Expected ",
                    max_values.to_string().as_bytes(),
                    b" entries, found too many values in '",
                    self.get_type_name(),
                    b"' IndexMap.",
                ]));
            }
        }
        Ok(())
    }
    fn get_parent(&self) -> &ContainerEltRcPtr {
        self.plain.get_parent()
    }
}
