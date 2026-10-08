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
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;

use ocio_ops::logging::{reset_to_default_logging_function, set_logging_function};
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

/// Serializes the tests that replace the logging function.
static LOGGING: Mutex<()> = Mutex::new(());

/// Runs `f`, and gives the messages OCIO logged on this thread meanwhile, each as the logging
/// function receives it (with its prefix and line feed). Messages of other threads go to
/// stderr, as with the default logging function.
pub(crate) fn capture_log<T>(f: impl FnOnce() -> T) -> (T, Vec<Vec<u8>>) {
    let _lock = LOGGING.lock().unwrap_or_else(PoisonError::into_inner);
    let me = thread::current().id();
    let messages = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&messages);
    set_logging_function(Some(Arc::new(move |message: &[u8]| {
        if thread::current().id() == me {
            sink.lock().unwrap().push(message.to_vec());
        } else {
            eprint!("{}", String::from_utf8_lossy(message));
        }
    })))
    .unwrap();
    let out = f();
    reset_to_default_logging_function();
    let log = messages.lock().unwrap().clone();
    (out, log)
}

/// Removes from `output` the first `line` that a line break follows, with the line breaks
/// after it; whether it found one.
///
/// Port of `LogGuard::findAndRemove` (tests/cpu/UnitTestLogUtils.cpp:71-86 @ v2.5.2): the
/// escaped `line` and `[\r\n]+` as a regular expression, which matches the text itself.
pub(crate) fn find_and_remove(output: &mut Vec<u8>, line: &str) -> bool {
    let line = line.as_bytes();
    let is_break = |b: &u8| *b == b'\r' || *b == b'\n';
    let found = (0..output.len().saturating_sub(line.len())).find(|&i| {
        output[i..].starts_with(line) && output.get(i + line.len()).is_some_and(is_break)
    });
    match found {
        Some(start) => {
            let mut end = start + line.len();
            while output.get(end).is_some_and(is_break) {
                end += 1;
            }
            output.drain(start..end);
            true
        }
        None => false,
    }
}

/// Port of `checkAndMuteSceneLinearRoleError` (tests/cpu/UnitTestLogUtils.cpp:118-123 @
/// v2.5.2).
pub(crate) fn check_and_mute_scene_linear_role_error(output: &mut Vec<u8>) -> bool {
    find_and_remove(
        output,
        "[OpenColorIO Error]: The scene_linear role is required for a config version 2.2 or \
         higher.",
    )
}

/// Port of `checkAndMuteCompositingLogRoleError` (tests/cpu/UnitTestLogUtils.cpp:125-130 @
/// v2.5.2).
pub(crate) fn check_and_mute_compositing_log_role_error(output: &mut Vec<u8>) -> bool {
    find_and_remove(
        output,
        "[OpenColorIO Error]: The compositing_log role is required for a config version 2.2 or \
         higher.",
    )
}

/// Port of `checkAndMuteColorTimingRoleError` (tests/cpu/UnitTestLogUtils.cpp:132-137 @
/// v2.5.2).
pub(crate) fn check_and_mute_color_timing_role_error(output: &mut Vec<u8>) -> bool {
    find_and_remove(
        output,
        "[OpenColorIO Error]: The color_timing role is required for a config version 2.2 or \
         higher.",
    )
}

/// Port of `checkAndMuteAcesInterchangeRoleError` (tests/cpu/UnitTestLogUtils.cpp:139-145 @
/// v2.5.2).
pub(crate) fn check_and_mute_aces_interchange_role_error(output: &mut Vec<u8>) -> bool {
    find_and_remove(
        output,
        "[OpenColorIO Error]: The aces_interchange role is required when there are \
         scene-referred color spaces and the config version is 2.2 or higher.",
    )
}

#[cfg(test)]
mod tests {
    use super::EnvGuard;
    use std::sync::mpsc;
    use std::time::Duration;

    /// OCIO's value of `name`, as text.
    fn getenv(name: &str) -> Option<String> {
        ocio_ops::platform::getenv(name).map(|v| String::from_utf8_lossy(&v).into_owned())
    }

    /// While one test holds an `EnvGuard` with a variable set, a test on another thread still
    /// reads the process environment, before, during and after; and the guarded thread reads
    /// its own environment. The threads pass what they read over channels and the checks run
    /// at the end, so a thread that fails can't leave the other waiting for it.
    #[test]
    fn a_guarded_environment_is_seen_on_its_own_thread_only() {
        const NAME: &str = "OCIO_OPTIMIZATION_FLAGS";
        const WAIT: Duration = Duration::from_secs(60);
        let process = || std::env::var_os(NAME).map(|v| v.to_string_lossy().into_owned());
        let before = getenv(NAME);
        let (read_tx, read_rx) = mpsc::channel();
        let (go_tx, go_rx) = mpsc::channel::<()>();
        let guarded = std::thread::spawn(move || {
            let env = EnvGuard::new();
            env.set(&[(NAME, "not a number")]);
            read_tx.send(getenv(NAME)).unwrap();
            let _ = go_rx.recv_timeout(WAIT);
            drop(env);
            read_tx.send(getenv(NAME)).unwrap();
        });
        let guarded_reads = read_rx
            .recv_timeout(WAIT)
            .expect("the guarded thread set its environment");
        let during = getenv(NAME);
        go_tx.send(()).expect("the guarded thread waits");
        let guarded_after = read_rx
            .recv_timeout(WAIT)
            .expect("the guarded thread dropped its guard");
        guarded.join().expect("the guarded thread");
        assert_eq!(guarded_reads.as_deref(), Some("not a number"));
        assert_eq!(guarded_after, process());
        assert_eq!(before, process());
        assert_eq!(during, process(), "another test's environment was read");
        assert_eq!(getenv(NAME), process());
    }
}
