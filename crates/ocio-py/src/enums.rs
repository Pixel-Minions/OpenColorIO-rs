// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! pybind11's `enum_` as the wheel shows it (chunk 6.2a): each enum a class of the module
//! (metatype, base, dictionary in the wheel's order), its values instances of it, exported
//! into the enclosing scope (`export_values`).
//!
//! An unscoped C++ enum converts to its underlying `int`, so pybind11 binds it "convertible":
//! `__eq__` compares `int(self)` with the other operand (`TRANSFORM_DIR_INVERSE == 1` is true,
//! `== None` false), `__ne__` its negation (true for `None`), `__hash__` and `__getstate__` are
//! `int(self)`. A value's name is looked up in the class's `__entries` by equality, `???` when
//! none matches (`TransformDirection(5)`). Constructing from an `int` makes a new instance each
//! time. An arithmetic enum (`py::arithmetic`) adds the comparisons and the bitwise operators,
//! on `int(self)` and `int(other)`. The class docstring is `\n\nMembers:` and each member's
//! `\n\n  NAME : ` (the wheel's docstrings are empty).

use pyo3::BoundObject;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyInt, PyString, PyTuple, PyType};

use crate::dispatch::{Cast, Function, Overload, Param};
use crate::pybind11::{self, MODULE};

/// `self` as an instance of the class the overload's first parameter names, or the next
/// overload.
macro_rules! this {
    ($o:expr, $a:expr) => {
        match pybind11::instance_of(&$a[0], $o.params[0].ty) {
            Some(this) => this,
            None => return Ok(None),
        }
    };
}

/// `int(obj)`: Python's `int()`, which calls the enum's `__int__`.
fn int_of<'py>(obj: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    obj.py().get_type::<PyInt>().call1((obj,))
}

/// pybind11's `enum_name`: the key of the class's `__entries` whose value equals `obj`, else
/// `???`.
fn enum_name(obj: &Bound<'_, PyAny>) -> PyResult<String> {
    let entries = obj.get_type().getattr("__entries")?.cast_into::<PyDict>()?;
    for (key, entry) in entries.iter() {
        if entry.get_item(0)?.eq(obj)? {
            return Ok(key.str()?.to_string());
        }
    }
    Ok("???".to_string())
}

fn type_name(obj: &Bound<'_, PyAny>) -> PyResult<String> {
    Ok(obj.get_type().getattr("__name__")?.str()?.to_string())
}

fn ok<'py>(obj: impl IntoPyObject<'py>, py: Python<'py>) -> PyResult<Option<Py<PyAny>>> {
    Ok(Some(
        obj.into_pyobject(py)
            .map_err(Into::into)?
            .into_any()
            .unbind(),
    ))
}

/// The C++ type an enum converts to (`std::underlying_type`): the type of `value`, and of
/// `__init__`'s and `__setstate__`'s argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scalar {
    /// `int`, an unscoped enum's without a fixed type.
    Int,
    /// `unsigned int`.
    UInt,
    /// `unsigned long` (32 bits in the Windows wheel, 64 in the Linux one).
    ULong,
}

/// An enum's scalar argument, loaded with the caster of `scalar`'s C++ type; its value as a
/// Python `int`.
fn load_scalar<'py>(
    py: Python<'py>,
    scalar: Scalar,
    obj: &Bound<'py, PyAny>,
    convert: bool,
) -> Option<Bound<'py, PyAny>> {
    match scalar {
        Scalar::Int => <i32 as Cast>::load(obj, convert).map(|v| PyInt::new(py, v).into_any()),
        Scalar::UInt => <u32 as Cast>::load(obj, convert).map(|v| PyInt::new(py, v).into_any()),
        Scalar::ULong => <crate::dispatch::CULong as Cast>::load(obj, convert)
            .map(|v| PyInt::new(py, v).into_any()),
    }
}

