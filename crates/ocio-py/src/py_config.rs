// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyConfig` (src/bindings/python/PyConfig.cpp @ v2.5.2): the class `Config`, its 25
//! iterator classes, and the module functions that end the binding (chunks 6.3f and 6.3g).
//!
//! **The objects a config holds.** pybind11 holds a config by its `ConfigRcPtr`, and the
//! config's getters return its own objects: `getColorSpace`, `getLook`, `getViewTransform`,
//! `getNamedTransform` and their iterators the config's `ColorSpaceRcPtr` and the others (a
//! change made through Python changes the config, and none of what the config computed from it:
//! its cache IDs, processors and name lists stay as they were); `getCurrentContext`,
//! `getFileRules` and `getViewingRules` its context and rules. The library holds its color
//! spaces, looks and transforms by value (owner decision 2026-10-22, Q1 = A): the binding holds
//! a config as a [`ConfigNode`], the library's config and the nodes of the objects Python got
//! from it, which it writes back into the config, as they are
//! (`Config::replace_*_as_is`), before each call ([`ConfigNode::sync`]); a node whose object the
//! config no longer holds (replaced, removed) is left to Python, as upstream's pointer. The
//! context and the rules are the library's own shared handles ([`Held::Config`]).
//!
//! **Calls into a config from a callback.** The binding locks a config while the library
//! works on it. A Python callback the library calls meanwhile (the logging function, an I/O
//! proxy) that uses the same config gets the port's error ([`busy`]): upstream's config isn't
//! locked, and C++ lets the callback read (or change) it while the library works on it.
//!
//! The static methods of `ConfigUtils` (`IdentifyBuiltinColorSpace`,
//! `IdentifyInterchangeSpace`, `GetProcessorToBuiltinColorSpace`,
//! `GetProcessorFromBuiltinColorSpace`, `GetProcessorFromConfigs`) come with chunk 6.8a.

use std::sync::{Arc, Mutex};

use ocio::builtinconfigs::builtin_config_registry::resolve_config_path;
use ocio::{
    ColorSpace, Config, ConfigRules, Context, FileRules, Look, NamedTransform, ViewTransform,
    ViewingRules,
};
use ocio_ops::open_color_types::{
    ColorSpaceVisibility, NamedTransformVisibility, SearchReferenceSpaceType, TransformDirection,
    ViewType,
};
use ocio_ops::utils::string_utils::{c_str, compare};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};

use crate::arg;
use crate::dispatch::{
    CharPtr, Function, NonNullCharPtr, Overload, Param, StdString, std_string_to_py,
};
use crate::py_color_space::{COLOR_SPACE, ColorSpaceNode, ColorSpaceRc, reference_space_type};
use crate::py_context::{CONTEXT, ContextRc, context_arg, environment_mode, null_string};
use crate::py_file_rules::{FILE_RULES, FileRulesRc};
use crate::py_look::{LOOK, LookNode};
use crate::py_named_transform::{
    NAMED_TRANSFORM, NamedTransformNode, NamedTransformRc, get_transform,
};
use crate::py_open_color_io::{function, py_result, to_py_err};
use crate::py_processor::{PROCESSOR, processor_out};
use crate::py_transform::{
    INT, Out, TRANSFORM, TRANSFORM_DIRECTION, class, def, def_static, enum_out, nested_class, none,
    ok, outside_enumerators, string_out,
};
use crate::py_utils::{check_index, new_iterator};
use crate::py_view_transform::{VIEW_TRANSFORM, ViewTransformNode, ViewTransformRc};
use crate::py_viewing_rules::{RulesRc, VIEWING_RULES};
use crate::shared::{Held, Shared, construct, holder_arg, holder_of, out};
use crate::transforms::py_color_space_transform::empty_string;
use crate::{enum_param, method_def, q, static_def, this_iter, this_shared};

pub(crate) const CONFIG: &str = q!("Config");
const SELF: Param = Param::new("self", CONFIG);
const ENVIRONMENT_MODE: &str = q!("EnvironmentMode");
const SEARCH_REFERENCE_SPACE_TYPE: &str = q!("SearchReferenceSpaceType");
const COLOR_SPACE_VISIBILITY: &str = q!("ColorSpaceVisibility");
const NAMED_TRANSFORM_VISIBILITY: &str = q!("NamedTransformVisibility");
const VIEW_TYPE: &str = q!("ViewType");
const REFERENCE_SPACE_TYPE: &str = q!("ReferenceSpaceType");
const PROCESSOR_CACHE_FLAGS: &str = q!("ProcessorCacheFlags");
const COLOR_SPACE_SET: &str = q!("ColorSpaceSet");
/// The proxy's C++ type, as the signatures write it: `bindPyConfig` runs before
/// `bindPyConfigIOProxy` registers the class.
const CONFIG_IO_PROXY: &str = "OpenColorIO_v2_5::ConfigIOProxy";

// ---------------------------------------------------------------------------------------------
// The config as the binding holds it

/// A config as the binding holds it (see the module's documentation).
#[derive(Debug)]
pub(crate) struct ConfigNode {
    /// The library's config, with the objects below as they were last written back.
    pub(crate) config: Config,
    color_spaces: Vec<(Arc<ColorSpace>, ColorSpaceRc)>,
    looks: Vec<(Arc<Look>, Shared<LookNode>)>,
    view_transforms: Vec<(Arc<ViewTransform>, ViewTransformRc)>,
    named_transforms: Vec<(Arc<NamedTransform>, NamedTransformRc)>,
}

/// A config shared by the Python objects that hold it.
pub(crate) type ConfigRc = Shared<ConfigNode>;

/// Writes the nodes of `held` back into the config with `replace` (the object's new pointer,
/// or none when the config no longer holds it: the node is dropped).
fn write_back<T, N>(
    held: &mut Vec<(Arc<T>, Shared<N>)>,
    mut replace: impl FnMut(&Arc<T>, T) -> Option<Arc<T>>,
    materialize: impl Fn(&N) -> PyResult<T>,
) -> PyResult<()> {
    let mut kept = Vec::with_capacity(held.len());
    for (arc, node) in std::mem::take(held) {
        let value = materialize(&node.lock())?;
        if let Some(new) = replace(&arc, value) {
            kept.push((new, node));
        }
    }
    *held = kept;
    Ok(())
}

impl ConfigNode {
    /// The node of a config of its own.
    pub(crate) fn new(config: Config) -> ConfigNode {
        ConfigNode {
            config,
            color_spaces: Vec::new(),
            looks: Vec::new(),
            view_transforms: Vec::new(),
            named_transforms: Vec::new(),
        }
    }

    /// The node of a config the library made: the config itself, else (another holder shares
    /// it) a copy as it is.
    pub(crate) fn from_arc(config: Arc<Config>) -> ConfigNode {
        ConfigNode::new(Arc::try_unwrap(config).unwrap_or_else(|c| c.clone_as_is()))
    }

    /// Writes the objects Python got from the config back into it, as they are now.
    pub(crate) fn sync(&mut self) -> PyResult<()> {
        let config = &mut self.config;
        write_back(
            &mut self.color_spaces,
            |old, new| config.replace_color_space_as_is(old, new),
            ColorSpaceNode::materialize,
        )?;
        write_back(
            &mut self.looks,
            |old, new| config.replace_look_as_is(old, new),
            LookNode::materialize,
        )?;
        write_back(
            &mut self.view_transforms,
            |old, new| config.replace_view_transform_as_is(old, new),
            ViewTransformNode::materialize,
        )?;
        write_back(
            &mut self.named_transforms,
            |old, new| config.replace_named_transform_as_is(old, new),
            NamedTransformNode::materialize,
        )
    }

    /// The node of the config's color space `arc`, made the first time.
    fn color_space(&mut self, arc: Arc<ColorSpace>) -> ColorSpaceRc {
        if let Some((_, node)) = self.color_spaces.iter().find(|(a, _)| Arc::ptr_eq(a, &arc)) {
            return node.clone();
        }
        let node = Shared::new(ColorSpaceNode::from_library(&arc));
        self.color_spaces.push((arc, node.clone()));
        node
    }

    fn look(&mut self, arc: Arc<Look>) -> Shared<LookNode> {
        if let Some((_, node)) = self.looks.iter().find(|(a, _)| Arc::ptr_eq(a, &arc)) {
            return node.clone();
        }
        let node = Shared::new(LookNode::from_library(&arc));
        self.looks.push((arc, node.clone()));
        node
    }

    fn view_transform(&mut self, arc: Arc<ViewTransform>) -> ViewTransformRc {
        if let Some((_, node)) = self
            .view_transforms
            .iter()
            .find(|(a, _)| Arc::ptr_eq(a, &arc))
        {
            return node.clone();
        }
        let node = Shared::new(ViewTransformNode::from_library(&arc));
        self.view_transforms.push((arc, node.clone()));
        node
    }

    fn named_transform(&mut self, arc: Arc<NamedTransform>) -> NamedTransformRc {
        if let Some((_, node)) = self
            .named_transforms
            .iter()
            .find(|(a, _)| Arc::ptr_eq(a, &arc))
        {
            return node.clone();
        }
        let node = Shared::new(NamedTransformNode::from_library(&arc));
        self.named_transforms.push((arc, node.clone()));
        node
    }

    /// `getColorSpace(name)`: the node of the config's color space, or none.
    pub(crate) fn color_space_by_name(&mut self, name: &[u8]) -> Option<ColorSpaceRc> {
        let arc = self
            .config
            .color_space(name)
            .and_then(|cs| self.config.color_space_arc(cs))?;
        Some(self.color_space(arc))
    }

    fn look_by_name(&mut self, name: &[u8]) -> Option<Shared<LookNode>> {
        let arc = self
            .config
            .look(name)
            .and_then(|l| self.config.look_arc(l))?;
        Some(self.look(arc))
    }

    fn view_transform_by_name(&mut self, name: &[u8]) -> Option<ViewTransformRc> {
        let arc = self
            .config
            .view_transform(name)
            .and_then(|vt| self.config.view_transform_arc(vt))?;
        Some(self.view_transform(arc))
    }

    fn named_transform_by_name(&mut self, name: &[u8]) -> Option<NamedTransformRc> {
        let arc = self
            .config
            .named_transform(name)
            .and_then(|nt| self.config.named_transform_arc(nt))?;
        Some(self.named_transform(arc))
    }

    fn default_scene_to_display_view_transform(&mut self) -> Option<ViewTransformRc> {
        let arc = self
            .config
            .default_scene_to_display_view_transform()
            .and_then(|vt| self.config.view_transform_arc(vt))?;
        Some(self.view_transform(arc))
    }
}

/// The port's error for a call on a config the library is working on, from a callback the
/// library calls meanwhile (see the module's documentation; docs/deviations.md, D-9).
fn busy() -> PyErr {
    pyo3::exceptions::PyRuntimeError::new_err(
        "Config: a callback of a call on this config uses it while the library works on it, \
         which the port can't share (docs/deviations.md, D-9)",
    )
}

/// `f` of the config `rc`, its objects written back first.
pub(crate) fn with_config<R>(
    rc: &ConfigRc,
    f: impl FnOnce(&mut ConfigNode) -> PyResult<R>,
) -> PyResult<R> {
    let mut node = rc.try_lock().ok_or_else(busy)?;
    node.sync()?;
    f(&mut node)
}

/// `f` of two configs (the same one twice, once locked).
fn with_two<R>(
    a: &ConfigRc,
    b: &ConfigRc,
    f: impl FnOnce(&Config, &Config) -> PyResult<R>,
) -> PyResult<R> {
    if a.same(b) {
        return with_config(a, |n| f(&n.config, &n.config));
    }
    with_config(a, |na| with_config(b, |nb| f(&na.config, &nb.config)))
}

/// A returned config of its own.
pub(crate) fn config_out(py: Python<'_>, config: Config) -> Out {
    out(py, CONFIG, &Shared::new(ConfigNode::new(config)))
}

/// A returned config the library made.
fn arc_out(py: Python<'_>, config: ocio::Result<Arc<Config>>) -> Out {
    let config = py_result(py, config)?;
    out(py, CONFIG, &Shared::new(ConfigNode::from_arc(config)))
}

/// A `ConstConfigRcPtr` argument (pybind11's holder caster): an instance; with conversions,
/// also `None`, a null pointer. `Ok(None)`: refused.
pub(crate) fn config_arg(
    obj: &Bound<'_, PyAny>,
    convert: bool,
) -> PyResult<Option<Option<ConfigRc>>> {
    holder_arg::<ConfigNode>(obj, CONFIG, convert)
}

// ---------------------------------------------------------------------------------------------
// Arguments

/// A `const char *` argument's bytes, a null pointer as the empty string: what the library
/// makes of it where upstream reads `!s || !*s` or makes a `std::string` only of a non-null
/// one (probed: none of these calls kills the wheel given `None`).
fn text(s: CharPtr) -> Vec<u8> {
    s.0.unwrap_or_default()
}

/// A `const char *` argument upstream makes a `std::string` of (or reads) unchecked: `None`
/// kills the wheel's process (probed), the port's error (U-123).
fn checked(s: CharPtr, what: &str) -> PyResult<Vec<u8>> {
    s.0.ok_or_else(|| null_string(what))
}

/// The library's enum of the C++ value `v` among `values`, else U-112's error (the wheel
/// takes a value outside the enumerators, which the library can't).
fn enumerator<E: Copy>(qualified: &str, v: i64, values: &[(E, i64)]) -> PyResult<E> {
    values
        .iter()
        .find(|(_, x)| *x == v)
        .map(|(e, _)| *e)
        .ok_or_else(|| outside_enumerators(qualified, v))
}

fn search_reference(v: i64) -> PyResult<SearchReferenceSpaceType> {
    use SearchReferenceSpaceType::*;
    enumerator(
        SEARCH_REFERENCE_SPACE_TYPE,
        v,
        &[(Scene, 0), (Display, 1), (All, 2)],
    )
}

fn color_space_visibility(v: i64) -> PyResult<ColorSpaceVisibility> {
    use ColorSpaceVisibility::*;
    enumerator(
        COLOR_SPACE_VISIBILITY,
        v,
        &[(Active, 0), (Inactive, 1), (All, 2)],
    )
}

fn named_transform_visibility(v: i64) -> PyResult<NamedTransformVisibility> {
    use NamedTransformVisibility::*;
    enumerator(
        NAMED_TRANSFORM_VISIBILITY,
        v,
        &[(Active, 0), (Inactive, 1), (All, 2)],
    )
}

fn view_type(v: i64) -> PyResult<ViewType> {
    enumerator(
        VIEW_TYPE,
        v,
        &[(ViewType::Shared, 0), (ViewType::DisplayDefined, 1)],
    )
}

fn direction(v: i64) -> PyResult<TransformDirection> {
    enumerator(
        TRANSFORM_DIRECTION,
        v,
        &[
            (TransformDirection::Forward, 0),
            (TransformDirection::Inverse, 1),
        ],
    )
}

