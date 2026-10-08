// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The XML of a CDL, shared by the writers of the CC, CCC and CDL formats.
//!
//! Port of `src/OpenColorIO/fileformats/cdl/CDLWriter.h` and `CDLWriter.cpp` @ v2.5.2.

use ocio_formats::fileformats::xmlutils::xml_reader_utils::{
    ATTR_ID, ATTR_NAME, CDL_TAG_COLOR_CORRECTION, TAG_DESCRIPTION, TAG_OFFSET, TAG_POWER,
    TAG_SATNODE, TAG_SATURATION, TAG_SLOPE, TAG_SOPNODE,
};
use ocio_formats::fileformats::xmlutils::xml_writer_utils::{
    Attributes, XmlFormatter, XmlScopeIndent,
};
use ocio_ops::format_metadata::{FormatMetadataImpl, METADATA_DESCRIPTION, METADATA_ID};
use ocio_ops::parse_utils::{
    convert_special_char_to_xml_token, double_to_string, double_vec_to_string,
};
use ocio_ops::platform::strcasecmp;

use crate::transforms::cdl_transform::{
    CdlTransform, METADATA_INPUT_DESCRIPTION, METADATA_SAT_DESCRIPTION, METADATA_SOP_DESCRIPTION,
    METADATA_VIEWING_DESCRIPTION,
};

/// Writes a content element `tag` for each of `strings`.
///
/// Port of `WriteStrings` (CDLWriter.cpp:15-21 @ v2.5.2).
pub fn write_strings(fmt: &mut XmlFormatter<'_>, tag: &[u8], strings: &[Vec<u8>]) {
    for it in strings {
        fmt.write_content_tag(tag, it);
    }
}

/// The values of the descriptions among the metadata's children, by kind (names compared
/// without case), each escaped for XML. The formatter escapes them again when it writes them
/// (I-173).
///
/// Port of `ExtractCDLMetadata` (CDLWriter.cpp:23-55 @ v2.5.2).
pub fn extract_cdl_metadata(
    metadata: &FormatMetadataImpl,
    main_desc: &mut Vec<Vec<u8>>,
    input_desc: &mut Vec<Vec<u8>>,
    viewing_desc: &mut Vec<Vec<u8>>,
    sop_desc: &mut Vec<Vec<u8>>,
    sat_desc: &mut Vec<Vec<u8>>,
) {
    let nb_elt = metadata.get_num_children_elements();
    for i in 0..nb_elt {
        let elt = metadata
            .get_child_element(i)
            .expect("an index below the number of children");
        let name = elt.get_element_name();
        let value = || convert_special_char_to_xml_token(elt.get_element_value());
        if strcasecmp(name, METADATA_DESCRIPTION).is_eq() {
            main_desc.push(value());
        } else if strcasecmp(name, METADATA_INPUT_DESCRIPTION).is_eq() {
            input_desc.push(value());
        } else if strcasecmp(name, METADATA_VIEWING_DESCRIPTION).is_eq() {
            viewing_desc.push(value());
        } else if strcasecmp(name, METADATA_SOP_DESCRIPTION).is_eq() {
            sop_desc.push(value());
        } else if strcasecmp(name, METADATA_SAT_DESCRIPTION).is_eq() {
            sat_desc.push(value());
        }
    }
}

/// Writes `cdl` as a `ColorCorrection` element: its id and name attributes when not empty, its
/// descriptions, and its SOP and Sat nodes.
///
/// Port of `Write` (CDLWriter.cpp:57-111 @ v2.5.2).
pub fn write(fmt: &mut XmlFormatter<'_>, cdl: &CdlTransform) {
    let metadata = cdl.format_metadata();

    let mut attributes = Attributes::new();
    let id = metadata.get_attribute_value_by_name(Some(METADATA_ID));

    if !id.is_empty() {
        attributes.push((ATTR_ID.to_vec(), id.to_vec()));
    }

    let name = metadata.get_name();
    if !name.is_empty() {
        attributes.push((ATTR_NAME.to_vec(), name.to_vec()));
    }

    fmt.write_start_tag_with(CDL_TAG_COLOR_CORRECTION, &attributes);
    {
        let mut scope_indent = XmlScopeIndent::new(fmt);

        let mut main_desc = Vec::new();
        let mut input_desc = Vec::new();
        let mut viewing_desc = Vec::new();
        let mut sop_desc = Vec::new();
        let mut sat_desc = Vec::new();
        extract_cdl_metadata(
            metadata,
            &mut main_desc,
            &mut input_desc,
            &mut viewing_desc,
            &mut sop_desc,
            &mut sat_desc,
        );
        write_strings(&mut scope_indent, TAG_DESCRIPTION, &main_desc);
        write_strings(&mut scope_indent, METADATA_INPUT_DESCRIPTION, &input_desc);
        write_strings(
            &mut scope_indent,
            METADATA_VIEWING_DESCRIPTION,
            &viewing_desc,
        );

        scope_indent.write_start_tag(TAG_SOPNODE);
        {
            let mut scope_indent = XmlScopeIndent::new(&mut scope_indent);
            write_strings(&mut scope_indent, TAG_DESCRIPTION, &sop_desc);
            let rgb = cdl.slope();
            scope_indent.write_content_tag(TAG_SLOPE, &double_vec_to_string(&rgb));
            let rgb = cdl.offset();
            scope_indent.write_content_tag(TAG_OFFSET, &double_vec_to_string(&rgb));
            let rgb = cdl.power();
            scope_indent.write_content_tag(TAG_POWER, &double_vec_to_string(&rgb));
        }
        scope_indent.write_end_tag(TAG_SOPNODE);
        scope_indent.write_start_tag(TAG_SATNODE);
        {
            let mut scope_indent = XmlScopeIndent::new(&mut scope_indent);
            write_strings(&mut scope_indent, TAG_DESCRIPTION, &sat_desc);
            scope_indent.write_content_tag(TAG_SATURATION, &double_to_string(cdl.sat()));
        }
        scope_indent.write_end_tag(TAG_SATNODE);
    }
    fmt.write_end_tag(CDL_TAG_COLOR_CORRECTION);
}
