// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyFileTransform` (src/bindings/python/transforms/PyFileTransform.cpp @ v2.5.2).

use ocio::{FileTransform, Interpolation, Transform};
use ocio_ops::utils::string_utils::c_str;
use pyo3::prelude::*;
use pyo3::types::PyTuple;

use crate::dispatch::{Function, NonNullCharPtr, Overload, Param, StdString};
use crate::py_transform::{
    DIRECTION_DEFAULT, INT, TransformRc, class, construct, def, def_static, default, enum_out,
    nested_class, none, ok, outside_enumerators, set_direction, string_out, validate, with,
};
use crate::py_utils::{check_index, new_iterator};
use crate::transforms::py_cdl_transform::{CDL_STYLE, cdl_style};
use crate::{enum_param, method_def, q, static_def, this_iter, this_rc};

const FILE_TRANSFORM: &str = q!("FileTransform");
const FORMAT_ITERATOR: &str = q!("FileTransform.FormatIterator");
const INTERPOLATION: &str = q!("Interpolation");
const SELF: Param = Param::new("self", FILE_TRANSFORM);
const IT_SELF: Param = Param::new("self", FORMAT_ITERATOR);

/// The library's `Interpolation` of the C++ value `v`, if it is an enumerator.
fn interpolation(v: i64) -> Option<Interpolation> {
    use Interpolation::*;
    [Unknown, Nearest, Linear, Tetrahedral, Cubic, Default, Best]
        .into_iter()
        .find(|i| *i as i64 == v)
}

const fn empty_string(name: &'static str) -> Param {
    Param {
        name,
        ty: "str",
        default: default("''", |py| Ok(pyo3::types::PyString::new(py, "").into_any())),
    }
}

// .def(py::init(&FileTransform::Create)), .def(py::init([](src, id, interp, dir) {...}),
// "src"_a = DEFAULT->getSrc(), "cccId"_a = DEFAULT->getCCCId(), "interpolation"_a =
// DEFAULT->getInterpolation(), "direction"_a = DEFAULT->getDirection())
// (PyFileTransform.cpp:34-54 @ v2.5.2).
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
                let rc = TransformRc::new(Transform::File(FileTransform::new()));
                construct(&a[0], o.params[0].ty, rc)?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                empty_string("src"),
                empty_string("cccId"),
                Param {
                    name: "interpolation",
                    ty: INTERPOLATION,
                    default: default("<Interpolation.INTERP_DEFAULT: 254>", |py| {
                        crate::enums::to_py(&crate::enums::enum_class(py, INTERPOLATION)?, 254)
                    }),
                },
                DIRECTION_DEFAULT,
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let StdString(src) = crate::arg!(a, 1, convert, StdString);
                let StdString(id) = crate::arg!(a, 2, convert, StdString);
                let interp = enum_param!(o, a, 3);
                let dir = enum_param!(o, a, 4);
                let mut t = FileTransform::new();
                if !src.is_empty() {
                    t.set_src(c_str(&src));
                }
                if !id.is_empty() {
                    t.set_ccc_id(c_str(&id));
                }
                // The wheel stores an interpolation outside the enumerators, which `validate`
                // doesn't read (FileTransform.cpp:84-104 @ v2.5.2), so its errors come first:
                // the port refuses the value (U-112) only once `validate` passed.
                let library_interp = interpolation(interp);
                if let Some(i) = library_interp {
                    t.set_interpolation(i);
                }
                let rc = TransformRc::new(Transform::File(t));
                set_direction(&rc, dir);
                validate(py, &rc)?;
                if library_interp.is_none() {
                    return Err(outside_enumerators(INTERPOLATION, interp));
                }
                construct(&a[0], o.params[0].ty, rc)?;
                none(py)
            },
            pos_only: 0,
        },
    ],
};

// .def_static("getFormats", []() { return FormatIterator(nullptr); })
// (PyFileTransform.cpp:55-59 @ v2.5.2).
static GET_FORMATS: Function = static_def!("getFormats", [], FORMAT_ITERATOR, |py, _, _, _| {
    Ok(Some(new_iterator(py, FORMAT_ITERATOR, None)?.unbind()))
});

// .def("getSrc", ...), .def("setSrc", ..., "src"_a.none(false)) (PyFileTransform.cpp:60-63 @
// v2.5.2).
static GET_SRC: Function = method_def!("getSrc", [SELF], "str", |py, o, a, _| {
    let rc = this_rc!(o, a);
    let src = with(&rc, |t: &mut FileTransform| t.src().to_vec())?;
    string_out(py, &src)
});

static SET_SRC: Function = method_def!(
    "setSrc",
    [SELF, Param::new("src", "str")],
    "None",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let NonNullCharPtr(src) = crate::arg!(a, 1, convert, NonNullCharPtr);
        with(&rc, |t: &mut FileTransform| t.set_src(&src))?;
        none(py)
    }
);

// .def("getCCCId", ...), .def("setCCCId", ..., "cccId"_a.none(false)) (PyFileTransform.cpp:
// 64-67 @ v2.5.2).
static GET_CCC_ID: Function = method_def!("getCCCId", [SELF], "str", |py, o, a, _| {
    let rc = this_rc!(o, a);
    let id = with(&rc, |t: &mut FileTransform| t.ccc_id().to_vec())?;
    string_out(py, &id)
});

static SET_CCC_ID: Function = method_def!(
    "setCCCId",
    [SELF, Param::new("cccId", "str")],
    "None",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let NonNullCharPtr(id) = crate::arg!(a, 1, convert, NonNullCharPtr);
        with(&rc, |t: &mut FileTransform| t.set_ccc_id(&id))?;
        none(py)
    }
);

