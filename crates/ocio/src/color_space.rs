// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The color space: a port of `src/OpenColorIO/ColorSpace.cpp` @ v2.5.2.

use std::collections::BTreeMap;
use std::fmt;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::open_color_types::{
    Allocation, BitDepth, ColorSpaceDirection, ReferenceSpaceType, allocation_to_string,
    bit_depth_to_string,
};
use ocio_ops::parse_utils::bool_to_string;
use ocio_ops::platform::strcasecmp;
use ocio_ops::utils::string_utils::{self, c_str};

use crate::tokens_manager::TokensManager;
use crate::transform::{Transform, put_c_str};

/// The interchange attributes a color space knows, in their canonical spelling.
///
/// Port of `knownInterchangeNames` (src/OpenColorIO/ColorSpace.cpp:18-21 @ v2.5.2).
const KNOWN_INTERCHANGE_NAMES: [&[u8]; 2] = [b"amf_transform_ids", b"icc_profile_name"];

/// The value of the known interchange attribute `attr_name` (matched ignoring case) in
/// `attribs`, or empty; "Unknown attribute name '<name>'." for any other name.
///
/// Port of `ColorSpace::getInterchangeAttribute` (ColorSpace.cpp:297-318 @ v2.5.2), which
/// `Look` and `ViewTransform` repeat with their own known names.
pub(crate) fn get_interchange_attribute<'a>(
    known: &[&[u8]],
    attribs: &'a BTreeMap<Vec<u8>, Vec<u8>>,
    attr_name: &[u8],
) -> Result<&'a [u8]> {
    let name = c_str(attr_name);

    for key in known {
        // do case-insensitive comparison.
        if string_utils::compare(key, name) {
            return Ok(attribs.get(*key).map_or(&b""[..], Vec::as_slice));
        }
    }

    Err(unknown_attribute_name(name))
}

/// Sets the known interchange attribute `attr_name` (matched ignoring case, stored under its
/// canonical spelling) to `value` (up to its first NUL); an empty value removes it.
/// "Unknown attribute name '<name>'." for any other name.
///
/// Port of `ColorSpace::setInterchangeAttribute` (ColorSpace.cpp:320-345 @ v2.5.2), which
/// `Look` and `ViewTransform` repeat with their own known names. Its null value, which
/// removes the attribute, is an empty slice here.
pub(crate) fn set_interchange_attribute(
    known: &[&[u8]],
    attribs: &mut BTreeMap<Vec<u8>, Vec<u8>>,
    attr_name: &[u8],
    value: &[u8],
) -> Result<()> {
    let name = c_str(attr_name);
    let value = c_str(value);

    for key in known {
        // Do case-insensitive comparison.
        if string_utils::compare(key, name) {
            // use key instead of name for storing in correct capitalization.
            if value.is_empty() {
                attribs.remove(*key);
            } else {
                attribs.insert(key.to_vec(), value.to_vec());
            }
            return Ok(());
        }
    }

    Err(unknown_attribute_name(name))
}

fn unknown_attribute_name(name: &[u8]) -> Exception {
    Exception::new([b"Unknown attribute name '".as_slice(), name, b"'."].concat())
}

/// An InteropID message: `before`, the ID's bytes, `after`.
fn interop_error(before: &str, id: &[u8], after: &str) -> Exception {
    Exception::new([before.as_bytes(), id, after.as_bytes()].concat())
}

