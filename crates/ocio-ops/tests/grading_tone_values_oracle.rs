// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! GradingTone values against the wheel (`object_calls`): their text (`operator<<`, which the
//! binding's `repr` prints) and `validate`'s answer, over the defaults of each style, each
//! check's boundary, and seeded random doubles with the specials (NaN, the infinities, -0.0,
//! the extremes) among them.

use ocio_ops::open_color_types::GradingStyle;
use ocio_ops::ops::gradingtone::grading_tone::{GradingRgbmsw, GradingTone};
use ocio_testkit::Oracle;
use ocio_testkit::oracle_values::bytes;
use ocio_testkit::probe::Rng;
use serde_json::{Value, json};

/// Doubles a value can take: the specials and the bounds, then seeded random values.
fn doubles(rng: &mut Rng, n: usize) -> Vec<f64> {
    let specials = [
        0.0,
        -0.0,
        1.0,
        0.1,
        0.0999,
        1.9,
        1.900002,
        0.2,
        1.8,
        0.01,
        0.009998,
        1.99,
        1.990002,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::MAX,
        -f64::MAX,
        5e-324,
        123456.789,
    ];
    let mut out = specials.to_vec();
    while out.len() < n {
        let bits = rng.next_u64();
        out.push(if bits & 1 == 0 {
            (bits >> 11) as f64 / (1u64 << 53) as f64 * 2.4 - 0.2
        } else {
            f64::from_bits(bits)
        });
    }
    out
}

/// The cases: each style's defaults, then defaults with a few components drawn from `doubles`.
fn cases() -> Vec<GradingTone> {
    let styles = [GradingStyle::Log, GradingStyle::Lin, GradingStyle::Video];
    let mut out: Vec<GradingTone> = styles.iter().map(|&s| GradingTone::new(s)).collect();
    let mut rng = Rng::new(0x70e5_2026);
    let pool = doubles(&mut rng, 64);
    let pick = |rng: &mut Rng| pool[(rng.next_u64() % pool.len() as u64) as usize];
    for i in 0..80 {
        let mut t = GradingTone::new(styles[i % 3]);
        for _ in 0..(1 + i % 3) {
            let v = pick(&mut rng);
            let part = match rng.next_u64() % 6 {
                0 => &mut t.blacks,
                1 => &mut t.shadows,
                2 => &mut t.midtones,
                3 => &mut t.highlights,
                4 => &mut t.whites,
                _ => {
                    t.scontrast = v;
                    continue;
                }
            };
            match rng.next_u64() % 6 {
                0 => part.red = v,
                1 => part.green = v,
                2 => part.blue = v,
                3 => part.master = v,
                4 => part.start = v,
                _ => part.width = v,
            }
        }
        out.push(t);
    }
    out
}

fn f64_spec(v: f64) -> Value {
    json!({"f64": v.to_bits()})
}

fn rgbmsw_spec(v: &GradingRgbmsw) -> Value {
    json!({"object": {"class": "GradingRGBMSW",
                      "args": [f64_spec(v.red), f64_spec(v.green), f64_spec(v.blue),
                               f64_spec(v.master), f64_spec(v.start), f64_spec(v.width)]}})
}

/// The object spec of `t` (the style only picks the defaults, which every field overrides).
fn tone_spec(t: &GradingTone) -> Value {
    json!({"class": "GradingTone", "args": [{"enum": "GRADING_LOG"}], "attrs": [
        ["blacks", rgbmsw_spec(&t.blacks)],
        ["shadows", rgbmsw_spec(&t.shadows)],
        ["midtones", rgbmsw_spec(&t.midtones)],
        ["highlights", rgbmsw_spec(&t.highlights)],
        ["whites", rgbmsw_spec(&t.whites)],
        ["scontrast", f64_spec(t.scontrast)],
    ]})
}

/// Each case's text and validate are the wheel's, byte for byte.
#[test]
fn grading_tone_text_and_validate_match_the_wheel() {
    let cases = cases();
    let mut objects = serde_json::Map::new();
    let mut calls = Vec::new();
    for (i, t) in cases.iter().enumerate() {
        let name = format!("t{i}");
        objects.insert(name.clone(), tone_spec(t));
        calls.push(json!({"call": "__repr__", "on": name}));
        calls.push(json!({"call": "validate", "on": name}));
    }
    let response = Oracle::get()
        .call(
            "object_calls",
            json!({"objects": objects, "calls": calls}),
            &[],
        )
        .result;
    assert_eq!(response["config"], Value::Null, "{response}");
    let results = response["calls"].as_array().unwrap();
    let mut refused = 0;
    for (i, t) in cases.iter().enumerate() {
        let text = &results[2 * i];
        let wheel_text = text["result"]
            .as_str()
            .unwrap_or_else(|| panic!("case {i}: {text}"));
        assert_eq!(t.to_string(), wheel_text, "case {i}");
        let wheel = &results[2 * i + 1];
        match t.validate() {
            Ok(()) => assert_eq!(wheel["result"], Value::Null, "case {i}: {wheel}"),
            Err(e) => {
                refused += 1;
                assert_eq!(
                    e.what(),
                    bytes(&wheel["exception"]["message"]).as_slice(),
                    "case {i}: {wheel}"
                );
            }
        }
    }
    // The cases reach both answers.
    assert!(refused > 0 && refused < cases.len(), "{refused}");
}
