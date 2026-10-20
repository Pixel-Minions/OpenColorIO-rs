// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The ExposureContrast op's GPU writer against the wheel, in the 10 languages:
//! - `gpu_shader`: for each case, the GPU processor's cache ID and queries, and what the
//!   extraction writes (the shader's text, cache ID, uniforms with their values, and dynamic
//!   properties), or the error, byte for byte;
//! - `dynamic_apply`: the uniforms' values after each value set on the description's dynamic
//!   properties (O5.1).
//!
//! The wheel builds `ExposureContrastTransform`s in a group, and its processor's GPU processor.
//! The port builds the op data as the binding's constructor does
//! (src/bindings/python/transforms/PyExposureContrastTransform.cpp:19-44 @ v2.5.2), the ops
//! with `create_exposure_contrast_op` (`BuildExposureContrastOp`,
//! src/OpenColorIO/ops/exposurecontrast/ExposureContrastOp.cpp:212-222), and runs the GPU
//! processor from them, as they are and finalized first. A dynamic value becomes a uniform
//! bound to the shader's own copy of the property (in OSL, which has no dynamic properties, a
//! constant and a warning).

use ocio_gpu::gpu_processor::GpuProcessor;
use ocio_gpu::gpu_shader::UniformData;
use ocio_gpu::{GpuLanguage, GpuShaderDesc};
use std::sync::{Arc, Mutex, PoisonError};

use ocio_ops::dynamic_property::DynamicPropertyRcPtr;
use ocio_ops::logging::{get_logging_level, set_logging_function};
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{
    DynamicPropertyType, ExposureContrastStyle, OptimizationFlags, TransformDirection,
};
use ocio_ops::ops::exposurecontrast::exposure_contrast_op::create_exposure_contrast_op;
use ocio_ops::ops::exposurecontrast::exposure_contrast_op_data::ExposureContrastOpData;
use ocio_ops::platform::{self, MapEnv};
use ocio_testkit::gpu::{
    self as oracle_gpu, GpuShaderReply, GpuShaderRequest, ShaderSettings, UniformValue,
};
use ocio_testkit::processor_ops::Dumped;
use ocio_testkit::transform_text::f64_spec;
use ocio_testkit::{Oracle, assert_text_eq};
use serde_json::{Value, json};

/// One ExposureContrast transform: style, direction, the six values (exposure, contrast,
/// gamma, pivot, log exposure step, log mid-gray), and which of the first three are dynamic.
#[derive(Debug, Clone)]
struct Op {
    style: ExposureContrastStyle,
    inverse: bool,
    values: [f64; 6],
    dynamic: [bool; 3],
}

impl Op {
    fn new(style: ExposureContrastStyle, values: [f64; 6]) -> Op {
        Op {
            style,
            inverse: false,
            values,
            dynamic: [false; 3],
        }
    }

    fn inverse(mut self) -> Op {
        self.inverse = true;
        self
    }

    fn dynamic(mut self, dynamic: [bool; 3]) -> Op {
        self.dynamic = dynamic;
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
            ExposureContrastStyle::Linear => "EXPOSURE_CONTRAST_LINEAR",
            ExposureContrastStyle::Video => "EXPOSURE_CONTRAST_VIDEO",
            ExposureContrastStyle::Logarithmic => "EXPOSURE_CONTRAST_LOGARITHMIC",
        }
    }

    fn spec(&self) -> Value {
        let v = |i: usize| f64_spec(self.values[i]);
        let dir = if self.inverse {
            "TRANSFORM_DIR_INVERSE"
        } else {
            "TRANSFORM_DIR_FORWARD"
        };
        json!({"class": "ExposureContrastTransform", "args": {
            "style": {"enum": self.style_name()},
            "exposure": v(0),
            "contrast": v(1),
            "gamma": v(2),
            "pivot": v(3),
            "logExposureStep": v(4),
            "logMidGray": v(5),
            "dynamicExposure": self.dynamic[0],
            "dynamicContrast": self.dynamic[1],
            "dynamicGamma": self.dynamic[2],
            "direction": {"enum": dir},
        }})
    }

    /// The op data as the binding's constructor makes it, then `BuildExposureContrastOp`'s
    /// copy.
    fn data(&self) -> ExposureContrastOpData {
        let mut data =
            ExposureContrastOpData::new(ExposureContrastOpData::convert_transform_style(
                self.style,
                TransformDirection::Forward,
            ));
        data.set_exposure(self.values[0]);
        data.set_contrast(self.values[1]);
        data.set_gamma(self.values[2]);
        data.set_pivot(self.values[3]);
        data.set_log_exposure_step(self.values[4]);
        data.set_log_mid_gray(self.values[5]);
        if self.dynamic[0] {
            data.get_exposure_property().make_dynamic();
        }
        if self.dynamic[1] {
            data.get_contrast_property().make_dynamic();
        }
        if self.dynamic[2] {
            data.get_gamma_property().make_dynamic();
        }
        data.set_direction(self.direction());
        data.validate();
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
            create_exposure_contrast_op(&mut raw, op.data(), TransformDirection::Forward);
        }
        if finalize_first && let Err(e) = raw.finalize() {
            return Err(format!("processor: {}", e.message()));
        }
        GpuProcessor::new(&raw, self.flags.1).map_err(|e| format!("gpu_processor: {}", e.message()))
    }
}