/// A color space: a name with aliases, how it relates to its reference space (the transforms
/// to and from it) and what describes it (family, equality group, description, encoding,
/// categories, bit depth, whether it holds data, the GPU allocation, the interop ID and
/// interchange attributes).
///
/// A copy is upstream's `createEditableCopy`: it copies the transforms too.
///
/// Port of `ColorSpace` and its `Impl` (include/OpenColorIO/OpenColorIO.h:1984-2218,
/// src/OpenColorIO/ColorSpace.cpp:27-491 @ v2.5.2). Upstream's `m_toRefSpecified` and
/// `m_fromRefSpecified` are never read, and left out.
#[derive(Debug, Clone)]
pub struct ColorSpace {
    /// `m_name`.
    name: Vec<u8>,
    /// `m_family`.
    family: Vec<u8>,
    /// `m_equalityGroup`.
    equality_group: Vec<u8>,
    /// `m_description`.
    description: Vec<u8>,
    /// `m_encoding`.
    encoding: Vec<u8>,
    /// `m_interopID`.
    interop_id: Vec<u8>,
    /// `m_aliases`.
    aliases: Vec<Vec<u8>>,
    /// `m_interchangeAttribs`.
    interchange_attribs: BTreeMap<Vec<u8>, Vec<u8>>,
    /// `m_bitDepth`.
    bit_depth: BitDepth,
    /// `m_isData`.
    is_data: bool,
    /// `m_referenceSpaceType`.
    reference_space_type: ReferenceSpaceType,
    /// `m_allocation`.
    allocation: Allocation,
    /// `m_allocationVars`.
    allocation_vars: Vec<f32>,
    /// `m_toRefTransform`.
    to_ref_transform: Option<Transform>,
    /// `m_fromRefTransform`.
    from_ref_transform: Option<Transform>,
    /// `m_categories`.
    categories: TokensManager,
}

impl Default for ColorSpace {
    fn default() -> Self {
        Self::new()
    }
}

impl ColorSpace {
    /// A color space of the scene reference space, without a name or transforms.
    ///
    /// Port of `ColorSpace::Create()` (ColorSpace.cpp:103-106 @ v2.5.2).
    #[doc(alias = "Create")]
    pub fn new() -> ColorSpace {
        ColorSpace::with_reference_space(ReferenceSpaceType::Scene)
    }

    /// A color space of the reference space `reference_space`, without a name or transforms.
    ///
    /// Port of `ColorSpace::Create(ReferenceSpaceType)` and `Impl::Impl` (ColorSpace.cpp:
    /// 27-62, 113-117 @ v2.5.2).
    #[doc(alias = "Create")]
    pub fn with_reference_space(reference_space: ReferenceSpaceType) -> ColorSpace {
        ColorSpace {
            name: Vec::new(),
            family: Vec::new(),
            equality_group: Vec::new(),
            description: Vec::new(),
            encoding: Vec::new(),
            interop_id: Vec::new(),
            aliases: Vec::new(),
            interchange_attribs: BTreeMap::new(),
            bit_depth: BitDepth::Unknown,
            is_data: false,
            reference_space_type: reference_space,
            allocation: Allocation::Uniform,
            allocation_vars: Vec::new(),
            to_ref_transform: None,
            from_ref_transform: None,
            categories: TokensManager::default(),
        }
    }

    /// Port of `ColorSpace::getName` (ColorSpace.cpp:137-140 @ v2.5.2).
    #[doc(alias = "getName")]
    pub fn name(&self) -> &[u8] {
        &self.name
    }

    /// Sets the name, up to its first NUL; an alias that matches it (ignoring case) is removed.
    ///
    /// Port of `ColorSpace::setName` (ColorSpace.cpp:142-147 @ v2.5.2).
    #[doc(alias = "setName")]
    pub fn set_name(&mut self, name: impl AsRef<[u8]>) {
        self.name = c_str(name.as_ref()).to_vec();
        // Name can no longer be an alias.
        string_utils::remove(&mut self.aliases, &self.name);
    }

    /// Port of `ColorSpace::getNumAliases` (ColorSpace.cpp:149-152 @ v2.5.2).
    #[doc(alias = "getNumAliases")]
    pub fn num_aliases(&self) -> usize {
        self.aliases.len()
    }

    /// The alias at `idx`, or empty outside the list.
    ///
    /// Port of `ColorSpace::getAlias` (ColorSpace.cpp:154-161 @ v2.5.2).
    #[doc(alias = "getAlias")]
    pub fn alias(&self, idx: usize) -> &[u8] {
        self.aliases.get(idx).map_or(&b""[..], Vec::as_slice)
    }

    /// Whether an alias matches `alias` (up to its first NUL) ignoring case.
    ///
    /// Port of `ColorSpace::hasAlias` (ColorSpace.cpp:163-174 @ v2.5.2).
    #[doc(alias = "hasAlias")]
    pub fn has_alias(&self, alias: impl AsRef<[u8]>) -> bool {
        let alias = alias.as_ref();
        self.aliases.iter().any(|a| strcasecmp(a, alias).is_eq())
    }

