// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The FixedFunction op's GPU writer against the wheel (`gpu_shader`), in the 10 languages:
//! for each case, the GPU processor's cache ID and queries, and the shader the extraction
//! writes (its text, cache ID and names), or the error, byte for byte.
//!
//! A `FixedFunctionTransform` builds a FixedFunction op with a validated copy of its data, in
//! its direction (`BuildFixedFunctionOp`, src/OpenColorIO/ops/fixedfunction/
//! FixedFunctionOp.cpp:180-189 @ v2.5.2); a `GroupTransform` builds each child, and the
//! processor finalizes the ops (Processor.cpp:618-641); the GPU processor is then
//! `getOptimizedGPUProcessor` at each level, or `getDefaultGPUProcessor`. The port builds the
//! same ops (`create_fixed_function_op_from_data`), and runs the GPU processor from them as
//! they are, and from them finalized first: both must give the wheel's outcome.
//!
//! The styles are those whose shaders are ported (`fixed_function_op_gpu.rs`): the ACES 1.x
//! ones, forward and inverse.

use ocio_gpu::gpu_processor::GpuProcessor;
use ocio_gpu::gpu_shader::{TextureDimensions, TextureType};
use ocio_gpu::{GpuLanguage, GpuShaderDesc};
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{OptimizationFlags, TransformDirection};
use ocio_ops::ops::fixedfunction::fixed_function_op::create_fixed_function_op_from_data;
use ocio_ops::ops::fixedfunction::fixed_function_op_data::{
    FixedFunctionOpData, FixedFunctionOpStyle,
};
use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;
use ocio_ops::ops::matrix::MatrixOpData;
use ocio_ops::ops::matrix::matrix_op::create_matrix_op;
use ocio_testkit::gpu::{self as oracle_gpu, GpuShaderReply, GpuShaderRequest, ShaderSettings};
use ocio_testkit::{Oracle, assert_text_eq};
use serde_json::{Value, json};

use TransformDirection::{Forward as F, Inverse as I};

/// The ACES 1.3 gamut compression's parameters, as upstream's tests use them
/// (tests/cpu/ops/fixedfunction/FixedFunctionOpCPU_tests.cpp:393 @ v2.5.2).
const GAMUT_COMP_13: [f64; 7] = [1.147, 1.264, 1.312, 0.815, 0.803, 0.880, 1.2];

/// One transform of a case's list.
#[derive(Debug, Clone)]
enum T {
    /// A `FixedFunctionTransform`: its `FIXED_FUNCTION_*` name, the op's forward style, the
    /// parameters and the direction.
    Fixed(
        &'static str,
        FixedFunctionOpStyle,
        Vec<f64>,
        TransformDirection,
    ),
    /// A `MatrixTransform` scaling R, G and B.
    Scale(f64),
}

/// The ACES 1.x styles: the transform's name and the op's forward style.
const STYLES: [(&str, FixedFunctionOpStyle); 6] = [
    (
        "FIXED_FUNCTION_ACES_RED_MOD_03",
        FixedFunctionOpStyle::AcesRedMod03Fwd,
    ),
    (
        "FIXED_FUNCTION_ACES_RED_MOD_10",
        FixedFunctionOpStyle::AcesRedMod10Fwd,
    ),
    (
        "FIXED_FUNCTION_ACES_GLOW_03",
        FixedFunctionOpStyle::AcesGlow03Fwd,
    ),
    (
        "FIXED_FUNCTION_ACES_GLOW_10",
        FixedFunctionOpStyle::AcesGlow10Fwd,
    ),
    (
        "FIXED_FUNCTION_ACES_DARK_TO_DIM_10",
        FixedFunctionOpStyle::AcesDarkToDim10Fwd,
    ),
    (
        "FIXED_FUNCTION_ACES_GAMUT_COMP_13",
        FixedFunctionOpStyle::AcesGamutComp13Fwd,
    ),
];

/// A style of [`STYLES`] with the parameters upstream's tests give it, in `dir`.
fn style(i: usize, dir: TransformDirection) -> T {
    let (name, style) = STYLES[i];
    let params = if style == FixedFunctionOpStyle::AcesGamutComp13Fwd {
        GAMUT_COMP_13.to_vec()
    } else {
        Vec::new()
    };
    T::Fixed(name, style, params, dir)
}

