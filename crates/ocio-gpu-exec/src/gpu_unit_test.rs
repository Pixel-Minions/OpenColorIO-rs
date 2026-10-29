// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/gpu/GPUUnitTest.h` and `GPUUnitTest.cpp:26-526,661-753` @ v2.5.2.
//! CPU expectations come only from the actual wheel, never this port's processor.

use std::sync::Arc;

use ocio::{Config, Processor, ProcessorCacheFlags, Transform};
use ocio_gpu::GpuShaderDesc;
use ocio_ops::open_color_types::OptimizationFlags;
use ocio_ops::{Exception, Result};
use ocio_testkit::gpu::{GpuShaderReply, Shader, ShaderSettings};
use ocio_testkit::upstream::gpu::{
    COMPONENTS, ComparisonSettings, ENTRIES, HEIGHT, ImageComparison, WIDTH, compare_image,
    comparison_dimensions,
};

use crate::nonfinite_class::{self, Companion, Environment, Frame};
use crate::wheel_shader;
use crate::{Components, HeadlessApp};

// Upstream's callback captures its OCIOGPUTest by reference. Borrowing the actual
// extracted description/processor at invocation avoids capturing or pre-extracting it.
type RetestCallback = Box<dyn FnMut(&GpuShaderDesc, &Processor)>;

/// `OCIOGPUTest::CustomValues` (`GPUUnitTest.h:30-41` @ v2.5.2).
#[derive(Debug, Clone, Default)]
pub struct CustomValues {
    /// Input values, padded to the fixed image size after preparation.
    pub input_values: Vec<f32>,
    /// Count before padding, used to choose the CPU/comparison rectangle.
    pub original_input_value_size: usize,
}

/// One actual wheel CPU/shader result, prepared before GL execution. A test file
/// sends its CPU requests together in one uncached `Oracle::batch`.
#[derive(Debug, Clone)]
pub struct ReferenceRun {
    /// Actual wheel CPU float bits for the comparison rectangle.
    pub cpu: Vec<f32>,
    /// Actual uncached wheel CPU result for the entire padded image after only
    /// non-finite image inputs become zero. Required for the approved class;
    /// no companion is needed when the original image is entirely finite.
    pub finite_cpu: Option<Vec<f32>>,
    /// Actual ordinary or native-legacy wheel shader reply.
    pub gpu: GpuShaderReply,
    /// Original descriptor setter arguments; normalization runs exactly once.
    pub shader_settings: ShaderSettings,
}

/// The upstream test object. Rust test functions replace main's callback registry.
/// Defaults are `GPUUnitTest.h:157-173` and `GPUUnitTest.cpp:30,127-134` @ v2.5.2.
pub struct GpuUnitTest {
    /// Upstream group name.
    pub group: String,
    /// Upstream test name.
    pub name: String,
    processor: Option<Arc<Processor>>,
    shader_desc: Option<GpuShaderDesc>,
    /// Original shader setter arguments, applied lazily by `shader_desc`.
    pub shader_settings: ShaderSettings,
    /// Upstream numeric/special comparison settings.
    pub comparison: ComparisonSettings,
    /// Generate the wide rather than [0,1] ramp.
    pub test_wide_range: bool,
    /// Wide ramp interval's lower bound.
    pub range_min: f32,
    /// Wide ramp interval's upper bound.
    pub range_max: f32,
    /// Include one NaN in each input channel.
    pub test_nan: bool,
    /// Include positive/negative infinity in each input channel.
    pub test_infinity: bool,
    /// Dump the shader when compiling it.
    pub verbose: bool,
    /// Upstream's enabled state; disabled cases need their reviewed n/a handling.
    pub enabled: bool,
    /// Use getOptimizedLegacyGPUProcessor rather than getDefaultGPUProcessor.
    pub legacy_shader: bool,
    /// The legacy lattice's side length, default 32.
    pub legacy_shader_lut_edge: u32,
    /// Custom or generated values.
    pub values: CustomValues,
    values_prepared: bool,
    retests: Vec<RetestCallback>,
    /// Largest difference across all runs, updated only on a strict increase.
    pub max_diff: f32,
    /// Component index of that strict increase; upstream initializes it to zero.
    pub max_diff_index: usize,
}

impl std::fmt::Debug for GpuUnitTest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GpuUnitTest")
            .field("group", &self.group)
            .field("name", &self.name)
            .field("valid", &self.processor.is_some())
            .field("comparison", &self.comparison)
            .field(
                "original_input_value_size",
                &self.values.original_input_value_size,
            )
            .field("num_retests", &self.num_retests())
            .field("legacy_shader", &self.legacy_shader)
            .field("legacy_shader_lut_edge", &self.legacy_shader_lut_edge)
            .finish_non_exhaustive()
    }
}

