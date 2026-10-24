// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyColorSpaceMenuHelpers` (src/bindings/python/apphelpers/PyColorSpaceHelpers.cpp @
//! v2.5.2): the submodule `ColorSpaceHelpers`, the classes `ColorSpaceMenuParameters` and
//! `ColorSpaceMenuHelper`, and their iterator classes (chunk 6.7c).
//!
//! **The config they hold.** Upstream's parameters hold a `ConstConfigRcPtr`: `getConfig`
//! returns the Python object of the config given (the same one while it lives), and a change
//! Python makes to that config afterwards is the parameters' (their `repr` writes its cache ID
//! now, a menu made from them reads it now). A menu holds a copy of its parameters, which
//! shares the config of the parameters that made it. The binding holds the config's node
//! ([`ConfigRc`]) beside the library's object, and hands the library the config as it is when
//! it reads it ([`config_arc`]).

use std::sync::{Arc, Mutex, PoisonError, Weak};

use ocio::apphelpers::color_space_helpers::{
    ColorSpaceMenuHelper, ColorSpaceMenuParameters, add_color_space,
};
use pyo3::prelude::*;

use crate::dispatch::{CharPtr, Function, NonNullCharPtr, Overload, Param, StdString};
use crate::py_config::{
    CONFIG, ConfigRc, cache_id, config_arc, config_arc_with_cache_id, config_arg, search_reference,
    with_cache_id, with_config,
};
use crate::py_open_color_io::{function, py_result};
use crate::py_transform::{
    INT, Out, class, def, default, enum_out, nested_class, none, ok, string_out,
};
use crate::py_utils::{check_index, new_iterator};
use crate::shared::{Shared, construct, out};
use crate::transforms::py_color_space_transform::{bool_param, empty_string};
use crate::{enum_param, method_def, q, this_iter, this_shared};

pub(crate) const PARAMETERS: &str = q!("ColorSpaceMenuParameters");
const ADDED_ITERATOR: &str = q!("ColorSpaceMenuParameters.AddedColorSpaceIterator");
const HELPER: &str = q!("ColorSpaceMenuHelper");
const LEVEL_ITERATOR: &str = q!("ColorSpaceMenuHelper.ColorSpaceLevelIterator");
const SEARCH_REFERENCE_SPACE_TYPE: &str = q!("SearchReferenceSpaceType");
const SELF: Param = Param::new("self", PARAMETERS);
const HELPER_SELF: Param = Param::new("self", HELPER);

// ---------------------------------------------------------------------------------------------
// The parameters

/// Parameters as the binding holds them: the library's, and the node of their config, which
/// the library's get as it is each time they are read ([`ParamsNode::current`]).
#[derive(Debug)]
pub(crate) struct ParamsNode {
    params: ColorSpaceMenuParameters,
    config: ConfigRc,
}

type ParamsRc = Shared<ParamsNode>;

/// `f` of the library's parameters of `rc`, their config the node's as it is now (read
/// before the parameters are locked: the config's lock can wait for another thread's call);
/// `reads_cache_id`: `f` reads the config's cache ID ([`config_arc_with_cache_id`]).
fn with_current<R>(
    rc: &ParamsRc,
    reads_cache_id: bool,
    f: impl FnOnce(&ColorSpaceMenuParameters, &ConfigRc) -> PyResult<R>,
) -> PyResult<R> {
    loop {
        let config = rc.lock().config.clone();
        let arc = if reads_cache_id {
            config_arc_with_cache_id(&config)?
        } else {
            config_arc(&config)?
        };
        let mut node = rc.lock();
        // Another thread may have set another config meanwhile.
        if node.config.same(&config) {
            node.params.set_config(arc);
            let node = &*node;
            return f(&node.params, &node.config);
        }
    }
}

/// Port of `ColorSpaceMenuParameters::Create` (ColorSpaceHelpers.cpp:290-293 @ v2.5.2) for the
/// config `rc`.
fn create(rc: ConfigRc) -> PyResult<ParamsNode> {
    let config = config_arc(&rc)?;
    Ok(ParamsNode {
        params: ColorSpaceMenuParameters::new(config),
        config: rc,
    })
}

/// A `ConstConfigRcPtr` parameter declared `.none(false)`: an instance, or the next overload.
macro_rules! config_param {
    ($a:expr, $i:expr, $convert:expr) => {
        match config_arg(&$a[$i], $convert)? {
            Some(Some(rc)) => rc,
            _ => return Ok(None),
        }
    };
}

