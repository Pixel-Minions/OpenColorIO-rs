// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyMixingHelpers` (src/bindings/python/apphelpers/PyMixingHelpers.cpp @ v2.5.2): the
//! classes `MixingSlider` and `MixingColorSpaceManager`, and the manager's two iterator classes
//! (chunk 6.7c).
//!
//! **The config.** Upstream's manager holds a `ConstConfigRcPtr`, which Python changes in
//! place: `getProcessor` and `repr` read the config as it is now, the mixing spaces and the
//! color picking role's color space are those of the last `refresh`. The binding holds the
//! config's node beside the library's manager, and gives the manager the config as it is
//! before it reads it ([`MixingColorSpaceManager::set_config_as_is`]).
//!
//! **The slider** is the manager's own (`getSlider`, `reference_internal`): the same Python
//! object while it lives, which keeps the manager alive ([`SliderRef`]).

use std::sync::Arc;

use ocio::MixingColorSpaceManager;
use pyo3::prelude::*;

use super::py_color_space_helpers::config_param;
use crate::dispatch::{Function, NonNullCharPtr, Overload, Param};
use crate::py_config::{
    CONFIG, ConfigRc, cache_id, config_arc, config_arg, direction, with_cache_id,
};
use crate::py_open_color_io::py_result;
use crate::py_processor::{PROCESSOR, processor_out};
use crate::py_transform::{
    DIRECTION_DEFAULT, FLOAT, INT, Out, class, def, nested_class, none, ok, string_out,
};
use crate::py_utils::{check_index, new_iterator};
use crate::shared::{Shared, construct, holder_with, to_py_with};
use crate::{enum_param, method_def, q, this_iter, this_shared};

const MIXING_SLIDER: &str = q!("MixingSlider");
const MANAGER: &str = q!("MixingColorSpaceManager");
const SPACE_ITERATOR: &str = q!("MixingColorSpaceManager.MixingSpaceIterator");
const ENCODING_ITERATOR: &str = q!("MixingColorSpaceManager.MixingEncodingIterator");
const SELF: Param = Param::new("self", MANAGER);
const SLIDER_SELF: Param = Param::new("self", MIXING_SLIDER);

/// A manager as the binding holds it: the library's, and the node of its config.
#[derive(Debug)]
pub(crate) struct MixNode {
    manager: MixingColorSpaceManager,
    config: ConfigRc,
}

type MixRc = Shared<MixNode>;

/// `f` of the library's manager of `rc`, its config the node's as it is now (read before the
/// manager is locked: the config's lock can wait for another thread's call). `f` gets a copy
/// of the manager, made under its lock and released before: the library can run Python code
/// (the config's I/O proxy loading a LUT, a logging function), which may read or change the
/// manager, as the wheel's can.
fn with_current<R>(
    rc: &MixRc,
    f: impl FnOnce(&MixingColorSpaceManager, &ocio::Config) -> PyResult<R>,
) -> PyResult<R> {
    loop {
        let config = rc.lock().config.clone();
        let arc = config_arc(&config)?;
        let manager = {
            let mut node = rc.lock();
            // Another thread may have refreshed the manager meanwhile.
            if !node.config.same(&config) {
                continue;
            }
            node.manager.set_config_as_is(Arc::clone(&arc));
            node.manager.clone()
        };
        return f(&manager, &arc);
    }
}

// .def(py::init([](ConstConfigRcPtr & config) { return MixingColorSpaceManager::Create(config);
// }), "config"_a.none(false)) (PyMixingHelpers.cpp:61-65 @ v2.5.2): a color picking role naming
// no color space is U-152.
static INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    operator: false,
    overloads: &[Overload {
        params: &[SELF, Param::new("config", CONFIG)],
        ret: "None",
        call: |py, _, a, convert| {
            let rc = config_param!(a, 1, convert);
            let manager = py_result(py, MixingColorSpaceManager::new(config_arc(&rc)?))?;
            construct(
                &a[0],
                MANAGER,
                Shared::new(MixNode {
                    manager,
                    config: rc,
                }),
            )?;
            none(py)
        },
        pos_only: 0,
    }],
};

