// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! pybind11's function calls as the wheel shows them (chunk 6.4a): its dispatcher, which tries
//! a function's overloads in definition order, and the type casters that turn Python arguments
//! into the C++ parameters' types. Every binding goes through [`dispatch`], so the bindings
//! accept, refuse and describe arguments as the wheel's do: the same overload wins, the same
//! values convert, the same `TypeError` texts and docstrings.
//!
//! The wheel is built with pybind11 3 (its docstrings name `typing.SupportsInt` and
//! `collections.abc.Sequence`; its function records are `pybind11_detail_function_record_v1`):
//! what follows is that version's `cpp_function::dispatcher` and casters (`cast.h`, `stl.h`),
//! as probing the wheel confirms them. Nothing of pybind11 is translated line by line (P6-7):
//! the rules are its observable behaviour.
//!
//! - **Binding the arguments.** Positional arguments fill the parameters in order; a keyword
//!   argument fills the parameter of its name not filled positionally; a parameter left over
//!   takes its default. An overload whose parameters can't be filled that way (too many
//!   positional arguments, an unknown keyword, a keyword naming a parameter given positionally,
//!   a missing argument without a default) is skipped. An overload bound without argument
//!   names (neither `py::arg` nor `py::pos_only`: its signature names its parameters `self`,
//!   `arg0`, `arg1`, ...; [`unnamed`]) has no keywords: pybind11 keeps no argument records for
//!   it, so any keyword argument, `self=` included, skips it, and its arguments are all
//!   positional.
//! - **Two passes.** A function with one overload is called once, its casters allowed to
//!   convert. With several, each is first tried without conversions, in definition order; then
//!   those whose arguments bound (and that have a positional parameter besides `self`) again,
//!   with conversions.
//! - **The casters** ([`Cast`]): see each implementation.
//! - **No overload accepts the arguments**: for an operator, `NotImplemented`; else `TypeError`, "`name`(): incompatible function
//!   arguments. The following argument types are supported:" with each overload's signature
//!   and "Invoked with:" the arguments' reprs (for a constructor, "__init__(): incompatible
//!   constructor arguments." and each signature as `Class(params)`).
//! - **Docstrings** ([`docstring`]): `name(params) -> ret` and a line end; for several
//!   overloads, `name(*args, **kwargs)`, "Overloaded function." and each numbered.

use ocio_py_ffi::NumberSlot;
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{
    PyBool, PyByteArray, PyBytes, PyDict, PyFloat, PyFrozenSet, PyInt, PySet, PyString, PyTuple,
    PyType,
};

/// A parameter of an overload, as pybind11's signature shows it.
#[derive(Debug, Clone, Copy)]
pub struct Param {
    /// Its keyword name (`"name"_a`); `self` for a method's first.
    pub name: &'static str,
    /// Its type as the signature prints it (`str`, `typing.SupportsInt`,
    /// `PyOpenColorIO.PyOpenColorIO.Config`, ...).
    pub ty: &'static str,
    /// Its default value, if it has one.
    pub default: Option<Default>,
}

/// A parameter's default value: as the signature prints it (its repr), and how to make it.
#[derive(Debug, Clone, Copy)]
pub struct Default {
    /// The value's repr in the signature (`'x'`, `[0.0, 0.0, 0.0, 0.0]`,
    /// `<TransformDirection.TRANSFORM_DIR_FORWARD: 0>`).
    pub repr: &'static str,
    /// The value.
    pub value: for<'py> fn(Python<'py>) -> PyResult<Bound<'py, PyAny>>,
}

impl Param {
    /// A parameter without a default.
    pub const fn new(name: &'static str, ty: &'static str) -> Param {
        Param {
            name,
            ty,
            default: None,
        }
    }
}

