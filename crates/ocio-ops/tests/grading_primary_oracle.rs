// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The GradingPrimary op's renderers against the wheel's `GradingPrimaryTransform`:
//! - the battery (`renderers_match_the_wheel`): the styles in both directions with fast math on
//!   and off (the renderers ignore it), each of an `apply`'s four loops (with and without the
//!   saturation, with and without its gamma or contrast), dynamic and fixed values, upstream's
//!   test values, finite and infinite parameters that make NaN coefficients, and the generated
//!   extreme, NaN and infinite parameters, on the tier's probes;
//! - dynamic sets (`dynamic_sets_apply_as_the_wheels`): the CPU processor's property set one
//!   value after another between applies (`dynamic_apply`), against the port's CPU processor
//!   with the same steps;
//! - decoupling (`renderer_property_is_decoupled_from_the_op`): a set on the op data's property
//!   after the renderer is built leaves the renderer's output as it was (a unit-level test of
//!   `get_grading_primary_cpu_renderer`: the CPU processor clones its ops).
//!
//! The port's side is the CPU processor of the op the wheel builds (`CPUProcessor` of a
//! processor of one GradingPrimary op): the optimizer removes an op that is an identity and
//! not dynamic, as the wheel's does.

use ocio_ops::cpu_processor::CpuProcessor;
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{
    BitDepth, DynamicPropertyType, GradingStyle, OptimizationFlags, TransformDirection,
};
use ocio_ops::ops::gradingprimary::grading_primary::{GradingPrimary, GradingRgbm};
use ocio_ops::ops::gradingprimary::grading_primary_op::create_grading_primary_op;
use ocio_ops::ops::gradingprimary::grading_primary_op_cpu::get_grading_primary_cpu_renderer;
use ocio_ops::ops::gradingprimary::grading_primary_op_data::GradingPrimaryOpData;
use ocio_testkit::Oracle;
use ocio_testkit::battery::params::{A, B, Case, Channels, G, Params, Precision, R, RGB, Slot};
use ocio_testkit::battery::{self, Combo, Direction, Family, Port, Spec};
use ocio_testkit::oracle::{bytes_to_f32, f32_to_bytes};
use ocio_testkit::probe::ProbeSet;
use ocio_testkit::transform_text::f64_spec;
use serde_json::{Value, json};

/// The styles.
const STYLES: [GradingStyle; 3] = [GradingStyle::Log, GradingStyle::Lin, GradingStyle::Video];

/// The RGBM values, by name, as the slots name them.
const RGBMS: [&str; 7] = [
    "brightness",
    "contrast",
    "gamma",
    "offset",
    "exposure",
    "lift",
    "gain",
];

/// The doubles after the RGBM values.
const SCALARS: [&str; 6] = [
    "saturation",
    "pivot",
    "pivotBlack",
    "pivotWhite",
    "clampBlack",
    "clampWhite",
];

/// The number of doubles in a GradingPrimary.
const NUM_FIELDS: usize = 34;

/// The fields of a GradingPrimary, by slot: the 28 RGBM components (red, green, blue, master
/// of each of [`RGBMS`]), then [`SCALARS`].
fn field(p: &mut GradingPrimary, index: usize) -> &mut f64 {
    fn rgbm(v: &mut GradingRgbm, c: usize) -> &mut f64 {
        match c {
            0 => &mut v.red,
            1 => &mut v.green,
            2 => &mut v.blue,
            _ => &mut v.master,
        }
    }
    match index {
        0..=3 => rgbm(&mut p.brightness, index),
        4..=7 => rgbm(&mut p.contrast, index - 4),
        8..=11 => rgbm(&mut p.gamma, index - 8),
        12..=15 => rgbm(&mut p.offset, index - 12),
        16..=19 => rgbm(&mut p.exposure, index - 16),
        20..=23 => rgbm(&mut p.lift, index - 20),
        24..=27 => rgbm(&mut p.gain, index - 24),
        28 => &mut p.saturation,
        29 => &mut p.pivot,
        30 => &mut p.pivot_black,
        31 => &mut p.pivot_white,
        32 => &mut p.clamp_black,
        _ => &mut p.clamp_white,
    }
}

/// A GradingPrimaryTransform's parameters: the style, the values, and whether it is dynamic.
#[derive(Debug, Clone)]
struct GpParams {
    style: GradingStyle,
    values: GradingPrimary,
    dynamic: bool,
}