// .def("getMixingSpaces", [](MixingColorSpaceManagerRcPtr & self) { return
// MixingSpaceIterator(self); }) (PyMixingHelpers.cpp:66-69 @ v2.5.2).
static GET_MIXING_SPACES: Function =
    method_def!("getMixingSpaces", [SELF], SPACE_ITERATOR, |py, o, a, _| {
        let rc = this_shared!(o, a, MixNode);
        Ok(Some(
            new_iterator(py, SPACE_ITERATOR, Some(Box::new(rc)))?.unbind(),
        ))
    });

// .def("setSelectedMixingSpaceIdx", &MixingColorSpaceManager::setSelectedMixingSpaceIdx)
// (PyMixingHelpers.cpp:70-71 @ v2.5.2).
static SET_SELECTED_MIXING_SPACE_IDX: Function = method_def!(
    "setSelectedMixingSpaceIdx",
    [SELF, Param::new("arg0", INT)],
    "None",
    |py, o, a, convert| {
        let rc = this_shared!(o, a, MixNode);
        let idx = crate::arg!(a, 1, convert, u64);
        py_result(
            py,
            rc.lock()
                .manager
                .set_selected_mixing_space_idx(idx as usize),
        )?;
        none(py)
    }
);

// .def("setSelectedMixingSpace", ..., "mixingSpace"_a.none(false)) (PyMixingHelpers.cpp:72-74
// @ v2.5.2).
static SET_SELECTED_MIXING_SPACE: Function = method_def!(
    "setSelectedMixingSpace",
    [SELF, Param::new("mixingSpace", "str")],
    "None",
    |py, o, a, convert| {
        let rc = this_shared!(o, a, MixNode);
        let NonNullCharPtr(name) = crate::arg!(a, 1, convert, NonNullCharPtr);
        py_result(py, rc.lock().manager.set_selected_mixing_space(&name))?;
        none(py)
    }
);

// .def("getSelectedMixingSpaceIdx", ...) (PyMixingHelpers.cpp:75-76 @ v2.5.2).
static GET_SELECTED_MIXING_SPACE_IDX: Function =
    method_def!("getSelectedMixingSpaceIdx", [SELF], "int", |py, o, a, _| {
        let rc = this_shared!(o, a, MixNode);
        let idx = rc.lock().manager.selected_mixing_space_idx();
        ok(py, idx)
    });

// .def("isPerceptuallyUniform", ...) (PyMixingHelpers.cpp:77-78 @ v2.5.2).
static IS_PERCEPTUALLY_UNIFORM: Function =
    method_def!("isPerceptuallyUniform", [SELF], "bool", |py, o, a, _| {
        let rc = this_shared!(o, a, MixNode);
        let v = rc.lock().manager.is_perceptually_uniform();
        ok(py, v)
    });

// .def("getMixingEncodings", [](MixingColorSpaceManagerRcPtr & self) { return
// MixingEncodingIterator(self); }) (PyMixingHelpers.cpp:79-82 @ v2.5.2).
static GET_MIXING_ENCODINGS: Function = method_def!(
    "getMixingEncodings",
    [SELF],
    ENCODING_ITERATOR,
    |py, o, a, _| {
        let rc = this_shared!(o, a, MixNode);
        Ok(Some(
            new_iterator(py, ENCODING_ITERATOR, Some(Box::new(rc)))?.unbind(),
        ))
    }
);

// .def("setSelectedMixingEncodingIdx", ...) (PyMixingHelpers.cpp:83-84 @ v2.5.2).
static SET_SELECTED_MIXING_ENCODING_IDX: Function = method_def!(
    "setSelectedMixingEncodingIdx",
    [SELF, Param::new("arg0", INT)],
    "None",
    |py, o, a, convert| {
        let rc = this_shared!(o, a, MixNode);
        let idx = crate::arg!(a, 1, convert, u64);
        py_result(
            py,
            rc.lock()
                .manager
                .set_selected_mixing_encoding_idx(idx as usize),
        )?;
        none(py)
    }
);

