// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/OpenColorIO/GpuShaderDesc.cpp` @ v2.5.2: [`GpuShaderDesc`], the description of
//! a shader program that a GPU processor fills in. Upstream splits it into the abstract
//! `GpuShaderCreator` and `GpuShaderDesc` classes (include/OpenColorIO/OpenColorIO.h:3361-3573,
//! 3725-3825) and their one implementation, `GenericGpuShaderDesc` (GpuShader.cpp, ported in
//! [`crate::gpu_shader`]). Here they are one type (the owner's decision, 2026-09-30).
//!
//! **Names and code are C strings upstream.** Every setter and code section takes a
//! `const char *`, so a name or a piece of code ends at its first NUL byte. The port takes
//! bytes, and ends them there too.

use std::str::Utf8Error;
use std::sync::{Mutex, PoisonError};

use ocio_ops::dynamic_property::DynamicPropertyRcPtr;
use ocio_ops::hash_utils::cache_id_hash;
use ocio_ops::logging::{is_debug_logging_enabled, log_debug};
use ocio_ops::open_color_types::DynamicPropertyType;
use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;
use ocio_ops::utils::string_utils::replace_in_place;
use ocio_ops::{Exception, Result};

use crate::gpu_shader::{
    GenericImpl, Getter, Texture, Texture3D, TextureDimensions, TextureType, Uniform, UniformData,
    c_string, uniform_layout,
};
use crate::gpu_shader_class_wrapper::GpuShaderClassWrapper;
use crate::open_color_types::{GpuLanguage, gpu_language_to_string};

/// A name as the setters keep it: a C string with every `__` made `_` ("Remove potentially
/// problematic double underscores from GLSL resource names").
fn resource_name(name: &[u8]) -> Vec<u8> {
    let mut name = c_string(name).to_vec();
    replace_in_place(&mut name, b"__", b"_");
    name
}

/// What a GPU processor fills in to describe a shader program: its language and names, its
/// code in sections, its uniforms and textures, and its dynamic properties.
///
/// It isn't `Clone`: upstream's `clone()` copies only part of it, see
/// [`GpuShaderDesc::clone_desc`].
///
/// Port of `GpuShaderCreator::Impl` and of `GpuShaderCreator`'s and `GpuShaderDesc`'s methods
/// (GpuShaderDesc.cpp:22-430 @ v2.5.2), with `GenericGpuShaderDesc` (GpuShader.cpp:478-645).
#[doc(alias = "GpuShaderCreator")]
#[derive(Debug)]
pub struct GpuShaderDesc {
    /// `m_uid`: a custom uid if needed.
    uid: Vec<u8>,
    /// `m_language`.
    language: GpuLanguage,
    /// `m_functionName`.
    function_name: Vec<u8>,
    /// `m_resourcePrefix`.
    resource_prefix: Vec<u8>,
    /// `m_pixelName`.
    pixel_name: Vec<u8>,
    /// `m_numResources`.
    num_resources: u32,
    /// `m_cacheID`, filled by [`GpuShaderDesc::cache_id`] when empty, under upstream's
    /// `m_cacheIDMutex`.
    cache_id: Mutex<Vec<u8>>,
    /// `m_parameterDeclarations`.
    parameter_declarations: Vec<u8>,
    /// `m_textureDeclarations`.
    texture_declarations: Vec<u8>,
    /// `m_helperMethods`.
    helper_methods: Vec<u8>,
    /// `m_functionHeader`.
    function_header: Vec<u8>,
    /// `m_functionBody`.
    function_body: Vec<u8>,
    /// `m_functionFooter`.
    function_footer: Vec<u8>,
    /// `m_shaderCode`.
    shader_code: Vec<u8>,
    /// `m_shaderCodeID`.
    shader_code_id: Vec<u8>,
    /// `m_dynamicProperties`.
    dynamic_properties: Vec<DynamicPropertyRcPtr>,
    /// `m_classWrappingInterface`.
    class_wrapper: GpuShaderClassWrapper,
    /// `m_descriptorSetIndex`.
    descriptor_set_index: u32,
    /// `m_textureBindingStart`.
    texture_binding_start: u32,
    /// `GenericGpuShaderDesc::m_implGeneric`: the uniforms, textures and texture limits.
    generic: GenericImpl,
}

