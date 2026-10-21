// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The GradingTone op's GPU writer against the wheel, in the 10 languages:
//! - `gpu_shader`: for each case, the GPU processor's cache ID and queries, and what the
//!   extraction writes (the shader's text, cache ID, uniforms with their values, and dynamic
//!   properties with their values), or the error, byte for byte;
//! - `dynamic_apply`: the uniforms' values after each value set on the description's dynamic
//!   property (O5.1);
//! - `shader_property_is_decoupled_from_the_op`: a set on the op data's property after the shader
//!   program is written leaves the uniforms as they were (a unit-level test of
//!   `get_grading_tone_gpu_shader_program`: the GPU processor clones its ops).
//!
//! The wheel builds `GradingToneTransform`s in a group, and its processor's GPU processor.
//! The port builds the op data as the binding's constructor does
//! (src/bindings/python/transforms/PyGradingToneTransform.cpp:17-31 @ v2.5.2), the ops with
//! `create_grading_tone_op` (`BuildGradingToneOp`,
//! src/OpenColorIO/ops/gradingtone/GradingToneOp.cpp:232-244), and runs the GPU
//! processor from them, as they are and finalized first. A dynamic op's values become uniforms
//! bound to the shader's own copy of the property (in OSL, which has no dynamic properties,
//! constants and a warning).

use ocio_gpu::gpu_processor::GpuProcessor;
use ocio_gpu::gpu_shader::UniformData;
use ocio_gpu::ops::gradingtone::grading_tone_op_gpu::get_grading_tone_gpu_shader_program;
use ocio_gpu::{GpuLanguage, GpuShaderDesc};
use std::sync::{Arc, Mutex, PoisonError};

use ocio_ops::dynamic_property::DynamicPropertyRcPtr;
use ocio_ops::logging::{get_logging_level, set_logging_function};
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{GradingStyle, OptimizationFlags, TransformDirection};
use ocio_ops::ops::gradingtone::grading_tone::{GradingRgbmsw, GradingTone};
use ocio_ops::ops::gradingtone::grading_tone_op::create_grading_tone_op;
use ocio_ops::ops::gradingtone::grading_tone_op_data::GradingToneOpData;
use ocio_ops::platform::{self, MapEnv};
use ocio_testkit::gpu::{
    self as oracle_gpu, GpuShaderReply, GpuShaderRequest, ShaderSettings, UniformValue,
};
use ocio_testkit::processor_ops::{Dump, Dumped};
use ocio_testkit::transform_text::f64_spec;
use ocio_testkit::{Oracle, assert_text_eq};
use serde_json::{Value, json};

/// The doubles of a GradingTone, by their bits, in a fixed order.
fn bits(t: &GradingTone) -> Vec<u64> {
    let mut out = Vec::new();
    for v in [&t.blacks, &t.shadows, &t.midtones, &t.highlights, &t.whites] {
        out.extend([v.red, v.green, v.blue, v.master, v.start, v.width]);
    }
    out.push(t.scontrast);
    out.iter().map(|v| v.to_bits()).collect()
}

fn getter_f64(dumped: &Dumped) -> f64 {
    match dumped {
        Dumped::F64(v) => *v,
        other => panic!("not a float: {other:?}"),
    }
}

/// The wheel's GradingTone, written out, as the port's.
fn tone_of(dump: &Dump) -> GradingTone {
    let rgbmsw = |name: &str| {
        let d = dump.property(name).object();
        GradingRgbmsw::new(
            getter_f64(d.property("red")),
            getter_f64(d.property("green")),
            getter_f64(d.property("blue")),
            getter_f64(d.property("master")),
            getter_f64(d.property("start")),
            getter_f64(d.property("width")),
        )
    };
    GradingTone {
        blacks: rgbmsw("blacks"),
        shadows: rgbmsw("shadows"),
        midtones: rgbmsw("midtones"),
        highlights: rgbmsw("highlights"),
        whites: rgbmsw("whites"),
        scontrast: getter_f64(dump.property("scontrast")),
    }
}

