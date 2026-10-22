// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The GradingPrimary op's GPU writer against the wheel, in the 10 languages:
//! - `gpu_shader`: for each case, the GPU processor's cache ID and queries, and what the
//!   extraction writes (the shader's text, cache ID, uniforms with their values, and dynamic
//!   properties with their values), or the error, byte for byte;
//! - `dynamic_apply`: the uniforms' values after each value set on the description's dynamic
//!   property (O5.1);
//! - `shader_property_is_decoupled_from_the_op`: a set on the op data's property after the shader
//!   program is written leaves the uniforms as they were (a unit-level test of
//!   `get_grading_primary_gpu_shader_program`: the GPU processor clones its ops).
//!
//! The wheel builds `GradingPrimaryTransform`s in a group, and its processor's GPU processor.
//! The port builds the op data as the binding's constructor does
//! (src/bindings/python/transforms/PyGradingPrimaryTransform.cpp:17-35 @ v2.5.2), the ops with
//! `create_grading_primary_op` (`BuildGradingPrimaryOp`,
//! src/OpenColorIO/ops/gradingprimary/GradingPrimaryOp.cpp:268-279), and runs the GPU
//! processor from them, as they are and finalized first. A dynamic op's values become uniforms
//! bound to the shader's own copy of the property (in OSL, which has no dynamic properties,
//! constants and a warning).

use ocio_gpu::gpu_processor::GpuProcessor;
use ocio_gpu::gpu_shader::UniformData;
use ocio_gpu::ops::gradingprimary::grading_primary_op_gpu::get_grading_primary_gpu_shader_program;
use ocio_gpu::{GpuLanguage, GpuShaderDesc};
use std::sync::{Arc, Mutex, PoisonError};

use ocio_ops::dynamic_property::DynamicPropertyRcPtr;
use ocio_ops::logging::{get_logging_level, set_logging_function};
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{GradingStyle, OptimizationFlags, TransformDirection};
use ocio_ops::ops::gradingprimary::grading_primary::{GradingPrimary, GradingRgbm};
use ocio_ops::ops::gradingprimary::grading_primary_op::create_grading_primary_op;
use ocio_ops::ops::gradingprimary::grading_primary_op_data::GradingPrimaryOpData;
use ocio_ops::platform::{self, MapEnv};
use ocio_testkit::gpu::{
    self as oracle_gpu, GpuShaderReply, GpuShaderRequest, ShaderSettings, UniformValue,
};
use ocio_testkit::processor_ops::{Dump, Dumped};
use ocio_testkit::transform_text::f64_spec;
use ocio_testkit::{Oracle, assert_text_eq};
use serde_json::{Value, json};

/// The doubles of a GradingPrimary, by their bits, in a fixed order.
fn bits(p: &GradingPrimary) -> Vec<u64> {
    let rgbm = |v: &GradingRgbm| [v.red, v.green, v.blue, v.master];
    let mut out = Vec::new();
    for v in [
        &p.brightness,
        &p.contrast,
        &p.gamma,
        &p.offset,
        &p.exposure,
        &p.lift,
        &p.gain,
    ] {
        out.extend(rgbm(v));
    }
    out.extend([
        p.saturation,
        p.pivot,
        p.pivot_black,
        p.pivot_white,
        p.clamp_black,
        p.clamp_white,
    ]);
    out.iter().map(|v| v.to_bits()).collect()
}

fn getter_f64(dumped: &Dumped) -> f64 {
    match dumped {
        Dumped::F64(v) => *v,
        other => panic!("not a float: {other:?}"),
    }
}

/// The wheel's GradingPrimary, written out, as the port's.
fn primary_of(dump: &Dump) -> GradingPrimary {
    let rgbm = |name: &str| {
        let d = dump.property(name).object();
        GradingRgbm::new(
            getter_f64(d.property("red")),
            getter_f64(d.property("green")),
            getter_f64(d.property("blue")),
            getter_f64(d.property("master")),
        )
    };
    let f = |name: &str| getter_f64(dump.property(name));
    GradingPrimary {
        brightness: rgbm("brightness"),
        contrast: rgbm("contrast"),
        gamma: rgbm("gamma"),
        offset: rgbm("offset"),
        exposure: rgbm("exposure"),
        lift: rgbm("lift"),
        gain: rgbm("gain"),
        saturation: f("saturation"),
        pivot: f("pivot"),
        pivot_black: f("pivotBlack"),
        pivot_white: f("pivotWhite"),
        clamp_black: f("clampBlack"),
        clamp_white: f("clampWhite"),
    }
}

fn rgbm_spec(v: &GradingRgbm) -> Value {
    json!({"object": {"class": "GradingRGBM",
                      "args": [f64_spec(v.red), f64_spec(v.green), f64_spec(v.blue),
                               f64_spec(v.master)]}})
}