// .def(py::init(&ColorSpaceMenuParameters::Create), "config"_a.none(false)), .def(py::init([](
// config, role, includeColorSpaces, searchReferenceSpaceType, includeNamedTransforms,
// treatNoCategoryAsAny, appCategories, encodings, userCategories, includeRoles) {...}),
// "config"_a.none(false), "role"_a.none(false) = "", "includeColorSpaces"_a = true,
// "searchReferenceSpaceType"_a = SEARCH_REFERENCE_SPACE_ALL, "includeNamedTransforms"_a = false,
// "treatNoCategoryAsAny"_a = true, "appCategories"_a.none(false) = "", "encodings"_a.none(false)
// = "", "userCategories"_a.none(false) = "", "includeRoles"_a = false)
// (PyColorSpaceHelpers.cpp:61-104 @ v2.5.2).
static INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[SELF, Param::new("config", CONFIG)],
            ret: "None",
            call: |py, _, a, convert| {
                let rc = config_param!(a, 1, convert);
                construct(&a[0], PARAMETERS, Shared::new(create(rc)?))?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param::new("config", CONFIG),
                empty_string("role"),
                bool_param("includeColorSpaces", true),
                Param {
                    name: "searchReferenceSpaceType",
                    ty: SEARCH_REFERENCE_SPACE_TYPE,
                    default: default(
                        "<SearchReferenceSpaceType.SEARCH_REFERENCE_SPACE_ALL: 2>",
                        search_all,
                    ),
                },
                bool_param("includeNamedTransforms", false),
                bool_param("treatNoCategoryAsAny", true),
                empty_string("appCategories"),
                empty_string("encodings"),
                empty_string("userCategories"),
                bool_param("includeRoles", false),
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let rc = config_param!(a, 1, convert);
                let StdString(role) = crate::arg!(a, 2, convert, StdString);
                let include_color_spaces = crate::arg!(a, 3, convert, bool);
                let search = enum_param!(o, a, 4);
                let include_named_transforms = crate::arg!(a, 5, convert, bool);
                let treat_no_category_as_any = crate::arg!(a, 6, convert, bool);
                let StdString(app_categories) = crate::arg!(a, 7, convert, StdString);
                let StdString(encodings) = crate::arg!(a, 8, convert, StdString);
                let StdString(user_categories) = crate::arg!(a, 9, convert, StdString);
                let include_roles = crate::arg!(a, 10, convert, bool);
                let mut node = create(rc)?;
                let p = &mut node.params;
                if !role.is_empty() {
                    p.set_role(&role);
                }
                if !app_categories.is_empty() {
                    p.set_app_categories(&app_categories);
                }
                if !user_categories.is_empty() {
                    p.set_user_categories(&user_categories);
                }
                if !encodings.is_empty() {
                    p.set_encodings(&encodings);
                }
                // A value outside the enumerators: U-112.
                p.set_search_reference_space_type(search_reference(search)?);
                p.set_include_color_spaces(include_color_spaces);
                p.set_include_roles(include_roles);
                p.set_include_named_transforms(include_named_transforms);
                p.set_treat_no_category_as_any(treat_no_category_as_any);
                construct(&a[0], PARAMETERS, Shared::new(node))?;
                none(py)
            },
            pos_only: 0,
        },
    ],
};

fn search_all(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    crate::enums::to_py(
        &crate::enums::enum_class(py, SEARCH_REFERENCE_SPACE_TYPE)?,
        2,
    )
}

// .def("getConfig", &ColorSpaceMenuParameters::getConfig) (PyColorSpaceHelpers.cpp:106-107 @
// v2.5.2): the config given, the same Python object while it lives.
static GET_CONFIG: Function = method_def!("getConfig", [SELF], CONFIG, |py, o, a, _| {
    let rc = this_shared!(o, a, ParamsNode);
    let config = rc.lock().config.clone();
    out(py, CONFIG, &config)
});

// .def("setConfig", &ColorSpaceMenuParameters::setConfig, "config"_a.none(false))
// (PyColorSpaceHelpers.cpp:108-109 @ v2.5.2).
static SET_CONFIG: Function = method_def!(
    "setConfig",
    [SELF, Param::new("config", CONFIG)],
    "None",
    |py, o, a, convert| {
        let rc = this_shared!(o, a, ParamsNode);
        let config = config_param!(a, 1, convert);
        rc.lock().config = config;
        none(py)
    }
);

