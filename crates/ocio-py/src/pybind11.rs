// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The pieces of pybind11 the wheel's classes are made of, as Python sees them (chunk 6.2a):
//! the metatype `pybind11_builtins.pybind11_type`, the base class
//! `pybind11_builtins.pybind11_object`, the descriptor `pybind11_builtins.pybind11_static_property`,
//! methods as `instancemethod` objects over builtin functions, and properties over them. Every
//! class and enum of the module is made here, so that its metatype, MRO, dictionary (names,
//! order, kinds, docstrings) and the reprs of its methods are the wheel's.
//!
//! `pybind11_object` is a Rust class ([`Pybind11Object`]): its `__init__` is a C slot, so a
//! class without a constructor holds it in its dictionary as the wheel's do (a
//! `wrapper_descriptor`, raising "No constructor defined!"), and an instance holds its class's
//! C++ value ([`value`], [`set_value`]; for an enum, the value as a Python `int`).
//!
//! How the types are made: [`new_class`] and [`instance_method`] are the only two places that
//! decide it. They make the classes with the metatype from Python (`type(name, bases, ns)`)
//! and the methods with a stand-in for CPython's `instancemethod` ([`InstanceMethod`]), as no
//! safe API makes either otherwise. Two differences remain from the wheel's types, pending the
//! owner's decision on a C-API module (PyType_FromMetaclass, PyInstanceMethod_New): a class's
//! C name (`tp_name`) is its short name, where pybind11's is the dotted
//! `PyOpenColorIO.PyOpenColorIO.Name`, which CPython prints in some messages ("'Name' object
//! has no attribute ..."); and the stand-in's type is a PyO3 class named `instancemethod` in
//! `builtins`.

use std::ffi::CString;

use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyCFunction, PyDict, PyTuple, PyType};

use crate::dispatch::{Function, dispatch};

/// `__module__` of the module's classes: `PyOpenColorIO.PyOpenColorIO`, the name the wheel's
/// module has while it initializes (imported as `PyOpenColorIO`'s submodule).
pub const MODULE: &str = "PyOpenColorIO.PyOpenColorIO";

/// pybind11_builtins, made once per interpreter: the metatype, the base class and the static
/// property, as Python source run in a namespace of its own.
const RUNTIME: &std::ffi::CStr = cr#"
class pybind11_type(type):
    pass

pybind11_type.__module__ = 'pybind11_builtins'


class pybind11_static_property(property):
    # pybind11_static_get: the class (the instance's class) is what the getter gets.
    def __get__(self, obj, cls=None):
        if obj is None:
            target = cls
        elif isinstance(obj, type):
            target = obj
        else:
            target = type(obj)
        return property.__get__(self, target, target)

    def __set__(self, obj, value):
        target = obj if isinstance(obj, type) else type(obj)
        property.__set__(self, target, value)


pybind11_static_property.__module__ = 'pybind11_builtins'
"#;

/// The runtime's namespace.
static RUNTIME_NS: PyOnceLock<Py<PyDict>> = PyOnceLock::new();

fn runtime(py: Python<'_>) -> PyResult<&Bound<'_, PyDict>> {
    let ns = RUNTIME_NS.get_or_try_init(py, || -> PyResult<Py<PyDict>> {
        let ns = PyDict::new(py);
        py.run(RUNTIME, Some(&ns), None)?;
        Ok(ns.unbind())
    })?;
    Ok(ns.bind(py))
}

fn runtime_item<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    runtime(py)?
        .get_item(name)?
        .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err(name.to_string()))
}

/// `pybind11_builtins.pybind11_object`, the base of every class of the module: the C++ value
/// of an instance of a bound class.
#[pyclass(
    name = "pybind11_object",
    module = "pybind11_builtins",
    subclass,
    frozen
)]
#[derive(Debug, Default)]
pub struct Pybind11Object {
    value: std::sync::Mutex<Option<Py<PyAny>>>,
}

#[pymethods]
impl Pybind11Object {
    #[new]
    #[pyo3(signature = (*_args, **_kwargs))]
    fn new(_args: &Bound<'_, PyTuple>, _kwargs: Option<&Bound<'_, PyDict>>) -> Self {
        Pybind11Object::default()
    }

    // pybind11_object_init: a class with no constructor of its own.
    #[pyo3(signature = (*_args, **_kwargs))]
    fn __init__(
        slf: &Bound<'_, Self>,
        _args: &Bound<'_, PyTuple>,
        _kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        let cls = slf.get_type();
        Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "{}.{}: No constructor defined!",
            cls.getattr("__module__")?,
            cls.getattr("__qualname__")?
        )))
    }
}

/// `pybind11_builtins.pybind11_object`'s type.
pub fn object_base(py: Python<'_>) -> PyResult<Bound<'_, PyType>> {
    Ok(py.get_type::<Pybind11Object>())
}

/// The C++ value an instance of a bound class holds, if it has one.
pub fn value<'py>(obj: &Bound<'py, PyAny>) -> Option<Bound<'py, PyAny>> {
    let this = obj.cast::<Pybind11Object>().ok()?;
    let guard = this.get().value.lock().ok()?;
    guard.as_ref().map(|v| v.bind(obj.py()).clone())
}

/// Sets the C++ value of an instance of a bound class.
pub fn set_value(obj: &Bound<'_, PyAny>, value: Bound<'_, PyAny>) -> PyResult<()> {
    let this = obj.cast::<Pybind11Object>()?;
    let mut guard = this
        .get()
        .value
        .lock()
        .map_err(|_| pyo3::exceptions::PyRuntimeError::new_err("poisoned"))?;
    *guard = Some(value.unbind());
    Ok(())
}

