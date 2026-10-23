// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyLook` (src/bindings/python/PyLook.cpp @ v2.5.2): the class `Look` (chunk 6.3e).
//!
//! As a color space's ([`crate::py_color_space`]), a look's transforms are the very objects
//! `getTransform` and `getInverseTransform` return, copies of those given: the binding holds a
//! look as a [`LookNode`], the library's look without its transforms and their shared nodes.

use ocio::Look;
use ocio_ops::utils::string_utils::c_str;
use pyo3::prelude::*;

use crate::dispatch::{CharPtr, Default, Function, NonNullCharPtr, Overload, Param, StdString};
use crate::py_color_space::interchange_dict;
use crate::py_open_color_io::py_result;
use crate::py_transform::{
    TRANSFORM, TransformRc, class, create_editable_copy, def, holder_arg, none, string_out,
    transform_out,
};
use crate::shared::{Shared, construct, out};
use crate::{method_def, q, this_shared};

pub(crate) const LOOK: &str = q!("Look");
const SELF: Param = Param::new("self", LOOK);

/// A look as the binding holds it.
#[derive(Debug)]
pub(crate) struct LookNode {
    /// The look, without its transforms.
    pub(crate) look: Look,
    /// `m_transform`.
    pub(crate) transform: Option<TransformRc>,
    /// `m_inverseTransform`.
    pub(crate) inverse: Option<TransformRc>,
}

impl LookNode {
    fn new() -> LookNode {
        LookNode {
            look: Look::new(),
            transform: None,
            inverse: None,
        }
    }

    /// Port of `Look::createEditableCopy` and `Look::Impl::operator=` (src/OpenColorIO/
    /// Look.cpp:50-68, 84-89 @ v2.5.2): the transforms copied with `createEditableCopy`, the
    /// look's first.
    pub(crate) fn copy(&self, py: Python<'_>) -> PyResult<LookNode> {
        let transform = match &self.transform {
            Some(t) => Some(create_editable_copy(py, t)?),
            None => None,
        };
        let inverse = match &self.inverse {
            Some(t) => Some(create_editable_copy(py, t)?),
            None => None,
        };
        Ok(LookNode {
            look: self.look.clone(),
            transform,
            inverse,
        })
    }

    /// The library's look with its transforms as they are now, stored as they are
    /// (`set_transform_as_is`): upstream prints the transforms it holds without copying them, so
    /// one made invalid through Python since it was set prints as it is.
    pub(crate) fn materialize(&self) -> PyResult<Look> {
        let mut look = self.look.clone();
        if let Some(t) = &self.transform {
            look.set_transform_as_is(Some(t.materialize()?));
        }
        if let Some(t) = &self.inverse {
            look.set_inverse_transform_as_is(Some(t.materialize()?));
        }
        Ok(look)
    }
}

/// The port's error where upstream copies a null transform (`transform->createEditableCopy()`,
/// Look.cpp:111-125 @ v2.5.2; U-124).
fn null_transform(what: &str) -> PyErr {
    pyo3::exceptions::PyValueError::new_err(format!(
        "{what}: None, a null pointer the wheel dereferences (its process dies; \
         docs/improvements.md, U-124)"
    ))
}

/// A `TransformRcPtr` argument `a[i]` (`None`, a null pointer, with conversions), or the next
/// overload.
macro_rules! transform_param {
    ($o:expr, $a:expr, $i:expr, $convert:expr) => {
        match holder_arg(&$a[$i], $o.params[$i].ty, $convert)? {
            Some(rc) => rc,
            None => return Ok(None),
        }
    };
}

fn empty_str(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    Ok(pyo3::types::PyString::new(py, "").into_any())
}

fn none_value(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    Ok(py.None().into_bound(py))
}

