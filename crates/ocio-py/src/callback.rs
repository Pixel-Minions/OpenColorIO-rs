// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Python callables the library calls (chunk 6.6a): the logging function, the hash function and
//! a `PyConfigIOProxy`'s overrides. pybind11 calls them with the GIL taken (`gil_scoped_acquire`,
//! from any thread), and what they raise travels through the library as a C++ exception: a
//! Python exception as `error_already_set`, whose `what()` is the exception's type, message and
//! traceback; pybind11's own errors (a result its caster refuses, a pure virtual function) as
//! `std::runtime_error`s of their message. The port raises them as the library's
//! [`ForeignException`] (owner decision 2026-10-22, Q2 = U), carrying the Python exception; the
//! dispatcher ([`crate::dispatch::dispatch`]) catches it at the Python boundary and restores the
//! exception ([`into_py_err`]).

use ocio_ops::foreign_exception::{ForeignException, raise};
use pyo3::prelude::*;
use pyo3::types::PyType;

/// The payload of a callback's exception: the Python exception the caller gets.
pub(crate) struct CallbackPayload(pub(crate) PyErr);

/// How a callback failed: a Python exception (`error_already_set`), or one of pybind11's own C++
/// errors, already its Python exception (`what()` its message).
pub(crate) enum CallbackError {
    Python(PyErr),
    Cpp(PyErr),
}

/// pybind11's `error_already_set::what()` (`error_fetch_and_normalize::error_string`, pybind11
/// 3.0): the exception type's C name (with "[WITH __notes__]" when it has the attribute), ": ",
/// then [`format_value_and_trace`].
pub(crate) fn error_already_set_what(py: Python<'_>, err: &PyErr) -> Vec<u8> {
    let value = err.value(py);
    let ty: Bound<'_, PyType> = err.get_type(py);
    let mut out = crate::pybind11::tp_name(&ty)
        .unwrap_or_default()
        .into_bytes();
    // `PyObject_HasAttrString`: an error reading the attribute is no mark. Python 3.13's
    // reports it as unraisable first, which the port doesn't (docs/improvements.md, I-302).
    if value.hasattr("__notes__").unwrap_or(false) {
        out.extend_from_slice(b"[WITH __notes__]");
    }
    out.extend_from_slice(b": ");
    out.extend_from_slice(&format_value_and_trace(py, err));
    out
}

/// The text of `obj` (a `str`, or an instance of a subclass) as pybind11 reads it: `str()`
/// (or the object itself, a note), encoded as UTF-8 with backslashreplace
/// (`PyUnicode_AsEncodedString`: `str`'s own `encode`, never a subclass's).
fn utf8(obj: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    obj.py()
        .get_type::<pyo3::types::PyString>()
        .getattr("encode")?
        .call1((obj, "utf-8", "backslashreplace"))?
        .extract::<Vec<u8>>()
}