// .def("getCDLStyle", ...), .def("setCDLStyle", ..., "style"_a) (PyFileTransform.cpp:68-71 @
// v2.5.2).
static GET_CDL_STYLE: Function = method_def!("getCDLStyle", [SELF], CDL_STYLE, |py, o, a, _| {
    let rc = this_rc!(o, a);
    let style = with(&rc, |t: &mut FileTransform| t.cdl_style())?;
    enum_out(py, CDL_STYLE, style as i64)
});

static SET_CDL_STYLE: Function = method_def!(
    "setCDLStyle",
    [SELF, Param::new("style", CDL_STYLE)],
    "None",
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        let v = enum_param!(o, a, 1);
        let style = cdl_style(v).ok_or_else(|| outside_enumerators(CDL_STYLE, v))?;
        with(&rc, |t: &mut FileTransform| t.set_cdl_style(style))?;
        none(py)
    }
);

// .def("getInterpolation", ...), .def("setInterpolation", ..., "interpolation"_a)
// (PyFileTransform.cpp:72-75 @ v2.5.2).
static GET_INTERPOLATION: Function =
    method_def!("getInterpolation", [SELF], INTERPOLATION, |py, o, a, _| {
        let rc = this_rc!(o, a);
        let interp = with(&rc, |t: &mut FileTransform| t.interpolation())?;
        enum_out(py, INTERPOLATION, interp as i64)
    });

static SET_INTERPOLATION: Function = method_def!(
    "setInterpolation",
    [SELF, Param::new("interpolation", INTERPOLATION)],
    "None",
    |py, o, a, _| {
        let rc = this_rc!(o, a);
        let v = enum_param!(o, a, 1);
        let interp = interpolation(v).ok_or_else(|| outside_enumerators(INTERPOLATION, v))?;
        with(&rc, |t: &mut FileTransform| t.set_interpolation(interp))?;
        none(py)
    }
);

// .def_static("IsFormatExtensionSupported", &FileTransform::IsFormatExtensionSupported,
// "extension"_a) (PyFileTransform.cpp:76-77 @ v2.5.2): a `const char *`.
static IS_FORMAT_EXTENSION_SUPPORTED: Function = static_def!(
    "IsFormatExtensionSupported",
    [Param::new("extension", "str")],
    "bool",
    |py, _, a, convert| {
        let extension = crate::arg!(a, 0, convert, crate::dispatch::CharPtr);
        let extension = extension.0.unwrap_or_default();
        ok(py, FileTransform::is_format_extension_supported(&extension))
    }
);

static REPR: Function = crate::repr_def!(SELF);

/// `(GetFormatNameByIndex(i), GetFormatExtensionByIndex(i))`.
fn format(py: Python<'_>, i: i32) -> PyResult<Option<Py<PyAny>>> {
    let name = crate::dispatch::std_string_to_py(py, FileTransform::format_name_by_index(i))?;
    let ext = crate::dispatch::std_string_to_py(py, FileTransform::format_extension_by_index(i))?;
    Ok(Some(PyTuple::new(py, [name, ext])?.into_any().unbind()))
}

// clsFormatIterator (PyFileTransform.cpp:81-101 @ v2.5.2).
static IT_LEN: Function = method_def!("__len__", [IT_SELF], "int", |py, o, a, _| {
    let _it = this_iter!(o, a);
    ok(py, FileTransform::num_formats())
});

static IT_GETITEM: Function = method_def!(
    "__getitem__",
    [IT_SELF, Param::new("arg0", INT)],
    "tuple",
    |py, o, a, convert| {
        let _it = this_iter!(o, a);
        let i = crate::arg!(a, 1, convert, i32);
        check_index(i, FileTransform::num_formats())?;
        format(py, i)
    }
);

static IT_ITER: Function = method_def!("__iter__", [IT_SELF], FORMAT_ITERATOR, |_, o, a, _| {
    let _it = this_iter!(o, a);
    Ok(Some(a[0].clone().unbind()))
});

static IT_NEXT: Function = method_def!("__next__", [IT_SELF], "tuple", |py, o, a, _| {
    let it = this_iter!(o, a);
    let i = it.get().next_index(FileTransform::num_formats())?;
    format(py, i)
});

/// Port of `bindPyFileTransform` (src/bindings/python/transforms/PyFileTransform.cpp:21-102 @
/// v2.5.2), in its order: the iterator class nested in the class (27-32), the class's methods
/// (34-79), the iterator's (81-101).
pub(crate) fn bind(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let cls = class(m.py(), FILE_TRANSFORM)?;
    let format_iterator = nested_class(&cls, "FileTransform", "FormatIterator")?;
    def(&cls, &INIT)?;
    def_static(&cls, &GET_FORMATS)?;
    for f in [
        &GET_SRC,
        &SET_SRC,
        &GET_CCC_ID,
        &SET_CCC_ID,
        &GET_CDL_STYLE,
        &SET_CDL_STYLE,
        &GET_INTERPOLATION,
        &SET_INTERPOLATION,
    ] {
        def(&cls, f)?;
    }
    def_static(&cls, &IS_FORMAT_EXTENSION_SUPPORTED)?;
    def(&cls, &REPR)?;
    for f in [&IT_LEN, &IT_GETITEM, &IT_ITER, &IT_NEXT] {
        def(&format_iterator, f)?;
    }
    Ok(())
}
