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
//! decide it, through the C API (`ocio_py_ffi`): a class is made by
//! `PyType_FromMetaclass` with pybind11's metatype, its C name (`tp_name`) the dotted
//! `PyOpenColorIO.PyOpenColorIO.Name` that CPython writes in some messages, and its `tp_init`
//! `pybind11_object`'s; a method is CPython's `instancemethod`.

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

# What CPython's C code checks with, which no metatype overrides: a type's MRO (tp_mro) and
# dictionary (tp_dict), and _PyObject_RealIsSubclass(sub, cls) (`_real_subclass(cls, sub)`).
_mro_of = type.__dict__['__mro__'].__get__
_dict_of = type.__dict__['__dict__'].__get__
_real_subclass = type.__dict__['__subclasscheck__']
_missing = object()


def _lookup(cls, name):
    # _PyType_Lookup: the attribute in the dictionaries of the class's MRO, or None; an error
    # hashing or comparing the name is cleared (no attribute).
    try:
        for klass in _mro_of(cls):
            descr = _dict_of(klass).get(name, _missing)
            if descr is not _missing:
                return descr
    except BaseException:
        pass
    return None


# The C name (tp_name) of an object's type, as CPython writes it in messages ('%.100s'): read
# from the refusal of a slot wrapper of bool, from which no class derives.
_bool_repr = bool.__dict__['__repr__']
_BOOL_REFUSAL = "descriptor '__repr__' requires a 'bool' object but received a '"


def _type_name(obj):
    if type(obj) is bool:
        return 'bool'
    try:
        _bool_repr(obj)
    except TypeError as e:
        return str(e)[len(_BOOL_REFUSAL):-1]


# The wheel's methods of pybind11_type and pybind11_static_property are the slot wrappers of
# their C slots, which check their arguments as CPython 3.13's wrapperdescr_call
# (Objects/descrobject.c) and wrap_* functions (Objects/typeobject.c) do, in this order: a
# `self`, an instance of the wrapper's class by its type's MRO; no keyword arguments but
# `__call__`'s; then the slot's arguments.
def _slot_self(name, owner, args):
    if not args:
        raise TypeError("descriptor '%s' of '%s' object needs an argument"
                        % (name, owner.__name__))
    if not _real_subclass(owner, type(args[0])):
        raise TypeError("descriptor '%s' requires a '%s' object but received a '%s'"
                        % (name, owner.__name__, _type_name(args[0])))
    return args[0]


def _no_keywords(name, kwargs):
    if kwargs:
        raise TypeError('wrapper %s() takes no keyword arguments' % name)


def _num_args(args, n):
    # check_num_args
    if len(args) != n:
        raise TypeError('expected %d argument%s, got %d' % (n, '' if n == 1 else 's', len(args)))


def _unpack(args, low, high):
    # PyArg_UnpackTuple(args, "", low, high, ...): its name, empty, starts the message.
    if len(args) < low:
        raise TypeError(' expected %s%d argument%s, got %d'
                        % ('' if low == high else 'at least ', low, '' if low == 1 else 's',
                           len(args)))
    if len(args) > high:
        raise TypeError(' expected %s%d argument%s, got %d'
                        % ('' if low == high else 'at most ', high, '' if high == 1 else 's',
                           len(args)))


# pybind11_meta_call: an instance of a class whose __init__ (a Python subclass's) didn't
# construct each bound class it derives from is refused.
def __call__(*args, **kwargs):
    cls = _slot_self('__call__', pybind11_type, args)
    self = type.__call__(cls, *args[1:], **kwargs)
    unconstructed = _unconstructed(self)
    if unconstructed is not None:
        raise TypeError(unconstructed + '.__init__() must be called when overriding __init__')
    return self


# pybind11_meta_getattro: a method (an instancemethod) is returned as it is, not bound.
def __getattribute__(*args, **kwargs):
    cls = _slot_self('__getattribute__', pybind11_type, args)
    _no_keywords('__getattribute__', kwargs)
    _num_args(args[1:], 1)
    name = args[1]
    descr = _lookup(cls, name)
    if descr is not None and type(descr) is _instancemethod:
        return descr
    return type.__getattribute__(cls, name)


# pybind11_meta_setattro: a static property's setter, unless the value is itself one; else
# type's. Replacing an enum's `value` property frees its getter, which corrupts the wheel's
# heap: the port's error (docs/improvements.md, U-100).
def __setattr__(*args, **kwargs):
    cls = _slot_self('__setattr__', pybind11_type, args)
    _no_keywords('__setattr__', kwargs)
    _unpack(args[1:], 2, 2)
    name, value = args[1], args[2]
    descr = _lookup(cls, name)
    if (descr is not None and isinstance(descr, pybind11_static_property)
            and not isinstance(value, pybind11_static_property)):
        type(descr).__set__(descr, cls, value)
        return
    _frees_value_getter(cls, name, value)
    type.__setattr__(cls, name, value)


