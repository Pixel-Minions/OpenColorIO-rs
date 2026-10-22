// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyNamedTransform` (src/bindings/python/PyNamedTransform.cpp @ v2.5.2): the class
//! `NamedTransform` and its two iterator classes (chunk 6.3e).
//!
//! As a color space's ([`crate::py_color_space`]), a named transform's transforms are the very
//! objects `getTransform` returns, copies of those given: the binding holds a named transform as
//! a [`NamedTransformNode`], the library's without its transforms and their shared nodes.

use ocio::NamedTransform;
use ocio_ops::utils::string_utils::c_str;
use pyo3::prelude::*;

use crate::dispatch::{CharPtr, Default, Function, NonNullCharPtr, Overload, Param, StdString};
use crate::py_open_color_io::to_py_err;
use crate::py_transform::{
    INT, Out, TRANSFORM, TRANSFORM_DIRECTION, TransformRc, class, create_editable_copy, def,
    def_static, direction_of, holder_arg, nested_class, none, ok, set_direction, string_out,
    transform_out,
};
use crate::py_utils::{check_index, new_iterator};
use crate::shared::{Shared, construct, holder_arg as shared_arg, out};
use crate::{enum_param, method_def, q, static_def, this_iter, this_shared};

pub(crate) const NAMED_TRANSFORM: &str = q!("NamedTransform");
const CATEGORY_ITERATOR: &str = q!("NamedTransform.NamedTransformCategoryIterator");
const ALIAS_ITERATOR: &str = q!("NamedTransform.NamedTransformAliasIterator");
const SELF: Param = Param::new("self", NAMED_TRANSFORM);
const STRINGS: &str = "collections.abc.Sequence[str]";

/// A named transform as the binding holds it.
#[derive(Debug)]
pub(crate) struct NamedTransformNode {
    /// The named transform, without its transforms.
    pub(crate) nt: NamedTransform,
    /// `m_forwardTransform`.
    pub(crate) forward: Option<TransformRc>,
    /// `m_inverseTransform`.
    pub(crate) inverse: Option<TransformRc>,
}

/// A named transform shared by the Python objects that hold it.
pub(crate) type NamedTransformRc = Shared<NamedTransformNode>;

/// "Named transform: Unspecified TransformDirection." (NamedTransform.cpp:179, 220, 249 @
/// v2.5.2).
fn unspecified(py: Python<'_>) -> PyErr {
    to_py_err(
        py,
        &ocio::Exception::new("Named transform: Unspecified TransformDirection."),
    )
}

impl NamedTransformNode {
    /// The node of a named transform the library holds (a config's): its transforms become
    /// nodes of their own, which Python then shares with the config.
    pub(crate) fn from_library(nt: &NamedTransform) -> NamedTransformNode {
        use ocio_ops::open_color_types::TransformDirection;
        let node = |dir| nt.transform(dir).map(|t| TransformRc::new(t.clone()));
        let forward = node(TransformDirection::Forward);
        let inverse = node(TransformDirection::Inverse);
        let mut bare = nt.clone();
        bare.set_transform_as_is(None, TransformDirection::Forward);
        bare.set_transform_as_is(None, TransformDirection::Inverse);
        NamedTransformNode {
            nt: bare,
            forward,
            inverse,
        }
    }

    fn new() -> NamedTransformNode {
        NamedTransformNode {
            nt: NamedTransform::new(),
            forward: None,
            inverse: None,
        }
    }

    /// Port of `NamedTransformImpl::createEditableCopy` (src/OpenColorIO/NamedTransform.cpp:
    /// 22-40 @ v2.5.2): the transforms copied, the forward one first.
    pub(crate) fn copy(&self, py: Python<'_>) -> PyResult<NamedTransformNode> {
        let forward = match &self.forward {
            Some(t) => Some(create_editable_copy(py, t)?),
            None => None,
        };
        let inverse = match &self.inverse {
            Some(t) => Some(create_editable_copy(py, t)?),
            None => None,
        };
        Ok(NamedTransformNode {
            nt: self.nt.clone(),
            forward,
            inverse,
        })
    }

    /// The library's named transform with its transforms as they are now, stored as they are
    /// (`set_transform_as_is`): upstream prints the transforms it holds without copying them, so
    /// one made invalid through Python since it was set prints as it is.
    pub(crate) fn materialize(&self) -> PyResult<NamedTransform> {
        use ocio_ops::open_color_types::TransformDirection;
        let mut nt = self.nt.clone();
        for (t, dir) in [
            (&self.forward, TransformDirection::Forward),
            (&self.inverse, TransformDirection::Inverse),
        ] {
            if let Some(t) = t {
                nt.set_transform_as_is(Some(t.materialize()?), dir);
            }
        }
        Ok(nt)
    }

