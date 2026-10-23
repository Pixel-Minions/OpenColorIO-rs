// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The embedding API (chunk 6.1c, owner decision P6-9): a Rust application that runs Python
//! in its own process registers `PyOpenColorIO` before the interpreter starts, and its Python
//! code then imports the module from the application itself, sharing the process's OCIO
//! state (caches, the logging level, environment variables) with the Rust side.
//!
//! The conversions of the shared Rust objects (`Arc<Config>`, processors, transforms) to
//! Python objects and back come with the chunks that bind each class (WP 6.3 to 6.7): each
//! adds them here, sharing the Rust object rather than copying it where it can.
//!
//! A config can't be shared (owner decision 2026-10-22, P6-8 Q1 = A): Python edits a config in
//! place, as the wheel does, through the objects its getters return, which the library's
//! `Arc<Config>` can't follow. [`config_to_python`] gives Python a copy of a Rust config, and
//! [`config_from_python`] gives Rust a snapshot of a Python one, as it is at that moment.
//!
//! The process's current config is shared: `ocio::set_current_config` from Rust is the config
//! Python's `GetCurrentConfig` returns next (a new object), `SetCurrentConfig` from Python is
//! the one `ocio::get_current_config` returns, and what Python changes in place in its current
//! config (through it, or the objects its getters return) is in the `Arc<Config>` the next
//! `ocio::get_current_config` returns, a new snapshot (an earlier one keeps what it had). While
//! Python changed nothing, `ocio::get_current_config` returns the same `Arc<Config>`, with the
//! processors it cached, as upstream's returns the same pointer. What counts as a change: a
//! call of a bound function other than a getter (`get...`, `is...`, `has...`) or a protocol
//! that reads (`__len__`, `__getitem__`, ...) that changes the config, an object of it Python
//! holds or a transform those hold, or any config's context or rules; such a change costs a
//! copy of the config at the next read.
//!
//! `ocio::get_current_config` neither takes the GIL nor waits for Python: called while a call
//! (a Python call on the current config, of any thread, or the call whose callback calls it)
//! holds Python's current config, it returns the config as last published, without what Python
//! changed since; the next read after that call ends has it.

/// The Python `PyOpenColorIO.CPUProcessor` of `cpu`, sharing it: the object the module made for
/// that processor while it lives, else a new one (as pybind11 returns a `CPUProcessorRcPtr`).
/// The interpreter must have imported `PyOpenColorIO`.
///
/// # Errors
///
/// A Python error making the object.
pub fn cpu_processor_to_python(
    py: pyo3::Python<'_>,
    cpu: &std::sync::Arc<ocio_ops::cpu_processor::CpuProcessor>,
) -> pyo3::PyResult<pyo3::Py<pyo3::PyAny>> {
    crate::py_cpu_processor::cpu_processor_out(py, cpu).map(pyo3::Bound::unbind)
}

/// The CPU processor the Python `PyOpenColorIO.CPUProcessor` `obj` holds, shared.
///
/// # Errors
///
/// TypeError when `obj` isn't a `CPUProcessor`.
pub fn cpu_processor_from_python(
    obj: &pyo3::Bound<'_, pyo3::PyAny>,
) -> pyo3::PyResult<std::sync::Arc<ocio_ops::cpu_processor::CpuProcessor>> {
    crate::py_cpu_processor::cpu_processor_of(obj)?.ok_or_else(|| {
        pyo3::exceptions::PyTypeError::new_err(
            "cpu_processor_from_python: not a PyOpenColorIO.CPUProcessor",
        )
    })
}

/// A copy of the transform the Python `PyOpenColorIO.Transform` (of any class) `obj` holds, as
/// the library holds one (a group with copies of its children): a later change Python makes
/// isn't the copy's.
///
/// # Errors
///
/// TypeError when `obj` isn't a `Transform`; the port's errors for a group that holds a null
/// transform or itself (U-110, U-111).
pub fn transform_from_python(
    obj: &pyo3::Bound<'_, pyo3::PyAny>,
) -> pyo3::PyResult<ocio::Transform> {
    let Some(rc) = crate::py_transform::holder(obj, crate::py_transform::TRANSFORM)? else {
        return Err(pyo3::exceptions::PyTypeError::new_err(
            "transform_from_python: not a PyOpenColorIO.Transform",
        ));
    };
    rc.materialize()
}

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

/// A Python `PyOpenColorIO.Config` holding a copy of `config` (its caches as they are): a
/// change Python makes to it is not `config`'s, nor the other way round.
///
/// # Errors
///
/// A Python error making the object.
#[cfg(feature = "python")]
pub fn config_to_python(
    py: pyo3::Python<'_>,
    config: &ocio::Config,
) -> pyo3::PyResult<pyo3::Py<pyo3::PyAny>> {
    crate::py_config::config_out(py, config.clone_as_is())
        .map(|o| o.expect("a config's Python object"))
}

/// A snapshot of the Python `PyOpenColorIO.Config` `obj`: a copy of its config as it is now,
/// with what Python changed through the objects it got from it. A later change Python makes
/// isn't the snapshot's.
///
/// # Errors
///
/// TypeError when `obj` isn't a `Config`; the port's D-9 RuntimeError when the library is
/// working on that config (from one of its callbacks).
#[cfg(feature = "python")]
pub fn config_from_python(
    obj: &pyo3::Bound<'_, pyo3::PyAny>,
) -> pyo3::PyResult<std::sync::Arc<ocio::Config>> {
    let Some(Some(rc)) = crate::py_config::config_arg(obj, false)? else {
        return Err(pyo3::exceptions::PyTypeError::new_err(
            "config_from_python: not a PyOpenColorIO.Config",
        ));
    };
    crate::py_config::with_config(&rc, |n| Ok(std::sync::Arc::new(n.config.clone_as_is())))
}

use crate::py_open_color_io::py_open_color_io;
