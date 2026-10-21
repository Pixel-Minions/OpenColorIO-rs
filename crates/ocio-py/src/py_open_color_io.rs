// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The module `PyOpenColorIO.PyOpenColorIO`: a port of `PYBIND11_MODULE(PyOpenColorIO, m)`
//! (src/bindings/python/PyOpenColorIO.cpp:9-104 @ v2.5.2) and of what `PyOpenColorIO.h`
//! declares for it. Bound so far (chunk 6.1a): the module's docstring and attributes, the two
//! exceptions, and the global functions on versions, caches and environment variables. The
//! other `bindPy*` calls, and the global functions that take enums (the logging level) or
//! Python callbacks (logging, hashing), come with their chunks (WP 6.2 to 6.7).
//!
//! Every function goes through pybind11's dispatcher ([`crate::dispatch`]): arguments,
//! errors and docstrings as the wheel's.

use pyo3::exceptions::{PyException, PyMemoryError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyDict, PyInt, PyString, PyTuple, PyType};

use crate::arg;
use crate::dispatch::{CharPtr, Function, Overload, Param, std_string_to_py};

/// `OCIO.Exception`, made when the module is (`register_exception`).
static EXCEPTION: PyOnceLock<Py<PyType>> = PyOnceLock::new();
/// `OCIO.ExceptionMissingFile`.
static EXCEPTION_MISSING_FILE: PyOnceLock<Py<PyType>> = PyOnceLock::new();

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
pub(crate) fn py_result<T>(py: Python<'_>, r: ocio::Result<T>) -> PyResult<T> {
    r.map_err(|e| to_py_err(py, &e))
}

/// Defines the module function `$rust`, which Python calls `$name`: bound through
/// [`crate::dispatch::dispatch`] with `$function`'s overloads, taking `*args` and `**kwargs`
/// (`METH_VARARGS | METH_KEYWORDS`, as pybind11's functions do, so CPython gives it no
/// `__text_signature__`), with the docstring `$doc`: pybind11's for `$function`'s overloads
/// ([`crate::dispatch::docstring`]), which tests/dispatch_oracle.rs compares with the wheel's.
macro_rules! py_def {
    ($rust:ident, $name:literal, $doc:literal, $function:path) => {
        #[doc = $doc]
        #[pyfunction(name = $name, signature = (*args, **kwargs))]
        #[pyo3(text_signature = None)]
        fn $rust<'py>(
            py: Python<'py>,
            args: &Bound<'py, PyTuple>,
            kwargs: Option<&Bound<'py, PyDict>>,
        ) -> PyResult<Py<PyAny>> {
            crate::dispatch::dispatch(py, &$function, args, kwargs)
        }
    };
}

/// A module function with one overload.
pub(crate) const fn function(name: &'static str, overloads: &'static [Overload]) -> Function {
    Function {
        name,
        constructor: false,
        method: false,
        overloads,
    }
}

fn none(py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
    Ok(Some(py.None()))
}

// m.def("ClearAllCaches", &ClearAllCaches) (PyOpenColorIO.cpp:41-42 @ v2.5.2).
static CLEAR_ALL_CACHES: Function = function(
    "ClearAllCaches",
    &[Overload {
        params: &[],
        ret: "None",
        call: |py, _, _, _| {
            ocio::clear_all_caches();
            none(py)
        },
        pos_only: 0,
    }],
);
py_def!(
    clear_all_caches,
    "ClearAllCaches",
    "ClearAllCaches() -> None\n",
    CLEAR_ALL_CACHES
);

// m.def("GetVersion", &GetVersion) (PyOpenColorIO.cpp:43-44 @ v2.5.2).
static GET_VERSION: Function = function(
    "GetVersion",
    &[Overload {
        params: &[],
        ret: "str",
        call: |py, _, _, _| Ok(Some(PyString::new(py, ocio::version()).into_any().unbind())),
        pos_only: 0,
    }],
);
py_def!(
    get_version,
    "GetVersion",
    "GetVersion() -> str\n",
    GET_VERSION
);

// m.def("GetVersionHex", &GetVersionHex) (PyOpenColorIO.cpp:45-46 @ v2.5.2).
static GET_VERSION_HEX: Function = function(
    "GetVersionHex",
    &[Overload {
        params: &[],
        ret: "int",
        call: |py, _, _, _| {
            Ok(Some(
                PyInt::new(py, ocio::version_hex()).into_any().unbind(),
            ))
        },
        pos_only: 0,
    }],
);
py_def!(
    get_version_hex,
    "GetVersionHex",
    "GetVersionHex() -> int\n",
    GET_VERSION_HEX
);

/// A `const char *` argument's bytes, a null pointer as the empty string: what upstream's
/// environment functions (Platform.cpp's Getenv, Setenv, Unsetenv) do with it.
fn c_str(s: CharPtr) -> Vec<u8> {
    s.0.unwrap_or_default()
}

const NAME: Param = Param::new("name", "str");

// m.def("GetEnvVariable", &GetEnvVariable, "name"_a) (PyOpenColorIO.cpp:61-62 @ v2.5.2).
static GET_ENV_VARIABLE: Function = function(
    "GetEnvVariable",
    &[Overload {
        params: &[NAME],
        ret: "str",
        call: |py, _, a, convert| {
            let name = arg!(a, 0, convert, CharPtr);
            let value = ocio::get_env_variable(c_str(name));
            Ok(Some(std_string_to_py(py, &value)?.into_any().unbind()))
        },
        pos_only: 0,
    }],
);
py_def!(
    get_env_variable,
    "GetEnvVariable",
    "GetEnvVariable(name: str) -> str\n",
    GET_ENV_VARIABLE
);

