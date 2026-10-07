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
//! `GpuShaderDesc` of each language: its text, cache ID and names, its textures (every value's
//! bits; the 1D LUTs' since WP 2.1h), and its uniforms and dynamic properties, of which these
//! classes have none. Or both must raise the same message at the same stage.

mod common;

use common::api::{Calls, LEVELS, port_transform};
use common::api_cases::{self, Cases};
use ocio::{Config, Exception, TransformDirection};
use ocio_gpu::gpu_shader::{TextureDimensions, TextureType};
use ocio_gpu::{GpuLanguage, GpuShaderDesc};
use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;
use ocio_testkit::Oracle;
use ocio_testkit::battery::Direction;
use ocio_testkit::gpu::{
    self as oracle_gpu, GpuShaderReply, GpuShaderRequest, ShaderSettings, Texture,
};
use serde_json::{Value, json};

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
}

impl Class {
    fn new(name: &'static str, cases: Cases) -> Class {
        Class {
            name,
            cases,
            v1: false,
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
                "textures_3d": shader.textures_3d.len(),
                "dynamic_properties": shader.dynamic_properties.len(),
            }))
        }
    };
    Outcome::Gpu { processor, shader }
}

/// The port's outcome for `job`.
fn port(class: &Class, job: &Job, calls: &Calls) -> Outcome {
    let raised = |stage: &str, e: Exception| Outcome::Raised(stage.into(), e.message().into());
    let text = |bytes: &[u8]| String::from_utf8(bytes.to_vec()).expect("UTF-8");
    let transform = match port_transform(&calls.spec(job.dir)) {
        Ok(t) => t,
        Err(e) => return raised("transform", e),
    };
    let mut config = (*Config::create_raw()).clone();
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
                "textures_3d": desc.num_textures_3d(),
                "dynamic_properties": desc.num_dynamic_properties(),
            })
        })
        .map_err(|e| ("extract".to_string(), e.message().to_string()));
    Outcome::Gpu { processor, shader }
}

/// Runs every job of `class` against the wheel; panics with a report if any differs.
fn check(class: &Class) {
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
            if !reply.log().is_empty() {
                failures.push(format!("{what}: OCIO logged {:?}", reply.log()));
            }
            let wheel = wheel(&reply);
            let port = port(class, job, case.params());
            if wheel != port {
                failures.push(format!("{what}\n  wheel {wheel:?}\n  port  {port:?}"));
                continue;
            }
            match wheel {
                Outcome::Gpu { shader: Ok(s), .. } => {
                    assert!(
                        s["uniforms"] == 0 && s["textures_3d"] == 0,
                        "{what}: compare the uniforms and 3D textures too: {s}"
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
