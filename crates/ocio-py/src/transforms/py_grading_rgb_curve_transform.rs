// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyGradingRGBCurveTransform` (src/bindings/python/transforms/
//! PyGradingRGBCurveTransform.cpp @ v2.5.2).
//!
//! **The value.** Upstream's `getValue` returns the curves the transform holds
//! (`ConstGradingRGBCurveRcPtr`, which pybind11 hands out as the very object): a change Python
//! makes through it (a curve's control points) shows in the transform, not validated, until
//! `setValue`, `setSlope` or a new style replaces the curves with a copy. The binding keeps
//! the object it gave in the transform's node ([`crate::py_grading_data::CurveValue`]), which
//! writes it back into the library's transform, as it is, each time the node is locked.

use ocio::{GradingRgbCurveTransform, Transform};
use ocio_ops::open_color_types::RgbCurveType;
use pyo3::prelude::*;

use crate::dispatch::{Function, Overload, Param};
use crate::py_grading_data::{
    CurveValue, GRADING_RGB_CURVE, GRADING_STYLE, RgbCurveNode, RgbCurveRc, rgb_curve_value,
};
use crate::py_open_color_io::py_result;
use crate::py_transform::{FLOAT, INT, TransformRc, class, def, enum_out, none, ok, with};
use crate::shared::{Shared, holder_arg, out};
use crate::transforms::py_grading_primary_transform::{
    DIR_DEFAULT, DYNAMIC_DEFAULT, STYLE_DEFAULT, finish, grading_style,
};
use crate::{enum_param, method_def, q, this_rc};

const GRADING_RGB_CURVE_TRANSFORM: &str = q!("GradingRGBCurveTransform");
const RGB_CURVE_TYPE: &str = q!("RGBCurveType");
const SELF: Param = Param::new("self", GRADING_RGB_CURVE_TRANSFORM);

/// The library's `RGBCurveType` of the C++ value `v`, as `GradingRGBCurveImpl::getCurve` reads
/// it: one outside `[RGB_RED, RGB_NUM_CURVES)` is "Invalid curve." there
/// (src/OpenColorIO/ops/gradingrgbcurve/GradingRGBCurve.cpp:131-147 @ v2.5.2), as
/// `RGB_NUM_CURVES` is the library's.
fn curve_type(v: i64) -> RgbCurveType {
    RgbCurveType::CURVES
        .into_iter()
        .find(|c| *c as i64 == v)
        .unwrap_or(RgbCurveType::NumCurves)
}

/// The port's error where the wheel dereferences a null `GradingRGBCurve` (U-167).
fn null_values() -> PyErr {
    pyo3::exceptions::PyValueError::new_err(format!(
        "{GRADING_RGB_CURVE}: None for a curve, a null pointer the wheel dereferences (its \
         process dies; docs/improvements.md, U-167)"
    ))
}

/// `f` of the transform; then, when `replaces` (the library replaced its curves with a copy),
/// the value object Python holds is no longer the transform's.
fn replacing<R>(
    rc: &TransformRc,
    replaces: bool,
    f: impl FnOnce(&mut GradingRgbCurveTransform) -> R,
) -> PyResult<R> {
    // One lock: the curves Python holds written back, `f`, then (when replaced) let go, so
    // that no later lock writes them over the library's new curves.
    let mut node = rc.lock();
    let t =
        <GradingRgbCurveTransform as crate::py_transform::Variant>::get_mut(&mut node.transform)
            .ok_or_else(|| {
                pyo3::exceptions::PyRuntimeError::new_err(
                    "the instance holds another transform class",
                )
            })?;
    let r = f(t);
    if replaces {
        node.curve_value = None;
    }
    Ok(r)
}