/// A uniform: its name, type, buffer offset and value.
type UniformRow = (String, String, u64, UniformValue);

/// A dynamic property: its type name and the bits of its value.
type DynamicRow = (String, u64);

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

fn type_name(t: DynamicPropertyType) -> &'static str {
    match t {
        DynamicPropertyType::Exposure => "DYNAMIC_PROPERTY_EXPOSURE",
        DynamicPropertyType::Contrast => "DYNAMIC_PROPERTY_CONTRAST",
        DynamicPropertyType::Gamma => "DYNAMIC_PROPERTY_GAMMA",
        other => panic!("not a double property: {other:?}"),
    }
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
                .map(|d| match d.value {
                    Dumped::F64(v) => (d.kind.clone(), v.to_bits()),
                    ref other => panic!("not a double: {other:?}"),
                })
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
                    DynamicPropertyRcPtr::Double(p) => {
                        (type_name(p.get_type()).to_string(), p.get_value().to_bits())
                    }
                    other => panic!("not a double property: {other:?}"),
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
    let mut extracted = 0;
    for (case, response) in cases.iter().zip(Oracle::get().batch(&calls, true)) {
        let reply = GpuShaderReply::from_response(
            response.unwrap_or_else(|e| panic!("{}: the oracle failed: {e}", case.label)),
        );
        let wheel = wheel(&reply);
        if matches!(wheel, Outcome::Extracted { shader: Ok(_), .. }) {
            extracted += 1;
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

const STYLES: [ExposureContrastStyle; 3] = [
    ExposureContrastStyle::Linear,
    ExposureContrastStyle::Video,
    ExposureContrastStyle::Logarithmic,
];

/// Every style, forward and inverse, fixed and dynamic (each property alone and all three),
/// at the defaults (an identity), with upstream's CPU test values, and with values whose text
/// takes every digit (7 significant ones, exponents, the extremes); two dynamic ops of a type
/// in one group (the second can't add its property); every level, in every language.
#[test]
fn exposure_contrast_shaders_match_the_wheel() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    capture_log();
    let defaults = [0.0, 1.0, 1.0, 0.18, 0.088, 0.435];
    let upstream = [1.5, 0.5, 1.1, 0.18, 0.088, 0.435];
    let digits = [
        -1.2345678901,
        1.0 / 3.0,
        1.25e-7,
        12345.678,
        0.0731234567,
        0.4123456789,
    ];
    let extremes = [1e30, 1e-30, 3.0, 1e300, 0.0, -1e-300];
    let mut lists: Vec<(String, Vec<Op>)> = Vec::new();
    for style in STYLES {
        let s = format!("{style:?}");
        for (vlabel, values) in [
            ("defaults", defaults),
            ("upstream", upstream),
            ("digits", digits),
            ("extremes", extremes),
        ] {
            for inverse in [false, true] {
                for (dlabel, dynamic) in [
                    ("fixed", [false; 3]),
                    ("dynamic exposure", [true, false, false]),
                    ("dynamic contrast", [false, true, false]),
                    ("dynamic gamma", [false, false, true]),
                    ("dynamic", [true; 3]),
                ] {
                    let mut op = Op::new(style, values).dynamic(dynamic);
                    if inverse {
                        op = op.inverse();
                    }
                    let label = format!(
                        "{s} {vlabel} {dlabel}{}",
                        if inverse { " inverse" } else { "" }
                    );
                    lists.push((label, vec![op]));
                }
            }
        }
    }
    let lin = Op::new(ExposureContrastStyle::Linear, upstream);
    let log = Op::new(ExposureContrastStyle::Logarithmic, digits);
    lists.push((
        "two dynamic exposures".to_string(),
        vec![
            lin.clone().dynamic([true, false, false]),
            log.clone().dynamic([true, false, true]),
        ],
    ));
    lists.push((
        "dynamic and not".to_string(),
        vec![lin.clone().dynamic([true; 3]), log.clone()],
    ));
    lists.push((
        "a pair of inverses".to_string(),
        vec![lin.clone(), lin.clone().inverse()],
    ));
    let mut cases = Vec::new();
    for (label, ops) in lists {
        cases.extend(cases_of(&label, ops));
    }
    check(&cases);
}

/// The uniforms read the values set on the description's dynamic properties: for each style
/// and direction with the three properties dynamic, `dynamic_apply` takes each property from
/// the GPU description, sets values one after another and reads the uniforms after each set;
/// the port sets the same values on its description's properties and reads its uniforms.
#[test]
fn uniforms_read_the_values_set() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let sets: [(DynamicPropertyType, f64); 7] = [
        (DynamicPropertyType::Exposure, 1.5),
        (DynamicPropertyType::Contrast, 0.25),
        (DynamicPropertyType::Gamma, -2.0),
        (DynamicPropertyType::Exposure, f64::NAN),
        (DynamicPropertyType::Contrast, 1e300),
        (DynamicPropertyType::Gamma, -0.0),
        (DynamicPropertyType::Exposure, 5e-324),
    ];
    let short = |t: DynamicPropertyType| match t {
        DynamicPropertyType::Exposure => "e",
        DynamicPropertyType::Contrast => "c",
        _ => "g",
    };
    let mut failures = Vec::new();
    for style in STYLES {
        for inverse in [false, true] {
            let mut op = Op::new(style, [0.3, 0.9, 1.1, 0.2, 0.08, 0.42]).dynamic([true; 3]);
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
            let mut steps = Vec::new();
            for t in [
                DynamicPropertyType::Exposure,
                DynamicPropertyType::Contrast,
                DynamicPropertyType::Gamma,
            ] {
                steps.push(json!({"get": type_name(t), "from": "gpu", "as": short(t)}));
            }
            steps.push(json!({"uniforms": true}));
            for (t, v) in sets {
                steps.push(json!({"set": short(t), "value": f64_spec(v)}));
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

            let gpu = case.gpu(false).unwrap();
            let mut desc = GpuShaderDesc::new(case.language);
            gpu.extract_gpu_shader_info(&mut desc).unwrap();
            let property = |t: DynamicPropertyType| {
                desc.dynamic_properties()
                    .iter()
                    .find(|p| p.get_type() == t)
                    .unwrap()
                    .as_double()
                    .unwrap()
            };
            let uniform_values = |desc: &GpuShaderDesc| -> Vec<Value> {
                port_uniforms(desc)
                    .into_iter()
                    .map(|(name, _, _, value)| match value {
                        UniformValue::Double(v) => json!([name, v.to_bits()]),
                        other => panic!("not a double uniform: {other:?}"),
                    })
                    .collect()
            };
            let wheel_values = |step: &Value| -> Vec<Value> {
                step["result"]
                    .as_array()
                    .unwrap_or_else(|| panic!("{step}"))
                    .iter()
                    .map(|u| {
                        assert_eq!(u["type"], json!("UNIFORM_DOUBLE"), "{u}");
                        json!([u["name"], u["value"].as_u64().expect("the bits")])
                    })
                    .collect()
            };
            let mut k = 3;
            for set in std::iter::once(None).chain(sets.iter().map(Some)) {
                if let Some((t, v)) = set {
                    property(*t).set_value(*v);
                    k += 1;
                }
                let wheel = wheel_values(&wheel_steps[k]);
                let port = uniform_values(&desc);
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
}
