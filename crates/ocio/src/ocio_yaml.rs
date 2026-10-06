// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! OCIO's YAML reader: a port of the `load` functions of `src/OpenColorIO/OCIOYaml.cpp` @
//! v2.5.2, which read a config's nodes (parsed by [`crate::yaml_cpp`]) into OCIO's objects.
//! The `save` functions (the writer) are WP 3.7's.
//!
//! So far: the typed loaders and their messages, the helpers that report unknown keys, bad
//! values and repeated keys, and the transforms ([`load_transform`]), except ExposureContrast
//! and the grading transforms (Phase 5). The loaders of the
//! config's other objects, and of the descriptions, custom keys and interchange attributes
//! they hold, come with them (WP 3.3j-m).
//!
//! **Errors.** A loader fails with OCIO's `Exception` or with an exception of yaml-cpp, which
//! upstream lets through (a key that isn't a string, a zombie node): [`LoadError`]. Upstream
//! catches both as `std::exception` in the typed loaders and in `OCIOYaml::Read`, and reads
//! their `what()` as a C string there, so a message ends at its first NUL even where yaml-cpp's
//! own `what()` holds bytes past it ([`LoadError::what`]).
//!
//! **Strings are bytes.** Keys, tags and values are the bytes yaml-cpp gives. Where upstream
//! passes a `std::string` on with `c_str()` (to a setter, an enum's `FromString`), the port
//! passes the bytes up to the first NUL.

// The config loader (WP 3.3l-m) calls these; until then only the tests do.
#![allow(dead_code)]

use std::collections::HashSet;

use ocio_ops::exception::Exception;
use ocio_ops::logging::log_warning;
use ocio_ops::open_color_types::{
    Allocation, FixedFunctionStyle, TransformDirection, fixed_function_style_from_string,
};
use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;
use ocio_ops::parse_utils::{
    allocation_from_string, cdl_style_from_string, interpolation_from_string,
    negative_style_from_string, transform_direction_from_string,
};
use ocio_ops::utils::string_utils::c_str;

use crate::transform::Transform;
use crate::transforms::allocation_transform::AllocationTransform;
use crate::transforms::builtin_transform::BuiltinTransform;
use crate::transforms::cdl_transform::CdlTransform;
use crate::transforms::color_space_transform::ColorSpaceTransform;
use crate::transforms::display_view_transform::DisplayViewTransform;
use crate::transforms::exponent_transform::ExponentTransform;
use crate::transforms::exponent_with_linear_transform::ExponentWithLinearTransform;
use crate::transforms::file_transform::FileTransform;
use crate::transforms::fixed_function_transform::FixedFunctionTransform;
use crate::transforms::group_transform::GroupTransform;
use crate::transforms::log_affine_transform::LogAffineTransform;
use crate::transforms::log_camera_transform::LogCameraTransform;
use crate::transforms::log_transform::LogTransform;
use crate::transforms::look_transform::LookTransform;
use crate::transforms::matrix_transform::MatrixTransform;
use crate::transforms::range_transform::{RangeTransform, range_style_from_string};
use crate::yaml_cpp::exceptions::Exception as YamlException;
use crate::yaml_cpp::node::{Node, NodeIter, NodeType};

/// Why loading failed: OCIO's `Exception`, or an exception of yaml-cpp that upstream doesn't
/// catch on the way.
#[derive(Debug, Clone)]
pub(crate) enum LoadError {
    /// `OCIO::Exception`.
    Ocio(Exception),
    /// A `YAML::Exception` (`TypedBadConversion`, `InvalidNode`, `BadSubscript`, ...).
    Yaml(YamlException),
}

impl LoadError {
    /// `what()` as upstream's handlers read it, a `const char *`: up to the first NUL.
    pub(crate) fn what(&self) -> Vec<u8> {
        match self {
            LoadError::Ocio(e) => e.what().to_vec(),
            LoadError::Yaml(e) => c_str(&e.what()).to_vec(),
        }
    }
}

impl From<Exception> for LoadError {
    fn from(e: Exception) -> Self {
        LoadError::Ocio(e)
    }
}

impl From<YamlException> for LoadError {
    fn from(e: YamlException) -> Self {
        LoadError::Yaml(e)
    }
}

pub(crate) type LoadResult<T> = std::result::Result<T, LoadError>;

/// `node.Mark().line + 1`, the line `int` upstream prints (an `int` addition). A zombie
/// throws `InvalidNode`.
fn line_of(node: &Node) -> LoadResult<i32> {
    Ok(node.mark()?.line.wrapping_add(1))
}

/// The message of a typed loader that failed: "At line N, 'Tag' parsing <what> failed with:
/// <the exception's what()>". Reading the node's mark and tag throws for a zombie, and that
/// exception replaces the first, as it leaves upstream's `catch` block.
fn parsing_failed(node: &Node, what: &str, e: &LoadError) -> LoadError {
    let line = match line_of(node) {
        Ok(line) => line,
        Err(err) => return err,
    };
    let tag = match node.tag() {
        Ok(tag) => tag,
        Err(err) => return err.into(),
    };
    let mut os = format!("At line {line}, '").into_bytes();
    os.extend_from_slice(tag);
    os.extend_from_slice(format!("' parsing {what} failed with: ").as_bytes());
    os.extend_from_slice(&e.what());
    LoadError::Ocio(Exception::new(os))
}

/// The node as a `double`.
///
/// Port of `load(const YAML::Node&, double&)` (OCIOYaml.cpp:82-96 @ v2.5.2).
pub(crate) fn load_double(node: &Node) -> LoadResult<f64> {
    node.as_::<f64>()
        .map_err(|e| parsing_failed(node, "double", &e.into()))
}