fn rgbmsw_spec(v: &GradingRgbmsw) -> Value {
    json!({"object": {"class": "GradingRGBMSW",
                      "args": [f64_spec(v.red), f64_spec(v.green), f64_spec(v.blue),
                               f64_spec(v.master), f64_spec(v.start), f64_spec(v.width)]}})
}

/// A GradingTone value spec, every double by its bits.
fn tone_spec(t: &GradingTone) -> Value {
    json!({"object": {"class": "GradingTone", "args": [{"enum": "GRADING_LOG"}], "attrs": [
        ["blacks", rgbmsw_spec(&t.blacks)],
        ["shadows", rgbmsw_spec(&t.shadows)],
        ["midtones", rgbmsw_spec(&t.midtones)],
        ["highlights", rgbmsw_spec(&t.highlights)],
        ["whites", rgbmsw_spec(&t.whites)],
        ["scontrast", f64_spec(t.scontrast)],
    ]}})
}

/// One GradingTone transform: style, direction, values, dynamic or not.
#[derive(Debug, Clone)]
struct Op {
    style: GradingStyle,
    inverse: bool,
    values: GradingTone,
    dynamic: bool,
}

impl Op {
    fn new(style: GradingStyle, values: GradingTone) -> Op {
        Op {
            style,
            inverse: false,
            values,
            dynamic: false,
        }
    }

    fn inverse(mut self) -> Op {
        self.inverse = true;
        self
    }

    fn dynamic(mut self) -> Op {
        self.dynamic = true;
        self
    }

    fn direction(&self) -> TransformDirection {
        if self.inverse {
            TransformDirection::Inverse
        } else {
            TransformDirection::Forward
        }
    }

    fn style_name(&self) -> &'static str {
        match self.style {
            GradingStyle::Log => "GRADING_LOG",
            GradingStyle::Lin => "GRADING_LIN",
            GradingStyle::Video => "GRADING_VIDEO",
        }
    }

    fn spec(&self) -> Value {
        let dir = if self.inverse {
            "TRANSFORM_DIR_INVERSE"
        } else {
            "TRANSFORM_DIR_FORWARD"
        };
        json!({"class": "GradingToneTransform", "args": {
            "values": tone_spec(&self.values),
            "style": {"enum": self.style_name()},
            "dynamic": self.dynamic,
            "dir": {"enum": dir},
        }})
    }

    /// The op data as the binding's constructor makes it, then `BuildGradingToneOp`'s copy.
    fn data(&self) -> GradingToneOpData {
        let mut data = GradingToneOpData::new(self.style);
        data.set_value(&self.values).unwrap();
        if self.dynamic {
            data.get_dynamic_property_internal().make_dynamic();
        }
        data.set_direction(self.direction());
        data.validate().unwrap();
        data.clone()
    }
}

/// The optimization levels, as the oracle takes them; `None` is `getDefaultGPUProcessor`.
fn levels() -> Vec<(Option<Value>, OptimizationFlags)> {
    let name = |n: &str| Some(json!(n));
    vec![
        (None, OptimizationFlags::DEFAULT),
        (name("OPTIMIZATION_NONE"), OptimizationFlags::NONE),
        (name("OPTIMIZATION_LOSSLESS"), OptimizationFlags::LOSSLESS),
        (name("OPTIMIZATION_VERY_GOOD"), OptimizationFlags::VERY_GOOD),
        (name("OPTIMIZATION_GOOD"), OptimizationFlags::GOOD),
        (name("OPTIMIZATION_DRAFT"), OptimizationFlags::DRAFT),
        (name("OPTIMIZATION_DEFAULT"), OptimizationFlags::DEFAULT),
        (name("OPTIMIZATION_ALL"), OptimizationFlags::ALL),
    ]
}

/// One extraction.
#[derive(Debug, Clone)]
struct Case {
    label: String,
    ops: Vec<Op>,
    flags: (Option<Value>, OptimizationFlags),
    language: GpuLanguage,
    oracle_language: oracle_gpu::GpuLanguage,
}

