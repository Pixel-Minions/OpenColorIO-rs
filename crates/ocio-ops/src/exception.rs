// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Errors: a port of `OCIO::Exception` and `OCIO::ExceptionMissingFile`
//! (`src/OpenColorIO/Exception.cpp`, `include/OpenColorIO/OpenColorIO.h` @ v2.5.2).
//!
//! Messages are part of the byte-exact surface (PLAN.md §3): every error carries upstream's
//! text verbatim, and `Display` prints exactly that text.

use std::fmt;

/// Which upstream exception type an error corresponds to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExceptionKind {
    /// `OCIO::Exception`.
    Exception,
    /// `OCIO::ExceptionMissingFile`, a subclass of `OCIO::Exception`.
    MissingFile,
}

/// An OpenColorIO error: upstream's exception type and its message, verbatim.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Exception {
    kind: ExceptionKind,
    message: String,
}

impl Exception {
    /// `OCIO::Exception(msg)`.
    pub fn new(message: impl Into<String>) -> Self {
        Exception {
            kind: ExceptionKind::Exception,
            message: message.into(),
        }
    }

    /// `OCIO::ExceptionMissingFile(msg)`.
    pub fn missing_file(message: impl Into<String>) -> Self {
        Exception {
            kind: ExceptionKind::MissingFile,
            message: message.into(),
        }
    }

    /// The upstream exception type.
    pub fn kind(&self) -> ExceptionKind {
        self.kind
    }

    /// `what()`: the message, verbatim.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// True for `ExceptionMissingFile`.
    pub fn is_missing_file(&self) -> bool {
        self.kind == ExceptionKind::MissingFile
    }
}

impl fmt::Display for Exception {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Exception {}

/// `Result` with an OpenColorIO [`Exception`].
pub type Result<T> = std::result::Result<T, Exception>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_is_verbatim() {
        let e = Exception::new("SetLoggingFunction: logFunction must not be null.");
        assert_eq!(
            e.to_string(),
            "SetLoggingFunction: logFunction must not be null."
        );
        assert!(!e.is_missing_file());
        assert!(Exception::missing_file("x").is_missing_file());
    }
}
