// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! WP7.1c: small legacy lattices, allocation/file boundaries and GPU-cache request order
//! against O7.1's native client linked to the official wheel. Shader text/cache IDs, logs
//! and every texture bit remain exact. No FixedFunctionTransform or execution exception
//! is involved. Larger edges and dynamic properties remain separate chunks.

use ocio::{
    Allocation, AllocationTransform, Config, FileTransform, GroupTransform, Lut1DTransform,
    MatrixTransform, OptimizationFlags, Transform,
};
use ocio_gpu::gpu_shader::{TextureDimensions, TextureType};
use ocio_gpu::{GpuLanguage, GpuShaderDesc};
use ocio_ops::logging::{reset_to_default_logging_function, set_logging_function};
use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;
use ocio_testkit::Oracle;
use ocio_testkit::gpu::{
    self as oracle_gpu, GpuShaderReply, GpuShaderRequest, ShaderSettings, Texture,
};
use ocio_testkit::oracle::BatchCall;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex, PoisonError};

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

const LEVELS: &[(&str, OptimizationFlags)] = &[
    ("OPTIMIZATION_NONE", OptimizationFlags::NONE),
    ("OPTIMIZATION_LOSSLESS", OptimizationFlags::LOSSLESS),
    ("OPTIMIZATION_VERY_GOOD", OptimizationFlags::VERY_GOOD),
    ("OPTIMIZATION_GOOD", OptimizationFlags::GOOD),
    ("OPTIMIZATION_DRAFT", OptimizationFlags::DRAFT),
    ("OPTIMIZATION_DEFAULT", OptimizationFlags::DEFAULT),
    ("OPTIMIZATION_ALL", OptimizationFlags::ALL),
];

struct Chain {
    label: &'static str,
    transform: Transform,
    spec: Value,
}