/// The scalar type an enum's parameters name (`value`, `state`) take, read back from the
/// overload: its class's [`Scalar`] is recorded in [`SCALARS`] by qualified name.
fn scalar_of(o: &Overload) -> Scalar {
    let scalars = SCALARS.lock().expect("not poisoned");
    scalars
        .iter()
        .find(|(name, _)| *name == o.params[0].ty)
        .map_or(Scalar::Int, |(_, s)| *s)
}

/// Each enum class's [`Scalar`], by qualified name.
static SCALARS: std::sync::Mutex<Vec<(&'static str, Scalar)>> = std::sync::Mutex::new(Vec::new());

const SELF_OBJECT: Param = Param::new("self", "object");
const OTHER: Param = Param::new("other", "object");
const ARG0_OBJECT: Param = Param::new("arg0", "object");
const CONDUIT_PARAMS: &[Param] = &[
    SELF_OBJECT,
    Param::new("arg0", "bytes"),
    Param::new("arg1", "types.CapsuleType"),
    Param::new("arg2", "bytes"),
];

fn overload(
    params: &'static [Param],
    ret: &'static str,
    call: crate::dispatch::Call,
    pos_only: usize,
) -> Overload {
    Overload {
        params,
        ret,
        call,
        pos_only,
    }
}

fn function(
    name: &'static str,
    overloads: Vec<Overload>,
    method: bool,
    constructor: bool,
) -> &'static Function {
    Box::leak(Box::new(Function {
        name,
        constructor,
        method,
        overloads: Box::leak(overloads.into_boxed_slice()),
    }))
}

fn leak(params: Vec<Param>) -> &'static [Param] {
    Box::leak(params.into_boxed_slice())
}

/// pybind11's `PYBIND11_ENUM_OP_CONV`: `int(self) <op> int(other)` (`int()` converts the
/// other operand as Python does, a `str` it parses), for an arithmetic enum (`py::arithmetic`).
fn arithmetic_functions() -> Vec<&'static Function> {
    // `int(self) <op> int(other)`, through the operator of Python's `int` (`a < b` and `a & b`
    // in the C++ are PyObject_RichCompare and PyNumber_And on two `int`s); the reflected
    // operators compute the same expression, `self` on the left.
    macro_rules! conv {
        ($name:literal, $ret:literal, $op:literal) => {
            function(
                $name,
                vec![overload(
                    &[SELF_OBJECT, OTHER],
                    $ret,
                    |_, _, a, _| {
                        let me = int_of(&a[0])?;
                        let other = int_of(&a[1])?;
                        Ok(Some(me.call_method1($op, (other,))?.unbind()))
                    },
                    2,
                )],
                true,
                false,
            )
        };
    }
    vec![
        conv!("__lt__", "bool", "__lt__"),
        conv!("__gt__", "bool", "__gt__"),
        conv!("__le__", "bool", "__le__"),
        conv!("__ge__", "bool", "__ge__"),
        conv!("__and__", "object", "__and__"),
        conv!("__rand__", "object", "__and__"),
        conv!("__or__", "object", "__or__"),
        conv!("__ror__", "object", "__or__"),
        conv!("__xor__", "object", "__xor__"),
        conv!("__rxor__", "object", "__xor__"),
        // __invert__(self: object, /) -> object: `~int(self)`.
        function(
            "__invert__",
            vec![overload(
                &[SELF_OBJECT],
                "object",
                |_, _, a, _| Ok(Some(int_of(&a[0])?.call_method0("__invert__")?.unbind())),
                1,
            )],
            true,
            false,
        ),
    ]
}

