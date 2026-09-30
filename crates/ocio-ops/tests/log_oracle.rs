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
//!
//! The op data goes through `std::hint::black_box`, so that the compiler cannot evaluate the
//! renderers' constructors on the tests' constant parameters: it folds, for example, `log2` of
//! a negative constant to a positive NaN, where the math library returns the negative x86
//! default NaN at run time.

mod common;

use std::hint::black_box;

use common::{
    Checks, Math, direction_enum, oracle_apply_yaml, probe_rgba, probe_rgba_with, yaml_direction,
    yaml_list, yaml_number,
};
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
use ocio_testkit::compare::{assert_pixels_bits_eq, assert_pixels_bits_eq_except_nan_bits};
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
            let data = black_box(log_transform_op(base, dir));
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

    /// The transform in the config's YAML syntax, which can hold NaN parameters.
    fn yaml(&self, dir: TransformDirection) -> String {
        format!(
            "!<LogAffineTransform> {{base: {}, log_side_slope: {}, log_side_offset: {}, \
             lin_side_slope: {}, lin_side_offset: {}, direction: {}}}",
            yaml_number(self.base),
            yaml_list(&self.log_side_slope),
            yaml_list(&self.log_side_offset),
            yaml_list(&self.lin_side_slope),
            yaml_list(&self.lin_side_offset),
            yaml_direction(dir)
        )
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
            let data = black_box(case.op(dir));
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

    /// The transform in the config's YAML syntax, which can hold NaN parameters.
    fn yaml(&self, dir: TransformDirection) -> String {
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
            yaml_direction(dir)
        )
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
        let data = black_box(self.op(Forward));
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
        negative_break(),
    ]
}

/// A negative break: log2 of a negative number. The break on the log side is NaN, with the
/// sign each platform's log2 gives (see log_utils::log2_glibc_2_2_5), and so is the offset of
/// the linear segment.
fn negative_break() -> Camera {
    Camera {
        name: "negative break",
        base: 2.0,
        lin_side_break: [-0.05, -0.1, -0.2],
        log_side_slope: [1.0; 3],
        log_side_offset: [0.0; 3],
        lin_side_slope: [1.0; 3],
        lin_side_offset: [0.0; 3],
        linear_slope: None,
    }
}

