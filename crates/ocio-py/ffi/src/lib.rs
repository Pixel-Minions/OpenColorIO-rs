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
//! checks that each `unsafe` block is one of the calls below, or the wrapping of the new
//! reference one of them returned in the statement just before:
//! - `PyType_FromMetaclass`: a class with pybind11's metatype whose C name (`tp_name`) is the
//!   dotted `PyOpenColorIO.PyOpenColorIO.Name`, as pybind11's are. CPython writes it in some
//!   messages ("'<' not supported between instances of ...", "'...' object has no attribute
//!   ...", "bad operand type for unary ~: ...").
//! - `PyInstanceMethod_New`: CPython's `instancemethod`, which pybind11 wraps each method in;
//! - `PyCFunction_NewEx`: a builtin function whose `__self__` is pybind11's function record,
//!   which CPython names in the function's `__qualname__` and repr;
//! - `PySequence_Check`: whether an object is a sequence as pybind11's sequence casters ask it
//!   (the type's `sq_item` slot), which no Python-level check answers exactly;
//! - `PyType_GetSlot`, compared only against null, for the number slots `nb_bool`, `nb_index`,
//!   `nb_int` and `nb_float` only (owner decision 2026-10-21, p6-core re-check round 4): whether
//!   a type has one, as pybind11's `bool` caster and `PyNumber_Check` ask it, which no
//!   Python-level check answers exactly (a static type may show an inherited `__bool__` in its
//!   MRO without the slot, as `numpy.bytes_`).
//! - The views of Python buffers (owner decision P6-10, card p6-buffers), as pybind11's
//!   `py::buffer` reads them: `PyObject_CheckBuffer`, whether an object exports a buffer;
//!   `PyObject_GetBuffer` and `PyBuffer_Release`, an export as `py::buffer::request()` asks for
//!   it, held by a [`Buffer`]; `std::ptr::read`, `CStr::from_ptr` and
//!   `std::slice::from_raw_parts`, the copy of what the exporter filled in; `Box::from_raw`, the
//!   freeing of the view's allocation; and `std::slice::from_raw_parts` and `from_raw_parts_mut`
//!   over the span of a buffer's items, which [`with_bytes`] lends only while the interpreter is
//!   detached, never two that share a byte when one of them writes.
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

/// The number slots [`has_number_slot`] answers for, the only ones (owner decision 2026-10-21).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberSlot {
    /// `nb_bool` (`__bool__`).
    Bool,
    /// `nb_index` (`__index__`).
    Index,
    /// `nb_int` (`__int__`).
    Int,
    /// `nb_float` (`__float__`).
    Float,
}

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

/// Whether `ty` has the number slot `slot`: `tp_as_number && tp_as_number->nb_...`, as
/// pybind11's `bool` caster reads `nb_bool` and `PyNumber_Check` the others.
pub fn has_number_slot(ty: &Bound<'_, PyType>, slot: NumberSlot) -> bool {
    let id = match slot {
        NumberSlot::Bool => ffi::Py_nb_bool,
        NumberSlot::Index => ffi::Py_nb_index,
        NumberSlot::Int => ffi::Py_nb_int,
        NumberSlot::Float => ffi::Py_nb_float,
    };
    // SAFETY: the interpreter is attached (`ty` is bound to it) and `ty` is a live type object.
    // Since Python 3.10 PyType_GetSlot reads any type's slot (static ones too); `id` is one of
    // the number slots' ids, valid, so the call sets no exception: it returns the slot's
    // function pointer, or null when the type has no number methods or not that one. The
    // pointer is only compared against null, never called or read.
    let pointer = unsafe { ffi::PyType_GetSlot(ty.as_type_ptr(), id) };
    !pointer.is_null()
}

// ---------------------------------------------------------------------------------------------
// Python buffers (owner decision P6-10): the memory of an object that exports a buffer, as
// pybind11's `py::buffer` reads it, lent as slices while the interpreter is detached.

/// `PyObject_CheckBuffer(obj)`: whether `obj`'s type exports buffers (its `bf_getbuffer` slot),
/// as pybind11's `py::buffer` caster asks before it takes an argument.
pub fn check_buffer(obj: &Bound<'_, PyAny>) -> bool {
    // SAFETY: the interpreter is attached (`obj` is bound to it) and `obj` is a live object;
    // PyObject_CheckBuffer only reads its type's buffer slot, takes no reference and can't
    // fail.
    let check = unsafe { ffi::PyObject_CheckBuffer(obj.as_ptr()) };
    check == 1
}

