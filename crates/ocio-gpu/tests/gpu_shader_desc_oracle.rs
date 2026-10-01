// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The port's [`GpuShaderDesc`] against the wheel's, call by call: each case drives both
//! through the same calls (the oracle's `gpu_shader_desc`), and every call's outcome and the
//! description's final state must agree, byte for byte and bit for bit.

use std::sync::Arc;

use ocio_gpu::gpu_shader::{TextureDimensions, TextureType};
use ocio_gpu::{GpuLanguage, GpuShaderDesc, UniformDataType};
use ocio_ops::Exception;
use ocio_ops::dynamic_property::{DynamicPropertyDoubleImpl, DynamicPropertyRcPtr};
use ocio_ops::open_color_types::DynamicPropertyType;
use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;
use ocio_testkit::Oracle;
use ocio_testkit::gpu::{
    self as oracle_gpu, GpuShaderReply, GpuShaderRequest, ShaderSettings, Texture, Texture3d,
};
use ocio_testkit::gpu_desc::{
    DescCall, DescOutcome, GpuShaderDescReply, GpuShaderDescRequest, Returned,
};

use DescCall::*;

/// The port's language for the oracle's.
fn language(l: oracle_gpu::GpuLanguage) -> GpuLanguage {
    let i = oracle_gpu::GpuLanguage::ALL
        .iter()
        .position(|&x| x == l)
        .expect("a language");
    GpuLanguage::ALL[i]
}

fn interpolation(name: &str) -> Interpolation {
    match name {
        "INTERP_UNKNOWN" => Interpolation::Unknown,
        "INTERP_NEAREST" => Interpolation::Nearest,
        "INTERP_LINEAR" => Interpolation::Linear,
        "INTERP_TETRAHEDRAL" => Interpolation::Tetrahedral,
        "INTERP_CUBIC" => Interpolation::Cubic,
        "INTERP_DEFAULT" => Interpolation::Default,
        "INTERP_BEST" => Interpolation::Best,
        _ => panic!("interpolation {name}"),
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

fn channel(name: &str) -> TextureType {
    match name {
        "TEXTURE_RED_CHANNEL" => TextureType::RedChannel,
        "TEXTURE_RGB_CHANNEL" => TextureType::RgbChannel,
        _ => panic!("channel {name}"),
    }
}

fn channel_name(c: TextureType) -> &'static str {
    match c {
        TextureType::RedChannel => "TEXTURE_RED_CHANNEL",
        TextureType::RgbChannel => "TEXTURE_RGB_CHANNEL",
    }
}

fn dimensions(name: &str) -> TextureDimensions {
    match name {
        "TEXTURE_1D" => TextureDimensions::D1,
        "TEXTURE_2D" => TextureDimensions::D2,
        _ => panic!("dimensions {name}"),
    }
}

/// The PyOpenColorIO names of the dynamic property types, in the enumerators' order.
const PROPERTY_TYPES: [(&str, DynamicPropertyType); 7] = [
    ("DYNAMIC_PROPERTY_EXPOSURE", DynamicPropertyType::Exposure),
    ("DYNAMIC_PROPERTY_CONTRAST", DynamicPropertyType::Contrast),
    ("DYNAMIC_PROPERTY_GAMMA", DynamicPropertyType::Gamma),
    (
        "DYNAMIC_PROPERTY_GRADING_PRIMARY",
        DynamicPropertyType::GradingPrimary,
    ),
    (
        "DYNAMIC_PROPERTY_GRADING_RGBCURVE",
        DynamicPropertyType::GradingRgbCurve,
    ),
    (
        "DYNAMIC_PROPERTY_GRADING_TONE",
        DynamicPropertyType::GradingTone,
    ),
    (
        "DYNAMIC_PROPERTY_GRADING_HUECURVE",
        DynamicPropertyType::GradingHueCurve,
    ),
];

fn property_type(name: &str) -> DynamicPropertyType {
    PROPERTY_TYPES
        .iter()
        .find(|(n, _)| *n == name)
        .unwrap_or_else(|| panic!("dynamic property type {name}"))
        .1
}

fn property_type_name(t: DynamicPropertyType) -> &'static str {
    PROPERTY_TYPES
        .iter()
        .find(|(_, x)| *x == t)
        .expect("a dynamic property type")
        .0
}

