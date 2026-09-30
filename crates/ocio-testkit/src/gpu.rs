// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Typed requests and replies for the oracle's `gpu_shader` command
//! (`oracle/ocio_oracle/gpu.py`, chunk O1.3): a GPU processor's shader, extracted into a
//! `GpuShaderDesc` with any of its settings, and everything the description then holds.
//!
//! A [`GpuShaderRequest`] names the processor as `cpu_apply` does (a JSON object with
//! `config`, `transform` or `src`/`dst`, `direction`, and optionally `optimization`) and the
//! description's settings ([`ShaderSettings`]). Its [`GpuShaderReply`] has the shader text,
//! the uniforms with their values, the textures with their values, the dynamic properties
//! and the cache IDs, or what OCIO raised.

use serde_json::{Map, Value, json};

use crate::Oracle;
use crate::oracle::{BatchCall, Response, bytes_to_f32};

/// A shading language (`GpuLanguage`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GpuLanguage {
    /// `GPU_LANGUAGE_CG`.
    Cg,
    /// `GPU_LANGUAGE_GLSL_1_2`, the default.
    Glsl12,
    /// `GPU_LANGUAGE_GLSL_1_3`.
    Glsl13,
    /// `GPU_LANGUAGE_GLSL_4_0`.
    Glsl40,
    /// `GPU_LANGUAGE_GLSL_VK_4_6`.
    GlslVk46,
    /// `GPU_LANGUAGE_GLSL_ES_1_0`.
    GlslEs10,
    /// `GPU_LANGUAGE_GLSL_ES_3_0`.
    GlslEs30,
    /// `GPU_LANGUAGE_HLSL_SM_5_0`, which the Python binding names `GPU_LANGUAGE_HLSL_DX11`.
    HlslSm50,
    /// `LANGUAGE_OSL_1`.
    Osl1,
    /// `GPU_LANGUAGE_MSL_2_0`.
    Msl20,
}

impl GpuLanguage {
    /// All 10 languages, in the enum's order.
    pub const ALL: [GpuLanguage; 10] = [
        GpuLanguage::Cg,
        GpuLanguage::Glsl12,
        GpuLanguage::Glsl13,
        GpuLanguage::Glsl40,
        GpuLanguage::GlslVk46,
        GpuLanguage::GlslEs10,
        GpuLanguage::GlslEs30,
        GpuLanguage::HlslSm50,
        GpuLanguage::Osl1,
        GpuLanguage::Msl20,
    ];

    /// The name the oracle takes and reports: the Python binding's.
    pub fn oracle_name(self) -> &'static str {
        match self {
            GpuLanguage::Cg => "GPU_LANGUAGE_CG",
            GpuLanguage::Glsl12 => "GPU_LANGUAGE_GLSL_1_2",
            GpuLanguage::Glsl13 => "GPU_LANGUAGE_GLSL_1_3",
            GpuLanguage::Glsl40 => "GPU_LANGUAGE_GLSL_4_0",
            GpuLanguage::GlslVk46 => "GPU_LANGUAGE_GLSL_VK_4_6",
            GpuLanguage::GlslEs10 => "GPU_LANGUAGE_GLSL_ES_1_0",
            GpuLanguage::GlslEs30 => "GPU_LANGUAGE_GLSL_ES_3_0",
            GpuLanguage::HlslSm50 => "GPU_LANGUAGE_HLSL_DX11",
            GpuLanguage::Osl1 => "LANGUAGE_OSL_1",
            GpuLanguage::Msl20 => "GPU_LANGUAGE_MSL_2_0",
        }
    }
}

