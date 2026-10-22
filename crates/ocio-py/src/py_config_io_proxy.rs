// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyConfigIOProxy` (src/bindings/python/PyConfigIOProxy.cpp @ v2.5.2), chunk 6.6a: the
//! class `vector_of_uint8_t` that `py::bind_vector<std::vector<uint8_t>>` makes, with the
//! iterator class of pybind11's `make_iterator`.
//!
//! **`vector_of_uint8_t`** is pybind11's own binding of a C++ vector (`stl_bind.h`), as the
//! wheel shows it: its methods, their docstrings and argument names, their errors (an
//! `IndexError` and a `ValueError` without a message, the `RuntimeError` of an item its caster
//! refuses), its slices, its `repr` (each byte written as a character, the text then decoded
//! as UTF-8), and the conversions `py::implicitly_convertible` registers from a `list` and a
//! `bytearray`. An instance holds a [`ByteVector`], which also keeps the capacity the wheel's
//! C++ runtime gives the vector, so that an iterator of it knows when the wheel's iterator
//! would read memory the vector freed (U-133).

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use pyo3::prelude::*;
use pyo3::types::{
    PyByteArray, PyCapsule, PyList, PySlice, PySliceMethods, PyType, PyWeakrefReference,
};

use crate::dispatch::{Cast, Function, Overload, Param};
use crate::py_transform::{class, def, none, ok};
use crate::pybind11;
use crate::shared::{Shared, construct, holder};
use crate::{q, this_shared};

pub(crate) const VECTOR: &str = q!("vector_of_uint8_t");
const ITERATOR: &str = "pybind11_builtins.iterator";
const SELF: Param = Param::new("self", VECTOR);
const INT: &str = "typing.SupportsInt";
const ITERABLE: &str = "collections.abc.Iterable";

/// The attribute pybind11 gives a `py::module_local()` class: a capsule of its type record,
/// named after the compiler and its ABI (`PYBIND11_MODULE_LOCAL_ID`).
#[cfg(windows)]
const MODULE_LOCAL_ID: &str = "__pybind11_module_local_v11_msvc_md_mscver19__";
/// The attribute pybind11 gives a `py::module_local()` class: a capsule of its type record,
/// named after the compiler and its ABI (`PYBIND11_MODULE_LOCAL_ID`).
#[cfg(not(windows))]
const MODULE_LOCAL_ID: &str =
    "__pybind11_module_local_v11_system_libstdcpp_gxx_abi_1xxx_use_cxx11_abi_1__";

// ---------------------------------------------------------------------------------------------
// The vector

/// The identities of the buffers vectors allocate: a buffer is the memory an iterator reads.
static BUFFERS: AtomicU64 = AtomicU64::new(1);

/// A `std::vector<uint8_t>` as the wheel's C++ runtime keeps it: the bytes, and the capacity of
/// the buffer that holds them (MSVC's STL grows it by half, libstdc++ doubles it), with the
/// buffer's identity, which changes whenever the vector reallocates (freeing the old one).
#[derive(Debug, Default)]
pub(crate) struct ByteVector {
    data: Vec<u8>,
    capacity: usize,
    /// 0 for no buffer (a vector that never allocated, or freed its buffer).
    buffer: u64,
}

/// The capacity a vector of `size` bytes in a buffer of `capacity` gets when `n` more don't fit:
/// MSVC's `_Calculate_growth(size + n)` (the capacity and half of it, or the new size if more),
/// libstdc++'s `_M_check_len(n)` (the size and the larger of the size and `n`).
fn grown(capacity: usize, size: usize, n: usize) -> usize {
    if cfg!(windows) {
        (capacity + capacity / 2).max(size + n)
    } else {
        size + size.max(n)
    }
}

impl ByteVector {
    /// A vector of `bytes` in a buffer of exactly their size (a copy, `std::vector(const
    /// std::vector &)`).
    pub(crate) fn from_bytes(bytes: Vec<u8>) -> ByteVector {
        let mut v = ByteVector::default();
        v.reallocate(bytes.len());
        v.data = bytes;
        v
    }

    /// A new buffer of `capacity` (none for 0), the old one freed.
    fn reallocate(&mut self, capacity: usize) {
        self.capacity = capacity;
        self.buffer = if capacity == 0 {
            0
        } else {
            BUFFERS.fetch_add(1, Ordering::Relaxed)
        };
    }

    /// Room for `n` more bytes, growing the buffer as a single insertion does.
    fn make_room(&mut self, n: usize) {
        if n > self.capacity - self.data.len() {
            let capacity = grown(self.capacity, self.data.len(), n);
            self.reallocate(capacity);
        }
    }

    /// `push_back(x)`.
    fn push(&mut self, x: u8) {
        self.make_room(1);
        self.data.push(x);
    }

    /// `insert(begin() + i, x)`.
    fn insert(&mut self, i: usize, x: u8) {
        self.make_room(1);
        self.data.insert(i, x);
    }

    /// `insert(end(), first, last)` of `bytes`.
    fn append_range(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        self.make_room(bytes.len());
        self.data.extend_from_slice(bytes);
    }

