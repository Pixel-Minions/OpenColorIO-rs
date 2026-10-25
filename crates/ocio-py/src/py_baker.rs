// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyBaker` (src/bindings/python/PyBaker.cpp @ v2.5.2): the class `Baker` and its iterator
//! class `FormatIterator` (chunk 6.3i).
//!
//! pybind11 holds a baker by its `BakerRcPtr`. The binding holds it as a [`BakerNode`]: the
//! library's baker without its config, the config as Python holds configs ([`ConfigRc`]), and
//! the buffers of its format metadata's vectors ([`MetadataHeap`]).
//!
//! - **The config.** `setConfig` stores a copy of the config (`createEditableCopy`), which
//!   `getConfig` returns and Python can change; `__deepcopy__` shares it (`Baker::Impl::
//!   operator=` copies the pointer). `bake` bakes with that config itself, through its processor
//!   cache, which keeps the processors between bakes (`Baker::bake_with_config`).
//! - **The metadata.** `getFormatMetadata` is a reference into the baker's metadata
//!   ([`crate::py_format_metadata`]), which the formats write: a change through it shows in what
//!   the baker bakes.
//! - **`None`** for the config, or for the format's name, is a null pointer that the wheel
//!   dereferences, or makes a `std::string` of: its process dies, the port raises its error
//!   (docs/improvements.md, U-201).

use ocio::Baker;
use ocio_ops::utils::string_utils::c_str;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};

use crate::dispatch::{CharPtr, Function, Overload, Param, StdString};
use crate::py_config::{CONFIG, ConfigNode, ConfigRc, config_arg, with_config};
use crate::py_format_metadata::{MetadataHeap, baker_metadata_out};
use crate::py_open_color_io::py_result;
use crate::py_transform::{FORMAT_METADATA, INT, Out, class, def, default, none, ok, string_out};
use crate::py_utils::{check_index, new_iterator};
use crate::shared::{Shared, construct, out};
use crate::{arg, method_def, q, static_def, this_iter, this_shared};

pub(crate) const BAKER: &str = q!("Baker");
const FORMAT_ITERATOR: &str = q!("Baker.FormatIterator");
const SELF: Param = Param::new("self", BAKER);
const IT_SELF: Param = Param::new("self", FORMAT_ITERATOR);

/// The iterator class's docstring (PyBaker.cpp:33-39 @ v2.5.2).
const FORMAT_ITERATOR_DOC: &str = "\nIterator on LUT baker Formats.\n\nEach item is a tuple \
                                   containing format name and format extension.\n";

/// A baker as the binding holds it.
#[derive(Debug)]
pub(crate) struct BakerNode {
    /// The library's baker, without config: the config is [`BakerNode::config`].
    pub(crate) baker: Baker,
    /// `m_config`: the copy of the config `setConfig` stores, shared by the baker's copies.
    pub(crate) config: Option<ConfigRc>,
    /// The buffers of the vectors of the metadata's child elements.
    pub(crate) metadata_heap: MetadataHeap,
}

impl BakerNode {
    fn new() -> BakerNode {
        BakerNode {
            baker: Baker::new(),
            config: None,
            metadata_heap: MetadataHeap::default(),
        }
    }
}

/// A shared baker node.
pub(crate) type BakerRc = Shared<BakerNode>;

/// The port's error where the wheel dereferences a null config, or makes a `std::string` of a
/// null format name (docs/improvements.md, U-201).
fn null_argument(what: &str) -> PyErr {
    pyo3::exceptions::PyValueError::new_err(format!(
        "{what}: None, a null pointer the wheel dereferences (its process dies; \
         docs/improvements.md, U-201)"
    ))
}

/// Port of `Baker::setConfig` (src/OpenColorIO/Baker.cpp:93-96 @ v2.5.2): a copy of the
/// config (`createEditableCopy`), its objects written back first.
fn set_config(node: &mut BakerNode, config: &ConfigRc) -> PyResult<()> {
    let copy = with_config(config, |n| Ok(n.config.clone()))?;
    node.config = Some(Shared::new(ConfigNode::new(copy)));
    Ok(())
}

/// Port of `Baker::setFormat` (Baker.cpp:118-139 @ v2.5.2): a null name is U-201.
fn set_format(py: Python<'_>, node: &mut BakerNode, name: CharPtr, what: &str) -> PyResult<()> {
    let name = name.0.ok_or_else(|| null_argument(what))?;
    py_result(py, node.baker.set_format(&name))
}

