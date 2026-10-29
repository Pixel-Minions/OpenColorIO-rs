// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Execute the actual wheel's shader strings, uniform snapshots and texture bits.
//! This decodes `ocio_testkit::gpu::Shader` (`oracle/ocio_oracle/gpu.py`), not ops.

use std::sync::Arc;

use ocio_gpu::gpu_shader::{TextureDimensions, TextureType};
use ocio_gpu::{GpuLanguage, GpuShaderDesc};
use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;
use ocio_ops::{Exception, Result};
use ocio_testkit::gpu::{Shader, ShaderSettings, UniformValue};

use crate::UniformUpload;

fn interpolation(value: &str) -> Interpolation {
    match value {
        "INTERP_UNKNOWN" => Interpolation::Unknown,
        "INTERP_NEAREST" => Interpolation::Nearest,
        "INTERP_LINEAR" => Interpolation::Linear,
        "INTERP_TETRAHEDRAL" => Interpolation::Tetrahedral,
        "INTERP_CUBIC" => Interpolation::Cubic,
        "INTERP_DEFAULT" => Interpolation::Default,
        "INTERP_BEST" => Interpolation::Best,
        _ => panic!("unknown wheel interpolation: {value}"),
    }
}

fn channel(value: &str) -> TextureType {
    match value {
        "TEXTURE_RED_CHANNEL" => TextureType::RedChannel,
        "TEXTURE_RGB_CHANNEL" => TextureType::RgbChannel,
        _ => panic!("unknown wheel channel: {value}"),
    }
}

fn dimensions(value: &str) -> TextureDimensions {
    match value {
        "TEXTURE_1D" => TextureDimensions::D1,
        "TEXTURE_2D" => TextureDimensions::D2,
        _ => panic!("unknown wheel texture dimensions: {value}"),
    }
}

/// Apply the original setter arguments exactly once. Canonical getter names
/// must not be passed through underscore normalization a second time.
pub(crate) fn description(settings: &ShaderSettings) -> Result<GpuShaderDesc> {
    assert!(
        settings
            .language
            .is_none_or(|v| v == ocio_testkit::gpu::GpuLanguage::Glsl12)
    );
    let mut desc = GpuShaderDesc::new(GpuLanguage::Glsl1_2);
    if let Some(value) = &settings.function_name {
        desc.set_function_name(value);
    }
    if let Some(value) = &settings.pixel_name {
        desc.set_pixel_name(value);
    }
    if let Some(value) = &settings.resource_prefix {
        desc.set_resource_prefix(value);
    }
    if let Some(value) = &settings.uid {
        desc.set_unique_id(value);
    }
    if let Some((index, start)) = settings.descriptor_set {
        desc.set_descriptor_set_index(index, start)?;
    }
    if let Some(value) = settings.texture_max_width {
        desc.set_texture_max_width(value);
    }
    if let Some(value) = settings.allow_texture_1d {
        desc.set_allow_texture_1d(value);
    }
    Ok(desc)
}

