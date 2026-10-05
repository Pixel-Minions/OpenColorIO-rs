// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/OpenColorIO/GpuShader.h` and `GpuShader.cpp` @ v2.5.2: what the generic shader
//! description holds besides the creator's state, its uniforms and its 1D, 2D and 3D textures
//! (`GenericGpuShaderDesc` and its `PrivateImpl`). [`GpuShaderDesc`] holds one, and gives its
//! methods.
//!
//! **Texture values.** Upstream copies `w * h * d * channels` floats from a pointer, that product
//! taken in `unsigned` arithmetic, which wraps (`CreateArray`, GpuShader.cpp:23-37). Here the
//! values come as a slice, and the same count is copied from it. A slice shorter than that
//! count is an error, where upstream would read past the caller's buffer (docs/improvements.md,
//! U-11).
//!
//! [`GpuShaderDesc`]: crate::gpu_shader_desc::GpuShaderDesc

use std::fmt;
use std::sync::Arc;

use ocio_ops::ops::lut3d::lut3d_op_data::{Interpolation, MAX_3D_LUT_LENGTH};
use ocio_ops::{Exception, Result};

use crate::open_color_types::UniformDataType;

/// A function that gives a uniform's current value. The op writers make them over their
/// dynamic properties, so the value follows the property after the extraction, as upstream's
/// `std::function` getters do.
pub type Getter<T> = Arc<dyn Fn() -> T + Send + Sync>;

/// What a texture's texels hold.
///
/// Port of `GpuShaderCreator::TextureType` (include/OpenColorIO/OpenColorIO.h:3485-3489 @
/// v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextureType {
    /// `TEXTURE_RED_CHANNEL`: only a red channel.
    RedChannel = 0,
    /// `TEXTURE_RGB_CHANNEL`: red, green and blue.
    RgbChannel = 1,
}

impl TextureType {
    /// The floats a texel holds.
    fn channels(self) -> u32 {
        match self {
            TextureType::RedChannel => 1,
            TextureType::RgbChannel => 3,
        }
    }
}

/// Whether a 1D LUT's texture is 1D or 2D.
///
/// Port of `GpuShaderCreator::TextureDimensions` (include/OpenColorIO/OpenColorIO.h:3494-3497
/// @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextureDimensions {
    /// `TEXTURE_1D`.
    D1 = 1,
    /// `TEXTURE_2D`.
    D2 = 2,
}

/// A uniform's getters, by type.
///
/// Port of `GpuShaderDesc::UniformData` (include/OpenColorIO/OpenColorIO.h:3748-3765 @ v2.5.2)
/// without its buffer offset, which is on [`Uniform`]. Upstream's `UNIFORM_UNKNOWN` is the type
/// of a default-constructed `UniformData`, which a description never holds.
#[derive(Clone)]
pub enum UniformData {
    /// `UNIFORM_DOUBLE`: `m_getDouble`.
    Double(Getter<f64>),
    /// `UNIFORM_BOOL`: `m_getBool`.
    Bool(Getter<bool>),
    /// `UNIFORM_FLOAT3`: `m_getFloat3`.
    Float3(Getter<[f32; 3]>),
    /// `UNIFORM_VECTOR_FLOAT`: `m_vectorFloat`, the number of values in use and the values.
    VectorFloat {
        /// `m_getSize`.
        size: Getter<i32>,
        /// `m_getVector`.
        values: Getter<Vec<f32>>,
    },
    /// `UNIFORM_VECTOR_INT`: `m_vectorInt`, the number of values in use and the values.
    VectorInt {
        /// `m_getSize`.
        size: Getter<i32>,
        /// `m_getVector`.
        values: Getter<Vec<i32>>,
    },
}

