// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyFormatMetadata` (src/bindings/python/PyFormatMetadata.cpp @ v2.5.2): the class
//! `FormatMetadata` and its four iterator classes (chunk 6.3d).
//!
//! **References.** No Python code makes a `FormatMetadata`: each is a C++ reference into a
//! transform's metadata (`getFormatMetadata`, pybind11's `reference_internal`), or into the
//! vector of an element's child elements (the child element iterators' `__getitem__` and
//! `__next__`), and each iterator holds a C++ reference to its element. The library holds a
//! transform's metadata by value, so the binding's reference ([`MetadataPtr`]) names the element
//! as the wheel's pointer does: the transform's metadata itself, or a slot of the buffer of a
//! children vector.
//!
//! **Buffers.** A C++ pointer into a vector stays valid until the vector reallocates, and the
//! vectors here reallocate as the wheel's C++ runtime makes them grow ([`MetadataHeap`]):
//! pybind11 finds a Python object by its pointer, so after `addChildElement` reallocates the
//! vector, its first child is a new Python object, and only then. A reference into a buffer the
//! wheel freed (a reallocation, `clear`, an erased element, a transform that died), or to a slot
//! past the vector's end, reads memory the wheel no longer owns: the port's error
//! (docs/improvements.md, U-121).
//!
//! **A processor's metadata** (chunk 6.3h): `Processor.getFormatMetadata` and the items of its
//! `TransformFormatMetadataIterator` are references into the processor's metadata and its ops'
//! (`const FormatMetadata &`, whose `const` pybind11 drops). The port's processors are
//! immutable: such a reference reads as the wheel's, with buffers of its own per processor and
//! op (so its child elements are the same objects while their buffers live), and a call through
//! it that would change the metadata raises the port's error, the metadata unchanged
//! (docs/deviations.md, D-13).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, Weak};

use ocio::{FormatMetadata, Transform};
use ocio_ops::utils::string_utils::{c_str, compare};
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyCFunction, PyTuple};

use crate::dispatch::{Function, NonNullCharPtr, Param, StdString};
use crate::py_open_color_io::py_result;
use crate::py_transform::{
    FORMAT_METADATA, INT, Out, Registry, TransformRc, WeakTransformRc, class, def, lock,
    nested_class, none, ok, register_in, string_out,
};
use crate::py_utils::{check_index, new_iterator};
use crate::{method_def, q, this_iter};

const ATTRIBUTE_NAME_ITERATOR: &str = q!("FormatMetadata.AttributeNameIterator");
const ATTRIBUTE_ITERATOR: &str = q!("FormatMetadata.AttributeIterator");
const CONST_CHILD_ELEMENT_ITERATOR: &str = q!("FormatMetadata.ConstChildElementIterator");
const CHILD_ELEMENT_ITERATOR: &str = q!("FormatMetadata.ChildElementIterator");
const SELF: Param = Param::new("self", FORMAT_METADATA);

// ---------------------------------------------------------------------------------------------
// The wheel's buffers

/// The buffers of the vectors of child elements (`FormatMetadataImpl::m_elements`, a
/// `std::vector<FormatMetadataImpl>`) of one transform's metadata, as the wheel's C++ runtime
/// allocates them: each live buffer with the element whose vector owns it (its path of child
/// indices from the metadata) and its capacity.
///
/// A vector the binding hasn't seen yet holds a copy's buffer, exactly as large as the vector:
/// the vectors of a new transform are empty, and those of a copy (a transform's
/// `createEditableCopy`, the library's transforms) are copy-constructed. Every change the
/// binding makes to a vector goes through [`MetadataHeap::push`], [`MetadataHeap::clear`] or
/// [`MetadataHeap::erase`].
#[derive(Debug, Default, Clone)]
pub(crate) struct MetadataHeap {
    /// The live buffers by id.
    buffers: HashMap<u64, Buffer>,
    /// The buffer of each vector seen, by its element's path.
    by_owner: HashMap<Vec<usize>, u64>,
}

/// A buffer of a vector of child elements.
#[derive(Debug, Clone)]
struct Buffer {
    /// The path of the element whose vector holds it.
    owner: Vec<usize>,
    /// Its capacity, in elements.
    capacity: usize,
}

/// The ids of buffers, never reused: a buffer the wheel frees is never found again.
static NEXT_BUFFER: AtomicU64 = AtomicU64::new(1);

/// The capacity `push_back` gives a full vector of `capacity` elements: MSVC's STL grows it by
/// half (`vector::_Calculate_growth(size() + 1)`), libstdc++ doubles it (`_M_check_len(1)`,
/// `size() + max(size(), 1)`).
fn grown(capacity: usize) -> usize {
    if cfg!(windows) {
        (capacity + capacity / 2).max(capacity + 1)
    } else {
        capacity + capacity.max(1)
    }
}

/// The capacity a vector of `capacity` elements gets from a copy-assignment of `size` elements
/// that doesn't fit: MSVC's STL grows it as `push_back` would, at least to `size`
/// (`_Assign_counted_range`, `_Clear_and_reserve_geometric`); libstdc++ allocates `size`
/// (`operator=`, `_M_allocate_and_copy`).
fn assigned(capacity: usize, size: usize) -> usize {
    if cfg!(windows) {
        (capacity + capacity / 2).max(size)
    } else {
        size
    }
}

