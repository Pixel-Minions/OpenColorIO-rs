// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Log renderers against the wheel, bit for bit, through the oracle test battery
//! (`ocio_testkit::battery`): every case in both directions, with fast math on and off, on
//! the tier's probe sets (`OCIO_RS_TIER`).
//!
//! The oracle builds a LogTransform, LogAffineTransform or LogCameraTransform in a raw config
//! and applies its CPU processor to F32 RGBA pixels. The families build the op data that
//! upstream's transform and `BuildLogOp` produce (each helper cites them) and the renderer
//! that `GetLogRenderer` picks.
//!
//! For a single Log transform at F32, the processor's op list is that one LogOp: `LogOpData`
//! is never a no-op or an identity (`LogOpData::isNoOp`/`isIdentity` return false), has no
//! simpler replacement, and the separable-prefix bake skips F32 and UINT32 inputs (it bakes
//! F16 and the other integer depths: `OptimizeSeparablePrefix`,
//! src/OpenColorIO/OpOptimizers.cpp:559-565 @ v2.5.2).
//!
//! The op data goes through `std::hint::black_box`, so that the compiler cannot evaluate the
//! renderers' constructors on the tests' constant parameters: it folds, for example, `log2` of
//! a negative constant to a positive NaN, where the math library returns the negative x86
//! default NaN at run time.
//!
//! The Log renderers never write alpha (they work in place), so alpha is a pass-through
//! channel for the battery.

use std::hint::black_box;

use ocio_ops::bit_depth_utils::{Converter, Uint8, Uint10, Uint12, Uint16};
use ocio_ops::open_color_types::TransformDirection;
use ocio_ops::ops::log::log_op_cpu::get_log_renderer;
use ocio_ops::ops::log::log_op_data::{LogAffineParameter, LogOpData};
use ocio_ops::ops::log::log_utils::{
    get_log_side_break, get_log_side_break_libstdcxx, get_log_side_break_msvc,
};
use ocio_testkit::Oracle;
use ocio_testkit::battery::params::{A, Case, Channels, Params, Precision, RGB, Slot};
use ocio_testkit::battery::{
    self, Combo, Direction, Family, Mutations, Plan, Port, Spec, Validation, yaml_list, yaml_number,
};
use ocio_testkit::oracle::{BatchCall, f32_to_bytes};
use ocio_testkit::probe::{self, ProbeSet, RandomRange};
use serde_json::json;

/// The port's direction for the battery's.
fn port_direction(direction: Direction) -> TransformDirection {
    match direction {
        Direction::Forward => TransformDirection::Forward,
        Direction::Inverse => TransformDirection::Inverse,
    }
}

/// The renderer `GetLogRenderer` picks for `data`, as a battery port.
fn log_port(data: LogOpData, combo: &Combo) -> Result<Port, String> {
    let renderer = get_log_renderer(&black_box(data), combo.fast_math);
    Ok(Port::in_place(move |px| renderer.apply(px)))
}

/// Whether every slot of `params` is finite: then a JSON transform spec can hold them.
fn all_finite<P: Params>(params: &P) -> bool {
    (0..params.slots().len()).all(|i| params.get(i).is_finite())
}

/// The parameters of a LogTransform.
#[derive(Debug, Clone, PartialEq)]
struct LogBase {
    base: f64,
}

impl Params for LogBase {
    fn slots(&self) -> Vec<Slot> {
        vec![Slot::new("base", Precision::F64AsF32, RGB)]
    }
    fn get(&self, _: usize) -> f64 {
        self.base
    }
    fn set(&mut self, _: usize, value: f64) {
        self.base = value;
    }
}

/// The op data of `LogTransform(base, direction)` in a forward processor: default parameters.
///
/// `LogTransformImpl()` holds `LogOpData(2.0f, TRANSFORM_DIR_FORWARD)`
/// (src/OpenColorIO/transforms/LogTransform.cpp:25-28 @ v2.5.2); the Python constructor calls
/// `setBase` and `setDirection` (src/bindings/python/transforms/PyLogTransform.cpp:20-27); and
/// `BuildLogOp` clones the data, inverting it only for an inverse processor
/// (src/OpenColorIO/ops/log/LogOp.cpp:142-153, 212-221).
fn log_transform_op(base: f64, dir: TransformDirection) -> LogOpData {
    let mut data = LogOpData::new(2.0, TransformDirection::Forward);
    data.set_base(base);
    data.set_direction(dir);
    data
}

/// LogTransform.
struct LogFamily {
    cases: Vec<Case<LogBase>>,
    bases: Vec<Case<LogBase>>,
}

impl Family for LogFamily {
    type Params = LogBase;

