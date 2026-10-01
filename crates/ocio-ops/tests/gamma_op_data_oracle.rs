// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `GammaOpData` against the wheel, through the transforms that hold one, `ExponentTransform`
//! (the basic styles) and `ExponentWithLinearTransform` (the moncurve styles), with the
//! oracle's `transform_text`:
//! - `validate`: the transforms' `validate` runs the data's and prefixes its message with
//!   `ExponentTransform validation failed: ` or `ExponentWithLinearTransform validation
//!   failed: ` (src/OpenColorIO/transforms/ExponentTransform.cpp:40-53,
//!   ExponentWithLinearTransform.cpp:60-73 @ v2.5.2). The bindings' constructors validate
//!   (src/bindings/python/transforms/PyExponentTransform.cpp:21-30,
//!   PyExponentWithLinearTransform.cpp:25-40), so a refusal comes from building the transform;
//! - `equals`: both transforms' `equals` compare their data with `GammaOpData::operator==`
//!   (ExponentTransform.cpp:65-69, ExponentWithLinearTransform.cpp:85-89).
//!
//! The parameters are finite here (JSON). Infinite and NaN ones reach the wheel through a
//! config's YAML in the battery (`tests/gamma_oracle.rs`); the cache ID, `compose`, `isInverse`
//! and the identity replacement through the optimizer and the CPU processor
//! (`tests/gamma_op_oracle.rs`).

mod common;

use common::gamma::{
    exponent_op, exponent_spec, exponent_with_linear_op, exponent_with_linear_spec,
};
use ocio_ops::open_color_types::{NegativeStyle, TransformDirection};
use ocio_ops::ops::gamma::gamma_op_data::GammaOpData;
use ocio_testkit::probe::Rng;
use ocio_testkit::transform_text::{Built, TransformTextRequest};
use serde_json::Value;

const DIRECTIONS: [TransformDirection; 2] =
    [TransformDirection::Forward, TransformDirection::Inverse];
const BASIC_STYLES: [NegativeStyle; 3] = [
    NegativeStyle::Clamp,
    NegativeStyle::Mirror,
    NegativeStyle::PassThru,
];
const MONCURVE_STYLES: [NegativeStyle; 2] = [NegativeStyle::Linear, NegativeStyle::Mirror];

/// One transform: its spec for the wheel, its data and message prefix for the port.
struct Transform {
    label: String,
    spec: Value,
    data: GammaOpData,
    prefix: &'static str,
}

fn exponent(value: [f64; 4], neg: NegativeStyle, dir: TransformDirection) -> Transform {
    Transform {
        label: format!("Exponent {value:?} {neg:?} {dir:?}"),
        spec: exponent_spec(value, neg, dir),
        data: exponent_op(value, neg, dir),
        prefix: "ExponentTransform validation failed: ",
    }
}

fn exponent_with_linear(
    gamma: [f64; 4],
    offset: [f64; 4],
    neg: NegativeStyle,
    dir: TransformDirection,
) -> Transform {
    Transform {
        label: format!("ExponentWithLinear {gamma:?} {offset:?} {neg:?} {dir:?}"),
        spec: exponent_with_linear_spec(gamma, offset, neg, dir),
        data: exponent_with_linear_op(gamma, offset, neg, dir),
        prefix: "ExponentWithLinearTransform validation failed: ",
    }
}

/// `value` with `v` in channel `c`.
fn with(mut value: [f64; 4], c: usize, v: f64) -> [f64; 4] {
    value[c] = v;
    value
}

