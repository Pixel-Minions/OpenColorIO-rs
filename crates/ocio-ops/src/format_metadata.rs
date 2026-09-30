// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The metadata of transforms, ops and LUT files: a port of `FormatMetadataImpl`
//! (`src/OpenColorIO/fileformats/FormatMetadata.h`, `FormatMetadata.cpp` @ v2.5.2) and of the
//! public `FormatMetadata` interface it implements (`include/OpenColorIO/OpenColorTransforms.h:
//! 44-117`). Upstream has no other implementation, so the port has no separate interface.
//!
//! A [`FormatMetadataImpl`] is a tree like an XML element: a name, a value, a list of
//! attributes (name and value pairs) and a list of child elements. CLF/CTF files keep their
//! `ProcessList` and process-node metadata in it, and every transform and op carries one.
//!
//! Strings are bytes (`docs/architecture.md`). A parameter upstream takes as `const char *` is
//! an `Option<&[u8]>`: `None` is a null pointer, and the value ends at its first NUL, as it
//! does in C. Getters that return a `const char *` upstream return the bytes before the first
//! NUL; the others return the whole string.
//!
//! Attribute names are compared in two ways, as upstream does: adding an attribute (and
//! [`set_name`](FormatMetadataImpl::set_name), [`set_id`](FormatMetadataImpl::set_id))
//! replaces one whose name is exactly the same, while looking one up by name and
//! [`combine`](FormatMetadataImpl::combine) ignore ASCII case (`Platform::Strcasecmp`).
//! Improvement candidate I-14 (`docs/improvements.md`).

use crate::platform;
use crate::utils::string_utils::c_str;
use crate::{Exception, Result};
use std::cmp::Ordering;

/// The name of a top-level element. Each element needs a name, and this one stands for the
/// file itself (a CLF/CTF `ProcessList`) or for an op; it is never written out.
///
/// Port of `METADATA_ROOT` (src/OpenColorIO/fileformats/FormatMetadata.h:19-23 @ v2.5.2).
pub const METADATA_ROOT: &[u8] = b"ROOT";

/// The "Description" element of CLF/CTF and CDL, which also holds the comments of other LUT
/// formats when baking.
///
/// Port of `METADATA_DESCRIPTION` (src/OpenColorIO/fileformats/FormatMetadata.cpp:17 @ v2.5.2;
/// declared in include/OpenColorIO/OpenColorTypes.h:942-946).
pub const METADATA_DESCRIPTION: &[u8] = b"Description";

/// The "Info" element of CLF/CTF, a block of informative metadata.
///
/// Port of `METADATA_INFO` (src/OpenColorIO/fileformats/FormatMetadata.cpp:18 @ v2.5.2;
/// declared in include/OpenColorIO/OpenColorTypes.h:948-952).
pub const METADATA_INFO: &[u8] = b"Info";

/// The "InputDescriptor" element of CLF/CTF (the "InputDescription" of CDL).
///
/// Port of `METADATA_INPUT_DESCRIPTOR` (src/OpenColorIO/fileformats/FormatMetadata.cpp:19 @
/// v2.5.2; declared in include/OpenColorIO/OpenColorTypes.h:954-958).
pub const METADATA_INPUT_DESCRIPTOR: &[u8] = b"InputDescriptor";

/// The "OutputDescriptor" element of CLF/CTF (the "OutputDescription" of CDL).
///
/// Port of `METADATA_OUTPUT_DESCRIPTOR` (src/OpenColorIO/fileformats/FormatMetadata.cpp:20 @
/// v2.5.2; declared in include/OpenColorIO/OpenColorTypes.h:960-964).
pub const METADATA_OUTPUT_DESCRIPTOR: &[u8] = b"OutputDescriptor";

/// The "name" attribute of CLF/CTF elements.
///
/// Port of `METADATA_NAME` (src/OpenColorIO/fileformats/FormatMetadata.cpp:23 @ v2.5.2;
/// declared in include/OpenColorIO/OpenColorTypes.h:966-971).
pub const METADATA_NAME: &[u8] = b"name";

/// The "id" attribute of CLF/CTF elements.
///
/// Port of `METADATA_ID` (src/OpenColorIO/fileformats/FormatMetadata.cpp:24 @ v2.5.2;
/// declared in include/OpenColorIO/OpenColorTypes.h:973-978).
pub const METADATA_ID: &[u8] = b"id";

