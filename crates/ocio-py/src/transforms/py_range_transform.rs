// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyRangeTransform` (src/bindings/python/transforms/PyRangeTransform.cpp @ v2.5.2).

use ocio::Transform;
use ocio::transforms::range_transform::{RangeStyle, RangeTransform};
use pyo3::prelude::*;

use crate::dispatch::{Function, Overload, Param};
use crate::py_transform::{
    DIRECTION_DEFAULT, FLOAT, TransformRc, class, construct, def, default, enum_out, none, ok,
    outside_enumerators, set_direction, validate, with,
};
use crate::{enum_param, method_def, q, ref_rc, this_rc};

const RANGE_TRANSFORM: &str = q!("RangeTransform");
const RANGE_STYLE: &str = q!("RangeStyle");
const SELF: Param = Param::new("self", RANGE_TRANSFORM);

/// `nan`, the default of each value (`DEFAULT->getMinInValue()` and the others: unset).
fn nan(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    Ok(pyo3::types::PyFloat::new(py, f64::NAN).into_any())
}

const fn value_default(name: &'static str) -> Param {
    Param {
        name,
        ty: FLOAT,
        default: default("nan", nan),
    }
}

// .def(py::init(&RangeTransform::Create)), .def(py::init([](minInValue, maxInValue,
// minOutValue, maxOutValue, dir) {...}), "minInValue"_a = DEFAULT->getMinInValue(), ...,
// "direction"_a = DEFAULT->getDirection()) (PyRangeTransform.cpp:17-40 @ v2.5.2).
static INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    overloads: &[
        Overload {
            params: &[SELF],
            ret: "None",
            call: |py, o, a, _| {
                let rc = TransformRc::new(Transform::Range(RangeTransform::new()));
                construct(&a[0], o.params[0].ty, rc)?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                value_default("minInValue"),
                value_default("maxInValue"),
                value_default("minOutValue"),
                value_default("maxOutValue"),
                DIRECTION_DEFAULT,
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let min_in = crate::arg!(a, 1, convert, f64);
                let max_in = crate::arg!(a, 2, convert, f64);
                let min_out = crate::arg!(a, 3, convert, f64);
                let max_out = crate::arg!(a, 4, convert, f64);
                let dir = enum_param!(o, a, 5);
                let mut t = RangeTransform::new();
                t.set_min_in_value(min_in);
                t.set_max_in_value(max_in);
                t.set_min_out_value(min_out);
                t.set_max_out_value(max_out);
                let rc = TransformRc::new(Transform::Range(t));
                set_direction(&rc, dir);
                validate(py, &rc)?;
                construct(&a[0], o.params[0].ty, rc)?;
                none(py)
            },
            pos_only: 0,
        },
    ],
};

// .def("getStyle", &RangeTransform::getStyle) (PyRangeTransform.cpp:42-43 @ v2.5.2).
static GET_STYLE: Function = method_def!("getStyle", [SELF], RANGE_STYLE, |py, o, a, _| {
    let rc = this_rc!(o, a);
    let style = with(&rc, |t: &mut RangeTransform| t.style())?;
    enum_out(py, RANGE_STYLE, style as i64)
});

// .def("setStyle", &RangeTransform::setStyle, "style"_a) (PyRangeTransform.cpp:44-45 @ v2.5.2).
static SET_STYLE: Function = method_def!(
    "setStyle",
    [SELF, Param::new("style", RANGE_STYLE)],
    "None",
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        let v = enum_param!(o, a, 1);
        let style = [RangeStyle::NoClamp, RangeStyle::Clamp]
            .into_iter()
            .find(|s| *s as i64 == v)
            .ok_or_else(|| outside_enumerators(RANGE_STYLE, v))?;
        with(&rc, |t: &mut RangeTransform| t.set_style(style))?;
        none(py)
    }
);

// .def("getFormatMetadata", ..., py::return_value_policy::reference_internal)
// (PyRangeTransform.cpp:46-49 @ v2.5.2).
static GET_FORMAT_METADATA: Function = crate::format_metadata_def!(SELF);

// .def("equals", &RangeTransform::equals, "other"_a) (PyRangeTransform.cpp:50-51 @ v2.5.2).
static EQUALS: Function = method_def!(
    "equals",
    [SELF, Param::new("other", RANGE_TRANSFORM)],
    "bool",
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        let other = ref_rc!(o, a, 1);
        let equal = crate::py_transform::equals::<RangeTransform>(&rc, &other, |x, y| x.equals(y))?;
        ok(py, equal)
    }
);