impl Default for GpuShaderDesc {
    /// `GpuShaderDesc::CreateShaderDesc()`: GLSL 1.2, the function `OCIOMain`, the pixel
    /// `outColor`, the resource prefix `ocio`, descriptor set 0 with textures bound from 1.
    ///
    /// Port of `GpuShaderCreator::Impl::Impl` (GpuShaderDesc.cpp:22-58 @ v2.5.2) and of
    /// `GpuShaderDesc::CreateShaderDesc` (lines 405-408).
    fn default() -> GpuShaderDesc {
        let language = GpuLanguage::Glsl1_2;
        GpuShaderDesc {
            uid: Vec::new(),
            language,
            function_name: b"OCIOMain".to_vec(),
            resource_prefix: b"ocio".to_vec(),
            pixel_name: b"outColor".to_vec(),
            num_resources: 0,
            cache_id: Mutex::new(Vec::new()),
            parameter_declarations: Vec::new(),
            texture_declarations: Vec::new(),
            helper_methods: Vec::new(),
            function_header: Vec::new(),
            function_body: Vec::new(),
            function_footer: Vec::new(),
            shader_code: Vec::new(),
            shader_code_id: Vec::new(),
            dynamic_properties: Vec::new(),
            class_wrapper: GpuShaderClassWrapper::create_class_wrapper(language),
            descriptor_set_index: 0,
            texture_binding_start: 1,
            generic: GenericImpl::default(),
        }
    }
}

impl GpuShaderDesc {
    /// A description in `language`, with the other defaults.
    pub fn new(language: GpuLanguage) -> GpuShaderDesc {
        let mut desc = GpuShaderDesc::default();
        desc.set_language(language);
        desc
    }

