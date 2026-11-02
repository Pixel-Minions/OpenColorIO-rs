// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! P88: complete legacy post-extraction dynamic observations, against P87/P90's
//! independently verified client of the official library. Processor.cpp:447-523,
//! OpenColorIO.h:2707-2709,3476-3483 and DynamicProperty.cpp @ v2.5.2.
//! Every value below is an input. Expected states come only from the wheel.

mod common;
#[path = "legacy_gpu_live/values.rs"]
mod values;

use common::transforms::grading_style_name;
use ocio::{
    Config, DynamicPropertyType, ExceptionKind, ExposureContrastTransform, GradingHueCurve,
    GradingHueCurveTransform, GradingPrimary, GradingPrimaryTransform, GradingRgbCurve,
    GradingRgbCurveTransform, GradingStyle, GradingTone, GradingToneTransform, GroupTransform,
    Lut1DTransform, OptimizationFlags, Processor, Transform,
};
use ocio_gpu::gpu_processor::GpuProcessor;
use ocio_gpu::gpu_shader::{TextureDimensions, TextureType, UniformData};
use ocio_gpu::{GpuLanguage, GpuShaderDesc};
use ocio_ops::logging::{reset_to_default_logging_function, set_logging_function};
use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;
use ocio_testkit::oracle::{BatchCall, Response};
use ocio_testkit::{Oracle, gpu};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use values::{DynValue, type_name};

const PARTS: usize = 28;
static SERIAL: Mutex<()> = Mutex::new(());

const TYPES: [DynamicPropertyType; 7] = [
    DynamicPropertyType::Exposure,
    DynamicPropertyType::Contrast,
    DynamicPropertyType::Gamma,
    DynamicPropertyType::GradingPrimary,
    DynamicPropertyType::GradingRgbCurve,
    DynamicPropertyType::GradingHueCurve,
    DynamicPropertyType::GradingTone,
];
const LEVELS: [(&str, OptimizationFlags); 7] = [
    ("OPTIMIZATION_NONE", OptimizationFlags::NONE),
    ("OPTIMIZATION_LOSSLESS", OptimizationFlags::LOSSLESS),
    ("OPTIMIZATION_VERY_GOOD", OptimizationFlags::VERY_GOOD),
    ("OPTIMIZATION_GOOD", OptimizationFlags::GOOD),
    ("OPTIMIZATION_DRAFT", OptimizationFlags::DRAFT),
    ("OPTIMIZATION_DEFAULT", OptimizationFlags::DEFAULT),
    ("OPTIMIZATION_ALL", OptimizationFlags::ALL),
];

struct Kind {
    ty: DynamicPropertyType,
    port: Transform,
    spec: Value,
    donor: Value,
    initial: DynValue,
    changed: DynValue,
}

/// Fresh transform/property handles for every request; no previous setter may
/// contaminate the next request's authored initial state.
fn kind(index: usize) -> Kind {
    let ty = TYPES[index];
    if index < 3 {
        let mut port = ExposureContrastTransform::new();
        let (set, make, initial, changed) = match ty {
            DynamicPropertyType::Exposure => {
                port.set_exposure(0.5);
                port.make_exposure_dynamic();
                ("setExposure", "makeExposureDynamic", 0.5, 1.25)
            }
            DynamicPropertyType::Contrast => {
                port.set_contrast(1.2);
                port.make_contrast_dynamic();
                ("setContrast", "makeContrastDynamic", 1.2, 0.8)
            }
            DynamicPropertyType::Gamma => {
                port.set_gamma(1.1);
                port.make_gamma_dynamic();
                ("setGamma", "makeGammaDynamic", 1.1, 0.75)
            }
            _ => unreachable!(),
        };
        return Kind {
            ty,
            port: port.into(),
            spec: json!({"class":"ExposureContrastTransform","calls":[[set,initial],[make]]}),
            donor: json!({"class":"ExposureContrastTransform","calls":[[set,changed]]}),
            initial: DynValue::Double(initial),
            changed: DynValue::Double(changed),
        };
    }
    let style = match ty {
        DynamicPropertyType::GradingPrimary | DynamicPropertyType::GradingHueCurve => {
            GradingStyle::Log
        }
        DynamicPropertyType::GradingRgbCurve => GradingStyle::Lin,
        _ => GradingStyle::Video,
    };
    let (name, port, initial, changed) = match ty {
        DynamicPropertyType::GradingPrimary => {
            let mut t = GradingPrimaryTransform::new(style);
            t.make_dynamic();
            (
                "GradingPrimaryTransform",
                t.into(),
                DynValue::Primary(GradingPrimary::new(style)),
                values::primary(style, 1.0),
            )
        }
        DynamicPropertyType::GradingRgbCurve => {
            let mut t = GradingRgbCurveTransform::new(style);
            t.make_dynamic();
            (
                "GradingRGBCurveTransform",
                t.into(),
                DynValue::RgbCurve(GradingRgbCurve::new(style)),
                values::rgb_curve(1.0),
            )
        }
        DynamicPropertyType::GradingHueCurve => {
            let mut t = GradingHueCurveTransform::new(style);
            t.make_dynamic();
            (
                "GradingHueCurveTransform",
                t.into(),
                DynValue::HueCurve(GradingHueCurve::new(style)),
                values::hue_curve(1.0),
            )
        }
        DynamicPropertyType::GradingTone => {
            let mut t = GradingToneTransform::new(style);
            t.make_dynamic();
            (
                "GradingToneTransform",
                t.into(),
                DynValue::Tone(GradingTone::new(style)),
                values::tone(style, 1.0),
            )
        }
        _ => unreachable!(),
    };
    let args = json!({"style":{"enum":grading_style_name(style)}});
    Kind {
        ty,
        port,
        spec: json!({"class":name,"args":args,"calls":[["makeDynamic"]]}),
        donor: json!({"class":name,"args":args,"calls":[["setValue",changed.spec(style)]]}),
        initial,
        changed,
    }
}

