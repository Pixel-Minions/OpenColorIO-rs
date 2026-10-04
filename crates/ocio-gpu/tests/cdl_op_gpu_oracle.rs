// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The CDL op's GPU writer against the wheel (`gpu_shader`), in the 10 languages: for each
//! case, the GPU processor's cache ID and queries, and the shader the extraction writes (its
//! text, cache ID and names), or the error, byte for byte.
//!
//! In a version 2 config, a `CDLTransform` builds a CDL op with a copy of its data
//! (`BuildCDLOp`, src/OpenColorIO/ops/cdl/CDLOp.cpp:200-265 @ v2.5.2); a version 1 config
//! builds matrices and an Exponent op instead, which the transforms' port (WP 1.8) brings. A
//! `GroupTransform` builds each child forward, and the processor finalizes the ops
//! (Processor.cpp:618-641); the GPU processor is then `getOptimizedGPUProcessor` at each level,
//! or `getDefaultGPUProcessor` (Processor.cpp:437-445, 491-523). The port builds the same data
//! (the helpers of `crates/ocio-ops/tests/common/cdl.rs`), the ops with `create_cdl_op`, and
//! runs the GPU processor from them as they are, and from them finalized first: both must give
//! the wheel's outcome.
//!
//! Finite parameters go through JSON, NaN and infinite ones through a config's YAML. The
//! optimizer replaces an identity CDL, an inverse pair, and a CDL whose power is 1, with Range
//! and Matrix ops (`CDLOpData::getIdentityReplacement`, `getSimplerReplacement`), whose GPU
//! writers are ported too.

use ocio_gpu::gpu_processor::GpuProcessor;
use ocio_gpu::{GpuLanguage, GpuShaderDesc};
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{CdlStyle, OptimizationFlags, TransformDirection};
use ocio_ops::ops::cdl::cdl_op::create_cdl_op;
use ocio_ops::ops::matrix::MatrixOpData;
use ocio_ops::ops::matrix::matrix_op::create_matrix_op;
use ocio_testkit::battery::{yaml_list, yaml_number};
use ocio_testkit::gpu::{self as oracle_gpu, GpuShaderReply, GpuShaderRequest, ShaderSettings};
use ocio_testkit::gpu_cases::cdl_cases;
use ocio_testkit::{Oracle, assert_text_eq};
use serde_json::{Value, json};

#[allow(dead_code)] // The GPU tests use a subset.
#[path = "../../ocio-ops/tests/common/cdl.rs"]
mod cdl;

use CdlStyle::{Asc, NoClamp};
use TransformDirection::{Forward as F, Inverse as I};
use cdl::{Cdl, yaml_style};

/// A transform of the lists.
#[derive(Debug, Clone, Copy)]
enum T {
    /// A `CDLTransform` and its direction.
    Cdl(Cdl, TransformDirection),
    /// A `MatrixTransform` scaling R, G and B.
    Scale(f64),
}

/// A CDL.
fn cdl(slope: [f64; 3], offset: [f64; 3], power: [f64; 3], sat: f64, style: CdlStyle) -> Cdl {
    Cdl {
        slope,
        offset,
        power,
        sat,
        style,
    }
}

impl T {
    /// Whether every parameter is finite: then JSON can hold them.
    fn finite(&self) -> bool {
        match self {
            T::Cdl(c, _) => c
                .slope
                .iter()
                .chain(&c.offset)
                .chain(&c.power)
                .chain([&c.sat])
                .all(|v| v.is_finite()),
            T::Scale(s) => s.is_finite(),
        }
    }

    /// The JSON spec: the constructor's arguments (it validates), then the style.
    fn spec(&self) -> Value {
        match *self {
            T::Cdl(c, dir) => c.spec(dir),
            T::Scale(s) => json!({"class": "MatrixTransform", "args": {"matrix": scale(s)}}),
        }
    }