    fn name(&self) -> String {
        "LogTransform".to_string()
    }
    fn cases(&self) -> Vec<Case<LogBase>> {
        self.cases.clone()
    }
    fn mutation_bases(&self) -> Vec<Case<LogBase>> {
        self.bases.clone()
    }
    fn spec(&self, p: &LogBase, direction: Direction) -> Spec {
        if all_finite(p) {
            Spec::Transform(json!({
                "class": "LogTransform",
                "args": {"base": p.base, "direction": direction.oracle_enum()},
            }))
        } else {
            Spec::Yaml(format!(
                "!<LogTransform> {{base: {}, direction: {}}}",
                yaml_number(p.base),
                direction.yaml()
            ))
        }
    }
    fn port(&self, p: &LogBase, combo: &Combo) -> Result<Port, String> {
        log_port(
            log_transform_op(p.base, port_direction(combo.direction)),
            combo,
        )
    }
    fn pass_through(&self, _: &LogBase, _: &Combo) -> Channels {
        A
    }
    fn validation(&self) -> Validation {
        Validation::NotPorted { card: "WP 1.3l1" }
    }
}

#[test]
fn log_transform_matches_the_wheel() {
    // 2 and 10 are the Log2/Log10 renderers; others use LinToLog and LogToLin.
    let mut cases: Vec<Case<LogBase>> = [2.0, 10.0, std::f64::consts::E, 3.7, 0.5]
        .map(|base| Case::new(format!("base {base}"), LogBase { base }))
        .to_vec();
    let bases = vec![cases[3].clone()];
    // A NaN base takes the YAML spec, as the generated NaN and ±Inf cases do: an explicit case
    // there makes a bug in that spec fail rather than show as refusals.
    cases.push(Case::new("base NaN", LogBase { base: f64::NAN }).w0002_nowhere());
    battery::run(&LogFamily { cases, bases });
}

/// The parameters of a LogAffineTransform.
#[derive(Debug, Clone, PartialEq)]
struct Affine {
    base: f64,
    log_side_slope: [f64; 3],
    log_side_offset: [f64; 3],
    lin_side_slope: [f64; 3],
    lin_side_offset: [f64; 3],
}

impl Params for Affine {
    fn slots(&self) -> Vec<Slot> {
        let p = Precision::F64AsF32;
        let mut s = vec![Slot::new("base", p, RGB)];
        for name in [
            "log_side_slope",
            "log_side_offset",
            "lin_side_slope",
            "lin_side_offset",
        ] {
            s.extend(Slot::rgb(name, p));
        }
        s
    }
    fn get(&self, i: usize) -> f64 {
        match i {
            0 => self.base,
            1..=3 => self.log_side_slope[i - 1],
            4..=6 => self.log_side_offset[i - 4],
            7..=9 => self.lin_side_slope[i - 7],
            _ => self.lin_side_offset[i - 10],
        }
    }
    fn set(&mut self, i: usize, v: f64) {
        match i {
            0 => self.base = v,
            1..=3 => self.log_side_slope[i - 1] = v,
            4..=6 => self.log_side_offset[i - 4] = v,
            7..=9 => self.lin_side_slope[i - 7] = v,
            _ => self.lin_side_offset[i - 10] = v,
        }
    }
}

impl Affine {
    fn spec(&self, dir: Direction) -> Spec {
        if !all_finite(self) {
            return Spec::Yaml(self.yaml(dir));
        }
        Spec::Transform(json!({
            "class": "LogAffineTransform",
            "args": {
                "logSideSlope": self.log_side_slope,
                "logSideOffset": self.log_side_offset,
                "linSideSlope": self.lin_side_slope,
                "linSideOffset": self.lin_side_offset,
                "direction": dir.oracle_enum(),
            },
            "calls": [["setBase", self.base]],
        }))
    }

    /// The transform in the config's YAML syntax, which can hold NaN and infinite parameters.
    fn yaml(&self, dir: Direction) -> String {
        format!(
            "!<LogAffineTransform> {{base: {}, log_side_slope: {}, log_side_offset: {}, \
             lin_side_slope: {}, lin_side_offset: {}, direction: {}}}",
            yaml_number(self.base),
            yaml_list(&self.log_side_slope),
            yaml_list(&self.log_side_offset),
            yaml_list(&self.lin_side_slope),
            yaml_list(&self.lin_side_offset),
            dir.yaml()
        )
    }

