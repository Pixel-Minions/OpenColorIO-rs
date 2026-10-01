// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The shader helpers of `GpuShaderUtils` (1.7b) against the wheel's shaders, in the 10
//! languages: upstream's op writers call them, so the lines they write are in the shaders the
//! wheel extracts. Each check rebuilds a line or a declaration as the writer it cites builds
//! it, with the port's helpers and the names, binding indices and settings the wheel reports,
//! and finds it, whole and byte for byte, in the wheel's shader text.
//!
//! The literals of every language, Cg's clamp to the half range included, are checked the
//! same way through the Matrix writer.

use ocio_gpu::GpuLanguage;
use ocio_gpu::gpu_shader_utils::{GpuShaderText, build_resource_name};
use ocio_testkit::battery::yaml_list;
use ocio_testkit::gpu::{self as oracle_gpu, GpuShaderReply, GpuShaderRequest, ShaderSettings};
use ocio_testkit::probe::{self, Rng};
use ocio_testkit::{Oracle, assert_text_eq};
use serde_json::{Value, json};

fn utf8(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).expect("the helpers write ASCII here")
}

/// The port's languages, each with the oracle's name for it.
fn languages() -> impl Iterator<Item = (GpuLanguage, oracle_gpu::GpuLanguage)> {
    GpuLanguage::ALL
        .into_iter()
        .zip(oracle_gpu::GpuLanguage::ALL)
}

/// Panics unless `text` has the lines of `block`, in a row and whole. On a mismatch, the
/// lines that start like `block` are compared with it, to show the difference.
fn assert_has_block(label: &str, text: &str, block: &str) {
    let want: Vec<&str> = block
        .strip_suffix('\n')
        .unwrap_or(block)
        .split('\n')
        .collect();
    let lines: Vec<&str> = text.split('\n').collect();
    if lines.windows(want.len()).any(|w| w == want.as_slice()) {
        return;
    }
    let first = want[0];
    let common = |l: &str| {
        l.bytes()
            .zip(first.bytes())
            .take_while(|(a, b)| a == b)
            .count()
    };
    let best = (0..lines.len())
        .max_by_key(|&i| common(lines[i]))
        .unwrap_or(0);
    let end = (best + want.len()).min(lines.len());
    assert_text_eq(
        &format!("{label}: the wheel's lines that start most like the expected ones"),
        &lines[best..end].join("\n"),
        &want.join("\n"),
    );
    panic!("{label}: the wheel's shader has no lines {block:?}\n{text}");
}

/// Panics unless a line of `text`, without its indentation (which is the writer's, not the
/// helpers'), is `line`.
fn assert_has_line(label: &str, text: &str, line: &str) {
    let lines: Vec<&str> = text.split('\n').map(str::trim_start).collect();
    if lines.contains(&line) {
        return;
    }
    let common = |l: &str| {
        l.bytes()
            .zip(line.bytes())
            .take_while(|(a, b)| a == b)
            .count()
    };
    let best = lines
        .iter()
        .copied()
        .max_by_key(|l| common(l))
        .unwrap_or("");
    assert_text_eq(
        &format!("{label}: the wheel's line that starts most like the expected one"),
        best,
        line,
    );
    panic!("{label}: the wheel's shader has no line {line:?}\n{text}");
}

/// A processor of the case, extracted without optimization (`getOptimizedGPUProcessor
/// (OPTIMIZATION_NONE)`), so that each transform gives its own op.
struct Case {
    name: &'static str,
    transform: Value,
    allow_texture_1d: Option<bool>,
    resource_prefix: Option<&'static str>,
}

impl Case {
    fn new(name: &'static str, transform: Value) -> Case {
        Case {
            name,
            transform,
            allow_texture_1d: None,
            resource_prefix: None,
        }
    }

