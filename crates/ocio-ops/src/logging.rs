// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! OCIO's log: a port of `src/OpenColorIO/Logging.h` and `Logging.cpp` @ v2.5.2, with the
//! public functions of `include/OpenColorIO/OpenColorIO.h:155-177`.
//!
//! A message is logged when the global level lets it through. It is cut into lines (after
//! trailing white space is removed), and the logging function receives each line with the
//! level's prefix, such as `[OpenColorIO Warning]: `, and a `'\n'`. The default logging
//! function writes to stderr.
//!
//! The level comes from `OCIO_LOGGING_LEVEL`, read once, through [`platform::getenv`], the
//! first time anything here needs the level; when that variable is set, [`set_logging_level`]
//! is ignored.
//!
//! Deviation D-3 (`docs/deviations.md`): upstream calls the logging function while it holds
//! its global lock, so a logging function that logs, or that waits for a lock held by a
//! thread that is logging (such as Python's GIL), deadlocks. Here the logging function is
//! called after the lock is released.

use crate::open_color_types::{LoggingLevel, logging_level_from_string};
use crate::platform;
use crate::utils::string_utils::{c_str, right_trim, split_by_lines};
use crate::{Exception, Result};
use std::io::Write;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// A logging function: it receives each line of a message, prefix and `'\n'` included, as the
/// bytes of a C string (up to the first NUL).
///
/// Port of `LoggingFunction` (include/OpenColorIO/OpenColorTypes.h:283-284 @ v2.5.2).
pub type LoggingFunction = Arc<dyn Fn(&[u8]) + Send + Sync>;

/// Port of `OCIO_LOGGING_LEVEL_ENVVAR` (Logging.cpp:21 @ v2.5.2).
const OCIO_LOGGING_LEVEL_ENVVAR: &str = "OCIO_LOGGING_LEVEL";

/// `GetVersion()` (src/OpenColorIO/Config.cpp:99-102 @ v2.5.2): `OCIO_VERSION_FULL_STR`, the
/// version with no release type.
const OCIO_VERSION_FULL_STR: &str = "2.5.2";

/// The global logging state, under `g_logmutex`.
///
/// Port of `g_logginglevel`, `g_initialized`, `g_loggingOverride` and `g_loggingFunction`
/// (Logging.cpp:23-28, 70-71 @ v2.5.2).
struct State {
    level: LoggingLevel,
    initialized: bool,
    logging_override: bool,
    /// `None` is the default logging function.
    function: Option<LoggingFunction>,
}

static STATE: Mutex<State> = Mutex::new(State {
    level: LoggingLevel::Unknown,
    initialized: false,
    logging_override: false,
    function: None,
});

/// `AutoMutex lock(g_logmutex)`. Nothing panics while holding it, so it is never poisoned;
/// if it were, the state is still consistent.
fn lock() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `std::cerr << text`: the bytes as they are. A failed write is ignored, as `std::cerr`
/// ignores it.
fn write_stderr(text: &[u8]) {
    let _ = std::io::stderr().lock().write_all(text);
}

impl State {
    /// Reads `OCIO_LOGGING_LEVEL` the first time it is called: sets the level, and whether the
    /// variable overrides [`set_logging_level`].
    ///
    /// Port of `InitLogging` (Logging.cpp:30-62 @ v2.5.2).
    fn init(&mut self) {
        if self.initialized {
            return;
        }

        self.initialized = true;

        // Platform::Getenv gives "" for a variable that doesn't exist.
        let levelstr = platform::getenv(OCIO_LOGGING_LEVEL_ENVVAR).unwrap_or_default();
        if !levelstr.is_empty() {
            self.logging_override = true;
            self.level = logging_level_from_string(Some(levelstr.as_bytes()));

            if self.level == LoggingLevel::Unknown {
                write_stderr(b"[OpenColorIO Warning]: Invalid $OCIO_LOGGING_LEVEL specified. ");
                write_stderr(b"Options: none (0), warning (1), info (2), debug (3)\n");
                self.level = LoggingLevel::DEFAULT;
            }
        } else {
            self.level = LoggingLevel::DEFAULT;
        }

        if self.level == LoggingLevel::Debug {
            write_stderr(
                format!(
                    "[OpenColorIO Debug]: Using OpenColorIO version: {OCIO_VERSION_FULL_STR}\n"
                )
                .as_bytes(),
            );
        }
    }
}

/// The default logging function: writes the line to stderr.
///
/// Port of `DefaultLoggingFunction` (Logging.cpp:64-68 @ v2.5.2).
fn default_logging_function(message: &[u8]) {
    write_stderr(message);
}

/// Sends `message` to `function` (the default one for `None`) line by line: trailing white
/// space removed, cut at each `'\n'`, and each line given `message_prefix` and a `'\n'`. The
/// function receives each line as a C string, so a line stops at its first NUL.
///
/// Port of the internal `LogMessage(const char *, const std::string &)` (Logging.cpp:73-88 @
/// v2.5.2), called after the lock is released (D-3).
fn emit(function: Option<&LoggingFunction>, message_prefix: &[u8], message: &[u8]) {
    for part in split_by_lines(right_trim(message)) {
        let mut msg = message_prefix.to_vec();
        msg.extend_from_slice(&part);
        msg.push(b'\n');

        match function {
            Some(f) => f(c_str(&msg)),
            None => default_logging_function(c_str(&msg)),
        }
    }
}

