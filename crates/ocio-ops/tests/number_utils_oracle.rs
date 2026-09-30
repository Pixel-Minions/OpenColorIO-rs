// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! NumberUtils against the wheel on this platform, through the two readers that call it
//! most directly (oracle commands `spi1d_values` and `clf_matrix_values`):
//! - `.spi1d` (FileFormatSpi1D.cpp:220-262): `from_chars(float)` on a `sscanf("%63s")` token
//!   in a zero-filled 64-byte buffer with `last` = buffer + 64; any error fails the file;
//! - CLF `<Array>` (XMLReaderUtils.h `ParseNumber<double>`): `from_chars(double)` on the
//!   token, followed in memory by the rest of the text, with `double val = 0.0`; only
//!   `invalid_argument` and a short parse are errors, and the value is stored regardless.
//!
//! The port runs the branch of this platform ([`Flavor::NATIVE`]): the tokens below parse
//! differently on Windows and on Linux, and each platform's wheel must agree with the port.

mod common;

use common::numbers::{midpoint_strings, numeric_strings, sweep_count};
use ocio_ops::utils::number_utils::{Errc, Flavor, from_chars_f32, from_chars_f64};
use ocio_testkit::Oracle;
use ocio_testkit::probe::Rng;
use serde_json::{Value, json};

/// Signs and prefixes, hex, partial parses, ranges, NaN and infinity spellings.
const EDGE_TOKENS: &[&str] = &[
    "1.5",
    "-0x10",
    "0x1p3",
    "0x-5",
    "+-5",
    "-+5",
    "++5",
    "+5",
    "+.5",
    "0x",
    "0X",
    "0x.",
    "0x.8",
    "0xp1",
    "0x1p",
    "0x1p+",
    "0x1.8p-1",
    "0x1P-1074",
    "0x1p-149",
    "0x1p-150",
    "0x1.fffffep127",
    "0x1.ffffffp127",
    "0x1.fffffffffffff8p1023",
    "0x1.fffffffffffffp1023",
    "-0x1p3",
    "+0x1p3",
    "1e-40",
    "1e-45",
    "1e-46",
    "1.4e-45",
    "7e-46",
    "7.1e-46",
    "1e39",
    "3.4028235e38",
    "3.4028236e38",
    "1.17549435e-38",
    "1.1754942e-38",
    "1e-310",
    "1e-400",
    "1e999",
    "-1e999",
    "4.9e-324",
    "2.4703282292062327e-324",
    "2.4703282292062328e-324",
    "2.2250738585072011e-308",
    "2.2250738585072012e-308",
    "1.7976931348623157e308",
    "1.7976931348623159e308",
    "nan",
    "-nan",
    "NaN",
    "nan(123)",
    "nan(0x7b)",
    "nan(ind)",
    "-nan(ind)",
    "nan(snan)",
    "nan(SNAN)",
    "nan()",
    "nan(",
    "nan(1",
    "nan(99999999999999999999)",
    "nan(0x3fffff)",
    "inf",
    "-inf",
    "infinity",
    "INFINITY",
    "infinit",
    "infx",
    "iNf",
    "1e",
    "1e+",
    "1e-",
    "1.e5",
    ".5",
    "5.",
    ".",
    "-.",
    "e5",
    "1.5x",
    "12abc",
    "0e999999999999",
    "1e-99999999999",
    "1e99999999999",
    "00012",
    "-0",
    "0x0",
    "0x0p0",
    "_1",
    "1_",
    "0x1_",
    "1e5_",
    "0.000000000000000000000000000000000000000000001",
    "123456789012345678901234567890",
    "0x123456789abcdef0123456789",
    "39315.267578125",
    "0.39315267578125e5",
    "-",
    "+",
    "x",
];

/// The `.spi1d` reader's call: `Some(bits)` when the file loads, `None` when it fails.
fn spi1d_model(token: &str) -> Option<u64> {
    let mut buf = [0u8; 64];
    buf[..token.len()].copy_from_slice(token.as_bytes());
    let mut v = f32::NAN;
    let r = from_chars_f32(Flavor::NATIVE, &buf, buf.len(), &mut v);
    (r.ec == Errc::Ok).then(|| u64::from(v.to_bits()))
}

