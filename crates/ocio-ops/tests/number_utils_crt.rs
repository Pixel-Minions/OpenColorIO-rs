// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `ocio_ops::utils::number_utils` against the platform C runtime through
//! `ocio_testkit::crt`. Expected values come from the C runtime, and from the wheel where the
//! C runtime is not what OCIO calls.
//!
//! - **Linux**: the `strtod` branch is glibc's `strtod_l`/`strtof_l`/`strtol_l` in full
//!   (value bits including NaN payloads, end offset, `ERANGE`) over well-formed numbers
//!   (every double and float as `%.17g`/`%.9g`/`%a` write them, long mantissas, exact
//!   midpoints between neighbouring values and the decimals just around them, boundaries)
//!   and over random strings of the characters the grammar looks at; the NumberUtils wrapper
//!   is checked around glibc's results for every `last`. The `from_chars` branch's values
//!   equal glibc's, which rounds correctly.
//! - **Windows**: the `from_chars` branch against UCRT `strtod`/`strtof` on value, end and
//!   range errors (MSVC's `from_chars` and UCRT both report underflow only for a zero
//!   result). The wheel runs MSVC's `from_chars`, not UCRT, and the two differ where UCRT
//!   rounds incorrectly: it drops digits past the 768th with no sticky bit
//!   (corecrt_internal_strtox.h `floating_point_string::_mantissa[768]`), and this UCRT
//!   (10.0.26100) rounds some exact `float` ties up. Every such difference must be settled
//!   by the wheel in the port's favour.
//!
//! `OCIO_RS_PARSE_SWEEP=<n>` sets the size of each generated group (default 50,000; the
//! Phase 0 evidence runs use 10,000,000 in release mode).

mod common;

use common::numbers::{batches, numeric_strings, sweep_count};
#[cfg(target_os = "linux")]
use ocio_ops::utils::number_utils::glibc;
use ocio_ops::utils::number_utils::{Errc, Flavor, from_chars_f32, from_chars_f64};
use ocio_testkit::crt;

/// Random strings over the characters strtod's grammar looks at.
#[cfg(target_os = "linux")]
fn fuzz_strings(n: usize, seed: u64) -> Vec<Vec<u8>> {
    const ALPHABET: &[u8] = b"0123456789..eEpPxX++--  \t\n\x0b\x0c\rinfINFityITYnaNA()_bcdBCDzZ0x";
    let mut rng = ocio_testkit::probe::Rng::new(seed);
    (0..n)
        .map(|_| {
            let len = (rng.next_u64() % 14) as usize;
            (0..len)
                .map(|_| ALPHABET[(rng.next_u64() % ALPHABET.len() as u64) as usize])
                .collect()
        })
        .collect()
}

/// NaN payload spellings, including ones glibc's `strtoull` overflows on.
#[cfg(target_os = "linux")]
fn nan_strings() -> Vec<Vec<u8>> {
    let mut v = Vec::new();
    for seq in [
        "",
        "0",
        "1",
        "123",
        "0x123",
        "0X7fffffffffffffff",
        "0xffffffffffffffff",
        "99999999999999999999",
        "0777",
        "08",
        "0x",
        "abc",
        "_",
        "1_",
        "ind",
        "snan",
        "18446744073709551615",
        "18446744073709551616",
        "0x1ffffffffffff",
        "0x3fffff",
    ] {
        for prefix in ["nan", "NaN", "-nan", "+NAN", " nan"] {
            v.push(format!("{prefix}({seq})").into_bytes());
            v.push(format!("{prefix}({seq}").into_bytes());
        }
    }
    v
}

#[cfg(target_os = "linux")]
#[test]
fn values_of_both_branches_match_glibc() {
    for (batch, size) in batches(sweep_count()) {
        for text in numeric_strings(size, 0x9a55 + (batch << 32)) {
            check_values_against_glibc(&text);
        }
    }
}