impl Case {
    fn processor(&self) -> Value {
        let children: Vec<Value> = self.ops.iter().map(Op::spec).collect();
        json!({"transform": {"class": "GroupTransform", "children": children}})
    }

    fn request(&self) -> GpuShaderRequest {
        let mut processor = self.processor();
        if let Some(flags) = &self.flags.0 {
            processor["optimization"] = flags.clone();
        }
        GpuShaderRequest::new(processor, ShaderSettings::language(self.oracle_language))
    }

    /// The port's GPU processor of the ops, as they are or finalized first.
    fn gpu(&self, finalize_first: bool) -> Result<GpuProcessor, String> {
        let mut raw = OpVec::new();
        for op in &self.ops {
            create_grading_tone_op(&mut raw, op.data(), TransformDirection::Forward);
        }
        if finalize_first && let Err(e) = raw.finalize() {
            return Err(format!("processor: {}", e.message()));
        }
        GpuProcessor::new(&raw, self.flags.1).map_err(|e| format!("gpu_processor: {}", e.message()))
    }
}

/// A uniform: its name, type, buffer offset and value.
type UniformRow = (String, String, u64, UniformValue);

/// A dynamic property: its type name and its values' bits.
type DynamicRow = (String, Vec<u64>);

/// An extraction: the shader's text and cache ID, its uniforms, and its dynamic properties.
type Extracted = (String, String, Vec<UniformRow>, Vec<DynamicRow>);

/// What the wheel and the port give for a case.
#[derive(Debug, Clone, PartialEq)]
enum Outcome {
    Raised(String),
    Extracted {
        gpu_cache_id: String,
        is_no_op: bool,
        has_channel_crosstalk: bool,
        shader: Result<Extracted, String>,
        /// The writer's warnings (OSL's dynamic properties) in the log.
        writer_log: Vec<String>,
    },
}

fn wheel(reply: &GpuShaderReply) -> Outcome {
    let result = &reply.result;
    if let Some(raised) = reply.raised()
        && raised.stage != "extract"
    {
        return Outcome::Raised(format!("{}: {}", raised.stage, raised.message));
    }
    let shader = match reply.raised() {
        Some(raised) => Err(raised.message),
        None => {
            let shader = reply.shader();
            let uniforms = shader
                .uniforms
                .iter()
                .map(|u| {
                    (
                        u.name.clone(),
                        u.kind.clone(),
                        u.buffer_offset,
                        u.value.clone(),
                    )
                })
                .collect();
            let dynamics = shader
                .dynamic_properties
                .iter()
                .map(|d| (d.kind.clone(), bits(&tone_of(d.value.object()))))
                .collect();
            Ok((
                shader.text.clone(),
                shader.cache_id.clone(),
                uniforms,
                dynamics,
            ))
        }
    };
    Outcome::Extracted {
        writer_log: writer_log(reply.log()),
        gpu_cache_id: result["gpu_cache_id"].as_str().unwrap().to_string(),
        is_no_op: result["gpu_processor"]["isNoOp"].as_bool().unwrap(),
        has_channel_crosstalk: result["gpu_processor"]["hasChannelCrosstalk"]
            .as_bool()
            .unwrap(),
        shader,
    }
}

/// The messages of the writer's warning.
fn writer_log(log: Vec<String>) -> Vec<String> {
    log.into_iter()
        .filter(|m| m.contains("Open Shading language"))
        .collect()
}

/// What the port logged since the last call.
fn take_log() -> Vec<String> {
    std::mem::take(&mut *LOG.lock().unwrap_or_else(PoisonError::into_inner))
}

/// The port's log. The logging state is global, so the tests of this process run one at a
/// time ([`SERIAL`]).
static LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Held by each test for its whole run, so that no test logs into another's [`LOG`].
static SERIAL: Mutex<()> = Mutex::new(());

/// Sends the port's log to [`LOG`], after OCIO's one-time read of `OCIO_LOGGING_LEVEL` in an
/// empty environment, as the oracle's process reads it.
fn capture_log() {
    platform::set_env_provider(Some(Arc::new(MapEnv::default())));
    let _ = get_logging_level();
    platform::set_env_provider(None);
    set_logging_function(Some(Arc::new(|line: &[u8]| {
        LOG.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(String::from_utf8(line.to_vec()).expect("UTF-8"));
    })))
    .unwrap();
}