/// A getter of one `const char *` of the parameters.
macro_rules! text_getter {
    ($name:literal, $get:ident) => {
        method_def!($name, [SELF], "str", |py, o, a, _| {
            let rc = this_shared!(o, a, ParamsNode);
            let text = rc.lock().params.$get().to_vec();
            string_out(py, &text)
        })
    };
}

/// A setter of one `const char *` of the parameters, declared `.none(false)`.
macro_rules! text_setter {
    ($name:literal, $param:literal, $set:ident) => {
        method_def!(
            $name,
            [SELF, Param::new($param, "str")],
            "None",
            |py, o, a, convert| {
                let rc = this_shared!(o, a, ParamsNode);
                let NonNullCharPtr(v) = crate::arg!(a, 1, convert, NonNullCharPtr);
                rc.lock().params.$set(&v);
                none(py)
            }
        )
    };
}

/// A getter of one `bool` of the parameters.
macro_rules! bool_getter {
    ($name:literal, $get:ident) => {
        method_def!($name, [SELF], "bool", |py, o, a, _| {
            let rc = this_shared!(o, a, ParamsNode);
            let v = rc.lock().params.$get();
            ok(py, v)
        })
    };
}

/// A setter of one `bool` of the parameters, whose argument defaults to `true`.
macro_rules! bool_setter {
    ($name:literal, $param:literal, $set:ident) => {
        method_def!(
            $name,
            [SELF, bool_param($param, true)],
            "None",
            |py, o, a, convert| {
                let rc = this_shared!(o, a, ParamsNode);
                let v = crate::arg!(a, 1, convert, bool);
                rc.lock().params.$set(v);
                none(py)
            }
        )
    };
}

// .def("getRole", ...), .def("setRole", ..., "role"_a.none(false)) (PyColorSpaceHelpers.cpp:
// 110-113 @ v2.5.2).
static GET_ROLE: Function = text_getter!("getRole", role);
static SET_ROLE: Function = text_setter!("setRole", "role", set_role);
// .def("getIncludeColorSpaces", ...), .def("setIncludeColorSpaces", ...,
// "includeColorSpaces"_a = true) (PyColorSpaceHelpers.cpp:114-118 @ v2.5.2).
static GET_INCLUDE_COLOR_SPACES: Function =
    bool_getter!("getIncludeColorSpaces", include_color_spaces);
static SET_INCLUDE_COLOR_SPACES: Function = bool_setter!(
    "setIncludeColorSpaces",
    "includeColorSpaces",
    set_include_color_spaces
);

// .def("getSearchReferenceSpaceType", ...) (PyColorSpaceHelpers.cpp:119-120 @ v2.5.2).
static GET_SEARCH_REFERENCE_SPACE_TYPE: Function = method_def!(
    "getSearchReferenceSpaceType",
    [SELF],
    SEARCH_REFERENCE_SPACE_TYPE,
    |py, o, a, _| {
        let rc = this_shared!(o, a, ParamsNode);
        let v = rc.lock().params.search_reference_space_type() as i64;
        enum_out(py, SEARCH_REFERENCE_SPACE_TYPE, v)
    }
);

// .def("setSearchReferenceSpaceType", ..., "searchReferenceSpaceType"_a.none(false))
// (PyColorSpaceHelpers.cpp:121-123 @ v2.5.2): a value outside the enumerators is U-112.
static SET_SEARCH_REFERENCE_SPACE_TYPE: Function = method_def!(
    "setSearchReferenceSpaceType",
    [
        SELF,
        Param::new("searchReferenceSpaceType", SEARCH_REFERENCE_SPACE_TYPE)
    ],
    "None",
    |py, o, a, _| {
        let rc = this_shared!(o, a, ParamsNode);
        let v = search_reference(enum_param!(o, a, 1))?;
        rc.lock().params.set_search_reference_space_type(v);
        none(py)
    }
);

// .def("getIncludeNamedTransforms", ...), .def("setIncludeNamedTransforms", ...,
// "includeNamedTransforms"_a = true) (PyColorSpaceHelpers.cpp:124-128 @ v2.5.2).
static GET_INCLUDE_NAMED_TRANSFORMS: Function =
    bool_getter!("getIncludeNamedTransforms", include_named_transforms);
static SET_INCLUDE_NAMED_TRANSFORMS: Function = bool_setter!(
    "setIncludeNamedTransforms",
    "includeNamedTransforms",
    set_include_named_transforms
);
// .def("getTreatNoCategoryAsAny", ...), .def("setTreatNoCategoryAsAny", ...,
// "treatNoCategoryAsAny"_a = true) (PyColorSpaceHelpers.cpp:129-133 @ v2.5.2).
static GET_TREAT_NO_CATEGORY_AS_ANY: Function =
    bool_getter!("getTreatNoCategoryAsAny", treat_no_category_as_any);
