// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! OCIO's YAML reader and writer: a port of the `load` functions of
//! `src/OpenColorIO/OCIOYaml.cpp` @ v2.5.2, which read a config's nodes (parsed by
//! [`crate::yaml_cpp`]) into OCIO's objects, and of its `save` functions, which write them
//! through yaml-cpp's emitter ([`save_transform`], [`write`]: `Config::serialize`).
//!
//! The reader: the typed loaders and their messages, the helpers that report unknown keys, bad
//! values and repeated keys, and the transforms ([`load_transform`]), except ExposureContrast
//! and the grading transforms (Phase 5); and the color spaces, looks, view transforms and
//! named transforms, with their descriptions and interchange attributes; the views, the file and
//! viewing rules, and the config itself ([`load_config`], [`read`]: `Config::CreateFromStream`).
//! The writer: every saver but those of ExposureContrast and the grading transforms (Phase 5).
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

use std::collections::{BTreeMap, HashSet};

use ocio_ops::exception::Exception;
use ocio_ops::format_metadata::FormatMetadataImpl;
use ocio_ops::logging::{log_debug, log_warning};
use ocio_ops::math_utils::{
    is_m44_identity, is_scalar_equal_to_one, is_vec_equal_to_one, is_vec_equal_to_zero,
};
use ocio_ops::open_color_types::{
    Allocation, BitDepth, CdlStyle, ColorSpaceDirection, ColorSpaceVisibility, EnvironmentMode,
    FixedFunctionStyle, NamedTransformVisibility, NegativeStyle, ReferenceSpaceType,
    SearchReferenceSpaceType, TransformDirection, ViewTransformDirection, ViewType,
    allocation_to_string, bit_depth_to_string, cdl_style_to_string,
    fixed_function_style_from_string, fixed_function_style_to_string, interpolation_to_string,
    negative_style_to_string, transform_direction_to_string,
};
use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;
use ocio_ops::parse_utils::{
    ROLE_DEFAULT, allocation_from_string, bit_depth_from_string, cdl_style_from_string,
    interpolation_from_string, join_string_env_style, negative_style_from_string,
    split_string_env_style, transform_direction_from_string,
};
use ocio_ops::platform::strcasecmp;
use ocio_ops::utils::pystring::os_path;
use ocio_ops::utils::string_utils::{c_str, split};

