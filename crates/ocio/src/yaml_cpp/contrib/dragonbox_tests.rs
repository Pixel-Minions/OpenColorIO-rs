// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The dragonbox port against the platform's C runtime.
//!
//! yaml-cpp 0.9.0 vendors dragonbox without its tests, so these tests check the contract that
//! dragonbox's default policies define (`dragonbox.h`, the policies at 2302-2862; see the
//! module) and take every fact about a value from the C runtime (CLAUDE.md rule 2: the
//! platform is the reference for C runtime behaviour):
//!
//! - the exact decimal expansion of `x`, from `printf("%.Ne")` with enough digits (both CRTs
//!   print exact expansions; the test checks that the precision was enough);
//! - whether a decimal reads back as `x`, from `strtod` (binary64) or `strtof` (binary32),
//!   which round to nearest, ties to even: the interval `nearest_to_even` describes.
//!
//! For each length `L` from 1, the candidates are the `L`-digit decimals just below and just
//! above `x` (from its exact expansion). The expected result is the first length with a
//! candidate that reads back as `x`; of two such candidates, the closer to `x`, and of two
//! equally close, the one with the even significand (`to_even`). Trailing zeros are removed
//! (`trailing_zero::remove`), and the sign is `x`'s (`sign::return_sign`).
//!
//! The inputs cover every binary exponent of both formats (so every cache entry the algorithm
//! can reach), with the significands at the edges of each binade, the shorter-interval case
//! (a zero significand) and random ones, plus random bit patterns. `OCIO_RS_TIER=full` adds
//! more random values.

use super::*;
use ocio_testkit::crt;

/// A positive decimal `digits * 10^exp`, with ASCII digits, no leading zero and no trailing
/// zero.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Decimal {
    digits: Vec<u8>,
    exp: i32,
}

impl Decimal {
    /// `digits * 10^exp` with trailing zeros removed.
    fn new(mut digits: Vec<u8>, mut exp: i32) -> Self {
        while digits.len() > 1 && digits.last() == Some(&b'0') {
            digits.pop();
            exp += 1;
        }
        assert!(!digits.is_empty() && digits[0] != b'0', "{digits:?}");
        Self { digits, exp }
    }

    /// `"<digits>e<exp>"`, as `strtod` reads it.
    fn text(&self) -> String {
        format!("{}e{}", String::from_utf8_lossy(&self.digits), self.exp)
    }

    /// `digits + 1` (the next decimal of the same length, before trailing zeros are removed),
    /// times `10^exp`, normalized.
    fn next_up(digits: &[u8], exp: i32) -> Self {
        let mut digits = digits.to_vec();
        let mut i = digits.len();
        loop {
            if i == 0 {
                digits.insert(0, b'1');
                break;
            }
            i -= 1;
            if digits[i] == b'9' {
                digits[i] = b'0';
            } else {
                digits[i] += 1;
                break;
            }
        }
        Self::new(digits, exp)
    }
}

/// Where `x` lies between the two candidates of a length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Position {
    /// Closer to the lower candidate.
    Below,
    /// Exactly halfway.
    Tie,
    /// Closer to the upper candidate.
    Above,
}

/// The exact decimal expansion of `x` (positive, finite), from the C runtime's `printf`.
fn exact_expansion(x: f64, precision: usize) -> Decimal {
    let text = crt::format_f64(&format!("%.{precision}e"), x);
    let (mantissa, exponent) = text.split_once('e').expect("an exponent");
    let exponent: i32 = exponent.parse().expect("a decimal exponent");
    let digits: Vec<u8> = mantissa.bytes().filter(|&b| b != b'.').collect();
    assert_eq!(digits.len(), precision + 1, "{text}");
    // The expansion is exact only if it ends with zeros: more digits would be needed otherwise.
    assert!(
        digits[digits.len() - 8..].iter().all(|&b| b == b'0'),
        "precision {precision} is too small for {text}"
    );
    Decimal::new(digits, exponent - precision as i32)
}

/// The `len`-digit decimals just below and just above `exact` (equal when `exact` has at most
/// `len` digits), where `exact` lies between them, and whether the lower one's `len`-digit
/// significand is even.
fn candidates(exact: &Decimal, len: usize) -> (Decimal, Decimal, Position, bool) {
    if exact.digits.len() <= len {
        return (exact.clone(), exact.clone(), Position::Below, true);
    }
    let cut = exact.digits.len() - len;
    let head = &exact.digits[..len];
    let lower_is_even = (head[len - 1] - b'0').is_multiple_of(2);
    let lower = Decimal::new(head.to_vec(), exact.exp + cut as i32);
    let rest = &exact.digits[len..];
    // `rest` has no trailing zero; compare `0.rest` with one half.
    let position = match (rest[0], rest.len()) {
        (b'5', 1) => Position::Tie,
        (b'5', _) => Position::Above,
        (d, _) if d > b'5' => Position::Above,
        _ => Position::Below,
    };
    let upper = Decimal::next_up(head, exact.exp + cut as i32);
    (lower, upper, position, lower_is_even)
}