/// `self->bake(os)` (Baker.cpp:237-414 @ v2.5.2) into bytes: what the format wrote, and the
/// library's error. The baker bakes with its config itself (`m_config`), its objects written
/// back first, as the config's own calls do ([`with_config`]): the processors come from the
/// config's processor cache and stay in it, shared with `getConfig()`'s object and the
/// baker's copies, so a change through an object held from the config, which upstream's cache
/// doesn't see, bakes the processor made before it, as the wheel's does. A Python logging
/// function that calls the config during the bake gets D-9's error, as during `getProcessor`.
fn bake(py: Python<'_>, rc: &BakerRc) -> PyResult<(Vec<u8>, PyResult<()>)> {
    let (baker, config) = {
        let node = rc.lock();
        (node.baker.clone(), node.config.clone())
    };
    let mut text = Vec::new();
    let result = match config {
        Some(config) => with_config(&config, |n| {
            Ok(baker.bake_with_config(&mut n.config, &mut text))
        })?,
        None => baker.bake(&mut text),
    };
    Ok((text, py_result(py, result)))
}

const fn empty_string(name: &'static str) -> Param {
    Param {
        name,
        ty: "str",
        default: default("''", |py| Ok(pyo3::types::PyString::new(py, "").into_any())),
    }
}

const fn minus_one(name: &'static str) -> Param {
    Param {
        name,
        ty: INT,
        default: default("-1", |py| Ok(pyo3::types::PyInt::new(py, -1).into_any())),
    }
}

// .def(py::init(&Baker::Create), DOC(Baker, Create)), .def(py::init([](config, format,
// inputSpace, targetSpace, looks, cubeSize, shaperSpace, shaperSize) {...}), "config"_a,
// "format"_a, "inputSpace"_a, "targetSpace"_a, "looks"_a = DEFAULT->getLooks(), "cubeSize"_a =
// DEFAULT->getCubeSize(), "shaperSpace"_a = DEFAULT->getShaperSpace(), "shaperSize"_a =
// DEFAULT->getShaperSize(), DOC(Baker, Create)) (PyBaker.cpp:43-73 @ v2.5.2).
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
                construct(&a[0], BAKER, Shared::new(BakerNode::new()))?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param::new("config", CONFIG),
                Param::new("format", "str"),
                Param::new("inputSpace", "str"),
                Param::new("targetSpace", "str"),
                empty_string("looks"),
                minus_one("cubeSize"),
                empty_string("shaperSpace"),
                minus_one("shaperSize"),
            ],
            ret: "None",
            call: |py, _, a, convert| {
                let Some(config) = config_arg(&a[1], convert)? else {
                    return Ok(None);
                };
                let StdString(format) = arg!(a, 2, convert, StdString);
                let StdString(input_space) = arg!(a, 3, convert, StdString);
                let StdString(target_space) = arg!(a, 4, convert, StdString);
                let StdString(looks) = arg!(a, 5, convert, StdString);
                let cube_size = arg!(a, 6, convert, i32);
                let StdString(shaper_space) = arg!(a, 7, convert, StdString);
                let shaper_size = arg!(a, 8, convert, i32);
                let config = config.ok_or_else(|| null_argument("Baker(config)"))?;
                let mut p = BakerNode::new();
                set_config(&mut p, &config)?;
                set_format(
                    py,
                    &mut p,
                    CharPtr(Some(c_str(&format).to_vec())),
                    "Baker(format)",
                )?;
                p.baker.set_input_space(&input_space);
                p.baker.set_target_space(&target_space);
                p.baker.set_cube_size(cube_size);
                p.baker.set_shaper_size(shaper_size);
                if !looks.is_empty() {
                    p.baker.set_looks(&looks);
                }
                if !shaper_space.is_empty() {
                    p.baker.set_shaper_space(&shaper_space);
                }
                construct(&a[0], BAKER, Shared::new(p))?;
                none(py)
            },
            pos_only: 0,
        },
    ],
};

// .def("__deepcopy__", [](const ConstBakerRcPtr & self, py::dict) { return
// self->createEditableCopy(); }, "memo"_a) (PyBaker.cpp:75-79 @ v2.5.2): the same settings and
// metadata, sharing the config (Baker.cpp:56-73, 86-91).
static DEEPCOPY: Function = method_def!(
    "__deepcopy__",
    [SELF, Param::new("memo", "dict")],
    BAKER,
    |py, o, a, _| {
        let rc = this_shared!(o, a, BakerNode);
        if !a[1].is_instance_of::<PyDict>() {
            return Ok(None);
        }
        let copy = {
            let node = rc.lock();
            BakerNode {
                baker: node.baker.create_editable_copy(),
                config: node.config.clone(),
                metadata_heap: MetadataHeap::default(),
            }
        };
        out(py, BAKER, &Shared::new(copy))
    }
);

