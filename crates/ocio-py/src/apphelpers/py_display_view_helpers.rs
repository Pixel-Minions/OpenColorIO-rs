// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyDisplayViewHelpers` (src/bindings/python/apphelpers/PyDisplayViewHelpers.cpp @
//! v2.5.2): the submodule `DisplayViewHelpers` (chunk 6.7c).

use ocio::display_view_helpers::{
    add_display_view, identity_processor, processor, remove_display_view,
};
use ocio_ops::utils::string_utils::c_str;
use pyo3::prelude::*;

use super::py_color_space_helpers::config_param;
use crate::dispatch::{Cast, Function, NonNullCharPtr, Overload, Param, StdString};
use crate::py_config::{CONFIG, config_arg, context_value, direction, with_config};
use crate::py_context::{CONTEXT, context_arg};
use crate::py_open_color_io::{function, py_result};
use crate::py_processor::{PROCESSOR, processor_out};
use crate::py_transform::{DIRECTION_DEFAULT, default, none};
use crate::transforms::py_color_space_transform::empty_string;
use crate::{enum_param, q};

const MATRIX_TRANSFORM: &str = q!("MatrixTransform");

fn none_value(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    Ok(py.None().into_bound(py))
}

// .def("GetProcessor", [](const ConstConfigRcPtr & config, const ConstContextRcPtr & context,
// const char * workingName, const char * displayName, const char * viewName, const
// ConstMatrixTransformRcPtr & channelView, TransformDirection direction) {...},
// "config"_a.none(false), "context"_a = ConstContextRcPtr(), "workingSpaceName"_a.none(false),
// "displayName"_a.none(false), "viewName"_a.none(false), "channelView"_a =
// ConstMatrixTransformRcPtr(), "direction"_a = TRANSFORM_DIR_FORWARD)
// (PyDisplayViewHelpers.cpp:12-37 @ v2.5.2): a null context is the config's current one; a
// direction outside the enumerators is U-112 (the wheel's display view transform keeps it, and
// its validation refuses it).
static GET_PROCESSOR: Function = function(
    "GetProcessor",
    &[Overload {
        params: &[
            Param::new("config", CONFIG),
            Param {
                name: "context",
                ty: CONTEXT,
                default: default("None", none_value),
            },
            Param::new("workingSpaceName", "str"),
            Param::new("displayName", "str"),
            Param::new("viewName", "str"),
            Param {
                name: "channelView",
                ty: MATRIX_TRANSFORM,
                default: default("None", none_value),
            },
            DIRECTION_DEFAULT,
        ],
        ret: PROCESSOR,
        call: |py, o, a, convert| {
            let rc = config_param!(a, 0, convert);
            let Some(context) = context_arg(&a[1], convert)? else {
                return Ok(None);
            };
            let NonNullCharPtr(working) = crate::arg!(a, 2, convert, NonNullCharPtr);
            let NonNullCharPtr(display) = crate::arg!(a, 3, convert, NonNullCharPtr);
            let NonNullCharPtr(view) = crate::arg!(a, 4, convert, NonNullCharPtr);
            let Some(channel_view) =
                crate::py_transform::holder_arg(&a[5], MATRIX_TRANSFORM, convert)?
            else {
                return Ok(None);
            };
            let dir = direction(enum_param!(o, a, 6))?;
            let channel_view = match channel_view {
                Some(t) => match t.materialize()? {
                    ocio::Transform::Matrix(m) => Some(m),
                    _ => {
                        return Err(pyo3::exceptions::PyRuntimeError::new_err(
                            "a MatrixTransform holder of another transform",
                        ));
                    }
                },
                None => None,
            };
            let p = with_config(&rc, |n| {
                let context = context.as_ref().map(context_value);
                py_result(
                    py,
                    processor(
                        &n.config,
                        context.as_ref(),
                        &working,
                        &display,
                        &view,
                        channel_view.as_ref(),
                        dir,
                    ),
                )
            })?;
            processor_out(py, p)
        },
        pos_only: 0,
    }],
);