/// Uniform getters are snapshots of the wheel's actual getter values. Retests
/// supply a new snapshot while the port's original program and live getters survive.
pub(crate) fn from_wheel(shader: &Shader, settings: &ShaderSettings) -> Result<GpuShaderDesc> {
    let mut desc = description(settings)?;
    for (index, uniform) in shader.uniforms.iter().enumerate() {
        let name = uniform.name.as_bytes();
        let end = shader.uniforms.get(index + 1).map_or_else(
            || {
                shader.getters["uniform_buffer_size"]
                    .as_u64()
                    .expect("buffer size")
            },
            |next| next.buffer_offset,
        );
        let max_size = || {
            let bytes = end
                .checked_sub(uniform.buffer_offset)
                .expect("ordered uniform offsets");
            assert!(bytes.is_multiple_of(16));
            u32::try_from(bytes / 16).expect("uniform maximum size")
        };
        let added = match &uniform.value {
            UniformValue::Double(value) => {
                let value = *value;
                desc.add_uniform_double(name, Arc::new(move || value))?
            }
            UniformValue::Bool(value) => {
                let value = *value;
                desc.add_uniform_bool(name, Arc::new(move || value))?
            }
            UniformValue::Float3(value) => {
                let value = *value;
                desc.add_uniform_float3(name, Arc::new(move || value))?
            }
            UniformValue::VectorFloat(values) => {
                let values = values.clone();
                let size = i32::try_from(values.len()).expect("uniform length");
                assert!(u32::try_from(size).unwrap() <= max_size());
                desc.add_uniform_vector_float(
                    name,
                    Arc::new(move || size),
                    Arc::new(move || values.clone()),
                    max_size(),
                )?
            }
            UniformValue::VectorInt(values) => {
                let values = values.clone();
                let size = i32::try_from(values.len()).expect("uniform length");
                assert!(u32::try_from(size).unwrap() <= max_size());
                desc.add_uniform_vector_int(
                    name,
                    Arc::new(move || size),
                    Arc::new(move || values.clone()),
                    max_size(),
                )?
            }
            UniformValue::Unknown => return Err(Exception::new("Unknown uniform in wheel shader")),
        };
        assert!(added, "wheel uniform names must be unique");
        assert_eq!(
            desc.uniform(index as u32)?.buffer_offset() as u64,
            uniform.buffer_offset
        );
    }
    // Both texture iterators share one binding order. Preserve that order rather
    // than appending all 1D/2D textures ahead of all 3D textures.
    let mut order: Vec<_> = shader
        .textures
        .iter()
        .enumerate()
        .map(|(index, value)| (value.binding_index, false, index))
        .chain(
            shader
                .textures_3d
                .iter()
                .enumerate()
                .map(|(index, value)| (value.binding_index, true, index)),
        )
        .collect();
    order.sort_by_key(|value| value.0);
    for (binding, three_d, index) in order {
        let actual = if three_d {
            let t = &shader.textures_3d[index];
            desc.add_3d_texture(
                &t.name,
                &t.sampler_name,
                t.edge_len.try_into().expect("edge"),
                interpolation(&t.interpolation),
                &t.values,
            )?
        } else {
            let t = &shader.textures[index];
            desc.add_texture(
                &t.name,
                &t.sampler_name,
                t.width.try_into().expect("width"),
                t.height.try_into().expect("height"),
                channel(&t.channel),
                dimensions(&t.dimensions),
                interpolation(&t.interpolation),
                &t.values,
            )?
        };
        assert_eq!(u64::from(actual), binding);
    }
    // The wheel already finalized the complete GLSL text. Do not extract ops or
    // regenerate any part of it. The writer's resource counter/cache ID is checked
    // on the port description separately, and is unnecessary to execute this copy.
    desc.create_shader_text(b"", b"", b"", shader.text.as_bytes(), b"", b"");
    assert_eq!(desc.shader_text(), shader.text.as_bytes());
    assert_eq!(
        desc.uniform_buffer_size() as u64,
        shader.getters["uniform_buffer_size"].as_u64().unwrap()
    );
    Ok(desc)
}

