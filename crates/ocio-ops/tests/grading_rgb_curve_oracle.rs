// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The grading curve value types (`GradingBSplineCurve`, `GradingRGBCurve`) against the wheel,
//! which builds them from object specs (`oracle/ocio_oracle/spec.py`, chunk O2.2) and hands
//! them to a `GradingRGBCurveTransform`:
//! - their text: the transform's `repr()` holds the curves' `operator<<`
//!   (src/OpenColorIO/transforms/GradingRGBCurveTransform.cpp:155-190 @ v2.5.2), control points
//!   and slopes as C++ `float`s with 6 significant digits;
//! - their validation: the binding's constructor sets the curves through the dynamic property,
//!   which validates them (`DynamicPropertyGradingRGBCurveImpl::setValue`,
//!   src/OpenColorIO/DynamicProperty.cpp:216-223), so the wheel raises
//!   `GradingRGBCurve::validate`'s message;
//! - whether they are identities: the processor's `createGroupTransform()` gives the curves
//!   back, and the dump calls `GradingRGBCurve.isIdentity()`.

use ocio_ops::dynamic_property::DynamicPropertyGradingRgbCurveImpl;
use ocio_ops::op::{OpVec, Pixels, PixelsMut};
use ocio_ops::open_color_types::{BSplineType, GradingStyle, RgbCurveType, TransformDirection};
use ocio_ops::ops::gradingrgbcurve::grading_b_spline_curve::{
    GradingBSplineCurve, GradingControlPoint,
};
use ocio_ops::ops::gradingrgbcurve::grading_rgb_curve::GradingRgbCurve;
use ocio_ops::ops::gradingrgbcurve::grading_rgb_curve_op::create_grading_rgb_curve_op;
use ocio_ops::ops::gradingrgbcurve::grading_rgb_curve_op_data::GradingRgbCurveOpData;
use ocio_testkit::assert_text_eq;
use ocio_testkit::battery::params::{A, B, Case, Channels, G, Params, Precision, R, RGB, Slot};
use ocio_testkit::battery::{self, Combo, Direction, Family, Port, Spec};
use ocio_testkit::processor_ops::ProcessorOpsRequest;
use ocio_testkit::transform_text::{Built, TransformTextRequest, f64_spec};
use serde_json::{Value, json};

/// A float as a spec value, by its bits.
fn f32_spec(v: f32) -> Value {
    f64_spec(f64::from(v))
}

/// A curve's object spec: the binding's constructor from a list of control points, then the
/// slopes (when any is set) and the spline type (when it isn't `B_SPLINE`).
fn curve_spec(curve: &GradingBSplineCurve) -> Value {
    let points = curve.control_points();
    // The binding's constructor takes at least 2 points: fewer are cut down from 2 copies.
    let given = if points.len() < 2 {
        vec![points[0]; 2]
    } else {
        points.to_vec()
    };
    let values: Vec<Value> = given
        .iter()
        .flat_map(|p| [f32_spec(p.x), f32_spec(p.y)])
        .collect();
    let mut calls = Vec::new();
    if points.len() < 2 {
        calls.push(json!(["setNumControlPoints", points.len()]));
    }
    if curve.slopes().iter().any(|s| s.to_bits() != 0) {
        let slopes: Vec<Value> = curve.slopes().iter().map(|&s| f32_spec(s)).collect();
        calls.push(json!(["setSlopes", slopes]));
    }
    let spline_type = match curve.spline_type() {
        BSplineType::BSpline => None,
        BSplineType::DiagonalBSpline => Some("DIAGONAL_B_SPLINE"),
        BSplineType::HueHueBSpline => Some("HUE_HUE_B_SPLINE"),
        BSplineType::Periodic1BSpline => Some("PERIODIC_1_B_SPLINE"),
        BSplineType::Periodic0BSpline => Some("PERIODIC_0_B_SPLINE"),
        BSplineType::Horizontal1BSpline => Some("HORIZONTAL1_B_SPLINE"),
    };
    if let Some(name) = spline_type {
        calls.push(json!(["setSplineType", {"enum": name}]));
    }
    json!({"object": {"class": "GradingBSplineCurve", "args": [values], "calls": calls}})
}

