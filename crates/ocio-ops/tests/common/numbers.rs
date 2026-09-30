// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Number-text generators shared by the NumberUtils tests. Decimal expansions come from the
//! C runtime (`%.800e` prints a double's exact digits on UCRT and glibc).

use ocio_testkit::crt;
use ocio_testkit::probe::Rng;

/// `OCIO_RS_PARSE_SWEEP=<n>`: the number of generated inputs per group (default 50,000).
pub(crate) fn sweep_count() -> usize {
    std::env::var("OCIO_RS_PARSE_SWEEP")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50_000)
}
/// The exact decimal expansion of a positive finite double, from the C runtime: its digits
/// (no leading zeros) and the exponent of `0.d...`.
pub(crate) fn crt_exact(x: f64) -> (Vec<u8>, i64) {
    let text = crt::format_f64("%.800e", x);
    let (mantissa, exponent) = text.split_once('e').expect("an exponent");
    let mut digits: Vec<u8> = mantissa
        .bytes()
        .filter(u8::is_ascii_digit)
        .map(|b| b - b'0')
        .collect();
    while digits.last() == Some(&0) {
        digits.pop();
    }
    (digits, exponent.parse::<i64>().expect("an exponent") + 1)
}

/// The exact midpoint of two non-negative doubles, as a decimal string, and the decimals
/// just below and above it.
pub(crate) fn midpoint_strings(a: f64, b: f64) -> [String; 3] {
    let (da, ea) = crt_exact(a);
    let (db, eb) = crt_exact(b);
    midpoint_of(&da, ea, &db, eb)
}

/// `midpoint_strings` for exact decimals `0.da * 10^ea` and `0.db * 10^eb`.
pub(crate) fn midpoint_of(da: &[u8], ea: i64, db: &[u8], eb: i64) -> [String; 3] {
    let top = ea.max(eb) + 1;
    // Align both on the same exponent with one leading zero digit for the carry.
    let align = |d: &[u8], e: i64, len: usize| {
        let mut v = vec![0u8; (top - e) as usize];
        v.extend_from_slice(d);
        v.resize(len, 0);
        v
    };
    let len = (top - ea.min(eb)) as usize + da.len().max(db.len()) + 1;
    let (va, vb) = (align(da, ea, len), align(db, eb, len));
    let mut sum = vec![0u8; len];
    let mut carry = 0;
    for i in (0..len).rev() {
        let s = va[i] + vb[i] + carry;
        sum[i] = s % 10;
        carry = s / 10;
    }
    // Halve (the sum of two exact decimals halves exactly with one more digit).
    let mut half = Vec::with_capacity(len + 1);
    let mut rem = 0;
    for &d in sum.iter().chain(std::iter::once(&0)) {
        let v = rem * 10 + d;
        half.push(v / 2);
        rem = v % 2;
    }
    let lead = half.iter().position(|&d| d != 0).unwrap_or(0);
    let exp = top - lead as i64;
    let mut digits: Vec<u8> = half[lead..].to_vec();
    while digits.last() == Some(&0) {
        digits.pop();
    }
    let text = |d: &[u8]| {
        let s: String = d.iter().map(|&x| char::from(b'0' + x)).collect();
        format!("0.{s}e{exp}")
    };
    let mid = text(&digits);
    let mut above = digits.clone();
    above.extend([0, 0, 0, 0, 1]);
    // The digits have no trailing zeros, so the last one is at least 1.
    let mut below = digits.clone();
    *below.last_mut().expect("a nonzero midpoint") -= 1;
    below.extend([9, 9, 9, 9, 9, 9]);
    [text(&below), mid, text(&above)]
}