    /// Adds `alias` (up to its first NUL), unless it is empty, the name or already an alias,
    /// ignoring case.
    ///
    /// Port of `ColorSpace::addAlias` (ColorSpace.cpp:176-188 @ v2.5.2).
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
    /// Port of `ColorSpace::removeAlias` (ColorSpace.cpp:190-197 @ v2.5.2).
    #[doc(alias = "removeAlias")]
    pub fn remove_alias(&mut self, name: impl AsRef<[u8]>) {
        let alias = c_str(name.as_ref());
        if !alias.is_empty() {
            string_utils::remove(&mut self.aliases, alias);
        }
    }

    /// Port of `ColorSpace::clearAliases` (ColorSpace.cpp:199-202 @ v2.5.2).
    #[doc(alias = "clearAliases")]
    pub fn clear_aliases(&mut self) {
        self.aliases.clear();
    }

    /// Port of `ColorSpace::getFamily` (ColorSpace.cpp:204-207 @ v2.5.2).
    #[doc(alias = "getFamily")]
    pub fn family(&self) -> &[u8] {
        &self.family
    }

    /// Port of `ColorSpace::setFamily` (ColorSpace.cpp:209-212 @ v2.5.2).
    #[doc(alias = "setFamily")]
    pub fn set_family(&mut self, family: impl AsRef<[u8]>) {
        self.family = c_str(family.as_ref()).to_vec();
    }

    /// Port of `ColorSpace::getEqualityGroup` (ColorSpace.cpp:214-217 @ v2.5.2).
    #[doc(alias = "getEqualityGroup")]
    pub fn equality_group(&self) -> &[u8] {
        &self.equality_group
    }

    /// Port of `ColorSpace::setEqualityGroup` (ColorSpace.cpp:219-222 @ v2.5.2).
    #[doc(alias = "setEqualityGroup")]
    pub fn set_equality_group(&mut self, equality_group: impl AsRef<[u8]>) {
        self.equality_group = c_str(equality_group.as_ref()).to_vec();
    }

    /// Port of `ColorSpace::getDescription` (ColorSpace.cpp:224-227 @ v2.5.2).
    #[doc(alias = "getDescription")]
    pub fn description(&self) -> &[u8] {
        &self.description
    }

    /// Port of `ColorSpace::setDescription` (ColorSpace.cpp:229-232 @ v2.5.2).
    #[doc(alias = "setDescription")]
    pub fn set_description(&mut self, description: impl AsRef<[u8]>) {
        self.description = c_str(description.as_ref()).to_vec();
    }

    /// Port of `ColorSpace::getInteropID` (ColorSpace.cpp:234-237 @ v2.5.2).
    #[doc(alias = "getInteropID")]
    pub fn interop_id(&self) -> &[u8] {
        &self.interop_id
    }

    /// Sets the interop ID (up to its first NUL), after checking it: only `0-9`, `a-z` and
    /// `. - _ ~ / * # % ^ + ( ) [ ] | :`, and at most one `:`, with text on both sides. The
    /// messages end with a newline (`std::endl`). An empty ID is always accepted.
    ///
    /// Port of `ColorSpace::setInteropID` (ColorSpace.cpp:239-295 @ v2.5.2).
    #[doc(alias = "setInteropID")]
    pub fn set_interop_id(&mut self, interop_id: impl AsRef<[u8]>) -> Result<()> {
        let id = c_str(interop_id.as_ref());

        if !id.is_empty() {
            // check if it only uses ASCII characters: 0-9, a-z, and the following characters
            // (no spaces): . - _ ~ / * # % ^ + ( ) [ ] |
            let allowed = |c: u8| {
                c.is_ascii_digit()
                    || c.is_ascii_lowercase()
                    || matches!(
                        c,
                        b'.' | b'-'
                            | b'_'
                            | b'~'
                            | b'/'
                            | b'*'
                            | b'#'
                            | b'%'
                            | b'^'
                            | b'+'
                            | b'('
                            | b')'
                            | b'['
                            | b']'
                            | b'|'
                            | b':'
                    )
            };

            if !id.iter().all(|&c| allowed(c)) {
                return Err(interop_error(
                    "InteropID '",
                    id,
                    "' contains invalid characters. Only lowercase a-z, 0-9 \
                     and . - _ ~ / * # % ^ + ( ) [ ] | are allowed.\n",
                ));
            }

            // Check if has a namespace.
            if let Some(pos) = id.iter().position(|&c| c == b':') {
                // Namespace found, split into namespace and color space.
                let ns = &id[..pos];
                let cs = &id[pos + 1..];

                // both should be non-empty
                if ns.is_empty() || cs.is_empty() {
                    return Err(interop_error(
                        "InteropID '",
                        id,
                        "' is not valid. If ':' is used, both the namespace \
                         and the color space parts must be non-empty.\n",
                    ));
                }

                // More than one ':' is an error.
                if cs.contains(&b':') {
                    return Err(interop_error(
                        "ERROR: InteropID '",
                        id,
                        "' is not valid. Only one ':' is allowed to \
                         separate the namespace and the color space.\n",
                    ));
                }
            }
        }

        self.interop_id = id.to_vec();
        Ok(())
    }

