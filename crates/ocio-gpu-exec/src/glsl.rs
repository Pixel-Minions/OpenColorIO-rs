// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/libutils/oglapphelpers/glsl.cpp/.h` @ v2.5.2: texture upload,
//! uniforms, fragment compilation and program linking on a current GL context.
#![allow(unsafe_code)]

use std::io::Write;
use std::rc::Rc;
use std::sync::Arc;

use glow::HasContext;
use ocio_gpu::GpuLanguage;
use ocio_gpu::GpuShaderDesc;
use ocio_gpu::gpu_shader::{TextureDimensions, TextureType, UniformData};
use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;
use ocio_ops::{Exception, Result};

use crate::oglapp::{GlContext, float_bytes};

const GL_PROXY_TEXTURE_2D: u32 = 0x8064;
const GL_TEXTURE_COMPONENTS: u32 = 0x1003;

/// The standard strings returned by gluErrorString for desktop GL errors.
/// Port of GetGLError/CheckStatus (glsl.cpp:35-58 @ v2.5.2), without linking GLU.
pub(crate) fn check_status(gl: &glow::Context) -> Result<()> {
    // SAFETY: only called while the owning context is current on this thread.
    let error = unsafe { gl.get_error() };
    let message = match error {
        glow::NO_ERROR => return Ok(()),
        glow::INVALID_ENUM => "invalid enumerant",
        glow::INVALID_VALUE => "invalid value",
        glow::INVALID_OPERATION => "invalid operation",
        glow::OUT_OF_MEMORY => "out of memory",
        glow::INVALID_FRAMEBUFFER_OPERATION => "invalid framebuffer operation",
        0x0503 => "stack overflow",
        0x0504 => "stack underflow",
        _ => "OpenGL Error",
    };
    Err(Exception::new(message))
}

#[derive(Debug)]
struct TextureId {
    handle: glow::Texture,
    sampler_name: String,
    target: u32,
}

#[derive(Debug)]
struct Uniform {
    location: Option<glow::UniformLocation>,
    data: UniformData,
}

/// glow's slice uploads infer a nonnegative count from length. Preserve the
/// original signed GLsizei for defined negative-count errors instead.
#[derive(Debug)]
struct VectorUniforms {
    float: unsafe extern "system" fn(i32, i32, *const f32),
    int: unsafe extern "system" fn(i32, i32, *const i32),
}

impl VectorUniforms {
    fn load(context: &GlContext) -> Result<Self> {
        let float = context.proc_address("glUniform1fv")?;
        let int = context.proc_address("glUniform1iv")?;
        // SAFETY: these are the named desktop GL 2.0 ABI entry points. The
        // builder's Rc context keeps the EGL/GL implementation alive.
        unsafe {
            Ok(Self {
                float: std::mem::transmute::<
                    *const std::ffi::c_void,
                    unsafe extern "system" fn(i32, i32, *const f32),
                >(float),
                int: std::mem::transmute::<
                    *const std::ffi::c_void,
                    unsafe extern "system" fn(i32, i32, *const i32),
                >(int),
            })
        }
    }
}

/// OpenGLBuilder (glsl.cpp:271-590 @ v2.5.2), retaining the description so every
/// dynamic getter remains alive. All resources share the Rc thread-bound context.
#[derive(Debug)]
pub(crate) struct OpenGLBuilder {
    context: Rc<GlContext>,
    desc: Arc<GpuShaderDesc>,
    textures: Vec<TextureId>,
    uniforms: Vec<Uniform>,
    vector_uniforms: VectorUniforms,
    start_index: u32,
    fragment_shader: Option<glow::Shader>,
    fragment_shader_attached: bool,
    program: glow::Program,
    shader_cache_id: Vec<u8>,
    verbose: bool,
}

impl OpenGLBuilder {
    /// Port of OpenGLBuilder::OpenGLBuilder (glsl.cpp:277-284 @ v2.5.2).
    pub(crate) fn new(context: Rc<GlContext>, desc: Arc<GpuShaderDesc>) -> Result<Self> {
        context.activate()?;
        let vector_uniforms = VectorUniforms::load(&context)?;
        // SAFETY: the owning context is current and remains alive through Rc.
        let program = unsafe { context.gl.create_program() }.map_err(Exception::new)?;
        Ok(Self {
            context,
            desc,
            textures: Vec::new(),
            uniforms: Vec::new(),
            vector_uniforms,
            start_index: 0,
            fragment_shader: None,
            fragment_shader_attached: false,
            program,
            shader_cache_id: Vec::new(),
            verbose: false,
        })
    }

