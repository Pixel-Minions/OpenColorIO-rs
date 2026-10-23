// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyExponentWithLinearTransform`
//! (src/bindings/python/transforms/PyExponentWithLinearTransform.cpp @ v2.5.2).

use ocio::{ExponentWithLinearTransform, Transform};
use pyo3::prelude::*;

use crate::dispatch::{Function, Overload, Param};
use crate::py_open_color_io::py_result;
use crate::py_transform::{
    DIRECTION_DEFAULT, NEGATIVE_STYLE, TransformRc, class, construct, def, default, enum_out,
    float_list, negative_style, none, ok, set_direction, validate, with,
};
use crate::{enum_param, float_array, float_array_out, method_def, q, ref_rc, this_rc};

const EXPONENT_WITH_LINEAR_TRANSFORM: &str = q!("ExponentWithLinearTransform");
const SELF: Param = Param::new("self", EXPONENT_WITH_LINEAR_TRANSFORM);
const ARRAY_4: &str = float_array!("4");

type Ewl = ExponentWithLinearTransform;

// .def(py::init(&ExponentWithLinearTransform::Create)), .def(py::init([](gamma, offset,
// negativeStyle, dir) {...}), "gamma"_a = DEFAULT_GAMMA, "offset"_a = DEFAULT_OFFSET,
// "negativeStyle"_a = DEFAULT->getNegativeStyle(), "direction"_a = DEFAULT->getDirection())
// (PyExponentWithLinearTransform.cpp:23-43 @ v2.5.2).
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
                let rc = TransformRc::new(Transform::ExponentWithLinear(Ewl::new()));
                construct(&a[0], o.params[0].ty, rc)?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param {
                    name: "gamma",
                    ty: ARRAY_4,
                    default: default("[1.0, 1.0, 1.0, 1.0]", |py| float_list(py, &[1.0; 4])),
                },
                Param {
                    name: "offset",
                    ty: ARRAY_4,
                    default: default("[0.0, 0.0, 0.0, 0.0]", |py| float_list(py, &[0.0; 4])),
                },
                Param {
                    name: "negativeStyle",
                    ty: NEGATIVE_STYLE,
                    default: default("<NegativeStyle.NEGATIVE_LINEAR: 3>", |py| {
                        crate::enums::to_py(&crate::enums::enum_class(py, NEGATIVE_STYLE)?, 3)
                    }),
                },
                DIRECTION_DEFAULT,
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let gamma = crate::arg!(a, 1, convert, [f64; 4]);
                let offset = crate::arg!(a, 2, convert, [f64; 4]);
                let style = enum_param!(o, a, 3);
                let dir = enum_param!(o, a, 4);
                let mut t = Ewl::new();
                t.set_gamma(&gamma);
                t.set_offset(&offset);
                py_result(py, t.set_negative_style(negative_style(py, style)?))?;
                let rc = TransformRc::new(Transform::ExponentWithLinear(t));
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
// (PyExponentWithLinearTransform.cpp:44-48 @ v2.5.2).
static GET_FORMAT_METADATA: Function = crate::format_metadata_def!(SELF);

// .def("equals", &ExponentWithLinearTransform::equals, "other"_a)
// (PyExponentWithLinearTransform.cpp:49-50 @ v2.5.2).
static EQUALS: Function = method_def!(
    "equals",
    [SELF, Param::new("other", EXPONENT_WITH_LINEAR_TRANSFORM)],
    "bool",
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        let other = ref_rc!(o, a, 1);
        let equal = crate::py_transform::equals::<Ewl>(&rc, &other, |x, y| x.equals(y))?;
        ok(py, equal)
    }
);

// .def("getGamma", ...), .def("setGamma", ..., "values"_a), .def("getOffset", ...),
// .def("setOffset", ..., "values"_a) (PyExponentWithLinearTransform.cpp:51-78 @ v2.5.2).
static GET_GAMMA: Function =
    method_def!("getGamma", [SELF], float_array_out!("4"), |py, o, a, _| {
        let rc = this_rc!(o, a);
        ok(py, with(&rc, |t: &mut Ewl| t.gamma())?.to_vec())
    });

static SET_GAMMA: Function = method_def!(
    "setGamma",
    [SELF, Param::new("values", ARRAY_4)],
    "None",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let values = crate::arg!(a, 1, convert, [f64; 4]);
        with(&rc, |t: &mut Ewl| t.set_gamma(&values))?;
        none(py)
    }
);

static GET_OFFSET: Function =
    method_def!("getOffset", [SELF], float_array_out!("4"), |py, o, a, _| {
        let rc = this_rc!(o, a);
        ok(py, with(&rc, |t: &mut Ewl| t.offset())?.to_vec())
    });

static SET_OFFSET: Function = method_def!(
    "setOffset",
    [SELF, Param::new("values", ARRAY_4)],
    "None",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let values = crate::arg!(a, 1, convert, [f64; 4]);
        with(&rc, |t: &mut Ewl| t.set_offset(&values))?;
        none(py)
    }
);

// .def("getNegativeStyle", ...), .def("setNegativeStyle", ..., "style"_a)
// (PyExponentWithLinearTransform.cpp:79-82 @ v2.5.2).
static GET_NEGATIVE_STYLE: Function =
    method_def!("getNegativeStyle", [SELF], NEGATIVE_STYLE, |py, o, a, _| {
        let rc = this_rc!(o, a);
        let style = with(&rc, |t: &mut Ewl| t.negative_style())?;
        enum_out(py, NEGATIVE_STYLE, style as i64)
    });

static SET_NEGATIVE_STYLE: Function = method_def!(
    "setNegativeStyle",
    [SELF, Param::new("style", NEGATIVE_STYLE)],
    "None",
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        let style = negative_style(py, enum_param!(o, a, 1))?;
        py_result(py, with(&rc, |t: &mut Ewl| t.set_negative_style(style))?)?;
        none(py)
    }
);

static REPR: Function = crate::repr_def!(SELF);

/// Port of `bindPyExponentWithLinearTransform`
/// (src/bindings/python/transforms/PyExponentWithLinearTransform.cpp:9-85 @ v2.5.2), in its
/// order.
pub(crate) fn bind(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let cls = class(m.py(), EXPONENT_WITH_LINEAR_TRANSFORM)?;
    for f in [
        &INIT,
        &GET_FORMAT_METADATA,
        &EQUALS,
        &GET_GAMMA,
        &SET_GAMMA,
        &GET_OFFSET,
        &SET_OFFSET,
        &GET_NEGATIVE_STYLE,
        &SET_NEGATIVE_STYLE,
        &REPR,
    ] {
        def(&cls, f)?;
    }
    Ok(())
}