    /// `reserve(n)`: more than `max_size()` (`PTRDIFF_MAX` bytes) throws `std::length_error`
    /// (pybind11's ValueError, "vector too long" from MSVC's STL, "vector::reserve" from
    /// libstdc++); a buffer that can't be had throws `std::bad_alloc` (MemoryError, "bad
    /// allocation" or "std::bad_alloc"); a buffer of exactly `n` replaces a smaller one.
    fn reserve(&mut self, n: usize) -> PyResult<()> {
        use pyo3::exceptions::{PyMemoryError, PyValueError};
        let (too_long, bad_alloc) = if cfg!(windows) {
            ("vector too long", "bad allocation")
        } else {
            ("vector::reserve", "std::bad_alloc")
        };
        if n > isize::MAX.unsigned_abs() {
            return Err(PyValueError::new_err(too_long));
        }
        if n > self.capacity {
            self.data
                .try_reserve_exact(n - self.data.len())
                .map_err(|_| PyMemoryError::new_err(bad_alloc))?;
            self.reallocate(n);
        }
        Ok(())
    }

    /// `shrink_to_fit()`: a buffer of exactly the size (none when empty) replaces a larger one.
    fn shrink_to_fit(&mut self) {
        if self.capacity > self.data.len() {
            self.reallocate(self.data.len());
            self.data.shrink_to_fit();
        }
    }
}

/// A vector shared by the Python objects that hold it (an instance and its iterators).
pub(crate) type ByteVectorRc = Shared<ByteVector>;

/// pybind11's `cast_error` of an object a caster refuses (`load_type`, without
/// `PYBIND11_DETAILED_ERROR_MESSAGES`): a RuntimeError naming the object's type as `str()` of
/// it prints.
pub(crate) fn cast_error(obj: &Bound<'_, PyAny>) -> PyErr {
    let ty = obj
        .get_type()
        .str()
        .map(|s| s.to_string())
        .unwrap_or_default();
    pyo3::exceptions::PyRuntimeError::new_err(format!(
        "Unable to cast Python instance of type {ty} to C++ type '?' (#define \
         PYBIND11_DETAILED_ERROR_MESSAGES or compile in debug mode for details)"
    ))
}

/// `h.cast<uint8_t>()`: the integer caster with conversions, or [`cast_error`].
fn cast_byte(obj: &Bound<'_, PyAny>) -> PyResult<u8> {
    <u8 as Cast>::load(obj, true).ok_or_else(|| cast_error(obj))
}

/// pybind11's `len_hint(it)`: `PyObject_LengthHint(it, 0)`, 0 when it errs (the error
/// cleared).
fn len_hint(obj: &Bound<'_, PyAny>) -> usize {
    let py = obj.py();
    py.import("operator")
        .and_then(|m| m.getattr("length_hint"))
        .and_then(|f| f.call1((obj, 0)))
        .and_then(|n| n.extract::<isize>())
        .ok()
        .filter(|n| *n >= 0)
        .map_or(0, |n| n.unsigned_abs())
}

/// Whether `obj` is iterable: pybind11's `iterable` caster (`PyObject_GetIter` succeeds, its
/// error cleared).
fn iterable(obj: &Bound<'_, PyAny>) -> bool {
    obj.try_iter().is_ok()
}

/// Whether an implicit conversion is running: pybind11's `implicitly_convertible` converters
/// don't nest (`currently_used`).
static CONVERTING: Mutex<bool> = Mutex::new(false);

/// A `const std::vector<uint8_t> &` argument: an instance of the class; with conversions, also
/// what `py::implicitly_convertible` converts (PyConfigIOProxy.cpp:55-56 @ v2.5.2): a `list` or a
/// `bytearray` (or a subclass), through the class (`vector_of_uint8_t(obj)`, whose error is
/// cleared and refuses the argument). `Ok(None)`: refused.
pub(crate) fn vector_arg(obj: &Bound<'_, PyAny>, convert: bool) -> PyResult<Option<ByteVectorRc>> {
    if let Some(rc) = holder::<ByteVector>(obj, VECTOR)? {
        return Ok(Some(rc));
    }
    if !convert || !(obj.is_instance_of::<PyList>() || obj.is_instance_of::<PyByteArray>()) {
        return Ok(None);
    }
    {
        let mut busy = CONVERTING
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if *busy {
            return Ok(None);
        }
        *busy = true;
    }
    let converted = class(obj.py(), VECTOR).and_then(|cls| cls.call1((obj,)));
    *CONVERTING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = false;
    match converted {
        Ok(v) => holder::<ByteVector>(&v, VECTOR),
        Err(_) => Ok(None),
    }
}

/// A vector argument `a[i]` ([`vector_arg`]), or the next overload.
macro_rules! vector_param {
    ($a:expr, $i:expr, $convert:expr) => {
        match vector_arg(&$a[$i], $convert)? {
            Some(rc) => rc,
            None => return Ok(None),
        }
    };
}

/// A `slice` argument `a[i]` (pybind11's caster: `PySlice_Check`), or the next overload.
macro_rules! slice_param {
    ($a:expr, $i:expr) => {
        match $a[$i].cast::<PySlice>() {
            Ok(s) => s.clone(),
            Err(_) => return Ok(None),
        }
    };
}

/// `wrap_i(i, n)` of `bind_vector`: a negative index counts from the end; `IndexError` (without
/// a message) outside the vector.
fn wrap_i(i: i64, n: usize) -> PyResult<usize> {
    let n = i64::try_from(n).unwrap_or(i64::MAX);
    let i = if i < 0 { i + n } else { i };
    if i < 0 || i >= n {
        return Err(pyo3::exceptions::PyIndexError::new_err(""));
    }
    Ok(usize::try_from(i).unwrap_or_default())
}

/// `slice.compute(size, ...)`: `PySlice_GetIndicesEx`, its error raised.
fn compute(slice: &Bound<'_, PySlice>, size: usize) -> PyResult<(i64, i64, usize)> {
    let len = isize::try_from(size).unwrap_or(isize::MAX);
    let ix = slice.indices(len)?;
    Ok((ix.start as i64, ix.step as i64, ix.slicelength))
}

