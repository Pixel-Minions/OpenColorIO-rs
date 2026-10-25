// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The items of color space menus: a port of `src/OpenColorIO/apphelpers/ColorSpaceHelpers.h`
//! and `ColorSpaceHelpers.cpp` @ v2.5.2.
//!
//! - [`ColorSpaceMenuParameters`]: what a menu holds (`OpenColorAppHelpers.h:18-199`);
//! - [`ColorSpaceMenuHelper`]: the menu's items, with their names, UI names, families,
//!   descriptions and hierarchy levels (`OpenColorAppHelpers.h:201-276`);
//! - [`add_color_space`]: `ColorSpaceHelpers::AddColorSpace` (`OpenColorAppHelpers.h:278-293`);
//! - `ColorSpaceInfo` (upstream's internal class): a menu item.
//!
//! Upstream keeps every menu it makes, for the life of the process, keyed by the hash of the
//! text of its parameters; the port does the same ([`ColorSpaceMenuHelper::new`]).

use std::cell::Cell;
use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock, Mutex};

use ocio_ops::exception::{Exception, Result};
use ocio_ops::logging::log_warning;
use ocio_ops::open_color_types::{
    ColorSpaceDirection, ColorSpaceVisibility, SearchReferenceSpaceType, TransformDirection,
    combine_transform_directions,
};
use ocio_ops::platform::{getenv, is_env_variable_present};
use ocio_ops::utils::string_utils::{self, StringVec, c_str};

use crate::apphelpers::category_helpers::{
    Categories, Encodings, Infos, extract_items, find_color_space_infos, find_color_space_names,
};
use crate::caching::{OCIO_DISABLE_ALL_CACHES, std_hash_string};
use crate::color_space::ColorSpace;
use crate::config::Config;
use crate::named_transform::NamedTransform;
use crate::transform::Transform;
use crate::transforms::file_transform::FileTransform;
use crate::transforms::group_transform::GroupTransform;

/// `OCIO_USER_CATEGORIES`: the user categories of every menu, in place of those the
/// application sets ([`ColorSpaceMenuParameters::set_user_categories`]), when it is set and
/// not blank.
///
/// Port of `OCIO_USER_CATEGORIES_ENVVAR` (src/OpenColorIO/Config.cpp:52 @ v2.5.2).
pub const OCIO_USER_CATEGORIES_ENVVAR: &str = "OCIO_USER_CATEGORIES";

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

    /// The info of the role `role`: named after the role, with the UI name
    /// `"<role> (<color space>)"` and the family `family`; `None` (upstream's null pointer)
    /// when the config has no such role, or the role is set to `""`.
    ///
    /// Upstream dereferences the null color space a role naming no color space gives; the port
    /// returns an error there (docs/improvements.md, U-141).
    ///
    /// Port of `ColorSpaceInfo::CreateFromRole` (ColorSpaceHelpers.cpp:59-76 @ v2.5.2).
    pub(crate) fn create_from_role(
        config: &Config,
        role: &[u8],
        family: Option<&[u8]>,
    ) -> Result<Option<ColorSpaceInfo>> {
        if config.has_role(role) {
            let cs = role_color_space(config, role)?;
            let ui_name = [c_str(role), b" (", c_str(cs.name()), b")"].concat();

            return Ok(Some(ColorSpaceInfo::new(
                config,
                Some(role), // use role name
                Some(&ui_name),
                family,
                None,
            )));
        }
        Ok(None)
    }

    /// The info of the color space of the role `role`: named after the color space, with the
    /// UI name `"<role> (<color space>)"`, no family and no description; `None` (upstream's null
    /// pointer) when the config has no such role.
    ///
    /// Upstream dereferences the null color space a role naming no color space gives; the port
    /// returns an error there (docs/improvements.md, U-141).
    ///
    /// Port of `ColorSpaceInfo::CreateFromSingleRole` (ColorSpaceHelpers.cpp:78-94 @ v2.5.2).
    pub(crate) fn create_from_single_role(
        config: &Config,
        role: &[u8],
    ) -> Result<Option<ColorSpaceInfo>> {
        if config.has_role(role) {
            let cs = role_color_space(config, role)?;
            let ui_name = [c_str(role), b" (", c_str(cs.name()), b")"].concat();

            return Ok(Some(ColorSpaceInfo::new(
                config,
                Some(cs.name()), // use color space name
                Some(&ui_name),
                None,
                None,
            )));
        }
        Ok(None)
    }
}

/// `config->getColorSpace(role)` for a role the config has: the color space of that name (or
/// alias), else the one the role names. When there is none, upstream reads through a null
/// pointer, and its process dies; the port returns an error (docs/improvements.md, U-141).
fn role_color_space<'a>(config: &'a Config, role: &[u8]) -> Result<&'a ColorSpace> {
    config.color_space(role).ok_or_else(|| {
        Exception::new(
            [
                b"ColorSpaceMenuHelper: the role '".as_slice(),
                c_str(role),
                b"' refers to the color space '",
                config.role_color_space(role),
                b"', which doesn't exist: OCIO 2.5.2 reads a null color space there, and its \
                  process dies (docs/improvements.md, U-141).",
            ]
            .concat(),
        )
    })
}