/// The C++ function behind an overload: called with the overload (its parameters' types name
/// the classes its class casters take), the bound arguments (one per parameter, defaults
/// filled in) and whether casters may convert. `Ok(None)` when an argument doesn't cast
/// (pybind11's `PYBIND11_TRY_NEXT_OVERLOAD`): the dispatcher tries the next overload.
pub type Call =
    for<'py> fn(Python<'py>, &Overload, &[Bound<'py, PyAny>], bool) -> PyResult<Option<Py<PyAny>>>;

/// One overload: its parameters, its return type as the signature prints it, its function.
#[derive(Debug, Clone, Copy)]
pub struct Overload {
    pub params: &'static [Param],
    pub ret: &'static str,
    pub call: Call,
    /// How many leading parameters are positional-only (`py::pos_only()`): no keyword fills
    /// them, and the signature marks them with `/`.
    pub pos_only: usize,
}

/// A bound function (a `def`): its name and overloads.
#[derive(Debug)]
pub struct Function {
    /// The name the docstring and errors print.
    pub name: &'static str,
    /// A constructor (`__init__`): errors print each signature as `Class(params)`, without
    /// `self`, and "Invoked with" leaves `self` out.
    pub constructor: bool,
    /// A method: its first parameter is `self`, which the second pass doesn't count.
    pub method: bool,
    /// An operator (`py::is_operator()`): a call no overload accepts returns `NotImplemented`,
    /// whatever its arguments, so that Python tries the other operand's method.
    pub operator: bool,
    pub overloads: &'static [Overload],
}

/// An overload's signature as pybind11 writes it: `(a: int, b: str = 'x') -> ret`.
pub fn signature(o: &Overload) -> String {
    let mut params: Vec<String> = o
        .params
        .iter()
        .map(|p| match p.default {
            Some(d) => format!("{}: {} = {}", p.name, p.ty, d.repr),
            None => format!("{}: {}", p.name, p.ty),
        })
        .collect();
    if o.pos_only > 0 {
        params.insert(o.pos_only, "/".to_string());
    }
    format!("({}) -> {}", params.join(", "), o.ret)
}

/// The function's `__doc__`, as pybind11 writes it when the wheel's docstrings are empty.
pub fn docstring(f: &Function) -> String {
    match f.overloads {
        [one] => format!("{}{}\n", f.name, signature(one)),
        many => {
            let each: Vec<String> = many
                .iter()
                .enumerate()
                .map(|(i, o)| format!("{}. {}{}\n", i + 1, f.name, signature(o)))
                .collect();
            format!(
                "{}(*args, **kwargs)\nOverloaded function.\n\n{}",
                f.name,
                each.join("\n")
            )
        }
    }
}

/// The function's `__doc__` when its definitions give docstrings of their own (`docs`, one per
/// overload, each `R"doc(...)doc"` as written): pybind11 writes each overload's signature line
/// (numbered for several), then a line end, the docstring and a line end.
pub fn docstring_with(f: &Function, docs: &[&str]) -> String {
    match (f.overloads, docs) {
        ([one], [doc]) => format!("{}{}\n\n{doc}\n", f.name, signature(one)),
        (many, docs) => {
            let each: Vec<String> = many
                .iter()
                .enumerate()
                .map(|(i, o)| {
                    let doc = docs.get(i).map_or(String::new(), |d| format!("\n{d}\n"));
                    format!("{}. {}{}\n{doc}", i + 1, f.name, signature(o))
                })
                .collect();
            format!(
                "{}(*args, **kwargs)\nOverloaded function.\n\n{}",
                f.name,
                each.join("\n")
            )
        }
    }
}

/// The function's `__doc__` when its definitions give docstrings of their own (`docs`, one per
/// overload, `None` for a definition without one), as pybind11 writes it: a single overload's
/// signature line, a blank line and its docstring; each of several overloads' numbered
/// signature line, followed by a blank line and its docstring when it has one.
pub fn docstring_with_docs(f: &Function, docs: &[Option<&str>]) -> String {
    debug_assert_eq!(docs.len(), f.overloads.len());
    match (f.overloads, docs) {
        ([one], [Some(doc)]) => format!("{}{}\n\n{doc}\n", f.name, signature(one)),
        ([_], [None]) => docstring(f),
        (many, _) => {
            let each: Vec<String> = many
                .iter()
                .zip(docs)
                .enumerate()
                .map(|(i, (o, doc))| match doc {
                    Some(doc) => format!("{}. {}{}\n\n{doc}\n", i + 1, f.name, signature(o)),
                    None => format!("{}. {}{}\n", i + 1, f.name, signature(o)),
                })
                .collect();
            format!(
                "{}(*args, **kwargs)\nOverloaded function.\n\n{}",
                f.name,
                each.join("\n")
            )
        }
    }
}

/// Whether `o` was bound without argument names: no `py::pos_only()`, and each parameter (after
/// a method's `self`) named as pybind11 names one it has no record of, `arg` and its position
/// among them (`cpp_function::initialize_generic`'s signature). pybind11 keeps argument records
/// only for an overload given `py::arg`s (one for each parameter; a method's `self` gets its
/// own) or `py::pos_only()`, and its dispatcher's step 3 skips an overload without records
/// that a keyword argument was given to, since no record consumed it.
pub fn unnamed(o: &Overload, method: bool) -> bool {
    let skip = usize::from(method);
    o.pos_only == 0
        && o.params.iter().enumerate().skip(skip).all(|(k, p)| {
            p.name
                .strip_prefix("arg")
                .is_some_and(|n| n == (k - skip).to_string())
        })
}

/// Binds `args` and `kwargs` to `o`'s parameters (of a method, `method`): one value per
/// parameter, or `None` when they don't fit.
fn bind<'py>(
    py: Python<'py>,
    o: &Overload,
    method: bool,
    args: &Bound<'py, PyTuple>,
    kwargs: Option<&Bound<'py, PyDict>>,
) -> PyResult<Option<Vec<Bound<'py, PyAny>>>> {
    if args.len() > o.params.len() {
        return Ok(None);
    }
    // An overload without argument names takes no keyword (pybind11 skips it whatever the
    // keyword names, `self` and `arg0` included).
    if kwargs.is_some_and(|k| !k.is_empty()) && unnamed(o, method) {
        return Ok(None);
    }
    let mut values: Vec<Bound<'py, PyAny>> = args.iter().collect();
    let mut used = 0;
    for (i, p) in o.params.iter().enumerate().skip(values.len()) {
        let given = match kwargs {
            Some(k) if i >= o.pos_only => k.get_item(p.name)?,
            _ => None,
        };
        match (given, p.default) {
            (Some(v), _) => {
                used += 1;
                values.push(v);
            }
            (None, Some(d)) => values.push((d.value)(py)?),
            (None, None) => return Ok(None),
        }
    }
    // A keyword argument no parameter took (unknown, or naming one given positionally).
    if kwargs.is_some_and(|k| k.len() > used) {
        return Ok(None);
    }
    Ok(Some(values))
}