/// What a [`Buffer`]'s exporter filled in, as pybind11's `buffer_info` reads it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BufferInfo {
    /// The address of the item at index 0 (`buf`).
    pub ptr: usize,
    /// The bytes of an item.
    pub itemsize: isize,
    /// The items' format, in the `struct` module's syntax.
    pub format: Vec<u8>,
    /// The number of dimensions.
    pub ndim: isize,
    /// The items along each dimension.
    pub shape: Vec<isize>,
    /// The bytes from an item to the next along each dimension: those of a C-contiguous array
    /// of `shape` when the exporter gave none (ctypes doesn't, though asked to), as pybind11
    /// makes them (`detail::c_strides`).
    pub strides: Vec<isize>,
    /// Whether the exporter marked the memory read-only.
    pub readonly: bool,
}

/// A Python object's buffer as pybind11's `py::buffer::request()` gets it: exported by the
/// object (`PyObject_GetBuffer` with `PyBUF_STRIDES | PyBUF_FORMAT`: strides and the format, not
/// writable, no suboffsets) until dropped (`PyBuffer_Release`). While the export lasts, an
/// exporter that keeps the buffer protocol keeps its items' memory where [`Buffer::info`] says
/// (CPython refuses to resize a `bytearray` that has an export, NumPy an array, `mmap` to close).
/// Not every exporter does: `ctypes.resize(obj, n)` and `ndarray.resize(..., refcheck=False)`
/// reallocate the memory while an export of it lasts, as they do under the wheel's raw pointer.
#[derive(Debug)]
pub struct Buffer {
    /// The view the exporter filled, allocated in [`Buffer::request`] and freed on drop: at a
    /// fixed address, since an exporter may point into it (`PyBuffer_FillInfo` points `shape`
    /// at `len`).
    view: *mut ffi::Py_buffer,
    /// Whether `view` holds an export, to end.
    exported: bool,
    info: BufferInfo,
    /// `buf`, from which the items are addressed.
    buf: *mut u8,
    /// The span of the items: the bytes from the lowest item's first byte to the highest item's
    /// last, as the offset of their start from `buf` (zero or negative) and their number (zero
    /// for a buffer without items).
    span: (isize, usize),
}

impl Buffer {
    /// pybind11's `py::buffer::request()` of `obj`: `PyObject_GetBuffer(obj, view, PyBUF_STRIDES |
    /// PyBUF_FORMAT)`, whose error goes through as it is.
    ///
    /// Refused (BufferError) once exported, which then ends: a view that breaks the buffer
    /// protocol where pybind11 would read memory the exporter didn't give (a null format; a
    /// null shape, or null memory, with items; a negative number of dimensions, dimension or
    /// item size; suboffsets, which the request doesn't ask for and without which the items
    /// aren't where `buf` and the strides place them), and items whose span doesn't fit in the
    /// address space.
    pub fn request(obj: &Bound<'_, PyAny>) -> PyResult<Buffer> {
        let py = obj.py();
        let view = Box::into_raw(Box::new(ffi::Py_buffer::new()));
        let mut buffer = Buffer {
            view,
            exported: false,
            info: BufferInfo::default(),
            buf: null_mut(),
            span: (0, 0),
        };
        let flags = ffi::PyBUF_STRIDES | ffi::PyBUF_FORMAT;
        // SAFETY: the interpreter is attached (`obj` is bound to it) and `obj` is a live object;
        // `view` points to the Py_buffer allocated above, which nothing else uses. On success
        // the exporter has filled it and holds an export (and a reference to the object, in
        // `obj`) until PyBuffer_Release, which `buffer`'s drop calls once; on failure it has set
        // an exception and holds nothing.
        let status = unsafe { ffi::PyObject_GetBuffer(obj.as_ptr(), view, flags) };
        if status != 0 {
            return Err(PyErr::fetch(py));
        }
        buffer.exported = true;
        // SAFETY: `view` points to the Py_buffer the exporter filled, alive until drop; this
        // copies its fields.
        let raw = unsafe { std::ptr::read(view) };
        let refused = |why: &str| {
            Err(pyo3::exceptions::PyBufferError::new_err(format!(
                "{why}: the view breaks the buffer protocol where pybind11 reads it \
                 (docs/improvements.md, U-157)"
            )))
        };
        if !raw.suboffsets.is_null() {
            return refused("the buffer has suboffsets, which pybind11's request doesn't ask for");
        }
        let format_ptr = raw.format.cast_const();
        if format_ptr.is_null() {
            return refused("the buffer has no format, which pybind11 reads");
        }
        // SAFETY: a format the exporter gave is a NUL-terminated string it keeps until the
        // export ends (the buffer protocol), which outlives this copy of it.
        let format = unsafe { CStr::from_ptr(format_ptr) }.to_bytes().to_vec();
        let ndim = raw.ndim as isize;
        let Ok(count) = usize::try_from(ndim) else {
            return refused("the buffer has a negative number of dimensions");
        };
        let shape_ptr = raw.shape.cast_const();
        let shape = if count == 0 {
            Vec::new()
        } else if shape_ptr.is_null() {
            return refused("the buffer has dimensions but no shape, which pybind11 reads");
        } else {
            // SAFETY: a shape the exporter gave is an array of `ndim` values it keeps until the
            // export ends (the buffer protocol); this copies it.
            unsafe { std::slice::from_raw_parts(shape_ptr, count) }.to_vec()
        };
        let strides_ptr = raw.strides.cast_const();
        let strides = if count == 0 {
            Vec::new()
        } else if strides_ptr.is_null() {
            c_strides(&shape, raw.itemsize)
        } else {
            // SAFETY: strides the exporter gave are an array of `ndim` values it keeps until the
            // export ends (the buffer protocol); this copies them.
            unsafe { std::slice::from_raw_parts(strides_ptr, count) }.to_vec()
        };
        if raw.itemsize < 0 || shape.iter().any(|&n| n < 0) {
            return refused("the buffer has a negative dimension or item size");
        }
        buffer.buf = raw.buf.cast::<u8>();
        buffer.info = BufferInfo {
            ptr: buffer.buf.addr(),
            itemsize: raw.itemsize,
            format,
            ndim,
            shape,
            strides,
            readonly: raw.readonly != 0,
        };
        let Some(span) = items_span(&buffer.info) else {
            return refused("the buffer's items don't fit in the address space");
        };
        if span.1 > 0 && buffer.buf.is_null() {
            return refused("the buffer has items but no memory");
        }
        buffer.span = span;
        Ok(buffer)
    }

