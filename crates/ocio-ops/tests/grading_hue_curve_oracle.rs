// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The GradingHueCurve op's renderers against the wheel's `GradingHueCurveTransform`:
//! - the battery (`renderers_match_the_wheel`): the three styles in both directions with fast
//!   math on and off (the renderers ignore it), with and without the RGB-to-HSY conversion,
//!   drawing the curves only, dynamic and fixed curves, upstream's test curves, curves whose
//!   gains overflow to infinities (where an infinite gain times 0 makes x86's default NaN meet
//!   a NaN pixel), hue-fx curves whose inverse wraps, and the generated extreme, NaN and
//!   infinite coordinates and slopes, on the tier's probes, pixels whose hue, saturation and
//!   luma sit on the curves' knots, and NaN probes;
//! - dynamic sets (`dynamic_sets_apply_as_the_wheels`): the CPU processor's property set one
//!   value after another between applies (`dynamic_apply`), against the port's CPU processor
//!   with the same steps;
//! - decoupling (`renderer_property_is_decoupled_from_the_op`): a set on the op data's property
//!   after the renderer is built doesn't reach the renderer, a set on the renderer's does.
//!
//! The port's side is the CPU processor of the op the wheel builds (`CPUProcessor` of a
//! processor of one GradingHueCurve op): the optimizer removes an op that is an identity and not
//! dynamic, as the wheel's does (a draw-curve-only one too, I-272).

use std::sync::Arc;

use ocio_ops::cpu_processor::CpuProcessor;
use ocio_ops::op::{CpuOp, OpVec};
use ocio_ops::open_color_types::{
    BSplineType, BitDepth, DynamicPropertyType, GradingStyle, HsyTransformStyle, HueCurveType,
    OptimizationFlags, TransformDirection,
};
use ocio_ops::ops::fixedfunction::fixed_function_op_cpu::get_fixed_function_cpu_renderer;
use ocio_ops::ops::fixedfunction::fixed_function_op_data::{
    FixedFunctionOpData, FixedFunctionOpStyle,
};
use ocio_ops::ops::gradinghuecurve::grading_hue_curve::GradingHueCurve;
use ocio_ops::ops::gradinghuecurve::grading_hue_curve_op::create_grading_hue_curve_op;
use ocio_ops::ops::gradinghuecurve::grading_hue_curve_op_cpu::get_grading_hue_curve_cpu_renderer;
use ocio_ops::ops::gradinghuecurve::grading_hue_curve_op_data::GradingHueCurveOpData;
use ocio_ops::ops::gradingrgbcurve::grading_b_spline_curve::{
    GradingBSplineCurve, GradingControlPoint,
};
use ocio_testkit::Oracle;
use ocio_testkit::battery::params::{A, Case, Channels, Params, Precision, RGB, Slot};
use ocio_testkit::battery::{self, Combo, Direction, Family, Port, Spec};
use ocio_testkit::oracle::{bytes_to_f32, f32_to_bytes};
use ocio_testkit::probe::{ProbeSet, Rng};
use ocio_testkit::transform_text::f64_spec;
use serde_json::{Value, json};

/// The styles.
const STYLES: [GradingStyle; 3] = [GradingStyle::Log, GradingStyle::Lin, GradingStyle::Video];

/// The curves' names, as the binding's properties and the slots name them.
const NAMES: [&str; 8] = [
    "hue_hue", "hue_sat", "hue_lum", "lum_sat", "sat_sat", "lum_lum", "sat_lum", "hue_fx",
];

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

fn hsy_name(hsy: HsyTransformStyle) -> &'static str {
    match hsy {
        HsyTransformStyle::None => "HSY_TRANSFORM_NONE",
        HsyTransformStyle::Transform1 => "HSY_TRANSFORM_1",
    }
}

fn spline_name(t: BSplineType) -> &'static str {
    match t {
        BSplineType::BSpline => "B_SPLINE",
        BSplineType::DiagonalBSpline => "DIAGONAL_B_SPLINE",
        BSplineType::HueHueBSpline => "HUE_HUE_B_SPLINE",
        BSplineType::Periodic1BSpline => "PERIODIC_1_B_SPLINE",
        BSplineType::Periodic0BSpline => "PERIODIC_0_B_SPLINE",
        BSplineType::Horizontal1BSpline => "HORIZONTAL1_B_SPLINE",
    }
}

fn transform_direction(direction: Direction) -> TransformDirection {
    match direction {
        Direction::Forward => TransformDirection::Forward,
        Direction::Inverse => TransformDirection::Inverse,
    }
}

fn flags(fast_math: bool) -> OptimizationFlags {
    if fast_math {
        OptimizationFlags::DEFAULT
    } else {
        OptimizationFlags(OptimizationFlags::DEFAULT.0 & !OptimizationFlags::FAST_LOG_EXP_POW.0)
    }
}

/// A float as a spec value, by its bits.
fn f32_spec(v: f32) -> Value {
    f64_spec(f64::from(v))
}

/// A curve's object spec: the binding's constructor from its control points, then its slopes
/// and its spline type.
fn curve_spec(curve: &GradingBSplineCurve) -> Value {
    let values: Vec<Value> = curve
        .control_points()
        .iter()
        .flat_map(|p| [f32_spec(p.x), f32_spec(p.y)])
        .collect();
    let slopes: Vec<Value> = curve.slopes().iter().map(|&s| f32_spec(s)).collect();
    json!({"object": {"class": "GradingBSplineCurve", "args": [values], "calls": [
        ["setSlopes", slopes],
        ["setSplineType", {"enum": spline_name(curve.spline_type())}],
    ]}})
}

/// The curves' object spec: the binding's constructor of a style's defaults, each curve copied
/// in through its property (`CopyGradingBSpline`, src/bindings/python/PyGradingData.cpp:22-33
/// @ v2.5.2, which validates nothing), then `setDrawCurveOnly`; the transform's `setValue`
/// validates them.
fn hue_curve_spec(curves: &GradingHueCurve) -> Value {
    let attrs: Vec<Value> = NAMES
        .iter()
        .zip(curves.curves())
        .map(|(name, curve)| json!([name, curve_spec(curve)]))
        .collect();
    json!({"object": {"class": "GradingHueCurve", "args": [{"enum": "GRADING_LOG"}],
                      "attrs": attrs,
                      "calls": [["setDrawCurveOnly", curves.get_draw_curve_only()]]}})
}