/// Whether the arguments after `self` of the overload `o` load as pybind11's casters load them
/// with conversions, for the parameter types of the overloads [`crate::self_kind`] lists with a
/// null `self` (a `const char *`: a `str`, `bytes` or `None`; a class's holder: an instance or
/// `None`).
fn rest_loads(o: &Overload, values: &[Bound<'_, PyAny>]) -> bool {
    o.params.iter().zip(values).skip(1).all(|(p, v)| {
        if p.ty == "str" {
            v.is_none() || v.is_instance_of::<PyString>() || v.is_instance_of::<PyBytes>()
        } else if p.ty.starts_with("PyOpenColorIO.PyOpenColorIO.") {
            v.is_none() || crate::pybind11::instance_of(v, p.ty).is_some()
        } else {
            false
        }
    })
}

/// The overload `index` of `f`, `o`, called: where pybind11 would call it with a null `self`
/// (`None`, an unnamed overload whose `self` is a pointer or a holder, with conversions), the
/// port's error (U-134, [`crate::self_kind`]), or for an overload that only builds an iterator
/// of `self`, that iterator of the null pointer; and where pybind11's dispatcher catches C++
/// exceptions, a Python callback's, which the library carries as a `ForeignException`, is
/// its Python exception again (`error_already_set::restore`, chunk 6.6a).
fn call<'py>(
    py: Python<'py>,
    f: &Function,
    index: usize,
    o: &Overload,
    values: &[Bound<'py, PyAny>],
    convert: bool,
) -> PyResult<Option<Py<PyAny>>> {
    if convert && f.method && !f.constructor && values.first().is_some_and(|v| v.is_none()) {
        let ty = o.params[0].ty;
        let class = ty
            .strip_prefix("PyOpenColorIO.PyOpenColorIO.")
            .unwrap_or(ty);
        let kind = crate::self_kind::kind(class, f.name, index);
        if matches!(
            kind,
            Some(crate::self_kind::SelfKind::Pointer | crate::self_kind::SelfKind::Holder)
        ) && rest_loads(o, values)
        {
            return Err(crate::self_kind::null_self(class, f.name));
        }
        // An overload that only builds an iterator of `self`: the iterator of the null
        // pointer, an instance of the class it returns.
        if kind == Some(crate::self_kind::SelfKind::Iterator) && rest_loads(o, values) {
            let it = crate::py_utils::new_null_iterator(py, o.ret, class, f.name)?;
            return Ok(Some(it.unbind()));
        }
    }
    match ocio_ops::foreign_exception::try_foreign(|| (o.call)(py, o, values, convert)) {
        Ok(result) => result,
        Err(e) => Err(crate::callback::into_py_err(e)),
    }
}

/// Calls `f` as pybind11's dispatcher does (see the module's documentation).
pub fn dispatch<'py>(
    py: Python<'py>,
    f: &Function,
    args: &Bound<'py, PyTuple>,
    kwargs: Option<&Bound<'py, PyDict>>,
) -> PyResult<Py<PyAny>> {
    // A constructor (`__init__`, `__setstate__`) checks `self` before loading any argument:
    // an instance of its class, which it leaves alone when constructed already.
    if f.constructor {
        let this = args
            .get_item(0)
            .ok()
            .and_then(|a| crate::pybind11::instance_of(&a, f.overloads[0].params[0].ty));
        match this {
            None => {
                return Err(PyTypeError::new_err(
                    "__init__(self, ...) called with invalid or missing `self` argument",
                ));
            }
            Some(this) if crate::pybind11::constructed(&this, f.overloads[0].params[0].ty)? => {
                return Ok(py.None());
            }
            Some(_) => {}
        }
    }
    let overloaded = f.overloads.len() > 1;
    let mut second_pass = Vec::new();
    for (index, o) in f.overloads.iter().enumerate() {
        let Some(values) = bind(py, o, f.method, args, kwargs)? else {
            continue;
        };
        if let Some(result) = call(py, f, index, o, &values, !overloaded)? {
            return Ok(result);
        }
        // pybind11 tries again, with conversions, an overload with a positional parameter
        // (besides `self`) whose caster may convert: all of them, here.
        if overloaded && o.params.len() > usize::from(f.method) {
            second_pass.push((index, o, values));
        }
    }
    for (index, o, values) in second_pass {
        if let Some(result) = call(py, f, index, o, &values, true)? {
            return Ok(result);
        }
    }
    if f.operator {
        return Ok(py.NotImplemented());
    }
    Err(PyTypeError::new_err(incompatible(f, args, kwargs)?))
}

