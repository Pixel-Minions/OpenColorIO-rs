// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The view transform: a port of `src/OpenColorIO/ViewTransform.cpp` @ v2.5.2.

use std::collections::BTreeMap;
use std::fmt;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::Result;
use ocio_ops::open_color_types::{ReferenceSpaceType, ViewTransformDirection};
use ocio_ops::utils::string_utils::c_str;

use crate::color_space::{get_interchange_attribute, set_interchange_attribute};
use crate::tokens_manager::TokensManager;
use crate::transform::{Transform, put_c_str};

/// The interchange attributes a view transform knows.
///
/// Port of `knownInterchangeNames` (src/OpenColorIO/ViewTransform.cpp:10-14 @ v2.5.2).
const KNOWN_INTERCHANGE_NAMES: [&[u8]; 1] = [b"amf_transform_ids"];

/// The reference space's name in a view transform's text: `scene` or `display`.
///
/// Port of `ReferenceSpaceTypeToString` (ViewTransform.cpp:253-263 @ v2.5.2). Its "Unknown
/// reference type" can't happen with an enum.
fn reference_space_type_to_string(reference: ReferenceSpaceType) -> &'static str {
    match reference {
        ReferenceSpaceType::Scene => "scene",
        ReferenceSpaceType::Display => "display",
    }
}

/// A view transform: how a view converts between the scene and the display reference spaces
/// (or within one of them), with its transforms to and from its reference space.
///
/// A copy is upstream's `createEditableCopy`: it copies the transforms too.
///
/// Port of `ViewTransform` and its `Impl` (include/OpenColorIO/OpenColorIO.h:2550-2626,
/// src/OpenColorIO/ViewTransform.cpp:17-245 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct ViewTransform {
    /// `m_name`.
    name: Vec<u8>,
    /// `m_family`.
    family: Vec<u8>,
    /// `m_description`.
    description: Vec<u8>,
    /// `m_referenceSpaceType`.
    reference_space_type: ReferenceSpaceType,
    /// `m_interchangeAttribs`.
    interchange_attribs: BTreeMap<Vec<u8>, Vec<u8>>,
    /// `m_toRefTransform`.
    to_ref_transform: Option<Transform>,
    /// `m_fromRefTransform`.
    from_ref_transform: Option<Transform>,
    /// `m_categories`.
    categories: TokensManager,
}

impl ViewTransform {
    /// A view transform of the reference space `reference_space`, without a name or
    /// transforms. (Upstream has no other constructor.)
    ///
    /// Port of `ViewTransform::Create` and `Impl::Impl` (ViewTransform.cpp:17-70 @ v2.5.2).
    #[doc(alias = "Create")]
    pub fn new(reference_space: ReferenceSpaceType) -> ViewTransform {
        ViewTransform {
            name: Vec::new(),
            family: Vec::new(),
            description: Vec::new(),
            reference_space_type: reference_space,
            interchange_attribs: BTreeMap::new(),
            to_ref_transform: None,
            from_ref_transform: None,
            categories: TokensManager::default(),
        }
    }

    /// Port of `ViewTransform::getName` (ViewTransform.cpp:95-98 @ v2.5.2).
    #[doc(alias = "getName")]
    pub fn name(&self) -> &[u8] {
        &self.name
    }

    /// Sets the name, up to its first NUL (a C string).
    ///
    /// Port of `ViewTransform::setName` (ViewTransform.cpp:100-103 @ v2.5.2).
    #[doc(alias = "setName")]
    pub fn set_name(&mut self, name: impl AsRef<[u8]>) {
        self.name = c_str(name.as_ref()).to_vec();
    }

    /// Port of `ViewTransform::getFamily` (ViewTransform.cpp:105-108 @ v2.5.2).
    #[doc(alias = "getFamily")]
    pub fn family(&self) -> &[u8] {
        &self.family
    }