/// The parameters of a menu: its config, a role that decides it alone, the color spaces (of
/// a reference space type), named transforms and roles it includes, the app categories, the
/// encodings and the user categories that filter its items (comma-separated lists), and the
/// color spaces added at its end. The algorithm is `OpenColorAppHelpers.h:19-103`'s.
///
/// Strings are bytes; each setter's argument ends at its first NUL, as the C strings upstream
/// takes.
///
/// Port of `ColorSpaceMenuParameters` (include/OpenColorIO/OpenColorAppHelpers.h:104-199) and
/// `ColorSpaceMenuParametersImpl` (src/OpenColorIO/apphelpers/ColorSpaceHelpers.h:82-137,
/// ColorSpaceHelpers.cpp:165-322 @ v2.5.2), its only implementation. The port's `Clone` is the
/// Python binding's: it calls the library on a copy, so that no lock of its own is held while
/// the library runs Python code (a config's I/O proxy, a logging function).
#[derive(Debug, Clone)]
pub struct ColorSpaceMenuParameters {
    /// `m_config`.
    config: Arc<Config>,
    /// `m_role`.
    role: Vec<u8>,
    /// `m_appCategories`.
    app_categories: Vec<u8>,
    /// `m_userCategories`.
    user_categories: Vec<u8>,
    /// `m_encodings`.
    encodings: Vec<u8>,
    /// `m_includeColorSpaces`.
    include_color_spaces: bool,
    /// `m_includeRoles`.
    include_roles: bool,
    /// `m_includeNamedTransforms`.
    include_named_transforms: bool,
    /// `m_treatNoCategoryAsAny`.
    treat_no_category_as_any: bool,
    /// `m_colorSpaceType`.
    color_space_type: SearchReferenceSpaceType,
    /// `m_additionalColorSpaces`.
    additional_color_spaces: StringVec,
}

impl ColorSpaceMenuParameters {
    /// The parameters of a menu of `config`: all its active color spaces, no named transform
    /// and no role; color spaces without categories count as having any.
    ///
    /// Port of `ColorSpaceMenuParameters::Create` and
    /// `ColorSpaceMenuParametersImpl::ColorSpaceMenuParametersImpl` (ColorSpaceHelpers.cpp:
    /// 165-174 @ v2.5.2).
    #[doc(alias = "Create")]
    pub fn new(config: Arc<Config>) -> ColorSpaceMenuParameters {
        ColorSpaceMenuParameters {
            config,
            role: Vec::new(),
            app_categories: Vec::new(),
            user_categories: Vec::new(),
            encodings: Vec::new(),
            include_color_spaces: true,
            include_roles: false,
            include_named_transforms: false,
            treat_no_category_as_any: true,
            color_space_type: SearchReferenceSpaceType::All,
            additional_color_spaces: StringVec::new(),
        }
    }

    /// A copy of `parameters`.
    ///
    /// Port of `ColorSpaceMenuParametersImpl::setParameters` (ColorSpaceHelpers.cpp:176-190 @
    /// v2.5.2), on new parameters.
    fn copy_of(parameters: &ColorSpaceMenuParameters) -> ColorSpaceMenuParameters {
        ColorSpaceMenuParameters {
            config: Arc::clone(&parameters.config),
            role: parameters.role.clone(),
            app_categories: parameters.app_categories.clone(),
            user_categories: parameters.user_categories.clone(),
            encodings: parameters.encodings.clone(),
            include_color_spaces: parameters.include_color_spaces,
            include_roles: parameters.include_roles,
            include_named_transforms: parameters.include_named_transforms,
            treat_no_category_as_any: parameters.treat_no_category_as_any,
            color_space_type: parameters.color_space_type,
            additional_color_spaces: parameters.additional_color_spaces.clone(),
        }
    }

    /// Port of `ColorSpaceMenuParametersImpl::setConfig` (ColorSpaceHelpers.cpp:192-195 @
    /// v2.5.2).
    #[doc(alias = "setConfig")]
    pub fn set_config(&mut self, config: Arc<Config>) {
        self.config = config;
    }

    /// Port of `ColorSpaceMenuParametersImpl::getConfig` (ColorSpaceHelpers.cpp:197-200 @
    /// v2.5.2).
    #[doc(alias = "getConfig")]
    pub fn config(&self) -> &Arc<Config> {
        &self.config
    }

    /// Sets the role that decides the menu: when the config has it, the menu is that role's
    /// color space alone, and the other parameters are ignored.
    ///
    /// Port of `ColorSpaceMenuParametersImpl::setRole` (ColorSpaceHelpers.cpp:202-205 @
    /// v2.5.2).
    #[doc(alias = "setRole")]
    pub fn set_role(&mut self, role: impl AsRef<[u8]>) {
        self.role = c_str(role.as_ref()).to_vec();
    }

    /// Port of `ColorSpaceMenuParametersImpl::getRole` (ColorSpaceHelpers.cpp:207-210 @
    /// v2.5.2).
    #[doc(alias = "getRole")]
    pub fn role(&self) -> &[u8] {
        &self.role
    }

    /// Sets the app categories, a comma-separated list.
    ///
    /// Port of `ColorSpaceMenuParametersImpl::setAppCategories` (ColorSpaceHelpers.cpp:
    /// 212-215 @ v2.5.2).
    #[doc(alias = "setAppCategories")]
    pub fn set_app_categories(&mut self, app_categories: impl AsRef<[u8]>) {
        self.app_categories = c_str(app_categories.as_ref()).to_vec();
    }

    /// Port of `ColorSpaceMenuParametersImpl::getAppCategories` (ColorSpaceHelpers.cpp:
    /// 217-220 @ v2.5.2).
    #[doc(alias = "getAppCategories")]
    pub fn app_categories(&self) -> &[u8] {
        &self.app_categories
    }

    /// Sets the user categories, a comma-separated list, which `OCIO_USER_CATEGORIES`
    /// overrides when it is set and not blank.
    ///
    /// Port of `ColorSpaceMenuParametersImpl::setUserCategories` (ColorSpaceHelpers.cpp:
    /// 222-225 @ v2.5.2).
    #[doc(alias = "setUserCategories")]
    pub fn set_user_categories(&mut self, user_categories: impl AsRef<[u8]>) {
        self.user_categories = c_str(user_categories.as_ref()).to_vec();
    }

