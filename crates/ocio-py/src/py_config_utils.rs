// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! ConfigUtils statics (src/bindings/python/PyConfig.cpp:355-384, 810-963 @ v2.5.2).

use ocio::{Config, Context};
use ocio_ops::open_color_types::{ReferenceSpaceType, TransformDirection};
use pyo3::prelude::*;
use pyo3::types::{PyTuple, PyType};

use super::{CONFIG, ConfigRc, config_arg, context_value, direction, with_config, with_two};
use crate::dispatch::{CharPtr, Function, Overload, Param};
use crate::py_context::{CONTEXT, context_arg};
use crate::py_open_color_io::{py_result, to_py_err};
use crate::py_processor::{PROCESSOR, processor_out};
use crate::py_transform::{TRANSFORM_DIRECTION, def_static, string_out};
use crate::{arg, enum_param, static_def};

/// The deliberate error for a null shared config the wheel dereferences (U-172).
fn null_config(name: &str) -> PyErr {
    pyo3::exceptions::PyValueError::new_err(format!(
        "Config.{name}: None, a null config the wheel dereferences (its process dies; docs/improvements.md, U-172)"
    ))
}

/// A config argument, including pybind11's null holder conversion.
macro_rules! config {
    ($a:expr, $i:expr, $convert:expr) => {
        match config_arg(&$a[$i], $convert)? {
            Some(v) => v,
            None => return Ok(None),
        }
    };
}

/// A context argument: None stays None, checked at the upstream getProcessor call.
macro_rules! context {
    ($a:expr, $i:expr, $convert:expr) => {
        match context_arg(&$a[$i], $convert)? {
            Some(v) => v.as_ref().map(context_value),
            None => return Ok(None),
        }
    };
}

/// The bytes of a const char pointer (the library treats null names as empty names).
fn text(value: CharPtr) -> Vec<u8> {
    value.0.unwrap_or_default()
}

/// Color-space names reach Config::getColorSpace, whose null string is U-123.
fn color_name(value: CharPtr) -> PyResult<Vec<u8>> {
    value
        .0
        .ok_or_else(|| crate::py_context::null_string("ConfigUtils color-space name"))
}

/// A pair of configs, synchronized and locked in the binding's shared order (D-9).
fn with_pair<T>(
    src: Option<&ConfigRc>,
    dst: Option<&ConfigRc>,
    name: &str,
    f: impl FnOnce(&Config, &Config) -> PyResult<T>,
) -> PyResult<T> {
    let src = src.ok_or_else(|| null_config(name))?;
    let dst = dst.ok_or_else(|| null_config(name))?;
    with_two(src, dst, f)
}

/// The explicit-context color-space overload looks up the destination before srcConfig.
/// Port of `GetInterchangeRolesForColorSpaceConversion` (ConfigUtils.cpp:78-106 @ v2.5.2).
fn null_source_prefix(
    py: Python<'_>,
    src: Option<&ConfigRc>,
    dst: Option<&ConfigRc>,
    dst_name: &[u8],
) -> PyResult<()> {
    if src.is_none()
        && let Some(dst) = dst
    {
        with_config(dst, |node| {
            if node.config.color_space(dst_name).is_none() {
                return Err(to_py_err(
                    py,
                    &ocio::Exception::new(
                        [
                            b"Could not find destination color space '".as_slice(),
                            dst_name,
                            b"'.",
                        ]
                        .concat(),
                    ),
                ));
            }
            Ok(())
        })?;
    }
    Ok(())
}