/// A GradingHueCurveTransform's parameters: the style, whether it converts RGB to HSY, whether
/// it is dynamic, whether its curves only draw, and the eight curves as control points and
/// slopes. Every x and y coordinate and slope is a slot (the curves mix the channels, so each
/// applies to R, G and B). `knot_probes` adds the pixels on the curves' knots: the explicit
/// cases have them, the generated ones don't (their bases are copies without them).
#[derive(Debug, Clone)]
struct HcParams {
    style: GradingStyle,
    rgb_to_hsy: HsyTransformStyle,
    dynamic: bool,
    draw: bool,
    points: [Vec<(f32, f32)>; 8],
    slopes: [Vec<f32>; 8],
    /// A spline type other than the slot's, which only a draw-curve-only op accepts.
    splines: [Option<BSplineType>; 8],
    knot_probes: bool,
}

impl HcParams {
    /// The curves, with default slopes.
    fn new(style: GradingStyle, points: [&[(f32, f32)]; 8]) -> Self {
        HcParams {
            style,
            rgb_to_hsy: HsyTransformStyle::Transform1,
            dynamic: false,
            draw: false,
            points: points.map(<[(f32, f32)]>::to_vec),
            slopes: points.map(|p| vec![0.0; p.len()]),
            splines: [None; 8],
            knot_probes: true,
        }
    }

    /// The default curves of `style`.
    fn defaults(style: GradingStyle) -> Self {
        let curves = GradingHueCurve::new(style);
        let points = curves.curves().clone().map(|c| {
            c.control_points()
                .iter()
                .map(|p| (p.x, p.y))
                .collect::<Vec<_>>()
        });
        let refs: [&[(f32, f32)]; 8] = std::array::from_fn(|c| points[c].as_slice());
        HcParams::new(style, refs)
    }

    fn dynamic(mut self) -> Self {
        self.dynamic = true;
        self
    }

    fn draw(mut self) -> Self {
        self.draw = true;
        self
    }

    fn no_hsy(mut self) -> Self {
        self.rgb_to_hsy = HsyTransformStyle::None;
        self
    }

    fn style(mut self, style: GradingStyle) -> Self {
        self.style = style;
        self
    }

    /// The same parameters with curve `c` replaced.
    fn with_curve(mut self, c: HueCurveType, points: &[(f32, f32)]) -> Self {
        self.points[c as usize] = points.to_vec();
        self.slopes[c as usize] = vec![0.0; points.len()];
        self
    }

    /// The same curves with these slopes on curve `c`.
    fn with_slopes(mut self, c: HueCurveType, slopes: &[f32]) -> Self {
        assert_eq!(slopes.len(), self.points[c as usize].len(), "{c:?}");
        self.slopes[c as usize] = slopes.to_vec();
        self
    }

    /// The same curves with curve `c` of spline type `t`.
    fn with_spline(mut self, c: HueCurveType, t: BSplineType) -> Self {
        self.splines[c as usize] = Some(t);
        self
    }

    /// Each slot's curve, what it is (0 x, 1 y, 2 slope), and its point.
    fn slot_places(&self) -> Vec<(usize, usize, usize)> {
        let mut places = Vec::new();
        for c in 0..8 {
            for i in 0..self.points[c].len() {
                places.extend([(c, 0, i), (c, 1, i), (c, 2, i)]);
            }
        }
        places
    }

    /// The port's curves, each of its type's spline type.
    fn curves(&self) -> GradingHueCurve {
        let mut curves = GradingHueCurve::new(GradingStyle::Log);
        for (c, t) in HueCurveType::CURVES.into_iter().enumerate() {
            let points: Vec<GradingControlPoint> = self.points[c]
                .iter()
                .map(|&(x, y)| GradingControlPoint::new(x, y))
                .collect();
            let mut curve = GradingBSplineCurve::with_points_and_hue_curve_type(&points, t);
            if let Some(spline) = self.splines[c] {
                curve.set_spline_type(spline);
            }
            for (i, &s) in self.slopes[c].iter().enumerate() {
                curve.set_slope(i, s).unwrap();
            }
            *curves.curve_mut(t).unwrap() = curve;
        }
        curves.set_draw_curve_only(self.draw);
        curves
    }

    /// The op data the binding's constructor makes (src/bindings/python/transforms/
    /// PyGradingHueCurveTransform.cpp:17-34 @ v2.5.2: `Create(style)`, `setStyle`, `setValue`,
    /// `makeDynamic`, `setDirection`, `validate`), then the spec's `setRGBToHSY`, then
    /// `BuildGradingHueCurveOp`'s copy of it (src/OpenColorIO/ops/gradinghuecurve/
    /// GradingHueCurveOp.cpp:234-245); or the exception of curves the transform refuses.
    fn op_data(&self, direction: TransformDirection) -> Result<GradingHueCurveOpData, String> {
        let mut data = GradingHueCurveOpData::new(self.style);
        data.set_value(&self.curves()).map_err(|e| e.to_string())?;
        if self.dynamic {
            data.get_dynamic_property_internal().make_dynamic();
        }
        data.set_direction(direction);
        data.validate().map_err(|e| e.to_string())?;
        data.set_rgb_to_hsy(self.rgb_to_hsy);
        data.validate().map_err(|e| e.to_string())?;
        Ok(data.clone())
    }

    /// The port's CPU processor of the processor's one op, for F32 images.
    fn cpu(&self, direction: TransformDirection, fast_math: bool) -> Result<CpuProcessor, String> {
        let mut raw = OpVec::new();
        create_grading_hue_curve_op(
            &mut raw,
            std::hint::black_box(self.op_data(direction)?),
            TransformDirection::Forward,
        );
        raw.finalize().unwrap();
        CpuProcessor::new(&raw, BitDepth::F32, BitDepth::F32, flags(fast_math))
            .map_err(|e| e.to_string())
    }

    /// The transform spec, every value by its bits.
    fn transform(&self, direction: TransformDirection) -> Value {
        json!({"class": "GradingHueCurveTransform",
            "args": {
                "values": hue_curve_spec(&self.curves()),
                "style": {"enum": style_name(self.style)},
                "dynamic": self.dynamic,
                "dir": {"enum": dir_name(direction)},
            },
            "calls": [["setRGBToHSY", {"enum": hsy_name(self.rgb_to_hsy)}]],
        })
    }
}

impl Params for HcParams {
    fn slots(&self) -> Vec<Slot> {
        self.slot_places()
            .into_iter()
            .map(|(c, what, i)| {
                let what = ["x", "y", "slope"][what];
                Slot::new(format!("{}[{i}].{what}", NAMES[c]), Precision::F32, RGB)
            })
            .collect()
    }
    fn get(&self, index: usize) -> f64 {
        let (c, what, i) = self.slot_places()[index];
        f64::from(match what {
            0 => self.points[c][i].0,
            1 => self.points[c][i].1,
            _ => self.slopes[c][i],
        })
    }
    fn set(&mut self, index: usize, value: f64) {
        let (c, what, i) = self.slot_places()[index];
        let value = value as f32;
        match what {
            0 => self.points[c][i].0 = value,
            1 => self.points[c][i].1 = value,
            _ => self.slopes[c][i] = value,
        }
    }
}