    /// Port of `ColorSpaceMenuParametersImpl::getUserCategories` (ColorSpaceHelpers.cpp:
    /// 227-230 @ v2.5.2).
    #[doc(alias = "getUserCategories")]
    pub fn user_categories(&self) -> &[u8] {
        &self.user_categories
    }

    /// Sets the encodings, a comma-separated list.
    ///
    /// Port of `ColorSpaceMenuParametersImpl::setEncodings` (ColorSpaceHelpers.cpp:232-235 @
    /// v2.5.2).
    #[doc(alias = "setEncodings")]
    pub fn set_encodings(&mut self, encodings: impl AsRef<[u8]>) {
        self.encodings = c_str(encodings.as_ref()).to_vec();
    }

    /// Port of `ColorSpaceMenuParametersImpl::getEncodings` (ColorSpaceHelpers.cpp:237-240 @
    /// v2.5.2).
    #[doc(alias = "getEncodings")]
    pub fn encodings(&self) -> &[u8] {
        &self.encodings
    }

    /// Port of `ColorSpaceMenuParametersImpl::setIncludeColorSpaces` (ColorSpaceHelpers.cpp:
    /// 242-245 @ v2.5.2).
    #[doc(alias = "setIncludeColorSpaces")]
    pub fn set_include_color_spaces(&mut self, include: bool) {
        self.include_color_spaces = include;
    }

    /// Port of `ColorSpaceMenuParametersImpl::getIncludeColorSpaces` (ColorSpaceHelpers.cpp:
    /// 247-250 @ v2.5.2).
    #[doc(alias = "getIncludeColorSpaces")]
    pub fn include_color_spaces(&self) -> bool {
        self.include_color_spaces
    }

    /// Sets whether the menu ends with the roles (after the color spaces and named
    /// transforms), each with the family `Roles`.
    ///
    /// Port of `ColorSpaceMenuParametersImpl::setIncludeRoles` (ColorSpaceHelpers.cpp:252-255
    /// @ v2.5.2).
    #[doc(alias = "setIncludeRoles")]
    pub fn set_include_roles(&mut self, include: bool) {
        self.include_roles = include;
    }

    /// Port of `ColorSpaceMenuParametersImpl::getIncludeRoles` (ColorSpaceHelpers.cpp:257-260
    /// @ v2.5.2).
    #[doc(alias = "getIncludeRoles")]
    pub fn include_roles(&self) -> bool {
        self.include_roles
    }

    /// Port of `ColorSpaceMenuParametersImpl::setIncludeNamedTransforms`
    /// (ColorSpaceHelpers.cpp:262-265 @ v2.5.2).
    #[doc(alias = "setIncludeNamedTransforms")]
    pub fn set_include_named_transforms(&mut self, include: bool) {
        self.include_named_transforms = include;
    }

    /// Port of `ColorSpaceMenuParametersImpl::getIncludeNamedTransforms`
    /// (ColorSpaceHelpers.cpp:267-270 @ v2.5.2).
    #[doc(alias = "getIncludeNamedTransforms")]
    pub fn include_named_transforms(&self) -> bool {
        self.include_named_transforms
    }

    /// Sets whether color spaces and named transforms without categories count as having any.
    ///
    /// Port of `ColorSpaceMenuParametersImpl::setTreatNoCategoryAsAny` (ColorSpaceHelpers.cpp:
    /// 272-275 @ v2.5.2).
    #[doc(alias = "setTreatNoCategoryAsAny")]
    pub fn set_treat_no_category_as_any(&mut self, value: bool) {
        self.treat_no_category_as_any = value;
    }

    /// Port of `ColorSpaceMenuParametersImpl::getTreatNoCategoryAsAny` (ColorSpaceHelpers.cpp:
    /// 277-280 @ v2.5.2).
    #[doc(alias = "getTreatNoCategoryAsAny")]
    pub fn treat_no_category_as_any(&self) -> bool {
        self.treat_no_category_as_any
    }

    /// Adds a color space (or a named transform, or a role's color space) to the end of the
    /// menu, unless `name` is empty or already added (ignoring case).
    ///
    /// Port of `ColorSpaceMenuParametersImpl::addColorSpace` (ColorSpaceHelpers.cpp:282-288 @
    /// v2.5.2).
    #[doc(alias = "addColorSpace")]
    pub fn add_color_space(&mut self, name: impl AsRef<[u8]>) {
        let name = c_str(name.as_ref());
        if !name.is_empty() && !string_utils::contain(&self.additional_color_spaces, name) {
            self.additional_color_spaces.push(name.to_vec());
        }
    }

    /// Port of `ColorSpaceMenuParametersImpl::getNumAddedColorSpaces` (ColorSpaceHelpers.cpp:
    /// 290-293 @ v2.5.2).
    #[doc(alias = "getNumAddedColorSpaces")]
    pub fn num_added_color_spaces(&self) -> usize {
        self.additional_color_spaces.len()
    }

    /// The added color space at `index`; `""` outside them.
    ///
    /// Port of `ColorSpaceMenuParametersImpl::getAddedColorSpace` (ColorSpaceHelpers.cpp:
    /// 295-302 @ v2.5.2).
    #[doc(alias = "getAddedColorSpace")]
    pub fn added_color_space(&self, index: usize) -> &[u8] {
        self.additional_color_spaces
            .get(index)
            .map_or(&[], Vec::as_slice)
    }

    /// Port of `ColorSpaceMenuParametersImpl::clearAddedColorSpaces` (ColorSpaceHelpers.cpp:
    /// 304-307 @ v2.5.2).
    #[doc(alias = "clearAddedColorSpaces")]
    pub fn clear_added_color_spaces(&mut self) {
        self.additional_color_spaces.clear();
    }

