// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! WP7.1c: small legacy lattices, allocation/file boundaries and GPU-cache request order
//! against O7.1's native client linked to the official wheel. Shader text/cache IDs, logs
//! and every texture bit remain exact. No FixedFunctionTransform or execution exception
//! is involved. Post-extraction dynamic updates remain separate work.

use ocio::{
    Allocation, AllocationTransform, Config, DynamicPropertyType, ExposureContrastTransform,
    FileTransform, GroupTransform, Lut1DTransform, Lut3DTransform, MatrixTransform,
    OptimizationFlags, RangeTransform, Transform,
};
use ocio_gpu::gpu_shader::{TextureDimensions, TextureType, UniformData};
use ocio_gpu::{GpuLanguage, GpuShaderDesc};
use ocio_ops::logging::{reset_to_default_logging_function, set_logging_function};
use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;
use ocio_testkit::Oracle;
use ocio_testkit::gpu::{
    self as oracle_gpu, GpuShaderReply, GpuShaderRequest, ShaderSettings, Texture,
};
use ocio_testkit::oracle::{BatchCall, Response};
use ocio_testkit::processor_ops::Dumped;
use serde_json::{Value, json};
use std::io::Write;
use std::process::Stdio;
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

fn uniform_value_json(value: &oracle_gpu::UniformValue) -> Value {
    match value {
        oracle_gpu::UniformValue::Unknown => Value::Null,
        oracle_gpu::UniformValue::Double(value) => json!(value.to_bits()),
        oracle_gpu::UniformValue::Bool(value) => json!(value),
        oracle_gpu::UniformValue::Float3(values) => {
            json!(values.map(f32::to_bits))
        }
        oracle_gpu::UniformValue::VectorFloat(values) => {
            json!(
                values
                    .iter()
                    .map(|value| value.to_bits())
                    .collect::<Vec<_>>()
            )
        }
        oracle_gpu::UniformValue::VectorInt(values) => json!(values),
    }
}

