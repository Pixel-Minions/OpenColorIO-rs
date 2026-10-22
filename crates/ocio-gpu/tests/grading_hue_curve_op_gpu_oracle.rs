// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The GradingHueCurve op's GPU writer against the wheel, in the 10 languages:
//! - `gpu_shader`: for each case, the GPU processor's cache ID and queries, and what the
//!   extraction writes (the shader's text, cache ID, uniforms with their values, and dynamic
//!   properties with their values), or the error, byte for byte;
//! - `dynamic_apply`: the uniforms' values after each value set on the description's dynamic
//!   property (O5.1);
//! - `shader_property_is_decoupled_from_the_op`: a set on the op data's property after the
//!   shader program is written leaves the uniforms as they were (a unit-level test of
//!   `get_grading_hue_curve_gpu_shader_program`: the GPU processor clones its ops).
//!
//! The wheel builds `GradingHueCurveTransform`s in a group, and its processor's GPU processor.
//! The port builds the op data as the binding's constructor does
//! (src/bindings/python/transforms/PyGradingHueCurveTransform.cpp:17-34 @ v2.5.2) followed by
//! the spec's `setRGBToHSY`, the ops with `create_grading_hue_curve_op`
//! (`BuildGradingHueCurveOp`, src/OpenColorIO/ops/gradinghuecurve/GradingHueCurveOp.cpp:
//! 234-245), and runs the GPU processor from them, as they are and finalized first. A dynamic
//! op's curves become uniforms bound to the shader's own copy of the property (in OSL, which
//! has no dynamic properties, constants and a warning).

use ocio_gpu::gpu_processor::GpuProcessor;
use ocio_gpu::gpu_shader::UniformData;
use ocio_gpu::ops::gradinghuecurve::grading_hue_curve_op_gpu::get_grading_hue_curve_gpu_shader_program;
use ocio_gpu::{GpuLanguage, GpuShaderDesc};
use std::sync::{Arc, Mutex, PoisonError};

use ocio_ops::dynamic_property::DynamicPropertyRcPtr;
use ocio_ops::logging::{get_logging_level, set_logging_function};
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{
    BSplineType, GradingStyle, HsyTransformStyle, HueCurveType, OptimizationFlags,
    TransformDirection,
};
use ocio_ops::ops::gradinghuecurve::grading_hue_curve::GradingHueCurve;
use ocio_ops::ops::gradinghuecurve::grading_hue_curve_op::create_grading_hue_curve_op;
use ocio_ops::ops::gradinghuecurve::grading_hue_curve_op_data::GradingHueCurveOpData;
use ocio_ops::ops::gradingrgbcurve::grading_b_spline_curve::{
    GradingBSplineCurve, GradingControlPoint,
};
use ocio_ops::platform::{self, MapEnv};
use ocio_testkit::gpu::{
    self as oracle_gpu, GpuShaderReply, GpuShaderRequest, ShaderSettings, UniformValue,
};
use ocio_testkit::processor_ops::{Dump, Dumped};
use ocio_testkit::transform_text::f64_spec;
use ocio_testkit::{Oracle, assert_text_eq};
use serde_json::{Value, json};

/// The curves' names, as the binding's properties name them.
const NAMES: [&str; 8] = [
    "hue_hue", "hue_sat", "hue_lum", "lum_sat", "sat_sat", "lum_lum", "sat_lum", "hue_fx",
];

const STYLES: [GradingStyle; 3] = [GradingStyle::Log, GradingStyle::Lin, GradingStyle::Video];

fn spline_name(t: BSplineType) -> &'static str {
    match t {
        BSplineType::BSpline => "B_SPLINE",
        BSplineType::DiagonalBSpline => "DIAGONAL_B_SPLINE",
        BSplineType::HueHueBSpline => "HUE_HUE_B_SPLINE",
        BSplineType::Periodic1BSpline => "PERIODIC_1_B_SPLINE",
        BSplineType::Periodic0BSpline => "PERIODIC_0_B_SPLINE",
        BSplineType::Horizontal1BSpline => "HORIZONTAL1_B_SPLINE",
    }
}