/// The same generic LUT input as NoOps_tests.cpp:32-52 @ v2.5.2 and P34.
fn lattice() -> (Transform, Value) {
    let mut lut = Lut1DTransform::new();
    lut.set_length(256).unwrap();
    let mut calls = vec![json!(["setLength", 256])];
    for index in 0..256 {
        let x = index as f32 / 255.0;
        let v = x * x;
        lut.set_value(index, v, v, v).unwrap();
        calls.push(json!(["setValue", index, v, v, v]));
    }
    (lut.into(), json!({"class":"Lut1DTransform","calls":calls}))
}

fn envelope(k: &Kind, position: usize) -> (Transform, Value) {
    let mut steps = vec![(k.port.clone(), k.spec.clone())];
    if position != 0 {
        steps.insert(0, lattice());
    }
    if position != 2 {
        steps.push(lattice());
    }
    let mut group = GroupTransform::new();
    for (transform, _) in &steps {
        group.append_transform(transform.clone());
    }
    (
        group.into(),
        json!({"class":"GroupTransform","children":steps.into_iter().map(|(_,v)|v).collect::<Vec<_>>()}),
    )
}

#[derive(Clone, Copy, Debug)]
struct Job {
    kind: usize,
    position: usize,
    edge: u32,
    level: usize,
    language: usize,
    before: bool,
}

fn args(job: Job) -> Value {
    let k = kind(job.kind);
    let (_, spec) = envelope(&k, job.position);
    let reset = match &k.spec {
        Value::Object(_) => {
            let mut v = k.spec.clone();
            // The donor only reads values; dynamic flags do not enter the receiver.
            if job.kind >= 3 {
                v.as_object_mut().unwrap().remove("calls");
            }
            v
        }
        _ => unreachable!(),
    };
    json!({"transform":spec,"edge_len":job.edge,"optimization":LEVELS[job.level].0,
        "shader":{"language":gpu::GpuLanguage::ALL[job.language].oracle_name()},
        "before":if job.before {vec![json!({"kind":"default","optimization":LEVELS[job.level].0})]} else {vec![]},
        "updates":[
            {"from":"processor","property":type_name(k.ty),"value_transform":k.donor},
            {"from":"shader","property":type_name(k.ty),"value_transform":k.donor},
            {"from":"processor","property":type_name(k.ty),"value_transform":reset},
            {"from":"shader","property":type_name(k.ty),"value_transform":reset}]})
}

struct Capture(Arc<Mutex<Vec<String>>>);
impl Capture {
    fn new() -> Self {
        let sink = Arc::new(Mutex::new(Vec::new()));
        let output = Arc::clone(&sink);
        set_logging_function(Some(Arc::new(move |v| {
            output
                .lock()
                .unwrap()
                .push(String::from_utf8(v.to_vec()).expect("UTF8 log"))
        })))
        .unwrap();
        Self(sink)
    }
    fn lines(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        reset_to_default_logging_function();
    }
}