    /// Port of `ColorSpaceMenuParametersImpl::getSearchReferenceSpaceType`
    /// (ColorSpaceHelpers.cpp:309-312 @ v2.5.2).
    #[doc(alias = "getSearchReferenceSpaceType")]
    pub fn search_reference_space_type(&self) -> SearchReferenceSpaceType {
        self.color_space_type
    }

    /// Sets the reference space type of the color spaces the menu includes (all, scene or
    /// display); it has no effect on roles and named transforms.
    ///
    /// Port of `ColorSpaceMenuParametersImpl::setSearchReferenceSpaceType`
    /// (ColorSpaceHelpers.cpp:314-317 @ v2.5.2).
    #[doc(alias = "setSearchReferenceSpaceType")]
    pub fn set_search_reference_space_type(&mut self, color_space_type: SearchReferenceSpaceType) {
        self.color_space_type = color_space_type;
    }

    /// The parameters' text, as Python's `repr()` prints it: the config's cache ID, then each
    /// parameter that isn't empty and the four flags, and the added color spaces. Computing the
    /// config's cache ID can fail (it serializes the config).
    ///
    /// Port of `operator<<(std::ostream &, const ColorSpaceMenuParameters &)`
    /// (ColorSpaceHelpers.cpp:324-376 @ v2.5.2).
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let bool_str = |b: bool| -> &'static [u8] { if b { b"true" } else { b"false" } };

        // A missing config is printed "missing" upstream; the port's parameters always have
        // one.
        let mut os = b"config: ".to_vec();
        os.extend_from_slice(self.config.cache_id()?.as_bytes());
        if !self.role.is_empty() {
            os.extend_from_slice(b", role: ");
            os.extend_from_slice(&self.role);
        }
        if !self.app_categories.is_empty() {
            os.extend_from_slice(b", appCategories: ");
            os.extend_from_slice(&self.app_categories);
        }
        if !self.user_categories.is_empty() {
            os.extend_from_slice(b", userCategories: ");
            os.extend_from_slice(&self.user_categories);
        }
        if !self.encodings.is_empty() {
            os.extend_from_slice(b", encodings: ");
            os.extend_from_slice(&self.encodings);
        }
        os.extend_from_slice(b", includeColorSpaces: ");
        os.extend_from_slice(bool_str(self.include_color_spaces()));
        os.extend_from_slice(b", includeRoles: ");
        os.extend_from_slice(bool_str(self.include_roles()));
        os.extend_from_slice(b", includeNamedTransforms: ");
        os.extend_from_slice(bool_str(self.include_named_transforms()));
        os.extend_from_slice(b", treatNoCategoryAsAny: ");
        os.extend_from_slice(bool_str(self.treat_no_category_as_any()));
        if self.color_space_type == SearchReferenceSpaceType::Scene {
            os.extend_from_slice(b", colorSpaceType: scene");
        } else if self.color_space_type == SearchReferenceSpaceType::Display {
            os.extend_from_slice(b", colorSpaceType: display");
        }
        let numcs = self.additional_color_spaces.len();
        if numcs != 0 {
            os.extend_from_slice(b", addedSpaces: ");
            if numcs == 1 {
                os.extend_from_slice(&self.additional_color_spaces[0]);
            } else {
                os.extend_from_slice(b"[");
                os.extend_from_slice(&self.additional_color_spaces[0]);
                for cs in &self.additional_color_spaces[1..] {
                    os.extend_from_slice(b", ");
                    os.extend_from_slice(cs);
                }
                os.extend_from_slice(b"]");
            }
        }
        Ok(os)
    }
}

/// The menus made so far, by the hash of their parameters' text (upstream's `g_entries`, a
/// `std::map`); an entry whose menu failed to be made holds `None`.
type Entries = BTreeMap<u64, Option<Arc<ColorSpaceMenuHelper>>>;

/// Port of `g_mutex` and `g_entries` (ColorSpaceHelpers.cpp:425-426 @ v2.5.2).
static ENTRIES: Mutex<Entries> = Mutex::new(BTreeMap::new());

thread_local! {
    /// Whether this thread holds [`ENTRIES`]' lock, making a menu: a menu made meanwhile on
    /// this thread (by a logging function the menu's refresh calls) would lock it again
    /// (docs/improvements.md, U-181).
    static MAKING_A_MENU: Cell<bool> = const { Cell::new(false) };
}

/// [`MAKING_A_MENU`] set while it lives.
struct MakingAMenu;

impl MakingAMenu {
    fn enter() -> MakingAMenu {
        MAKING_A_MENU.with(|m| m.set(true));
        MakingAMenu
    }
}

impl Drop for MakingAMenu {
    fn drop(&mut self) {
        MAKING_A_MENU.with(|m| m.set(false));
    }
}

/// Whether menus are kept: `OCIO_DISABLE_ALL_CACHES` isn't set, as the environment is when the
/// first menu is asked for (past the checks of its parameters), and never read again.
///
/// Port of `isCacheEnabled` (ColorSpaceHelpers.cpp:428 @ v2.5.2), a function-local `static`.
static IS_CACHE_ENABLED: LazyLock<bool> =
    LazyLock::new(|| !is_env_variable_present(OCIO_DISABLE_ALL_CACHES));

