// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! A set of color spaces: a port of `src/OpenColorIO/ColorSpaceSet.cpp` @ v2.5.2.

use ocio_ops::exception::{Exception, Result};
use ocio_ops::utils::string_utils::{self, c_str, lower};

use crate::color_space::ColorSpace;

/// An ordered set of color spaces, looked up by name or alias, ignoring case. It holds copies
/// of the color spaces it is given.
///
/// A copy is upstream's `createEditableCopy`: it copies the color spaces. Two sets are equal
/// when they hold as many color spaces and each name of one names a color space of the other.
///
/// Port of `ColorSpaceSet` and its `Impl` (include/OpenColorIO/OpenColorIO.h:2241-2340,
/// 2355-2389, src/OpenColorIO/ColorSpaceSet.cpp @ v2.5.2).
#[derive(Debug, Clone, Default)]
pub struct ColorSpaceSet {
    /// `m_colorSpaces`.
    color_spaces: Vec<ColorSpace>,
}

impl ColorSpaceSet {
    /// An empty set.
    ///
    /// Port of `ColorSpaceSet::Create` (ColorSpaceSet.cpp:218-221 @ v2.5.2).
    #[doc(alias = "Create")]
    pub fn new() -> ColorSpaceSet {
        ColorSpaceSet::default()
    }

    /// Whether the sets hold as many color spaces, and each name of this one names a color
    /// space of `other` (by name or alias, ignoring case). Only the names are compared.
    ///
    /// Port of `ColorSpaceSet::operator==` and `Impl::operator==` (ColorSpaceSet.cpp:38-57,
    /// 251-254 @ v2.5.2).
    pub fn equals(&self, other: &ColorSpaceSet) -> bool {
        if std::ptr::eq(self, other) {
            return true;
        }

        if self.color_spaces.len() != other.color_spaces.len() {
            return false;
        }

        // NB: Only the names are compared.
        self.color_spaces
            .iter()
            .all(|cs| other.has_color_space(cs.name()))
    }

    /// The number of color spaces.
    ///
    /// Port of `ColorSpaceSet::getNumColorSpaces` (ColorSpaceSet.cpp:261-264 @ v2.5.2).
    #[doc(alias = "getNumColorSpaces")]
    pub fn num_color_spaces(&self) -> i32 {
        self.color_spaces.len() as i32
    }

    /// The name of the color space at `index`, or `None` (upstream's null pointer) outside the
    /// set.
    ///
    /// Port of `ColorSpaceSet::getColorSpaceNameByIndex` and `Impl::getName`
    /// (ColorSpaceSet.cpp:74-82, 266-269 @ v2.5.2).
    #[doc(alias = "getColorSpaceNameByIndex")]
    pub fn color_space_name_by_index(&self, index: i32) -> Option<&[u8]> {
        self.color_space_by_index(index).map(ColorSpace::name)
    }

    /// The color space at `index`, or `None` outside the set.
    ///
    /// Port of `ColorSpaceSet::getColorSpaceByIndex` and `Impl::get` (ColorSpaceSet.cpp:64-72,
    /// 271-274 @ v2.5.2).
    #[doc(alias = "getColorSpaceByIndex")]
    pub fn color_space_by_index(&self, index: i32) -> Option<&ColorSpace> {
        if index < 0 || index >= self.num_color_spaces() {
            return None;
        }

        Some(&self.color_spaces[index as usize])
    }

    /// The color space whose name or alias is `name`, ignoring case, or `None`.
    ///
    /// Port of `ColorSpaceSet::getColorSpace` and `Impl::getByName` (ColorSpaceSet.cpp:84-87,
    /// 276-279 @ v2.5.2).
    #[doc(alias = "getColorSpace")]
    pub fn color_space(&self, name: impl AsRef<[u8]>) -> Option<&ColorSpace> {
        self.color_space_by_index(self.color_space_index(name))
    }

