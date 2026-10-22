// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! pybind11's `shared_ptr` holders of the classes other than the transforms (chunk 6.3d): a
//! Python object of such a class holds a [`Shared`] value, which other objects (an iterator, a
//! set's color space) can share, so a change through one shows through the others, as through
//! upstream's `ContextRcPtr`, `ColorSpaceRcPtr` and the others; and pybind11's registered
//! instances return the Python object made for a holder while it lives ([`to_py`]).

use std::any::Any;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};

use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::PyCFunction;

use crate::py_transform::{Registry, lock, register_in};
use crate::pybind11;

/// A value as a `shared_ptr` holds it: cloning the handle shares it.
#[derive(Debug)]
pub(crate) struct Shared<T>(Arc<Mutex<T>>);

impl<T> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Shared(self.0.clone())
    }
}

impl<T> Shared<T> {
    pub(crate) fn new(value: T) -> Shared<T> {
        Shared(Arc::new(Mutex::new(value)))
    }

    /// The value, locked. The binding runs under the GIL and locks two values at once only
    /// when they are different ones ([`Shared::same`]), so the lock is never contended.
    pub(crate) fn lock(&self) -> MutexGuard<'_, T> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The value, locked, unless it is locked already (by a call of this thread, under a
    /// callback of the library's, or of another thread whose callback released the GIL).
    pub(crate) fn try_lock(&self) -> Option<MutexGuard<'_, T>> {
        match self.0.try_lock() {
            Ok(g) => Some(g),
            Err(std::sync::TryLockError::Poisoned(e)) => Some(e.into_inner()),
            Err(std::sync::TryLockError::WouldBlock) => None,
        }
    }

    /// The address pybind11's registry keys an instance by.
    pub(crate) fn key(&self) -> usize {
        Arc::as_ptr(&self.0) as *const u8 as usize
    }

    /// Whether both are the same value.
    pub(crate) fn same(&self, other: &Shared<T>) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// The C++ value of an instance of one of these classes: pybind11's holder.
#[pyclass(frozen, module = "pybind11_builtins", name = "_shared_holder")]
pub(crate) struct Holder(Box<dyn Any + Send + Sync>);

/// pybind11's registered instances of these classes, by class and address.
static INSTANCES: LazyLock<Mutex<Registry<(&'static str, usize)>>> =
    LazyLock::new(|| Mutex::new(Registry::new()));
static INSTANCES_CALLBACK: PyOnceLock<Py<PyCFunction>> = PyOnceLock::new();

/// A returned holder of the class `qualified` (the class's qualified name, as signatures write
/// it): the Python object registered for it while it lives, else a new instance holding it.
pub(crate) fn to_py<'py, R: Keyed>(
    py: Python<'py>,
    qualified: &'static str,
    rc: &R,
) -> PyResult<Bound<'py, PyAny>> {
    let key = (qualified, rc.key());
    if let Some(obj) = lock(&INSTANCES).find(py, &key) {
        return Ok(obj);
    }
    let cls = pybind11::class(py, qualified)
        .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err(qualified))?;
    let obj = cls.call_method1("__new__", (&cls,))?;
    pybind11::set_value(
        &obj,
        qualified,
        Bound::new(py, Holder(Box::new(rc.clone())))?.into_any(),
    )?;
    register_in(&INSTANCES, &INSTANCES_CALLBACK, &obj, key)?;
    Ok(obj)
}

/// `to_py` as a call's result.
pub(crate) fn out<R: Keyed>(
    py: Python<'_>,
    qualified: &'static str,
    rc: &R,
) -> PyResult<Option<Py<PyAny>>> {
    Ok(Some(to_py(py, qualified, rc)?.unbind()))
}

/// A constructor's result: `rc` as the value of `this`, an instance of the class `qualified`
/// (pybind11's `py::init` of a factory returning the holder), registered.
pub(crate) fn construct<R: Keyed>(
    this: &Bound<'_, PyAny>,
    qualified: &'static str,
    rc: R,
) -> PyResult<()> {
    let key = (qualified, rc.key());
    pybind11::set_value(
        this,
        qualified,
        Bound::new(this.py(), Holder(Box::new(rc)))?.into_any(),
    )?;
    register_in(&INSTANCES, &INSTANCES_CALLBACK, this, key)
}

