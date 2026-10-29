// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/libutils/oglapphelpers/oglapp.cpp/.h` @ v2.5.2: the headless
//! EGL context, image upload, fixed-function quad and floating-point readback.
#![allow(unsafe_code)]

use std::ffi::c_void;
use std::fmt;
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};

use glow::HasContext;
use khronos_egl as egl;
use ocio_gpu::GpuShaderDesc;
use ocio_ops::{Exception, Result};

use crate::glsl::OpenGLBuilder;

const GL_CLAMP: i32 = 0x2900;
const GL_QUADS: u32 = 0x0007;
const GL_PROJECTION: u32 = 0x1701;
const GL_MODELVIEW: u32 = 0x1700;
const GL_CLAMP_VERTEX_COLOR: u32 = 0x891A;
const GL_CLAMP_FRAGMENT_COLOR: u32 = 0x891B;
const GL_CLAMP_READ_COLOR: u32 = 0x891C;

// EGL_DEFAULT_DISPLAY is shared by the process. EGL initialization is not
// reference-counted by the driver: terminate only after the last test context,
// or one parallel test would invalidate another's still-live context.
static EGL_CONNECTIONS: Mutex<usize> = Mutex::new(0);

/// The image components upstream's OglApp accepts (oglapp.h:83-87 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Components {
    Rgb,
    Rgba,
}

impl Components {
    fn count(self) -> usize {
        match self {
            Self::Rgb => 3,
            Self::Rgba => 4,
        }
    }

    fn format(self) -> u32 {
        match self {
            Self::Rgb => glow::RGB,
            Self::Rgba => glow::RGBA,
        }
    }
}

/// The platform's renderer identity, as OglApp::printGLInfo/printEGLInfo report
/// it (oglapp.cpp:230-237, 375-380 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct GlInfo {
    pub vendor: String,
    pub renderer: String,
    pub version: String,
    pub shading_language_version: String,
    pub egl_vendor: String,
    pub egl_version: String,
    /// The driver's GL_MAX_TEXTURE_SIZE, an independent platform reference for
    /// OpenGLBuilder's proxy-texture search on the pinned llvmpipe renderer.
    pub max_texture_size: i32,
}

/// A connection owns its context and pbuffer, including during a failed setup.
struct EglConnection {
    api: egl::DynamicInstance<egl::EGL1_4>,
    display: egl::Display,
    surface: Option<egl::Surface>,
    context: Option<egl::Context>,
}

impl EglConnection {
    /// Port of HeadlessApp::HeadlessApp (oglapp.cpp:309-362 @ v2.5.2).
    fn new(width: i32, height: i32) -> Result<Self> {
        // SAFETY: libEGL is the platform implementation of the EGL ABI. The
        // loaded library stays owned by `api` until every EGL object is gone.
        let api = unsafe {
            egl::DynamicInstance::<egl::EGL1_4>::load_required_from_filename("libEGL.so.1")
        }
        .map_err(|_| Exception::new("EGL could not be initialized."))?;
        let mut connections = EGL_CONNECTIONS
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        // SAFETY: EGL_DEFAULT_DISPLAY is the null NativeDisplayType, as upstream.
        let display = unsafe { api.get_display(egl::DEFAULT_DISPLAY) }
            .ok_or_else(|| Exception::new("EGL could not be initialized."))?;
        api.initialize(display)
            .map_err(|_| Exception::new("EGL display connection couldn't be started."))?;
        *connections += 1;
        drop(connections);
        let mut connection = Self {
            api,
            display,
            surface: None,
            context: None,
        };
        let attributes = [
            egl::SURFACE_TYPE,
            egl::PBUFFER_BIT,
            egl::BLUE_SIZE,
            8,
            egl::GREEN_SIZE,
            8,
            egl::RED_SIZE,
            8,
            egl::DEPTH_SIZE,
            8,
            egl::RENDERABLE_TYPE,
            egl::OPENGL_BIT,
            egl::NONE,
        ];
        let config = connection
            .api
            .choose_first_config(display, &attributes)
            .map_err(|_| Exception::new("Failed to choose EGL configuration."))?
            .ok_or_else(|| Exception::new("Failed to choose EGL configuration."))?;
        connection.surface = Some(
            connection
                .api
                .create_pbuffer_surface(
                    display,
                    config,
                    &[egl::WIDTH, width, egl::HEIGHT, height, egl::NONE],
                )
                .map_err(|_| Exception::new("Could not create EGL pbuffer."))?,
        );
        connection
            .api
            .bind_api(egl::OPENGL_API)
            .map_err(|_| Exception::new("Could not select desktop OpenGL."))?;
        connection.context = Some(
            connection
                .api
                .create_context(display, config, None, &[egl::NONE])
                .map_err(|_| Exception::new("Could not make EGL context current."))?,
        );
        connection.activate()?;
        Ok(connection)
    }