/// The node as a string.
///
/// Port of `load(const YAML::Node&, std::string&)` (OCIOYaml.cpp:98-112 @ v2.5.2).
pub(crate) fn load_string(node: &Node) -> LoadResult<Vec<u8>> {
    node.as_::<Vec<u8>>()
        .map_err(|e| parsing_failed(node, "string", &e.into()))
}

/// The node as a `std::vector<double>`.
///
/// Port of `load(const YAML::Node&, std::vector<double>&)` (OCIOYaml.cpp:146-160 @ v2.5.2).
pub(crate) fn load_vec_f64(node: &Node) -> LoadResult<Vec<f64>> {
    node.as_::<Vec<f64>>()
        .map_err(|e| parsing_failed(node, "vector<double>", &e.into()))
}

/// The node as a transform direction: a string, then `TransformDirectionFromString`.
///
/// Port of `load(const YAML::Node&, TransformDirection&)` (OCIOYaml.cpp:189-194 @ v2.5.2).
pub(crate) fn load_direction(node: &Node) -> LoadResult<TransformDirection> {
    let s = load_string(node)?;
    Ok(transform_direction_from_string(Some(c_str(&s)))?)
}

/// Logs "At line N, unknown key 'key' in 'Tag'.", the line of the key and the tag of the node
/// that holds it.
///
/// Port of `LogUnknownKeyWarning(const YAML::Node&, const YAML::Node&)` (OCIOYaml.cpp:235-246
/// @ v2.5.2).
pub(crate) fn log_unknown_key_warning(node: &Node, key: &Node) -> LoadResult<()> {
    let key_name = load_string(key)?;
    let mut os = format!("At line {}, unknown key '", line_of(key)?).into_bytes();
    os.extend_from_slice(&key_name);
    os.extend_from_slice(b"' in '");
    os.extend_from_slice(node.tag()?);
    os.extend_from_slice(b"'.");
    log_warning(os);
    Ok(())
}

/// Logs "Unknown key in name: 'key'.".
///
/// Port of `LogUnknownKeyWarning(const std::string&, const YAML::Node&)`
/// (OCIOYaml.cpp:248-257 @ v2.5.2).
pub(crate) fn log_unknown_key_warning_in(name: &[u8], tag: &Node) -> LoadResult<()> {
    let key = load_string(tag)?;
    let mut os = b"Unknown key in ".to_vec();
    os.extend_from_slice(name);
    os.extend_from_slice(b": '");
    os.extend_from_slice(&key);
    os.extend_from_slice(b"'.");
    log_warning(os);
    Ok(())
}

/// The error "At line N, 'Tag' parsing failed: msg", or the yaml-cpp exception that reading
/// the node's mark or tag throws.
///
/// Port of `throwError` (OCIOYaml.cpp:259-268 @ v2.5.2).
pub(crate) fn throw_error(node: &Node, msg: &[u8]) -> LoadError {
    let line = match line_of(node) {
        Ok(line) => line,
        Err(err) => return err,
    };
    let tag = match node.tag() {
        Ok(tag) => tag,
        Err(err) => return err.into(),
    };
    let mut os = format!("At line {line}, '").into_bytes();
    os.extend_from_slice(tag);
    os.extend_from_slice(b"' parsing failed: ");
    os.extend_from_slice(msg);
    LoadError::Ocio(Exception::new(os))
}

/// The error "At line N, the value parsing of the key 'key' from 'nodeName' failed: msg", the
/// line of the key; or the error that reading the key or its mark gives.
///
/// Port of `throwValueError(const std::string&, const YAML::Node&, const std::string&)`
/// (OCIOYaml.cpp:270-283 @ v2.5.2).
pub(crate) fn throw_value_error(node_name: &[u8], key: &Node, msg: &[u8]) -> LoadError {
    let key_name = match load_string(key) {
        Ok(name) => name,
        Err(err) => return err,
    };
    let line = match line_of(key) {
        Ok(line) => line,
        Err(err) => return err,
    };
    let mut os = format!("At line {line}, the value parsing of the key '").into_bytes();
    os.extend_from_slice(&key_name);
    os.extend_from_slice(b"' from '");
    os.extend_from_slice(node_name);
    os.extend_from_slice(b"' failed: ");
    os.extend_from_slice(msg);
    LoadError::Ocio(Exception::new(os))
}

/// Fails on the first key of the map that repeats an earlier one (keys compared as strings):
/// "Key-value pair with key 'key' specified more than once. ", through
/// [`throw_value_error`]. A key that isn't a string throws yaml-cpp's bad conversion.
///
/// Port of `CheckDuplicates` (OCIOYaml.cpp:301-321 @ v2.5.2).
pub(crate) fn check_duplicates(node: &Node) -> LoadResult<()> {
    let mut keyset = HashSet::new();
    for iter in node.iter() {
        let key = iter.first.as_::<Vec<u8>>()?;
        if !keyset.contains(&key) {
            keyset.insert(key);
        } else {
            let mut os = b"Key-value pair with key '".to_vec();
            os.extend_from_slice(&key);
            os.extend_from_slice(b"' specified more than once. ");
            return Err(throw_value_error(node.tag()?, &iter.first, &os));
        }
    }
    Ok(())
}

