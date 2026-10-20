// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The module `PyOpenColorIO.PyOpenColorIO`: a port of `PYBIND11_MODULE(PyOpenColorIO, m)`
//! (src/bindings/python/PyOpenColorIO.cpp:9-104 @ v2.5.2) and of what `PyOpenColorIO.h`
//! declares for it. Bound so far (chunk 6.1a): the module's docstring and attributes, the two
//! exceptions, and the global functions on versions, caches and environment variables. The
//! other `bindPy*` calls, and the global functions that take enums (the logging level) or
//! Python callbacks (logging, hashing), come with their chunks (WP 6.2 to 6.7).
//!
//! Arguments: a `std::string` parameter takes a `str` (encoded as UTF-8) or `bytes`, as
//! pybind11's string caster does; a returned `std::string` is decoded as strict UTF-8, so
//! bytes that aren't UTF-8 raise `UnicodeDecodeError`. The `TypeError` texts of arguments no
//! overload accepts are pybind11's dispatcher's, which chunk 6.4a brings: until then they are
//! PyO3's.

use pyo3::exceptions::{PyException, PyMemoryError, PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyBytes, PyString, PyType};

/// `OCIO.Exception`, made when the module is (`register_exception`).
static EXCEPTION: PyOnceLock<Py<PyType>> = PyOnceLock::new();
/// `OCIO.ExceptionMissingFile`.
static EXCEPTION_MISSING_FILE: PyOnceLock<Py<PyType>> = PyOnceLock::new();

/// pybind11's string caster for a `std::string` parameter: a `str`, encoded as UTF-8, or
/// `bytes`, as they are.
#[derive(Debug)]
pub struct StdString(pub Vec<u8>);

impl<'a, 'py> FromPyObject<'a, 'py> for StdString {
    type Error = PyErr;

    fn extract(obj: Borrowed<'a, 'py, PyAny>) -> PyResult<Self> {
        if let Ok(s) = obj.cast::<PyString>() {
            // PyUnicode_AsEncodedString(s, "utf-8", nullptr): strict, so a lone surrogate
            // fails the conversion.
            let encoded = s.encode_utf8().map_err(|_| {
                PyTypeError::new_err("incompatible function arguments (a str that isn't UTF-8)")
            })?;
            return Ok(StdString(encoded.as_bytes().to_vec()));
        }
        if let Ok(b) = obj.cast::<PyBytes>() {
            return Ok(StdString(b.as_bytes().to_vec()));
        }
        Err(PyTypeError::new_err(
            "incompatible function arguments (expected str or bytes)",
        ))
    }
}

/// A returned `std::string` as pybind11 hands it to Python: decoded as strict UTF-8
/// (`PyUnicode_DecodeUTF8`), so bytes that aren't UTF-8 raise `UnicodeDecodeError`.
pub fn std_string_to_py<'py>(py: Python<'py>, bytes: &[u8]) -> PyResult<Bound<'py, PyString>> {
    PyString::from_encoded_object(&PyBytes::new(py, bytes), Some(c"utf-8"), Some(c"strict"))
}

/// A Rust error as the Python exception the wheel raises for the C++ one: pybind11's
/// translators (`register_exception` for OCIO's two classes, its own for the standard
/// exceptions), each with `what()` as its message (`PyErr_SetString`, which decodes it as
/// strict UTF-8: a message that isn't raises `UnicodeDecodeError` instead).
pub fn to_py_err(py: Python<'_>, e: &ocio::Exception) -> PyErr {
    let message = match std_string_to_py(py, e.what()) {
        Ok(message) => message.unbind(),
        Err(decode_error) => return decode_error,
    };
    match e.kind() {
        ocio::ExceptionKind::Exception => match EXCEPTION.get(py) {
            Some(ty) => PyErr::from_type(ty.bind(py).clone(), (message,)),
            None => PyException::new_err((message,)),
        },
        ocio::ExceptionKind::MissingFile => match EXCEPTION_MISSING_FILE.get(py) {
            Some(ty) => PyErr::from_type(ty.bind(py).clone(), (message,)),
            None => PyException::new_err((message,)),
        },
        ocio::ExceptionKind::LengthError => PyValueError::new_err((message,)),
        ocio::ExceptionKind::BadAlloc => PyMemoryError::new_err((message,)),
        // std::regex_error is a std::runtime_error, and so is anything else pybind11 doesn't
        // translate itself.
        _ => PyRuntimeError::new_err((message,)),
    }
}

/// `Result<T, ocio::Exception>` as a Python result.
fn py_result<T>(py: Python<'_>, r: ocio::Result<T>) -> PyResult<T> {
    r.map_err(|e| to_py_err(py, &e))
}

// m.def("ClearAllCaches", &ClearAllCaches) (PyOpenColorIO.cpp:41-42 @ v2.5.2).
#[doc = "ClearAllCaches() -> None\n"]
#[pyfunction(name = "ClearAllCaches")]
#[pyo3(text_signature = None)]
fn clear_all_caches() {
    ocio::clear_all_caches();
}

// m.def("GetVersion", &GetVersion) (PyOpenColorIO.cpp:43-44 @ v2.5.2).
#[doc = "GetVersion() -> str\n"]
#[pyfunction(name = "GetVersion")]
#[pyo3(text_signature = None)]
fn get_version() -> &'static str {
    ocio::version()
}