static SET_TREAT_NO_CATEGORY_AS_ANY: Function = bool_setter!(
    "setTreatNoCategoryAsAny",
    "treatNoCategoryAsAny",
    set_treat_no_category_as_any
);
// .def("getEncodings", ...), .def("setEncodings", ..., "encodings"_a.none(false)),
// .def("getAppCategories", ...), .def("setAppCategories", ..., "appCategories"_a.none(false)),
// .def("getUserCategories", ...), .def("setUserCategories", ..., "categories"_a.none(false))
// (PyColorSpaceHelpers.cpp:134-148 @ v2.5.2).
static GET_ENCODINGS: Function = text_getter!("getEncodings", encodings);
static SET_ENCODINGS: Function = text_setter!("setEncodings", "encodings", set_encodings);
static GET_APP_CATEGORIES: Function = text_getter!("getAppCategories", app_categories);
static SET_APP_CATEGORIES: Function =
    text_setter!("setAppCategories", "appCategories", set_app_categories);
static GET_USER_CATEGORIES: Function = text_getter!("getUserCategories", user_categories);
static SET_USER_CATEGORIES: Function =
    text_setter!("setUserCategories", "categories", set_user_categories);
// .def("getIncludeRoles", ...), .def("setIncludeRoles", ..., "includeRoles"_a = true)
// (PyColorSpaceHelpers.cpp:149-153 @ v2.5.2).
static GET_INCLUDE_ROLES: Function = bool_getter!("getIncludeRoles", include_roles);
static SET_INCLUDE_ROLES: Function =
    bool_setter!("setIncludeRoles", "includeRoles", set_include_roles);
// .def("addColorSpace", ..., "colorSpace"_a.none(false)) (PyColorSpaceHelpers.cpp:154-156 @
// v2.5.2).
static ADD_COLOR_SPACE: Function = text_setter!("addColorSpace", "colorSpace", add_color_space);

// .def("getAddedColorSpaces", [](ColorSpaceMenuParametersRcPtr & self) { return
// AddedColorSpaceIterator(self); }) (PyColorSpaceHelpers.cpp:157-160 @ v2.5.2).
static GET_ADDED_COLOR_SPACES: Function = method_def!(
    "getAddedColorSpaces",
    [SELF],
    ADDED_ITERATOR,
    |py, o, a, _| {
        let rc = this_shared!(o, a, ParamsNode);
        Ok(Some(
            new_iterator(py, ADDED_ITERATOR, Some(Box::new(rc)))?.unbind(),
        ))
    }
);

// .def("clearAddedColorSpaces", ...) (PyColorSpaceHelpers.cpp:161-162 @ v2.5.2).
static CLEAR_ADDED_COLOR_SPACES: Function =
    method_def!("clearAddedColorSpaces", [SELF], "None", |py, o, a, _| {
        let rc = this_shared!(o, a, ParamsNode);
        rc.lock().params.clear_added_color_spaces();
        none(py)
    });

// defRepr(clsColorSpaceMenuParameters) (PyColorSpaceHelpers.cpp:164 @ v2.5.2): the config's
// cache ID as it is now.
static REPR: Function = method_def!("__repr__", [SELF], "str", |py, o, a, _| {
    let rc = this_shared!(o, a, ParamsNode);
    let config = rc.lock().config.clone();
    let id = py_result(py, cache_id(&config)?)?;
    let (text, old_len) = with_current(&rc, false, |p, _| {
        Ok((
            py_result(py, p.to_bytes())?,
            py_result(py, p.config().cache_id())?.len(),
        ))
    })?;
    string_out(py, &with_cache_id(&text, old_len, &id))
});

// The iterator (PyColorSpaceHelpers.cpp:166-185 @ v2.5.2).

const ADDED_SELF: Param = Param::new("self", ADDED_ITERATOR);

/// The parameters an iterator of `self` walks, or the next overload.
macro_rules! iter_params {
    ($o:expr, $a:expr) => {{
        let it = this_iter!($o, $a);
        let rc = it.get().obj::<ParamsRc>()?.clone();
        (it, rc)
    }};
}

/// `(int)it.m_obj->getNumAddedColorSpaces()`.
fn num_added(rc: &ParamsRc) -> i32 {
    rc.lock().params.num_added_color_spaces() as i32
}

