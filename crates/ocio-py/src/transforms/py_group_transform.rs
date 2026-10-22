// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyGroupTransform` (src/bindings/python/transforms/PyGroupTransform.cpp @ v2.5.2).
//!
//! The group's children are the binding's nodes ([`TransformRc`]): appending a transform
//! shares it, as upstream's `appendTransform` shares the `TransformRcPtr`, and the library sees
//! a copy of the group with copies of its children ([`TransformRc::materialize`]).

use std::sync::Arc;

use ocio::transforms::file_format::{FILEFORMAT_CLF, FILEFORMAT_CTF};
use ocio::{GroupTransform, Transform};
use pyo3::prelude::*;
use pyo3::types::PyTuple;

use crate::dispatch::{Default, Function, Overload, Param, StdString};
use crate::py_open_color_io::{py_result, to_py_err};
use crate::py_transform::{
    DIRECTION_DEFAULT, Holes, INT, TRANSFORM, TransformRc, class, construct, def, def_static,
    holder_arg, nested_class, none, ok, set_direction, string_out, transform_out, validate,
};
use crate::py_utils::{check_index, new_iterator};
use crate::{enum_param, method_def, q, static_def, this_iter, this_rc};

const GROUP_TRANSFORM: &str = q!("GroupTransform");
const TRANSFORM_ITERATOR: &str = q!("GroupTransform.TransformIterator");
const WRITE_FORMAT_ITERATOR: &str = q!("GroupTransform.WriteFormatIterator");
const CONFIG: &str = q!("Config");
const SELF: Param = Param::new("self", GROUP_TRANSFORM);

/// A `TransformRcPtr` argument `a[i]` (its class the parameter's type; `None`, a null pointer,
/// with conversions), or the next overload.
macro_rules! holder_param {
    ($o:expr, $a:expr, $i:expr, $convert:expr) => {
        match holder_arg(&$a[$i], $o.params[$i].ty, $convert)? {
            Some(rc) => rc,
            None => return Ok(None),
        }
    };
}

/// Port of `GroupTransformImpl::getTransform` (GroupTransform.cpp:92-102 @ v2.5.2): child
/// `index`, "Invalid transform index <index>." outside the children.
fn get_transform(py: Python<'_>, rc: &TransformRc, index: i32) -> PyResult<Option<TransformRc>> {
    // The one child, not a copy of them all: iterating a group reads each child once.
    let child = usize::try_from(index)
        .ok()
        .and_then(|i| rc.lock().children.get(i).cloned());
    child.ok_or_else(|| {
        to_py_err(
            py,
            &ocio::Exception::new(format!("Invalid transform index {index}.")),
        )
    })
}

/// `getNumTransforms`.
fn num_transforms(rc: &TransformRc) -> i32 {
    rc.lock().children.len() as i32
}

/// A `Sequence[Transform]` argument (`std::vector<TransformRcPtr>`): pybind11's list caster over
/// the holder caster, whose items may be `None` with conversions.
struct Holders(Vec<Option<TransformRc>>);

impl<'py> crate::dispatch::Cast<'py> for Holders {
    /// `std::shared_ptr`: two pointers.
    const CPP_SIZE: usize = 16;

    fn load(obj: &Bound<'py, PyAny>, convert: bool) -> Option<Self> {
        Self::try_load(obj, convert).ok().flatten()
    }

    fn try_load(obj: &Bound<'py, PyAny>, convert: bool) -> PyResult<Option<Self>> {
        let Some(items) = <Vec<Item> as crate::dispatch::Cast>::try_load(obj, convert)? else {
            return Ok(None);
        };
        Ok(Some(Holders(items.into_iter().map(|i| i.0).collect())))
    }
}

/// One item of [`Holders`].
struct Item(Option<TransformRc>);

impl<'py> crate::dispatch::Cast<'py> for Item {
    const CPP_SIZE: usize = 16;

    fn load(obj: &Bound<'py, PyAny>, convert: bool) -> Option<Self> {
        Self::try_load(obj, convert).ok().flatten()
    }

    fn try_load(obj: &Bound<'py, PyAny>, convert: bool) -> PyResult<Option<Self>> {
        Ok(holder_arg(obj, TRANSFORM, convert)?.map(Item))
    }
}