/// A `GradingRGBCurveTransform` of the four curves, log style.
fn transform_spec(curves: &GradingRgbCurve) -> Value {
    let curve = |c| curve_spec(curves.curve(c).unwrap());
    json!({"class": "GradingRGBCurveTransform", "args": {
        "values": {"object": {"class": "GradingRGBCurve", "args": {
            "red": curve(RgbCurveType::Red),
            "green": curve(RgbCurveType::Green),
            "blue": curve(RgbCurveType::Blue),
            "master": curve(RgbCurveType::Master),
        }}},
        "style": {"enum": "GRADING_LOG"},
    }})
}

fn curve(xy: &[(f32, f32)]) -> GradingBSplineCurve {
    let points: Vec<GradingControlPoint> = xy
        .iter()
        .map(|&(x, y)| GradingControlPoint::new(x, y))
        .collect();
    GradingBSplineCurve::with_points(&points)
}

fn with_slopes(mut c: GradingBSplineCurve, slopes: &[f32]) -> GradingBSplineCurve {
    for (i, &s) in slopes.iter().enumerate() {
        c.set_slope(i, s).unwrap();
    }
    c
}

/// Valid curves whose numbers print in every form `%g` has: integers, fractions of 6 and
/// more digits, exponents, negative zero, the extremes, and slopes (default, set, and -0.0,
/// which counts as default).
fn valid_curves() -> Vec<GradingRgbCurve> {
    let identity = curve(&[(0.0, 0.0), (1.0, 1.0)]);
    let odd = curve(&[
        (-f32::MAX, -f32::MAX),
        (-1234567.0, -0.0),
        (-0.0, 0.0),
        (1e-7, 1.0 / 3.0),
        (0.1, 0.40000001),
        (123456.7, 1e10),
        (f32::MAX, f32::MAX),
    ]);
    let tiny = curve(&[
        (f32::from_bits(1), f32::MIN_POSITIVE),
        (1e-30, 2e-5),
        (0.000123456, 9.99999e-5),
    ]);
    let sloped = with_slopes(
        curve(&[(0.0, 0.1), (0.5, 0.6), (1.0, 1.2)]),
        &[1.0, -0.0, 2.5e-8],
    );
    let negative_zero_slopes = with_slopes(identity.clone(), &[-0.0, -0.0]);
    vec![
        GradingRgbCurve::with_curves(&identity, &identity, &identity, &identity),
        GradingRgbCurve::with_curves(&odd, &tiny, &sloped, &negative_zero_slopes),
        GradingRgbCurve::with_curves(&sloped, &odd, &identity, &tiny),
        GradingRgbCurve::new(GradingStyle::Lin),
    ]
}

/// The curves' text is the wheel's: the transform's `repr()` holds it after `values=`.
#[test]
fn curves_print_as_the_wheel_prints_them() {
    let cases = valid_curves();
    let reply = TransformTextRequest {
        transforms: cases.iter().map(transform_spec).collect(),
        pairs: Vec::new(),
    }
    .run();
    for (curves, built) in cases.iter().zip(&reply.transforms) {
        let repr = &built.text().repr;
        let values = repr
            .split_once(", values=")
            .and_then(|(_, rest)| rest.strip_suffix('>'))
            .unwrap_or_else(|| panic!("no values in {repr}"));
        assert_text_eq("the curves' text", values, &curves.to_string());
    }
}