// .def(py::init(&Look::Create)), .def(py::init([](name, processSpace, transform,
// inverseTransform, description) {...}), "name"_a = DEFAULT->getName(), "processSpace"_a =
// DEFAULT->getProcessSpace(), "transform"_a = DEFAULT->getTransform(), "inverseTransform"_a =
// DEFAULT->getInverseTransform(), "description"_a = DEFAULT->getDescription())
// (PyLook.cpp:18-38 @ v2.5.2).
static INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[SELF],
            ret: "None",
            call: |py, _, a, _| {
                construct(&a[0], LOOK, Shared::new(LookNode::new()))?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param {
                    name: "name",
                    ty: "str",
                    default: Some(Default {
                        repr: "''",
                        value: empty_str,
                    }),
                },
                Param {
                    name: "processSpace",
                    ty: "str",
                    default: Some(Default {
                        repr: "''",
                        value: empty_str,
                    }),
                },
                Param {
                    name: "transform",
                    ty: TRANSFORM,
                    default: Some(Default {
                        repr: "None",
                        value: none_value,
                    }),
                },
                Param {
                    name: "inverseTransform",
                    ty: TRANSFORM,
                    default: Some(Default {
                        repr: "None",
                        value: none_value,
                    }),
                },
                Param {
                    name: "description",
                    ty: "str",
                    default: Some(Default {
                        repr: "''",
                        value: empty_str,
                    }),
                },
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let StdString(name) = crate::arg!(a, 1, convert, StdString);
                let StdString(process_space) = crate::arg!(a, 2, convert, StdString);
                let transform = transform_param!(o, a, 3, convert);
                let inverse = transform_param!(o, a, 4, convert);
                let StdString(description) = crate::arg!(a, 5, convert, StdString);
                let mut p = LookNode::new();
                if !name.is_empty() {
                    p.look.set_name(c_str(&name));
                }
                if !process_space.is_empty() {
                    p.look.set_process_space(c_str(&process_space));
                }
                if let Some(t) = &transform {
                    p.transform = Some(create_editable_copy(py, t)?);
                }
                if let Some(t) = &inverse {
                    p.inverse = Some(create_editable_copy(py, t)?);
                }
                if !description.is_empty() {
                    p.look.set_description(c_str(&description));
                }
                construct(&a[0], LOOK, Shared::new(p))?;
                none(py)
            },
            pos_only: 0,
        },
    ],
};

// .def("__deepcopy__", [](const ConstLookRcPtr & self, py::dict) { return
// self->createEditableCopy(); }, "memo"_a) (PyLook.cpp:40-44 @ v2.5.2).
static DEEPCOPY: Function = method_def!(
    "__deepcopy__",
    [SELF, Param::new("memo", "dict")],
    LOOK,
    |py, o, a, _| {
        let rc = this_shared!(o, a, LookNode);
        if !a[1].is_instance_of::<pyo3::types::PyDict>() {
            return Ok(None);
        }
        let copy = rc.lock().copy(py)?;
        out(py, LOOK, &Shared::new(copy))
    }
);

/// A getter of one `const char *` of the look.
macro_rules! text_getter {
    ($name:literal, $get:ident) => {
        method_def!($name, [SELF], "str", |py, o, a, _| {
            let rc = this_shared!(o, a, LookNode);
            let text = c_str(rc.lock().look.$get()).to_vec();
            string_out(py, &text)
        })
    };
}

/// A setter of one `const char *` of the look, declared `.none(false)`.
macro_rules! text_setter {
    ($name:literal, $param:literal, $set:ident) => {
        method_def!(
            $name,
            [SELF, Param::new($param, "str")],
            "None",
            |py, o, a, convert| {
                let rc = this_shared!(o, a, LookNode);
                let NonNullCharPtr(s) = crate::arg!(a, 1, convert, NonNullCharPtr);
                rc.lock().look.$set(&s);
                none(py)
            }
        )
    };
}

// .def("getName", ...), .def("setName", ..., "name"_a.none(false)), .def("getProcessSpace",
// ...), .def("setProcessSpace", ..., "processSpace"_a.none(false)) (PyLook.cpp:46-53 @ v2.5.2).
static GET_NAME: Function = text_getter!("getName", name);
static SET_NAME: Function = text_setter!("setName", "name", set_name);
static GET_PROCESS_SPACE: Function = text_getter!("getProcessSpace", process_space);
static SET_PROCESS_SPACE: Function =
    text_setter!("setProcessSpace", "processSpace", set_process_space);

/// A getter of one of the look's transforms (`ConstTransformRcPtr`): the look's own object.
macro_rules! transform_getter {
    ($name:literal, $field:ident) => {
        method_def!($name, [SELF], TRANSFORM, |py, o, a, _| {
            let rc = this_shared!(o, a, LookNode);
            let t = rc.lock().$field.clone();
            transform_out(py, t.as_ref())
        })
    };
}