/// The method `_pybind11_conduit_v1_` every class of the module has (`class_`'s and
/// `enum_`'s constructors define it).
pub fn conduit(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    // _pybind11_conduit_v1_(self: object, arg0: bytes, arg1: types.CapsuleType, arg2: bytes)
    // -> object: pybind11's cross-module conduit, which answers None to any other ABI.
    let f = function(
        "_pybind11_conduit_v1_",
        vec![overload(
            CONDUIT_PARAMS,
            "object",
            |py, _, a, _| {
                let capsule = a[2].get_type().name().is_ok_and(|n| n == "PyCapsule");
                if !a[1].is_instance_of::<PyBytes>()
                    || !capsule
                    || !a[3].is_instance_of::<PyBytes>()
                {
                    return Ok(None);
                }
                Ok(Some(py.None()))
            },
            0,
        )],
        true,
        false,
    );
    pybind11::instance_method(py, pybind11::builtin(py, f, false)?)
}

/// The methods every enum class has, whatever its C++ type (`enum_base::init`).
fn shared_functions() -> [&'static Function; 7] {
    [
        // __repr__(self: object, /) -> str: "<Name.MEMBER: value>".
        function(
            "__repr__",
            vec![overload(
                &[SELF_OBJECT],
                "str",
                |py, _, a, _| {
                    let s = format!(
                        "<{}.{}: {}>",
                        type_name(&a[0])?,
                        enum_name(&a[0])?,
                        int_of(&a[0])?
                    );
                    ok(s, py)
                },
                1,
            )],
            true,
            false,
        ),
        // name(self: object, /) -> str, the getter of the property `name`.
        function(
            "name",
            vec![overload(
                &[SELF_OBJECT],
                "str",
                |py, _, a, _| ok(enum_name(&a[0])?, py),
                1,
            )],
            true,
            false,
        ),
        // __str__(self: object, /) -> str: "Name.MEMBER".
        function(
            "__str__",
            vec![overload(
                &[SELF_OBJECT],
                "str",
                |py, _, a, _| ok(format!("{}.{}", type_name(&a[0])?, enum_name(&a[0])?), py),
                1,
            )],
            true,
            false,
        ),
        // __eq__(self: object, other: object, /) -> bool: `int(self) == other`, never for None.
        function(
            "__eq__",
            vec![overload(
                &[SELF_OBJECT, OTHER],
                "bool",
                |py, _, a, _| {
                    let me = int_of(&a[0])?;
                    ok(!a[1].is_none() && me.eq(&a[1])?, py)
                },
                2,
            )],
            true,
            false,
        ),
        // __ne__(self: object, other: object, /) -> bool: None, or `int(self) != other`.
        function(
            "__ne__",
            vec![overload(
                &[SELF_OBJECT, OTHER],
                "bool",
                |py, _, a, _| {
                    let me = int_of(&a[0])?;
                    ok(a[1].is_none() || !me.eq(&a[1])?, py)
                },
                2,
            )],
            true,
            false,
        ),
        // __getstate__(self: object, /) -> int.
        function(
            "__getstate__",
            vec![overload(
                &[SELF_OBJECT],
                "int",
                |py, _, a, _| ok(int_of(&a[0])?, py),
                1,
            )],
            true,
            false,
        ),
        // __hash__(self: object, /) -> int.
        function(
            "__hash__",
            vec![overload(
                &[SELF_OBJECT],
                "int",
                |py, _, a, _| ok(int_of(&a[0])?, py),
                1,
            )],
            true,
            false,
        ),
    ]
}

/// The class's docstring (`__doc__`'s static property): the C++ docstring (empty in the wheel),
/// two line ends, "Members:", and per member its name and ` : ` with its (empty) docstring.
fn class_doc(cls: &Bound<'_, PyAny>) -> PyResult<String> {
    let entries = cls.getattr("__entries")?.cast_into::<PyDict>()?;
    let mut doc = String::from("\n\nMembers:");
    for (key, entry) in entries.iter() {
        doc.push_str("\n\n  ");
        doc.push_str(&key.str()?.to_string());
        let comment = entry.get_item(1)?;
        if !comment.is_none() {
            doc.push_str(" : ");
            doc.push_str(&comment.str()?.to_string());
        }
    }
    Ok(doc)
}