/// The platform variants of `GetLogSideBreak` differ for some of the camera cases, so the
/// camera test distinguishes them: it passes only with the variant of the platform it runs on.
#[test]
fn camera_cases_distinguish_the_log_side_break_variants() {
    let mut differ = Vec::new();
    for case in camera_cases() {
        let data = black_box(case.op(Forward));
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

/// NaN pixels meet the NaN offset of the negative break's linear segment, which the SSE
/// renderers compute for every pixel: x86 returns the first operand's NaN, the pixel's in
/// `_mm_add_ps(pixel, offset)` and `_mm_mul_ps(pixel, slope)`. Every buffer length from 1 to 24
/// pixels, so that each part of the port's loops (in a release build, a vectorized body and a
/// scalar remainder) meets the NaNs.
#[test]
fn nan_pixels_meet_nan_offsets_at_every_buffer_length() {
    let case = negative_break();
    let nans = [0xffc0_0000u32, 0xffc1_2345, 0x7fc1_2345, 0xff80_0001].map(f32::from_bits);
    let mut checks = Checks::default();
    for dir in DIRECTIONS {
        let data = black_box(case.op(dir));
        for math in Math::BOTH {
            let renderer = get_log_renderer(&data, math.fast());
            for n in 1..=24usize {
                let input: Vec<f32> = (0..n)
                    .flat_map(|i| [nans[i % 4], nans[(i + 1) % 4], nans[(i + 2) % 4], 0.25])
                    .collect();
                let label = format!("LogCameraTransform {} {dir:?}, {n} pixels", case.name);
                checks.check(&label, &case.spec(dir), math, &input, renderer.as_ref());
            }
        }
    }
    checks.finish();
}

#[test]
fn log_camera_transform_matches_the_wheel() {
    let mut checks = Checks::default();
    for case in camera_cases() {
        let input = probe_rgba_with(&case.break_points());
        for dir in DIRECTIONS {
            let data = black_box(case.op(dir));
            for math in Math::BOTH {
                let renderer = get_log_renderer(&data, math.fast());
                let label = format!("LogCameraTransform {} {dir:?}", case.name);
                checks.check(&label, &case.spec(dir), math, &input, renderer.as_ref());
            }
        }
    }
    checks.finish();
}

/// LogAffineTransforms whose finite parameters overflow `float` (|value| >= FLT_MAX, or a base
/// below the smallest subnormal), so that coefficients are infinite or NaN (`inf / inf`).
fn extreme_affine_cases() -> [Affine; 3] {
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
}

/// LogCameraTransforms whose finite parameters overflow `float` or `double`, so that the
/// break, the linear slope and the linear offset are infinite or NaN, with different NaN
/// signs: the default NaN of `inf / inf` is negative, a negated one positive, and on Linux
/// glibc's `log2` of a negative number positive.
fn extreme_camera_cases() -> Vec<Camera> {
    let camera = |name,
                  base,
                  lin_side_break,
                  log_side_slope,
                  log_side_offset,
                  lin_side_slope,
                  lin_side_offset,
                  linear_slope| Camera {
        name,
        base,
        lin_side_break,
        log_side_slope,
        log_side_offset,
        lin_side_slope,
        lin_side_offset,
        linear_slope,
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
/// parameters only). Their NaN coefficients reach the sites where the wheels' machine code
/// does not use upstream's source order: the camera log-to-lin linear segment (both wheels)
/// and `GetLogSideBreak` (Linux).
#[test]
fn extreme_finite_parameters_match_the_wheel() {
    let mut checks = Checks::default();
    for (i, case) in extreme_affine_cases().iter().enumerate() {
        for dir in DIRECTIONS {
            let data = black_box(case.op(dir));
            for math in Math::BOTH {
                let renderer = get_log_renderer(&data, math.fast());
                let label = format!("extreme LogAffineTransform {i} {dir:?}");
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
    for case in extreme_camera_cases() {
        let input = probe_rgba_with(&case.break_points());
        for dir in DIRECTIONS {
            let data = black_box(case.op(dir));
            for math in Math::BOTH {
                let renderer = get_log_renderer(&data, math.fast());
                let label = format!("extreme LogCameraTransform {} {dir:?}", case.name);
                checks.check(&label, &case.spec(dir), math, &input, renderer.as_ref());
            }
        }
    }
    checks.finish();
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
#[test]
fn nan_parameters_match_the_wheel_under_waiver_w0002() {
    let nan = f64::NAN;
    let affine = [
        Affine {
            base: 10.0,
            log_side_slope: [nan, 0.5, 1.0],
            log_side_offset: [0.1, nan, 0.2],
            lin_side_slope: [1.0, 1.0, nan],
            lin_side_offset: [nan, 0.01, 0.1],
        },
        Affine {
            base: nan,
            log_side_slope: [0.3, 0.5, 1.0],
            log_side_offset: [0.1, 0.2, 0.3],
            lin_side_slope: [1.0; 3],
            lin_side_offset: [0.0; 3],
        },
    ];
    let camera = Camera {
        name: "NaN parameters",
        base: 2.0,
        lin_side_break: [0.1, nan, 0.2],
        log_side_slope: [0.25, 0.3, nan],
        log_side_offset: [0.5, nan, 0.6],
        lin_side_slope: [1.0; 3],
        lin_side_offset: [nan, 0.02, 0.01],
        linear_slope: Some([1.2, 1.0, nan]),
    };
    // (label, YAML, op data, whether W0002 applies with fast math off)
    let mut cases: Vec<(String, String, LogOpData, bool)> = Vec::new();
    for dir in DIRECTIONS {
        for (i, a) in affine.iter().enumerate() {
            let w0002 = i == 0 && dir == Inverse;
            let label = format!("LogAffineTransform {i} {dir:?}");
            cases.push((label, a.yaml(dir), a.op(dir), w0002));
        }
        let label = format!("LogCameraTransform {dir:?}");
        cases.push((label, camera.yaml(dir), camera.op(dir), false));
    }

    let input = probe_rgba();
    for (label, yaml, data, w0002) in cases {
        // The channels with a NaN parameter or a NaN base; the op passes alpha through.
        let params = [data.red_params(), data.green_params(), data.blue_params()];
        let nan_channel = |c: usize| data.base().is_nan() || params[c].iter().any(|p| p.is_nan());
        let waived_channels = [nan_channel(0), nan_channel(1), nan_channel(2), false];
        for math in Math::BOTH {
            let expected = oracle_apply_yaml(&yaml, math, input);
            let mut actual = input.to_vec();
            get_log_renderer(&black_box(data.clone()), math.fast()).apply(&mut actual);
            let label = format!("{label} ({math:?}), {yaml}");
            if w0002 && math == Math::Exact {
                let waived = assert_pixels_bits_eq_except_nan_bits(
                    &label,
                    "W0002",
                    &waived_channels,
                    input,
                    &expected,
                    &actual,
                );
                println!("{label}: {waived} NaN values differ in their bits only (W0002)");
            } else {
                assert_pixels_bits_eq(&label, input, 4, &expected, &actual);
            }
        }
    }
}

/// The scalar integer casts (`Converter<BD>::CastValue`), against the wheel: a LogTransform's
/// processor at F32 in and an integer bit depth out ends with `BitDepthCast<F32, BD>`, which
/// converts `value * maxValue` (src/OpenColorIO/CPUProcessor.cpp:20-48 @ v2.5.2). The Log op
/// passes alpha through, so alpha shows the cast alone, and RGB shows it after the log.
#[test]
fn integer_output_casts_match_the_wheel() {
    let input = probe_rgba();
    let data = black_box(log_transform_op(2.0, Forward));
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
