// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The XML writers' formatter: tags on their own lines, indented by four spaces a level, with
//! the special characters of attribute values and content escaped.
//!
//! Port of `src/OpenColorIO/fileformats/xmlutils/XMLWriterUtils.h` and `XMLWriterUtils.cpp`
//! (@ v2.5.2). Upstream writes to a `std::ostream`, which the writers also format numbers
//! into (`getStream`); here that is an [`OStringStream`].

use std::ops::{Deref, DerefMut};

use ocio_ops::Result;
use ocio_ops::cfmt::OStringStream;
use ocio_ops::parse_utils::convert_special_char_to_xml_token;

/// An attribute: its name and value (`XmlFormatter::Attribute`).
pub type Attribute = (Vec<u8>, Vec<u8>);
/// `XmlFormatter::Attributes`.
pub type Attributes = Vec<Attribute>;

/// Provides all services to write xml to an output stream.
///
/// Port of `XmlFormatter` (XMLWriterUtils.h:15-70, XMLWriterUtils.cpp:12-127 @ v2.5.2).
#[derive(Debug)]
pub struct XmlFormatter<'s> {
    stream: &'s mut OStringStream,
    indent_level: i32,
}

impl<'s> XmlFormatter<'s> {
    /// Port of `XmlFormatter::XmlFormatter` (XMLWriterUtils.cpp:12-16 @ v2.5.2).
    pub fn new(stream: &'s mut OStringStream) -> XmlFormatter<'s> {
        XmlFormatter {
            stream,
            indent_level: 0,
        }
    }

    /// Port of `XmlFormatter::incrementIndent` (XMLWriterUtils.cpp:23-26 @ v2.5.2).
    pub fn increment_indent(&mut self) {
        self.indent_level += 1;
    }

    /// Port of `XmlFormatter::decrementIndent` (XMLWriterUtils.cpp:28-31 @ v2.5.2).
    pub fn decrement_indent(&mut self) {
        self.indent_level -= 1;
    }

    /// Writes ` name="value"` for each attribute.
    fn write_attributes(&mut self, attributes: &Attributes) {
        for (name, value) in attributes {
            self.stream.put_str(" ");
            self.stream.put_bytes(name);
            self.stream.put_str("=\"");
            self.write_string(value);
            self.stream.put_str("\"");
        }
    }

    /// Write a start Element on a standalone line.
    ///
    /// Port of `XmlFormatter::writeStartTag(const std::string &, const Attributes &)`
    /// (XMLWriterUtils.cpp:33-47 @ v2.5.2).
    pub fn write_start_tag_with(&mut self, tag_name: &[u8], attributes: &Attributes) {
        self.write_indent();
        self.stream.put_str("<");
        self.stream.put_bytes(tag_name);
        self.write_attributes(attributes);
        self.stream.put_str(">\n");
    }

    /// Write a start Element on a standalone line.
    ///
    /// Port of `XmlFormatter::writeStartTag(const std::string &)` (XMLWriterUtils.cpp:49-53 @
    /// v2.5.2).
    pub fn write_start_tag(&mut self, tag_name: &[u8]) {
        let atts = Attributes::new();
        self.write_start_tag_with(tag_name, &atts);
    }

    /// Write an end Element on a standalone line.
    ///
    /// Port of `XmlFormatter::writeEndTag` (XMLWriterUtils.cpp:55-59 @ v2.5.2).
    pub fn write_end_tag(&mut self, tag_name: &[u8]) {
        self.write_indent();
        self.stream.put_str("</");
        self.stream.put_bytes(tag_name);
        self.stream.put_str(">\n");
    }

    /// Write \<tagName\>content\</tagName\> on a standalone line.
    ///
    /// Port of `XmlFormatter::writeContentTag(const std::string &, const std::string &)`
    /// (XMLWriterUtils.cpp:61-66 @ v2.5.2).
    pub fn write_content_tag(&mut self, tag_name: &[u8], content: &[u8]) {
        let atts = Attributes::new();
        self.write_content_tag_with(tag_name, &atts, content);
    }

    /// Write \<tagName\>content\</tagName\> on a standalone line.
    ///
    /// Port of `XmlFormatter::writeContentTag(const std::string &, const Attributes &,
    /// const std::string &)` (XMLWriterUtils.cpp:68-83 @ v2.5.2).
    pub fn write_content_tag_with(
        &mut self,
        tag_name: &[u8],
        attributes: &Attributes,
        content: &[u8],
    ) {
        self.write_indent();
        self.stream.put_str("<");
        self.stream.put_bytes(tag_name);
        self.write_attributes(attributes);
        self.stream.put_str(">");
        self.write_string(content);
        self.stream.put_str("</");
        self.stream.put_bytes(tag_name);
        self.stream.put_str(">\n");
    }

    /// Write the content using escaped characters if needed.
    ///
    /// Port of `XmlFormatter::writeContent` (XMLWriterUtils.cpp:85-91 @ v2.5.2).
    pub fn write_content(&mut self, content: &[u8]) {
        self.write_indent();
        self.write_string(content);
        self.stream.put_str("\n");
    }

    /// Write an empty Element on a standalone line: an Element without content and without
    /// children and which does not have a separate end tag.
    ///
    /// Port of `XmlFormatter::writeEmptyTag` (XMLWriterUtils.cpp:93-109 @ v2.5.2).
    pub fn write_empty_tag(&mut self, tag_name: &[u8], attributes: &Attributes) {
        self.write_indent();
        self.stream.put_str("<");
        self.stream.put_bytes(tag_name);
        self.write_attributes(attributes);
        // Note we close the tag, no end tag is needed.
        self.stream.put_str(" />\n");
    }

    /// Port of `XmlFormatter::getStream` (XMLWriterUtils.cpp:111-114 @ v2.5.2).
    pub fn get_stream(&mut self) -> &mut OStringStream {
        self.stream
    }

    /// Port of `XmlFormatter::writeIndent` (XMLWriterUtils.cpp:116-122 @ v2.5.2).
    fn write_indent(&mut self) {
        for _ in 0..self.indent_level {
            self.stream.put_str("    ");
        }
    }

    /// Port of `XmlFormatter::writeString` (XMLWriterUtils.cpp:124-127 @ v2.5.2).
    fn write_string(&mut self, content: &[u8]) {
        self.stream
            .put_bytes(&convert_special_char_to_xml_token(content));
    }
}

/// A transform element's XML writer.
///
/// Port of `XmlElementWriter` (XMLWriterUtils.h:72-87, XMLWriterUtils.cpp:141-148 @ v2.5.2).
/// Upstream's writers hold their formatter; here `write` is given it.
pub trait XmlElementWriter {
    /// `write`.
    fn write(&self, formatter: &mut XmlFormatter<'_>) -> Result<()>;
}

/// Increments the formatter's indentation while it lives, and gives access to the formatter.
///
/// Port of `XmlScopeIndent` (XMLWriterUtils.h:89-103, XMLWriterUtils.cpp:129-139 @ v2.5.2).
#[derive(Debug)]
pub struct XmlScopeIndent<'f, 's> {
    formatter: &'f mut XmlFormatter<'s>,
}

impl<'f, 's> XmlScopeIndent<'f, 's> {
    /// Port of `XmlScopeIndent::XmlScopeIndent` (XMLWriterUtils.cpp:129-133 @ v2.5.2).
    pub fn new(formatter: &'f mut XmlFormatter<'s>) -> XmlScopeIndent<'f, 's> {
        formatter.increment_indent();
        XmlScopeIndent { formatter }
    }
}

impl Drop for XmlScopeIndent<'_, '_> {
    /// Port of `XmlScopeIndent::~XmlScopeIndent` (XMLWriterUtils.cpp:135-139 @ v2.5.2).
    fn drop(&mut self) {
        self.formatter.decrement_indent();
    }
}

impl<'s> Deref for XmlScopeIndent<'_, 's> {
    type Target = XmlFormatter<'s>;
    fn deref(&self) -> &XmlFormatter<'s> {
        self.formatter
    }
}

impl<'s> DerefMut for XmlScopeIndent<'_, 's> {
    fn deref_mut(&mut self) -> &mut XmlFormatter<'s> {
        self.formatter
    }
}
