// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Gamma op's GPU writer against the wheel (`gpu_shader`), in the 10 languages: for each
//! case, the GPU processor's cache ID and queries, and the shader the extraction writes (its
//! text, cache ID and names), or the error, byte for byte.
//!
//! In a version 2 config, an `ExponentTransform` and an `ExponentWithLinearTransform` each
//! build a Gamma op with a copy of their data (`BuildExponentOp`, `BuildExponentWithLinearOp`,
//! src/OpenColorIO/ops/gamma/GammaOp.cpp:179-216 @ v2.5.2): the negative styles and the two
//! directions give the 10 Gamma styles. A `GroupTransform` builds each child forward, and the
//! processor finalizes the ops (src/OpenColorIO/Processor.cpp:618-641); the GPU processor is
//! then `getOptimizedGPUProcessor` at each level, or `getDefaultGPUProcessor`
//! (Processor.cpp:437-445, 491-523). The port builds the same data (the helpers of
//! `crates/ocio-ops/tests/common/gamma.rs`), the ops with `create_gamma_op`, and runs the GPU
//! processor from them as they are, and from them finalized first: both must give the
//! wheel's outcome.
//!
//! Finite parameters go through JSON (the binding's constructor validates each child, with
//! its transform's prefix), NaN and infinite ones through a config's YAML (`BuildExponentOp`
//! validates, without a prefix).
//!
//! An identity of a basic style that clamps, and an inverse pair of them, become a Range op
//! where the optimizer replaces identities (`GammaOpData::getIdentityReplacement`); the
//! Range op's GPU writer (1.3r3) isn't ported yet, so those lists run without optimization.

use ocio_gpu::gpu_processor::GpuProcessor;
use ocio_gpu::{GpuLanguage, GpuShaderDesc};
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{
    NegativeStyle, OptimizationFlags, TransformDirection, get_inverse_transform_direction,
};
use ocio_ops::ops::gamma::GammaOpData;
use ocio_ops::ops::gamma::gamma_op::create_gamma_op;
use ocio_ops::ops::matrix::MatrixOpData;
use ocio_ops::ops::matrix::matrix_op::create_matrix_op;
use ocio_testkit::battery::yaml_list;
use ocio_testkit::gpu::{self as oracle_gpu, GpuShaderReply, GpuShaderRequest, ShaderSettings};
use ocio_testkit::gpu_cases::{GammaTransform, gamma_cases};
use ocio_testkit::{Oracle, assert_text_eq};
use serde_json::{Value, json};

#[allow(dead_code)] // The GPU tests use a subset.
#[path = "../../ocio-ops/tests/common/gamma.rs"]
mod gamma;

use NegativeStyle::{Clamp, Linear, Mirror, PassThru};
use TransformDirection::{Forward as F, Inverse as I};
use gamma::{
    direction_enum, exponent_op, exponent_with_linear_op, negative_style_enum, yaml_direction,
    yaml_style,
};

/// A transform of the lists.
#[derive(Debug, Clone, Copy)]
enum T {
    /// `ExponentTransform(value, negativeStyle, direction)`.
    Exp([f64; 4], NegativeStyle, TransformDirection),
    /// `ExponentWithLinearTransform(gamma, offset, negativeStyle, direction)`.
    Lin([f64; 4], [f64; 4], NegativeStyle, TransformDirection),
    /// A `MatrixTransform` scaling R, G and B.
    Scale(f64),
}

impl T {
    /// Whether every parameter is finite: then JSON can hold them.
    fn finite(&self) -> bool {
        match self {
            T::Exp(v, ..) => v.iter().all(|x| x.is_finite()),
            T::Lin(g, o, ..) => g.iter().chain(o).all(|x| x.is_finite()),
            T::Scale(s) => s.is_finite(),
        }
    }