fn spline_of(name: &str) -> BSplineType {
    match name {
        "B_SPLINE" => BSplineType::BSpline,
        "DIAGONAL_B_SPLINE" => BSplineType::DiagonalBSpline,
        "HUE_HUE_B_SPLINE" => BSplineType::HueHueBSpline,
        "PERIODIC_1_B_SPLINE" => BSplineType::Periodic1BSpline,
        "PERIODIC_0_B_SPLINE" => BSplineType::Periodic0BSpline,
        "HORIZONTAL1_B_SPLINE" => BSplineType::Horizontal1BSpline,
        other => panic!("spline type {other}"),
    }
}

/// A float as a spec value, by its bits.
fn f32_spec(v: f32) -> Value {
    f64_spec(f64::from(v))
}

fn curve_spec(curve: &GradingBSplineCurve) -> Value {
    let values: Vec<Value> = curve
        .control_points()
        .iter()
        .flat_map(|p| [f32_spec(p.x), f32_spec(p.y)])
        .collect();
    let slopes: Vec<Value> = curve.slopes().iter().map(|&s| f32_spec(s)).collect();
    json!({"object": {"class": "GradingBSplineCurve", "args": [values], "calls": [
        ["setSlopes", slopes],
        ["setSplineType", {"enum": spline_name(curve.spline_type())}],
    ]}})
}

/// The curves' object spec: the style's defaults, each curve copied in through its property
/// (which validates nothing), then `setDrawCurveOnly`.
fn hue_curve_spec(curves: &GradingHueCurve) -> Value {
    let attrs: Vec<Value> = NAMES
        .iter()
        .zip(curves.curves())
        .map(|(name, curve)| json!([name, curve_spec(curve)]))
        .collect();
    json!({"object": {"class": "GradingHueCurve", "args": [{"enum": "GRADING_LOG"}],
                      "attrs": attrs,
                      "calls": [["setDrawCurveOnly", curves.get_draw_curve_only()]]}})
}

/// The bits of every curve's points, slopes and spline type, and the draw flag.
fn bits(curves: &GradingHueCurve) -> Vec<u64> {
    let mut out = Vec::new();
    for c in curves.curves() {
        out.push(c.spline_type() as u64);
        out.extend(
            c.control_points()
                .iter()
                .flat_map(|p| [u64::from(p.x.to_bits()), u64::from(p.y.to_bits())]),
        );
        out.extend(c.slopes().iter().map(|s| u64::from(s.to_bits())));
    }
    out.push(u64::from(curves.get_draw_curve_only()));
    out
}

/// The wheel's GradingHueCurve, written out, as the port's.
fn curves_of(dump: &Dump) -> GradingHueCurve {
    let mut curves = GradingHueCurve::new(GradingStyle::Log);
    for (c, name) in HueCurveType::CURVES.into_iter().zip(NAMES) {
        let d = dump.property(name).object();
        let Dumped::List(points) = d.getter("getControlPoints") else {
            panic!("no control points: {d:?}");
        };
        let points: Vec<GradingControlPoint> = points
            .iter()
            .map(|p| {
                let p = p.object();
                GradingControlPoint::new(p.property("x").f64() as f32, p.property("y").f64() as f32)
            })
            .collect();
        let mut curve = GradingBSplineCurve::with_points_and_spline_type(
            &points,
            spline_of(d.getter("getSplineType").name()),
        );
        for (i, s) in d.getter("getSlopes").f64s().into_iter().enumerate() {
            curve.set_slope(i, s as f32).unwrap();
        }
        *curves.curve_mut(c).unwrap() = curve;
    }
    curves.set_draw_curve_only(matches!(
        dump.getter("getDrawCurveOnly"),
        Dumped::Bool(true)
    ));
    curves
}

/// Curves from points of each of the eight curves, of their types' spline types.
fn curves(points: [&[(f32, f32)]; 8]) -> GradingHueCurve {
    let mut curves = GradingHueCurve::new(GradingStyle::Log);
    for (c, pts) in HueCurveType::CURVES.into_iter().zip(points) {
        let pts: Vec<GradingControlPoint> = pts
            .iter()
            .map(|&(x, y)| GradingControlPoint::new(x, y))
            .collect();
        *curves.curve_mut(c).unwrap() =
            GradingBSplineCurve::with_points_and_hue_curve_type(&pts, c);
    }
    curves
}

