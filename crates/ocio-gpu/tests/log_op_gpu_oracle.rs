// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Log op's GPU writer against the wheel (`gpu_shader`), in the 10 languages: for each
//! case, the GPU processor's cache ID and queries, and the shader the extraction writes (its
//! text, cache ID and names), or the error, byte for byte.
//!
//! A `LogTransform`, `LogAffineTransform` or `LogCameraTransform` builds a Log op with a copy
//! of its data (`BuildLogOp`, src/OpenColorIO/ops/log/LogOp.cpp:190-221 @ v2.5.2). A
//! `GroupTransform` builds each child forward, and the processor finalizes the ops
//! (Processor.cpp:618-641); the GPU processor is then `getOptimizedGPUProcessor` at each level,
//! or `getDefaultGPUProcessor` (Processor.cpp:437-445, 491-523). The port builds the same data
//! (as `crates/ocio-ops/tests/log_op_oracle.rs` does), the ops with `create_log_op`, and runs
//! the GPU processor from them as they are, and from them finalized first: both must give the
//! wheel's outcome.
//!
//! The writer has six shapes: a plain base-2 or base-10 log and its inverse, the affine log
//! and its inverse, and the camera log (with a linear segment below a break) and its inverse.
//! The camera log's break and the offset of its linear segment are each platform's (I-70), so
//! its shaders differ between Windows and Linux, as the wheels' do; nothing here is committed,
//! every case is compared live on the platform it runs on.
//!
//! Finite parameters go through JSON, NaN and infinite ones through a config's YAML. The
//! optimizer replaces an inverse pair of logs with a Range or an identity Matrix op
//! (`LogOpData::getIdentityReplacement`), whose GPU writers are ported too.

use ocio_gpu::gpu_processor::GpuProcessor;
use ocio_gpu::{GpuLanguage, GpuShaderDesc};
use ocio_ops::Exception;
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{OptimizationFlags, TransformDirection};
use ocio_ops::ops::log::log_op::create_log_op;
use ocio_ops::ops::log::log_op_data::{LogAffineParameter, LogOpData};
use ocio_ops::ops::matrix::MatrixOpData;
use ocio_ops::ops::matrix::matrix_op::create_matrix_op;
use ocio_ops::ops::range::RangeOpData;
use ocio_ops::ops::range::range_op::create_range_op;
use ocio_testkit::battery::{yaml_list, yaml_number};
use ocio_testkit::gpu::{self as oracle_gpu, GpuShaderReply, GpuShaderRequest, ShaderSettings};
use ocio_testkit::gpu_cases::{LogCase, log_cases};
use ocio_testkit::{Oracle, assert_text_eq};
use serde_json::{Value, json};

use TransformDirection::{Forward as F, Inverse as I};

/// The four affine parameters: `[logSideSlope, logSideOffset, linSideSlope, linSideOffset]`.
type Affine = [[f64; 3]; 4];

/// The affine parameters' defaults (`LogAffineTransform`'s).
const DEFAULTS: Affine = [[1.0; 3], [0.0; 3], [1.0; 3], [0.0; 3]];

/// The setters of the affine parameters, in [`Affine`]'s order, and their YAML keys.
const SETTERS: [(&str, &str, LogAffineParameter); 4] = [
    (
        "setLogSideSlopeValue",
        "log_side_slope",
        LogAffineParameter::LogSideSlope,
    ),
    (
        "setLogSideOffsetValue",
        "log_side_offset",
        LogAffineParameter::LogSideOffset,
    ),
    (
        "setLinSideSlopeValue",
        "lin_side_slope",
        LogAffineParameter::LinSideSlope,
    ),
    (
        "setLinSideOffsetValue",
        "lin_side_offset",
        LogAffineParameter::LinSideOffset,
    ),
];

/// The bounds of [`T::Range`].
const RANGE: [f64; 4] = [0.25, 2.0, 0.25, 2.0];

/// A transform of the lists.
#[derive(Debug, Clone, Copy)]
enum T {
    /// A `LogTransform` of this base.
    Log(f64, TransformDirection),
    /// A `LogAffineTransform`: the base and the four parameters.
    Affine(f64, Affine, TransformDirection),
    /// A `LogCameraTransform`: the base, the four parameters, the break and maybe the linear
    /// slope.
    Camera(f64, Affine, [f64; 3], Option<[f64; 3]>, TransformDirection),
    /// A `MatrixTransform` scaling R, G and B.
    Scale(f64),
    /// A clamping `RangeTransform` from 0.25 to 2 on both sides.
    Range,
}

