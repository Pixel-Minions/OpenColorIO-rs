// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The GradingTone op's renderers against the wheel's `GradingToneTransform`:
//! - the battery (`renderers_match_the_wheel`): the three styles in both directions with fast math on and off (the renderers
//!   ignore it), each tone alone and all of them together with values below and above 1 (each
//!   of the curves' forward and reverse branches, the whites' and blacks' decreasing and
//!   increasing cases, the s-contrast above and below 1), dynamic and fixed values, upstream's
//!   test values, starts and widths whose pre-render form overflows to infinities (NaN
//!   coefficients of either sign), the starts the pre-render clamps, and the generated
//!   extreme, NaN and infinite values, on the tier's probes, the curves' knots and the NaN
//!   probes;
//! - dynamic sets (`dynamic_sets_apply_as_the_wheels`): the CPU processor's property set one
//!   value after another between applies (`dynamic_apply`), against the port's CPU processor
//!   with the same steps.
//!
//! The port's side is the CPU processor of the op the wheel builds (`CPUProcessor` of a
//! processor of one GradingTone op): the optimizer removes an op that is an identity and not
//! dynamic, as the wheel's does.

use ocio_ops::cpu_processor::CpuProcessor;
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{
    BitDepth, DynamicPropertyType, GradingStyle, OptimizationFlags, TransformDirection,
};
use ocio_ops::ops::gradingtone::grading_tone::{GradingRgbmsw, GradingTone, GradingTonePreRender};
use ocio_ops::ops::gradingtone::grading_tone_op::create_grading_tone_op;
use ocio_ops::ops::gradingtone::grading_tone_op_data::GradingToneOpData;
use ocio_testkit::Oracle;
use ocio_testkit::battery::params::{A, B, Case, Channels, G, Params, Precision, R, RGB, Slot};
use ocio_testkit::battery::{self, Combo, Direction, Family, Port, Spec};
use ocio_testkit::oracle::{bytes_to_f32, f32_to_bytes};
use ocio_testkit::probe::{ProbeSet, Rng};
use ocio_testkit::transform_text::f64_spec;
use serde_json::{Value, json};

/// The styles.
const STYLES: [GradingStyle; 3] = [GradingStyle::Log, GradingStyle::Lin, GradingStyle::Video];

/// The linear value whose log the linear style's renderers grade is `t`: the inverse of their
/// `LinLog` (`GradingToneOpCPU.cpp:1063-1115` @ v2.5.2) in double, to put probes near the
/// curves' control points, which are in that log space.
fn lin_of_log(t: f32) -> f32 {
    let t = f64::from(t);
    let shift = -0.000157849851665374f64;
    if t > -5.5 {
        (t.exp2() * (0.18 + shift) - shift) as f32
    } else {
        ((t + 7.0) / 363.034608563) as f32
    }
}

/// `points` in the style's pixel space: the linear style's through [`lin_of_log`], with the
/// break of its `LinLog`.
fn to_pixel_space(style: GradingStyle, mut points: Vec<f32>) -> Vec<f32> {
    if style == GradingStyle::Lin {
        points = points.into_iter().map(lin_of_log).collect();
        points.push(0.0041318374739483946);
    }
    points
}

/// The tones, by name, as the slots name them, in the order of [`field`].
const TONES: [&str; 5] = ["blacks", "shadows", "midtones", "highlights", "whites"];

/// The number of doubles in a GradingTone.
const NUM_FIELDS: usize = 31;

/// The fields of a GradingTone, by slot: the 5 x 6 RGBMSW components (red, green, blue,
/// master, start, width of each of [`TONES`]), then the s-contrast.
fn field(t: &mut GradingTone, index: usize) -> &mut f64 {
    fn rgbmsw(v: &mut GradingRgbmsw, c: usize) -> &mut f64 {
        match c {
            0 => &mut v.red,
            1 => &mut v.green,
            2 => &mut v.blue,
            3 => &mut v.master,
            4 => &mut v.start,
            _ => &mut v.width,
        }
    }
    match index {
        0..=5 => rgbmsw(&mut t.blacks, index),
        6..=11 => rgbmsw(&mut t.shadows, index - 6),
        12..=17 => rgbmsw(&mut t.midtones, index - 12),
        18..=23 => rgbmsw(&mut t.highlights, index - 18),
        24..=29 => rgbmsw(&mut t.whites, index - 24),
        _ => &mut t.scontrast,
    }
}

