// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyViewTransform` (src/bindings/python/PyViewTransform.cpp @ v2.5.2): the class
//! `ViewTransform` and its iterator class (chunk 6.3e).
//!
//! As a color space's ([`crate::py_color_space`]), a view transform's transforms are the very
//! objects `getTransform` returns, copies of those given: the binding holds a view transform as
//! a [`ViewTransformNode`], the library's without its transforms and their shared nodes.

use ocio::ViewTransform;
use ocio_ops::open_color_types::{ReferenceSpaceType, ViewTransformDirection};
use ocio_ops::utils::string_utils::c_str;
use pyo3::prelude::*;

use crate::dispatch::{CharPtr, Default, Function, Overload, Param, StdString};
use crate::py_color_space::{interchange_dict, reference_space_type};
use crate::py_open_color_io::py_result;
use crate::py_transform::{
    INT, Out, TRANSFORM, TransformRc, class, create_editable_copy, def, enum_out, holder_arg,
    nested_class, none, ok, string_out, transform_out,
};
use crate::py_utils::{check_index, new_iterator};
use crate::shared::{Shared, construct, out};
use crate::{enum_param, method_def, q, this_iter, this_shared};

pub(crate) const VIEW_TRANSFORM: &str = q!("ViewTransform");
const CATEGORY_ITERATOR: &str = q!("ViewTransform.ViewTransformCategoryIterator");
const REFERENCE_SPACE_TYPE: &str = q!("ReferenceSpaceType");
const VIEW_TRANSFORM_DIRECTION: &str = q!("ViewTransformDirection");
const SELF: Param = Param::new("self", VIEW_TRANSFORM);

/// A view transform as the binding holds it.
#[derive(Debug)]
pub(crate) struct ViewTransformNode {
    /// The view transform, without its transforms.
    pub(crate) vt: ViewTransform,
    /// `m_toRefTransform`.
    pub(crate) to_ref: Option<TransformRc>,
    /// `m_fromRefTransform`.
    pub(crate) from_ref: Option<TransformRc>,
}

/// A view transform shared by the Python objects that hold it.
pub(crate) type ViewTransformRc = Shared<ViewTransformNode>;

impl ViewTransformNode {
    /// The node of a view transform the library holds (a config's): its transforms become
    /// nodes of their own, which Python then shares with the config.
    pub(crate) fn from_library(vt: &ViewTransform) -> ViewTransformNode {
        let node = |dir| vt.transform(dir).map(|t| TransformRc::new(t.clone()));
        let to_ref = node(ViewTransformDirection::ToReference);
        let from_ref = node(ViewTransformDirection::FromReference);
        let mut bare = vt.clone();
        bare.set_transform_as_is(None, ViewTransformDirection::ToReference);
        bare.set_transform_as_is(None, ViewTransformDirection::FromReference);
        ViewTransformNode {
            vt: bare,
            to_ref,
            from_ref,
        }
    }

    fn new(reference: ReferenceSpaceType) -> ViewTransformNode {
        ViewTransformNode {
            vt: ViewTransform::new(reference),
            to_ref: None,
            from_ref: None,
        }
    }

    /// Port of `ViewTransform::createEditableCopy` and `ViewTransform::Impl::operator=`
    /// (src/OpenColorIO/ViewTransform.cpp:42-63, 77-82 @ v2.5.2): the transforms copied, the
    /// one to the reference first.
    pub(crate) fn copy(&self, py: Python<'_>) -> PyResult<ViewTransformNode> {
        let to_ref = match &self.to_ref {
            Some(t) => Some(create_editable_copy(py, t)?),
            None => None,
        };
        let from_ref = match &self.from_ref {
            Some(t) => Some(create_editable_copy(py, t)?),
            None => None,
        };
        Ok(ViewTransformNode {
            vt: self.vt.clone(),
            to_ref,
            from_ref,
        })
    }