/// A camera log in base 2 with the parameters `p` and the break 0.1.
fn cam(p: Affine, slope: Option<[f64; 3]>, dir: TransformDirection) -> T {
    T::Camera(2.0, p, [0.1; 3], slope, dir)
}

impl T {
    /// Every number of the transform.
    fn numbers(&self) -> Vec<f64> {
        let flat = |p: &Affine| p.iter().flatten().copied().collect::<Vec<f64>>();
        match self {
            T::Log(base, _) => vec![*base],
            T::Affine(base, p, _) => [vec![*base], flat(p)].concat(),
            T::Camera(base, p, brk, slope, _) => [
                vec![*base],
                flat(p),
                brk.to_vec(),
                slope.map(|s| s.to_vec()).unwrap_or_default(),
            ]
            .concat(),
            T::Scale(s) => vec![*s],
            T::Range => RANGE.to_vec(),
        }
    }

    /// Whether every number is finite: then JSON can hold them.
    fn finite(&self) -> bool {
        self.numbers().iter().all(|v| v.is_finite())
    }

    /// The JSON spec: built empty, then set, so that the binding's constructors don't
    /// validate (`log_op_oracle.rs`'s order: the base, the parameters, the linear slope, the
    /// direction).
    fn spec(&self) -> Value {
        let log = |class: &str, args: Value, base: f64, params: Option<&Affine>| {
            let mut calls = vec![json!(["setBase", base])];
            if let Some(params) = params {
                for ((setter, _, _), values) in SETTERS.iter().zip(params) {
                    calls.push(json!([setter, values]));
                }
            }
            (class.to_string(), args, calls)
        };
        let ((class, args, mut calls), dir) = match self {
            T::Log(base, dir) => (log("LogTransform", json!({}), *base, None), *dir),
            T::Affine(base, p, dir) => (log("LogAffineTransform", json!({}), *base, Some(p)), *dir),
            T::Camera(base, p, brk, _, dir) => (
                log(
                    "LogCameraTransform",
                    json!({"linSideBreak": brk}),
                    *base,
                    Some(p),
                ),
                *dir,
            ),
            T::Scale(s) => {
                return json!({"class": "MatrixTransform", "args": {"matrix": scale(*s)}});
            }
            T::Range => {
                return json!({"class": "RangeTransform", "args": {
                    "minInValue": RANGE[0], "maxInValue": RANGE[1],
                    "minOutValue": RANGE[2], "maxOutValue": RANGE[3],
                }});
            }
        };
        if let T::Camera(_, _, _, Some(slope), _) = self {
            calls.push(json!(["setLinearSlopeValue", slope]));
        }
        let dir = match dir {
            F => "TRANSFORM_DIR_FORWARD",
            I => "TRANSFORM_DIR_INVERSE",
        };
        calls.push(json!(["setDirection", {"enum": dir}]));
        json!({"class": class, "args": args, "calls": calls})
    }

    /// The config YAML.
    fn yaml(&self) -> String {
        let dir = |dir: &TransformDirection| match dir {
            F => "forward",
            I => "inverse",
        };
        let params = |p: &Affine| {
            SETTERS
                .iter()
                .zip(p)
                .map(|((_, key, _), values)| format!("{key}: {}, ", yaml_list(values)))
                .collect::<String>()
        };
        match self {
            T::Log(base, d) => format!(
                "!<LogTransform> {{base: {}, direction: {}}}",
                yaml_number(*base),
                dir(d)
            ),
            T::Affine(base, p, d) => format!(
                "!<LogAffineTransform> {{base: {}, {}direction: {}}}",
                yaml_number(*base),
                params(p),
                dir(d)
            ),
            T::Camera(base, p, brk, slope, d) => format!(
                "!<LogCameraTransform> {{base: {}, {}lin_side_break: {}, {}direction: {}}}",
                yaml_number(*base),
                params(p),
                yaml_list(brk),
                slope
                    .map(|s| format!("linear_slope: {}, ", yaml_list(&s)))
                    .unwrap_or_default(),
                dir(d)
            ),
            T::Scale(s) => format!("!<MatrixTransform> {{matrix: {}}}", yaml_list(&scale(*s))),
            T::Range => format!(
                "!<RangeTransform> {{min_in_value: {}, max_in_value: {}, min_out_value: {}, \
                 max_out_value: {}}}",
                RANGE[0], RANGE[1], RANGE[2], RANGE[3]
            ),
        }
    }