/// The holder `obj` holds as an instance of the class `qualified`, when it is one (`Ok(None)`:
/// not an instance, the next overload). An instance no constructor constructed holds
/// uninitialized memory in the wheel: the port's error (U-86).
pub(crate) fn holder<T: Send + 'static>(
    obj: &Bound<'_, PyAny>,
    qualified: &str,
) -> PyResult<Option<Shared<T>>> {
    holder_of::<Shared<T>>(obj, qualified)
}

/// [`holder`] of a class whose holder is `R` (a [`Held`] value).
pub(crate) fn holder_of<R: Keyed>(obj: &Bound<'_, PyAny>, qualified: &str) -> PyResult<Option<R>> {
    let Some(this) = pybind11::instance_of(obj, qualified) else {
        return Ok(None);
    };
    let value = pybind11::read_value(&this, qualified)?;
    let holder = value.cast::<Holder>()?;
    holder
        .get()
        .0
        .downcast_ref::<R>()
        .cloned()
        .map(Some)
        .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("a holder of another class"))
}

/// A holder argument (pybind11's holder caster): an instance of the class `qualified`; with
/// conversions, also `None`, a null pointer. `Ok(None)`: refused.
pub(crate) fn holder_arg<T: Send + 'static>(
    obj: &Bound<'_, PyAny>,
    qualified: &str,
    convert: bool,
) -> PyResult<Option<Option<Shared<T>>>> {
    if obj.is_none() {
        return Ok(convert.then_some(None));
    }
    Ok(holder(obj, qualified)?.map(Some))
}

/// What a holder is: a handle whose clones share one value, keyed by its address.
pub(crate) trait Keyed: Clone + Send + Sync + 'static {
    /// The address pybind11's registry keys an instance by.
    fn key(&self) -> usize;
}

impl<T: Send + 'static> Keyed for Shared<T> {
    fn key(&self) -> usize {
        Shared::key(self)
    }
}

/// A value a config holds and gives out, through the config's own handle (the library's
/// [`ocio::CurrentContext`] and [`ocio::ConfigRules`]): what Python changes through it, the
/// config sees.
pub(crate) trait ConfigCell<T>: Clone + Send + Sync + 'static {
    /// `f` of the value as the config holds it now.
    fn read<R>(&self, f: impl FnOnce(&T) -> R) -> R;
    /// `f` changing the value the config holds.
    fn write<R>(&self, f: impl FnOnce(&mut T) -> R) -> R;
    /// The handle's identity.
    fn cell_key(&self) -> usize;
}

impl ConfigCell<ocio::Context> for ocio::CurrentContext {
    fn read<R>(&self, f: impl FnOnce(&ocio::Context) -> R) -> R {
        f(&self.get())
    }
    fn write<R>(&self, f: impl FnOnce(&mut ocio::Context) -> R) -> R {
        let mut out = None;
        self.update(|c| out = Some(f(c)));
        out.expect("update runs the change")
    }
    fn cell_key(&self) -> usize {
        self.key()
    }
}

impl<T: Clone + Send + Sync + 'static> ConfigCell<T> for ocio::ConfigRules<T> {
    fn read<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(&self.get())
    }
    fn write<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        self.update(f)
    }
    fn cell_key(&self) -> usize {
        self.key()
    }
}

/// The holder of a class whose objects are either their own (`Own`, upstream's
/// `XxxRcPtr` made by `Create`) or a config's (`Config`, the pointer `getCurrentContext`,
/// `getFileRules` or `getViewingRules` returns).
#[derive(Debug)]
pub(crate) enum Held<T, C> {
    Own(Shared<T>),
    Config(C),
}

impl<T, C: Clone> Clone for Held<T, C> {
    fn clone(&self) -> Self {
        match self {
            Held::Own(rc) => Held::Own(rc.clone()),
            Held::Config(c) => Held::Config(c.clone()),
        }
    }
}

