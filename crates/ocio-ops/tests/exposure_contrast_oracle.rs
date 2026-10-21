// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The ExposureContrast op's renderers against the wheel's `ExposureContrastTransform`:
//! - the battery (`renderers_match_the_wheel`): every style in both directions with fast math
//!   on and off (the renderers ignore it), contrast 1 (the scalar branches) and not (the
//!   `OCIO_USE_SSE2` branches), dynamic and fixed values, and the generated extreme, NaN and
//!   infinite parameters, on the tier's probes;
//! - dynamic sets (`dynamic_sets_apply_as_the_wheels`): the CPU processor's exposure, contrast
//!   and gamma properties set one value after another between applies (`dynamic_apply`),
//!   against the port's CPU processor with the same steps.
//!
//! The port's side is the CPU processor of the op the wheel builds (`CPUProcessor` of a
//! processor of one ExposureContrast op): the optimizer removes an op that is an identity and
//! not dynamic, as the wheel's does.

use ocio_ops::cpu_processor::CpuProcessor;
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{
    BitDepth, DynamicPropertyType, ExposureContrastStyle, OptimizationFlags, TransformDirection,
};
use ocio_ops::ops::exposurecontrast::exposure_contrast_op::create_exposure_contrast_op;
use ocio_ops::ops::exposurecontrast::exposure_contrast_op_data::ExposureContrastOpData;
use ocio_testkit::Oracle;
use ocio_testkit::battery::params::{A, Case, Channels, Params, Precision, RGB, Slot};
use ocio_testkit::battery::{self, Combo, Direction, Family, Port, Spec};
use ocio_testkit::oracle::{bytes_to_f32, f32_to_bytes};
use ocio_testkit::transform_text::f64_spec;
use serde_json::{Value, json};

/// An ExposureContrastTransform's parameters: the style, exposure, contrast, gamma, pivot,
/// log exposure step and log mid-gray, and which of exposure, contrast and gamma are dynamic.
#[derive(Debug, Clone)]
struct EcParams {
    style: ExposureContrastStyle,
    values: [f64; 6],
    dynamic: [bool; 3],
}

const SLOT_NAMES: [&str; 6] = [
    "exposure",
    "contrast",
    "gamma",
    "pivot",
    "logExposureStep",
    "logMidGray",
];

impl EcParams {
    fn new(style: ExposureContrastStyle, values: [f64; 6]) -> Self {
        EcParams {
            style,
            values,
            dynamic: [false; 3],
        }
    }

    fn dynamic(mut self, dynamic: [bool; 3]) -> Self {
        self.dynamic = dynamic;
        self
    }

    /// The op data the binding's constructor makes (src/bindings/python/transforms/
    /// PyExposureContrastTransform.cpp:19-44 @ v2.5.2: `Create`, `setStyle`, the six setters,
    /// the `make*Dynamic` calls, `setDirection`, `validate`), then `BuildExposureContrastOp`'s
    /// copy of it (src/OpenColorIO/ops/exposurecontrast/ExposureContrastOp.cpp:212-222).
    fn op_data(&self, direction: TransformDirection) -> ExposureContrastOpData {
        let mut data =
            ExposureContrastOpData::new(ExposureContrastOpData::convert_transform_style(
                self.style,
                TransformDirection::Forward,
            ));
        data.set_exposure(self.values[0]);
        data.set_contrast(self.values[1]);
        data.set_gamma(self.values[2]);
        data.set_pivot(self.values[3]);
        data.set_log_exposure_step(self.values[4]);
        data.set_log_mid_gray(self.values[5]);
        if self.dynamic[0] {
            data.get_exposure_property().make_dynamic();
        }
        if self.dynamic[1] {
            data.get_contrast_property().make_dynamic();
        }
        if self.dynamic[2] {
            data.get_gamma_property().make_dynamic();
        }
        data.set_direction(direction);
        data.validate();
        data.clone()
    }

    /// The port's CPU processor of the processor's one op, for F32 images.
    fn cpu(&self, direction: TransformDirection, fast_math: bool) -> CpuProcessor {
        let mut raw = OpVec::new();
        create_exposure_contrast_op(
            &mut raw,
            std::hint::black_box(self.op_data(direction)),
            TransformDirection::Forward,
        );
        raw.finalize().unwrap();
        CpuProcessor::new(&raw, BitDepth::F32, BitDepth::F32, flags(fast_math)).unwrap()
    }

