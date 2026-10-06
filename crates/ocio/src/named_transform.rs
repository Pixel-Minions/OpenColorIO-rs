// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The named transform: a port of `src/OpenColorIO/NamedTransform.h` and
//! `NamedTransform.cpp` @ v2.5.2. `NamedTransform::GetTransform` and the `GetTransform` of a
//! source and a destination come with the color space transform's op builder (WP 3.2a).

use std::fmt;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::open_color_types::TransformDirection;
use ocio_ops::platform::strcasecmp;
use ocio_ops::utils::string_utils::{self, c_str};

use crate::tokens_manager::TokensManager;
use crate::transform::{Transform, put_c_str};

/// A named transform: a transform a config names, usable in place of a color space, with
/// aliases, a family, a description, categories and an encoding, and its forward and inverse
/// transforms (either may be missing).
///
/// A copy is upstream's `createEditableCopy`: it copies the transforms too.
///
/// Port of `NamedTransform` (include/OpenColorIO/OpenColorIO.h:2460-2531 @ v2.5.2) and
/// `NamedTransformImpl` (src/OpenColorIO/NamedTransform.h:17-62,
/// src/OpenColorIO/NamedTransform.cpp:12-251 @ v2.5.2), its only implementation.
#[derive(Debug, Clone, Default)]
pub struct NamedTransform {
    /// `m_name`.
    name: Vec<u8>,
    /// `m_aliases`.
    aliases: Vec<Vec<u8>>,
    /// `m_forwardTransform`.
    forward_transform: Option<Transform>,
    /// `m_inverseTransform`.
    inverse_transform: Option<Transform>,
    /// `m_family`.
    family: Vec<u8>,
    /// `m_description`.
    description: Vec<u8>,
    /// `m_categories`.
    categories: TokensManager,
    /// `m_encoding`.
    encoding: Vec<u8>,
}

impl NamedTransform {
    /// A named transform without a name or transforms.
    ///
    /// Port of `NamedTransform::Create` (NamedTransform.cpp:12-15 @ v2.5.2).
    #[doc(alias = "Create")]
    pub fn new() -> NamedTransform {
        NamedTransform::default()
    }

    /// Port of `NamedTransformImpl::getName` (NamedTransform.cpp:42-45 @ v2.5.2).
    #[doc(alias = "getName")]
    pub fn name(&self) -> &[u8] {
        &self.name
    }

    /// Sets the name, up to its first NUL; an alias that matches it (ignoring case) is removed.
    ///
    /// Port of `NamedTransformImpl::setName` (NamedTransform.cpp:47-52 @ v2.5.2).
    #[doc(alias = "setName")]
    pub fn set_name(&mut self, name: impl AsRef<[u8]>) {
        self.name = c_str(name.as_ref()).to_vec();
        // Name can no longer be an alias.
        string_utils::remove(&mut self.aliases, &self.name);
    }

    /// Port of `NamedTransformImpl::getNumAliases` (NamedTransform.cpp:54-57 @ v2.5.2).
    #[doc(alias = "getNumAliases")]
    pub fn num_aliases(&self) -> usize {
        self.aliases.len()
    }

    /// The alias at `idx`, or empty outside the list.
    ///
    /// Port of `NamedTransformImpl::getAlias` (NamedTransform.cpp:59-66 @ v2.5.2).
    #[doc(alias = "getAlias")]
    pub fn alias(&self, idx: usize) -> &[u8] {
        self.aliases.get(idx).map_or(&b""[..], Vec::as_slice)
    }

    /// Whether an alias matches `alias` (up to its first NUL, not empty) ignoring case.
    ///
    /// Port of `NamedTransformImpl::hasAlias` (NamedTransform.cpp:68-79 @ v2.5.2).
    #[doc(alias = "hasAlias")]
    pub fn has_alias(&self, alias: impl AsRef<[u8]>) -> bool {
        let alias = c_str(alias.as_ref());
        if alias.is_empty() {
            return false;
        }
        self.aliases.iter().any(|a| strcasecmp(a, alias).is_eq())
    }