// .def(py::init(&GroupTransform::Create)), .def(py::init([](transforms, dir) {...}),
// "transforms"_a = std::vector<ConstTransformRcPtr>{}, "direction"_a =
// DEFAULT->getDirection()) (PyGroupTransform.cpp:40-55 @ v2.5.2).
static INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    overloads: &[
        Overload {
            params: &[SELF],
            ret: "None",
            call: |py, o, a, _| {
                let rc = TransformRc::new(Transform::Group(GroupTransform::new()));
                construct(&a[0], o.params[0].ty, rc)?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param {
                    name: "transforms",
                    ty: "collections.abc.Sequence[PyOpenColorIO.PyOpenColorIO.Transform]",
                    default: Some(Default {
                        repr: "[]",
                        value: |py| Ok(pyo3::types::PyList::empty(py).into_any()),
                    }),
                },
                DIRECTION_DEFAULT,
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let Holders(transforms) = crate::arg!(a, 1, convert, Holders);
                let dir = enum_param!(o, a, 2);
                let rc = TransformRc::new(Transform::Group(GroupTransform::new()));
                rc.lock().children.extend(transforms);
                set_direction(&rc, dir);
                validate(py, &rc)?;
                construct(&a[0], o.params[0].ty, rc)?;
                none(py)
            },
            pos_only: 0,
        },
    ],
};

// .def_static("GetWriteFormats", []() { return WriteFormatIterator(nullptr); })
// (PyGroupTransform.cpp:57-60 @ v2.5.2).
static GET_WRITE_FORMATS: Function = static_def!(
    "GetWriteFormats",
    [],
    WRITE_FORMAT_ITERATOR,
    |py, _, _, _| {
        Ok(Some(
            new_iterator(py, WRITE_FORMAT_ITERATOR, None)?.unbind(),
        ))
    }
);

// .def("__iter__", [](GroupTransformRcPtr & self) { return TransformIterator(self); })
// (PyGroupTransform.cpp:62-65 @ v2.5.2).
static ITER: Function = method_def!("__iter__", [SELF], TRANSFORM_ITERATOR, |py, o, a, _| {
    let rc = this_rc!(o, a);
    Ok(Some(
        new_iterator(py, TRANSFORM_ITERATOR, Some(Box::new(rc)))?.unbind(),
    ))
});

// .def("__len__", &GroupTransform::getNumTransforms) (PyGroupTransform.cpp:66-67 @ v2.5.2).
static LEN: Function = method_def!("__len__", [SELF], "int", |py, o, a, _| {
    let rc = this_rc!(o, a);
    ok(py, num_transforms(&rc))
});

// .def("__getitem__", (TransformRcPtr & (GroupTransform::*)(int)) &GroupTransform::getTransform,
// "index"_a) (PyGroupTransform.cpp:68-71 @ v2.5.2).
static GETITEM: Function = method_def!(
    "__getitem__",
    [SELF, Param::new("index", INT)],
    TRANSFORM,
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let index = crate::arg!(a, 1, convert, i32);
        transform_out(py, get_transform(py, &rc, index)?.as_ref())
    }
);

// .def("getFormatMetadata", ..., py::return_value_policy::reference_internal)
// (PyGroupTransform.cpp:73-76 @ v2.5.2).
static GET_FORMAT_METADATA: Function = crate::format_metadata_def!(SELF);

// .def("appendTransform", &GroupTransform::appendTransform, "transform"_a)
// (PyGroupTransform.cpp:77-78 @ v2.5.2).
static APPEND: Function = method_def!(
    "appendTransform",
    [SELF, Param::new("transform", TRANSFORM)],
    "None",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let t = holder_param!(o, a, 1, convert);
        rc.lock().children.push(t);
        none(py)
    }
);

// .def("prependTransform", &GroupTransform::prependTransform, "transform"_a)
// (PyGroupTransform.cpp:79-80 @ v2.5.2).
static PREPEND: Function = method_def!(
    "prependTransform",
    [SELF, Param::new("transform", TRANSFORM)],
    "None",
    |py, o, a, convert| {
        let rc = this_rc!(o, a);
        let t = holder_param!(o, a, 1, convert);
        rc.lock().children.insert(0, t);
        none(py)
    }
);

/// A `ConstConfigRcPtr` argument whose default is `nullptr`: `None` with conversions (the
/// default's own conversion too: the dispatcher's first pass refuses it), or an instance of the
/// class `Config`, whose value its card (6.3f) defines; until then no instance is constructed,
/// and reading one is U-86's error.
fn config_arg(
    obj: &Bound<'_, PyAny>,
    convert: bool,
) -> PyResult<Option<Option<Arc<ocio::Config>>>> {
    if obj.is_none() {
        return Ok(convert.then_some(None));
    }
    let Some(this) = crate::pybind11::instance_of(obj, CONFIG) else {
        return Ok(None);
    };
    let value = crate::pybind11::read_value(&this, CONFIG)?;
    Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
        "a Config value ({}) before its class is bound",
        value.get_type().name()?
    )))
}

/// The config `write` uses: the one given, else the current config (`GetCurrentConfig`).
/// Upstream's "A config is required." follows a null current config, which `GetCurrentConfig`
/// never gives (it throws instead).
fn config_or_current(
    py: Python<'_>,
    config: Option<Arc<ocio::Config>>,
) -> PyResult<Arc<ocio::Config>> {
    match config {
        Some(config) => Ok(config),
        None => py_result(py, ocio::config::get_current_config()),
    }
}

