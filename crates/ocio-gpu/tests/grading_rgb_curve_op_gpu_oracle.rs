// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The GradingRGBCurve op's GPU writer against the wheel (`gpu_shader`), in the 10 languages:
//! for each case, the GPU processor's cache ID and queries, and what the extraction writes
//! (the shader's text, cache ID, uniforms with their values, and dynamic properties), or the
//! error, byte for byte.
//!
//! The wheel builds a `GradingRGBCurveTransform` of the curves (object specs,
//! `oracle/ocio_oracle/spec.py`) in a group, and its processor's GPU processor. The port builds
//! the op data as the binding's constructor does
//! (src/bindings/python/transforms/PyGradingRGBCurveTransform.cpp:17-34 @ v2.5.2), the ops
//! with `create_grading_rgb_curve_op`, and runs the GPU processor from them, as they are and
//! finalized first. Non-dynamic curves become constant arrays in a helper function per op;
//! dynamic ones become uniforms bound to the shader's own copy of the dynamic property (in
//! OSL, which has no dynamic properties, constants and a warning).

use ocio_gpu::gpu_processor::GpuProcessor;
use ocio_gpu::gpu_shader::UniformData;
use ocio_gpu::{GpuLanguage, GpuShaderDesc};
use std::sync::{Arc, Mutex, PoisonError};

use ocio_ops::dynamic_property::DynamicPropertyRcPtr;
use ocio_ops::logging::{get_logging_level, set_logging_function};
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{
    GradingStyle, OptimizationFlags, RgbCurveType, TransformDirection,
};
use ocio_ops::ops::gradingrgbcurve::grading_b_spline_curve::{
    GradingBSplineCurve, GradingControlPoint,
};
use ocio_ops::ops::gradingrgbcurve::grading_rgb_curve::GradingRgbCurve;
use ocio_ops::ops::gradingrgbcurve::grading_rgb_curve_op::create_grading_rgb_curve_op;
use ocio_ops::ops::gradingrgbcurve::grading_rgb_curve_op_data::GradingRgbCurveOpData;
use ocio_ops::platform::{self, MapEnv};
use ocio_testkit::gpu::{
    self as oracle_gpu, GpuShaderReply, GpuShaderRequest, ShaderSettings, UniformValue,
};
use ocio_testkit::processor_ops::Dumped;
use ocio_testkit::transform_text::f64_spec;
use ocio_testkit::{Oracle, assert_text_eq};
use serde_json::{Value, json};

/// One GradingRGBCurve op: style, direction, bypass, dynamic, and the four curves.
#[derive(Debug, Clone)]
struct Op {
    style: GradingStyle,
    inverse: bool,
    bypass: bool,
    dynamic: bool,
    curves: [Vec<(f32, f32)>; 4],
    slopes: [Vec<f32>; 4],
}

impl Op {
    fn new(style: GradingStyle, curves: [&[(f32, f32)]; 4]) -> Op {
        Op {
            style,
            inverse: false,
            bypass: false,
            dynamic: false,
            curves: curves.map(<[(f32, f32)]>::to_vec),
            slopes: curves.map(|c| vec![0.0; c.len()]),
        }
    }

    fn inverse(mut self) -> Op {
        self.inverse = true;
        self
    }

    fn bypass(mut self) -> Op {
        self.bypass = true;
        self
    }

    fn dynamic(mut self) -> Op {
        self.dynamic = true;
        self
    }

    fn slopes(mut self, c: usize, slopes: &[f32]) -> Op {
        self.slopes[c] = slopes.to_vec();
        self
    }

    fn curve(&self, c: usize) -> GradingBSplineCurve {
        let points: Vec<GradingControlPoint> = self.curves[c]
            .iter()
            .map(|&(x, y)| GradingControlPoint::new(x, y))
            .collect();
        let mut curve = GradingBSplineCurve::with_points(&points);
        for (i, &s) in self.slopes[c].iter().enumerate() {
            curve.set_slope(i, s).unwrap();
        }
        curve
    }

