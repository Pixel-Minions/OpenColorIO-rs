// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/GpuShader_tests.cpp` @ v2.5.2, so far its test of the description
//! itself. The other tests extract shaders from processors, and come with the ops' GPU
//! writers.

use std::sync::Arc;

use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;
use ocio_testkit::upstream::{check_equal, check_throw_what};

use super::*;
use crate::gpu_shader_desc::GpuShaderDesc;
use crate::open_color_types::GpuLanguage;

/// Port of `OCIO_ADD_TEST(GpuShader, generic_shader)` @ v2.5.2.
#[test]
fn generic_shader() {
    let mut shader_desc = GpuShaderDesc::default();

    {
        assert_ne!(shader_desc.language(), GpuLanguage::Glsl1_3);
        shader_desc.set_language(GpuLanguage::Glsl1_3);
        check_equal(shader_desc.language(), GpuLanguage::Glsl1_3);

        assert_ne!(shader_desc.function_name(), b"1sd234_");
        shader_desc.set_function_name("1sd234_");
        check_equal(shader_desc.function_name(), b"1sd234_");

        assert_ne!(shader_desc.pixel_name(), b"pxl_1sd234_");
        shader_desc.set_pixel_name("pxl_1sd234_");
        check_equal(shader_desc.pixel_name(), b"pxl_1sd234_");

        assert_ne!(shader_desc.resource_prefix(), b"res_1sd234_");
        shader_desc.set_resource_prefix("res_1sd234_");
        check_equal(shader_desc.resource_prefix(), b"res_1sd234_");

        shader_desc.finalize().unwrap();
        let id = shader_desc.cache_id();
        check_equal(
            id.as_slice(),
            b"glsl_1.3 1sd234_ res_1sd234_ pxl_1sd234_ 0 0 1 \
              6001c324468d497f99aa06d3014798d8",
        );
        shader_desc.set_resource_prefix("res_1");
        shader_desc.finalize().unwrap();
        assert_ne!(shader_desc.cache_id(), id);
    }

    {
        let width: u32 = 3;
        let height: u32 = 2;
        let size = (width * height * 3) as usize;

        let values: [f32; 18] = [
            0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, //
            0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9,
        ];

        check_equal(shader_desc.num_textures(), 0);
        let texture_shader_binding_index = shader_desc
            .add_texture(
                "lut1",
                "lut1Sampler",
                width,
                height,
                TextureType::RgbChannel,
                TextureDimensions::D2,
                Interpolation::Tetrahedral,
                &values,
            )
            .unwrap();

        check_equal(shader_desc.num_textures(), 1);
        check_equal(texture_shader_binding_index, 1);

        let t = shader_desc.texture(0).unwrap();
        check_equal(t.texture_name(), b"lut1");
        check_equal(t.sampler_name(), b"lut1Sampler");
        check_equal(width, t.width());
        check_equal(height, t.height());
        check_equal(TextureType::RgbChannel, t.channel());
        check_equal(Interpolation::Tetrahedral, t.interpolation());
        let d = t.dimensions();

        check_throw_what(shader_desc.texture(1), "1D LUT access error");

        let vals = shader_desc.texture(0).unwrap().values();
        for idx in 0..size {
            check_equal(values[idx], vals[idx]);
        }

        check_throw_what(
            shader_desc.texture(1).map(Texture::values),
            "1D LUT access error",
        );

        // Support several 1D LUTs

        let texture_shader_binding_index = shader_desc
            .add_texture(
                "lut2",
                "lut2Sampler",
                width,
                height,
                TextureType::RgbChannel,
                d,
                Interpolation::Tetrahedral,
                &values,
            )
            .unwrap();
        check_equal(shader_desc.num_textures(), 2);
        check_equal(texture_shader_binding_index, 2);

        shader_desc.texture(0).unwrap();
        shader_desc.texture(1).unwrap();
        check_throw_what(
            shader_desc.texture(2).map(Texture::values),
            "1D LUT access error",
        );
    }

    {
        let edgelen: u32 = 2;
        let size = (edgelen * edgelen * edgelen * 3) as usize;
        let values: [f32; 24] = [
            0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 0.7, 0.8, 0.9, //
            0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 0.7, 0.8, 0.9,
        ];

        check_equal(shader_desc.num_textures_3d(), 0);
        let texture_shader_binding_index = shader_desc
            .add_3d_texture(
                "lut1",
                "lut1Sampler",
                edgelen,
                Interpolation::Tetrahedral,
                &values,
            )
            .unwrap();
        check_equal(shader_desc.num_textures_3d(), 1);
        check_equal(texture_shader_binding_index, 3);

        let t = shader_desc.texture_3d(0).unwrap();
        check_equal(t.texture_name(), b"lut1");
        check_equal(t.sampler_name(), b"lut1Sampler");
        check_equal(edgelen, t.edge_len());
        check_equal(Interpolation::Tetrahedral, t.interpolation());

        check_throw_what(shader_desc.texture_3d(1), "3D LUT access error");

        let vals = shader_desc.texture_3d(0).unwrap().values();
        for idx in 0..size {
            check_equal(values[idx], vals[idx]);
        }

        check_throw_what(
            shader_desc.texture_3d(1).map(Texture3D::values),
            "3D LUT access error",
        );

        // Supports several 3D LUTs

        let texture_shader_binding_index = shader_desc
            .add_3d_texture(
                "lut2",
                "lut2Sampler",
                edgelen,
                Interpolation::Tetrahedral,
                &values,
            )
            .unwrap();
        check_equal(shader_desc.num_textures_3d(), 2);
        check_equal(texture_shader_binding_index, 4);

        // Check the 3D LUT limit

        assert!(
            shader_desc
                .add_3d_texture(
                    "lut1",
                    "lut1Sampler",
                    130,
                    Interpolation::Tetrahedral,
                    &values
                )
                .is_err()
        );
    }

    {
        shader_desc.add_to_parameter_declare_shader_code("vec2 coords;\n");
        shader_desc.add_to_helper_shader_code("vec2 helpers() {}\n\n");
        shader_desc.add_to_function_header_shader_code("void func() {\n");
        shader_desc.add_to_function_shader_code("  int i;\n");
        shader_desc.add_to_function_footer_shader_code("}\n");

        shader_desc.finalize().unwrap();

        let mut frag_text = String::new();
        frag_text += "\n";
        frag_text += "// Declaration of all variables\n";
        frag_text += "\n";
        frag_text += "vec2 coords;\n";
        frag_text += "\n";
        frag_text += "// Declaration of all helper methods\n";
        frag_text += "\n";
        frag_text += "vec2 helpers() {}\n\n";
        frag_text += "void func() {\n";
        frag_text += "  int i;\n";
        frag_text += "}\n";

        check_equal(frag_text.as_bytes(), shader_desc.shader_text());
    }

    {
        assert_ne!(shader_desc.language(), GpuLanguage::GlslVk4_6);
        shader_desc.set_language(GpuLanguage::GlslVk4_6);
        check_equal(shader_desc.language(), GpuLanguage::GlslVk4_6);

        check_throw_what(
            shader_desc.set_descriptor_set_index(123, 0),
            "Texture binding start index must be greater than 0.",
        );
        shader_desc.set_descriptor_set_index(123, 456).unwrap();
        check_equal(shader_desc.descriptor_set_index(), 123);
        check_equal(shader_desc.texture_binding_start(), 456);

        let get_size: Getter<i32> = Arc::new(|| 2); // simulate only 2 elements in the array
        let get_array: Getter<Vec<f32>> = Arc::new(Vec::new);
        let max_size: u32 = 3;
        shader_desc
            .add_uniform_vector_float("array", get_size, get_array, max_size)
            .unwrap();
        check_equal(shader_desc.uniform_buffer_size(), 16 * max_size as usize);

        shader_desc.add_to_texture_declare_shader_code(
            "layout(set=123, binding = 456) uniform sampler2D samplerName; \n",
        );

        shader_desc.finalize().unwrap();

        let mut frag_text = String::new();
        frag_text += "layout (set = 123, binding = 0) uniform 1sd234__Parameters\n";
        frag_text += "{\n";
        frag_text += "\n";
        frag_text += "// Declaration of all variables\n";
        frag_text += "\n";
        frag_text += "vec2 coords;\n";
        frag_text += "\n";
        frag_text += "};\n";
        frag_text += "\n";
        frag_text += "// Declaration of all textures\n";
        frag_text += "\n";
        frag_text += "layout(set=123, binding = 456) uniform sampler2D samplerName; \n";
        frag_text += "\n";
        frag_text += "// Declaration of all helper methods\n";
        frag_text += "\n";
        frag_text += "vec2 helpers() {}\n\n";
        frag_text += "void func() {\n";
        frag_text += "  int i;\n";
        frag_text += "}\n";

        check_equal(frag_text.as_bytes(), shader_desc.shader_text());
    }
}