// m.def("GetVersionHex", &GetVersionHex) (PyOpenColorIO.cpp:45-46 @ v2.5.2).
#[doc = "GetVersionHex() -> int\n"]
#[pyfunction(name = "GetVersionHex")]
#[pyo3(text_signature = None)]
fn get_version_hex() -> i32 {
    ocio::version_hex()
}

// m.def("GetEnvVariable", &GetEnvVariable, "name"_a) (PyOpenColorIO.cpp:61-62 @ v2.5.2).
#[doc = "GetEnvVariable(name: str) -> str\n"]
#[pyfunction(name = "GetEnvVariable")]
#[pyo3(text_signature = None)]
fn get_env_variable(py: Python<'_>, name: StdString) -> PyResult<Bound<'_, PyString>> {
    std_string_to_py(py, &ocio::get_env_variable(&name.0))
}

// m.def("SetEnvVariable", &SetEnvVariable, "name"_a, "value"_a)
// (PyOpenColorIO.cpp:63-64 @ v2.5.2).
#[doc = "SetEnvVariable(name: str, value: str) -> None\n"]
#[pyfunction(name = "SetEnvVariable")]
#[pyo3(text_signature = None)]
fn set_env_variable(py: Python<'_>, name: StdString, value: StdString) -> PyResult<()> {
    py_result(py, ocio::set_env_variable(&name.0, Some(&value.0)))
}

// m.def("UnsetEnvVariable", &UnsetEnvVariable, "name"_a) (PyOpenColorIO.cpp:65-66 @ v2.5.2).
#[doc = "UnsetEnvVariable(name: str) -> None\n"]
#[pyfunction(name = "UnsetEnvVariable")]
#[pyo3(text_signature = None)]
fn unset_env_variable(py: Python<'_>, name: StdString) -> PyResult<()> {
    py_result(py, ocio::unset_env_variable(&name.0))
}

// m.def("IsEnvVariablePresent", &IsEnvVariablePresent, "name"_a)
// (PyOpenColorIO.cpp:67-68 @ v2.5.2).
#[doc = "IsEnvVariablePresent(name: str) -> bool\n"]
#[pyfunction(name = "IsEnvVariablePresent")]
#[pyo3(text_signature = None)]
fn is_env_variable_present(name: StdString) -> bool {
    ocio::is_env_variable_present(&name.0)
}

/// An OCIO exception class as pybind11's `register_exception` makes it: a subclass of
/// Python's `Exception` in the module (`PyErr_NewException`, without a docstring), whose
/// `__doc__` the module then sets to `DOC(Exception)`, empty in the wheel's build (so the
/// class's dictionary holds `__doc__` after `__module__` and `__weakref__`, as the wheel's).
fn register_exception(
    m: &Bound<'_, PyModule>,
    name: &str,
    qualified: &std::ffi::CStr,
    cell: &PyOnceLock<Py<PyType>>,
) -> PyResult<()> {
    let py = m.py();
    let ty = PyErr::new_type(
        py,
        qualified,
        None,
        Some(&py.get_type::<PyException>()),
        None,
    )?;
    ty.bind(py).setattr("__doc__", "")?;
    m.add(name, ty.bind(py))?;
    let _ = cell.set(py, ty);
    Ok(())
}

// PYBIND11_MODULE(PyOpenColorIO, m) (PyOpenColorIO.cpp:9-104 @ v2.5.2), in its order: the
// docstring, the exceptions (17-28), the attributes (31-38), the global functions (40-68).
// The module's __doc__ is set twice upstream, by m.doc() and by m.attr("__doc__"), to the same
// text.
#[doc = "OpenColorIO (OCIO) is a complete color management solution geared towards motion picture production"]
#[pymodule(name = "PyOpenColorIO")]
fn py_open_color_io(m: &Bound<'_, PyModule>) -> PyResult<()> {
    register_exception(
        m,
        "Exception",
        c"PyOpenColorIO.PyOpenColorIO.Exception",
        &EXCEPTION,
    )?;
    register_exception(
        m,
        "ExceptionMissingFile",
        c"PyOpenColorIO.PyOpenColorIO.ExceptionMissingFile",
        &EXCEPTION_MISSING_FILE,
    )?;

    m.add("__author__", "OpenColorIO Contributors")?;
    m.add("__email__", "ocio-dev@lists.aswf.io")?;
    m.add("__license__", "SPDX-License-Identifier: BSD-3-Clause")?;
    m.add(
        "__copyright__",
        "Copyright Contributors to the OpenColorIO Project",
    )?;
    m.add("__version__", ocio::version())?;
    // OCIO_VERSION_STATUS_STR is empty in a release build: "Production".
    m.add("__status__", "Production")?;
    m.add(
        "__doc__",
        "OpenColorIO (OCIO) is a complete color management solution geared towards motion \
         picture production",
    )?;

    m.add_function(wrap_pyfunction!(clear_all_caches, m)?)?;
    m.add_function(wrap_pyfunction!(get_version, m)?)?;
    m.add_function(wrap_pyfunction!(get_version_hex, m)?)?;
    m.add_function(wrap_pyfunction!(get_env_variable, m)?)?;
    m.add_function(wrap_pyfunction!(set_env_variable, m)?)?;
    m.add_function(wrap_pyfunction!(unset_env_variable, m)?)?;
    m.add_function(wrap_pyfunction!(is_env_variable_present, m)?)?;
    Ok(())
}