    fn request(&self, language: oracle_gpu::GpuLanguage) -> GpuShaderRequest {
        GpuShaderRequest::new(
            json!({"transform": self.transform, "optimization": "OPTIMIZATION_NONE"}),
            ShaderSettings {
                language: Some(language),
                allow_texture_1d: self.allow_texture_1d,
                resource_prefix: self.resource_prefix.map(String::from),
                ..ShaderSettings::default()
            },
        )
    }
}

fn lut3d() -> Value {
    let mut calls = vec![json!(["setInterpolation", {"enum": "INTERP_LINEAR"}])];
    for r in 0..2u32 {
        for g in 0..2u32 {
            for b in 0..2u32 {
                let (x, y, z) = (f64::from(r), f64::from(g), f64::from(b));
                calls.push(json!([
                    "setValue",
                    r,
                    g,
                    b,
                    x * 0.9,
                    y * 0.8 + 0.05,
                    z * 0.7 + 0.1
                ]));
            }
        }
    }
    json!({"class": "Lut3DTransform", "args": {"gridSize": 2}, "calls": calls})
}

fn lut1d() -> Value {
    let calls: Vec<Value> = (0..4)
        .map(|i| {
            let i = f64::from(i);
            json!(["setValue", i as u32, i * 0.3, i * 0.31, i * 0.29])
        })
        .collect();
    json!({"class": "Lut1DTransform", "args": {"length": 4}, "calls": calls})
}

fn exposure_contrast() -> Value {
    json!({
        "class": "ExposureContrastTransform",
        "args": {"exposure": 0.5, "contrast": 1.2, "gamma": 1.1, "pivot": 0.18},
        "calls": [["makeExposureDynamic"], ["makeContrastDynamic"], ["makeGammaDynamic"]],
    })
}

/// A non-diagonal matrix with offsets, the values row by row.
const MATRIX: [f64; 16] = [
    1.1,
    0.2,
    -0.3,
    0.0,
    0.01,
    0.9,
    1.0 / 3.0,
    0.0,
    0.25,
    -0.125,
    1.5,
    0.0,
    0.0,
    0.0,
    0.0,
    1.0,
];
const OFFSETS: [f64; 4] = [0.1, 0.0, 0.0, 0.0];

/// Values Cg clamps: beyond the half range, and below the smallest normal half.
const MATRIX_CG: [f64; 16] = [
    1e6, 0.2, -0.3, 0.0, 0.01, -70000.0, 65504.0, 0.0, 1e-9, -0.125, 65520.0, 0.0, 0.0, 0.0,
    6.0e-5, 1.0,
];
const OFFSETS_CG: [f64; 4] = [123456.0, -1e-7, 6.103515625e-5, -65505.0];

fn matrix(values: &[f64; 16], offsets: &[f64; 4]) -> Value {
    json!({"class": "MatrixTransform", "args": {"matrix": values, "offset": offsets}})
}