    /// The library's view transform with its transforms as they are now, stored as they are
    /// (`set_transform_as_is`): upstream prints the transforms it holds without copying them, so
    /// one made invalid through Python since it was set prints as it is.
    pub(crate) fn materialize(&self) -> PyResult<ViewTransform> {
        let mut vt = self.vt.clone();
        for (t, dir) in [
            (&self.to_ref, ViewTransformDirection::ToReference),
            (&self.from_ref, ViewTransformDirection::FromReference),
        ] {
            if let Some(t) = t {
                vt.set_transform_as_is(Some(t.materialize()?), dir);
            }
        }
        Ok(vt)
    }

    /// Port of `ViewTransform::getTransform` (ViewTransform.cpp:216-226 @ v2.5.2): none for a
    /// direction outside the enumerators.
    fn transform(&self, dir: i64) -> Option<&TransformRc> {
        match dir {
            0 => self.to_ref.as_ref(),
            1 => self.from_ref.as_ref(),
            _ => None,
        }
    }

    /// Port of `ViewTransform::setTransform` (ViewTransform.cpp:228-245 @ v2.5.2): a copy,
    /// stored for a direction of the enumerators only.
    fn set_transform(
        &mut self,
        py: Python<'_>,
        transform: Option<&TransformRc>,
        dir: i64,
    ) -> PyResult<()> {
        let copy = match transform {
            Some(t) => Some(create_editable_copy(py, t)?),
            None => None,
        };
        match dir {
            0 => self.to_ref = copy,
            1 => self.from_ref = copy,
            _ => {}
        }
        Ok(())
    }
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

const fn text_param(name: &'static str) -> Param {
    Param {
        name,
        ty: "str",
        default: Some(Default {
            repr: "''",
            value: empty_str,
        }),
    }
}

// .def(py::init([](ReferenceSpaceType referenceSpace) {...}), "referenceSpace"_a),
// .def(py::init([](referenceSpace, name, family, description, toReference, fromReference,
// categories) {...}), "referenceSpace"_a = DEFAULT->getReferenceSpaceType(), "name"_a =
// DEFAULT->getName(), "family"_a = DEFAULT->getFamily(), "description"_a =
// DEFAULT->getDescription(), "toReference"_a = ..., "fromReference"_a = ..., "categories"_a =
// getCategoriesStdVec(DEFAULT)) (PyViewTransform.cpp:51-94 @ v2.5.2).
static INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[SELF, Param::new("referenceSpace", REFERENCE_SPACE_TYPE)],
            ret: "None",
            call: |py, o, a, _| {
                let reference = reference_space_type(enum_param!(o, a, 1))?;
                construct(
                    &a[0],
                    VIEW_TRANSFORM,
                    Shared::new(ViewTransformNode::new(reference)),
                )?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param {
                    name: "referenceSpace",
                    ty: REFERENCE_SPACE_TYPE,
                    default: Some(Default {
                        repr: "<ReferenceSpaceType.REFERENCE_SPACE_SCENE: 0>",
                        value: |py| {
                            crate::enums::to_py(
                                &crate::enums::enum_class(py, REFERENCE_SPACE_TYPE)?,
                                0,
                            )
                        },
                    }),
                },
                text_param("name"),
                text_param("family"),
                text_param("description"),
                Param {
                    name: "toReference",
                    ty: TRANSFORM,
                    default: Some(Default {
                        repr: "None",
                        value: none_value,
                    }),
                },
                Param {
                    name: "fromReference",
                    ty: TRANSFORM,
                    default: Some(Default {
                        repr: "None",
                        value: none_value,
                    }),
                },
                Param {
                    name: "categories",
                    ty: "collections.abc.Sequence[str]",
                    default: Some(Default {
                        repr: "[]",
                        value: |py| Ok(pyo3::types::PyList::empty(py).into_any()),
                    }),
                },
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let reference = enum_param!(o, a, 1);
                let StdString(name) = crate::arg!(a, 2, convert, StdString);
                let StdString(family) = crate::arg!(a, 3, convert, StdString);
                let StdString(description) = crate::arg!(a, 4, convert, StdString);
                let to_ref = transform_param!(o, a, 5, convert);
                let from_ref = transform_param!(o, a, 6, convert);
                let categories = crate::arg!(a, 7, convert, Vec<StdString>);
                let mut p = ViewTransformNode::new(reference_space_type(reference)?);
                if !name.is_empty() {
                    p.vt.set_name(c_str(&name));
                }
                if !family.is_empty() {
                    p.vt.set_family(c_str(&family));
                }
                if !description.is_empty() {
                    p.vt.set_description(c_str(&description));
                }
                if let Some(t) = &to_ref {
                    p.set_transform(py, Some(t), 0)?;
                }
                if let Some(t) = &from_ref {
                    p.set_transform(py, Some(t), 1)?;
                }
                if !categories.is_empty() {
                    p.vt.clear_categories();
                    for StdString(category) in &categories {
                        p.vt.add_category(c_str(category));
                    }
                }
                construct(&a[0], VIEW_TRANSFORM, Shared::new(p))?;
                none(py)
            },
            pos_only: 0,
        },
    ],
};

