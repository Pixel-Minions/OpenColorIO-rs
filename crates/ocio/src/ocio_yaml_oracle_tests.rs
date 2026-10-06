// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The transform loaders against the wheel, through the oracle's `config_calls` (O3.1).
//!
//! Each case is the YAML text of a transform, put as the `to_scene_reference` of the only
//! color space of a version 2 config ([`config_text`]). The wheel loads the config
//! (`Config::CreateFromStream`) and gives the color space's transform; the port parses the same
//! text, takes the same node and loads it with [`load_transform`]. They must agree on:
//! - the error, byte for byte: the wheel's message is `OCIOYaml::Read`'s "Error: Loading the
//!   OCIO profile failed. " and the loader's `what()` (the wrapper is WP 3.3m's; until then the
//!   test adds it to the port's message);
//! - or the transform: its class and text (`operator<<`), then its values through its getters
//!   (numbers by their bits, names as bytes), asked in a second batch for the cases whose class
//!   matched;
//! - and the warnings logged while loading, byte for byte.

use std::sync::{Arc, Mutex, PoisonError};
use std::thread;

use ocio_ops::logging::{reset_to_default_logging_function, set_logging_function};
use ocio_ops::open_color_types::{CdlStyle, NegativeStyle};
use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::oracle_values::{bytes, hex, log};
use serde_json::{Value, json};

use super::*;
use crate::yaml_cpp::parse::load;

/// The config around a case: the case's text follows `to_scene_reference: ` on line 7, so a
/// case written on one line is on line 7, as the messages report it.
const HEAD: &[u8] = b"ocio_profile_version: 2\n\
roles: {default: raw}\n\
\n\
colorspaces:\n  \
- !<ColorSpace>\n    \
name: raw\n    \
to_scene_reference: ";

/// The text of `OCIOYaml::Read`'s error for a config read from a stream (OCIOYaml.cpp:5427-5440
/// @ v2.5.2).
const READ_ERROR: &[u8] = b"Error: Loading the OCIO profile failed. ";

/// The config of a case.
pub(super) fn config_text(case: &[u8]) -> Vec<u8> {
    [HEAD, case, b"\n"].concat()
}

/// Serializes the tests that replace the logging function.
static LOGGING: Mutex<()> = Mutex::new(());

/// Runs `f`, and gives the messages OCIO logged on this thread meanwhile, each as the logging
/// function receives it (with its prefix and line feed). Messages of other threads go to
/// stderr, as with the default logging function.
pub(super) fn capture_log<T>(f: impl FnOnce() -> T) -> (T, Vec<Vec<u8>>) {
    let _lock = LOGGING.lock().unwrap_or_else(PoisonError::into_inner);
    let me = thread::current().id();
    let messages = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&messages);
    set_logging_function(Some(Arc::new(move |message: &[u8]| {
        if thread::current().id() == me {
            sink.lock().unwrap().push(message.to_vec());
        } else {
            eprint!("{}", String::from_utf8_lossy(message));
        }
    })))
    .unwrap();
    let out = f();
    reset_to_default_logging_function();
    let log = messages.lock().unwrap().clone();
    (out, log)
}

/// What the port makes of a case: the error message as the wheel reports it, or the color
/// space's transform (none where the value is null).
pub(super) enum Loaded {
    Error(Vec<u8>),
    Transform(Option<Transform>),
}

/// The port's load of a case, as the color space loader reaches the transform: the config's
/// node `colorspaces[0]["to_scene_reference"]`, skipped when it is null (OCIOYaml.cpp:3443,
/// 3523-3533 @ v2.5.2).
pub(super) fn port_load(case: &[u8]) -> (Loaded, Vec<Vec<u8>>) {
    capture_log(|| {
        let failed = |what: Vec<u8>| Loaded::Error([READ_ERROR, &what].concat());
        let doc = match load(&config_text(case)) {
            Ok(doc) => doc,
            Err(e) => return failed(LoadError::from(e).what()),
        };
        let node = doc
            .get("colorspaces")
            .and_then(|n| n.get(0usize))
            .and_then(|n| n.get("to_scene_reference"))
            .expect("the case's node");
        if node.is_null().unwrap() || !node.is_defined() {
            return Loaded::Transform(None);
        }
        match load_transform(&node) {
            Ok(t) => Loaded::Transform(Some(t)),
            Err(e) => failed(e.what()),
        }
    })
}