/// The `TypeError` text of a call no overload accepts, as pybind11's dispatcher writes it: each
/// argument's `repr()` ("<repr raised Error>" when it raises or isn't UTF-8, the C++ string
/// pybind11 makes of it), each keyword `"{}=".format(name)` and its value's `repr()`; then cut
/// at the first NUL, since pybind11 sets the error from the C string (I-237). A keyword whose
/// formatting fails (a name that isn't UTF-8) aborts the wheel's process, as pybind11 formats it
/// outside its `try`: the port's error (U-99).
fn incompatible(
    f: &Function,
    args: &Bound<'_, PyTuple>,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<String> {
    let mut msg = if f.constructor {
        format!(
            "{}(): incompatible constructor arguments. The following argument types are \
             supported:\n",
            f.name
        )
    } else {
        format!(
            "{}(): incompatible function arguments. The following argument types are \
             supported:\n",
            f.name
        )
    };
    for (i, o) in f.overloads.iter().enumerate() {
        let sig = signature(o);
        let shown = if f.constructor {
            constructor_signature(&sig).unwrap_or(sig)
        } else {
            sig
        };
        msg.push_str(&format!("    {}. {shown}\n", i + 1));
    }
    msg.push_str("\nInvoked with: ");
    let skip = usize::from(f.constructor);
    let reprs: Vec<String> = args.iter().skip(skip).map(|a| repr_or_error(&a)).collect();
    msg.push_str(&reprs.join(", "));
    if let Some(kwargs) = kwargs
        && !kwargs.is_empty()
    {
        if !reprs.is_empty() {
            msg.push_str("; ");
        }
        msg.push_str("kwargs: ");
        let mut items = Vec::new();
        for (k, v) in kwargs.iter() {
            let key = PyString::new(k.py(), "{}=")
                .call_method1("format", (&k,))
                .and_then(|s| Ok(s.cast_into::<PyString>()?.to_str()?.to_string()))
                .map_err(|e| {
                    let error = PyTypeError::new_err(format!(
                        "{}(): a keyword argument's name pybind11 can't write in its error \
                         (the wheel's process aborts; docs/improvements.md, U-99)",
                        f.name
                    ));
                    error.set_cause(k.py(), Some(e));
                    error
                })?;
            items.push(format!("{key}{}", repr_or_error(&v)));
        }
        msg.push_str(&items.join(", "));
    }
    if let Some(nul) = msg.find('\0') {
        msg.truncate(nul);
    }
    Ok(msg)
}

/// `repr(v)` as pybind11 appends it to a C++ string: "<repr raised Error>" when `repr()` raises
/// or gives text that isn't UTF-8 (a lone surrogate).
fn repr_or_error(v: &Bound<'_, PyAny>) -> String {
    v.repr()
        .and_then(|r| Ok(r.to_str()?.to_string()))
        .unwrap_or_else(|_| "<repr raised Error>".to_string())
}

/// pybind11's rewrite of a constructor's signature `(self: Class, a: int) -> None` as
/// `Class(a: int)`: from after `(self: ` to the first `, ` (or the `)`), then the rest up to
/// ` -> `.
fn constructor_signature(sig: &str) -> Option<String> {
    let start = sig.find('(')? + 7;
    let ret = sig.rfind(" -> ")?;
    let (end, next) = match sig.find(", ") {
        Some(end) => (end, end + 2),
        None => {
            let end = sig.find(')')?;
            (end, end)
        }
    };
    (start < end && next < sig.len() && start < sig.len())
        .then(|| format!("{}({}", &sig[start..end], &sig[next..ret]))
}

// ---------------------------------------------------------------------------------------------
// Casters

/// A pybind11 type caster: loads a Python value into a C++ parameter's type, or refuses it
/// (`None`), with or without the conversions of the dispatcher's second pass.
pub trait Cast<'py>: Sized {
    /// `sizeof` of the C++ type the caster loads, which a container caster reserves room for.
    const CPP_SIZE: usize;

    /// The value, or `None` when the caster refuses it. A Python error raised while loading is
    /// cleared, as pybind11 does.
    fn load(obj: &Bound<'py, PyAny>, convert: bool) -> Option<Self>;

    /// [`load`](Self::load), or the Python error the caster lets through instead of refusing
    /// the value (the sequence casters: the length of a sequence that has none, `tuple(obj)`
    /// of a convertible non-sequence that isn't iterable). The dispatcher loads through it.
    fn try_load(obj: &Bound<'py, PyAny>, convert: bool) -> PyResult<Option<Self>> {
        Ok(Self::load(obj, convert))
    }
}

/// Loads argument `i` of `args` as `T`, returning `Ok(None)` from the enclosing [`Call`] when it
/// doesn't cast (the next overload is tried).
#[macro_export]
macro_rules! arg {
    ($args:expr, $i:expr, $convert:expr, $t:ty) => {
        match <$t as $crate::dispatch::Cast>::try_load(&$args[$i], $convert)? {
            Some(v) => v,
            None => return Ok(None),
        }
    };
}

/// Whether `obj`'s type has the number slot `slot` (`Py_TYPE(obj)->tp_as_number->nb_...`), the
/// C slot itself: not the special method an attribute lookup finds, which a metaclass's
/// method or a static type's base can show without the slot (re-check round 4).
fn has_slot(obj: &Bound<'_, PyAny>, slot: NumberSlot) -> bool {
    ocio_py_ffi::has_number_slot(&obj.get_type(), slot)
}

