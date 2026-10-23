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

use crate::py_open_color_io::py_open_color_io;