    /// `LogAffineTransformImpl()` holds `LogOpData(2.0f, TRANSFORM_DIR_FORWARD)`
    /// (src/OpenColorIO/transforms/LogAffineTransform.cpp:25-28 @ v2.5.2); the Python
    /// constructor sets the four values and the direction
    /// (src/bindings/python/transforms/PyLogAffineTransform.cpp:31-45), `setBase` sets the base
    /// (LogAffineTransform.cpp:78-81), and `BuildLogOp` clones the data
    /// (src/OpenColorIO/ops/log/LogOp.cpp:190-199).
    fn op(&self, dir: TransformDirection) -> LogOpData {
        let mut data = LogOpData::new(2.0, TransformDirection::Forward);
        for (param, values) in [
            (LogAffineParameter::LogSideSlope, &self.log_side_slope),
            (LogAffineParameter::LogSideOffset, &self.log_side_offset),
            (LogAffineParameter::LinSideSlope, &self.lin_side_slope),
            (LogAffineParameter::LinSideOffset, &self.lin_side_offset),
        ] {
            data.set_value(param, values).unwrap();
        }
        data.set_direction(dir);
        data.set_base(self.base);
        data
    }
}

/// LogAffineTransform.
struct AffineFamily {
    cases: Vec<Case<Affine>>,
    bases: Vec<Case<Affine>>,
}

impl Family for AffineFamily {
    type Params = Affine;

    fn name(&self) -> String {
        "LogAffineTransform".to_string()
    }
    fn cases(&self) -> Vec<Case<Affine>> {
        self.cases.clone()
    }
    fn mutation_bases(&self) -> Vec<Case<Affine>> {
        self.bases.clone()
    }
    fn spec(&self, p: &Affine, direction: Direction) -> Spec {
        p.spec(direction)
    }
    fn port(&self, p: &Affine, combo: &Combo) -> Result<Port, String> {
        log_port(p.op(port_direction(combo.direction)), combo)
    }
    fn pass_through(&self, _: &Affine, _: &Combo) -> Channels {
        A
    }
    fn validation(&self) -> Validation {
        Validation::NotPorted { card: "WP 1.3l1" }
    }
}

fn affine_cases() -> Vec<Case<Affine>> {
    [
        // Different parameters per channel.
        Affine {
            base: 10.0,
            log_side_slope: [0.18, 0.5, 1.7],
            log_side_offset: [0.4, -0.1, 0.0],
            lin_side_slope: [1.5, 0.9, 2.2],
            lin_side_offset: [0.01, 0.2, -0.05],
        },
        // Cineon-like, base 10.
        Affine {
            base: 10.0,
            log_side_slope: [0.293255132; 3],
            log_side_offset: [0.669599218; 3],
            lin_side_slope: [0.9892; 3],
            lin_side_offset: [0.0108; 3],
        },
        // Base e and base 2 with non-default parameters.
        Affine {
            base: std::f64::consts::E,
            log_side_slope: [0.25, 0.3, 0.35],
            log_side_offset: [0.5; 3],
            lin_side_slope: [4.0, 5.0, 6.0],
            lin_side_offset: [0.1; 3],
        },
        Affine {
            base: 2.0,
            log_side_slope: [0.05; 3],
            log_side_offset: [0.6; 3],
            lin_side_slope: [1.0; 3],
            lin_side_offset: [0.0078125; 3],
        },
        // Default parameters: these are plain Log2 and Log10 ops (GetLogRenderer's isLog2 and
        // isLog10), not LinToLog.
        Affine {
            base: 2.0,
            log_side_slope: [1.0; 3],
            log_side_offset: [0.0; 3],
            lin_side_slope: [1.0; 3],
            lin_side_offset: [0.0; 3],
        },
        Affine {
            base: 10.0,
            log_side_slope: [1.0; 3],
            log_side_offset: [0.0; 3],
            lin_side_slope: [1.0; 3],
            lin_side_offset: [0.0; 3],
        },
    ]
    .into_iter()
    .enumerate()
    .map(|(i, a)| Case::new(format!("case {i}"), a))
    .collect()
}

#[test]
fn log_affine_transform_matches_the_wheel() {
    let mut cases = affine_cases();
    let bases = vec![cases[0].clone()];
    // An explicit case on the YAML spec the generated NaN and ±Inf cases take.
    let [_, nan_base] = affine_nan_cases();
    cases.push(nan_base);
    battery::run(&AffineFamily { cases, bases });
}

/// The parameters of a LogCameraTransform.
#[derive(Debug, Clone, PartialEq)]
struct Camera {
    base: f64,
    lin_side_break: [f64; 3],
    log_side_slope: [f64; 3],
    log_side_offset: [f64; 3],
    lin_side_slope: [f64; 3],
    lin_side_offset: [f64; 3],
    linear_slope: Option<[f64; 3]>,
}

