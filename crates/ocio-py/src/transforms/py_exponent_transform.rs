// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyExponentTransform` (src/bindings/python/transforms/PyExponentTransform.cpp @ v2.5.2).

use ocio::{ExponentTransform, Transform};
use pyo3::prelude::*;

use crate::dispatch::{Function, Overload, Param};
use crate::py_open_color_io::py_result;
use crate::py_transform::{
    DIRECTION_DEFAULT, NEGATIVE_STYLE, TransformRc, class, construct, def, default, enum_out,
    float_list, negative_style, none, ok, set_direction, validate, with,
};
use crate::{enum_param, float_array, float_array_out, method_def, q, ref_rc, this_rc};

const EXPONENT_TRANSFORM: &str = q!("ExponentTransform");
const SELF: Param = Param::new("self", EXPONENT_TRANSFORM);
const ARRAY_4: &str = float_array!("4");

// .def(py::init(&ExponentTransform::Create)), .def(py::init([](vec4, negativeStyle, dir) {...}),
// "value"_a = DEFAULT_VALUE, "negativeStyle"_a = DEFAULT->getNegativeStyle(), "direction"_a =
// DEFAULT->getDirection()) (PyExponentTransform.cpp:20-36 @ v2.5.2).
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
                let rc = TransformRc::new(Transform::Exponent(ExponentTransform::new()));
                construct(&a[0], o.params[0].ty, rc)?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param {
                    name: "value",
                    ty: ARRAY_4,
                    default: default("[1.0, 1.0, 1.0, 1.0]", |py| float_list(py, &[1.0; 4])),
                },
                Param {
                    name: "negativeStyle",
                    ty: NEGATIVE_STYLE,
                    default: default("<NegativeStyle.NEGATIVE_CLAMP: 0>", |py| {
                        crate::enums::to_py(&crate::enums::enum_class(py, NEGATIVE_STYLE)?, 0)
                    }),
                },
                DIRECTION_DEFAULT,
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let vec4 = crate::arg!(a, 1, convert, [f64; 4]);
                let style = enum_param!(o, a, 2);
                let dir = enum_param!(o, a, 3);
                let mut t = ExponentTransform::new();
                t.set_value(&vec4);
                py_result(py, t.set_negative_style(negative_style(py, style)?))?;
                let rc = TransformRc::new(Transform::Exponent(t));
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
// (PyExponentTransform.cpp:38-41 @ v2.5.2).
static GET_FORMAT_METADATA: Function = crate::format_metadata_def!(SELF);

// .def("equals", &ExponentTransform::equals, "other"_a) (PyExponentTransform.cpp:42-43 @ v2.5.2).
static EQUALS: Function = method_def!(
    "equals",
    [SELF, Param::new("other", EXPONENT_TRANSFORM)],
    "bool",
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        let other = ref_rc!(o, a, 1);
        let equal =
            crate::py_transform::equals::<ExponentTransform>(&rc, &other, |x, y| x.equals(y))?;
        ok(py, equal)
    }
);

// .def("getValue", ...) (PyExponentTransform.cpp:44-50 @ v2.5.2).
static GET_VALUE: Function =
    method_def!("getValue", [SELF], float_array_out!("4"), |py, o, a, _| {
        let rc = this_rc!(o, a);
        ok(
            py,
            with(&rc, |t: &mut ExponentTransform| t.value())?.to_vec(),
        )
    });

// .def("setValue", ..., "value"_a) (PyExponentTransform.cpp:51-56 @ v2.5.2).
static SET_VALUE: Function = method_def!(
    "setValue",
    [SELF, Param::new("value", ARRAY_4)],
    "None",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let vec4 = crate::arg!(a, 1, convert, [f64; 4]);
        with(&rc, |t: &mut ExponentTransform| t.set_value(&vec4))?;
        none(py)
    }
);

// .def("getNegativeStyle", &ExponentTransform::getNegativeStyle) (PyExponentTransform.cpp:57-58
// @ v2.5.2).
static GET_NEGATIVE_STYLE: Function =
    method_def!("getNegativeStyle", [SELF], NEGATIVE_STYLE, |py, o, a, _| {
        let rc = this_rc!(o, a);
        let style = with(&rc, |t: &mut ExponentTransform| t.negative_style())?;
        enum_out(py, NEGATIVE_STYLE, style as i64)
    });

// .def("setNegativeStyle", &ExponentTransform::setNegativeStyle, "style"_a)
// (PyExponentTransform.cpp:59-60 @ v2.5.2).
static SET_NEGATIVE_STYLE: Function = method_def!(
    "setNegativeStyle",
    [SELF, Param::new("style", NEGATIVE_STYLE)],
    "None",
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        let style = negative_style(py, enum_param!(o, a, 1))?;
        py_result(
            py,
            with(&rc, |t: &mut ExponentTransform| t.set_negative_style(style))?,
        )?;
        none(py)
    }
);

static REPR: Function = crate::repr_def!(SELF);

/// Port of `bindPyExponentTransform` (src/bindings/python/transforms/PyExponentTransform.cpp:
/// 9-63 @ v2.5.2), in its order.
pub(crate) fn bind(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let cls = class(m.py(), EXPONENT_TRANSFORM)?;
    for f in [
        &INIT,
        &GET_FORMAT_METADATA,
        &EQUALS,
        &GET_VALUE,
        &SET_VALUE,
        &GET_NEGATIVE_STYLE,
        &SET_NEGATIVE_STYLE,
        &REPR,
    ] {
        def(&cls, f)?;
    }
    Ok(())
}
