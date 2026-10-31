// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! What the developer tools share: `src/bin` holds the tools, and these modules what they use
//! of upstream's `src/apputils`, with the C runtime's streams and number parsing (Phase 10).
#![deny(unsafe_code)]

pub mod argparse;
pub mod c_numbers;
pub mod lcms2;
pub mod log_guard;
pub mod ocioicc;
#[cfg(target_os = "linux")]
mod sigpipe;
pub mod stdio;

use ocio::{Exception, ExceptionKind};

/// Whether `catch (OCIO::Exception &)` catches `exception`: `Exception` and its subclass
/// `ExceptionMissingFile`, and not the C++ standard exceptions that reach OCIO's callers.
pub fn is_ocio_exception(exception: &Exception) -> bool {
    matches!(
        exception.kind(),
        ExceptionKind::Exception | ExceptionKind::MissingFile
    )
}
