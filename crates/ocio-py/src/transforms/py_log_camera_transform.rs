// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyLogCameraTransform` (src/bindings/python/transforms/PyLogCameraTransform.cpp @
//! v2.5.2).

use ocio::{LogCameraTransform, Transform};
use pyo3::prelude::*;

use crate::dispatch::{Function, Overload, Param};
use crate::py_open_color_io::{py_result, to_py_err};
use crate::py_transform::{
    DIRECTION_DEFAULT, FLOAT, TransformRc, class, construct, def, default, float_list, none, ok,
    set_direction, with,
};
use crate::{enum_param, float_array, float_array_out, method_def, q, ref_rc, this_rc};

const LOG_CAMERA_TRANSFORM: &str = q!("LogCameraTransform");
const SELF: Param = Param::new("self", LOG_CAMERA_TRANSFORM);
const ARRAY_3: &str = float_array!("3");

/// `std::numeric_limits<double>::quiet_NaN()`, which the getters return for a value not set.
const QUIET_NAN: f64 = f64::from_bits(0x7ff8_0000_0000_0000);

const fn array3(name: &'static str, ones: bool) -> Param {
    Param {
        name,
        ty: ARRAY_3,
        default: if ones {
            default("[1.0, 1.0, 1.0]", |py| float_list(py, &[1.0; 3]))
        } else {
            default("[0.0, 0.0, 0.0]", |py| float_list(py, &[0.0; 3]))
        },
    }
}

// .def(py::init([](linSideBreak, base, logSideSlope, logSideOffset, linSideSlope,
// linSideOffset, linearSlope, dir) {...}), "linSideBreak"_a, "base"_a = DEFAULT_BASE, ...,
// "linearSlope"_a = DEFAULT_EMPTY, "direction"_a = DEFAULT->getDirection())
// (PyLogCameraTransform.cpp:37-72 @ v2.5.2): no `validate`.
static INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    overloads: &[Overload {
        params: &[
            SELF,
            Param::new("linSideBreak", ARRAY_3),
            Param {
                name: "base",
                ty: FLOAT,
                default: default(
                    "2.0",
                    |py| Ok(pyo3::types::PyFloat::new(py, 2.0).into_any()),
                ),
            },
            array3("logSideSlope", true),
            array3("logSideOffset", false),
            array3("linSideSlope", true),
            array3("linSideOffset", false),
            Param {
                name: "linearSlope",
                ty: "collections.abc.Sequence[typing.SupportsFloat]",
                default: default("[]", |py| Ok(pyo3::types::PyList::empty(py).into_any())),
            },
            DIRECTION_DEFAULT,
        ],
        ret: "None",
        call: |py, o, a, convert| {
            let lin_side_break = crate::arg!(a, 1, convert, [f64; 3]);
            let base = crate::arg!(a, 2, convert, f64);
            let log_side_slope = crate::arg!(a, 3, convert, [f64; 3]);
            let log_side_offset = crate::arg!(a, 4, convert, [f64; 3]);
            let lin_side_slope = crate::arg!(a, 5, convert, [f64; 3]);
            let lin_side_offset = crate::arg!(a, 6, convert, [f64; 3]);
            let linear_slope = crate::arg!(a, 7, convert, Vec<f64>);
            let dir = enum_param!(o, a, 8);
            let mut t = LogCameraTransform::new(&lin_side_break);
            t.set_base(base);
            t.set_log_side_slope_value(&log_side_slope);
            t.set_log_side_offset_value(&log_side_offset);
            t.set_lin_side_slope_value(&lin_side_slope);
            t.set_lin_side_offset_value(&lin_side_offset);
            if !linear_slope.is_empty() {
                let Ok(values) = <[f64; 3]>::try_from(linear_slope.as_slice()) else {
                    return Err(to_py_err(
                        py,
                        &ocio::Exception::new("LinearSlope must be 3 doubles."),
                    ));
                };
                py_result(py, t.set_linear_slope_value(&values))?;
            }
            let rc = TransformRc::new(Transform::LogCamera(t));
            set_direction(&rc, dir);
            construct(&a[0], o.params[0].ty, rc)?;
            none(py)
        },
        pos_only: 0,
    }],
};

// .def("getFormatMetadata", ..., py::return_value_policy::reference_internal)
// (PyLogCameraTransform.cpp:74-77 @ v2.5.2).
static GET_FORMAT_METADATA: Function = crate::format_metadata_def!(SELF);

// .def("equals", &LogCameraTransform::equals, "other"_a) (PyLogCameraTransform.cpp:78-79 @
// v2.5.2).
static EQUALS: Function = method_def!(
    "equals",
    [SELF, Param::new("other", LOG_CAMERA_TRANSFORM)],
    "bool",
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        let other = ref_rc!(o, a, 1);
        let equal =
            crate::py_transform::equals::<LogCameraTransform>(&rc, &other, |x, y| x.equals(y))?;
        ok(py, equal)
    }
);

// .def("getBase", ...), .def("setBase", ..., "base"_a) (PyLogCameraTransform.cpp:80-83 @
// v2.5.2).
static GET_BASE: Function = method_def!("getBase", [SELF], "float", |py, o, a, _| {
    let rc = this_rc!(o, a);
    ok(py, with(&rc, |t: &mut LogCameraTransform| t.base())?)
});

