// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The optimizer through the API against the wheel: for groups of transforms of every family
//! that has a pair of inverse ops or a pair it combines, the CPU processor's cache ID and the
//! debug log of building it, which prints the ops before and after and the pass counts
//! (`OpRcPtrVec::optimize`, src/OpenColorIO/OpOptimizers.cpp:611-756 @ v2.5.2), through the
//! oracle's `processor_debug_log`, message for message.
//!
//! - Nested pairs of inverses, `A B B' A'`, of each family: `RemoveInverseOps` steps back
//!   after removing a pair, so one pass removes both (OpOptimizers.cpp:207-300); with the
//!   family's `PAIR_IDENTITY_*` flag off, the ops stay or combine.
//! - A pass that only removes inverse pairs makes the loop go on (OpOptimizers.cpp:696).
//! - Two ops of a type that combines, with every flag and with every flag but that type's
//!   `COMP_*` (`IsCombineEnabled`, OpOptimizers.cpp:61-70).
//!
//! The inverse and composed Lut1D come with Phase 2 (WP 2.1, 2.5), and the Exponent op only in
//! a version 1 config. The tests set the process's logging level and function, so each holds
//! `LOGGING`.

use std::sync::{Arc, Mutex};

use ocio::{
    BitDepth, CdlStyle, CdlTransform, Config, ExponentTransform, ExponentWithLinearTransform,
    GroupTransform, LogAffineTransform, LogCameraTransform, LogTransform, MatrixTransform,
    OptimizationFlags, RangeTransform, Transform, TransformDirection,
};
use ocio_ops::logging::{
    LoggingFunction, get_logging_level, reset_to_default_logging_function, set_logging_function,
    set_logging_level,
};
use ocio_ops::open_color_types::LoggingLevel;
use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::transform_text::f64_spec;
use serde_json::{Value, json};

use TransformDirection::{Forward, Inverse};

static LOGGING: Mutex<()> = Mutex::new(());

/// A version 1 config with one color space: its transforms build Exponent ops.
const V1_CONFIG: &str = "ocio_profile_version: 1\n\
roles: {default: raw}\n\
colorspaces:\n  \
- !<ColorSpace> {name: raw}\n";

/// A transform: the spec that builds it in the wheel, and the port's.
type Built = (Value, Transform);

fn f64s(values: &[f64]) -> Value {
    Value::Array(values.iter().map(|&v| f64_spec(v)).collect())
}

fn dir_spec(dir: TransformDirection) -> Value {
    json!({"enum": match dir {
        Forward => "TRANSFORM_DIR_FORWARD",
        Inverse => "TRANSFORM_DIR_INVERSE",
    }})
}

/// A moncurve ExponentWithLinearTransform (a Gamma op).
fn moncurve(gamma: f64, offset: f64, dir: TransformDirection) -> Built {
    let g = [gamma, gamma, gamma, 1.0];
    let o = [offset, offset, offset, 0.0];
    let mut t = ExponentWithLinearTransform::new();
    t.set_gamma(&g);
    t.set_offset(&o);
    t.set_direction(dir);
    let spec = json!({"class": "ExponentWithLinearTransform", "calls": [
        ["setGamma", f64s(&g)],
        ["setOffset", f64s(&o)],
        ["setDirection", dir_spec(dir)],
    ]});
    (spec, t.into())
}

/// An ExponentTransform: a basic Gamma op in a version 2 config, an Exponent op in version 1.
fn exponent(value: f64, dir: TransformDirection) -> Built {
    let v = [value, value, value, 1.0];
    let mut t = ExponentTransform::new();
    t.set_value(&v);
    t.set_direction(dir);
    let spec = json!({"class": "ExponentTransform", "calls": [
        ["setValue", f64s(&v)],
        ["setDirection", dir_spec(dir)],
    ]});
    (spec, t.into())
}

fn log(base: f64, dir: TransformDirection) -> Built {
    let mut t = LogTransform::new();
    t.set_base(base);
    t.set_direction(dir);
    let spec = json!({"class": "LogTransform", "calls": [
        ["setBase", f64_spec(base)],
        ["setDirection", dir_spec(dir)],
    ]});
    (spec, t.into())
}