/// The port's uniforms, read with their getters as the oracle reads the wheel's.
fn port_uniforms(desc: &GpuShaderDesc) -> Vec<UniformRow> {
    let text = |b: &[u8]| String::from_utf8(b.to_vec()).unwrap();
    desc.uniforms()
        .iter()
        .map(|u| {
            let (kind, value) = match u.data() {
                UniformData::Double(get) => ("UNIFORM_DOUBLE", UniformValue::Double(get())),
                UniformData::Bool(get) => ("UNIFORM_BOOL", UniformValue::Bool(get())),
                UniformData::Float3(get) => ("UNIFORM_FLOAT3", UniformValue::Float3(get())),
                UniformData::VectorFloat { size, values } => (
                    "UNIFORM_VECTOR_FLOAT",
                    UniformValue::VectorFloat(values()[..size() as usize].to_vec()),
                ),
                UniformData::VectorInt { size, values } => (
                    "UNIFORM_VECTOR_INT",
                    UniformValue::VectorInt(values()[..size() as usize].to_vec()),
                ),
            };
            (
                text(u.name()),
                kind.to_string(),
                u.buffer_offset() as u64,
                value,
            )
        })
        .collect()
}

fn port(case: &Case, finalize_first: bool) -> Outcome {
    take_log();
    let text = |b: &[u8]| String::from_utf8(b.to_vec()).unwrap();
    let gpu = match case.gpu(finalize_first) {
        Ok(gpu) => gpu,
        Err(e) => return Outcome::Raised(e),
    };
    let mut desc = GpuShaderDesc::new(case.language);
    let shader = gpu
        .extract_gpu_shader_info(&mut desc)
        .map(|()| {
            let dynamics = desc
                .dynamic_properties()
                .iter()
                .map(|p| match p {
                    DynamicPropertyRcPtr::GradingTone(p) => (
                        "DYNAMIC_PROPERTY_GRADING_TONE".to_string(),
                        bits(&p.get_value()),
                    ),
                    other => panic!("not a grading tone property: {other:?}"),
                })
                .collect();
            (
                text(desc.shader_text()),
                text(&desc.cache_id()),
                port_uniforms(&desc),
                dynamics,
            )
        })
        .map_err(|e| e.message().to_string());
    Outcome::Extracted {
        writer_log: writer_log(take_log()),
        gpu_cache_id: text(gpu.get_cache_id()),
        is_no_op: gpu.is_no_op(),
        has_channel_crosstalk: gpu.has_channel_crosstalk(),
        shader,
    }
}

/// Runs every case through the wheel and the port, and fails with every case that differs.
fn check(cases: &[Case]) {
    let requests: Vec<GpuShaderRequest> = cases.iter().map(Case::request).collect();
    let calls: Vec<_> = requests.iter().map(GpuShaderRequest::call).collect();
    let mut failures: Vec<(&Case, Outcome, Outcome)> = Vec::new();
    let (mut extracted, mut with_uniforms, mut empty) = (0, 0, 0);
    for (case, response) in cases.iter().zip(Oracle::get().batch(&calls, true)) {
        let reply = GpuShaderReply::from_response(
            response.unwrap_or_else(|e| panic!("{}: the oracle failed: {e}", case.label)),
        );
        let wheel = wheel(&reply);
        if let Outcome::Extracted { shader: Ok(s), .. } = &wheel {
            extracted += 1;
            if !s.2.is_empty() {
                with_uniforms += 1;
            }
            if !s.0.contains("GradingTone") {
                empty += 1;
            }
        }
        for finalize_first in [false, true] {
            let port = port(case, finalize_first);
            if wheel != port {
                failures.push((case, wheel.clone(), port));
                break;
            }
        }
    }
    if let Some((case, wheel, port)) = failures.first() {
        if let (
            Outcome::Extracted { shader: Ok(w), .. },
            Outcome::Extracted { shader: Ok(p), .. },
        ) = (wheel, port)
        {
            assert_text_eq(&case.label, &w.0, &p.0);
        }
        let labels: Vec<&str> = failures.iter().map(|(c, _, _)| c.label.as_str()).collect();
        panic!(
            "{} of {} cases differ; the first, {}:\n  wheel {wheel:?}\n  port  {port:?}\nall: {labels:#?}",
            failures.len(),
            cases.len(),
            case.label
        );
    }
    assert!(extracted > 0, "no case extracted a shader");
    assert!(with_uniforms > 0, "no case has uniforms");
    assert!(empty > 0, "no case writes nothing (an identity)");
}