// .def("setSelectedMixingEncoding", ..., "mixingEncoding"_a.none(false))
// (PyMixingHelpers.cpp:85-87 @ v2.5.2).
static SET_SELECTED_MIXING_ENCODING: Function = method_def!(
    "setSelectedMixingEncoding",
    [SELF, Param::new("mixingEncoding", "str")],
    "None",
    |py, o, a, convert| {
        let rc = this_shared!(o, a, MixNode);
        let NonNullCharPtr(name) = crate::arg!(a, 1, convert, NonNullCharPtr);
        py_result(py, rc.lock().manager.set_selected_mixing_encoding(&name))?;
        none(py)
    }
);

// .def("getSelectedMixingEncodingIdx", ...) (PyMixingHelpers.cpp:88-89 @ v2.5.2).
static GET_SELECTED_MIXING_ENCODING_IDX: Function = method_def!(
    "getSelectedMixingEncodingIdx",
    [SELF],
    "int",
    |py, o, a, _| {
        let rc = this_shared!(o, a, MixNode);
        let idx = rc.lock().manager.selected_mixing_encoding_idx();
        ok(py, idx)
    }
);

// .def("refresh", &MixingColorSpaceManager::refresh, "config"_a.none(false))
// (PyMixingHelpers.cpp:90-91 @ v2.5.2): the manager of the config now; a color picking role
// naming no color space is U-152 (the manager left as it was).
static REFRESH: Function = method_def!(
    "refresh",
    [SELF, Param::new("config", CONFIG)],
    "None",
    |py, o, a, convert| {
        let rc = this_shared!(o, a, MixNode);
        let config = config_param!(a, 1, convert);
        let arc = config_arc(&config)?;
        let mut node = rc.lock();
        py_result(py, node.manager.refresh(arc))?;
        node.config = config;
        none(py)
    }
);

// .def("getProcessor", &MixingColorSpaceManager::getProcessor, "workingSpaceName"_a.none(false),
// "displayName"_a.none(false), "viewName"_a.none(false), "direction"_a = TRANSFORM_DIR_FORWARD)
// (PyMixingHelpers.cpp:92-97 @ v2.5.2): a direction outside the enumerators is U-112.
static GET_PROCESSOR: Function = method_def!(
    "getProcessor",
    [
        SELF,
        Param::new("workingSpaceName", "str"),
        Param::new("displayName", "str"),
        Param::new("viewName", "str"),
        DIRECTION_DEFAULT
    ],
    PROCESSOR,
    |py, o, a, convert| {
        let rc = this_shared!(o, a, MixNode);
        let NonNullCharPtr(working) = crate::arg!(a, 1, convert, NonNullCharPtr);
        let NonNullCharPtr(display) = crate::arg!(a, 2, convert, NonNullCharPtr);
        let NonNullCharPtr(view) = crate::arg!(a, 3, convert, NonNullCharPtr);
        let dir = direction(enum_param!(o, a, 4))?;
        let p = with_current(&rc, |m, _| {
            py_result(py, m.processor(&working, &display, &view, dir))
        })?;
        processor_out(py, p)
    }
);

/// The manager's slider (`MixingSlider &`, `reference_internal`): the object registered for
/// it while it lives, else a new one keeping the manager `owner` alive.
fn slider_out(owner: &Bound<'_, PyAny>, rc: MixRc) -> Out {
    let key = rc.key();
    let obj = to_py_with(owner.py(), MIXING_SLIDER, key, || SliderRef {
        _owner: Arc::new(owner.clone().unbind()),
        manager: rc,
    })?;
    Ok(Some(obj.unbind()))
}

// .def("getSlider", (MixingSlider & (MixingColorSpaceManager::*)()) &...::getSlider,
// py::return_value_policy::reference_internal), .def("getSlider", (MixingSlider &
// (MixingColorSpaceManager::*)(float, float)) &...::getSlider,
// "sliderMixingMinEdge"_a.none(false), "sliderMixingMaxEdge"_a.none(false),
// py::return_value_policy::reference_internal) (PyMixingHelpers.cpp:98-110 @ v2.5.2).
static GET_SLIDER: Function = Function {
    name: "getSlider",
    constructor: false,
    method: true,
    operator: false,
    overloads: &[
        Overload {
            params: &[SELF],
            ret: MIXING_SLIDER,
            call: |_, o, a, _| {
                let rc = this_shared!(o, a, MixNode);
                slider_out(&a[0], rc)
            },
            pos_only: 0,
        },
        Overload {
            params: &[
                SELF,
                Param::new("sliderMixingMinEdge", FLOAT),
                Param::new("sliderMixingMaxEdge", FLOAT),
            ],
            ret: MIXING_SLIDER,
            call: |_, o, a, convert| {
                let rc = this_shared!(o, a, MixNode);
                let min = crate::arg!(a, 1, convert, f32);
                let max = crate::arg!(a, 2, convert, f32);
                rc.lock().manager.slider_with_range(min, max);
                slider_out(&a[0], rc)
            },
            pos_only: 0,
        },
    ],
};