/// A C++ `char` argument as pybind11's caster gives it (`type_caster<char>::cast_op`): a `str`
/// or `bytes` of one character (as UTF-8, a code point up to 0xFF); its errors are ValueErrors
/// at the call ("Cannot convert None to a character" for `None`, which only the conversions
/// take).
fn char_arg(obj: &Bound<'_, PyAny>, convert: bool) -> PyResult<Option<PyResult<u8>>> {
    use pyo3::exceptions::PyValueError;
    if obj.is_none() {
        return Ok(
            convert.then(|| Err(PyValueError::new_err("Cannot convert None to a character")))
        );
    }
    let Some(StdString(s)) = <StdString as crate::dispatch::Cast>::load(obj, convert) else {
        return Ok(None);
    };
    let value = match s.len() {
        0 => Err(PyValueError::new_err(
            "Cannot convert empty string to a character",
        )),
        1 => Ok(s[0]),
        2..=4 if s[0] >= 0x80 => {
            let width = if s[0] & 0xE0 == 0xC0 {
                2
            } else if s[0] & 0xF0 == 0xE0 {
                3
            } else if s[0] & 0xF8 == 0xF0 {
                4
            } else {
                0
            };
            if width == s.len() {
                if width == 2 && s[0] & 0xFC == 0xC0 {
                    Ok(((s[0] & 0x03) << 6) | (s[1] & 0x3F))
                } else {
                    Err(PyValueError::new_err(
                        "Character code point not in range(0x100)",
                    ))
                }
            } else {
                Err(PyValueError::new_err(
                    "Expected a character, but multi-character string found",
                ))
            }
        }
        _ => Err(PyValueError::new_err(
            "Expected a character, but multi-character string found",
        )),
    };
    Ok(Some(value))
}

/// [`arg!`] inside a [`config_method`] body.
macro_rules! arg_in {
    ($a:expr, $i:expr, $convert:expr, $t:ty) => {
        match <$t as $crate::dispatch::Cast>::try_load(&$a[$i], $convert)? {
            Some(v) => v,
            None => return Ok(None),
        }
    };
}

/// A method of the config: `|py, n: &mut ConfigNode, a, convert| ...` after the write-back.
macro_rules! config_method {
    ($name:literal, [$($param:expr),*], $ret:expr, $body:expr) => {
        method_def!($name, [SELF $(, $param)*], $ret, |py, o, a, convert| {
            let rc = this_shared!(o, a, ConfigNode);
            let f: &dyn Fn(Python<'_>, &mut ConfigNode, &[Bound<'_, PyAny>], bool) -> Out = &$body;
            with_config(&rc, |n| f(py, n, a, convert))
        })
    };
}

/// A method returning an iterator of the config (without arguments of its own).
macro_rules! config_iterator {
    ($name:literal, [], $qualified:expr, $args:expr) => {
        method_def!($name, [SELF], $qualified, |py, o, a, _| {
            let rc = this_shared!(o, a, ConfigNode);
            iterator_out(py, $qualified, rc, IterArgs::None)
        })
    };
}

// ---------------------------------------------------------------------------------------------
// Creation and copies (PyConfig.cpp:189-216 @ v2.5.2)

// .def(py::init(&Config::Create)).
static INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    operator: false,
    overloads: &[Overload {
        params: &[SELF],
        ret: "None",
        call: |py, o, a, _| {
            let config = py_result(py, Config::new())?;
            construct(&a[0], o.params[0].ty, Shared::new(ConfigNode::new(config)))?;
            none(py)
        },
        pos_only: 0,
    }],
};

// .def("__deepcopy__", [](const ConstConfigRcPtr & self, py::dict) { return
// self->createEditableCopy(); }, "memo"_a).
static DEEPCOPY: Function = method_def!(
    "__deepcopy__",
    [SELF, Param::new("memo", "dict")],
    CONFIG,
    |py, o, a, _| {
        let rc = this_shared!(o, a, ConfigNode);
        if !a[1].is_instance_of::<PyDict>() {
            return Ok(None);
        }
        let copy = with_config(&rc, |n| Ok(n.config.clone()))?;
        config_out(py, copy)
    }
);

// .def_static("CreateRaw", &Config::CreateRaw).
static CREATE_RAW: Function = static_def!("CreateRaw", [], CONFIG, |py, _, _, _| {
    arc_out(py, Config::create_raw())
});

// .def_static("CreateFromEnv", &Config::CreateFromEnv).
static CREATE_FROM_ENV: Function = static_def!("CreateFromEnv", [], CONFIG, |py, _, _, _| {
    arc_out(py, Config::create_from_env())
});

// .def_static("CreateFromFile", &Config::CreateFromFile, "fileName"_a): a `const char *`.
static CREATE_FROM_FILE: Function = static_def!(
    "CreateFromFile",
    [Param::new("fileName", "str")],
    CONFIG,
    |py, _, a, convert| {
        let name = text(arg!(a, 0, convert, CharPtr));
        arc_out(py, Config::create_from_file(name))
    }
);

// .def_static("CreateFromStream", [](const std::string & str) {...}, "str"_a).
static CREATE_FROM_STREAM: Function = static_def!(
    "CreateFromStream",
    [Param::new("str", "str")],
    CONFIG,
    |py, _, a, convert| {
        let StdString(text) = arg!(a, 0, convert, StdString);
        arc_out(py, Config::create_from_stream(&text))
    }
);

// .def_static("CreateFromBuiltinConfig", &Config::CreateFromBuiltinConfig): a `const char *`
// without a name.
static CREATE_FROM_BUILTIN_CONFIG: Function = static_def!(
    "CreateFromBuiltinConfig",
    [Param::new("arg0", "str")],
    CONFIG,
    |py, _, a, convert| {
        let name = checked(
            arg!(a, 0, convert, CharPtr),
            "Config.CreateFromBuiltinConfig(arg0)",
        )?;
        arc_out(py, Config::create_from_builtin_config(name))
    }
);

// .def_static("CreateFromConfigIOProxy", &Config::CreateFromConfigIOProxy): a
// `std::shared_ptr<ConfigIOProxy>` without a name.
static CREATE_FROM_CONFIG_IO_PROXY: Function = static_def!(
    "CreateFromConfigIOProxy",
    [Param::new("arg0", CONFIG_IO_PROXY)],
    CONFIG,
    |py, _, a, convert| {
        let Some(proxy) = crate::py_config_io_proxy::proxy_arg(&a[0], convert)? else {
            return Ok(None);
        };
        let Some(proxy) = proxy else {
            return Err(crate::py_config_io_proxy::null_proxy(
                "Config.CreateFromConfigIOProxy",
            ));
        };
        arc_out(py, Config::create_from_config_io_proxy(proxy))
    }
);

// ---------------------------------------------------------------------------------------------
// Versions, names, serialization (PyConfig.cpp:217-262 @ v2.5.2)

static GET_MAJOR_VERSION: Function =
    config_method!("getMajorVersion", [], "int", |py, n, _, _| ok(
        py,
        n.config.major_version()
    ));

static SET_MAJOR_VERSION: Function = config_method!(
    "setMajorVersion",
    [Param::new("major", INT)],
    "None",
    |py, n, a, c| {
        let major = arg_in!(a, 1, c, u32);
        py_result(py, n.config.set_major_version(major))?;
        none(py)
    }
);

static GET_MINOR_VERSION: Function =
    config_method!("getMinorVersion", [], "int", |py, n, _, _| ok(
        py,
        n.config.minor_version()
    ));

static SET_MINOR_VERSION: Function = config_method!(
    "setMinorVersion",
    [Param::new("minor", INT)],
    "None",
    |py, n, a, c| {
        let minor = arg_in!(a, 1, c, u32);
        py_result(py, n.config.set_minor_version(minor))?;
        none(py)
    }
);

static SET_VERSION: Function = config_method!(
    "setVersion",
    [Param::new("major", INT), Param::new("minor", INT)],
    "None",
    |py, n, a, c| {
        let major = arg_in!(a, 1, c, u32);
        let minor = arg_in!(a, 2, c, u32);
        py_result(py, n.config.set_version(major, minor))?;
        none(py)
    }
);

static UPGRADE_TO_LATEST_VERSION: Function =
    config_method!("upgradeToLatestVersion", [], "None", |py, n, _, _| {
        py_result(py, n.config.upgrade_to_latest_version())?;
        none(py)
    });

static VALIDATE: Function = config_method!("validate", [], "None", |py, n, _, _| {
    py_result(py, n.config.validate())?;
    none(py)
});

static GET_NAME: Function = config_method!("getName", [], "str", |py, n, _, _| string_out(
    py,
    n.config.name()
));

static SET_NAME: Function = config_method!(
    "setName",
    [Param::new("name", "str")],
    "None",
    |py, n, a, c| {
        let NonNullCharPtr(name) = arg_in!(a, 1, c, NonNullCharPtr);
        n.config.set_name(name);
        none(py)
    }
);

static GET_FAMILY_SEPARATOR: Function =
    config_method!("getFamilySeparator", [], "str", |py, n, _, _| string_out(
        py,
        &[n.config.family_separator()]
    ));

static SET_FAMILY_SEPARATOR: Function = config_method!(
    "setFamilySeparator",
    [Param::new("separator", "str")],
    "None",
    |py, n, a, c| {
        let Some(separator) = char_arg(&a[1], c)? else {
            return Ok(None);
        };
        py_result(py, n.config.set_family_separator(separator?))?;
        none(py)
    }
);

static GET_DESCRIPTION: Function = config_method!("getDescription", [], "str", |py, n, _, _| {
    string_out(py, n.config.description())
});

static SET_DESCRIPTION: Function = config_method!(
    "setDescription",
    [Param::new("description", "str")],
    "None",
    |py, n, a, c| {
        let description = text(arg_in!(a, 1, c, CharPtr));
        n.config.set_description(description);
        none(py)
    }
);

/// `std::ofstream f(fileName.c_str())`: the file, created or truncated, in text mode, or none
/// when it can't be opened (writes to it are then lost); see `GroupTransform.write`.
fn open_text_file(file_name: &[u8]) -> PyResult<Option<std::fs::File>> {
    crate::transforms::py_group_transform::open_text_file(file_name)
}

// .def("serialize", [](ConfigRcPtr & self, const std::string & fileName) {...}, "fileName"_a),
// .def("serialize", [](ConfigRcPtr & self) {...}): the text the library writes, into a file
// in text mode (opened before the config is written, so a failure leaves it empty).
static SERIALIZE: Function = Function {
    name: "serialize",
    constructor: false,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[SELF, Param::new("fileName", "str")],
            ret: "None",
            call: |py, o, a, convert| {
                let rc = this_shared!(o, a, ConfigNode);
                let StdString(file_name) = arg!(a, 1, convert, StdString);
                with_config(&rc, |n| {
                    let file = open_text_file(&file_name)?;
                    let text = py_result(py, n.config.serialize())?;
                    if let Some(mut file) = file {
                        let text = if cfg!(windows) {
                            crate::transforms::py_group_transform::crlf(&text)
                        } else {
                            text
                        };
                        let _ = std::io::Write::write_all(&mut file, &text);
                    }
                    none(py)
                })
            },
            pos_only: 0,
        },
        Overload {
            params: &[SELF],
            ret: "str",
            call: |py, o, a, _| {
                let rc = this_shared!(o, a, ConfigNode);
                with_config(&rc, |n| {
                    string_out(py, &py_result(py, n.config.serialize())?)
                })
            },
            pos_only: 0,
        },
    ],
};

// .def("getCacheID", (const char * (Config::*)() const)), .def("getCacheID", (const char *
// (Config::*)(const ConstContextRcPtr &) const), "context"_a).
static GET_CACHE_ID: Function = Function {
    name: "getCacheID",
    constructor: false,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[SELF],
            ret: "str",
            call: |py, o, a, _| {
                let rc = this_shared!(o, a, ConfigNode);
                with_config(&rc, |n| {
                    let id = py_result(py, n.config.cache_id())?;
                    string_out(py, id.as_bytes())
                })
            },
            pos_only: 0,
        },
        Overload {
            params: &[SELF, Param::new("context", CONTEXT)],
            ret: "str",
            call: |py, o, a, convert| {
                let rc = this_shared!(o, a, ConfigNode);
                let Some(context) = context_arg(&a[1], convert)? else {
                    return Ok(None);
                };
                with_config(&rc, |n| {
                    let id = match &context {
                        None => n.config.cache_id_with_context(None),
                        Some(ctx) => ctx.read(|c| n.config.cache_id_with_context(Some(c))),
                    };
                    string_out(py, py_result(py, id)?.as_bytes())
                })
            },
            pos_only: 0,
        },
    ],
};

// ---------------------------------------------------------------------------------------------
// Resources (PyConfig.cpp:264-302 @ v2.5.2)

// .def("getCurrentContext", &Config::getCurrentContext): the config's own context.
static GET_CURRENT_CONTEXT: Function =
    config_method!("getCurrentContext", [], CONTEXT, |py, n, _, _| {
        out(py, CONTEXT, &ContextRc::Config(n.config.current_context()))
    });

static ADD_ENVIRONMENT_VAR: Function = config_method!(
    "addEnvironmentVar",
    [Param::new("name", "str"), Param::new("defaultValue", "str")],
    "None",
    |py, n, a, c| {
        let name = text(arg_in!(a, 1, c, CharPtr));
        let CharPtr(value) = arg_in!(a, 2, c, CharPtr);
        n.config.add_environment_var(name, value.as_deref());
        none(py)
    }
);

static GET_ENVIRONMENT_VAR_NAMES: Function = config_iterator!(
    "getEnvironmentVarNames",
    [],
    ENVIRONMENT_VAR_NAME_ITERATOR,
    |_a, _c| Ok(Some(IterArgs::None))
);

static GET_ENVIRONMENT_VAR_DEFAULT: Function = config_method!(
    "getEnvironmentVarDefault",
    [Param::new("name", "str")],
    "str",
    |py, n, a, c| {
        let name = text(arg_in!(a, 1, c, CharPtr));
        string_out(py, n.config.environment_var_default(name))
    }
);

static CLEAR_ENVIRONMENT_VARS: Function =
    config_method!("clearEnvironmentVars", [], "None", |py, n, _, _| {
        n.config.clear_environment_vars();
        none(py)
    });

static SET_ENVIRONMENT_MODE: Function = config_method!(
    "setEnvironmentMode",
    [Param::new("mode", ENVIRONMENT_MODE)],
    "None",
    |py, n, a, _| {
        let Some(v) = crate::enums::enum_value(&a[1], ENVIRONMENT_MODE)? else {
            return Ok(None);
        };
        n.config.set_environment_mode(environment_mode(v)?);
        none(py)
    }
);

static GET_ENVIRONMENT_MODE: Function =
    config_method!("getEnvironmentMode", [], ENVIRONMENT_MODE, |py, n, _, _| {
        enum_out(py, ENVIRONMENT_MODE, n.config.environment_mode() as i64)
    });

static LOAD_ENVIRONMENT: Function = config_method!("loadEnvironment", [], "None", |py, n, _, _| {
    n.config.load_environment();
    none(py)
});

static GET_SEARCH_PATH: Function = config_method!("getSearchPath", [], "str", |py, n, _, _| {
    string_out(py, c_str(&n.config.search_path()))
});

static SET_SEARCH_PATH: Function = config_method!(
    "setSearchPath",
    [Param::new("path", "str")],
    "None",
    |py, n, a, c| {
        let path = text(arg_in!(a, 1, c, CharPtr));
        n.config.set_search_path(path);
        none(py)
    }
);

static GET_SEARCH_PATHS: Function =
    config_iterator!("getSearchPaths", [], SEARCH_PATH_ITERATOR, |_a, _c| Ok(
        Some(IterArgs::None)
    ));

static CLEAR_SEARCH_PATHS: Function =
    config_method!("clearSearchPaths", [], "None", |py, n, _, _| {
        n.config.clear_search_paths();
        none(py)
    });

static ADD_SEARCH_PATH: Function = config_method!(
    "addSearchPath",
    [Param::new("path", "str")],
    "None",
    |py, n, a, c| {
        let path = text(arg_in!(a, 1, c, CharPtr));
        n.config.add_search_path(path);
        none(py)
    }
);