impl GpuUnitTest {
    /// Port of `OCIOGPUTest::OCIOGPUTest` and member initializers @ v2.5.2.
    pub fn new(group: &str, name: &str) -> Self {
        Self {
            group: group.into(),
            name: name.into(),
            processor: None,
            shader_desc: None,
            shader_settings: ShaderSettings {
                language: Some(ocio_testkit::gpu::GpuLanguage::Glsl12),
                pixel_name: Some("myPixel".into()),
                ..ShaderSettings::default()
            },
            comparison: ComparisonSettings::default(),
            test_wide_range: true,
            range_min: -1.0,
            range_max: 2.0,
            test_nan: true,
            test_infinity: true,
            verbose: false,
            enabled: true,
            legacy_shader: false,
            legacy_shader_lut_edge: 32,
            values: CustomValues::default(),
            values_prepared: false,
            retests: Vec::new(),
            max_diff: 0.0,
            max_diff_index: 0,
        }
    }

    /// Port of the transform-only setProcessor (`GPUUnitTest.cpp:137-142` @ v2.5.2).
    pub fn set_processor_transform(&mut self, transform: &Transform) -> Result<()> {
        let config = Config::new()?;
        config.set_processor_cache_flags(ProcessorCacheFlags::OFF);
        self.set_processor_config(&config, transform)
    }

    /// Port of config/transform setProcessor (`GPUUnitTest.cpp:144-148` @ v2.5.2).
    pub fn set_processor_config(&mut self, config: &Config, transform: &Transform) -> Result<()> {
        self.set_processor(config.processor(transform)?)
    }

    /// Port of processor setProcessor (`GPUUnitTest.cpp:150-158` @ v2.5.2).
    pub fn set_processor(&mut self, processor: Arc<Processor>) -> Result<()> {
        if self.processor.is_some() {
            return Err(Exception::new("GPU Unit test already exists"));
        }
        self.processor = Some(processor);
        Ok(())
    }

    /// The processor shared by setup callbacks and the later GPU extraction.
    pub fn processor(&self) -> Option<&Arc<Processor>> {
        self.processor.as_ref()
    }

    /// Port of getShaderDesc (`GPUUnitTest.cpp:160-170` @ v2.5.2).
    pub fn shader_desc(&mut self) -> Result<&mut GpuShaderDesc> {
        match self.shader_desc {
            Some(ref mut desc) => Ok(desc),
            None => {
                let desc = wheel_shader::description(&self.shader_settings)?;
                Ok(self.shader_desc.insert(desc))
            }
        }
    }

    /// Port of addRetest (`GPUUnitTest.h:126-129` @ v2.5.2). The callback receives
    /// the actual description after extraction, and the original processor;
    /// their decoupled properties match ECOp_test.cpp:163-185's first-retest lookup.
    pub fn add_retest(&mut self, callback: impl FnMut(&GpuShaderDesc, &Processor) + 'static) {
        self.retests.push(Box::new(callback));
    }

    /// Number of callbacks; the initial run is additional.
    pub fn num_retests(&self) -> usize {
        self.retests.len()
    }

    /// Port of setCustomValues (`GPUUnitTest.h:76-77` @ v2.5.2).
    pub fn set_custom_values(&mut self, values: CustomValues) {
        self.values = values;
        self.values_prepared = false;
    }

    /// Port of retestSetup (`GPUUnitTest.cpp:172-178` @ v2.5.2).
    pub fn retest_setup(
        &mut self,
        index: usize,
        shader_desc: &GpuShaderDesc,
        processor: &Processor,
    ) {
        if let Some(callback) = self.retests.get_mut(index) {
            callback(shader_desc, processor);
        }
    }

    /// Port of SetTestValue (`GPUUnitTest.cpp:212-222` @ v2.5.2).
    fn set_test_value(image: &mut [f32], value: f32) {
        for component in 0..COMPONENTS {
            for index in 0..COMPONENTS {
                image[index + component * COMPONENTS] =
                    if index == component { value } else { 0.0 };
            }
        }
    }