# The same slot, deleting: type's (U-100 as above).
def __delattr__(*args, **kwargs):
    cls = _slot_self('__delattr__', pybind11_type, args)
    _no_keywords('__delattr__', kwargs)
    _num_args(args[1:], 1)
    name = args[1]
    _lookup(cls, name)
    _frees_value_getter(cls, name, _missing)
    type.__delattr__(cls, name)


pybind11_type = type('pybind11_type', (type,), {
    '__call__': __call__, '__getattribute__': __getattribute__, '__setattr__': __setattr__,
    '__delattr__': __delattr__, '__doc__': None})


# pybind11_static_get: property's getter of the owner (the third argument, a class), whatever
# the instance; without an owner, the property itself.
def __get__(*args, **kwargs):
    self = _slot_self('__get__', pybind11_static_property, args)
    _no_keywords('__get__', kwargs)
    _unpack(args[1:], 1, 2)
    obj = args[1]
    cls = args[2] if len(args) > 2 else None
    if obj is None and cls is None:
        raise TypeError('__get__(None, None) is invalid')
    if cls is None:
        return self
    return property.__get__(self, cls, cls)


# pybind11_static_set (both its wrappers, __set__ and __delete__): property's, on the class (the
# instance itself when it is a type, by its type's MRO, else its type).
def __set__(*args, **kwargs):
    self = _slot_self('__set__', pybind11_static_property, args)
    _no_keywords('__set__', kwargs)
    _unpack(args[1:], 2, 2)
    obj, value = args[1], args[2]
    property.__set__(self, obj if _real_subclass(type, type(obj)) else type(obj), value)


def __delete__(*args, **kwargs):
    self = _slot_self('__delete__', pybind11_static_property, args)
    _no_keywords('__delete__', kwargs)
    _num_args(args[1:], 1)
    obj = args[1]
    property.__delete__(self, obj if _real_subclass(type, type(obj)) else type(obj))


# A __dict__ (property subclasses have one) and no weak references, as pybind11's.
pybind11_static_property = type('pybind11_static_property', (property,), {
    '__get__': __get__, '__set__': __set__, '__delete__': __delete__,
    '__slots__': ('__dict__',)})
del pybind11_static_property.__slots__
# The slot wrappers' names, docstrings, signatures and classes.
for _f, _owner, _doc, _signature in (
        (__call__, pybind11_type, 'Call self as a function.', '($self, /, *args, **kwargs)'),
        (__getattribute__, pybind11_type, 'Return getattr(self, name).', '($self, name, /)'),
        (__setattr__, pybind11_type, 'Implement setattr(self, name, value).',
         '($self, name, value, /)'),
        (__delattr__, pybind11_type, 'Implement delattr(self, name).', '($self, name, /)'),
        (__get__, pybind11_static_property,
         'Return an attribute of instance, which is of type owner.',
         '($self, instance, owner=None, /)'),
        (__set__, pybind11_static_property, 'Set an attribute of instance to value.',
         '($self, instance, value, /)'),
        (__delete__, pybind11_static_property, 'Delete an attribute of instance.',
         '($self, instance, /)')):
    _f.__qualname__ = _owner.__name__ + '.' + _f.__name__
    _f.__module__ = 'pybind11_builtins'
    _f.__doc__ = _doc
    _f.__text_signature__ = _signature
    _f.__objclass__ = _owner
del __call__, __getattribute__, __setattr__, __delattr__, __get__, __set__, __delete__
del _f, _owner, _doc, _signature
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
        let instancemethod = ocio_py_ffi::instance_method(&len)?.get_type();
        ns.set_item("_instancemethod", instancemethod)?;
        let unconstructed = PyCFunction::new_closure(
            py,
            Some(c"_unconstructed"),
            None,
            |args, _| -> PyResult<Option<String>> { unconstructed(&args.get_item(0)?) },
        )?;
        ns.set_item("_unconstructed", unconstructed)?;
        let frees_value_getter = PyCFunction::new_closure(
            py,
            Some(c"_frees_value_getter"),
            None,
            |args, _| -> PyResult<()> {
                frees_value_getter(&args.get_item(0)?, &args.get_item(1)?, &args.get_item(2)?)
            },
        )?;
        ns.set_item("_frees_value_getter", frees_value_getter)?;
        Ok(ns.unbind())
    })?;
    Ok(ns.bind(py))
}