#[cfg(target_os = "linux")]
fn check_values_against_glibc(text: &str) {
    {
        let bytes = text.as_bytes();
        let d = crt::strtod_l(bytes);
        let f = crt::strtof_l(bytes);
        let (gd, _, _) = glibc::strtod(bytes);
        let (gf, _, _) = glibc::strtof(bytes);
        assert_eq!(gd.to_bits(), d.value.to_bits(), "strtod {text}");
        assert_eq!(gf.to_bits(), f.value.to_bits(), "strtof {text}");
        // The from_chars branch has no sign before "0x": compare it on unsigned input.
        if !text.starts_with(['-', '+']) {
            let mut v = 0.0f64;
            let r = from_chars_f64(Flavor::FromChars, bytes, bytes.len(), &mut v);
            assert_eq!(v.to_bits(), d.value.to_bits(), "from_chars {text}");
            assert_eq!(r.ptr, d.end, "from_chars ptr {text}");
            let mut w = 0.0f32;
            from_chars_f32(Flavor::FromChars, bytes, bytes.len(), &mut w);
            assert_eq!(w.to_bits(), f.value.to_bits(), "from_chars f32 {text}");
        }
    }
}

#[cfg(target_os = "linux")]
#[test]
fn strtod_branch_matches_glibc() {
    for text in nan_strings() {
        check_strtod_branch(&text);
    }
    for (batch, size) in batches(sweep_count()) {
        for text in numeric_strings(size / 5, 0x61bc + (batch << 32)) {
            check_strtod_branch(text.as_bytes());
        }
        for text in fuzz_strings(size * 4, 0xf022 + (batch << 32)) {
            check_strtod_branch(&text);
        }
    }
}

#[cfg(target_os = "linux")]
fn check_strtod_branch(text: &[u8]) {
    {
        let shown = String::from_utf8_lossy(text);
        let d = crt::strtod_l(text);
        let (gd, gd_end, gd_erange) = glibc::strtod(text);
        assert_eq!(gd.to_bits(), d.value.to_bits(), "strtod {shown:?}");
        assert_eq!(gd_end, d.end, "strtod end {shown:?}");
        assert_eq!(gd_erange, d.errno == crt::ERANGE, "strtod errno {shown:?}");

        let f = crt::strtof_l(text);
        let (gf, gf_end, gf_erange) = glibc::strtof(text);
        assert_eq!(gf.to_bits(), f.value.to_bits(), "strtof {shown:?}");
        assert_eq!(gf_end, f.end, "strtof end {shown:?}");
        assert_eq!(gf_erange, f.errno == crt::ERANGE, "strtof errno {shown:?}");

        let l = crt::strtol_l(text, 0);
        let (gl, gl_end, gl_erange) = glibc::strtol_base0(text);
        assert_eq!(gl, l.value, "strtol {shown:?}");
        assert_eq!(gl_end, l.end, "strtol end {shown:?}");
        assert_eq!(gl_erange, l.errno == crt::ERANGE, "strtol errno {shown:?}");

        // The NumberUtils wrapper (NumberUtils.h:121-149, 168-205) around the C library's
        // results, for `last` at the start, around both end offsets and at the end.
        let mut lasts: Vec<usize> = [1, 2, 3, text.len()]
            .into_iter()
            .chain(
                [d.end, f.end]
                    .into_iter()
                    .flat_map(|e| [e.saturating_sub(1), e, e + 1]),
            )
            .filter(|&l| (1..=text.len()).contains(&l))
            .collect();
        lasts.sort_unstable();
        lasts.dedup();
        for last in lasts {
            let expect = |end: usize, errno: i32| {
                if errno != 0 {
                    (end, Errc::ResultOutOfRange, false)
                } else if end == 0 {
                    (0, Errc::InvalidArgument, false)
                } else if end <= last {
                    (end, Errc::Ok, true)
                } else {
                    (0, Errc::ArgumentOutOfDomain, false)
                }
            };
            let mut v = f64::from_bits(0x1234);
            let r = from_chars_f64(Flavor::Strtod, text, last, &mut v);
            let (ptr, ec, stored) = expect(d.end, d.errno);
            assert_eq!((r.ptr, r.ec), (ptr, ec), "f64 {shown:?} last {last}");
            let bits = if stored { d.value.to_bits() } else { 0x1234 };
            assert_eq!(v.to_bits(), bits, "f64 {shown:?} last {last}");

            let mut w = f32::from_bits(0x1234);
            let r = from_chars_f32(Flavor::Strtod, text, last, &mut w);
            let (ptr, ec, stored) = expect(f.end, f.errno);
            assert_eq!((r.ptr, r.ec), (ptr, ec), "f32 {shown:?} last {last}");
            let bits = if stored { f.value.to_bits() } else { 0x1234 };
            assert_eq!(w.to_bits(), bits, "f32 {shown:?} last {last}");
        }
    }
}