/// Inputs from CreateGenericScaleOp/CreateGenericLutOp, NoOps_tests.cpp:26-52 @v2.5.2.
fn matrix() -> (Transform, Value) {
    let values = [
        1.04, 0.0, 0.0, 0.0, 0.0, 1.05, 0.0, 0.0, 0.0, 0.0, 1.06, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    let mut transform = MatrixTransform::new();
    transform.set_matrix(&values);
    (
        transform.into(),
        json!({"class": "MatrixTransform", "calls": [["setMatrix", values]]}),
    )
}

/// Input table of `CreateGenericLutOp` (tests/cpu/ops/noop/NoOps_tests.cpp:32-52 @v2.5.2).
fn lut() -> (Transform, Value) {
    let mut transform = Lut1DTransform::new();
    transform.set_length(256).unwrap();
    let mut calls = vec![json!(["setLength", 256])];
    for index in 0..256 {
        let x = index as f32 / 255.0;
        let value = x * x;
        transform.set_value(index, value, value, value).unwrap();
        calls.push(json!(["setValue", index, value, value, value]));
    }
    (
        transform.into(),
        json!({"class": "Lut1DTransform", "calls": calls}),
    )
}

fn group_chain(label: &'static str, steps: Vec<(Transform, Value)>) -> Chain {
    let mut group = GroupTransform::new();
    let mut children = Vec::new();
    for (transform, spec) in steps {
        group.append_transform(transform);
        children.push(spec);
    }
    Chain {
        label,
        transform: group.into(),
        spec: json!({"class": "GroupTransform", "children": children}),
    }
}

fn chains() -> Vec<Chain> {
    [
        ("analytic", vec![false]),
        ("lut", vec![true]),
        ("analytic-lut-analytic", vec![false, true, false]),
        ("two-separated-luts", vec![true, false, true]),
    ]
    .into_iter()
    .map(|(label, steps)| {
        group_chain(
            label,
            steps
                .into_iter()
                .map(|is_lut| if is_lut { lut() } else { matrix() })
                .collect(),
        )
    })
    .collect()
}

/// Allocation inputs of NoOps_tests.cpp:17-23 and its partition chains at 160-234 @v2.5.2.
fn allocation(kind: Allocation, vars: Option<&[f32]>) -> (Transform, Value) {
    let mut transform = AllocationTransform::new();
    transform.set_allocation(kind);
    let name = match kind {
        Allocation::Uniform => "ALLOCATION_UNIFORM",
        Allocation::Lg2 => "ALLOCATION_LG2",
        other => panic!("allocation input {other:?}"),
    };
    let mut calls = vec![json!(["setAllocation", {"enum": name}])];
    if let Some(vars) = vars {
        transform.set_vars(vars);
        calls.push(json!(["setVars", vars]));
    }
    (
        transform.into(),
        json!({"class": "AllocationTransform", "calls": calls}),
    )
}

/// File-loaded lattice boundary of NoOps.cpp:266-269, using upstream's SPI1D input @v2.5.2.
fn file() -> (Transform, Value) {
    let path = ocio_testkit::paths::upstream_dir().join("tests/data/files/lut1d_1.spi1d");
    let path = path.to_str().expect("UTF-8").replace('\\', "/");
    let path = path.trim_start_matches("//?/");
    let mut transform = FileTransform::new();
    transform.set_src(path);
    (
        transform.into(),
        json!({"class": "FileTransform", "calls": [["setSrc", path]]}),
    )
}

fn allocation_file_chains() -> Vec<Chain> {
    let lg2 = || allocation(Allocation::Lg2, Some(&[-8.0, 8.0]));
    vec![
        group_chain("lg2-lut-scale", vec![lg2(), lut(), matrix()]),
        group_chain(
            "lut-scale-lg2-lut-scale-lg2",
            vec![lut(), matrix(), lg2(), lut(), matrix(), lg2()],
        ),
        group_chain(
            "lg2-scale-lut-scale-lg2-lut-scale-lg2",
            vec![
                lg2(),
                matrix(),
                lut(),
                matrix(),
                lg2(),
                lut(),
                matrix(),
                lg2(),
            ],
        ),
        group_chain(
            "uniform-default-lut",
            vec![allocation(Allocation::Uniform, None), lut()],
        ),
        group_chain(
            "lg2-default-lut",
            vec![allocation(Allocation::Lg2, None), lut()],
        ),
        group_chain("file", vec![file()]),
        group_chain("scale-file-scale", vec![matrix(), file(), matrix()]),
    ]
}

#[derive(Debug)]
struct Job {
    chain: usize,
    level: usize,
    edge: u32,
    before: usize,
    language: usize,
}

impl Job {
    fn args(&self, chain: &Chain) -> Value {
        let before = match self.before {
            0 => json!([]),
            1 => json!([{"kind": "default", "optimization": LEVELS[self.level].0}]),
            2 => json!([{"kind": "legacy", "optimization": LEVELS[self.level].0, "edge_len": 2}]),
            _ => unreachable!("the generated request order"),
        };
        GpuShaderRequest::new(
            json!({"transform": chain.spec.clone(), "optimization": LEVELS[self.level].0,
                   "edge_len": self.edge, "before": before}),
            ShaderSettings::language(oracle_gpu::GpuLanguage::ALL[self.language]),
        )
        .args()
    }
}

fn port(chain: &Chain, job: &Job) -> Outcome {
    let raised = |stage: &str, error: ocio::Exception| {
        Outcome::Raised(stage.into(), error.message().to_string())
    };
    let config = match Config::create_raw() {
        Ok(value) => value,
        Err(error) => return raised("config", error),
    };
    let processor = match config.processor(&chain.transform) {
        Ok(value) => value,
        Err(error) => return raised("processor", error),
    };
    let flags = LEVELS[job.level].1;
    // Match the existing native helper's earlier cache-priming requests, including I-501.
    let prime = match job.before {
        0 => None,
        1 => Some(processor.optimized_gpu_processor(flags)),
        2 => Some(processor.optimized_legacy_gpu_processor(flags, 2)),
        _ => unreachable!("the generated request order"),
    };
    let _prime = match prime {
        Some(Err(error)) => return raised("gpu_processor", error),
        Some(Ok(value)) => Some(value),
        None => None,
    };
    let gpu = match processor.optimized_legacy_gpu_processor(flags, job.edge) {
        Ok(value) => value,
        Err(error) => return raised("gpu_processor", error),
    };
    let cache_id = match processor.cache_id() {
        Ok(value) => value,
        Err(error) => return raised("gpu_processor", error),
    };
    let text = |bytes: &[u8]| String::from_utf8(bytes.to_vec()).expect("UTF-8");
    let processor = json!({
        "processor_cache_id": cache_id,
        "gpu_cache_id": text(gpu.get_cache_id()),
        "gpu_processor": {"isNoOp": gpu.is_no_op(), "hasChannelCrosstalk": gpu.has_channel_crosstalk()},
    });
    let mut desc = GpuShaderDesc::new(GpuLanguage::ALL[job.language]);
    let shader = gpu.extract_gpu_shader_info(&mut desc).map(|()| json!({
        "text": text(desc.shader_text()), "cache_id": text(&desc.cache_id()),
        "function_name": text(desc.function_name()), "pixel_name": text(desc.pixel_name()),
        "resource_prefix": text(desc.resource_prefix()), "uniforms": desc.num_uniforms(),
        "textures": (0..desc.num_textures()).map(|i| texture_json(&port_texture(&desc, i))).collect::<Vec<_>>(),
        "textures_3d": (0..desc.num_textures_3d()).map(|i| port_texture_3d(&desc, i)).collect::<Vec<_>>(),
        "dynamic_properties": desc.num_dynamic_properties(),
    })).map_err(|error| ("extract".to_string(), error.message().to_string()));
    Outcome::Gpu { processor, shader }
}

#[test]
fn small_legacy_lattices_and_cache_order_match_native_wheel() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let chains = chains();
    let mut jobs = Vec::new();
    for chain in 0..chains.len() {
        for level in 0..LEVELS.len() {
            for edge in [2, 3] {
                for before in 0..3 {
                    for language in 0..GpuLanguage::ALL.len() {
                        jobs.push(Job {
                            chain,
                            level,
                            edge,
                            before,
                            language,
                        });
                    }
                }
            }
        }
    }
    assert_eq!(jobs.len(), 1680, "the complete P32 input plan");
    compare_jobs("P32", &chains, &jobs);
}