/// The `value` properties of the module's enums: replacing or deleting one in its class's
/// dictionary frees its getter, which corrupts the wheel's heap (its process dies with
/// 0xC0000374 on Windows, at once, or when another reference to the property goes).
static VALUE_PROPERTIES: std::sync::Mutex<Vec<Py<PyAny>>> = std::sync::Mutex::new(Vec::new());

/// Registers `property`, an enum's `value` property, for [`frees_value_getter`].
pub fn guard_value_property(property: &Bound<'_, PyAny>) {
    VALUE_PROPERTIES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(property.clone().unbind());
}

/// The port's error, before pybind11's metatype replaces (`value`: the new value) or deletes
/// (`value`: none of the properties) the attribute `name` of the class `cls`, when that is
/// `value` and the class's own dictionary holds one of [`VALUE_PROPERTIES`] there, which the
/// wheel's heap doesn't survive (docs/improvements.md, U-100). `name` as type's `__setattr__`
/// takes it: a str's characters, whatever its class.
fn frees_value_getter(
    cls: &Bound<'_, PyAny>,
    name: &Bound<'_, PyAny>,
    value: &Bound<'_, PyAny>,
) -> PyResult<()> {
    let py = cls.py();
    let is_value = name
        .cast::<PyString>()
        .is_ok_and(|n| n.to_str().is_ok_and(|n| n == "value"));
    if !is_value {
        return Ok(());
    }
    let dict = py
        .get_type::<PyType>()
        .getattr("__dict__")?
        .get_item("__dict__")?
        .call_method1("__get__", (cls,))?;
    let Some(current) = dict.call_method1("get", ("value",)).ok() else {
        return Ok(());
    };
    let guarded = VALUE_PROPERTIES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .any(|p| p.bind(py).is(&current));
    if !guarded || value.is(&current) {
        return Ok(());
    }
    Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
        "{}: replacing or deleting an enum's `value` property frees its getter, which corrupts \
         the wheel's heap (docs/improvements.md, U-100)",
        cls.cast::<PyType>()?.fully_qualified_name()?
    )))
}

/// `_PyType_Lookup(ty, name)`: the attribute in the dictionaries of the type's MRO, if any (a
/// `None` there reads as none).
pub fn type_lookup<'py>(
    ty: &Bound<'py, PyType>,
    name: &str,
) -> PyResult<Option<Bound<'py, PyAny>>> {
    let found = runtime_item(ty.py(), "_lookup")?.call1((ty, name))?;
    Ok((!found.is_none()).then_some(found))
}

/// The C name (`tp_name`) of `obj`'s type, as CPython writes it in messages (`%.200s`).
pub fn c_type_name(obj: &Bound<'_, PyAny>) -> PyResult<String> {
    runtime_item(obj.py(), "_type_name")?
        .call1((obj,))?
        .extract()
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
// it with the metatype `type` (pybind11's `pybind11_type`); the methods of the port's
// `pybind11_type` and static property are Python functions that check their arguments as the
// wheel's slot wrappers do (the runtime above): docs/deviations.md, D-7 (owner
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

/// A class of the module as [`new_class`] made it: the name its signatures write
/// (`MODULE.qualname`), its C name (`tp_name`), the class.
type Registered = (String, String, Py<PyType>);

/// Each class of the module: pybind11's registered types.
static CLASSES: std::sync::Mutex<Vec<Registered>> = std::sync::Mutex::new(Vec::new());

/// The class of the module whose signatures write it `qualified` (`MODULE.qualname`).
pub fn class<'py>(py: Python<'py>, qualified: &str) -> Option<Bound<'py, PyType>> {
    let classes = CLASSES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    classes
        .iter()
        .rev()
        .find(|(name, _, _)| name == qualified)
        .map(|(_, _, cls)| cls.bind(py).clone())
}

/// The name the signatures write for `ty`, a class of the module (`MODULE.qualname`), by which
/// [`class`] finds it.
pub fn qualified_name(ty: &Bound<'_, PyType>) -> PyResult<String> {
    CLASSES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .find(|(_, _, cls)| ty.is(cls.bind(ty.py())))
        .map(|(name, _, _)| name.clone())
        .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("not a class of the module"))
}