/// The shape of an element: its child elements' shapes, in order.
#[derive(Debug, Clone)]
pub(crate) struct Shape(Vec<Shape>);

impl Shape {
    pub(crate) fn of(md: &FormatMetadata) -> Shape {
        Shape(md.get_children_elements().iter().map(Shape::of).collect())
    }

    /// The number of child elements.
    pub(crate) fn num_children(&self) -> usize {
        self.0.len()
    }
}

/// `path` followed by `i`.
fn child_path(path: &[usize], i: usize) -> Vec<usize> {
    let mut p = path.to_vec();
    p.push(i);
    p
}

impl MetadataHeap {
    /// The buffer of the vector of `owner`, which holds `len` elements: a copy's (as large as
    /// the vector) when it was never seen.
    fn buffer(&mut self, owner: &[usize], len: usize) -> u64 {
        if let Some(&id) = self.by_owner.get(owner) {
            return id;
        }
        self.allocate(owner, len)
    }

    /// A new buffer of `capacity` for the vector of `owner`.
    fn allocate(&mut self, owner: &[usize], capacity: usize) -> u64 {
        let id = NEXT_BUFFER.fetch_add(1, Ordering::Relaxed);
        self.buffers.insert(
            id,
            Buffer {
                owner: owner.to_vec(),
                capacity,
            },
        );
        self.by_owner.insert(owner.to_vec(), id);
        id
    }

    /// Frees the buffers of the vectors of `path` and of every element below it.
    fn free_subtree(&mut self, path: &[usize]) {
        self.free_where(|owner| owner.starts_with(path));
    }

    /// Frees the buffers of the vectors of every element below `path`.
    fn free_below(&mut self, path: &[usize]) {
        self.free_where(|owner| owner.len() > path.len() && owner.starts_with(path));
    }

    fn free_where(&mut self, freed: impl Fn(&[usize]) -> bool) {
        self.by_owner.retain(|owner, _| !freed(owner));
        self.buffers.retain(|_, b| !freed(&b.owner));
    }

    /// `m_elements.emplace_back(...)` on the vector of `owner`, which held `len` elements: when
    /// full, a new buffer, into which the elements are copied (the class has no move
    /// constructor), so that the vectors of the elements below are copies too.
    pub(crate) fn push(&mut self, owner: &[usize], len: usize) {
        let id = self.buffer(owner, len);
        let capacity = self.buffers[&id].capacity;
        if len == capacity {
            self.free_subtree(owner);
            self.allocate(owner, grown(capacity));
        }
    }

    /// `m_elements.clear()` on the vector of `owner`: its elements destroyed, its buffer kept.
    pub(crate) fn clear(&mut self, owner: &[usize]) {
        self.free_below(owner);
    }

    /// `m_elements.erase(begin() + index)` on the vector of `owner`, whose element had the
    /// shape `before`: each element after `index` copy-assigned to the one before it (the class
    /// has no move assignment), the last destroyed.
    pub(crate) fn erase(&mut self, owner: &[usize], index: usize, before: &Shape) {
        let len = before.0.len();
        for k in index..len.saturating_sub(1) {
            self.assign(&child_path(owner, k), &before.0[k], &before.0[k + 1]);
        }
        if len > 0 {
            self.free_subtree(&child_path(owner, len - 1));
        }
    }

    /// `FormatMetadataImpl::operator=` (FormatMetadata.cpp:181-191 @ v2.5.2) of the element at
    /// `path`, of the shape `dst`, from an element of the shape `src`: its vector
    /// copy-assigned. A vector too small for the new elements gets a new buffer (into which they
    /// are copy-constructed); else each element it keeps is assigned in turn, those past the
    /// new end destroyed, and the new ones copy-constructed.
    fn assign(&mut self, path: &[usize], dst: &Shape, src: &Shape) {
        let (old, new) = (dst.0.len(), src.0.len());
        let id = self.buffer(path, old);
        let capacity = self.buffers[&id].capacity;
        if new > capacity {
            self.free_subtree(path);
            self.allocate(path, assigned(capacity, new));
            return;
        }
        for m in 0..old.min(new) {
            self.assign(&child_path(path, m), &dst.0[m], &src.0[m]);
        }
        for m in new.min(old)..old.max(new) {
            self.free_subtree(&child_path(path, m));
        }
    }
}

// ---------------------------------------------------------------------------------------------
// References

/// Whose metadata a reference is into.
#[derive(Debug, Clone)]
enum Root {
    /// A transform's (a reference that doesn't keep the transform alive).
    Transform(WeakTransformRc),
    /// A processor's (`None`) or its op's (`Some(i)`): read only (D-13). pybind11's
    /// `reference_internal` keeps the processor alive while the reference lives.
    Processor(Arc<ocio::Processor>, Option<i32>),
}

