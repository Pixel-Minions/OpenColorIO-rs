// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyLogAffineTransform` (src/bindings/python/transforms/PyLogAffineTransform.cpp @
//! v2.5.2).

use ocio::{LogAffineTransform, Transform};
use pyo3::prelude::*;

use crate::dispatch::{Function, Overload, Param};
use crate::py_transform::{
    DIRECTION_DEFAULT, FLOAT, TransformRc, class, construct, def, default, float_list, none, ok,
    set_direction, validate, with,
};
use crate::{enum_param, float_array, method_def, q, ref_rc, this_rc};

const LOG_AFFINE_TRANSFORM: &str = q!("LogAffineTransform");
const SELF: Param = Param::new("self", LOG_AFFINE_TRANSFORM);
const ARRAY_3: &str = float_array!("3");

/// `[1.0, 1.0, 1.0]`.
fn ones3(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    float_list(py, &[1.0; 3])
}

/// `[0.0, 0.0, 0.0]`.
fn zeros3(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    float_list(py, &[0.0; 3])
}

const fn array3(name: &'static str, ones: bool) -> Param {
    Param {
        name,
        ty: ARRAY_3,
        default: if ones {
            default("[1.0, 1.0, 1.0]", ones3)
        } else {
            default("[0.0, 0.0, 0.0]", zeros3)
        },
    }
}

