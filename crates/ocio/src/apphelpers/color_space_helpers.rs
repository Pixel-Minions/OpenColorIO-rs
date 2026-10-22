// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The items of color space menus: a port of `src/OpenColorIO/apphelpers/ColorSpaceHelpers.h`
//! and `ColorSpaceHelpers.cpp` @ v2.5.2.
//!
//! - [`ColorSpaceInfo`] (upstream's internal class): a menu item, with its name, UI name,
//!   family, description and the hierarchy levels of its family.

// The menu helper (5.7b) uses them.
#![allow(dead_code)]

use ocio_ops::utils::string_utils::{self, StringVec, c_str};

use crate::color_space::ColorSpace;
use crate::config::Config;
use crate::named_transform::NamedTransform;

/// An item of a menu: a color space, a role or a named transform, with its name, a UI name (the
/// name when none is given), its family, split into hierarchy levels by the config's family
/// separator, and its description.
///
/// Port of `ColorSpaceInfo` (ColorSpaceHelpers.h:22-79, ColorSpaceHelpers.cpp:22-163 @
/// v2.5.2). Upstream hands out shared pointers to constant infos (`ConstColorSpaceInfoRcPtr`),
/// which the port holds as values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ColorSpaceInfo {
    /// `m_name`.
    name: Vec<u8>,
    /// `m_uiName`.
    ui_name: Vec<u8>,
    /// `m_family`.
    family: Vec<u8>,
    /// `m_description`.
    description: Vec<u8>,
    /// `m_hierarchyLevels`: extracted from the family to be used for a hierarchical menu.
    hierarchy_levels: StringVec,
}

impl ColorSpaceInfo {
    /// The info of a color space: its name, family and description (the UI name is the name).
    ///
    /// Port of `ColorSpaceInfo::Create(const ConstConfigRcPtr &, const ColorSpace &)`
    /// (ColorSpaceHelpers.cpp:22-29 @ v2.5.2).
    pub(crate) fn from_color_space(config: &Config, cs: &ColorSpace) -> ColorSpaceInfo {
        ColorSpaceInfo::new(
            config,
            Some(cs.name()),
            None,
            Some(cs.family()),
            Some(cs.description()),
        )
    }

    /// The info of a named transform: its name, family and description (the UI name is the
    /// name).
    ///
    /// Port of `ColorSpaceInfo::Create(const ConstConfigRcPtr &, const NamedTransform &)`
    /// (ColorSpaceHelpers.cpp:31-38 @ v2.5.2).
    pub(crate) fn from_named_transform(config: &Config, nt: &NamedTransform) -> ColorSpaceInfo {
        ColorSpaceInfo::new(
            config,
            Some(nt.name()),
            None,
            Some(nt.family()),
            Some(nt.description()),
        )
    }

    /// An info of a name, a family and a description (the UI name is the name); `None` is a
    /// null pointer, which gives `""`.
    ///
    /// Port of `ColorSpaceInfo::Create(config, name, family, description)`
    /// (ColorSpaceHelpers.cpp:40-46 @ v2.5.2).
    pub(crate) fn create(
        config: &Config,
        name: Option<&[u8]>,
        family: Option<&[u8]>,
        description: Option<&[u8]>,
    ) -> ColorSpaceInfo {
        ColorSpaceInfo::new(config, name, None, family, description)
    }

    /// An info of a name, a UI name (the name when `None` or empty), a family and a
    /// description; `None` is a null pointer, which gives `""`. Each string ends at its first
    /// NUL, as the C strings upstream takes. The family is split by the config's family
    /// separator (unless it is NUL) into its levels, each trimmed, the empty ones left out.
    ///
    /// Port of `ColorSpaceInfo::Create(config, name, uiName, family, description)` and
    /// `ColorSpaceInfo::ColorSpaceInfo` (ColorSpaceHelpers.cpp:48-57, 101-129 @ v2.5.2).
    pub(crate) fn new(
        config: &Config,
        name: Option<&[u8]>,
        ui_name: Option<&[u8]>,
        family: Option<&[u8]>,
        description: Option<&[u8]>,
    ) -> ColorSpaceInfo {
        let name = name.map_or_else(Vec::new, |s| c_str(s).to_vec());
        let ui_name = match ui_name.map(c_str) {
            Some(ui_name) if !ui_name.is_empty() => ui_name.to_vec(),
            _ => name.clone(),
        };
        let family = family.map_or_else(Vec::new, |s| c_str(s).to_vec());
        let description = description.map_or_else(Vec::new, |s| c_str(s).to_vec());

        let vals = if config.family_separator() != 0 && !family.is_empty() {
            string_utils::split(&family, config.family_separator())
        } else {
            vec![family.clone()]
        };

        let mut hierarchy_levels = StringVec::new();
        for val in &vals {
            let v = string_utils::trim(val);
            if !v.is_empty() {
                hierarchy_levels.push(v.to_vec());
            }
        }

        ColorSpaceInfo {
            name,
            ui_name,
            family,
            description,
            hierarchy_levels,
        }
    }

    /// Port of `ColorSpaceInfo::getName` (ColorSpaceHelpers.cpp:131-134 @ v2.5.2).
    pub(crate) fn name(&self) -> &[u8] {
        &self.name
    }

    /// Port of `ColorSpaceInfo::getUIName` (ColorSpaceHelpers.cpp:136-139 @ v2.5.2).
    pub(crate) fn ui_name(&self) -> &[u8] {
        &self.ui_name
    }

    /// Port of `ColorSpaceInfo::getFamily` (ColorSpaceHelpers.cpp:141-144 @ v2.5.2).
    pub(crate) fn family(&self) -> &[u8] {
        &self.family
    }

    /// Port of `ColorSpaceInfo::getDescription` (ColorSpaceHelpers.cpp:146-149 @ v2.5.2).
    pub(crate) fn description(&self) -> &[u8] {
        &self.description
    }

    /// Port of `ColorSpaceInfo::getNumHierarchyLevels` (ColorSpaceHelpers.cpp:151-154 @
    /// v2.5.2).
    pub(crate) fn num_hierarchy_levels(&self) -> usize {
        self.hierarchy_levels.len()
    }

    /// The hierarchy level `i`; `""` outside them.
    ///
    /// Port of `ColorSpaceInfo::getHierarchyLevel` (ColorSpaceHelpers.cpp:156-163 @ v2.5.2).
    pub(crate) fn hierarchy_level(&self, i: usize) -> &[u8] {
        self.hierarchy_levels.get(i).map_or(&[], Vec::as_slice)
    }
}

#[cfg(test)]
#[path = "color_space_helpers_tests.rs"]
mod tests;