    /// The port's data of a Log transform, as the transform builds it: `m_data(2.0f,
    /// TRANSFORM_DIR_FORWARD)`, the break for a camera log, then the setters.
    fn log_data(&self) -> LogOpData {
        let mut data = LogOpData::new(f64::from(2.0f32), F);
        let (base, params, dir) = match self {
            T::Log(base, dir) => (*base, None, *dir),
            T::Affine(base, p, dir) => (*base, Some(p), *dir),
            T::Camera(base, p, brk, _, dir) => {
                data.set_value(LogAffineParameter::LinSideBreak, brk)
                    .unwrap();
                (*base, Some(p), *dir)
            }
            T::Scale(_) | T::Range => unreachable!("a log"),
        };
        data.set_base(base);
        if let Some(params) = params {
            for ((_, _, param), values) in SETTERS.iter().zip(params) {
                data.set_value(*param, values).unwrap();
            }
        }
        if let T::Camera(_, _, _, Some(slope), _) = self {
            data.set_value(LogAffineParameter::LinearSlope, slope)
                .unwrap();
        }
        data.set_direction(dir);
        data
    }

    /// The transform's validation prefix (`LogTransformImpl::validate` and the others).
    fn prefix(&self) -> Option<&'static str> {
        match self {
            T::Log(..) => Some("LogTransform validation failed: "),
            T::Affine(..) => Some("LogAffineTransform validation failed: "),
            T::Camera(..) => Some("LogCameraTransform validation failed: "),
            T::Scale(_) | T::Range => None,
        }
    }
}