/// A GradingPrimary value spec, every double by its bits.
fn primary_spec(p: &GradingPrimary) -> Value {
    json!({"object": {"class": "GradingPrimary", "args": [{"enum": "GRADING_LOG"}], "attrs": [
        ["brightness", rgbm_spec(&p.brightness)],
        ["contrast", rgbm_spec(&p.contrast)],
        ["gamma", rgbm_spec(&p.gamma)],
        ["offset", rgbm_spec(&p.offset)],
        ["exposure", rgbm_spec(&p.exposure)],
        ["lift", rgbm_spec(&p.lift)],
        ["gain", rgbm_spec(&p.gain)],
        ["saturation", f64_spec(p.saturation)],
        ["pivot", f64_spec(p.pivot)],
        ["pivotBlack", f64_spec(p.pivot_black)],
        ["pivotWhite", f64_spec(p.pivot_white)],
        ["clampBlack", f64_spec(p.clamp_black)],
        ["clampWhite", f64_spec(p.clamp_white)],
    ]}})
}

/// One GradingPrimary transform: style, direction, values, dynamic or not.
#[derive(Debug, Clone)]
struct Op {
    style: GradingStyle,
    inverse: bool,
    values: GradingPrimary,
    dynamic: bool,
}

impl Op {
    fn new(style: GradingStyle, values: GradingPrimary) -> Op {
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
        json!({"class": "GradingPrimaryTransform", "args": {
            "values": primary_spec(&self.values),
            "style": {"enum": self.style_name()},
            "dynamic": self.dynamic,
            "dir": {"enum": dir},
        }})
    }