/// The config call of a case: the config from its text, then the color space's transform,
/// stored as `t`, and `more` calls.
fn request(case: &[u8], more: &[Value]) -> Value {
    let mut calls = vec![
        json!({"call": "getColorSpace", "args": ["raw"], "as": "cs"}),
        json!({"call": "getTransform", "on": "cs",
               "args": [{"enum": "COLORSPACE_DIR_TO_REFERENCE"}], "as": "t"}),
    ];
    calls.extend_from_slice(more);
    json!({"config": {"yaml": {"bytes": hex(&config_text(case))}}, "calls": calls})
}

fn run(requests: Vec<Value>) -> Vec<Value> {
    let calls: Vec<BatchCall<'_>> = requests
        .into_iter()
        .map(|args| BatchCall {
            cmd: "config_calls",
            args,
            blobs: Vec::new(),
        })
        .collect();
    Oracle::get()
        .batch(&calls, true)
        .into_iter()
        .map(|r| r.expect("config_calls").result)
        .collect()
}

/// A float as the oracle writes it.
pub(super) fn f64_out(v: f64) -> Value {
    json!({"f64": v.to_bits()})
}

/// A string as the oracle writes it.
pub(super) fn bytes_out(v: &[u8]) -> Value {
    json!({"bytes": hex(v)})
}

/// Floats as the oracle writes a list of them.
pub(super) fn f64s(values: &[f64]) -> Value {
    Value::Array(values.iter().map(|&v| f64_out(v)).collect())
}

/// A negative style as the oracle writes it.
fn negative_style_out(style: NegativeStyle) -> Value {
    json!({"enum": match style {
        NegativeStyle::Clamp => "NEGATIVE_CLAMP",
        NegativeStyle::Mirror => "NEGATIVE_MIRROR",
        NegativeStyle::PassThru => "NEGATIVE_PASS_THRU",
        NegativeStyle::Linear => "NEGATIVE_LINEAR",
    }})
}

/// A direction as the oracle writes it.
pub(super) fn direction_out(dir: TransformDirection) -> Value {
    json!({"enum": match dir {
        TransformDirection::Forward => "TRANSFORM_DIR_FORWARD",
        TransformDirection::Inverse => "TRANSFORM_DIR_INVERSE",
    }})
}

