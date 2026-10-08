// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The config: a port of `src/OpenColorIO/Config.cpp` @ v2.5.2, in part: its state and
//! constructor, the copy, the versions, the name, description and family separator, the
//! environment and search paths, and the processors of a transform with their cache. Color
//! spaces, roles, displays, looks and the rest come with the later Phase 3 chunks.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, RwLock};

use ocio_ops::exception::{Exception, Result};
use ocio_ops::image_desc::PackedImageDesc;
use ocio_ops::logging::{log_error, log_info};
use ocio_ops::math_utils::equal_with_abs_error;
use ocio_ops::open_color_types::{
    CdlStyle, ChannelOrdering, ColorSpaceDirection, ColorSpaceVisibility, EnvironmentMode,
    FixedFunctionStyle, NamedTransformVisibility, NegativeStyle, OptimizationFlags,
    ReferenceSpaceType, SearchReferenceSpaceType, TransformDirection, ViewTransformDirection,
    ViewType, fixed_function_style_to_string,
};
use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;
use ocio_ops::parse_utils::{
    ROLE_COLOR_TIMING, ROLE_COMPOSITING_LOG, ROLE_DEFAULT, ROLE_INTERCHANGE_DISPLAY,
    ROLE_INTERCHANGE_SCENE, ROLE_SCENE_LINEAR, find_in_string_vec_case_ignore,
    intersect_string_vecs_case_ignore, join_string_env_style, split_string_env_style,
};
use ocio_ops::platform::strcasecmp;
use ocio_ops::platform::{create_input_file_stream, getenv, is_env_present};
use ocio_ops::utils::pystring::{self, os_path};
use ocio_ops::utils::string_utils::{
    StringVec, c_str, compare, contain, find, lower, remove, split, starts_with, trim,
};

use crate::builtinconfigs::builtin_config_registry::{
    BuiltinConfigRegistry, OCIO_BUILTIN_URI_PREFIX, resolve_config_path, search_builtin_uri,
};
use crate::caching::{OCIO_DISABLE_CACHE_FALLBACK, ProcessorCache, std_hash_string};
use crate::color_space::ColorSpace;
use crate::color_space_set::ColorSpaceSet;
use crate::config_io_proxy::ConfigIoProxy;
use crate::context::Context;
use crate::context_variable_utils::{
    collect_context_variables, contains_context_variable_token, contains_context_variables,
};
use crate::display::{
    Display, DisplayMap, OCIO_VIEW_USE_DISPLAY_NAME, View, ViewVec, add_view, compute_displays,
    find_display, find_view,
};
use crate::file_rules::{FileRules, update_file_rules_from_v1_to_v2};
use crate::look::Look;
use crate::look_parse::LookParseResult;
use crate::named_transform::NamedTransform;
use crate::path_utils::{get_fast_file_hash, parse_color_space_from_string};
use crate::processor::{Processor, ProcessorCacheFlags};
use crate::transform::Transform;
use crate::view_transform::ViewTransform;
use crate::viewing_rules::{ViewingRules, find_rule};
use crate::yaml_cpp::stream::IStream;
use ocio_ops::hash_utils::cache_id_hash;

/// `OCIO_ACTIVE_DISPLAYS`: the displays a config shows, overriding its own list.
///
/// `$OCIO`: the config that [`Config::create_from_env`] reads.
///
/// Port of `OCIO_CONFIG_ENVVAR` (src/OpenColorIO/Config.cpp:47 @ v2.5.2).
pub const OCIO_CONFIG_ENVVAR: &str = "OCIO";

/// Port of `OCIO_ACTIVE_DISPLAYS_ENVVAR` (src/OpenColorIO/Config.cpp:49 @ v2.5.2).
pub const OCIO_ACTIVE_DISPLAYS_ENVVAR: &str = "OCIO_ACTIVE_DISPLAYS";

/// `OCIO_ACTIVE_VIEWS`: the views a config shows, overriding its own list.
///
/// Port of `OCIO_ACTIVE_VIEWS_ENVVAR` (src/OpenColorIO/Config.cpp:50 @ v2.5.2).
pub const OCIO_ACTIVE_VIEWS_ENVVAR: &str = "OCIO_ACTIVE_VIEWS";

/// `OCIO_INACTIVE_COLORSPACES`: the inactive color spaces, overriding the config's own list.
///
/// Port of `OCIO_INACTIVE_COLORSPACES_ENVVAR` (src/OpenColorIO/Config.cpp:51 @ v2.5.2).
pub const OCIO_INACTIVE_COLORSPACES_ENVVAR: &str = "OCIO_INACTIVE_COLORSPACES";

/// The Rec.709 luma coefficients, as specified by the ASC.
///
/// Port of `DEFAULT_LUMA_COEFF_R`, `_G` and `_B` (src/OpenColorIO/Config.cpp:68-70 @ v2.5.2).
const DEFAULT_LUMA_COEFFS: [f64; 3] = [0.2126, 0.7152, 0.0722];

/// `FirstSupportedMajorVersion`.
const FIRST_SUPPORTED_MAJOR_VERSION: u32 = 1;
/// `LastSupportedMajorVersion`: `OCIO_VERSION_MAJOR`.
const LAST_SUPPORTED_MAJOR_VERSION: u32 = 2;
/// `LastSupportedMinorVersion`: for each major version, the most recent minor.
///
/// Port of `FirstSupportedMajorVersion`, `LastSupportedMajorVersion` and
/// `LastSupportedMinorVersion` (src/OpenColorIO/Config.cpp:245-251 @ v2.5.2).
const LAST_SUPPORTED_MINOR_VERSION: [u32; 2] = [0, 5];

/// Port of `Config::Impl::DefaultFamilySeparator` (src/OpenColorIO/Config.cpp:270 @ v2.5.2).
const DEFAULT_FAMILY_SEPARATOR: u8 = b'/';

/// The raw config's profile ([`Config::create_raw`]).
///
/// Port of `INTERNAL_RAW_PROFILE` (src/OpenColorIO/Config.cpp:74-92 @ v2.5.2).
pub(crate) const INTERNAL_RAW_PROFILE: &str = concat!(
    "ocio_profile_version: 2\n",
    "strictparsing: false\n",
    "roles:\n",
    "  default: raw\n",
    "file_rules:\n",
    "  - !<Rule> {name: Default, colorspace: default}\n",
    "displays:\n",
    "  sRGB:\n",
    "  - !<View> {name: Raw, colorspace: raw}\n",
    "colorspaces:\n",
    "  - !<ColorSpace>\n",
    "      name: raw\n",
    "      family: raw\n",
    "      equalitygroup:\n",
    "      bitdepth: 32f\n",
    "      isdata: true\n",
    "      allocation: uniform\n",
    "      description: 'A raw color space. Conversions to and from this space are no-ops.'\n",
);

/// The built-in transform styles a config needs version 2.2 for (src/OpenColorIO/Config.cpp:
/// 5603-5614 @ v2.5.2).
const BUILTIN_STYLES_2_2: [&str; 3] = [
    "ARRI_LOGC4_to_ACES2065-1",
    "CURVE - CANON_CLOG2_to_LINEAR",
    "CURVE - CANON_CLOG3_to_LINEAR",
];

/// The built-in transform styles a config needs version 2.4 for (src/OpenColorIO/Config.cpp:
/// 5620-5666 @ v2.5.2), the last one added in OCIO 2.4.1.
const BUILTIN_STYLES_2_4: [&str; 38] = [
    "APPLE_LOG_to_ACES2065-1",
    "CURVE - APPLE_LOG_to_LINEAR",
    "CURVE - HLG-OETF",
    "CURVE - HLG-OETF-INVERSE",
    "DISPLAY - CIE-XYZ-D65_to_DCDM-D65",
    "DISPLAY - CIE-XYZ-D65_to_ST2084-DCDM-D65",
    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-REC709_2.0",
    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-P3-D65_2.0",
    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-108nit-P3-D65_2.0",
    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-300nit-P3-D65_2.0",
    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-500nit-P3-D65_2.0",
    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-P3-D65_2.0",
    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-2000nit-P3-D65_2.0",
    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-P3-D65_2.0",
    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-500nit-REC2020_2.0",
    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-REC2020_2.0",
    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-2000nit-REC2020_2.0",
    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-REC2020_2.0",
    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-REC709-D60-in-REC709-D65_2.0",
    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-REC709-D60-in-P3-D65_2.0",
    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-REC709-D60-in-REC2020-D65_2.0",
    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-P3-D60-in-P3-D65_2.0",
    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-P3-D60-in-XYZ-E_2.0",
    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-108nit-P3-D60-in-P3-D65_2.0",
    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-300nit-P3-D60-in-XYZ-E_2.0",
    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-500nit-P3-D60-in-P3-D65_2.0",
    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-P3-D60-in-P3-D65_2.0",
    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-2000nit-P3-D60-in-P3-D65_2.0",
    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-P3-D60-in-P3-D65_2.0",
    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-500nit-P3-D60-in-REC2020-D65_2.0",
    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-P3-D60-in-REC2020-D65_2.0",
    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-2000nit-P3-D60-in-REC2020-D65_2.0",
    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-P3-D60-in-REC2020-D65_2.0",
    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-500nit-REC2020-D60-in-REC2020-D65_2.0",
    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-REC2020-D60-in-REC2020-D65_2.0",
    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-2000nit-REC2020-D60-in-REC2020-D65_2.0",
    "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-REC2020-D60-in-REC2020-D65_2.0",
    "DISPLAY - CIE-XYZ-D65_to_DisplayP3-HDR",
];

/// The default value of the environment variable `name`, `""` when the config has none.
///
/// Port of `LookupEnvironment` (src/OpenColorIO/Config.cpp:132-138 @ v2.5.2).
fn lookup_environment<'a>(env: &'a BTreeMap<Vec<u8>, Vec<u8>>, name: &[u8]) -> &'a [u8] {
    env.get(name).map_or(&[], Vec::as_slice)
}

/// The color space of the role `rolename`, ignoring case; `""` when there is no such role.
/// The roles are stored by their lower-case names.
///
/// Port of `LookupRole` (src/OpenColorIO/Config.cpp:140-148 @ v2.5.2).
fn lookup_role<'a>(roles: &'a BTreeMap<Vec<u8>, Vec<u8>>, rolename: &[u8]) -> &'a [u8] {
    roles.get(&lower(rolename)).map_or(&[], Vec::as_slice)
}

/// Adds to `files` the sources of the FileTransforms of `transform` (its own, or a group's,
/// recursively), each up to its first NUL.
///
/// Port of `GetFileReferences` (src/OpenColorIO/Config.cpp:154-171 @ v2.5.2).
fn get_file_references(files: &mut BTreeSet<Vec<u8>>, transform: &Transform) {
    match transform {
        Transform::Group(group_transform) => {
            for i in 0..group_transform.num_transforms() {
                if let Ok(t) = group_transform.transform(i) {
                    get_file_references(files, t);
                }
            }
        }
        Transform::File(file_transform) => {
            files.insert(c_str(file_transform.src()).to_vec());
        }
        _ => {}
    }
}

/// The start of a view's validation message: "Config failed display view validation. ", then
/// "Shared " (`display` empty) or "Display '<display>' has a ", then "view with an empty name."
/// or "view '<name>' ".
///
/// Port of `GetDisplayViewPrefixErrorMsg` (src/OpenColorIO/Config.cpp:222-243 @ v2.5.2).
fn get_display_view_prefix_error_msg(display: &[u8], view: &View) -> Vec<u8> {
    let mut oss = b"Config failed display view validation. ".to_vec();
    if display.is_empty() {
        oss.extend_from_slice(b"Shared ");
    } else {
        oss.extend_from_slice(b"Display '");
        oss.extend_from_slice(display);
        oss.extend_from_slice(b"' has a ");
    }
    if view.name.is_empty() {
        oss.extend_from_slice(b"view with an empty name.");
    } else {
        oss.extend_from_slice(b"view '");
        oss.extend_from_slice(&view.name);
        oss.extend_from_slice(b"' ");
    }
    oss
}

/// Whether a color space of the reference space `t` is one a search of `st` keeps.
///
/// Port of `MatchReferenceType` (src/OpenColorIO/Config.cpp:2316-2330 @ v2.5.2).
fn match_reference_type(st: SearchReferenceSpaceType, t: ReferenceSpaceType) -> bool {
    match st {
        SearchReferenceSpaceType::Scene => t == ReferenceSpaceType::Scene,
        SearchReferenceSpaceType::Display => t == ReferenceSpaceType::Display,
        SearchReferenceSpaceType::All => true,
    }
}

/// What a list of inactive names is built for.
///
/// Port of `Config::Impl::InactiveType` (src/OpenColorIO/Config.cpp:521-526 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InactiveType {
    /// `INACTIVE_COLORSPACE`.
    ColorSpace,
    /// `INACTIVE_NAMEDTRANSFORM`.
    NamedTransform,
    /// `INACTIVE_ALL`: the names as they are, for `validate`.
    All,
}

/// A config's current context, shared with the config: what it reads is the context as the
/// config holds it at that moment, so a handle taken before a change of the config (its
/// environment, search paths, working directory) sees the change, as the pointer upstream's
/// `getCurrentContext` returns does. The handle only reads the context.
///
/// Port of the `ConstContextRcPtr` of `Config::getCurrentContext`
/// (src/OpenColorIO/Config.cpp:2161-2164 @ v2.5.2): the config and the callers share one
/// context object (`Config::Impl::m_context`).
#[derive(Clone)]
pub struct CurrentContext(Arc<RwLock<Arc<Context>>>);

impl CurrentContext {
    fn new(context: Arc<Context>) -> CurrentContext {
        CurrentContext(Arc::new(RwLock::new(context)))
    }

    /// The context as the config holds it now.
    pub fn get(&self) -> Arc<Context> {
        self.0.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Changes the shared context through `change`, as the config sees it from then on. It
    /// keeps the config's cache IDs and processors, as a change through upstream's context
    /// does. A copy of the context is changed (with its environment mode, which
    /// `Context::createEditableCopy` doesn't keep) when a caller holds one from
    /// [`CurrentContext::get`], which keeps the state it had.
    ///
    /// C++ callers get a `ConstContextRcPtr`, which they can't change; the Python binding casts
    /// the constness away, so a Python caller changes the config's context through it
    /// (`getCurrentContext().setSearchPath(...)`). This is for the Python module.
    #[doc(hidden)]
    pub fn update(&self, change: impl FnOnce(&mut Context)) {
        let mut cell = self.0.write().unwrap_or_else(|e| e.into_inner());
        if Arc::get_mut(&mut cell).is_none() {
            let mode = cell.environment_mode();
            let mut copy = (**cell).clone();
            copy.set_environment_mode(mode);
            *cell = Arc::new(copy);
        }
        change(Arc::get_mut(&mut cell).expect("a context of its own"));
    }
}

impl fmt::Debug for CurrentContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&*self.get(), f)
    }
}

/// A config's file rules or viewing rules, shared with the config: what it reads is the rules
/// as the config holds them at that moment. `setFileRules` and `setViewingRules` give the
/// config a new copy, and a handle taken before keeps the rules it had; the config's copy
/// (`createEditableCopy`) copies them too. The handle only reads the rules.
///
/// Port of the `ConstFileRulesRcPtr` and `ConstViewingRulesRcPtr` of `Config::getFileRules`
/// and `Config::getViewingRules` (src/OpenColorIO/Config.cpp:4643-4646, 3332-3335 @ v2.5.2):
/// the config and the callers share one rules object (`m_fileRules`, `m_viewingRules`).
pub struct ConfigRules<T>(Arc<RwLock<Arc<T>>>);

impl<T> Clone for ConfigRules<T> {
    fn clone(&self) -> ConfigRules<T> {
        ConfigRules(self.0.clone())
    }
}

impl<T: Clone> ConfigRules<T> {
    fn new(rules: T) -> ConfigRules<T> {
        ConfigRules(Arc::new(RwLock::new(Arc::new(rules))))
    }

    /// The rules as the config holds them now.
    pub fn get(&self) -> Arc<T> {
        self.0.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Changes the shared rules through `change`, as the config sees them from then on. It
    /// keeps the config's cache IDs and processors, as a change through upstream's rules
    /// does. A copy of the rules is changed when a caller holds them from
    /// [`ConfigRules::get`], which keeps the state it had.
    ///
    /// C++ callers get a const pointer, which they can't change; the Python binding casts the
    /// constness away, so a Python caller changes the config's rules through it
    /// (`getFileRules().insertRule(...)`). This is for the Python module.
    #[doc(hidden)]
    pub fn update<R>(&self, change: impl FnOnce(&mut T) -> R) -> R {
        let mut cell = self.0.write().unwrap_or_else(|e| e.into_inner());
        change(Arc::make_mut(&mut cell))
    }
}

impl<T: Clone + fmt::Debug> fmt::Debug for ConfigRules<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&*self.get(), f)
    }
}

/// The names of `views`.
///
/// Port of `GetViewNames` (src/OpenColorIO/Config.cpp:202-210 @ v2.5.2).
fn get_view_names(views: &[&View]) -> StringVec {
    views.iter().map(|view| view.name.clone()).collect()
}

/// Adds the color spaces `transform` names, and the groups it holds name, to
/// `color_space_names`: a color space transform's source and destination (their context
/// variables resolved), a display view transform's source, a look transform's source and
/// destination.
///
/// Port of `GetColorSpaceReferences` (src/OpenColorIO/Config.cpp:171-208 @ v2.5.2).
fn get_color_space_references(
    color_space_names: &mut BTreeSet<Vec<u8>>,
    transform: &Transform,
    context: &Context,
) {
    match transform {
        Transform::Group(group) => {
            for i in 0..group.num_transforms() {
                if let Ok(t) = group.transform(i) {
                    get_color_space_references(color_space_names, t, context);
                }
            }
        }
        Transform::ColorSpace(t) => {
            color_space_names.insert(context.resolve_string_var(t.src()));
            color_space_names.insert(context.resolve_string_var(t.dst()));
        }
        Transform::DisplayView(t) => {
            color_space_names.insert(t.src().to_vec());
        }
        Transform::Look(t) => {
            color_space_names.insert(t.src().to_vec());
            color_space_names.insert(t.dst().to_vec());
        }
        _ => {}
    }
}

/// The result of the config's last validation.
///
/// Port of `Config::Impl::Validation` (src/OpenColorIO/Config.cpp:257-262 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Validation {
    /// `VALIDATION_UNKNOWN`.
    Unknown,
    /// `VALIDATION_PASSED`.
    Passed,
    /// `VALIDATION_FAILED`.
    Failed,
}

/// What the config computes and keeps, which every change resets: the result of its
/// validation and its cache IDs. Upstream keeps them in `mutable` members, the cache IDs under
/// `m_cacheidMutex`. `validate` (3.8a) and `getCacheID` (3.7d) fill them.
///
/// Port of `Config::Impl`'s `m_validation`, `m_validationtext`, `m_cacheids` and
/// `m_cacheidnocontext` (src/OpenColorIO/Config.cpp:318-323 @ v2.5.2).
#[derive(Debug, Clone)]
struct CacheIds {
    /// `m_validation`.
    validation: Validation,
    /// `m_validationtext`.
    validation_text: Vec<u8>,
    /// `m_cacheids`: the config's cache ID by the context's cache ID.
    cache_ids: BTreeMap<String, String>,
    /// `m_cacheidnocontext`.
    cache_id_no_context: String,
}

impl CacheIds {
    fn new() -> CacheIds {
        CacheIds {
            validation: Validation::Unknown,
            validation_text: Vec::new(),
            cache_ids: BTreeMap::new(),
            cache_id_no_context: String::new(),
        }
    }
}

/// A config.
///
/// A copy is upstream's `createEditableCopy`: a config with the same state and cache flags,
/// copies of its objects, and an empty cache of processors.
///
/// Port of `Config` and `Config::Impl` (include/OpenColorIO/OpenColorIO.h:285,
/// src/OpenColorIO/Config.cpp:253-1112 @ v2.5.2), in part.
#[derive(Debug)]
pub struct Config {
    /// `m_majorVersion`.
    major_version: u32,
    /// `m_minorVersion`.
    minor_version: u32,
    /// `m_env`: the environment variables and their default values.
    env: BTreeMap<Vec<u8>, Vec<u8>>,
    /// `m_context`, which the callers of `getCurrentContext` share.
    shared_context: CurrentContext,
    /// `m_name`.
    name: Vec<u8>,
    /// `m_familySeparator`.
    family_separator: u8,
    /// `m_description`.
    description: Vec<u8>,
    /// `m_allColorSpaces`: all the color spaces, active or not.
    all_color_spaces: ColorSpaceSet,
    /// `m_activeColorSpaceNames`.
    active_color_space_names: StringVec,
    /// `m_inactiveColorSpaceNames`.
    inactive_color_space_names: StringVec,
    /// `m_inactiveColorSpaceNamesAPI`: the inactive color spaces and named transforms an API
    /// request gives, which supersedes the two lists below.
    inactive_color_space_names_api: Vec<u8>,
    /// `m_inactiveColorSpaceNamesEnv`: `OCIO_INACTIVE_COLORSPACES`, trimmed, which supersedes
    /// the config's list.
    inactive_color_space_names_env: Vec<u8>,
    /// `m_inactiveColorSpaceNamesConf`: the config's list (`inactive_colorspaces`).
    inactive_color_space_names_conf: Vec<u8>,
    /// `m_roles`: the color space of each role, by the role's lower-case name.
    roles: BTreeMap<Vec<u8>, Vec<u8>>,
    /// `m_displays`: the displays, in config order.
    displays: DisplayMap,
    /// `m_activeDisplays`: the config's list of active displays.
    active_displays: StringVec,
    /// `m_activeViews`: the config's list of active views.
    active_views: StringVec,
    /// `m_sharedViews`: the config's shared views.
    shared_views: ViewVec,
    /// `m_displayCache` (`mutable`): the active displays, computed when first needed.
    display_cache: OnceLock<StringVec>,
    /// `m_virtualDisplay`: the views of the virtual display. It is temporary, so that the
    /// displays instantiated from it aren't saved.
    virtual_display: Display,
    /// `m_activeDisplaysEnvOverride`: the displays of `OCIO_ACTIVE_DISPLAYS`.
    active_displays_env_override: StringVec,
    /// `m_activeViewsEnvOverride`: the views of `OCIO_ACTIVE_VIEWS`.
    active_views_env_override: StringVec,
    /// `m_looksList`.
    looks_list: Vec<Look>,
    /// `m_viewTransforms`.
    view_transforms: Vec<ViewTransform>,
    /// `m_defaultViewTransform`: the name of the default view transform.
    default_view_transform: Vec<u8>,
    /// `m_allNamedTransforms`: all the named transforms, active or not.
    all_named_transforms: Vec<NamedTransform>,
    /// `m_activeNamedTransformNames`.
    active_named_transform_names: StringVec,
    /// `m_inactiveNamedTransformNames`.
    inactive_named_transform_names: StringVec,
    /// `m_defaultLumaCoefs`.
    default_luma_coefs: [f64; 3],
    /// `m_strictParsing`.
    strict_parsing: bool,
    /// `m_fileRules`.
    file_rules: ConfigRules<FileRules>,
    /// `m_viewingRules`.
    viewing_rules: ConfigRules<ViewingRules>,
    /// The validation and the cache IDs, under `m_cacheidMutex`.
    cache_ids: Mutex<CacheIds>,
    /// `m_cacheFlags` (`mutable`: a const config changes it).
    cache_flags: AtomicU32,
    /// `m_processorCache`: the processors, by `std::hash` of their key.
    processor_cache: ProcessorCache<u64, Arc<Processor>>,
}

impl Clone for Config {
    /// A copy with the same state and cache flags, and an empty processor cache, enabled by
    /// them. The context is copied too (`Context::createEditableCopy`, whose copy has the
    /// default environment mode, docs/improvements.md I-111).
    ///
    /// Port of `Config::createEditableCopy` and `Config::Impl::operator=`
    /// (src/OpenColorIO/Config.cpp:379-457, 1352-1357 @ v2.5.2), in part.
    fn clone(&self) -> Config {
        let context = Arc::new((*self.shared_context.get()).clone());
        let config = Config {
            major_version: self.major_version,
            minor_version: self.minor_version,
            env: self.env.clone(),
            shared_context: CurrentContext::new(context),
            name: self.name.clone(),
            family_separator: self.family_separator,
            description: self.description.clone(),
            all_color_spaces: self.all_color_spaces.clone(),
            active_color_space_names: self.active_color_space_names.clone(),
            inactive_color_space_names: self.inactive_color_space_names.clone(),
            inactive_color_space_names_api: self.inactive_color_space_names_api.clone(),
            inactive_color_space_names_env: self.inactive_color_space_names_env.clone(),
            inactive_color_space_names_conf: self.inactive_color_space_names_conf.clone(),
            roles: self.roles.clone(),
            looks_list: self.looks_list.clone(),
            view_transforms: self.view_transforms.clone(),
            default_view_transform: self.default_view_transform.clone(),
            all_named_transforms: self.all_named_transforms.clone(),
            active_named_transform_names: self.active_named_transform_names.clone(),
            inactive_named_transform_names: self.inactive_named_transform_names.clone(),
            displays: self.displays.clone(),
            active_displays: self.active_displays.clone(),
            active_views: self.active_views.clone(),
            shared_views: self.shared_views.clone(),
            display_cache: self.display_cache.clone(),
            virtual_display: self.virtual_display.clone(),
            active_displays_env_override: self.active_displays_env_override.clone(),
            active_views_env_override: self.active_views_env_override.clone(),
            default_luma_coefs: self.default_luma_coefs,
            strict_parsing: self.strict_parsing,
            file_rules: ConfigRules::new((*self.file_rules.get()).clone()),
            viewing_rules: ConfigRules::new((*self.viewing_rules.get()).clone()),
            cache_ids: Mutex::new(self.lock_cache_ids().clone()),
            cache_flags: AtomicU32::new(self.cache_flags.load(Ordering::Relaxed)),
            processor_cache: ProcessorCache::new(),
        };
        config.processor_cache.clear();
        config.processor_cache.enable(
            config
                .processor_cache_flags()
                .has_flag(ProcessorCacheFlags::ENABLED),
        );
        config
    }
}

/// The C library's `strerror` of an I/O error: its text without Rust's " (os error N)".
fn strerror(e: &std::io::Error) -> String {
    let text = e.to_string();
    match (e.raw_os_error(), text.rfind(" (os error ")) {
        (Some(_), Some(at)) => text[..at].to_string(),
        _ => text,
    }
}

/// `g_currentConfig`, under its lock `g_currentConfigLock` (Config.cpp:110-113 @ v2.5.2).
static CURRENT_CONFIG: Mutex<Option<Arc<Config>>> = Mutex::new(None);

/// The process's current config: the one [`set_current_config`] gave, or, the first time
/// without one, [`Config::create_from_env`]'s (whose error leaves no current config).
///
/// Port of `GetCurrentConfig` (src/OpenColorIO/Config.cpp:115-125 @ v2.5.2).
#[doc(alias = "GetCurrentConfig")]
pub fn get_current_config() -> Result<Arc<Config>> {
    let mut current = CURRENT_CONFIG
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    if current.is_none() {
        *current = Some(Config::create_from_env()?);
    }

    Ok(Arc::clone(current.as_ref().expect("set above")))
}