/// pybind11's `error_fetch_and_normalize::format_value_and_trace` (pybind11 3.0): the
/// exception's `str()` as UTF-8 with backslashreplace ("<MESSAGE UNAVAILABLE DUE TO ANOTHER
/// EXCEPTION>" when that raises); the notes of its `__notes__` list, each a `str` ("FAILURE
/// obtaining __notes__[i]: " and the error otherwise); "<EMPTY MESSAGE>" for an empty text;
/// then "\n\nAt:\n" and a line "  <file>(<line>): <function>" per frame, from the deepest of its
/// traceback outwards; and, when `str()` raised, "\nMESSAGE UNAVAILABLE DUE TO EXCEPTION: " and
/// that error's own [`error_already_set_what`] (after a line end of its own when there is no
/// traceback).
fn format_value_and_trace(py: Python<'_>, err: &PyErr) -> Vec<u8> {
    let value = err.value(py);
    let mut message_error = None;
    let mut result = match value.str().and_then(|s| utf8(s.as_any())) {
        Ok(text) => text,
        Err(e) => {
            message_error = Some(error_already_set_what(py, &e));
            b"<MESSAGE UNAVAILABLE DUE TO ANOTHER EXCEPTION>".to_vec()
        }
    };
    // Deviation D-10 (owner decision 2026-10-22): the wheel's `PyList_Size` of an attribute that
    // isn't a list fails, and its message holds "FAILURE obtaining len(__notes__): SystemError:"
    // with CPython's error text, the interpreter's build path in it. The port lists no notes.
    // `PyList_Size` and `PyList_GET_ITEM` of the attribute (a list's own items); a note that
    // isn't a `str` is `PyUnicode_AsEncodedString`'s `PyErr_BadArgument`, a TypeError without
    // a traceback.
    if let Ok(notes) = value.getattr("__notes__")
        && let Ok(notes) = notes.cast::<pyo3::types::PyList>()
    {
        result.extend_from_slice(format!("\n__notes__ (len={}):", notes.len()).as_bytes());
        for (i, note) in notes.iter().enumerate() {
            let encoded = if note.is_instance_of::<pyo3::types::PyString>() {
                utf8(&note)
            } else {
                Err(pyo3::exceptions::PyTypeError::new_err(
                    "bad argument type for built-in operation",
                ))
            };
            match encoded {
                Ok(text) => {
                    result.push(b'\n');
                    result.extend_from_slice(&text);
                }
                Err(e) => {
                    result.extend_from_slice(
                        format!("\nFAILURE obtaining __notes__[{i}]: ").as_bytes(),
                    );
                    result.extend_from_slice(&error_already_set_what(py, &e));
                }
            }
        }
    }
    if result.is_empty() {
        result = b"<EMPTY MESSAGE>".to_vec();
    }
    let trace = err.traceback(py);
    let have_trace = trace.is_some();
    if let Some(tb) = trace {
        let mut tb = tb.into_any();
        while let Ok(next) = tb.getattr("tb_next") {
            if next.is_none() {
                break;
            }
            tb = next;
        }
        result.extend_from_slice(b"\n\nAt:\n");
        let mut frame = tb.getattr("tb_frame").ok();
        while let Some(f) = frame.filter(|f| !f.is_none()) {
            let code = f.getattr("f_code").ok();
            let file = code
                .as_ref()
                .and_then(|c| c.getattr("co_filename").ok())
                .and_then(|n| n.extract::<String>().ok())
                .unwrap_or_default();
            let name = code
                .as_ref()
                .and_then(|c| c.getattr("co_name").ok())
                .and_then(|n| n.extract::<String>().ok())
                .unwrap_or_default();
            let line = f
                .getattr("f_lineno")
                .ok()
                .and_then(|l| l.extract::<i64>().ok())
                .unwrap_or(-1);
            result.extend_from_slice(format!("  {file}({line}): {name}\n").as_bytes());
            frame = f.getattr("f_back").ok();
        }
    }
    if let Some(message_error) = message_error {
        if !have_trace {
            result.push(b'\n');
        }
        result.extend_from_slice(b"\nMESSAGE UNAVAILABLE DUE TO EXCEPTION: ");
        result.extend_from_slice(&message_error);
    }
    result
}

/// Throws a callback's error through the library.
pub(crate) fn throw(py: Python<'_>, e: CallbackError) -> ! {
    let (what, err) = match e {
        CallbackError::Python(err) => (error_already_set_what(py, &err), err),
        CallbackError::Cpp(err) => {
            let what = err
                .value(py)
                .str()
                .map(|s| s.to_string().into_bytes())
                .unwrap_or_default();
            (what, err)
        }
    };
    raise(ForeignException::new(what, Box::new(CallbackPayload(err))))
}

/// The Python exception of a callback's error that reached the Python boundary, or the
/// unwind again when it isn't one (an application's own callbacks, in an embedding host).
pub(crate) fn into_py_err(e: ForeignException) -> PyErr {
    let what = e.what().to_vec();
    match e.into_payload().downcast::<CallbackPayload>() {
        Ok(p) => p.0,
        Err(other) => raise(ForeignException::new(what, other)),
    }
}