/// Random values over many magnitudes, of both signs: they print with every exponent and
/// number of digits of `%g`.
fn random_values(seed: u64, n: usize) -> Vec<f64> {
    let mut rng = Rng::new(seed);
    (0..n)
        .map(|i| {
            let unit = (rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
            let exp = (rng.next_u64() % 80) as i32 - 40;
            let v = (1.0 + unit) * 2f64.powi(exp);
            if i % 5 == 4 { -v } else { v }
        })
        .collect()
}

/// The transforms whose validation the test compares: values at, inside and just outside each
/// bound, in each channel (the first channel out of bounds is reported), zeros of both signs,
/// the smallest and largest doubles, and random magnitudes.
fn validation_cases() -> Vec<Transform> {
    let mut out = Vec::new();

    // Basic styles: gamma in [0.01, 100].
    let mut basic = vec![
        0.01,
        100.0,
        0.009999999,
        0.00999999999999,
        100.00001,
        100.00000000001,
        // tests/cpu/ops/gamma/GammaOpData_tests.cpp:297-313 @ v2.5.2.
        0.006,
        110.,
        0.0,
        -0.0,
        -1.0,
        1e-300,
        f64::from_bits(1),
        1e300,
        f64::MAX,
        -f64::MAX,
        2.2,
        1.0 / 0.45,
        123456789.0,
        0.000123456789,
        1e-5,
        99.9999999,
        0.0012345649,
        0.00123456500001,
    ];
    basic.extend(random_values(0x4741_4d4d, 40));
    let valid = [2.2, 1.8, 2.6, 1.0];
    for &v in &basic {
        for c in 0..4 {
            out.push(exponent(
                with(valid, c, v),
                NegativeStyle::Clamp,
                TransformDirection::Forward,
            ));
        }
        for neg in BASIC_STYLES {
            for dir in DIRECTIONS {
                out.push(exponent(with(valid, 0, v), neg, dir));
            }
        }
    }
    // Several out of bounds: the first channel's is reported.
    out.push(exponent(
        [0.001, 200., 1., 1.],
        NegativeStyle::Clamp,
        TransformDirection::Forward,
    ));
    out.push(exponent(
        [2., 200., 0.001, 1.],
        NegativeStyle::Mirror,
        TransformDirection::Inverse,
    ));

    // Moncurve styles: gamma in [1, 10], offset in [0, 0.9].
    let mut gammas = vec![
        1.0,
        10.0,
        0.99999999,
        0.9999999999999,
        10.0000001,
        0.5,
        11.0,
        -0.0,
        2.4,
        1.0 / 0.45,
        1e-300,
        1e300,
    ];
    gammas.extend(random_values(0x4d4f_4e47, 20));
    let mut offsets = vec![
        0.0,
        -0.0,
        0.9,
        0.90000001,
        0.9000000000001,
        // tests/cpu/ops/gamma/GammaOpData_tests.cpp:317-347 @ v2.5.2.
        11.,
        -1e-6,
        1.0,
        0.055,
        f64::from_bits(1),
        -f64::from_bits(1),
        1e300,
    ];
    offsets.extend(random_values(0x4f46_4653, 20));
    let (gamma, offset) = ([2.4, 2.2, 3.0, 1.8], [0.055, 0.099, 0.16, 0.6]);
    for &g in &gammas {
        for c in 0..4 {
            out.push(exponent_with_linear(
                with(gamma, c, g),
                offset,
                NegativeStyle::Linear,
                TransformDirection::Forward,
            ));
        }
        for neg in MONCURVE_STYLES {
            for dir in DIRECTIONS {
                out.push(exponent_with_linear(with(gamma, 1, g), offset, neg, dir));
            }
        }
    }
    for &o in &offsets {
        for c in 0..4 {
            out.push(exponent_with_linear(
                gamma,
                with(offset, c, o),
                NegativeStyle::Mirror,
                TransformDirection::Inverse,
            ));
        }
    }
    // A channel's gamma is checked before its offset, and red before blue.
    for (g, o) in [
        ([0.5, 2., 2., 2.], [2., 0., 0., 0.]),
        ([2., 2., 0.5, 2.], [2., 0., 0., 0.]),
    ] {
        out.push(exponent_with_linear(
            g,
            o,
            NegativeStyle::Linear,
            TransformDirection::Forward,
        ));
    }
    out
}

#[test]
fn validation_matches_the_wheel() {
    let cases = validation_cases();
    let reply = TransformTextRequest {
        transforms: cases.iter().map(|t| t.spec.clone()).collect(),
        pairs: Vec::new(),
    }
    .run();

    let mut failures = Vec::new();
    let mut refused = 0;
    for (t, built) in cases.iter().zip(&reply.transforms) {
        let wheel = match built {
            Built::Raised(e) => Err(e.message.clone()),
            Built::Text(text) => match &text.validate {
                Some(e) => Err(e.message.clone()),
                None => Ok(()),
            },
        };
        refused += usize::from(wheel.is_err());
        let port = t
            .data
            .validate()
            .map_err(|e| format!("{}{}", t.prefix, e.message()));
        if port != wheel {
            failures.push(format!("{}\n  wheel {wheel:?}\n  port  {port:?}", t.label));
        }
    }
    println!("{} transforms, {refused} refused by the wheel", cases.len());
    assert!(
        failures.is_empty(),
        "{} of {} differ:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}

#[test]
fn equality_matches_the_wheel() {
    let next_up = |v: f64| f64::from_bits(v.to_bits() + 1);
    let mut basic = Vec::new();
    for value in [
        [2.2, 2.2, 2.2, 1.0],
        [2.2, 2.2, 2.2, 1.0],
        [2.2, 2.2, next_up(2.2), 1.0],
        [2.2, 2.2, 2.2, 1.5],
    ] {
        for neg in BASIC_STYLES {
            for dir in DIRECTIONS {
                basic.push(exponent(value, neg, dir));
            }
        }
    }
    let mut moncurve = Vec::new();
    for (gamma, offset) in [
        ([2.4; 4], [0.055, 0.055, 0.055, 0.0]),
        ([2.4; 4], [0.055, 0.055, 0.055, 0.0]),
        ([2.4; 4], [0.055, 0.055, 0.055, -0.0]),
        ([2.4, next_up(2.4), 2.4, 2.4], [0.055, 0.055, 0.055, 0.0]),
        ([2.4; 4], [0.055, 0.055, next_up(0.055), 0.0]),
    ] {
        for neg in MONCURVE_STYLES {
            for dir in DIRECTIONS {
                moncurve.push(exponent_with_linear(gamma, offset, neg, dir));
            }
        }
    }

    // Every pair within each class (the bindings' `equals` takes its own class only).
    let mut transforms = Vec::new();
    let mut pairs = Vec::new();
    for class in [basic, moncurve] {
        let base = transforms.len();
        for i in 0..class.len() {
            for j in 0..class.len() {
                pairs.push((base + i, base + j));
            }
        }
        transforms.extend(class);
    }
    let reply = TransformTextRequest {
        transforms: transforms.iter().map(|t| t.spec.clone()).collect(),
        pairs: pairs.clone(),
    }
    .run();

    let mut failures = Vec::new();
    let mut equal = 0;
    for ((i, j), wheel) in pairs.iter().zip(&reply.pairs) {
        let (a, b) = (&transforms[*i], &transforms[*j]);
        let wheel = wheel.unwrap_or_else(|| panic!("no equals for {} {}", a.label, b.label));
        equal += usize::from(wheel);
        let port = a.data.equals(&b.data);
        if port != wheel || (a.data == b.data) != wheel {
            failures.push(format!("{} == {}: wheel {wheel}", a.label, b.label));
        }
    }
    println!("{} pairs, {equal} equal", pairs.len());
    assert!(
        failures.is_empty(),
        "{} of {} pairs differ:\n{}",
        failures.len(),
        pairs.len(),
        failures.join("\n")
    );
}
