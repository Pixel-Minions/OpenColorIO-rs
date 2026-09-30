// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The oracle's `gpu_shader` command (`oracle/ocio_oracle/gpu.py`, chunk O1.3) against the
//! wheel itself:
//! - every language and setting reaches the shader description;
//! - uniforms, textures and dynamic properties come back with their values, each in its place,
//!   and the grading values exact;
//! - every field agrees with an independent read of the wheel, bit for bit, the processor keys
//!   (`direction`, a config's `src` and `dst`) included; upstream's `VulkanSupport` holds;
//! - a 3D texture holds its LUT in `getData()` order; a missing file raises
//!   `ExceptionMissingFile`;
//! - each message is logged once, as one extraction logs it;
//! - the default GPU processor is the optimized one with the default flags;
//! - every error path raises where the command says; the command's refusals, exactly the names
//!   the wheel can't read and settings of the wrong type; and replies that don't depend on the
//!   run.
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
    GpuLanguage, GpuShaderReply, GpuShaderRequest, Raised, ShaderSettings, UniformValue,
};
use ocio_testkit::oracle::{BatchCall, Response};
use ocio_testkit::processor_ops::{Dumped, ProcessorOpsReply, ProcessorOpsRequest};
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
                Dumped::F64(d) => (p.kind.clone(), d.to_bits()),
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
            &shader.dynamic_properties[0].value,
            Dumped::Object(value) if value.class.starts_with("Grading")
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
/// to it:
/// - MSL resource prefixes the Metal class wrapper can't read back: a first byte past white
///   space that isn't ASCII, or a line feed. Control characters, other names, a non-ASCII byte
///   later in the prefix, and every name in the other languages go through;
/// - 1D LUTs that don't fit the texture width limit (dividing by zero at a width of 0, looping
///   forever at 1, exhausting memory when the padded rows outnumber the texels, as a LUT of
///   8191 entries does at the default width). The same LUTs at other widths, a width of 0
///   without a LUT, and ACES 2 tables, which the wheel refuses itself, go through;
/// - keys it doesn't know, and settings of the wrong type.
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
    let msl = |settings: ShaderSettings| {
        named(ShaderSettings {
            language: Some(GpuLanguage::Msl20),
            ..settings
        })
    };
    let msl_lut = |p: &str| {
        GpuShaderRequest::new(
            json!({"transform": lut1d(&curve(5, false)), "optimization": "OPTIMIZATION_NONE"}),
            ShaderSettings {
                language: Some(GpuLanguage::Msl20),
                resource_prefix: Some(p.into()),
                ..ShaderSettings::default()
            },
        )
    };
    const CAN_T_READ: &str = "passes its first byte past white space to std::isdigit";
    const LINE_FEED: &str = "a line feed in their names";
    let cases: Vec<(&str, GpuShaderRequest, Outcome)> = vec![
        (
            "a non-ASCII byte later in an MSL prefix",
            msl(prefix("caf\u{e9}")),
            Extracts,
        ),
        (
            "an MSL prefix starting with a non-ASCII byte",
            msl(prefix("\u{e9}t\u{e9}")),
            Refused(CAN_T_READ),
        ),
        (
            "an MSL prefix starting with white space, then a non-ASCII byte",
            msl(prefix(" \t\u{3c0}")),
            Refused(CAN_T_READ),
        ),
        (
            "an MSL prefix with a line feed",
            msl(prefix("a\nb")),
            Refused(LINE_FEED),
        ),
        (
            "an MSL prefix with a line feed, for a texture",
            msl_lut("x\ny\nz"),
            Refused(LINE_FEED),
        ),
        (
            "an MSL prefix with tab, carriage return and DEL",
            msl(prefix("a\tb\rc\u{7f}d")),
            Extracts,
        ),
        ("an MSL prefix of white space", msl(prefix(" \t")), Extracts),
        (
            "a non-ASCII MSL function name, and a line feed in its pixel name",
            msl(ShaderSettings {
                function_name: Some("f\u{e9}".into()),
                pixel_name: Some("p\nx".into()),
                ..ShaderSettings::default()
            }),
            Extracts,
        ),
        (
            "a prefix starting with a non-ASCII byte in GLSL",
            named(prefix("\u{e9}t\u{e9}")),
            Extracts,
        ),
        (
            "a line feed in a function name",
            named(ShaderSettings {
                function_name: Some("a\nb".into()),
                ..ShaderSettings::default()
            }),
            Extracts,
        ),
        (
            "a tab in a uid",
            named(ShaderSettings {
                uid: Some("a\tb".into()),
                ..ShaderSettings::default()
            }),
            Extracts,
        ),
        (
            "a non-ASCII pixel name",
            named(ShaderSettings {
                pixel_name: Some("\u{3c0}".into()),
                ..ShaderSettings::default()
            }),
            Extracts,
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
        (
            "a language by number",
            json!({"transform": {"class": "MatrixTransform"}, "shader": {"language": 5}}),
            "language: unknown GpuLanguage 5",
        ),
        (
            "a name that isn't a string",
            json!({"transform": {"class": "MatrixTransform"}, "shader": {"uid": 5}}),
            "uid must be a string, not 5",
        ),
        (
            "a bool as a string",
            json!({"transform": {"class": "MatrixTransform"},
                "shader": {"allow_texture_1d": "false"}}),
            "allow_texture_1d must be true or false, not 'false'",
        ),
        (
            "a bool as a number",
            json!({"transform": {"class": "MatrixTransform"},
                "shader": {"allow_texture_1d": 0}}),
            "allow_texture_1d must be true or false, not 0",
        ),
        (
            "a fractional width",
            json!({"transform": {"class": "MatrixTransform"},
                "shader": {"texture_max_width": 4096.9}}),
            "texture_max_width must be an integer from 0 to 4294967295, not 4096.9",
        ),
        (
            "a width beyond an unsigned",
            json!({"transform": {"class": "MatrixTransform"},
                "shader": {"texture_max_width": 4294967296u64}}),
            "texture_max_width must be an integer from 0 to 4294967295, not 4294967296",
        ),
        (
            "a bool as an index",
            json!({"transform": {"class": "MatrixTransform"},
                "shader": {"descriptor_set": {"index": true, "texture_binding_start": 1}}}),
            "descriptor_set index must be an integer from 0 to 4294967295, not True",
        ),
        (
            "a fractional binding start",
            json!({"transform": {"class": "MatrixTransform"},
                "shader": {"descriptor_set": {"index": 1, "texture_binding_start": 2.5}}}),
            "descriptor_set texture_binding_start must be an integer from 0 to 4294967295, \
             not 2.5",
        ),
        (
            "a descriptor set without its binding start",
            json!({"transform": {"class": "MatrixTransform"},
                "shader": {"descriptor_set": {"index": 1}}}),
            "descriptor_set needs texture_binding_start",
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

/// The four grading transforms, dynamic, with the style each is built with.
const GRADINGS: [(&str, &str); 4] = [
    ("GradingPrimaryTransform", "GRADING_LOG"),
    ("GradingRGBCurveTransform", "GRADING_LIN"),
    ("GradingToneTransform", "GRADING_VIDEO"),
    ("GradingHueCurveTransform", "GRADING_LOG"),
];

fn grading(class: &str, style: &str) -> Value {
    json!({"class": class, "args": {"style": {"enum": style}, "dynamic": true}})
}

/// Reads the dynamic properties' grading values from a GLSL 4.0 extraction, written
/// independently of the command: floats as their bits, curves as their control points and
/// slopes, other values property by property.
const GRADING_READ: &str = r#"
import json, struct, sys
import PyOpenColorIO as OCIO
from ocio_oracle import spec

def bits(v):
    return struct.unpack("<Q", struct.pack("<d", v))[0]

def read(v):
    if isinstance(v, float):
        return bits(v)
    if isinstance(v, OCIO.GradingBSplineCurve):
        return {"points": [[bits(p.x), bits(p.y)] for p in v.getControlPoints()],
                "slopes": [bits(s) for s in v.getSlopes()]}
    return {n: read(getattr(v, n)) for n in dir(type(v))
            if isinstance(getattr(type(v), n), property)}

GETTERS = {
    OCIO.DYNAMIC_PROPERTY_GRADING_PRIMARY: "getGradingPrimary",
    OCIO.DYNAMIC_PROPERTY_GRADING_RGBCURVE: "getGradingRGBCurve",
    OCIO.DYNAMIC_PROPERTY_GRADING_TONE: "getGradingTone",
    OCIO.DYNAMIC_PROPERTY_GRADING_HUECURVE: "getGradingHueCurve",
}

for transform in json.loads(sys.argv[1]):
    proc = OCIO.Config.CreateRaw().getProcessor(spec.transform(transform),
                                                OCIO.TRANSFORM_DIR_FORWARD)
    desc = OCIO.GpuShaderDesc.CreateShaderDesc(language=OCIO.GPU_LANGUAGE_GLSL_4_0)
    proc.getDefaultGPUProcessor().extractGpuShaderInfo(desc)
    print(json.dumps([{"type": p.getType().name, "value": read(getattr(p, GETTERS[p.getType()])())}
                      for p in desc.getDynamicProperties()]))
"#;

/// Checks a reported value against `GRADING_READ`'s reading of it, at `path`.
fn check_grading_value(reported: &Dumped, expected: &Value, path: &str) {
    match expected {
        Value::Number(bits) => {
            assert_eq!(Some(reported.f64().to_bits()), bits.as_u64(), "{path}");
        }
        Value::Object(fields) if fields.contains_key("points") => {
            let curve = reported.object();
            let Dumped::List(points) = curve.getter("getControlPoints") else {
                panic!("{path}: {curve:?}");
            };
            let points: Vec<Value> = points
                .iter()
                .map(|p| {
                    let p = p.object();
                    json!([
                        p.property("x").f64().to_bits(),
                        p.property("y").f64().to_bits()
                    ])
                })
                .collect();
            assert_eq!(Value::from(points), fields["points"], "{path}.points");
            let slopes: Vec<u64> = curve
                .getter("getSlopes")
                .f64s()
                .iter()
                .map(|s| s.to_bits())
                .collect();
            assert_eq!(json!(slopes), fields["slopes"], "{path}.slopes");
        }
        Value::Object(fields) => {
            let object = reported.object();
            for (name, value) in fields {
                check_grading_value(object.property(name), value, &format!("{path}.{name}"));
            }
        }
        other => panic!("{path}: {other}"),
    }
}

/// The grading transforms' dynamic properties come back exact and whole: every float of the
/// value, bit for bit, and every control point and slope of its curves, as an independent read
/// of the wheel gives them (`GRADING_READ`). (The binding's `repr()` of these values, which the
/// command reported before, prints 6 digits, and an address for a `GradingRGBCurve`.)
#[test]
fn grading_values_match_an_independent_read() {
    let transforms: Vec<Value> = GRADINGS
        .iter()
        .map(|(class, style)| grading(class, style))
        .collect();
    let requests: Vec<GpuShaderRequest> = transforms
        .iter()
        .map(|t| {
            GpuShaderRequest::new(
                transform(t.clone()),
                ShaderSettings::language(GpuLanguage::Glsl40),
            )
        })
        .collect();
    let lines = Oracle::get().run_script(
        GRADING_READ,
        &[serde_json::to_string(&transforms).expect("JSON")],
    );
    assert_eq!(lines.len(), transforms.len(), "{lines:?}");
    for ((reply, line), (class, _)) in run(&requests).iter().zip(&lines).zip(GRADINGS) {
        let expected: Value = serde_json::from_str(line).expect("the script's JSON");
        let expected = expected.as_array().expect("properties");
        let properties = &reply.shader().dynamic_properties;
        assert_eq!(properties.len(), expected.len(), "{class}");
        for (property, read) in properties.iter().zip(expected) {
            assert_eq!(property.kind, read["type"], "{class}");
            check_grading_value(&property.value, &read["value"], class);
        }
    }
}

/// Each OCIO message is logged once, as a single extraction logs it: the probe extraction that
/// the texture width check runs first logs nothing into the reply. A dynamic exposure, contrast
/// and gamma in OSL warn that they become local variables; the grading transforms, dynamic in
/// OSL, log what they log.
#[test]
fn warnings_are_logged_once() {
    const SCRIPT: &str = r#"
import json, sys
import PyOpenColorIO as OCIO
from ocio_oracle import spec

for transform in json.loads(sys.argv[1]):
    messages = []
    OCIO.SetLoggingFunction(messages.append)
    proc = OCIO.Config.CreateRaw().getProcessor(spec.transform(transform),
                                                OCIO.TRANSFORM_DIR_FORWARD)
    desc = OCIO.GpuShaderDesc.CreateShaderDesc(language=OCIO.LANGUAGE_OSL_1)
    proc.getDefaultGPUProcessor().extractGpuShaderInfo(desc)
    OCIO.ResetToDefaultLoggingFunction()
    print(json.dumps(messages))
"#;
    let mut transforms = vec![analytic(0.5, 1.25, 1.5)];
    transforms.extend(GRADINGS.iter().map(|(class, style)| grading(class, style)));
    let requests: Vec<GpuShaderRequest> = transforms
        .iter()
        .map(|t| {
            GpuShaderRequest::new(
                transform(t.clone()),
                ShaderSettings::language(GpuLanguage::Osl1),
            )
        })
        .collect();
    let lines =
        Oracle::get().run_script(SCRIPT, &[serde_json::to_string(&transforms).expect("JSON")]);
    assert_eq!(lines.len(), transforms.len(), "{lines:?}");
    for (i, ((reply, line), t)) in run(&requests)
        .iter()
        .zip(&lines)
        .zip(&transforms)
        .enumerate()
    {
        let once: Vec<String> = serde_json::from_str(line).expect("the script's JSON");
        assert!(i > 0 || !once.is_empty(), "{t}: no warning to check");
        assert_eq!(reply.log(), once, "{t}");
    }
}

/// A request's reply is the same on every run, alone or in a batch, bytes included, so batching
/// and the oracle's cache (keyed by the request) stay valid: for MSL, with control characters
/// in the resource prefix, and for the grading transforms' dynamic properties.
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
        GpuShaderRequest::new(
            transform(analytic(0.5, 1.25, 1.5)),
            ShaderSettings {
                language: Some(GpuLanguage::Msl20),
                resource_prefix: Some("a\tb\rc\u{7f}d".into()),
                ..ShaderSettings::default()
            },
        ),
    ]
    .into_iter()
    .chain(GRADINGS.iter().map(|(class, style)| {
        GpuShaderRequest::new(
            transform(grading(class, style)),
            ShaderSettings::language(GpuLanguage::Glsl40),
        )
    }))
    .collect::<Vec<_>>();
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

/// Reads shaders the command's way, written independently of it: the processor's and the GPU
/// processor's cache IDs and flags, the description's cache ID and text, each uniform's name,
/// type, offset and value read with its type's getter, each texture's names, size, channel,
/// dimensions, interpolation, binding index and values, and the dynamic properties' types.
/// Floats as bits. The cases are JSON: the processor keys, "optimization", and "shader" with
/// the settings the cases use.
const SHADER_READ: &str = r#"
import json, struct, sys
import numpy as np
import PyOpenColorIO as OCIO
from ocio_oracle import spec

def bits64(v):
    return struct.unpack("<Q", struct.pack("<d", v))[0]

def bits32(values):
    return [struct.unpack("<I", struct.pack("<f", v))[0] for v in values]

def uniform(name, data):
    kind = data.type
    if kind == OCIO.UNIFORM_DOUBLE:
        value = bits64(data.getDouble())
    elif kind == OCIO.UNIFORM_BOOL:
        value = data.getBool()
    elif kind == OCIO.UNIFORM_FLOAT3:
        value = bits32(data.getFloat3())
    elif kind == OCIO.UNIFORM_VECTOR_FLOAT:
        value = bits32(np.asarray(data.getVectorFloat(), dtype=np.float32).tolist())
    elif kind == OCIO.UNIFORM_VECTOR_INT:
        value = [int(i) for i in data.getVectorInt()]
    else:
        value = None
    return {"name": name, "type": kind.name, "offset": data.bufferOffset, "value": value}

for case in json.loads(sys.argv[1]):
    config = spec.config(case.get("config"))
    if "transform" in case:
        direction = getattr(OCIO, case.get("direction", "TRANSFORM_DIR_FORWARD"))
        proc = config.getProcessor(spec.transform(case["transform"]), direction)
    else:
        proc = config.getProcessor(case["src"], case["dst"])
    if "optimization" in case:
        gpu = proc.getOptimizedGPUProcessor(spec.flags(case["optimization"]))
    else:
        gpu = proc.getDefaultGPUProcessor()
    settings = case.get("shader", {})
    desc = OCIO.GpuShaderDesc.CreateShaderDesc()
    if "language" in settings:
        desc.setLanguage(getattr(OCIO.GpuLanguage, settings["language"]))
    if "descriptor_set" in settings:
        desc.setDescriptorSetIndex(settings["descriptor_set"]["index"],
                                   settings["descriptor_set"]["texture_binding_start"])
    if "texture_max_width" in settings:
        desc.setTextureMaxWidth(settings["texture_max_width"])
    gpu.extractGpuShaderInfo(desc)
    values = lambda t: bits32(np.asarray(t.getValues(), dtype=np.float32).tolist())
    print(json.dumps({
        "processor_cache_id": proc.getCacheID(), "gpu_cache_id": gpu.getCacheID(),
        "isNoOp": gpu.isNoOp(), "hasChannelCrosstalk": gpu.hasChannelCrosstalk(),
        "cache_id": desc.getCacheID(), "text": desc.getShaderText(),
        "uniforms": [uniform(name, data) for name, data in desc.getUniforms()],
        "textures": [{"name": t.textureName, "sampler": t.samplerName, "width": t.width,
                      "height": t.height, "channel": t.channel.name,
                      "dimensions": t.dimensions.name, "interpolation": t.interpolation.name,
                      "binding": t.textureShaderBindingIndex, "values": values(t)}
                     for t in desc.getTextures()],
        "textures_3d": [{"name": t.textureName, "sampler": t.samplerName, "edge": t.edgeLen,
                         "interpolation": t.interpolation.name,
                         "binding": t.textureShaderBindingIndex, "values": values(t)}
                        for t in desc.get3DTextures()],
        "dynamic": [p.getType().name for p in desc.getDynamicProperties()],
    }))
"#;

/// A reply in `SHADER_READ`'s form.
fn shader_summary(reply: &GpuShaderReply) -> Value {
    let shader = reply.shader();
    let bits = |v: &[f32]| v.iter().map(|x| x.to_bits()).collect::<Vec<u32>>();
    let uniforms: Vec<Value> = shader
        .uniforms
        .iter()
        .map(|u| {
            let value = match &u.value {
                UniformValue::Double(d) => json!(d.to_bits()),
                UniformValue::Bool(b) => json!(b),
                UniformValue::Float3(f) => json!(bits(f)),
                UniformValue::VectorFloat(v) => json!(bits(v)),
                UniformValue::VectorInt(v) => json!(v),
                UniformValue::Unknown => Value::Null,
            };
            json!({"name": u.name, "type": u.kind, "offset": u.buffer_offset, "value": value})
        })
        .collect();
    let textures: Vec<Value> = shader
        .textures
        .iter()
        .map(|t| {
            json!({"name": t.name, "sampler": t.sampler_name, "width": t.width,
                "height": t.height, "channel": t.channel, "dimensions": t.dimensions,
                "interpolation": t.interpolation, "binding": t.binding_index,
                "values": bits(&t.values)})
        })
        .collect();
    let textures_3d: Vec<Value> = shader
        .textures_3d
        .iter()
        .map(|t| {
            json!({"name": t.name, "sampler": t.sampler_name, "edge": t.edge_len,
                "interpolation": t.interpolation, "binding": t.binding_index,
                "values": bits(&t.values)})
        })
        .collect();
    let dynamic: Vec<&str> = shader
        .dynamic_properties
        .iter()
        .map(|p| p.kind.as_str())
        .collect();
    json!({
        "processor_cache_id": reply.result["processor_cache_id"],
        "gpu_cache_id": reply.result["gpu_cache_id"],
        "isNoOp": reply.result["gpu_processor"]["isNoOp"],
        "hasChannelCrosstalk": reply.result["gpu_processor"]["hasChannelCrosstalk"],
        "cache_id": shader.cache_id, "text": shader.text,
        "uniforms": uniforms, "textures": textures, "textures_3d": textures_3d,
        "dynamic": dynamic,
    })
}

/// The command agrees with an independent read of the wheel (`SHADER_READ`), field by field
/// and bit for bit, for:
/// - every dynamic property at once, which gives uniforms of every type;
/// - three LUTs in Vulkan GLSL with a descriptor set: a 1D texture, a 2D texture of two rows
///   (a width limit of 4), and a 3D texture, their binding indices from 10;
/// - a log inverted by the `direction` key, in MSL;
/// - a config's source and destination color spaces, in HLSL;
/// - a matrix with crosstalk, in OSL, and a no-op, in Cg.
#[test]
fn shaders_match_an_independent_read() {
    let config = json!({"yaml": "ocio_profile_version: 2
roles:
  default: ref
colorspaces:
  - !<ColorSpace>
    name: ref
  - !<ColorSpace>
    name: graded
    from_scene_reference: !<MatrixTransform> {offset: [0.125, -0.25, 0.0625, 0.5]}
"});
    let mut dynamic = vec![analytic(0.5, 1.25, 1.5)];
    dynamic.extend(GRADINGS.iter().map(|(class, style)| grading(class, style)));
    // A curve that isn't the identity: its knots and coefficients (vectors of floats) are then
    // uniforms of their own values, which the identity leaves empty.
    dynamic[2]["calls"] = json!([["setSlope", {"enum": "RGB_RED"}, 1, 0.5]]);
    let lut3d = {
        let calls: Vec<Value> = (0..8)
            .map(|i| {
                let (r, g, b) = (i / 4, (i / 2) % 2, i % 2);
                json!([
                    "setValue",
                    r,
                    g,
                    b,
                    0.1 + 0.3 * f64::from(r),
                    0.2 + 0.25 * f64::from(g),
                    0.05 + 0.5 * f64::from(b) + 0.01 * f64::from(i)
                ])
            })
            .collect();
        json!({"class": "Lut3DTransform", "args": {"gridSize": 2}, "calls": calls})
    };
    let cases: Vec<(Value, ShaderSettings)> = vec![
        (
            json!({"transform": {"class": "GroupTransform", "children": dynamic}}),
            ShaderSettings::language(GpuLanguage::Glsl40),
        ),
        (
            json!({"transform": {"class": "GroupTransform", "children": [
                    lut1d(&curve(9, false)), lut1d(&curve(6, false)), lut3d]},
                "optimization": "OPTIMIZATION_NONE"}),
            ShaderSettings {
                language: Some(GpuLanguage::GlslVk46),
                descriptor_set: Some((2, 10)),
                texture_max_width: Some(4),
                ..ShaderSettings::default()
            },
        ),
        (
            json!({"transform": {"class": "LogTransform", "args": {"base": 2.0}},
                "direction": "TRANSFORM_DIR_INVERSE"}),
            ShaderSettings::language(GpuLanguage::Msl20),
        ),
        (
            json!({"config": config, "src": "ref", "dst": "graded"}),
            ShaderSettings::language(GpuLanguage::HlslSm50),
        ),
        (
            json!({"transform": {"class": "MatrixTransform", "args": {"matrix":
                [1.0, 0.25, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]}}}),
            ShaderSettings::language(GpuLanguage::Osl1),
        ),
        (
            json!({"transform": {"class": "MatrixTransform"}}),
            ShaderSettings::language(GpuLanguage::Cg),
        ),
    ];
    let requests: Vec<GpuShaderRequest> = cases
        .iter()
        .map(|(processor, settings)| GpuShaderRequest::new(processor.clone(), settings.clone()))
        .collect();
    let script_cases: Vec<Value> = requests.iter().map(GpuShaderRequest::args).collect();
    let lines = Oracle::get().run_script(
        SHADER_READ,
        &[serde_json::to_string(&script_cases).expect("JSON")],
    );
    assert_eq!(lines.len(), cases.len(), "{lines:?}");
    for (i, (reply, line)) in run(&requests).iter().zip(&lines).enumerate() {
        let expected: Value = serde_json::from_str(line).expect("the script's JSON");
        assert_eq!(shader_summary(reply), expected, "case {i}");
    }
}

/// Upstream's GpuShader test `VulkanSupport` (tests/cpu/GpuShader_tests.cpp:1368-1531
/// @ v2.5.2), run on the wheel through the command: upstream's `lut1d_lut3d_lut1d.clf` without
/// optimization, in Vulkan GLSL with the descriptor set (2, 10), gives upstream's textures,
/// binding indices and shader text. (Upstream builds the processor from `Config::Create()`; a
/// file's ops don't depend on the config, and this uses the raw one.)
#[test]
fn vulkan_bindings_as_upstream_expects() {
    // tests/cpu/GpuShader_tests.cpp:1462-1528 @ v2.5.2, verbatim.
    const EXPECTED: &str = r#"
// Declaration of all textures

layout(set=2, binding = 10) uniform sampler1D ocio_lut1d_0Sampler; 
layout(set=2, binding = 11) uniform sampler3D ocio_lut3d_1Sampler; 
layout(set=2, binding = 12) uniform sampler2D ocio_lut1d_2Sampler; 

// Declaration of all helper methods

vec2 ocio_lut1d_2_computePos(float f)
{
  float dep;
  float abs_f = abs(f);
  if (abs_f > 6.10351562e-05)
  {
    vec3 fComp = vec3(15., 15., 15.);
    float absarr = min( abs_f, 65504.);
    fComp.x = floor( log2( absarr ) );
    float lower = pow( 2.0, fComp.x );
    fComp.y = ( absarr - lower ) / lower;
    vec3 scale = vec3(1024., 1024., 1024.);
    dep = dot( fComp, scale );
  }
  else
  {
    dep = abs_f * 16777216.;
  }
  dep += (f < 0.) ? 32768.0 : 0.0;
  vec2 retVal;
  retVal.y = floor(dep / 4095.);
  retVal.x = dep - retVal.y * 4095.;
  retVal.x = (retVal.x + 0.5) / 4096.;
  retVal.y = (retVal.y + 0.5) / 17.;
  return retVal;
}

// Declaration of the OCIO shader function

vec4 OCIOMain(vec4 inPixel)
{
  vec4 outColor = inPixel;
  
  // Add LUT 1D processing for ocio_lut1d_0
  
  {
    vec3 ocio_lut1d_0_coords = (outColor.rgb * vec3(64., 64., 64.) + vec3(0.5, 0.5, 0.5) ) / vec3(65., 65., 65.);
    outColor.r = texture(ocio_lut1d_0Sampler, ocio_lut1d_0_coords.r).r;
    outColor.g = texture(ocio_lut1d_0Sampler, ocio_lut1d_0_coords.g).r;
    outColor.b = texture(ocio_lut1d_0Sampler, ocio_lut1d_0_coords.b).r;
  }
  
  // Add LUT 3D processing for ocio_lut3d_1
  
  vec3 ocio_lut3d_1_coords = (outColor.zyx * vec3(2., 2., 2.) + vec3(0.5, 0.5, 0.5)) / vec3(3., 3., 3.);
  outColor.rgb = texture(ocio_lut3d_1Sampler, ocio_lut3d_1_coords).rgb;
  
  // Add LUT 1D processing for ocio_lut1d_2
  
  {
    outColor.r = texture(ocio_lut1d_2Sampler, ocio_lut1d_2_computePos(outColor.r)).r;
    outColor.g = texture(ocio_lut1d_2Sampler, ocio_lut1d_2_computePos(outColor.g)).r;
    outColor.b = texture(ocio_lut1d_2Sampler, ocio_lut1d_2_computePos(outColor.b)).r;
  }

  return outColor;
}
"#;
    let clf =
        ocio_testkit::paths::upstream_dir().join("tests/data/files/clf/lut1d_lut3d_lut1d.clf");
    let request = GpuShaderRequest::new(
        json!({"transform": {"class": "FileTransform", "args": {"src": clf}},
            "optimization": "OPTIMIZATION_NONE"}),
        ShaderSettings {
            language: Some(GpuLanguage::GlslVk46),
            descriptor_set: Some((2, 10)),
            ..ShaderSettings::default()
        },
    );
    let reply = &run(&[request])[0];
    let shader = reply.shader();
    assert_eq!(shader.getters["descriptor_set_index"], 2);
    assert_eq!(shader.textures.len(), 2);
    assert_eq!(shader.textures_3d.len(), 1);
    let texture = |t: &ocio_testkit::gpu::Texture| {
        (
            t.name.clone(),
            t.sampler_name.clone(),
            t.width,
            t.height,
            t.channel.clone(),
            t.dimensions.clone(),
            t.interpolation.clone(),
            t.binding_index,
        )
    };
    let text = String::from;
    assert_eq!(
        texture(&shader.textures[0]),
        (
            text("ocio_lut1d_0"),
            text("ocio_lut1d_0Sampler"),
            65,
            1,
            text("TEXTURE_RED_CHANNEL"),
            text("TEXTURE_1D"),
            text("INTERP_LINEAR"),
            10
        )
    );
    assert_eq!(
        texture(&shader.textures[1]),
        (
            text("ocio_lut1d_2"),
            text("ocio_lut1d_2Sampler"),
            4096,
            17,
            text("TEXTURE_RED_CHANNEL"),
            text("TEXTURE_2D"),
            text("INTERP_LINEAR"),
            12
        )
    );
    let cube = &shader.textures_3d[0];
    assert_eq!(
        (
            cube.name.as_str(),
            cube.sampler_name.as_str(),
            cube.edge_len,
            cube.interpolation.as_str(),
            cube.binding_index
        ),
        (
            "ocio_lut3d_1",
            "ocio_lut3d_1Sampler",
            3,
            "INTERP_LINEAR",
            11
        )
    );
    assert_eq!(shader.text, EXPECTED);
}

/// A missing file raises `ExceptionMissingFile`, which in PyOpenColorIO isn't an
/// `OCIO.Exception`, where and as the wheel raises it (an independent read gives the type and
/// the message).
#[test]
fn a_missing_file_raises_exception_missing_file() {
    const SCRIPT: &str = r#"
import sys
import PyOpenColorIO as OCIO
try:
    OCIO.Config.CreateRaw().getProcessor(OCIO.FileTransform(src=sys.argv[1]))
    print("nothing raised")
except Exception as exc:
    print(type(exc).__name__)
    print(str(exc))
"#;
    let missing = ocio_testkit::paths::target_dir().join("gpu-oracle-no-such-file.clf");
    assert!(!missing.exists(), "{}", missing.display());
    let request = GpuShaderRequest::new(
        json!({"transform": {"class": "FileTransform", "args": {"src": missing}}}),
        ShaderSettings::default(),
    );
    let raised = run(&[request])[0].raised().expect("an exception");
    let lines = Oracle::get().run_script(SCRIPT, &[missing.display().to_string()]);
    assert_eq!(raised.kind, lines[0]);
    assert_eq!(raised.message, lines[1..].join("\n"));
    assert_eq!(raised.stage, "processor");
}

/// A 3D texture holds a Lut3D's values in the order its `getData()` gives them: the bytes the
/// `processor_ops` command reports for the same transform, entry by entry distinct.
#[test]
fn a_3d_texture_holds_the_lut_in_get_data_order() {
    let calls: Vec<Value> = (0..27)
        .map(|i| {
            let (r, g, b) = (i / 9, (i / 3) % 3, i % 3);
            json!([
                "setValue",
                r,
                g,
                b,
                0.01 * f64::from(i),
                0.5 + 0.01 * f64::from(i),
                1.0 - 0.02 * f64::from(i)
            ])
        })
        .collect();
    let lut = json!({"class": "Lut3DTransform", "args": {"gridSize": 3}, "calls": calls});
    let gpu = GpuShaderRequest::new(
        json!({"transform": lut, "optimization": "OPTIMIZATION_NONE"}),
        ShaderSettings::language(GpuLanguage::Glsl40),
    );
    let mut ops = ProcessorOpsRequest::new(json!({"transform": lut}));
    ops.optimization = Some(json!("OPTIMIZATION_NONE"));
    let responses = batch(&[gpu.call(), ops.call()]);
    let mut responses = responses.into_iter();
    let shader_reply = GpuShaderReply::from_response(responses.next().expect("two"));
    let ops_reply = ProcessorOpsReply::from_response(responses.next().expect("two"));
    let bits = |v: &[f32]| v.iter().map(|x| x.to_bits()).collect::<Vec<u32>>();
    let texture = &shader_reply.shader().textures_3d[0].values;
    let data = ops_reply.processor().group.children[0]
        .getter("getData")
        .f32s();
    assert_eq!(texture.len(), 27 * 3);
    assert_eq!(bits(texture), bits(&data));
}