    /// The cache ID, to be cleared by every setter.
    fn cache_id_mut(&mut self) -> &mut Vec<u8> {
        self.cache_id
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// A copy as upstream's `clone()` makes it: a new description whose creator state is
    /// copied (`Impl::operator=`). The names, language, resource count, cache ID, code sections,
    /// class wrapper (see [`GpuShaderClassWrapper::clone_wrapper`]) and descriptor set are
    /// copied, and the shader text and its ID are empty. The uniforms, textures, texture limits
    /// and dynamic properties are the new description's defaults.
    ///
    /// Port of `GpuShaderDesc::clone` (GpuShaderDesc.cpp:419-425 @ v2.5.2) and
    /// `GpuShaderCreator::Impl::operator=` (lines 64-91).
    pub fn clone_desc(&self) -> GpuShaderDesc {
        let cache_id = self
            .cache_id
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        GpuShaderDesc {
            uid: self.uid.clone(),
            language: self.language,
            function_name: self.function_name.clone(),
            resource_prefix: self.resource_prefix.clone(),
            pixel_name: self.pixel_name.clone(),
            num_resources: self.num_resources,
            cache_id: Mutex::new(cache_id),

            parameter_declarations: self.parameter_declarations.clone(),
            texture_declarations: self.texture_declarations.clone(),
            helper_methods: self.helper_methods.clone(),
            function_header: self.function_header.clone(),
            function_body: self.function_body.clone(),
            function_footer: self.function_footer.clone(),

            class_wrapper: self.class_wrapper.clone_wrapper(),

            descriptor_set_index: self.descriptor_set_index,
            texture_binding_start: self.texture_binding_start,

            shader_code: Vec::new(),
            shader_code_id: Vec::new(),
            // Not in `Impl`: the new description's.
            ..GpuShaderDesc::default()
        }
    }

    /// Port of `GpuShaderCreator::getUniqueID` (GpuShaderDesc.cpp:113-116 @ v2.5.2).
    pub fn unique_id(&self) -> &[u8] {
        &self.uid
    }

    /// Port of `GpuShaderCreator::setUniqueID` (GpuShaderDesc.cpp:106-111 @ v2.5.2).
    pub fn set_unique_id(&mut self, uid: impl AsRef<[u8]>) {
        self.uid = c_string(uid.as_ref()).to_vec();
        self.cache_id_mut().clear();
    }

    /// Port of `GpuShaderCreator::getLanguage` (GpuShaderDesc.cpp:129-132 @ v2.5.2).
    pub fn language(&self) -> GpuLanguage {
        self.language
    }

    /// Sets the language, and the class wrapper it needs.
    ///
    /// Port of `GpuShaderCreator::setLanguage` (GpuShaderDesc.cpp:118-127 @ v2.5.2).
    pub fn set_language(&mut self, lang: GpuLanguage) {
        self.language = lang;
        self.class_wrapper = GpuShaderClassWrapper::create_class_wrapper(self.language);
        self.cache_id_mut().clear();
    }

    /// Port of `GpuShaderCreator::getFunctionName` (GpuShaderDesc.cpp:142-145 @ v2.5.2).
    pub fn function_name(&self) -> &[u8] {
        &self.function_name
    }

    /// Port of `GpuShaderCreator::setFunctionName` (GpuShaderDesc.cpp:132-140 @ v2.5.2).
    pub fn set_function_name(&mut self, name: impl AsRef<[u8]>) {
        self.function_name = resource_name(name.as_ref());
        self.cache_id_mut().clear();
    }

    /// Port of `GpuShaderCreator::getResourcePrefix` (GpuShaderDesc.cpp:155-158 @ v2.5.2).
    pub fn resource_prefix(&self) -> &[u8] {
        &self.resource_prefix
    }

    /// Port of `GpuShaderCreator::setResourcePrefix` (GpuShaderDesc.cpp:147-153 @ v2.5.2).
    pub fn set_resource_prefix(&mut self, prefix: impl AsRef<[u8]>) {
        self.resource_prefix = resource_name(prefix.as_ref());
        self.cache_id_mut().clear();
    }

    /// Port of `GpuShaderCreator::getPixelName` (GpuShaderDesc.cpp:168-171 @ v2.5.2).
    pub fn pixel_name(&self) -> &[u8] {
        &self.pixel_name
    }

    /// Port of `GpuShaderCreator::setPixelName` (GpuShaderDesc.cpp:160-166 @ v2.5.2).
    pub fn set_pixel_name(&mut self, name: impl AsRef<[u8]>) {
        self.pixel_name = resource_name(name.as_ref());
        self.cache_id_mut().clear();
    }

    /// The index to append to the next resource's name, then counts it. Like upstream's, it
    /// doesn't clear the cache ID, which includes the count.
    ///
    /// Port of `GpuShaderCreator::getNextResourceIndex` (GpuShaderDesc.cpp:173-176 @ v2.5.2).
    pub fn next_resource_index(&mut self) -> u32 {
        let index = self.num_resources;
        self.num_resources = self.num_resources.wrapping_add(1);
        index
    }

    /// Sets the descriptor set index and the first texture binding, for the languages that
    /// use them (Vulkan GLSL). Binding 0 is the uniform buffer's.
    ///
    /// Port of `GpuShaderCreator::setDescriptorSetIndex` (GpuShaderDesc.cpp:178-188 @ v2.5.2).
    pub fn set_descriptor_set_index(
        &mut self,
        index: u32,
        texture_binding_start: u32,
    ) -> Result<()> {
        if texture_binding_start == 0 {
            return Err(Exception::new(
                "Texture binding start index must be greater than 0.",
            ));
        }
        self.descriptor_set_index = index;
        self.texture_binding_start = texture_binding_start;
        self.cache_id_mut().clear();
        Ok(())
    }

    /// Port of `GpuShaderCreator::getDescriptorSetIndex` (GpuShaderDesc.cpp:190-193 @ v2.5.2).
    pub fn descriptor_set_index(&self) -> u32 {
        self.descriptor_set_index
    }

    /// Port of `GpuShaderCreator::getTextureBindingStart` (GpuShaderDesc.cpp:195-198 @ v2.5.2).
    pub fn texture_binding_start(&self) -> u32 {
        self.texture_binding_start
    }

    /// Port of `GpuShaderCreator::hasDynamicProperty` (GpuShaderDesc.cpp:200-211 @ v2.5.2).
    pub fn has_dynamic_property(&self, type_: DynamicPropertyType) -> bool {
        self.dynamic_properties
            .iter()
            .any(|dp| dp.get_type() == type_)
    }

    /// Adds a dynamic property; one of each type at most. The message gives the type's number.
    ///
    /// Port of `GpuShaderCreator::addDynamicProperty` (GpuShaderDesc.cpp:213-224 @ v2.5.2).
    pub fn add_dynamic_property(&mut self, prop: DynamicPropertyRcPtr) -> Result<()> {
        if self.has_dynamic_property(prop.get_type()) {
            // Dynamic property is already there.
            return Err(Exception::new(format!(
                "Dynamic property already here: {}.",
                prop.get_type() as i32
            )));
        }
        self.dynamic_properties.push(prop);
        Ok(())
    }

    /// Port of `GpuShaderCreator::getNumDynamicProperties` (GpuShaderDesc.cpp:226-229 @ v2.5.2).
    pub fn num_dynamic_properties(&self) -> u32 {
        self.dynamic_properties.len() as u32
    }

    /// The dynamic properties, in the order they were added.
    pub fn dynamic_properties(&self) -> &[DynamicPropertyRcPtr] {
        &self.dynamic_properties
    }

    /// Port of `GpuShaderCreator::getDynamicProperty(unsigned)` (GpuShaderDesc.cpp:231-241 @
    /// v2.5.2).
    pub fn dynamic_property(&self, index: u32) -> Result<DynamicPropertyRcPtr> {
        self.dynamic_properties
            .get(index as usize)
            .cloned()
            .ok_or_else(|| {
                Exception::new(format!(
                    "Dynamic properties access error: index = {index} where size = {}",
                    self.dynamic_properties.len()
                ))
            })
    }

    /// Port of `GpuShaderCreator::getDynamicProperty(DynamicPropertyType)`
    /// (GpuShaderDesc.cpp:243-253 @ v2.5.2).
    pub fn dynamic_property_by_type(
        &self,
        type_: DynamicPropertyType,
    ) -> Result<DynamicPropertyRcPtr> {
        self.dynamic_properties
            .iter()
            .find(|dp| dp.get_type() == type_)
            .cloned()
            .ok_or_else(|| Exception::new("Dynamic property not found."))
    }

    /// Starts collecting the shader's code; it does nothing.
    ///
    /// Port of `GpuShaderCreator::begin` (GpuShaderDesc.cpp:255-257 @ v2.5.2).
    pub fn begin(&mut self, _uid: impl AsRef<[u8]>) {}

    /// Ends collecting the shader's code; it does nothing.
    ///
    /// Port of `GpuShaderCreator::end` (GpuShaderDesc.cpp:259-261 @ v2.5.2).
    pub fn end(&mut self) {}

    /// The cache ID: the language, the names, the resource count, the descriptor set and first
    /// binding, and the shader text's hash, separated by spaces. It is kept until a setter or
    /// [`GpuShaderDesc::create_shader_text`] clears it, so a later resource count isn't in it.
    ///
    /// Port of `GpuShaderCreator::getCacheID` (GpuShaderDesc.cpp:263-282 @ v2.5.2).
    pub fn cache_id(&self) -> Vec<u8> {
        let mut cache_id = self.cache_id.lock().unwrap_or_else(PoisonError::into_inner);
        if cache_id.is_empty() {
            let mut os = Vec::new();
            for part in [
                gpu_language_to_string(self.language).as_bytes(),
                &self.function_name,
                &self.resource_prefix,
                &self.pixel_name,
                self.num_resources.to_string().as_bytes(),
                self.descriptor_set_index.to_string().as_bytes(),
                self.texture_binding_start.to_string().as_bytes(),
            ] {
                os.extend_from_slice(part);
                os.push(b' ');
            }
            os.extend_from_slice(&self.shader_code_id);
            *cache_id = os;
        }
        cache_id.clone()
    }

    /// Port of `GpuShaderCreator::addToParameterDeclareShaderCode` (GpuShaderDesc.cpp:284-291
    /// @ v2.5.2).
    pub fn add_to_parameter_declare_shader_code(&mut self, shader_code: impl AsRef<[u8]>) {
        if self.parameter_declarations.is_empty() {
            self.parameter_declarations
                .extend_from_slice(b"\n// Declaration of all variables\n\n");
        }
        self.parameter_declarations
            .extend_from_slice(c_string(shader_code.as_ref()));
    }

    /// Port of `GpuShaderCreator::addToTextureDeclareShaderCode` (GpuShaderDesc.cpp:293-300
    /// @ v2.5.2).
    pub fn add_to_texture_declare_shader_code(&mut self, shader_code: impl AsRef<[u8]>) {
        if self.texture_declarations.is_empty() {
            self.texture_declarations
                .extend_from_slice(b"\n// Declaration of all textures\n\n");
        }
        self.texture_declarations
            .extend_from_slice(c_string(shader_code.as_ref()));
    }

    /// Port of `GpuShaderCreator::addToHelperShaderCode` (GpuShaderDesc.cpp:302-309 @ v2.5.2).
    pub fn add_to_helper_shader_code(&mut self, shader_code: impl AsRef<[u8]>) {
        if self.helper_methods.is_empty() {
            self.helper_methods
                .extend_from_slice(b"\n// Declaration of all helper methods\n\n");
        }
        self.helper_methods
            .extend_from_slice(c_string(shader_code.as_ref()));
    }

    /// Port of `GpuShaderCreator::addToFunctionShaderCode` (GpuShaderDesc.cpp:311-314 @ v2.5.2).
    pub fn add_to_function_shader_code(&mut self, shader_code: impl AsRef<[u8]>) {
        self.function_body
            .extend_from_slice(c_string(shader_code.as_ref()));
    }

    /// Port of `GpuShaderCreator::addToFunctionHeaderShaderCode` (GpuShaderDesc.cpp:316-319
    /// @ v2.5.2).
    pub fn add_to_function_header_shader_code(&mut self, shader_code: impl AsRef<[u8]>) {
        self.function_header
            .extend_from_slice(c_string(shader_code.as_ref()));
    }

    /// Port of `GpuShaderCreator::addToFunctionFooterShaderCode` (GpuShaderDesc.cpp:321-324
    /// @ v2.5.2).
    pub fn add_to_function_footer_shader_code(&mut self, shader_code: impl AsRef<[u8]>) {
        self.function_footer
            .extend_from_slice(c_string(shader_code.as_ref()));
    }

    /// Builds the shader program from its six sections, in order. In Vulkan GLSL, the
    /// parameter declarations, when there are any, go in the uniform block
    /// `<function name>_Parameters` of binding 0 in the descriptor set. The text's hash goes
    /// into the cache ID, which is cleared.
    ///
    /// Port of `GpuShaderCreator::createShaderText` (GpuShaderDesc.cpp:326-359 @ v2.5.2).
    pub fn create_shader_text(
        &mut self,
        shader_parameter_declarations: impl AsRef<[u8]>,
        shader_texture_declarations: impl AsRef<[u8]>,
        shader_helper_methods: impl AsRef<[u8]>,
        shader_function_header: impl AsRef<[u8]>,
        shader_function_body: impl AsRef<[u8]>,
        shader_function_footer: impl AsRef<[u8]>,
    ) {
        let parameter_declarations = c_string(shader_parameter_declarations.as_ref());
        let vulkan_block =
            self.language == GpuLanguage::GlslVk4_6 && !parameter_declarations.is_empty();

        let mut code = Vec::new();
        if vulkan_block {
            code.extend_from_slice(b"layout (set = ");
            code.extend_from_slice(self.descriptor_set_index.to_string().as_bytes());
            code.extend_from_slice(b", binding = 0) uniform ");
            code.extend_from_slice(&self.function_name);
            code.extend_from_slice(b"_Parameters\n{\n");
        }
        code.extend_from_slice(parameter_declarations);
        if vulkan_block {
            code.extend_from_slice(b"\n};\n");
        }

        for section in [
            shader_texture_declarations.as_ref(),
            shader_helper_methods.as_ref(),
            shader_function_header.as_ref(),
            shader_function_body.as_ref(),
            shader_function_footer.as_ref(),
        ] {
            code.extend_from_slice(c_string(section));
        }

        self.shader_code_id = cache_id_hash(&code).into_bytes();
        self.shader_code = code;
        self.cache_id_mut().clear();
    }

    /// Completes the shader program: the class wrapper, if the language needs one, goes around
    /// the declarations and after the function's footer, then the six sections make the text.
    /// With debug logging, the text is logged.
    ///
    /// The wrapper's errors come through: an MSL class name starting with a digit, and the
    /// port's error where upstream's Metal wrapper reads past a line (docs/improvements.md,
    /// U-10).
    ///
    /// Port of `GpuShaderCreator::finalize` (GpuShaderDesc.cpp:361-401 @ v2.5.2).
    pub fn finalize(&mut self) -> Result<()> {
        // For some GPU languages, the default header and footer do not fit well so, the class
        // wrapper encapsulates differences when needed.

        let original_header = [
            &self.parameter_declarations[..],
            b"\n",
            &self.texture_declarations[..],
        ]
        .concat();
        self.class_wrapper.prepare_class_wrapper(
            &self.resource_prefix,
            &self.function_name,
            &original_header,
        )?;

        if self.class_wrapper.has_class_wrapper_header() {
            self.parameter_declarations = self
                .class_wrapper
                .get_class_wrapper_header(&original_header)?;
            // clear texture declarations since they're already included in the header
            self.texture_declarations.clear();
        }
        self.function_footer = self
            .class_wrapper
            .get_class_wrapper_footer(&self.function_footer)?;

        // Build the complete shader program.

        let sections = [
            self.parameter_declarations.clone(),
            self.texture_declarations.clone(),
            self.helper_methods.clone(),
            self.function_header.clone(),
            self.function_body.clone(),
            self.function_footer.clone(),
        ];
        let [p, t, h, fh, fb, ff] = sections;
        self.create_shader_text(p, t, h, fh, fb, ff);

        if is_debug_logging_enabled() {
            let mut oss = b"\n**\nGPU Fragment Shader program\n".to_vec();
            oss.extend_from_slice(&self.shader_code);
            oss.push(b'\n');
            log_debug(oss);
        }
        Ok(())
    }

    /// The shader program, after [`GpuShaderDesc::finalize`].
    ///
    /// Port of `GpuShaderDesc::getShaderText` (GpuShaderDesc.cpp:427-430 @ v2.5.2).
    pub fn shader_text(&self) -> &[u8] {
        &self.shader_code
    }

    /// The shader program as text, when it is UTF-8, as it is unless a name isn't.
    pub fn shader_text_utf8(&self) -> std::result::Result<&str, Utf8Error> {
        std::str::from_utf8(&self.shader_code)
    }

    // -- The generic description: texture limits ------------------------------------------

    /// The widest 1D or 2D texture: 4096 by default.
    ///
    /// Port of `GenericGpuShaderDesc::getTextureMaxWidth` (GpuShader.cpp:548-551 @ v2.5.2).
    pub fn texture_max_width(&self) -> u32 {
        self.generic.texture_max_width()
    }

    /// Port of `GenericGpuShaderDesc::setTextureMaxWidth` (GpuShader.cpp:553-556 @ v2.5.2).
    pub fn set_texture_max_width(&mut self, max_width: u32) {
        self.generic.set_texture_max_width(max_width);
    }

    /// Whether 1D LUTs may use 1D textures (otherwise 2D ones): true by default.
    ///
    /// Port of `GenericGpuShaderDesc::getAllowTexture1D` (GpuShader.cpp:558-561 @ v2.5.2).
    pub fn allow_texture_1d(&self) -> bool {
        self.generic.allow_texture_1d()
    }

    /// Port of `GenericGpuShaderDesc::setAllowTexture1D` (GpuShader.cpp:563-566 @ v2.5.2).
    pub fn set_allow_texture_1d(&mut self, allowed: bool) {
        self.generic.set_allow_texture_1d(allowed);
    }

    // -- Uniforms ---------------------------------------------------------------------------

    /// Port of `GenericGpuShaderDesc::getNumUniforms` (GpuShader.cpp:502-505 @ v2.5.2).
    pub fn num_uniforms(&self) -> u32 {
        self.generic.num_uniforms()
    }

    /// The uniforms, in the order they were added.
    pub fn uniforms(&self) -> &[Uniform] {
        self.generic.uniforms()
    }

    /// Port of `GenericGpuShaderDesc::getUniform` (GpuShader.cpp:507-510 @ v2.5.2).
    pub fn uniform(&self, index: u32) -> Result<&Uniform> {
        self.generic.uniform(index)
    }

    /// Adds a uniform, aligned after the others in the uniform buffer, unless one has the
    /// name already: then nothing changes, and the result is `false`. An empty name is an
    /// error ("The dynamic property name is invalid."), after the buffer is aligned for it.
    /// A vector takes `max_size` 16-byte elements: the size of the array the shader declares.
    ///
    /// Port of the five `GenericGpuShaderDesc::addUniform` (GpuShader.cpp:512-541 @ v2.5.2).
    fn add_uniform(&mut self, name: &[u8], data: UniformData, max_size: u32) -> Result<bool> {
        let (alignment, size) = uniform_layout(&data, max_size);
        self.generic.add_uniform(name, data, alignment, size)
    }

    /// A double uniform (a float in the buffer). See `add_uniform`.
    pub fn add_uniform_double(&mut self, name: impl AsRef<[u8]>, get: Getter<f64>) -> Result<bool> {
        self.add_uniform(name.as_ref(), UniformData::Double(get), 0)
    }

    /// A bool uniform (an int in the buffer). See `add_uniform`.
    pub fn add_uniform_bool(&mut self, name: impl AsRef<[u8]>, get: Getter<bool>) -> Result<bool> {
        self.add_uniform(name.as_ref(), UniformData::Bool(get), 0)
    }

    /// A uniform of 3 floats. See `add_uniform`.
    pub fn add_uniform_float3(
        &mut self,
        name: impl AsRef<[u8]>,
        get: Getter<[f32; 3]>,
    ) -> Result<bool> {
        self.add_uniform(name.as_ref(), UniformData::Float3(get), 0)
    }

    /// A uniform vector of floats, `max_size` long in the shader. See `add_uniform`.
    pub fn add_uniform_vector_float(
        &mut self,
        name: impl AsRef<[u8]>,
        size: Getter<i32>,
        values: Getter<Vec<f32>>,
        max_size: u32,
    ) -> Result<bool> {
        self.add_uniform(
            name.as_ref(),
            UniformData::VectorFloat { size, values },
            max_size,
        )
    }

    /// A uniform vector of ints, `max_size` long in the shader. See `add_uniform`.
    pub fn add_uniform_vector_int(
        &mut self,
        name: impl AsRef<[u8]>,
        size: Getter<i32>,
        values: Getter<Vec<i32>>,
        max_size: u32,
    ) -> Result<bool> {
        self.add_uniform(
            name.as_ref(),
            UniformData::VectorInt { size, values },
            max_size,
        )
    }

    /// The uniform buffer's size, in bytes.
    ///
    /// Port of `GenericGpuShaderDesc::getUniformBufferSize` (GpuShader.cpp:543-546 @ v2.5.2).
    pub fn uniform_buffer_size(&self) -> usize {
        self.generic.uniform_buffer_size()
    }

    // -- Textures ---------------------------------------------------------------------------

    /// Port of `GenericGpuShaderDesc::getNumTextures` (GpuShader.cpp:568-571 @ v2.5.2).
    pub fn num_textures(&self) -> u32 {
        self.generic.num_textures()
    }

    /// Adds a 1D LUT's texture, and returns its shader binding index: its place among all the
    /// textures plus the texture binding start. `values` holds `width * height` texels of 1 or
    /// 3 floats (that count taken in `u32`, which wraps, as upstream's).
    ///
    /// Port of `GenericGpuShaderDesc::addTexture` (GpuShader.cpp:573-584 @ v2.5.2).
    #[allow(clippy::too_many_arguments)]
    pub fn add_texture(
        &mut self,
        texture_name: impl AsRef<[u8]>,
        sampler_name: impl AsRef<[u8]>,
        width: u32,
        height: u32,
        channel: TextureType,
        dimensions: TextureDimensions,
        interpolation: Interpolation,
        values: &[f32],
    ) -> Result<u32> {
        let index = self.generic.add_texture(
            texture_name.as_ref(),
            sampler_name.as_ref(),
            width,
            height,
            channel,
            dimensions,
            interpolation,
            values,
        )?;
        Ok(index.wrapping_add(self.texture_binding_start()))
    }

    /// The 1D LUTs' textures, in the order they were added.
    pub fn textures(&self) -> &[Texture] {
        self.generic.textures()
    }

    /// Port of `GenericGpuShaderDesc::getTexture` and `getTextureValues` (GpuShader.cpp:586-601
    /// @ v2.5.2).
    pub fn texture(&self, index: u32) -> Result<&Texture> {
        Ok(self.generic.texture(index)?.0)
    }

    /// The shader binding index of the 1D LUT texture at `index`, with the current binding
    /// start.
    ///
    /// Port of `GenericGpuShaderDesc::getTextureShaderBindingIndex` (GpuShader.cpp:603-606 @
    /// v2.5.2).
    pub fn texture_shader_binding_index(&self, index: u32) -> Result<u32> {
        let (_, binding_index) = self.generic.texture(index)?;
        Ok(binding_index.wrapping_add(self.texture_binding_start()))
    }

    /// Port of `GenericGpuShaderDesc::getNum3DTextures` (GpuShader.cpp:608-611 @ v2.5.2).
    pub fn num_textures_3d(&self) -> u32 {
        self.generic.num_textures_3d()
    }

    /// Adds a 3D LUT's texture, and returns its shader binding index: its place among all the
    /// textures plus the texture binding start. `values` holds `edgelen^3` RGB texels.
    ///
    /// Port of `GenericGpuShaderDesc::add3DTexture` (GpuShader.cpp:613-621 @ v2.5.2).
    pub fn add_3d_texture(
        &mut self,
        texture_name: impl AsRef<[u8]>,
        sampler_name: impl AsRef<[u8]>,
        edgelen: u32,
        interpolation: Interpolation,
        values: &[f32],
    ) -> Result<u32> {
        let index = self.generic.add_3d_texture(
            texture_name.as_ref(),
            sampler_name.as_ref(),
            edgelen,
            interpolation,
            values,
        )?;
        Ok(index.wrapping_add(self.texture_binding_start()))
    }

    /// The 3D LUTs' textures, in the order they were added.
    pub fn textures_3d(&self) -> &[Texture3D] {
        self.generic.textures_3d()
    }

    /// Port of `GenericGpuShaderDesc::get3DTexture` and `get3DTextureValues`
    /// (GpuShader.cpp:623-635 @ v2.5.2).
    pub fn texture_3d(&self, index: u32) -> Result<&Texture3D> {
        Ok(self.generic.texture_3d(index)?.0)
    }

    /// The shader binding index of the 3D LUT texture at `index`, with the current binding
    /// start.
    ///
    /// Port of `GenericGpuShaderDesc::get3DTextureShaderBindingIndex` (GpuShader.cpp:637-640
    /// @ v2.5.2).
    pub fn texture_3d_shader_binding_index(&self, index: u32) -> Result<u32> {
        let (_, binding_index) = self.generic.texture_3d(index)?;
        Ok(binding_index.wrapping_add(self.texture_binding_start()))
    }
}
