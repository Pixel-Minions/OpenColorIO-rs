// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's `gpu_shader` command (`oracle/ocio_oracle/gpu.py`, chunk O1.3) against the
//! wheel itself: every language and setting reaches the shader description; uniforms,
//! textures and dynamic properties come back with their values, each in its place; the default
//! GPU processor is the optimized one with the default flags; every error path raises where the
//! command says; the command's refusals; and replies that don't depend on the run.
//!
//! **Error paths**, by stage (paths relative to `upstream/OpenColorIO/src/OpenColorIO` @ v2.5.2):
//! - `config`, `transform`, `processor`: as in `cpu_apply`.
//! - `gpu_processor`: `getDefaultGPUProcessor` and `getOptimizedGPUProcessor` finalize,
//!   optimize and validate the dynamic properties (GPUProcessor.cpp:73-97). Nothing a request
//!   can build raises there: a processor with two dynamic exposures raises at extraction.
//! - `shader_desc`: `setDescriptorSetIndex` with a texture binding start of 0
//!   (GpuShaderDesc.cpp:178-188).
//! - `extract`: an op the language doesn't support (OSL and the 1D and 3D LUTs), an MSL class
//!   name starting with a digit (GpuShaderClassWrapper.cpp:157-160), an ACES 2 table wider than
//!   the texture limit (GpuShader.cpp:212-219), a dynamic property of the same type twice
//!   (GpuShaderDesc.cpp:213-224).

use ocio_testkit::Oracle;
use ocio_testkit::gpu::{
    DynamicValue, GpuLanguage, GpuShaderReply, GpuShaderRequest, Raised, ShaderSettings,
    UniformValue,
};
use ocio_testkit::oracle::{BatchCall, Response};
use serde_json::{Value, json};

