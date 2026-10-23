// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyExposureContrastTransform` (src/bindings/python/transforms/
//! PyExposureContrastTransform.cpp @ v2.5.2).

use ocio::{ExposureContrastTransform, Transform};
use ocio_ops::open_color_types::ExposureContrastStyle;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyFloat};

use crate::dispatch::{Function, Overload, Param};
use crate::py_transform::{
    DIRECTION_DEFAULT, FLOAT, TransformRc, class, construct, def, default, enum_out, none, ok,
    set_direction, unknown_style, validate, with,
};
use crate::{enum_param, method_def, q, ref_rc, this_rc};

const EXPOSURE_CONTRAST_TRANSFORM: &str = q!("ExposureContrastTransform");
const EXPOSURE_CONTRAST_STYLE: &str = q!("ExposureContrastStyle");
const SELF: Param = Param::new("self", EXPOSURE_CONTRAST_TRANSFORM);

/// The library's `ExposureContrastStyle` of the C++ value `v`, which the constructor and
/// `setStyle` convert at once (`ExposureContrastOpData::ConvertStyle`): the wheel's error for
/// one outside the enumerators ([`unknown_style`], I-262).
///
/// Port of the end of `ExposureContrastOpData::ConvertStyle(ExposureContrastStyle,
/// TransformDirection)` (src/OpenColorIO/ops/exposurecontrast/ExposureContrastOpData.cpp:
/// 103-131 @ v2.5.2).
fn style(py: Python<'_>, v: i64) -> PyResult<ExposureContrastStyle> {
    use ExposureContrastStyle::*;
    [Linear, Video, Logarithmic]
        .into_iter()
        .find(|s| *s as i64 == v)
        .ok_or_else(|| unknown_style(py, "Unknown ExposureContrast transform style: ", v))
}

/// A `double` parameter whose default is the value `DEFAULT` (a new transform) gives.
const fn number_default(
    name: &'static str,
    repr: &'static str,
    value: for<'py> fn(Python<'py>) -> PyResult<Bound<'py, PyAny>>,
) -> Param {
    Param {
        name,
        ty: FLOAT,
        default: default(repr, value),
    }
}

/// A `bool` parameter whose default is `False` (`DEFAULT->isExposureDynamic()` and the others).
const fn false_default(name: &'static str) -> Param {
    Param {
        name,
        ty: "bool",
        default: default("False", |py| {
            Ok(PyBool::new(py, false).to_owned().into_any())
        }),
    }
}