/// An attribute: its name and its value.
///
/// Port of `FormatMetadataImpl::Attribute` (FormatMetadata.h:42 @ v2.5.2).
pub type Attribute = (Vec<u8>, Vec<u8>);

/// Port of `FormatMetadataImpl::Attributes` (FormatMetadata.h:43 @ v2.5.2).
pub type Attributes = Vec<Attribute>;

/// Port of `FormatMetadataImpl::Elements` (FormatMetadata.h:41 @ v2.5.2).
pub type Elements = Vec<FormatMetadataImpl>;

/// The null-terminated string a `const char *` argument points to, or `""` for a null pointer:
/// upstream's `name ? name : ""`.
fn arg(s: Option<&[u8]>) -> &[u8] {
    s.map_or(&[], c_str)
}

/// Whether two C strings are equal ignoring ASCII case: `0 == Platform::Strcasecmp(a, b)`.
fn equal_ignoring_case(a: &[u8], b: &[u8]) -> bool {
    platform::strcasecmp(a, b) == Ordering::Equal
}

/// Appends `second` to `first`, separated by `" + "` when both are non-empty.
///
/// Port of `Combine` (src/OpenColorIO/fileformats/FormatMetadata.cpp:127-137 @ v2.5.2).
fn combine_strings(first: &mut Vec<u8>, second: &[u8]) {
    if !second.is_empty() {
        if !first.is_empty() {
            first.extend_from_slice(b" + ");
        }
        first.extend_from_slice(second);
    }
}

/// A metadata element: a name (such as `Description`), a value (such as `updated viewing
/// LUT`), a list of attributes (such as `version` = `1.5`), and a list of child elements.
///
/// `Clone` is upstream's copy constructor and `operator=`; `==` compares the name, the value,
/// the attributes and the children, as upstream's `operator==` does
/// (FormatMetadata.cpp:71-78, 181-203 @ v2.5.2).
///
/// Port of `FormatMetadataImpl` (src/OpenColorIO/fileformats/FormatMetadata.h:38-117 @ v2.5.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatMetadataImpl {
    /// The element name.
    name: Vec<u8>,
    /// The element value.
    value: Vec<u8>,
    /// The element's list of attributes.
    attributes: Attributes,
    /// The list of sub-elements.
    elements: Elements,
}

impl Default for FormatMetadataImpl {
    /// [`FormatMetadataImpl::root`].
    fn default() -> Self {
        Self::root()
    }
}

impl FormatMetadataImpl {
    /// A top-level element: named [`METADATA_ROOT`], with no value, attributes or children.
    ///
    /// Port of `FormatMetadataImpl::FormatMetadataImpl()` (FormatMetadata.cpp:53-58 @ v2.5.2).
    pub fn root() -> Self {
        FormatMetadataImpl {
            name: METADATA_ROOT.to_vec(),
            value: Vec::new(),
            attributes: Attributes::new(),
            elements: Elements::new(),
        }
    }

    /// An element with a name and a value. The name must not be empty; unlike
    /// [`add_child_element`](Self::add_child_element), this accepts [`METADATA_ROOT`].
    ///
    /// Port of `FormatMetadataImpl::FormatMetadataImpl(const std::string &, const std::string &)`
    /// (FormatMetadata.cpp:60-69 @ v2.5.2).
    pub fn new(name: &[u8], value: &[u8]) -> Result<Self> {
        if name.is_empty() {
            return Err(Exception::new(
                "FormatMetadata has to have a non-empty name.",
            ));
        }
        Ok(FormatMetadataImpl {
            name: name.to_vec(),
            value: value.to_vec(),
            attributes: Attributes::new(),
            elements: Elements::new(),
        })
    }

    /// The attributes, in the order they were added.
    ///
    /// Port of `FormatMetadataImpl::getAttributes` (FormatMetadata.cpp:89-92 @ v2.5.2).
    pub fn get_attributes(&self) -> &Attributes {
        &self.attributes
    }

    /// Replaces the value of the attribute whose name is exactly `attribute`'s, or adds
    /// `attribute` after the others. This keeps an attribute name from appearing twice.
    ///
    /// Port of the protected `FormatMetadataImpl::addAttribute(const Attribute &)`
    /// (FormatMetadata.cpp:94-112 @ v2.5.2).
    fn add_attribute_pair(&mut self, attribute: Attribute) {
        match self.attributes.iter_mut().find(|a| a.0 == attribute.0) {
            Some(existing) => existing.1 = attribute.1,
            None => self.attributes.push(attribute),
        }
    }