impl UniformData {
    /// `m_type`.
    pub fn data_type(&self) -> UniformDataType {
        match self {
            UniformData::Double(_) => UniformDataType::Double,
            UniformData::Bool(_) => UniformDataType::Bool,
            UniformData::Float3(_) => UniformDataType::Float3,
            UniformData::VectorFloat { .. } => UniformDataType::VectorFloat,
            UniformData::VectorInt { .. } => UniformDataType::VectorInt,
        }
    }
}

impl fmt::Debug for UniformData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "UniformData::{:?}", self.data_type())
    }
}

/// A uniform: its name, its getters, and where it sits in the uniform buffer.
///
/// Port of `GPUShaderImpl::PrivateImpl::Uniform` (GpuShader.cpp:127-186 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct Uniform {
    /// `m_name`.
    name: Vec<u8>,
    /// `m_data`, less its buffer offset.
    data: UniformData,
    /// `m_data.m_bufferOffset`.
    buffer_offset: usize,
}

impl Uniform {
    /// Port of `Uniform::Uniform` (GpuShader.cpp:129-174, 178-185 @ v2.5.2): an empty name is
    /// refused.
    fn new(name: Vec<u8>, data: UniformData, buffer_offset: usize) -> Result<Uniform> {
        if name.is_empty() {
            return Err(Exception::new("The dynamic property name is invalid."));
        }
        Ok(Uniform {
            name,
            data,
            buffer_offset,
        })
    }

    /// The name, as `getUniform` returns it.
    pub fn name(&self) -> &[u8] {
        &self.name
    }

    /// The getters.
    pub fn data(&self) -> &UniformData {
        &self.data
    }

    /// `m_bufferOffset`: the uniform's offset in the uniform buffer, in bytes.
    pub fn buffer_offset(&self) -> usize {
        self.buffer_offset
    }
}

/// A texture: a 1D LUT's, in a 1D or 2D texture, or a 3D LUT's.
///
/// Port of `GPUShaderImpl::PrivateImpl::Texture` (GpuShader.cpp:65-123 @ v2.5.2).
#[derive(Debug, Clone, PartialEq)]
struct TextureData {
    /// `m_textureName`.
    texture_name: Vec<u8>,
    /// `m_samplerName`.
    sampler_name: Vec<u8>,
    /// `m_width`.
    width: u32,
    /// `m_height`.
    height: u32,
    /// `m_depth`.
    depth: u32,
    /// `m_type`.
    channel: TextureType,
    /// `m_dimensions`: 1, 2 or 3.
    dimensions: u32,
    /// `m_interp`.
    interpolation: Interpolation,
    /// `m_textureShaderBindingIndex`, before the description's binding start is added.
    binding_index: u32,
    /// `m_values`.
    values: Vec<f32>,
}

/// `std::string(s)` for a `const char *`: the bytes up to the first NUL.
pub(crate) fn c_string(s: &[u8]) -> &[u8] {
    s.iter().position(|&b| b == 0).map_or(s, |nul| &s[..nul])
}