/// The calls on the transform `t` that read its values, each with the port's value as the
/// oracle writes it. Calls whose name has `:` call the part after it on the object the part
/// before it gave (`getFormatMetadata:getName`).
pub(super) fn getters(t: &Transform) -> Vec<(&'static str, Value)> {
    let mut out = vec![("getDirection", direction_out(t.direction()))];
    let name = |m: &ocio_ops::format_metadata::FormatMetadataImpl| {
        ("getFormatMetadata:getName", bytes_out(m.get_name()))
    };
    match t {
        Transform::Log(t) => {
            out.push(("getBase", f64_out(t.base())));
            out.push(name(t.format_metadata()));
        }
        Transform::Matrix(t) => {
            out.push((
                "getMatrix",
                Value::Array(t.matrix().iter().map(|&v| f64_out(v)).collect()),
            ));
            out.push((
                "getOffset",
                Value::Array(t.offset().iter().map(|&v| f64_out(v)).collect()),
            ));
            out.push(name(t.format_metadata()));
        }
        Transform::Range(t) => {
            out.push(("getMinInValue", f64_out(t.min_in_value())));
            out.push(("getMaxInValue", f64_out(t.max_in_value())));
            out.push(("getMinOutValue", f64_out(t.min_out_value())));
            out.push(("getMaxOutValue", f64_out(t.max_out_value())));
            out.push(("hasMinInValue", json!(t.has_min_in_value())));
            out.push(("hasMaxInValue", json!(t.has_max_in_value())));
            out.push(("hasMinOutValue", json!(t.has_min_out_value())));
            out.push(("hasMaxOutValue", json!(t.has_max_out_value())));
            out.push(name(t.format_metadata()));
        }
        Transform::Allocation(t) => {
            out.push((
                "getAllocation",
                json!({"enum": match t.allocation() {
                    Allocation::Unknown => "ALLOCATION_UNKNOWN",
                    Allocation::Uniform => "ALLOCATION_UNIFORM",
                    Allocation::Lg2 => "ALLOCATION_LG2",
                }}),
            ));
            out.push((
                "getVars",
                Value::Array(t.vars().iter().map(|&v| f64_out(f64::from(v))).collect()),
            ));
        }
        Transform::Cdl(t) => {
            out.push(("getSlope", f64s(&t.slope())));
            out.push(("getOffset", f64s(&t.offset())));
            out.push(("getPower", f64s(&t.power())));
            out.push(("getSat", f64_out(t.sat())));
            out.push((
                "getStyle",
                json!({"enum": match t.style() {
                    CdlStyle::Asc => "CDL_ASC",
                    CdlStyle::NoClamp => "CDL_NO_CLAMP",
                }}),
            ));
            out.push(name(t.format_metadata()));
        }
        Transform::Exponent(t) => {
            out.push(("getValue", f64s(&t.value())));
            out.push(("getNegativeStyle", negative_style_out(t.negative_style())));
            out.push(name(t.format_metadata()));
        }
        Transform::ExponentWithLinear(t) => {
            out.push(("getGamma", f64s(&t.gamma())));
            out.push(("getOffset", f64s(&t.offset())));
            out.push(("getNegativeStyle", negative_style_out(t.negative_style())));
            out.push(name(t.format_metadata()));
        }
        Transform::LogAffine(t) => {
            out.push(("getBase", f64_out(t.base())));
            out.push(("getLogSideSlopeValue", f64s(&t.log_side_slope_value())));
            out.push(("getLogSideOffsetValue", f64s(&t.log_side_offset_value())));
            out.push(("getLinSideSlopeValue", f64s(&t.lin_side_slope_value())));
            out.push(("getLinSideOffsetValue", f64s(&t.lin_side_offset_value())));
            out.push(name(t.format_metadata()));
        }
        Transform::LogCamera(t) => {
            out.push(("getBase", f64_out(t.base())));
            out.push(("getLogSideSlopeValue", f64s(&t.log_side_slope_value())));
            out.push(("getLogSideOffsetValue", f64s(&t.log_side_offset_value())));
            out.push(("getLinSideSlopeValue", f64s(&t.lin_side_slope_value())));
            out.push(("getLinSideOffsetValue", f64s(&t.lin_side_offset_value())));
            out.push(("getLinSideBreakValue", f64s(&t.lin_side_break_value())));
            let slope = t.linear_slope_value();
            out.push(("isLinearSlopeValueSet", json!(slope.is_some())));
            if let Some(slope) = slope {
                out.push(("getLinearSlopeValue", f64s(&slope)));
            }
            out.push(name(t.format_metadata()));
        }
        _ => {}
    }
    out
}

