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
//! `wrapper_descriptor`, raising "No constructor defined!"), and an instance holds one C++ value
//! per class of the module among its type's bases, as pybind11's values and holders
//! ([`all_type_info`], [`value`], [`set_value`]; for an enum, the value as a Python `int`), so
//! that a Python class may derive from several. Its layout and metatype: docs/deviations.md,
//! D-7.
//!
//! How the types are made: [`new_class`] and [`instance_method`] are the only two places that
//! decide it, through the C API ([`crate::type_object`]): a class is made by
//! `PyType_FromMetaclass` with pybind11's metatype, its C name (`tp_name`) the dotted
//! `PyOpenColorIO.PyOpenColorIO.Name` that CPython writes in some messages, and its `tp_init`
//! `pybind11_object`'s; a method is CPython's `instancemethod`.

use std::ffi::CString;

use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyCFunction, PyDict, PyModule, PyString, PyTuple, PyType, PyWeakrefReference};

use crate::dispatch::{Function, dispatch};

/// `__module__` of the module's classes: `PyOpenColorIO.PyOpenColorIO`, the name the wheel's
/// module has while it initializes (imported as `PyOpenColorIO`'s submodule).
pub const MODULE: &str = "PyOpenColorIO.PyOpenColorIO";

/// pybind11_builtins, made once per interpreter: the metatype and the static property, made by
/// `type(name, bases, namespace)` in the wheel's dictionary order (a class statement would add
/// `__static_attributes__` and `__firstlineno__`), in a namespace without `__name__` so that
/// `type()` sets no `__module__` until Rust sets it last.
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


# pybind11_meta_call: an instance of a class whose __init__ (a Python subclass's) didn't
# construct each bound class it derives from is refused.
def __call__(cls, *args, **kwargs):
    self = type.__call__(cls, *args, **kwargs)
    unconstructed = _unconstructed(self)
    if unconstructed is not None:
        raise TypeError(unconstructed + '.__init__() must be called when overriding __init__')
    return self


# pybind11_meta_getattro: a method (an instancemethod) is returned as it is, not bound.
def __getattribute__(cls, name):
    descr = _lookup(cls, name)
    if descr is not None and type(descr) is _instancemethod:
        return descr
    return type.__getattribute__(cls, name)


# pybind11_meta_setattro: a static property's setter, unless the value is itself one; a
# deletion is type's (the slot's other wrapper, __delattr__).
def __setattr__(cls, name, value):
    descr = _lookup(cls, name)
    if (descr is not None and isinstance(descr, pybind11_static_property)
            and not isinstance(value, pybind11_static_property)):
        type(descr).__set__(descr, cls, value)
        return
    type.__setattr__(cls, name, value)


def __delattr__(cls, name):
    type.__delattr__(cls, name)


pybind11_type = type('pybind11_type', (type,), {
    '__call__': __call__, '__getattribute__': __getattribute__, '__setattr__': __setattr__,
    '__delattr__': __delattr__, '__doc__': None})


# pybind11_static_get: the class (the instance's class) is what the getter gets.
def __get__(self, obj, cls=None):
    if obj is None:
        target = cls
    elif isinstance(obj, type):
        target = obj
    else:
        target = type(obj)
    return property.__get__(self, target, target)


# pybind11_static_set (both its wrappers, __set__ and __delete__): property's, on the class.
def __set__(self, obj, value):
    property.__set__(self, obj if isinstance(obj, type) else type(obj), value)


def __delete__(self, obj):
    property.__delete__(self, obj if isinstance(obj, type) else type(obj))


# A __dict__ (property subclasses have one) and no weak references, as pybind11's.
pybind11_static_property = type('pybind11_static_property', (property,), {
    '__get__': __get__, '__set__': __set__, '__delete__': __delete__,
    '__slots__': ('__dict__',)})
del pybind11_static_property.__slots__
for _f, _owner in ((__call__, 'pybind11_type'), (__getattribute__, 'pybind11_type'),
                   (__setattr__, 'pybind11_type'), (__delattr__, 'pybind11_type'),
                   (__get__, 'pybind11_static_property'), (__set__, 'pybind11_static_property'),
                   (__delete__, 'pybind11_static_property')):
    _f.__qualname__ = _owner + '.' + _f.__name__
    _f.__module__ = 'pybind11_builtins'
