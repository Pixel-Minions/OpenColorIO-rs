// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyGradingData` (src/bindings/python/PyGradingData.cpp @ v2.5.2): the grading values and
//! curves (chunk 6.7a).
//!
//! **Values and the references into them.** pybind11 holds `GradingRGBM`, `GradingPrimary`,
//! `GradingRGBMSW`, `GradingTone` and `GradingControlPoint` by value, one per Python object, and
//! binds their members with `def_readwrite`, whose getter returns a reference into the object
//! (`reference_internal`): `primary.brightness` is a `GradingRGBM` object over the primary's own
//! `m_brightness`, through which a change shows in the primary, that keeps the primary's object
//! alive (`keep_alive<0, 1>`), and that pybind11 returns again while it lives (its registered
//! instance at that address). The binding holds each value as a [`Shared`] value, and a
//! `GradingRGBM` or `GradingRGBMSW` as a [`Place`]: its own value, or a field of a primary's or a
//! tone's.

use std::mem::offset_of;
use std::sync::Arc;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::open_color_types::{BSplineType, GradingStyle, HueCurveType};
use ocio_ops::ops::gradinghuecurve::grading_hue_curve::GradingHueCurve;
use ocio_ops::ops::gradingprimary::grading_primary::{GradingPrimary, GradingRgbm};
use ocio_ops::ops::gradingrgbcurve::grading_b_spline_curve::{
    GradingBSplineCurve, GradingControlPoint,
};
use ocio_ops::ops::gradingrgbcurve::grading_rgb_curve::GradingRgbCurve;
use ocio_ops::ops::gradingtone::grading_tone::{GradingRgbmsw, GradingTone};
use pyo3::prelude::*;

use crate::dispatch::{Function, Overload, Param};
use crate::py_open_color_io::{py_result, to_py_err};
use crate::py_transform::{
    FLOAT, INT, Out, class, def, def_property, def_property_readonly_static, def_readwrite,
    default, enum_out, nested_class, none, ok, outside_enumerators, string_out,
};
use crate::py_utils::new_iterator;
use crate::shared::{
    Shared, construct, construct_with, holder, holder_arg, holder_with, out, to_py, to_py_with,
};
use crate::{enum_param, method_def, q, this_iter};

pub(crate) const GRADING_RGBM: &str = q!("GradingRGBM");
pub(crate) const GRADING_PRIMARY: &str = q!("GradingPrimary");
pub(crate) const GRADING_RGBMSW: &str = q!("GradingRGBMSW");
pub(crate) const GRADING_TONE: &str = q!("GradingTone");
pub(crate) const GRADING_CONTROL_POINT: &str = q!("GradingControlPoint");
pub(crate) const GRADING_STYLE: &str = q!("GradingStyle");

// ---------------------------------------------------------------------------------------------
// Where the values live

/// Where the value of a `GradingRGBM` (`T` = [`GradingRgbm`], `P` = [`GradingPrimary`]) or a
/// `GradingRGBMSW` (`T` = [`GradingRgbmsw`], `P` = [`GradingTone`]) object lives.
pub(crate) enum Place<T, P> {
    /// The object's own value.
    Own(Shared<T>),
    /// A field of the value of another object (`def_readwrite`'s getter).
    Field {
        /// That object, which this one keeps alive (`keep_alive<0, 1>`).
        owner: Arc<Py<PyAny>>,
        /// Its value.
        parent: Shared<P>,
        /// The field.
        field: fn(&mut P) -> &mut T,
        /// The field's address, which pybind11's registry keys the object by.
        key: usize,
    },
}

impl<T, P> Clone for Place<T, P> {
    fn clone(&self) -> Self {
        match self {
            Place::Own(v) => Place::Own(v.clone()),
            Place::Field {
                owner,
                parent,
                field,
                key,
            } => Place::Field {
                owner: owner.clone(),
                parent: parent.clone(),
                field: *field,
                key: *key,
            },
        }
    }
}

impl<T: Copy, P> Place<T, P> {
    /// The value now.
    pub(crate) fn get(&self) -> T {
        match self {
            Place::Own(v) => *v.lock(),
            Place::Field { parent, field, .. } => *field(&mut parent.lock()),
        }
    }

    /// Changes the value in place.
    pub(crate) fn update(&self, f: impl FnOnce(&mut T)) {
        match self {
            Place::Own(v) => f(&mut v.lock()),
            Place::Field { parent, field, .. } => f(field(&mut parent.lock())),
        }
    }
}

/// A `GradingRGBM` object's value.
pub(crate) type RgbmPlace = Place<GradingRgbm, GradingPrimary>;
/// A `GradingRGBMSW` object's value.
pub(crate) type RgbmswPlace = Place<GradingRgbmsw, GradingTone>;

/// The `GradingRGBM` or `GradingRGBMSW` object over the field `field` of the value `parent` of
/// `owner`, at `offset` in it: the object registered there while it lives, else a new one that
/// keeps `owner` alive (`reference_internal`).
fn field_out<T: Send + 'static, P: Send + 'static>(
    owner: &Bound<'_, PyAny>,
    qualified: &'static str,
    parent: Shared<P>,
    field: fn(&mut P) -> &mut T,
    offset: usize,
) -> Out {
    let key = parent.key() + offset;
    let obj = to_py_with(owner.py(), qualified, key, || Place::Field {
        owner: Arc::new(owner.clone().unbind()),
        parent,
        field,
        key,
    })?;
    Ok(Some(obj.unbind()))
}

/// The value an argument holds as a `GradingRGBM` or `GradingRGBMSW` object (a `const T &`:
/// `None` refused), or the next overload.
macro_rules! place_arg {
    ($o:expr, $a:expr, $i:expr, $t:ty) => {
        match holder_with::<$t>(&$a[$i], $o.params[$i].ty)? {
            Some(place) => place,
            None => return Ok(None),
        }
    };
}

/// The value an argument holds as an object of a class held by a [`Shared`] value (a
/// `const T &`: `None` refused), or the next overload.
macro_rules! shared_arg {
    ($o:expr, $a:expr, $i:expr, $t:ty) => {
        match holder::<$t>(&$a[$i], $o.params[$i].ty)? {
            Some(rc) => rc,
            None => return Ok(None),
        }
    };
}

/// The library's style for the C++ value `v` of a `GradingStyle` that a value's constructor or
/// `validate` takes: these only compare it with `GRADING_LOG` and `GRADING_LIN`, so a value
/// outside the enumerators (which Python can make, I-223) acts as `GRADING_VIDEO` does.
///
/// `GradingPrimary(GradingStyle)` (include/OpenColorIO/OpenColorTransforms.h:465-471 @ v2.5.2),
/// `GradingPrimary::validate` (src/OpenColorIO/ops/gradingprimary/GradingPrimary.cpp:65-106)
/// and `GradingTone(GradingStyle)` (OpenColorTransforms.h:684-700).
pub(crate) fn value_style(v: i64) -> GradingStyle {
    match v {
        0 => GradingStyle::Log,
        1 => GradingStyle::Lin,
        _ => GradingStyle::Video,
    }
}

/// `operator<<` of a value on a new stream (`defRepr`, PyUtils.h:28-37 @ v2.5.2), as a `str`.
fn repr_out(py: Python<'_>, write: impl FnOnce(&mut OStringStream)) -> Out {
    let mut os = OStringStream::new(Crt::NATIVE);
    write(&mut os);
    string_out(py, &os.into_bytes())
}

/// An operator of one overload (`py::is_operator()`): `(self: <class>, arg0: <class>) -> bool`.
macro_rules! operator_def {
    ($name:literal, $self:expr, $other:expr, $call:expr) => {
        Function {
            name: $name,
            constructor: false,
            method: true,
            operator: true,
            overloads: &[Overload {
                params: &[$self, $other],
                ret: "bool",
                call: $call,
                pos_only: 0,
            }],
        }
    };
}

/// `def_readwrite` of a number: its getter and setter (`f64`, or `f32` for a `float` member),
/// as statics `$get` and `$set`, reading and writing `$field` of the value `$load` gives.
macro_rules! number_member {
    ($get:ident, $set:ident, $self:expr, $num:ty, $load:ident, $field:ident) => {
        static $get: Function = method_def!("", [$self], "float", |py, o, a, _| {
            let v = $load!(o, a);
            ok(py, f64::from(v.get().$field))
        });
        static $set: Function = method_def!(
            "",
            [$self, Param::new("arg0", FLOAT)],
            "None",
            |py, o, a, convert| {
                let v = $load!(o, a);
                let x = crate::arg!(a, 1, convert, $num);
                v.update(|v| v.$field = x);
                none(py)
            }
        );
    };
}

/// A [`Shared`] value read and changed as a [`Place`] is.
trait Cell<T> {
    fn get(&self) -> T;
    fn update(&self, f: impl FnOnce(&mut T));
}

impl<T: Copy> Cell<T> for Shared<T> {
    fn get(&self) -> T {
        *self.lock()
    }

    fn update(&self, f: impl FnOnce(&mut T)) {
        f(&mut self.lock())
    }
}

// ---------------------------------------------------------------------------------------------
// GradingRGBM

const RGBM_SELF: Param = Param::new("self", GRADING_RGBM);
const RGBM_ARG0: Param = Param::new("arg0", GRADING_RGBM);

macro_rules! this_rgbm {
    ($o:expr, $a:expr) => {
        place_arg!($o, $a, 0, RgbmPlace)
    };
}

/// A new `GradingRGBM` object `this` with its own value `v`.
fn construct_rgbm(this: &Bound<'_, PyAny>, v: GradingRgbm) -> PyResult<()> {
    let rc = Shared::new(v);
    construct_with(this, GRADING_RGBM, rc.key(), RgbmPlace::Own(rc))
}