    /// The config YAML.
    fn yaml(&self) -> String {
        match *self {
            T::Cdl(c, dir) => format!(
                "!<CDLTransform> {{slope: {}, offset: {}, power: {}, sat: {}, style: {}, \
                 direction: {}}}",
                yaml_list(&c.slope),
                yaml_list(&c.offset),
                yaml_list(&c.power),
                yaml_number(c.sat),
                yaml_style(c.style),
                match dir {
                    F => "forward",
                    I => "inverse",
                }
            ),
            T::Scale(s) => format!("!<MatrixTransform> {{matrix: {}}}", yaml_list(&scale(s))),
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
    route: Route,
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

/// The oracle's processor for `chain`: a `GroupTransform`, as JSON, or, when a bound is
/// infinite, in a config's YAML (JSON can't hold infinities).
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

/// How the oracle's processor gets its transforms.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Route {
    /// JSON, each `CDLTransform` from the binding's constructor, which validates it
    /// (PyCDLTransform.cpp:37-62 @ v2.5.2), then its style's setter.
    Constructor,
    /// JSON, a default `CDLTransform` and its setters, which don't validate.
    Setters,
    /// A config's YAML, from a colour space: no transform's `validate`.
    Yaml,
}

/// The processor's ops, as the transforms build them (see the module notes), before the
/// processor finalizes them. An error carries the wheel's stage:
/// - on the constructor route, the binding validates each CDL transform: "transform";
/// - on both JSON routes, `Processor::Impl::setTransform` validates the group, which validates
///   each child, with `CDLTransformImpl::validate`'s prefix (Processor.cpp:633,
///   GroupTransform.cpp:56-73, CDLTransform.cpp:142-155): "processor";
/// - `BuildCDLOp` validates the data, without a prefix (CDLOp.cpp:255-264): "processor".
fn raw_ops(chain: &[T], route: Route) -> Result<OpVec, String> {
    let prefixed = |c: &Cdl, dir: TransformDirection, stage: &str| {
        c.op_data(dir)
            .validate()
            .map_err(|e| format!("{stage}: CDLTransform validation failed: {}", e.message()))
    };
    if route == Route::Constructor {
        for t in chain {
            if let T::Cdl(c, dir) = t {
                prefixed(c, *dir, "transform")?;
            }
        }
    }
    if route != Route::Yaml {
        for t in chain {
            if let T::Cdl(c, dir) = t {
                prefixed(c, *dir, "processor")?;
            }
        }
    }
    let processor = |e: ocio_ops::Exception| format!("processor: {}", e.message());
    let mut raw = OpVec::new();
    for t in chain {
        match t {
            T::Cdl(c, dir) => {
                let data = c.op_data(*dir);
                data.validate().map_err(processor)?;
                create_cdl_op(&mut raw, data, F);
            }
            T::Scale(s) => {
                let mut data = MatrixOpData::new();
                data.set_rgba(&scale(*s));
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
                "a CDL shader has no uniform nor texture"
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
    let mut raw = match raw_ops(&case.chain, case.route) {
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
    route: Route,
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
                route,
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
    let route = if processor.get("transform").is_some() {
        Route::Constructor
    } else {
        Route::Yaml
    };
    cases_with(label, chain, processor, route, flags, names)
}

/// Upstream's `CDLOp` GPU tests (tests/gpu/CDLOp_test.cpp @ v2.5.2, `ocio_testkit::gpu_cases`)
/// that use a version 2 config, which builds a CDL op: the wheel's shader for each, from
/// `getDefaultGPUProcessor` as the tests take it and from every optimization level, in every
/// language, is the port's. (The four at version 1 build matrices and an Exponent op, through
/// the transforms' port, WP 1.8; the legacy GPU processor comes in Phase 7.)
#[test]
fn upstreams_gpu_tests_write_the_wheels_shaders() {
    let mut cases = Vec::new();
    let upstream: Vec<_> = cdl_cases().into_iter().filter(|c| c.version == 2).collect();
    assert_eq!(upstream.len(), 11);
    for case in upstream {
        let dir = if case.inverse { I } else { F };
        let style = match case.style {
            Some("CDL_ASC") => Asc,
            Some("CDL_NO_CLAMP") => NoClamp,
            other => panic!("{}: style {other:?}", case.name),
        };
        let p = case.params;
        let t = T::Cdl(
            cdl(p.slope, p.offset, p.power, p.sat.unwrap_or(1.0), style),
            dir,
        );
        let processor = json!({ "transform": case.transform() });
        cases.extend(cases_with(
            &format!("CDLOp {}", case.name),
            vec![t],
            processor,
            Route::Setters,
            &levels(),
            Names::default(),
        ));
    }
    let (extracted, _) = check(&cases);
    assert_eq!(extracted, cases.len());
}

/// Each style and direction, in every language and at every level: the lists of
/// `crates/ocio-ops/tests/cdl_op_oracle.rs` (upstream's data, powers of 1 that the optimizer
/// simplifies into matrices and ranges, identities, inverse pairs and pairs just past the
/// tolerance, saturation only, zero and tiny slopes and saturations, parameters that need 7
/// digits, refusals), a list around a matrix, and names (an empty pixel name is an error, but
/// in OSL).
#[test]
fn every_path_writes_the_wheels_shader() {
    // tests/cpu/ops/cdl/CDLOp_tests.cpp:60-66 @ v2.5.2.
    let data_1 = |style| {
        cdl(
            [1.35, 1.1, 0.071],
            [0.05, -0.23, 0.11],
            [0.93, 0.81, 1.27],
            1.23,
            style,
        )
    };
    let power_1 = |sat, style| cdl([1.2, 0.8, 1.1], [0.05, -0.1, 0.0], [1.0; 3], sat, style);
    let identity = |style| cdl([1.0; 3], [0.0; 3], [1.0; 3], 1.0, style);
    let mut chains: Vec<(String, Vec<T>)> = Vec::new();
    for style in [Asc, NoClamp] {
        for dir in [F, I] {
            let other = match dir {
                F => I,
                I => F,
            };
            let mut push = |label: &str, chain: Vec<T>| {
                chains.push((format!("{style:?} {dir:?} {label}"), chain));
            };
            push("data 1", vec![T::Cdl(data_1(style), dir)]);
            push("power 1", vec![T::Cdl(power_1(1.0, style), dir)]);
            push("power 1, sat", vec![T::Cdl(power_1(0.7, style), dir)]);
            push("identity", vec![T::Cdl(identity(style), dir)]);
            push(
                "inverse pair",
                vec![T::Cdl(data_1(style), dir), T::Cdl(data_1(style), other)],
            );
            let mut sat = data_1(style);
            sat.sat = 1.3;
            push(
                "pair, other sat",
                vec![T::Cdl(data_1(style), dir), T::Cdl(sat, other)],
            );
            let mut past = data_1(style);
            past.power[1] += 2e-9;
            push(
                "pair past the tolerance",
                vec![T::Cdl(data_1(style), dir), T::Cdl(past, other)],
            );
            let mut within = data_1(style);
            within.power[1] += 0.5e-9;
            push(
                "pair within the tolerance",
                vec![T::Cdl(data_1(style), dir), T::Cdl(within, other)],
            );
            let mut near = power_1(0.7, style);
            near.power = [1.0 + 0.5e-9, 1.0 - 0.5e-9, 1.0];
            push("power near 1", vec![T::Cdl(near, dir)]);
            push(
                "two simplified",
                vec![
                    T::Cdl(power_1(0.7, style), dir),
                    T::Cdl(power_1(1.0, style), dir),
                ],
            );
            for sat in [0.7, 1.3] {
                push(
                    &format!("sat {sat} only"),
                    vec![T::Cdl(cdl([1.0; 3], [0.0; 3], [1.0; 3], sat, style), dir)],
                );
            }
            for (slope, sat) in [
                ([0.0, 1.0, 1.2], 0.9),
                ([1.2, 1.0, 0.9], 0.0),
                ([0.005, 1.0, 1.2], 0.9),
                ([1.2, 1.0, 0.9], 0.005),
            ] {
                push(
                    &format!("slope {slope:?}, sat {sat}"),
                    vec![T::Cdl(
                        cdl(slope, [0.1, 0.0, -0.1], [1.0; 3], sat, style),
                        dir,
                    )],
                );
            }
            push(
                "around a matrix",
                vec![
                    T::Cdl(data_1(style), dir),
                    T::Scale(2.0),
                    T::Cdl(data_1(style), other),
                ],
            );
        }
    }
    chains.extend([
        (
            "7 digits".to_string(),
            vec![T::Cdl(
                cdl(
                    [1.234567, 0.1234567, 12.34567],
                    [0.001234565, -1.2345678, 1234567.8],
                    [1.1234567, 0.98765432, 2.5],
                    0.9876543,
                    Asc,
                ),
                F,
            )],
        ),
        (
            "both styles".to_string(),
            vec![T::Cdl(data_1(Asc), F), T::Cdl(data_1(NoClamp), I)],
        ),
        // Refused: a negative slope, a zero power.
        (
            "negative slope".to_string(),
            vec![T::Cdl(
                cdl([-0.5, 1.0, 1.0], [0.0; 3], [1.2; 3], 1.0, Asc),
                F,
            )],
        ),
        (
            "zero power".to_string(),
            vec![T::Cdl(
                cdl([1.0; 3], [0.0; 3], [1.2, 0.0, 1.2], 1.0, NoClamp),
                I,
            )],
        ),
    ]);
    let mut cases = Vec::new();
    for (label, chain) in chains {
        cases.extend(cases_of(&label, chain, &levels(), Names::default()));
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
        for style in [Asc, NoClamp] {
            for dir in [F, I] {
                cases.extend(cases_of(
                    "names",
                    vec![T::Cdl(data_1(style), dir)],
                    &no_optimization(),
                    names,
                ));
            }
        }
    }
    let (extracted, refused) = check(&cases);
    assert!(extracted > 0 && refused > 0);
}

/// Extreme parameters, generated: each slope, offset and power, and the saturation, in turn
/// NaN, ±Inf, the largest double, -0, the smallest denormal, 1e-9, 1e39 and 65504.5 (beyond the
/// float range and Cg's half range), for each style and direction, without optimization, in
/// every language.
#[test]
fn extreme_parameters_write_the_wheels_shader() {
    let base = |style| {
        cdl(
            [1.15, 1.10, 0.90],
            [0.05, -0.02, 0.07],
            [1.20, 0.95, 1.13],
            0.9,
            style,
        )
    };
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
    for style in [Asc, NoClamp] {
        for dir in [F, I] {
            for slot in 0..10 {
                for v in values {
                    let mut c = base(style);
                    match slot {
                        0..3 => c.slope[slot] = v,
                        3..6 => c.offset[slot - 3] = v,
                        6..9 => c.power[slot - 6] = v,
                        _ => c.sat = v,
                    }
                    cases.extend(cases_of(
                        &format!("{style:?} {dir:?} slot {slot} = {v:e}"),
                        vec![T::Cdl(c, dir)],
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