    /// The child elements.
    ///
    /// Port of `FormatMetadataImpl::getChildrenElements() const` (FormatMetadata.cpp:119-122 @
    /// v2.5.2).
    pub fn get_children_elements(&self) -> &Elements {
        &self.elements
    }

    /// The child elements, to change.
    ///
    /// Port of `FormatMetadataImpl::getChildrenElements()` (FormatMetadata.cpp:114-117 @
    /// v2.5.2).
    pub fn get_children_elements_mut(&mut self) -> &mut Elements {
        &mut self.elements
    }

    /// Merges `rhs` into `self`, as the optimizer does for the metadata of ops it combines.
    ///
    /// The two elements must have the same name. The values are joined with `" + "`. An
    /// attribute of `rhs` with an empty value is skipped; one whose name matches an attribute
    /// of `self` ignoring ASCII case has its value joined to that one's; any other is added.
    /// The children of `rhs` are added after those of `self`.
    ///
    /// Upstream does nothing when `rhs` is `self`; Rust's borrow rules keep that from
    /// happening.
    ///
    /// Port of `FormatMetadataImpl::combine` (FormatMetadata.cpp:140-179 @ v2.5.2).
    pub fn combine(&mut self, rhs: &FormatMetadataImpl) -> Result<()> {
        if self.name != rhs.name {
            return Err(Exception::new(
                "Only FormatMetadata with the same name can be combined.",
            ));
        }

        combine_strings(&mut self.value, &rhs.value);

        // XML attribute names must be unique, so any rhs attributes that use an existing name
        // get merged by combining the value strings. New rhs attributes simply get added.
        for attrib in &rhs.attributes {
            if !attrib.1.is_empty() {
                match self.find_named_attribute(&attrib.0) {
                    Some(index) => combine_strings(&mut self.attributes[index].1, &attrib.1),
                    None => self.attributes.push(attrib.clone()),
                }
            }
        }

        // All child elements for rhs simply get added to the object. Note that the results
        // may need to be cleaned up later if the schema for the given file format does not
        // want more than one element with a given name.
        self.elements.extend(rhs.elements.iter().cloned());
        Ok(())
    }

    /// The index of the first child whose name is `name`, ignoring ASCII case.
    ///
    /// Port of `FormatMetadataImpl::getFirstChildIndex` (FormatMetadata.cpp:205-217 @ v2.5.2),
    /// which returns -1 where this returns `None`.
    pub fn get_first_child_index(&self, name: &[u8]) -> Option<usize> {
        self.elements
            .iter()
            .position(|e| equal_ignoring_case(name, e.get_element_name()))
    }

    /// The index of the first attribute whose name is `name`, ignoring ASCII case.
    ///
    /// Port of `FormatMetadataImpl::findNamedAttribute` (FormatMetadata.cpp:219-231 @
    /// v2.5.2), which returns -1 where this returns `None`.
    fn find_named_attribute(&self, name: &[u8]) -> Option<usize> {
        self.attributes
            .iter()
            .position(|a| equal_ignoring_case(name, &a.0))
    }

    /// Port of the static `FormatMetadataImpl::ValidateElementName` (FormatMetadata.cpp:233-243
    /// @ v2.5.2).
    fn validate_element_name_static(name: &[u8]) -> Result<()> {
        if name.is_empty() {
            return Err(Exception::new(
                "FormatMetadata has to have a non-empty name.",
            ));
        }
        if c_str(name) == METADATA_ROOT {
            // The typo is upstream's, and part of the output (improvement candidate I-15).
            return Err(Exception::new(
                "'ROOT' is reversed for root FormatMetadata elements.",
            ));
        }
        Ok(())
    }

    /// Port of `FormatMetadataImpl::validateElementName` (FormatMetadata.cpp:245-252 @
    /// v2.5.2).
    fn validate_element_name(&self, name: &[u8]) -> Result<()> {
        Self::validate_element_name_static(name)?;
        if c_str(&self.name) == METADATA_ROOT {
            return Err(Exception::new(
                "FormatMetadata 'ROOT' element can't be renamed.",
            ));
        }
        Ok(())
    }

    /// The element's name.
    ///
    /// Port of `FormatMetadataImpl::getElementName` (FormatMetadata.cpp:254-257 @ v2.5.2).
    pub fn get_element_name(&self) -> &[u8] {
        c_str(&self.name)
    }

