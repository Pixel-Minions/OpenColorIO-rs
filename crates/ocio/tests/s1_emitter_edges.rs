// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! S1 edge cases (docs/cards/phase0.md, WP 0.5 + S1): empty containers, strings that need
//! quotes or escapes, multi-line and empty descriptions, non-ASCII text, long strings and
//! keys, special floats and precision edges.
//!
//! The oracle builds each config through the Python API (`ocio_oracle/text.py`
//! `serialize_built_config`) and returns `serialize()`; the port builds the same config
//! through its API with the same calls (`common::built_config`) and serializes it
//! (`Config::serialize`), and the texts must be equal byte for byte. The fixtures' cases
//! (`yaml_emitter/*/spec.json`) are the wheel's committed texts; the random numbers are
//! compared live.

mod common;

use common::built_config::build_config;
use ocio_testkit::probe::Rng;
use ocio_testkit::{Oracle, assert_text_eq, fixtures};
use serde_json::{Value, json};

/// Serializes the port's config of `spec` and compares with `expected`.
fn check(label: &str, spec: &Value, expected: &str) {
    let text = build_config(spec)
        .serialize()
        .unwrap_or_else(|e| panic!("{label}: {e}"));
    assert_text_eq(label, expected, std::str::from_utf8(&text).unwrap());
}

#[test]
fn edge_cases_serialize_byte_identically() {
    let specs: Vec<String> = fixtures::list("yaml_emitter/")
        .into_iter()
        .filter(|p| p.ends_with("/spec.json"))
        .collect();
    assert_eq!(specs.len(), 11, "the yaml_emitter cases: {specs:?}");
    for spec_path in specs {
        let case: Value = serde_json::from_str(&fixtures::read_text(&spec_path))
            .unwrap_or_else(|e| panic!("{spec_path}: {e}"));
        let text_path = spec_path.replace("/spec.json", "/serialize.ocio");
        let expected = fixtures::read_text(&text_path);
        // The cases with substitutions keep their spec under "spec", with placeholders for
        // the strings in "substitutions" (for the test-only writer this test used before the
        // port's): the spec gets its strings back.
        match case.get("substitutions") {
            Some(subs) => {
                let mut spec = case["spec"].clone();
                for pair in subs.as_array().expect("substitutions") {
                    substitute(&mut spec, &pair[0], &pair[1]);
                }
                check(&text_path, &spec, &expected);
            }
            None => check(&text_path, &case, &expected),
        }
    }
}

/// Replaces every string `placeholder` of `spec` with `string`.
fn substitute(spec: &mut Value, placeholder: &Value, string: &Value) {
    match spec {
        Value::String(_) if spec == placeholder => *spec = string.clone(),
        Value::Array(items) => {
            for v in items {
                substitute(v, placeholder, string);
            }
        }
        Value::Object(map) => {
            for v in map.values_mut() {
                substitute(v, placeholder, string);
            }
        }
        _ => {}
    }
}

/// Numbers per kind in `random_numbers_serialize_like_the_wheel`: `OCIO_RS_S1_SWEEP`, or
/// 20,000.
fn sweep_count() -> usize {
    std::env::var("OCIO_RS_S1_SWEEP")
        .ok()
        .map(|v| v.parse().expect("OCIO_RS_S1_SWEEP is a count"))
        .unwrap_or(20_000)
}

/// An odd `q` with `q * 5^f` of `digits` digits, and `q < 2^mantissa`: `q / 2^f` is then
/// exactly halfway between two `digits - 1`-digit decimals.
fn tie(rng: &mut Rng, digits: u32, mantissa: u32, max_f: u32) -> Option<(u64, i32)> {
    let f = 1 + (rng.next_u64() % u64::from(max_f)) as u32;
    let five = 5u128.pow(f);
    let lo = 10u128.pow(digits - 1).div_ceil(five);
    let hi = ((10u128.pow(digits) - 1) / five).min((1u128 << mantissa) - 1);
    if lo > hi {
        return None;
    }
    let q = (lo + u128::from(rng.next_u64()) % (hi - lo + 1)) | 1;
    (q <= hi).then(|| (q as u64, -(f as i32)))
}