crate::file_bit_depths!(RangeTransform, SELF);

/// A value's getter, setter, `has` and `unset` (PyRangeTransform.cpp:60-91 @ v2.5.2).
macro_rules! value_methods {
    ($get:ident, $set:ident, $has:ident, $unset:ident,
     $get_name:literal, $set_name:literal, $has_name:literal, $unset_name:literal,
     $getter:ident, $setter:ident, $haser:ident, $unsetter:ident) => {
        static $get: Function = method_def!($get_name, [SELF], "float", |py, o, a, _| {
            let rc = this_rc!(o, a);
            ok(py, with(&rc, |t: &mut RangeTransform| t.$getter())?)
        });
        static $set: Function = method_def!(
            $set_name,
            [SELF, Param::new("value", FLOAT)],
            "None",
            |py, o, a, convert| {
                let rc = this_rc!(o, a);
                let v = crate::arg!(a, 1, convert, f64);
                with(&rc, |t: &mut RangeTransform| t.$setter(v))?;
                none(py)
            }
        );
        static $has: Function = method_def!($has_name, [SELF], "bool", |py, o, a, _| {
            let rc = this_rc!(o, a);
            ok(py, with(&rc, |t: &mut RangeTransform| t.$haser())?)
        });
        static $unset: Function = method_def!($unset_name, [SELF], "None", |py, o, a, _| {
            let rc = this_rc!(o, a);
            with(&rc, |t: &mut RangeTransform| t.$unsetter())?;
            none(py)
        });
    };
}

value_methods!(
    GET_MIN_IN,
    SET_MIN_IN,
    HAS_MIN_IN,
    UNSET_MIN_IN,
    "getMinInValue",
    "setMinInValue",
    "hasMinInValue",
    "unsetMinInValue",
    min_in_value,
    set_min_in_value,
    has_min_in_value,
    unset_min_in_value
);
value_methods!(
    GET_MAX_IN,
    SET_MAX_IN,
    HAS_MAX_IN,
    UNSET_MAX_IN,
    "getMaxInValue",
    "setMaxInValue",
    "hasMaxInValue",
    "unsetMaxInValue",
    max_in_value,
    set_max_in_value,
    has_max_in_value,
    unset_max_in_value
);
value_methods!(
    GET_MIN_OUT,
    SET_MIN_OUT,
    HAS_MIN_OUT,
    UNSET_MIN_OUT,
    "getMinOutValue",
    "setMinOutValue",
    "hasMinOutValue",
    "unsetMinOutValue",
    min_out_value,
    set_min_out_value,
    has_min_out_value,
    unset_min_out_value
);
value_methods!(
    GET_MAX_OUT,
    SET_MAX_OUT,
    HAS_MAX_OUT,
    UNSET_MAX_OUT,
    "getMaxOutValue",
    "setMaxOutValue",
    "hasMaxOutValue",
    "unsetMaxOutValue",
    max_out_value,
    set_max_out_value,
    has_max_out_value,
    unset_max_out_value
);

static REPR: Function = crate::repr_def!(SELF);

/// Port of `bindPyRangeTransform` (src/bindings/python/transforms/PyRangeTransform.cpp:9-96 @
/// v2.5.2), in its order.
pub(crate) fn bind(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let cls = class(m.py(), RANGE_TRANSFORM)?;
    for f in [
        &INIT,
        &GET_STYLE,
        &SET_STYLE,
        &GET_FORMAT_METADATA,
        &EQUALS,
        &GET_FILE_INPUT_BIT_DEPTH,
        &SET_FILE_INPUT_BIT_DEPTH,
        &GET_FILE_OUTPUT_BIT_DEPTH,
        &SET_FILE_OUTPUT_BIT_DEPTH,
        &GET_MIN_IN,
        &SET_MIN_IN,
        &HAS_MIN_IN,
        &UNSET_MIN_IN,
        &GET_MAX_IN,
        &SET_MAX_IN,
        &HAS_MAX_IN,
        &UNSET_MAX_IN,
        &GET_MIN_OUT,
        &SET_MIN_OUT,
        &HAS_MIN_OUT,
        &UNSET_MIN_OUT,
        &GET_MAX_OUT,
        &SET_MAX_OUT,
        &HAS_MAX_OUT,
        &UNSET_MAX_OUT,
        &REPR,
    ] {
        def(&cls, f)?;
    }
    Ok(())
}
