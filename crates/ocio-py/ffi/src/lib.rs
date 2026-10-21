// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The CPython calls that make pybind11's types and functions as the wheel has them, which no
//! safe API makes: the only `unsafe` code of the Python binding (owner decisions 2026-10-20,
//! "option A", and 2026-10-21; a crate of its own since p6-core's re-check round 3).
//!
//! `ocio-py` forbids `unsafe_code` at its root, which no attribute or macro inside it can
//! relax, so the compiler holds the binding to the safe functions of this crate.
//! `cargo xtask guards` allows `unsafe` here only, refuses any other source file in this
//! crate, pins this file's SHA-256 (a change shows in review as a change of the pin), and
//! checks that each `unsafe` block is one of the four calls below, or the wrapping of the new
//! reference one of them returned in the statement just before:
//! - `PyType_FromMetaclass`: a class with pybind11's metatype whose C name (`tp_name`) is the
//!   dotted `PyOpenColorIO.PyOpenColorIO.Name`, as pybind11's are. CPython writes it in some
//!   messages ("'<' not supported between instances of ...", "'...' object has no attribute
//!   ...", "bad operand type for unary ~: ...").
//! - `PyInstanceMethod_New`: CPython's `instancemethod`, which pybind11 wraps each method in;
//! - `PyCFunction_NewEx`: a builtin function whose `__self__` is pybind11's function record,
//!   which CPython names in the function's `__qualname__` and repr;
//! - `PySequence_Check`: whether an object is a sequence as pybind11's sequence casters ask it
//!   (the type's `sq_item` slot), which no Python-level check answers exactly.
//!
//! Each function is sound whatever its arguments: what a call relies on (the types it is
//! given, the instance size, the receiver PyO3's C functions take unchecked) is checked here,
//! or follows from the function's signature.
#![cfg(feature = "python")]
#![allow(unsafe_code)]

use std::any::TypeId;
use std::ffi::{CStr, CString};
use std::ptr::null_mut;
use std::sync::Mutex;

use pyo3::PyClass;
use pyo3::ffi;
use pyo3::impl_::pymethods::PyMethodDefType;
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
/// - `tp_init` the slot function of `T`'s `__init__` (PyO3's), so that the type's dictionary
///   starts with its own `__init__` slot wrapper, as pybind11's do (it sets `tp_init` on every
///   type);
/// - `tp_doc` empty (the wheel's docstrings are), and the flags default and base type;
/// - the instance size `basicsize`, the layout the bases'.
///
/// Its dictionary then holds `__init__`, `__doc__` and `__module__`, in that order.
///
/// Refused (TypeError): a `metatype` that isn't a subclass of `type`, `bases` none of which
/// derives from `T` (PyO3's slot functions take their receiver as a `T` unchecked), a base
/// whose instances are larger than `basicsize`, and a `T` without `__init__`.
pub fn from_metaclass<'py, T: PyClass>(
    metatype: &Bound<'py, PyType>,
    name: &str,
    bases: &Bound<'py, PyTuple>,
    basicsize: usize,
) -> PyResult<Bound<'py, PyType>> {
    let py = metatype.py();
    let refused = |why: String| Err(pyo3::exceptions::PyTypeError::new_err(why));
    if !real_subclass(metatype, &py.get_type::<PyType>())? {
        return refused(format!("{name}: the metatype isn't a subclass of type"));
    }
    let target = T::type_object(py);
    let mut derives = false;
    for base in bases.iter() {
        let base = base.cast_into::<PyType>()?;
        let size: usize = type_descriptor(py, "__basicsize__")?
            .call_method1("__get__", (&base,))?
            .extract()?;
        if size > basicsize {
            return refused(format!(
                "{name}: a base's instances ({size} bytes) are larger than {basicsize} bytes"
            ));
        }
        derives |= real_subclass(&base, &target)?;
    }
    if !derives {
        return refused(format!(
            "{name}: no base derives from {}",
            <T as PyClass>::NAME
        ));
    }
    let Some(init) = T::items_iter()
        .flat_map(|items| items.slots)
        .find(|slot| slot.slot == ffi::Py_tp_init)
        .map(|slot| slot.pfunc)
    else {
        return refused(format!("{name}: {} has no __init__", <T as PyClass>::NAME));
    };
    let name: &'static CStr = leak_c_str(name)?;
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
    // the right types (a subclass of type, a tuple of types) for the call's duration; `spec`,
    // its name and its zero-terminated slot array are leaked, so they outlive the type
    // (CPython copies the name and the docstring, and reads the rest during the call); `init`
    // is PyO3's `tp_init` of `T`, which takes its receiver as a `T`: every instance of the type
    // is one (a base derives from `T`, in its MRO as CPython checks types), at least as large
    // as each base's. The call returns a new reference, or null with an exception set.
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

/// Whether `ty`'s MRO, as CPython checks types with it (`tp_mro`, read through the descriptor
/// `type.__mro__`, which no metatype overrides), holds `base`.
fn real_subclass(ty: &Bound<'_, PyType>, base: &Bound<'_, PyType>) -> PyResult<bool> {
    let mro = type_descriptor(ty.py(), "__mro__")?
        .call_method1("__get__", (ty,))?
        .cast_into::<PyTuple>()?;
    Ok(mro.iter().any(|k| k.is(base)))
}

/// The descriptor `name` of `type`'s own dictionary.
fn type_descriptor<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    py.get_type::<PyType>().getattr("__dict__")?.get_item(name)
}