/// `ops` at every level, in every language.
fn cases_of(label: &str, ops: Vec<Op>) -> Vec<Case> {
    let mut cases = Vec::new();
    for flags in levels() {
        for (language, oracle_language) in GpuLanguage::ALL
            .into_iter()
            .zip(oracle_gpu::GpuLanguage::ALL)
        {
            cases.push(Case {
                label: format!("{label} {:?} {language:?}", flags.0),
                ops: ops.clone(),
                flags: flags.clone(),
                language,
                oracle_language,
            });
        }
    }
    cases
}

const STYLES: [GradingStyle; 3] = [GradingStyle::Log, GradingStyle::Lin, GradingStyle::Video];

/// `GradingRGBMSW(red, green, blue, master, start, width)`.
fn sw(red: f64, green: f64, blue: f64, master: f64, start: f64, width: f64) -> GradingRgbmsw {
    GradingRgbmsw::new(red, green, blue, master, start, width)
}

/// The components of `v` set, its start and width kept.
fn with_rgbm(v: GradingRgbmsw, red: f64, green: f64, blue: f64, master: f64) -> GradingRgbmsw {
    GradingRgbmsw {
        red,
        green,
        blue,
        master,
        ..v
    }
}

/// Upstream's CPU test values (tests/cpu/ops/gradingtone/GradingToneOpCPU_tests.cpp @ v2.5.2:
/// TS1 to TS6) together, in `style`.
fn upstream_values(style: GradingStyle) -> GradingTone {
    let mut t = GradingTone::new(style);
    t.midtones = sw(0.3, 1.0, 1.8, 1.2, 0.47, 0.6);
    t.highlights = sw(0.3, 1.0, 1.8, 1.4, -0.1, 0.9);
    t.shadows = sw(0.3, 1., 1.79, 0.6, 0.8, -0.1);
    t.whites = sw(0.3, 1., 1.9, 0.6, -0.2, 1.4);
    t.blacks = sw(0.3, 1., 1.9, 0.6, 0.8, 0.9);
    t.scontrast = 1.8;
    t
}

/// Values whose text takes every digit (7 significant ones), every component, start and width
/// off its default.
fn digits(style: GradingStyle) -> GradingTone {
    let mut t = GradingTone::new(style);
    t.blacks = with_rgbm(t.blacks, 1.2345678901, 0.1 + 1.0 / 3.0, 1.8999999, 0.75);
    t.shadows = with_rgbm(t.shadows, 0.4123456789, 1.0 / 7.0 + 1.0, 0.2000001, 1.6);
    t.midtones = with_rgbm(t.midtones, 1.0 / 3.0 + 0.5, 1.7654321, 0.123456789, 1.05);
    t.highlights = with_rgbm(t.highlights, 1.2345678, 0.25, 1.75, 0.9999999);
    t.whites = with_rgbm(t.whites, 0.6789, 1.4321, 1.0 / 3.0 + 1.0, 0.3);
    t.blacks.start += 0.01234;
    t.blacks.width += 0.0987654321;
    t.shadows.start += 0.0765432;
    t.shadows.width -= 0.0123456789;
    t.midtones.start += 0.0123456789;
    t.midtones.width = 0.4321098765;
    t.highlights.start -= 0.1234567;
    t.highlights.width += 0.0456789;
    t.whites.start -= 0.0345678;
    t.whites.width = 0.6543219;
    t.scontrast = 0.876543219;
    t
}

