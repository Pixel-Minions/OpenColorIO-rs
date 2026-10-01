// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `CDLOpData` against the wheel, through the `CDLTransform` that holds one, with the oracle's
//! `transform_text`:
//! - `validate`: the transform's `validate` runs the data's and prefixes its message with
//!   `CDLTransform validation failed: ` (src/OpenColorIO/transforms/CDLTransform.cpp:142-155
//!   @ v2.5.2). The binding's constructor validates (src/bindings/python/transforms/
//!   PyCDLTransform.cpp:37-62), so a refusal comes from building the transform;
//! - `equals`: the transform compares its data with `CDLOpData::operator==`, the slope, offset
//!   and power within 1e-9 (CDLTransform.cpp:170-175).
//!
//! The parameters are finite here (JSON). NaN and infinite ones reach the wheel through a
//! config's YAML in the battery (`tests/cdl_oracle.rs`); the cache ID, the identity and
//! simpler replacements and `isInverse` through the optimizer and the CPU processor
//! (`tests/cdl_op_oracle.rs`).

mod common;

use common::cdl::Cdl;
use ocio_ops::open_color_types::{CdlStyle, TransformDirection};
use ocio_testkit::probe::Rng;
use ocio_testkit::transform_text::{Built, TransformTextRequest};

const DIRECTIONS: [TransformDirection; 2] =
    [TransformDirection::Forward, TransformDirection::Inverse];

/// The CDL of upstream's tests (tests/cpu/ops/cdl/CDLOpData_tests.cpp:14-19 @ v2.5.2).
const BASE: Cdl = Cdl {
    slope: [1.35, 1.1, 0.71],
    offset: [0.05, -0.23, 0.11],
    power: [0.93, 0.81, 1.27],
    sat: 1.23,
    style: CdlStyle::Asc,
};

/// Random values over many magnitudes, of both signs.
fn random_values(seed: u64, n: usize) -> Vec<f64> {
    let mut rng = Rng::new(seed);
    (0..n)
        .map(|i| {
            let unit = (rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
            let exp = (rng.next_u64() % 80) as i32 - 40;
            let v = (1.0 + unit) * 2f64.powi(exp);
            if i % 3 == 2 { -v } else { v }
        })
        .collect()
}

/// The CDLs whose validation the test compares: each bound's value, zeros of both signs, the
/// smallest values on both sides, the extremes, random magnitudes, in each channel; and
/// several bad values at once (the slope's red is reported first, then the power, then the
/// saturation).
fn validation_cases() -> Vec<(Cdl, TransformDirection)> {
    let mut values = vec![
        0.0,
        -0.0,
        f64::from_bits(1),
        -f64::from_bits(1),
        1e-300,
        -1e-300,
        // tests/cpu/ops/cdl/CDLOpData_tests.cpp:363-398 @ v2.5.2.
        -0.9,
        -1.2,
        -1.17,
        f64::MAX,
        -f64::MAX,
        0.000123456789,
        -123456789.0,
    ];
    values.extend(random_values(0x4344_4c00, 30));
    let mut out = Vec::new();
    for &v in &values {
        for c in 0..3 {
            let mut cdl = BASE;
            cdl.slope[c] = v;
            out.push((cdl, TransformDirection::Forward));
            let mut cdl = BASE;
            cdl.power[c] = v;
            out.push((cdl, TransformDirection::Inverse));
            let mut cdl = BASE;
            cdl.offset[c] = v;
            out.push((cdl, TransformDirection::Forward));
        }
        let mut cdl = BASE;
        cdl.sat = v;
        cdl.style = CdlStyle::NoClamp;
        out.push((cdl, TransformDirection::Forward));
    }
    let mut cdl = BASE;
    cdl.slope = [1.0, -1.0, -2.0];
    cdl.power = [-1.0, 1.0, 1.0];
    cdl.sat = -1.0;
    out.push((cdl, TransformDirection::Forward));
    cdl.slope = [1.0, 1.0, 1.0];
    out.push((cdl, TransformDirection::Inverse));
    cdl.power = [1.0, 1.0, 1.0];
    out.push((cdl, TransformDirection::Forward));
    out
}

#[test]
fn validation_matches_the_wheel() {
    let cases = validation_cases();
    let reply = TransformTextRequest {
        transforms: cases.iter().map(|(c, dir)| c.spec(*dir)).collect(),
        pairs: Vec::new(),
    }
    .run();

    let mut failures = Vec::new();
    let mut refused = 0;
    for ((cdl, dir), built) in cases.iter().zip(&reply.transforms) {
        let wheel = match built {
            Built::Raised(e) => Err(e.message.clone()),
            Built::Text(text) => match &text.validate {
                Some(e) => Err(e.message.clone()),
                None => Ok(()),
            },
        };
        refused += usize::from(wheel.is_err());
        let port = cdl
            .op_data(*dir)
            .validate()
            .map_err(|e| format!("CDLTransform validation failed: {}", e.message()));
        if port != wheel {
            failures.push(format!(
                "{cdl:?} {dir:?}\n  wheel {wheel:?}\n  port  {port:?}"
            ));
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
    // Values at, inside and past the 1e-9 tolerance of each parameter, an exact saturation,
    // and every style and direction.
    let mut cdls = vec![BASE, BASE];
    for delta in [1e-9, 0.9e-9, 1.1e-9, 1e-10, 1e-8, -1e-9, -2e-9] {
        let mut cdl = BASE;
        cdl.slope[0] += delta;
        cdls.push(cdl);
        let mut cdl = BASE;
        cdl.offset[1] += delta;
        cdls.push(cdl);
        let mut cdl = BASE;
        cdl.power[2] += delta;
        cdls.push(cdl);
    }
    let mut cdl = BASE;
    cdl.offset = [0.0, -0.0, 1e-10];
    cdls.push(cdl);
    let mut cdl = BASE;
    cdl.offset = [-0.0, 0.0, -1e-10];
    cdls.push(cdl);
    let mut cdl = BASE;
    cdl.sat = f64::from_bits(BASE.sat.to_bits() + 1);
    cdls.push(cdl);
    let mut cdl = BASE;
    cdl.style = CdlStyle::NoClamp;
    cdls.push(cdl);

    let mut transforms = Vec::new();
    for cdl in &cdls {
        for dir in DIRECTIONS {
            transforms.push((*cdl, dir));
        }
    }
    let mut pairs = Vec::new();
    for i in 0..transforms.len() {
        for j in 0..transforms.len() {
            pairs.push((i, j));
        }
    }
    let reply = TransformTextRequest {
        transforms: transforms.iter().map(|(c, dir)| c.spec(*dir)).collect(),
        pairs: pairs.clone(),
    }
    .run();

    let mut failures = Vec::new();
    let mut equal = 0;
    for ((i, j), wheel) in pairs.iter().zip(&reply.pairs) {
        let (a, b) = (&transforms[*i], &transforms[*j]);
        let wheel = wheel.unwrap_or_else(|| panic!("no equals for {a:?} {b:?}"));
        equal += usize::from(wheel);
        let (da, db) = (a.0.op_data(a.1), b.0.op_data(b.1));
        if da.equals(&db) != wheel || (da == db) != wheel {
            failures.push(format!("{a:?} == {b:?}: wheel {wheel}"));
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
