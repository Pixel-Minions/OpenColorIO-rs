// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Matrix op's GPU writer and the GPU processor against the wheel (`gpu_shader`), in the
//! 10 languages: for each case, the GPU processor's cache ID and queries, and the shader the
//! extraction writes (its text, cache ID and names), or the error, byte for byte.
//!
//! The port builds the processor's ops as the wheel does: a `GroupTransform` builds each child
//! forward, `BuildMatrixOp` validates the transform's data and makes the op in the transform's
//! direction (src/OpenColorIO/ops/matrix/MatrixOp.cpp:395-404 @ v2.5.2), and the processor
//! finalizes them (src/OpenColorIO/Processor.cpp:618-641). The GPU processor is then
//! `getOptimizedGPUProcessor` at each level, or `getDefaultGPUProcessor`
//! (Processor.cpp:437-445, 491-523).

use ocio_gpu::gpu_processor::GpuProcessor;
use ocio_gpu::{GpuLanguage, GpuShaderDesc};
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{OptimizationFlags, TransformDirection};
use ocio_ops::ops::matrix::MatrixOpData;
use ocio_ops::ops::matrix::matrix_op::create_matrix_op;
use ocio_testkit::battery::yaml_list;
use ocio_testkit::gpu::{self as oracle_gpu, GpuShaderReply, GpuShaderRequest, ShaderSettings};
use ocio_testkit::gpu_cases::matrix_cases;
use ocio_testkit::{Oracle, assert_text_eq};
use serde_json::{Value, json};

/// A matrix and its offsets.
type Matrix = ([f64; 16], [f64; 4]);

/// A diagonal matrix.
fn diagonal(d: [f64; 4]) -> [f64; 16] {
    let mut m = [0.0; 16];
    for (i, v) in d.into_iter().enumerate() {
        m[i * 5] = v;
    }
    m
}

const IDENTITY: [f64; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

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

/// The shader description's names; `None` keeps the default.
#[derive(Debug, Clone, Copy, Default)]
struct Names {
    pixel: Option<&'static str>,
    prefix: Option<&'static str>,
    function: Option<&'static str>,
}

/// One extraction: a list of Matrix transforms, an optimization level, a language and names.
#[derive(Debug, Clone)]
struct Case {
    label: String,
    chain: Vec<(Matrix, TransformDirection)>,
    /// The processor, as the oracle takes it.
    processor: Value,
    flags: (Option<Value>, OptimizationFlags),
    language: GpuLanguage,
    oracle_language: oracle_gpu::GpuLanguage,
    names: Names,
}

impl Case {
    fn request(&self) -> GpuShaderRequest {
        let mut processor = self.processor.clone();
        if let Some(flags) = &self.flags.0 {
            processor["optimization"] = flags.clone();
        }
        GpuShaderRequest::new(
            processor,
            ShaderSettings {
                language: Some(self.oracle_language),
                pixel_name: self.names.pixel.map(String::from),
                resource_prefix: self.names.prefix.map(String::from),
                function_name: self.names.function.map(String::from),
                ..ShaderSettings::default()
            },
        )
    }
}

fn direction_name(dir: TransformDirection, yaml: bool) -> &'static str {
    match (dir, yaml) {
        (TransformDirection::Forward, false) => "TRANSFORM_DIR_FORWARD",
        (TransformDirection::Inverse, false) => "TRANSFORM_DIR_INVERSE",
        (TransformDirection::Forward, true) => "forward",
        (TransformDirection::Inverse, true) => "inverse",
    }
}

