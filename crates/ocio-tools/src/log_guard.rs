// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Trapping log messages: a port of `src/apputils/logGuard.h` and `logGuard.cpp` @ v2.5.2.
//!
//! Left out, since no ported tool calls them: `clear` and `empty`.

use std::sync::{Arc, Mutex, PoisonError};

use ocio::Exception;
use ocio_ops::logging::{
    get_logging_level, reset_to_default_logging_function, set_logging_function, set_logging_level,
};
use ocio_ops::open_color_types::LoggingLevel;

/// Port of `g_output` (logGuard.cpp:6 @ v2.5.2).
static OUTPUT: Mutex<Vec<u8>> = Mutex::new(Vec::new());

/// Traps any log message while preserving the original logging settings. As upstream's, the
/// mechanism isn't thread-safe.
///
/// Port of `LogGuard` (logGuard.h:12-25 @ v2.5.2).
#[derive(Debug)]
pub struct LogGuard {
    log_level: LoggingLevel,
}

impl LogGuard {
    /// Port of `LogGuard::LogGuard` (logGuard.cpp:12-17 @ v2.5.2).
    pub fn new() -> Result<LogGuard, Exception> {
        let log_level = get_logging_level();
        set_logging_level(LoggingLevel::Debug);
        set_logging_function(Some(Arc::new(|message: &[u8]| {
            OUTPUT
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .extend_from_slice(message);
        })))?;
        Ok(LogGuard { log_level })
    }

    /// The trapped messages.
    ///
    /// Port of `LogGuard::output` (logGuard.cpp:26-29 @ v2.5.2).
    pub fn output(&self) -> Vec<u8> {
        OUTPUT
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Drop for LogGuard {
    /// Port of `LogGuard::~LogGuard` (logGuard.cpp:19-24 @ v2.5.2).
    fn drop(&mut self) {
        reset_to_default_logging_function();
        set_logging_level(self.log_level);
        OUTPUT
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }
}