// .def(py::init(&LogAffineTransform::Create)), .def(py::init([](logSideSlope, logSideOffset,
// linSideSlope, linSideOffset, dir) {...}), "logSideSlope"_a = DEFAULT_LOG_SIDE_SLOPE, ...,
// "direction"_a = DEFAULT->getDirection()) (PyLogAffineTransform.cpp:29-51 @ v2.5.2).
static INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    overloads: &[
        Overload {
            params: &[SELF],
            ret: "None",
            call: |py, o, a, _| {
                let rc = TransformRc::new(Transform::LogAffine(LogAffineTransform::new()));
                construct(&a[0], o.params[0].ty, rc)?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                array3("logSideSlope", true),
                array3("logSideOffset", false),
                array3("linSideSlope", true),
                array3("linSideOffset", false),
                DIRECTION_DEFAULT,
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let log_side_slope = crate::arg!(a, 1, convert, [f64; 3]);
                let log_side_offset = crate::arg!(a, 2, convert, [f64; 3]);
                let lin_side_slope = crate::arg!(a, 3, convert, [f64; 3]);
                let lin_side_offset = crate::arg!(a, 4, convert, [f64; 3]);
                let dir = enum_param!(o, a, 5);
                let mut t = LogAffineTransform::new();
                t.set_log_side_slope_value(&log_side_slope);
                t.set_log_side_offset_value(&log_side_offset);
                t.set_lin_side_slope_value(&lin_side_slope);
                t.set_lin_side_offset_value(&lin_side_offset);
                let rc = TransformRc::new(Transform::LogAffine(t));
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
// (PyLogAffineTransform.cpp:53-56 @ v2.5.2).
static GET_FORMAT_METADATA: Function = crate::format_metadata_def!(SELF);

// .def("equals", &LogAffineTransform::equals, "other"_a) (PyLogAffineTransform.cpp:57-58 @
// v2.5.2).
static EQUALS: Function = method_def!(
    "equals",
    [SELF, Param::new("other", LOG_AFFINE_TRANSFORM)],
    "bool",
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        let other = ref_rc!(o, a, 1);
        let equal =
            crate::py_transform::equals::<LogAffineTransform>(&rc, &other, |x, y| x.equals(y))?;
        ok(py, equal)
    }
);

// .def("getBase", ...), .def("setBase", ..., "base"_a) (PyLogAffineTransform.cpp:59-62 @
// v2.5.2).
static GET_BASE: Function = method_def!("getBase", [SELF], "float", |py, o, a, _| {
    let rc = this_rc!(o, a);
    ok(py, with(&rc, |t: &mut LogAffineTransform| t.base())?)
});

static SET_BASE: Function = method_def!(
    "setBase",
    [SELF, Param::new("base", FLOAT)],
    "None",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let base = crate::arg!(a, 1, convert, f64);
        with(&rc, |t: &mut LogAffineTransform| t.set_base(base))?;
        none(py)
    }
);

/// The getter and the setter ("values"_a) of a value of three of the class `$t` with `$self`.
#[macro_export]
macro_rules! array3_methods {
    ($t:ty, $self:expr, $get:ident, $set:ident, $get_name:literal, $set_name:literal,
     $getter:ident, $setter:ident) => {
        static $get: $crate::dispatch::Function = $crate::method_def!(
            $get_name,
            [$self],
            $crate::float_array_out!("3"),
            |py, o, a, _| {
                let rc = $crate::this_rc!(o, a);
                $crate::py_transform::ok(
                    py,
                    $crate::py_transform::with(&rc, |t: &mut $t| t.$getter())?.to_vec(),
                )
            }
        );
        static $set: $crate::dispatch::Function = $crate::method_def!(
            $set_name,
            [
                $self,
                $crate::dispatch::Param::new("values", $crate::float_array!("3"))
            ],
            "None",
            |py, o, a, convert| {
                let rc = $crate::this_rc!(o, a);
                let values = $crate::arg!(a, 1, convert, [f64; 3]);
                $crate::py_transform::with(&rc, |t: &mut $t| t.$setter(&values))?;
                $crate::py_transform::none(py)
            }
        );
    };
}

// The four values of three (PyLogAffineTransform.cpp:63-118 @ v2.5.2).
crate::array3_methods!(
    LogAffineTransform,
    SELF,
    GET_LOG_SIDE_SLOPE,
    SET_LOG_SIDE_SLOPE,
    "getLogSideSlopeValue",
    "setLogSideSlopeValue",
    log_side_slope_value,
    set_log_side_slope_value
);
crate::array3_methods!(
    LogAffineTransform,
    SELF,
    GET_LOG_SIDE_OFFSET,
    SET_LOG_SIDE_OFFSET,
    "getLogSideOffsetValue",
    "setLogSideOffsetValue",
    log_side_offset_value,
    set_log_side_offset_value
);
crate::array3_methods!(
    LogAffineTransform,
    SELF,
    GET_LIN_SIDE_SLOPE,
    SET_LIN_SIDE_SLOPE,
    "getLinSideSlopeValue",
    "setLinSideSlopeValue",
    lin_side_slope_value,
    set_lin_side_slope_value
);
crate::array3_methods!(
    LogAffineTransform,
    SELF,
    GET_LIN_SIDE_OFFSET,
    SET_LIN_SIDE_OFFSET,
    "getLinSideOffsetValue",
    "setLinSideOffsetValue",
    lin_side_offset_value,
    set_lin_side_offset_value
);

static REPR: Function = crate::repr_def!(SELF);

/// Port of `bindPyLogAffineTransform` (src/bindings/python/transforms/
/// PyLogAffineTransform.cpp:9-121 @ v2.5.2), in its order.
pub(crate) fn bind(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let cls = class(m.py(), LOG_AFFINE_TRANSFORM)?;
    for f in [
        &INIT,
        &GET_FORMAT_METADATA,
        &EQUALS,
        &GET_BASE,
        &SET_BASE,
        &GET_LOG_SIDE_SLOPE,
        &SET_LOG_SIDE_SLOPE,
        &GET_LOG_SIDE_OFFSET,
        &SET_LOG_SIDE_OFFSET,
        &GET_LIN_SIDE_SLOPE,
        &SET_LIN_SIDE_SLOPE,
        &GET_LIN_SIDE_OFFSET,
        &SET_LIN_SIDE_OFFSET,
        &REPR,
    ] {
        def(&cls, f)?;
    }
    Ok(())
}
