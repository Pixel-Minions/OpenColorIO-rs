// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyTransform` (src/bindings/python/PyTransform.cpp @ v2.5.2): the `Transform` base class
//! and what every transform class of the module shares, as the wheel shows it (chunk 6.3a).
//!
//! **Values.** pybind11 holds each transform by OCIO's `TransformRcPtr`, a `shared_ptr`: a
//! Python object and a group's child (and a copy of a group, whose children are its source's,
//! `GroupTransformImpl::createEditableCopy`) can be the very same C++ object, so a change
//! through one shows in the other. The port's library owns a group's children by value (the
//! owner's API decision, 2026-10-01, docs/improvements.md, I-11), so the binding holds each
//! transform as a [`TransformRc`] (a shared node: the transform, and for a group its children's
//! nodes), and hands the library a copy built from the nodes ([`TransformRc::materialize`])
//! whenever it calls it.
//!
//! **Identity.** pybind11 returns the Python object it made for a C++ pointer while that object
//! lives (its registered instances), and otherwise makes one of the pointer's most derived class
//! (the `polymorphic_type_hook` of `PyOpenColorIO.h`): [`to_py`] does the same with a registry of
//! weak references by node.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};

use ocio::{Transform, TransformType};
use ocio_ops::open_color_types::TransformDirection;
use pyo3::BoundObject;
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyCFunction, PyDict, PyType, PyWeakrefReference};

use crate::dispatch::{Function, Param};
use crate::enums::enum_class;
use crate::py_open_color_io::py_result;
use crate::pybind11;

/// The qualified name of the module's class `$name`, as pybind11 writes a parameter's type.
#[macro_export]
macro_rules! q {
    ($name:literal) => {
        concat!("PyOpenColorIO.PyOpenColorIO.", $name)
    };
}

pub(crate) const TRANSFORM: &str = q!("Transform");
pub(crate) const TRANSFORM_DIRECTION: &str = q!("TransformDirection");
pub(crate) const TRANSFORM_TYPE: &str = q!("TransformType");
pub(crate) const BIT_DEPTH: &str = q!("BitDepth");

// ---------------------------------------------------------------------------------------------
// The shared nodes

/// A transform as pybind11's holder shares it (`TransformRcPtr`): cloning the handle shares the
/// node.
#[derive(Debug, Clone)]
pub(crate) struct TransformRc(Arc<Mutex<Node>>);

/// What a [`TransformRc`] holds.
#[derive(Debug)]
pub(crate) struct Node {
    /// The transform; for a group, without its children, which are [`Node::children`].
    pub(crate) transform: Transform,
    /// A group's children, in order; `None` for a null pointer, which pybind11 appends for a
    /// Python `None` (the holder caster's conversion).
    pub(crate) children: Vec<Option<TransformRc>>,
    /// The direction as the C++ enum holds it, which Python can set outside the enumerators
    /// (`OCIO.TransformDirection(5)`, I-223); the library's direction is forward for 0 and
    /// inverse otherwise, as upstream's `TransformDirectionToString` and `operator<<` read it.
    pub(crate) direction: i64,
}

impl TransformRc {
    /// A new node of `transform` (a group's children become nodes of their own).
    pub(crate) fn new(transform: Transform) -> TransformRc {
        let direction = match transform.direction() {
            TransformDirection::Forward => 0,
            TransformDirection::Inverse => 1,
        };
        let (transform, children) = match transform {
            Transform::Group(group) => {
                let mut shell = ocio::GroupTransform::new();
                shell.set_direction(group.direction());
                *shell.format_metadata_mut() = group.format_metadata().clone();
                let children = (0..group.num_transforms())
                    .filter_map(|i| group.transform(i).ok())
                    .map(|t| Some(TransformRc::new(t.clone())))
                    .collect();
                (Transform::Group(shell), children)
            }
            other => (other, Vec::new()),
        };
        TransformRc(Arc::new(Mutex::new(Node {
            transform,
            children,
            direction,
        })))
    }

    /// The node, locked. The binding runs under the GIL, and never locks a node while it holds
    /// another's lock, so the lock is never contended.
    pub(crate) fn lock(&self) -> MutexGuard<'_, Node> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The node's address, which pybind11's registry keys an instance by.
    fn key(&self) -> usize {
        Arc::as_ptr(&self.0) as usize
    }

    /// Whether `self` and `other` are the same node.
    pub(crate) fn same(&self, other: &TransformRc) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    /// The transform's class.
    pub(crate) fn transform_type(&self) -> TransformType {
        self.lock().transform.transform_type()
    }

    /// A copy of the transform as the library holds one: a group with copies of its
    /// children, at every level. A null child, which upstream dereferences (U-110), and a group
    /// that holds itself, through which upstream recurses until its stack overflows (U-111), are
    /// the port's errors. It walks the nested groups with a stack of its own.
    pub(crate) fn materialize(&self) -> PyResult<Transform> {
        self.materialize_with(Holes::Error, Holes::Error)
    }

    /// [`materialize`](Self::materialize), with what stands for a null child (`nulls`) and for a
    /// child that is a group the walk is in (`cycles`): the port's error, or an empty group,
    /// which builds no op and is no CDL, as a null pointer does for the writers
    /// (`BuildOps` skips a null transform, Transform.cpp:48-50 @ v2.5.2, and the CDL writers'
    /// cast refuses it).
    pub(crate) fn materialize_with(&self, nulls: Holes, cycles: Holes) -> PyResult<Transform> {
        // Each level: the node, its copy so far (a group), its children, the next child.
        struct Level {
            node: TransformRc,
            copy: ocio::GroupTransform,
            children: Vec<Option<TransformRc>>,
            next: usize,
        }
        let open = |rc: &TransformRc| -> Result<Level, Box<Transform>> {
            let node = rc.lock();
            match &node.transform {
                Transform::Group(g) => Ok(Level {
                    node: rc.clone(),
                    copy: g.clone(),
                    children: node.children.clone(),
                    next: 0,
                }),
                other => Err(Box::new(other.clone())),
            }
        };
        let hole = || Transform::Group(ocio::GroupTransform::new());
        let mut stack = match open(self) {
            Ok(level) => vec![level],
            Err(leaf) => return Ok(*leaf),
        };
        loop {
            let top = stack.last_mut().expect("a level");
            if let Some(child) = top.children.get(top.next) {
                top.next += 1;
                let child = match (child.clone(), nulls) {
                    (Some(child), _) => child,
                    (None, Holes::Placeholder) => {
                        top.copy.append_transform(hole());
                        continue;
                    }
                    (None, Holes::Error) => return Err(null_child()),
                };
                if stack.iter().any(|l| l.node.same(&child)) {
                    match cycles {
                        Holes::Placeholder => {
                            stack
                                .last_mut()
                                .expect("a level")
                                .copy
                                .append_transform(hole());
                            continue;
                        }
                        Holes::Error => return Err(cycle()),
                    }
                }
                match open(&child) {
                    Ok(level) => stack.push(level),
                    Err(leaf) => stack
                        .last_mut()
                        .expect("a level")
                        .copy
                        .append_transform(*leaf),
                }
                continue;
            }
            let done = stack.pop().expect("a level");
            match stack.last_mut() {
                Some(parent) => parent.copy.append_transform(Transform::Group(done.copy)),
                None => return Ok(Transform::Group(done.copy)),
            }
        }
    }
}