fn cases() -> Vec<Case> {
    vec![
        Case::new("lut3d", lut3d()),
        Case {
            allow_texture_1d: Some(true),
            ..Case::new("lut1d as 1D", lut1d())
        },
        Case {
            allow_texture_1d: Some(false),
            ..Case::new("lut1d as 2D", lut1d())
        },
        Case {
            resource_prefix: Some("pre__x_"),
            ..Case::new("exposure contrast", exposure_contrast())
        },
        Case::new("exposure contrast, default prefix", exposure_contrast()),
        Case::new("matrix", matrix(&MATRIX, &OFFSETS)),
        Case::new("matrix, clamped in Cg", matrix(&MATRIX_CG, &OFFSETS_CG)),
        Case::new(
            "exponent, mirror",
            json!({"class": "ExponentTransform", "args": {"value": [2.2, 2.4, 1.8, 1.0],
                   "negativeStyle": {"enum": "NEGATIVE_MIRROR"}}}),
        ),
        Case::new(
            "exponent, pass through",
            json!({"class": "ExponentTransform", "args": {"value": [2.2, 2.4, 1.8, 1.0],
                   "negativeStyle": {"enum": "NEGATIVE_PASS_THRU"}}}),
        ),
        Case::new(
            "log camera",
            json!({"class": "LogCameraTransform", "args": {"linSideBreak": [0.1, 0.1, 0.1]}}),
        ),
        Case::new(
            "cdl, no clamp",
            json!({"class": "CDLTransform",
                   "args": {"slope": [1.1, 0.9, 1.3], "offset": [0.01, 0.0, -0.02],
                            "power": [1.2, 1.0, 0.8], "sat": 1.1},
                   "calls": [["setStyle", {"enum": "CDL_NO_CLAMP"}]]}),
        ),
        Case::new(
            "red modifier",
            json!({"class": "FixedFunctionTransform",
                   "args": {"style": {"enum": "FIXED_FUNCTION_ACES_RED_MOD_03"}}}),
        ),
        Case::new(
            "aces 2 output transform",
            json!({"class": "FixedFunctionTransform",
                   "args": {"style": {"enum": "FIXED_FUNCTION_ACES_OUTPUT_TRANSFORM_20"},
                            "params": [100.0, 0.708, 0.292, 0.170, 0.797, 0.131, 0.046,
                                       0.3127, 0.3290]}}),
        ),
        Case {
            resource_prefix: Some("pre__x_"),
            ..Case::new(
                "rgb curve",
                json!({"class": "GradingRGBCurveTransform",
                       "args": {"style": {"enum": "GRADING_LOG"}}, "calls": [["makeDynamic"]]}),
            )
        },
        Case::new(
            "rgb curve, default prefix",
            json!({"class": "GradingRGBCurveTransform",
                   "args": {"style": {"enum": "GRADING_LOG"}}, "calls": [["makeDynamic"]]}),
        ),
    ]
}

/// The first error of a LUT's declaration and lookup, as the Lut1D and Lut3D writers make
/// them (Lut1DOpGPU.cpp:225-227, 311-313 and 343-371, Lut3DOpGPU.cpp:52-56 @ v2.5.2).
fn lut_error(lang: GpuLanguage, dimensions: u32) -> String {
    let ss = GpuShaderText::new(lang);
    let result = match dimensions {
        1 => ss
            .declare_tex1d("lut", 0, 1)
            .and_then(|()| ss.sample_tex1d("lut", "lut_coords.r").map(drop)),
        2 => ss
            .declare_tex2d("lut", 0, 1)
            .and_then(|()| ss.sample_tex2d("lut", "pos").map(drop)),
        _ => ss
            .declare_tex3d("lut", 0, 1)
            .and_then(|()| ss.sample_tex3d("lut", "lut_coords").map(drop)),
    };
    result.expect_err("an error").message().to_string()
}

/// The 3x3 matrices of the ACES 2 output transform, read from the GLSL shader's
/// `mat3(...) * v` products (`matrix3Mul`, GpuShaderUtils.cpp:993-1001 @ v2.5.2, which writes
/// the values transposed, `getMatrixValues`, GpuShaderUtils.cpp:221-237), with the left side
/// and the vector of each line: (left side, values by rows, vector).
fn aces2_matrices(glsl_text: &str) -> Vec<(String, [f32; 9], String)> {
    let mut found = Vec::new();
    for line in glsl_text.split('\n').map(str::trim_start) {
        let (Some(eq), Some(start)) = (line.find(" = "), line.find("mat3(")) else {
            continue;
        };
        let rest = &line[start + "mat3(".len()..];
        let Some(close) = rest.find(')') else {
            continue;
        };
        let text: Vec<f32> = rest[..close]
            .split(", ")
            .filter_map(|v| v.trim_end_matches('.').parse().ok())
            .collect();
        let Some(vector) = rest[close..]
            .strip_prefix(") * ")
            .and_then(|v| v.strip_suffix(';'))
        else {
            continue;
        };
        if text.len() != 9 {
            continue;
        }
        // text[i] = m[(i % 3) * 3 + i / 3], so m[r * 3 + c] = text[c * 3 + r].
        let m: [f32; 9] = std::array::from_fn(|i| text[(i % 3) * 3 + i / 3]);
        found.push((line[..eq].to_string(), m, vector.to_string()));
    }
    found
}

