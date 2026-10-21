// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The hue curves' fitting where NaN values meet, against the wheel: the knots and coefficients
//! of the eight curves of a GradingHueCurve, which the wheel's dynamic GradingHueCurve op hands
//! its GPU shader as uniforms (`gpu_shader`), bit for bit. Where two NaNs meet in an addition
//! or a multiplication, the result is the first operand's NaN, and the two wheels compile some
//! of those operations with their operands swapped (`docs/improvements.md` I-219).
//!
//! Three seeded sets of curves: y coordinates and slopes that are NaN each with its own payload
//! and sign (so a wrong operand order shows), infinities and huge values among them; the same
//! with NaN x coordinates too (through `PrepHueCurveData`'s sort and spacing); and the
//! verifier's set of NaN, infinite, huge and subnormal values (seed 0x5eed_0bad).

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

/// A quiet NaN with a payload of its own (`k`, counted up) and a random sign.
fn nan(k: &mut u32, rng: &mut Rng) -> f32 {
    *k += 1;
    let sign = if rng.next_u64().is_multiple_of(2) {
        0
    } else {
        0x8000_0000
    };
    f32::from_bits(0x7fc0_0000 | sign | (*k << 6))
}

/// The sets of curves.
#[derive(Clone, Copy, PartialEq)]
enum Set {
    /// NaN y coordinates and slopes, each with its own payload.
    Payloads,
    /// The same, and NaN x coordinates.
    PayloadsAndX,
    /// The verifier's specials.
    Specials,
}