/// Well-formed numbers (decimal and hexadecimal, with and without signs).
pub(crate) fn numeric_strings(n: usize, seed: u64) -> Vec<String> {
    let mut rng = Rng::new(seed);
    let mut out = Vec::with_capacity(n * 3);
    for _ in 0..n / 10 {
        let x = f64::from_bits(rng.next_u64()).abs();
        if x.is_finite() {
            out.push(crt::format_f64("%.17g", x));
            out.push(crt::format_f64("%.25e", x));
            out.push(crt::format_f64("%a", x));
        }
        let f = f32::from_bits(rng.next_u64() as u32).abs();
        if f.is_finite() {
            out.push(crt::format_f64("%.9g", f64::from(f)));
            out.push(crt::format_f64("%.12e", f64::from(f)));
            out.push(crt::format_f64("%a", f64::from(f)));
        }
    }
    // Midpoints between neighbouring doubles and floats, and just around them.
    for _ in 0..n / 50 {
        let bits = rng.next_u64() & 0x7fff_ffff_ffff_ffff;
        let a = f64::from_bits(bits);
        let b = f64::from_bits(bits + 1);
        if a.is_finite() && b.is_finite() {
            out.extend(midpoint_strings(a, b));
        }
        let fbits = (rng.next_u64() as u32) & 0x7fff_ffff;
        let (fa, fb) = (f32::from_bits(fbits), f32::from_bits(fbits + 1));
        if fa.is_finite() && fb.is_finite() {
            out.extend(midpoint_strings(f64::from(fa), f64::from(fb)));
        }
    }
    // The underflow, subnormal/normal and overflow boundaries of both formats.
    for bits in [0u64, 1, 2, (1 << 52) - 1, 1 << 52, 0x7fef_ffff_ffff_fffe] {
        out.extend(midpoint_strings(
            f64::from_bits(bits),
            f64::from_bits(bits + 1),
        ));
    }
    for bits in [0u32, 1, 2, (1 << 23) - 1, 1 << 23, 0x7f7f_fffe] {
        let (a, b) = (f32::from_bits(bits), f32::from_bits(bits + 1));
        out.extend(midpoint_strings(f64::from(a), f64::from(b)));
    }
    // f32::MAX and 2^128 are both doubles; 2^1024 is not, so double 2^1023's digits.
    out.extend(midpoint_strings(f64::from(f32::MAX), 2f64.powi(128)));
    let (d1023, e1023) = crt_exact(2f64.powi(1023));
    let mut d1024 = vec![0u8; d1023.len() + 1];
    let mut carry = 0;
    for i in (0..d1023.len()).rev() {
        let v = d1023[i] * 2 + carry;
        d1024[i + 1] = v % 10;
        carry = v / 10;
    }
    d1024[0] = carry;
    let e1024 = if carry == 0 {
        d1024.remove(0);
        e1023
    } else {
        e1023 + 1
    };
    while d1024.last() == Some(&0) {
        d1024.pop();
    }
    let (dmax, emax) = crt_exact(f64::MAX);
    out.extend(midpoint_of(&dmax, emax, &d1024, e1024));
    // Random digit strings with random exponents, and long mantissas.
    for _ in 0..n / 10 {
        let digits: String = (0..1 + rng.next_u64() % 40)
            .map(|_| char::from(b'0' + (rng.next_u64() % 10) as u8))
            .collect();
        let exp = (rng.next_u64() % 800) as i64 - 400;
        let point = (rng.next_u64() as usize) % (digits.len() + 1);
        out.push(format!("{}.{}e{exp}", &digits[..point], &digits[point..]));
    }
    for k in -340i32..=320 {
        out.push(format!("1e{k}"));
        out.push(format!("9.999999999999999e{k}"));
    }
    let long: String = std::iter::repeat_n("1234567890", 120).collect();
    out.push(format!("0.{long}"));
    out.push(format!("{long}e-1100"));
    out.push(format!("{}1", "0".repeat(900)));
    // Signs on a copy of everything so far.
    let signed: Vec<String> = out
        .iter()
        .enumerate()
        .map(|(i, s)| {
            if i % 2 == 0 {
                format!("-{s}")
            } else {
                format!("+{s}")
            }
        })
        .collect();
    out.extend(signed);
    out
}

/// Splits `total` generated inputs into batches of at most `BATCH`, as (seed offset, size),
/// so a ten-million-input sweep never holds more than one batch in memory.
pub(crate) fn batches(total: usize) -> impl Iterator<Item = (u64, usize)> {
    const BATCH: usize = 100_000;
    (0..total.div_ceil(BATCH)).map(move |i| (i as u64, BATCH.min(total - i * BATCH)))
}
