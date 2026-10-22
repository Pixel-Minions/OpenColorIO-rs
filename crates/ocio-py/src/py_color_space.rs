// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyColorSpace` (src/bindings/python/PyColorSpace.cpp @ v2.5.2): the class `ColorSpace`
//! and its two iterator classes (chunk 6.3e).
//!
//! **Shared transforms.** pybind11 holds a color space by its `ColorSpaceRcPtr`, and
//! `getTransform` returns the very `TransformRcPtr` the color space holds: the same Python
//! object each time, through which a change shows in the color space; `setTransform` stores a
//! copy (`createEditableCopy`: a group's copy shares its children). The library holds a color
//! space's transforms by value, so the binding holds a color space as a [`ColorSpaceNode`]: the
//! library's color space without its transforms, and the transforms' shared nodes
//! ([`TransformRc`]), from which it builds the library's color space when it needs one
//! ([`ColorSpaceNode::materialize`]).

use ocio::ColorSpace;
use ocio_ops::open_color_types::{ColorSpaceDirection, ReferenceSpaceType};
use ocio_ops::utils::string_utils::c_str;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::dispatch::{CharPtr, Default, Function, NonNullCharPtr, Overload, Param, StdString};
use crate::py_open_color_io::{py_result, to_py_err};
use crate::py_transform::{
    BIT_DEPTH, INT, Out, TRANSFORM, TransformRc, bit_depth, class, create_editable_copy, def,
    enum_out, holder_arg, nested_class, none, ok, outside_enumerators, string_out, transform_out,
};
use crate::py_utils::{check_index, new_iterator};
use crate::shared::{Shared, construct, out};
use crate::transforms::py_allocation_transform::{ALLOCATION, allocation};
use crate::{enum_param, method_def, q, this_iter, this_shared};

pub(crate) const COLOR_SPACE: &str = q!("ColorSpace");
const CATEGORY_ITERATOR: &str = q!("ColorSpace.ColorSpaceCategoryIterator");
const ALIAS_ITERATOR: &str = q!("ColorSpace.ColorSpaceAliasIterator");
const REFERENCE_SPACE_TYPE: &str = q!("ReferenceSpaceType");
const COLOR_SPACE_DIRECTION: &str = q!("ColorSpaceDirection");
const SELF: Param = Param::new("self", COLOR_SPACE);
const STRINGS: &str = "collections.abc.Sequence[str]";
const FLOATS: &str = "collections.abc.Sequence[typing.SupportsFloat]";

/// A color space as the binding holds it (see the module's documentation).
#[derive(Debug)]
pub(crate) struct ColorSpaceNode {
    /// The color space, without its transforms.
    pub(crate) cs: ColorSpace,
    /// `m_toRefTransform`.
    pub(crate) to_ref: Option<TransformRc>,
    /// `m_fromRefTransform`.
    pub(crate) from_ref: Option<TransformRc>,
}

/// A color space shared by the Python objects that hold it.
pub(crate) type ColorSpaceRc = Shared<ColorSpaceNode>;

impl ColorSpaceNode {
    fn new(reference: ReferenceSpaceType) -> ColorSpaceNode {
        ColorSpaceNode {
            cs: ColorSpace::with_reference_space(reference),
            to_ref: None,
            from_ref: None,
        }
    }

    /// Port of `ColorSpace::createEditableCopy` and `ColorSpace::Impl::operator=`
    /// (src/OpenColorIO/ColorSpace.cpp:64-95, 128-133 @ v2.5.2): the transforms copied with
    /// `createEditableCopy`, the one to the reference first (an invalid fixed function
    /// transform's error).
    pub(crate) fn copy(&self, py: Python<'_>) -> PyResult<ColorSpaceNode> {
        let to_ref = match &self.to_ref {
            Some(t) => Some(create_editable_copy(py, t)?),
            None => None,
        };
        let from_ref = match &self.from_ref {
            Some(t) => Some(create_editable_copy(py, t)?),
            None => None,
        };
        Ok(ColorSpaceNode {
            cs: self.cs.clone(),
            to_ref,
            from_ref,
        })
    }