fn log_affine(log_slope: f64, lin_offset: f64, dir: TransformDirection) -> Built {
    let (log_slope, log_offset) = ([log_slope; 3], [0.5, 0.5, 0.5]);
    let (lin_slope, lin_offset) = ([1.5, 1.5, 1.5], [lin_offset; 3]);
    let mut t = LogAffineTransform::new();
    t.set_base(10.0);
    t.set_log_side_slope_value(&log_slope);
    t.set_log_side_offset_value(&log_offset);
    t.set_lin_side_slope_value(&lin_slope);
    t.set_lin_side_offset_value(&lin_offset);
    t.set_direction(dir);
    let spec = json!({"class": "LogAffineTransform", "calls": [
        ["setBase", f64_spec(10.0)],
        ["setLogSideSlopeValue", f64s(&log_slope)],
        ["setLogSideOffsetValue", f64s(&log_offset)],
        ["setLinSideSlopeValue", f64s(&lin_slope)],
        ["setLinSideOffsetValue", f64s(&lin_offset)],
        ["setDirection", dir_spec(dir)],
    ]});
    (spec, t.into())
}

fn log_camera(lin_side_break: f64, dir: TransformDirection) -> Built {
    let lin_side_break = [lin_side_break; 3];
    let mut t = LogCameraTransform::new(&lin_side_break);
    t.set_base(2.0);
    t.set_direction(dir);
    let spec = json!({"class": "LogCameraTransform",
    "args": {"linSideBreak": f64s(&lin_side_break)},
    "calls": [
        ["setBase", f64_spec(2.0)],
        ["setDirection", dir_spec(dir)],
    ]});
    (spec, t.into())
}

fn cdl(slope: f64, sat: f64, style: CdlStyle, dir: TransformDirection) -> Built {
    let (slope, offset, power) = ([slope, 1.0, 0.9], [0.01, -0.02, 0.0], [1.0, 1.1, 1.2]);
    let mut t = CdlTransform::new();
    t.set_slope(&slope);
    t.set_offset(&offset);
    t.set_power(&power);
    t.set_sat(sat);
    t.set_style(style);
    t.set_direction(dir);
    let style = match style {
        CdlStyle::Asc => "CDL_ASC",
        CdlStyle::NoClamp => "CDL_NO_CLAMP",
    };
    let spec = json!({"class": "CDLTransform", "calls": [
        ["setSlope", f64s(&slope)],
        ["setOffset", f64s(&offset)],
        ["setPower", f64s(&power)],
        ["setSat", f64_spec(sat)],
        ["setStyle", {"enum": style}],
        ["setDirection", dir_spec(dir)],
    ]});
    (spec, t.into())
}

fn offset_matrix(offset: [f64; 4], dir: TransformDirection) -> Built {
    let mut t = MatrixTransform::new();
    t.set_offset(&offset);
    t.set_direction(dir);
    let spec = json!({"class": "MatrixTransform", "calls": [
        ["setOffset", f64s(&offset)],
        ["setDirection", dir_spec(dir)],
    ]});
    (spec, t.into())
}

fn range(min_in: f64, max_in: f64, min_out: f64, max_out: f64) -> Built {
    let mut t = RangeTransform::new();
    t.set_min_in_value(min_in);
    t.set_max_in_value(max_in);
    t.set_min_out_value(min_out);
    t.set_max_out_value(max_out);
    let spec = json!({"class": "RangeTransform", "calls": [
        ["setMinInValue", f64_spec(min_in)],
        ["setMaxInValue", f64_spec(max_in)],
        ["setMinOutValue", f64_spec(min_out)],
        ["setMaxOutValue", f64_spec(max_out)],
    ]});
    (spec, t.into())
}

/// `A B B' A'`, from a maker of a transform and its direction.
fn nested(
    a: impl Fn(TransformDirection) -> Built,
    b: impl Fn(TransformDirection) -> Built,
) -> Vec<Built> {
    vec![a(Forward), b(Forward), b(Inverse), a(Inverse)]
}

fn all_but(flag: OptimizationFlags) -> OptimizationFlags {
    OptimizationFlags(OptimizationFlags::ALL.0 & !flag.0)
}

/// A group of transforms, in a config of version 1 or 2, optimized with each of `flags`.
struct Case {
    label: &'static str,
    v1: bool,
    children: Vec<Built>,
    flags: Vec<OptimizationFlags>,
}