/// The enum `name` of `scope` (the module, or an enclosing class whose name `outer` prefixes
/// the qualified name), with `entries` (name, value) in declaration order, its values exported
/// into `scope` (`export_values`). The class, as `scope` holds it.
pub fn add_enum<'py>(
    scope: &Bound<'py, PyAny>,
    outer: Option<&str>,
    def: &EnumDef,
) -> PyResult<Bound<'py, PyType>> {
    let EnumDef {
        name,
        scalar,
        arithmetic,
        entries,
    } = *def;
    let py = scope.py();
    let qualname = match outer {
        Some(outer) => format!("{outer}.{name}"),
        None => name.to_string(),
    };
    let qualified: &'static str = Box::leak(format!("{MODULE}.{qualname}").into_boxed_str());
    SCALARS
        .lock()
        .map_err(|_| pyo3::exceptions::PyRuntimeError::new_err("poisoned"))?
        .push((qualified, scalar));
    let self_typed = Param::new("self", qualified);

    // __init__(self: Name, value: typing.SupportsInt) -> None.
    let init = function(
        "__init__",
        vec![overload(
            leak(vec![self_typed, Param::new("value", "typing.SupportsInt")]),
            "None",
            |py, o, a, convert| {
                let this = this!(o, a);
                let Some(value) = load_scalar(py, scalar_of(o), &a[1], convert) else {
                    return Ok(None);
                };
                // pybind11 constructs the value once: on an instance that has one, __init__
                // changes nothing.
                if pybind11::value(&this).is_none() {
                    pybind11::set_value(&this, value)?;
                }
                Ok(Some(py.None()))
            },
            0,
        )],
        true,
        true,
    );
    // __doc__(arg0: object) -> str and __members__(arg0: object) -> dict, the getters of the
    // static properties (they get the class).
    let doc_getter = function(
        "__doc__",
        vec![overload(
            &[ARG0_OBJECT],
            "str",
            |py, _, a, _| ok(class_doc(&a[0])?, py),
            0,
        )],
        false,
        false,
    );
    let members_getter = function(
        "__members__",
        vec![overload(
            &[ARG0_OBJECT],
            "dict",
            |py, _, a, _| {
                let entries = a[0].getattr("__entries")?.cast_into::<PyDict>()?;
                let members = PyDict::new(py);
                for (key, entry) in entries.iter() {
                    members.set_item(key, entry.get_item(0)?)?;
                }
                ok(members, py)
            },
            0,
        )],
        false,
        false,
    );
    // The value: (arg0: Name) -> int, the getter of the property `value`.
    let value_getter = function(
        "",
        vec![overload(
            leak(vec![Param::new("arg0", qualified)]),
            "int",
            |py, o, a, _| {
                let this = this!(o, a);
                ok(pybind11::value(&this), py)
            },
            0,
        )],
        false,
        false,
    );
    let as_int = |name: &'static str| {
        function(
            name,
            vec![overload(
                leak(vec![self_typed]),
                "int",
                |py, o, a, _| {
                    let this = this!(o, a);
                    ok(pybind11::value(&this), py)
                },
                1,
            )],
            true,
            false,
        )
    };
    // __setstate__(self: Name, state: typing.SupportsInt, /) -> None.
    let setstate = function(
        "__setstate__",
        vec![overload(
            leak(vec![self_typed, Param::new("state", "typing.SupportsInt")]),
            "None",
            |py, o, a, convert| {
                let this = this!(o, a);
                let Some(value) = load_scalar(py, scalar_of(o), &a[1], convert) else {
                    return Ok(None);
                };
                pybind11::set_value(&this, value)?;
                Ok(Some(py.None()))
            },
            2,
        )],
        true,
        false,
    );

    let static_property = |f: &'static Function| -> PyResult<Bound<'py, PyAny>> {
        pybind11::static_property(py, pybind11::builtin(py, f, true)?)
    };
    let property = py.import("builtins")?.getattr("property")?;

    let cls = pybind11::new_class(
        py,
        name,
        &qualname,
        &[],
        &[
            ("__init__", pybind11::method(py, init)?),
            ("__doc__", static_property(doc_getter)?),
            ("__module__", PyString::new(py, MODULE).into_any()),
        ],
    )?;
    scope.setattr(name, &cls)?;
    let [repr, name_fn, str_fn, eq, ne, getstate, hash] = shared_functions();
    cls.setattr("_pybind11_conduit_v1_", conduit(py)?)?;
    cls.setattr("__entries", PyDict::new(py))?;
    cls.setattr("__repr__", pybind11::method(py, repr)?)?;
    cls.setattr("name", property.call1((pybind11::method(py, name_fn)?,))?)?;
    cls.setattr("__str__", pybind11::method(py, str_fn)?)?;
    cls.setattr("__members__", static_property(members_getter)?)?;
    cls.setattr("__eq__", pybind11::method(py, eq)?)?;
    cls.setattr("__ne__", pybind11::method(py, ne)?)?;
    if arithmetic {
        for f in arithmetic_functions() {
            cls.setattr(f.name, pybind11::method(py, f)?)?;
        }
    }
    cls.setattr("__getstate__", pybind11::method(py, getstate)?)?;
    cls.setattr("__hash__", pybind11::method(py, hash)?)?;
    cls.setattr(
        "value",
        property.call1((
            pybind11::builtin(py, value_getter, true)?,
            py.None(),
            py.None(),
            "",
        ))?,
    )?;
    cls.setattr("__int__", pybind11::method(py, as_int("__int__"))?)?;
    cls.setattr("__index__", pybind11::method(py, as_int("__index__"))?)?;
    cls.setattr("__setstate__", pybind11::method(py, setstate)?)?;

    let table = cls.getattr("__entries")?.cast_into::<PyDict>()?;
    for &(member, value) in entries {
        let instance = to_py(&cls, value)?;
        table.set_item(
            member,
            PyTuple::new(py, [instance.clone(), PyString::new(py, "").into_any()])?,
        )?;
        cls.setattr(member, &instance)?;
    }
    // export_values: each value into the enclosing scope.
    for (member, entry) in table.iter() {
        scope.setattr(member.cast::<PyString>()?, entry.get_item(0)?)?;
    }
    Ok(cls)
}

