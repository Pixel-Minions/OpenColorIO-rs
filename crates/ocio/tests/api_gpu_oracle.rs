// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The transforms' shaders through the port's API against the wheel's, byte for byte, in the
//! 10 shading languages, through the oracle's `gpu_shader`.
//!
//! Each case of each class (`common::api_cases`) goes in both directions through
//! `Config::CreateRaw()->getProcessor(transform)` and its GPU processor:
//! `getDefaultGPUProcessor()`, or `getOptimizedGPUProcessor` with `OPTIMIZATION_NONE`,
//! `LOSSLESS`, `VERY_GOOD`, `GOOD`, `DRAFT` or `DEFAULT`. The GPU processor's cache ID and
//! queries must be the wheel's, and so must the shader the extraction writes into a
//! `GpuShaderDesc` of each language: its text, cache ID and names, its textures (ACES 2.0's
//! tables, and the 1D LUTs' since WP 2.1h) and 3D textures (the 3D LUTs) with their values'
//! bits, and the number of its uniforms and dynamic properties. Or both must raise the same
//! message at the same stage. OCIO's log must be the wheel's too (the OSL writer warns of each
//! dynamic property it replaces by a local variable).
//!
//! The ExposureContrast and grading transforms' uniforms are compared row by row (name, type,
//! buffer offset, value) by [`check_uniforms`], through `dynamic_apply`, for their dynamic
//! cases (`common::api_grading_cases`) at the same levels and in the same languages: after
//! the extraction, and after each set of a dynamic property taken from the description (one
//! value the setter refuses among them, which the wheel must refuse with the same message).

mod common;

use common::api::{Calls, LEVELS, port_transform};
use common::api_cases::{self, Cases};
use common::api_grading_cases::{self, DynCases, DynSet};
use common::grading_specs::{self, Object};
use ocio::{Config, DynamicPropertyType, Exception, TransformDirection};
use ocio_gpu::gpu_shader::{TextureDimensions, TextureType, UniformData};
use ocio_gpu::{GpuLanguage, GpuShaderDesc};
use ocio_ops::dynamic_property::DynamicPropertyRcPtr;
use ocio_ops::logging::{reset_to_default_logging_function, set_logging_function};
use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;
use ocio_testkit::Oracle;
use ocio_testkit::battery::Direction;
use ocio_testkit::gpu::{
    self as oracle_gpu, GpuShaderReply, GpuShaderRequest, ShaderSettings, Texture,
};
use ocio_testkit::oracle::BatchCall;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex, PoisonError};

/// The tests of this file swap the logging function: one at a time.
static SERIAL: Mutex<()> = Mutex::new(());

/// What `f` logs on the port's side, each message as the oracle reports the wheel's.
fn capture<T>(f: impl FnOnce() -> T) -> (T, Vec<String>) {
    let messages = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&messages);
    set_logging_function(Some(Arc::new(move |m: &[u8]| {
        sink.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(String::from_utf8(m.to_vec()).expect("UTF-8"));
    })))
    .expect("a logging function");
    let out = f();
    reset_to_default_logging_function();
    let log = messages
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    (out, log)
}

/// A version 1 config with one color space, for the classes that build other ops there.
const V1_CONFIG: &str = "ocio_profile_version: 1
roles:
  default: raw
displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
colorspaces:
  - !<ColorSpace>
    name: raw
";

/// A class of the sweep: its cases and the config.
struct Class {
    name: &'static str,
    cases: Cases,
    /// A version 1 config instead of the raw config.
    v1: bool,
    /// A class with dynamic properties, whose uniforms are counted here and compared, with
    /// their values after sets, by [`check_uniforms`].
    dynamic: bool,
}

impl Class {
    fn new(name: &'static str, cases: Cases) -> Class {
        Class {
            name,
            cases,
            v1: false,
            dynamic: false,
        }
    }

    fn dynamic(name: &'static str, cases: Cases) -> Class {
        Class {
            dynamic: true,
            ..Class::new(name, cases)
        }
    }
}

/// A texture as the comparison holds it: its settings, and its values' bits.
fn texture_json(t: &Texture) -> Value {
    json!({
        "name": t.name,
        "sampler_name": t.sampler_name,
        "width": t.width,
        "height": t.height,
        "channel": t.channel,
        "dimensions": t.dimensions,
        "interpolation": t.interpolation,
        "binding_index": t.binding_index,
        "values": t.values.iter().map(|v| v.to_bits()).collect::<Vec<u32>>(),
    })
}