/// Random doubles: every bit pattern, decimals of 1 to 17 digits, exact ties at 15 digits
/// and their neighbors.
fn random_double(rng: &mut Rng, i: usize) -> u64 {
    match i % 4 {
        0 => rng.next_u64(),
        1 => {
            let digits = 1 + rng.next_u64() % 17;
            let mantissa = rng.next_u64() % 10u64.pow(digits as u32);
            let exp = (rng.next_u64() % 80) as i32 - 40;
            let sign = if rng.next_u64() & 1 == 1 { "-" } else { "" };
            format!("{sign}{mantissa}e{exp}")
                .parse::<f64>()
                .expect("a decimal")
                .to_bits()
        }
        _ => loop {
            if let Some((q, e)) = tie(rng, 16, 53, 22) {
                let bits = (q as f64 * 2f64.powi(e)).to_bits();
                // The tie itself, or one of its neighbors.
                break match rng.next_u64() % 3 {
                    0 => bits,
                    1 => bits + 1,
                    _ => bits - 1,
                };
            }
        },
    }
}

/// Random floats: every bit pattern, decimals of 1 to 9 digits, exact ties at 7 digits and
/// their neighbors.
fn random_float(rng: &mut Rng, i: usize) -> u32 {
    match i % 4 {
        0 => (rng.next_u64() >> 32) as u32,
        1 => {
            let digits = 1 + rng.next_u64() % 9;
            let mantissa = rng.next_u64() % 10u64.pow(digits as u32);
            let exp = (rng.next_u64() % 90) as i32 - 50;
            let sign = if rng.next_u64() & 1 == 1 { "-" } else { "" };
            format!("{sign}{mantissa}e{exp}")
                .parse::<f32>()
                .expect("a decimal")
                .to_bits()
        }
        _ => loop {
            if let Some((q, e)) = tie(rng, 8, 24, 10) {
                let bits = (q as f32 * 2f32.powi(e)).to_bits();
                break match rng.next_u64() % 3 {
                    0 => bits,
                    1 => bits + 1,
                    _ => bits - 1,
                };
            }
        },
    }
}

#[test]
fn random_numbers_serialize_like_the_wheel() {
    const BATCH: usize = 60_000;
    let total = sweep_count();
    let mut rng = Rng::new(0x51_2025);
    let mut done = 0;
    while done < total {
        let n = BATCH.min(total - done);
        let doubles: Vec<u64> = (0..n).map(|i| random_double(&mut rng, i)).collect();
        let floats: Vec<u32> = (0..n).map(|i| random_float(&mut rng, i)).collect();
        let luma: Vec<u64> = (0..3).map(|i| random_double(&mut rng, i)).collect();
        // 20 doubles per matrix and 3 floats per allocation transform; the last ones are
        // padded with 0.5.
        let mut padded = doubles.clone();
        padded.resize(n.div_ceil(20) * 20, 0.5f64.to_bits());
        let matrices: Vec<&[u64]> = padded.chunks(20).collect();
        let mut vars = floats.clone();
        vars.resize(n.div_ceil(3) * 3, 0.5f32.to_bits());
        let allocations: Vec<&[u32]> = vars.chunks(3).collect();
        let spec = json!({
            "luma_bits": luma,
            "colorspaces": [
                {"name": "doubles", "matrices_bits": matrices},
                {"name": "floats", "allocations_bits": allocations},
            ],
        });
        let response = Oracle::get().call("serialize_built_config", json!({ "spec": spec }), &[]);
        assert!(
            response.result.get("exception").is_none(),
            "{:?}",
            response.result
        );
        check(
            &format!("random numbers {done}..{}", done + n),
            &spec,
            response.blob_text(0),
        );
        done += n;
    }
}
