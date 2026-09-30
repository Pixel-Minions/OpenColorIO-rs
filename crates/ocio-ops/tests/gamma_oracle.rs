// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Gamma renderers against the wheel, bit for bit, through the oracle test battery
//! (`ocio_testkit::battery`): every case in both directions, with fast math on and off, on
//! the tier's probe sets (`OCIO_RS_TIER`).
//!
//! The oracle builds an ExponentTransform or ExponentWithLinearTransform in a raw config (a
//! version 2 config, so an ExponentTransform becomes a GammaOp, not an ExponentOp) and applies
//! its CPU processor to F32 RGBA pixels. The families build the op data that upstream's
//! transform and builder produce (each helper cites them) and the renderer that
//! `GetGammaRenderer` picks.
//!
//! For a single Gamma transform at F32 whose exponents are not all 1 (the cases below mix 1.0
//! with other exponents), the processor's op list is that one GammaOp: it is neither a no-op
//! nor an identity, so `RemoveNoOps` and `ReplaceIdentityOps` keep it, there is no pair to
//! combine or cancel, and the separable-prefix bake only applies to integer input bit depths
//! (src/OpenColorIO/OpOptimizers.cpp:559-563 @ v2.5.2).
//!
//! Gamma ops have alpha parameters, so no channel passes through.

use std::hint::black_box;

use ocio_ops::open_color_types::{NegativeStyle, TransformDirection};
use ocio_ops::ops::gamma::gamma_op_cpu::get_gamma_renderer;
use ocio_ops::ops::gamma::gamma_op_data::{GammaOpData, GammaStyle};
use ocio_ops::ops::gamma::gamma_op_utils::{compute_params_fwd, compute_params_rev};
use ocio_testkit::battery::params::{Case, Params, Precision, Slot};
use ocio_testkit::battery::{self, Combo, Direction, Family, Port, Spec, Validation, yaml_list};
use serde_json::{Value, json};

/// The port's direction for the battery's.
fn port_direction(direction: Direction) -> TransformDirection {
    match direction {
        Direction::Forward => TransformDirection::Forward,
        Direction::Inverse => TransformDirection::Inverse,
    }
}

/// The renderer `GetGammaRenderer` picks for `data`, as a battery port.
fn gamma_port(data: GammaOpData, combo: &Combo) -> Result<Port, String> {
    let renderer = get_gamma_renderer(&black_box(data), combo.fast_math);
    Ok(Port::in_place(move |px| renderer.apply(px)))
}

/// Whether every slot of `params` is finite: then a JSON transform spec can hold them.
fn all_finite<P: Params>(params: &P) -> bool {
    (0..params.slots().len()).all(|i| params.get(i).is_finite())
}