/// What [`TransformRc::materialize_with`] puts for a null child or a cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Holes {
    /// The port's error (U-110, U-111): the wheel dereferences the null pointer or recurses
    /// without end.
    Error,
    /// An empty group.
    Placeholder,
}

/// The port's error for a null child where the wheel dereferences it (U-110).
pub(crate) fn null_child() -> PyErr {
    pyo3::exceptions::PyRuntimeError::new_err(
        "the group holds a null transform (a None it was given), which the wheel dereferences \
         here (docs/improvements.md, U-110)",
    )
}

/// The port's error for a group that holds itself where the wheel recurses through it (U-111).
pub(crate) fn cycle() -> PyErr {
    pyo3::exceptions::PyRuntimeError::new_err(
        "the group holds itself, through which the wheel recurses until its stack overflows \
         (docs/improvements.md, U-111)",
    )
}

/// The library's direction for a direction as the C++ enum holds it.
pub(crate) fn library_direction(direction: i64) -> TransformDirection {
    if direction == 0 {
        TransformDirection::Forward
    } else {
        TransformDirection::Inverse
    }
}

/// The C++ value a Python transform object holds: pybind11's holder.
#[pyclass(frozen, module = "pybind11_builtins", name = "_transform_holder")]
#[derive(Debug)]
pub(crate) struct TransformHolder(pub(crate) TransformRc);

// ---------------------------------------------------------------------------------------------
// The registry and the polymorphic cast

/// pybind11's registered instances (`registered_instances`, an `unordered_multimap` from a C++
/// pointer to the Python objects made for it, from which an object's deallocation removes it):
/// the live Python objects by key, in the order they were registered, each by a weak reference
/// whose callback removes it when the object dies. So it holds live objects only, and a lookup
/// reads its own key's entries only, whatever else lives.
///
/// Never touch Python while holding its lock beyond reading a weak reference (no allocation,
/// which can collect garbage, and no last reference dropped): an object dying then would call
/// the callback, which takes the lock again.
#[derive(Debug)]
pub(crate) struct Registry<K> {
    /// The weak references by key.
    by_key: HashMap<K, Vec<Py<PyWeakrefReference>>>,
    /// Each weak reference's key, by the weak reference's address.
    keys: HashMap<usize, K>,
}

impl<K: Eq + Hash + Clone> Registry<K> {
    fn new() -> Self {
        Registry {
            by_key: HashMap::new(),
            keys: HashMap::new(),
        }
    }

    /// The first object registered under `key` that lives.
    fn find<'py>(&self, py: Python<'py>, key: &K) -> Option<Bound<'py, PyAny>> {
        self.by_key.get(key)?.iter().find_map(|wr| {
            EXAMINED.fetch_add(1, Ordering::Relaxed);
            wr.bind(py).upgrade()
        })
    }

    fn insert(&mut self, key: K, wr: Py<PyWeakrefReference>) {
        self.keys.insert(wr.as_ptr() as usize, key.clone());
        self.by_key.entry(key).or_default().push(wr);
    }

    /// Removes the weak reference at `address`; it, to be dropped once the lock is released.
    fn remove(&mut self, address: usize) -> Option<Py<PyWeakrefReference>> {
        let key = self.keys.remove(&address)?;
        let list = self.by_key.get_mut(&key)?;
        let i = list.iter().position(|wr| wr.as_ptr() as usize == address)?;
        let wr = list.remove(i);
        if list.is_empty() {
            self.by_key.remove(&key);
        }
        Some(wr)
    }
}