    /// Port of UpdateImageTexture (`GPUUnitTest.cpp:225-329` @ v2.5.2).
    /// Every arithmetic step is f32; no fused multiply-add substitutes the ramp.
    pub fn prepare_values(&mut self) -> Result<()> {
        if self.values_prepared {
            return Ok(());
        }
        if self.values.input_values.is_empty() {
            let (min, max) = if self.test_wide_range {
                (self.range_min, self.range_max)
            } else {
                (0.0, 1.0)
            };
            let range = max - min;
            let mut values = vec![min; ENTRIES];
            let mut index = 0;
            if self.test_nan {
                Self::set_test_value(&mut values[index..], f32::NAN);
                index += COMPONENTS * COMPONENTS;
            }
            if self.test_infinity {
                Self::set_test_value(&mut values[index..], f32::INFINITY);
                index += COMPONENTS * COMPONENTS;
                Self::set_test_value(&mut values[index..], f32::NEG_INFINITY);
                index += COMPONENTS * COMPONENTS;
            }
            let entries = ENTRIES - index;
            let step = range / entries as f32;
            for offset in 0..entries {
                values[index + offset] = min + step * offset as f32;
            }
            self.values = CustomValues {
                input_values: values,
                original_input_value_size: ENTRIES,
            };
        } else {
            let length = self.values.input_values.len();
            if !length.is_multiple_of(COMPONENTS) {
                return Err(Exception::new("Only the RGBA input values are supported"));
            }
            self.values.original_input_value_size = length;
            if length > ENTRIES {
                return Err(Exception::new("Exceed the predefined texture maximum size"));
            }
            self.values.input_values.resize(ENTRIES, 0.0);
        }
        if self.values.input_values.len() != ENTRIES {
            return Err(Exception::new("Missing some expected input values"));
        }
        self.values_prepared = true;
        Ok(())
    }

    /// The rectangle includes a custom image's partially padded final row, exactly
    /// as ValidateImageTexture (`GPUUnitTest.cpp:396-420` @ v2.5.2).
    pub fn comparison_entries(&self) -> Result<usize> {
        let (width, height) =
            comparison_dimensions(self.values.original_input_value_size).map_err(Exception::new)?;
        Ok(width * height * COMPONENTS)
    }

    fn validate_reference(&self, reference: &ReferenceRun, entries: usize) -> Result<()> {
        if reference.cpu.len() != entries
            || reference.gpu.raised().is_some()
            || reference.gpu.shader.is_none()
        {
            return Err(Exception::new(
                "Missing wheel CPU pixels or shader for GPU test",
            ));
        }
        if reference.shader_settings != self.shader_settings {
            return Err(Exception::new(
                "Wheel shader settings differ from GPU test settings",
            ));
        }
        Ok(())
    }