/// A C++ reference to an element of a transform's or a processor's metadata, as the wheel's
/// pointer names it: the metadata itself (`at` none), or slot `at.1` of the buffer `at.0`
/// ([`MetadataHeap`]).
#[derive(Debug, Clone)]
pub(crate) struct MetadataPtr {
    root: Root,
    /// The node's or the processor's address, which keys the registry.
    key: usize,
    at: Option<(u64, usize)>,
}

/// The buffers of the metadata of each processor (`None`) and op (`Some(i)`) Python read,
/// while the processor lives.
type ProcessorHeaps = Vec<(Weak<ocio::Processor>, Option<i32>, MetadataHeap)>;
static PROCESSOR_HEAPS: Mutex<ProcessorHeaps> = Mutex::new(Vec::new());

/// The port's error for a change of a processor's metadata (docs/deviations.md, D-13).
fn processor_unchanged() -> PyErr {
    pyo3::exceptions::PyRuntimeError::new_err(
        "FormatMetadata: the metadata of a processor, which the port's processors hold \
         unchanged; OCIO 2.5.2 changes it, for that processor and those sharing its ops \
         (docs/deviations.md, D-13)",
    )
}

/// The port's error for a reference into memory the wheel freed or never constructed
/// (docs/improvements.md, U-121).
fn dangling() -> PyErr {
    pyo3::exceptions::PyRuntimeError::new_err(
        "the FormatMetadata element was freed (its transform freed, or the vector of child \
         elements holding it reallocated, cleared or shortened), and the wheel reads its memory \
         anyway (docs/improvements.md, U-121)",
    )
}

/// The metadata of a transform class that has some.
pub(crate) fn metadata_mut(t: &mut Transform) -> Option<&mut FormatMetadata> {
    Some(match t {
        Transform::Cdl(t) => t.format_metadata_mut(),
        Transform::Exponent(t) => t.format_metadata_mut(),
        Transform::ExponentWithLinear(t) => t.format_metadata_mut(),
        Transform::ExposureContrast(t) => t.format_metadata_mut(),
        Transform::FixedFunction(t) => t.format_metadata_mut(),
        Transform::GradingHueCurve(t) => t.format_metadata_mut(),
        Transform::GradingPrimary(t) => t.format_metadata_mut(),
        Transform::GradingRgbCurve(t) => t.format_metadata_mut(),
        Transform::GradingTone(t) => t.format_metadata_mut(),
        Transform::Group(t) => t.format_metadata_mut(),
        Transform::LogAffine(t) => t.format_metadata_mut(),
        Transform::LogCamera(t) => t.format_metadata_mut(),
        Transform::Log(t) => t.format_metadata_mut(),
        Transform::Lut1D(t) => t.format_metadata_mut(),
        Transform::Lut3D(t) => t.format_metadata_mut(),
        Transform::Matrix(t) => t.format_metadata_mut(),
        Transform::Range(t) => t.format_metadata_mut(),
        _ => return None,
    })
}

impl MetadataPtr {
    /// A reference to the metadata of `rc`.
    pub(crate) fn root(rc: &TransformRc) -> MetadataPtr {
        MetadataPtr {
            root: Root::Transform(rc.downgrade()),
            key: rc.key(),
            at: None,
        }
    }

    /// A reference to the metadata of the processor `p` (`op` none) or of its op `op`.
    fn processor(p: &Arc<ocio::Processor>, op: Option<i32>) -> MetadataPtr {
        MetadataPtr {
            key: Arc::as_ptr(p) as *const u8 as usize,
            root: Root::Processor(p.clone(), op),
            at: None,
        }
    }

    /// The registry's key of the element.
    fn registry_key(&self) -> MetadataKey {
        let op = match self.root {
            Root::Transform(_) => None,
            Root::Processor(_, op) => Some(op),
        };
        (self.key, op, self.at)
    }

