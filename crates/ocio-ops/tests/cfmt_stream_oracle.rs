// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `cfmt::OStringStream` against the real iostreams of the wheel on this platform: MSVC's
//! `num_put` over the UCRT on Windows, libstdc++'s over glibc on Linux.
//!
//! A transform's `repr()` is its C++ `operator<<` into a fresh `std::ostringstream`
//! (PyUtils.h `defRepr`; oracle command `stream_reprs`). MatrixTransform writes its doubles
//! at precision 16 (MatrixTransform.cpp:340-364), ExponentTransform its doubles at the
//! default 6 (ExponentTransform.cpp:98-115), AllocationTransform its floats, promoted to
//! double, at 6 (AllocationTransform.cpp:159-183). NaN and infinity spellings differ between
//! the platforms, and the port must give this platform's.

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_testkit::Oracle;
use ocio_testkit::probe::Rng;
use serde_json::{Value, json};

/// `OCIO_RS_STREAM_SWEEP=<n>`: doubles and floats per run (default 20,000 of each).
fn sweep_count() -> usize {
    std::env::var("OCIO_RS_STREAM_SWEEP")
        .ok()
        .map(|v| v.parse().expect("OCIO_RS_STREAM_SWEEP is a count"))
        .unwrap_or(20_000)
}

/// Port of `operator<<(std::ostream &, const MatrixTransform &)` (MatrixTransform.cpp:
/// 340-364) for a default transform with `values` = 16 matrix values then 4 offsets.
fn matrix_repr(values: &[f64]) -> String {
    let mut os = OStringStream::new(Crt::NATIVE);
    os.precision = 16;
    os.put_str("<MatrixTransform ");
    os.put_str("direction=forward");
    os.put_str(", fileindepth=unknown");
    os.put_str(", fileoutdepth=unknown");
    os.put_str(", matrix=[");
    os.put_f64(values[0]);
    for &v in &values[1..16] {
        os.put_str(", ");
        os.put_f64(v);
    }
    os.put_str("], offset=[");
    os.put_f64(values[16]);
    for &v in &values[17..20] {
        os.put_str(", ");
        os.put_f64(v);
    }
    os.put_str("]>");
    os.into_string()
}

/// Port of `operator<<(std::ostream &, const ExponentTransform &)` (ExponentTransform.cpp:
/// 98-115) for a default transform holding `values`.
fn exponent_repr(values: &[f64]) -> String {
    let mut os = OStringStream::new(Crt::NATIVE);
    os.put_str("<ExponentTransform ");
    os.put_str("direction=forward, ");
    os.put_str("value=[");
    os.put_f64(values[0]);
    for &v in &values[1..4] {
        os.put_str(", ");
        os.put_f64(v);
    }
    os.put_str("], style=clamp");
    os.put_str(">");
    os.into_string()
}

/// Port of `operator<<(std::ostream &, const AllocationTransform &)` (AllocationTransform.cpp:
/// 159-183) for an lg2 transform holding `vars`.
fn allocation_repr(vars: &[f32]) -> String {
    let mut os = OStringStream::new(Crt::NATIVE);
    os.put_str("<AllocationTransform ");
    os.put_str("direction=forward");
    os.put_str(", allocation=lg2, ");
    os.put_str("vars=");
    os.put_f32(vars[0]);
    for &v in &vars[1..] {
        os.put_str(" ");
        os.put_f32(v);
    }
    os.put_str(">");
    os.into_string()
}

/// Every NaN and infinity spelling, signed zeros, subnormals and extremes.
const SPECIAL_F64: &[u64] = &[
    0x7FF8_0000_0000_0000, // quiet NaN
    0xFFF8_0000_0000_0000, // negative quiet NaN: UCRT's "-nan(ind)"
    0x7FF0_0000_0000_0001, // signaling NaN: UCRT's "nan(snan)"
    0xFFF0_0000_0000_0001,
    0x7FF8_0000_0000_0123, // quiet NaNs with a payload
    0xFFF8_0000_0000_0123,
    0x7FFF_FFFF_FFFF_FFFF,
    0xFFFF_FFFF_FFFF_FFFF,
    0x7FF0_0000_0000_0000, // infinities
    0xFFF0_0000_0000_0000,
    0x0000_0000_0000_0000,
    0x8000_0000_0000_0000,
    0x0000_0000_0000_0001,
    0x000F_FFFF_FFFF_FFFF,
    0x0010_0000_0000_0000,
    0x7FEF_FFFF_FFFF_FFFF,
    0x8000_0000_0000_0001,
    0xFFEF_FFFF_FFFF_FFFF,
    0x3FF0_0000_0000_0000,
    0x3FB9_9999_9999_999A,
];

