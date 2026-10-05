// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The FixedFunction renderers against the wheel, bit for bit, through the oracle test
//! battery (`ocio_testkit::battery`): every case in both directions, with fast math on and
//! off, on the tier's probe sets (`OCIO_RS_TIER`). So far the ACES 1.x styles (chunk 2.3b).
//!
//! The oracle builds a FixedFunctionTransform in a raw config and applies its CPU processor
//! to F32 RGBA pixels. The family builds the op data that upstream's transform and
//! `BuildFixedFunctionOp` produce and the renderer that `GetFixedFunctionCPURenderer` picks.
//! For a single FixedFunction transform at F32, the processor's op list is that one op:
//! `FixedFunctionOpData` is never a no-op or an identity, the op combines with nothing, and
//! the separable-prefix bake skips F32 input.
//!
//! The op data goes through `std::hint::black_box`, so that the compiler cannot evaluate the
//! renderers' constructors on the tests' constant parameters with another implementation of
//! the math functions.
//!
//! The renderers never write alpha (they work in place), so alpha is a pass-through channel
//! for the battery.
//!
//! The oracle builds every case from a JSON transform spec, with each parameter as its bits
//! (`{"f64": bits}`, `oracle/ocio_oracle/spec.py`), NaN and the infinities included. A
//! config's YAML can't serve the refused cases: a colour space keeps an editable copy of its
//! transform (`ColorSpace::setTransform`), and `FixedFunctionTransformImpl::createEditableCopy`
//! makes it with the validating `FixedFunctionTransform::Create`
//! (src/OpenColorIO/transforms/FixedFunctionTransform.cpp:54-73 @ v2.5.2), so the wheel refuses
//! them while it loads the config, which the battery takes for a broken spec.
//!
//! The wheel refuses parameters out of `FixedFunctionOpData::validate`'s bounds, and the port
//! gives the same texts. The binding's constructor makes the transform's data with the
//! validating constructor `FixedFunctionOpData(Style, const Params &)` in the forward style
//! (no prefix), then sets the direction and validates the transform, with the prefix
//! `FixedFunctionTransform validation failed: `
//! (src/bindings/python/transforms/PyFixedFunctionTransform.cpp:26-42,
//! src/OpenColorIO/transforms/FixedFunctionTransform.cpp:14-47, 86-99 @ v2.5.2);
//! `Config::getProcessor(transform)` validates it again (`Processor::Impl::setTransform`,
//! src/OpenColorIO/Processor.cpp:623-633), and `BuildFixedFunctionOp` validates its data
//! (src/OpenColorIO/ops/fixedfunction/FixedFunctionOp.cpp:180-189).

mod common;

use std::hint::black_box;

use common::fixed_function::style_enum;
use ocio_ops::open_color_types::{FixedFunctionStyle, TransformDirection};
use ocio_ops::ops::fixedfunction::FixedFunctionOpStyle;
use ocio_ops::ops::fixedfunction::fixed_function_op_cpu::get_fixed_function_cpu_renderer;
use ocio_ops::ops::fixedfunction::fixed_function_op_data::FixedFunctionOpData;
use ocio_testkit::battery::params::{A, B, Case, Channels, G, Params, Precision, R, RGB, Slot};
use ocio_testkit::battery::{self, Combo, Direction, Family, Port, Spec, Validation};
use ocio_testkit::transform_text::f64_spec;
use serde_json::Value;
use serde_json::json;

/// The port's direction for the battery's.
fn port_direction(direction: Direction) -> TransformDirection {
    match direction {
        Direction::Forward => TransformDirection::Forward,
        Direction::Inverse => TransformDirection::Inverse,
    }
}

/// The transform's validation prefix (FixedFunctionTransform.cpp:86-99 @ v2.5.2).
const PREFIX: &str = "FixedFunctionTransform validation failed: ";

/// A FixedFunctionTransform's style and parameters.
#[derive(Debug, Clone, PartialEq)]
struct Fixed {
    style: FixedFunctionStyle,
    params: Vec<f64>,
}