    /// The op data as the binding's constructor makes it, then `BuildGradingPrimaryOp`'s copy.
    fn data(&self) -> GradingPrimaryOpData {
        let mut data = GradingPrimaryOpData::new(self.style);
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
            create_grading_primary_op(&mut raw, op.data(), TransformDirection::Forward);
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
                .map(|d| (d.kind.clone(), bits(&primary_of(d.value.object()))))
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
                    DynamicPropertyRcPtr::GradingPrimary(p) => (
                        "DYNAMIC_PROPERTY_GRADING_PRIMARY".to_string(),
                        bits(&p.get_value()),
                    ),
                    other => panic!("not a grading primary property: {other:?}"),
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
            if !s.0.contains("GradingPrimary") {
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

fn rgbm(red: f64, green: f64, blue: f64, master: f64) -> GradingRgbm {
    GradingRgbm::new(red, green, blue, master)
}

/// Upstream's CPU test values of `style` (tests/cpu/ops/gradingprimary/
/// GradingPrimaryOpCPU_tests.cpp @ v2.5.2: TS1, TS2 and TS3), with their clamps and pivots.
fn upstream_values(style: GradingStyle) -> GradingPrimary {
    let mut p = GradingPrimary::new(style);
    match style {
        GradingStyle::Log => {
            p.brightness = rgbm(-10., 45., -5., 50.);
            p.contrast = rgbm(0.9, 1.4, 0.7, 0.75);
            p.gamma = rgbm(1.1, 0.7, 1.05, 1.15);
            p.pivot = -0.3;
            p.saturation = 1.21;
            p.clamp_black = -0.05;
            p.clamp_white = 1.50;
            p.pivot_black = 0.05;
            p.pivot_white = 0.9;
        }
        GradingStyle::Lin => {
            p.exposure = rgbm(0.5, -0.2, 0.4, -0.25);
            p.offset = rgbm(-0.03, 0.02, 0.1, -0.1);
            p.contrast = rgbm(0.9, 1.4, 0.7, 0.75);
            p.pivot = 0.5;
            p.saturation = 1.33;
            p.clamp_black = -0.40;
            p.clamp_white = 1.05;
        }
        GradingStyle::Video => {
            p.lift = rgbm(0.05, -0.04, 0.02, 0.05);
            p.gamma = rgbm(0.9, 1.4, 0.7, 0.75);
            p.gain = rgbm(1.2, 1.1, 1.25, 0.8);
            p.offset = rgbm(-0.03, 0.02, 0.1, -0.1);
            p.saturation = 1.2;
            p.clamp_black = -0.15;
            p.clamp_white = 1.50;
            p.pivot_black = 0.05;
            p.pivot_white = 0.9;
        }
    }
    p
}

/// Values whose text takes every digit: 7 significant ones, exponents, and every value off
/// its identity.
fn digits(style: GradingStyle) -> GradingPrimary {
    let mut p = GradingPrimary::new(style);
    p.brightness = rgbm(-1.2345678901, 1.0 / 3.0, 12345.678, 1.25e-7);
    p.contrast = rgbm(1.0 / 7.0 + 1.0, 0.0731234567 + 1.0, 2.5, 0.9999999);
    p.gamma = rgbm(1.75, 0.4123456789, 3.0, 1.0 / 3.0 + 0.5);
    p.offset = rgbm(0.0123456789, -0.25, 1e-30, 0.1);
    p.exposure = rgbm(-1.5, 2.25, 0.123456789, 1e-7);
    p.lift = rgbm(-0.012345, 0.05, 0.1234567, -0.0001);
    p.gain = rgbm(1.2345678, 0.75, 2.0, 1.1);
    p.saturation = 0.876543219;
    p.pivot = 0.123456789;
    p.pivot_black = -0.0123456789;
    p.pivot_white = 1.23456789;
    p.clamp_black = -0.1234567;
    p.clamp_white = 12.345678;
    p
}

/// Large and infinite values: clamps that overflow a float, infinite clamps (OSL and Cg write
/// the largest floats), a saturation of 0, and a gamma and contrast of 1 with other changes
/// (no gamma or contrast block's effect).
fn extremes(style: GradingStyle) -> GradingPrimary {
    let mut p = GradingPrimary::new(style);
    p.brightness = rgbm(1e30, 0.0, -1e30, 3.0);
    p.offset = rgbm(1e30, -1e-30, 0.0, 0.5);
    p.exposure = rgbm(100.0, -100.0, 0.0, 1.0);
    p.lift = rgbm(0.5, -0.5, 0.0, 1e-300);
    p.gain = rgbm(1e10, 1.0, 1e-10, 2.0);
    p.saturation = 0.0;
    p.pivot = 1e300;
    p.pivot_black = -1e300;
    p.pivot_white = 1e300;
    p.clamp_black = f64::NEG_INFINITY;
    p.clamp_white = 1e300;
    p
}

/// Every style, forward and inverse, fixed and dynamic, at the defaults (an identity), with
/// upstream's CPU test values, with values whose text takes every digit, and with extreme and
/// infinite values; two dynamic ops of a style in one group (the second can't add its
/// property), dynamic and not, ops of every style together, a pair of inverses; every level,
/// in every language.
#[test]
fn grading_primary_shaders_match_the_wheel() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    capture_log();
    let mut lists: Vec<(String, Vec<Op>)> = Vec::new();
    for style in STYLES {
        let s = format!("{style:?}");
        for (vlabel, values) in [
            ("defaults", GradingPrimary::new(style)),
            ("upstream", upstream_values(style)),
            ("digits", digits(style)),
            ("extremes", extremes(style)),
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
        other => panic!("not a GradingPrimary uniform: {other:?}"),
    }
}

/// The uniforms read the values set on the description's dynamic property: for each style and
/// direction, `dynamic_apply` takes the property from the GPU description, sets values one
/// after another (each style's, the defaults, which bypass, NaN and infinite values, one the
/// setter refuses) and reads the uniforms after each set; the port sets the same values on its
/// description's property and reads its uniforms.
#[test]
fn uniforms_read_the_values_set() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let mut failures = Vec::new();
    let mut compared = 0;
    for style in STYLES {
        let mut nan = digits(style);
        nan.brightness.green = f64::NAN;
        nan.offset.master = -f64::NAN;
        nan.contrast.red = f64::NAN;
        nan.saturation = f64::NAN;
        let mut refused = digits(style);
        refused.pivot_white = -5.0;
        let sets = [
            digits(style),
            GradingPrimary::new(style),
            extremes(style),
            nan,
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
                json!({"get": "DYNAMIC_PROPERTY_GRADING_PRIMARY", "from": "gpu", "as": "p"}),
                json!({"uniforms": true}),
            ];
            for v in &sets {
                steps.push(json!({"set": "p", "value": primary_spec(v)}));
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
            let property = desc.dynamic_properties()[0].as_grading_primary().unwrap();
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
    assert_eq!(compared, 3 * 2 * 7);
}

/// The shader description's property is a copy of the op's: after the shader program is
/// written, a set on the op data's own property leaves the uniforms as they were, and a set on
/// the description's property changes them. The wheel's copy (`createEditableCopy`,
/// `AddGPLogProperties` and its siblings, GradingPrimaryOpGPU.cpp:79-154 @ v2.5.2) gives the
/// uniforms' values before and after a set on the description's property, which the port's
/// uniforms must follow; the set on the op data's property is the port's own step (the
/// wheel's processor keeps its ops out of reach), expected to change nothing, as the copy
/// guarantees.
///
/// The port's step is a unit-level test: the port's GPU processor clones its ops, so a set
/// on the original property is out of the pipeline's reach, and the test writes the shader
/// program of the op data directly (`get_grading_primary_gpu_shader_program`).
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
                {"get": "DYNAMIC_PROPERTY_GRADING_PRIMARY", "from": "gpu", "as": "p"},
                {"uniforms": true},
                {"set": "p", "value": primary_spec(&after)},
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
            get_grading_primary_gpu_shader_program(&mut desc, &data).unwrap();
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
                .as_grading_primary()
                .unwrap()
                .set_value(&after)
                .unwrap();
            assert_eq!(uniform_values(&desc), wheel_after, "{}", case.label);
            compared += 1;
        }
    }
    assert_eq!(compared, 3 * 2);
}