/// Applies `cpu` to packed F32 RGBA pixels, in place.
fn apply(cpu: &CpuProcessor, px: &mut [f32]) {
    for pixel in px.as_chunks_mut::<4>().0 {
        cpu.apply_rgba(pixel).unwrap();
    }
}

/// The linear value whose grading log is `t`: the inverse of the renderers' `LinLog`
/// (GradingHueCurveOpCPU.cpp:35-40 @ v2.5.2) in double, to put probes near the luma curves'
/// knots, which are in that log space.
fn lin_of_log(t: f32) -> f32 {
    let t = f64::from(t);
    let shift = -0.000157849851665374f64;
    if t > -5.5 {
        (t.exp2() * (0.18 + shift) - shift) as f32
    } else {
        ((t + 7.0) / 363.034608563) as f32
    }
}

/// The values next to `v`: `v` and 1 and 2 ulps either side.
fn around(v: f32) -> [f32; 5] {
    let b = v.to_bits();
    [
        v,
        f32::from_bits(b.wrapping_add(1)),
        f32::from_bits(b.wrapping_sub(1)),
        f32::from_bits(b.wrapping_add(2)),
        f32::from_bits(b.wrapping_sub(2)),
    ]
}

/// The number of pixels the knot probes lay out: [`ProbeSet::Values`] cycles its values with a
/// stride of 7919 between channels, so a buffer of 4 x 7919 values whose `c`-th block holds
/// channel `c` makes the first 7919 pixels exactly those laid out (and the rest their
/// rotations).
const KNOT_PIXELS: usize = 7919;

/// Pixels whose hue, saturation and luma sit on and next to the curves' knots: each
/// combination of a hue from the hue curves' x (forward) or y (inverse; the hue-fx curve's x +
/// y) coordinates, a saturation from the sat curves', and a luma from the lum curves' (for the
/// linear style through its log), converted to RGB by the style's HSY conversion (none when
/// the op doesn't convert), with alphas among the specials. Only inputs: the conversion is the
/// port's, and the outputs are the wheel's.
fn knot_pixels(p: &HcParams, direction: Direction) -> Vec<f32> {
    let forward = direction == Direction::Forward;
    let finite = |v: &f32| v.is_finite();
    let coord = |c: HueCurveType| -> Vec<f32> {
        p.points[c as usize]
            .iter()
            .map(|&(x, y)| {
                if forward {
                    x
                } else if c == HueCurveType::HueFx {
                    x + y
                } else {
                    y
                }
            })
            .filter(finite)
            .collect()
    };
    let mut hues: Vec<f32> = vec![0.0, 0.25, 0.5, 0.999, -0.1, 1.1];
    for c in [
        HueCurveType::HueHue,
        HueCurveType::HueSat,
        HueCurveType::HueLum,
        HueCurveType::HueFx,
    ] {
        for v in coord(c) {
            hues.extend(around(v));
            hues.extend(around(v - v.floor()));
        }
    }
    let mut sats: Vec<f32> = vec![0.0, 0.5, 1.0, 1.5];
    for c in [HueCurveType::SatSat, HueCurveType::SatLum] {
        for v in coord(c) {
            sats.extend(around(v));
        }
    }
    let lin = p.style == GradingStyle::Lin;
    let mut lumas: Vec<f32> = vec![0.18, 0.5, -0.05, 1.5];
    if lin {
        lumas.push(0.0041318374739483946);
    }
    for c in [HueCurveType::LumSat, HueCurveType::LumLum] {
        for v in coord(c) {
            for v in around(v) {
                lumas.push(if lin { lin_of_log(v) } else { v });
            }
        }
    }
    let alphas = [
        0.5,
        f32::from_bits(0x7fa0_0001),
        f32::from_bits(0xffc0_0042),
        f32::INFINITY,
        -0.0,
    ];
    let mut rng = Rng::new(0x48e5_9001);
    let mut hsy = Vec::with_capacity(4 * KNOT_PIXELS);
    for i in 0..KNOT_PIXELS {
        let pick = |rng: &mut Rng, v: &[f32]| v[(rng.next_u64() % v.len() as u64) as usize];
        hsy.extend([
            pick(&mut rng, &hues),
            pick(&mut rng, &sats),
            pick(&mut rng, &lumas),
            alphas[i % alphas.len()],
        ]);
    }
    if p.rgb_to_hsy == HsyTransformStyle::Transform1 {
        let style = match p.style {
            GradingStyle::Lin => FixedFunctionOpStyle::HsyLinToRgb,
            GradingStyle::Log => FixedFunctionOpStyle::HsyLogToRgb,
            GradingStyle::Video => FixedFunctionOpStyle::HsyVidToRgb,
        };
        let to_rgb: Arc<dyn CpuOp> =
            get_fixed_function_cpu_renderer(&FixedFunctionOpData::new(style).unwrap(), false)
                .unwrap();
        to_rgb.apply(&mut hsy);
    }
    // Channel c of pixel i at index c * KNOT_PIXELS + i.
    let mut values = vec![0.0f32; 4 * KNOT_PIXELS];
    for (i, px) in hsy.chunks(4).enumerate() {
        for c in 0..4 {
            values[c * KNOT_PIXELS + i] = px[c];
        }
    }
    values
}

/// NaNs of distinct payloads and both signs, quiet and signalling, and other specials, cycled
/// into every channel: a NaN pixel meets the default NaN of an infinite gain times 0.
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
        0.25,
        1.0,
    ]);
    values
}

struct HueCurveFamily;

impl Family for HueCurveFamily {
    type Params = HcParams;

    fn name(&self) -> String {
        "GradingHueCurveTransform".to_string()
    }

    fn cases(&self) -> Vec<Case<HcParams>> {
        explicit_cases()
    }

    /// Upstream's curves of all three styles, and with the conversion bypassed, and drawn:
    /// without the knot probes.
    fn mutation_bases(&self) -> Vec<Case<HcParams>> {
        explicit_cases()
            .into_iter()
            .filter(|c| {
                matches!(
                    c.label(),
                    "all curves, log"
                        | "all curves, lin"
                        | "all curves, video, no HSY"
                        | "upstream draw"
                )
            })
            .map(|c| {
                let mut p = c.params().clone();
                p.knot_probes = false;
                Case::new(c.label().to_string(), p)
            })
            .collect()
    }

