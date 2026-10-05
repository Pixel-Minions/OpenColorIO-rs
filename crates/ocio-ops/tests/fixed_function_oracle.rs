// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The FixedFunction renderers against the wheel, bit for bit, through the oracle test
//! battery (`ocio_testkit::battery`): every case in both directions, with fast math on and
//! off, on the tier's probe sets (`OCIO_RS_TIER`). So far the ACES 1.x styles (chunk 2.3b), the
//! Rec.2100 surround, HSV and the CIE styles (2.3c1), the HSY styles (2.3c2), and the
//! gamma-log and double-log styles (2.3d1).
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

use common::fixed_function::{params_digest, random_gamut_comp_13_params, style_enum};
use ocio_ops::open_color_types::{FixedFunctionStyle, TransformDirection};
use ocio_ops::ops::fixedfunction::FixedFunctionOpStyle;
use ocio_ops::ops::fixedfunction::fixed_function_op_cpu::get_fixed_function_cpu_renderer;
use ocio_ops::ops::fixedfunction::fixed_function_op_data::FixedFunctionOpData;
use ocio_testkit::battery::params::{
    A, B, Case, Channels, G, Params, Precision, R, RGB, Slot, W0001Function,
};
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

/// A set of styles.
struct FixedFamily {
    name: &'static str,
    cases: Vec<Case<Fixed>>,
    bases: Vec<Case<Fixed>>,
}

impl Family for FixedFamily {
    type Params = Fixed;