    /// Port of OpenGLBuilder::setVerbose (glsl.h:69 @ v2.5.2).
    pub(crate) fn set_verbose(&mut self, verbose: bool) {
        self.verbose = verbose;
    }

    /// Port of OpenGLBuilder::allocateAllTextures (glsl.cpp:304-392 @ v2.5.2).
    pub(crate) fn allocate_all_textures(&mut self, start_index: u32) -> Result<()> {
        self.context.activate()?;
        self.delete_all_textures();
        self.start_index = start_index;
        let gl = &self.context.gl;
        // SAFETY: each texture is allocated in the current context, recorded for
        // deletion before the next fallible operation, and supplied with exactly
        // the immutable texel slice held by the description.
        unsafe {
            for texture in self.desc.textures_3d() {
                if texture.texture_name().is_empty()
                    || texture.sampler_name().is_empty()
                    || texture.edge_len() == 0
                {
                    return Err(Exception::new("The texture data is corrupted"));
                }
                let edge = texture.edge_len();
                check_texture_values(&[edge, edge, edge], 3, texture.values())?;
                let sampler_name = text(texture.sampler_name())?;
                let handle = gl.create_texture().map_err(Exception::new)?;
                let index = start_index + self.textures.len() as u32;
                self.textures.push(TextureId {
                    handle,
                    sampler_name,
                    target: glow::TEXTURE_3D,
                });
                gl.active_texture(glow::TEXTURE0 + index);
                gl.bind_texture(glow::TEXTURE_3D, Some(handle));
                set_texture_parameters(gl, glow::TEXTURE_3D, texture.interpolation());
                let edge = texture.edge_len() as i32;
                gl.tex_image_3d(
                    glow::TEXTURE_3D,
                    0,
                    glow::RGB32F as i32,
                    edge,
                    edge,
                    edge,
                    0,
                    glow::RGB,
                    glow::FLOAT,
                    glow::PixelUnpackData::Slice(Some(float_bytes(texture.values()))),
                );
            }
            for texture in self.desc.textures() {
                if texture.texture_name().is_empty()
                    || texture.sampler_name().is_empty()
                    || texture.width() == 0
                {
                    return Err(Exception::new("The texture data is corrupted"));
                }
                // getTexture's check precedes AllocateTexture's switch upstream.
                let target = match texture.raw_dimensions()? {
                    1 => glow::TEXTURE_1D,
                    2 => glow::TEXTURE_2D,
                    _ => return Err(Exception::new("Invalid 1D LUT texture dimensions")),
                };
                let channels = if texture.channel() == TextureType::RedChannel {
                    1
                } else {
                    3
                };
                let height = if target == glow::TEXTURE_1D {
                    1
                } else {
                    texture.height()
                };
                check_texture_values(&[texture.width(), height], channels, texture.values())?;
                let sampler_name = text(texture.sampler_name())?;
                let handle = gl.create_texture().map_err(Exception::new)?;
                let index = start_index + self.textures.len() as u32;
                self.textures.push(TextureId {
                    handle,
                    sampler_name,
                    target,
                });
                let (internal, format) = match texture.channel() {
                    TextureType::RedChannel => (glow::R32F as i32, glow::RED),
                    TextureType::RgbChannel => (glow::RGB32F as i32, glow::RGB),
                };
                gl.active_texture(glow::TEXTURE0 + index);
                gl.bind_texture(target, Some(handle));
                set_texture_parameters(gl, target, texture.interpolation());
                let pixels = glow::PixelUnpackData::Slice(Some(float_bytes(texture.values())));
                match texture.dimensions() {
                    TextureDimensions::D1 => gl.tex_image_1d(
                        target,
                        0,
                        internal,
                        texture.width() as i32,
                        0,
                        format,
                        glow::FLOAT,
                        pixels,
                    ),
                    TextureDimensions::D2 => gl.tex_image_2d(
                        target,
                        0,
                        internal,
                        texture.width() as i32,
                        texture.height() as i32,
                        0,
                        format,
                        glow::FLOAT,
                        pixels,
                    ),
                }
            }
        }
        Ok(())
    }