/// The port's texture `index`, as the oracle reports the wheel's.
fn port_texture(desc: &GpuShaderDesc, index: u32) -> Texture {
    let utf8 = |bytes: &[u8]| String::from_utf8(bytes.to_vec()).expect("UTF-8");
    let t = desc.texture(index).expect("a texture");
    let interpolation = match t.interpolation() {
        Interpolation::Unknown => "INTERP_UNKNOWN",
        Interpolation::Nearest => "INTERP_NEAREST",
        Interpolation::Linear => "INTERP_LINEAR",
        Interpolation::Tetrahedral => "INTERP_TETRAHEDRAL",
        Interpolation::Cubic => "INTERP_CUBIC",
        Interpolation::Default => "INTERP_DEFAULT",
        Interpolation::Best => "INTERP_BEST",
    };
    Texture {
        name: utf8(t.texture_name()),
        sampler_name: utf8(t.sampler_name()),
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
        interpolation: interpolation.into(),
        binding_index: u64::from(desc.texture_shader_binding_index(index).expect("a binding")),
        values: t.values().to_vec(),
    }
}

/// One extraction: a case in a direction, a level (`None`: the default GPU processor) and a
/// language.
struct Job {
    case: usize,
    dir: Direction,
    level: Option<usize>,
    language: usize,
    request: GpuShaderRequest,
}

/// The jobs of `class`: every case in both directions, at every level, in every language.
fn jobs(class: &Class) -> Vec<Job> {
    let levels: Vec<Option<usize>> = std::iter::once(None)
        .chain((0..LEVELS.len()).map(Some))
        .collect();
    let mut jobs = Vec::new();
    for (case, c) in class.cases.cases.iter().enumerate() {
        for dir in [Direction::Forward, Direction::Inverse] {
            for &level in &levels {
                for (language, oracle_language) in oracle_gpu::GpuLanguage::ALL.iter().enumerate() {
                    let mut processor = json!({"transform": c.params().spec(dir)});
                    if let Some(level) = level {
                        processor["optimization"] = json!(LEVELS[level].0);
                    }
                    if class.v1 {
                        processor["config"] = json!({"yaml": V1_CONFIG});
                    }
                    jobs.push(Job {
                        case,
                        dir,
                        level,
                        language,
                        request: GpuShaderRequest::new(
                            processor,
                            ShaderSettings::language(*oracle_language),
                        ),
                    });
                }
            }
        }
    }
    jobs
}

/// What the wheel or the port did: the GPU processor's cache IDs and queries, then the shader
/// or the extraction's error; or the error of an earlier stage.
#[derive(Debug, Clone, PartialEq)]
enum Outcome {
    Raised(String, String),
    Gpu {
        processor: Value,
        shader: Result<Value, (String, String)>,
    },
}

/// The wheel's outcome.
fn wheel(reply: &GpuShaderReply) -> Outcome {
    let result = &reply.result;
    let raised = reply.raised();
    if let Some(raised) = &raised
        && !matches!(raised.stage.as_str(), "extract" | "shader_desc")
    {
        return Outcome::Raised(raised.stage.clone(), raised.message.clone());
    }
    let processor = json!({
        "processor_cache_id": result["processor_cache_id"],
        "gpu_cache_id": result["gpu_cache_id"],
        "gpu_processor": result["gpu_processor"],
    });
    let shader = match raised {
        Some(raised) => Err((raised.stage, raised.message)),
        None => {
            let shader = reply.shader();
            Ok(json!({
                "text": shader.text,
                "cache_id": shader.cache_id,
                "function_name": shader.getters["function_name"],
                "pixel_name": shader.getters["pixel_name"],
                "resource_prefix": shader.getters["resource_prefix"],
                "uniforms": shader.uniforms.len(),
                "textures": shader.textures.iter().map(texture_json).collect::<Vec<_>>(),
                "textures_3d": shader.textures_3d.iter().map(|t| json!({
                    "name": t.name, "sampler_name": t.sampler_name, "edge_len": t.edge_len,
                    "interpolation": t.interpolation, "binding_index": t.binding_index,
                    "values": t.values.iter().map(|v| v.to_bits()).collect::<Vec<u32>>(),
                })).collect::<Vec<Value>>(),
                "dynamic_properties": shader.dynamic_properties.len(),
            }))
        }
    };
    Outcome::Gpu { processor, shader }
}