impl Params for Camera {
    fn slots(&self) -> Vec<Slot> {
        let p = Precision::F64AsF32;
        let mut s = vec![Slot::new("base", p, RGB)];
        for name in [
            "lin_side_break",
            "log_side_slope",
            "log_side_offset",
            "lin_side_slope",
            "lin_side_offset",
        ] {
            s.extend(Slot::rgb(name, p));
        }
        if self.linear_slope.is_some() {
            s.extend(Slot::rgb("linear_slope", p));
        }
        s
    }
    fn get(&self, i: usize) -> f64 {
        match i {
            0 => self.base,
            1..=3 => self.lin_side_break[i - 1],
            4..=6 => self.log_side_slope[i - 4],
            7..=9 => self.log_side_offset[i - 7],
            10..=12 => self.lin_side_slope[i - 10],
            13..=15 => self.lin_side_offset[i - 13],
            _ => self.linear_slope.expect("a linear slope")[i - 16],
        }
    }
    fn set(&mut self, i: usize, v: f64) {
        match i {
            0 => self.base = v,
            1..=3 => self.lin_side_break[i - 1] = v,
            4..=6 => self.log_side_slope[i - 4] = v,
            7..=9 => self.log_side_offset[i - 7] = v,
            10..=12 => self.lin_side_slope[i - 10] = v,
            13..=15 => self.lin_side_offset[i - 13] = v,
            _ => self.linear_slope.as_mut().expect("a linear slope")[i - 16] = v,
        }
    }
}

impl Camera {
    fn spec(&self, dir: Direction) -> Spec {
        if !all_finite(self) {
            return Spec::Yaml(self.yaml(dir));
        }
        Spec::Transform(json!({
            "class": "LogCameraTransform",
            "args": {
                "linSideBreak": self.lin_side_break,
                "base": self.base,
                "logSideSlope": self.log_side_slope,
                "logSideOffset": self.log_side_offset,
                "linSideSlope": self.lin_side_slope,
                "linSideOffset": self.lin_side_offset,
                "linearSlope": self.linear_slope.map_or(json!([]), |s| json!(s)),
                "direction": dir.oracle_enum(),
            },
        }))
    }

    /// The transform in the config's YAML syntax, which can hold NaN and infinite parameters.
    fn yaml(&self, dir: Direction) -> String {
        let linear_slope = self.linear_slope.map_or(String::new(), |s| {
            format!("linear_slope: {}, ", yaml_list(&s))
        });
        format!(
            "!<LogCameraTransform> {{base: {}, lin_side_break: {}, log_side_slope: {}, \
             log_side_offset: {}, lin_side_slope: {}, lin_side_offset: {}, {linear_slope}\
             direction: {}}}",
            yaml_number(self.base),
            yaml_list(&self.lin_side_break),
            yaml_list(&self.log_side_slope),
            yaml_list(&self.log_side_offset),
            yaml_list(&self.lin_side_slope),
            yaml_list(&self.lin_side_offset),
            dir.yaml()
        )
    }

    /// `LogCameraTransformImpl(linSideBreak)` holds `LogOpData(2.0f, TRANSFORM_DIR_FORWARD)`
    /// with the break set (src/OpenColorIO/transforms/LogCameraTransform.cpp:26-30 @ v2.5.2);
    /// the Python constructor sets the base, the four values, the linear slope when given and
    /// the direction (src/bindings/python/transforms/PyLogCameraTransform.cpp:37-63); and
    /// `BuildLogOp` clones the data (src/OpenColorIO/ops/log/LogOp.cpp:201-210).
    fn op(&self, dir: TransformDirection) -> LogOpData {
        let mut data = LogOpData::new(2.0, TransformDirection::Forward);
        data.set_value(LogAffineParameter::LinSideBreak, &self.lin_side_break)
            .unwrap();
        data.set_base(self.base);
        for (param, values) in [
            (LogAffineParameter::LogSideSlope, &self.log_side_slope),
            (LogAffineParameter::LogSideOffset, &self.log_side_offset),
            (LogAffineParameter::LinSideSlope, &self.lin_side_slope),
            (LogAffineParameter::LinSideOffset, &self.lin_side_offset),
        ] {
            data.set_value(param, values).unwrap();
        }
        if let Some(slope) = &self.linear_slope {
            data.set_value(LogAffineParameter::LinearSlope, slope)
                .unwrap();
        }
        data.set_direction(dir);
        data
    }

    /// The break points on both sides, per channel: the battery probes their neighbourhoods.
    fn break_points(&self) -> Vec<f32> {
        let data = black_box(self.op(TransformDirection::Forward));
        let base = self.base as f32;
        let params = [data.red_params(), data.green_params(), data.blue_params()];
        let mut v = Vec::new();
        for (lin_side_break, p) in self.lin_side_break.iter().zip(params) {
            v.push(*lin_side_break as f32);
            v.push(get_log_side_break(p, f64::from(base)));
        }
        v
    }
}

/// LogCameraTransform.
struct CameraFamily {
    cases: Vec<Case<Camera>>,
    bases: Vec<Case<Camera>>,
}

impl Family for CameraFamily {
    type Params = Camera;