impl TextureData {
    /// Refuses an empty texture or sampler name and a size of 0, then copies the values.
    ///
    /// Port of `Texture::Texture` (GpuShader.cpp:67-108 @ v2.5.2) and of `CreateArray`
    /// (GpuShader.cpp:23-37), whose `buf == nullptr` check ("The buffer is invalid") a slice
    /// can't reach.
    #[allow(clippy::too_many_arguments)]
    fn new(
        texture_name: &[u8],
        sampler_name: &[u8],
        w: u32,
        h: u32,
        d: u32,
        channel: TextureType,
        dimensions: u32,
        interpolation: Interpolation,
        binding_index: u32,
        values: &[f32],
    ) -> Result<TextureData> {
        let texture_name = c_string(texture_name);
        let sampler_name = c_string(sampler_name);
        if texture_name.is_empty() {
            return Err(Exception::new("The texture name is invalid."));
        }
        if sampler_name.is_empty() {
            return Err(Exception::new("The texture sampler name is invalid."));
        }
        if w == 0 || h == 0 || d == 0 {
            return Err(Exception::new(format!(
                "The texture buffer size is invalid: [{w} x {h} x {d}]."
            )));
        }

        // `w * h * d * (type==TEXTURE_RGB_CHANNEL ? 3 : 1)`, in `unsigned`.
        let size = w
            .wrapping_mul(h)
            .wrapping_mul(d)
            .wrapping_mul(channel.channels()) as usize;
        if values.len() < size {
            // Upstream reads `size` floats from the pointer, past the caller's buffer (U-11).
            return Err(Exception::new(
                [
                    b"The texture '".as_slice(),
                    texture_name,
                    format!(
                        "' needs {size} values, but only {} were given.",
                        values.len()
                    )
                    .as_bytes(),
                ]
                .concat(),
            ));
        }

        Ok(TextureData {
            texture_name: texture_name.to_vec(),
            sampler_name: sampler_name.to_vec(),
            width: w,
            height: h,
            depth: d,
            channel,
            dimensions,
            interpolation,
            binding_index,
            values: values[..size].to_vec(),
        })
    }
}

/// A 1D LUT's texture, in one or two dimensions.
///
/// What `GpuShaderDesc::getTexture` and `getTextureValues` give (GpuShader.cpp:229-274 @
/// v2.5.2). The binding index is the description's
/// ([`GpuShaderDesc::texture_shader_binding_index`]), which adds its current binding start.
///
/// [`GpuShaderDesc::texture_shader_binding_index`]:
///     crate::gpu_shader_desc::GpuShaderDesc::texture_shader_binding_index
#[derive(Debug, Clone, PartialEq)]
pub struct Texture(TextureData);

impl Texture {
    /// The texture's name.
    pub fn texture_name(&self) -> &[u8] {
        &self.0.texture_name
    }

    /// The sampler's name.
    pub fn sampler_name(&self) -> &[u8] {
        &self.0.sampler_name
    }

    /// The width, in texels.
    pub fn width(&self) -> u32 {
        self.0.width
    }

    /// The height, in texels: 1 for a texture in one row.
    pub fn height(&self) -> u32 {
        self.0.height
    }

    /// What the texels hold.
    pub fn channel(&self) -> TextureType {
        self.0.channel
    }

    /// 1D or 2D. Upstream's `getTexture` throws "1D LUT cannot have more than two dimensions"
    /// for more; `addTexture` takes a [`TextureDimensions`], so that can't happen.
    pub fn dimensions(&self) -> TextureDimensions {
        match self.0.dimensions {
            1 => TextureDimensions::D1,
            _ => TextureDimensions::D2,
        }
    }

    /// The interpolation.
    pub fn interpolation(&self) -> Interpolation {
        self.0.interpolation
    }

    /// The values, `width * height` texels of 1 or 3 floats.
    pub fn values(&self) -> &[f32] {
        &self.0.values
    }
}

/// A 3D LUT's texture: RGB, `edge_len` texels a side.
///
/// What `GpuShaderDesc::get3DTexture` and `get3DTextureValues` give (GpuShader.cpp:313-346 @
/// v2.5.2). The binding index is the description's
/// ([`GpuShaderDesc::texture_3d_shader_binding_index`]), which adds its current binding start.
///
/// [`GpuShaderDesc::texture_3d_shader_binding_index`]:
///     crate::gpu_shader_desc::GpuShaderDesc::texture_3d_shader_binding_index
#[derive(Debug, Clone, PartialEq)]
pub struct Texture3D(TextureData);

impl Texture3D {
    /// The texture's name.
    pub fn texture_name(&self) -> &[u8] {
        &self.0.texture_name
    }

    /// The sampler's name.
    pub fn sampler_name(&self) -> &[u8] {
        &self.0.sampler_name
    }

    /// The texels a side.
    pub fn edge_len(&self) -> u32 {
        self.0.width
    }