    /// The value of the interchange attribute `attr_name` (`amf_transform_ids` or
    /// `icc_profile_name`, ignoring case), or empty; "Unknown attribute name '<name>'." for any
    /// other name.
    ///
    /// Port of `ColorSpace::getInterchangeAttribute` (ColorSpace.cpp:297-318 @ v2.5.2).
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
    /// Port of `ColorSpace::setInterchangeAttribute` (ColorSpace.cpp:320-345 @ v2.5.2).
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
    /// Port of `ColorSpace::getInterchangeAttributes` (ColorSpace.cpp:347-350 @ v2.5.2), which
    /// returns a copy of the `std::map`.
    #[doc(alias = "getInterchangeAttributes")]
    pub fn interchange_attributes(&self) -> &BTreeMap<Vec<u8>, Vec<u8>> {
        &self.interchange_attribs
    }

    /// Port of `ColorSpace::getBitDepth` (ColorSpace.cpp:352-355 @ v2.5.2).
    #[doc(alias = "getBitDepth")]
    pub fn bit_depth(&self) -> BitDepth {
        self.bit_depth
    }

    /// Port of `ColorSpace::setBitDepth` (ColorSpace.cpp:357-360 @ v2.5.2).
    #[doc(alias = "setBitDepth")]
    pub fn set_bit_depth(&mut self, bit_depth: BitDepth) {
        self.bit_depth = bit_depth;
    }

    /// Whether a category matches `category` ignoring case and the surrounding whitespace.
    ///
    /// Port of `ColorSpace::hasCategory` (ColorSpace.cpp:362-365 @ v2.5.2).
    #[doc(alias = "hasCategory")]
    pub fn has_category(&self, category: impl AsRef<[u8]>) -> bool {
        self.categories.has_token(category.as_ref())
    }

    /// Adds `category`, trimmed, unless it is empty or already there.
    ///
    /// Port of `ColorSpace::addCategory` (ColorSpace.cpp:367-370 @ v2.5.2).
    #[doc(alias = "addCategory")]
    pub fn add_category(&mut self, category: impl AsRef<[u8]>) {
        self.categories.add_token(category.as_ref());
    }

    /// Port of `ColorSpace::removeCategory` (ColorSpace.cpp:372-375 @ v2.5.2).
    #[doc(alias = "removeCategory")]
    pub fn remove_category(&mut self, category: impl AsRef<[u8]>) {
        self.categories.remove_token(category.as_ref());
    }

    /// Port of `ColorSpace::getNumCategories` (ColorSpace.cpp:377-380 @ v2.5.2).
    #[doc(alias = "getNumCategories")]
    pub fn num_categories(&self) -> i32 {
        self.categories.num_tokens()
    }

    /// The category at `index`, or `None` (upstream's null pointer) outside the list.
    ///
    /// Port of `ColorSpace::getCategory` (ColorSpace.cpp:382-385 @ v2.5.2).
    #[doc(alias = "getCategory")]
    pub fn category(&self, index: i32) -> Option<&[u8]> {
        self.categories.token(index)
    }