    /// Port of `NamedTransformImpl::getTransform` (NamedTransform.cpp:169-180 @ v2.5.2).
    fn transform(&self, py: Python<'_>, dir: i64) -> PyResult<Option<TransformRc>> {
        match dir {
            0 => Ok(self.forward.clone()),
            1 => Ok(self.inverse.clone()),
            _ => Err(unspecified(py)),
        }
    }

    /// Port of `NamedTransformImpl::setTransform` (NamedTransform.cpp:223-251 @ v2.5.2).
    fn set_transform(
        &mut self,
        py: Python<'_>,
        transform: Option<&TransformRc>,
        dir: i64,
    ) -> PyResult<()> {
        if dir != 0 && dir != 1 {
            return Err(unspecified(py));
        }
        let copy = match transform {
            Some(t) => Some(create_editable_copy(py, t)?),
            None => None,
        };
        if dir == 0 {
            self.forward = copy;
        } else {
            self.inverse = copy;
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

fn empty_list(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    Ok(pyo3::types::PyList::empty(py).into_any())
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

// .def(py::init([]() { return NamedTransform::Create(); })), .def(py::init([](name, aliases,
// family, description, forwardTransform, inverseTransform, categories) {...}),
// "name"_a.none(false) = "", "aliases"_a = getAliasesStdVec(DEFAULT), "family"_a.none(false) =
// "", "description"_a.none(false) = "", "forwardTransform"_a = ConstTransformRcPtr(),
// "inverseTransform"_a = ConstTransformRcPtr(), "categories"_a = getCategoriesStdVec(DEFAULT))
// (PyNamedTransform.cpp:66-123 @ v2.5.2).
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
                construct(
                    &a[0],
                    NAMED_TRANSFORM,
                    Shared::new(NamedTransformNode::new()),
                )?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                text_param("name"),
                Param {
                    name: "aliases",
                    ty: STRINGS,
                    default: Some(Default {
                        repr: "[]",
                        value: empty_list,
                    }),
                },
                text_param("family"),
                text_param("description"),
                Param {
                    name: "forwardTransform",
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
                    name: "categories",
                    ty: STRINGS,
                    default: Some(Default {
                        repr: "[]",
                        value: empty_list,
                    }),
                },
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let StdString(name) = crate::arg!(a, 1, convert, StdString);
                let aliases = crate::arg!(a, 2, convert, Vec<StdString>);
                let StdString(family) = crate::arg!(a, 3, convert, StdString);
                let StdString(description) = crate::arg!(a, 4, convert, StdString);
                let forward = transform_param!(o, a, 5, convert);
                let inverse = transform_param!(o, a, 6, convert);
                let categories = crate::arg!(a, 7, convert, Vec<StdString>);
                let mut p = NamedTransformNode::new();
                if !aliases.is_empty() {
                    p.nt.clear_aliases();
                    for StdString(alias) in &aliases {
                        p.nt.add_alias(c_str(alias));
                    }
                }
                // Setting the name will remove alias named the same, so set name after.
                if !name.is_empty() {
                    p.nt.set_name(c_str(&name));
                }
                if !family.is_empty() {
                    p.nt.set_family(c_str(&family));
                }
                if !description.is_empty() {
                    p.nt.set_description(c_str(&description));
                }
                if let Some(t) = &forward {
                    p.set_transform(py, Some(t), 0)?;
                }
                if let Some(t) = &inverse {
                    p.set_transform(py, Some(t), 1)?;
                }
                if !categories.is_empty() {
                    p.nt.clear_categories();
                    for StdString(category) in &categories {
                        p.nt.add_category(c_str(category));
                    }
                }
                construct(&a[0], NAMED_TRANSFORM, Shared::new(p))?;
                none(py)
            },
            pos_only: 0,
        },
    ],
};

// .def("__deepcopy__", [](const ConstNamedTransformRcPtr & self, py::dict) { return
// self->createEditableCopy(); }, "memo"_a) (PyNamedTransform.cpp:125-129 @ v2.5.2).
static DEEPCOPY: Function = method_def!(
    "__deepcopy__",
    [SELF, Param::new("memo", "dict")],
    NAMED_TRANSFORM,
    |py, o, a, _| {
        let rc = this_shared!(o, a, NamedTransformNode);
        if !a[1].is_instance_of::<pyo3::types::PyDict>() {
            return Ok(None);
        }
        let copy = rc.lock().copy(py)?;
        out(py, NAMED_TRANSFORM, &Shared::new(copy))
    }
);