/// U-11 (docs/improvements.md): texture values shorter than the texture are the port's error,
/// after upstream's own checks; a longer slice is read up to the texture's count, as upstream
/// reads the buffer.
#[test]
fn texture_values_shorter_than_the_texture_are_an_error() {
    let mut desc = GpuShaderDesc::default();
    let rgb = |desc: &mut GpuShaderDesc, name: &str, values: &[f32]| {
        desc.add_texture(
            name,
            "s",
            2,
            2,
            TextureType::RgbChannel,
            TextureDimensions::D2,
            Interpolation::Linear,
            values,
        )
    };
    check_equal(
        rgb(&mut desc, "t", &[0.0; 11]).unwrap_err().message(),
        "The texture 't' needs 12 values, but only 11 were given.",
    );
    // Upstream's own check of the name comes first.
    let error = rgb(&mut desc, "", &[0.0; 11]).unwrap_err();
    assert!(!error.message().contains("values, but only"), "{error}");
    check_equal(
        desc.add_3d_texture("c", "s", 2, Interpolation::Linear, &[0.0; 23])
            .unwrap_err()
            .message(),
        "The texture 'c' needs 24 values, but only 23 were given.",
    );
    check_equal(desc.num_textures() + desc.num_textures_3d(), 0);

    desc.add_texture(
        "t",
        "s",
        2,
        1,
        TextureType::RedChannel,
        TextureDimensions::D1,
        Interpolation::Linear,
        &[1.0, 2.0, 3.0],
    )
    .unwrap();
    check_equal(desc.texture(0).unwrap().values(), &[1.0f32, 2.0][..]);
}