// .def(py::init<>()), .def(py::init<double, double, double, double>(), "red"_a, "green"_a,
// "blue"_a, "master"_a) (PyGradingData.cpp:79-83 @ v2.5.2).
static RGBM_INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[RGBM_SELF],
            ret: "None",
            call: |py, _, a, _| {
                construct_rgbm(&a[0], GradingRgbm::default())?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                RGBM_SELF,
                Param::new("red", FLOAT),
                Param::new("green", FLOAT),
                Param::new("blue", FLOAT),
                Param::new("master", FLOAT),
            ],
            ret: "None",
            call: |py, _, a, convert| {
                let red = crate::arg!(a, 1, convert, f64);
                let green = crate::arg!(a, 2, convert, f64);
                let blue = crate::arg!(a, 3, convert, f64);
                let master = crate::arg!(a, 4, convert, f64);
                construct_rgbm(&a[0], GradingRgbm::new(red, green, blue, master))?;
                none(py)
            },
            pos_only: 0,
        },
    ],
};

// .def("__eq__", [](const GradingRGBM &self, const GradingRGBM &other) { return self ==
// other; }, py::is_operator()) and "__ne__" (PyGradingData.cpp:85-92 @ v2.5.2).
static RGBM_EQ: Function = operator_def!("__eq__", RGBM_SELF, RGBM_ARG0, |py, o, a, _| {
    let lhs = this_rgbm!(o, a).get();
    let rhs = place_arg!(o, a, 1, RgbmPlace).get();
    ok(py, lhs == rhs)
});
static RGBM_NE: Function = operator_def!("__ne__", RGBM_SELF, RGBM_ARG0, |py, o, a, _| {
    let lhs = this_rgbm!(o, a).get();
    let rhs = place_arg!(o, a, 1, RgbmPlace).get();
    ok(py, lhs != rhs)
});

// .def_readwrite("red", &GradingRGBM::m_red) and the others (PyGradingData.cpp:94-101 @
// v2.5.2).
number_member!(RGBM_GET_RED, RGBM_SET_RED, RGBM_SELF, f64, this_rgbm, red);
number_member!(
    RGBM_GET_GREEN,
    RGBM_SET_GREEN,
    RGBM_SELF,
    f64,
    this_rgbm,
    green
);
number_member!(
    RGBM_GET_BLUE,
    RGBM_SET_BLUE,
    RGBM_SELF,
    f64,
    this_rgbm,
    blue
);
number_member!(
    RGBM_GET_MASTER,
    RGBM_SET_MASTER,
    RGBM_SELF,
    f64,
    this_rgbm,
    master
);

// defRepr(clsGradingRGBM) (PyGradingData.cpp:103 @ v2.5.2; PyUtils.h:28-37).
static RGBM_REPR: Function = method_def!("__repr__", [RGBM_SELF], "str", |py, o, a, _| {
    let v = this_rgbm!(o, a).get();
    repr_out(py, |os| v.write_to(os))
});

// ---------------------------------------------------------------------------------------------
// GradingPrimary

const PRIMARY_SELF: Param = Param::new("self", GRADING_PRIMARY);
const PRIMARY_ARG0: Param = Param::new("arg0", GRADING_PRIMARY);
const STYLE_ARG0: Param = Param::new("arg0", GRADING_STYLE);

macro_rules! this_primary {
    ($o:expr, $a:expr) => {
        shared_arg!($o, $a, 0, GradingPrimary)
    };
}

// .def(py::init<GradingStyle>()) (PyGradingData.cpp:106-107 @ v2.5.2).
static PRIMARY_INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    operator: false,
    overloads: &[Overload {
        params: &[PRIMARY_SELF, STYLE_ARG0],
        ret: "None",
        call: |py, o, a, _| {
            let style = value_style(enum_param!(o, a, 1));
            construct(
                &a[0],
                GRADING_PRIMARY,
                Shared::new(GradingPrimary::new(style)),
            )?;
            none(py)
        },
        pos_only: 0,
    }],
};

// .def("__eq__", ...), .def("__ne__", ...) (PyGradingData.cpp:109-116 @ v2.5.2).
static PRIMARY_EQ: Function = operator_def!("__eq__", PRIMARY_SELF, PRIMARY_ARG0, |py, o, a, _| {
    let lhs = this_primary!(o, a).get();
    let rhs = shared_arg!(o, a, 1, GradingPrimary).get();
    ok(py, lhs == rhs)
});
static PRIMARY_NE: Function = operator_def!("__ne__", PRIMARY_SELF, PRIMARY_ARG0, |py, o, a, _| {
    let lhs = this_primary!(o, a).get();
    let rhs = shared_arg!(o, a, 1, GradingPrimary).get();
    ok(py, lhs != rhs)
});

// .def("validate", &GradingPrimary::validate) (PyGradingData.cpp:118-119 @ v2.5.2).
static PRIMARY_VALIDATE: Function = method_def!(
    "validate",
    [PRIMARY_SELF, STYLE_ARG0],
    "None",
    |py, o, a, _| {
        let v = this_primary!(o, a).get();
        let style = value_style(enum_param!(o, a, 1));
        py_result(py, v.validate(style))?;
        none(py)
    }
);

/// `def_readwrite` of a `GradingRGBM` member of `GradingPrimary`: its getter (the reference
/// into the primary) and setter (a copy of the value assigned), as statics `$get` and `$set`.
macro_rules! rgbm_member {
    ($get:ident, $set:ident, $field:ident) => {
        static $get: Function = method_def!("", [PRIMARY_SELF], GRADING_RGBM, |_py, o, a, _| {
            let rc = this_primary!(o, a);
            field_out(
                &a[0],
                GRADING_RGBM,
                rc,
                |p: &mut GradingPrimary| &mut p.$field,
                offset_of!(GradingPrimary, $field),
            )
        });
        static $set: Function =
            method_def!("", [PRIMARY_SELF, RGBM_ARG0], "None", |py, o, a, _| {
                let rc = this_primary!(o, a);
                let v = place_arg!(o, a, 1, RgbmPlace).get();
                rc.update(|p| p.$field = v);
                none(py)
            });
    };
}

// .def_readwrite("brightness", &GradingPrimary::m_brightness) to "pivotBlack"
// (PyGradingData.cpp:121-146 @ v2.5.2).
rgbm_member!(PRIMARY_GET_BRIGHTNESS, PRIMARY_SET_BRIGHTNESS, brightness);
rgbm_member!(PRIMARY_GET_CONTRAST, PRIMARY_SET_CONTRAST, contrast);
rgbm_member!(PRIMARY_GET_GAMMA, PRIMARY_SET_GAMMA, gamma);
rgbm_member!(PRIMARY_GET_OFFSET, PRIMARY_SET_OFFSET, offset);
rgbm_member!(PRIMARY_GET_EXPOSURE, PRIMARY_SET_EXPOSURE, exposure);
rgbm_member!(PRIMARY_GET_LIFT, PRIMARY_SET_LIFT, lift);
rgbm_member!(PRIMARY_GET_GAIN, PRIMARY_SET_GAIN, gain);
number_member!(
    PRIMARY_GET_PIVOT,
    PRIMARY_SET_PIVOT,
    PRIMARY_SELF,
    f64,
    this_primary,
    pivot
);
number_member!(
    PRIMARY_GET_SATURATION,
    PRIMARY_SET_SATURATION,
    PRIMARY_SELF,
    f64,
    this_primary,
    saturation
);
number_member!(
    PRIMARY_GET_CLAMP_WHITE,
    PRIMARY_SET_CLAMP_WHITE,
    PRIMARY_SELF,
    f64,
    this_primary,
    clamp_white
);
number_member!(
    PRIMARY_GET_CLAMP_BLACK,
    PRIMARY_SET_CLAMP_BLACK,
    PRIMARY_SELF,
    f64,
    this_primary,
    clamp_black
);
number_member!(
    PRIMARY_GET_PIVOT_WHITE,
    PRIMARY_SET_PIVOT_WHITE,
    PRIMARY_SELF,
    f64,
    this_primary,
    pivot_white
);
number_member!(
    PRIMARY_GET_PIVOT_BLACK,
    PRIMARY_SET_PIVOT_BLACK,
    PRIMARY_SELF,
    f64,
    this_primary,
    pivot_black
);

// .def_property_readonly_static("NoClampBlack", [](py::object) { return
// GradingPrimary::NoClampBlack(); }) and "NoClampWhite" (PyGradingData.cpp:148-157 @ v2.5.2).
static NO_CLAMP_BLACK: Function = Function {
    name: "",
    constructor: false,
    method: false,
    operator: false,
    overloads: &[Overload {
        params: &[Param::new("arg0", "object")],
        ret: "float",
        call: |py, _, _, _| ok(py, GradingPrimary::NO_CLAMP_BLACK),
        pos_only: 0,
    }],
};
static NO_CLAMP_WHITE: Function = Function {
    name: "",
    constructor: false,
    method: false,
    operator: false,
    overloads: &[Overload {
        params: &[Param::new("arg0", "object")],
        ret: "float",
        call: |py, _, _, _| ok(py, GradingPrimary::NO_CLAMP_WHITE),
        pos_only: 0,
    }],
};

// defRepr(clsGradingPrimary) (PyGradingData.cpp:159 @ v2.5.2).
static PRIMARY_REPR: Function = method_def!("__repr__", [PRIMARY_SELF], "str", |py, o, a, _| {
    let v = this_primary!(o, a).get();
    repr_out(py, |os| v.write_to(os))
});

// ---------------------------------------------------------------------------------------------
// GradingRGBMSW

const RGBMSW_SELF: Param = Param::new("self", GRADING_RGBMSW);
const RGBMSW_ARG0: Param = Param::new("arg0", GRADING_RGBMSW);