    /// Renames the element. The name must not be empty or [`METADATA_ROOT`], and a top-level
    /// element can't be renamed.
    ///
    /// Port of `FormatMetadataImpl::setElementName` (FormatMetadata.cpp:259-264 @ v2.5.2).
    pub fn set_element_name(&mut self, name: Option<&[u8]>) -> Result<()> {
        let name = arg(name);
        self.validate_element_name(name)?;
        self.name = name.to_vec();
        Ok(())
    }

    /// The element's value.
    ///
    /// Port of `FormatMetadataImpl::getElementValue` (FormatMetadata.cpp:266-269 @ v2.5.2).
    pub fn get_element_value(&self) -> &[u8] {
        c_str(&self.value)
    }

    /// Sets the element's value. A top-level element can't have one.
    ///
    /// Port of `FormatMetadataImpl::setElementValue` (FormatMetadata.cpp:271-278 @ v2.5.2).
    pub fn set_element_value(&mut self, value: Option<&[u8]>) -> Result<()> {
        if self.name == METADATA_ROOT {
            return Err(Exception::new("FormatMetadata 'ROOT' can't have a value."));
        }
        self.value = arg(value).to_vec();
        Ok(())
    }

    /// The number of attributes.
    ///
    /// Port of `FormatMetadataImpl::getNumAttributes` (FormatMetadata.cpp:280-283 @ v2.5.2).
    pub fn get_num_attributes(&self) -> i32 {
        self.attributes.len() as i32
    }

    /// The name of attribute `i`, or `""` if there is none.
    ///
    /// Port of `FormatMetadataImpl::getAttributeName` (FormatMetadata.cpp:285-292 @ v2.5.2).
    pub fn get_attribute_name(&self, i: i32) -> &[u8] {
        if i >= 0 && i < self.get_num_attributes() {
            return c_str(&self.attributes[i as usize].0);
        }
        b""
    }

    /// The value of attribute `i`, or `""` if there is none.
    ///
    /// Port of `FormatMetadataImpl::getAttributeValue(int)` (FormatMetadata.cpp:294-301 @
    /// v2.5.2).
    pub fn get_attribute_value(&self, i: i32) -> &[u8] {
        if i >= 0 && i < self.get_num_attributes() {
            return c_str(&self.attributes[i as usize].1);
        }
        b""
    }

    /// The value of the first attribute named `name` ignoring ASCII case, or `""` if there is
    /// none.
    ///
    /// Port of `FormatMetadataImpl::getAttributeValue(const char *)` (FormatMetadata.cpp:
    /// 303-316 @ v2.5.2).
    pub fn get_attribute_value_by_name(&self, name: Option<&[u8]>) -> &[u8] {
        c_str(self.get_attribute_value_string(name))
    }

    /// The whole value of the first attribute named `name` ignoring ASCII case, or `""` if
    /// there is none.
    ///
    /// Port of `FormatMetadataImpl::getAttributeValueString` (FormatMetadata.cpp:318-332 @
    /// v2.5.2).
    pub fn get_attribute_value_string(&self, name: Option<&[u8]>) -> &[u8] {
        let name = arg(name);
        if !name.is_empty()
            && let Some(attrib) = self
                .attributes
                .iter()
                .find(|a| equal_ignoring_case(name, &a.0))
        {
            return &attrib.1;
        }
        b""
    }

    /// Adds an attribute, or replaces the value of the attribute whose name is exactly `name`.
    /// The name must not be null or empty.
    ///
    /// Port of `FormatMetadataImpl::addAttribute(const char *, const char *)`
    /// (FormatMetadata.cpp:334-342 @ v2.5.2).
    pub fn add_attribute(&mut self, name: Option<&[u8]>, value: Option<&[u8]>) -> Result<()> {
        let name = arg(name);
        if name.is_empty() {
            return Err(Exception::new("Attribute must have a non-empty name."));
        }
        self.add_attribute_pair((name.to_vec(), arg(value).to_vec()));
        Ok(())
    }

    /// The number of child elements.
    ///
    /// Port of `FormatMetadataImpl::getNumChildrenElements` (FormatMetadata.cpp:344-347 @
    /// v2.5.2).
    pub fn get_num_children_elements(&self) -> i32 {
        self.elements.len() as i32
    }

