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
pub(crate) fn to_py<'py, T: Send + 'static>(
    py: Python<'py>,
    qualified: &'static str,
    rc: &Shared<T>,
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
pub(crate) fn out<T: Send + 'static>(
    py: Python<'_>,
    qualified: &'static str,
    rc: &Shared<T>,
) -> PyResult<Option<Py<PyAny>>> {
    Ok(Some(to_py(py, qualified, rc)?.unbind()))
}

/// A constructor's result: `rc` as the value of `this`, an instance of the class `qualified`
/// (pybind11's `py::init` of a factory returning the holder), registered.
pub(crate) fn construct<T: Send + 'static>(
    this: &Bound<'_, PyAny>,
    qualified: &'static str,
    rc: Shared<T>,
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
    let Some(this) = pybind11::instance_of(obj, qualified) else {
        return Ok(None);
    };
    let value = pybind11::read_value(&this, qualified)?;
    let holder = value.cast::<Holder>()?;
    holder
        .get()
        .0
        .downcast_ref::<Shared<T>>()
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

/// The number of keys the registry holds, for the registry's tests.
pub(crate) fn registered_keys() -> usize {
    lock(&INSTANCES).key_count()
}

/// The number of objects registered, for the registry's tests.
pub(crate) fn registered_count() -> usize {
    lock(&INSTANCES).keys.len()
}
