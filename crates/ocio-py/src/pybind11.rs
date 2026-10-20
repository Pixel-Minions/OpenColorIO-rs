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
//! decide it, through the C API ([`crate::type_object`]): a class is made by
//! `PyType_FromMetaclass` with pybind11's metatype, its C name (`tp_name`) the dotted
//! `PyOpenColorIO.PyOpenColorIO.Name` that CPython writes in some messages, and its `tp_init`
//! `pybind11_object`'s; a method is CPython's `instancemethod`.

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
# CPython's instancemethod type, set from Rust once the runtime runs.
_instancemethod = None


def _lookup(cls, name):
    # _PyType_Lookup: the attribute in the dictionaries of the class's MRO, or None.
    for klass in type.__getattribute__(cls, '__mro__'):
        namespace = type.__getattribute__(klass, '__dict__')
        if name in namespace:
            return namespace[name]
    return None


class pybind11_type(type):
    # pybind11_meta_getattro: a method (an instancemethod) is returned as it is, not bound.
    def __getattribute__(cls, name):
        descr = _lookup(cls, name)
        if descr is not None and type(descr) is _instancemethod:
            return descr
        return type.__getattribute__(cls, name)

    # pybind11_meta_setattro: a static property's setter, unless the value is itself one.
    def __setattr__(cls, name, value):
        descr = _lookup(cls, name)
        if (descr is not None and isinstance(descr, pybind11_static_property)
                and not isinstance(value, pybind11_static_property)):
            type(descr).__set__(descr, cls, value)
            return
        type.__setattr__(cls, name, value)

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
        let len = py.import("builtins")?.getattr("len")?;
        let instancemethod = crate::type_object::instance_method(&len)?.get_type();
        ns.set_item("_instancemethod", instancemethod)?;
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

/// `pybind11_builtins.pybind11_static_property(fget, None, None, "")`.
pub fn static_property<'py>(
    py: Python<'py>,
    fget: Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    runtime_item(py, "pybind11_static_property")?.call1((fget, py.None(), py.None(), ""))
}

/// A class of the module, made as pybind11 makes it (`make_new_python_type`): named `name`,
/// its `__qualname__` `qualname` (the enclosing class's name first, for a nested class) and
/// its C name `MODULE.qualname`, with the metatype and `bases` (`pybind11_object` when empty).
/// Its dictionary starts with `__init__` (pybind11_object's slot wrapper, raising "No
/// constructor defined!"), `__doc__` and `__module__`; `ns` then sets those of them a class
/// gives otherwise (an enum's constructor, its docstring property), in place.
pub fn new_class<'py>(
    py: Python<'py>,
    name: &str,
    qualname: &str,
    bases: &[Bound<'py, PyType>],
    ns: &[(&str, Bound<'py, PyAny>)],
) -> PyResult<Bound<'py, PyType>> {
    let metatype = runtime_item(py, "pybind11_type")?.cast_into::<PyType>()?;
    let bases: Vec<Bound<'py, PyType>> = if bases.is_empty() {
        vec![object_base(py)?]
    } else {
        bases.to_vec()
    };
    let cls = crate::type_object::from_metaclass(
        &metatype,
        &format!("{MODULE}.{qualname}"),
        &PyTuple::new(py, bases)?,
        object_init_slot(),
    )?;
    debug_assert_eq!(cls.name()?.to_string(), name);
    for (k, v) in ns {
        cls.setattr(*k, v)?;
    }
    cls.setattr("__module__", MODULE)?;
    cls.setattr("__qualname__", qualname)?;
    Ok(cls)
}

/// `pybind11_object`'s `tp_init` (PyO3's slot for [`Pybind11Object`]'s `__init__`), which
/// pybind11 gives every class (`pybind11_object_init`): it raises "No constructor defined!"
/// until the class's `__init__` is set.
fn object_init_slot() -> *mut std::ffi::c_void {
    use pyo3::impl_::pyclass::PyClassImpl;
    <Pybind11Object as PyClassImpl>::items_iter()
        .flat_map(|items| items.slots)
        .find(|slot| slot.slot == pyo3::ffi::Py_tp_init)
        .map_or(std::ptr::null_mut(), |slot| slot.pfunc)
}

/// `f` wrapped as a method, as pybind11 wraps each: CPython's `instancemethod`
/// (`PyInstanceMethod_New`).
pub fn instance_method<'py>(py: Python<'py>, f: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    let _ = py;
    crate::type_object::instance_method(&f)
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
