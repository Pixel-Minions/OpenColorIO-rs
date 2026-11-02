// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Log capture for this crate's unit tests: a port of `LogGuard`
//! (`tests/cpu/UnitTestLogUtils.h`, `UnitTestLogUtils.cpp` @ v2.5.2).
//!
//! Upstream runs its tests one at a time. Rust runs them on parallel threads, and the logging
//! state is global, so three things are added:
//! - a [`LogGuard`] holds [`logging_test_lock`] while it lives. A test that reads the logging
//!   state before it creates its guard takes the lock first; the lock is reentrant, so the
//!   guard can take it again;
//! - a guard keeps only the messages logged on the thread that created it. Messages logged
//!   on other threads, by tests that don't hold the lock, go to stderr, as they would with the
//!   default logging function;
//! - the first time either is used, OCIO's one-time read of `OCIO_LOGGING_LEVEL` happens
//!   in an empty environment ([`init_logging_with_empty_environment`]), so no test depends on
//!   the developer's environment. A test that logs must use one of them, so that no logging
//!   call reads the variable earlier.

use crate::logging::{
    get_logging_level, reset_to_default_logging_function, set_logging_function, set_logging_level,
};
use crate::open_color_types::LoggingLevel;
use crate::platform::{self, MapEnv};
use std::io::Write;
use std::marker::PhantomData;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, Once, PoisonError};
use std::thread::{self, ThreadId};

static ENVIRONMENT: Mutex<()> = Mutex::new(());

/// Held by the tests that replace the process-wide environment provider
/// (`platform::set_env_provider`), so that they can't change it under each other. Other tests
/// read the environment through that provider at the same time, unguarded: such a test gives
/// it only variables that no other test reads, over the process environment.
pub(crate) fn environment_lock() -> MutexGuard<'static, ()> {
    ENVIRONMENT.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Runs OCIO's one-time logging initialization, which reads `OCIO_LOGGING_LEVEL`, with an
/// empty environment. It does it once per test process; later calls do nothing. The empty
/// environment is this thread's only (`platform::set_thread_env_provider`): the tests running
/// beside it never read it.
pub(crate) fn init_logging_with_empty_environment() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        platform::set_thread_env_provider(Some(Arc::new(MapEnv::default())));
        let _ = get_logging_level();
        platform::set_thread_env_provider(None);
    });
}

/// Which thread holds the lock, and how many times.
struct Owner {
    thread: Option<ThreadId>,
    depth: usize,
}

static OWNER: Mutex<Owner> = Mutex::new(Owner {
    thread: None,
    depth: 0,
});
static RELEASED: Condvar = Condvar::new();

/// The lock of the tests that change or read the global logging state. It is released when
/// the last guard of its thread is dropped.
pub(crate) struct LoggingTestLock {
    // Released on the thread that took it.
    _not_send: PhantomData<*const ()>,
}

/// Waits until no other thread holds the lock, and takes it; initializes logging first
/// ([`init_logging_with_empty_environment`]).
pub(crate) fn logging_test_lock() -> LoggingTestLock {
    init_logging_with_empty_environment();
    let me = thread::current().id();
    let mut owner = OWNER.lock().unwrap_or_else(PoisonError::into_inner);
    loop {
        match owner.thread {
            None => {
                owner.thread = Some(me);
                owner.depth = 1;
                break;
            }
            Some(thread) if thread == me => {
                owner.depth += 1;
                break;
            }
            Some(_) => {
                owner = RELEASED.wait(owner).unwrap_or_else(PoisonError::into_inner);
            }
        }
    }
    LoggingTestLock {
        _not_send: PhantomData,
    }
}

impl Drop for LoggingTestLock {
    fn drop(&mut self) {
        let mut owner = OWNER.lock().unwrap_or_else(PoisonError::into_inner);
        owner.depth -= 1;
        if owner.depth == 0 {
            owner.thread = None;
            RELEASED.notify_all();
        }
    }
}

/// Traps the log messages of its thread while keeping the original logging settings: they
/// are restored when it is dropped.
///
/// Only the thread that creates the guard is trapped. Messages logged on any other thread go
/// to stderr, including those of threads the test itself spawns: such a test must collect
/// them with a logging function of its own.
///
/// Port of `LogGuard` (tests/cpu/UnitTestLogUtils.h:12-34, UnitTestLogUtils.cpp:17-67 @
/// v2.5.2). The check-and-mute message helpers live with their callers in ocio's
/// test_env and merge-config tests.
pub(crate) struct LogGuard {
    log_level: LoggingLevel,
    output: Arc<Mutex<Vec<u8>>>,
    // Dropped last, after the settings are restored.
    _lock: LoggingTestLock,
}

impl LogGuard {
    /// Sets the level to `Debug` while the guard lives.
    ///
    /// Port of `LogGuard::LogGuard()` (UnitTestLogUtils.cpp:33-38 @ v2.5.2).
    pub(crate) fn new() -> Self {
        Self::with_level(LoggingLevel::Debug)
    }

    /// Sets the level to `level` while the guard lives.
    ///
    /// Port of `LogGuard::LogGuard(LoggingLevel)` (UnitTestLogUtils.cpp:40-45 @ v2.5.2).
    pub(crate) fn with_level(level: LoggingLevel) -> Self {
        let lock = logging_test_lock();
        let log_level = get_logging_level();
        set_logging_level(level);

        let output = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&output);
        let owner = thread::current().id();
        set_logging_function(Some(Arc::new(move |message: &[u8]| {
            if thread::current().id() == owner {
                sink.lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .extend_from_slice(message);
            } else {
                let _ = std::io::stderr().write_all(message);
            }
        })))
        .expect("the function is not null");