    /// The interpolation.
    pub fn interpolation(&self) -> Interpolation {
        self.0.interpolation
    }

    /// The values, `edge_len^3` RGB texels.
    pub fn values(&self) -> &[f32] {
        &self.0.values
    }
}

/// `GPU_FLOAT_SIZE`, `GPU_FLOAT_ALIGNMENT` (GpuShader.cpp:53-54 @ v2.5.2).
const GPU_FLOAT_SIZE: usize = 4;
const GPU_FLOAT_ALIGNMENT: usize = 4;
/// `GPU_INT_SIZE`, `GPU_INT_ALIGNMENT` (GpuShader.cpp:55-56 @ v2.5.2).
const GPU_INT_SIZE: usize = 4;
const GPU_INT_ALIGNMENT: usize = 4;
/// `GPU_VEC3_SIZE`, `GPU_VEC3_ALIGNMENT` (GpuShader.cpp:57-58 @ v2.5.2).
const GPU_VEC3_SIZE: usize = 12;
const GPU_VEC3_ALIGNMENT: usize = 16;
/// `GPU_ARRAY_ALIGNMENT`, `GPU_ARRAY_STRIDE` (GpuShader.cpp:59-60 @ v2.5.2).
const GPU_ARRAY_ALIGNMENT: usize = 16;
const GPU_ARRAY_STRIDE: usize = 16;

/// `offset` rounded up to a multiple of `alignment`. Upstream throws "Alignment cannot be
/// zero." for an alignment of 0, which no caller passes.
///
/// Port of `alignOffset` (GpuShader.cpp:39-46 @ v2.5.2).
fn align_offset(offset: usize, alignment: usize) -> usize {
    offset.div_ceil(alignment) * alignment
}

/// The uniforms and textures of a shader description, and its texture limits.
///
/// Port of `GPUShaderImpl::PrivateImpl` (GpuShader.cpp:62-474 @ v2.5.2).
#[derive(Debug, Clone)]
pub(crate) struct GenericImpl {
    /// `m_textures`.
    textures: Vec<Texture>,
    /// `m_textures3D`.
    textures_3d: Vec<Texture3D>,
    /// `m_uniforms`.
    uniforms: Vec<Uniform>,
    /// `m_max1DLUTWidth`.
    max_1d_lut_width: u32,
    /// `m_allowTexture1D`.
    allow_texture_1d: bool,
    /// `m_uniformBufferSize`.
    uniform_buffer_size: usize,
}

impl Default for GenericImpl {
    /// Port of `PrivateImpl::PrivateImpl` (GpuShader.cpp:191 @ v2.5.2): textures 4096 wide at
    /// most, 1D textures allowed, an empty uniform buffer.
    fn default() -> GenericImpl {
        GenericImpl {
            textures: Vec::new(),
            textures_3d: Vec::new(),
            uniforms: Vec::new(),
            max_1d_lut_width: 4 * 1024,
            allow_texture_1d: true,
            uniform_buffer_size: 0,
        }
    }
}

/// "<what> access error: index = N where size = M".
fn access_error(what: &str, index: u32, size: usize) -> Exception {
    Exception::new(format!(
        "{what} access error: index = {index} where size = {size}"
    ))
}

impl GenericImpl {
    /// Port of `PrivateImpl::get1dLutMaxWidth` (GpuShader.cpp:199 @ v2.5.2).
    pub(crate) fn texture_max_width(&self) -> u32 {
        self.max_1d_lut_width
    }

    /// Port of `PrivateImpl::set1dLutMaxWidth` (GpuShader.cpp:200 @ v2.5.2).
    pub(crate) fn set_texture_max_width(&mut self, max_width: u32) {
        self.max_1d_lut_width = max_width;
    }

    /// Port of `PrivateImpl::getAllowTexture1D` (GpuShader.cpp:202 @ v2.5.2).
    pub(crate) fn allow_texture_1d(&self) -> bool {
        self.allow_texture_1d
    }