/// A menu: the items of a config that a [`ColorSpaceMenuParameters`] selects (its
/// algorithm, `OpenColorAppHelpers.h:19-103`), each with its name, UI name, family,
/// description and hierarchy levels (the family split by the config's family separator). In
/// order: the color spaces, the named transforms, the roles, then the added color spaces.
///
/// With the roles included, a role set to `""` makes upstream build a menu with a null item,
/// and its process dies when that item is read; the port refuses to make that menu
/// (docs/improvements.md, U-142).
///
/// Port of `ColorSpaceMenuHelper` (include/OpenColorIO/OpenColorAppHelpers.h:201-276) and
/// `ColorSpaceMenuHelperImpl` (src/OpenColorIO/apphelpers/ColorSpaceHelpers.h:140-180,
/// ColorSpaceHelpers.cpp:378-734 @ v2.5.2), its only implementation.
#[derive(Debug)]
pub struct ColorSpaceMenuHelper {
    /// `m_parameters`: a copy of the parameters the menu was made with.
    parameters: ColorSpaceMenuParameters,
    /// `m_entries`: the items, the added color spaces included.
    entries: Vec<ColorSpaceInfo>,
}

impl ColorSpaceMenuHelper {
    /// The menu of `p`. The config is validated first, and what it refuses is logged as a
    /// warning ("ColorSpaceMenuHelper needs a valid config. Validation warning is: ..."). Roles
    /// need the color spaces included. `OCIO_USER_CATEGORIES`, when set and not blank once
    /// trimmed, replaces the user categories.
    ///
    /// Menus are kept for the life of the process, by the hash of their parameters' text
    /// ([`ColorSpaceMenuParameters::to_bytes`], `std::hash` as each platform's C++ library
    /// computes it): parameters of the same text give the very menu made first, and a menu is
    /// logged about only when it is made. `OCIO_DISABLE_ALL_CACHES`, as the environment is at
    /// the first menu, keeps none (docs/improvements.md, I-317).
    ///
    /// An added color space that is neither a color space nor a named transform of the config
    /// fails ("Element '<name>' is neither a color space not a named transform."). A role that
    /// names a missing color space fails, where upstream's process dies (docs/improvements.md,
    /// U-141), and so does a role set to the empty name with the roles included, where
    /// upstream makes a menu with a null item and its process dies when that item is read
    /// (U-142). A menu asked for on a thread that is making one (by a logging function its
    /// making calls) fails, where upstream locks its menus' mutex again (U-181): on Windows
    /// with MSVC's `std::system_error` ("resource deadlock would occur: resource deadlock would
    /// occur"), elsewhere with the port's error, where upstream's process hangs.
    ///
    /// Port of `ColorSpaceMenuHelper::Create` (ColorSpaceHelpers.cpp:378-450 @ v2.5.2).
    #[doc(alias = "Create")]
    pub fn new(p: &ColorSpaceMenuParameters) -> Result<Arc<ColorSpaceMenuHelper>> {
        ColorSpaceMenuHelper::log_validation(p);
        ColorSpaceMenuHelper::new_validated(p)
    }

    /// [`ColorSpaceMenuHelper::new`]'s first step, public for the Python binding only
    /// (`ocio-py`), with [`ColorSpaceMenuHelper::new_validated`]: the config's validation,
    /// whose error is logged. The library can run Python code here (a logging function), which
    /// can change the parameters, and upstream reads them after it, so the binding hands the
    /// second step the parameters as they are then. Behaviour-neutral for the Rust API (owner
    /// decision 2026-10-24, relayed by the coordinator, for the mixing manager's processor).
    ///
    /// Port of `ColorSpaceMenuHelper::Create` (ColorSpaceHelpers.cpp:378-394 @ v2.5.2).
    #[doc(hidden)]
    pub fn log_validation(p: &ColorSpaceMenuParameters) {
        // Upstream refuses parameters without a config ("ColorSpaceMenuHelper needs parameters
        // with a config."): the port's always have one.

        if let Err(e) = p.config.validate() {
            let mut oss =
                b"ColorSpaceMenuHelper needs a valid config. Validation warning is: ".to_vec();
            oss.extend_from_slice(e.what());
            log_warning(&oss);
        }
    }

    /// [`ColorSpaceMenuHelper::new`]'s second step (see
    /// [`ColorSpaceMenuHelper::log_validation`]). For the Python binding only.
    ///
    /// Port of `ColorSpaceMenuHelper::Create` (ColorSpaceHelpers.cpp:396-450 @ v2.5.2).
    #[doc(hidden)]
    pub fn new_validated(p: &ColorSpaceMenuParameters) -> Result<Arc<ColorSpaceMenuHelper>> {
        if !p.include_color_spaces() && p.include_roles() {
            return Err(Exception::new(
                "ColorSpaceMenuHelper needs to include color spaces if roles are included.",
            ));
        }

        // User categories from the environment variable override what is specified by the
        // application.
        let env_parameters;
        let mut parameters = p;
        if let Some(env_user_categories) = getenv(OCIO_USER_CATEGORIES_ENVVAR)
            && !env_user_categories.is_empty()
        {
            let user_categories = string_utils::trim(&env_user_categories);
            if !user_categories.is_empty() {
                let mut new_p = ColorSpaceMenuParameters::copy_of(parameters);
                new_p.set_user_categories(user_categories);
                env_parameters = new_p;
                parameters = &env_parameters;
            }
        }

        // Note: Add a cache to not recreate a new menu helper for the same parameters.

        if *IS_CACHE_ENABLED {
            // Upstream locks its `std::mutex` again on this thread, which is undefined
            // behaviour (docs/improvements.md, U-181): MSVC's throws `std::system_error`, which
            // the port matches on Windows; glibc's waits for itself, where the port refuses.
            if MAKING_A_MENU.with(Cell::get) {
                return Err(if cfg!(windows) {
                    Exception::system_error(
                        "resource deadlock would occur: resource deadlock would occur",
                    )
                } else {
                    Exception::new(
                        "ColorSpaceMenuHelper: a menu is made while this thread makes another \
                         one (from a logging function its making calls): OCIO 2.5.2 locks its \
                         menus' mutex again on the same thread, which is undefined behaviour \
                         (docs/improvements.md, U-181).",
                    )
                });
            }
            let mut entries = ENTRIES.lock().unwrap_or_else(|e| e.into_inner());
            let _making = MakingAMenu::enter();

            let oss = parameters.to_bytes()?;

            let key = std_hash_string(&oss);

            let entry = entries.entry(key).or_default();
            if entry.is_none() {
                *entry = Some(Arc::new(ColorSpaceMenuHelper::create(parameters)?));
            }

            return Ok(Arc::clone(entry.as_ref().expect("made above")));
        }

        Ok(Arc::new(ColorSpaceMenuHelper::create(parameters)?))
    }