static GET_WORKING_DIR: Function = config_method!("getWorkingDir", [], "str", |py, n, _, _| {
    string_out(py, c_str(&n.config.working_dir()))
});

static SET_WORKING_DIR: Function = config_method!(
    "setWorkingDir",
    [Param::new("dirName", "str")],
    "None",
    |py, n, a, c| {
        let dir = text(arg_in!(a, 1, c, CharPtr));
        n.config.set_working_dir(dir);
        none(py)
    }
);

// .def("getConfigIOProxy", &Config::getConfigIOProxy): the Python object of the proxy, or
// None.
static GET_CONFIG_IO_PROXY: Function =
    config_method!("getConfigIOProxy", [], CONFIG_IO_PROXY, |py, n, _, _| {
        crate::py_config_io_proxy::proxy_out(py, n.config.config_io_proxy())
    });

static SET_CONFIG_IO_PROXY: Function = config_method!(
    "setConfigIOProxy",
    [Param::new("ciop", CONFIG_IO_PROXY)],
    "None",
    |py, n, a, c| {
        let Some(proxy) = crate::py_config_io_proxy::proxy_arg(&a[1], c)? else {
            return Ok(None);
        };
        n.config.set_config_io_proxy(proxy);
        none(py)
    }
);

// ---------------------------------------------------------------------------------------------
// Color spaces (PyConfig.cpp:304-384 @ v2.5.2)

/// A returned color space of the config, or None.
fn color_space_out(py: Python<'_>, cs: Option<ColorSpaceRc>) -> Out {
    match cs {
        Some(cs) => out(py, COLOR_SPACE, &cs),
        None => none(py),
    }
}

// .def("getColorSpaces", &Config::getColorSpaces, "category"_a), .def("getColorSpaces",
// [](self, searchReferenceType, visibility) {...}), .def("getColorSpaces", [](self) {...}).
static GET_COLOR_SPACES: Function = Function {
    name: "getColorSpaces",
    constructor: false,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[SELF, Param::new("category", "str")],
            ret: COLOR_SPACE_SET,
            call: |py, o, a, convert| {
                let rc = this_shared!(o, a, ConfigNode);
                let category = text(arg!(a, 1, convert, CharPtr));
                let set = with_config(&rc, |n| Ok(n.config.color_spaces(category)))?;
                crate::py_color_space_set::set_out(py, &set)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param::new("searchReferenceType", SEARCH_REFERENCE_SPACE_TYPE),
                Param::new("visibility", COLOR_SPACE_VISIBILITY),
            ],
            ret: COLOR_SPACE_ITERATOR,
            call: |py, o, a, _| {
                let rc = this_shared!(o, a, ConfigNode);
                let srt = enum_param!(o, a, 1);
                let vis = enum_param!(o, a, 2);
                let args = IterArgs::Search(search_reference(srt)?, color_space_visibility(vis)?);
                iterator_out(py, COLOR_SPACE_ITERATOR, rc, args)
            },
            pos_only: 0,
        },
        Overload {
            params: &[SELF],
            ret: ACTIVE_COLOR_SPACE_ITERATOR,
            call: |py, o, a, _| {
                let rc = this_shared!(o, a, ConfigNode);
                iterator_out(py, ACTIVE_COLOR_SPACE_ITERATOR, rc, IterArgs::None)
            },
            pos_only: 0,
        },
    ],
};

static GET_COLOR_SPACE: Function = config_method!(
    "getColorSpace",
    [Param::new("name", "str")],
    COLOR_SPACE,
    |py, n, a, c| {
        let name = checked(arg_in!(a, 1, c, CharPtr), "Config.getColorSpace(name)")?;
        let cs = n.color_space_by_name(&name);
        color_space_out(py, cs)
    }
);

static GET_COLOR_SPACE_NAMES: Function = Function {
    name: "getColorSpaceNames",
    constructor: false,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[
                SELF,
                Param::new("searchReferenceType", SEARCH_REFERENCE_SPACE_TYPE),
                Param::new("visibility", COLOR_SPACE_VISIBILITY),
            ],
            ret: COLOR_SPACE_NAME_ITERATOR,
            call: |py, o, a, _| {
                let rc = this_shared!(o, a, ConfigNode);
                let srt = enum_param!(o, a, 1);
                let vis = enum_param!(o, a, 2);
                let args = IterArgs::Search(search_reference(srt)?, color_space_visibility(vis)?);
                iterator_out(py, COLOR_SPACE_NAME_ITERATOR, rc, args)
            },
            pos_only: 0,
        },
        Overload {
            params: &[SELF],
            ret: ACTIVE_COLOR_SPACE_NAME_ITERATOR,
            call: |py, o, a, _| {
                let rc = this_shared!(o, a, ConfigNode);
                iterator_out(py, ACTIVE_COLOR_SPACE_NAME_ITERATOR, rc, IterArgs::None)
            },
            pos_only: 0,
        },
    ],
};

static GET_CANONICAL_NAME: Function = config_method!(
    "getCanonicalName",
    [Param::new("name", "str")],
    "str",
    |py, n, a, c| {
        let name = checked(arg_in!(a, 1, c, CharPtr), "Config.getCanonicalName(name)")?;
        string_out(py, n.config.canonical_name(name))
    }
);

// .def("addColorSpace", &Config::addColorSpace, "colorSpace"_a): a copy of the color space;
// `None` (a null pointer) is dereferenced upstream (U-124).
static ADD_COLOR_SPACE: Function = config_method!(
    "addColorSpace",
    [Param::new("colorSpace", COLOR_SPACE)],
    "None",
    |py, n, a, c| {
        let Some(cs) = holder_arg::<ColorSpaceNode>(&a[1], COLOR_SPACE, c)? else {
            return Ok(None);
        };
        let cs = cs.ok_or_else(|| null_object("Config.addColorSpace(colorSpace)"))?;
        let cs = cs.lock().materialize()?;
        py_result(py, n.config.add_color_space(&cs))?;
        none(py)
    }
);

/// The port's error for `None` where upstream dereferences the null pointer (U-124).
fn null_object(what: &str) -> PyErr {
    pyo3::exceptions::PyValueError::new_err(format!(
        "{what}: None, a null pointer the wheel dereferences (its process dies; \
         docs/improvements.md, U-124)"
    ))
}

static REMOVE_COLOR_SPACE: Function = config_method!(
    "removeColorSpace",
    [Param::new("name", "str")],
    "None",
    |py, n, a, c| {
        let name = text(arg_in!(a, 1, c, CharPtr));
        n.config.remove_color_space(name);
        none(py)
    }
);

static IS_COLOR_SPACE_LINEAR: Function = config_method!(
    "isColorSpaceLinear",
    [
        Param::new("colorSpace", "str"),
        Param::new("referenceSpaceType", REFERENCE_SPACE_TYPE)
    ],
    "bool",
    |py, n, a, c| {
        let name = checked(
            arg_in!(a, 1, c, CharPtr),
            "Config.isColorSpaceLinear(colorSpace)",
        )?;
        let Some(v) = crate::enums::enum_value(&a[2], REFERENCE_SPACE_TYPE)? else {
            return Ok(None);
        };
        let reference = reference_space_type(v)?;
        ok(
            py,
            py_result(py, n.config.is_color_space_linear(name, reference))?,
        )
    }
);

static IS_COLOR_SPACE_USED: Function = config_method!(
    "isColorSpaceUsed",
    [Param::new("name", "str")],
    "bool",
    |py, n, a, c| {
        let name = text(arg_in!(a, 1, c, CharPtr));
        ok(py, n.config.is_color_space_used(name))
    }
);

static CLEAR_COLOR_SPACES: Function =
    config_method!("clearColorSpaces", [], "None", |py, n, _, _| {
        n.config.clear_color_spaces();
        none(py)
    });

static PARSE_COLOR_SPACE_FROM_STRING: Function = config_method!(
    "parseColorSpaceFromString",
    [Param::new("str", "str")],
    "str",
    |py, n, a, c| {
        let s = text(arg_in!(a, 1, c, CharPtr));
        string_out(py, n.config.parse_color_space_from_string(s))
    }
);

static IS_STRICT_PARSING_ENABLED: Function =
    config_method!("isStrictParsingEnabled", [], "bool", |py, n, _, _| ok(
        py,
        n.config.is_strict_parsing_enabled()
    ));

static SET_STRICT_PARSING_ENABLED: Function = config_method!(
    "setStrictParsingEnabled",
    [Param::new("enabled", "bool")],
    "None",
    |py, n, a, c| {
        let enabled = arg_in!(a, 1, c, bool);
        n.config.set_strict_parsing_enabled(enabled);
        none(py)
    }
);

static SET_INACTIVE_COLOR_SPACES: Function = config_method!(
    "setInactiveColorSpaces",
    [Param::new("inactiveColorSpaces", "str")],
    "None",
    |py, n, a, c| {
        let names = text(arg_in!(a, 1, c, CharPtr));
        n.config.set_inactive_color_spaces(names);
        none(py)
    }
);

static GET_INACTIVE_COLOR_SPACES: Function =
    config_method!("getInactiveColorSpaces", [], "str", |py, n, _, _| {
        string_out(py, n.config.inactive_color_spaces())
    });

static IS_INACTIVE_COLOR_SPACE: Function = config_method!(
    "isInactiveColorSpace",
    [Param::new("colorspace", "str")],
    "bool",
    |py, n, a, c| {
        let name = checked(
            arg_in!(a, 1, c, CharPtr),
            "Config.isInactiveColorSpace(colorspace)",
        )?;
        ok(py, n.config.is_inactive_color_space(name))
    }
);

// ---------------------------------------------------------------------------------------------
// Roles (PyConfig.cpp:386-402 @ v2.5.2)

// .def("setRole", &Config::setRole, "role"_a, "colorSpaceName"_a): a null role is
// "The role name is null." (Config.cpp:2978-2983 @ v2.5.2); a null color space removes it.
static SET_ROLE: Function = config_method!(
    "setRole",
    [
        Param::new("role", "str"),
        Param::new("colorSpaceName", "str")
    ],
    "None",
    |py, n, a, c| {
        let CharPtr(role) = arg_in!(a, 1, c, CharPtr);
        let CharPtr(cs) = arg_in!(a, 2, c, CharPtr);
        let Some(role) = role else {
            return Err(to_py_err(
                py,
                &ocio::Exception::new("The role name is null."),
            ));
        };
        py_result(py, n.config.set_role(role, cs.as_deref()))?;
        none(py)
    }
);

static HAS_ROLE: Function = config_method!(
    "hasRole",
    [Param::new("role", "str")],
    "bool",
    |py, n, a, c| {
        let role = text(arg_in!(a, 1, c, CharPtr));
        ok(py, n.config.has_role(role))
    }
);

static GET_ROLE_NAMES: Function =
    config_iterator!("getRoleNames", [], ROLE_NAME_ITERATOR, |_a, _c| Ok(Some(
        IterArgs::None
    )));

static GET_ROLES: Function =
    config_iterator!("getRoles", [], ROLE_COLOR_SPACE_ITERATOR, |_a, _c| Ok(
        Some(IterArgs::None)
    ));

static GET_ROLE_COLOR_SPACE: Function = config_method!(
    "getRoleColorSpace",
    [Param::new("roleName", "str")],
    "str",
    |py, n, a, c| {
        let role = text(arg_in!(a, 1, c, CharPtr));
        string_out(py, n.config.role_color_space(role))
    }
);

// ---------------------------------------------------------------------------------------------
// Displays and views (PyConfig.cpp:404-511 @ v2.5.2)

const fn text_default(name: &'static str) -> Param {
    empty_string(name)
}

static ADD_SHARED_VIEW: Function = config_method!(
    "addSharedView",
    [
        Param::new("view", "str"),
        Param::new("viewTransformName", "str"),
        Param::new("colorSpaceName", "str"),
        text_default("looks"),
        text_default("ruleName"),
        text_default("description")
    ],
    "None",
    |py, n, a, c| {
        let view = text(arg_in!(a, 1, c, CharPtr));
        let vt = text(arg_in!(a, 2, c, CharPtr));
        let cs = text(arg_in!(a, 3, c, CharPtr));
        let looks = text(arg_in!(a, 4, c, CharPtr));
        let rule = text(arg_in!(a, 5, c, CharPtr));
        let description = text(arg_in!(a, 6, c, CharPtr));
        py_result(
            py,
            n.config
                .add_shared_view(view, vt, cs, looks, rule, description),
        )?;
        none(py)
    }
);

static REMOVE_SHARED_VIEW: Function = config_method!(
    "removeSharedView",
    [Param::new("view", "str")],
    "None",
    |py, n, a, c| {
        let view = text(arg_in!(a, 1, c, CharPtr));
        py_result(py, n.config.remove_shared_view(view))?;
        none(py)
    }
);

static CLEAR_SHARED_VIEWS: Function =
    config_method!("clearSharedViews", [], "None", |py, n, _, _| {
        n.config.clear_shared_views();
        none(py)
    });

static GET_SHARED_VIEWS: Function =
    config_iterator!("getSharedViews", [], SHARED_VIEW_ITERATOR, |_a, _c| Ok(
        Some(IterArgs::None)
    ));

static GET_DEFAULT_DISPLAY: Function =
    config_method!("getDefaultDisplay", [], "str", |py, n, _, _| {
        string_out(py, n.config.default_display())
    });

static GET_DISPLAYS: Function = config_iterator!("getDisplays", [], DISPLAY_ITERATOR, |_a, _c| Ok(
    Some(IterArgs::None)
));

static GET_DISPLAYS_ALL: Function =
    config_iterator!("getDisplaysAll", [], DISPLAY_ALL_ITERATOR, |_a, _c| Ok(
        Some(IterArgs::None)
    ));

static GET_DEFAULT_VIEW: Function = Function {
    name: "getDefaultView",
    constructor: false,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[SELF, Param::new("display", "str")],
            ret: "str",
            call: |py, o, a, convert| {
                let rc = this_shared!(o, a, ConfigNode);
                let display = text(arg!(a, 1, convert, CharPtr));
                with_config(&rc, |n| string_out(py, n.config.default_view(display)))
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param::new("display", "str"),
                Param::new("colorSpacename", "str"),
            ],
            ret: "str",
            call: |py, o, a, convert| {
                let rc = this_shared!(o, a, ConfigNode);
                let display = text(arg!(a, 1, convert, CharPtr));
                let cs = text(arg!(a, 2, convert, CharPtr));
                with_config(&rc, |n| {
                    let view = py_result(py, n.config.default_view_for_color_space(display, cs))?;
                    string_out(py, view)
                })
            },
            pos_only: 0,
        },
    ],
};

static GET_VIEWS: Function = Function {
    name: "getViews",
    constructor: false,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[SELF, Param::new("display", "str")],
            ret: VIEW_ITERATOR,
            call: |py, o, a, convert| {
                let rc = this_shared!(o, a, ConfigNode);
                let StdString(display) = arg!(a, 1, convert, StdString);
                iterator_out(py, VIEW_ITERATOR, rc, IterArgs::Display(display))
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param::new("type", VIEW_TYPE),
                Param::new("display", "str"),
            ],
            ret: VIEW_FOR_VIEW_TYPE_ITERATOR,
            call: |py, o, a, convert| {
                let rc = this_shared!(o, a, ConfigNode);
                let t = enum_param!(o, a, 1);
                let StdString(display) = arg!(a, 2, convert, StdString);
                let args = IterArgs::TypeDisplay(view_type(t)?, display);
                iterator_out(py, VIEW_FOR_VIEW_TYPE_ITERATOR, rc, args)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param::new("display", "str"),
                Param::new("colorSpaceName", "str"),
            ],
            ret: VIEW_FOR_COLOR_SPACE_ITERATOR,
            call: |py, o, a, convert| {
                let rc = this_shared!(o, a, ConfigNode);
                let StdString(display) = arg!(a, 1, convert, StdString);
                let StdString(cs) = arg!(a, 2, convert, StdString);
                let args = IterArgs::DisplayColorSpace(display, cs);
                iterator_out(py, VIEW_FOR_COLOR_SPACE_ITERATOR, rc, args)
            },
            pos_only: 0,
        },
    ],
};

