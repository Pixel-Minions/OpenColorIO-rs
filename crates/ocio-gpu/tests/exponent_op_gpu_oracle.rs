// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Exponent op's GPU writer against the wheel (`gpu_shader`), in the 10 languages: for
//! each case, the GPU processor's cache ID and queries, and the shader the extraction writes
//! (its text, cache ID and names), or the error, byte for byte.
//!
//! Only a version 1 config builds an Exponent op (`BuildExponentOp`,
//! src/OpenColorIO/ops/gamma/GammaOp.cpp:190-215 @ v2.5.2), so each case is a `GroupTransform`
//! of ExponentTransforms (and a MatrixTransform) as the `from_reference` of a colour space in a
//! version 1 config, and the processor goes from the raw colour space to it. The port builds
//! the same ops (`CreateExponentOp` in each transform's direction, `BuildMatrixOp` for the
//! matrix), and runs the GPU processor from them as they are, and from them finalized first
//! as the wheel's processor does (src/OpenColorIO/Processor.cpp:618-641): both must give the
//! wheel's outcome.

use ocio_gpu::gpu_processor::GpuProcessor;
use ocio_gpu::{GpuLanguage, GpuShaderDesc};
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{OptimizationFlags, TransformDirection};
use ocio_ops::ops::exponent::ExponentOpData;
use ocio_ops::ops::exponent::exponent_op::create_exponent_op;
use ocio_ops::ops::matrix::MatrixOpData;
use ocio_ops::ops::matrix::matrix_op::create_matrix_op;
use ocio_testkit::battery::yaml_list;
use ocio_testkit::gpu::{self as oracle_gpu, GpuShaderReply, GpuShaderRequest, ShaderSettings};
use ocio_testkit::{Oracle, assert_text_eq};
use serde_json::{Value, json};

/// One transform of a case's list.
#[derive(Debug, Clone, Copy)]
enum Item {
    /// An ExponentTransform: its values, inverse or not.
    Exponent([f64; 4], bool),
    /// A MatrixTransform scaling R, G and B.
    Scale(f64),
}

/// The optimization levels, as the oracle takes them; `None` is `getDefaultGPUProcessor`.
fn levels() -> Vec<(Option<Value>, OptimizationFlags)> {
    let name = |n: &str| Some(json!(n));
    vec![
        (None, OptimizationFlags::DEFAULT),
        (name("OPTIMIZATION_NONE"), OptimizationFlags::NONE),
        (name("OPTIMIZATION_LOSSLESS"), OptimizationFlags::LOSSLESS),
        (name("OPTIMIZATION_DEFAULT"), OptimizationFlags::DEFAULT),
        (name("OPTIMIZATION_ALL"), OptimizationFlags::ALL),
    ]
}

/// A version 1 config with one colour space, `raw`, the processor's source.
const RAW_CONFIG_V1: &str = "ocio_profile_version: 1
roles:
  default: raw
displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}
colorspaces:
  - !<ColorSpace>
    name: raw
";

/// The shader description's names; `None` keeps the default.
#[derive(Debug, Clone, Copy, Default)]
struct Names {
    pixel: Option<&'static str>,
    prefix: Option<&'static str>,
}

/// One extraction.
#[derive(Debug, Clone)]
struct Case {
    label: String,
    items: Vec<Item>,
    flags: (Option<Value>, OptimizationFlags),
    language: GpuLanguage,
    oracle_language: oracle_gpu::GpuLanguage,
    names: Names,
}

impl Case {
    fn request(&self) -> GpuShaderRequest {
        let children: Vec<String> = self
            .items
            .iter()
            .map(|item| match item {
                Item::Exponent(v, inverse) => format!(
                    "!<ExponentTransform> {{value: {}, direction: {}}}",
                    yaml_list(v),
                    if *inverse { "inverse" } else { "forward" }
                ),
                Item::Scale(s) => {
                    format!("!<MatrixTransform> {{matrix: {}}}", yaml_list(&scale(*s)))
                }
            })
            .collect();
        let config = format!(
            "{RAW_CONFIG_V1}  - !<ColorSpace>\n    name: cs\n    from_reference: \
             !<GroupTransform> {{children: [{}]}}\n",
            children.join(", ")
        );
        let mut processor = json!({"config": {"yaml": config}, "src": "raw", "dst": "cs"});
        if let Some(flags) = &self.flags.0 {
            processor["optimization"] = flags.clone();
        }
        GpuShaderRequest::new(
            processor,
            ShaderSettings {
                language: Some(self.oracle_language),
                pixel_name: self.names.pixel.map(String::from),
                resource_prefix: self.names.prefix.map(String::from),
                ..ShaderSettings::default()
            },
        )
    }
}