/// `getAddedColorSpace(i)`, the `int` converted to `size_t` (a negative index wraps, past the
/// last: "").
fn added(py: Python<'_>, rc: &ParamsRc, i: i32) -> Out {
    let text = rc.lock().params.added_color_space(i as usize).to_vec();
    string_out(py, &text)
}

// .def("__len__", [](AddedColorSpaceIterator & it) { return
// it.m_obj->getNumAddedColorSpaces(); }) (PyColorSpaceHelpers.cpp:167-170 @ v2.5.2).
static ADDED_LEN: Function = method_def!("__len__", [ADDED_SELF], "int", |py, o, a, _| {
    let (_, rc) = iter_params!(o, a);
    let n = rc.lock().params.num_added_color_spaces();
    ok(py, n)
});

// .def("__getitem__", [](AddedColorSpaceIterator & it, int i) {...})
// (PyColorSpaceHelpers.cpp:171-175 @ v2.5.2).
static ADDED_GETITEM: Function = method_def!(
    "__getitem__",
    [ADDED_SELF, Param::new("arg0", INT)],
    "str",
    |py, o, a, convert| {
        let (_, rc) = iter_params!(o, a);
        let i = crate::arg!(a, 1, convert, i32);
        check_index(i, num_added(&rc))?;
        added(py, &rc, i)
    }
);

// .def("__iter__", [](AddedColorSpaceIterator & it) -> AddedColorSpaceIterator & { return it;
// }) (PyColorSpaceHelpers.cpp:176-179 @ v2.5.2).
static ADDED_ITER: Function =
    method_def!("__iter__", [ADDED_SELF], ADDED_ITERATOR, |_, o, a, _| {
        let _ = this_iter!(o, a);
        Ok(Some(a[0].clone().unbind()))
    });

// .def("__next__", [](AddedColorSpaceIterator & it) {...}) (PyColorSpaceHelpers.cpp:180-184 @
// v2.5.2).
static ADDED_NEXT: Function = method_def!("__next__", [ADDED_SELF], "str", |py, o, a, _| {
    let (it, rc) = iter_params!(o, a);
    let i = it.get().next_index(num_added(&rc))?;
    added(py, &rc, i)
});

// ---------------------------------------------------------------------------------------------
// The menu

/// A menu as the binding holds it: the library's, and the node of the config of the
/// parameters that made it (upstream's menu holds a copy of those parameters, sharing their
/// config).
#[derive(Debug)]
pub(crate) struct HelperNode {
    helper: Arc<ColorSpaceMenuHelper>,
    /// The config of the parameters that made the library's menu, and the length of its
    /// cache ID then.
    config: ConfigRc,
    id_len: usize,
}

type HelperRc = Shared<HelperNode>;

/// The library's menus the binding made, with the config of the parameters that made each
/// first: the library keeps a menu for the life of the process (by its parameters' text,
/// unless `OCIO_DISABLE_ALL_CACHES`), and gives it again for parameters of the same text,
/// whose config upstream's menu keeps sharing (its `repr` then writes that config's cache ID
/// as it is now, whichever parameters asked for the menu since).
static MENUS: Mutex<Vec<(Weak<ColorSpaceMenuHelper>, ConfigRc, usize)>> = Mutex::new(Vec::new());

/// Port of `ColorSpaceMenuHelper::Create` (ColorSpaceHelpers.cpp:378-450 @ v2.5.2) of the
/// parameters `rc`: the library's menu of the parameters, their config as it is now, and the
/// config it shares.
fn create_helper(py: Python<'_>, rc: &ParamsRc) -> PyResult<HelperNode> {
    let (helper, config, id_len) = with_current(rc, true, |params, config| {
        let id_len = py_result(py, params.config().cache_id())?.len();
        let helper = py_result(py, ColorSpaceMenuHelper::new(params))?;
        Ok((helper, config.clone(), id_len))
    })?;
    let mut menus = MENUS.lock().unwrap_or_else(PoisonError::into_inner);
    menus.retain(|(menu, _, _)| menu.strong_count() > 0);
    let (config, id_len) = match menus
        .iter()
        .find(|(menu, _, _)| std::ptr::eq(menu.as_ptr(), Arc::as_ptr(&helper)))
    {
        Some((_, config, id_len)) => (config.clone(), *id_len),
        None => {
            menus.push((Arc::downgrade(&helper), config.clone(), id_len));
            (config, id_len)
        }
    };
    Ok(HelperNode {
        helper,
        config,
        id_len,
    })
}