fn negative_style_enum(style: NegativeStyle) -> Value {
    let name = match style {
        NegativeStyle::Clamp => "NEGATIVE_CLAMP",
        NegativeStyle::Mirror => "NEGATIVE_MIRROR",
        NegativeStyle::PassThru => "NEGATIVE_PASS_THRU",
        NegativeStyle::Linear => "NEGATIVE_LINEAR",
    };
    json!({ "enum": name })
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

/// The parameters of an ExponentTransform.
#[derive(Debug, Clone, PartialEq)]
struct Exponent {
    value: [f64; 4],
    style: NegativeStyle,
}

impl Params for Exponent {
    fn slots(&self) -> Vec<Slot> {
        Slot::rgba("value", Precision::F64AsF32).to_vec()
    }
    fn get(&self, i: usize) -> f64 {
        self.value[i]
    }
    fn set(&mut self, i: usize, v: f64) {
        self.value[i] = v;
    }
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

/// ExponentTransform.
struct ExponentFamily {
    cases: Vec<Case<Exponent>>,
    bases: Vec<Case<Exponent>>,
}

impl Family for ExponentFamily {
    type Params = Exponent;

    fn name(&self) -> String {
        "ExponentTransform".to_string()
    }
    fn cases(&self) -> Vec<Case<Exponent>> {
        self.cases.clone()
    }
    fn mutation_bases(&self) -> Vec<Case<Exponent>> {
        self.bases.clone()
    }
    fn spec(&self, p: &Exponent, dir: Direction) -> Spec {
        if all_finite(p) {
            Spec::Transform(json!({
                "class": "ExponentTransform",
                "args": {
                    "value": p.value,
                    "negativeStyle": negative_style_enum(p.style),
                    "direction": dir.oracle_enum(),
                },
            }))
        } else {
            Spec::Yaml(format!(
                "!<ExponentTransform> {{value: {}, style: {}, direction: {}}}",
                yaml_list(&p.value),
                yaml_style(p.style),
                dir.yaml()
            ))
        }
    }
    fn port(&self, p: &Exponent, combo: &Combo) -> Result<Port, String> {
        gamma_port(
            exponent_op(p.value, p.style, port_direction(combo.direction)),
            combo,
        )
    }
    /// The values near zero and one.
    fn breakpoints(&self, _: &Exponent, _: Direction) -> Vec<f32> {
        vec![0.0, 1.0]
    }
    fn validation(&self) -> Validation {
        Validation::NotPorted { card: "WP 1.3g1" }
    }
}

#[test]
fn exponent_transform_matches_the_wheel() {
    // Each includes an exponent of 1.0, as upstream's apply_basic_style_fwd does.
    let values: [[f64; 4]; 3] = [
        [1.0, 2.2, 0.45, 2.6],
        [2.4, 1.0, 1.8, 0.5],
        [0.01, 100.0, 1.0, 3.3],
    ];
    let mut cases = Vec::new();
    let mut bases = Vec::new();
    for style in [
        NegativeStyle::Clamp,
        NegativeStyle::Mirror,
        NegativeStyle::PassThru,
    ] {
        for value in values {
            cases.push(Case::new(
                format!("{value:?} {style:?}"),
                Exponent { value, style },
            ));
        }
        bases.push(cases[cases.len() - 3].clone());
    }
    battery::run(&ExponentFamily { cases, bases });
}

/// The parameters of an ExponentWithLinearTransform.
#[derive(Debug, Clone, PartialEq)]
struct ExponentWithLinear {
    gamma: [f64; 4],
    offset: [f64; 4],
    style: NegativeStyle,
}

impl Params for ExponentWithLinear {
    fn slots(&self) -> Vec<Slot> {
        let mut s = Slot::rgba("gamma", Precision::F64AsF32).to_vec();
        s.extend(Slot::rgba("offset", Precision::F64AsF32));
        s
    }
    fn get(&self, i: usize) -> f64 {
        if i < 4 {
            self.gamma[i]
        } else {
            self.offset[i - 4]
        }
    }
    fn set(&mut self, i: usize, v: f64) {
        if i < 4 {
            self.gamma[i] = v;
        } else {
            self.offset[i - 4] = v;
        }
    }
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

/// ExponentWithLinearTransform.
struct ExponentWithLinearFamily {
    cases: Vec<Case<ExponentWithLinear>>,
    bases: Vec<Case<ExponentWithLinear>>,
}

impl Family for ExponentWithLinearFamily {
    type Params = ExponentWithLinear;

    fn name(&self) -> String {
        "ExponentWithLinearTransform".to_string()
    }
    fn cases(&self) -> Vec<Case<ExponentWithLinear>> {
        self.cases.clone()
    }
    fn mutation_bases(&self) -> Vec<Case<ExponentWithLinear>> {
        self.bases.clone()
    }
    fn spec(&self, p: &ExponentWithLinear, dir: Direction) -> Spec {
        if all_finite(p) {
            Spec::Transform(json!({
                "class": "ExponentWithLinearTransform",
                "args": {
                    "gamma": p.gamma,
                    "offset": p.offset,
                    "negativeStyle": negative_style_enum(p.style),
                    "direction": dir.oracle_enum(),
                },
            }))
        } else {
            Spec::Yaml(format!(
                "!<ExponentWithLinearTransform> {{gamma: {}, offset: {}, style: {}, \
                 direction: {}}}",
                yaml_list(&p.gamma),
                yaml_list(&p.offset),
                yaml_style(p.style),
                dir.yaml()
            ))
        }
    }
    fn port(&self, p: &ExponentWithLinear, combo: &Combo) -> Result<Port, String> {
        let dir = port_direction(combo.direction);
        gamma_port(
            exponent_with_linear_op(p.gamma, p.offset, p.style, dir),
            combo,
        )
    }
    /// Zero and the break points of the forward and reverse curves.
    fn breakpoints(&self, p: &ExponentWithLinear, dir: Direction) -> Vec<f32> {
        let data = exponent_with_linear_op(p.gamma, p.offset, p.style, port_direction(dir));
        let mut points = vec![0.0f32];
        for params in [
            data.red_params(),
            data.green_params(),
            data.blue_params(),
            data.alpha_params(),
        ] {
            points.push(compute_params_fwd(params).break_pnt);
            points.push(compute_params_rev(params).break_pnt);
        }
        points
    }
    fn validation(&self) -> Validation {
        Validation::NotPorted { card: "WP 1.3g1" }
    }
}

#[test]
fn exponent_with_linear_transform_matches_the_wheel() {
    let params: [([f64; 4], [f64; 4]); 3] = [
        // As upstream's apply_moncurve_style_fwd, with an identity blue channel {1, 0}.
        ([2.4, 2.2, 1.0, 1.8], [0.055, 0.2, 0.0, 0.6]),
        // sRGB, Rec.709 and L* (GammaOpData.h:37-41), and the bounds.
        ([2.4, 1.0 / 0.45, 3.0, 10.0], [0.055, 0.099, 0.16, 0.9]),
        ([1.0, 1.5, 7.5, 2.0], [0.5, 0.0, 0.001, 0.4]),
    ];
    let mut cases = Vec::new();
    let mut bases = Vec::new();
    for style in [NegativeStyle::Linear, NegativeStyle::Mirror] {
        for (gamma, offset) in params {
            cases.push(Case::new(
                format!("{gamma:?} {offset:?} {style:?}"),
                ExponentWithLinear {
                    gamma,
                    offset,
                    style,
                },
            ));
        }
        bases.push(cases[cases.len() - 3].clone());
    }
    battery::run(&ExponentWithLinearFamily { cases, bases });
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
/// to be NaN in the port, and every other value is still compared bit for bit. (The battery's
/// generated NaN and ±Inf cases take W0002 in every combination.)
#[test]
fn nan_parameters_match_the_wheel_under_waiver_w0002() {
    let nan = f64::NAN;
    let exponent = [
        NegativeStyle::Clamp,
        NegativeStyle::Mirror,
        NegativeStyle::PassThru,
    ]
    .map(|style| {
        Case::new(
            format!("NaN value {style:?}"),
            Exponent {
                value: [2.2, nan, 1.8, 1.0],
                style,
            },
        )
        .w0002_nowhere()
    })
    .to_vec();
    battery::run(&ExponentFamily {
        cases: exponent,
        bases: Vec::new(),
    });

    let (gamma, offset) = ([2.4, nan, 2.2, 1.8], [0.055, 0.1, nan, 0.2]);
    let forward_exact = |c: &Combo| c.direction == Direction::Forward && !c.fast_math;
    let with_linear = vec![
        Case::new(
            "NaN gamma and offset Linear",
            ExponentWithLinear {
                gamma,
                offset,
                style: NegativeStyle::Linear,
            },
        )
        .w0002_only_where(forward_exact),
        Case::new(
            "NaN gamma and offset Mirror",
            ExponentWithLinear {
                gamma,
                offset,
                style: NegativeStyle::Mirror,
            },
        )
        .w0002_nowhere(),
    ];
    battery::run(&ExponentWithLinearFamily {
        cases: with_linear,
        bases: Vec::new(),
    });
}
