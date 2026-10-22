// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyBuiltinTransform` (src/bindings/python/transforms/PyBuiltinTransform.cpp @ v2.5.2).

use ocio::{BuiltinTransform, Transform};
use ocio_ops::utils::string_utils::c_str;
use pyo3::prelude::*;

use crate::dispatch::{Function, NonNullCharPtr, Overload, Param, StdString};
use crate::py_open_color_io::py_result;
use crate::py_transform::{
    DIRECTION_DEFAULT, TransformRc, class, construct, def, default, none, set_direction,
    string_out, validate, with,
};
use crate::{enum_param, method_def, q, this_rc};

const BUILTIN_TRANSFORM: &str = q!("BuiltinTransform");
const SELF: Param = Param::new("self", BUILTIN_TRANSFORM);

// .def(py::init(&BuiltinTransform::Create)), .def(py::init([](style, dir) {...}), "style"_a =
// DEFAULT->getStyle(), "direction"_a = DEFAULT->getDirection()) (PyBuiltinTransform.cpp:17-30
// @ v2.5.2).
static INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    overloads: &[
        Overload {
            params: &[SELF],
            ret: "None",
            call: |py, o, a, _| {
                let rc = TransformRc::new(Transform::Builtin(BuiltinTransform::new()));
                construct(&a[0], o.params[0].ty, rc)?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param {
                    name: "style",
                    ty: "str",
                    default: default("'IDENTITY'", |py| {
                        Ok(pyo3::types::PyString::new(py, "IDENTITY").into_any())
                    }),
                },
                DIRECTION_DEFAULT,
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let StdString(style) = crate::arg!(a, 1, convert, StdString);
                let dir = enum_param!(o, a, 2);
                let mut t = BuiltinTransform::new();
                if !style.is_empty() {
                    py_result(py, t.set_style(c_str(&style)))?;
                }
                let rc = TransformRc::new(Transform::Builtin(t));
                set_direction(&rc, dir);
                validate(py, &rc)?;
                construct(&a[0], o.params[0].ty, rc)?;
                none(py)
            },
            pos_only: 0,
        },
    ],
};

// .def("setStyle", &BuiltinTransform::setStyle, "style"_a.none(false))
// (PyBuiltinTransform.cpp:31-32 @ v2.5.2).
static SET_STYLE: Function = method_def!(
    "setStyle",
    [SELF, Param::new("style", "str")],
    "None",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let NonNullCharPtr(style) = crate::arg!(a, 1, convert, NonNullCharPtr);
        py_result(
            py,
            with(&rc, |t: &mut BuiltinTransform| t.set_style(&style))?,
        )?;
        none(py)
    }
);

// .def("getStyle", &BuiltinTransform::getStyle) (PyBuiltinTransform.cpp:33-34 @ v2.5.2).
static GET_STYLE: Function = method_def!("getStyle", [SELF], "str", |py, o, a, _| {
    let rc = this_rc!(o, a);
    string_out(py, with(&rc, |t: &mut BuiltinTransform| t.style())?)
});

// .def("getDescription", &BuiltinTransform::getDescription) (PyBuiltinTransform.cpp:35-36 @
// v2.5.2).
static GET_DESCRIPTION: Function = method_def!("getDescription", [SELF], "str", |py, o, a, _| {
    let rc = this_rc!(o, a);
    string_out(py, with(&rc, |t: &mut BuiltinTransform| t.description())?)
});

static REPR: Function = crate::repr_def!(SELF);

/// Port of `bindPyBuiltinTransform` (src/bindings/python/transforms/PyBuiltinTransform.cpp:
/// 9-39 @ v2.5.2), in its order.
pub(crate) fn bind(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let cls = class(m.py(), BUILTIN_TRANSFORM)?;
    for f in [&INIT, &SET_STYLE, &GET_STYLE, &GET_DESCRIPTION, &REPR] {
        def(&cls, f)?;
    }
    Ok(())
}