/// Curves that don't validate: the wheel raises `GradingRGBCurve::validate`'s message, which
/// the port's validation gives, byte for byte.
#[test]
fn invalid_curves_raise_the_wheels_messages() {
    let identity = curve(&[(0.0, 0.0), (1.0, 1.0)]);
    let one_point = curve(&[(0.5, 0.5)]);
    let x_decreases = curve(&[(0.0, 0.0), (0.30000001, 0.3), (0.1, 0.7), (1.0, 1.0)]);
    let x_below_max = curve(&[(f32::NEG_INFINITY, 0.0), (1.0, 1.0)]);
    let y_decreases = curve(&[(0.0, 0.0), (0.3, 1e-7), (0.5, -1e-7), (1.0, 1.0)]);
    let mut diagonal = identity.clone();
    diagonal.set_spline_type(BSplineType::DiagonalBSpline);
    let mut periodic = curve(&[(0.0, 0.0), (1.0, 0.5)]);
    periodic.set_spline_type(BSplineType::Periodic1BSpline);
    let i = &identity;
    let cases = [
        GradingRgbCurve::with_curves(&one_point, i, i, i),
        GradingRgbCurve::with_curves(i, &x_decreases, i, i),
        GradingRgbCurve::with_curves(i, i, &x_below_max, i),
        GradingRgbCurve::with_curves(i, i, i, &y_decreases),
        GradingRgbCurve::with_curves(&diagonal, i, i, i),
        GradingRgbCurve::with_curves(i, i, &periodic, i),
    ];
    let reply = TransformTextRequest {
        transforms: cases.iter().map(transform_spec).collect(),
        pairs: Vec::new(),
    }
    .run();
    for (curves, built) in cases.iter().zip(&reply.transforms) {
        let Built::Raised(wheel) = built else {
            panic!("the wheel accepts {curves}");
        };
        let port = curves.validate().expect_err("the port refuses it");
        assert_text_eq("the validation's message", &wheel.message, port.message());
    }
    for curves in valid_curves() {
        curves.validate().unwrap();
    }
}

/// Curves of the most knots an op takes, 120 (GradingBSplineCurve.h:118 @ v2.5.2), and of one
/// more: the wheel's GradingRGBCurveTransform takes the first and raises the second's message,
/// as the port's fitting does. A master curve of points on y = 2x is fitted with one segment
/// between two points, so its knots are its points; the other curves are identities, which
/// have none.
#[test]
fn the_most_knots_are_the_wheels() {
    let identity = curve(&[(0.0, 0.0), (1.0, 1.0)]);
    let line = |n: u16| {
        let points: Vec<(f32, f32)> = (0..n).map(|k| (f32::from(k), 2.0 * f32::from(k))).collect();
        curve(&points)
    };
    let i = &identity;
    let cases = [
        GradingRgbCurve::with_curves(i, i, i, &line(119)),
        GradingRgbCurve::with_curves(i, i, i, &line(120)),
        GradingRgbCurve::with_curves(i, i, i, &line(121)),
        GradingRgbCurve::with_curves(&line(60), i, i, &line(60)),
        GradingRgbCurve::with_curves(&line(60), i, i, &line(61)),
    ];
    let reply = TransformTextRequest {
        transforms: cases.iter().map(transform_spec).collect(),
        pairs: Vec::new(),
    }
    .run();
    let (mut accepted, mut refused) = (0, 0);
    for (curves, built) in cases.iter().zip(&reply.transforms) {
        let port = DynamicPropertyGradingRgbCurveImpl::new(curves, false);
        match (built, port) {
            (Built::Text(_), Ok(_)) => accepted += 1,
            (Built::Raised(wheel), Err(port)) => {
                refused += 1;
                assert_text_eq("the fitting's message", &wheel.message, port.message());
            }
            (built, port) => panic!("{curves}: the wheel gives {built:?}, the port {port:?}"),
        }
    }
    assert!(
        accepted > 0 && refused > 0,
        "the cases are on both sides of the wheel's limit"
    );
}

/// Whether the curves are identities, as the wheel's `GradingRGBCurve.isIdentity()` says of
/// the curves the processor's group gives back.
#[test]
fn identities_are_the_wheels() {
    let identity = curve(&[(0.0, 0.0), (1.0, 1.0)]);
    let mut cases = valid_curves();
    let shifted = curve(&[(0.0, 0.0), (1.0, 1.0000001)]);
    let sloped = with_slopes(identity.clone(), &[1.0, 1.0]);
    let i = &identity;
    cases.push(GradingRgbCurve::with_curves(i, i, i, &shifted));
    cases.push(GradingRgbCurve::with_curves(i, &sloped, i, i));
    for curves in &cases {
        let mut request = ProcessorOpsRequest::new(json!({ "transform": transform_spec(curves) }));
        request.optimization = Some(json!("OPTIMIZATION_NONE"));
        let reply = request.run();
        let group = &reply.processor().group;
        let value = group.children[0].getter("getValue").object();
        let wheel = matches!(
            value.getter("isIdentity"),
            ocio_testkit::processor_ops::Dumped::Bool(true)
        );
        assert_eq!(curves.is_identity(), wheel, "{curves}");
    }
}

