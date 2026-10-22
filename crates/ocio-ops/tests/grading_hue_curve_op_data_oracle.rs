// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `GradingHueCurveOpData` against the wheel, through processors of a
//! `GradingHueCurveTransform` (`oracle processor_ops`):
//! - the cache ID: the processor holds one GradingHueCurve op, so its cache ID is the hash of
//!   its ops' cache ID, `" <GradingHueCurveOp " + data cache ID + ">"` (Processor.cpp:331-348,
//!   Op.cpp:448-465, GradingHueCurveOp.cpp:123-132 @ v2.5.2); the data's text (the style, the
//!   direction, `bypassRGBToHSY`, the curves unless dynamic, 7 significant digits) is compared
//!   through its hash;
//! - `isNoOp` (`isIdentity`), `hasChannelCrosstalk` and `isDynamic` of the processor, which
//!   are its op's;
//! - the op data the processor built, in either direction: `createGroupTransform()` copies it
//!   into a transform (`CreateGradingHueCurveTransform`, GradingHueCurveOp.cpp:219-232), whose
//!   getters give the style, the direction, the RGB-to-HSY setting, the dynamic flag and the
//!   curves (each curve's control points, slopes and spline type, and the draw-curve-only
//!   flag, by their bits);
//! - the identity replacement: GradingHueCurveOpData doesn't override
//!   `getIdentityReplacement`, so the optimized processor (`OPTIMIZATION_DEFAULT`) removes an
//!   op that is an identity (a draw-curve-only op of identity curves too, I-272), and a pair of
//!   an op and its inverse (`RemoveInverseOps`, OpOptimizers.cpp:207-300), whose RGB-to-HSY
//!   settings it compares in the linear style only (I-271);
//! - the curves the transform refuses: the binding's constructor sets them, and `setValue`
//!   validates and fits them.
//!
//! The binding's constructor creates the transform for the style, sets the curves, makes it
//! dynamic, then sets the direction (src/bindings/python/transforms/
//! PyGradingHueCurveTransform.cpp:17-34), and the spec then calls `setRGBToHSY`; the processor
//! builds its op from a copy of the transform's data, inverted for an inverse processor
//! (GradingHueCurveOp.cpp:204-215, 234-245).

use ocio_ops::hash_utils::cache_id_hash;
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{
    BSplineType, GradingStyle, HsyTransformStyle, HueCurveType, TransformDirection,
};
use ocio_ops::ops::gradinghuecurve::grading_hue_curve::GradingHueCurve;
use ocio_ops::ops::gradinghuecurve::grading_hue_curve_op::create_grading_hue_curve_op;
use ocio_ops::ops::gradinghuecurve::grading_hue_curve_op_data::GradingHueCurveOpData;
use ocio_ops::ops::gradingrgbcurve::grading_b_spline_curve::{
    GradingBSplineCurve, GradingControlPoint,
};
use ocio_testkit::probe::Rng;
use ocio_testkit::processor_ops::{Dump, Dumped, ProcessorOpsReply, ProcessorOpsRequest};
use ocio_testkit::transform_text::f64_spec;
use serde_json::{Value, json};

const STYLES: [GradingStyle; 3] = [GradingStyle::Log, GradingStyle::Lin, GradingStyle::Video];
const DIRS: [TransformDirection; 2] = [TransformDirection::Forward, TransformDirection::Inverse];
const HSY: [HsyTransformStyle; 2] = [HsyTransformStyle::Transform1, HsyTransformStyle::None];