/// The settings of a `GpuShaderDesc`, each set with its setter on `CreateShaderDesc()`'s
/// defaults, in this order. `None` keeps the default.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShaderSettings {
    /// `setLanguage`.
    pub language: Option<GpuLanguage>,
    /// `setFunctionName`.
    pub function_name: Option<String>,
    /// `setPixelName`.
    pub pixel_name: Option<String>,
    /// `setResourcePrefix`.
    pub resource_prefix: Option<String>,
    /// `setUniqueID`.
    pub uid: Option<String>,
    /// `setDescriptorSetIndex(index, textureBindingStart)`.
    pub descriptor_set: Option<(u32, u32)>,
    /// `setTextureMaxWidth`.
    pub texture_max_width: Option<u32>,
    /// `setAllowTexture1D`.
    pub allow_texture_1d: Option<bool>,
}

impl ShaderSettings {
    /// Settings with only a language.
    pub fn language(language: GpuLanguage) -> ShaderSettings {
        ShaderSettings {
            language: Some(language),
            ..ShaderSettings::default()
        }
    }

    /// The `shader` argument of the command.
    pub fn json(&self) -> Value {
        let mut out = Map::new();
        if let Some(language) = self.language {
            out.insert("language".into(), json!(language.oracle_name()));
        }
        for (key, value) in [
            ("function_name", &self.function_name),
            ("pixel_name", &self.pixel_name),
            ("resource_prefix", &self.resource_prefix),
            ("uid", &self.uid),
        ] {
            if let Some(value) = value {
                out.insert(key.into(), json!(value));
            }
        }
        if let Some((index, start)) = self.descriptor_set {
            out.insert(
                "descriptor_set".into(),
                json!({"index": index, "texture_binding_start": start}),
            );
        }
        if let Some(width) = self.texture_max_width {
            out.insert("texture_max_width".into(), json!(width));
        }
        if let Some(allowed) = self.allow_texture_1d {
            out.insert("allow_texture_1d".into(), json!(allowed));
        }
        Value::Object(out)
    }
}

/// One `gpu_shader` call.
#[derive(Debug, Clone, PartialEq)]
pub struct GpuShaderRequest {
    /// The processor: `config`, `transform` or `src`/`dst`, `direction`, and `optimization`
    /// for `getOptimizedGPUProcessor` (absent: `getDefaultGPUProcessor`).
    pub processor: Value,
    /// The description's settings.
    pub shader: ShaderSettings,
}

impl GpuShaderRequest {
    /// A request for `processor`'s shader with `shader`'s settings.
    pub fn new(processor: Value, shader: ShaderSettings) -> GpuShaderRequest {
        GpuShaderRequest { processor, shader }
    }

    /// The command's arguments.
    pub fn args(&self) -> Value {
        let mut args = self.processor.clone();
        args["shader"] = self.shader.json();
        args
    }

    /// The request as a call of an [`Oracle::batch`].
    pub fn call(&self) -> BatchCall<'_> {
        BatchCall {
            cmd: "gpu_shader",
            args: self.args(),
            blobs: Vec::new(),
        }
    }

    /// Runs the request alone; panics if the oracle fails (a refusal included).
    pub fn run(&self) -> GpuShaderReply {
        GpuShaderReply::from_response(Oracle::get().call("gpu_shader", self.args(), &[]))
    }
}

/// What OCIO raised, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Raised {
    /// The Python type: `Exception` for OCIO's.
    pub kind: String,
    /// The message.
    pub message: String,
    /// `config`, `transform`, `processor`, `gpu_processor`, `shader_desc` or `extract`.
    pub stage: String,
}

/// A uniform's value, read with the getter of its type.
#[derive(Debug, Clone, PartialEq)]
pub enum UniformValue {
    /// `UNIFORM_DOUBLE`.
    Double(f64),
    /// `UNIFORM_BOOL`.
    Bool(bool),
    /// `UNIFORM_FLOAT3`. The binding passes these as Python floats, which quiets a signalling
    /// NaN.
    Float3([f32; 3]),
    /// `UNIFORM_VECTOR_FLOAT`.
    VectorFloat(Vec<f32>),
    /// `UNIFORM_VECTOR_INT`.
    VectorInt(Vec<i32>),
    /// `UNIFORM_UNKNOWN`.
    Unknown,
}