    /// The eglMakeCurrent of HeadlessApp's setup (oglapp.cpp:362-365 @ v2.5.2).
    fn activate(&self) -> Result<()> {
        self.api
            .make_current(self.display, self.surface, self.surface, self.context)
            .map_err(|_| Exception::new("Could not make EGL context current."))
    }
}

impl Drop for EglConnection {
    fn drop(&mut self) {
        // Upstream terminates the connection (oglapp.cpp:364-367 @ v2.5.2).
        // Release its two objects explicitly too, so repeated tests cannot leak
        // deferred contexts while another test still uses this EGL display.
        let mut connections = EGL_CONNECTIONS
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let _ = self.api.make_current(self.display, None, None, None);
        if let Some(context) = self.context.take() {
            let _ = self.api.destroy_context(self.display, context);
        }
        if let Some(surface) = self.surface.take() {
            let _ = self.api.destroy_surface(self.display, surface);
        }
        *connections -= 1;
        if *connections == 0 {
            let _ = self.api.terminate(self.display);
        }
    }
}

/// Fixed-function desktop entry points which glow's core API does not expose.
struct LegacyGl {
    begin: unsafe extern "system" fn(u32),
    end: unsafe extern "system" fn(),
    color3f: unsafe extern "system" fn(f32, f32, f32),
    tex_coord2f: unsafe extern "system" fn(f32, f32),
    vertex2f: unsafe extern "system" fn(f32, f32),
    push_matrix: unsafe extern "system" fn(),
    pop_matrix: unsafe extern "system" fn(),
    matrix_mode: unsafe extern "system" fn(u32),
    load_identity: unsafe extern "system" fn(),
    ortho: unsafe extern "system" fn(f64, f64, f64, f64, f64, f64),
    clamp_color: unsafe extern "system" fn(u32, u32),
}

impl LegacyGl {
    fn load(api: &egl::DynamicInstance<egl::EGL1_4>) -> Result<Self> {
        macro_rules! load {
            ($name:literal, $signature:ty) => {{
                let pointer = api.get_proc_address($name).ok_or_else(|| {
                    Exception::new(concat!("OpenGL entry point missing: ", $name))
                })?;
                // SAFETY: each signature is the named desktop OpenGL entry
                // point's ABI, obtained from the current EGL implementation.
                unsafe { std::mem::transmute::<extern "system" fn(), $signature>(pointer) }
            }};
        }
        Ok(Self {
            begin: load!("glBegin", unsafe extern "system" fn(u32)),
            end: load!("glEnd", unsafe extern "system" fn()),
            color3f: load!("glColor3f", unsafe extern "system" fn(f32, f32, f32)),
            tex_coord2f: load!("glTexCoord2f", unsafe extern "system" fn(f32, f32)),
            vertex2f: load!("glVertex2f", unsafe extern "system" fn(f32, f32)),
            push_matrix: load!("glPushMatrix", unsafe extern "system" fn()),
            pop_matrix: load!("glPopMatrix", unsafe extern "system" fn()),
            matrix_mode: load!("glMatrixMode", unsafe extern "system" fn(u32)),
            load_identity: load!("glLoadIdentity", unsafe extern "system" fn()),
            ortho: load!(
                "glOrtho",
                unsafe extern "system" fn(f64, f64, f64, f64, f64, f64)
            ),
            clamp_color: load!("glClampColor", unsafe extern "system" fn(u32, u32)),
        })
    }
}

/// Rc confines all GL resources and calls to the creating thread.
pub(crate) struct GlContext {
    pub(crate) gl: glow::Context,
    egl: EglConnection,
    legacy: LegacyGl,
}