/// Makes a copy of `config` ([`Config`]'s `Clone`, `createEditableCopy`) the process's current
/// config.
///
/// Port of `SetCurrentConfig` (src/OpenColorIO/Config.cpp:127-132 @ v2.5.2).
#[doc(alias = "SetCurrentConfig")]
pub fn set_current_config(config: &Config) {
    let mut current = CURRENT_CONFIG
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    *current = Some(Arc::new(config.clone()));
}

impl Config {
    /// The state `Config::Impl::Impl` gives a config before it reads the environment.
    fn blank() -> Config {
        let context = Arc::new(Context::new());
        let config = Config {
            major_version: LAST_SUPPORTED_MAJOR_VERSION,
            minor_version: LAST_SUPPORTED_MINOR_VERSION[LAST_SUPPORTED_MAJOR_VERSION as usize - 1],
            env: BTreeMap::new(),
            shared_context: CurrentContext::new(context),
            name: Vec::new(),
            family_separator: DEFAULT_FAMILY_SEPARATOR,
            description: Vec::new(),
            all_color_spaces: ColorSpaceSet::new(),
            active_color_space_names: StringVec::new(),
            inactive_color_space_names: StringVec::new(),
            inactive_color_space_names_api: Vec::new(),
            inactive_color_space_names_env: Vec::new(),
            inactive_color_space_names_conf: Vec::new(),
            roles: BTreeMap::new(),
            looks_list: Vec::new(),
            view_transforms: Vec::new(),
            default_view_transform: Vec::new(),
            all_named_transforms: Vec::new(),
            active_named_transform_names: StringVec::new(),
            inactive_named_transform_names: StringVec::new(),
            displays: DisplayMap::new(),
            active_displays: StringVec::new(),
            active_views: StringVec::new(),
            shared_views: ViewVec::new(),
            display_cache: OnceLock::new(),
            // This is used to allow the YAML writer to not save any virtual displays that were
            // instantiated.
            virtual_display: Display {
                temporary: true,
                ..Display::default()
            },
            active_displays_env_override: StringVec::new(),
            active_views_env_override: StringVec::new(),
            default_luma_coefs: DEFAULT_LUMA_COEFFS,
            strict_parsing: true,
            file_rules: ConfigRules::new(FileRules::new()),
            viewing_rules: ConfigRules::new(ViewingRules::new()),
            cache_ids: Mutex::new(CacheIds::new()),
            cache_flags: AtomicU32::new(ProcessorCacheFlags::DEFAULT.0),
            processor_cache: ProcessorCache::new(),
        };
        config.processor_cache.enable(
            config
                .processor_cache_flags()
                .has_flag(ProcessorCacheFlags::ENABLED),
        );
        config
    }

    /// A new config: version 2.5, nothing in it, the default luma coefficients, strict
    /// parsing, and the active displays, active views and inactive color spaces of the
    /// environment (`OCIO_ACTIVE_DISPLAYS`, `OCIO_ACTIVE_VIEWS`, `OCIO_INACTIVE_COLORSPACES`).
    /// Fails when the list of active displays or views opens a quote it doesn't close before a
    /// separator (`SplitStringEnvStyle`).
    ///
    /// Port of `Config::Create` and `Config::Impl::Impl` (src/OpenColorIO/Config.cpp:325-374,
    /// 1117-1120 @ v2.5.2).
    #[doc(alias = "Create")]
    pub fn new() -> Result<Config> {
        let mut config = Config::blank();

        let active_displays = getenv(OCIO_ACTIVE_DISPLAYS_ENVVAR).unwrap_or_default();
        let active_displays = trim(&active_displays);
        if !active_displays.is_empty() {
            config.active_displays_env_override = split_string_env_style(active_displays)?;
        }

        let active_views = getenv(OCIO_ACTIVE_VIEWS_ENVVAR).unwrap_or_default();
        let active_views = trim(&active_views);
        if !active_views.is_empty() {
            config.active_views_env_override = split_string_env_style(active_views)?;
        }

        let inactive = getenv(OCIO_INACTIVE_COLORSPACES_ENVVAR).unwrap_or_default();
        config.inactive_color_space_names_env = trim(&inactive).to_vec();

        Ok(config)
    }

    fn lock_cache_ids(&self) -> MutexGuard<'_, CacheIds> {
        self.cache_ids.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Resets what the config computed from its state: the validation, the cache IDs, and the
    /// cache of processors. Every change of the state calls it.
    ///
    /// Port of `Config::Impl::resetCacheIDs` (src/OpenColorIO/Config.cpp:5462-5474 @ v2.5.2).
    fn reset_cache_ids(&mut self) {
        let cache_ids = self.cache_ids.get_mut().unwrap_or_else(|e| e.into_inner());
        cache_ids.cache_ids.clear();
        cache_ids.cache_id_no_context.clear();
        cache_ids.validation = Validation::Unknown;
        cache_ids.validation_text.clear();

        // As any changes could impact the cache keys, it's better to always flush the cache
        // of processors to not keep in memory useless instances.
        self.processor_cache.clear();
    }

    /// Changes the current context through `change`, as the callers of `getCurrentContext` see
    /// it too.
    fn update_context(&mut self, change: impl FnOnce(&mut Context)) {
        self.shared_context.update(change);
    }

    /// The raw config, read from upstream's internal profile ([`INTERNAL_RAW_PROFILE`]):
    /// version 2.0, no strict parsing, its one color space `raw` and the role `default`, its
    /// display `sRGB` and view `Raw`, and the whole environment in its context (the profile has
    /// no `environment` section). As any config, it reads the environment's active displays and
    /// views and inactive color spaces ([`Config::new`]), and fails as [`Config::new`] does.
    ///
    /// Port of `Config::CreateRaw` (src/OpenColorIO/Config.cpp:1127-1133 @ v2.5.2).
    #[doc(alias = "CreateRaw")]
    pub fn create_raw() -> Result<Arc<Config>> {
        Config::create_from_stream(INTERNAL_RAW_PROFILE.as_bytes())
    }

    /// A config read from the YAML text `istream`, or the reader's error, "Error: Loading the
    /// OCIO profile failed. ..." (see [`Config::read`]).
    ///
    /// Port of `Config::CreateFromStream` (src/OpenColorIO/Config.cpp:1209-1212 @ v2.5.2).
    #[doc(alias = "CreateFromStream")]
    pub fn create_from_stream(istream: &[u8]) -> Result<Arc<Config>> {
        Config::read(istream, None)
    }

    /// The built-in config that `config_name` names, with or without the `ocio://` prefix:
    /// a name of the registry, ignoring case, or `default`, `cg-config-latest` or
    /// `studio-config-latest` ([`resolve_config_path`]). Otherwise "Could not find '<name>' in
    /// the built-in configurations.". The name ends at its first NUL.
    ///
    /// Port of `Config::CreateFromBuiltinConfig` (src/OpenColorIO/Config.cpp:1233-1262 @
    /// v2.5.2).
    #[doc(alias = "CreateFromBuiltinConfig")]
    pub fn create_from_builtin_config(config_name: impl AsRef<[u8]>) -> Result<Arc<Config>> {
        let mut builtin_config_name = c_str(config_name.as_ref()).to_vec();

        // Normalize the input to the URI format.
        if !starts_with(&builtin_config_name, OCIO_BUILTIN_URI_PREFIX.as_bytes()) {
            let mut uri = OCIO_BUILTIN_URI_PREFIX.as_bytes().to_vec();
            uri.extend_from_slice(&builtin_config_name);
            builtin_config_name = uri;
        }

        // Resolve the URI if needed.
        let uri = resolve_config_path(&builtin_config_name).to_vec();

        // Check if the config path starts with ocio://
        if let Some(name) = search_builtin_uri(&uri) {
            // Store config path without the "ocio://" prefix, if present.
            builtin_config_name = name.to_vec();
        }

        let reg = BuiltinConfigRegistry::get();

        // getBuiltinConfigByName will throw if config name not found.
        let builtin_config_str = reg.builtin_config_by_name(&builtin_config_name)?;
        Config::create_from_stream(builtin_config_str)
    }

    /// The config that `$OCIO` names (a file, an archive or a built-in config's URI, read by
    /// [`Config::create_from_file`]); without it, or empty, the raw config, after logging
    /// "Color management disabled. (Specify the $OCIO environment variable to enable.)".
    ///
    /// Port of `Config::CreateFromEnv` (src/OpenColorIO/Config.cpp:1135-1152 @ v2.5.2).
    #[doc(alias = "CreateFromEnv")]
    pub fn create_from_env() -> Result<Arc<Config>> {
        let file = getenv(OCIO_CONFIG_ENVVAR).unwrap_or_default();

        // File may be one of the following:
        //   1) Path to a config file (e.g. /home/user/ocio/config.ocio)
        //   2) Path to an archived config file (e.g. /home/user/ocio/archived_config.ocioz)
        //   3) URI to a built-in config (e.g. ocio://cg-config-v0.1.0_aces-v1.3_ocio-v2.1.1)
        if !file.is_empty() {
            return Config::create_from_file(&file);
        }

        log_info("Color management disabled. (Specify the $OCIO environment variable to enable.)");

        Config::create_raw()
    }

    /// The config in the file `filename` (up to its first NUL), or the built-in config of the
    /// `ocio://` URI in it ([`Config::create_from_builtin_config`], docs/improvements.md I-149).
    /// The errors: "The config filepath is missing." (a missing-file exception) for an
    /// empty name, "Error could not read '<filename>' OCIO profile." for a file that can't be
    /// opened, and the reader's, which names the file.
    ///
    /// A file that starts with `PK` is an OCIOZ archive, whose reader is not ported yet (WP
    /// 4.x): an error. The file's bytes are read as `CreateFromFile`'s `std::ifstream` gives
    /// them to yaml-cpp ([`IStream::file`]): on Windows, a file of fewer than four bytes that
    /// makes yaml-cpp put back three of them reads as an empty document.
    ///
    /// Port of `Config::CreateFromFile` (src/OpenColorIO/Config.cpp:1154-1207 @ v2.5.2).
    #[doc(alias = "CreateFromFile")]
    pub fn create_from_file(filename: impl AsRef<[u8]>) -> Result<Arc<Config>> {
        let filename = c_str(filename.as_ref());
        if filename.is_empty() {
            return Err(Exception::missing_file("The config filepath is missing."));
        }

        // Check for URI Pattern: ocio://<config name>
        if search_builtin_uri(filename).is_some() {
            return Config::create_from_builtin_config(filename);
        }

        let could_not_read = || {
            let mut os = b"Error could not read '".to_vec();
            os.extend_from_slice(filename);
            os.extend_from_slice(b"' OCIO profile.");
            Exception::new(os)
        };
        let mut ifstream = create_input_file_stream(filename).map_err(|_| could_not_read())?;

        // The stream's bytes, up to a read that fails (a directory, which Linux opens).
        let mut data = Vec::new();
        let read = std::io::Read::read_to_end(&mut ifstream, &mut data);
        drop(ifstream);

        if data.starts_with(b"PK") {
            // The file should be an OCIOZ archive file.
            return Err(Exception::new(
                "Config::CreateFromFile: reading an OCIOZ archive is not ported yet.",
            ));
        }

        // A read that fails ends MSVC's file stream, as the end of the file does. libstdc++'s
        // throws `std::ios_base::failure` from `basic_filebuf::underflow`: the stream's own reads
        // swallow it, but yaml-cpp reads its bytes from the stream buffer (`sgetn`), so it
        // reaches `OCIOYaml::Read`, which wraps its `what()`. (The port fails before reading the
        // bytes before the failure; only a directory has been seen to fail, at the first read.)
        if let Err(e) = read
            && !cfg!(windows)
        {
            let mut os = b"Error: Loading the OCIO profile '".to_vec();
            os.extend_from_slice(filename);
            os.extend_from_slice(b"' failed. basic_filebuf::underflow error reading the file: ");
            os.extend_from_slice(strerror(&e).as_bytes());
            return Err(Exception::new(os));
        }

        // Not an OCIOZ archive. Continue as usual.
        Config::read(IStream::file(&data), Some(filename))
    }

    /// The config whose text `ciop` gives ([`ConfigIoProxy::config_data`]), read with the proxy:
    /// the reader's errors name the file "from Archive/ConfigIOProxy" (and the config's working
    /// directory isn't set from it), and the config keeps the proxy for its LUT files.
    ///
    /// Port of `Config::CreateFromConfigIOProxy` (src/OpenColorIO/Config.cpp:1214-1231 @
    /// v2.5.2) and `Config::Impl::Read(std::istream&, ConfigIOProxyRcPtr)` (Config.cpp:
    /// 5564-5584). Upstream's check for a null config can't fail: the reader throws instead.
    #[doc(alias = "CreateFromConfigIOProxy")]
    pub fn create_from_config_io_proxy(ciop: Arc<dyn ConfigIoProxy>) -> Result<Arc<Config>> {
        // Get a stream of the config.
        let config_str = ciop.config_data()?;

        let mut config = Config::new()?;
        // Passing special string for the file path to enable the parser to provide a more
        // meaningful error message if a problem is encountered.  (The working directory is not
        // set to this string.)
        crate::ocio_yaml::read(
            config_str.as_slice(),
            &mut config,
            Some(b"from Archive/ConfigIOProxy"),
        )?;

        config.check_version_consistency()?;

        // An API request always supersedes the env. variable. As the OCIOYaml helper methods
        // use the Config public API, the variable reset highlights that only the
        // env. variable and the config contents are valid after a config file read.
        config.inactive_color_space_names_api.clear();
        config.refresh_active_color_spaces();

        // Set the ConfigIOProxy object.
        config.set_config_io_proxy(Some(ciop));

        Ok(Arc::new(config))
    }

    /// Gives the config's context the I/O proxy `ciop` (or none), and resets its cache IDs.
    ///
    /// Port of `Config::setConfigIOProxy` (src/OpenColorIO/Config.cpp:5995-6001 @ v2.5.2).
    #[doc(alias = "setConfigIOProxy")]
    pub fn set_config_io_proxy(&mut self, ciop: Option<Arc<dyn ConfigIoProxy>>) {
        self.update_context(|c| c.set_config_io_proxy(ciop));

        self.reset_cache_ids();
    }

    /// The I/O proxy of the config's context.
    ///
    /// Port of `Config::getConfigIOProxy` (src/OpenColorIO/Config.cpp:6003-6006 @ v2.5.2).
    #[doc(alias = "getConfigIOProxy")]
    pub fn config_io_proxy(&self) -> Option<Arc<dyn ConfigIoProxy>> {
        self.shared_context.get().config_io_proxy().cloned()
    }

    /// Whether the config can be archived: its working directory is absolute, and none of its
    /// search paths and file transforms' files is absolute, starts with `..` once normalized, or
    /// starts with a context variable (`$` or `%` first).
    ///
    /// Port of `Config::isArchivable` (src/OpenColorIO/Config.cpp:6008-6082 @ v2.5.2).
    #[doc(alias = "isArchivable")]
    pub fn is_archivable(&self) -> bool {
        // Current archive implementation needs a working directory to look for LUT files and
        // working directory must be an absolute path.
        let working_directory = self.working_dir();
        if working_directory.is_empty() || !os_path::isabs(&working_directory) {
            return false;
        }

        // Utility lambda to check the following criteria.
        let validate_path_for_archiving = |path: &[u8]| {
            // Using the normalized path.
            let norm_path = os_path::normpath(path);
            // 1) Path may not be absolute.
            // 2) Path may not start with double dot ".." (going above working directory).
            // 3) A context variable may not be located at the start of the path.
            !(os_path::isabs(&norm_path)
                || pystring::startswith(&norm_path, b"..", 0, pystring::MAX_32BIT_INT)
                || (contains_context_variables(path)
                    && (find(path, b"$") == Some(0) || find(path, b"%") == Some(0))))
        };

        ///////////////////////////////
        // Search path verification. //
        ///////////////////////////////
        // Check that search paths are not absolute nor have context variables outside of config
        // working directory.
        let num_search_paths = self.num_search_paths();
        for i in 0..num_search_paths {
            let current_path = self.search_path_with_index(i);
            if !validate_path_for_archiving(&current_path) {
                // Exit and return false.
                return false;
            }
        }

        /////////////////////////////////
        // FileTransform verification. //
        /////////////////////////////////
        let mut files = BTreeSet::new();
        for transform in self.all_internal_transforms() {
            get_file_references(&mut files, transform);
        }

        // Check that FileTransform sources are not absolute nor have context variables outside
        // of config working directory.
        files.iter().all(|path| validate_path_for_archiving(path))
    }

    /// The config's YAML text: checked against its version
    /// ([`Config::check_version_consistency`]), then written (`OCIOYaml::Write`). Either's
    /// error is "Error building YAML: " and its message. It is also the config's text as
    /// upstream's `operator<<` writes it.
    ///
    /// Port of `Config::serialize(std::ostream&)` (src/OpenColorIO/Config.cpp:5317-5331 @
    /// v2.5.2) and `operator<<(std::ostream&, const Config&)` (Config.cpp:5236-5240), into a
    /// buffer: upstream writes nothing to the stream when it fails.
    pub fn serialize(&self) -> Result<Vec<u8>> {
        self.check_version_consistency()
            .and_then(|()| crate::ocio_yaml::write(self))
            .map_err(|e| {
                let mut error = b"Error building YAML: ".to_vec();
                error.extend_from_slice(e.what());
                Exception::new(error)
            })
    }

    /// The config's cache ID with its current context ([`Config::cache_id_with_context`]).
    ///
    /// Port of `Config::getCacheID()` (src/OpenColorIO/Config.cpp:5245-5248 @ v2.5.2).
    #[doc(alias = "getCacheID")]
    pub fn cache_id(&self) -> Result<String> {
        self.cache_id_with_context(Some(&self.current_context().get()))
    }

    /// The config's cache ID with `context` (`None`: upstream's null context): the hash of its
    /// text ([`Config::serialize`]), a colon, and with a context the hash of the files its
    /// FileTransforms name, each resolved in the context and hashed, or `?` when it can't be.
    /// Computed once per context cache ID, until the config changes; serializing can fail.
    ///
    /// Port of `Config::getCacheID(const ConstContextRcPtr&)` (src/OpenColorIO/Config.cpp:
    /// 5250-5312 @ v2.5.2).
    #[doc(alias = "getCacheID")]
    pub fn cache_id_with_context(&self, context: Option<&Context>) -> Result<String> {
        let mut cache_ids = self.lock_cache_ids();

        // A null context will use the empty cacheid
        let contextcacheid = context.map(Context::cache_id).unwrap_or_default();

        if let Some(id) = cache_ids.cache_ids.get(&contextcacheid) {
            return Ok(id.clone());
        }

        // Include the hash of the yaml config serialization
        if cache_ids.cache_id_no_context.is_empty() {
            let fullstr = self.serialize()?;
            cache_ids.cache_id_no_context = cache_id_hash(&fullstr);
        }

        // Also include all file references, using the context (if specified)
        let mut file_references_fast_hash = String::new();
        if let Some(context) = context {
            let mut filehash = Vec::new();

            let mut files = BTreeSet::new();
            for transform in self.all_internal_transforms() {
                get_file_references(&mut files, transform);
            }

            for iter in &files {
                if iter.is_empty() {
                    continue;
                }

                filehash.extend_from_slice(iter);
                filehash.push(b'=');

                match context
                    .resolve_file_location(iter)
                    .and_then(|resolved| get_fast_file_hash(&resolved, context))
                {
                    Ok(hash) => {
                        filehash.extend_from_slice(&hash);
                        filehash.push(b' ');
                    }
                    Err(_) => {
                        filehash.extend_from_slice(b"? ");
                        continue;
                    }
                }
            }

            file_references_fast_hash = cache_id_hash(&filehash);
        }

        let id = format!(
            "{}:{}",
            cache_ids.cache_id_no_context, file_references_fast_hash
        );
        cache_ids.cache_ids.insert(contextcacheid, id.clone());
        Ok(id)
    }