/// A `ConstColorSpaceMenuParametersRcPtr` parameter declared `.none(false)`: an instance, or
/// the next overload.
fn params_arg(obj: &Bound<'_, PyAny>, convert: bool) -> PyResult<Option<ParamsRc>> {
    Ok(
        match crate::shared::holder_arg::<ParamsNode>(obj, PARAMETERS, convert)? {
            Some(Some(rc)) => Some(rc),
            _ => None,
        },
    )
}

// .def(py::init(&ColorSpaceMenuHelper::Create), "parameters"_a.none(false))
// (PyColorSpaceHelpers.cpp:187-188 @ v2.5.2).
static HELPER_INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    operator: false,
    overloads: &[Overload {
        params: &[HELPER_SELF, Param::new("parameters", PARAMETERS)],
        ret: "None",
        call: |py, _, a, convert| {
            let Some(params) = params_arg(&a[1], convert)? else {
                return Ok(None);
            };
            let node = create_helper(py, &params)?;
            construct(&a[0], HELPER, Shared::new(node))?;
            none(py)
        },
        pos_only: 0,
    }],
};

// .def("getNumColorSpaces", &ColorSpaceMenuHelper::getNumColorSpaces)
// (PyColorSpaceHelpers.cpp:189-190 @ v2.5.2).
static GET_NUM_COLOR_SPACES: Function =
    method_def!("getNumColorSpaces", [HELPER_SELF], "int", |py, o, a, _| {
        let rc = this_shared!(o, a, HelperNode);
        let n = rc.lock().helper.num_color_spaces();
        ok(py, n)
    });

/// A getter of the text of a menu item, `(index: int) -> str` (`size_t idx`).
macro_rules! item_getter {
    ($name:literal, $get:ident) => {
        method_def!(
            $name,
            [HELPER_SELF, Param::new("index", INT)],
            "str",
            |py, o, a, convert| {
                let rc = this_shared!(o, a, HelperNode);
                let idx = crate::arg!(a, 1, convert, u64);
                let text = rc.lock().helper.$get(idx as usize).to_vec();
                string_out(py, &text)
            }
        )
    };
}

/// A lookup of a menu item by a name, `(name: str)`, declared `.none(false)`.
macro_rules! by_name {
    ($name:literal, $ret:literal, $get:ident, $out:expr) => {
        method_def!(
            $name,
            [HELPER_SELF, Param::new("name", "str")],
            $ret,
            |py, o, a, convert| {
                let rc = this_shared!(o, a, HelperNode);
                let NonNullCharPtr(name) = crate::arg!(a, 1, convert, NonNullCharPtr);
                let node = rc.lock();
                $out(py, node.helper.$get(&name))
            }
        )
    };
}

fn index_out(py: Python<'_>, i: usize) -> Out {
    ok(py, i)
}

fn text_out(py: Python<'_>, text: &[u8]) -> Out {
    string_out(py, text)
}

// .def("getName", ..., "index"_a), .def("getUIName", ..., "index"_a)
// (PyColorSpaceHelpers.cpp:191-194 @ v2.5.2).
static GET_NAME: Function = item_getter!("getName", name);
static GET_UI_NAME: Function = item_getter!("getUIName", ui_name);
// .def("getIndexFromName", ..., "name"_a.none(false)), .def("getIndexFromUIName", ...,
// "name"_a.none(false)) (PyColorSpaceHelpers.cpp:195-198 @ v2.5.2).
static GET_INDEX_FROM_NAME: Function =
    by_name!("getIndexFromName", "int", index_from_name, index_out);
static GET_INDEX_FROM_UI_NAME: Function =
    by_name!("getIndexFromUIName", "int", index_from_ui_name, index_out);
// .def("getDescription", ..., "index"_a), .def("getFamily", ..., "index"_a)
// (PyColorSpaceHelpers.cpp:199-202 @ v2.5.2).
static GET_DESCRIPTION: Function = item_getter!("getDescription", description);
static GET_FAMILY: Function = item_getter!("getFamily", family);

// .def("getHierarchyLevels", [](ColorSpaceMenuHelperRcPtr & self, size_t index) { return
// ColorSpaceLevelIterator(self, index); }, "index"_a) (PyColorSpaceHelpers.cpp:203-208 @
// v2.5.2).
static GET_HIERARCHY_LEVELS: Function = method_def!(
    "getHierarchyLevels",
    [HELPER_SELF, Param::new("index", INT)],
    LEVEL_ITERATOR,
    |py, o, a, convert| {
        let rc = this_shared!(o, a, HelperNode);
        let index = crate::arg!(a, 1, convert, u64) as usize;
        Ok(Some(
            new_iterator(py, LEVEL_ITERATOR, Some(Box::new((rc, index))))?.unbind(),
        ))
    }
);