/// In the 10 languages, the port's helpers write the lines and declarations of the wheel's
/// shaders:
/// - `declareTex1D/2D/3D` and `getSamplerName` (Lut1D and Lut3D), and the lookups
///   `sampleTex1D/2D/3D`; where the wheel raises, the helpers raise its message;
/// - `BuildResourceName`, `declareUniformFloat` (ExposureContrast), `declareUniformArrayInt`,
///   `declareUniformArrayFloat`, `declareUniformBool` and `castToBool` (GradingRGBCurve);
/// - `mat4fMul` and `float4Const` through the Matrix writer, and `mat3fMul` (ACES 2);
/// - `sign` and `float4GreaterThan` (Exponent), `float3GreaterThan` (Log), `lerp` (CDL) and
///   `atan2` (the red modifier).
#[test]
fn helpers_write_the_wheels_lines() {
    let cases = cases();
    let requests: Vec<_> = cases
        .iter()
        .flat_map(|case| languages().map(move |(lang, oracle)| (case, lang, case.request(oracle))))
        .collect();
    let calls: Vec<_> = requests.iter().map(|(_, _, r)| r.call()).collect();
    let replies: Vec<GpuShaderReply> = Oracle::get()
        .batch(&calls, true)
        .into_iter()
        .map(|r| GpuShaderReply::from_response(r.expect("the oracle")))
        .collect();

    let glsl_aces2 = requests
        .iter()
        .zip(&replies)
        .find(|((case, lang, _), _)| {
            case.name == "aces 2 output transform" && *lang == GpuLanguage::Glsl4_0
        })
        .map(|(_, reply)| reply.shader().text.clone())
        .expect("the GLSL 4.0 ACES 2 shader");
    let aces2 = aces2_matrices(&glsl_aces2);
    assert!(aces2.len() >= 3, "the ACES 2 matrices: {aces2:?}");

    let mut checks_per_language = [0usize; 10];
    let mut checks_per_case = vec![0usize; cases.len()];
    for (((case, lang, _), reply), index) in requests.iter().zip(&replies).zip(0..) {
        let (case, lang) = (*case, *lang);
        let label = format!("{} {lang:?}", case.name);
        let mut checks = 0;
        let mut check_block = |block: &str| {
            assert_has_block(&label, &reply.shader().text, block);
            checks += 1;
        };

        if let Some(raised) = reply.raised() {
            if lang == GpuLanguage::Osl1 && case.name.starts_with("lut") {
                // The LUT writers refuse OSL before they call a helper (Lut1DOpGPU.cpp:150,
                // Lut3DOpGPU.cpp:22 @ v2.5.2).
                continue;
            }
            let dimensions = match case.name {
                "lut3d" => 3,
                "lut1d as 1D" => 1,
                "lut1d as 2D" => 2,
                _ => {
                    // OSL has no translation of the ACES 2 output transform; nothing to compare.
                    assert_eq!(
                        (case.name, lang),
                        ("aces 2 output transform", GpuLanguage::Osl1),
                        "{label}: {raised:?}"
                    );
                    continue;
                }
            };
            assert_text_eq(&label, &raised.message, &lut_error(lang, dimensions));
            checks_per_language[index % 10] += 1;
            checks_per_case[index / 10] += 1;
            continue;
        }

        let shader = reply.shader();
        let text = &shader.text;
        let getter = |key: &str| {
            shader.getters[key]
                .as_str()
                .unwrap_or_else(|| panic!("{key}"))
                .to_string()
        };
        let set = u32::try_from(
            shader.getters["descriptor_set_index"]
                .as_u64()
                .expect("descriptor_set_index"),
        )
        .unwrap();
        let pixel = getter("pixel_name");
        let prefix = getter("resource_prefix");
        let new = || GpuShaderText::new(lang);
        let mut line_checks = 0;
        let mut check_line = |line: String| {
            assert_has_line(&label, text, &line);
            line_checks += 1;
        };

        // Lut3DOpGPU.cpp:42-56 and 236-243 @ v2.5.2.
        for t in &shader.textures_3d {
            let binding = u32::try_from(t.binding_index).unwrap();
            assert_text_eq(
                &label,
                &t.sampler_name,
                &utf8(GpuShaderText::get_sampler_name(&t.name)),
            );
            let ss = new();
            ss.declare_tex3d(&t.name, set, binding).unwrap();
            check_block(&utf8(ss.string()));
            let coords = format!("{}_coords", t.name);
            let sample = utf8(new().sample_tex3d(&t.name, coords).unwrap());
            check_line(format!("{pixel}.rgb = {sample}.rgb;"));
        }
        // Lut1DOpGPU.cpp:196-230, 300-316 and 340-371 @ v2.5.2. The ACES 2 output transform
        // declares its tables the same way, and looks them up otherwise.
        for t in &shader.textures {
            let binding = u32::try_from(t.binding_index).unwrap();
            assert_text_eq(
                &label,
                &t.sampler_name,
                &utf8(GpuShaderText::get_sampler_name(&t.name)),
            );
            let ss = new();
            let name = &t.name;
            let one_d = t.dimensions == "TEXTURE_1D";
            if one_d {
                ss.declare_tex1d(name, set, binding).unwrap();
            } else {
                ss.declare_tex2d(name, set, binding).unwrap();
            }
            check_block(&utf8(ss.string()));
            if !case.name.starts_with("lut") {
                continue;
            }
            for c in ["r", "g", "b"] {
                let sample = if one_d {
                    new().sample_tex1d(name, format!("{name}_coords.{c}"))
                } else {
                    new().sample_tex2d(name, format!("{name}_computePos({pixel}.{c})"))
                };
                check_line(format!("{pixel}.{c} = {}.{c};", utf8(sample.unwrap())));
            }
        }

        let uniforms: Vec<&str> = shader.uniforms.iter().map(|u| u.name.as_str()).collect();
        match case.name {
            // ExposureContrastOpGPU.cpp:21-33 and 40-58 @ v2.5.2; OSL has no uniforms.
            "exposure contrast" | "exposure contrast, default prefix" => {
                let bases = ["exposureVal", "contrastVal", "gammaVal"];
                assert_eq!(
                    uniforms.len(),
                    if lang == GpuLanguage::Osl1 { 0 } else { 3 }
                );
                for (name, base) in uniforms.iter().zip(bases) {
                    let built = utf8(build_resource_name(&prefix, "exposure_contrast", base));
                    assert_text_eq(&label, name, &built);
                    let ss = new();
                    ss.declare_uniform_float(name);
                    check_block(&utf8(ss.string()));
                }
            }
            // GradingRGBCurveOpGPU.cpp:82-130, 142-158, 198-208 and 266-270 @ v2.5.2; the
            // float arrays hold MAX_NUM_KNOTS and MAX_NUM_COEFS values
            // (GradingBSplineCurve.h:103-105) and the int arrays 4 curves x 2
            // (GradingRGBCurveOpGPU.cpp:102). OSL has no uniforms.
            "rgb curve" | "rgb curve, default prefix" => {
                let bases = [
                    "knotsOffsets",
                    "knots",
                    "coefsOffsets",
                    "coefs",
                    "localBypass",
                ];
                assert_eq!(
                    uniforms.len(),
                    if lang == GpuLanguage::Osl1 { 0 } else { 5 }
                );
                for (name, base) in uniforms.iter().zip(bases) {
                    let built = utf8(build_resource_name(&prefix, "grading_rgbcurve", base));
                    assert_text_eq(&label, name, &built);
                    let ss = new();
                    match base {
                        "knotsOffsets" | "coefsOffsets" => ss.declare_uniform_array_int(name, 8),
                        "knots" => ss.declare_uniform_array_float(name, 120),
                        "coefs" => ss.declare_uniform_array_float(name, 360),
                        _ => {
                            ss.declare_uniform_bool(name);
                            check_line(format!("if (!{})", utf8(new().cast_to_bool(name))));
                        }
                    }
                    check_block(&utf8(ss.string()));
                }
            }
            // MatrixOpGPU.cpp:15-66 @ v2.5.2.
            "matrix" | "matrix, clamped in Cg" => {
                let (values, offsets) = if case.name == "matrix" {
                    (&MATRIX, &OFFSETS)
                } else {
                    (&MATRIX_CG, &OFFSETS_CG)
                };
                let ss = new();
                ss.indent();
                ss.new_line().put("");
                ss.new_line().put("// Add Matrix processing");
                ss.new_line().put("");
                ss.new_line().put("{");
                ss.indent();
                ss.new_line()
                    .put(ss.float4_decl("res").unwrap())
                    .put(" = ")
                    .put(ss.float4_const(
                        format!("{pixel}.rgb.r"),
                        format!("{pixel}.rgb.g"),
                        format!("{pixel}.rgb.b"),
                        format!("{pixel}.a"),
                    ))
                    .put(";");
                ss.new_line()
                    .put(ss.float4_decl("tmp").unwrap())
                    .put(" = res;");
                ss.new_line()
                    .put("res = ")
                    .put(ss.mat4f_mul_f64(values, "tmp").unwrap())
                    .put(";");
                let o = offsets.map(|o| o as f32);
                ss.new_line()
                    .put("res = ")
                    .put(ss.float4_const_f32(o[0], o[1], o[2], o[3]))
                    .put(" + res;");
                ss.new_line()
                    .put(&pixel)
                    .put(".rgb = ")
                    .put(ss.float3_const("res.x", "res.y", "res.z"))
                    .put(";");
                ss.new_line().put(&pixel).put(".a = res.w;");
                ss.dedent();
                ss.new_line().put("}");
                check_block(&utf8(ss.string()));
            }
            // GammaOpGPU.cpp:63-77 and 103-119 @ v2.5.2.
            "exponent, mirror" => {
                let ss = new();
                let decl = utf8(ss.float4_decl("signcol").unwrap());
                check_line(format!("{decl} = {};", utf8(ss.sign(&pixel))));
            }
            "exponent, pass through" => {
                let ss = new();
                let decl = utf8(ss.float4_decl("isAboveBreak").unwrap());
                let gt = utf8(ss.float4_greater_than(&pixel, "breakPnt"));
                check_line(format!("{decl} = {gt};"));
            }
            // LogOpGPU.cpp:295 @ v2.5.2 (the camera log, forward).
            "log camera" => {
                let ss = new();
                let decl = utf8(ss.float3_decl("isAboveBreak").unwrap());
                let gt = utf8(ss.float3_greater_than(format!("{pixel}.rgb"), "linear_break"));
                check_line(format!("{decl} = {gt};"));
            }
            // CDLOpGPU.cpp:63-65 @ v2.5.2.
            "cdl, no clamp" => {
                let rgb = format!("{pixel}.rgb");
                let lerp = utf8(new().lerp(&rgb, "pixPower", "posPix"));
                check_line(format!("{rgb} = {lerp};"));
            }
            // FixedFunctionOpGPU.cpp:35 @ v2.5.2.
            "red modifier" => {
                let ss = new();
                let decl = utf8(ss.float_decl("hue").unwrap());
                check_line(format!("{decl} = {};", utf8(ss.atan2("b", "a"))));
            }
            // FixedFunctionOpGPU.cpp:384, 389, 480 and 484 @ v2.5.2.
            "aces 2 output transform" => {
                for (lhs, m, vector) in &aces2 {
                    let ss = new();
                    let lhs = match lhs.split_once(' ') {
                        Some((_, name)) => utf8(ss.float3_decl(name).unwrap()),
                        None => lhs.clone(),
                    };
                    let product = utf8(ss.mat3f_mul_f32(m, vector).unwrap());
                    check_line(format!("{lhs} = {product};"));
                }
            }
            _ => {}
        }
        checks += line_checks;
        checks_per_language[index % 10] += checks;
        checks_per_case[index / 10] += checks;
    }

    for (lang, checks) in GpuLanguage::ALL.iter().zip(checks_per_language) {
        assert!(checks > 0, "{lang:?}: no check");
    }
    for (case, checks) in cases.iter().zip(checks_per_case) {
        assert!(checks > 0, "{}: no check", case.name);
    }
}

