// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The config: a port of `src/OpenColorIO/Config.cpp` @ v2.5.2, in part: its state and
//! constructor, the copy, the versions, the name, description and family separator, the
//! environment and search paths, and the processors of a transform with their cache. Color
//! spaces, roles, displays, looks and the rest come with the later Phase 3 chunks.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use ocio_ops::exception::{Exception, Result};
use ocio_ops::image_desc::PackedImageDesc;
use ocio_ops::math_utils::equal_with_abs_error;
use ocio_ops::open_color_types::{
    Allocation, BitDepth, ChannelOrdering, ColorSpaceDirection, ColorSpaceVisibility,
    EnvironmentMode, OptimizationFlags, ReferenceSpaceType, SearchReferenceSpaceType,
    TransformDirection,
};
use ocio_ops::parse_utils::{ROLE_DEFAULT, split_string_env_style};
use ocio_ops::platform::{getenv, is_env_present};
use ocio_ops::utils::pystring;
use ocio_ops::utils::string_utils::{StringVec, c_str, compare, lower, split, trim};

use crate::caching::{OCIO_DISABLE_CACHE_FALLBACK, ProcessorCache, std_hash_string};
use crate::color_space::ColorSpace;
use crate::color_space_set::ColorSpaceSet;
use crate::context::Context;
use crate::context_variable_utils::{collect_context_variables, contains_context_variable_token};
use crate::named_transform::NamedTransform;
use crate::processor::{Processor, ProcessorCacheFlags};
use crate::transform::Transform;

/// `OCIO_ACTIVE_DISPLAYS`: the displays a config shows, overriding its own list.
///
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

/// What a list of inactive names is built for. Upstream's `INACTIVE_ALL` (the names as they
/// are, for `validate`) comes with `validate` (3.8a).
///
/// Port of `Config::Impl::InactiveType` (src/OpenColorIO/Config.cpp:521-526 @ v2.5.2), in part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InactiveType {
    /// `INACTIVE_COLORSPACE`.
    ColorSpace,
    /// `INACTIVE_NAMEDTRANSFORM`.
    NamedTransform,
}