del __call__, __getattribute__, __setattr__, __delattr__, __get__, __set__, __delete__, _f, _owner
"#;

/// The runtime's namespace.
static RUNTIME_NS: PyOnceLock<Py<PyDict>> = PyOnceLock::new();

fn runtime(py: Python<'_>) -> PyResult<&Bound<'_, PyDict>> {
    let ns = RUNTIME_NS.get_or_try_init(py, || -> PyResult<Py<PyDict>> {
        let ns = PyDict::new(py);
        py.run(RUNTIME, Some(&ns), None)?;
        for t in ["pybind11_type", "pybind11_static_property"] {
            if let Some(t) = ns.get_item(t)? {
                t.setattr("__module__", "pybind11_builtins")?;
            }
        }
        let len = py.import("builtins")?.getattr("len")?;
        let instancemethod = crate::type_object::instance_method(&len)?.get_type();
        ns.set_item("_instancemethod", instancemethod)?;
        let unconstructed = PyCFunction::new_closure(
            py,
            Some(c"_unconstructed"),
            None,
            |args, _| -> PyResult<Option<String>> { unconstructed(&args.get_item(0)?) },
        )?;
        ns.set_item("_unconstructed", unconstructed)?;
        Ok(ns.unbind())
    })?;
    Ok(ns.bind(py))
}

fn runtime_item<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    runtime(py)?
        .get_item(name)?
        .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err(name.to_string()))
}

/// The values an instance holds: one per class of the module among its type's bases
/// ([`all_type_info`], in that order), each set once, when a constructor of that class
/// (`__init__`, `__setstate__`) constructs it (pybind11's values and holders).
type Values = std::sync::Mutex<Vec<Option<Py<PyAny>>>>;

// `pybind11_builtins.pybind11_object`, the base of every class of the module, with pybind11's
// instance size (`sizeof(instance)`, 56: [`INSTANCE_SIZE`]) so that a Python class may derive
// from several classes of the module, as from the wheel's (CPython refuses bases of different
// sizes). PyO3 places weak references last (`__weakrefoffset__` 48, pybind11's 40), names the
// type `pybind11_builtins.pybind11_object` (pybind11's C name is `pybind11_object`) and makes
// it with the metatype `type` (pybind11's `pybind11_type`): docs/deviations.md, D-7 (owner
// decision 2026-10-21). (No Rust doc comment: it would be the class's docstring, which the
// wheel's hasn't.)
#[pyclass(
    name = "pybind11_object",
    module = "pybind11_builtins",
    subclass,
    frozen,
    weakref
)]
#[derive(Debug, Default)]
pub struct Pybind11Object {
    values: Values,
    _pad: [u8; VALUES_SIZE - std::mem::size_of::<Values>()],
}

/// The size of the instance's contents before PyO3's weak reference slot: pybind11's 24-byte
/// union of values and holders, its weak reference pointer and its flags.
const VALUES_SIZE: usize = 32;

/// pybind11's `sizeof(instance)`: what `__basicsize__` and `sys.getsizeof` give for an instance
/// of the module's classes, and of `pybind11_object`.
pub const INSTANCE_SIZE: usize = 56;

#[pymethods]
impl Pybind11Object {
    // pybind11_object_new (make_new_instance, instance::allocate_layout): one value per class
    // of the module among the type's bases, none constructed.
    #[new]
    #[classmethod]
    #[pyo3(signature = (*_args, **_kwargs), text_signature = None)]
    fn new(
        cls: &Bound<'_, PyType>,
        _args: &Bound<'_, PyTuple>,
        _kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Self> {
        let n = all_type_info(cls)?.len();
        // A type without one (pybind11_object itself, a Python class deriving from it only):
        // pybind11 fails ("instance allocation failed: new instance has no
        // pybind11-registered base types") where no C++ exception may leave, and the wheel's
        // process aborts. So does `__reduce__()` of an enum value, which copyreg makes by
        // calling `pybind11_object`. The port refuses it (docs/improvements.md, U-87).
        if n == 0 {
            return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                "{}: pybind11 makes no instance of a class without a class of the module among \
                 its bases (the wheel's process aborts)",
                tp_name(cls)?
            )));
        }
        Ok(Pybind11Object {
            values: std::sync::Mutex::new((0..n).map(|_| None).collect()),
            _pad: [0; VALUES_SIZE - std::mem::size_of::<Values>()],
        })
    }

    // pybind11_object_init: a class with no constructor of its own.
    #[pyo3(signature = (*_args, **_kwargs))]
    fn __init__(
        slf: &Bound<'_, Self>,
        _args: &Bound<'_, PyTuple>,
        _kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "{}: No constructor defined!",
            tp_name(&slf.get_type())?
        )))
    }
}