// .def_static("getFormats", []() { return FormatIterator(nullptr); }) (PyBaker.cpp:81-84 @
// v2.5.2).
static GET_FORMATS: Function = static_def!("getFormats", [], FORMAT_ITERATOR, |py, _, _, _| {
    Ok(Some(new_iterator(py, FORMAT_ITERATOR, None)?.unbind()))
});

// .def("getConfig", &Baker::getConfig), .def("setConfig", &Baker::setConfig, "config"_a)
// (PyBaker.cpp:86-89 @ v2.5.2).
static GET_CONFIG: Function = method_def!("getConfig", [SELF], CONFIG, |py, o, a, _| {
    let rc = this_shared!(o, a, BakerNode);
    let config = rc.lock().config.clone();
    match config {
        Some(config) => out(py, CONFIG, &config),
        None => none(py),
    }
});

static SET_CONFIG: Function = method_def!(
    "setConfig",
    [SELF, Param::new("config", CONFIG)],
    "None",
    |py, o, a, convert| {
        let rc = this_shared!(o, a, BakerNode);
        let Some(config) = config_arg(&a[1], convert)? else {
            return Ok(None);
        };
        let config = config.ok_or_else(|| null_argument("Baker.setConfig(config)"))?;
        // The copy is made before the baker is locked: the config's own lock is the only one
        // held while its objects are written back.
        let copy = with_config(&config, |n| Ok(n.config.clone()))?;
        rc.lock().config = Some(Shared::new(ConfigNode::new(copy)));
        none(py)
    }
);

// .def("getFormat", &Baker::getFormat), .def("setFormat", &Baker::setFormat, "formatName"_a)
// (PyBaker.cpp:90-93 @ v2.5.2).
static GET_FORMAT: Function = method_def!("getFormat", [SELF], "str", |py, o, a, _| {
    let rc = this_shared!(o, a, BakerNode);
    let text = rc.lock().baker.format().to_vec();
    string_out(py, &text)
});

static SET_FORMAT: Function = method_def!(
    "setFormat",
    [SELF, Param::new("formatName", "str")],
    "None",
    |py, o, a, convert| {
        let rc = this_shared!(o, a, BakerNode);
        let name = arg!(a, 1, convert, CharPtr);
        set_format(py, &mut rc.lock(), name, "Baker.setFormat(formatName)")?;
        none(py)
    }
);

// .def("getFormatMetadata", (FormatMetadata & (Baker::*)()) &Baker::getFormatMetadata,
// py::return_value_policy::reference_internal) (PyBaker.cpp:94-97 @ v2.5.2).
static GET_FORMAT_METADATA: Function = method_def!(
    "getFormatMetadata",
    [SELF],
    FORMAT_METADATA,
    |py, o, a, _| {
        let rc = this_shared!(o, a, BakerNode);
        baker_metadata_out(py, &a[0], &rc)
    }
);

/// A getter of one `const char *` of the baker.
macro_rules! text_getter {
    ($name:literal, $get:ident) => {
        method_def!($name, [SELF], "str", |py, o, a, _| {
            let rc = this_shared!(o, a, BakerNode);
            let text = rc.lock().baker.$get().to_vec();
            string_out(py, &text)
        })
    };
}

/// A setter of one `const char *` of the baker: a null pointer is the empty string.
macro_rules! text_setter {
    ($name:literal, $param:literal, $set:ident) => {
        method_def!(
            $name,
            [SELF, Param::new($param, "str")],
            "None",
            |py, o, a, convert| {
                let rc = this_shared!(o, a, BakerNode);
                let CharPtr(s) = arg!(a, 1, convert, CharPtr);
                rc.lock().baker.$set(s.unwrap_or_default());
                none(py)
            }
        )
    };
}

/// A getter of one `int` of the baker.
macro_rules! int_getter {
    ($name:literal, $get:ident) => {
        method_def!($name, [SELF], "int", |py, o, a, _| {
            let rc = this_shared!(o, a, BakerNode);
            let v = rc.lock().baker.$get();
            ok(py, v)
        })
    };
}