/// A setter of one of the look's transforms: a copy of it; `None` is U-124.
macro_rules! transform_setter {
    ($name:literal, $field:ident) => {
        method_def!(
            $name,
            [SELF, Param::new("transform", TRANSFORM)],
            "None",
            |py, o, a, convert| {
                let rc = this_shared!(o, a, LookNode);
                let Some(t) = transform_param!(o, a, 1, convert) else {
                    return Err(null_transform(concat!("Look.", $name)));
                };
                let copy = create_editable_copy(py, &t)?;
                rc.lock().$field = Some(copy);
                none(py)
            }
        )
    };
}

// .def("getTransform", ...), .def("setTransform", ..., "transform"_a), .def(
// "getInverseTransform", ...), .def("setInverseTransform", ..., "transform"_a) (PyLook.cpp:54-61
// @ v2.5.2).
static GET_TRANSFORM: Function = transform_getter!("getTransform", transform);
static SET_TRANSFORM: Function = transform_setter!("setTransform", transform);
static GET_INVERSE_TRANSFORM: Function = transform_getter!("getInverseTransform", inverse);
static SET_INVERSE_TRANSFORM: Function = transform_setter!("setInverseTransform", inverse);

// .def("getDescription", ...), .def("setDescription", ..., "description"_a.none(false))
// (PyLook.cpp:62-65 @ v2.5.2).
static GET_DESCRIPTION: Function = text_getter!("getDescription", description);
static SET_DESCRIPTION: Function = text_setter!("setDescription", "description", set_description);

// .def("getInterchangeAttribute", ..., "attrName"_a), .def("setInterchangeAttribute", ...,
// "attrName"_a, "attrValue"_a), .def("getInterchangeAttributes", ...) (PyLook.cpp:66-71 @
// v2.5.2).
static GET_INTERCHANGE_ATTRIBUTE: Function = method_def!(
    "getInterchangeAttribute",
    [SELF, Param::new("attrName", "str")],
    "str",
    |py, o, a, convert| {
        let rc = this_shared!(o, a, LookNode);
        let name = crate::arg!(a, 1, convert, CharPtr).0.unwrap_or_default();
        let value = py_result(
            py,
            rc.lock()
                .look
                .interchange_attribute(&name)
                .map(<[u8]>::to_vec),
        )?;
        string_out(py, c_str(&value))
    }
);
static SET_INTERCHANGE_ATTRIBUTE: Function = method_def!(
    "setInterchangeAttribute",
    [
        SELF,
        Param::new("attrName", "str"),
        Param::new("attrValue", "str")
    ],
    "None",
    |py, o, a, convert| {
        let rc = this_shared!(o, a, LookNode);
        let name = crate::arg!(a, 1, convert, CharPtr).0.unwrap_or_default();
        let value = crate::arg!(a, 2, convert, CharPtr).0.unwrap_or_default();
        py_result(py, rc.lock().look.set_interchange_attribute(&name, &value))?;
        none(py)
    }
);
static GET_INTERCHANGE_ATTRIBUTES: Function = method_def!(
    "getInterchangeAttributes",
    [SELF],
    "dict[str, str]",
    |py, o, a, _| {
        let rc = this_shared!(o, a, LookNode);
        let attributes = rc.lock().look.interchange_attributes().clone();
        interchange_dict(py, &attributes)
    }
);

// defRepr(clsLook) (PyLook.cpp:73 @ v2.5.2).
static REPR: Function = method_def!("__repr__", [SELF], "str", |py, o, a, _| {
    let rc = this_shared!(o, a, LookNode);
    let text = rc.lock().materialize()?.to_bytes();
    string_out(py, &text)
});

/// Port of `bindPyLook` (src/bindings/python/PyLook.cpp:10-74 @ v2.5.2), in its order.
pub(crate) fn bind_py_look(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let cls = class(m.py(), LOOK)?;
    for f in [
        &INIT,
        &DEEPCOPY,
        &GET_NAME,
        &SET_NAME,
        &GET_PROCESS_SPACE,
        &SET_PROCESS_SPACE,
        &GET_TRANSFORM,
        &SET_TRANSFORM,
        &GET_INVERSE_TRANSFORM,
        &SET_INVERSE_TRANSFORM,
        &GET_DESCRIPTION,
        &SET_DESCRIPTION,
        &GET_INTERCHANGE_ATTRIBUTE,
        &SET_INTERCHANGE_ATTRIBUTE,
        &GET_INTERCHANGE_ATTRIBUTES,
        &REPR,
    ] {
        def(&cls, f)?;
    }
    Ok(())
}
