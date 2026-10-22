// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyCDLTransform` (src/bindings/python/transforms/PyCDLTransform.cpp @ v2.5.2).
//!
//! `CreateFromFile` and `CreateGroupFromFile` return new transforms each call: the library
//! returns copies of the file cache's CDLs, where upstream returns the cached ones
//! (docs/improvements.md, I-174).

use ocio::{CdlTransform, Transform};
use ocio_ops::open_color_types::CdlStyle;
use ocio_ops::utils::string_utils::c_str;
use pyo3::prelude::*;

use crate::dispatch::{Function, NonNullCharPtr, Overload, Param, StdString};
use crate::py_format_metadata::Shape;
use crate::py_open_color_io::py_result;
use crate::py_transform::{
    DIRECTION_DEFAULT, FLOAT, TransformRc, class, construct, def, def_static, default, enum_out,
    float_list, none, ok, set_direction, string_out, transform_out, unknown_style, validate, with,
};
use crate::{enum_param, float_array, float_array_out, method_def, q, ref_rc, static_def, this_rc};

const CDL_TRANSFORM: &str = q!("CDLTransform");
const GROUP_TRANSFORM: &str = q!("GroupTransform");
pub(crate) const CDL_STYLE: &str = q!("CDLStyle");
const SELF: Param = Param::new("self", CDL_TRANSFORM);
const ARRAY_3: &str = float_array!("3");

/// The library's `CdlStyle` of the C++ value `v`, if it is an enumerator.
pub(crate) fn cdl_style(v: i64) -> Option<CdlStyle> {
    [CdlStyle::Asc, CdlStyle::NoClamp]
        .into_iter()
        .find(|s| *s as i64 == v)
}

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

const fn empty_string(name: &'static str) -> Param {
    Param {
        name,
        ty: "str",
        default: default("''", |py| Ok(pyo3::types::PyString::new(py, "").into_any())),
    }
}

// .def(py::init(&CDLTransform::Create)), .def(py::init([](dir) {...}), "direction"_a =
// DEFAULT->getDirection()), .def(py::init([](slope, offset, power, sat, id, desc, dir) {...}),
// "slope"_a = DEFAULT_SLOPE, ..., "direction"_a = DEFAULT->getDirection())
// (PyCDLTransform.cpp:26-64 @ v2.5.2).
static INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    overloads: &[
        Overload {
            params: &[SELF],
            ret: "None",
            call: |py, o, a, _| {
                let rc = TransformRc::new(Transform::Cdl(CdlTransform::new()));
                construct(&a[0], o.params[0].ty, rc)?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[SELF, DIRECTION_DEFAULT],
            ret: "None",
            call: |py, o, a, _| {
                let dir = enum_param!(o, a, 1);
                let rc = TransformRc::new(Transform::Cdl(CdlTransform::new()));
                set_direction(&rc, dir);
                validate(py, &rc)?;
                construct(&a[0], o.params[0].ty, rc)?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                array3("slope", true),
                array3("offset", false),
                array3("power", true),
                Param {
                    name: "sat",
                    ty: FLOAT,
                    default: default(
                        "1.0",
                        |py| Ok(pyo3::types::PyFloat::new(py, 1.0).into_any()),
                    ),
                },
                empty_string("id"),
                empty_string("description"),
                DIRECTION_DEFAULT,
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let slope = crate::arg!(a, 1, convert, [f64; 3]);
                let offset = crate::arg!(a, 2, convert, [f64; 3]);
                let power = crate::arg!(a, 3, convert, [f64; 3]);
                let sat = crate::arg!(a, 4, convert, f64);
                let StdString(id) = crate::arg!(a, 5, convert, StdString);
                let StdString(desc) = crate::arg!(a, 6, convert, StdString);
                let dir = enum_param!(o, a, 7);
                let mut t = CdlTransform::new();
                t.set_slope(&slope);
                t.set_offset(&offset);
                t.set_power(&power);
                t.set_sat(sat);
                if !id.is_empty() {
                    t.set_id(c_str(&id));
                }
                if !desc.is_empty() {
                    t.set_first_sop_description(c_str(&desc));
                }
                let rc = TransformRc::new(Transform::Cdl(t));
                set_direction(&rc, dir);
                validate(py, &rc)?;
                construct(&a[0], o.params[0].ty, rc)?;
                none(py)
            },
            pos_only: 0,
        },
    ],
};

// .def_static("CreateFromFile", &CDLTransform::CreateFromFile, "src"_a.none(false),
// "id"_a.none(false)) (PyCDLTransform.cpp:65-68 @ v2.5.2).
static CREATE_FROM_FILE: Function = static_def!(
    "CreateFromFile",
    [Param::new("src", "str"), Param::new("id", "str")],
    CDL_TRANSFORM,
    |py, _, a, convert| {
        let NonNullCharPtr(src) = crate::arg!(a, 0, convert, NonNullCharPtr);
        let NonNullCharPtr(id) = crate::arg!(a, 1, convert, NonNullCharPtr);
        let t = py_result(py, CdlTransform::from_file(&src, &id))?;
        transform_out(py, Some(&TransformRc::new(Transform::Cdl(t))))
    }
);