    /// The transform spec, every value by its bits.
    fn transform(&self, direction: &str) -> Value {
        let v = |i: usize| f64_spec(self.values[i]);
        json!({"class": "ExposureContrastTransform", "args": {
            "style": {"enum": style_name(self.style)},
            "exposure": v(0),
            "contrast": v(1),
            "gamma": v(2),
            "pivot": v(3),
            "logExposureStep": v(4),
            "logMidGray": v(5),
            "dynamicExposure": self.dynamic[0],
            "dynamicContrast": self.dynamic[1],
            "dynamicGamma": self.dynamic[2],
            "direction": {"enum": direction},
        }})
    }
}

impl Params for EcParams {
    fn slots(&self) -> Vec<Slot> {
        SLOT_NAMES
            .iter()
            .map(|name| Slot::new(*name, Precision::F64AsF32, RGB))
            .collect()
    }

    fn get(&self, index: usize) -> f64 {
        self.values[index]
    }

    fn set(&mut self, index: usize, value: f64) {
        self.values[index] = value;
    }
}

/// The default optimization, with fast math on or off.
fn flags(fast_math: bool) -> OptimizationFlags {
    if fast_math {
        OptimizationFlags::DEFAULT
    } else {
        OptimizationFlags(OptimizationFlags::DEFAULT.0 & !OptimizationFlags::FAST_LOG_EXP_POW.0)
    }
}

fn style_name(style: ExposureContrastStyle) -> &'static str {
    match style {
        ExposureContrastStyle::Linear => "EXPOSURE_CONTRAST_LINEAR",
        ExposureContrastStyle::Video => "EXPOSURE_CONTRAST_VIDEO",
        ExposureContrastStyle::Logarithmic => "EXPOSURE_CONTRAST_LOGARITHMIC",
    }
}

fn transform_direction(direction: Direction) -> TransformDirection {
    match direction {
        Direction::Forward => TransformDirection::Forward,
        Direction::Inverse => TransformDirection::Inverse,
    }
}

/// Applies `cpu` to packed F32 RGBA pixels, in place.
fn apply(cpu: &CpuProcessor, px: &mut [f32]) {
    for pixel in px.as_chunks_mut::<4>().0 {
        cpu.apply_rgba(pixel).unwrap();
    }
}

const STYLES: [ExposureContrastStyle; 3] = [
    ExposureContrastStyle::Linear,
    ExposureContrastStyle::Video,
    ExposureContrastStyle::Logarithmic,
];

/// The defaults but for the values given: exposure 0, contrast 1, gamma 1, pivot 0.18, and the
/// default log exposure step and mid-gray.
const DEFAULTS: [f64; 6] = [0.0, 1.0, 1.0, 0.18, 0.088, 0.435];

fn with(changes: &[(usize, f64)]) -> [f64; 6] {
    let mut values = DEFAULTS;
    for &(i, v) in changes {
        values[i] = v;
    }
    values
}

fn explicit_cases() -> Vec<Case<EcParams>> {
    let mut cases = Vec::new();
    for style in STYLES {
        let s = style_name(style);
        // Upstream's test values (tests/cpu/ops/exposurecontrast/ExposureContrastOpCPU_tests.cpp
        // @ v2.5.2).
        cases.push(Case::new(
            format!("{s}, contrast 1"),
            EcParams::new(style, with(&[(0, 0.2), (2, 1.0)])),
        ));
        cases.push(Case::new(
            format!("{s}, contrast 1.5, gamma 1.2"),
            EcParams::new(style, with(&[(0, 0.2), (1, 1.5), (2, 1.2)])),
        ));
        cases.push(Case::new(
            format!("{s}, the inverse test's"),
            EcParams::new(style, with(&[(0, 1.5), (1, 0.5), (2, 1.1), (3, 0.18)])),
        ));
        cases.push(Case::new(
            format!("{s}, log parameters"),
            EcParams::new(
                style,
                with(&[(0, 0.2), (1, 1.0), (2, 1.2), (4, 0.1), (5, 0.4)]),
            ),
        ));
        cases.push(Case::new(
            format!("{s}, contrast 0.5 gamma 1.6, dynamic"),
            EcParams::new(style, with(&[(0, 0.2), (1, 0.5), (2, 1.6)])).dynamic([true; 3]),
        ));
        cases.push(Case::new(
            format!("{s}, identity, dynamic exposure"),
            EcParams::new(style, DEFAULTS).dynamic([true, false, false]),
        ));
        cases.push(Case::new(
            format!("{s}, identity"),
            EcParams::new(style, DEFAULTS),
        ));
        // Only the gamma isn't the identity's (fixed, not dynamic): the op isn't an identity,
        // which the optimizer would remove.
        cases.push(Case::new(
            format!("{s}, gamma 1.3 alone"),
            EcParams::new(style, with(&[(2, 1.3)])),
        ));
        // The limits: a pivot below 0.001, a contrast times gamma below 0.001, a contrast
        // times gamma of 1 from two values that aren't.
        cases.push(Case::new(
            format!("{s}, pivot and contrast below their minimums"),
            EcParams::new(
                style,
                with(&[(0, -0.7), (1, 0.0005), (2, 0.5), (3, 0.0002)]),
            ),
        ));
        cases.push(Case::new(
            format!("{s}, contrast 0.8 times gamma 1.25"),
            EcParams::new(style, with(&[(0, 1.0), (1, 0.8), (2, 1.25)])),
        ));
        cases.push(Case::new(
            format!("{s}, negative contrast"),
            EcParams::new(style, with(&[(1, -2.0), (2, 0.5)])),
        ));
        // Finite parameters that make NaN coefficients: an exposure that overflows a float
        // times a zero step (log), a pivot that overflows a float (its log pivot is +Inf).
        cases.push(Case::new(
            format!("{s}, exposure 1e300, step 0"),
            EcParams::new(style, with(&[(0, 1e300), (1, 0.7), (4, 0.0)])),
        ));
        cases.push(Case::new(
            format!("{s}, pivot 1e300"),
            EcParams::new(style, with(&[(0, 0.5), (1, 0.7), (3, 1e300)])),
        ));
        cases.push(Case::new(
            format!("{s}, pivot 1e300, contrast 1"),
            EcParams::new(style, with(&[(0, 0.5), (3, 1e300)])),
        ));
    }
    cases
}