    fn spec(&self, p: &HcParams, direction: Direction) -> Spec {
        Spec::Transform(p.transform(transform_direction(direction)))
    }

    fn port(&self, p: &HcParams, combo: &Combo) -> Result<Port, String> {
        let cpu = p.cpu(transform_direction(combo.direction), combo.fast_math)?;
        Ok(Port::in_place(move |px| apply(&cpu, px)))
    }

    /// Alpha, which the HSY conversions and the draw renderer never write; none when the
    /// identity matrix replaces the conversions (it scales alpha by 1), and none when the
    /// optimizer removes the op (an identity, not dynamic), where alpha is the CPU processor's
    /// to copy.
    fn pass_through(&self, p: &HcParams, combo: &Combo) -> Channels {
        match p.op_data(transform_direction(combo.direction)) {
            Ok(data)
                if !data.is_no_op()
                    && (p.draw || p.rgb_to_hsy == HsyTransformStyle::Transform1) =>
            {
                A
            }
            _ => [false; 4],
        }
    }

    fn extra_probes(&self, p: &HcParams, direction: Direction) -> Vec<ProbeSet> {
        let mut probes = vec![
            ProbeSet::NanBuffers { max_pixels: 8 },
            ProbeSet::Values {
                name: "distinct NaNs".to_string(),
                values: distinct_nans(),
            },
        ];
        if p.knot_probes {
            probes.push(ProbeSet::Values {
                name: "pixels on the knots".to_string(),
                values: knot_pixels(p, direction),
            });
        }
        probes
    }
}

/// The curves of upstream's tests (tests/cpu/ops/gradinghuecurve/GradingHueCurveOpCPU_tests.cpp
/// @ v2.5.2).
mod upstream {
    pub(super) type Pts = &'static [(f32, f32)];
    pub(super) const HH: Pts = &[
        (0.05, 0.15),
        (0.2, 0.3),
        (0.35, 0.4),
        (0.45, 0.45),
        (0.6, 0.7),
        (0.8, 0.85),
    ];
    pub(super) const HS: Pts = &[
        (-0.1, 1.2),
        (0.2, 0.7),
        (0.4, 1.5),
        (0.5, 0.5),
        (0.6, 1.4),
        (0.8, 0.7),
    ];
    pub(super) const HL: Pts = &[(0.1, 1.5), (0.2, 0.7), (0.4, 1.4), (0.5, 0.8), (0.8, 0.5)];
    pub(super) const LS: Pts = &[(0.05, 1.5), (0.5, 0.9), (1.1, 1.4)];
    pub(super) const SS: Pts = &[(0.0, 0.1), (0.5, 0.45), (1.0, 1.1)];
    pub(super) const LL: Pts = &[(-0.02, -0.04), (0.2, 0.1), (0.8, 0.95), (1.1, 1.2)];
    pub(super) const SL: Pts = &[(0.0, 1.2), (0.6, 0.8), (0.9, 1.1)];
    pub(super) const HFX: Pts = &[
        (0.2, 0.05),
        (0.4, -0.09),
        (0.6, -0.2),
        (0.8, 0.05),
        (0.99, -0.02),
    ];
    /// `lin_all_curves`'s lum-sat and lum-lum curves, in f-stops.
    pub(super) const LS_LIN: Pts = &[
        (-6.0, 0.9),
        (-3.0, 0.8),
        (0.0, 1.2),
        (2.0, 1.0),
        (4.0, 0.6),
        (6.0, 0.55),
    ];
    pub(super) const LL_LIN: Pts = &[(-8.0, -7.0), (-2.0, -3.0), (2.0, 3.5), (8.0, 7.0)];
}