    /// Child element `i`.
    ///
    /// Port of `FormatMetadataImpl::getChildElement(int) const` (FormatMetadata.cpp:358-365 @
    /// v2.5.2).
    pub fn get_child_element(&self, i: i32) -> Result<&FormatMetadataImpl> {
        if i >= 0 && i < self.get_num_children_elements() {
            return Ok(&self.elements[i as usize]);
        }
        Err(Exception::new("Invalid index for metadata object."))
    }

    /// Child element `i`, to change.
    ///
    /// Port of `FormatMetadataImpl::getChildElement(int)` (FormatMetadata.cpp:349-356 @
    /// v2.5.2).
    pub fn get_child_element_mut(&mut self, i: i32) -> Result<&mut FormatMetadataImpl> {
        if i >= 0 && i < self.get_num_children_elements() {
            return Ok(&mut self.elements[i as usize]);
        }
        Err(Exception::new("Invalid index for metadata object."))
    }

    /// Adds a child element after the others. The name must not be null, empty or
    /// [`METADATA_ROOT`]; the value may be empty.
    ///
    /// Port of `FormatMetadataImpl::addChildElement` (FormatMetadata.cpp:367-372 @ v2.5.2).
    pub fn add_child_element(&mut self, name: Option<&[u8]>, value: Option<&[u8]>) -> Result<()> {
        let name = arg(name);
        Self::validate_element_name_static(name)?;
        self.elements.push(Self::new(name, arg(value))?);
        Ok(())
    }

    /// Removes the value, the attributes and the child elements, and keeps the name.
    ///
    /// Port of `FormatMetadataImpl::clear` (FormatMetadata.cpp:374-379 @ v2.5.2).
    pub fn clear(&mut self) {
        self.attributes.clear();
        self.value.clear();
        self.elements.clear();
    }

    /// The value of the `name` attribute (found ignoring ASCII case), or `""`: the name of a
    /// CLF/CTF process node, or the `name` key of a transform in a config.
    ///
    /// Port of `FormatMetadataImpl::getName` (FormatMetadata.cpp:391-394 @ v2.5.2).
    pub fn get_name(&self) -> &[u8] {
        c_str(self.get_attribute_value_string(Some(METADATA_NAME)))
    }

    /// Sets the attribute named exactly `name`.
    ///
    /// Port of `FormatMetadataImpl::setName` (FormatMetadata.cpp:396-400 @ v2.5.2).
    pub fn set_name(&mut self, name: Option<&[u8]>) {
        self.add_attribute_pair((METADATA_NAME.to_vec(), arg(name).to_vec()));
    }

    /// The value of the `id` attribute (found ignoring ASCII case), or `""`: the id of a
    /// CLF/CTF process node, or of a CDL `ColorCorrection`.
    ///
    /// Port of `FormatMetadataImpl::getID` (FormatMetadata.cpp:402-405 @ v2.5.2).
    pub fn get_id(&self) -> &[u8] {
        c_str(self.get_attribute_value_string(Some(METADATA_ID)))
    }

    /// Sets the attribute named exactly `id`.
    ///
    /// Port of `FormatMetadataImpl::setID` (FormatMetadata.cpp:407-411 @ v2.5.2).
    pub fn set_id(&mut self, id: Option<&[u8]>) {
        self.add_attribute_pair((METADATA_ID.to_vec(), arg(id).to_vec()));
    }

    /// Writes the element as XML-like text: `<name attr="value" ...>value children</name>`,
    /// with nothing escaped. This is the Python binding's `repr()`.
    ///
    /// Port of `operator<<(std::ostream &, const FormatMetadata &)` (FormatMetadata.cpp:26-51
    /// @ v2.5.2).
    pub fn write_to(&self, os: &mut Vec<u8>) {
        let name = self.get_element_name();
        os.push(b'<');
        os.extend_from_slice(name);
        for i in 0..self.get_num_attributes() {
            os.push(b' ');
            os.extend_from_slice(self.get_attribute_name(i));
            os.extend_from_slice(b"=\"");
            os.extend_from_slice(self.get_attribute_value(i));
            os.push(b'"');
        }
        os.push(b'>');
        os.extend_from_slice(self.get_element_value());
        for child in &self.elements {
            child.write_to(os);
        }
        os.extend_from_slice(b"</");
        os.extend_from_slice(name);
        os.push(b'>');
    }
}

#[cfg(test)]
#[path = "format_metadata_tests.rs"]
mod tests;
