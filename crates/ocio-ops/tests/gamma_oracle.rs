// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The Gamma renderers against the wheel, bit for bit, through the oracle test battery
//! (`ocio_testkit::battery`): every case in both directions, with fast math on and off, on
//! the tier's probe sets (`OCIO_RS_TIER`).
//!
//! The oracle builds an ExponentTransform or ExponentWithLinearTransform in a raw config (a
//! version 2 config, so an ExponentTransform becomes a GammaOp, not an ExponentOp) and applies
//! its CPU processor to F32 RGBA pixels. The families build the op data that upstream's
//! transform and builder produce (`common::gamma` cites them) and the renderer that
//! `GetGammaRenderer` picks.
//!
//! The wheel refuses parameters out of `GammaOpData::validate`'s bounds, and the port gives
//! the same texts:
//! - a JSON spec's transform comes from the binding's constructor, which validates it with
//!   the transform's prefix (`ExponentTransform validation failed: `,
//!   src/OpenColorIO/transforms/ExponentTransform.cpp:40-53 @ v2.5.2, and
//!   ExponentWithLinearTransform.cpp:60-73);
//! - a YAML spec's transform is read without validating (src/OpenColorIO/OCIOYaml.cpp:
//!   917-1140), and the processor between the config's color spaces builds its op with
//!   `BuildExponentOp` or `BuildExponentWithLinearOp`, which validate the data without a
//!   prefix (src/OpenColorIO/ops/gamma/GammaOp.cpp:179-216).
//!
//! For a single Gamma transform at F32 whose exponents are not all 1 (the cases below mix 1.0
//! with other exponents), the processor's op list is that one GammaOp: it is neither a no-op
//! nor an identity, so `RemoveNoOps` and `ReplaceIdentityOps` keep it, there is no pair to
//! combine or cancel, and the separable-prefix bake only applies to integer input bit depths
//! (src/OpenColorIO/OpOptimizers.cpp:559-563 @ v2.5.2).
//!
//! Gamma ops have alpha parameters, so no channel passes through, and the renderers have no
//! numeric profiles: the fast-math kernels are SSE2 code, which every x86-64 CPU runs.

mod common;

use std::hint::black_box;

use common::gamma::{
    exponent_op, exponent_with_linear_op, negative_style_enum, yaml_direction, yaml_style,
};
use ocio_ops::open_color_types::{NegativeStyle, TransformDirection};
use ocio_ops::ops::gamma::gamma_op_cpu::get_gamma_renderer;
use ocio_ops::ops::gamma::gamma_op_data::GammaOpData;
use ocio_ops::ops::gamma::gamma_op_utils::{compute_params_fwd, compute_params_rev};
use ocio_testkit::battery::params::{Case, Params, Precision, Slot};
use ocio_testkit::battery::{self, Combo, Direction, Family, Port, Spec, Validation, yaml_list};
use serde_json::json;

/// The port's direction for the battery's.
fn port_direction(direction: Direction) -> TransformDirection {
    match direction {
        Direction::Forward => TransformDirection::Forward,
        Direction::Inverse => TransformDirection::Inverse,
    }
}

/// The renderer `GetGammaRenderer` picks for `data`, as a battery port; or the wheel's
/// refusal. `prefix` is the transform's validation prefix where the spec is JSON (the
/// binding's constructor validates the transform), `None` where it is YAML.
fn gamma_port(data: GammaOpData, prefix: Option<&str>, combo: &Combo) -> Result<Port, String> {
    if let Some(prefix) = prefix {
        data.validate()
            .map_err(|e| format!("{prefix}{}", e.message()))?;
    }
    // BuildExponentOp, BuildExponentWithLinearOp: `data.validate()`.
    data.validate().map_err(|e| e.message().to_string())?;
    let renderer = get_gamma_renderer(&black_box(data), combo.fast_math)
        .map_err(|e| e.message().to_string())?;
    Ok(Port::in_place(move |px| renderer.apply(px)))
}