// .def(py::init([](const ConstGradingRGBCurveRcPtr & values, GradingStyle style, bool dynamic,
// TransformDirection dir) {...}), "values"_a, "style"_a = DEFAULT->getStyle(), ...),
// .def(py::init([](GradingStyle style, bool dynamic, TransformDirection dir) {...}), ...)
// (PyGradingRGBCurveTransform.cpp:11, 17-50 @ v2.5.2).
static INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[
                SELF,
                Param::new("values", GRADING_RGB_CURVE),
                STYLE_DEFAULT,
                DYNAMIC_DEFAULT,
                DIR_DEFAULT,
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let Some(values) = holder_arg::<RgbCurveNode>(&a[1], o.params[1].ty, convert)?
                else {
                    return Ok(None);
                };
                let style = enum_param!(o, a, 2);
                let dynamic = crate::arg!(a, 3, convert, bool);
                let dir = enum_param!(o, a, 4);
                let values = values.ok_or_else(null_values)?;
                let values = rgb_curve_value(&values);
                // The wheel's transform validates the values (whatever its style) before the
                // port refuses a style outside the enumerators.
                let style = match grading_style(style) {
                    Ok(style) => style,
                    Err(e) => {
                        py_result(py, values.validate())?;
                        return Err(e);
                    }
                };
                let mut t = GradingRgbCurveTransform::new(style);
                t.set_style(style);
                py_result(py, t.set_value(&values))?;
                if dynamic {
                    t.make_dynamic();
                }
                let rc = TransformRc::new(Transform::GradingRgbCurve(t));
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
                let mut t = GradingRgbCurveTransform::new(style);
                t.set_style(style);
                if dynamic {
                    t.make_dynamic();
                }
                let rc = TransformRc::new(Transform::GradingRgbCurve(t));
                finish(py, &a[0], o.params[0].ty, rc, dir)?;
                none(py)
            },
            pos_only: 0,
        },
    ],
};

// .def("getFormatMetadata", ..., py::return_value_policy::reference_internal)
// (PyGradingRGBCurveTransform.cpp:52-56 @ v2.5.2).
static GET_FORMAT_METADATA: Function = crate::format_metadata_def!(SELF);

// .def("getStyle", &GradingRGBCurveTransform::getStyle) (PyGradingRGBCurveTransform.cpp:57-58
// @ v2.5.2).
static GET_STYLE: Function = method_def!("getStyle", [SELF], GRADING_STYLE, |py, o, a, _| {
    let rc = this_rc!(o, a);
    let style = with(&rc, |t: &mut GradingRgbCurveTransform| t.style())?;
    enum_out(py, GRADING_STYLE, style as i64)
});

// .def("setStyle", &GradingRGBCurveTransform::setStyle, "style"_a)
// (PyGradingRGBCurveTransform.cpp:59-60 @ v2.5.2): a new style resets the curves.
static SET_STYLE: Function = method_def!(
    "setStyle",
    [SELF, Param::new("style", GRADING_STYLE)],
    "None",
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        let style = grading_style(enum_param!(o, a, 1))?;
        let current = with(&rc, |t: &mut GradingRgbCurveTransform| t.style())?;
        replacing(&rc, style != current, |t| t.set_style(style))?;
        none(py)
    }
);

/// The value object of `rc` (an RGB curve transform's node): the one Python holds while the
/// transform's curves are its, else a new one of the transform's curves, which becomes its.
pub(crate) fn value_object(rc: &TransformRc) -> PyResult<RgbCurveRc> {
    let mut node = rc.lock();
    if let Some(CurveValue::Rgb(v)) = &node.curve_value {
        return Ok(v.clone());
    }
    let value = match &node.transform {
        Transform::GradingRgbCurve(t) => t.value(),
        _ => {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "the instance holds another transform class",
            ));
        }
    };
    let v = Shared::new(RgbCurveNode::new(&value));
    node.curve_value = Some(CurveValue::Rgb(v.clone()));
    Ok(v)
}

// .def("getValue", &GradingRGBCurveTransform::getValue) (PyGradingRGBCurveTransform.cpp:61-62
// @ v2.5.2): the transform's own curves.
static GET_VALUE: Function = method_def!("getValue", [SELF], GRADING_RGB_CURVE, |py, o, a, _| {
    let rc = this_rc!(o, a);
    let v = value_object(&rc)?;
    out(py, GRADING_RGB_CURVE, &v)
});

// .def("setValue", &GradingRGBCurveTransform::setValue, "values"_a)
// (PyGradingRGBCurveTransform.cpp:63-64 @ v2.5.2): a copy, validated (a None: U-167).
static SET_VALUE: Function = method_def!(
    "setValue",
    [SELF, Param::new("values", GRADING_RGB_CURVE)],
    "None",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let Some(values) = holder_arg::<RgbCurveNode>(&a[1], o.params[1].ty, convert)? else {
            return Ok(None);
        };
        let values = rgb_curve_value(&values.ok_or_else(null_values)?);
        // The property validates first: a value it refuses leaves the curves as they were.
        let replaces = values.validate().is_ok();
        py_result(
            py,
            replacing(&rc, replaces, |t: &mut GradingRgbCurveTransform| {
                t.set_value(&values)
            })?,
        )?;
        none(py)
    }
);