fn dimensions_name(d: TextureDimensions) -> &'static str {
    match d {
        TextureDimensions::D1 => "TEXTURE_1D",
        TextureDimensions::D2 => "TEXTURE_2D",
    }
}

/// The text of a name or code, which the oracle reports as UTF-8.
fn utf8(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("UTF-8")
}

/// The oracle's outcome of an error.
fn raised(e: Exception) -> DescOutcome {
    DescOutcome::Raised(oracle_gpu::Raised {
        kind: "Exception".into(),
        message: e.message().into(),
        stage: String::new(),
    })
}

fn texture(desc: &GpuShaderDesc, index: u32) -> Result<Texture, Exception> {
    let t = desc.texture(index)?;
    Ok(Texture {
        name: utf8(t.texture_name()),
        sampler_name: utf8(t.sampler_name()),
        width: u64::from(t.width()),
        height: u64::from(t.height()),
        channel: channel_name(t.channel()).into(),
        dimensions: dimensions_name(t.dimensions()).into(),
        interpolation: interpolation_name(t.interpolation()).into(),
        binding_index: u64::from(desc.texture_shader_binding_index(index)?),
        values: t.values().to_vec(),
    })
}

fn texture_3d(desc: &GpuShaderDesc, index: u32) -> Result<Texture3d, Exception> {
    let t = desc.texture_3d(index)?;
    Ok(Texture3d {
        name: utf8(t.texture_name()),
        sampler_name: utf8(t.sampler_name()),
        edge_len: u64::from(t.edge_len()),
        interpolation: interpolation_name(t.interpolation()).into(),
        binding_index: u64::from(desc.texture_3d_shader_binding_index(index)?),
        values: t.values().to_vec(),
    })
}

