// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Lut3D op's GPU writer against the wheel (`gpu_shader`), in the 10 languages: for each
//! case, the GPU processor's cache ID and queries, and what the extraction writes (the
//! shader's text and cache ID, and the 3D textures with their names, sizes, interpolation,
//! binding indices and values, bit for bit), or the error, byte for byte.
//!
//! The wheel builds a `GroupTransform` of `Lut3DTransform`s, their values as blobs
//! (`setData`), and the GPU processor of its processor at `OPTIMIZATION_NONE`, so that each
//! LUT stays an op of its own. The port makes the ops as `BuildLut3DOp` does
//! (src/OpenColorIO/ops/lut3d/Lut3DOp.cpp:251-261 @ v2.5.2) and runs the GPU processor from
//! them. The cases: tetrahedral and trilinear interpolation (nearest, linear, best and
//! default too); an inverse LUT, which the writer replaces with its fast forward LUT; grids of
//! 1 to 129 entries per side; NaN and infinite values; several LUTs in one shader; and the
//! description's names (a resource prefix that makes a double underscore, which the writer
//! removes; a pixel name; a Vulkan descriptor set).
//!
//! The inverse of a grid of one entry is left out: the wheel never returns (U-65).

use std::ffi::c_ulong;

use ocio_gpu::gpu_processor::GpuProcessor;
use ocio_gpu::{GpuLanguage, GpuShaderDesc};
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{OptimizationFlags, TransformDirection};
use ocio_ops::ops::lut3d::lut3d_op::create_lut3d_op;
use ocio_ops::ops::lut3d::lut3d_op_data::{Interpolation, Lut3DOpData};
use ocio_testkit::Oracle;
use ocio_testkit::gpu::{self as oracle_gpu, GpuShaderReply, GpuShaderRequest, ShaderSettings};
use ocio_testkit::oracle::{BatchCall, f32_to_bytes};
use ocio_testkit::probe::Rng;
use serde_json::{Value, json};

/// One `Lut3DTransform`.
#[derive(Debug, Clone)]
struct Lut {
    grid_size: u32,
    interpolation: Interpolation,
    inverse: bool,
    values: Vec<f32>,
}

impl Lut {
    fn new(grid_size: u32, interpolation: Interpolation, values: Vec<f32>) -> Lut {
        assert_eq!(values.len(), (grid_size as usize).pow(3) * 3);
        Lut {
            grid_size,
            interpolation,
            inverse: false,
            values,
        }
    }

    fn inverse(mut self) -> Lut {
        self.inverse = true;
        self
    }

    /// The transform, its values as blob `blob`.
    fn spec(&self, blob: usize) -> Value {
        let interp = match self.interpolation {
            Interpolation::Nearest => "INTERP_NEAREST",
            Interpolation::Linear => "INTERP_LINEAR",
            Interpolation::Tetrahedral => "INTERP_TETRAHEDRAL",
            Interpolation::Default => "INTERP_DEFAULT",
            Interpolation::Best => "INTERP_BEST",
            other => panic!("no name for {other:?}"),
        };
        let dir = if self.inverse {
            "TRANSFORM_DIR_INVERSE"
        } else {
            "TRANSFORM_DIR_FORWARD"
        };
        json!({"class": "Lut3DTransform", "calls": [
            ["setData", {"blob": blob, "dtype": "float32"}],
            ["setInterpolation", {"enum": interp}],
            ["setDirection", {"enum": dir}],
        ]})
    }

    /// The op data, as the transform's setters make it and `BuildLut3DOp` validates it.
    fn data(&self) -> Lut3DOpData {
        let mut lut =
            Lut3DOpData::with_interpolation(self.interpolation, c_ulong::from(self.grid_size))
                .expect("a valid LUT");
        lut.get_array_mut()
            .get_values_mut()
            .copy_from_slice(&self.values);
        lut.validate().expect("a valid LUT");
        lut
    }
}

/// A blue-fastest LUT of `grid_size` entries per side, `f` of each entry's grid position
/// scaled to [0, 1].
fn lut_of(grid_size: u32, f: impl Fn([f32; 3]) -> [f32; 3]) -> Vec<f32> {
    let n = grid_size as usize;
    let last = (n.max(2) - 1) as f32;
    let mut values = Vec::with_capacity(n * n * n * 3);
    for r in 0..n {
        for g in 0..n {
            for b in 0..n {
                values.extend(f([r, g, b].map(|i| i as f32 / last)));
            }
        }
    }
    values
}

fn smooth([r, g, b]: [f32; 3]) -> [f32; 3] {
    [r * r * 0.9 + 0.05, g.sqrt(), 0.2 + 0.6 * b + 0.1 * r]
}

