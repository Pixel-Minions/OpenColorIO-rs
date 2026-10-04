// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Range op's GPU writer against the wheel (`gpu_shader`), in the 10 languages: for each
//! case, the GPU processor's cache ID and queries, and the shader the extraction writes (its
//! text, cache ID and names), or the error, byte for byte.
//!
//! The port builds the processor's ops as the wheel does: a `GroupTransform` builds each child
//! forward; `BuildRangeOp` validates the transform's data and makes a Range op with a copy of
//! it in the transform's direction, or, for `RANGE_NO_CLAMP`, a Matrix op from it
//! (`convertToMatrix`; src/OpenColorIO/ops/range/RangeOp.cpp:263-281 @ v2.5.2); and the
//! processor finalizes them, which makes an inverse range forward (Processor.cpp:618-641,
//! RangeOp.cpp:176-183). The GPU processor is then `getOptimizedGPUProcessor` at each level, or
//! `getDefaultGPUProcessor` (Processor.cpp:437-445, 491-523). The port runs it from the ops as
//! they are, and from them finalized first: both must give the wheel's outcome.
//!
//! Finite bounds go through JSON (the binding's constructor validates the transform, with the
//! prefix "RangeTransform validation failed: "), NaN and infinite ones through a config's YAML
//! (`BuildRangeOp` validates, without a prefix). An empty bound is left out of the JSON spec;
//! in YAML it is `.nan`, which the setters take for empty.

use ocio_gpu::gpu_processor::GpuProcessor;
use ocio_gpu::{GpuLanguage, GpuShaderDesc};
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{OptimizationFlags, TransformDirection};
use ocio_ops::ops::matrix::MatrixOpData;
use ocio_ops::ops::matrix::matrix_op::create_matrix_op;
use ocio_ops::ops::range::RangeOpData;
use ocio_ops::ops::range::range_op::create_range_op;
use ocio_testkit::battery::{yaml_list, yaml_number};
use ocio_testkit::gpu::{self as oracle_gpu, GpuShaderReply, GpuShaderRequest, ShaderSettings};
use ocio_testkit::gpu_cases::range_cases;
use ocio_testkit::{Oracle, assert_text_eq};
use serde_json::{Map, Value, json};

use TransformDirection::{Forward as F, Inverse as I};

/// An empty bound.
const E: f64 = f64::NAN;

/// A transform of the lists.
#[derive(Debug, Clone, Copy)]
enum T {
    /// A `RangeTransform`: `[minIn, maxIn, minOut, maxOut]` (NaN for an empty bound), its
    /// direction, and `RANGE_NO_CLAMP` rather than `RANGE_CLAMP`.
    Range([f64; 4], TransformDirection, bool),
    /// A `MatrixTransform` scaling R, G and B.
    Scale(f64),
}

impl T {
    /// Whether every bound is finite or empty, and so JSON can hold it.
    fn finite(&self) -> bool {
        match self {
            T::Range(b, ..) => b.iter().all(|x| x.is_nan() || x.is_finite()),
            T::Scale(s) => s.is_finite(),
        }
    }

    /// The JSON spec: the constructor's arguments (it validates), then the style.
    fn spec(&self) -> Value {
        match *self {
            T::Range(bounds, dir, no_clamp) => {
                let mut args = Map::new();
                for (key, b) in ["minInValue", "maxInValue", "minOutValue", "maxOutValue"]
                    .iter()
                    .zip(bounds)
                {
                    if !b.is_nan() {
                        args.insert(key.to_string(), json!(b));
                    }
                }
                args.insert("direction".into(), direction_enum(dir));
                let mut spec = json!({"class": "RangeTransform", "args": args});
                if no_clamp {
                    spec["calls"] = json!([["setStyle", {"enum": "RANGE_NO_CLAMP"}]]);
                }
                spec
            }
            T::Scale(s) => json!({"class": "MatrixTransform", "args": {"matrix": scale(s)}}),
        }
    }