/// `ParseNumber<double>` on a token followed by a space: `Some(bits)` or `None` for an error.
fn clf_model(token: &str) -> Option<u64> {
    let text = format!("{token} 0");
    let mut val = 0.0f64;
    let r = from_chars_f64(Flavor::NATIVE, text.as_bytes(), token.len(), &mut val);
    if r.ec == Errc::InvalidArgument || r.ptr != token.len() {
        None
    } else {
        Some(val.to_bits())
    }
}

fn first_bits(result: &Value) -> Option<u64> {
    result["bits"][0].as_u64()
}

#[test]
fn edge_tokens_parse_like_the_wheel() {
    let spi1d = Oracle::get().call(
        "spi1d_values",
        json!({"tokens": EDGE_TOKENS, "separately": true}),
        &[],
    );
    let clf = Oracle::get().call(
        "clf_matrix_values",
        json!({"tokens": EDGE_TOKENS, "separately": true}),
        &[],
    );
    let spi1d = spi1d.result.as_array().expect("a list");
    let clf = clf.result.as_array().expect("a list");
    for (i, token) in EDGE_TOKENS.iter().enumerate() {
        assert_eq!(
            spi1d_model(token),
            first_bits(&spi1d[i]),
            ".spi1d {token:?}: wheel {}",
            spi1d[i]
        );
        assert_eq!(
            clf_model(token),
            first_bits(&clf[i]),
            "CLF {token:?}: wheel {}",
            clf[i]
        );
    }
}

#[test]
fn random_numbers_parse_like_the_wheel() {
    // The wheel reads each batch as one file: keep it to a LUT and a CLF of sensible size.
    let n = sweep_count().min(200_000);
    let mut rng = Rng::new(0x0dd5);

    // Floats the .spi1d reader accepts on both platforms: normal, finite, at most 63
    // characters (glibc reports ERANGE for subnormal floats, which fails the file).
    let mut floats = Vec::new();
    while floats.len() < n / 2 {
        let f = f32::from_bits(rng.next_u64() as u32);
        if !f.is_normal() {
            continue;
        }
        let x = f64::from(f);
        for format in ["%.9g", "%.12e", "%a", "%.6g"] {
            floats.push(ocio_testkit::crt::format_f64(format, x));
        }
        let next = f32::from_bits(f.to_bits() + 1);
        if next.is_normal() && f > 0.0 {
            floats.extend(midpoint_strings(x, f64::from(next)));
        }
    }
    floats.retain(|t| t.len() <= 63);
    let spi1d = Oracle::get().call(
        "spi1d_values",
        json!({"tokens": floats, "separately": false}),
        &[],
    );
    let wheel = spi1d.result[0]["bits"]
        .as_array()
        .unwrap_or_else(|| panic!("the .spi1d batch failed: {}", spi1d.result[0]));
    assert_eq!(wheel.len(), floats.len());
    for (token, bits) in floats.iter().zip(wheel) {
        assert_eq!(spi1d_model(token), bits.as_u64(), ".spi1d {token:?}");
    }

    // Doubles through CLF: everything but a '-' before "0x", which the from_chars branch
    // stops at (an error in ParseNumber, which would fail the whole file).
    let mut doubles: Vec<String> = numeric_strings(n, 0xc1f)
        .into_iter()
        .filter(|t| !t.starts_with("-0x"))
        .collect();
    while !doubles.len().is_multiple_of(9) {
        doubles.push("0".to_string());
    }
    let clf = Oracle::get().call(
        "clf_matrix_values",
        json!({"tokens": doubles, "separately": false}),
        &[],
    );
    let wheel = clf.result[0]["bits"]
        .as_array()
        .unwrap_or_else(|| panic!("the CLF batch failed: {}", clf.result[0]));
    assert_eq!(wheel.len(), doubles.len());
    for (token, bits) in doubles.iter().zip(wheel) {
        assert_eq!(clf_model(token), bits.as_u64(), "CLF {token:?}");
    }
}
