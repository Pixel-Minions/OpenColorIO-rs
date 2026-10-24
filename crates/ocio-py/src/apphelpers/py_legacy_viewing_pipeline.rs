// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyLegacyViewingPipeline` (src/bindings/python/apphelpers/PyLegacyViewingPipeline.cpp @
//! v2.5.2): the class `LegacyViewingPipeline` (chunk 6.7c).
//!
//! **Its transforms.** Upstream's setters keep a copy of the transform given
//! (`createEditableCopy`; the display view transform's copy bypasses its looks, and the
//! pipeline remembers whether the one given did), and the getters return the very copy, which
//! Python can change in place: the same Python object while it lives, and what Python changes
//! through it is the pipeline's. The binding holds each copy as a node ([`TransformRc`]) beside
//! the library's pipeline, which gets them as they are when it reads them
//! ([`LegacyViewingPipeline::set_transforms_as_is`]).

use ocio::{DisplayViewTransform, LegacyViewingPipeline, Transform};
use ocio_ops::utils::string_utils::c_str;
use pyo3::prelude::*;

use super::py_color_space_helpers::config_param;
use crate::dispatch::{Function, NonNullCharPtr, Overload, Param};
use crate::py_config::{CONFIG, config_arg, context_value, with_config};
use crate::py_context::{CONTEXT, context_arg};
use crate::py_open_color_io::py_result;
use crate::py_processor::{PROCESSOR, processor_out};
use crate::py_transform::{
    TRANSFORM, TransformRc, class, create_editable_copy, def, default, holder_arg, none, ok,
    string_out, transform_out, validate_ocio, with,
};
use crate::shared::{Shared, construct};
use crate::{method_def, q, this_shared};

const PIPELINE: &str = q!("LegacyViewingPipeline");
const DISPLAY_VIEW_TRANSFORM: &str = q!("DisplayViewTransform");
const SELF: Param = Param::new("self", PIPELINE);

/// A pipeline as the binding holds it.
#[derive(Debug, Default)]
pub(crate) struct PipelineNode {
    /// The library's pipeline: its looks override, and whether the display view transform
    /// given bypassed its looks (`m_dtOriginalLooksBypass`); its transforms are the nodes
    /// below, which it gets as they are when it is read ([`PipelineNode::current`]).
    pipeline: LegacyViewingPipeline,
    /// `m_displayViewTransform`.
    display_view_transform: Option<TransformRc>,
    /// `m_linearCC`.
    linear_cc: Option<TransformRc>,
    /// `m_colorTimingCC`.
    color_timing_cc: Option<TransformRc>,
    /// `m_channelView`.
    channel_view: Option<TransformRc>,
    /// `m_displayCC`.
    display_cc: Option<TransformRc>,
}

impl PipelineNode {
    /// The transforms in the order upstream's `validate` checks them.
    fn transforms(&self) -> [&Option<TransformRc>; 5] {
        [
            &self.display_view_transform,
            &self.linear_cc,
            &self.color_timing_cc,
            &self.channel_view,
            &self.display_cc,
        ]
    }

    /// The library's pipeline, its transforms the nodes' as they are now. A node that can't be
    /// materialized (a group holding a null transform or itself) is the port's error (U-110,
    /// U-111), where the wheel dereferences or recurses.
    fn current(&mut self) -> PyResult<&LegacyViewingPipeline> {
        let materialize =
            |t: &Option<TransformRc>| t.as_ref().map(TransformRc::materialize).transpose();
        let display_view_transform = match materialize(&self.display_view_transform)? {
            Some(Transform::DisplayView(t)) => Some(t),
            Some(_) => {
                return Err(pyo3::exceptions::PyRuntimeError::new_err(
                    "a DisplayViewTransform holder of another transform",
                ));
            }
            None => None,
        };
        let linear_cc = materialize(&self.linear_cc)?;
        let color_timing_cc = materialize(&self.color_timing_cc)?;
        let channel_view = materialize(&self.channel_view)?;
        let display_cc = materialize(&self.display_cc)?;
        self.pipeline.set_transforms_as_is(
            display_view_transform,
            linear_cc,
            color_timing_cc,
            channel_view,
            display_cc,
        );
        Ok(&self.pipeline)
    }

    /// Port of `LegacyViewingPipelineImpl::validate`'s checks of the transforms
    /// (LegacyViewingPipeline.cpp:137-171 @ v2.5.2), with the directions the binding keeps
    /// outside the enumerators (I-261), which the library's transforms can't hold: each
    /// transform's `validate` ([`validate_ocio`]), its error as "LegacyViewingPipeline is not
    /// valid: <error>". The library checks the rest (a missing display view transform first),
    /// and these again.
    fn validate(&self) -> PyResult<ocio::Result<()>> {
        if self.display_view_transform.is_none() {
            return Ok(Ok(()));
        }
        for t in self.transforms().into_iter().flatten() {
            if let Err(e) = validate_ocio(t)? {
                return Ok(Err(ocio::Exception::new(
                    [
                        b"LegacyViewingPipeline is not valid: ".as_slice(),
                        c_str(e.what()),
                    ]
                    .concat(),
                )));
            }
        }
        Ok(Ok(()))
    }
}