/// An enum of the module, as `py::enum_` declares it: its name, its C++ scalar type, whether
/// it is `py::arithmetic`, and its values in declaration order.
#[derive(Debug, Clone, Copy)]
pub struct EnumDef {
    pub name: &'static str,
    pub scalar: Scalar,
    pub arithmetic: bool,
    pub entries: &'static [(&'static str, i64)],
}

/// An enum argument: an instance of the enum class `qualified` (or of a subclass), its value.
/// pybind11 takes no `int` for an enum; `None` it takes with conversions, as a null pointer
/// its call then refuses (pybind11's `reference_cast_error`), so the next overload is tried.
pub fn enum_value(obj: &Bound<'_, PyAny>, qualified: &str) -> Option<i64> {
    let this = pybind11::instance_of(obj, qualified)?;
    pybind11::value(&this)?.extract().ok()
}

/// Loads argument `i` of an overload's arguments as the enum its parameter names, returning
/// `Ok(None)` (the next overload) when it isn't one.
#[macro_export]
macro_rules! enum_arg {
    ($o:expr, $a:expr, $i:expr) => {
        match $crate::enums::enum_value(&$a[$i], $o.params[$i].ty) {
            Some(v) => v,
            None => return Ok(None),
        }
    };
}

/// The enum instance of class `cls` for `value`, as pybind11 casts a returned C++ enum: a new
/// instance.
pub fn to_py<'py>(cls: &Bound<'py, PyType>, value: i64) -> PyResult<Bound<'py, PyAny>> {
    let instance = cls.call_method1("__new__", (cls,))?;
    pybind11::set_value(&instance, PyInt::new(cls.py(), value).into_any())?;
    Ok(instance)
}