/// The oracle's processor for `chain`: a `GroupTransform` of `MatrixTransform`s, as JSON, or,
/// when a value isn't finite, in a config's YAML (JSON can't hold NaN and infinities).
fn processor(chain: &[(Matrix, TransformDirection)]) -> Value {
    let finite = chain
        .iter()
        .all(|((m, o), _)| m.iter().chain(o).all(|v| v.is_finite()));
    if finite {
        let children: Vec<Value> = chain
            .iter()
            .map(|((m, o), dir)| {
                json!({"class": "MatrixTransform", "args": {"matrix": m.to_vec(),
                    "offset": o.to_vec(), "direction": {"enum": direction_name(*dir, false)}}})
            })
            .collect();
        return json!({"transform": {"class": "GroupTransform", "children": children}});
    }
    let children: Vec<String> = chain
        .iter()
        .map(|((m, o), dir)| {
            format!(
                "!<MatrixTransform> {{matrix: {}, offset: {}, direction: {}}}",
                yaml_list(m),
                yaml_list(o),
                direction_name(*dir, true)
            )
        })
        .collect();
    let config = format!(
        "{RAW_CONFIG}  - !<ColorSpace>\n    name: cs\n    from_scene_reference: \
         !<GroupTransform> {{children: [{}]}}\n",
        children.join(", ")
    );
    json!({"config": {"yaml": config}, "src": "raw", "dst": "cs"})
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

/// The processor's ops, as `BuildMatrixOp` makes them (see the module notes), before the
/// processor finalizes them.
fn raw_ops(chain: &[(Matrix, TransformDirection)]) -> ocio_ops::Result<OpVec> {
    let mut raw = OpVec::new();
    for ((m, o), dir) in chain {
        let mut data = MatrixOpData::new();
        data.set_rgba(m);
        data.set_rgba_offsets(o);
        data.set_direction(*dir);
        data.validate()?;
        create_matrix_op(&mut raw, data, TransformDirection::Forward);
    }
    Ok(raw)
}

/// What the wheel and the port give for a case, to compare as a whole.
#[derive(Debug, Clone, PartialEq)]
enum Outcome {
    Raised(String),
    Extracted {
        gpu_cache_id: String,
        is_no_op: bool,
        has_channel_crosstalk: bool,
        /// The shader's text, cache ID and names, or the extraction's error.
        shader: Result<[String; 5], String>,
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
                    && shader.textures_3d.is_empty(),
                "a Matrix shader has no uniform nor texture"
            );
            let getter = |key: &str| shader.getters[key].as_str().unwrap().to_string();
            Ok([
                shader.text.clone(),
                shader.cache_id.clone(),
                getter("pixel_name"),
                getter("resource_prefix"),
                getter("function_name"),
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

/// The port's outcome, from the raw ops as `BuildMatrixOp` makes them, or, when
/// `finalize_first`, from ops the processor finalized first, as the wheel's processor does
/// (Processor.cpp:618-641): `GpuProcessor::new` takes either, and finalizes them itself.
///
/// An error carries the wheel's stage: the data's validation is the transform's on the JSON
/// route (the binding builds the transform; its message would start "MatrixTransform
/// validation failed: ", which the cases don't reach) and the processor's on the YAML route.
fn port(case: &Case, finalize_first: bool) -> Outcome {
    let text = |b: &[u8]| String::from_utf8(b.to_vec()).unwrap();
    let validation_stage = if case.processor.get("transform").is_some() {
        "transform"
    } else {
        "processor"
    };
    let mut raw = match raw_ops(&case.chain) {
        Ok(raw) => raw,
        Err(e) => return Outcome::Raised(format!("{validation_stage}: {}", e.message())),
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
    if let Some(f) = case.names.function {
        desc.set_function_name(f);
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
                text(desc.function_name()),
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
    for (case, response) in cases.iter().zip(Oracle::get().batch(&calls, true)) {
        let reply = GpuShaderReply::from_response(
            response.unwrap_or_else(|e| panic!("{}: the oracle failed: {e}", case.label)),
        );
        let wheel = wheel(&reply);
        // The raw ops, and ops finalized first: both must give the wheel's outcome.
        for finalize_first in [false, true] {
            let port = port(case, finalize_first);
            if wheel != port {
                failures.push((case, wheel, port));
                break;
            }
        }
    }
    if let Some((case, wheel, port)) = failures.first() {
        // The first difference in full, the texts compared line by line.
        if let (
            Outcome::Extracted { shader: Ok(w), .. },
            Outcome::Extracted { shader: Ok(p), .. },
        ) = (&wheel, &port)
        {
            assert_text_eq(&case.label, &w[0], &p[0]);
        }
        let labels: Vec<&str> = failures.iter().map(|(c, _, _)| c.label.as_str()).collect();
        let (wheel, port) = (wheel.clone(), port.clone());
        panic!(
            "{} of {} cases differ; the first, {}:\n  wheel {wheel:?}\n  port  {port:?}\nall: {labels:#?}",
            failures.len(),
            cases.len(),
            case.label
        );
    }
}

/// Every language, with the oracle's name for it.
fn languages() -> impl Iterator<Item = (GpuLanguage, oracle_gpu::GpuLanguage)> {
    GpuLanguage::ALL
        .into_iter()
        .zip(oracle_gpu::GpuLanguage::ALL)
}

/// `chain` at the levels `flags`, in every language, with `names`.
fn cases_of(
    label: &str,
    chain: Vec<(Matrix, TransformDirection)>,
    flags: &[(Option<Value>, OptimizationFlags)],
    names: Names,
) -> Vec<Case> {
    let processor = processor(&chain);
    let mut cases = Vec::new();
    for flags in flags {
        for (language, oracle_language) in languages() {
            cases.push(Case {
                label: format!("{label} {:?} {language:?} {names:?}", flags.0),
                chain: chain.clone(),
                processor: processor.clone(),
                flags: flags.clone(),
                language,
                oracle_language,
                names,
            });
        }
    }
    cases
}

/// Upstream's `MatrixOps` GPU tests (tests/gpu/MatrixOp_test.cpp @ v2.5.2,
/// `ocio_testkit::gpu_cases`): the wheel's shader for each, from `getDefaultGPUProcessor` as
/// the generic-shader tests take it and from every optimization level, in every language, is
/// the port's. (The legacy GPU processor, which 8 of them render with, comes in Phase 7.)
#[test]
fn upstreams_gpu_tests_write_the_wheels_shaders() {
    let mut cases = Vec::new();
    for upstream in matrix_cases() {
        let dir = if upstream.inverse {
            TransformDirection::Inverse
        } else {
            TransformDirection::Forward
        };
        let matrix = (
            upstream.matrix.unwrap_or(IDENTITY),
            upstream.offset.unwrap_or([0.0; 4]),
        );
        let processor = json!({ "transform": upstream.transform() });
        for flags in &levels() {
            for (language, oracle_language) in languages() {
                cases.push(Case {
                    label: format!("{} {:?} {language:?}", upstream.name, flags.0),
                    chain: vec![(matrix, dir)],
                    processor: processor.clone(),
                    flags: flags.clone(),
                    language,
                    oracle_language,
                    names: Names::default(),
                });
            }
        }
    }
    check(&cases);
}

/// Each path of the writer, in every language and at every level: an identity (no product,
/// or nothing when the optimizer removes it), diagonals (floats: thirds and tenths round),
/// one that changes alpha only, a full matrix (doubles), offsets alone and on alpha alone, a
/// matrix with offsets, values beyond Cg's half range and below its smallest normal, lists
/// that combine, cancel, or stay within the identity tolerances, and names (an empty pixel
/// name is an error, but in OSL).
#[test]
fn every_path_writes_the_wheels_shader() {
    use TransformDirection::{Forward, Inverse};
    let full = [
        1.1, 0.2, 0.3, 0.4, 0.5, 1.6, 0.7, 0.8, 0.2, 0.1, 1.1, 0.2, 0.3, 0.4, 0.5, 1.6,
    ];
    let thirds: Matrix = (diagonal([1.0 / 3.0, 0.1, 2.0, 1.0]), [0.0; 4]);
    let alpha: Matrix = (diagonal([1.0, 1.0, 1.0, 0.5]), [0.0; 4]);
    let offsets: Matrix = (IDENTITY, [0.1, -0.2, 1.0 / 3.0, 0.0]);
    let alpha_offset: Matrix = (IDENTITY, [0.0, 0.0, 0.0, 0.25]);
    let both: Matrix = (full, [-0.5, -0.25, 0.25, 0.1]);
    let half_range: Matrix = (
        [
            70000.0, 1e-9, 0.0, 0.0, -65520.0, 1.0, 6.0e-5, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0,
            1.0,
        ],
        [123456.0, -1e-7, 6.1e-5, -65505.0],
    );
    let scale: Matrix = (diagonal([2.0, 0.5, 4.0, 1.0]), [0.0; 4]);
    let near_alpha: Matrix = (diagonal([1.0, 1.0, 1.0, 1.0 + 5e-7]), [0.0; 4]);
    let past_alpha: Matrix = (diagonal([1.0, 1.0, 1.0, 1.0 + 2e-6]), [0.0; 4]);
    let chains: Vec<(&str, Vec<(Matrix, TransformDirection)>)> = vec![
        ("identity", vec![((IDENTITY, [0.0; 4]), Forward)]),
        ("thirds", vec![(thirds, Forward)]),
        ("thirds inverse", vec![(thirds, Inverse)]),
        ("alpha", vec![(alpha, Forward)]),
        ("full", vec![((full, [0.0; 4]), Forward)]),
        ("offsets", vec![(offsets, Forward)]),
        ("alpha offset", vec![(alpha_offset, Forward)]),
        ("both", vec![(both, Forward)]),
        ("both inverse", vec![(both, Inverse)]),
        ("half range", vec![(half_range, Forward)]),
        ("near alpha", vec![(near_alpha, Forward)]),
        ("past alpha", vec![(past_alpha, Forward)]),
        (
            "combined",
            vec![(scale, Forward), (offsets, Forward), (both, Inverse)],
        ),
        ("cancelled", vec![(offsets, Forward), (offsets, Inverse)]),
        (
            "around a full matrix",
            vec![(both, Forward), (scale, Forward), (both, Inverse)],
        ),
    ];
    let mut cases = Vec::new();
    for (label, chain) in chains {
        cases.extend(cases_of(label, chain, &levels(), Names::default()));
    }
    let names = [
        Names {
            pixel: Some("px"),
            prefix: Some("p__q"),
            function: Some("F__1"),
        },
        Names {
            pixel: Some(""),
            ..Names::default()
        },
    ];
    for names in names {
        cases.extend(cases_of(
            "both",
            vec![(both, Forward)],
            &levels()[1..2],
            names,
        ));
    }
    // Flags above bit 31, which only Linux's 64-bit flags hold: the GPU processor's cache ID
    // prints them whole (docs/improvements.md, I-45).
    #[cfg(not(windows))]
    {
        let high = (1 << 32) | OptimizationFlags::DEFAULT.0;
        cases.extend(cases_of(
            "both, flags above bit 31",
            vec![(both, Forward)],
            &[(Some(json!(high)), OptimizationFlags(high))],
            Names::default(),
        ));
    }
    check(&cases);
}

/// Extreme parameters, generated: each of a full matrix's and a diagonal's 16 values and 4
/// offsets in turn NaN, ±Inf, the largest double, -0, the smallest denormal, and values Cg
/// clamps, without optimization, in every language.
#[test]
fn extreme_parameters_write_the_wheels_shader() {
    let full = [
        1.1, 0.2, 0.3, 0.4, 0.5, 1.6, 0.7, 0.8, 0.2, 0.1, 1.1, 0.2, 0.3, 0.4, 0.5, 1.6,
    ];
    let bases: [(&str, Matrix); 2] = [
        ("full", (full, [0.1, 0.2, 0.3, 0.0])),
        (
            "diagonal",
            (diagonal([2.0, 0.5, 4.0, 1.0]), [0.1, 0.2, 0.3, 0.0]),
        ),
    ];
    let values = [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::MAX,
        -0.0,
        f64::from_bits(1),
        1e-9,
        65504.5,
    ];
    let mut cases = Vec::new();
    for (base_label, base) in bases {
        for slot in 0..20 {
            for v in values {
                let mut matrix = base;
                if slot < 16 {
                    matrix.0[slot] = v;
                } else {
                    matrix.1[slot - 16] = v;
                }
                cases.extend(cases_of(
                    &format!("{base_label} slot {slot} = {v:e}"),
                    vec![(matrix, TransformDirection::Forward)],
                    &levels()[1..2],
                    Names::default(),
                ));
            }
        }
    }
    check(&cases);
}