    /// `f` of the element, the transform's buffers, and the element's path: the port's error
    /// when the wheel's reference dangles.
    pub(crate) fn with<R>(
        &self,
        f: impl FnOnce(&mut FormatMetadata, &mut MetadataHeap, &[usize]) -> PyResult<R>,
    ) -> PyResult<R> {
        let rc = match &self.root {
            Root::Transform(node) => node.upgrade().ok_or_else(dangling)?,
            Root::Processor(p, op) => return self.with_processor(p, *op, f),
        };
        let mut node = rc.lock();
        let node = &mut *node;
        let path = match self.at {
            None => Vec::new(),
            Some((id, i)) => {
                let buffer = node.metadata_heap.buffers.get(&id).ok_or_else(dangling)?;
                child_path(&buffer.owner, i)
            }
        };
        let mut md = metadata_mut(&mut node.transform).ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("the transform holds no format metadata")
        })?;
        for &i in &path {
            md = md
                .get_children_elements_mut()
                .get_mut(i)
                .ok_or_else(dangling)?;
        }
        f(md, &mut node.metadata_heap, &path)
    }

    /// [`with`](Self::with) of a processor's metadata (`op` none) or its op's: `f` of a copy,
    /// with a copy of its buffers, which stay when the metadata is the same after `f`; a change
    /// is the port's error (D-13), and leaves nothing changed.
    fn with_processor<R>(
        &self,
        p: &Arc<ocio::Processor>,
        op: Option<i32>,
        f: impl FnOnce(&mut FormatMetadata, &mut MetadataHeap, &[usize]) -> PyResult<R>,
    ) -> PyResult<R> {
        let original = match op {
            None => p.format_metadata(),
            Some(i) => p
                .transform_format_metadata(i)
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{e}")))?,
        };
        let mut heaps = PROCESSOR_HEAPS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        heaps.retain(|(w, _, _)| w.strong_count() > 0);
        let weak = Arc::downgrade(p);
        let at = match heaps
            .iter()
            .position(|(w, o, _)| w.ptr_eq(&weak) && *o == op)
        {
            Some(at) => at,
            None => {
                heaps.push((weak, op, MetadataHeap::default()));
                heaps.len() - 1
            }
        };
        let mut heap = heaps[at].2.clone();
        let path = match self.at {
            None => Vec::new(),
            Some((id, i)) => {
                let buffer = heap.buffers.get(&id).ok_or_else(dangling)?;
                child_path(&buffer.owner, i)
            }
        };
        let mut copy = original.clone();
        let mut md = &mut copy;
        for &i in &path {
            md = md
                .get_children_elements_mut()
                .get_mut(i)
                .ok_or_else(dangling)?;
        }
        let r = f(md, &mut heap, &path)?;
        if copy != *original {
            return Err(processor_unchanged());
        }
        heaps[at].2 = heap;
        Ok(r)
    }

    /// `getChildElement(i)` of the element: a reference to its child `i`, or upstream's error
    /// outside its children (FormatMetadata.cpp:349-356 @ v2.5.2).
    fn child(&self, py: Python<'_>, i: i32) -> PyResult<MetadataPtr> {
        let at = self.with(|md, heap, path| {
            py_result(py, md.get_child_element(i).map(|_| ()))?;
            let id = heap.buffer(path, md.get_children_elements().len());
            Ok((id, i as usize))
        })?;
        Ok(MetadataPtr {
            root: self.root.clone(),
            key: self.key,
            at: Some(at),
        })
    }

    /// The number of child elements.
    fn num_children(&self) -> PyResult<i32> {
        self.with(|md, _, _| Ok(md.get_num_children_elements()))
    }

    /// The number of attributes.
    fn num_attributes(&self) -> PyResult<i32> {
        self.with(|md, _, _| Ok(md.get_num_attributes()))
    }
}

/// The value of a `FormatMetadata` object: its reference, and the objects pybind11's
/// `keep_alive<0, 1>` keeps alive with it (`reference_internal`: the transform's object for
/// `getFormatMetadata`, the iterator's for a child element; one more each time a call returns
/// the object again, as pybind11 adds a patient each time).
#[pyclass(frozen, module = "pybind11_builtins", name = "_format_metadata_ref")]
#[derive(Debug)]
pub(crate) struct FormatMetadataRef {
    pub(crate) ptr: MetadataPtr,
    patients: Mutex<Vec<Py<PyAny>>>,
}

/// The registered `FormatMetadata` objects, by node and buffer slot ([`Registry`]).
type MetadataKey = (usize, Option<Option<i32>>, Option<(u64, usize)>);
static INSTANCES: LazyLock<Mutex<Registry<MetadataKey>>> =
    LazyLock::new(|| Mutex::new(Registry::new()));
static INSTANCES_CALLBACK: PyOnceLock<Py<PyCFunction>> = PyOnceLock::new();

/// The number of `FormatMetadata` objects registered.
pub(crate) fn registered_count() -> usize {
    lock(&INSTANCES).keys.len()
}

/// The number of keys the registry holds.
pub(crate) fn registered_keys() -> usize {
    lock(&INSTANCES).key_count()
}

/// A returned `FormatMetadata &` (`reference_internal`), whose call's first argument is
/// `patient`: the object registered for the element while it lives, else a new one; either way
/// keeping `patient` alive.
fn element_out(py: Python<'_>, ptr: MetadataPtr, patient: &Bound<'_, PyAny>) -> Out {
    let key = ptr.registry_key();
    let found = lock(&INSTANCES).find(py, &key);
    let obj = match found {
        Some(obj) => obj,
        None => {
            let cls = class(py, FORMAT_METADATA)?;
            let obj = cls.call_method1("__new__", (&cls,))?;
            let value = FormatMetadataRef {
                ptr,
                patients: Mutex::new(Vec::new()),
            };
            crate::pybind11::set_value(&obj, FORMAT_METADATA, Bound::new(py, value)?.into_any())?;
            register_in(&INSTANCES, &INSTANCES_CALLBACK, &obj, key)?;
            obj
        }
    };
    let value = crate::pybind11::read_value(&obj, FORMAT_METADATA)?;
    value
        .cast::<FormatMetadataRef>()?
        .get()
        .patients
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(patient.clone().unbind());
    Ok(Some(obj.unbind()))
}