/// A registry, locked.
fn lock<K>(registry: &'static LazyLock<Mutex<Registry<K>>>) -> MutexGuard<'static, Registry<K>> {
    registry
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Registers `obj` under `key` in `registry`, by a weak reference whose callback (`callback`,
/// made once) removes it when `obj` dies.
fn register_in<K: Eq + Hash + Clone + Send + 'static>(
    registry: &'static LazyLock<Mutex<Registry<K>>>,
    callback: &'static PyOnceLock<Py<PyCFunction>>,
    obj: &Bound<'_, PyAny>,
    key: K,
) -> PyResult<()> {
    let py = obj.py();
    let callback = callback.get_or_try_init(py, || {
        PyCFunction::new_closure(py, None, None, move |args, _| -> PyResult<()> {
            let wr = args.get_item(0)?;
            let removed = lock(registry).remove(wr.as_ptr() as usize);
            drop(removed);
            Ok(())
        })
        .map(Bound::unbind)
    })?;
    let wr = PyWeakrefReference::new_with(obj, callback.bind(py))?;
    lock(registry).insert(key, wr.unbind());
    Ok(())
}

/// The weak references the registries' lookups have read, for the tests of their cost.
static EXAMINED: AtomicUsize = AtomicUsize::new(0);

/// The registries' state, for tests: what the registries of transforms and of their format
/// metadata hold, and how many entries their lookups have read in all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegistryStats {
    /// The transform objects registered.
    pub transforms: usize,
    /// The `FormatMetadata` objects registered.
    pub format_metadata: usize,
    /// The entries the lookups have read since the module was loaded.
    pub examined: usize,
}

/// [`RegistryStats`] now.
pub fn registry_stats() -> RegistryStats {
    RegistryStats {
        transforms: lock(&INSTANCES).keys.len(),
        format_metadata: lock(&METADATA_INSTANCES).keys.len(),
        examined: EXAMINED.load(Ordering::Relaxed),
    }
}

/// pybind11's registered instances of the transform classes, by node ([`Registry`]).
static INSTANCES: LazyLock<Mutex<Registry<usize>>> = LazyLock::new(|| Mutex::new(Registry::new()));
static INSTANCES_CALLBACK: PyOnceLock<Py<PyCFunction>> = PyOnceLock::new();

/// The class of the module for a transform type (the `polymorphic_type_hook`).
pub(crate) fn class_name(t: TransformType) -> &'static str {
    match t {
        TransformType::Allocation => q!("AllocationTransform"),
        TransformType::Builtin => q!("BuiltinTransform"),
        TransformType::Cdl => q!("CDLTransform"),
        TransformType::ColorSpace => q!("ColorSpaceTransform"),
        TransformType::DisplayView => q!("DisplayViewTransform"),
        TransformType::Exponent => q!("ExponentTransform"),
        TransformType::ExponentWithLinear => q!("ExponentWithLinearTransform"),
        TransformType::ExposureContrast => q!("ExposureContrastTransform"),
        TransformType::File => q!("FileTransform"),
        TransformType::FixedFunction => q!("FixedFunctionTransform"),
        TransformType::GradingHueCurve => q!("GradingHueCurveTransform"),
        TransformType::GradingPrimary => q!("GradingPrimaryTransform"),
        TransformType::GradingRgbCurve => q!("GradingRGBCurveTransform"),
        TransformType::GradingTone => q!("GradingToneTransform"),
        TransformType::Group => q!("GroupTransform"),
        TransformType::LogAffine => q!("LogAffineTransform"),
        TransformType::LogCamera => q!("LogCameraTransform"),
        TransformType::Log => q!("LogTransform"),
        TransformType::Look => q!("LookTransform"),
        TransformType::Lut1D => q!("Lut1DTransform"),
        TransformType::Lut3D => q!("Lut3DTransform"),
        TransformType::Matrix => q!("MatrixTransform"),
        TransformType::Range => q!("RangeTransform"),
    }
}

/// The Python object of the live instance registered for `rc`, if any.
fn registered<'py>(py: Python<'py>, rc: &TransformRc) -> Option<Bound<'py, PyAny>> {
    lock(&INSTANCES).find(py, &rc.key())
}

/// Registers `obj` as the instance of `rc`.
fn register(obj: &Bound<'_, PyAny>, rc: &TransformRc) -> PyResult<()> {
    register_in(&INSTANCES, &INSTANCES_CALLBACK, obj, rc.key())
}

/// A returned `TransformRcPtr` (or a subclass's) as pybind11 casts it: `None` for a null
/// pointer; the Python object registered for the node while it lives; else a new instance of
/// the node's most derived class, holding the node, registered.
pub(crate) fn to_py<'py>(py: Python<'py>, rc: Option<&TransformRc>) -> PyResult<Bound<'py, PyAny>> {
    let Some(rc) = rc else {
        return Ok(py.None().into_bound(py));
    };
    if let Some(obj) = registered(py, rc) {
        return Ok(obj);
    }
    let qualified = class_name(rc.transform_type());
    let cls = pybind11::class(py, qualified)
        .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err(qualified.to_string()))?;
    let obj = cls.call_method1("__new__", (&cls,))?;
    pybind11::set_value(
        &obj,
        qualified,
        Bound::new(py, TransformHolder(rc.clone()))?.into_any(),
    )?;
    register(&obj, rc)?;
    Ok(obj)
}

/// A constructor's result: `rc` as the value of `this` as an instance of the class `qualified`
/// (pybind11's `py::init` of a factory returning the holder), registered.
pub(crate) fn construct(this: &Bound<'_, PyAny>, qualified: &str, rc: TransformRc) -> PyResult<()> {
    pybind11::set_value(
        this,
        qualified,
        Bound::new(this.py(), TransformHolder(rc.clone()))?.into_any(),
    )?;
    register(this, &rc)
}

/// The node `obj` holds as an instance of the class `qualified`, when it is one (`Ok(None)`:
/// not an instance, the next overload). An instance no constructor constructed holds
/// uninitialized memory in the wheel: the port's error (U-86).
pub(crate) fn holder(obj: &Bound<'_, PyAny>, qualified: &str) -> PyResult<Option<TransformRc>> {
    let Some(this) = pybind11::instance_of(obj, qualified) else {
        return Ok(None);
    };
    let value = pybind11::read_value(&this, qualified)?;
    let holder = value.cast::<TransformHolder>()?;
    Ok(Some(holder.get().0.clone()))
}

/// A `TransformRcPtr` argument (pybind11's holder caster): an instance of the class
/// `qualified`; with conversions, also `None`, a null pointer. `Ok(None)`: refused.
pub(crate) fn holder_arg(
    obj: &Bound<'_, PyAny>,
    qualified: &str,
    convert: bool,
) -> PyResult<Option<Option<TransformRc>>> {
    if obj.is_none() {
        return Ok(convert.then_some(None));
    }
    Ok(holder(obj, qualified)?.map(Some))
}

/// The node of `self` (an overload's first parameter, whose type names the class), or the next
/// overload.
#[macro_export]
macro_rules! this_rc {
    ($o:expr, $a:expr) => {
        match $crate::py_transform::holder(&$a[0], $o.params[0].ty)? {
            Some(rc) => rc,
            None => return Ok(None),
        }
    };
}

/// An argument of a class type held by reference (`const MatrixTransform & other`): an
/// instance of the class its parameter names (`None` refused: pybind11's reference cast), or
/// the next overload.
#[macro_export]
macro_rules! ref_rc {
    ($o:expr, $a:expr, $i:expr) => {
        match $crate::py_transform::holder(&$a[$i], $o.params[$i].ty)? {
            Some(rc) => rc,
            None => return Ok(None),
        }
    };
}

