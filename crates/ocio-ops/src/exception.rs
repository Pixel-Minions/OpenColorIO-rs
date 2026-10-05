// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Errors: a port of `OCIO::Exception` and `OCIO::ExceptionMissingFile`
//! (`src/OpenColorIO/Exception.cpp`, `include/OpenColorIO/OpenColorIO.h` @ v2.5.2), and of the
//! C++ standard exceptions that reach OCIO's callers (`std::length_error`, `std::bad_alloc`).
//!
//! Messages are part of the byte-exact surface (PLAN.md §3): every error carries upstream's
//! text verbatim, and `Display` prints exactly that text.

use std::fmt;

/// `std::bad_alloc::what()` of the C++ library the wheel uses.
const BAD_ALLOC: &str = if cfg!(target_os = "windows") {
    "bad allocation"
} else {
    "std::bad_alloc"
};

/// Which upstream exception type an error corresponds to. More may come, as the port reaches
/// other C++ exceptions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ExceptionKind {
    /// `OCIO::Exception`.
    Exception,
    /// `OCIO::ExceptionMissingFile`, a subclass of `OCIO::Exception`.
    MissingFile,
    /// `std::length_error`, which a C++ standard container raises for a size past its limit
    /// (`std::vector::resize`, improvement candidate U-3). PyOpenColorIO raises it as
    /// `ValueError`.
    LengthError,
    /// `std::bad_alloc`, which `operator new` raises when the memory can't be had (a
    /// `std::vector::resize` of the scanline rows, U-3). PyOpenColorIO raises it as
    /// `MemoryError`.
    BadAlloc,
}

/// An OpenColorIO error: upstream's exception type and its message, verbatim.
///
/// The message is bytes, as upstream's `what()` holds them: it embeds names, paths and config
/// text, which need not be UTF-8. [`what`](Self::what) gives the bytes; [`message`](Self::message)
/// and `Display` give them as text, each byte sequence that isn't UTF-8 replaced by U+FFFD.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Exception {
    kind: ExceptionKind,
    message: Vec<u8>,
    /// The message as text, kept for `message`.
    text: String,
}

/// A message as the C++ exceptions take it, a `const char *`: up to its first NUL.
fn c_message(message: impl Into<Vec<u8>>) -> Vec<u8> {
    let mut message = message.into();
    if let Some(nul) = message.iter().position(|&c| c == 0) {
        message.truncate(nul);
    }
    message
}

impl Exception {
    /// `OCIO::Exception(msg)`. The message ends at its first NUL, as the `const char *` upstream
    /// takes.
    pub fn new(message: impl Into<Vec<u8>>) -> Self {
        Exception {
            kind: ExceptionKind::Exception,
            text: String::new(),
            message: c_message(message),
        }
        .with_text()
    }

    /// `OCIO::ExceptionMissingFile(msg)`, the message up to its first NUL.
    pub fn missing_file(message: impl Into<Vec<u8>>) -> Self {
        Exception {
            kind: ExceptionKind::MissingFile,
            text: String::new(),
            message: c_message(message),
        }
        .with_text()
    }

    /// A `std::length_error` with `what()` = `msg`, up to its first NUL.
    pub fn length_error(message: impl Into<Vec<u8>>) -> Self {
        Exception {
            kind: ExceptionKind::LengthError,
            text: String::new(),
            message: c_message(message),
        }
        .with_text()
    }

    /// A `std::bad_alloc`, with the C++ library's `what()`: "bad allocation" in MSVC's (both
    /// modules of the Windows wheel hold the text), "std::bad_alloc" in libstdc++.
    pub fn bad_alloc() -> Self {
        Exception {
            kind: ExceptionKind::BadAlloc,
            message: BAD_ALLOC.as_bytes().to_vec(),
            text: BAD_ALLOC.to_string(),
        }
    }

    /// Fills in the message's text.
    fn with_text(mut self) -> Self {
        self.text = String::from_utf8_lossy(&self.message).into_owned();
        self
    }

    /// The upstream exception type.
    pub fn kind(&self) -> ExceptionKind {
        self.kind
    }

    /// `what()`: the message's bytes, verbatim.
    pub fn what(&self) -> &[u8] {
        &self.message
    }

    /// The message as text: [`what`](Self::what), with each byte sequence that isn't UTF-8
    /// replaced by U+FFFD.
    pub fn message(&self) -> &str {
        &self.text
    }

    /// True for `ExceptionMissingFile`.
    pub fn is_missing_file(&self) -> bool {
        self.kind == ExceptionKind::MissingFile
    }
}

impl fmt::Display for Exception {
    /// [`Exception::message`].
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
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

    /// The message is bytes up to the first NUL, and its text replaces what isn't UTF-8.
    #[test]
    fn message_is_bytes() {
        let e = Exception::new(b"a\xffb\0c".to_vec());
        assert_eq!(e.what(), b"a\xffb");
        assert_eq!(e.message(), "a\u{fffd}b");
        assert_eq!(e.to_string(), "a\u{fffd}b");
    }
}