/// A `LogTransform`: `base` (one number), `direction`, `name`. Unknown keys are reported with
/// the node's tag ("Unknown key in LogTransform: 'key'.").
///
/// Port of `load(const YAML::Node&, LogTransformRcPtr&)` (OCIOYaml.cpp:2877-2923 @ v2.5.2).
fn load_log(node: &Node) -> LoadResult<LogTransform> {
    let mut t = LogTransform::new();

    check_duplicates(node)?;

    for iter in node.iter() {
        let key = iter.first.as_::<Vec<u8>>()?;
        if iter.second.is_null()? || !iter.second.is_defined() {
            continue;
        }
        let value = &iter.second;
        match key.as_slice() {
            b"base" => {
                // Upstream starts `base` at 2.0, which every path replaces or leaves unused.
                let nb = value.size()?;
                let base = if nb == 0 {
                    load_double(value)?
                } else {
                    return Err(Exception::new(format!(
                        "LogTransform parse error, base must be a  single double. Found {nb}."
                    ))
                    .into());
                };
                t.set_base(base);
            }
            b"direction" => t.set_direction(load_direction(value)?),
            b"name" => {
                let name = load_string(value)?;
                t.format_metadata_mut().set_name(Some(c_str(&name)));
            }
            _ => log_unknown_key_warning_in(node.tag()?, &iter.first)?,
        }
    }
    Ok(t)
}

/// A `MatrixTransform`: `matrix` (16 numbers), `offset` (4), `direction`, `name`.
///
/// Port of `load(const YAML::Node&, MatrixTransformRcPtr&)` (OCIOYaml.cpp:3002-3057 @
/// v2.5.2).
fn load_matrix(node: &Node) -> LoadResult<MatrixTransform> {
    let mut t = MatrixTransform::new();

    check_duplicates(node)?;

    for iter in node.iter() {
        let key = iter.first.as_::<Vec<u8>>()?;
        if iter.second.is_null()? || !iter.second.is_defined() {
            continue;
        }
        let value = &iter.second;
        match key.as_slice() {
            b"matrix" => {
                let val = load_vec_f64(value)?;
                let Ok(m44) = <[f64; 16]>::try_from(val.as_slice()) else {
                    let os = format!("'matrix' values must be 16 numbers. Found '{}'.", val.len());
                    return Err(throw_value_error(node.tag()?, &iter.first, os.as_bytes()));
                };
                t.set_matrix(&m44);
            }
            b"offset" => {
                let val = load_vec_f64(value)?;
                let Ok(offset4) = <[f64; 4]>::try_from(val.as_slice()) else {
                    let os = format!("'offset' values must be 4 numbers. Found '{}'.", val.len());
                    return Err(throw_value_error(node.tag()?, &iter.first, os.as_bytes()));
                };
                t.set_offset(&offset4);
            }
            b"direction" => t.set_direction(load_direction(value)?),
            b"name" => {
                let name = load_string(value)?;
                t.format_metadata_mut().set_name(Some(c_str(&name)));
            }
            _ => log_unknown_key_warning(node, &iter.first)?,
        }
    }
    Ok(t)
}

/// A `RangeTransform`: the four bounds (numbers), `style`, `direction`, `name`.
///
/// Port of `load(const YAML::Node&, RangeTransformRcPtr&)` (OCIOYaml.cpp:3091-3151 @ v2.5.2).
fn load_range(node: &Node) -> LoadResult<RangeTransform> {
    let mut t = RangeTransform::new();

    check_duplicates(node)?;

    for iter in node.iter() {
        let key = iter.first.as_::<Vec<u8>>()?;
        if iter.second.is_null()? || !iter.second.is_defined() {
            continue;
        }
        let value = &iter.second;
        match key.as_slice() {
            b"min_in_value" => t.set_min_in_value(load_double(value)?),
            b"max_in_value" => t.set_max_in_value(load_double(value)?),
            b"min_out_value" => t.set_min_out_value(load_double(value)?),
            b"max_out_value" => t.set_max_out_value(load_double(value)?),
            b"style" => {
                let style = load_string(value)?;
                t.set_style(range_style_from_string(Some(c_str(&style)))?);
            }
            b"direction" => t.set_direction(load_direction(value)?),
            b"name" => {
                let name = load_string(value)?;
                t.format_metadata_mut().set_name(Some(c_str(&name)));
            }
            _ => log_unknown_key_warning(node, &iter.first)?,
        }
    }
    Ok(t)
}

/// The node as a `std::vector<float>`.
///
/// Port of `load(const YAML::Node&, std::vector<float>&)` (OCIOYaml.cpp:130-144 @ v2.5.2).
pub(crate) fn load_vec_f32(node: &Node) -> LoadResult<Vec<f32>> {
    node.as_::<Vec<f32>>()
        .map_err(|e| parsing_failed(node, "vector<float>", &e.into()))
}

/// The node as an allocation: a string, then `AllocationFromString`.
///
/// Port of `load(const YAML::Node&, Allocation&)` (OCIOYaml.cpp:176-181 @ v2.5.2).
pub(crate) fn load_allocation(node: &Node) -> LoadResult<Allocation> {
    let s = load_string(node)?;
    Ok(allocation_from_string(Some(c_str(&s))))
}

/// An `AllocationTransform`: `allocation`, `vars` (numbers, kept when there are any),
/// `direction`.
///
/// Port of `load(const YAML::Node&, AllocationTransformRcPtr&)` (OCIOYaml.cpp:537-575 @
/// v2.5.2).
fn load_allocation_transform(node: &Node) -> LoadResult<AllocationTransform> {
    let mut t = AllocationTransform::new();

    check_duplicates(node)?;

    for iter in node.iter() {
        let key = iter.first.as_::<Vec<u8>>()?;
        if iter.second.is_null()? || !iter.second.is_defined() {
            continue;
        }
        let value = &iter.second;
        match key.as_slice() {
            b"allocation" => t.set_allocation(load_allocation(value)?),
            b"vars" => {
                let val = load_vec_f32(value)?;
                if !val.is_empty() {
                    t.set_vars(&val);
                }
            }
            b"direction" => t.set_direction(load_direction(value)?),
            _ => log_unknown_key_warning(node, &iter.first)?,
        }
    }
    Ok(t)
}

