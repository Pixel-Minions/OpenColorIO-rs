// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyColorSpaceTransform` (src/bindings/python/transforms/PyColorSpaceTransform.cpp @
//! v2.5.2).

use ocio::{ColorSpaceTransform, Transform};
use ocio_ops::utils::string_utils::c_str;
use pyo3::prelude::*;

use crate::dispatch::{Function, NonNullCharPtr, Overload, Param, StdString};
use crate::py_transform::{
    DIRECTION_DEFAULT, TransformRc, class, construct, def, default, none, ok, set_direction,
    string_out, validate, with,
};
use crate::{enum_param, method_def, q, this_rc};

const COLOR_SPACE_TRANSFORM: &str = q!("ColorSpaceTransform");
const SELF: Param = Param::new("self", COLOR_SPACE_TRANSFORM);

/// A `const std::string &` parameter whose default is `DEFAULT->get...()` of an empty string.
pub(crate) const fn empty_string(name: &'static str) -> Param {
    Param {
        name,
        ty: "str",
        default: default("''", |py| Ok(pyo3::types::PyString::new(py, "").into_any())),
    }
}

/// A `bool` parameter whose default is `value`.
pub(crate) const fn bool_param(name: &'static str, value: bool) -> Param {
    Param {
        name,
        ty: "bool",
        default: if value {
            default("True", |py| {
                Ok(pyo3::types::PyBool::new(py, true).to_owned().into_any())
            })
        } else {
            default("False", |py| {
                Ok(pyo3::types::PyBool::new(py, false).to_owned().into_any())
            })
        },
    }
}

// .def(py::init(&ColorSpaceTransform::Create)), .def(py::init([](src, dst, dir, dataBypass)
// {...}), "src"_a = DEFAULT->getSrc(), "dst"_a = DEFAULT->getDst(), "direction"_a =
// DEFAULT->getDirection(), "dataBypass"_a = DEFAULT->getDataBypass())
// (PyColorSpaceTransform.cpp:17-37 @ v2.5.2).
static INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[SELF],
            ret: "None",
            call: |py, o, a, _| {
                let rc = TransformRc::new(Transform::ColorSpace(ColorSpaceTransform::new()));
                construct(&a[0], o.params[0].ty, rc)?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                empty_string("src"),
                empty_string("dst"),
                DIRECTION_DEFAULT,
                bool_param("dataBypass", true),
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let StdString(src) = crate::arg!(a, 1, convert, StdString);
                let StdString(dst) = crate::arg!(a, 2, convert, StdString);
                let dir = enum_param!(o, a, 3);
                let data_bypass = crate::arg!(a, 4, convert, bool);
                let mut t = ColorSpaceTransform::new();
                if !src.is_empty() {
                    t.set_src(c_str(&src));
                }
                if !dst.is_empty() {
                    t.set_dst(c_str(&dst));
                }
                let rc = TransformRc::new(Transform::ColorSpace(t));
                set_direction(&rc, dir);
                with(&rc, |t: &mut ColorSpaceTransform| {
                    t.set_data_bypass(data_bypass)
                })?;
                validate(py, &rc)?;
                construct(&a[0], o.params[0].ty, rc)?;
                none(py)
            },
            pos_only: 0,
        },
    ],
};

// .def("getSrc", ...), .def("setSrc", ..., "src"_a.none(false)) (PyColorSpaceTransform.cpp:
// 38-41 @ v2.5.2).
static GET_SRC: Function = method_def!("getSrc", [SELF], "str", |py, o, a, _| {
    let rc = this_rc!(o, a);
    let src = with(&rc, |t: &mut ColorSpaceTransform| t.src().to_vec())?;
    string_out(py, &src)
});

static SET_SRC: Function = method_def!(
    "setSrc",
    [SELF, Param::new("src", "str")],
    "None",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let NonNullCharPtr(src) = crate::arg!(a, 1, convert, NonNullCharPtr);
        with(&rc, |t: &mut ColorSpaceTransform| t.set_src(&src))?;
        none(py)
    }
);

// .def("getDst", ...), .def("setDst", ..., "dst"_a.none(false)) (PyColorSpaceTransform.cpp:
// 42-45 @ v2.5.2).
static GET_DST: Function = method_def!("getDst", [SELF], "str", |py, o, a, _| {
    let rc = this_rc!(o, a);
    let dst = with(&rc, |t: &mut ColorSpaceTransform| t.dst().to_vec())?;
    string_out(py, &dst)
});

static SET_DST: Function = method_def!(
    "setDst",
    [SELF, Param::new("dst", "str")],
    "None",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let NonNullCharPtr(dst) = crate::arg!(a, 1, convert, NonNullCharPtr);
        with(&rc, |t: &mut ColorSpaceTransform| t.set_dst(&dst))?;
        none(py)
    }
);

// .def("getDataBypass", ...), .def("setDataBypass", ..., "dataBypass"_a)
// (PyColorSpaceTransform.cpp:46-49 @ v2.5.2).
static GET_DATA_BYPASS: Function = method_def!("getDataBypass", [SELF], "bool", |py, o, a, _| {
    let rc = this_rc!(o, a);
    ok(
        py,
        with(&rc, |t: &mut ColorSpaceTransform| t.data_bypass())?,
    )
});

static SET_DATA_BYPASS: Function = method_def!(
    "setDataBypass",
    [SELF, Param::new("dataBypass", "bool")],
    "None",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let bypass = crate::arg!(a, 1, convert, bool);
        with(&rc, |t: &mut ColorSpaceTransform| t.set_data_bypass(bypass))?;
        none(py)
    }
);

static REPR: Function = crate::repr_def!(SELF);

/// Port of `bindPyColorSpaceTransform` (src/bindings/python/transforms/
/// PyColorSpaceTransform.cpp:9-52 @ v2.5.2), in its order.
pub(crate) fn bind(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let cls = class(m.py(), COLOR_SPACE_TRANSFORM)?;
    for f in [
        &INIT,
        &GET_SRC,
        &SET_SRC,
        &GET_DST,
        &SET_DST,
        &GET_DATA_BYPASS,
        &SET_DATA_BYPASS,
        &REPR,
    ] {
        def(&cls, f)?;
    }
    Ok(())
}