/// Extreme values: components at their bounds, starts and widths far out (their pre-render
/// form overflows a float) or infinite, an s-contrast at its lower bound.
fn extremes(style: GradingStyle) -> GradingTone {
    let mut t = GradingTone::new(style);
    t.blacks = with_rgbm(t.blacks, 0.1, 1.9, 1.0, 0.1);
    t.shadows = with_rgbm(t.shadows, 0.2, 1.8, 0.2, 1.0);
    t.midtones = with_rgbm(t.midtones, 1.9, 0.1, 1.0, 1.9);
    t.highlights = with_rgbm(t.highlights, 1.8, 0.2, 1.8, 1.0);
    t.whites = with_rgbm(t.whites, 0.1, 1.9, 1.9, 1.0);
    t.highlights.start = -1e300;
    t.highlights.width = 1e300;
    t.shadows.start = 1e300;
    t.shadows.width = f64::NEG_INFINITY;
    t.whites.start = -3e38;
    t.whites.width = 3e38;
    t.blacks.width = 1e30;
    t.midtones.start = -1e300;
    t.midtones.width = f64::INFINITY;
    t.scontrast = 0.01;
    t
}

/// Upstream's values with the highlights' and shadows' starts closer to their pivots than 0.01
/// (`validate` allows them within 1e-6 of it): the pre-render clamps them, and the shader takes
/// the clamped starts.
fn clamped_starts(style: GradingStyle) -> GradingTone {
    let mut t = upstream_values(style);
    t.highlights.start = t.highlights.width - 0.0099995;
    t.shadows.start = t.shadows.width + 0.0099995;
    t
}

/// Every style, forward and inverse, fixed and dynamic, at the defaults (an identity), with
/// upstream's CPU test values, with values whose text takes every digit, with extreme and
/// infinite values, and with starts the pre-render clamps; two dynamic ops of a style in one group (the second can't add its
/// property), dynamic and not, ops of every style together, a pair of inverses; every level,
/// in every language.
#[test]
fn grading_tone_shaders_match_the_wheel() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    capture_log();
    let mut lists: Vec<(String, Vec<Op>)> = Vec::new();
    for style in STYLES {
        let s = format!("{style:?}");
        for (vlabel, values) in [
            ("defaults", GradingTone::new(style)),
            ("upstream", upstream_values(style)),
            ("digits", digits(style)),
            ("extremes", extremes(style)),
            ("clamped starts", clamped_starts(style)),
        ] {
            for inverse in [false, true] {
                for dynamic in [false, true] {
                    let mut op = Op::new(style, values);
                    if inverse {
                        op = op.inverse();
                    }
                    if dynamic {
                        op = op.dynamic();
                    }
                    let label = format!(
                        "{s} {vlabel}{}{}",
                        if dynamic { " dynamic" } else { "" },
                        if inverse { " inverse" } else { "" }
                    );
                    lists.push((label, vec![op]));
                }
            }
        }
        let a = Op::new(style, upstream_values(style));
        let b = Op::new(style, digits(style));
        lists.push((
            format!("{s} two dynamic ops"),
            vec![a.clone().dynamic(), b.clone().dynamic().inverse()],
        ));
        lists.push((
            format!("{s} dynamic and not"),
            vec![a.clone().dynamic(), b.clone()],
        ));
        lists.push((
            format!("{s} a pair of inverses"),
            vec![a.clone(), a.clone().inverse()],
        ));
    }
    lists.push((
        "every style, dynamic".to_string(),
        STYLES
            .iter()
            .map(|&s| Op::new(s, upstream_values(s)).dynamic())
            .collect(),
    ));
    let mut cases = Vec::new();
    for (label, ops) in lists {
        cases.extend(cases_of(&label, ops));
    }
    check(&cases);
}

/// A uniform as the oracle's `dynamic_apply` writes it: its name and value (floats by their
/// bits).
fn uniform_json(name: &str, value: &UniformValue) -> Value {
    match value {
        UniformValue::Double(v) => json!([name, v.to_bits()]),
        UniformValue::Bool(v) => json!([name, v]),
        UniformValue::Float3(v) => json!([name, v.map(f32::to_bits)]),
        other => panic!("not a GradingTone uniform: {other:?}"),
    }
}

