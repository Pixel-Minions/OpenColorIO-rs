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
use pyo3::types::{PyCFunction, PyDict, PyModule, PyString, PyTuple, PyType};

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

    # pybind11_meta_call: an instance of a class whose __init__ (a Python subclass's) didn't
    # construct the bound class it derives from is refused.
    def __call__(cls, *args, **kwargs):
        self = type.__call__(cls, *args, **kwargs)
        unconstructed = _unconstructed(self)
        if unconstructed is not None:
            raise TypeError(unconstructed + '.__init__() must be called when overriding __init__')
        return self

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
        let unconstructed = PyCFunction::new_closure(
            py,
            Some(c"_unconstructed"),
            None,
            |args, _| -> PyResult<Option<String>> {
                let obj = args.get_item(0)?;
                Ok(match bound_class_of(&obj) {
                    Some((name, _)) if value(&obj).is_none() => Some(name),
                    _ => None,
                })
            },
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

/// The size of pybind11's `instance` before its `weakrefs` member, past the object header:
/// its value-and-holder union, three pointers for OCIO's `shared_ptr` holders.
const VALUES_SIZE: usize = 24;

/// `pybind11_builtins.pybind11_object`, the base of every class of the module: the C++ value
/// of an instance of a bound class, set once, when a constructor (`__init__`, `__setstate__`)
/// constructs it. Weak references go after it, at pybind11's offset (40): the module's
/// classes have pybind11's instance size ([`INSTANCE_SIZE`]).
#[pyclass(
    name = "pybind11_object",
    module = "pybind11_builtins",
    subclass,
    frozen,
    weakref
)]
#[derive(Debug, Default)]
pub struct Pybind11Object {
    value: std::sync::OnceLock<Py<PyAny>>,
    _pad: [u8; VALUES_SIZE - std::mem::size_of::<std::sync::OnceLock<Py<PyAny>>>()],
}

/// pybind11's `sizeof(instance)`: what `__basicsize__` and `sys.getsizeof` give for an instance
/// of the module's classes.
pub const INSTANCE_SIZE: usize = 56;

#[pymethods]
impl Pybind11Object {
    #[new]
    #[classmethod]
    #[pyo3(signature = (*_args, **_kwargs))]
    fn new(
        cls: &Bound<'_, PyType>,
        _args: &Bound<'_, PyTuple>,
        _kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Self> {
        // pybind11_object itself: the wheel's process aborts (pybind11 allocates no value for
        // a type it didn't register), and so does `__reduce__()` of an instance, which copyreg
        // makes by calling it. The port refuses it (docs/improvements.md, U-87).
        if cls.is(&object_base(cls.py())?) {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "pybind11_builtins.pybind11_object: pybind11's base class has no instances \
                 (the wheel's process aborts)",
            ));
        }
        Ok(Pybind11Object::default())
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

/// The C++ value an instance of a bound class holds, if a constructor constructed it.
pub fn value<'py>(obj: &Bound<'py, PyAny>) -> Option<Bound<'py, PyAny>> {
    let this = obj.cast::<Pybind11Object>().ok()?;
    this.get().value.get().map(|v| v.bind(obj.py()).clone())
}

/// The C++ value of `obj`, an instance a method reads. One that no constructor constructed
/// (`OCIO.TransformDirection.__new__(OCIO.TransformDirection)`) holds uninitialized memory in
/// the wheel, which its methods read; the port refuses it (docs/improvements.md, U-86).
pub fn read_value<'py>(obj: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    match value(obj) {
        Some(v) => Ok(v),
        None => Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
            "{}: the instance was never constructed (no __init__ or __setstate__ gave it a \
             value; the wheel reads uninitialized memory)",
            tp_name(&obj.get_type())?
        ))),
    }
}

/// Constructs the C++ value of an instance of a bound class: once, as a constructor
/// (`__init__`, `__setstate__`) does; the dispatcher calls no constructor on an instance it
/// constructed already.
pub fn set_value(obj: &Bound<'_, PyAny>, value: Bound<'_, PyAny>) -> PyResult<()> {
    let this = obj.cast::<Pybind11Object>()?;
    let _ = this.get().value.set(value.unbind());
    Ok(())
}

/// Each class of the module, by its C name (`tp_name`), as [`new_class`] made it.
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

/// The most derived class of the module `obj` is an instance of, with its C name: pybind11's
/// type of the instance's value.
fn bound_class_of(obj: &Bound<'_, PyAny>) -> Option<(String, Py<PyType>)> {
    let ty = obj.get_type();
    let classes = CLASSES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    ty.mro().iter().find_map(|c| {
        classes
            .iter()
            .find(|(_, cls)| c.is(cls.bind(obj.py())))
            .map(|(name, cls)| (name.clone(), cls.clone_ref(obj.py())))
    })
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