macro_rules! this_rgbmsw {
    ($o:expr, $a:expr) => {
        place_arg!($o, $a, 0, RgbmswPlace)
    };
}

/// A new `GradingRGBMSW` object `this` with its own value `v`.
fn construct_rgbmsw(this: &Bound<'_, PyAny>, v: GradingRgbmsw) -> PyResult<()> {
    let rc = Shared::new(v);
    construct_with(this, GRADING_RGBMSW, rc.key(), RgbmswPlace::Own(rc))
}

// .def(py::init<>()), .def(py::init<double, double, double, double, double, double>(), "red"_a,
// "green"_a, "blue"_a, "master"_a, "start"_a, "width"_a), .def(py::init<double, double>(),
// "start"_a, "width"_a) (PyGradingData.cpp:162-169 @ v2.5.2).
static RGBMSW_INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[RGBMSW_SELF],
            ret: "None",
            call: |py, _, a, _| {
                construct_rgbmsw(&a[0], GradingRgbmsw::default())?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                RGBMSW_SELF,
                Param::new("red", FLOAT),
                Param::new("green", FLOAT),
                Param::new("blue", FLOAT),
                Param::new("master", FLOAT),
                Param::new("start", FLOAT),
                Param::new("width", FLOAT),
            ],
            ret: "None",
            call: |py, _, a, convert| {
                let red = crate::arg!(a, 1, convert, f64);
                let green = crate::arg!(a, 2, convert, f64);
                let blue = crate::arg!(a, 3, convert, f64);
                let master = crate::arg!(a, 4, convert, f64);
                let start = crate::arg!(a, 5, convert, f64);
                let width = crate::arg!(a, 6, convert, f64);
                let v = GradingRgbmsw::new(red, green, blue, master, start, width);
                construct_rgbmsw(&a[0], v)?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                RGBMSW_SELF,
                Param::new("start", FLOAT),
                Param::new("width", FLOAT),
            ],
            ret: "None",
            call: |py, _, a, convert| {
                let start = crate::arg!(a, 1, convert, f64);
                let width = crate::arg!(a, 2, convert, f64);
                construct_rgbmsw(&a[0], GradingRgbmsw::with_start_width(start, width))?;
                none(py)
            },
            pos_only: 0,
        },
    ],
};

// .def("__eq__", ...), .def("__ne__", ...) (PyGradingData.cpp:171-178 @ v2.5.2).
static RGBMSW_EQ: Function = operator_def!("__eq__", RGBMSW_SELF, RGBMSW_ARG0, |py, o, a, _| {
    let lhs = this_rgbmsw!(o, a).get();
    let rhs = place_arg!(o, a, 1, RgbmswPlace).get();
    ok(py, lhs == rhs)
});
static RGBMSW_NE: Function = operator_def!("__ne__", RGBMSW_SELF, RGBMSW_ARG0, |py, o, a, _| {
    let lhs = this_rgbmsw!(o, a).get();
    let rhs = place_arg!(o, a, 1, RgbmswPlace).get();
    ok(py, lhs != rhs)
});

// .def_readwrite("red", &GradingRGBMSW::m_red) to "width" (PyGradingData.cpp:180-191 @ v2.5.2).
number_member!(
    RGBMSW_GET_RED,
    RGBMSW_SET_RED,
    RGBMSW_SELF,
    f64,
    this_rgbmsw,
    red
);
number_member!(
    RGBMSW_GET_GREEN,
    RGBMSW_SET_GREEN,
    RGBMSW_SELF,
    f64,
    this_rgbmsw,
    green
);
number_member!(
    RGBMSW_GET_BLUE,
    RGBMSW_SET_BLUE,
    RGBMSW_SELF,
    f64,
    this_rgbmsw,
    blue
);
number_member!(
    RGBMSW_GET_MASTER,
    RGBMSW_SET_MASTER,
    RGBMSW_SELF,
    f64,
    this_rgbmsw,
    master
);
number_member!(
    RGBMSW_GET_START,
    RGBMSW_SET_START,
    RGBMSW_SELF,
    f64,
    this_rgbmsw,
    start
);
number_member!(
    RGBMSW_GET_WIDTH,
    RGBMSW_SET_WIDTH,
    RGBMSW_SELF,
    f64,
    this_rgbmsw,
    width
);

// defRepr(clsGradingRGBMSW) (PyGradingData.cpp:193 @ v2.5.2).
static RGBMSW_REPR: Function = method_def!("__repr__", [RGBMSW_SELF], "str", |py, o, a, _| {
    let v = this_rgbmsw!(o, a).get();
    repr_out(py, |os| v.write_to(os))
});

// ---------------------------------------------------------------------------------------------
// GradingTone

const TONE_SELF: Param = Param::new("self", GRADING_TONE);
const TONE_ARG0: Param = Param::new("arg0", GRADING_TONE);

macro_rules! this_tone {
    ($o:expr, $a:expr) => {
        shared_arg!($o, $a, 0, GradingTone)
    };
}

// .def(py::init<GradingStyle>()) (PyGradingData.cpp:196-197 @ v2.5.2).
static TONE_INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    operator: false,
    overloads: &[Overload {
        params: &[TONE_SELF, STYLE_ARG0],
        ret: "None",
        call: |py, o, a, _| {
            let style = value_style(enum_param!(o, a, 1));
            construct(&a[0], GRADING_TONE, Shared::new(GradingTone::new(style)))?;
            none(py)
        },
        pos_only: 0,
    }],
};

// .def("validate", &GradingTone::validate) (PyGradingData.cpp:199-200 @ v2.5.2).
static TONE_VALIDATE: Function = method_def!("validate", [TONE_SELF], "None", |py, o, a, _| {
    let v = this_tone!(o, a).get();
    py_result(py, v.validate())?;
    none(py)
});

// .def("__eq__", ...), .def("__ne__", ...) (PyGradingData.cpp:202-209 @ v2.5.2).
static TONE_EQ: Function = operator_def!("__eq__", TONE_SELF, TONE_ARG0, |py, o, a, _| {
    let lhs = this_tone!(o, a).get();
    let rhs = shared_arg!(o, a, 1, GradingTone).get();
    ok(py, lhs == rhs)
});
static TONE_NE: Function = operator_def!("__ne__", TONE_SELF, TONE_ARG0, |py, o, a, _| {
    let lhs = this_tone!(o, a).get();
    let rhs = shared_arg!(o, a, 1, GradingTone).get();
    ok(py, lhs != rhs)
});

/// `def_readwrite` of a `GradingRGBMSW` member of `GradingTone` (see [`rgbm_member`]).
macro_rules! rgbmsw_member {
    ($get:ident, $set:ident, $field:ident) => {
        static $get: Function = method_def!("", [TONE_SELF], GRADING_RGBMSW, |_py, o, a, _| {
            let rc = this_tone!(o, a);
            field_out(
                &a[0],
                GRADING_RGBMSW,
                rc,
                |t: &mut GradingTone| &mut t.$field,
                offset_of!(GradingTone, $field),
            )
        });
        static $set: Function = method_def!("", [TONE_SELF, RGBMSW_ARG0], "None", |py, o, a, _| {
            let rc = this_tone!(o, a);
            let v = place_arg!(o, a, 1, RgbmswPlace).get();
            rc.update(|t| t.$field = v);
            none(py)
        });
    };
}

// .def_readwrite("blacks", &GradingTone::m_blacks) to "scontrast" (PyGradingData.cpp:211-222 @
// v2.5.2).
rgbmsw_member!(TONE_GET_BLACKS, TONE_SET_BLACKS, blacks);
rgbmsw_member!(TONE_GET_WHITES, TONE_SET_WHITES, whites);
rgbmsw_member!(TONE_GET_SHADOWS, TONE_SET_SHADOWS, shadows);
rgbmsw_member!(TONE_GET_HIGHLIGHTS, TONE_SET_HIGHLIGHTS, highlights);
rgbmsw_member!(TONE_GET_MIDTONES, TONE_SET_MIDTONES, midtones);
number_member!(
    TONE_GET_SCONTRAST,
    TONE_SET_SCONTRAST,
    TONE_SELF,
    f64,
    this_tone,
    scontrast
);

// defRepr(clsGradingTone) (PyGradingData.cpp:224 @ v2.5.2).
static TONE_REPR: Function = method_def!("__repr__", [TONE_SELF], "str", |py, o, a, _| {
    let v = this_tone!(o, a).get();
    repr_out(py, |os| v.write_to(os))
});

// ---------------------------------------------------------------------------------------------
// GradingControlPoint

const POINT_SELF: Param = Param::new("self", GRADING_CONTROL_POINT);
const POINT_ARG0: Param = Param::new("arg0", GRADING_CONTROL_POINT);

macro_rules! this_point {
    ($o:expr, $a:expr) => {
        shared_arg!($o, $a, 0, GradingControlPoint)
    };
}

/// `0.0`, the default of each coordinate (`DEFAULT_CONTROL_POINT.m_x`, `m_y`).
fn zero(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    Ok(pyo3::types::PyFloat::new(py, 0.0).into_any())
}

// .def(py::init<>()), .def(py::init<float, float>(), "x"_a = DEFAULT_CONTROL_POINT.m_x, "y"_a
// = DEFAULT_CONTROL_POINT.m_y) (PyGradingData.cpp:40, 227-232 @ v2.5.2).
static POINT_INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[POINT_SELF],
            ret: "None",
            call: |py, _, a, _| {
                let rc = Shared::new(GradingControlPoint::default());
                construct(&a[0], GRADING_CONTROL_POINT, rc)?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                POINT_SELF,
                Param {
                    name: "x",
                    ty: FLOAT,
                    default: default("0.0", zero),
                },
                Param {
                    name: "y",
                    ty: FLOAT,
                    default: default("0.0", zero),
                },
            ],
            ret: "None",
            call: |py, _, a, convert| {
                let x = crate::arg!(a, 1, convert, f32);
                let y = crate::arg!(a, 2, convert, f32);
                let rc = Shared::new(GradingControlPoint::new(x, y));
                construct(&a[0], GRADING_CONTROL_POINT, rc)?;
                none(py)
            },
            pos_only: 0,
        },
    ],
};