/// A uniform of the shader.
#[derive(Debug, Clone, PartialEq)]
pub struct Uniform {
    /// Its name.
    pub name: String,
    /// Its `UniformDataType` name.
    pub kind: String,
    /// Its `bufferOffset`.
    pub buffer_offset: u64,
    /// Its value at extraction.
    pub value: UniformValue,
}

/// A 1D or 2D texture of the shader.
#[derive(Debug, Clone, PartialEq)]
pub struct Texture {
    /// `textureName`.
    pub name: String,
    /// `samplerName`.
    pub sampler_name: String,
    /// `width`, in texels.
    pub width: u64,
    /// `height`, in texels.
    pub height: u64,
    /// `TEXTURE_RED_CHANNEL` or `TEXTURE_RGB_CHANNEL`.
    pub channel: String,
    /// `TEXTURE_1D` or `TEXTURE_2D`.
    pub dimensions: String,
    /// The `Interpolation` name.
    pub interpolation: String,
    /// `textureShaderBindingIndex`.
    pub binding_index: u64,
    /// The values, `width * height` texels of 1 or 3 channels.
    pub values: Vec<f32>,
}

/// A 3D texture of the shader.
#[derive(Debug, Clone, PartialEq)]
pub struct Texture3d {
    /// `textureName`.
    pub name: String,
    /// `samplerName`.
    pub sampler_name: String,
    /// `edgeLen`.
    pub edge_len: u64,
    /// The `Interpolation` name.
    pub interpolation: String,
    /// `textureShaderBindingIndex`.
    pub binding_index: u64,
    /// The values, `edge_len^3` RGB texels.
    pub values: Vec<f32>,
}

/// A dynamic property's value.
#[derive(Debug, Clone, PartialEq)]
pub enum DynamicValue {
    /// Exposure, contrast or gamma.
    Double(f64),
    /// A grading value: its `repr()`.
    Repr(String),
}

/// A dynamic property of the shader description.
#[derive(Debug, Clone, PartialEq)]
pub struct DynamicProperty {
    /// Its `DynamicPropertyType` name.
    pub kind: String,
    /// Its value at extraction.
    pub value: DynamicValue,
}

/// What the description holds after the extraction.
#[derive(Debug, Clone, PartialEq)]
pub struct Shader {
    /// `getCacheID()`.
    pub cache_id: String,
    /// `getShaderText()`.
    pub text: String,
    /// The uniforms, in order.
    pub uniforms: Vec<Uniform>,
    /// The 1D and 2D textures, in order.
    pub textures: Vec<Texture>,
    /// The 3D textures, in order.
    pub textures_3d: Vec<Texture3d>,
    /// The dynamic properties, in order.
    pub dynamic_properties: Vec<DynamicProperty>,
    /// The other getters, as the command reports them: `language`, `function_name`,
    /// `pixel_name`, `resource_prefix`, `uid`, `descriptor_set_index`,
    /// `texture_binding_start`, `texture_max_width`, `allow_texture_1d`,
    /// `uniform_buffer_size`.
    pub getters: Value,
}

/// The wheel's answer to a [`GpuShaderRequest`].
#[derive(Debug, Clone, PartialEq)]
pub struct GpuShaderReply {
    /// The command's result: the cache IDs, `gpu_processor`, `shader`, `exception`, `stage`,
    /// `log`.
    pub result: Value,
    /// The shader, when nothing raised.
    pub shader: Option<Shader>,
}

fn string(value: &Value, key: &str) -> String {
    value[key]
        .as_str()
        .unwrap_or_else(|| panic!("{key} in {value}"))
        .to_string()
}

fn number(value: &Value, key: &str) -> u64 {
    value[key]
        .as_u64()
        .unwrap_or_else(|| panic!("{key} in {value}"))
}

