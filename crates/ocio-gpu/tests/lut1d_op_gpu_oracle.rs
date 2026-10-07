// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The 1D LUT op's GPU writer against the wheel (`gpu_shader`), in the 10 languages: for each
//! case, the GPU processor's cache ID and queries, and the shader the extraction writes (its
//! text, cache ID and names, and its textures: names, sizes, channels, dimensions,
//! interpolation, binding index and every value's bits), or the error, byte for byte.
//!
//! Each case is a list of `Lut1DTransform`s (built empty and set, as in
//! `crates/ocio-ops/tests/lut1d_op_oracle.rs`). `BuildLut1DOp` validates each transform's data
//! and creates a Lut1D op with a copy of it (src/OpenColorIO/ops/lut1d/Lut1DOp.cpp:244-253 @
//! v2.5.2); the processor finalizes the ops (Processor.cpp:618-641), and the GPU processor is
//! `getOptimizedGPUProcessor` at a level, or `getDefaultGPUProcessor` (Processor.cpp:437-445,
//! 491-523). The port builds the same data and ops, and runs the GPU processor from them as
//! they are, and from them finalized first: both must give the wheel's outcome.
//!
//! The cases cover the 1D texture and the 2D one (a LUT longer than the width limit, over
//! several rows; a half domain; GLSL ES; a description without 1D textures), one channel (an
//! identity LUT, whose channels are the same) and three, the hue adjustment, the
//! interpolations, NaN and infinite entries (the texture sanitizes them), inverse LUTs (made
//! into their fast forward LUT, by the optimizer at the levels with `OPTIMIZATION_LUT_INV_FAST`,
//! by the writer otherwise), two LUTs in one shader, and the description's names, descriptor
//! set and width limit. A fast inverse LUT is rendered on its domain by the CPU renderers, so
//! this test target is in `cpu-tests`.

use core::ffi::c_ulong;

use ocio_gpu::gpu_processor::GpuProcessor;
use ocio_gpu::gpu_shader::{TextureDimensions, TextureType};
use ocio_gpu::{GpuLanguage, GpuShaderDesc};
use ocio_ops::Exception;
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{Lut1DHueAdjust, OptimizationFlags, TransformDirection};
use ocio_ops::ops::lut1d::Lut1DOpData;
use ocio_ops::ops::lut1d::lut1d_op::create_lut1d_op;
use ocio_ops::ops::lut1d::lut1d_op_data::Lut3by1DArray;
use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;
use ocio_testkit::gpu::{
    self as oracle_gpu, GpuShaderReply, GpuShaderRequest, ShaderSettings, Texture,
};
use ocio_testkit::{Oracle, assert_text_eq};
use serde_json::{Value, json};

use TransformDirection::{Forward as F, Inverse as I};

/// A `Lut1DTransform`'s settings: the half domain, the length, entries set to `f(x)` of the
/// identity's value `x` (or left as they are), entries then set to the values of `specials`,
/// the hue adjust, the interpolation and the direction. The curve sets every `stride`th entry
/// only, which keeps the requests of half-domain LUTs small.
#[derive(Debug, Clone, Copy)]
struct Lut {
    half_domain: bool,
    length: c_ulong,
    /// `None`: the identity; otherwise each entry's RGB from the identity's value.
    curve: Option<fn(f32) -> [f32; 3]>,
    stride: usize,
    specials: &'static [(usize, [f32; 3])],
    hue: Lut1DHueAdjust,
    interpolation: Interpolation,
    dir: TransformDirection,
}

/// Each channel a power of the input, the powers apart.
fn powers(x: f32) -> [f32; 3] {
    [x.powf(0.5), x.powf(0.6), x.powf(0.7)]
}

/// An increasing curve with values past [0, 1], so that its fast inverse has a half domain.
fn wide(x: f32) -> [f32; 3] {
    [x * 1.5 - 0.1, x * 1.25 - 0.05, x * 2.0]
}

/// NaN and infinite entries.
const SPECIALS: &[(usize, [f32; 3])] = &[
    (1, [f32::NAN, 0.5, f32::INFINITY]),
    (3, [f32::NEG_INFINITY, f32::NAN, 0.25]),
];

impl Lut {
    fn new(length: c_ulong, curve: Option<fn(f32) -> [f32; 3]>) -> Lut {
        Lut {
            half_domain: false,
            length,
            curve,
            stride: 1,
            specials: &[],
            hue: Lut1DHueAdjust::None,
            interpolation: Interpolation::Linear,
            dir: F,
        }
    }

