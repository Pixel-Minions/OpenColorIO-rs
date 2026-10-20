// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! GradingHueCurve values against the wheel (`object_calls`): the defaults of each style, and
//! seeded sets of eight curves (some of the wrong spline type, some with decreasing points, some
//! drawing their curves only), made with the eight-curve constructor, which validates: the
//! wheel's text (`operator<<`, which the binding's `repr` prints) of each one it accepts, and
//! its message for each one it refuses, are the port's.

use ocio_ops::open_color_types::{BSplineType, GradingStyle, HueCurveType};
use ocio_ops::ops::gradinghuecurve::grading_hue_curve::GradingHueCurve;
use ocio_ops::ops::gradingrgbcurve::grading_b_spline_curve::{
    GradingBSplineCurve, GradingControlPoint,
};
use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::oracle_values::bytes;
use ocio_testkit::probe::Rng;
use serde_json::{Value, json};

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

const SPLINES: [BSplineType; 6] = [
    BSplineType::BSpline,
    BSplineType::DiagonalBSpline,
    BSplineType::HueHueBSpline,
    BSplineType::Periodic1BSpline,
    BSplineType::Periodic0BSpline,
    BSplineType::Horizontal1BSpline,
];

/// A float in [lo, hi).
fn uniform(rng: &mut Rng, lo: f32, hi: f32) -> f32 {
    lo + (rng.next_u64() >> 40) as f32 / (1u64 << 24) as f32 * (hi - lo)
}

/// A curve of type `c`'s spline type (now and then another), 2 to 6 points, mostly increasing.
fn random_curve(rng: &mut Rng, c: HueCurveType) -> GradingBSplineCurve {
    let spline = if rng.next_u64().is_multiple_of(12) {
        SPLINES[(rng.next_u64() % 6) as usize]
    } else {
        c.b_spline_type()
    };
    let n = 2 + (rng.next_u64() % 5) as usize;
    let mut x = uniform(rng, -0.1, 0.2);
    let mut y = x;
    let points: Vec<GradingControlPoint> = (0..n)
        .map(|_| {
            // Now and then a step back, which validate refuses.
            x += if rng.next_u64().is_multiple_of(15) {
                -0.1
            } else {
                uniform(rng, 0.0, 0.3)
            };
            y += uniform(rng, -0.02, 0.2);
            GradingControlPoint::new(x, y)
        })
        .collect();
    GradingBSplineCurve::with_points_and_spline_type(&points, spline)
}

fn curve_spec(curve: &GradingBSplineCurve) -> Value {
    let xy: Vec<f32> = curve
        .control_points()
        .iter()
        .flat_map(|p| [p.x, p.y])
        .collect();
    let mut spec = json!({"class": "GradingBSplineCurve", "args": [xy]});
    spec["calls"] = json!([["setSplineType", {"enum": spline_name(curve.spline_type())}]]);
    json!({"object": spec})
}

#[test]
fn grading_hue_curve_text_and_validate_match_the_wheel() {
    let mut rng = Rng::new(0x48e5_7e47);
    let mut curves: Vec<[GradingBSplineCurve; 8]> =
        [GradingStyle::Log, GradingStyle::Lin, GradingStyle::Video]
            .iter()
            .map(|&s| GradingHueCurve::new(s).curves().clone())
            .collect();
    for _ in 0..60 {
        curves.push(HueCurveType::CURVES.map(|c| random_curve(&mut rng, c)));
    }
    let calls: Vec<BatchCall<'_>> = curves
        .iter()
        .map(|eight| BatchCall {
            cmd: "object_calls",
            args: json!({
                "objects": {"h": {"class": "GradingHueCurve",
                                  "args": eight.iter().map(curve_spec).collect::<Vec<_>>()}},
                "calls": [{"call": "__repr__", "on": "h"}],
            }),
            blobs: vec![],
        })
        .collect();
    let replies = Oracle::get().batch(&calls, true);
    let (mut accepted, mut refused) = (0, 0);
    for (i, (eight, reply)) in curves.iter().zip(replies).enumerate() {
        let result = reply.unwrap_or_else(|e| panic!("case {i}: {e}")).result;
        let [hh, hs, hl, ls, ss, ll, sl, hfx] = eight;
        let port = GradingHueCurve::with_curves(hh, hs, hl, ls, ss, ll, sl, hfx);
        match (&result["config"], port) {
            (Value::Null, Ok(port)) => {
                accepted += 1;
                let text = result["calls"][0]["result"]
                    .as_str()
                    .unwrap_or_else(|| panic!("case {i}: {result}"));
                assert_eq!(port.to_string(), text, "case {i}");
            }
            (wheel, Err(e)) if wheel.get("exception").is_some() => {
                refused += 1;
                assert_eq!(
                    e.what(),
                    bytes(&wheel["exception"]["message"]).as_slice(),
                    "case {i}: {result}"
                );
            }
            (wheel, port) => panic!("case {i}: the wheel {wheel}, the port {port:?}"),
        }
    }
    assert!(
        accepted > 3 && refused > 3,
        "{accepted} accepted, {refused} refused"
    );
}