impl fmt::Debug for GlContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GlContext").finish_non_exhaustive()
    }
}

impl GlContext {
    /// Resolve a desktop GL entry point while this context's EGL library lives.
    /// ABI typing remains inside the calling allowlisted module.
    pub(crate) fn proc_address(&self, name: &str) -> Result<*const c_void> {
        self.egl
            .api
            .get_proc_address(name)
            .map(|function| function as *const c_void)
            .ok_or_else(|| Exception::new(format!("OpenGL entry point missing: {name}")))
    }

    fn new(width: i32, height: i32) -> Result<Rc<Self>> {
        let egl = EglConnection::new(width, height)?;
        // SAFETY: this thread has the desktop context current. EGL keeps the
        // library and context alive for every use of the loaded GL entry points.
        let gl = unsafe {
            glow::Context::from_loader_function(|name| {
                egl.api
                    .get_proc_address(name)
                    .map_or(std::ptr::null(), |f| f as *const c_void)
            })
        };
        let legacy = LegacyGl::load(&egl.api)?;
        let context = Rc::new(Self { gl, egl, legacy });
        // Port of OglApp::setupCommon (oglapp.cpp:239-263 @ v2.5.2).
        if context.gl.version().major < 2 {
            return Err(Exception::new("OpenGL 2.0 not supported."));
        }
        // SAFETY: context is current, enum arguments are desktop GL constants.
        unsafe {
            context.gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
            (context.legacy.clamp_color)(GL_CLAMP_READ_COLOR, glow::FALSE as u32);
            (context.legacy.clamp_color)(GL_CLAMP_VERTEX_COLOR, glow::FALSE as u32);
            (context.legacy.clamp_color)(GL_CLAMP_FRAGMENT_COLOR, glow::FALSE as u32);
            context.gl.enable(glow::TEXTURE_2D);
        }
        Ok(context)
    }

    pub(crate) fn activate(&self) -> Result<()> {
        self.egl.activate()
    }
}

/// Upstream's OglApp with HeadlessApp's EGL surface. The Rc context deliberately
/// makes the application neither Send nor Sync.
#[derive(Debug)]
pub struct HeadlessApp {
    context: Rc<GlContext>,
    builder: Option<OpenGLBuilder>,
    image_texture: Option<glow::Texture>,
    framebuffer: Option<glow::Framebuffer>,
    renderbuffer: Option<glow::Renderbuffer>,
    viewport_width: i32,
    viewport_height: i32,
    image_width: i32,
    image_height: i32,
    image_aspect: f32,
    components: Components,
    y_mirror: bool,
    print_shader: bool,
}

impl HeadlessApp {
    /// Port of OglApp/HeadlessApp constructors (oglapp.cpp:36-39, 309-362 @ v2.5.2).
    pub fn new(width: i32, height: i32) -> Result<Self> {
        if width <= 0 || height <= 0 {
            return Err(Exception::new(
                "The headless image dimensions must be positive.",
            ));
        }
        Ok(Self {
            context: GlContext::new(width, height)?,
            builder: None,
            image_texture: None,
            framebuffer: None,
            renderbuffer: None,
            viewport_width: width,
            viewport_height: height,
            image_width: 0,
            image_height: 0,
            image_aspect: 1.0,
            components: Components::Rgba,
            y_mirror: false,
            print_shader: false,
        })
    }

    /// Port of OglApp::setYMirror (oglapp.h:72-75 @ v2.5.2).
    pub fn set_y_mirror(&mut self) {
        self.y_mirror = true;
    }

    /// Port of OglApp::setPrintShader (oglapp.h:78-81 @ v2.5.2).
    pub fn set_print_shader(&mut self, print: bool) {
        self.print_shader = print;
    }