impl Params for Fixed {
    fn slots(&self) -> Vec<Slot> {
        // The renderers narrow every parameter to float. The gamut compression's limits and
        // thresholds are per channel (cyan for red, magenta for green, yellow for blue); its
        // power is used by all three.
        let p = Precision::F64AsF32;
        match (self.style, self.params.len()) {
            (FixedFunctionStyle::AcesGamutComp13, 7) => vec![
                Slot::new("lim_cyan", p, R),
                Slot::new("lim_magenta", p, G),
                Slot::new("lim_yellow", p, B),
                Slot::new("thr_cyan", p, R),
                Slot::new("thr_magenta", p, G),
                Slot::new("thr_yellow", p, B),
                Slot::new("power", p, RGB),
            ],
            _ => (0..self.params.len())
                .map(|i| Slot::new(format!("params[{i}]"), p, RGB))
                .collect(),
        }
    }
    fn get(&self, i: usize) -> f64 {
        self.params[i]
    }
    fn set(&mut self, i: usize, value: f64) {
        self.params[i] = value;
    }
}

impl Fixed {
    fn new(style: FixedFunctionStyle, params: &[f64]) -> Self {
        Fixed {
            style,
            params: params.to_vec(),
        }
    }

    /// The op data of the transform the binding's constructor builds (see the module's docs),
    /// or the wheel's refusal while building it or its processor.
    fn transform_data(&self, dir: TransformDirection) -> Result<FixedFunctionOpData, String> {
        let message = |e: ocio_ops::exception::Exception| e.message().to_string();
        let prefixed = |e: ocio_ops::exception::Exception| format!("{PREFIX}{}", e.message());
        let style =
            FixedFunctionOpStyle::from_transform_style(self.style, TransformDirection::Forward)
                .map_err(message)?;
        let mut data =
            FixedFunctionOpData::with_params(style, self.params.clone()).map_err(message)?;
        data.set_direction(dir);
        data.validate().map_err(prefixed)?;
        // `Processor::Impl::setTransform`: `transform->validate()`.
        data.validate().map_err(prefixed)?;
        Ok(data)
    }
}

/// The renderer `GetFixedFunctionCPURenderer` picks for the transform's op, as a battery port;
/// or the wheel's refusal.
fn fixed_port(p: &Fixed, combo: &Combo) -> Result<Port, String> {
    let message = |e: ocio_ops::exception::Exception| e.message().to_string();
    let data = p.transform_data(port_direction(combo.direction))?;
    // BuildFixedFunctionOp: `data.validate()`, then `data.clone()` (which validates) for
    // `CreateFixedFunctionOp` in the processor's forward direction, which keeps the data.
    data.validate().map_err(message)?;
    let data = data.try_clone().map_err(message)?;
    let renderer =
        get_fixed_function_cpu_renderer(&black_box(data), combo.fast_math).map_err(message)?;
    Ok(Port::in_place(move |px| renderer.apply(px)))
}

/// The ACES 1.x styles.
struct AcesFamily {
    cases: Vec<Case<Fixed>>,
    bases: Vec<Case<Fixed>>,
}

impl Family for AcesFamily {
    type Params = Fixed;