/// `write(formatName, config)` into `os`: the library's `write` of the group's copy. A null
/// child is no op and no CDL for the writers, as the empty group that stands for it
/// ([`Holes::Placeholder`]). The CLF and CTF writer recurses through the children
/// (`BuildGroupOps`) and through a group that holds itself without end (U-111); the CDL
/// writers look at the group's own children only, so a group holding itself is to them a
/// child that isn't a CDL, as the empty group that stands for it.
fn write_to(
    py: Python<'_>,
    rc: &TransformRc,
    config: &ocio::Config,
    format_name: &[u8],
    os: &mut impl std::io::Write,
) -> PyResult<()> {
    let name = ocio_ops::utils::string_utils::c_str(format_name);
    let ctf = [FILEFORMAT_CLF, FILEFORMAT_CTF]
        .iter()
        .any(|f| name.eq_ignore_ascii_case(f.as_bytes()));
    let cycles = if ctf {
        Holes::Error
    } else {
        Holes::Placeholder
    };
    let Transform::Group(group) = rc.materialize_with(Holes::Placeholder, cycles)? else {
        return Err(pyo3::exceptions::PyRuntimeError::new_err("not a group"));
    };
    py_result(py, group.write(config, format_name, os))
}

/// `std::ofstream f(fileName.c_str())`: the file, created or truncated, in text mode (on
/// Windows, each line end written as CR LF), or nothing when it can't be opened (writes to a
/// stream that failed to open are lost, and `write` doesn't look). On Windows the C runtime
/// reads a narrow name in the ANSI code page: a name that isn't ASCII is the port's error
/// (U-113).
fn open_text_file(file_name: &[u8]) -> PyResult<Option<std::fs::File>> {
    let name = ocio_ops::utils::string_utils::c_str(file_name);
    #[cfg(windows)]
    let path = {
        if !name.is_ascii() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "the wheel opens a file name that isn't ASCII through the C runtime's ANSI code \
                 page (docs/improvements.md, U-113)",
            ));
        }
        std::path::PathBuf::from(String::from_utf8_lossy(name).into_owned())
    };
    #[cfg(not(windows))]
    let path = {
        use std::os::unix::ffi::OsStrExt;
        std::path::PathBuf::from(std::ffi::OsStr::from_bytes(name))
    };
    Ok(std::fs::File::create(path).ok())
}

// .def("write", [](self, formatName, fileName, config) {...}, "formatName"_a.none(false),
// "fileName"_a.none(false), "config"_a = nullptr) and .def("write", [](self, formatName,
// config) {...}, "formatName"_a.none(false), "config"_a = nullptr) (PyGroupTransform.cpp:81-120
// @ v2.5.2).
static WRITE: Function = Function {
    name: "write",
    constructor: false,
    method: true,
    overloads: &[
        Overload {
            params: &[
                SELF,
                Param::new("formatName", "str"),
                Param::new("fileName", "str"),
                Param {
                    name: "config",
                    ty: CONFIG,
                    default: Some(Default {
                        repr: "None",
                        value: |py| Ok(py.None().into_bound(py)),
                    }),
                },
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let rc = this_rc!(o, a);
                let StdString(format_name) = crate::arg!(a, 1, convert, StdString);
                let StdString(file_name) = crate::arg!(a, 2, convert, StdString);
                let Some(config) = config_arg(&a[3], convert)? else {
                    return Ok(None);
                };
                let config = config_or_current(py, config)?;
                let file = open_text_file(&file_name)?;
                let mut text = Vec::new();
                let written = write_to(py, &rc, &config, &format_name, &mut text);
                if let Some(mut file) = file {
                    let text = if cfg!(windows) { crlf(&text) } else { text };
                    let _ = std::io::Write::write_all(&mut file, &text);
                }
                written?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param::new("formatName", "str"),
                Param {
                    name: "config",
                    ty: CONFIG,
                    default: Some(Default {
                        repr: "None",
                        value: |py| Ok(py.None().into_bound(py)),
                    }),
                },
            ],
            ret: "str",
            call: |py, o, a, convert| {
                let rc = this_rc!(o, a);
                let StdString(format_name) = crate::arg!(a, 1, convert, StdString);
                let Some(config) = config_arg(&a[2], convert)? else {
                    return Ok(None);
                };
                let config = config_or_current(py, config)?;
                let mut text = Vec::new();
                write_to(py, &rc, &config, &format_name, &mut text)?;
                string_out(py, &text)
            },
            pos_only: 0,
        },
    ],
};