    /// Port of OpenGLBuilder::deleteAllTextures (glsl.cpp:394-404 @ v2.5.2).
    fn delete_all_textures(&mut self) {
        // SAFETY: called only with this context current; handles are removed
        // from the owner before deletion and cannot be deleted twice.
        unsafe {
            for texture in self.textures.drain(..) {
                self.context.gl.delete_texture(texture.handle);
            }
        }
    }

    /// Port of OpenGLBuilder::useAllTextures (glsl.cpp:406-419 @ v2.5.2).
    pub(crate) fn use_all_textures(&self) -> Result<()> {
        self.context.activate()?;
        // SAFETY: handles belong to this context, sampler names are valid UTF-8
        // C names (the description already truncates at NUL), values are scalar.
        unsafe {
            let gl = &self.context.gl;
            for (offset, texture) in self.textures.iter().enumerate() {
                let unit = self.start_index + offset as u32;
                gl.active_texture(glow::TEXTURE0 + unit);
                gl.bind_texture(texture.target, Some(texture.handle));
                let location = gl.get_uniform_location(self.program, &texture.sampler_name);
                gl.uniform_1_i32(location.as_ref(), unit as i32);
            }
        }
        Ok(())
    }

    /// Port of OpenGLBuilder::linkAllUniforms and Uniform::setUp
    /// (glsl.cpp:222-234, 421-439 @ v2.5.2).
    fn link_all_uniforms(&mut self) -> Result<()> {
        self.uniforms.clear();
        for uniform in self.desc.uniforms() {
            let name = text(uniform.name())?;
            // SAFETY: program belongs to this current context; GL copies name.
            let location = unsafe { self.context.gl.get_uniform_location(self.program, &name) };
            if check_status(&self.context.gl).is_err() {
                return Err(Exception::new(format!(
                    "Shader parameter {name} not found: "
                )));
            }
            self.uniforms.push(Uniform {
                location,
                data: uniform.data().clone(),
            });
        }
        Ok(())
    }

