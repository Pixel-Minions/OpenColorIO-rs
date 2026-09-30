// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Gamma renderers against the wheel, bit for bit, with fast math on and off.
//!
//! The oracle builds an ExponentTransform or ExponentWithLinearTransform in a raw config (a
//! version 2 config, so an ExponentTransform becomes a GammaOp, not an ExponentOp) and applies
//! its CPU processor to F32 RGBA pixels. The test builds the op data that upstream's transform
//! and builder produce (each helper cites them) and the renderer that `GetGammaRenderer` picks.
//!
//! For a single Gamma transform at F32 whose exponents are not all 1 (the cases below mix 1.0
//! with other exponents), the processor's op list is that one GammaOp: it is neither a no-op
//! nor an identity, so `RemoveNoOps` and `ReplaceIdentityOps` keep it, there is no pair to
//! combine or cancel, and the separable-prefix bake only applies to integer input bit depths
//! (src/OpenColorIO/OpOptimizers.cpp:559-563 @ v2.5.2).

mod common;

use std::hint::black_box;

use common::{
    Checks, Math, direction_enum, oracle_apply_yaml, probe_rgba, probe_rgba_with, yaml_direction,
    yaml_list,
};
use ocio_ops::open_color_types::NegativeStyle;
use ocio_ops::open_color_types::TransformDirection::{self, Forward, Inverse};
use ocio_ops::ops::gamma::gamma_op_cpu::get_gamma_renderer;
use ocio_ops::ops::gamma::gamma_op_data::{GammaOpData, GammaStyle};
use ocio_ops::ops::gamma::gamma_op_utils::{compute_params_fwd, compute_params_rev};
use ocio_testkit::compare::{assert_pixels_bits_eq, assert_pixels_bits_eq_except_nan_bits};
use serde_json::{Value, json};

const DIRECTIONS: [TransformDirection; 2] = [Forward, Inverse];

fn negative_style_enum(style: NegativeStyle) -> Value {
    let name = match style {
        NegativeStyle::Clamp => "NEGATIVE_CLAMP",
        NegativeStyle::Mirror => "NEGATIVE_MIRROR",
        NegativeStyle::PassThru => "NEGATIVE_PASS_THRU",
        NegativeStyle::Linear => "NEGATIVE_LINEAR",
    };
    json!({ "enum": name })
}

/// The values near each channel's break points and near zero, to add to the probe.
fn neighbours(points: &[f32]) -> Vec<f32> {
    let mut v = Vec::new();
    for &x in points {
        for d in -3i32..=3 {
            v.push(f32::from_bits(x.to_bits().wrapping_add_signed(d)));
        }
    }
    v
}

/// The op data of `ExponentTransform(value, negativeStyle, direction)`.
///
/// `ExponentTransformImpl` holds a default `GammaOpData` (BASIC_FWD, identity parameters;
/// src/OpenColorIO/transforms/ExponentTransform.h:46 and ops/gamma/GammaOpData.cpp:244-252
/// @ v2.5.2). The Python constructor calls `setValue`, `setNegativeStyle` (which converts the
/// style for the current direction) and `setDirection` (which inverts the style if needed)
/// (src/bindings/python/transforms/PyExponentTransform.cpp:16-25;
/// transforms/ExponentTransform.cpp:72-99). `BuildExponentOp` clones the data for a version 2
/// config (ops/gamma/GammaOp.cpp:190-216).
fn exponent_op(value: [f64; 4], neg: NegativeStyle, dir: TransformDirection) -> GammaOpData {
    let mut data = GammaOpData::default();
    data.red_params_mut()[0] = value[0];
    data.green_params_mut()[0] = value[1];
    data.blue_params_mut()[0] = value[2];
    data.alpha_params_mut()[0] = value[3];
    let cur_dir = data.direction();
    data.set_style(GammaOpData::convert_style_basic(neg, cur_dir).unwrap());
    data.set_direction(dir);
    data
}