/// Floats the cache ID prints: values that need all 7 significant digits or round in the 7th,
/// the extremes of `float`, signed zeros and the non-finite ones.
const VALUES: [f32; 16] = [
    0.0,
    -0.0,
    0.1,
    0.18,
    1234567.8,
    0.99999995,
    0.000123456789,
    123.456789,
    1.2345678e20,
    1e-7,
    f32::MAX,
    f32::MIN_POSITIVE,
    1e-45,
    f32::NAN,
    f32::INFINITY,
    f32::NEG_INFINITY,
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

/// A float as a spec value, by its bits.
fn f32_spec(v: f32) -> Value {
    f64_spec(f64::from(v))
}

/// A curve's object spec: the binding's constructor from its control points, then its slopes
/// (when any isn't +0) and its spline type.
fn curve_spec(curve: &GradingBSplineCurve) -> Value {
    let values: Vec<Value> = curve
        .control_points()
        .iter()
        .flat_map(|p| [f32_spec(p.x), f32_spec(p.y)])
        .collect();
    let mut calls = Vec::new();
    if curve.slopes().iter().any(|s| s.to_bits() != 0) {
        let slopes: Vec<Value> = curve.slopes().iter().map(|&s| f32_spec(s)).collect();
        calls.push(json!(["setSlopes", slopes]));
    }
    calls.push(json!(["setSplineType", {"enum": spline_name(curve.spline_type())}]));
    json!({"object": {"class": "GradingBSplineCurve", "args": [values], "calls": calls}})
}

/// The eight curves' object spec: the binding's constructor of a style's defaults, then each
/// curve copied in through its property (`CopyGradingBSpline`, src/bindings/python/
/// PyGradingData.cpp:22-33 @ v2.5.2, which validates nothing), then `setDrawCurveOnly`. The
/// transform's `setValue` then validates the curves with the flag (the eight-curve constructor
/// would validate them without it).
fn hue_curve_spec(curves: &GradingHueCurve) -> Value {
    let names = [
        "hue_hue", "hue_sat", "hue_lum", "lum_sat", "sat_sat", "lum_lum", "sat_lum", "hue_fx",
    ];
    let attrs: Vec<Value> = names
        .iter()
        .zip(curves.curves())
        .map(|(name, curve)| json!([name, curve_spec(curve)]))
        .collect();
    json!({"object": {"class": "GradingHueCurve", "args": [{"enum": "GRADING_LOG"}],
                      "attrs": attrs,
                      "calls": [["setDrawCurveOnly", curves.get_draw_curve_only()]]}})
}

/// One processor: the transform's parameters and the processor's direction.
#[derive(Debug, Clone)]
struct HcCase {
    style: GradingStyle,
    curves: GradingHueCurve,
    dynamic: bool,
    rgb_to_hsy: HsyTransformStyle,
    transform_dir: TransformDirection,
    processor_dir: TransformDirection,
}

impl HcCase {
    fn transform(&self, dir: TransformDirection) -> Value {
        json!({"class": "GradingHueCurveTransform",
            "args": {
                "values": hue_curve_spec(&self.curves),
                "style": {"enum": style_name(self.style)},
                "dynamic": self.dynamic,
                "dir": {"enum": dir_name(dir)},
            },
            "calls": [["setRGBToHSY", {"enum": hsy_name(self.rgb_to_hsy)}]],
        })
    }

    fn processor(&self) -> Value {
        json!({
            "transform": self.transform(self.transform_dir),
            "direction": dir_name(self.processor_dir),
        })
    }

    /// The op data the processor holds, built as the wheel builds it, or the exception of
    /// the curves the transform refuses.
    fn port(&self) -> Result<GradingHueCurveOpData, String> {
        // GradingHueCurveTransform::Create(style), setStyle, setValue, makeDynamic,
        // setDirection, validate; then setRGBToHSY.
        let mut data = GradingHueCurveOpData::new(self.style);
        data.set_style(self.style);
        data.set_value(&self.curves).map_err(|e| e.to_string())?;
        if self.dynamic {
            data.get_dynamic_property_internal().make_dynamic();
        }
        data.set_direction(self.transform_dir);
        data.validate().map_err(|e| {
            format!(
                "GradingHueCurveTransform validation failed: {}",
                e.message()
            )
        })?;
        data.set_rgb_to_hsy(self.rgb_to_hsy);
        // BuildGradingHueCurveOp: validate, a clone, inverted for an inverse processor.
        data.validate().map_err(|e| e.to_string())?;
        let built = data.clone();
        Ok(match self.processor_dir {
            TransformDirection::Forward => built,
            TransformDirection::Inverse => built.inverse(),
        })
    }
}

/// A float in [lo, hi).
fn uniform(rng: &mut Rng, lo: f32, hi: f32) -> f32 {
    lo + (rng.next_u64() >> 40) as f32 / (1u64 << 24) as f32 * (hi - lo)
}

/// A curve of type `c`: 2 to 6 points, increasing x (in [0, 1] for hue-hue), increasing y for
/// the diagonal types, sometimes slopes, and now and then a value of [`VALUES`] in place of a
/// coordinate or slope (which the curve may then refuse), or the wrong spline type.
fn random_curve(rng: &mut Rng, c: HueCurveType, k: &mut usize) -> GradingBSplineCurve {
    let diagonal = matches!(
        c,
        HueCurveType::HueHue | HueCurveType::SatSat | HueCurveType::LumLum
    );
    let n = 2 + (rng.next_u64() % 5) as usize;
    let mut x = uniform(rng, 0.0, 0.15);
    let mut y = x;
    let mut points: Vec<GradingControlPoint> = (0..n)
        .map(|i| {
            if i > 0 {
                x += uniform(rng, 0.02, 0.18);
                y += uniform(rng, 0.0, 0.2);
            }
            let v = if diagonal { y } else { uniform(rng, 0.0, 2.0) };
            GradingControlPoint::new(x, v)
        })
        .collect();
    let mut slopes = vec![0.0f32; n];
    if rng.next_u64().is_multiple_of(4) {
        for s in &mut slopes {
            *s = uniform(rng, -0.5, 2.0);
        }
    }
    if rng.next_u64().is_multiple_of(3) {
        let v = VALUES[*k % VALUES.len()];
        *k += 1;
        let i = (rng.next_u64() % n as u64) as usize;
        match rng.next_u64() % 3 {
            0 => points[i].x = v,
            1 => points[i].y = v,
            _ => slopes[i] = v,
        }
    }
    let spline = if rng.next_u64().is_multiple_of(25) {
        BSplineType::BSpline
    } else {
        c.b_spline_type()
    };
    let mut curve = GradingBSplineCurve::with_points_and_spline_type(&points, spline);
    for (i, s) in slopes.into_iter().enumerate() {
        curve.set_slope(i, s).unwrap();
    }
    curve
}

/// Eight random curves, drawing their curves only now and then.
fn random_curves(rng: &mut Rng, k: &mut usize) -> GradingHueCurve {
    let mut curves = GradingHueCurve::new(GradingStyle::Log);
    for c in HueCurveType::CURVES {
        *curves.curve_mut(c).unwrap() = random_curve(rng, c, k);
    }
    curves.set_draw_curve_only(rng.next_u64().is_multiple_of(6));
    curves
}

fn cases() -> Vec<HcCase> {
    let mut cases = Vec::new();
    let mut rng = Rng::new(0x48e5_0da7);
    let mut k = 0;
    for style in STYLES {
        for dynamic in [false, true] {
            for rgb_to_hsy in HSY {
                for transform_dir in DIRS {
                    for processor_dir in DIRS {
                        // The defaults, the defaults drawing their curves only, then random
                        // curves.
                        for n in 0..6 {
                            let curves = match n {
                                0 => GradingHueCurve::new(style),
                                1 => {
                                    let mut curves = GradingHueCurve::new(style);
                                    curves.set_draw_curve_only(true);
                                    curves
                                }
                                _ => random_curves(&mut rng, &mut k),
                            };
                            cases.push(HcCase {
                                style,
                                curves,
                                dynamic,
                                rgb_to_hsy,
                                transform_dir,
                                processor_dir,
                            });
                        }
                    }
                }
            }
        }
    }
    // More random curves, not dynamic, forward.
    for i in 0..150 {
        cases.push(HcCase {
            style: STYLES[i % 3],
            curves: random_curves(&mut rng, &mut k),
            dynamic: false,
            rgb_to_hsy: HSY[i % 2],
            transform_dir: TransformDirection::Forward,
            processor_dir: TransformDirection::Forward,
        });
    }
    cases
}

/// The wheel's curve, written out: its control points, slopes and spline type, as the port's.
fn curve_of(dump: &Dump) -> GradingBSplineCurve {
    let Dumped::List(points) = dump.getter("getControlPoints") else {
        panic!("no control points: {dump:?}");
    };
    let points: Vec<GradingControlPoint> = points
        .iter()
        .map(|p| {
            let p = p.object();
            GradingControlPoint::new(p.property("x").f64() as f32, p.property("y").f64() as f32)
        })
        .collect();
    let spline = match dump.getter("getSplineType").name() {
        "B_SPLINE" => BSplineType::BSpline,
        "DIAGONAL_B_SPLINE" => BSplineType::DiagonalBSpline,
        "HUE_HUE_B_SPLINE" => BSplineType::HueHueBSpline,
        "PERIODIC_1_B_SPLINE" => BSplineType::Periodic1BSpline,
        "PERIODIC_0_B_SPLINE" => BSplineType::Periodic0BSpline,
        "HORIZONTAL1_B_SPLINE" => BSplineType::Horizontal1BSpline,
        other => panic!("spline type {other}"),
    };
    let mut curve = GradingBSplineCurve::with_points_and_spline_type(&points, spline);
    for (i, s) in dump.getter("getSlopes").f64s().into_iter().enumerate() {
        curve.set_slope(i, s as f32).unwrap();
    }
    curve
}

/// A curve's point and slope bits and its spline type.
type CurveBits = (Vec<u32>, Vec<u32>, BSplineType);

/// The bits of the curves (points, slopes, spline types) and the draw-curve-only flag, so
/// that NaNs compare.
fn bits(curves: &GradingHueCurve) -> (Vec<CurveBits>, bool) {
    (
        curves
            .curves()
            .iter()
            .map(|c| {
                (
                    c.control_points()
                        .iter()
                        .flat_map(|p| [p.x.to_bits(), p.y.to_bits()])
                        .collect(),
                    c.slopes().iter().map(|s| s.to_bits()).collect(),
                    c.spline_type(),
                )
            })
            .collect(),
        curves.get_draw_curve_only(),
    )
}

/// The wheel's GradingHueCurve, written out, as the port's.
fn curves_of(dump: &Dump) -> GradingHueCurve {
    let mut curves = GradingHueCurve::new(GradingStyle::Log);
    for (c, name) in HueCurveType::CURVES.into_iter().zip([
        "hue_hue", "hue_sat", "hue_lum", "lum_sat", "sat_sat", "lum_lum", "sat_lum", "hue_fx",
    ]) {
        *curves.curve_mut(c).unwrap() = curve_of(dump.property(name).object());
    }
    curves.set_draw_curve_only(matches!(
        dump.getter("getDrawCurveOnly"),
        Dumped::Bool(true)
    ));
    curves
}

/// The processor's cache ID, flags and the op data it built are the wheel's; so are the
/// curves the transform refuses, and whether the optimized processor removes the op.
#[test]
fn cache_ids_and_built_data_are_the_wheels() {
    let cases = cases();
    let requests: Vec<ProcessorOpsRequest> = cases
        .iter()
        .map(|case| ProcessorOpsRequest::new(case.processor()))
        .collect();
    let calls: Vec<_> = requests.iter().map(|r| r.call()).collect();
    let responses = ocio_testkit::Oracle::get().batch(&calls, true);
    assert_eq!(responses.len(), cases.len());
    let (mut refused, mut removed, mut kept, mut draw_removed, mut bypass) = (0, 0, 0, 0, 0);
    for (case, response) in cases.iter().zip(responses) {
        let reply = ProcessorOpsReply::from_response(response.expect("the oracle"));
        let data = match case.port() {
            Ok(data) => data,
            Err(text) => {
                let raised = reply
                    .raised()
                    .unwrap_or_else(|| panic!("{case:?}: the wheel accepts it, the port: {text}"));
                assert_eq!(raised.message, text, "{case:?}");
                refused += 1;
                continue;
            }
        };
        assert!(reply.raised().is_none(), "{case:?}: {:?}", reply.result);
        let wheel = reply.processor();

        // The cache ID.
        let mut ops = OpVec::new();
        create_grading_hue_curve_op(&mut ops, data.clone(), TransformDirection::Forward);
        let full = ops.get_cache_id().unwrap();
        assert_eq!(
            cache_id_hash(&full),
            wheel.cache_id,
            "{case:?}: {}",
            String::from_utf8_lossy(&full)
        );
        assert_eq!(wheel.is_no_op, data.is_no_op(), "{case:?}");
        assert_eq!(
            wheel.has_channel_crosstalk,
            data.has_channel_crosstalk(),
            "{case:?}"
        );
        assert_eq!(wheel.is_dynamic, data.is_dynamic(), "{case:?}");
        if data.get_rgb_to_hsy() == HsyTransformStyle::None && !data.is_dynamic() {
            bypass += 1;
        }

        // The data, through the group transform's getters.
        assert_eq!(wheel.group.children.len(), 1, "{case:?}");
        let t = &wheel.group.children[0];
        assert_eq!(t.class, "GradingHueCurveTransform", "{case:?}");
        assert_eq!(
            t.getter("getStyle"),
            &Dumped::Enum(style_name(data.get_style()).to_string()),
            "{case:?}"
        );
        assert_eq!(
            t.getter("getDirection"),
            &Dumped::Enum(dir_name(data.get_direction()).to_string()),
            "{case:?}"
        );
        assert_eq!(
            t.getter("getRGBToHSY"),
            &Dumped::Enum(hsy_name(data.get_rgb_to_hsy()).to_string()),
            "{case:?}"
        );
        assert_eq!(
            t.getter("isDynamic"),
            &Dumped::Bool(data.is_dynamic()),
            "{case:?}"
        );
        let wheel_curves = curves_of(t.getter("getValue").object());
        assert_eq!(bits(&wheel_curves), bits(&data.get_value()), "{case:?}");

        // The optimized processor: an identity is removed (its replacement is the identity
        // matrix), any other op kept.
        let optimized = reply.optimized();
        if data.is_identity() {
            assert!(optimized.group.children.is_empty(), "{case:?}");
            removed += 1;
            if data.get_value().get_draw_curve_only() {
                draw_removed += 1;
            }
        } else {
            assert_eq!(
                optimized.classes(),
                ["GradingHueCurveTransform"],
                "{case:?}"
            );
            kept += 1;
        }
    }
    // Each kind of case is there.
    assert!(refused > 30, "{refused} refused");
    assert!(removed > 20, "{removed} removed");
    assert!(
        draw_removed > 5,
        "{draw_removed} draw-curve-only identities removed"
    );
    assert!(kept > 100, "{kept} kept");
    assert!(bypass > 50, "{bypass} without the HSY conversion");
}

/// The eight curves of upstream's `log_all_curves` test (GradingHueCurveOpCPU_tests.cpp:
/// 209-233 @ v2.5.2): no curve is an identity.
fn all_curves() -> GradingHueCurve {
    let curve = |xy: &[(f32, f32)], c| {
        let points: Vec<GradingControlPoint> = xy
            .iter()
            .map(|&(x, y)| GradingControlPoint::new(x, y))
            .collect();
        GradingBSplineCurve::with_points_and_hue_curve_type(&points, c)
    };
    use HueCurveType::*;
    GradingHueCurve::with_curves(
        &curve(
            &[
                (0.05, 0.15),
                (0.2, 0.3),
                (0.35, 0.4),
                (0.45, 0.45),
                (0.6, 0.7),
                (0.8, 0.85),
            ],
            HueHue,
        ),
        &curve(
            &[
                (-0.1, 1.2),
                (0.2, 0.7),
                (0.4, 1.5),
                (0.5, 0.5),
                (0.6, 1.4),
                (0.8, 0.7),
            ],
            HueSat,
        ),
        &curve(
            &[(0.1, 1.5), (0.2, 0.7), (0.4, 1.4), (0.5, 0.8), (0.8, 0.5)],
            HueLum,
        ),
        &curve(&[(0.05, 1.5), (0.5, 0.9), (1.1, 1.4)], LumSat),
        &curve(&[(0.0, 0.1), (0.5, 0.45), (1.0, 1.1)], SatSat),
        &curve(
            &[(-0.02, -0.04), (0.2, 0.1), (0.8, 0.95), (1.1, 1.2)],
            LumLum,
        ),
        &curve(&[(0.0, 1.2), (0.6, 0.8), (0.9, 1.1)], SatLum),
        &curve(
            &[
                (0.2, 0.05),
                (0.4, -0.09),
                (0.6, -0.2),
                (0.8, 0.05),
                (0.99, -0.02),
            ],
            HueFx,
        ),
    )
    .unwrap()
}

/// A non-dynamic op and its inverse, in a group: the optimizer removes the pair (the identity
/// matrix replaces it) when the curves are equal, the draw-curve-only flags too, and, in the
/// linear style only, the RGB-to-HSY settings (I-271: in the log and video styles, an op that
/// converts and its inverse that doesn't make a pair); curves that don't equal themselves (a
/// NaN) make no pair, nor do two dynamic ops.
#[test]
fn inverse_pairs_are_removed_as_the_wheels() {
    let mut pairs = Vec::new();
    for style in STYLES {
        for dir in DIRS {
            for (first_hsy, second_hsy) in [
                (HsyTransformStyle::Transform1, HsyTransformStyle::Transform1),
                (HsyTransformStyle::None, HsyTransformStyle::None),
                (HsyTransformStyle::Transform1, HsyTransformStyle::None),
            ] {
                for variant in 0..4 {
                    let mut curves = all_curves();
                    let mut second_curves = curves.clone();
                    match variant {
                        // The same curves.
                        0 => {}
                        // A NaN slope.
                        1 => {
                            curves
                                .curve_mut(HueCurveType::HueSat)
                                .unwrap()
                                .set_slope(1, f32::NAN)
                                .unwrap();
                            second_curves = curves.clone();
                        }
                        // Draw-curve-only on one side.
                        2 => second_curves.set_draw_curve_only(true),
                        // Draw-curve-only on both.
                        _ => {
                            curves.set_draw_curve_only(true);
                            second_curves = curves.clone();
                        }
                    }
                    for dynamic in [false, true] {
                        let first = HcCase {
                            style,
                            curves: curves.clone(),
                            dynamic,
                            rgb_to_hsy: first_hsy,
                            transform_dir: dir,
                            processor_dir: TransformDirection::Forward,
                        };
                        let inverse = match dir {
                            TransformDirection::Forward => TransformDirection::Inverse,
                            TransformDirection::Inverse => TransformDirection::Forward,
                        };
                        let second = HcCase {
                            curves: second_curves.clone(),
                            rgb_to_hsy: second_hsy,
                            transform_dir: inverse,
                            ..first.clone()
                        };
                        pairs.push((first, second));
                    }
                }
            }
        }
    }
    let requests: Vec<ProcessorOpsRequest> = pairs
        .iter()
        .map(|(first, second)| {
            ProcessorOpsRequest::new(json!({"transform": {"class": "GroupTransform",
                "children": [first.transform(first.transform_dir),
                             second.transform(second.transform_dir)]}}))
        })
        .collect();
    let calls: Vec<_> = requests.iter().map(|r| r.call()).collect();
    let responses = ocio_testkit::Oracle::get().batch(&calls, true);
    assert_eq!(responses.len(), pairs.len());
    let (mut removed, mut kept, mut removed_mixed_hsy) = (0, 0, 0);
    for ((first, second), response) in pairs.iter().zip(responses) {
        let reply = ProcessorOpsReply::from_response(response.expect("the oracle"));
        assert!(reply.raised().is_none(), "{first:?}: {:?}", reply.result);
        let a = first.port().unwrap();
        let b = second.port().unwrap();
        if a.is_inverse(&b) {
            assert!(reply.optimized().group.children.is_empty(), "{first:?}");
            removed += 1;
            if a.get_rgb_to_hsy() != b.get_rgb_to_hsy() {
                removed_mixed_hsy += 1;
            }
        } else {
            assert_eq!(
                reply.optimized().classes(),
                ["GradingHueCurveTransform"; 2],
                "{first:?}"
            );
            kept += 1;
        }
    }
    // Not dynamic, no NaN, the same draw-curve-only flags: variants 0 and 3, 3 styles, 2
    // directions, 3 HSY pairs, less the linear style's mixed pair.
    assert_eq!(removed, 2 * 3 * 2 * 3 - 2 * 2, "{removed} removed");
    // The log and video styles' mixed pairs (I-271).
    assert_eq!(removed_mixed_hsy, 2 * 2 * 2, "{removed_mixed_hsy}");
    assert_eq!(kept, pairs.len() - removed, "{kept} kept");
}