/// Runs `calls` on a new port description, as the command runs them on the wheel's.
fn replay(calls: &[DescCall]) -> (Vec<DescOutcome>, GpuShaderDesc) {
    let mut desc = GpuShaderDesc::default();
    let mut outcomes = Vec::new();
    let nothing = || DescOutcome::Returned(Returned::Nothing);
    let text = |b: Vec<u8>| DescOutcome::Returned(Returned::Text(utf8(&b)));
    let number = |n: u32| DescOutcome::Returned(Returned::Number(u64::from(n)));
    for call in calls {
        let outcome = match call {
            SetUniqueId(s) => {
                desc.set_unique_id(s);
                nothing()
            }
            SetLanguage(l) => {
                desc.set_language(language(*l));
                nothing()
            }
            SetFunctionName(s) => {
                desc.set_function_name(s);
                nothing()
            }
            SetPixelName(s) => {
                desc.set_pixel_name(s);
                nothing()
            }
            SetResourcePrefix(s) => {
                desc.set_resource_prefix(s);
                nothing()
            }
            SetDescriptorSetIndex(i, start) => match desc.set_descriptor_set_index(*i, *start) {
                Ok(()) => nothing(),
                Err(e) => raised(e),
            },
            SetTextureMaxWidth(w) => {
                desc.set_texture_max_width(*w);
                nothing()
            }
            SetAllowTexture1d(b) => {
                desc.set_allow_texture_1d(*b);
                nothing()
            }
            GetNextResourceIndex => number(desc.next_resource_index()),
            GetCacheId => text(desc.cache_id()),
            Begin(s) => {
                desc.begin(s);
                nothing()
            }
            End => {
                desc.end();
                nothing()
            }
            AddToParameterDeclareShaderCode(s) => {
                desc.add_to_parameter_declare_shader_code(s);
                nothing()
            }
            AddToTextureDeclareShaderCode(s) => {
                desc.add_to_texture_declare_shader_code(s);
                nothing()
            }
            AddToHelperShaderCode(s) => {
                desc.add_to_helper_shader_code(s);
                nothing()
            }
            AddToFunctionHeaderShaderCode(s) => {
                desc.add_to_function_header_shader_code(s);
                nothing()
            }
            AddToFunctionShaderCode(s) => {
                desc.add_to_function_shader_code(s);
                nothing()
            }
            AddToFunctionFooterShaderCode(s) => {
                desc.add_to_function_footer_shader_code(s);
                nothing()
            }
            CreateShaderText([a, b, c, d, e, f]) => {
                desc.create_shader_text(a, b, c, d, e, f);
                nothing()
            }
            Finalize => match desc.finalize() {
                Ok(()) => nothing(),
                Err(e) => raised(e),
            },
            GetShaderText => text(desc.shader_text().to_vec()),
            AddTexture {
                name,
                sampler_name,
                width,
                height,
                channel: c,
                dimensions: d,
                interpolation: i,
                values,
            } => match desc.add_texture(
                name,
                sampler_name,
                *width,
                *height,
                channel(c),
                dimensions(d),
                interpolation(i),
                values,
            ) {
                Ok(index) => number(index),
                Err(e) => raised(e),
            },
            Add3dTexture {
                name,
                sampler_name,
                edge_len,
                interpolation: i,
                values,
            } => match desc.add_3d_texture(name, sampler_name, *edge_len, interpolation(i), values)
            {
                Ok(index) => number(index),
                Err(e) => raised(e),
            },
            GetTexture(i) => match texture(&desc, *i) {
                Ok(t) => DescOutcome::Returned(Returned::Texture(t)),
                Err(e) => raised(e),
            },
            Get3dTexture(i) => match texture_3d(&desc, *i) {
                Ok(t) => DescOutcome::Returned(Returned::Texture3d(t)),
                Err(e) => raised(e),
            },
            GetNumDynamicProperties => number(desc.num_dynamic_properties()),
            GetDynamicProperty(t) => match desc.dynamic_property_by_type(property_type(t)) {
                Ok(p) => DescOutcome::Returned(Returned::DynamicProperty(
                    property_type_name(p.get_type()).into(),
                )),
                Err(e) => raised(e),
            },
            GetDynamicPropertyAt(i) => match desc.dynamic_property(*i) {
                Ok(p) => DescOutcome::Returned(Returned::DynamicProperty(
                    property_type_name(p.get_type()).into(),
                )),
                Err(e) => raised(e),
            },
            Clone => {
                desc = desc.clone_desc();
                nothing()
            }
        };
        outcomes.push(outcome);
    }
    (outcomes, desc)
}

/// Checks the port's outcomes and final state against the wheel's reply.
fn check(label: &str, calls: &[DescCall], reply: &GpuShaderDescReply) {
    let (outcomes, desc) = replay(calls);
    assert_eq!(outcomes.len(), reply.calls.len(), "{label}");
    for (i, (port, wheel)) in outcomes.iter().zip(&reply.calls).enumerate() {
        assert_eq!(port, wheel, "{label}: call {i}, {:?}", calls[i]);
    }

    // The final state, as the command reads it: the cache ID last fills it, as here.
    let shader = &reply.shader;
    let getters = &shader.getters;
    let names = [
        ("function_name", desc.function_name()),
        ("pixel_name", desc.pixel_name()),
        ("resource_prefix", desc.resource_prefix()),
        ("uid", desc.unique_id()),
    ];
    for (key, value) in names {
        assert_eq!(
            getters[key].as_str(),
            Some(utf8(value).as_str()),
            "{label}: {key}"
        );
    }
    let language_name = oracle_gpu::GpuLanguage::ALL[GpuLanguage::ALL
        .iter()
        .position(|&l| l == desc.language())
        .expect("a language")]
    .oracle_name();
    assert_eq!(getters["language"], language_name, "{label}");
    let numbers = [
        (
            "descriptor_set_index",
            u64::from(desc.descriptor_set_index()),
        ),
        (
            "texture_binding_start",
            u64::from(desc.texture_binding_start()),
        ),
        ("texture_max_width", u64::from(desc.texture_max_width())),
        ("uniform_buffer_size", desc.uniform_buffer_size() as u64),
    ];
    for (key, value) in numbers {
        assert_eq!(getters[key].as_u64(), Some(value), "{label}: {key}");
    }
    assert_eq!(
        getters["allow_texture_1d"].as_bool(),
        Some(desc.allow_texture_1d()),
        "{label}"
    );
    assert_eq!(utf8(desc.shader_text()), shader.text, "{label}: the text");
    let textures: Vec<Texture> = (0..desc.num_textures())
        .map(|i| texture(&desc, i).expect("a texture"))
        .collect();
    assert_eq!(textures, shader.textures, "{label}: the textures");
    let textures_3d: Vec<Texture3d> = (0..desc.num_textures_3d())
        .map(|i| texture_3d(&desc, i).expect("a 3D texture"))
        .collect();
    assert_eq!(textures_3d, shader.textures_3d, "{label}: the 3D textures");
    assert!(shader.uniforms.is_empty() && shader.dynamic_properties.is_empty());
    assert_eq!(
        utf8(&desc.cache_id()),
        shader.cache_id,
        "{label}: the cache ID"
    );
}