        LogGuard {
            log_level,
            output,
            _lock: lock,
        }
    }

    /// Everything logged so far, in order.
    ///
    /// Port of `LogGuard::output` (UnitTestLogUtils.cpp:54-57 @ v2.5.2).
    pub(crate) fn output(&self) -> Vec<u8> {
        self.output
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Port of `LogGuard::clear` (UnitTestLogUtils.cpp:59-62 @ v2.5.2).
    pub(crate) fn clear(&self) {
        self.output
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }

    /// Port of `LogGuard::empty` (UnitTestLogUtils.cpp:64-67 @ v2.5.2).
    pub(crate) fn empty(&self) -> bool {
        self.output
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_empty()
    }

    /// Port of `LogGuard::findAndRemove` (UnitTestLogUtils.cpp:71-86 @ v2.5.2).
    /// The escaped literal followed by one or more CR/LF bytes is a byte search.
    pub(crate) fn find_and_remove(&self, line: &[u8]) -> bool {
        let mut output = self.output.lock().unwrap_or_else(PoisonError::into_inner);
        let is_break = |b: &u8| matches!(*b, b'\r' | b'\n');
        let Some(start) = (0..output.len().saturating_sub(line.len())).find(|&i| {
            output[i..].starts_with(line) && output.get(i + line.len()).is_some_and(is_break)
        }) else {
            return false;
        };
        let mut end = start + line.len();
        while output.get(end).is_some_and(is_break) {
            end += 1;
        }
        output.drain(start..end);
        true
    }

    /// Port of `LogGuard::findAllAndRemove` (UnitTestLogUtils.cpp:88-101 @ v2.5.2).
    /// Restart after each single removal, including matches formed at its boundary.
    pub(crate) fn find_all_and_remove(&self, pattern: &[u8]) -> bool {
        use crate::std_regex::{Library, Regex, test_log_search};
        let library = if cfg!(windows) {
            Library::Msvc
        } else {
            Library::Libstdcxx
        };
        let regex = Regex::new(pattern, library).expect("the upstream test's regular expression");
        let mut output = self.output.lock().unwrap_or_else(PoisonError::into_inner);
        let mut found = false;
        while let Some((start, end)) =
            test_log_search(&output, &regex).expect("the upstream test's log search")
        {
            output.drain(start..end);
            found = true;
        }
        found
    }

    /// Port of `LogGuard::print` (UnitTestLogUtils.cpp:103-106 @ v2.5.2).
    pub(crate) fn print(&self) {
        self.write_output(&mut std::io::stdout());
    }

    fn write_output(&self, output: &mut impl Write) {
        // The default C++ ostream records write failure without throwing.
        let _ = output.write_all(&self.output());
    }
}

/// Port of `MuteLogging` (UnitTestLogUtils.cpp:109-116 @ v2.5.2).
pub(crate) struct MuteLogging {
    _lock: LoggingTestLock,
}

impl MuteLogging {
    pub(crate) fn new() -> Self {
        let lock = logging_test_lock();
        set_logging_function(Some(Arc::new(|_| {}))).expect("the function is not null");
        Self { _lock: lock }
    }
}

impl Drop for MuteLogging {
    fn drop(&mut self) {
        reset_to_default_logging_function();
    }
}

impl Drop for LogGuard {
    /// Port of `LogGuard::~LogGuard` (UnitTestLogUtils.cpp:47-52 @ v2.5.2).
    fn drop(&mut self) {
        reset_to_default_logging_function();
        set_logging_level(self.log_level);
        self.clear();
    }
}

#[cfg(test)]
mod helper_controls {
    use super::*;

    #[test]
    fn literal_line_removal_preserves_the_other_input_bytes() {
        let guard = LogGuard::new();
        guard
            .output
            .lock()
            .unwrap()
            .extend_from_slice(b"keep\n[.*]\r\n\n[.*]\n");
        assert!(guard.find_and_remove(b"[.*]"));
        assert_eq!(guard.output(), b"keep\n[.*]\n");
        assert!(guard.find_and_remove(b"[.*]"));
        assert_eq!(guard.output(), b"keep\n");
        assert!(!guard.find_and_remove(b"[.*]"));
    }

    #[test]
    fn regex_removal_restarts_after_one_match() {
        let guard = LogGuard::new();
        // Protocol input: erasing x-space first joins a and y. The word boundary
        // before y disappears, so erasing all original matches at once is wrong.
        guard.output.lock().unwrap().extend_from_slice(b"ax y");
        assert!(guard.find_all_and_remove(br"x |\by"));
        assert_eq!(guard.output(), b"ay");
        assert!(!guard.find_all_and_remove(br"x |\by"));
    }

    #[test]
    fn diagnostics_copy_raw_input_bytes() {
        let guard = LogGuard::new();
        guard
            .output
            .lock()
            .unwrap()
            .extend_from_slice(b"input\0\xff\n");
        let mut sink = Vec::new();
        guard.write_output(&mut sink);
        assert_eq!(sink, b"input\0\xff\n");
        guard.clear();
        guard.print();
    }

    #[test]
    fn mute_keeps_the_level_and_suppresses_the_callback() {
        let guard = LogGuard::with_level(LoggingLevel::Info);
        {
            let _mute = MuteLogging::new();
            crate::logging::log_info("Dummy message");
            assert!(guard.empty());
            assert_eq!(get_logging_level(), LoggingLevel::Info);
        }
    }
}