fn style_name(style: GradingStyle) -> &'static str {
    match style {
        GradingStyle::Log => "GRADING_LOG",
        GradingStyle::Lin => "GRADING_LIN",
        GradingStyle::Video => "GRADING_VIDEO",
    }
}

fn dir_name(dir: TransformDirection) -> &'static str {
    match dir {
        TransformDirection::Forward => "TRANSFORM_DIR_FORWARD",
        TransformDirection::Inverse => "TRANSFORM_DIR_INVERSE",
    }
}

fn rgbm_spec(v: &GradingRgbm) -> Value {
    json!({"object": {"class": "GradingRGBM",
                      "args": [f64_spec(v.red), f64_spec(v.green), f64_spec(v.blue),
                               f64_spec(v.master)]}})
}

/// A GradingPrimary value spec, every double by its bits.
fn primary_spec(p: &GradingPrimary) -> Value {
    json!({"object": {"class": "GradingPrimary", "args": [{"enum": "GRADING_LOG"}], "attrs": [
        ["brightness", rgbm_spec(&p.brightness)],
        ["contrast", rgbm_spec(&p.contrast)],
        ["gamma", rgbm_spec(&p.gamma)],
        ["offset", rgbm_spec(&p.offset)],
        ["exposure", rgbm_spec(&p.exposure)],
        ["lift", rgbm_spec(&p.lift)],
        ["gain", rgbm_spec(&p.gain)],
        ["saturation", f64_spec(p.saturation)],
        ["pivot", f64_spec(p.pivot)],
        ["pivotBlack", f64_spec(p.pivot_black)],
        ["pivotWhite", f64_spec(p.pivot_white)],
        ["clampBlack", f64_spec(p.clamp_black)],
        ["clampWhite", f64_spec(p.clamp_white)],
    ]}})
}

impl GpParams {
    fn new(style: GradingStyle, values: GradingPrimary) -> Self {
        GpParams {
            style,
            values,
            dynamic: false,
        }
    }

    fn dynamic(mut self) -> Self {
        self.dynamic = true;
        self
    }

    /// The op data the binding's constructor makes (src/bindings/python/transforms/
    /// PyGradingPrimaryTransform.cpp:17-35 @ v2.5.2: `Create(style)`, `setValue`,
    /// `makeDynamic`, `setDirection`, `validate`), then `BuildGradingPrimaryOp`'s copy of it
    /// (src/OpenColorIO/ops/gradingprimary/GradingPrimaryOp.cpp:238-249); or the exception of
    /// values the transform refuses.
    fn op_data(&self, direction: TransformDirection) -> Result<GradingPrimaryOpData, String> {
        let mut data = GradingPrimaryOpData::new(self.style);
        data.set_value(&self.values).map_err(|e| e.to_string())?;
        if self.dynamic {
            data.get_dynamic_property_internal().make_dynamic();
        }
        data.set_direction(direction);
        data.validate().map_err(|e| e.to_string())?;
        Ok(data.clone())
    }

    /// The port's CPU processor of the processor's one op, for F32 images.
    fn cpu(&self, direction: TransformDirection, fast_math: bool) -> Result<CpuProcessor, String> {
        let mut raw = OpVec::new();
        create_grading_primary_op(
            &mut raw,
            std::hint::black_box(self.op_data(direction)?),
            TransformDirection::Forward,
        );
        raw.finalize().unwrap();
        Ok(CpuProcessor::new(&raw, BitDepth::F32, BitDepth::F32, flags(fast_math)).unwrap())
    }

    /// The transform spec, every value by its bits.
    fn transform(&self, direction: TransformDirection) -> Value {
        json!({"class": "GradingPrimaryTransform", "args": {
            "values": primary_spec(&self.values),
            "style": {"enum": style_name(self.style)},
            "dynamic": self.dynamic,
            "dir": {"enum": dir_name(direction)},
        }})
    }
}