    /// Port of Uniform::use and OpenGLBuilder::useAllUniforms
    /// (glsl.cpp:236-270, 446-452 @ v2.5.2).
    pub(crate) fn use_all_uniforms(&self) -> Result<()> {
        self.context.activate()?;
        let gl = &self.context.gl;
        for uniform in &self.uniforms {
            let location = uniform.location.as_ref();
            // SAFETY: GL uniforms belong to the current program. Nonnegative
            // counts are checked against slice lengths; GL's defined negative-
            // count error path does not read the borrowed pointer.
            unsafe {
                match &uniform.data {
                    UniformData::Double(get) => gl.uniform_1_f32(location, get() as f32),
                    UniformData::Bool(get) => {
                        gl.uniform_1_f32(location, if get() { 1.0 } else { 0.0 })
                    }
                    UniformData::Float3(get) => {
                        // The exact upstream function-pointer expression calls
                        // this getter three times. Rocky GCC evaluates the
                        // arguments right-to-left at both -O0 and -O2 (I-481).
                        let b = get()[2];
                        let g = get()[1];
                        let r = get()[0];
                        gl.uniform_3_f32(location, r, g, b);
                    }
                    UniformData::VectorFloat { size, values } => {
                        // Rocky GCC evaluates the pointer argument before the
                        // count in upstream's function-pointer call (I-481).
                        let values = values();
                        let count = size();
                        if count < 0 {
                            // GL records INVALID_VALUE without reading the
                            // pointer for a negative count. Keep that pending
                            // error; Uniform::use/setShader do not CheckStatus.
                            let location = location.map_or(-1, |value| value.0 as i32);
                            (self.vector_uniforms.float)(location, count, values.as_ptr());
                        } else {
                            let used = used_values(count, &values)?;
                            gl.uniform_1_f32_slice(location, used);
                        }
                    }
                    UniformData::VectorInt { size, values } => {
                        let values = values();
                        let count = size();
                        if count < 0 {
                            let location = location.map_or(-1, |value| value.0 as i32);
                            (self.vector_uniforms.int)(location, count, values.as_ptr());
                        } else {
                            let used = used_values(count, &values)?;
                            gl.uniform_1_i32_slice(location, used);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Port of OpenGLBuilder::getGLSLVersionString (glsl.cpp:454-480 @ v2.5.2).
    fn glsl_version_string(&self) -> Result<&'static str> {
        match self.desc.language() {
            GpuLanguage::Glsl1_2 | GpuLanguage::Msl2_0 => Ok("#version 120"),
            GpuLanguage::Glsl1_3 => Ok("#version 130"),
            GpuLanguage::Glsl4_0 => Ok("#version 400 core"),
            GpuLanguage::GlslVk4_6 => Ok("#version 460 core"),
            GpuLanguage::GlslEs1_0 => Ok("#version 100"),
            GpuLanguage::GlslEs3_0 => Ok("#version 300 es"),
            _ => Err(Exception::new("Invalid shader language for OpenGLBuilder")),
        }
    }

    /// Port of OpenGLBuilder::buildProgram, CompileShaderText and LinkShaders
    /// (glsl.cpp:148-207, 482-515 @ v2.5.2).
    pub(crate) fn build_program(&mut self, client: &str, standalone: bool) -> Result<()> {
        self.context.activate()?;
        let cache_id = self.desc.cache_id();
        if cache_id == self.shader_cache_id {
            return Ok(());
        }
        let text = format!(
            "{}\n{}\n{}\n",
            self.glsl_version_string()?,
            if standalone {
                ""
            } else {
                std::str::from_utf8(self.desc.shader_text())
                    .map_err(|e| Exception::new(e.to_string()))?
            },
            client
        );
        if self.verbose {
            let mut stdout = std::io::stdout().lock();
            let _ = write!(stdout, "\nGPU Shader Program:\n\n{text}\n\n");
            let _ = stdout.flush();
        }
        check_status(&self.context.gl)?;
        let gl = &self.context.gl;
        // SAFETY: the current context owns shader and program. GLSL source is
        // copied synchronously; newly created handles are saved before errors.
        unsafe {
            if let Some(shader) = self.fragment_shader.take() {
                if self.fragment_shader_attached {
                    gl.detach_shader(self.program, shader);
                }
                gl.delete_shader(shader);
                self.fragment_shader_attached = false;
            }
            let shader = gl
                .create_shader(glow::FRAGMENT_SHADER)
                .map_err(Exception::new)?;
            self.fragment_shader = Some(shader);
            gl.shader_source(shader, &text);
            gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader) {
                let log = gl.get_shader_info_log(shader);
                return Err(Exception::new(
                    [
                        b"OCIO Shader program compilation failed: ".as_slice(),
                        &log.as_bytes()[..log.len().min(999)],
                        b"\n",
                        text.as_bytes(),
                    ]
                    .concat(),
                ));
            }
            check_status(gl)?;
            gl.attach_shader(self.program, shader);
            self.fragment_shader_attached = true;
            gl.link_program(self.program);
            if !gl.get_program_link_status(self.program) {
                let log = gl.get_program_info_log(self.program);
                return Err(Exception::new(
                    [
                        b"Shader link error:\n".as_slice(),
                        &log.as_bytes()[..log.len().min(999)],
                    ]
                    .concat(),
                ));
            }
        }
        self.shader_cache_id = cache_id;
        self.link_all_uniforms()
    }

    /// Port of OpenGLBuilder::useProgram (glsl.cpp:517-520 @ v2.5.2).
    pub(crate) fn use_program(&self) -> Result<()> {
        self.context.activate()?;
        // SAFETY: linked program belongs to current context.
        unsafe {
            self.context.gl.use_program(Some(self.program));
        }
        Ok(())
    }

    /// Image sampler setup of OglApp::setShader (oglapp.cpp:224-225 @ v2.5.2).
    pub(crate) fn bind_image_sampler(&self) -> Result<()> {
        self.context.activate()?;
        // SAFETY: program is current and belongs to this context.
        unsafe {
            let location = self.context.gl.get_uniform_location(self.program, "img");
            self.context.gl.uniform_1_i32(location.as_ref(), 0);
        }
        Ok(())
    }

    /// Port of OpenGLBuilder::GetTextureMaxWidth (glsl.cpp:527-590 @ v2.5.2).
    pub(crate) fn texture_max_width(gl: &glow::Context) -> Result<u32> {
        check_status(gl)?;
        let (mut width, mut height) = (256 * 1024, 1);
        // SAFETY: proxy allocation has no caller buffer; only queries the
        // driver's supported layout and does not allocate a huge texture.
        unsafe {
            while width > 1 {
                gl.tex_image_2d(
                    GL_PROXY_TEXTURE_2D,
                    0,
                    glow::RGB32F as i32,
                    width,
                    height,
                    0,
                    glow::RGB,
                    glow::FLOAT,
                    glow::PixelUnpackData::Slice(None),
                );
                let mut valid = true;
                loop {
                    let error = gl.get_error();
                    if error == glow::NO_ERROR {
                        break;
                    }
                    if error == glow::INVALID_VALUE {
                        valid = false;
                    }
                }
                if valid {
                    valid = gl.get_tex_level_parameter_i32(
                        GL_PROXY_TEXTURE_2D,
                        0,
                        GL_TEXTURE_COMPONENTS,
                    ) == glow::RGB32F as i32;
                    while gl.get_error() != glow::NO_ERROR {}
                }
                if valid {
                    break;
                }
                width >>= 1;
                height <<= 1;
            }
        }
        if width == 1 {
            return Err(Exception::new("Maximum texture size unknown"));
        }
        check_status(gl)?;
        Ok(width as u32)
    }
}

impl Drop for OpenGLBuilder {
    fn drop(&mut self) {
        if self.context.activate().is_err() {
            return;
        }
        self.delete_all_textures();
        // SAFETY: all handles belong to the current context; this is their only
        // owning builder and every shader/program is deleted once.
        unsafe {
            if let Some(shader) = self.fragment_shader.take() {
                // CompileShaderText throws before upstream assigns m_fragShader
                // (glsl.cpp:148-182,506). We own the handle early for cleanup,
                // but a compile/check-status failure has not attached it yet.
                if self.fragment_shader_attached {
                    self.context.gl.detach_shader(self.program, shader);
                }
                self.context.gl.delete_shader(shader);
                self.fragment_shader_attached = false;
            }
            self.context.gl.delete_program(self.program);
        }
    }
}

/// Port of SetTextureParameters (glsl.cpp:60-76 @ v2.5.2).
fn set_texture_parameters(gl: &glow::Context, target: u32, interpolation: Interpolation) {
    let filter = if interpolation == Interpolation::Nearest {
        glow::NEAREST
    } else {
        glow::LINEAR
    } as i32;
    // SAFETY: callers bind a texture of this target in the current context.
    unsafe {
        gl.tex_parameter_i32(target, glow::TEXTURE_MIN_FILTER, filter);
        gl.tex_parameter_i32(target, glow::TEXTURE_MAG_FILTER, filter);
        for axis in [
            glow::TEXTURE_WRAP_S,
            glow::TEXTURE_WRAP_T,
            glow::TEXTURE_WRAP_R,
        ] {
            gl.tex_parameter_i32(target, axis, glow::CLAMP_TO_EDGE as i32);
        }
    }
}

fn text(bytes: &[u8]) -> Result<String> {
    String::from_utf8(bytes.to_vec()).map_err(|e| Exception::new(e.to_string()))
}

fn used_values<T>(count: i32, values: &[T]) -> Result<&[T]> {
    let count = usize::try_from(count).map_err(|_| Exception::new("invalid value"))?;
    values
        .get(..count)
        .ok_or_else(|| Exception::new("Uniform vector size exceeds its value buffer."))
}

fn check_texture_values(dimensions: &[u32], channels: usize, values: &[f32]) -> Result<()> {
    let count = dimensions.iter().try_fold(channels, |count, &dimension| {
        let dimension = i32::try_from(dimension).ok()?;
        count.checked_mul(dimension as usize)
    });
    if count.is_none_or(|count| count > values.len()) {
        return Err(Exception::new(
            "The texture values do not match its dimensions.",
        ));
    }
    Ok(())
}
