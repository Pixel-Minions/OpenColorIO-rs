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
/// "C" locale, i.e. bytes compared as unsigned after ASCII lowercasing.
pub fn strcasecmp(a: &str, b: &str) -> Ordering {
    let lower = |s: &str| {
        s.bytes()
            .map(|c| c.to_ascii_lowercase())
            .collect::<Vec<u8>>()
    };
    lower(a).cmp(&lower(b))
}

/// Port of `Platform::Strncasecmp` (Platform.cpp @ v2.5.2).
pub fn strncasecmp(a: &str, b: &str, n: usize) -> Ordering {
    let lower = |s: &str| {
        s.bytes()
            .take(n)
            .map(|c| c.to_ascii_lowercase())
            .collect::<Vec<u8>>()
    };
    lower(a).cmp(&lower(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injected_environment() {
        let mut vars = BTreeMap::new();
        vars.insert("OCIO_TEST_EMPTY".to_string(), String::new());
        set_env_provider(Some(Arc::new(MapEnv(vars))));
        assert_eq!(getenv("OCIO_TEST_EMPTY"), Some(String::new()));
        assert!(is_env_present("OCIO_TEST_EMPTY"));
        assert_eq!(getenv("OCIO_TEST_MISSING"), None);
        assert_eq!(getenv(""), None);
        set_env_provider(None);
    }

    #[test]
    fn case_insensitive_compare() {
        assert_eq!(strcasecmp("ProcessList", "processlist"), Ordering::Equal);
        assert_eq!(strcasecmp("a", "B"), Ordering::Less);
        assert_eq!(strncasecmp("Info", "INFORMATION", 4), Ordering::Equal);
    }
}