// .def("__eq__", ...), .def("__ne__", ...) (PyGradingData.cpp:234-241 @ v2.5.2).
static POINT_EQ: Function = operator_def!("__eq__", POINT_SELF, POINT_ARG0, |py, o, a, _| {
    let lhs = this_point!(o, a).get();
    let rhs = shared_arg!(o, a, 1, GradingControlPoint).get();
    ok(py, lhs == rhs)
});
static POINT_NE: Function = operator_def!("__ne__", POINT_SELF, POINT_ARG0, |py, o, a, _| {
    let lhs = this_point!(o, a).get();
    let rhs = shared_arg!(o, a, 1, GradingControlPoint).get();
    ok(py, lhs != rhs)
});

// .def_readwrite("x", &GradingControlPoint::m_x), "y" (PyGradingData.cpp:243-246 @ v2.5.2).
number_member!(POINT_GET_X, POINT_SET_X, POINT_SELF, f32, this_point, x);
number_member!(POINT_GET_Y, POINT_SET_Y, POINT_SELF, f32, this_point, y);

// defRepr(clsGradingControlPoint) (PyGradingData.cpp:248 @ v2.5.2).
static POINT_REPR: Function = method_def!("__repr__", [POINT_SELF], "str", |py, o, a, _| {
    let v = this_point!(o, a).get();
    repr_out(py, |os| v.write_to(os))
});

// ---------------------------------------------------------------------------------------------
// The curves
//
// pybind11 holds `GradingBSplineCurve`, `GradingRGBCurve` and `GradingHueCurve` by their
// `shared_ptr`s, and an RGB or hue curve's properties return the very curves it holds: the
// binding holds a curve as a [`BSplineRc`], and an RGB or hue curve as the shared handles of its
// curves ([`RgbCurveNode`], [`HueCurveNode`]), from which it builds the library's value when it
// calls the library.

/// A `GradingBSplineCurve` as pybind11's holder shares it.
pub(crate) type BSplineRc = Shared<GradingBSplineCurve>;

/// A `GradingRGBCurve` as the binding holds it: its four curves' shared handles.
#[derive(Debug)]
pub(crate) struct RgbCurveNode {
    /// `m_curves`, in `RgbCurveType::CURVES`'s order.
    pub(crate) curves: [BSplineRc; 4],
}

/// A `GradingRGBCurve` shared by the Python objects that hold it.
pub(crate) type RgbCurveRc = Shared<RgbCurveNode>;

impl RgbCurveNode {
    /// New curves of the values of `value`'s (`createEditableCopy` of each).
    pub(crate) fn new(value: &GradingRgbCurve) -> RgbCurveNode {
        RgbCurveNode {
            curves: value.curves().clone().map(Shared::new),
        }
    }
}

/// The library's value of the RGB curve `rc` now.
pub(crate) fn rgb_curve_value(rc: &RgbCurveRc) -> GradingRgbCurve {
    let curves = rc.lock().curves.clone();
    let [r, g, b, m] = curves.map(|c| c.lock().clone());
    GradingRgbCurve::with_curves(&r, &g, &b, &m)
}

/// A `GradingHueCurve` as the binding holds it: its eight curves' shared handles and its
/// draw-curve-only flag.
#[derive(Debug)]
pub(crate) struct HueCurveNode {
    /// `m_curves`, in [`HueCurveType::CURVES`]'s order.
    pub(crate) curves: [BSplineRc; 8],
    /// `m_drawCurveOnly`.
    pub(crate) draw_curve_only: bool,
}

/// A `GradingHueCurve` shared by the Python objects that hold it.
pub(crate) type HueCurveRc = Shared<HueCurveNode>;

impl HueCurveNode {
    /// New curves of the values of `value`'s (`createEditableCopy` of each).
    pub(crate) fn new(value: &GradingHueCurve) -> HueCurveNode {
        HueCurveNode {
            curves: value.curves().clone().map(Shared::new),
            draw_curve_only: value.get_draw_curve_only(),
        }
    }
}

/// The library's value of the hue curve `rc` now, as it is (not validated).
pub(crate) fn hue_curve_value(rc: &HueCurveRc) -> GradingHueCurve {
    let (curves, draw) = {
        let node = rc.lock();
        (node.curves.clone(), node.draw_curve_only)
    };
    let mut value = GradingHueCurve::new(GradingStyle::Log);
    for (c, curve) in HueCurveType::CURVES.into_iter().zip(curves) {
        // Each curve of the enumerators has its place.
        if let Ok(slot) = value.curve_mut(c) {
            *slot = curve.lock().clone();
        }
    }
    value.set_draw_curve_only(draw);
    value
}

/// The value object that `getValue` of a `GradingRGBCurveTransform` or
/// `GradingHueCurveTransform` returned: the transform's own curves, as upstream's
/// `getValue` returns them, which Python changes in place (chunk 6.7b). The transform's node
/// holds it until the library replaces the value (`setValue`, `setSlope`, a new style).
#[derive(Debug, Clone)]
pub(crate) enum CurveValue {
    Rgb(RgbCurveRc),
    Hue(HueCurveRc),
}

impl CurveValue {
    /// The curves now into the transform `t`, as they are (not validated:
    /// `set_value_as_is`), as upstream's transform holds what Python changed.
    pub(crate) fn write_back(&self, t: &mut ocio::Transform) {
        match (self, t) {
            (CurveValue::Rgb(rc), ocio::Transform::GradingRgbCurve(t)) => {
                t.set_value_as_is(&rgb_curve_value(rc));
            }
            (CurveValue::Hue(rc), ocio::Transform::GradingHueCurve(t)) => {
                t.set_value_as_is(&hue_curve_value(rc));
            }
            _ => {}
        }
    }
}

/// A library result whose error can't happen here (a curve of the enumerators, an index below
/// the count), as a Python error all the same.
fn unreachable_error<T>(r: ocio::Result<T>) -> PyResult<T> {
    r.map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
}

/// The port's error where the wheel dereferences a null curve: `None` for a curve, or for the
/// RGB or hue curve whose property is read or set (U-167).
fn null_curve(what: &str) -> PyErr {
    pyo3::exceptions::PyValueError::new_err(format!(
        "{what}: None for a curve, a null pointer the wheel dereferences (its process dies; \
         docs/improvements.md, U-167)"
    ))
}

/// `std::vector<T>` of `n` elements of `size` bytes, made (`resize` false: its constructor) or
/// grown from `old` elements (`resize`: its `resize`), as the wheel's C++ runtime allocates it:
/// more than `max_size()` elements throw `std::length_error` (pybind11's ValueError), an
/// allocation that fails throws `std::bad_alloc` (MemoryError); MSVC's, for 4096 bytes or more,
/// adds 47 bytes for its alignment first, and throws `std::bad_array_new_length` when that
/// overflows. The port checks before the library allocates (whose allocation failure would
/// abort the process), and holds the memory until the returned value goes, as the wheel holds
/// each vector while it makes the next (docs/improvements.md, I-366).
fn vector_alloc(n: usize, size: usize, old: usize, resize: bool) -> PyResult<Vec<u8>> {
    use pyo3::exceptions::{PyMemoryError, PyValueError};
    if resize && n <= old {
        return Ok(Vec::new());
    }
    let ptrdiff_max = isize::MAX.unsigned_abs();
    // MSVC: min(PTRDIFF_MAX, SIZE_MAX / sizeof(T)); libstdc++: PTRDIFF_MAX / sizeof(T).
    #[cfg(windows)]
    let (max, too_long, bad_alloc) = (
        ptrdiff_max.min(usize::MAX / size),
        "vector too long",
        "bad allocation",
    );
    #[cfg(not(windows))]
    let (max, too_long, bad_alloc) = (
        ptrdiff_max / size,
        if resize {
            "vector::_M_default_append"
        } else {
            "cannot create std::vector larger than max_size()"
        },
        "std::bad_alloc",
    );
    if n > max {
        return Err(PyValueError::new_err(too_long));
    }
    let mut bytes = n * size;
    if cfg!(windows) && bytes >= 4096 {
        bytes = bytes
            .checked_add(47)
            .ok_or_else(|| PyMemoryError::new_err("bad array new length"))?;
    }
    let mut reserved = Vec::new();
    reserved
        .try_reserve_exact(bytes)
        .map_err(|_| PyMemoryError::new_err(bad_alloc))?;
    Ok(reserved)
}

/// The memory a curve of `n` control points takes, made (`old` None) or grown from `old`
/// points: its control points (8 bytes each), then its slopes (4 bytes each), as
/// `GradingBSplineCurveImpl(size_t)` and `setNumControlPoints` allocate them
/// (src/OpenColorIO/ops/gradingrgbcurve/GradingBSplineCurve.cpp:556-559, 602-606 @ v2.5.2).
fn curve_alloc(n: usize, old: Option<usize>) -> PyResult<()> {
    let (old, resize) = (old.unwrap_or(0), old.is_some());
    let _points = vector_alloc(n, std::mem::size_of::<GradingControlPoint>(), old, resize)?;
    let _slopes = vector_alloc(n, std::mem::size_of::<f32>(), old, resize)?;
    Ok(())
}

/// The library's `BSplineType` of the C++ value `v`, which the curve stores as it is: the
/// port's error for one outside the enumerators (U-112).
fn spline_type(v: i64) -> PyResult<BSplineType> {
    use BSplineType::*;
    [
        BSpline,
        DiagonalBSpline,
        HueHueBSpline,
        Periodic1BSpline,
        Periodic0BSpline,
        Horizontal1BSpline,
    ]
    .into_iter()
    .find(|t| *t as i64 == v)
    .ok_or_else(|| outside_enumerators(B_SPLINE_TYPE, v))
}