    fn half(curve: Option<fn(f32) -> [f32; 3]>) -> Lut {
        Lut {
            half_domain: true,
            stride: 97,
            ..Lut::new(65536, curve)
        }
    }

    /// The identity's value of each entry, as `setLength` fills them.
    fn values(&self) -> Lut3by1DArray {
        let mut data = Lut1DOpData::new(2).expect("a LUT");
        data.set_input_half_domain(self.half_domain);
        Lut3by1DArray::new(data.get_half_flags(), 3, self.length, false).expect("a valid length")
    }

    /// The entries the transform's `setValue` sets: the curve's finite values, then the
    /// specials.
    fn entries(&self) -> Vec<(usize, [f32; 3])> {
        let mut out = Vec::new();
        if let Some(curve) = self.curve {
            let identity = self.values();
            out.extend(
                (0..self.length as usize)
                    .step_by(self.stride)
                    .map(|i| (i, curve(identity[3 * i])))
                    .filter(|(_, rgb)| rgb.iter().all(|v| v.is_finite())),
            );
        }
        out.extend_from_slice(self.specials);
        out
    }

    fn spec(&self) -> Value {
        let mut calls = vec![
            json!(["setInputHalfDomain", self.half_domain]),
            json!(["setLength", self.length]),
        ];
        // A float as a C double's bits: JSON has no NaN or infinity.
        let f = |v: f32| json!({ "f64": f64::from(v).to_bits() });
        for (index, rgb) in self.entries() {
            calls.push(json!(["setValue", index, f(rgb[0]), f(rgb[1]), f(rgb[2])]));
        }
        let hue = match self.hue {
            Lut1DHueAdjust::None => "HUE_NONE",
            Lut1DHueAdjust::Dw3 => "HUE_DW3",
            Lut1DHueAdjust::Wypn => "HUE_WYPN",
        };
        calls.push(json!(["setHueAdjust", {"enum": hue}]));
        calls.push(json!(["setInterpolation", {"enum": interpolation_name(self.interpolation)}]));
        let dir = match self.dir {
            F => "TRANSFORM_DIR_FORWARD",
            I => "TRANSFORM_DIR_INVERSE",
        };
        calls.push(json!(["setDirection", {"enum": dir}]));
        json!({"class": "Lut1DTransform", "args": {}, "calls": calls})
    }

    /// The port's data, as the transform's setters make it.
    fn port(&self) -> ocio_ops::Result<Lut1DOpData> {
        let mut data = Lut1DOpData::new(2)?;
        data.set_input_half_domain(self.half_domain);
        *data.get_array_mut() = self.values();
        for (index, rgb) in self.entries() {
            for (c, v) in rgb.into_iter().enumerate() {
                // `setValue` takes floats; the binding passes the doubles as floats.
                data.get_array_mut()[3 * index + c] = v;
            }
        }
        data.set_hue_adjust(self.hue)?;
        data.set_interpolation(self.interpolation);
        data.set_direction(self.dir);
        Ok(data)
    }
}

fn interpolation_name(i: Interpolation) -> &'static str {
    match i {
        Interpolation::Unknown => "INTERP_UNKNOWN",
        Interpolation::Nearest => "INTERP_NEAREST",
        Interpolation::Linear => "INTERP_LINEAR",
        Interpolation::Tetrahedral => "INTERP_TETRAHEDRAL",
        Interpolation::Cubic => "INTERP_CUBIC",
        Interpolation::Default => "INTERP_DEFAULT",
        Interpolation::Best => "INTERP_BEST",
    }
}

fn channel_name(c: TextureType) -> &'static str {
    match c {
        TextureType::RedChannel => "TEXTURE_RED_CHANNEL",
        TextureType::RgbChannel => "TEXTURE_RGB_CHANNEL",
    }
}

fn dimensions_name(d: TextureDimensions) -> &'static str {
    match d {
        TextureDimensions::D1 => "TEXTURE_1D",
        TextureDimensions::D2 => "TEXTURE_2D",
    }
}

fn utf8(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("UTF-8")
}