    /// A menu of a copy of `parameters`, its items found.
    ///
    /// Port of `ColorSpaceMenuHelperImpl::ColorSpaceMenuHelperImpl` (ColorSpaceHelpers.cpp:
    /// 457-463 @ v2.5.2).
    fn create(parameters: &ColorSpaceMenuParameters) -> Result<ColorSpaceMenuHelper> {
        let mut helper = ColorSpaceMenuHelper {
            parameters: ColorSpaceMenuParameters::copy_of(parameters),
            entries: Vec::new(),
        };
        helper.refresh()?;
        Ok(helper)
    }

    /// Finds the items: the role's color space alone, if the config has the role; else the
    /// items of `FindColorSpaceInfos`, the roles, and the added color spaces not already
    /// there.
    ///
    /// Port of `ColorSpaceMenuHelperImpl::refresh` (ColorSpaceHelpers.cpp:465-588 @ v2.5.2).
    fn refresh(&mut self) -> Result<()> {
        self.entries.clear();
        let params = &self.parameters;
        let config: &Config = &params.config;

        // 1) If the role exists, use only that space.

        if !params.role.is_empty() && config.has_role(&params.role) {
            let info = ColorSpaceInfo::create_from_single_role(config, &params.role)?
                .expect("the config has the role");
            self.entries.push(info);
            return Ok(());
        }

        // 2) & 3) Identify potential menu items and then filter them by category and
        // encoding.

        let all_app_categories: Categories = extract_items(Some(&params.app_categories));
        let all_user_categories: Categories = extract_items(Some(&params.user_categories));
        let all_encodings: Encodings = extract_items(Some(&params.encodings));
        let num_nt = config.num_named_transforms();
        let num_cs =
            config.num_color_spaces_with(params.color_space_type, ColorSpaceVisibility::Active);
        if (params.include_color_spaces && num_cs != 0)
            || (params.include_named_transforms && num_nt != 0)
        {
            let infos: Infos = find_color_space_infos(
                config,
                &all_app_categories,
                &all_user_categories,
                params.include_color_spaces,
                params.include_named_transforms,
                params.treat_no_category_as_any,
                &all_encodings,
                params.color_space_type,
            );
            self.entries = infos;
        }

        // 4) Include roles if requested.

        if params.include_roles {
            for idx in 0..config.num_roles() {
                let role = config.role_name(idx);
                // A role set to "" gives a null item upstream, and its process dies when that
                // item is read: the port refuses (docs/improvements.md, U-142).
                let Some(info) = ColorSpaceInfo::create_from_role(config, role, Some(b"Roles"))?
                else {
                    return Err(Exception::new(
                        [
                            b"ColorSpaceMenuHelper: the menu item of the role '".as_slice(),
                            role,
                            b"' is null (the role refers to no color space): OCIO 2.5.2 makes a \
                              menu with a null item there, and its process dies when the item \
                              is read (docs/improvements.md, U-142).",
                        ]
                        .concat(),
                    ));
                };
                self.entries.push(info);
            }
        }

        // 5) Add additional color spaces.

        let mut additional_color_spaces = Infos::new();
        for name in &params.additional_color_spaces {
            // Whether an item, or an added one, is already named `item_name`.
            let already_there = |item_name: &[u8], added: &Infos| -> bool {
                self.entries
                    .iter()
                    .chain(added.iter())
                    .any(|entry| string_utils::compare(c_str(item_name), entry.name()))
            };
            if let Some(cs) = config.color_space(name) {
                if !already_there(cs.name(), &additional_color_spaces) {
                    let cs_info = ColorSpaceInfo::from_color_space(config, cs);
                    additional_color_spaces.push(cs_info);
                }
            } else if let Some(nt) = config.named_transform(name) {
                if !already_there(nt.name(), &additional_color_spaces) {
                    let cs_info = ColorSpaceInfo::from_named_transform(config, nt);
                    additional_color_spaces.push(cs_info);
                }
            } else {
                return Err(Exception::new(
                    [
                        b"Element '".as_slice(),
                        name,
                        b"' is neither a color space not a named transform.",
                    ]
                    .concat(),
                ));
            }
        }

        self.entries.extend(additional_color_spaces);
        Ok(())
    }

    /// The item at `idx`, `None` outside the items.
    fn item(&self, idx: usize) -> Option<&ColorSpaceInfo> {
        self.entries.get(idx)
    }

    /// The number of items.
    ///
    /// Port of `ColorSpaceMenuHelperImpl::getNumColorSpaces` (ColorSpaceHelpers.cpp:590-593 @
    /// v2.5.2).
    #[doc(alias = "getNumColorSpaces")]
    pub fn num_color_spaces(&self) -> usize {
        self.entries.len()
    }

    /// The config's name of the item at `idx` (a role's color space for a menu of a role, a
    /// role for the roles included); `""` outside the items.
    ///
    /// Port of `ColorSpaceMenuHelperImpl::getName` (ColorSpaceHelpers.cpp:595-602 @ v2.5.2).
    #[doc(alias = "getName")]
    pub fn name(&self, idx: usize) -> &[u8] {
        self.item(idx).map_or(&[], ColorSpaceInfo::name)
    }