impl Params for GpParams {
    /// Each component of an RGBM applies to its channel, the master and the scalars to the
    /// three; with a saturation other than 1, which mixes the channels, every value applies
    /// to the three.
    fn slots(&self) -> Vec<Slot> {
        let mixed = self.values.saturation != 1.0;
        let mut slots = Vec::with_capacity(NUM_FIELDS);
        for name in RGBMS {
            for (c, channels) in [("r", R), ("g", G), ("b", B), ("m", RGB)] {
                let channels: Channels = if mixed { RGB } else { channels };
                slots.push(Slot::new(
                    format!("{name}[{c}]"),
                    Precision::F64AsF32,
                    channels,
                ));
            }
        }
        for name in SCALARS {
            slots.push(Slot::new(name, Precision::F64AsF32, RGB));
        }
        slots
    }

    fn get(&self, index: usize) -> f64 {
        let mut values = self.values;
        *field(&mut values, index)
    }

    fn set(&mut self, index: usize, value: f64) {
        *field(&mut self.values, index) = value;
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

fn rgbm(red: f64, green: f64, blue: f64, master: f64) -> GradingRgbm {
    GradingRgbm::new(red, green, blue, master)
}

/// Upstream's test values of `style` (tests/cpu/ops/gradingprimary/
/// GradingPrimaryOpCPU_tests.cpp @ v2.5.2: TS1, TS2 and TS3), without clamps or pivots.
fn upstream_values(style: GradingStyle) -> GradingPrimary {
    let mut p = GradingPrimary::new(style);
    match style {
        GradingStyle::Log => {
            p.brightness = rgbm(-10., 45., -5., 50.);
            p.contrast = rgbm(0.9, 1.4, 0.7, 0.75);
            p.gamma = rgbm(1.1, 0.7, 1.05, 1.15);
            p.pivot = -0.3;
            p.saturation = 1.21;
        }
        GradingStyle::Lin => {
            p.exposure = rgbm(0.5, -0.2, 0.4, -0.25);
            p.offset = rgbm(-0.03, 0.02, 0.1, -0.1);
            p.contrast = rgbm(0.9, 1.4, 0.7, 0.75);
            p.pivot = 0.5;
            p.saturation = 1.33;
        }
        GradingStyle::Video => {
            p.lift = rgbm(0.05, -0.04, 0.02, 0.05);
            p.gamma = rgbm(0.9, 1.4, 0.7, 0.75);
            p.gain = rgbm(1.2, 1.1, 1.25, 0.8);
            p.offset = rgbm(-0.03, 0.02, 0.1, -0.1);
            p.saturation = 1.2;
        }
    }
    p
}

/// Upstream's clamps and pivots of `style` (TS1, TS2, TS3).
fn upstream_clamps_and_pivots(style: GradingStyle, p: &mut GradingPrimary) {
    match style {
        GradingStyle::Log => {
            p.clamp_black = -0.05;
            p.clamp_white = 1.50;
            p.pivot_black = 0.05;
            p.pivot_white = 0.9;
        }
        GradingStyle::Lin => {
            p.clamp_black = -0.40;
            p.clamp_white = 1.05;
        }
        GradingStyle::Video => {
            p.clamp_black = -0.15;
            p.clamp_white = 1.50;
            p.pivot_black = 0.05;
            p.pivot_white = 0.9;
        }
    }
}

/// Values that run each of an `apply`'s four loops: the power (gamma for log and video,
/// contrast for linear) and the saturation, each its identity or not; the other values off
/// their identity, the pivots and clamps not the defaults.
fn loop_values(style: GradingStyle, saturation: bool, power: bool) -> GradingPrimary {
    let mut p = GradingPrimary::new(style);
    p.brightness = rgbm(12.0, -30.0, 4.5, 8.0);
    p.offset = rgbm(0.02, -0.05, 0.1, -0.01);
    p.exposure = rgbm(0.3, -0.4, 0.25, 0.1);
    p.lift = rgbm(0.03, -0.02, 0.01, 0.02);
    p.gain = rgbm(1.1, 0.95, 1.2, 0.9);
    p.contrast = rgbm(1.2, 0.85, 1.1, 0.95);
    p.pivot = if style == GradingStyle::Log { 0.1 } else { 0.3 };
    p.pivot_black = 0.02;
    p.pivot_white = 0.95;
    p.clamp_black = -0.6;
    p.clamp_white = 2.5;
    if power {
        p.gamma = rgbm(0.8, 1.25, 1.05, 0.9);
    }
    if style == GradingStyle::Lin && !power {
        p.contrast = GradingRgbm::new(1.0, 1.0, 1.0, 1.0);
    }
    if saturation {
        p.saturation = 0.7;
    }
    p
}

fn explicit_cases() -> Vec<Case<GpParams>> {
    let mut cases = Vec::new();
    for style in STYLES {
        let s = style_name(style);
        cases.push(Case::new(
            format!("{s}, upstream's values"),
            GpParams::new(style, upstream_values(style)),
        ));
        let mut clamped = upstream_values(style);
        upstream_clamps_and_pivots(style, &mut clamped);
        cases.push(Case::new(
            format!("{s}, upstream's values with clamps and pivots"),
            GpParams::new(style, clamped),
        ));
        cases.push(Case::new(
            format!("{s}, upstream's values, dynamic"),
            GpParams::new(style, upstream_values(style)).dynamic(),
        ));
        for (saturation, power) in [(false, false), (false, true), (true, false), (true, true)] {
            cases.push(Case::new(
                format!("{s}, saturation {saturation}, power {power}"),
                GpParams::new(style, loop_values(style, saturation, power)),
            ));
        }
        cases.push(Case::new(
            format!("{s}, identity"),
            GpParams::new(style, GradingPrimary::new(style)),
        ));
        cases.push(Case::new(
            format!("{s}, identity, dynamic"),
            GpParams::new(style, GradingPrimary::new(style)).dynamic(),
        ));
        // Saturations of 0 (the inverse styles skip it), negative, and large.
        for saturation in [0.0, -0.5, 3.0] {
            let mut p = loop_values(style, true, true);
            p.saturation = saturation;
            cases.push(Case::new(
                format!("{s}, saturation {saturation}"),
                GpParams::new(style, p),
            ));
        }
        // Only a clamp: not an identity, the clamps alone.
        let mut p = GradingPrimary::new(style);
        p.clamp_black = 0.1;
        p.clamp_white = 0.8;
        cases.push(Case::new(
            format!("{s}, clamps only"),
            GpParams::new(style, p),
        ));
        // Infinite parameters that make NaN coefficients: brightness, offset, exposure, lift
        // of +Inf and -Inf (their sum); white and black pivots both +Inf (their range); a
        // gain of 0 (video's slope).
        for (label, f, g, v) in [
            ("infinite brightness sum", 0, 3, f64::INFINITY),
            ("infinite offset sum", 12, 15, f64::INFINITY),
            ("infinite exposure sum", 16, 19, f64::INFINITY),
            ("infinite lift sum", 20, 23, f64::INFINITY),
            ("infinite pivots", 30, 31, f64::INFINITY),
            ("infinite contrast", 5, 7, f64::INFINITY),
        ] {
            for saturation in [false, true] {
                let mut p = loop_values(style, saturation, true);
                *field(&mut p, f) = v;
                *field(&mut p, g) = if f == 30 || f == 5 { v } else { -v };
                cases.push(Case::new(
                    format!("{s}, {label}, saturation {saturation}"),
                    GpParams::new(style, p),
                ));
            }
        }
        // An infinite brightness, offset or lift sum with an infinite contrast: the reverse
        // styles' last addition meets a pixel's NaN and the coefficient's.
        for (f, g) in [(0, 3), (12, 15), (20, 23)] {
            let mut p = loop_values(style, false, false);
            *field(&mut p, f) = f64::INFINITY;
            *field(&mut p, g) = f64::NEG_INFINITY;
            p.contrast.blue = f64::INFINITY;
            p.gain.blue = 0.0;
            cases.push(Case::new(
                format!("{s}, infinite sums of slot {f}, infinite contrast"),
                GpParams::new(style, p),
            ));
        }
        // The forward styles' first addition (`pixel + coefficient`, or `coefficient + pixel`
        // on Linux) of a pixel's NaN and a NaN coefficient (the sum of +Inf and -Inf): the
        // sign of the NaN it keeps shows as a signed zero once `ApplyLinContrast` or
        // `ApplyGamma` (gamma red 0.8 in `loop_values`) takes its power, which turns a NaN
        // into 0 and keeps the sign. The black pivot of 0 of either sign makes the gamma's
        // `pix - blackPivot` keep or change a zero's sign.
        for (f, g) in [(0, 3), (12, 15), (20, 23)] {
            for pivot_black in [0.0, -0.0] {
                let mut p = loop_values(style, false, true);
                *field(&mut p, f) = f64::INFINITY;
                *field(&mut p, g) = f64::NEG_INFINITY;
                p.pivot_black = pivot_black;
                cases.push(Case::new(
                    format!("{s}, infinite sums of slot {f}, power, pivot black {pivot_black:?}"),
                    GpParams::new(style, p),
                ));
            }
        }
        // The reverse styles' last addition, where a pixel's default NaN meets a NaN
        // coefficient on the same channel: an infinite sum on red, with a contrast of +Inf
        // (log's and video's inverse slope), an exposure of -Inf and a gain of 0 (video's
        // inverse gain) on red too, so that the NaNs of the contrast, the exposure and the sum
        // meet on one channel.
        for (f, g) in [(0, 3), (12, 15), (20, 23)] {
            let mut p = loop_values(style, false, false);
            *field(&mut p, f) = f64::INFINITY;
            *field(&mut p, g) = f64::NEG_INFINITY;
            p.contrast.red = f64::INFINITY;
            p.exposure.red = f64::NEG_INFINITY;
            p.gain.red = 0.0;
            cases.push(Case::new(
                format!("{s}, infinite sums of slot {f}, red contrast, exposure and gain"),
                GpParams::new(style, p),
            ));
        }
        // A contrast of 0 on the colors and Inf on the master, with a black pivot of -0.0:
        // `ApplyContrast`'s multiplication `(pix - pivot) * contrast` of a zero and an
        // infinity, and its addition of the pivot.
        let mut p = loop_values(style, false, true);
        p.contrast = rgbm(0.0, 0.0, 0.0, f64::INFINITY);
        p.pivot_black = -0.0;
        cases.push(Case::new(
            format!("{s}, contrast 0 and master Inf, pivot black -0.0"),
            GpParams::new(style, p),
        ));
        // Gains of 0: video's forward slope and inverse gain fall back on 1.
        let mut p = loop_values(style, true, true);
        p.gain = rgbm(0.0, 1.0, -0.0, 1.0);
        cases.push(Case::new(format!("{s}, gain 0"), GpParams::new(style, p)));
        // Each value alone off the identity: the renderers' bypass checks each of the style's.
        for f in 0..NUM_FIELDS {
            let mut p = GradingPrimary::new(style);
            let value = match f {
                0..=3 => 5.0,
                4..=11 | 24..=27 => 1.15,
                12..=23 => 0.05,
                28 => 0.8,
                29 => 0.3,
                30 => 0.1,
                31 => 0.8,
                32 => 0.05,
                _ => 0.95,
            };
            *field(&mut p, f) = value;
            let name = GpParams::new(style, p).slots()[f].name.clone();
            cases.push(Case::new(
                format!("{s}, {name} = {value} alone"),
                GpParams::new(style, p),
            ));
        }
        // Contrasts whose product is 0 (the log style doesn't validate them): its inverse
        // falls back on 1.
        let mut p = loop_values(style, false, true);
        p.contrast = rgbm(0.0, 1.2, -0.0, 0.9);
        cases.push(Case::new(
            format!("{s}, contrast 0"),
            GpParams::new(style, p),
        ));
        // Lifts that nearly cancel the gain's term of video's inverse slope: the slope's
        // last bits depend on the order of its sum (found by a search in double precision).
        if style == GradingStyle::Video {
            for (lm, lr, gm, gr, pb, pw) in [
                (0.37, -0.6766209, 2.85, 2.3, -0.18, 0.83),
                (-0.66, 0.33819633, 1.43, 2.65, -0.05, 1.03),
            ] {
                let mut p = GradingPrimary::new(style);
                p.lift = rgbm(lr, 0.0, 0.0, lm);
                p.gain = rgbm(gr, 1.0, 1.0, gm);
                p.pivot_black = pb;
                p.pivot_white = pw;
                cases.push(Case::new(
                    format!("{s}, near-cancelling slope {lr}"),
                    GpParams::new(style, p),
                ));
            }
        }
        // A NaN value in the JSON route, which the wheel accepts.
        let mut p = loop_values(style, false, true);
        p.brightness.green = f64::NAN;
        cases.push(Case::new(
            format!("{s}, NaN value"),
            GpParams::new(style, p),
        ));
    }
    cases
}

struct GpFamily;

impl Family for GpFamily {
    type Params = GpParams;

    fn name(&self) -> String {
        "GradingPrimaryTransform".to_string()
    }

    fn cases(&self) -> Vec<Case<GpParams>> {
        explicit_cases()
    }

    /// Per style, each of an `apply`'s four loops.
    fn mutation_bases(&self) -> Vec<Case<GpParams>> {
        let mut bases = Vec::new();
        for style in STYLES {
            let s = style_name(style);
            for (saturation, power) in [(false, false), (false, true), (true, false), (true, true)]
            {
                bases.push(Case::new(
                    format!("{s}, base, saturation {saturation}, power {power}"),
                    GpParams::new(style, loop_values(style, saturation, power)),
                ));
            }
        }
        bases
    }

    /// `GradingPrimaryTransform(values, style, dynamic, dir)`.
    fn spec(&self, p: &GpParams, direction: Direction) -> Spec {
        Spec::Transform(p.transform(transform_direction(direction)))
    }

    /// The CPU processor of the op the wheel builds ([`GpParams::op_data`]), with the
    /// combination's optimization.
    fn port(&self, p: &GpParams, combo: &Combo) -> Result<Port, String> {
        let cpu = p.cpu(transform_direction(combo.direction), combo.fast_math)?;
        Ok(Port::in_place(move |px| apply(&cpu, px)))
    }

    /// Alpha, which the renderers never write; none when the optimizer removes the op (an
    /// identity, not dynamic), where alpha is the CPU processor's to copy.
    fn pass_through(&self, p: &GpParams, combo: &Combo) -> Channels {
        match p.op_data(transform_direction(combo.direction)) {
            Ok(data) if !data.is_no_op() => A,
            _ => [false; 4],
        }
    }

    /// Buffers of NaNs, each channel's its own (the battery's NaN buffers, and
    /// [`distinct_nans`]).
    fn extra_probes(&self, _: &GpParams, _: Direction) -> Vec<ProbeSet> {
        vec![
            ProbeSet::NanBuffers { max_pixels: 8 },
            ProbeSet::Values {
                name: "distinct NaNs".to_string(),
                values: distinct_nans(),
            },
        ]
    }

    /// The pivots and the clamps: the gamma's sign changes at the black pivot, and the clamps
    /// cut.
    fn breakpoints(&self, p: &GpParams, _: Direction) -> Vec<f32> {
        let v = &p.values;
        [v.pivot_black, v.pivot_white, v.clamp_black, v.clamp_white]
            .into_iter()
            .map(|x| x as f32)
            .filter(|x| x.is_finite())
            .collect()
    }
}

/// Values with a NaN payload of their own each (quiet and signalling, either sign), the
/// infinities and finite values, cycled through the channels: the NaNs of one pixel's channels
/// meet in the saturation's sums, and meet the NaN coefficients of infinite values, so each
/// output names the operand the wheel put first.
fn distinct_nans() -> Vec<f32> {
    let mut values = Vec::new();
    for k in 1..=24u32 {
        let payload = k * 0x111;
        values.push(f32::from_bits(0x7fc0_0000 | payload));
        values.push(f32::from_bits(0xffc0_0000 | (payload << 1)));
        values.push(f32::from_bits(0x7f80_0000 | (payload << 2)));
        values.push(f32::from_bits(0xff80_0000 | (payload << 3)));
    }
    values.extend([
        f32::INFINITY,
        f32::NEG_INFINITY,
        0.5,
        -2.0,
        1e30,
        -1e-30,
        0.0,
        -0.0,
    ]);
    values
}

#[test]
fn renderers_match_the_wheel() {
    battery::run(&GpFamily);
}

// ---------------------------------------------------------------------------------------------
// Dynamic sets.
// ---------------------------------------------------------------------------------------------

/// Pixels of each sign and size, NaNs and the infinities in the colors and in alpha.
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
        0.3,
        0.6,
        0.9,
        f32::INFINITY,
        0.4,
        0.1,
        -0.2,
        f32::from_bits(0xffc0_1234),
    ]
}

