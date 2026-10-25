// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyBuiltinConfigRegistry` (src/bindings/python/PyBuiltinConfigRegistry.cpp and
//! PyBuiltinConfigRegistry.h @ v2.5.2): a class over the library's global registry of built-in
//! configs, with its two iterators (chunk 6.3f).

use ocio::BuiltinConfigRegistry;
use ocio::builtinconfigs::builtin_config_registry::resolve_config_path;
use ocio_ops::utils::string_utils::{c_str, compare};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyTuple};

use crate::dispatch::{Function, Overload, Param, StdString, std_string_to_py};
use crate::py_open_color_io::py_result;
use crate::py_transform::{INT, class, def, nested_class, none, ok, string_out};
use crate::py_utils::new_iterator;
use crate::{method_def, q, this_iter};

const REGISTRY: &str = q!("BuiltinConfigRegistry");
const NAME_ITERATOR: &str = q!("BuiltinConfigRegistry.BuiltinConfigNameIterator");
const CONFIG_ITERATOR: &str = q!("BuiltinConfigRegistry.BuiltinConfigIterator");
const SELF: Param = Param::new("self", REGISTRY);
const NAME_SELF: Param = Param::new("self", NAME_ITERATOR);
const CONFIG_SELF: Param = Param::new("self", CONFIG_ITERATOR);

/// `self`, an instance of the registry class, or the next overload. Its C++ value
/// (`PyBuiltinConfigRegistry`, PyBuiltinConfigRegistry.h:13-48 @ v2.5.2) holds nothing: each
/// method reads the global registry (`BuiltinConfigRegistry::Get()`). An instance no
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

/// `getNumBuiltinConfigs()` (PyBuiltinConfigRegistry.h:19-22 @ v2.5.2).
fn num_builtin_configs() -> usize {
    BuiltinConfigRegistry::get().num_builtin_configs()
}