/// The bytes a text-mode stream of MSVC's C runtime writes for `text`: each LF as CR LF.
fn crlf(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    for &b in text {
        if b == b'\n' {
            out.push(b'\r');
        }
        out.push(b);
    }
    out
}

static REPR: Function = crate::repr_def!(SELF);

const IT_SELF: Param = Param::new("self", TRANSFORM_ITERATOR);
const WF_SELF: Param = Param::new("self", WRITE_FORMAT_ITERATOR);

// clsTransformIterator (PyGroupTransform.cpp:124-142 @ v2.5.2).
static IT_LEN: Function = method_def!("__len__", [IT_SELF], "int", |py, o, a, _| {
    let it = this_iter!(o, a);
    let rc = it.get().obj::<TransformRc>()?;
    ok(py, num_transforms(rc))
});

static IT_GETITEM: Function = method_def!(
    "__getitem__",
    [IT_SELF, Param::new("arg0", INT)],
    TRANSFORM,
    |py, o, a, convert| {
        let it = this_iter!(o, a);
        let i = crate::arg!(a, 1, convert, i32);
        let rc = it.get().obj::<TransformRc>()?;
        transform_out(py, get_transform(py, rc, i)?.as_ref())
    }
);

static IT_ITER: Function = method_def!("__iter__", [IT_SELF], TRANSFORM_ITERATOR, |_, o, a, _| {
    let _it = this_iter!(o, a);
    Ok(Some(a[0].clone().unbind()))
});

static IT_NEXT: Function = method_def!("__next__", [IT_SELF], TRANSFORM, |py, o, a, _| {
    let it = this_iter!(o, a);
    let rc = it.get().obj::<TransformRc>()?;
    let i = it.get().next_index(num_transforms(rc))?;
    transform_out(py, get_transform(py, rc, i)?.as_ref())
});

/// `(GetFormatNameByIndex(i), GetFormatExtensionByIndex(i))`.
fn write_format(py: Python<'_>, i: i32) -> PyResult<Option<Py<PyAny>>> {
    let name = GroupTransform::format_name_by_index(i);
    let ext = GroupTransform::format_extension_by_index(i);
    let tuple = PyTuple::new(
        py,
        [
            crate::dispatch::std_string_to_py(py, name)?,
            crate::dispatch::std_string_to_py(py, ext)?,
        ],
    )?;
    Ok(Some(tuple.into_any().unbind()))
}

// clsWriteFormatIterator (PyGroupTransform.cpp:144-166 @ v2.5.2).
static WF_LEN: Function = method_def!("__len__", [WF_SELF], "int", |py, o, a, _| {
    let _it = this_iter!(o, a);
    ok(py, GroupTransform::num_write_formats())
});

static WF_GETITEM: Function = method_def!(
    "__getitem__",
    [WF_SELF, Param::new("arg0", INT)],
    "tuple",
    |py, o, a, convert| {
        let _it = this_iter!(o, a);
        let i = crate::arg!(a, 1, convert, i32);
        check_index(i, GroupTransform::num_write_formats())?;
        write_format(py, i)
    }
);

static WF_ITER: Function = method_def!(
    "__iter__",
    [WF_SELF],
    WRITE_FORMAT_ITERATOR,
    |_, o, a, _| {
        let _it = this_iter!(o, a);
        Ok(Some(a[0].clone().unbind()))
    }
);

static WF_NEXT: Function = method_def!("__next__", [WF_SELF], "tuple", |py, o, a, _| {
    let it = this_iter!(o, a);
    let i = it.get().next_index(GroupTransform::num_write_formats())?;
    write_format(py, i)
});

/// Port of `bindPyGroupTransform` (src/bindings/python/transforms/PyGroupTransform.cpp:25-167
/// @ v2.5.2), in its order: the iterator classes nested in the class (31-37), the class's
/// methods (39-122), the iterators' (124-166).
pub(crate) fn bind(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let cls = class(py, GROUP_TRANSFORM)?;
    let transform_iterator = nested_class(&cls, "GroupTransform", "TransformIterator")?;
    let write_format_iterator = nested_class(&cls, "GroupTransform", "WriteFormatIterator")?;
    def(&cls, &INIT)?;
    def_static(&cls, &GET_WRITE_FORMATS)?;
    for f in [
        &ITER,
        &LEN,
        &GETITEM,
        &GET_FORMAT_METADATA,
        &APPEND,
        &PREPEND,
        &WRITE,
        &REPR,
    ] {
        def(&cls, f)?;
    }
    for f in [&IT_LEN, &IT_GETITEM, &IT_ITER, &IT_NEXT] {
        def(&transform_iterator, f)?;
    }
    for f in [&WF_LEN, &WF_GETITEM, &WF_ITER, &WF_NEXT] {
        def(&write_format_iterator, f)?;
    }
    Ok(())
}