    /// The index of the first color space whose name, or else one of whose aliases, is
    /// `name` ignoring case; -1 for none (or an empty name).
    ///
    /// Port of `ColorSpaceSet::getColorSpaceIndex` and `Impl::getIndex` (ColorSpaceSet.cpp:
    /// 89-112, 281-284 @ v2.5.2).
    #[doc(alias = "getColorSpaceIndex")]
    pub fn color_space_index(&self, name: impl AsRef<[u8]>) -> i32 {
        let cs_name = c_str(name.as_ref());
        // Search for name and aliases.
        if !cs_name.is_empty() {
            let s = lower(cs_name);
            for (idx, cs) in self.color_spaces.iter().enumerate() {
                if string_utils::compare(cs.name(), &s) {
                    return idx as i32;
                }
                for aidx in 0..cs.num_aliases() {
                    if string_utils::compare(cs.alias(aidx), &s) {
                        return idx as i32;
                    }
                }
            }
        }

        -1
    }

    /// Whether a color space has `name` as its name or an alias, ignoring case.
    ///
    /// Port of `ColorSpaceSet::hasColorSpace` and `Impl::isPresent` (ColorSpaceSet.cpp:
    /// 114-117, 286-289 @ v2.5.2).
    #[doc(alias = "hasColorSpace")]
    pub fn has_color_space(&self, name: impl AsRef<[u8]>) -> bool {
        -1 != self.color_space_index(name)
    }

    /// Adds a copy of `cs`, or replaces the color space of the same name (ignoring case) with
    /// it. Refuses a color space without a name, one whose name is another color space's
    /// alias, and one with an alias another color space uses.
    ///
    /// Port of `ColorSpaceSet::addColorSpace` and `Impl::add(const ConstColorSpaceRcPtr &)`
    /// (ColorSpaceSet.cpp:119-168, 291-294 @ v2.5.2).
    #[doc(alias = "addColorSpace")]
    pub fn add_color_space(&mut self, cs: &ColorSpace) -> Result<()> {
        let cs_name = cs.name();
        if cs_name.is_empty() {
            return Err(Exception::new(
                "Cannot add a color space with an empty name.",
            ));
        }

        let mut entry_idx = self.color_space_index(cs_name);
        let mut replace_idx: Option<usize> = None;
        if entry_idx != -1 {
            // If getIndex succeeds but the csName is not the name of the matching color space,
            // it means that csName must be an alias name.  Color space will be replaced only
            // when canonical names match.
            let existing = self.color_spaces[entry_idx as usize].name();
            if !string_utils::compare(existing, cs_name) {
                return Err(Exception::new(
                    [
                        b"Cannot add '".as_slice(),
                        cs_name,
                        b"' color space, existing color space, '",
                        existing,
                        b"' is using this name as an alias.",
                    ]
                    .concat(),
                ));
            }
            // There is a color space with the same name that will be replaced (if new color
            // space can be used).
            replace_idx = Some(entry_idx as usize);
        }

        for aidx in 0..cs.num_aliases() {
            let alias = cs.alias(aidx);
            entry_idx = self.color_space_index(alias);
            // Is an alias of the color space already used by a color space?
            // Skip existing colorspace that might be replaced.
            if entry_idx != -1 && replace_idx != Some(entry_idx as usize) {
                return Err(Exception::new(
                    [
                        b"Cannot add '".as_slice(),
                        cs_name,
                        b"' color space, it has '",
                        alias,
                        b"' alias and existing color space, '",
                        self.color_spaces[entry_idx as usize].name(),
                        b"' is using the same alias.",
                    ]
                    .concat(),
                ));
            }
        }
        if let Some(replace_idx) = replace_idx {
            // The color space replaces the existing one.
            self.color_spaces[replace_idx] = cs.clone();
            return Ok(());
        }

        self.color_spaces.push(cs.clone());
        Ok(())
    }

    /// Adds the color spaces of `css`, in order, as [`add_color_space`](Self::add_color_space)
    /// does; it stops at the first refused, keeping those added before.
    ///
    /// Port of `ColorSpaceSet::addColorSpaces` and `Impl::add(const Impl &)`
    /// (ColorSpaceSet.cpp:170-176, 296-299 @ v2.5.2).
    #[doc(alias = "addColorSpaces")]
    pub fn add_color_spaces(&mut self, css: &ColorSpaceSet) -> Result<()> {
        for cs in &css.color_spaces {
            self.add_color_space(cs)?;
        }
        Ok(())
    }