/// The `__init__` a class without a constructor holds in its dictionary: pybind11_object's
/// slot wrapper.
pub fn no_constructor(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    object_base(py)?.getattr("__dict__")?.get_item("__init__")
}

/// `pybind11_builtins.pybind11_static_property(fget, None, None, "")`.
pub fn static_property<'py>(
    py: Python<'py>,
    fget: Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    runtime_item(py, "pybind11_static_property")?.call1((fget, py.None(), py.None(), ""))
}

/// A class of the module, made as pybind11 makes it: named `name`, its `__qualname__`
/// `qualname` (the enclosing class's name first, for a nested class), in [`MODULE`], with the
/// metatype and `bases` (`pybind11_object` when empty), its dictionary starting with `ns` in
/// order (the names after `__module__` and `__doc__` come from the caller).
pub fn new_class<'py>(
    py: Python<'py>,
    name: &str,
    qualname: &str,
    bases: &[Bound<'py, PyType>],
    ns: &[(&str, Bound<'py, PyAny>)],
) -> PyResult<Bound<'py, PyType>> {
    let metatype = runtime_item(py, "pybind11_type")?;
    let bases: Vec<Bound<'py, PyType>> = if bases.is_empty() {
        vec![object_base(py)?]
    } else {
        bases.to_vec()
    };
    let dict = PyDict::new(py);
    for (k, v) in ns {
        dict.set_item(k, v)?;
    }
    // No instance dictionary, as pybind11's objects have none.
    dict.set_item("__slots__", PyTuple::empty(py))?;
    let cls = metatype
        .call1((name, PyTuple::new(py, bases)?, dict))?
        .cast_into::<PyType>()?;
    cls.delattr("__slots__")?;
    cls.setattr("__qualname__", qualname)?;
    Ok(cls)
}

// Stand-in for CPython's `instancemethod` (`PyInstanceMethod_New`), which pybind11 wraps each
// method's builtin function in: a descriptor binding its function to the instance, with the
// function's docstring and name. (No Rust doc comment: it would be the type's docstring, which
// shadows the function's.)
#[pyclass(name = "instancemethod", module = "builtins", frozen)]
#[derive(Debug)]
pub struct InstanceMethod {
    func: Py<PyAny>,
}

#[pymethods]
impl InstanceMethod {
    #[getter]
    fn __func__(&self, py: Python<'_>) -> Py<PyAny> {
        self.func.clone_ref(py)
    }

    #[getter]
    fn __doc__(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.func.getattr(py, "__doc__")
    }

    #[getter]
    fn __name__(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.func.getattr(py, "__name__")
    }

    fn __get__(
        &self,
        py: Python<'_>,
        obj: Option<Bound<'_, PyAny>>,
        cls: Option<Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let _ = cls;
        match obj {
            None => Ok(self.func.clone_ref(py)),
            Some(obj) if obj.is_none() => Ok(self.func.clone_ref(py)),
            Some(obj) => {
                let method_type = py.import("types")?.getattr("MethodType")?;
                Ok(method_type.call1((self.func.bind(py), obj))?.unbind())
            }
        }
    }

    #[pyo3(signature = (*args, **kwargs))]
    fn __call__(
        &self,
        py: Python<'_>,
        args: &Bound<'_, PyTuple>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Py<PyAny>> {
        Ok(self.func.bind(py).call(args, kwargs)?.unbind())
    }

    fn __repr__(slf: &Bound<'_, Self>) -> PyResult<String> {
        let name: String = slf
            .get()
            .func
            .getattr(slf.py(), "__name__")?
            .extract(slf.py())?;
        Ok(format!(
            "<instancemethod {name} at {:#018X}>",
            slf.as_ptr() as usize
        ))
    }
}

/// `f` wrapped as a method (see [`InstanceMethod`]).
pub fn instance_method<'py>(py: Python<'py>, f: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    Ok(Bound::new(py, InstanceMethod { func: f.unbind() })?.into_any())
}

/// A builtin function over `f` (`PyCFunction`, taking `*args` and `**kwargs` as pybind11's
/// functions do): its name `f.name`, its docstring pybind11's for `f`'s overloads, or `None`
/// for a function pybind11 gives none (an empty docstring here, pending the C-API module).
pub fn builtin<'py>(
    py: Python<'py>,
    f: &'static Function,
    doc: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let name: &'static std::ffi::CStr = Box::leak(
        CString::new(f.name)
            .expect("no NUL in a name")
            .into_boxed_c_str(),
    );
    let docstring: Option<&'static std::ffi::CStr> = doc.then(|| {
        &*Box::leak(
            CString::new(crate::dispatch::docstring(f))
                .expect("no NUL in a docstring")
                .into_boxed_c_str(),
        )
    });
    let function = PyCFunction::new_closure(py, Some(name), docstring, move |args, kwargs| {
        dispatch(args.py(), f, args, kwargs)
    })?;
    Ok(function.into_any())
}

/// A method: [`instance_method`] of [`builtin`].
pub fn method<'py>(py: Python<'py>, f: &'static Function) -> PyResult<Bound<'py, PyAny>> {
    instance_method(py, builtin(py, f, true)?)
}

/// `obj` when it is an instance of the class named `qualified` (`module.qualname`) or of a
/// subclass: pybind11's caster of a class parameter (and of `self`).
pub fn instance_of<'py>(obj: &Bound<'py, PyAny>, qualified: &str) -> Option<Bound<'py, PyAny>> {
    let mro = obj.get_type().mro();
    mro.iter()
        .any(|c| {
            let module: Option<String> = c.getattr("__module__").and_then(|m| m.extract()).ok();
            let qualname: Option<String> = c.getattr("__qualname__").and_then(|m| m.extract()).ok();
            matches!((module, qualname), (Some(m), Some(q)) if format!("{m}.{q}") == qualified)
        })
        .then(|| obj.clone())
}