    /// What the exporter filled in.
    pub fn info(&self) -> &BufferInfo {
        &self.info
    }

    /// The addresses `[start, end)` of the span of the items: the bytes from the lowest item's
    /// first byte to the highest item's last (empty for a buffer without items).
    pub fn span(&self) -> (usize, usize) {
        let start = self.buf.wrapping_offset(self.span.0).addr();
        (start, start + self.span.1)
    }

    /// Where the item at index 0 (`buf`) is in the span, in bytes from its start.
    pub fn span_offset(&self) -> usize {
        self.span.0.unsigned_abs()
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        let view = self.view;
        if self.exported {
            Python::attach(|_| {
                // SAFETY: the interpreter is attached; `view` holds this buffer's export, which
                // ends here, once.
                unsafe { ffi::PyBuffer_Release(view) };
            });
        }
        // SAFETY: `view` was allocated by `Box::into_raw` in `request`, and nothing uses it after
        // this.
        let boxed = unsafe { Box::from_raw(view) };
        drop(boxed);
    }
}

/// pybind11's `detail::c_strides(shape, itemsize)`: the strides of a C-contiguous array, its
/// products wrapping as C++'s do in the wheels.
fn c_strides(shape: &[isize], itemsize: isize) -> Vec<isize> {
    let mut strides = vec![itemsize; shape.len()];
    for i in (1..shape.len()).rev() {
        strides[i - 1] = strides[i].wrapping_mul(shape[i]);
    }
    strides
}

/// The span of `info`'s items, as [`Buffer`] holds it: the offset of the lowest item's first
/// byte from `ptr` and the bytes to the highest item's last; `None` when it leaves the address
/// space, or is larger than an allocation can be, or when its sums leave `i128` (16 or more
/// dimensions of about 2^62 items, which no allocation holds), all checked. Public for the
/// binding's tests, which give it views no Python exporter makes.
#[doc(hidden)]
pub fn items_span(info: &BufferInfo) -> Option<(isize, usize)> {
    if info.shape.contains(&0) {
        return Some((0, 0));
    }
    let (mut low, mut high) = (0i128, info.itemsize as i128);
    for (&n, &stride) in info.shape.iter().zip(&info.strides) {
        let last = (n as i128 - 1).checked_mul(stride as i128)?;
        low = low.checked_add(last.min(0))?;
        high = high.checked_add(last.max(0))?;
    }
    let start = (info.ptr as i128).checked_add(low)?;
    let len = high.checked_sub(low)?;
    if start < 0 || len > isize::MAX as i128 || start.checked_add(len)? > usize::MAX as i128 {
        return None;
    }
    Some((isize::try_from(low).ok()?, usize::try_from(len).ok()?))
}

