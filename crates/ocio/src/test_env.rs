// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The environment of the crate's unit tests. OCIO reads its environment through one global
//! provider (`ocio_ops::platform::set_env_provider`), and the tests run in parallel, so every
//! unit test that reads the environment (the caches, the optimization flags) holds an
//! [`EnvGuard`]: it takes a lock, gives OCIO a fixed environment, and restores the process
//! environment when it drops.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

use ocio_ops::platform::{MapEnv, set_env_provider};

static ENVIRONMENT: Mutex<()> = Mutex::new(());

/// Holds the environment lock, with OCIO reading a fixed environment.
pub(crate) struct EnvGuard {
    _lock: MutexGuard<'static, ()>,
}

impl EnvGuard {
    /// Takes the lock, and gives OCIO an empty environment.
    pub(crate) fn new() -> EnvGuard {
        let lock = ENVIRONMENT.lock().unwrap_or_else(|e| e.into_inner());
        let guard = EnvGuard { _lock: lock };
        guard.set(&[]);
        guard
    }

    /// Gives OCIO the environment `vars`, and nothing else.
    pub(crate) fn set(&self, vars: &[(&str, &str)]) {
        let map: BTreeMap<String, String> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        set_env_provider(Some(Arc::new(MapEnv::from(map))));
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        set_env_provider(None);
    }
}