/// Validations and, for a display-view overload, the source processor that run before the
/// first dereference of a null destination config. Preserve those defined errors first.
/// Port of `Config::GetProcessorFromConfigs` (Config.cpp:4940-5145 @ v2.5.2).
#[allow(clippy::too_many_arguments)]
fn null_destination_prefix(
    py: Python<'_>,
    src: Option<&ConfigRc>,
    dst: Option<&ConfigRc>,
    src_name: &[u8],
    interchange: Option<&[u8]>,
    context: Option<&Context>,
    display: bool,
    direction: TransformDirection,
) -> PyResult<()> {
    if dst.is_some() {
        return Ok(());
    }
    let Some(src) = src else {
        return Ok(());
    };
    with_config(src, |node| {
        let config = &node.config;
        let Some(mut source) = config.color_space(src_name) else {
            return Err(to_py_err(
                py,
                &ocio::Exception::new(
                    [
                        b"Could not find source color space '".as_slice(),
                        src_name,
                        b"'.",
                    ]
                    .concat(),
                ),
            ));
        };
        let Some(interchange) = interchange else {
            // The display/view overload without explicit interchange names checks the
            // source role before reading the destination config's corresponding role.
            let role = if source.reference_space_type() == ReferenceSpaceType::Scene {
                "aces_interchange"
            } else {
                "cie_xyz_d65_interchange"
            };
            let name = config.role_color_space(role);
            if name.is_empty() {
                return Err(to_py_err(
                    py,
                    &ocio::Exception::new(format!(
                        "The role '{role}' is missing in the source config."
                    )),
                ));
            }
            if config.color_space(name).is_none() {
                return Err(to_py_err(
                    py,
                    &ocio::Exception::new(
                        [
                            b"The role '".as_slice(),
                            role.as_bytes(),
                            b"' refers to color space '",
                            name,
                            b"' that is missing in the source config.",
                        ]
                        .concat(),
                    ),
                ));
            }
            return Ok(());
        };
        let Some(mut exchange) = config.color_space(interchange) else {
            return Err(to_py_err(
                py,
                &ocio::Exception::new(
                    [
                        b"Could not find source interchange color space '".as_slice(),
                        interchange,
                        b"'.",
                    ]
                    .concat(),
                ),
            ));
        };
        if display {
            if direction == TransformDirection::Inverse {
                std::mem::swap(&mut source, &mut exchange);
            }
            let context = context.ok_or_else(|| {
                to_py_err(
                    py,
                    &ocio::Exception::new("Config::GetProcessor failed. Context is null."),
                )
            })?;
            py_result(
                py,
                config.processor_between_color_spaces_with_context(
                    context,
                    Some(source),
                    Some(exchange),
                ),
            )?;
        }
        Ok(())
    })
}

// .def_static("IdentifyBuiltinColorSpace", lambda, ...)
// (PyConfig.cpp:355-364 @ v2.5.2).
static IDENTIFY_BUILTIN: Function = static_def!(
    "IdentifyBuiltinColorSpace",
    [
        Param::new("srcConfig", CONFIG),
        Param::new("builtinConfig", CONFIG),
        Param::new("builtinColorSpaceName", "str")
    ],
    "str",
    |py, _, a, convert| {
        let src = config!(a, 0, convert);
        let builtin = config!(a, 1, convert);
        let name = arg!(a, 2, convert, CharPtr);
        let name = color_name(name)?;
        if src.is_none()
            && let Some(builtin) = &builtin
        {
            // The destination lookup precedes the first dereference of srcConfig.
            with_config(builtin, |node| {
                if node.config.color_space(&name).is_none() {
                    let error = [
                        b"Built-in config does not contain the requested color space: ".as_slice(),
                        &name,
                        b".",
                    ]
                    .concat();
                    return Err(to_py_err(py, &ocio::Exception::new(error)));
                }
                Ok(())
            })?;
        }
        let value = with_pair(
            src.as_ref(),
            builtin.as_ref(),
            "IdentifyBuiltinColorSpace",
            |src, builtin| {
                py_result(
                    py,
                    Config::identify_builtin_color_space(src, builtin, &name).map(<[u8]>::to_vec),
                )
            },
        )?;
        string_out(py, &value)
    }
);

// .def_static("IdentifyInterchangeSpace", lambda returning a copied string tuple)
// (PyConfig.cpp:366-384 @ v2.5.2).
static IDENTIFY_INTERCHANGE: Function = static_def!(
    "IdentifyInterchangeSpace",
    [
        Param::new("srcConfig", CONFIG),
        Param::new("srcColorSpaceName", "str"),
        Param::new("builtinConfig", CONFIG),
        Param::new("builtinColorSpaceName", "str")
    ],
    "tuple[str, str]",
    |py, _, a, convert| {
        let src = config!(a, 0, convert);
        let src_name = arg!(a, 1, convert, CharPtr);
        let builtin = config!(a, 2, convert);
        let builtin_name = arg!(a, 3, convert, CharPtr);
        let src_name = color_name(src_name)?;
        let builtin_name = color_name(builtin_name)?;
        if src.is_none()
            && let Some(builtin) = &builtin
        {
            with_config(builtin, |node| {
                if node.config.color_space(&builtin_name).is_none() {
                    let error = [
                        b"Could not find destination color space '".as_slice(),
                        &builtin_name,
                        b"'.",
                    ]
                    .concat();
                    return Err(to_py_err(py, &ocio::Exception::new(error)));
                }
                Ok(())
            })?;
        }
        let (src, builtin) = with_pair(
            src.as_ref(),
            builtin.as_ref(),
            "IdentifyInterchangeSpace",
            |src, builtin| {
                py_result(
                    py,
                    Config::identify_interchange_space(src, &src_name, builtin, &builtin_name)
                        .map(|(s, b)| (s.to_vec(), b.to_vec())),
                )
            },
        )?;
        let src = crate::dispatch::std_string_to_py(py, &src)?;
        let builtin = crate::dispatch::std_string_to_py(py, &builtin)?;
        Ok(Some(PyTuple::new(py, [src, builtin])?.into_any().unbind()))
    }
);