/// Upstream's test curves, in each style, with and without the conversion, dynamic, drawn;
/// the defaults (identities, which the optimizer removes, drawn ones too: I-272); curves whose
/// gains overflow; hue-fx curves whose inverse wraps; slopes; NaN values.
fn explicit_cases() -> Vec<Case<HcParams>> {
    use GradingStyle::{Lin, Log, Video};
    use HueCurveType::*;
    use upstream::*;
    let all = |style| {
        let (ls, ll) = if style == Lin {
            (LS_LIN, LL_LIN)
        } else {
            (LS, LL)
        };
        HcParams::new(style, [HH, HS, HL, ls, SS, ll, SL, HFX])
    };
    let mut cases = Vec::new();
    for style in STYLES {
        let name = match style {
            Log => "log",
            Lin => "lin",
            Video => "video",
        };
        cases.push(Case::new(format!("all curves, {name}"), all(style)));
        cases.push(Case::new(
            format!("all curves, {name}, no HSY"),
            all(style).no_hsy(),
        ));
        cases.push(Case::new(
            format!("all curves, {name}, dynamic"),
            all(style).dynamic(),
        ));
        cases.push(Case::new(
            format!("defaults, {name}"),
            HcParams::defaults(style),
        ));
        cases.push(Case::new(
            format!("defaults, {name}, drawn"),
            HcParams::defaults(style).draw(),
        ));
        cases.push(Case::new(
            format!("defaults, {name}, dynamic"),
            HcParams::defaults(style).dynamic(),
        ));
        // Gains that overflow: an infinite hue-lum gain times a saturation of 0, an infinite
        // hue-sat gain times a lum-sat gain of 0, and an infinite sat-lum gain: x86's default
        // NaN meets the pixels' NaNs.
        cases.push(Case::new(
            format!("infinite gains, {name}"),
            all(style)
                .with_curve(HueLum, &[(0.1, 1.0), (0.3, 3e38), (0.6, 3e38), (0.9, 1.0)])
                .with_curve(HueSat, &[(0.0, 3e38), (0.5, 3e38)])
                .with_curve(LumSat, &[(-1.0, 0.0), (8.0, 0.0)])
                .with_curve(SatLum, &[(0.0, 3e38), (2.0, 3e38)]),
        ));
        cases.push(Case::new(
            format!("infinite gains, {name}, no HSY"),
            all(style)
                .no_hsy()
                .with_curve(HueLum, &[(0.0, 3e38), (0.5, 3e38)])
                .with_curve(HueSat, &[(0.0, 3e38), (0.5, 0.0)])
                .with_curve(SatLum, &[(0.0, 1.0), (1.0, 3e38)]),
        ));
        cases.push(Case::new(
            format!("infinite curves, {name}"),
            all(style)
                .with_curve(HueLum, &[(0.1, 1.0), (0.3, f32::INFINITY), (0.6, 1.0)])
                .with_curve(HueSat, &[(0.0, f32::INFINITY), (0.5, 1.0)])
                .with_curve(LumSat, &[(-1.0, 0.0), (8.0, 0.0)]),
        ));
        cases.push(Case::new(format!("infinite slopes, {name}"), {
            let p = all(style);
            let hl = p.points[HueLum as usize].len();
            let ls = p.points[LumSat as usize].len();
            let mut hl_slopes = vec![0.0; hl];
            hl_slopes[0] = f32::INFINITY;
            hl_slopes[hl - 1] = f32::INFINITY;
            let mut ls_slopes = vec![0.0; ls];
            ls_slopes[1] = f32::INFINITY;
            p.with_slopes(HueLum, &hl_slopes)
                .with_slopes(LumSat, &ls_slopes)
        }));
        cases.push(Case::new(
            format!("NaN values, {name}"),
            all(style)
                .with_curve(HueSat, &[(0.0, f32::NAN), (0.5, 1.0)])
                .with_slopes(SatLum, &[f32::NAN, 0.0, 0.0]),
        ));
    }
    // Upstream's other tests.
    cases.push(Case::new(
        "upstream log identity",
        HcParams::new(
            Log,
            [
                &[
                    (0.0, 0.0),
                    (0.1, 0.1),
                    (0.2, 0.2),
                    (0.4, 0.4),
                    (0.6, 0.6),
                    (0.8, 0.8),
                ],
                &[
                    (0.0, 1.0),
                    (0.1, 1.0),
                    (0.2, 1.0),
                    (0.4, 1.0),
                    (0.6, 1.0),
                    (0.8, 1.0),
                ],
                &[
                    (0.0, 1.0),
                    (0.1, 1.0),
                    (0.2, 1.0),
                    (0.4, 1.0),
                    (0.6, 1.0),
                    (0.8, 1.0),
                ],
                &[(0.0, 1.0), (1.0, 1.0)],
                &[(0.0, 0.0), (0.25, 0.25), (1.0, 1.0)],
                &[(0.0, 0.0), (0.25, 0.25), (0.5, 0.5), (1.0, 1.0)],
                &[(0.0, 1.0), (0.25, 1.0), (0.5, 1.0), (1.0, 1.0)],
                &[
                    (0.0, 0.0),
                    (0.1, 0.0),
                    (0.2, 0.0),
                    (0.4, 0.0),
                    (0.6, 0.0),
                    (0.8, 0.0),
                ],
            ],
        ),
    ));
    let identity2: [&[(f32, f32)]; 6] = [
        &[(0.0, 1.0), (0.9, 1.0)],
        &[(0.0, 1.0), (0.9, 1.0)],
        &[(0.0, 1.0), (0.9, 1.0)],
        &[(0.0, 0.0), (0.9, 0.9)],
        &[(0.0, 0.0), (0.9, 0.9)],
        &[(0.0, 1.0), (0.9, 1.0)],
    ];
    let [hs, hl, ls, ss, ll, sl] = identity2;
    let hh_hfx = HcParams::new(Log, [HH, hs, hl, ls, ss, ll, sl, HFX]);
    cases.push(Case::new("upstream hh hfx", hh_hfx.clone()));
    cases.push(Case::new(
        "upstream hh hfx, video",
        hh_hfx.clone().style(Video),
    ));
    let mut drawn = HcParams::defaults(Log).draw();
    drawn.points[HueSat as usize][1] = (0.15, 1.4);
    cases.push(Case::new("upstream draw", drawn.clone()));
    cases.push(Case::new(
        "upstream draw, lin, no HSY",
        drawn.style(Lin).no_hsy(),
    ));
    let mut bypass = HcParams::defaults(Log).no_hsy();
    bypass.points[SatSat as usize][1] = (0.4, 0.8);
    cases.push(Case::new("upstream bypass", bypass));
    // Hue-fx curves whose inverse wraps: outputs far below 0 and above 1.
    cases.push(Case::new(
        "wide hue-fx",
        hh_hfx.clone().with_curve(
            HueFx,
            &[
                (0.1, 0.6),
                (0.3, -0.7),
                (0.5, 1.4),
                (0.7, -1.6),
                (0.95, 0.2),
            ],
        ),
    ));
    cases.push(Case::new(
        "wide hue-fx, lin",
        hh_hfx
            .clone()
            .style(Lin)
            .with_curve(HueFx, &[(0.0, -0.45), (0.5, 0.45), (0.75, -0.3)]),
    ));
    // Hue curves out of [0, 1], slopes.
    cases.push(Case::new(
        "hue curves outside [0, 1], slopes",
        all(Log)
            .with_curve(HueSat, &[(-0.5, 0.2), (0.25, 1.8), (1.4, 0.6)])
            .with_slopes(HueSat, &[0.5, -2.0, 1.0])
            .with_slopes(SatSat, &[0.3, 1.0, 2.0])
            .with_slopes(LumLum, &[2.0, 0.5, 0.5, 2.0]),
    ));
    // Hue-fx segments of slope -1 + 4e-6: the diagonal added, their slope is 4e-6, between
    // the 1e-5 of the inverse's small-denominator test and 0 (GradingBSplineCurve.cpp:1451 @
    // v2.5.2).
    let near = -1.0f32 + 4e-6;
    let fx: Vec<(f32, f32)> = [0.1f32, 0.3, 0.5, 0.7]
        .iter()
        .map(|&x| (x, 0.3 + (x - 0.1) * near))
        .collect();
    for style in [Log, Lin] {
        cases.push(Case::new(
            format!("hue-fx slope near -1, {style:?}"),
            hh_hfx
                .clone()
                .style(style)
                .with_curve(HueFx, &fx)
                .with_slopes(HueFx, &[near; 4]),
        ));
    }
    // Flat (horizontal) segments, which the inverses' small-slope branches take.
    cases.push(Case::new(
        "flat segments",
        all(Video)
            .with_curve(SatSat, &[(0.0, 0.0), (0.3, 0.3), (0.6, 0.3), (1.0, 1.0)])
            .with_curve(LumLum, &[(0.0, 0.0), (0.4, 0.4), (0.7, 0.4), (1.0, 1.0)])
            .with_curve(HueFx, &[(0.0, 0.0), (0.4, -0.4), (0.6, -0.6), (0.9, 0.0)]),
    ));
    extreme_cases(&mut cases);
    cases
}

/// The six spline types, in the enum's order.
const SPLINES: [BSplineType; 6] = [
    BSplineType::BSpline,
    BSplineType::DiagonalBSpline,
    BSplineType::HueHueBSpline,
    BSplineType::Periodic1BSpline,
    BSplineType::Periodic0BSpline,
    BSplineType::Horizontal1BSpline,
];

