// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The embedding API (chunk 6.1c, owner decision P6-9): a Rust application that runs Python
//! in its own process registers `PyOpenColorIO` before the interpreter starts, and its Python
//! code then imports the module from the application itself, sharing the process's OCIO
//! state (caches, the logging level, environment variables) with the Rust side.
//!
//! The conversions of the shared Rust objects (`Arc<Config>`, processors, transforms) to
//! Python objects and back come with the chunks that bind each class (WP 6.3 to 6.7): each
//! adds them here, sharing the Rust object rather than copying it.

/// Registers the module `PyOpenColorIO` as a built-in module of the interpreter this process
/// will start (`PyImport_AppendInittab`), so that `import PyOpenColorIO` finds it without the
/// package on disk. Call it before the interpreter starts (`pyo3::Python::initialize`, or
/// before `Py_Initialize` in an application that starts Python itself).
///
/// Embedded, `import PyOpenColorIO` gives the compiled module itself (named
/// `PyOpenColorIO`), with every name the package re-exports.
///
/// # Panics
///
/// If an interpreter is already running in this process.
pub fn append_to_inittab() {
    pyo3::append_to_inittab!(py_open_color_io);
}

use crate::py_open_color_io::py_open_color_io;