    fn name(&self) -> String {
        "LogCameraTransform".to_string()
    }
    fn cases(&self) -> Vec<Case<Camera>> {
        self.cases.clone()
    }
    fn mutation_bases(&self) -> Vec<Case<Camera>> {
        self.bases.clone()
    }
    fn spec(&self, p: &Camera, direction: Direction) -> Spec {
        p.spec(direction)
    }
    fn port(&self, p: &Camera, combo: &Combo) -> Result<Port, String> {
        log_port(p.op(port_direction(combo.direction)), combo)
    }
    fn pass_through(&self, _: &Camera, _: &Combo) -> Channels {
        A
    }
    fn breakpoints(&self, p: &Camera, _: Direction) -> Vec<f32> {
        p.break_points()
    }
    fn validation(&self) -> Validation {
        Validation::NotPorted { card: "WP 1.3l1" }
    }
}

fn camera_cases() -> Vec<Case<Camera>> {
    // ARRI LogC3 (EI 800) as a LogCameraTransform.
    let logc3 = Camera {
        base: 10.0,
        lin_side_break: [0.010591; 3],
        log_side_slope: [0.247190; 3],
        log_side_offset: [0.385537; 3],
        lin_side_slope: [5.555556; 3],
        lin_side_offset: [0.052272; 3],
        linear_slope: Some([5.367655; 3]),
    };
    let rgb = Camera {
        base: 2.0,
        lin_side_break: [0.1, 0.05, 0.2],
        log_side_slope: [0.2, 0.25, 0.18],
        log_side_offset: [0.6, 0.55, 0.62],
        lin_side_slope: [1.1, 1.3, 0.9],
        lin_side_offset: [0.05, 0.02, 0.1],
        linear_slope: Some([1.2, 1.4, 0.95]),
    };
    // Base 10, where GetLogSideBreak's float (Windows) and double (Linux) computations give
    // different breaks on every channel (camera_cases_distinguish_the_log_side_break_variants).
    let rgb10 = Camera {
        base: 10.0,
        lin_side_break: [0.010591; 3],
        log_side_slope: [0.24719; 3],
        log_side_offset: [0.385537, 0.6, 0.0],
        lin_side_slope: [5.555556, 1.0, 1.0],
        lin_side_offset: [0.05, 0.05, 0.0],
        linear_slope: Some([5.367655, 1.1, 0.9]),
    };
    vec![
        Case::new(
            "LogC3 EI800, computed linear slope",
            Camera {
                linear_slope: None,
                ..logc3.clone()
            },
        ),
        Case::new("LogC3 EI800", logc3),
        Case::new(
            "per channel, computed linear slope",
            Camera {
                linear_slope: None,
                ..rgb.clone()
            },
        ),
        Case::new("per channel", rgb),
        Case::new(
            "per channel, base 10, computed linear slope",
            Camera {
                linear_slope: None,
                ..rgb10.clone()
            },
        ),
        Case::new("per channel, base 10", rgb10),
        Case::new(
            "base e",
            Camera {
                base: std::f64::consts::E,
                lin_side_break: [0.18, 0.02, 0.3],
                log_side_slope: [0.3, 0.3, 0.3],
                log_side_offset: [0.5, 0.45, 0.55],
                lin_side_slope: [3.0, 2.5, 4.0],
                lin_side_offset: [0.01, 0.03, 0.005],
                linear_slope: None,
            },
        ),
        negative_break(),
    ]
}

/// A negative break: log2 of a negative number. The break on the log side is NaN, with the
/// sign each platform's log2 gives (see log_utils::log2_glibc_2_2_5), and so is the offset of
/// the linear segment.
fn negative_break() -> Case<Camera> {
    Case::new(
        "negative break",
        Camera {
            base: 2.0,
            lin_side_break: [-0.05, -0.1, -0.2],
            log_side_slope: [1.0; 3],
            log_side_offset: [0.0; 3],
            lin_side_slope: [1.0; 3],
            lin_side_offset: [0.0; 3],
            linear_slope: None,
        },
    )
}

/// The platform variants of `GetLogSideBreak` differ for some of the camera cases, so the
/// camera test distinguishes them: it passes only with the variant of the platform it runs on.
#[test]
fn camera_cases_distinguish_the_log_side_break_variants() {
    let mut differ = Vec::new();
    for case in camera_cases() {
        let data = black_box(case.params().op(TransformDirection::Forward));
        let base = f64::from(case.params().base as f32);
        for (c, params) in [data.red_params(), data.green_params(), data.blue_params()]
            .into_iter()
            .enumerate()
        {
            let msvc = get_log_side_break_msvc(params, base);
            let libstdcxx = get_log_side_break_libstdcxx(params, base);
            if msvc.to_bits() != libstdcxx.to_bits() {
                differ.push(format!(
                    "{} channel {c}: {msvc:e} ({:#010x}) vs {libstdcxx:e} ({:#010x})",
                    case.label(),
                    msvc.to_bits(),
                    libstdcxx.to_bits()
                ));
            }
        }
    }
    assert!(
        differ.iter().any(|d| !d.contains("NaN")),
        "no camera case distinguishes the variants with finite breaks: {differ:?}"
    );
    println!("{}", differ.join("\n"));
}