/// A hue-sat curve of `n` points, the knot limit's: x at i / n, y between 0.5 and 1.4.
fn many_points(n: usize) -> Vec<(f32, f32)> {
    (0..n)
        .map(|i| (i as f32 / n as f32, 0.5 + ((i * 5) % 7) as f32 * 0.15))
        .collect()
}

/// Explicit cases for the corners of the curves' values, in each style: signed zeros, gains of
/// 0, the largest finite and infinite values, a drawn curve of each spline type, and curves at
/// the knot limit.
fn extreme_cases(cases: &mut Vec<Case<HcParams>>) {
    use GradingStyle::{Lin, Log, Video};
    use HueCurveType::*;
    use upstream::*;
    let all = |style| {
        let (ls, ll) = if style == Lin {
            (LS_LIN, LL_LIN)
        } else {
            (LS, LL)
        };
        HcParams::new(style, [HH, HS, HL, ls, SS, ll, SL, HFX])
    };
    let (inf, ninf) = (f32::INFINITY, f32::NEG_INFINITY);
    for style in STYLES {
        let name = match style {
            Log => "log",
            Lin => "lin",
            Video => "video",
        };
        // Signed zeros: points at -0.0 (the hue-hue, hue-fx, lum-lum and sat-sat curves), and
        // slopes of -0.0.
        let zeros = all(style)
            .with_curve(HueHue, &[(-0.0, -0.0), (0.3, 0.35), (0.7, 0.65)])
            .with_curve(HueFx, &[(-0.0, -0.0), (0.4, -0.1), (0.8, 0.05)])
            .with_curve(
                LumLum,
                &[(-0.5, -0.5), (-0.0, -0.0), (0.5, 0.6), (1.0, 1.0)],
            )
            .with_curve(SatSat, &[(-0.0, -0.0), (0.5, 0.45), (1.0, 1.0)]);
        cases.push(Case::new(
            format!("signed zero points, {name}"),
            zeros.clone(),
        ));
        cases.push(Case::new(
            format!("signed zero points, {name}, no HSY"),
            zeros.clone().no_hsy(),
        ));
        let mut slopes = zeros.clone();
        for c in 0..8 {
            slopes.slopes[c] = vec![-0.0; slopes.points[c].len()];
        }
        cases.push(Case::new(format!("signed zero slopes, {name}"), slopes));
        // All four gain curves at 0.
        let gains = all(style)
            .with_curve(HueSat, &[(0.0, 0.0), (0.5, 0.0)])
            .with_curve(HueLum, &[(0.1, 0.0), (0.6, 0.0)])
            .with_curve(LumSat, &[(-8.0, 0.0), (8.0, 0.0)])
            .with_curve(SatLum, &[(0.0, 0.0), (2.0, 0.0)]);
        cases.push(Case::new(format!("zero gains, {name}"), gains.clone()));
        cases.push(Case::new(
            format!("zero gains, {name}, no HSY"),
            gains.no_hsy(),
        ));
        // The hue-fx curve at +-3e38, then +inf; the lum-lum curve at -inf, which validation
        // refuses (every y must be at least the one before, the first at least -f32::MAX): the
        // battery compares the wheel's exception with the port's, byte for byte.
        cases.push(Case::new(
            format!("huge hue-fx, {name}"),
            all(style).with_curve(HueFx, &[(0.1, 3e38), (0.4, -3e38), (0.7, inf)]),
        ));
        cases.push(Case::new(
            format!("-inf lum-lum, {name}"),
            all(style).with_curve(LumLum, &[(0.0, 0.0), (0.4, ninf), (0.8, 0.95), (1.1, 1.2)]),
        ));
        // Slopes of +-3e38 and +-inf, alternating in sign along each curve.
        for (label, big) in [("huge", 3e38f32), ("infinite", inf)] {
            let mut p = all(style);
            for c in [HueHue, HueSat, SatSat, LumLum, HueFx, SatLum] {
                let n = p.points[c as usize].len();
                p.slopes[c as usize] = (0..n)
                    .map(|i| if i % 2 == 0 { big } else { -big })
                    .collect();
            }
            cases.push(Case::new(format!("{label} slopes, {name}"), p));
        }
        // The hue-lum curve at 0 and 3e38 on the way to f32::MAX, with and without the HSY
        // conversion.
        let hue_lum = all(style).with_curve(HueLum, &[(0.0, 3e38), (0.5, f32::MAX)]);
        cases.push(Case::new(format!("max hue-lum, {name}"), hue_lum.clone()));
        cases.push(Case::new(
            format!("max hue-lum, {name}, no HSY"),
            hue_lum.no_hsy(),
        ));
        // Hue-sat curves at the knot limit: 26 points make 118 knots, 27 exactly 120.
        for n in [26, 27] {
            cases.push(Case::new(
                format!("{n} hue-sat points, {name}"),
                HcParams::defaults(style).with_curve(HueSat, &many_points(n)),
            ));
        }
    }
    // Draw-curve-only ops with each spline type in the hue-sat slot (a draw-curve-only op
    // doesn't check the types, but the curve's own validation depends on it: the B_SPLINE,
    // DIAGONAL and HUE_HUE types need non-decreasing y), identity or shaped (non-decreasing,
    // with a flat step), or, for the types that allow it, wavy; fixed and dynamic.
    for spline in SPLINES {
        let monotonic = matches!(
            spline,
            BSplineType::BSpline | BSplineType::DiagonalBSpline | BSplineType::HueHueBSpline
        );
        let shapes: &[&str] = if monotonic {
            &["identity", "shaped"]
        } else {
            &["identity", "shaped", "wavy"]
        };
        for &shape in shapes {
            for dynamic in [false, true] {
                let mut p = HcParams::defaults(Log).draw().with_spline(HueSat, spline);
                match shape {
                    "shaped" => {
                        for (point, y) in p.points[HueSat as usize]
                            .iter_mut()
                            .zip([1.0, 1.0, 1.2, 1.2, 1.5, 1.7])
                        {
                            point.1 = y;
                        }
                    }
                    "wavy" => p.points[HueSat as usize][1] = (0.15, 1.4),
                    _ => {}
                }
                if dynamic {
                    p = p.dynamic();
                }
                cases.push(Case::new(
                    format!(
                        "draw {}, {shape}, {}",
                        spline_name(spline),
                        if dynamic { "dynamic" } else { "fixed" }
                    ),
                    p,
                ));
            }
        }
    }
}