/// Random values with NaNs (with payloads, either sign) and infinities in a third of them.
fn nan_inf(grid_size: u32, seed: u64) -> Vec<f32> {
    let specials = [
        f32::NAN,
        -f32::NAN,
        f32::from_bits(0x7fc0_1234),
        f32::INFINITY,
        f32::NEG_INFINITY,
    ];
    let mut rng = Rng::new(seed);
    (0..(grid_size as usize).pow(3) * 3)
        .map(|_| {
            if rng.next_u64().is_multiple_of(3) {
                specials[(rng.next_u64() % specials.len() as u64) as usize]
            } else {
                rng.uniform(-0.5, 1.5)
            }
        })
        .collect()
}

/// One shader: LUTs in a group, and the description's names.
#[derive(Debug, Clone)]
struct Case {
    label: String,
    luts: Vec<Lut>,
    settings: ShaderSettings,
}

impl Case {
    fn new(label: &str, luts: Vec<Lut>) -> Case {
        Case {
            label: label.to_string(),
            luts,
            settings: ShaderSettings::default(),
        }
    }

    fn settings(mut self, settings: ShaderSettings) -> Case {
        self.settings = settings;
        self
    }
}

fn cases() -> Vec<Case> {
    use Interpolation::{Best, Default, Linear, Nearest, Tetrahedral};
    let mut cases = Vec::new();
    for interp in [Tetrahedral, Linear, Nearest, Best, Default] {
        cases.push(Case::new(
            &format!("smooth 5^3, {interp:?}"),
            vec![Lut::new(5, interp, lut_of(5, smooth))],
        ));
    }
    for grid_size in [1, 2, 33, 65, 129] {
        cases.push(Case::new(
            &format!("smooth {grid_size}^3"),
            vec![Lut::new(grid_size, Tetrahedral, lut_of(grid_size, smooth))],
        ));
    }
    for interp in [Tetrahedral, Linear] {
        cases.push(Case::new(
            &format!("NaN and infinities 3^3, {interp:?}"),
            vec![Lut::new(3, interp, nan_inf(3, 0x4e41_4e00))],
        ));
        cases.push(Case::new(
            &format!("inverse smooth 5^3, {interp:?}"),
            vec![Lut::new(5, interp, lut_of(5, smooth)).inverse()],
        ));
    }
    cases.push(Case::new(
        "inverse smooth 49^3",
        vec![Lut::new(49, Tetrahedral, lut_of(49, smooth)).inverse()],
    ));
    let three = vec![
        Lut::new(3, Tetrahedral, lut_of(3, smooth)),
        Lut::new(4, Linear, lut_of(4, |[r, g, b]| [g, b, r])),
        Lut::new(2, Tetrahedral, lut_of(2, smooth)).inverse(),
    ];
    cases.push(Case::new("three LUTs", three.clone()));
    let named = |prefix: &str, pixel: &str| ShaderSettings {
        resource_prefix: Some(prefix.to_string()),
        pixel_name: Some(pixel.to_string()),
        ..ShaderSettings::default()
    };
    cases.push(Case::new("three LUTs, prefix my_", three.clone()).settings(named("my_", "px")));
    cases.push(Case::new("three LUTs, prefix __", three.clone()).settings(named("__", "outColor")));
    cases.push(
        Case::new("three LUTs, descriptor set", three).settings(ShaderSettings {
            descriptor_set: Some((2, 3)),
            ..ShaderSettings::default()
        }),
    );
    cases
}

/// What the wheel or the port did, to compare as a whole.
#[derive(Debug, Clone, PartialEq)]
enum Outcome {
    Raised(String, String),
    Gpu {
        processor: Value,
        shader: Result<Value, (String, String)>,
    },
}

fn bits(values: &[f32]) -> Vec<u32> {
    values.iter().map(|v| v.to_bits()).collect()
}

fn wheel(reply: &GpuShaderReply) -> Outcome {
    let result = &reply.result;
    let raised = reply.raised();
    if let Some(raised) = &raised
        && !matches!(raised.stage.as_str(), "extract" | "shader_desc")
    {
        return Outcome::Raised(raised.stage.clone(), raised.message.clone());
    }
    let processor = json!({
        "gpu_cache_id": result["gpu_cache_id"],
        "gpu_processor": result["gpu_processor"],
    });
    let shader = match raised {
        Some(raised) => Err((raised.stage, raised.message)),
        None => {
            let s = reply.shader();
            Ok(json!({
                "text": s.text,
                "cache_id": s.cache_id,
                "uniforms": s.uniforms.len(),
                "textures": s.textures.len(),
                "dynamic_properties": s.dynamic_properties.len(),
                "textures_3d": s.textures_3d.iter().map(|t| json!({
                    "name": t.name, "sampler_name": t.sampler_name, "edge_len": t.edge_len,
                    "interpolation": t.interpolation, "binding_index": t.binding_index,
                    "values": bits(&t.values),
                })).collect::<Vec<Value>>(),
            }))
        }
    };
    Outcome::Gpu { processor, shader }
}