/// I-33 (docs/improvements.md): a texture's float count wraps at 2^32, as upstream's `unsigned`
/// count does, and that many values are kept.
#[test]
fn a_texture_float_count_wraps_at_2_to_the_32() {
    let mut desc = GpuShaderDesc::default();
    desc.set_texture_max_width(u32::MAX);
    // 65536 * 65536 red texels: 2^32, which wraps to 0.
    desc.add_texture(
        "w",
        "s",
        65536,
        65536,
        TextureType::RedChannel,
        TextureDimensions::D2,
        Interpolation::Linear,
        &[],
    )
    .unwrap();
    check_equal(desc.texture(0).unwrap().values().len(), 0);
    // 65536 * 43691 RGB texels: 2^33 + 65536 floats, which wraps to 65536.
    let values: Vec<f32> = (0..65536).map(|i| i as f32).collect();
    desc.add_texture(
        "v",
        "s",
        65536,
        43691,
        TextureType::RgbChannel,
        TextureDimensions::D2,
        Interpolation::Linear,
        &values,
    )
    .unwrap();
    check_equal(desc.texture(1).unwrap().values(), &values[..]);
}

/// A uniform whose name is taken, by a uniform of any type, is not added: `addUniform`
/// returns false before it moves the buffer (`if (uniformNameUsed(name)) return false;`,
/// GpuShader.cpp:380-449 @ v2.5.2). Upstream's tests add one uniform only, and the wheel's
/// processors never add a name twice, so this follows upstream's code.
#[test]
fn a_uniform_whose_name_is_taken_is_not_added() {
    let mut desc = GpuShaderDesc::default();
    let size: Getter<i32> = Arc::new(|| 1);
    let floats: Getter<Vec<f32>> = Arc::new(Vec::new);
    let ints: Getter<Vec<i32>> = Arc::new(Vec::new);
    let double: Getter<f64> = Arc::new(|| 0.5);
    let boolean: Getter<bool> = Arc::new(|| true);
    let float3: Getter<[f32; 3]> = Arc::new(|| [1.0, 2.0, 3.0]);
    let names = ["d", "b", "f", "v", "i"];
    let add = |desc: &mut GpuShaderDesc, kind: usize, name: &str| match kind {
        0 => desc.add_uniform_double(name, double.clone()),
        1 => desc.add_uniform_bool(name, boolean.clone()),
        2 => desc.add_uniform_float3(name, float3.clone()),
        3 => desc.add_uniform_vector_float(name, size.clone(), floats.clone(), 3),
        _ => desc.add_uniform_vector_int(name, size.clone(), ints.clone(), 5),
    };
    for (kind, name) in names.iter().enumerate() {
        check_equal(add(&mut desc, kind, name).unwrap(), true);
    }
    let buffer_size = desc.uniform_buffer_size();
    for kind in 0..names.len() {
        for name in names {
            check_equal(add(&mut desc, kind, name).unwrap(), false);
            check_equal(desc.num_uniforms(), 5);
            check_equal(desc.uniform_buffer_size(), buffer_size);
        }
    }
    // Only the C string counts: the name ends at a NUL.
    check_equal(add(&mut desc, 0, "d\0x").unwrap(), false);
    check_equal(desc.num_uniforms(), 5);
}

/// An empty uniform name is refused by the `Uniform` constructor
/// (GpuShader.cpp:178-185 @ v2.5.2), which `addUniform` calls after it aligns the buffer
/// (`m_uniformBufferSize = alignOffset(...)`, lines 387, 400, 413, 429 and 445): the refused
/// uniform still aligns the buffer, here to `GPU_ARRAY_ALIGNMENT`, 16 (line 59), after a
/// double's 4 bytes (`GPU_FLOAT_SIZE`, line 53).
#[test]
fn an_empty_uniform_name_is_refused_after_the_buffer_is_aligned() {
    let mut desc = GpuShaderDesc::default();
    desc.add_uniform_double("d", Arc::new(|| 0.5)).unwrap();
    check_equal(desc.uniform_buffer_size(), 4);
    check_equal(
        desc.add_uniform_vector_float("", Arc::new(|| 1), Arc::new(Vec::new), 3)
            .unwrap_err()
            .message(),
        "The dynamic property name is invalid.",
    );
    check_equal(desc.num_uniforms(), 1);
    check_equal(desc.uniform_buffer_size(), 16);
    // A name that is empty as a C string is refused too.
    check_equal(
        desc.add_uniform_double("\0d", Arc::new(|| 0.5))
            .unwrap_err()
            .message(),
        "The dynamic property name is invalid.",
    );
    check_equal(desc.uniform_buffer_size(), 16);
    desc.add_uniform_bool("b", Arc::new(|| true)).unwrap();
    check_equal(desc.uniform(1).unwrap().buffer_offset(), 16);
}