    /// The config YAML.
    fn yaml(&self) -> String {
        match *self {
            T::Range(b, dir, no_clamp) => format!(
                "!<RangeTransform> {{min_in_value: {}, max_in_value: {}, min_out_value: {}, \
                 max_out_value: {}, style: {}, direction: {}}}",
                yaml_number(b[0]),
                yaml_number(b[1]),
                yaml_number(b[2]),
                yaml_number(b[3]),
                if no_clamp { "noClamp" } else { "Clamp" },
                match dir {
                    F => "forward",
                    I => "inverse",
                }
            ),
            T::Scale(s) => format!("!<MatrixTransform> {{matrix: {}}}", yaml_list(&scale(s))),
        }
    }
}

fn direction_enum(dir: TransformDirection) -> Value {
    match dir {
        F => json!({"enum": "TRANSFORM_DIR_FORWARD"}),
        I => json!({"enum": "TRANSFORM_DIR_INVERSE"}),
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
    /// JSON, each `RangeTransform` from the binding's constructor, which validates it
    /// (PyRangeTransform.cpp:17-36 @ v2.5.2), then its style's setter.
    Constructor,
    /// JSON, a default `RangeTransform` and its setters, which don't validate.
    Setters,
    /// A config's YAML, from a colour space: no transform's `validate`.
    Yaml,
}

/// `RangeTransformImpl::validate` (src/OpenColorIO/transforms/RangeTransform.cpp:52-73
/// @ v2.5.2): the data's validation, and both bounds for the non-clamping style, the message
/// prefixed with "RangeTransform validation failed: " (a second time for the style's own).
fn range_transform_validate(data: &RangeOpData, no_clamp: bool) -> Result<(), String> {
    let inner = data
        .validate()
        .map_err(|e| e.message().to_string())
        .and_then(|()| {
            if no_clamp && (data.min_is_empty() || data.max_is_empty()) {
                Err(
                    "RangeTransform validation failed: non clamping range must have min and max \
                 values defined."
                        .to_string(),
                )
            } else {
                Ok(())
            }
        });
    inner.map_err(|m| format!("RangeTransform validation failed: {m}"))
}

/// The processor's ops, as the transforms build them (see the module notes), before the
/// processor finalizes them. An error carries the wheel's stage:
/// - on the constructor route, the binding validates each Range transform, as a clamping one
///   (the style comes after): "transform";
/// - on both JSON routes, `Processor::Impl::setTransform` validates the group, which validates
///   each child (Processor.cpp:633, GroupTransform.cpp:56-73): "processor";
/// - `BuildRangeOp` validates the data, without a prefix, and a non-clamping range converts
///   to a matrix (RangeOp.cpp:263-281): "processor".
fn raw_ops(chain: &[T], route: Route) -> Result<OpVec, String> {
    let range_data = |bounds: &[f64; 4], dir: TransformDirection| {
        let mut data = RangeOpData::new();
        data.set_min_in_value(bounds[0]);
        data.set_max_in_value(bounds[1]);
        data.set_min_out_value(bounds[2]);
        data.set_max_out_value(bounds[3]);
        data.set_direction(dir);
        data
    };
    if route == Route::Constructor {
        for t in chain {
            if let T::Range(bounds, dir, _) = t {
                range_transform_validate(&range_data(bounds, *dir), false)
                    .map_err(|m| format!("transform: {m}"))?;
            }
        }
    }
    if route != Route::Yaml {
        for t in chain {
            if let T::Range(bounds, dir, no_clamp) = t {
                range_transform_validate(&range_data(bounds, *dir), *no_clamp)
                    .map_err(|m| format!("processor: {m}"))?;
            }
        }
    }
    let processor = |e: ocio_ops::Exception| format!("processor: {}", e.message());
    let mut raw = OpVec::new();
    for t in chain {
        match t {
            T::Range(bounds, dir, no_clamp) => {
                let data = range_data(bounds, *dir);
                data.validate().map_err(processor)?;
                if *no_clamp {
                    let m = data.convert_to_matrix().map_err(processor)?;
                    create_matrix_op(&mut raw, m, F);
                } else {
                    create_range_op(&mut raw, data, F).map_err(processor)?;
                }
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
                "a Range shader has no uniform nor texture"
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

/// Upstream's `RangeOp` GPU tests (tests/gpu/RangeOp_test.cpp @ v2.5.2,
/// `ocio_testkit::gpu_cases`): the wheel's shader for each, from `getDefaultGPUProcessor` as
/// the tests take it and from every optimization level, in every language, is the port's.
/// The two `RANGE_NO_CLAMP` ones build a Matrix op.
#[test]
fn upstreams_gpu_tests_write_the_wheels_shaders() {
    let mut cases = Vec::new();
    for case in range_cases() {
        let bound = |b: Option<f64>| b.unwrap_or(E);
        let t = T::Range(
            [
                bound(case.min_in),
                bound(case.max_in),
                bound(case.min_out),
                bound(case.max_out),
            ],
            F,
            case.no_clamp,
        );
        let processor = json!({ "transform": case.transform() });
        cases.extend(cases_with(
            &format!("RangeOp {}", case.name),
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

/// Each path of the writer, in every language and at every level: ranges that scale or only
/// clamp (offsets either side of the 1e-6 that `scales` takes for none), one-sided ranges,
/// inverses, compositions the optimizer makes (upstream's `compose` cases,
/// tests/cpu/ops/range/RangeOpData_tests.cpp), constants, identities, ranges around a matrix,
/// bounds beyond Cg's half range and below its smallest normal, the non-clamping style,
/// refusals, and names (an empty pixel name is an error, but in OSL).
#[test]
fn every_path_writes_the_wheels_shader() {
    let r = |b: [f64; 4], dir| T::Range(b, dir, false);
    let r1 = [0., 1., 0., 1.];
    let r2 = [0.1, 0.9, 0.1, 0.9];
    let r3 = [0.1, 1.9, 0.1, 1.9];
    let r4 = [0.1, 1.9, 0.2, 1.8];
    let r6 = [-1.0, 1.0, 0., 1.2];
    let r7 = [E, 0.5, E, 0.5];
    let r8 = [0.5, E, 0.5, E];
    let r9 = [1.1, 1.9, 1.2, 1.5];
    let r10 = [-1.1, -0.1, 1.1, 1.9];
    let neg = [0., E, 0., E];
    let wide = [-0.1, 1.1, -0.1, 1.1];
    let thirds = [1.0 / 3.0, 2.0 / 3.0, 0.1, 0.7];
    let off_lo = [0., 1., 0.5e-6, 1. + 0.5e-6];
    let off_hi = [0., 1., 1.5e-6, 1. + 1.5e-6];
    let half = [-70000.0, 70000.0, -65520.0, 1e-9];
    let tiny = [1e-9, 2.0, 6.0e-5, 65504.5];
    let chains: Vec<(&str, Vec<T>)> = vec![
        ("r1", vec![r(r1, F)]),
        ("r4", vec![r(r4, F)]),
        ("r4 inverse", vec![r(r4, I)]),
        ("r6 inverse", vec![r(r6, I)]),
        ("max only", vec![r(r7, F)]),
        ("min only inverse", vec![r(r8, I)]),
        ("thirds", vec![r(thirds, F)]),
        ("thirds inverse", vec![r(thirds, I)]),
        ("offset below 1e-6", vec![r(off_lo, F)]),
        ("offset above 1e-6", vec![r(off_hi, F)]),
        ("offset above 1e-6 inverse", vec![r(off_hi, I)]),
        ("half range", vec![r(half, F)]),
        ("half range inverse", vec![r(half, I)]),
        ("tiny", vec![r(tiny, F)]),
        ("r1 r2", vec![r(r1, F), r(r2, F)]),
        ("r1 r3", vec![r(r1, F), r(r3, F)]),
        ("r1 r4", vec![r(r1, F), r(r4, F)]),
        ("r1 r6", vec![r(r1, F), r(r6, F)]),
        ("max only r4", vec![r(r7, F), r(r4, F)]),
        ("r8 r3", vec![r(r8, F), r(r3, F)]),
        ("r1 r9 (constant)", vec![r(r1, F), r(r9, F)]),
        ("r1 r10 (constant)", vec![r(r1, F), r(r10, F)]),
        ("max only, min only", vec![r(r7, F), r(r8, F)]),
        ("r4 cancelled", vec![r(r4, F), r(r4, I)]),
        ("r6 r2 r9", vec![r(r6, I), r(r2, F), r(r9, I)]),
        (
            "around a matrix",
            vec![r(neg, F), T::Scale(2.0), r(wide, F)],
        ),
        ("r1 offset below 1e-6", vec![r(r1, F), r(off_lo, F)]),
        ("no clamp", vec![T::Range(r4, F, true)]),
        ("no clamp inverse", vec![T::Range(r4, I, true)]),
        ("no clamp, one-sided (refused)", vec![T::Range(r7, F, true)]),
        ("refused bounds", vec![r([0.5, 0.5, 0.1, 0.9], F)]),
        ("refused one side", vec![r([0.1, E, E, 0.9], F)]),
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
        cases.extend(cases_of("names", vec![r(r4, F)], &no_optimization(), names));
    }
    let (extracted, refused) = check(&cases);
    assert!(extracted > 0 && refused > 0);
}

/// Extreme bounds, generated: each of a range's four bounds in turn empty (NaN), ±Inf, the
/// largest double, -0, the smallest denormal, 1e-9, values beyond the float range and Cg's
/// half range, forward and inverse, at every level, in every language; alone, and in lists the
/// optimizer combines or removes: after `[0, 1, 0, 1]`, before it, after an inverse `r6`,
/// twice, as an inverse pair, and around a matrix.
#[test]
fn extreme_bounds_write_the_wheels_shader() {
    let bases = [
        [0.1, 1.1, 0.5, 1.5],
        [-0.010201, 0.601102, 0.209803, 1.600208],
    ];
    let values = [
        E,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::MAX,
        -f64::MAX,
        -0.0,
        f64::from_bits(1),
        1e-9,
        1e39,
        65504.5,
    ];
    let r1 = T::Range([0., 1., 0., 1.], F, false);
    let r6_inverse = T::Range([-1.0, 1.0, 0., 1.2], I, false);
    let mut cases = Vec::new();
    for (b, base) in bases.into_iter().enumerate() {
        for slot in 0..4 {
            for v in values {
                for dir in [F, I] {
                    let mut bounds = base;
                    bounds[slot] = v;
                    let x = T::Range(bounds, dir, false);
                    let inverse = match dir {
                        F => I,
                        I => F,
                    };
                    let other = T::Range(bounds, inverse, false);
                    let chains = [
                        ("", vec![x]),
                        (" after r1", vec![r1, x]),
                        (" before r1", vec![x, r1]),
                        (" after r6 inverse", vec![r6_inverse, x]),
                        (" twice", vec![x, x]),
                        (" inverse pair", vec![x, other]),
                        (" around a matrix", vec![x, T::Scale(2.0), x]),
                    ];
                    for (label, chain) in chains {
                        cases.extend(cases_of(
                            &format!("base {b} bound[{slot}] = {v:e} {dir:?}{label}"),
                            chain,
                            &levels(),
                            Names::default(),
                        ));
                    }
                }
            }
        }
    }
    let (extracted, refused) = check(&cases);
    assert!(extracted > 0 && refused > 0);
}