/// Whether every slot of `params` is finite: then a JSON transform spec can hold them.
fn all_finite<P: Params>(params: &P) -> bool {
    (0..params.slots().len()).all(|i| params.get(i).is_finite())
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
                yaml_direction(port_direction(dir))
            ))
        }
    }
    fn port(&self, p: &Exponent, combo: &Combo) -> Result<Port, String> {
        let prefix = all_finite(p).then_some("ExponentTransform validation failed: ");
        gamma_port(
            exponent_op(p.value, p.style, port_direction(combo.direction)),
            prefix,
            combo,
        )
    }
    /// The values near zero and one.
    fn breakpoints(&self, _: &Exponent, _: Direction) -> Vec<f32> {
        vec![0.0, 1.0]
    }
    fn validation(&self) -> Validation {
        Validation::Ported
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
    // Refusals, on both routes: JSON, then YAML for the infinite value.
    for value in [
        [2.2, 0.006, 1.0, 1.0],
        [2.2, 2.2, 1.0, 110.0],
        [f64::INFINITY, 2.2, 1.0, 1.0],
    ] {
        cases.push(Case::new(
            format!("refused {value:?}"),
            Exponent {
                value,
                style: NegativeStyle::Mirror,
            },
        ));
    }
    // An explicit case on the YAML spec the generated NaN and infinite cases take.
    let [nan_clamp, _, _] = exponent_nan_cases();
    cases.push(nan_clamp);
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
                yaml_direction(port_direction(dir))
            ))
        }
    }
    fn port(&self, p: &ExponentWithLinear, combo: &Combo) -> Result<Port, String> {
        let dir = port_direction(combo.direction);
        let prefix = all_finite(p).then_some("ExponentWithLinearTransform validation failed: ");
        gamma_port(
            exponent_with_linear_op(p.gamma, p.offset, p.style, dir),
            prefix,
            combo,
        )
    }
    /// Zero and the break points of the forward and reverse curves.
    fn breakpoints(&self, p: &ExponentWithLinear, dir: Direction) -> Vec<f32> {
        let data = exponent_with_linear_op(p.gamma, p.offset, p.style, port_direction(dir));
        let mut points = vec![0.0f32];
        for params in data.all_params() {
            points.push(compute_params_fwd(params).unwrap().break_pnt);
            points.push(compute_params_rev(params).unwrap().break_pnt);
        }
        points
    }
    fn validation(&self) -> Validation {
        Validation::Ported
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
    // Refusals, on both routes: JSON, then YAML for the infinite values.
    for (gamma, offset) in [
        ([2.4, 0.5, 2.2, 1.8], [0.055, 0.1, 0.1, 0.1]),
        ([2.4, 2.2, 2.2, 1.8], [0.055, 0.1, 0.1, 1.0]),
        ([2.4, 2.2, 2.2, 1.8], [0.055, f64::NEG_INFINITY, 0.1, 0.1]),
        ([2.4, f64::INFINITY, 2.2, 1.8], [0.055, 0.1, 0.1, 0.1]),
    ] {
        cases.push(Case::new(
            format!("refused {gamma:?} {offset:?}"),
            ExponentWithLinear {
                gamma,
                offset,
                style: NegativeStyle::Linear,
            },
        ));
    }
    // An explicit case on the YAML spec the generated NaN and ±Inf cases take.
    let [_, nan_mirror] = exponent_with_linear_nan_cases();
    cases.push(nan_mirror);
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
/// generated NaN cases take W0002 in every combination.)
#[test]
fn nan_parameters_match_the_wheel_under_waiver_w0002() {
    battery::run(&ExponentFamily {
        cases: exponent_nan_cases().to_vec(),
        bases: Vec::new(),
    });
    battery::run(&ExponentWithLinearFamily {
        cases: exponent_with_linear_nan_cases().to_vec(),
        bases: Vec::new(),
    });
}

/// ExponentTransforms with a NaN value, one per negative style, bit for bit everywhere.
fn exponent_nan_cases() -> [Case<Exponent>; 3] {
    [
        NegativeStyle::Clamp,
        NegativeStyle::Mirror,
        NegativeStyle::PassThru,
    ]
    .map(|style| {
        Case::new(
            format!("NaN value {style:?}"),
            Exponent {
                value: [2.2, f64::NAN, 1.8, 1.0],
                style,
            },
        )
        .w0002_nowhere()
    })
}

/// ExponentWithLinearTransforms with a NaN gamma and a NaN offset: the linear style compares
/// under W0002 only where GCC and MSVC multiply in different orders (forward, fast math off;
/// see `nan_parameters_match_the_wheel_under_waiver_w0002`), the mirror style bit for bit
/// everywhere.
fn exponent_with_linear_nan_cases() -> [Case<ExponentWithLinear>; 2] {
    let nan = f64::NAN;
    let (gamma, offset) = ([2.4, nan, 2.2, 1.8], [0.055, 0.1, nan, 0.2]);
    let forward_exact = |c: &Combo| c.direction == Direction::Forward && !c.fast_math;
    [
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
    ]
}