/// The port's texture `index`, as the oracle reports the wheel's.
fn texture(desc: &GpuShaderDesc, index: u32) -> Texture {
    let t = desc.texture(index).expect("a texture");
    Texture {
        name: utf8(t.texture_name()),
        sampler_name: utf8(t.sampler_name()),
        width: u64::from(t.width()),
        height: u64::from(t.height()),
        channel: channel_name(t.channel()).into(),
        dimensions: dimensions_name(t.dimensions()).into(),
        interpolation: interpolation_name(t.interpolation()).into(),
        binding_index: u64::from(desc.texture_shader_binding_index(index).expect("a binding")),
        values: t.values().to_vec(),
    }
}

/// The optimization levels, as the oracle takes them; `None` is `getDefaultGPUProcessor`.
fn levels() -> Vec<(Option<&'static str>, OptimizationFlags)> {
    vec![
        (None, OptimizationFlags::DEFAULT),
        (Some("OPTIMIZATION_NONE"), OptimizationFlags::NONE),
        (Some("OPTIMIZATION_ALL"), OptimizationFlags::ALL),
    ]
}

/// One extraction.
#[derive(Debug, Clone)]
struct Case {
    label: String,
    luts: Vec<Lut>,
    flags: (Option<&'static str>, OptimizationFlags),
    language: GpuLanguage,
    settings: ShaderSettings,
}

impl Case {
    fn request(&self) -> GpuShaderRequest {
        let children: Vec<Value> = self.luts.iter().map(Lut::spec).collect();
        let transform = json!({"class": "GroupTransform", "children": children});
        let mut processor = json!({ "transform": transform });
        if let Some(flags) = self.flags.0 {
            processor["optimization"] = json!(flags);
        }
        GpuShaderRequest::new(processor, self.settings.clone())
    }
}

/// What the wheel and the port give for a case.
#[derive(Debug, Clone, PartialEq)]
enum Outcome {
    Raised(String),
    Extracted {
        gpu_cache_id: String,
        is_no_op: bool,
        has_channel_crosstalk: bool,
        /// The shader's text, cache ID, pixel name and resource prefix, and its textures; or
        /// the error.
        shader: Result<([String; 4], Vec<Texture>), String>,
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
            assert!(
                shader.uniforms.is_empty()
                    && shader.textures_3d.is_empty()
                    && shader.dynamic_properties.is_empty()
            );
            let getter = |key: &str| shader.getters[key].as_str().unwrap().to_string();
            Ok((
                [
                    shader.text.clone(),
                    shader.cache_id.clone(),
                    getter("pixel_name"),
                    getter("resource_prefix"),
                ],
                shader.textures.clone(),
            ))
        }
    };
    Outcome::Extracted {
        gpu_cache_id: result["gpu_cache_id"].as_str().unwrap().to_string(),
        is_no_op: result["gpu_processor"]["isNoOp"].as_bool().unwrap(),
        has_channel_crosstalk: result["gpu_processor"]["hasChannelCrosstalk"]
            .as_bool()
            .unwrap(),
        shader,
    }
}

/// The processor's ops, as `BuildLut1DOp` makes them, before the processor finalizes them.
fn raw_ops(luts: &[Lut]) -> ocio_ops::Result<OpVec> {
    let mut raw = OpVec::new();
    for lut in luts {
        // `BuildLut1DOp`: the transform's data, validated, then copied.
        let data = lut.port()?;
        data.validate()
            .map_err(|e| Exception::new(e.message().to_string()))?;
        create_lut1d_op(&mut raw, data, F);
    }
    Ok(raw)
}

/// A description with the case's settings, set in the oracle's order.
fn shader_desc(settings: &ShaderSettings, language: GpuLanguage) -> GpuShaderDesc {
    let mut desc = GpuShaderDesc::new(language);
    if let Some(p) = &settings.pixel_name {
        desc.set_pixel_name(p);
    }
    if let Some(p) = &settings.resource_prefix {
        desc.set_resource_prefix(p);
    }
    if let Some((index, start)) = settings.descriptor_set {
        desc.set_descriptor_set_index(index, start)
            .expect("a texture binding start");
    }
    if let Some(width) = settings.texture_max_width {
        desc.set_texture_max_width(width);
    }
    if let Some(allowed) = settings.allow_texture_1d {
        desc.set_allow_texture_1d(allowed);
    }
    desc
}