/// `pybind11_builtins.pybind11_object`'s type.
pub fn object_base(py: Python<'_>) -> PyResult<Bound<'_, PyType>> {
    Ok(py.get_type::<Pybind11Object>())
}

/// Each class of the module, by its C name (`tp_name`), as [`new_class`] made it: pybind11's
/// registered types.
static CLASSES: std::sync::Mutex<Vec<(String, Py<PyType>)>> = std::sync::Mutex::new(Vec::new());

/// The class of the module whose C name is `qualified` (`MODULE.qualname`).
pub fn class<'py>(py: Python<'py>, qualified: &str) -> Option<Bound<'py, PyType>> {
    let classes = CLASSES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    classes
        .iter()
        .rev()
        .find(|(name, _)| name == qualified)
        .map(|(_, cls)| cls.bind(py).clone())
}

/// Whether `ty` is a class of the module.
fn registered(ty: &Bound<'_, PyType>) -> bool {
    CLASSES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .any(|(_, cls)| ty.is(cls.bind(ty.py())))
}

/// pybind11's cache of [`all_type_info`] for the other types it was asked about (Python
/// classes deriving from the module's), each entry held by a weak reference to its type whose
/// callback removes it, as pybind11's `all_type_info_get_cache` holds it.
static TYPE_INFO_CACHE: std::sync::Mutex<Vec<CachedTypeInfo>> = std::sync::Mutex::new(Vec::new());

/// An entry of [`TYPE_INFO_CACHE`]: the weak reference to the type, its classes.
type CachedTypeInfo = (Py<PyWeakrefReference>, Vec<Py<PyType>>);

/// The cached classes of `ty`, a type [`all_type_info`] was asked about before.
fn cached_type_info<'py>(ty: &Bound<'py, PyType>) -> Option<Vec<Bound<'py, PyType>>> {
    let py = ty.py();
    let cache = TYPE_INFO_CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    cache
        .iter()
        .find(|(wr, _)| wr.bind(py).upgrade().is_some_and(|t| t.is(ty)))
        .map(|(_, infos)| infos.iter().map(|c| c.bind(py).clone()).collect())
}

/// pybind11's `all_type_info(type)`: the classes of the module whose values an instance of
/// `ty` holds. A class of the module: itself. Another type: cached the first time it is asked
/// (an instance's allocation), from its bases (`__bases__`) in breadth-first order (pybind11's
/// `all_type_info_populate`), each a class of the module (itself) or a type asked about
/// before (its cached classes), else its own bases in turn; each class once.
pub fn all_type_info<'py>(ty: &Bound<'py, PyType>) -> PyResult<Vec<Bound<'py, PyType>>> {
    let py = ty.py();
    if registered(ty) {
        return Ok(vec![ty.clone()]);
    }
    if let Some(infos) = cached_type_info(ty) {
        return Ok(infos);
    }
    let mut check: Vec<Bound<'py, PyAny>> = ty.bases().iter().collect();
    let mut infos: Vec<Bound<'py, PyType>> = Vec::new();
    let mut i = 0;
    while i < check.len() {
        if let Ok(t) = check[i].cast::<PyType>() {
            let known = if registered(t) {
                Some(vec![t.clone()])
            } else {
                cached_type_info(t)
            };
            match known {
                Some(known) => {
                    for k in known {
                        if !infos.iter().any(|c| c.is(&k)) {
                            infos.push(k);
                        }
                    }
                }
                None => check.extend(t.bases().iter()),
            }
        }
        i += 1;
    }
    let remove = PyCFunction::new_closure(py, None, None, |args, _| -> PyResult<()> {
        let wr = args.get_item(0)?;
        TYPE_INFO_CACHE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .retain(|(w, _)| !w.bind(args.py()).is(&wr));
        Ok(())
    })?;
    let wr = PyWeakrefReference::new_with(ty, remove)?;
    TYPE_INFO_CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push((
            wr.unbind(),
            infos.iter().map(|c| c.clone().unbind()).collect(),
        ));
    Ok(infos)
}