// defRepr(clsMixingColorSpaceManager) (PyMixingHelpers.cpp:112 @ v2.5.2): the cache ID of the
// config itself, as upstream's manager reads it from the config it points to.
static REPR: Function = method_def!("__repr__", [SELF], "str", |py, o, a, _| {
    let rc = this_shared!(o, a, MixNode);
    let config = rc.lock().config.clone();
    let id = py_result(py, cache_id(&config)?)?;
    let (text, old_len) = with_current(&rc, |m, config| {
        Ok((
            py_result(py, m.to_bytes())?,
            py_result(py, config.cache_id())?.len(),
        ))
    })?;
    string_out(py, &with_cache_id(&text, old_len, &id))
});

// ---------------------------------------------------------------------------------------------
// The slider

/// What a `MixingSlider` object holds: its manager, whose slider it is, and the manager's
/// Python object, which it keeps alive (`keep_alive<0, 1>`).
#[derive(Debug, Clone)]
pub(crate) struct SliderRef {
    /// Held, never read: it keeps the manager's Python object alive.
    _owner: Arc<Py<PyAny>>,
    manager: MixRc,
}

/// The slider `self` refers to, or the next overload.
macro_rules! this_slider {
    ($o:expr, $a:expr) => {
        match holder_with::<SliderRef>(&$a[0], $o.params[0].ty)? {
            Some(s) => s.manager,
            None => return Ok(None),
        }
    };
}

/// A setter of an edge, `(arg0: float)`.
macro_rules! edge_setter {
    ($name:literal, $set:ident) => {
        method_def!(
            $name,
            [SLIDER_SELF, Param::new("arg0", FLOAT)],
            "None",
            |py, o, a, convert| {
                let rc = this_slider!(o, a);
                let v = crate::arg!(a, 1, convert, f32);
                rc.lock().manager.slider().$set(v);
                none(py)
            }
        )
    };
}

/// A getter of an edge.
macro_rules! edge_getter {
    ($name:literal, $get:ident) => {
        method_def!($name, [SLIDER_SELF], "float", |py, o, a, _| {
            let rc = this_slider!(o, a);
            let v = rc.lock().manager.slider().$get();
            ok(py, f64::from(v))
        })
    };
}

/// A conversion of a value, `(<param>: float) -> float`.
macro_rules! conversion {
    ($name:literal, $param:literal, $f:ident) => {
        method_def!(
            $name,
            [SLIDER_SELF, Param::new($param, FLOAT)],
            "float",
            |py, o, a, convert| {
                let rc = this_slider!(o, a);
                let v = crate::arg!(a, 1, convert, f32);
                let r = rc.lock().manager.slider().$f(v);
                ok(py, f64::from(r))
            }
        )
    };
}

// .def("setSliderMinEdge", ...), .def("getSliderMinEdge", ...), .def("setSliderMaxEdge", ...),
// .def("getSliderMaxEdge", ...), .def("sliderToMixing", ..., "sliderUnits"_a),
// .def("mixingToSlider", ..., "mixingUnits"_a) (PyMixingHelpers.cpp:44-56 @ v2.5.2).
static SET_SLIDER_MIN_EDGE: Function = edge_setter!("setSliderMinEdge", set_slider_min_edge);
static GET_SLIDER_MIN_EDGE: Function = edge_getter!("getSliderMinEdge", slider_min_edge);
static SET_SLIDER_MAX_EDGE: Function = edge_setter!("setSliderMaxEdge", set_slider_max_edge);
static GET_SLIDER_MAX_EDGE: Function = edge_getter!("getSliderMaxEdge", slider_max_edge);
static SLIDER_TO_MIXING: Function = conversion!("sliderToMixing", "sliderUnits", slider_to_mixing);
static MIXING_TO_SLIDER: Function = conversion!("mixingToSlider", "mixingUnits", mixing_to_slider);