// .def("__deepcopy__", [](const ConstViewTransformRcPtr & self, py::dict) { return
// self->createEditableCopy(); }, "memo"_a) (PyViewTransform.cpp:96-100 @ v2.5.2).
static DEEPCOPY: Function = method_def!(
    "__deepcopy__",
    [SELF, Param::new("memo", "dict")],
    VIEW_TRANSFORM,
    |py, o, a, _| {
        let rc = this_shared!(o, a, ViewTransformNode);
        if !a[1].is_instance_of::<pyo3::types::PyDict>() {
            return Ok(None);
        }
        let copy = rc.lock().copy(py)?;
        out(py, VIEW_TRANSFORM, &Shared::new(copy))
    }
);

/// A getter of one `const char *` of the view transform.
macro_rules! text_getter {
    ($name:literal, $get:ident) => {
        method_def!($name, [SELF], "str", |py, o, a, _| {
            let rc = this_shared!(o, a, ViewTransformNode);
            let text = c_str(rc.lock().vt.$get()).to_vec();
            string_out(py, &text)
        })
    };
}

/// A method `(<param>: str)` of the view transform, a null pointer as `""`.
macro_rules! text_method {
    ($name:literal, $param:literal, $ret:expr, $body:expr) => {
        method_def!(
            $name,
            [SELF, Param::new($param, "str")],
            $ret,
            |py, o, a, convert| {
                let rc = this_shared!(o, a, ViewTransformNode);
                let s = crate::arg!(a, 1, convert, CharPtr).0.unwrap_or_default();
                $body(py, &mut rc.lock().vt, &s)
            }
        )
    };
}

// .def("getName", ...) to .def("setDescription", ...) (PyViewTransform.cpp:102-113 @ v2.5.2).
static GET_NAME: Function = text_getter!("getName", name);
static SET_NAME: Function = text_method!(
    "setName",
    "name",
    "None",
    |py, vt: &mut ViewTransform, s: &[u8]| {
        vt.set_name(s);
        none(py)
    }
);
static GET_FAMILY: Function = text_getter!("getFamily", family);
static SET_FAMILY: Function = text_method!(
    "setFamily",
    "family",
    "None",
    |py, vt: &mut ViewTransform, s: &[u8]| {
        vt.set_family(s);
        none(py)
    }
);
static GET_DESCRIPTION: Function = text_getter!("getDescription", description);
static SET_DESCRIPTION: Function = text_method!(
    "setDescription",
    "description",
    "None",
    |py, vt: &mut ViewTransform, s: &[u8]| {
        vt.set_description(s);
        none(py)
    }
);