    /// Port of `ViewTransform::setFamily` (ViewTransform.cpp:110-113 @ v2.5.2).
    #[doc(alias = "setFamily")]
    pub fn set_family(&mut self, family: impl AsRef<[u8]>) {
        self.family = c_str(family.as_ref()).to_vec();
    }

    /// Port of `ViewTransform::getDescription` (ViewTransform.cpp:115-118 @ v2.5.2).
    #[doc(alias = "getDescription")]
    pub fn description(&self) -> &[u8] {
        &self.description
    }

    /// Port of `ViewTransform::setDescription` (ViewTransform.cpp:120-123 @ v2.5.2).
    #[doc(alias = "setDescription")]
    pub fn set_description(&mut self, description: impl AsRef<[u8]>) {
        self.description = c_str(description.as_ref()).to_vec();
    }

    /// The value of the interchange attribute `attr_name` (`amf_transform_ids`, ignoring
    /// case), or empty; "Unknown attribute name '<name>'." for any other name.
    ///
    /// Port of `ViewTransform::getInterchangeAttribute` (ViewTransform.cpp:125-146 @ v2.5.2).
    #[doc(alias = "getInterchangeAttribute")]
    pub fn interchange_attribute(&self, attr_name: impl AsRef<[u8]>) -> Result<&[u8]> {
        get_interchange_attribute(
            &KNOWN_INTERCHANGE_NAMES,
            &self.interchange_attribs,
            attr_name.as_ref(),
        )
    }

    /// Sets the interchange attribute `attr_name`; an empty value removes it.
    ///
    /// Port of `ViewTransform::setInterchangeAttribute` (ViewTransform.cpp:148-173 @ v2.5.2).
    #[doc(alias = "setInterchangeAttribute")]
    pub fn set_interchange_attribute(
        &mut self,
        attr_name: impl AsRef<[u8]>,
        value: impl AsRef<[u8]>,
    ) -> Result<()> {
        set_interchange_attribute(
            &KNOWN_INTERCHANGE_NAMES,
            &mut self.interchange_attribs,
            attr_name.as_ref(),
            value.as_ref(),
        )
    }

    /// The interchange attributes that are set, by name.
    ///
    /// Port of `ViewTransform::getInterchangeAttributes` (ViewTransform.cpp:175-178 @ v2.5.2).
    #[doc(alias = "getInterchangeAttributes")]
    pub fn interchange_attributes(&self) -> &BTreeMap<Vec<u8>, Vec<u8>> {
        &self.interchange_attribs
    }

    /// Whether a category matches `category` ignoring case and the surrounding whitespace.
    ///
    /// Port of `ViewTransform::hasCategory` (ViewTransform.cpp:181-184 @ v2.5.2).
    #[doc(alias = "hasCategory")]
    pub fn has_category(&self, category: impl AsRef<[u8]>) -> bool {
        self.categories.has_token(category.as_ref())
    }

    /// Adds `category`, trimmed, unless it is empty or already there.
    ///
    /// Port of `ViewTransform::addCategory` (ViewTransform.cpp:186-189 @ v2.5.2).
    #[doc(alias = "addCategory")]
    pub fn add_category(&mut self, category: impl AsRef<[u8]>) {
        self.categories.add_token(category.as_ref());
    }

    /// Port of `ViewTransform::removeCategory` (ViewTransform.cpp:190-193 @ v2.5.2).
    #[doc(alias = "removeCategory")]
    pub fn remove_category(&mut self, category: impl AsRef<[u8]>) {
        self.categories.remove_token(category.as_ref());
    }

    /// Port of `ViewTransform::getNumCategories` (ViewTransform.cpp:195-198 @ v2.5.2).
    #[doc(alias = "getNumCategories")]
    pub fn num_categories(&self) -> i32 {
        self.categories.num_tokens()
    }

    /// The category at `index`, or `None` (upstream's null pointer) outside the list.
    ///
    /// Port of `ViewTransform::getCategory` (ViewTransform.cpp:200-203 @ v2.5.2).
    #[doc(alias = "getCategory")]
    pub fn category(&self, index: i32) -> Option<&[u8]> {
        self.categories.token(index)
    }