/// A getter of a display's view's text.
macro_rules! display_view_getter {
    ($name:literal, $get:ident) => {
        config_method!(
            $name,
            [Param::new("display", "str"), Param::new("view", "str")],
            "str",
            |py, n, a, c| {
                let display = text(arg_in!(a, 1, c, CharPtr));
                let view = text(arg_in!(a, 2, c, CharPtr));
                string_out(py, n.config.$get(display, view))
            }
        )
    };
}

static GET_DISPLAY_VIEW_TRANSFORM_NAME: Function =
    display_view_getter!("getDisplayViewTransformName", display_view_transform_name);
static GET_DISPLAY_VIEW_COLOR_SPACE_NAME: Function = display_view_getter!(
    "getDisplayViewColorSpaceName",
    display_view_color_space_name
);
static GET_DISPLAY_VIEW_LOOKS: Function =
    display_view_getter!("getDisplayViewLooks", display_view_looks);
static GET_DISPLAY_VIEW_RULE: Function =
    display_view_getter!("getDisplayViewRule", display_view_rule);
static GET_DISPLAY_VIEW_DESCRIPTION: Function =
    display_view_getter!("getDisplayViewDescription", display_view_description);

/// A predicate of a display and a view.
macro_rules! display_view_predicate {
    ($name:literal, $get:ident) => {
        config_method!(
            $name,
            [Param::new("display", "str"), Param::new("view", "str")],
            "bool",
            |py, n, a, c| {
                let display = text(arg_in!(a, 1, c, CharPtr));
                let view = text(arg_in!(a, 2, c, CharPtr));
                ok(py, n.config.$get(display, view))
            }
        )
    };
}

static HAS_VIEW: Function = display_view_predicate!("hasView", has_view);

static ADD_DISPLAY_VIEW: Function = Function {
    name: "addDisplayView",
    constructor: false,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[
                SELF,
                Param::new("display", "str"),
                Param::new("view", "str"),
                Param::new("colorSpaceName", "str"),
                text_default("looks"),
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let rc = this_shared!(o, a, ConfigNode);
                let display = text(arg!(a, 1, convert, CharPtr));
                let view = text(arg!(a, 2, convert, CharPtr));
                let cs = text(arg!(a, 3, convert, CharPtr));
                let looks = text(arg!(a, 4, convert, CharPtr));
                with_config(&rc, |n| {
                    py_result(py, n.config.add_display_view(display, view, cs, looks))?;
                    none(py)
                })
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param::new("display", "str"),
                Param::new("view", "str"),
                Param::new("viewTransform", "str"),
                Param::new("displayColorSpaceName", "str"),
                text_default("looks"),
                text_default("ruleName"),
                text_default("description"),
            ],
            ret: "None",
            call: |py, o, a, convert| {
                let rc = this_shared!(o, a, ConfigNode);
                let display = text(arg!(a, 1, convert, CharPtr));
                let view = text(arg!(a, 2, convert, CharPtr));
                let vt = text(arg!(a, 3, convert, CharPtr));
                let cs = text(arg!(a, 4, convert, CharPtr));
                let looks = text(arg!(a, 5, convert, CharPtr));
                let rule = text(arg!(a, 6, convert, CharPtr));
                let description = text(arg!(a, 7, convert, CharPtr));
                with_config(&rc, |n| {
                    py_result(
                        py,
                        n.config.add_display_view_with_view_transform(
                            display,
                            view,
                            vt,
                            cs,
                            looks,
                            rule,
                            description,
                        ),
                    )?;
                    none(py)
                })
            },
            pos_only: 0,
        },
    ],
};

static IS_VIEW_SHARED: Function = display_view_predicate!("isViewShared", is_view_shared);

static ADD_DISPLAY_SHARED_VIEW: Function = config_method!(
    "addDisplaySharedView",
    [Param::new("display", "str"), Param::new("view", "str")],
    "None",
    |py, n, a, c| {
        let display = text(arg_in!(a, 1, c, CharPtr));
        let view = text(arg_in!(a, 2, c, CharPtr));
        py_result(py, n.config.add_display_shared_view(display, view))?;
        none(py)
    }
);

static REMOVE_DISPLAY_VIEW: Function = config_method!(
    "removeDisplayView",
    [Param::new("display", "str"), Param::new("view", "str")],
    "None",
    |py, n, a, c| {
        let display = text(arg_in!(a, 1, c, CharPtr));
        let view = text(arg_in!(a, 2, c, CharPtr));
        py_result(py, n.config.remove_display_view(display, view))?;
        none(py)
    }
);

static CLEAR_DISPLAYS: Function = config_method!("clearDisplays", [], "None", |py, n, _, _| {
    n.config.clear_displays();
    none(py)
});

/// Two `ConstConfigRcPtr` arguments, or the next overload; `None` is dereferenced upstream
/// (U-124).
macro_rules! two_configs {
    ($a:expr, $convert:expr, $what:literal) => {{
        let Some(first) = config_arg(&$a[0], $convert)? else {
            return Ok(None);
        };
        let Some(second) = config_arg(&$a[1], $convert)? else {
            return Ok(None);
        };
        (
            first.ok_or_else(|| null_object($what))?,
            second.ok_or_else(|| null_object($what))?,
        )
    }};
}

static ARE_VIEWS_EQUAL: Function = static_def!(
    "AreViewsEqual",
    [
        Param::new("first", CONFIG),
        Param::new("second", CONFIG),
        Param::new("dispName", "str"),
        Param::new("viewName", "str")
    ],
    "bool",
    |py, _, a, convert| {
        let (first, second) = two_configs!(a, convert, "Config.AreViewsEqual");
        let display = text(arg!(a, 2, convert, CharPtr));
        let view = text(arg!(a, 3, convert, CharPtr));
        let equal = with_two(&first, &second, |x, y| {
            Ok(Config::are_views_equal(x, y, &display, &view))
        })?;
        ok(py, equal)
    }
);

// ---------------------------------------------------------------------------------------------
// The virtual display (PyConfig.cpp:513-586 @ v2.5.2)

static HAS_VIRTUAL_VIEW: Function = config_method!(
    "hasVirtualView",
    [Param::new("view", "str")],
    "bool",
    |py, n, a, c| {
        let view = text(arg_in!(a, 1, c, CharPtr));
        ok(py, n.config.has_virtual_view(view))
    }
);

static ADD_VIRTUAL_DISPLAY_VIEW: Function = config_method!(
    "addVirtualDisplayView",
    [
        Param::new("view", "str"),
        Param::new("viewTransformName", "str"),
        Param::new("colorSpaceName", "str"),
        text_default("looks"),
        text_default("ruleName"),
        text_default("description")
    ],
    "None",
    |py, n, a, c| {
        let view = text(arg_in!(a, 1, c, CharPtr));
        let vt = text(arg_in!(a, 2, c, CharPtr));
        let cs = text(arg_in!(a, 3, c, CharPtr));
        let looks = text(arg_in!(a, 4, c, CharPtr));
        let rule = text(arg_in!(a, 5, c, CharPtr));
        let description = text(arg_in!(a, 6, c, CharPtr));
        py_result(
            py,
            n.config
                .add_virtual_display_view(view, vt, cs, looks, rule, description),
        )?;
        none(py)
    }
);

static IS_VIRTUAL_VIEW_SHARED: Function = config_method!(
    "isVirtualViewShared",
    [Param::new("view", "str")],
    "bool",
    |py, n, a, c| {
        let view = text(arg_in!(a, 1, c, CharPtr));
        ok(py, n.config.is_virtual_view_shared(view))
    }
);

static ADD_VIRTUAL_DISPLAY_SHARED_VIEW: Function = config_method!(
    "addVirtualDisplaySharedView",
    [Param::new("sharedView", "str")],
    "None",
    |py, n, a, c| {
        let view = text(arg_in!(a, 1, c, CharPtr));
        py_result(py, n.config.add_virtual_display_shared_view(view))?;
        none(py)
    }
);

// .def("getVirtualDisplayViews", [](self, ViewType type) {...}, "display"_a).
static GET_VIRTUAL_DISPLAY_VIEWS: Function = method_def!(
    "getVirtualDisplayViews",
    [SELF, Param::new("display", VIEW_TYPE)],
    VIRTUAL_VIEW_ITERATOR,
    |py, o, a, _| {
        let rc = this_shared!(o, a, ConfigNode);
        let t = enum_param!(o, a, 1);
        iterator_out(py, VIRTUAL_VIEW_ITERATOR, rc, IterArgs::Type(view_type(t)?))
    }
);

/// A getter of the virtual display's view's text.
macro_rules! virtual_view_getter {
    ($name:literal, $get:ident) => {
        config_method!($name, [Param::new("view", "str")], "str", |py, n, a, c| {
            let view = text(arg_in!(a, 1, c, CharPtr));
            string_out(py, n.config.$get(view))
        })
    };
}

static GET_VIRTUAL_DISPLAY_VIEW_TRANSFORM_NAME: Function = virtual_view_getter!(
    "getVirtualDisplayViewTransformName",
    virtual_display_view_transform_name
);
static GET_VIRTUAL_DISPLAY_VIEW_COLOR_SPACE_NAME: Function = virtual_view_getter!(
    "getVirtualDisplayViewColorSpaceName",
    virtual_display_view_color_space_name
);
static GET_VIRTUAL_DISPLAY_VIEW_LOOKS: Function =
    virtual_view_getter!("getVirtualDisplayViewLooks", virtual_display_view_looks);
static GET_VIRTUAL_DISPLAY_VIEW_RULE: Function =
    virtual_view_getter!("getVirtualDisplayViewRule", virtual_display_view_rule);
static GET_VIRTUAL_DISPLAY_VIEW_DESCRIPTION: Function = virtual_view_getter!(
    "getVirtualDisplayViewDescription",
    virtual_display_view_description
);

static REMOVE_VIRTUAL_DISPLAY_VIEW: Function = config_method!(
    "removeVirtualDisplayView",
    [Param::new("view", "str")],
    "None",
    |py, n, a, c| {
        let view = text(arg_in!(a, 1, c, CharPtr));
        n.config.remove_virtual_display_view(view);
        none(py)
    }
);

static CLEAR_VIRTUAL_DISPLAY: Function =
    config_method!("clearVirtualDisplay", [], "None", |py, n, _, _| {
        n.config.clear_virtual_display();
        none(py)
    });

// .def("instantiateDisplayFromMonitorName", &Config::instantiateDisplayFromMonitorName,
// "monitorName"_a): a null name is "The system monitor name cannot be null."
// (Config.cpp:4104-4109 @ v2.5.2).
static INSTANTIATE_DISPLAY_FROM_MONITOR_NAME: Function = config_method!(
    "instantiateDisplayFromMonitorName",
    [Param::new("monitorName", "str")],
    "int",
    |py, n, a, c| {
        let CharPtr(name) = arg_in!(a, 1, c, CharPtr);
        let Some(name) = name else {
            return Err(to_py_err(
                py,
                &ocio::Exception::new("The system monitor name cannot be null."),
            ));
        };
        ok(
            py,
            py_result(py, n.config.instantiate_display_from_monitor_name(name))?,
        )
    }
);

// .def("instantiateDisplayFromICCProfile", &Config::instantiateDisplayFromICCProfile,
// "ICCProfileFilepath"_a): "The ICC profile filepath cannot be null." (Config.cpp:4120-4125 @
// v2.5.2).
static INSTANTIATE_DISPLAY_FROM_ICC_PROFILE: Function = config_method!(
    "instantiateDisplayFromICCProfile",
    [Param::new("ICCProfileFilepath", "str")],
    "int",
    |py, n, a, c| {
        let CharPtr(path) = arg_in!(a, 1, c, CharPtr);
        let Some(path) = path else {
            return Err(to_py_err(
                py,
                &ocio::Exception::new("The ICC profile filepath cannot be null."),
            ));
        };
        ok(
            py,
            py_result(py, n.config.instantiate_display_from_icc_profile(path))?,
        )
    }
);

/// The indices of the displays (temporary ones included) whose names equal `display`, ignoring
/// case (`StringUtils::Compare` of the whole `std::string`s).
fn displays_named(config: &Config, display: &[u8]) -> Vec<i32> {
    (0..config.num_displays_all())
        .filter(|&i| compare(display, config.display_all(i)))
        .collect()
}

// .def("isDisplayTemporary", [](self, const std::string & display) -> bool {...},
// "display"_a): the first display of that name.
static IS_DISPLAY_TEMPORARY: Function = config_method!(
    "isDisplayTemporary",
    [Param::new("display", "str")],
    "bool",
    |py, n, a, c| {
        let StdString(display) = arg_in!(a, 1, c, StdString);
        let first = displays_named(&n.config, &display).first().copied();
        ok(py, first.is_some_and(|i| n.config.is_display_temporary(i)))
    }
);

// .def("setDisplayTemporary", [](self, const std::string & display, bool isTemporary) {...},
// "display"_a, "isTemporary"_a): each display of that name.
static SET_DISPLAY_TEMPORARY: Function = config_method!(
    "setDisplayTemporary",
    [
        Param::new("display", "str"),
        Param::new("isTemporary", "bool")
    ],
    "None",
    |py, n, a, c| {
        let StdString(display) = arg_in!(a, 1, c, StdString);
        let temporary = arg_in!(a, 2, c, bool);
        for i in 0..n.config.num_displays_all() {
            if compare(&display, n.config.display_all(i)) {
                n.config.set_display_temporary(i, temporary);
            }
        }
        none(py)
    }
);

static ARE_VIRTUAL_VIEWS_EQUAL: Function = static_def!(
    "AreVirtualViewsEqual",
    [
        Param::new("first", CONFIG),
        Param::new("second", CONFIG),
        Param::new("viewName", "str")
    ],
    "bool",
    |py, _, a, convert| {
        let (first, second) = two_configs!(a, convert, "Config.AreVirtualViewsEqual");
        let view = text(arg!(a, 2, convert, CharPtr));
        let equal = with_two(&first, &second, |x, y| {
            Ok(Config::are_virtual_views_equal(x, y, &view))
        })?;
        ok(py, equal)
    }
);

// ---------------------------------------------------------------------------------------------
// Active displays and views (PyConfig.cpp:588-617 @ v2.5.2)

/// A setter or a change of a list, a `const char *` argument.
macro_rules! list_method {
    ($name:literal, $param:literal, $call:ident, $checked:expr) => {
        config_method!($name, [Param::new($param, "str")], "None", |py, n, a, c| {
            let s = match $checked {
                Some(what) => checked(arg_in!(a, 1, c, CharPtr), what)?,
                None => text(arg_in!(a, 1, c, CharPtr)),
            };
            py_result(py, n.config.$call(s))?;
            none(py)
        })
    };
}

