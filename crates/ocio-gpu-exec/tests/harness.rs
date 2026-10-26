// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Infrastructure checks for the port of upstream's oglapp.cpp/glsl.cpp.
//! Expected values are the input image, uniform getter inputs and uploaded
//! texels, written out by platform GLSL. No OCIO CPU renderer is a reference.
//! P7-3: execution is on Rocky llvmpipe only; Windows compiles this empty target.
#![cfg(target_os = "linux")]
#![forbid(unsafe_code)]

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use ocio_gpu::GpuShaderDesc;
use ocio_gpu::gpu_shader::{TextureDimensions, TextureType};
use ocio_gpu_exec::{Components, HeadlessApp};
use ocio_ops::ops::lut3d::lut3d_op_data::Interpolation;

fn description(declarations: &str, body: &str) -> GpuShaderDesc {
    let mut desc = GpuShaderDesc::default();
    desc.create_shader_text(
        declarations,
        b"",
        b"",
        b"vec4 OCIOMain(vec4 inPixel)\n{\n",
        body,
        b"\n}\n",
    );
    desc
}

fn image(width: usize, height: usize, components: usize) -> Vec<f32> {
    // Input construction only: distinct dyadic values expose channel/row swaps,
    // clamping and changed dimensions without a transcendentals reference.
    (0..width * height * components)
        .map(|i| (i as f32 - 32.0) / 16.0)
        .collect()
}

fn assert_bits_equal(actual: &[f32], expected: &[f32]) {
    assert!(!expected.is_empty());
    ocio_testkit::assert_f32_bits_eq("headless GL readback", expected, actual);
}

fn app_with_shader(desc: GpuShaderDesc, input: &[f32], width: i32, height: i32) -> HeadlessApp {
    let mut app = HeadlessApp::new(width, height).unwrap();
    app.init_image(width, height, Components::Rgba, input)
        .unwrap();
    app.create_gl_buffers().unwrap();
    app.reshape(width, height).unwrap();
    app.set_shader(Arc::new(desc)).unwrap();
    app
}

#[test]
fn platform_is_llvmpipe_with_a_compatibility_context() {
    let app = HeadlessApp::new(16, 16).unwrap();
    let info = app.info().unwrap();
    eprintln!("{info:#?}");
    assert!(info.renderer.contains("llvmpipe"), "{info:?}");
    assert!(info.version.contains("Compatibility Profile"), "{info:?}");
    let version: Vec<u32> = info
        .shading_language_version
        .split_whitespace()
        .next()
        .unwrap()
        .split('.')
        .take(2)
        .map(|n| n.parse().unwrap())
        .collect();
    assert!(version.as_slice() >= [1, 20].as_slice(), "{info:?}");
    assert!(info.max_texture_size >= 2);
    assert_eq!(
        app.texture_max_width().unwrap(),
        info.max_texture_size as u32
    );
}

#[test]
fn identity_round_trips_rgb_and_rgba_and_image_updates() {
    for components in [Components::Rgb, Components::Rgba] {
        let count = if components == Components::Rgb { 3 } else { 4 };
        let input = image(8, 4, count);
        let mut app = HeadlessApp::new(8, 4).unwrap();
        app.init_image(8, 4, components, &input).unwrap();
        app.create_gl_buffers().unwrap();
        app.reshape(8, 4).unwrap();
        app.set_shader(Arc::new(description("", "return inPixel;")))
            .unwrap();
        app.redisplay().unwrap();
        assert_bits_equal(&app.read_image().unwrap(), &input);
        let changed: Vec<f32> = input.iter().rev().copied().collect();
        app.update_image(&changed).unwrap();
        app.redisplay().unwrap();
        assert_bits_equal(&app.read_image().unwrap(), &changed);
        app.set_y_mirror();
        app.redisplay().unwrap();
        let mirrored: Vec<f32> = changed
            .chunks_exact(8 * count)
            .rev()
            .flatten()
            .copied()
            .collect();
        assert_bits_equal(&app.read_image().unwrap(), &mirrored);
    }
}