// .def("getNameFromUIName", ..., "name"_a.none(false)), .def("getUINameFromName", ...,
// "name"_a.none(false)) (PyColorSpaceHelpers.cpp:209-212 @ v2.5.2).
static GET_NAME_FROM_UI_NAME: Function =
    by_name!("getNameFromUIName", "str", name_from_ui_name, text_out);
static GET_UI_NAME_FROM_NAME: Function =
    by_name!("getUINameFromName", "str", ui_name_from_name, text_out);

// defRepr(clsColorSpaceMenuHelper) (PyColorSpaceHelpers.cpp:214 @ v2.5.2): the text of the
// menu's parameters, with the cache ID of their config as it is now.
static HELPER_REPR: Function = method_def!("__repr__", [HELPER_SELF], "str", |py, o, a, _| {
    let rc = this_shared!(o, a, HelperNode);
    let (helper, config, id_len) = {
        let node = rc.lock();
        (node.helper.clone(), node.config.clone(), node.id_len)
    };
    let id = py_result(py, cache_id(&config)?)?;
    // The library's text starts with the cache ID of the config as it was when the menu was
    // made.
    let text = py_result(py, helper.to_bytes())?;
    string_out(py, &with_cache_id(&text, id_len, &id))
});

// The iterator (PyColorSpaceHelpers.cpp:216-235 @ v2.5.2): `std::get<0>(it.m_args)` is the
// index the iterator was made with.

const LEVEL_SELF: Param = Param::new("self", LEVEL_ITERATOR);

/// The menu and the item index an iterator of `self` walks, or the next overload.
macro_rules! iter_levels {
    ($o:expr, $a:expr) => {{
        let it = this_iter!($o, $a);
        let (rc, index) = it.get().obj::<(HelperRc, usize)>()?.clone();
        (it, rc, index)
    }};
}

/// `static_cast<int>(it.m_obj->getNumHierarchyLevels(index))`.
fn num_levels(rc: &HelperRc, index: usize) -> i32 {
    rc.lock().helper.num_hierarchy_levels(index) as i32
}

/// `getHierarchyLevel(index, static_cast<size_t>(i))`.
fn level(py: Python<'_>, rc: &HelperRc, index: usize, i: i32) -> Out {
    let text = rc.lock().helper.hierarchy_level(index, i as usize).to_vec();
    string_out(py, &text)
}

// .def("__len__", [](ColorSpaceLevelIterator & it) { return
// it.m_obj->getNumHierarchyLevels(std::get<0>(it.m_args)); }) (PyColorSpaceHelpers.cpp:217-220
// @ v2.5.2).
static LEVEL_LEN: Function = method_def!("__len__", [LEVEL_SELF], "int", |py, o, a, _| {
    let (_, rc, index) = iter_levels!(o, a);
    let n = rc.lock().helper.num_hierarchy_levels(index);
    ok(py, n)
});

// .def("__getitem__", [](ColorSpaceLevelIterator & it, int i) {...})
// (PyColorSpaceHelpers.cpp:221-225 @ v2.5.2).
static LEVEL_GETITEM: Function = method_def!(
    "__getitem__",
    [LEVEL_SELF, Param::new("arg0", INT)],
    "str",
    |py, o, a, convert| {
        let (_, rc, index) = iter_levels!(o, a);
        let i = crate::arg!(a, 1, convert, i32);
        check_index(i, num_levels(&rc, index))?;
        level(py, &rc, index, i)
    }
);

// .def("__iter__", ...) (PyColorSpaceHelpers.cpp:226-229 @ v2.5.2).
static LEVEL_ITER: Function =
    method_def!("__iter__", [LEVEL_SELF], LEVEL_ITERATOR, |_, o, a, _| {
        let _ = this_iter!(o, a);
        Ok(Some(a[0].clone().unbind()))
    });

// .def("__next__", ...) (PyColorSpaceHelpers.cpp:230-234 @ v2.5.2).
static LEVEL_NEXT: Function = method_def!("__next__", [LEVEL_SELF], "str", |py, o, a, _| {
    let (it, rc, index) = iter_levels!(o, a);
    let i = it.get().next_index(num_levels(&rc, index))?;
    level(py, &rc, index, i)
});

// ---------------------------------------------------------------------------------------------
// The submodule