/// The decimal dragonbox's default policies define for `x` (positive, finite), given the
/// exact expansion and the C runtime's reading back.
fn expected(exact: &Decimal, reads_back: &dyn Fn(&Decimal) -> bool, max_len: usize) -> Decimal {
    for len in 1..=max_len {
        let (lower, upper, position, lower_is_even) = candidates(exact, len);
        let result = match (reads_back(&lower), reads_back(&upper)) {
            (false, false) => continue,
            (true, false) => lower,
            (false, true) => upper,
            (true, true) => match position {
                Position::Below => lower,
                Position::Above => upper,
                Position::Tie if lower_is_even => lower,
                Position::Tie => upper,
            },
        };
        return result;
    }
    panic!("no decimal of up to {max_len} digits reads back as {exact:?}");
}

/// What the port returned, as a decimal.
fn port_decimal<T: std::fmt::Display>(significand: T, exponent: i32) -> Decimal {
    let digits = significand.to_string().into_bytes();
    Decimal {
        digits,
        exp: exponent,
    }
}

/// A fixed-seed xorshift64* generator, for the random inputs.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
}

/// How many random bit patterns to add for this tier.
fn random_count(quick: usize) -> usize {
    match std::env::var("OCIO_RS_TIER").as_deref() {
        Ok("full") | Ok("exhaustive") => quick * 50,
        _ => quick,
    }
}

/// Writes the mismatches to a file and fails, or does nothing when there are none.
fn report(name: &str, checked: usize, mismatches: &[String]) {
    if mismatches.is_empty() {
        return;
    }
    let path = std::env::temp_dir().join(format!("ocio-rs-{name}-mismatches.txt"));
    std::fs::write(&path, mismatches.join("\n")).expect("write the mismatches");
    panic!(
        "{name}: {} of {checked} values differ from the C runtime's contract (all in {}); first: {}",
        mismatches.len(),
        path.display(),
        mismatches[..mismatches.len().min(10)].join("; ")
    );
}

/// The binary64 inputs: every exponent with edge, zero (shorter interval) and random
/// significands, both signs, and random bit patterns.
fn binary64_inputs() -> Vec<u64> {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mantissa_mask = (1u64 << 52) - 1;
    let mut inputs = Vec::new();
    for exponent_bits in 0..=2046u64 {
        let mut significands = vec![
            1,
            2,
            3,
            1 << 51,
            mantissa_mask,
            mantissa_mask - 1,
            rng.next() & mantissa_mask,
            rng.next() & mantissa_mask,
        ];
        if exponent_bits != 0 {
            significands.push(0);
        }
        for significand in significands {
            let sign = (exponent_bits & 1) << 63;
            inputs.push(sign | (exponent_bits << 52) | significand);
        }
    }
    for _ in 0..random_count(20_000) {
        let bits = rng.next();
        let value = f64::from_bits(bits);
        if value.is_finite() && value != 0.0 {
            inputs.push(bits);
        }
    }
    inputs
}

/// The binary32 inputs: every exponent with edge, zero (shorter interval) and random
/// significands, both signs, and random bit patterns.
fn binary32_inputs() -> Vec<u32> {
    let mut rng = Rng(0xd1b5_4a32_d192_ed03);
    let mantissa_mask = (1u32 << 23) - 1;
    let mut inputs = Vec::new();
    for exponent_bits in 0..=254u32 {
        let mut significands = vec![1, 2, 3, 1 << 22, mantissa_mask, mantissa_mask - 1];
        for _ in 0..8 {
            significands.push(rng.next() as u32 & mantissa_mask);
        }
        if exponent_bits != 0 {
            significands.push(0);
        }
        for significand in significands {
            let sign = (exponent_bits & 1) << 31;
            inputs.push(sign | (exponent_bits << 23) | significand);
        }
    }
    for _ in 0..random_count(50_000) {
        let bits = rng.next() as u32;
        let value = f32::from_bits(bits);
        if value.is_finite() && value != 0.0 {
            inputs.push(bits);
        }
    }
    inputs
}

#[test]
fn binary64_is_the_shortest_closest_decimal() {
    let inputs = binary64_inputs();
    assert!(inputs.len() > 30_000, "{} inputs", inputs.len());
    let mut mismatches = Vec::new();
    let mut lengths = [0usize; 18];
    for &bits in &inputs {
        let x = f64::from_bits(bits);
        let magnitude = x.abs();
        let exact = exact_expansion(magnitude, 800);
        let reads_back =
            |d: &Decimal| crt::strtod_c(d.text().as_bytes()).value.to_bits() == magnitude.to_bits();
        let want = expected(&exact, &reads_back, 17);
        lengths[want.digits.len()] += 1;
        let got = to_decimal_f64(x);
        let got_decimal = port_decimal(got.significand, got.exponent);
        if got_decimal != want || got.is_negative != x.is_sign_negative() {
            mismatches.push(format!(
                "{bits:#018x}: port {}{} want {}{}",
                if got.is_negative { "-" } else { "" },
                got_decimal.text(),
                if x.is_sign_negative() { "-" } else { "" },
                want.text()
            ));
        }
    }
    report("dragonbox-binary64", inputs.len(), &mismatches);
    // Every length from 1 to 17 digits occurred, so no part of the contract went untested.
    for (len, &count) in lengths.iter().enumerate().skip(1) {
        assert!(
            count > 0,
            "no binary64 input whose shortest decimal has {len} digits"
        );
    }
}