/// A new instance of the class holding `v` (a returned `std::vector`, moved into the instance).
pub(crate) fn vector_out(py: Python<'_>, v: ByteVector) -> PyResult<Option<Py<PyAny>>> {
    crate::shared::out(py, VECTOR, &Shared::new(v))
}

/// The error of `__delitem__` of a slice whose pybind11 loop erases outside the vector, which
/// the wheel does (docs/improvements.md, U-132).
fn erase_outside() -> PyErr {
    pyo3::exceptions::PyValueError::new_err(
        "vector_of_uint8_t.__delitem__: a slice whose erasing loop leaves the vector, where the \
         wheel erases memory it doesn't own (docs/improvements.md, U-132)",
    )
}

// cl.def(init<>()), cl.def(init<const Vector &>(), "Copy constructor"), cl.def(init([](const
// iterable &it) {...})) (pybind11's vector_modifiers and bind_vector, as the wheel binds them).
static INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[SELF],
            ret: "None",
            call: |py, o, a, _| {
                construct(&a[0], o.params[0].ty, Shared::new(ByteVector::default()))?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[SELF, Param::new("arg0", VECTOR)],
            ret: "None",
            call: |py, o, a, convert| {
                let src = vector_param!(a, 1, convert);
                let copy = ByteVector::from_bytes(src.lock().data.clone());
                construct(&a[0], o.params[0].ty, Shared::new(copy))?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[SELF, Param::new("arg0", ITERABLE)],
            ret: "None",
            call: |py, o, a, _| {
                if !iterable(&a[1]) {
                    return Ok(None);
                }
                let mut v = ByteVector::default();
                v.reserve(len_hint(&a[1]))?;
                for item in a[1].try_iter()? {
                    v.push(cast_byte(&item?)?);
                }
                construct(&a[0], o.params[0].ty, Shared::new(v))?;
                none(py)
            },
            pos_only: 0,
        },
    ],
};

/// `self == other` and `self != other` (`cl.def(self == self)`, `cl.def(self != self)`).
macro_rules! comparison {
    ($name:literal, $eq:expr) => {
        Function {
            name: $name,
            constructor: false,
            method: true,
            operator: true,
            overloads: &[Overload {
                params: &[SELF, Param::new("arg0", VECTOR)],
                ret: "bool",
                call: |py, o, a, convert| {
                    let this = this_shared!(o, a, ByteVector);
                    let other = vector_param!(a, 1, convert);
                    let equal = this.same(&other) || this.lock().data == other.lock().data;
                    ok(py, equal == $eq)
                },
                pos_only: 0,
            }],
        }
    };
}

static EQ: Function = comparison!("__eq__", true);
static NE: Function = comparison!("__ne__", false);

/// A method of one overload.
macro_rules! vector_method {
    ($name:literal, [$($param:expr),*], $ret:expr, $call:expr) => {
        $crate::method_def!($name, [SELF $(, $param)*], $ret, $call)
    };
}

// cl.def("count", ..., arg("x"), "Return the number of times ``x`` appears in the list").
static COUNT: Function = vector_method!("count", [Param::new("x", INT)], "int", |py, o, a, c| {
    let this = this_shared!(o, a, ByteVector);
    let x = crate::arg!(a, 1, c, u8);
    let n = this.lock().data.iter().filter(|&&b| b == x).count();
    ok(py, n)
});

// cl.def("remove", ..., arg("x"), "Remove the first item from the list whose value is x. ..."):
// `value_error()` when there is none.
static REMOVE: Function =
    vector_method!("remove", [Param::new("x", INT)], "None", |py, o, a, c| {
        let this = this_shared!(o, a, ByteVector);
        let x = crate::arg!(a, 1, c, u8);
        let mut v = this.lock();
        match v.data.iter().position(|&b| b == x) {
            Some(at) => {
                v.data.remove(at);
                none(py)
            }
            None => Err(pyo3::exceptions::PyValueError::new_err("")),
        }
    });

// cl.def("__contains__", ..., arg("x"), "Return true the container contains ``x``").
static CONTAINS: Function = vector_method!(
    "__contains__",
    [Param::new("x", INT)],
    "bool",
    |py, o, a, c| {
        let this = this_shared!(o, a, ByteVector);
        let x = crate::arg!(a, 1, c, u8);
        let found = this.lock().data.contains(&x);
        ok(py, found)
    }
);

// cl.def("__repr__", ..., "Return the canonical string representation of this list."): the
// class's name, then each byte as `operator<<` writes a `uint8_t` (a character), the text
// decoded as UTF-8 (pybind11's `std::string` result).
static REPR: Function = vector_method!("__repr__", [], "str", |py, o, a, _| {
    let this = this_shared!(o, a, ByteVector);
    let mut text = b"vector_of_uint8_t[".to_vec();
    {
        let v = this.lock();
        for (i, &b) in v.data.iter().enumerate() {
            text.push(b);
            if i + 1 != v.data.len() {
                text.extend_from_slice(b", ");
            }
        }
    }
    text.push(b']');
    Ok(Some(
        crate::dispatch::std_string_to_py(py, &text)?
            .into_any()
            .unbind(),
    ))
});

// cl.def("append", ..., arg("x"), "Add an item to the end of the list").
static APPEND: Function =
    vector_method!("append", [Param::new("x", INT)], "None", |py, o, a, c| {
        let this = this_shared!(o, a, ByteVector);
        let x = crate::arg!(a, 1, c, u8);
        this.lock().push(x);
        none(py)
    });