fn port(case: &Case, language: GpuLanguage) -> Outcome {
    let text = |bytes: &[u8]| String::from_utf8(bytes.to_vec()).expect("UTF-8");
    let mut ops = OpVec::new();
    for lut in &case.luts {
        let dir = if lut.inverse {
            TransformDirection::Inverse
        } else {
            TransformDirection::Forward
        };
        create_lut3d_op(&mut ops, lut.data(), dir);
    }
    let gpu = match GpuProcessor::new(&ops, OptimizationFlags::NONE) {
        Ok(gpu) => gpu,
        Err(e) => return Outcome::Raised("gpu_processor".into(), e.message().into()),
    };
    let processor = json!({
        "gpu_cache_id": text(gpu.get_cache_id()),
        "gpu_processor": {"isNoOp": gpu.is_no_op(),
            "hasChannelCrosstalk": gpu.has_channel_crosstalk()},
    });
    let mut desc = GpuShaderDesc::new(language);
    let s = &case.settings;
    if let Some(prefix) = &s.resource_prefix {
        desc.set_resource_prefix(prefix);
    }
    if let Some(pixel) = &s.pixel_name {
        desc.set_pixel_name(pixel);
    }
    if let Some((index, start)) = s.descriptor_set {
        desc.set_descriptor_set_index(index, start)
            .expect("a descriptor set");
    }
    let shader = gpu
        .extract_gpu_shader_info(&mut desc)
        .map(|()| {
            let textures_3d: Vec<Value> = (0..desc.num_textures_3d())
                .map(|i| {
                    let t = desc.texture_3d(i).expect("a 3D texture");
                    json!({
                        "name": text(t.texture_name()),
                        "sampler_name": text(t.sampler_name()),
                        "edge_len": t.edge_len(),
                        "interpolation": match t.interpolation() {
                            Interpolation::Nearest => "INTERP_NEAREST",
                            Interpolation::Linear => "INTERP_LINEAR",
                            other => panic!("interpolation {other:?}"),
                        },
                        "binding_index": desc.texture_3d_shader_binding_index(i)
                            .expect("its binding"),
                        "values": bits(t.values()),
                    })
                })
                .collect();
            json!({
                "text": text(desc.shader_text()),
                "cache_id": text(&desc.cache_id()),
                "uniforms": desc.num_uniforms(),
                "textures": desc.num_textures(),
                "dynamic_properties": desc.num_dynamic_properties(),
                "textures_3d": textures_3d,
            })
        })
        .map_err(|e| ("extract".to_string(), e.message().to_string()));
    Outcome::Gpu { processor, shader }
}

#[test]
fn shaders_match_the_wheel() {
    let cases = cases();
    let mut requests = Vec::new();
    for (k, case) in cases.iter().enumerate() {
        let children: Vec<Value> = case
            .luts
            .iter()
            .enumerate()
            .map(|(i, lut)| lut.spec(i))
            .collect();
        let blobs: Vec<Vec<u8>> = case.luts.iter().map(|l| f32_to_bytes(&l.values)).collect();
        for (language, oracle_language) in oracle_gpu::GpuLanguage::ALL.iter().enumerate() {
            let request = GpuShaderRequest::new(
                json!({
                    "transform": {"class": "GroupTransform", "children": children},
                    "optimization": "OPTIMIZATION_NONE",
                }),
                ShaderSettings {
                    language: Some(*oracle_language),
                    ..case.settings.clone()
                },
            );
            requests.push((k, language, request.args(), blobs.clone()));
        }
    }
    let calls: Vec<BatchCall<'_>> = requests
        .iter()
        .map(|(_, _, args, blobs)| BatchCall {
            cmd: "gpu_shader",
            args: args.clone(),
            blobs: blobs.iter().map(Vec::as_slice).collect(),
        })
        .collect();
    let mut failures = Vec::new();
    let (mut shaders, mut refusals) = (0, 0);
    for ((k, language, ..), response) in requests.iter().zip(Oracle::get().batch(&calls, true)) {
        let case = &cases[*k];
        let what = format!("{} {:?}", case.label, GpuLanguage::ALL[*language]);
        let reply =
            GpuShaderReply::from_response(response.unwrap_or_else(|e| panic!("{what}: {e}")));
        if !reply.log().is_empty() {
            failures.push(format!("{what}: OCIO logged {:?}", reply.log()));
        }
        let wheel = wheel(&reply);
        let port = port(case, GpuLanguage::ALL[*language]);
        if wheel != port {
            failures.push(format!("{what}\n  wheel {wheel:?}\n  port  {port:?}"));
            continue;
        }
        match wheel {
            Outcome::Gpu { shader: Ok(_), .. } => shaders += 1,
            _ => refusals += 1,
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} shaders differ:\n{}",
        failures.len(),
        requests.len(),
        failures[..failures.len().min(5)]
            .iter()
            .map(|f| f.chars().take(3000).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    );
    // Every language but OSL writes the shaders; OSL refuses each.
    assert_eq!(refusals, cases.len(), "OSL's refusals");
    assert_eq!(shaders, cases.len() * 9);
}