#[test]
fn binary32_is_the_shortest_closest_decimal() {
    let inputs = binary32_inputs();
    assert!(inputs.len() > 50_000, "{} inputs", inputs.len());
    let mut mismatches = Vec::new();
    let mut lengths = [0usize; 10];
    for &bits in &inputs {
        let x = f32::from_bits(bits);
        let magnitude = x.abs();
        let exact = exact_expansion(f64::from(magnitude), 200);
        let reads_back =
            |d: &Decimal| crt::strtof_c(d.text().as_bytes()).value.to_bits() == magnitude.to_bits();
        let want = expected(&exact, &reads_back, 9);
        lengths[want.digits.len()] += 1;
        let got = to_decimal_f32(x);
        let got_decimal = port_decimal(got.significand, got.exponent);
        if got_decimal != want || got.is_negative != x.is_sign_negative() {
            mismatches.push(format!(
                "{bits:#010x}: port {}{} want {}{}",
                if got.is_negative { "-" } else { "" },
                got_decimal.text(),
                if x.is_sign_negative() { "-" } else { "" },
                want.text()
            ));
        }
    }
    report("dragonbox-binary32", inputs.len(), &mismatches);
    for (len, &count) in lengths.iter().enumerate().skip(1) {
        assert!(
            count > 0,
            "no binary32 input whose shortest decimal has {len} digits"
        );
    }
}

/// The reference itself: on decimals whose answer the C runtime alone settles, it picks
/// them. `strtod("0.1")` reads back as 0.1, and the exact expansion of 0.5 has one digit.
#[test]
fn the_reference_picks_what_the_c_runtime_reads_back() {
    let tenth = crt::strtod_c(b"0.1").value;
    let exact = exact_expansion(tenth, 800);
    assert!(exact.digits.len() > 17, "0.1 is not a binary fraction");
    let reads_back =
        |d: &Decimal| crt::strtod_c(d.text().as_bytes()).value.to_bits() == tenth.to_bits();
    assert_eq!(
        expected(&exact, &reads_back, 17),
        Decimal::new(b"1".to_vec(), -1)
    );

    let half = crt::strtod_c(b"0.5").value;
    assert_eq!(exact_expansion(half, 800), Decimal::new(b"5".to_vec(), -1));
}

/// The cache tables are the vendored ones: their entries, one hexadecimal literal per line
/// (binary64: high and low, separated by a space), hash to the SHA-256 of the same lines taken
/// from yaml-cpp 0.9.0's header. A table entry off by one bit can leave the round trips above
/// unchanged, so the tables are checked as the data they are. The digests come from
/// `git -C upstream/yaml-cpp show yaml-cpp-0.9.0:src/contrib/dragonbox.h`, through
///
/// ```text
/// sed -n '1340,1386p' | grep -o 'UINT64_C(0x[0-9a-f]*)' | sed 's/UINT64_C(\(.*\))/\1/' | sha256sum
/// sed -n '1396,2022p' | grep -o '{UINT64_C(0x[0-9a-f]*), UINT64_C(0x[0-9a-f]*)}' \
///   | sed 's/{UINT64_C(\(0x[0-9a-f]*\)), UINT64_C(\(0x[0-9a-f]*\))}/\1 \2/' | sha256sum
/// ```
#[test]
fn cache_tables_are_the_vendored_ones() {
    let binary32: String = binary32::CACHE
        .iter()
        .map(|entry| format!("{entry:#018x}\n"))
        .collect();
    let binary64: String = binary64::CACHE
        .iter()
        .map(|entry| format!("{:#018x} {:#018x}\n", entry.high, entry.low))
        .collect();
    assert_eq!(binary32.lines().count(), 78);
    assert_eq!(binary64.lines().count(), 619);
    assert_eq!(
        ocio_testkit::fixtures::sha256_hex(binary32.as_bytes()),
        "3f680e5b91a24803083e2556c9a0ef3ee191b6b93f3315cbcf7a6aeb95baf9dd"
    );
    assert_eq!(
        ocio_testkit::fixtures::sha256_hex(binary64.as_bytes()),
        "aaced676cc350eaa7c13d4fb5ec521f09836ff4b6961127b1cec8a6492170449"
    );
}
