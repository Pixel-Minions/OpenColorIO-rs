// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Typed requests and replies for the oracle's `gpu_shader_desc` command
//! (`oracle/ocio_oracle/gpu_desc.py`, card p1-gpu-infra): a `GpuShaderDesc` made by
//! `CreateShaderDesc()` and driven through the calls Python has, in order; what each returns or
//! raises, then everything the description holds, as `gpu_shader` reports it ([`Shader`]).

use serde_json::{Value, json};

use crate::Oracle;
use crate::gpu::{GpuLanguage, Raised, Shader, Texture, Texture3d, shader, texture, texture_3d};
use crate::oracle::{BatchCall, Response};

/// A call on the description, with its arguments. Enums go by their PyOpenColorIO names
/// (`TEXTURE_RGB_CHANNEL`, `TEXTURE_2D`, `INTERP_LINEAR`).
#[derive(Debug, Clone, PartialEq)]
pub enum DescCall {
    /// `setUniqueID`.
    SetUniqueId(String),
    /// `setLanguage`.
    SetLanguage(GpuLanguage),
    /// `setFunctionName`.
    SetFunctionName(String),
    /// `setPixelName`.
    SetPixelName(String),
    /// `setResourcePrefix`.
    SetResourcePrefix(String),
    /// `setDescriptorSetIndex(index, textureBindingStart)`.
    SetDescriptorSetIndex(u32, u32),
    /// `setTextureMaxWidth`.
    SetTextureMaxWidth(u32),
    /// `setAllowTexture1D`.
    SetAllowTexture1d(bool),
    /// `getNextResourceIndex`.
    GetNextResourceIndex,
    /// `getCacheID`.
    GetCacheId,
    /// `begin(uid)`.
    Begin(String),
    /// `end`.
    End,
    /// `addToParameterDeclareShaderCode`.
    AddToParameterDeclareShaderCode(String),
    /// `addToTextureDeclareShaderCode`.
    AddToTextureDeclareShaderCode(String),
    /// `addToHelperShaderCode`.
    AddToHelperShaderCode(String),
    /// `addToFunctionHeaderShaderCode`.
    AddToFunctionHeaderShaderCode(String),
    /// `addToFunctionShaderCode`.
    AddToFunctionShaderCode(String),
    /// `addToFunctionFooterShaderCode`.
    AddToFunctionFooterShaderCode(String),
    /// `createShaderText` of the six sections.
    CreateShaderText([String; 6]),
    /// `finalize`.
    Finalize,
    /// `getShaderText`.
    GetShaderText,
    /// `addTexture`.
    AddTexture {
        /// `textureName`.
        name: String,
        /// `samplerName`.
        sampler_name: String,
        /// `width`.
        width: u32,
        /// `height`.
        height: u32,
        /// `channel`: a `TextureType` name.
        channel: String,
        /// `dimensions`: a `TextureDimensions` name.
        dimensions: String,
        /// `interpolation`: an `Interpolation` name.
        interpolation: String,
        /// `values`.
        values: Vec<f32>,
    },
    /// `add3DTexture`.
    Add3dTexture {
        /// `textureName`.
        name: String,
        /// `samplerName`.
        sampler_name: String,
        /// `edgeLen`.
        edge_len: u32,
        /// `interpolation`: an `Interpolation` name.
        interpolation: String,
        /// `values`.
        values: Vec<f32>,
    },
    /// The texture iterator's item `index` (`getTexture` and `getTextureValues`).
    GetTexture(u32),
    /// The 3D texture iterator's item `index` (`get3DTexture` and `get3DTextureValues`).
    Get3dTexture(u32),
    /// The length of the dynamic property iterator (`getNumDynamicProperties`).
    GetNumDynamicProperties,
    /// `getDynamicProperty(type)`: a `DynamicPropertyType` name.
    GetDynamicProperty(String),
    /// The dynamic property iterator's item `index` (`getDynamicProperty(index)`).
    GetDynamicPropertyAt(u32),
    /// `clone()`, which then replaces the description.
    Clone,
}

