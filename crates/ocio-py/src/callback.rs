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

/// pybind11's `error_already_set::what()` (`error_fetch_and_normalize`, pybind11 3.0): the
/// exception type's C name (with "[WITH __notes__]" when it has notes), ": ", its `str()` (UTF-8
/// with backslashreplace; "<EMPTY MESSAGE>" when empty), each note, then "\n\nAt:\n" and a line
/// "  <file>(<line>): <function>" per frame, from the deepest of its traceback outwards.
pub(crate) fn error_already_set_what(py: Python<'_>, err: &PyErr) -> Vec<u8> {
    let value = err.value(py);
    let ty: Bound<'_, PyType> = err.get_type(py);
    let mut out = crate::pybind11::tp_name(&ty)
        .unwrap_or_default()
        .into_bytes();
    let notes = value.getattr("__notes__").ok();
    if notes.is_some() {
        out.extend_from_slice(b"[WITH __notes__]");
    }
    out.extend_from_slice(b": ");
    let text = |obj: &Bound<'_, PyAny>| -> Option<Vec<u8>> {
        let s = obj.str().ok()?;
        let b = s
            .call_method1("encode", ("utf-8", "backslashreplace"))
            .ok()?;
        b.extract::<Vec<u8>>().ok()
    };
    let mut result = match text(value.as_any()) {
        Some(t) => t,
        None => b"<MESSAGE UNAVAILABLE DUE TO ANOTHER EXCEPTION>".to_vec(),
    };
    if let Some(notes) = notes
        && let Ok(n) = notes.len()
    {
        result.extend_from_slice(format!("\n__notes__ (len={n}):").as_bytes());
        for note in notes.try_iter().into_iter().flatten().flatten() {
            result.push(b'\n');
            result.extend_from_slice(&text(&note).unwrap_or_default());
        }
    }
    if result.is_empty() {
        result = b"<EMPTY MESSAGE>".to_vec();
    }
    if let Some(tb) = err.traceback(py) {
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
    out.extend_from_slice(&result);
    out
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