    fn rgb_curve(&self) -> GradingRgbCurve {
        GradingRgbCurve::with_curves(
            &self.curve(0),
            &self.curve(1),
            &self.curve(2),
            &self.curve(3),
        )
    }

    fn style_name(&self) -> &'static str {
        match self.style {
            GradingStyle::Log => "GRADING_LOG",
            GradingStyle::Lin => "GRADING_LIN",
            GradingStyle::Video => "GRADING_VIDEO",
        }
    }

    /// The transform spec.
    fn spec(&self) -> Value {
        let f = |v: f32| f64_spec(f64::from(v));
        let curve = |c: usize| {
            let values: Vec<Value> = self.curves[c]
                .iter()
                .flat_map(|&(x, y)| [f(x), f(y)])
                .collect();
            let slopes: Vec<Value> = self.slopes[c].iter().map(|&s| f(s)).collect();
            json!({"object": {"class": "GradingBSplineCurve", "args": [values],
                "calls": [["setSlopes", slopes]]}})
        };
        let dir = if self.inverse {
            "TRANSFORM_DIR_INVERSE"
        } else {
            "TRANSFORM_DIR_FORWARD"
        };
        json!({"class": "GradingRGBCurveTransform",
            "args": {
                "values": {"object": {"class": "GradingRGBCurve", "args": {
                    "red": curve(0), "green": curve(1), "blue": curve(2), "master": curve(3),
                }}},
                "style": {"enum": self.style_name()},
                "dynamic": self.dynamic,
                "dir": {"enum": dir},
            },
            "calls": [["setBypassLinToLog", self.bypass]],
        })
    }

    /// The op data, as the binding's constructor and `setBypassLinToLog` make it.
    fn data(&self) -> ocio_ops::Result<GradingRgbCurveOpData> {
        let mut data = GradingRgbCurveOpData::new(self.style);
        data.set_value(&self.rgb_curve())?;
        if self.dynamic {
            data.get_dynamic_property_internal().make_dynamic();
        }
        data.set_direction(if self.inverse {
            TransformDirection::Inverse
        } else {
            TransformDirection::Forward
        });
        data.validate()?;
        data.set_bypass_lin_to_log(self.bypass);
        Ok(data)
    }
}

/// The optimization levels, as the oracle takes them; `None` is `getDefaultGPUProcessor`.
fn levels() -> Vec<(Option<Value>, OptimizationFlags)> {
    let name = |n: &str| Some(json!(n));
    vec![
        (None, OptimizationFlags::DEFAULT),
        (name("OPTIMIZATION_NONE"), OptimizationFlags::NONE),
        (name("OPTIMIZATION_ALL"), OptimizationFlags::ALL),
    ]
}

/// One extraction.
#[derive(Debug, Clone)]
struct Case {
    label: String,
    ops: Vec<Op>,
    flags: (Option<Value>, OptimizationFlags),
    language: GpuLanguage,
    oracle_language: oracle_gpu::GpuLanguage,
}

impl Case {
    fn request(&self) -> GpuShaderRequest {
        let children: Vec<Value> = self.ops.iter().map(Op::spec).collect();
        let mut processor = json!({"transform": {"class": "GroupTransform", "children": children}});
        if let Some(flags) = &self.flags.0 {
            processor["optimization"] = flags.clone();
        }
        GpuShaderRequest::new(processor, ShaderSettings::language(self.oracle_language))
    }
}

/// A uniform: its name, type, buffer offset and value.
type UniformRow = (String, String, u64, UniformValue);

/// An extraction: the shader's text and cache ID, its uniforms, and its dynamic properties'
/// curves.
type Extracted = (String, String, Vec<UniformRow>, Vec<Vec<u32>>);

/// What the wheel and the port give for a case.
#[derive(Debug, Clone, PartialEq)]
enum Outcome {
    Raised(String),
    Extracted {
        gpu_cache_id: String,
        is_no_op: bool,
        has_channel_crosstalk: bool,
        /// The shader's text, cache ID, uniforms, and dynamic properties' curves (each curve's
        /// points and slopes, by their bits), or the error.
        shader: Result<Extracted, String>,
        /// The writer's warnings (OSL's dynamic properties) in the log: the processors' own
        /// warnings (`validateDynamicProperties`) come from steps the port's ops skip.
        writer_log: Vec<String>,
    },
}