/// Logs `text` with `message_prefix` when the level is at least `min_level`. The level check
/// uses the enum's values, as upstream's integer comparison does.
fn log_at(min_level: LoggingLevel, message_prefix: &[u8], text: &[u8]) {
    let function = {
        let mut state = lock();
        state.init();

        if (state.level as i32) < (min_level as i32) {
            return;
        }
        state.function.clone()
    };
    emit(function.as_ref(), message_prefix, text);
}

/// The global logging level. The default is `Info`; `OCIO_LOGGING_LEVEL` overrides it.
///
/// Port of `GetLoggingLevel` (Logging.cpp:92-98 @ v2.5.2).
pub fn get_logging_level() -> LoggingLevel {
    let mut state = lock();
    state.init();

    state.level
}

/// Sets the global logging level, unless `OCIO_LOGGING_LEVEL` is set: the variable lets users
/// debug OCIO in applications that turn logging off.
///
/// Port of `SetLoggingLevel` (Logging.cpp:100-113 @ v2.5.2).
pub fn set_logging_level(level: LoggingLevel) {
    let mut state = lock();
    state.init();

    // Calls to SetLoggingLevel are ignored if OCIO_LOGGING_LEVEL_ENVVAR is specified. This is
    // to allow users to optionally debug OCIO at runtime even in applications that disable
    // logging.

    if !state.logging_override {
        state.level = level;
    }
}

/// Replaces the logging function. `None` is upstream's empty `std::function`, which it
/// refuses.
///
/// Port of `SetLoggingFunction` (Logging.cpp:115-123 @ v2.5.2).
pub fn set_logging_function(log_function: Option<LoggingFunction>) -> Result<()> {
    let Some(log_function) = log_function else {
        return Err(Exception::new(
            "SetLoggingFunction: logFunction must not be null.",
        ));
    };
    lock().function = Some(log_function);
    Ok(())
}

/// Goes back to the default logging function, which writes to stderr.
///
/// Port of `ResetToDefaultLoggingFunction` (Logging.cpp:125-129 @ v2.5.2).
pub fn reset_to_default_logging_function() {
    lock().function = None;
}

/// Logs `message` at `level`: as a warning, information or a debugging message, or not at all
/// for `None`. `message` is a C string: it ends at its first NUL.
///
/// Port of the public `LogMessage(LoggingLevel, const char *)` (Logging.cpp:131-160 @
/// v2.5.2).
pub fn log_message(level: LoggingLevel, message: &[u8]) -> Result<()> {
    let message = c_str(message);
    match level {
        LoggingLevel::Warning => log_warning(message),
        LoggingLevel::Info => log_info(message),
        LoggingLevel::Debug => log_debug(message),
        LoggingLevel::None => {
            // No logging.
        }
        LoggingLevel::Unknown => return Err(Exception::new("Unsupported logging level.")),
    }
    Ok(())
}

/// Logs an error, with the `[OpenColorIO Error]: ` prefix, from the `Warning` level up: there
/// is no error level, as it would have to come between `None` and `Warning`.
///
/// Port of `LogError` (Logging.cpp:162-174 @ v2.5.2).
pub fn log_error(text: impl AsRef<[u8]>) {
    log_at(
        LoggingLevel::Warning,
        b"[OpenColorIO Error]: ",
        text.as_ref(),
    );
}

/// Logs a warning, with the `[OpenColorIO Warning]: ` prefix, from the `Warning` level up.
///
/// Port of `LogWarning` (Logging.cpp:176-184 @ v2.5.2).
pub fn log_warning(text: impl AsRef<[u8]>) {
    log_at(
        LoggingLevel::Warning,
        b"[OpenColorIO Warning]: ",
        text.as_ref(),
    );
}

/// Logs information, with the `[OpenColorIO Info]: ` prefix, from the `Info` level up.
///
/// Port of `LogInfo` (Logging.cpp:186-194 @ v2.5.2).
pub fn log_info(text: impl AsRef<[u8]>) {
    log_at(LoggingLevel::Info, b"[OpenColorIO Info]: ", text.as_ref());
}

/// Logs a debugging message, with the `[OpenColorIO Debug]: ` prefix, from the `Debug` level
/// up.
///
/// Port of `LogDebug` (Logging.cpp:196-204 @ v2.5.2).
pub fn log_debug(text: impl AsRef<[u8]>) {
    log_at(LoggingLevel::Debug, b"[OpenColorIO Debug]: ", text.as_ref());
}

/// Whether debugging messages are logged: the level is `Debug` or `Unknown`.
///
/// Port of `IsDebugLoggingEnabled` (Logging.cpp:206-209 @ v2.5.2).
pub fn is_debug_logging_enabled() -> bool {
    (get_logging_level() as i32) >= (LoggingLevel::Debug as i32)
}

#[cfg(test)]
#[path = "logging_tests.rs"]
mod tests;