/// A CDL's slope, offset or power: 3 numbers, or "'slope' values must be 3 floats. Found
/// 'N'." through [`throw_value_error`].
fn load_cdl_triple(node: &Node, key: &Node, name: &str, value: &Node) -> LoadResult<[f64; 3]> {
    let floatvecval = load_vec_f64(value)?;
    match <[f64; 3]>::try_from(floatvecval.as_slice()) {
        Ok(rgb) => Ok(rgb),
        Err(_) => {
            let os = format!(
                "'{name}' values must be 3 floats. Found '{}'.",
                floatvecval.len()
            );
            Err(throw_value_error(node.tag()?, key, os.as_bytes()))
        }
    }
}

/// A `CDLTransform`: `slope`, `offset`, `power` (3 numbers each), `saturation` or `sat`,
/// `style`, `direction`, `name`.
///
/// Port of `load(const YAML::Node&, CDLTransformRcPtr&)` (OCIOYaml.cpp:646-726 @ v2.5.2).
fn load_cdl(node: &Node) -> LoadResult<CdlTransform> {
    let mut t = CdlTransform::new();

    check_duplicates(node)?;

    for iter in node.iter() {
        let key = iter.first.as_::<Vec<u8>>()?;
        if iter.second.is_null()? || !iter.second.is_defined() {
            continue;
        }
        let value = &iter.second;
        match key.as_slice() {
            b"slope" => t.set_slope(&load_cdl_triple(node, &iter.first, "slope", value)?),
            b"offset" => t.set_offset(&load_cdl_triple(node, &iter.first, "offset", value)?),
            b"power" => t.set_power(&load_cdl_triple(node, &iter.first, "power", value)?),
            b"saturation" | b"sat" => t.set_sat(load_double(value)?),
            b"style" => {
                let style = load_string(value)?;
                t.set_style(cdl_style_from_string(Some(c_str(&style)))?);
            }
            b"direction" => t.set_direction(load_direction(value)?),
            b"name" => {
                let name = load_string(value)?;
                t.format_metadata_mut().set_name(Some(c_str(&name)));
            }
            _ => log_unknown_key_warning(node, &iter.first)?,
        }
    }
    Ok(t)
}

/// Four numbers, or one number `v` read as `[v, v, v, alpha]`: the values of an exponent, or
/// the gamma and offset of an exponent with a linear segment.
fn load_rgba_or_single(value: &Node, alpha: f64) -> LoadResult<Vec<f64>> {
    if value.node_type()? == NodeType::Sequence {
        load_vec_f64(value)
    } else {
        // If a single value is supplied...
        let single_val = load_double(value)?;
        Ok(vec![single_val, single_val, single_val, alpha])
    }
}

/// An `ExponentTransform`: `value` (4 numbers, or one for RGB with an alpha of 1), `style`,
/// `direction`, `name`. The style depends on the direction set before it.
///
/// Port of `load(const YAML::Node&, ExponentTransformRcPtr&)` (OCIOYaml.cpp:917-977 @ v2.5.2).
fn load_exponent(node: &Node) -> LoadResult<ExponentTransform> {
    let mut t = ExponentTransform::new();

    check_duplicates(node)?;

    for iter in node.iter() {
        let key = iter.first.as_::<Vec<u8>>()?;
        if iter.second.is_null()? || !iter.second.is_defined() {
            continue;
        }
        let value = &iter.second;
        match key.as_slice() {
            b"value" => {
                let val = load_rgba_or_single(value, 1.0)?;
                let Ok(v) = <[f64; 4]>::try_from(val.as_slice()) else {
                    let os = format!("'value' values must be 4 floats. Found '{}'.", val.len());
                    return Err(throw_value_error(node.tag()?, &iter.first, os.as_bytes()));
                };
                t.set_value(&v);
            }
            b"style" => {
                let style = load_string(value)?;
                t.set_negative_style(negative_style_from_string(Some(c_str(&style)))?)?;
            }
            b"direction" => t.set_direction(load_direction(value)?),
            b"name" => {
                let name = load_string(value)?;
                t.format_metadata_mut().set_name(Some(c_str(&name)));
            }
            _ => log_unknown_key_warning(node, &iter.first)?,
        }
    }
    Ok(t)
}

/// An `ExponentWithLinearTransform`: `gamma` (4 numbers, or one for RGB with an alpha of 1)
/// and `offset` (4, or one with an alpha of 0), both required, `style`, `direction`, `name`.
/// Its messages start "ExponentWithLinear parse error, " and have no line; unknown keys are
/// reported with the node's tag (I-140).
///
/// Port of `load(const YAML::Node&, ExponentWithLinearTransformRcPtr&)` (OCIOYaml.cpp:
/// 1018-1140 @ v2.5.2).
fn load_exponent_with_linear(node: &Node) -> LoadResult<ExponentWithLinearTransform> {
    const ERR: &str = "ExponentWithLinear parse error, ";

    let mut t = ExponentWithLinearTransform::new();

    let mut gamma_found = false;
    let mut offset_found = false;

    check_duplicates(node)?;

    for iter in node.iter() {
        let key = iter.first.as_::<Vec<u8>>()?;
        if iter.second.is_null()? || !iter.second.is_defined() {
            continue;
        }
        let value = &iter.second;
        match key.as_slice() {
            b"gamma" => {
                let val = load_rgba_or_single(value, 1.0)?;
                let Ok(v) = <[f64; 4]>::try_from(val.as_slice()) else {
                    return Err(Exception::new(format!(
                        "{ERR}gamma field must be 4 floats. Found '{}'.",
                        val.len()
                    ))
                    .into());
                };
                t.set_gamma(&v);
                gamma_found = true;
            }
            b"offset" => {
                let val = load_rgba_or_single(value, 0.0)?;
                let Ok(v) = <[f64; 4]>::try_from(val.as_slice()) else {
                    return Err(Exception::new(format!(
                        "{ERR}offset field must be 4 floats. Found '{}'.",
                        val.len()
                    ))
                    .into());
                };
                t.set_offset(&v);
                offset_found = true;
            }
            b"style" => {
                let style = load_string(value)?;
                t.set_negative_style(negative_style_from_string(Some(c_str(&style)))?)?;
            }
            b"direction" => t.set_direction(load_direction(value)?),
            b"name" => {
                let name = load_string(value)?;
                t.format_metadata_mut().set_name(Some(c_str(&name)));
            }
            _ => log_unknown_key_warning_in(node.tag()?, &iter.first)?,
        }
    }

    if !(gamma_found && offset_found) {
        let missing = if !gamma_found && !offset_found {
            "gamma and offset fields are missing"
        } else if !gamma_found {
            "gamma field is missing"
        } else {
            "offset field is missing"
        };
        return Err(Exception::new(format!("{ERR}{missing}")).into());
    }
    Ok(t)
}