    /// The name to show for the item at `idx` (`"<role> (<color space>)"` for a role, the
    /// name otherwise); `""` outside the items.
    ///
    /// Port of `ColorSpaceMenuHelperImpl::getUIName` (ColorSpaceHelpers.cpp:604-611 @ v2.5.2).
    #[doc(alias = "getUIName")]
    pub fn ui_name(&self, idx: usize) -> &[u8] {
        self.item(idx).map_or(&[], ColorSpaceInfo::ui_name)
    }

    /// The index of the first item named `name` (ignoring case); `usize::MAX` (upstream's
    /// `(size_t)-1`) for none, or an empty name.
    ///
    /// Port of `ColorSpaceMenuHelperImpl::getIndexFromName` (ColorSpaceHelpers.cpp:613-627 @
    /// v2.5.2).
    #[doc(alias = "getIndexFromName")]
    pub fn index_from_name(&self, name: impl AsRef<[u8]>) -> usize {
        let cs_name = c_str(name.as_ref());
        if !cs_name.is_empty() {
            for (idx, entry) in self.entries.iter().enumerate() {
                if string_utils::compare(entry.name(), cs_name) {
                    return idx;
                }
            }
        }
        usize::MAX
    }

    /// The index of the first item whose UI name is `name` (ignoring case); `usize::MAX`
    /// (upstream's `(size_t)-1`) for none, or an empty name.
    ///
    /// Port of `ColorSpaceMenuHelperImpl::getIndexFromUIName` (ColorSpaceHelpers.cpp:629-643 @
    /// v2.5.2).
    #[doc(alias = "getIndexFromUIName")]
    pub fn index_from_ui_name(&self, name: impl AsRef<[u8]>) -> usize {
        let cs_name = c_str(name.as_ref());
        if !cs_name.is_empty() {
            for (idx, entry) in self.entries.iter().enumerate() {
                if string_utils::compare(entry.ui_name(), cs_name) {
                    return idx;
                }
            }
        }
        usize::MAX
    }

    /// The family of the item at `idx`; `""` outside the items.
    ///
    /// Port of `ColorSpaceMenuHelperImpl::getFamily` (ColorSpaceHelpers.cpp:645-652 @ v2.5.2).
    #[doc(alias = "getFamily")]
    pub fn family(&self, idx: usize) -> &[u8] {
        self.item(idx).map_or(&[], ColorSpaceInfo::family)
    }

    /// The description of the item at `idx`; `""` outside the items.
    ///
    /// Port of `ColorSpaceMenuHelperImpl::getDescription` (ColorSpaceHelpers.cpp:654-661 @
    /// v2.5.2).
    #[doc(alias = "getDescription")]
    pub fn description(&self, idx: usize) -> &[u8] {
        self.item(idx).map_or(&[], ColorSpaceInfo::description)
    }

    /// The number of hierarchy levels of the item at `idx` (its family split by the config's
    /// family separator, each level trimmed, the empty ones left out); 0 outside the items.
    ///
    /// Port of `ColorSpaceMenuHelperImpl::getNumHierarchyLevels` (ColorSpaceHelpers.cpp:
    /// 663-670 @ v2.5.2).
    #[doc(alias = "getNumHierarchyLevels")]
    pub fn num_hierarchy_levels(&self, idx: usize) -> usize {
        self.item(idx)
            .map_or(0, ColorSpaceInfo::num_hierarchy_levels)
    }

    /// The hierarchy level `i` of the item at `idx`; `""` outside them.
    ///
    /// Port of `ColorSpaceMenuHelperImpl::getHierarchyLevel` (ColorSpaceHelpers.cpp:672-679 @
    /// v2.5.2).
    #[doc(alias = "getHierarchyLevel")]
    pub fn hierarchy_level(&self, idx: usize, i: usize) -> &[u8] {
        self.item(idx).map_or(&[], |entry| entry.hierarchy_level(i))
    }

    /// The name of the first item whose UI name is `ui_name` (ignoring case); `""` for none,
    /// or an empty UI name.
    ///
    /// Port of `ColorSpaceMenuHelperImpl::getNameFromUIName` (ColorSpaceHelpers.cpp:681-694 @
    /// v2.5.2).
    #[doc(alias = "getNameFromUIName")]
    pub fn name_from_ui_name(&self, ui_name: impl AsRef<[u8]>) -> &[u8] {
        let ui_name = c_str(ui_name.as_ref());
        if !ui_name.is_empty() {
            for entry in &self.entries {
                if string_utils::compare(ui_name, entry.ui_name()) {
                    return entry.name();
                }
            }
        }
        &[]
    }

    /// The UI name of the first item named `name` (ignoring case); `""` for none, or an empty
    /// name.
    ///
    /// Port of `ColorSpaceMenuHelperImpl::getUINameFromName` (ColorSpaceHelpers.cpp:696-709 @
    /// v2.5.2).
    #[doc(alias = "getUINameFromName")]
    pub fn ui_name_from_name(&self, name: impl AsRef<[u8]>) -> &[u8] {
        let name = c_str(name.as_ref());
        if !name.is_empty() {
            for entry in &self.entries {
                if string_utils::compare(name, entry.name()) {
                    return entry.ui_name();
                }
            }
        }
        &[]
    }