    /// Adds `alias` (up to its first NUL), unless it is empty, the name or already an alias,
    /// ignoring case.
    ///
    /// Port of `NamedTransformImpl::addAlias` (NamedTransform.cpp:81-93 @ v2.5.2).
    #[doc(alias = "addAlias")]
    pub fn add_alias(&mut self, alias: impl AsRef<[u8]>) {
        let alias = c_str(alias.as_ref());
        if !alias.is_empty()
            && !string_utils::compare(alias, &self.name)
            && !string_utils::contain(&self.aliases, alias)
        {
            self.aliases.push(alias.to_vec());
        }
    }

    /// Removes the first alias that matches `name` ignoring case.
    ///
    /// Port of `NamedTransformImpl::removeAlias` (NamedTransform.cpp:95-102 @ v2.5.2).
    #[doc(alias = "removeAlias")]
    pub fn remove_alias(&mut self, name: impl AsRef<[u8]>) {
        let alias = c_str(name.as_ref());
        if !alias.is_empty() {
            string_utils::remove(&mut self.aliases, alias);
        }
    }

    /// Port of `NamedTransformImpl::clearAliases` (NamedTransform.cpp:104-107 @ v2.5.2).
    #[doc(alias = "clearAliases")]
    pub fn clear_aliases(&mut self) {
        self.aliases.clear();
    }

    /// Port of `NamedTransformImpl::getFamily` (NamedTransform.cpp:109-112 @ v2.5.2).
    #[doc(alias = "getFamily")]
    pub fn family(&self) -> &[u8] {
        &self.family
    }

    /// Port of `NamedTransformImpl::setFamily` (NamedTransform.cpp:114-117 @ v2.5.2).
    #[doc(alias = "setFamily")]
    pub fn set_family(&mut self, family: impl AsRef<[u8]>) {
        self.family = c_str(family.as_ref()).to_vec();
    }

    /// Port of `NamedTransformImpl::getDescription` (NamedTransform.cpp:119-122 @ v2.5.2).
    #[doc(alias = "getDescription")]
    pub fn description(&self) -> &[u8] {
        &self.description
    }

    /// Port of `NamedTransformImpl::setDescription` (NamedTransform.cpp:124-127 @ v2.5.2).
    #[doc(alias = "setDescription")]
    pub fn set_description(&mut self, description: impl AsRef<[u8]>) {
        self.description = c_str(description.as_ref()).to_vec();
    }

    /// Whether a category matches `category` ignoring case and the surrounding whitespace.
    ///
    /// Port of `NamedTransformImpl::hasCategory` (NamedTransform.cpp:129-132 @ v2.5.2).
    #[doc(alias = "hasCategory")]
    pub fn has_category(&self, category: impl AsRef<[u8]>) -> bool {
        self.categories.has_token(category.as_ref())
    }

    /// Adds `category`, trimmed, unless it is empty or already there.
    ///
    /// Port of `NamedTransformImpl::addCategory` (NamedTransform.cpp:134-137 @ v2.5.2).
    #[doc(alias = "addCategory")]
    pub fn add_category(&mut self, category: impl AsRef<[u8]>) {
        self.categories.add_token(category.as_ref());
    }

    /// Port of `NamedTransformImpl::removeCategory` (NamedTransform.cpp:139-142 @ v2.5.2).
    #[doc(alias = "removeCategory")]
    pub fn remove_category(&mut self, category: impl AsRef<[u8]>) {
        self.categories.remove_token(category.as_ref());
    }

    /// Port of `NamedTransformImpl::getNumCategories` (NamedTransform.cpp:144-147 @ v2.5.2).
    #[doc(alias = "getNumCategories")]
    pub fn num_categories(&self) -> i32 {
        self.categories.num_tokens()
    }

    /// The category at `index`, or `None` (upstream's null pointer) outside the list.
    ///
    /// Port of `NamedTransformImpl::getCategory` (NamedTransform.cpp:149-152 @ v2.5.2).
    #[doc(alias = "getCategory")]
    pub fn category(&self, index: i32) -> Option<&[u8]> {
        self.categories.token(index)
    }

    /// Port of `NamedTransformImpl::clearCategories` (NamedTransform.cpp:154-157 @ v2.5.2).
    #[doc(alias = "clearCategories")]
    pub fn clear_categories(&mut self) {
        self.categories.clear_tokens();
    }

    /// Port of `NamedTransformImpl::getEncoding` (NamedTransform.cpp:159-162 @ v2.5.2).
    #[doc(alias = "getEncoding")]
    pub fn encoding(&self) -> &[u8] {
        &self.encoding
    }