    /// The library's color space with its transforms as they are now, stored as they are
    /// (`set_transform_as_is`): upstream prints the transforms it holds without copying them, so
    /// one made invalid through Python since it was set prints as it is.
    pub(crate) fn materialize(&self) -> PyResult<ColorSpace> {
        let mut cs = self.cs.clone();
        for (t, dir) in [
            (&self.to_ref, ColorSpaceDirection::ToReference),
            (&self.from_ref, ColorSpaceDirection::FromReference),
        ] {
            if let Some(t) = t {
                cs.set_transform_as_is(Some(t.materialize()?), dir);
            }
        }
        Ok(cs)
    }

    /// Port of `ColorSpace::getTransform` (ColorSpace.cpp:464-474 @ v2.5.2): the transform of
    /// the direction, none for a direction outside the enumerators.
    fn transform(&self, dir: i64) -> Option<&TransformRc> {
        match dir {
            0 => self.to_ref.as_ref(),
            1 => self.from_ref.as_ref(),
            _ => None,
        }
    }

    /// Port of `ColorSpace::setTransform` (ColorSpace.cpp:476-491 @ v2.5.2): a copy of
    /// `transform`, stored for a direction of the enumerators only.
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

/// The library's `ReferenceSpaceType` of the C++ value `v` (U-112 for another).
pub(crate) fn reference_space_type(v: i64) -> PyResult<ReferenceSpaceType> {
    [ReferenceSpaceType::Scene, ReferenceSpaceType::Display]
        .into_iter()
        .find(|r| *r as i64 == v)
        .ok_or_else(|| outside_enumerators(REFERENCE_SPACE_TYPE, v))
}

/// A `const char *` argument's bytes, a null pointer as the empty string (the setters'
/// `s ? s : ""`, ColorSpace.cpp:137-462 @ v2.5.2).
fn or_empty(s: CharPtr) -> Vec<u8> {
    s.0.unwrap_or_default()
}

/// The error of the binding's `allocationVars` checks (PyColorSpace.cpp:111-118, 193-200 @
/// v2.5.2).
fn vars_size(py: Python<'_>) -> PyErr {
    to_py_err(
        py,
        &ocio::Exception::new("vars must be a float array, size 2 or 3"),
    )
}

/// A `TransformRcPtr` argument `a[i]` (its class the parameter's type; `None`, a null pointer,
/// with conversions), or the next overload.
macro_rules! transform_param {
    ($o:expr, $a:expr, $i:expr, $convert:expr) => {
        match holder_arg(&$a[$i], $o.params[$i].ty, $convert)? {
            Some(rc) => rc,
            None => return Ok(None),
        }
    };
}

fn empty_list(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    Ok(pyo3::types::PyList::empty(py).into_any())
}

fn empty_str(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    Ok(pyo3::types::PyString::new(py, "").into_any())
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

// .def(py::init([]() { return ColorSpace::Create(); })), .def(py::init([](ReferenceSpaceType
// referenceSpace) {...}), "referenceSpace"_a), .def(py::init([](referenceSpace, name, aliases,
// family, encoding, equalityGroup, description, bitDepth, isData, allocation, allocationVars,
// toReference, fromReference, categories, interopID) {...}), ...) (PyColorSpace.cpp:68-160 @
// v2.5.2).
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
                let node = ColorSpaceNode::new(ReferenceSpaceType::Scene);
                construct(&a[0], COLOR_SPACE, Shared::new(node))?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[SELF, Param::new("referenceSpace", REFERENCE_SPACE_TYPE)],
            ret: "None",
            call: |py, o, a, _| {
                let reference = reference_space_type(enum_param!(o, a, 1))?;
                construct(
                    &a[0],
                    COLOR_SPACE,
                    Shared::new(ColorSpaceNode::new(reference)),
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
                Param {
                    name: "aliases",
                    ty: STRINGS,
                    default: Some(Default {
                        repr: "[]",
                        value: empty_list,
                    }),
                },
                text_param("family"),
                text_param("encoding"),
                text_param("equalityGroup"),
                text_param("description"),
                Param {
                    name: "bitDepth",
                    ty: BIT_DEPTH,
                    default: Some(Default {
                        repr: "<BitDepth.BIT_DEPTH_UNKNOWN: 0>",
                        value: |py| {
                            crate::enums::to_py(&crate::enums::enum_class(py, BIT_DEPTH)?, 0)
                        },
                    }),
                },
                Param {
                    name: "isData",
                    ty: "bool",
                    default: Some(Default {
                        repr: "False",
                        value: |py| Ok(pyo3::types::PyBool::new(py, false).to_owned().into_any()),
                    }),
                },
                Param {
                    name: "allocation",
                    ty: ALLOCATION,
                    default: Some(Default {
                        repr: "<Allocation.ALLOCATION_UNIFORM: 1>",
                        value: |py| {
                            crate::enums::to_py(&crate::enums::enum_class(py, ALLOCATION)?, 1)
                        },
                    }),
                },
                Param {
                    name: "allocationVars",
                    ty: FLOATS,
                    default: Some(Default {
                        repr: "[]",
                        value: empty_list,
                    }),
                },
                Param {
                    name: "toReference",
                    ty: TRANSFORM,
                    default: Some(Default {
                        repr: "None",
                        value: |py| Ok(py.None().into_bound(py)),
                    }),
                },
                Param {
                    name: "fromReference",
                    ty: TRANSFORM,
                    default: Some(Default {
                        repr: "None",
                        value: |py| Ok(py.None().into_bound(py)),
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
                text_param("interopID"),
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let reference = enum_param!(o, a, 1);
                let StdString(name) = crate::arg!(a, 2, convert, StdString);
                let aliases = crate::arg!(a, 3, convert, Vec<StdString>);
                let StdString(family) = crate::arg!(a, 4, convert, StdString);
                let StdString(encoding) = crate::arg!(a, 5, convert, StdString);
                let StdString(equality_group) = crate::arg!(a, 6, convert, StdString);
                let StdString(description) = crate::arg!(a, 7, convert, StdString);
                let bd = enum_param!(o, a, 8);
                let is_data = crate::arg!(a, 9, convert, bool);
                let alloc = enum_param!(o, a, 10);
                let vars = crate::arg!(a, 11, convert, Vec<f32>);
                let to_ref = transform_param!(o, a, 12, convert);
                let from_ref = transform_param!(o, a, 13, convert);
                let categories = crate::arg!(a, 14, convert, Vec<StdString>);
                let StdString(interop_id) = crate::arg!(a, 15, convert, StdString);
                let (reference, bd, alloc) = (
                    reference_space_type(reference)?,
                    bit_depth(bd)?,
                    allocation(alloc)?,
                );
                let mut p = ColorSpaceNode::new(reference);
                if !aliases.is_empty() {
                    p.cs.clear_aliases();
                    for StdString(alias) in &aliases {
                        p.cs.add_alias(c_str(alias));
                    }
                }
                // Setting the name will remove alias named the same, so set name after.
                if !name.is_empty() {
                    p.cs.set_name(c_str(&name));
                }
                if !family.is_empty() {
                    p.cs.set_family(c_str(&family));
                }
                if !encoding.is_empty() {
                    p.cs.set_encoding(c_str(&encoding));
                }
                if !equality_group.is_empty() {
                    p.cs.set_equality_group(c_str(&equality_group));
                }
                if !description.is_empty() {
                    p.cs.set_description(c_str(&description));
                }
                if !interop_id.is_empty() {
                    py_result(py, p.cs.set_interop_id(c_str(&interop_id)))?;
                }
                p.cs.set_bit_depth(bd);
                p.cs.set_is_data(is_data);
                p.cs.set_allocation(alloc);
                if !vars.is_empty() {
                    if vars.len() < 2 || vars.len() > 3 {
                        return Err(vars_size(py));
                    }
                    p.cs.set_allocation_vars(&vars);
                }
                if let Some(t) = &to_ref {
                    p.set_transform(py, Some(t), 0)?;
                }
                if let Some(t) = &from_ref {
                    p.set_transform(py, Some(t), 1)?;
                }
                if !categories.is_empty() {
                    p.cs.clear_categories();
                    for StdString(category) in &categories {
                        p.cs.add_category(c_str(category));
                    }
                }
                construct(&a[0], COLOR_SPACE, Shared::new(p))?;
                none(py)
            },
            pos_only: 0,
        },
    ],
};

// .def("__deepcopy__", [](const ConstColorSpaceRcPtr & self, py::dict) { return
// self->createEditableCopy(); }, "memo"_a) (PyColorSpace.cpp:162-166 @ v2.5.2).
static DEEPCOPY: Function = method_def!(
    "__deepcopy__",
    [SELF, Param::new("memo", "dict")],
    COLOR_SPACE,
    |py, o, a, _| {
        let rc = this_shared!(o, a, ColorSpaceNode);
        if !a[1].is_instance_of::<PyDict>() {
            return Ok(None);
        }
        let copy = rc.lock().copy(py)?;
        out(py, COLOR_SPACE, &Shared::new(copy))
    }
);

/// A getter of one `const char *` of the color space.
macro_rules! text_getter {
    ($name:literal, $get:ident) => {
        method_def!($name, [SELF], "str", |py, o, a, _| {
            let rc = this_shared!(o, a, ColorSpaceNode);
            let text = c_str(rc.lock().cs.$get()).to_vec();
            string_out(py, &text)
        })
    };
}

/// A setter of one `const char *` of the color space, a null pointer as `""`.
macro_rules! text_setter {
    ($name:literal, $param:literal, $set:ident) => {
        method_def!(
            $name,
            [SELF, Param::new($param, "str")],
            "None",
            |py, o, a, convert| {
                let rc = this_shared!(o, a, ColorSpaceNode);
                let s = or_empty(crate::arg!(a, 1, convert, CharPtr));
                rc.lock().cs.$set(&s);
                none(py)
            }
        )
    };
}

// .def("getName", ...), .def("setName", ..., "name"_a.none(false)) (PyColorSpace.cpp:168-171
// @ v2.5.2).
static GET_NAME: Function = text_getter!("getName", name);
static SET_NAME: Function = method_def!(
    "setName",
    [SELF, Param::new("name", "str")],
    "None",
    |py, o, a, convert| {
        let rc = this_shared!(o, a, ColorSpaceNode);
        let NonNullCharPtr(name) = crate::arg!(a, 1, convert, NonNullCharPtr);
        rc.lock().cs.set_name(&name);
        none(py)
    }
);

// .def("hasAlias", &ColorSpace::hasAlias, "alias"_a) (PyColorSpace.cpp:174-175 @ v2.5.2): a
// null alias is none.
static HAS_ALIAS: Function = method_def!(
    "hasAlias",
    [SELF, Param::new("alias", "str")],
    "bool",
    |py, o, a, convert| {
        let rc = this_shared!(o, a, ColorSpaceNode);
        let alias = crate::arg!(a, 1, convert, CharPtr);
        let has = alias.0.is_some_and(|alias| rc.lock().cs.has_alias(&alias));
        ok(py, has)
    }
);

// .def("addAlias", ..., "alias"_a.none(false)), .def("removeAlias", ..., "alias"_a.none(false))
// (PyColorSpace.cpp:176-179 @ v2.5.2).
static ADD_ALIAS: Function = method_def!(
    "addAlias",
    [SELF, Param::new("alias", "str")],
    "None",
    |py, o, a, convert| {
        let rc = this_shared!(o, a, ColorSpaceNode);
        let NonNullCharPtr(alias) = crate::arg!(a, 1, convert, NonNullCharPtr);
        rc.lock().cs.add_alias(&alias);
        none(py)
    }
);
static REMOVE_ALIAS: Function = method_def!(
    "removeAlias",
    [SELF, Param::new("alias", "str")],
    "None",
    |py, o, a, convert| {
        let rc = this_shared!(o, a, ColorSpaceNode);
        let NonNullCharPtr(alias) = crate::arg!(a, 1, convert, NonNullCharPtr);
        rc.lock().cs.remove_alias(&alias);
        none(py)
    }
);

/// A method returning the iterator class `$qualified` over the color space.
macro_rules! iterator_getter {
    ($name:literal, $qualified:expr) => {
        method_def!($name, [SELF], $qualified, |py, o, a, _| {
            let rc = this_shared!(o, a, ColorSpaceNode);
            Ok(Some(
                new_iterator(py, $qualified, Some(Box::new(rc)))?.unbind(),
            ))
        })
    };
}

// .def("getAliases", ...), .def("clearAliases", ...) (PyColorSpace.cpp:180-185 @ v2.5.2).
static GET_ALIASES: Function = iterator_getter!("getAliases", ALIAS_ITERATOR);
static CLEAR_ALIASES: Function = method_def!("clearAliases", [SELF], "None", |py, o, a, _| {
    let rc = this_shared!(o, a, ColorSpaceNode);
    rc.lock().cs.clear_aliases();
    none(py)
});

// .def("getFamily", ...) to .def("setInteropID", ...) (PyColorSpace.cpp:187-202 @ v2.5.2).
static GET_FAMILY: Function = text_getter!("getFamily", family);
static SET_FAMILY: Function = text_setter!("setFamily", "family", set_family);
static GET_ENCODING: Function = text_getter!("getEncoding", encoding);
static SET_ENCODING: Function = text_setter!("setEncoding", "encoding", set_encoding);
static GET_EQUALITY_GROUP: Function = text_getter!("getEqualityGroup", equality_group);
static SET_EQUALITY_GROUP: Function =
    text_setter!("setEqualityGroup", "equalityGroup", set_equality_group);
static GET_DESCRIPTION: Function = text_getter!("getDescription", description);
static SET_DESCRIPTION: Function = text_setter!("setDescription", "description", set_description);
static GET_INTEROP_ID: Function = text_getter!("getInteropID", interop_id);
static SET_INTEROP_ID: Function = method_def!(
    "setInteropID",
    [SELF, Param::new("interopID", "str")],
    "None",
    |py, o, a, convert| {
        let rc = this_shared!(o, a, ColorSpaceNode);
        let id = or_empty(crate::arg!(a, 1, convert, CharPtr));
        py_result(py, rc.lock().cs.set_interop_id(&id))?;
        none(py)
    }
);

// .def("getInterchangeAttribute", ..., "attrName"_a), .def("setInterchangeAttribute", ...,
// "attrName"_a, "attrValue"_a), .def("getInterchangeAttributes", ...) (PyColorSpace.cpp:203-208
// @ v2.5.2).
static GET_INTERCHANGE_ATTRIBUTE: Function = method_def!(
    "getInterchangeAttribute",
    [SELF, Param::new("attrName", "str")],
    "str",
    |py, o, a, convert| {
        let rc = this_shared!(o, a, ColorSpaceNode);
        let name = or_empty(crate::arg!(a, 1, convert, CharPtr));
        let value = py_result(
            py,
            rc.lock()
                .cs
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
        let rc = this_shared!(o, a, ColorSpaceNode);
        let name = or_empty(crate::arg!(a, 1, convert, CharPtr));
        let value = or_empty(crate::arg!(a, 2, convert, CharPtr));
        py_result(py, rc.lock().cs.set_interchange_attribute(&name, &value))?;
        none(py)
    }
);
static GET_INTERCHANGE_ATTRIBUTES: Function = method_def!(
    "getInterchangeAttributes",
    [SELF],
    "dict[str, str]",
    |py, o, a, _| {
        let rc = this_shared!(o, a, ColorSpaceNode);
        let attributes = rc.lock().cs.interchange_attributes().clone();
        interchange_dict(py, &attributes)
    }
);

/// A returned `std::map<std::string, std::string>`: a dict, each key and value decoded as
/// strict UTF-8, in the keys' order.
pub(crate) fn interchange_dict(
    py: Python<'_>,
    map: &std::collections::BTreeMap<Vec<u8>, Vec<u8>>,
) -> Out {
    let dict = PyDict::new(py);
    for (k, v) in map {
        dict.set_item(
            crate::dispatch::std_string_to_py(py, k)?,
            crate::dispatch::std_string_to_py(py, v)?,
        )?;
    }
    Ok(Some(dict.into_any().unbind()))
}

// .def("getBitDepth", ...), .def("setBitDepth", ..., "bitDepth"_a) (PyColorSpace.cpp:209-212
// @ v2.5.2).
static GET_BIT_DEPTH: Function = method_def!("getBitDepth", [SELF], BIT_DEPTH, |py, o, a, _| {
    let rc = this_shared!(o, a, ColorSpaceNode);
    let bd = rc.lock().cs.bit_depth();
    enum_out(py, BIT_DEPTH, bd as i64)
});
static SET_BIT_DEPTH: Function = method_def!(
    "setBitDepth",
    [SELF, Param::new("bitDepth", BIT_DEPTH)],
    "None",
    |py, o, a, _| {
        let rc = this_shared!(o, a, ColorSpaceNode);
        let bd = bit_depth(enum_param!(o, a, 1))?;
        rc.lock().cs.set_bit_depth(bd);
        none(py)
    }
);

/// A method `(category: str)` of the categories, a null pointer as `""` (the tokens' `!s ||
/// !*s`, TokensManager.h).
macro_rules! category_method {
    ($name:literal, $ret:expr, $body:expr) => {
        method_def!(
            $name,
            [SELF, Param::new("category", "str")],
            $ret,
            |py, o, a, convert| {
                let rc = this_shared!(o, a, ColorSpaceNode);
                let c = or_empty(crate::arg!(a, 1, convert, CharPtr));
                $body(py, &mut rc.lock().cs, &c)
            }
        )
    };
}

// .def("hasCategory", ...), .def("addCategory", ...), .def("removeCategory", ...),
// .def("getCategories", ...), .def("clearCategories", ...) (PyColorSpace.cpp:215-226 @ v2.5.2).
static HAS_CATEGORY: Function = category_method!(
    "hasCategory",
    "bool",
    |py, cs: &mut ColorSpace, c: &[u8]| { ok(py, cs.has_category(c)) }
);
static ADD_CATEGORY: Function = category_method!(
    "addCategory",
    "None",
    |py, cs: &mut ColorSpace, c: &[u8]| {
        cs.add_category(c);
        none(py)
    }
);
static REMOVE_CATEGORY: Function = category_method!(
    "removeCategory",
    "None",
    |py, cs: &mut ColorSpace, c: &[u8]| {
        cs.remove_category(c);
        none(py)
    }
);
static GET_CATEGORIES: Function = iterator_getter!("getCategories", CATEGORY_ITERATOR);
static CLEAR_CATEGORIES: Function =
    method_def!("clearCategories", [SELF], "None", |py, o, a, _| {
        let rc = this_shared!(o, a, ColorSpaceNode);
        rc.lock().cs.clear_categories();
        none(py)
    });

// .def("isData", ...), .def("setIsData", ..., "isData"_a), .def("getReferenceSpaceType", ...)
// (PyColorSpace.cpp:229-234 @ v2.5.2).
static IS_DATA: Function = method_def!("isData", [SELF], "bool", |py, o, a, _| {
    let rc = this_shared!(o, a, ColorSpaceNode);
    let d = rc.lock().cs.is_data();
    ok(py, d)
});
static SET_IS_DATA: Function = method_def!(
    "setIsData",
    [SELF, Param::new("isData", "bool")],
    "None",
    |py, o, a, convert| {
        let rc = this_shared!(o, a, ColorSpaceNode);
        let d = crate::arg!(a, 1, convert, bool);
        rc.lock().cs.set_is_data(d);
        none(py)
    }
);
static GET_REFERENCE_SPACE_TYPE: Function = method_def!(
    "getReferenceSpaceType",
    [SELF],
    REFERENCE_SPACE_TYPE,
    |py, o, a, _| {
        let rc = this_shared!(o, a, ColorSpaceNode);
        let r = rc.lock().cs.reference_space_type();
        enum_out(py, REFERENCE_SPACE_TYPE, r as i64)
    }
);

// .def("getAllocation", ...), .def("setAllocation", ..., "allocation"_a),
// .def("getAllocationVars", ...), .def("setAllocationVars", ..., "vars"_a) (PyColorSpace.cpp:
// 237-255 @ v2.5.2).
static GET_ALLOCATION: Function =
    method_def!("getAllocation", [SELF], ALLOCATION, |py, o, a, _| {
        let rc = this_shared!(o, a, ColorSpaceNode);
        let al = rc.lock().cs.allocation();
        enum_out(py, ALLOCATION, al as i64)
    });
static SET_ALLOCATION: Function = method_def!(
    "setAllocation",
    [SELF, Param::new("allocation", ALLOCATION)],
    "None",
    |py, o, a, _| {
        let rc = this_shared!(o, a, ColorSpaceNode);
        let al = allocation(enum_param!(o, a, 1))?;
        rc.lock().cs.set_allocation(al);
        none(py)
    }
);
static GET_ALLOCATION_VARS: Function =
    method_def!("getAllocationVars", [SELF], "list[float]", |py, o, a, _| {
        let rc = this_shared!(o, a, ColorSpaceNode);
        let vars: Vec<f64> = rc
            .lock()
            .cs
            .allocation_vars()
            .iter()
            .map(|&v| f64::from(v))
            .collect();
        ok(py, vars)
    });
static SET_ALLOCATION_VARS: Function = method_def!(
    "setAllocationVars",
    [SELF, Param::new("vars", FLOATS)],
    "None",
    |py, o, a, convert| {
        let rc = this_shared!(o, a, ColorSpaceNode);
        let vars = crate::arg!(a, 1, convert, Vec<f32>);
        if vars.len() < 2 || vars.len() > 3 {
            return Err(vars_size(py));
        }
        rc.lock().cs.set_allocation_vars(&vars);
        none(py)
    }
);

// .def("getTransform", &ColorSpace::getTransform, "direction"_a), .def("setTransform",
// &ColorSpace::setTransform, "transform"_a, "direction"_a) (PyColorSpace.cpp:258-261 @
// v2.5.2).
static GET_TRANSFORM: Function = method_def!(
    "getTransform",
    [SELF, Param::new("direction", COLOR_SPACE_DIRECTION)],
    TRANSFORM,
    |py, o, a, _| {
        let rc = this_shared!(o, a, ColorSpaceNode);
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
        Param::new("direction", COLOR_SPACE_DIRECTION)
    ],
    "None",
    |py, o, a, convert| {
        let rc = this_shared!(o, a, ColorSpaceNode);
        let t = transform_param!(o, a, 1, convert);
        let dir = enum_param!(o, a, 2);
        rc.lock().set_transform(py, t.as_ref(), dir)?;
        none(py)
    }
);

// defRepr(clsColorSpace) (PyColorSpace.cpp:263 @ v2.5.2).
static REPR: Function = method_def!("__repr__", [SELF], "str", |py, o, a, _| {
    let rc = this_shared!(o, a, ColorSpaceNode);
    let text = rc.lock().materialize()?.to_bytes();
    string_out(py, &text)
});

// ---------------------------------------------------------------------------------------------
// The iterators (PyColorSpace.cpp:265-302 @ v2.5.2)

/// The color space an iterator of `self` walks, or the next overload.
macro_rules! iter_rc {
    ($o:expr, $a:expr) => {{
        let it = this_iter!($o, $a);
        let rc = it.get().obj::<ColorSpaceRc>()?.clone();
        (it, rc)
    }};
}

fn num_categories(rc: &ColorSpaceRc) -> i32 {
    rc.lock().cs.num_categories()
}

fn num_aliases(rc: &ColorSpaceRc) -> i32 {
    rc.lock().cs.num_aliases() as i32
}

/// `getCategory(i)`: `None` outside the categories (a null pointer).
fn category(py: Python<'_>, rc: &ColorSpaceRc, i: i32) -> Out {
    let c = rc.lock().cs.category(i).map(<[u8]>::to_vec);
    Ok(Some(
        crate::dispatch::char_ptr_to_py(py, c.as_deref())?.unbind(),
    ))
}

/// `getAlias(i)`: the `int` made a `size_t`, `""` outside the aliases.
fn alias(py: Python<'_>, rc: &ColorSpaceRc, i: i32) -> Out {
    let a = rc.lock().cs.alias(i as isize as usize).to_vec();
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

/// Port of `bindPyColorSpace` (src/bindings/python/PyColorSpace.cpp:52-304 @ v2.5.2), in its
/// order: the iterator classes nested in the class (58-66), the class's methods (68-261),
/// `__repr__` (263), the iterators' (265-302).
pub(crate) fn bind_py_color_space(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let cls = class(py, COLOR_SPACE)?;
    let categories = nested_class(&cls, "ColorSpace", "ColorSpaceCategoryIterator")?;
    let aliases = nested_class(&cls, "ColorSpace", "ColorSpaceAliasIterator")?;
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
        &GET_ENCODING,
        &SET_ENCODING,
        &GET_EQUALITY_GROUP,
        &SET_EQUALITY_GROUP,
        &GET_DESCRIPTION,
        &SET_DESCRIPTION,
        &GET_INTEROP_ID,
        &SET_INTEROP_ID,
        &GET_INTERCHANGE_ATTRIBUTE,
        &SET_INTERCHANGE_ATTRIBUTE,
        &GET_INTERCHANGE_ATTRIBUTES,
        &GET_BIT_DEPTH,
        &SET_BIT_DEPTH,
        &HAS_CATEGORY,
        &ADD_CATEGORY,
        &REMOVE_CATEGORY,
        &GET_CATEGORIES,
        &CLEAR_CATEGORIES,
        &IS_DATA,
        &SET_IS_DATA,
        &GET_REFERENCE_SPACE_TYPE,
        &GET_ALLOCATION,
        &SET_ALLOCATION,
        &GET_ALLOCATION_VARS,
        &SET_ALLOCATION_VARS,
        &GET_TRANSFORM,
        &SET_TRANSFORM,
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