#[test]
fn each_uniform_type_and_dynamic_updates_reaches_the_shader() {
    let input = image(8, 4, 4);
    let mut cases = Vec::new();
    let mut scalar = description("uniform float value;", "return vec4(value);");
    let scalar_value = 0.375f32;
    scalar
        .add_uniform_double("value", Arc::new(move || scalar_value as f64))
        .unwrap();
    cases.push((scalar, [scalar_value; 4]));
    for value in [false, true] {
        let mut boolean = description("uniform float value;", "return vec4(value);");
        boolean
            .add_uniform_bool("value", Arc::new(move || value))
            .unwrap();
        cases.push((boolean, [if value { 1.0 } else { 0.0 }; 4]));
    }
    let float_values = [-2.0, 0.125, 1.5, 4.0];
    let mut floats = description(
        "uniform float values[8];",
        "return vec4(values[0], values[1], values[2], values[3]);",
    );
    floats
        .add_uniform_vector_float(
            "values",
            Arc::new(|| 4),
            Arc::new(move || float_values.to_vec()),
            8,
        )
        .unwrap();
    cases.push((floats, float_values));
    let int_values = [-2, 1, 3, 7];
    let mut ints = description(
        "uniform int values[8];",
        "return vec4(float(values[0]), float(values[1]), float(values[2]), float(values[3]));",
    );
    ints.add_uniform_vector_int(
        "values",
        Arc::new(|| 4),
        Arc::new(move || int_values.to_vec()),
        8,
    )
    .unwrap();
    cases.push((ints, int_values.map(|n| n as f32)));
    assert_eq!(cases.len(), 5);
    for (desc, expected) in cases {
        let app = app_with_shader(desc, &input, 8, 4);
        app.redisplay().unwrap();
        assert_bits_equal(&app.read_image().unwrap(), &expected.repeat(32));
    }
    let values = Arc::new(Mutex::new([0.125f32, -2.0, 1.5]));
    let getter = values.clone();
    let mut desc = description("uniform vec3 values;", "return vec4(values, 1.0);");
    desc.add_uniform_float3(
        "values",
        Arc::new(move || *getter.lock().unwrap_or_else(PoisonError::into_inner)),
    )
    .unwrap();
    let app = app_with_shader(desc, &input, 8, 4);
    for expected in [[0.125f32, -2.0, 1.5], [4.0, 0.5, -0.25]] {
        *values.lock().unwrap() = expected;
        app.redisplay().unwrap();
        assert_bits_equal(
            &app.read_image().unwrap(),
            &[expected[0], expected[1], expected[2], 1.0].repeat(32),
        );
    }
}

#[test]
fn float3_getter_calls_match_the_rocky_cpp_argument_order() {
    let calls = Arc::new(AtomicU32::new(0));
    let observed = calls.clone();
    let mut desc = description("uniform vec3 values;", "return vec4(values, 1.0);");
    desc.add_uniform_float3(
        "values",
        Arc::new(move || {
            let count = observed.fetch_add(1, Ordering::SeqCst) + 1;
            [count as f32; 3]
        }),
    )
    .unwrap();
    let input = image(8, 4, 4);
    let app = app_with_shader(desc, &input, 8, 4);
    // setShader uploads uniforms too. Isolate the draw's calls, exactly as the
    // independent source-expression probe isolated one glUniform3f invocation.
    calls.store(0, Ordering::SeqCst);
    app.redisplay().unwrap();
    // Platform reference: target/p7b-callback-order.cpp/.log, Rocky GCC at O0/O2,
    // exact std::function<const Float3&()> expression through a function pointer.
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert_bits_equal(&app.read_image().unwrap(), &[3.0, 2.0, 1.0, 1.0].repeat(32));
}

#[test]
fn vector_values_getter_precedes_the_size_getter_on_rocky() {
    let input = image(8, 4, 4);
    for integer in [false, true] {
        let ready = Arc::new(AtomicBool::new(false));
        let order = Arc::new(Mutex::new(Vec::new()));
        let size_ready = ready.clone();
        let size_order = order.clone();
        let size = Arc::new(move || {
            size_order.lock().unwrap().push(b'S');
            if size_ready.swap(false, Ordering::SeqCst) {
                2
            } else {
                0
            }
        });
        let values_ready = ready.clone();
        let values_order = order.clone();
        let mut desc = if integer {
            description(
                "uniform int values[2];",
                "return vec4(float(values[0]), float(values[1]), 0.0, 1.0);",
            )
        } else {
            description(
                "uniform float values[2];",
                "return vec4(values[0], values[1], 0.0, 1.0);",
            )
        };
        if integer {
            desc.add_uniform_vector_int(
                "values",
                size,
                Arc::new(move || {
                    values_order.lock().unwrap().push(b'V');
                    values_ready.store(true, Ordering::SeqCst);
                    vec![1, 2]
                }),
                2,
            )
            .unwrap();
        } else {
            desc.add_uniform_vector_float(
                "values",
                size,
                Arc::new(move || {
                    values_order.lock().unwrap().push(b'V');
                    values_ready.store(true, Ordering::SeqCst);
                    vec![0.25, 0.5]
                }),
                2,
            )
            .unwrap();
        }
        let app = app_with_shader(desc, &input, 8, 4);
        ready.store(false, Ordering::SeqCst);
        order.lock().unwrap().clear();
        app.redisplay().unwrap();
        // Exact function-pointer/std::function expressions, float and int, at
        // GCC O0/O2: target/p7b-vector-callback-order.cpp and p7b-vector-order.log.
        assert_eq!(order.lock().unwrap().as_slice(), b"VS");
        assert!(!ready.load(Ordering::SeqCst));
        let expected = if integer {
            [1.0, 2.0, 0.0, 1.0]
        } else {
            [0.25, 0.5, 0.0, 1.0]
        };
        assert_bits_equal(&app.read_image().unwrap(), &expected.repeat(32));
    }
}