// cl.def("clear", ..., "Clear the contents").
static CLEAR: Function = vector_method!("clear", [], "None", |py, o, a, _| {
    let this = this_shared!(o, a, ByteVector);
    this.lock().data.clear();
    none(py)
});

// cl.def("extend", [](Vector &v, const Vector &src) { v.insert(v.end(), src.begin(),
// src.end()); }, arg("L"), ...), cl.def("extend", [](Vector &v, const iterable &it) {...},
// arg("L"), ...): the iterable's items after `reserve(size + len_hint)`; an item the caster
// refuses erases those appended (and shrinks the buffer) before its error; another error keeps
// them.
static EXTEND: Function = Function {
    name: "extend",
    constructor: false,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[SELF, Param::new("L", VECTOR)],
            ret: "None",
            call: |py, o, a, convert| {
                let this = this_shared!(o, a, ByteVector);
                let src = vector_param!(a, 1, convert);
                let bytes = src.lock().data.clone();
                this.lock().append_range(&bytes);
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[SELF, Param::new("L", ITERABLE)],
            ret: "None",
            call: |py, o, a, _| {
                let this = this_shared!(o, a, ByteVector);
                if !iterable(&a[1]) {
                    return Ok(None);
                }
                let old_size = this.lock().data.len();
                let hint = len_hint(&a[1]);
                this.lock().reserve(old_size.saturating_add(hint))?;
                for item in a[1].try_iter()? {
                    let item = item?;
                    match cast_byte(&item) {
                        Ok(b) => this.lock().push(b),
                        Err(e) => {
                            let mut v = this.lock();
                            v.data.truncate(old_size);
                            v.shrink_to_fit();
                            return Err(e);
                        }
                    }
                }
                none(py)
            },
            pos_only: 0,
        },
    ],
};

// cl.def("insert", [](Vector &v, DiffType i, const T &x) {...}, arg("i"), arg("x"), "Insert
// an item at a given position."): a negative index counts from the end; the size itself is
// allowed.
static INSERT: Function = vector_method!(
    "insert",
    [Param::new("i", INT), Param::new("x", INT)],
    "None",
    |py, o, a, c| {
        let this = this_shared!(o, a, ByteVector);
        let i = crate::arg!(a, 1, c, i64);
        let x = crate::arg!(a, 2, c, u8);
        let mut v = this.lock();
        let n = i64::try_from(v.data.len()).unwrap_or(i64::MAX);
        let i = if i < 0 { i + n } else { i };
        if i < 0 || i > n {
            return Err(pyo3::exceptions::PyIndexError::new_err(""));
        }
        v.insert(usize::try_from(i).unwrap_or_default(), x);
        none(py)
    }
);

// cl.def("pop", [](Vector &v) {...}, "Remove and return the last item"), cl.def("pop",
// [wrap_i](Vector &v, DiffType i) {...}, arg("i"), "Remove and return the item at index
// ``i``").
static POP: Function = Function {
    name: "pop",
    constructor: false,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[SELF],
            ret: "int",
            call: |py, o, a, _| {
                let this = this_shared!(o, a, ByteVector);
                let popped = this.lock().data.pop();
                match popped {
                    Some(b) => ok(py, b),
                    None => Err(pyo3::exceptions::PyIndexError::new_err("")),
                }
            },
            pos_only: 0,
        },
        Overload {
            params: &[SELF, Param::new("i", INT)],
            ret: "int",
            call: |py, o, a, convert| {
                let this = this_shared!(o, a, ByteVector);
                let i = crate::arg!(a, 1, convert, i64);
                let mut v = this.lock();
                let at = wrap_i(i, v.data.len())?;
                let b = v.data.remove(at);
                ok(py, b)
            },
            pos_only: 0,
        },
    ],
};

// cl.def("__setitem__", [wrap_i](Vector &v, DiffType i, const T &t) {...}), cl.def(
// "__setitem__", [](Vector &v, const slice &slice, const Vector &value) {...}, "Assign list
// elements using a slice object"): the slice's elements one by one, from the value's (which
// may be the vector itself, read as it changes).
static SETITEM: Function = Function {
    name: "__setitem__",
    constructor: false,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[SELF, Param::new("arg0", INT), Param::new("arg1", INT)],
            ret: "None",
            call: |py, o, a, convert| {
                let this = this_shared!(o, a, ByteVector);
                let i = crate::arg!(a, 1, convert, i64);
                let t = crate::arg!(a, 2, convert, u8);
                let mut v = this.lock();
                let at = wrap_i(i, v.data.len())?;
                v.data[at] = t;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param::new("arg0", "slice"),
                Param::new("arg1", VECTOR),
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let this = this_shared!(o, a, ByteVector);
                let slice = slice_param!(a, 1);
                let value = vector_param!(a, 2, convert);
                let size = this.lock().data.len();
                let (mut start, step, slicelength) = compute(&slice, size)?;
                let source = if this.same(&value) {
                    None
                } else {
                    Some(value.lock().data.clone())
                };
                let value_len = source.as_ref().map_or(size, Vec::len);
                if slicelength != value_len {
                    return Err(pyo3::exceptions::PyRuntimeError::new_err(
                        "Left and right hand size of slice assignment have different sizes!",
                    ));
                }
                let mut v = this.lock();
                for i in 0..slicelength {
                    let b = match &source {
                        Some(s) => s[i],
                        None => v.data[i],
                    };
                    v.data[usize::try_from(start).unwrap_or_default()] = b;
                    start += step;
                }
                none(py)
            },
            pos_only: 0,
        },
    ],
};