/// The library's `HueCurveType` of the C++ value `v`, as `GetBSplineTypeForHueCurveType` reads
/// it: one outside the enumerators is the switch's `default`, as `HUE_NUM_CURVES`
/// (src/OpenColorIO/ops/gradinghuecurve/GradingHueCurve.cpp:254-278 @ v2.5.2).
fn hue_curve_type(v: i64) -> HueCurveType {
    HueCurveType::CURVES
        .into_iter()
        .find(|t| *t as i64 == v)
        .unwrap_or(HueCurveType::NumCurves)
}

/// The library's curve of `values` (x, y pairs) of the spline type `t`, or the binding's errors.
///
/// Port of the lambdas of `GradingBSplineCurve`'s constructors from values
/// (src/bindings/python/PyGradingData.cpp:273-317 @ v2.5.2).
fn curve_of_values(
    py: Python<'_>,
    values: &[f32],
    t: BSplineType,
) -> PyResult<GradingBSplineCurve> {
    let size = values.len();
    if size < 4 {
        return Err(to_py_err(
            py,
            &ocio::Exception::new("GradingBSpline needs at least 4 values."),
        ));
    }
    if !size.is_multiple_of(2) {
        return Err(to_py_err(
            py,
            &ocio::Exception::new("GradingBSpline needs an even number of values."),
        ));
    }
    let points: Vec<GradingControlPoint> = values
        .as_chunks::<2>()
        .0
        .iter()
        .map(|[x, y]| GradingControlPoint::new(*x, *y))
        .collect();
    Ok(GradingBSplineCurve::with_points_and_spline_type(&points, t))
}

/// `CopyGradingBSpline(to, from)`: `from`'s spline type, control points and slopes into `to`.
///
/// Port of `CopyGradingBSpline` (src/bindings/python/PyGradingData.cpp:22-33 @ v2.5.2).
fn copy_bspline(to: &BSplineRc, from: &BSplineRc) -> PyResult<()> {
    let from = from.lock().clone();
    let mut to = to.lock();
    to.set_spline_type(from.spline_type());
    let num = from.num_control_points();
    curve_alloc(num, Some(to.num_control_points()))?;
    to.set_num_control_points(num);
    for pt in 0..num {
        *unreachable_error(to.control_point_mut(pt))? = *unreachable_error(from.control_point(pt))?;
        unreachable_error(to.set_slope(pt, unreachable_error(from.slope(pt))?))?;
    }
    Ok(())
}

pub(crate) const GRADING_B_SPLINE_CURVE: &str = q!("GradingBSplineCurve");
const CONTROL_POINT_ITERATOR: &str = q!("GradingBSplineCurve.GradingControlPointIterator");
pub(crate) const GRADING_RGB_CURVE: &str = q!("GradingRGBCurve");
pub(crate) const GRADING_HUE_CURVE: &str = q!("GradingHueCurve");
const HUE_CURVE_TYPE: &str = q!("HueCurveType");
const B_SPLINE_TYPE: &str = q!("BSplineType");
const FLOATS: &str = "collections.abc.Sequence[typing.SupportsFloat]";

const CURVE_SELF: Param = Param::new("self", GRADING_B_SPLINE_CURVE);
const CURVE_ARG0: Param = Param::new("arg0", GRADING_B_SPLINE_CURVE);

macro_rules! this_curve {
    ($o:expr, $a:expr) => {
        shared_arg!($o, $a, 0, GradingBSplineCurve)
    };
}

/// A new `GradingBSplineCurve` object `this` holding `curve`.
fn construct_curve(this: &Bound<'_, PyAny>, curve: GradingBSplineCurve) -> PyResult<()> {
    construct(this, GRADING_B_SPLINE_CURVE, Shared::new(curve))
}

/// The curve `rc` as a returned `GradingBSplineCurveRcPtr`: its registered object, or a new one.
pub(crate) fn curve_out(py: Python<'_>, rc: &BSplineRc) -> Out {
    out(py, GRADING_B_SPLINE_CURVE, rc)
}

// .def(py::init([](size_t size) {...}), "size"_a), with "huecurvetype"_a, with "splinetype"_a,
// .def(py::init([](const std::vector<float> & values) {...})), with a HueCurveType
// (PyGradingData.cpp:251-317 @ v2.5.2).
static CURVE_INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[CURVE_SELF, Param::new("size", INT)],
            ret: "None",
            call: |py, _, a, convert| {
                let size = crate::arg!(a, 1, convert, u64) as usize;
                curve_alloc(size, None)?;
                construct_curve(&a[0], GradingBSplineCurve::new(size))?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                CURVE_SELF,
                Param::new("size", INT),
                Param::new("huecurvetype", HUE_CURVE_TYPE),
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let size = crate::arg!(a, 1, convert, u64) as usize;
                let t = hue_curve_type(enum_param!(o, a, 2));
                curve_alloc(size, None)?;
                construct_curve(&a[0], GradingBSplineCurve::with_hue_curve_type(size, t))?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                CURVE_SELF,
                Param::new("size", INT),
                Param::new("splinetype", B_SPLINE_TYPE),
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let size = crate::arg!(a, 1, convert, u64) as usize;
                let v = enum_param!(o, a, 2);
                curve_alloc(size, None)?;
                let t = spline_type(v)?;
                construct_curve(&a[0], GradingBSplineCurve::with_spline_type(size, t))?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[CURVE_SELF, Param::new("arg0", FLOATS)],
            ret: "None",
            call: |py, _, a, convert| {
                let values = crate::arg!(a, 1, convert, Vec<f32>);
                let curve = curve_of_values(py, &values, BSplineType::BSpline)?;
                construct_curve(&a[0], curve)?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                CURVE_SELF,
                Param::new("arg0", FLOATS),
                Param::new("arg1", HUE_CURVE_TYPE),
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let values = crate::arg!(a, 1, convert, Vec<f32>);
                let t = hue_curve_type(enum_param!(o, a, 2)).b_spline_type();
                let curve = curve_of_values(py, &values, t)?;
                construct_curve(&a[0], curve)?;
                none(py)
            },
            pos_only: 0,
        },
    ],
};

// .def("__eq__", ...), .def("__ne__", ...) (PyGradingData.cpp:319-326 @ v2.5.2).
static CURVE_EQ: Function = operator_def!("__eq__", CURVE_SELF, CURVE_ARG0, |py, o, a, _| {
    let lhs = this_curve!(o, a).lock().clone();
    let rhs = shared_arg!(o, a, 1, GradingBSplineCurve).lock().clone();
    ok(py, lhs == rhs)
});
static CURVE_NE: Function = operator_def!("__ne__", CURVE_SELF, CURVE_ARG0, |py, o, a, _| {
    let lhs = this_curve!(o, a).lock().clone();
    let rhs = shared_arg!(o, a, 1, GradingBSplineCurve).lock().clone();
    ok(py, lhs != rhs)
});

// .def("validate", &GradingBSplineCurve::validate) (PyGradingData.cpp:328-329 @ v2.5.2).
static CURVE_VALIDATE: Function = method_def!("validate", [CURVE_SELF], "None", |py, o, a, _| {
    let curve = this_curve!(o, a).lock().clone();
    py_result(py, curve.validate())?;
    none(py)
});

// .def("setNumControlPoints", &GradingBSplineCurve::setNumControlPoints, "size"_a)
// (PyGradingData.cpp:330-331 @ v2.5.2).
static CURVE_SET_NUM: Function = method_def!(
    "setNumControlPoints",
    [CURVE_SELF, Param::new("size", INT)],
    "None",
    |py, o, a, convert| {
        let rc = this_curve!(o, a);
        let size = crate::arg!(a, 1, convert, u64) as usize;
        let mut curve = rc.lock();
        curve_alloc(size, Some(curve.num_control_points()))?;
        curve.set_num_control_points(size);
        none(py)
    }
);

// .def("getControlPoints", [](GradingBSplineCurveRcPtr & self) { return
// GradingControlPointIterator(self); }) (PyGradingData.cpp:332-335 @ v2.5.2).
static CURVE_GET_POINTS: Function = method_def!(
    "getControlPoints",
    [CURVE_SELF],
    CONTROL_POINT_ITERATOR,
    |py, o, a, _| {
        let rc = this_curve!(o, a);
        let it = new_iterator(py, CONTROL_POINT_ITERATOR, Some(Box::new(rc)))?;
        Ok(Some(it.unbind()))
    }
);

// .def("getSplineType", &GradingBSplineCurve::getSplineType) (PyGradingData.cpp:336-337 @
// v2.5.2).
static CURVE_GET_TYPE: Function = method_def!(
    "getSplineType",
    [CURVE_SELF],
    B_SPLINE_TYPE,
    |py, o, a, _| {
        let t = this_curve!(o, a).lock().spline_type();
        enum_out(py, B_SPLINE_TYPE, t as i64)
    }
);

// .def("setSplineType", &GradingBSplineCurve::setSplineType, "splinetype"_a)
// (PyGradingData.cpp:338-339 @ v2.5.2).
static CURVE_SET_TYPE: Function = method_def!(
    "setSplineType",
    [CURVE_SELF, Param::new("splinetype", B_SPLINE_TYPE)],
    "None",
    |py, o, a, _| {
        let rc = this_curve!(o, a);
        let t = spline_type(enum_param!(o, a, 1))?;
        rc.lock().set_spline_type(t);
        none(py)
    }
);