/// The GradingHueCurve op's renderers against the wheel's GradingHueCurveTransform: every case
/// in both directions with fast math on and off (the renderers ignore it), the generated
/// extreme, NaN and infinite coordinates and slopes, on the tier's probes, the knot probes and
/// the NaN probes.
#[test]
fn renderers_match_the_wheel() {
    battery::run(&HueCurveFamily);
}

/// The corner cases are not vacuous: the battery lets a case pass when the wheel and the port
/// both refuse it with the same text, so the port must accept each but the -inf lum-lum (and
/// where it does, the battery fails unless the wheel does too, with the same pixels), and
/// refuse that one, with the text the battery then compares with the wheel's.
#[test]
fn the_extreme_cases_are_accepted() {
    let mut cases = Vec::new();
    extreme_cases(&mut cases);
    // Per style: signed zero points (with and without the conversion) and slopes, zero gains
    // (both ways), the huge hue-fx, the -inf lum-lum, huge and infinite slopes, the maximal
    // hue-lum (both ways) and the two knot-limit curves; and the drawn hue-sat curves: 3
    // spline types identity or shaped, 3 also wavy, fixed or dynamic.
    assert_eq!(cases.len(), STYLES.len() * 13 + (3 * 2 + 3 * 3) * 2);
    let mut labels: Vec<&str> = cases.iter().map(|c| c.label()).collect();
    labels.sort_unstable();
    labels.dedup();
    assert_eq!(labels.len(), cases.len());
    for case in &cases {
        for direction in [TransformDirection::Forward, TransformDirection::Inverse] {
            let result = case.params().op_data(direction);
            if case.label().starts_with("-inf lum-lum") {
                // The text is the battery's to compare with the wheel's.
                assert!(result.is_err(), "{} {direction:?}", case.label());
            } else if let Err(e) = result {
                panic!("{} {direction:?}: {e}", case.label());
            }
        }
    }
}

/// Pixels for the dynamic tests: some on the knots, the specials' NaNs, infinities.
fn pixels() -> Vec<f32> {
    let mut px = vec![
        0.0367126, 0.5, 1.0, 0.25, 0.2, 0.0, 0.99, 128.0, -0.25, 2.0, 1e-3, 0.5, 0.18, 0.75, -1.0,
        1.0, 0.3, 0.6, 0.9, 0.0, 0.4, 0.1, -0.2, 0.5, 8.0, 0.02, 0.7, 1.0,
    ];
    px.extend([
        f32::from_bits(0x7fa0_0000),
        0.3,
        0.2,
        f32::from_bits(0xffc0_1234),
        0.2,
        f32::NAN,
        0.4,
        0.5,
        0.1,
        0.6,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::from_bits(0xffc0_0001),
        f32::from_bits(0x7fc0_0777),
        f32::from_bits(0xff80_0003),
        0.25,
    ]);
    px
}

/// The values set between applies: upstream's curves, the defaults (the bypass), drawn ones,
/// overflowing and NaN ones, one the setter refuses.
fn sets(style: GradingStyle) -> Vec<GradingHueCurve> {
    let cases = explicit_cases();
    let get = |label: String| {
        cases
            .iter()
            .find(|c| c.label() == label)
            .unwrap_or_else(|| panic!("no case {label}"))
            .params()
            .curves()
    };
    let name = match style {
        GradingStyle::Log => "log",
        GradingStyle::Lin => "lin",
        GradingStyle::Video => "video",
    };
    let mut refused = get(format!("all curves, {name}"));
    // The x coordinates decrease: validate refuses it, the property keeps the last value.
    refused
        .curve_mut(HueCurveType::SatSat)
        .unwrap()
        .control_point_mut(1)
        .unwrap()
        .x = -1.0;
    vec![
        get(format!("all curves, {name}")),
        GradingHueCurve::new(style),
        get(format!("infinite gains, {name}")),
        get(format!("NaN values, {name}")),
        get("upstream hh hfx".to_string()),
        refused,
        get(format!("defaults, {name}, drawn")),
        get(format!("all curves, {name}")),
    ]
}

/// What `check_dynamic_sets` compared.
#[derive(Default)]
struct DynamicStats {
    /// The applies compared with the wheel's.
    compared: usize,
    /// The sets both refused.
    refused: usize,
    /// The applies whose wheel output differs from the one before.
    changed: usize,
}

/// A dynamic op of `params`, in `direction`: the CPU processor's property set as `sets` lists,
/// an apply after each set, against the wheel's `dynamic_apply` with the same steps, bit for
/// bit (W0002 doesn't apply: these are inputs, not the battery's NaN parameters). A set the
/// wheel refuses is refused by the port with the same text. Returns the wheel's outputs, one
/// per apply (the first before any set), as bits.
fn check_dynamic_sets(
    params: &HcParams,
    direction: TransformDirection,
    sets: &[GradingHueCurve],
    input: &[f32],
    failures: &mut Vec<String>,
    stats: &mut DynamicStats,
) -> Vec<Vec<u32>> {
    let bytes = f32_to_bytes(input);
    let width = input.len() / 4;
    let mut steps = vec![
        json!({"get": "DYNAMIC_PROPERTY_GRADING_HUECURVE", "from": "cpu", "as": "p"}),
        json!({"apply": [0, 1]}),
    ];
    for value in sets {
        steps.push(json!({"set": "p", "value": hue_curve_spec(value)}));
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

    // The port: the CPU processor of a forward processor of the op the wheel builds, its
    // property set the same way.
    let cpu = params.cpu(direction, true).unwrap();
    assert!(cpu.has_dynamic_property(DynamicPropertyType::GradingHueCurve));
    assert_eq!(wheel_steps[0]["has"], json!(true), "{result}");
    let property = cpu
        .get_dynamic_property(DynamicPropertyType::GradingHueCurve)
        .unwrap()
        .as_grading_hue_curve()
        .unwrap();
    let mut applies: Vec<Option<&GradingHueCurve>> = vec![None];
    applies.extend(sets.iter().map(Some));
    let mut outputs: Vec<Vec<u32>> = Vec::new();
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
                    stats.refused += 1;
                }
            }
        }
        let mut port = input.to_vec();
        apply(&cpu, &mut port);
        let step = &wheel_steps[2 * k + 1];
        let index = step["result"]["buffers"][1]
            .as_u64()
            .unwrap_or_else(|| panic!("{step}")) as usize;
        let wheel = bytes_to_f32(&response.blobs[index]);
        assert_eq!(wheel.len(), port.len());
        stats.compared += 1;
        let wheel_bits: Vec<u32> = wheel.iter().map(|v| v.to_bits()).collect();
        if outputs.last().is_some_and(|p| *p != wheel_bits) {
            stats.changed += 1;
        }
        outputs.push(wheel_bits);
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
                "{:?} {direction:?} after set {k}: {}",
                params.style,
                differ.join(", ")
            ));
        }
    }
    outputs
}