/// The port's 3D texture `index`, as the wheel's outcome lists one (its values by their bits).
fn port_texture_3d(desc: &GpuShaderDesc, index: u32) -> Value {
    let t = desc.texture_3d(index).expect("a 3D texture");
    let text = |bytes: &[u8]| String::from_utf8(bytes.to_vec()).expect("UTF-8");
    json!({
        "name": text(t.texture_name()),
        "sampler_name": text(t.sampler_name()),
        "edge_len": t.edge_len(),
        "interpolation": match t.interpolation() {
            Interpolation::Nearest => "INTERP_NEAREST",
            Interpolation::Linear => "INTERP_LINEAR",
            other => panic!("interpolation {other:?}"),
        },
        "binding_index": desc.texture_3d_shader_binding_index(index).expect("its binding"),
        "values": t.values().iter().map(|v| v.to_bits()).collect::<Vec<u32>>(),
    })
}

/// The port's outcome for `job`.
fn port(class: &Class, job: &Job, calls: &Calls) -> Outcome {
    let raised = |stage: &str, e: Exception| Outcome::Raised(stage.into(), e.message().into());
    let text = |bytes: &[u8]| String::from_utf8(bytes.to_vec()).expect("UTF-8");
    let transform = match port_transform(&calls.spec(job.dir)) {
        Ok(t) => t,
        Err(e) => return raised("transform", e),
    };
    let mut config = (*Config::create_raw().unwrap()).clone();
    if class.v1 {
        config.set_major_version(1).expect("version 1");
    }
    let processor = match config.processor_in_direction(&transform, TransformDirection::Forward) {
        Ok(p) => p,
        Err(e) => return raised("processor", e),
    };
    let gpu = match job.level {
        None => processor.default_gpu_processor(),
        Some(level) => processor.optimized_gpu_processor(LEVELS[level].1),
    };
    let gpu = match gpu {
        Ok(gpu) => gpu,
        Err(e) => return raised("gpu_processor", e),
    };
    let cache_id = match processor.cache_id() {
        Ok(id) => id,
        Err(e) => return raised("gpu_processor", e),
    };
    let processor = json!({
        "processor_cache_id": cache_id,
        "gpu_cache_id": text(gpu.get_cache_id()),
        "gpu_processor": {"isNoOp": gpu.is_no_op(),
            "hasChannelCrosstalk": gpu.has_channel_crosstalk()},
    });
    let mut desc = GpuShaderDesc::new(GpuLanguage::ALL[job.language]);
    let shader = gpu
        .extract_gpu_shader_info(&mut desc)
        .map(|()| {
            json!({
                "text": text(desc.shader_text()),
                "cache_id": text(&desc.cache_id()),
                "function_name": text(desc.function_name()),
                "pixel_name": text(desc.pixel_name()),
                "resource_prefix": text(desc.resource_prefix()),
                "uniforms": desc.num_uniforms(),
                "textures": (0..desc.num_textures())
                    .map(|i| texture_json(&port_texture(&desc, i)))
                    .collect::<Vec<_>>(),
                "textures_3d": (0..desc.num_textures_3d()).map(|i| port_texture_3d(&desc, i)).collect::<Vec<Value>>(),
                "dynamic_properties": desc.num_dynamic_properties(),
            })
        })
        .map_err(|e| ("extract".to_string(), e.message().to_string()));
    Outcome::Gpu { processor, shader }
}