#[test]
fn negative_vector_counts_leave_the_defined_gl_error_pending() {
    let input = image(8, 4, 4);
    for integer in [false, true] {
        let mut app = HeadlessApp::new(8, 4).unwrap();
        app.init_image(8, 4, Components::Rgba, &input).unwrap();
        app.create_gl_buffers().unwrap();
        app.reshape(8, 4).unwrap();
        let mut desc = if integer {
            description("uniform int values[4];", "return vec4(float(values[0]));")
        } else {
            description("uniform float values[4];", "return vec4(values[0]);")
        };
        if integer {
            desc.add_uniform_vector_int("values", Arc::new(|| -1), Arc::new(Vec::new), 4)
                .unwrap();
        } else {
            desc.add_uniform_vector_float("values", Arc::new(|| -1), Arc::new(Vec::new), 4)
                .unwrap();
        }
        // GL 4.6 compatibility, section 7.6: a negative count records
        // INVALID_VALUE and reads no input values. Uniform::use and setShader
        // do not immediately check that error (glsl.cpp:236-270; oglapp:196-228).
        app.set_shader(Arc::new(desc)).unwrap();
        let error = app.texture_max_width().unwrap_err();
        assert_eq!(error.what(), b"invalid value");
        // That CheckStatus consumed the error; a later valid shader still works.
        app.set_shader(Arc::new(description("", "return inPixel;")))
            .unwrap();
        app.redisplay().unwrap();
        assert_bits_equal(&app.read_image().unwrap(), &input);
    }
}

#[test]
fn one_and_two_dimensional_red_and_rgb_textures_round_trip() {
    for dimensions in [TextureDimensions::D1, TextureDimensions::D2] {
        for channel in [TextureType::RedChannel, TextureType::RgbChannel] {
            for interpolation in [Interpolation::Nearest, Interpolation::Linear] {
                let channels = if channel == TextureType::RedChannel {
                    1
                } else {
                    3
                };
                let height = if dimensions == TextureDimensions::D1 {
                    1
                } else {
                    2
                };
                let texels: Vec<f32> = (0..4 * height * channels)
                    .map(|i| (i as f32 - 8.0) / 8.0)
                    .collect();
                let mut input = Vec::new();
                let mut expected = Vec::new();
                for y in 0..4 {
                    for x in 0..8 {
                        let (tx, ty) = (x % 4, y % height);
                        input.extend_from_slice(&[
                            (tx as f32 + 0.5) / 4.0,
                            (ty as f32 + 0.5) / height as f32,
                            0.0,
                            1.0,
                        ]);
                        let start = (ty * 4 + tx) * channels;
                        let rgb = if channels == 1 {
                            [texels[start], 0.0, 0.0]
                        } else {
                            [texels[start], texels[start + 1], texels[start + 2]]
                        };
                        expected.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 1.0]);
                    }
                }
                let (sampler, body) = if dimensions == TextureDimensions::D1 {
                    (
                        "uniform sampler1D lut;",
                        "return texture1D(lut, inPixel.r);",
                    )
                } else {
                    (
                        "uniform sampler2D lut;",
                        "return texture2D(lut, inPixel.rg);",
                    )
                };
                let mut desc = description(sampler, body);
                desc.add_texture(
                    "texture",
                    "lut",
                    4,
                    height as u32,
                    channel,
                    dimensions,
                    interpolation,
                    &texels,
                )
                .unwrap();
                let app = app_with_shader(desc, &input, 8, 4);
                app.redisplay().unwrap();
                assert_bits_equal(&app.read_image().unwrap(), &expected);
            }
        }
    }
}

