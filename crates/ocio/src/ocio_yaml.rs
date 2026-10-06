// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! OCIO's YAML reader: a port of the `load` functions of `src/OpenColorIO/OCIOYaml.cpp` @
//! v2.5.2, which read a config's nodes (parsed by [`crate::yaml_cpp`]) into OCIO's objects.
//! The `save` functions (the writer) are WP 3.7's.
//!
//! So far: the typed loaders and their messages, the helpers that report unknown keys, bad
//! values and repeated keys, and the transforms ([`load_transform`]). The loaders of the
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
use ocio_ops::open_color_types::TransformDirection;
use ocio_ops::parse_utils::transform_direction_from_string;
use ocio_ops::utils::string_utils::c_str;

use crate::transform::Transform;
use crate::transforms::log_transform::LogTransform;
use crate::transforms::matrix_transform::MatrixTransform;
use crate::transforms::range_transform::{RangeTransform, range_style_from_string};
use crate::yaml_cpp::exceptions::Exception as YamlException;
use crate::yaml_cpp::node::{Node, NodeType};

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
        let os = format!(
            "Unsupported Transform type encountered: ({}) in OCIO profile. Only Mapping types \
             supported.",
            node_type as i32
        );
        return Err(throw_error(node, os.as_bytes()));
    }

    let ty = node.tag()?.to_vec();
    Ok(match ty.as_slice() {
        b"AllocationTransform"
        | b"BuiltinTransform"
        | b"CDLTransform"
        | b"ColorSpaceTransform"
        | b"DisplayViewTransform"
        | b"ExponentTransform"
        | b"ExponentWithLinearTransform"
        | b"FileTransform"
        | b"GroupTransform"
        | b"LogAffineTransform"
        | b"LogCameraTransform"
        | b"LookTransform" => return Err(not_ported_yet(&ty, "WP 3.3h-i")),
        b"ExposureContrastTransform"
        | b"GradingPrimaryTransform"
        | b"GradingRGBCurveTransform"
        | b"GradingHueCurveTransform"
        | b"GradingToneTransform" => return Err(not_ported_yet(&ty, "Phase 5")),
        b"FixedFunctionTransform" => return Err(not_ported_yet(&ty, "p3-after-p2")),
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