/// The bits of a dumped GradingRGBCurve's points and slopes, curve by curve.
fn dumped_curve_bits(value: &Dumped) -> Vec<u32> {
    let curves = value.object();
    let mut bits = Vec::new();
    for name in ["red", "green", "blue", "master"] {
        let curve = curves.property(name).object();
        if let Dumped::List(points) = curve.getter("getControlPoints") {
            for p in points {
                for c in ["x", "y"] {
                    bits.push((p.object().property(c).f64() as f32).to_bits());
                }
            }
        }
        bits.extend(
            curve
                .getter("getSlopes")
                .f64s()
                .iter()
                .map(|&s| (s as f32).to_bits()),
        );
    }
    bits
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
            let uniforms = shader
                .uniforms
                .iter()
                .map(|u| {
                    (
                        u.name.clone(),
                        u.kind.clone(),
                        u.buffer_offset,
                        u.value.clone(),
                    )
                })
                .collect();
            let dynamics = shader
                .dynamic_properties
                .iter()
                .map(|d| {
                    assert_eq!(d.kind, "DYNAMIC_PROPERTY_GRADING_RGBCURVE");
                    dumped_curve_bits(&d.value)
                })
                .collect();
            Ok((
                shader.text.clone(),
                shader.cache_id.clone(),
                uniforms,
                dynamics,
            ))
        }
    };
    Outcome::Extracted {
        writer_log: writer_log(reply.log()),
        gpu_cache_id: result["gpu_cache_id"].as_str().unwrap().to_string(),
        is_no_op: result["gpu_processor"]["isNoOp"].as_bool().unwrap(),
        has_channel_crosstalk: result["gpu_processor"]["hasChannelCrosstalk"]
            .as_bool()
            .unwrap(),
        shader,
    }
}

/// The messages of the writer's warning.
fn writer_log(log: Vec<String>) -> Vec<String> {
    log.into_iter()
        .filter(|m| m.contains("Open Shading language"))
        .collect()
}

/// What the port logged since the last call.
fn take_log() -> Vec<String> {
    std::mem::take(&mut *LOG.lock().unwrap_or_else(PoisonError::into_inner))
}

/// The port's log. The logging state is global, so the tests of this process run one at a
/// time ([`SERIAL`]).
static LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Held by each test for its whole run, so that no test logs into another's [`LOG`].
static SERIAL: Mutex<()> = Mutex::new(());

/// Sends the port's log to [`LOG`], after OCIO's one-time read of `OCIO_LOGGING_LEVEL` in an
/// empty environment, as the oracle's process reads it.
fn capture_log() {
    platform::set_env_provider(Some(Arc::new(MapEnv::default())));
    let _ = get_logging_level();
    platform::set_env_provider(None);
    set_logging_function(Some(Arc::new(|line: &[u8]| {
        LOG.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(String::from_utf8(line.to_vec()).expect("UTF-8"));
    })))
    .unwrap();
}

/// The port's uniforms, read with their getters as the oracle reads the wheel's.
fn port_uniforms(desc: &GpuShaderDesc) -> Vec<UniformRow> {
    let text = |b: &[u8]| String::from_utf8(b.to_vec()).unwrap();
    desc.uniforms()
        .iter()
        .map(|u| {
            let (kind, value) = match u.data() {
                UniformData::Double(get) => ("UNIFORM_DOUBLE", UniformValue::Double(get())),
                UniformData::Bool(get) => ("UNIFORM_BOOL", UniformValue::Bool(get())),
                UniformData::Float3(get) => ("UNIFORM_FLOAT3", UniformValue::Float3(get())),
                UniformData::VectorFloat { size, values } => (
                    "UNIFORM_VECTOR_FLOAT",
                    UniformValue::VectorFloat(values()[..size() as usize].to_vec()),
                ),
                UniformData::VectorInt { size, values } => (
                    "UNIFORM_VECTOR_INT",
                    UniformValue::VectorInt(values()[..size() as usize].to_vec()),
                ),
            };
            (
                text(u.name()),
                kind.to_string(),
                u.buffer_offset() as u64,
                value,
            )
        })
        .collect()
}