// .def_static("GetProcessorToBuiltinColorSpace", ...)
// (PyConfig.cpp:810-819 @ v2.5.2).
static TO_BUILTIN: Function = static_def!(
    "GetProcessorToBuiltinColorSpace",
    [
        Param::new("srcConfig", CONFIG),
        Param::new("srcColorSpaceName", "str"),
        Param::new("builtinColorSpaceName", "str")
    ],
    PROCESSOR,
    |py, _, a, convert| {
        let src = config!(a, 0, convert);
        let src_name = arg!(a, 1, convert, CharPtr);
        let builtin_name = arg!(a, 2, convert, CharPtr);
        let src_name = color_name(src_name)?;
        let builtin_name = color_name(builtin_name)?;
        builtin_processor(py, src.as_ref(), &src_name, &builtin_name, false)
    }
);

// .def_static("GetProcessorFromBuiltinColorSpace", ...)
// (PyConfig.cpp:820-829 @ v2.5.2).
static FROM_BUILTIN: Function = static_def!(
    "GetProcessorFromBuiltinColorSpace",
    [
        Param::new("builtinColorSpaceName", "str"),
        Param::new("srcConfig", CONFIG),
        Param::new("srcColorSpaceName", "str")
    ],
    PROCESSOR,
    |py, _, a, convert| {
        let builtin_name = arg!(a, 0, convert, CharPtr);
        let src = config!(a, 1, convert);
        let src_name = arg!(a, 2, convert, CharPtr);
        let builtin_name = color_name(builtin_name)?;
        let src_name = color_name(src_name)?;
        builtin_processor(py, src.as_ref(), &src_name, &builtin_name, true)
    }
);

/// The default built-in config and its named space are checked before srcConfig is used.
/// Port of `GetProcessorToBuiltinCS` (Config.cpp:5147-5208 @ v2.5.2).
fn builtin_processor(
    py: Python<'_>,
    src: Option<&ConfigRc>,
    src_name: &[u8],
    builtin_name: &[u8],
    inverse: bool,
) -> crate::py_transform::Out {
    if src.is_none() {
        let builtin = py_result(py, Config::create_from_file("ocio://default"))?;
        if builtin.color_space(builtin_name).is_none() {
            return Err(to_py_err(
                py,
                &ocio::Exception::new(
                    [
                        b"Built-in config does not contain the requested color space: ".as_slice(),
                        builtin_name,
                        b".",
                    ]
                    .concat(),
                ),
            ));
        }
    }
    let src = src.ok_or_else(|| {
        null_config(if inverse {
            "GetProcessorFromBuiltinColorSpace"
        } else {
            "GetProcessorToBuiltinColorSpace"
        })
    })?;
    let processor = with_config(src, |node| {
        py_result(
            py,
            if inverse {
                Config::processor_from_builtin_color_space(builtin_name, &node.config, src_name)
            } else {
                Config::processor_to_builtin_color_space(&node.config, src_name, builtin_name)
            },
        )
    })?;
    processor_out(py, processor)
}

/// One GetProcessorFromConfigs overload with its exact pybind11 parameter order.
macro_rules! overload {
    ([$($name:literal => $ty:expr),*], $call:expr) => {
        Overload { params: &[$(Param::new($name, $ty)),*], ret: PROCESSOR, call: $call, pos_only: 0 }
    };
}

