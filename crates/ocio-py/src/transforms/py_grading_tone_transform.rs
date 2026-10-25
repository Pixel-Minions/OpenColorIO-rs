// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyGradingToneTransform` (src/bindings/python/transforms/PyGradingToneTransform.cpp @
//! v2.5.2).

use ocio::{GradingToneTransform, Transform};
use ocio_ops::ops::gradingtone::grading_tone::GradingTone;
use pyo3::prelude::*;

use crate::dispatch::{Function, Overload, Param};
use crate::py_grading_data::{GRADING_STYLE, GRADING_TONE};
use crate::py_open_color_io::py_result;
use crate::py_transform::{TransformRc, class, def, enum_out, none, with};
use crate::shared::{Shared, holder, out};
use crate::transforms::py_grading_primary_transform::{
    DIR_DEFAULT, DYNAMIC_DEFAULT, STYLE_DEFAULT, finish, grading_style,
};
use crate::{enum_param, method_def, q, this_rc};

const GRADING_TONE_TRANSFORM: &str = q!("GradingToneTransform");
const SELF: Param = Param::new("self", GRADING_TONE_TRANSFORM);

/// A `GradingTone` argument (`const GradingTone &`: `None` refused), its value, or the next
/// overload.
macro_rules! tone_arg {
    ($o:expr, $a:expr, $i:expr) => {
        match holder::<GradingTone>(&$a[$i], $o.params[$i].ty)? {
            Some(rc) => *rc.lock(),
            None => return Ok(None),
        }
    };
}

// .def(py::init([](const GradingTone & values, GradingStyle style, bool dynamic,
// TransformDirection dir) {...}), "values"_a, "style"_a = DEFAULT->getStyle(), "dynamic"_a =
// DEFAULT->isDynamic(), "dir"_a = DEFAULT->getDirection()), .def(py::init([](GradingStyle
// style, bool dynamic, TransformDirection dir) {...}), ...) (PyGradingToneTransform.cpp:11,
// 17-50 @ v2.5.2).
static INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[
                SELF,
                Param::new("values", GRADING_TONE),
                STYLE_DEFAULT,
                DYNAMIC_DEFAULT,
                DIR_DEFAULT,
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let values = tone_arg!(o, a, 1);
                let style = enum_param!(o, a, 2);
                let dynamic = crate::arg!(a, 3, convert, bool);
                let dir = enum_param!(o, a, 4);
                // The wheel's transform validates the values (whatever its style) before the
                // port refuses a style outside the enumerators.
                let style = match grading_style(style) {
                    Ok(style) => style,
                    Err(e) => {
                        py_result(py, values.validate())?;
                        return Err(e);
                    }
                };
                let mut t = GradingToneTransform::new(style);
                t.set_style(style);
                py_result(py, t.set_value(&values))?;
                if dynamic {
                    t.make_dynamic();
                }
                let rc = TransformRc::new(Transform::GradingTone(t));
                finish(py, &a[0], o.params[0].ty, rc, dir)?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[SELF, STYLE_DEFAULT, DYNAMIC_DEFAULT, DIR_DEFAULT],
            ret: "None",
            call: |py, o, a, convert| {
                let style = enum_param!(o, a, 1);
                let dynamic = crate::arg!(a, 2, convert, bool);
                let dir = enum_param!(o, a, 3);
                let style = grading_style(style)?;
                let mut t = GradingToneTransform::new(style);
                t.set_style(style);
                if dynamic {
                    t.make_dynamic();
                }
                let rc = TransformRc::new(Transform::GradingTone(t));
                finish(py, &a[0], o.params[0].ty, rc, dir)?;
                none(py)
            },
            pos_only: 0,
        },
    ],
};

// .def("getFormatMetadata", ..., py::return_value_policy::reference_internal)
// (PyGradingToneTransform.cpp:52-56 @ v2.5.2).
static GET_FORMAT_METADATA: Function = crate::format_metadata_def!(SELF);

// .def("getStyle", &GradingToneTransform::getStyle) (PyGradingToneTransform.cpp:57-58 @
// v2.5.2).
static GET_STYLE: Function = method_def!("getStyle", [SELF], GRADING_STYLE, |py, o, a, _| {
    let rc = this_rc!(o, a);
    let style = with(&rc, |t: &mut GradingToneTransform| t.style())?;
    enum_out(py, GRADING_STYLE, style as i64)
});

// .def("setStyle", &GradingToneTransform::setStyle, "style"_a) (PyGradingToneTransform.cpp:
// 59-60 @ v2.5.2).
static SET_STYLE: Function = method_def!(
    "setStyle",
    [SELF, Param::new("style", GRADING_STYLE)],
    "None",
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        let style = grading_style(enum_param!(o, a, 1))?;
        with(&rc, |t: &mut GradingToneTransform| t.set_style(style))?;
        none(py)
    }
);

// .def("getValue", &GradingToneTransform::getValue) (PyGradingToneTransform.cpp:61-62 @
// v2.5.2): a `const GradingTone &`, which pybind11 copies into a new object.
static GET_VALUE: Function = method_def!("getValue", [SELF], GRADING_TONE, |py, o, a, _| {
    let rc = this_rc!(o, a);
    let value = with(&rc, |t: &mut GradingToneTransform| t.value())?;
    out(py, GRADING_TONE, &Shared::new(value))
});

// .def("setValue", &GradingToneTransform::setValue, "values"_a) (PyGradingToneTransform.cpp:
// 63-64 @ v2.5.2).
static SET_VALUE: Function = method_def!(
    "setValue",
    [SELF, Param::new("values", GRADING_TONE)],
    "None",
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        let values = tone_arg!(o, a, 1);
        py_result(
            py,
            with(&rc, |t: &mut GradingToneTransform| t.set_value(&values))?,
        )?;
        none(py)
    }
);

// .def("isDynamic", ...), .def("makeDynamic", ...), .def("makeNonDynamic", ...)
// (PyGradingToneTransform.cpp:65-70 @ v2.5.2).
crate::grading_dynamic_methods!(GradingToneTransform, SELF);

static REPR: Function = crate::repr_def!(SELF);

/// Port of `bindPyGradingToneTransform` (src/bindings/python/transforms/
/// PyGradingToneTransform.cpp:9-73 @ v2.5.2), in its order.
pub(crate) fn bind(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let cls = class(m.py(), GRADING_TONE_TRANSFORM)?;
    for f in [
        &INIT,
        &GET_FORMAT_METADATA,
        &GET_STYLE,
        &SET_STYLE,
        &GET_VALUE,
        &SET_VALUE,
        &IS_DYNAMIC,
        &MAKE_DYNAMIC,
        &MAKE_NON_DYNAMIC,
        &REPR,
    ] {
        def(&cls, f)?;
    }
    Ok(())
}