/// Runs every job of `class` against the wheel; panics with a report if any differs.
fn check(class: &Class) {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let jobs = jobs(class);
    let mut failures = Vec::new();
    let (mut compared, mut refusals) = (0, 0);
    for batch in jobs.chunks(500) {
        let calls: Vec<_> = batch.iter().map(|job| job.request.call()).collect();
        for (job, response) in batch.iter().zip(Oracle::get().batch(&calls, true)) {
            let case = &class.cases.cases[job.case];
            let what = format!(
                "{} \"{}\" {:?} {} {:?}",
                class.name,
                case.label(),
                job.dir,
                job.level.map_or("getDefaultGPUProcessor", |l| LEVELS[l].0),
                GpuLanguage::ALL[job.language]
            );
            let reply =
                GpuShaderReply::from_response(response.unwrap_or_else(|e| panic!("{what}: {e}")));
            // The OSL writer warns of each dynamic property it replaces by a local variable.
            let wheel = wheel(&reply);
            let (port, port_log) = capture(|| port(class, job, case.params()));
            if reply.log() != port_log {
                failures.push(format!(
                    "{what}: logs
  wheel {:?}
  port  {port_log:?}",
                    reply.log()
                ));
            }
            if wheel != port {
                failures.push(format!("{what}\n  wheel {wheel:?}\n  port  {port:?}"));
                continue;
            }
            match wheel {
                Outcome::Gpu { shader: Ok(s), .. } => {
                    assert!(
                        class.dynamic || s["uniforms"] == 0,
                        "{what}: compare the uniforms too: {s}"
                    );
                    compared += 1;
                }
                _ => refusals += 1,
            }
        }
    }
    println!(
        "{}: {} extractions: {compared} compared, {refusals} refusals compared",
        class.name,
        jobs.len(),
    );
    assert!(
        failures.is_empty(),
        "{} of {} extractions differ:\n{}",
        failures.len(),
        jobs.len(),
        failures[..failures.len().min(10)].join("\n")
    );
    assert!(compared > 0, "{}: nothing compared", class.name);
}

#[test]
fn matrix_transform_shaders_match_the_wheel() {
    check(&Class::new("MatrixTransform", api_cases::matrix()));
}

#[test]
fn range_transform_shaders_match_the_wheel() {
    check(&Class::new("RangeTransform", api_cases::range()));
}

#[test]
fn cdl_transform_shaders_match_the_wheel() {
    check(&Class::new("CDLTransform", api_cases::cdl()));
}

#[test]
fn log_transform_shaders_match_the_wheel() {
    check(&Class::new("LogTransform", api_cases::log()));
}

#[test]
fn log_affine_transform_shaders_match_the_wheel() {
    check(&Class::new("LogAffineTransform", api_cases::log_affine()));
}

#[test]
fn log_camera_transform_shaders_match_the_wheel() {
    check(&Class::new("LogCameraTransform", api_cases::log_camera()));
}

#[test]
fn exponent_transform_shaders_match_the_wheel() {
    check(&Class::new("ExponentTransform", api_cases::exponent()));
    check(&Class {
        v1: true,
        ..Class::new("ExponentTransform (version 1)", api_cases::exponent_v1())
    });
}

#[test]
fn exponent_with_linear_transform_shaders_match_the_wheel() {
    check(&Class::new(
        "ExponentWithLinearTransform",
        api_cases::exponent_with_linear(),
    ));
}

#[test]
fn allocation_transform_shaders_match_the_wheel() {
    check(&Class::new("AllocationTransform", api_cases::allocation()));
}

#[test]
fn group_transform_shaders_match_the_wheel() {
    check(&Class::new("GroupTransform", api_cases::group()));
}

#[test]
fn lut1d_transform_shaders_match_the_wheel() {
    check(&Class::new("Lut1DTransform", api_cases::lut1d()));
}

/// The cube of one entry is left out: its inverse never returns in the wheel (U-65).
#[test]
fn lut3d_transform_shaders_match_the_wheel() {
    check(&Class::new("Lut3DTransform", api_cases::lut3d()));
}

/// The built-in transforms whose ops are ported (`api_cases::BUILTINS_WITH_OPS`).
#[test]
fn builtin_transform_shaders_match_the_wheel() {
    check(&Class::new(
        "BuiltinTransform",
        api_cases::builtin(api_cases::BUILTINS_WITH_OPS),
    ));
}

#[test]
fn fixed_function_transform_shaders_match_the_wheel() {
    check(&Class::new(
        "FixedFunctionTransform",
        api_cases::fixed_function(),
    ));
}