/// Probe values for literals: the specials (NaN only as the quiet NaN a config's `.nan`
/// reads as), whole numbers, the edges of the half range, and random bit patterns.
fn literal_probes() -> (Vec<f32>, Vec<f64>) {
    let mut floats: Vec<f32> = probe::specials()
        .into_iter()
        .filter(|v| !v.is_nan())
        .collect();
    floats.extend((-40i32..=40).map(|i| i as f32));
    floats.extend([
        65504.0,
        65505.0,
        65519.0,
        65520.0,
        65536.0,
        1e10,
        16777216.0,
        6.1e-5,
        6.2e-5,
        6.103515625e-5,
        5.9604645e-8,
        1e-30,
    ]);
    floats.extend(floats.clone().iter().map(|v| -v));
    floats.push(f32::NAN);
    let mut rng = Rng::new(0x11e7a15);
    floats.extend((0..1500).map(|_| rng.any_bits()).filter(|v| !v.is_nan()));

    let mut doubles: Vec<f64> = floats.iter().map(|&f| f64::from(f)).collect();
    doubles.extend([
        f64::MAX,
        f64::MIN,
        f64::MIN_POSITIVE,
        f64::from_bits(1),
        1.0 / 3.0,
        65504.000000000007,
        1e300,
    ]);
    doubles.extend(
        (0..1500)
            .map(|_| f64::from_bits(rng.next_u64()))
            .filter(|v| !v.is_nan()),
    );
    (floats, doubles)
}

