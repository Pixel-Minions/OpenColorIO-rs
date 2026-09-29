// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Log renderers against the wheel, bit for bit, with fast math on and off.
//!
//! The oracle builds a LogTransform, LogAffineTransform or LogCameraTransform in a raw config
//! and applies its CPU processor to F32 RGBA pixels. The test builds the op data that
//! upstream's transform and `BuildLogOp` produce (each helper cites them) and the renderer
//! that `GetLogRenderer` picks.
//!
//! For a single Log transform at F32, the processor's op list is that one LogOp: `LogOpData`
//! is never a no-op or an identity (`LogOpData::isNoOp`/`isIdentity` return false), has no
//! simpler replacement, and the separable-prefix bake only applies to integer input bit
//! depths (`OptimizeSeparablePrefix`, src/OpenColorIO/OpOptimizers.cpp:559-563 @ v2.5.2).

mod common;

use common::{Checks, Math, direction_enum, probe_rgba, probe_rgba_with};
use ocio_ops::math_utils::{
    cast_value_uint8, cast_value_uint10, cast_value_uint12, cast_value_uint16,
};
use ocio_ops::open_color_types::TransformDirection::{self, Forward, Inverse};
use ocio_ops::ops::log::log_op_cpu::get_log_renderer;
use ocio_ops::ops::log::log_op_data::{LogAffineParameter, LogOpData};
use ocio_ops::ops::log::log_utils::{
    get_log_side_break, get_log_side_break_libstdcxx, get_log_side_break_msvc,
};
use ocio_testkit::Oracle;
use ocio_testkit::oracle::f32_to_bytes;
use serde_json::{Value, json};

const DIRECTIONS: [TransformDirection; 2] = [Forward, Inverse];

/// The op data of `LogTransform(base, direction)` in a forward processor: default parameters.
///
/// `LogTransformImpl()` holds `LogOpData(2.0f, TRANSFORM_DIR_FORWARD)`
/// (src/OpenColorIO/transforms/LogTransform.cpp:25-28 @ v2.5.2); the Python constructor calls
/// `setBase` and `setDirection` (src/bindings/python/transforms/PyLogTransform.cpp:20-27); and
/// `BuildLogOp` clones the data, inverting it only for an inverse processor
/// (src/OpenColorIO/ops/log/LogOp.cpp:142-153, 212-221).
fn log_transform_op(base: f64, dir: TransformDirection) -> LogOpData {
    let mut data = LogOpData::new(2.0, Forward);
    data.set_base(base);
    data.set_direction(dir);
    data
}

#[test]
fn log_transform_matches_the_wheel() {
    let mut checks = Checks::default();
    // 2 and 10 are the Log2/Log10 renderers; others use LinToLog and LogToLin.
    for base in [2.0, 10.0, std::f64::consts::E, 3.7, 0.5] {
        for dir in DIRECTIONS {
            let spec = json!({
                "class": "LogTransform",
                "args": {"base": base, "direction": direction_enum(dir)},
            });
            let data = log_transform_op(base, dir);
            for math in Math::BOTH {
                let renderer = get_log_renderer(&data, math.fast());
                let label = format!("LogTransform base {base} {dir:?}");
                checks.check(&label, &spec, math, probe_rgba(), renderer.as_ref());
            }
        }
    }
    checks.finish();
}

/// The parameters of a LogAffineTransform.
struct Affine {
    base: f64,
    log_side_slope: [f64; 3],
    log_side_offset: [f64; 3],
    lin_side_slope: [f64; 3],
    lin_side_offset: [f64; 3],
}

impl Affine {
    fn spec(&self, dir: TransformDirection) -> Value {
        json!({
            "class": "LogAffineTransform",
            "args": {
                "logSideSlope": self.log_side_slope,
                "logSideOffset": self.log_side_offset,
                "linSideSlope": self.lin_side_slope,
                "linSideOffset": self.lin_side_offset,
                "direction": direction_enum(dir),
            },
            "calls": [["setBase", self.base]],
        })
    }