    /// Port of OglApp::printGLInfo and HeadlessApp::printEGLInfo
    /// (oglapp.cpp:230-237, 375-380 @ v2.5.2), returned for assertions.
    pub fn info(&self) -> Result<GlInfo> {
        self.context.activate()?;
        // SAFETY: the context is current; strings are copied out of GL/EGL.
        unsafe {
            let egl_string = |name| {
                self.context
                    .egl
                    .api
                    .query_string(Some(self.context.egl.display), name)
                    .map(|s| s.to_string_lossy().into_owned())
                    .map_err(|e| Exception::new(e.to_string()))
            };
            Ok(GlInfo {
                vendor: self.context.gl.get_parameter_string(glow::VENDOR),
                renderer: self.context.gl.get_parameter_string(glow::RENDERER),
                version: self.context.gl.get_parameter_string(glow::VERSION),
                shading_language_version: self
                    .context
                    .gl
                    .get_parameter_string(glow::SHADING_LANGUAGE_VERSION),
                egl_vendor: egl_string(egl::VENDOR)?,
                egl_version: egl_string(egl::VERSION)?,
                max_texture_size: self.context.gl.get_parameter_i32(glow::MAX_TEXTURE_SIZE),
            })
        }
    }

    /// Port of OglApp::setImageDimensions/initImage (oglapp.cpp:46-71 @ v2.5.2).
    pub fn init_image(
        &mut self,
        width: i32,
        height: i32,
        components: Components,
        image: &[f32],
    ) -> Result<()> {
        self.context.activate()?;
        let count = image_count(width, height, components)?;
        if image.len() != count {
            return Err(Exception::new(
                "The image buffer does not match its dimensions.",
            ));
        }
        self.image_width = width;
        self.image_height = height;
        self.components = components;
        self.image_aspect = width as f32 / height as f32;
        // SAFETY: the context is current, texture is created here, byte view is
        // exactly the validated image slice and glTexImage copies it synchronously.
        unsafe {
            let gl = &self.context.gl;
            if let Some(texture) = self.image_texture.take() {
                gl.delete_texture(texture);
            }
            self.image_texture = Some(gl.create_texture().map_err(Exception::new)?);
            self.update_image(image)?;
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::LINEAR as i32,
            );
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, GL_CLAMP);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, GL_CLAMP);
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_R,
                glow::CLAMP_TO_EDGE as i32,
            );
        }
        Ok(())
    }

    /// Port of OglApp::updateImage (oglapp.cpp:73-82 @ v2.5.2).
    pub fn update_image(&self, image: &[f32]) -> Result<()> {
        self.context.activate()?;
        if self.image_texture.is_none()
            || image.len() != image_count(self.image_width, self.image_height, self.components)?
        {
            return Err(Exception::new(
                "The image buffer does not match its dimensions.",
            ));
        }
        // SAFETY: the allocated texture belongs to this current context. The
        // slice has the exact RGB/RGBA dimensions supplied to the synchronous call.
        unsafe {
            let gl = &self.context.gl;
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, self.image_texture);
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA32F as i32,
                self.image_width,
                self.image_height,
                0,
                self.components.format(),
                glow::FLOAT,
                glow::PixelUnpackData::Slice(Some(float_bytes(image))),
            );
        }
        Ok(())
    }

    /// Port of OglApp::createGLBuffers (oglapp.cpp:165-190 @ v2.5.2).
    pub fn create_gl_buffers(&mut self) -> Result<()> {
        self.context.activate()?;
        if self.image_texture.is_none() {
            return Err(Exception::new("The image has not been initialized."));
        }
        // SAFETY: every object is created and used in this context, and the
        // attachments have the image dimensions validated by init_image.
        unsafe {
            let gl = &self.context.gl;
            if let Some(buffer) = self.framebuffer.take() {
                gl.delete_framebuffer(buffer);
            }
            if let Some(buffer) = self.renderbuffer.take() {
                gl.delete_renderbuffer(buffer);
            }
            self.framebuffer = Some(gl.create_framebuffer().map_err(Exception::new)?);
            gl.bind_framebuffer(glow::FRAMEBUFFER, self.framebuffer);
            self.renderbuffer = Some(gl.create_renderbuffer().map_err(Exception::new)?);
            gl.bind_renderbuffer(glow::RENDERBUFFER, self.renderbuffer);
            gl.renderbuffer_storage(
                glow::RENDERBUFFER,
                glow::RGBA32F,
                self.image_width,
                self.image_height,
            );
            gl.bind_renderbuffer(glow::RENDERBUFFER, None);
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT1,
                glow::TEXTURE_2D,
                self.image_texture,
                0,
            );
            gl.framebuffer_renderbuffer(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::RENDERBUFFER,
                self.renderbuffer,
            );
            if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
                return Err(Exception::new("The OpenGL framebuffer is incomplete."));
            }
        }
        Ok(())
    }

    /// Port of OglApp::reshape (oglapp.cpp:152-163 @ v2.5.2).
    /// gluOrtho2D is its documented glOrtho(left,right,bottom,top,-1,1).
    pub fn reshape(&mut self, width: i32, height: i32) -> Result<()> {
        self.context.activate()?;
        self.viewport_width = width;
        self.viewport_height = height;
        // SAFETY: compatibility context current, fixed-function matrices own
        // no caller memory and the entry-point signatures match desktop OpenGL.
        unsafe {
            let gl = &self.context.gl;
            let legacy = &self.context.legacy;
            gl.viewport(0, 0, width, height);
            (legacy.matrix_mode)(GL_PROJECTION);
            (legacy.load_identity)();
            (legacy.ortho)(0.0, width as f64, 0.0, height as f64, -1.0, 1.0);
            (legacy.matrix_mode)(GL_MODELVIEW);
            (legacy.load_identity)();
        }
        Ok(())
    }

    /// Port of OglApp::setShader (oglapp.cpp:196-228 @ v2.5.2).
    pub fn set_shader(&mut self, desc: Arc<GpuShaderDesc>) -> Result<()> {
        self.context.activate()?;
        let function_name =
            std::str::from_utf8(desc.function_name()).map_err(|e| Exception::new(e.to_string()))?;
        let client = format!(
            "\nuniform sampler2D img;\n\nvoid main()\n{{\n    vec4 col = texture2D(img, gl_TexCoord[0].st);\n    gl_FragColor = {function_name}(col);\n}}\n"
        );
        self.builder = Some(OpenGLBuilder::new(self.context.clone(), desc)?);
        let builder = self.builder.as_mut().expect("builder just initialized");
        builder.set_verbose(self.print_shader);
        builder.allocate_all_textures(1)?;
        builder.build_program(&client, false)?;
        builder.use_program()?;
        builder.bind_image_sampler()?;
        builder.use_all_textures()?;
        builder.use_all_uniforms()
    }

    /// Port of OglApp::redisplay, then HeadlessApp::redisplay's eglSwapBuffers
    /// (oglapp.cpp:83-150, 382-387 @ v2.5.2).
    pub fn redisplay(&self) -> Result<()> {
        self.context.activate()?;
        if let Some(builder) = &self.builder {
            builder.use_all_uniforms()?;
        }
        let aspect = if self.viewport_height == 0 {
            1.0
        } else {
            self.viewport_width as f32 / self.viewport_height as f32
        };
        let mut points = [0.0f32; 4];
        if aspect >= self.image_aspect {
            let width = self.image_aspect * self.viewport_height as f32;
            points[0] = self.viewport_width as f32 * 0.5 - width * 0.5;
            points[2] = self.viewport_width as f32 * 0.5 + width * 0.5;
            points[3] = self.viewport_height as f32;
        } else {
            let height = self.viewport_width as f32 / self.image_aspect;
            points[2] = self.viewport_width as f32;
            points[1] = self.viewport_height as f32 * 0.5 - height * 0.5;
            points[3] = self.viewport_height as f32 * 0.5 + height * 0.5;
        }
        if self.y_mirror {
            points.swap(1, 3);
        }
        // SAFETY: current compatibility context, complete FBO, immediate-mode
        // vertex arguments are values and no pointers outlive the calls.
        unsafe {
            let gl = &self.context.gl;
            let legacy = &self.context.legacy;
            gl.enable(glow::TEXTURE_2D);
            gl.clear_color(0.2, 0.2, 0.2, 0.0);
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
            (legacy.color3f)(1.0, 1.0, 1.0);
            (legacy.push_matrix)();
            (legacy.begin)(GL_QUADS);
            (legacy.tex_coord2f)(0.0, 1.0);
            (legacy.vertex2f)(points[0], points[3]);
            (legacy.tex_coord2f)(0.0, 0.0);
            (legacy.vertex2f)(points[0], points[1]);
            (legacy.tex_coord2f)(1.0, 0.0);
            (legacy.vertex2f)(points[2], points[1]);
            (legacy.tex_coord2f)(1.0, 1.0);
            (legacy.vertex2f)(points[2], points[3]);
            (legacy.end)();
            (legacy.pop_matrix)();
            gl.disable(glow::TEXTURE_2D);
        }
        self.context
            .egl
            .api
            .swap_buffers(
                self.context.egl.display,
                self.context.egl.surface.expect("pbuffer exists"),
            )
            .map_err(|e| Exception::new(e.to_string()))
    }

    /// Render an actual wheel description on this same context without replacing
    /// the port's compiled program or its live dynamic getters. The original GL
    /// bindings are restored even when reference compilation/rendering fails.
    /// This is the Phase 7 wheel-shader identity check, in addition to upstream's run.
    pub fn render_reference_shader(&mut self, desc: Arc<GpuShaderDesc>) -> Result<Vec<f32>> {
        if self.builder.is_none() {
            return Err(Exception::new(
                "Reference rendering requires an existing shader",
            ));
        }
        let original = self.builder.take();
        let result = self.set_shader(desc).and_then(|()| {
            self.redisplay()?;
            self.read_image()
        });
        drop(self.builder.take());
        self.builder = original;
        if let Some(builder) = &self.builder {
            builder.use_program()?;
            builder.bind_image_sampler()?;
            builder.use_all_textures()?;
        }
        result
    }

    /// Values from the last actual uniform upload; does not call any getter.
    pub fn uniform_uploads(&self) -> Vec<crate::UniformUpload> {
        self.builder
            .as_ref()
            .map_or_else(Vec::new, OpenGLBuilder::uniform_uploads)
    }

    /// Port of OglApp::readImage (oglapp.cpp:189-194 @ v2.5.2).
    pub fn read_image(&self) -> Result<Vec<f32>> {
        self.context.activate()?;
        let mut image =
            vec![0.0f32; image_count(self.image_width, self.image_height, self.components)?];
        // SAFETY: byte view spans exactly the allocated initialized floats. GL
        // writes only the validated packed RGB/RGBA image into it synchronously.
        unsafe {
            let bytes = std::slice::from_raw_parts_mut(
                image.as_mut_ptr().cast::<u8>(),
                std::mem::size_of_val(image.as_slice()),
            );
            self.context.gl.read_buffer(glow::COLOR_ATTACHMENT0);
            self.context.gl.read_pixels(
                0,
                0,
                self.image_width,
                self.image_height,
                self.components.format(),
                glow::FLOAT,
                glow::PixelPackData::Slice(Some(bytes)),
            );
        }
        crate::glsl::check_status(&self.context.gl)?;
        Ok(image)
    }

    /// Port of OpenGLBuilder::GetTextureMaxWidth (glsl.cpp:527-590 @ v2.5.2).
    pub fn texture_max_width(&self) -> Result<u32> {
        self.context.activate()?;
        OpenGLBuilder::texture_max_width(&self.context.gl)
    }
}