/// A matrix scaling R, G and B by `s`.
fn scale(s: f64) -> [f64; 16] {
    [
        s, 0.0, 0.0, 0.0, 0.0, s, 0.0, 0.0, 0.0, 0.0, s, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

/// What the wheel and the port give for a case.
#[derive(Debug, Clone, PartialEq)]
enum Outcome {
    Raised(String),
    Extracted {
        gpu_cache_id: String,
        is_no_op: bool,
        has_channel_crosstalk: bool,
        /// The shader's text, cache ID, pixel name and resource prefix, or the error.
        shader: Result<[String; 4], String>,
    },
}

fn wheel(reply: &GpuShaderReply) -> Outcome {
    let result = &reply.result;
    if let Some(raised) = reply.raised()
        && raised.stage != "extract"
    {
        return Outcome::Raised(format!("{}: {}", raised.stage, raised.message));
    }
    let shader = match reply.raised() {
        Some(raised) => Err(raised.message),
        None => {
            let shader = reply.shader();
            assert!(
                shader.uniforms.is_empty()
                    && shader.textures.is_empty()
                    && shader.textures_3d.is_empty()
            );
            let getter = |key: &str| shader.getters[key].as_str().unwrap().to_string();
            Ok([
                shader.text.clone(),
                shader.cache_id.clone(),
                getter("pixel_name"),
                getter("resource_prefix"),
            ])
        }
    };
    Outcome::Extracted {
        gpu_cache_id: result["gpu_cache_id"].as_str().unwrap().to_string(),
        is_no_op: result["gpu_processor"]["isNoOp"].as_bool().unwrap(),
        has_channel_crosstalk: result["gpu_processor"]["hasChannelCrosstalk"]
            .as_bool()
            .unwrap(),
        shader,
    }
}

/// The processor's ops, as `BuildExponentOp` and `BuildMatrixOp` make them, before the
/// processor finalizes them. A refusal is the processor's (the YAML route).
fn raw_ops(items: &[Item]) -> ocio_ops::Result<OpVec> {
    let mut raw = OpVec::new();
    for item in items {
        match item {
            Item::Exponent(v, inverse) => {
                let dir = if *inverse {
                    TransformDirection::Inverse
                } else {
                    TransformDirection::Forward
                };
                create_exponent_op(&mut raw, ExponentOpData::from_values(v), dir)?;
            }
            Item::Scale(s) => {
                let mut data = MatrixOpData::new();
                data.set_rgba(&scale(*s));
                data.validate()?;
                create_matrix_op(&mut raw, data, TransformDirection::Forward);
            }
        }
    }
    Ok(raw)
}

fn port(case: &Case, finalize_first: bool) -> Outcome {
    let text = |b: &[u8]| String::from_utf8(b.to_vec()).unwrap();
    let mut raw = match raw_ops(&case.items) {
        Ok(raw) => raw,
        Err(e) => return Outcome::Raised(format!("processor: {}", e.message())),
    };
    if finalize_first && let Err(e) = raw.finalize() {
        return Outcome::Raised(format!("processor: {}", e.message()));
    }
    let gpu = match GpuProcessor::new(&raw, case.flags.1) {
        Ok(gpu) => gpu,
        Err(e) => return Outcome::Raised(format!("gpu_processor: {}", e.message())),
    };
    let mut desc = GpuShaderDesc::new(case.language);
    if let Some(p) = case.names.pixel {
        desc.set_pixel_name(p);
    }
    if let Some(p) = case.names.prefix {
        desc.set_resource_prefix(p);
    }
    let shader = gpu
        .extract_gpu_shader_info(&mut desc)
        .map(|()| {
            assert!(desc.num_uniforms() == 0 && desc.num_textures() + desc.num_textures_3d() == 0);
            [
                text(desc.shader_text()),
                text(&desc.cache_id()),
                text(desc.pixel_name()),
                text(desc.resource_prefix()),
            ]
        })
        .map_err(|e| e.message().to_string());
    Outcome::Extracted {
        gpu_cache_id: text(gpu.get_cache_id()),
        is_no_op: gpu.is_no_op(),
        has_channel_crosstalk: gpu.has_channel_crosstalk(),
        shader,
    }
}

/// Runs every case through the wheel and the port, and fails with every case that differs.
fn check(cases: &[Case]) {
    let requests: Vec<GpuShaderRequest> = cases.iter().map(Case::request).collect();
    let calls: Vec<_> = requests.iter().map(GpuShaderRequest::call).collect();
    let mut failures: Vec<(&Case, Outcome, Outcome)> = Vec::new();
    let mut extracted = 0;
    for (case, response) in cases.iter().zip(Oracle::get().batch(&calls, true)) {
        let reply = GpuShaderReply::from_response(
            response.unwrap_or_else(|e| panic!("{}: the oracle failed: {e}", case.label)),
        );
        let wheel = wheel(&reply);
        if matches!(wheel, Outcome::Extracted { shader: Ok(_), .. }) {
            extracted += 1;
        }
        for finalize_first in [false, true] {
            let port = port(case, finalize_first);
            if wheel != port {
                failures.push((case, wheel.clone(), port));
                break;
            }
        }
    }
    if let Some((case, wheel, port)) = failures.first() {
        if let (
            Outcome::Extracted { shader: Ok(w), .. },
            Outcome::Extracted { shader: Ok(p), .. },
        ) = (wheel, port)
        {
            assert_text_eq(&case.label, &w[0], &p[0]);
        }
        let labels: Vec<&str> = failures.iter().map(|(c, _, _)| c.label.as_str()).collect();
        panic!(
            "{} of {} cases differ; the first, {}:\n  wheel {wheel:?}\n  port  {port:?}\nall: {labels:#?}",
            failures.len(),
            cases.len(),
            case.label
        );
    }
    assert!(extracted > 0, "no case extracted a shader");
}

/// `items` at the levels `flags`, in every language, with `names`.
fn cases_of(
    label: &str,
    items: Vec<Item>,
    flags: &[(Option<Value>, OptimizationFlags)],
    names: Names,
) -> Vec<Case> {
    let mut cases = Vec::new();
    for flags in flags {
        for (language, oracle_language) in GpuLanguage::ALL
            .into_iter()
            .zip(oracle_gpu::GpuLanguage::ALL)
        {
            cases.push(Case {
                label: format!("{label} {:?} {language:?} {names:?}", flags.0),
                items: items.clone(),
                flags: flags.clone(),
                language,
                oracle_language,
                names,
            });
        }
    }
    cases
}

/// Upstream's values (tests/cpu/ops/exponent/ExponentOp_tests.cpp @ v2.5.2), forward and
/// inverse, and lists the optimizer combines, cancels or leaves around a matrix, at every
/// level, in every language; other names, and an empty pixel name (an error, but in OSL).
#[test]
fn exponent_shaders_match_the_wheel() {
    use Item::{Exponent, Scale};
    let a = [1.037289, 1.019015, 0.966082, 1.0];
    let b = [2.0, 2.1, 3.0, 3.1];
    let lists: Vec<(&str, Vec<Item>)> = vec![
        ("value", vec![Exponent([1.2, 1.3, 1.4, 1.5], false)]),
        ("value inverse", vec![Exponent([1.2, 1.3, 1.4, 1.5], true)]),
        ("value_limits", vec![Exponent([0.0, 2.0, -2.0, 1.5], false)]),
        ("zero inverse", vec![Exponent([0.0, 1.3, 1.4, 1.5], true)]),
        ("identity", vec![Exponent([1.0; 4], false)]),
        ("combined", vec![Exponent(a, false), Exponent(b, false)]),
        ("cancelled", vec![Exponent(a, false), Exponent(a, true)]),
        ("three", vec![Exponent(a, false); 3]),
        (
            "around a matrix",
            vec![Exponent(a, false), Scale(2.0), Exponent(a, true)],
        ),
    ];
    let mut cases = Vec::new();
    for (label, items) in lists {
        cases.extend(cases_of(label, items, &levels(), Names::default()));
    }
    for names in [
        Names {
            pixel: Some("px"),
            prefix: Some("p__q"),
        },
        Names {
            pixel: Some(""),
            prefix: None,
        },
    ] {
        cases.extend(cases_of(
            "value",
            vec![Exponent([1.2, 1.3, 1.4, 1.5], false)],
            &levels()[1..2],
            names,
        ));
    }
    check(&cases);
}

/// Extreme exponents, generated: each of the four in turn NaN, ±Inf, the largest double, -0,
/// the smallest denormal, 1e-9, a value beyond the float range and one Cg clamps, forward and
/// inverse, without optimization, in every language.
#[test]
fn extreme_exponents_match_the_wheel() {
    let values = [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::MAX,
        -0.0,
        f64::from_bits(1),
        1e-9,
        1e39,
        65504.5,
    ];
    let mut cases = Vec::new();
    for slot in 0..4 {
        for v in values {
            for inverse in [false, true] {
                let mut e = [1.2, 1.3, 1.4, 1.5];
                e[slot] = v;
                cases.extend(cases_of(
                    &format!("value[{slot}] = {v:e}, inverse {inverse}"),
                    vec![Item::Exponent(e, inverse)],
                    &levels()[1..2],
                    Names::default(),
                ));
            }
        }
    }
    check(&cases);
}