/// `PyNumber_Check` (Objects/abstract.c): the slot `nb_index`, `nb_int` or `nb_float`, or a
/// complex number (`PyComplex_Check`, a subclass too).
fn number_check(obj: &Bound<'_, PyAny>) -> bool {
    has_slot(obj, NumberSlot::Index)
        || has_slot(obj, NumberSlot::Int)
        || has_slot(obj, NumberSlot::Float)
        || obj.is_instance_of::<pyo3::types::PyComplex>()
}

/// The C integer types pybind11 loads through `PyLong_AsLong`, `PyLong_AsUnsignedLong` and
/// their `long long` forms. `long` and `unsigned long` are 32 bits in the Windows wheel and 64
/// bits in the Linux one ([`CLong`], [`CULong`]).
macro_rules! int_caster {
    ($t:ty, $signed:expr) => {
        impl<'py> Cast<'py> for $t {
            const CPP_SIZE: usize = std::mem::size_of::<$t>();

            /// pybind11's integer caster: never a `float` (a subclass included); without
            /// conversions, an `int` (a `bool` too) or an object whose type has `nb_index`
            /// (`PyIndex_Check`); the value through `PyLong_AsLong` (signed, which calls
            /// `__index__`: a pybind11 enum value goes to a C++ `long` in the first pass, I-12)
            /// or `PyLong_AsUnsignedLong` (unsigned, an `int` only: an object with `__index__`
            /// is refused in the first pass, so a `list` subclass with `__index__` goes to a
            /// later overload's sequence), refused when it errs or doesn't fit; with
            /// conversions, when that errs for an object `PyNumber_Check` accepts, again with
            /// `int(obj)`.
            fn load(obj: &Bound<'py, PyAny>, convert: bool) -> Option<Self> {
                if obj.is_instance_of::<PyFloat>()
                    || (!convert
                        && !obj.is_instance_of::<PyInt>()
                        && !has_slot(obj, NumberSlot::Index))
                {
                    return None;
                }
                let value: Result<Self, bool> = if $signed {
                    match obj.extract::<i64>() {
                        Ok(v) => <$t>::try_from(v).map_err(|_| false),
                        Err(_) => Err(true),
                    }
                } else if obj.is_instance_of::<PyInt>() {
                    match obj.extract::<u64>() {
                        Ok(v) => <$t>::try_from(v).map_err(|_| false),
                        Err(_) => Err(true),
                    }
                } else {
                    Err(true)
                };
                match value {
                    Ok(v) => Some(v),
                    // An error (not merely a value out of range): convert through int().
                    Err(true) if convert && number_check(obj) => {
                        let int = obj.py().get_type::<PyInt>().call1((obj,)).ok()?;
                        Self::load(&int, false)
                    }
                    Err(_) => None,
                }
            }
        }
    };
}

int_caster!(i32, true);
int_caster!(u32, false);
int_caster!(i64, true);
int_caster!(u64, false);
int_caster!(u8, false);

/// C++ `long`: 32 bits in the Windows wheel (MSVC), 64 in the Linux one.
#[cfg(windows)]
pub type CLong = i32;
/// C++ `long`: 32 bits in the Windows wheel (MSVC), 64 in the Linux one.
#[cfg(not(windows))]
pub type CLong = i64;
/// C++ `unsigned long`: 32 bits in the Windows wheel, 64 in the Linux one.
#[cfg(windows)]
pub type CULong = u32;
/// C++ `unsigned long`: 32 bits in the Windows wheel, 64 in the Linux one.
#[cfg(not(windows))]
pub type CULong = u64;

macro_rules! float_caster {
    ($t:ty) => {
        impl<'py> Cast<'py> for $t {
            const CPP_SIZE: usize = std::mem::size_of::<$t>();

            /// pybind11's floating-point caster: without conversions, a `float` or an `int`
            /// (a `bool` too); the value through `PyFloat_AsDouble` (which takes `__float__`
            /// and `__index__`); with conversions, any object, and when that fails with a
            /// `TypeError` for an object `PyNumber_Check` accepts, again with `float(obj)`. A
            /// C++ `float` takes the double, rounded.
            fn load(obj: &Bound<'py, PyAny>, convert: bool) -> Option<Self> {
                if !convert && !obj.is_instance_of::<PyFloat>() && !obj.is_instance_of::<PyInt>() {
                    return None;
                }
                match obj.extract::<f64>() {
                    Ok(v) => Some(v as $t),
                    Err(e)
                        if convert
                            && e.is_instance_of::<PyTypeError>(obj.py())
                            && number_check(obj) =>
                    {
                        let f = obj.py().get_type::<PyFloat>().call1((obj,)).ok()?;
                        Self::load(&f, false)
                    }
                    Err(_) => None,
                }
            }
        }
    };
}

float_caster!(f64);
float_caster!(f32);

impl<'py> Cast<'py> for bool {
    const CPP_SIZE: usize = 1;