/// A curve of `t`: 2 to 9 points (15 for the specials), x non-decreasing (some equal or close
/// together), y non-decreasing for the diagonal curves, then some values replaced by the set's
/// special values.
fn random_curve(rng: &mut Rng, t: HueCurveType, k: &mut u32, set: Set) -> Curve {
    let diagonal = matches!(
        t,
        HueCurveType::HueHue | HueCurveType::SatSat | HueCurveType::LumLum
    );
    let hue_hue = t == HueCurveType::HueHue;
    let n = if set == Set::Specials {
        2 + (rng.next_u64() % 14) as usize
    } else {
        2 + (rng.next_u64() % 8) as usize
    };
    let flat = set == Set::Specials && rng.next_u64().is_multiple_of(5);
    let mut xy = Vec::new();
    let mut x = if hue_hue {
        uniform(rng, 0.0, 0.3)
    } else {
        uniform(rng, -0.3, 0.3)
    };
    let mut y = if set == Set::Specials {
        x + uniform(rng, -0.05, 0.05)
    } else {
        x
    };
    for i in 0..n {
        if i > 0 {
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
        let mut v = if flat {
            match t {
                HueCurveType::HueFx => 0.0,
                _ if diagonal => x,
                _ => 1.0,
            }
        } else if diagonal {
            y
        } else {
            uniform(rng, 0.0, 2.0)
        };
        let mut xx = x;
        if set == Set::Specials {
            let sp = [
                f32::NAN,
                f32::INFINITY,
                f32::NEG_INFINITY,
                1e30,
                -1e30,
                1e-40,
                f32::from_bits(0xffc0_0000),
            ];
            match rng.next_u64() % 60 {
                0 | 1 => {
                    xx = if rng.next_u64().is_multiple_of(2) {
                        f32::NAN
                    } else {
                        f32::from_bits(0xffc0_0000)
                    }
                }
                2..=5 if !diagonal => v = sp[(rng.next_u64() % 7) as usize],
                6 if diagonal => v = f32::NAN,
                _ => {}
            }
        } else {
            match rng.next_u64() % 10 {
                0..=2 => v = nan(k, rng),
                3 if set == Set::PayloadsAndX => xx = nan(k, rng),
                4 => v = [f32::INFINITY, f32::NEG_INFINITY, 1e30][(rng.next_u64() % 3) as usize],
                _ => {}
            }
        }
        xy.push(xx);
        xy.push(v);
    }
    let slopes = if set == Set::Specials {
        rng.next_u64().is_multiple_of(4).then(|| {
            (0..n)
                .map(|_| {
                    let s = uniform(rng, -1.0, 2.0);
                    match rng.next_u64() % 11 {
                        0 => f32::NAN,
                        1 => f32::INFINITY,
                        2 => 1e30,
                        3 => 0.0,
                        _ => s,
                    }
                })
                .collect()
        })
    } else {
        rng.next_u64().is_multiple_of(3).then(|| {
            (0..n)
                .map(|_| match rng.next_u64() % 6 {
                    0 | 1 => nan(k, rng),
                    2 => f32::INFINITY,
                    _ => uniform(rng, -1.0, 2.0),
                })
                .collect()
        })
    };
    Curve {
        curve_type: t,
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

/// Floats as spec values, by their bits.
fn bits(v: &[f32]) -> Vec<Value> {
    v.iter()
        .map(|x| json!({"f64": f64::from(*x).to_bits()}))
        .collect()
}

fn curve_spec(c: &Curve) -> Value {
    let mut spec = json!({"class": "GradingBSplineCurve",
                          "args": [bits(&c.xy), {"enum": curve_name(c.curve_type)}]});
    if let Some(s) = &c.slopes {
        spec["calls"] = json!([["setSlopes", bits(s)]]);
    }
    json!({"object": spec})
}

/// The port's knots and coefficients of the eight curves, or the error.
fn port(curves: &[Curve]) -> Result<(Vec<u32>, Vec<u32>), String> {
    let mut kc = KnotsCoefs::new(8);
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
            .compute_knots_and_coefs(&mut kc, i, false)
            .map_err(|e| e.to_string())?;
    }
    Ok((
        kc.knots[..kc.num_knots as usize]
            .iter()
            .map(|v| v.to_bits())
            .collect(),
        kc.coefs[..kc.num_coefs as usize]
            .iter()
            .map(|v| v.to_bits())
            .collect(),
    ))
}

/// A uniform's float values, by their bits.
fn uniform_bits(result: &Value, suffix: &str) -> Vec<u32> {
    result["shader"]["uniforms"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["name"].as_str().unwrap().ends_with(suffix))
        .unwrap_or_else(|| panic!("no {suffix}: {result}"))["value"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as u32)
        .collect()
}

fn check(set: Set, seed: u64, count: usize) {
    let mut rng = Rng::new(seed);
    let cases: Vec<Vec<Curve>> = (0..count)
        .map(|_| {
            let mut k = 0;
            HueCurveType::CURVES
                .iter()
                .map(|&t| random_curve(&mut rng, t, &mut k, set))
                .collect()
        })
        .collect();
    let calls: Vec<BatchCall<'_>> = cases
        .iter()
        .map(|curves| BatchCall {
            cmd: "gpu_shader",
            args: json!({
                "transform": {"class": "GradingHueCurveTransform", "args": {
                    "values": {"object": {"class": "GradingHueCurve",
                        "args": curves.iter().map(curve_spec).collect::<Vec<_>>()}},
                    "dynamic": true}},
                "shader": {"language": "GPU_LANGUAGE_GLSL_4_0"},
            }),
            blobs: vec![],
        })
        .collect();
    let mut compared = 0;
    let mut failures = Vec::new();
    for (i, (curves, reply)) in cases
        .iter()
        .zip(Oracle::get().batch(&calls, true))
        .enumerate()
    {
        let result = reply.unwrap_or_else(|e| panic!("case {i}: {e}")).result;
        if let Some(exception) = result.get("exception") {
            // Values the transform refuses (validation) or curves too long to fit.
            if result["stage"] != json!("transform") {
                let wheel = exception["message"].as_str().map(str::to_string);
                if port(curves).err() != wheel {
                    failures.push(format!("case {i}: the wheel raised {wheel:?}"));
                }
            }
            continue;
        }
        compared += 1;
        let wheel = (
            uniform_bits(&result, "knots"),
            uniform_bits(&result, "coefs"),
        );
        match port(curves) {
            Ok(port) if port == wheel => {}
            Ok(port) => failures.push(format!(
                "case {i}: knots {:x?} / {:x?}, coefs {:x?} / {:x?} (wheel / port)",
                wheel.0, port.0, wheel.1, port.1
            )),
            Err(e) => failures.push(format!("case {i}: the port failed: {e}")),
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures of {compared}:\n{}",
        failures.len(),
        failures[..failures.len().min(5)].join("\n")
    );
    assert!(compared > count / 4, "{compared} of {count} compared");
}

#[test]
fn nan_y_and_slopes_fit_as_the_wheel() {
    check(Set::Payloads, 0x7a5e_0001, 1500);
}

#[test]
fn nan_x_fit_as_the_wheel() {
    check(Set::PayloadsAndX, 0x7a5e_0002, 1500);
}

#[test]
fn specials_fit_as_the_wheel() {
    check(Set::Specials, 0x5eed_0bad, 1500);
}
