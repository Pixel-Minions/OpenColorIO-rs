// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The hue curves' fitting against the wheel: the knots and coefficients the port computes for
//! the eight curves of a GradingHueCurve are the ones the wheel's dynamic GradingHueCurve op
//! hands its GPU shader as uniforms (`gpu_shader`), bit for bit: their offsets, the knots, the
//! coefficients and the bypass. Seeded cases of each curve type: 2 to 9 points, in order or
//! not, wrapping around the period, close together, with given slopes or estimated ones, and
//! with `drawCurveOnly`, which fits identity curves too.

use ocio_ops::open_color_types::HueCurveType;
use ocio_ops::ops::gradingrgbcurve::grading_b_spline_curve::{
    GradingBSplineCurve, GradingControlPoint, KnotsCoefs,
};
use ocio_testkit::Oracle;
use ocio_testkit::oracle::BatchCall;
use ocio_testkit::probe::Rng;
use serde_json::{Value, json};

/// A float in [lo, hi).
fn uniform(rng: &mut Rng, lo: f32, hi: f32) -> f32 {
    lo + (rng.next_u64() >> 40) as f32 / (1u64 << 24) as f32 * (hi - lo)
}

/// A curve of `curve_type`: its points and, for some, slopes.
struct Curve {
    curve_type: HueCurveType,
    xy: Vec<f32>,
    slopes: Option<Vec<f32>>,
}

/// A valid curve of `curve_type`: an identity-like one now and then, else 2 to 9 points with
/// non-decreasing x (some equal or close together), past [0, 1) for the periodic curves but the
/// hue-hue one, and non-decreasing y for the diagonal ones (a hue-hue one spanning less than 1).
fn random_curve(rng: &mut Rng, curve_type: HueCurveType) -> Curve {
    let diagonal = matches!(
        curve_type,
        HueCurveType::HueHue | HueCurveType::SatSat | HueCurveType::LumLum
    );
    let hue_hue = curve_type == HueCurveType::HueHue;
    let n = 2 + (rng.next_u64() % 8) as usize;
    let flat = rng.next_u64().is_multiple_of(5);
    let mut xy = Vec::new();
    let mut x = if hue_hue {
        uniform(rng, 0.0, 0.3)
    } else {
        uniform(rng, -0.3, 0.3)
    };
    let mut y = x + uniform(rng, -0.05, 0.05);
    for k in 0..n {
        if k > 0 {
            x += match rng.next_u64() % 8 {
                0 => 0.0,
                1 => uniform(rng, 0.0, 0.002),
                _ => uniform(rng, 0.03, 0.25),
            };
            if hue_hue && x > 1.0 {
                x = 1.0;
            }
            y += uniform(rng, 0.0, 0.12);
        }
        let value = if flat {
            match curve_type {
                HueCurveType::HueFx => 0.0,
                _ if diagonal => x,
                _ => 1.0,
            }
        } else if diagonal {
            y
        } else {
            uniform(rng, 0.0, 2.0)
        };
        xy.push(x);
        xy.push(value);
    }
    let slopes = rng
        .next_u64()
        .is_multiple_of(4)
        .then(|| (0..n).map(|_| uniform(rng, -1.0, 2.0)).collect());
    Curve {
        curve_type,
        xy,
        slopes,
    }
}

fn curve_name(t: HueCurveType) -> &'static str {
    match t {
        HueCurveType::HueHue => "HUE_HUE",
        HueCurveType::HueSat => "HUE_SAT",
        HueCurveType::HueLum => "HUE_LUM",
        HueCurveType::LumSat => "LUM_SAT",
        HueCurveType::SatSat => "SAT_SAT",
        HueCurveType::LumLum => "LUM_LUM",
        HueCurveType::SatLum => "SAT_LUM",
        HueCurveType::HueFx => "HUE_FX",
        HueCurveType::NumCurves => unreachable!(),
    }
}

fn curve_spec(c: &Curve) -> Value {
    let mut spec = json!({"class": "GradingBSplineCurve",
                          "args": [c.xy, {"enum": curve_name(c.curve_type)}]});
    if let Some(slopes) = &c.slopes {
        spec["calls"] = json!([["setSlopes", slopes]]);
    }
    json!({"object": spec})
}