/// A transform class of the library: its variant of [`Transform`].
pub(crate) trait Variant: Sized {
    fn get(t: &Transform) -> Option<&Self>;
    fn get_mut(t: &mut Transform) -> Option<&mut Self>;
}

macro_rules! variant {
    ($t:ty, $v:ident) => {
        impl Variant for $t {
            fn get(t: &Transform) -> Option<&Self> {
                match t {
                    Transform::$v(x) => Some(x),
                    _ => None,
                }
            }
            fn get_mut(t: &mut Transform) -> Option<&mut Self> {
                match t {
                    Transform::$v(x) => Some(x),
                    _ => None,
                }
            }
        }
    };
}

variant!(ocio::AllocationTransform, Allocation);
variant!(ocio::BuiltinTransform, Builtin);
variant!(ocio::CdlTransform, Cdl);
variant!(ocio::ExponentTransform, Exponent);
variant!(ocio::ExponentWithLinearTransform, ExponentWithLinear);
variant!(ocio::FileTransform, File);
variant!(ocio::FixedFunctionTransform, FixedFunction);
variant!(ocio::GroupTransform, Group);
variant!(ocio::LogAffineTransform, LogAffine);
variant!(ocio::LogCameraTransform, LogCamera);
variant!(ocio::LogTransform, Log);
variant!(ocio::transforms::matrix_transform::MatrixTransform, Matrix);
variant!(ocio::transforms::range_transform::RangeTransform, Range);

/// `f` of the node's transform as the class `T`, which a node of a class's instance always is.
pub(crate) fn with<T: Variant, R>(rc: &TransformRc, f: impl FnOnce(&mut T) -> R) -> PyResult<R> {
    let mut node = rc.lock();
    let t = T::get_mut(&mut node.transform).ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("the instance holds another transform class")
    })?;
    Ok(f(t))
}

// ---------------------------------------------------------------------------------------------
// Returned values

pub(crate) type Out = PyResult<Option<Py<PyAny>>>;

/// `None`.
pub(crate) fn none(py: Python<'_>) -> Out {
    Ok(Some(py.None()))
}

/// Any value Python converts.
pub(crate) fn ok<'py>(py: Python<'py>, v: impl IntoPyObject<'py>) -> Out {
    Ok(Some(
        v.into_pyobject(py).map_err(Into::into)?.into_any().unbind(),
    ))
}

/// A returned C++ enum of the class `qualified`: a new instance.
pub(crate) fn enum_out(py: Python<'_>, qualified: &str, value: i64) -> Out {
    Ok(Some(
        crate::enums::to_py(&enum_class(py, qualified)?, value)?.unbind(),
    ))
}

/// A returned `std::string` (strict UTF-8).
pub(crate) fn string_out(py: Python<'_>, bytes: &[u8]) -> Out {
    Ok(Some(
        crate::dispatch::std_string_to_py(py, bytes)?
            .into_any()
            .unbind(),
    ))
}

/// A returned transform (`TransformRcPtr`), polymorphic.
pub(crate) fn transform_out(py: Python<'_>, rc: Option<&TransformRc>) -> Out {
    Ok(Some(to_py(py, rc)?.unbind()))
}

/// The value of the enum argument `a[i]` (its class the parameter's type), or the next
/// overload.
#[macro_export]
macro_rules! enum_param {
    ($o:expr, $a:expr, $i:expr) => {
        match $crate::enums::enum_value(&$a[$i], $o.params[$i].ty)? {
            Some(v) => v,
            None => return Ok(None),
        }
    };
}

// ---------------------------------------------------------------------------------------------
// Defining classes

/// A method of one overload, `def!(name, [params], ret, call)`.
#[macro_export]
macro_rules! method_def {
    ($name:expr, [$($param:expr),*], $ret:expr, $call:expr) => {
        $crate::dispatch::Function {
            name: $name,
            constructor: false,
            method: true,
            overloads: &[$crate::dispatch::Overload {
                params: &[$($param),*],
                ret: $ret,
                call: $call,
                pos_only: 0,
            }],
        }
    };
}

/// A static method of one overload (`def_static`).
#[macro_export]
macro_rules! static_def {
    ($name:expr, [$($param:expr),*], $ret:expr, $call:expr) => {
        $crate::dispatch::Function {
            name: $name,
            constructor: false,
            method: false,
            overloads: &[$crate::dispatch::Overload {
                params: &[$($param),*],
                ret: $ret,
                call: $call,
                pos_only: 0,
            }],
        }
    };
}

/// `cls.def(name, ...)`: the method into the class's dictionary.
pub(crate) fn def(cls: &Bound<'_, PyType>, f: &'static Function) -> PyResult<()> {
    cls.setattr(f.name, pybind11::method(cls.py(), f)?)
}

/// `cls.def_static(name, ...)`: `staticmethod` of the function.
pub(crate) fn def_static(cls: &Bound<'_, PyType>, f: &'static Function) -> PyResult<()> {
    let py = cls.py();
    let staticmethod = py.import("builtins")?.getattr("staticmethod")?;
    cls.setattr(
        f.name,
        staticmethod.call1((pybind11::builtin(py, f, true)?,))?,
    )
}

/// The module's class `qualified`.
pub(crate) fn class<'py>(py: Python<'py>, qualified: &str) -> PyResult<Bound<'py, PyType>> {
    pybind11::class(py, qualified)
        .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err(qualified.to_string()))
}

/// A class pybind11 nests in `outer` without a docstring (`py::class_<T>(outer, name)`): its
/// `__doc__` None, then `_pybind11_conduit_v1_`.
pub(crate) fn nested_class<'py>(
    outer: &Bound<'py, PyType>,
    outer_name: &str,
    name: &str,
) -> PyResult<Bound<'py, PyType>> {
    let py = outer.py();
    let cls = pybind11::new_class(
        py,
        name,
        &format!("{outer_name}.{name}"),
        &[],
        &[("__doc__", py.None().into_bound(py))],
    )?;
    cls.setattr("_pybind11_conduit_v1_", crate::enums::conduit(py)?)?;
    outer.setattr(name, &cls)?;
    Ok(cls)
}

/// A default argument: its repr in the signature and its value.
pub(crate) const fn default(
    repr: &'static str,
    value: for<'py> fn(Python<'py>) -> PyResult<Bound<'py, PyAny>>,
) -> Option<crate::dispatch::Default> {
    Some(crate::dispatch::Default { repr, value })
}