// .def("AddColorSpace", &ColorSpaceHelpers::AddColorSpace, "config"_a.none(false),
// "name"_a.none(false), "transformFilePath"_a.none(false), "categories"_a = "",
// "connectionColorSpaceName"_a.none(false)) (PyColorSpaceHelpers.cpp:34-40 @ v2.5.2):
// `categories` may be `None`, a null pointer upstream reads as empty
// (ColorSpaceHelpers.cpp:812 @ v2.5.2).
static ADD_COLOR_SPACE_FN: Function = function(
    "AddColorSpace",
    &[Overload {
        params: &[
            Param::new("config", CONFIG),
            Param::new("name", "str"),
            Param::new("transformFilePath", "str"),
            Param {
                name: "categories",
                ty: "str",
                default: default("''", |py| Ok(pyo3::types::PyString::new(py, "").into_any())),
            },
            Param::new("connectionColorSpaceName", "str"),
        ],
        ret: "None",
        call: |py, _, a, convert| {
            let rc = config_param!(a, 0, convert);
            let NonNullCharPtr(name) = crate::arg!(a, 1, convert, NonNullCharPtr);
            let NonNullCharPtr(path) = crate::arg!(a, 2, convert, NonNullCharPtr);
            let categories = crate::arg!(a, 3, convert, CharPtr).0.unwrap_or_default();
            let NonNullCharPtr(connection) = crate::arg!(a, 4, convert, NonNullCharPtr);
            with_config(&rc, |n| {
                py_result(
                    py,
                    add_color_space(&mut n.config, &name, &path, &categories, &connection),
                )
            })?;
            none(py)
        },
        pos_only: 0,
    }],
);

/// Port of `bindPyColorSpaceMenuHelpers` (src/bindings/python/apphelpers/
/// PyColorSpaceHelpers.cpp:31-236 @ v2.5.2), in its order.
pub(crate) fn bind_py_color_space_menu_helpers(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let sub = crate::pybind11::def_submodule(m, "ColorSpaceHelpers")?;
    crate::pybind11::def_submodule_function(&sub, &ADD_COLOR_SPACE_FN)?;

    let params = class(py, PARAMETERS)?;
    let added = nested_class(
        &params,
        "ColorSpaceMenuParameters",
        "AddedColorSpaceIterator",
    )?;
    let helper = class(py, HELPER)?;
    let levels = nested_class(&helper, "ColorSpaceMenuHelper", "ColorSpaceLevelIterator")?;

    for f in [
        &INIT,
        &GET_CONFIG,
        &SET_CONFIG,
        &GET_ROLE,
        &SET_ROLE,
        &GET_INCLUDE_COLOR_SPACES,
        &SET_INCLUDE_COLOR_SPACES,
        &GET_SEARCH_REFERENCE_SPACE_TYPE,
        &SET_SEARCH_REFERENCE_SPACE_TYPE,
        &GET_INCLUDE_NAMED_TRANSFORMS,
        &SET_INCLUDE_NAMED_TRANSFORMS,
        &GET_TREAT_NO_CATEGORY_AS_ANY,
        &SET_TREAT_NO_CATEGORY_AS_ANY,
        &GET_ENCODINGS,
        &SET_ENCODINGS,
        &GET_APP_CATEGORIES,
        &SET_APP_CATEGORIES,
        &GET_USER_CATEGORIES,
        &SET_USER_CATEGORIES,
        &GET_INCLUDE_ROLES,
        &SET_INCLUDE_ROLES,
        &ADD_COLOR_SPACE,
        &GET_ADDED_COLOR_SPACES,
        &CLEAR_ADDED_COLOR_SPACES,
        &REPR,
    ] {
        def(&params, f)?;
    }
    for f in [&ADDED_LEN, &ADDED_GETITEM, &ADDED_ITER, &ADDED_NEXT] {
        def(&added, f)?;
    }
    for f in [
        &HELPER_INIT,
        &GET_NUM_COLOR_SPACES,
        &GET_NAME,
        &GET_UI_NAME,
        &GET_INDEX_FROM_NAME,
        &GET_INDEX_FROM_UI_NAME,
        &GET_DESCRIPTION,
        &GET_FAMILY,
        &GET_HIERARCHY_LEVELS,
        &GET_NAME_FROM_UI_NAME,
        &GET_UI_NAME_FROM_NAME,
        &HELPER_REPR,
    ] {
        def(&helper, f)?;
    }
    for f in [&LEVEL_LEN, &LEVEL_GETITEM, &LEVEL_ITER, &LEVEL_NEXT] {
        def(&levels, f)?;
    }
    Ok(())
}