/// Each style in each direction, dynamic: the CPU processor's property set as `sets()` lists,
/// an apply after each set, against the wheel's `dynamic_apply` with the same steps.
#[test]
fn dynamic_sets_apply_as_the_wheels() {
    let input = pixels();
    let mut failures = Vec::new();
    let mut stats = DynamicStats::default();
    for style in STYLES {
        for direction in [TransformDirection::Forward, TransformDirection::Inverse] {
            let params = HcParams::defaults(style).dynamic();
            check_dynamic_sets(
                &params,
                direction,
                &sets(style),
                &input,
                &mut failures,
                &mut stats,
            );
        }
    }
    assert!(
        failures.is_empty(),
        "{}",
        failures.join(
            "
"
        )
    );
    assert_eq!(
        stats.compared,
        STYLES.len() * 2 * (1 + sets(GradingStyle::Log).len())
    );
    assert_eq!(stats.refused, STYLES.len() * 2);
    // Most sets change the wheel's output (not the refused one).
    assert!(
        stats.changed >= STYLES.len() * 2 * 5,
        "{} sets changed the output",
        stats.changed
    );
}

/// A dynamic op built in draw-curve-only mode, its property then set to curves that don't draw
/// (and back): the renderer is chosen when the op is built, so upstream's stays the draw
/// renderer (`GetGradingHueCurveCPURenderer`, GradingHueCurveOpCPU.cpp:365-389 @ v2.5.2), and
/// the port's must too, bit for bit with the wheel's at every step.
#[test]
fn a_dynamic_draw_op_set_to_normal_curves_applies_as_the_wheels() {
    let input = pixels();
    let cases = explicit_cases();
    let get = |label: String| {
        cases
            .iter()
            .find(|c| c.label() == label)
            .unwrap_or_else(|| panic!("no case {label}"))
            .params()
            .clone()
    };
    let mut failures = Vec::new();
    let mut stats = DynamicStats::default();
    for style in STYLES {
        let name = match style {
            GradingStyle::Log => "log",
            GradingStyle::Lin => "lin",
            GradingStyle::Video => "video",
        };
        for direction in [TransformDirection::Forward, TransformDirection::Inverse] {
            let normal = get(format!("all curves, {name}"));
            assert!(!normal.draw);
            let params = normal.clone().draw().dynamic();
            let sets = vec![
                normal.curves(),
                get("upstream hh hfx".to_string()).curves(),
                get(format!("defaults, {name}, drawn")).curves(),
                normal.curves(),
            ];
            let outputs =
                check_dynamic_sets(&params, direction, &sets, &input, &mut failures, &mut stats);
            // The sets reach the draw renderer's curve: the wheel's outputs differ between the
            // first curves and the drawn defaults.
            assert_ne!(outputs[0], outputs[3], "{name} {direction:?}");
        }
    }
    assert!(
        failures.is_empty(),
        "{}",
        failures.join(
            "
"
        )
    );
    assert_eq!(stats.compared, STYLES.len() * 2 * 5);
    assert_eq!(stats.refused, 0);
}

/// The renderer's property is a copy of the op data's: a set on the op data's own property
/// after the renderer is built leaves the renderer's output as it was, a set on the renderer's
/// property changes it. The wheel (`GradingHueCurveOpCPU::GradingHueCurveOpCPU`, which
/// decouples a dynamic property with `createEditableCopy`, GradingHueCurveOpCPU.cpp:78-85 @
/// v2.5.2) gives the output at the curves the op was built with, at another set on the CPU
/// processor's property, and at a third; the port's renderer must give the first after a set of
/// its op data's property, and the second after the same set of its own.
///
/// The port's step is a unit-level test: the CPU processor clones its ops, so a set on the
/// original property is out of the pipeline's reach, and the test builds the renderer of an op
/// data directly (`get_grading_hue_curve_cpu_renderer`).
#[test]
fn renderer_property_is_decoupled_from_the_op() {
    let input = pixels();
    let bytes = f32_to_bytes(&input);
    let width = input.len() / 4;
    let mut compared = 0;
    let cases = explicit_cases();
    let curves_of = |label: &str| {
        cases
            .iter()
            .find(|c| c.label() == label)
            .unwrap_or_else(|| panic!("no case {label}"))
            .params()
            .clone()
    };
    for (built, on_op, on_renderer) in [
        ("all curves, log", "upstream hh hfx", "infinite gains, log"),
        (
            "all curves, lin",
            "wide hue-fx, lin",
            "infinite slopes, lin",
        ),
        ("all curves, video", "flat segments", "upstream draw"),
    ] {
        for direction in [TransformDirection::Forward, TransformDirection::Inverse] {
            let params = curves_of(built).dynamic();
            let on_op = curves_of(on_op).curves();
            let on_renderer = curves_of(on_renderer).curves();
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
                        {"get": "DYNAMIC_PROPERTY_GRADING_HUECURVE", "from": "cpu", "as": "p"},
                        {"apply": [0, 1]},
                        {"set": "p", "value": hue_curve_spec(&on_op)},
                        {"apply": [0, 1]},
                        {"set": "p", "value": hue_curve_spec(&on_renderer)},
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
            assert_ne!(at_built, at_op_set, "{built} {direction:?}");
            assert_ne!(at_built, at_renderer_set, "{built} {direction:?}");

            let data = params.op_data(direction).unwrap();
            let renderer = get_grading_hue_curve_cpu_renderer(&data).unwrap();
            let render = || -> Vec<u32> {
                let mut px = input.clone();
                renderer.apply(&mut px);
                px.iter().map(|v| v.to_bits()).collect()
            };
            assert_eq!(render(), at_built, "{built} {direction:?}: built");

            // A set on the op data's property: the renderer keeps its curves.
            data.get_dynamic_property_internal()
                .set_value(&on_op)
                .unwrap();
            assert_eq!(
                render(),
                at_built,
                "{built} {direction:?}: the renderer follows the op's property"
            );

            // A set on the renderer's property.
            renderer
                .get_dynamic_property(DynamicPropertyType::GradingHueCurve)
                .unwrap()
                .as_grading_hue_curve()
                .unwrap()
                .set_value(&on_renderer)
                .unwrap();
            assert_eq!(
                render(),
                at_renderer_set,
                "{built} {direction:?}: after the set"
            );
            compared += 1;
        }
    }
    assert_eq!(compared, 6);
}