fn port(case: &Case, finalize_first: bool) -> Outcome {
    let mut raw = match raw_ops(&case.luts) {
        Ok(raw) => raw,
        Err(e) => return Outcome::Raised(format!("transform: {}", e.message())),
    };
    if finalize_first && let Err(e) = raw.finalize() {
        return Outcome::Raised(format!("processor: {}", e.message()));
    }
    let gpu = match GpuProcessor::new(&raw, case.flags.1) {
        Ok(gpu) => gpu,
        Err(e) => return Outcome::Raised(format!("gpu_processor: {}", e.message())),
    };
    let mut desc = shader_desc(&case.settings, case.language);
    let shader = gpu
        .extract_gpu_shader_info(&mut desc)
        .map(|()| {
            assert!(desc.num_uniforms() == 0 && desc.num_textures_3d() == 0);
            (
                [
                    utf8(desc.shader_text()),
                    utf8(&desc.cache_id()),
                    utf8(desc.pixel_name()),
                    utf8(desc.resource_prefix()),
                ],
                (0..desc.num_textures())
                    .map(|i| texture(&desc, i))
                    .collect(),
            )
        })
        .map_err(|e| e.message().to_string());
    Outcome::Extracted {
        gpu_cache_id: utf8(gpu.get_cache_id()),
        is_no_op: gpu.is_no_op(),
        has_channel_crosstalk: gpu.has_channel_crosstalk(),
        shader,
    }
}