struct EcFamily;

impl Family for EcFamily {
    type Params = EcParams;

    fn name(&self) -> String {
        "ExposureContrastTransform".to_string()
    }

    fn cases(&self) -> Vec<Case<EcParams>> {
        explicit_cases()
    }

    /// Per style, the SSE2 branch (contrast 0.8, gamma 1.1) and the scalar one (contrast 1),
    /// with a non-default pivot, step and mid-gray.
    fn mutation_bases(&self) -> Vec<Case<EcParams>> {
        let mut bases = Vec::new();
        for style in STYLES {
            let s = style_name(style);
            bases.push(Case::new(
                format!("{s}, base"),
                EcParams::new(style, [1.2, 0.8, 1.1, 0.25, 0.07, 0.4]),
            ));
            bases.push(Case::new(
                format!("{s}, base, contrast 1"),
                EcParams::new(style, [-0.6, 1.0, 1.0, 0.25, 0.07, 0.4]),
            ));
        }
        bases
    }

    /// `ExposureContrastTransform(style, exposure, contrast, gamma, pivot, logExposureStep,
    /// logMidGray, dynamicExposure, dynamicContrast, dynamicGamma, direction)`.
    fn spec(&self, p: &EcParams, direction: Direction) -> Spec {
        let name = match direction {
            Direction::Forward => "TRANSFORM_DIR_FORWARD",
            Direction::Inverse => "TRANSFORM_DIR_INVERSE",
        };
        Spec::Transform(p.transform(name))
    }

    /// The CPU processor of the op the wheel builds ([`EcParams::op_data`]), with the
    /// combination's optimization.
    fn port(&self, p: &EcParams, combo: &Combo) -> Result<Port, String> {
        let cpu = p.cpu(transform_direction(combo.direction), combo.fast_math);
        Ok(Port::in_place(move |px| apply(&cpu, px)))
    }

    /// Alpha, which the renderers never write; none when the optimizer removes the op (an
    /// identity, not dynamic), where alpha is the CPU processor's to copy.
    fn pass_through(&self, p: &EcParams, combo: &Combo) -> Channels {
        if p.op_data(transform_direction(combo.direction)).is_no_op() {
            [false; 4]
        } else {
            A
        }
    }

    /// The pivot, where the linear and video styles' power has a base of 1.
    fn breakpoints(&self, p: &EcParams, _: Direction) -> Vec<f32> {
        let pivot = p.values[3] as f32;
        if pivot.is_finite() {
            vec![pivot]
        } else {
            Vec::new()
        }
    }
}

#[test]
fn renderers_match_the_wheel() {
    battery::run(&EcFamily);
}

// ---------------------------------------------------------------------------------------------
// Inverse pairs.
// ---------------------------------------------------------------------------------------------