    /// Port of `ViewTransform::clearCategories` (ViewTransform.cpp:205-208 @ v2.5.2).
    #[doc(alias = "clearCategories")]
    pub fn clear_categories(&mut self) {
        self.categories.clear_tokens();
    }

    /// Port of `ViewTransform::getReferenceSpaceType` (ViewTransform.cpp:211-214 @ v2.5.2).
    #[doc(alias = "getReferenceSpaceType")]
    pub fn reference_space_type(&self) -> ReferenceSpaceType {
        self.reference_space_type
    }

    /// The transform to the reference space or from it, if it is set.
    ///
    /// Port of `ViewTransform::getTransform` (ViewTransform.cpp:216-226 @ v2.5.2).
    #[doc(alias = "getTransform")]
    pub fn transform(&self, dir: ViewTransformDirection) -> Option<&Transform> {
        match dir {
            ViewTransformDirection::ToReference => self.to_ref_transform.as_ref(),
            ViewTransformDirection::FromReference => self.from_ref_transform.as_ref(),
        }
    }

    /// Sets a copy of `transform` as the transform to the reference space or from it; `None`
    /// (upstream's null pointer) removes it.
    ///
    /// Port of `ViewTransform::setTransform` (ViewTransform.cpp:228-245 @ v2.5.2).
    #[doc(alias = "setTransform")]
    pub fn set_transform(&mut self, transform: Option<&Transform>, dir: ViewTransformDirection) {
        let transform_copy = transform.cloned();

        match dir {
            ViewTransformDirection::ToReference => self.to_ref_transform = transform_copy,
            ViewTransformDirection::FromReference => self.from_ref_transform = transform_copy,
        }
    }

    /// Writes the view transform's text to `os`; the transforms on the same stream. The
    /// categories are not printed.
    ///
    /// Port of `operator<<(std::ostream &, const ViewTransform &)` (ViewTransform.cpp:266-295
    /// @ v2.5.2).
    pub(crate) fn write_text(&self, os: &mut OStringStream) {
        os.put_str("<ViewTransform ");
        os.put_str("name=");
        put_c_str(os, self.name());
        os.put_str(", ");
        os.put_str("family=");
        put_c_str(os, self.family());
        os.put_str(", ");
        os.put_str("referenceSpaceType=");
        os.put_str(reference_space_type_to_string(self.reference_space_type()));
        if !self.description().is_empty() {
            os.put_str(", description=");
            put_c_str(os, self.description());
        }
        for (name, value) in self.interchange_attributes() {
            os.put_str(", ");
            put_c_str(os, name);
            os.put_str("=");
            put_c_str(os, value);
        }
        if let Some(to_ref) = self.transform(ViewTransformDirection::ToReference) {
            os.put_str(",\n    ");
            put_c_str(os, self.name());
            os.put_str(" --> Reference");
            os.put_str("\n        ");
            to_ref.write_text(os);
        }
        if let Some(from_ref) = self.transform(ViewTransformDirection::FromReference) {
            os.put_str(",\n    Reference --> ");
            put_c_str(os, self.name());
            os.put_str("\n        ");
            from_ref.write_text(os);
        }
        os.put_str(">");
    }
}

impl ViewTransform {
    /// The view transform's text, as Python's `repr()` prints it, in bytes: names and
    /// descriptions that aren't UTF-8 pass through unchanged, where `Display` replaces them.
    ///
    /// Port of `operator<<(std::ostream &, const ViewTransform &)` (ViewTransform.cpp:266-295
    /// @ v2.5.2), on a new stream.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_text(&mut os);
        os.into_bytes()
    }
}

impl fmt::Display for ViewTransform {
    /// [`ViewTransform::to_bytes`], what isn't UTF-8 replaced by U+FFFD.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&String::from_utf8_lossy(&self.to_bytes()))
    }
}

#[cfg(test)]
#[path = "view_transform_tests.rs"]
mod tests;