// defRepr(clsMixingSlider) (PyMixingHelpers.cpp:58 @ v2.5.2).
static SLIDER_REPR: Function = method_def!("__repr__", [SLIDER_SELF], "str", |py, o, a, _| {
    let rc = this_slider!(o, a);
    let text = rc.lock().manager.slider().to_bytes();
    string_out(py, &text)
});

// ---------------------------------------------------------------------------------------------
// The iterators (PyMixingHelpers.cpp:114-151 @ v2.5.2)

const SPACE_SELF: Param = Param::new("self", SPACE_ITERATOR);
const ENCODING_SELF: Param = Param::new("self", ENCODING_ITERATOR);

/// The manager an iterator of `self` walks, or the next overload.
macro_rules! iter_manager {
    ($o:expr, $a:expr) => {{
        let it = this_iter!($o, $a);
        let rc = it.get().obj::<MixRc>()?.clone();
        (it, rc)
    }};
}

/// `static_cast<int>(it.m_obj->getNumMixingSpaces())`.
fn num_spaces(rc: &MixRc) -> i32 {
    rc.lock().manager.num_mixing_spaces() as i32
}

/// `static_cast<int>(it.m_obj->getNumMixingEncodings())`.
fn num_encodings(rc: &MixRc) -> i32 {
    rc.lock().manager.num_mixing_encodings() as i32
}

/// `getMixingSpaceUIName(i)`, the `int` converted to `size_t` (a negative index wraps, and the
/// library refuses it).
fn space(py: Python<'_>, rc: &MixRc, i: i32) -> Out {
    let name = py_result(
        py,
        rc.lock()
            .manager
            .mixing_space_ui_name(i as usize)
            .map(<[u8]>::to_vec),
    )?;
    string_out(py, &name)
}

/// `getMixingEncodingName(i)`, as [`space`].
fn encoding(py: Python<'_>, rc: &MixRc, i: i32) -> Out {
    let name = py_result(
        py,
        rc.lock()
            .manager
            .mixing_encoding_name(i as usize)
            .map(<[u8]>::to_vec),
    )?;
    string_out(py, &name)
}

// .def("__len__", [](MixingSpaceIterator & it) { return
// static_cast<int>(it.m_obj->getNumMixingSpaces()); }) (PyMixingHelpers.cpp:115-118 @ v2.5.2).
static SPACE_LEN: Function = method_def!("__len__", [SPACE_SELF], "int", |py, o, a, _| {
    let (_, rc) = iter_manager!(o, a);
    ok(py, num_spaces(&rc))
});

// .def("__getitem__", [](MixingSpaceIterator & it, int i) {...}) (PyMixingHelpers.cpp:119-123 @
// v2.5.2).
static SPACE_GETITEM: Function = method_def!(
    "__getitem__",
    [SPACE_SELF, Param::new("arg0", INT)],
    "str",
    |py, o, a, convert| {
        let (_, rc) = iter_manager!(o, a);
        let i = crate::arg!(a, 1, convert, i32);
        check_index(i, num_spaces(&rc))?;
        space(py, &rc, i)
    }
);

// .def("__iter__", ...) (PyMixingHelpers.cpp:124-127 @ v2.5.2).
static SPACE_ITER: Function =
    method_def!("__iter__", [SPACE_SELF], SPACE_ITERATOR, |_, o, a, _| {
        let _ = this_iter!(o, a);
        Ok(Some(a[0].clone().unbind()))
    });

// .def("__next__", [](MixingSpaceIterator & it) {...}) (PyMixingHelpers.cpp:128-132 @ v2.5.2).
static SPACE_NEXT: Function = method_def!("__next__", [SPACE_SELF], "str", |py, o, a, _| {
    let (it, rc) = iter_manager!(o, a);
    let i = it.get().next_index(num_spaces(&rc))?;
    space(py, &rc, i)
});