/// An instance and the classes of the module its values belong to.
type InstanceValues<'a, 'py> = (&'a Bound<'py, Pybind11Object>, Vec<Bound<'py, PyType>>);

/// `obj`'s values and the classes they belong to, when it is an instance of the module's
/// classes.
fn values_of<'a, 'py>(obj: &'a Bound<'py, PyAny>) -> PyResult<Option<InstanceValues<'a, 'py>>> {
    let Ok(this) = obj.cast::<Pybind11Object>() else {
        return Ok(None);
    };
    Ok(Some((this, all_type_info(&obj.get_type())?)))
}

/// The index of the value `obj` holds as an instance of `cls`, as pybind11's class caster
/// finds it (`type_caster_generic::load_impl`): `obj`'s type itself, or a type with one class
/// of the module, its value; with several, the first that is `cls` or derives from it.
fn caster_index(obj: &Bound<'_, PyAny>, cls: &Bound<'_, PyType>) -> PyResult<Option<usize>> {
    let ty = obj.get_type();
    if ty.is(cls) {
        return Ok(Some(0));
    }
    if !ty.is_subclass(cls)? {
        return Ok(None);
    }
    let infos = all_type_info(&ty)?;
    if infos.len() == 1 {
        return Ok(Some(0));
    }
    for (i, info) in infos.iter().enumerate() {
        if info.is_subclass(cls)? {
            return Ok(Some(i));
        }
    }
    Ok(None)
}

/// The C++ value `obj` holds as an instance of the class of the module whose C name is
/// `qualified`, if a constructor constructed it.
pub fn value<'py>(obj: &Bound<'py, PyAny>, qualified: &str) -> Option<Bound<'py, PyAny>> {
    let cls = class(obj.py(), qualified)?;
    let index = caster_index(obj, &cls).ok()??;
    let this = obj.cast::<Pybind11Object>().ok()?;
    let values = this
        .get()
        .values
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    values
        .get(index)?
        .as_ref()
        .map(|v| v.bind(obj.py()).clone())
}

/// [`value`] of `obj`, an instance a method reads. One that no constructor constructed
/// (`OCIO.TransformDirection.__new__(OCIO.TransformDirection)`) holds uninitialized memory in
/// the wheel, which its methods read; the port refuses it (docs/improvements.md, U-86).
pub fn read_value<'py>(obj: &Bound<'py, PyAny>, qualified: &str) -> PyResult<Bound<'py, PyAny>> {
    match value(obj, qualified) {
        Some(v) => Ok(v),
        None => Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
            "{qualified}: the instance was never constructed (no __init__ or __setstate__ gave \
             it a value; the wheel reads uninitialized memory)"
        ))),
    }
}

/// The index of the value a constructor of `cls` constructs in `obj`, as pybind11's
/// `instance::get_value_and_holder(tinfo, true)` finds it: `obj`'s type itself, its first;
/// else the one of `cls` exactly; else pybind11 fails (`pybind11_fail`, RuntimeError).
fn constructor_index(obj: &Bound<'_, PyAny>, cls: &Bound<'_, PyType>) -> PyResult<usize> {
    let ty = obj.get_type();
    if ty.is(cls) {
        return Ok(0);
    }
    all_type_info(&ty)?
        .iter()
        .position(|info| info.is(cls))
        .ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err(
                "pybind11::detail::instance::get_value_and_holder: type is not a pybind11 base \
                 of the given instance (#define PYBIND11_DETAILED_ERROR_MESSAGES or compile in \
                 debug mode for type details)",
            )
        })
}

/// Whether a constructor of the class of the module whose C name is `qualified` constructed
/// its value in `obj` already (the dispatcher then calls none: pybind11's
/// `instance_registered()`).
pub fn constructed(obj: &Bound<'_, PyAny>, qualified: &str) -> PyResult<bool> {
    let Some(cls) = class(obj.py(), qualified) else {
        return Ok(false);
    };
    let index = constructor_index(obj, &cls)?;
    let this = obj.cast::<Pybind11Object>()?;
    let values = this
        .get()
        .values
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    Ok(values.get(index).is_some_and(Option::is_some))
}