/// NaN pixels meet the NaN offset of the negative break's linear segment, which the SSE
/// renderers compute for every pixel: x86 returns the first operand's NaN, the pixel's in
/// `_mm_add_ps(pixel, offset)` and `_mm_mul_ps(pixel, slope)`. Every buffer length from 1 to 24
/// pixels, so that each part of the port's loops (in a release build, a vectorized body and a
/// scalar remainder) meets the NaNs. (Every battery run probes these buffers for every case;
/// this test runs them alone for the case that needs them.)
#[test]
fn nan_pixels_meet_nan_offsets_at_every_buffer_length() {
    let plan = Plan {
        name: "NaN buffers".to_string(),
        probes: vec![ProbeSet::NanBuffers { max_pixels: 24 }],
        generated_probes: Vec::new(),
        breakpoint_ulps: 0,
        mutations: Mutations::None,
        sweep: None,
        ..ocio_testkit::battery::Tier::Quick.plan()
    };
    let family = CameraFamily {
        cases: vec![negative_break()],
        bases: Vec::new(),
    };
    battery::run_with(&family, &plan);
}

#[test]
fn log_camera_transform_matches_the_wheel() {
    let mut cases = camera_cases();
    let bases = vec![cases[3].clone()];
    // An explicit case on the YAML spec the generated NaN and ±Inf cases take.
    cases.push(camera_nan_case());
    battery::run(&CameraFamily { cases, bases });
}

/// LogAffineTransforms whose finite parameters overflow `float` (|value| >= FLT_MAX, or a base
/// below the smallest subnormal), so that coefficients are infinite or NaN (`inf / inf`).
fn extreme_affine_cases() -> Vec<Case<Affine>> {
    [
        Affine {
            base: 1e39,
            log_side_slope: [1e39, 0.5, -1e39],
            log_side_offset: [0.1, 0.2, 0.3],
            lin_side_slope: [1.0; 3],
            lin_side_offset: [0.01, 0.02, 0.03],
        },
        Affine {
            base: 1e-46,
            log_side_slope: [1e39, 0.5, -1e39],
            log_side_offset: [0.1, 0.2, 0.3],
            lin_side_slope: [1.0; 3],
            lin_side_offset: [0.01, 0.02, 0.03],
        },
        Affine {
            base: 10.0,
            log_side_slope: [0.3; 3],
            log_side_offset: [1e39, -1e39, 0.3],
            lin_side_slope: [1e39, -1e39, 1.0],
            lin_side_offset: [-1e39, 1e39, 0.03],
        },
    ]
    .into_iter()
    .enumerate()
    .map(|(i, a)| Case::new(format!("extreme {i}"), a))
    .collect()
}

/// LogCameraTransforms whose finite parameters overflow `float` or `double`, so that the
/// break, the linear slope and the linear offset are infinite or NaN, with different NaN
/// signs: the default NaN of `inf / inf` is negative, a negated one positive, and on Linux
/// glibc's `log2` of a negative number positive.
fn extreme_camera_cases() -> Vec<Case<Camera>> {
    let camera = |name: &str,
                  base,
                  lin_side_break,
                  log_side_slope,
                  log_side_offset,
                  lin_side_slope,
                  lin_side_offset,
                  linear_slope| {
        Case::new(
            name,
            Camera {
                base,
                lin_side_break,
                log_side_slope,
                log_side_offset,
                lin_side_slope,
                lin_side_offset,
                linear_slope,
            },
        )
    };
    vec![
        camera(
            "base 1e39, log slope 1e39, negative break",
            1e39,
            [-0.05, -0.1, -0.2],
            [1e39; 3],
            [0.0; 3],
            [1.0; 3],
            [0.0; 3],
            None,
        ),
        camera(
            "base 1e39, log slope 1e39, positive break",
            1e39,
            [0.1, 0.2, 0.3],
            [1e39; 3],
            [0.5; 3],
            [1.0; 3],
            [0.01; 3],
            None,
        ),
        camera(
            "base 1e-46, log slope -1e39, negative break",
            1e-46,
            [-0.05, -0.1, -0.2],
            [-1e39; 3],
            [0.0; 3],
            [1.0; 3],
            [0.0; 3],
            None,
        ),
        // The computed linear slope is inf / inf in `double`, and the break on the log side
        // +Inf: the linear segment covers every finite input, with two NaN coefficients.
        camera(
            "overflowing computed slope",
            10.0,
            [1e200, 1e200, 0.1],
            [1e200, 1e200, 0.3],
            [0.5; 3],
            [1e200, 1e200, 1.0],
            [0.0; 3],
            None,
        ),
        camera(
            "linear slope 1e39, zero break",
            2.0,
            [0.0; 3],
            [0.25; 3],
            [0.5; 3],
            [1.0; 3],
            [0.0; 3],
            Some([1e39, -1e39, 1e39]),
        ),
        camera(
            "negative lin side to -inf",
            2.0,
            [-1e300, -0.1, 0.1],
            [1e39, 1.0, 1.0],
            [0.0; 3],
            [1e10, 1.0, 1.0],
            [0.0; 3],
            None,
        ),
        // `linSlope * linBreak` overflows to -Inf in `double` with a finite break: the linear
        // slope is NaN (inf / -inf), and on Linux so is the break on the log side (glibc's
        // `log2(-inf)`, positive), so the lin-to-log linear segment adds two different NaNs.
        camera(
            "overflowing lin side, finite break",
            2.0,
            [-2.0, -2.0, -2.0],
            [2.0; 3],
            [0.0; 3],
            [1.7e308; 3],
            [0.0; 3],
            None,
        ),
    ]
}