impl Drop for HeadlessApp {
    fn drop(&mut self) {
        if self.context.activate().is_ok() {
            self.builder.take();
            // SAFETY: these handles were allocated in the now-current context,
            // and no longer referenced by a builder; no handle is deleted twice.
            unsafe {
                if let Some(texture) = self.image_texture.take() {
                    self.context.gl.delete_texture(texture);
                }
                if let Some(buffer) = self.framebuffer.take() {
                    self.context.gl.delete_framebuffer(buffer);
                }
                if let Some(buffer) = self.renderbuffer.take() {
                    self.context.gl.delete_renderbuffer(buffer);
                }
            }
        }
    }
}

fn image_count(width: i32, height: i32, components: Components) -> Result<usize> {
    if width <= 0 || height <= 0 {
        return Err(Exception::new(
            "The headless image dimensions must be positive.",
        ));
    }
    (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(components.count()))
        .ok_or_else(|| Exception::new("The image dimensions exceed the buffer size limit."))
}

pub(crate) fn float_bytes(values: &[f32]) -> &[u8] {
    // SAFETY: every f32 has a fully initialized representation, u8 has alignment
    // one, and the read-only byte view has the same lifetime and complete size.
    unsafe {
        std::slice::from_raw_parts(values.as_ptr().cast::<u8>(), std::mem::size_of_val(values))
    }
}