    /// pybind11's `bool` caster: `True` and `False`; with conversions, or for a NumPy bool,
    /// `None` (as `false`) and any object whose type has the slot `nb_bool` (its result, which
    /// `PyObject_IsTrue` calls the slot for once `True`, `False` and `None` are out of the
    /// way; an error refuses the value).
    fn load(obj: &Bound<'py, PyAny>, convert: bool) -> Option<Self> {
        if let Ok(b) = obj.cast::<PyBool>() {
            return Some(b.is_true());
        }
        let numpy_bool = obj
            .get_type()
            .fully_qualified_name()
            .is_ok_and(|n| n == "numpy.bool" || n == "numpy.bool_");
        if !(convert || numpy_bool) {
            return None;
        }
        if obj.is_none() {
            return Some(false);
        }
        if !has_slot(obj, NumberSlot::Bool) {
            return None;
        }
        obj.is_truthy().ok()
    }
}

/// A C++ `std::string` parameter: pybind11's string caster takes a `str` (strictly encoded as
/// UTF-8: a lone surrogate refuses it), `bytes` or a `bytearray`, as they are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StdString(pub Vec<u8>);

impl<'py> Cast<'py> for StdString {
    /// `std::string`: 32 bytes in MSVC's STL and in libstdc++.
    const CPP_SIZE: usize = 32;

    fn load(obj: &Bound<'py, PyAny>, _convert: bool) -> Option<Self> {
        if let Ok(s) = obj.cast::<PyString>() {
            return s
                .encode_utf8()
                .ok()
                .map(|b| StdString(b.as_bytes().to_vec()));
        }
        if let Ok(b) = obj.cast::<PyBytes>() {
            return Some(StdString(b.as_bytes().to_vec()));
        }
        if let Ok(b) = obj.cast::<PyByteArray>() {
            return Some(StdString(b.to_vec()));
        }
        None
    }
}

/// A C++ `const char *` parameter: as [`StdString`], up to its first NUL (the C++ function
/// sees no further), and, with conversions only, `None` as a null pointer (`None` here).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharPtr(pub Option<Vec<u8>>);

impl<'py> Cast<'py> for CharPtr {
    const CPP_SIZE: usize = 8;

    fn load(obj: &Bound<'py, PyAny>, convert: bool) -> Option<Self> {
        if obj.is_none() {
            return convert.then_some(CharPtr(None));
        }
        let StdString(mut bytes) = StdString::load(obj, convert)?;
        if let Some(nul) = bytes.iter().position(|&b| b == 0) {
            bytes.truncate(nul);
        }
        Some(CharPtr(Some(bytes)))
    }
}

/// A `const char *` parameter declared `.none(false)` (`"src"_a.none(false)`): as [`CharPtr`],
/// but `None` is refused, also with conversions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NonNullCharPtr(pub Vec<u8>);

impl<'py> Cast<'py> for NonNullCharPtr {
    const CPP_SIZE: usize = 8;

    fn load(obj: &Bound<'py, PyAny>, convert: bool) -> Option<Self> {
        if obj.is_none() {
            return None;
        }
        CharPtr::load(obj, convert)?.0.map(NonNullCharPtr)
    }
}
/// pybind11's `object_is_convertible_to_std_vector`: a sequence (`PySequence_Check`) that isn't
/// a `str` or `bytes`; else a generator (`PyGen_Check`, its exact type), a set or frozenset (or
/// a subclass), or an instance (not a class) of a type whose C name (`tp_name`) is
/// `dict_keys`, `dict_values`, `dict_items`, `map` or `zip`.
fn convertible_to_vector(obj: &Bound<'_, PyAny>) -> bool {
    if ocio_py_ffi::sequence_check(obj) {
        return !obj.is_instance_of::<PyString>() && !obj.is_instance_of::<PyBytes>();
    }
    let generator = obj
        .py()
        .import("types")
        .and_then(|t| t.getattr("GeneratorType"))
        .is_ok_and(|g| obj.get_type().is(&g));
    generator
        || obj.is_instance_of::<PySet>()
        || obj.is_instance_of::<PyFrozenSet>()
        || (!obj.is_instance_of::<PyType>()
            && c_name_is_one_of(
                obj,
                &["dict_keys", "dict_values", "dict_items", "map", "zip"],
            ))
}

/// Whether the C name (`tp_name`) of `obj`'s type is one of `names`. A type's `__name__` is
/// its C name's last dotted part, so it must be the name; the C name itself is what
/// `object.__format__` writes in its error for a format it doesn't take (CPython's
/// `object___format___impl`, "unsupported format string passed to %.200s.__format__"), which
/// runs no code of the object's.
fn c_name_is_one_of(obj: &Bound<'_, PyAny>, names: &[&str]) -> bool {
    let py = obj.py();
    let Some(name) = obj
        .get_type()
        .name()
        .ok()
        .and_then(|n| names.iter().find(|m| n == **m).copied())
    else {
        return false;
    };
    let expected = format!("unsupported format string passed to {name}.__format__");
    match py
        .get_type::<PyAny>()
        .getattr("__format__")
        .and_then(|f| f.call1((obj, "x")))
    {
        Ok(_) => false,
        Err(e) => {
            e.is_instance_of::<PyTypeError>(py)
                && e.value(py).str().is_ok_and(|s| s == expected.as_str())
        }
    }
}