    fn name(&self) -> String {
        format!("FixedFunctionTransform ({})", self.name)
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
        if let Some(mid) = mid {
            return vec![mid * 2.0, mid * 2.0 / 3.0];
        }
        match p.style {
            // L* switches from linear to the cube root at this Y, and back at this L*
            // (FixedFunctionOpCPU.cpp:1974, 2012 @ v2.5.2).
            FixedFunctionStyle::XyzToLuv => vec![0.008856451679, 0.08],
            // The linear HSY blends its low and high saturations between these lumas
            // (FixedFunctionOpCPU.cpp:1638-1640, 1714-1716 @ v2.5.2).
            FixedFunctionStyle::RgbToHsyLin => vec![0.001, 0.01],
            // The mirror and break points, and the break point after the gamma segment
            // (FixedFunctionOpCPU.cpp:2260-2263, 2284-2285 @ v2.5.2).
            FixedFunctionStyle::LinToGammaLog if p.params.len() == 10 => {
                let q = |i: usize| p.params[i] as f32;
                let prime_break = q(3) * (q(1) + q(4)).powf(q(2));
                let prime_mirror = q(3) * (q(0) + q(4)).powf(q(2));
                vec![q(0), q(1), prime_break, prime_mirror]
            }
            // The break points (FixedFunctionOpCPU.cpp:2359, 2363 @ v2.5.2).
            FixedFunctionStyle::LinToDoubleLog if p.params.len() == 13 => {
                vec![p.params[1] as f32, p.params[2] as f32]
            }
            _ => Vec::new(),
        }
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
    // Random sets within the bounds (pinned by `random_gamut_comp_params_are_pinned`).
    for (i, params) in random_gamut_comp_13_params(RANDOM_GAMUT_COMP_SETS)
        .iter()
        .enumerate()
    {
        cases.push(Case::new(
            format!("AcesGamutComp13 random {i}"),
            Fixed::new(AcesGamutComp13, params),
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

    battery::run(&FixedFamily {
        name: "ACES 1.x",
        cases,
        bases,
    });
}

#[test]
fn surround_hsv_hsy_and_cie_styles_match_the_wheel() {
    use FixedFunctionStyle::*;
    let mut cases: Vec<Case<Fixed>> = [
        RgbToHsv,
        RgbToHsyLin,
        RgbToHsyLog,
        RgbToHsyVid,
        XyzToXyy,
        XyzToUvy,
        XyzToLuv,
    ]
    .map(|style| Case::new(format!("{style:?}"), Fixed::new(style, &[])))
    .to_vec();

    // tests/cpu/ops/fixedfunction/FixedFunctionOpCPU_tests.cpp:996, 1030 @ v2.5.2.
    let surround = Case::new("Rec2100Surround 0.78", Fixed::new(Rec2100Surround, &[0.78]));
    let bases = vec![surround.clone()];
    cases.push(surround);
    cases.push(Case::new(
        "Rec2100Surround 1.2",
        Fixed::new(Rec2100Surround, &[1.2]),
    ));
    // The bounds.
    cases.push(Case::new(
        "Rec2100Surround 0.01",
        Fixed::new(Rec2100Surround, &[0.01]),
    ));
    cases.push(Case::new(
        "Rec2100Surround 100",
        Fixed::new(Rec2100Surround, &[100.0]),
    ));
    // A NaN gamma, which validation accepts.
    cases.push(Case::new(
        "Rec2100Surround NaN",
        Fixed::new(Rec2100Surround, &[f64::NAN]),
    ));
    // Refusals.
    cases.push(Case::new(
        "refused surround 0.001",
        Fixed::new(Rec2100Surround, &[0.001]),
    ));
    cases.push(Case::new(
        "refused surround no parameter",
        Fixed::new(Rec2100Surround, &[]),
    ));
    cases.push(Case::new(
        "refused HSV parameter",
        Fixed::new(RgbToHsv, &[1.0]),
    ));

    battery::run(&FixedFamily {
        name: "surround, HSV, HSY, CIE",
        cases,
        bases,
    });
}

#[test]
fn gamma_log_and_double_log_match_the_wheel() {
    use FixedFunctionStyle::*;
    // The Rec.2100 HLG curve (tests/cpu/ops/fixedfunction/FixedFunctionOpCPU_tests.cpp:1311-1325
    // @ v2.5.2).
    let hlg = [
        0.0,
        0.25,
        0.5,
        1.0,
        0.0,
        std::f64::consts::E,
        0.17883277,
        0.807825590164,
        1.0,
        -0.07116723,
    ];
    // FixedFunctionOpCPU_tests.cpp:1374-1382 and FixedFunctionOp_tests.cpp:536-543 @ v2.5.2.
    let double_log = [
        10.0, 0.25, 0.5, -1.0, 0.0, -1.0, 1.25, 1.0, 1.0, 1.0, 0.5, 1.0, 0.0,
    ];
    let double_log_2 = [
        10.0, 0.5, 0.5, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0,
    ];
    let gamma_log = Case::new("LinToGammaLog HLG", Fixed::new(LinToGammaLog, &hlg));
    let double = Case::new("LinToDoubleLog", Fixed::new(LinToDoubleLog, &double_log));
    let bases = vec![gamma_log.clone(), double.clone()];
    let mut cases = vec![
        gamma_log,
        double,
        Case::new(
            "LinToDoubleLog 2",
            Fixed::new(LinToDoubleLog, &double_log_2),
        ),
    ];
    // A mirror below 0, a gamma above 1 and an offset gamma segment.
    let mut mirrored = hlg;
    mirrored[0] = -0.1;
    mirrored[2] = 2.4;
    mirrored[4] = 0.055;
    mirrored[5] = 2.0;
    cases.push(Case::new(
        "LinToGammaLog mirrored",
        Fixed::new(LinToGammaLog, &mirrored),
    ));
    // NaN parameters, which validation accepts.
    let mut nan = hlg;
    nan[6] = f64::NAN;
    cases.push(Case::new(
        "LinToGammaLog NaN slope",
        Fixed::new(LinToGammaLog, &nan),
    ));
    let mut nan = double_log;
    nan[12] = f64::NAN;
    cases.push(Case::new(
        "LinToDoubleLog NaN offset",
        Fixed::new(LinToDoubleLog, &nan),
    ));
    // Refusals.
    for (label, i, v) in [
        ("base 0", 5, 0.0),
        ("mirror at the break", 0, 0.25),
        ("gamma power 0", 2, 0.0),
    ] {
        let mut p = hlg;
        p[i] = v;
        cases.push(Case::new(
            format!("refused LinToGammaLog {label}"),
            Fixed::new(LinToGammaLog, &p),
        ));
    }
    for (label, i, v) in [("base -1", 0, -1.0), ("break order", 1, 0.75)] {
        let mut p = double_log;
        p[i] = v;
        cases.push(Case::new(
            format!("refused LinToDoubleLog {label}"),
            Fixed::new(LinToDoubleLog, &p),
        ));
    }
    cases.push(Case::new(
        "refused LinToDoubleLog 12 parameters",
        Fixed::new(LinToDoubleLog, &double_log[..12]),
    ));

    battery::run(&FixedFamily {
        name: "gamma-log, double-log",
        cases,
        bases,
    });
}

#[test]
fn pq_matches_the_wheel() {
    use FixedFunctionStyle::*;
    // W0001: without fast math, the Windows wheel computes PQ with SVML's pow, the port with
    // powf; finite values within the waiver's bound per renderer.
    let pq = Case::new("LinToPq", Fixed::new(LinToPq, &[]))
        .w0001(W0001Function::LinToPq, W0001Function::PqToLin);
    let cases = vec![
        pq,
        Case::new("refused LinToPq parameter", Fixed::new(LinToPq, &[1.0])),
    ];

    battery::run(&FixedFamily {
        name: "PQ",
        cases,
        bases: Vec::new(),
    });
}

/// The pole window of `PQ_TO_LIN`, in magnitude: the curve's denominator `c2 - c3 *
/// x^(1/m2)` is 0 near 1.992 (FixedFunctionOpCPU.cpp:2066-2068, 2160-2161 @ v2.5.2).
const PQ_POLE_WINDOW: (f32, f32) = (1.95, 2.05);

/// What W0001 waives without bound, `PQ_TO_LIN` above 1 in magnitude on Windows without fast
/// math, can't grow unnoticed where it matters most: every `f32` of `PQ_TO_LIN` in
/// [1.93, 2.07] and [-2.07, -1.93], around the pole. On Windows without fast math, the wheel
/// and the port give NaNs and infinities at the same inputs outside [`PQ_POLE_WINDOW`] (both
/// signs), where only finite values differ, and the test prints what differs inside it. Alpha
/// passes through bit for bit. With fast math, and on Linux, every value is bit-identical.
#[test]
fn pq_to_lin_pole_window_is_pinned() {
    use FixedFunctionStyle::*;
    let p = Fixed::new(LinToPq, &[]);
    let family = FixedFamily {
        name: "PQ pole window",
        cases: Vec::new(),
        bases: Vec::new(),
    };
    let (lo, hi) = (1.93f32.to_bits(), 2.07f32.to_bits());
    let values: Vec<f32> = (lo..=hi)
        .map(f32::from_bits)
        .flat_map(|v| [v, -v])
        .collect();
    let input: Vec<f32> = values
        .chunks(3)
        .flat_map(|c| {
            [
                c[0],
                c.get(1).copied().unwrap_or(1.0),
                c.get(2).copied().unwrap_or(1.0),
                1.0,
            ]
        })
        .collect();
    let bytes = ocio_testkit::oracle::f32_to_bytes(&input);
    let combos: Vec<Combo> = [false, true]
        .map(|fast_math| Combo {
            direction: Direction::Inverse,
            fast_math,
            format: battery::Format::F32_RGBA,
        })
        .to_vec();
    let calls: Vec<ocio_testkit::oracle::BatchCall<'_>> = combos
        .iter()
        .map(|combo| ocio_testkit::oracle::BatchCall {
            cmd: "cpu_apply",
            args: family.spec(&p, combo.direction).cpu_apply_args(combo),
            blobs: vec![&bytes],
        })
        .collect();
    // Not cached: the responses are large, and cheap to compute again.
    let responses = ocio_testkit::oracle::Oracle::get().batch(&calls, false);
    for (combo, response) in combos.iter().zip(responses) {
        let expected = response
            .unwrap_or_else(|e| panic!("{combo}: the oracle failed: {e}"))
            .blob_f32(0);
        let data = p.transform_data(port_direction(combo.direction)).unwrap();
        let renderer = get_fixed_function_cpu_renderer(&black_box(data), combo.fast_math).unwrap();
        let mut actual = input.clone();
        renderer.apply(&mut actual);
        if combo.fast_math || !cfg!(target_os = "windows") {
            if let Some(report) =
                ocio_testkit::compare::f32_bits_report(&expected, &actual, Some(&input), 4)
            {
                panic!("{combo}: {report}");
            }
            continue;
        }
        let (mut differ, mut kind_inside, mut outside) = (0, Vec::new(), Vec::new());
        for (i, ((&x, &e), &a)) in input.iter().zip(&expected).zip(&actual).enumerate() {
            if e.to_bits() == a.to_bits() {
                continue;
            }
            if i % 4 == 3 {
                outside.push(format!(
                    "alpha of {x:e}: wheel {:08x}, port {:08x}",
                    e.to_bits(),
                    a.to_bits()
                ));
                continue;
            }
            differ += 1;
            if e.is_nan() == a.is_nan() && e.is_infinite() == a.is_infinite() {
                continue;
            }
            let entry = format!(
                "{x:e} ({:08x}): wheel {e:e} ({:08x}), port {a:e} ({:08x})",
                x.to_bits(),
                e.to_bits(),
                a.to_bits()
            );
            if (PQ_POLE_WINDOW.0..=PQ_POLE_WINDOW.1).contains(&x.abs()) {
                kind_inside.push(entry);
            } else {
                outside.push(entry);
            }
        }
        println!(
            "{combo}: {} values, {differ} differ; NaN or infinity at different inputs inside \
             the pole window: {kind_inside:#?}",
            values.len()
        );
        assert!(
            outside.is_empty(),
            "{combo}: NaN or infinity at different inputs outside the pole window \
             {PQ_POLE_WINDOW:?}, or alpha differs: {outside:#?}"
        );
    }
}

/// The values of [`nan_combination_pixels`]: NaNs of different signs and payloads, quiet and
/// signalling, with finite values and infinities of both signs.
const COMBINATION_VALUES: [u32; 15] = [
    0xffc0_0000, // the x86 default NaN
    0x7fc1_2345,
    0xffc5_4321,
    0xff80_0001, // signalling
    0x7fa0_0000, // signalling
    0x0000_0000,
    0x8000_0000,
    0x3e38_51ec, // 0.18
    0x3f80_0000, // 1
    0xbf00_0000, // -0.5
    0x4000_0000, // 2
    0x3a83_126f, // 0.001
    0x3ba3_d70a, // 0.005, between the linear HSY's blending lumas
    0x7f80_0000, // +inf
    0xff80_0000, // -inf
];

/// Every combination of [`COMBINATION_VALUES`] in red, green and blue, one pixel each, alpha
/// cycling through them too. The battery's NaN buffers put NaNs in all three channels at once;
/// these also put two NaNs of different payloads next to a finite value or an infinity, which
/// is where a renderer's arithmetic can meet two NaNs in a branch the all-NaN pixels skip.
fn nan_combination_pixels() -> Vec<f32> {
    let v = COMBINATION_VALUES.map(f32::from_bits);
    let n = v.len();
    (0..n * n * n)
        .flat_map(|i| [v[i / (n * n)], v[i / n % n], v[i % n], v[i % (n - 1)]])
        .collect()
}

/// Every style whose renderer mixes channels, in both directions and with fast math on and
/// off, on [`nan_combination_pixels`]: where two NaNs of different payloads meet, the result
/// is the one the wheel's machine code picks (`CLAUDE.md`, "NaN operand order").
#[test]
fn nan_combinations_match_the_wheel() {
    use FixedFunctionStyle::*;
    let cases = [
        Fixed::new(AcesRedMod03, &[]),
        Fixed::new(AcesRedMod10, &[]),
        Fixed::new(AcesGlow03, &[]),
        Fixed::new(AcesGlow10, &[]),
        Fixed::new(AcesDarkToDim10, &[]),
        Fixed::new(AcesGamutComp13, &GAMUT_COMP_13),
        Fixed::new(Rec2100Surround, &[0.78]),
        Fixed::new(RgbToHsv, &[]),
        Fixed::new(RgbToHsyLin, &[]),
        Fixed::new(RgbToHsyLog, &[]),
        Fixed::new(RgbToHsyVid, &[]),
        Fixed::new(XyzToXyy, &[]),
        Fixed::new(XyzToUvy, &[]),
        Fixed::new(XyzToLuv, &[]),
    ];
    let family = FixedFamily {
        name: "NaN combinations",
        cases: Vec::new(),
        bases: Vec::new(),
    };
    let input = nan_combination_pixels();
    let bytes = ocio_testkit::oracle::f32_to_bytes(&input);
    let mut combos = Vec::new();
    for p in &cases {
        for direction in [Direction::Forward, Direction::Inverse] {
            for fast_math in [true, false] {
                let combo = Combo {
                    direction,
                    fast_math,
                    format: battery::Format::F32_RGBA,
                };
                combos.push((p, combo));
            }
        }
    }
    let calls: Vec<ocio_testkit::oracle::BatchCall<'_>> = combos
        .iter()
        .map(|(p, combo)| ocio_testkit::oracle::BatchCall {
            cmd: "cpu_apply",
            args: family.spec(p, combo.direction).cpu_apply_args(combo),
            blobs: vec![&bytes],
        })
        .collect();
    let responses = ocio_testkit::oracle::Oracle::get().batch(&calls, true);
    let mut failures = Vec::new();
    for ((p, combo), response) in combos.iter().zip(responses) {
        let label = format!("{:?} {:?}, {combo}", p.style, p.params);
        let response = response.unwrap_or_else(|e| panic!("{label}: the oracle failed: {e}"));
        if let Some(exception) = response.result.get("exception") {
            panic!("{label}: the wheel refused it: {exception}");
        }
        let expected = response.blob_f32(0);
        let data = p
            .transform_data(port_direction(combo.direction))
            .unwrap_or_else(|e| panic!("{label}: the port refused it: {e}"));
        let renderer = get_fixed_function_cpu_renderer(&black_box(data), combo.fast_math)
            .unwrap_or_else(|e| panic!("{label}: {}", e.message()));
        let mut actual = input.clone();
        renderer.apply(&mut actual);
        if let Some(report) =
            ocio_testkit::compare::f32_bits_report(&expected, &actual, Some(&input), 4)
        {
            failures.push(format!("{label}: {report}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} combinations differ:\n\n{}",
        failures.len(),
        combos.len(),
        failures.join("\n")
    );
}

/// How many random gamut compression parameter sets the battery runs.
const RANDOM_GAMUT_COMP_SETS: usize = 16;

/// The random gamut compression parameter sets can't change unnoticed.
#[test]
fn random_gamut_comp_params_are_pinned() {
    let sets = random_gamut_comp_13_params(RANDOM_GAMUT_COMP_SETS);
    assert_eq!(params_digest(&sets), 0x8fca_09f0_28f3_5149);
}