// .def("slopesAreDefault", &GradingBSplineCurve::slopesAreDefault) (PyGradingData.cpp:340-341
// @ v2.5.2).
static CURVE_SLOPES_DEFAULT: Function =
    method_def!("slopesAreDefault", [CURVE_SELF], "bool", |py, o, a, _| {
        let default = this_curve!(o, a).lock().slopes_are_default();
        ok(py, default)
    });

// .def("getSlopes", [](GradingBSplineCurveRcPtr self) {...}) (PyGradingData.cpp:342-352 @
// v2.5.2).
static CURVE_GET_SLOPES: Function =
    method_def!("getSlopes", [CURVE_SELF], "list[float]", |py, o, a, _| {
        let slopes: Vec<f64> = this_curve!(o, a)
            .lock()
            .slopes()
            .iter()
            .map(|&s| f64::from(s))
            .collect();
        ok(py, slopes)
    });

// .def("setSlopes", [](GradingBSplineCurveRcPtr self, const std::vector<float> & slopes)
// {...}) (PyGradingData.cpp:353-365 @ v2.5.2).
static CURVE_SET_SLOPES: Function = method_def!(
    "setSlopes",
    [CURVE_SELF, Param::new("arg0", FLOATS)],
    "None",
    |py, o, a, convert| {
        let rc = this_curve!(o, a);
        let slopes = crate::arg!(a, 1, convert, Vec<f32>);
        let mut curve = rc.lock();
        if slopes.len() != curve.num_control_points() {
            return Err(to_py_err(
                py,
                &ocio::Exception::new(
                    "Length of slopes vector must match number of control points.",
                ),
            ));
        }
        for (pt, slope) in slopes.into_iter().enumerate() {
            py_result(py, curve.set_slope(pt, slope))?;
        }
        none(py)
    }
);

// defRepr(clsGradingBSplineCurve) (PyGradingData.cpp:367 @ v2.5.2; PyUtils.h:17-26).
static CURVE_REPR: Function = method_def!("__repr__", [CURVE_SELF], "str", |py, o, a, _| {
    let curve = this_curve!(o, a).lock().clone();
    repr_out(py, |os| curve.write_to(os))
});

// The iterator (PyGradingData.cpp:369-393 @ v2.5.2).

const ITER_SELF: Param = Param::new("self", CONTROL_POINT_ITERATOR);

/// The curve an iterator of `self` walks, or the next overload.
macro_rules! iter_curve {
    ($o:expr, $a:expr) => {{
        let it = this_iter!($o, $a);
        let rc = it.get().obj::<BSplineRc>()?.clone();
        (it, rc)
    }};
}

/// Control point `i` of the curve (`getControlPoint(size_t)`, its index converted from an
/// `int`): a copy.
fn point_out(py: Python<'_>, rc: &BSplineRc, i: i32) -> Out {
    let point = *py_result(py, rc.lock().control_point(i as usize))?;
    Ok(Some(
        to_py(py, GRADING_CONTROL_POINT, &Shared::new(point))?.unbind(),
    ))
}

// .def("__len__", [](GradingControlPointIterator & it) { return
// it.m_obj->getNumControlPoints(); }).
static ITER_LEN: Function = method_def!("__len__", [ITER_SELF], "int", |py, o, a, _| {
    let (_, rc) = iter_curve!(o, a);
    let n = rc.lock().num_control_points();
    ok(py, n)
});

// .def("__getitem__", [](GradingControlPointIterator & it, int i) { return
// it.m_obj->getControlPoint(i); }).
static ITER_GETITEM: Function = method_def!(
    "__getitem__",
    [ITER_SELF, Param::new("arg0", INT)],
    GRADING_CONTROL_POINT,
    |py, o, a, convert| {
        let (_, rc) = iter_curve!(o, a);
        let i = crate::arg!(a, 1, convert, i32);
        point_out(py, &rc, i)
    }
);

// .def("__setitem__", [](GradingControlPointIterator & it, int i, const GradingControlPoint &
// cpt) { it.m_obj->getControlPoint(i) = cpt; }).
static ITER_SETITEM: Function = method_def!(
    "__setitem__",
    [
        ITER_SELF,
        Param::new("arg0", INT),
        Param::new("arg1", GRADING_CONTROL_POINT)
    ],
    "None",
    |py, o, a, convert| {
        let (_, rc) = iter_curve!(o, a);
        let i = crate::arg!(a, 1, convert, i32);
        let point = shared_arg!(o, a, 2, GradingControlPoint).get();
        *py_result(py, rc.lock().control_point_mut(i as usize))? = point;
        none(py)
    }
);

// .def("__iter__", [](GradingControlPointIterator & it) -> GradingControlPointIterator & {
// return it; }).
static ITER_ITER: Function = method_def!(
    "__iter__",
    [ITER_SELF],
    CONTROL_POINT_ITERATOR,
    |_, o, a, _| {
        // Never reads the curve: an iterator made of `None` as `self` returns itself (U-134).
        let _ = this_iter!(o, a);
        Ok(Some(a[0].clone().unbind()))
    }
);

// .def("__next__", [](GradingControlPointIterator & it) {...}).
static ITER_NEXT: Function = method_def!(
    "__next__",
    [ITER_SELF],
    GRADING_CONTROL_POINT,
    |py, o, a, _| {
        let (it, rc) = iter_curve!(o, a);
        let num = rc.lock().num_control_points() as i32;
        let i = it.get().next_index(num)?;
        point_out(py, &rc, i)
    }
);

// ---------------------------------------------------------------------------------------------
// GradingRGBCurve

const RGB_SELF: Param = Param::new("self", GRADING_RGB_CURVE);
const RGB_ARG0: Param = Param::new("arg0", GRADING_RGB_CURVE);

macro_rules! this_rgb {
    ($o:expr, $a:expr) => {
        shared_arg!($o, $a, 0, RgbCurveNode)
    };
}

/// The repr of the default of each curve parameter of `GradingRGBCurve`'s constructor
/// (`DEFAULT_RGB_CURVE->getCurve(...)`, a curve of `GRADING_LOG`), as pybind11 writes it in the
/// signature: the curve's `__repr__`.
const DEFAULT_CURVE_REPR: &str = "<control_points=[<x=0, y=0><x=0.5, y=0.5><x=1, y=1>]>";

/// A new object of the default curve of `GRADING_LOG`, each curve parameter's default value.
fn default_curve(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    to_py(
        py,
        GRADING_B_SPLINE_CURVE,
        &Shared::new(ocio_ops::ops::gradingrgbcurve::grading_rgb_curve::default_curve()),
    )
}

const fn rgb_curve_param(name: &'static str) -> Param {
    Param {
        name,
        ty: GRADING_B_SPLINE_CURVE,
        default: default(DEFAULT_CURVE_REPR, default_curve),
    }
}

/// A curve argument (`const GradingBSplineCurveRcPtr &`, pybind11's holder caster: with
/// conversions, `None` too, a null pointer), or the next overload.
macro_rules! curve_holder_arg {
    ($o:expr, $a:expr, $i:expr, $convert:expr) => {
        match holder_arg::<GradingBSplineCurve>(&$a[$i], $o.params[$i].ty, $convert)? {
            Some(rc) => rc,
            None => return Ok(None),
        }
    };
}

// .def(py::init([](GradingStyle style) {...}), "style"_a), .def(py::init([](red, green, blue,
// master) {...}), "red"_a = DEFAULT_RGB_CURVE->getCurve(RGB_RED), ...) (PyGradingData.cpp:39,
// 396-413 @ v2.5.2).
static RGB_INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[RGB_SELF, Param::new("style", GRADING_STYLE)],
            ret: "None",
            call: |py, o, a, _| {
                let style = value_style(enum_param!(o, a, 1));
                let node = RgbCurveNode::new(&GradingRgbCurve::new(style));
                construct(&a[0], GRADING_RGB_CURVE, Shared::new(node))?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                RGB_SELF,
                rgb_curve_param("red"),
                rgb_curve_param("green"),
                rgb_curve_param("blue"),
                rgb_curve_param("master"),
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let red = curve_holder_arg!(o, a, 1, convert);
                let green = curve_holder_arg!(o, a, 2, convert);
                let blue = curve_holder_arg!(o, a, 3, convert);
                let master = curve_holder_arg!(o, a, 4, convert);
                // GradingRGBCurveImpl(red, green, blue, master) (src/OpenColorIO/ops/
                // gradingrgbcurve/GradingRGBCurve.cpp:30-44 @ v2.5.2).
                let (Some(red), Some(green), Some(blue), Some(master)) = (red, green, blue, master)
                else {
                    return Err(to_py_err(
                        py,
                        &ocio::Exception::new("All curves have to be defined"),
                    ));
                };
                let curves = [red, green, blue, master].map(|c| Shared::new(c.lock().clone()));
                construct(
                    &a[0],
                    GRADING_RGB_CURVE,
                    Shared::new(RgbCurveNode { curves }),
                )?;
                none(py)
            },
            pos_only: 0,
        },
    ],
};

// .def("__eq__", ...), .def("__ne__", ...) (PyGradingData.cpp:415-422 @ v2.5.2).
static RGB_EQ: Function = operator_def!("__eq__", RGB_SELF, RGB_ARG0, |py, o, a, _| {
    let lhs = rgb_curve_value(&this_rgb!(o, a));
    let rhs = rgb_curve_value(&shared_arg!(o, a, 1, RgbCurveNode));
    ok(py, lhs == rhs)
});
static RGB_NE: Function = operator_def!("__ne__", RGB_SELF, RGB_ARG0, |py, o, a, _| {
    let lhs = rgb_curve_value(&this_rgb!(o, a));
    let rhs = rgb_curve_value(&shared_arg!(o, a, 1, RgbCurveNode));
    ok(py, lhs != rhs)
});