/// Little-endian bytes of `values`.
fn f32_bytes(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

impl DescCall {
    /// The call as the command takes it. Texture values go to `blobs`.
    fn json(&self, blobs: &mut Vec<Vec<u8>>) -> Value {
        let mut blob = |values: &[f32]| {
            blobs.push(f32_bytes(values));
            blobs.len() - 1
        };
        match self {
            DescCall::SetUniqueId(s) => json!(["setUniqueID", s]),
            DescCall::SetLanguage(l) => json!(["setLanguage", l.oracle_name()]),
            DescCall::SetFunctionName(s) => json!(["setFunctionName", s]),
            DescCall::SetPixelName(s) => json!(["setPixelName", s]),
            DescCall::SetResourcePrefix(s) => json!(["setResourcePrefix", s]),
            DescCall::SetDescriptorSetIndex(i, start) => {
                json!(["setDescriptorSetIndex", i, start])
            }
            DescCall::SetTextureMaxWidth(w) => json!(["setTextureMaxWidth", w]),
            DescCall::SetAllowTexture1d(b) => json!(["setAllowTexture1D", b]),
            DescCall::GetNextResourceIndex => json!(["getNextResourceIndex"]),
            DescCall::GetCacheId => json!(["getCacheID"]),
            DescCall::Begin(s) => json!(["begin", s]),
            DescCall::End => json!(["end"]),
            DescCall::AddToParameterDeclareShaderCode(s) => {
                json!(["addToParameterDeclareShaderCode", s])
            }
            DescCall::AddToTextureDeclareShaderCode(s) => {
                json!(["addToTextureDeclareShaderCode", s])
            }
            DescCall::AddToHelperShaderCode(s) => json!(["addToHelperShaderCode", s]),
            DescCall::AddToFunctionHeaderShaderCode(s) => {
                json!(["addToFunctionHeaderShaderCode", s])
            }
            DescCall::AddToFunctionShaderCode(s) => json!(["addToFunctionShaderCode", s]),
            DescCall::AddToFunctionFooterShaderCode(s) => {
                json!(["addToFunctionFooterShaderCode", s])
            }
            DescCall::CreateShaderText([a, b, c, d, e, f]) => {
                json!(["createShaderText", a, b, c, d, e, f])
            }
            DescCall::Finalize => json!(["finalize"]),
            DescCall::GetShaderText => json!(["getShaderText"]),
            DescCall::AddTexture {
                name,
                sampler_name,
                width,
                height,
                channel,
                dimensions,
                interpolation,
                values,
            } => json!([
                "addTexture",
                name,
                sampler_name,
                width,
                height,
                channel,
                dimensions,
                interpolation,
                blob(values)
            ]),
            DescCall::Add3dTexture {
                name,
                sampler_name,
                edge_len,
                interpolation,
                values,
            } => json!([
                "add3DTexture",
                name,
                sampler_name,
                edge_len,
                interpolation,
                blob(values)
            ]),
            DescCall::GetTexture(i) => json!(["getTexture", i]),
            DescCall::Get3dTexture(i) => json!(["get3DTexture", i]),
            DescCall::GetNumDynamicProperties => json!(["getNumDynamicProperties"]),
            DescCall::GetDynamicProperty(t) => json!(["getDynamicProperty", t]),
            DescCall::GetDynamicPropertyAt(i) => json!(["getDynamicPropertyAt", i]),
            DescCall::Clone => json!(["clone"]),
        }
    }
}

/// One `gpu_shader_desc` call: the description's calls, in order.
#[derive(Debug, Clone, PartialEq)]
pub struct GpuShaderDescRequest {
    /// The calls.
    pub calls: Vec<DescCall>,
    args: Value,
    blobs: Vec<Vec<u8>>,
}

impl GpuShaderDescRequest {
    /// A request for `calls`.
    pub fn new(calls: Vec<DescCall>) -> GpuShaderDescRequest {
        let mut blobs = Vec::new();
        let list: Vec<Value> = calls.iter().map(|c| c.json(&mut blobs)).collect();
        GpuShaderDescRequest {
            calls,
            args: json!({ "calls": list }),
            blobs,
        }
    }

    /// The same calls at `LOGGING_LEVEL_DEBUG`, at which `finalize` logs the shader.
    pub fn with_debug_log(mut self) -> GpuShaderDescRequest {
        self.args["debug_log"] = json!(true);
        self
    }

    /// The command's arguments.
    pub fn args(&self) -> &Value {
        &self.args
    }

    /// The request as a call of an [`Oracle::batch`].
    pub fn call(&self) -> BatchCall<'_> {
        BatchCall {
            cmd: "gpu_shader_desc",
            args: self.args.clone(),
            blobs: self.blobs.iter().map(Vec::as_slice).collect(),
        }
    }

    /// Runs the request alone; panics if the oracle fails (a refusal included).
    pub fn run(&self) -> GpuShaderDescReply {
        let blobs: Vec<&[u8]> = self.blobs.iter().map(Vec::as_slice).collect();
        GpuShaderDescReply::from_response(Oracle::get().call(
            "gpu_shader_desc",
            self.args.clone(),
            &blobs,
        ))
    }
}