/// The port's knots and coefficients of the eight curves, or the error.
fn port(curves: &[Curve], draw_curve_only: bool) -> Result<KnotsCoefs, String> {
    let mut kc = KnotsCoefs::new(8);
    kc.local_bypass = false;
    kc.num_coefs = 0;
    kc.num_knots = 0;
    for (i, c) in curves.iter().enumerate() {
        let points: Vec<GradingControlPoint> =
            c.xy.chunks(2)
                .map(|p| GradingControlPoint::new(p[0], p[1]))
                .collect();
        let mut curve = GradingBSplineCurve::with_points_and_hue_curve_type(&points, c.curve_type);
        if let Some(slopes) = &c.slopes {
            for (k, s) in slopes.iter().enumerate() {
                curve.set_slope(k, *s).unwrap();
            }
        }
        curve
            .compute_knots_and_coefs(&mut kc, i, draw_curve_only)
            .map_err(|e| e.to_string())?;
    }
    if kc.num_knots <= 0 {
        kc.local_bypass = true;
    }
    Ok(kc)
}

fn f32_bits(v: &[f32]) -> Vec<u32> {
    v.iter().map(|x| x.to_bits()).collect()
}

#[test]
fn hue_curve_knots_and_coefs_match_the_wheel() {
    let mut rng = Rng::new(0x48e5_c0de);
    let mut cases = Vec::new();
    for i in 0..120u32 {
        let curves: Vec<Curve> = HueCurveType::CURVES
            .iter()
            .map(|&t| random_curve(&mut rng, t))
            .collect();
        cases.push((curves, i.is_multiple_of(5)));
    }
    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .map(|(curves, draw)| {
            let values = json!({"class": "GradingHueCurve",
                                "args": curves.iter().map(curve_spec).collect::<Vec<_>>(),
                                "calls": [["setDrawCurveOnly", draw]]});
            BatchCall {
                cmd: "gpu_shader",
                args: json!({
                    "transform": {"class": "GradingHueCurveTransform",
                                  "args": {"values": {"object": values}, "dynamic": true}},
                    "shader": {"language": "GPU_LANGUAGE_GLSL_4_0"},
                }),
                blobs: vec![],
            }
        })
        .collect();
    let replies = Oracle::get().batch(&calls, true);
    let mut compared = 0;
    let mut refusals = Vec::new();
    for (i, ((curves, draw), reply)) in cases.iter().zip(replies).enumerate() {
        let reply = reply.unwrap_or_else(|e| panic!("case {i}: {e}"));
        let result = &reply.result;
        let port = port(curves, *draw);
        if let Some(exception) = result.get("exception") {
            // The wheel refused the values (validation, or too many knots): the port's fitting
            // must fail the same way where the wheel's did.
            if result["stage"] == json!("transform") {
                refusals.push(exception["message"].to_string());
                continue;
            }
            assert_eq!(
                port.err(),
                exception["message"].as_str().map(str::to_string),
                "case {i}: {result}"
            );
            continue;
        }
        let kc = port.unwrap_or_else(|e| panic!("case {i}: the port failed: {e}"));
        let uniforms: Vec<&Value> = result["shader"]["uniforms"]
            .as_array()
            .unwrap()
            .iter()
            .collect();
        let value = |suffix: &str| -> &Value {
            &uniforms
                .iter()
                .find(|u| u["name"].as_str().unwrap().ends_with(suffix))
                .unwrap_or_else(|| panic!("case {i}: no {suffix}"))["value"]
        };
        assert_eq!(*value("knotsOffsets"), json!(kc.knots_offsets), "case {i}");
        assert_eq!(*value("coefsOffsets"), json!(kc.coefs_offsets), "case {i}");
        assert_eq!(
            *value("knots"),
            json!(f32_bits(&kc.knots[..kc.num_knots as usize])),
            "case {i}"
        );
        assert_eq!(
            *value("coefs"),
            json!(f32_bits(&kc.coefs[..kc.num_coefs as usize])),
            "case {i}"
        );
        assert_eq!(*value("localBypass"), json!(kc.local_bypass), "case {i}");
        compared += 1;
    }
    // Most cases reach the fitting.
    assert!(
        compared >= cases.len() / 2,
        "{compared} of {}: {refusals:?}",
        cases.len()
    );
}