    /// Removes the first color space whose name is `name`, ignoring case (aliases are not
    /// looked at).
    ///
    /// Port of `ColorSpaceSet::removeColorSpace` and `Impl::remove(const char *)`
    /// (ColorSpaceSet.cpp:178-193, 301-304 @ v2.5.2).
    #[doc(alias = "removeColorSpace")]
    pub fn remove_color_space(&mut self, name: impl AsRef<[u8]>) {
        let cs_name = c_str(name.as_ref());
        if cs_name.is_empty() {
            return;
        }
        let name = lower(cs_name);
        if name.is_empty() {
            return;
        }

        if let Some(idx) = self
            .color_spaces
            .iter()
            .position(|cs| lower(cs.name()) == name)
        {
            self.color_spaces.remove(idx);
        }
    }

    /// Removes the color spaces named as those of `css`.
    ///
    /// Port of `ColorSpaceSet::removeColorSpaces` and `Impl::remove(const Impl &)`
    /// (ColorSpaceSet.cpp:195-201, 306-309 @ v2.5.2).
    #[doc(alias = "removeColorSpaces")]
    pub fn remove_color_spaces(&mut self, css: &ColorSpaceSet) {
        for cs in &css.color_spaces {
            self.remove_color_space(cs.name());
        }
    }

    /// Port of `ColorSpaceSet::clearColorSpaces` (ColorSpaceSet.cpp:311-314 @ v2.5.2).
    #[doc(alias = "clearColorSpaces")]
    pub fn clear_color_spaces(&mut self) {
        self.color_spaces.clear();
    }

    /// The union: a copy of `lcss` with the color spaces of `rcss` added.
    ///
    /// Port of `operator||(const ConstColorSpaceSetRcPtr &, const ConstColorSpaceSetRcPtr &)`
    /// (ColorSpaceSet.cpp:316-322 @ v2.5.2).
    #[doc(alias = "operator||")]
    pub fn union(lcss: &ColorSpaceSet, rcss: &ColorSpaceSet) -> Result<ColorSpaceSet> {
        let mut css = lcss.clone();
        css.add_color_spaces(rcss)?;
        Ok(css)
    }

    /// The intersection: the color spaces of `rcss`, in its order, that `lcss` has by name.
    ///
    /// Port of `operator&&(const ConstColorSpaceSetRcPtr &, const ConstColorSpaceSetRcPtr &)`
    /// (ColorSpaceSet.cpp:324-339 @ v2.5.2).
    #[doc(alias = "operator&&")]
    pub fn intersection(lcss: &ColorSpaceSet, rcss: &ColorSpaceSet) -> Result<ColorSpaceSet> {
        let mut css = ColorSpaceSet::new();

        for tmp in &rcss.color_spaces {
            if lcss.has_color_space(tmp.name()) {
                css.add_color_space(tmp)?;
            }
        }

        Ok(css)
    }

    /// The difference: the color spaces of `lcss`, in its order, that `rcss` doesn't have by
    /// name.
    ///
    /// Port of `operator-(const ConstColorSpaceSetRcPtr &, const ConstColorSpaceSetRcPtr &)`
    /// (ColorSpaceSet.cpp:341-357 @ v2.5.2).
    #[doc(alias = "operator-")]
    pub fn difference(lcss: &ColorSpaceSet, rcss: &ColorSpaceSet) -> Result<ColorSpaceSet> {
        let mut css = ColorSpaceSet::new();

        for tmp in &lcss.color_spaces {
            if !rcss.has_color_space(tmp.name()) {
                css.add_color_space(tmp)?;
            }
        }

        Ok(css)
    }
}

impl PartialEq for ColorSpaceSet {
    /// [`ColorSpaceSet::equals`].
    fn eq(&self, other: &ColorSpaceSet) -> bool {
        self.equals(other)
    }
}

#[cfg(test)]
#[path = "color_space_set_tests.rs"]
mod tests;