// .def("validate", &GradingRGBCurve::validate), .def("isIdentity", &GradingRGBCurve::isIdentity)
// (PyGradingData.cpp:424-427 @ v2.5.2).
static RGB_VALIDATE: Function = method_def!("validate", [RGB_SELF], "None", |py, o, a, _| {
    let value = rgb_curve_value(&this_rgb!(o, a));
    py_result(py, value.validate())?;
    none(py)
});
static RGB_IS_IDENTITY: Function = method_def!("isIdentity", [RGB_SELF], "bool", |py, o, a, _| {
    let value = rgb_curve_value(&this_rgb!(o, a));
    ok(py, value.is_identity())
});

/// A curve property of an RGB or hue curve (`def_property` of two lambdas taking the curve's
/// holder: its getter returns the curve it holds, its setter copies the value in with
/// `CopyGradingBSpline`), as statics `$get` and `$set`: `$node` is the node type, `$class` the
/// qualified class, `$i` the curve's index.
macro_rules! curve_property {
    ($get:ident, $set:ident, $node:ty, $class:expr, $i:expr) => {
        static $get: Function = Function {
            name: "",
            constructor: false,
            method: false,
            operator: false,
            overloads: &[Overload {
                params: &[Param::new("arg0", $class)],
                ret: GRADING_B_SPLINE_CURVE,
                call: |py, o, a, convert| {
                    let Some(rc) = holder_arg::<$node>(&a[0], o.params[0].ty, convert)? else {
                        return Ok(None);
                    };
                    let rc = rc.ok_or_else(|| null_curve($class))?;
                    let curve = rc.lock().curves[$i].clone();
                    curve_out(py, &curve)
                },
                pos_only: 0,
            }],
        };
        static $set: Function = Function {
            name: "",
            constructor: false,
            method: false,
            operator: false,
            overloads: &[Overload {
                params: &[
                    Param::new("arg0", $class),
                    Param::new("arg1", GRADING_B_SPLINE_CURVE),
                ],
                ret: "None",
                call: |py, o, a, convert| {
                    let Some(rc) = holder_arg::<$node>(&a[0], o.params[0].ty, convert)? else {
                        return Ok(None);
                    };
                    let from = curve_holder_arg!(o, a, 1, convert);
                    let rc = rc.ok_or_else(|| null_curve($class))?;
                    let from = from.ok_or_else(|| null_curve(GRADING_B_SPLINE_CURVE))?;
                    let to = rc.lock().curves[$i].clone();
                    copy_bspline(&to, &from)?;
                    none(py)
                },
                pos_only: 0,
            }],
        };
    };
}

// .def_property("red", ...) to "master" (PyGradingData.cpp:429-468 @ v2.5.2).
curve_property!(RGB_GET_RED, RGB_SET_RED, RgbCurveNode, GRADING_RGB_CURVE, 0);
curve_property!(
    RGB_GET_GREEN,
    RGB_SET_GREEN,
    RgbCurveNode,
    GRADING_RGB_CURVE,
    1
);
curve_property!(
    RGB_GET_BLUE,
    RGB_SET_BLUE,
    RgbCurveNode,
    GRADING_RGB_CURVE,
    2
);
curve_property!(
    RGB_GET_MASTER,
    RGB_SET_MASTER,
    RgbCurveNode,
    GRADING_RGB_CURVE,
    3
);

// ---------------------------------------------------------------------------------------------
// GradingHueCurve

const HUE_SELF: Param = Param::new("self", GRADING_HUE_CURVE);
const HUE_ARG0: Param = Param::new("arg0", GRADING_HUE_CURVE);

macro_rules! this_hue {
    ($o:expr, $a:expr) => {
        shared_arg!($o, $a, 0, HueCurveNode)
    };
}

fn hue_repr<'py>(
    py: Python<'py>,
    o: &Overload,
    a: &[Bound<'py, PyAny>],
    _: bool,
) -> PyResult<Option<Py<PyAny>>> {
    let value = hue_curve_value(&this_hue!(o, a));
    repr_out(py, |os| value.write_to(os))
}

// defRepr(clsGradingHueCurve), twice (PyGradingData.cpp:470, 592 @ v2.5.2): the second makes a
// second overload of the same function, which no call reaches.
static HUE_REPR: Function = Function {
    name: "__repr__",
    constructor: false,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[HUE_SELF],
            ret: "str",
            call: hue_repr,
            pos_only: 0,
        },
        Overload {
            params: &[HUE_SELF],
            ret: "str",
            call: hue_repr,
            pos_only: 0,
        },
    ],
};

/// One of the eight unnamed curve parameters of `GradingHueCurve`'s second constructor.
const fn hue_arg(name: &'static str) -> Param {
    Param::new(name, GRADING_B_SPLINE_CURVE)
}

// .def(py::init([](GradingStyle style) {...}), "style"_a), .def(py::init([](hue_hue, hue_sat,
// hue_lum, lum_sat, sat_sat, lum_lum, sat_lum, hue_fx) {...})) (PyGradingData.cpp:472-491 @
// v2.5.2).
static HUE_INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[HUE_SELF, Param::new("style", GRADING_STYLE)],
            ret: "None",
            call: |py, o, a, _| {
                let style = value_style(enum_param!(o, a, 1));
                let node = HueCurveNode::new(&GradingHueCurve::new(style));
                construct(&a[0], GRADING_HUE_CURVE, Shared::new(node))?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                HUE_SELF,
                hue_arg("arg0"),
                hue_arg("arg1"),
                hue_arg("arg2"),
                hue_arg("arg3"),
                hue_arg("arg4"),
                hue_arg("arg5"),
                hue_arg("arg6"),
                hue_arg("arg7"),
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let mut curves = Vec::with_capacity(8);
                for (arg, param) in a.iter().zip(o.params).skip(1) {
                    match holder_arg::<GradingBSplineCurve>(arg, param.ty, convert)? {
                        Some(rc) => curves.push(rc),
                        None => return Ok(None),
                    }
                }
                // GradingHueCurveImpl's constructor (src/OpenColorIO/ops/gradinghuecurve/
                // GradingHueCurve.cpp:99-117 @ v2.5.2) copies each curve through its pointer.
                let mut values = Vec::with_capacity(8);
                for c in curves {
                    let c = c.ok_or_else(|| null_curve(GRADING_HUE_CURVE))?;
                    values.push(c.lock().clone());
                }
                let value = py_result(
                    py,
                    GradingHueCurve::with_curves(
                        &values[0], &values[1], &values[2], &values[3], &values[4], &values[5],
                        &values[6], &values[7],
                    ),
                )?;
                let node = HueCurveNode::new(&value);
                construct(&a[0], GRADING_HUE_CURVE, Shared::new(node))?;
                none(py)
            },
            pos_only: 0,
        },
    ],
};

// .def("__eq__", ...), .def("__ne__", ...) (PyGradingData.cpp:493-500 @ v2.5.2).
static HUE_EQ: Function = operator_def!("__eq__", HUE_SELF, HUE_ARG0, |py, o, a, _| {
    let lhs = hue_curve_value(&this_hue!(o, a));
    let rhs = hue_curve_value(&shared_arg!(o, a, 1, HueCurveNode));
    ok(py, lhs == rhs)
});
static HUE_NE: Function = operator_def!("__ne__", HUE_SELF, HUE_ARG0, |py, o, a, _| {
    let lhs = hue_curve_value(&this_hue!(o, a));
    let rhs = hue_curve_value(&shared_arg!(o, a, 1, HueCurveNode));
    ok(py, lhs != rhs)
});

// .def("validate", ...), .def("isIdentity", ...), .def("getDrawCurveOnly", ...),
// .def("setDrawCurveOnly", ..., "drawcurveonly"_a) (PyGradingData.cpp:502-509 @ v2.5.2).
static HUE_VALIDATE: Function = method_def!("validate", [HUE_SELF], "None", |py, o, a, _| {
    let value = hue_curve_value(&this_hue!(o, a));
    py_result(py, value.validate())?;
    none(py)
});
static HUE_IS_IDENTITY: Function = method_def!("isIdentity", [HUE_SELF], "bool", |py, o, a, _| {
    let value = hue_curve_value(&this_hue!(o, a));
    ok(py, value.is_identity())
});
static HUE_GET_DRAW: Function =
    method_def!("getDrawCurveOnly", [HUE_SELF], "bool", |py, o, a, _| {
        let draw = this_hue!(o, a).lock().draw_curve_only;
        ok(py, draw)
    });
static HUE_SET_DRAW: Function = method_def!(
    "setDrawCurveOnly",
    [HUE_SELF, Param::new("drawcurveonly", "bool")],
    "None",
    |py, o, a, convert| {
        let rc = this_hue!(o, a);
        let draw = crate::arg!(a, 1, convert, bool);
        rc.lock().draw_curve_only = draw;
        none(py)
    }
);

// .def_property("hue_hue", ...) to "hue_fx" (PyGradingData.cpp:511-590 @ v2.5.2).
curve_property!(
    HUE_GET_HUE_HUE,
    HUE_SET_HUE_HUE,
    HueCurveNode,
    GRADING_HUE_CURVE,
    0
);
curve_property!(
    HUE_GET_HUE_SAT,
    HUE_SET_HUE_SAT,
    HueCurveNode,
    GRADING_HUE_CURVE,
    1
);
curve_property!(
    HUE_GET_HUE_LUM,
    HUE_SET_HUE_LUM,
    HueCurveNode,
    GRADING_HUE_CURVE,
    2
);
curve_property!(
    HUE_GET_LUM_SAT,
    HUE_SET_LUM_SAT,
    HueCurveNode,
    GRADING_HUE_CURVE,
    3
);
curve_property!(
    HUE_GET_SAT_SAT,
    HUE_SET_SAT_SAT,
    HueCurveNode,
    GRADING_HUE_CURVE,
    4
);
curve_property!(
    HUE_GET_LUM_LUM,
    HUE_SET_LUM_LUM,
    HueCurveNode,
    GRADING_HUE_CURVE,
    5
);
curve_property!(
    HUE_GET_SAT_LUM,
    HUE_SET_SAT_LUM,
    HueCurveNode,
    GRADING_HUE_CURVE,
    6
);
curve_property!(
    HUE_GET_HUE_FX,
    HUE_SET_HUE_FX,
    HueCurveNode,
    GRADING_HUE_CURVE,
    7
);