    /// Upstream's per-test run and every retest (`GPUUnitTest.cpp:661-753` @ v2.5.2),
    /// using actual wheel CPU outputs and an additional exact wheel-shader pixel check.
    /// Returns false only for an upstream-disabled test; Rust callers must account for it.
    pub fn run(&mut self, references: &[ReferenceRun]) -> Result<bool> {
        if !self.enabled {
            return Ok(false);
        }
        let processor = self
            .processor
            .clone()
            .ok_or_else(|| Exception::new("Invalid test"))?;
        if references.len() != self.num_retests() + 1 {
            return Err(Exception::new(
                "Missing wheel results for one or more GPU test runs",
            ));
        }
        self.prepare_values()?;
        let entries = self.comparison_entries()?;
        let class = nonfinite_class::approval(&format!("{}/{}", self.group, self.name))?;
        let special_image = self.values.input_values.iter().any(|v| !v.is_finite());
        for reference in references {
            self.validate_reference(reference, entries)?;
        }
        let mut app = HeadlessApp::new(10, 10)?;
        let zeros = vec![0.0; ENTRIES];
        app.init_image(WIDTH as i32, HEIGHT as i32, Components::Rgba, &zeros)?;
        app.create_gl_buffers()?;
        app.reshape(WIDTH as i32, HEIGHT as i32)?;
        app.update_image(&self.values.input_values)?;
        app.set_print_shader(self.verbose);
        // UpdateOCIOGLState, GPUUnitTest.cpp:332-353 @ v2.5.2. This extraction
        // occurs exactly once; neither dynamic retests nor wheel rendering replace it.
        let gpu = if self.legacy_shader {
            processor.optimized_legacy_gpu_processor(
                OptimizationFlags::DEFAULT,
                self.legacy_shader_lut_edge,
            )?
        } else {
            processor.default_gpu_processor()?
        };
        gpu.extract_gpu_shader_info(self.shader_desc()?)?;
        let desc = Arc::new(self.shader_desc.take().expect("shader just extracted"));
        app.set_shader(desc.clone())?;
        let result = (|| {
            for (index, reference) in references.iter().enumerate() {
                if index != 0 {
                    self.retest_setup(index - 1, &desc, &processor);
                }
                assert_eq!(
                    gpu.get_cache_id(),
                    reference.gpu.result["gpu_cache_id"]
                        .as_str()
                        .expect("GPU cache ID")
                        .as_bytes()
                );
                assert_eq!(
                    reference.gpu.result["gpu_processor"]["isNoOp"].as_bool(),
                    Some(gpu.is_no_op())
                );
                assert_eq!(
                    reference.gpu.result["gpu_processor"]["hasChannelCrosstalk"].as_bool(),
                    Some(gpu.has_channel_crosstalk())
                );
                let shader: &Shader = reference.gpu.shader();
                // A retest changes values while retaining exactly the same code.
                assert_eq!(shader.text, references[0].gpu.shader().text);
                app.redisplay()?;
                wheel_shader::assert_matches(&desc, shader, &app.uniform_uploads());
                let pixels = app.read_image()?;
                let wheel_desc = wheel_shader::from_wheel(shader, &reference.shader_settings)?;
                let wheel_pixels = app.render_reference_shader(Arc::new(wheel_desc))?;
                ocio_testkit::assert_f32_bits_eq(
                    "port and wheel shader on one GL context",
                    &wheel_pixels,
                    &pixels,
                );
                let comparison: ImageComparison =
                    compare_image(&reference.cpu, &pixels, entries, self.comparison);
                let class_checked = if let Some(class) = class.as_ref().filter(|_| special_image) {
                    if !app.upstream_class_sampling()? {
                        return Err(Exception::new(
                            "GPU non-finite class: unverified image sampling state",
                        ));
                    }
                    let proof = crate::native_reference::controls()?;
                    if !nonfinite_class::finite_shader_parameters(shader) {
                        return Err(Exception::new(
                            "GPU non-finite class excludes non-finite shader parameters",
                        ));
                    }
                    let finite_cpu = reference.finite_cpu.as_ref().ok_or_else(|| {
                        Exception::new("GPU non-finite class: missing complete finite companion")
                    })?;
                    let finite_inputs: Vec<_> = self
                        .values
                        .input_values
                        .iter()
                        .map(|&v| if v.is_finite() { v } else { 0.0 })
                        .collect();
                    app.update_image(&finite_inputs)?;
                    let finite_result = (|| {
                        app.redisplay()?;
                        wheel_shader::assert_matches(&desc, shader, &app.uniform_uploads());
                        let finite_port = app.read_image()?;
                        let reference_desc =
                            wheel_shader::from_wheel(shader, &reference.shader_settings)?;
                        let finite_wheel = app.render_reference_shader(Arc::new(reference_desc))?;
                        Ok::<_, ocio_ops::Exception>((finite_port, finite_wheel))
                    })();
                    // Each retest sees the original complete input and original
                    // port program, even when the finite companion is rejected.
                    app.update_image(&self.values.input_values)?;
                    let (finite_port, finite_wheel) = finite_result?;
                    let info = app.info()?;
                    let mut assessed = nonfinite_class::assess(
                        class,
                        Environment {
                            info: &info,
                            native: Some(&proof.info),
                            language: self
                                .shader_settings
                                .language
                                .unwrap_or(ocio_testkit::gpu::GpuLanguage::Glsl12),
                        },
                        Frame {
                            inputs: &self.values.input_values,
                            cpu: &reference.cpu,
                            port: &pixels,
                            wheel: &wheel_pixels,
                            entries,
                            settings: self.comparison,
                            companion: Some(Companion {
                                cpu: finite_cpu,
                                port: &finite_port,
                                wheel: &finite_wheel,
                            }),
                        },
                    );
                    // A candidate class must not replace the active signed
                    // Matrix tuple rules before both CI profiles pass.
                    let mut retained_legacy_id = None;
                    if assessed.is_ok()
                        && !comparison.passes(self.comparison)
                        && class.retained_legacy
                    {
                        match crate::gpu_renderer_waiver::matching(
                            self,
                            index,
                            &info,
                            [&reference.cpu, &pixels, &wheel_pixels],
                            entries,
                            &comparison,
                        ) {
                            Ok(Some(id)) => retained_legacy_id = Some(id),
                            Ok(None) => {
                                assessed = Err(Exception::new(
                                    "Retained legacy GPU rule did not match; it cannot be replaced before CI passes",
                                ))
                            }
                            Err(error) => assessed = Err(error),
                        }
                    }
                    let stamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_err(|e| Exception::new(e.to_string()))?
                        .as_nanos();
                    let record = serde_json::json!({
                        "test":format!("{}/{}",self.group,self.name),"run":index,"class":class.id,
                        "retained_legacy_required":class.retained_legacy,
                        "retained_legacy_id":retained_legacy_id,
                        "original_max_diff_bits":format!("{:08x}",comparison.max_diff.to_bits()),
                        "original_index_diff":comparison.index_diff,"original_index_inf":comparison.index_inf,"original_index_nan":comparison.index_nan,
                        "excepted_components":assessed.as_ref().ok(),"error":assessed.as_ref().err().map(ToString::to_string),
                        "input_sha256":ocio_testkit::upstream::gpu::input_digest(&self.values.input_values),
                        "finite_input_sha256":ocio_testkit::upstream::gpu::input_digest(&finite_inputs),
                        "finite_cpu_sha256":ocio_testkit::upstream::gpu::input_digest(finite_cpu),
                        "finite_port_sha256":ocio_testkit::upstream::gpu::input_digest(&finite_port),
                        "finite_wheel_sha256":ocio_testkit::upstream::gpu::input_digest(&finite_wheel),
                        "original_port_sha256":ocio_testkit::upstream::gpu::input_digest(&pixels),
                        "original_wheel_sha256":ocio_testkit::upstream::gpu::input_digest(&wheel_pixels),
                        "exceptions":assessed.as_ref().ok().map(|indices| indices.iter().map(|&i| serde_json::json!({
                            "component":i,"input_bits":format!("{:08x}",self.values.input_values[i].to_bits()),
                            "cpu_bits":format!("{:08x}",reference.cpu[i].to_bits()),"port_bits":format!("{:08x}",pixels[i].to_bits()),
                            "wheel_bits":format!("{:08x}",wheel_pixels[i].to_bits()),
                            "category":if reference.cpu[i].is_nan()!=pixels[i].is_nan() {"nan"} else if ocio_testkit::upstream::gpu::validate_inf(reference.cpu[i],pixels[i])==ocio_testkit::upstream::gpu::LimitsDiff::Inconsistent {"inf"} else {"numeric"}
                        })).collect::<Vec<_>>()),
                    });
                    std::fs::write(
                        proof.directory.join(format!("class-{stamp}-{index}.json")),
                        serde_json::to_vec_pretty(&record)
                            .map_err(|e| Exception::new(e.to_string()))?,
                    )
                    .map_err(|e| Exception::new(e.to_string()))?;
                    let excepted = assessed?;
                    eprintln!(
                        "GPU class {} verified {}/{} run {}: original {:?}; excepted {:?}; complete finite companion passed; full GPU bits exact",
                        class.id, self.group, self.name, index, comparison, excepted
                    );
                    true
                } else {
                    false
                };
                if !comparison.passes(self.comparison) && !class_checked {
                    let owner_waiver = crate::gpu_renderer_waiver::matching(
                        self,
                        index,
                        &app.info()?,
                        [&reference.cpu, &pixels, &wheel_pixels],
                        entries,
                        &comparison,
                    )?;
                    if let Some(id) = owner_waiver {
                        eprintln!(
                            "Owner renderer waiver {id}: {} / {} run {}; original upstream comparison {:?}; full port/wheel GPU bits remain equal",
                            self.group, self.name, index, comparison,
                        );
                    } else {
                        return Err(Exception::new(format!(
                            "{} / {} run {}: GPU max vals = {:?}; GPU min vals = {:?}; Maximum error: {}; component {:?}; Large number error at {:?}; NAN error at {:?}; {} tolerance={}",
                            self.group,
                            self.name,
                            index,
                            comparison.max_values,
                            comparison.min_values,
                            comparison.max_diff,
                            comparison.index_diff,
                            comparison.index_inf,
                            comparison.index_nan,
                            if self.comparison.relative {
                                "relative"
                            } else {
                                "absolute"
                            },
                            self.comparison.error_threshold,
                        )));
                    }
                }
                if comparison.max_diff > self.max_diff {
                    self.max_diff = comparison.max_diff;
                    self.max_diff_index = comparison.index_diff.unwrap_or(usize::MAX);
                }
            }
            Ok(true)
        })();
        drop(app);
        self.shader_desc = Some(Arc::try_unwrap(desc).expect("GL app released the description"));
        result
    }
}