    /// Port of `ColorSpace::clearCategories` (ColorSpace.cpp:387-390 @ v2.5.2).
    #[doc(alias = "clearCategories")]
    pub fn clear_categories(&mut self) {
        self.categories.clear_tokens();
    }

    /// Port of `ColorSpace::getEncoding` (ColorSpace.cpp:392-395 @ v2.5.2).
    #[doc(alias = "getEncoding")]
    pub fn encoding(&self) -> &[u8] {
        &self.encoding
    }

    /// Port of `ColorSpace::setEncoding` (ColorSpace.cpp:397-400 @ v2.5.2).
    #[doc(alias = "setEncoding")]
    pub fn set_encoding(&mut self, encoding: impl AsRef<[u8]>) {
        self.encoding = c_str(encoding.as_ref()).to_vec();
    }

    /// Port of `ColorSpace::isData` (ColorSpace.cpp:402-405 @ v2.5.2).
    #[doc(alias = "isData")]
    pub fn is_data(&self) -> bool {
        self.is_data
    }

    /// Port of `ColorSpace::setIsData` (ColorSpace.cpp:407-410 @ v2.5.2).
    #[doc(alias = "setIsData")]
    pub fn set_is_data(&mut self, val: bool) {
        self.is_data = val;
    }

    /// Port of `ColorSpace::getReferenceSpaceType` (ColorSpace.cpp:412-415 @ v2.5.2).
    #[doc(alias = "getReferenceSpaceType")]
    pub fn reference_space_type(&self) -> ReferenceSpaceType {
        self.reference_space_type
    }

    /// Port of `ColorSpace::getAllocation` (ColorSpace.cpp:417-420 @ v2.5.2).
    #[doc(alias = "getAllocation")]
    pub fn allocation(&self) -> Allocation {
        self.allocation
    }

    /// Port of `ColorSpace::setAllocation` (ColorSpace.cpp:422-425 @ v2.5.2).
    #[doc(alias = "setAllocation")]
    pub fn set_allocation(&mut self, allocation: Allocation) {
        self.allocation = allocation;
    }

    /// Port of `ColorSpace::getAllocationNumVars` (ColorSpace.cpp:427-430 @ v2.5.2).
    #[doc(alias = "getAllocationNumVars")]
    pub fn allocation_num_vars(&self) -> i32 {
        self.allocation_vars.len() as i32
    }

    /// The allocation's variables.
    ///
    /// Port of `ColorSpace::getAllocationVars` (ColorSpace.cpp:432-441 @ v2.5.2), which copies
    /// them to the caller's array.
    #[doc(alias = "getAllocationVars")]
    pub fn allocation_vars(&self) -> &[f32] {
        &self.allocation_vars
    }

    /// Replaces the allocation's variables. Upstream's errors for a negative count and for a
    /// null pointer with a positive count can't happen with a slice.
    ///
    /// Port of `ColorSpace::setAllocationVars` (ColorSpace.cpp:443-462 @ v2.5.2).
    #[doc(alias = "setAllocationVars")]
    pub fn set_allocation_vars(&mut self, vars: &[f32]) {
        self.allocation_vars = vars.to_vec();
    }

    /// The transform to the reference space or from it, if it is set.
    ///
    /// Port of `ColorSpace::getTransform` (ColorSpace.cpp:464-474 @ v2.5.2).
    #[doc(alias = "getTransform")]
    pub fn transform(&self, dir: ColorSpaceDirection) -> Option<&Transform> {
        match dir {
            ColorSpaceDirection::ToReference => self.to_ref_transform.as_ref(),
            ColorSpaceDirection::FromReference => self.from_ref_transform.as_ref(),
        }
    }

    /// Sets a copy of `transform` as the transform to the reference space or from it; `None`
    /// (upstream's null pointer) removes it. The copy is upstream's `createEditableCopy`
    /// ([`Transform::create_editable_copy`]): an invalid FixedFunctionTransform is refused with
    /// its error, and the color space keeps its transform.
    ///
    /// Port of `ColorSpace::setTransform` (ColorSpace.cpp:476-491 @ v2.5.2).
    #[doc(alias = "setTransform")]
    pub fn set_transform(
        &mut self,
        transform: Option<&Transform>,
        dir: ColorSpaceDirection,
    ) -> Result<()> {
        let transform_copy = transform.map(Transform::create_editable_copy).transpose()?;

        match dir {
            ColorSpaceDirection::ToReference => self.to_ref_transform = transform_copy,
            ColorSpaceDirection::FromReference => self.from_ref_transform = transform_copy,
        }
        Ok(())
    }