fn text(value: &[u8]) -> String {
    String::from_utf8(value.to_vec()).expect("UTF8 API text")
}
fn interpolation(value: Interpolation) -> String {
    match value {
        Interpolation::Unknown => "INTERP_UNKNOWN",
        Interpolation::Nearest => "INTERP_NEAREST",
        Interpolation::Linear => "INTERP_LINEAR",
        Interpolation::Tetrahedral => "INTERP_TETRAHEDRAL",
        Interpolation::Cubic => "INTERP_CUBIC",
        Interpolation::Default => "INTERP_DEFAULT",
        Interpolation::Best => "INTERP_BEST",
    }
    .into()
}

/// Every public shader getter, with typed bitwise comparators from ocio-testkit.
fn shader(desc: &GpuShaderDesc, language: usize) -> gpu::Shader {
    let uniforms = desc
        .uniforms()
        .iter()
        .map(|u| {
            let (kind, value) = match u.data() {
                UniformData::Double(get) => ("UNIFORM_DOUBLE", gpu::UniformValue::Double(get())),
                UniformData::Bool(get) => ("UNIFORM_BOOL", gpu::UniformValue::Bool(get())),
                UniformData::Float3(get) => ("UNIFORM_FLOAT3", gpu::UniformValue::Float3(get())),
                UniformData::VectorFloat { size, values } => (
                    "UNIFORM_VECTOR_FLOAT",
                    gpu::UniformValue::VectorFloat(values()[..size() as usize].to_vec()),
                ),
                UniformData::VectorInt { size, values } => (
                    "UNIFORM_VECTOR_INT",
                    gpu::UniformValue::VectorInt(values()[..size() as usize].to_vec()),
                ),
            };
            gpu::Uniform {
                name: text(u.name()),
                kind: kind.into(),
                buffer_offset: u.buffer_offset() as u64,
                value,
            }
        })
        .collect();
    let textures = (0..desc.num_textures())
        .map(|i| {
            let t = desc.texture(i).unwrap();
            gpu::Texture {
                name: text(t.texture_name()),
                sampler_name: text(t.sampler_name()),
                width: u64::from(t.width()),
                height: u64::from(t.height()),
                channel: match t.channel() {
                    TextureType::RedChannel => "TEXTURE_RED_CHANNEL",
                    TextureType::RgbChannel => "TEXTURE_RGB_CHANNEL",
                }
                .into(),
                dimensions: match t.dimensions() {
                    TextureDimensions::D1 => "TEXTURE_1D",
                    TextureDimensions::D2 => "TEXTURE_2D",
                }
                .into(),
                interpolation: interpolation(t.interpolation()),
                binding_index: u64::from(desc.texture_shader_binding_index(i).unwrap()),
                values: t.values().to_vec(),
            }
        })
        .collect();
    let textures_3d = (0..desc.num_textures_3d())
        .map(|i| {
            let t = desc.texture_3d(i).unwrap();
            gpu::Texture3d {
                name: text(t.texture_name()),
                sampler_name: text(t.sampler_name()),
                edge_len: u64::from(t.edge_len()),
                interpolation: interpolation(t.interpolation()),
                binding_index: u64::from(desc.texture_3d_shader_binding_index(i).unwrap()),
                values: t.values().to_vec(),
            }
        })
        .collect();
    gpu::Shader {
        cache_id: text(&desc.cache_id()),
        text: text(desc.shader_text()),
        uniforms,
        textures,
        textures_3d,
        dynamic_properties: desc
            .dynamic_properties()
            .iter()
            .map(|p| gpu::DynamicProperty {
                kind: type_name(p.get_type()).into(),
                value: values::read(p, p.get_type()).unwrap(),
            })
            .collect(),
        getters: json!({"language":gpu::GpuLanguage::ALL[language].oracle_name(),"function_name":text(desc.function_name()),"pixel_name":text(desc.pixel_name()),
            "resource_prefix":text(desc.resource_prefix()),"uid":text(desc.unique_id()),"descriptor_set_index":desc.descriptor_set_index(),
            "texture_binding_start":desc.texture_binding_start(),"texture_max_width":desc.texture_max_width(),"allow_texture_1d":desc.allow_texture_1d(),"uniform_buffer_size":desc.uniform_buffer_size()}),
    }
}

fn metadata(proc: &Processor, gpu: &GpuProcessor) -> Value {
    json!({"processor_cache_id":proc.cache_id().unwrap(),"gpu_cache_id":text(gpu.get_cache_id()),
    "gpu_processor":{"isNoOp":gpu.is_no_op(),"hasChannelCrosstalk":gpu.has_channel_crosstalk()}})
}