/// A float of `DEFAULT`, a new transform: `f` of it.
fn of_default(py: Python<'_>, f: fn(&ExposureContrastTransform) -> f64) -> Bound<'_, PyAny> {
    PyFloat::new(py, f(&ExposureContrastTransform::new())).into_any()
}

// .def(py::init(&ExposureContrastTransform::Create)), .def(py::init([](style, exposure,
// contrast, gamma, pivot, logExposureStep, logMidGray, dynamicExposure, dynamicContrast,
// dynamicGamma, dir) {...}), "style"_a = DEFAULT->getStyle(), ..., "direction"_a =
// DEFAULT->getDirection()) (PyExposureContrastTransform.cpp:11, 17-57 @ v2.5.2).
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
                let rc =
                    TransformRc::new(Transform::ExposureContrast(ExposureContrastTransform::new()));
                construct(&a[0], o.params[0].ty, rc)?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param {
                    name: "style",
                    ty: EXPOSURE_CONTRAST_STYLE,
                    default: default(
                        "<ExposureContrastStyle.EXPOSURE_CONTRAST_LINEAR: 0>",
                        |py| {
                            let style = ExposureContrastTransform::new().style();
                            crate::enums::to_py(
                                &crate::enums::enum_class(py, EXPOSURE_CONTRAST_STYLE)?,
                                style as i64,
                            )
                        },
                    ),
                },
                number_default("exposure", "0.0", |py| Ok(of_default(py, |t| t.exposure()))),
                number_default("contrast", "1.0", |py| Ok(of_default(py, |t| t.contrast()))),
                number_default("gamma", "1.0", |py| Ok(of_default(py, |t| t.gamma()))),
                number_default("pivot", "0.18", |py| Ok(of_default(py, |t| t.pivot()))),
                number_default("logExposureStep", "0.088", |py| {
                    Ok(of_default(py, |t| t.log_exposure_step()))
                }),
                number_default("logMidGray", "0.435", |py| {
                    Ok(of_default(py, |t| t.log_mid_gray()))
                }),
                false_default("dynamicExposure"),
                false_default("dynamicContrast"),
                false_default("dynamicGamma"),
                DIRECTION_DEFAULT,
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let style_value = enum_param!(o, a, 1);
                let exposure = crate::arg!(a, 2, convert, f64);
                let contrast = crate::arg!(a, 3, convert, f64);
                let gamma = crate::arg!(a, 4, convert, f64);
                let pivot = crate::arg!(a, 5, convert, f64);
                let log_exposure_step = crate::arg!(a, 6, convert, f64);
                let log_mid_gray = crate::arg!(a, 7, convert, f64);
                let dynamic_exposure = crate::arg!(a, 8, convert, bool);
                let dynamic_contrast = crate::arg!(a, 9, convert, bool);
                let dynamic_gamma = crate::arg!(a, 10, convert, bool);
                let dir = enum_param!(o, a, 11);
                let mut t = ExposureContrastTransform::new();
                t.set_style(style(py, style_value)?);
                t.set_exposure(exposure);
                t.set_contrast(contrast);
                t.set_gamma(gamma);
                t.set_pivot(pivot);
                t.set_log_exposure_step(log_exposure_step);
                t.set_log_mid_gray(log_mid_gray);
                if dynamic_exposure {
                    t.make_exposure_dynamic();
                }
                if dynamic_contrast {
                    t.make_contrast_dynamic();
                }
                if dynamic_gamma {
                    t.make_gamma_dynamic();
                }
                let rc = TransformRc::new(Transform::ExposureContrast(t));
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
// (PyExposureContrastTransform.cpp:59-63 @ v2.5.2).
static GET_FORMAT_METADATA: Function = crate::format_metadata_def!(SELF);

// .def("equals", &ExposureContrastTransform::equals, "other"_a)
// (PyExposureContrastTransform.cpp:64-65 @ v2.5.2).
static EQUALS: Function = method_def!(
    "equals",
    [SELF, Param::new("other", EXPOSURE_CONTRAST_TRANSFORM)],
    "bool",
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        let other = ref_rc!(o, a, 1);
        let equal =
            crate::py_transform::equals::<ExposureContrastTransform>(&rc, &other, |x, y| {
                x.equals(y)
            })?;
        ok(py, equal)
    }
);

// .def("getStyle", &ExposureContrastTransform::getStyle) (PyExposureContrastTransform.cpp:66-67
// @ v2.5.2).
static GET_STYLE: Function = method_def!(
    "getStyle",
    [SELF],
    EXPOSURE_CONTRAST_STYLE,
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        let style = with(&rc, |t: &mut ExposureContrastTransform| t.style())?;
        enum_out(py, EXPOSURE_CONTRAST_STYLE, style as i64)
    }
);

// .def("setStyle", &ExposureContrastTransform::setStyle, "style"_a)
// (PyExposureContrastTransform.cpp:68-69 @ v2.5.2).
static SET_STYLE: Function = method_def!(
    "setStyle",
    [SELF, Param::new("style", EXPOSURE_CONTRAST_STYLE)],
    "None",
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        let style = style(py, enum_param!(o, a, 1))?;
        with(&rc, |t: &mut ExposureContrastTransform| t.set_style(style))?;
        none(py)
    }
);

/// A `double` value's getter and setter (`$set_name` taking the keyword `$arg`), as statics
/// `$get` and `$set`.
macro_rules! value_methods {
    ($get:ident, $set:ident, $get_name:literal, $set_name:literal, $arg:literal,
     $getter:ident, $setter:ident) => {
        static $get: Function = method_def!($get_name, [SELF], "float", |py, o, a, _| {
            let rc = this_rc!(o, a);
            ok(
                py,
                with(&rc, |t: &mut ExposureContrastTransform| t.$getter())?,
            )
        });
        static $set: Function = method_def!(
            $set_name,
            [SELF, Param::new($arg, FLOAT)],
            "None",
            |py, o, a, convert| {
                let rc = this_rc!(o, a);
                let v = crate::arg!(a, 1, convert, f64);
                with(&rc, |t: &mut ExposureContrastTransform| t.$setter(v))?;
                none(py)
            }
        );
    };
}