// cl.def("__getitem__", [](const Vector &v, const slice &slice) -> Vector * {...}, arg("s"),
// "Retrieve list elements using a slice object"), cl.def("__getitem__", [wrap_i](Vector &v,
// DiffType i) -> T & {...}).
static GETITEM: Function = Function {
    name: "__getitem__",
    constructor: false,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[SELF, Param::new("s", "slice")],
            ret: VECTOR,
            call: |py, o, a, _| {
                let this = this_shared!(o, a, ByteVector);
                let slice = slice_param!(a, 1);
                let v = this.lock();
                let (mut start, step, slicelength) = compute(&slice, v.data.len())?;
                let mut seq = ByteVector::default();
                seq.reserve(slicelength)?;
                for _ in 0..slicelength {
                    seq.push(v.data[usize::try_from(start).unwrap_or_default()]);
                    start += step;
                }
                drop(v);
                vector_out(py, seq)
            },
            pos_only: 0,
        },
        Overload {
            params: &[SELF, Param::new("arg0", INT)],
            ret: "int",
            call: |py, o, a, convert| {
                let this = this_shared!(o, a, ByteVector);
                let i = crate::arg!(a, 1, convert, i64);
                let v = this.lock();
                let at = wrap_i(i, v.data.len())?;
                ok(py, v.data[at])
            },
            pos_only: 0,
        },
    ],
};

// cl.def("__delitem__", [wrap_i](Vector &v, DiffType i) {...}, "Delete the list elements at
// index ``i``"), cl.def("__delitem__", [](Vector &v, const slice &slice) {...}, "Delete list
// elements using a slice object"): pybind11 erases the slice's elements one at a time, moving
// `start` by `step - 1` after each, which is right for a positive step only; an erasure outside
// the vector is the port's error before any (U-132).
static DELITEM: Function = Function {
    name: "__delitem__",
    constructor: false,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[SELF, Param::new("arg0", INT)],
            ret: "None",
            call: |py, o, a, convert| {
                let this = this_shared!(o, a, ByteVector);
                let i = crate::arg!(a, 1, convert, i64);
                let mut v = this.lock();
                let at = wrap_i(i, v.data.len())?;
                v.data.remove(at);
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[SELF, Param::new("arg0", "slice")],
            ret: "None",
            call: |py, o, a, _| {
                let this = this_shared!(o, a, ByteVector);
                let slice = slice_param!(a, 1);
                let mut v = this.lock();
                let (start, step, slicelength) = compute(&slice, v.data.len())?;
                // The positions pybind11's loop erases at, each within the vector as it is
                // then.
                let mut at = Vec::with_capacity(slicelength);
                let (mut s, mut len) = (start, v.data.len() as i64);
                for _ in 0..slicelength {
                    if s < 0 || s >= len {
                        return Err(erase_outside());
                    }
                    at.push(s as usize);
                    len -= 1;
                    s += step - 1;
                }
                for i in at {
                    v.data.remove(i);
                }
                none(py)
            },
            pos_only: 0,
        },
    ],
};

// cl.def("__iter__", [](Vector &v) { return make_iterator<...>(v.begin(), v.end()); },
// keep_alive<0, 1>()).
static ITER: Function = vector_method!(
    "__iter__",
    [],
    "collections.abc.Iterator[int]",
    |py, o, a, _| {
        let this = this_shared!(o, a, ByteVector);
        let (buffer, end) = {
            let v = this.lock();
            (v.buffer, v.data.len())
        };
        let state = IteratorState {
            vector: this,
            _owner: a[0].clone().unbind(),
            buffer,
            end,
            at: Mutex::new((0, true)),
        };
        let it = iterator_class(py)?.call_method1("__new__", (iterator_class(py)?,))?;
        pybind11::set_value(&it, ITERATOR, Bound::new(py, state)?.into_any())?;
        Ok(Some(it.unbind()))
    }
);

// cl.def("__bool__", ..., "Check whether the list is nonempty").
static BOOL: Function = vector_method!("__bool__", [], "bool", |py, o, a, _| {
    let this = this_shared!(o, a, ByteVector);
    let nonempty = !this.lock().data.is_empty();
    ok(py, nonempty)
});

// cl.def("__len__", [](const Vector &vec) { return vec.size(); }).
static LEN: Function = vector_method!("__len__", [], "int", |py, o, a, _| {
    let this = this_shared!(o, a, ByteVector);
    let n = this.lock().data.len();
    ok(py, n)
});

// ---------------------------------------------------------------------------------------------
// make_iterator's class

/// The state of an iterator of a vector (`iterator_state`): the vector (kept alive with its
/// Python object, `keep_alive<0, 1>`), the buffer and the end the C++ iterators point into,
/// and the position with `first_or_done`.
#[pyclass(frozen, module = "pybind11_builtins", name = "_iterator_state")]
struct IteratorState {
    vector: ByteVectorRc,
    _owner: Py<PyAny>,
    buffer: u64,
    end: usize,
    at: Mutex<(usize, bool)>,
}

/// The error of an iterator that would read a byte where the vector it was made of no longer
/// holds one: a buffer the vector freed, or past its end (docs/improvements.md, U-133).
fn invalidated() -> PyErr {
    pyo3::exceptions::PyValueError::new_err(
        "iterator of vector_of_uint8_t: the vector reallocated or shrank since the iterator was \
         made, and the wheel's iterator reads memory the vector no longer holds there \
         (docs/improvements.md, U-133)",
    )
}

/// The class `make_iterator` makes the first time it is called (`class_<state>(handle(),
/// "iterator", module_local())`).
static ITERATOR_CLASS: pyo3::sync::PyOnceLock<Py<pyo3::types::PyType>> =
    pyo3::sync::PyOnceLock::new();