/// `direction: TransformDirection = <TransformDirection.TRANSFORM_DIR_FORWARD: 0>`, every
/// constructor's last parameter (`DEFAULT->getDirection()`).
pub(crate) const DIRECTION_DEFAULT: Param = Param {
    name: "direction",
    ty: TRANSFORM_DIRECTION,
    default: default(
        "<TransformDirection.TRANSFORM_DIR_FORWARD: 0>",
        forward_direction,
    ),
};

fn forward_direction(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    crate::enums::to_py(&enum_class(py, TRANSFORM_DIRECTION)?, 0)
}

/// A list of floats, as pybind11 makes the default of a `std::array<double, N>` or a
/// `std::vector<float>` parameter.
pub(crate) fn float_list<'py>(py: Python<'py>, values: &[f64]) -> PyResult<Bound<'py, PyAny>> {
    Ok(pyo3::types::PyList::new(py, values)?.into_any())
}

/// The type pybind11 writes for a `std::array<double, N>` parameter.
#[macro_export]
macro_rules! float_array {
    ($n:literal) => {
        concat!(
            "typing.Annotated[collections.abc.Sequence[typing.SupportsFloat], \"FixedSize(",
            $n,
            ")\"]"
        )
    };
}

/// The type pybind11 writes for a returned `std::array<double, N>`.
#[macro_export]
macro_rules! float_array_out {
    ($n:literal) => {
        concat!("typing.Annotated[list[float], \"FixedSize(", $n, ")\"]")
    };
}

pub(crate) const FLOAT: &str = "typing.SupportsFloat";
pub(crate) const INT: &str = "typing.SupportsInt";

// ---------------------------------------------------------------------------------------------
// The base class

/// `self` of the base class's methods.
pub(crate) const SELF: Param = Param::new("self", TRANSFORM);

// .def("__deepcopy__", [](const ConstTransformRcPtr & self, py::dict) { return
// self->createEditableCopy(); }, "memo"_a) (PyTransform.cpp:16-20 @ v2.5.2).
static DEEPCOPY: Function = method_def!(
    "__deepcopy__",
    [SELF, Param::new("memo", "dict")],
    TRANSFORM,
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        if !a[1].is_instance_of::<PyDict>() {
            return Ok(None);
        }
        transform_out(py, Some(&create_editable_copy(py, &rc)?))
    }
);

/// `createEditableCopy` of the node: a new node of a copy of its transform (a
/// FixedFunctionTransform's copy validates it); a group's copy shares its children's nodes, as
/// upstream's (`GroupTransformImpl::createEditableCopy`, GroupTransform.cpp:37-45 @ v2.5.2).
pub(crate) fn create_editable_copy(py: Python<'_>, rc: &TransformRc) -> PyResult<TransformRc> {
    let node = rc.lock();
    let transform = py_result(py, node.transform.create_editable_copy())?;
    Ok(TransformRc(Arc::new(Mutex::new(Node {
        transform,
        children: node.children.clone(),
        direction: node.direction,
    }))))
}

// .def("validate", &Transform::validate) (PyTransform.cpp:22-23 @ v2.5.2).
static VALIDATE: Function = method_def!("validate", [SELF], "None", |py, o, a, _| {
    let rc = this_rc!(o, a);
    validate(py, &rc)?;
    none(py)
});

/// `validate` of the node, as upstream's runs it: each node in order, a group before its
/// children (`GroupTransformImpl::validate`, GroupTransform.cpp:56-73 @ v2.5.2), the first
/// error as it is. A node checks its direction first (`Transform::validate`,
/// Transform.cpp:30-40), which only Python can set outside the enumerators
/// ([`invalid_direction`]); then a transform other than a group is the library's to check. A
/// null child or a group that holds itself is reached only once every node before it passed:
/// the port's errors, as in [`TransformRc::materialize`].
pub(crate) fn validate(py: Python<'_>, rc: &TransformRc) -> PyResult<()> {
    // Each level: a group's node and its children, the next child.
    let mut stack: Vec<(TransformRc, Vec<Option<TransformRc>>, usize)> = Vec::new();
    let mut current = Some(rc.clone());
    loop {
        if let Some(rc) = current.take() {
            let (transform, children, direction) = {
                let node = rc.lock();
                let children = node.children.clone();
                let transform = match &node.transform {
                    Transform::Group(_) => None,
                    other => Some(other.clone()),
                };
                (transform, children, direction_of(&node))
            };
            let t = rc.transform_type();
            if direction != 0 && direction != 1 {
                return Err(crate::py_open_color_io::to_py_err(
                    py,
                    &ocio::Exception::new(invalid_direction(t)),
                ));
            }
            match transform {
                Some(transform) => py_result(py, transform.validate())?,
                None => stack.push((rc, children, 0)),
            }
        }
        let Some((_, children, next)) = stack.last_mut() else {
            return Ok(());
        };
        let Some(child) = children.get(*next).cloned() else {
            stack.pop();
            continue;
        };
        *next += 1;
        let Some(child) = child else {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "the group holds a null transform (a None it was given), which the wheel \
                 dereferences here (docs/improvements.md, U-110)",
            ));
        };
        if stack.iter().any(|(g, _, _)| g.same(&child)) {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "the group holds itself, through which the wheel recurses until its stack \
                 overflows (docs/improvements.md, U-111)",
            ));
        }
        current = Some(child);
    }
}

