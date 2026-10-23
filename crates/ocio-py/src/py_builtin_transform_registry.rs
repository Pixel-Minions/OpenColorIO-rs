// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyBuiltinTransformRegistry` (src/bindings/python/PyBuiltinTransformRegistry.cpp and
//! PyBuiltinTransformRegistry.h @ v2.5.2): a class over the library's global registry, with its
//! two iterators.

use ocio::BuiltinTransformRegistry;
use ocio_ops::utils::string_utils::compare;
use pyo3::prelude::*;
use pyo3::types::PyTuple;

use crate::dispatch::{Function, Overload, Param, StdString};
use crate::py_open_color_io::py_result;
use crate::py_transform::{INT, class, def, nested_class, none, ok, string_out};
use crate::py_utils::new_iterator;
use crate::{method_def, q, this_iter};

const REGISTRY: &str = q!("BuiltinTransformRegistry");
const STYLE_ITERATOR: &str = q!("BuiltinTransformRegistry.BuiltinStyleIterator");
const BUILTIN_ITERATOR: &str = q!("BuiltinTransformRegistry.BuiltinIterator");
const SELF: Param = Param::new("self", REGISTRY);
const STYLE_SELF: Param = Param::new("self", STYLE_ITERATOR);
const BUILTIN_SELF: Param = Param::new("self", BUILTIN_ITERATOR);

/// `self`, an instance of the registry class, or the next overload. Its C++ value
/// (`PyBuiltinTransformRegistry`, PyBuiltinTransformRegistry.h:13-33 @ v2.5.2) holds nothing:
/// each method reads the global registry (`BuiltinTransformRegistry::Get()`). An instance no
/// constructor constructed is U-86's error, as the wheel reads its memory.
macro_rules! this_registry {
    ($o:expr, $a:expr) => {
        match crate::pybind11::instance_of(&$a[0], $o.params[0].ty) {
            Some(this) => {
                crate::pybind11::read_value(&this, $o.params[0].ty)?;
            }
            None => return Ok(None),
        }
    };
}