// .def("GetIdentityProcessor", &DisplayViewHelpers::GetIdentityProcessor,
// "config"_a.none(false)) (PyDisplayViewHelpers.cpp:38-40 @ v2.5.2).
static GET_IDENTITY_PROCESSOR: Function = function(
    "GetIdentityProcessor",
    &[Overload {
        params: &[Param::new("config", CONFIG)],
        ret: PROCESSOR,
        call: |py, _, a, convert| {
            let rc = config_param!(a, 0, convert);
            let p = with_config(&rc, |n| py_result(py, identity_processor(&n.config)))?;
            processor_out(py, p)
        },
        pos_only: 0,
    }],
);

// .def("AddDisplayView", [](ConfigRcPtr & config, const std::string & displayName, ...) {
// DisplayViewHelpers::AddDisplayView(config, displayName.c_str(), ...); },
// "config"_a.none(false), "displayName"_a, "viewName"_a, "lookName"_a = std::string(""),
// "colorSpaceName"_a = std::string(""), "colorSpaceFamily"_a = std::string(""),
// "colorSpaceDescription"_a = std::string(""), "colorSpaceCategories"_a = std::string(""),
// "transformFilePath"_a, "connectionColorSpaceName"_a) (PyDisplayViewHelpers.cpp:41-75 @ v2.5.2):
// each string as a C string (up to its first NUL).
static ADD_DISPLAY_VIEW: Function = function(
    "AddDisplayView",
    &[Overload {
        params: &[
            Param::new("config", CONFIG),
            Param::new("displayName", "str"),
            Param::new("viewName", "str"),
            empty_string("lookName"),
            empty_string("colorSpaceName"),
            empty_string("colorSpaceFamily"),
            empty_string("colorSpaceDescription"),
            empty_string("colorSpaceCategories"),
            Param::new("transformFilePath", "str"),
            Param::new("connectionColorSpaceName", "str"),
        ],
        ret: "None",
        call: |py, _, a, convert| {
            let rc = config_param!(a, 0, convert);
            let mut s: Vec<Vec<u8>> = Vec::with_capacity(9);
            for v in &a[1..10] {
                let Some(StdString(v)) = <StdString as Cast>::try_load(v, convert)? else {
                    return Ok(None);
                };
                s.push(c_str(&v).to_vec());
            }
            with_config(&rc, |n| {
                py_result(
                    py,
                    add_display_view(
                        &mut n.config,
                        &s[0],
                        &s[1],
                        &s[2],
                        &s[3],
                        &s[4],
                        &s[5],
                        &s[6],
                        &s[7],
                        &s[8],
                    ),
                )
            })?;
            none(py)
        },
        pos_only: 0,
    }],
);

// .def("RemoveDisplayView", &DisplayViewHelpers::RemoveDisplayView, "config"_a.none(false),
// "displayName"_a.none(false), "viewName"_a.none(false)) (PyDisplayViewHelpers.cpp:76-78 @
// v2.5.2).
static REMOVE_DISPLAY_VIEW: Function = function(
    "RemoveDisplayView",
    &[Overload {
        params: &[
            Param::new("config", CONFIG),
            Param::new("displayName", "str"),
            Param::new("viewName", "str"),
        ],
        ret: "None",
        call: |py, _, a, convert| {
            let rc = config_param!(a, 0, convert);
            let NonNullCharPtr(display) = crate::arg!(a, 1, convert, NonNullCharPtr);
            let NonNullCharPtr(view) = crate::arg!(a, 2, convert, NonNullCharPtr);
            with_config(&rc, |n| {
                py_result(py, remove_display_view(&mut n.config, &display, &view))
            })?;
            none(py)
        },
        pos_only: 0,
    }],
);

/// Port of `bindPyDisplayViewHelpers` (src/bindings/python/apphelpers/
/// PyDisplayViewHelpers.cpp:10-79 @ v2.5.2), in its order.
pub(crate) fn bind_py_display_view_helpers(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let sub = crate::pybind11::def_submodule(m, "DisplayViewHelpers")?;
    for f in [
        &GET_PROCESSOR,
        &GET_IDENTITY_PROCESSOR,
        &ADD_DISPLAY_VIEW,
        &REMOVE_DISPLAY_VIEW,
    ] {
        crate::pybind11::def_submodule_function(&sub, f)?;
    }
    Ok(())
}