/// Runs `calls` in one oracle process; each must succeed.
fn batch(calls: &[BatchCall<'_>]) -> Vec<Response> {
    Oracle::get()
        .batch(calls, true)
        .into_iter()
        .enumerate()
        .map(|(i, r)| r.unwrap_or_else(|e| panic!("call {i}: {e}")))
        .collect()
}

/// Runs `requests` in one oracle process; each must succeed.
fn run(requests: &[GpuShaderRequest]) -> Vec<GpuShaderReply> {
    let calls: Vec<BatchCall<'_>> = requests.iter().map(GpuShaderRequest::call).collect();
    batch(&calls)
        .into_iter()
        .map(GpuShaderReply::from_response)
        .collect()
}

fn transform(transform: Value) -> Value {
    json!({ "transform": transform })
}

/// A matrix, a log and an exposure and contrast with every value dynamic: ops every language
/// renders, OSL included, and uniforms of dynamic properties.
fn analytic(exposure: f64, contrast: f64, gamma: f64) -> Value {
    json!({"class": "GroupTransform", "children": [
        {"class": "MatrixTransform", "args": {"offset": [0.125, -0.25, 0.0625, 0.5]}},
        {"class": "LogTransform", "args": {"base": 2.0}},
        {"class": "ExposureContrastTransform",
         "args": {"exposure": exposure, "contrast": contrast, "gamma": gamma},
         "calls": [["makeExposureDynamic"], ["makeContrastDynamic"], ["makeGammaDynamic"]]},
    ]})
}

/// A 1D LUT of `values` (RGB triples), set entry by entry.
fn lut1d(values: &[[f32; 3]]) -> Value {
    let calls: Vec<Value> = values
        .iter()
        .enumerate()
        .map(|(i, v)| json!(["setValue", i, v[0], v[1], v[2]]))
        .collect();
    json!({"class": "Lut1DTransform", "args": {"length": values.len()}, "calls": calls})
}

/// `n` entries of a curve, its three channels apart unless `grey`.
fn curve(n: usize, grey: bool) -> Vec<[f32; 3]> {
    (0..n)
        .map(|i| {
            let x = i as f32 / (n - 1) as f32;
            let x = x * x;
            if grey {
                [x; 3]
            } else {
                [x, x * 0.5, x * 0.25 + 0.125]
            }
        })
        .collect()
}

/// Every language renders the settings it was given: each setter reaches the description
/// (its getters), and the shader text and cache ID change with it. The analytic ops render in
/// all 10 languages.
#[test]
fn every_language_takes_every_setting() {
    let custom = |language: GpuLanguage| ShaderSettings {
        language: Some(language),
        function_name: Some("F1".into()),
        pixel_name: Some("P2".into()),
        resource_prefix: Some("R3".into()),
        uid: Some("U4".into()),
        descriptor_set: Some((5, 6)),
        texture_max_width: Some(7),
        allow_texture_1d: Some(false),
    };
    let processor = transform(analytic(0.5, 1.25, 1.5));
    let mut requests = Vec::new();
    for language in GpuLanguage::ALL {
        requests.push(GpuShaderRequest::new(
            processor.clone(),
            ShaderSettings::language(language),
        ));
        requests.push(GpuShaderRequest::new(processor.clone(), custom(language)));
    }
    let replies = run(&requests);
    for (pair, language) in replies.as_chunks::<2>().0.iter().zip(GpuLanguage::ALL) {
        let (plain, custom) = (pair[0].shader(), pair[1].shader());
        let getters = &custom.getters;
        assert_eq!(getters["language"], language.oracle_name());
        assert_eq!(plain.getters["language"], language.oracle_name());
        for (key, value) in [
            ("function_name", json!("F1")),
            ("pixel_name", json!("P2")),
            ("resource_prefix", json!("R3")),
            ("uid", json!("U4")),
            ("descriptor_set_index", json!(5)),
            ("texture_binding_start", json!(6)),
            ("texture_max_width", json!(7)),
            ("allow_texture_1d", json!(false)),
        ] {
            assert_eq!(getters[key], value, "{language:?}: {key}");
            assert_ne!(plain.getters[key], value, "{language:?}: {key} by default");
        }
        assert_ne!(plain.text, custom.text, "{language:?}");
        assert_ne!(plain.cache_id, custom.cache_id, "{language:?}");
        for name in ["F1", "P2"] {
            assert!(custom.text.contains(name), "{language:?}: {name}");
        }
        for uniform in &custom.uniforms {
            assert!(uniform.name.starts_with("R3"), "{language:?}: {uniform:?}");
        }
    }
    // The language is part of the cache ID.
    let ids: std::collections::HashSet<&str> = replies
        .iter()
        .step_by(2)
        .map(|r| r.shader().cache_id.as_str())
        .collect();
    assert_eq!(ids.len(), GpuLanguage::ALL.len());
}

/// The uniforms of dynamic properties hold the properties' values at extraction, and so do the
/// dynamic properties: the exposure, contrast and gamma a transform sets, of any sign and
/// magnitude. Every uniform type occurs: the grading transforms, dynamic, add booleans, float
/// triples and vectors of floats and ints.
#[test]
fn uniforms_hold_their_values_at_extraction() {
    let triples = [
        (0.5, 1.25, 1.5),
        (-3.75, 0.001, 0.125),
        (1e-30, 7.0, 2.5e10),
    ];
    let mut requests: Vec<GpuShaderRequest> = triples
        .iter()
        .map(|&(e, c, g)| {
            GpuShaderRequest::new(
                transform(analytic(e, c, g)),
                ShaderSettings::language(GpuLanguage::Glsl40),
            )
        })
        .collect();
    for (class, style) in [
        ("GradingPrimaryTransform", "GRADING_LOG"),
        ("GradingRGBCurveTransform", "GRADING_LIN"),
        ("GradingToneTransform", "GRADING_VIDEO"),
        ("GradingHueCurveTransform", "GRADING_LOG"),
    ] {
        requests.push(GpuShaderRequest::new(
            transform(json!({"class": class,
                "args": {"style": {"enum": style}, "dynamic": true}})),
            ShaderSettings::language(GpuLanguage::Glsl40),
        ));
    }
    let replies = run(&requests);
    for (reply, &(e, c, g)) in replies.iter().zip(&triples) {
        let shader = reply.shader();
        let doubles: Vec<f64> = shader
            .uniforms
            .iter()
            .map(|u| match u.value {
                UniformValue::Double(d) => d,
                ref other => panic!("{u:?}: {other:?}"),
            })
            .collect();
        // One uniform per dynamic property, each at its own place in the uniform buffer.
        assert_eq!(
            doubles.len(),
            shader.dynamic_properties.len(),
            "{:?}",
            shader.uniforms
        );
        let size = shader.getters["uniform_buffer_size"]
            .as_u64()
            .expect("a size");
        let offsets: Vec<u64> = shader.uniforms.iter().map(|u| u.buffer_offset).collect();
        assert!(
            offsets.windows(2).all(|w| w[0] < w[1]) && offsets.iter().all(|&o| o < size),
            "{offsets:?} in {size} bytes"
        );
        for value in [e, c, g] {
            assert!(
                doubles.iter().any(|d| d.to_bits() == value.to_bits()),
                "{value:e} among {doubles:?}"
            );
        }
        let properties: Vec<(String, u64)> = shader
            .dynamic_properties
            .iter()
            .map(|p| match &p.value {
                DynamicValue::Double(d) => (p.kind.clone(), d.to_bits()),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(
            properties,
            vec![
                ("DYNAMIC_PROPERTY_EXPOSURE".into(), e.to_bits()),
                ("DYNAMIC_PROPERTY_CONTRAST".into(), c.to_bits()),
                ("DYNAMIC_PROPERTY_GAMMA".into(), g.to_bits()),
            ]
        );
    }
    let mut kinds = std::collections::BTreeSet::new();
    for reply in &replies[triples.len()..] {
        let shader = reply.shader();
        assert_eq!(shader.dynamic_properties.len(), 1, "{:?}", reply.result);
        assert!(matches!(
            shader.dynamic_properties[0].value,
            DynamicValue::Repr(_)
        ));
        for uniform in &shader.uniforms {
            kinds.insert(uniform.kind.clone());
            let consistent = matches!(
                (&uniform.kind[..], &uniform.value),
                ("UNIFORM_DOUBLE", UniformValue::Double(_))
                    | ("UNIFORM_BOOL", UniformValue::Bool(_))
                    | ("UNIFORM_FLOAT3", UniformValue::Float3(_))
                    | ("UNIFORM_VECTOR_FLOAT", UniformValue::VectorFloat(_))
                    | ("UNIFORM_VECTOR_INT", UniformValue::VectorInt(_))
            );
            assert!(consistent, "{uniform:?}");
        }
    }
    let every: std::collections::BTreeSet<String> = [
        "UNIFORM_BOOL",
        "UNIFORM_DOUBLE",
        "UNIFORM_FLOAT3",
        "UNIFORM_VECTOR_FLOAT",
        "UNIFORM_VECTOR_INT",
    ]
    .map(String::from)
    .into();
    assert_eq!(kinds, every);
}

/// Each texture comes back with its own values, in the order the shader declares them: two 1D
/// LUTs short enough for one row hold their entries (one channel when the channels agree, three
/// otherwise), and a 3D LUT holds its values. A LUT longer than the width limit, a language
/// without 1D textures, or `setAllowTexture1D(false)` give 2D textures of `width * height`
/// texels.
#[test]
fn textures_hold_their_values() {
    let colour = curve(9, false);
    let grey = curve(6, true);
    // Unoptimized, so that the two LUTs stay two (the default optimization composes them).
    let luts = json!({
        "transform": {"class": "GroupTransform", "children": [lut1d(&colour), lut1d(&grey)]},
        "optimization": "OPTIMIZATION_NONE",
    });
    let lut3d: Vec<[f32; 3]> = (0..27)
        .map(|i| {
            let i = i as f32;
            [i / 26.0, (26.0 - i) / 26.0, (i * 7.0 % 27.0) / 26.0]
        })
        .collect();
    let calls: Vec<Value> = lut3d
        .iter()
        .enumerate()
        .map(|(i, v)| json!(["setValue", i / 9, i / 3 % 3, i % 3, v[0], v[1], v[2]]))
        .collect();
    let cube = json!({"class": "Lut3DTransform", "args": {"gridSize": 3}, "calls": calls});
    let long = lut1d(&curve(100, true));
    let requests = vec![
        GpuShaderRequest::new(luts.clone(), ShaderSettings::default()),
        GpuShaderRequest::new(transform(cube), ShaderSettings::default()),
        GpuShaderRequest::new(
            transform(long),
            ShaderSettings {
                texture_max_width: Some(16),
                ..ShaderSettings::default()
            },
        ),
        GpuShaderRequest::new(
            luts.clone(),
            ShaderSettings::language(GpuLanguage::GlslEs30),
        ),
        GpuShaderRequest::new(
            luts,
            ShaderSettings {
                allow_texture_1d: Some(false),
                ..ShaderSettings::default()
            },
        ),
    ];
    let replies = run(&requests);

    let one_row = replies[0].shader();
    assert_eq!(one_row.textures.len(), 2, "{:?}", replies[0].result);
    let (first, second) = (&one_row.textures[0], &one_row.textures[1]);
    assert_eq!((first.width, first.height), (9, 1));
    assert_eq!(first.channel, "TEXTURE_RGB_CHANNEL");
    assert_eq!(first.dimensions, "TEXTURE_1D");
    assert_eq!(
        first.values.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
        colour
            .concat()
            .iter()
            .map(|v| v.to_bits())
            .collect::<Vec<_>>()
    );
    assert_eq!((second.width, second.height), (6, 1));
    assert_eq!(second.channel, "TEXTURE_RED_CHANNEL");
    assert_eq!(
        second
            .values
            .iter()
            .map(|v| v.to_bits())
            .collect::<Vec<_>>(),
        grey.iter().map(|v| v[0].to_bits()).collect::<Vec<_>>()
    );
    assert_ne!(first.name, second.name);
    assert!(first.binding_index < second.binding_index);
    for texture in &one_row.textures {
        assert!(one_row.text.contains(&texture.sampler_name), "{texture:?}");
    }

    let cube = replies[1].shader();
    assert!(cube.textures.is_empty());
    assert_eq!(cube.textures_3d.len(), 1, "{:?}", replies[1].result);
    let texture = &cube.textures_3d[0];
    assert_eq!(texture.edge_len, 3);
    // The LUT's entries in one of the two orders a 3D texture can have: blue fastest, as
    // `lut3d` holds them, or red fastest.
    let bits = |entries: &[[f32; 3]]| -> Vec<u32> {
        entries.concat().iter().map(|v| v.to_bits()).collect()
    };
    let red_fastest: Vec<[f32; 3]> = (0..27)
        .map(|k| lut3d[k % 3 * 9 + k / 3 % 3 * 3 + k / 9])
        .collect();
    let values: Vec<u32> = texture.values.iter().map(|v| v.to_bits()).collect();
    assert!(
        values == bits(&lut3d) || values == bits(&red_fastest),
        "the 3D texture holds the LUT's entries in order: {:?}",
        texture.values
    );

    let long = &replies[2].shader().textures[0];
    assert_eq!(long.width, 16);
    assert!(long.height > 1, "{long:?}");
    assert_eq!(long.dimensions, "TEXTURE_2D");
    assert_eq!(long.values.len() as u64, long.width * long.height);

    for reply in &replies[3..] {
        let shader = reply.shader();
        for (texture, row) in shader.textures.iter().zip(&one_row.textures) {
            assert_eq!(texture.dimensions, "TEXTURE_2D", "{texture:?}");
            assert_eq!((texture.width, texture.height), (row.width, row.height));
            assert_eq!(texture.values, row.values);
        }
    }
}

/// `getDefaultGPUProcessor()` is `getOptimizedGPUProcessor(OPTIMIZATION_DEFAULT)`: the same
/// reply. Other flags reach the GPU processor: without optimization, an identity matrix stays in
/// the shader.
#[test]
fn optimization_flags_reach_the_gpu_processor() {
    let spec = json!({"class": "GroupTransform", "children": [
        {"class": "MatrixTransform"},
        {"class": "LogTransform", "args": {"base": 10.0}},
    ]});
    let with = |flags: Option<&str>| {
        let mut processor = transform(spec.clone());
        if let Some(flags) = flags {
            processor["optimization"] = json!(flags);
        }
        GpuShaderRequest::new(processor, ShaderSettings::default())
    };
    let replies = run(&[
        with(None),
        with(Some("OPTIMIZATION_DEFAULT")),
        with(Some("OPTIMIZATION_NONE")),
    ]);
    assert_eq!(replies[0].result, replies[1].result);
    assert_eq!(replies[0].shader, replies[1].shader);
    assert_ne!(
        replies[0].result["gpu_cache_id"],
        replies[2].result["gpu_cache_id"]
    );
    assert!(!replies[0].shader().text.contains("Matrix"));
    assert!(replies[2].shader().text.contains("Matrix"));
}

/// Every error path the header lists, with its stage and message.
#[test]
fn every_error_path_raises() {
    let exposure = json!({"class": "ExposureContrastTransform", "args": {"exposure": 0.5},
        "calls": [["makeExposureDynamic"]]});
    let cube = json!({"class": "Lut3DTransform", "args": {"gridSize": 2}});
    let aces2 = json!({"class": "BuiltinTransform", "args": {
        "style": "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-REC709_2.0"}});
    let cases: Vec<(&str, GpuShaderRequest, &str, &str)> = vec![
        (
            "a config that doesn't parse",
            GpuShaderRequest::new(
                json!({"config": {"yaml": "ocio_profile_version: 2\ncolorspaces: 3\n"},
                    "src": "a", "dst": "b"}),
                ShaderSettings::default(),
            ),
            "config",
            "",
        ),
        (
            "a transform the binding's constructor refuses",
            GpuShaderRequest::new(
                transform(json!({"class": "LogAffineTransform",
                    "args": {"linSideSlope": [0.0, 1.0, 1.0]}})),
                ShaderSettings::default(),
            ),
            "transform",
            "linear side slope cannot be 0",
        ),
        (
            "a color space the raw config doesn't have",
            GpuShaderRequest::new(
                json!({"src": "raw", "dst": "nowhere"}),
                ShaderSettings::default(),
            ),
            "processor",
            "nowhere",
        ),
        (
            "a texture binding start of 0",
            GpuShaderRequest::new(
                transform(exposure.clone()),
                ShaderSettings {
                    descriptor_set: Some((1, 0)),
                    ..ShaderSettings::default()
                },
            ),
            "shader_desc",
            "Texture binding start index must be greater than 0.",
        ),
        (
            "OSL and a 3D LUT",
            GpuShaderRequest::new(
                transform(cube.clone()),
                ShaderSettings::language(GpuLanguage::Osl1),
            ),
            "extract",
            "The Lut3DOp is not yet supported by the 'Open Shading language (OSL)' translation",
        ),
        (
            "OSL and a 1D LUT",
            GpuShaderRequest::new(
                transform(lut1d(&curve(4, false))),
                ShaderSettings::language(GpuLanguage::Osl1),
            ),
            "extract",
            "The Lut1DOp is not yet supported by the 'Open Shading language (OSL)' translation",
        ),
        (
            "an MSL class name starting with a digit",
            GpuShaderRequest::new(
                transform(exposure.clone()),
                ShaderSettings {
                    language: Some(GpuLanguage::Msl20),
                    resource_prefix: Some("9x".into()),
                    ..ShaderSettings::default()
                },
            ),
            "extract",
            "Struct name must not start with a digit. Invalid className passed in: 9xOCIOMain",
        ),
        (
            "an ACES 2 table wider than the limit",
            GpuShaderRequest::new(
                transform(aces2),
                ShaderSettings {
                    texture_max_width: Some(100),
                    ..ShaderSettings::default()
                },
            ),
            "extract",
            "1D LUT size exceeds the maximum: ",
        ),
        (
            "two dynamic exposures",
            GpuShaderRequest::new(
                transform(json!({"class": "GroupTransform",
                    "children": [exposure.clone(), exposure]})),
                ShaderSettings::default(),
            ),
            "extract",
            "Dynamic property already here: 0.",
        ),
    ];
    let requests: Vec<GpuShaderRequest> = cases.iter().map(|(_, r, _, _)| r.clone()).collect();
    for ((label, _, stage, fragment), reply) in cases.iter().zip(run(&requests)) {
        let raised = reply
            .raised()
            .unwrap_or_else(|| panic!("{label}: {}", reply.result));
        assert_eq!(raised.kind, "Exception", "{label}: {raised:?}");
        assert_eq!(raised.stage, *stage, "{label}: {raised:?}");
        assert!(raised.message.contains(fragment), "{label}: {raised:?}");
        assert!(reply.shader.is_none(), "{label}");
    }
}

/// What the oracle does with a request of [`refusals_and_the_requests_next_to_them`].
#[derive(Debug, Clone, Copy)]
enum Outcome {
    /// The command refuses it, with this fragment in its message.
    Refused(&'static str),
    /// The wheel extracts the shader.
    Extracts,
    /// The wheel raises, with this fragment in its message.
    Raises(&'static str),
}

/// The oracle refuses a request where the wheel would do something undefined, and nothing next
/// to it: names outside printable ASCII; 1D LUTs that don't fit the texture width limit
/// (dividing by zero at a width of 0, looping forever at 1, exhausting memory when the padded
/// rows outnumber the texels, as a LUT of 8191 entries does at the default width); and keys it
/// doesn't know. The same LUTs at other widths, a width of 0 without a LUT, and ACES 2 tables,
/// which the wheel refuses itself, go through.
#[test]
fn refusals_and_the_requests_next_to_them() {
    use Outcome::{Extracts, Raises, Refused};
    let named = |settings: ShaderSettings| {
        GpuShaderRequest::new(transform(analytic(0.5, 1.25, 1.5)), settings)
    };
    let lut = |n: usize, width: Option<u32>| {
        GpuShaderRequest::new(
            transform(lut1d(&curve(n, false))),
            ShaderSettings {
                texture_max_width: width,
                ..ShaderSettings::default()
            },
        )
    };
    let prefix = |p: &str| ShaderSettings {
        resource_prefix: Some(p.into()),
        ..ShaderSettings::default()
    };
    let aces2 = json!({"class": "BuiltinTransform", "args": {
        "style": "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-REC709_2.0"}});
    // A second one of these raises at extraction, after the ops before it.
    let exposure = json!({"class": "ExposureContrastTransform", "args": {"exposure": 0.5},
        "calls": [["makeExposureDynamic"]]});
    let cases: Vec<(&str, GpuShaderRequest, Outcome)> = vec![
        (
            "a non-ASCII prefix",
            named(prefix("caf\u{e9}")),
            Refused("only printable ASCII"),
        ),
        (
            "a line break in a function name",
            named(ShaderSettings {
                function_name: Some("a\nb".into()),
                ..ShaderSettings::default()
            }),
            Refused("only printable ASCII"),
        ),
        (
            "a tab in a uid",
            named(ShaderSettings {
                uid: Some("a\tb".into()),
                ..ShaderSettings::default()
            }),
            Refused("only printable ASCII"),
        ),
        (
            "a non-ASCII pixel name",
            named(ShaderSettings {
                pixel_name: Some("\u{3c0}".into()),
                ..ShaderSettings::default()
            }),
            Refused("only printable ASCII"),
        ),
        (
            "printable punctuation and a double underscore",
            named(prefix("a__b ~!")),
            Extracts,
        ),
        (
            "8191 entries at the default width",
            lut(8191, None),
            Refused("8191 entries"),
        ),
        (
            "8191 entries at the default width, before an op that raises",
            GpuShaderRequest::new(
                transform(json!({"class": "GroupTransform", "children": [
                    lut1d(&curve(8191, false)), exposure.clone(), exposure.clone()]})),
                ShaderSettings::default(),
            ),
            Refused("8191 entries"),
        ),
        (
            "8190 entries at the default width, before an op that raises",
            GpuShaderRequest::new(
                transform(json!({"class": "GroupTransform", "children": [
                    lut1d(&curve(8190, false)), exposure.clone(), exposure]})),
                ShaderSettings::default(),
            ),
            Raises("Dynamic property already here: 0."),
        ),
        (
            "8190 entries at the default width",
            lut(8190, None),
            Extracts,
        ),
        (
            "8192 entries at the default width",
            lut(8192, None),
            Extracts,
        ),
        (
            "5 entries at a width of 0",
            lut(5, Some(0)),
            Refused("5 entries"),
        ),
        (
            "2 entries at a width of 1",
            lut(2, Some(1)),
            Refused("2 entries"),
        ),
        (
            "5 entries at a width of 2",
            lut(5, Some(2)),
            Refused("5 entries"),
        ),
        (
            "4 entries at a width of 2",
            lut(4, Some(2)),
            Refused("4 entries"),
        ),
        ("2 entries at a width of 2", lut(2, Some(2)), Extracts),
        ("4 entries at a width of 3", lut(4, Some(3)), Extracts),
        ("5 entries at a width of 5", lut(5, Some(5)), Extracts),
        (
            "a width of 0 without a LUT",
            named(ShaderSettings {
                texture_max_width: Some(0),
                ..ShaderSettings::default()
            }),
            Extracts,
        ),
        (
            "ACES 2 tables at a width of 1",
            GpuShaderRequest::new(
                transform(aces2),
                ShaderSettings {
                    texture_max_width: Some(1),
                    ..ShaderSettings::default()
                },
            ),
            Raises("1D LUT size exceeds the maximum"),
        ),
    ];
    // Requests the typed builder can't write.
    let raw: Vec<(&str, Value, &'static str)> = vec![
        (
            "an unknown top-level key",
            json!({"transform": {"class": "MatrixTransform"}, "optimisation": "OPTIMIZATION_NONE"}),
            "unknown keys ['optimisation']",
        ),
        (
            "an unknown setting",
            json!({"transform": {"class": "MatrixTransform"}, "shader": {"langauge": "x"}}),
            "unknown keys ['langauge']",
        ),
        (
            "an unknown descriptor set key",
            json!({"transform": {"class": "MatrixTransform"},
                "shader": {"descriptor_set": {"index": 1, "start": 1}}}),
            "unknown keys ['start']",
        ),
        (
            "an unknown language",
            json!({"transform": {"class": "MatrixTransform"},
                "shader": {"language": "GPU_LANGUAGE_GLSL_9"}}),
            "unknown GpuLanguage",
        ),
    ];
    let mut calls: Vec<BatchCall<'_>> = cases.iter().map(|(_, r, _)| r.call()).collect();
    calls.extend(raw.iter().map(|(_, args, _)| BatchCall {
        cmd: "gpu_shader",
        args: args.clone(),
        blobs: Vec::new(),
    }));
    let expected = cases
        .iter()
        .map(|(label, _, outcome)| (*label, *outcome))
        .chain(
            raw.iter()
                .map(|(label, _, fragment)| (*label, Refused(fragment))),
        );
    for ((label, outcome), result) in expected.zip(Oracle::get().batch(&calls, false)) {
        match (outcome, result) {
            (Refused(fragment), Err(e)) => assert!(e.contains(fragment), "{label}: {e}"),
            (Extracts, Ok(response)) => {
                let reply = GpuShaderReply::from_response(response);
                assert!(reply.raised().is_none(), "{label}: {}", reply.result);
            }
            (Raises(fragment), Ok(response)) => {
                let reply = GpuShaderReply::from_response(response);
                let raised: Option<Raised> = reply.raised();
                assert!(
                    raised
                        .as_ref()
                        .is_some_and(|r| r.message.contains(fragment)),
                    "{label}: {raised:?}"
                );
            }
            (outcome, result) => panic!("{label}: expected {outcome:?}, got {result:?}"),
        }
    }
}

/// A request's reply is the same on every run, alone or in a batch, bytes included, so batching
/// and the oracle's cache (keyed by the request) stay valid.
#[test]
fn replies_are_the_same_on_every_run() {
    let requests = [
        GpuShaderRequest::new(
            transform(analytic(0.5, 1.25, 1.5)),
            ShaderSettings::language(GpuLanguage::Msl20),
        ),
        GpuShaderRequest::new(
            json!({
                "transform": {"class": "GroupTransform", "children": [
                    lut1d(&curve(9, false)), lut1d(&curve(6, true))]},
                "optimization": "OPTIMIZATION_NONE",
            }),
            ShaderSettings::language(GpuLanguage::GlslVk46),
        ),
    ];
    let calls: Vec<BatchCall<'_>> = requests.iter().map(GpuShaderRequest::call).collect();
    let runs = [
        Oracle::get().batch(&calls, false),
        Oracle::get().batch(&calls, false),
    ];
    for (i, call) in calls.iter().enumerate() {
        let alone = Oracle::get().call_uncached(call.cmd, call.args.clone(), &call.blobs);
        assert!(
            alone.result.get("exception").is_none(),
            "call {i}: {}",
            alone.result
        );
        for run in &runs {
            let batched = run[i].as_ref().unwrap_or_else(|e| panic!("call {i}: {e}"));
            assert_eq!(batched.result, alone.result, "call {i}");
            assert_eq!(batched.blobs, alone.blobs, "call {i}");
        }
    }
}