// ---------------------------------------------------------------------------------------------
// The op's renderers against the wheel: the battery (chunk 2.6d).
// ---------------------------------------------------------------------------------------------

/// A GradingRGBCurve op's parameters: the style, whether the linear style bypasses its
/// conversion, whether the op is dynamic, and the four curves (red, green, blue, master) as
/// control points and slopes. The y coordinates and the slopes are the battery's slots (each
/// applies to its curve's channel, master's to all three); the x coordinates are not, since a
/// NaN x makes the wheel read past the control points (U-35): their extremes are explicit
/// cases.
#[derive(Debug, Clone)]
struct RgbCurveParams {
    style: GradingStyle,
    bypass: bool,
    dynamic: bool,
    points: [Vec<(f32, f32)>; 4],
    slopes: [Vec<f32>; 4],
}

/// The channels each curve applies to.
const CURVE_CHANNELS: [Channels; 4] = [R, G, B, RGB];
const CURVE_NAMES: [&str; 4] = ["red", "green", "blue", "master"];

impl RgbCurveParams {
    /// The curves, with default slopes.
    fn new(style: GradingStyle, points: [&[(f32, f32)]; 4]) -> Self {
        RgbCurveParams {
            style,
            bypass: false,
            dynamic: false,
            points: points.map(<[(f32, f32)]>::to_vec),
            slopes: points.map(|p| vec![0.0; p.len()]),
        }
    }

    /// The same curves with these slopes on curve `c`.
    fn with_slopes(mut self, c: usize, slopes: &[f32]) -> Self {
        self.slopes[c] = slopes.to_vec();
        self
    }

    /// Each slot's curve, whether it is a slope, and its index.
    fn slot_places(&self) -> Vec<(usize, bool, usize)> {
        let mut places = Vec::new();
        for c in 0..4 {
            places.extend((0..self.points[c].len()).map(|i| (c, false, i)));
            places.extend((0..self.slopes[c].len()).map(|i| (c, true, i)));
        }
        places
    }

    /// The port's curves.
    fn curves(&self) -> GradingRgbCurve {
        let make = |c: usize| {
            let mut made = curve(&self.points[c]);
            for (i, &s) in self.slopes[c].iter().enumerate() {
                made.set_slope(i, s).unwrap();
            }
            made
        };
        GradingRgbCurve::with_curves(&make(0), &make(1), &make(2), &make(3))
    }
}

impl Params for RgbCurveParams {
    fn slots(&self) -> Vec<Slot> {
        self.slot_places()
            .into_iter()
            .map(|(c, slope, i)| {
                let what = if slope { "slope" } else { "y" };
                Slot::new(
                    format!("{}[{i}].{what}", CURVE_NAMES[c]),
                    Precision::F32,
                    CURVE_CHANNELS[c],
                )
            })
            .collect()
    }
    fn get(&self, index: usize) -> f64 {
        let (c, slope, i) = self.slot_places()[index];
        f64::from(if slope {
            self.slopes[c][i]
        } else {
            self.points[c][i].1
        })
    }
    fn set(&mut self, index: usize, value: f64) {
        let (c, slope, i) = self.slot_places()[index];
        if slope {
            self.slopes[c][i] = value as f32;
        } else {
            self.points[c][i].1 = value as f32;
        }
    }
}

fn style_name(style: GradingStyle) -> &'static str {
    match style {
        GradingStyle::Log => "GRADING_LOG",
        GradingStyle::Lin => "GRADING_LIN",
        GradingStyle::Video => "GRADING_VIDEO",
    }
}

struct RgbCurveFamily;

impl Family for RgbCurveFamily {
    type Params = RgbCurveParams;

    fn name(&self) -> String {
        "GradingRGBCurveTransform".to_string()
    }

    fn cases(&self) -> Vec<Case<RgbCurveParams>> {
        explicit_cases()
    }

    fn mutation_bases(&self) -> Vec<Case<RgbCurveParams>> {
        explicit_cases()
            .into_iter()
            .filter(|c| {
                matches!(
                    c.label(),
                    "upstream log" | "upstream lin" | "upstream slopes" | "lin, bypass"
                )
            })
            .collect()
    }