fn f32_bits(value: &Value) -> Vec<f32> {
    value
        .as_array()
        .unwrap_or_else(|| panic!("float bits: {value}"))
        .iter()
        .map(|b| f32::from_bits(b.as_u64().expect("float bits") as u32))
        .collect()
}

fn uniform(value: &Value) -> Uniform {
    let kind = string(value, "type");
    let v = &value["value"];
    let value_of = match kind.as_str() {
        "UNIFORM_DOUBLE" => UniformValue::Double(f64::from_bits(v.as_u64().expect("bits"))),
        "UNIFORM_BOOL" => UniformValue::Bool(v.as_bool().expect("a bool")),
        "UNIFORM_FLOAT3" => UniformValue::Float3(f32_bits(v).try_into().expect("3 floats")),
        "UNIFORM_VECTOR_FLOAT" => UniformValue::VectorFloat(f32_bits(v)),
        "UNIFORM_VECTOR_INT" => UniformValue::VectorInt(
            v.as_array()
                .expect("ints")
                .iter()
                .map(|i| i32::try_from(i.as_i64().expect("an int")).expect("an i32"))
                .collect(),
        ),
        _ => UniformValue::Unknown,
    };
    Uniform {
        name: string(value, "name"),
        kind,
        buffer_offset: number(value, "buffer_offset"),
        value: value_of,
    }
}

impl GpuShaderReply {
    /// Reads the command's response.
    pub fn from_response(response: Response) -> GpuShaderReply {
        let shader = response.result.get("shader").map(|s| {
            let values = |t: &Value| bytes_to_f32(&response.blobs[number(t, "values") as usize]);
            let list = |key: &str| s[key].as_array().cloned().unwrap_or_default();
            let mut getters = s.clone();
            if let Some(object) = getters.as_object_mut() {
                for key in [
                    "cache_id",
                    "text",
                    "uniforms",
                    "textures",
                    "textures_3d",
                    "dynamic_properties",
                ] {
                    object.remove(key);
                }
            }
            Shader {
                cache_id: string(s, "cache_id"),
                text: response.blob_text(number(s, "text") as usize).to_string(),
                uniforms: list("uniforms").iter().map(uniform).collect(),
                textures: list("textures")
                    .iter()
                    .map(|t| Texture {
                        name: string(t, "name"),
                        sampler_name: string(t, "sampler_name"),
                        width: number(t, "width"),
                        height: number(t, "height"),
                        channel: string(t, "channel"),
                        dimensions: string(t, "dimensions"),
                        interpolation: string(t, "interpolation"),
                        binding_index: number(t, "binding_index"),
                        values: values(t),
                    })
                    .collect(),
                textures_3d: list("textures_3d")
                    .iter()
                    .map(|t| Texture3d {
                        name: string(t, "name"),
                        sampler_name: string(t, "sampler_name"),
                        edge_len: number(t, "edge_len"),
                        interpolation: string(t, "interpolation"),
                        binding_index: number(t, "binding_index"),
                        values: values(t),
                    })
                    .collect(),
                dynamic_properties: list("dynamic_properties")
                    .iter()
                    .map(|p| DynamicProperty {
                        kind: string(p, "type"),
                        value: match p.get("double") {
                            Some(bits) => {
                                DynamicValue::Double(f64::from_bits(bits.as_u64().expect("bits")))
                            }
                            None => DynamicValue::Repr(string(p, "repr")),
                        },
                    })
                    .collect(),
                getters,
            }
        });
        GpuShaderReply {
            result: response.result,
            shader,
        }
    }

    /// What OCIO raised, if anything.
    pub fn raised(&self) -> Option<Raised> {
        let exception = self.result.get("exception")?;
        Some(Raised {
            kind: string(exception, "type"),
            message: string(exception, "message"),
            stage: string(&self.result, "stage"),
        })
    }

    /// The shader; panics with the result if OCIO raised.
    pub fn shader(&self) -> &Shader {
        self.shader
            .as_ref()
            .unwrap_or_else(|| panic!("no shader: {}", self.result))
    }