/// A forward and an inverse ExposureContrast transform in a group, at the default
/// optimization (`OPTIMIZATION_PAIR_IDENTITY_EXPOSURE_CONTRAST` is on): the optimizer removes
/// the pair where the second is the first's inverse (`ExposureContrastOpData::isInverse`,
/// ExposureContrastOpData.cpp:210-218 @ v2.5.2, which compares the data with `equals`: the
/// style, the pivot, the log exposure step and the log mid-gray, and the properties). A pair
/// whose log mid-grays differ stays, and the wheel's pixels are not the input's.
///
/// Each case's pixels and CPU cache ID against the wheel's. The wheel's own answers pin what
/// the case tests: the removed pair gives the input's bits, the kept pair doesn't.
#[test]
fn inverse_pairs_are_removed_as_the_wheels() {
    let input = pixels();
    let bytes = f32_to_bytes(&input);
    let log = |mid_gray: f64| {
        EcParams::new(
            ExposureContrastStyle::Logarithmic,
            with(&[(0, 0.5), (1, 0.8), (2, 1.1), (5, mid_gray)]),
        )
    };
    let linear = EcParams::new(
        ExposureContrastStyle::Linear,
        with(&[(0, 0.5), (1, 0.8), (2, 1.1)]),
    );
    // (name, the pair, whether the wheel removes it)
    let cases = [
        ("log, same mid-gray", [log(0.435), log(0.435)], true),
        ("log, mid-grays differ", [log(0.435), log(0.4)], false),
        (
            "log, mid-grays differ the other way",
            [log(0.4), log(0.435)],
            false,
        ),
        (
            "linear, same values",
            [linear.clone(), linear.clone()],
            true,
        ),
    ];
    for (name, [first, second], removed) in cases {
        let response = Oracle::get().call(
            "cpu_apply",
            json!({
                "transform": {"class": "GroupTransform", "children": [
                    first.transform("TRANSFORM_DIR_FORWARD"),
                    second.transform("TRANSFORM_DIR_INVERSE"),
                ]},
            }),
            &[&bytes],
        );
        let result = &response.result;
        assert!(result.get("exception").is_none(), "{name}: {result}");
        let wheel = bytes_to_f32(&response.blobs[0]);
        // (The input's signaling NaN comes back quieted either way.)
        let same_as_input = wheel
            .iter()
            .zip(&input)
            .all(|(w, i)| w.to_bits() == i.to_bits() || (w.is_nan() && i.is_nan()));
        assert_eq!(
            same_as_input, removed,
            "{name}: the wheel's pixels {wheel:?}"
        );

        let mut raw = OpVec::new();
        for (params, direction) in [
            (&first, TransformDirection::Forward),
            (&second, TransformDirection::Inverse),
        ] {
            create_exposure_contrast_op(
                &mut raw,
                std::hint::black_box(params.op_data(direction)),
                TransformDirection::Forward,
            );
        }
        raw.finalize().unwrap();
        let cpu = CpuProcessor::new(&raw, BitDepth::F32, BitDepth::F32, flags(true)).unwrap();
        let mut port = input.clone();
        apply(&cpu, &mut port);
        let differ: Vec<usize> = (0..port.len())
            .filter(|&i| port[i].to_bits() != wheel[i].to_bits())
            .collect();
        assert!(differ.is_empty(), "{name}: pixels {differ:?} differ");
        assert_eq!(
            std::str::from_utf8(cpu.get_cache_id()).unwrap(),
            result["cpu_cache_id"].as_str().unwrap(),
            "{name}"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Dynamic sets.
// ---------------------------------------------------------------------------------------------

/// Pixels of each sign and size, a NaN and the infinities, with alphas that aren't 1.
fn pixels() -> Vec<f32> {
    vec![
        0.0367126,
        0.5,
        1.0,
        0.25,
        0.2,
        0.0,
        0.99,
        128.0,
        -0.25,
        2.0,
        1e-3,
        f32::from_bits(0x7fa0_0000),
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        0.0,
        100.0,
        0.18,
        0.75,
        -1.0,
    ]
}

/// The (type, value) sets between applies: each property alone, then together, a contrast
/// that makes the product 1, values under the minimums, and NaN.
fn sets() -> Vec<(DynamicPropertyType, f64)> {
    use DynamicPropertyType::{Contrast, Exposure, Gamma};
    vec![
        (Exposure, 1.0),
        (Exposure, -0.5),
        (Contrast, 1.5),
        (Gamma, 0.8),
        (Contrast, 1.25),
        (Gamma, 0.0005),
        (Exposure, 2.5),
        (Contrast, f64::NAN),
        (Exposure, f64::INFINITY),
        (Gamma, 1.0),
        (Contrast, 1.0),
        // The scalar loops of a contrast of 1 meet a NaN exposure and a NaN pixel (the pixels'
        // NaNs have payloads 0x200000 and 0x400000): each wheel's operand order decides whose
        // NaN, and which sign, comes out.
        (Exposure, f64::from_bits(0x7ff8_4000_0000_0000)),
        (Exposure, f64::from_bits(0xfff4_0000_0000_0000)),
        (Exposure, 0.0),
    ]
}

/// Each style in each direction, with the three properties dynamic: the CPU processor's
/// properties set as `sets()` lists, an apply after each set, against the wheel's
/// `dynamic_apply` with the same steps, bit for bit (W0002 doesn't apply: these are inputs,
/// not the battery's NaN parameters, and NaN parameters have no channel of their own here).
#[test]
fn dynamic_sets_apply_as_the_wheels() {
    let input = pixels();
    let bytes = f32_to_bytes(&input);
    let base = [0.3, 0.9, 1.1, 0.2, 0.08, 0.42];
    let mut cases = Vec::new();
    for style in STYLES {
        for direction in [TransformDirection::Forward, TransformDirection::Inverse] {
            cases.push((EcParams::new(style, base).dynamic([true; 3]), direction));
        }
    }
    let mut failures = Vec::new();
    for (params, direction) in &cases {
        let dir = match direction {
            TransformDirection::Forward => "TRANSFORM_DIR_FORWARD",
            TransformDirection::Inverse => "TRANSFORM_DIR_INVERSE",
        };
        let mut steps = vec![
            json!({"get": "DYNAMIC_PROPERTY_EXPOSURE", "from": "cpu", "as": "e"}),
            json!({"get": "DYNAMIC_PROPERTY_CONTRAST", "from": "cpu", "as": "c"}),
            json!({"get": "DYNAMIC_PROPERTY_GAMMA", "from": "cpu", "as": "g"}),
            json!({"apply": [0, 1]}),
        ];
        for (t, v) in sets() {
            let name = match t {
                DynamicPropertyType::Exposure => "e",
                DynamicPropertyType::Contrast => "c",
                _ => "g",
            };
            steps.push(json!({"set": name, "value": f64_spec(v)}));
            steps.push(json!({"apply": [0, 1]}));
        }
        let response = Oracle::get().call(
            "dynamic_apply",
            json!({
                "transform": params.transform(dir),
                "buffers": [{"blob": 0}, {"size": bytes.len(), "fill": [0]}],
                "images": [
                    {"kind": "packed", "data": {"buffer": 0}, "width": 5, "height": 1,
                     "num_channels": 4},
                    {"kind": "packed", "data": {"buffer": 1}, "width": 5, "height": 1,
                     "num_channels": 4},
                ],
                "steps": steps,
            }),
            &[&bytes],
        );
        let result = &response.result;
        assert!(result.get("exception").is_none(), "{result}");
        let wheel_steps = result["steps"].as_array().unwrap();

        // The port: the CPU processor of a forward processor of the op the wheel builds, its
        // properties set the same way.
        let cpu = params.cpu(*direction, true);
        for (i, t) in [
            DynamicPropertyType::Exposure,
            DynamicPropertyType::Contrast,
            DynamicPropertyType::Gamma,
        ]
        .into_iter()
        .enumerate()
        {
            assert!(cpu.has_dynamic_property(t));
            assert_eq!(wheel_steps[i]["has"], json!(true), "{result}");
        }
        let property = |t| cpu.get_dynamic_property(t).unwrap().as_double().unwrap();
        let mut applies = vec![None];
        for (t, v) in sets() {
            applies.push(Some((t, v)));
        }
        for (k, set) in applies.iter().enumerate() {
            if let Some((t, v)) = set {
                property(*t).set_value(*v);
            }
            let mut port = input.clone();
            apply(&cpu, &mut port);
            let step = &wheel_steps[if k == 0 { 3 } else { 3 + 2 * k }];
            let index = step["result"]["buffers"][1]
                .as_u64()
                .unwrap_or_else(|| panic!("{step}")) as usize;
            let wheel = bytes_to_f32(&response.blobs[index]);
            let differ: Vec<String> = port
                .iter()
                .zip(&wheel)
                .enumerate()
                .filter(|(_, (p, w))| p.to_bits() != w.to_bits())
                .map(|(i, (p, w))| {
                    format!(
                        "[{i}] port {p:e} ({:#x}) wheel {w:e} ({:#x})",
                        p.to_bits(),
                        w.to_bits()
                    )
                })
                .collect();
            if !differ.is_empty() {
                failures.push(format!(
                    "{:?} {direction:?} after {set:?}: {}",
                    params.style,
                    differ.join(", ")
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