/// Constructs the C++ value of `obj` as an instance of the class of the module whose C name is
/// `qualified`: once, as a constructor (`__init__`, `__setstate__`) does; the dispatcher calls
/// no constructor on a value constructed already.
pub fn set_value(obj: &Bound<'_, PyAny>, qualified: &str, value: Bound<'_, PyAny>) -> PyResult<()> {
    let cls = class(obj.py(), qualified)
        .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err(qualified.to_string()))?;
    let index = constructor_index(obj, &cls)?;
    let this = obj.cast::<Pybind11Object>()?;
    let mut values = this
        .get()
        .values
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(slot) = values.get_mut(index)
        && slot.is_none()
    {
        *slot = Some(value.unbind());
    }
    Ok(())
}

/// pybind11_meta_call's check, after a class's `__call__` made `obj`: the C name of the first
/// class of the module whose value no constructor constructed (a Python subclass's `__init__`
/// that didn't call its `__init__`), skipping one a later class derives from and an earlier
/// one already holds (`is_redundant_value_and_holder`).
fn unconstructed(obj: &Bound<'_, PyAny>) -> PyResult<Option<String>> {
    let Some((this, infos)) = values_of(obj)? else {
        return Ok(None);
    };
    let constructed: Vec<bool> = this
        .get()
        .values
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .map(Option::is_some)
        .collect();
    for (i, info) in infos.iter().enumerate() {
        if constructed.get(i).copied().unwrap_or(true) {
            continue;
        }
        let mut redundant = false;
        for earlier in &infos[..i] {
            if earlier.is_subclass(info)? {
                redundant = true;
            }
        }
        if !redundant {
            return Ok(Some(tp_name(info)?));
        }
    }
    Ok(None)
}

/// A type's C name, as CPython's messages write it: a class of the module's, its dotted
/// `MODULE.qualname`; another (a Python class), its name.
pub fn tp_name(ty: &Bound<'_, PyType>) -> PyResult<String> {
    let classes = CLASSES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((name, _)) = classes.iter().find(|(_, cls)| ty.is(cls.bind(ty.py()))) {
        return Ok(name.clone());
    }
    drop(classes);
    // pybind11_object: PyO3's C name (docs/deviations.md, D-7).
    if ty.is(&object_base(ty.py())?) {
        return Ok("pybind11_builtins.pybind11_object".to_string());
    }
    Ok(ty.name()?.to_string())
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
    let tp_name = format!("{MODULE}.{qualname}");
    let cls = crate::type_object::from_metaclass(
        &metatype,
        &tp_name,
        &PyTuple::new(py, bases)?,
        object_init_slot(),
        INSTANCE_SIZE,
    )?;
    CLASSES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push((tp_name, cls.clone().unbind()));
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

// pybind11's function record (`detail::function_record`), the `__self__` of every function it
// makes, as Python sees it: a class of pybind11_builtins whose name carries pybind11's ABI tag,
// which differs between the Windows wheel (MSVC) and the Linux one (GCC, libstdc++); CPython
// names it in a function's `__qualname__` and repr. Its dictionary holds `__new__`, `__init__` and
// `__reduce_ex__`, as the wheel's; its method `dispatch_call` is the C function of every
// function, removed from the dictionary. (No Rust doc comment: it would be the class's
// docstring.)
#[cfg_attr(
    windows,
    pyclass(
        name = "pybind11_detail_function_record_v1_msvc_md_mscver19",
        module = "pybind11_builtins",
        frozen
    )
)]
#[cfg_attr(
    not(windows),
    pyclass(
        name = "pybind11_detail_function_record_v1_system_libstdcpp_gxx_abi_1xxx_use_cxx11_abi_1",
        module = "pybind11_builtins",
        frozen
    )
)]
#[derive(Debug)]
pub struct FunctionRecord {
    // One pointer, as the wheel's record object holds (its `__basicsize__` 24).
    record: &'static Record,
}

/// What a [`FunctionRecord`] holds: the function, and pybind11's `rec->scope` when it is a
/// module (a function `m.def` defines), which makes the record pickleable.
#[derive(Debug)]
struct Record {
    f: &'static Function,
    module_scope: Option<Py<PyModule>>,
}