/// Upstream's `log_all_curves` curves (tests/cpu/ops/gradinghuecurve/
/// GradingHueCurveOpCPU_tests.cpp:209-233 @ v2.5.2), with `lin_all_curves`'s lum curves for
/// the linear style (297-303).
fn all_curves(style: GradingStyle) -> GradingHueCurve {
    let lin = style == GradingStyle::Lin;
    let ls: &[(f32, f32)] = if lin {
        &[
            (-6.0, 0.9),
            (-3.0, 0.8),
            (0.0, 1.2),
            (2.0, 1.0),
            (4.0, 0.6),
            (6.0, 0.55),
        ]
    } else {
        &[(0.05, 1.5), (0.5, 0.9), (1.1, 1.4)]
    };
    let ll: &[(f32, f32)] = if lin {
        &[(-8.0, -7.0), (-2.0, -3.0), (2.0, 3.5), (8.0, 7.0)]
    } else {
        &[(-0.02, -0.04), (0.2, 0.1), (0.8, 0.95), (1.1, 1.2)]
    };
    curves([
        &[
            (0.05, 0.15),
            (0.2, 0.3),
            (0.35, 0.4),
            (0.45, 0.45),
            (0.6, 0.7),
            (0.8, 0.85),
        ],
        &[
            (-0.1, 1.2),
            (0.2, 0.7),
            (0.4, 1.5),
            (0.5, 0.5),
            (0.6, 1.4),
            (0.8, 0.7),
        ],
        &[(0.1, 1.5), (0.2, 0.7), (0.4, 1.4), (0.5, 0.8), (0.8, 0.5)],
        ls,
        &[(0.0, 0.1), (0.5, 0.45), (1.0, 1.1)],
        ll,
        &[(0.0, 1.2), (0.6, 0.8), (0.9, 1.1)],
        &[
            (0.2, 0.05),
            (0.4, -0.09),
            (0.6, -0.2),
            (0.8, 0.05),
            (0.99, -0.02),
        ],
    ])
}

/// The curves with one replaced.
fn with(mut c: GradingHueCurve, t: HueCurveType, pts: &[(f32, f32)]) -> GradingHueCurve {
    let pts: Vec<GradingControlPoint> = pts
        .iter()
        .map(|&(x, y)| GradingControlPoint::new(x, y))
        .collect();
    *c.curve_mut(t).unwrap() = GradingBSplineCurve::with_points_and_hue_curve_type(&pts, t);
    c
}

/// Values whose text takes every digit, infinite and huge ones, NaN slopes.
fn extremes(style: GradingStyle) -> GradingHueCurve {
    let mut c = all_curves(style);
    c = with(
        c,
        HueCurveType::HueLum,
        &[(0.1, 1.0), (0.3, 3e38), (0.6, f32::INFINITY)],
    );
    c = with(
        c,
        HueCurveType::HueFx,
        &[
            (0.1, 0.6),
            (0.3, -0.7),
            (0.5, 1.4),
            (0.7, -1.6),
            (0.95, 0.2),
        ],
    );
    c = with(
        c,
        HueCurveType::SatLum,
        &[
            (1.0 / 3.0, 0.123456789),
            (0.7654321, 1.2345678),
            (1.9999999, 1e-30),
        ],
    );
    c.curve_mut(HueCurveType::HueSat)
        .unwrap()
        .set_slope(2, f32::NAN)
        .unwrap();
    c
}

/// The default curves of `style` with the hue-sat curve's second point moved, drawn
/// (upstream's `draw_curve_only` test, GradingHueCurveOpCPU_tests.cpp:346-358).
fn drawn(style: GradingStyle) -> GradingHueCurve {
    let mut c = GradingHueCurve::new(style);
    *c.curve_mut(HueCurveType::HueSat)
        .unwrap()
        .control_point_mut(1)
        .unwrap() = GradingControlPoint::new(0.15, 1.4);
    c.set_draw_curve_only(true);
    c
}

/// The default curves, drawn: an identity (I-272).
fn drawn_defaults(style: GradingStyle) -> GradingHueCurve {
    let mut c = GradingHueCurve::new(style);
    c.set_draw_curve_only(true);
    c
}

/// One GradingHueCurve transform.
#[derive(Debug, Clone)]
struct Op {
    style: GradingStyle,
    inverse: bool,
    curves: GradingHueCurve,
    dynamic: bool,
    rgb_to_hsy: HsyTransformStyle,
}

impl Op {
    fn new(style: GradingStyle, curves: GradingHueCurve) -> Op {
        Op {
            style,
            inverse: false,
            curves,
            dynamic: false,
            rgb_to_hsy: HsyTransformStyle::Transform1,
        }
    }

    fn inverse(mut self) -> Op {
        self.inverse = true;
        self
    }