/// The object a container caster reads the items of: `obj` when it is a sequence; else, with
/// conversions only, `tuple(obj)` (`PySequence_Tuple`, whose error goes through), as pybind11
/// does for the generators, sets and views [`convertible_to_vector`] takes; else `None`.
fn sequence_of<'py>(obj: &Bound<'py, PyAny>, convert: bool) -> PyResult<Option<Bound<'py, PyAny>>> {
    if !convertible_to_vector(obj) {
        return Ok(None);
    }
    if ocio_py_ffi::sequence_check(obj) {
        return Ok(Some(obj.clone()));
    }
    if !convert {
        return Ok(None);
    }
    Ok(Some(obj.py().get_type::<PyTuple>().call1((obj,))?))
}

/// `std::vector<T>::reserve(n)` of a C++ type of `size` bytes, as the wheel's C++ runtime runs
/// it on an empty vector: more than `max_size()` elements throw `std::length_error` (pybind11's
/// ValueError, "vector too long" from MSVC's STL, "vector::reserve" from libstdc++); an
/// allocation that fails throws `std::bad_alloc` (MemoryError, "bad allocation" or
/// "std::bad_alloc"); MSVC's, for 4096 bytes or more, first adds 47 bytes for its alignment
/// (`_Allocate_manually_vector_aligned`), and throws `std::bad_array_new_length` ("bad array
/// new length") when that overflows. The memory is reserved as the wheel's, and held while the
/// items load (docs/improvements.md, I-236).
fn reserve(n: usize, size: usize) -> PyResult<Vec<u8>> {
    use pyo3::exceptions::{PyMemoryError, PyValueError};
    let ptrdiff_max = isize::MAX.unsigned_abs();
    // MSVC: min(PTRDIFF_MAX, SIZE_MAX / sizeof(T)); libstdc++: PTRDIFF_MAX / sizeof(T).
    #[cfg(windows)]
    let (max, too_long, bad_alloc) = (
        ptrdiff_max.min(usize::MAX / size),
        "vector too long",
        "bad allocation",
    );
    #[cfg(not(windows))]
    let (max, too_long, bad_alloc) = (ptrdiff_max / size, "vector::reserve", "std::bad_alloc");
    if n > max {
        return Err(PyValueError::new_err(too_long));
    }
    let mut bytes = n * size;
    if cfg!(windows) && bytes >= 4096 {
        bytes = bytes
            .checked_add(47)
            .ok_or_else(|| PyMemoryError::new_err("bad array new length"))?;
    }
    let mut reserved = Vec::new();
    reserved
        .try_reserve_exact(bytes)
        .map_err(|_| PyMemoryError::new_err(bad_alloc))?;
    Ok(reserved)
}

impl<'py, T: Cast<'py>> Cast<'py> for Vec<T> {
    const CPP_SIZE: usize = 24;

    fn load(obj: &Bound<'py, PyAny>, convert: bool) -> Option<Self> {
        Self::try_load(obj, convert).ok().flatten()
    }

    /// pybind11's `std::vector` caster (`list_caster`): the sequence ([`sequence_of`]); its
    /// `len()` (whose error goes through) reserved ([`reserve`]); then its items, read as
    /// `iter()` gives them, each loaded by `T`'s caster before the next is read, up to the
    /// first `T` refuses (the vector is then refused).
    fn try_load(obj: &Bound<'py, PyAny>, convert: bool) -> PyResult<Option<Self>> {
        let Some(seq) = sequence_of(obj, convert)? else {
            return Ok(None);
        };
        let _reserved = reserve(seq.len()?, T::CPP_SIZE)?;
        let mut out = Vec::new();
        for item in seq.try_iter()? {
            match T::try_load(&item?, convert)? {
                Some(v) => out.push(v),
                None => return Ok(None),
            }
        }
        Ok(Some(out))
    }
}

impl<'py, T: Cast<'py>, const N: usize> Cast<'py> for [T; N] {
    const CPP_SIZE: usize = N * T::CPP_SIZE;

    fn load(obj: &Bound<'py, PyAny>, convert: bool) -> Option<Self> {
        Self::try_load(obj, convert).ok().flatten()
    }

    /// pybind11's `std::array` caster (`array_caster`): the sequence ([`sequence_of`]), refused
    /// unless its `len()` is `N`; a second `len()` reserved ([`reserve`]) for the items; then
    /// the items by index (`PySequence_GetItem`), from 0 up to a third `len()`, each loaded by
    /// `T`'s caster before the next is read, up to the first `T` refuses; the array is the
    /// first `N`. Every `len()`'s and read's error goes through, but the third `len()`'s, which
    /// pybind11 doesn't check (U-96), and a third `len()` under `N`, which leaves the array
    /// partly unread (U-97): both are the port's errors.
    fn try_load(obj: &Bound<'py, PyAny>, convert: bool) -> PyResult<Option<Self>> {
        let Some(seq) = sequence_of(obj, convert)? else {
            return Ok(None);
        };
        if seq.len()? != N {
            return Ok(None);
        }
        let _reserved = reserve(seq.len()?, T::CPP_SIZE)?;
        let end = match seq.len() {
            Ok(end) => end,
            Err(e) => {
                let error = pyo3::exceptions::PyRuntimeError::new_err(
                    "the sequence's third len() raised, and pybind11 reads its items with that \
                     exception set (docs/improvements.md, U-96)",
                );
                error.set_cause(seq.py(), Some(e));
                return Err(error);
            }
        };
        let mut items = Vec::new();
        for i in 0..end {
            match T::try_load(&seq.get_item(i)?, convert)? {
                Some(v) => items.push(v),
                None => return Ok(None),
            }
        }
        if items.len() < N {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
                "the sequence's third len() gave {end}, and pybind11 reads the {N} values of \
                 the array from memory it didn't initialize (docs/improvements.md, U-97)"
            )));
        }
        items.truncate(N);
        Ok(items.try_into().ok())
    }
}