/// A red texture of `width * height` values.
fn red(name: &str, width: u32, height: u32) -> DescCall {
    AddTexture {
        name: name.into(),
        sampler_name: format!("{name}Sampler"),
        width,
        height,
        channel: "TEXTURE_RED_CHANNEL".into(),
        dimensions: "TEXTURE_1D".into(),
        interpolation: "INTERP_LINEAR".into(),
        values: (0..width * height).map(|i| i as f32 * 0.25 - 1.0).collect(),
    }
}

/// The six code sections, then the text and the cache ID, after a finalize and after a
/// `createShaderText` of other sections.
fn code(language: oracle_gpu::GpuLanguage, declarations: &str) -> Vec<DescCall> {
    vec![
        SetLanguage(language),
        SetFunctionName("F__1".into()),
        AddToParameterDeclareShaderCode("float p;\n".into()),
        AddToTextureDeclareShaderCode(declarations.into()),
        AddToHelperShaderCode("float h() { return 1.; }\n".into()),
        AddToFunctionHeaderShaderCode("float4 F_1(float4 inPixel)\n{\n".into()),
        AddToFunctionShaderCode("  inPixel.r = h();\n".into()),
        AddToFunctionFooterShaderCode("  return inPixel;\n}\n".into()),
        Finalize,
        GetShaderText,
        GetCacheId,
        CreateShaderText([
            "a\0b".into(),
            "t\0u".into(),
            "h".into(),
            "fh".into(),
            "fb".into(),
            "ff".into(),
        ]),
        GetShaderText,
        GetCacheId,
    ]
}