const IT_SELF: Param = Param::new("self", ITERATOR);

// .def("__iter__", [](state &s) -> state & { return s; }) (positional-only `self`).
static IT_ITER: Function = Function {
    name: "__iter__",
    constructor: false,
    method: true,
    operator: false,
    overloads: &[Overload {
        params: &[IT_SELF],
        ret: ITERATOR,
        call: |_, o, a, _| match pybind11::instance_of(&a[0], o.params[0].ty) {
            Some(it) => {
                pybind11::read_value(&it, ITERATOR)?;
                Ok(Some(it.unbind()))
            }
            None => Ok(None),
        },
        pos_only: 1,
    }],
};

// .def("__next__", [](state &s) -> ValueType {...}): StopIteration at the end, without reading.
static IT_NEXT: Function = Function {
    name: "__next__",
    constructor: false,
    method: true,
    operator: false,
    overloads: &[Overload {
        params: &[IT_SELF],
        ret: "int",
        call: |py, o, a, _| {
            let Some(it) = pybind11::instance_of(&a[0], o.params[0].ty) else {
                return Ok(None);
            };
            let value = pybind11::read_value(&it, ITERATOR)?;
            let state = value.cast::<IteratorState>()?.get();
            let mut at = state
                .at
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !at.1 {
                at.0 += 1;
            } else {
                at.1 = false;
            }
            if at.0 == state.end {
                at.1 = true;
                return Err(pyo3::exceptions::PyStopIteration::new_err(""));
            }
            let v = state.vector.lock();
            if v.buffer != state.buffer || at.0 >= v.data.len() {
                return Err(invalidated());
            }
            ok(py, v.data[at.0])
        },
        pos_only: 1,
    }],
};

/// The iterator class, made the first time.
fn iterator_class(py: Python<'_>) -> PyResult<Bound<'_, pyo3::types::PyType>> {
    let cls = ITERATOR_CLASS.get_or_try_init(py, || -> PyResult<_> {
        let cls = pybind11::new_scopeless_class(py, "iterator")?;
        cls.setattr(MODULE_LOCAL_ID, module_local_record(py)?)?;
        cls.setattr("_pybind11_conduit_v1_", crate::enums::conduit(py)?)?;
        def(&cls, &IT_ITER)?;
        def(&cls, &IT_NEXT)?;
        Ok(cls.unbind())
    })?;
    Ok(cls.bind(py).clone())
}

/// A method with the docstrings of its overloads' definitions.
fn def_doc(
    cls: &Bound<'_, pyo3::types::PyType>,
    f: &'static Function,
    docs: &[Option<&str>],
) -> PyResult<()> {
    cls.setattr(
        f.name,
        pybind11::method_with_overload_docs(cls.py(), f, docs)?,
    )
}

/// Port of `py::bind_vector<std::vector<uint8_t>>(m, "vector_of_uint8_t")` and
/// `py::implicitly_convertible` from `py::list` and `py::bytearray` (PyConfigIOProxy.cpp:54-56
/// @ v2.5.2): the class, `module_local` as pybind11 makes a vector of a builtin type, its
/// methods in pybind11's order.
fn bind_vector(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let cls = pybind11::new_class(
        py,
        "vector_of_uint8_t",
        "vector_of_uint8_t",
        &[],
        &[("__doc__", py.None().into_bound(py))],
    )?;
    cls.setattr(MODULE_LOCAL_ID, module_local_record(py)?)?;
    cls.setattr("_pybind11_conduit_v1_", crate::enums::conduit(py)?)?;
    def_doc(&cls, &INIT, &[None, Some("Copy constructor"), None])?;
    def(&cls, &EQ)?;
    cls.setattr("__hash__", py.None())?;
    def(&cls, &NE)?;
    def_doc(
        &cls,
        &COUNT,
        &[Some("Return the number of times ``x`` appears in the list")],
    )?;
    def_doc(
        &cls,
        &REMOVE,
        &[Some(
            "Remove the first item from the list whose value is x. It is an error if there is \
             no such item.",
        )],
    )?;
    def_doc(
        &cls,
        &CONTAINS,
        &[Some("Return true the container contains ``x``")],
    )?;
    def_doc(
        &cls,
        &REPR,
        &[Some(
            "Return the canonical string representation of this list.",
        )],
    )?;
    def_doc(&cls, &APPEND, &[Some("Add an item to the end of the list")])?;
    def_doc(&cls, &CLEAR, &[Some("Clear the contents")])?;
    let extend = "Extend the list by appending all the items in the given list";
    def_doc(&cls, &EXTEND, &[Some(extend), Some(extend)])?;
    def_doc(
        &cls,
        &INSERT,
        &[Some("Insert an item at a given position.")],
    )?;
    def_doc(
        &cls,
        &POP,
        &[
            Some("Remove and return the last item"),
            Some("Remove and return the item at index ``i``"),
        ],
    )?;
    def_doc(
        &cls,
        &SETITEM,
        &[None, Some("Assign list elements using a slice object")],
    )?;
    def_doc(
        &cls,
        &GETITEM,
        &[Some("Retrieve list elements using a slice object"), None],
    )?;
    def_doc(
        &cls,
        &DELITEM,
        &[
            Some("Delete the list elements at index ``i``"),
            Some("Delete list elements using a slice object"),
        ],
    )?;
    def(&cls, &ITER)?;
    def_doc(&cls, &BOOL, &[Some("Check whether the list is nonempty")])?;
    def(&cls, &LEN)?;
    m.setattr("vector_of_uint8_t", &cls)?;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// PyConfigIOProxy

/// The C++ value of a `PyConfigIOProxy` instance: the trampoline `PyConfigIOProxy`
/// (PyConfigIOProxy.cpp:18-50 @ v2.5.2), which holds nothing of its own; its virtual functions
/// call the Python overrides of the instance pybind11 registered for it ([`get_override`]).
#[derive(Debug, Default)]
pub(crate) struct ProxyNode;

/// The types pybind11 found to have no Python override of a function (`get_type_override`'s
/// `inactive_override_cache`), by the type's address and the function's name, each with a
/// weak reference to the type, so that a type that died doesn't answer for another made at its
/// address.
static INACTIVE_OVERRIDES: Mutex<Vec<(usize, &'static str, Py<PyWeakrefReference>)>> =
    Mutex::new(Vec::new());

/// Whether the cache holds `(ty, name)`, dropping the entries of types that died.
fn inactive(ty: &Bound<'_, PyType>, name: &str) -> bool {
    let py = ty.py();
    let mut cache = INACTIVE_OVERRIDES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    cache.retain(|(_, _, w)| w.bind(py).upgrade().is_some());
    let key = ty.as_ptr() as usize;
    cache.iter().any(|(k, n, _)| *k == key && *n == name)
}

/// `function override = getattr(self, name, function())`: the attribute, or none when getting
/// it fails (the error cleared); an attribute Python can't call is a TypeError ("Object of type
/// '<C name>' is not an instance of 'function'", pybind11's `function` check).
fn override_attr<'py>(this: &Bound<'py, PyAny>, name: &str) -> PyResult<Option<Bound<'py, PyAny>>> {
    let Ok(attr) = this.getattr(name) else {
        return Ok(None);
    };
    if !attr.is_callable() {
        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "Object of type '{}' is not an instance of 'function'",
            pybind11::tp_name(&attr.get_type())?
        )));
    }
    Ok(Some(attr))
}