// .def("getInterchangeAttribute", ...), .def("setInterchangeAttribute", ...),
// .def("getInterchangeAttributes", ...) (PyViewTransform.cpp:114-119 @ v2.5.2).
static GET_INTERCHANGE_ATTRIBUTE: Function = text_method!(
    "getInterchangeAttribute",
    "attrName",
    "str",
    |py, vt: &mut ViewTransform, s: &[u8]| {
        let value = py_result(py, vt.interchange_attribute(s).map(<[u8]>::to_vec))?;
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
        let rc = this_shared!(o, a, ViewTransformNode);
        let name = crate::arg!(a, 1, convert, CharPtr).0.unwrap_or_default();
        let value = crate::arg!(a, 2, convert, CharPtr).0.unwrap_or_default();
        py_result(py, rc.lock().vt.set_interchange_attribute(&name, &value))?;
        none(py)
    }
);
static GET_INTERCHANGE_ATTRIBUTES: Function = method_def!(
    "getInterchangeAttributes",
    [SELF],
    "dict[str, str]",
    |py, o, a, _| {
        let rc = this_shared!(o, a, ViewTransformNode);
        let attributes = rc.lock().vt.interchange_attributes().clone();
        interchange_dict(py, &attributes)
    }
);

// Categories (PyViewTransform.cpp:120-130 @ v2.5.2).
static HAS_CATEGORY: Function = text_method!(
    "hasCategory",
    "category",
    "bool",
    |py, vt: &mut ViewTransform, s: &[u8]| ok(py, vt.has_category(s))
);
static ADD_CATEGORY: Function = text_method!(
    "addCategory",
    "category",
    "None",
    |py, vt: &mut ViewTransform, s: &[u8]| {
        vt.add_category(s);
        none(py)
    }
);
static REMOVE_CATEGORY: Function = text_method!(
    "removeCategory",
    "category",
    "None",
    |py, vt: &mut ViewTransform, s: &[u8]| {
        vt.remove_category(s);
        none(py)
    }
);
static GET_CATEGORIES: Function =
    method_def!("getCategories", [SELF], CATEGORY_ITERATOR, |py, o, a, _| {
        let rc = this_shared!(o, a, ViewTransformNode);
        Ok(Some(
            new_iterator(py, CATEGORY_ITERATOR, Some(Box::new(rc)))?.unbind(),
        ))
    });
static CLEAR_CATEGORIES: Function =
    method_def!("clearCategories", [SELF], "None", |py, o, a, _| {
        let rc = this_shared!(o, a, ViewTransformNode);
        rc.lock().vt.clear_categories();
        none(py)
    });

// .def("getReferenceSpaceType", ...), .def("getTransform", ..., "direction"_a),
// .def("setTransform", ..., "transform"_a, "direction"_a) (PyViewTransform.cpp:131-136 @
// v2.5.2).
static GET_REFERENCE_SPACE_TYPE: Function = method_def!(
    "getReferenceSpaceType",
    [SELF],
    REFERENCE_SPACE_TYPE,
    |py, o, a, _| {
        let rc = this_shared!(o, a, ViewTransformNode);
        let r = rc.lock().vt.reference_space_type();
        enum_out(py, REFERENCE_SPACE_TYPE, r as i64)
    }
);
static GET_TRANSFORM: Function = method_def!(
    "getTransform",
    [SELF, Param::new("direction", VIEW_TRANSFORM_DIRECTION)],
    TRANSFORM,
    |py, o, a, _| {
        let rc = this_shared!(o, a, ViewTransformNode);
        let dir = enum_param!(o, a, 1);
        let t = rc.lock().transform(dir).cloned();
        transform_out(py, t.as_ref())
    }
);
static SET_TRANSFORM: Function = method_def!(
    "setTransform",
    [
        SELF,
        Param::new("transform", TRANSFORM),
        Param::new("direction", VIEW_TRANSFORM_DIRECTION)
    ],
    "None",
    |py, o, a, convert| {
        let rc = this_shared!(o, a, ViewTransformNode);
        let t = transform_param!(o, a, 1, convert);
        let dir = enum_param!(o, a, 2);
        rc.lock().set_transform(py, t.as_ref(), dir)?;
        none(py)
    }
);