/// A Matrix shader that carries literals: `values` as a 4x4 matrix (all 16 written as
/// doubles when it isn't diagonal), or as its diagonal (4 written as floats), and 4 offsets
/// (floats).
struct Carrier {
    matrix: [f64; 16],
    diagonal: bool,
    offsets: [f64; 4],
}

impl Carrier {
    /// The transform in the config's YAML syntax, which can hold NaN and infinities.
    fn yaml(&self) -> String {
        format!(
            "!<MatrixTransform> {{matrix: {}, offset: {}}}",
            yaml_list(&self.matrix),
            yaml_list(&self.offsets)
        )
    }

    /// The `res = ` lines the Matrix writer writes (MatrixOpGPU.cpp:35-61 @ v2.5.2).
    fn lines(&self, lang: GpuLanguage) -> Vec<String> {
        let ss = GpuShaderText::new(lang);
        let product = if self.diagonal {
            let d = [0, 5, 10, 15].map(|i| self.matrix[i] as f32);
            format!(
                "{} * res",
                utf8(ss.float4_const_f32(d[0], d[1], d[2], d[3]))
            )
        } else {
            utf8(ss.mat4f_mul_f64(&self.matrix, "tmp").unwrap())
        };
        let o = self.offsets.map(|o| o as f32);
        vec![
            format!("res = {product};"),
            format!(
                "res = {} + res;",
                utf8(ss.float4_const_f32(o[0], o[1], o[2], o[3]))
            ),
        ]
    }
}