/// Asks the wheel about inputs where UCRT and the port disagree. Doubles go through a CLF
/// Matrix (ParseNumber<double>), floats through a .spi1d LUT; each comes back as
/// `Some(bits)`, or `None` when the reader rejected it.
#[cfg(windows)]
fn wheel_values(doubles: &[String], floats: &[String]) -> (Vec<Option<u64>>, Vec<Option<u64>>) {
    use ocio_testkit::Oracle;
    use serde_json::json;
    let pick = |r: &serde_json::Value| r["bits"][0].as_u64();
    let d = if doubles.is_empty() {
        Vec::new()
    } else {
        let r = Oracle::get().call(
            "clf_matrix_values",
            json!({"tokens": doubles, "separately": true}),
            &[],
        );
        r.result
            .as_array()
            .expect("a list")
            .iter()
            .map(pick)
            .collect()
    };
    let f = if floats.is_empty() {
        Vec::new()
    } else {
        let r = Oracle::get().call(
            "spi1d_values",
            json!({"tokens": floats, "separately": true}),
            &[],
        );
        r.result
            .as_array()
            .expect("a list")
            .iter()
            .map(pick)
            .collect()
    };
    (d, f)
}

#[cfg(windows)]
#[test]
fn from_chars_branch_matches_ucrt_or_the_wheel() {
    let mut disputed_doubles = Vec::new();
    let mut disputed_floats = Vec::new();
    let mut compared = 0usize;
    for (batch, size) in batches(sweep_count()) {
        let inputs = numeric_strings(size, 0x3cb7 + (batch << 32));
        for text in inputs.iter().filter(|t| !t.starts_with(['-', '+'])) {
            compared += 1;
            let bytes = text.as_bytes();
            let d = crt::strtod_c(bytes);
            let mut v = 0.0f64;
            let r = from_chars_f64(Flavor::FromChars, bytes, bytes.len(), &mut v);
            assert_eq!(r.ptr, d.end, "{text}");
            assert_eq!(
                r.ec == Errc::ResultOutOfRange,
                d.errno == crt::ERANGE,
                "{text}"
            );
            if v.to_bits() != d.value.to_bits() {
                disputed_doubles.push((text.clone(), v.to_bits()));
            }

            let f = crt::strtof_c(bytes);
            let mut w = 0.0f32;
            let r = from_chars_f32(Flavor::FromChars, bytes, bytes.len(), &mut w);
            assert_eq!(
                r.ec == Errc::ResultOutOfRange,
                f.errno == crt::ERANGE,
                "f32 {text}"
            );
            if w.to_bits() != f.value.to_bits() {
                assert!(
                    text.len() <= 63,
                    "UCRT strtof disagrees on {text:?}, too long for the wheel's .spi1d reader"
                );
                disputed_floats.push((text.clone(), u64::from(w.to_bits())));
            }
        }
    }
    let tokens = |v: &[(String, u64)]| v.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>();
    let (wheel_d, wheel_f) = wheel_values(&tokens(&disputed_doubles), &tokens(&disputed_floats));
    for ((text, port), wheel) in disputed_doubles.iter().zip(&wheel_d) {
        assert_eq!(
            Some(*port),
            *wheel,
            "double {text}: UCRT differs; the wheel decides"
        );
    }
    for ((text, port), wheel) in disputed_floats.iter().zip(&wheel_f) {
        assert_eq!(
            Some(*port),
            *wheel,
            "float {text}: UCRT differs; the wheel decides"
        );
    }
    eprintln!(
        "compared {compared} inputs with UCRT; the wheel settled {} double and {} float differences",
        disputed_doubles.len(),
        disputed_floats.len()
    );
}