/// What a call returned.
#[derive(Debug, Clone, PartialEq)]
pub enum Returned {
    /// Nothing (`void`).
    Nothing,
    /// A string: the cache ID or the shader text.
    Text(String),
    /// A number: a resource index or a texture's binding index.
    Number(u64),
    /// A 1D or 2D texture.
    Texture(Texture),
    /// A 3D texture.
    Texture3d(Texture3d),
    /// A dynamic property, by its `DynamicPropertyType` name.
    DynamicProperty(String),
}

/// What a call returned or raised.
#[derive(Debug, Clone, PartialEq)]
pub enum DescOutcome {
    /// It returned.
    Returned(Returned),
    /// It raised (`stage` is empty).
    Raised(Raised),
}

/// The wheel's answer to a [`GpuShaderDescRequest`].
#[derive(Debug, Clone, PartialEq)]
pub struct GpuShaderDescReply {
    /// Each call's outcome, in order.
    pub calls: Vec<DescOutcome>,
    /// The description after the calls.
    pub shader: Shader,
    /// OCIO's log messages.
    pub log: Vec<String>,
}

impl GpuShaderDescReply {
    /// Reads the command's response.
    pub fn from_response(response: Response) -> GpuShaderDescReply {
        let result = &response.result;
        let calls = result["calls"]
            .as_array()
            .unwrap_or_else(|| panic!("calls in {result}"))
            .iter()
            .map(|c| {
                if let Some(e) = c.get("exception") {
                    return DescOutcome::Raised(Raised {
                        kind: e["type"].as_str().expect("type").to_string(),
                        message: e["message"].as_str().expect("message").to_string(),
                        stage: String::new(),
                    });
                }
                let r = &c["returned"];
                DescOutcome::Returned(match r {
                    Value::Null => Returned::Nothing,
                    Value::String(s) => Returned::Text(s.clone()),
                    Value::Number(n) => Returned::Number(n.as_u64().expect("a number")),
                    Value::Object(t) if t.contains_key("edge_len") => {
                        Returned::Texture3d(texture_3d(r, &response))
                    }
                    Value::Object(t) if t.len() == 1 && t.contains_key("type") => {
                        Returned::DynamicProperty(t["type"].as_str().expect("a type").into())
                    }
                    Value::Object(_) => Returned::Texture(texture(r, &response)),
                    _ => panic!("returned {r}"),
                })
            })
            .collect();
        let log = result["log"]
            .as_array()
            .map(|l| {
                l.iter()
                    .map(|m| m.as_str().expect("a message").to_string())
                    .collect()
            })
            .unwrap_or_default();
        GpuShaderDescReply {
            calls,
            shader: shader(&result["shader"], &response),
            log,
        }
    }
}
