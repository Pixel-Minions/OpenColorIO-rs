// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The context: a port of `src/OpenColorIO/Context.cpp` @ v2.5.2. It holds what a config
//! resolves file paths and variables with: search paths, a working directory, the context
//! variables and how the environment fills them, and the I/O proxy.
//!
//! Strings are bytes, as upstream's C strings: each argument ends at its first NUL.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex};

use ocio_ops::hash_utils::cache_id_hash;
use ocio_ops::open_color_types::EnvironmentMode;
use ocio_ops::parse_utils::environment_mode_to_string;
use ocio_ops::utils::string_utils::{c_str, split};

use crate::config_io_proxy::ConfigIoProxy;
use crate::context_variable_utils::{
    EnvMap, EnvMapKey, UsedEnvs, load_environment, resolve_context_variables,
};

/// A resolved string, and the context variables its resolution used.
type Resolved = (Vec<u8>, UsedEnvs);

/// What a context computes and keeps: its cache ID, and the strings and file paths it resolved.
///
/// Port of `Context::Impl`'s `m_cacheID`, `m_resultsStringCache` and `m_resultsFilepathCache`
/// (src/OpenColorIO/Context.cpp:46-53 @ v2.5.2).
#[derive(Debug, Clone, Default)]
struct Caches {
    cache_id: String,
    strings: BTreeMap<Vec<u8>, Resolved>,
    filepaths: BTreeMap<Vec<u8>, Resolved>,
}

impl Caches {
    /// Port of `Context::Impl::clearCaches` (Context.cpp:123-128 @ v2.5.2).
    fn clear(&mut self) {
        self.strings.clear();
        self.filepaths.clear();
        self.cache_id.clear();
    }
}

/// The context in which a config resolves file paths and context variables.
///
/// Port of `Context` (include/OpenColorIO/OpenColorIO.h:3865-3951, src/OpenColorIO/Context.cpp
/// @ v2.5.2).
pub struct Context {
    /// `m_searchPaths`.
    search_paths: Vec<Vec<u8>>,
    /// `m_searchPath`: the search paths as one string, as set or joined with `:`.
    search_path: Vec<u8>,
    /// `m_workingDir`.
    working_dir: Vec<u8>,
    /// `m_envmode`.
    env_mode: EnvironmentMode,
    /// `m_envMap`.
    env_map: EnvMap,
    /// The caches, under `m_resultsCacheMutex`.
    caches: Mutex<Caches>,
    /// `m_configIOProxy`.
    config_io_proxy: Option<Arc<dyn ConfigIoProxy>>,
}

impl Default for Context {
    fn default() -> Context {
        Context::new()
    }
}

impl Clone for Context {
    /// Upstream's `createEditableCopy`, which copies everything but the environment mode: the
    /// copy's is the default, `LoadPredefined` (docs/improvements.md, I-111). The caches are
    /// copied, the cache ID included.
    ///
    /// Port of `Context::createEditableCopy` and `Context::Impl::operator=`
    /// (src/OpenColorIO/Context.cpp:60-80, 156-161 @ v2.5.2).
    fn clone(&self) -> Context {
        Context {
            search_paths: self.search_paths.clone(),
            search_path: self.search_path.clone(),
            working_dir: self.working_dir.clone(),
            env_mode: EnvironmentMode::LoadPredefined,
            env_map: self.env_map.clone(),
            caches: Mutex::new(self.lock().clone()),
            config_io_proxy: self.config_io_proxy.clone(),
        }
    }
}

impl fmt::Debug for Context {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&String::from_utf8_lossy(&self.to_bytes()))
    }
}

/// An index into a list of `len` entries, as upstream's `int` getters check it.
fn index_of(index: i32, len: usize) -> Option<usize> {
    usize::try_from(index).ok().filter(|&i| i < len)
}

impl Context {
    /// An empty context: no search path, no working directory, no variable, the environment
    /// mode `LoadPredefined`.
    ///
    /// Port of `Context::Create` (src/OpenColorIO/Context.cpp:133-136 @ v2.5.2).
    #[doc(alias = "Create")]
    pub fn new() -> Context {
        Context {
            search_paths: Vec::new(),
            search_path: Vec::new(),
            working_dir: Vec::new(),
            env_mode: EnvironmentMode::LoadPredefined,
            env_map: EnvMap::new(),
            caches: Mutex::new(Caches::default()),
            config_io_proxy: None,
        }
    }