    /// Port of `PrivateImpl::setAllowTexture1D` (GpuShader.cpp:203 @ v2.5.2).
    pub(crate) fn set_allow_texture_1d(&mut self, allowed: bool) {
        self.allow_texture_1d = allowed;
    }

    /// The next texture's binding index, before the binding start: the textures so far, 1D,
    /// 2D and 3D, as `unsigned`.
    fn next_binding_index(&self) -> u32 {
        (self.textures.len() as u32).wrapping_add(self.textures_3d.len() as u32)
    }

    /// Adds a 1D LUT's texture, and returns its binding index before the binding start.
    ///
    /// Port of `PrivateImpl::addTexture` (GpuShader.cpp:205-227 @ v2.5.2).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn add_texture(
        &mut self,
        texture_name: &[u8],
        sampler_name: &[u8],
        width: u32,
        height: u32,
        channel: TextureType,
        dimensions: TextureDimensions,
        interpolation: Interpolation,
        values: &[f32],
    ) -> Result<u32> {
        if width > self.texture_max_width() {
            return Err(Exception::new(format!(
                "1D LUT size exceeds the maximum: {width} > {}",
                self.texture_max_width()
            )));
        }
        let binding_index = self.next_binding_index();
        let t = TextureData::new(
            texture_name,
            sampler_name,
            width,
            height,
            1,
            channel,
            dimensions as u32,
            interpolation,
            binding_index,
            values,
        )?;
        self.textures.push(Texture(t));
        Ok(binding_index)
    }

    /// The 1D LUT textures.
    pub(crate) fn textures(&self) -> &[Texture] {
        &self.textures
    }

    /// The 1D LUT texture at `index`, and its binding index before the binding start; the
    /// error is upstream's for each of `getTexture`, `getTextureValues` and
    /// `getTextureShaderBindingIndex`.
    ///
    /// Port of `PrivateImpl::getTexture`, `getTextureValues` and
    /// `getTextureShaderBindingIndex` (GpuShader.cpp:229-288 @ v2.5.2).
    pub(crate) fn texture(&self, index: u32) -> Result<(&Texture, u32)> {
        let t = self
            .textures
            .get(index as usize)
            .ok_or_else(|| access_error("1D LUT", index, self.textures.len()))?;
        Ok((t, t.0.binding_index))
    }

    /// The number of 1D LUT textures.
    pub(crate) fn num_textures(&self) -> u32 {
        self.textures.len() as u32
    }

    /// Adds a 3D LUT's texture, and returns its binding index before the binding start.
    ///
    /// Port of `PrivateImpl::add3DTexture` (GpuShader.cpp:290-311 @ v2.5.2).
    pub(crate) fn add_3d_texture(
        &mut self,
        texture_name: &[u8],
        sampler_name: &[u8],
        edgelen: u32,
        interpolation: Interpolation,
        values: &[f32],
    ) -> Result<u32> {
        if edgelen > MAX_3D_LUT_LENGTH {
            return Err(Exception::new(format!(
                "3D LUT edge length exceeds the maximum: {edgelen} > {MAX_3D_LUT_LENGTH}"
            )));
        }

        let binding_index = self.next_binding_index();
        let t = TextureData::new(
            texture_name,
            sampler_name,
            edgelen,
            edgelen,
            edgelen,
            TextureType::RgbChannel,
            3,
            interpolation,
            binding_index,
            values,
        )?;
        self.textures_3d.push(Texture3D(t));
        Ok(binding_index)
    }

    /// The 3D LUT textures.
    pub(crate) fn textures_3d(&self) -> &[Texture3D] {
        &self.textures_3d
    }

    /// The 3D LUT texture at `index`, and its binding index before the binding start; the
    /// error is upstream's for each of `get3DTexture`, `get3DTextureValues` and
    /// `get3DTextureShaderBindingIndex`.
    ///
    /// Port of `PrivateImpl::get3DTexture`, `get3DTextureValues` and
    /// `get3DTextureShaderBindingIndex` (GpuShader.cpp:313-360 @ v2.5.2).
    pub(crate) fn texture_3d(&self, index: u32) -> Result<(&Texture3D, u32)> {
        let t = self
            .textures_3d
            .get(index as usize)
            .ok_or_else(|| access_error("3D LUT", index, self.textures_3d.len()))?;
        Ok((t, t.0.binding_index))
    }

    /// The number of 3D LUT textures.
    pub(crate) fn num_textures_3d(&self) -> u32 {
        self.textures_3d.len() as u32
    }

    /// Port of `PrivateImpl::getNumUniforms` (GpuShader.cpp:362-365 @ v2.5.2).
    pub(crate) fn num_uniforms(&self) -> u32 {
        self.uniforms.len() as u32
    }

    /// The uniforms, in the order they were added.
    pub(crate) fn uniforms(&self) -> &[Uniform] {
        &self.uniforms
    }

    /// Port of `PrivateImpl::getUniform` (GpuShader.cpp:367-378 @ v2.5.2).
    pub(crate) fn uniform(&self, index: u32) -> Result<&Uniform> {
        self.uniforms
            .get(index as usize)
            .ok_or_else(|| access_error("Uniforms", index, self.uniforms.len()))
    }

    /// Adds a uniform at the buffer's end, aligned, unless one has the name already. As
    /// upstream, the buffer is aligned before an empty name is refused, so a refused uniform
    /// can still move the next one's offset.
    ///
    /// Port of the five `PrivateImpl::addUniform` (GpuShader.cpp:380-449 @ v2.5.2).
    pub(crate) fn add_uniform(
        &mut self,
        name: &[u8],
        data: UniformData,
        alignment: usize,
        size: usize,
    ) -> Result<bool> {
        let name = c_string(name);
        if self.uniform_name_used(name) {
            // Uniform is already there.
            return Ok(false);
        }
        self.uniform_buffer_size = align_offset(self.uniform_buffer_size, alignment);
        let uniform = Uniform::new(name.to_vec(), data, self.uniform_buffer_size)?;
        self.uniforms.push(uniform);
        self.uniform_buffer_size += size;
        Ok(true)
    }

    /// Port of `PrivateImpl::getUniformBufferSize` (GpuShader.cpp:451-454 @ v2.5.2).
    pub(crate) fn uniform_buffer_size(&self) -> usize {
        self.uniform_buffer_size
    }

    /// Port of `PrivateImpl::uniformNameUsed` (GpuShader.cpp:460-470 @ v2.5.2).
    fn uniform_name_used(&self, name: &[u8]) -> bool {
        self.uniforms.iter().any(|u| u.name == name)
    }
}

/// Each `addUniform` overload's alignment and size in the buffer: a double is a float, a bool
/// an int, a float3 a vec3, and a vector an array of `max_size` 16-byte elements.
///
/// GpuShader.cpp:380-449 @ v2.5.2.
pub(crate) fn uniform_layout(data: &UniformData, max_size: u32) -> (usize, usize) {
    match data {
        UniformData::Double(_) => (GPU_FLOAT_ALIGNMENT, GPU_FLOAT_SIZE),
        // bool not supported for buffered uniforms, using int instead
        UniformData::Bool(_) => (GPU_INT_ALIGNMENT, GPU_INT_SIZE),
        UniformData::Float3(_) => (GPU_VEC3_ALIGNMENT, GPU_VEC3_SIZE),
        UniformData::VectorFloat { .. } | UniformData::VectorInt { .. } => {
            (GPU_ARRAY_ALIGNMENT, GPU_ARRAY_STRIDE * max_size as usize)
        }
    }
}

#[cfg(test)]
#[path = "gpu_shader_tests.rs"]
mod tests;