#[derive(Debug, PartialEq)]
struct State {
    metadata: Value,
    shader: gpu::Shader,
    properties: Vec<gpu::DynamicProperty>,
    log: Vec<String>,
}
fn state(
    proc: &Processor,
    gpu: &GpuProcessor,
    desc: &GpuShaderDesc,
    language: usize,
    log: &Capture,
) -> State {
    let metadata = metadata(proc, gpu);
    let shader = shader(desc, language);
    let properties = TYPES
        .iter()
        .filter(|&&ty| proc.has_dynamic_property(ty))
        .map(|&ty| gpu::DynamicProperty {
            kind: type_name(ty).into(),
            value: values::read(&proc.dynamic_property(ty).unwrap(), ty).unwrap(),
        })
        .collect();
    State {
        metadata,
        shader,
        properties,
        log: log.lines(),
    }
}

#[derive(Debug, PartialEq, Default)]
struct Outcome {
    states: Vec<State>,
    initial_metadata: Option<Value>,
    error: Option<(String, String, String)>,
    log: Vec<String>,
}
fn error(stage: &str, e: ocio::Exception) -> (String, String, String) {
    let kind = match e.kind() {
        ExceptionKind::Exception => "Exception",
        ExceptionKind::MissingFile => "ExceptionMissingFile",
        _ => "RuntimeError",
    };
    (
        stage.into(),
        kind.into(),
        String::from_utf8(e.what().to_vec()).expect("UTF8 error"),
    )
}

fn port(job: Job) -> Outcome {
    let capture = Capture::new();
    let mut outcome = Outcome::default();
    let mut stage = "config";
    let run = (|| -> ocio::Result<()> {
        let k = kind(job.kind);
        let (transform, _) = envelope(&k, job.position);
        let config = Config::create_raw()?;
        stage = "processor";
        let proc = config.processor(&transform)?;
        let flags = LEVELS[job.level].1;
        stage = "gpu_processor";
        if job.before {
            proc.optimized_gpu_processor(flags)?;
        }
        let gpu = proc.optimized_legacy_gpu_processor(flags, job.edge)?;
        outcome.initial_metadata = Some(metadata(&proc, &gpu));
        stage = "shader_desc";
        let mut desc = GpuShaderDesc::new(GpuLanguage::ALL[job.language]);
        stage = "extract";
        gpu.extract_gpu_shader_info(&mut desc)?;
        outcome
            .states
            .push(state(&proc, &gpu, &desc, job.language, &capture));
        outcome.initial_metadata = None;
        for (from_shader, value) in [
            (false, &k.changed),
            (true, &k.changed),
            (false, &k.initial),
            (true, &k.initial),
        ] {
            stage = "update";
            let property = if from_shader {
                desc.dynamic_property_by_type(k.ty)?
            } else {
                proc.dynamic_property(k.ty)?
            };
            value.set(&property)?;
            outcome
                .states
                .push(state(&proc, &gpu, &desc, job.language, &capture));
        }
        Ok(())
    })();
    if let Err(e) = run {
        outcome.error = Some(error(stage, e));
    }
    outcome.log = capture.lines();
    outcome
}

