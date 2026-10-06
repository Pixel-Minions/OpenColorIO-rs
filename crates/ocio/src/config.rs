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
use ocio_ops::open_color_types::{EnvironmentMode, TransformDirection};
use ocio_ops::parse_utils::split_string_env_style;
use ocio_ops::platform::{getenv, is_env_present};
use ocio_ops::utils::string_utils::{StringVec, c_str, trim};

use crate::caching::{OCIO_DISABLE_CACHE_FALLBACK, ProcessorCache, std_hash_string};
use crate::context::Context;
use crate::context_variable_utils::collect_context_variables;
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
    /// `m_inactiveColorSpaceNamesEnv`: `OCIO_INACTIVE_COLORSPACES`, trimmed.
    inactive_color_space_names_env: Vec<u8>,
    /// `m_activeDisplaysEnvOverride`: the displays of `OCIO_ACTIVE_DISPLAYS`.
    active_displays_env_override: StringVec,
    /// `m_activeViewsEnvOverride`: the views of `OCIO_ACTIVE_VIEWS`.
    active_views_env_override: StringVec,
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
            inactive_color_space_names_env: self.inactive_color_space_names_env.clone(),
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
            inactive_color_space_names_env: Vec::new(),
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

    /// The raw config: version 2.0, its one color space `raw`, its roles and its display, and
    /// the whole environment in its context (its profile has no `environment` section). So far
    /// its version and its environment; the rest of its state comes with the color spaces,
    /// roles and displays, built directly until the YAML reader parses upstream's profile
    /// (3.7d).
    ///
    /// Port of `Config::CreateRaw` (src/OpenColorIO/Config.cpp:74-92, 1127-1133 @ v2.5.2), in
    /// part, with what `OCIOYaml`'s `load` sets from that profile (`setVersion`,
    /// `setEnvironmentMode`, `loadEnvironment`, OCIOYaml.cpp:4454-4455, 5013-5014 @ v2.5.2).
    #[doc(alias = "CreateRaw")]
    pub fn create_raw() -> Arc<Config> {
        let mut config = Config::blank();
        config.minor_version = 0;
        config.set_environment_mode(EnvironmentMode::LoadAll);
        config.load_environment();
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