    /// Port of `NamedTransformImpl::setEncoding` (NamedTransform.cpp:164-167 @ v2.5.2).
    #[doc(alias = "setEncoding")]
    pub fn set_encoding(&mut self, encoding: impl AsRef<[u8]>) {
        self.encoding = c_str(encoding.as_ref()).to_vec();
    }

    /// The forward or the inverse transform, if it is set. Upstream's "Named transform:
    /// Unspecified TransformDirection." can't happen with a [`TransformDirection`].
    ///
    /// Port of `NamedTransformImpl::getTransform` (NamedTransform.cpp:169-180 @ v2.5.2).
    #[doc(alias = "getTransform")]
    pub fn transform(&self, dir: TransformDirection) -> Option<&Transform> {
        match dir {
            TransformDirection::Forward => self.forward_transform.as_ref(),
            TransformDirection::Inverse => self.inverse_transform.as_ref(),
        }
    }

    /// Sets a copy of `transform` as the forward or the inverse transform; `None` (upstream's
    /// null pointer) removes it.
    ///
    /// Port of `NamedTransformImpl::setTransform` (NamedTransform.cpp:223-251 @ v2.5.2).
    #[doc(alias = "setTransform")]
    pub fn set_transform(&mut self, transform: Option<&Transform>, dir: TransformDirection) {
        let transform = transform.cloned();
        match dir {
            TransformDirection::Forward => self.forward_transform = transform,
            TransformDirection::Inverse => self.inverse_transform = transform,
        }
    }

    /// Writes the named transform's text to `os`; the transforms on the same stream.
    ///
    /// Port of `operator<<(std::ostream &, const NamedTransform &)` (NamedTransform.cpp:
    /// 253-308 @ v2.5.2).
    pub(crate) fn write_text(&self, os: &mut OStringStream) {
        os.put_str("<NamedTransform ");
        os.put_str("name=");
        put_c_str(os, self.name());
        let num_aliases = self.num_aliases();
        if num_aliases == 1 {
            os.put_str(", alias= ");
            put_c_str(os, self.alias(0));
        } else if num_aliases > 1 {
            os.put_str(", aliases=[");
            put_c_str(os, self.alias(0));
            for aidx in 1..num_aliases {
                os.put_str(", ");
                put_c_str(os, self.alias(aidx));
            }
            os.put_str("]");
        }
        if !self.family().is_empty() {
            os.put_str(", family=");
            put_c_str(os, self.family());
        }
        if self.num_categories() != 0 {
            os.put_str(", categories=[");
            let categories: Vec<Vec<u8>> = (0..self.num_categories())
                .map(|i| self.category(i).expect("an index inside the list").to_vec())
                .collect();
            put_c_str(os, &string_utils::join(&categories, b','));
            os.put_str("]");
        }
        if !self.description().is_empty() {
            os.put_str(", description=");
            put_c_str(os, self.description());
        }
        if !self.encoding().is_empty() {
            os.put_str(", encoding=");
            put_c_str(os, self.encoding());
        }
        if let Some(forward) = self.transform(TransformDirection::Forward) {
            os.put_str(",\n    forward=");
            os.put_str("\n        ");
            forward.write_text(os);
        }
        if let Some(inverse) = self.transform(TransformDirection::Inverse) {
            os.put_str(",\n    inverse=");
            os.put_str("\n        ");
            inverse.write_text(os);
        }
        os.put_str(">");
    }
}

impl NamedTransform {
    /// The named transform's text, as Python's `repr()` prints it, in bytes: names and
    /// descriptions that aren't UTF-8 pass through unchanged, where `Display` replaces them.
    ///
    /// Port of `operator<<(std::ostream &, const NamedTransform &)` (NamedTransform.cpp:253-308
    /// @ v2.5.2), on a new stream.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_text(&mut os);
        os.into_bytes()
    }
}

impl fmt::Display for NamedTransform {
    /// [`NamedTransform::to_bytes`], what isn't UTF-8 replaced by U+FFFD.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&String::from_utf8_lossy(&self.to_bytes()))
    }
}

#[cfg(test)]
#[path = "named_transform_tests.rs"]
mod tests;