/// `getFormatMetadata()` of the transform object `this` holding `rc`.
pub(crate) fn transform_metadata_out(
    py: Python<'_>,
    this: &Bound<'_, PyAny>,
    rc: &TransformRc,
) -> Out {
    element_out(py, MetadataPtr::root(rc), this)
}

/// `getFormatMetadata()` (`op` none) or the iterator's `getTransformFormatMetadata(op)` of the
/// processor `p`, whose call's first argument is `patient` (pybind11's `reference_internal`):
/// the library's error for an op outside the processor's.
pub(crate) fn processor_metadata_out(
    py: Python<'_>,
    patient: &Bound<'_, PyAny>,
    p: &Arc<ocio::Processor>,
    op: Option<i32>,
) -> Out {
    if let Some(i) = op {
        py_result(py, p.transform_format_metadata(i))?;
    }
    element_out(py, MetadataPtr::processor(p, op), patient)
}

/// The reference `obj` holds as a `FormatMetadata` (`Ok(None)`: not one, the next overload).
fn reference(obj: &Bound<'_, PyAny>) -> PyResult<Option<MetadataPtr>> {
    let Some(this) = crate::pybind11::instance_of(obj, FORMAT_METADATA) else {
        return Ok(None);
    };
    let value = crate::pybind11::read_value(&this, FORMAT_METADATA)?;
    Ok(Some(value.cast::<FormatMetadataRef>()?.get().ptr.clone()))
}

/// The reference of `self`, or the next overload.
macro_rules! this_md {
    ($a:expr) => {
        match reference(&$a[0])? {
            Some(ptr) => ptr,
            None => return Ok(None),
        }
    };
}

/// A `KeyError` of the message `bytes` as pybind11's `key_error` raises it: the C string
/// (up to a NUL), decoded as strict UTF-8.
pub(crate) fn key_error(py: Python<'_>, bytes: &[u8]) -> PyErr {
    match crate::dispatch::std_string_to_py(py, c_str(bytes)) {
        Ok(message) => pyo3::exceptions::PyKeyError::new_err((message.unbind(),)),
        Err(e) => e,
    }
}

// ---------------------------------------------------------------------------------------------
// The class

// .def("__iter__", [](const FormatMetadata & self) { return AttributeNameIterator(self); })
// (PyFormatMetadata.cpp:56-59 @ v2.5.2).
static ITER: Function = method_def!(
    "__iter__",
    [SELF],
    ATTRIBUTE_NAME_ITERATOR,
    |py, _, a, _| {
        let ptr = this_md!(a);
        Ok(Some(
            new_iterator(py, ATTRIBUTE_NAME_ITERATOR, Some(Box::new(ptr)))?.unbind(),
        ))
    }
);

// .def("__len__", &FormatMetadata::getNumAttributes) (PyFormatMetadata.cpp:60-61 @ v2.5.2).
static LEN: Function = method_def!("__len__", [SELF], "int", |py, _, a, _| {
    let ptr = this_md!(a);
    ok(py, ptr.num_attributes()?)
});

// .def("__getitem__", [](self, const std::string & name) {...}, "name"_a.none(false))
// (PyFormatMetadata.cpp:62-78 @ v2.5.2).
static GETITEM: Function = method_def!(
    "__getitem__",
    [SELF, Param::new("name", "str")],
    "str",
    |py, _, a, convert| {
        let ptr = this_md!(a);
        let StdString(name) = crate::arg!(a, 1, convert, StdString);
        let value = ptr.with(|md, _, _| {
            Ok((0..md.get_num_attributes())
                .find(|&i| compare(md.get_attribute_name(i), &name))
                .map(|i| md.get_attribute_value(i).to_vec()))
        })?;
        match value {
            Some(value) => string_out(py, &value),
            None => {
                let mut message = b" '".to_vec();
                message.extend_from_slice(&name);
                message.push(b'\'');
                Err(key_error(py, &message))
            }
        }
    }
);

// .def("__setitem__", &FormatMetadata::addAttribute, "name"_a.none(false),
// "value"_a.none(false)) (PyFormatMetadata.cpp:79-80 @ v2.5.2).
static SETITEM: Function = method_def!(
    "__setitem__",
    [SELF, Param::new("name", "str"), Param::new("value", "str")],
    "None",
    |py, _, a, convert| {
        let ptr = this_md!(a);
        let NonNullCharPtr(name) = crate::arg!(a, 1, convert, NonNullCharPtr);
        let NonNullCharPtr(value) = crate::arg!(a, 2, convert, NonNullCharPtr);
        ptr.with(|md, _, _| py_result(py, md.add_attribute(Some(&name), Some(&value))))?;
        none(py)
    }
);

// .def("__contains__", [](self, const std::string & name) -> bool {...}, "name"_a.none(false))
// (PyFormatMetadata.cpp:81-93 @ v2.5.2).
static CONTAINS: Function = method_def!(
    "__contains__",
    [SELF, Param::new("name", "str")],
    "bool",
    |py, _, a, convert| {
        let ptr = this_md!(a);
        let StdString(name) = crate::arg!(a, 1, convert, StdString);
        let found = ptr.with(|md, _, _| {
            Ok((0..md.get_num_attributes()).any(|i| compare(md.get_attribute_name(i), &name)))
        })?;
        ok(py, found)
    }
);