    fn caches_mut(&mut self) -> &mut Caches {
        self.caches.get_mut().unwrap_or_else(|e| e.into_inner())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Caches> {
        self.caches.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The context's cache ID: the hash of its search paths, working directory, environment
    /// mode and variables.
    ///
    /// Port of `Context::getCacheID` (Context.cpp:163-193 @ v2.5.2).
    #[doc(alias = "getCacheID")]
    pub fn cache_id(&self) -> String {
        let mut caches = self.lock();
        if caches.cache_id.is_empty() {
            let mut cacheid = Vec::new();
            if !self.search_paths.is_empty() {
                cacheid.extend_from_slice(b"Search Path ");
                for path in &self.search_paths {
                    cacheid.extend_from_slice(path);
                    cacheid.push(b' ');
                }
            }
            cacheid.extend_from_slice(b"Working Dir ");
            cacheid.extend_from_slice(&self.working_dir);
            cacheid.push(b' ');
            cacheid.extend_from_slice(b"Environment Mode ");
            // The enum, written as its integer.
            cacheid.extend_from_slice((self.env_mode as i32).to_string().as_bytes());
            cacheid.push(b' ');
            for (name, value) in &self.env_map {
                cacheid.extend_from_slice(&name.0);
                cacheid.push(b'=');
                cacheid.extend_from_slice(value);
                cacheid.push(b' ');
            }
            caches.cache_id = cache_id_hash(&cacheid);
        }
        caches.cache_id.clone()
    }

    /// Sets the search paths from `path`, split on `:` (even on Windows).
    ///
    /// Port of `Context::setSearchPath` (Context.cpp:195-205 @ v2.5.2).
    #[doc(alias = "setSearchPath")]
    pub fn set_search_path(&mut self, path: impl AsRef<[u8]>) {
        let path = c_str(path.as_ref());
        self.search_paths = split(path, b':');
        self.search_path = path.to_vec();
        self.caches_mut().clear();
    }

    /// The search paths as one string: as set, or joined with `:`.
    ///
    /// Port of `Context::getSearchPath()` (Context.cpp:207-210 @ v2.5.2).
    #[doc(alias = "getSearchPath")]
    pub fn search_path(&self) -> &[u8] {
        &self.search_path
    }

    /// Port of `Context::getNumSearchPaths` (Context.cpp:212-215 @ v2.5.2).
    #[doc(alias = "getNumSearchPaths")]
    pub fn num_search_paths(&self) -> i32 {
        i32::try_from(self.search_paths.len()).unwrap_or(i32::MAX)
    }

    /// The search path at `index`, or empty when there is none.
    ///
    /// Port of `Context::getSearchPath(int)` (Context.cpp:217-221 @ v2.5.2).
    #[doc(alias = "getSearchPath")]
    pub fn search_path_with_index(&self, index: i32) -> &[u8] {
        index_of(index, self.search_paths.len()).map_or(&[], |i| &self.search_paths[i])
    }

    /// Port of `Context::clearSearchPaths` (Context.cpp:223-230 @ v2.5.2).
    #[doc(alias = "clearSearchPaths")]
    pub fn clear_search_paths(&mut self) {
        self.search_path.clear();
        self.search_paths.clear();
        self.caches_mut().clear();
    }

    /// Adds a search path; an empty one is ignored.
    ///
    /// Port of `Context::addSearchPath` (Context.cpp:232-250 @ v2.5.2).
    #[doc(alias = "addSearchPath")]
    pub fn add_search_path(&mut self, path: impl AsRef<[u8]>) {
        let path = c_str(path.as_ref());
        if !path.is_empty() {
            self.search_paths.push(path.to_vec());
            self.caches_mut().clear();
            if !self.search_path.is_empty() {
                self.search_path.push(b':');
            }
            self.search_path.extend_from_slice(path);
        }
    }

    /// Port of `Context::setWorkingDir` (Context.cpp:252-258 @ v2.5.2).
    #[doc(alias = "setWorkingDir")]
    pub fn set_working_dir(&mut self, dirname: impl AsRef<[u8]>) {
        self.working_dir = c_str(dirname.as_ref()).to_vec();
        self.caches_mut().clear();
    }

    /// Port of `Context::getWorkingDir` (Context.cpp:260-263 @ v2.5.2).
    #[doc(alias = "getWorkingDir")]
    pub fn working_dir(&self) -> &[u8] {
        &self.working_dir
    }

    /// Port of `Context::setEnvironmentMode` (Context.cpp:265-272 @ v2.5.2).
    #[doc(alias = "setEnvironmentMode")]
    pub fn set_environment_mode(&mut self, mode: EnvironmentMode) {
        self.env_mode = mode;
        self.caches_mut().clear();
    }

    /// Port of `Context::getEnvironmentMode` (Context.cpp:274-277 @ v2.5.2).
    #[doc(alias = "getEnvironmentMode")]
    pub fn environment_mode(&self) -> EnvironmentMode {
        self.env_mode
    }

    /// Loads the environment: in `LoadAll` mode, adds every variable of the environment (the
    /// context's own keep their values); otherwise, updates the values of the context's own
    /// variables from the environment.
    ///
    /// Port of `Context::loadEnvironment` (Context.cpp:279-286 @ v2.5.2).
    #[doc(alias = "loadEnvironment")]
    pub fn load_environment(&mut self) {
        let update = self.env_mode != EnvironmentMode::LoadAll;
        load_environment(&mut self.env_map, update);
        self.lock().clear();
    }

    /// Sets the context variable `name` to `value`, or removes it for `None` (upstream's null
    /// pointer). An empty name is ignored; setting a variable to its value keeps the caches.
    ///
    /// Port of `Context::setStringVar` (Context.cpp:288-329 @ v2.5.2).
    #[doc(alias = "setStringVar")]
    pub fn set_string_var(&mut self, name: impl AsRef<[u8]>, value: Option<&[u8]>) {
        let name = c_str(name.as_ref());
        if name.is_empty() {
            return;
        }
        let key = EnvMapKey(name.to_vec());
        match value {
            Some(value) => {
                let value = c_str(value);
                match self.env_map.get_mut(&key) {
                    Some(current) => {
                        // strcmp: the stored value up to its first NUL.
                        if c_str(current) != value {
                            *current = value.to_vec();
                        } else {
                            // Do not flush the cache because nothing changed.
                            return;
                        }
                    }
                    None => {
                        self.env_map.insert(key, value.to_vec());
                    }
                }
            }
            // If a null value is specified, erase it.
            None => {
                self.env_map.remove(&key);
            }
        }
        self.caches_mut().clear();
    }

    /// The value of the context variable `name`, or empty.
    ///
    /// Port of `Context::getStringVar` (Context.cpp:331-345 @ v2.5.2).
    #[doc(alias = "getStringVar")]
    pub fn string_var(&self, name: impl AsRef<[u8]>) -> &[u8] {
        let name = c_str(name.as_ref());
        if name.is_empty() {
            return &[];
        }
        self.env_map
            .get(&EnvMapKey(name.to_vec()))
            .map_or(&[], Vec::as_slice)
    }

    /// Port of `Context::getNumStringVars` (Context.cpp:347-350 @ v2.5.2).
    #[doc(alias = "getNumStringVars")]
    pub fn num_string_vars(&self) -> i32 {
        i32::try_from(self.env_map.len()).unwrap_or(i32::MAX)
    }

    /// The name of the variable at `index`, in the variables' order (longest name first), or
    /// empty.
    ///
    /// Port of `Context::getStringVarNameByIndex` (Context.cpp:352-361 @ v2.5.2).
    #[doc(alias = "getStringVarNameByIndex")]
    pub fn string_var_name_by_index(&self, index: i32) -> &[u8] {
        index_of(index, self.env_map.len())
            .and_then(|i| self.env_map.keys().nth(i))
            .map_or(&[], |k| k.0.as_slice())
    }

    /// The value of the variable at `index`, or empty.
    ///
    /// Port of `Context::getStringVarByIndex` (Context.cpp:363-372 @ v2.5.2).
    #[doc(alias = "getStringVarByIndex")]
    pub fn string_var_by_index(&self, index: i32) -> &[u8] {
        index_of(index, self.env_map.len())
            .and_then(|i| self.env_map.values().nth(i))
            .map_or(&[], Vec::as_slice)
    }

    /// Sets each variable of `ctx` in this context.
    ///
    /// Port of `Context::addStringVars` (Context.cpp:374-380 @ v2.5.2).
    #[doc(alias = "addStringVars")]
    pub fn add_string_vars(&mut self, ctx: &Context) {
        for (name, value) in &ctx.env_map {
            self.set_string_var(&name.0, Some(value));
        }
    }

    /// Removes every variable. (Upstream doesn't clear the caches here.)
    ///
    /// Port of `Context::clearStringVars` (Context.cpp:382-385 @ v2.5.2).
    #[doc(alias = "clearStringVars")]
    pub fn clear_string_vars(&mut self) {
        self.env_map.clear();
    }

    /// Port of `Context::Impl::resolveStringVar` (Context.cpp:83-121 @ v2.5.2).
    fn resolve_string_var_impl(
        &self,
        caches: &mut Caches,
        string: &[u8],
        used_context_vars: Option<&mut Context>,
    ) -> Vec<u8> {
        if string.is_empty() {
            return Vec::new();
        }
        if let Some((resolved, used)) = caches.strings.get(string) {
            if let Some(used_vars) = used_context_vars {
                // Collect the used context variables.
                for (name, value) in used {
                    used_vars.set_string_var(name, Some(value));
                }
            }
            return resolved.clone();
        }
        // Search some context variables to replace.
        let mut envs = UsedEnvs::new();
        let resolved = resolve_context_variables(string, &self.env_map, &mut envs);
        if let Some(used_vars) = used_context_vars {
            // Record all the used context variables.
            for (name, value) in &envs {
                used_vars.set_string_var(name, Some(value));
            }
        }
        caches
            .strings
            .insert(string.to_vec(), (resolved.clone(), envs));
        resolved
    }

    /// `string` with its context variables replaced by their values.
    ///
    /// Port of `Context::resolveStringVar(const char *)` (Context.cpp:387-394 @ v2.5.2).
    #[doc(alias = "resolveStringVar")]
    pub fn resolve_string_var(&self, string: impl AsRef<[u8]>) -> Vec<u8> {
        let mut caches = self.lock();
        self.resolve_string_var_impl(&mut caches, c_str(string.as_ref()), None)
    }

    /// [`resolve_string_var`](Self::resolve_string_var), adding the variables it used to
    /// `used_context_vars`.
    ///
    /// Port of `Context::resolveStringVar(const char *, ContextRcPtr &)` (Context.cpp:396-401 @
    /// v2.5.2).
    #[doc(alias = "resolveStringVar")]
    pub fn resolve_string_var_with_used(
        &self,
        string: impl AsRef<[u8]>,
        used_context_vars: &mut Context,
    ) -> Vec<u8> {
        let mut caches = self.lock();
        self.resolve_string_var_impl(&mut caches, c_str(string.as_ref()), Some(used_context_vars))
    }

    /// Port of `Context::setConfigIOProxy` (Context.cpp:518-521 @ v2.5.2).
    #[doc(alias = "setConfigIOProxy")]
    pub fn set_config_io_proxy(&mut self, ciop: Option<Arc<dyn ConfigIoProxy>>) {
        self.config_io_proxy = ciop;
    }

    /// Port of `Context::getConfigIOProxy` (Context.cpp:523-526 @ v2.5.2).
    #[doc(alias = "getConfigIOProxy")]
    pub fn config_io_proxy(&self) -> Option<&Arc<dyn ConfigIoProxy>> {
        self.config_io_proxy.as_ref()
    }

    /// Writes the context as upstream's `operator<<` does, the text of Python's `repr()`:
    /// `<Context searchPath=["a", "b"], workingDir=..., environmentMode=..., environment=` and
    /// a `\n    NAME: value` line per variable, then `>`.
    ///
    /// Port of `operator<<(std::ostream &, const Context &)` (Context.cpp:528-551 @ v2.5.2).
    pub fn write_to(&self, os: &mut Vec<u8>) {
        os.extend_from_slice(b"<Context");
        os.extend_from_slice(b" searchPath=[");
        let num = self.num_search_paths();
        for i in 0..num {
            os.push(b'"');
            os.extend_from_slice(self.search_path_with_index(i));
            os.push(b'"');
            if i != num - 1 {
                os.extend_from_slice(b", ");
            }
        }
        os.extend_from_slice(b"], workingDir=");
        os.extend_from_slice(c_str(self.working_dir()));
        os.extend_from_slice(b", environmentMode=");
        os.extend_from_slice(environment_mode_to_string(self.environment_mode()).as_bytes());
        os.extend_from_slice(b", environment=");
        for i in 0..self.num_string_vars() {
            let key = self.string_var_name_by_index(i);
            os.extend_from_slice(b"\n    ");
            os.extend_from_slice(c_str(key));
            os.extend_from_slice(b": ");
            os.extend_from_slice(c_str(self.string_var(key)));
        }
        os.push(b'>');
    }

    /// The text [`write_to`](Self::write_to) writes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut os = Vec::new();
        self.write_to(&mut os);
        os
    }
}

impl fmt::Display for Context {
    /// [`Context::write_to`]'s text; bytes that aren't UTF-8 become U+FFFD.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&String::from_utf8_lossy(&self.to_bytes()))
    }
}

#[cfg(test)]
#[path = "context_tests.rs"]
mod tests;