impl<T: Send + 'static, C: ConfigCell<T>> Held<T, C> {
    /// `f` of the value.
    pub(crate) fn read<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        match self {
            Held::Own(rc) => f(&rc.lock()),
            Held::Config(c) => c.read(f),
        }
    }

    /// `f` changing the value.
    pub(crate) fn write<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        match self {
            Held::Own(rc) => f(&mut rc.lock()),
            Held::Config(c) => c.write(f),
        }
    }

    /// Whether both are the same value.
    pub(crate) fn same(&self, other: &Held<T, C>) -> bool {
        self.key_of() == other.key_of()
    }

    fn key_of(&self) -> usize {
        match self {
            Held::Own(rc) => rc.key(),
            Held::Config(c) => c.cell_key(),
        }
    }
}

impl<T: Send + 'static, C: ConfigCell<T> + std::fmt::Debug> Keyed for Held<T, C>
where
    Held<T, C>: Send + Sync,
{
    fn key(&self) -> usize {
        self.key_of()
    }
}

/// The holder of `self` (an overload's first parameter, whose type names the class), or the
/// next overload.
#[macro_export]
macro_rules! this_shared {
    ($o:expr, $a:expr, $t:ty) => {
        match $crate::shared::holder::<$t>(&$a[0], $o.params[0].ty)? {
            Some(rc) => rc,
            None => return Ok(None),
        }
    };
}

impl<T: Clone + Send + 'static, C: ConfigCell<T>> Held<T, C> {
    /// The value, as a guard: the object's own, locked; a copy of the config's, written back
    /// when the guard changed it and drops (for the rules, whose copies are exact).
    pub(crate) fn lock(&self) -> HeldGuard<'_, T, C> {
        match self {
            Held::Own(rc) => HeldGuard::Own(rc.lock()),
            Held::Config(c) => HeldGuard::Config {
                cell: c,
                value: Some(c.read(T::clone)),
                dirty: false,
            },
        }
    }
}

/// [`Held::lock`]'s guard.
pub(crate) enum HeldGuard<'a, T: Clone, C: ConfigCell<T>> {
    Own(MutexGuard<'a, T>),
    Config {
        cell: &'a C,
        value: Option<T>,
        dirty: bool,
    },
}

impl<T: Clone, C: ConfigCell<T>> std::ops::Deref for HeldGuard<'_, T, C> {
    type Target = T;
    fn deref(&self) -> &T {
        match self {
            HeldGuard::Own(g) => g,
            HeldGuard::Config { value, .. } => value.as_ref().expect("until dropped"),
        }
    }
}

impl<T: Clone, C: ConfigCell<T>> std::ops::DerefMut for HeldGuard<'_, T, C> {
    fn deref_mut(&mut self) -> &mut T {
        match self {
            HeldGuard::Own(g) => g,
            HeldGuard::Config { value, dirty, .. } => {
                *dirty = true;
                value.as_mut().expect("until dropped")
            }
        }
    }
}

impl<T: Clone, C: ConfigCell<T>> Drop for HeldGuard<'_, T, C> {
    fn drop(&mut self) {
        if let HeldGuard::Config {
            cell,
            value,
            dirty: true,
        } = self
            && let Some(v) = value.take()
        {
            cell.write(|t| *t = v);
        }
    }
}

/// The [`Held`] holder of `self` (`$t` the value's type, `$c` the config's handle), or the
/// next overload.
#[macro_export]
macro_rules! this_held {
    ($o:expr, $a:expr, $t:ty, $c:ty) => {
        match $crate::shared::holder_of::<$crate::shared::Held<$t, $c>>(&$a[0], $o.params[0].ty)? {
            Some(rc) => rc,
            None => return Ok(None),
        }
    };
}

/// The number of keys the registry holds, for the registry's tests.
pub(crate) fn registered_keys() -> usize {
    lock(&INSTANCES).key_count()
}

/// The number of objects registered, for the registry's tests.
pub(crate) fn registered_count() -> usize {
    lock(&INSTANCES).keys.len()
}

/// The Python object registered for the holder of the class `qualified` keyed `key`, while it
/// lives (pybind11's `get_object_handle`).
pub(crate) fn registered<'py>(
    py: Python<'py>,
    qualified: &'static str,
    key: usize,
) -> Option<Bound<'py, PyAny>> {
    lock(&INSTANCES).find(py, &(qualified, key))
}