/// A `std::map<std::string, std::string>` parameter: pybind11's `map_caster`. A `dict` (or a
/// subclass), its items as the dict iterates them; else, with conversions only, an object with
/// an `items` attribute whose type has `__getitem__` (pybind11's
/// `object_is_convertible_to_std_map`, `PyMapping_Check`), whose items are
/// `dict(PyMapping_Items(obj))` (their errors go through). Each key and value loads as a
/// `std::string`, the first that doesn't refusing the map; a key the map holds already (`'a'`
/// and `b'a'`) keeps its first value (`emplace`). In the keys' byte order, as `std::map`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StringMap(pub std::collections::BTreeMap<Vec<u8>, Vec<u8>>);

impl<'py> Cast<'py> for StringMap {
    /// `std::map`: 16 bytes in MSVC's STL, 48 in libstdc++ (not reserved: no vector holds it).
    const CPP_SIZE: usize = if cfg!(windows) { 16 } else { 48 };

    fn load(obj: &Bound<'py, PyAny>, convert: bool) -> Option<Self> {
        Self::try_load(obj, convert).ok().flatten()
    }

    fn try_load(obj: &Bound<'py, PyAny>, convert: bool) -> PyResult<Option<Self>> {
        let py = obj.py();
        let dict = match obj.cast::<PyDict>() {
            Ok(dict) => dict.clone(),
            Err(_) => {
                // PyMapping_Check: the type's mp_subscript, which a type with `__getitem__` has
                // (a C type with only sq_item and an `items` attribute is the exception).
                let mapping = obj.hasattr("items").unwrap_or(false)
                    && crate::pybind11::type_lookup(&obj.get_type(), "__getitem__")?.is_some();
                if !mapping || !convert {
                    return Ok(None);
                }
                dict_of_items(obj)?
            }
        };
        let mut map = std::collections::BTreeMap::new();
        for (k, v) in dict.iter() {
            let Some(StdString(k)) = StdString::try_load(&k, convert)? else {
                return Ok(None);
            };
            let Some(StdString(v)) = StdString::try_load(&v, convert)? else {
                return Ok(None);
            };
            map.entry(k).or_insert(v);
        }
        let _ = py;
        Ok(Some(StringMap(map)))
    }
}

/// `dict(PyMapping_Items(obj))` of an object that isn't a `dict`: `obj.items()` as a list
/// (CPython's `method_output_as_list`, whose TypeError for a result that isn't iterable names
/// the types), then a dict of it.
fn dict_of_items<'py>(obj: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyDict>> {
    let py = obj.py();
    let out = obj.call_method0("items")?;
    let it = match out.try_iter() {
        Ok(it) => it,
        Err(e) if e.is_instance_of::<PyTypeError>(py) => {
            return Err(PyTypeError::new_err(format!(
                "{}.items() returned a non-iterable (type {})",
                crate::pybind11::c_type_name(obj)?,
                crate::pybind11::c_type_name(&out)?
            )));
        }
        Err(e) => return Err(e),
    };
    let list = py.get_type::<pyo3::types::PyList>().call1((it,))?;
    Ok(py
        .get_type::<PyDict>()
        .call1((list,))?
        .cast_into::<PyDict>()?)
}

/// Any Python object (`py::object`), as it is.
impl<'py> Cast<'py> for Bound<'py, PyAny> {
    /// `py::object`: a pointer.
    const CPP_SIZE: usize = 8;

    fn load(obj: &Bound<'py, PyAny>, _convert: bool) -> Option<Self> {
        Some(obj.clone())
    }
}

// ---------------------------------------------------------------------------------------------
// Returned values

/// A returned `std::string` as pybind11 hands it to Python: decoded as strict UTF-8
/// (`PyUnicode_DecodeUTF8`), so bytes that aren't UTF-8 raise `UnicodeDecodeError`.
pub fn std_string_to_py<'py>(py: Python<'py>, bytes: &[u8]) -> PyResult<Bound<'py, PyString>> {
    PyString::from_encoded_object(&PyBytes::new(py, bytes), Some(c"utf-8"), Some(c"strict"))
}

/// A returned `const char *`: `None` for a null pointer, else as [`std_string_to_py`].
pub fn char_ptr_to_py<'py>(py: Python<'py>, bytes: Option<&[u8]>) -> PyResult<Bound<'py, PyAny>> {
    match bytes {
        Some(b) => Ok(std_string_to_py(py, b)?.into_any()),
        None => Ok(py.None().into_bound(py)),
    }
}