    /// The color space's text, as Python's `repr()` prints it, in bytes: names and
    /// descriptions that aren't UTF-8 pass through unchanged, where `Display` replaces them.
    ///
    /// Port of `operator<<(std::ostream &, const ColorSpace &)` (ColorSpace.cpp:493-597 @
    /// v2.5.2), on a new stream.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_text(&mut os);
        os.into_bytes()
    }

    /// Writes the color space's text to `os`; the transforms on the same stream.
    ///
    /// Port of `operator<<(std::ostream &, const ColorSpace &)` (ColorSpace.cpp:493-597 @
    /// v2.5.2).
    pub(crate) fn write_text(&self, os: &mut OStringStream) {
        let vars = self.allocation_vars();
        let num_vars = vars.len();

        os.put_str("<ColorSpace referenceSpaceType=");

        match self.reference_space_type() {
            ReferenceSpaceType::Scene => os.put_str("scene, "),
            ReferenceSpaceType::Display => os.put_str("display, "),
        }
        os.put_str("name=");
        put_c_str(os, self.name());
        os.put_str(", ");
        let num_aliases = self.num_aliases();
        if num_aliases == 1 {
            os.put_str("alias= ");
            put_c_str(os, self.alias(0));
            os.put_str(", ");
        } else if num_aliases > 1 {
            os.put_str("aliases=[");
            put_c_str(os, self.alias(0));
            for aidx in 1..num_aliases {
                os.put_str(", ");
                put_c_str(os, self.alias(aidx));
            }
            os.put_str("], ");
        }

        let put_field = |os: &mut OStringStream, label: &str, value: &[u8]| {
            if !value.is_empty() {
                os.put_str(label);
                put_c_str(os, value);
                os.put_str(", ");
            }
        };
        put_field(os, "interop_id=", self.interop_id());
        put_field(os, "family=", self.family());
        put_field(os, "equalityGroup=", self.equality_group());
        let bd = self.bit_depth();
        if bd != BitDepth::Unknown {
            os.put_str("bitDepth=");
            os.put_str(bit_depth_to_string(bd));
            os.put_str(", ");
        }
        os.put_str("isData=");
        os.put_str(bool_to_string(self.is_data()));
        if num_vars > 0 {
            os.put_str(", allocation=");
            os.put_str(allocation_to_string(self.allocation()));
            os.put_str(", ");
            os.put_str("vars=");
            os.put_f32(vars[0]);
            for &var in &vars[1..] {
                os.put_str(" ");
                os.put_f32(var);
            }
        }
        if self.num_categories() != 0 {
            let categories: Vec<Vec<u8>> = (0..self.num_categories())
                .map(|i| self.category(i).expect("an index inside the list").to_vec())
                .collect();
            os.put_str(", categories=");
            put_c_str(os, &string_utils::join(&categories, b','));
        }
        if !self.encoding().is_empty() {
            os.put_str(", encoding=");
            put_c_str(os, self.encoding());
        }
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
        if let Some(to_ref) = self.transform(ColorSpaceDirection::ToReference) {
            os.put_str(",\n    ");
            put_c_str(os, self.name());
            os.put_str(" --> Reference");
            os.put_str("\n        ");
            to_ref.write_text(os);
        }
        if let Some(from_ref) = self.transform(ColorSpaceDirection::FromReference) {
            os.put_str(",\n    Reference --> ");
            put_c_str(os, self.name());
            os.put_str("\n        ");
            from_ref.write_text(os);
        }
        os.put_str(">");
    }
}

impl fmt::Display for ColorSpace {
    /// The color space's text, as Python's `repr()` prints it.
    ///
    /// Port of `operator<<(std::ostream &, const ColorSpace &)` (ColorSpace.cpp:493-597 @
    /// v2.5.2), on a new stream.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&String::from_utf8_lossy(&self.to_bytes()))
    }
}

#[cfg(test)]
#[path = "color_space_tests.rs"]
mod tests;