// defRepr(clsViewTransform) (PyViewTransform.cpp:138 @ v2.5.2).
static REPR: Function = method_def!("__repr__", [SELF], "str", |py, o, a, _| {
    let rc = this_shared!(o, a, ViewTransformNode);
    let text = rc.lock().materialize()?.to_bytes();
    string_out(py, &text)
});

// clsViewTransformCategoryIterator (PyViewTransform.cpp:140-152 @ v2.5.2).
const CAT_SELF: Param = Param::new("self", CATEGORY_ITERATOR);

/// The view transform an iterator of `self` walks, or the next overload.
macro_rules! iter_rc {
    ($o:expr, $a:expr) => {{
        let it = this_iter!($o, $a);
        let rc = it.get().obj::<ViewTransformRc>()?.clone();
        (it, rc)
    }};
}

fn num_categories(rc: &ViewTransformRc) -> i32 {
    rc.lock().vt.num_categories()
}

/// `getCategory(i)`: `None` outside the categories.
fn category(py: Python<'_>, rc: &ViewTransformRc, i: i32) -> Out {
    let c = rc.lock().vt.category(i).map(<[u8]>::to_vec);
    Ok(Some(
        crate::dispatch::char_ptr_to_py(py, c.as_deref())?.unbind(),
    ))
}

static CAT_LEN: Function = method_def!("__len__", [CAT_SELF], "int", |py, o, a, _| {
    let (_, rc) = iter_rc!(o, a);
    ok(py, num_categories(&rc))
});

static CAT_GETITEM: Function = method_def!(
    "__getitem__",
    [CAT_SELF, Param::new("arg0", INT)],
    "str",
    |py, o, a, convert| {
        let (_, rc) = iter_rc!(o, a);
        let i = crate::arg!(a, 1, convert, i32);
        check_index(i, num_categories(&rc))?;
        category(py, &rc, i)
    }
);

static CAT_ITER: Function = method_def!("__iter__", [CAT_SELF], CATEGORY_ITERATOR, |_, o, a, _| {
    let _ = this_iter!(o, a);
    Ok(Some(a[0].clone().unbind()))
});

static CAT_NEXT: Function = method_def!("__next__", [CAT_SELF], "str", |py, o, a, _| {
    let (it, rc) = iter_rc!(o, a);
    let i = it.get().next_index(num_categories(&rc))?;
    category(py, &rc, i)
});

/// Port of `bindPyViewTransform` (src/bindings/python/PyViewTransform.cpp:35-153 @ v2.5.2), in
/// its order.
pub(crate) fn bind_py_view_transform(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let cls = class(py, VIEW_TRANSFORM)?;
    let categories = nested_class(&cls, "ViewTransform", "ViewTransformCategoryIterator")?;
    for f in [
        &INIT,
        &DEEPCOPY,
        &GET_NAME,
        &SET_NAME,
        &GET_FAMILY,
        &SET_FAMILY,
        &GET_DESCRIPTION,
        &SET_DESCRIPTION,
        &GET_INTERCHANGE_ATTRIBUTE,
        &SET_INTERCHANGE_ATTRIBUTE,
        &GET_INTERCHANGE_ATTRIBUTES,
        &HAS_CATEGORY,
        &ADD_CATEGORY,
        &REMOVE_CATEGORY,
        &GET_CATEGORIES,
        &CLEAR_CATEGORIES,
        &GET_REFERENCE_SPACE_TYPE,
        &GET_TRANSFORM,
        &SET_TRANSFORM,
        &REPR,
    ] {
        def(&cls, f)?;
    }
    for f in [&CAT_LEN, &CAT_GETITEM, &CAT_ITER, &CAT_NEXT] {
        def(&categories, f)?;
    }
    Ok(())
}