/// Finite parameters that overflow, against the wheel, bit for bit (W0002 covers NaN
/// parameters only). Their NaN coefficients reach the sites where the wheels' machine
/// code does not use upstream's source order: the camera log-to-lin linear segment (both
/// wheels) and `GetLogSideBreak` (Linux).
#[test]
fn extreme_finite_parameters_match_the_wheel() {
    let cases = extreme_affine_cases();
    assert!(
        cases
            .iter()
            .all(|c| c.kind() == battery::params::Kind::ExtremeFinite)
    );
    battery::run(&AffineFamily {
        cases,
        bases: Vec::new(),
    });
    let cases = extreme_camera_cases();
    assert!(
        cases
            .iter()
            .all(|c| c.kind() == battery::params::Kind::ExtremeFinite)
    );
    battery::run(&CameraFamily {
        cases,
        bases: Vec::new(),
    });
}

/// NaN parameters, which OCIO 2.5.2 accepts (YAML `.nan`), against the wheel.
///
/// Where a NaN parameter meets a NaN pixel value in one operation, the NaN that comes out
/// depends on the operand order in the wheel's machine code. The port follows it where MSVC
/// and GCC agree (`math_utils::sse_add`/`sse_mul`), so every case is compared bit for bit but
/// one. In `Log2LinRenderer`, MSVC swapped the operands of the red and green `+ minusb` and of
/// the blue `+ minuskb` (at 0x180211650, 0x18021165f and 0x1802115fb) and GCC did not, so the
/// port keeps the source order there. For that case (the first LogAffineTransform, inverse,
/// fast math off), waiver W0002 applies: in the channels with a NaN parameter, a NaN from the
/// wheel only has to be NaN in the port, and every other value is still compared bit for bit.
/// (The battery's generated NaN cases take W0002 in every combination.)
#[test]
fn nan_parameters_match_the_wheel_under_waiver_w0002() {
    battery::run(&AffineFamily {
        cases: affine_nan_cases().to_vec(),
        bases: Vec::new(),
    });
    battery::run(&CameraFamily {
        cases: vec![camera_nan_case()],
        bases: Vec::new(),
    });
}

/// The LogAffineTransforms with NaN parameters (YAML `.nan`): the first compares under W0002
/// only where MSVC swapped operands (inverse, fast math off; see
/// `nan_parameters_match_the_wheel_under_waiver_w0002`), the second bit for bit everywhere.
fn affine_nan_cases() -> [Case<Affine>; 2] {
    let nan = f64::NAN;
    let inverse_exact = |c: &Combo| c.direction == Direction::Inverse && !c.fast_math;
    [
        Case::new(
            "NaN parameters",
            Affine {
                base: 10.0,
                log_side_slope: [nan, 0.5, 1.0],
                log_side_offset: [0.1, nan, 0.2],
                lin_side_slope: [1.0, 1.0, nan],
                lin_side_offset: [nan, 0.01, 0.1],
            },
        )
        .w0002_only_where(inverse_exact),
        Case::new(
            "NaN base",
            Affine {
                base: nan,
                log_side_slope: [0.3, 0.5, 1.0],
                log_side_offset: [0.1, 0.2, 0.3],
                lin_side_slope: [1.0; 3],
                lin_side_offset: [0.0; 3],
            },
        )
        .w0002_nowhere(),
    ]
}