use crate::color_space::ColorSpace;
use crate::config::Config;
use crate::display::View;
use crate::file_rules::{FileRules, update_file_rules_from_v1_to_v2};
use crate::look::Look;
use crate::named_transform::NamedTransform;
use crate::path_utils::abs_path;
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
use crate::transforms::range_transform::{
    RangeStyle, RangeTransform, range_style_from_string, range_style_to_string,
};
use crate::view_transform::ViewTransform;
use crate::viewing_rules::ViewingRules;
use crate::yaml_cpp::emitter::Emitter;
use crate::yaml_cpp::emitter_manip::EmitterManip::{
    BeginMap, BeginSeq, Block, EndMap, EndSeq, Flow, Key, Literal, Newline, Value,
};
use crate::yaml_cpp::emitter_manip::verbatim_tag;
use crate::yaml_cpp::exceptions::Exception as YamlException;
use crate::yaml_cpp::node::{Node, NodeIter, NodeType};
use crate::yaml_cpp::parse::load;

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
fn load_look_transform(node: &Node) -> LoadResult<LookTransform> {
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
        b"LookTransform" => load_look_transform(node)?.into(),
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

/// The string without its trailing newlines: YAML keeps them inconsistently (a literal block
/// reads back with one, a plain value with all), so a description loses them all.
///
/// Upstream reads `str.back()` of the string it has just emptied when every character was a
/// newline (an access out of range), and drops the value: the loop stops as the string is
/// empty, whatever was read (docs/improvements.md, I-143). The port reads nothing there.
///
/// Port of `SanitizeNewlines` (OCIOYaml.cpp:37-62 @ v2.5.2).
pub(crate) fn sanitize_newlines(input: &[u8]) -> Vec<u8> {
    if input.is_empty() {
        return input.to_vec();
    }

    let mut str = input.to_vec();
    let mut last = str.last().copied();
    while last == Some(b'\n') && !str.is_empty() {
        str.pop();
        last = str.last().copied();
    }

    str
}

/// The node as a string without its trailing newlines: a description.
///
/// Port of `loadDescription` (OCIOYaml.cpp:213-217 @ v2.5.2).
pub(crate) fn load_description(node: &Node) -> LoadResult<Vec<u8>> {
    Ok(sanitize_newlines(&load_string(node)?))
}

/// The node as a `StringVec`, a list of strings.
///
/// Port of `load(const YAML::Node&, StringUtils::StringVec&)` (OCIOYaml.cpp:114-128 @ v2.5.2).
pub(crate) fn load_string_vec(node: &Node) -> LoadResult<Vec<Vec<u8>>> {
    node.as_::<Vec<Vec<u8>>>()
        .map_err(|e| parsing_failed(node, "StringVec", &e.into()))
}

/// The node as a bit depth: a string, then `BitDepthFromString` (an unknown name is
/// `BIT_DEPTH_UNKNOWN`).
///
/// Port of `load(const YAML::Node&, BitDepth&)` (OCIOYaml.cpp:164-169 @ v2.5.2).
pub(crate) fn load_bit_depth(node: &Node) -> LoadResult<BitDepth> {
    let s = load_string(node)?;
    Ok(bit_depth_from_string(Some(c_str(&s))))
}

/// The pairs of a map, kept as nodes, or "Expected a YAML map in the <section> section."
/// through [`throw_error`].
///
/// Port of `CustomKeysLoader` and `loadCustomKeys` (OCIOYaml.cpp:325-346 @ v2.5.2).
pub(crate) fn load_custom_keys(node: &Node, section_name: &str) -> LoadResult<Vec<(Node, Node)>> {
    if node.node_type()? == NodeType::Map {
        Ok(node.iter().map(|iter| (iter.first, iter.second)).collect())
    } else {
        let ss = format!("Expected a YAML map in the {section_name} section.");
        Err(throw_error(node, ss.as_bytes()))
    }
}

/// Sets the interchange attributes of a map through `set_attribute` (each value without its
/// trailing newlines); an attribute the owner refuses (it doesn't know its name) is a warning,
/// "Unknown key in interchange: 'key'.". A node that isn't a map fails with "The 'interchange'
/// content needs to be a map.".
///
/// Port of `loadInterchangeAttributes` (OCIOYaml.cpp:375-404 @ v2.5.2).
pub(crate) fn load_interchange_attributes(
    node: &Node,
    mut set_attribute: impl FnMut(&[u8], &[u8]) -> ocio_ops::exception::Result<()>,
) -> LoadResult<()> {
    if node.node_type()? != NodeType::Map {
        return Err(throw_error(
            node,
            b"The 'interchange' content needs to be a map.",
        ));
    }

    let kv = load_custom_keys(node, "interchange")?;

    for (key, value) in &kv {
        let keystr = key.as_::<Vec<u8>>()?;
        let valstr = value.as_::<Vec<u8>>()?;
        let valstr = sanitize_newlines(&valstr);

        // OCIO exception means the key is not recognized. Convert that to a warning.
        if set_attribute(c_str(&keystr), c_str(&valstr)).is_err() {
            log_unknown_key_warning_in(b"interchange", key)?;
        }
    }
    Ok(())
}

/// A color space's map, into `cs` (created by the config with the reference space of its
/// section). A node not tagged `!<ColorSpace>` is left alone. The scene-referred transform
/// keys `to_scene_reference` and `from_scene_reference` exist from version 2 on; earlier, they
/// are unknown keys.
///
/// Port of `load(const YAML::Node&, ColorSpaceRcPtr&, unsigned int)` (OCIOYaml.cpp:3424-3572
/// @ v2.5.2).
pub(crate) fn load_color_space(
    node: &Node,
    cs: &mut ColorSpace,
    major_version: u32,
) -> LoadResult<()> {
    if node.tag()? != b"ColorSpace" {
        return Ok(()); // not a !<ColorSpace> tag
    }

    if node.node_type()? != NodeType::Map {
        return Err(throw_error(
            node,
            b"The '!<ColorSpace>' content needs to be a map.",
        ));
    }

    check_duplicates(node)?;

    for iter in node.iter() {
        let key = iter.first.as_::<Vec<u8>>()?;
        if iter.second.is_null()? || !iter.second.is_defined() {
            continue;
        }
        let value = &iter.second;
        match key.as_slice() {
            b"name" => cs.set_name(c_str(&load_string(value)?)),
            b"aliases" => {
                for alias in load_string_vec(value)? {
                    cs.add_alias(c_str(&alias));
                }
            }
            b"interop_id" => cs.set_interop_id(c_str(&load_string(value)?))?,
            b"description" => cs.set_description(c_str(&load_description(value)?)),
            b"interchange" => {
                load_interchange_attributes(value, |k, v| cs.set_interchange_attribute(k, v))?
            }
            b"family" => cs.set_family(c_str(&load_string(value)?)),
            b"equalitygroup" => cs.set_equality_group(c_str(&load_string(value)?)),
            b"bitdepth" => cs.set_bit_depth(load_bit_depth(value)?),
            b"isdata" => cs.set_is_data(load_bool(value)?),
            b"categories" => {
                for name in load_string_vec(value)? {
                    cs.add_category(c_str(&name));
                }
            }
            b"encoding" => cs.set_encoding(c_str(&load_string(value)?)),
            b"allocation" => cs.set_allocation(load_allocation(value)?),
            b"allocationvars" => {
                let val = load_vec_f32(value)?;
                if !val.is_empty() {
                    cs.set_allocation_vars(&val);
                }
            }
            b"to_reference" | b"to_scene_reference"
                if key == b"to_reference" || major_version >= 2 =>
            {
                if cs.reference_space_type() == ReferenceSpaceType::Display {
                    return Err(throw_error(
                        node,
                        b"'to_reference' or 'to_scene_reference' cannot be used for a display \
                          color space.",
                    ));
                }
                let val = load_transform(value)?;
                cs.set_transform(Some(&val), ColorSpaceDirection::ToReference)?;
            }
            b"to_display_reference" => {
                if cs.reference_space_type() == ReferenceSpaceType::Scene {
                    return Err(throw_error(
                        node,
                        b"'to_display_reference' cannot be used for a non-display color space.",
                    ));
                }
                let val = load_transform(value)?;
                cs.set_transform(Some(&val), ColorSpaceDirection::ToReference)?;
            }
            b"from_reference" | b"from_scene_reference"
                if key == b"from_reference" || major_version >= 2 =>
            {
                if cs.reference_space_type() == ReferenceSpaceType::Display {
                    return Err(throw_error(
                        node,
                        b"'from_reference' or 'from_scene_reference' cannot be used for a \
                          display color space.",
                    ));
                }
                let val = load_transform(value)?;
                cs.set_transform(Some(&val), ColorSpaceDirection::FromReference)?;
            }
            b"from_display_reference" => {
                if cs.reference_space_type() == ReferenceSpaceType::Scene {
                    return Err(throw_error(
                        node,
                        b"'from_display_reference' cannot be used for a non-display color \
                          space.",
                    ));
                }
                let val = load_transform(value)?;
                cs.set_transform(Some(&val), ColorSpaceDirection::FromReference)?;
            }
            _ => log_unknown_key_warning(node, &iter.first)?,
        }
    }
    Ok(())
}

/// A look's map, into `look`. A node not tagged `!<Look>` is left alone.
///
/// Port of `load(const YAML::Node&, LookRcPtr&)` (OCIOYaml.cpp:3667-3719 @ v2.5.2).
pub(crate) fn load_look(node: &Node, look: &mut Look) -> LoadResult<()> {
    if node.tag()? != b"Look" {
        return Ok(());
    }

    check_duplicates(node)?;

    for iter in node.iter() {
        let key = iter.first.as_::<Vec<u8>>()?;
        if iter.second.is_null()? || !iter.second.is_defined() {
            continue;
        }
        let value = &iter.second;
        match key.as_slice() {
            b"name" => look.set_name(c_str(&load_string(value)?)),
            b"process_space" => look.set_process_space(c_str(&load_string(value)?)),
            b"transform" => look.set_transform(&load_transform(value)?)?,
            b"inverse_transform" => look.set_inverse_transform(&load_transform(value)?)?,
            b"description" => look.set_description(c_str(&load_description(value)?)),
            b"interchange" => {
                load_interchange_attributes(value, |k, v| look.set_interchange_attribute(k, v))?
            }
            _ => log_unknown_key_warning(node, &iter.first)?,
        }
    }
    Ok(())
}

/// The reference space of a view transform, from the transform keys of its map: display when
/// it has `to_display_reference` or `from_display_reference`, scene when it has the scene
/// ones; neither or both are errors.
///
/// Port of `peekViewTransformReferenceSpace` (OCIOYaml.cpp:3750-3795 @ v2.5.2).
pub(crate) fn peek_view_transform_reference_space(node: &Node) -> LoadResult<ReferenceSpaceType> {
    if node.node_type()? != NodeType::Map {
        return Err(throw_error(
            node,
            b"The '!<ViewTransform>' content needs to be a map.",
        ));
    }

    let mut is_scene = false;
    let mut is_display = false;

    for iter in node.iter() {
        let key = iter.first.as_::<Vec<u8>>()?;
        if iter.second.is_null()? || !iter.second.is_defined() {
            continue;
        }
        match key.as_slice() {
            b"to_scene_reference" | b"from_scene_reference" => is_scene = true,
            b"to_display_reference" | b"from_display_reference" => is_display = true,
            _ => {}
        }
    }

    if !is_scene && !is_display {
        return Err(throw_error(
            node,
            b"The '!<ViewTransform>' needs to refer to a transform.",
        ));
    } else if is_scene && is_display {
        return Err(throw_error(
            node,
            b"The '!<ViewTransform>' cannot have both to/from_reference and \
              to/from_display_reference transforms.",
        ));
    }
    Ok(if is_display {
        ReferenceSpaceType::Display
    } else {
        ReferenceSpaceType::Scene
    })
}

/// A view transform's map, into `vt` (created with the reference space
/// [`peek_view_transform_reference_space`] finds). A node not tagged `!<ViewTransform>` is
/// left alone.
///
/// Port of `load(const YAML::Node&, ViewTransformRcPtr&)` (OCIOYaml.cpp:3797-3879 @ v2.5.2).
pub(crate) fn load_view_transform(node: &Node, vt: &mut ViewTransform) -> LoadResult<()> {
    if node.tag()? != b"ViewTransform" {
        return Ok(()); // not a !<ViewTransform> tag
    }

    if node.node_type()? != NodeType::Map {
        return Err(throw_error(
            node,
            b"The '!<ViewTransform>' content needs to be a map.",
        ));
    }

    check_duplicates(node)?;

    for iter in node.iter() {
        let key = iter.first.as_::<Vec<u8>>()?;
        if iter.second.is_null()? || !iter.second.is_defined() {
            continue;
        }
        let value = &iter.second;
        match key.as_slice() {
            b"name" => vt.set_name(c_str(&load_string(value)?)),
            b"description" => vt.set_description(c_str(&load_description(value)?)),
            b"interchange" => {
                load_interchange_attributes(value, |k, v| vt.set_interchange_attribute(k, v))?
            }
            b"family" => vt.set_family(c_str(&load_string(value)?)),
            b"categories" => {
                for name in load_string_vec(value)? {
                    vt.add_category(c_str(&name));
                }
            }
            b"to_scene_reference" | b"to_display_reference" => {
                let val = load_transform(value)?;
                vt.set_transform(Some(&val), ViewTransformDirection::ToReference)?;
            }
            b"from_scene_reference" | b"from_display_reference" => {
                let val = load_transform(value)?;
                vt.set_transform(Some(&val), ViewTransformDirection::FromReference)?;
            }
            _ => log_unknown_key_warning(node, &iter.first)?,
        }
    }
    Ok(())
}

/// A named transform's map, into `nt`. A node not tagged `!<NamedTransform>` is left alone.
/// Its description keeps its trailing newlines (upstream reads it with the plain string
/// loader, I-144).
///
/// Port of `load(const YAML::Node&, NamedTransformRcPtr&)` (OCIOYaml.cpp:3927-4006 @ v2.5.2).
pub(crate) fn load_named_transform(node: &Node, nt: &mut NamedTransform) -> LoadResult<()> {
    if node.tag()? != b"NamedTransform" {
        return Ok(()); // not a !<NamedTransform> tag
    }

    if node.node_type()? != NodeType::Map {
        return Err(throw_error(
            node,
            b"The '!<NamedTransform>' content needs to be a map.",
        ));
    }

    check_duplicates(node)?;

    for iter in node.iter() {
        let key = iter.first.as_::<Vec<u8>>()?;
        if iter.second.is_null()? || !iter.second.is_defined() {
            continue;
        }
        let value = &iter.second;
        match key.as_slice() {
            b"name" => nt.set_name(c_str(&load_string(value)?)),
            b"aliases" => {
                for alias in load_string_vec(value)? {
                    nt.add_alias(c_str(&alias));
                }
            }
            b"description" => nt.set_description(c_str(&load_string(value)?)),
            b"family" => nt.set_family(c_str(&load_string(value)?)),
            b"categories" => {
                for name in load_string_vec(value)? {
                    nt.add_category(c_str(&name));
                }
            }
            b"encoding" => nt.set_encoding(c_str(&load_string(value)?)),
            b"transform" => {
                let val = load_transform(value)?;
                nt.set_transform(Some(&val), TransformDirection::Forward)?;
            }
            b"inverse_transform" => {
                let val = load_transform(value)?;
                nt.set_transform(Some(&val), TransformDirection::Inverse)?;
            }
            _ => log_unknown_key_warning(node, &iter.first)?,
        }
    }
    Ok(())
}

/// Wraps an OCIO exception of a rule's setters as upstream's `catch (Exception & ex)` does,
/// "<prefix><what()>" through [`throw_error`]; a yaml-cpp exception passes through.
fn rule_error(node: &Node, prefix: &[u8], e: LoadError) -> LoadError {
    match e {
        LoadError::Ocio(ex) => {
            let mut os = prefix.to_vec();
            os.extend_from_slice(ex.what());
            throw_error(node, c_str(&os))
        }
        yaml => yaml,
    }
}

/// A file rule's map, inserted before the default rule of `fr`; the `Default` rule sets the
/// default rule's color space instead, and sets `default_rule_found`. A node not tagged
/// `!<Rule>` is left alone. The setters' errors become "File rules: <what()>" through
/// [`throw_error`].
///
/// Port of `load(const YAML::Node&, FileRulesRcPtr&, bool&)` (OCIOYaml.cpp:4072-4204 @
/// v2.5.2).
pub(crate) fn load_file_rule(
    node: &Node,
    fr: &mut FileRules,
    default_rule_found: &mut bool,
) -> LoadResult<()> {
    if node.tag()? != b"Rule" {
        return Ok(());
    }

    check_duplicates(node)?;

    let mut name = Vec::new();
    let mut colorspace = Vec::new();
    let mut pattern = Vec::new();
    let mut extension = Vec::new();
    let mut regex = Vec::new();
    let mut key_vals = Vec::new();

    for iter in node.iter() {
        let key = iter.first.as_::<Vec<u8>>()?;
        if iter.second.is_null()? || !iter.second.is_defined() {
            continue;
        }
        let value = &iter.second;
        match key.as_slice() {
            b"name" => name = load_string(value)?,
            b"colorspace" => colorspace = load_string(value)?,
            b"pattern" => pattern = load_string(value)?,
            b"extension" => extension = load_string(value)?,
            b"regex" => regex = load_string(value)?,
            b"custom" => key_vals = load_custom_keys(value, "file_rules custom attribute")?,
            _ => log_unknown_key_warning(node, &iter.first)?,
        }
    }

    let mut set = || -> LoadResult<()> {
        let pos = fr.num_entries() - 1;
        if strcasecmp(c_str(&name), FileRules::DEFAULT_RULE_NAME).is_eq() {
            if !regex.is_empty() || !pattern.is_empty() || !extension.is_empty() {
                return Err(Exception::new(format!(
                    "'{}' rule can't use pattern, extension or regex.",
                    FileRules::DEFAULT_RULE_NAME
                ))
                .into());
            }
            if colorspace.is_empty() {
                return Err(Exception::new(format!(
                    "'{}' rule cannot have an empty color space name.",
                    FileRules::DEFAULT_RULE_NAME
                ))
                .into());
            }
            *default_rule_found = true;
            fr.set_color_space(pos, c_str(&colorspace))?;
        } else if strcasecmp(c_str(&name), FileRules::FILE_PATH_SEARCH_RULE_NAME).is_eq() {
            if !regex.is_empty() || !pattern.is_empty() || !extension.is_empty() {
                return Err(Exception::new(format!(
                    "'{}' rule can't use pattern, extension or regex.",
                    FileRules::FILE_PATH_SEARCH_RULE_NAME
                ))
                .into());
            }
            fr.insert_path_search_rule(pos)?;
        } else {
            if !regex.is_empty() && (!pattern.is_empty() || !extension.is_empty()) {
                let mut oss = b"File rule '".to_vec();
                oss.extend_from_slice(&name);
                oss.extend_from_slice(b"' can't use regex '");
                oss.extend_from_slice(&regex);
                oss.extend_from_slice(b"' and pattern & extension '");
                oss.extend_from_slice(&pattern);
                oss.extend_from_slice(b"' '");
                oss.extend_from_slice(&extension);
                oss.extend_from_slice(b"'.");
                return Err(Exception::new(oss).into());
            }
            if colorspace.is_empty() {
                let mut oss = b"File rule '".to_vec();
                oss.extend_from_slice(&name);
                oss.extend_from_slice(b"' cannot have an empty color space name.");
                return Err(Exception::new(oss).into());
            }
            if regex.is_empty() {
                fr.insert_rule(
                    pos,
                    c_str(&name),
                    c_str(&colorspace),
                    c_str(&pattern),
                    c_str(&extension),
                )?;
            } else {
                fr.insert_regex_rule(pos, c_str(&name), c_str(&colorspace), c_str(&regex))?;
            }
        }
        for (key, value) in &key_vals {
            let key = key.as_::<Vec<u8>>()?;
            let value = value.as_::<Vec<u8>>()?;
            fr.set_custom_key(pos, c_str(&key), c_str(&value))?;
        }
        Ok(())
    };
    set().map_err(|e| rule_error(node, b"File rules: ", e))
}

/// A viewing rule's map, appended to `vr`. A node not tagged `!<Rule>` is left alone. Its
/// `colorspaces` and `encodings` are lists, or one name. The setters' errors become "Viewing
/// rules: <what()>" through [`throw_error`].
///
/// Port of `load(const YAML::Node&, ViewingRulesRcPtr&)` (OCIOYaml.cpp:4251-4338 @ v2.5.2).
pub(crate) fn load_viewing_rule(node: &Node, vr: &mut ViewingRules) -> LoadResult<()> {
    if node.tag()? != b"Rule" {
        return Ok(());
    }

    let mut name = Vec::new();
    let mut colorspaces = Vec::new();
    let mut encodings = Vec::new();
    let mut key_vals = Vec::new();

    for iter in node.iter() {
        let key = iter.first.as_::<Vec<u8>>()?;
        if iter.second.is_null()? || !iter.second.is_defined() {
            continue;
        }
        let value = &iter.second;
        match key.as_slice() {
            b"name" => name = load_string(value)?,
            b"colorspaces" => {
                if value.node_type()? == NodeType::Sequence {
                    colorspaces = load_string_vec(value)?;
                } else {
                    // If a single value is supplied...
                    colorspaces.push(load_string(value)?);
                }
            }
            b"encodings" => {
                if value.node_type()? == NodeType::Sequence {
                    encodings = load_string_vec(value)?;
                } else {
                    // If a single value is supplied...
                    encodings.push(load_string(value)?);
                }
            }
            b"custom" => {
                key_vals = load_custom_keys(value, "viewing_rules custom attribute")?;
            }
            _ => log_unknown_key_warning(node, &iter.first)?,
        }
    }

    let mut set = || -> LoadResult<()> {
        let pos = vr.num_entries();
        vr.insert_rule(pos, c_str(&name))?;
        for cs in &colorspaces {
            vr.add_color_space(pos, c_str(cs))?;
        }
        for is in &encodings {
            vr.add_encoding(pos, c_str(is))?;
        }
        for (key, value) in &key_vals {
            let key = key.as_::<Vec<u8>>()?;
            let value = value.as_::<Vec<u8>>()?;
            vr.set_custom_key(pos, c_str(&key), c_str(&value))?;
        }
        Ok(())
    };
    set().map_err(|e| rule_error(node, b"Viewing rules: ", e))
}

/// A view's map, into `v` (fresh, all fields empty). A node not tagged `!<View>` is left
/// alone. The fields take the strings whole (a NUL included; the config's setters then read
/// them up to it), and the description keeps its trailing newlines (I-144). A view needs a
/// name, and either a color space or a view transform with a display color space.
///
/// Port of `load(const YAML::Node&, View&)` (OCIOYaml.cpp:409-478 @ v2.5.2).
pub(crate) fn load_view(node: &Node, v: &mut View) -> LoadResult<()> {
    if node.tag()? != b"View" {
        return Ok(());
    }

    check_duplicates(node)?;

    let mut expecting_scene_cs = false;
    let mut expecting_display_cs = false;

    for iter in node.iter() {
        let key = iter.first.as_::<Vec<u8>>()?;
        if iter.second.is_null()? || !iter.second.is_defined() {
            continue;
        }
        let value = &iter.second;
        match key.as_slice() {
            b"name" => v.name = load_string(value)?,
            b"view_transform" => {
                expecting_display_cs = true;
                v.view_transform = load_string(value)?;
            }
            b"colorspace" => {
                expecting_scene_cs = true;
                v.colorspace = load_string(value)?;
            }
            b"display_colorspace" => {
                expecting_display_cs = true;
                v.colorspace = load_string(value)?;
            }
            b"looks" | b"look" => v.looks = load_string(value)?,
            b"rule" => v.rule = load_string(value)?,
            b"description" => v.description = load_string(value)?,
            _ => log_unknown_key_warning(node, &iter.first)?,
        }
    }
    if v.name.is_empty() {
        return Err(throw_error(node, b"View does not specify 'name'."));
    }
    if expecting_display_cs == expecting_scene_cs {
        let mut os = b"View '".to_vec();
        os.extend_from_slice(&v.name);
        os.extend_from_slice(
            b"' must specify colorspace or view_transform and display_colorspace.",
        );
        return Err(throw_error(node, c_str(&os)));
    }
    if v.colorspace.is_empty() {
        let mut os = b"View '".to_vec();
        os.extend_from_slice(&v.name);
        os.extend_from_slice(b"' does not specify colorspace.");
        return Err(throw_error(node, c_str(&os)));
    }
    Ok(())
}

/// `std::stoi(str)`: `strtol` in base 10 (white space skipped, a sign, digits), or `None`
/// where `stoi` throws: no digits (`std::invalid_argument`) or a value outside `int`
/// (`std::out_of_range`). Both wheels' C++ libraries agree.
fn stoi(s: &[u8]) -> Option<i32> {
    let s = c_str(s);
    let mut i = 0;
    // isspace in the C locale: space, \t, \n, \v, \f, \r.
    while i < s.len() && matches!(s[i], b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r') {
        i += 1;
    }
    let negative = match s.get(i) {
        Some(b'-') => {
            i += 1;
            true
        }
        Some(b'+') => {
            i += 1;
            false
        }
        _ => false,
    };
    let start = i;
    let mut value: i64 = 0;
    while i < s.len() && s[i].is_ascii_digit() {
        value = value
            .saturating_mul(10)
            .saturating_add(i64::from(s[i] - b'0'));
        i += 1;
    }
    if i == start {
        return None;
    }
    let value = if negative { -value } else { value };
    i32::try_from(value).ok()
}

/// The configs that report their errors without a file name: the config read from an I/O
/// proxy (`Config::Impl::Read(std::istream&, ConfigIOProxyRcPtr)`).
const FROM_ARCHIVE: &str = "from Archive/ConfigIOProxy";

/// Whether `filename` names the config's file: given, not empty, and not [`FROM_ARCHIVE`].
fn names_a_file(filename: Option<&[u8]>) -> Option<&[u8]> {
    filename.filter(|f| !c_str(f).is_empty() && strcasecmp(c_str(f), FROM_ARCHIVE).is_ne())
}

/// The config's document, into `config` (a new config): the profile version first, then
/// each key in its order, then the file rules' default, the working directory (the file's
/// directory, when `filename` names a file) and the environment. `filename` is upstream's
/// `const char *`, `None` for a null pointer (a config read from a stream).
///
/// Port of `load(const YAML::Node&, ConfigRcPtr&, const char*)` (OCIOYaml.cpp:4398-5031 @
/// v2.5.2).
pub(crate) fn load_config(
    node: &Node,
    config: &mut Config,
    filename: Option<&[u8]>,
) -> LoadResult<()> {
    // check profile version
    let mut profile_major_version: i32 = 0;
    let mut profile_minor_version: i32 = 0;

    let version_node = node.get("ocio_profile_version")?;
    let mut faulty_version = !version_node.is_defined();

    let mut version = Vec::new();

    if !faulty_version {
        version = load_string(&version_node)?;

        let results = split(&version, b'.');

        let parsed = match results.len() {
            1 => stoi(&results[0]).map(|major| (major, 0)),
            2 => stoi(&results[0]).zip(stoi(&results[1])),
            _ => None,
        };
        match parsed {
            Some((major, minor)) => {
                profile_major_version = major;
                profile_minor_version = minor;
            }
            None => faulty_version = true,
        }
    }

    if faulty_version {
        let mut os = b"The specified OCIO configuration file ".to_vec();
        match filename.filter(|f| !c_str(f).is_empty()) {
            Some(f) => os.extend_from_slice(c_str(f)),
            None => os.extend_from_slice(b"<null>"),
        }
        os.extend_from_slice(b" does not appear to have a valid version ");
        if version.is_empty() {
            os.extend_from_slice(b"<null>");
        } else {
            os.extend_from_slice(&version);
        }
        os.extend_from_slice(b".");

        return Err(throw_error(node, &os));
    }

    if let Err(ex) = config.set_version(profile_major_version as u32, profile_minor_version as u32)
    {
        let mut os = b"This .ocio config ".to_vec();
        if let Some(f) = filename.filter(|f| !c_str(f).is_empty()) {
            os.extend_from_slice(b" '");
            os.extend_from_slice(c_str(f));
            os.extend_from_slice(b"' ");
        }
        os.extend_from_slice(
            format!("is version {profile_major_version}.{profile_minor_version}. ").as_bytes(),
        );
        os.extend_from_slice(
            format!(
                "This version of the OpenColorIO library ({}) is not able to load that config \
                 version.",
                crate::version()
            )
            .as_bytes(),
        );
        os.push(b'\n');
        os.extend_from_slice(ex.what());

        return Err(Exception::new(os).into());
    }

    let mut file_rules_found = false;
    let mut default_file_rule_found = false;
    let mut file_rules = (*config.file_rules().get()).clone();

    check_duplicates(node)?;

    let mut mode = EnvironmentMode::LoadAll;

    for iter in node.iter() {
        let key = iter.first.as_::<Vec<u8>>()?;
        if iter.second.is_null()? || !iter.second.is_defined() {
            continue;
        }
        let value = &iter.second;
        match key.as_slice() {
            b"ocio_profile_version" => {} // Already handled above.
            b"environment" => {
                mode = EnvironmentMode::LoadPredefined;
                if value.node_type()? != NodeType::Map {
                    return Err(throw_value_error(
                        node.tag()?,
                        &iter.first,
                        b"The value type of key 'environment' needs to be a map.",
                    ));
                }
                for it in value.iter() {
                    let k = it.first.as_::<Vec<u8>>()?;
                    let v = it.second.as_::<Vec<u8>>()?;
                    config.add_environment_var(c_str(&k), Some(c_str(&v)));
                }
            }
            b"search_path" | b"resource_path" => {
                if value.size()? == 0 {
                    let stringval = load_string(value)?;
                    config.set_search_path(c_str(&stringval));
                } else {
                    for path in load_string_vec(value)? {
                        config.add_search_path(c_str(&path));
                    }
                }
            }
            b"strictparsing" => config.set_strict_parsing_enabled(load_bool(value)?),
            // Read as a description: its trailing newlines go (docs/improvements.md, I-145).
            b"name" => config.set_name(c_str(&load_description(value)?)),
            b"family_separator" => {
                // Check that the key is not present in a v1 config (checkVersionConsistency
                // is not able to detect this).
                if config.major_version() < 2 {
                    return Err(throw_error(
                        &iter.first,
                        b"Config v1 can't have 'family_separator'.",
                    ));
                }

                let stringval = load_string(value)?;
                if stringval.len() != 1 {
                    let mut os =
                        b"'family_separator' value must be a single character. Found '".to_vec();
                    os.extend_from_slice(&stringval);
                    os.extend_from_slice(b"'.");
                    return Err(throw_value_error(node.tag()?, &iter.first, &os));
                }
                config.set_family_separator(stringval[0])?;
            }
            b"description" => config.set_description(c_str(&load_description(value)?)),
            b"luma" => {
                let val = load_vec_f64(value)?;
                let Ok(luma) = <[f64; 3]>::try_from(val.as_slice()) else {
                    let os = format!("'luma' values must be 3 floats. Found '{}'.", val.len());
                    return Err(throw_value_error(node.tag()?, &iter.first, os.as_bytes()));
                };
                config.set_default_luma_coefs(&luma);
            }
            b"roles" => {
                if value.node_type()? != NodeType::Map {
                    return Err(throw_value_error(
                        node.tag()?,
                        &iter.first,
                        b"The value type of the key 'roles' needs to be a map.",
                    ));
                }
                for it in value.iter() {
                    let k = it.first.as_::<Vec<u8>>()?;
                    let v = it.second.as_::<Vec<u8>>()?;
                    config.set_role(c_str(&k), Some(c_str(&v)))?;
                }
            }
            b"file_rules" => {
                // Check that the key is not present in a v1 config (checkVersionConsistency
                // is not able to detect this).
                if config.major_version() < 2 {
                    return Err(throw_error(
                        &iter.first,
                        b"Config v1 can't use 'file_rules'",
                    ));
                }

                if value.node_type()? != NodeType::Sequence {
                    return Err(throw_error(
                        value,
                        b"The 'file_rules' field needs to be a (- !<Rule>) list.",
                    ));
                }

                for i in 0..value.size()? {
                    let val = value.get(i)?;

                    if val.tag()? == b"Rule" {
                        if default_file_rule_found {
                            return Err(throw_error(
                                value,
                                b"The 'file_rules' Default rule has to be the last rule.",
                            ));
                        }
                        load_file_rule(&val, &mut file_rules, &mut default_file_rule_found)?;
                    } else {
                        let mut os = b"Unknown element found in file_rules:".to_vec();
                        os.extend_from_slice(val.tag()?);
                        os.extend_from_slice(b". Only Rule(s) are currently handled.");
                        log_warning(os);
                    }
                }

                if !default_file_rule_found {
                    return Err(throw_error(
                        &iter.first,
                        b"The 'file_rules' does not contain a Default <Rule>.",
                    ));
                }
                file_rules_found = true;
            }
            b"viewing_rules" => {
                if value.node_type()? != NodeType::Sequence {
                    return Err(throw_error(
                        value,
                        b"The 'viewing_rules' field needs to be a (- !<Rule>) list.",
                    ));
                }

                let mut viewing_rules = ViewingRules::new();

                for i in 0..value.size()? {
                    let val = value.get(i)?;

                    if val.tag()? == b"Rule" {
                        load_viewing_rule(&val, &mut viewing_rules)?;
                    } else {
                        let mut os = b"Unknown element found in viewing_rules:".to_vec();
                        os.extend_from_slice(val.tag()?);
                        os.extend_from_slice(b". Only Rule(s) are currently handled.");
                        log_warning(os);
                    }
                }

                config.set_viewing_rules(&viewing_rules);
            }
            b"shared_views" => {
                if value.node_type()? != NodeType::Sequence {
                    return Err(throw_value_error(
                        node.tag()?,
                        &iter.first,
                        b"The view list is a sequence.",
                    ));
                }

                for i in 0..value.size()? {
                    let val = value.get(i)?;

                    let mut view = View::default();
                    load_view(&val, &mut view)?;
                    config.add_shared_view(
                        c_str(&view.name),
                        c_str(&view.view_transform),
                        c_str(&view.colorspace),
                        c_str(&view.looks),
                        c_str(&view.rule),
                        c_str(&view.description),
                    )?;
                }
            }
            b"displays" => {
                if value.node_type()? != NodeType::Map {
                    return Err(throw_value_error(
                        node.tag()?,
                        &iter.first,
                        b"The value type of the key 'displays' needs to be a map.",
                    ));
                }
                for it in value.iter() {
                    let display = it.first.as_::<Vec<u8>>()?;

                    if it.second.node_type()? != NodeType::Sequence {
                        return Err(throw_value_error(
                            node.tag()?,
                            &iter.first,
                            b"The view list is a sequence.",
                        ));
                    }

                    for i in 0..it.second.size()? {
                        let n = it.second.get(i)?;

                        if n.tag()? == b"View" {
                            let mut view = View::default();
                            load_view(&n, &mut view)?;
                            config.add_display_view_with_view_transform(
                                c_str(&display),
                                c_str(&view.name),
                                c_str(&view.view_transform),
                                c_str(&view.colorspace),
                                c_str(&view.looks),
                                c_str(&view.rule),
                                c_str(&view.description),
                            )?;
                        } else if n.tag()? == b"Views" {
                            for shared_view in load_string_vec(&n)? {
                                config.add_display_shared_view(
                                    c_str(&display),
                                    c_str(&shared_view),
                                )?;
                            }
                        }
                    }
                }
            }
            b"virtual_display" => {
                if value.node_type()? != NodeType::Sequence {
                    return Err(throw_value_error(
                        node.tag()?,
                        &iter.first,
                        b"The view list is a sequence.",
                    ));
                }

                for i in 0..value.size()? {
                    let val = value.get(i)?;

                    if val.tag()? == b"View" {
                        let mut view = View::default();
                        load_view(&val, &mut view)?;
                        config.add_virtual_display_view(
                            c_str(&view.name),
                            c_str(&view.view_transform),
                            c_str(&view.colorspace),
                            c_str(&view.looks),
                            c_str(&view.rule),
                            c_str(&view.description),
                        )?;
                    } else if val.tag()? == b"Views" {
                        for shared_view in load_string_vec(&val)? {
                            config.add_virtual_display_shared_view(c_str(&shared_view))?;
                        }
                    } else {
                        let mut os = b"Unknown element found in virtual_display:".to_vec();
                        os.extend_from_slice(val.tag()?);
                        os.extend_from_slice(b".");
                        log_warning(os);
                    }
                }
            }
            b"active_displays" => {
                let displays = join_string_env_style(&load_string_vec(value)?);
                config.set_active_displays(c_str(&displays))?;
            }
            b"active_views" => {
                let views = join_string_env_style(&load_string_vec(value)?);
                config.set_active_views(c_str(&views))?;
            }
            b"inactive_colorspaces" => {
                let inactive = join_string_env_style(&load_string_vec(value)?);
                config.set_inactive_color_spaces(c_str(&inactive));
            }
            b"colorspaces" | b"display_colorspaces" => {
                let (reference, list_error): (_, &[u8]) = if key == b"colorspaces" {
                    (
                        ReferenceSpaceType::Scene,
                        b"'colorspaces' field needs to be a (- !<ColorSpace>) list.",
                    )
                } else {
                    (
                        ReferenceSpaceType::Display,
                        b"'display_colorspaces' field needs to be a (- !<ColorSpace>) list.",
                    )
                };
                if value.node_type()? != NodeType::Sequence {
                    return Err(throw_error(value, list_error));
                }

                for i in 0..value.size()? {
                    let val = value.get(i)?;

                    if val.tag()? == b"ColorSpace" {
                        let mut cs = ColorSpace::with_reference_space(reference);
                        load_color_space(&val, &mut cs, config.major_version())?;
                        for ii in 0..config.num_color_spaces() {
                            if config.color_space_name_by_index(ii) == cs.name() {
                                let mut os = b"Colorspace with name '".to_vec();
                                os.extend_from_slice(cs.name());
                                os.extend_from_slice(b"' already defined.");
                                return Err(throw_error(value, &os));
                            }
                        }
                        config.add_color_space(&cs)?;
                    } else {
                        let mut os = b"Unknown element found in colorspaces:".to_vec();
                        os.extend_from_slice(val.tag()?);
                        os.extend_from_slice(b". Only ColorSpace(s) currently handled.");
                        log_warning(os);
                    }
                }
            }
            b"looks" => {
                if value.node_type()? != NodeType::Sequence {
                    return Err(throw_error(
                        value,
                        b"'looks' field needs to be a (- !<Look>) list.",
                    ));
                }

                for i in 0..value.size()? {
                    let val = value.get(i)?;

                    if val.tag()? == b"Look" {
                        let mut look = Look::new();
                        load_look(&val, &mut look)?;
                        config.add_look(&look)?;
                    } else {
                        let mut os = b"Unknown element found in looks:".to_vec();
                        os.extend_from_slice(val.tag()?);
                        os.extend_from_slice(b". Only Look(s) currently handled.");
                        log_warning(os);
                    }
                }
            }
            b"view_transforms" => {
                if value.node_type()? != NodeType::Sequence {
                    return Err(throw_error(
                        value,
                        b"'view_transforms' field needs to be a (- !<ViewTransform>) list.",
                    ));
                }

                for i in 0..value.size()? {
                    let val = value.get(i)?;

                    if val.tag()? == b"ViewTransform" {
                        let rst = peek_view_transform_reference_space(&val)?;
                        let mut vt = ViewTransform::new(rst);
                        load_view_transform(&val, &mut vt)?;
                        config.add_view_transform(&vt)?;
                    } else {
                        let mut os = b"Unknown element found in view_transforms:".to_vec();
                        os.extend_from_slice(val.tag()?);
                        os.extend_from_slice(b". Only ViewTransform(s) currently handled.");
                        log_warning(os);
                    }
                }
            }
            b"default_view_transform" => {
                let stringval = load_string(value)?;
                config.set_default_view_transform_name(c_str(&stringval));
            }
            b"named_transforms" => {
                if value.node_type()? != NodeType::Sequence {
                    return Err(throw_error(
                        value,
                        b"'named_transforms' field needs to be a (- !<NamedTransform>) list.",
                    ));
                }

                for i in 0..value.size()? {
                    let val = value.get(i)?;

                    if val.tag()? == b"NamedTransform" {
                        let mut nt = NamedTransform::new();
                        load_named_transform(&val, &mut nt)?;
                        // Upstream tests `if (nt->getName())`, a C string never null.
                        // Test that the name transform definitions are unique.
                        if config.named_transform(nt.name()).is_some() {
                            let mut oss =
                                b"NamedTransform: There is already one NamedTransform named: '"
                                    .to_vec();
                            oss.extend_from_slice(nt.name());
                            oss.extend_from_slice(b"'.");
                            return Err(Exception::new(oss).into());
                        }
                        // Will throw if name is empty.
                        config.add_named_transform(&nt)?;
                    } else {
                        let mut os = b"Unknown element found in named_transforms:".to_vec();
                        os.extend_from_slice(val.tag()?);
                        os.extend_from_slice(b". Only NamedTransform(s) currently handled.");
                        log_warning(os);
                    }
                }
            }
            _ => log_unknown_key_warning_in(b"profile", &iter.first)?,
        }
    }

    // Do not set the working dir when the filename is empty or contains the special string
    // "from Archive/ConfigIOProxy".
    if let Some(f) = names_a_file(filename) {
        let realfilename = abs_path(c_str(f))?;
        let configrootdir = os_path::dirname(&realfilename);
        config.set_working_dir(c_str(&configrootdir));
    }

    if !file_rules_found {
        if config.major_version() >= 2 {
            if !config.has_role(ROLE_DEFAULT) {
                // Note that no validation of the default color space is done (e.g. to check that
                // it exists in the config) in order to enable loading configs that are only
                // partially complete. The caller may use config->validate() after, if desired.
                return Err(throw_error(
                    node,
                    b"The config must contain either a Default file rule or the 'default' role.",
                ));
            }
        } else {
            // In order to use Config::getColorSpaceFromFilepath() method for any version of
            // config instance, the method updates the in-memory file rules created by a v1
            // config to have valid file rules and most importantly, to mimic
            // Config::parseColorSpaceFromString() which is now deprecated since v2.
            update_file_rules_from_v1_to_v2(config, &mut file_rules)?;

            config.set_file_rules(&file_rules);
        }
    } else {
        // If default role is also defined.
        if let Some(default_cs) = config.color_space(ROLE_DEFAULT) {
            let default_rule = file_rules.num_entries() - 1;
            let default_rule_cs = file_rules.color_space(default_rule)?;
            if default_rule_cs != ROLE_DEFAULT.as_bytes() && default_rule_cs != default_cs.name() {
                let mut oss = b"file_rules: defines a default rule using color-space '".to_vec();
                oss.extend_from_slice(&default_rule_cs);
                oss.extend_from_slice(b"' that does not match the default role '");
                oss.extend_from_slice(default_cs.name());
                oss.extend_from_slice(b"'.");
                log_warning(oss);
            }
        }
        config.set_file_rules(&file_rules);
    }

    config.set_environment_mode(mode);
    config.load_environment();

    if mode == EnvironmentMode::LoadAll {
        let mut os = b"This .ocio config ".to_vec();
        if let Some(f) = filename.filter(|f| !c_str(f).is_empty()) {
            os.extend_from_slice(b" '");
            os.extend_from_slice(c_str(f));
            os.extend_from_slice(b"' ");
        }
        os.extend_from_slice(
            format!(
                "has no environment section defined. The default behaviour is to load all \
                 environment variables ({}), which reduces the efficiency of OCIO's caching. \
                 Consider predefining the environment variables used.",
                config.num_environment_vars()
            )
            .as_bytes(),
        );

        log_debug(os);
    }
    Ok(())
}

/// Reads a config from `input` into `config`: yaml-cpp's document, then [`load_config`].
/// What either throws becomes "Error: Loading the OCIO profile failed. <what()>" (with the
/// file's name in quotes after "profile " when `filename` names a file); `what()` is read as
/// a C string.
///
/// Port of `OCIOYaml::Read` (OCIOYaml.cpp:5420-5439 @ v2.5.2).
pub(crate) fn read(
    input: &[u8],
    config: &mut Config,
    filename: Option<&[u8]>,
) -> ocio_ops::exception::Result<()> {
    let loaded = load(input)
        .map_err(LoadError::from)
        .and_then(|node| load_config(&node, config, filename));
    loaded.map_err(|e| {
        let mut os = b"Error: Loading the OCIO profile ".to_vec();
        if let Some(f) = names_a_file(filename) {
            os.extend_from_slice(b"'");
            os.extend_from_slice(c_str(f));
            os.extend_from_slice(b"' ");
        }
        os.extend_from_slice(b"failed. ");
        os.extend_from_slice(&e.what());
        Exception::new(os)
    })
}

// The writer (WP 3.7) ////////////////////////////////////////////////////////////////////////

/// What a saver gives: nothing, or the exception that stopped it.
pub(crate) type SaveResult = ocio_ops::exception::Result<()>;

/// The allocation's name.
///
/// Port of `save(YAML::Emitter&, Allocation)` (OCIOYaml.cpp:183-186 @ v2.5.2).
fn save_allocation(out: &mut Emitter, alloc: Allocation) {
    out.put(allocation_to_string(alloc));
}

/// The direction's name.
///
/// Port of `save(YAML::Emitter&, TransformDirection)` (OCIOYaml.cpp:196-199 @ v2.5.2).
fn save_direction(out: &mut Emitter, dir: TransformDirection) {
    out.put(transform_direction_to_string(dir));
}

/// The interpolation's name.
///
/// Port of `save(YAML::Emitter&, Interpolation)` (OCIOYaml.cpp:208-211 @ v2.5.2).
fn save_interpolation(out: &mut Emitter, interp: Interpolation) {
    out.put(interpolation_to_string(interp));
}

/// The transform's `direction` key, for an inverse transform only.
///
/// Port of `EmitBaseTransformKeyValues` (OCIOYaml.cpp:509-522 @ v2.5.2).
fn emit_base_transform_key_values(out: &mut Emitter, direction: TransformDirection) {
    match direction {
        TransformDirection::Forward => {}
        TransformDirection::Inverse => {
            out.put(Key).put("direction");
            out.put(Value).put(Flow);
            save_direction(out, direction);
        }
    }
}

/// The transform's `name` key, when its format metadata has a name.
///
/// Port of `EmitTransformName` (OCIOYaml.cpp:524-533 @ v2.5.2).
fn emit_transform_name(out: &mut Emitter, metadata: &FormatMetadataImpl) {
    let name = metadata.get_name();
    if !name.is_empty() {
        out.put(Key).put("name").put(Value).put(name);
    }
}

/// Port of `save(YAML::Emitter&, ConstAllocationTransformRcPtr)` (OCIOYaml.cpp:577-596 @
/// v2.5.2).
fn save_allocation_transform(out: &mut Emitter, t: &AllocationTransform) {
    out.put(verbatim_tag("AllocationTransform"));
    out.put(Flow).put(BeginMap);

    out.put(Key).put("allocation");
    out.put(Value).put(Flow);
    save_allocation(out, t.allocation());

    if t.num_vars() > 0 {
        out.put(Key).put("vars");
        out.put(Flow).put(Value).put(t.vars());
    }

    emit_base_transform_key_values(out, t.direction());
    out.put(EndMap);
}

/// Port of `save(YAML::Emitter&, const ConstBuiltinTransformRcPtr&)` (OCIOYaml.cpp:631-642 @
/// v2.5.2).
fn save_builtin(out: &mut Emitter, t: &BuiltinTransform) {
    out.put(verbatim_tag("BuiltinTransform"));
    out.put(Flow).put(BeginMap);

    out.put(Key).put("style");
    out.put(Value).put(Flow).put(t.style());

    emit_base_transform_key_values(out, t.direction());

    out.put(EndMap);
}

/// The slope, offset and power that aren't their defaults (compared in float precision,
/// I-20), the saturation, the style, and the name from version 2.
///
/// Port of `save(YAML::Emitter&, ConstCDLTransformRcPtr, unsigned int)` (OCIOYaml.cpp:728-774
/// @ v2.5.2).
fn save_cdl(out: &mut Emitter, t: &CdlTransform, major_version: u32) {
    out.put(verbatim_tag("CDLTransform"));
    out.put(Flow).put(BeginMap);

    if major_version >= 2 {
        emit_transform_name(out, t.format_metadata());
    }

    let slope = t.slope();
    if !is_vec_equal_to_one(&slope) {
        out.put(Key).put("slope");
        out.put(Value).put(Flow).put(&slope[..]);
    }

    let offset = t.offset();
    if !is_vec_equal_to_zero(&offset) {
        out.put(Key).put("offset");
        out.put(Value).put(Flow).put(&offset[..]);
    }

    let power = t.power();
    if !is_vec_equal_to_one(&power) {
        out.put(Key).put("power");
        out.put(Value).put(Flow).put(&power[..]);
    }

    if !is_scalar_equal_to_one(t.sat()) {
        out.put(Key).put("sat").put(Value).put(t.sat());
    }

    if t.style() != CdlStyle::TRANSFORM_DEFAULT {
        out.put(Key)
            .put("style")
            .put(Value)
            .put(cdl_style_to_string(t.style()));
    }

    emit_base_transform_key_values(out, t.direction());
    out.put(EndMap);
}

/// Port of `save(YAML::Emitter&, ConstColorSpaceTransformRcPtr)` (OCIOYaml.cpp:821-836 @
/// v2.5.2).
fn save_color_space_transform(out: &mut Emitter, t: &ColorSpaceTransform) {
    out.put(verbatim_tag("ColorSpaceTransform"));
    out.put(Flow).put(BeginMap);
    out.put(Key).put("src").put(Value).put(c_str(t.src()));
    out.put(Key).put("dst").put(Value).put(c_str(t.dst()));
    let bypass = t.data_bypass();
    if !bypass {
        // NB: Will log a warning if read by a v1 library.
        out.put(Key).put("data_bypass").put(Value).put(bypass);
    }

    emit_base_transform_key_values(out, t.direction());
    out.put(EndMap);
}

/// Port of `save(YAML::Emitter&, ConstDisplayViewTransformRcPtr)` (OCIOYaml.cpp:893-913 @
/// v2.5.2).
fn save_display_view(out: &mut Emitter, t: &DisplayViewTransform) {
    out.put(verbatim_tag("DisplayViewTransform"));
    out.put(Flow).put(BeginMap);
    out.put(Key).put("src").put(Value).put(c_str(t.src()));
    out.put(Key)
        .put("display")
        .put(Value)
        .put(c_str(t.display()));
    out.put(Key).put("view").put(Value).put(c_str(t.view()));
    let looks_bypass = t.looks_bypass();
    if looks_bypass {
        out.put(Key)
            .put("looks_bypass")
            .put(Value)
            .put(looks_bypass);
    }
    let data_bypass = t.data_bypass();
    if !data_bypass {
        out.put(Key).put("data_bypass").put(Value).put(data_bypass);
    }

    emit_base_transform_key_values(out, t.direction());
    out.put(EndMap);
}

/// The value as one number from version 2 when the RGB values are equal and alpha is 1, else
/// the four; the negative style unless it's `clamp`; the name from version 2.
///
/// Port of `save(YAML::Emitter&, ConstExponentTransformRcPtr, unsigned int)`
/// (OCIOYaml.cpp:979-1014 @ v2.5.2).
fn save_exponent(out: &mut Emitter, t: &ExponentTransform, major_version: u32) {
    out.put(verbatim_tag("ExponentTransform"));
    out.put(Flow).put(BeginMap);

    if major_version >= 2 {
        emit_transform_name(out, t.format_metadata());
    }

    let value = t.value();
    if major_version >= 2 && value[0] == value[1] && value[0] == value[2] && value[3] == 1.0 {
        out.put(Key).put("value").put(Value).put(value[0]);
    } else {
        out.put(Key).put("value");
        out.put(Value).put(Flow).put(&value[..]);
    }

    let style = t.negative_style();
    if style != NegativeStyle::Clamp {
        // NB: Will log a warning if read by a v1 library.
        out.put(Key).put("style");
        out.put(Value)
            .put(Flow)
            .put(negative_style_to_string(style));
    }
    emit_base_transform_key_values(out, t.direction());

    out.put(EndMap);
}

/// The gamma and the offset, each as one number when the RGB values are equal and alpha is
/// the default, else the four; the negative style unless it's `linear`.
///
/// Port of `save(YAML::Emitter&, ConstExponentWithLinearTransformRcPtr)` (OCIOYaml.cpp:
/// 1142-1190 @ v2.5.2).
fn save_exponent_with_linear(out: &mut Emitter, t: &ExponentWithLinearTransform) {
    out.put(verbatim_tag("ExponentWithLinearTransform"));
    out.put(Flow).put(BeginMap);

    emit_transform_name(out, t.format_metadata());

    let gamma = t.gamma();
    if gamma[0] == gamma[1] && gamma[0] == gamma[2] && gamma[3] == 1.0 {
        out.put(Key).put("gamma").put(Value).put(gamma[0]);
    } else {
        out.put(Key).put("gamma");
        out.put(Value).put(Flow).put(&gamma[..]);
    }

    let offset = t.offset();

    if offset[0] == offset[1] && offset[0] == offset[2] && offset[3] == 0.0 {
        out.put(Key).put("offset").put(Value).put(offset[0]);
    } else {
        out.put(Key).put("offset");
        out.put(Value).put(Flow).put(&offset[..]);
    }

    // Only save style if not default
    let style = t.negative_style();
    if style != NegativeStyle::Linear {
        out.put(Key).put("style");
        out.put(Value)
            .put(Flow)
            .put(negative_style_to_string(style));
    }

    emit_base_transform_key_values(out, t.direction());
    out.put(EndMap);
}

/// The source, the CCC ID when there is one, the CDL style unless it's the default, and the
/// interpolation unless it's `default` (in version 1, `default` is written `linear`).
///
/// Port of `save(YAML::Emitter&, ConstFileTransformRcPtr, unsigned int)` (OCIOYaml.cpp:
/// 1384-1417 @ v2.5.2).
fn save_file(out: &mut Emitter, t: &FileTransform, major_version: u32) {
    out.put(verbatim_tag("FileTransform"));
    out.put(Flow).put(BeginMap);
    out.put(Key).put("src").put(Value).put(c_str(t.src()));
    let cccid = c_str(t.ccc_id());
    if !cccid.is_empty() {
        out.put(Key).put("cccid").put(Value).put(cccid);
    }
    if t.cdl_style() != CdlStyle::TRANSFORM_DEFAULT {
        // NB: Will log a warning if read by a v1 library.
        out.put(Key)
            .put("cdl_style")
            .put(Value)
            .put(cdl_style_to_string(t.cdl_style()));
    }
    let mut interp = t.interpolation();
    if major_version == 1 && interp == Interpolation::Default {
        // The DEFAULT method is not available in a v1 library.  If the v1 config is read by a v1
        // library and the file is a LUT, a missing interp would end up set to UNKNOWN and a
        // throw would happen when the processor is built.  Setting to LINEAR to provide more
        // robust compatibility.
        interp = Interpolation::Linear;
    }
    if interp != Interpolation::Default {
        out.put(Key).put("interpolation");
        out.put(Value);
        save_interpolation(out, interp);
    }

    emit_base_transform_key_values(out, t.direction());
    out.put(EndMap);
}

/// The style, a warning for the experimental ACES 2 styles, and the parameters when there are
/// some.
///
/// Port of `save(YAML::Emitter&, ConstFixedFunctionTransformRcPtr)` (OCIOYaml.cpp:1485-1517 @
/// v2.5.2).
fn save_fixed_function(out: &mut Emitter, t: &FixedFunctionTransform) -> SaveResult {
    out.put(verbatim_tag("FixedFunctionTransform"));
    out.put(Flow).put(BeginMap);

    emit_transform_name(out, t.format_metadata());

    out.put(Key).put("style");
    out.put(Value)
        .put(Flow)
        .put(fixed_function_style_to_string(t.style())?);

    let style_id = t.style();
    if matches!(
        style_id,
        FixedFunctionStyle::AcesOutputTransform20
            | FixedFunctionStyle::AcesRgbToJmh20
            | FixedFunctionStyle::AcesTonescaleCompress20
            | FixedFunctionStyle::AcesGamutCompress20
    ) {
        let mut os =
            b"FixedFunction style is experimental and may be removed in a future release: '"
                .to_vec();
        os.extend_from_slice(fixed_function_style_to_string(t.style())?.as_bytes());
        os.extend_from_slice(b"'.");
        log_warning(os);
    }

    let params = t.params();
    if !params.is_empty() {
        out.put(Key).put("params");
        out.put(Value).put(Flow).put(&params);
    }

    emit_base_transform_key_values(out, t.direction());
    out.put(EndMap);
    Ok(())
}

/// A block map: the name from version 2, the direction, then the children. It recurses once
/// per nested group, in a small frame, as printing and validating do (U-60).
///
/// Port of `save(YAML::Emitter&, ConstGroupTransformRcPtr, unsigned int)` (OCIOYaml.cpp:
/// 2558-2580 @ v2.5.2).
fn save_group(out: &mut Emitter, t: &GroupTransform, major_version: u32) -> SaveResult {
    out.put(verbatim_tag("GroupTransform"));
    out.put(BeginMap);

    if major_version >= 2 {
        emit_transform_name(out, t.format_metadata());
    }
    emit_base_transform_key_values(out, t.direction());

    out.put(Key).put("children");
    out.put(Value);

    out.put(BeginSeq);
    for i in 0..t.num_transforms() {
        save_transform(out, t.transform(i)?, major_version)?;
    }
    out.put(EndSeq);

    out.put(EndMap);
    Ok(())
}

/// A log parameter: one number when the three are equal (none when it is `default_val`; a NaN
/// default writes it always), else the three.
///
/// Port of `saveLogParam` (OCIOYaml.cpp:2687-2706 @ v2.5.2).
fn save_log_param(out: &mut Emitter, param: &[f64; 3], default_val: f64, param_name: &str) {
    // (See test in Config_test.cpp that verifies double precision is preserved.)
    if param[0] == param[1] && param[0] == param[2] {
        // Set defaultVal to NaN if there is no default value. It will always write param,
        // otherwise default params are not saved.
        if param[0] != default_val {
            out.put(Key).put(param_name).put(Value).put(param[0]);
        }
    } else {
        out.put(Key).put(param_name).put(Value).put(&param[..]);
    }
}

/// Port of `save(YAML::Emitter&, ConstLogAffineTransformRcPtr)` (OCIOYaml.cpp:2708-2736 @
/// v2.5.2).
fn save_log_affine(out: &mut Emitter, t: &LogAffineTransform) {
    out.put(verbatim_tag("LogAffineTransform"));
    out.put(Flow).put(BeginMap);

    emit_transform_name(out, t.format_metadata());

    let log_slope = t.log_side_slope_value();
    let log_offset = t.log_side_offset_value();
    let lin_slope = t.lin_side_slope_value();
    let lin_offset = t.lin_side_offset_value();

    let base_val = t.base();
    if base_val != 2.0 {
        out.put(Key).put("base").put(Value).put(base_val);
    }
    save_log_param(out, &log_slope, 1.0, "log_side_slope");
    save_log_param(out, &log_offset, 0.0, "log_side_offset");
    save_log_param(out, &lin_slope, 1.0, "lin_side_slope");
    save_log_param(out, &lin_offset, 0.0, "lin_side_offset");

    emit_base_transform_key_values(out, t.direction());
    out.put(EndMap);
}

/// Port of `save(YAML::Emitter&, ConstLogCameraTransformRcPtr)` (OCIOYaml.cpp:2836-2873 @
/// v2.5.2).
fn save_log_camera(out: &mut Emitter, t: &LogCameraTransform) {
    out.put(verbatim_tag("LogCameraTransform"));
    out.put(Flow).put(BeginMap);

    emit_transform_name(out, t.format_metadata());

    let log_slope = t.log_side_slope_value();
    let log_offset = t.log_side_offset_value();
    let lin_slope = t.lin_side_slope_value();
    let lin_offset = t.lin_side_offset_value();
    let lin_break = t.lin_side_break_value();
    let linear_slope = t.linear_slope_value();

    let base_val = t.base();
    if base_val != 2.0 {
        out.put(Key).put("base").put(Value).put(base_val);
    }
    save_log_param(out, &log_slope, 1.0, "log_side_slope");
    save_log_param(out, &log_offset, 0.0, "log_side_offset");
    save_log_param(out, &lin_slope, 1.0, "lin_side_slope");
    save_log_param(out, &lin_offset, 0.0, "lin_side_offset");
    save_log_param(out, &lin_break, f64::NAN, "lin_side_break");
    if let Some(linear_slope) = linear_slope {
        save_log_param(out, &linear_slope, f64::NAN, "linear_slope");
    }

    emit_base_transform_key_values(out, t.direction());
    out.put(EndMap);
}

/// The base, unless it's 2 in version 2; the name from version 2.
///
/// Port of `save(YAML::Emitter&, ConstLogTransformRcPtr, unsigned int)` (OCIOYaml.cpp:
/// 2925-2942 @ v2.5.2).
fn save_log(out: &mut Emitter, t: &LogTransform, major_version: u32) {
    out.put(verbatim_tag("LogTransform"));
    out.put(Flow).put(BeginMap);

    if major_version >= 2 {
        emit_transform_name(out, t.format_metadata());
    }

    let base_val = t.base();
    if base_val != 2.0 || major_version < 2 {
        out.put(Key).put("base").put(Value).put(base_val);
    }
    emit_base_transform_key_values(out, t.direction());
    out.put(EndMap);
}

/// Port of `save(YAML::Emitter&, ConstLookTransformRcPtr)` (OCIOYaml.cpp:2989-2998 @ v2.5.2).
fn save_look_transform(out: &mut Emitter, t: &LookTransform) {
    out.put(verbatim_tag("LookTransform"));
    out.put(Flow).put(BeginMap);
    out.put(Key).put("src").put(Value).put(c_str(t.src()));
    out.put(Key).put("dst").put(Value).put(c_str(t.dst()));
    out.put(Key).put("looks").put(Value).put(c_str(t.looks()));
    emit_base_transform_key_values(out, t.direction());
    out.put(EndMap);
}

/// The matrix unless it's the identity, and the offset unless it's zero (compared in float
/// precision, I-20); the name from version 2.
///
/// Port of `save(YAML::Emitter&, ConstMatrixTransformRcPtr, unsigned int)` (OCIOYaml.cpp:
/// 3059-3087 @ v2.5.2).
fn save_matrix(out: &mut Emitter, t: &MatrixTransform, major_version: u32) {
    out.put(verbatim_tag("MatrixTransform"));
    out.put(Flow).put(BeginMap);

    if major_version >= 2 {
        emit_transform_name(out, t.format_metadata());
    }

    let matrix = t.matrix();
    if !is_m44_identity(&matrix) {
        out.put(Key).put("matrix");
        out.put(Value).put(Flow).put(&matrix[..]);
    }

    let offset = t.offset();
    if !is_vec_equal_to_zero(&offset) {
        out.put(Key).put("offset");
        out.put(Value).put(Flow).put(&offset[..]);
    }

    emit_base_transform_key_values(out, t.direction());
    out.put(EndMap);
}

/// The bounds that are set, and the style unless it's `Clamp`.
///
/// Port of `save(YAML::Emitter&, ConstRangeTransformRcPtr)` (OCIOYaml.cpp:3153-3192 @
/// v2.5.2).
fn save_range(out: &mut Emitter, t: &RangeTransform) {
    out.put(verbatim_tag("RangeTransform"));
    out.put(Flow).put(BeginMap);

    emit_transform_name(out, t.format_metadata());

    if t.has_min_in_value() {
        out.put(Key).put("min_in_value");
        out.put(Value).put(Flow).put(t.min_in_value());
    }

    if t.has_max_in_value() {
        out.put(Key).put("max_in_value");
        out.put(Value).put(Flow).put(t.max_in_value());
    }

    if t.has_min_out_value() {
        out.put(Key).put("min_out_value");
        out.put(Value).put(Flow).put(t.min_out_value());
    }

    if t.has_max_out_value() {
        out.put(Key).put("max_out_value");
        out.put(Value).put(Flow).put(t.max_out_value());
    }

    if t.style() != RangeStyle::Clamp {
        out.put(Key).put("style");
        out.put(Value)
            .put(Flow)
            .put(range_style_to_string(t.style()));
    }

    emit_base_transform_key_values(out, t.direction());
    out.put(EndMap);
}

/// A transform, by its class. A class without a saver (`Lut1DTransform`) is refused with
/// "Unsupported Transform() type for serialization.".
///
/// Port of `save(YAML::Emitter&, ConstTransformRcPtr, unsigned int)` (OCIOYaml.cpp:3353-3420 @
/// v2.5.2). The ExposureContrast and grading savers come with their classes (Phase 5).
pub(crate) fn save_transform(out: &mut Emitter, t: &Transform, major_version: u32) -> SaveResult {
    match t {
        Transform::Allocation(t) => save_allocation_transform(out, t),
        Transform::Builtin(t) => save_builtin(out, t),
        Transform::Cdl(t) => save_cdl(out, t, major_version),
        Transform::ColorSpace(t) => save_color_space_transform(out, t),
        Transform::DisplayView(t) => save_display_view(out, t),
        Transform::Exponent(t) => save_exponent(out, t, major_version),
        Transform::ExponentWithLinear(t) => save_exponent_with_linear(out, t),
        Transform::File(t) => save_file(out, t, major_version),
        Transform::FixedFunction(t) => save_fixed_function(out, t)?,
        Transform::Group(t) => save_group(out, t, major_version)?,
        Transform::LogAffine(t) => save_log_affine(out, t),
        Transform::LogCamera(t) => save_log_camera(out, t),
        Transform::Log(t) => save_log(out, t, major_version),
        Transform::Look(t) => save_look_transform(out, t),
        Transform::Matrix(t) => save_matrix(out, t, major_version),
        Transform::Range(t) => save_range(out, t),
        Transform::Lut1D(_) => {
            return Err(Exception::new(
                "Unsupported Transform() type for serialization.",
            ));
        }
    }
    Ok(())
}
/// A sequence of strings, as `out << std::vector<std::string>` writes it (stlemitter.h:16-28):
/// each as its bytes.
fn put_strings(out: &mut Emitter, values: &[Vec<u8>]) {
    out.put(BeginSeq);
    for v in values {
        out.put(v.as_slice());
    }
    out.put(EndSeq);
}

/// The bit depth's name.
///
/// Port of `save(YAML::Emitter&, BitDepth)` (OCIOYaml.cpp:171-174 @ v2.5.2).
fn save_bit_depth(out: &mut Emitter, depth: BitDepth) {
    out.put(bit_depth_to_string(depth));
}

/// A non-empty description as the `description` key, without its trailing newlines, in the
/// literal style when it holds a newline. `desc` is upstream's `const char *`: up to its first
/// NUL.
///
/// Port of `saveDescription` (OCIOYaml.cpp:219-233 @ v2.5.2).
pub(crate) fn save_description(out: &mut Emitter, desc: &[u8]) {
    let desc = c_str(desc);
    if !desc.is_empty() {
        // Remove trailing newlines so that only one is saved because they won't be read back.
        let desc_str = sanitize_newlines(desc);

        out.put(Key).put("description").put(Value);
        if desc_str.contains(&b'\n') {
            out.put(Literal);
        }
        out.put(desc_str.as_slice());
    }
}

/// The interchange attributes, when there are some, as the `interchange` map: each value
/// without its trailing newlines, in the literal style when it holds a newline.
///
/// Port of `saveInterchangeAttributes` (OCIOYaml.cpp:350-373 @ v2.5.2).
pub(crate) fn save_interchange_attributes(
    out: &mut Emitter,
    interchangemap: &BTreeMap<Vec<u8>, Vec<u8>>,
) {
    if interchangemap.is_empty() {
        return;
    }

    out.put(Key).put("interchange");
    out.put(Value);
    out.put(BeginMap);
    for (key, value) in interchangemap {
        let val_str = sanitize_newlines(value);

        out.put(Key).put(key.as_slice()).put(Value);
        if val_str.contains(&b'\n') {
            out.put(Literal);
        }
        out.put(val_str.as_slice());
    }

    out.put(EndMap);
}

/// A view, a flow map: its name, its color space (or its view transform and display color
/// space), its looks and rule when set, and its description. The fields are written whole, as
/// `std::string`s.
///
/// Port of `save(YAML::Emitter&, const View&)` (OCIOYaml.cpp:480-505 @ v2.5.2).
pub(crate) fn save_view(out: &mut Emitter, view: &View) {
    out.put(verbatim_tag("View"));
    out.put(Flow);
    out.put(BeginMap);
    out.put(Key)
        .put("name")
        .put(Value)
        .put(view.name.as_slice());
    if view.view_transform.is_empty() {
        out.put(Key)
            .put("colorspace")
            .put(Value)
            .put(view.colorspace.as_slice());
    } else {
        out.put(Key)
            .put("view_transform")
            .put(Value)
            .put(view.view_transform.as_slice());
        out.put(Key)
            .put("display_colorspace")
            .put(Value)
            .put(view.colorspace.as_slice());
    }
    if !view.looks.is_empty() {
        out.put(Key)
            .put("looks")
            .put(Value)
            .put(view.looks.as_slice());
    }
    if !view.rule.is_empty() {
        out.put(Key)
            .put("rule")
            .put(Value)
            .put(view.rule.as_slice());
    }
    save_description(out, &view.description);
    out.put(EndMap);
}

/// The names of `n` items (a color space's aliases, categories, ...) up to their first NUL, as
/// upstream's `StringVec` of `const char *`s.
fn names(n: usize, item: impl Fn(usize) -> Vec<u8>) -> Vec<Vec<u8>> {
    (0..n).map(|i| c_str(&item(i)).to_vec()).collect()
}

/// A color space, a block map: name, aliases (from version 2), interop ID, family, equality
/// group, bit depth, description, `isdata`, categories, encoding, interchange attributes,
/// allocation and its variables, and its transforms under the keys of its reference space and
/// version; then a newline.
///
/// Port of `save(YAML::Emitter&, ConstColorSpaceRcPtr, unsigned int)` (OCIOYaml.cpp:3576-3663
/// @ v2.5.2).
pub(crate) fn save_color_space(
    out: &mut Emitter,
    cs: &ColorSpace,
    major_version: u32,
) -> SaveResult {
    out.put(verbatim_tag("ColorSpace"));
    out.put(BeginMap);

    out.put(Key).put("name").put(Value).put(c_str(cs.name()));
    let num_aliases = cs.num_aliases();
    if major_version >= 2 && num_aliases != 0 {
        out.put(Key).put("aliases");
        let aliases = names(num_aliases, |i| cs.alias(i).to_vec());
        out.put(Flow).put(Value);
        put_strings(out, &aliases);
    }

    let interop_id = c_str(cs.interop_id());
    if !interop_id.is_empty() {
        out.put(Key).put("interop_id");
        out.put(Value).put(interop_id);
    }

    out.put(Key)
        .put("family")
        .put(Value)
        .put(c_str(cs.family()));

    out.put(Key)
        .put("equalitygroup")
        .put(Value)
        .put(c_str(cs.equality_group()));

    out.put(Key).put("bitdepth").put(Value);
    save_bit_depth(out, cs.bit_depth());

    save_description(out, cs.description());

    out.put(Key).put("isdata").put(Value).put(cs.is_data());

    if cs.num_categories() > 0 {
        let categories = names(cs.num_categories() as usize, |i| {
            cs.category(i as i32).unwrap_or_default().to_vec()
        });
        out.put(Key).put("categories");
        out.put(Flow).put(Value);
        put_strings(out, &categories);
    }

    let is = c_str(cs.encoding());
    if !is.is_empty() {
        out.put(Key).put("encoding");
        out.put(Value).put(is);
    }

    save_interchange_attributes(out, cs.interchange_attributes());

    out.put(Key).put("allocation").put(Value);
    save_allocation(out, cs.allocation());
    if cs.allocation_num_vars() > 0 {
        out.put(Key).put("allocationvars");
        out.put(Flow).put(Value).put(cs.allocation_vars());
    }

    let is_display = cs.reference_space_type() == ReferenceSpaceType::Display;
    if let Some(toref) = cs.transform(ColorSpaceDirection::ToReference) {
        out.put(Key)
            .put(if is_display {
                "to_display_reference"
            } else if major_version < 2 {
                "to_reference"
            } else {
                "to_scene_reference"
            })
            .put(Value);
        save_transform(out, toref, major_version)?;
    }

    if let Some(fromref) = cs.transform(ColorSpaceDirection::FromReference) {
        out.put(Key)
            .put(if is_display {
                "from_display_reference"
            } else if major_version < 2 {
                "from_reference"
            } else {
                "from_scene_reference"
            })
            .put(Value);
        save_transform(out, fromref, major_version)?;
    }

    out.put(EndMap);
    out.put(Newline);
    Ok(())
}

/// A look, a block map: name, process space, description, interchange attributes and its
/// transforms; then a newline.
///
/// Port of `save(YAML::Emitter&, ConstLookRcPtr, unsigned int)` (OCIOYaml.cpp:3721-3746 @
/// v2.5.2).
pub(crate) fn save_look(out: &mut Emitter, look: &Look, major_version: u32) -> SaveResult {
    out.put(verbatim_tag("Look"));
    out.put(BeginMap);
    out.put(Key).put("name").put(Value).put(c_str(look.name()));
    out.put(Key)
        .put("process_space")
        .put(Value)
        .put(c_str(look.process_space()));
    save_description(out, look.description());
    save_interchange_attributes(out, look.interchange_attributes());

    if let Some(t) = look.transform() {
        out.put(Key).put("transform");
        out.put(Value);
        save_transform(out, t, major_version)?;
    }

    if let Some(t) = look.inverse_transform() {
        out.put(Key).put("inverse_transform");
        out.put(Value);
        save_transform(out, t, major_version)?;
    }

    out.put(EndMap);
    out.put(Newline);
    Ok(())
}

/// A view transform, a block map: name, family when set, description, interchange
/// attributes, categories, and its transforms under the keys of its reference space; then a
/// newline.
///
/// Port of `save(YAML::Emitter&, ConstViewTransformRcPtr&, unsigned int)` (OCIOYaml.cpp:
/// 3881-3923 @ v2.5.2).
pub(crate) fn save_view_transform(
    out: &mut Emitter,
    vt: &ViewTransform,
    major_version: u32,
) -> SaveResult {
    out.put(verbatim_tag("ViewTransform"));
    out.put(BeginMap);

    out.put(Key).put("name").put(Value).put(c_str(vt.name()));
    let family = c_str(vt.family());
    if !family.is_empty() {
        out.put(Key).put("family").put(Value).put(family);
    }
    save_description(out, vt.description());
    save_interchange_attributes(out, vt.interchange_attributes());

    if vt.num_categories() > 0 {
        let categories = names(vt.num_categories() as usize, |i| {
            vt.category(i as i32).unwrap_or_default().to_vec()
        });
        out.put(Key).put("categories");
        out.put(Flow).put(Value);
        put_strings(out, &categories);
    }

    let is_display = vt.reference_space_type() == ReferenceSpaceType::Display;
    if let Some(toref) = vt.transform(ViewTransformDirection::ToReference) {
        out.put(Key)
            .put(if is_display {
                "to_display_reference"
            } else {
                "to_scene_reference"
            })
            .put(Value);
        save_transform(out, toref, major_version)?;
    }

    if let Some(fromref) = vt.transform(ViewTransformDirection::FromReference) {
        out.put(Key)
            .put(if is_display {
                "from_display_reference"
            } else {
                "from_scene_reference"
            })
            .put(Value);
        save_transform(out, fromref, major_version)?;
    }

    out.put(EndMap);
    out.put(Newline);
    Ok(())
}

/// A named transform, a block map: name, aliases (from version 2), description, family,
/// categories and encoding when set, and its transforms; then a newline.
///
/// Port of `save(YAML::Emitter&, ConstNamedTransformRcPtr&, unsigned int)` (OCIOYaml.cpp:
/// 4008-4068 @ v2.5.2).
pub(crate) fn save_named_transform(
    out: &mut Emitter,
    nt: &NamedTransform,
    major_version: u32,
) -> SaveResult {
    out.put(verbatim_tag("NamedTransform"));
    out.put(BeginMap);

    out.put(Key).put("name").put(Value).put(c_str(nt.name()));

    let num_aliases = nt.num_aliases();
    if major_version >= 2 && num_aliases != 0 {
        out.put(Key).put("aliases");
        let aliases = names(num_aliases, |i| nt.alias(i).to_vec());
        out.put(Flow).put(Value);
        put_strings(out, &aliases);
    }

    save_description(out, nt.description());

    let family = c_str(nt.family());
    if !family.is_empty() {
        out.put(Key).put("family").put(Value).put(family);
    }

    if nt.num_categories() > 0 {
        let categories = names(nt.num_categories() as usize, |i| {
            nt.category(i as i32).unwrap_or_default().to_vec()
        });
        out.put(Key).put("categories");
        out.put(Flow).put(Value);
        put_strings(out, &categories);
    }

    let encoding = c_str(nt.encoding());
    if !encoding.is_empty() {
        out.put(Key).put("encoding").put(Value).put(encoding);
    }

    if let Some(t) = nt.transform(TransformDirection::Forward) {
        out.put(Key).put("transform").put(Value);
        save_transform(out, t, major_version)?;
    }

    if let Some(t) = nt.transform(TransformDirection::Inverse) {
        out.put(Key).put("inverse_transform").put(Value);
        save_transform(out, t, major_version)?;
    }

    out.put(EndMap);
    out.put(Newline);
    Ok(())
}

/// The custom keys of a rule, as the `custom` map, when there are some.
fn save_custom_keys(
    out: &mut Emitter,
    num_keys: usize,
    key: impl Fn(usize) -> ocio_ops::exception::Result<(Vec<u8>, Vec<u8>)>,
) -> SaveResult {
    if num_keys != 0 {
        out.put(Key).put("custom");
        out.put(Value);
        out.put(BeginMap);

        for i in 0..num_keys {
            let (name, value) = key(i)?;
            out.put(Key).put(c_str(&name)).put(Value).put(c_str(&value));
        }
        out.put(EndMap);
    }
    Ok(())
}

/// The file rule at `position`, a flow map: its name, and its color space, regex, pattern,
/// extension and custom keys when set.
///
/// Port of `save(YAML::Emitter&, ConstFileRulesRcPtr&, size_t)` (OCIOYaml.cpp:4206-4247 @
/// v2.5.2).
pub(crate) fn save_file_rule(out: &mut Emitter, fr: &FileRules, position: usize) -> SaveResult {
    out.put(verbatim_tag("Rule"));
    out.put(Flow);
    out.put(BeginMap);
    out.put(Key)
        .put("name")
        .put(Value)
        .put(c_str(fr.name(position)?));
    let cs = fr.color_space(position)?;
    if !c_str(&cs).is_empty() {
        out.put(Key).put("colorspace").put(Value).put(c_str(&cs));
    }
    let regex = c_str(fr.regex(position)?);
    if !regex.is_empty() {
        out.put(Key).put("regex").put(Value).put(regex);
    }
    let pattern = c_str(fr.pattern(position)?);
    if !pattern.is_empty() {
        out.put(Key).put("pattern").put(Value).put(pattern);
    }
    let extension = c_str(fr.extension(position)?);
    if !extension.is_empty() {
        out.put(Key).put("extension").put(Value).put(extension);
    }
    save_custom_keys(out, fr.num_custom_keys(position)?, |i| {
        Ok((
            fr.custom_key_name(position, i)?.to_vec(),
            fr.custom_key_value(position, i)?.to_vec(),
        ))
    })?;
    out.put(EndMap);
    Ok(())
}

/// The viewing rule at `position`, a flow map: its name, its color spaces and encodings (one
/// as a string, more as a list) and its custom keys.
///
/// Port of `save(YAML::Emitter&, ConstViewingRulesRcPtr&, size_t)` (OCIOYaml.cpp:4340-4393 @
/// v2.5.2).
pub(crate) fn save_viewing_rule(
    out: &mut Emitter,
    vr: &ViewingRules,
    position: usize,
) -> SaveResult {
    out.put(verbatim_tag("Rule"));
    out.put(Flow);
    out.put(BeginMap);
    out.put(Key)
        .put("name")
        .put(Value)
        .put(c_str(vr.name(position)?));
    let numcs = vr.num_color_spaces(position)?;
    if numcs == 1 {
        out.put(Key).put("colorspaces");
        out.put(Value)
            .put(c_str(vr.color_space(position, 0)?.unwrap_or_default()));
    } else if numcs > 1 {
        let mut colorspaces = Vec::new();
        for i in 0..numcs {
            colorspaces.push(c_str(vr.color_space(position, i)?.unwrap_or_default()).to_vec());
        }
        out.put(Key).put("colorspaces");
        out.put(Value).put(Flow);
        put_strings(out, &colorspaces);
    }
    let numenc = vr.num_encodings(position)?;
    if numenc == 1 {
        out.put(Key).put("encodings");
        out.put(Value)
            .put(c_str(vr.encoding(position, 0)?.unwrap_or_default()));
    } else if numenc > 1 {
        let mut encodings = Vec::new();
        for i in 0..numenc {
            encodings.push(c_str(vr.encoding(position, i)?.unwrap_or_default()).to_vec());
        }
        out.put(Key).put("encodings");
        out.put(Value).put(Flow);
        put_strings(out, &encodings);
    }
    save_custom_keys(out, vr.num_custom_keys(position)?, |i| {
        Ok((
            vr.custom_key_name(position, i)?.to_vec(),
            vr.custom_key_value(position, i)?.to_vec(),
        ))
    })?;
    out.put(EndMap);
    Ok(())
}

/// The view `name` of `display` (`""`: the config's shared views) as the config's getters give
/// it, each string up to its first NUL.
fn config_view(config: &Config, display: &[u8], name: &[u8]) -> View {
    View::new(
        name,
        config.display_view_transform_name(display, name),
        config.display_view_color_space_name(display, name),
        config.display_view_looks(display, name),
        config.display_view_rule(display, name),
        config.display_view_description(display, name),
    )
}

/// The config, a block map in upstream's order: the profile version; the environment (always
/// from version 2); the search path (version 1: one string; version 2: `""`, one path, or a
/// list); strict parsing; the family separator unless it's `/` (version 2); the luma; the name
/// (version 2) and description; the roles; the file rules (version 2) and viewing rules
/// (version 2, when there are some); the shared views; the displays, but those a virtual
/// display made, with their views and shared views; the virtual display (version 2); the active
/// displays and views and the inactive color spaces; the looks; the default view transform and
/// the view transforms; the display color spaces (but those a virtual display made) and the
/// color spaces; the named transforms.
///
/// Port of `save(YAML::Emitter&, const Config&)` (OCIOYaml.cpp:5033-5414 @ v2.5.2).
fn save_config(out: &mut Emitter, config: &Config) -> SaveResult {
    let config_major_version = config.major_version();
    let config_minor_version = config.minor_version();

    let mut ss = config_major_version.to_string();
    if config_minor_version != 0 {
        ss.push('.');
        ss.push_str(&config_minor_version.to_string());
    }

    out.put(Block);
    out.put(BeginMap);
    out.put(Key)
        .put("ocio_profile_version")
        .put(Value)
        .put(ss.as_str());
    out.put(Newline);
    out.put(Newline);

    if config_major_version >= 2 || config.num_environment_vars() > 0 {
        // For v2 configs, write the environment section, even if empty.
        out.put(Key).put("environment");
        out.put(Value).put(BeginMap);
        for i in 0..config.num_environment_vars() {
            let name = c_str(config.environment_var_name_by_index(i));
            out.put(Key).put(name);
            out.put(Value)
                .put(c_str(config.environment_var_default(name)));
        }
        out.put(EndMap);
        out.put(Newline);
    }

    if config_major_version < 2 {
        // Save search paths as a single string.
        out.put(Key)
            .put("search_path")
            .put(Value)
            .put(c_str(&config.search_path()));
    } else {
        let num_sp = config.num_search_paths();
        let search_paths: Vec<Vec<u8>> = (0..num_sp)
            .map(|i| c_str(&config.search_path_with_index(i)).to_vec())
            .collect();

        if num_sp == 0 {
            out.put(Key).put("search_path").put(Value).put("");
        } else if num_sp == 1 {
            out.put(Key)
                .put("search_path")
                .put(Value)
                .put(search_paths[0].as_slice());
        } else {
            out.put(Key).put("search_path").put(Value);
            put_strings(out, &search_paths);
        }
    }
    out.put(Key)
        .put("strictparsing")
        .put(Value)
        .put(config.is_strict_parsing_enabled());

    if config_major_version >= 2 {
        let family_separator = config.family_separator();
        if family_separator != b'/' {
            out.put(Key)
                .put("family_separator")
                .put(Value)
                .put(family_separator);
        }
    }

    let luma = config.default_luma_coefs();
    out.put(Key).put("luma").put(Value).put(Flow).put(&luma[..]);

    if config_major_version >= 2 {
        let name = c_str(config.name());
        if !name.is_empty() {
            out.put(Key).put("name").put(Value).put(name);
        }
    }
    save_description(out, config.description());

    // Roles
    out.put(Newline);
    out.put(Newline);
    out.put(Key).put("roles");
    out.put(Value).put(BeginMap);
    for i in 0..config.num_roles() {
        let role = c_str(config.role_name(i));
        if !role.is_empty() {
            // Note that no validation of the name strings is done here (e.g. to check that
            // they exist in the config) in order to enable serializing configs that are only
            // partially complete. The caller may use config->validate() first, if desired.
            out.put(Key).put(role);
            out.put(Value)
                .put(c_str(config.role_color_space_by_index(i)));
        }
    }
    out.put(EndMap);
    out.put(Newline);

    // File rules
    if config_major_version >= 2 {
        let rules = config.file_rules().get();
        out.put(Newline);
        out.put(Key).put("file_rules");
        out.put(Value).put(BeginSeq);
        for i in 0..rules.num_entries() {
            save_file_rule(out, &rules, i)?;
        }
        out.put(EndSeq);
        out.put(Newline);
    }

    // Viewing rules
    if config_major_version >= 2 {
        let rules = config.viewing_rules().get();
        let num_rules = rules.num_entries();
        if num_rules != 0 {
            out.put(Newline);
            out.put(Key).put("viewing_rules");
            out.put(Value).put(BeginSeq);
            for i in 0..num_rules {
                save_viewing_rule(out, &rules, i)?;
            }
            out.put(EndSeq);
            out.put(Newline);
        }
    }

    // Shared views
    let num_shared_views = config.num_views_of_type(ViewType::Shared, b"");
    if num_shared_views != 0 {
        out.put(Newline);
        out.put(Key).put("shared_views");
        out.put(Value).put(BeginSeq);
        for v in 0..num_shared_views {
            let name = config.view_of_type(ViewType::Shared, b"", v);
            save_view(out, &config_view(config, b"", name));
        }
        out.put(EndSeq);
        out.put(Newline);
    }

    // Displays.
    out.put(Newline);
    out.put(Key).put("displays");
    out.put(Value).put(BeginMap);
    // All displays are saved (not just active ones).
    for i in 0..config.num_displays_all() {
        // Do not save displays instantiated from a virtual display.
        if !config.is_display_temporary(i) {
            let display = c_str(config.display_all(i));

            out.put(Key).put(display);
            out.put(Value).put(BeginSeq);
            for v in 0..config.num_views_of_type(ViewType::DisplayDefined, display) {
                let name = config.view_of_type(ViewType::DisplayDefined, display, v);
                save_view(out, &config_view(config, display, name));
            }

            let shared_views: Vec<Vec<u8>> = (0..config
                .num_views_of_type(ViewType::Shared, display))
                .map(|v| c_str(config.view_of_type(ViewType::Shared, display, v)).to_vec())
                .collect();
            if !shared_views.is_empty() {
                out.put(verbatim_tag("Views"));
                out.put(Flow);
                put_strings(out, &shared_views);
            }
            out.put(EndSeq);
        }
    }
    out.put(EndMap);

    // Virtual Display.
    let num_virtual_display_views = config.virtual_display_num_views(ViewType::DisplayDefined)
        + config.virtual_display_num_views(ViewType::Shared);

    if config_major_version >= 2 && num_virtual_display_views > 0 {
        out.put(Newline);
        out.put(Newline);
        out.put(Key).put("virtual_display");
        out.put(Value).put(BeginSeq);

        for idx in 0..config.virtual_display_num_views(ViewType::DisplayDefined) {
            let view_name = config.virtual_display_view(ViewType::DisplayDefined, idx);
            let view = View::new(
                view_name,
                config.virtual_display_view_transform_name(view_name),
                config.virtual_display_view_color_space_name(view_name),
                config.virtual_display_view_looks(view_name),
                config.virtual_display_view_rule(view_name),
                config.virtual_display_view_description(view_name),
            );
            save_view(out, &view);
        }

        let shared_views: Vec<Vec<u8>> = (0..config.virtual_display_num_views(ViewType::Shared))
            .map(|idx| c_str(config.virtual_display_view(ViewType::Shared, idx)).to_vec())
            .collect();
        if !shared_views.is_empty() {
            out.put(verbatim_tag("Views"));
            out.put(Flow);
            put_strings(out, &shared_views);
        }

        out.put(EndSeq);
    }

    out.put(Newline);
    out.put(Newline);
    out.put(Key).put("active_displays");
    let active_displays: Vec<Vec<u8>> = (0..config.num_active_displays())
        .map(|i| c_str(config.active_display(i).unwrap_or_default()).to_vec())
        .collect();

    // The YAML library will wrap names that use a comma in quotes.
    out.put(Value).put(Flow);
    put_strings(out, &active_displays);

    out.put(Key).put("active_views");
    let active_views: Vec<Vec<u8>> = (0..config.num_active_views())
        .map(|i| c_str(config.active_view(i).unwrap_or_default()).to_vec())
        .collect();

    // The YAML library will wrap names that use a comma in quotes.
    out.put(Value).put(Flow);
    put_strings(out, &active_views);

    let inactive_css = c_str(config.inactive_color_spaces());
    if !inactive_css.is_empty() {
        let inactive_colorspaces = split_string_env_style(inactive_css)?;
        out.put(Key).put("inactive_colorspaces");
        out.put(Value).put(Flow);
        put_strings(out, &inactive_colorspaces);
    }

    out.put(Newline);

    // Looks
    if config.num_looks() > 0 {
        out.put(Newline);
        out.put(Key).put("looks");
        out.put(Value).put(BeginSeq);
        for i in 0..config.num_looks() {
            let name = config.look_name_by_index(i);
            let look = config
                .look(name)
                .expect("the look of a name the config lists");
            save_look(out, look, config_major_version)?;
        }
        out.put(EndSeq);
        out.put(Newline);
    }

    // View transforms.
    let def_vt = c_str(config.default_view_transform_name());
    if !def_vt.is_empty() {
        out.put(Newline);
        out.put(Key)
            .put("default_view_transform")
            .put(Value)
            .put(def_vt);
        out.put(Newline);
    }
    let num_vt = config.num_view_transforms();
    if num_vt > 0 {
        out.put(Newline);
        out.put(Key).put("view_transforms");
        out.put(Value).put(BeginSeq);
        for i in 0..num_vt {
            let name = config.view_transform_name_by_index(i);
            let vt = config
                .view_transform(name)
                .expect("the view transform of a name the config lists");
            save_view_transform(out, vt, config_major_version)?;
        }
        out.put(EndSeq);
    }

    let mut scene_cs = Vec::new();
    let mut display_cs = Vec::new();
    let num_cs =
        config.num_color_spaces_with(SearchReferenceSpaceType::All, ColorSpaceVisibility::All);
    for i in 0..num_cs {
        let name = config.color_space_name_by_index_with(
            SearchReferenceSpaceType::All,
            ColorSpaceVisibility::All,
            i,
        );

        let cs = config
            .color_space(name)
            .expect("the color space of a name the config lists");
        if cs.reference_space_type() == ReferenceSpaceType::Display {
            // Display color spaces instantiated from a virtual display must not be saved.
            // Check them using their name as they have the same name as the display.

            let idx = config.display_all_by_name(name);
            if idx == -1 || !config.is_display_temporary(idx) {
                display_cs.push(cs);
            }
        } else {
            scene_cs.push(cs);
        }
    }

    // Display ColorSpaces
    if !display_cs.is_empty() {
        out.put(Newline);
        out.put(Key).put("display_colorspaces");
        out.put(Value).put(BeginSeq);
        for cs in &display_cs {
            save_color_space(out, cs, config_major_version)?;
        }
        out.put(EndSeq);
    }

    // ColorSpaces
    {
        out.put(Newline);
        out.put(Key).put("colorspaces");
        out.put(Value).put(BeginSeq);
        for cs in &scene_cs {
            save_color_space(out, cs, config_major_version)?;
        }
        out.put(EndSeq);
    }

    // Named transforms.
    let num_nt = config.num_named_transforms_with(NamedTransformVisibility::All);
    if num_nt > 0 {
        out.put(Newline);
        out.put(Key).put("named_transforms");
        out.put(Value).put(BeginSeq);
        for i in 0..num_nt {
            let name = config.named_transform_name_by_index_with(NamedTransformVisibility::All, i);
            let nt = config
                .named_transform(name)
                .expect("the named transform of a name the config lists");
            save_named_transform(out, nt, config_major_version)?;
        }
        out.put(EndSeq);
    }

    out.put(EndMap);
    Ok(())
}

/// The config's YAML text, with doubles at precision 15 (`digits10`) and floats at 7.
///
/// Port of `OCIOYaml::Write` (OCIOYaml.cpp:5441-5448 @ v2.5.2).
pub(crate) fn write(config: &Config) -> ocio_ops::exception::Result<Vec<u8>> {
    let mut out = Emitter::new();
    out.set_double_precision(f64::DIGITS as usize);
    out.set_float_precision(7);
    save_config(&mut out, config)?;
    Ok(c_str(out.c_str()).to_vec())
}

#[cfg(test)]
#[path = "ocio_yaml_oracle_tests.rs"]
mod oracle_tests;

#[cfg(test)]
#[path = "ocio_yaml_objects_oracle_tests.rs"]
mod objects_oracle_tests;

#[cfg(test)]
#[path = "ocio_yaml_save_oracle_tests.rs"]
mod save_oracle_tests;