/// The error of `Transform::validate` (Transform.cpp:30-40 @ v2.5.2) for a direction outside the
/// enumerators, as each class's `validate` passes it on: its C++ class's `typeid` name, which
/// the compiler writes (MSVC's `class OpenColorIO_v2_5::MatrixTransformImpl`, GCC's mangled
/// `N16OpenColorIO_v2_519MatrixTransformImplE`), after the class's own prefix where its
/// `validate` adds one ("MatrixTransform validation failed: "; `BuiltinTransformImpl` has
/// none). docs/improvements.md, I-261.
pub(crate) fn invalid_direction(t: TransformType) -> String {
    let (prefix, class) = match t {
        TransformType::Allocation => ("AllocationTransform", "AllocationTransform"),
        TransformType::Builtin => ("", "BuiltinTransformImpl"),
        TransformType::Cdl => ("CDLTransform", "CDLTransformImpl"),
        TransformType::ColorSpace => ("ColorSpaceTransform", "ColorSpaceTransform"),
        TransformType::DisplayView => ("DisplayViewTransform", "DisplayViewTransform"),
        TransformType::Exponent => ("ExponentTransform", "ExponentTransformImpl"),
        TransformType::ExponentWithLinear => (
            "ExponentWithLinearTransform",
            "ExponentWithLinearTransformImpl",
        ),
        TransformType::ExposureContrast => {
            ("ExposureContrastTransform", "ExposureContrastTransformImpl")
        }
        TransformType::File => ("FileTransform", "FileTransform"),
        TransformType::FixedFunction => ("FixedFunctionTransform", "FixedFunctionTransformImpl"),
        TransformType::GradingHueCurve => {
            ("GradingHueCurveTransform", "GradingHueCurveTransformImpl")
        }
        TransformType::GradingPrimary => ("GradingPrimaryTransform", "GradingPrimaryTransformImpl"),
        TransformType::GradingRgbCurve => {
            ("GradingRGBCurveTransform", "GradingRGBCurveTransformImpl")
        }
        TransformType::GradingTone => ("GradingToneTransform", "GradingToneTransformImpl"),
        TransformType::Group => ("GroupTransform", "GroupTransformImpl"),
        TransformType::LogAffine => ("LogAffineTransform", "LogAffineTransformImpl"),
        TransformType::LogCamera => ("LogCameraTransform", "LogCameraTransformImpl"),
        TransformType::Log => ("LogTransform", "LogTransformImpl"),
        TransformType::Look => ("LookTransform", "LookTransform"),
        TransformType::Lut1D => ("Lut1DTransform", "Lut1DTransformImpl"),
        TransformType::Lut3D => ("Lut3DTransform", "Lut3DTransformImpl"),
        TransformType::Matrix => ("MatrixTransform", "MatrixTransformImpl"),
        TransformType::Range => ("RangeTransform", "RangeTransformImpl"),
    };
    let prefix = if prefix.is_empty() {
        String::new()
    } else {
        format!("{prefix} validation failed: ")
    };
    format!("{prefix}{}: invalid direction.", typeid_name(class))
}

/// `typeid(T).name()` of the class `class` of OCIO's namespace, as the wheel's compiler writes
/// it.
fn typeid_name(class: &str) -> String {
    if cfg!(windows) {
        format!("class OpenColorIO_v2_5::{class}")
    } else {
        format!("N16OpenColorIO_v2_5{}{class}E", class.len())
    }
}

// .def("getTransformType", &Transform::getTransformType) (PyTransform.cpp:24-25 @ v2.5.2).
static GET_TRANSFORM_TYPE: Function =
    method_def!("getTransformType", [SELF], TRANSFORM_TYPE, |py, o, a, _| {
        let rc = this_rc!(o, a);
        let t = rc.transform_type();
        enum_out(py, TRANSFORM_TYPE, t as i64)
    });

// .def("getDirection", &Transform::getDirection) (PyTransform.cpp:26-27 @ v2.5.2).
static GET_DIRECTION: Function = method_def!(
    "getDirection",
    [SELF],
    TRANSFORM_DIRECTION,
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        let direction = direction_of(&rc.lock());
        enum_out(py, TRANSFORM_DIRECTION, direction)
    }
);

// .def("setDirection", &Transform::setDirection, "direction"_a) (PyTransform.cpp:28-29 @
// v2.5.2).
static SET_DIRECTION: Function = method_def!(
    "setDirection",
    [SELF, Param::new("direction", TRANSFORM_DIRECTION)],
    "None",
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        let direction = enum_param!(o, a, 1);
        set_direction(&rc, direction);
        none(py)
    }
);

/// `setDirection` of the node, with the C++ enum's value. Most classes keep the value as it is.
/// Those whose op data hold the direction in a style (CDL, exponent, exponent with linear,
/// exposure contrast, fixed function: `GammaOpData::setDirection`, ops/gamma/GammaOpData.cpp:
/// 840-846 @ v2.5.2, and their likes) invert themselves when the value isn't their direction,
/// so a value outside the enumerators flips them (docs/improvements.md, I-261); their direction
/// is the library's, which their other setters may change too (a fixed function's style, I-80).
pub(crate) fn set_direction(rc: &TransformRc, direction: i64) {
    let mut node = rc.lock();
    let direction = if keeps_direction(&node.transform) || direction == 0 || direction == 1 {
        direction
    } else {
        1 - library_value(node.transform.direction())
    };
    node.direction = direction;
    node.transform.set_direction(library_direction(direction));
}

/// Whether the class keeps the C++ enum's value of its direction as it is given (see
/// [`set_direction`]).
fn keeps_direction(t: &Transform) -> bool {
    !matches!(
        t.transform_type(),
        TransformType::Cdl
            | TransformType::Exponent
            | TransformType::ExponentWithLinear
            | TransformType::ExposureContrast
            | TransformType::FixedFunction
    )
}

/// The C++ value of a library direction.
fn library_value(dir: TransformDirection) -> i64 {
    match dir {
        TransformDirection::Forward => 0,
        TransformDirection::Inverse => 1,
    }
}

/// `getDirection` of a node: the value it keeps, or the library's (see [`set_direction`]).
pub(crate) fn direction_of(node: &Node) -> i64 {
    if keeps_direction(&node.transform) {
        node.direction
    } else {
        library_value(node.transform.direction())
    }
}

/// `__repr__` (`defRepr`, PyUtils.h:17-26 @ v2.5.2): `operator<<` of the transform, decoded as
/// strict UTF-8.
pub(crate) fn repr(py: Python<'_>, rc: &TransformRc) -> Out {
    let transform = rc.materialize()?;
    string_out(py, &transform.to_bytes())
}

/// `defRepr(cls)` of the class `qualified`'s instances: `__repr__(self: <class>) -> str`.
#[macro_export]
macro_rules! repr_def {
    ($self:expr) => {
        $crate::method_def!("__repr__", [$self], "str", |py, o, a, _| {
            let rc = $crate::this_rc!(o, a);
            $crate::py_transform::repr(py, &rc)
        })
    };
}