// .def(py::init(&LegacyViewingPipeline::Create)) (PyLegacyViewingPipeline.cpp:16-17 @ v2.5.2).
static INIT: Function = Function {
    name: "__init__",
    constructor: true,
    method: true,
    operator: false,
    overloads: &[Overload {
        params: &[SELF],
        ret: "None",
        call: |py, _, a, _| {
            construct(&a[0], PIPELINE, Shared::new(PipelineNode::default()))?;
            none(py)
        },
        pos_only: 0,
    }],
};

// .def("getDisplayViewTransform", &LegacyViewingPipeline::getDisplayViewTransform)
// (PyLegacyViewingPipeline.cpp:18-19 @ v2.5.2): the pipeline's own copy.
static GET_DISPLAY_VIEW_TRANSFORM: Function = method_def!(
    "getDisplayViewTransform",
    [SELF],
    DISPLAY_VIEW_TRANSFORM,
    |py, o, a, _| {
        let rc = this_shared!(o, a, PipelineNode);
        let t = rc.lock().display_view_transform.clone();
        transform_out(py, t.as_ref())
    }
);

// .def("setDisplayViewTransform", &LegacyViewingPipeline::setDisplayViewTransform)
// (PyLegacyViewingPipeline.cpp:20-21 @ v2.5.2): a copy of the transform, its looks bypassed;
// whether the one given bypassed them is remembered (LegacyViewingPipeline.cpp:33-47 @ v2.5.2).
static SET_DISPLAY_VIEW_TRANSFORM: Function = method_def!(
    "setDisplayViewTransform",
    [SELF, Param::new("arg0", DISPLAY_VIEW_TRANSFORM)],
    "None",
    |py, o, a, convert| {
        let rc = this_shared!(o, a, PipelineNode);
        let Some(dt) = holder_arg(&a[1], DISPLAY_VIEW_TRANSFORM, convert)? else {
            return Ok(None);
        };
        let copy = match dt {
            Some(dt) => {
                let copy = create_editable_copy(py, &dt)?;
                let original = with(&copy, |t: &mut DisplayViewTransform| {
                    let original = t.clone();
                    t.set_looks_bypass(true);
                    original
                })?;
                // The library remembers the looks bypass of the one given.
                rc.lock()
                    .pipeline
                    .set_display_view_transform(Some(&original));
                Some(copy)
            }
            None => {
                rc.lock().pipeline.set_display_view_transform(None);
                None
            }
        };
        rc.lock().display_view_transform = copy;
        none(py)
    }
);

/// A getter of one of the pipeline's transforms: its own copy.
macro_rules! cc_getter {
    ($name:literal, $field:ident) => {
        method_def!($name, [SELF], TRANSFORM, |py, o, a, _| {
            let rc = this_shared!(o, a, PipelineNode);
            let t = rc.lock().$field.clone();
            transform_out(py, t.as_ref())
        })
    };
}

/// A setter of one of the pipeline's transforms: a copy of the transform
/// (`createEditableCopy`), or none for `None`.
macro_rules! cc_setter {
    ($name:literal, $field:ident) => {
        method_def!(
            $name,
            [SELF, Param::new("arg0", TRANSFORM)],
            "None",
            |py, o, a, convert| {
                let rc = this_shared!(o, a, PipelineNode);
                let Some(t) = holder_arg(&a[1], TRANSFORM, convert)? else {
                    return Ok(None);
                };
                let copy = match t {
                    Some(t) => Some(create_editable_copy(py, &t)?),
                    None => None,
                };
                rc.lock().$field = copy;
                none(py)
            }
        )
    };
}

// .def("getLinearCC", ...), .def("setLinearCC", ...), .def("getColorTimingCC", ...),
// .def("setColorTimingCC", ...), .def("getChannelView", ...), .def("setChannelView", ...),
// .def("getDisplayCC", ...), .def("setDisplayCC", ...) (PyLegacyViewingPipeline.cpp:22-37 @
// v2.5.2; LegacyViewingPipeline.cpp:49-112 @ v2.5.2).
static GET_LINEAR_CC: Function = cc_getter!("getLinearCC", linear_cc);
static SET_LINEAR_CC: Function = cc_setter!("setLinearCC", linear_cc);
static GET_COLOR_TIMING_CC: Function = cc_getter!("getColorTimingCC", color_timing_cc);
static SET_COLOR_TIMING_CC: Function = cc_setter!("setColorTimingCC", color_timing_cc);
static GET_CHANNEL_VIEW: Function = cc_getter!("getChannelView", channel_view);
static SET_CHANNEL_VIEW: Function = cc_setter!("setChannelView", channel_view);
static GET_DISPLAY_CC: Function = cc_getter!("getDisplayCC", display_cc);
static SET_DISPLAY_CC: Function = cc_setter!("setDisplayCC", display_cc);