/// Exact uniform rows, as api_gpu_oracle::port_uniform_rows compares the ordinary API.
fn port_uniforms(desc: &GpuShaderDesc) -> Vec<Value> {
    let text = |value: &[u8]| String::from_utf8(value.to_vec()).expect("UTF-8");
    let bits = |values: &[f32]| {
        values
            .iter()
            .map(|value| value.to_bits())
            .collect::<Vec<_>>()
    };
    desc.uniforms()
        .iter()
        .map(|uniform| {
            let (kind, value) = match uniform.data() {
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
            json!([text(uniform.name()), kind, uniform.buffer_offset(), value])
        })
        .collect()
}

fn port_dynamic_properties(desc: &GpuShaderDesc) -> Vec<Value> {
    desc.dynamic_properties()
        .iter()
        .map(|property| {
            let kind = match property.get_type() {
                DynamicPropertyType::Exposure => "DYNAMIC_PROPERTY_EXPOSURE",
                DynamicPropertyType::Contrast => "DYNAMIC_PROPERTY_CONTRAST",
                DynamicPropertyType::Gamma => "DYNAMIC_PROPERTY_GAMMA",
                other => panic!("unexpected dynamic input {other:?}"),
            };
            let value = property.as_double().expect("the input's double property");
            json!([kind, value.get_value().to_bits()])
        })
        .collect()
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
                "getters": shader.getters,
                "uniforms": shader.uniforms.iter().map(|uniform| json!([
                    uniform.name, uniform.kind, uniform.buffer_offset,
                    uniform_value_json(&uniform.value),
                ])).collect::<Vec<_>>(),
                "textures": shader.textures.iter().map(texture_json).collect::<Vec<_>>(),
                "textures_3d": shader.textures_3d.iter().map(|t| json!({
                    "name": t.name, "sampler_name": t.sampler_name, "edge_len": t.edge_len,
                    "interpolation": t.interpolation, "binding_index": t.binding_index,
                    "values": t.values.iter().map(|v| v.to_bits()).collect::<Vec<u32>>(),
                })).collect::<Vec<Value>>(),
                "dynamic_properties": shader.dynamic_properties.iter().map(|property| {
                    let Dumped::F64(value) = &property.value else {
                        panic!("unexpected dynamic input {:?}", property.value)
                    };
                    json!([property.kind, value.to_bits()])
                }).collect::<Vec<_>>(),
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
    dynamic: bool,
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
        dynamic: false,
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

/// Initial values from ExposureContrastTransform_tests.cpp:47-50 @v2.5.2.
fn dynamic(kind: usize) -> (Transform, Value) {
    let mut transform = ExposureContrastTransform::new();
    transform.set_exposure(1.1);
    transform.set_contrast(0.5);
    transform.set_gamma(1.5);
    let mut calls = vec![
        json!(["setExposure", 1.1]),
        json!(["setContrast", 0.5]),
        json!(["setGamma", 1.5]),
    ];
    if matches!(kind, 0 | 3) {
        transform.make_exposure_dynamic();
        calls.push(json!(["makeExposureDynamic"]));
    }
    if matches!(kind, 1 | 3) {
        transform.make_contrast_dynamic();
        calls.push(json!(["makeContrastDynamic"]));
    }
    if matches!(kind, 2 | 3) {
        transform.make_gamma_dynamic();
        calls.push(json!(["makeGammaDynamic"]));
    }
    (
        transform.into(),
        json!({"class": "ExposureContrastTransform", "calls": calls}),
    )
}

fn dynamic_chains() -> Vec<Chain> {
    let mut chains = Vec::new();
    for kind in 0..4 {
        for (label, position) in [
            ("dynamic-before", 0),
            ("dynamic-inside", 1),
            ("dynamic-after", 2),
        ] {
            let mut steps = vec![dynamic(kind)];
            match position {
                0 => steps.push(lut()),
                1 => {
                    steps.insert(0, lut());
                    steps.push(lut());
                }
                2 => steps.insert(0, lut()),
                _ => unreachable!("the three lattice positions"),
            }
            let mut chain = group_chain(label, steps);
            chain.dynamic = true;
            chains.push(chain);
        }
    }
    chains
}

/// The existing zero-alpha lattice probe's inputs (no_ops_tests.rs:
/// a_legacy_lattice_matches_the_wheels_cpu_pixels), now through the complete processor.
fn matrix_alpha() -> (Transform, Value) {
    let values = [
        1.1, 0.2, -0.3, 0.7, -0.4, 0.9, 0.15, -0.2, 0.3, -0.1, 1.2, 0.5, 0.0, 0.0, 0.0, 1.0,
    ];
    let offset = [0.03, -0.04, 0.12, 0.25];
    let mut transform = MatrixTransform::new();
    transform.set_matrix(&values);
    transform.set_offset(&offset);
    (
        transform.into(),
        json!({"class": "MatrixTransform", "calls": [["setMatrix", values], ["setOffset", offset]]}),
    )
}

/// Probe inputs only; outputs come from the native wheel, including every baked value bit.
fn lut3() -> (Transform, Value) {
    let mut transform = Lut3DTransform::with_grid_size(2).expect("the input grid");
    transform
        .set_value(1, 1, 1, 0.8, 0.9, 0.7)
        .expect("the input corner");
    (
        transform.into(),
        json!({"class": "Lut3DTransform", "args": {"gridSize": 2},
               "calls": [["setValue", 1, 1, 1, 0.8, 0.9, 0.7]]}),
    )
}

fn range() -> (Transform, Value) {
    let mut transform = RangeTransform::new();
    transform.set_min_in_value(0.0);
    transform.set_max_in_value(1.0);
    transform.set_min_out_value(0.125);
    transform.set_max_out_value(0.875);
    (
        transform.into(),
        json!({"class": "RangeTransform", "calls": [
            ["setMinInValue", 0.0], ["setMaxInValue", 1.0],
            ["setMinOutValue", 0.125], ["setMaxOutValue", 0.875],
        ]}),
    )
}

fn mixed_chains() -> Vec<Chain> {
    vec![
        group_chain("lut3", vec![lut3()]),
        group_chain("alpha-lut3-range", vec![matrix_alpha(), lut3(), range()]),
        group_chain(
            "lut1-alpha-lut3-range",
            vec![lut(), matrix_alpha(), lut3(), range()],
        ),
        group_chain("range-lut1-range", vec![range(), lut(), range()]),
        group_chain(
            "uniform-vars-lut3",
            vec![allocation(Allocation::Uniform, Some(&[-0.5, 1.5])), lut3()],
        ),
        group_chain(
            "lg2-vars-lut3",
            vec![allocation(Allocation::Lg2, Some(&[-8.0, 8.0])), lut3()],
        ),
        group_chain(
            "lg2-lut3-alpha-lut1",
            vec![
                allocation(Allocation::Lg2, Some(&[-8.0, 8.0])),
                lut3(),
                matrix_alpha(),
                lut(),
            ],
        ),
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
    port_requested(chain, job, false)
}

fn port_requested(chain: &Chain, job: &Job, ordinary_last: bool) -> Outcome {
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
    let last = if ordinary_last {
        processor.optimized_gpu_processor(flags)
    } else {
        processor.optimized_legacy_gpu_processor(flags, job.edge)
    };
    let gpu = match last {
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
        "resource_prefix": text(desc.resource_prefix()), "uniforms": port_uniforms(&desc),
        "getters": {
            "language": oracle_gpu::GpuLanguage::ALL[job.language].oracle_name(),
            "function_name": text(desc.function_name()), "pixel_name": text(desc.pixel_name()),
            "resource_prefix": text(desc.resource_prefix()), "uid": text(desc.unique_id()),
            "descriptor_set_index": desc.descriptor_set_index(),
            "texture_binding_start": desc.texture_binding_start(),
            "texture_max_width": desc.texture_max_width(), "allow_texture_1d": desc.allow_texture_1d(),
            "uniform_buffer_size": desc.uniform_buffer_size(),
        },
        "textures": (0..desc.num_textures()).map(|i| texture_json(&port_texture(&desc, i))).collect::<Vec<_>>(),
        "textures_3d": (0..desc.num_textures_3d()).map(|i| port_texture_3d(&desc, i)).collect::<Vec<_>>(),
        "dynamic_properties": port_dynamic_properties(&desc),
    })).map_err(|error| ("extract".to_string(), error.message().to_string()));
    Outcome::Gpu { processor, shader }
}

// The original complete P32 generator is retained before selecting any execution part.
fn small_legacy_plan() -> (Vec<Chain>, Vec<Job>) {
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
    (chains, jobs)
}

fn small_legacy_part(jobs: Vec<Job>, part: usize) -> Vec<Job> {
    assert!(part < 4, "one of the four complete execution parts");
    assert_eq!(jobs.len(), 1680, "retain the original complete P32 plan");
    let selected = jobs
        .into_iter()
        .skip(part * 420)
        .take(420)
        .collect::<Vec<_>>();
    assert_eq!(
        selected.len(),
        420,
        "each original request belongs to one part"
    );
    selected
}

fn compare_small_legacy_part(part: usize) {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let (chains, jobs) = small_legacy_plan();
    let selected = small_legacy_part(jobs, part);
    println!("P32 execution part {part}/4; complete original plan 1680 requests");
    // Keep P32's original no-dynamic checks and all exact comparisons in compare_jobs.
    compare_jobs("P32", &chains, &selected);
}

#[test]
fn small_legacy_lattices_and_cache_order_match_native_wheel_part0() {
    compare_small_legacy_part(0);
}

#[test]
fn small_legacy_lattices_and_cache_order_match_native_wheel_part1() {
    compare_small_legacy_part(1);
}

#[test]
fn small_legacy_lattices_and_cache_order_match_native_wheel_part2() {
    compare_small_legacy_part(2);
}

#[test]
fn small_legacy_lattices_and_cache_order_match_native_wheel_part3() {
    compare_small_legacy_part(3);
}

#[test]
fn small_legacy_parts_cover_original_requests_once() {
    let (_, jobs) = small_legacy_plan();
    let key = |j: &Job| (j.chain, j.level, j.edge, j.before, j.language);
    let original = jobs.iter().map(key).collect::<Vec<_>>();
    let mut expected = std::collections::BTreeSet::new();
    // Original P32 domains (the complete generator above, before execution partitioning).
    for chain in 0..4 {
        for level in 0..7 {
            for edge in [2, 3] {
                for before in 0..3 {
                    for language in 0..10 {
                        assert!(expected.insert((chain, level, edge, before, language)));
                    }
                }
            }
        }
    }
    assert_eq!(
        expected.len(),
        1680,
        "all original P32 Cartesian-product inputs"
    );
    assert_eq!(
        original
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>(),
        expected
    );
    let mut observed = Vec::new();
    for part in 0..4 {
        let (_, full) = small_legacy_plan();
        observed.extend(small_legacy_part(full, part).iter().map(key));
    }
    assert_eq!(
        observed, original,
        "four parts preserve every original input and its order"
    );
    assert_eq!(
        observed
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        1680
    );
    println!("P32 input audit: all 1680 original requests exactly once in four ordered parts");
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

#[test]
fn legacy_dynamic_positions_and_snapshot_values_match_native_wheel() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let chains = dynamic_chains();
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
    assert_eq!(jobs.len(), 1680, "the complete P34 input plan");
    let measured = compare_jobs("P34", &chains, &jobs);
    assert!(
        measured > 0,
        "the native sweep must expose dynamic snapshot values"
    );
    println!("P34: {measured} exact uniform/property snapshot rows");
}

#[test]
fn legacy_mixed_lut_range_and_alpha_lattices_match_native_wheel() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let chains = mixed_chains();
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
    assert_eq!(jobs.len(), 980, "the complete P37 mixed-op input plan");
    compare_batches("P37", &chains, &jobs, 50, false);
}