#[pymethods]
impl FunctionRecord {
    // The record type's `tp_new` and `tp_init`, which the wheel's has (its dictionary holds
    // `__new__` and `__init__`): constructing or initializing a record from Python aborts the
    // wheel's process, so the port refuses both (docs/improvements.md, U-98).
    #[new]
    #[pyo3(signature = (*_args, **_kwargs), text_signature = None)]
    fn new(_args: &Bound<'_, PyTuple>, _kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Self> {
        Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "{RECORD_NAME}: pybind11's function records are made by pybind11 only (the wheel's \
             process aborts; docs/improvements.md, U-98)"
        )))
    }

    #[pyo3(signature = (*_args, **_kwargs))]
    fn __init__(
        _slf: &Bound<'_, Self>,
        _args: &Bound<'_, PyTuple>,
        _kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "{RECORD_NAME}: pybind11's function records are initialized by pybind11 only (the \
             wheel's process aborts; docs/improvements.md, U-98)"
        )))
    }

    #[pyo3(signature = (*args, **kwargs))]
    fn dispatch_call(
        slf: &Bound<'_, Self>,
        args: &Bound<'_, PyTuple>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Py<PyAny>> {
        dispatch(slf.py(), slf.get().record.f, args, kwargs)
    }

    // pybind11's `function_record_PyTypeObject_methods::reduce_ex_impl` (pybind11 3,
    // pybind11/detail/function_record_pyobject.h), its arguments ignored: a named function
    // whose scope is a module pickles as `eval("__import__('importlib').import_module('<the
    // scope's __module__, else its __name__>')")`, read when called; any other record raises
    // RuntimeError "<repr> is not pickleable.". `eval` is the builtins module's (pybind11 takes
    // the running frame's builtins, the same dictionary unless code runs with builtins of its
    // own). A Python error on the way (a name that isn't a str, `eval` deleted) leaves the
    // wheel's C function as a C++ exception and aborts its process: the port's error (U-98).
    #[pyo3(signature = (*_args, **_kwargs), text_signature = None)]
    fn __reduce_ex__(
        slf: &Bound<'_, Self>,
        _args: &Bound<'_, PyTuple>,
        _kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let record = slf.get().record;
        let reduced = || -> PyResult<Option<Py<PyAny>>> {
            let Some(scope) = record.module_scope.as_ref().map(|m| m.bind(py)) else {
                return Ok(None);
            };
            if record.f.name.is_empty() {
                return Ok(None);
            }
            let module = if scope.hasattr("__module__").unwrap_or(false) {
                scope.getattr("__module__")?
            } else if scope.hasattr("__name__").unwrap_or(false) {
                scope.getattr("__name__")?
            } else {
                return Ok(None);
            };
            let source = PyString::new(py, "__import__('importlib').import_module('")
                .add(module)?
                .add("')")?;
            let eval = py.import("builtins")?.dict().as_any().get_item("eval")?;
            Ok(Some(
                PyTuple::new(py, [eval, PyTuple::new(py, [source])?.into_any()])?
                    .into_any()
                    .unbind(),
            ))
        };
        match reduced() {
            Ok(Some(reduced)) => Ok(reduced),
            Ok(None) => Err(pyo3::exceptions::PyRuntimeError::new_err(
                slf.repr()?.add(" is not pickleable.")?.unbind(),
            )),
            Err(e) => {
                let error = pyo3::exceptions::PyTypeError::new_err(format!(
                    "{RECORD_NAME}: pickling a function record raised an error, which aborts the \
                     wheel's process (docs/improvements.md, U-98)"
                ));
                error.set_cause(py, Some(e));
                Err(error)
            }
        }
    }
}

/// The record type's C name.
#[cfg(windows)]
const RECORD_NAME: &str = "pybind11_builtins.pybind11_detail_function_record_v1_msvc_md_mscver19";
/// The record type's C name.
#[cfg(not(windows))]
const RECORD_NAME: &str = "pybind11_builtins.pybind11_detail_function_record_v1_system_libstdcpp_gxx_abi_1xxx_use_cxx11_abi_1";

/// `dispatch_call`'s method definition (PyO3's trampoline and flags), once its class no longer
/// shows it: the index, among the class's method definitions, of the one whose function is
/// named `dispatch_call` (a function made from each, with its own name, tells).
static RECORD_METHOD: PyOnceLock<usize> = PyOnceLock::new();

/// A function without overloads, for the records [`record_method`] makes to tell the method
/// definitions apart.
static NO_FUNCTION: Function = Function {
    name: "",
    constructor: false,
    method: false,
    overloads: &[],
};