/// A dynamic property type's name in PyOpenColorIO.
fn property_name(ty: DynamicPropertyType) -> &'static str {
    match ty {
        DynamicPropertyType::Exposure => "DYNAMIC_PROPERTY_EXPOSURE",
        DynamicPropertyType::Contrast => "DYNAMIC_PROPERTY_CONTRAST",
        DynamicPropertyType::Gamma => "DYNAMIC_PROPERTY_GAMMA",
        DynamicPropertyType::GradingPrimary => "DYNAMIC_PROPERTY_GRADING_PRIMARY",
        DynamicPropertyType::GradingRgbCurve => "DYNAMIC_PROPERTY_GRADING_RGBCURVE",
        DynamicPropertyType::GradingTone => "DYNAMIC_PROPERTY_GRADING_TONE",
        DynamicPropertyType::GradingHueCurve => "DYNAMIC_PROPERTY_GRADING_HUECURVE",
    }
}

/// The property types `sets` names, each once, in order.
fn property_types(sets: &[DynSet]) -> Vec<DynamicPropertyType> {
    let mut types = Vec::new();
    for set in sets {
        if !types.contains(&set.ty) {
            types.push(set.ty);
        }
    }
    types
}

/// The port's uniforms, each `[name, type, buffer offset, value]` as `gpu_shader` writes the
/// wheel's: a double, a float or a float of a vector by its bits.
fn port_uniform_rows(desc: &GpuShaderDesc) -> Vec<Value> {
    let text = |b: &[u8]| String::from_utf8(b.to_vec()).expect("UTF-8");
    let bits = |v: &[f32]| v.iter().map(|f| f.to_bits()).collect::<Vec<u32>>();
    desc.uniforms()
        .iter()
        .map(|u| {
            let (kind, value) = match u.data() {
                UniformData::Double(get) => ("UNIFORM_DOUBLE", json!(get().to_bits())),
                UniformData::Bool(get) => ("UNIFORM_BOOL", json!(get())),
                UniformData::Float3(get) => ("UNIFORM_FLOAT3", json!(bits(&get()))),
                UniformData::VectorFloat { size, values } => (
                    "UNIFORM_VECTOR_FLOAT",
                    json!(bits(&values()[..size() as usize])),
                ),
                UniformData::VectorInt { size, values } => (
                    "UNIFORM_VECTOR_INT",
                    json!(values()[..size() as usize].to_vec()),
                ),
            };
            json!([text(u.name()), kind, u.buffer_offset(), value])
        })
        .collect()
}

/// The wheel's uniforms of a `dynamic_apply` step, as [`port_uniform_rows`] writes the port's.
fn wheel_uniform_rows(step: &Value) -> Option<Vec<Value>> {
    let rows = step["result"].as_array()?;
    Some(
        rows.iter()
            .map(|u| json!([u["name"], u["type"], u["buffer_offset"], u["value"]]))
            .collect(),
    )
}

/// Sets the port's property to a set's value, built from its spec as the oracle builds it; or
/// where that raised, as `dynamic_apply` names it ("value" or "set"), and the error.
fn port_set(prop: &DynamicPropertyRcPtr, set: &DynSet) -> Result<(), (&'static str, Exception)> {
    let on_set = |e: Exception| ("set", e);
    if set.value.get("f64").is_some() {
        prop.as_double()
            .map_err(on_set)?
            .set_value(grading_specs::f64_of(&set.value));
        return Ok(());
    }
    let value = grading_specs::object(&set.value["object"]).map_err(|e| ("value", e))?;
    let set = |r: Result<(), Exception>| r.map_err(on_set);
    match value {
        Object::Primary(v, _) => set(prop.as_grading_primary().and_then(|p| p.set_value(&v))),
        Object::Tone(v) => set(prop.as_grading_tone().and_then(|p| p.set_value(&v))),
        Object::RgbCurve(v) => set(prop.as_grading_rgb_curve().and_then(|p| p.set_value(&v))),
        Object::HueCurve(v) => set(prop.as_grading_hue_curve().and_then(|p| p.set_value(&v))),
        other => panic!("not a dynamic property's value: {other:?}"),
    }
}

/// What [`check_uniforms`] compared.
#[derive(Default, Debug)]
struct UniformCounts {
    reads: usize,
    expected_reads: usize,
    refused_sets: usize,
    missing: usize,
    refusals: usize,
}