/// Whether `ty` is a class of the module.
fn registered(ty: &Bound<'_, PyType>) -> bool {
    CLASSES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .any(|(_, _, cls)| ty.is(cls.bind(ty.py())))
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

/// The C++ value `obj` holds as an instance of the class of the module that signatures write
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

/// Whether a constructor of the class of the module that signatures write `qualified`
/// constructed its value in `obj` already (the dispatcher then calls none: pybind11's
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

/// Constructs the C++ value of `obj` as an instance of the class of the module that signatures
/// write `qualified`: once, as a constructor (`__init__`, `__setstate__`) does; the dispatcher
/// calls no constructor on a value constructed already.
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
/// `MODULE.name`, the class's own name also for a class nested in another (pybind11's
/// `make_new_python_type` joins the module and `rec.name`); another (a Python class), its name.
pub fn tp_name(ty: &Bound<'_, PyType>) -> PyResult<String> {
    let classes = CLASSES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((_, name, _)) = classes.iter().find(|(_, _, cls)| ty.is(cls.bind(ty.py()))) {
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
/// its C name `MODULE.name`, with the metatype and `bases` (`pybind11_object` when empty).
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
    let tp_name = format!("{MODULE}.{name}");
    let cls = ocio_py_ffi::from_metaclass::<Pybind11Object>(
        &metatype,
        &tp_name,
        &PyTuple::new(py, bases)?,
        INSTANCE_SIZE,
    )?;
    CLASSES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push((
            format!("{MODULE}.{qualname}"),
            tp_name,
            cls.clone().unbind(),
        ));
    debug_assert_eq!(cls.name()?.to_string(), name);
    for (k, v) in ns {
        cls.setattr(*k, v)?;
    }
    cls.setattr("__module__", MODULE)?;
    cls.setattr("__qualname__", qualname)?;
    Ok(cls)
}

/// A class pybind11 makes without a scope (`class_<T>(handle(), name, module_local())`, the
/// state of `make_iterator`): its C name and `__qualname__` `name`, no `__module__` in its
/// dictionary (Python reads `pybind11_object`'s, `pybind11_builtins`, and prints the class as
/// `<class 'name'>`), and signatures write it `pybind11_builtins.name`. `PyType_FromMetaclass`
/// warns of a C name without a module (a DeprecationWarning the wheel's code never raises), so
/// the metatype makes it, called from a frame whose globals have no `__name__` (which `type()`
/// would make its `__module__`); its dictionary starts with `__init__` (`pybind11_object`'s
/// slot wrapper: the wheel's is the class's own) and `__doc__` None.
pub fn new_scopeless_class<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyType>> {
    let base = object_base(py)?;
    let ns = PyDict::new(py);
    ns.set_item("__init__", base.getattr("__dict__")?.get_item("__init__")?)?;
    ns.set_item("__doc__", py.None())?;
    ns.set_item("__slots__", PyTuple::empty(py))?;
    let globals = PyDict::new(py);
    globals.set_item("meta", runtime_item(py, "pybind11_type")?)?;
    globals.set_item("args", (name, (base,), ns))?;
    py.run(c"cls = meta(*args)", Some(&globals), None)?;
    let cls = globals
        .get_item("cls")?
        .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err(name.to_string()))?
        .cast_into::<PyType>()?;
    cls.delattr("__slots__")?;
    CLASSES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push((
            format!("pybind11_builtins.{name}"),
            name.to_string(),
            cls.clone().unbind(),
        ));
    Ok(cls)
}

/// `f` wrapped as a method, as pybind11 wraps each: CPython's `instancemethod`
/// (`PyInstanceMethod_New`).
pub fn instance_method<'py>(py: Python<'py>, f: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    let _ = py;
    ocio_py_ffi::instance_method(&f)
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

/// The record type once its dictionary no longer shows `dispatch_call`, the method whose C
/// function every function is ([`function`]).
static RECORD_TYPE_READY: PyOnceLock<()> = PyOnceLock::new();

/// A builtin function over `f`, as pybind11 makes it (taking `*args` and `**kwargs`): its name
/// `f.name`, its docstring pybind11's for `f`'s overloads, or `None` for a function pybind11
/// gives none (`doc` false), its `__self__` a function record of `f`, its `__module__`
/// [`MODULE`] (a function with a scope: the module's, a class's method).
pub fn builtin<'py>(
    py: Python<'py>,
    f: &'static Function,
    doc: bool,
) -> PyResult<Bound<'py, PyAny>> {
    function(py, f, Doc::from(doc), Some(MODULE), None)
}

/// [`builtin`] for a function pybind11 makes without a scope (the getters of an enum's
/// `value`, `__doc__` and `__members__`): its `__module__` None.
pub fn builtin_unscoped<'py>(
    py: Python<'py>,
    f: &'static Function,
    doc: bool,
) -> PyResult<Bound<'py, PyAny>> {
    function(py, f, Doc::from(doc), None, None)
}