/// `s` as a C string that lives as long as the process (CPython keeps pointers to names and
/// docstrings).
fn leak_c_str(s: &str) -> PyResult<&'static CStr> {
    Ok(Box::leak(
        CString::new(s)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?
            .into_boxed_c_str(),
    ))
}

/// `PyCFunction_NewEx(def, record, module)`: a builtin function, as pybind11 makes each, whose
/// C function is `T`'s method `method` (PyO3's, with its flags), named `name`, its docstring
/// `doc` (none for None), its `__self__` `record` and its `__module__` `module`.
///
/// Refused (TypeError): a `method` that isn't one of `T`'s instance methods (a class or static
/// method's C function takes its receiver as a type, unchecked).
pub fn new_function<'py, T: PyClass>(
    record: &Bound<'py, T>,
    method: &str,
    name: &str,
    doc: Option<&str>,
    module: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let mut def = method_def(record, method)?;
    def.ml_name = leak_c_str(name)?.as_ptr();
    def.ml_doc = match doc {
        Some(doc) => leak_c_str(doc)?.as_ptr(),
        None => std::ptr::null(),
    };
    function(record, Box::leak(Box::new(def)), module)
}

/// `T`'s instance methods' definitions (PyO3's trampolines and flags; class and static methods
/// left out).
fn instance_method_defs<T: PyClass>() -> Vec<ffi::PyMethodDef> {
    T::items_iter()
        .flat_map(|items| items.methods)
        .filter_map(|m| match m {
            PyMethodDefType::Method(def) => Some(def.into_raw()),
            _ => None,
        })
        .filter(|def| def.ml_flags & (ffi::METH_CLASS | ffi::METH_STATIC) == 0)
        .collect()
}

/// The index among [`instance_method_defs`] of each class's methods that [`method_def`]
/// found, by the class and the method's name.
static METHOD_DEFS: Mutex<Vec<(TypeId, String, usize)>> = Mutex::new(Vec::new());

/// `T`'s instance method `method`'s definition: the one whose function is named `method`
/// (a function made from each, with its own name, tells; the definitions' names are C
/// pointers).
fn method_def<T: PyClass>(record: &Bound<'_, T>, method: &str) -> PyResult<ffi::PyMethodDef> {
    let defs = instance_method_defs::<T>();
    let known = |list: &Vec<(TypeId, String, usize)>| {
        list.iter()
            .find(|(t, m, _)| *t == TypeId::of::<T>() && m == method)
            .map(|(_, _, i)| *i)
    };
    let cached = known(&METHOD_DEFS.lock().unwrap_or_else(|e| e.into_inner()));
    let index = match cached {
        Some(index) => Some(index),
        None => {
            let none = record.py().None().into_bound(record.py());
            let mut found = None;
            for (i, def) in defs.iter().enumerate() {
                let f = function(record, Box::leak(Box::new(*def)), &none)?;
                if f.getattr("__name__")?.to_string() == method {
                    found = Some(i);
                    break;
                }
            }
            if let Some(i) = found {
                METHOD_DEFS.lock().unwrap_or_else(|e| e.into_inner()).push((
                    TypeId::of::<T>(),
                    method.to_string(),
                    i,
                ));
            }
            found
        }
    };
    index.and_then(|i| defs.get(i).copied()).ok_or_else(|| {
        pyo3::exceptions::PyTypeError::new_err(format!(
            "{} has no instance method {method}",
            <T as PyClass>::NAME
        ))
    })
}

/// `PyCFunction_NewEx(def, record, module)`, `def` one of [`instance_method_defs`] of `T`.
fn function<'py, T: PyClass>(
    record: &Bound<'py, T>,
    def: &'static mut ffi::PyMethodDef,
    module: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    // SAFETY: the interpreter is attached (`record` is bound to it); `def` is leaked, so it
    // outlives the function, which keeps a pointer to it; its name and docstring are leaked C
    // strings or null, and its C function and flags are PyO3's for an instance method of `T`
    // (callers take it from `instance_method_defs::<T>`), which takes its receiver as a `T`
    // unchecked: `record`, an instance of `T` (`Bound<T>`). `record` and `module` are live
    // objects, of which the function takes new references. The call returns a new reference,
    // or null with an exception set.
    let ptr = unsafe {
        ffi::PyCFunction_NewEx(std::ptr::from_mut(def), record.as_ptr(), module.as_ptr())
    };
    // SAFETY: `ptr` is the call's result: a new reference we own, or null with an exception
    // set.
    unsafe { Bound::from_owned_ptr_or_err(record.py(), ptr) }
}

/// `PySequence_Check(obj)`: whether `obj`'s type has the sequence slot `sq_item` and isn't a
/// `dict` subclass, which pybind11's sequence casters check first, before any call into the
/// object.
pub fn sequence_check(obj: &Bound<'_, PyAny>) -> bool {
    // SAFETY: the interpreter is attached (`obj` is bound to it) and `obj` is a live object;
    // PySequence_Check only reads its type's flags and slots, takes no reference and can't
    // fail.
    let check = unsafe { ffi::PySequence_Check(obj.as_ptr()) };
    check == 1
}

/// `PyInstanceMethod_New(f)`: `f` as a method, CPython's `instancemethod`.
pub fn instance_method<'py>(f: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    // SAFETY: the interpreter is attached (`f` is bound to it) and `f` is a live object;
    // PyInstanceMethod_New takes a new reference to it and returns a new reference, or null
    // with an exception set.
    let ptr = unsafe { PyInstanceMethod_New(f.as_ptr()) };
    // SAFETY: `ptr` is the call's result: a new reference we own, or null with an exception
    // set.
    unsafe { Bound::from_owned_ptr_or_err(f.py(), ptr) }
}