#[test]
fn exponent_transform_matches_the_wheel() {
    // Each includes an exponent of 1.0, as upstream's apply_basic_style_fwd does.
    let values: [[f64; 4]; 3] = [
        [1.0, 2.2, 0.45, 2.6],
        [2.4, 1.0, 1.8, 0.5],
        [0.01, 100.0, 1.0, 3.3],
    ];
    let mut checks = Checks::default();
    for value in values {
        for neg in [
            NegativeStyle::Clamp,
            NegativeStyle::Mirror,
            NegativeStyle::PassThru,
        ] {
            for dir in DIRECTIONS {
                let spec = json!({
                    "class": "ExponentTransform",
                    "args": {
                        "value": value,
                        "negativeStyle": negative_style_enum(neg),
                        "direction": direction_enum(dir),
                    },
                });
                let data = exponent_op(value, neg, dir);
                let input = probe_rgba_with(&neighbours(&[0.0, 1.0]));
                for math in Math::BOTH {
                    let renderer = get_gamma_renderer(&black_box(data.clone()), math.fast());
                    let label = format!("ExponentTransform {value:?} {neg:?} {dir:?}");
                    checks.check(&label, &spec, math, &input, renderer.as_ref());
                }
            }
        }
    }
    checks.finish();
}

/// The op data of `ExponentWithLinearTransform(gamma, offset, negativeStyle, direction)`.
///
/// `ExponentWithLinearTransformImpl()` sets `{1, 0}` on the four channels and MONCURVE_FWD
/// (src/OpenColorIO/transforms/ExponentWithLinearTransform.cpp:25-33 @ v2.5.2). The Python
/// constructor calls `setGamma`, `setOffset`, `setNegativeStyle` (which converts the style for
/// the current direction) and `setDirection`
/// (src/bindings/python/transforms/PyExponentWithLinearTransform.cpp:25-37;
/// ExponentWithLinearTransform.cpp:91-138). `BuildExponentWithLinearOp` clones the data
/// (ops/gamma/GammaOp.cpp:179-188).
fn exponent_with_linear_op(
    gamma: [f64; 4],
    offset: [f64; 4],
    neg: NegativeStyle,
    dir: TransformDirection,
) -> GammaOpData {
    let mut data = GammaOpData::default();
    data.set_red_params(vec![1.0, 0.0]);
    data.set_green_params(vec![1.0, 0.0]);
    data.set_blue_params(vec![1.0, 0.0]);
    data.set_alpha_params(vec![1.0, 0.0]);
    data.set_style(GammaStyle::MoncurveFwd);
    // setGamma.
    data.red_params_mut()[0] = gamma[0];
    data.green_params_mut()[0] = gamma[1];
    data.blue_params_mut()[0] = gamma[2];
    data.alpha_params_mut()[0] = gamma[3];
    // setOffset.
    let red = vec![data.red_params()[0], offset[0]];
    let grn = vec![data.green_params()[0], offset[1]];
    let blu = vec![data.blue_params()[0], offset[2]];
    let alp = vec![data.alpha_params()[0], offset[3]];
    data.set_red_params(red);
    data.set_green_params(grn);
    data.set_blue_params(blu);
    data.set_alpha_params(alp);
    let cur_dir = data.direction();
    data.set_style(GammaOpData::convert_style_mon_curve(neg, cur_dir).unwrap());
    data.set_direction(dir);
    data
}

#[test]
fn exponent_with_linear_transform_matches_the_wheel() {
    let cases: [([f64; 4], [f64; 4]); 3] = [
        // As upstream's apply_moncurve_style_fwd, with an identity blue channel {1, 0}.
        ([2.4, 2.2, 1.0, 1.8], [0.055, 0.2, 0.0, 0.6]),
        // sRGB, Rec.709 and L* (GammaOpData.h:37-41), and the bounds.
        ([2.4, 1.0 / 0.45, 3.0, 10.0], [0.055, 0.099, 0.16, 0.9]),
        ([1.0, 1.5, 7.5, 2.0], [0.5, 0.0, 0.001, 0.4]),
    ];
    let mut checks = Checks::default();
    for (gamma, offset) in cases {
        for neg in [NegativeStyle::Linear, NegativeStyle::Mirror] {
            for dir in DIRECTIONS {
                let spec = json!({
                    "class": "ExponentWithLinearTransform",
                    "args": {
                        "gamma": gamma,
                        "offset": offset,
                        "negativeStyle": negative_style_enum(neg),
                        "direction": direction_enum(dir),
                    },
                });
                let data = exponent_with_linear_op(gamma, offset, neg, dir);

                // The break points of the forward and reverse curves.
                let mut points = vec![0.0f32];
                for p in [
                    data.red_params(),
                    data.green_params(),
                    data.blue_params(),
                    data.alpha_params(),
                ] {
                    points.push(compute_params_fwd(p).break_pnt);
                    points.push(compute_params_rev(p).break_pnt);
                }
                let input = probe_rgba_with(&neighbours(&points));

                for math in Math::BOTH {
                    let renderer = get_gamma_renderer(&black_box(data.clone()), math.fast());
                    let label =
                        format!("ExponentWithLinearTransform {gamma:?} {offset:?} {neg:?} {dir:?}");
                    checks.check(&label, &spec, math, &input, renderer.as_ref());
                }
            }
        }
    }
    checks.finish();
}