/// `getBuiltinConfigName(i)` of an `int` index, which C++ converts to `size_t` (a negative one
/// wraps to a huge one, which the registry refuses: "Config index is out of range.").
fn name(py: Python<'_>, i: i32) -> PyResult<&'static [u8]> {
    py_result(
        py,
        BuiltinConfigRegistry::get().builtin_config_name(i as usize),
    )
}

/// The tuple of the iterator of configs: the name, the name for user interfaces, whether the
/// config is recommended, and whether `ocio://<name>` is the default config (ignoring case).
///
/// Port of the `py::make_tuple(...)` of `clsBuiltinConfigIterator`'s `__getitem__` and
/// `__next__` (PyBuiltinConfigRegistry.cpp:68-87 @ v2.5.2).
fn config_tuple(py: Python<'_>, i: i32) -> PyResult<Option<Py<PyAny>>> {
    let registry = BuiltinConfigRegistry::get();
    let name = name(py, i)?;
    let ui_name = py_result(py, registry.builtin_config_ui_name(i as usize))?;
    let recommended = py_result(py, registry.is_builtin_config_recommended(i as usize))?;
    let is_default = compare(
        &[b"ocio://".as_slice(), name].concat(),
        resolve_config_path(b"ocio://default"),
    );
    let items: [Bound<'_, PyAny>; 4] = [
        std_string_to_py(py, name)?.into_any(),
        std_string_to_py(py, ui_name)?.into_any(),
        PyBool::new(py, recommended).to_owned().into_any(),
        PyBool::new(py, is_default).to_owned().into_any(),
    ];
    Ok(Some(PyTuple::new(py, items)?.into_any().unbind()))
}

// .def(py::init<>()) (PyBuiltinConfigRegistry.cpp:32 @ v2.5.2).
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

// .def("__iter__", [](self) { return BuiltinConfigNameIterator(self); })
// (PyBuiltinConfigRegistry.cpp:33-37 @ v2.5.2).
static ITER: Function = method_def!("__iter__", [SELF], NAME_ITERATOR, |py, o, a, _| {
    this_registry!(o, a);
    Ok(Some(new_iterator(py, NAME_ITERATOR, None)?.unbind()))
});

// .def("__len__", [](self) { return self.getNumBuiltinConfigs(); })
// (PyBuiltinConfigRegistry.cpp:38-42 @ v2.5.2).
static LEN: Function = method_def!("__len__", [SELF], "int", |py, o, a, _| {
    this_registry!(o, a);
    ok(py, num_builtin_configs())
});

// .def("__getitem__", [](self, const std::string & name) { return
// self.getBuiltinConfigByName(name.c_str()); }) (PyBuiltinConfigRegistry.cpp:43-47 @ v2.5.2).
static GETITEM: Function = method_def!(
    "__getitem__",
    [SELF, Param::new("arg0", "str")],
    "str",
    |py, o, a, convert| {
        this_registry!(o, a);
        let StdString(config_name) = crate::arg!(a, 1, convert, StdString);
        let config = py_result(
            py,
            BuiltinConfigRegistry::get().builtin_config_by_name(c_str(&config_name)),
        )?;
        string_out(py, config)
    }
);

// .def("__contains__", [](self, const std::string & name) {...})
// (PyBuiltinConfigRegistry.cpp:48-60 @ v2.5.2): whether a config's name equals `name` (the
// whole string) ignoring case.
static CONTAINS: Function = method_def!(
    "__contains__",
    [SELF, Param::new("arg0", "str")],
    "bool",
    |py, o, a, convert| {
        this_registry!(o, a);
        let StdString(config_name) = crate::arg!(a, 1, convert, StdString);
        let registry = BuiltinConfigRegistry::get();
        let found = (0..registry.num_builtin_configs()).any(|i| {
            registry
                .builtin_config_name(i)
                .is_ok_and(|n| compare(n, &config_name))
        });
        ok(py, found)
    }
);

// .def("getBuiltinConfigs", [](self) { return BuiltinConfigIterator(self); })
// (PyBuiltinConfigRegistry.cpp:61-64 @ v2.5.2).
static GET_BUILTIN_CONFIGS: Function = method_def!(
    "getBuiltinConfigs",
    [SELF],
    CONFIG_ITERATOR,
    |py, o, a, _| {
        this_registry!(o, a);
        Ok(Some(new_iterator(py, CONFIG_ITERATOR, None)?.unbind()))
    }
);

// clsBuiltinConfigIterator (PyBuiltinConfigRegistry.cpp:66-87 @ v2.5.2): `__getitem__` checks
// no index; the registry refuses one out of range.
static CONFIG_LEN: Function = method_def!("__len__", [CONFIG_SELF], "int", |py, o, a, _| {
    let _it = this_iter!(o, a);
    ok(py, num_builtin_configs())
});

static CONFIG_GETITEM: Function = method_def!(
    "__getitem__",
    [CONFIG_SELF, Param::new("arg0", INT)],
    "tuple",
    |py, o, a, convert| {
        let _it = this_iter!(o, a);
        let i = crate::arg!(a, 1, convert, i32);
        config_tuple(py, i)
    }
);

static CONFIG_ITER: Function =
    method_def!("__iter__", [CONFIG_SELF], CONFIG_ITERATOR, |_, o, a, _| {
        let _it = this_iter!(o, a);
        Ok(Some(a[0].clone().unbind()))
    });

static CONFIG_NEXT: Function = method_def!("__next__", [CONFIG_SELF], "tuple", |py, o, a, _| {
    let it = this_iter!(o, a);
    let i = it.get().next_index(num_builtin_configs() as i32)?;
    config_tuple(py, i)
});

// clsBuiltinConfigNameIterator (PyBuiltinConfigRegistry.cpp:89-100 @ v2.5.2): `__getitem__`
// checks no index either.
static NAME_LEN: Function = method_def!("__len__", [NAME_SELF], "int", |py, o, a, _| {
    let _it = this_iter!(o, a);
    ok(py, num_builtin_configs())
});

static NAME_GETITEM: Function = method_def!(
    "__getitem__",
    [NAME_SELF, Param::new("arg0", INT)],
    "str",
    |py, o, a, convert| {
        let _it = this_iter!(o, a);
        let i = crate::arg!(a, 1, convert, i32);
        string_out(py, name(py, i)?)
    }
);

static NAME_ITER: Function = method_def!("__iter__", [NAME_SELF], NAME_ITERATOR, |_, o, a, _| {
    let _it = this_iter!(o, a);
    Ok(Some(a[0].clone().unbind()))
});

static NAME_NEXT: Function = method_def!("__next__", [NAME_SELF], "str", |py, o, a, _| {
    let it = this_iter!(o, a);
    let i = it.get().next_index(num_builtin_configs() as i32)?;
    string_out(py, name(py, i)?)
});

/// Port of `bindPyBuiltinConfigRegistry` (src/bindings/python/PyBuiltinConfigRegistry.cpp:
/// 18-101 @ v2.5.2), in its order: the iterator classes nested in the class (23-29), the
/// class's methods (31-64), the config iterator's (66-87), the name iterator's (89-100).
pub(crate) fn bind_py_builtin_config_registry(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let cls = class(py, REGISTRY)?;
    let name_iterator = nested_class(&cls, "BuiltinConfigRegistry", "BuiltinConfigNameIterator")?;
    let config_iterator = nested_class(&cls, "BuiltinConfigRegistry", "BuiltinConfigIterator")?;
    for f in [
        &INIT,
        &ITER,
        &LEN,
        &GETITEM,
        &CONTAINS,
        &GET_BUILTIN_CONFIGS,
    ] {
        def(&cls, f)?;
    }
    for f in [&CONFIG_LEN, &CONFIG_GETITEM, &CONFIG_ITER, &CONFIG_NEXT] {
        def(&config_iterator, f)?;
    }
    for f in [&NAME_LEN, &NAME_GETITEM, &NAME_ITER, &NAME_NEXT] {
        def(&name_iterator, f)?;
    }
    Ok(())
}