// The eight .def_static("GetProcessorFromConfigs", lambda) definitions, in source order
// (PyConfig.cpp:830-963 @ v2.5.2). The sixth's dstView/dstDisplay keyword typo is upstream's.
static FROM_CONFIGS: Function = Function {
    name: "GetProcessorFromConfigs",
    constructor: false,
    method: false,
    operator: false,
    overloads: &[
        overload!(["srcConfig" => CONFIG, "srcColorSpaceName" => "str", "dstConfig" => CONFIG, "dstColorSpaceName" => "str"], |py, _, a, convert| {
            let src = config!(a, 0, convert); let sn = arg!(a, 1, convert, CharPtr);
            let dst = config!(a, 2, convert); let dn = arg!(a, 3, convert, CharPtr);
            let sn = color_name(sn)?;
        let dn = color_name(dn)?;
        let p = with_pair(src.as_ref(), dst.as_ref(), "GetProcessorFromConfigs", |s, d| py_result(py, Config::processor_from_configs(s, &sn, d, &dn)))?;
            processor_out(py, p)
        }),
        overload!(["srcContext" => CONTEXT, "srcConfig" => CONFIG, "srcColorSpaceName" => "str", "dstContext" => CONTEXT, "dstConfig" => CONFIG, "dstColorSpaceName" => "str"], |py, _, a, convert| {
            let sc = context!(a, 0, convert); let src = config!(a, 1, convert); let sn = arg!(a, 2, convert, CharPtr);
            let dc = context!(a, 3, convert); let dst = config!(a, 4, convert); let dn = arg!(a, 5, convert, CharPtr);
            let sn = color_name(sn)?;
        let dn = color_name(dn)?;
        null_source_prefix(py, src.as_ref(), dst.as_ref(), &dn)?;
            let p = with_pair(src.as_ref(), dst.as_ref(), "GetProcessorFromConfigs", |s, d| py_result(py, Config::processor_from_configs_with_contexts_optional(sc.as_ref(), s, &sn, dc.as_ref(), d, &dn)))?;
            processor_out(py, p)
        }),
        overload!(["srcConfig" => CONFIG, "srcColorSpaceName" => "str", "srcInterchangeName" => "str", "dstConfig" => CONFIG, "dstColorSpaceName" => "str", "dstInterchangeName" => "str"], |py, _, a, convert| {
            let src = config!(a, 0, convert); let sn = arg!(a, 1, convert, CharPtr); let si = arg!(a, 2, convert, CharPtr);
            let dst = config!(a, 3, convert); let dn = arg!(a, 4, convert, CharPtr); let di = arg!(a, 5, convert, CharPtr);
            let sn = color_name(sn)?;
        let si = color_name(si)?;
        let dn = color_name(dn)?;
        let di = color_name(di)?;
        let p = with_pair(src.as_ref(), dst.as_ref(), "GetProcessorFromConfigs", |s, d| py_result(py, Config::processor_from_configs_with_interchange(s, &sn, &si, d, &dn, &di)))?;
            processor_out(py, p)
        }),
        overload!(["srcContext" => CONTEXT, "srcConfig" => CONFIG, "srcColorSpaceName" => "str", "srcInterchangeName" => "str", "dstContext" => CONTEXT, "dstConfig" => CONFIG, "dstColorSpaceName" => "str", "dstInterchangeName" => "str"], |py, _, a, convert| {
            let sc = context!(a, 0, convert); let src = config!(a, 1, convert); let sn = arg!(a, 2, convert, CharPtr); let si = arg!(a, 3, convert, CharPtr);
            let dc = context!(a, 4, convert); let dst = config!(a, 5, convert); let dn = arg!(a, 6, convert, CharPtr); let di = arg!(a, 7, convert, CharPtr);
            let sn = color_name(sn)?;
        let si = color_name(si)?;
        let dn = color_name(dn)?;
        let di = color_name(di)?;
        null_destination_prefix(py, src.as_ref(), dst.as_ref(), &sn, Some(&si), sc.as_ref(), false, TransformDirection::Forward)?;
            let p = with_pair(src.as_ref(), dst.as_ref(), "GetProcessorFromConfigs", |s, d| py_result(py, Config::processor_from_configs_with_interchange_with_contexts_optional(sc.as_ref(), s, &sn, &si, dc.as_ref(), d, &dn, &di)))?;
            processor_out(py, p)
        }),
        overload!(["srcConfig" => CONFIG, "srcColorSpaceName" => "str", "dstConfig" => CONFIG, "dstDisplay" => "str", "dstView" => "str", "direction" => TRANSFORM_DIRECTION], |py, o, a, convert| {
            let src = config!(a, 0, convert); let sn = arg!(a, 1, convert, CharPtr); let dst = config!(a, 2, convert);
            let display = text(arg!(a, 3, convert, CharPtr)); let view = text(arg!(a, 4, convert, CharPtr)); let dir = direction(enum_param!(o, a, 5))?;
            let sn = color_name(sn)?;
        let p = with_pair(src.as_ref(), dst.as_ref(), "GetProcessorFromConfigs", |s, d| py_result(py, Config::processor_from_configs_to_display_view(s, &sn, d, &display, &view, dir)))?;
            processor_out(py, p)
        }),
        overload!(["srcContext" => CONTEXT, "srcConfig" => CONFIG, "srcColorSpaceName" => "str", "dstContext" => CONTEXT, "dstConfig" => CONFIG, "dstView" => "str", "dstDisplay" => "str", "direction" => TRANSFORM_DIRECTION], |py, o, a, convert| {
            let sc = context!(a, 0, convert); let src = config!(a, 1, convert); let sn = arg!(a, 2, convert, CharPtr);
            let dc = context!(a, 3, convert); let dst = config!(a, 4, convert); let display = text(arg!(a, 5, convert, CharPtr)); let view = text(arg!(a, 6, convert, CharPtr)); let dir = direction(enum_param!(o, a, 7))?;
            let sn = color_name(sn)?;
        null_destination_prefix(py, src.as_ref(), dst.as_ref(), &sn, None, sc.as_ref(), true, dir)?;
            let p = with_pair(src.as_ref(), dst.as_ref(), "GetProcessorFromConfigs", |s, d| py_result(py, Config::processor_from_configs_to_display_view_with_contexts_optional(sc.as_ref(), s, &sn, dc.as_ref(), d, &display, &view, dir)))?;
            processor_out(py, p)
        }),
        overload!(["srcConfig" => CONFIG, "srcColorSpaceName" => "str", "srcInterchangeName" => "str", "dstConfig" => CONFIG, "dstDisplay" => "str", "dstView" => "str", "dstInterchangeName" => "str", "direction" => TRANSFORM_DIRECTION], |py, o, a, convert| {
            let src = config!(a, 0, convert); let sn = arg!(a, 1, convert, CharPtr); let si = arg!(a, 2, convert, CharPtr); let dst = config!(a, 3, convert);
            let display = text(arg!(a, 4, convert, CharPtr)); let view = text(arg!(a, 5, convert, CharPtr)); let di = text(arg!(a, 6, convert, CharPtr)); let dir = direction(enum_param!(o, a, 7))?;
            let sn = color_name(sn)?;
        let si = color_name(si)?;
        let p = with_pair(src.as_ref(), dst.as_ref(), "GetProcessorFromConfigs", |s, d| py_result(py, Config::processor_from_configs_to_display_view_with_interchange(s, &sn, &si, d, &display, &view, &di, dir)))?;
            processor_out(py, p)
        }),
        overload!(["srcContext" => CONTEXT, "srcConfig" => CONFIG, "srcColorSpaceName" => "str", "srcInterchangeName" => "str", "dstContext" => CONTEXT, "dstConfig" => CONFIG, "dstDisplay" => "str", "dstView" => "str", "dstInterchangeName" => "str", "direction" => TRANSFORM_DIRECTION], |py, o, a, convert| {
            let sc = context!(a, 0, convert); let src = config!(a, 1, convert); let sn = arg!(a, 2, convert, CharPtr); let si = arg!(a, 3, convert, CharPtr);
            let dc = context!(a, 4, convert); let dst = config!(a, 5, convert); let display = text(arg!(a, 6, convert, CharPtr)); let view = text(arg!(a, 7, convert, CharPtr)); let di = text(arg!(a, 8, convert, CharPtr)); let dir = direction(enum_param!(o, a, 9))?;
            let sn = color_name(sn)?;
        let si = color_name(si)?;
        null_destination_prefix(py, src.as_ref(), dst.as_ref(), &sn, Some(&si), sc.as_ref(), true, dir)?;
            let p = with_pair(src.as_ref(), dst.as_ref(), "GetProcessorFromConfigs", |s, d| py_result(py, Config::processor_from_configs_to_display_view_with_interchange_with_contexts_optional(sc.as_ref(), s, &sn, &si, dc.as_ref(), d, &display, &view, &di, dir)))?;
            processor_out(py, p)
        }),
    ],
};

/// Port of `bindPyConfig`'s identification statics (PyConfig.cpp:355-384 @ v2.5.2).
pub(super) fn bind_identification(cls: &Bound<'_, PyType>) -> PyResult<()> {
    for f in [&IDENTIFY_BUILTIN, &IDENTIFY_INTERCHANGE] {
        def_static(cls, f)?;
    }
    Ok(())
}

/// Port of `bindPyConfig`'s cross-config processor statics (PyConfig.cpp:810-963 @ v2.5.2).
pub(super) fn bind_processors(cls: &Bound<'_, PyType>) -> PyResult<()> {
    for f in [&TO_BUILTIN, &FROM_BUILTIN, &FROM_CONFIGS] {
        def_static(cls, f)?;
    }
    Ok(())
}