/// An odd `q` below `2^mantissa` with `q * 5^f` of `digits` digits: `q / 2^f` lies exactly
/// halfway between two decimals of `digits - 1` significant digits.
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

/// Random doubles: every bit pattern, decimals of 1 to 17 digits, exact ties at 16 and at
/// 6 significant digits, and their neighbors.
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
            let (digits, max_f) = if rng.next_u64() & 1 == 0 {
                (17, 23)
            } else {
                (7, 10)
            };
            if let Some((q, e)) = tie(rng, digits, 53, max_f) {
                let bits = (q as f64 * 2f64.powi(e)).to_bits();
                break match rng.next_u64() % 3 {
                    0 => bits,
                    1 => bits + 1,
                    _ => bits - 1,
                };
            }
        },
    }
}

/// Random floats: every bit pattern, decimals of 1 to 9 digits, exact ties at 6 significant
/// digits, and their neighbors.
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
            if let Some((q, e)) = tie(rng, 7, 24, 10) {
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

/// The float the binding stores for `bits`: Python holds floats as doubles, and the
/// round trip quiets a signaling NaN (payload kept).
fn through_python(bits: u32) -> u32 {
    let nan = bits & 0x7F80_0000 == 0x7F80_0000 && bits & 0x007F_FFFF != 0;
    if nan { bits | 0x0040_0000 } else { bits }
}

fn strings(result: &Value, key: &str) -> Vec<String> {
    result[key]
        .as_array()
        .unwrap_or_else(|| panic!("no {key} in {result}"))
        .iter()
        .map(|s| s.as_str().expect("a string").to_string())
        .collect()
}

fn check(label: &str, expected: &[String], actual: &[String]) {
    assert_eq!(expected.len(), actual.len(), "{label}: count");
    let mismatches: Vec<String> = expected
        .iter()
        .zip(actual)
        .filter(|(e, a)| e != a)
        .take(10)
        .map(|(e, a)| format!("  wheel {e}\n  port  {a}"))
        .collect();
    assert!(
        mismatches.is_empty(),
        "{label}: repr differs:\n{}",
        mismatches.join("\n")
    );
}

#[test]
fn transform_reprs_match_the_wheel() {
    const BATCH: usize = 60_000;
    let total = sweep_count();
    let mut rng = Rng::new(0x0515_7ea3);
    let mut done = 0;
    while done < total {
        let n = BATCH.min(total - done);
        let mut doubles: Vec<u64> = SPECIAL_F64.to_vec();
        doubles.extend((0..n).map(|i| random_double(&mut rng, i)));
        doubles.resize(doubles.len().div_ceil(20) * 20, 0.5f64.to_bits());
        let mut floats: Vec<u32> = SPECIAL_F64
            .iter()
            .map(|&b| (f64::from_bits(b) as f32).to_bits())
            .collect();
        floats.extend((0..n).map(|i| through_python(random_float(&mut rng, i))));
        floats.resize(floats.len().div_ceil(3) * 3, 0.5f32.to_bits());

        let response = Oracle::get().call(
            "stream_reprs",
            json!({ "doubles_bits": doubles, "floats_bits": floats }),
            &[],
        );
        let values: Vec<f64> = doubles.iter().map(|&b| f64::from_bits(b)).collect();
        let vars: Vec<f32> = floats.iter().map(|&b| f32::from_bits(b)).collect();
        let label = format!("{done}..{}", done + n);
        let matrices: Vec<String> = values.chunks(20).map(matrix_repr).collect();
        check(
            &format!("MatrixTransform {label}"),
            &strings(&response.result, "matrix"),
            &matrices,
        );
        let exponents: Vec<String> = values.chunks(4).map(exponent_repr).collect();
        check(
            &format!("ExponentTransform {label}"),
            &strings(&response.result, "exponent"),
            &exponents,
        );
        let allocations: Vec<String> = vars.chunks(3).map(allocation_repr).collect();
        check(
            &format!("AllocationTransform {label}"),
            &strings(&response.result, "allocation"),
            &allocations,
        );
        done += n;
    }
}