/// The calls of [`getters`] as config calls on `t`.
fn getter_calls(getters: &[(&'static str, Value)]) -> Vec<Value> {
    let mut calls = Vec::new();
    for (i, (name, _)) in getters.iter().enumerate() {
        match name.split_once(':') {
            None => calls.push(json!({"call": name, "on": "t"})),
            Some((first, second)) => {
                let object = format!("g{i}");
                calls.push(json!({"call": first, "on": "t", "as": object}));
                calls.push(json!({"call": second, "on": object}));
            }
        }
    }
    calls
}

/// The results of the getter calls, in the order of [`getters`].
fn getter_results(getters: &[(&'static str, Value)], calls: &[Value]) -> Vec<Value> {
    let mut out = Vec::new();
    let mut at = 0;
    for (name, _) in getters {
        at += if name.contains(':') { 2 } else { 1 };
        let call = &calls[at - 1];
        // A string the binding can't decode comes back as its bytes, `undecodable`.
        let result = match (call.get("result"), call.get("undecodable")) {
            (Some(result), _) => result.clone(),
            (None, Some(h)) => json!({"bytes": h}),
            _ => call.clone(),
        };
        out.push(result);
    }
    out
}

/// The error message of a config the wheel failed to make.
fn wheel_error(config: &Value) -> Vec<u8> {
    if let Some(h) = config["undecodable"].as_str() {
        return (0..h.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap())
            .collect();
    }
    bytes(&config["exception"]["message"])
}

/// Checks each case against the wheel; panics listing every case that differs.
pub(super) fn check(cases: &[&[u8]]) {
    let ported: Vec<(Loaded, Vec<Vec<u8>>)> = cases.iter().map(|c| port_load(c)).collect();
    let wheel = run(cases.iter().map(|c| request(c, &[])).collect());

    let mut failures = Vec::new();
    let mut second: Vec<(usize, Vec<(&'static str, Value)>)> = Vec::new();
    for (i, ((loaded, port_log), w)) in ported.iter().zip(&wheel).enumerate() {
        let label = String::from_utf8_lossy(cases[i]);
        let wheel_log = log(&w["config_log"]);
        if &wheel_log != port_log {
            failures.push(format!(
                "{label}: log\n  wheel {:?}\n  port  {:?}",
                lossy(&wheel_log),
                lossy(port_log)
            ));
        }
        let config = &w["config"];
        match loaded {
            Loaded::Error(message) => {
                if config.is_null() {
                    failures.push(format!(
                        "{label}: the wheel loads it, the port fails: {}",
                        String::from_utf8_lossy(message)
                    ));
                } else if &wheel_error(config) != message {
                    failures.push(format!(
                        "{label}: error\n  wheel {}\n  port  {}",
                        String::from_utf8_lossy(&wheel_error(config)),
                        String::from_utf8_lossy(message)
                    ));
                }
            }
            Loaded::Transform(t) => {
                if !config.is_null() {
                    failures.push(format!(
                        "{label}: the port loads it, the wheel fails: {}",
                        String::from_utf8_lossy(&wheel_error(config))
                    ));
                    continue;
                }
                let got = &w["calls"][1]["result"];
                match t {
                    None => {
                        if !got.is_null() {
                            failures.push(format!("{label}: the wheel has a transform: {got}"));
                        }
                    }
                    Some(t) => {
                        let class = format!("{}Transform", class_name(t));
                        let text = bytes_out(&t.to_bytes());
                        if got["class"] != json!(class) || got["repr"] != text {
                            failures.push(format!(
                                "{label}: transform\n  wheel {got}\n  port  {class} {}",
                                String::from_utf8_lossy(&t.to_bytes())
                            ));
                        } else {
                            second.push((i, getters(t)));
                        }
                    }
                }
            }
        }
    }

    let wheel = run(second
        .iter()
        .map(|(i, g)| request(cases[*i], &getter_calls(g)))
        .collect());
    for ((i, g), w) in second.iter().zip(&wheel) {
        let label = String::from_utf8_lossy(cases[*i]);
        let results = getter_results(g, &w["calls"].as_array().unwrap()[2..]);
        for ((name, port), wheel) in g.iter().zip(&results) {
            if port != wheel {
                failures.push(format!("{label}: {name}\n  wheel {wheel}\n  port  {port}"));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}

/// The class's name without "Transform", as upstream's tags and Python class names spell it.
fn class_name(t: &Transform) -> &'static str {
    match t {
        Transform::Allocation(_) => "Allocation",
        Transform::Builtin(_) => "Builtin",
        Transform::Cdl(_) => "CDL",
        Transform::ColorSpace(_) => "ColorSpace",
        Transform::DisplayView(_) => "DisplayView",
        Transform::Exponent(_) => "Exponent",
        Transform::ExponentWithLinear(_) => "ExponentWithLinear",
        Transform::File(_) => "File",
        Transform::FixedFunction(_) => "FixedFunction",
        Transform::Group(_) => "Group",
        Transform::LogAffine(_) => "LogAffine",
        Transform::LogCamera(_) => "LogCamera",
        Transform::Log(_) => "Log",
        Transform::Look(_) => "Look",
        Transform::Lut1D(_) => "Lut1D",
        Transform::Matrix(_) => "Matrix",
        Transform::Range(_) => "Range",
    }
}

fn lossy(log: &[Vec<u8>]) -> Vec<String> {
    log.iter()
        .map(|m| String::from_utf8_lossy(m).into_owned())
        .collect()
}

/// Nodes that aren't maps, tags that name no transform, keys that aren't strings, repeated
/// keys, and errors and warnings whose text holds a NUL or bytes that aren't UTF-8.
#[test]
fn the_dispatch_and_the_helpers_match_the_wheel() {
    check(&[
        b"!<LogTransform> 5",
        b"!<LogTransform> [1, 2]",
        b"!<LogTransform> \"x\"",
        b"[1, 2]",
        b"5",
        b"'quoted'",
        b"{base: 3}",
        b"!LogTransform {}",
        b"!!map {}",
        b"!<FooTransform> {}",
        b"!<Lut1DTransform> {}",
        b"!<Lut3DTransform> {}",
        b"!<logtransform> {}",
        b"!<LogTransform> {[a]: 1}",
        b"!<LogTransform> {base: 2, [a]: 1}",
        b"!<LogTransform> {? {a: 1} : 2}",
        b"!<LogTransform> {base: 1, base: 2}",
        b"!<LogTransform> {base: 1, \"base\": 2}",
        b"!<LogTransform> {1: a, 1.0: b, 01: c}",
        b"!<LogTransform> {\"a\\0b\": 1, \"a\\0b\": 2}",
        b"!<LogTransform> {\"a\\0b\": 1}",
        b"!<LogTransform> {\"x\\ny\": 1}",
        b"!<LogTransform> {\xe9t\xe9: 1}",
        b"!<LogTransform> {bse: 3, foo: , bar: [1]}",
        b"!<MatrixTransform> {foo: 1, \"a\\0b\": 2}",
        b"!<MatrixTransform> {[x]: 1}",
        b"!<RangeTransform> {foo: 1}",
        b"",
        b"~",
        b"!<LogTransform>",
        b"!<LogTransform> {base: [1",
        b"!<LogTransform> {base: \"\\\x00x\"}",
        b"!<LogTransform> {base: \"\\\xffx\"}",
        b"!<LogTransform>\n      base: abc",
        b"!<LogTransform>\n      bse: 1\n      base: 1\n      base: 2",
    ]);
}

/// The LogTransform's keys, numbers in every spelling, and the errors of each key.
#[test]
fn log_transforms_load_as_in_the_wheel() {
    check(&[
        b"!<LogTransform> {}",
        b"!<LogTransform> {base: 10}",
        b"!<LogTransform> {base: 2.5e1}",
        b"!<LogTransform> {base: 0x10}",
        b"!<LogTransform> {base: 010}",
        b"!<LogTransform> {base: 0x1p3}",
        b"!<LogTransform> {base: 1e400}",
        b"!<LogTransform> {base: 1e-400}",
        b"!<LogTransform> {base: -0}",
        b"!<LogTransform> {base: .inf}",
        b"!<LogTransform> {base: -.Inf}",
        b"!<LogTransform> {base: .NaN}",
        b"!<LogTransform> {base: nan}",
        b"!<LogTransform> {base: \" 3\"}",
        b"!<LogTransform> {base: \"3 \"}",
        b"!<LogTransform> {base: abc}",
        b"!<LogTransform> {base: \"abc\"}",
        b"!<LogTransform> {base: \"1\\0x\"}",
        b"!<LogTransform> {base: [1, 2]}",
        b"!<LogTransform> {base: []}",
        b"!<LogTransform> {base: {a: 1}}",
        b"!<LogTransform> {base: }",
        b"!<LogTransform> {base: ~}",
        b"!<LogTransform> {base: !!str 3}",
        b"!<LogTransform> {direction: inverse}",
        b"!<LogTransform> {direction: Forward}",
        b"!<LogTransform> {direction: foo}",
        b"!<LogTransform> {direction: \"inverse\\0x\"}",
        b"!<LogTransform> {direction: [inverse]}",
        b"!<LogTransform> {name: log}",
        b"!<LogTransform> {name: \"a\\0b\"}",
        b"!<LogTransform> {name: \xff\xfe}",
        b"!<LogTransform> {name: [x]}",
        b"!<LogTransform> {name: 3, base: 3, direction: inverse}",
    ]);
}

/// The MatrixTransform's keys and the sizes of its values.
#[test]
fn matrix_transforms_load_as_in_the_wheel() {
    check(&[
        b"!<MatrixTransform> {}",
        b"!<MatrixTransform> {matrix: [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]}",
        b"!<MatrixTransform> {matrix: [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.1, 1.2, \
          1.3, 1.4, 1.5, 1.6, 1.7], offset: [0.01, -0.02, 1e-7, 4]}",
        b"!<MatrixTransform> {matrix: [1, 2, 3]}",
        b"!<MatrixTransform> {matrix: []}",
        b"!<MatrixTransform> {matrix: [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0]}",
        b"!<MatrixTransform> {matrix: 5}",
        b"!<MatrixTransform> {matrix: [a, b]}",
        b"!<MatrixTransform> {matrix: [[1]]}",
        b"!<MatrixTransform> {matrix: {a: 1}}",
        b"!<MatrixTransform> {offset: [1, 2, 3, 4]}",
        b"!<MatrixTransform> {offset: [1]}",
        b"!<MatrixTransform> {offset: [0x1p3, .inf, -.inf, .nan]}",
        b"!<MatrixTransform> {offset: [1e400, 1, 1, 1]}",
        b"!<MatrixTransform> {offset: [1, 2, 3, 4], offset: [1, 2, 3, 4]}",
        b"!<MatrixTransform> {direction: inverse, name: m}",
        b"!<MatrixTransform>\n      matrix: [1, 2]\n      offset: [1, 2, 3, 4]",
    ]);
}

/// The RangeTransform's bounds, styles and errors.
#[test]
fn range_transforms_load_as_in_the_wheel() {
    check(&[
        b"!<RangeTransform> {}",
        b"!<RangeTransform> {min_in_value: 0, max_in_value: 1, min_out_value: 0.5, \
          max_out_value: 2}",
        b"!<RangeTransform> {min_in_value: -0.125, max_out_value: 1e300}",
        b"!<RangeTransform> {style: noClamp}",
        b"!<RangeTransform> {style: CLAMP}",
        b"!<RangeTransform> {style: bad}",
        b"!<RangeTransform> {style: \"clamp\\0x\"}",
        b"!<RangeTransform> {style: [clamp]}",
        b"!<RangeTransform> {min_in_value: x}",
        b"!<RangeTransform> {min_in_value: [1]}",
        b"!<RangeTransform> {min_in_value: , max_in_value: 1}",
        b"!<RangeTransform> {min_in_value: .nan}",
        b"!<RangeTransform> {direction: inverse, name: r}",
        b"!<RangeTransform> {min_in_value: 1, min_in_value: 1}",
    ]);
}

/// The AllocationTransform's keys: its allocations, variables as floats, and their errors.
#[test]
fn allocation_transforms_load_as_in_the_wheel() {
    check(&[
        b"!<AllocationTransform> {}",
        b"!<AllocationTransform> {allocation: lg2, vars: [-8, 5, 0.00390625]}",
        b"!<AllocationTransform> {allocation: UNIFORM, vars: [0, 1]}",
        b"!<AllocationTransform> {allocation: foo}",
        b"!<AllocationTransform> {allocation: \"lg2\0x\"}",
        b"!<AllocationTransform> {allocation: [lg2]}",
        b"!<AllocationTransform> {vars: []}",
        b"!<AllocationTransform> {vars: [0.1, 1e-50, 3.4028236e38, 1e39]}",
        b"!<AllocationTransform> {vars: [1, 2, 3, 4, 5]}",
        b"!<AllocationTransform> {vars: 1}",
        b"!<AllocationTransform> {vars: [a]}",
        b"!<AllocationTransform> {direction: inverse, name: a}",
        b"!<AllocationTransform> {vars: [1], vars: [2]}",
    ]);
}

/// The CDLTransform's keys, the sizes of its values, its styles and errors.
#[test]
fn cdl_transforms_load_as_in_the_wheel() {
    check(&[
        b"!<CDLTransform> {}",
        b"!<CDLTransform> {slope: [1.1, 1.2, 1.3], offset: [-0.1, 0, 0.1], power: [0.9, 1, 1.1], \
          sat: 0.8}",
        b"!<CDLTransform> {saturation: 1.5, style: noclamp, name: grade}",
        b"!<CDLTransform> {saturation: 1.5, sat: 0.5}",
        b"!<CDLTransform> {slope: [1, 2]}",
        b"!<CDLTransform> {offset: [1, 2, 3, 4]}",
        b"!<CDLTransform> {power: []}",
        b"!<CDLTransform> {slope: 1}",
        b"!<CDLTransform> {slope: [a, b, c]}",
        b"!<CDLTransform> {sat: [1]}",
        b"!<CDLTransform> {style: Asc}",
        b"!<CDLTransform> {style: v1.2}",
        b"!<CDLTransform> {direction: inverse, foo: 1}",
        b"!<CDLTransform> {slope: [1, 1, 1], slope: [2, 2, 2]}",
    ]);
}

/// The ExponentTransform's values (four, or one for RGB), its styles in either order with
/// the direction, and its errors.
#[test]
fn exponent_transforms_load_as_in_the_wheel() {
    check(&[
        b"!<ExponentTransform> {}",
        b"!<ExponentTransform> {value: [2.2, 2.2, 2.2, 1]}",
        b"!<ExponentTransform> {value: 2.4}",
        b"!<ExponentTransform> {value: [1, 2, 3]}",
        b"!<ExponentTransform> {value: []}",
        b"!<ExponentTransform> {value: {a: 1}}",
        b"!<ExponentTransform> {value: abc}",
        b"!<ExponentTransform> {value: [a, 1, 1, 1]}",
        b"!<ExponentTransform> {style: mirror}",
        b"!<ExponentTransform> {style: pass_thru}",
        b"!<ExponentTransform> {style: linear}",
        b"!<ExponentTransform> {style: Clamp}",
        b"!<ExponentTransform> {style: foo}",
        b"!<ExponentTransform> {style: mirror, direction: inverse}",
        b"!<ExponentTransform> {direction: inverse, style: mirror}",
        b"!<ExponentTransform> {direction: inverse, style: pass_thru}",
        b"!<ExponentTransform> {value: 2, name: e, foo: 1}",
        b"!<ExponentTransform> {value: 2, value: 3}",
    ]);
}

/// The ExponentWithLinearTransform's required gamma and offset, single values, styles, and
/// its errors, which have no line.
#[test]
fn exponent_with_linear_transforms_load_as_in_the_wheel() {
    check(&[
        b"!<ExponentWithLinearTransform> {}",
        b"!<ExponentWithLinearTransform> {gamma: 2.4}",
        b"!<ExponentWithLinearTransform> {offset: 0.055}",
        b"!<ExponentWithLinearTransform> {gamma: 2.4, offset: 0.055}",
        b"!<ExponentWithLinearTransform> {gamma: [2.2, 2.4, 2.6, 1], offset: [0.1, 0.2, 0.3, 0]}",
        b"!<ExponentWithLinearTransform> {gamma: [1, 2], offset: 0.1}",
        b"!<ExponentWithLinearTransform> {gamma: 2, offset: [1, 2, 3, 4, 5]}",
        b"!<ExponentWithLinearTransform> {gamma: x, offset: 0.1}",
        b"!<ExponentWithLinearTransform> {gamma: 2, offset: 0.1, style: mirror}",
        b"!<ExponentWithLinearTransform> {gamma: 2, offset: 0.1, style: pass_thru}",
        b"!<ExponentWithLinearTransform> {gamma: 2, offset: 0.1, style: linear}",
        b"!<ExponentWithLinearTransform> {gamma: 2, offset: 0.1, direction: inverse, \
          name: ewl, foo: 1}",
        b"!<ExponentWithLinearTransform> {gamma: 2, offset: 0.1, \"a\0b\": 1}",
        b"!<ExponentWithLinearTransform> {gamma: 2, gamma: 3}",
    ]);
}

/// The LogAffineTransform's base and parameters (three, or one for all), set once the map is
/// read, and the errors of each.
#[test]
fn log_affine_transforms_load_as_in_the_wheel() {
    check(&[
        b"!<LogAffineTransform> {}",
        b"!<LogAffineTransform> {base: 10, lin_side_offset: [0.1, 0.2, 0.3], \
          lin_side_slope: 2, log_side_offset: [-1, 0, 1], log_side_slope: 0.5}",
        b"!<LogAffineTransform> {base: [1]}",
        b"!<LogAffineTransform> {base: {a: 1, b: 2}}",
        b"!<LogAffineTransform> {base: x}",
        b"!<LogAffineTransform> {lin_side_slope: [1, 2]}",
        b"!<LogAffineTransform> {log_side_slope: []}",
        b"!<LogAffineTransform> {lin_side_offset: {a: 1}}",
        b"!<LogAffineTransform> {log_side_offset: [a, b, c]}",
        b"!<LogAffineTransform> {direction: inverse, name: la, bar: 1}",
        b"!<LogAffineTransform> {base: 2, base: 3}",
    ]);
}

/// The LogCameraTransform's parameters, its required linear break and optional linear slope,
/// and the errors of each.
#[test]
fn log_camera_transforms_load_as_in_the_wheel() {
    check(&[
        b"!<LogCameraTransform> {}",
        b"!<LogCameraTransform> {base: 10}",
        b"!<LogCameraTransform> {lin_side_break: 0.01}",
        b"!<LogCameraTransform> {lin_side_break: [0.1, 0.2, 0.3], linear_slope: [1, 2, 3]}",
        b"!<LogCameraTransform> {lin_side_break: 0.01, linear_slope: 5, base: 2.5, \
          lin_side_offset: 0.1, lin_side_slope: 2, log_side_offset: 0.3, log_side_slope: 0.4}",
        b"!<LogCameraTransform> {lin_side_break: [1, 2]}",
        b"!<LogCameraTransform> {lin_side_break: x}",
        b"!<LogCameraTransform> {lin_side_break: 0.01, linear_slope: [1]}",
        b"!<LogCameraTransform> {lin_side_break: 0.01, base: [2, 3]}",
        b"!<LogCameraTransform> {lin_side_break: 0.01, direction: inverse, name: lc, baz: 1}",
        b"!<LogCameraTransform> {lin_side_break: 1, lin_side_break: 2}",
        b"!<LogCameraTransform> {lin_side_break: }",
    ]);
}