fn wheel(response: &Response) -> Outcome {
    let result = &response.result;
    assert!(
        result.get("crashed").is_none(),
        "native oracle crashed: {result}"
    );
    assert_eq!(result["oracle_native"]["wheel_version"], "2.5.2");
    assert_eq!(
        result["oracle_native"]["source_key"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    assert_eq!(
        result["oracle_native"]["library_sha256"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    let meta = |value: &Value| json!({"processor_cache_id":value["processor_cache_id"],"gpu_cache_id":value["gpu_cache_id"],"gpu_processor":value["gpu_processor"]});
    let log = |value: &Value| {
        value["log"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    };
    let mut outcome = Outcome {
        log: log(result),
        ..Outcome::default()
    };
    if let Some(raised) = result.get("exception") {
        outcome.error = Some((
            result["stage"].as_str().unwrap().into(),
            raised["type"].as_str().unwrap().into(),
            raised["message"].as_str().unwrap().into(),
        ));
    }
    if let Some(states) = result.get("states") {
        outcome.states = states
            .as_array()
            .unwrap()
            .iter()
            .map(|s| {
                let reply = gpu::GpuShaderReply::from_response(Response {
                    result: s.clone(),
                    blobs: response.blobs.clone(),
                });
                State {
                    metadata: meta(s),
                    shader: reply.shader().clone(),
                    log: log(s),
                    properties: s["processor_properties"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|p| gpu::DynamicProperty {
                            kind: p["type"].as_str().unwrap().into(),
                            value: ocio_testkit::processor_ops::Dumped::parse(
                                &p["value"],
                                &response.blobs,
                            ),
                        })
                        .collect(),
                }
            })
            .collect();
    } else if result.get("gpu_cache_id").is_some() {
        outcome.initial_metadata = Some(meta(result));
    }
    outcome
}

fn jobs() -> Vec<Job> {
    let mut jobs = Vec::new();
    for kind in 0..TYPES.len() {
        for position in 0..3 {
            for edge in [2, 3] {
                for level in 0..LEVELS.len() {
                    for language in 0..GpuLanguage::ALL.len() {
                        for before in [false, true] {
                            jobs.push(Job {
                                kind,
                                position,
                                edge,
                                level,
                                language,
                                before,
                            });
                        }
                    }
                }
            }
        }
    }
    assert_eq!(
        jobs.len(),
        TYPES.len() * 3 * 2 * LEVELS.len() * GpuLanguage::ALL.len() * 2
    );
    jobs
}

fn run_part(part: usize) {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let original = jobs();
    let indices: Vec<_> = (part..original.len()).step_by(PARTS).collect();
    let jobs: Vec<_> = indices.iter().map(|&i| original[i]).collect();
    assert_eq!(jobs.len(), original.len() / PARTS);
    let mut completed = 0;
    let mut live = BTreeSet::new();
    for batch in jobs.chunks(24) {
        let requests: Vec<_> = batch.iter().map(|&job| args(job)).collect();
        let calls: Vec<_> = requests
            .iter()
            .map(|a| BatchCall {
                cmd: "legacy_gpu_updates",
                args: a.clone(),
                blobs: vec![],
            })
            .collect();
        let replies = Oracle::get().batch(&calls, false);
        assert_eq!(replies.len(), batch.len());
        for (&job, response) in batch.iter().zip(&replies) {
            let expected = wheel(
                response
                    .as_ref()
                    .unwrap_or_else(|error| panic!("native update request {job:?}: {error}")),
            );
            let actual = port(job);
            assert_eq!(actual, expected, "legacy live request {job:?}");
            if expected.states.len() == 5 {
                live.insert(job.kind);
            }
            completed += 1;
        }
    }
    assert_eq!(completed, jobs.len());
    // Part1 includes the ordinary-cache-first/NONE witness in the first language
    // for every property type; the complete prefix and each live setter compare.
    if part == 1 {
        assert_eq!(
            live.len(),
            TYPES.len(),
            "every property needs a live witness"
        );
    }
    eprintln!(
        "P88 legacy live part {part}/{PARTS}: {completed} exact requests, {} property types with all four setters observed; original indices {indices:?}",
        live.len()
    );
}

#[test]
fn legacy_live_parts_cover_original_cases_once() {
    let original = jobs();
    let mut union = Vec::new();
    for part in 0..PARTS {
        union.extend((part..original.len()).step_by(PARTS));
    }
    union.sort_unstable();
    assert_eq!(union, (0..original.len()).collect::<Vec<_>>());
    assert_eq!(original.len(), 5880);
    assert_eq!(original.len() % PARTS, 0);
}

macro_rules! part {
    ($name:ident, $number:expr) => {
        #[test]
        fn $name() {
            run_part($number)
        }
    };
}
part!(legacy_live_part00, 0);
part!(legacy_live_part01, 1);
part!(legacy_live_part02, 2);
part!(legacy_live_part03, 3);
part!(legacy_live_part04, 4);
part!(legacy_live_part05, 5);
part!(legacy_live_part06, 6);
part!(legacy_live_part07, 7);
part!(legacy_live_part08, 8);
part!(legacy_live_part09, 9);
part!(legacy_live_part10, 10);
part!(legacy_live_part11, 11);
part!(legacy_live_part12, 12);
part!(legacy_live_part13, 13);
part!(legacy_live_part14, 14);
part!(legacy_live_part15, 15);
part!(legacy_live_part16, 16);
part!(legacy_live_part17, 17);
part!(legacy_live_part18, 18);
part!(legacy_live_part19, 19);
part!(legacy_live_part20, 20);
part!(legacy_live_part21, 21);
part!(legacy_live_part22, 22);
part!(legacy_live_part23, 23);
part!(legacy_live_part24, 24);
part!(legacy_live_part25, 25);
part!(legacy_live_part26, 26);
part!(legacy_live_part27, 27);