    /// A new config ([`Config::new`]) read from the YAML text `input` (`OCIOYaml::Read`), checked
    /// against its version ([`Config::check_version_consistency`]), then its inactive color
    /// spaces refreshed from the config's and the environment's lists only: what the reader set
    /// through the API doesn't supersede them. `filename` is the config's file (`None`, a null
    /// pointer, for a stream).
    ///
    /// Port of `Config::Impl::Read(std::istream&, const char*)` (src/OpenColorIO/
    /// Config.cpp:5548-5562 @ v2.5.2).
    pub(crate) fn read<'a>(
        input: impl Into<IStream<'a>>,
        filename: Option<&[u8]>,
    ) -> Result<Arc<Config>> {
        let mut config = Config::new()?;
        crate::ocio_yaml::read(input, &mut config, filename)?;
        config.check_version_consistency()?;

        // An API request always supersedes the env. variable. As the OCIOYaml helper methods
        // use the Config public API, the variable reset highlights that only the
        // env. variable and the config contents are valid after a config file read.
        config.inactive_color_space_names_api.clear();
        config.refresh_active_color_spaces();

        Ok(Arc::new(config))
    }

    /// Port of `Config::getMajorVersion` (src/OpenColorIO/Config.cpp:1280-1283 @ v2.5.2).
    #[doc(alias = "getMajorVersion")]
    pub fn major_version(&self) -> u32 {
        self.major_version
    }

    /// Sets the major version, and the minor version to the last one this release supports for
    /// it: "The version is <v> where supported versions start at 1 and end at 2." outside them.
    /// Then it resets the config's cache IDs, which empties its cache of processors.
    ///
    /// Port of `Config::setMajorVersion` (src/OpenColorIO/Config.cpp:1285-1304 @ v2.5.2).
    #[doc(alias = "setMajorVersion")]
    pub fn set_major_version(&mut self, version: u32) -> Result<()> {
        if !(FIRST_SUPPORTED_MAJOR_VERSION..=LAST_SUPPORTED_MAJOR_VERSION).contains(&version) {
            return Err(Exception::new(format!(
                "The version is {version} where supported versions start at \
                 {FIRST_SUPPORTED_MAJOR_VERSION} and end at {LAST_SUPPORTED_MAJOR_VERSION}."
            )));
        }
        self.major_version = version;
        self.minor_version = LAST_SUPPORTED_MINOR_VERSION[(version - 1) as usize];

        self.reset_cache_ids();
        Ok(())
    }

    /// Port of `Config::getMinorVersion` (src/OpenColorIO/Config.cpp:1306-1309 @ v2.5.2).
    #[doc(alias = "getMinorVersion")]
    pub fn minor_version(&self) -> u32 {
        self.minor_version
    }

    /// Sets the minor version, up to the last one this release supports for the major
    /// version. Unlike the other setters, it keeps the cache IDs.
    ///
    /// Port of `Config::setMinorVersion` (src/OpenColorIO/Config.cpp:1311-1324 @ v2.5.2).
    #[doc(alias = "setMinorVersion")]
    pub fn set_minor_version(&mut self, version: u32) -> Result<()> {
        let max_minor = LAST_SUPPORTED_MINOR_VERSION[(self.major_version - 1) as usize];
        if version > max_minor {
            return Err(Exception::new(format!(
                "The minor version {version} is not supported for major version {}. Maximum \
                 minor version is {max_minor}.",
                self.major_version
            )));
        }
        self.minor_version = version;
        Ok(())
    }

    /// Sets the major version, then the minor version.
    ///
    /// Port of `Config::setVersion` (src/OpenColorIO/Config.cpp:1326-1330 @ v2.5.2).
    #[doc(alias = "setVersion")]
    pub fn set_version(&mut self, major: u32, minor: u32) -> Result<()> {
        self.set_major_version(major)?;
        self.set_minor_version(minor)
    }

    /// Port of `Config::getName` (src/OpenColorIO/Config.cpp:2110-2113 @ v2.5.2).
    #[doc(alias = "getName")]
    pub fn name(&self) -> &[u8] {
        &self.name
    }

    /// Sets the name, up to its first NUL. It keeps the cache IDs.
    ///
    /// Port of `Config::setName` (src/OpenColorIO/Config.cpp:2115-2118 @ v2.5.2).
    #[doc(alias = "setName")]
    pub fn set_name(&mut self, name: impl AsRef<[u8]>) {
        self.name = c_str(name.as_ref()).to_vec();
    }

    /// The byte that separates the levels of a family name.
    ///
    /// Port of `Config::getFamilySeparator` (src/OpenColorIO/Config.cpp:2122-2125 @ v2.5.2).
    #[doc(alias = "getFamilySeparator")]
    pub fn family_separator(&self) -> u8 {
        self.family_separator
    }

    /// `/`.
    ///
    /// Port of `Config::GetDefaultFamilySeparator` (src/OpenColorIO/Config.cpp:2127-2130 @
    /// v2.5.2).
    #[doc(alias = "GetDefaultFamilySeparator")]
    pub fn default_family_separator() -> u8 {
        DEFAULT_FAMILY_SEPARATOR
    }

    /// Sets the family separator: NUL (no separator) or a printable ASCII byte (32 to 126).
    /// It keeps the cache IDs.
    ///
    /// Port of `Config::setFamilySeparator` (src/OpenColorIO/Config.cpp:2132-2145 @ v2.5.2).
    /// Upstream takes a `char`, signed on both wheels' platforms, so the bytes from 128 up are
    /// refused as negative values.
    #[doc(alias = "setFamilySeparator")]
    pub fn set_family_separator(&mut self, separator: u8) -> Result<()> {
        let val = i32::from(separator as i8);
        if val != 0 && !(32..=126).contains(&val) {
            let mut err = b"Invalid family separator '".to_vec();
            err.push(separator);
            err.extend_from_slice(b"'.");
            return Err(Exception::new(err));
        }

        self.family_separator = separator;
        Ok(())
    }

    /// Port of `Config::getDescription` (src/OpenColorIO/Config.cpp:2149-2152 @ v2.5.2).
    #[doc(alias = "getDescription")]
    pub fn description(&self) -> &[u8] {
        &self.description
    }

    /// Sets the description, up to its first NUL. It keeps the cache IDs.
    ///
    /// Port of `Config::setDescription` (src/OpenColorIO/Config.cpp:2154-2157 @ v2.5.2).
    #[doc(alias = "setDescription")]
    pub fn set_description(&mut self, description: impl AsRef<[u8]>) {
        self.description = c_str(description.as_ref()).to_vec();
    }

    /// The current context, shared with the config: a later change of the config's context
    /// (its environment, search paths or working directory) is seen through it.
    ///
    /// Port of `Config::getCurrentContext` (src/OpenColorIO/Config.cpp:2161-2164 @ v2.5.2).
    #[doc(alias = "getCurrentContext")]
    pub fn current_context(&self) -> CurrentContext {
        self.shared_context.clone()
    }

    /// Adds the environment variable `name` with the default value `default_value`, or
    /// removes it for `None` (upstream's null pointer): in the config's list and as a variable
    /// of its context. An empty name is ignored.
    ///
    /// Port of `Config::addEnvironmentVar` (src/OpenColorIO/Config.cpp:2166-2189 @ v2.5.2).
    #[doc(alias = "addEnvironmentVar")]
    pub fn add_environment_var(&mut self, name: impl AsRef<[u8]>, default_value: Option<&[u8]>) {
        let name = c_str(name.as_ref());
        if name.is_empty() {
            return;
        }

        // Note: Only a null default value removes the entry.

        match default_value {
            Some(default_value) => {
                let default_value = c_str(default_value);
                self.env.insert(name.to_vec(), default_value.to_vec());
                self.update_context(|c| c.set_string_var(name, Some(default_value)));
            }
            None => {
                self.env.remove(name);
                self.update_context(|c| c.set_string_var(name, None));
            }
        }

        self.reset_cache_ids();
    }

    /// Port of `Config::getNumEnvironmentVars` (src/OpenColorIO/Config.cpp:2191-2194 @ v2.5.2).
    #[doc(alias = "getNumEnvironmentVars")]
    pub fn num_environment_vars(&self) -> i32 {
        self.env.len() as i32
    }

    /// The name of the environment variable at `index`, in byte order; `""` outside the list.
    ///
    /// Port of `Config::getEnvironmentVarNameByIndex` (src/OpenColorIO/Config.cpp:2196-2202 @
    /// v2.5.2).
    #[doc(alias = "getEnvironmentVarNameByIndex")]
    pub fn environment_var_name_by_index(&self, index: i32) -> &[u8] {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.env.keys().nth(i))
            .map_or(&[], Vec::as_slice)
    }

    /// The default value of the environment variable `name`; `""` for an unknown or empty
    /// name.
    ///
    /// Port of `Config::getEnvironmentVarDefault` (src/OpenColorIO/Config.cpp:2204-2208 @
    /// v2.5.2).
    #[doc(alias = "getEnvironmentVarDefault")]
    pub fn environment_var_default(&self, name: impl AsRef<[u8]>) -> &[u8] {
        let name = c_str(name.as_ref());
        if name.is_empty() {
            return &[];
        }
        lookup_environment(&self.env, name)
    }

    /// Removes every environment variable, and every variable of the context.
    ///
    /// Port of `Config::clearEnvironmentVars` (src/OpenColorIO/Config.cpp:2210-2217 @ v2.5.2).
    #[doc(alias = "clearEnvironmentVars")]
    pub fn clear_environment_vars(&mut self) {
        self.env.clear();
        self.update_context(|c| c.clear_string_vars());

        self.reset_cache_ids();
    }

    /// Sets the environment mode of the context.
    ///
    /// Port of `Config::setEnvironmentMode` (src/OpenColorIO/Config.cpp:2219-2225 @ v2.5.2).
    #[doc(alias = "setEnvironmentMode")]
    pub fn set_environment_mode(&mut self, mode: EnvironmentMode) {
        self.update_context(|c| c.set_environment_mode(mode));

        self.reset_cache_ids();
    }

    /// Port of `Config::getEnvironmentMode` (src/OpenColorIO/Config.cpp:2227-2230 @ v2.5.2).
    #[doc(alias = "getEnvironmentMode")]
    pub fn environment_mode(&self) -> EnvironmentMode {
        self.shared_context.get().environment_mode()
    }

    /// Loads the environment into the context (`Context::loadEnvironment`).
    ///
    /// Port of `Config::loadEnvironment` (src/OpenColorIO/Config.cpp:2232-2238 @ v2.5.2).
    #[doc(alias = "loadEnvironment")]
    pub fn load_environment(&mut self) {
        self.update_context(|c| c.load_environment());

        self.reset_cache_ids();
    }

    /// The search path, as set or joined with `:`.
    ///
    /// Port of `Config::getSearchPath()` (src/OpenColorIO/Config.cpp:2240-2243 @ v2.5.2).
    #[doc(alias = "getSearchPath")]
    pub fn search_path(&self) -> Vec<u8> {
        self.shared_context.get().search_path().to_vec()
    }

    /// Sets the search paths from `path`, split on `:` (`Context::setSearchPath`).
    ///
    /// Port of `Config::setSearchPath` (src/OpenColorIO/Config.cpp:2245-2251 @ v2.5.2).
    #[doc(alias = "setSearchPath")]
    pub fn set_search_path(&mut self, path: impl AsRef<[u8]>) {
        self.update_context(|c| c.set_search_path(path));

        self.reset_cache_ids();
    }

    /// Port of `Config::getNumSearchPaths` (src/OpenColorIO/Config.cpp:2253-2256 @ v2.5.2).
    #[doc(alias = "getNumSearchPaths")]
    pub fn num_search_paths(&self) -> i32 {
        self.shared_context.get().num_search_paths()
    }

    /// The search path at `index`; `""` outside the list.
    ///
    /// Port of `Config::getSearchPath(int)` (src/OpenColorIO/Config.cpp:2258-2261 @ v2.5.2).
    #[doc(alias = "getSearchPath")]
    pub fn search_path_with_index(&self, index: i32) -> Vec<u8> {
        self.shared_context
            .get()
            .search_path_with_index(index)
            .to_vec()
    }

    /// Port of `Config::clearSearchPaths` (src/OpenColorIO/Config.cpp:2263-2269 @ v2.5.2).
    #[doc(alias = "clearSearchPaths")]
    pub fn clear_search_paths(&mut self) {
        self.update_context(|c| c.clear_search_paths());

        self.reset_cache_ids();
    }

    /// Adds a search path; an empty one is ignored, and keeps the cache IDs.
    ///
    /// Port of `Config::addSearchPath` (src/OpenColorIO/Config.cpp:2271-2278 @ v2.5.2).
    #[doc(alias = "addSearchPath")]
    pub fn add_search_path(&mut self, path: impl AsRef<[u8]>) {
        let path = c_str(path.as_ref());
        if path.is_empty() {
            return;
        }
        self.update_context(|c| c.add_search_path(path));

        self.reset_cache_ids();
    }

    /// Port of `Config::getWorkingDir` (src/OpenColorIO/Config.cpp:2280-2283 @ v2.5.2).
    #[doc(alias = "getWorkingDir")]
    pub fn working_dir(&self) -> Vec<u8> {
        self.shared_context.get().working_dir().to_vec()
    }

    /// Port of `Config::setWorkingDir` (src/OpenColorIO/Config.cpp:2285-2291 @ v2.5.2).
    #[doc(alias = "setWorkingDir")]
    pub fn set_working_dir(&mut self, dirname: impl AsRef<[u8]>) {
        self.update_context(|c| c.set_working_dir(dirname));

        self.reset_cache_ids();
    }

    // Color spaces ////////////////////////////////////////////////////////////////////////////

    /// The color space named `name` (or with the alias `name`), ignoring case; else the color
    /// space of the role `name`.
    ///
    /// Port of `Config::Impl::getColorSpace` (src/OpenColorIO/Config.cpp:459-471 @ v2.5.2).
    fn impl_color_space(&self, name: &[u8]) -> Option<&ColorSpace> {
        // Check to see if the name is a color space.
        self.all_color_spaces.color_space(name).or_else(|| {
            // Check to see if the name is a role.
            let csname = lookup_role(&self.roles, c_str(name));
            self.all_color_spaces.color_space(csname)
        })
    }

    /// The named transform whose name or alias is `name`, ignoring case.
    ///
    /// Port of `Config::Impl::getNamedTransform` (src/OpenColorIO/Config.cpp:485-494 @
    /// v2.5.2).
    fn impl_named_transform(&self, name: &[u8]) -> Option<&NamedTransform> {
        self.all_named_transforms
            .get(self.impl_named_transform_index(name))
    }

    /// The index of the named transform whose name, or else one of whose aliases, is `name`,
    /// ignoring case; `usize::MAX` (upstream's `static_cast<size_t>(-1)`) for none or an empty
    /// name.
    ///
    /// Port of `Config::Impl::getNamedTransformIndex` (src/OpenColorIO/Config.cpp:496-519 @
    /// v2.5.2).
    fn impl_named_transform_index(&self, name: &[u8]) -> usize {
        let name = c_str(name);
        if !name.is_empty() {
            let str = lower(name);
            for (idx, nt) in self.all_named_transforms.iter().enumerate() {
                if lower(nt.name()) == str {
                    return idx;
                }
                for alias in 0..nt.num_aliases() {
                    if lower(nt.alias(alias)) == str {
                        return idx;
                    }
                }
            }
        }
        usize::MAX
    }

    /// The inactive names: the list of the API, else of `OCIO_INACTIVE_COLORSPACES`, else of
    /// the config, split on `,` and trimmed; for `ColorSpace` and `NamedTransform`, only those
    /// that name one (by name, alias or, for a color space, role), as its own name.
    ///
    /// Port of `Config::Impl::buildInactiveNamesList` (src/OpenColorIO/Config.cpp:5351-5407 @
    /// v2.5.2).
    fn build_inactive_names_list(&self, kind: InactiveType) -> StringVec {
        let mut inactive_names = StringVec::new();

        // An API request always supersedes the other lists.
        if !self.inactive_color_space_names_api.is_empty() {
            inactive_names = split(&self.inactive_color_space_names_api, b',');
        }
        // The env. variable only supersedes the config list.
        else if !self.inactive_color_space_names_env.is_empty() {
            inactive_names = split(&self.inactive_color_space_names_env, b',');
        } else if !self.inactive_color_space_names_conf.is_empty() {
            inactive_names = split(&self.inactive_color_space_names_conf, b',');
        }

        let mut res = StringVec::new();
        for v in &mut inactive_names {
            *v = trim(v).to_vec();
            match kind {
                InactiveType::ColorSpace => {
                    // Only add existing items.
                    if let Some(cs) = self.impl_color_space(v) {
                        // Use the canonical name (alias or role might have been used).
                        res.push(cs.name().to_vec());
                    }
                }
                InactiveType::NamedTransform => {
                    // Only add existing items.
                    if let Some(nt) = self.impl_named_transform(v) {
                        // Use the canonical name (alias might have been used).
                        res.push(nt.name().to_vec());
                    }
                }
                InactiveType::All => {
                    // This is only used to verify that all items of the list do exists (only
                    // used by the validate() function.
                    res.push(v.clone());
                }
            }
        }

        res
    }

    /// Rebuilds the lists of active and inactive color spaces and named transforms.
    ///
    /// Port of `Config::Impl::refreshActiveColorSpaces` (src/OpenColorIO/Config.cpp:5409-5460
    /// @ v2.5.2).
    fn refresh_active_color_spaces(&mut self) {
        self.active_color_space_names.clear();
        self.active_named_transform_names.clear();

        self.inactive_color_space_names = self.build_inactive_names_list(InactiveType::ColorSpace);

        for i in 0..self.all_color_spaces.num_color_spaces() {
            let cs = self
                .all_color_spaces
                .color_space_by_index(i)
                .expect("an index of the set");
            let name = cs.name();

            let is_active = !self
                .inactive_color_space_names
                .iter()
                .any(|cs_name| cs_name == name);

            if is_active {
                self.active_color_space_names.push(name.to_vec());
            }
        }

        self.inactive_named_transform_names =
            self.build_inactive_names_list(InactiveType::NamedTransform);

        for nt in &self.all_named_transforms {
            let name = nt.name();

            let is_active = !self
                .inactive_named_transform_names
                .iter()
                .any(|cs_name| cs_name == name);

            if is_active {
                self.active_named_transform_names.push(name.to_vec());
            }
        }
    }

    /// The active color spaces that have the category `category` (all of them for an empty
    /// one), copied into a new set.
    ///
    /// Port of `Config::getColorSpaces(const char *)` (src/OpenColorIO/Config.cpp:2296-2314 @
    /// v2.5.2).
    #[doc(alias = "getColorSpaces")]
    pub fn color_spaces(&self, category: impl AsRef<[u8]>) -> ColorSpaceSet {
        let category = c_str(category.as_ref());
        let mut res = ColorSpaceSet::new();

        // Loop on the list of active color spaces.

        for idx in 0..self.num_color_spaces() {
            let cs_name = self.color_space_name_by_index(idx);
            let cs = self
                .all_color_spaces
                .color_space(cs_name)
                .expect("an active color space");
            if category.is_empty() || cs.has_category(category) {
                res.add_color_space(cs)
                    .expect("the config's color spaces have distinct names and aliases");
            }
        }

        res
    }

    /// The number of color spaces of the reference space `search_reference_type`, among those
    /// of `visibility`.
    ///
    /// Port of `Config::getNumColorSpaces(SearchReferenceSpaceType, ColorSpaceVisibility)`
    /// (src/OpenColorIO/Config.cpp:2332-2392 @ v2.5.2).
    #[doc(alias = "getNumColorSpaces")]
    pub fn num_color_spaces_with(
        &self,
        search_reference_type: SearchReferenceSpaceType,
        visibility: ColorSpaceVisibility,
    ) -> i32 {
        let mut res = 0;
        match visibility {
            ColorSpaceVisibility::All => {
                let nb_cs = self.all_color_spaces.num_color_spaces();
                if search_reference_type == SearchReferenceSpaceType::All {
                    return nb_cs;
                }
                for i in 0..nb_cs {
                    let cs = self
                        .all_color_spaces
                        .color_space_by_index(i)
                        .expect("an index of the set");
                    if match_reference_type(search_reference_type, cs.reference_space_type()) {
                        res += 1;
                    }
                }
            }
            ColorSpaceVisibility::Active => {
                if search_reference_type == SearchReferenceSpaceType::All {
                    return self.active_color_space_names.len() as i32;
                }
                for csname in &self.active_color_space_names {
                    let cs = self.color_space(csname).expect("an active color space");
                    if match_reference_type(search_reference_type, cs.reference_space_type()) {
                        res += 1;
                    }
                }
            }
            ColorSpaceVisibility::Inactive => {
                if search_reference_type == SearchReferenceSpaceType::All {
                    return self.inactive_color_space_names.len() as i32;
                }
                for csname in &self.inactive_color_space_names {
                    let cs = self.color_space(csname).expect("an inactive color space");
                    if match_reference_type(search_reference_type, cs.reference_space_type()) {
                        res += 1;
                    }
                }
            }
        }

        res
    }

    /// The name of the color space at `index` among those of the reference space
    /// `search_reference_type` and of `visibility`; `""` outside them.
    ///
    /// Port of `Config::getColorSpaceNameByIndex(SearchReferenceSpaceType,
    /// ColorSpaceVisibility, int)` (src/OpenColorIO/Config.cpp:2394-2495 @ v2.5.2).
    #[doc(alias = "getColorSpaceNameByIndex")]
    pub fn color_space_name_by_index_with(
        &self,
        search_reference_type: SearchReferenceSpaceType,
        visibility: ColorSpaceVisibility,
        index: i32,
    ) -> &[u8] {
        if index < 0 {
            return &[];
        }

        let mut current = 0;
        match visibility {
            ColorSpaceVisibility::All => {
                if search_reference_type == SearchReferenceSpaceType::All {
                    return self
                        .all_color_spaces
                        .color_space_name_by_index(index)
                        .unwrap_or(&[]);
                }
                let nb_cs = self.all_color_spaces.num_color_spaces();
                for i in 0..nb_cs {
                    let cs = self
                        .all_color_spaces
                        .color_space_by_index(i)
                        .expect("an index of the set");
                    if match_reference_type(search_reference_type, cs.reference_space_type()) {
                        if current == index {
                            return cs.name();
                        }
                        current += 1;
                    }
                }
            }
            ColorSpaceVisibility::Active | ColorSpaceVisibility::Inactive => {
                let names = if visibility == ColorSpaceVisibility::Active {
                    &self.active_color_space_names
                } else {
                    &self.inactive_color_space_names
                };
                if search_reference_type == SearchReferenceSpaceType::All {
                    return names.get(index as usize).map_or(&[], Vec::as_slice);
                }
                for csname in names {
                    let cs = self.color_space(csname).expect("a listed color space");
                    if match_reference_type(search_reference_type, cs.reference_space_type()) {
                        if current == index {
                            return cs.name();
                        }
                        current += 1;
                    }
                }
            }
        }

        &[]
    }

    /// The color space named `name` (or with the alias `name`), ignoring case, else the color
    /// space of the role `name`, among all the color spaces (inactive ones included).
    ///
    /// Port of `Config::getColorSpace` (src/OpenColorIO/Config.cpp:2497-2500 @ v2.5.2).
    #[doc(alias = "getColorSpace")]
    pub fn color_space(&self, name: impl AsRef<[u8]>) -> Option<&ColorSpace> {
        self.impl_color_space(name.as_ref())
    }

    /// The name of the color space (by name, alias or role) or named transform (by name or
    /// alias) `name`; `""` for none.
    ///
    /// Port of `Config::getCanonicalName` (src/OpenColorIO/Config.cpp:2502-2515 @ v2.5.2).
    #[doc(alias = "getCanonicalName")]
    pub fn canonical_name(&self, name: impl AsRef<[u8]>) -> &[u8] {
        let name = name.as_ref();
        if let Some(cs) = self.color_space(name) {
            return cs.name();
        }
        if let Some(nt) = self.impl_named_transform(name) {
            return nt.name();
        }
        &[]
    }

    /// The number of active color spaces.
    ///
    /// Port of `Config::getNumColorSpaces()` (src/OpenColorIO/Config.cpp:2517-2520 @ v2.5.2).
    #[doc(alias = "getNumColorSpaces")]
    pub fn num_color_spaces(&self) -> i32 {
        self.num_color_spaces_with(SearchReferenceSpaceType::All, ColorSpaceVisibility::Active)
    }

    /// The name of the active color space at `index`; `""` outside them.
    ///
    /// Port of `Config::getColorSpaceNameByIndex(int)` (src/OpenColorIO/Config.cpp:2522-2525 @
    /// v2.5.2).
    #[doc(alias = "getColorSpaceNameByIndex")]
    pub fn color_space_name_by_index(&self, index: i32) -> &[u8] {
        self.color_space_name_by_index_with(
            SearchReferenceSpaceType::All,
            ColorSpaceVisibility::Active,
            index,
        )
    }

    /// The index among the active color spaces of the color space `name` (by name, alias or
    /// role); -1 for none or an inactive one.
    ///
    /// Port of `Config::getIndexForColorSpace` (src/OpenColorIO/Config.cpp:2527-2549 @ v2.5.2).
    #[doc(alias = "getIndexForColorSpace")]
    pub fn index_for_color_space(&self, name: impl AsRef<[u8]>) -> i32 {
        let Some(cs) = self.color_space(name) else {
            return -1;
        };

        // Check to see if the name is an active color space.
        let num =
            self.num_color_spaces_with(SearchReferenceSpaceType::All, ColorSpaceVisibility::Active);
        for idx in 0..num {
            // strcmp: names hold no NUL.
            if self.color_space_name_by_index_with(
                SearchReferenceSpaceType::All,
                ColorSpaceVisibility::Active,
                idx,
            ) == cs.name()
            {
                return idx;
            }
        }

        // Requests for an inactive color space or a role mapping
        // to an inactive color space will both fail.
        -1
    }

    /// Adds a copy of `original`, or replaces the color space of the same name. Refuses a
    /// color space without a name, one whose name or an alias is a role or names a named
    /// transform, one whose name (in a version 2 config) or an alias holds `$` or `%`, and what
    /// [`ColorSpaceSet::add_color_space`] refuses.
    ///
    /// Port of `Config::addColorSpace` (src/OpenColorIO/Config.cpp:2577-2648 @ v2.5.2).
    #[doc(alias = "addColorSpace")]
    pub fn add_color_space(&mut self, original: &ColorSpace) -> Result<()> {
        let name = original.name();
        if name.is_empty() {
            return Err(Exception::new("Color space must have a non-empty name."));
        }

        // Check this is not an existing role or named transform.
        if self.has_role(name) {
            return Err(Exception::new(
                [
                    b"Cannot add '".as_slice(),
                    name,
                    b"' color space, there is already a role with this name.",
                ]
                .concat(),
            ));
        }
        if let Some(nt) = self.impl_named_transform(name) {
            return Err(Exception::new(
                [
                    b"Cannot add '".as_slice(),
                    name,
                    b"' color space, there is already a named transform using this name as a \
                      name or as an alias: '",
                    nt.name(),
                    b"'.",
                ]
                .concat(),
            ));
        }

        if self.major_version() >= 2 && contains_context_variable_token(name) {
            return Err(Exception::new(
                [
                    b"A color space name '".as_slice(),
                    name,
                    b"' cannot contain a context variable reserved token i.e. % or $.",
                ]
                .concat(),
            ));
        }

        let num_aliases = original.num_aliases();
        for aidx in 0..num_aliases {
            let alias = original.alias(aidx);

            if self.has_role(alias) {
                return Err(Exception::new(
                    [
                        b"Cannot add '".as_slice(),
                        name,
                        b"' color space, it has an alias '",
                        alias,
                        b"' and there is already a role with this name.",
                    ]
                    .concat(),
                ));
            }
            if let Some(nt) = self.impl_named_transform(alias) {
                return Err(Exception::new(
                    [
                        b"Cannot add '".as_slice(),
                        name,
                        b"' color space, it has an alias '",
                        alias,
                        b"' and there is already a named transform using this name as a name or \
                          as an alias: '",
                        nt.name(),
                        b"'.",
                    ]
                    .concat(),
                ));
            }
            if contains_context_variable_token(alias) {
                return Err(Exception::new(
                    [
                        b"Cannot add '".as_slice(),
                        name,
                        b"' color space, it has an alias '",
                        alias,
                        b"' that cannot contain a context variable reserved token i.e. % or $.",
                    ]
                    .concat(),
                ));
            }
        }

        // This is verifying that name and aliases are fine with other color spaces.
        self.all_color_spaces.add_color_space(original)?;

        self.reset_cache_ids();
        self.refresh_active_color_spaces();
        Ok(())
    }

    /// Removes the color space whose name (not an alias) is `name`, ignoring case.
    ///
    /// Port of `Config::removeColorSpace` (src/OpenColorIO/Config.cpp:2650-2657 @ v2.5.2).
    #[doc(alias = "removeColorSpace")]
    pub fn remove_color_space(&mut self, name: impl AsRef<[u8]>) {
        self.all_color_spaces.remove_color_space(name);

        self.reset_cache_ids();
        self.refresh_active_color_spaces();
    }

    /// Removes every color space.
    ///
    /// Port of `Config::clearColorSpaces` (src/OpenColorIO/Config.cpp:2773-2780 @ v2.5.2).
    #[doc(alias = "clearColorSpaces")]
    pub fn clear_color_spaces(&mut self) {
        self.all_color_spaces.clear_color_spaces();

        self.reset_cache_ids();
        self.refresh_active_color_spaces();
    }

    // Roles ///////////////////////////////////////////////////////////////////////////////////

    /// Sets the role `role` (stored in lower case) to the color space `color_space_name`, or
    /// removes it for `None` (upstream's null pointer). A new role may not be the name or alias
    /// of a color space or a named transform, nor hold `$` or `%` (in a version 2 config).
    ///
    /// Port of `Config::setRole` (src/OpenColorIO/Config.cpp:2978-3027 @ v2.5.2).
    #[doc(alias = "setRole")]
    pub fn set_role(
        &mut self,
        role: impl AsRef<[u8]>,
        color_space_name: Option<&[u8]>,
    ) -> Result<()> {
        let role = c_str(role.as_ref());
        if role.is_empty() {
            return Err(Exception::new("The role name is null."));
        }

        // Set the role.
        if let Some(color_space_name) = color_space_name {
            if !self.has_role(role) {
                if self.color_space(role).is_some() {
                    return Err(Exception::new(
                        [
                            b"Cannot add '".as_slice(),
                            role,
                            b"' role, there is already a color space using this as a name or \
                              an alias.",
                        ]
                        .concat(),
                    ));
                }
                if self.impl_named_transform(role).is_some() {
                    return Err(Exception::new(
                        [
                            b"Cannot add '".as_slice(),
                            role,
                            b"' role, there is already a named transform using this as a name \
                              or an alias.",
                        ]
                        .concat(),
                    ));
                }
                if self.major_version() >= 2 && contains_context_variable_token(role) {
                    return Err(Exception::new(
                        [
                            b"Role name '".as_slice(),
                            role,
                            b"' cannot contain a context variable reserved token i.e. % or $.",
                        ]
                        .concat(),
                    ));
                }
            }
            self.roles
                .insert(lower(role), c_str(color_space_name).to_vec());
        }
        // Unset the role.
        else {
            self.roles.remove(&lower(role));
        }

        self.reset_cache_ids();
        Ok(())
    }

    /// Port of `Config::getNumRoles` (src/OpenColorIO/Config.cpp:3029-3032 @ v2.5.2).
    #[doc(alias = "getNumRoles")]
    pub fn num_roles(&self) -> i32 {
        self.roles.len() as i32
    }

    /// Whether the role `role` (ignoring case) exists and names a color space (a role set to
    /// `""` doesn't count).
    ///
    /// Port of `Config::hasRole` (src/OpenColorIO/Config.cpp:3034-3039 @ v2.5.2).
    #[doc(alias = "hasRole")]
    pub fn has_role(&self, role: impl AsRef<[u8]>) -> bool {
        let role = c_str(role.as_ref());
        if role.is_empty() {
            return false;
        }
        let rname = lookup_role(&self.roles, role);
        !rname.is_empty()
    }

    /// The name of the role at `index`, in lower case, in byte order; `""` outside them.
    ///
    /// Port of `Config::getRoleName` (src/OpenColorIO/Config.cpp:3041-3047 @ v2.5.2).
    #[doc(alias = "getRoleName")]
    pub fn role_name(&self, index: i32) -> &[u8] {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.roles.keys().nth(i))
            .map_or(&[], Vec::as_slice)
    }

    /// The color space of the role at `index`; `""` outside them.
    ///
    /// Port of `Config::getRoleColorSpace(int)` (src/OpenColorIO/Config.cpp:3049-3052 @
    /// v2.5.2).
    #[doc(alias = "getRoleColorSpace")]
    pub fn role_color_space_by_index(&self, index: i32) -> &[u8] {
        lookup_role(&self.roles, self.role_name(index))
    }

    /// The color space of the role `role_name`, ignoring case; `""` for none.
    ///
    /// Port of `Config::getRoleColorSpace(const char *)` (src/OpenColorIO/Config.cpp:3054-3058
    /// @ v2.5.2).
    #[doc(alias = "getRoleColorSpace")]
    pub fn role_color_space(&self, role_name: impl AsRef<[u8]>) -> &[u8] {
        let role_name = c_str(role_name.as_ref());
        if role_name.is_empty() {
            return &[];
        }
        lookup_role(&self.roles, role_name)
    }

    /// Sets the config's list of inactive color spaces and named transforms (trimmed), which
    /// supersedes the environment's: an API request.
    ///
    /// Port of `Config::setInactiveColorSpaces` and `Config::Impl::setInactiveColorSpaces`
    /// (src/OpenColorIO/Config.cpp:766-778, 2551-2554 @ v2.5.2).
    #[doc(alias = "setInactiveColorSpaces")]
    pub fn set_inactive_color_spaces(&mut self, inactive_color_spaces: impl AsRef<[u8]>) {
        self.inactive_color_space_names_conf = trim(c_str(inactive_color_spaces.as_ref())).to_vec();

        // An API request must always supersede the two other lists. Filling the
        // m_inactiveColorSpaceNamesAPI list highlights the API request precedence.
        self.inactive_color_space_names_api = self.inactive_color_space_names_conf.clone();

        self.reset_cache_ids();
        self.refresh_active_color_spaces();
    }

    /// The config's list of inactive color spaces, as set (not the environment's).
    ///
    /// Port of `Config::getInactiveColorSpaces` (src/OpenColorIO/Config.cpp:2556-2559 @
    /// v2.5.2).
    #[doc(alias = "getInactiveColorSpaces")]
    pub fn inactive_color_spaces(&self) -> &[u8] {
        &self.inactive_color_space_names_conf
    }

    /// Whether the config's list of inactive color spaces, split on `", "`, holds `colorspace`,
    /// ignoring case. Only the config's list counts, as written: not the environment's, and
    /// neither aliases nor roles.
    ///
    /// Port of `Config::isInactiveColorSpace` (src/OpenColorIO/Config.cpp:2561-2575 @ v2.5.2).
    #[doc(alias = "isInactiveColorSpace")]
    pub fn is_inactive_color_space(&self, colorspace: impl AsRef<[u8]>) -> bool {
        let colorspace = c_str(colorspace.as_ref());
        let svec = pystring::split(&self.inactive_color_space_names_conf, b", ", -1);

        // Colorspace is inactive.
        svec.iter().any(|s| compare(colorspace, s))
    }

    /// Port of `Config::isStrictParsingEnabled` (src/OpenColorIO/Config.cpp:2964-2967 @
    /// v2.5.2).
    #[doc(alias = "isStrictParsingEnabled")]
    pub fn is_strict_parsing_enabled(&self) -> bool {
        self.strict_parsing
    }

    /// Port of `Config::setStrictParsingEnabled` (src/OpenColorIO/Config.cpp:2969-2975 @
    /// v2.5.2).
    #[doc(alias = "setStrictParsingEnabled")]
    pub fn set_strict_parsing_enabled(&mut self, enabled: bool) {
        self.strict_parsing = enabled;

        self.reset_cache_ids();
    }

    /// The default luma coefficients, R, G and B.
    ///
    /// Port of `Config::getDefaultLumaCoefs` (src/OpenColorIO/Config.cpp:4450-4457 @ v2.5.2).
    /// Its error for a null array can't happen with a reference.
    #[doc(alias = "getDefaultLumaCoefs")]
    pub fn default_luma_coefs(&self) -> [f64; 3] {
        self.default_luma_coefs
    }

    /// Port of `Config::setDefaultLumaCoefs` (src/OpenColorIO/Config.cpp:4459-4470 @ v2.5.2).
    /// Its error for a null array can't happen with a reference.
    #[doc(alias = "setDefaultLumaCoefs")]
    pub fn set_default_luma_coefs(&mut self, c3: &[f64; 3]) {
        self.default_luma_coefs = *c3;

        self.reset_cache_ids();
    }

    /// The processor of `transform` in the direction `direction`, in the current context, not
    /// cached and with the processor caches off.
    ///
    /// Port of `Config::Impl::getProcessorWithoutCaching` (src/OpenColorIO/Config.cpp:903-918
    /// @ v2.5.2). Its error for a null transform can't happen with a reference.
    fn processor_without_caching(
        &self,
        transform: &Transform,
        direction: TransformDirection,
    ) -> Result<Processor> {
        let mut processor = Processor::new();
        processor.set_processor_cache_flags(ProcessorCacheFlags::OFF);
        processor.set_transform(self, &self.shared_context.get(), transform, direction)?;
        Ok(processor)
    }

    /// Whether the color space `color_space` (by name, alias or role) is linear for the
    /// reference space `reference_space_type`: not data, of that reference space, and either
    /// of the encoding `scene-linear` (scene) or `display-linear` (display), or without an
    /// encoding and with transforms that keep 4 x 1/16 equal to 4 within 1e-5 on the neutral,
    /// red, green and blue axes (or no transform).
    ///
    /// Port of `Config::isColorSpaceLinear` (src/OpenColorIO/Config.cpp:2784-2906 @ v2.5.2).
    #[doc(alias = "isColorSpaceLinear")]
    pub fn is_color_space_linear(
        &self,
        color_space: impl AsRef<[u8]>,
        reference_space_type: ReferenceSpaceType,
    ) -> Result<bool> {
        let color_space = c_str(color_space.as_ref());
        let Some(cs) = self.color_space(color_space) else {
            return Err(Exception::new(
                [
                    b"Could not test colorspace linearity. Colorspace ".as_slice(),
                    color_space,
                    b" does not exist.",
                ]
                .concat(),
            ));
        };

        if cs.is_data() {
            return Ok(false);
        }

        // Colorspace is not linear if the types are opposite.
        if cs.reference_space_type() != reference_space_type {
            return Ok(false);
        }

        let encoding = cs.encoding();
        if !encoding.is_empty() {
            // Check the encoding value if it is set.
            return Ok((compare(encoding, b"scene-linear")
                && reference_space_type == ReferenceSpaceType::Scene)
                || (compare(encoding, b"display-linear")
                    && reference_space_type == ReferenceSpaceType::Display));
        }

        // We want to assess linearity over at least a reasonable range of values, so use a very
        // dark value and a very bright value. Test neutral, red, green, and blue points to
        // detect situations where the neutral may be linear but there is non-linearity off the
        // neutral axis.
        let evaluate = |config: &Config, t: &Transform| -> Result<bool> {
            #[rustfmt::skip]
            let img: [f32; 24] = [
                0.0625, 0.0625, 0.0625, 4., 4., 4.,
                0.0625, 0., 0., 4., 0., 0.,
                0., 0.0625, 0., 0., 4., 0.,
                0., 0., 0.0625, 0., 0., 4.,
            ];
            let mut dst = [0f32; 24];

            let proc_to_reference =
                config.processor_without_caching(t, TransformDirection::Forward)?;

            // TODO: It could be useful to try and avoid evaluating points through ops that are
            // expensive but highly unlikely to be linear (with inverse Lut3D being the prime
            // example). (Upstream's comment continues at Config.cpp:2850-2864.)

            let opt_cpu_proc =
                proc_to_reference.optimized_cpu_processor(OptimizationFlags::NONE)?;
            {
                let desc =
                    PackedImageDesc::with_channel_order(&img[..], 8, 1, ChannelOrdering::Rgb)?;
                let mut desc_dst =
                    PackedImageDesc::with_channel_order(&mut dst[..], 8, 1, ChannelOrdering::Rgb)?;
                opt_cpu_proc.apply_src_dst(&desc, &mut desc_dst)?;
            }

            let abs_error = 1e-5f32;
            let multiplier = 64.0f32;
            let mut ret = true;

            // Test the first RGB pair.
            ret &= equal_with_abs_error(dst[0] * multiplier, dst[3], abs_error);
            ret &= equal_with_abs_error(dst[1] * multiplier, dst[4], abs_error);
            ret &= equal_with_abs_error(dst[2] * multiplier, dst[5], abs_error);

            // Test the second RGB pair.
            ret &= equal_with_abs_error(dst[6] * multiplier, dst[9], abs_error);
            ret &= equal_with_abs_error(dst[7] * multiplier, dst[10], abs_error);
            ret &= equal_with_abs_error(dst[8] * multiplier, dst[11], abs_error);

            // Test the third RGB pair.
            ret &= equal_with_abs_error(dst[12] * multiplier, dst[15], abs_error);
            ret &= equal_with_abs_error(dst[13] * multiplier, dst[16], abs_error);
            ret &= equal_with_abs_error(dst[14] * multiplier, dst[17], abs_error);

            // Test the fourth RGB pair.
            ret &= equal_with_abs_error(dst[18] * multiplier, dst[21], abs_error);
            ret &= equal_with_abs_error(dst[19] * multiplier, dst[22], abs_error);
            ret &= equal_with_abs_error(dst[20] * multiplier, dst[23], abs_error);

            Ok(ret)
        };

        let transform_to_reference = cs.transform(ColorSpaceDirection::ToReference);
        let transform_from_reference = cs.transform(ColorSpaceDirection::FromReference);
        if let Some(to_reference) = transform_to_reference {
            // Color space has a transform for the to-reference direction, or both directions.
            return evaluate(self, to_reference);
        } else if let Some(from_reference) = transform_from_reference {
            // Color space only has a transform for the from-reference direction.
            return evaluate(self, from_reference);
        }

        // Color space matches the desired reference space type, is not a data space, and has
        // no transforms, so it is equivalent to the reference space and hence linear.
        Ok(true)
    }

    // Displays and views //////////////////////////////////////////////////////////////////////

    /// The views of `display`: its own, then the config's shared views it names (those that
    /// exist), in their orders.
    ///
    /// Port of `Config::Impl::getViews` (src/OpenColorIO/Config.cpp:556-575 @ v2.5.2).
    fn impl_views<'a>(&'a self, display: &'a Display) -> Vec<&'a View> {
        let mut views: Vec<&View> = display.views.iter().collect();

        for shared in &display.shared_views {
            if let Some(i) = find_view(&self.shared_views, shared) {
                views.push(&self.shared_views[i]);
            }
        }
        views
    }

    /// The view `view` of `display`: its own, or the shared view it names; with an empty
    /// display (upstream's null pointer too), the config's shared view of that name.
    ///
    /// Port of `Config::Impl::getView` (src/OpenColorIO/Config.cpp:783-806 @ v2.5.2).
    fn impl_view(&self, display: &[u8], view: &[u8]) -> Option<&View> {
        let display = c_str(display);
        let view = c_str(view);
        if view.is_empty() {
            return None;
        }

        let mut search_shared = display.is_empty();

        let mut iter = None;
        if !search_shared {
            let i = find_display(&self.displays, display)?;
            iter = Some(i);

            let shared_views = &self.displays[i].1.shared_views;
            search_shared = contain(shared_views, view);
        }

        let views = match iter {
            Some(i) if !search_shared => &self.displays[i].1.views,
            _ => &self.shared_views,
        };
        find_view(views, view).map(|i| &views[i])
    }

    /// The active views among `views`: those of the environment's list, else of the config's
    /// list, in that list's order and spelling; all of `views` when that leaves none.
    ///
    /// Port of `Config::Impl::getActiveViews` (src/OpenColorIO/Config.cpp:808-836 @ v2.5.2).
    fn impl_active_views(&self, views: &[Vec<u8>]) -> StringVec {
        let mut active_views = StringVec::new();
        if !self.active_views_env_override.is_empty() {
            let ordered_views =
                intersect_string_vecs_case_ignore(&self.active_views_env_override, views);

            if !ordered_views.is_empty() {
                active_views = ordered_views;
            }
        } else if !self.active_views.is_empty() {
            let ordered_views = intersect_string_vecs_case_ignore(&self.active_views, views);

            if !ordered_views.is_empty() {
                active_views = ordered_views;
            }
        }

        if active_views.is_empty() {
            active_views = views.to_vec();
        }
        active_views
    }

    /// The active views of `views` for an image in the color space `image_cs_name`: the views
    /// without a viewing rule, and those whose rule names that color space (or a role of it)
    /// or its encoding. `view_names` gets the names of `views`. Upstream compares the name it
    /// is given, not the color space's own, and the encoding as the color space writes it with
    /// the rule's in lower case (docs/improvements.md, I-136).
    ///
    /// Port of `Config::Impl::getFilteredViews` (src/OpenColorIO/Config.cpp:838-901 @ v2.5.2).
    fn impl_filtered_views(
        &self,
        view_names: &mut StringVec,
        views: &[&View],
        image_cs_name: &[u8],
    ) -> Result<StringVec> {
        let image_cs_name = c_str(image_cs_name);
        let Some(image_color_space) = self.impl_color_space(image_cs_name) else {
            return Err(Exception::new(
                [
                    b"Could not find source color space '".as_slice(),
                    image_cs_name,
                    b"'.",
                ]
                .concat(),
            ));
        };

        let view_encoding = image_color_space.encoding();

        *view_names = get_view_names(views);
        let active_views = self.impl_active_views(view_names);

        let image_color_space_name = lower(image_cs_name);
        let viewing_rules = self.viewing_rules.get();
        let mut filtered_active_views = StringVec::new();
        for view in &active_views {
            let idx = find_in_string_vec_case_ignore(view_names, view);
            let rule_name = &views[idx as usize].rule;
            if rule_name.is_empty() {
                // Include all views that do not have a rule.
                filtered_active_views.push(view.clone());
            } else if let Some(rule_idx) = find_rule(&viewing_rules, rule_name) {
                let numcs = viewing_rules
                    .num_color_spaces(rule_idx)
                    .expect("a rule FindRule found");
                let mut added = false;
                for cs_idx in 0..numcs {
                    // Rule can use role names.
                    let rolename = viewing_rules
                        .color_space(rule_idx, cs_idx)
                        .expect("a rule FindRule found")
                        .expect("a color space in the list");
                    let csname = lookup_role(&self.roles, rolename);

                    let cs_name = if !csname.is_empty() { csname } else { rolename };
                    if lower(cs_name) == image_color_space_name {
                        // Include a view if its rule contains the image's color space.
                        filtered_active_views.push(view.clone());
                        added = true;
                        break;
                    }
                }
                if !added && !view_encoding.is_empty() {
                    let num_enc = viewing_rules
                        .num_encodings(rule_idx)
                        .expect("a rule FindRule found");
                    for enc_idx in 0..num_enc {
                        let enc_name = viewing_rules
                            .encoding(rule_idx, enc_idx)
                            .expect("a rule FindRule found")
                            .expect("an encoding in the list");
                        if lower(enc_name) == view_encoding {
                            // Include a view if its rule contains the image's color space
                            // encoding.
                            filtered_active_views.push(view.clone());
                            break;
                        }
                    }
                }
            }
        }
        Ok(filtered_active_views)
    }

    /// The active displays, computed when first needed after a change.
    ///
    /// Port of `Config::Impl::updateDisplayCache` (src/OpenColorIO/Config.cpp:903-913 @
    /// v2.5.2). Upstream recomputes an empty cache each time; the port computes it once after
    /// each change, which gives the same displays, as every change of what it is computed
    /// from clears it.
    fn display_cache(&self) -> &StringVec {
        self.display_cache.get_or_init(|| {
            let mut cache = StringVec::new();
            compute_displays(
                &mut cache,
                &self.displays,
                &self.active_displays,
                &self.active_displays_env_override,
            );
            cache
        })
    }

    /// Empties the cache of active displays (`m_displayCache.clear()`).
    fn clear_display_cache(&mut self) {
        self.display_cache = OnceLock::new();
    }

    /// The config's viewing rules, shared with the config (see [`ConfigRules`]).
    ///
    /// Port of `Config::getViewingRules` (src/OpenColorIO/Config.cpp:3332-3335 @ v2.5.2).
    #[doc(alias = "getViewingRules")]
    pub fn viewing_rules(&self) -> ConfigRules<ViewingRules> {
        self.viewing_rules.clone()
    }

    /// Sets the config's viewing rules to a copy of `viewing_rules`. Handles taken from
    /// [`Config::viewing_rules`] before keep the rules they had.
    ///
    /// Port of `Config::setViewingRules` (src/OpenColorIO/Config.cpp:3337-3343 @ v2.5.2).
    #[doc(alias = "setViewingRules")]
    pub fn set_viewing_rules(&mut self, viewing_rules: &ViewingRules) {
        self.viewing_rules = ConfigRules::new(viewing_rules.clone());

        self.reset_cache_ids();
    }

    /// Whether `view_name` is one of the shared views of `disp_name` (with an empty display
    /// name, of the config), ignoring case.
    ///
    /// Port of `Config::isViewShared` (src/OpenColorIO/Config.cpp:3345-3359 @ v2.5.2).
    #[doc(alias = "isViewShared")]
    pub fn is_view_shared(&self, disp_name: impl AsRef<[u8]>, view_name: impl AsRef<[u8]>) -> bool {
        let disp_name = disp_name.as_ref();
        let view_name = c_str(view_name.as_ref());
        if view_name.is_empty() {
            return false;
        }

        for v in 0..self.num_views_of_type(ViewType::Shared, disp_name) {
            let shared_view_name = self.view_of_type(ViewType::Shared, disp_name, v);
            if !shared_view_name.is_empty() && strcasecmp(shared_view_name, view_name).is_eq() {
                return true;
            }
        }

        false
    }

    /// Adds a shared view to the config, or replaces the one of that name (ignoring case).
    ///
    /// Port of `Config::addSharedView` (src/OpenColorIO/Config.cpp:3361-3384 @ v2.5.2).
    #[doc(alias = "addSharedView")]
    pub fn add_shared_view(
        &mut self,
        view: impl AsRef<[u8]>,
        view_transform: impl AsRef<[u8]>,
        color_space: impl AsRef<[u8]>,
        looks: impl AsRef<[u8]>,
        rule: impl AsRef<[u8]>,
        description: impl AsRef<[u8]>,
    ) -> Result<()> {
        let view = c_str(view.as_ref());
        if view.is_empty() {
            return Err(Exception::new(
                "Shared view could not be added to config, view name has to be a non-empty name.",
            ));
        }

        let color_space = c_str(color_space.as_ref());
        if color_space.is_empty() {
            return Err(Exception::new(
                "Shared view could not be added to config, color space name has to be a \
                 non-empty name.",
            ));
        }

        add_view(
            &mut self.shared_views,
            view,
            view_transform.as_ref(),
            color_space,
            looks.as_ref(),
            rule.as_ref(),
            description.as_ref(),
        );

        self.clear_display_cache();

        self.reset_cache_ids();
        Ok(())
    }

    /// Removes the config's shared view `view` (ignoring case).
    ///
    /// Port of `Config::removeSharedView` (src/OpenColorIO/Config.cpp:3386-3412 @ v2.5.2).
    #[doc(alias = "removeSharedView")]
    pub fn remove_shared_view(&mut self, view: impl AsRef<[u8]>) -> Result<()> {
        let view = c_str(view.as_ref());
        if view.is_empty() {
            return Err(Exception::new(
                "Shared view could not be removed from config, view name has to be a non-empty \
                 name.",
            ));
        }

        match find_view(&self.shared_views, view) {
            Some(i) => {
                self.shared_views.remove(i);

                self.clear_display_cache();

                self.reset_cache_ids();
                Ok(())
            }
            None => Err(Exception::new(
                [
                    b"Shared view could not be removed from config. A shared view named '"
                        .as_slice(),
                    view,
                    b"' could not be found.",
                ]
                .concat(),
            )),
        }
    }

    /// Removes the config's shared views, the last first.
    ///
    /// Port of `Config::clearSharedViews` (src/OpenColorIO/Config.cpp:3414-3425 @ v2.5.2).
    #[doc(alias = "clearSharedViews")]
    pub fn clear_shared_views(&mut self) {
        let num_views = self.num_views_of_type(ViewType::Shared, b"");
        for v in (0..num_views).rev() {
            let shared_view_name = self.view_of_type(ViewType::Shared, b"", v).to_vec();
            if !shared_view_name.is_empty() {
                // The view exists: removing it can't fail.
                let _ = self.remove_shared_view(&shared_view_name);
            }
        }
    }

    /// The first active display; `""` for none.
    ///
    /// Port of `Config::getDefaultDisplay` (src/OpenColorIO/Config.cpp:3427-3430 @ v2.5.2).
    #[doc(alias = "getDefaultDisplay")]
    pub fn default_display(&self) -> &[u8] {
        self.display(0)
    }

    /// The number of active displays.
    ///
    /// Port of `Config::getNumDisplays` (src/OpenColorIO/Config.cpp:3432-3437 @ v2.5.2).
    #[doc(alias = "getNumDisplays")]
    pub fn num_displays(&self) -> i32 {
        self.display_cache().len() as i32
    }

    /// The active display at `index`; `""` outside them.
    ///
    /// Port of `Config::getDisplay` (src/OpenColorIO/Config.cpp:3439-3449 @ v2.5.2).
    #[doc(alias = "getDisplay")]
    pub fn display(&self, index: i32) -> &[u8] {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.display_cache().get(i))
            .map_or(&[], Vec::as_slice)
    }

    /// The first active view of `display`; `""` for none.
    ///
    /// Port of `Config::getDefaultView(const char *)` (src/OpenColorIO/Config.cpp:3451-3454 @
    /// v2.5.2).
    #[doc(alias = "getDefaultView")]
    pub fn default_view(&self, display: impl AsRef<[u8]>) -> &[u8] {
        self.view(display, 0)
    }

    /// The first view of `display` for an image in `colorspace_name` (see
    /// [`Config::view_for_color_space`]).
    ///
    /// Port of `Config::getDefaultView(const char *, const char *)`
    /// (src/OpenColorIO/Config.cpp:3456-3459 @ v2.5.2).
    #[doc(alias = "getDefaultView")]
    pub fn default_view_for_color_space(
        &self,
        display: impl AsRef<[u8]>,
        colorspace_name: impl AsRef<[u8]>,
    ) -> Result<&[u8]> {
        self.view_for_color_space(display, colorspace_name, 0)
    }

    /// The number of active views of `display` (active or not, any display); 0 for an unknown
    /// display.
    ///
    /// Port of `Config::getNumViews(const char *)` (src/OpenColorIO/Config.cpp:3461-3473 @
    /// v2.5.2).
    #[doc(alias = "getNumViews")]
    pub fn num_views(&self, display: impl AsRef<[u8]>) -> i32 {
        let display = c_str(display.as_ref());
        if display.is_empty() {
            return 0;
        }

        let Some(iter) = find_display(&self.displays, display) else {
            return 0;
        };

        let views = self.impl_views(&self.displays[iter].1);

        let master_views = get_view_names(&views);
        let active_views = self.impl_active_views(&master_views);
        active_views.len() as i32
    }

    /// The active view of `display` at `index`; `""` outside them.
    ///
    /// Port of `Config::getView(const char *, int)` (src/OpenColorIO/Config.cpp:3475-3500 @
    /// v2.5.2).
    #[doc(alias = "getView")]
    pub fn view(&self, display: impl AsRef<[u8]>, index: i32) -> &[u8] {
        let display = c_str(display.as_ref());
        if display.is_empty() {
            return &[];
        }

        // Include all displays, do not limit to active displays. Consider active views only.
        let Some(iter) = find_display(&self.displays, display) else {
            return &[];
        };

        let views = self.impl_views(&self.displays[iter].1);

        let master_views = get_view_names(&views);
        let active_views = self.impl_active_views(&master_views);

        let Some(active) = usize::try_from(index)
            .ok()
            .and_then(|i| active_views.get(i))
        else {
            return &[];
        };
        let idx = find_in_string_vec_case_ignore(&master_views, active);

        match usize::try_from(idx).ok().and_then(|i| views.get(i)) {
            Some(view) => &view.name,
            None => &[],
        }
    }

    /// The number of active views of `display` for an image in the color space `colorspace`
    /// (the views without a viewing rule, and those whose rule fits it); 0 for an unknown
    /// display or an empty color space name, an error for an unknown color space.
    ///
    /// Port of `Config::getNumViews(const char *, const char *)`
    /// (src/OpenColorIO/Config.cpp:3502-3517 @ v2.5.2).
    #[doc(alias = "getNumViews")]
    pub fn num_views_for_color_space(
        &self,
        display: impl AsRef<[u8]>,
        colorspace: impl AsRef<[u8]>,
    ) -> Result<i32> {
        let display = c_str(display.as_ref());
        let colorspace = c_str(colorspace.as_ref());
        if display.is_empty() || colorspace.is_empty() {
            return Ok(0);
        }

        let Some(iter) = find_display(&self.displays, display) else {
            return Ok(0);
        };

        let views = self.impl_views(&self.displays[iter].1);

        let mut view_names = StringVec::new();
        let filtered_views = self.impl_filtered_views(&mut view_names, &views, colorspace)?;

        Ok(filtered_views.len() as i32)
    }

    /// The view of `display` at `index` among those for an image in the color space
    /// `colorspace`. When there are none, the view at `index` among all of the display's, else
    /// its first view.
    ///
    /// Port of `Config::getView(const char *, const char *, int)`
    /// (src/OpenColorIO/Config.cpp:3519-3554 @ v2.5.2).
    #[doc(alias = "getView")]
    pub fn view_for_color_space(
        &self,
        display: impl AsRef<[u8]>,
        colorspace: impl AsRef<[u8]>,
        index: i32,
    ) -> Result<&[u8]> {
        let display = c_str(display.as_ref());
        let colorspace = c_str(colorspace.as_ref());
        if display.is_empty() || colorspace.is_empty() {
            return Ok(&[]);
        }

        let Some(iter) = find_display(&self.displays, display) else {
            return Ok(&[]);
        };

        let views = self.impl_views(&self.displays[iter].1);

        let mut view_names = StringVec::new();
        let filtered_views = self.impl_filtered_views(&mut view_names, &views, colorspace)?;
        let mut idx = index;

        if !filtered_views.is_empty() {
            let Some(filtered) = usize::try_from(index)
                .ok()
                .and_then(|i| filtered_views.get(i))
            else {
                return Ok(&[]);
            };
            idx = find_in_string_vec_case_ignore(&view_names, filtered);
        }

        if let Some(view) = usize::try_from(idx).ok().and_then(|i| views.get(i)) {
            return Ok(&view.name);
        }

        if let Some(view) = views.first() {
            return Ok(&view.name);
        }

        Ok(&[])
    }

    /// Whether the two configs have the view `view_name` of `disp_name` (or the shared view,
    /// with an empty display name), with the same color space, looks, view transform and rule,
    /// ignoring case (not the description).
    ///
    /// Port of `Config::AreViewsEqual` (src/OpenColorIO/Config.cpp:3557-3591 @ v2.5.2).
    #[doc(alias = "AreViewsEqual")]
    pub fn are_views_equal(
        first: &Config,
        second: &Config,
        disp_name: impl AsRef<[u8]>,
        view_name: impl AsRef<[u8]>,
    ) -> bool {
        let (disp_name, view_name) = (disp_name.as_ref(), view_name.as_ref());
        // It's ok to call this even for displays/views that don't exist, it will simply return
        // false.

        let cs1 = first.display_view_color_space_name(disp_name, view_name);
        let cs2 = second.display_view_color_space_name(disp_name, view_name);

        // If the color space is not empty, the display and view exist.
        if !cs1.is_empty() && !cs2.is_empty() && strcasecmp(cs1, cs2).is_eq() {
            // Note the remaining strings may be empty in a valid view.
            // Intentionally not checking the description since it is not a functional
            // difference.
            if strcasecmp(
                first.display_view_looks(disp_name, view_name),
                second.display_view_looks(disp_name, view_name),
            )
            .is_eq()
                && strcasecmp(
                    first.display_view_transform_name(disp_name, view_name),
                    second.display_view_transform_name(disp_name, view_name),
                )
                .is_eq()
                && strcasecmp(
                    first.display_view_rule(disp_name, view_name),
                    second.display_view_rule(disp_name, view_name),
                )
                .is_eq()
            {
                return true;
            }
        }
        false
    }

    /// The view transform of the view `view` of `display` (see [`Config::has_view`]); `""` for
    /// none.
    ///
    /// Port of `Config::getDisplayViewTransformName` (src/OpenColorIO/Config.cpp:3593-3599 @
    /// v2.5.2).
    #[doc(alias = "getDisplayViewTransformName")]
    pub fn display_view_transform_name(
        &self,
        display: impl AsRef<[u8]>,
        view: impl AsRef<[u8]>,
    ) -> &[u8] {
        self.impl_view(display.as_ref(), view.as_ref())
            .map_or(&[], |v| &v.view_transform)
    }

    /// Port of `Config::getDisplayViewColorSpaceName` (src/OpenColorIO/Config.cpp:3601-3607 @
    /// v2.5.2).
    #[doc(alias = "getDisplayViewColorSpaceName")]
    pub fn display_view_color_space_name(
        &self,
        display: impl AsRef<[u8]>,
        view: impl AsRef<[u8]>,
    ) -> &[u8] {
        self.impl_view(display.as_ref(), view.as_ref())
            .map_or(&[], |v| &v.colorspace)
    }

    /// Port of `Config::getDisplayViewLooks` (src/OpenColorIO/Config.cpp:3609-3615 @ v2.5.2).
    #[doc(alias = "getDisplayViewLooks")]
    pub fn display_view_looks(&self, display: impl AsRef<[u8]>, view: impl AsRef<[u8]>) -> &[u8] {
        self.impl_view(display.as_ref(), view.as_ref())
            .map_or(&[], |v| &v.looks)
    }

    /// Port of `Config::getDisplayViewRule` (src/OpenColorIO/Config.cpp:3617-3622 @ v2.5.2).
    #[doc(alias = "getDisplayViewRule")]
    pub fn display_view_rule(&self, display: impl AsRef<[u8]>, view: impl AsRef<[u8]>) -> &[u8] {
        self.impl_view(display.as_ref(), view.as_ref())
            .map_or(&[], |v| &v.rule)
    }

    /// Port of `Config::getDisplayViewDescription` (src/OpenColorIO/Config.cpp:3624-3629 @
    /// v2.5.2).
    #[doc(alias = "getDisplayViewDescription")]
    pub fn display_view_description(
        &self,
        display: impl AsRef<[u8]>,
        view: impl AsRef<[u8]>,
    ) -> &[u8] {
        self.impl_view(display.as_ref(), view.as_ref())
            .map_or(&[], |v| &v.description)
    }

    /// Whether `disp_name` has the view `view_name` (its own or a shared one it names), active
    /// or not; with an empty display name, whether the config has that shared view.
    ///
    /// Port of `Config::hasView` (src/OpenColorIO/Config.cpp:3631-3644 @ v2.5.2).
    #[doc(alias = "hasView")]
    pub fn has_view(&self, disp_name: impl AsRef<[u8]>, view_name: impl AsRef<[u8]>) -> bool {
        // All views must have a color space, so if it's not empty, the view exists.
        !self
            .display_view_color_space_name(disp_name, view_name)
            .is_empty()
    }

    /// Adds the config's shared view `shared_view` to `display` (made if needed).
    ///
    /// Port of `Config::addDisplaySharedView` (src/OpenColorIO/Config.cpp:3646-3695 @ v2.5.2).
    #[doc(alias = "addDisplaySharedView")]
    pub fn add_display_shared_view(
        &mut self,
        display: impl AsRef<[u8]>,
        shared_view: impl AsRef<[u8]>,
    ) -> Result<()> {
        let display = c_str(display.as_ref());
        let shared_view = c_str(shared_view.as_ref());
        if display.is_empty() {
            return Err(Exception::new(
                "Shared view could not be added to display: non-empty display name is needed.",
            ));
        }
        if shared_view.is_empty() {
            return Err(Exception::new(
                "Shared view could not be added to display: non-empty view name is needed.",
            ));
        }

        let mut invalidate_cache = false;
        let iter = match find_display(&self.displays, display) {
            Some(i) => i,
            None => {
                self.displays.push((display.to_vec(), Display::default()));
                invalidate_cache = true;
                self.displays.len() - 1
            }
        };

        let existing_views = &self.displays[iter].1.views;
        if find_view(existing_views, shared_view).is_some() {
            return Err(Exception::new(
                [
                    b"There is already a view named '".as_slice(),
                    shared_view,
                    b"' in the display '",
                    display,
                    b"'.",
                ]
                .concat(),
            ));
        }

        let views = &mut self.displays[iter].1.shared_views;
        if contain(views, shared_view) {
            return Err(Exception::new(
                [
                    b"There is already a shared view named '".as_slice(),
                    shared_view,
                    b"' in the display '",
                    display,
                    b"'.",
                ]
                .concat(),
            ));
        }
        views.push(shared_view.to_vec());
        if invalidate_cache {
            self.clear_display_cache();
        }
        self.reset_cache_ids();
        Ok(())
    }

    /// Adds the view `view` of `display` (made if needed) for the color space `color_space`
    /// with `looks`, or replaces the view of that name.
    ///
    /// Port of `Config::addDisplayView(const char *, const char *, const char *, const char *)`
    /// (src/OpenColorIO/Config.cpp:3697-3701 @ v2.5.2).
    #[doc(alias = "addDisplayView")]
    pub fn add_display_view(
        &mut self,
        display: impl AsRef<[u8]>,
        view: impl AsRef<[u8]>,
        color_space: impl AsRef<[u8]>,
        looks: impl AsRef<[u8]>,
    ) -> Result<()> {
        self.add_display_view_with_view_transform(display, view, b"", color_space, looks, b"", b"")
    }

    /// Adds the view `view` of `display` (made if needed), or replaces the view of that name
    /// (ignoring case). Refuses a view named as one of the display's shared views.
    ///
    /// Port of `Config::addDisplayView(const char *, const char *, const char *, const char *,
    /// const char *, const char *, const char *)` (src/OpenColorIO/Config.cpp:3703-3750 @
    /// v2.5.2).
    #[doc(alias = "addDisplayView")]
    #[allow(clippy::too_many_arguments)]
    pub fn add_display_view_with_view_transform(
        &mut self,
        display: impl AsRef<[u8]>,
        view: impl AsRef<[u8]>,
        view_transform: impl AsRef<[u8]>,
        color_space: impl AsRef<[u8]>,
        looks: impl AsRef<[u8]>,
        rule: impl AsRef<[u8]>,
        description: impl AsRef<[u8]>,
    ) -> Result<()> {
        let display = c_str(display.as_ref());
        let view = c_str(view.as_ref());
        let color_space = c_str(color_space.as_ref());
        if display.is_empty() {
            return Err(Exception::new(
                "View could not be added to display in config: a non-empty display name is \
                 needed.",
            ));
        }
        if view.is_empty() {
            return Err(Exception::new(
                "View could not be added to display in config: a non-empty view name is needed.",
            ));
        }
        if color_space.is_empty() {
            return Err(Exception::new(
                "View could not be added to display in config: a non-empty color space name is \
                 needed.",
            ));
        }

        match find_display(&self.displays, display) {
            None => {
                let mut d = Display::default();
                d.views.push(View::new(
                    view,
                    view_transform.as_ref(),
                    color_space,
                    looks.as_ref(),
                    rule.as_ref(),
                    description.as_ref(),
                ));
                self.displays.push((display.to_vec(), d));
                self.clear_display_cache();
            }
            Some(iter) => {
                if contain(&self.displays[iter].1.shared_views, view) {
                    return Err(Exception::new(
                        [
                            b"There is already a shared view named '".as_slice(),
                            view,
                            b"' in the display '",
                            display,
                            b"'.",
                        ]
                        .concat(),
                    ));
                }

                add_view(
                    &mut self.displays[iter].1.views,
                    view,
                    view_transform.as_ref(),
                    color_space,
                    looks.as_ref(),
                    rule.as_ref(),
                    description.as_ref(),
                );
            }
        }

        self.reset_cache_ids();
        Ok(())
    }

    /// Removes the view (or the reference to a shared view) `view` from `display`, and the
    /// display when it has no view left.
    ///
    /// Port of `Config::removeDisplayView` (src/OpenColorIO/Config.cpp:3752-3805 @ v2.5.2).
    #[doc(alias = "removeDisplayView")]
    pub fn remove_display_view(
        &mut self,
        display: impl AsRef<[u8]>,
        view: impl AsRef<[u8]>,
    ) -> Result<()> {
        let display = c_str(display.as_ref());
        let view = c_str(view.as_ref());
        if display.is_empty() {
            return Err(Exception::new(
                "Can't remove a view from a display with an empty display name.",
            ));
        }
        if view.is_empty() {
            return Err(Exception::new(
                "Can't remove a view from a display with an empty view name.",
            ));
        }

        // Check if the display exists.

        let Some(iter) = find_display(&self.displays, display) else {
            return Err(Exception::new(
                [
                    b"Could not find a display named '".as_slice(),
                    display,
                    b"' to be removed from config.",
                ]
                .concat(),
            ));
        };

        let entry = &mut self.displays[iter].1;
        if !remove(&mut entry.shared_views, view) {
            // view is not a shared view.
            // Is it a view?
            let Some(view_it) = find_view(&entry.views, view) else {
                return Err(Exception::new(
                    [
                        b"Could not find a view named '".as_slice(),
                        view,
                        b" to be removed from the display named '",
                        display,
                        b"'.",
                    ]
                    .concat(),
                ));
            };

            entry.views.remove(view_it);
        }

        // Check if the display needs to be removed also.
        if entry.views.is_empty() && entry.shared_views.is_empty() {
            self.displays.remove(iter);
        }

        self.clear_display_cache();

        self.reset_cache_ids();
        Ok(())
    }

    /// Removes every display.
    ///
    /// Port of `Config::clearDisplays` (src/OpenColorIO/Config.cpp:3807-3814 @ v2.5.2).
    #[doc(alias = "clearDisplays")]
    pub fn clear_displays(&mut self) {
        self.displays.clear();
        self.clear_display_cache();

        self.reset_cache_ids();
    }

    /// The number of views of `type_` of `display`: its shared or its own views; with an empty
    /// display name, the config's shared views; 0 for an unknown display.
    ///
    /// Port of `Config::getNumViews(ViewType, const char *)` (src/OpenColorIO/Config.cpp:
    /// 4388-4407 @ v2.5.2).
    #[doc(alias = "getNumViews")]
    pub fn num_views_of_type(&self, type_: ViewType, display: impl AsRef<[u8]>) -> i32 {
        let display = c_str(display.as_ref());
        if display.is_empty() {
            return self.shared_views.len() as i32;
        }

        let Some(iter) = find_display(&self.displays, display) else {
            return 0;
        };

        match type_ {
            ViewType::Shared => self.displays[iter].1.shared_views.len() as i32,
            ViewType::DisplayDefined => self.displays[iter].1.views.len() as i32,
        }
    }

    /// The view of `type_` of `display` at `index` (see [`Config::num_views_of_type`]); `""`
    /// outside them.
    ///
    /// Port of `Config::getView(ViewType, const char *, int)` (src/OpenColorIO/Config.cpp:
    /// 4409-4446 @ v2.5.2).
    #[doc(alias = "getView")]
    pub fn view_of_type(&self, type_: ViewType, display: impl AsRef<[u8]>, index: i32) -> &[u8] {
        let display = c_str(display.as_ref());
        let index = usize::try_from(index).ok();
        if display.is_empty() {
            return index
                .and_then(|i| self.shared_views.get(i))
                .map_or(&[], |v| &v.name);
        }

        let Some(iter) = find_display(&self.displays, display) else {
            return &[];
        };

        match type_ {
            ViewType::Shared => index
                .and_then(|i| self.displays[iter].1.shared_views.get(i))
                .map_or(&[], Vec::as_slice),
            ViewType::DisplayDefined => index
                .and_then(|i| self.displays[iter].1.views.get(i))
                .map_or(&[], |v| &v.name),
        }
    }

    // Virtual display /////////////////////////////////////////////////////////////////////////

    /// Whether the virtual display has the view `view_name` (its own or a shared one).
    ///
    /// Port of `Config::hasVirtualView` (src/OpenColorIO/Config.cpp:3816-3822 @ v2.5.2).
    #[doc(alias = "hasVirtualView")]
    pub fn has_virtual_view(&self, view_name: impl AsRef<[u8]>) -> bool {
        // All views must have a color space, so if it's not empty, the view exists.
        !self
            .virtual_display_view_color_space_name(view_name)
            .is_empty()
    }

    /// Whether `view_name` is one of the virtual display's shared views, ignoring case.
    ///
    /// Port of `Config::isVirtualViewShared` (src/OpenColorIO/Config.cpp:3824-3838 @ v2.5.2).
    #[doc(alias = "isVirtualViewShared")]
    pub fn is_virtual_view_shared(&self, view_name: impl AsRef<[u8]>) -> bool {
        let view_name = c_str(view_name.as_ref());
        if view_name.is_empty() {
            return false;
        }

        for v in 0..self.virtual_display_num_views(ViewType::Shared) {
            let shared_view_name = self.virtual_display_view(ViewType::Shared, v);
            if !shared_view_name.is_empty() && strcasecmp(shared_view_name, view_name).is_eq() {
                return true;
            }
        }

        false
    }

    /// Adds a view to the virtual display; refuses one of a name it has (ignoring case).
    ///
    /// Port of `Config::addVirtualDisplayView` (src/OpenColorIO/Config.cpp:3840-3874 @ v2.5.2).
    #[doc(alias = "addVirtualDisplayView")]
    pub fn add_virtual_display_view(
        &mut self,
        view: impl AsRef<[u8]>,
        view_transform: impl AsRef<[u8]>,
        color_space: impl AsRef<[u8]>,
        looks: impl AsRef<[u8]>,
        rule: impl AsRef<[u8]>,
        description: impl AsRef<[u8]>,
    ) -> Result<()> {
        let view = c_str(view.as_ref());
        let color_space = c_str(color_space.as_ref());
        if view.is_empty() {
            return Err(Exception::new(
                "View could not be added to virtual_display in config: a non-empty view name is \
                 needed.",
            ));
        }

        if color_space.is_empty() {
            return Err(Exception::new(
                "View could not be added to virtual_display in config: a non-empty color space \
                 name is needed.",
            ));
        }

        if find_view(&self.virtual_display.views, view).is_some() {
            return Err(Exception::new(
                [
                    b"View could not be added to virtual_display in config: View '".as_slice(),
                    view,
                    b"' already exists.",
                ]
                .concat(),
            ));
        }

        self.virtual_display.views.push(View::new(
            view,
            view_transform.as_ref(),
            color_space,
            looks.as_ref(),
            rule.as_ref(),
            description.as_ref(),
        ));

        self.reset_cache_ids();
        Ok(())
    }

    /// Adds a reference to the config's shared view `shared_view` to the virtual display.
    ///
    /// Port of `Config::addVirtualDisplaySharedView` (src/OpenColorIO/Config.cpp:3876-3898 @
    /// v2.5.2).
    #[doc(alias = "addVirtualDisplaySharedView")]
    pub fn add_virtual_display_shared_view(&mut self, shared_view: impl AsRef<[u8]>) -> Result<()> {
        let shared_view = c_str(shared_view.as_ref());
        if shared_view.is_empty() {
            return Err(Exception::new(
                "Shared view could not be added to virtual_display: non-empty view name is \
                 needed.",
            ));
        }

        let views = &mut self.virtual_display.shared_views;
        if contain(views, shared_view) {
            return Err(Exception::new(
                [
                    b"Shared view could not be added to virtual_display: There is already a \
                      shared view named '"
                        .as_slice(),
                    shared_view,
                    b"'.",
                ]
                .concat(),
            ));
        }

        views.push(shared_view.to_vec());

        self.reset_cache_ids();
        Ok(())
    }

    /// The number of the virtual display's shared or own views.
    ///
    /// Port of `Config::getVirtualDisplayNumViews` (src/OpenColorIO/Config.cpp:3900-3911 @
    /// v2.5.2).
    #[doc(alias = "getVirtualDisplayNumViews")]
    pub fn virtual_display_num_views(&self, type_: ViewType) -> i32 {
        match type_ {
            ViewType::DisplayDefined => self.virtual_display.views.len() as i32,
            ViewType::Shared => self.virtual_display.shared_views.len() as i32,
        }
    }

    /// The virtual display's shared or own view at `index`; `""` outside them.
    ///
    /// Port of `Config::getVirtualDisplayView` (src/OpenColorIO/Config.cpp:3913-3938 @
    /// v2.5.2).
    #[doc(alias = "getVirtualDisplayView")]
    pub fn virtual_display_view(&self, type_: ViewType, index: i32) -> &[u8] {
        let index = usize::try_from(index).ok();
        match type_ {
            ViewType::DisplayDefined => index
                .and_then(|i| self.virtual_display.views.get(i))
                .map_or(&[], |v| &v.name),
            ViewType::Shared => index
                .and_then(|i| self.virtual_display.shared_views.get(i))
                .map_or(&[], Vec::as_slice),
        }
    }

    /// Whether the two configs' virtual displays have the view `view_name` with the same color
    /// space, looks, view transform and rule, ignoring case (not the description).
    ///
    /// Port of `Config::AreVirtualViewsEqual` (src/OpenColorIO/Config.cpp:3940-3966 @ v2.5.2).
    #[doc(alias = "AreVirtualViewsEqual")]
    pub fn are_virtual_views_equal(
        first: &Config,
        second: &Config,
        view_name: impl AsRef<[u8]>,
    ) -> bool {
        let view_name = view_name.as_ref();
        let cs1 = first.virtual_display_view_color_space_name(view_name);
        let cs2 = second.virtual_display_view_color_space_name(view_name);

        // If the color space is not empty, the display and view exist.
        if !cs1.is_empty() && !cs2.is_empty() && strcasecmp(cs1, cs2).is_eq() {
            // Note the remaining strings may be empty in a valid view.
            // Intentionally not checking the description since it is not a functional
            // difference.
            if strcasecmp(
                first.virtual_display_view_looks(view_name),
                second.virtual_display_view_looks(view_name),
            )
            .is_eq()
                && strcasecmp(
                    first.virtual_display_view_transform_name(view_name),
                    second.virtual_display_view_transform_name(view_name),
                )
                .is_eq()
                && strcasecmp(
                    first.virtual_display_view_rule(view_name),
                    second.virtual_display_view_rule(view_name),
                )
                .is_eq()
            {
                return true;
            }
        }
        false
    }

    /// A field of the virtual display's view `view`: the config's shared view when the virtual
    /// display shares it, else its own view of that name (ignoring case); `""` for none.
    ///
    /// The body of upstream's `getVirtualDisplayView*` getters (src/OpenColorIO/Config.cpp:
    /// 3968-4061 @ v2.5.2).
    fn virtual_display_view_field(&self, view: &[u8], field: fn(&View) -> &[u8]) -> &[u8] {
        // Get the field for the case where a virtual view is shared.
        if self.is_virtual_view_shared(view) {
            return self.impl_view(b"", view).map_or(&[], field);
        }

        match find_view(&self.virtual_display.views, c_str(view)) {
            Some(i) => field(&self.virtual_display.views[i]),
            None => &[],
        }
    }

    /// Port of `Config::getVirtualDisplayViewTransformName` (src/OpenColorIO/Config.cpp:
    /// 3968-3985 @ v2.5.2).
    #[doc(alias = "getVirtualDisplayViewTransformName")]
    pub fn virtual_display_view_transform_name(&self, view: impl AsRef<[u8]>) -> &[u8] {
        self.virtual_display_view_field(view.as_ref(), |v| &v.view_transform)
    }

    /// Port of `Config::getVirtualDisplayViewColorSpaceName` (src/OpenColorIO/Config.cpp:
    /// 3987-4004 @ v2.5.2).
    #[doc(alias = "getVirtualDisplayViewColorSpaceName")]
    pub fn virtual_display_view_color_space_name(&self, view: impl AsRef<[u8]>) -> &[u8] {
        self.virtual_display_view_field(view.as_ref(), |v| &v.colorspace)
    }

    /// Port of `Config::getVirtualDisplayViewLooks` (src/OpenColorIO/Config.cpp:4006-4023 @
    /// v2.5.2).
    #[doc(alias = "getVirtualDisplayViewLooks")]
    pub fn virtual_display_view_looks(&self, view: impl AsRef<[u8]>) -> &[u8] {
        self.virtual_display_view_field(view.as_ref(), |v| &v.looks)
    }

    /// Port of `Config::getVirtualDisplayViewRule` (src/OpenColorIO/Config.cpp:4025-4042 @
    /// v2.5.2).
    #[doc(alias = "getVirtualDisplayViewRule")]
    pub fn virtual_display_view_rule(&self, view: impl AsRef<[u8]>) -> &[u8] {
        self.virtual_display_view_field(view.as_ref(), |v| &v.rule)
    }

    /// Port of `Config::getVirtualDisplayViewDescription` (src/OpenColorIO/Config.cpp:
    /// 4044-4061 @ v2.5.2).
    #[doc(alias = "getVirtualDisplayViewDescription")]
    pub fn virtual_display_view_description(&self, view: impl AsRef<[u8]>) -> &[u8] {
        self.virtual_display_view_field(view.as_ref(), |v| &v.description)
    }

    /// Removes the virtual display's own view `view`, else its shared view `view` (ignoring
    /// case); nothing for neither.
    ///
    /// Port of `Config::removeVirtualDisplayView` (src/OpenColorIO/Config.cpp:4063-4093 @
    /// v2.5.2).
    #[doc(alias = "removeVirtualDisplayView")]
    pub fn remove_virtual_display_view(&mut self, view: impl AsRef<[u8]>) {
        let view = c_str(view.as_ref());

        if find_view(&self.virtual_display.views, view).is_some() {
            let views = &mut self.virtual_display.views;

            if let Some(it) = views.iter().position(|v| compare(&v.name, view)) {
                views.remove(it);

                self.reset_cache_ids();
                return;
            }
        }

        if remove(&mut self.virtual_display.shared_views, view) {
            self.reset_cache_ids();
        }
    }

    /// Removes the virtual display's views.
    ///
    /// Port of `Config::clearVirtualDisplay` (src/OpenColorIO/Config.cpp:4095-4102 @ v2.5.2).
    #[doc(alias = "clearVirtualDisplay")]
    pub fn clear_virtual_display(&mut self) {
        self.virtual_display.views.clear();
        self.virtual_display.shared_views.clear();

        self.reset_cache_ids();
    }

    /// Makes a display from the virtual display for the monitor `monitor_name`. Its ICC
    /// profile is found by `SystemMonitor` (Phase 9) and read by the ICC reader (WP 4.7), not
    /// ported yet: past the check of the name, an error.
    ///
    /// Port of `Config::instantiateDisplayFromMonitorName` (src/OpenColorIO/Config.cpp:
    /// 4104-4118 @ v2.5.2), in part.
    #[doc(alias = "instantiateDisplayFromMonitorName")]
    pub fn instantiate_display_from_monitor_name(
        &mut self,
        monitor_name: impl AsRef<[u8]>,
    ) -> Result<i32> {
        if c_str(monitor_name.as_ref()).is_empty() {
            return Err(Exception::new("The system monitor name cannot be null."));
        }
        Err(Exception::new(
            "Config::instantiateDisplayFromMonitorName: the system monitors and the ICC reader \
             are not ported yet.",
        ))
    }

    /// Makes a display from the virtual display for the ICC profile `icc_profile_filepath`.
    /// The ICC reader (WP 4.7) is not ported yet: past the check of the path, an error.
    ///
    /// Port of `Config::instantiateDisplayFromICCProfile` (src/OpenColorIO/Config.cpp:
    /// 4120-4131 @ v2.5.2), in part.
    #[doc(alias = "instantiateDisplayFromICCProfile")]
    pub fn instantiate_display_from_icc_profile(
        &mut self,
        icc_profile_filepath: impl AsRef<[u8]>,
    ) -> Result<i32> {
        if c_str(icc_profile_filepath.as_ref()).is_empty() {
            return Err(Exception::new("The ICC profile filepath cannot be null."));
        }
        Err(Exception::new(
            "Config::instantiateDisplayFromICCProfile: the ICC reader is not ported yet.",
        ))
    }

    // Active displays and views ///////////////////////////////////////////////////////////////

    /// The elements of `list` split as `SplitStringEnvStyle` does; a list that is one empty
    /// element is empty.
    ///
    /// The shared body of `Config::setActiveDisplays` and `setActiveViews`
    /// (src/OpenColorIO/Config.cpp:4133-4150, 4233-4250 @ v2.5.2).
    fn split_active_list(list: &[u8]) -> Result<StringVec> {
        let mut out = split_string_env_style(c_str(list))?;

        // SplitStringEnvStyle needs to always return a result, even if empty, for look
        // parsing. However, this does not count as an active display or view, so delete it.
        if out.len() == 1 && out[0].is_empty() {
            out.clear();
        }
        Ok(out)
    }

    /// Sets the config's list of active displays, split on `,` (or `:`).
    ///
    /// Port of `Config::setActiveDisplays` (src/OpenColorIO/Config.cpp:4133-4150 @ v2.5.2).
    #[doc(alias = "setActiveDisplays")]
    pub fn set_active_displays(&mut self, displays: impl AsRef<[u8]>) -> Result<()> {
        self.active_displays.clear();
        self.active_displays = Config::split_active_list(displays.as_ref())?;

        self.clear_display_cache();

        self.reset_cache_ids();
        Ok(())
    }

    /// The config's list of active displays, joined with `, ` (quoting the names that hold
    /// a separator).
    ///
    /// Port of `Config::getActiveDisplays` (src/OpenColorIO/Config.cpp:4152-4156 @ v2.5.2).
    #[doc(alias = "getActiveDisplays")]
    pub fn active_displays(&self) -> Vec<u8> {
        join_string_env_style(&self.active_displays)
    }

    /// Adds `display` to the config's list of active displays (unless it is there, spelled the
    /// same).
    ///
    /// Port of `Config::addActiveDisplay` (src/OpenColorIO/Config.cpp:4158-4179 @ v2.5.2).
    #[doc(alias = "addActiveDisplay")]
    pub fn add_active_display(&mut self, display: impl AsRef<[u8]>) -> Result<()> {
        let display = c_str(display.as_ref());
        if display.is_empty() {
            return Err(Exception::new(
                "Active display could not be added to config, display name was empty",
            ));
        }

        if self.active_displays.iter().any(|d| d == display) {
            // Display is already present.
            return Ok(());
        }

        self.active_displays.push(display.to_vec());

        self.clear_display_cache();
        self.reset_cache_ids();
        Ok(())
    }

    /// Removes `display` (spelled the same) from the config's list of active displays.
    ///
    /// Port of `Config::removeActiveDisplay` (src/OpenColorIO/Config.cpp:4181-4206 @ v2.5.2).
    #[doc(alias = "removeActiveDisplay")]
    pub fn remove_active_display(&mut self, display: impl AsRef<[u8]>) -> Result<()> {
        let display = c_str(display.as_ref());
        if display.is_empty() {
            return Err(Exception::new(
                "Active display could not be removed from config, display name was empty.",
            ));
        }

        match self.active_displays.iter().position(|d| d == display) {
            Some(it) => {
                self.active_displays.remove(it);
            }
            None => {
                return Err(Exception::new(
                    [
                        b"Active display could not be removed from config, display '".as_slice(),
                        display,
                        b"' was not found.",
                    ]
                    .concat(),
                ));
            }
        }

        self.clear_display_cache();
        self.reset_cache_ids();
        Ok(())
    }

    /// Port of `Config::clearActiveDisplays` (src/OpenColorIO/Config.cpp:4208-4215 @ v2.5.2).
    #[doc(alias = "clearActiveDisplays")]
    pub fn clear_active_displays(&mut self) {
        self.active_displays.clear();

        self.clear_display_cache();
        self.reset_cache_ids();
    }

    /// The config's active display at `index`, or `None` (upstream's null pointer) outside the
    /// list.
    ///
    /// Port of `Config::getActiveDisplay` (src/OpenColorIO/Config.cpp:4217-4226 @ v2.5.2).
    #[doc(alias = "getActiveDisplay")]
    pub fn active_display(&self, index: i32) -> Option<&[u8]> {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.active_displays.get(i))
            .map(Vec::as_slice)
    }

    /// Port of `Config::getNumActiveDisplays` (src/OpenColorIO/Config.cpp:4228-4231 @ v2.5.2).
    #[doc(alias = "getNumActiveDisplays")]
    pub fn num_active_displays(&self) -> i32 {
        self.active_displays.len() as i32
    }

    /// Sets the config's list of active views, split on `,` (or `:`).
    ///
    /// Port of `Config::setActiveViews` (src/OpenColorIO/Config.cpp:4233-4250 @ v2.5.2).
    #[doc(alias = "setActiveViews")]
    pub fn set_active_views(&mut self, views: impl AsRef<[u8]>) -> Result<()> {
        self.active_views.clear();
        self.active_views = Config::split_active_list(views.as_ref())?;

        self.clear_display_cache();

        self.reset_cache_ids();
        Ok(())
    }

    /// The config's list of active views, joined with `, ` (quoting the names that hold a
    /// separator).
    ///
    /// Port of `Config::getActiveViews` (src/OpenColorIO/Config.cpp:4252-4256 @ v2.5.2).
    #[doc(alias = "getActiveViews")]
    pub fn active_views(&self) -> Vec<u8> {
        join_string_env_style(&self.active_views)
    }

    /// Adds `view` to the config's list of active views (unless it is there, spelled the
    /// same).
    ///
    /// Port of `Config::addActiveView` (src/OpenColorIO/Config.cpp:4258-4279 @ v2.5.2).
    #[doc(alias = "addActiveView")]
    pub fn add_active_view(&mut self, view: impl AsRef<[u8]>) -> Result<()> {
        let view = c_str(view.as_ref());
        if view.is_empty() {
            return Err(Exception::new(
                "Active view could not be added to config, view name was empty.",
            ));
        }

        if self.active_views.iter().any(|v| v == view) {
            // View is already present.
            return Ok(());
        }

        self.active_views.push(view.to_vec());

        self.clear_display_cache();
        self.reset_cache_ids();
        Ok(())
    }

    /// Removes `view` (spelled the same) from the config's list of active views.
    ///
    /// Port of `Config::removeActiveView` (src/OpenColorIO/Config.cpp:4281-4306 @ v2.5.2).
    #[doc(alias = "removeActiveView")]
    pub fn remove_active_view(&mut self, view: impl AsRef<[u8]>) -> Result<()> {
        let view = c_str(view.as_ref());
        if view.is_empty() {
            return Err(Exception::new(
                "Active view could not be removed from config, view name was empty.",
            ));
        }

        match self.active_views.iter().position(|v| v == view) {
            Some(it) => {
                self.active_views.remove(it);
            }
            None => {
                return Err(Exception::new(
                    [
                        b"Active view could not be removed from config, view '".as_slice(),
                        view,
                        b"' was not found.",
                    ]
                    .concat(),
                ));
            }
        }

        self.clear_display_cache();
        self.reset_cache_ids();
        Ok(())
    }

    /// Port of `Config::clearActiveViews` (src/OpenColorIO/Config.cpp:4308-4315 @ v2.5.2).
    #[doc(alias = "clearActiveViews")]
    pub fn clear_active_views(&mut self) {
        self.active_views.clear();

        self.clear_display_cache();
        self.reset_cache_ids();
    }

    /// The config's active view at `index`, or `None` (upstream's null pointer) outside the
    /// list.
    ///
    /// Port of `Config::getActiveView` (src/OpenColorIO/Config.cpp:4317-4326 @ v2.5.2).
    #[doc(alias = "getActiveView")]
    pub fn active_view(&self, index: i32) -> Option<&[u8]> {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.active_views.get(i))
            .map(Vec::as_slice)
    }

    /// Port of `Config::getNumActiveViews` (src/OpenColorIO/Config.cpp:4328-4331 @ v2.5.2).
    #[doc(alias = "getNumActiveViews")]
    pub fn num_active_views(&self) -> i32 {
        self.active_views.len() as i32
    }

    /// The number of displays, active or not.
    ///
    /// Port of `Config::getNumDisplaysAll` (src/OpenColorIO/Config.cpp:4333-4336 @ v2.5.2).
    #[doc(alias = "getNumDisplaysAll")]
    pub fn num_displays_all(&self) -> i32 {
        self.displays.len() as i32
    }

    /// The display at `index` among all of them; `""` outside them.
    ///
    /// Port of `Config::getDisplayAll` (src/OpenColorIO/Config.cpp:4338-4346 @ v2.5.2).
    #[doc(alias = "getDisplayAll")]
    pub fn display_all(&self, index: i32) -> &[u8] {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.displays.get(i))
            .map_or(&[], |d| &d.0)
    }

    /// The index among all the displays of the display named `name` (spelled the same); -1 for
    /// none or an empty name.
    ///
    /// Port of `Config::getDisplayAllByName` (src/OpenColorIO/Config.cpp:4348-4364 @ v2.5.2).
    #[doc(alias = "getDisplayAllByName")]
    pub fn display_all_by_name(&self, name: impl AsRef<[u8]>) -> i32 {
        let name = c_str(name.as_ref());
        if name.is_empty() {
            return -1;
        }

        // strcmp: the names hold no NUL.
        self.displays
            .iter()
            .position(|d| d.0 == name)
            .map_or(-1, |idx| idx as i32)
    }

    /// Whether the display at `index` was made from the virtual display (and isn't saved).
    ///
    /// Port of `Config::isDisplayTemporary` (src/OpenColorIO/Config.cpp:4366-4374 @ v2.5.2).
    #[doc(alias = "isDisplayTemporary")]
    pub fn is_display_temporary(&self, index: i32) -> bool {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.displays.get(i))
            .is_some_and(|d| d.1.temporary)
    }

    /// Port of `Config::setDisplayTemporary` (src/OpenColorIO/Config.cpp:4376-4386 @ v2.5.2).
    #[doc(alias = "setDisplayTemporary")]
    pub fn set_display_temporary(&mut self, index: i32, is_temporary: bool) {
        if let Some(i) = usize::try_from(index)
            .ok()
            .filter(|&i| i < self.displays.len())
        {
            self.displays[i].1.temporary = is_temporary;

            self.clear_display_cache();
            self.reset_cache_ids();
        }
    }

    // Named transforms ////////////////////////////////////////////////////////////////////////

    /// The number of named transforms of `visibility`.
    ///
    /// Port of `Config::getNumNamedTransforms(NamedTransformVisibility)`
    /// (src/OpenColorIO/Config.cpp:3065-3089 @ v2.5.2).
    #[doc(alias = "getNumNamedTransforms")]
    pub fn num_named_transforms_with(&self, visibility: NamedTransformVisibility) -> i32 {
        match visibility {
            NamedTransformVisibility::All => self.all_named_transforms.len() as i32,
            NamedTransformVisibility::Active => self.active_named_transform_names.len() as i32,
            NamedTransformVisibility::Inactive => self.inactive_named_transform_names.len() as i32,
        }
    }

    /// The name of the named transform at `index` among those of `visibility`; `""` outside
    /// them.
    ///
    /// Port of `Config::getNamedTransformNameByIndex(NamedTransformVisibility, int)`
    /// (src/OpenColorIO/Config.cpp:3091-3128 @ v2.5.2).
    #[doc(alias = "getNamedTransformNameByIndex")]
    pub fn named_transform_name_by_index_with(
        &self,
        visibility: NamedTransformVisibility,
        index: i32,
    ) -> &[u8] {
        let Ok(index) = usize::try_from(index) else {
            return &[];
        };

        match visibility {
            NamedTransformVisibility::All => self
                .all_named_transforms
                .get(index)
                .map_or(&[], NamedTransform::name),
            NamedTransformVisibility::Active => self
                .active_named_transform_names
                .get(index)
                .map_or(&[], Vec::as_slice),
            NamedTransformVisibility::Inactive => self
                .inactive_named_transform_names
                .get(index)
                .map_or(&[], Vec::as_slice),
        }
    }

    /// The named transform whose name or alias is `name`, ignoring case, active or not.
    ///
    /// Port of `Config::getNamedTransform` (src/OpenColorIO/Config.cpp:3130-3134 @ v2.5.2).
    #[doc(alias = "getNamedTransform")]
    pub fn named_transform(&self, name: impl AsRef<[u8]>) -> Option<&NamedTransform> {
        // Use all named transforms.
        self.impl_named_transform(name.as_ref())
    }

    /// The number of active named transforms.
    ///
    /// Port of `Config::getNumNamedTransforms()` (src/OpenColorIO/Config.cpp:3136-3139 @
    /// v2.5.2).
    #[doc(alias = "getNumNamedTransforms")]
    pub fn num_named_transforms(&self) -> i32 {
        self.num_named_transforms_with(NamedTransformVisibility::Active)
    }

    /// The name of the active named transform at `index`; `""` outside them.
    ///
    /// Port of `Config::getNamedTransformNameByIndex(int)` (src/OpenColorIO/Config.cpp:
    /// 3141-3144 @ v2.5.2).
    #[doc(alias = "getNamedTransformNameByIndex")]
    pub fn named_transform_name_by_index(&self, index: i32) -> &[u8] {
        self.named_transform_name_by_index_with(NamedTransformVisibility::Active, index)
    }

    /// The index among the active named transforms of the named transform `name` (by name or
    /// alias); -1 for none or an inactive one.
    ///
    /// Port of `Config::getIndexForNamedTransform` (src/OpenColorIO/Config.cpp:3146-3167 @
    /// v2.5.2).
    #[doc(alias = "getIndexForNamedTransform")]
    pub fn index_for_named_transform(&self, name: impl AsRef<[u8]>) -> i32 {
        let Some(nt) = self.named_transform(name) else {
            return -1;
        };

        // Check to see if the name is an active named transform.
        let num = self.num_named_transforms_with(NamedTransformVisibility::Active);
        for idx in 0..num {
            // strcmp: names hold no NUL.
            if self.named_transform_name_by_index_with(NamedTransformVisibility::Active, idx)
                == nt.name()
            {
                return idx;
            }
        }

        // Requests for an inactive named transform or an inactive color space will both fail.
        -1
    }

    /// Adds a copy of `nt`, or replaces the named transform of the same name. Refuses a named
    /// transform without a name or a transform, one whose name or an alias is a role, a color
    /// space or holds `$` or `%`, and one that conflicts with another named transform's name
    /// or aliases.
    ///
    /// Port of `Config::addNamedTransform` (src/OpenColorIO/Config.cpp:3169-3298 @ v2.5.2).
    /// Its error for a null named transform can't happen with a reference.
    #[doc(alias = "addNamedTransform")]
    pub fn add_named_transform(&mut self, nt: &NamedTransform) -> Result<()> {
        let name = nt.name();
        if name.is_empty() {
            return Err(Exception::new(
                "Named transform must have a non-empty name.",
            ));
        }
        if nt.transform(TransformDirection::Forward).is_none()
            && nt.transform(TransformDirection::Inverse).is_none()
        {
            return Err(Exception::new(
                "Named transform must define at least one transform.",
            ));
        }

        if self.has_role(name) {
            return Err(Exception::new(
                [
                    b"Cannot add '".as_slice(),
                    name,
                    b"' named transform, there is already a role with this name.",
                ]
                .concat(),
            ));
        }
        if let Some(cs) = self.color_space(name) {
            return Err(Exception::new(
                [
                    b"Cannot add '".as_slice(),
                    name,
                    b"' named transform, there is already a color space using this name as a \
                      name or as an alias: '",
                    cs.name(),
                    b"'.",
                ]
                .concat(),
            ));
        }

        if contains_context_variable_token(name) {
            return Err(Exception::new(
                [
                    b"A named transform name '".as_slice(),
                    name,
                    b"' cannot contain a context variable reserved token i.e. % or $.",
                ]
                .concat(),
            ));
        }

        let mut existing = self.impl_named_transform_index(name);

        let mut replace_idx = usize::MAX;
        let num_nt = self.all_named_transforms.len();
        if existing < num_nt {
            let existing_name = self.all_named_transforms[existing].name();
            if !compare(existing_name, name) {
                return Err(Exception::new(
                    [
                        b"Cannot add '".as_slice(),
                        name,
                        b"' named transform, existing named transform, '",
                        existing_name,
                        b"' is using this name as an alias.",
                    ]
                    .concat(),
                ));
            }
            // There is a named transform with the same name that will be replaced (if new
            // named transform can be used).
            replace_idx = existing;
        }

        let num_aliases = nt.num_aliases();
        for aidx in 0..num_aliases {
            let alias = nt.alias(aidx);

            if self.has_role(alias) {
                return Err(Exception::new(
                    [
                        b"Cannot add '".as_slice(),
                        name,
                        b"' named transform, it has an alias '",
                        alias,
                        b"' and there is already a role with this name.",
                    ]
                    .concat(),
                ));
            }
            if let Some(cs) = self.color_space(alias) {
                return Err(Exception::new(
                    [
                        b"Cannot add '".as_slice(),
                        name,
                        b"' named transform, it has an alias '",
                        alias,
                        b"' and there is already a color space using this name as a name or as \
                          an alias: '",
                        cs.name(),
                        b"'.",
                    ]
                    .concat(),
                ));
            }
            if contains_context_variable_token(alias) {
                return Err(Exception::new(
                    [
                        b"Cannot add '".as_slice(),
                        name,
                        b"' named transform, it has an alias '",
                        alias,
                        b"' that cannot contain a context variable reserved token i.e. % or $.",
                    ]
                    .concat(),
                ));
            }

            existing = self.impl_named_transform_index(alias);
            // Is an alias of the named transform already used by a named transform?
            // Skip existing named transform that might be replaced.
            if existing != replace_idx && existing < num_nt {
                let existing_name = self.all_named_transforms[existing].name();
                return Err(Exception::new(
                    [
                        b"Cannot add '".as_slice(),
                        name,
                        b"' named transform, it has '",
                        alias,
                        b"' alias and existing named transform, '",
                        existing_name,
                        b"' is using the same alias.",
                    ]
                    .concat(),
                ));
            }
        }

        if replace_idx < num_nt {
            let existing_name = self.all_named_transforms[replace_idx].name();
            if !compare(existing_name, name) {
                return Err(Exception::new(
                    [
                        b"Cannot add '".as_slice(),
                        name,
                        b"' named transform, existing named transform, '",
                        existing_name,
                        b"' is using this name as an alias.",
                    ]
                    .concat(),
                ));
            }
            self.all_named_transforms[replace_idx] = nt.clone();
        } else {
            self.all_named_transforms.push(nt.clone());
        }

        self.reset_cache_ids();
        self.refresh_active_color_spaces();
        Ok(())
    }

    /// Removes the named transform whose name (not an alias) is `name`, ignoring case. It keeps
    /// the cache IDs and the lists of active and inactive named transforms, which then still
    /// name it (docs/improvements.md, I-132).
    ///
    /// Port of `Config::removeNamedTransform` (src/OpenColorIO/Config.cpp:3300-3317 @ v2.5.2).
    #[doc(alias = "removeNamedTransform")]
    pub fn remove_named_transform(&mut self, name: impl AsRef<[u8]>) {
        let name_to_search = lower(c_str(name.as_ref()));
        if name_to_search.is_empty() {
            return;
        }

        if let Some(itr) = self
            .all_named_transforms
            .iter()
            .position(|nt| lower(nt.name()) == name_to_search)
        {
            self.all_named_transforms.remove(itr);
            return;
        }

        self.reset_cache_ids();
        self.refresh_active_color_spaces();
    }

    /// Removes every named transform.
    ///
    /// Port of `Config::clearNamedTransforms` (src/OpenColorIO/Config.cpp:3319-3325 @ v2.5.2).
    #[doc(alias = "clearNamedTransforms")]
    pub fn clear_named_transforms(&mut self) {
        self.all_named_transforms.clear();

        self.reset_cache_ids();
        self.refresh_active_color_spaces();
    }

    // Looks ///////////////////////////////////////////////////////////////////////////////////

    /// The look named `name`, ignoring case.
    ///
    /// Port of `Config::getLook` and `Config::Impl::getLook` (src/OpenColorIO/Config.cpp:
    /// 543-554, 4473-4476 @ v2.5.2).
    #[doc(alias = "getLook")]
    pub fn look(&self, name: impl AsRef<[u8]>) -> Option<&Look> {
        let namelower = lower(c_str(name.as_ref()));

        self.looks_list
            .iter()
            .find(|look| lower(look.name()) == namelower)
    }

    /// Port of `Config::getNumLooks` (src/OpenColorIO/Config.cpp:4478-4481 @ v2.5.2).
    #[doc(alias = "getNumLooks")]
    pub fn num_looks(&self) -> i32 {
        self.looks_list.len() as i32
    }

    /// The name of the look at `index`; `""` outside them.
    ///
    /// Port of `Config::getLookNameByIndex` (src/OpenColorIO/Config.cpp:4483-4491 @ v2.5.2).
    #[doc(alias = "getLookNameByIndex")]
    pub fn look_name_by_index(&self, index: i32) -> &[u8] {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.looks_list.get(i))
            .map_or(&[], Look::name)
    }

    /// Adds a copy of `look`, or replaces the look of the same name (ignoring case).
    ///
    /// Port of `Config::addLook` (src/OpenColorIO/Config.cpp:4493-4520 @ v2.5.2).
    #[doc(alias = "addLook")]
    pub fn add_look(&mut self, look: &Look) -> Result<()> {
        let name = look.name();
        if name.is_empty() {
            return Err(Exception::new("Cannot addLook with an empty name."));
        }

        let namelower = lower(name);

        // If the look exists, replace it
        match self
            .looks_list
            .iter()
            .position(|l| lower(l.name()) == namelower)
        {
            Some(i) => self.looks_list[i] = look.clone(),
            // Otherwise, add it
            None => self.looks_list.push(look.clone()),
        }

        self.reset_cache_ids();
        Ok(())
    }

    /// Port of `Config::clearLooks` (src/OpenColorIO/Config.cpp:4522-4528 @ v2.5.2).
    #[doc(alias = "clearLooks")]
    pub fn clear_looks(&mut self) {
        self.looks_list.clear();

        self.reset_cache_ids();
    }

    // View transforms /////////////////////////////////////////////////////////////////////////

    /// Port of `Config::getNumViewTransforms` (src/OpenColorIO/Config.cpp:4532-4535 @ v2.5.2).
    #[doc(alias = "getNumViewTransforms")]
    pub fn num_view_transforms(&self) -> i32 {
        self.view_transforms.len() as i32
    }

    /// The view transform named `name`, ignoring case.
    ///
    /// Port of `Config::getViewTransform` and `Config::Impl::getViewTransform`
    /// (src/OpenColorIO/Config.cpp:529-541, 4537-4540 @ v2.5.2).
    #[doc(alias = "getViewTransform")]
    pub fn view_transform(&self, name: impl AsRef<[u8]>) -> Option<&ViewTransform> {
        let namelower = lower(c_str(name.as_ref()));

        self.view_transforms
            .iter()
            .find(|vt| lower(vt.name()) == namelower)
    }

    /// The name of the view transform at `index`; `""` outside them.
    ///
    /// Port of `Config::getViewTransformNameByIndex` (src/OpenColorIO/Config.cpp:4542-4550 @
    /// v2.5.2).
    #[doc(alias = "getViewTransformNameByIndex")]
    pub fn view_transform_name_by_index(&self, index: i32) -> &[u8] {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.view_transforms.get(i))
            .map_or(&[], ViewTransform::name)
    }

    /// The default view transform from the scene reference space to the display one: the
    /// default view transform when it is a scene-referred one, else the first scene-referred
    /// view transform.
    ///
    /// Port of `Config::getDefaultSceneToDisplayViewTransform` (src/OpenColorIO/Config.cpp:
    /// 4552-4577 @ v2.5.2).
    #[doc(alias = "getDefaultSceneToDisplayViewTransform")]
    pub fn default_scene_to_display_view_transform(&self) -> Option<&ViewTransform> {
        // The default view transform between the main reference space (scene-referred) and the
        // display-referred space if it is not defined, it is the first one in the list that
        // uses a scene-referred reference space.

        if !self.default_view_transform.is_empty()
            && let Some(vt) = self.view_transform(&self.default_view_transform)
            && vt.reference_space_type() == ReferenceSpaceType::Scene
        {
            return Some(vt);
        }
        self.view_transforms
            .iter()
            .find(|vt| vt.reference_space_type() == ReferenceSpaceType::Scene)
    }

    /// Port of `Config::getDefaultViewTransformName` (src/OpenColorIO/Config.cpp:4579-4582 @
    /// v2.5.2).
    #[doc(alias = "getDefaultViewTransformName")]
    pub fn default_view_transform_name(&self) -> &[u8] {
        &self.default_view_transform
    }

    /// Port of `Config::setDefaultViewTransformName` (src/OpenColorIO/Config.cpp:4584-4590 @
    /// v2.5.2).
    #[doc(alias = "setDefaultViewTransformName")]
    pub fn set_default_view_transform_name(&mut self, default_vt: impl AsRef<[u8]>) {
        self.default_view_transform = c_str(default_vt.as_ref()).to_vec();

        self.reset_cache_ids();
    }

    /// Adds a copy of `view_transform`, or replaces the view transform of the same name
    /// (ignoring case). Refuses one without a name or a transform.
    ///
    /// Port of `Config::addViewTransform` (src/OpenColorIO/Config.cpp:4592-4631 @ v2.5.2).
    #[doc(alias = "addViewTransform")]
    pub fn add_view_transform(&mut self, view_transform: &ViewTransform) -> Result<()> {
        let name = view_transform.name();
        if name.is_empty() {
            return Err(Exception::new(
                "Cannot add view transform with an empty name.",
            ));
        }

        if view_transform
            .transform(ViewTransformDirection::ToReference)
            .is_none()
            && view_transform
                .transform(ViewTransformDirection::FromReference)
                .is_none()
        {
            return Err(Exception::new(
                [
                    b"Cannot add view transform '".as_slice(),
                    name,
                    b"' with no transform.",
                ]
                .concat(),
            ));
        }

        let namelower = lower(name);

        // If the view transform exists, replace it.
        match self
            .view_transforms
            .iter()
            .position(|vt| lower(vt.name()) == namelower)
        {
            Some(i) => self.view_transforms[i] = view_transform.clone(),
            // Otherwise, add it.
            None => self.view_transforms.push(view_transform.clone()),
        }

        self.reset_cache_ids();
        Ok(())
    }

    /// Port of `Config::clearViewTransforms` (src/OpenColorIO/Config.cpp:4633-4639 @ v2.5.2).
    #[doc(alias = "clearViewTransforms")]
    pub fn clear_view_transforms(&mut self) {
        self.view_transforms.clear();

        self.reset_cache_ids();
    }

    // File rules //////////////////////////////////////////////////////////////////////////////

    /// The config's file rules, shared with the config (see [`ConfigRules`]).
    ///
    /// Port of `Config::getFileRules` (src/OpenColorIO/Config.cpp:4643-4646 @ v2.5.2).
    #[doc(alias = "getFileRules")]
    pub fn file_rules(&self) -> ConfigRules<FileRules> {
        self.file_rules.clone()
    }

    /// Sets the config's file rules to a copy of `file_rules`. Handles taken from
    /// [`Config::file_rules`] before keep the rules they had.
    ///
    /// Port of `Config::setFileRules` (src/OpenColorIO/Config.cpp:4648-4654 @ v2.5.2).
    #[doc(alias = "setFileRules")]
    pub fn set_file_rules(&mut self, file_rules: &FileRules) {
        self.file_rules = ConfigRules::new(file_rules.clone());

        self.reset_cache_ids();
    }

    /// The color space of the first file rule that matches `file_path`. Owned: the path search
    /// rule's color space changes as it matches.
    ///
    /// Port of `Config::getColorSpaceFromFilepath(const char *)` (src/OpenColorIO/Config.cpp:
    /// 4656-4660 @ v2.5.2).
    #[doc(alias = "getColorSpaceFromFilepath")]
    pub fn color_space_from_filepath(&self, file_path: impl AsRef<[u8]>) -> Result<Vec<u8>> {
        Ok(self
            .file_rules
            .get()
            .color_space_from_filepath(self, c_str(file_path.as_ref()))?
            .0)
    }

    /// The color space of the first file rule that matches `file_path`, and that rule's index.
    ///
    /// Port of `Config::getColorSpaceFromFilepath(const char *, size_t &)`
    /// (src/OpenColorIO/Config.cpp:4662-4667 @ v2.5.2).
    #[doc(alias = "getColorSpaceFromFilepath")]
    pub fn color_space_from_filepath_with_index(
        &self,
        file_path: impl AsRef<[u8]>,
    ) -> Result<(Vec<u8>, usize)> {
        self.file_rules
            .get()
            .color_space_from_filepath(self, c_str(file_path.as_ref()))
    }

    /// Whether only the default file rule matches `file_path`.
    ///
    /// Port of `Config::filepathOnlyMatchesDefaultRule` (src/OpenColorIO/Config.cpp:4669-4673 @
    /// v2.5.2).
    #[doc(alias = "filepathOnlyMatchesDefaultRule")]
    pub fn filepath_only_matches_default_rule(&self, file_path: impl AsRef<[u8]>) -> Result<bool> {
        self.file_rules
            .get()
            .filepath_only_matches_default_rule(self, c_str(file_path.as_ref()))
    }

    /// The color space whose name (or alias) ends rightmost in `str`; without one, unless
    /// parsing is strict, the color space of the `default` role; else `""`.
    ///
    /// Port of `Config::parseColorSpaceFromString` (src/OpenColorIO/Config.cpp:2936-2962 @
    /// v2.5.2).
    #[doc(alias = "parseColorSpaceFromString")]
    pub fn parse_color_space_from_string(&self, str: impl AsRef<[u8]>) -> &[u8] {
        let right_most_color_space_index = parse_color_space_from_string(self, str.as_ref());

        // Index is using all color spaces.
        if right_most_color_space_index >= 0 {
            return self
                .all_color_spaces
                .color_space_name_by_index(right_most_color_space_index)
                .unwrap_or(&[]);
        }

        if !self.strict_parsing {
            // Is a default role defined?
            let csname = lookup_role(&self.roles, ROLE_DEFAULT.as_bytes());
            if !csname.is_empty() {
                let csindex = self.all_color_spaces.color_space_index(csname);
                if -1 != csindex {
                    return self
                        .all_color_spaces
                        .color_space_name_by_index(csindex)
                        .unwrap_or(&[]);
                }
            }
        }

        &[]
    }

    /// Upgrades a version 1 config to the latest version (2.5): its file rules get the path
    /// search rule and a default rule (`UpdateFileRulesFromV1ToV2`). A config without a color
    /// space for that default rule is an error, where upstream's `noexcept` function ends the
    /// program (docs/improvements.md, U-55); the config is then left as it was.
    ///
    /// Port of `Config::upgradeToLatestVersion` (src/OpenColorIO/Config.cpp:1332-1350 @
    /// v2.5.2).
    #[doc(alias = "upgradeToLatestVersion")]
    pub fn upgrade_to_latest_version(&mut self) -> Result<()> {
        let was_version = self.major_version;
        if was_version != LAST_SUPPORTED_MAJOR_VERSION {
            if was_version == 1 {
                // Upstream updates the shared rules in place: the callers that hold them see the
                // update.
                let mut file_rules = (*self.file_rules.get()).clone();
                update_file_rules_from_v1_to_v2(self, &mut file_rules)?;
                self.file_rules.update(|rules| *rules = file_rules);

                // The instance version is now 2.0
                self.major_version = 2;
                self.minor_version = 0;
            }

            const _: () = assert!(
                LAST_SUPPORTED_MAJOR_VERSION == 2,
                "Config: Handle newer versions"
            );
            self.set_major_version(LAST_SUPPORTED_MAJOR_VERSION)?;
            self.set_minor_version(
                LAST_SUPPORTED_MINOR_VERSION[LAST_SUPPORTED_MAJOR_VERSION as usize - 1],
            )?;
        }
        Ok(())
    }

    /// The transforms of the color spaces, looks, view transforms and named transforms, both
    /// directions each, in that order.
    ///
    /// Port of `Config::Impl::getAllInternalTransforms` (src/OpenColorIO/Config.cpp:5476-5542
    /// @ v2.5.2).
    fn all_internal_transforms(&self) -> Vec<&Transform> {
        let mut transform_vec = Vec::new();

        // Grab all transforms from the ColorSpaces.

        for i in 0..self.all_color_spaces.num_color_spaces() {
            let cs = self
                .all_color_spaces
                .color_space_by_index(i)
                .expect("an index of the set");
            if let Some(tr) = cs.transform(ColorSpaceDirection::ToReference) {
                transform_vec.push(tr);
            }

            if let Some(tr) = cs.transform(ColorSpaceDirection::FromReference) {
                transform_vec.push(tr);
            }
        }

        // Grab all transforms from the Looks.

        for look in &self.looks_list {
            if let Some(tr) = look.transform() {
                transform_vec.push(tr);
            }

            if let Some(tr) = look.inverse_transform() {
                transform_vec.push(tr);
            }
        }

        // Grab all transforms from the view transforms.

        for vt in &self.view_transforms {
            if let Some(tr) = vt.transform(ViewTransformDirection::ToReference) {
                transform_vec.push(tr);
            }

            if let Some(tr) = vt.transform(ViewTransformDirection::FromReference) {
                transform_vec.push(tr);
            }
        }

        // Grab all transforms from the named transforms.

        for nt in &self.all_named_transforms {
            if let Some(tr) = nt.transform(TransformDirection::Forward) {
                transform_vec.push(tr);
            }

            if let Some(tr) = nt.transform(TransformDirection::Inverse) {
                transform_vec.push(tr);
            }
        }

        transform_vec
    }

    /// Refuses a transform the config's version can't have: in version 1, the classes and
    /// styles version 2 brought; in versions 2.0 to 2.4, the built-in and fixed function
    /// styles of later minor versions. A group's children are checked the same way.
    ///
    /// Port of `Config::Impl::checkVersionConsistency(ConstTransformRcPtr&)`
    /// (src/OpenColorIO/Config.cpp:5586-5832 @ v2.5.2). Its ExposureContrast and grading arms
    /// come with those classes (Phase 5); until then their tags can't be loaded. Two messages
    /// keep upstream's misspellings (docs/improvements.md, I-146).
    fn check_transform_version_consistency(&self, transform: &Transform) -> Result<()> {
        let major = self.major_version;
        let minor = self.minor_version;
        match transform {
            Transform::Builtin(blt) => {
                if major < 2 {
                    return Err(Exception::new(
                        "Only config version 2 (or higher) can have BuiltinInTransform.",
                    ));
                }

                let style = blt.style();
                let is = |name: &str| strcasecmp(style, name).is_eq();
                if major == 2 && minor < 1 && is("ACES-LMT - ACES 1.3 Reference Gamut Compression")
                {
                    return Err(Exception::new(
                        "Only config version 2.1 (or higher) can have BuiltinTransform style \
                         'ACES-LMT - ACES 1.3 Reference Gamut Compression'.",
                    ));
                }
                if major == 2 && minor < 2 && BUILTIN_STYLES_2_2.iter().any(|s| is(s)) {
                    let mut os = b"Only config version 2.2 (or higher) can have BuiltinTransform \
                                   style '"
                        .to_vec();
                    os.extend_from_slice(style);
                    os.extend_from_slice(b"'.");
                    return Err(Exception::new(os));
                }
                if major == 2 && minor < 3 && is("DISPLAY - CIE-XYZ-D65_to_DisplayP3") {
                    return Err(Exception::new(
                        "Only config version 2.3 (or higher) can have BuiltinTransform style \
                         'DISPLAY - CIE-XYZ-D65_to_DisplayP3'.",
                    ));
                }
                if major == 2 && minor < 4 && BUILTIN_STYLES_2_4.iter().any(|s| is(s)) {
                    let mut os = b"Only config version 2.4 (or higher) can have BuiltinTransform \
                                   style '"
                        .to_vec();
                    os.extend_from_slice(style);
                    os.extend_from_slice(b"'.");
                    return Err(Exception::new(os));
                }
            }
            Transform::Cdl(cdl) => {
                if major < 2 && cdl.style() != CdlStyle::TRANSFORM_DEFAULT {
                    return Err(Exception::new(
                        "Only config version 2 (or higher) can have style for CDLTransform.",
                    ));
                }
            }
            Transform::DisplayView(_) => {
                if major < 2 {
                    return Err(Exception::new(
                        "Only config version 2 (or higher) can have DisplayViewTransform.",
                    ));
                }
            }
            Transform::Exponent(ex) => {
                if major < 2 && ex.negative_style() != NegativeStyle::Clamp {
                    return Err(Exception::new(
                        "Config version 1 only supports ExponentTransform clamping negative \
                         values.",
                    ));
                }
            }
            Transform::ExponentWithLinear(_) => {
                if major < 2 {
                    return Err(Exception::new(
                        "Only config version 2 (or higher) can have ExponentWithLinearTransform.",
                    ));
                }
            }
            Transform::File(ft) => {
                if major < 2 {
                    if ft.interpolation() == Interpolation::Cubic {
                        return Err(Exception::new(
                            "Only config version 2 (or higher) can use 'cubic' interpolation \
                             with FileTransform.",
                        ));
                    }
                    if ft.cdl_style() != CdlStyle::TRANSFORM_DEFAULT {
                        return Err(Exception::new(
                            "Only config version 2 (or higher) can use CDL style' for \
                             FileTransform.",
                        ));
                    }
                }
            }
            Transform::FixedFunction(ff) => {
                use FixedFunctionStyle::*;
                let ffstyle = ff.style();
                if major < 2 {
                    return Err(Exception::new(
                        "Only config version 2 (or higher) can have FixedFunctionTransform.",
                    ));
                }

                if major == 2 && minor < 1 && ffstyle == AcesGamutComp13 {
                    return Err(Exception::new(
                        "Only config version 2.1 (or higher) can have FixedFunctionTransform \
                         style 'ACES_GAMUT_COMP_13'.",
                    ));
                }

                if major == 2
                    && minor < 4
                    && matches!(
                        ffstyle,
                        LinToPq
                            | LinToGammaLog
                            | LinToDoubleLog
                            | AcesOutputTransform20
                            | AcesRgbToJmh20
                            | AcesTonescaleCompress20
                            | AcesGamutCompress20
                    )
                {
                    return Err(Exception::new(format!(
                        "Only config version 2.4 (or higher) can have FixedFunctionTransform \
                         style '{}'.",
                        fixed_function_style_to_string(ffstyle)?
                    )));
                }

                if major == 2
                    && minor < 5
                    && matches!(ffstyle, RgbToHsyLin | RgbToHsyLog | RgbToHsyVid)
                {
                    return Err(Exception::new(format!(
                        "Only config version 2.5 (or higher) can have FixedFunctionTransform \
                         style '{}'.",
                        fixed_function_style_to_string(ffstyle)?
                    )));
                }
            }
            Transform::LogAffine(_) => {
                if major < 2 {
                    return Err(Exception::new(
                        "Only config version 2 (or higher) can have LogAffineTransform.",
                    ));
                }
            }
            Transform::LogCamera(_) => {
                if major < 2 {
                    return Err(Exception::new(
                        "Only config version 2 (or higher) can have LogCameraTransform.",
                    ));
                }
            }
            Transform::Range(_) => {
                if major < 2 {
                    return Err(Exception::new(
                        "Only config version 2 (or higher) can have RangeTransform.",
                    ));
                }
            }
            Transform::Group(grp) => {
                for idx in 0..grp.num_transforms() {
                    let tr = grp.transform(idx)?;
                    self.check_transform_version_consistency(tr)?;
                }
            }
            Transform::Allocation(_)
            | Transform::ColorSpace(_)
            | Transform::Log(_)
            | Transform::Look(_)
            | Transform::Lut1D(_)
            | Transform::Matrix(_) => {}
        }
        Ok(())
    }

    /// Refuses what the config's version can't have: the transforms
    /// ([`Config::check_transform_version_consistency`]); in version 1 a family separator, file
    /// rules, inactive color spaces, viewing rules, shared views, a virtual display, display
    /// color spaces, interop IDs, view transforms and named transforms; before 2.5 interchange
    /// attributes.
    ///
    /// Port of `Config::Impl::checkVersionConsistency()` (src/OpenColorIO/Config.cpp:5834-5993
    /// @ v2.5.2).
    pub(crate) fn check_version_consistency(&self) -> Result<()> {
        let hex_version: u32 = (self.major_version << 24) | (self.minor_version << 16);

        // Check for the Transforms.

        for transform in self.all_internal_transforms() {
            self.check_transform_version_consistency(transform)?;
        }

        // Check for the family separator.

        if self.major_version < 2 && self.family_separator != b'/' {
            return Err(Exception::new(
                "Only version 2 (or higher) can have a family separator.",
            ));
        }

        // Check for the file rules.

        if self.major_version < 2 && self.file_rules.get().num_entries() > 2 {
            return Err(Exception::new(
                "Only version 2 (or higher) can have file rules.",
            ));
        }

        // Check for inactive color spaces.

        if self.major_version < 2 && !self.inactive_color_space_names_conf.is_empty() {
            return Err(Exception::new(
                "Only version 2 (or higher) can have inactive color spaces.",
            ));
        }

        // Check for ViewingRules.

        if self.major_version < 2 && self.viewing_rules.get().num_entries() != 0 {
            return Err(Exception::new(
                "Only version 2 (or higher) can have viewing rules.",
            ));
        }

        // Check for shared views.

        if self.major_version < 2 {
            if !self.shared_views.is_empty() {
                return Err(Exception::new(
                    "Only version 2 (or higher) can have shared views.",
                ));
            }
            for (name, display) in &self.displays {
                if !display.shared_views.is_empty() {
                    let mut os = b"Config failed validation. The display '".to_vec();
                    os.extend_from_slice(name);
                    os.extend_from_slice(b"' uses shared views and config version is less than 2.");
                    return Err(Exception::new(os));
                }
            }
        }

        // Check for virtual display.

        if self.major_version < 2
            && (!self.virtual_display.views.is_empty()
                || !self.virtual_display.shared_views.is_empty())
        {
            return Err(Exception::new(
                "Only version 2 (or higher) can have a virtual display.",
            ));
        }

        // Check ColorSpace properties.

        for i in 0..self.all_color_spaces.num_color_spaces() {
            // Check for display color spaces.

            let cs = self
                .all_color_spaces
                .color_space_by_index(i)
                .expect("an index of the set");
            if self.major_version < 2
                && match_reference_type(
                    SearchReferenceSpaceType::Display,
                    cs.reference_space_type(),
                )
            {
                return Err(Exception::new(
                    "Only version 2 (or higher) can have DisplayColorSpaces.",
                ));
            }

            // Check for new color space attributes.

            if self.major_version < 2 && !c_str(cs.interop_id()).is_empty() {
                let mut os = b"Config failed validation. The color space '".to_vec();
                os.extend_from_slice(c_str(cs.name()));
                os.extend_from_slice(
                    b"' has non-empty InteropID and config version is less than 2.0.",
                );
                return Err(Exception::new(os));
            }

            if hex_version < 0x02050000 && !cs.interchange_attributes().is_empty() {
                let mut os = b"Config failed validation. The color space '".to_vec();
                os.extend_from_slice(c_str(cs.name()));
                os.extend_from_slice(
                    b"' has non-empty interchange attributes and config version is less than \
                      2.5.",
                );
                return Err(Exception::new(os));
            }
        }

        // Check for the ViewTransforms.

        if self.major_version < 2
            && (!self.view_transforms.is_empty() || !self.default_view_transform.is_empty())
        {
            return Err(Exception::new(
                "Only version 2 (or higher) can have ViewTransforms.",
            ));
        }

        // Check for new ViewTransform properties.

        if hex_version < 0x02050000 {
            for vt in &self.view_transforms {
                if !vt.interchange_attributes().is_empty() {
                    let mut os = b"Config failed validation. The view transform '".to_vec();
                    os.extend_from_slice(c_str(vt.name()));
                    os.extend_from_slice(
                        b"' has non-empty interchange attributes and config version is less \
                          than 2.5.",
                    );
                    return Err(Exception::new(os));
                }
            }
        }

        // Check for new Look properties.

        if hex_version < 0x02050000 {
            for look in &self.looks_list {
                if !look.interchange_attributes().is_empty() {
                    let mut os = b"Config failed validation. The look '".to_vec();
                    os.extend_from_slice(c_str(look.name()));
                    os.extend_from_slice(
                        b"' has non-empty interchange attributes and config version is less \
                          than 2.5.",
                    );
                    return Err(Exception::new(os));
                }
            }
        }

        // Check for the NamedTransforms.

        if self.major_version < 2 && !self.all_named_transforms.is_empty() {
            return Err(Exception::new(
                "Only version 2 (or higher) can have NamedTransforms.",
            ));
        }
        Ok(())
    }

    // Validation //////////////////////////////////////////////////////////////////////////////

    /// Checks the config: its predefined context variables, color spaces, roles (and the
    /// interchange roles version 2.2 asks for, which only log errors), inactive lists, viewing
    /// rules, displays and views, the virtual display, the active displays, the transforms and
    /// the color spaces they name, looks, view transforms, file rules, search paths and file
    /// transform paths, named transforms, and the version
    /// ([`Config::check_version_consistency`]). The result is kept until the config changes: a
    /// failed config fails again with the same message, which is empty where upstream throws
    /// without setting it (docs/improvements.md, I-147).
    ///
    /// Port of `Config::validate` (src/OpenColorIO/Config.cpp:1359-2106 @ v2.5.2).
    pub fn validate(&self) -> Result<()> {
        {
            let mut cache_ids = self.lock_cache_ids();
            match cache_ids.validation {
                Validation::Passed => return Ok(()),
                Validation::Failed => {
                    return Err(Exception::new(cache_ids.validation_text.clone()));
                }
                Validation::Unknown => {}
            }

            cache_ids.validation = Validation::Failed;
            cache_ids.validation_text.clear();
        }

        let mut validation_text = Vec::new();
        let result = self.validate_impl(&mut validation_text);
        let mut cache_ids = self.lock_cache_ids();
        cache_ids.validation_text = validation_text;
        if result.is_ok() {
            // Everything is groovy.
            cache_ids.validation = Validation::Passed;
        }
        result
    }

    /// The checks of [`Config::validate`]; `validation_text` is upstream's `m_validationtext`,
    /// which most failures set to their message before throwing it.
    fn validate_impl(&self, validation_text: &mut Vec<u8>) -> Result<()> {
        // Sets the validation text and gives its exception.
        let fail = |validation_text: &mut Vec<u8>, os: Vec<u8>| {
            *validation_text = os;
            Exception::new(validation_text.clone())
        };

        ///// PREDEFINED CONTEXT VARIABLES

        // Only the 'predefined' mode imposes to have all the context variables explicitely
        // defined in the config file. The 'all' mode exclusively relies on the environment
        // variables.
        if self.major_version() >= 2
            && self.shared_context.get().environment_mode() == EnvironmentMode::LoadPredefined
        {
            for (name, value) in &self.env {
                let ctx_value = value;

                if contains_context_variables(ctx_value) {
                    // When a context variable default value contains another context variable,
                    // the only legal case is ENV = $ENV. It means that there is no default
                    // value i.e. an system env. variable must exist.

                    let ctx_variable1 = [b"$".as_slice(), name].concat();
                    let ctx_variable2 = [b"${".as_slice(), name, b"}"].concat();
                    let ctx_variable3 = [b"%".as_slice(), name, b"%"].concat();

                    let is_valid = &ctx_variable1 == ctx_value
                        || &ctx_variable2 == ctx_value
                        || &ctx_variable3 == ctx_value;

                    if !is_valid {
                        let mut oss =
                            b"Unresolved context variable in environment declaration \x27".to_vec();
                        oss.extend_from_slice(name);
                        oss.extend_from_slice(b" = ");
                        oss.extend_from_slice(value);
                        oss.extend_from_slice(b"\x27.");
                        return Err(Exception::new(oss));
                    }
                }
            }
        }

        ///// COLORSPACES

        let mut has_display_referred_colorspace = false;
        let mut has_scene_referred_colorspace = false;

        // Confirm all ColorSpaces are valid.
        for i in 0..self.all_color_spaces.num_color_spaces() {
            let cs = self
                .all_color_spaces
                .color_space_by_index(i)
                .expect("an index of the set");

            let name = c_str(cs.name());
            // Name is not empty and unique (checked by addColorSpace ).

            // Retest that name does not contain reserved characters (vesion might have change).
            if self.major_version() >= 2 && contains_context_variable_token(name) {
                let mut oss =
                    b"Config failed color space validation. A color space name \x27".to_vec();
                oss.extend_from_slice(name);
                oss.extend_from_slice(
                    b"\x27 cannot contain a context variable reserved token i.e. % or $.",
                );
                return Err(fail(validation_text, oss));
            }

            let num_aliases = cs.num_aliases();
            if num_aliases != 0 && self.major_version() < 2 {
                let mut oss = b"Config failed color space validation. Aliases may not be used \
                                in a v1 config.  Color space name: \x27"
                    .to_vec();
                oss.extend_from_slice(name);
                oss.extend_from_slice(b"\x27.");
                return Err(fail(validation_text, oss));
            }

            // Make sure that all used interopIDs are available in this config.
            let interop = c_str(cs.interop_id());
            if !interop.is_empty() && self.color_space(interop).is_none() {
                let mut os = b"Config failed color space validation. The color space \x27".to_vec();
                os.extend_from_slice(name);
                os.extend_from_slice(b"\x27 refers to an interop ID, \x27");
                os.extend_from_slice(interop);
                os.extend_from_slice(b"\x27, which is not a color space name or alias.");
                return Err(fail(validation_text, os));
            }

            // AddColorSpace, addNamedTransform & setRole already check there is no name & alias
            // conflict.

            if cs.reference_space_type() == ReferenceSpaceType::Display {
                has_display_referred_colorspace = true;
            } else if cs.reference_space_type() == ReferenceSpaceType::Scene {
                has_scene_referred_colorspace = true;
            }
        }

        // Confirm all roles used by the config are valid and that essential roles are present.
        {
            for (role, colorspace) in &self.roles {
                // Retest in case version did change.
                if self.major_version() >= 2 && contains_context_variable_token(role) {
                    let mut oss = b"Config failed role validation. A role name \x27".to_vec();
                    oss.extend_from_slice(role);
                    oss.extend_from_slice(
                        b"\x27 cannot contain a context variable reserved token i.e. % or $.",
                    );
                    return Err(fail(validation_text, oss));
                }

                if !self.has_color_space(c_str(colorspace)) {
                    let mut os = b"Config failed role validation. The role \x27".to_vec();
                    os.extend_from_slice(role);
                    os.extend_from_slice(b"\x27 refers to a color space, \x27");
                    os.extend_from_slice(colorspace);
                    os.extend_from_slice(b"\x27, which is not defined.");
                    return Err(fail(validation_text, os));
                }

                // AddColorSpace, addNamedTransform & setRole already check there is no name
                // conflict.
            }

            // Check for interchange roles requirements - scene-referred and display-referred.
            let version_hex: u32 = (self.major_version() << 24) | (self.minor_version() << 16);
            if version_hex >= 0x02020000 {
                // v2.2 or higher
                let mut has_role_scene_linear = false;
                let mut has_role_compositing_log = false;
                let mut has_role_color_timing = false;

                let mut has_role_aces_interchange = false;
                let mut aces_inter_has_scene_ref_colorspace = false;
                let mut has_role_cie_xyz_d65_interchange = false;
                let mut cie_inter_has_display_ref_colorspace = false;

                for (role, colorspace) in &self.roles {
                    let role = c_str(role);
                    if strcasecmp(role, ROLE_SCENE_LINEAR).is_eq() {
                        has_role_scene_linear = true;
                    } else if strcasecmp(role, ROLE_COMPOSITING_LOG).is_eq() {
                        has_role_compositing_log = true;
                    } else if strcasecmp(role, ROLE_COLOR_TIMING).is_eq() {
                        has_role_color_timing = true;
                    } else if strcasecmp(role, ROLE_INTERCHANGE_SCENE).is_eq() {
                        has_role_aces_interchange = true;

                        let cs = self
                            .color_space(c_str(colorspace))
                            .expect("a role\x27s color space, checked above");
                        aces_inter_has_scene_ref_colorspace =
                            cs.reference_space_type() == ReferenceSpaceType::Scene;
                    } else if strcasecmp(role, ROLE_INTERCHANGE_DISPLAY).is_eq() {
                        has_role_cie_xyz_d65_interchange = true;

                        let cs = self
                            .color_space(c_str(colorspace))
                            .expect("a role\x27s color space, checked above");
                        cie_inter_has_display_ref_colorspace =
                            cs.reference_space_type() == ReferenceSpaceType::Display;
                    }
                }

                // All LogError below are technically a validation failure, but only logging a
                // message rather than throwing (for now). This is to make it possible for
                // upgradeToLatestVersion to always result in a config that does not fail
                // validation.

                if !has_role_scene_linear {
                    log_error(
                        "The scene_linear role is required for a config version 2.2 or higher.",
                    );
                }

                if !has_role_compositing_log {
                    log_error(
                        "The compositing_log role is required for a config version 2.2 or higher.",
                    );
                }

                if !has_role_color_timing {
                    log_error(
                        "The color_timing role is required for a config version 2.2 or higher.",
                    );
                }

                if has_scene_referred_colorspace && !has_role_aces_interchange {
                    log_error(
                        "The aces_interchange role is required when there are scene-referred \
                         color spaces and the config version is 2.2 or higher.",
                    );
                } else if has_role_aces_interchange && !aces_inter_has_scene_ref_colorspace {
                    log_error("The aces_interchange role must be a scene-referred color space.");
                }

                if has_display_referred_colorspace && !has_role_cie_xyz_d65_interchange {
                    log_error(
                        "The cie_xyz_d65_interchange role is required when there are \
                         display-referred color spaces and the config version is 2.2 or higher.",
                    );
                } else if has_role_cie_xyz_d65_interchange && !cie_inter_has_display_ref_colorspace
                {
                    log_error(
                        "The cie_xyz_d65_interchange role must be a display-referred color space.",
                    );
                }
            }
        }

        // Confirm all inactive color spaces or named transforms exist.
        let inactive_color_space_names = self.build_inactive_names_list(InactiveType::All);

        for name in &inactive_color_space_names {
            if self.impl_color_space(name).is_none() && self.impl_named_transform(name).is_none() {
                let mut os = b"Inactive \x27".to_vec();
                os.extend_from_slice(name);
                os.extend_from_slice(b"\x27 is neither a color space nor a named transform.");
                log_info(os);
            }
        }

        ///// DISPLAYS / VIEWS

        // Viewing rules.

        let color_space_accessor = |name: &[u8]| self.color_space(name).is_some();
        if let Err(e) = self
            .viewing_rules
            .get()
            .validate(&color_space_accessor, &self.all_color_spaces)
        {
            let mut os =
                b"Config failed validation. Viewing rules failed validation with: ".to_vec();
            os.extend_from_slice(e.what());
            return Err(fail(validation_text, os));
        }

        // Shared views.
        for view in &self.shared_views {
            self.validate_view(b"", view, true, validation_text)?;
        }

        let mut numdisplays = 0;

        // Confirm all Display transforms refer to colorspaces that exist.
        for (display, d) in &self.displays {
            let views = &d.views;
            let shared_views = &d.shared_views;
            if views.is_empty() && shared_views.is_empty() {
                let mut os = b"Config failed display validation. The display \x27".to_vec();
                os.extend_from_slice(display);
                os.extend_from_slice(b"\x27 does not define any views.");
                return Err(fail(validation_text, os));
            }
            numdisplays += 1;

            // Confirm shared view exist and do not conflict with views.
            for shared_view in shared_views {
                self.validate_shared_view(display, views, shared_view, true, validation_text)?;
            }

            // Confirm view references exist.
            for view in views {
                self.validate_view(display, view, true, validation_text)?;
            }
        }

        // Confirm at least one display entry exists.
        if numdisplays == 0 {
            return Err(fail(
                validation_text,
                b"Config failed display validation. No displays are specified.".to_vec(),
            ));
        }

        ///// VIRTUAL DISPLAY.

        if self.major_version() >= 2 {
            // Confirm shared view exist and do not conflict with views.
            for shared_view in &self.virtual_display.shared_views {
                // Bypass the <USE_DISPLAY_NAME> validation.
                self.validate_shared_view(
                    b"virtual_display",
                    &self.virtual_display.views,
                    shared_view,
                    false,
                    validation_text,
                )?;
            }

            // Confirm view references exist.
            for view in &self.virtual_display.views {
                // Bypass the <USE_DISPLAY_NAME> validation.
                self.validate_view(b"virtual_display", view, false, validation_text)?;
            }
        }

        ///// ACTIVE DISPLAYS & VIEWS

        let displays: StringVec = self.displays.iter().map(|(name, _)| name.clone()).collect();

        if !self.active_displays_env_override.is_empty() {
            let use_all_displays = self.active_displays_env_override.len() == 1
                && self.active_displays_env_override[0].is_empty();

            if !use_all_displays {
                let ordered_displays = intersect_string_vecs_case_ignore(
                    &self.active_displays_env_override,
                    &displays,
                );
                if ordered_displays.is_empty() {
                    let mut os =
                        b"The content of the env. variable for the list of active displays ["
                            .to_vec();
                    os.extend_from_slice(&join_string_env_style(
                        &self.active_displays_env_override,
                    ));
                    os.extend_from_slice(b"] is invalid.");
                    return Err(fail(validation_text, os));
                }
                if ordered_displays.len() != self.active_displays_env_override.len() {
                    let mut os =
                        b"The content of the env. variable for the list of active displays ["
                            .to_vec();
                    os.extend_from_slice(&join_string_env_style(
                        &self.active_displays_env_override,
                    ));
                    os.extend_from_slice(b"] contains invalid display name(s).");
                    return Err(fail(validation_text, os));
                }
            }
        } else if !self.active_displays.is_empty() {
            let use_all_displays =
                self.active_displays.len() == 1 && self.active_displays[0].is_empty();

            if !use_all_displays {
                let ordered_displays =
                    intersect_string_vecs_case_ignore(&self.active_displays, &displays);
                if ordered_displays.is_empty() {
                    let mut os = b"The list of active displays [".to_vec();
                    os.extend_from_slice(&join_string_env_style(&self.active_displays));
                    os.extend_from_slice(b"] from the config file is invalid.");
                    return Err(fail(validation_text, os));
                }
                if ordered_displays.len() != self.active_displays.len() {
                    let mut os = b"The list of active displays [".to_vec();
                    os.extend_from_slice(&join_string_env_style(&self.active_displays));
                    os.extend_from_slice(
                        b"] from the config file contains invalid display name(s).",
                    );
                    return Err(fail(validation_text, os));
                }
            }
        }

        // TODO: Add validation for active views.

        ///// TRANSFORMS

        // Confirm for all transforms that reference internal color spaces,
        // the named color space exists and that all transforms are valid.
        {
            let all_transforms = self.all_internal_transforms();

            let context = self.current_context().get();

            let mut color_space_names = BTreeSet::new();
            for transform in &all_transforms {
                transform.validate()?;
                get_color_space_references(&mut color_space_names, transform, &context);
            }

            for name in &color_space_names {
                let name = c_str(name);
                // Check to see if the name is a color space.
                if !self.has_color_space(name) {
                    // As a role name forbids the use of context variable keywords and
                    // GetColorSpaceReferences() should expand context variables, throw if a
                    // context variable keyword is still present.
                    if contains_context_variables(name) {
                        let mut oss = b"Config failed transform validation. This config \
                                        references a color space \x27"
                            .to_vec();
                        oss.extend_from_slice(name);
                        oss.extend_from_slice(b"\x27 using an unknown context variable.");
                        return Err(fail(validation_text, oss));
                    }

                    // Check to see if the name is a role.
                    let csname = lookup_role(&self.roles, name);

                    let mut os = b"Config failed transform validation. This config references \
                                   a color space, \x27"
                        .to_vec();

                    if csname.is_empty() {
                        // It's not a role, check to see if it's a named transform.
                        if self.impl_named_transform(name).is_none() {
                            // It's not a color space, a role, or a named transform.
                            os.extend_from_slice(name);
                            os.extend_from_slice(b"\x27, which is not defined.");
                            return Err(fail(validation_text, os));
                        }
                    } else if !self.has_color_space(c_str(csname)) {
                        // It's a role, but the color space it points to doesn't exist.
                        os.extend_from_slice(csname);
                        os.extend_from_slice(b"\x27 (for role \x27");
                        os.extend_from_slice(name);
                        os.extend_from_slice(b"\x27), which is not defined.");
                        return Err(fail(validation_text, os));
                    }
                }
            }
        }

        ///// LOOKS

        // For all looks, confirm the process space exists and the look is named.
        for (i, look) in self.looks_list.iter().enumerate() {
            let look_name = c_str(look.name());
            if look_name.is_empty() {
                return Err(fail(
                    validation_text,
                    format!(
                        "Config failed Look validation. The look at index \x27{i}\x27 does not \
                         specify a name."
                    )
                    .into_bytes(),
                ));
            }

            let process_space = c_str(look.process_space());
            if process_space.is_empty() {
                let mut os = b"Config failed Look validation. The look \x27".to_vec();
                os.extend_from_slice(look_name);
                os.extend_from_slice(b"\x27 does not specify a process space.");
                return Err(fail(validation_text, os));
            }

            if !self.has_color_space(process_space) {
                // Check to see if the processSpace is a role.
                let csname = lookup_role(&self.roles, process_space);

                let mut os = b"Config failed Look validation. The look \x27".to_vec();
                os.extend_from_slice(look_name);
                os.extend_from_slice(b"\x27 specifies a process color space, \x27");

                if csname.is_empty() {
                    os.extend_from_slice(process_space);
                    os.extend_from_slice(b"\x27, which is not defined.");
                    return Err(fail(validation_text, os));
                } else if !self.has_color_space(c_str(csname)) {
                    os.extend_from_slice(csname);
                    os.extend_from_slice(b"\x27 (for role \x27");
                    os.extend_from_slice(process_space);
                    os.extend_from_slice(b"\x27), which is not defined.");
                    return Err(fail(validation_text, os));
                }
            }
        }

        ///// ViewTransforms

        if !self.view_transforms.is_empty() {
            // Note: Config::addViewTransform validates that view_transforms have a unique,
            // non-empty name and define a transform.

            let from_scene = self.default_scene_to_display_view_transform();
            // If there are view transforms, there must be one from the scene reference space.
            if from_scene.is_none() {
                return Err(fail(
                    validation_text,
                    b"Config failed validation. If there are view_transforms, at least one must \
                      use the scene reference space."
                        .to_vec(),
                ));
            }
        } else if has_display_referred_colorspace {
            return Err(fail(
                validation_text,
                b"Config failed validation. If there are display-referred color spaces, there \
                  must be view_transforms."
                    .to_vec(),
            ));
        }

        if !self.default_view_transform.is_empty() {
            let vt = self.default_scene_to_display_view_transform();
            if vt.is_none_or(|vt| !compare(vt.name(), &self.default_view_transform)) {
                let mut os =
                    b"Config failed validation. Default view transform is defined as: \x27"
                        .to_vec();
                os.extend_from_slice(&self.default_view_transform);
                os.extend_from_slice(
                    b"\x27 but this does not correspond to an existing scene-referred view \
                      transform.",
                );
                return Err(fail(validation_text, os));
            }
        }

        ///// FileRules

        if let Err(e) = self.file_rules.get().validate(self) {
            let mut os = b"Config failed validation. File rules failed with: ".to_vec();
            os.extend_from_slice(e.what());
            return Err(fail(validation_text, os));
        }

        ///// Resolve all file Transforms using context variables.

        {
            let all_transforms = self.all_internal_transforms();

            let mut files = BTreeSet::new();
            for transform in &all_transforms {
                get_file_references(&mut files, transform);
            }

            // Check that at least one of the search paths can be resolved into a valid path.
            // Note that a search path without context variable(s) always correctly resolves.

            let context = self.shared_context.get();
            if !files.is_empty() {
                let mut found_one = false;
                let mut err_msg = b"Config failed search path validation.".to_vec();

                for idx in 0..context.num_search_paths() {
                    let path = c_str(context.search_path_with_index(idx));
                    if path.is_empty() {
                        err_msg.extend_from_slice(
                            b" The search_path must not be an empty string if there are \
                              FileTransforms.",
                        );
                        continue;
                    }

                    let resolved_search_path = context.resolve_string_var(path);
                    if contains_context_variables(&resolved_search_path) {
                        let mut oss = b"  The search_path \x27".to_vec();
                        oss.extend_from_slice(path);
                        oss.extend_from_slice(b"\x27 cannot be resolved");

                        if path != resolved_search_path.as_slice() {
                            // Adjust the error message when the search_path is defined with
                            // some context variable(s).
                            oss.extend_from_slice(b" by \x27");
                            oss.extend_from_slice(&resolved_search_path);
                            oss.extend_from_slice(b"\x27");
                        }

                        oss.extend_from_slice(b".");
                        err_msg.extend_from_slice(&oss);
                        continue;
                    }

                    found_one = true;
                }

                // After looping over all the search paths, none of them can be successfully
                // resolved.
                if !found_one {
                    if context.num_search_paths() == 0 {
                        err_msg.extend_from_slice(
                            b" The search_path must not be empty if there are FileTransforms.",
                        );
                    }
                    return Err(fail(validation_text, err_msg));
                }
            }

            // Expand all file transform paths.

            for file in &files {
                // Resolve the file name without testing if it exists (which could add an
                // unnecessary performance hit).
                let resolved_file = context.resolve_string_var(file);
                if resolved_file.is_empty() || contains_context_variables(&resolved_file) {
                    let mut oss = b"Config failed validation expanding file transform paths. \
                                    The file transform source cannot be resolved: \x27"
                        .to_vec();

                    if file != &resolved_file {
                        oss.extend_from_slice(file);
                        oss.extend_from_slice(b"\x27 vs. \x27");
                        oss.extend_from_slice(&resolved_file);
                        oss.extend_from_slice(b"\x27.");
                    } else {
                        oss.extend_from_slice(file);
                        oss.extend_from_slice(b"\x27.");
                    }

                    return Err(fail(validation_text, oss));
                }
            }
        }

        ///// NamedTransforms

        // As Config::addNamedTransform() already validates some properties of the instance
        // (i.e. name is not null, at least forward or inverse transform exits, etc.), the code
        // below only has to validate name conflicts. The NamedTransform name can not use a role,
        // a color space, a look, or a view transform name.  All transforms are validated above.

        for nt in &self.all_named_transforms {
            let name = c_str(nt.name());

            if self.look(name).is_some() {
                let mut os =
                    b"Config failed validation. NamedTransform can\x27t be named \x27".to_vec();
                os.extend_from_slice(name);
                os.extend_from_slice(b"\x27. This name is already used for a look.");
                return Err(fail(validation_text, os));
            }
            if self.view_transform(name).is_some() {
                let mut os =
                    b"Config failed validation. NamedTransform can\x27t be named \x27".to_vec();
                os.extend_from_slice(name);
                os.extend_from_slice(b"\x27. This name is already used for a view transform.");
                return Err(fail(validation_text, os));
            }

            // AddColorSpace, addNamedTransform & setRole already check there is no name & alias
            // conflict.
        }

        ///// Check new features are not used with older config versions.

        self.check_version_consistency()
    }

    /// Whether the config has the color space (or alias) `csname`, ignoring case; roles don't
    /// count.
    ///
    /// Port of `Config::Impl::hasColorSpace` (src/OpenColorIO/Config.cpp:480-483 @ v2.5.2).
    fn has_color_space(&self, csname: &[u8]) -> bool {
        self.all_color_spaces.has_color_space(csname)
    }

    /// Refuses a view (of `display`, or a shared view when `display` is empty) without a name or
    /// a color space, one that uses `<USE_DISPLAY_NAME>` where it can't (with
    /// `check_use_display_name`), one whose color space, view transform (and its display color
    /// space), looks or viewing rule the config doesn't have. `validation_text` is upstream's
    /// `m_validationtext`, which each refusal sets.
    ///
    /// Port of `Config::Impl::validateView` (src/OpenColorIO/Config.cpp:592-701 @ v2.5.2).
    fn validate_view(
        &self,
        display: &[u8],
        view: &View,
        check_use_display_name: bool,
        validation_text: &mut Vec<u8>,
    ) -> Result<()> {
        let mut fail = |os: Vec<u8>| {
            *validation_text = os;
            Err(Exception::new(validation_text.clone()))
        };

        if view.name.is_empty() {
            return fail(get_display_view_prefix_error_msg(display, view));
        }

        let shared_view_with_view_transform = display.is_empty() && !view.view_transform.is_empty();

        // Validate color space name is not empty.
        if view.colorspace.is_empty() {
            let mut os = get_display_view_prefix_error_msg(display, view);
            os.extend_from_slice(b"does not refer to a color space.");
            return fail(os);
        }

        // USE_DISPLAY_NAME can only be used by shared views.
        if check_use_display_name
            && !shared_view_with_view_transform
            && view.use_display_name_for_colorspace()
        {
            let mut os = get_display_view_prefix_error_msg(display, view);
            os.extend_from_slice(b"can not use \x27");
            os.extend_from_slice(OCIO_VIEW_USE_DISPLAY_NAME.as_bytes());
            os.extend_from_slice(b"\x27 keyword for the color space name.");
            return fail(os);
        }

        // If USE_DISPLAY_NAME is not present, a valid color space must be specified.
        if !view.use_display_name_for_colorspace()
            && !self.has_color_space(c_str(&view.colorspace))
            && self.impl_named_transform(c_str(&view.colorspace)).is_none()
        {
            let mut os = get_display_view_prefix_error_msg(display, view);
            os.extend_from_slice(b"that refers to a color space or a named transform, \x27");
            os.extend_from_slice(&view.colorspace);
            os.extend_from_slice(b"\x27, which is not defined.");
            return fail(os);
        }

        // If there is a view transform, it must exist (or be a named transform) and its color
        // space must be a display-referred color space.
        if !view.view_transform.is_empty() {
            if self
                .impl_named_transform(c_str(&view.view_transform))
                .is_none()
                && self.view_transform(c_str(&view.view_transform)).is_none()
            {
                let mut os = get_display_view_prefix_error_msg(display, view);
                os.extend_from_slice(b"that refers to a view transform, \x27");
                os.extend_from_slice(&view.view_transform);
                os.extend_from_slice(
                    b"\x27, which is neither a view transform nor a named transform.",
                );
                return fail(os);
            }
            let display_cs = if view.use_display_name_for_colorspace() {
                c_str(display)
            } else {
                c_str(&view.colorspace)
            };
            if let Some(cs) = self.color_space(display_cs)
                && cs.reference_space_type() != ReferenceSpaceType::Display
            {
                let mut os = get_display_view_prefix_error_msg(display, view);
                os.extend_from_slice(b"refers to a color space, \x27");
                os.extend_from_slice(display_cs);
                os.extend_from_slice(b"\x27, that is not a display-referred color space.");
                return fail(os);
            }
        }

        // Confirm looks references exist.
        let mut looks = LookParseResult::default();
        let options = looks.parse(&view.looks)?;

        for option in options {
            for token in option {
                let look = &token.name;

                if !look.is_empty() && self.look(c_str(look)).is_none() {
                    let mut os = get_display_view_prefix_error_msg(display, view);
                    os.extend_from_slice(b"refers to a look, \x27");
                    os.extend_from_slice(look);
                    os.extend_from_slice(b"\x27, which is not defined.");
                    return fail(os);
                }
            }
        }

        if !view.rule.is_empty() && find_rule(&self.viewing_rules.get(), &view.rule).is_none() {
            let mut os = get_display_view_prefix_error_msg(display, view);
            os.extend_from_slice(b"refers to a viewing rule, \x27");
            os.extend_from_slice(&view.rule);
            os.extend_from_slice(b"\x27, which is not defined.");
            return fail(os);
        }
        Ok(())
    }

    /// Refuses a shared view of `display` that is also one of its own views, one the config
    /// doesn't define, and (with `check_use_display_name`) a shared view with a view transform
    /// and `<USE_DISPLAY_NAME>` whose display has no display-referred color space of its name.
    ///
    /// Port of `Config::Impl::validateSharedView` (src/OpenColorIO/Config.cpp:704-765 @
    /// v2.5.2).
    fn validate_shared_view(
        &self,
        display: &[u8],
        views_of_display: &[View],
        shared_view: &[u8],
        check_use_display_name: bool,
        validation_text: &mut Vec<u8>,
    ) -> Result<()> {
        let mut fail = |os: Vec<u8>| {
            *validation_text = os;
            Err(Exception::new(validation_text.clone()))
        };

        // Is the name already used for a display-defined view?
        // This should never happen because this is checked when adding a view.
        if find_view(views_of_display, shared_view).is_some() {
            let mut os = b"Config failed view validation. The display \x27".to_vec();
            os.extend_from_slice(display);
            os.extend_from_slice(b"\x27 contains a shared view \x27");
            os.extend_from_slice(shared_view);
            os.extend_from_slice(b"\x27 that is already defined as a view.");
            return fail(os);
        }

        // Is the shared view defined?
        match find_view(&self.shared_views, shared_view) {
            None => {
                let mut os = b"Config failed view validation. The display \x27".to_vec();
                os.extend_from_slice(display);
                os.extend_from_slice(b"\x27 contains a shared view \x27");
                os.extend_from_slice(shared_view);
                os.extend_from_slice(b"\x27 that is not defined.");
                return fail(os);
            }
            Some(idx) if check_use_display_name => {
                let view = &self.shared_views[idx];
                if !view.view_transform.is_empty() && view.use_display_name_for_colorspace() {
                    // Shared views using a view transform can omit the colorspace, in that
                    // case the color space to use should be named from the display.
                    match self.color_space(c_str(display)) {
                        None => {
                            let mut os =
                                b"Config failed view validation. The display \x27".to_vec();
                            os.extend_from_slice(display);
                            os.extend_from_slice(b"\x27 contains a shared view \x27");
                            os.extend_from_slice(&view.name);
                            os.extend_from_slice(
                                b"\x27 which does not define a color space and there is no \
                                  color space that matches the display name.",
                            );
                            return fail(os);
                        }
                        Some(display_cs)
                            if display_cs.reference_space_type() != ReferenceSpaceType::Display =>
                        {
                            let mut os =
                                b"Config failed view validation. The display \x27".to_vec();
                            os.extend_from_slice(display);
                            os.extend_from_slice(b"\x27 contains a shared view \x27");
                            os.extend_from_slice(&view.name);
                            os.extend_from_slice(b"\x27 that refers to a color space, \x27");
                            os.extend_from_slice(display);
                            os.extend_from_slice(
                                b"\x27, that is not a display-referred color space.",
                            );
                            return fail(os);
                        }
                        Some(_) => {}
                    }
                }
            }
            Some(_) => {}
        }
        Ok(())
    }

    /// Whether a color space named `name` is used other than where it is defined: by a
    /// transform (a color space, display view or look transform, context variables resolved),
    /// a role, a shared view, a (display, view) pair, a look's process space or a file rule;
    /// ignoring case.
    ///
    /// Port of `Config::isColorSpaceUsed` (src/OpenColorIO/Config.cpp:2659-2771 @ v2.5.2).
    #[doc(alias = "isColorSpaceUsed")]
    pub fn is_color_space_used(&self, name: impl AsRef<[u8]>) -> bool {
        // Check if a color space is used somewhere in the config other than where it is
        // defined, for example, in a display/view, look, or ColorSpaceTransform. If the color
        // space is defined in the config, but not used elsewhere, this function returns false.

        let name = c_str(name.as_ref());
        if name.is_empty() {
            return false;
        }

        // Check for all color spaces, looks and view transforms.

        let all_transforms = self.all_internal_transforms();

        let mut color_space_names = BTreeSet::new();
        for transform in all_transforms {
            get_color_space_references(
                &mut color_space_names,
                transform,
                &self.shared_context.get(),
            );
        }

        if color_space_names
            .iter()
            .any(|cs_name| strcasecmp(name, c_str(cs_name)).is_eq())
        {
            return true;
        }

        // Check for roles.

        for idx in 0..self.num_roles() {
            let role_name = self.role_name(idx);
            let cs_name = lookup_role(&self.roles, role_name);
            if strcasecmp(cs_name, name).is_eq() {
                return true;
            }
        }

        // Check for all shared views.

        if self
            .shared_views
            .iter()
            .any(|view| strcasecmp(&view.colorspace, name).is_eq())
        {
            return true;
        }

        // Check for all (display, view) pairs (i.e. active and inactive ones).

        for (disp_name, display) in &self.displays {
            for view in &display.views {
                let cs_name = self.display_view_color_space_name(disp_name, &view.name);
                if strcasecmp(cs_name, name).is_eq() {
                    return true;
                }
            }
            for shared_view in &display.shared_views {
                if let Some(i) = find_view(&self.shared_views, shared_view) {
                    let view = &self.shared_views[i];
                    if !view.view_transform.is_empty()
                        && view.use_display_name_for_colorspace()
                        && strcasecmp(disp_name, name).is_eq()
                    {
                        return true;
                    }
                }
            }
        }

        // Check for 'process_space' from look.

        for idx in 0..self.num_looks() {
            let look_name = self.look_name_by_index(idx);

            let l = self.look(look_name).expect("a look of the config");
            if strcasecmp(l.process_space(), name).is_eq() {
                return true;
            }
        }

        // Check the file rules.

        let rules = self.file_rules.get();

        let num_rules = rules.num_entries();
        for idx in 0..num_rules {
            let cs_name = rules.color_space(idx).expect("a rule's index");
            if strcasecmp(&cs_name, name).is_eq() {
                return true;
            }
        }

        false
    }

    /// Port of `Config::getProcessorCacheFlags` (Config.cpp:924-927, 5333-5336 @ v2.5.2).
    #[doc(alias = "getProcessorCacheFlags")]
    pub fn processor_cache_flags(&self) -> ProcessorCacheFlags {
        ProcessorCacheFlags(self.cache_flags.load(Ordering::Relaxed))
    }

    /// Sets the cache flags of the config and of the processors it makes from now on, and
    /// enables or disables the config's cache of processors (which keeps its entries).
    ///
    /// Port of `Config::setProcessorCacheFlags` (Config.cpp:929-933, 5338-5341 @ v2.5.2).
    #[doc(alias = "setProcessorCacheFlags")]
    pub fn set_processor_cache_flags(&self, flags: ProcessorCacheFlags) {
        self.cache_flags.store(flags.0, Ordering::Relaxed);
        self.processor_cache
            .enable(flags.has_flag(ProcessorCacheFlags::ENABLED));
    }

    /// Empties the config's cache of processors.
    ///
    /// Port of `Config::clearProcessorCache` (Config.cpp:5343-5346 @ v2.5.2).
    #[doc(alias = "clearProcessorCache")]
    pub fn clear_processor_cache(&self) {
        self.processor_cache.clear();
    }

    /// The processor of `transform`, forward, in the current context.
    ///
    /// Port of `Config::getProcessor(const ConstTransformRcPtr &)` (Config.cpp:4779-4782 @
    /// v2.5.2).
    #[doc(alias = "getProcessor")]
    pub fn processor(&self, transform: &Transform) -> Result<Arc<Processor>> {
        self.processor_in_direction(transform, TransformDirection::Forward)
    }

    /// The processor of `transform` in the direction `direction`, in the current context.
    ///
    /// Port of `Config::getProcessor(const ConstTransformRcPtr &, TransformDirection)`
    /// (Config.cpp:4784-4789 @ v2.5.2).
    #[doc(alias = "getProcessor")]
    pub fn processor_in_direction(
        &self,
        transform: &Transform,
        direction: TransformDirection,
    ) -> Result<Arc<Processor>> {
        let context = self.shared_context.get();
        self.processor_with_context(&context, transform, direction)
    }

    /// The processor of `transform` in the direction `direction`, in `context`: from the
    /// config's cache when it has one under the same key (the transform's text, the direction,
    /// and the context variables it uses), or, unless `OCIO_DISABLE_CACHE_FALLBACK` is set, one
    /// with the same cache ID; otherwise a new one, which the cache keeps.
    ///
    /// Port of `Config::getProcessor(const ConstContextRcPtr &, const ConstTransformRcPtr &,
    /// TransformDirection)` (Config.cpp:4791-4880 @ v2.5.2). Its errors for a null context or
    /// transform can't happen with references.
    #[doc(alias = "getProcessor")]
    pub fn processor_with_context(
        &self,
        context: &Context,
        transform: &Transform,
        direction: TransformDirection,
    ) -> Result<Arc<Processor>> {
        // The goal of the usedContext is to only contain the context vars that are actually
        // used for this transform. This allows the cache to be more efficient. However, there
        // are still some various TODOs since the usedContext will sometimes contain more vars
        // than are needed.
        //
        // The context's search path, working directory and IO proxy, which upstream copies
        // into it, come with the context's state (Phase 3).
        let mut used_context = Context::new();

        let need_context_variables =
            collect_context_variables(self, context, transform, &mut used_context);

        // Create helper method.
        let create_processor = || -> Result<Arc<Processor>> {
            let mut processor = Processor::new();
            processor.set_processor_cache_flags(self.processor_cache_flags());
            processor.set_transform(self, context, transform, direction)?;
            processor.compute_metadata();
            Ok(Arc::new(processor))
        };

        if let Some(mut cache) = self.processor_cache.lock() {
            // Note that the key includes a string description of the transform which does not
            // include all the LUT entries (just the arguments of the FileTransforms for LUTs).
            if need_context_variables {
                // Only the transforms that read context variables (color space, display view,
                // file and look transforms) need them, and none of those is ported yet; the
                // used context's cache ID comes with them.
                return Err(Exception::new(
                    "Config::getProcessor: the cache ID of a context is not ported yet.",
                ));
            }
            // `oss << "" << *transform << direction`: the direction prints as its value.
            let text = format!("{transform}{}", direction as i32);
            let key = std_hash_string(text.as_bytes());

            // Upstream's `m_processorCache[key]` adds an empty entry before it creates the
            // processor, which stays empty when the creation throws; the fallback below skips
            // empty entries, so the port adds none.
            if let Some(processor) = cache.entries().get(&key) {
                return Ok(processor.clone());
            }

            let proc = create_processor()?;

            let mut processor = None;

            let do_fallback = !is_env_present(OCIO_DISABLE_CACHE_FALLBACK);
            if do_fallback {
                // If an entry with the same cache ID already exists in the cache then reuse it
                // instead of the newly created one. Even with different context, the same
                // processor could be created (e.g. the processor creation does not rely on
                // some context variables).

                // The benefit to using the existing one is that it may already have an
                // optimized Processor, CPUProcessor, or GPUProcessor inside it.

                // Upstream compares the two cache IDs with `strcmp`; a cache ID is a hash or
                // "<NOOP>", without NUL. Both processors' ops are finalized, so neither cache
                // ID can fail.
                for entry in cache.entries().values() {
                    if entry.cache_id()? == proc.cache_id()? {
                        processor = Some(entry.clone());
                        break;
                    }
                }
            }

            let processor = processor.unwrap_or(proc);
            cache.entries().insert(key, processor.clone());
            return Ok(processor);
        }
        create_processor()
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
