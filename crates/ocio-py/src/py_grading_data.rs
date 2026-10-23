// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyGradingData` (src/bindings/python/PyGradingData.cpp @ v2.5.2): the grading values
//! (chunk 6.7a).
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
use ocio_ops::open_color_types::GradingStyle;
use ocio_ops::ops::gradingprimary::grading_primary::{GradingPrimary, GradingRgbm};
use ocio_ops::ops::gradingrgbcurve::grading_b_spline_curve::GradingControlPoint;
use ocio_ops::ops::gradingtone::grading_tone::{GradingRgbmsw, GradingTone};
use pyo3::prelude::*;

use crate::dispatch::{Function, Overload, Param};
use crate::py_open_color_io::py_result;
use crate::py_transform::{
    FLOAT, Out, class, def, def_property_readonly_static, def_readwrite, default, none, ok,
    string_out,
};
use crate::shared::{Shared, construct, construct_with, holder, holder_with, to_py_with};
use crate::{enum_param, method_def, q};

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
fn value_style(v: i64) -> GradingStyle {
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
/// order: the values (78-248).
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
    Ok(())
}