/// The port's description agrees with the wheel's after every call:
/// - names with a NUL and double underscores, every setter, the cache ID kept stale by
///   `getNextResourceIndex` and cleared by the setters, `begin`, `end` and a clone (with the
///   unique ID, and without the shader text of a finalize);
/// - the code sections in the 10 languages, finalized (the OSL and Metal class wrappers
///   included) and rebuilt by `createShaderText`, and two parameter declarations; the Metal
///   wrapper reading declarations with a texture's sampler, an array, a comment, a sampler
///   line of 6 bytes, a texture on the last line without a line feed, a short sampler line, a
///   line starting with one `/`, and every white-space character before a type;
/// - finalizes in turn in several languages;
/// - textures in 1D, 2D and 3D, with every error, the binding start added after them, a
///   clone, which has none, and a 3D texture of the largest edge.
#[test]
fn descriptions_match_the_wheel_call_by_call() {
    let mut cases: Vec<(String, Vec<DescCall>)> = vec![(
        "names and the cache ID".into(),
        vec![
            SetUniqueId("u\0v__w".into()),
            SetFunctionName("a__b___c\0d".into()),
            SetPixelName("p__x".into()),
            SetResourcePrefix("r__".into()),
            GetCacheId,
            GetNextResourceIndex,
            GetNextResourceIndex,
            GetCacheId,
            SetDescriptorSetIndex(7, 9),
            GetCacheId,
            SetDescriptorSetIndex(7, 0),
            Begin("x".into()),
            End,
            SetTextureMaxWidth(17),
            SetAllowTexture1d(false),
            Clone,
            GetCacheId,
            GetNextResourceIndex,
            GetCacheId,
            SetLanguage(oracle_gpu::GpuLanguage::GlslEs30),
            GetCacheId,
            GetNumDynamicProperties,
            GetDynamicProperty("DYNAMIC_PROPERTY_EXPOSURE".into()),
            GetDynamicProperty("DYNAMIC_PROPERTY_GRADING_HUECURVE".into()),
            GetDynamicPropertyAt(0),
            GetDynamicPropertyAt(3),
            SetUniqueId("x__y".into()),
        ],
    )];
    cases.push((
        "Vulkan without parameters".into(),
        vec![
            SetLanguage(oracle_gpu::GpuLanguage::GlslVk46),
            AddToTextureDeclareShaderCode("x;\n".into()),
            Finalize,
            GetShaderText,
        ],
    ));
    cases.push((
        "a clone after a finalize".into(),
        vec![
            AddToFunctionShaderCode("x;\n".into()),
            Finalize,
            Clone,
            GetShaderText,
            SetLanguage(oracle_gpu::GpuLanguage::Glsl12),
            GetCacheId,
        ],
    ));
    cases.push((
        "a clone's unique ID".into(),
        vec![SetUniqueId("u__1".into()), Clone, GetCacheId],
    ));
    cases.push((
        "setters clear a cache ID left stale by getNextResourceIndex".into(),
        vec![
            GetCacheId,
            GetNextResourceIndex,
            GetCacheId,
            SetUniqueId("u".into()),
            GetCacheId,
            GetNextResourceIndex,
            GetCacheId,
            SetFunctionName("OCIOMain".into()),
            GetCacheId,
        ],
    ));
    cases.push((
        "two parameter declarations".into(),
        vec![
            AddToParameterDeclareShaderCode("float p;\n".into()),
            AddToParameterDeclareShaderCode("float q;\n".into()),
            Finalize,
            GetShaderText,
        ],
    ));
    let declarations = [
        "texture2d<float> t;\nsampler s;\n  // a comment\nint u[4];\n",
        "texture1d<float> t;\n123456\n",
        "texture1d<float> t;",
        "texture2d<float> t;\nsampler\n",
        "  \ttexture3d<float> c;\nsampler   cS ;\nfloat\tf;\nbool b;\n",
        "texture2d<float> t;\nsampler s;\n/x y;\n\u{b}float v;\n \u{c}\u{b}\rint w;\n",
    ];
    for language in oracle_gpu::GpuLanguage::ALL {
        for (i, d) in declarations.iter().enumerate() {
            cases.push((format!("code {i} in {language:?}"), code(language, d)));
        }
    }
    cases.push((
        "finalizes in turn".into(),
        [
            code(oracle_gpu::GpuLanguage::Glsl12, declarations[0]),
            vec![
                SetLanguage(oracle_gpu::GpuLanguage::GlslVk46),
                Finalize,
                GetShaderText,
                SetLanguage(oracle_gpu::GpuLanguage::Msl20),
                SetResourcePrefix("".into()),
                Finalize,
                GetShaderText,
                SetLanguage(oracle_gpu::GpuLanguage::Osl1),
                Finalize,
                Finalize,
                GetShaderText,
                GetCacheId,
                Clone,
                GetCacheId,
                Finalize,
                GetShaderText,
            ],
        ]
        .concat(),
    ));
    cases.push((
        "textures".into(),
        vec![
            red("r1", 5, 1),
            red("r2", 4, 3),
            red("wide", 4097, 1),
            red("", 1, 1),
            AddTexture {
                name: "rgb".into(),
                sampler_name: String::new(),
                width: 2,
                height: 2,
                channel: "TEXTURE_RGB_CHANNEL".into(),
                dimensions: "TEXTURE_2D".into(),
                interpolation: "INTERP_NEAREST".into(),
                values: vec![0.5; 12],
            },
            AddTexture {
                name: "rgb\0x".into(),
                sampler_name: "s\0y".into(),
                width: 2,
                height: 2,
                channel: "TEXTURE_RGB_CHANNEL".into(),
                dimensions: "TEXTURE_2D".into(),
                interpolation: "INTERP_TETRAHEDRAL".into(),
                values: (0..12).map(|i| f32::from_bits(0x7fc0_0000 | i)).collect(),
            },
            red("zero", 0, 2),
            red("high", 2, 0),
            Add3dTexture {
                name: "c".into(),
                sampler_name: "cS".into(),
                edge_len: 3,
                interpolation: "INTERP_BEST".into(),
                values: (0..81).map(|i| i as f32 / 81.0).collect(),
            },
            Add3dTexture {
                name: "e".into(),
                sampler_name: "eS".into(),
                edge_len: 0,
                interpolation: "INTERP_LINEAR".into(),
                values: Vec::new(),
            },
            // One texel a side past the limit, with all its values (the binding checks them
            // first).
            Add3dTexture {
                name: "big".into(),
                sampler_name: "bigS".into(),
                edge_len: 130,
                interpolation: "INTERP_LINEAR".into(),
                values: vec![0.0; 130 * 130 * 130 * 3],
            },
            Add3dTexture {
                name: "".into(),
                sampler_name: "eS".into(),
                edge_len: 1,
                interpolation: "INTERP_DEFAULT".into(),
                values: vec![1.0, 2.0, 3.0],
            },
            SetDescriptorSetIndex(1, 40),
            GetTexture(0),
            GetTexture(1),
            GetTexture(3),
            GetTexture(4),
            Get3dTexture(0),
            Get3dTexture(1),
            SetTextureMaxWidth(3),
            red("narrow", 4, 1),
            red("fits", 3, 1),
            SetDescriptorSetIndex(0, u32::MAX),
            GetTexture(0),
            Get3dTexture(0),
            Clone,
            GetTexture(0),
        ],
    ));
    cases.push((
        "a 3D texture of the largest edge".into(),
        vec![Add3dTexture {
            name: "c".into(),
            sampler_name: "cS".into(),
            edge_len: 129,
            interpolation: "INTERP_LINEAR".into(),
            values: (0..129 * 129 * 129 * 3)
                .map(|i| (i % 1000) as f32)
                .collect(),
        }],
    ));

    let requests: Vec<GpuShaderDescRequest> = cases
        .iter()
        .map(|(_, calls)| GpuShaderDescRequest::new(calls.clone()))
        .collect();
    let calls: Vec<_> = requests.iter().map(GpuShaderDescRequest::call).collect();
    for ((label, calls), response) in cases.iter().zip(Oracle::get().batch(&calls, true)) {
        let reply = GpuShaderDescReply::from_response(
            response.unwrap_or_else(|e| panic!("{label}: the oracle failed: {e}")),
        );
        check(label, calls, &reply);
    }
}