/// Memory [`with_bytes`] lends: the spans of one or more buffers that share bytes, as one slice
/// (their union, which lies in the one allocation they share), to read or to write.
#[derive(Debug)]
pub enum Bytes<'a> {
    /// The memory, to read.
    Read(&'a [u8]),
    /// The memory, to write (and read).
    Write(&'a mut [u8]),
}

/// A memory region [`with_bytes`] lends: the addresses `[start, end)`, and whether to write.
type Region = (usize, usize, bool);

/// The regions lent now, by every thread.
static LENT: Mutex<Vec<Region>> = Mutex::new(Vec::new());

/// The regions one [`with_bytes`] lent, given back when dropped (also when `f` panics).
struct Lent(Vec<Region>);

impl Drop for Lent {
    fn drop(&mut self) {
        let mut lent = LENT.lock().unwrap_or_else(|e| e.into_inner());
        for region in &self.0 {
            if let Some(i) = lent.iter().position(|r| r == region) {
                lent.remove(i);
            }
        }
    }
}

/// Whether two regions `[start, end)` share a byte.
fn overlap(a: &Region, b: &Region) -> bool {
    a.0 < a.1 && b.0 < b.1 && a.0 < b.1 && b.0 < a.1
}

/// Runs `f` with the interpreter detached (the GIL released, `Python::detach`) over the memory
/// of `buffers`, each lent to read, or to write (`true`). Memory that is written is lent once:
/// spans ([`Buffer::span`]) that share bytes with a span lent to write, directly or through
/// others, are lent together as one region to write, their union; any other span is a region of
/// its own, to read. `f` gets the regions, and for each buffer, in order, its region and where
/// its item at index 0 (`buf`) is in it, in bytes. The slices exist only while `f` runs, with
/// the GIL released, so no Python code runs on this thread meanwhile; Python code that other
/// threads run then can still reach the memory, as it can while the wheel's C++ code runs.
///
/// Refused (BufferError), before `f` runs: a buffer lent to write whose exporter marked it
/// read-only; a region that shares a byte with one another call lends now (on any thread),
/// when either is lent to write. That is judged by span: two calls whose items are disjoint but
/// whose spans interleave (the left and right halves of an image's rows) are refused, though the
/// wheel doesn't race there (docs/improvements.md, U-150).
pub fn with_bytes<R: Send>(
    py: Python<'_>,
    buffers: &[(&Buffer, bool)],
    f: impl for<'b> FnOnce(Vec<Bytes<'b>>, Vec<(usize, usize)>) -> R + Send,
) -> PyResult<R> {
    let refused = |why: &str| Err(pyo3::exceptions::PyBufferError::new_err(why.to_string()));
    if buffers
        .iter()
        .any(|(b, write)| *write && b.info.readonly && b.span.1 > 0)
    {
        return refused(
            "the buffer is read-only, and the wheel writes into it (docs/improvements.md, U-156)",
        );
    }
    // Each buffer's group (the lowest index among its members), merged until no group that
    // writes shares a byte with another group.
    let spans: Vec<Region> = buffers
        .iter()
        .map(|(b, write)| {
            let (start, end) = b.span();
            (start, end, *write)
        })
        .collect();
    let mut group: Vec<usize> = (0..buffers.len()).collect();
    let hull = |group: &[usize], g: usize| -> Region {
        let members = spans
            .iter()
            .zip(group)
            .filter(|(span, k)| **k == g && span.0 < span.1);
        let (mut start, mut end, mut write) = (usize::MAX, 0, false);
        for (span, _) in members {
            start = start.min(span.0);
            end = end.max(span.1);
            write |= span.2;
        }
        if start >= end {
            (0, 0, write)
        } else {
            (start, end, write)
        }
    };
    loop {
        let mut merged = false;
        for i in 0..buffers.len() {
            for j in 0..buffers.len() {
                let (gi, gj) = (group[i], group[j]);
                if gi == gj {
                    continue;
                }
                let (a, b) = (hull(&group, gi), hull(&group, gj));
                if (a.2 || b.2) && overlap(&a, &b) {
                    let into = gi.min(gj);
                    for g in group.iter_mut() {
                        if *g == gi || *g == gj {
                            *g = into;
                        }
                    }
                    merged = true;
                }
            }
        }
        if !merged {
            break;
        }
    }
    // The regions in the order of their groups, each with the buffer whose span starts it.
    let mut regions: Vec<(Region, usize)> = Vec::new();
    let mut region_of = vec![0; buffers.len()];
    for i in 0..buffers.len() {
        if group[i] != i {
            region_of[i] = region_of[group[i]];
            continue;
        }
        let members: Vec<usize> = (0..buffers.len()).filter(|&k| group[k] == i).collect();
        let first = members
            .iter()
            .copied()
            .filter(|&k| spans[k].0 < spans[k].1)
            .min_by_key(|&k| spans[k].0)
            .unwrap_or(i);
        let region = if members.len() == 1 {
            spans[i]
        } else {
            hull(&group, i)
        };
        regions.push((region, first));
        region_of[i] = regions.len() - 1;
    }
    let lent = {
        let mut now = LENT.lock().unwrap_or_else(|e| e.into_inner());
        let busy = regions
            .iter()
            .any(|(r, _)| now.iter().any(|o| (r.2 || o.2) && overlap(r, o)));
        if busy {
            return refused(
                "another call (of another thread) is using memory in the span of the buffer's \
                 items (from the lowest item to the highest), with one of the calls writing; the \
                 wheel would race there only where items overlap, which the port doesn't tell \
                 (docs/improvements.md, U-150)",
            );
        }
        let mine: Vec<Region> = regions.iter().map(|(r, _)| *r).collect();
        now.extend(mine.iter().copied());
        Lent(mine)
    };
    let places: Vec<(usize, usize)> = buffers
        .iter()
        .zip(&region_of)
        .map(|((buffer, _), &r)| {
            let start = buffer.span().0 - regions[r].0.0;
            (r, start + buffer.span_offset())
        })
        .collect();
    let slices: Vec<Bytes<'_>> = regions
        .iter()
        .map(|&((start, end, write), first)| lend(buffers[first].0, end - start, write))
        .collect();
    let out = py.detach(move || f(slices, places));
    drop(lent);
    Ok(out)
}