// .def("__repr__", [](const FormatMetadata & self) { oss << self; ... })
// (PyFormatMetadata.cpp:94-99 @ v2.5.2).
static REPR: Function = method_def!("__repr__", [SELF], "str", |py, _, a, _| {
    let ptr = this_md!(a);
    let text = ptr.with(|md, _, _| {
        let mut os = Vec::new();
        md.write_to(&mut os);
        Ok(os)
    })?;
    string_out(py, &text)
});

// .def("getElementName", &FormatMetadata::getElementName) (PyFormatMetadata.cpp:101-102 @
// v2.5.2).
static GET_ELEMENT_NAME: Function = method_def!("getElementName", [SELF], "str", |py, _, a, _| {
    let ptr = this_md!(a);
    let name = ptr.with(|md, _, _| Ok(md.get_element_name().to_vec()))?;
    string_out(py, &name)
});

// .def("setElementName", &FormatMetadata::setElementName, "name"_a.none(false))
// (PyFormatMetadata.cpp:103-104 @ v2.5.2).
static SET_ELEMENT_NAME: Function = method_def!(
    "setElementName",
    [SELF, Param::new("name", "str")],
    "None",
    |py, _, a, convert| {
        let ptr = this_md!(a);
        let NonNullCharPtr(name) = crate::arg!(a, 1, convert, NonNullCharPtr);
        ptr.with(|md, _, _| py_result(py, md.set_element_name(Some(&name))))?;
        none(py)
    }
);

// .def("getElementValue", &FormatMetadata::getElementValue) (PyFormatMetadata.cpp:105-106 @
// v2.5.2).
static GET_ELEMENT_VALUE: Function =
    method_def!("getElementValue", [SELF], "str", |py, _, a, _| {
        let ptr = this_md!(a);
        let value = ptr.with(|md, _, _| Ok(md.get_element_value().to_vec()))?;
        string_out(py, &value)
    });

// .def("setElementValue", &FormatMetadata::setElementValue, "value"_a.none(false))
// (PyFormatMetadata.cpp:107-108 @ v2.5.2).
static SET_ELEMENT_VALUE: Function = method_def!(
    "setElementValue",
    [SELF, Param::new("value", "str")],
    "None",
    |py, _, a, convert| {
        let ptr = this_md!(a);
        let NonNullCharPtr(value) = crate::arg!(a, 1, convert, NonNullCharPtr);
        ptr.with(|md, _, _| py_result(py, md.set_element_value(Some(&value))))?;
        none(py)
    }
);

// .def("getAttributes", [](const FormatMetadata & self) { return AttributeIterator(self); })
// (PyFormatMetadata.cpp:109-112 @ v2.5.2).
static GET_ATTRIBUTES: Function = method_def!(
    "getAttributes",
    [SELF],
    ATTRIBUTE_ITERATOR,
    |py, _, a, _| {
        let ptr = this_md!(a);
        Ok(Some(
            new_iterator(py, ATTRIBUTE_ITERATOR, Some(Box::new(ptr)))?.unbind(),
        ))
    }
);

// .def("getChildElements", [](FormatMetadata & self) { return ChildElementIterator(self); })
// (PyFormatMetadata.cpp:113-116 @ v2.5.2).
static GET_CHILD_ELEMENTS: Function = method_def!(
    "getChildElements",
    [SELF],
    CHILD_ELEMENT_ITERATOR,
    |py, _, a, _| {
        let ptr = this_md!(a);
        Ok(Some(
            new_iterator(py, CHILD_ELEMENT_ITERATOR, Some(Box::new(ptr)))?.unbind(),
        ))
    }
);

// .def("addChildElement", &FormatMetadata::addChildElement, "name"_a.none(false),
// "value"_a.none(false)) (PyFormatMetadata.cpp:117-119 @ v2.5.2).
static ADD_CHILD_ELEMENT: Function = method_def!(
    "addChildElement",
    [SELF, Param::new("name", "str"), Param::new("value", "str")],
    "None",
    |py, _, a, convert| {
        let ptr = this_md!(a);
        let NonNullCharPtr(name) = crate::arg!(a, 1, convert, NonNullCharPtr);
        let NonNullCharPtr(value) = crate::arg!(a, 2, convert, NonNullCharPtr);
        ptr.with(|md, heap, path| {
            let len = md.get_children_elements().len();
            py_result(py, md.add_child_element(Some(&name), Some(&value)))?;
            heap.push(path, len);
            Ok(())
        })?;
        none(py)
    }
);

// .def("getName", &FormatMetadata::getName) (PyFormatMetadata.cpp:120-121 @ v2.5.2).
static GET_NAME: Function = method_def!("getName", [SELF], "str", |py, _, a, _| {
    let ptr = this_md!(a);
    let name = ptr.with(|md, _, _| Ok(md.get_name().to_vec()))?;
    string_out(py, &name)
});

