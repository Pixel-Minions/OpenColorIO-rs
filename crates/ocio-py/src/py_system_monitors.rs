// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPySystemMonitors` (src/bindings/python/PySystemMonitors.cpp/.h @ v2.5.2):
//! a value wrapper over the library's process singleton, and its tuple iterator.

use ocio::SystemMonitors;
use pyo3::prelude::*;
use pyo3::types::PyTuple;

use crate::dispatch::{Function, Overload, Param, char_ptr_to_py};
use crate::py_open_color_io::py_result;
use crate::py_transform::{INT, Out, class, def, nested_class, none, ok};
use crate::py_utils::new_iterator;
use crate::{arg, method_def, q, this_iter};

const MONITORS: &str = q!("SystemMonitors");
const ITERATOR: &str = q!("SystemMonitors.MonitorIterator");
const SELF: Param = Param::new("self", MONITORS);
const ITER_SELF: Param = Param::new("self", ITERATOR);

/// The wrapper holds no monitor list: each getter calls SystemMonitors::Get(). Reading its
/// value still rejects an instance made with __new__ alone under the existing U-86 policy.
macro_rules! this_monitors {
    ($o:expr, $a:expr) => {
        match crate::pybind11::instance_of(&$a[0], $o.params[0].ty) {
            Some(this) => {
                crate::pybind11::read_value(&this, $o.params[0].ty)?;
            }
            None => return Ok(None),
        }
    };
}

/// PySystemMonitors calls the singleton lazily (PySystemMonitors.h:19-32 @ v2.5.2).
fn num_monitors() -> usize {
    SystemMonitors::get().num_monitors()
}

/// The iterator passes int to size_t without checking it: a negative index wraps, then
/// SystemMonitors reports its own Exception rather than PyIterator's IndexError.
/// Port of PySystemMonitors.cpp:43-48,50-57 @ v2.5.2.
fn monitor_tuple(py: Python<'_>, index: i32) -> Out {
    let monitors = SystemMonitors::get();
    let name = py_result(py, monitors.monitor_name(index as usize))?;
    let profile = py_result(py, monitors.profile_filepath(index as usize))?;
    let items = [
        char_ptr_to_py(py, Some(name))?,
        char_ptr_to_py(py, Some(profile))?,
    ];
    Ok(Some(PyTuple::new(py, items)?.into_any().unbind()))
}

// .def(py::init<>(), DOC(SystemMonitors, SystemMonitors)) (PySystemMonitors.cpp:33-34).
// The wheel's DOC macro expands to empty text, as for the other declared classes.
static INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    operator: false,
    overloads: &[Overload {
        params: &[SELF],
        ret: "None",
        pos_only: 0,
        call: |py, o, a, _| {
            crate::pybind11::set_value(&a[0], o.params[0].ty, py.None().into_bound(py))?;
            none(py)
        },
    }],
};

// .def("getMonitors", ... return MonitorIterator(self)) (PySystemMonitors.cpp:36-39).
// PyIterator stores a copy of the empty wrapper, not the Python parent's lifetime.
static GET_MONITORS: Function = method_def!("getMonitors", [SELF], ITERATOR, |py, o, a, _| {
    this_monitors!(o, a);
    Ok(Some(new_iterator(py, ITERATOR, None)?.unbind()))
});

// The iterator's four methods, in source order (PySystemMonitors.cpp:41-57 @ v2.5.2).
static LEN: Function = method_def!("__len__", [ITER_SELF], "int", |py, o, a, _| {
    let _it = this_iter!(o, a);
    ok(py, num_monitors())
});

static GETITEM: Function = method_def!(
    "__getitem__",
    [ITER_SELF, Param::new("arg0", INT)],
    "tuple",
    |py, o, a, convert| {
        let _it = this_iter!(o, a);
        let index = arg!(a, 1, convert, i32);
        monitor_tuple(py, index)
    }
);

static ITER: Function = method_def!("__iter__", [ITER_SELF], ITERATOR, |_, o, a, _| {
    let _it = this_iter!(o, a);
    Ok(Some(a[0].clone().unbind()))
});

static NEXT: Function = method_def!("__next__", [ITER_SELF], "tuple", |py, o, a, _| {
    let it = this_iter!(o, a);
    let index = it.get().next_index(num_monitors() as i32)?;
    monitor_tuple(py, index)
});

/// Port of bindPySystemMonitors (PySystemMonitors.cpp:22-58 @ v2.5.2), in its order:
/// the nested iterator class, the wrapper's constructor/getMonitors, the iterator methods.
pub(crate) fn bind_py_system_monitors(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let cls = class(m.py(), MONITORS)?;
    let iterator = nested_class(&cls, "SystemMonitors", "MonitorIterator")?;
    for f in [&INIT, &GET_MONITORS] {
        def(&cls, f)?;
    }
    for f in [&LEN, &GETITEM, &ITER, &NEXT] {
        def(&iterator, f)?;
    }
    Ok(())
}