static SET_ACTIVE_DISPLAYS: Function = list_method!(
    "setActiveDisplays",
    "displays",
    set_active_displays,
    Some("Config.setActiveDisplays(displays)")
);
static GET_ACTIVE_DISPLAYS: Function = config_iterator!(
    "getActiveDisplays",
    [],
    ACTIVE_DISPLAYS_LIST_ITERATOR,
    |_a, _c| Ok(Some(IterArgs::None))
);
static ADD_ACTIVE_DISPLAY: Function = list_method!(
    "addActiveDisplay",
    "display",
    add_active_display,
    None::<&str>
);
static REMOVE_ACTIVE_DISPLAY: Function = list_method!(
    "removeActiveDisplay",
    "display",
    remove_active_display,
    None::<&str>
);
static CLEAR_ACTIVE_DISPLAYS: Function =
    config_method!("clearActiveDisplays", [], "None", |py, n, _, _| {
        n.config.clear_active_displays();
        none(py)
    });
static GET_NUM_ACTIVE_DISPLAYS: Function =
    config_method!("getNumActiveDisplays", [], "int", |py, n, _, _| ok(
        py,
        n.config.num_active_displays()
    ));

static SET_ACTIVE_VIEWS: Function = list_method!(
    "setActiveViews",
    "views",
    set_active_views,
    Some("Config.setActiveViews(views)")
);
static GET_ACTIVE_VIEWS: Function = config_iterator!(
    "getActiveViews",
    [],
    ACTIVE_VIEWS_LIST_ITERATOR,
    |_a, _c| Ok(Some(IterArgs::None))
);
static ADD_ACTIVE_VIEW: Function =
    list_method!("addActiveView", "view", add_active_view, None::<&str>);
static REMOVE_ACTIVE_VIEW: Function =
    list_method!("removeActiveView", "view", remove_active_view, None::<&str>);
static CLEAR_ACTIVE_VIEWS: Function =
    config_method!("clearActiveViews", [], "None", |py, n, _, _| {
        n.config.clear_active_views();
        none(py)
    });
static GET_NUM_ACTIVE_VIEWS: Function =
    config_method!("getNumActiveViews", [], "int", |py, n, _, _| ok(
        py,
        n.config.num_active_views()
    ));

// ---------------------------------------------------------------------------------------------
// Luma (PyConfig.cpp:619-632 @ v2.5.2)

static GET_DEFAULT_LUMA_COEFS: Function = config_method!(
    "getDefaultLumaCoefs",
    [],
    "typing.Annotated[list[float], \"FixedSize(3)\"]",
    |py, n, _, _| ok(py, n.config.default_luma_coefs().to_vec())
);

static SET_DEFAULT_LUMA_COEFS: Function = config_method!(
    "setDefaultLumaCoefs",
    [Param::new(
        "rgb",
        "typing.Annotated[collections.abc.Sequence[typing.SupportsFloat], \"FixedSize(3)\"]"
    )],
    "None",
    |py, n, a, c| {
        let rgb = arg_in!(a, 1, c, [f64; 3]);
        n.config.set_default_luma_coefs(&rgb);
        none(py)
    }
);

// ---------------------------------------------------------------------------------------------
// Looks, view transforms, named transforms (PyConfig.cpp:634-698 @ v2.5.2)

static GET_LOOK: Function = config_method!(
    "getLook",
    [Param::new("name", "str")],
    LOOK,
    |py, n, a, c| {
        let name = text(arg_in!(a, 1, c, CharPtr));
        match n.look_by_name(&name) {
            Some(look) => out(py, LOOK, &look),
            None => none(py),
        }
    }
);

static GET_LOOK_NAMES: Function =
    config_iterator!("getLookNames", [], LOOK_NAME_ITERATOR, |_a, _c| Ok(Some(
        IterArgs::None
    )));
static GET_LOOKS: Function = config_iterator!("getLooks", [], LOOK_ITERATOR, |_a, _c| Ok(Some(
    IterArgs::None
)));

static ADD_LOOK: Function = config_method!(
    "addLook",
    [Param::new("look", LOOK)],
    "None",
    |py, n, a, c| {
        let Some(look) = holder_arg::<LookNode>(&a[1], LOOK, c)? else {
            return Ok(None);
        };
        let look = look.ok_or_else(|| null_object("Config.addLook(look)"))?;
        let look = look.lock().materialize()?;
        py_result(py, n.config.add_look(&look))?;
        none(py)
    }
);

static CLEAR_LOOKS: Function = config_method!("clearLooks", [], "None", |py, n, _, _| {
    n.config.clear_looks();
    none(py)
});

static GET_VIEW_TRANSFORM: Function = config_method!(
    "getViewTransform",
    [Param::new("name", "str")],
    VIEW_TRANSFORM,
    |py, n, a, c| {
        let name = text(arg_in!(a, 1, c, CharPtr));
        match n.view_transform_by_name(&name) {
            Some(vt) => out(py, VIEW_TRANSFORM, &vt),
            None => none(py),
        }
    }
);

static GET_VIEW_TRANSFORM_NAMES: Function = config_iterator!(
    "getViewTransformNames",
    [],
    VIEW_TRANSFORM_NAME_ITERATOR,
    |_a, _c| Ok(Some(IterArgs::None))
);
static GET_VIEW_TRANSFORMS: Function = config_iterator!(
    "getViewTransforms",
    [],
    VIEW_TRANSFORM_ITERATOR,
    |_a, _c| Ok(Some(IterArgs::None))
);

static ADD_VIEW_TRANSFORM: Function = config_method!(
    "addViewTransform",
    [Param::new("viewTransform", VIEW_TRANSFORM)],
    "None",
    |py, n, a, c| {
        let Some(vt) = holder_arg::<ViewTransformNode>(&a[1], VIEW_TRANSFORM, c)? else {
            return Ok(None);
        };
        let vt = vt.ok_or_else(|| null_object("Config.addViewTransform(viewTransform)"))?;
        let vt = vt.lock().materialize()?;
        py_result(py, n.config.add_view_transform(&vt))?;
        none(py)
    }
);

static GET_DEFAULT_SCENE_TO_DISPLAY_VIEW_TRANSFORM: Function = config_method!(
    "getDefaultSceneToDisplayViewTransform",
    [],
    VIEW_TRANSFORM,
    |py, n, _, _| match n.default_scene_to_display_view_transform() {
        Some(vt) => out(py, VIEW_TRANSFORM, &vt),
        None => none(py),
    }
);

static GET_DEFAULT_VIEW_TRANSFORM_NAME: Function =
    config_method!("getDefaultViewTransformName", [], "str", |py, n, _, _| {
        string_out(py, n.config.default_view_transform_name())
    });

static SET_DEFAULT_VIEW_TRANSFORM_NAME: Function = config_method!(
    "setDefaultViewTransformName",
    [Param::new("name", "str")],
    "None",
    |py, n, a, c| {
        let NonNullCharPtr(name) = arg_in!(a, 1, c, NonNullCharPtr);
        n.config.set_default_view_transform_name(name);
        none(py)
    }
);

static CLEAR_VIEW_TRANSFORMS: Function =
    config_method!("clearViewTransforms", [], "None", |py, n, _, _| {
        n.config.clear_view_transforms();
        none(py)
    });

// .def("getNamedTransform", &Config::getNamedTransform, "name"_a).
static GET_NAMED_TRANSFORM: Function = config_method!(
    "getNamedTransform",
    [Param::new("name", "str")],
    NAMED_TRANSFORM,
    |py, n, a, c| {
        let name = text(arg_in!(a, 1, c, CharPtr));
        match n.named_transform_by_name(&name) {
            Some(nt) => out(py, NAMED_TRANSFORM, &nt),
            None => none(py),
        }
    }
);

static GET_NAMED_TRANSFORM_NAMES: Function = Function {
    name: "getNamedTransformNames",
    constructor: false,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[SELF, Param::new("visibility", NAMED_TRANSFORM_VISIBILITY)],
            ret: NAMED_TRANSFORM_NAME_ITERATOR,
            call: |py, o, a, _| {
                let rc = this_shared!(o, a, ConfigNode);
                let v = enum_param!(o, a, 1);
                let args = IterArgs::Visibility(named_transform_visibility(v)?);
                iterator_out(py, NAMED_TRANSFORM_NAME_ITERATOR, rc, args)
            },
            pos_only: 0,
        },
        Overload {
            params: &[SELF],
            ret: ACTIVE_NAMED_TRANSFORM_NAME_ITERATOR,
            call: |py, o, a, _| {
                let rc = this_shared!(o, a, ConfigNode);
                iterator_out(py, ACTIVE_NAMED_TRANSFORM_NAME_ITERATOR, rc, IterArgs::None)
            },
            pos_only: 0,
        },
    ],
};

static GET_NAMED_TRANSFORMS: Function = Function {
    name: "getNamedTransforms",
    constructor: false,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[SELF, Param::new("visibility", NAMED_TRANSFORM_VISIBILITY)],
            ret: NAMED_TRANSFORM_ITERATOR,
            call: |py, o, a, _| {
                let rc = this_shared!(o, a, ConfigNode);
                let v = enum_param!(o, a, 1);
                let args = IterArgs::Visibility(named_transform_visibility(v)?);
                iterator_out(py, NAMED_TRANSFORM_ITERATOR, rc, args)
            },
            pos_only: 0,
        },
        Overload {
            params: &[SELF],
            ret: ACTIVE_NAMED_TRANSFORM_ITERATOR,
            call: |py, o, a, _| {
                let rc = this_shared!(o, a, ConfigNode);
                iterator_out(py, ACTIVE_NAMED_TRANSFORM_ITERATOR, rc, IterArgs::None)
            },
            pos_only: 0,
        },
    ],
};

// .def("addNamedTransform", &Config::addNamedTransform, "namedTransform"_a): a null one is
// "Named transform is null." (Config.cpp:3169-3174 @ v2.5.2).
static ADD_NAMED_TRANSFORM: Function = config_method!(
    "addNamedTransform",
    [Param::new("namedTransform", NAMED_TRANSFORM)],
    "None",
    |py, n, a, c| {
        let Some(nt) = holder_arg::<NamedTransformNode>(&a[1], NAMED_TRANSFORM, c)? else {
            return Ok(None);
        };
        let Some(nt) = nt else {
            return Err(to_py_err(
                py,
                &ocio::Exception::new("Named transform is null."),
            ));
        };
        let nt = nt.lock().materialize()?;
        py_result(py, n.config.add_named_transform(&nt))?;
        none(py)
    }
);

static CLEAR_NAMED_TRANSFORMS: Function =
    config_method!("clearNamedTransforms", [], "None", |py, n, _, _| {
        n.config.clear_named_transforms();
        none(py)
    });

// ---------------------------------------------------------------------------------------------
// Rules (PyConfig.cpp:700-721 @ v2.5.2)

static GET_VIEWING_RULES: Function =
    config_method!("getViewingRules", [], VIEWING_RULES, |py, n, _, _| {
        out(
            py,
            VIEWING_RULES,
            &RulesRc::Config(n.config.viewing_rules()),
        )
    });

static SET_VIEWING_RULES: Function = config_method!(
    "setViewingRules",
    [Param::new("ViewingRules", VIEWING_RULES)],
    "None",
    |py, n, a, c| {
        let rules = rules_arg::<ViewingRules, ConfigRules<ViewingRules>>(&a[1], VIEWING_RULES, c)?;
        let Some(rules) = rules else {
            return Ok(None);
        };
        let rules = rules.ok_or_else(|| null_object("Config.setViewingRules(ViewingRules)"))?;
        let copy = rules.read(ViewingRules::clone);
        n.config.set_viewing_rules(&copy);
        none(py)
    }
);

static GET_FILE_RULES: Function = config_method!("getFileRules", [], FILE_RULES, |py, n, _, _| {
    out(py, FILE_RULES, &FileRulesRc::Config(n.config.file_rules()))
});

static SET_FILE_RULES: Function = config_method!(
    "setFileRules",
    [Param::new("fileRules", FILE_RULES)],
    "None",
    |py, n, a, c| {
        let rules = rules_arg::<FileRules, ConfigRules<FileRules>>(&a[1], FILE_RULES, c)?;
        let Some(rules) = rules else {
            return Ok(None);
        };
        let rules = rules.ok_or_else(|| null_object("Config.setFileRules(fileRules)"))?;
        let copy = rules.read(FileRules::clone);
        n.config.set_file_rules(&copy);
        none(py)
    }
);

/// A rules argument (holder caster: `None` with conversions), or the next overload.
fn rules_arg<T: Clone + Send + Sync + 'static, C>(
    obj: &Bound<'_, PyAny>,
    qualified: &str,
    convert: bool,
) -> PyResult<Option<Option<Held<T, C>>>>
where
    C: crate::shared::ConfigCell<T> + std::fmt::Debug,
    Held<T, C>: Send + Sync,
{
    if obj.is_none() {
        return Ok(convert.then_some(None));
    }
    Ok(holder_of::<Held<T, C>>(obj, qualified)?.map(Some))
}

// .def("getColorSpaceFromFilepath", [](self, const std::string & filePath) {...},
// "filePath"_a): (color space name, rule index).
static GET_COLOR_SPACE_FROM_FILEPATH: Function = config_method!(
    "getColorSpaceFromFilepath",
    [Param::new("filePath", "str")],
    "tuple",
    |py, n, a, c| {
        let StdString(path) = arg_in!(a, 1, c, StdString);
        let (name, index) = py_result(
            py,
            n.config.color_space_from_filepath_with_index(c_str(&path)),
        )?;
        let items: [Bound<'_, PyAny>; 2] = [
            std_string_to_py(py, &name)?.into_any(),
            index.into_pyobject(py)?.into_any(),
        ];
        Ok(Some(PyTuple::new(py, items)?.into_any().unbind()))
    }
);

static FILEPATH_ONLY_MATCHES_DEFAULT_RULE: Function = config_method!(
    "filepathOnlyMatchesDefaultRule",
    [Param::new("filePath", "str")],
    "bool",
    |py, n, a, c| {
        let path = text(arg_in!(a, 1, c, CharPtr));
        ok(
            py,
            py_result(py, n.config.filepath_only_matches_default_rule(path))?,
        )
    }
);

// ---------------------------------------------------------------------------------------------
// Processors (PyConfig.cpp:723-809, 964-967 @ v2.5.2)

/// The context of a `ConstContextRcPtr` argument, read as it is now.
fn context_value(ctx: &ContextRc) -> Context {
    ctx.read(|c| {
        let mut copy = c.clone();
        copy.set_environment_mode(c.environment_mode());
        copy
    })
}

/// `Config::getProcessor(context, transform, direction)`'s check of the context
/// (Config.cpp:4795-4799 @ v2.5.2).
fn null_context(py: Python<'_>) -> PyErr {
    to_py_err(
        py,
        &ocio::Exception::new("Config::GetProcessor failed. Context is null."),
    )
}

/// `getProcessor(context, transform, direction)` of the library, the context given or null.
fn processor_with(
    py: Python<'_>,
    config: &Config,
    context: Option<&ContextRc>,
    transform: &ocio::Transform,
    dir: TransformDirection,
) -> Out {
    let Some(ctx) = context else {
        return Err(null_context(py));
    };
    let ctx = context_value(ctx);
    processor_out(
        py,
        py_result(py, config.processor_with_context(&ctx, transform, dir))?,
    )
}

