// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! pybind11's `shared_ptr` holders of the classes other than the transforms (chunk 6.3d): a
//! Python object of such a class holds a [`Shared`] value, which other objects (an iterator, a
//! set's color space) can share, so a change through one shows through the others, as through
//! upstream's `ContextRcPtr`, `ColorSpaceRcPtr` and the others; and pybind11's registered
//! instances return the Python object made for a holder while it lives ([`to_py`]).

use std::any::Any;
use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};

use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::PyCFunction;

use crate::py_transform::{Registry, lock, register_in};
use crate::pybind11;

/// A value as a `shared_ptr` holds it: cloning the handle shares it. It counts its changes
/// ([`Shared::version`]).
#[derive(Debug)]
pub(crate) struct Shared<T>(Arc<SharedCell<T>>);

#[derive(Debug)]
struct SharedCell<T> {
    value: Mutex<T>,
    version: AtomicU64,
}

impl<T> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Shared(self.0.clone())
    }
}

impl<T> Shared<T> {
    pub(crate) fn new(value: T) -> Shared<T> {
        Shared(Arc::new(SharedCell {
            value: Mutex::new(value),
            version: AtomicU64::new(0),
        }))
    }

    /// The value, locked. The binding runs under the GIL and locks two values at once only
    /// when they are different ones ([`Shared::same`]), so the lock is never contended (but
    /// by the current config's write-back for an embedding host, [`crate::embed`], which
    /// locks each value briefly).
    pub(crate) fn lock(&self) -> SharedGuard<'_, T> {
        SharedGuard {
            value: self
                .0
                .value
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            version: &self.0.version,
        }
    }

    /// The address pybind11's registry keys an instance by.
    pub(crate) fn key(&self) -> usize {
        Arc::as_ptr(&self.0) as *const u8 as usize
    }

    /// A handle that tells whether the value still lives (some handle holds it).
    pub(crate) fn downgrade(&self) -> WeakShared<T> {
        WeakShared(Arc::downgrade(&self.0))
    }

    /// Whether both are the same value.
    pub(crate) fn same(&self, other: &Shared<T>) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    /// How many times the value was changed: taken mutably ([`SharedGuard`]) other than in a
    /// call that only reads ([`changes_count`]).
    pub(crate) fn version(&self) -> u64 {
        self.0.version.load(Ordering::SeqCst)
    }
}

/// [`Shared::downgrade`]'s handle.
#[derive(Debug)]
pub(crate) struct WeakShared<T>(std::sync::Weak<SharedCell<T>>);

impl<T> Clone for WeakShared<T> {
    fn clone(&self) -> Self {
        WeakShared(self.0.clone())
    }
}

impl<T> WeakShared<T> {
    /// Whether a [`Shared`] handle of the value lives.
    pub(crate) fn alive(&self) -> bool {
        self.0.strong_count() > 0
    }
}

/// [`Shared::lock`]'s guard: taking the value mutably counts a change ([`Shared::version`]),
/// unless the binding's call only reads ([`changes_count`]).
pub(crate) struct SharedGuard<'a, T> {
    value: MutexGuard<'a, T>,
    version: &'a AtomicU64,
}

impl<T> std::ops::Deref for SharedGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T> std::ops::DerefMut for SharedGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        if changes_count() {
            self.version.fetch_add(1, Ordering::SeqCst);
        }
        &mut self.value
    }
}

thread_local! {
    /// Whether the binding's call this thread runs only reads ([`Reading`]).
    static READING: Cell<bool> = const { Cell::new(false) };
}

/// Whether a value taken mutably now counts as a change: not in a call that only reads
/// ([`reads_only`]; such a call takes values mutably to read them, as the transforms'
/// getters do, or to write back what didn't change, as a config's write-back of its objects).
pub(crate) fn changes_count() -> bool {
    !READING.with(Cell::get)
}

/// While it lives, this thread's calls only read (or not); the state before comes back when
/// it drops. The dispatcher enters it around each call of a bound function, so a Python
/// callback's own calls (a setter in a logging function) count as theirs do.
pub(crate) struct Reading(bool);

impl Reading {
    pub(crate) fn enter(reading: bool) -> Reading {
        Reading(READING.with(|r| r.replace(reading)))
    }
}