fn boundary_legacy_plan() -> (Vec<Chain>, Vec<Job>) {
    let chains = chains();
    let mut jobs = Vec::new();
    // NoOps.cpp:175-206 constructs Lut3DOpData before allocating the RGBA lattice.
    // The existing array constructor rejects >129. Zero retains I-503's native error;
    // an analytic-only chain can bypass the lattice. The wheel decides every outcome.
    for chain in 0..chains.len() {
        for level in 0..LEVELS.len() {
            for edge in [0, 1, 130, 1626, u32::MAX] {
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
    assert_eq!(jobs.len(), 1400, "the complete P35 boundary input plan");
    (chains, jobs)
}

// Partition execution only: keep the complete original P35 Cartesian product above.
const BOUNDARY_PARTS: usize = 7;

fn boundary_legacy_part(jobs: Vec<Job>, part: usize) -> Vec<Job> {
    assert!(part < BOUNDARY_PARTS);
    assert_eq!(jobs.len(), 1400, "the unchanged original P35 input plan");
    let selected = jobs
        .into_iter()
        .enumerate()
        .filter_map(|(index, job)| (index % BOUNDARY_PARTS == part).then_some(job))
        .collect::<Vec<_>>();
    assert_eq!(selected.len(), 200);
    selected
}

fn compare_boundary_legacy_part(part: usize) {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let (chains, jobs) = boundary_legacy_plan();
    let selected = boundary_legacy_part(jobs, part);
    let indices = (part..1400).step_by(BOUNDARY_PARTS).collect::<Vec<_>>();
    println!("P64 boundary part {part}/7 original indices: {indices:?}");
    // Every part retains the same exact outcome/log/texture/uniform checks and batch size.
    compare_batches("P35 boundary", &chains, &selected, 50, false);
}

#[test]
fn legacy_boundary_edges_match_native_wheel_part0() {
    compare_boundary_legacy_part(0);
}

#[test]
fn legacy_boundary_edges_match_native_wheel_part1() {
    compare_boundary_legacy_part(1);
}

#[test]
fn legacy_boundary_edges_match_native_wheel_part2() {
    compare_boundary_legacy_part(2);
}

#[test]
fn legacy_boundary_edges_match_native_wheel_part3() {
    compare_boundary_legacy_part(3);
}

#[test]
fn legacy_boundary_edges_match_native_wheel_part4() {
    compare_boundary_legacy_part(4);
}

#[test]
fn legacy_boundary_edges_match_native_wheel_part5() {
    compare_boundary_legacy_part(5);
}

#[test]
fn legacy_boundary_edges_match_native_wheel_part6() {
    compare_boundary_legacy_part(6);
}

#[test]
fn boundary_legacy_parts_cover_original_requests_once() {
    let (_, jobs) = boundary_legacy_plan();
    let key = |job: &Job| (job.chain, job.level, job.edge, job.before, job.language);
    let original = jobs.iter().map(key).collect::<Vec<_>>();
    let mut expected = Vec::new();
    // Independent enumeration of the original P35 domains, before execution partitioning.
    for chain in 0..4 {
        for level in 0..7 {
            for edge in [0, 1, 130, 1626, u32::MAX] {
                for language in 0..10 {
                    expected.push((chain, level, edge, 0, language));
                }
            }
        }
    }
    assert_eq!(original, expected);
    let mut observed = std::collections::BTreeMap::new();
    for part in 0..BOUNDARY_PARTS {
        let (_, jobs) = boundary_legacy_plan();
        for (offset, job) in boundary_legacy_part(jobs, part).iter().enumerate() {
            let index = part + offset * BOUNDARY_PARTS;
            assert!(observed.insert(index, key(job)).is_none());
        }
    }
    assert_eq!(observed.len(), 1400);
    assert_eq!(observed.into_values().collect::<Vec<_>>(), original);
    println!("P64 input audit: all 1400 original requests exactly once in seven 200-request parts");
}

#[test]
fn legacy_large_lattice_values_match_native_wheel() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let chains = chains();
    let full = matches!(
        std::env::var("OCIO_RS_TIER").as_deref(),
        Ok("full" | "exhaustive")
    );
    let edges: &[u32] = if full { &[32, 64, 129] } else { &[32] };
    let level = LEVELS
        .iter()
        .position(|(name, _)| *name == "OPTIMIZATION_DEFAULT")
        .expect("the named DEFAULT input");
    let mut jobs = Vec::new();
    for chain in 0..chains.len() {
        for &edge in edges {
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
    assert_eq!(
        jobs.len(),
        if full { 120 } else { 40 },
        "the P35 large input plan"
    );
    // One live response at a time bounds peak memory. Never cache copies of 129^3 textures.
    compare_batches("P35 large", &chains, &jobs, 1, false);
}

/// The already-reviewed native packet supports both request kinds. The public command ends
/// legacy; consume its existing transport to end ordinary, after its complete ordinary
/// Python/native identity check. No output value is computed by this test-side consumer.
fn reverse_reference(args: &[Value]) -> Vec<Response> {
    const SCRIPT: &str = r#"
import json,sys
from ocio_oracle import commands,legacy_gpu,native_build
for index,args in enumerate(json.load(sys.stdin)):
    try:
        checked,_=legacy_gpu.legacy_gpu_shader(args,[])
        assert checked['oracle_native']['wheel_version']=='2.5.2'
        program,provenance=native_build.executable()
        requests=args['before']+[{'kind':'default','optimization':args['optimization']}]
        result,blobs,abi=legacy_gpu._native(program,legacy_gpu._message(args,[],requests))
        assert abi['wheel_version']=='2.5.2' and abi['abi']==checked['oracle_native']['abi']
        provenance.update(abi)
        result['oracle_native']=provenance
        print(json.dumps({'result':result,'blobs':[list(blob) for blob in blobs]}),flush=True)
    except Exception as error:
        raise RuntimeError(f'reverse request {index}: {args!r}') from error
"#;
    // Batch via stdin: full transform requests exceed Windows' argument limit together.
    // Oracle::command provides the same pinned Python, cwd and sanitized environment as
    // run_script; no response cache or per-case Python process is used here.
    let mut child = Oracle::get()
        .command()
        .args(["-X", "utf8", "-c", SCRIPT])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the oracle's Python runs");
    child
        .stdin
        .take()
        .expect("request stdin")
        .write_all(&serde_json::to_vec(args).expect("request JSON"))
        .expect("write reference requests");
    let output = child.wait_with_output().expect("reference process exits");
    assert!(
        output.status.success(),
        "native reverse reference: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).expect("reference UTF-8");
    let responses = text
        .lines()
        .map(|line| {
            let report: Value = serde_json::from_str(line).expect("native reference JSON");
            Response {
                result: report["result"].clone(),
                blobs: serde_json::from_value(report["blobs"].clone()).expect("native byte blobs"),
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(responses.len(), args.len(), "every reverse request replied");
    responses
}

#[test]
fn ordinary_after_legacy_keeps_native_cached_shader_and_texture_values() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let chains = chains();
    let mut jobs = Vec::new();
    for chain in 0..chains.len() {
        for level in 0..LEVELS.len() {
            for language in 0..GpuLanguage::ALL.len() {
                jobs.push(Job {
                    chain,
                    level,
                    edge: 2,
                    before: 2,
                    language,
                });
            }
        }
    }
    assert_eq!(jobs.len(), 280, "the complete P36 reverse input plan");
    let mut compared = 0;
    let mut refusals = 0;
    for batch in jobs.chunks(50) {
        let requests = batch
            .iter()
            .map(|job| job.args(&chains[job.chain]))
            .collect::<Vec<_>>();
        let responses = reverse_reference(&requests);
        for (job, response) in batch.iter().zip(responses) {
            let chain = &chains[job.chain];
            let label = format!("P36 {} {job:?}", chain.label);
            let reply = GpuShaderReply::from_response(response);
            assert_eq!(
                reply.result["oracle_native"]["wheel_version"],
                json!("2.5.2"),
                "{label}"
            );
            let expected = wheel(&reply);
            let (actual, log) = capture(|| port_requested(chain, job, true));
            assert_eq!(log, reply.log(), "{label}: log");
            assert!(
                actual == expected,
                "{label}: differs at {}; request {}",
                outcome_difference(&actual, &expected),
                job.args(chain)
            );
            if let Outcome::Gpu { shader: Ok(_), .. } = expected {
                assert!(
                    reply.shader().uniforms.is_empty(),
                    "{label}: non-dynamic uniforms"
                );
                assert!(
                    reply.shader().dynamic_properties.is_empty(),
                    "{label}: non-dynamic properties"
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
        "P36: {} exact reverse requests: {compared} shaders/textures, {refusals} matching refusals",
        jobs.len()
    );
}

/// A bounded diagnostic path; equality still compares every bit/byte of the whole outcome.
fn json_difference_path(actual: &Value, expected: &Value, path: &str) -> Option<String> {
    if actual == expected {
        return None;
    }
    match (actual, expected) {
        (Value::Array(actual), Value::Array(expected)) if actual.len() == expected.len() => actual
            .iter()
            .zip(expected)
            .enumerate()
            .find_map(|(index, (actual, expected))| {
                json_difference_path(actual, expected, &format!("{path}[{index}]"))
            }),
        (Value::Object(actual), Value::Object(expected)) if actual.len() == expected.len() => {
            actual
                .iter()
                .find_map(|(key, actual)| match expected.get(key) {
                    Some(expected) => {
                        json_difference_path(actual, expected, &format!("{path}.{key}"))
                    }
                    None => Some(format!("{path}.{key} (missing key)")),
                })
        }
        _ => Some(path.into()),
    }
}

fn outcome_difference(actual: &Outcome, expected: &Outcome) -> String {
    if let (
        Outcome::Gpu {
            processor: actual_processor,
            shader: actual_shader,
        },
        Outcome::Gpu {
            processor: expected_processor,
            shader: expected_shader,
        },
    ) = (actual, expected)
    {
        if let Some(path) = json_difference_path(actual_processor, expected_processor, "processor")
        {
            return path;
        }
        if let (Ok(actual), Ok(expected)) = (actual_shader, expected_shader) {
            return json_difference_path(actual, expected, "shader").expect("different shader");
        }
        return "extraction outcome/stage/message".into();
    }
    "processor outcome/stage/message".into()
}

fn compare_jobs(chunk: &str, chains: &[Chain], jobs: &[Job]) -> usize {
    compare_batches(chunk, chains, jobs, 50, true)
}

fn compare_batches(
    chunk: &str,
    chains: &[Chain],
    jobs: &[Job],
    batch_size: usize,
    cacheable: bool,
) -> usize {
    let mut compared = 0;
    let mut refusals = 0;
    let mut measured_dynamic = 0;
    for (batch_index, batch) in jobs.chunks(batch_size).enumerate() {
        println!(
            "{chunk}: starting original request batch {}..{} of {}",
            batch_index * batch_size,
            batch_index * batch_size + batch.len(),
            jobs.len()
        );
        let calls = batch
            .iter()
            .map(|job| BatchCall {
                cmd: "legacy_gpu_shader",
                args: job.args(&chains[job.chain]),
                blobs: Vec::new(),
            })
            .collect::<Vec<_>>();
        let responses = Oracle::get().batch(&calls, cacheable);
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
            assert!(
                actual == expected,
                "{label}: exact outcome differs at {}; request {}",
                outcome_difference(&actual, &expected),
                job.args(chain)
            );
            if let Outcome::Gpu { shader: Ok(_), .. } = expected {
                measured_dynamic +=
                    reply.shader().uniforms.len() + reply.shader().dynamic_properties.len();
                // Preserve P32/P33's no-dynamic invariant in addition to comparing all
                // rows and value bits. P34's deliberately dynamic chains compare the full
                // snapshots, including what the native lattice retained or baked away.
                if !chain.dynamic {
                    assert!(
                        reply.shader().uniforms.is_empty(),
                        "{label}: non-dynamic uniforms"
                    );
                    assert!(
                        reply.shader().dynamic_properties.is_empty(),
                        "{label}: non-dynamic properties"
                    );
                }
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
    measured_dynamic
}