/// A value's dynamic state: `is...Dynamic`, `make...Dynamic`, `make...NonDynamic`, as statics.
macro_rules! dynamic_methods {
    ($is:ident, $make:ident, $unmake:ident, $is_name:literal, $make_name:literal,
     $unmake_name:literal, $is_fn:ident, $make_fn:ident, $unmake_fn:ident) => {
        static $is: Function = method_def!($is_name, [SELF], "bool", |py, o, a, _| {
            let rc = this_rc!(o, a);
            ok(
                py,
                with(&rc, |t: &mut ExposureContrastTransform| t.$is_fn())?,
            )
        });
        static $make: Function = method_def!($make_name, [SELF], "None", |py, o, a, _| {
            let rc = this_rc!(o, a);
            with(&rc, |t: &mut ExposureContrastTransform| t.$make_fn())?;
            none(py)
        });
        static $unmake: Function = method_def!($unmake_name, [SELF], "None", |py, o, a, _| {
            let rc = this_rc!(o, a);
            with(&rc, |t: &mut ExposureContrastTransform| t.$unmake_fn())?;
            none(py)
        });
    };
}

// PyExposureContrastTransform.cpp:70-112 @ v2.5.2.
value_methods!(
    GET_EXPOSURE,
    SET_EXPOSURE,
    "getExposure",
    "setExposure",
    "exposure",
    exposure,
    set_exposure
);
dynamic_methods!(
    IS_EXPOSURE_DYNAMIC,
    MAKE_EXPOSURE_DYNAMIC,
    MAKE_EXPOSURE_NON_DYNAMIC,
    "isExposureDynamic",
    "makeExposureDynamic",
    "makeExposureNonDynamic",
    is_exposure_dynamic,
    make_exposure_dynamic,
    make_exposure_non_dynamic
);
value_methods!(
    GET_CONTRAST,
    SET_CONTRAST,
    "getContrast",
    "setContrast",
    "contrast",
    contrast,
    set_contrast
);
dynamic_methods!(
    IS_CONTRAST_DYNAMIC,
    MAKE_CONTRAST_DYNAMIC,
    MAKE_CONTRAST_NON_DYNAMIC,
    "isContrastDynamic",
    "makeContrastDynamic",
    "makeContrastNonDynamic",
    is_contrast_dynamic,
    make_contrast_dynamic,
    make_contrast_non_dynamic
);
value_methods!(
    GET_GAMMA, SET_GAMMA, "getGamma", "setGamma", "gamma", gamma, set_gamma
);
dynamic_methods!(
    IS_GAMMA_DYNAMIC,
    MAKE_GAMMA_DYNAMIC,
    MAKE_GAMMA_NON_DYNAMIC,
    "isGammaDynamic",
    "makeGammaDynamic",
    "makeGammaNonDynamic",
    is_gamma_dynamic,
    make_gamma_dynamic,
    make_gamma_non_dynamic
);
value_methods!(
    GET_PIVOT, SET_PIVOT, "getPivot", "setPivot", "pivot", pivot, set_pivot
);
value_methods!(
    GET_LOG_EXPOSURE_STEP,
    SET_LOG_EXPOSURE_STEP,
    "getLogExposureStep",
    "setLogExposureStep",
    "logExposureStep",
    log_exposure_step,
    set_log_exposure_step
);
value_methods!(
    GET_LOG_MID_GRAY,
    SET_LOG_MID_GRAY,
    "getLogMidGray",
    "setLogMidGray",
    "logMidGray",
    log_mid_gray,
    set_log_mid_gray
);

static REPR: Function = crate::repr_def!(SELF);

/// Port of `bindPyExposureContrastTransform` (src/bindings/python/transforms/
/// PyExposureContrastTransform.cpp:9-115 @ v2.5.2), in its order.
pub(crate) fn bind(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let cls = class(m.py(), EXPOSURE_CONTRAST_TRANSFORM)?;
    for f in [
        &INIT,
        &GET_FORMAT_METADATA,
        &EQUALS,
        &GET_STYLE,
        &SET_STYLE,
        &GET_EXPOSURE,
        &SET_EXPOSURE,
        &IS_EXPOSURE_DYNAMIC,
        &MAKE_EXPOSURE_DYNAMIC,
        &MAKE_EXPOSURE_NON_DYNAMIC,
        &GET_CONTRAST,
        &SET_CONTRAST,
        &IS_CONTRAST_DYNAMIC,
        &MAKE_CONTRAST_DYNAMIC,
        &MAKE_CONTRAST_NON_DYNAMIC,
        &GET_GAMMA,
        &SET_GAMMA,
        &IS_GAMMA_DYNAMIC,
        &MAKE_GAMMA_DYNAMIC,
        &MAKE_GAMMA_NON_DYNAMIC,
        &GET_PIVOT,
        &SET_PIVOT,
        &GET_LOG_EXPOSURE_STEP,
        &SET_LOG_EXPOSURE_STEP,
        &GET_LOG_MID_GRAY,
        &SET_LOG_MID_GRAY,
        &REPR,
    ] {
        def(&cls, f)?;
    }
    Ok(())
}