    fn name(&self) -> String {
        "FixedFunctionTransform (ACES 1.x)".to_string()
    }
    fn cases(&self) -> Vec<Case<Fixed>> {
        self.cases.clone()
    }
    fn mutation_bases(&self) -> Vec<Case<Fixed>> {
        self.bases.clone()
    }
    fn spec(&self, p: &Fixed, direction: Direction) -> Spec {
        let params: Vec<Value> = p.params.iter().map(|&v| f64_spec(v)).collect();
        Spec::Transform(json!({
            "class": "FixedFunctionTransform",
            "args": {
                "style": style_enum(p.style),
                "params": params,
                "direction": direction.oracle_enum(),
            },
        }))
    }
    fn port(&self, p: &Fixed, combo: &Combo) -> Result<Port, String> {
        fixed_port(p, combo)
    }
    fn pass_through(&self, _: &Fixed, _: &Combo) -> Channels {
        A
    }
    fn breakpoints(&self, p: &Fixed, _: Direction) -> Vec<f32> {
        // The glow's YC thresholds, `2 * mid` and `2 * mid / 3`, which a grey pixel's YC
        // equals (GetFixedFunctionCPURenderer, FixedFunctionOpCPU.cpp:2453-2468 @ v2.5.2).
        let mid: Option<f32> = match p.style {
            FixedFunctionStyle::AcesGlow03 => Some(0.1),
            FixedFunctionStyle::AcesGlow10 => Some(0.08),
            _ => None,
        };
        mid.map_or_else(Vec::new, |mid| vec![mid * 2.0, mid * 2.0 / 3.0])
    }
    fn validation(&self) -> Validation {
        Validation::Ported
    }
}

/// The ACES 1.3 gamut compression's parameters, as upstream's tests use them
/// (tests/cpu/ops/fixedfunction/FixedFunctionOpCPU_tests.cpp:393 @ v2.5.2).
const GAMUT_COMP_13: [f64; 7] = [1.147, 1.264, 1.312, 0.815, 0.803, 0.880, 1.2];

#[test]
fn aces_1_styles_match_the_wheel() {
    use FixedFunctionStyle::*;
    let mut cases: Vec<Case<Fixed>> = [
        AcesRedMod03,
        AcesRedMod10,
        AcesGlow03,
        AcesGlow10,
        AcesDarkToDim10,
    ]
    .map(|style| Case::new(format!("{style:?}"), Fixed::new(style, &[])))
    .to_vec();

    let gamut = Case::new(
        "AcesGamutComp13",
        Fixed::new(AcesGamutComp13, &GAMUT_COMP_13),
    );
    let bases = vec![gamut.clone()];
    cases.push(gamut);
    // The bounds of each parameter, and a power of 1 (the compression's knee is a corner).
    for (label, params) in [
        ("lower bounds", [1.001, 1.001, 1.001, 0.0, 0.0, 0.0, 1.0]),
        (
            "upper bounds",
            [65504.0, 65504.0, 65504.0, 0.9995, 0.9995, 0.9995, 65504.0],
        ),
        ("mixed", [1.5, 2.0, 1.01, 0.5, 0.9, 0.2, 3.0]),
    ] {
        cases.push(Case::new(
            format!("AcesGamutComp13 {label}"),
            Fixed::new(AcesGamutComp13, &params),
        ));
    }
    // A NaN power, which validation accepts: the route of the generated NaN cases, where a
    // bug in the spec would otherwise only show as refusals.
    let mut nan = GAMUT_COMP_13;
    nan[6] = f64::NAN;
    cases.push(Case::new(
        "AcesGamutComp13 NaN power",
        Fixed::new(AcesGamutComp13, &nan),
    ));
    // Refusals.
    let mut low = GAMUT_COMP_13;
    low[0] = 1.0;
    cases.push(Case::new(
        "refused lim_cyan 1",
        Fixed::new(AcesGamutComp13, &low),
    ));
    cases.push(Case::new(
        "refused 6 parameters",
        Fixed::new(AcesGamutComp13, &GAMUT_COMP_13[..6]),
    ));
    cases.push(Case::new(
        "refused glow parameter",
        Fixed::new(AcesGlow03, &[1.0]),
    ));
    let mut inf = GAMUT_COMP_13;
    inf[1] = f64::NEG_INFINITY;
    cases.push(Case::new(
        "refused lim_magenta -inf",
        Fixed::new(AcesGamutComp13, &inf),
    ));
    cases.push(Case::new(
        "refused glow parameter inf",
        Fixed::new(AcesGlow10, &[f64::INFINITY]),
    ));

    battery::run(&AcesFamily { cases, bases });
}
