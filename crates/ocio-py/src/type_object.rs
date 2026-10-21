// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The two CPython calls that make pybind11's types as the wheel has them, which no safe API
//! makes (owner decision 2026-10-20, "option A": `unsafe` in this file only, limited to these
//! two calls):
//! - `PyType_FromMetaclass`: a class with pybind11's metatype whose C name (`tp_name`) is the
//!   dotted `PyOpenColorIO.PyOpenColorIO.Name`, as pybind11's are. CPython writes it in some
//!   messages ("'<' not supported between instances of ...", "'...' object has no attribute
//!   ...", "bad operand type for unary ~: ...").
//! - `PyInstanceMethod_New`: CPython's `instancemethod`, which pybind11 wraps each method in.
#![allow(unsafe_code)]

use std::ffi::{CStr, CString, c_void};
use std::ptr::null_mut;

use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyTuple, PyType};

// On Windows, linked as pyo3-ffi links the interpreter's other functions: from the DLL of
// target/py-port's Python 3.13 (py-port/.python-version), by `raw-dylib`.
#[cfg_attr(windows, link(name = "python313", kind = "raw-dylib"))]
unsafe extern "C" {
    /// `PyObject *PyInstanceMethod_New(PyObject *func)` (Include/cpython/classobject.h), which
    /// pyo3-ffi doesn't declare.
    fn PyInstanceMethod_New(func: *mut ffi::PyObject) -> *mut ffi::PyObject;
}

/// A heap type made by `PyType_FromMetaclass(metatype, NULL, spec, bases)`, its spec:
/// - `name`, its `tp_name` (CPython takes `__name__` and `__qualname__` from the part after the
///   last dot, and `__module__` from the part before it);
/// - `tp_init` the slot function `init`, so that the type's dictionary starts with its own
///   `__init__` slot wrapper, as pybind11's do (it sets `tp_init` on every type);
/// - `tp_doc` empty (the wheel's docstrings are), and the flags default and base type;
/// - the instance size `basicsize` (pybind11's, at least the base's), the layout the base's.
///
/// Its dictionary then holds `__init__`, `__doc__` and `__module__`, in that order.
pub(crate) fn from_metaclass<'py>(
    metatype: &Bound<'py, PyType>,
    name: &str,
    bases: &Bound<'py, PyTuple>,
    init: *mut c_void,
    basicsize: usize,
) -> PyResult<Bound<'py, PyType>> {
    let py = metatype.py();
    let name: &'static CStr = Box::leak(
        CString::new(name)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?
            .into_boxed_c_str(),
    );
    let slots: &'static mut [ffi::PyType_Slot] = Box::leak(Box::new([
        ffi::PyType_Slot {
            slot: ffi::Py_tp_init,
            pfunc: init,
        },
        ffi::PyType_Slot {
            slot: ffi::Py_tp_doc,
            pfunc: c"".as_ptr().cast_mut().cast(),
        },
        ffi::PyType_Slot {
            slot: 0,
            pfunc: null_mut(),
        },
    ]));
    let spec: &'static mut ffi::PyType_Spec = Box::leak(Box::new(ffi::PyType_Spec {
        name: name.as_ptr(),
        basicsize: std::ffi::c_int::try_from(basicsize)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?,
        itemsize: 0,
        flags: (ffi::Py_TPFLAGS_DEFAULT | ffi::Py_TPFLAGS_BASETYPE) as std::ffi::c_uint,
        slots: slots.as_mut_ptr(),
    }));
    // SAFETY: the interpreter is attached (`py`); `metatype` and `bases` are live objects of
    // the right types (a type, a tuple of types) for the call's duration; `spec`, its name and
    // its zero-terminated slot array are leaked, so they outlive the type (CPython copies
    // the name and the docstring, and reads the rest during the call); `init` is a
    // `tp_init` function (PyO3's slot for `pybind11_object.__init__`). The call returns a new
    // reference, or null with an exception set.
    let ptr = unsafe {
        ffi::PyType_FromMetaclass(
            metatype.as_type_ptr(),
            null_mut(),
            std::ptr::from_mut(spec),
            bases.as_ptr(),
        )
    };
    // SAFETY: `ptr` is the call's result: a new reference we own, or null with an exception
    // set, which `from_owned_ptr_or_err` turns into the error.
    let cls = unsafe { Bound::from_owned_ptr_or_err(py, ptr) }?;
    Ok(cls.cast_into::<PyType>()?)
}

/// `PyInstanceMethod_New(f)`: `f` as a method, CPython's `instancemethod`.
pub(crate) fn instance_method<'py>(f: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    // SAFETY: the interpreter is attached (`f` is bound to it) and `f` is a live object;
    // PyInstanceMethod_New takes a new reference to it and returns a new reference, or null
    // with an exception set.
    let ptr = unsafe { PyInstanceMethod_New(f.as_ptr()) };
    // SAFETY: `ptr` is the call's result: a new reference we own, or null with an exception
    // set.
    unsafe { Bound::from_owned_ptr_or_err(f.py(), ptr) }
}