/// A getter of one `const char *` of the named transform.
macro_rules! text_getter {
    ($name:literal, $get:ident) => {
        method_def!($name, [SELF], "str", |py, o, a, _| {
            let rc = this_shared!(o, a, NamedTransformNode);
            let text = c_str(rc.lock().nt.$get()).to_vec();
            string_out(py, &text)
        })
    };
}

/// A method `(<param>: str)` of the named transform, declared `.none(false)`.
macro_rules! text_setter {
    ($name:literal, $param:literal, $set:ident) => {
        method_def!(
            $name,
            [SELF, Param::new($param, "str")],
            "None",
            |py, o, a, convert| {
                let rc = this_shared!(o, a, NamedTransformNode);
                let NonNullCharPtr(s) = crate::arg!(a, 1, convert, NonNullCharPtr);
                rc.lock().nt.$set(&s);
                none(py)
            }
        )
    };
}

/// A method `(<param>: str)` of the named transform, a null pointer as `""`.
macro_rules! nullable_method {
    ($name:literal, $param:literal, $ret:expr, $body:expr) => {
        method_def!(
            $name,
            [SELF, Param::new($param, "str")],
            $ret,
            |py, o, a, convert| {
                let rc = this_shared!(o, a, NamedTransformNode);
                let s = crate::arg!(a, 1, convert, CharPtr).0.unwrap_or_default();
                $body(py, &mut rc.lock().nt, &s)
            }
        )
    };
}

/// A method returning the iterator class `$qualified` over the named transform.
macro_rules! iterator_getter {
    ($name:literal, $qualified:expr) => {
        method_def!($name, [SELF], $qualified, |py, o, a, _| {
            let rc = this_shared!(o, a, NamedTransformNode);
            Ok(Some(
                new_iterator(py, $qualified, Some(Box::new(rc)))?.unbind(),
            ))
        })
    };
}

// .def("getName", ...), .def("setName", ..., "name"_a.none(false)) (PyNamedTransform.cpp:
// 131-134 @ v2.5.2).
static GET_NAME: Function = text_getter!("getName", name);
static SET_NAME: Function = text_setter!("setName", "name", set_name);

// Aliases (PyNamedTransform.cpp:136-148 @ v2.5.2).
static HAS_ALIAS: Function = nullable_method!(
    "hasAlias",
    "alias",
    "bool",
    |py, nt: &mut NamedTransform, s: &[u8]| ok(py, nt.has_alias(s))
);
static ADD_ALIAS: Function = text_setter!("addAlias", "alias", add_alias);
static REMOVE_ALIAS: Function = text_setter!("removeAlias", "alias", remove_alias);
static GET_ALIASES: Function = iterator_getter!("getAliases", ALIAS_ITERATOR);
static CLEAR_ALIASES: Function = method_def!("clearAliases", [SELF], "None", |py, o, a, _| {
    let rc = this_shared!(o, a, NamedTransformNode);
    rc.lock().nt.clear_aliases();
    none(py)
});

// .def("getFamily", ...) to .def("setEncoding", ...) (PyNamedTransform.cpp:150-161 @ v2.5.2),
// each setter's parameter `.none(false)`.
static GET_FAMILY: Function = text_getter!("getFamily", family);
static SET_FAMILY: Function = text_setter!("setFamily", "family", set_family);
static GET_DESCRIPTION: Function = text_getter!("getDescription", description);
static SET_DESCRIPTION: Function = text_setter!("setDescription", "description", set_description);
static GET_ENCODING: Function = text_getter!("getEncoding", encoding);
static SET_ENCODING: Function = text_setter!("setEncoding", "encoding", set_encoding);

// .def("getTransform", &NamedTransform::getTransform, "direction"_a), .def("setTransform",
// &NamedTransform::setTransform, "transform"_a, "direction"_a) (PyNamedTransform.cpp:163-167 @
// v2.5.2).
static GET_TRANSFORM: Function = method_def!(
    "getTransform",
    [SELF, Param::new("direction", TRANSFORM_DIRECTION)],
    TRANSFORM,
    |py, o, a, _| {
        let rc = this_shared!(o, a, NamedTransformNode);
        let dir = enum_param!(o, a, 1);
        let t = rc.lock().transform(py, dir)?;
        transform_out(py, t.as_ref())
    }
);
static SET_TRANSFORM: Function = method_def!(
    "setTransform",
    [
        SELF,
        Param::new("transform", TRANSFORM),
        Param::new("direction", TRANSFORM_DIRECTION)
    ],
    "None",
    |py, o, a, convert| {
        let rc = this_shared!(o, a, NamedTransformNode);
        let t = transform_param!(o, a, 1, convert);
        let dir = enum_param!(o, a, 2);
        rc.lock().set_transform(py, t.as_ref(), dir)?;
        none(py)
    }
);