// m.def("SetEnvVariable", &SetEnvVariable, "name"_a, "value"_a)
// (PyOpenColorIO.cpp:63-64 @ v2.5.2).
static SET_ENV_VARIABLE: Function = function(
    "SetEnvVariable",
    &[Overload {
        params: &[NAME, Param::new("value", "str")],
        ret: "None",
        call: |py, _, a, convert| {
            let name = arg!(a, 0, convert, CharPtr);
            let value = arg!(a, 1, convert, CharPtr);
            py_result(py, ocio::set_env_variable(c_str(name), value.0.as_deref()))?;
            none(py)
        },
        pos_only: 0,
    }],
);
py_def!(
    set_env_variable,
    "SetEnvVariable",
    "SetEnvVariable(name: str, value: str) -> None\n",
    SET_ENV_VARIABLE
);

// m.def("UnsetEnvVariable", &UnsetEnvVariable, "name"_a) (PyOpenColorIO.cpp:65-66 @ v2.5.2).
static UNSET_ENV_VARIABLE: Function = function(
    "UnsetEnvVariable",
    &[Overload {
        params: &[NAME],
        ret: "None",
        call: |py, _, a, convert| {
            let name = arg!(a, 0, convert, CharPtr);
            py_result(py, ocio::unset_env_variable(c_str(name)))?;
            none(py)
        },
        pos_only: 0,
    }],
);
py_def!(
    unset_env_variable,
    "UnsetEnvVariable",
    "UnsetEnvVariable(name: str) -> None\n",
    UNSET_ENV_VARIABLE
);

// m.def("IsEnvVariablePresent", &IsEnvVariablePresent, "name"_a)
// (PyOpenColorIO.cpp:67-68 @ v2.5.2).
static IS_ENV_VARIABLE_PRESENT: Function = function(
    "IsEnvVariablePresent",
    &[Overload {
        params: &[NAME],
        ret: "bool",
        call: |py, _, a, convert| {
            let name = arg!(a, 0, convert, CharPtr);
            let present = ocio::is_env_variable_present(c_str(name));
            Ok(Some(
                pyo3::types::PyBool::new(py, present)
                    .to_owned()
                    .into_any()
                    .unbind(),
            ))
        },
        pos_only: 0,
    }],
);
py_def!(
    is_env_variable_present,
    "IsEnvVariablePresent",
    "IsEnvVariablePresent(name: str) -> bool\n",
    IS_ENV_VARIABLE_PRESENT
);

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
    m.setattr(name, ty.bind(py))?;
    let _ = cell.set(py, ty);
    Ok(())
}

/// `m.def(...)`: the function into the module's dictionary. (PyO3's `add_function` and `add`
/// would also list the name in an `__all__`, which the wheel's module doesn't have; the
/// package's `from .PyOpenColorIO import *` would then import only those names.)
pub(crate) fn def(m: &Bound<'_, PyModule>, f: Bound<'_, pyo3::types::PyCFunction>) -> PyResult<()> {
    // pybind11 gives every function the module's name (`m_module`), also embedded, where the
    // module is named `PyOpenColorIO`.
    f.setattr("__module__", crate::pybind11::MODULE)?;
    m.setattr(f.getattr("__name__")?.cast_into::<PyString>()?, f)
}

// PYBIND11_MODULE(PyOpenColorIO, m) (PyOpenColorIO.cpp:9-104 @ v2.5.2), in its order: the
// docstring, the exceptions (17-28), the attributes (31-38), the global functions (40-68).
// The module's __doc__ is set twice upstream, by m.doc() and by m.attr("__doc__"), to the same
// text.
#[doc = "OpenColorIO (OCIO) is a complete color management solution geared towards motion picture production"]
#[pymodule(name = "PyOpenColorIO")]
pub(crate) fn py_open_color_io(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // OpenColorIOTypes (PyOpenColorIO.cpp:13-14).
    crate::py_types::bind_py_types(m)?;
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

    m.setattr("__author__", "OpenColorIO Contributors")?;
    m.setattr("__email__", "ocio-dev@lists.aswf.io")?;
    m.setattr("__license__", "SPDX-License-Identifier: BSD-3-Clause")?;
    m.setattr(
        "__copyright__",
        "Copyright Contributors to the OpenColorIO Project",
    )?;
    m.setattr("__version__", ocio::version())?;
    // OCIO_VERSION_STATUS_STR is empty in a release build: "Production".
    m.setattr("__status__", "Production")?;
    m.setattr(
        "__doc__",
        "OpenColorIO (OCIO) is a complete color management solution geared towards motion \
         picture production",
    )?;

    def(m, wrap_pyfunction!(clear_all_caches, m)?)?;
    def(m, wrap_pyfunction!(get_version, m)?)?;
    def(m, wrap_pyfunction!(get_version_hex, m)?)?;
    def(m, wrap_pyfunction!(get_env_variable, m)?)?;
    def(m, wrap_pyfunction!(set_env_variable, m)?)?;
    def(m, wrap_pyfunction!(unset_env_variable, m)?)?;
    def(m, wrap_pyfunction!(is_env_variable_present, m)?)?;

    // The classes' bindings (PyOpenColorIO.cpp:70-103), those ported so far.
    crate::py_gpu_shader_creator::bind_py_gpu_shader_creator(m)?;
    Ok(())
}