/// The result of the config's last validation.
///
/// Port of `Config::Impl::Validation` (src/OpenColorIO/Config.cpp:257-262 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Validation {
    /// `VALIDATION_UNKNOWN`.
    Unknown,
    /// `VALIDATION_PASSED`: set by `validate` (3.8a).
    #[allow(dead_code)]
    Passed,
    /// `VALIDATION_FAILED`: set by `validate` (3.8a).
    #[allow(dead_code)]
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
    /// `m_context`.
    context: Arc<Context>,
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
    /// `m_activeDisplaysEnvOverride`: the displays of `OCIO_ACTIVE_DISPLAYS`.
    active_displays_env_override: StringVec,
    /// `m_activeViewsEnvOverride`: the views of `OCIO_ACTIVE_VIEWS`.
    active_views_env_override: StringVec,
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
        let config = Config {
            major_version: self.major_version,
            minor_version: self.minor_version,
            env: self.env.clone(),
            context: Arc::new((*self.context).clone()),
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
            all_named_transforms: self.all_named_transforms.clone(),
            active_named_transform_names: self.active_named_transform_names.clone(),
            inactive_named_transform_names: self.inactive_named_transform_names.clone(),
            active_displays_env_override: self.active_displays_env_override.clone(),
            active_views_env_override: self.active_views_env_override.clone(),
            default_luma_coefs: self.default_luma_coefs,
            strict_parsing: self.strict_parsing,
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

impl Config {
    /// The state `Config::Impl::Impl` gives a config before it reads the environment.
    fn blank() -> Config {
        let config = Config {
            major_version: LAST_SUPPORTED_MAJOR_VERSION,
            minor_version: LAST_SUPPORTED_MINOR_VERSION[LAST_SUPPORTED_MAJOR_VERSION as usize - 1],
            env: BTreeMap::new(),
            context: Arc::new(Context::new()),
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
            all_named_transforms: Vec::new(),
            active_named_transform_names: StringVec::new(),
            inactive_named_transform_names: StringVec::new(),
            active_displays_env_override: StringVec::new(),
            active_views_env_override: StringVec::new(),
            default_luma_coefs: DEFAULT_LUMA_COEFFS,
            strict_parsing: true,
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

    /// The current context, to change it. Upstream changes the context it shares with the
    /// callers of `getCurrentContext`; the port changes its own, so a caller's
    /// [`Config::current_context`] keeps the state it had: when the context is shared, the
    /// config takes a copy of it first (with its environment mode, which
    /// `Context::createEditableCopy` doesn't keep).
    fn context_mut(&mut self) -> &mut Context {
        if Arc::get_mut(&mut self.context).is_none() {
            let mode = self.context.environment_mode();
            let mut copy = (*self.context).clone();
            copy.set_environment_mode(mode);
            self.context = Arc::new(copy);
        }
        Arc::get_mut(&mut self.context).expect("a context of its own")
    }

    /// The raw config: version 2.0, no strict parsing, its one color space `raw` and the role
    /// `default`, its display, and the whole environment in its context (its profile has no
    /// `environment` section). So far its version, strict parsing, color space, role and
    /// environment; the rest of its state comes with the displays and the file rules, built
    /// directly until the YAML reader parses upstream's profile (3.7d).
    ///
    /// Port of `Config::CreateRaw` (src/OpenColorIO/Config.cpp:74-92, 1127-1133 @ v2.5.2), in
    /// part, with what `OCIOYaml`'s `load` sets from that profile (`setVersion`,
    /// `setStrictParsingEnabled`, `setRole`, the
    /// color space's setters and `addColorSpace`, `setEnvironmentMode`, `loadEnvironment`) and
    /// `Config::Impl::Read`'s refresh of the active color spaces (Config.cpp:5545-5561).
    #[doc(alias = "CreateRaw")]
    pub fn create_raw() -> Arc<Config> {
        let mut config = Config::blank();
        config.minor_version = 0;
        config.set_strict_parsing_enabled(false);
        config
            .set_role(ROLE_DEFAULT, Some(b"raw"))
            .expect("the raw config's role");

        let mut cs = ColorSpace::new();
        cs.set_name("raw");
        cs.set_family("raw");
        cs.set_equality_group("");
        cs.set_bit_depth(BitDepth::F32);
        cs.set_is_data(true);
        cs.set_allocation(Allocation::Uniform);
        cs.set_description("A raw color space. Conversions to and from this space are no-ops.");
        config
            .add_color_space(&cs)
            .expect("the raw config's color space");

        config.set_environment_mode(EnvironmentMode::LoadAll);
        config.load_environment();

        config.inactive_color_space_names_api.clear();
        config.refresh_active_color_spaces();
        Arc::new(config)
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

    /// The current context. A change of the config's context (its environment, search paths
    /// or working directory) is not seen through a context returned before it.
    ///
    /// Port of `Config::getCurrentContext` (src/OpenColorIO/Config.cpp:2161-2164 @ v2.5.2).
    #[doc(alias = "getCurrentContext")]
    pub fn current_context(&self) -> &Arc<Context> {
        &self.context
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
                self.context_mut().set_string_var(name, Some(default_value));
            }
            None => {
                self.env.remove(name);
                self.context_mut().set_string_var(name, None);
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
        self.context_mut().clear_string_vars();

        self.reset_cache_ids();
    }

    /// Sets the environment mode of the context.
    ///
    /// Port of `Config::setEnvironmentMode` (src/OpenColorIO/Config.cpp:2219-2225 @ v2.5.2).
    #[doc(alias = "setEnvironmentMode")]
    pub fn set_environment_mode(&mut self, mode: EnvironmentMode) {
        self.context_mut().set_environment_mode(mode);

        self.reset_cache_ids();
    }

    /// Port of `Config::getEnvironmentMode` (src/OpenColorIO/Config.cpp:2227-2230 @ v2.5.2).
    #[doc(alias = "getEnvironmentMode")]
    pub fn environment_mode(&self) -> EnvironmentMode {
        self.context.environment_mode()
    }

    /// Loads the environment into the context (`Context::loadEnvironment`).
    ///
    /// Port of `Config::loadEnvironment` (src/OpenColorIO/Config.cpp:2232-2238 @ v2.5.2).
    #[doc(alias = "loadEnvironment")]
    pub fn load_environment(&mut self) {
        self.context_mut().load_environment();

        self.reset_cache_ids();
    }

    /// The search path, as set or joined with `:`.
    ///
    /// Port of `Config::getSearchPath()` (src/OpenColorIO/Config.cpp:2240-2243 @ v2.5.2).
    #[doc(alias = "getSearchPath")]
    pub fn search_path(&self) -> &[u8] {
        self.context.search_path()
    }

    /// Sets the search paths from `path`, split on `:` (`Context::setSearchPath`).
    ///
    /// Port of `Config::setSearchPath` (src/OpenColorIO/Config.cpp:2245-2251 @ v2.5.2).
    #[doc(alias = "setSearchPath")]
    pub fn set_search_path(&mut self, path: impl AsRef<[u8]>) {
        self.context_mut().set_search_path(path);

        self.reset_cache_ids();
    }

    /// Port of `Config::getNumSearchPaths` (src/OpenColorIO/Config.cpp:2253-2256 @ v2.5.2).
    #[doc(alias = "getNumSearchPaths")]
    pub fn num_search_paths(&self) -> i32 {
        self.context.num_search_paths()
    }

    /// The search path at `index`; `""` outside the list.
    ///
    /// Port of `Config::getSearchPath(int)` (src/OpenColorIO/Config.cpp:2258-2261 @ v2.5.2).
    #[doc(alias = "getSearchPath")]
    pub fn search_path_with_index(&self, index: i32) -> &[u8] {
        self.context.search_path_with_index(index)
    }

    /// Port of `Config::clearSearchPaths` (src/OpenColorIO/Config.cpp:2263-2269 @ v2.5.2).
    #[doc(alias = "clearSearchPaths")]
    pub fn clear_search_paths(&mut self) {
        self.context_mut().clear_search_paths();

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
        self.context_mut().add_search_path(path);

        self.reset_cache_ids();
    }

    /// Port of `Config::getWorkingDir` (src/OpenColorIO/Config.cpp:2280-2283 @ v2.5.2).
    #[doc(alias = "getWorkingDir")]
    pub fn working_dir(&self) -> &[u8] {
        self.context.working_dir()
    }

    /// Port of `Config::setWorkingDir` (src/OpenColorIO/Config.cpp:2285-2291 @ v2.5.2).
    #[doc(alias = "setWorkingDir")]
    pub fn set_working_dir(&mut self, dirname: impl AsRef<[u8]>) {
        self.context_mut().set_working_dir(dirname);

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
        processor.set_transform(self, &self.context, transform, direction)?;
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
        let context = self.current_context().clone();
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