/// The uniforms read the values set on the description's dynamic property: for each style and
/// direction, `dynamic_apply` takes the property from the GPU description, sets values one
/// after another (each style's, the defaults, which bypass, extreme, infinite and NaN values,
/// starts the pre-render clamps, one the setter refuses) and reads the uniforms after each set; the port sets the same values on its
/// description's property and reads its uniforms.
#[test]
fn uniforms_read_the_values_set() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let mut failures = Vec::new();
    let mut compared = 0;
    for style in STYLES {
        let mut nan = digits(style);
        nan.shadows.green = f64::NAN;
        nan.whites.master = -f64::NAN;
        nan.midtones.width = f64::NAN;
        nan.highlights.start = f64::NAN;
        nan.scontrast = f64::NAN;
        let mut refused = digits(style);
        refused.blacks.red = 2.0;
        let sets = [
            digits(style),
            GradingTone::new(style),
            extremes(style),
            nan,
            clamped_starts(style),
            refused,
            upstream_values(style),
        ];
        for inverse in [false, true] {
            let mut op = Op::new(style, upstream_values(style)).dynamic();
            if inverse {
                op = op.inverse();
            }
            let case = Case {
                label: format!("{style:?} inverse {inverse}"),
                ops: vec![op],
                flags: (None, OptimizationFlags::DEFAULT),
                language: GpuLanguage::Glsl4_0,
                oracle_language: oracle_gpu::GpuLanguage::Glsl40,
            };
            let mut steps = vec![
                json!({"get": "DYNAMIC_PROPERTY_GRADING_TONE", "from": "gpu", "as": "p"}),
                json!({"uniforms": true}),
            ];
            for v in &sets {
                steps.push(json!({"set": "p", "value": tone_spec(v)}));
                steps.push(json!({"uniforms": true}));
            }
            let mut args = case.processor();
            args["gpu"] = json!({"shader": {"language": "GPU_LANGUAGE_GLSL_4_0"}});
            args["buffers"] = json!([]);
            args["images"] = json!([]);
            args["steps"] = json!(steps);
            let response = Oracle::get().call("dynamic_apply", args, &[]);
            let result = &response.result;
            assert!(result.get("exception").is_none(), "{result}");
            let wheel_steps = result["steps"].as_array().unwrap();
            assert_eq!(wheel_steps.len(), 2 + 2 * sets.len(), "{result}");

            let gpu = case.gpu(false).unwrap();
            let mut desc = GpuShaderDesc::new(case.language);
            gpu.extract_gpu_shader_info(&mut desc).unwrap();
            let property = desc.dynamic_properties()[0].as_grading_tone().unwrap();
            let uniform_values = |desc: &GpuShaderDesc| -> Vec<Value> {
                port_uniforms(desc)
                    .into_iter()
                    .map(|(name, _, _, value)| uniform_json(&name, &value))
                    .collect()
            };
            let wheel_values = |step: &Value| -> Vec<Value> {
                step["result"]
                    .as_array()
                    .unwrap_or_else(|| panic!("{step}"))
                    .iter()
                    .map(|u| json!([u["name"], u["value"]]))
                    .collect()
            };
            let mut k = 1;
            for set in std::iter::once(None).chain(sets.iter().map(Some)) {
                if let Some(v) = set {
                    let wheel_set = &wheel_steps[k];
                    match property.set_value(v) {
                        Ok(()) => assert!(wheel_set.get("exception").is_none(), "{wheel_set}"),
                        Err(e) => assert_eq!(
                            wheel_set["exception"]["message"],
                            json!(e.to_string()),
                            "{wheel_set}"
                        ),
                    }
                    k += 1;
                }
                let wheel = wheel_values(&wheel_steps[k]);
                let port = uniform_values(&desc);
                assert!(!wheel.is_empty());
                compared += 1;
                if wheel != port {
                    failures.push(format!(
                        "{} after {set:?}: wheel {wheel:?} port {port:?}",
                        case.label
                    ));
                }
                k += 1;
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert_eq!(compared, 3 * 2 * 8);
}

/// The shader description's property is a copy of the op's: after the shader program is
/// written, a set on the op data's own property leaves the uniforms as they were, and a set on
/// the description's property changes them. The wheel's copy (`createEditableCopy`,
/// `AddGTProperties`, GradingToneOpGPU.cpp:89-264 @ v2.5.2) gives the
/// uniforms' values before and after a set on the description's property, which the port's
/// uniforms must follow; the set on the op data's property is the port's own step (the
/// wheel's processor keeps its ops out of reach), expected to change nothing, as the copy
/// guarantees.
///
/// The port's step is a unit-level test: the port's GPU processor clones its ops, so a set
/// on the original property is out of the pipeline's reach, and the test writes the shader
/// program of the op data directly (`get_grading_tone_gpu_shader_program`).
#[test]
fn shader_property_is_decoupled_from_the_op() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let mut compared = 0;
    for style in STYLES {
        for inverse in [false, true] {
            let mut op = Op::new(style, upstream_values(style)).dynamic();
            if inverse {
                op = op.inverse();
            }
            let case = Case {
                label: format!("{style:?} inverse {inverse}"),
                ops: vec![op.clone()],
                flags: (None, OptimizationFlags::DEFAULT),
                language: GpuLanguage::Glsl4_0,
                oracle_language: oracle_gpu::GpuLanguage::Glsl40,
            };
            let after = extremes(style);
            let mut args = case.processor();
            args["gpu"] = json!({"shader": {"language": "GPU_LANGUAGE_GLSL_4_0"}});
            args["buffers"] = json!([]);
            args["images"] = json!([]);
            args["steps"] = json!([
                {"get": "DYNAMIC_PROPERTY_GRADING_TONE", "from": "gpu", "as": "p"},
                {"uniforms": true},
                {"set": "p", "value": tone_spec(&after)},
                {"uniforms": true},
            ]);
            let response = Oracle::get().call("dynamic_apply", args, &[]);
            let result = &response.result;
            assert!(result.get("exception").is_none(), "{result}");
            let wheel_steps = result["steps"].as_array().unwrap();
            assert_eq!(wheel_steps.len(), 4, "{result}");
            let wheel_values = |step: &Value| -> Vec<Value> {
                step["result"]
                    .as_array()
                    .unwrap_or_else(|| panic!("{step}"))
                    .iter()
                    .map(|u| json!([u["name"], u["value"]]))
                    .collect()
            };
            let (wheel_before, wheel_after) =
                (wheel_values(&wheel_steps[1]), wheel_values(&wheel_steps[3]));
            assert!(!wheel_before.is_empty());
            assert_ne!(
                wheel_before, wheel_after,
                "{}: the set changes no uniform",
                case.label
            );

            let data = op.data();
            let mut desc = GpuShaderDesc::new(case.language);
            get_grading_tone_gpu_shader_program(&mut desc, &data).unwrap();
            let uniform_values = |desc: &GpuShaderDesc| -> Vec<Value> {
                port_uniforms(desc)
                    .into_iter()
                    .map(|(name, _, _, value)| uniform_json(&name, &value))
                    .collect()
            };
            assert_eq!(uniform_values(&desc), wheel_before, "{}", case.label);

            // A set on the op data's property: the description's uniforms keep their values.
            data.get_dynamic_property_internal()
                .set_value(&digits(style))
                .unwrap();
            assert_eq!(
                uniform_values(&desc),
                wheel_before,
                "{}: the description's uniforms follow the op's property",
                case.label
            );

            // A set on the description's property: the uniforms follow it, as the wheel's.
            desc.dynamic_properties()[0]
                .as_grading_tone()
                .unwrap()
                .set_value(&after)
                .unwrap();
            assert_eq!(uniform_values(&desc), wheel_after, "{}", case.label);
            compared += 1;
        }
    }
    assert_eq!(compared, 3 * 2);
}