    fn dynamic(mut self) -> Op {
        self.dynamic = true;
        self
    }

    fn no_hsy(mut self) -> Op {
        self.rgb_to_hsy = HsyTransformStyle::None;
        self
    }

    fn direction(&self) -> TransformDirection {
        if self.inverse {
            TransformDirection::Inverse
        } else {
            TransformDirection::Forward
        }
    }

    fn spec(&self) -> Value {
        let style = match self.style {
            GradingStyle::Log => "GRADING_LOG",
            GradingStyle::Lin => "GRADING_LIN",
            GradingStyle::Video => "GRADING_VIDEO",
        };
        let dir = if self.inverse {
            "TRANSFORM_DIR_INVERSE"
        } else {
            "TRANSFORM_DIR_FORWARD"
        };
        let hsy = match self.rgb_to_hsy {
            HsyTransformStyle::None => "HSY_TRANSFORM_NONE",
            HsyTransformStyle::Transform1 => "HSY_TRANSFORM_1",
        };
        json!({"class": "GradingHueCurveTransform",
            "args": {
                "values": hue_curve_spec(&self.curves),
                "style": {"enum": style},
                "dynamic": self.dynamic,
                "dir": {"enum": dir},
            },
            "calls": [["setRGBToHSY", {"enum": hsy}]],
        })
    }

    /// The op data as the binding's constructor makes it, then `setRGBToHSY`, then
    /// `BuildGradingHueCurveOp`'s copy.
    fn data(&self) -> GradingHueCurveOpData {
        let mut data = GradingHueCurveOpData::new(self.style);
        data.set_value(&self.curves).unwrap();
        if self.dynamic {
            data.get_dynamic_property_internal().make_dynamic();
        }
        data.set_direction(self.direction());
        data.validate().unwrap();
        data.set_rgb_to_hsy(self.rgb_to_hsy);
        data.clone()
    }
}