    /// `GradingRGBCurveTransform(values=GradingRGBCurve(red, green, blue, master), style,
    /// dynamic, dir)`, then `setBypassLinToLog`.
    fn spec(&self, p: &RgbCurveParams, direction: Direction) -> Spec {
        Spec::Transform(json!({"class": "GradingRGBCurveTransform",
            "args": {
                "values": {"object": {"class": "GradingRGBCurve", "args": {
                    "red": curve_spec(p.curves().curve(RgbCurveType::Red).unwrap()),
                    "green": curve_spec(p.curves().curve(RgbCurveType::Green).unwrap()),
                    "blue": curve_spec(p.curves().curve(RgbCurveType::Blue).unwrap()),
                    "master": curve_spec(p.curves().curve(RgbCurveType::Master).unwrap()),
                }}},
                "style": {"enum": style_name(p.style)},
                "dynamic": p.dynamic,
                "dir": direction.oracle_enum(),
            },
            "calls": [["setBypassLinToLog", p.bypass]],
        }))
    }

    /// The op data as the binding's `GradingRGBCurveTransform` constructor makes it
    /// (src/bindings/python/transforms/PyGradingRGBCurveTransform.cpp:17-34 @ v2.5.2:
    /// `Create(style)`, `setValue`, `makeDynamic`, `setDirection`, `validate`), then
    /// `setBypassLinToLog`; then `BuildGradingRGBCurveOp`'s copy of it
    /// (src/OpenColorIO/ops/gradingrgbcurve/GradingRGBCurveOp.cpp:235-248), forward from the
    /// raw config's processor, and the renderer of the op. The CPU processor applies it, its
    /// first op, from the source image to the destination (`apply_bit_depth`).
    fn port(&self, p: &RgbCurveParams, combo: &Combo) -> Result<Port, String> {
        let message = |e: ocio_ops::exception::Exception| e.message().to_string();
        let mut data = GradingRgbCurveOpData::new(p.style);
        data.set_value(&p.curves()).map_err(message)?;
        if p.dynamic {
            data.get_dynamic_property_internal().make_dynamic();
        }
        data.set_direction(match combo.direction {
            Direction::Forward => TransformDirection::Forward,
            Direction::Inverse => TransformDirection::Inverse,
        });
        data.validate().map_err(message)?;
        data.set_bypass_lin_to_log(p.bypass);
        let mut ops = OpVec::new();
        create_grading_rgb_curve_op(
            &mut ops,
            std::hint::black_box(data.clone()),
            TransformDirection::Forward,
        );
        let renderer = ops[0]
            .get_cpu_op(combo.fast_math)
            .map_err(message)?
            .expect("a renderer");
        Ok(Port::in_place(move |px| {
            let input = px.to_vec();
            renderer.apply_bit_depth(Pixels::F32(&input), PixelsMut::F32(px));
        }))
    }

    fn pass_through(&self, _: &RgbCurveParams, _: &Combo) -> Channels {
        A
    }

    /// The control points' x (forward) and y (inverse) coordinates of every curve; for the
    /// linear style, the breaks of its conversions to and from the grading log, `xbrk` and
    /// `ybrk` (GradingRGBCurveOpCPU.cpp:161-166 @ v2.5.2): `LinLog` takes the pixels, and maps
    /// those at `xbrk` to `ybrk` or next to it, where `LogLin` takes a channel whose curves are
    /// identities.
    fn breakpoints(&self, p: &RgbCurveParams, direction: Direction) -> Vec<f32> {
        let mut points: Vec<f32> = p
            .points
            .iter()
            .flatten()
            .map(|&(x, y)| {
                if direction == Direction::Forward {
                    x
                } else {
                    y
                }
            })
            .filter(|v| v.is_finite())
            .collect();
        if p.style == GradingStyle::Lin {
            points.extend([0.0041318374739483946, -5.5]);
        }
        points
    }
}

