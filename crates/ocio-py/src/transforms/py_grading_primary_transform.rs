// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyGradingPrimaryTransform` (src/bindings/python/transforms/PyGradingPrimaryTransform.cpp
//! @ v2.5.2), and what the four grading transforms' bindings share: their style, `dynamic` and
//! `dir` parameters.

use ocio::{GradingPrimaryTransform, Transform};
use ocio_ops::open_color_types::GradingStyle;
use ocio_ops::ops::gradingprimary::grading_primary::GradingPrimary;
use pyo3::prelude::*;
use pyo3::types::PyBool;

use crate::dispatch::{Function, Overload, Param};
use crate::py_grading_data::{GRADING_PRIMARY, GRADING_STYLE, value_style};
use crate::py_open_color_io::py_result;
use crate::py_transform::{
    TRANSFORM_DIRECTION, TransformRc, class, construct, def, default, enum_out, none,
    outside_enumerators, set_direction, validate, with,
};
use crate::shared::{Shared, holder, out};
use crate::{enum_param, method_def, q, this_rc};

const GRADING_PRIMARY_TRANSFORM: &str = q!("GradingPrimaryTransform");
const SELF: Param = Param::new("self", GRADING_PRIMARY_TRANSFORM);

/// The library's `GradingStyle` of the C++ value `v`, which the transforms store as it is: the
/// port's error for one outside the enumerators (U-112; the wheel's `repr` of such a transform
/// kills its process).
pub(crate) fn grading_style(v: i64) -> PyResult<GradingStyle> {
    [GradingStyle::Log, GradingStyle::Lin, GradingStyle::Video]
        .into_iter()
        .find(|s| *s as i64 == v)
        .ok_or_else(|| outside_enumerators(GRADING_STYLE, v))
}

/// `"style"_a = DEFAULT->getStyle()`: `GRADING_LOG`, the style of each class's `DEFAULT`.
pub(crate) const STYLE_DEFAULT: Param = Param {
    name: "style",
    ty: GRADING_STYLE,
    default: default("<GradingStyle.GRADING_LOG: 0>", |py| {
        crate::enums::to_py(
            &crate::enums::enum_class(py, GRADING_STYLE)?,
            GradingStyle::Log as i64,
        )
    }),
};

/// `"dynamic"_a = DEFAULT->isDynamic()`: `False`.
pub(crate) const DYNAMIC_DEFAULT: Param = Param {
    name: "dynamic",
    ty: "bool",
    default: default("False", |py| {
        Ok(PyBool::new(py, false).to_owned().into_any())
    }),
};

/// `"dir"_a = DEFAULT->getDirection()`: forward.
pub(crate) const DIR_DEFAULT: Param = Param {
    name: "dir",
    ty: TRANSFORM_DIRECTION,
    default: default("<TransformDirection.TRANSFORM_DIR_FORWARD: 0>", |py| {
        crate::enums::to_py(&crate::enums::enum_class(py, TRANSFORM_DIRECTION)?, 0)
    }),
};

/// The end of each constructor: `if (dynamic) { p->makeDynamic(); }`, `p->setDirection(dir)`,
/// `p->validate()`; then `rc` is `this`'s value.
pub(crate) fn finish(
    py: Python<'_>,
    this: &Bound<'_, PyAny>,
    qualified: &str,
    rc: TransformRc,
    dir: i64,
) -> PyResult<()> {
    set_direction(&rc, dir);
    validate(py, &rc)?;
    construct(this, qualified, rc)
}

/// A `GradingPrimary` argument (`const GradingPrimary &`: `None` refused), its value, or the
/// next overload.
macro_rules! primary_arg {
    ($o:expr, $a:expr, $i:expr) => {
        match holder::<GradingPrimary>(&$a[$i], $o.params[$i].ty)? {
            Some(rc) => *rc.lock(),
            None => return Ok(None),
        }
    };
}

// .def(py::init([](const GradingPrimary & values, GradingStyle style, bool dynamic,
// TransformDirection dir) {...}), "values"_a, "style"_a = DEFAULT->getStyle(), "dynamic"_a =
// DEFAULT->isDynamic(), "dir"_a = DEFAULT->getDirection()), .def(py::init([](GradingStyle
// style, bool dynamic, TransformDirection dir) {...}), ...) (PyGradingPrimaryTransform.cpp:11,
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
                Param::new("values", GRADING_PRIMARY),
                STYLE_DEFAULT,
                DYNAMIC_DEFAULT,
                DIR_DEFAULT,
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let values = primary_arg!(o, a, 1);
                let style = enum_param!(o, a, 2);
                let dynamic = crate::arg!(a, 3, convert, bool);
                let dir = enum_param!(o, a, 4);
                // The wheel's transform validates the values with the style it stores (as a
                // value's validate reads it, py_grading_data::value_style) before the port
                // refuses one outside the enumerators.
                let style = match grading_style(style) {
                    Ok(style) => style,
                    Err(e) => {
                        py_result(py, values.validate(value_style(style)))?;
                        return Err(e);
                    }
                };
                let mut t = GradingPrimaryTransform::new(style);
                t.set_style(style);
                py_result(py, t.set_value(&values))?;
                if dynamic {
                    t.make_dynamic();
                }
                let rc = TransformRc::new(Transform::GradingPrimary(t));
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
                let mut t = GradingPrimaryTransform::new(style);
                t.set_style(style);
                if dynamic {
                    t.make_dynamic();
                }
                let rc = TransformRc::new(Transform::GradingPrimary(t));
                finish(py, &a[0], o.params[0].ty, rc, dir)?;
                none(py)
            },
            pos_only: 0,
        },
    ],
};