    /// The menu's text, as Python's `repr()` prints it: its parameters' text
    /// ([`ColorSpaceMenuParameters::to_bytes`]), then `, color spaces = [<names>]`.
    ///
    /// Port of `ColorSpaceMenuHelperImpl::serialize` and `operator<<(std::ostream &, const
    /// ColorSpaceMenuHelper &)` (ColorSpaceHelpers.cpp:711-734 @ v2.5.2).
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut os = self.parameters.to_bytes()?;
        os.extend_from_slice(b", color spaces = [");
        let mut first = true;
        for cs in &self.entries {
            if !first {
                os.extend_from_slice(b", ");
            }
            os.extend_from_slice(cs.name());
            first = false;
        }
        os.extend_from_slice(b"]");
        Ok(os)
    }
}

/// Adds the color space `color_space` to `config`, with the transform to its reference space
/// `user_transform`, then the connection color space's transform to the reference (or the
/// inverse of its transform from the reference).
///
/// Port of `ColorSpaceHelpers::AddColorSpace(ConfigRcPtr &, ColorSpaceRcPtr &,
/// FileTransformRcPtr &, const char *)` (ColorSpaceHelpers.cpp:741-801 @ v2.5.2), in its
/// anonymous namespace.
fn add_color_space_with_transform(
    config: &mut Config,
    color_space: &mut ColorSpace,
    user_transform: FileTransform,
    connection_color_space_name: &[u8],
) -> Result<()> {
    let connection_color_space_name = c_str(connection_color_space_name);
    if connection_color_space_name.is_empty() {
        return Err(Exception::new("Invalid connection color space name."));
    }

    // Check for a role and an active or inactive color space.
    if config.color_space(color_space.name()).is_some() {
        return Err(Exception::new(
            [
                b"Color space name '".as_slice(),
                color_space.name(),
                b"' already exists.",
            ]
            .concat(),
        ));
    }

    // Step 1 - Create the color transformation.

    let mut grp = GroupTransform::new();
    grp.append_transform(Transform::from(user_transform));

    // Check for an active or inactive color space.
    let Some(connection_cs) = config.color_space(connection_color_space_name) else {
        return Err(Exception::new(
            [
                b"Connection color space name '".as_slice(),
                connection_color_space_name,
                b"' does not exist.",
            ]
            .concat(),
        ));
    };

    if let Some(tr) = connection_cs.transform(ColorSpaceDirection::ToReference) {
        grp.append_transform(tr.create_editable_copy()?);
    } else if let Some(tr) = connection_cs.transform(ColorSpaceDirection::FromReference) {
        let mut t = tr.create_editable_copy()?;
        t.set_direction(combine_transform_directions(
            tr.direction(),
            TransformDirection::Inverse,
        ));
        grp.append_transform(t);
    }

    grp.validate()?;

    // Step 2 - Add the color space to the config.

    color_space.set_transform(
        Some(&Transform::from(grp)),
        ColorSpaceDirection::ToReference,
    )?;
    config.add_color_space(color_space)
}

/// Adds a color space of the name, family and description of `color_space_info` to `config`,
/// with the categories `categories` (a comma-separated list, in lower case) if one of the
/// config's active color spaces has one of them, and the transform `user_transform` then the
/// connection color space's to the reference.
///
/// Port of `ColorSpaceHelpers::AddColorSpace(ConfigRcPtr &, const ColorSpaceInfo &,
/// FileTransformRcPtr &, const char *, const char *)` (ColorSpaceHelpers.cpp:806-834 @
/// v2.5.2).
fn add_color_space_from_info(
    config: &mut Config,
    color_space_info: &ColorSpaceInfo,
    user_transform: FileTransform,
    categories: &[u8],
    connection_color_space_name: &[u8],
) -> Result<()> {
    let mut color_space = ColorSpace::new();

    color_space.set_name(color_space_info.name());
    color_space.set_family(color_space_info.family());
    color_space.set_description(color_space_info.description());

    let categories = c_str(categories);
    if !categories.is_empty() {
        let cats = extract_items(Some(categories));

        // Only add the categories if already used.
        let names = find_color_space_names(config, &cats);
        if !names.is_empty() {
            for cat in &cats {
                color_space.add_category(cat);
            }
        }
    }

    add_color_space_with_transform(
        config,
        &mut color_space,
        user_transform,
        connection_color_space_name,
    )
}

/// Adds to `config` a color space named `name`, whose transform to the reference space is the
/// file `transform_file_path` (a FileTransform) followed by the connection color space's
/// transform to the reference (or the inverse of its transform from the reference): the file
/// must convert to the connection color space. The color space gets the categories
/// `categories` (a comma-separated list, in lower case; may be empty) only if one of the
/// config's active color spaces already has one of them, so as not to change how a config
/// without categories shows in menus.
///
/// Fails with "Invalid connection color space name." for an empty connection name, "Color
/// space name '<name>' already exists." for a color space (or alias, or role) of that name,
/// "Connection color space name '<name>' does not exist.", and what validating the
/// transforms and `Config::add_color_space` refuse; `config` is unchanged then.
///
/// Port of `ColorSpaceHelpers::AddColorSpace(ConfigRcPtr &, const char *, const char *, const
/// char *, const char *)` (include/OpenColorIO/OpenColorAppHelpers.h:287-291,
/// ColorSpaceHelpers.cpp:836-848 @ v2.5.2).
#[doc(alias = "AddColorSpace")]
pub fn add_color_space(
    config: &mut Config,
    name: impl AsRef<[u8]>,
    transform_file_path: impl AsRef<[u8]>,
    categories: impl AsRef<[u8]>,
    connection_color_space_name: impl AsRef<[u8]>,
) -> Result<()> {
    let info = ColorSpaceInfo::create(config, Some(name.as_ref()), None, None);

    let mut file = FileTransform::new();
    file.set_src(transform_file_path);

    add_color_space_from_info(
        config,
        &info,
        file,
        categories.as_ref(),
        connection_color_space_name.as_ref(),
    )
}

#[cfg(test)]
#[path = "color_space_helpers_tests.rs"]
mod tests;