/// The processor of a named transform (`NamedTransform::GetTransform`, then the processor of
/// that transform, forward: Config.cpp:4754-4762 @ v2.5.2).
fn named_processor(
    py: Python<'_>,
    config: &Config,
    context: Option<&ContextRc>,
    nt: Option<&NamedTransformRc>,
    dir: i64,
) -> Out {
    let transform = get_transform(py, nt, dir)?;
    let Some(transform) = transform else {
        return Err(to_py_err(
            py,
            &ocio::Exception::new("Config::GetProcessor failed. Transform is null."),
        ));
    };
    let transform = transform.materialize()?;
    processor_with(py, config, context, &transform, TransformDirection::Forward)
}

/// A color space transform from `src` to `dst` (Config.cpp:4714-4723 @ v2.5.2).
fn color_space_transform(src: &[u8], dst: &[u8]) -> ocio::Transform {
    let mut t = ocio::ColorSpaceTransform::new();
    t.set_src(src);
    t.set_dst(dst);
    ocio::Transform::from(t)
}

/// The display view transform `Config::getProcessor` makes of a color space, a display and a
/// view, validated (Config.cpp:4739-4743 @ v2.5.2): its own direction stays forward, the
/// processor's is the one asked for.
fn display_view_transform(
    py: Python<'_>,
    src: &[u8],
    display: &[u8],
    view: &[u8],
) -> PyResult<ocio::Transform> {
    let mut t = ocio::DisplayViewTransform::new();
    t.set_src(src);
    t.set_display(display);
    t.set_view(view);
    py_result(py, t.validate())?;
    Ok(ocio::Transform::from(t))
}

/// A `ConstContextRcPtr` parameter, `ContextRc` or null, or the next overload.
macro_rules! context_param {
    ($a:expr, $i:expr, $convert:expr) => {
        match context_arg(&$a[$i], $convert)? {
            Some(ctx) => ctx,
            None => return Ok(None),
        }
    };
}

/// A `ConstColorSpaceRcPtr` parameter, or the next overload.
macro_rules! color_space_param {
    ($a:expr, $i:expr, $convert:expr) => {
        match holder_arg::<ColorSpaceNode>(&$a[$i], COLOR_SPACE, $convert)? {
            Some(cs) => cs,
            None => return Ok(None),
        }
    };
}

/// A `ConstNamedTransformRcPtr` parameter, or the next overload.
macro_rules! named_transform_param {
    ($a:expr, $i:expr, $convert:expr) => {
        match holder_arg::<NamedTransformNode>(&$a[$i], NAMED_TRANSFORM, $convert)? {
            Some(nt) => nt,
            None => return Ok(None),
        }
    };
}

/// A `ConstTransformRcPtr` parameter, or the next overload.
macro_rules! transform_param {
    ($a:expr, $i:expr, $convert:expr) => {
        match crate::py_transform::holder_arg(&$a[$i], TRANSFORM, $convert)? {
            Some(t) => t,
            None => return Ok(None),
        }
    };
}

/// `getProcessor(src, dst)` of two color spaces (Config.cpp:4686-4704 @ v2.5.2).
fn processor_between_spaces(
    py: Python<'_>,
    config: &Config,
    context: Option<&ContextRc>,
    src: Option<ColorSpaceRc>,
    dst: Option<ColorSpaceRc>,
) -> Out {
    let Some(src) = src else {
        return Err(to_py_err(
            py,
            &ocio::Exception::new("Config::GetProcessor failed. Source color space is null."),
        ));
    };
    let Some(dst) = dst else {
        return Err(to_py_err(
            py,
            &ocio::Exception::new("Config::GetProcessor failed. Destination color space is null."),
        ));
    };
    // Each name in its own statement: `src` and `dst` may be the same color space, whose lock
    // two guards alive at once would take twice.
    let src = src.lock().cs.name().to_vec();
    let dst = dst.lock().cs.name().to_vec();
    processor_with(
        py,
        config,
        context,
        &color_space_transform(&src, &dst),
        TransformDirection::Forward,
    )
}

/// The config's current context, as a `ConstContextRcPtr` argument would give it.
fn current(config: &Config) -> ContextRc {
    ContextRc::Config(config.current_context())
}

static GET_PROCESSOR: Function = Function {
    name: "getProcessor",
    constructor: false,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[
                SELF,
                Param::new("srcColorSpace", COLOR_SPACE),
                Param::new("dstColorSpace", COLOR_SPACE),
            ],
            ret: PROCESSOR,
            call: |py, o, a, c| {
                let rc = this_shared!(o, a, ConfigNode);
                let src = color_space_param!(a, 1, c);
                let dst = color_space_param!(a, 2, c);
                with_config(&rc, |n| {
                    let ctx = current(&n.config);
                    processor_between_spaces(py, &n.config, Some(&ctx), src, dst)
                })
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param::new("context", CONTEXT),
                Param::new("srcColorSpace", COLOR_SPACE),
                Param::new("dstColorSpace", COLOR_SPACE),
            ],
            ret: PROCESSOR,
            call: |py, o, a, c| {
                let rc = this_shared!(o, a, ConfigNode);
                let ctx = context_param!(a, 1, c);
                let src = color_space_param!(a, 2, c);
                let dst = color_space_param!(a, 3, c);
                with_config(&rc, |n| {
                    processor_between_spaces(py, &n.config, ctx.as_ref(), src, dst)
                })
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param::new("srcColorSpaceName", "str"),
                Param::new("dstColorSpaceName", "str"),
            ],
            ret: PROCESSOR,
            call: |py, o, a, c| {
                let rc = this_shared!(o, a, ConfigNode);
                let src = text(arg!(a, 1, c, CharPtr));
                let dst = text(arg!(a, 2, c, CharPtr));
                with_config(&rc, |n| {
                    let ctx = current(&n.config);
                    let t = color_space_transform(&src, &dst);
                    processor_with(py, &n.config, Some(&ctx), &t, TransformDirection::Forward)
                })
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param::new("context", CONTEXT),
                Param::new("srcColorSpaceName", "str"),
                Param::new("dstColorSpaceName", "str"),
            ],
            ret: PROCESSOR,
            call: |py, o, a, c| {
                let rc = this_shared!(o, a, ConfigNode);
                let ctx = context_param!(a, 1, c);
                let src = text(arg!(a, 2, c, CharPtr));
                let dst = text(arg!(a, 3, c, CharPtr));
                with_config(&rc, |n| {
                    let t = color_space_transform(&src, &dst);
                    processor_with(py, &n.config, ctx.as_ref(), &t, TransformDirection::Forward)
                })
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param::new("srcColorSpaceName", "str"),
                Param::new("display", "str"),
                Param::new("view", "str"),
                Param::new("direction", TRANSFORM_DIRECTION),
            ],
            ret: PROCESSOR,
            call: |py, o, a, c| {
                let rc = this_shared!(o, a, ConfigNode);
                let src = text(arg!(a, 1, c, CharPtr));
                let display = text(arg!(a, 2, c, CharPtr));
                let view = text(arg!(a, 3, c, CharPtr));
                let dir = direction(enum_param!(o, a, 4))?;
                with_config(&rc, |n| {
                    let ctx = current(&n.config);
                    let t = display_view_transform(py, &src, &display, &view)?;
                    processor_with(py, &n.config, Some(&ctx), &t, dir)
                })
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param::new("context", CONTEXT),
                Param::new("srcColorSpaceName", "str"),
                Param::new("display", "str"),
                Param::new("view", "str"),
                Param::new("direction", TRANSFORM_DIRECTION),
            ],
            ret: PROCESSOR,
            call: |py, o, a, c| {
                let rc = this_shared!(o, a, ConfigNode);
                let ctx = context_param!(a, 1, c);
                let src = text(arg!(a, 2, c, CharPtr));
                let display = text(arg!(a, 3, c, CharPtr));
                let view = text(arg!(a, 4, c, CharPtr));
                let dir = direction(enum_param!(o, a, 5))?;
                with_config(&rc, |n| {
                    let t = display_view_transform(py, &src, &display, &view)?;
                    processor_with(py, &n.config, ctx.as_ref(), &t, dir)
                })
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param::new("namedTransform", NAMED_TRANSFORM),
                Param::new("direction", TRANSFORM_DIRECTION),
            ],
            ret: PROCESSOR,
            call: |py, o, a, c| {
                let rc = this_shared!(o, a, ConfigNode);
                let nt = named_transform_param!(a, 1, c);
                let dir = enum_param!(o, a, 2);
                with_config(&rc, |n| {
                    let ctx = current(&n.config);
                    named_processor(py, &n.config, Some(&ctx), nt.as_ref(), dir)
                })
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param::new("context", CONTEXT),
                Param::new("namedTransform", NAMED_TRANSFORM),
                Param::new("direction", TRANSFORM_DIRECTION),
            ],
            ret: PROCESSOR,
            call: |py, o, a, c| {
                let rc = this_shared!(o, a, ConfigNode);
                let ctx = context_param!(a, 1, c);
                let nt = named_transform_param!(a, 2, c);
                let dir = enum_param!(o, a, 3);
                with_config(&rc, |n| {
                    named_processor(py, &n.config, ctx.as_ref(), nt.as_ref(), dir)
                })
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param::new("namedTransformName", "str"),
                Param::new("direction", TRANSFORM_DIRECTION),
            ],
            ret: PROCESSOR,
            call: |py, o, a, c| {
                let rc = this_shared!(o, a, ConfigNode);
                let name = text(arg!(a, 1, c, CharPtr));
                let dir = enum_param!(o, a, 2);
                with_config(&rc, |n| {
                    let nt = n.named_transform_by_name(&name);
                    let ctx = current(&n.config);
                    named_processor(py, &n.config, Some(&ctx), nt.as_ref(), dir)
                })
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param::new("context", CONTEXT),
                Param::new("namedTransformName", "str"),
                Param::new("direction", TRANSFORM_DIRECTION),
            ],
            ret: PROCESSOR,
            call: |py, o, a, c| {
                let rc = this_shared!(o, a, ConfigNode);
                let ctx = context_param!(a, 1, c);
                let name = text(arg!(a, 2, c, CharPtr));
                let dir = enum_param!(o, a, 3);
                with_config(&rc, |n| {
                    let nt = n.named_transform_by_name(&name);
                    named_processor(py, &n.config, ctx.as_ref(), nt.as_ref(), dir)
                })
            },
            pos_only: 0,
        },
        Overload {
            params: &[SELF, Param::new("transform", TRANSFORM)],
            ret: PROCESSOR,
            call: |py, o, a, c| {
                let rc = this_shared!(o, a, ConfigNode);
                let t = transform_param!(a, 1, c);
                with_config(&rc, |n| {
                    let ctx = current(&n.config);
                    transform_processor(py, &n.config, Some(&ctx), t.as_ref(), 0)
                })
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param::new("transform", TRANSFORM),
                Param::new("direction", TRANSFORM_DIRECTION),
            ],
            ret: PROCESSOR,
            call: |py, o, a, c| {
                let rc = this_shared!(o, a, ConfigNode);
                let t = transform_param!(a, 1, c);
                let dir = enum_param!(o, a, 2);
                with_config(&rc, |n| {
                    let ctx = current(&n.config);
                    transform_processor(py, &n.config, Some(&ctx), t.as_ref(), dir)
                })
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param::new("context", CONTEXT),
                Param::new("transform", TRANSFORM),
                Param::new("direction", TRANSFORM_DIRECTION),
            ],
            ret: PROCESSOR,
            call: |py, o, a, c| {
                let rc = this_shared!(o, a, ConfigNode);
                let ctx = context_param!(a, 1, c);
                let t = transform_param!(a, 2, c);
                let dir = enum_param!(o, a, 3);
                with_config(&rc, |n| {
                    transform_processor(py, &n.config, ctx.as_ref(), t.as_ref(), dir)
                })
            },
            pos_only: 0,
        },
    ],
};

/// `getProcessor(context, transform, direction)` of a transform node (Config.cpp:4791-4803 @
/// v2.5.2): the context's check, then the transform's.
fn transform_processor(
    py: Python<'_>,
    config: &Config,
    context: Option<&ContextRc>,
    transform: Option<&crate::py_transform::TransformRc>,
    dir: i64,
) -> Out {
    if context.is_none() {
        return Err(null_context(py));
    }
    let Some(transform) = transform else {
        return Err(to_py_err(
            py,
            &ocio::Exception::new("Config::GetProcessor failed. Transform is null."),
        ));
    };
    let dir = direction(dir)?;
    let transform = transform.materialize()?;
    processor_with(py, config, context, &transform, dir)
}

static SET_PROCESSOR_CACHE_FLAGS: Function = config_method!(
    "setProcessorCacheFlags",
    [Param::new("flags", PROCESSOR_CACHE_FLAGS)],
    "None",
    |py, n, a, _| {
        let Some(v) = crate::enums::enum_value(&a[1], PROCESSOR_CACHE_FLAGS)? else {
            return Ok(None);
        };
        n.config
            .set_processor_cache_flags(ocio::ProcessorCacheFlags(v as u32));
        none(py)
    }
);

static CLEAR_PROCESSOR_CACHE: Function =
    config_method!("clearProcessorCache", [], "None", |py, n, _, _| {
        n.config.clear_processor_cache();
        none(py)
    });

// ---------------------------------------------------------------------------------------------
// Archiving and text (PyConfig.cpp:969-1011 @ v2.5.2)

static IS_ARCHIVABLE: Function = config_method!("isArchivable", [], "bool", |py, n, _, _| ok(
    py,
    n.config.is_archivable()
));

// .def("archive", [](ConfigRcPtr & self, const char * filepath) {...}): the file opened
// (created or truncated, binary) before the config is archived into it; a path that can't be
// opened loses what is written. A null path kills the wheel (U-123).
static ARCHIVE: Function = config_method!(
    "archive",
    [Param::new("arg0", "str")],
    "None",
    |py, n, a, c| {
        let path = checked(arg_in!(a, 1, c, CharPtr), "Config.archive(arg0)")?;
        let file = crate::transforms::py_group_transform::open_binary_file(&path)?;
        let mut data = Vec::new();
        let archived = n.config.archive(&mut data);
        if let Some(mut file) = file {
            let _ = std::io::Write::write_all(&mut file, &data);
        }
        py_result(py, archived)?;
        none(py)
    }
);

static STR: Function = config_method!("__str__", [], "str", |py, n, _, _| {
    // `os << *self`: the serialization, nothing when it fails (upstream's exception goes
    // through).
    string_out(py, &py_result(py, n.config.serialize())?)
});

static REPR: Function = config_method!("__repr__", [], "str", |py, n, _, _| {
    let c = &n.config;
    let mut os = b"<Config name=".to_vec();
    os.extend_from_slice(c.name());
    os.extend_from_slice(b", description=");
    os.extend_from_slice(c.description());
    os.extend_from_slice(format!(", ocio_profile_version={}", c.major_version()).as_bytes());
    if c.minor_version() != 0 {
        os.extend_from_slice(format!(".{}", c.minor_version()).as_bytes());
    }
    os.extend_from_slice(
        format!(
            ", active_colorspaces={}, active_displays={}>",
            c.num_color_spaces(),
            c.num_displays()
        )
        .as_bytes(),
    );
    string_out(py, &os)
});

// ---------------------------------------------------------------------------------------------
// The iterators (PyConfig.cpp:1013-1479 @ v2.5.2)