const RAW_CONFIG: &str = "ocio_profile_version: 2.1
roles:
  default: raw
file_rules:
  - !<Rule> {name: Default, colorspace: raw}
displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
colorspaces:
  - !<ColorSpace>
    name: raw
";

/// Literals are what the wheel writes, in every language: the C runtime's `%.9g` (floats) or
/// `%.17g` (doubles) digits, a `.` after finite whole numbers, the C runtime's infinities
/// and NaN, and in Cg the value clamped to the half range first (`getFloatString`,
/// GpuShaderUtils.cpp:21-35 @ v2.5.2). Matrix shaders carry them: a non-diagonal matrix's
/// 16 values as doubles, a diagonal's 4 and the offsets as floats. Cg and GLSL 4.0 take every
/// probe; the other languages, which write literals as GLSL does, the first carriers, which
/// hold the specials.
#[test]
fn literals_are_the_wheels() {
    let (floats, doubles) = literal_probes();
    let float_count = floats.len();
    let mut floats = floats.into_iter().cycle();
    let mut carriers = Vec::new();
    for chunk in doubles.chunks(16) {
        let mut matrix: [f64; 16] = std::array::from_fn(|i| chunk[i % chunk.len()]);
        if [1, 2, 3, 4, 6, 7, 8, 9, 11, 12, 13, 14]
            .iter()
            .all(|&i| matrix[i] == 0.0)
        {
            matrix[1] = 0.5;
        }
        carriers.push(Carrier {
            matrix,
            diagonal: false,
            offsets: std::array::from_fn(|_| f64::from(floats.next().unwrap())),
        });
    }
    let non_diagonal = carriers.len();
    for _ in 0..float_count.div_ceil(8) {
        let mut matrix = [0.0; 16];
        for i in [0, 5, 10, 15] {
            matrix[i] = f64::from(floats.next().unwrap());
        }
        carriers.push(Carrier {
            matrix,
            diagonal: true,
            offsets: std::array::from_fn(|_| f64::from(floats.next().unwrap())),
        });
    }

    let mut requests = Vec::new();
    for (lang, oracle) in languages() {
        let all = matches!(lang, GpuLanguage::Cg | GpuLanguage::Glsl4_0);
        for (i, carrier) in carriers.iter().enumerate() {
            let first = if carrier.diagonal {
                i - non_diagonal < 8
            } else {
                i < 16
            };
            if !all && !first {
                continue;
            }
            let config = format!(
                "{RAW_CONFIG}  - !<ColorSpace>\n    name: cs\n    from_scene_reference: {}\n",
                carrier.yaml()
            );
            requests.push((
                lang,
                i,
                GpuShaderRequest::new(
                    json!({"config": {"yaml": config}, "src": "raw", "dst": "cs",
                           "optimization": "OPTIMIZATION_NONE"}),
                    ShaderSettings::language(oracle),
                ),
            ));
        }
    }
    let calls: Vec<_> = requests.iter().map(|(_, _, r)| r.call()).collect();
    let mut compared = 0;
    for ((lang, i, _), response) in requests.iter().zip(Oracle::get().batch(&calls, true)) {
        let reply = GpuShaderReply::from_response(response.expect("the oracle"));
        let label = format!("carrier {i} {lang:?}");
        let wheel: Vec<&str> = reply
            .shader()
            .text
            .split('\n')
            .map(str::trim_start)
            .filter(|l| l.starts_with("res = "))
            .collect();
        let port = carriers[*i].lines(*lang);
        assert_eq!(wheel.len(), port.len(), "{label}: {wheel:?}");
        for (w, p) in wheel.iter().zip(&port) {
            assert_text_eq(&label, w, p);
        }
        compared += 1;
    }
    assert!(compared > 2 * carriers.len(), "{compared} shaders compared");
}