/// `getBuiltinStyle(idx)` of an `int` index, which C++ converts to `size_t` (a negative one
/// wraps to a huge one, which the registry refuses: "Invalid index.").
fn style(py: Python<'_>, i: i32) -> PyResult<&'static [u8]> {
    py_result(
        py,
        BuiltinTransformRegistry::get().builtin_style(i as usize),
    )
}

/// `getBuiltinDescription(idx)`, as [`style`].
fn description(py: Python<'_>, i: i32) -> PyResult<&'static [u8]> {
    py_result(
        py,
        BuiltinTransformRegistry::get().builtin_description(i as usize),
    )
}

/// The number of built-in transforms, as `getNumBuiltins` (a `size_t`) gives it to Python.
fn num_builtins() -> usize {
    BuiltinTransformRegistry::get().num_builtins()
}

/// The index of the style that equals `style` ignoring case (`StringUtils::Compare`).
fn find(style: &[u8]) -> Option<usize> {
    let registry = BuiltinTransformRegistry::get();
    (0..registry.num_builtins()).find(|&i| {
        registry
            .builtin_style(i)
            .is_ok_and(|this_style| compare(this_style, style))
    })
}

// .def(py::init<>()) (PyBuiltinTransformRegistry.cpp:39 @ v2.5.2).
static INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    operator: false,
    overloads: &[Overload {
        params: &[SELF],
        ret: "None",
        call: |py, o, a, _| {
            crate::pybind11::set_value(&a[0], o.params[0].ty, py.None().into_bound(py))?;
            none(py)
        },
        pos_only: 0,
    }],
};

// .def("__iter__", [](self) { return BuiltinStyleIterator(self); })
// (PyBuiltinTransformRegistry.cpp:41-45 @ v2.5.2).
static ITER: Function = method_def!("__iter__", [SELF], STYLE_ITERATOR, |py, o, a, _| {
    this_registry!(o, a);
    Ok(Some(new_iterator(py, STYLE_ITERATOR, None)?.unbind()))
});

// .def("__len__", [](self) { return self.getNumBuiltins(); }) (PyBuiltinTransformRegistry.cpp:
// 46-47 @ v2.5.2).
static LEN: Function = method_def!("__len__", [SELF], "int", |py, o, a, _| {
    this_registry!(o, a);
    ok(py, num_builtins())
});

// .def("__getitem__", [](self, const std::string & style) {...})
// (PyBuiltinTransformRegistry.cpp:48-63 @ v2.5.2): the description of the style that equals
// `style` ignoring case; else `py::key_error` of "'<style>'", which pybind11 sets from the C
// string (up to a NUL).
static GETITEM: Function = method_def!(
    "__getitem__",
    [SELF, Param::new("arg0", "str")],
    "str",
    |py, o, a, convert| {
        this_registry!(o, a);
        let StdString(style) = crate::arg!(a, 1, convert, StdString);
        if let Some(i) = find(&style) {
            let description =
                py_result(py, BuiltinTransformRegistry::get().builtin_description(i))?;
            return string_out(py, description);
        }
        let mut message = [b"'".as_slice(), &style, b"'"].concat();
        if let Some(nul) = message.iter().position(|&b| b == 0) {
            message.truncate(nul);
        }
        let text = crate::dispatch::std_string_to_py(py, &message)?;
        Err(pyo3::exceptions::PyKeyError::new_err((text.unbind(),)))
    }
);

// .def("__contains__", [](self, const std::string & style) -> bool {...})
// (PyBuiltinTransformRegistry.cpp:64-76 @ v2.5.2).
static CONTAINS: Function = method_def!(
    "__contains__",
    [SELF, Param::new("arg0", "str")],
    "bool",
    |py, o, a, convert| {
        this_registry!(o, a);
        let StdString(style) = crate::arg!(a, 1, convert, StdString);
        ok(py, find(&style).is_some())
    }
);

// .def("getBuiltins", [](self) { return BuiltinIterator(self); })
// (PyBuiltinTransformRegistry.cpp:77-80 @ v2.5.2).
static GET_BUILTINS: Function =
    method_def!("getBuiltins", [SELF], BUILTIN_ITERATOR, |py, o, a, _| {
        this_registry!(o, a);
        Ok(Some(new_iterator(py, BUILTIN_ITERATOR, None)?.unbind()))
    });

// clsBuiltinStyleIterator (PyBuiltinTransformRegistry.cpp:82-94 @ v2.5.2).
static STYLE_LEN: Function = method_def!("__len__", [STYLE_SELF], "int", |py, o, a, _| {
    let _it = this_iter!(o, a);
    ok(py, num_builtins())
});

static STYLE_GETITEM: Function = method_def!(
    "__getitem__",
    [STYLE_SELF, Param::new("arg0", INT)],
    "str",
    |py, o, a, convert| {
        let _it = this_iter!(o, a);
        let i = crate::arg!(a, 1, convert, i32);
        string_out(py, style(py, i)?)
    }
);

static STYLE_ITER: Function =
    method_def!("__iter__", [STYLE_SELF], STYLE_ITERATOR, |_, o, a, _| {
        let _it = this_iter!(o, a);
        Ok(Some(a[0].clone().unbind()))
    });

static STYLE_NEXT: Function = method_def!("__next__", [STYLE_SELF], "str", |py, o, a, _| {
    let it = this_iter!(o, a);
    let i = it.get().next_index(num_builtins() as i32)?;
    string_out(py, style(py, i)?)
});

/// `py::make_tuple(getBuiltinStyle(i), getBuiltinDescription(i))`.
fn builtin_tuple(py: Python<'_>, i: i32) -> PyResult<Option<Py<PyAny>>> {
    let style = crate::dispatch::std_string_to_py(py, style(py, i)?)?;
    let description = crate::dispatch::std_string_to_py(py, description(py, i)?)?;
    Ok(Some(
        PyTuple::new(py, [style, description])?.into_any().unbind(),
    ))
}

// clsBuiltinIterator (PyBuiltinTransformRegistry.cpp:96-110 @ v2.5.2).
static BUILTIN_LEN: Function = method_def!("__len__", [BUILTIN_SELF], "int", |py, o, a, _| {
    let _it = this_iter!(o, a);
    ok(py, num_builtins())
});

static BUILTIN_GETITEM: Function = method_def!(
    "__getitem__",
    [BUILTIN_SELF, Param::new("arg0", INT)],
    "tuple",
    |py, o, a, convert| {
        let _it = this_iter!(o, a);
        let i = crate::arg!(a, 1, convert, i32);
        builtin_tuple(py, i)
    }
);

static BUILTIN_ITER: Function = method_def!(
    "__iter__",
    [BUILTIN_SELF],
    BUILTIN_ITERATOR,
    |_, o, a, _| {
        let _it = this_iter!(o, a);
        Ok(Some(a[0].clone().unbind()))
    }
);

static BUILTIN_NEXT: Function = method_def!("__next__", [BUILTIN_SELF], "tuple", |py, o, a, _| {
    let it = this_iter!(o, a);
    let i = it.get().next_index(num_builtins() as i32)?;
    builtin_tuple(py, i)
});

/// Port of `bindPyBuiltinTransformRegistry` (src/bindings/python/
/// PyBuiltinTransformRegistry.cpp:24-111 @ v2.5.2), in its order: the iterator classes nested
/// in the class (30-36), the class's methods (38-80), the iterators' (82-110).
pub(crate) fn bind_py_builtin_transform_registry(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let cls = class(py, REGISTRY)?;
    let style_iterator = nested_class(&cls, "BuiltinTransformRegistry", "BuiltinStyleIterator")?;
    let builtin_iterator = nested_class(&cls, "BuiltinTransformRegistry", "BuiltinIterator")?;
    for f in [&INIT, &ITER, &LEN, &GETITEM, &CONTAINS, &GET_BUILTINS] {
        def(&cls, f)?;
    }
    for f in [&STYLE_LEN, &STYLE_GETITEM, &STYLE_ITER, &STYLE_NEXT] {
        def(&style_iterator, f)?;
    }
    for f in [&BUILTIN_LEN, &BUILTIN_GETITEM, &BUILTIN_ITER, &BUILTIN_NEXT] {
        def(&builtin_iterator, f)?;
    }
    Ok(())
}