    /// The JSON spec.
    fn spec(&self) -> Value {
        match *self {
            T::Exp(value, neg, dir) => json!({"class": "ExponentTransform", "args": {
                "value": value, "negativeStyle": negative_style_enum(neg),
                "direction": direction_enum(dir)}}),
            T::Lin(gamma, offset, neg, dir) => {
                json!({"class": "ExponentWithLinearTransform", "args": {
                "gamma": gamma, "offset": offset, "negativeStyle": negative_style_enum(neg),
                "direction": direction_enum(dir)}})
            }
            T::Scale(s) => json!({"class": "MatrixTransform", "args": {"matrix": scale(s)}}),
        }
    }

    /// The config YAML.
    fn yaml(&self) -> String {
        match *self {
            T::Exp(value, neg, dir) => format!(
                "!<ExponentTransform> {{value: {}, style: {}, direction: {}}}",
                yaml_list(&value),
                yaml_style(neg),
                yaml_direction(dir)
            ),
            T::Lin(gamma, offset, neg, dir) => format!(
                "!<ExponentWithLinearTransform> {{gamma: {}, offset: {}, style: {}, \
                 direction: {}}}",
                yaml_list(&gamma),
                yaml_list(&offset),
                yaml_style(neg),
                yaml_direction(dir)
            ),
            T::Scale(s) => format!("!<MatrixTransform> {{matrix: {}}}", yaml_list(&scale(s))),
        }
    }

    /// The op data of a Gamma transform, and the transform's validation prefix.
    fn gamma_data(&self) -> Option<(GammaOpData, &'static str)> {
        match *self {
            T::Exp(value, neg, dir) => Some((
                exponent_op(value, neg, dir),
                "ExponentTransform validation failed: ",
            )),
            T::Lin(gamma, offset, neg, dir) => Some((
                exponent_with_linear_op(gamma, offset, neg, dir),
                "ExponentWithLinearTransform validation failed: ",
            )),
            T::Scale(_) => None,
        }
    }
}