/// The size a shader declares for the array `name`: the `N` of `name[N]` in its text.
fn array_size(text: &str, name: &str) -> u32 {
    let start = text
        .find(&format!("{name}["))
        .unwrap_or_else(|| panic!("{name}[ in the shader"))
        + name.len()
        + 1;
    let end = start + text[start..].find(']').expect("]");
    text[start..end].parse().expect("an array size")
}

/// A dynamic transform of the oracle's spec.
fn dynamic(class: &str, style: &str) -> serde_json::Value {
    serde_json::json!({"class": class, "args": {"style": {"enum": style}, "dynamic": true}})
}

/// The uniform buffer's layout follows the wheel's, for every uniform type and their mixes:
/// the port adds the uniforms each of the wheel's shaders holds, in its order and with the
/// array sizes it declares, and gets the wheel's buffer offsets and buffer size. (The values
/// come from the ops' writers, ported with the ops.)
///
/// On the way, before each uniform, one of its type with an empty name is refused, and the
/// buffer then ends at the wheel's offset for that uniform: upstream aligns the buffer before
/// the `Uniform` constructor refuses the name (GpuShader.cpp:380-449, 178-185 @ v2.5.2).
/// After each uniform, every name taken so far, added again with its type, is refused
/// (`uniformNameUsed`, lines 460-470) and moves nothing, so the layout stays the wheel's. The
/// wheel's processors never add an empty or a taken name, so these refusals are checked by
/// the layout they leave.
#[test]
fn uniform_buffer_layouts_match_the_wheel() {
    let exposure_contrast = serde_json::json!({"class": "ExposureContrastTransform",
        "args": {"exposure": 0.5, "contrast": 1.25, "gamma": 1.5},
        "calls": [["makeExposureDynamic"], ["makeContrastDynamic"], ["makeGammaDynamic"]]});
    let gradings = [
        dynamic("GradingPrimaryTransform", "GRADING_LOG"),
        dynamic("GradingRGBCurveTransform", "GRADING_LIN"),
        dynamic("GradingToneTransform", "GRADING_VIDEO"),
        dynamic("GradingHueCurveTransform", "GRADING_LOG"),
    ];
    let mut transforms = vec![exposure_contrast.clone()];
    transforms.extend(gradings.iter().cloned());
    transforms.push(serde_json::json!({"class": "GroupTransform", "children": [
        gradings[1].clone(), exposure_contrast, gradings[0].clone(), gradings[3].clone(),
        gradings[2].clone()]}));
    let requests: Vec<GpuShaderRequest> = transforms
        .into_iter()
        .map(|t| {
            GpuShaderRequest::new(
                serde_json::json!({ "transform": t }),
                ShaderSettings::language(oracle_gpu::GpuLanguage::Glsl40),
            )
        })
        .collect();
    let calls: Vec<_> = requests.iter().map(GpuShaderRequest::call).collect();
    for (i, response) in Oracle::get().batch(&calls, true).into_iter().enumerate() {
        let reply = GpuShaderReply::from_response(response.expect("the oracle"));
        let shader = reply.shader();
        let mut desc = GpuShaderDesc::new(GpuLanguage::Glsl4_0);
        // Adds a uniform of the type and array size of the wheel's uniform `u`, named `name`.
        let add =
            |desc: &mut GpuShaderDesc, u: &oracle_gpu::Uniform, name: &str| match u.kind.as_str() {
                "UNIFORM_DOUBLE" => desc.add_uniform_double(name, Arc::new(|| 0.0)),
                "UNIFORM_BOOL" => desc.add_uniform_bool(name, Arc::new(|| false)),
                "UNIFORM_FLOAT3" => desc.add_uniform_float3(name, Arc::new(|| [0.0; 3])),
                "UNIFORM_VECTOR_FLOAT" => desc.add_uniform_vector_float(
                    name,
                    Arc::new(|| 0),
                    Arc::new(Vec::new),
                    array_size(&shader.text, &u.name),
                ),
                "UNIFORM_VECTOR_INT" => desc.add_uniform_vector_int(
                    name,
                    Arc::new(|| 0),
                    Arc::new(Vec::new),
                    array_size(&shader.text, &u.name),
                ),
                other => panic!("{other}"),
            };
        for (k, u) in shader.uniforms.iter().enumerate() {
            // An empty name is refused, after the buffer is aligned for the uniform's type:
            // the buffer then ends where the wheel puts the uniform.
            assert!(add(&mut desc, u, "").is_err(), "case {i}: an empty name");
            assert_eq!(desc.num_uniforms() as usize, k, "case {i}");
            assert_eq!(
                desc.uniform_buffer_size() as u64,
                u.buffer_offset,
                "case {i}: the buffer after an empty name of the type of {}",
                u.name
            );

            assert_eq!(add(&mut desc, u, &u.name), Ok(true), "case {i}: {}", u.name);

            // A name taken, with any type, adds nothing and leaves the buffer as it is.
            let buffer_size = desc.uniform_buffer_size();
            for taken in &shader.uniforms[..=k] {
                assert_eq!(
                    add(&mut desc, u, &taken.name),
                    Ok(false),
                    "case {i}: {} again",
                    taken.name
                );
            }
            assert_eq!(desc.num_uniforms() as usize, k + 1, "case {i}");
            assert_eq!(desc.uniform_buffer_size(), buffer_size, "case {i}");
        }
        let offsets: Vec<u64> = desc
            .uniforms()
            .iter()
            .map(|u| u.buffer_offset() as u64)
            .collect();
        let wheel_offsets: Vec<u64> = shader.uniforms.iter().map(|u| u.buffer_offset).collect();
        assert_eq!(offsets, wheel_offsets, "case {i}");
        let types: Vec<String> = desc
            .uniforms()
            .iter()
            .map(|u| uniform_type_name(u.data().data_type()).to_string())
            .collect();
        let wheel_types: Vec<String> = shader.uniforms.iter().map(|u| u.kind.clone()).collect();
        assert_eq!(types, wheel_types, "case {i}");
        assert_eq!(
            Some(desc.uniform_buffer_size() as u64),
            shader.getters["uniform_buffer_size"].as_u64(),
            "case {i}"
        );
        assert!(!desc.uniforms().is_empty(), "case {i}");
    }
}

