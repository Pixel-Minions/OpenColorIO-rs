// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `bindPyConfigCompatibilityHelpers` (src/bindings/python/apphelpers/
//! PyConfigCompatibilityHelpers.cpp @ v2.6.0): the submodule `ConfigCompatibilityHelpers`.

use ocio::apphelpers::config_compatibility::check_compatibility;
use ocio_ops::open_color_types::ConfigCompatibility;
use pyo3::prelude::*;

use super::py_color_space_helpers::config_param;
use crate::dispatch::{Function, Overload, Param};
use crate::py_config::{CONFIG, config_arg, with_config};
use crate::py_open_color_io::function;
use crate::py_transform::{ok, outside_enumerators};
use crate::{enum_param, q};

pub(crate) const CONFIG_COMPATIBILITY: &str = q!("ConfigCompatibility");

/// The library's `ConfigCompatibility` of the C++ value `v`: `CONFIG_HDR_DISPLAY_SUPPORT_26`
/// is its only enumerator and value ([dcl.enum]: the values of an enum without a fixed type
/// whose enumerators are all 0 are 0 alone), so any other is U-112's error.
fn config_compatibility(v: i64) -> PyResult<ConfigCompatibility> {
    match v {
        0 => Ok(ConfigCompatibility::HdrDisplaySupport26),
        _ => Err(outside_enumerators(CONFIG_COMPATIBILITY, v)),
    }
}

// .def("CheckCompatibility", &ConfigCompatibilityHelpers::CheckCompatibility,
// "config"_a.none(false), "compatibility"_a.none(false))
// (PyConfigCompatibilityHelpers.cpp:11-14 @ v2.6.0).
static CHECK_COMPATIBILITY: Function = function(
    "CheckCompatibility",
    &[Overload {
        params: &[
            Param::new("config", CONFIG),
            Param::new("compatibility", CONFIG_COMPATIBILITY),
        ],
        ret: "bool",
        call: |py, o, a, convert| {
            let rc = config_param!(a, 0, convert);
            let compatibility = config_compatibility(enum_param!(o, a, 1))?;
            with_config(&rc, |n| {
                ok(py, check_compatibility(&n.config, compatibility))
            })
        },
        pos_only: 0,
    }],
);

/// Port of `bindPyConfigCompatibilityHelpers` (src/bindings/python/apphelpers/
/// PyConfigCompatibilityHelpers.cpp:9-16 @ v2.6.0).
pub(crate) fn bind_py_config_compatibility_helpers(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let sub = crate::pybind11::def_submodule(m, "ConfigCompatibilityHelpers")?;
    crate::pybind11::def_submodule_function(&sub, &CHECK_COMPATIBILITY)?;
    Ok(())
}