/// The bits of a curve value's points and slopes, curve by curve.
fn curve_bits(curves: &GradingRgbCurve) -> Vec<u32> {
    let mut bits = Vec::new();
    for c in RgbCurveType::CURVES {
        let curve = curves.curve(c).unwrap();
        for p in curve.control_points() {
            bits.extend([p.x.to_bits(), p.y.to_bits()]);
        }
        bits.extend(curve.slopes().iter().map(|s| s.to_bits()));
    }
    bits
}

fn port(case: &Case, finalize_first: bool) -> Outcome {
    take_log();
    let text = |b: &[u8]| String::from_utf8(b.to_vec()).unwrap();
    let mut raw = OpVec::new();
    for op in &case.ops {
        match op.data() {
            Ok(data) => create_grading_rgb_curve_op(&mut raw, data, TransformDirection::Forward),
            Err(e) => return Outcome::Raised(format!("transform: {}", e.message())),
        }
    }
    if finalize_first && let Err(e) = raw.finalize() {
        return Outcome::Raised(format!("processor: {}", e.message()));
    }
    let gpu = match GpuProcessor::new(&raw, case.flags.1) {
        Ok(gpu) => gpu,
        Err(e) => return Outcome::Raised(format!("gpu_processor: {}", e.message())),
    };
    let mut desc = GpuShaderDesc::new(case.language);
    let shader = gpu
        .extract_gpu_shader_info(&mut desc)
        .map(|()| {
            let dynamics = desc
                .dynamic_properties()
                .iter()
                .map(|p| match p {
                    DynamicPropertyRcPtr::GradingRgbCurve(p) => curve_bits(&p.get_value()),
                    other => panic!("not an RGB curve property: {other:?}"),
                })
                .collect();
            (
                text(desc.shader_text()),
                text(&desc.cache_id()),
                port_uniforms(&desc),
                dynamics,
            )
        })
        .map_err(|e| e.message().to_string());
    Outcome::Extracted {
        writer_log: writer_log(take_log()),
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
            assert_text_eq(&case.label, &w.0, &p.0);
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

/// `ops` at every level, in every language.
fn cases_of(label: &str, ops: Vec<Op>) -> Vec<Case> {
    let mut cases = Vec::new();
    for flags in levels() {
        for (language, oracle_language) in GpuLanguage::ALL
            .into_iter()
            .zip(oracle_gpu::GpuLanguage::ALL)
        {
            cases.push(Case {
                label: format!("{label} {:?} {language:?}", flags.0),
                ops: ops.clone(),
                flags: flags.clone(),
                language,
                oracle_language,
            });
        }
    }
    cases
}

/// Upstream's CPU test curves (tests/cpu/ops/gradingrgbcurve/GradingRGBCurveOpCPU_tests.cpp @
/// v2.5.2) and the ACES RRT shaper (src/OpenColorIO/transforms/builtins/ACES.cpp:178-199), in
/// every style, forward and inverse, bypassed, dynamic and not, alone and two at a time.
#[test]
fn grading_rgb_curve_shaders_match_the_wheel() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    capture_log();
    use GradingStyle::{Lin, Log, Video};
    let identity: &[(f32, f32)] = &[(0.0, 0.0), (1.0, 1.0)];
    let lin_rgb: &[(f32, f32)] = &[(-6.0, -8.0), (-2.0, -5.0), (2.0, 4.0), (5.0, 6.0)];
    let lin_m: &[(f32, f32)] = &[(0.0, 0.0), (0.5, 0.5), (1.0, 1.0)];
    let log = Op::new(
        Log,
        [
            &[(0.1, 0.15), (0.55, 0.45), (0.9, 1.1)],
            &[(0.1, 0.15), (0.55, 0.35), (0.9, 1.1)],
            &[(0.1, 0.15), (0.55, 0.85), (0.9, 1.1)],
            &[(-0.1, 0.1), (1.1, 1.3)],
        ],
    );
    let lin = Op::new(Lin, [lin_rgb, lin_rgb, lin_rgb, lin_m]);
    let rrt = Op::new(
        Log,
        [
            identity,
            identity,
            identity,
            &[
                (-5.26017743, -4.0),
                (-3.75502745, -3.57868829),
                (-2.24987747, -1.82131329),
                (-0.74472749, 0.68124124),
                (1.06145248, 2.87457742),
                (2.86763245, 3.83406206),
                (4.67381243, 4.0),
            ],
        ],
    )
    .slopes(
        3,
        &[
            0.0, 0.55982688, 1.77532247, 1.55, 0.8787017, 0.18374463, 0.0,
        ],
    );
    let default_log = Op::new(Video, [lin_m; 4]);
    let lists: Vec<(&str, Vec<Op>)> = vec![
        ("log", vec![log.clone()]),
        ("log inverse", vec![log.clone().inverse()]),
        ("lin", vec![lin.clone()]),
        ("lin inverse", vec![lin.clone().inverse()]),
        ("lin bypass", vec![lin.clone().bypass()]),
        ("lin bypass inverse", vec![lin.clone().bypass().inverse()]),
        ("ACES RRT shaper", vec![rrt.clone()]),
        ("identity", vec![default_log.clone()]),
        ("identity dynamic", vec![default_log.clone().dynamic()]),
        ("log dynamic", vec![log.clone().dynamic()]),
        ("lin dynamic inverse", vec![lin.clone().dynamic().inverse()]),
        ("two", vec![log.clone(), rrt.clone()]),
        (
            "a pair of inverses",
            vec![log.clone(), log.clone().inverse()],
        ),
        ("dynamic and not", vec![lin.clone().dynamic(), rrt.clone()]),
        (
            "two dynamic",
            vec![log.clone().dynamic(), rrt.clone().dynamic()],
        ),
    ];
    let mut cases = Vec::new();
    for (label, ops) in lists {
        cases.extend(cases_of(label, ops));
    }
    check(&cases);
}

/// Curves with a NaN y coordinate or slope, where the fitting's NaNs meet in each wheel's
/// operand order (`docs/improvements.md` I-91): NaN y coordinates first, in the middle and
/// last, quiet NaNs of both signs and with a payload, beside repeated x coordinates (two
/// points at the same x give the fitting infinite and default-NaN coefficients) and infinite
/// values; NaN slopes beside finite and infinite ones; and the curves a random sweep found
/// differing before the fitting took each wheel's order. Each master curve is in a log-style
/// op, forward and inverse, dynamic (the coefficients are uniforms) and not (constants), at
/// every level, in every language.
#[test]
fn nan_curves_shaders_match_the_wheel() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    capture_log();
    use GradingStyle::{Lin, Log, Video};
    let n = f32::NAN;
    let neg_payload = f32::from_bits(0xffc1_2345);
    let payload = f32::from_bits(0x7fd0_0001);
    let inf = f32::INFINITY;
    let identity: &[(f32, f32)] = &[(0.0, 0.0), (1.0, 1.0)];
    let master = |points: &[(f32, f32)], slopes: Option<&[f32]>| {
        let op = Op::new(Log, [identity, identity, identity, points]);
        match slopes {
            Some(s) => op.slopes(3, s),
            None => op,
        }
    };
    let masters: Vec<(&str, Op)> = vec![
        (
            "NaN y, x repeated",
            master(&[(0.0, 0.0), (1.0, n), (2.0, 2.0), (2.0, 3.0)], None),
        ),
        (
            "NaN y",
            master(&[(0.0, 0.0), (1.0, n), (2.0, 2.0), (3.0, 3.0)], None),
        ),
        (
            "NaN y first",
            master(&[(0.0, n), (1.0, 1.0), (2.0, 2.0)], None),
        ),
        (
            "NaN y last",
            master(&[(0.0, 0.0), (1.0, 1.0), (2.0, n)], None),
        ),
        (
            "negative NaN y with a payload",
            master(
                &[(0.0, 0.0), (1.0, neg_payload), (2.0, 2.0), (3.0, 3.0)],
                None,
            ),
        ),
        (
            "NaN y with a payload, x repeated first",
            master(&[(0.0, 0.0), (0.0, 0.5), (1.0, payload), (2.0, 2.0)], None),
        ),
        (
            "NaN y and +Inf y",
            master(&[(0.0, 0.0), (1.0, n), (2.0, inf)], None),
        ),
        (
            "NaN first slope",
            master(&[(0.0, 0.0), (1.0, 1.0), (2.0, 2.0)], Some(&[n, 1.0, 1.0])),
        ),
        (
            "NaN middle slope",
            master(&[(0.0, 0.0), (1.0, 1.5), (2.0, 2.0)], Some(&[1.0, n, 1.0])),
        ),
        (
            "NaN slope after +Inf",
            master(&[(0.0, 0.0), (1.0, 1.5), (2.0, 2.0)], Some(&[inf, n, 1.0])),
        ),
        (
            "negative NaN slope, x repeated",
            master(
                &[(0.0, 0.0), (1.0, 0.5), (1.0, 1.5), (2.0, 2.0)],
                Some(&[1.0, neg_payload, 1.0, 1.0]),
            ),
        ),
    ];
    let mut cases = Vec::new();
    for (label, op) in masters {
        for (variant, op) in [
            ("", op.clone()),
            (" inverse", op.clone().inverse()),
            (" dynamic", op.clone().dynamic()),
            (" dynamic inverse", op.dynamic().inverse()),
        ] {
            cases.extend(cases_of(&format!("{label}{variant}"), vec![op]));
        }
    }
    // The sweep's curves (the verifier's random GPU probe of card p2-rgbcurve).
    let rgb = |red: &[(f32, f32)], green: &[(f32, f32)], blue: &[(f32, f32)], m: &[(f32, f32)]| {
        [red.to_vec(), green.to_vec(), blue.to_vec(), m.to_vec()]
    };
    let id = identity;
    let swept = [
        Op {
            style: Lin,
            inverse: true,
            bypass: false,
            dynamic: true,
            curves: rgb(&[(-0.4203682, n), (1.2377115, 1.8922781)], id, id, id),
            slopes: [vec![n, 2.9866364], vec![0.0; 2], vec![0.0; 2], vec![0.0; 2]],
        },
        Op {
            style: Video,
            inverse: false,
            bypass: false,
            dynamic: true,
            curves: rgb(
                id,
                id,
                id,
                &[
                    (-0.34862792, n),
                    (0.6080862, 1.3145257),
                    (1.1321678, 1.3369833),
                ],
            ),
            slopes: [
                vec![0.0; 2],
                vec![0.0; 2],
                vec![0.0; 2],
                vec![n, 1.6824479, 2.2533407],
            ],
        },
        Op {
            style: Log,
            inverse: false,
            bypass: false,
            dynamic: false,
            curves: rgb(
                id,
                &[
                    (-0.8917401, -0.57419956),
                    (-0.8917401, 0.5),
                    (-0.8917401, 1.3048507),
                    (-0.8917401, 1.5977271),
                    (0.87305605, 3.1626081),
                    (1.1617106, 4.247768),
                ],
                &[(-0.8532873, -0.64455146), (-0.8532873, 0.94803864)],
                &[
                    (-0.6654385, -0.91152686),
                    (1.028795, -0.91152686),
                    (2.859833, n),
                ],
            ),
            slopes: [
                vec![0.0; 2],
                vec![0.0; 6],
                vec![n, 0.0],
                vec![3.4028235e38, 0.0, 0.0],
            ],
        },
    ];
    for (i, op) in swept.into_iter().enumerate() {
        cases.extend(cases_of(&format!("swept {i}"), vec![op]));
    }
    check(&cases);
}