// ---------------------------------------------------------------------------------------------
// The bindings

/// `cls.def("__eq__", ...)`, then the `__hash__` None pybind11 sets for a class that defines
/// `__eq__`, then `cls.def("__ne__", ...)`.
fn def_eq_ne(
    cls: &Bound<'_, pyo3::types::PyType>,
    eq: &'static Function,
    ne: &'static Function,
) -> PyResult<()> {
    def(cls, eq)?;
    cls.setattr("__hash__", cls.py().None())?;
    def(cls, ne)
}

/// Port of `bindPyGradingData` (src/bindings/python/PyGradingData.cpp:37-593 @ v2.5.2), in its
/// order: the values (78-248), the B-spline curve and its iterator (250-393), the RGB curve
/// (395-468) and the hue curve (470-592).
pub(crate) fn bind_py_grading_data(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();

    let rgbm = class(py, GRADING_RGBM)?;
    def(&rgbm, &RGBM_INIT)?;
    def_eq_ne(&rgbm, &RGBM_EQ, &RGBM_NE)?;
    def_readwrite(&rgbm, "red", &RGBM_GET_RED, &RGBM_SET_RED)?;
    def_readwrite(&rgbm, "green", &RGBM_GET_GREEN, &RGBM_SET_GREEN)?;
    def_readwrite(&rgbm, "blue", &RGBM_GET_BLUE, &RGBM_SET_BLUE)?;
    def_readwrite(&rgbm, "master", &RGBM_GET_MASTER, &RGBM_SET_MASTER)?;
    def(&rgbm, &RGBM_REPR)?;

    let primary = class(py, GRADING_PRIMARY)?;
    def(&primary, &PRIMARY_INIT)?;
    def_eq_ne(&primary, &PRIMARY_EQ, &PRIMARY_NE)?;
    def(&primary, &PRIMARY_VALIDATE)?;
    for (name, get, set) in [
        (
            "brightness",
            &PRIMARY_GET_BRIGHTNESS,
            &PRIMARY_SET_BRIGHTNESS,
        ),
        ("contrast", &PRIMARY_GET_CONTRAST, &PRIMARY_SET_CONTRAST),
        ("gamma", &PRIMARY_GET_GAMMA, &PRIMARY_SET_GAMMA),
        ("offset", &PRIMARY_GET_OFFSET, &PRIMARY_SET_OFFSET),
        ("exposure", &PRIMARY_GET_EXPOSURE, &PRIMARY_SET_EXPOSURE),
        ("lift", &PRIMARY_GET_LIFT, &PRIMARY_SET_LIFT),
        ("gain", &PRIMARY_GET_GAIN, &PRIMARY_SET_GAIN),
        ("pivot", &PRIMARY_GET_PIVOT, &PRIMARY_SET_PIVOT),
        (
            "saturation",
            &PRIMARY_GET_SATURATION,
            &PRIMARY_SET_SATURATION,
        ),
        (
            "clampWhite",
            &PRIMARY_GET_CLAMP_WHITE,
            &PRIMARY_SET_CLAMP_WHITE,
        ),
        (
            "clampBlack",
            &PRIMARY_GET_CLAMP_BLACK,
            &PRIMARY_SET_CLAMP_BLACK,
        ),
        (
            "pivotWhite",
            &PRIMARY_GET_PIVOT_WHITE,
            &PRIMARY_SET_PIVOT_WHITE,
        ),
        (
            "pivotBlack",
            &PRIMARY_GET_PIVOT_BLACK,
            &PRIMARY_SET_PIVOT_BLACK,
        ),
    ] {
        def_readwrite(&primary, name, get, set)?;
    }
    def_property_readonly_static(&primary, "NoClampBlack", &NO_CLAMP_BLACK)?;
    def_property_readonly_static(&primary, "NoClampWhite", &NO_CLAMP_WHITE)?;
    def(&primary, &PRIMARY_REPR)?;

    let rgbmsw = class(py, GRADING_RGBMSW)?;
    def(&rgbmsw, &RGBMSW_INIT)?;
    def_eq_ne(&rgbmsw, &RGBMSW_EQ, &RGBMSW_NE)?;
    for (name, get, set) in [
        ("red", &RGBMSW_GET_RED, &RGBMSW_SET_RED),
        ("green", &RGBMSW_GET_GREEN, &RGBMSW_SET_GREEN),
        ("blue", &RGBMSW_GET_BLUE, &RGBMSW_SET_BLUE),
        ("master", &RGBMSW_GET_MASTER, &RGBMSW_SET_MASTER),
        ("start", &RGBMSW_GET_START, &RGBMSW_SET_START),
        ("width", &RGBMSW_GET_WIDTH, &RGBMSW_SET_WIDTH),
    ] {
        def_readwrite(&rgbmsw, name, get, set)?;
    }
    def(&rgbmsw, &RGBMSW_REPR)?;

    let tone = class(py, GRADING_TONE)?;
    def(&tone, &TONE_INIT)?;
    def(&tone, &TONE_VALIDATE)?;
    def_eq_ne(&tone, &TONE_EQ, &TONE_NE)?;
    for (name, get, set) in [
        ("blacks", &TONE_GET_BLACKS, &TONE_SET_BLACKS),
        ("whites", &TONE_GET_WHITES, &TONE_SET_WHITES),
        ("shadows", &TONE_GET_SHADOWS, &TONE_SET_SHADOWS),
        ("highlights", &TONE_GET_HIGHLIGHTS, &TONE_SET_HIGHLIGHTS),
        ("midtones", &TONE_GET_MIDTONES, &TONE_SET_MIDTONES),
        ("scontrast", &TONE_GET_SCONTRAST, &TONE_SET_SCONTRAST),
    ] {
        def_readwrite(&tone, name, get, set)?;
    }
    def(&tone, &TONE_REPR)?;

    let point = class(py, GRADING_CONTROL_POINT)?;
    def(&point, &POINT_INIT)?;
    def_eq_ne(&point, &POINT_EQ, &POINT_NE)?;
    def_readwrite(&point, "x", &POINT_GET_X, &POINT_SET_X)?;
    def_readwrite(&point, "y", &POINT_GET_Y, &POINT_SET_Y)?;
    def(&point, &POINT_REPR)?;

    let curve = class(py, GRADING_B_SPLINE_CURVE)?;
    let iterator = nested_class(&curve, "GradingBSplineCurve", "GradingControlPointIterator")?;
    def(&curve, &CURVE_INIT)?;
    def_eq_ne(&curve, &CURVE_EQ, &CURVE_NE)?;
    for f in [
        &CURVE_VALIDATE,
        &CURVE_SET_NUM,
        &CURVE_GET_POINTS,
        &CURVE_GET_TYPE,
        &CURVE_SET_TYPE,
        &CURVE_SLOPES_DEFAULT,
        &CURVE_GET_SLOPES,
        &CURVE_SET_SLOPES,
        &CURVE_REPR,
    ] {
        def(&curve, f)?;
    }
    for f in [
        &ITER_LEN,
        &ITER_GETITEM,
        &ITER_SETITEM,
        &ITER_ITER,
        &ITER_NEXT,
    ] {
        def(&iterator, f)?;
    }

    let rgb = class(py, GRADING_RGB_CURVE)?;
    def(&rgb, &RGB_INIT)?;
    def_eq_ne(&rgb, &RGB_EQ, &RGB_NE)?;
    def(&rgb, &RGB_VALIDATE)?;
    def(&rgb, &RGB_IS_IDENTITY)?;
    for (name, get, set) in [
        ("red", &RGB_GET_RED, &RGB_SET_RED),
        ("green", &RGB_GET_GREEN, &RGB_SET_GREEN),
        ("blue", &RGB_GET_BLUE, &RGB_SET_BLUE),
        ("master", &RGB_GET_MASTER, &RGB_SET_MASTER),
    ] {
        def_property(&rgb, name, get, set)?;
    }

    // GradingHueCurve: its __repr__ first (470), with the second overload of 592.
    let hue = class(py, GRADING_HUE_CURVE)?;
    def(&hue, &HUE_REPR)?;
    def(&hue, &HUE_INIT)?;
    def_eq_ne(&hue, &HUE_EQ, &HUE_NE)?;
    for f in [
        &HUE_VALIDATE,
        &HUE_IS_IDENTITY,
        &HUE_GET_DRAW,
        &HUE_SET_DRAW,
    ] {
        def(&hue, f)?;
    }
    for (name, get, set) in [
        ("hue_hue", &HUE_GET_HUE_HUE, &HUE_SET_HUE_HUE),
        ("hue_sat", &HUE_GET_HUE_SAT, &HUE_SET_HUE_SAT),
        ("hue_lum", &HUE_GET_HUE_LUM, &HUE_SET_HUE_LUM),
        ("lum_sat", &HUE_GET_LUM_SAT, &HUE_SET_LUM_SAT),
        ("sat_sat", &HUE_GET_SAT_SAT, &HUE_SET_SAT_SAT),
        ("lum_lum", &HUE_GET_LUM_LUM, &HUE_SET_LUM_LUM),
        ("sat_lum", &HUE_GET_SAT_LUM, &HUE_SET_SAT_LUM),
        ("hue_fx", &HUE_GET_HUE_FX, &HUE_SET_HUE_FX),
    ] {
        def_property(&hue, name, get, set)?;
    }
    Ok(())
}