/// The optimization levels, as the oracle takes them; `None` is `getDefaultGPUProcessor`.
fn levels() -> Vec<(Option<Value>, OptimizationFlags)> {
    let name = |n: &str| Some(json!(n));
    vec![
        (None, OptimizationFlags::DEFAULT),
        (name("OPTIMIZATION_NONE"), OptimizationFlags::NONE),
        (name("OPTIMIZATION_LOSSLESS"), OptimizationFlags::LOSSLESS),
        (name("OPTIMIZATION_VERY_GOOD"), OptimizationFlags::VERY_GOOD),
        (name("OPTIMIZATION_GOOD"), OptimizationFlags::GOOD),
        (name("OPTIMIZATION_DRAFT"), OptimizationFlags::DRAFT),
        (name("OPTIMIZATION_DEFAULT"), OptimizationFlags::DEFAULT),
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
    fn processor(&self) -> Value {
        let children: Vec<Value> = self.ops.iter().map(Op::spec).collect();
        json!({"transform": {"class": "GroupTransform", "children": children}})
    }

    fn request(&self) -> GpuShaderRequest {
        let mut processor = self.processor();
        if let Some(flags) = &self.flags.0 {
            processor["optimization"] = flags.clone();
        }
        GpuShaderRequest::new(processor, ShaderSettings::language(self.oracle_language))
    }

    /// The port's GPU processor of the ops, as they are or finalized first.
    fn gpu(&self, finalize_first: bool) -> Result<GpuProcessor, String> {
        let mut raw = OpVec::new();
        for op in &self.ops {
            create_grading_hue_curve_op(&mut raw, op.data(), TransformDirection::Forward);
        }
        if finalize_first && let Err(e) = raw.finalize() {
            return Err(format!("processor: {}", e.message()));
        }
        GpuProcessor::new(&raw, self.flags.1).map_err(|e| format!("gpu_processor: {}", e.message()))
    }
}

/// A uniform: its name, type, buffer offset and value.
type UniformRow = (String, String, u64, UniformValue);

/// A dynamic property: its type name and its values' bits.
type DynamicRow = (String, Vec<u64>);

/// An extraction: the shader's text and cache ID, its uniforms, and its dynamic properties.
type Extracted = (String, String, Vec<UniformRow>, Vec<DynamicRow>);

/// What the wheel and the port give for a case.
#[derive(Debug, Clone, PartialEq)]
enum Outcome {
    Raised(String),
    Extracted {
        gpu_cache_id: String,
        is_no_op: bool,
        has_channel_crosstalk: bool,
        shader: Result<Extracted, String>,
        /// The writer's warnings (OSL's dynamic properties) in the log.
        writer_log: Vec<String>,
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
                .map(|d| (d.kind.clone(), bits(&curves_of(d.value.object()))))
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

fn port(case: &Case, finalize_first: bool) -> Outcome {
    take_log();
    let text = |b: &[u8]| String::from_utf8(b.to_vec()).unwrap();
    let gpu = match case.gpu(finalize_first) {
        Ok(gpu) => gpu,
        Err(e) => return Outcome::Raised(e),
    };
    let mut desc = GpuShaderDesc::new(case.language);
    let shader = gpu
        .extract_gpu_shader_info(&mut desc)
        .map(|()| {
            let dynamics = desc
                .dynamic_properties()
                .iter()
                .map(|p| match p {
                    DynamicPropertyRcPtr::GradingHueCurve(p) => (
                        "DYNAMIC_PROPERTY_GRADING_HUECURVE".to_string(),
                        bits(&p.get_value()),
                    ),
                    other => panic!("not a grading hue curve property: {other:?}"),
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
    let (mut extracted, mut with_uniforms, mut empty, mut drawn) = (0, 0, 0, 0);
    for (case, response) in cases.iter().zip(Oracle::get().batch(&calls, true)) {
        let reply = GpuShaderReply::from_response(
            response.unwrap_or_else(|e| panic!("{}: the oracle failed: {e}", case.label)),
        );
        let wheel = wheel(&reply);
        if let Outcome::Extracted { shader: Ok(s), .. } = &wheel {
            extracted += 1;
            if !s.2.is_empty() {
                with_uniforms += 1;
            }
            if !s.0.contains("GradingHueCurve") {
                empty += 1;
            }
            if s.0.contains("(1, outColor.b, 1.);") {
                drawn += 1;
            }
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
    assert!(with_uniforms > 0, "no case has uniforms");
    assert!(empty > 0, "no case writes nothing (an identity)");
    assert!(drawn > 0, "no case draws its curves");
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

/// Every style, forward and inverse, fixed and dynamic, with and without the HSY conversion,
/// at the defaults (an identity), drawn defaults (I-272), upstream's curves, drawn curves and
/// extreme values; two dynamic ops of a style in one group (the second can't add its
/// property), dynamic and not, a pair of inverses; every level, in every language.
#[test]
fn grading_hue_curve_shaders_match_the_wheel() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    capture_log();
    let mut lists: Vec<(String, Vec<Op>)> = Vec::new();
    for style in STYLES {
        let s = format!("{style:?}");
        for (vlabel, values) in [
            ("defaults", GradingHueCurve::new(style)),
            ("drawn defaults", drawn_defaults(style)),
            ("upstream", all_curves(style)),
            ("drawn", drawn(style)),
            ("extremes", extremes(style)),
        ] {
            for inverse in [false, true] {
                for dynamic in [false, true] {
                    for hsy in [true, false] {
                        if !hsy && vlabel != "upstream" {
                            continue;
                        }
                        let mut op = Op::new(style, values.clone());
                        if inverse {
                            op = op.inverse();
                        }
                        if dynamic {
                            op = op.dynamic();
                        }
                        if !hsy {
                            op = op.no_hsy();
                        }
                        let label = format!(
                            "{s} {vlabel}{}{}{}",
                            if dynamic { " dynamic" } else { "" },
                            if inverse { " inverse" } else { "" },
                            if hsy { "" } else { " no HSY" }
                        );
                        lists.push((label, vec![op]));
                    }
                }
            }
        }
        let a = Op::new(style, all_curves(style));
        let b = Op::new(style, extremes(style));
        lists.push((
            format!("{s} two dynamic ops"),
            vec![a.clone().dynamic(), b.clone().dynamic().inverse()],
        ));
        lists.push((
            format!("{s} dynamic and not"),
            vec![a.clone().dynamic(), b.clone()],
        ));
        lists.push((
            format!("{s} a pair of inverses"),
            vec![a.clone(), a.clone().inverse()],
        ));
    }
    lists.push((
        "every style, dynamic".to_string(),
        STYLES
            .iter()
            .map(|&s| Op::new(s, all_curves(s)).dynamic())
            .collect(),
    ));
    let mut cases = Vec::new();
    for (label, ops) in lists {
        cases.extend(cases_of(&label, ops));
    }
    check(&cases);
}

/// A uniform as the oracle's `dynamic_apply` writes it: its name and value (floats by their
/// bits).
fn uniform_json(name: &str, value: &UniformValue) -> Value {
    match value {
        UniformValue::Bool(v) => json!([name, v]),
        UniformValue::VectorFloat(v) => {
            json!([name, v.iter().map(|x| x.to_bits()).collect::<Vec<_>>()])
        }
        UniformValue::VectorInt(v) => json!([name, v]),
        other => panic!("not a GradingHueCurve uniform: {other:?}"),
    }
}

/// The uniforms read the values set on the description's dynamic property: for each style and
/// direction, `dynamic_apply` takes the property from the GPU description, sets curves one
/// after another (upstream's, the defaults, which bypass, drawn ones, extreme ones, one the
/// setter refuses) and reads the uniforms after each set; the port sets the same curves on its
/// description's property and reads its uniforms.
#[test]
fn uniforms_read_the_values_set() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let mut failures = Vec::new();
    let mut compared = 0;
    let mut refused_count = 0;
    let mut changed = 0;
    for style in STYLES {
        let mut refused = all_curves(style);
        refused
            .curve_mut(HueCurveType::SatSat)
            .unwrap()
            .control_point_mut(1)
            .unwrap()
            .x = -1.0;
        let sets = [
            extremes(style),
            GradingHueCurve::new(style),
            drawn(style),
            refused,
            drawn_defaults(style),
            all_curves(style),
        ];
        for inverse in [false, true] {
            let mut op = Op::new(style, all_curves(style)).dynamic();
            if inverse {
                op = op.inverse();
            }
            let case = Case {
                label: format!("{style:?} inverse {inverse}"),
                ops: vec![op],
                flags: (None, OptimizationFlags::DEFAULT),
                language: GpuLanguage::Glsl4_0,
                oracle_language: oracle_gpu::GpuLanguage::Glsl40,
            };
            let mut steps = vec![
                json!({"get": "DYNAMIC_PROPERTY_GRADING_HUECURVE", "from": "gpu", "as": "p"}),
                json!({"uniforms": true}),
            ];
            for v in &sets {
                steps.push(json!({"set": "p", "value": hue_curve_spec(v)}));
                steps.push(json!({"uniforms": true}));
            }
            let mut args = case.processor();
            args["gpu"] = json!({"shader": {"language": "GPU_LANGUAGE_GLSL_4_0"}});
            args["buffers"] = json!([]);
            args["images"] = json!([]);
            args["steps"] = json!(steps);
            let response = Oracle::get().call("dynamic_apply", args, &[]);
            let result = &response.result;
            assert!(result.get("exception").is_none(), "{result}");
            let wheel_steps = result["steps"].as_array().unwrap();
            assert_eq!(wheel_steps.len(), 2 + 2 * sets.len(), "{result}");

            let gpu = case.gpu(false).unwrap();
            let mut desc = GpuShaderDesc::new(case.language);
            gpu.extract_gpu_shader_info(&mut desc).unwrap();
            let property = desc.dynamic_properties()[0].as_grading_hue_curve().unwrap();
            let uniform_values = |desc: &GpuShaderDesc| -> Vec<Value> {
                port_uniforms(desc)
                    .into_iter()
                    .map(|(name, _, _, value)| uniform_json(&name, &value))
                    .collect()
            };
            let wheel_values = |step: &Value| -> Vec<Value> {
                step["result"]
                    .as_array()
                    .unwrap_or_else(|| panic!("{step}"))
                    .iter()
                    .map(|u| json!([u["name"], u["value"]]))
                    .collect()
            };
            let mut k = 1;
            let mut previous: Option<Vec<Value>> = None;
            for set in std::iter::once(None).chain(sets.iter().map(Some)) {
                if let Some(v) = set {
                    let wheel_set = &wheel_steps[k];
                    match property.set_value(v) {
                        Ok(()) => assert!(wheel_set.get("exception").is_none(), "{wheel_set}"),
                        Err(e) => {
                            assert_eq!(
                                wheel_set["exception"]["message"],
                                json!(e.to_string()),
                                "{wheel_set}"
                            );
                            refused_count += 1;
                        }
                    }
                    k += 1;
                }
                let wheel = wheel_values(&wheel_steps[k]);
                let port = uniform_values(&desc);
                assert_eq!(wheel.len(), 5);
                compared += 1;
                if wheel != port {
                    failures.push(format!(
                        "{} after {set:?}: wheel {wheel:?} port {port:?}",
                        case.label
                    ));
                }
                if previous.as_ref().is_some_and(|p| p != &wheel) {
                    changed += 1;
                }
                previous = Some(wheel);
                k += 1;
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert_eq!(compared, 3 * 2 * 7);
    assert_eq!(refused_count, 3 * 2);
    // The sets but the refused one change the uniforms.
    assert!(changed >= 3 * 2 * 4, "{changed} sets changed the uniforms");
}

/// The shader description's property is a copy of the op's: after the shader program is
/// written, a set on the op data's own property leaves the uniforms as they were, and a set on
/// the description's property changes them. The wheel's copy (`createEditableCopy`,
/// GradingHueCurveOpGPU.cpp:646-652 @ v2.5.2) gives the uniforms' values before and after a
/// set on the description's property, which the port's uniforms must follow; the set on the
/// op data's property is the port's own step (the wheel's processor keeps its ops out of
/// reach), expected to change nothing, as the copy guarantees.
///
/// The port's step is a unit-level test: the port's GPU processor clones its ops, so a set
/// on the original property is out of the pipeline's reach, and the test writes the shader
/// program of the op data directly (`get_grading_hue_curve_gpu_shader_program`).
#[test]
fn shader_property_is_decoupled_from_the_op() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let mut compared = 0;
    for style in STYLES {
        for inverse in [false, true] {
            let mut op = Op::new(style, all_curves(style)).dynamic();
            if inverse {
                op = op.inverse();
            }
            let case = Case {
                label: format!("{style:?} inverse {inverse}"),
                ops: vec![op.clone()],
                flags: (None, OptimizationFlags::DEFAULT),
                language: GpuLanguage::Glsl4_0,
                oracle_language: oracle_gpu::GpuLanguage::Glsl40,
            };
            let after = extremes(style);
            let mut args = case.processor();
            args["gpu"] = json!({"shader": {"language": "GPU_LANGUAGE_GLSL_4_0"}});
            args["buffers"] = json!([]);
            args["images"] = json!([]);
            args["steps"] = json!([
                {"get": "DYNAMIC_PROPERTY_GRADING_HUECURVE", "from": "gpu", "as": "p"},
                {"uniforms": true},
                {"set": "p", "value": hue_curve_spec(&after)},
                {"uniforms": true},
            ]);
            let response = Oracle::get().call("dynamic_apply", args, &[]);
            let result = &response.result;
            assert!(result.get("exception").is_none(), "{result}");
            let wheel_steps = result["steps"].as_array().unwrap();
            assert_eq!(wheel_steps.len(), 4, "{result}");
            let wheel_values = |step: &Value| -> Vec<Value> {
                step["result"]
                    .as_array()
                    .unwrap_or_else(|| panic!("{step}"))
                    .iter()
                    .map(|u| json!([u["name"], u["value"]]))
                    .collect()
            };
            let (wheel_before, wheel_after) =
                (wheel_values(&wheel_steps[1]), wheel_values(&wheel_steps[3]));
            assert!(!wheel_before.is_empty());
            assert_ne!(
                wheel_before, wheel_after,
                "{}: the set changes no uniform",
                case.label
            );

            let data = op.data();
            let mut desc = GpuShaderDesc::new(case.language);
            get_grading_hue_curve_gpu_shader_program(&mut desc, &data).unwrap();
            let uniform_values = |desc: &GpuShaderDesc| -> Vec<Value> {
                port_uniforms(desc)
                    .into_iter()
                    .map(|(name, _, _, value)| uniform_json(&name, &value))
                    .collect()
            };
            assert_eq!(uniform_values(&desc), wheel_before, "{}", case.label);

            // A set on the op data's property: the description's uniforms keep their values.
            data.get_dynamic_property_internal()
                .set_value(&drawn(style))
                .unwrap();
            assert_eq!(
                uniform_values(&desc),
                wheel_before,
                "{}: the description's uniforms follow the op's property",
                case.label
            );

            // A set on the description's property: the uniforms follow it, as the wheel's.
            desc.dynamic_properties()[0]
                .as_grading_hue_curve()
                .unwrap()
                .set_value(&after)
                .unwrap();
            assert_eq!(uniform_values(&desc), wheel_after, "{}", case.label);
            compared += 1;
        }
    }
    assert_eq!(compared, 3 * 2);
}