fn function<'py>(
    py: Python<'py>,
    f: &'static Function,
    doc: Doc<'_>,
    module: Option<&str>,
    module_scope: Option<&Bound<'py, PyModule>>,
) -> PyResult<Bound<'py, PyAny>> {
    RECORD_TYPE_READY.get_or_try_init(py, || {
        py.get_type::<FunctionRecord>().delattr("dispatch_call")
    })?;
    let docstring = match doc {
        Doc::None => None,
        Doc::Signatures => Some(crate::dispatch::docstring(f)),
        Doc::Text(text) => Some(format!("{}\n{text}\n", crate::dispatch::docstring(f))),
        Doc::Texts(texts) => Some(crate::dispatch::docstring_with(f, texts)),
        Doc::Overloads(docs) => Some(crate::dispatch::docstring_with_docs(f, docs)),
    };
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
    ocio_py_ffi::new_function(
        &record,
        "dispatch_call",
        f.name,
        docstring.as_deref(),
        &module,
    )
}

/// `m.def(name, ...)`: [`builtin`] of `f` into the module's dictionary, its record's scope the
/// module (which makes it pickleable).
pub fn def_function(m: &Bound<'_, PyModule>, f: &'static Function) -> PyResult<()> {
    m.setattr(
        f.name,
        function(m.py(), f, Doc::Signatures, Some(MODULE), Some(m))?,
    )
}

/// A method: [`instance_method`] of [`builtin`].
pub fn method<'py>(py: Python<'py>, f: &'static Function) -> PyResult<Bound<'py, PyAny>> {
    instance_method(py, builtin(py, f, true)?)
}

/// `obj` when it is an instance of the class of the module that signatures write `qualified` (or
/// of a subclass): pybind11's caster of a class parameter (and of `self`), which compares types,
/// not names (a class's `__module__` and `__qualname__` can be changed).
pub fn instance_of<'py>(obj: &Bound<'py, PyAny>, qualified: &str) -> Option<Bound<'py, PyAny>> {
    let cls = class(obj.py(), qualified)?;
    obj.get_type().is_subclass(&cls).ok()?.then(|| obj.clone())
}

/// A function's docstring as pybind11 writes it.
#[derive(Debug, Clone, Copy)]
enum Doc<'a> {
    /// None (`__doc__` None).
    None,
    /// The signatures ([`crate::dispatch::docstring`]): the wheel's docstrings are empty.
    Signatures,
    /// The signatures, then a line end, the definition's own docstring and a line end (a
    /// docstring the binding gives in place of `DOC(...)`, `PyLogCameraTransform.cpp:156-166`).
    Text(&'a str),
    /// Each overload's signature, then its own docstring, as [`crate::dispatch::docstring_with`]
    /// writes them (`PyCPUProcessor.cpp:47-249`).
    Texts(&'a [&'a str]),
    /// Each overload's own docstring, or none ([`crate::dispatch::docstring_with_docs`]): the
    /// definitions of pybind11's own bindings (`bind_vector`).
    Overloads(&'a [Option<&'a str>]),
}

impl From<bool> for Doc<'_> {
    fn from(signatures: bool) -> Self {
        if signatures {
            Doc::Signatures
        } else {
            Doc::None
        }
    }
}

/// A method whose overloads' definitions give their own docstrings `docs`: [`method`], with each
/// after its signature ([`Doc::Texts`]).
pub fn method_with_docs<'py>(
    py: Python<'py>,
    f: &'static Function,
    docs: &[&str],
) -> PyResult<Bound<'py, PyAny>> {
    instance_method(py, function(py, f, Doc::Texts(docs), Some(MODULE), None)?)
}

/// A method whose definition gives its own docstring `doc`: [`method`], with `doc` after the
/// signature ([`Doc::Text`]).
pub fn method_with_doc<'py>(
    py: Python<'py>,
    f: &'static Function,
    doc: &str,
) -> PyResult<Bound<'py, PyAny>> {
    instance_method(py, function(py, f, Doc::Text(doc), Some(MODULE), None)?)
}

/// A method whose overloads' definitions give docstrings of their own (`docs`, one per
/// overload): [`method`], with [`Doc::Overloads`].
pub fn method_with_overload_docs<'py>(
    py: Python<'py>,
    f: &'static Function,
    docs: &[Option<&str>],
) -> PyResult<Bound<'py, PyAny>> {
    instance_method(
        py,
        function(py, f, Doc::Overloads(docs), Some(MODULE), None)?,
    )
}