/// One extraction of [`check_uniforms`] against the wheel's reply: the uniforms read after
/// the extraction and after each set, every row (name, type, buffer offset, value).
fn compare_uniforms(
    class: &Class,
    job: &Job,
    sets: &[DynSet],
    result: &Value,
    counts: &mut UniformCounts,
    failures: &mut Vec<String>,
) {
    let case = &class.cases.cases[job.case];
    let what = format!(
        "{} \"{}\" {:?} {} {:?}",
        class.name,
        case.label(),
        job.dir,
        job.level.map_or("getDefaultGPUProcessor", |l| LEVELS[l].0),
        GpuLanguage::ALL[job.language]
    );
    let (port, port_log) = capture(|| {
        let transform =
            port_transform(&case.params().spec(job.dir)).map_err(|e| ("transform", e))?;
        let processor = Config::create_raw()
            .unwrap()
            .processor_in_direction(&transform, TransformDirection::Forward)
            .map_err(|e| ("processor", e))?;
        let gpu = match job.level {
            None => processor.default_gpu_processor(),
            Some(level) => processor.optimized_gpu_processor(LEVELS[level].1),
        }
        .map_err(|e| ("gpu_processor", e))?;
        let mut desc = GpuShaderDesc::new(GpuLanguage::ALL[job.language]);
        gpu.extract_gpu_shader_info(&mut desc)
            .map_err(|e| ("extract", e))?;
        Ok::<_, (&str, Exception)>(desc)
    });
    // The OSL writer warns of each dynamic property it replaces by a local variable.
    if result["log"] != json!(port_log) {
        failures.push(format!(
            "{what}: logs
  wheel {}
  port  {port_log:?}",
            result["log"]
        ));
    }
    let desc = match (result.get("exception"), port) {
        (None, Ok(desc)) => desc,
        (Some(e), Err((stage, p))) if result["stage"] == stage && e["message"] == p.message() => {
            counts.refusals += 1;
            return;
        }
        (_, port) => {
            failures.push(format!("{what}: wheel {result}, port {:?}", port.err()));
            return;
        }
    };
    counts.expected_reads += sets.len() + 1;
    let steps = result["steps"].as_array().expect("steps");
    let mut k = 0;
    let mut props = Vec::new();
    for ty in property_types(sets) {
        let step = &steps[k];
        k += 1;
        match (desc.dynamic_property_by_type(ty), step.get("exception")) {
            (Ok(p), None) => props.push((ty, Some(p))),
            (Err(p), Some(e)) if e["message"] == p.message() => {
                counts.missing += 1;
                props.push((ty, None));
            }
            (port, _) => failures.push(format!(
                "{what}: get {ty:?}: wheel {step}, port {:?}",
                port.err()
            )),
        }
    }
    let mut read = |k: usize, failures: &mut Vec<String>| {
        let port = port_uniform_rows(&desc);
        match wheel_uniform_rows(&steps[k]) {
            Some(wheel) if wheel == port => counts.reads += 1,
            wheel => failures.push(format!(
                "{what}: step {k}\n  wheel {wheel:?}\n  port  {port:?}"
            )),
        }
    };
    read(k, failures);
    k += 1;
    for set in sets {
        let step = &steps[k];
        match props.iter().find(|(t, _)| *t == set.ty) {
            Some((_, Some(prop))) => match (step.get("exception"), capture(|| port_set(prop, set)))
            {
                (_, (_, log)) if step["log"] != json!(log) => {
                    failures.push(format!("{what}: set logs: wheel {step}, port {log:?}"));
                }
                (wheel, (port, _)) => match (wheel, port) {
                    (None, Ok(())) => {}
                    (Some(e), Err((stage, p)))
                        if e["message"] == p.message() && step["stage"] == stage =>
                    {
                        counts.refused_sets += 1;
                    }
                    (_, port) => failures.push(format!("{what}: set: wheel {step}, port {port:?}")),
                },
            },
            Some((_, None)) => {
                if step["result"]["unbound"] != true {
                    failures.push(format!("{what}: set of a property not taken: wheel {step}"));
                }
            }
            None => return,
        }
        read(k + 1, failures);
        k += 2;
    }
}