/// A matrix scaling R, G and B by `s`.
fn scale(s: f64) -> [f64; 16] {
    [
        s, 0.0, 0.0, 0.0, 0.0, s, 0.0, 0.0, 0.0, 0.0, s, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
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

/// `OPTIMIZATION_NONE` alone.
fn no_optimization() -> Vec<(Option<Value>, OptimizationFlags)> {
    levels()[1..2].to_vec()
}

/// The shader description's names; `None` keeps the default.
#[derive(Debug, Clone, Copy, Default)]
struct Names {
    pixel: Option<&'static str>,
    prefix: Option<&'static str>,
    function: Option<&'static str>,
}

/// One extraction: a list of transforms, an optimization level, a language and names.
#[derive(Debug, Clone)]
struct Case {
    label: String,
    chain: Vec<T>,
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

/// The oracle's processor for `chain`: a `GroupTransform`, as JSON, or, when a value isn't
/// finite, in a config's YAML (JSON can't hold NaN and infinities).
fn processor(chain: &[T]) -> Value {
    if chain.iter().all(T::finite) {
        let children: Vec<Value> = chain.iter().map(T::spec).collect();
        return json!({"transform": {"class": "GroupTransform", "children": children}});
    }
    let children: Vec<String> = chain.iter().map(T::yaml).collect();
    let config = format!(
        "{RAW_CONFIG}  - !<ColorSpace>\n    name: cs\n    from_scene_reference: \
         !<GroupTransform> {{children: [{}]}}\n",
        children.join(", ")
    );
    json!({"config": {"yaml": config}, "src": "raw", "dst": "cs"})
}

/// The processor's ops, as the transforms build them (see the module notes), before the
/// processor finalizes them. An error carries the wheel's stage: the binding's constructor
/// validates each Gamma transform with its prefix on the JSON route (`json`), and
/// `BuildExponentOp` validates without one.
fn raw_ops(chain: &[T], json: bool) -> Result<OpVec, String> {
    if json {
        for t in chain {
            if let Some((data, prefix)) = t.gamma_data() {
                data.validate()
                    .map_err(|e| format!("transform: {prefix}{}", e.message()))?;
            }
        }
    }
    let processor = |e: ocio_ops::Exception| format!("processor: {}", e.message());
    let mut raw = OpVec::new();
    for t in chain {
        match t.gamma_data() {
            Some((data, _)) => {
                data.validate().map_err(processor)?;
                create_gamma_op(&mut raw, data, F);
            }
            None => {
                let T::Scale(s) = *t else { unreachable!() };
                let mut data = MatrixOpData::new();
                data.set_rgba(&scale(s));
                data.validate().map_err(processor)?;
                create_matrix_op(&mut raw, data, F);
            }
        }
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
                "a Gamma shader has no uniform nor texture"
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

/// The port's outcome, from the raw ops, or, when `finalize_first`, from ops the processor
/// finalized first, as the wheel's processor does (Processor.cpp:618-641): `GpuProcessor::new`
/// takes either, and finalizes them itself.
fn port(case: &Case, finalize_first: bool) -> Outcome {
    let text = |b: &[u8]| String::from_utf8(b.to_vec()).unwrap();
    let json = case.processor.get("transform").is_some();
    let mut raw = match raw_ops(&case.chain, json) {
        Ok(raw) => raw,
        Err(e) => return Outcome::Raised(e),
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
/// Returns how many cases the wheel extracted a shader for, and how many it refused.
fn check(cases: &[Case]) -> (usize, usize) {
    let requests: Vec<GpuShaderRequest> = cases.iter().map(Case::request).collect();
    let calls: Vec<_> = requests.iter().map(GpuShaderRequest::call).collect();
    let mut failures: Vec<(&Case, Outcome, Outcome)> = Vec::new();
    let (mut extracted, mut refused) = (0, 0);
    for (case, response) in cases.iter().zip(Oracle::get().batch(&calls, true)) {
        let reply = GpuShaderReply::from_response(
            response.unwrap_or_else(|e| panic!("{}: the oracle failed: {e}", case.label)),
        );
        let wheel = wheel(&reply);
        match &wheel {
            Outcome::Extracted { shader: Ok(_), .. } => extracted += 1,
            _ => refused += 1,
        }
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
    (extracted, refused)
}

/// Every language, with the oracle's name for it.
fn languages() -> impl Iterator<Item = (GpuLanguage, oracle_gpu::GpuLanguage)> {
    GpuLanguage::ALL
        .into_iter()
        .zip(oracle_gpu::GpuLanguage::ALL)
}

/// `chain` at the levels `flags`, in every language, with `names`; the oracle's processor is
/// `processor`.
fn cases_with(
    label: &str,
    chain: Vec<T>,
    processor: Value,
    flags: &[(Option<Value>, OptimizationFlags)],
    names: Names,
) -> Vec<Case> {
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

/// `chain` at the levels `flags`, in every language, with `names`.
fn cases_of(
    label: &str,
    chain: Vec<T>,
    flags: &[(Option<Value>, OptimizationFlags)],
    names: Names,
) -> Vec<Case> {
    let processor = processor(&chain);
    cases_with(label, chain, processor, flags, names)
}

/// The negative style of an oracle `NEGATIVE_*` name.
fn negative_style(name: &str) -> NegativeStyle {
    match name {
        "NEGATIVE_CLAMP" => Clamp,
        "NEGATIVE_MIRROR" => Mirror,
        "NEGATIVE_PASS_THRU" => PassThru,
        "NEGATIVE_LINEAR" => Linear,
        _ => panic!("unknown negative style {name}"),
    }
}

/// Upstream's `ExponentOp` and `ExponentWithLinearOp` GPU tests (tests/gpu/GammaOp_test.cpp
/// @ v2.5.2, `ocio_testkit::gpu_cases`) that use a version 2 config, which builds Gamma ops:
/// the wheel's shader for each, from `getDefaultGPUProcessor` as the tests take it and from
/// every optimization level, in every language, is the port's. (The four at version 1 build
/// Exponent ops, `p1-exponent`'s; the legacy GPU processor comes in Phase 7.)
#[test]
fn upstreams_gpu_tests_write_the_wheels_shaders() {
    let mut cases = Vec::new();
    let upstream: Vec<_> = gamma_cases()
        .into_iter()
        .filter(|c| c.config_major_version() == 2)
        .collect();
    assert_eq!(upstream.len(), 10);
    for case in upstream {
        let dir = if case.inverse { I } else { F };
        let neg = negative_style(case.negative_style);
        let t = match case.transform {
            GammaTransform::Exponent { value, .. } => T::Exp(value, neg, dir),
            GammaTransform::ExponentWithLinear { gamma, offset } => T::Lin(gamma, offset, neg, dir),
        };
        let processor = json!({ "transform": case.transform() });
        cases.extend(cases_with(
            &format!("{} {}", case.group, case.name),
            vec![t],
            processor,
            &levels(),
            Names::default(),
        ));
    }
    let (extracted, _) = check(&cases);
    assert_eq!(extracted, cases.len());
}

/// Each style, forward and inverse, in every language and at every level, with exponents
/// whose `double` and `float` literals differ; identities and inverse pairs the optimizer
/// removes (the clamping ones without optimization: see the module notes); every pair of
/// basic styles and directions, which combine; moncurves, which don't; a list around a
/// matrix; refusals; and names (an empty pixel name is an error, but in OSL).
#[test]
fn every_style_writes_the_wheels_shader() {
    let v = [2.2, 1.0 / 0.45, 1.23456789, 1.5];
    let lin = ([2.4, 1.0 / 0.45, 3.0, 1.1], [0.055, 0.099, 0.16, 0.0]);
    let one = [1.0; 4];
    let lin_one = ([1.0; 4], [0.0; 4]);
    let mut all: Vec<(String, Vec<T>)> = Vec::new();
    let mut unoptimized: Vec<(String, Vec<T>)> = Vec::new();
    for neg in [Clamp, Mirror, PassThru] {
        for dir in [F, I] {
            let inv = get_inverse_transform_direction(dir);
            all.push((format!("{neg:?} {dir:?}"), vec![T::Exp(v, neg, dir)]));
            let identities = if neg == Clamp {
                &mut unoptimized
            } else {
                &mut all
            };
            identities.push((
                format!("{neg:?} {dir:?} identity"),
                vec![T::Exp(one, neg, dir)],
            ));
            identities.push((
                format!("{neg:?} {dir:?} inverse pair"),
                vec![T::Exp(v, neg, dir), T::Exp(v, neg, inv)],
            ));
        }
    }
    for neg in [Linear, Mirror] {
        for dir in [F, I] {
            let inv = get_inverse_transform_direction(dir);
            all.extend([
                (
                    format!("moncurve {neg:?} {dir:?}"),
                    vec![T::Lin(lin.0, lin.1, neg, dir)],
                ),
                (
                    format!("moncurve {neg:?} {dir:?} identity"),
                    vec![T::Lin(lin_one.0, lin_one.1, neg, dir)],
                ),
                (
                    format!("moncurve {neg:?} {dir:?} inverse pair"),
                    vec![
                        T::Lin(lin.0, lin.1, neg, dir),
                        T::Lin(lin.0, lin.1, neg, inv),
                    ],
                ),
                (
                    format!("moncurve {neg:?} {dir:?} twice"),
                    vec![
                        T::Lin(lin.0, lin.1, neg, dir),
                        T::Lin(lin.0, lin.1, neg, dir),
                    ],
                ),
                (
                    format!("moncurve {neg:?} {dir:?} after a basic one"),
                    vec![T::Exp(v, Mirror, dir), T::Lin(lin.0, lin.1, neg, dir)],
                ),
            ]);
        }
    }
    // Every pair of basic styles and directions: combined where `mayCompose` allows it.
    let a = [1.0 / 0.45, 2.0, 0.5, 1.25];
    let b = [0.45, 3.0, 0.8, 1.0];
    for neg1 in [Clamp, Mirror, PassThru] {
        for neg2 in [Clamp, Mirror, PassThru] {
            for (d1, d2) in [(F, F), (F, I), (I, F), (I, I)] {
                all.push((
                    format!("{neg1:?} {d1:?} then {neg2:?} {d2:?}"),
                    vec![T::Exp(a, neg1, d1), T::Exp(b, neg2, d2)],
                ));
            }
        }
    }
    all.extend([
        (
            "around a matrix".to_string(),
            vec![T::Exp(v, Mirror, F), T::Scale(2.0), T::Exp(v, Mirror, I)],
        ),
        (
            "around a matrix, moncurve".to_string(),
            vec![
                T::Lin(lin.0, lin.1, Linear, F),
                T::Scale(0.5),
                T::Lin(lin.0, lin.1, Linear, I),
            ],
        ),
        // Refused: a basic exponent below its bound, a moncurve offset above its bound.
        (
            "refused basic".to_string(),
            vec![
                T::Exp(v, Clamp, F),
                T::Exp([0.001, 1.0, 1.0, 1.0], Clamp, F),
            ],
        ),
        (
            "refused moncurve".to_string(),
            vec![T::Lin(
                [2.4, 2.2, 2.2, 1.0],
                [0.055, 0.95, 0.1, 0.0],
                Linear,
                F,
            )],
        ),
    ]);
    let mut cases = Vec::new();
    for (label, chain) in all {
        cases.extend(cases_of(&label, chain, &levels(), Names::default()));
    }
    for (label, chain) in unoptimized {
        cases.extend(cases_of(
            &label,
            chain,
            &no_optimization(),
            Names::default(),
        ));
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
        for chain in [
            vec![T::Exp(v, PassThru, I)],
            vec![T::Lin(lin.0, lin.1, Mirror, F)],
        ] {
            cases.extend(cases_of("names", chain, &no_optimization(), names));
        }
    }
    let (extracted, refused) = check(&cases);
    assert!(extracted > 0 && refused > 0);
}

/// Extreme parameters, generated: each exponent of each basic style, and each gamma and
/// offset of each moncurve style, in turn NaN, ±Inf, the largest double, -0, the smallest
/// denormal, 1e-9, 65504.5 (beyond Cg's half range), the bounds `validate` checks and values
/// just past them, without optimization, in every language.
#[test]
fn extreme_parameters_write_the_wheels_shader() {
    let v = [2.2, 1.0 / 0.45, 1.23456789, 1.5];
    let lin = ([2.4, 1.0 / 0.45, 3.0, 1.1], [0.055, 0.099, 0.16, 0.0]);
    let common = [
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
    // The basic styles' bounds: [0.01, 100].
    let basic: Vec<f64> = common
        .into_iter()
        .chain([0.01, 0.009999999, 100.0, 100.00001])
        .collect();
    for neg in [Clamp, Mirror, PassThru] {
        for dir in [F, I] {
            for slot in 0..4 {
                for &x in &basic {
                    let mut e = v;
                    e[slot] = x;
                    cases.extend(cases_of(
                        &format!("{neg:?} {dir:?} value[{slot}] = {x:e}"),
                        vec![T::Exp(e, neg, dir)],
                        &no_optimization(),
                        Names::default(),
                    ));
                }
            }
        }
    }
    // The moncurve styles' bounds: [1, 10] for the gamma, [0, 0.9] for the offset.
    let moncurve: Vec<f64> = common
        .into_iter()
        .chain([1.0, 0.9999999, 10.0, 10.000001, 0.9, 0.9000001])
        .collect();
    for neg in [Linear, Mirror] {
        for dir in [F, I] {
            for slot in 0..8 {
                for &x in &moncurve {
                    let (mut g, mut o) = lin;
                    if slot < 4 {
                        g[slot] = x;
                    } else {
                        o[slot - 4] = x;
                    }
                    cases.extend(cases_of(
                        &format!("moncurve {neg:?} {dir:?} slot {slot} = {x:e}"),
                        vec![T::Lin(g, o, neg, dir)],
                        &no_optimization(),
                        Names::default(),
                    ));
                }
            }
        }
    }
    let (extracted, refused) = check(&cases);
    assert!(extracted > 0 && refused > 0);
}