#[test]
fn legacy_allocation_cuts_and_file_boundaries_match_native_wheel() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let chains = allocation_file_chains();
    let mut jobs = Vec::new();
    for chain in 0..chains.len() {
        for level in 0..LEVELS.len() {
            for edge in [2, 3] {
                for language in 0..GpuLanguage::ALL.len() {
                    jobs.push(Job {
                        chain,
                        level,
                        edge,
                        before: 0,
                        language,
                    });
                }
            }
        }
    }
    assert_eq!(jobs.len(), 980, "the complete P33 input plan");
    compare_jobs("P33", &chains, &jobs);
}

fn compare_jobs(chunk: &str, chains: &[Chain], jobs: &[Job]) {
    let mut compared = 0;
    let mut refusals = 0;
    for batch in jobs.chunks(50) {
        let calls = batch
            .iter()
            .map(|job| BatchCall {
                cmd: "legacy_gpu_shader",
                args: job.args(&chains[job.chain]),
                blobs: Vec::new(),
            })
            .collect::<Vec<_>>();
        let responses = Oracle::get().batch(&calls, true);
        assert_eq!(responses.len(), batch.len());
        for (job, response) in batch.iter().zip(responses) {
            let chain = &chains[job.chain];
            let label = format!("{} {job:?}", chain.label);
            let reply = GpuShaderReply::from_response(
                response.unwrap_or_else(|error| panic!("{label}: {error}")),
            );
            assert_eq!(
                reply.result["oracle_native"]["wheel_version"],
                json!("2.5.2"),
                "{label}: actual native wheel"
            );
            let expected = wheel(&reply);
            let (actual, actual_log) = capture(|| port(chain, job));
            assert_eq!(actual_log, reply.log(), "{label}: log");
            assert_eq!(actual, expected, "{label}");
            if let Outcome::Gpu { shader: Ok(_), .. } = expected {
                // These chains contain no dynamic ops. A new uniform/property must not
                // silently go unmeasured by this non-dynamic comparison.
                assert!(
                    reply.shader().uniforms.is_empty(),
                    "{label}: compare uniform values"
                );
                assert!(
                    reply.shader().dynamic_properties.is_empty(),
                    "{label}: compare dynamic values"
                );
                compared += 1;
            } else {
                refusals += 1;
            }
        }
    }
    assert_eq!(compared + refusals, jobs.len());
    assert!(compared > 0);
    println!(
        "{chunk}: {} exact legacy requests: {compared} shaders/textures, {refusals} matching refusals",
        jobs.len()
    );
}
