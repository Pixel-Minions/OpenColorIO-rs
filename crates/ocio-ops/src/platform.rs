// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Platform helpers: a port of `src/OpenColorIO/Platform.cpp` @ v2.5.2.
//!
//! Environment access goes through an injectable [`EnvProvider`] (PLAN.md §9), so tests can
//! give OCIO an environment without mutating the process environment.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

/// Where OCIO reads environment variables from.
pub trait EnvProvider: Send + Sync {
    /// The value of `name`, or `None` if it is not set. A variable that is set to the empty
    /// string is `Some("")`.
    fn var(&self, name: &str) -> Option<String>;
}

/// The process environment.
#[derive(Debug, Default, Clone, Copy)]
pub struct ProcessEnv;

impl EnvProvider for ProcessEnv {
    fn var(&self, name: &str) -> Option<String> {
        std::env::var_os(name).map(|v| v.to_string_lossy().into_owned())
    }
}

/// A fixed environment, for tests.
#[derive(Debug, Default, Clone)]
pub struct MapEnv(pub BTreeMap<String, String>);

impl EnvProvider for MapEnv {
    fn var(&self, name: &str) -> Option<String> {
        self.0.get(name).cloned()
    }
}

static ENV: RwLock<Option<Arc<dyn EnvProvider>>> = RwLock::new(None);

/// Replaces the environment OCIO reads (`None` restores the process environment).
pub fn set_env_provider(provider: Option<Arc<dyn EnvProvider>>) {
    *ENV.write().unwrap_or_else(|e| e.into_inner()) = provider;
}

fn provider() -> Arc<dyn EnvProvider> {
    ENV.read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_else(|| Arc::new(ProcessEnv))
}

/// Port of `Platform::Getenv` (Platform.cpp @ v2.5.2): `None` when `name` is empty or the
/// variable does not exist; otherwise its value, which may be empty.
pub fn getenv(name: &str) -> Option<String> {
    if name.is_empty() {
        return None;
    }
    provider().var(name)
}

/// Port of `Platform::isEnvPresent` (Platform.cpp @ v2.5.2).
pub fn is_env_present(name: &str) -> bool {
    getenv(name).is_some()
}

/// Port of `Platform::Strcasecmp` (Platform.cpp @ v2.5.2): `_stricmp` / `strcasecmp` in the
/// classic "C" locale, i.e. bytes compared as unsigned after folding `A`-`Z` to lowercase. The
/// C functions take `const char *`, so each side ends at its first NUL. (Upstream throws for
/// a null pointer; a slice is never null.)
///
/// Deviation D-4 (`docs/deviations.md`): upstream folds case by the process's C locale, and
/// Python sets that locale at startup, so under Python the Windows wheel also folds non-ASCII
/// bytes by the ANSI code page. The port folds `A`-`Z` only, whatever the locale.
/// `tests/platform_crt.rs` checks this against the C runtime in the "C" locale.
pub fn strcasecmp(a: impl AsRef<[u8]>, b: impl AsRef<[u8]>) -> Ordering {
    strncasecmp(a, b, usize::MAX)
}

/// Port of `Platform::Strncasecmp` (Platform.cpp @ v2.5.2): [`strcasecmp`] on at most the
/// first `n` bytes of each side, as `_strnicmp` / `strncasecmp` in the classic "C" locale
/// (deviation D-4).
pub fn strncasecmp(a: impl AsRef<[u8]>, b: impl AsRef<[u8]>, n: usize) -> Ordering {
    let lower = |s: &[u8]| {
        crate::utils::string_utils::c_str(s)
            .iter()
            .take(n)
            .map(|c| c.to_ascii_lowercase())
            .collect::<Vec<u8>>()
    };
    lower(a.as_ref()).cmp(&lower(b.as_ref()))
}

#[cfg(test)]
#[path = "platform_tests.rs"]
mod tests;