/// The uniforms of the dynamic cases of `cases`, through `dynamic_apply`, in both directions, at
/// every level and in every language: the properties taken from the GPU description after the
/// extraction, the uniforms read, then each set followed by a read. Every uniform's name,
/// type, buffer offset and value must be the wheel's after every step, and a set the port
/// refuses must be refused by the wheel with the same message.
fn check_uniforms(name: &'static str, cases: DynCases) {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let (kept, sets): (Vec<_>, Vec<_>) = cases
        .cases
        .cases
        .into_iter()
        .zip(cases.sets)
        .filter(|(_, sets)| !sets.is_empty())
        .unzip();
    assert!(!kept.is_empty(), "{name}: no dynamic case");
    let class = Class::new(
        name,
        Cases {
            cases: kept,
            bases: Vec::new(),
        },
    );
    let jobs = jobs(&class);
    let mut counts = UniformCounts::default();
    let mut failures = Vec::new();
    for batch in jobs.chunks(300) {
        let calls: Vec<BatchCall<'_>> = batch
            .iter()
            .map(|job| {
                let s = &sets[job.case];
                let mut steps: Vec<Value> = property_types(s)
                    .into_iter()
                    .map(|ty| json!({"get": property_name(ty), "from": "gpu", "as": property_name(ty)}))
                    .collect();
                steps.push(json!({"uniforms": true}));
                for set in s {
                    steps.push(json!({"set": property_name(set.ty), "value": set.value}));
                    steps.push(json!({"uniforms": true}));
                }
                let mut gpu = json!({"shader": {"language": oracle_gpu::GpuLanguage::ALL[job.language].oracle_name()}});
                if let Some(level) = job.level {
                    gpu["optimization"] = json!(LEVELS[level].0);
                }
                BatchCall {
                    cmd: "dynamic_apply",
                    args: json!({
                        "transform": job.request.processor["transform"],
                        "gpu": gpu,
                        "steps": steps,
                    }),
                    blobs: Vec::new(),
                }
            })
            .collect();
        for (job, response) in batch.iter().zip(Oracle::get().batch(&calls, true)) {
            let response = response.unwrap_or_else(|e| panic!("{name}: {e}"));
            compare_uniforms(
                &class,
                job,
                &sets[job.case],
                &response.result,
                &mut counts,
                &mut failures,
            );
        }
    }
    println!("{name}: {} extractions: {counts:?}", jobs.len());
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures[..failures.len().min(10)].join("\n")
    );
    // Every extraction both made reads the uniforms once, then once after each set.
    assert!(counts.reads > 0, "{name}: {counts:?}");
    assert_eq!(
        counts.reads, counts.expected_reads,
        "{name}: every read compared"
    );
}

#[test]
fn exposure_contrast_transform_shaders_match_the_wheel() {
    check(&Class::dynamic(
        "ExposureContrastTransform",
        api_grading_cases::exposure_contrast().cases,
    ));
}

#[test]
fn exposure_contrast_uniforms_follow_their_sets_as_in_the_wheel() {
    check_uniforms(
        "ExposureContrastTransform",
        api_grading_cases::exposure_contrast(),
    );
}

#[test]
fn grading_primary_transform_shaders_match_the_wheel() {
    check(&Class::dynamic(
        "GradingPrimaryTransform",
        api_grading_cases::primary().cases,
    ));
}

#[test]
fn grading_primary_uniforms_follow_their_sets_as_in_the_wheel() {
    check_uniforms("GradingPrimaryTransform", api_grading_cases::primary());
}

#[test]
fn grading_tone_transform_shaders_match_the_wheel() {
    check(&Class::dynamic(
        "GradingToneTransform",
        api_grading_cases::tone().cases,
    ));
}

#[test]
fn grading_tone_uniforms_follow_their_sets_as_in_the_wheel() {
    check_uniforms("GradingToneTransform", api_grading_cases::tone());
}

#[test]
fn grading_hue_curve_transform_shaders_match_the_wheel() {
    check(&Class::dynamic(
        "GradingHueCurveTransform",
        api_grading_cases::hue_curve().cases,
    ));
}

#[test]
fn grading_hue_curve_uniforms_follow_their_sets_as_in_the_wheel() {
    check_uniforms("GradingHueCurveTransform", api_grading_cases::hue_curve());
}

#[test]
fn grading_rgb_curve_transform_shaders_match_the_wheel() {
    check(&Class::dynamic(
        "GradingRGBCurveTransform",
        api_cases::rgb_curve_cases(),
    ));
}

#[test]
fn grading_rgb_curve_uniforms_follow_their_sets_as_in_the_wheel() {
    check_uniforms(
        "GradingRGBCurveTransform",
        api_grading_cases::rgb_curve_dynamic(),
    );
}