// .def_static("CreateGroupFromFile", &CDLTransform::CreateGroupFromFile, "src"_a.none(false))
// (PyCDLTransform.cpp:69-71 @ v2.5.2).
static CREATE_GROUP_FROM_FILE: Function = static_def!(
    "CreateGroupFromFile",
    [Param::new("src", "str")],
    GROUP_TRANSFORM,
    |py, _, a, convert| {
        let NonNullCharPtr(src) = crate::arg!(a, 0, convert, NonNullCharPtr);
        let group = py_result(py, CdlTransform::group_from_file(&src))?;
        transform_out(py, Some(&TransformRc::new(Transform::Group(group))))
    }
);

// .def("getFormatMetadata", ..., py::return_value_policy::reference_internal)
// (PyCDLTransform.cpp:73-76 @ v2.5.2).
static GET_FORMAT_METADATA: Function = crate::format_metadata_def!(SELF);

// .def("setStyle", &CDLTransform::setStyle, "style"_a) (PyCDLTransform.cpp:77-78 @ v2.5.2).
static SET_STYLE: Function = method_def!(
    "setStyle",
    [SELF, Param::new("style", CDL_STYLE)],
    "None",
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        // The library converts it at once (`CDLTransformImpl::setStyle`, CDLTransform.cpp:182-186
        // @ v2.5.2): the wheel's error for one outside the enumerators.
        let v = enum_param!(o, a, 1);
        let style =
            cdl_style(v).ok_or_else(|| unknown_style(py, "Unknown CDL transform style: ", v))?;
        with(&rc, |t: &mut CdlTransform| t.set_style(style))?;
        none(py)
    }
);

// .def("equals", &CDLTransform::equals, "other"_a) (PyCDLTransform.cpp:79-80 @ v2.5.2).
static EQUALS: Function = method_def!(
    "equals",
    [SELF, Param::new("other", CDL_TRANSFORM)],
    "bool",
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        let other = ref_rc!(o, a, 1);
        let equal = crate::py_transform::equals::<CdlTransform>(&rc, &other, |x, y| x.equals(y))?;
        ok(py, equal)
    }
);

// .def("getStyle", &CDLTransform::getStyle) (PyCDLTransform.cpp:81-82 @ v2.5.2).
static GET_STYLE: Function = method_def!("getStyle", [SELF], CDL_STYLE, |py, o, a, _| {
    let rc = this_rc!(o, a);
    let style = with(&rc, |t: &mut CdlTransform| t.style())?;
    enum_out(py, CDL_STYLE, style as i64)
});

/// A getter and a setter ("rgb"_a) of three values (PyCDLTransform.cpp:83-121 @ v2.5.2).
macro_rules! rgb_methods {
    ($get:ident, $set:ident, $get_name:literal, $set_name:literal, $getter:ident,
     $setter:ident) => {
        static $get: Function =
            method_def!($get_name, [SELF], float_array_out!("3"), |py, o, a, _| {
                let rc = this_rc!(o, a);
                ok(py, with(&rc, |t: &mut CdlTransform| t.$getter())?.to_vec())
            });
        static $set: Function = method_def!(
            $set_name,
            [SELF, Param::new("rgb", ARRAY_3)],
            "None",
            |py, o, a, convert| {
                let rc = this_rc!(o, a);
                let rgb = crate::arg!(a, 1, convert, [f64; 3]);
                with(&rc, |t: &mut CdlTransform| t.$setter(&rgb))?;
                none(py)
            }
        );
    };
}

rgb_methods!(
    GET_SLOPE, SET_SLOPE, "getSlope", "setSlope", slope, set_slope
);
rgb_methods!(
    GET_OFFSET,
    SET_OFFSET,
    "getOffset",
    "setOffset",
    offset,
    set_offset
);
rgb_methods!(
    GET_POWER, SET_POWER, "getPower", "setPower", power, set_power
);

// .def("getSOP", ...), .def("setSOP", ..., "vec9"_a) (PyCDLTransform.cpp:122-134 @ v2.5.2).
static GET_SOP: Function = method_def!("getSOP", [SELF], float_array_out!("9"), |py, o, a, _| {
    let rc = this_rc!(o, a);
    ok(py, with(&rc, |t: &mut CdlTransform| t.sop())?.to_vec())
});

static SET_SOP: Function = method_def!(
    "setSOP",
    [SELF, Param::new("vec9", float_array!("9"))],
    "None",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let vec9 = crate::arg!(a, 1, convert, [f64; 9]);
        with(&rc, |t: &mut CdlTransform| t.set_sop(&vec9))?;
        none(py)
    }
);

// .def("getSat", ...), .def("setSat", ..., "sat"_a), .def("getSatLumaCoefs", ...)
// (PyCDLTransform.cpp:135-145 @ v2.5.2).
static GET_SAT: Function = method_def!("getSat", [SELF], "float", |py, o, a, _| {
    let rc = this_rc!(o, a);
    ok(py, with(&rc, |t: &mut CdlTransform| t.sat())?)
});