/// The negative style in the config's YAML syntax.
fn yaml_style(style: NegativeStyle) -> &'static str {
    match style {
        NegativeStyle::Clamp => "clamp",
        NegativeStyle::Mirror => "mirror",
        NegativeStyle::PassThru => "pass_thru",
        NegativeStyle::Linear => "linear",
    }
}

/// NaN parameters, which OCIO 2.5.2 accepts (YAML `.nan`), against the wheel.
///
/// Where a NaN parameter meets a NaN pixel value in one operation, the NaN that comes out
/// depends on the operand order in the wheel's machine code. The port follows it where MSVC
/// and GCC agree (`math_utils::sse_add`/`sse_mul`), so every case is compared bit for bit but
/// one. In `GammaMoncurveOpCPUFwd`, GCC multiplies `scale * pixel` in every channel (at
/// 0x384695) and MSVC `pixel * scale` (at 0x1801bea9e), so the port keeps the source order
/// there. For that case (ExponentWithLinearTransform, linear style, forward, fast math off),
/// waiver W0002 applies: in the channels with a NaN parameter, a NaN from the wheel only has
/// to be NaN in the port, and every other value is still compared bit for bit.
#[test]
fn nan_parameters_match_the_wheel_under_waiver_w0002() {
    let nan = f64::NAN;
    // (label, YAML, op data, whether W0002 applies with fast math off)
    let mut cases: Vec<(String, String, GammaOpData, bool)> = Vec::new();
    for dir in DIRECTIONS {
        let value = [2.2, nan, 1.8, 1.0];
        for neg in [
            NegativeStyle::Clamp,
            NegativeStyle::Mirror,
            NegativeStyle::PassThru,
        ] {
            let yaml = format!(
                "!<ExponentTransform> {{value: {}, style: {}, direction: {}}}",
                yaml_list(&value),
                yaml_style(neg),
                yaml_direction(dir)
            );
            let label = format!("ExponentTransform {neg:?} {dir:?}");
            cases.push((label, yaml, exponent_op(value, neg, dir), false));
        }
        let (gamma, offset) = ([2.4, nan, 2.2, 1.8], [0.055, 0.1, nan, 0.2]);
        for neg in [NegativeStyle::Linear, NegativeStyle::Mirror] {
            let yaml = format!(
                "!<ExponentWithLinearTransform> {{gamma: {}, offset: {}, style: {}, \
                 direction: {}}}",
                yaml_list(&gamma),
                yaml_list(&offset),
                yaml_style(neg),
                yaml_direction(dir)
            );
            let label = format!("ExponentWithLinearTransform {neg:?} {dir:?}");
            let data = exponent_with_linear_op(gamma, offset, neg, dir);
            let w0002 = neg == NegativeStyle::Linear && dir == Forward;
            cases.push((label, yaml, data, w0002));
        }
    }

    let input = probe_rgba();
    for (label, yaml, data, w0002) in cases {
        let params = [
            data.red_params(),
            data.green_params(),
            data.blue_params(),
            data.alpha_params(),
        ];
        let waived_channels = params.map(|p| p.iter().any(|v| v.is_nan()));
        for math in Math::BOTH {
            let expected = oracle_apply_yaml(&yaml, math, input);
            let mut actual = input.to_vec();
            get_gamma_renderer(&black_box(data.clone()), math.fast()).apply(&mut actual);
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