fn uniform_type_name(t: UniformDataType) -> &'static str {
    match t {
        UniformDataType::Double => "UNIFORM_DOUBLE",
        UniformDataType::Bool => "UNIFORM_BOOL",
        UniformDataType::Float3 => "UNIFORM_FLOAT3",
        UniformDataType::VectorFloat => "UNIFORM_VECTOR_FLOAT",
        UniformDataType::VectorInt => "UNIFORM_VECTOR_INT",
        UniformDataType::Unknown => "UNIFORM_UNKNOWN",
    }
}

/// A second dynamic property of a type is refused with the wheel's message, which numbers the
/// type: the wheel raises it when two ops of a processor make the same property dynamic. The
/// port's description takes a property of each type here (the grading types as stand-ins of
/// the double kind: the description reads only the type).
#[test]
fn a_second_dynamic_property_of_a_type_is_refused_as_the_wheel_refuses_it() {
    let exposure_contrast = |call: &str| {
        serde_json::json!({"class": "ExposureContrastTransform",
            "args": {"exposure": 0.5, "contrast": 1.25, "gamma": 1.5}, "calls": [[call]]})
    };
    let pairs: Vec<serde_json::Value> = vec![
        exposure_contrast("makeExposureDynamic"),
        exposure_contrast("makeContrastDynamic"),
        exposure_contrast("makeGammaDynamic"),
        dynamic("GradingPrimaryTransform", "GRADING_LOG"),
        dynamic("GradingRGBCurveTransform", "GRADING_LIN"),
        dynamic("GradingToneTransform", "GRADING_VIDEO"),
        dynamic("GradingHueCurveTransform", "GRADING_LOG"),
    ];
    let requests: Vec<GpuShaderRequest> = pairs
        .iter()
        .map(|t| {
            GpuShaderRequest::new(
                serde_json::json!({"transform": {"class": "GroupTransform",
                    "children": [t, t]}}),
                ShaderSettings::language(oracle_gpu::GpuLanguage::Glsl40),
            )
        })
        .collect();
    let calls: Vec<_> = requests.iter().map(GpuShaderRequest::call).collect();
    for ((_, type_), response) in PROPERTY_TYPES.iter().zip(Oracle::get().batch(&calls, true)) {
        let reply = GpuShaderReply::from_response(response.expect("the oracle"));
        let raised = reply.raised().expect("the wheel raises");
        let mut desc = GpuShaderDesc::default();
        let property = || {
            DynamicPropertyRcPtr::from(Arc::new(DynamicPropertyDoubleImpl::new(*type_, 0.5, true)))
        };
        desc.add_dynamic_property(property()).unwrap();
        assert!(desc.has_dynamic_property(*type_));
        let error = desc.add_dynamic_property(property()).unwrap_err();
        assert_eq!(error.message(), raised.message, "{type_:?}");
        assert_eq!(desc.num_dynamic_properties(), 1);
    }
}