const ENVIRONMENT_VAR_NAME_ITERATOR: &str = q!("Config.EnvironmentVarNameIterator");
const SEARCH_PATH_ITERATOR: &str = q!("Config.SearchPathIterator");
const COLOR_SPACE_NAME_ITERATOR: &str = q!("Config.ColorSpaceNameIterator");
const COLOR_SPACE_ITERATOR: &str = q!("Config.ColorSpaceIterator");
const ACTIVE_COLOR_SPACE_NAME_ITERATOR: &str = q!("Config.ActiveColorSpaceNameIterator");
const ACTIVE_COLOR_SPACE_ITERATOR: &str = q!("Config.ActiveColorSpaceIterator");
const ROLE_NAME_ITERATOR: &str = q!("Config.RoleNameIterator");
const ROLE_COLOR_SPACE_ITERATOR: &str = q!("Config.RoleColorSpaceIterator");
const DISPLAY_ITERATOR: &str = q!("Config.DisplayIterator");
const DISPLAY_ALL_ITERATOR: &str = q!("Config.DisplayAllIterator");
const SHARED_VIEW_ITERATOR: &str = q!("Config.SharedViewIterator");
const VIRTUAL_VIEW_ITERATOR: &str = q!("Config.VirtualViewIterator");
const VIEW_ITERATOR: &str = q!("Config.ViewIterator");
const VIEW_FOR_COLOR_SPACE_ITERATOR: &str = q!("Config.ViewForColorSpaceIterator");
const VIEW_FOR_VIEW_TYPE_ITERATOR: &str = q!("Config.ViewForViewTypeIterator");
const ACTIVE_DISPLAYS_LIST_ITERATOR: &str = q!("Config.ActiveDisplaysListIterator");
const ACTIVE_VIEWS_LIST_ITERATOR: &str = q!("Config.ActiveViewsListIterator");
const LOOK_NAME_ITERATOR: &str = q!("Config.LookNameIterator");
const LOOK_ITERATOR: &str = q!("Config.LookIterator");
const VIEW_TRANSFORM_NAME_ITERATOR: &str = q!("Config.ViewTransformNameIterator");
const VIEW_TRANSFORM_ITERATOR: &str = q!("Config.ViewTransformIterator");
const NAMED_TRANSFORM_NAME_ITERATOR: &str = q!("Config.NamedTransformNameIterator");
const NAMED_TRANSFORM_ITERATOR: &str = q!("Config.NamedTransformIterator");
const ACTIVE_NAMED_TRANSFORM_NAME_ITERATOR: &str = q!("Config.ActiveNamedTransformNameIterator");
const ACTIVE_NAMED_TRANSFORM_ITERATOR: &str = q!("Config.ActiveNamedTransformIterator");

/// What an iterator holds besides its config (`PyIterator`'s arguments).
#[derive(Debug, Clone)]
enum IterArgs {
    None,
    Search(SearchReferenceSpaceType, ColorSpaceVisibility),
    Display(Vec<u8>),
    DisplayColorSpace(Vec<u8>, Vec<u8>),
    TypeDisplay(ViewType, Vec<u8>),
    Type(ViewType),
    Visibility(NamedTransformVisibility),
}

/// An iterator of the config `rc` with `args`.
fn iterator_out(py: Python<'_>, qualified: &str, rc: ConfigRc, args: IterArgs) -> Out {
    Ok(Some(
        new_iterator(py, qualified, Some(Box::new((rc, args))))?.unbind(),
    ))
}

/// The number of entries and the entry at `i` of an iterator kind.
struct IterKind {
    num: fn(&Config, &IterArgs) -> i32,
    item: fn(Python<'_>, &mut ConfigNode, &IterArgs, i32) -> Out,
}

fn search_args(args: &IterArgs) -> (SearchReferenceSpaceType, ColorSpaceVisibility) {
    match args {
        IterArgs::Search(s, v) => (*s, *v),
        _ => (SearchReferenceSpaceType::All, ColorSpaceVisibility::All),
    }
}

fn display_arg(args: &IterArgs) -> &[u8] {
    match args {
        IterArgs::Display(d) | IterArgs::TypeDisplay(_, d) | IterArgs::DisplayColorSpace(d, _) => d,
        _ => b"",
    }
}

fn type_arg(args: &IterArgs) -> ViewType {
    match args {
        IterArgs::Type(t) | IterArgs::TypeDisplay(t, _) => *t,
        _ => ViewType::Shared,
    }
}

fn visibility_arg(args: &IterArgs) -> NamedTransformVisibility {
    match args {
        IterArgs::Visibility(v) => *v,
        _ => NamedTransformVisibility::All,
    }
}

fn cs_arg(args: &IterArgs) -> &[u8] {
    match args {
        IterArgs::DisplayColorSpace(_, cs) => cs,
        _ => b"",
    }
}

/// A string entry of the config.
fn str_item(py: Python<'_>, s: &[u8]) -> Out {
    string_out(py, s)
}

macro_rules! iter_kind {
    ($kind:ident, $num:expr, $item:expr) => {
        static $kind: IterKind = IterKind {
            num: $num,
            item: $item,
        };
    };
}

iter_kind!(ENV_NAMES, |c, _| c.num_environment_vars(), |py, n, _, i| {
    str_item(py, n.config.environment_var_name_by_index(i))
});
iter_kind!(SEARCH_PATHS, |c, _| c.num_search_paths(), |py, n, _, i| {
    str_item(py, c_str(&n.config.search_path_with_index(i)))
});
iter_kind!(
    CS_NAMES,
    |c, a| {
        let (s, v) = search_args(a);
        c.num_color_spaces_with(s, v)
    },
    |py, n, a, i| {
        let (s, v) = search_args(a);
        str_item(py, n.config.color_space_name_by_index_with(s, v, i))
    }
);
iter_kind!(
    CS,
    |c, a| {
        let (s, v) = search_args(a);
        c.num_color_spaces_with(s, v)
    },
    |py, n, a, i| {
        let (s, v) = search_args(a);
        let name = n.config.color_space_name_by_index_with(s, v, i).to_vec();
        let cs = n.color_space_by_name(&name);
        color_space_out(py, cs)
    }
);
iter_kind!(
    ACTIVE_CS_NAMES,
    |c, _| c.num_color_spaces(),
    |py, n, _, i| str_item(py, n.config.color_space_name_by_index(i))
);
iter_kind!(ACTIVE_CS, |c, _| c.num_color_spaces(), |py, n, _, i| {
    let name = n.config.color_space_name_by_index(i).to_vec();
    let cs = n.color_space_by_name(&name);
    color_space_out(py, cs)
});
iter_kind!(ROLE_NAMES, |c, _| c.num_roles(), |py, n, _, i| str_item(
    py,
    n.config.role_name(i)
));
iter_kind!(ROLES, |c, _| c.num_roles(), |py, n, _, i| {
    let items: [Bound<'_, PyAny>; 2] = [
        std_string_to_py(py, n.config.role_name(i))?.into_any(),
        std_string_to_py(py, n.config.role_color_space_by_index(i))?.into_any(),
    ];
    Ok(Some(PyTuple::new(py, items)?.into_any().unbind()))
});
iter_kind!(DISPLAYS, |c, _| c.num_displays(), |py, n, _, i| str_item(
    py,
    n.config.display(i)
));
iter_kind!(DISPLAYS_ALL, |c, _| c.num_displays_all(), |py, n, _, i| {
    str_item(py, n.config.display_all(i))
});
iter_kind!(
    SHARED_VIEWS,
    |c, _| c.num_views_of_type(ViewType::Shared, b""),
    |py, n, _, i| str_item(py, n.config.view_of_type(ViewType::Shared, b"", i))
);
iter_kind!(
    VIRTUAL_VIEWS,
    |c, a| c.virtual_display_num_views(type_arg(a)),
    |py, n, a, i| str_item(py, n.config.virtual_display_view(type_arg(a), i))
);
iter_kind!(VIEWS, |c, a| c.num_views(display_arg(a)), |py, n, a, i| {
    str_item(py, n.config.view(display_arg(a), i))
});
iter_kind!(
    VIEWS_FOR_CS,
    |c, a| c
        .num_views_for_color_space(display_arg(a), cs_arg(a))
        .unwrap_or(0),
    |py, n, a, i| {
        let view = py_result(
            py,
            n.config
                .view_for_color_space(display_arg(a), cs_arg(a), i)
                .map(<[u8]>::to_vec),
        )?;
        str_item(py, &view)
    }
);
iter_kind!(
    VIEWS_FOR_TYPE,
    |c, a| c.num_views_of_type(type_arg(a), display_arg(a)),
    |py, n, a, i| str_item(py, n.config.view_of_type(type_arg(a), display_arg(a), i))
);
iter_kind!(
    ACTIVE_DISPLAYS,
    |c, _| c.num_active_displays(),
    |py, n, _, i| match n.config.active_display(i) {
        Some(s) => str_item(py, s),
        None => none(py),
    }
);
iter_kind!(
    ACTIVE_VIEWS,
    |c, _| c.num_active_views(),
    |py, n, _, i| match n.config.active_view(i) {
        Some(s) => str_item(py, s),
        None => none(py),
    }
);
iter_kind!(LOOK_NAMES, |c, _| c.num_looks(), |py, n, _, i| str_item(
    py,
    n.config.look_name_by_index(i)
));
iter_kind!(LOOKS, |c, _| c.num_looks(), |py, n, _, i| {
    let name = n.config.look_name_by_index(i).to_vec();
    match n.look_by_name(&name) {
        Some(look) => out(py, LOOK, &look),
        None => none(py),
    }
});
iter_kind!(VT_NAMES, |c, _| c.num_view_transforms(), |py, n, _, i| {
    str_item(py, n.config.view_transform_name_by_index(i))
});
iter_kind!(VTS, |c, _| c.num_view_transforms(), |py, n, _, i| {
    let name = n.config.view_transform_name_by_index(i).to_vec();
    match n.view_transform_by_name(&name) {
        Some(vt) => out(py, VIEW_TRANSFORM, &vt),
        None => none(py),
    }
});
iter_kind!(
    NT_NAMES,
    |c, a| c.num_named_transforms_with(visibility_arg(a)),
    |py, n, a, i| str_item(
        py,
        n.config
            .named_transform_name_by_index_with(visibility_arg(a), i)
    )
);
iter_kind!(
    NTS,
    |c, a| c.num_named_transforms_with(visibility_arg(a)),
    |py, n, a, i| {
        let name = n
            .config
            .named_transform_name_by_index_with(visibility_arg(a), i)
            .to_vec();
        match n.named_transform_by_name(&name) {
            Some(nt) => out(py, NAMED_TRANSFORM, &nt),
            None => none(py),
        }
    }
);
iter_kind!(
    ACTIVE_NT_NAMES,
    |c, _| c.num_named_transforms(),
    |py, n, _, i| str_item(py, n.config.named_transform_name_by_index(i))
);
iter_kind!(
    ACTIVE_NTS,
    |c, _| c.num_named_transforms(),
    |py, n, _, i| {
        let name = n.config.named_transform_name_by_index(i).to_vec();
        match n.named_transform_by_name(&name) {
            Some(nt) => out(py, NAMED_TRANSFORM, &nt),
            None => none(py),
        }
    }
);

/// The iterator classes: the class, the kind, the item type as signatures write it.
static ITERATORS: [(&str, &str, &IterKind, &str); 25] = [
    (
        "EnvironmentVarNameIterator",
        ENVIRONMENT_VAR_NAME_ITERATOR,
        &ENV_NAMES,
        "str",
    ),
    (
        "SearchPathIterator",
        SEARCH_PATH_ITERATOR,
        &SEARCH_PATHS,
        "str",
    ),
    (
        "ColorSpaceNameIterator",
        COLOR_SPACE_NAME_ITERATOR,
        &CS_NAMES,
        "str",
    ),
    ("ColorSpaceIterator", COLOR_SPACE_ITERATOR, &CS, COLOR_SPACE),
    (
        "ActiveColorSpaceNameIterator",
        ACTIVE_COLOR_SPACE_NAME_ITERATOR,
        &ACTIVE_CS_NAMES,
        "str",
    ),
    (
        "ActiveColorSpaceIterator",
        ACTIVE_COLOR_SPACE_ITERATOR,
        &ACTIVE_CS,
        COLOR_SPACE,
    ),
    ("RoleNameIterator", ROLE_NAME_ITERATOR, &ROLE_NAMES, "str"),
    (
        "RoleColorSpaceIterator",
        ROLE_COLOR_SPACE_ITERATOR,
        &ROLES,
        "tuple",
    ),
    ("DisplayIterator", DISPLAY_ITERATOR, &DISPLAYS, "str"),
    (
        "DisplayAllIterator",
        DISPLAY_ALL_ITERATOR,
        &DISPLAYS_ALL,
        "str",
    ),
    (
        "SharedViewIterator",
        SHARED_VIEW_ITERATOR,
        &SHARED_VIEWS,
        "str",
    ),
    (
        "VirtualViewIterator",
        VIRTUAL_VIEW_ITERATOR,
        &VIRTUAL_VIEWS,
        "str",
    ),
    ("ViewIterator", VIEW_ITERATOR, &VIEWS, "str"),
    (
        "ViewForColorSpaceIterator",
        VIEW_FOR_COLOR_SPACE_ITERATOR,
        &VIEWS_FOR_CS,
        "str",
    ),
    (
        "ViewForViewTypeIterator",
        VIEW_FOR_VIEW_TYPE_ITERATOR,
        &VIEWS_FOR_TYPE,
        "str",
    ),
    (
        "ActiveDisplaysListIterator",
        ACTIVE_DISPLAYS_LIST_ITERATOR,
        &ACTIVE_DISPLAYS,
        "str",
    ),
    (
        "ActiveViewsListIterator",
        ACTIVE_VIEWS_LIST_ITERATOR,
        &ACTIVE_VIEWS,
        "str",
    ),
    ("LookNameIterator", LOOK_NAME_ITERATOR, &LOOK_NAMES, "str"),
    ("LookIterator", LOOK_ITERATOR, &LOOKS, LOOK),
    (
        "ViewTransformNameIterator",
        VIEW_TRANSFORM_NAME_ITERATOR,
        &VT_NAMES,
        "str",
    ),
    (
        "ViewTransformIterator",
        VIEW_TRANSFORM_ITERATOR,
        &VTS,
        VIEW_TRANSFORM,
    ),
    (
        "NamedTransformNameIterator",
        NAMED_TRANSFORM_NAME_ITERATOR,
        &NT_NAMES,
        "str",
    ),
    (
        "NamedTransformIterator",
        NAMED_TRANSFORM_ITERATOR,
        &NTS,
        NAMED_TRANSFORM,
    ),
    (
        "ActiveNamedTransformNameIterator",
        ACTIVE_NAMED_TRANSFORM_NAME_ITERATOR,
        &ACTIVE_NT_NAMES,
        "str",
    ),
    (
        "ActiveNamedTransformIterator",
        ACTIVE_NAMED_TRANSFORM_ITERATOR,
        &ACTIVE_NTS,
        NAMED_TRANSFORM,
    ),
];

/// The kind of the iterator class of `self` (by the parameter's type).
fn kind_of(qualified: &str) -> &'static IterKind {
    ITERATORS
        .iter()
        .find(|(_, q, _, _)| *q == qualified)
        .map(|(_, _, k, _)| *k)
        .expect("an iterator class of the config")
}