fn record_methods() -> Vec<pyo3::ffi::PyMethodDef> {
    use pyo3::impl_::pyclass::PyClassImpl;
    use pyo3::impl_::pymethods::PyMethodDefType;
    <FunctionRecord as PyClassImpl>::items_iter()
        .flat_map(|items| items.methods)
        .filter_map(|m| match m {
            PyMethodDefType::Method(def) => Some(def.into_raw()),
            _ => None,
        })
        .collect()
}

fn record_method(py: Python<'_>) -> PyResult<pyo3::ffi::PyMethodDef> {
    let index = *RECORD_METHOD.get_or_try_init(py, || -> PyResult<usize> {
        py.get_type::<FunctionRecord>().delattr("dispatch_call")?;
        let record = Bound::new(
            py,
            FunctionRecord {
                record: Box::leak(Box::new(Record {
                    f: &NO_FUNCTION,
                    module_scope: None,
                })),
            },
        )?;
        for (i, def) in record_methods().into_iter().enumerate() {
            let function = crate::type_object::new_function(
                Box::leak(Box::new(def)),
                record.as_any(),
                &py.None().into_bound(py),
            )?;
            if function.getattr("__name__")?.to_string() == "dispatch_call" {
                return Ok(i);
            }
        }
        Err(pyo3::exceptions::PyRuntimeError::new_err(
            "no dispatch_call",
        ))
    })?;
    record_methods()
        .into_iter()
        .nth(index)
        .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("no dispatch_call"))
}

/// A builtin function over `f`, as pybind11 makes it (taking `*args` and `**kwargs`): its name
/// `f.name`, its docstring pybind11's for `f`'s overloads, or `None` for a function pybind11
/// gives none (`doc` false), its `__self__` a function record of `f`, its `__module__`
/// [`MODULE`] (a function with a scope: the module's, a class's method).
pub fn builtin<'py>(
    py: Python<'py>,
    f: &'static Function,
    doc: bool,
) -> PyResult<Bound<'py, PyAny>> {
    function(py, f, doc, Some(MODULE), None)
}

/// [`builtin`] for a function pybind11 makes without a scope (the getters of an enum's
/// `value`, `__doc__` and `__members__`): its `__module__` None.
pub fn builtin_unscoped<'py>(
    py: Python<'py>,
    f: &'static Function,
    doc: bool,
) -> PyResult<Bound<'py, PyAny>> {
    function(py, f, doc, None, None)
}

fn function<'py>(
    py: Python<'py>,
    f: &'static Function,
    doc: bool,
    module: Option<&str>,
    module_scope: Option<&Bound<'py, PyModule>>,
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
    let mut def = record_method(py)?;
    def.ml_name = name.as_ptr();
    def.ml_doc = docstring.map_or(std::ptr::null(), std::ffi::CStr::as_ptr);
    let record = Bound::new(
        py,
        FunctionRecord {
            record: Box::leak(Box::new(Record {
                f,
                module_scope: module_scope.map(|m| m.clone().unbind()),
            })),
        },
    )?;
    let module = match module {
        Some(m) => PyString::new(py, m).into_any(),
        None => py.None().into_bound(py),
    };
    crate::type_object::new_function(Box::leak(Box::new(def)), record.as_any(), &module)
}

/// `m.def(name, ...)`: [`builtin`] of `f` into the module's dictionary, its record's scope the
/// module (which makes it pickleable).
pub fn def_function(m: &Bound<'_, PyModule>, f: &'static Function) -> PyResult<()> {
    m.setattr(f.name, function(m.py(), f, true, Some(MODULE), Some(m))?)
}

/// A method: [`instance_method`] of [`builtin`].
pub fn method<'py>(py: Python<'py>, f: &'static Function) -> PyResult<Bound<'py, PyAny>> {
    instance_method(py, builtin(py, f, true)?)
}

/// `obj` when it is an instance of the class of the module whose C name is `qualified` (or of a
/// subclass): pybind11's caster of a class parameter (and of `self`), which compares types, not
/// names (a class's `__module__` and `__qualname__` can be changed).
pub fn instance_of<'py>(obj: &Bound<'py, PyAny>, qualified: &str) -> Option<Bound<'py, PyAny>> {
    let cls = class(obj.py(), qualified)?;
    obj.get_type().is_subclass(&cls).ok()?.then(|| obj.clone())
}
