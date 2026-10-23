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
use pyo3::types::{PyInt, PyString, PyType};

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

/// A module function with one overload.
pub(crate) const fn function(name: &'static str, overloads: &'static [Overload]) -> Function {
    Function {
        name,
        constructor: false,
        method: false,
        operator: false,
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

const LOGGING_LEVEL: &str = "PyOpenColorIO.PyOpenColorIO.LoggingLevel";

/// The library's `LoggingLevel` of the C++ value `v`, if it is an enumerator.
fn logging_level(v: i64) -> Option<ocio_ops::open_color_types::LoggingLevel> {
    use ocio_ops::open_color_types::LoggingLevel::*;
    [None, Warning, Info, Debug, Unknown]
        .into_iter()
        .find(|l| *l as i64 == v)
}

// m.def("GetLoggingLevel", &GetLoggingLevel) (PyOpenColorIO.cpp:47-48 @ v2.5.2).
static GET_LOGGING_LEVEL: Function = function(
    "GetLoggingLevel",
    &[Overload {
        params: &[],
        ret: LOGGING_LEVEL,
        call: |py, _, _, _| {
            let level = ocio_ops::logging::get_logging_level();
            crate::py_transform::enum_out(py, LOGGING_LEVEL, level as i64)
        },
        pos_only: 0,
    }],
);

// m.def("SetLoggingLevel", &SetLoggingLevel, "level"_a) (PyOpenColorIO.cpp:49-50 @ v2.5.2): a
// level outside the enumerators, which the wheel keeps, is U-112's error.
static SET_LOGGING_LEVEL: Function = function(
    "SetLoggingLevel",
    &[Overload {
        params: &[Param::new("level", LOGGING_LEVEL)],
        ret: "None",
        call: |py, _, a, _| {
            let Some(v) = crate::enums::enum_value(&a[0], LOGGING_LEVEL)? else {
                return Ok(None);
            };
            let level = logging_level(v)
                .ok_or_else(|| crate::py_transform::outside_enumerators(LOGGING_LEVEL, v))?;
            ocio_ops::logging::set_logging_level(level);
            none(py)
        },
        pos_only: 0,
    }],
);

/// The library's logging function that calls the Python callable `f` with each message, the
/// GIL taken (pybind11's `std::function` wrapper): its result ignored, its exception thrown
/// through the library ([`crate::callback`]).
fn python_logging_function(f: Py<PyAny>) -> ocio_ops::logging::LoggingFunction {
    std::sync::Arc::new(move |message: &[u8]| {
        Python::attach(|py| {
            use crate::callback::{CallbackError, throw};
            let outcome =
                crate::dispatch::char_ptr_to_py(py, Some(message)).and_then(|m| f.call1(py, (m,)));
            if let Err(e) = outcome {
                throw(py, CallbackError::Python(e));
            }
        })
    })
}

/// A `std::function` argument (pybind11's caster): a callable; with conversions, also `None`,
/// an empty function. `Ok(None)`: refused.
fn function_arg(obj: &Bound<'_, PyAny>, convert: bool) -> Option<Option<Py<PyAny>>> {
    if obj.is_none() {
        return convert.then_some(None);
    }
    obj.is_callable().then(|| Some(obj.clone().unbind()))
}

// m.def("SetLoggingFunction", &SetLoggingFunction, "logFunction"_a) (PyOpenColorIO.cpp:51-52
// @ v2.5.2): `None` is the empty function the library refuses.
static SET_LOGGING_FUNCTION: Function = function(
    "SetLoggingFunction",
    &[Overload {
        params: &[Param::new(
            "logFunction",
            "collections.abc.Callable[[str], None]",
        )],
        ret: "None",
        call: |py, _, a, convert| {
            let Some(f) = function_arg(&a[0], convert) else {
                return Ok(None);
            };
            py_result(
                py,
                ocio_ops::logging::set_logging_function(f.map(python_logging_function)),
            )?;
            none(py)
        },
        pos_only: 0,
    }],
);

// m.def("ResetToDefaultLoggingFunction", &ResetToDefaultLoggingFunction)
// (PyOpenColorIO.cpp:53-54 @ v2.5.2).
static RESET_TO_DEFAULT_LOGGING_FUNCTION: Function = function(
    "ResetToDefaultLoggingFunction",
    &[Overload {
        params: &[],
        ret: "None",
        call: |py, _, _, _| {
            ocio_ops::logging::reset_to_default_logging_function();
            none(py)
        },
        pos_only: 0,
    }],
);

// m.def("LogMessage", &LogMessage, "level"_a, "message"_a) (PyOpenColorIO.cpp:55-56 @ v2.5.2):
// upstream's `switch` (Logging.cpp:131-160) logs nothing for a level outside the enumerators;
// a null message is made a `std::string` of (U-123).
static LOG_MESSAGE: Function = function(
    "LogMessage",
    &[Overload {
        params: &[
            Param::new("level", LOGGING_LEVEL),
            Param::new("message", "str"),
        ],
        ret: "None",
        call: |py, _, a, convert| {
            let Some(v) = crate::enums::enum_value(&a[0], LOGGING_LEVEL)? else {
                return Ok(None);
            };
            let CharPtr(message) = arg!(a, 1, convert, CharPtr);
            let Some(message) = message else {
                return Err(crate::py_context::null_string("LogMessage(message)"));
            };
            if let Some(level) = logging_level(v) {
                py_result(py, ocio_ops::logging::log_message(level, &message))?;
            }
            none(py)
        },
        pos_only: 0,
    }],
);

/// The library's hash function that calls the Python callable `f` with the path (as a `str`,
/// strict UTF-8), the GIL taken: its result as a `std::string` (`str`, `bytes` or `bytearray`;
/// pybind11's `cast_error` otherwise); `None` is the empty `std::function`, whose call throws
/// `std::bad_function_call` (RuntimeError "bad function call" from MSVC's STL,
/// "bad_function_call" from libstdc++: I-301).
fn python_hash_function(f: Option<Py<PyAny>>) -> ocio::path_utils::ComputeHashFunction {
    std::sync::Arc::new(move |path: &[u8]| {
        Python::attach(|py| {
            use crate::callback::{CallbackError, throw};
            let Some(f) = &f else {
                let what = if cfg!(windows) {
                    "bad function call"
                } else {
                    "bad_function_call"
                };
                throw(py, CallbackError::Cpp(PyRuntimeError::new_err(what)));
            };
            let result = std_string_to_py(py, path)
                .and_then(|p| f.call1(py, (p,)))
                .map_err(CallbackError::Python);
            let text = result.and_then(|r| {
                crate::py_config_io_proxy::cast_string(r.bind(py)).map_err(CallbackError::Cpp)
            });
            match text {
                Ok(t) => t,
                Err(e) => throw(py, e),
            }
        })
    })
}

// m.def("SetComputeHashFunction", &SetComputeHashFunction, "hashFunction"_a)
// (PyOpenColorIO.cpp:57-58 @ v2.5.2).
static SET_COMPUTE_HASH_FUNCTION: Function = function(
    "SetComputeHashFunction",
    &[Overload {
        params: &[Param::new(
            "hashFunction",
            "collections.abc.Callable[[str], str]",
        )],
        ret: "None",
        call: |py, _, a, convert| {
            let Some(f) = function_arg(&a[0], convert) else {
                return Ok(None);
            };
            ocio::path_utils::set_compute_hash_function(python_hash_function(f));
            none(py)
        },
        pos_only: 0,
    }],
);

// m.def("ResetComputeHashFunction", &ResetComputeHashFunction) (PyOpenColorIO.cpp:59-60 @
// v2.5.2).
static RESET_COMPUTE_HASH_FUNCTION: Function = function(
    "ResetComputeHashFunction",
    &[Overload {
        params: &[],
        ret: "None",
        call: |py, _, _, _| {
            ocio::path_utils::reset_compute_hash_function();
            none(py)
        },
        pos_only: 0,
    }],
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

    crate::pybind11::def_function(m, &CLEAR_ALL_CACHES)?;
    crate::pybind11::def_function(m, &GET_VERSION)?;
    crate::pybind11::def_function(m, &GET_VERSION_HEX)?;
    crate::pybind11::def_function(m, &GET_LOGGING_LEVEL)?;
    crate::pybind11::def_function(m, &SET_LOGGING_LEVEL)?;
    crate::pybind11::def_function(m, &SET_LOGGING_FUNCTION)?;
    crate::pybind11::def_function(m, &RESET_TO_DEFAULT_LOGGING_FUNCTION)?;
    crate::pybind11::def_function(m, &LOG_MESSAGE)?;
    crate::pybind11::def_function(m, &SET_COMPUTE_HASH_FUNCTION)?;
    crate::pybind11::def_function(m, &RESET_COMPUTE_HASH_FUNCTION)?;
    crate::pybind11::def_function(m, &GET_ENV_VARIABLE)?;
    crate::pybind11::def_function(m, &SET_ENV_VARIABLE)?;
    crate::pybind11::def_function(m, &UNSET_ENV_VARIABLE)?;
    crate::pybind11::def_function(m, &IS_ENV_VARIABLE_PRESENT)?;

    // The classes' bindings (PyOpenColorIO.cpp:70-103), those ported so far.
    crate::py_builtin_config_registry::bind_py_builtin_config_registry(m)?;
    crate::py_color_space::bind_py_color_space(m)?;
    crate::py_color_space_set::bind_py_color_space_set(m)?;
    crate::py_config::bind_py_config(m)?;
    crate::py_context::bind_py_context(m)?;
    crate::py_config_io_proxy::bind_py_config_io_proxy(m)?;
    crate::py_cpu_processor::bind_py_cpu_processor(m)?;
    crate::py_file_rules::bind_py_file_rules(m)?;
    crate::py_gpu_shader_creator::bind_py_gpu_shader_creator(m)?;
    crate::py_image_desc::bind_py_image_desc(m)?;
    crate::py_look::bind_py_look(m)?;
    crate::py_named_transform::bind_py_named_transform(m)?;
    crate::py_processor_metadata::bind_py_processor_metadata(m)?;
    crate::py_viewing_rules::bind_py_viewing_rules(m)?;
    crate::py_view_transform::bind_py_view_transform(m)?;
    crate::py_builtin_transform_registry::bind_py_builtin_transform_registry(m)?;
    crate::py_format_metadata::bind_py_format_metadata(m)?;
    crate::py_grading_data::bind_py_grading_data(m)?;
    crate::py_transform::bind_py_transform(m)?;
    Ok(())
}