/// Port of `NamedTransform::GetTransform` (src/OpenColorIO/NamedTransform.cpp:182-221 @
/// v2.5.2): the transform of the direction, else a copy of the other inverted; a null named
/// transform or a direction outside the enumerators is upstream's error, and neither transform
/// the port's (U-124: upstream copies the null one).
pub(crate) fn get_transform(
    py: Python<'_>,
    nt: Option<&NamedTransformRc>,
    dir: i64,
) -> PyResult<Option<TransformRc>> {
    let Some(nt) = nt else {
        return Err(unspecified(py));
    };
    let (forward, inverse) = {
        let node = nt.lock();
        (node.forward.clone(), node.inverse.clone())
    };
    let (this, other) = match dir {
        0 => (forward, inverse),
        1 => (inverse, forward),
        _ => return Err(unspecified(py)),
    };
    if let Some(this) = this {
        return Ok(Some(this));
    }
    let Some(other) = other else {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "NamedTransform.GetTransform: neither transform is set, and the wheel copies the \
             null one (its process dies; docs/improvements.md, U-124)",
        ));
    };
    let transform = create_editable_copy(py, &other)?;
    let current = direction_of(&transform.lock());
    // GetInverseTransformDirection (ParseUtils.cpp:164-169 @ v2.5.2).
    set_direction(&transform, if current == 0 { 1 } else { 0 });
    Ok(Some(transform))
}

// .def_static("GetTransform", &NamedTransform::GetTransform, "transform"_a, "direction"_a)
// (PyNamedTransform.cpp:169-170 @ v2.5.2).
static GET_TRANSFORM_STATIC: Function = static_def!(
    "GetTransform",
    [
        Param::new("transform", NAMED_TRANSFORM),
        Param::new("direction", TRANSFORM_DIRECTION)
    ],
    TRANSFORM,
    |py, o, a, convert| {
        let Some(nt) = shared_arg::<NamedTransformNode>(&a[0], NAMED_TRANSFORM, convert)? else {
            return Ok(None);
        };
        let dir = enum_param!(o, a, 1);
        transform_out(py, get_transform(py, nt.as_ref(), dir)?.as_ref())
    }
);

// Categories (PyNamedTransform.cpp:172-184 @ v2.5.2).
static HAS_CATEGORY: Function = nullable_method!(
    "hasCategory",
    "category",
    "bool",
    |py, nt: &mut NamedTransform, s: &[u8]| ok(py, nt.has_category(s))
);
static ADD_CATEGORY: Function = nullable_method!(
    "addCategory",
    "category",
    "None",
    |py, nt: &mut NamedTransform, s: &[u8]| {
        nt.add_category(s);
        none(py)
    }
);
static REMOVE_CATEGORY: Function = nullable_method!(
    "removeCategory",
    "category",
    "None",
    |py, nt: &mut NamedTransform, s: &[u8]| {
        nt.remove_category(s);
        none(py)
    }
);
static GET_CATEGORIES: Function = iterator_getter!("getCategories", CATEGORY_ITERATOR);
static CLEAR_CATEGORIES: Function =
    method_def!("clearCategories", [SELF], "None", |py, o, a, _| {
        let rc = this_shared!(o, a, NamedTransformNode);
        rc.lock().nt.clear_categories();
        none(py)
    });

// defRepr(clsNamedTransform) (PyNamedTransform.cpp:186 @ v2.5.2).
static REPR: Function = method_def!("__repr__", [SELF], "str", |py, o, a, _| {
    let rc = this_shared!(o, a, NamedTransformNode);
    let text = rc.lock().materialize()?.to_bytes();
    string_out(py, &text)
});

// ---------------------------------------------------------------------------------------------
// The iterators (PyNamedTransform.cpp:188-221 @ v2.5.2)

/// The named transform an iterator of `self` walks, or the next overload.
macro_rules! iter_rc {
    ($o:expr, $a:expr) => {{
        let it = this_iter!($o, $a);
        let rc = it.get().obj::<NamedTransformRc>()?.clone();
        (it, rc)
    }};
}

fn num_categories(rc: &NamedTransformRc) -> i32 {
    rc.lock().nt.num_categories()
}