static SET_BASE: Function = method_def!(
    "setBase",
    [SELF, Param::new("base", FLOAT)],
    "None",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let base = crate::arg!(a, 1, convert, f64);
        with(&rc, |t: &mut LogCameraTransform| t.set_base(base))?;
        none(py)
    }
);

// The values of three (PyLogCameraTransform.cpp:84-155 @ v2.5.2).
crate::array3_methods!(
    LogCameraTransform,
    SELF,
    GET_LOG_SIDE_SLOPE,
    SET_LOG_SIDE_SLOPE,
    "getLogSideSlopeValue",
    "setLogSideSlopeValue",
    log_side_slope_value,
    set_log_side_slope_value
);
crate::array3_methods!(
    LogCameraTransform,
    SELF,
    GET_LOG_SIDE_OFFSET,
    SET_LOG_SIDE_OFFSET,
    "getLogSideOffsetValue",
    "setLogSideOffsetValue",
    log_side_offset_value,
    set_log_side_offset_value
);
crate::array3_methods!(
    LogCameraTransform,
    SELF,
    GET_LIN_SIDE_SLOPE,
    SET_LIN_SIDE_SLOPE,
    "getLinSideSlopeValue",
    "setLinSideSlopeValue",
    lin_side_slope_value,
    set_lin_side_slope_value
);
crate::array3_methods!(
    LogCameraTransform,
    SELF,
    GET_LIN_SIDE_OFFSET,
    SET_LIN_SIDE_OFFSET,
    "getLinSideOffsetValue",
    "setLinSideOffsetValue",
    lin_side_offset_value,
    set_lin_side_offset_value
);
crate::array3_methods!(
    LogCameraTransform,
    SELF,
    GET_LIN_SIDE_BREAK,
    SET_LIN_SIDE_BREAK,
    "getLinSideBreakValue",
    "setLinSideBreakValue",
    lin_side_break_value,
    set_lin_side_break_value
);

// .def("getLinearSlopeValue", [](self) {...}, R"doc(...)doc") (PyLogCameraTransform.cpp:
// 156-166 @ v2.5.2): the slope, or three quiet NaNs when it isn't set.
static GET_LINEAR_SLOPE: Function = method_def!(
    "getLinearSlopeValue",
    [SELF],
    float_array_out!("3"),
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        let values = with(&rc, |t: &mut LogCameraTransform| t.linear_slope_value())?;
        ok(py, values.unwrap_or([QUIET_NAN; 3]).to_vec())
    }
);

/// The docstring `getLinearSlopeValue`'s definition gives (PyLogCameraTransform.cpp:163-165 @
/// v2.5.2).
const GET_LINEAR_SLOPE_DOC: &str = "\nReturn LinearSlope or 3 qnan values if not defined.\n";

// .def("setLinearSlopeValue", ..., "values"_a) (PyLogCameraTransform.cpp:167-173 @ v2.5.2).
static SET_LINEAR_SLOPE: Function = method_def!(
    "setLinearSlopeValue",
    [SELF, Param::new("values", ARRAY_3)],
    "None",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let values = crate::arg!(a, 1, convert, [f64; 3]);
        py_result(
            py,
            with(&rc, |t: &mut LogCameraTransform| {
                t.set_linear_slope_value(&values)
            })?,
        )?;
        none(py)
    }
);

// .def("isLinearSlopeValueSet", [](self) {...}) (PyLogCameraTransform.cpp:174-178 @ v2.5.2).
static IS_LINEAR_SLOPE_SET: Function =
    method_def!("isLinearSlopeValueSet", [SELF], "bool", |py, o, a, _| {
        let rc = this_rc!(o, a);
        let set = with(&rc, |t: &mut LogCameraTransform| {
            t.linear_slope_value().is_some()
        })?;
        ok(py, set)
    });

// .def("unsetLinearSlopeValue", &LogCameraTransform::unsetLinearSlopeValue)
// (PyLogCameraTransform.cpp:179-180 @ v2.5.2).
static UNSET_LINEAR_SLOPE: Function =
    method_def!("unsetLinearSlopeValue", [SELF], "None", |py, o, a, _| {
        let rc = this_rc!(o, a);
        with(&rc, |t: &mut LogCameraTransform| {
            t.unset_linear_slope_value()
        })?;
        none(py)
    });

static REPR: Function = crate::repr_def!(SELF);

/// Port of `bindPyLogCameraTransform` (src/bindings/python/transforms/
/// PyLogCameraTransform.cpp:14-183 @ v2.5.2), in its order.
pub(crate) fn bind(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let cls = class(py, LOG_CAMERA_TRANSFORM)?;
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
        &GET_LIN_SIDE_BREAK,
        &SET_LIN_SIDE_BREAK,
    ] {
        def(&cls, f)?;
    }
    cls.setattr(
        GET_LINEAR_SLOPE.name,
        crate::pybind11::method_with_doc(py, &GET_LINEAR_SLOPE, GET_LINEAR_SLOPE_DOC)?,
    )?;
    for f in [
        &SET_LINEAR_SLOPE,
        &IS_LINEAR_SLOPE_SET,
        &UNSET_LINEAR_SLOPE,
        &REPR,
    ] {
        def(&cls, f)?;
    }
    Ok(())
}
