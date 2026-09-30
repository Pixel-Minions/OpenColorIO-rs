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

/// Held by the tests that replace the environment provider (`platform::set_env_provider`),
/// so that they can't change it under each other.
pub(crate) fn environment_lock() -> MutexGuard<'static, ()> {
    ENVIRONMENT.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Runs OCIO's one-time logging initialization, which reads `OCIO_LOGGING_LEVEL`, with an
/// empty environment. It does it once per test process; later calls do nothing.
pub(crate) fn init_logging_with_empty_environment() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let _environment = environment_lock();
        platform::set_env_provider(Some(Arc::new(MapEnv::default())));
        let _ = get_logging_level();
        platform::set_env_provider(None);
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
/// v2.5.2). Not ported yet: `findAndRemove`, `findAllAndRemove` and `print`.
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
}

impl Drop for LogGuard {
    /// Port of `LogGuard::~LogGuard` (UnitTestLogUtils.cpp:47-52 @ v2.5.2).
    fn drop(&mut self) {
        reset_to_default_logging_function();
        set_logging_level(self.log_level);
        self.clear();
    }
}