// .def("__len__", [](MixingEncodingIterator & it) {...}) (PyMixingHelpers.cpp:135-138 @
// v2.5.2).
static ENCODING_LEN: Function = method_def!("__len__", [ENCODING_SELF], "int", |py, o, a, _| {
    let (_, rc) = iter_manager!(o, a);
    ok(py, num_encodings(&rc))
});

// .def("__getitem__", [](MixingEncodingIterator & it, int i) {...}) (PyMixingHelpers.cpp:
// 139-143 @ v2.5.2).
static ENCODING_GETITEM: Function = method_def!(
    "__getitem__",
    [ENCODING_SELF, Param::new("arg0", INT)],
    "str",
    |py, o, a, convert| {
        let (_, rc) = iter_manager!(o, a);
        let i = crate::arg!(a, 1, convert, i32);
        check_index(i, num_encodings(&rc))?;
        encoding(py, &rc, i)
    }
);

// .def("__iter__", ...) (PyMixingHelpers.cpp:144-147 @ v2.5.2).
static ENCODING_ITER: Function = method_def!(
    "__iter__",
    [ENCODING_SELF],
    ENCODING_ITERATOR,
    |_, o, a, _| {
        let _ = this_iter!(o, a);
        Ok(Some(a[0].clone().unbind()))
    }
);

// .def("__next__", [](MixingSpaceIterator & it) { int i =
// it.nextIndex(static_cast<int>(it.m_obj->getNumMixingEncodings())); return
// it.m_obj->getMixingEncodingName(i); }) (PyMixingHelpers.cpp:148-152 @ v2.5.2): upstream's
// lambda takes a `MixingSpaceIterator`, so `next()` of an encoding iterator is refused
// (TypeError), and the method, given a space iterator, walks the encodings with the space
// iterator's index (I-396).
static ENCODING_NEXT: Function = method_def!("__next__", [SPACE_SELF], "str", |py, o, a, _| {
    let (it, rc) = iter_manager!(o, a);
    let i = it.get().next_index(num_encodings(&rc))?;
    encoding(py, &rc, i)
});

/// Port of `bindPyMixingHelpers` (src/bindings/python/apphelpers/PyMixingHelpers.cpp:25-153 @
/// v2.5.2), in its order.
pub(crate) fn bind_py_mixing_helpers(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let slider = class(py, MIXING_SLIDER)?;
    let manager = class(py, MANAGER)?;
    let spaces = nested_class(&manager, "MixingColorSpaceManager", "MixingSpaceIterator")?;
    let encodings = nested_class(
        &manager,
        "MixingColorSpaceManager",
        "MixingEncodingIterator",
    )?;
    for f in [
        &SET_SLIDER_MIN_EDGE,
        &GET_SLIDER_MIN_EDGE,
        &SET_SLIDER_MAX_EDGE,
        &GET_SLIDER_MAX_EDGE,
        &SLIDER_TO_MIXING,
        &MIXING_TO_SLIDER,
        &SLIDER_REPR,
    ] {
        def(&slider, f)?;
    }
    for f in [
        &INIT,
        &GET_MIXING_SPACES,
        &SET_SELECTED_MIXING_SPACE_IDX,
        &SET_SELECTED_MIXING_SPACE,
        &GET_SELECTED_MIXING_SPACE_IDX,
        &IS_PERCEPTUALLY_UNIFORM,
        &GET_MIXING_ENCODINGS,
        &SET_SELECTED_MIXING_ENCODING_IDX,
        &SET_SELECTED_MIXING_ENCODING,
        &GET_SELECTED_MIXING_ENCODING_IDX,
        &REFRESH,
        &GET_PROCESSOR,
        &GET_SLIDER,
        &REPR,
    ] {
        def(&manager, f)?;
    }
    for f in [&SPACE_LEN, &SPACE_GETITEM, &SPACE_ITER, &SPACE_NEXT] {
        def(&spaces, f)?;
    }
    for f in [
        &ENCODING_LEN,
        &ENCODING_GETITEM,
        &ENCODING_ITER,
        &ENCODING_NEXT,
    ] {
        def(&encodings, f)?;
    }
    Ok(())
}