/// The LogCameraTransform with NaN parameters, bit for bit everywhere.
fn camera_nan_case() -> Case<Camera> {
    let nan = f64::NAN;
    Case::new(
        "NaN parameters",
        Camera {
            base: 2.0,
            lin_side_break: [0.1, nan, 0.2],
            log_side_slope: [0.25, 0.3, nan],
            log_side_offset: [0.5, nan, 0.6],
            lin_side_slope: [1.0; 3],
            lin_side_offset: [nan, 0.02, 0.01],
            linear_slope: Some([1.2, 1.0, nan]),
        },
    )
    .w0002_nowhere()
}

/// The S2 oracle tests' probe values: all halves, the specials, and 1,000,000 seeded random
/// values in several ranges, as RGBA pixels with each value in each channel.
fn s2_probe_rgba() -> Vec<f32> {
    let mut values = probe::all_half_values();
    values.extend(probe::specials());
    values.extend(probe::random(
        0x5252_0001,
        &[
            (RandomRange::Unit, 200_000),
            (RandomRange::Overshoot, 200_000),
            (RandomRange::Exponent, 100_000),
            (RandomRange::Hdr, 100_000),
        ],
    ));
    values.extend(probe::random(
        0x5252_0002,
        &[
            (RandomRange::Finite, 200_000),
            (RandomRange::Tiny, 100_000),
            (RandomRange::AllBits, 100_000),
        ],
    ));
    probe::to_rgba_cycled(&values)
}

/// The scalar integer casts (`Converter<BD>::CastValue`), against the wheel: a LogTransform's
/// processor at F32 in and an integer bit depth out ends with `BitDepthCast<F32, BD>`, which
/// converts `value * maxValue` (src/OpenColorIO/CPUProcessor.cpp:20-48 @ v2.5.2). The Log op
/// passes alpha through, so alpha shows the cast alone, and RGB shows it after the log.
///
/// Not a battery run yet: the battery's integer formats need the port's CPU engine (WP 1.2d).
/// Its four oracle calls go in one batch.
#[test]
fn integer_output_casts_match_the_wheel() {
    let input = s2_probe_rgba();
    let data = black_box(log_transform_op(2.0, TransformDirection::Forward));
    let mut log_out = input.clone();
    get_log_renderer(&data, true).apply(&mut log_out);

    // (bit depth, maxValue from BitDepthUtils.h:34-60, cast)
    type Cast = fn(f32) -> u16;
    let depths: [(&str, u16, Cast); 4] = [
        ("BIT_DEPTH_UINT8", 255, |v| u16::from(Uint8::cast_value(v))),
        ("BIT_DEPTH_UINT10", 1023, Uint10::cast_value),
        ("BIT_DEPTH_UINT12", 4095, Uint12::cast_value),
        ("BIT_DEPTH_UINT16", 65535, Uint16::cast_value),
    ];
    let input_bytes = f32_to_bytes(&input);
    let calls: Vec<BatchCall<'_>> = depths
        .iter()
        .map(|(depth, _, _)| BatchCall {
            cmd: "cpu_apply",
            args: json!({
                "transform": {"class": "LogTransform", "args": {"base": 2.0}},
                "out_bitdepth": depth,
            }),
            blobs: vec![&input_bytes],
        })
        .collect();
    let responses = Oracle::get().batch(&calls, true);
    let mut failures = Vec::new();
    for ((depth, max_value, cast), resp) in depths.into_iter().zip(responses) {
        let resp = resp.unwrap_or_else(|e| panic!("{depth}: {e}"));
        assert!(resp.result.get("exception").is_none(), "{}", resp.result);
        let expected: Vec<u16> = if max_value == 255 {
            resp.blobs[0].iter().map(|&b| u16::from(b)).collect()
        } else {
            resp.blobs[0]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| u16::from_le_bytes(*b))
                .collect()
        };

        // m_scale = float(maxValue) / float(1) (CPUProcessor.cpp:47-48).
        let scale = f32::from(max_value) / 1.0f32;
        let actual: Vec<u16> = log_out.iter().map(|&v| cast(v * scale)).collect();

        let mismatches: Vec<String> = expected
            .iter()
            .zip(&actual)
            .enumerate()
            .filter(|(_, (e, a))| e != a)
            .take(20)
            .map(|(i, (e, a))| {
                format!(
                    "  [{i}] expected {e}, actual {a}; f32 {:e} ({:#010x})",
                    log_out[i],
                    log_out[i].to_bits()
                )
            })
            .collect();
        if expected.len() != actual.len() || !mismatches.is_empty() {
            failures.push(format!(
                "{depth}: {} of {} values differ\n{}",
                expected.iter().zip(&actual).filter(|(e, a)| e != a).count(),
                expected.len(),
                mismatches.join("\n")
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