/// A `FixedFunctionTransform`: `params` (numbers, kept when there are any), `style`
/// (required), `direction`, `name`. The ACES 2 styles log that they are experimental; unknown
/// keys are reported with the node's tag (I-140).
///
/// Port of `load(const YAML::Node&, FixedFunctionTransformRcPtr&)` (OCIOYaml.cpp:1421-1483 @
/// v2.5.2).
fn load_fixed_function(node: &Node) -> LoadResult<FixedFunctionTransform> {
    let mut t = FixedFunctionTransform::new(FixedFunctionStyle::AcesRedMod03, &[])?;

    check_duplicates(node)?;

    let mut style_found = false;

    for iter in node.iter() {
        let key = iter.first.as_::<Vec<u8>>()?;
        if iter.second.is_null()? || !iter.second.is_defined() {
            continue;
        }
        let value = &iter.second;
        match key.as_slice() {
            b"params" => {
                let params = load_vec_f64(value)?;
                if !params.is_empty() {
                    t.set_params(&params);
                }
            }
            b"style" => {
                let style = load_string(value)?;
                t.set_style(fixed_function_style_from_string(Some(c_str(&style)))?)?;
                style_found = true;
                if matches!(
                    t.style(),
                    FixedFunctionStyle::AcesOutputTransform20
                        | FixedFunctionStyle::AcesRgbToJmh20
                        | FixedFunctionStyle::AcesTonescaleCompress20
                        | FixedFunctionStyle::AcesGamutCompress20
                ) {
                    let mut os = b"FixedFunction style is experimental and may be removed in a \
                                   future release: '"
                        .to_vec();
                    os.extend_from_slice(&style);
                    os.extend_from_slice(b"'.");
                    log_warning(os);
                }
            }
            b"direction" => t.set_direction(load_direction(value)?),
            b"name" => {
                let name = load_string(value)?;
                t.format_metadata_mut().set_name(Some(c_str(&name)));
            }
            _ => log_unknown_key_warning_in(node.tag()?, &iter.first)?,
        }
    }

    if !style_found {
        return Err(throw_error(node, b"style value is missing."));
    }
    Ok(t)
}

/// A log parameter of 3 numbers, or one number for all three. A sequence of another size
/// fails with "LogAffine/CameraTransform parse error, <key> value field must have 3
/// components. Found 'N'.".
///
/// Port of `loadLogParam` (OCIOYaml.cpp:2584-2612 @ v2.5.2).
fn load_log_param(node: &Node, param: &mut [f64; 3], param_name: &[u8]) -> LoadResult<()> {
    if node.size()? == 0 {
        // If a single value is provided.
        let val = load_double(node)?;
        *param = [val; 3];
    } else {
        let val = load_vec_f64(node)?;
        let Ok(values) = <[f64; 3]>::try_from(val.as_slice()) else {
            let mut os = b"LogAffine/CameraTransform parse error, ".to_vec();
            os.extend_from_slice(param_name);
            os.extend_from_slice(
                format!(
                    " value field must have 3 components. Found '{}'.",
                    val.len()
                )
                .as_bytes(),
            );
            return Err(Exception::new(os).into());
        };
        *param = values;
    }
    Ok(())
}

/// A log's `base`: one number, or "<class> parse error, base must be a single double. Found
/// N." for a sequence or a map of N elements.
fn load_base(value: &Node, class: &str) -> LoadResult<f64> {
    let nb = value.size()?;
    if nb == 0 {
        load_double(value)
    } else {
        Err(Exception::new(format!(
            "{class} parse error, base must be a single double. Found {nb}."
        ))
        .into())
    }
}

/// A `LogAffineTransform`: `base`, the four log parameters, `direction`, `name`. The values
/// are set once the map is read.
///
/// Port of `load(const YAML::Node&, LogAffineTransformRcPtr&)` (OCIOYaml.cpp:2614-2685 @
/// v2.5.2).
fn load_log_affine(node: &Node) -> LoadResult<LogAffineTransform> {
    let mut t = LogAffineTransform::new();

    check_duplicates(node)?;

    let mut base = 2.0;
    let mut log_slope = [1.0; 3];
    let mut lin_slope = [1.0; 3];
    let mut lin_offset = [0.0; 3];
    let mut log_offset = [0.0; 3];

    for iter in node.iter() {
        let key = iter.first.as_::<Vec<u8>>()?;
        if iter.second.is_null()? || !iter.second.is_defined() {
            continue;
        }
        let value = &iter.second;
        match key.as_slice() {
            b"base" => base = load_base(value, "LogAffineTransform")?,
            b"lin_side_offset" => load_log_param(value, &mut lin_offset, &key)?,
            b"lin_side_slope" => load_log_param(value, &mut lin_slope, &key)?,
            b"log_side_offset" => load_log_param(value, &mut log_offset, &key)?,
            b"log_side_slope" => load_log_param(value, &mut log_slope, &key)?,
            b"direction" => t.set_direction(load_direction(value)?),
            b"name" => {
                let name = load_string(value)?;
                t.format_metadata_mut().set_name(Some(c_str(&name)));
            }
            _ => log_unknown_key_warning(node, &iter.first)?,
        }
    }

    t.set_base(base);
    t.set_log_side_slope_value(&log_slope);
    t.set_lin_side_slope_value(&lin_slope);
    t.set_lin_side_offset_value(&lin_offset);
    t.set_log_side_offset_value(&log_offset);
    Ok(t)
}

