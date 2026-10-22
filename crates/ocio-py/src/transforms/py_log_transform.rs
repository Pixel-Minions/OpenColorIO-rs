// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyLogTransform` (src/bindings/python/transforms/PyLogTransform.cpp @ v2.5.2).

use ocio::{LogTransform, Transform};
use pyo3::prelude::*;

use crate::dispatch::{Function, Overload, Param};
use crate::py_transform::{
    DIRECTION_DEFAULT, FLOAT, TransformRc, class, construct, def, default, none, ok, set_direction,
    validate, with,
};
use crate::{enum_param, method_def, q, ref_rc, this_rc};

const LOG_TRANSFORM: &str = q!("LogTransform");
const SELF: Param = Param::new("self", LOG_TRANSFORM);

// .def(py::init(&LogTransform::Create)), .def(py::init([](base, dir) {...}), "base"_a =
// DEFAULT->getBase(), "direction"_a = DEFAULT->getDirection()) (PyLogTransform.cpp:17-30 @
// v2.5.2).
static INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    overloads: &[
        Overload {
            params: &[SELF],
            ret: "None",
            call: |py, o, a, _| {
                let rc = TransformRc::new(Transform::Log(LogTransform::new()));
                construct(&a[0], o.params[0].ty, rc)?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param {
                    name: "base",
                    ty: FLOAT,
                    default: default(
                        "2.0",
                        |py| Ok(pyo3::types::PyFloat::new(py, 2.0).into_any()),
                    ),
                },
                DIRECTION_DEFAULT,
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let base = crate::arg!(a, 1, convert, f64);
                let dir = enum_param!(o, a, 2);
                let mut t = LogTransform::new();
                t.set_base(base);
                let rc = TransformRc::new(Transform::Log(t));
                set_direction(&rc, dir);
                validate(py, &rc)?;
                construct(&a[0], o.params[0].ty, rc)?;
                none(py)
            },
            pos_only: 0,
        },
    ],
};

// .def("getFormatMetadata", ..., py::return_value_policy::reference_internal)
// (PyLogTransform.cpp:32-35 @ v2.5.2).
static GET_FORMAT_METADATA: Function = crate::format_metadata_def!(SELF);

// .def("equals", &LogTransform::equals, "other"_a) (PyLogTransform.cpp:36-37 @ v2.5.2).
static EQUALS: Function = method_def!(
    "equals",
    [SELF, Param::new("other", LOG_TRANSFORM)],
    "bool",
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        let other = ref_rc!(o, a, 1);
        let equal = crate::py_transform::equals::<LogTransform>(&rc, &other, |x, y| x.equals(y))?;
        ok(py, equal)
    }
);

// .def("getBase", &LogTransform::getBase) (PyLogTransform.cpp:38-39 @ v2.5.2).
static GET_BASE: Function = method_def!("getBase", [SELF], "float", |py, o, a, _| {
    let rc = this_rc!(o, a);
    ok(py, with(&rc, |t: &mut LogTransform| t.base())?)
});

// .def("setBase", &LogTransform::setBase, "base"_a) (PyLogTransform.cpp:40-41 @ v2.5.2).
static SET_BASE: Function = method_def!(
    "setBase",
    [SELF, Param::new("base", FLOAT)],
    "None",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let base = crate::arg!(a, 1, convert, f64);
        with(&rc, |t: &mut LogTransform| t.set_base(base))?;
        none(py)
    }
);

static REPR: Function = crate::repr_def!(SELF);

/// Port of `bindPyLogTransform` (src/bindings/python/transforms/PyLogTransform.cpp:9-44 @
/// v2.5.2), in its order.
pub(crate) fn bind(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let cls = class(m.py(), LOG_TRANSFORM)?;
    for f in [
        &INIT,
        &GET_FORMAT_METADATA,
        &EQUALS,
        &GET_BASE,
        &SET_BASE,
        &REPR,
    ] {
        def(&cls, f)?;
    }
    Ok(())
}