/// Each case's CPU processor (F32 in and out) with each of its flags, on both sides: the
/// cache ID and the debug log of building it.
// `unsigned long` is already `u64` on Linux.
#[allow(clippy::unnecessary_cast)]
fn check(cases: &[Case]) {
    let _logging = LOGGING.lock().unwrap_or_else(|e| e.into_inner());
    let runs: Vec<(&Case, OptimizationFlags)> = cases
        .iter()
        .flat_map(|case| case.flags.iter().map(move |&flags| (case, flags)))
        .collect();
    let calls: Vec<BatchCall<'_>> = runs
        .iter()
        .map(|(case, flags)| {
            let children: Vec<Value> = case.children.iter().map(|(s, _)| s.clone()).collect();
            let mut args = json!({
                "transform": {"class": "GroupTransform", "children": children},
                "optimization": flags.0 as u64,
            });
            if case.v1 {
                args["config"] = json!({"yaml": V1_CONFIG});
            }
            BatchCall {
                cmd: "processor_debug_log",
                args,
                blobs: vec![],
            }
        })
        .collect();
    let results = Oracle::get().batch(&calls, true);

    let messages = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = messages.clone();
    let function: LoggingFunction = Arc::new(move |m: &[u8]| {
        sink.lock()
            .unwrap()
            .push(String::from_utf8(m.to_vec()).unwrap());
    });
    let level = get_logging_level();

    let mut failures = Vec::new();
    for ((case, flags), result) in runs.iter().zip(results) {
        let wheel = result.unwrap_or_else(|e| panic!("{e}")).result;
        assert!(
            wheel.get("exception").is_none(),
            "{}: the wheel raised {}",
            case.label,
            wheel["exception"]
        );
        let wheel_log: Vec<String> = wheel["cpu_processor"]
            .as_array()
            .expect("the CPU processor's log")
            .iter()
            .map(|m| m.as_str().unwrap().to_string())
            .collect();

        let mut config = Config::create_raw().unwrap();
        if case.v1 {
            Arc::get_mut(&mut config)
                .unwrap()
                .set_major_version(1)
                .unwrap();
        }
        let mut group = GroupTransform::new();
        for (_, child) in &case.children {
            group.append_transform(child.clone());
        }
        let proc = config.processor(&group.into()).unwrap();
        messages.lock().unwrap().clear();
        set_logging_function(Some(function.clone())).unwrap();
        set_logging_level(LoggingLevel::Debug);
        let cpu =
            proc.optimized_cpu_processor_with_bit_depths(BitDepth::F32, BitDepth::F32, *flags);
        set_logging_level(level);
        reset_to_default_logging_function();
        let cpu = cpu.unwrap();
        let port_log = messages.lock().unwrap().clone();
        let port_id = String::from_utf8(cpu.get_cache_id().to_vec()).unwrap();
        if wheel["cpu_cache_id"].as_str() != Some(port_id.as_str()) || wheel_log != port_log {
            failures.push(format!(
                "{}, flags {:#x}\n  wheel {} {wheel_log:#?}\n  port  {port_id} {port_log:#?}",
                case.label, flags.0, wheel["cpu_cache_id"]
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} runs differ:\n{}",
        failures.len(),
        runs.len(),
        failures.join("\n")
    );
}

/// Nested pairs of inverses of each family with a pair-identity flag, and of mixed families,
/// with the default flags, all of them, none, and all but the family's flag.
#[test]
fn nested_inverse_pairs_match_the_wheel() {
    let flags = |pair: OptimizationFlags| {
        vec![
            OptimizationFlags::DEFAULT,
            OptimizationFlags::ALL,
            OptimizationFlags::NONE,
            all_but(pair),
        ]
    };
    let gamma = OptimizationFlags::PAIR_IDENTITY_GAMMA;
    let log_flag = OptimizationFlags::PAIR_IDENTITY_LOG;
    let cdl_flag = OptimizationFlags::PAIR_IDENTITY_CDL;
    let cases = vec![
        Case {
            label: "moncurve gammas",
            v1: false,
            children: nested(|d| moncurve(2.2, 0.1, d), |d| moncurve(2.4, 0.055, d)),
            flags: flags(gamma),
        },
        Case {
            label: "basic gammas",
            v1: false,
            children: nested(|d| exponent(2.2, d), |d| exponent(1.8, d)),
            flags: flags(gamma),
        },
        Case {
            label: "logs",
            v1: false,
            children: nested(|d| log(2.0, d), |d| log(10.0, d)),
            flags: flags(log_flag),
        },
        Case {
            label: "affine logs",
            v1: false,
            children: nested(|d| log_affine(0.3, 0.1, d), |d| log_affine(0.5, 0.2, d)),
            flags: flags(log_flag),
        },
        Case {
            label: "camera logs",
            v1: false,
            children: nested(|d| log_camera(0.1, d), |d| log_camera(0.2, d)),
            flags: flags(log_flag),
        },
        Case {
            label: "no-clamp CDLs",
            v1: false,
            children: nested(
                |d| cdl(1.1, 1.2, CdlStyle::NoClamp, d),
                |d| cdl(0.9, 0.8, CdlStyle::NoClamp, d),
            ),
            flags: flags(cdl_flag),
        },
        Case {
            label: "ASC CDLs",
            v1: false,
            children: nested(
                |d| cdl(1.1, 1.2, CdlStyle::Asc, d),
                |d| cdl(0.9, 0.8, CdlStyle::Asc, d),
            ),
            flags: flags(cdl_flag),
        },
        Case {
            label: "matrices",
            v1: false,
            children: nested(
                |d| offset_matrix([0.1, 0.0, 0.0, 0.0], d),
                |d| offset_matrix([0.0, 0.2, 0.0, 0.0], d),
            ),
            flags: flags(OptimizationFlags::COMP_MATRIX),
        },
        Case {
            label: "exponents in a version 1 config",
            v1: true,
            children: nested(|d| exponent(2.2, d), |d| exponent(1.8, d)),
            flags: flags(OptimizationFlags::COMP_EXPONENT),
        },
        Case {
            label: "a log, a moncurve gamma and a CDL, nested",
            v1: false,
            children: vec![
                log(2.0, Forward),
                moncurve(2.4, 0.055, Forward),
                cdl(1.1, 1.2, CdlStyle::NoClamp, Forward),
                cdl(1.1, 1.2, CdlStyle::NoClamp, Inverse),
                moncurve(2.4, 0.055, Inverse),
                log(2.0, Inverse),
            ],
            flags: vec![
                OptimizationFlags::DEFAULT,
                all_but(gamma),
                all_but(log_flag),
                all_but(cdl_flag),
            ],
        },
        Case {
            label: "a pair between matrices that then combine",
            v1: false,
            children: vec![
                offset_matrix([0.1, 0.0, 0.0, 0.0], Forward),
                moncurve(2.2, 0.1, Forward),
                moncurve(2.2, 0.1, Inverse),
                offset_matrix([0.0, 0.2, 0.0, 0.0], Forward),
            ],
            flags: flags(gamma),
        },
    ];
    check(&cases);
}

/// Two ops of each type that combines, with every flag, the default, and every flag but the
/// type's `COMP_*`.
#[test]
fn combinations_match_the_wheel() {
    let flags = |comp: OptimizationFlags| {
        vec![
            OptimizationFlags::ALL,
            OptimizationFlags::DEFAULT,
            all_but(comp),
        ]
    };
    let cases = vec![
        Case {
            label: "ranges",
            v1: false,
            children: vec![range(0.0, 1.0, 0.1, 0.9), range(0.2, 0.8, 0.0, 1.0)],
            flags: flags(OptimizationFlags::COMP_RANGE),
        },
        Case {
            label: "matrices",
            v1: false,
            children: vec![
                offset_matrix([0.1, 0.0, 0.0, 0.0], Forward),
                offset_matrix([0.0, 0.2, 0.0, 0.0], Forward),
            ],
            flags: flags(OptimizationFlags::COMP_MATRIX),
        },
        Case {
            label: "basic gammas",
            v1: false,
            children: vec![exponent(2.2, Forward), exponent(1.5, Forward)],
            flags: flags(OptimizationFlags::COMP_GAMMA),
        },
        Case {
            label: "exponents in a version 1 config",
            v1: true,
            children: vec![exponent(2.2, Forward), exponent(1.5, Forward)],
            flags: flags(OptimizationFlags::COMP_EXPONENT),
        },
    ];
    check(&cases);
}