/// A `LogCameraTransform`: `base`, the four log parameters, `lin_side_break` (required),
/// `linear_slope` (set only when given), `direction`, `name`. The values are set once the map
/// is read.
///
/// Port of `load(const YAML::Node&, LogCameraTransformRcPtr&)` (OCIOYaml.cpp:2740-2834 @
/// v2.5.2).
fn load_log_camera(node: &Node) -> LoadResult<LogCameraTransform> {
    let mut lin_break = [0.0; 3];
    let mut t = LogCameraTransform::new(&lin_break);

    check_duplicates(node)?;

    let mut base = 2.0;
    let mut log_slope = [1.0; 3];
    let mut lin_slope = [1.0; 3];
    let mut lin_offset = [0.0; 3];
    let mut log_offset = [0.0; 3];
    let mut linear_slope = [1.0; 3];
    let mut lin_break_found = false;
    let mut linear_slope_found = false;

    for iter in node.iter() {
        let key = iter.first.as_::<Vec<u8>>()?;
        if iter.second.is_null()? || !iter.second.is_defined() {
            continue;
        }
        let value = &iter.second;
        match key.as_slice() {
            b"base" => base = load_base(value, "LogCameraTransform")?,
            b"lin_side_offset" => load_log_param(value, &mut lin_offset, &key)?,
            b"lin_side_slope" => load_log_param(value, &mut lin_slope, &key)?,
            b"log_side_offset" => load_log_param(value, &mut log_offset, &key)?,
            b"log_side_slope" => load_log_param(value, &mut log_slope, &key)?,
            b"lin_side_break" => {
                lin_break_found = true;
                load_log_param(value, &mut lin_break, &key)?;
            }
            b"linear_slope" => {
                linear_slope_found = true;
                load_log_param(value, &mut linear_slope, &key)?;
            }
            b"direction" => t.set_direction(load_direction(value)?),
            b"name" => {
                let name = load_string(value)?;
                t.format_metadata_mut().set_name(Some(c_str(&name)));
            }
            _ => log_unknown_key_warning(node, &iter.first)?,
        }
    }

    if !lin_break_found {
        return Err(Exception::new(
            "LogCameraTransform parse error: lin_side_break values are missing.",
        )
        .into());
    }

    t.set_base(base);
    t.set_log_side_slope_value(&log_slope);
    t.set_lin_side_slope_value(&lin_slope);
    t.set_lin_side_offset_value(&lin_offset);
    t.set_log_side_offset_value(&log_offset);
    t.set_lin_side_break_value(&lin_break);
    if linear_slope_found {
        t.set_linear_slope_value(&linear_slope)?;
    }
    Ok(t)
}

/// The deepest the port nests GroupTransforms while loading them (U-60): about half the lowest
/// depth at which the wheel's recursion overflows its stack, 1,182 levels in Python's main thread
/// on Windows (5,129 on Linux), as it does on a group that contains itself through an alias.
/// The port loads and copies groups without recursion.
pub(crate) const MAX_GROUP_DEPTH: usize = 590;

/// A group being loaded: its node, the transform so far, the pairs of its map still to read,
/// and the `children` being read (the sequence, the next index and the size).
struct GroupFrame {
    node: Node,
    group: GroupTransform,
    pairs: NodeIter,
    children: Option<(Node, usize, usize)>,
}

impl GroupFrame {
    /// Starts a group: `t = GroupTransform::Create()`, then `CheckDuplicates(node)`.
    fn open(node: &Node) -> LoadResult<GroupFrame> {
        let group = GroupTransform::new();
        check_duplicates(node)?;
        Ok(GroupFrame {
            node: node.clone(),
            group,
            pairs: node.iter(),
            children: None,
        })
    }
}

