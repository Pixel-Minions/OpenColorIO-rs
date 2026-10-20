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
//!   a missing argument without a default) is skipped.
//! - **Two passes.** A function with one overload is called once, its casters allowed to
//!   convert. With several, each is first tried without conversions, in definition order; then
//!   those whose arguments bound (and that have a positional parameter besides `self`) again,
//!   with conversions.
//! - **The casters** ([`Cast`]): see each implementation.
//! - **No overload accepts the arguments**: `TypeError`, "`name`(): incompatible function
//!   arguments. The following argument types are supported:" with each overload's signature
//!   and "Invoked with:" the arguments' reprs (for a constructor, "__init__(): incompatible
//!   constructor arguments." and each signature as `Class(params)`).
//! - **Docstrings** ([`docstring`]): `name(params) -> ret` and a line end; for several
//!   overloads, `name(*args, **kwargs)`, "Overloaded function." and each numbered.

use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{
    PyBool, PyByteArray, PyBytes, PyDict, PyFloat, PyFrozenSet, PyInt, PySet, PyString, PyTuple,
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

/// The C++ function behind an overload: called with the bound arguments (one per parameter,
/// defaults filled in) and whether casters may convert. `Ok(None)` when an argument doesn't
/// cast (pybind11's `PYBIND11_TRY_NEXT_OVERLOAD`): the dispatcher tries the next overload.
pub type Call = for<'py> fn(Python<'py>, &[Bound<'py, PyAny>], bool) -> PyResult<Option<Py<PyAny>>>;

/// One overload: its parameters, its return type as the signature prints it, its function.
#[derive(Debug, Clone, Copy)]
pub struct Overload {
    pub params: &'static [Param],
    pub ret: &'static str,
    pub call: Call,
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
    pub overloads: &'static [Overload],
}

/// An overload's signature as pybind11 writes it: `(a: int, b: str = 'x') -> ret`.
pub fn signature(o: &Overload) -> String {
    let params: Vec<String> = o
        .params
        .iter()
        .map(|p| match p.default {
            Some(d) => format!("{}: {} = {}", p.name, p.ty, d.repr),
            None => format!("{}: {}", p.name, p.ty),
        })
        .collect();
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

/// Binds `args` and `kwargs` to `o`'s parameters: one value per parameter, or `None` when they
/// don't fit.
fn bind<'py>(
    py: Python<'py>,
    o: &Overload,
    args: &Bound<'py, PyTuple>,
    kwargs: Option<&Bound<'py, PyDict>>,
) -> PyResult<Option<Vec<Bound<'py, PyAny>>>> {
    if args.len() > o.params.len() {
        return Ok(None);
    }
    let mut values: Vec<Bound<'py, PyAny>> = args.iter().collect();
    let mut used = 0;
    for p in &o.params[values.len()..] {
        let given = match kwargs {
            Some(k) => k.get_item(p.name)?,
            None => None,
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

/// Calls `f` as pybind11's dispatcher does (see the module's documentation).
pub fn dispatch<'py>(
    py: Python<'py>,
    f: &Function,
    args: &Bound<'py, PyTuple>,
    kwargs: Option<&Bound<'py, PyDict>>,
) -> PyResult<Py<PyAny>> {
    let overloaded = f.overloads.len() > 1;
    let mut second_pass = Vec::new();
    for o in f.overloads {
        let Some(values) = bind(py, o, args, kwargs)? else {
            continue;
        };
        if let Some(result) = (o.call)(py, &values, !overloaded)? {
            return Ok(result);
        }
        // pybind11 tries again, with conversions, an overload with a positional parameter
        // (besides `self`) whose caster may convert: all of them, here.
        if overloaded && o.params.len() > usize::from(f.method) {
            second_pass.push((o, values));
        }
    }
    for (o, values) in second_pass {
        if let Some(result) = (o.call)(py, &values, true)? {
            return Ok(result);
        }
    }
    Err(PyTypeError::new_err(incompatible(f, args, kwargs)))
}

/// The `TypeError` text of a call no overload accepts.
fn incompatible(
    f: &Function,
    args: &Bound<'_, PyTuple>,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> String {
    let mut msg = if f.constructor {
        "__init__(): incompatible constructor arguments. The following argument types are \
         supported:\n"
            .to_string()
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
        let items: Vec<String> = kwargs
            .iter()
            .map(|(k, v)| {
                let key = k.str().map(|s| s.to_string()).unwrap_or_default();
                format!("{key}={}", repr_or_error(&v))
            })
            .collect();
        msg.push_str(&items.join(", "));
    }
    msg
}

fn repr_or_error(v: &Bound<'_, PyAny>) -> String {
    v.repr()
        .map(|r| r.to_string())
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
    /// The value, or `None` when the caster refuses it. A Python error raised while loading is
    /// cleared, as pybind11 does.
    fn load(obj: &Bound<'py, PyAny>, convert: bool) -> Option<Self>;
}

/// Loads argument `i` of `args` as `T`, returning `Ok(None)` from the enclosing [`Call`] when it
/// doesn't cast (the next overload is tried).
#[macro_export]
macro_rules! arg {
    ($args:expr, $i:expr, $convert:expr, $t:ty) => {
        match <$t as $crate::dispatch::Cast>::load(&$args[$i], $convert) {
            Some(v) => v,
            None => return Ok(None),
        }
    };
}

/// Whether `obj`'s type has the special method `name` (a number slot of CPython's).
fn has_slot(obj: &Bound<'_, PyAny>, name: &str) -> bool {
    obj.get_type().hasattr(name).unwrap_or(false)
}

/// `PyIndex_Check`: the type has `__index__`.
fn index_check(obj: &Bound<'_, PyAny>) -> bool {
    has_slot(obj, "__index__")
}

/// `PyNumber_Check`: `__index__`, `__int__` or `__float__`, or a complex number.
fn number_check(obj: &Bound<'_, PyAny>) -> bool {
    has_slot(obj, "__index__")
        || has_slot(obj, "__int__")
        || has_slot(obj, "__float__")
        || obj.is_instance_of::<pyo3::types::PyComplex>()
}

/// The C integer types pybind11 loads through `PyLong_AsLong`, `PyLong_AsUnsignedLong` and
/// their `long long` forms. `long` and `unsigned long` are 32 bits in the Windows wheel and 64
/// bits in the Linux one ([`CLong`], [`CULong`]).
macro_rules! int_caster {
    ($t:ty, $signed:expr) => {
        impl<'py> Cast<'py> for $t {
            /// pybind11's integer caster: never a `float` (a subclass included); without
            /// conversions, an `int` (a `bool` too) or an object with `__index__`; the value
            /// through `PyLong_AsLong` (signed, which calls `__index__`) or
            /// `PyLong_AsUnsignedLong` (unsigned, an `int` only), refused when it errs or
            /// doesn't fit; with conversions, when that errs for an object `PyNumber_Check`
            /// accepts, again with `int(obj)`.
            fn load(obj: &Bound<'py, PyAny>, convert: bool) -> Option<Self> {
                if obj.is_instance_of::<PyFloat>()
                    || (!convert && !obj.is_instance_of::<PyInt>() && !index_check(obj))
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
    /// pybind11's `bool` caster: `True` and `False`; with conversions, or for a NumPy bool,
    /// `None` (as `false`) and any object whose type has `__bool__` (its result).
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
        if !has_slot(obj, "__bool__") {
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

/// pybind11's `object_is_convertible_to_std_vector`: a sequence that isn't a `str` or `bytes`;
/// a generator, a set or frozenset; or a `dict_keys`, `dict_values`, `dict_items`, `map` or
/// `zip`. The sequences are taken as they are; the others, with conversions only, through
/// `tuple(obj)`.
fn sequence_items<'py>(obj: &Bound<'py, PyAny>, convert: bool) -> Option<Vec<Bound<'py, PyAny>>> {
    if obj.is_instance_of::<PyString>() || obj.is_instance_of::<PyBytes>() {
        return None;
    }
    // PySequence_Check: the type has `__getitem__` (its sequence slot), and isn't a dict.
    let sequence = !obj.is_instance_of::<PyDict>() && has_slot(obj, "__getitem__");
    if !sequence {
        let other = obj.is_instance_of::<PySet>()
            || obj.is_instance_of::<PyFrozenSet>()
            || obj.get_type().name().is_ok_and(|n| {
                [
                    "generator",
                    "dict_keys",
                    "dict_values",
                    "dict_items",
                    "map",
                    "zip",
                ]
                .iter()
                .any(|t| n == *t)
            });
        if !other || !convert {
            return None;
        }
        let tuple = obj.py().get_type::<PyTuple>().call1((obj,)).ok()?;
        return tuple.try_iter().ok()?.collect::<PyResult<Vec<_>>>().ok();
    }
    obj.try_iter().ok()?.collect::<PyResult<Vec<_>>>().ok()
}

impl<'py, T: Cast<'py>> Cast<'py> for Vec<T> {
    /// pybind11's `std::vector` caster: [`sequence_items`], each item through `T`'s caster.
    fn load(obj: &Bound<'py, PyAny>, convert: bool) -> Option<Self> {
        sequence_items(obj, convert)?
            .iter()
            .map(|item| T::load(item, convert))
            .collect()
    }
}

impl<'py, T: Cast<'py>, const N: usize> Cast<'py> for [T; N] {
    /// pybind11's `std::array` caster: as `std::vector`'s, with exactly `N` items.
    fn load(obj: &Bound<'py, PyAny>, convert: bool) -> Option<Self> {
        let items: Vec<T> = Vec::load(obj, convert)?;
        items.try_into().ok()
    }
}

/// Any Python object (`py::object`), as it is.
impl<'py> Cast<'py> for Bound<'py, PyAny> {
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