/// A setter of one `int` of the baker.
macro_rules! int_setter {
    ($name:literal, $param:literal, $set:ident) => {
        method_def!(
            $name,
            [SELF, Param::new($param, INT)],
            "None",
            |py, o, a, convert| {
                let rc = this_shared!(o, a, BakerNode);
                let v = arg!(a, 1, convert, i32);
                rc.lock().baker.$set(v);
                none(py)
            }
        )
    };
}

// .def("getInputSpace", ...), .def("setInputSpace", ..., "inputSpace"_a), .def(
// "getShaperSpace", ...), .def("setShaperSpace", ..., "shaperSpace"_a), .def("getLooks", ...),
// .def("setLooks", ..., "looks"_a), .def("getDisplay", ...), .def("getView", ...)
// (PyBaker.cpp:98-113 @ v2.5.2).
static GET_INPUT_SPACE: Function = text_getter!("getInputSpace", input_space);
static SET_INPUT_SPACE: Function = text_setter!("setInputSpace", "inputSpace", set_input_space);
static GET_SHAPER_SPACE: Function = text_getter!("getShaperSpace", shaper_space);
static SET_SHAPER_SPACE: Function = text_setter!("setShaperSpace", "shaperSpace", set_shaper_space);
static GET_LOOKS: Function = text_getter!("getLooks", looks);
static SET_LOOKS: Function = text_setter!("setLooks", "looks", set_looks);
static GET_DISPLAY: Function = text_getter!("getDisplay", display);
static GET_VIEW: Function = text_getter!("getView", view);

// .def("setDisplayView", &Baker::setDisplayView, "display"_a, "view"_a) (PyBaker.cpp:114-115 @
// v2.5.2): a null pointer for either is upstream's error (Baker.cpp:206-215).
static SET_DISPLAY_VIEW: Function = method_def!(
    "setDisplayView",
    [
        SELF,
        Param::new("display", "str"),
        Param::new("view", "str")
    ],
    "None",
    |py, o, a, convert| {
        let rc = this_shared!(o, a, BakerNode);
        let CharPtr(display) = arg!(a, 1, convert, CharPtr);
        let CharPtr(view) = arg!(a, 2, convert, CharPtr);
        let (Some(display), Some(view)) = (display, view) else {
            return Err(py_result::<()>(
                py,
                Err(ocio::Exception::new("Both display and view must be set.")),
            )
            .unwrap_err());
        };
        rc.lock().baker.set_display_view(display, view);
        none(py)
    }
);

// .def("getTargetSpace", ...), .def("setTargetSpace", ..., "targetSpace"_a), .def(
// "getShaperSize", ...), .def("setShaperSize", ..., "shaperSize"_a), .def("getCubeSize", ...),
// .def("setCubeSize", ..., "cubeSize"_a) (PyBaker.cpp:116-127 @ v2.5.2).
static GET_TARGET_SPACE: Function = text_getter!("getTargetSpace", target_space);
static SET_TARGET_SPACE: Function = text_setter!("setTargetSpace", "targetSpace", set_target_space);
static GET_SHAPER_SIZE: Function = int_getter!("getShaperSize", shaper_size);
static SET_SHAPER_SIZE: Function = int_setter!("setShaperSize", "shaperSize", set_shaper_size);
static GET_CUBE_SIZE: Function = int_getter!("getCubeSize", cube_size);
static SET_CUBE_SIZE: Function = int_setter!("setCubeSize", "cubeSize", set_cube_size);

// .def("bake", [](BakerRcPtr & self, const std::string & fileName) { std::ofstream
// f(fileName.c_str()); self->bake(f); f.close(); }, "fileName"_a), .def("bake",
// [](BakerRcPtr & self) { std::ostringstream os; self->bake(os); return os.str(); })
// (PyBaker.cpp:128-141 @ v2.5.2). The file is opened (created or truncated, in text mode)
// before the baker bakes; what the format wrote before an error stays in it.
static BAKE: Function = Function {
    name: "bake",
    constructor: false,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[SELF, Param::new("fileName", "str")],
            ret: "None",
            call: |py, o, a, convert| {
                let rc = this_shared!(o, a, BakerNode);
                let StdString(file_name) = arg!(a, 1, convert, StdString);
                let file = crate::transforms::py_group_transform::open_text_file(&file_name)?;
                let (text, result) = bake(py, &rc)?;
                if let Some(mut file) = file {
                    let text = if cfg!(windows) {
                        crate::transforms::py_group_transform::crlf(&text)
                    } else {
                        text
                    };
                    let _ = std::io::Write::write_all(&mut file, &text);
                }
                result?;
                none(py)
            },
            pos_only: 0,
        },
        Overload {
            params: &[SELF],
            ret: "str",
            call: |py, o, a, _| {
                let rc = this_shared!(o, a, BakerNode);
                let (text, result) = bake(py, &rc)?;
                result?;
                string_out(py, &text)
            },
            pos_only: 0,
        },
    ],
};