#[test]
fn three_dimensional_texture_axes_and_binding_order_round_trip() {
    // Both 1D/2D textures are inserted before the 3D one in the description;
    // OpenGLBuilder allocates the 3D texture first, as upstream does.
    let texels: Vec<f32> = (0..24).map(|i| (i as f32 - 8.0) / 8.0).collect();
    let mut input = Vec::new();
    let mut expected = Vec::new();
    for i in 0..32 {
        let index = i % 8;
        let (x, y, z) = (index % 2, (index / 2) % 2, index / 4);
        input.extend_from_slice(&[
            (x as f32 + 0.5) / 2.0,
            (y as f32 + 0.5) / 2.0,
            (z as f32 + 0.5) / 2.0,
            1.0,
        ]);
        expected.extend_from_slice(&texels[index * 3..index * 3 + 3]);
        expected.push(1.0);
    }
    for interpolation in [Interpolation::Nearest, Interpolation::Linear] {
        let mut desc = description(
            "uniform sampler1D one; uniform sampler2D two; uniform sampler3D three;",
            "return vec4(texture3D(three, inPixel.rgb).rgb, texture1D(one, 0.25).r + texture2D(two, vec2(0.25)).r);",
        );
        desc.add_texture(
            "texture1",
            "one",
            2,
            1,
            TextureType::RedChannel,
            TextureDimensions::D1,
            Interpolation::Nearest,
            &[0.25, 0.5],
        )
        .unwrap();
        desc.add_texture(
            "texture2",
            "two",
            2,
            2,
            TextureType::RedChannel,
            TextureDimensions::D2,
            Interpolation::Nearest,
            &[0.75, 1.0, -0.25, -0.5],
        )
        .unwrap();
        desc.add_3d_texture("texture3", "three", 2, interpolation, &texels)
            .unwrap();
        let app = app_with_shader(desc, &input, 8, 4);
        app.redisplay().unwrap();
        assert_bits_equal(&app.read_image().unwrap(), &expected);
    }
}

#[test]
fn interpolation_is_nearest_only_for_nearest_and_linear_otherwise() {
    let input = [0.5f32, 0.0, 0.0, 1.0].repeat(32);
    for (interpolation, expected) in [
        (Interpolation::Nearest, 0.625f32),
        (Interpolation::Linear, 0.375),
        (Interpolation::Default, 0.375),
    ] {
        let mut desc = description(
            "uniform sampler1D lut;",
            "return vec4(texture1D(lut, inPixel.r).r);",
        );
        desc.add_texture(
            "texture",
            "lut",
            2,
            1,
            TextureType::RedChannel,
            TextureDimensions::D1,
            interpolation,
            &[0.125, 0.625],
        )
        .unwrap();
        let app = app_with_shader(desc, &input, 8, 4);
        app.redisplay().unwrap();
        assert_bits_equal(&app.read_image().unwrap(), &[expected; 128]);
    }
}

#[test]
fn two_apps_on_one_thread_restore_their_own_context() {
    let input = image(8, 4, 4);
    let first = app_with_shader(description("", "return inPixel;"), &input, 8, 4);
    {
        let second_input: Vec<f32> = input.iter().rev().copied().collect();
        let second = app_with_shader(description("", "return inPixel;"), &second_input, 8, 4);
        first.redisplay().unwrap();
        assert_bits_equal(&first.read_image().unwrap(), &input);
        second.redisplay().unwrap();
        assert_bits_equal(&second.read_image().unwrap(), &second_input);
    }
    first.redisplay().unwrap();
    assert_bits_equal(&first.read_image().unwrap(), &input);
}

#[test]
fn shader_compilation_errors_are_reported() {
    let mut app = HeadlessApp::new(8, 4).unwrap();
    let input = image(8, 4, 4);
    app.init_image(8, 4, Components::Rgba, &input).unwrap();
    app.create_gl_buffers().unwrap();
    app.reshape(8, 4).unwrap();
    let desc = description("", "invalid GLSL token sequence;");
    let error = app.set_shader(Arc::new(desc)).unwrap_err();
    assert!(
        error
            .what()
            .starts_with(b"OCIO Shader program compilation failed: "),
        "{error}"
    );
    assert!(
        error.what().ends_with(b"}\n\n"),
        "the driver error includes the full submitted shader: {error}"
    );
    // A failed compile never attached its shader. Cleanup must not issue a
    // detach of that shader, which would poison the next CheckStatus on this
    // SAME context with GL_INVALID_OPERATION. Expected pixels are the platform
    // identity, independent of any OCIO CPU implementation.
    app.set_shader(Arc::new(description("", "return inPixel;")))
        .unwrap();
    app.redisplay().unwrap();
    assert_bits_equal(&app.read_image().unwrap(), &input);
}

#[test]
fn short_caller_buffers_are_refused_before_gl_reads_them() {
    let mut app = HeadlessApp::new(8, 4).unwrap();
    assert!(app.init_image(8, 4, Components::Rgba, &[0.0; 127]).is_err());
    let mut desc = description(
        "uniform float values[4];",
        "return vec4(values[0], values[1], values[2], values[3]);",
    );
    desc.add_uniform_vector_float("values", Arc::new(|| 4), Arc::new(|| vec![0.25, 0.5]), 4)
        .unwrap();
    let error = app.set_shader(Arc::new(desc)).unwrap_err();
    assert_eq!(
        error.what(),
        b"Uniform vector size exceeds its value buffer."
    );
}