/// The transform an upstream case builds: the class's defaults (base 2, the default affine
/// parameters) where the test sets nothing.
fn upstream_transform(case: &LogCase) -> T {
    let dir = if case.inverse { I } else { F };
    let base = case.base.unwrap_or(2.0);
    let p = case.params;
    let affine = [
        p.log_side_slope.unwrap_or(DEFAULTS[0]),
        p.log_side_offset.unwrap_or(DEFAULTS[1]),
        p.lin_side_slope.unwrap_or(DEFAULTS[2]),
        p.lin_side_offset.unwrap_or(DEFAULTS[3]),
    ];
    match case.group {
        "LogTransform" => T::Log(base, dir),
        "LogAffineTransform" => T::Affine(base, affine, dir),
        "LogCameraTransform" => T::Camera(
            base,
            affine,
            case.lin_side_break.unwrap(),
            p.linear_slope,
            dir,
        ),
        other => panic!("{}: group {other}", case.name),
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

/// The oracle's processor for `chain`: a `GroupTransform`, as JSON, or, when a number isn't
/// finite, in a config's YAML (JSON can't hold them).
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
    /// JSON: the transforms, built empty and set, which doesn't validate; then
    /// `Processor::Impl::setTransform` validates them.
    Setters,
    /// A config's YAML, from a colour space: no transform's `validate`.
    Yaml,
}

/// The processor's ops, as the transforms build them (see the module notes), before the
/// processor finalizes them. An error carries the wheel's stage, "processor":
/// - on the JSON route, `Processor::Impl::setTransform` validates the transform (a group
///   validates each child), with the transform's prefix (Processor.cpp:633,
///   GroupTransform.cpp:56-73, LogTransform.cpp, LogAffineTransform.cpp and
///   LogCameraTransform.cpp's `validate`), as `log_op_oracle.rs`'s `port_processor` does;
/// - `BuildLogOp` validates the data, without a prefix (LogOp.cpp:190-221).
fn raw_ops(chain: &[T], route: Route) -> Result<OpVec, String> {
    if route == Route::Setters {
        for t in chain {
            if let Some(prefix) = t.prefix() {
                let data = t.log_data();
                let mut valid = data.validate();
                if valid.is_ok() && matches!(t, T::Camera(..)) && data.red_params().len() < 5 {
                    valid = Err(Exception::new("LinSideBreak has to be defined."));
                }
                valid.map_err(|e| format!("processor: {prefix}{}", e.message()))?;
            }
        }
    }
    let processor = |e: Exception| format!("processor: {}", e.message());
    let mut raw = OpVec::new();
    for t in chain {
        match t {
            T::Scale(s) => {
                let mut data = MatrixOpData::new();
                data.set_rgba(&scale(*s));
                data.validate().map_err(processor)?;
                create_matrix_op(&mut raw, data, F);
            }
            T::Range => {
                let data = RangeOpData::with_values(RANGE[0], RANGE[1], RANGE[2], RANGE[3])
                    .map_err(processor)?;
                create_range_op(&mut raw, data, F).map_err(processor)?;
            }
            log => {
                let data = log.log_data();
                data.validate().map_err(processor)?;
                let copy = data.try_clone().map_err(processor)?;
                create_log_op(&mut raw, copy, F).map_err(processor)?;
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
                "a Log shader has no uniform nor texture"
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
        Route::Setters
    } else {
        Route::Yaml
    };
    cases_with(label, chain, processor, route, flags, names)
}

/// Upstream's Log GPU tests (tests/gpu/LogOp_test.cpp @ v2.5.2, `ocio_testkit::gpu_cases`):
/// the wheel's shader for each, from `getDefaultGPUProcessor` as the tests take it and from
/// every optimization level, in every language, is the port's. (The legacy GPU processor of
/// four of them comes in Phase 7.)
#[test]
fn upstreams_gpu_tests_write_the_wheels_shaders() {
    let mut cases = Vec::new();
    let upstream = log_cases();
    assert_eq!(upstream.len(), 22);
    for case in upstream {
        let processor = json!({ "transform": case.transform() });
        cases.extend(cases_with(
            &format!("{} {}", case.group, case.name),
            vec![upstream_transform(&case)],
            processor,
            Route::Setters,
            &levels(),
            Names::default(),
        ));
    }
    let (extracted, _) = check(&cases);
    assert_eq!(extracted, cases.len());
}

/// Each of the six shapes in every language and at every level: plain logs in base 2, 10 and
/// e, affine logs in base 10 and others, camera logs with and without a linear slope, in both
/// directions; inverse pairs of each (replaced by a Range or an identity Matrix op), pairs
/// that aren't inverses, logs next to a range and a matrix; a base-2 or base-10 affine log off
/// its defaults, which is no plain log; refusals; and names (an empty pixel name is an error,
/// but in OSL).
#[test]
fn every_path_writes_the_wheels_shader() {
    let up: Affine = [[0.18; 3], [1.0; 3], [2.0; 3], [0.1; 3]];
    let unequal: Affine = [[0.5, 1.0, 1.5], [1.0, 0.9, 1.1], [2.0, 3.0, 4.0], [0.1; 3]];
    let neg_offset: Affine = [[0.25; 3], [0.5; 3], [4.0; 3], [-0.5; 3]];
    // The default parameters, but parameter `k` (`Affine`'s order) set to `value`.
    let only = |k: usize, value: f64| {
        let mut p = DEFAULTS;
        p[k] = [value; 3];
        p
    };
    // A steep camera log in base 10 with a small break.
    let steep: Affine = [[0.25; 3], [0.4; 3], [5.5; 3], [0.05; 3]];
    let mut chains: Vec<(String, Vec<T>)> = Vec::new();
    for dir in [F, I] {
        let other = match dir {
            F => I,
            I => F,
        };
        let mut push = |label: &str, chain: Vec<T>| {
            chains.push((format!("{dir:?} {label}"), chain));
        };
        for base in [2.0, 10.0, std::f64::consts::E, 3.5, 0.5] {
            push(&format!("log {base}"), vec![T::Log(base, dir)]);
            push(
                &format!("log {base} inverse pair"),
                vec![T::Log(base, dir), T::Log(base, other)],
            );
        }
        for base in [10.0, 2.0, 1.5] {
            push(&format!("affine {base}"), vec![T::Affine(base, up, dir)]);
            push(
                &format!("affine {base} unequal"),
                vec![T::Affine(base, unequal, dir)],
            );
        }
        push(
            "affine inverse pair",
            vec![T::Affine(10.0, up, dir), T::Affine(10.0, up, other)],
        );
        push(
            "affine negative offset inverse pair",
            vec![
                T::Affine(10.0, neg_offset, dir),
                T::Affine(10.0, neg_offset, other),
            ],
        );
        push(
            "affine unequal pair",
            vec![
                T::Affine(10.0, unequal, dir),
                T::Affine(10.0, unequal, other),
            ],
        );
        for k in 0..4 {
            for base in [2.0, 10.0] {
                push(
                    &format!("affine {base} only {k}"),
                    vec![T::Affine(base, only(k, 0.5), dir)],
                );
            }
        }
        push("camera", vec![cam(up, None, dir)]);
        push("camera linear slope", vec![cam(up, Some([1.5; 3]), dir)]);
        push(
            "camera unequal",
            vec![T::Camera(
                10.0,
                unequal,
                [0.1, 0.2, 0.05],
                Some([1.2, 1.3, 1.4]),
                dir,
            )],
        );
        push(
            "camera small break",
            vec![T::Camera(10.0, steep, [0.01; 3], None, dir)],
        );
        // A break whose log argument is negative: a NaN break, of each platform's sign.
        push(
            "camera negative argument",
            vec![T::Camera(2.0, only(3, -0.5), [0.1; 3], None, dir)],
        );
        push(
            "camera inverse pair",
            vec![cam(up, None, dir), cam(up, None, other)],
        );
        push(
            "camera pair, other slope",
            vec![cam(up, None, dir), cam(up, Some([1.5; 3]), other)],
        );
        push(
            "around a matrix",
            vec![
                T::Affine(10.0, up, dir),
                T::Scale(2.0),
                T::Affine(10.0, up, other),
            ],
        );
        push(
            "next to a range",
            vec![T::Range, T::Log(2.0, dir), T::Range],
        );
        push(
            "all three",
            vec![
                T::Log(10.0, dir),
                T::Affine(10.0, up, dir),
                cam(up, None, dir),
            ],
        );
    }
    chains.extend([
        // Refused by the validation, with each transform's prefix: a base of 1, a zero or
        // negative base, a zero slope on either side.
        ("base 1".to_string(), vec![T::Log(1.0, I), T::Scale(2.0)]),
        ("base 0".to_string(), vec![T::Affine(0.0, up, F)]),
        ("base -2".to_string(), vec![T::Log(-2.0, F)]),
        (
            "zero lin slope".to_string(),
            vec![T::Range, T::Affine(10.0, only(2, 0.0), F)],
        ),
        (
            "zero log slope".to_string(),
            vec![cam(only(0, 0.0), None, I)],
        ),
        // Parameters that need 7 and more digits.
        (
            "7 digits".to_string(),
            vec![T::Camera(
                1.2345678,
                [
                    [1.234567, 0.1234567, 12.34567],
                    [0.001234565, -1.2345678, 1234567.8],
                    [1.1234567, 0.98765432, 2.5],
                    [0.9876543, 1e-7, 3.3333333],
                ],
                [0.0123456789, 0.5, 1.5],
                Some([0.87654321, 1.0, 2.0]),
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
        for dir in [F, I] {
            for t in [
                T::Log(2.0, dir),
                T::Log(10.0, dir),
                T::Affine(10.0, up, dir),
                cam(up, None, dir),
            ] {
                cases.extend(cases_of("names", vec![t], &no_optimization(), names));
            }
        }
    }
    let (extracted, refused) = check(&cases);
    assert!(extracted > 0 && refused > 0);
}

/// Extreme parameters, generated: the base, each affine parameter, the break and the linear
/// slope, in turn NaN, ±Inf, the largest double, -0, the smallest denormal, 1e-9, 1e39 and
/// 65504.5 (beyond the float range and Cg's half range), for each kind of log and direction,
/// without optimization, in every language.
#[test]
fn extreme_parameters_write_the_wheels_shader() {
    let base_params: Affine = [
        [0.2, 0.3, 0.4],
        [0.7, 0.6, 0.5],
        [1.4, 1.1, 1.2],
        [0.15, 0.16, 0.25],
    ];
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
    for dir in [F, I] {
        for v in values {
            for (label, t) in [
                ("log base", T::Log(v, dir)),
                ("affine base", T::Affine(v, base_params, dir)),
                (
                    "camera base",
                    T::Camera(v, base_params, [0.12; 3], Some([1.2; 3]), dir),
                ),
            ] {
                cases.extend(cases_of(
                    &format!("{dir:?} {label} = {v:e}"),
                    vec![t],
                    &no_optimization(),
                    Names::default(),
                ));
            }
            for slot in 0..12 {
                let mut p = base_params;
                p[slot / 3][slot % 3] = v;
                for (label, t) in [
                    ("affine", T::Affine(10.0, p, dir)),
                    ("camera", T::Camera(10.0, p, [0.12; 3], None, dir)),
                ] {
                    cases.extend(cases_of(
                        &format!("{dir:?} {label} slot {slot} = {v:e}"),
                        vec![t],
                        &no_optimization(),
                        Names::default(),
                    ));
                }
            }
            for c in 0..3 {
                let mut brk = [0.12; 3];
                brk[c] = v;
                let mut slope = [1.2; 3];
                slope[c] = v;
                for (label, t) in [
                    ("camera break", T::Camera(10.0, base_params, brk, None, dir)),
                    (
                        "camera linear slope",
                        T::Camera(10.0, base_params, [0.12; 3], Some(slope), dir),
                    ),
                ] {
                    cases.extend(cases_of(
                        &format!("{dir:?} {label} {c} = {v:e}"),
                        vec![t],
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
