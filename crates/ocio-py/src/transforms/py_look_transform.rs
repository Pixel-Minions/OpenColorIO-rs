// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyLookTransform` (src/bindings/python/transforms/PyLookTransform.cpp @ v2.5.2).

use ocio::{LookTransform, Transform};
use ocio_ops::utils::string_utils::c_str;
use pyo3::prelude::*;

use crate::dispatch::{Function, NonNullCharPtr, Overload, Param, StdString};
use crate::py_transform::{
    DIRECTION_DEFAULT, TransformRc, class, construct, def, none, ok, set_direction, string_out,
    validate, with,
};
use crate::transforms::py_color_space_transform::{bool_param, empty_string};
use crate::{enum_param, method_def, q, this_rc};

const LOOK_TRANSFORM: &str = q!("LookTransform");
const SELF: Param = Param::new("self", LOOK_TRANSFORM);

// .def(py::init(&LookTransform::Create)), .def(py::init([](src, dst, looks,
// skipColorSpaceConversion, dir) {...}), "src"_a.none(false), "dst"_a.none(false),
// "looks"_a.none(false) = DEFAULT->getLooks(), "skipColorSpaceConversion"_a =
// DEFAULT->getSkipColorSpaceConversion(), "direction"_a = DEFAULT->getDirection())
// (PyLookTransform.cpp:17-40 @ v2.5.2).
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
                let rc = TransformRc::new(Transform::Look(LookTransform::new()));
                construct(&a[0], o.params[0].ty, rc)?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param::new("src", "str"),
                Param::new("dst", "str"),
                empty_string("looks"),
                bool_param("skipColorSpaceConversion", false),
                DIRECTION_DEFAULT,
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let StdString(src) = crate::arg!(a, 1, convert, StdString);
                let StdString(dst) = crate::arg!(a, 2, convert, StdString);
                let StdString(looks) = crate::arg!(a, 3, convert, StdString);
                let skip = crate::arg!(a, 4, convert, bool);
                let dir = enum_param!(o, a, 5);
                let mut t = LookTransform::new();
                if !src.is_empty() {
                    t.set_src(c_str(&src));
                }
                if !dst.is_empty() {
                    t.set_dst(c_str(&dst));
                }
                if !looks.is_empty() {
                    t.set_looks(c_str(&looks));
                }
                t.set_skip_color_space_conversion(skip);
                let rc = TransformRc::new(Transform::Look(t));
                set_direction(&rc, dir);
                validate(py, &rc)?;
                construct(&a[0], o.params[0].ty, rc)?;
                none(py)
            },
            pos_only: 0,
        },
    ],
};

/// A getter of the class's text `$get` and a setter of it whose argument can't be `None`
/// (`"<name>"_a.none(false)`).
macro_rules! text_accessors {
    ($getter:ident, $setter:ident, $get_name:literal, $set_name:literal, $param:literal,
     $get:ident, $set:ident) => {
        static $getter: Function = method_def!($get_name, [SELF], "str", |py, o, a, _| {
            let rc = this_rc!(o, a);
            let text = with(&rc, |t: &mut LookTransform| t.$get().to_vec())?;
            string_out(py, &text)
        });

        static $setter: Function = method_def!(
            $set_name,
            [SELF, Param::new($param, "str")],
            "None",
            |py, o, a, convert| {
                let rc = this_rc!(o, a);
                let NonNullCharPtr(text) = crate::arg!(a, 1, convert, NonNullCharPtr);
                with(&rc, |t: &mut LookTransform| t.$set(&text))?;
                none(py)
            }
        );
    };
}

// .def("getSrc", ...), .def("setSrc", ..., "src"_a.none(false)) (PyLookTransform.cpp:41-44 @
// v2.5.2).
text_accessors!(GET_SRC, SET_SRC, "getSrc", "setSrc", "src", src, set_src);
// .def("getDst", ...), .def("setDst", ..., "dst"_a.none(false)) (PyLookTransform.cpp:45-48 @
// v2.5.2).
text_accessors!(GET_DST, SET_DST, "getDst", "setDst", "dst", dst, set_dst);
// .def("getLooks", ...), .def("setLooks", ..., "looks"_a.none(false)) (PyLookTransform.cpp:
// 49-52 @ v2.5.2).
text_accessors!(
    GET_LOOKS, SET_LOOKS, "getLooks", "setLooks", "looks", looks, set_looks
);

// .def("getSkipColorSpaceConversion", ...), .def("setSkipColorSpaceConversion", ...,
// "skipColorSpaceConversion"_a) (PyLookTransform.cpp:53-57 @ v2.5.2).
static GET_SKIP: Function = method_def!(
    "getSkipColorSpaceConversion",
    [SELF],
    "bool",
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        ok(
            py,
            with(&rc, |t: &mut LookTransform| t.skip_color_space_conversion())?,
        )
    }
);

static SET_SKIP: Function = method_def!(
    "setSkipColorSpaceConversion",
    [SELF, Param::new("skipColorSpaceConversion", "bool")],
    "None",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let skip = crate::arg!(a, 1, convert, bool);
        with(&rc, |t: &mut LookTransform| {
            t.set_skip_color_space_conversion(skip)
        })?;
        none(py)
    }
);

static REPR: Function = crate::repr_def!(SELF);

/// Port of `bindPyLookTransform` (src/bindings/python/transforms/PyLookTransform.cpp:9-60 @
/// v2.5.2), in its order.
pub(crate) fn bind(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let cls = class(m.py(), LOOK_TRANSFORM)?;
    for f in [
        &INIT, &GET_SRC, &SET_SRC, &GET_DST, &SET_DST, &GET_LOOKS, &SET_LOOKS, &GET_SKIP,
        &SET_SKIP, &REPR,
    ] {
        def(&cls, f)?;
    }
    Ok(())
}
