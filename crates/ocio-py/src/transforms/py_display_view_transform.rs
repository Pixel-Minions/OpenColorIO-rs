// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyDisplayViewTransform` (src/bindings/python/transforms/PyDisplayViewTransform.cpp @
//! v2.5.2).

use ocio::{DisplayViewTransform, Transform};
use ocio_ops::utils::string_utils::c_str;
use pyo3::prelude::*;

use crate::dispatch::{CharPtr, Function, Overload, Param, StdString};
use crate::py_transform::{
    DIRECTION_DEFAULT, TransformRc, class, construct, def, none, ok, set_direction, string_out,
    validate, with,
};
use crate::transforms::py_color_space_transform::{bool_param, empty_string};
use crate::{enum_param, method_def, q, this_rc};

const DISPLAY_VIEW_TRANSFORM: &str = q!("DisplayViewTransform");
const SELF: Param = Param::new("self", DISPLAY_VIEW_TRANSFORM);

// .def(py::init(&DisplayViewTransform::Create)), .def(py::init([](src, display, view,
// looksBypass, dataBypass, dir) {...}), "src"_a = DEFAULT->getSrc(), "display"_a =
// DEFAULT->getDisplay(), "view"_a = DEFAULT->getView(), "looksBypass"_a =
// DEFAULT->getLooksBypass(), "dataBypass"_a = DEFAULT->getDataBypass(), "direction"_a =
// DEFAULT->getDirection()) (PyDisplayViewTransform.cpp:17-43 @ v2.5.2).
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
                let rc = TransformRc::new(Transform::DisplayView(DisplayViewTransform::new()));
                construct(&a[0], o.params[0].ty, rc)?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                empty_string("src"),
                empty_string("display"),
                empty_string("view"),
                bool_param("looksBypass", false),
                bool_param("dataBypass", true),
                DIRECTION_DEFAULT,
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let StdString(src) = crate::arg!(a, 1, convert, StdString);
                let StdString(display) = crate::arg!(a, 2, convert, StdString);
                let StdString(view) = crate::arg!(a, 3, convert, StdString);
                let looks_bypass = crate::arg!(a, 4, convert, bool);
                let data_bypass = crate::arg!(a, 5, convert, bool);
                let dir = enum_param!(o, a, 6);
                let mut t = DisplayViewTransform::new();
                if !src.is_empty() {
                    t.set_src(c_str(&src));
                }
                if !display.is_empty() {
                    t.set_display(c_str(&display));
                }
                if !view.is_empty() {
                    t.set_view(c_str(&view));
                }
                t.set_looks_bypass(looks_bypass);
                t.set_data_bypass(data_bypass);
                let rc = TransformRc::new(Transform::DisplayView(t));
                set_direction(&rc, dir);
                validate(py, &rc)?;
                construct(&a[0], o.params[0].ty, rc)?;
                none(py)
            },
            pos_only: 0,
        },
    ],
};

/// A `const char *` setter's argument (`None` a null pointer with conversions, which the
/// setters store as the empty string: `name ? name : ""`, DisplayViewTransform.cpp:104-127 @
/// v2.5.2).
macro_rules! text_setter {
    ($name:literal, $param:literal, $set:ident) => {
        method_def!(
            $name,
            [SELF, Param::new($param, "str")],
            "None",
            |py, o, a, convert| {
                let rc = this_rc!(o, a);
                let CharPtr(text) = crate::arg!(a, 1, convert, CharPtr);
                let text = text.unwrap_or_default();
                with(&rc, |t: &mut DisplayViewTransform| t.$set(&text))?;
                none(py)
            }
        )
    };
}

// .def("getSrc", ...), .def("setSrc", ..., "src"_a) (PyDisplayViewTransform.cpp:44-47 @
// v2.5.2).
static GET_SRC: Function = method_def!("getSrc", [SELF], "str", |py, o, a, _| {
    let rc = this_rc!(o, a);
    let src = with(&rc, |t: &mut DisplayViewTransform| t.src().to_vec())?;
    string_out(py, &src)
});

static SET_SRC: Function = text_setter!("setSrc", "src", set_src);

// .def("getDisplay", ...), .def("setDisplay", ..., "display"_a) (PyDisplayViewTransform.cpp:
// 48-51 @ v2.5.2).
static GET_DISPLAY: Function = method_def!("getDisplay", [SELF], "str", |py, o, a, _| {
    let rc = this_rc!(o, a);
    let display = with(&rc, |t: &mut DisplayViewTransform| t.display().to_vec())?;
    string_out(py, &display)
});

static SET_DISPLAY: Function = text_setter!("setDisplay", "display", set_display);

// .def("getView", ...), .def("setView", ..., "view"_a) (PyDisplayViewTransform.cpp:52-55 @
// v2.5.2).
static GET_VIEW: Function = method_def!("getView", [SELF], "str", |py, o, a, _| {
    let rc = this_rc!(o, a);
    let view = with(&rc, |t: &mut DisplayViewTransform| t.view().to_vec())?;
    string_out(py, &view)
});

static SET_VIEW: Function = text_setter!("setView", "view", set_view);

// .def("getLooksBypass", ...), .def("setLooksBypass", ..., "looksBypass"_a)
// (PyDisplayViewTransform.cpp:56-59 @ v2.5.2).
static GET_LOOKS_BYPASS: Function = method_def!("getLooksBypass", [SELF], "bool", |py, o, a, _| {
    let rc = this_rc!(o, a);
    ok(
        py,
        with(&rc, |t: &mut DisplayViewTransform| t.looks_bypass())?,
    )
});

static SET_LOOKS_BYPASS: Function = method_def!(
    "setLooksBypass",
    [SELF, Param::new("looksBypass", "bool")],
    "None",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let bypass = crate::arg!(a, 1, convert, bool);
        with(&rc, |t: &mut DisplayViewTransform| {
            t.set_looks_bypass(bypass)
        })?;
        none(py)
    }
);

// .def("getDataBypass", ...), .def("setDataBypass", ..., "dataBypass"_a)
// (PyDisplayViewTransform.cpp:60-63 @ v2.5.2).
static GET_DATA_BYPASS: Function = method_def!("getDataBypass", [SELF], "bool", |py, o, a, _| {
    let rc = this_rc!(o, a);
    ok(
        py,
        with(&rc, |t: &mut DisplayViewTransform| t.data_bypass())?,
    )
});

static SET_DATA_BYPASS: Function = method_def!(
    "setDataBypass",
    [SELF, Param::new("dataBypass", "bool")],
    "None",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let bypass = crate::arg!(a, 1, convert, bool);
        with(&rc, |t: &mut DisplayViewTransform| {
            t.set_data_bypass(bypass)
        })?;
        none(py)
    }
);

static REPR: Function = crate::repr_def!(SELF);

/// Port of `bindPyDisplayViewTransform` (src/bindings/python/transforms/
/// PyDisplayViewTransform.cpp:9-66 @ v2.5.2), in its order.
pub(crate) fn bind(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let cls = class(m.py(), DISPLAY_VIEW_TRANSFORM)?;
    for f in [
        &INIT,
        &GET_SRC,
        &SET_SRC,
        &GET_DISPLAY,
        &SET_DISPLAY,
        &GET_VIEW,
        &SET_VIEW,
        &GET_LOOKS_BYPASS,
        &SET_LOOKS_BYPASS,
        &GET_DATA_BYPASS,
        &SET_DATA_BYPASS,
        &REPR,
    ] {
        def(&cls, f)?;
    }
    Ok(())
}