/// `override.is_cpp_function()`: the function of a method (an `instancemethod` or a bound
/// method, one level, `detail::get_function`) is a C function (`PyCFunction_Check`).
fn is_cpp_function(f: &Bound<'_, PyAny>) -> PyResult<bool> {
    let py = f.py();
    let types = py.import("types")?;
    let kind = f.get_type().name()?;
    let function = if kind == "instancemethod" || f.is_instance(&types.getattr("MethodType")?)? {
        f.getattr("__func__")?
    } else {
        f.clone()
    };
    function.is_instance(&types.getattr("BuiltinFunctionType")?)
}

/// Whether the calling Python frame is the override itself, called on `this`: its code named
/// `name` with a positional parameter, whose first local is `this` (`get_type_override`'s
/// recursion check, through `PyThreadState_GetFrame` and `PyEval_GetFrameLocals`).
fn called_from_override(this: &Bound<'_, PyAny>, name: &str) -> PyResult<bool> {
    let py = this.py();
    let Ok(frame) = py.import("sys")?.call_method1("_getframe", (0,)) else {
        return Ok(false);
    };
    let code = frame.getattr("f_code")?;
    let co_name: String = code.getattr("co_name")?.str()?.to_string();
    let argcount: isize = code.getattr("co_argcount")?.extract()?;
    if co_name != name || argcount <= 0 {
        return Ok(false);
    }
    let locals = frame.getattr("f_locals")?;
    let first = code.getattr("co_varnames")?.get_item(0)?;
    let caller = locals.call_method1("get", (first,))?;
    Ok(caller.is(this))
}

/// `get_override(this, name)` of the proxy `this` (`get_type_override`, pybind11 3.0): its
/// Python override of the virtual function `name`, or none: the type known to have none; an
/// attribute that is a C function (pybind11's own binding: the type is then remembered); a call
/// from the override itself on the same instance.
pub(crate) fn get_override<'py>(
    this: &Bound<'py, PyAny>,
    name: &'static str,
) -> PyResult<Option<Bound<'py, PyAny>>> {
    let ty = this.get_type();
    if inactive(&ty, name) {
        return Ok(None);
    }
    let attr = override_attr(this, name)?;
    if let Some(f) = &attr
        && is_cpp_function(f)?
    {
        let weak = PyWeakrefReference::new(&ty)?.unbind();
        INACTIVE_OVERRIDES
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push((ty.as_ptr() as usize, name, weak));
        return Ok(None);
    }
    if called_from_override(this, name)? {
        return Ok(None);
    }
    Ok(attr)
}

/// pybind11's error of a pure virtual function without a Python override.
pub(crate) fn pure_virtual(name: &str) -> PyErr {
    pyo3::exceptions::PyRuntimeError::new_err(format!(
        "Tried to call pure virtual function \"ConfigIOProxy::{name}\""
    ))
}

/// An override's result as a `std::string` (`cast_safe<std::string>`): pybind11's string
/// caster with conversions (`str` as strict UTF-8, `bytes`, `bytearray`), or [`cast_error`].
pub(crate) fn cast_string(result: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    <crate::dispatch::StdString as Cast>::load(result, true)
        .map(|s| s.0)
        .ok_or_else(|| cast_error(result))
}

/// An override's result as a `std::vector<uint8_t>` (`cast_safe`): an instance of the class,
/// moved from when nothing else refers to it, else copied; a `list` or a `bytearray` through
/// the implicit conversion (its temporary moved from); else [`cast_error`].
pub(crate) fn cast_vector(result: &Bound<'_, PyAny>) -> PyResult<ByteVector> {
    if let Some(rc) = holder::<ByteVector>(result, VECTOR)? {
        // `object::ref_count()`: what PyO3 deprecates for `ffi::Py_REFCNT`, which is unsafe.
        #[allow(deprecated)]
        let unreferenced = result.get_refcnt() == 1;
        if unreferenced {
            return Ok(std::mem::take(&mut *rc.lock()));
        }
        return Ok(ByteVector::from_bytes(rc.lock().data.clone()));
    }
    match vector_arg(result, true)? {
        Some(rc) => Ok(std::mem::take(&mut *rc.lock())),
        None => Err(cast_error(result)),
    }
}

