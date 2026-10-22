// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyFixedFunctionTransform` (src/bindings/python/transforms/PyFixedFunctionTransform.cpp
//! @ v2.5.2).

use ocio::{FixedFunctionTransform, Transform};
use ocio_ops::open_color_types::FixedFunctionStyle;
use pyo3::prelude::*;

use crate::dispatch::{Function, Overload, Param};
use crate::py_open_color_io::py_result;
use crate::py_transform::{
    DIRECTION_DEFAULT, TransformRc, class, construct, def, default, enum_out, none, ok,
    set_direction, unknown_style, validate, with,
};
use crate::{enum_param, method_def, q, ref_rc, this_rc};

const FIXED_FUNCTION_TRANSFORM: &str = q!("FixedFunctionTransform");
const FIXED_FUNCTION_STYLE: &str = q!("FixedFunctionStyle");
const SELF: Param = Param::new("self", FIXED_FUNCTION_TRANSFORM);
const PARAMS: &str = "collections.abc.Sequence[typing.SupportsFloat]";

/// The library's `FixedFunctionStyle` of the C++ value `v`, which the constructor and `setStyle`
/// convert at once: the wheel's error for one outside the enumerators ([`unknown_style`]).
fn style(py: Python<'_>, v: i64) -> PyResult<FixedFunctionStyle> {
    use FixedFunctionStyle::*;
    [
        AcesRedMod03,
        AcesRedMod10,
        AcesGlow03,
        AcesGlow10,
        AcesDarkToDim10,
        Rec2100Surround,
        RgbToHsv,
        XyzToXyy,
        XyzToUvy,
        XyzToLuv,
        AcesGamutMap02,
        AcesGamutMap07,
        AcesGamutComp13,
        LinToPq,
        LinToGammaLog,
        LinToDoubleLog,
        AcesOutputTransform20,
        AcesRgbToJmh20,
        AcesTonescaleCompress20,
        AcesGamutCompress20,
        RgbToHsyLin,
        RgbToHsyLog,
        RgbToHsyVid,
    ]
    .into_iter()
    .find(|s| *s as i64 == v)
    .ok_or_else(|| unknown_style(py, "Unknown FixedFunction transform style: ", v))
}

// .def(py::init([](style, params, dir) {...}), "style"_a, "params"_a =
// getParamsStdVec(DEFAULT), "direction"_a = DEFAULT->getDirection())
// (PyFixedFunctionTransform.cpp:26-43 @ v2.5.2).
static INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    operator: false,
    overloads: &[Overload {
        params: &[
            SELF,
            Param::new("style", FIXED_FUNCTION_STYLE),
            Param {
                name: "params",
                ty: PARAMS,
                default: default("[]", |py| Ok(pyo3::types::PyList::empty(py).into_any())),
            },
            DIRECTION_DEFAULT,
        ],
        ret: "None",
        call: |py, o, a, convert| {
            let style_value = enum_param!(o, a, 1);
            let params = crate::arg!(a, 2, convert, Vec<f64>);
            let dir = enum_param!(o, a, 3);
            let t = py_result(
                py,
                FixedFunctionTransform::new(style(py, style_value)?, &params),
            )?;
            let rc = TransformRc::new(Transform::FixedFunction(t));
            set_direction(&rc, dir);
            validate(py, &rc)?;
            construct(&a[0], o.params[0].ty, rc)?;
            none(py)
        },
        pos_only: 0,
    }],
};

// .def("getFormatMetadata", ..., py::return_value_policy::reference_internal)
// (PyFixedFunctionTransform.cpp:44-48 @ v2.5.2).
static GET_FORMAT_METADATA: Function = crate::format_metadata_def!(SELF);

// .def("equals", &FixedFunctionTransform::equals, "other"_a)
// (PyFixedFunctionTransform.cpp:49-50 @ v2.5.2).
static EQUALS: Function = method_def!(
    "equals",
    [SELF, Param::new("other", FIXED_FUNCTION_TRANSFORM)],
    "bool",
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        let other = ref_rc!(o, a, 1);
        let equal =
            crate::py_transform::equals::<FixedFunctionTransform>(&rc, &other, |x, y| x.equals(y))?;
        ok(py, equal)
    }
);

// .def("getStyle", ...), .def("setStyle", ..., "style"_a) (PyFixedFunctionTransform.cpp:51-54 @
// v2.5.2).
static GET_STYLE: Function =
    method_def!("getStyle", [SELF], FIXED_FUNCTION_STYLE, |py, o, a, _| {
        let rc = this_rc!(o, a);
        let style = with(&rc, |t: &mut FixedFunctionTransform| t.style())?;
        enum_out(py, FIXED_FUNCTION_STYLE, style as i64)
    });

static SET_STYLE: Function = method_def!(
    "setStyle",
    [SELF, Param::new("style", FIXED_FUNCTION_STYLE)],
    "None",
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        let style = style(py, enum_param!(o, a, 1))?;
        py_result(
            py,
            with(&rc, |t: &mut FixedFunctionTransform| t.set_style(style))?,
        )?;
        none(py)
    }
);

// .def("getParams", ...), .def("setParams", ..., "params"_a) (PyFixedFunctionTransform.cpp:
// 55-65 @ v2.5.2).
static GET_PARAMS: Function = method_def!("getParams", [SELF], "list[float]", |py, o, a, _| {
    let rc = this_rc!(o, a);
    ok(py, with(&rc, |t: &mut FixedFunctionTransform| t.params())?)
});

static SET_PARAMS: Function = method_def!(
    "setParams",
    [SELF, Param::new("params", PARAMS)],
    "None",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let params = crate::arg!(a, 1, convert, Vec<f64>);
        with(&rc, |t: &mut FixedFunctionTransform| t.set_params(&params))?;
        none(py)
    }
);

static REPR: Function = crate::repr_def!(SELF);

/// Port of `bindPyFixedFunctionTransform` (src/bindings/python/transforms/
/// PyFixedFunctionTransform.cpp:18-68 @ v2.5.2), in its order.
pub(crate) fn bind(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let cls = class(m.py(), FIXED_FUNCTION_TRANSFORM)?;
    for f in [
        &INIT,
        &GET_FORMAT_METADATA,
        &EQUALS,
        &GET_STYLE,
        &SET_STYLE,
        &GET_PARAMS,
        &SET_PARAMS,
        &REPR,
    ] {
        def(&cls, f)?;
    }
    Ok(())
}