// .def("setName", &FormatMetadata::setName, "name"_a.none(false)) (PyFormatMetadata.cpp:
// 122-123 @ v2.5.2).
static SET_NAME: Function = method_def!(
    "setName",
    [SELF, Param::new("name", "str")],
    "None",
    |py, _, a, convert| {
        let ptr = this_md!(a);
        let NonNullCharPtr(name) = crate::arg!(a, 1, convert, NonNullCharPtr);
        ptr.with(|md, _, _| {
            md.set_name(Some(&name));
            Ok(())
        })?;
        none(py)
    }
);

// .def("getID", &FormatMetadata::getID) (PyFormatMetadata.cpp:124-125 @ v2.5.2).
static GET_ID: Function = method_def!("getID", [SELF], "str", |py, _, a, _| {
    let ptr = this_md!(a);
    let id = ptr.with(|md, _, _| Ok(md.get_id().to_vec()))?;
    string_out(py, &id)
});

// .def("setID", &FormatMetadata::setID, "id"_a.none(false)) (PyFormatMetadata.cpp:126-127 @
// v2.5.2).
static SET_ID: Function = method_def!(
    "setID",
    [SELF, Param::new("id", "str")],
    "None",
    |py, _, a, convert| {
        let ptr = this_md!(a);
        let NonNullCharPtr(id) = crate::arg!(a, 1, convert, NonNullCharPtr);
        ptr.with(|md, _, _| {
            md.set_id(Some(&id));
            Ok(())
        })?;
        none(py)
    }
);

// .def("clear", &FormatMetadata::clear) (PyFormatMetadata.cpp:128-129 @ v2.5.2).
static CLEAR: Function = method_def!("clear", [SELF], "None", |py, _, a, _| {
    let ptr = this_md!(a);
    ptr.with(|md, heap, path| {
        md.clear();
        heap.clear(path);
        Ok(())
    })?;
    none(py)
});

// ---------------------------------------------------------------------------------------------
// The iterators

/// The reference an iterator of `self` holds, or the next overload.
macro_rules! this_ptr {
    ($o:expr, $a:expr) => {{
        let it = this_iter!($o, $a);
        it.get().obj::<MetadataPtr>()?.clone()
    }};
}

const AN_SELF: Param = Param::new("self", ATTRIBUTE_NAME_ITERATOR);
const AT_SELF: Param = Param::new("self", ATTRIBUTE_ITERATOR);
const CC_SELF: Param = Param::new("self", CONST_CHILD_ELEMENT_ITERATOR);
const CH_SELF: Param = Param::new("self", CHILD_ELEMENT_ITERATOR);

/// `getAttributeName(i)` of the element.
fn attribute_name(py: Python<'_>, ptr: &MetadataPtr, i: i32) -> Out {
    let name = ptr.with(|md, _, _| Ok(md.get_attribute_name(i).to_vec()))?;
    string_out(py, &name)
}

/// `py::make_tuple(getAttributeName(i), getAttributeValue(i))` of the element.
fn attribute(py: Python<'_>, ptr: &MetadataPtr, i: i32) -> Out {
    let (name, value) = ptr.with(|md, _, _| {
        Ok((
            md.get_attribute_name(i).to_vec(),
            md.get_attribute_value(i).to_vec(),
        ))
    })?;
    let tuple = PyTuple::new(
        py,
        [
            crate::dispatch::std_string_to_py(py, &name)?,
            crate::dispatch::std_string_to_py(py, &value)?,
        ],
    )?;
    Ok(Some(tuple.into_any().unbind()))
}