impl T {
    /// The oracle's JSON transform.
    fn spec(&self) -> Value {
        match self {
            T::Fixed(name, _, params, dir) => json!({
                "class": "FixedFunctionTransform",
                "args": {
                    "style": {"enum": name},
                    "params": params,
                    "direction": {"enum": match dir {
                        F => "TRANSFORM_DIR_FORWARD",
                        I => "TRANSFORM_DIR_INVERSE",
                    }},
                },
            }),
            T::Scale(s) => json!({"class": "MatrixTransform", "args": {"matrix": scale(*s)}}),
        }
    }
}

/// A matrix scaling R, G and B by `s`.
fn scale(s: f64) -> [f64; 16] {
    [
        s, 0.0, 0.0, 0.0, 0.0, s, 0.0, 0.0, 0.0, 0.0, s, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

/// The optimization levels, as the oracle takes them; `None` is `getDefaultGPUProcessor`.
fn levels() -> Vec<(Option<Value>, OptimizationFlags)> {
    let name = |n: &str| Some(json!(n));
    vec![
        (None, OptimizationFlags::DEFAULT),
        (name("OPTIMIZATION_NONE"), OptimizationFlags::NONE),
        (name("OPTIMIZATION_LOSSLESS"), OptimizationFlags::LOSSLESS),
        (name("OPTIMIZATION_DEFAULT"), OptimizationFlags::DEFAULT),
        (name("OPTIMIZATION_ALL"), OptimizationFlags::ALL),
    ]
}

/// The shader description's names; `None` keeps the default.
#[derive(Debug, Clone, Copy, Default)]
struct Names {
    pixel: Option<&'static str>,
    prefix: Option<&'static str>,
    /// `setDescriptorSetIndex(index, textureBindingStart)`.
    descriptor_set: Option<(u32, u32)>,
    /// `setAllowTexture1D`.
    allow_texture_1d: Option<bool>,
}

/// One extraction.
#[derive(Debug, Clone)]
struct Case {
    label: String,
    items: Vec<T>,
    flags: (Option<Value>, OptimizationFlags),
    language: GpuLanguage,
    oracle_language: oracle_gpu::GpuLanguage,
    names: Names,
}

impl Case {
    fn request(&self) -> GpuShaderRequest {
        let children: Vec<Value> = self.items.iter().map(T::spec).collect();
        let mut processor = json!({"transform": {"class": "GroupTransform", "children": children}});
        if let Some(flags) = &self.flags.0 {
            processor["optimization"] = flags.clone();
        }
        GpuShaderRequest::new(
            processor,
            ShaderSettings {
                language: Some(self.oracle_language),
                pixel_name: self.names.pixel.map(String::from),
                resource_prefix: self.names.prefix.map(String::from),
                descriptor_set: self.names.descriptor_set,
                allow_texture_1d: self.names.allow_texture_1d,
                ..ShaderSettings::default()
            },
        )
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
        /// The shader's text, cache ID, pixel name and resource prefix, and its textures, or
        /// the error.
        shader: Result<([String; 4], Vec<Tex>), String>,
    },
}

/// A texture of the shader, its values by their bits.
#[derive(Debug, Clone, PartialEq)]
struct Tex {
    name: String,
    sampler_name: String,
    width: u64,
    height: u64,
    channel: String,
    dimensions: String,
    interpolation: String,
    binding_index: u64,
    values: Vec<u32>,
}

impl From<&oracle_gpu::Texture> for Tex {
    fn from(t: &oracle_gpu::Texture) -> Tex {
        Tex {
            name: t.name.clone(),
            sampler_name: t.sampler_name.clone(),
            width: t.width,
            height: t.height,
            channel: t.channel.clone(),
            dimensions: t.dimensions.clone(),
            interpolation: t.interpolation.clone(),
            binding_index: t.binding_index,
            values: t.values.iter().map(|v| v.to_bits()).collect(),
        }
    }
}

/// The port's texture `index`, as the oracle reports one.
fn port_texture(desc: &GpuShaderDesc, index: u32) -> Tex {
    let utf8 = |b: &[u8]| String::from_utf8(b.to_vec()).unwrap();
    let t = desc.texture(index).unwrap();
    Tex {
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
        interpolation: match t.interpolation() {
            Interpolation::Nearest => "INTERP_NEAREST",
            other => panic!("interpolation {other:?}"),
        }
        .into(),
        binding_index: u64::from(desc.texture_shader_binding_index(index).unwrap()),
        values: t.values().iter().map(|v| v.to_bits()).collect(),
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
            assert!(shader.uniforms.is_empty() && shader.textures_3d.is_empty());
            let getter = |key: &str| shader.getters[key].as_str().unwrap().to_string();
            Ok((
                [
                    shader.text.clone(),
                    shader.cache_id.clone(),
                    getter("pixel_name"),
                    getter("resource_prefix"),
                ],
                shader.textures.iter().map(Tex::from).collect(),
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

/// The processor's ops, as `BuildFixedFunctionOp` and `BuildMatrixOp` make them, before the
/// processor finalizes them. A refusal is the transform's.
fn raw_ops(items: &[T]) -> ocio_ops::Result<OpVec> {
    let mut raw = OpVec::new();
    for item in items {
        match item {
            T::Fixed(_, style, params, dir) => {
                let data = FixedFunctionOpData::with_params(*style, params.clone())?;
                data.validate()?;
                create_fixed_function_op_from_data(&mut raw, data, *dir)?;
            }
            T::Scale(s) => {
                let mut data = MatrixOpData::new();
                data.set_rgba(&scale(*s));
                data.validate()?;
                create_matrix_op(&mut raw, data, F);
            }
        }
    }
    Ok(raw)
}

fn port(case: &Case, finalize_first: bool) -> Outcome {
    let text = |b: &[u8]| String::from_utf8(b.to_vec()).unwrap();
    let mut raw = match raw_ops(&case.items) {
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
    let mut desc = GpuShaderDesc::new(case.language);
    if let Some(p) = case.names.pixel {
        desc.set_pixel_name(p);
    }
    if let Some(p) = case.names.prefix {
        desc.set_resource_prefix(p);
    }
    if let Some((index, start)) = case.names.descriptor_set {
        desc.set_descriptor_set_index(index, start).unwrap();
    }
    if let Some(allowed) = case.names.allow_texture_1d {
        desc.set_allow_texture_1d(allowed);
    }
    let shader = gpu
        .extract_gpu_shader_info(&mut desc)
        .map(|()| {
            assert!(desc.num_uniforms() == 0 && desc.num_textures_3d() == 0);
            (
                [
                    text(desc.shader_text()),
                    text(&desc.cache_id()),
                    text(desc.pixel_name()),
                    text(desc.resource_prefix()),
                ],
                (0..desc.num_textures())
                    .map(|i| port_texture(&desc, i))
                    .collect(),
            )
        })
        .map_err(|e| e.message().to_string());
    Outcome::Extracted {
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
            assert_text_eq(&case.label, &w.0[0], &p.0[0]);
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

/// `items` at the levels `flags`, in every language, with `names`.
fn cases_of(
    label: &str,
    items: Vec<T>,
    flags: &[(Option<Value>, OptimizationFlags)],
    names: Names,
) -> Vec<Case> {
    let mut cases = Vec::new();
    for flags in flags {
        for (language, oracle_language) in GpuLanguage::ALL
            .into_iter()
            .zip(oracle_gpu::GpuLanguage::ALL)
        {
            cases.push(Case {
                label: format!("{label} {:?} {language:?} {names:?}", flags.0),
                items: items.clone(),
                flags: flags.clone(),
                language,
                oracle_language,
                names,
            });
        }
    }
    cases
}

/// Every ACES 1.x style forward and inverse, at every level, in every language; an inverse
/// pair the optimizer removes, and one around a matrix; other names, and an empty pixel name.
#[test]
fn aces_1_shaders_match_the_wheel() {
    let mut cases = Vec::new();
    for (i, (name, _)) in STYLES.iter().enumerate() {
        for dir in [F, I] {
            cases.extend(cases_of(
                &format!("{name} {dir:?}"),
                vec![style(i, dir)],
                &levels(),
                Names::default(),
            ));
        }
    }
    cases.extend(cases_of(
        "glow 1.0 and its inverse",
        vec![style(3, F), style(3, I)],
        &levels(),
        Names::default(),
    ));
    cases.extend(cases_of(
        "red modifier 0.3 around a matrix",
        vec![style(0, F), T::Scale(2.0), style(0, I)],
        &levels(),
        Names::default(),
    ));
    for names in [
        Names {
            pixel: Some("px"),
            prefix: Some("p__q"),
            ..Names::default()
        },
        Names {
            pixel: Some(""),
            ..Names::default()
        },
    ] {
        for i in 0..STYLES.len() {
            cases.extend(cases_of("names", vec![style(i, I)], &levels()[1..2], names));
        }
    }
    check(&cases);
}

/// The ACES 1.3 gamut compression with other parameters: validation's bounds and values
/// between them, forward and inverse, without optimization, in every language.
#[test]
fn gamut_comp_13_shaders_match_the_wheel() {
    let mut cases = Vec::new();
    for (label, params) in [
        ("lower bounds", [1.001, 1.001, 1.001, 0.0, 0.0, 0.0, 1.0]),
        (
            "upper bounds",
            [65504.0, 65504.0, 65504.0, 0.9995, 0.9995, 0.9995, 65504.0],
        ),
        ("mixed", [1.5, 2.0, 1.01, 0.5, 0.9, 0.2, 3.0]),
        (
            "many digits",
            [
                1.123456789,
                1.987654321,
                3.3,
                0.111111111,
                0.5,
                0.75,
                1.333333333,
            ],
        ),
    ] {
        for dir in [F, I] {
            cases.extend(cases_of(
                &format!("gamut compression 1.3 {label} {dir:?}"),
                vec![T::Fixed(STYLES[5].0, STYLES[5].1, params.to_vec(), dir)],
                &levels()[1..2],
                Names::default(),
            ));
        }
    }
    check(&cases);
}

/// The Rec.2100 surround, RGB to and from HSV and the three HSYs, and XYZ to and from xyY,
/// u'v'Y and CIELUV (chunk 2.3g1): the transform's name, the op's forward style and the
/// parameters (upstream's tests' surround gammas, tests/cpu/ops/fixedfunction/
/// FixedFunctionOpCPU_tests.cpp:996, 1030 @ v2.5.2).
const G1_STYLES: [(&str, FixedFunctionOpStyle, &[f64]); 9] = [
    (
        "FIXED_FUNCTION_REC2100_SURROUND",
        FixedFunctionOpStyle::Rec2100SurroundFwd,
        &[0.78],
    ),
    (
        "FIXED_FUNCTION_REC2100_SURROUND",
        FixedFunctionOpStyle::Rec2100SurroundFwd,
        &[1.2],
    ),
    (
        "FIXED_FUNCTION_RGB_TO_HSV",
        FixedFunctionOpStyle::RgbToHsv,
        &[],
    ),
    (
        "FIXED_FUNCTION_RGB_TO_HSY_LIN",
        FixedFunctionOpStyle::RgbToHsyLin,
        &[],
    ),
    (
        "FIXED_FUNCTION_RGB_TO_HSY_LOG",
        FixedFunctionOpStyle::RgbToHsyLog,
        &[],
    ),
    (
        "FIXED_FUNCTION_RGB_TO_HSY_VID",
        FixedFunctionOpStyle::RgbToHsyVid,
        &[],
    ),
    (
        "FIXED_FUNCTION_XYZ_TO_xyY",
        FixedFunctionOpStyle::XyzToXyy,
        &[],
    ),
    (
        "FIXED_FUNCTION_XYZ_TO_uvY",
        FixedFunctionOpStyle::XyzToUvy,
        &[],
    ),
    (
        "FIXED_FUNCTION_XYZ_TO_LUV",
        FixedFunctionOpStyle::XyzToLuv,
        &[],
    ),
];

/// Every style of [`G1_STYLES`] forward and inverse, at every level, in every language; a
/// surround and its inverse, which the optimizer removes; other names.
#[test]
fn surround_hsv_hsy_and_cie_shaders_match_the_wheel() {
    let fixed = |i: usize, dir| {
        let (name, style, params) = G1_STYLES[i];
        T::Fixed(name, style, params.to_vec(), dir)
    };
    let mut cases = Vec::new();
    for (i, (name, _, params)) in G1_STYLES.iter().enumerate() {
        for dir in [F, I] {
            cases.extend(cases_of(
                &format!("{name} {params:?} {dir:?}"),
                vec![fixed(i, dir)],
                &levels(),
                Names::default(),
            ));
        }
    }
    cases.extend(cases_of(
        "surround and its inverse",
        vec![fixed(0, F), fixed(0, I)],
        &levels(),
        Names::default(),
    ));
    for i in 0..G1_STYLES.len() {
        cases.extend(cases_of(
            "names",
            vec![fixed(i, I)],
            &levels()[1..2],
            Names {
                pixel: Some("px"),
                prefix: Some("p__q"),
                ..Names::default()
            },
        ));
    }
    check(&cases);
}

/// The Rec.2100 HLG curve as a gamma-log, and a double log (tests/cpu/ops/fixedfunction/
/// FixedFunctionOpCPU_tests.cpp:1311-1325, 1374-1382 @ v2.5.2).
const HLG: [f64; 10] = [
    0.0,
    0.25,
    0.5,
    1.0,
    0.0,
    std::f64::consts::E,
    0.17883277,
    0.807825590164,
    1.0,
    -0.07116723,
];
const DOUBLE_LOG: [f64; 13] = [
    10.0, 0.25, 0.5, -1.0, 0.0, -1.0, 1.25, 1.0, 1.0, 1.0, 0.5, 1.0, 0.0,
];

/// PQ, the gamma-log and the double-log curves (chunk 2.3g2), forward and inverse, at every
/// level, in every language: PQ, the HLG curve and one with a gamma segment offset (which the
/// shader subtracts, I-85), and the double log with other parameters; other names.
#[test]
fn pq_gamma_log_and_double_log_shaders_match_the_wheel() {
    let mut offset = HLG;
    offset[4] = 0.125;
    let mut other_double = DOUBLE_LOG;
    other_double[0] = 2.0;
    other_double[11] = 1.5;
    other_double[12] = 0.25;
    let styles: Vec<(&str, &str, FixedFunctionOpStyle, Vec<f64>)> = vec![
        (
            "PQ",
            "FIXED_FUNCTION_LIN_TO_PQ",
            FixedFunctionOpStyle::LinToPq,
            Vec::new(),
        ),
        (
            "HLG",
            "FIXED_FUNCTION_LIN_TO_GAMMA_LOG",
            FixedFunctionOpStyle::LinToGammaLog,
            HLG.to_vec(),
        ),
        (
            "gamma offset",
            "FIXED_FUNCTION_LIN_TO_GAMMA_LOG",
            FixedFunctionOpStyle::LinToGammaLog,
            offset.to_vec(),
        ),
        (
            "double log",
            "FIXED_FUNCTION_LIN_TO_DOUBLE_LOG",
            FixedFunctionOpStyle::LinToDoubleLog,
            DOUBLE_LOG.to_vec(),
        ),
        (
            "other double log",
            "FIXED_FUNCTION_LIN_TO_DOUBLE_LOG",
            FixedFunctionOpStyle::LinToDoubleLog,
            other_double.to_vec(),
        ),
    ];
    let mut cases = Vec::new();
    for (label, name, style, params) in &styles {
        for dir in [F, I] {
            cases.extend(cases_of(
                &format!("{label} {dir:?}"),
                vec![T::Fixed(name, *style, params.clone(), dir)],
                &levels(),
                Names::default(),
            ));
        }
        cases.extend(cases_of(
            &format!("{label} names"),
            vec![T::Fixed(name, *style, params.clone(), I)],
            &levels()[1..2],
            Names {
                pixel: Some("px"),
                prefix: Some("p__q"),
                ..Names::default()
            },
        ));
    }
    check(&cases);
}

/// The ACES 2.0 outputs' limiting primaries: Rec.709, P3-D65, Rec.2020, P3-DCI, P3-D60.
const ACES2_PRIMARIES: [[f64; 8]; 5] = [
    [0.64, 0.33, 0.30, 0.60, 0.15, 0.06, 0.3127, 0.3290],
    [0.680, 0.320, 0.265, 0.690, 0.150, 0.060, 0.3127, 0.3290],
    [0.708, 0.292, 0.170, 0.797, 0.131, 0.046, 0.3127, 0.3290],
    [0.680, 0.320, 0.265, 0.690, 0.150, 0.060, 0.314, 0.351],
    [0.680, 0.320, 0.265, 0.690, 0.150, 0.060, 0.32168, 0.33767],
];

/// ACES2065-1's primaries (AP0), as upstream's aces_rgb_to_jmh_20 gives them
/// (tests/cpu/ops/fixedfunction/FixedFunctionOpCPU_tests.cpp:773 @ v2.5.2), and AP1.
const AP0: [f64; 8] = [
    0.7347, 0.2653, 0.0000, 1.0000, 0.0001, -0.0770, 0.32168, 0.33767,
];
const AP1: [f64; 8] = [0.713, 0.293, 0.165, 0.830, 0.128, 0.044, 0.32168, 0.33767];

/// The peak luminances of the ACES 2.0 cases.
const PEAKS: [f64; 6] = [1000.0, 100.0, 4000.0, 108.0, 48.0, 500.0];

/// The settings of the ACES 2.0 cases besides the defaults: names, a descriptor set (the
/// textures' bindings in GLSL for Vulkan), and 2D textures where 1D ones aren't allowed.
fn aces2_settings() -> [Names; 3] {
    [
        Names {
            pixel: Some("px"),
            prefix: Some("p__q"),
            ..Names::default()
        },
        Names {
            descriptor_set: Some((2, 3)),
            ..Names::default()
        },
        Names {
            allow_texture_1d: Some(false),
            ..Names::default()
        },
    ]
}

/// RGB to and from ACES 2.0's JMh (chunk 2.4f1) for AP0, AP1 and the outputs' primaries, at
/// every level, in every language; the shader's settings.
#[test]
fn aces_2_rgb_to_jmh_shaders_match_the_wheel() {
    let style = |p: &[f64; 8], dir| {
        T::Fixed(
            "FIXED_FUNCTION_ACES_RGB_TO_JMH_20",
            FixedFunctionOpStyle::AcesRgbToJmh20,
            p.to_vec(),
            dir,
        )
    };
    let mut cases = Vec::new();
    for (i, p) in [AP0, AP1].iter().chain(&ACES2_PRIMARIES).enumerate() {
        for dir in [F, I] {
            cases.extend(cases_of(
                &format!("RGB to JMh {i} {dir:?}"),
                vec![style(p, dir)],
                &levels()[if i == 0 { 0..5 } else { 1..2 }],
                Names::default(),
            ));
        }
    }
    for names in aces2_settings() {
        for dir in [F, I] {
            cases.extend(cases_of(
                "RGB to JMh settings",
                vec![style(&AP0, dir)],
                &levels()[1..2],
                names,
            ));
        }
    }
    check(&cases);
}

/// ACES 2.0's tone scale and chroma compression (chunk 2.4f2) for six peak luminances, both
/// ways, at every level, in every language; two in one shader (each with its own resources);
/// the shader's settings.
#[test]
fn aces_2_tonescale_compress_shaders_match_the_wheel() {
    let style = |peak: f64, dir| {
        T::Fixed(
            "FIXED_FUNCTION_ACES_TONESCALE_COMPRESS_20",
            FixedFunctionOpStyle::AcesTonescaleCompress20Fwd,
            vec![peak],
            dir,
        )
    };
    let mut cases = Vec::new();
    for (i, peak) in PEAKS.iter().enumerate() {
        for dir in [F, I] {
            cases.extend(cases_of(
                &format!("tone scale {peak} {dir:?}"),
                vec![style(*peak, dir)],
                &levels()[if i == 0 { 0..5 } else { 1..2 }],
                Names::default(),
            ));
        }
    }
    cases.extend(cases_of(
        "two tone scales",
        vec![style(1000.0, F), style(100.0, I)],
        &levels()[1..2],
        Names::default(),
    ));
    for names in aces2_settings() {
        for dir in [F, I] {
            cases.extend(cases_of(
                "tone scale settings",
                vec![style(1000.0, dir)],
                &levels()[1..2],
                names,
            ));
        }
    }
    check(&cases);
}

/// A peak luminance and limiting primaries, as the ACES 2.0 output transform and gamut
/// compression take them.
fn peak_and(peak: f64, primaries: &[f64; 8]) -> Vec<f64> {
    std::iter::once(peak)
        .chain(primaries.iter().copied())
        .collect()
}

/// The peak luminances and limiting primaries (indices into [`ACES2_PRIMARIES`]) of the ACES
/// 2.0 output transform and gamut compression cases: upstream's tests' 1000 nits P3-D65 first.
const PEAK_PRIMARIES: [(f64, usize); 6] = [
    (1000.0, 1),
    (100.0, 0),
    (4000.0, 2),
    (108.0, 3),
    (48.0, 4),
    (500.0, 0),
];

/// ACES 2.0's gamut compression (chunk 2.4g) for six peak luminances and five limiting gamuts,
/// both ways, at every level, in every language; the shader's settings.
#[test]
fn aces_2_gamut_compress_shaders_match_the_wheel() {
    let style = |peak: f64, primaries: usize, dir| {
        T::Fixed(
            "FIXED_FUNCTION_ACES_GAMUT_COMPRESS_20",
            FixedFunctionOpStyle::AcesGamutCompress20Fwd,
            peak_and(peak, &ACES2_PRIMARIES[primaries]),
            dir,
        )
    };
    let mut cases = Vec::new();
    for (i, (peak, primaries)) in PEAK_PRIMARIES.iter().enumerate() {
        for dir in [F, I] {
            cases.extend(cases_of(
                &format!("gamut compression {peak} {primaries} {dir:?}"),
                vec![style(*peak, *primaries, dir)],
                &levels()[if i == 0 { 0..5 } else { 1..2 }],
                Names::default(),
            ));
        }
    }
    for names in aces2_settings() {
        for dir in [F, I] {
            cases.extend(cases_of(
                "gamut compression settings",
                vec![style(1000.0, 1, dir)],
                &levels()[1..2],
                names,
            ));
        }
    }
    check(&cases);
}

/// ACES 2.0's output transform (chunk 2.4h) for six peak luminances and five limiting gamuts,
/// both ways, at every level, in every language; a transform and its inverse (each with its own
/// resources); the shader's settings.
#[test]
fn aces_2_output_transform_shaders_match_the_wheel() {
    let style = |peak: f64, primaries: usize, dir| {
        T::Fixed(
            "FIXED_FUNCTION_ACES_OUTPUT_TRANSFORM_20",
            FixedFunctionOpStyle::AcesOutputTransform20Fwd,
            peak_and(peak, &ACES2_PRIMARIES[primaries]),
            dir,
        )
    };
    let mut cases = Vec::new();
    for (i, (peak, primaries)) in PEAK_PRIMARIES.iter().enumerate() {
        for dir in [F, I] {
            cases.extend(cases_of(
                &format!("output transform {peak} {primaries} {dir:?}"),
                vec![style(*peak, *primaries, dir)],
                &levels()[if i == 0 { 0..5 } else { 1..2 }],
                Names::default(),
            ));
        }
    }
    cases.extend(cases_of(
        "output transform and its inverse",
        vec![style(1000.0, 1, F), style(1000.0, 1, I)],
        &levels(),
        Names::default(),
    ));
    for names in aces2_settings() {
        for dir in [F, I] {
            cases.extend(cases_of(
                "output transform settings",
                vec![style(1000.0, 1, dir)],
                &levels()[1..2],
                names,
            ));
        }
    }
    check(&cases);
}