/// A `GroupTransform`: `children` (each a transform, loaded in order), `direction`, `name`.
/// A group that [`MAX_GROUP_DEPTH`] groups hold fails (U-60).
///
/// Upstream recurses into each child (`load(const YAML::Node&, TransformRcPtr&)`). The port
/// walks the nested groups with a stack of its own instead, in the same order, so that no
/// depth overflows the caller's stack: each group's pairs are read in order, and a child
/// group is read whole before its parent's next child. Upstream fails with "Child transform
/// could not be parsed." when a child loads as null, which can't happen: a child is a
/// transform or an exception.
///
/// Port of `load(const YAML::Node&, GroupTransformRcPtr&)` (OCIOYaml.cpp:2507-2556 @ v2.5.2).
fn load_group(node: &Node) -> LoadResult<GroupTransform> {
    let mut stack = vec![GroupFrame::open(node)?];
    loop {
        let level = stack.len() - 1;
        // The next child of the `children` being read.
        if let Some((value, next, size)) = &mut stack[level].children {
            if *next < *size {
                let val = value.get(*next)?;
                *next += 1;
                // load(val, childTransform): `level + 1` groups hold the child.
                let node_type = val.node_type()?;
                if node_type != NodeType::Map {
                    return Err(not_a_map(&val, node_type));
                }
                if val.tag()? == b"GroupTransform" {
                    if level + 1 >= MAX_GROUP_DEPTH {
                        return Err(nested_too_deep(&val));
                    }
                    stack.push(GroupFrame::open(&val)?);
                } else {
                    let child = load_leaf(&val)?;
                    stack[level].group.append_transform(child);
                }
                continue;
            }
            stack[level].children = None;
        }

        // The next pair of the map, or the group is complete.
        let frame = &mut stack[level];
        let Some(iter) = frame.pairs.next() else {
            let done = stack.pop().expect("a group").group;
            match stack.last_mut() {
                Some(parent) => parent.group.append_transform(Transform::Group(done)),
                None => return Ok(done),
            }
            continue;
        };
        let key = iter.first.as_::<Vec<u8>>()?;
        if iter.second.is_null()? || !iter.second.is_defined() {
            continue;
        }
        let value = &iter.second;
        match key.as_slice() {
            b"children" => frame.children = Some((value.clone(), 0, value.size()?)),
            b"direction" => frame.group.set_direction(load_direction(value)?),
            b"name" => {
                let name = load_string(value)?;
                frame
                    .group
                    .format_metadata_mut()
                    .set_name(Some(c_str(&name)));
            }
            _ => log_unknown_key_warning(&frame.node, &iter.first)?,
        }
    }
}

/// The node as a `bool`: yaml-cpp's spellings (`true`, `yes`, `on`, ... in three cases).
///
/// Port of `load(const YAML::Node&, bool&)` (OCIOYaml.cpp:66-80 @ v2.5.2).
pub(crate) fn load_bool(node: &Node) -> LoadResult<bool> {
    node.as_::<bool>()
        .map_err(|e| parsing_failed(node, "boolean", &e.into()))
}

/// The node as an interpolation: a string, then `InterpolationFromString`.
///
/// Port of `load(const YAML::Node&, Interpolation&)` (OCIOYaml.cpp:201-206 @ v2.5.2).
pub(crate) fn load_interpolation(node: &Node) -> LoadResult<Interpolation> {
    let s = load_string(node)?;
    Ok(interpolation_from_string(Some(c_str(&s))))
}

/// A `BuiltinTransform`: `style` (a built-in transform's name, in any case), `direction`. It
/// doesn't check for repeated keys: the last one wins.
///
/// Port of `load(const YAML::Node&, BuiltinTransformRcPtr&)` (OCIOYaml.cpp:600-629 @ v2.5.2).
fn load_builtin(node: &Node) -> LoadResult<BuiltinTransform> {
    let mut t = BuiltinTransform::new();

    for iter in node.iter() {
        let key = iter.first.as_::<Vec<u8>>()?;
        if iter.second.is_null()? || !iter.second.is_defined() {
            continue;
        }
        let value = &iter.second;
        match key.as_slice() {
            b"style" => {
                let transform_style = load_string(value)?;
                t.set_style(c_str(&transform_style))?;
            }
            b"direction" => t.set_direction(load_direction(value)?),
            _ => log_unknown_key_warning(node, &iter.first)?,
        }
    }
    Ok(t)
}

/// A `ColorSpaceTransform`: `src`, `dst`, `direction`, `data_bypass`.
///
/// Port of `load(const YAML::Node&, ColorSpaceTransformRcPtr&)` (OCIOYaml.cpp:778-819 @
/// v2.5.2).
fn load_color_space_transform(node: &Node) -> LoadResult<ColorSpaceTransform> {
    let mut t = ColorSpaceTransform::new();

    check_duplicates(node)?;

    for iter in node.iter() {
        let key = iter.first.as_::<Vec<u8>>()?;
        if iter.second.is_null()? || !iter.second.is_defined() {
            continue;
        }
        let value = &iter.second;
        match key.as_slice() {
            b"src" => t.set_src(c_str(&load_string(value)?)),
            b"dst" => t.set_dst(c_str(&load_string(value)?)),
            b"direction" => t.set_direction(load_direction(value)?),
            b"data_bypass" => t.set_data_bypass(load_bool(value)?),
            _ => log_unknown_key_warning(node, &iter.first)?,
        }
    }
    Ok(t)
}

/// A `DisplayViewTransform`: `src`, `display`, `view`, `direction`, `looks_bypass`,
/// `data_bypass`. It doesn't check for repeated keys: the last one wins.
///
/// Port of `load(const YAML::Node&, DisplayViewTransformRcPtr&)` (OCIOYaml.cpp:840-891 @
/// v2.5.2).
fn load_display_view(node: &Node) -> LoadResult<DisplayViewTransform> {
    let mut t = DisplayViewTransform::new();

    for iter in node.iter() {
        let key = iter.first.as_::<Vec<u8>>()?;
        if iter.second.is_null()? || !iter.second.is_defined() {
            continue;
        }
        let value = &iter.second;
        match key.as_slice() {
            b"src" => t.set_src(c_str(&load_string(value)?)),
            b"display" => t.set_display(c_str(&load_string(value)?)),
            b"view" => t.set_view(c_str(&load_string(value)?)),
            b"direction" => t.set_direction(load_direction(value)?),
            b"looks_bypass" => t.set_looks_bypass(load_bool(value)?),
            b"data_bypass" => t.set_data_bypass(load_bool(value)?),
            _ => log_unknown_key_warning(node, &iter.first)?,
        }
    }
    Ok(t)
}