/// A GradingToneTransform's parameters: the style, the values, and whether it is dynamic.
#[derive(Debug, Clone)]
struct GtParams {
    style: GradingStyle,
    values: GradingTone,
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

fn rgbmsw_spec(v: &GradingRgbmsw) -> Value {
    json!({"object": {"class": "GradingRGBMSW",
                      "args": [f64_spec(v.red), f64_spec(v.green), f64_spec(v.blue),
                               f64_spec(v.master), f64_spec(v.start), f64_spec(v.width)]}})
}

/// A GradingTone value spec, every double by its bits.
fn tone_spec(t: &GradingTone) -> Value {
    json!({"object": {"class": "GradingTone", "args": [{"enum": "GRADING_LOG"}], "attrs": [
        ["blacks", rgbmsw_spec(&t.blacks)],
        ["shadows", rgbmsw_spec(&t.shadows)],
        ["midtones", rgbmsw_spec(&t.midtones)],
        ["highlights", rgbmsw_spec(&t.highlights)],
        ["whites", rgbmsw_spec(&t.whites)],
        ["scontrast", f64_spec(t.scontrast)],
    ]}})
}

impl GtParams {
    fn new(style: GradingStyle, values: GradingTone) -> Self {
        GtParams {
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
    /// PyGradingToneTransform.cpp:17-35 @ v2.5.2: `Create(style)`, `setStyle`, `setValue`,
    /// `makeDynamic`, `setDirection`, `validate`), then `BuildGradingToneOp`'s copy of it
    /// (src/OpenColorIO/ops/gradingtone/GradingToneOp.cpp:232-244); or the exception of values
    /// the transform refuses.
    fn op_data(&self, direction: TransformDirection) -> Result<GradingToneOpData, String> {
        let mut data = GradingToneOpData::new(self.style);
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
        create_grading_tone_op(
            &mut raw,
            std::hint::black_box(self.op_data(direction)?),
            TransformDirection::Forward,
        );
        raw.finalize().unwrap();
        Ok(CpuProcessor::new(&raw, BitDepth::F32, BitDepth::F32, flags(fast_math)).unwrap())
    }

    /// The transform spec, every value by its bits.
    fn transform(&self, direction: TransformDirection) -> Value {
        json!({"class": "GradingToneTransform", "args": {
            "values": tone_spec(&self.values),
            "style": {"enum": style_name(self.style)},
            "dynamic": self.dynamic,
            "dir": {"enum": dir_name(direction)},
        }})
    }
}

impl Params for GtParams {
    /// Each tone's red, green and blue component applies to its channel; the master
    /// component, the start, the width and the s-contrast to the three (the highlights' and
    /// shadows' master components also move the whites' and blacks' ranges).
    fn slots(&self) -> Vec<Slot> {
        let mut slots = Vec::with_capacity(NUM_FIELDS);
        for name in TONES {
            for (c, channels) in [
                ("red", R),
                ("green", G),
                ("blue", B),
                ("master", RGB),
                ("start", RGB),
                ("width", RGB),
            ] {
                slots.push(Slot::new(
                    format!("{name}.{c}"),
                    Precision::F64AsF32,
                    channels,
                ));
            }
        }
        slots.push(Slot::new("scontrast", Precision::F64AsF32, RGB));
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

/// `GradingRGBMSW(red, green, blue, master, start, width)`.
fn sw(red: f64, green: f64, blue: f64, master: f64, start: f64, width: f64) -> GradingRgbmsw {
    GradingRgbmsw::new(red, green, blue, master, start, width)
}

/// The components of `v` set, its start and width kept.
fn with_rgbm(v: GradingRgbmsw, red: f64, green: f64, blue: f64, master: f64) -> GradingRgbmsw {
    GradingRgbmsw {
        red,
        green,
        blue,
        master,
        ..v
    }
}

/// Every tone on, the s-contrast too. `low` puts the values on one side of 1 and the other
/// side on the others: each curve's forward and reverse branch, the whites' and blacks'
/// decreasing and increasing cases.
fn all_on(style: GradingStyle, low: bool) -> GradingTone {
    let mut t = GradingTone::new(style);
    let (a, b) = if low { (0.6, 1.45) } else { (1.45, 0.6) };
    t.midtones = with_rgbm(t.midtones, a, b, 1.0, 1.2);
    t.highlights = with_rgbm(t.highlights, b, a, 1.0, 0.8);
    t.shadows = with_rgbm(t.shadows, a, b, 1.0, 1.25);
    t.whites = with_rgbm(t.whites, a, b, 1.0, 1.3);
    t.blacks = with_rgbm(t.blacks, b, a, 1.0, 0.75);
    t.scontrast = if low { 1.35 } else { 0.7 };
    t
}

/// Upstream's test values (tests/cpu/ops/gradingtone/GradingToneOpCPU_tests.cpp @ v2.5.2:
/// TS1 to TS6), each on its own, in `style`.
fn upstream_values(style: GradingStyle) -> Vec<(&'static str, GradingTone)> {
    let mut out = Vec::new();
    let mut t = GradingTone::new(style);
    t.midtones = sw(0.3, 1.0, 1.8, 1.2, 0.47, 0.6);
    out.push(("TS1 midtones", t));
    let mut t = GradingTone::new(style);
    t.highlights = sw(0.3, 1.0, 1.8, 1.4, -0.1, 0.9);
    out.push(("TS2 highlights", t));
    let mut t = GradingTone::new(style);
    t.shadows = sw(0.3, 1., 1.79, 0.6, 0.8, -0.1);
    out.push(("TS3 shadows", t));
    let mut t = GradingTone::new(style);
    t.whites = sw(0.3, 1., 1.9, 0.6, -0.2, 1.4);
    out.push(("TS4 whites", t));
    let mut t = GradingTone::new(style);
    t.blacks = sw(0.3, 1., 1.9, 0.6, 0.8, 0.9);
    out.push(("TS5 blacks", t));
    for scontrast in [1.8, 0.3] {
        let mut t = GradingTone::new(style);
        t.scontrast = scontrast;
        out.push(("TS6 s-contrast", t));
    }
    out
}

/// Highlights starts and pivots where `0.5 / (x2 - x0)` differs from `0.5 / ((x2 - x1) +
/// (x1 - x0))` in `float` (the pre-render's `val < 1` and `val > 1` denominators of the
/// highlights' middle control point), with x0, x1 and x2 as the pre-render makes them: a seeded
/// search over the inputs.
fn hs_denominator_inputs() -> Vec<(f64, f64)> {
    let mut rng = Rng::new(0x7013_0d4e);
    let mut out = Vec::new();
    while out.len() < 4 {
        let u = |rng: &mut Rng| (rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
        let pivot = 0.2 + 1.5 * u(&mut rng);
        let start = pivot - 0.02 - 2.0 * u(&mut rng);
        let x0 = start as f32;
        let x2 = pivot as f32;
        let x1 = x0 + (x2 - x0) * 0.5;
        if 0.5 / (x2 - x0) != 0.5 / ((x2 - x1) + (x1 - x0)) {
            out.push((start, pivot));
        }
    }
    out
}

/// Midtones where a curve segment's value at its upper knot (`x2`, `x3` or `x4`) isn't the
/// next segment's value there in `float`, as the pre-render makes the control points: the
/// `float` and `float3` code choose different segments on a knot (I-242), which then shows.
/// A seeded search over the inputs, for each knot (some knots never differ: the search gives
/// up there).
fn mids_knot_inputs(style: GradingStyle) -> Vec<GradingRgbmsw> {
    let mut rng = Rng::new(0x7013_0a1d ^ style as u64);
    let u = |rng: &mut Rng| (rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
    let mut out = Vec::new();
    // One for each of the knots x2, x3 and x4 where the search finds one: the red, green
    // and blue components only (the `float` code), and the master only (the `float3` code,
    // which then sees the pixel as it is).
    for (c, k) in [0, 3].into_iter().flat_map(|c| (1..4).map(move |k| (c, k))) {
        for _ in 0..20000 {
            let v = 0.1 + 1.8 * u(&mut rng);
            let (rgb, m) = if c == 3 { (1.0, v) } else { (v, 1.0) };
            let mut t = GradingTone::new(style);
            t.midtones = sw(
                rgb,
                rgb,
                rgb,
                m,
                0.1 + 0.6 * u(&mut rng),
                0.05 + 0.9 * u(&mut rng),
            );
            let mut pr = GradingTonePreRender::new(style);
            pr.update(&t);
            let (x, y, m) = (pr.mid_x[c], pr.mid_y[c], pr.mid_m[c]);
            let seg = |k: usize, t: f32| {
                let dx = x[k + 1] - x[k];
                let tk = (t - x[k]) / dx;
                tk * dx * (tk * 0.5 * (m[k + 1] - m[k]) + m[k]) + y[k]
            };
            if seg(k, x[k + 1]) != y[k + 1] {
                out.push(t.midtones);
                break;
            }
        }
    }
    assert!(out.len() >= 2, "{style:?}: {} knots found", out.len());
    out
}

fn explicit_cases() -> Vec<Case<GtParams>> {
    let mut cases = Vec::new();
    for style in STYLES {
        let s = style_name(style);
        for (label, t) in upstream_values(style) {
            cases.push(Case::new(
                format!("{s}, upstream's {label}"),
                GtParams::new(style, t),
            ));
        }
        cases.push(Case::new(
            format!("{s}, upstream's TS1 midtones, dynamic"),
            GtParams::new(style, upstream_values(style)[0].1).dynamic(),
        ));
        for low in [true, false] {
            cases.push(Case::new(
                format!("{s}, all tones, low {low}"),
                GtParams::new(style, all_on(style, low)),
            ));
            cases.push(Case::new(
                format!("{s}, all tones, low {low}, dynamic"),
                GtParams::new(style, all_on(style, low)).dynamic(),
            ));
        }
        cases.push(Case::new(
            format!("{s}, identity"),
            GtParams::new(style, GradingTone::new(style)),
        ));
        cases.push(Case::new(
            format!("{s}, identity, dynamic"),
            GtParams::new(style, GradingTone::new(style)).dynamic(),
        ));
        // Each component alone off 1, below and above: the renderers skip a 1, and the
        // components below and above 1 take each curve's two branches.
        for f in 0..NUM_FIELDS {
            if f % 6 >= 4 && f != 30 {
                continue;
            }
            for value in [0.55, 1.6] {
                let mut t = GradingTone::new(style);
                *field(&mut t, f) = value;
                let name = GtParams::new(style, t).slots()[f].name.clone();
                cases.push(Case::new(
                    format!("{s}, {name} = {value} alone"),
                    GtParams::new(style, t),
                ));
            }
        }
        // The components at their bounds (validate's, and the renderers' limits).
        for (label, v) in [("lower", 0.1), ("upper", 1.9)] {
            let mut t = GradingTone::new(style);
            t.midtones = with_rgbm(t.midtones, v, v, v, v);
            t.whites = with_rgbm(t.whites, v, v, v, v);
            t.blacks = with_rgbm(t.blacks, v, v, v, v);
            let hs = if v < 1.0 { 0.2 } else { 1.8 };
            t.highlights = with_rgbm(t.highlights, hs, hs, hs, hs);
            t.shadows = with_rgbm(t.shadows, hs, hs, hs, hs);
            t.scontrast = if v < 1.0 { 0.01 } else { 1.99 };
            cases.push(Case::new(
                format!("{s}, every value at its {label} bound"),
                GtParams::new(style, t),
            ));
        }
        // Starts the pre-render clamps to 0.01 from the pivot (validate allows 0.01 - 1e-6),
        // and starts just clear of it.
        for d in [0.00999901, 0.0099995, 0.0100005, 0.0105, 0.0109, 0.0111] {
            let mut t = all_on(style, true);
            t.highlights.start = t.highlights.width - d;
            t.shadows.start = t.shadows.width + d;
            cases.push(Case::new(
                format!("{s}, starts {d} from the pivots"),
                GtParams::new(style, t),
            ));
        }
        // Highlights whose middle control point's denominator rounds differently in its two
        // forms.
        for (start, pivot) in hs_denominator_inputs() {
            for low in [true, false] {
                let mut t = all_on(style, low);
                t.highlights.start = start;
                t.highlights.width = pivot;
                t.shadows.start = 1.0 - start;
                t.shadows.width = 1.0 - pivot;
                cases.push(Case::new(
                    format!("{s}, highlights start {start} pivot {pivot}, low {low}"),
                    GtParams::new(style, t),
                ));
            }
        }
        // Starts and widths whose pre-render form overflows a float or a double: infinite
        // control points, whose differences are NaN, and the NaN of either sign the
        // pre-render's inverse evaluations give the whites' and blacks' ranges.
        for (k, (start, width)) in [
            (-3e38, 3e38),
            (-1.7e38, 1.7e38),
            (-1e300, 1e300),
            (-f64::MAX, f64::MAX),
            (-1e30, 1e30),
        ]
        .into_iter()
        .enumerate()
        {
            for low in [true, false] {
                let mut t = all_on(style, low);
                t.highlights.start = start;
                t.highlights.width = width;
                t.shadows.start = width;
                t.shadows.width = start;
                cases.push(Case::new(
                    format!("{s}, overflowing highlights and shadows {k}, low {low}"),
                    GtParams::new(style, t),
                ));
                let mut t = all_on(style, low);
                t.whites.start = start;
                t.whites.width = width;
                t.blacks.start = width;
                t.blacks.width = width;
                cases.push(Case::new(
                    format!("{s}, overflowing whites and blacks {k}, low {low}"),
                    GtParams::new(style, t),
                ));
                let mut t = all_on(style, low);
                t.highlights.start = start;
                t.highlights.width = 0.5;
                t.whites.start = width;
                t.whites.width = width;
                t.shadows.start = width;
                t.shadows.width = 0.5;
                t.blacks.start = start;
                t.blacks.width = width;
                cases.push(Case::new(
                    format!("{s}, overflowing ranges {k}, low {low}"),
                    GtParams::new(style, t),
                ));
            }
        }
        // A start whose pre-render form is infinite (or beyond a double's reach) while the
        // blacks or whites move the master curve's decreasing path: the path meets NaN
        // coefficients (the verifier's table flips of WB_FWD f3 lanes 0-2 `w9` and WB_REV
        // scalar whites and f3 lanes 0-2 `v8` survived without these).
        for (label, start) in [
            ("shadows.start inf", f64::INFINITY),
            ("shadows.start 1e300", 1e300),
        ] {
            let mut t = GradingTone::new(style);
            t.blacks = with_rgbm(t.blacks, 1.6, 1.6, 1.6, 1.6);
            t.shadows.start = start;
            cases.push(Case::new(
                format!("{s}, blacks 1.6, {label}"),
                GtParams::new(style, t),
            ));
        }
        for (label, start) in [
            ("highlights.start -1e300", -1e300),
            ("highlights.start -inf", f64::NEG_INFINITY),
        ] {
            let mut t = GradingTone::new(style);
            t.whites = with_rgbm(t.whites, 0.55, 0.55, 0.55, 0.55);
            t.highlights.start = start;
            cases.push(Case::new(
                format!("{s}, whites 0.55, {label}"),
                GtParams::new(style, t),
            ));
        }
        // The s-contrast alone at values where `t < x1` and `t <= x1` of the forward
        // renderer's top segment differ in `float` (the verifier's mutant A24).
        if style != GradingStyle::Lin {
            for c in [0.7, 0.8918666442285068, 1.5222737455736959] {
                let mut t = GradingTone::new(style);
                t.scontrast = c;
                cases.push(Case::new(
                    format!("{s}, scontrast {c} alone"),
                    GtParams::new(style, t),
                ));
            }
        }
        // Midtones whose segments differ on a knot.
        for (k, midtones) in mids_knot_inputs(style).into_iter().enumerate() {
            let mut t = GradingTone::new(style);
            t.midtones = midtones;
            cases.push(Case::new(
                format!("{s}, midtones differing on a knot {k}"),
                GtParams::new(style, t),
            ));
        }
        // A NaN value in the JSON route, which the wheel accepts.
        let mut t = all_on(style, true);
        t.highlights.green = f64::NAN;
        cases.push(Case::new(
            format!("{s}, NaN value"),
            GtParams::new(style, t),
        ));
    }
    cases
}

struct GtFamily;

impl Family for GtFamily {
    type Params = GtParams;

    fn name(&self) -> String {
        "GradingToneTransform".to_string()
    }

    fn cases(&self) -> Vec<Case<GtParams>> {
        explicit_cases()
    }

    /// Per style, every tone on, each side of 1.
    fn mutation_bases(&self) -> Vec<Case<GtParams>> {
        let mut bases = Vec::new();
        for style in STYLES {
            let s = style_name(style);
            for low in [true, false] {
                bases.push(Case::new(
                    format!("{s}, base, low {low}"),
                    GtParams::new(style, all_on(style, low)),
                ));
            }
        }
        bases
    }

    /// `GradingToneTransform(values, style, dynamic, dir)`.
    fn spec(&self, p: &GtParams, direction: Direction) -> Spec {
        Spec::Transform(p.transform(transform_direction(direction)))
    }

    /// The CPU processor of the op the wheel builds ([`GtParams::op_data`]), with the
    /// combination's optimization.
    fn port(&self, p: &GtParams, combo: &Combo) -> Result<Port, String> {
        let cpu = p.cpu(transform_direction(combo.direction), combo.fast_math)?;
        Ok(Port::in_place(move |px| apply(&cpu, px)))
    }

    /// Alpha, which the log and video renderers never write; none for the linear style, and
    /// none when the optimizer removes the op (an identity, not dynamic), where alpha is the CPU
    /// processor's to copy.
    fn pass_through(&self, p: &GtParams, combo: &Combo) -> Channels {
        match p.op_data(transform_direction(combo.direction)) {
            // The linear style converts alpha to its log space and back (I-244).
            Ok(data) if !data.is_no_op() && p.style != GradingStyle::Lin => A,
            _ => [false; 4],
        }
    }

    /// Buffers of NaNs, each channel's its own, distinct NaN payloads, and ramps across each
    /// curve's span (some are narrow: a highlights curve between a start and a pivot 0.01
    /// apart).
    fn extra_probes(&self, p: &GtParams, direction: Direction) -> Vec<ProbeSet> {
        vec![
            ProbeSet::NanBuffers { max_pixels: 8 },
            ProbeSet::Values {
                name: "distinct NaNs".to_string(),
                values: distinct_nans(),
            },
            ProbeSet::Values {
                name: "ramps across the curves".to_string(),
                values: curve_ramps(p, direction),
            },
        ]
    }

    /// The curves' control points, as the pre-render makes them (forward: their x, inverse:
    /// their y): the segments change there, the `float` and `float3` code differently on a
    /// knot.
    fn breakpoints(&self, p: &GtParams, direction: Direction) -> Vec<f32> {
        let mut pr = GradingTonePreRender::new(p.style);
        pr.update(&p.values);
        let mut points = Vec::new();
        let forward = direction == Direction::Forward;
        for c in 0..4 {
            points.extend(if forward { pr.mid_x[c] } else { pr.mid_y[c] });
            for s in 0..2 {
                points.extend(if forward {
                    pr.hs_x[s][c]
                } else {
                    pr.hs_y[s][c]
                });
                points.extend(if forward {
                    pr.wb_x[s][c]
                } else {
                    pr.wb_y[s][c]
                });
            }
        }
        for s in 0..2 {
            points.extend(pr.sc_x[s]);
            points.extend(pr.sc_y[s]);
        }
        points.push(pr.pivot);
        let mut points = to_pixel_space(p.style, points);
        points.retain(|x| x.is_finite());
        points.sort_by(f32::total_cmp);
        points.dedup();
        points
    }
}

/// 24 values across each curve's span and a tenth beyond each end, from the pre-render's
/// control points (forward: their x, inverse: their y).
fn curve_ramps(p: &GtParams, direction: Direction) -> Vec<f32> {
    let mut pr = GradingTonePreRender::new(p.style);
    pr.update(&p.values);
    let forward = direction == Direction::Forward;
    let mut spans: Vec<(f32, f32)> = Vec::new();
    for c in 0..4 {
        let (x, y) = (pr.mid_x[c], pr.mid_y[c]);
        spans.push(if forward { (x[0], x[5]) } else { (y[0], y[5]) });
        for s in 0..2 {
            let (x, y) = (pr.hs_x[s][c], pr.hs_y[s][c]);
            spans.push(if forward { (x[0], x[2]) } else { (y[0], y[2]) });
            let (x, y) = (pr.wb_x[s][c], pr.wb_y[s][c]);
            spans.push(if forward { (x[0], x[1]) } else { (y[0], y[1]) });
        }
    }
    for s in 0..2 {
        let (x, y) = (pr.sc_x[s], pr.sc_y[s]);
        spans.push(if forward { (x[0], x[3]) } else { (y[0], y[3]) });
    }
    let mut values = Vec::new();
    for (lo, hi) in spans {
        let (lo, hi) = (lo.min(hi), lo.max(hi));
        let w = hi - lo;
        if !w.is_finite() || w <= 0.0 {
            continue;
        }
        for k in 0..24 {
            values.push(lo - 0.1 * w + 1.2 * w * k as f32 / 23.0);
        }
    }
    let mut values = to_pixel_space(p.style, values);
    values.retain(|x| x.is_finite());
    values.sort_by(f32::total_cmp);
    values.dedup();
    values
}

/// Values with a NaN payload of their own each (quiet and signalling, either sign), the
/// infinities and finite values: a pixel's NaN meets the coefficients' NaNs (of infinite
/// control points) and the default NaNs of infinities, so each output names the operand the
/// wheel put first.
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
        3e38,
        -3e38,
    ]);
    values
}

#[test]
fn renderers_match_the_wheel() {
    battery::run(&GtFamily);
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
        8.0,
        -0.05,
        0.45,
        f32::from_bits(0x7f80_0001),
    ]
}

/// The values set between applies: both sides of 1, upstream's, an identity (the bypass),
/// NaN and overflowing values, one the setter refuses.
fn sets(style: GradingStyle) -> Vec<GradingTone> {
    let mut out = vec![all_on(style, true), all_on(style, false)];
    out.extend(upstream_values(style).into_iter().map(|(_, t)| t));
    out.push(GradingTone::new(style));
    let mut t = all_on(style, false);
    t.whites.red = f64::NAN;
    t.scontrast = f64::NAN;
    out.push(t);
    let mut t = all_on(style, true);
    t.highlights.start = -3e38;
    t.highlights.width = 3e38;
    out.push(t);
    // A value the setter refuses: the property keeps the last value.
    let mut t = all_on(style, true);
    t.blacks.red = 2.0;
    out.push(t);
    out.push(all_on(style, true));
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
            let params = GtParams::new(style, all_on(style, true)).dynamic();
            let sets = sets(style);
            let mut steps = vec![
                json!({"get": "DYNAMIC_PROPERTY_GRADING_TONE", "from": "cpu", "as": "p"}),
                json!({"apply": [0, 1]}),
            ];
            for value in &sets {
                steps.push(json!({"set": "p", "value": tone_spec(value)}));
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
            assert!(cpu.has_dynamic_property(DynamicPropertyType::GradingTone));
            assert_eq!(wheel_steps[0]["has"], json!(true), "{result}");
            let property = cpu
                .get_dynamic_property(DynamicPropertyType::GradingTone)
                .unwrap()
                .as_grading_tone()
                .unwrap();
            let mut applies: Vec<Option<&GradingTone>> = vec![None];
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