/// The four methods of each iterator class, made at bind time from [`ITERATORS`] (each
/// `Function` is leaked once: 100 small statics).
fn iterator_functions(qualified: &'static str, item_ty: &'static str) -> [&'static Function; 4] {
    fn leak(f: Function) -> &'static Function {
        Box::leak(Box::new(f))
    }
    let self_param = Param::new("self", qualified);
    let params1: &'static [Param] = Box::leak(Box::new([self_param]));
    let params2: &'static [Param] = Box::leak(Box::new([self_param, Param::new("arg0", INT)]));
    let len = leak(Function {
        name: "__len__",
        constructor: false,
        method: true,
        operator: false,
        overloads: Box::leak(Box::new([Overload {
            params: params1,
            ret: "int",
            call: |py, o, a, _| {
                let it = this_iter!(o, a);
                let (rc, args) = it.get().obj::<(ConfigRc, IterArgs)>()?.clone();
                let kind = kind_of(o.params[0].ty);
                with_config(&rc, |n| ok(py, (kind.num)(&n.config, &args)))
            },
            pos_only: 0,
        }])),
    });
    let getitem = leak(Function {
        name: "__getitem__",
        constructor: false,
        method: true,
        operator: false,
        overloads: Box::leak(Box::new([Overload {
            params: params2,
            ret: item_ty,
            call: |py, o, a, convert| {
                let it = this_iter!(o, a);
                let i = arg!(a, 1, convert, i32);
                let (rc, args) = it.get().obj::<(ConfigRc, IterArgs)>()?.clone();
                let kind = kind_of(o.params[0].ty);
                with_config(&rc, |n| {
                    check_index(i, (kind.num)(&n.config, &args))?;
                    (kind.item)(py, n, &args, i)
                })
            },
            pos_only: 0,
        }])),
    });
    let iter = leak(Function {
        name: "__iter__",
        constructor: false,
        method: true,
        operator: false,
        overloads: Box::leak(Box::new([Overload {
            params: params1,
            ret: qualified,
            call: |_, o, a, _| {
                let _ = this_iter!(o, a);
                Ok(Some(a[0].clone().unbind()))
            },
            pos_only: 0,
        }])),
    });
    let next = leak(Function {
        name: "__next__",
        constructor: false,
        method: true,
        operator: false,
        overloads: Box::leak(Box::new([Overload {
            params: params1,
            ret: item_ty,
            call: |py, o, a, _| {
                let it = this_iter!(o, a);
                let (rc, args) = it.get().obj::<(ConfigRc, IterArgs)>()?.clone();
                let kind = kind_of(o.params[0].ty);
                with_config(&rc, |n| {
                    let i = it.get().next_index((kind.num)(&n.config, &args))?;
                    (kind.item)(py, n, &args, i)
                })
            },
            pos_only: 0,
        }])),
    });
    [len, getitem, iter, next]
}

// ---------------------------------------------------------------------------------------------
// The module functions (PyConfig.cpp:1481-1490 @ v2.5.2)

/// The process's current config as Python shares it: `GetCurrentConfig` returns the same
/// object while it lives, and a change made through it is the current config's. The library's
/// current config (`ocio::get_current_config`) is where it comes from the first time, and
/// `SetCurrentConfig` sets both (owner decision 2026-10-22: a Rust holder of the library's
/// `Arc<Config>` keeps a snapshot).
static CURRENT: Mutex<Option<ConfigRc>> = Mutex::new(None);

/// The current config's node (`GetCurrentConfig`), the library's the first time.
pub(crate) fn current_config(py: Python<'_>) -> PyResult<ConfigRc> {
    let mut current = CURRENT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(rc) = current.as_ref() {
        return Ok(rc.clone());
    }
    let config = py_result(py, ocio::config::get_current_config())?;
    let rc = Shared::new(ConfigNode::new(config.clone_as_is()));
    *current = Some(rc.clone());
    Ok(rc)
}

// m.def("GetCurrentConfig", &GetCurrentConfig).
static GET_CURRENT_CONFIG: Function = function(
    "GetCurrentConfig",
    &[Overload {
        params: &[],
        ret: CONFIG,
        call: |py, _, _, _| {
            let rc = current_config(py)?;
            out(py, CONFIG, &rc)
        },
        pos_only: 0,
    }],
);

// m.def("SetCurrentConfig", &SetCurrentConfig, "config"_a): a copy of the config
// (createEditableCopy) becomes the current one; `None` is dereferenced upstream (U-124).
static SET_CURRENT_CONFIG: Function = function(
    "SetCurrentConfig",
    &[Overload {
        params: &[Param::new("config", CONFIG)],
        ret: "None",
        call: |py, _, a, convert| {
            let Some(config) = config_arg(&a[0], convert)? else {
                return Ok(None);
            };
            let config = config.ok_or_else(|| null_object("SetCurrentConfig(config)"))?;
            let copy = with_config(&config, |n| {
                ocio::config::set_current_config(&n.config);
                Ok(n.config.clone())
            })?;
            *CURRENT
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) =
                Some(Shared::new(ConfigNode::new(copy)));
            none(py)
        },
        pos_only: 0,
    }],
);

/// The port's error for a null path given to `ExtractOCIOZArchive`: the wheel makes a
/// `std::string` of a null destination and opens a null archive path (its process dies;
/// docs/improvements.md, U-131).
fn null_path() -> PyErr {
    pyo3::exceptions::PyValueError::new_err(
        "ExtractOCIOZArchive: None, a null pointer the wheel reads as a path (its process dies; \
         docs/improvements.md, U-131)",
    )
}

// m.def("ExtractOCIOZArchive", &ExtractOCIOZArchive) (PyConfig.cpp:1486-1487 @ v2.5.2): two
// `const char *` parameters without names.
static EXTRACT_OCIOZ_ARCHIVE: Function = function(
    "ExtractOCIOZArchive",
    &[Overload {
        params: &[Param::new("arg0", "str"), Param::new("arg1", "str")],
        ret: "None",
        call: |py, _, a, convert| {
            let CharPtr(archive_path) = arg!(a, 0, convert, CharPtr);
            let CharPtr(destination) = arg!(a, 1, convert, CharPtr);
            let (Some(archive_path), Some(destination)) = (archive_path, destination) else {
                return Err(null_path());
            };
            // The GIL stays held, as pybind11's binding holds it.
            py_result(py, ocio::extract_ocioz_archive(archive_path, destination))?;
            Ok(Some(py.None()))
        },
        pos_only: 0,
    }],
);

// m.def("ResolveConfigPath", &ResolveConfigPath) (PyConfig.cpp:1489-1490 @ v2.5.2): a
// `const char *` parameter without a name; a null pointer is the empty string.
static RESOLVE_CONFIG_PATH: Function = function(
    "ResolveConfigPath",
    &[Overload {
        params: &[Param::new("arg0", "str")],
        ret: "str",
        call: |py, _, a, convert| {
            let CharPtr(path) = arg!(a, 0, convert, CharPtr);
            let path = path.unwrap_or_default();
            let resolved = resolve_config_path(&path);
            Ok(Some(std_string_to_py(py, resolved)?.into_any().unbind()))
        },
        pos_only: 0,
    }],
);

/// Port of `bindPyConfig` (src/bindings/python/PyConfig.cpp:83-1491 @ v2.5.2), in its order:
/// the iterator classes nested in the class (89-187), the class's methods (189-1011), the
/// iterators' (1013-1479), the module functions (1481-1490).
pub(crate) fn bind_py_config(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let cls = class(py, CONFIG)?;
    let mut iterator_classes = Vec::new();
    for (name, qualified, _, item) in ITERATORS.iter() {
        iterator_classes.push((nested_class(&cls, "Config", name)?, *qualified, *item));
    }
    def(&cls, &INIT)?;
    def(&cls, &DEEPCOPY)?;
    for f in [
        &CREATE_RAW,
        &CREATE_FROM_ENV,
        &CREATE_FROM_FILE,
        &CREATE_FROM_STREAM,
        &CREATE_FROM_BUILTIN_CONFIG,
        &CREATE_FROM_CONFIG_IO_PROXY,
    ] {
        def_static(&cls, f)?;
    }
    for f in [
        &GET_MAJOR_VERSION,
        &SET_MAJOR_VERSION,
        &GET_MINOR_VERSION,
        &SET_MINOR_VERSION,
        &SET_VERSION,
        &UPGRADE_TO_LATEST_VERSION,
        &VALIDATE,
        &GET_NAME,
        &SET_NAME,
        &GET_FAMILY_SEPARATOR,
        &SET_FAMILY_SEPARATOR,
        &GET_DESCRIPTION,
        &SET_DESCRIPTION,
        &SERIALIZE,
        &GET_CACHE_ID,
        &GET_CURRENT_CONTEXT,
        &ADD_ENVIRONMENT_VAR,
        &GET_ENVIRONMENT_VAR_NAMES,
        &GET_ENVIRONMENT_VAR_DEFAULT,
        &CLEAR_ENVIRONMENT_VARS,
        &SET_ENVIRONMENT_MODE,
        &GET_ENVIRONMENT_MODE,
        &LOAD_ENVIRONMENT,
        &GET_SEARCH_PATH,
        &SET_SEARCH_PATH,
        &GET_SEARCH_PATHS,
        &CLEAR_SEARCH_PATHS,
        &ADD_SEARCH_PATH,
        &GET_WORKING_DIR,
        &SET_WORKING_DIR,
        &GET_CONFIG_IO_PROXY,
        &SET_CONFIG_IO_PROXY,
        &GET_COLOR_SPACES,
        &GET_COLOR_SPACE,
        &GET_COLOR_SPACE_NAMES,
        &GET_CANONICAL_NAME,
        &ADD_COLOR_SPACE,
        &REMOVE_COLOR_SPACE,
        &IS_COLOR_SPACE_LINEAR,
        &IS_COLOR_SPACE_USED,
        &CLEAR_COLOR_SPACES,
        &PARSE_COLOR_SPACE_FROM_STRING,
        &IS_STRICT_PARSING_ENABLED,
        &SET_STRICT_PARSING_ENABLED,
        &SET_INACTIVE_COLOR_SPACES,
        &GET_INACTIVE_COLOR_SPACES,
        &IS_INACTIVE_COLOR_SPACE,
    ] {
        def(&cls, f)?;
    }
    // IdentifyBuiltinColorSpace, IdentifyInterchangeSpace (355-384): chunk 6.8a.
    for f in [
        &SET_ROLE,
        &HAS_ROLE,
        &GET_ROLE_NAMES,
        &GET_ROLES,
        &GET_ROLE_COLOR_SPACE,
        &ADD_SHARED_VIEW,
        &REMOVE_SHARED_VIEW,
        &CLEAR_SHARED_VIEWS,
        &GET_SHARED_VIEWS,
        &GET_DEFAULT_DISPLAY,
        &GET_DISPLAYS,
        &GET_DISPLAYS_ALL,
        &GET_DEFAULT_VIEW,
        &GET_VIEWS,
        &GET_DISPLAY_VIEW_TRANSFORM_NAME,
        &GET_DISPLAY_VIEW_COLOR_SPACE_NAME,
        &GET_DISPLAY_VIEW_LOOKS,
        &GET_DISPLAY_VIEW_RULE,
        &GET_DISPLAY_VIEW_DESCRIPTION,
        &HAS_VIEW,
        &ADD_DISPLAY_VIEW,
        &IS_VIEW_SHARED,
        &ADD_DISPLAY_SHARED_VIEW,
        &REMOVE_DISPLAY_VIEW,
        &CLEAR_DISPLAYS,
    ] {
        def(&cls, f)?;
    }
    def_static(&cls, &ARE_VIEWS_EQUAL)?;
    for f in [
        &HAS_VIRTUAL_VIEW,
        &ADD_VIRTUAL_DISPLAY_VIEW,
        &IS_VIRTUAL_VIEW_SHARED,
        &ADD_VIRTUAL_DISPLAY_SHARED_VIEW,
        &GET_VIRTUAL_DISPLAY_VIEWS,
        &GET_VIRTUAL_DISPLAY_VIEW_TRANSFORM_NAME,
        &GET_VIRTUAL_DISPLAY_VIEW_COLOR_SPACE_NAME,
        &GET_VIRTUAL_DISPLAY_VIEW_LOOKS,
        &GET_VIRTUAL_DISPLAY_VIEW_RULE,
        &GET_VIRTUAL_DISPLAY_VIEW_DESCRIPTION,
        &REMOVE_VIRTUAL_DISPLAY_VIEW,
        &CLEAR_VIRTUAL_DISPLAY,
        &INSTANTIATE_DISPLAY_FROM_MONITOR_NAME,
        &INSTANTIATE_DISPLAY_FROM_ICC_PROFILE,
        &IS_DISPLAY_TEMPORARY,
        &SET_DISPLAY_TEMPORARY,
    ] {
        def(&cls, f)?;
    }
    def_static(&cls, &ARE_VIRTUAL_VIEWS_EQUAL)?;
    for f in [
        &SET_ACTIVE_DISPLAYS,
        &GET_ACTIVE_DISPLAYS,
        &ADD_ACTIVE_DISPLAY,
        &REMOVE_ACTIVE_DISPLAY,
        &CLEAR_ACTIVE_DISPLAYS,
        &GET_NUM_ACTIVE_DISPLAYS,
        &SET_ACTIVE_VIEWS,
        &GET_ACTIVE_VIEWS,
        &ADD_ACTIVE_VIEW,
        &REMOVE_ACTIVE_VIEW,
        &CLEAR_ACTIVE_VIEWS,
        &GET_NUM_ACTIVE_VIEWS,
        &GET_DEFAULT_LUMA_COEFS,
        &SET_DEFAULT_LUMA_COEFS,
        &GET_LOOK,
        &GET_LOOK_NAMES,
        &GET_LOOKS,
        &ADD_LOOK,
        &CLEAR_LOOKS,
        &GET_VIEW_TRANSFORM,
        &GET_VIEW_TRANSFORM_NAMES,
        &GET_VIEW_TRANSFORMS,
        &ADD_VIEW_TRANSFORM,
        &GET_DEFAULT_SCENE_TO_DISPLAY_VIEW_TRANSFORM,
        &GET_DEFAULT_VIEW_TRANSFORM_NAME,
        &SET_DEFAULT_VIEW_TRANSFORM_NAME,
        &CLEAR_VIEW_TRANSFORMS,
        &GET_NAMED_TRANSFORM,
        &GET_NAMED_TRANSFORM_NAMES,
        &GET_NAMED_TRANSFORMS,
        &ADD_NAMED_TRANSFORM,
        &CLEAR_NAMED_TRANSFORMS,
        &GET_VIEWING_RULES,
        &SET_VIEWING_RULES,
        &GET_FILE_RULES,
        &SET_FILE_RULES,
        &GET_COLOR_SPACE_FROM_FILEPATH,
        &FILEPATH_ONLY_MATCHES_DEFAULT_RULE,
        &GET_PROCESSOR,
    ] {
        def(&cls, f)?;
    }
    // GetProcessorToBuiltinColorSpace, GetProcessorFromBuiltinColorSpace,
    // GetProcessorFromConfigs (810-963): chunk 6.8a.
    for f in [
        &SET_PROCESSOR_CACHE_FLAGS,
        &CLEAR_PROCESSOR_CACHE,
        &IS_ARCHIVABLE,
        &ARCHIVE,
        &STR,
        &REPR,
    ] {
        def(&cls, f)?;
    }
    for (iterator, qualified, item) in iterator_classes {
        for f in iterator_functions(qualified, item) {
            def(&iterator, f)?;
        }
    }
    crate::pybind11::def_function(m, &GET_CURRENT_CONFIG)?;
    crate::pybind11::def_function(m, &SET_CURRENT_CONFIG)?;
    crate::pybind11::def_function(m, &EXTRACT_OCIOZ_ARCHIVE)?;
    crate::pybind11::def_function(m, &RESOLVE_CONFIG_PATH)?;
    Ok(())
}