/// Shader metadata, all uniform getter bits and all texture bits must match
/// before comparing the two descriptions' pixels on the same GL context.
pub(crate) fn assert_matches(desc: &GpuShaderDesc, shader: &Shader, uploads: &[UniformUpload]) {
    assert_eq!(desc.shader_text(), shader.text.as_bytes());
    assert_eq!(desc.cache_id(), shader.cache_id.as_bytes());
    assert_eq!(shader.getters["language"], "GPU_LANGUAGE_GLSL_1_2");
    assert_eq!(desc.language(), GpuLanguage::Glsl1_2);
    for (key, bytes) in [
        ("function_name", desc.function_name()),
        ("pixel_name", desc.pixel_name()),
        ("resource_prefix", desc.resource_prefix()),
        ("uid", desc.unique_id()),
    ] {
        assert_eq!(
            shader.getters[key].as_str().map(str::as_bytes),
            Some(bytes),
            "{key}"
        );
    }
    for (key, value) in [
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
    ] {
        assert_eq!(shader.getters[key].as_u64(), Some(value), "{key}");
    }
    assert_eq!(
        shader.getters["allow_texture_1d"].as_bool(),
        Some(desc.allow_texture_1d())
    );
    assert_eq!(desc.uniforms().len(), shader.uniforms.len());
    assert_eq!(uploads.len(), shader.uniforms.len());
    for ((actual, expected), upload) in desc.uniforms().iter().zip(&shader.uniforms).zip(uploads) {
        assert_eq!(actual.name(), expected.name.as_bytes());
        assert_eq!(actual.buffer_offset() as u64, expected.buffer_offset);
        let value = match upload {
            UniformUpload::Double(value) => UniformValue::Double(*value),
            UniformUpload::Bool(value) => UniformValue::Bool(*value),
            UniformUpload::Float3(value) => UniformValue::Float3(*value),
            UniformUpload::VectorFloat { count, values } => {
                assert_eq!(
                    usize::try_from(*count).expect("nonnegative count"),
                    values.len()
                );
                UniformValue::VectorFloat(values.clone())
            }
            UniformUpload::VectorInt { count, values } => {
                assert_eq!(
                    usize::try_from(*count).expect("nonnegative count"),
                    values.len()
                );
                UniformValue::VectorInt(values.clone())
            }
        };
        assert_eq!(value, expected.value);
    }
    assert_eq!(desc.textures().len(), shader.textures.len());
    for (index, (a, e)) in desc.textures().iter().zip(&shader.textures).enumerate() {
        assert_eq!(a.texture_name(), e.name.as_bytes());
        assert_eq!(a.sampler_name(), e.sampler_name.as_bytes());
        assert_eq!(
            (u64::from(a.width()), u64::from(a.height())),
            (e.width, e.height)
        );
        assert_eq!(a.channel(), channel(&e.channel));
        assert_eq!(a.dimensions(), dimensions(&e.dimensions));
        assert_eq!(a.interpolation(), interpolation(&e.interpolation));
        assert_eq!(
            u64::from(desc.texture_shader_binding_index(index as u32).unwrap()),
            e.binding_index
        );
        ocio_testkit::assert_f32_bits_eq("wheel shader texture", &e.values, a.values());
    }
    assert_eq!(desc.textures_3d().len(), shader.textures_3d.len());
    for (index, (a, e)) in desc
        .textures_3d()
        .iter()
        .zip(&shader.textures_3d)
        .enumerate()
    {
        assert_eq!(a.texture_name(), e.name.as_bytes());
        assert_eq!(a.sampler_name(), e.sampler_name.as_bytes());
        assert_eq!(u64::from(a.edge_len()), e.edge_len);
        assert_eq!(a.interpolation(), interpolation(&e.interpolation));
        assert_eq!(
            u64::from(desc.texture_3d_shader_binding_index(index as u32).unwrap()),
            e.binding_index
        );
        ocio_testkit::assert_f32_bits_eq("wheel shader 3D texture", &e.values, a.values());
    }
    assert_eq!(
        desc.num_dynamic_properties() as usize,
        shader.dynamic_properties.len()
    );
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use ocio::{DynamicPropertyType, ExposureContrastStyle, ExposureContrastTransform, Transform};
    use ocio_testkit::gpu::GpuShaderReply;
    use ocio_testkit::oracle::BatchCall;
    use ocio_testkit::upstream::gpu::{COMPONENTS, ENTRIES, HEIGHT, WIDTH, input_digest};
    use ocio_testkit::{Oracle, assert_f32_bits_eq};
    use serde_json::json;

    use super::{assert_matches, description, from_wheel};
    use crate::gpu_unit_test::GpuUnitTest;
    use crate::{Components, HeadlessApp};

    const DEFAULT_INPUT_DIGEST: &str =
        "2df789d173dad3435098ab4632d20b35e6ed61238cd9e3b06a35fb942de957af";

    /// The error text/condition is GpuShaderDesc.cpp:178-188 @ v2.5.2.
    #[test]
    fn zero_texture_binding_start_error_is_propagated() {
        let mut test = GpuUnitTest::new("Infrastructure", "descriptor setter error");
        test.shader_settings.descriptor_set = Some((0, 0));
        let error = test.shader_desc().unwrap_err();
        assert_eq!(
            error.what(),
            b"Texture binding start index must be greater than 0."
        );
    }

    /// Full original default probes (GPUUnitTest.cpp:212-301 @ v2.5.2),
    /// including NaN and both infinities, checked against actual wheel GPU
    /// snapshots at exposures 0/1/2 on the same extracted port program.
    /// This additional authored test makes no CPU-pixel expectation.
    #[test]
    fn original_default_special_inputs_match_the_actual_wheel_gpu_shader() {
        let mut ec = ExposureContrastTransform::new();
        ec.set_style(ExposureContrastStyle::Linear);
        ec.set_exposure(0.0);
        ec.make_exposure_dynamic();
        let mut test = GpuUnitTest::new("Infrastructure", "default special GPU parity");
        test.set_processor_transform(&Transform::from(ec)).unwrap();
        assert!(test.test_nan && test.test_infinity && test.test_wide_range);
        test.prepare_values().unwrap();
        assert_eq!(
            input_digest(&test.values.input_values),
            DEFAULT_INPUT_DIGEST
        );
        assert_eq!(test.values.original_input_value_size, ENTRIES);
        assert_eq!(test.values.input_values.len(), ENTRIES);
        assert_eq!(test.comparison_entries().unwrap(), ENTRIES);
        assert_eq!(COMPONENTS, 4);
        let settings = test.shader_settings.clone();
        let exposures = [0.0_f64, 1.0, 2.0];
        let mut calls = Vec::new();
        for exposure in exposures.iter().copied() {
            calls.push(BatchCall {
                cmd: "gpu_shader",
                args: json!({"config":{"create":{"processor_cache":"off"}},
                    "transform":{"class":"ExposureContrastTransform","calls":[
                        ["setStyle",{"enum":"EXPOSURE_CONTRAST_LINEAR"}],
                        ["setExposure",exposure],["makeExposureDynamic"]]},
                    "shader":settings.json()}),
                blobs: Vec::new(),
            });
        }
        let answers = Oracle::get().batch(&calls, false);
        assert_eq!(answers.len(), exposures.len());
        let mut references = Vec::new();
        for answer in answers {
            let reference =
                GpuShaderReply::from_response(answer.expect("actual wheel GPU request"));
            assert!(reference.raised().is_none(), "{:?}", reference.raised());
            assert!(reference.shader.is_some());
            assert_eq!(reference.shader().dynamic_properties.len(), 1);
            references.push(reference);
        }
        assert_eq!(references.len(), 3);
        let gpu = test.processor().unwrap().default_gpu_processor().unwrap();
        let mut port_desc = description(&settings).unwrap();
        gpu.extract_gpu_shader_info(&mut port_desc).unwrap();
        let port_desc = Arc::new(port_desc);
        let property = port_desc
            .dynamic_property_by_type(DynamicPropertyType::Exposure)
            .unwrap()
            .as_double()
            .unwrap();
        // Same complete image/FBO/viewport as GPUUnitTest::run; preserve
        // GL_LINEAR and the entire original exceptional-value spatial layout.
        let mut app = HeadlessApp::new(10, 10).unwrap();
        app.init_image(
            WIDTH as i32,
            HEIGHT as i32,
            Components::Rgba,
            &vec![0.0; ENTRIES],
        )
        .unwrap();
        app.create_gl_buffers().unwrap();
        app.reshape(WIDTH as i32, HEIGHT as i32).unwrap();
        app.update_image(&test.values.input_values).unwrap();
        app.set_shader(port_desc.clone()).unwrap(); // Exactly once, before all frames.
        let mut rendered = 0;
        for (frame, (exposure, reference)) in exposures.iter().copied().zip(&references).enumerate()
        {
            property.set_value(exposure);
            let retained = port_desc
                .dynamic_property_by_type(DynamicPropertyType::Exposure)
                .unwrap()
                .as_double()
                .unwrap();
            assert!(Arc::ptr_eq(&property, &retained));
            assert_eq!(reference.shader().text, references[0].shader().text);
            assert_eq!(
                gpu.get_cache_id(),
                reference.result["gpu_cache_id"]
                    .as_str()
                    .unwrap()
                    .as_bytes()
            );
            assert_eq!(
                reference.result["gpu_processor"]["isNoOp"].as_bool(),
                Some(gpu.is_no_op())
            );
            assert_eq!(
                reference.result["gpu_processor"]["hasChannelCrosstalk"].as_bool(),
                Some(gpu.has_channel_crosstalk())
            );
            app.redisplay().unwrap();
            assert_matches(&port_desc, reference.shader(), &app.uniform_uploads());
            let port_pixels = app.read_image().unwrap();
            let wheel_desc = from_wheel(reference.shader(), &settings).unwrap();
            let wheel_pixels = app.render_reference_shader(Arc::new(wheel_desc)).unwrap();
            assert_eq!(port_pixels.len(), ENTRIES);
            assert_eq!(wheel_pixels.len(), ENTRIES);
            assert_f32_bits_eq(
                &format!("full default special probes frame {frame}: wheel GPU versus port GPU"),
                &wheel_pixels,
                &port_pixels,
            );
            rendered += 1;
        }
        assert_eq!(rendered, 3);
    }
}