    /// `LogAffineTransformImpl()` holds `LogOpData(2.0f, TRANSFORM_DIR_FORWARD)`
    /// (src/OpenColorIO/transforms/LogAffineTransform.cpp:25-28 @ v2.5.2); the Python
    /// constructor sets the four values and the direction
    /// (src/bindings/python/transforms/PyLogAffineTransform.cpp:31-45), `setBase` sets the base
    /// (LogAffineTransform.cpp:78-81), and `BuildLogOp` clones the data
    /// (src/OpenColorIO/ops/log/LogOp.cpp:190-199).
    fn op(&self, dir: TransformDirection) -> LogOpData {
        let mut data = LogOpData::new(2.0, Forward);
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

#[test]
fn log_affine_transform_matches_the_wheel() {
    let cases = [
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
    ];
    let mut checks = Checks::default();
    for (i, case) in cases.iter().enumerate() {
        for dir in DIRECTIONS {
            let data = case.op(dir);
            for math in Math::BOTH {
                let renderer = get_log_renderer(&data, math.fast());
                let label = format!("LogAffineTransform case {i} {dir:?}");
                checks.check(
                    &label,
                    &case.spec(dir),
                    math,
                    probe_rgba(),
                    renderer.as_ref(),
                );
            }
        }
    }
    checks.finish();
}

/// The parameters of a LogCameraTransform.
struct Camera {
    name: &'static str,
    base: f64,
    lin_side_break: [f64; 3],
    log_side_slope: [f64; 3],
    log_side_offset: [f64; 3],
    lin_side_slope: [f64; 3],
    lin_side_offset: [f64; 3],
    linear_slope: Option<[f64; 3]>,
}

impl Camera {
    fn spec(&self, dir: TransformDirection) -> Value {
        json!({
            "class": "LogCameraTransform",
            "args": {
                "linSideBreak": self.lin_side_break,
                "base": self.base,
                "logSideSlope": self.log_side_slope,
                "logSideOffset": self.log_side_offset,
                "linSideSlope": self.lin_side_slope,
                "linSideOffset": self.lin_side_offset,
                "linearSlope": self.linear_slope.map_or(json!([]), |s| json!(s)),
                "direction": direction_enum(dir),
            },
        })
    }

    /// `LogCameraTransformImpl(linSideBreak)` holds `LogOpData(2.0f, TRANSFORM_DIR_FORWARD)`
    /// with the break set (src/OpenColorIO/transforms/LogCameraTransform.cpp:26-30 @ v2.5.2);
    /// the Python constructor sets the base, the four values, the linear slope when given and
    /// the direction (src/bindings/python/transforms/PyLogCameraTransform.cpp:37-63); and
    /// `BuildLogOp` clones the data (src/OpenColorIO/ops/log/LogOp.cpp:201-210).
    fn op(&self, dir: TransformDirection) -> LogOpData {
        let mut data = LogOpData::new(2.0, Forward);
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

    /// The break points on both sides and their neighbours, to add to the probe.
    fn break_points(&self) -> Vec<f32> {
        let data = self.op(Forward);
        let base = self.base as f32;
        let params = [data.red_params(), data.green_params(), data.blue_params()];
        let mut v = Vec::new();
        for (lin_side_break, p) in self.lin_side_break.iter().zip(params) {
            let lin = *lin_side_break as f32;
            let log = get_log_side_break(p, f64::from(base));
            for x in [lin, log] {
                for d in -3i32..=3 {
                    v.push(f32::from_bits(x.to_bits().wrapping_add_signed(d)));
                }
            }
        }
        v
    }
}

fn camera_cases() -> Vec<Camera> {
    // ARRI LogC3 (EI 800) as a LogCameraTransform.
    let logc3 = Camera {
        name: "LogC3 EI800",
        base: 10.0,
        lin_side_break: [0.010591; 3],
        log_side_slope: [0.247190; 3],
        log_side_offset: [0.385537; 3],
        lin_side_slope: [5.555556; 3],
        lin_side_offset: [0.052272; 3],
        linear_slope: Some([5.367655; 3]),
    };
    let rgb = Camera {
        name: "per channel",
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
        name: "per channel, base 10",
        base: 10.0,
        lin_side_break: [0.010591; 3],
        log_side_slope: [0.24719; 3],
        log_side_offset: [0.385537, 0.6, 0.0],
        lin_side_slope: [5.555556, 1.0, 1.0],
        lin_side_offset: [0.05, 0.05, 0.0],
        linear_slope: Some([5.367655, 1.1, 0.9]),
    };
    vec![
        Camera {
            name: "LogC3 EI800, computed linear slope",
            linear_slope: None,
            ..logc3
        },
        logc3,
        Camera {
            name: "per channel, computed linear slope",
            linear_slope: None,
            ..rgb
        },
        rgb,
        Camera {
            name: "per channel, base 10, computed linear slope",
            linear_slope: None,
            ..rgb10
        },
        rgb10,
        Camera {
            name: "base e",
            base: std::f64::consts::E,
            lin_side_break: [0.18, 0.02, 0.3],
            log_side_slope: [0.3, 0.3, 0.3],
            log_side_offset: [0.5, 0.45, 0.55],
            lin_side_slope: [3.0, 2.5, 4.0],
            lin_side_offset: [0.01, 0.03, 0.005],
            linear_slope: None,
        },
        // A negative break: log2 of a negative number. The break on the log side is NaN, with
        // the sign each platform's log2 gives (see log_utils::log2_glibc_2_2_5).
        Camera {
            name: "negative break",
            base: 2.0,
            lin_side_break: [-0.05, -0.1, -0.2],
            log_side_slope: [1.0; 3],
            log_side_offset: [0.0; 3],
            lin_side_slope: [1.0; 3],
            lin_side_offset: [0.0; 3],
            linear_slope: None,
        },
    ]
}

/// The platform variants of `GetLogSideBreak` differ for some of the camera cases, so the
/// camera test distinguishes them: it passes only with the variant of the platform it runs on.
#[test]
fn camera_cases_distinguish_the_log_side_break_variants() {
    let mut differ = Vec::new();
    for case in camera_cases() {
        let data = case.op(Forward);
        let base = f64::from(case.base as f32);
        for (c, params) in [data.red_params(), data.green_params(), data.blue_params()]
            .into_iter()
            .enumerate()
        {
            let msvc = get_log_side_break_msvc(params, base);
            let libstdcxx = get_log_side_break_libstdcxx(params, base);
            if msvc.to_bits() != libstdcxx.to_bits() {
                differ.push(format!(
                    "{} channel {c}: {msvc:e} ({:#010x}) vs {libstdcxx:e} ({:#010x})",
                    case.name,
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
    println!(
        "{}",
        differ.join(
            "
"
        )
    );
}

#[test]
fn log_camera_transform_matches_the_wheel() {
    let mut checks = Checks::default();
    for case in camera_cases() {
        let input = probe_rgba_with(&case.break_points());
        for dir in DIRECTIONS {
            let data = case.op(dir);
            for math in Math::BOTH {
                let renderer = get_log_renderer(&data, math.fast());
                let label = format!("LogCameraTransform {} {dir:?}", case.name);
                checks.check(&label, &case.spec(dir), math, &input, renderer.as_ref());
            }
        }
    }
    checks.finish();
}

/// The scalar integer casts (`Converter<BD>::CastValue`), against the wheel: a LogTransform's
/// processor at F32 in and an integer bit depth out ends with `BitDepthCast<F32, BD>`, which
/// converts `value * maxValue` (src/OpenColorIO/CPUProcessor.cpp:20-48 @ v2.5.2). The Log op
/// passes alpha through, so alpha shows the cast alone, and RGB shows it after the log.
#[test]
fn integer_output_casts_match_the_wheel() {
    let input = probe_rgba();
    let data = log_transform_op(2.0, Forward);
    let mut log_out = input.to_vec();
    get_log_renderer(&data, true).apply(&mut log_out);

    // (bit depth, maxValue from BitDepthUtils.h:34-60, cast)
    type Cast = fn(f32) -> u16;
    let depths: [(&str, u16, Cast); 4] = [
        ("BIT_DEPTH_UINT8", 255, |v| u16::from(cast_value_uint8(v))),
        ("BIT_DEPTH_UINT10", 1023, cast_value_uint10),
        ("BIT_DEPTH_UINT12", 4095, cast_value_uint12),
        ("BIT_DEPTH_UINT16", 65535, cast_value_uint16),
    ];
    let mut checks = Checks::default();
    for (depth, max_value, cast) in depths {
        let spec = json!({"class": "LogTransform", "args": {"base": 2.0}});
        let args = json!({"transform": spec, "out_bitdepth": depth});
        let resp = Oracle::get().call("cpu_apply", args, &[&f32_to_bytes(input)]);
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
        let failure = (expected.len() != actual.len() || !mismatches.is_empty()).then(|| {
            format!(
                "{depth}: {} of {} values differ\n{}",
                expected.iter().zip(&actual).filter(|(e, a)| e != a).count(),
                expected.len(),
                mismatches.join("\n")
            )
        });
        checks.record(failure);
    }
    checks.finish();
}
