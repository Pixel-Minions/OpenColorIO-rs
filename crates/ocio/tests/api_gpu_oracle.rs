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
//! `GpuShaderDesc` of each language: its text, cache ID and names, and its uniforms, textures
//! and dynamic properties, of which the analytic ops have none. Or both must raise the same
//! message at the same stage.
//!
//! The `Lut1DTransform`'s GPU writer (`Lut1DOpGPU`) is Phase 2's, as are its inverse LUT and
//! hue adjustment: where the wheel builds a GPU processor (then writes the shader, or refuses
//! the OSL translation) and the port refuses with its "not ported yet" message, the test
//! counts a deferral, and only for that class. The GPU processor's cache ID and queries are
//! still compared when the port gets that far.

mod common;

use std::collections::BTreeMap;

use common::api::{Calls, LEVELS, port_transform};
use common::api_cases::{self, Cases};
use ocio::{Config, Exception, TransformDirection};
use ocio_gpu::{GpuLanguage, GpuShaderDesc};
use ocio_testkit::Oracle;
use ocio_testkit::battery::Direction;
use ocio_testkit::gpu::{self as oracle_gpu, GpuShaderReply, GpuShaderRequest, ShaderSettings};
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

/// A class of the sweep: its cases, the config, and whether it has Phase 2 deferrals.
struct Class {
    name: &'static str,
    cases: Cases,
    /// A version 1 config instead of the raw config.
    v1: bool,
    /// The class's "not ported yet" refusals count as deferrals.
    deferred: bool,
}

impl Class {
    fn new(name: &'static str, cases: Cases) -> Class {
        Class {
            name,
            cases,
            v1: false,
            deferred: false,
        }
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
                "textures": shader.textures.len(),
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
                "textures": desc.num_textures(),
                "textures_3d": desc.num_textures_3d(),
                "dynamic_properties": desc.num_dynamic_properties(),
            })
        })
        .map_err(|e| ("extract".to_string(), e.message().to_string()));
    Outcome::Gpu { processor, shader }
}

/// The `Lut1DTransform`'s Phase 2 deferral in `dir`, the stage and message of the port's
/// refusal: an inverse LUT is set up when the processor finalizes it (`Lut1DOpData::finalize`,
/// WP 2.1); a forward one reaches the GPU processor, whose extraction needs the Lut1D op's GPU
/// writer (`GetLut1DGPUShaderProgram`, src/OpenColorIO/ops/lut1d/Lut1DOpGPU.cpp @ v2.5.2).
fn lut1d_deferral(dir: Direction) -> (&'static str, &'static str) {
    match dir {
        Direction::Inverse => (
            "processor",
            "Lut1D: the inverse 1D LUT is not ported yet (WP 2.1).",
        ),
        Direction::Forward => ("extract", "The GPU writer of <Lut1DOp> is not ported yet."),
    }
}

/// Whether the port's outcome is the `Lut1DTransform`'s deferral in `dir`, where the wheel
/// built a GPU processor: it then wrote a shader, or refused in the extraction with the
/// writer's own message (upstream's Lut1D writer has no OSL translation).
fn deferral(wheel: &Outcome, port: &Outcome, dir: Direction) -> bool {
    let Outcome::Gpu { .. } = wheel else {
        return false;
    };
    let (stage, message) = lut1d_deferral(dir);
    match port {
        Outcome::Raised(s, m)
        | Outcome::Gpu {
            shader: Err((s, m)),
            ..
        } => s == stage && m == message,
        Outcome::Gpu { shader: Ok(_), .. } => false,
    }
}

/// Runs every job of `class` against the wheel; panics with a report if any differs.
fn check(class: &Class) {
    let jobs = jobs(class);
    let mut failures = Vec::new();
    let mut deferred: BTreeMap<String, usize> = BTreeMap::new();
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
            if class.deferred {
                // Every extraction of the class is a deferral, at its stage, with its message.
                if !deferral(&wheel, &port, job.dir) {
                    failures.push(format!(
                        "{what}: the deferral {:?} was expected\n  wheel {wheel:?}\n  port  \
                         {port:?}",
                        lut1d_deferral(job.dir)
                    ));
                    continue;
                }
                // What the port computed before it refused must still be the wheel's.
                if let (Outcome::Gpu { processor: w, .. }, Outcome::Gpu { processor: p, .. }) =
                    (&wheel, &port)
                    && w != p
                {
                    failures.push(format!("{what}\n  wheel {w}\n  port  {p}"));
                }
                *deferred
                    .entry(lut1d_deferral(job.dir).1.to_string())
                    .or_default() += 1;
                continue;
            }
            if wheel != port {
                failures.push(format!("{what}\n  wheel {wheel:?}\n  port  {port:?}"));
                continue;
            }
            match wheel {
                Outcome::Gpu { shader: Ok(s), .. } => {
                    assert!(
                        s["uniforms"] == 0 && s["textures"] == 0 && s["textures_3d"] == 0,
                        "{what}: compare the uniforms and textures too: {s}"
                    );
                    compared += 1;
                }
                _ => refusals += 1,
            }
        }
    }
    println!(
        "{}: {} extractions: {compared} compared, {refusals} refusals compared, {} \
         deferred to Phase 2{}",
        class.name,
        jobs.len(),
        deferred.values().sum::<usize>(),
        deferred
            .iter()
            .map(|(m, n)| format!("\n  {n}: {m}"))
            .collect::<String>()
    );
    assert!(
        failures.is_empty(),
        "{} of {} extractions differ:\n{}",
        failures.len(),
        jobs.len(),
        failures[..failures.len().min(10)].join("\n")
    );
    assert!(
        compared > 0 || class.deferred,
        "{}: nothing compared",
        class.name
    );
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
fn lut1d_transform_shaders_are_deferred_where_the_wheel_writes_them() {
    check(&Class {
        deferred: true,
        ..Class::new("Lut1DTransform", api_cases::lut1d())
    });
}