const PROXY: &str = q!("PyConfigIOProxy");
const PROXY_SELF: Param = Param::new("self", PROXY);

// .def(py::init()) (PyConfigIOProxy.cpp:59 @ v2.5.2).
static PROXY_INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    operator: false,
    overloads: &[Overload {
        params: &[PROXY_SELF],
        ret: "None",
        call: |py, o, a, _| {
            construct(&a[0], o.params[0].ty, Shared::new(ProxyNode))?;
            none(py)
        },
        pos_only: 0,
    }],
};

// .def("getLutData", &ConfigIOProxy::getLutData) (PyConfigIOProxy.cpp:60 @ v2.5.2): the
// trampoline's `PYBIND11_OVERRIDE_PURE(std::vector<uint8_t>, ConfigIOProxy, getLutData,
// filepath)` (22-30).
static GET_LUT_DATA: Function = crate::method_def!(
    "getLutData",
    [PROXY_SELF, Param::new("arg0", "str")],
    VECTOR,
    |py, o, a, convert| {
        let _this = this_shared!(o, a, ProxyNode);
        let crate::dispatch::CharPtr(filepath) =
            crate::arg!(a, 1, convert, crate::dispatch::CharPtr);
        let Some(f) = get_override(&a[0], "getLutData")? else {
            return Err(pure_virtual("getLutData"));
        };
        let arg = crate::dispatch::char_ptr_to_py(py, filepath.as_deref())?;
        let result = f.call1((arg,))?;
        let data = cast_vector(&result)?;
        vector_out(py, data)
    }
);

// .def("getConfigData", &ConfigIOProxy::getConfigData) (PyConfigIOProxy.cpp:61 @ v2.5.2): the
// trampoline's `PYBIND11_OVERRIDE_PURE(std::string, ConfigIOProxy, getConfigData, )` (32-39).
static GET_CONFIG_DATA: Function =
    crate::method_def!("getConfigData", [PROXY_SELF], "str", |py, o, a, _| {
        let _this = this_shared!(o, a, ProxyNode);
        let Some(f) = get_override(&a[0], "getConfigData")? else {
            return Err(pure_virtual("getConfigData"));
        };
        let result = f.call0()?;
        let text = cast_string(&result)?;
        Ok(Some(
            crate::dispatch::std_string_to_py(py, &text)?
                .into_any()
                .unbind(),
        ))
    });

// .def("getFastLutFileHash", &ConfigIOProxy::getFastLutFileHash) (PyConfigIOProxy.cpp:62 @
// v2.5.2): the trampoline's `PYBIND11_OVERRIDE_PURE(std::string, ConfigIOProxy,
// getFastLutFileHash, filepath)` (41-49).
static GET_FAST_LUT_FILE_HASH: Function = crate::method_def!(
    "getFastLutFileHash",
    [PROXY_SELF, Param::new("arg0", "str")],
    "str",
    |py, o, a, convert| {
        let _this = this_shared!(o, a, ProxyNode);
        let crate::dispatch::CharPtr(filepath) =
            crate::arg!(a, 1, convert, crate::dispatch::CharPtr);
        let Some(f) = get_override(&a[0], "getFastLutFileHash")? else {
            return Err(pure_virtual("getFastLutFileHash"));
        };
        let arg = crate::dispatch::char_ptr_to_py(py, filepath.as_deref())?;
        let result = f.call1((arg,))?;
        let text = cast_string(&result)?;
        Ok(Some(
            crate::dispatch::std_string_to_py(py, &text)?
                .into_any()
                .unbind(),
        ))
    }
);

/// Port of `py::class_<ConfigIOProxy, std::shared_ptr<ConfigIOProxy>, PyConfigIOProxy>(m,
/// "PyConfigIOProxy")` and its definitions (PyConfigIOProxy.cpp:58-62 @ v2.5.2).
fn bind_proxy(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let cls = pybind11::new_class(
        py,
        "PyConfigIOProxy",
        "PyConfigIOProxy",
        &[],
        &[("__doc__", py.None().into_bound(py))],
    )?;
    cls.setattr("_pybind11_conduit_v1_", crate::enums::conduit(py)?)?;
    for f in [
        &PROXY_INIT,
        &GET_LUT_DATA,
        &GET_CONFIG_DATA,
        &GET_FAST_LUT_FILE_HASH,
    ] {
        def(&cls, f)?;
    }
    m.setattr("PyConfigIOProxy", &cls)?;
    Ok(())
}

/// Port of `bindPyConfigIOProxy` (src/bindings/python/PyConfigIOProxy.cpp:52-63 @ v2.5.2): the
/// vector class (54-56), then the proxy class (58-62).
pub(crate) fn bind_py_config_io_proxy(m: &Bound<'_, PyModule>) -> PyResult<()> {
    bind_vector(m)?;
    bind_proxy(m)
}

/// The capsule of a `module_local` class's type record, which pybind11 makes without a name.
#[allow(deprecated)]
fn module_local_record(py: Python<'_>) -> PyResult<Bound<'_, PyCapsule>> {
    // `PyCapsule::new_with_value` requires a name; pybind11's capsule has none.
    PyCapsule::new(py, 0usize, None)
}
