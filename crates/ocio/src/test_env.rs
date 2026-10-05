// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The environment of the crate's unit tests. OCIO reads its environment through
//! `ocio_ops::platform::getenv`, and the tests run in parallel threads, so a unit test that
//! gives OCIO an environment (the caches, the optimization flags) holds an [`EnvGuard`]: it
//! takes a lock, and gives OCIO a fixed environment on the test's own thread only
//! (`set_thread_env_provider`). The tests that don't hold it read the process environment, never
//! the environment of a test running beside them (`OCIO_OPTIMIZATION_FLAGS` set to a value
//! OCIO refuses once failed an unrelated test that built a processor at that moment).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

use ocio_ops::platform::{MapEnv, set_thread_env_provider};

static ENVIRONMENT: Mutex<()> = Mutex::new(());

/// Holds the environment lock, with OCIO reading a fixed environment on this thread.
pub(crate) struct EnvGuard {
    _lock: MutexGuard<'static, ()>,
}

impl EnvGuard {
    /// Takes the lock, and gives OCIO an empty environment on this thread.
    pub(crate) fn new() -> EnvGuard {
        let lock = ENVIRONMENT.lock().unwrap_or_else(|e| e.into_inner());
        let guard = EnvGuard { _lock: lock };
        guard.set(&[]);
        guard
    }

    /// Gives OCIO the environment `vars`, and nothing else, on this thread.
    pub(crate) fn set(&self, vars: &[(&str, &str)]) {
        let map: BTreeMap<String, String> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        set_thread_env_provider(Some(Arc::new(MapEnv::from(map))));
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        set_thread_env_provider(None);
    }
}

#[cfg(test)]
mod tests {
    use super::EnvGuard;
    use std::sync::{Arc, Barrier};

    /// OCIO's value of `name`, as text.
    fn getenv(name: &str) -> Option<String> {
        ocio_ops::platform::getenv(name).map(|v| String::from_utf8_lossy(&v).into_owned())
    }

    /// While one test holds an `EnvGuard` with a variable set, a test on another thread still
    /// reads the process environment, before, during and after; and the guarded thread reads
    /// its own environment.
    #[test]
    fn a_guarded_environment_is_seen_on_its_own_thread_only() {
        const NAME: &str = "OCIO_OPTIMIZATION_FLAGS";
        let process = std::env::var_os(NAME).map(|v| v.to_string_lossy().into_owned());
        let set = Arc::new(Barrier::new(2));
        let read = Arc::new(Barrier::new(2));
        let guarded = {
            let (set, read) = (Arc::clone(&set), Arc::clone(&read));
            std::thread::spawn(move || {
                let env = EnvGuard::new();
                env.set(&[(NAME, "not a number")]);
                assert_eq!(getenv(NAME).as_deref(), Some("not a number"));
                set.wait();
                read.wait();
                drop(env);
                assert_eq!(
                    getenv(NAME),
                    std::env::var_os(NAME).map(|v| v.to_string_lossy().into_owned())
                );
            })
        };
        set.wait();
        assert_eq!(getenv(NAME), process, "another test's environment was read");
        read.wait();
        guarded.join().expect("the guarded thread");
        assert_eq!(getenv(NAME), process);
    }
}