    /// OCIO's log messages.
    pub fn log(&self) -> Vec<String> {
        self.result["log"]
            .as_array()
            .map(|log| {
                log.iter()
                    .map(|m| m.as_str().unwrap_or_default().to_string())
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A request's arguments: the processor keys at the top level, the settings under `shader`,
    /// only those given.
    #[test]
    fn requests_become_the_commands_arguments() {
        let request = GpuShaderRequest::new(
            json!({"transform": {"class": "LogTransform"}, "optimization": "OPTIMIZATION_NONE"}),
            ShaderSettings {
                language: Some(GpuLanguage::HlslSm50),
                resource_prefix: Some("p".into()),
                descriptor_set: Some((2, 3)),
                allow_texture_1d: Some(false),
                ..ShaderSettings::default()
            },
        );
        assert_eq!(
            request.args(),
            json!({
                "transform": {"class": "LogTransform"},
                "optimization": "OPTIMIZATION_NONE",
                "shader": {
                    "language": "GPU_LANGUAGE_HLSL_DX11",
                    "resource_prefix": "p",
                    "descriptor_set": {"index": 2, "texture_binding_start": 3},
                    "allow_texture_1d": false,
                },
            })
        );
        assert_eq!(ShaderSettings::default().json(), json!({}));
    }

    /// A reply's uniforms, textures and dynamic properties come from the result and the
    /// blobs: floats from their bits, texture values from their blob.
    #[test]
    fn replies_read_the_result_and_the_blobs() {
        let result = json!({
            "shader": {
                "cache_id": "id", "text": 0, "language": "GPU_LANGUAGE_CG",
                "uniforms": [
                    {"name": "d", "type": "UNIFORM_DOUBLE", "buffer_offset": 0,
                     "value": 0x3FF8_0000_0000_0000u64},
                    {"name": "f", "type": "UNIFORM_FLOAT3", "buffer_offset": 16,
                     "value": [0x3F80_0000u32, 0x7FC0_0001u32, 0]},
                    {"name": "i", "type": "UNIFORM_VECTOR_INT", "buffer_offset": 32,
                     "value": [-1, 2]},
                    {"name": "u", "type": "UNIFORM_UNKNOWN", "buffer_offset": 0, "value": null},
                ],
                "textures": [{"name": "t", "sampler_name": "s", "width": 2, "height": 1,
                              "channel": "TEXTURE_RED_CHANNEL", "dimensions": "TEXTURE_1D",
                              "interpolation": "INTERP_LINEAR", "binding_index": 1,
                              "values": 1}],
                "textures_3d": [],
                "dynamic_properties": [{"type": "DYNAMIC_PROPERTY_GAMMA",
                                        "double": 0x4000_0000_0000_0000u64}],
            },
            "log": [],
        });
        let blobs = vec![
            b"text".to_vec(),
            [1.5f32, -2.0].map(f32::to_le_bytes).concat(),
        ];
        let reply = GpuShaderReply::from_response(Response { result, blobs });
        let shader = reply.shader();
        assert_eq!(shader.text, "text");
        assert_eq!(shader.uniforms[0].value, UniformValue::Double(1.5));
        let UniformValue::Float3(f) = shader.uniforms[1].value else {
            panic!("{:?}", shader.uniforms[1]);
        };
        assert_eq!(f.map(f32::to_bits), [0x3F80_0000, 0x7FC0_0001, 0]);
        assert_eq!(
            shader.uniforms[2].value,
            UniformValue::VectorInt(vec![-1, 2])
        );
        assert_eq!(shader.uniforms[3].value, UniformValue::Unknown);
        assert_eq!(shader.textures[0].values, vec![1.5, -2.0]);
        assert_eq!(
            shader.dynamic_properties[0].value,
            DynamicValue::Double(2.0)
        );
        assert_eq!(shader.getters, json!({"language": "GPU_LANGUAGE_CG"}));
        assert!(reply.raised().is_none());
    }
}