/// Runs every case through the wheel and the port, and fails with every case that differs;
/// returns how many shaders were compared.
fn check(cases: &[Case]) -> usize {
    let mut failures: Vec<(&Case, Outcome, Outcome)> = Vec::new();
    let mut extracted = 0;
    for chunk in cases.chunks(200) {
        let requests: Vec<GpuShaderRequest> = chunk.iter().map(Case::request).collect();
        let calls: Vec<_> = requests.iter().map(GpuShaderRequest::call).collect();
        for (case, response) in chunk.iter().zip(Oracle::get().batch(&calls, true)) {
            let reply = GpuShaderReply::from_response(
                response.unwrap_or_else(|e| panic!("{}: the oracle failed: {e}", case.label)),
            );
            assert!(reply.log().is_empty(), "{}: {:?}", case.label, reply.log());
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
    }
    if let Some((case, wheel, port)) = failures.first() {
        // The first failure's texts, line by line.
        if let (
            Outcome::Extracted {
                shader: Ok((w, _)), ..
            },
            Outcome::Extracted {
                shader: Ok((p, _)), ..
            },
        ) = (wheel, port)
        {
            assert_text_eq(&case.label, &w[0], &p[0]);
        }
        let labels: Vec<&str> = failures.iter().map(|(c, ..)| c.label.as_str()).collect();
        panic!(
            "{} of {} cases differ ({labels:?}); the first, {}:\n  wheel {wheel:?}\n  port  \
             {port:?}",
            failures.len(),
            cases.len(),
            case.label
        );
    }
    extracted
}

/// The cases: each list of LUTs with each description, at each level, in each language.
fn cases(luts: &[(&str, Vec<Lut>)], settings: &[(&str, ShaderSettings)]) -> Vec<Case> {
    let mut out = Vec::new();
    for (label, list) in luts {
        for (settings_label, s) in settings {
            for flags in levels() {
                for (language, oracle_language) in GpuLanguage::ALL
                    .into_iter()
                    .zip(oracle_gpu::GpuLanguage::ALL)
                {
                    out.push(Case {
                        label: format!(
                            "{label} {settings_label} {} {language:?}",
                            flags.0.unwrap_or("default")
                        ),
                        luts: list.clone(),
                        flags,
                        language,
                        settings: ShaderSettings {
                            language: Some(oracle_language),
                            ..s.clone()
                        },
                    });
                }
            }
        }
    }
    out
}

fn default_settings() -> (&'static str, ShaderSettings) {
    ("defaults", ShaderSettings::default())
}

/// LUTs in one row of a 1D texture, and in a 2D one for the languages without 1D textures:
/// one channel and three, the hue adjustment, the interpolations, NaN and infinite entries.
#[test]
fn short_luts_match_the_wheel() {
    let luts = vec![
        ("identity 2", vec![Lut::new(2, None)]),
        ("powers 17", vec![Lut::new(17, Some(powers))]),
        (
            "powers 17 DW3",
            vec![Lut {
                hue: Lut1DHueAdjust::Dw3,
                ..Lut::new(17, Some(powers))
            }],
        ),
        (
            "identity 9 nearest",
            vec![Lut {
                interpolation: Interpolation::Nearest,
                ..Lut::new(9, None)
            }],
        ),
        (
            "powers 33 default interpolation",
            vec![Lut {
                interpolation: Interpolation::Default,
                ..Lut::new(33, Some(powers))
            }],
        ),
        (
            "specials 8",
            vec![Lut {
                specials: SPECIALS,
                ..Lut::new(8, Some(powers))
            }],
        ),
        (
            "two LUTs",
            vec![Lut::new(5, Some(wide)), Lut::new(17, Some(powers))],
        ),
    ];
    let compared = check(&cases(&luts, &[default_settings()]));
    assert!(compared > 0);
}

/// LUTs longer than the texture width limit, over several rows of a 2D texture; a description
/// without 1D textures; the names (a resource prefix whose double underscores the writer
/// removes) and the descriptor set.
#[test]
fn luts_in_rows_and_descriptions_match_the_wheel() {
    let luts = vec![
        ("identity 17", vec![Lut::new(17, None)]),
        ("powers 17", vec![Lut::new(17, Some(powers))]),
        (
            "powers 40 DW3",
            vec![Lut {
                hue: Lut1DHueAdjust::Dw3,
                ..Lut::new(40, Some(powers))
            }],
        ),
    ];
    let settings = vec![
        default_settings(),
        (
            "width 16",
            ShaderSettings {
                texture_max_width: Some(16),
                ..ShaderSettings::default()
            },
        ),
        (
            "width 13",
            ShaderSettings {
                texture_max_width: Some(13),
                ..ShaderSettings::default()
            },
        ),
        (
            "no 1D textures",
            ShaderSettings {
                allow_texture_1d: Some(false),
                ..ShaderSettings::default()
            },
        ),
        (
            "names",
            ShaderSettings {
                pixel_name: Some("px".into()),
                resource_prefix: Some("my_".into()),
                descriptor_set: Some((2, 5)),
                ..ShaderSettings::default()
            },
        ),
    ];
    let compared = check(&cases(&luts, &settings));
    assert!(compared > 0);
}

/// LUTs as long as the default width limit and longer: 4096 entries in one row, 4097 and 8190
/// in two.
#[test]
fn long_luts_match_the_wheel() {
    let luts = vec![
        ("identity 4096", vec![Lut::new(4096, None)]),
        ("identity 4097", vec![Lut::new(4097, None)]),
        ("powers 4097", vec![Lut::new(4097, Some(powers))]),
        ("identity 8190", vec![Lut::new(8190, None)]),
    ];
    let compared = check(&cases(&luts, &[default_settings()]));
    assert!(compared > 0);
}

/// Half-domain LUTs: a 2D texture with the half-code position helper, in several rows at the
/// default width limit and in one row at wider limits.
#[test]
fn half_domain_luts_match_the_wheel() {
    let luts = vec![
        ("half identity", vec![Lut::half(None)]),
        ("half powers", vec![Lut::half(Some(powers))]),
        (
            "half wide DW3",
            vec![Lut {
                hue: Lut1DHueAdjust::Dw3,
                ..Lut::half(Some(wide))
            }],
        ),
    ];
    // Width limits that hold a half domain in one row: the half domain alone makes the texture
    // 2D (Lut1DOpGPU.cpp:193-199), 65,536 texels by 1.
    let wide = |width: u32| ShaderSettings {
        texture_max_width: Some(width),
        ..ShaderSettings::default()
    };
    let settings = vec![
        default_settings(),
        ("width 65537", wide(65537)),
        ("width 131072", wide(131072)),
    ];
    let compared = check(&cases(&luts, &settings));
    assert!(compared > 0);
}

/// Inverse LUTs: made into their fast forward LUTs, by the optimizer or by the writer; one
/// with values past [0, 1] gets a half domain; a LUT and its inverse.
#[test]
fn inverse_luts_match_the_wheel() {
    let inverse = |lut: Lut| Lut { dir: I, ..lut };
    let luts = vec![
        (
            "inverse powers 17",
            vec![inverse(Lut::new(17, Some(powers)))],
        ),
        ("inverse wide 33", vec![inverse(Lut::new(33, Some(wide)))]),
        (
            "inverse powers 17 DW3",
            vec![inverse(Lut {
                hue: Lut1DHueAdjust::Dw3,
                ..Lut::new(17, Some(powers))
            })],
        ),
        ("inverse half wide", vec![inverse(Lut::half(Some(wide)))]),
        (
            "a LUT and its inverse",
            vec![
                Lut::new(17, Some(powers)),
                inverse(Lut::new(17, Some(powers))),
            ],
        ),
    ];
    let compared = check(&cases(&luts, &[default_settings()]));
    assert!(compared > 0);
}