/// A shaper: its name, control points and slopes.
type Shaper = (&'static str, Vec<(f32, f32)>, Vec<f32>);

/// The ACES 1.x shapers of the built-in output transforms
/// (src/OpenColorIO/transforms/builtins/ACES.cpp:174-385 @ v2.5.2): the RRT, the SDR ODT, and
/// the four HDR RRT+ODT ones, on the master curve of a log-style op with identity RGB curves.
fn aces_shapers() -> Vec<Shaper> {
    vec![
        (
            "ACES RRT shaper",
            vec![
                (-5.26017743, -4.0),
                (-3.75502745, -3.57868829),
                (-2.24987747, -1.82131329),
                (-0.74472749, 0.68124124),
                (1.06145248, 2.87457742),
                (2.86763245, 3.83406206),
                (4.67381243, 4.0),
            ],
            vec![
                0.0, 0.55982688, 1.77532247, 1.55, 0.8787017, 0.18374463, 0.0,
            ],
        ),
        (
            "ACES SDR ODT shaper",
            vec![
                (-2.54062362, -1.69897000),
                (-2.08035721, -1.58843500),
                (-1.62009080, -1.35350000),
                (-1.15982439, -1.04695000),
                (-0.69955799, -0.65640000),
                (-0.23929158, -0.22141000),
                (0.22097483, 0.22814402),
                (0.68124124, 0.68124124),
                (1.01284632, 0.99142189),
                (1.34445140, 1.25800000),
                (1.67605648, 1.44995000),
                (2.00766156, 1.55910000),
                (2.33926665, 1.62260000),
                (2.67087173, 1.66065457),
                (3.00247681, 1.68124124),
            ],
            vec![
                0.0, 0.4803088, 0.5405565, 0.79149813, 0.9055625, 0.98460368, 0.96884766, 1.0,
                0.87078346, 0.73702127, 0.42068113, 0.23763206, 0.14535362, 0.08416378, 0.04,
            ],
        ),
        (
            "ACES HDR 1000 nit shaper",
            vec![
                (-5.60050155, -4.00000000),
                (-4.09535157, -3.57868829),
                (-2.59020159, -1.82131329),
                (-1.08505161, 0.68124124),
                (0.22347059, 2.22673503),
                (1.53199279, 2.87906206),
                (2.84051500, 3.00000000),
            ],
            vec![
                0.0, 0.55982688, 1.77532247, 1.55, 0.81219728, 0.1848466, 0.0,
            ],
        ),
        (
            "ACES HDR 2000 nit shaper",
            vec![
                (-5.59738488, -4.00000000),
                (-4.09223490, -3.57868829),
                (-2.58708492, -1.82131329),
                (-1.08193494, 0.68124124),
                (0.37639718, 2.42130131),
                (1.83472930, 3.16609199),
                (3.29306142, 3.30103000),
            ],
            vec![
                0.0, 0.55982688, 1.77532247, 1.55, 0.83637009, 0.18505799, 0.0,
            ],
        ),
        (
            "ACES HDR 4000 nit shaper",
            vec![
                (-5.59503319, -4.00000000),
                (-4.08988322, -3.57868829),
                (-2.58473324, -1.82131329),
                (-1.07958326, 0.68124124),
                (0.52855878, 2.61625839),
                (2.13670081, 3.45351273),
                (3.74484285, 3.60205999),
            ],
            vec![
                0.0, 0.55982688, 1.77532247, 1.55, 0.85652519, 0.18474395, 0.0,
            ],
        ),
        (
            "ACES HDR 108 nit shaper",
            vec![
                (-5.37852506, -4.00000000),
                (-3.87337508, -3.57868829),
                (-2.36822510, -1.82131329),
                (-0.86307513, 0.68124124),
                (-0.03557710, 1.60464482),
                (0.79192092, 1.96008059),
                (1.61941895, 2.03342376),
            ],
            vec![
                0.0, 0.55982688, 1.77532247, 1.55, 0.68179646, 0.17726487, 0.0,
            ],
        ),
    ]
}

/// Upstream's test curves (tests/cpu/ops/gradingrgbcurve/GradingRGBCurveOpCPU_tests.cpp @
/// v2.5.2), the ACES shapers, dynamic and bypassed variants, the default curves of each style,
/// and x coordinates at the extremes of `float`.
fn explicit_cases() -> Vec<Case<RgbCurveParams>> {
    use GradingStyle::{Lin, Log, Video};
    let identity: &[(f32, f32)] = &[(0.0, 0.0), (1.0, 1.0)];
    let lin_rgb: &[(f32, f32)] = &[(-6.0, -8.0), (-2.0, -5.0), (2.0, 4.0), (5.0, 6.0)];
    let lin_m: &[(f32, f32)] = &[(0.0, 0.0), (0.5, 0.5), (1.0, 1.0)];
    let default_log: &[(f32, f32)] = &[(0.0, 0.0), (0.5, 0.5), (1.0, 1.0)];
    let default_lin: &[(f32, f32)] = &[(-7.0, -7.0), (0.0, 0.0), (7.0, 7.0)];
    let upstream_log = RgbCurveParams::new(
        Log,
        [
            &[(0.1, 0.15), (0.55, 0.45), (0.9, 1.1)],
            &[(0.1, 0.15), (0.55, 0.35), (0.9, 1.1)],
            &[(0.1, 0.15), (0.55, 0.85), (0.9, 1.1)],
            &[(-0.1, 0.1), (1.1, 1.3)],
        ],
    );
    let upstream_lin = RgbCurveParams::new(Lin, [lin_rgb, lin_rgb, lin_rgb, lin_m]);
    let slopes_m = &aces_shapers()[0];
    let upstream_slopes = RgbCurveParams::new(Log, [identity, identity, identity, &slopes_m.1])
        .with_slopes(3, &slopes_m.2);
    let mut cases = vec![
        Case::new("upstream log", upstream_log.clone()),
        Case::new(
            "upstream log partial identity",
            RgbCurveParams::new(
                Log,
                [
                    &[(0.1, 0.1), (0.9, 0.9)],
                    &[(0.1, 0.15), (0.55, 0.35), (0.9, 1.1)],
                    &[(0.0, 0.0), (0.5, 0.5), (1.0, 1.0)],
                    &[(0.1, 0.1), (1.1, 1.1)],
                ],
            ),
        ),
        Case::new(
            "upstream monotonic",
            RgbCurveParams::new(
                Log,
                [
                    &[
                        (0.0, 0.0),
                        (0.785, 0.231),
                        (0.809, 0.631),
                        (0.948, 0.704),
                        (1.0, 1.0),
                    ],
                    &[(-0.1, -0.1), (1.1, 1.1)],
                    &[(-0.1, -0.1), (1.1, 1.1)],
                    &[(-0.1, -0.1), (1.1, 1.1)],
                ],
            ),
        ),
        Case::new("upstream lin", upstream_lin.clone()),
        Case::new(
            "lin, bypass",
            RgbCurveParams {
                bypass: true,
                ..upstream_lin.clone()
            },
        ),
        Case::new("upstream slopes", upstream_slopes),
        Case::new(
            "log curves, video style",
            RgbCurveParams {
                style: Video,
                ..upstream_log.clone()
            },
        ),
        Case::new(
            "upstream lin, dynamic",
            RgbCurveParams {
                dynamic: true,
                ..upstream_lin
            },
        ),
        Case::new(
            "default log curves, dynamic",
            RgbCurveParams {
                dynamic: true,
                ..RgbCurveParams::new(Log, [default_log; 4])
            },
        ),
        Case::new(
            "default lin curves, a slope on green",
            RgbCurveParams::new(Lin, [default_lin; 4]).with_slopes(1, &[0.0, 2.0, 0.0]),
        ),
        Case::new(
            "x at the extremes of float",
            RgbCurveParams::new(
                Log,
                [
                    &[(f32::NEG_INFINITY, -1.0), (0.5, 0.25), (f32::INFINITY, 2.0)],
                    &[(-f32::MAX, -1.0), (0.0, 0.0), (f32::MAX, 1.0)],
                    &[(f32::from_bits(1), 0.0), (1e-38, 0.5), (1.0, 1.0)],
                    &[(-1e38, -1.0), (1.0, 2.0)],
                ],
            ),
        ),
        Case::new(
            "lin, red curve and master identities",
            RgbCurveParams::new(Lin, [default_lin, lin_rgb, lin_rgb, lin_m]),
        ),
        // The inverse's low end where the first slope is near 0 but above its 1e-5 test
        // (GradingBSplineCurve.cpp:1344 @ v2.5.2).
        Case::new(
            "a first slope of 5e-4",
            RgbCurveParams::new(Log, [default_log, identity, identity, identity])
                .with_slopes(0, &[5e-4, 1.0, 1.0]),
        ),
        Case::new(
            "x repeated",
            RgbCurveParams::new(
                Log,
                [
                    &[(0.0, 0.0), (0.5, 0.2), (0.5, 0.8), (1.0, 1.0)],
                    identity,
                    &[(0.2, 0.1), (0.2, 0.1), (0.9, 0.95)],
                    identity,
                ],
            ),
        ),
    ];
    for (label, points, slopes) in aces_shapers() {
        cases.push(Case::new(
            label,
            RgbCurveParams::new(Log, [identity, identity, identity, &points])
                .with_slopes(3, &slopes),
        ));
    }
    cases
}

/// The GradingRGBCurve op's renderers against the wheel's GradingRGBCurveTransform: every
/// case in both directions with fast math on and off (the renderers ignore it), the
/// generated extreme, NaN and infinite y coordinates and slopes, on the tier's probes.
#[test]
fn renderers_match_the_wheel() {
    battery::run(&RgbCurveFamily);
}

/// In place, the linear style's renderers convert alpha to the grading log and back (I-90):
/// Python's `applyRGBA` on an array processes it in place, the first op too, where the
/// battery's images go from one buffer to another. Every value of the specials in every
/// channel, forward and inverse, bypassed and not, compares bit for bit with
/// `CpuOp::apply`.
#[test]
fn in_place_the_linear_style_converts_alpha() {
    let pixels = ocio_testkit::probe::to_rgba_cycled(&ocio_testkit::probe::specials());
    let bytes = ocio_testkit::oracle::f32_to_bytes(&pixels);
    let lin = explicit_cases()
        .into_iter()
        .find(|c| c.label() == "upstream lin")
        .expect("the case")
        .params()
        .clone();
    let mut cases = Vec::new();
    for bypass in [false, true] {
        for direction in Direction::BOTH {
            cases.push((
                RgbCurveParams {
                    bypass,
                    ..lin.clone()
                },
                direction,
            ));
        }
    }
    let requests: Vec<ocio_testkit::image::RgbRequest> = cases
        .iter()
        .map(|(p, direction)| {
            let Spec::Transform(transform) = RgbCurveFamily.spec(p, *direction) else {
                panic!("a transform spec");
            };
            ocio_testkit::image::RgbRequest {
                processor: json!({ "transform": transform }),
                rgba: true,
                input: ocio_testkit::image::RgbInput::array(bytes.clone(), "float32"),
            }
        })
        .collect();
    let calls: Vec<_> = requests.iter().map(|r| r.call()).collect();
    let mut converted = 0;
    for ((p, direction), response) in cases
        .iter()
        .zip(ocio_testkit::Oracle::get().batch(&calls, true))
    {
        let reply = ocio_testkit::image::RgbReply::from_response(response.expect("the oracle"));
        assert!(reply.raised().is_none(), "{:?}", reply.result);
        let wheel = ocio_testkit::oracle::bytes_to_f32(reply.output.as_deref().expect("the array"));
        let combo = Combo {
            direction: *direction,
            fast_math: true,
            format: battery::Format::F32_RGBA,
        };
        let mut data = GradingRgbCurveOpData::new(p.style);
        data.set_value(&p.curves()).unwrap();
        data.set_direction(match direction {
            Direction::Forward => TransformDirection::Forward,
            Direction::Inverse => TransformDirection::Inverse,
        });
        data.set_bypass_lin_to_log(p.bypass);
        let mut ops = OpVec::new();
        create_grading_rgb_curve_op(&mut ops, data, TransformDirection::Forward);
        let renderer = ops[0]
            .get_cpu_op(combo.fast_math)
            .unwrap()
            .expect("a renderer");
        let mut port = pixels.clone();
        renderer.apply(&mut port);
        ocio_testkit::assert_f32_bits_eq(&format!("{combo}, bypass {}", p.bypass), &wheel, &port);
        let alpha_changed = wheel
            .iter()
            .zip(&pixels)
            .skip(3)
            .step_by(4)
            .any(|(w, i)| w.to_bits() != i.to_bits());
        assert_eq!(alpha_changed, !p.bypass, "{combo}, bypass {}", p.bypass);
        converted += usize::from(alpha_changed);
    }
    assert_eq!(converted, 2);
}