/// `(getFormatNameByIndex(i), getFormatExtensionByIndex(i))`.
fn format(py: Python<'_>, i: i32) -> Out {
    let name = crate::dispatch::std_string_to_py(py, Baker::format_name_by_index(i))?;
    let ext = crate::dispatch::std_string_to_py(py, Baker::format_extension_by_index(i))?;
    Ok(Some(PyTuple::new(py, [name, ext])?.into_any().unbind()))
}

// clsFormatIterator (PyBaker.cpp:143-160 @ v2.5.2).
static IT_LEN: Function = method_def!("__len__", [IT_SELF], "int", |py, o, a, _| {
    let _it = this_iter!(o, a);
    ok(py, Baker::num_formats())
});

static IT_GETITEM: Function = method_def!(
    "__getitem__",
    [IT_SELF, Param::new("arg0", INT)],
    "tuple",
    |py, o, a, convert| {
        let _it = this_iter!(o, a);
        let i = arg!(a, 1, convert, i32);
        check_index(i, Baker::num_formats())?;
        format(py, i)
    }
);

static IT_ITER: Function = method_def!("__iter__", [IT_SELF], FORMAT_ITERATOR, |_, o, a, _| {
    let _it = this_iter!(o, a);
    Ok(Some(a[0].clone().unbind()))
});

static IT_NEXT: Function = method_def!("__next__", [IT_SELF], "tuple", |py, o, a, _| {
    let it = this_iter!(o, a);
    let i = it.get().next_index(Baker::num_formats())?;
    format(py, i)
});

/// `py::class_<FormatIterator>(clsBaker, "FormatIterator", doc)`: the class nested in `Baker`
/// with its docstring, then `_pybind11_conduit_v1_` (as [`crate::py_transform::nested_class`]
/// makes one without a docstring).
fn format_iterator_class<'py>(
    outer: &Bound<'py, pyo3::types::PyType>,
) -> PyResult<Bound<'py, pyo3::types::PyType>> {
    let py = outer.py();
    let cls = crate::pybind11::new_class(
        py,
        "FormatIterator",
        "Baker.FormatIterator",
        &[],
        &[(
            "__doc__",
            pyo3::types::PyString::new(py, FORMAT_ITERATOR_DOC).into_any(),
        )],
    )?;
    cls.setattr("_pybind11_conduit_v1_", crate::enums::conduit(py)?)?;
    outer.setattr("FormatIterator", &cls)?;
    Ok(cls)
}

/// Port of `bindPyBaker` (src/bindings/python/PyBaker.cpp:25-161 @ v2.5.2), in its order: the
/// iterator class nested in the class (33-39), the class's methods (43-141), the iterator's
/// (143-160).
pub(crate) fn bind_py_baker(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let cls = class(m.py(), BAKER)?;
    let format_iterator = format_iterator_class(&cls)?;
    def(&cls, &INIT)?;
    def(&cls, &DEEPCOPY)?;
    crate::py_transform::def_static(&cls, &GET_FORMATS)?;
    for f in [
        &GET_CONFIG,
        &SET_CONFIG,
        &GET_FORMAT,
        &SET_FORMAT,
        &GET_FORMAT_METADATA,
        &GET_INPUT_SPACE,
        &SET_INPUT_SPACE,
        &GET_SHAPER_SPACE,
        &SET_SHAPER_SPACE,
        &GET_LOOKS,
        &SET_LOOKS,
        &GET_DISPLAY,
        &GET_VIEW,
        &SET_DISPLAY_VIEW,
        &GET_TARGET_SPACE,
        &SET_TARGET_SPACE,
        &GET_SHAPER_SIZE,
        &SET_SHAPER_SIZE,
        &GET_CUBE_SIZE,
        &SET_CUBE_SIZE,
        &BAKE,
    ] {
        def(&cls, f)?;
    }
    for f in [&IT_LEN, &IT_GETITEM, &IT_ITER, &IT_NEXT] {
        def(&format_iterator, f)?;
    }
    Ok(())
}