/// `len` bytes from the start of `first`'s span, a region [`with_bytes`] lends, which checked
/// what it relies on.
fn lend(first: &Buffer, len: usize, write: bool) -> Bytes<'_> {
    if len == 0 {
        return if write {
            Bytes::Write(&mut [])
        } else {
            Bytes::Read(&[])
        };
    }
    let start = first.buf.wrapping_offset(first.span.0);
    if write {
        // SAFETY: the region is the union of the spans of exports that `with_bytes`' buffers
        // hold while the slice lives (it borrows `first`, the buffer whose span starts it, and
        // `with_bytes` borrows the others for longer); each span is the bytes from its lowest
        // item to its highest, all in the one allocation its exporter addresses the items in
        // from `buf` with fixed strides (without suboffsets, which `Buffer::request` refuses),
        // within the address space and an allocation's size (`items_span`); spans that share
        // bytes lie in the one allocation they share, so their union does too. Each buffer lent
        // to write wasn't marked read-only; a read-only span merged into the region shares its
        // allocation with a writable export's. No other slice this crate lends, on any thread,
        // shares a byte with this one while it lives (`LENT`).
        // What this relies on and can't check, as the wheel's C++ relies on it with its raw
        // pointer (docs/improvements.md, U-150):
        // - The exporter keeps that memory there until its export ends, as the buffer protocol
        //   asks. Not all do: `ctypes.resize(obj, n)` and `ndarray.resize(..., refcheck=False)`
        //   reallocate while an export lasts, so another thread (the GIL is released) can free
        //   the memory under a live slice, as it can under the wheel's pointer.
        // - No Python code another thread runs meanwhile writes memory in the slice. That
        //   includes the bytes between items (the gaps of a strided view), which the slice
        //   covers, and which the binding reads or writes only where the wheel does: another
        //   thread writing gap bytes the wheel doesn't touch races with nothing there, and here
        //   writes under a `&mut [u8]` (or a `&[u8]`) that the binding doesn't access there.
        let bytes = unsafe { std::slice::from_raw_parts_mut(start, len) };
        Bytes::Write(bytes)
    } else {
        let start = start.cast_const();
        // SAFETY: as above, for reading, with the same reliances: no slice this crate lends to
        // write shares a byte with this one while it lives (`LENT`).
        let bytes = unsafe { std::slice::from_raw_parts(start, len) };
        Bytes::Read(bytes)
    }
}