/// `__iter__` of an iterator: itself.
fn iter_self(a: &[Bound<'_, PyAny>]) -> Out {
    Ok(Some(a[0].clone().unbind()))
}

// clsAttributeNameIterator (PyFormatMetadata.cpp:131-143 @ v2.5.2).
static AN_LEN: Function = method_def!("__len__", [AN_SELF], "int", |py, o, a, _| {
    let ptr = this_ptr!(o, a);
    ok(py, ptr.num_attributes()?)
});

static AN_GETITEM: Function = method_def!(
    "__getitem__",
    [AN_SELF, Param::new("arg0", INT)],
    "str",
    |py, o, a, convert| {
        let ptr = this_ptr!(o, a);
        let i = crate::arg!(a, 1, convert, i32);
        check_index(i, ptr.num_attributes()?)?;
        attribute_name(py, &ptr, i)
    }
);

static AN_ITER: Function = method_def!(
    "__iter__",
    [AN_SELF],
    ATTRIBUTE_NAME_ITERATOR,
    |_, o, a, _| {
        let _ = this_ptr!(o, a);
        iter_self(a)
    }
);

static AN_NEXT: Function = method_def!("__next__", [AN_SELF], "str", |py, o, a, _| {
    let it = this_iter!(o, a);
    let ptr = it.get().obj::<MetadataPtr>()?.clone();
    let i = it.get().next_index(ptr.num_attributes()?)?;
    attribute_name(py, &ptr, i)
});

// clsAttributeIterator (PyFormatMetadata.cpp:145-159 @ v2.5.2).
static AT_LEN: Function = method_def!("__len__", [AT_SELF], "int", |py, o, a, _| {
    let ptr = this_ptr!(o, a);
    ok(py, ptr.num_attributes()?)
});

static AT_GETITEM: Function = method_def!(
    "__getitem__",
    [AT_SELF, Param::new("arg0", INT)],
    "tuple",
    |py, o, a, convert| {
        let ptr = this_ptr!(o, a);
        let i = crate::arg!(a, 1, convert, i32);
        check_index(i, ptr.num_attributes()?)?;
        attribute(py, &ptr, i)
    }
);

static AT_ITER: Function = method_def!("__iter__", [AT_SELF], ATTRIBUTE_ITERATOR, |_, o, a, _| {
    let _ = this_ptr!(o, a);
    iter_self(a)
});

static AT_NEXT: Function = method_def!("__next__", [AT_SELF], "tuple", |py, o, a, _| {
    let it = this_iter!(o, a);
    let ptr = it.get().obj::<MetadataPtr>()?.clone();
    let i = it.get().next_index(ptr.num_attributes()?)?;
    attribute(py, &ptr, i)
});

/// The four methods of a child element iterator class (the const one's and the other's are
/// the same, PyFormatMetadata.cpp:161-201 @ v2.5.2), as statics `$len`, `$getitem`, `$iter`,
/// `$next`.
macro_rules! child_iterator {
    ($self:expr, $qualified:expr, $len:ident, $getitem:ident, $iter:ident, $next:ident) => {
        static $len: Function = method_def!("__len__", [$self], "int", |py, o, a, _| {
            let ptr = this_ptr!(o, a);
            ok(py, ptr.num_children()?)
        });

        // FormatMetadata provides index check with exception.
        static $getitem: Function = method_def!(
            "__getitem__",
            [$self, Param::new("arg0", INT)],
            FORMAT_METADATA,
            |py, o, a, convert| {
                let ptr = this_ptr!(o, a);
                let i = crate::arg!(a, 1, convert, i32);
                element_out(py, ptr.child(py, i)?, &a[0])
            }
        );

        static $iter: Function = method_def!("__iter__", [$self], $qualified, |_, o, a, _| {
            let _ = this_ptr!(o, a);
            iter_self(a)
        });

        static $next: Function =
            method_def!("__next__", [$self], FORMAT_METADATA, |py, o, a, _| {
                let it = this_iter!(o, a);
                let ptr = it.get().obj::<MetadataPtr>()?.clone();
                let i = it.get().next_index(ptr.num_children()?)?;
                element_out(py, ptr.child(py, i)?, &a[0])
            });
    };
}

child_iterator!(
    CC_SELF,
    CONST_CHILD_ELEMENT_ITERATOR,
    CC_LEN,
    CC_GETITEM,
    CC_ITER,
    CC_NEXT
);
child_iterator!(
    CH_SELF,
    CHILD_ELEMENT_ITERATOR,
    CH_LEN,
    CH_GETITEM,
    CH_ITER,
    CH_NEXT
);

/// Port of `bindPyFormatMetadata` (src/bindings/python/PyFormatMetadata.cpp:32-201 @ v2.5.2),
/// in its order: the iterator classes nested in the class (34-52), the class's methods
/// (54-130), the iterators' (131-201).
pub(crate) fn bind_py_format_metadata(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let cls = class(py, FORMAT_METADATA)?;
    let attribute_name_iterator = nested_class(&cls, "FormatMetadata", "AttributeNameIterator")?;
    let attribute_iterator = nested_class(&cls, "FormatMetadata", "AttributeIterator")?;
    let const_child_iterator = nested_class(&cls, "FormatMetadata", "ConstChildElementIterator")?;
    let child_iterator = nested_class(&cls, "FormatMetadata", "ChildElementIterator")?;
    for f in [
        &ITER,
        &LEN,
        &GETITEM,
        &SETITEM,
        &CONTAINS,
        &REPR,
        &GET_ELEMENT_NAME,
        &SET_ELEMENT_NAME,
        &GET_ELEMENT_VALUE,
        &SET_ELEMENT_VALUE,
        &GET_ATTRIBUTES,
        &GET_CHILD_ELEMENTS,
        &ADD_CHILD_ELEMENT,
        &GET_NAME,
        &SET_NAME,
        &GET_ID,
        &SET_ID,
        &CLEAR,
    ] {
        def(&cls, f)?;
    }
    for f in [&AN_LEN, &AN_GETITEM, &AN_ITER, &AN_NEXT] {
        def(&attribute_name_iterator, f)?;
    }
    for f in [&AT_LEN, &AT_GETITEM, &AT_ITER, &AT_NEXT] {
        def(&attribute_iterator, f)?;
    }
    for f in [&CC_LEN, &CC_GETITEM, &CC_ITER, &CC_NEXT] {
        def(&const_child_iterator, f)?;
    }
    for f in [&CH_LEN, &CH_GETITEM, &CH_ITER, &CH_NEXT] {
        def(&child_iterator, f)?;
    }
    Ok(())
}