// .def("setLooksOverrideEnabled", &LegacyViewingPipeline::setLooksOverrideEnabled)
// (PyLegacyViewingPipeline.cpp:38-39 @ v2.5.2).
static SET_LOOKS_OVERRIDE_ENABLED: Function = method_def!(
    "setLooksOverrideEnabled",
    [SELF, Param::new("arg0", "bool")],
    "None",
    |py, o, a, convert| {
        let rc = this_shared!(o, a, PipelineNode);
        let v = crate::arg!(a, 1, convert, bool);
        rc.lock().pipeline.set_looks_override_enabled(v);
        none(py)
    }
);

// .def("getLooksOverrideEnabled", ...) (PyLegacyViewingPipeline.cpp:40-41 @ v2.5.2).
static GET_LOOKS_OVERRIDE_ENABLED: Function =
    method_def!("getLooksOverrideEnabled", [SELF], "bool", |py, o, a, _| {
        let rc = this_shared!(o, a, PipelineNode);
        let v = rc.lock().pipeline.looks_override_enabled();
        ok(py, v)
    });

// .def("setLooksOverride", &LegacyViewingPipeline::setLooksOverride, "looks"_a.none(false))
// (PyLegacyViewingPipeline.cpp:42-43 @ v2.5.2).
static SET_LOOKS_OVERRIDE: Function = method_def!(
    "setLooksOverride",
    [SELF, Param::new("looks", "str")],
    "None",
    |py, o, a, convert| {
        let rc = this_shared!(o, a, PipelineNode);
        let NonNullCharPtr(looks) = crate::arg!(a, 1, convert, NonNullCharPtr);
        rc.lock().pipeline.set_looks_override(&looks);
        none(py)
    }
);

// .def("getLooksOverride", ...) (PyLegacyViewingPipeline.cpp:44-45 @ v2.5.2).
static GET_LOOKS_OVERRIDE: Function =
    method_def!("getLooksOverride", [SELF], "str", |py, o, a, _| {
        let rc = this_shared!(o, a, PipelineNode);
        let looks = rc.lock().pipeline.looks_override().to_vec();
        string_out(py, &looks)
    });

fn none_value(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    Ok(py.None().into_bound(py))
}

// .def("getProcessor", [](LegacyViewingPipelineRcPtr & self, const ConstConfigRcPtr & config,
// const ConstContextRcPtr & context) {...}, "config"_a.none(false), "context"_a =
// ConstContextRcPtr()) (PyLegacyViewingPipeline.cpp:46-55 @ v2.5.2): a null context is the
// config's current one.
static GET_PROCESSOR: Function = method_def!(
    "getProcessor",
    [
        SELF,
        Param::new("config", CONFIG),
        Param {
            name: "context",
            ty: CONTEXT,
            default: default("None", none_value),
        }
    ],
    PROCESSOR,
    |py, o, a, convert| {
        let rc = this_shared!(o, a, PipelineNode);
        let config = config_param!(a, 1, convert);
        let Some(context) = context_arg(&a[2], convert)? else {
            return Ok(None);
        };
        let p = with_config(&config, |n| {
            let context = context.as_ref().map(context_value);
            let mut node = rc.lock();
            py_result(py, node.validate()?)?;
            let pipeline = node.current()?;
            py_result(py, pipeline.processor(&n.config, context.as_ref()))
        })?;
        processor_out(py, p)
    }
);

// defRepr(clsLegacyViewingPipeline) (PyLegacyViewingPipeline.cpp:57 @ v2.5.2).
static REPR: Function = method_def!("__repr__", [SELF], "str", |py, o, a, _| {
    let rc = this_shared!(o, a, PipelineNode);
    let text = rc.lock().current()?.to_bytes();
    string_out(py, &text)
});

/// Port of `bindPyLegacyViewingPipeline` (src/bindings/python/apphelpers/
/// PyLegacyViewingPipeline.cpp:10-58 @ v2.5.2), in its order.
pub(crate) fn bind_py_legacy_viewing_pipeline(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let cls = class(m.py(), PIPELINE)?;
    for f in [
        &INIT,
        &GET_DISPLAY_VIEW_TRANSFORM,
        &SET_DISPLAY_VIEW_TRANSFORM,
        &GET_LINEAR_CC,
        &SET_LINEAR_CC,
        &GET_COLOR_TIMING_CC,
        &SET_COLOR_TIMING_CC,
        &GET_CHANNEL_VIEW,
        &SET_CHANNEL_VIEW,
        &GET_DISPLAY_CC,
        &SET_DISPLAY_CC,
        &SET_LOOKS_OVERRIDE_ENABLED,
        &GET_LOOKS_OVERRIDE_ENABLED,
        &SET_LOOKS_OVERRIDE,
        &GET_LOOKS_OVERRIDE,
        &GET_PROCESSOR,
        &REPR,
    ] {
        def(&cls, f)?;
    }
    Ok(())
}
