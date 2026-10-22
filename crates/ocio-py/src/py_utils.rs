// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! What `src/bindings/python/PyUtils.h` @ v2.5.2 gives every binding, ported so far: the
//! iterators' `PyIterator` (PyUtils.h:40-61).

use std::any::Any;
use std::sync::Mutex;

use pyo3::prelude::*;

use crate::pybind11;

/// The C++ value of an iterator class's instance: `PyIterator<T, UNIQUE, ARGS...>`, its object
/// (`m_obj`, a holder or null) and its next index (`m_i`).
#[pyclass(frozen, module = "pybind11_builtins", name = "_py_iterator")]
pub(crate) struct PyIterator {
    /// `m_obj`: what the iterator walks, as its class's binding holds it.
    obj: Option<Box<dyn Any + Send + Sync>>,
    /// `m_i`.
    index: Mutex<i32>,
}

impl PyIterator {
    /// `m_obj`, as the type `T` its class holds.
    pub(crate) fn obj<T: 'static>(&self) -> PyResult<&T> {
        self.obj
            .as_ref()
            .and_then(|o| o.downcast_ref::<T>())
            .ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err("an iterator of another class")
            })
    }

    /// Port of `PyIterator::nextIndex` (PyUtils.h:44-48 @ v2.5.2): the next index, or
    /// `StopIteration` (`py::stop_iteration()`, whose message is empty) from `num` on.
    pub(crate) fn next_index(&self, num: i32) -> PyResult<i32> {
        let mut i = self
            .index
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if *i >= num {
            return Err(pyo3::exceptions::PyStopIteration::new_err(""));
        }
        let at = *i;
        *i += 1;
        Ok(at)
    }
}

/// Port of `PyIterator::checkIndex` (PyUtils.h:50-53 @ v2.5.2): `IndexError` "Iterator index
/// out of range" for an index from `num` on (a negative index passes).
pub(crate) fn check_index(i: i32, num: i32) -> PyResult<()> {
    if i >= num {
        return Err(pyo3::exceptions::PyIndexError::new_err(
            "Iterator index out of range",
        ));
    }
    Ok(())
}

/// A new instance of the iterator class that signatures write `qualified`, over `obj` (`None`: a
/// null object), from its first index: a returned `PyIterator` (pybind11 moves it into a new
/// instance).
pub(crate) fn new_iterator<'py>(
    py: Python<'py>,
    qualified: &str,
    obj: Option<Box<dyn Any + Send + Sync>>,
) -> PyResult<Bound<'py, PyAny>> {
    let cls = pybind11::class(py, qualified)
        .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err(qualified.to_string()))?;
    let instance = cls.call_method1("__new__", (&cls,))?;
    let value = PyIterator {
        obj,
        index: Mutex::new(0),
    };
    pybind11::set_value(&instance, qualified, Bound::new(py, value)?.into_any())?;
    Ok(instance)
}

/// The iterator value `obj` holds as an instance of the iterator class `qualified` (`Ok(None)`:
/// not one, the next overload).
pub(crate) fn iterator<'py>(
    obj: &Bound<'py, PyAny>,
    qualified: &str,
) -> PyResult<Option<Bound<'py, PyIterator>>> {
    let Some(this) = pybind11::instance_of(obj, qualified) else {
        return Ok(None);
    };
    let value = pybind11::read_value(&this, qualified)?;
    Ok(Some(value.cast_into::<PyIterator>()?))
}

/// The iterator of `self` (an overload's first parameter, whose type names its class), or the
/// next overload.
#[macro_export]
macro_rules! this_iter {
    ($o:expr, $a:expr) => {
        match $crate::py_utils::iterator(&$a[0], $o.params[0].ty)? {
            Some(it) => it,
            None => return Ok(None),
        }
    };
}