impl Drop for Reading {
    fn drop(&mut self) {
        READING.with(|r| r.set(self.0));
    }
}

/// Whether the bound function `name` only reads the objects it is given: upstream's getters
/// (`get`, `is`, `has`, and the module's `Get`, `Is`, `Has`, each followed by a capital) and
/// the protocols and methods that read (`__len__`, `__iter__`, `__next__`, `__getitem__`,
/// `__contains__`, `__repr__`, `__str__`, `__eq__`, `__ne__`, `serialize`, `validate`, and
/// `slopesAreDefault`, `const` in upstream, which takes the transform or curve mutably to read
/// it, as the getters do). Every other call counts its changes; a name guessed wrong costs a
/// needless copy of the current config for an embedding host, never a stale one, and D-12's
/// error for a pipeline's processor whose callback calls it
/// (`crate::apphelpers::py_legacy_viewing_pipeline`), but for a getter that changes what it
/// reads, which upstream has none of.
pub(crate) fn reads_only(name: &str) -> bool {
    let getter = ["get", "is", "has", "Get", "Is", "Has"].iter().any(|p| {
        name.strip_prefix(p)
            .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_uppercase()))
    });
    getter
        || matches!(
            name,
            "__len__"
                | "__iter__"
                | "__next__"
                | "__getitem__"
                | "__contains__"
                | "__repr__"
                | "__str__"
                | "__eq__"
                | "__ne__"
                | "serialize"
                | "validate"
                | "slopesAreDefault"
        )
}

/// How many times Python changed a config's context or rules through the config's own handle
/// (a [`ConfigCell`]), any config's.
static CELL_CHANGES: AtomicU64 = AtomicU64::new(0);

/// [`CELL_CHANGES`].
pub(crate) fn cell_changes() -> u64 {
    CELL_CHANGES.load(Ordering::SeqCst)
}

fn count_cell_change() {
    if changes_count() {
        CELL_CHANGES.fetch_add(1, Ordering::SeqCst);
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
    to_py_with(py, qualified, rc.key(), || rc.clone())
}

/// A returned value of the class `qualified` that pybind11 registers under the address `key`:
/// the Python object registered there while it lives, else a new instance holding `value()`
/// (an instance's own `Shared` value, or a reference into another's, chunk 6.7a).
pub(crate) fn to_py_with<'py, V: Send + Sync + 'static>(
    py: Python<'py>,
    qualified: &'static str,
    key: usize,
    value: impl FnOnce() -> V,
) -> PyResult<Bound<'py, PyAny>> {
    let key = (qualified, key);
    if let Some(obj) = lock(&INSTANCES).find(py, &key) {
        return Ok(obj);
    }
    let cls = pybind11::class(py, qualified)
        .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err(qualified))?;
    let obj = cls.call_method1("__new__", (&cls,))?;
    pybind11::set_value(
        &obj,
        qualified,
        Bound::new(py, Holder(Box::new(value())))?.into_any(),
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
    let key = rc.key();
    construct_with(this, qualified, key, rc)
}

/// A constructor's result: `value` as the value of `this`, an instance of the class
/// `qualified`, registered under the address `key`.
pub(crate) fn construct_with<V: Send + Sync + 'static>(
    this: &Bound<'_, PyAny>,
    qualified: &'static str,
    key: usize,
    value: V,
) -> PyResult<()> {
    pybind11::set_value(
        this,
        qualified,
        Bound::new(this.py(), Holder(Box::new(value)))?.into_any(),
    )?;
    register_in(&INSTANCES, &INSTANCES_CALLBACK, this, (qualified, key))
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
    holder_with::<R>(obj, qualified)
}

/// The value `obj` holds as an instance of the class `qualified` (see [`holder`]), as the type
/// `V` its binding holds.
pub(crate) fn holder_with<V: Clone + 'static>(
    obj: &Bound<'_, PyAny>,
    qualified: &str,
) -> PyResult<Option<V>> {
    let Some(this) = pybind11::instance_of(obj, qualified) else {
        return Ok(None);
    };
    let value = pybind11::read_value(&this, qualified)?;
    let holder = value.cast::<Holder>()?;
    holder
        .get()
        .0
        .downcast_ref::<V>()
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
        count_cell_change();
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
        let out = self.update(f);
        count_cell_change();
        out
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
    Own(SharedGuard<'a, T>),
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