fn num_aliases(rc: &NamedTransformRc) -> i32 {
    rc.lock().nt.num_aliases() as i32
}

/// `getCategory(i)`: `None` outside the categories.
fn category(py: Python<'_>, rc: &NamedTransformRc, i: i32) -> Out {
    let c = rc.lock().nt.category(i).map(<[u8]>::to_vec);
    Ok(Some(
        crate::dispatch::char_ptr_to_py(py, c.as_deref())?.unbind(),
    ))
}

/// `getAlias(i)`: the `int` made a `size_t`, `""` outside the aliases.
fn alias(py: Python<'_>, rc: &NamedTransformRc, i: i32) -> Out {
    let a = rc.lock().nt.alias(i as isize as usize).to_vec();
    string_out(py, &a)
}

/// The four methods of an iterator class.
macro_rules! iterator {
    ($self:expr, $qualified:expr, $num:expr, $item:expr,
     $len:ident, $getitem:ident, $iter:ident, $next:ident) => {
        static $len: Function = method_def!("__len__", [$self], "int", |py, o, a, _| {
            let (_, rc) = iter_rc!(o, a);
            ok(py, $num(&rc))
        });

        static $getitem: Function = method_def!(
            "__getitem__",
            [$self, Param::new("arg0", INT)],
            "str",
            |py, o, a, convert| {
                let (_, rc) = iter_rc!(o, a);
                let i = crate::arg!(a, 1, convert, i32);
                check_index(i, $num(&rc))?;
                $item(py, &rc, i)
            }
        );

        static $iter: Function = method_def!("__iter__", [$self], $qualified, |_, o, a, _| {
            let _ = iter_rc!(o, a);
            Ok(Some(a[0].clone().unbind()))
        });

        static $next: Function = method_def!("__next__", [$self], "str", |py, o, a, _| {
            let (it, rc) = iter_rc!(o, a);
            let i = it.get().next_index($num(&rc))?;
            $item(py, &rc, i)
        });
    };
}

const CAT_SELF: Param = Param::new("self", CATEGORY_ITERATOR);
const ALIAS_SELF: Param = Param::new("self", ALIAS_ITERATOR);

iterator!(
    CAT_SELF,
    CATEGORY_ITERATOR,
    num_categories,
    category,
    CAT_LEN,
    CAT_GETITEM,
    CAT_ITER,
    CAT_NEXT
);
iterator!(
    ALIAS_SELF,
    ALIAS_ITERATOR,
    num_aliases,
    alias,
    ALIAS_LEN,
    ALIAS_GETITEM,
    ALIAS_ITER,
    ALIAS_NEXT
);

/// Port of `bindPyNamedTransform` (src/bindings/python/PyNamedTransform.cpp:51-222 @ v2.5.2),
/// in its order: the iterator classes nested in the class (57-65), the class's methods
/// (66-184), `__repr__` (186), the iterators' (188-221).
pub(crate) fn bind_py_named_transform(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let cls = class(py, NAMED_TRANSFORM)?;
    let categories = nested_class(&cls, "NamedTransform", "NamedTransformCategoryIterator")?;
    let aliases = nested_class(&cls, "NamedTransform", "NamedTransformAliasIterator")?;
    for f in [
        &INIT,
        &DEEPCOPY,
        &GET_NAME,
        &SET_NAME,
        &HAS_ALIAS,
        &ADD_ALIAS,
        &REMOVE_ALIAS,
        &GET_ALIASES,
        &CLEAR_ALIASES,
        &GET_FAMILY,
        &SET_FAMILY,
        &GET_DESCRIPTION,
        &SET_DESCRIPTION,
        &GET_ENCODING,
        &SET_ENCODING,
        &GET_TRANSFORM,
        &SET_TRANSFORM,
    ] {
        def(&cls, f)?;
    }
    def_static(&cls, &GET_TRANSFORM_STATIC)?;
    for f in [
        &HAS_CATEGORY,
        &ADD_CATEGORY,
        &REMOVE_CATEGORY,
        &GET_CATEGORIES,
        &CLEAR_CATEGORIES,
        &REPR,
    ] {
        def(&cls, f)?;
    }
    for f in [&CAT_LEN, &CAT_GETITEM, &CAT_ITER, &CAT_NEXT] {
        def(&categories, f)?;
    }
    for f in [&ALIAS_LEN, &ALIAS_GETITEM, &ALIAS_ITER, &ALIAS_NEXT] {
        def(&aliases, f)?;
    }
    Ok(())
}