static SET_SAT: Function = method_def!(
    "setSat",
    [SELF, Param::new("sat", FLOAT)],
    "None",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let sat = crate::arg!(a, 1, convert, f64);
        with(&rc, |t: &mut CdlTransform| t.set_sat(sat))?;
        none(py)
    }
);

static GET_SAT_LUMA_COEFS: Function = method_def!(
    "getSatLumaCoefs",
    [SELF],
    float_array_out!("3"),
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        ok(
            py,
            with(&rc, |t: &mut CdlTransform| t.sat_luma_coefs())?.to_vec(),
        )
    }
);

// .def("getID", ...), .def("setID", ..., "id"_a.none(false)) (PyCDLTransform.cpp:146-149 @
// v2.5.2).
static GET_ID: Function = method_def!("getID", [SELF], "str", |py, o, a, _| {
    let rc = this_rc!(o, a);
    let id = with(&rc, |t: &mut CdlTransform| t.id().to_vec())?;
    string_out(py, &id)
});

static SET_ID: Function = method_def!(
    "setID",
    [SELF, Param::new("id", "str")],
    "None",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let NonNullCharPtr(id) = crate::arg!(a, 1, convert, NonNullCharPtr);
        with(&rc, |t: &mut CdlTransform| t.set_id(&id))?;
        none(py)
    }
);

// .def("getFirstSOPDescription", ...), .def("setFirstSOPDescription", ...,
// "description"_a.none(false)) (PyCDLTransform.cpp:150-154 @ v2.5.2).
static GET_FIRST_SOP_DESCRIPTION: Function =
    method_def!("getFirstSOPDescription", [SELF], "str", |py, o, a, _| {
        let rc = this_rc!(o, a);
        let description = with(&rc, |t: &mut CdlTransform| {
            t.first_sop_description().to_vec()
        })?;
        string_out(py, &description)
    });

static SET_FIRST_SOP_DESCRIPTION: Function = method_def!(
    "setFirstSOPDescription",
    [SELF, Param::new("description", "str")],
    "None",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let NonNullCharPtr(description) = crate::arg!(a, 1, convert, NonNullCharPtr);
        set_first_sop_description(&rc, &description)?;
        none(py)
    }
);

/// Port of `METADATA_SOP_DESCRIPTION` (src/OpenColorIO/transforms/CDLTransform.h:19 @ v2.5.2).
const METADATA_SOP_DESCRIPTION: &[u8] = b"SOPDescription";

/// `setFirstSOPDescription` of the node, and what it does to the wheel's buffer of the
/// metadata's child elements (`CDLTransformImpl::setFirstSOPDescription`, CDLTransform.cpp:
/// 345-367 @ v2.5.2): a description added (`emplace_back`), or the first one erased.
fn set_first_sop_description(rc: &TransformRc, description: &[u8]) -> PyResult<()> {
    let mut node = rc.lock();
    let node = &mut *node;
    let Transform::Cdl(t) = &mut node.transform else {
        return Err(pyo3::exceptions::PyRuntimeError::new_err(
            "the instance holds another transform class",
        ));
    };
    let before = Shape::of(t.format_metadata());
    let first = t
        .format_metadata()
        .get_first_child_index(METADATA_SOP_DESCRIPTION);
    t.set_first_sop_description(description);
    let len = t.format_metadata().get_children_elements().len();
    match (len.cmp(&before.num_children()), first) {
        (std::cmp::Ordering::Greater, _) => node.metadata_heap.push(&[], before.num_children()),
        (std::cmp::Ordering::Less, Some(index)) => node.metadata_heap.erase(&[], index, &before),
        _ => {}
    }
    Ok(())
}

static REPR: Function = crate::repr_def!(SELF);

/// Port of `bindPyCDLTransform` (src/bindings/python/transforms/PyCDLTransform.cpp:9-157 @
/// v2.5.2), in its order.
pub(crate) fn bind(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let cls = class(m.py(), CDL_TRANSFORM)?;
    def(&cls, &INIT)?;
    def_static(&cls, &CREATE_FROM_FILE)?;
    def_static(&cls, &CREATE_GROUP_FROM_FILE)?;
    for f in [
        &GET_FORMAT_METADATA,
        &SET_STYLE,
        &EQUALS,
        &GET_STYLE,
        &GET_SLOPE,
        &SET_SLOPE,
        &GET_OFFSET,
        &SET_OFFSET,
        &GET_POWER,
        &SET_POWER,
        &GET_SOP,
        &SET_SOP,
        &GET_SAT,
        &SET_SAT,
        &GET_SAT_LUMA_COEFS,
        &GET_ID,
        &SET_ID,
        &GET_FIRST_SOP_DESCRIPTION,
        &SET_FIRST_SOP_DESCRIPTION,
        &REPR,
    ] {
        def(&cls, f)?;
    }
    Ok(())
}