// .def("getFormatMetadata", ..., py::return_value_policy::reference_internal)
// (PyGradingPrimaryTransform.cpp:52-56 @ v2.5.2).
static GET_FORMAT_METADATA: Function = crate::format_metadata_def!(SELF);

// .def("getStyle", &GradingPrimaryTransform::getStyle) (PyGradingPrimaryTransform.cpp:57-58 @
// v2.5.2).
static GET_STYLE: Function = method_def!("getStyle", [SELF], GRADING_STYLE, |py, o, a, _| {
    let rc = this_rc!(o, a);
    let style = with(&rc, |t: &mut GradingPrimaryTransform| t.style())?;
    enum_out(py, GRADING_STYLE, style as i64)
});

// .def("setStyle", &GradingPrimaryTransform::setStyle, "style"_a)
// (PyGradingPrimaryTransform.cpp:59-60 @ v2.5.2).
static SET_STYLE: Function = method_def!(
    "setStyle",
    [SELF, Param::new("style", GRADING_STYLE)],
    "None",
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        let style = grading_style(enum_param!(o, a, 1))?;
        with(&rc, |t: &mut GradingPrimaryTransform| t.set_style(style))?;
        none(py)
    }
);

// .def("getValue", &GradingPrimaryTransform::getValue) (PyGradingPrimaryTransform.cpp:61-62 @
// v2.5.2): a `const GradingPrimary &`, which pybind11 copies into a new object.
static GET_VALUE: Function = method_def!("getValue", [SELF], GRADING_PRIMARY, |py, o, a, _| {
    let rc = this_rc!(o, a);
    let value = with(&rc, |t: &mut GradingPrimaryTransform| t.value())?;
    out(py, GRADING_PRIMARY, &Shared::new(value))
});

// .def("setValue", &GradingPrimaryTransform::setValue, "values"_a)
// (PyGradingPrimaryTransform.cpp:63-64 @ v2.5.2).
static SET_VALUE: Function = method_def!(
    "setValue",
    [SELF, Param::new("values", GRADING_PRIMARY)],
    "None",
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        let values = primary_arg!(o, a, 1);
        py_result(
            py,
            with(&rc, |t: &mut GradingPrimaryTransform| t.set_value(&values))?,
        )?;
        none(py)
    }
);

/// `isDynamic`, `makeDynamic` and `makeNonDynamic` of the transform class `$t`, as statics.
#[macro_export]
macro_rules! grading_dynamic_methods {
    ($t:ty, $self:expr) => {
        static IS_DYNAMIC: $crate::dispatch::Function =
            $crate::method_def!("isDynamic", [$self], "bool", |py, o, a, _| {
                let rc = $crate::this_rc!(o, a);
                $crate::py_transform::ok(
                    py,
                    $crate::py_transform::with(&rc, |t: &mut $t| t.is_dynamic())?,
                )
            });
        static MAKE_DYNAMIC: $crate::dispatch::Function =
            $crate::method_def!("makeDynamic", [$self], "None", |py, o, a, _| {
                let rc = $crate::this_rc!(o, a);
                $crate::py_transform::with(&rc, |t: &mut $t| t.make_dynamic())?;
                $crate::py_transform::none(py)
            });
        static MAKE_NON_DYNAMIC: $crate::dispatch::Function =
            $crate::method_def!("makeNonDynamic", [$self], "None", |py, o, a, _| {
                let rc = $crate::this_rc!(o, a);
                $crate::py_transform::with(&rc, |t: &mut $t| t.make_non_dynamic())?;
                $crate::py_transform::none(py)
            });
    };
}

// .def("isDynamic", ...), .def("makeDynamic", ...), .def("makeNonDynamic", ...)
// (PyGradingPrimaryTransform.cpp:65-70 @ v2.5.2).
grading_dynamic_methods!(GradingPrimaryTransform, SELF);

static REPR: Function = crate::repr_def!(SELF);

/// Port of `bindPyGradingPrimaryTransform` (src/bindings/python/transforms/
/// PyGradingPrimaryTransform.cpp:9-73 @ v2.5.2), in its order.
pub(crate) fn bind(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let cls = class(m.py(), GRADING_PRIMARY_TRANSFORM)?;
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