// .def("getSlope", &GradingRGBCurveTransform::getSlope, "channel"_a, "index"_a)
// (PyGradingRGBCurveTransform.cpp:65-66 @ v2.5.2).
static GET_SLOPE: Function = method_def!(
    "getSlope",
    [
        SELF,
        Param::new("channel", RGB_CURVE_TYPE),
        Param::new("index", INT)
    ],
    "float",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let c = curve_type(enum_param!(o, a, 1));
        let index = crate::arg!(a, 2, convert, u64) as usize;
        let slope = py_result(
            py,
            with(&rc, |t: &mut GradingRgbCurveTransform| t.slope(c, index))?,
        )?;
        ok(py, f64::from(slope))
    }
);

// .def("setSlope", &GradingRGBCurveTransform::setSlope, "channel"_a, "index"_a, "slope"_a)
// (PyGradingRGBCurveTransform.cpp:67-68 @ v2.5.2): upstream sets the slope on a copy of the
// curves, then sets that copy (GradingRGBCurveOpData::setSlope, GradingRGBCurveOpData.cpp:
// 170-176), which replaces the curves unless the curve or the index is refused or the copy
// doesn't validate.
static SET_SLOPE: Function = method_def!(
    "setSlope",
    [
        SELF,
        Param::new("channel", RGB_CURVE_TYPE),
        Param::new("index", INT),
        Param::new("slope", FLOAT)
    ],
    "None",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let c = curve_type(enum_param!(o, a, 1));
        let index = crate::arg!(a, 2, convert, u64) as usize;
        let slope = crate::arg!(a, 3, convert, f32);
        let mut copy = with(&rc, |t: &mut GradingRgbCurveTransform| t.value())?;
        let replaces = copy
            .curve_mut(c)
            .and_then(|curve| curve.set_slope(index, slope))
            .and_then(|()| copy.validate())
            .is_ok();
        py_result(
            py,
            replacing(&rc, replaces, |t: &mut GradingRgbCurveTransform| {
                t.set_slope(c, index, slope)
            })?,
        )?;
        none(py)
    }
);

// .def("slopesAreDefault", &GradingRGBCurveTransform::slopesAreDefault, "channel"_a)
// (PyGradingRGBCurveTransform.cpp:69-70 @ v2.5.2).
static SLOPES_ARE_DEFAULT: Function = method_def!(
    "slopesAreDefault",
    [SELF, Param::new("channel", RGB_CURVE_TYPE)],
    "bool",
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        let c = curve_type(enum_param!(o, a, 1));
        let default = py_result(
            py,
            with(&rc, |t: &mut GradingRgbCurveTransform| {
                t.slopes_are_default(c)
            })?,
        )?;
        ok(py, default)
    }
);

// .def("getBypassLinToLog", ...), .def("setBypassLinToLog", ..., "bypass"_a)
// (PyGradingRGBCurveTransform.cpp:71-74 @ v2.5.2).
static GET_BYPASS: Function = method_def!("getBypassLinToLog", [SELF], "bool", |py, o, a, _| {
    let rc = this_rc!(o, a);
    ok(
        py,
        with(&rc, |t: &mut GradingRgbCurveTransform| {
            t.bypass_lin_to_log()
        })?,
    )
});
static SET_BYPASS: Function = method_def!(
    "setBypassLinToLog",
    [SELF, Param::new("bypass", "bool")],
    "None",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let bypass = crate::arg!(a, 1, convert, bool);
        with(&rc, |t: &mut GradingRgbCurveTransform| {
            t.set_bypass_lin_to_log(bypass)
        })?;
        none(py)
    }
);

// .def("isDynamic", ...), .def("makeDynamic", ...), .def("makeNonDynamic", ...)
// (PyGradingRGBCurveTransform.cpp:75-80 @ v2.5.2).
crate::grading_dynamic_methods!(GradingRgbCurveTransform, SELF);

static REPR: Function = crate::repr_def!(SELF);

/// Port of `bindPyGradingRGBCurveTransform` (src/bindings/python/transforms/
/// PyGradingRGBCurveTransform.cpp:9-83 @ v2.5.2), in its order.
pub(crate) fn bind(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let cls = class(m.py(), GRADING_RGB_CURVE_TRANSFORM)?;
    for f in [
        &INIT,
        &GET_FORMAT_METADATA,
        &GET_STYLE,
        &SET_STYLE,
        &GET_VALUE,
        &SET_VALUE,
        &GET_SLOPE,
        &SET_SLOPE,
        &SLOPES_ARE_DEFAULT,
        &GET_BYPASS,
        &SET_BYPASS,
        &IS_DYNAMIC,
        &MAKE_DYNAMIC,
        &MAKE_NON_DYNAMIC,
        &REPR,
    ] {
        def(&cls, f)?;
    }
    Ok(())
}
