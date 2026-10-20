// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! GradingPrimary values against the wheel (`object_calls`): their text (`operator<<`, which the
//! binding's `repr` prints) and `validate`'s answer for each style, over the defaults of each
//! style, typical values, each check's boundary, and seeded random doubles with the specials
//! (NaN, the infinities, -0.0, the extremes) among them.

use ocio_ops::open_color_types::GradingStyle;
use ocio_ops::ops::gradingprimary::grading_primary::{GradingPrimary, GradingRgbm};
use ocio_testkit::Oracle;
use ocio_testkit::oracle_values::bytes;
use ocio_testkit::probe::Rng;
use serde_json::{Value, json};

const STYLES: [(GradingStyle, &str); 3] = [
    (GradingStyle::Log, "GRADING_LOG"),
    (GradingStyle::Lin, "GRADING_LIN"),
    (GradingStyle::Video, "GRADING_VIDEO"),
];

/// Doubles a value can take: the specials, then seeded random bit patterns.
fn doubles(rng: &mut Rng, n: usize) -> Vec<f64> {
    let specials = [
        0.0,
        -0.0,
        1.0,
        0.01,
        0.009999,
        0.009998,
        f64::NAN,
        -f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::MAX,
        -f64::MAX,
        f64::MIN_POSITIVE,
        5e-324,
        1.0e-7,
        123456.789,
        -0.2,
        0.18,
    ];
    let mut out = specials.to_vec();
    while out.len() < n {
        let bits = rng.next_u64();
        // Half the values small decimals, half any bit pattern.
        out.push(if bits & 1 == 0 {
            (bits >> 11) as f64 / (1u64 << 53) as f64 * 4.0 - 2.0
        } else {
            f64::from_bits(bits)
        });
    }
    out
}

/// The cases: each style's defaults, then values drawn from `doubles`.
fn cases() -> Vec<GradingPrimary> {
    let mut out: Vec<GradingPrimary> = STYLES
        .iter()
        .map(|&(style, _)| GradingPrimary::new(style))
        .collect();
    let mut rng = Rng::new(0x5e7a_9019);
    let pool = doubles(&mut rng, 64);
    let pick = |rng: &mut Rng| pool[(rng.next_u64() % pool.len() as u64) as usize];
    for i in 0..60 {
        let mut p = GradingPrimary::new(STYLES[i % 3].0);
        // Change a few fields, so that the checks see one problem at a time as often as many.
        for _ in 0..(1 + i % 4) {
            let v = pick(&mut rng);
            match rng.next_u64() % 13 {
                0 => p.brightness = GradingRgbm::new(v, pick(&mut rng), v, pick(&mut rng)),
                1 => p.contrast = GradingRgbm::new(pick(&mut rng), v, 1.0, pick(&mut rng)),
                2 => p.gamma = GradingRgbm::new(1.0, pick(&mut rng), v, 1.0),
                3 => p.offset = GradingRgbm::new(v, v, pick(&mut rng), v),
                4 => p.exposure = GradingRgbm::new(pick(&mut rng), 0.0, v, 0.0),
                5 => p.lift.master = v,
                6 => p.gain.red = v,
                7 => p.saturation = v,
                8 => p.pivot = v,
                9 => p.pivot_black = v,
                10 => p.pivot_white = v,
                11 => p.clamp_black = v,
                _ => p.clamp_white = v,
            }
        }
        out.push(p);
    }
    out
}

fn f64_spec(v: f64) -> Value {
    json!({"f64": v.to_bits()})
}

fn rgbm_spec(v: &GradingRgbm) -> Value {
    json!({"object": {"class": "GradingRGBM",
                      "args": [f64_spec(v.red), f64_spec(v.green), f64_spec(v.blue),
                               f64_spec(v.master)]}})
}

/// The object spec of `p` (the style only picks the defaults, which every field overrides).
fn primary_spec(p: &GradingPrimary) -> Value {
    json!({"class": "GradingPrimary", "args": [{"enum": "GRADING_LOG"}], "attrs": [
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
    ]})
}

/// Each case's text and each style's validate are the wheel's, byte for byte.
#[test]
fn grading_primary_text_and_validate_match_the_wheel() {
    let cases = cases();
    let mut objects = serde_json::Map::new();
    let mut calls = Vec::new();
    for (i, p) in cases.iter().enumerate() {
        let name = format!("p{i}");
        objects.insert(name.clone(), primary_spec(p));
        calls.push(json!({"call": "__repr__", "on": name}));
        for (_, style) in STYLES {
            calls.push(json!({"call": "validate", "on": name, "args": [{"enum": style}]}));
        }
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
    let per_case = 1 + STYLES.len();
    for (i, p) in cases.iter().enumerate() {
        let entry = |k: usize| &results[i * per_case + k];
        let wheel_text = entry(0)["result"]
            .as_str()
            .unwrap_or_else(|| panic!("case {i}: {}", entry(0)));
        assert_eq!(p.to_string(), wheel_text, "case {i}");
        for (s, (style, _)) in STYLES.iter().enumerate() {
            let wheel = entry(1 + s);
            match p.validate(*style) {
                Ok(()) => assert_eq!(wheel["result"], Value::Null, "case {i} {style:?}: {wheel}"),
                Err(e) => {
                    assert_eq!(
                        e.what(),
                        bytes(&wheel["exception"]["message"]).as_slice(),
                        "case {i} {style:?}: {wheel}"
                    );
                }
            }
        }
    }
    // Each RGBM's own text, as the binding prints a GradingRGBM.
    let rgbm = GradingRgbm::new(f64::NAN, -0.0, 1.0e-7, f64::MAX);
    let response = Oracle::get()
        .call(
            "object_calls",
            json!({"objects": {"r": rgbm_spec(&rgbm)["object"]},
                   "calls": [{"call": "__repr__", "on": "r"}]}),
            &[],
        )
        .result;
    assert_eq!(
        rgbm.to_string(),
        response["calls"][0]["result"].as_str().unwrap(),
        "{response}"
    );
}