/// The values set between applies: each loop's, upstream's, an identity (the bypass), clamps,
/// a saturation of 0, NaN and infinite values.
fn sets(style: GradingStyle) -> Vec<GradingPrimary> {
    let mut out = vec![
        loop_values(style, true, true),
        loop_values(style, false, false),
        upstream_values(style),
        GradingPrimary::new(style),
        loop_values(style, true, false),
        loop_values(style, false, true),
    ];
    let mut p = upstream_values(style);
    upstream_clamps_and_pivots(style, &mut p);
    out.push(p);
    let mut p = loop_values(style, true, true);
    p.saturation = 0.0;
    out.push(p);
    let mut p = loop_values(style, true, true);
    p.brightness.red = f64::NAN;
    p.offset.blue = f64::NAN;
    out.push(p);
    let mut p = loop_values(style, true, true);
    p.pivot_black = f64::INFINITY;
    p.pivot_white = f64::INFINITY;
    out.push(p);
    // A value the setter refuses (a white pivot under the black one): the property keeps the
    // last value.
    let mut p = loop_values(style, false, true);
    p.pivot_white = -1.0;
    out.push(p);
    out.push(upstream_values(style));
    out
}

/// Each style in each direction, dynamic: the CPU processor's property set as `sets()` lists,
/// an apply after each set, against the wheel's `dynamic_apply` with the same steps, bit for
/// bit (W0002 doesn't apply: these are inputs, not the battery's NaN parameters). A set the
/// wheel refuses is refused by the port with the same text.
#[test]
fn dynamic_sets_apply_as_the_wheels() {
    let input = pixels();
    let bytes = f32_to_bytes(&input);
    let width = input.len() / 4;
    let mut failures = Vec::new();
    let mut compared = 0;
    let mut refused = 0;
    for style in STYLES {
        for direction in [TransformDirection::Forward, TransformDirection::Inverse] {
            let params = GpParams::new(style, upstream_values(style)).dynamic();
            let sets = sets(style);
            let mut steps = vec![
                json!({"get": "DYNAMIC_PROPERTY_GRADING_PRIMARY", "from": "cpu", "as": "p"}),
                json!({"apply": [0, 1]}),
            ];
            for value in &sets {
                steps.push(json!({"set": "p", "value": primary_spec(value)}));
                steps.push(json!({"apply": [0, 1]}));
            }
            let response = Oracle::get().call(
                "dynamic_apply",
                json!({
                    "transform": params.transform(direction),
                    "buffers": [{"blob": 0}, {"size": bytes.len(), "fill": [0]}],
                    "images": [
                        {"kind": "packed", "data": {"buffer": 0}, "width": width,
                         "height": 1, "num_channels": 4},
                        {"kind": "packed", "data": {"buffer": 1}, "width": width,
                         "height": 1, "num_channels": 4},
                    ],
                    "steps": steps,
                }),
                &[&bytes],
            );
            let result = &response.result;
            assert!(result.get("exception").is_none(), "{result}");
            let wheel_steps = result["steps"].as_array().unwrap();
            assert_eq!(wheel_steps.len(), 2 + 2 * sets.len(), "{result}");

            // The port: the CPU processor of a forward processor of the op the wheel builds,
            // its property set the same way.
            let cpu = params.cpu(direction, true).unwrap();
            assert!(cpu.has_dynamic_property(DynamicPropertyType::GradingPrimary));
            assert_eq!(wheel_steps[0]["has"], json!(true), "{result}");
            let property = cpu
                .get_dynamic_property(DynamicPropertyType::GradingPrimary)
                .unwrap()
                .as_grading_primary()
                .unwrap();
            let mut applies: Vec<Option<&GradingPrimary>> = vec![None];
            applies.extend(sets.iter().map(Some));
            for (k, set) in applies.iter().enumerate() {
                if let Some(value) = set {
                    let wheel_set = &wheel_steps[2 * k];
                    match property.set_value(value) {
                        Ok(()) => {
                            assert!(wheel_set.get("exception").is_none(), "{wheel_set}");
                        }
                        Err(e) => {
                            assert_eq!(
                                wheel_set["exception"]["message"],
                                json!(e.to_string()),
                                "{wheel_set}"
                            );
                            refused += 1;
                        }
                    }
                }
                let mut port = input.clone();
                apply(&cpu, &mut port);
                let step = &wheel_steps[2 * k + 1];
                let index = step["result"]["buffers"][1]
                    .as_u64()
                    .unwrap_or_else(|| panic!("{step}")) as usize;
                let wheel = bytes_to_f32(&response.blobs[index]);
                assert_eq!(wheel.len(), port.len());
                compared += 1;
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
                        "{style:?} {direction:?} after {set:?}: {}",
                        differ.join(", ")
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert_eq!(
        compared,
        STYLES.len() * 2 * (1 + sets(GradingStyle::Log).len())
    );
    assert_eq!(refused, STYLES.len() * 2);
}

/// The renderer's property is a copy of the op data's: a set on the op data's own property
/// after the renderer is built leaves the renderer's output as it was, a set on the renderer's
/// property changes it. The wheel (`GradingPrimaryOpCPU::GradingPrimaryOpCPU`, which decouples
/// a dynamic property with `createEditableCopy`, GradingPrimaryOpCPU.cpp:36-44 @ v2.5.2) gives
/// the output at the values the op was built with, at another set on the CPU processor's
/// property, and at a third; the port's renderer must give the first after a set of its op
/// data's property, and the second after the same set of its own.
///
/// The port's step is a unit-level test: the CPU processor clones its ops, so a set on the
/// original property is out of the pipeline's reach, and the test builds the renderer of an op
/// data directly (`get_grading_primary_cpu_renderer`).
#[test]
fn renderer_property_is_decoupled_from_the_op() {
    let input = pixels();
    let bytes = f32_to_bytes(&input);
    let width = input.len() / 4;
    let mut compared = 0;
    for style in STYLES {
        for direction in [TransformDirection::Forward, TransformDirection::Inverse] {
            let params = GpParams::new(style, upstream_values(style)).dynamic();
            // The set on the op data's property, and the set on the renderer's.
            let on_op = loop_values(style, false, true);
            let on_renderer = loop_values(style, true, true);
            let response = Oracle::get().call(
                "dynamic_apply",
                json!({
                    "transform": params.transform(direction),
                    "buffers": [{"blob": 0}, {"size": bytes.len(), "fill": [0]}],
                    "images": [
                        {"kind": "packed", "data": {"buffer": 0}, "width": width,
                         "height": 1, "num_channels": 4},
                        {"kind": "packed", "data": {"buffer": 1}, "width": width,
                         "height": 1, "num_channels": 4},
                    ],
                    "steps": [
                        {"get": "DYNAMIC_PROPERTY_GRADING_PRIMARY", "from": "cpu", "as": "p"},
                        {"apply": [0, 1]},
                        {"set": "p", "value": primary_spec(&on_op)},
                        {"apply": [0, 1]},
                        {"set": "p", "value": primary_spec(&on_renderer)},
                        {"apply": [0, 1]},
                    ],
                }),
                &[&bytes],
            );
            let result = &response.result;
            assert!(result.get("exception").is_none(), "{result}");
            let wheel_steps = result["steps"].as_array().unwrap();
            assert_eq!(wheel_steps.len(), 6, "{result}");
            let wheel = |step: usize| -> Vec<u32> {
                let index = wheel_steps[step]["result"]["buffers"][1]
                    .as_u64()
                    .unwrap_or_else(|| panic!("{}", wheel_steps[step]))
                    as usize;
                bytes_to_f32(&response.blobs[index])
                    .iter()
                    .map(|v| v.to_bits())
                    .collect()
            };
            let (at_built, at_op_set, at_renderer_set) = (wheel(1), wheel(3), wheel(5));
            // The sets change the output, so a renderer that followed the op would differ.
            assert_ne!(at_built, at_op_set, "{style:?} {direction:?}");
            assert_ne!(at_built, at_renderer_set, "{style:?} {direction:?}");

            let data = params.op_data(direction).unwrap();
            let renderer = get_grading_primary_cpu_renderer(&data).unwrap();
            let render = || -> Vec<u32> {
                let mut px = input.clone();
                renderer.apply(&mut px);
                px.iter().map(|v| v.to_bits()).collect()
            };
            assert_eq!(render(), at_built, "{style:?} {direction:?}: built");

            // A set on the op data's property: the renderer keeps its values.
            data.get_dynamic_property_internal()
                .set_value(&on_op)
                .unwrap();
            assert_eq!(
                render(),
                at_built,
                "{style:?} {direction:?}: the renderer follows the op's property"
            );

            // A set on the renderer's property.
            renderer
                .get_dynamic_property(DynamicPropertyType::GradingPrimary)
                .unwrap()
                .as_grading_primary()
                .unwrap()
                .set_value(&on_renderer)
                .unwrap();
            assert_eq!(
                render(),
                at_renderer_set,
                "{style:?} {direction:?}: renderer set"
            );
            compared += 1;
        }
    }
    assert_eq!(compared, STYLES.len() * 2);
}