/// A `FileTransform`: `src`, `cccid`, `cdl_style`, `interpolation`, `direction`.
///
/// Port of `load(const YAML::Node&, FileTransformRcPtr&)` (OCIOYaml.cpp:1336-1382 @ v2.5.2).
fn load_file(node: &Node) -> LoadResult<FileTransform> {
    let mut t = FileTransform::new();

    check_duplicates(node)?;

    for iter in node.iter() {
        let key = iter.first.as_::<Vec<u8>>()?;
        if iter.second.is_null()? || !iter.second.is_defined() {
            continue;
        }
        let value = &iter.second;
        match key.as_slice() {
            b"src" => t.set_src(c_str(&load_string(value)?)),
            b"cccid" => t.set_ccc_id(c_str(&load_string(value)?)),
            b"cdl_style" => {
                let stringval = load_string(value)?;
                t.set_cdl_style(cdl_style_from_string(Some(c_str(&stringval)))?);
            }
            b"interpolation" => t.set_interpolation(load_interpolation(value)?),
            b"direction" => t.set_direction(load_direction(value)?),
            _ => log_unknown_key_warning(node, &iter.first)?,
        }
    }
    Ok(t)
}

/// A `LookTransform`: `src`, `dst`, `looks`, `direction`.
///
/// Port of `load(const YAML::Node&, LookTransformRcPtr&)` (OCIOYaml.cpp:2946-2987 @ v2.5.2).
fn load_look(node: &Node) -> LoadResult<LookTransform> {
    let mut t = LookTransform::new();

    check_duplicates(node)?;

    for iter in node.iter() {
        let key = iter.first.as_::<Vec<u8>>()?;
        if iter.second.is_null()? || !iter.second.is_defined() {
            continue;
        }
        let value = &iter.second;
        match key.as_slice() {
            b"src" => t.set_src(c_str(&load_string(value)?)),
            b"dst" => t.set_dst(c_str(&load_string(value)?)),
            b"looks" => t.set_looks(c_str(&load_string(value)?)),
            b"direction" => t.set_direction(load_direction(value)?),
            _ => log_unknown_key_warning(node, &iter.first)?,
        }
    }
    Ok(t)
}

/// The error of a transform class whose loader is not ported yet: `work_package` ports it.
fn not_ported_yet(tag: &[u8], work_package: &str) -> LoadError {
    let mut msg = b"Loading a !<".to_vec();
    msg.extend_from_slice(tag);
    msg.extend_from_slice(
        format!("> from a config is not ported yet ({work_package}).").as_bytes(),
    );
    LoadError::Ocio(Exception::new(msg))
}

/// A transform: a map, whose tag names its class. Another node fails with "Unsupported
/// Transform type encountered: (N) in OCIO profile. Only Mapping types supported." (N the
/// node's `YAML::NodeType` value), and an unknown tag with "Unsupported transform type !<Tag>
/// in OCIO profile. ". The Lut1D and Lut3D transforms have no YAML form, so their tags are
/// unknown too.
///
/// Port of `load(const YAML::Node&, TransformRcPtr&)` (OCIOYaml.cpp:3196-3351 @ v2.5.2).
pub(crate) fn load_transform(node: &Node) -> LoadResult<Transform> {
    let node_type = node.node_type()?;
    if node_type != NodeType::Map {
        return Err(not_a_map(node, node_type));
    }

    if node.tag()? == b"GroupTransform" {
        return Ok(Transform::Group(load_group(node)?));
    }
    load_leaf(node)
}

/// The error of a transform node that isn't a map.
fn not_a_map(node: &Node, node_type: NodeType) -> LoadError {
    let os = format!(
        "Unsupported Transform type encountered: ({}) in OCIO profile. Only Mapping types \
         supported.",
        node_type as i32
    );
    throw_error(node, os.as_bytes())
}

/// The error of a group nested past [`MAX_GROUP_DEPTH`] (U-60).
fn nested_too_deep(node: &Node) -> LoadError {
    let os = format!(
        "GroupTransforms nested more than {MAX_GROUP_DEPTH} deep can't be loaded: upstream's \
         stack overflows."
    );
    throw_error(node, os.as_bytes())
}

/// A transform of a class other than a group, by the map's tag.
fn load_leaf(node: &Node) -> LoadResult<Transform> {
    let ty = node.tag()?.to_vec();
    Ok(match ty.as_slice() {
        b"BuiltinTransform" => load_builtin(node)?.into(),
        b"ColorSpaceTransform" => load_color_space_transform(node)?.into(),
        b"DisplayViewTransform" => load_display_view(node)?.into(),
        b"FileTransform" => load_file(node)?.into(),
        b"LookTransform" => load_look(node)?.into(),
        b"AllocationTransform" => load_allocation_transform(node)?.into(),
        b"CDLTransform" => load_cdl(node)?.into(),
        b"ExponentTransform" => load_exponent(node)?.into(),
        b"ExponentWithLinearTransform" => load_exponent_with_linear(node)?.into(),
        b"LogAffineTransform" => load_log_affine(node)?.into(),
        b"LogCameraTransform" => load_log_camera(node)?.into(),
        b"ExposureContrastTransform"
        | b"GradingPrimaryTransform"
        | b"GradingRGBCurveTransform"
        | b"GradingHueCurveTransform"
        | b"GradingToneTransform" => return Err(not_ported_yet(&ty, "Phase 5")),
        b"FixedFunctionTransform" => load_fixed_function(node)?.into(),
        b"LogTransform" => load_log(node)?.into(),
        b"MatrixTransform" => load_matrix(node)?.into(),
        b"RangeTransform" => load_range(node)?.into(),
        _ => {
            let mut os = b"Unsupported transform type !<".to_vec();
            os.extend_from_slice(&ty);
            os.extend_from_slice(b"> in OCIO profile. ");
            return Err(throw_error(node, &os));
        }
    })
}

#[cfg(test)]
#[path = "ocio_yaml_oracle_tests.rs"]
mod oracle_tests;