static REPR: Function = repr_def!(SELF);

/// Port of `bindPyTransform` (src/bindings/python/PyTransform.cpp:8-62 @ v2.5.2): the base
/// class's methods (13-31) and `__repr__` (33), then the subclasses' bindings (35-60), those
/// ported so far.
pub(crate) fn bind_py_transform(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let cls = class(py, TRANSFORM)?;
    for f in [
        &DEEPCOPY,
        &VALIDATE,
        &GET_TRANSFORM_TYPE,
        &GET_DIRECTION,
        &SET_DIRECTION,
        &REPR,
    ] {
        def(&cls, f)?;
    }

    crate::transforms::py_allocation_transform::bind(m)?;
    crate::transforms::py_builtin_transform::bind(m)?;
    crate::transforms::py_cdl_transform::bind(m)?;
    crate::transforms::py_exponent_transform::bind(m)?;
    crate::transforms::py_exponent_with_linear_transform::bind(m)?;
    crate::transforms::py_file_transform::bind(m)?;
    crate::transforms::py_fixed_function_transform::bind(m)?;
    crate::transforms::py_group_transform::bind(m)?;
    crate::transforms::py_log_affine_transform::bind(m)?;
    crate::transforms::py_log_camera_transform::bind(m)?;
    crate::transforms::py_log_transform::bind(m)?;
    crate::transforms::py_matrix_transform::bind(m)?;
    crate::transforms::py_range_transform::bind(m)?;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// What several classes share

/// `equals(other)` of two nodes of the class `T`: `f(self, other)`, on the very same transform
/// when both are one node (upstream's `this == &other` then answers, NaNs and all).
pub(crate) fn equals<T: Variant + Clone>(
    rc: &TransformRc,
    other: &TransformRc,
    f: impl FnOnce(&T, &T) -> bool,
) -> PyResult<bool> {
    let wrong = || pyo3::exceptions::PyRuntimeError::new_err("another transform class");
    if rc.same(other) {
        let node = rc.lock();
        let t = T::get(&node.transform).ok_or_else(wrong)?;
        return Ok(f(t, t));
    }
    let theirs = T::get(&other.lock().transform).ok_or_else(wrong)?.clone();
    let node = rc.lock();
    let t = T::get(&node.transform).ok_or_else(wrong)?;
    Ok(f(t, &theirs))
}

/// The port's error for a C++ enum value outside its enumerators, which Python can make
/// (I-223) and the wheel's setters store, but the library's enums can't hold
/// (docs/improvements.md, U-112).
pub(crate) fn outside_enumerators(qualified: &str, v: i64) -> PyErr {
    pyo3::exceptions::PyValueError::new_err(format!(
        "{qualified}({v}): the wheel stores an enum value outside its enumerators, which the \
         port's library can't hold (docs/improvements.md, U-112)"
    ))
}

/// The wheel's error for a style outside its enumerators that the library converts at once
/// (and so never stores): `std::stringstream ss(initial); ss << style;` at the end of the
/// conversion's `switch`, which writes the number over the start of `initial`
/// (docs/improvements.md, I-262). The number is the enum's promotion, an `int`: the `int` the
/// Windows wheel stores, and the `unsigned int` the Linux one stores (I-225) wrapped to an
/// `int`.
///
/// Port of the end of `GammaOpData::ConvertStyleBasic` and `ConvertStyleMonCurve`
/// (src/OpenColorIO/ops/gamma/GammaOpData.cpp:205-208, 238-241 @ v2.5.2),
/// `FixedFunctionOpData::ConvertStyle(FixedFunctionStyle, TransformDirection)`
/// (src/OpenColorIO/ops/fixedfunction/FixedFunctionOpData.cpp:488-491 @ v2.5.2) and
/// `CDLOpData::ConvertStyle(CDLStyle, TransformDirection)` (src/OpenColorIO/ops/cdl/
/// CDLOpData.cpp:102-105 @ v2.5.2).
pub(crate) fn unknown_style(py: Python<'_>, initial: &str, v: i64) -> PyErr {
    crate::py_open_color_io::to_py_err(
        py,
        &ocio::Exception::new(ocio_ops::ops::log::log_utils::overwritten(
            initial,
            &(v as i32).to_string(),
        )),
    )
}

/// The library's `BitDepth` of the C++ value `v` ([`outside_enumerators`]).
pub(crate) fn bit_depth(v: i64) -> PyResult<ocio_ops::open_color_types::BitDepth> {
    use ocio_ops::open_color_types::BitDepth::*;
    [
        Unknown, Uint8, Uint10, Uint12, Uint14, Uint16, Uint32, F16, F32,
    ]
    .into_iter()
    .find(|b| *b as i64 == v)
    .ok_or_else(|| outside_enumerators(BIT_DEPTH, v))
}

/// The four file bit depth methods of the class `$t` (`getFileInputBitDepth`,
/// `setFileInputBitDepth` "bitDepth"_a, `getFileOutputBitDepth`, `setFileOutputBitDepth`), as
/// statics `GET_FILE_INPUT_BIT_DEPTH` and the others.
#[macro_export]
macro_rules! file_bit_depths {
    ($t:ty, $self:expr) => {
        static GET_FILE_INPUT_BIT_DEPTH: $crate::dispatch::Function = $crate::method_def!(
            "getFileInputBitDepth",
            [$self],
            $crate::py_transform::BIT_DEPTH,
            |py, o, a, _| {
                let rc = $crate::this_rc!(o, a);
                let bd = $crate::py_transform::with(&rc, |t: &mut $t| t.file_input_bit_depth())?;
                $crate::py_transform::enum_out(py, $crate::py_transform::BIT_DEPTH, bd as i64)
            }
        );
        static SET_FILE_INPUT_BIT_DEPTH: $crate::dispatch::Function = $crate::method_def!(
            "setFileInputBitDepth",
            [
                $self,
                $crate::dispatch::Param::new("bitDepth", $crate::py_transform::BIT_DEPTH)
            ],
            "None",
            |py, o, a, _| {
                let rc = $crate::this_rc!(o, a);
                let bd = $crate::py_transform::bit_depth($crate::enum_param!(o, a, 1))?;
                $crate::py_transform::with(&rc, |t: &mut $t| t.set_file_input_bit_depth(bd))?;
                $crate::py_transform::none(py)
            }
        );
        static GET_FILE_OUTPUT_BIT_DEPTH: $crate::dispatch::Function = $crate::method_def!(
            "getFileOutputBitDepth",
            [$self],
            $crate::py_transform::BIT_DEPTH,
            |py, o, a, _| {
                let rc = $crate::this_rc!(o, a);
                let bd = $crate::py_transform::with(&rc, |t: &mut $t| t.file_output_bit_depth())?;
                $crate::py_transform::enum_out(py, $crate::py_transform::BIT_DEPTH, bd as i64)
            }
        );
        static SET_FILE_OUTPUT_BIT_DEPTH: $crate::dispatch::Function = $crate::method_def!(
            "setFileOutputBitDepth",
            [
                $self,
                $crate::dispatch::Param::new("bitDepth", $crate::py_transform::BIT_DEPTH)
            ],
            "None",
            |py, o, a, _| {
                let rc = $crate::this_rc!(o, a);
                let bd = $crate::py_transform::bit_depth($crate::enum_param!(o, a, 1))?;
                $crate::py_transform::with(&rc, |t: &mut $t| t.set_file_output_bit_depth(bd))?;
                $crate::py_transform::none(py)
            }
        );
    };
}

// ---------------------------------------------------------------------------------------------
// getFormatMetadata

pub(crate) const FORMAT_METADATA: &str = q!("FormatMetadata");

/// The value of a `FormatMetadata` object that `getFormatMetadata` returns: a reference into the
/// transform's metadata (pybind11's `return_value_policy::reference_internal`), which keeps the
/// transform's Python object alive (`keep_alive<0, 1>`) and sees every change of the
/// transform's metadata, and makes its own seen by the transform. `path` leads from the
/// transform's metadata to the element through child indices (empty: the metadata itself).
#[pyclass(frozen, module = "pybind11_builtins", name = "_format_metadata_ref")]
#[derive(Debug)]
#[expect(dead_code, reason = "FormatMetadata's methods (6.3d) read it")]
pub(crate) struct FormatMetadataRef {
    /// The Python object `getFormatMetadata` was called on.
    pub(crate) parent: Py<PyAny>,
    /// Its node.
    pub(crate) node: TransformRc,
    /// The element's child indices.
    pub(crate) path: Vec<usize>,
}

impl FormatMetadataRef {
    #[expect(dead_code, reason = "FormatMetadata's methods (6.3d) call it")]
    /// `f` of the element: the port's error when the transform no longer holds it.
    pub(crate) fn with<R>(&self, f: impl FnOnce(&mut ocio::FormatMetadata) -> R) -> PyResult<R> {
        let mut node = self.node.lock();
        let mut md = metadata_mut(&mut node.transform).ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("the transform holds no format metadata")
        })?;
        for &i in &self.path {
            md = md.get_children_elements_mut().get_mut(i).ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err(
                    "the format metadata no longer holds the element",
                )
            })?;
        }
        Ok(f(md))
    }
}

/// The metadata of a transform class that has some.
#[expect(dead_code, reason = "FormatMetadata's methods (6.3d) call it")]
pub(crate) fn metadata_mut(t: &mut Transform) -> Option<&mut ocio::FormatMetadata> {
    Some(match t {
        Transform::Cdl(t) => t.format_metadata_mut(),
        Transform::Exponent(t) => t.format_metadata_mut(),
        Transform::ExponentWithLinear(t) => t.format_metadata_mut(),
        Transform::FixedFunction(t) => t.format_metadata_mut(),
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

/// The registered `FormatMetadata` objects, by node and path ([`Registry`]).
type MetadataKey = (usize, Vec<usize>);
static METADATA_INSTANCES: LazyLock<Mutex<Registry<MetadataKey>>> =
    LazyLock::new(|| Mutex::new(Registry::new()));
static METADATA_CALLBACK: PyOnceLock<Py<PyCFunction>> = PyOnceLock::new();

/// `getFormatMetadata()` of the instance `this` holding `rc`, as pybind11 returns the reference:
/// the `FormatMetadata` object registered for the element while it lives, else a new one.
pub(crate) fn format_metadata_out(
    py: Python<'_>,
    this: &Bound<'_, PyAny>,
    rc: &TransformRc,
) -> Out {
    let key = (rc.key(), Vec::new());
    if let Some(obj) = lock(&METADATA_INSTANCES).find(py, &key) {
        return Ok(Some(obj.unbind()));
    }
    let cls = class(py, FORMAT_METADATA)?;
    let obj = cls.call_method1("__new__", (&cls,))?;
    let value = FormatMetadataRef {
        parent: this.clone().unbind(),
        node: rc.clone(),
        path: Vec::new(),
    };
    pybind11::set_value(&obj, FORMAT_METADATA, Bound::new(py, value)?.into_any())?;
    register_in(&METADATA_INSTANCES, &METADATA_CALLBACK, &obj, key)?;
    Ok(Some(obj.unbind()))
}

/// `getFormatMetadata(self) -> FormatMetadata` of the class whose `self` parameter is `$self`.
#[macro_export]
macro_rules! format_metadata_def {
    ($self:expr) => {
        $crate::method_def!(
            "getFormatMetadata",
            [$self],
            $crate::py_transform::FORMAT_METADATA,
            |py, o, a, _| {
                let rc = $crate::this_rc!(o, a);
                $crate::py_transform::format_metadata_out(py, &a[0], &rc)
            }
        )
    };
}

pub(crate) const NEGATIVE_STYLE: &str = q!("NegativeStyle");

/// The library's `NegativeStyle` of the C++ value `v`, which `setNegativeStyle` converts at
/// once: the wheel's error for one outside the enumerators ([`unknown_style`]).
pub(crate) fn negative_style(
    py: Python<'_>,
    v: i64,
) -> PyResult<ocio_ops::open_color_types::NegativeStyle> {
    use ocio_ops::open_color_types::NegativeStyle::*;
    [Clamp, Mirror, PassThru, Linear]
        .into_iter()
        .find(|s| *s as i64 == v)
        .ok_or_else(|| unknown_style(py, "Unknown negative extrapolation style: ", v))
}
