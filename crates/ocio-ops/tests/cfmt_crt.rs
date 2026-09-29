// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `ocio_ops::cfmt` against the platform C runtime (UCRT on Windows, glibc on Linux), called
//! through `ocio_testkit::crt`. Every expected string comes from the C runtime.
//!
//! Each OCIO format is swept over the same kinds of values: random `f64` bit patterns,
//! random `f32` bit patterns (OCIO formats floats by promoting them), subnormals of both,
//! exact halfway cases for that format, values next to powers of ten and next to the
//! rounding boundaries below them, and specials (signed zeros, infinities, NaNs with every
//! kind of payload).
//!
//! `OCIO_RS_CFMT_SWEEP=<n>` sets the number of values per format (default 200,000). The
//! Phase 0 evidence runs use ten million per format in release mode, on both platforms:
//!
//! ```text
//! OCIO_RS_CFMT_SWEEP=10000000 cargo test --release -p ocio-ops --test cfmt_crt
//! OCIO_RS_CFMT_SWEEP=10000000 scripts/rocky9.sh cargo test --release -p ocio-ops --test cfmt_crt
//! ```

use ocio_ops::cfmt::{self, Adjust, Base, Crt, FloatField, OStringStream};
use ocio_testkit::crt;
use ocio_testkit::probe::Rng;

/// The conversions OCIO uses (see `docs/spikes/s1-wp05.md`, the inventory):
/// - `%.5g`: CTF writer, F16 arrays (`CTFTransform.cpp:606-612`);
/// - `%.6g`: the default stream precision (`operator<<` of most transforms, error messages);
/// - `%.7g`: `FloatToString`, op cache IDs, yaml-cpp floats;
/// - `%.8g`: CTF writer, F32 arrays of floats;
/// - `%.9g`: `max_digits10` of float (GPU shader text);
/// - `%.15g`: yaml-cpp doubles, CTF writer doubles;
/// - `%.16g`: `DoubleToString`, `MatrixTransform` `operator<<`, `GpuShaderText`;
/// - `%.17g`: `max_digits10` of double (GPU shader text);
/// - `%.6f`: `std::to_string(float/double)` and the LUT writers' `std::fixed` precision 6.
const OCIO_FORMATS: &[&str] = &[
    "%.5g", "%.6g", "%.7g", "%.8g", "%.9g", "%.15g", "%.16g", "%.17g", "%.6f",
];

fn sweep_count() -> usize {
    std::env::var("OCIO_RS_CFMT_SWEEP")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(200_000)
}

/// Signed zeros, infinities, NaNs (quiet and signaling, with and without payload, both
/// signs, including UCRT's "indeterminate" pattern), and the extremes.
fn specials() -> Vec<f64> {
    let mut bits: Vec<u64> = vec![
        0,
        1 << 63,
        0x7ff0_0000_0000_0000,
        0xfff0_0000_0000_0000,
        0x7ff8_0000_0000_0000,
        0xfff8_0000_0000_0000,
        0x7ff8_0000_0000_0001,
        0xfff8_0000_0000_0001,
        0x7ff0_0000_0000_0001,
        0xfff0_0000_0000_0001,
        0x7ff4_0000_0000_0000,
        0x7fff_ffff_ffff_ffff,
        0xffff_ffff_ffff_ffff,
        1,
        (1 << 52) - 1,
        1 << 52,
        0x7fef_ffff_ffff_ffff,
    ];
    let negated: Vec<u64> = bits.iter().map(|b| b ^ (1 << 63)).collect();
    bits.extend(negated);
    let mut values: Vec<f64> = bits.into_iter().map(f64::from_bits).collect();
    for f in [
        f32::MIN_POSITIVE,
        f32::MAX,
        f32::from_bits(1),
        f32::NAN,
        f32::from_bits(0x7fc0_0001),
        f32::from_bits(0x7f80_0001),
    ] {
        values.push(f64::from(f));
        values.push(-f64::from(f));
    }
    values
}

/// Exact halfway cases for `%.{sig}g` / `%.{sig-1}e`: values whose exact decimal expansion
/// has `sig + 1` significant digits ending in 5. They are `M * 5^s * 2^(s-k)` with `M` odd,
/// so that `M * 5^k` has `sig + 1` digits.
fn halfway_values(rng: &mut Rng, sig: u32, count: usize) -> Vec<f64> {
    let lo = 10u128.pow(sig);
    let hi = 10u128.pow(sig + 1) - 1;
    let mut out = Vec::with_capacity(count);
    while out.len() < count {
        let k = 1 + (rng.next_u64() % 30) as u32;
        let p5 = 5u128.pow(k);
        let m_lo = lo.div_ceil(p5);
        let m_hi = (hi / p5).min((1 << 53) - 1);
        if m_lo > m_hi {
            continue;
        }
        let m = (m_lo + u128::from(rng.next_u64()) % (m_hi - m_lo + 1)) | 1;
        if m > m_hi {
            continue;
        }
        // Scale by 10^s while the mantissa stays exact.
        let s = (rng.next_u64() % 12) as u32;
        let ms = m * 5u128.pow(s);
        if ms >= 1 << 53 {
            continue;
        }
        let v = (ms as f64) * 2f64.powi(s as i32 - k as i32);
        out.push(if rng.next_u64() & 1 == 0 { v } else { -v });
    }
    out
}

/// Exact halfway cases for `%.{p}f`: odd multiples of `2^-(p+1)`.
fn fixed_halfway_values(rng: &mut Rng, p: u32, count: usize) -> Vec<f64> {
    (0..count)
        .map(|_| {
            let bits = 1 + (rng.next_u64() % 52) as u32;
            let m = (rng.next_u64() >> (64 - bits)) | 1;
            let v = (m as f64) * 2f64.powi(-(p as i32 + 1));
            if rng.next_u64() & 1 == 0 { v } else { -v }
        })
        .collect()
}

/// The doubles nearest to 10^k and to the `%.{sig}g` carry boundary below it
/// (`9.99..95e(k-1)`), with their neighbours, for every decimal exponent in range.
fn power_of_ten_values(sig: u32) -> Vec<f64> {
    let mut out = Vec::new();
    for k in -330i32..=310 {
        let nines = "9".repeat(sig as usize);
        for text in [format!("1e{k}"), format!("{nines}5e{}", k - sig as i32 - 1)] {
            let Ok(x) = text.parse::<f64>() else { continue };
            if !x.is_finite() || x == 0.0 {
                continue;
            }
            for delta in -3i64..=3 {
                let v = f64::from_bits((x.to_bits() as i64 + delta) as u64);
                if v.is_finite() {
                    out.push(v);
                    out.push(-v);
                }
            }
            let f = x as f32;
            if f.is_finite() && f != 0.0 {
                for delta in -2i32..=2 {
                    let g = f32::from_bits((f.to_bits() as i32 + delta) as u32);
                    out.push(f64::from(g));
                }
            }
        }
    }
    out
}

/// The value set for one format: 40% random f64 bits, 40% random f32 bits, 10% subnormals,
/// 10% halfway cases; plus specials and the power-of-ten set.
fn values_for(format: &str, n: usize, seed: u64) -> Vec<f64> {
    let spec = cfmt::Spec::parse(format).expect("an OCIO format");
    let precision = spec.precision.unwrap_or(6) as u32;
    let mut rng = Rng::new(seed);
    let mut v = specials();
    for _ in 0..n * 4 / 10 {
        v.push(f64::from_bits(rng.next_u64()));
    }
    for _ in 0..n * 4 / 10 {
        v.push(f64::from(f32::from_bits(rng.next_u64() as u32)));
    }
    for _ in 0..n / 20 {
        let sign = rng.next_u64() & (1 << 63);
        v.push(f64::from_bits(
            sign | (rng.next_u64() % ((1 << 52) - 1) + 1),
        ));
        let sign32 = (rng.next_u64() as u32) & (1 << 31);
        let sub32 = f32::from_bits(sign32 | ((rng.next_u64() as u32) % ((1 << 23) - 1) + 1));
        v.push(f64::from(sub32));
    }
    match spec.conv {
        cfmt::Conv::F | cfmt::Conv::UpperF => {
            v.extend(fixed_halfway_values(&mut rng, precision, n / 10));
        }
        cfmt::Conv::E | cfmt::Conv::UpperE => {
            v.extend(halfway_values(&mut rng, precision + 1, n / 10));
        }
        cfmt::Conv::G | cfmt::Conv::UpperG => {
            v.extend(halfway_values(&mut rng, precision.max(1), n / 10));
        }
    }
    v.extend(power_of_ten_values(precision.max(1)));
    v
}

/// Compares `format` for all `values` on every available core; panics with the first
/// mismatches.
fn check_format(format: &str, values: &[f64]) {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let chunk = values.len().div_ceil(threads).max(1);
    let failures: Vec<String> = std::thread::scope(|scope| {
        let handles: Vec<_> = values
            .chunks(chunk)
            .map(|part| {
                scope.spawn(move || {
                    let mut bad = Vec::new();
                    for &v in part {
                        let expected = crt::format_f64(format, v);
                        let actual = cfmt::format(Crt::NATIVE, format, v);
                        if expected != actual && bad.len() < 10 {
                            bad.push(format!(
                                "{format} of {v:e} ({:#018x}): crt {expected:?}, cfmt {actual:?}",
                                v.to_bits()
                            ));
                        }
                    }
                    bad
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("sweep thread"))
            .collect()
    });
    assert!(
        failures.is_empty(),
        "{} mismatches (first ones):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn ocio_formats_match_the_crt() {
    let n = sweep_count();
    for (i, format) in OCIO_FORMATS.iter().enumerate() {
        let values = values_for(format, n, 0x5eed_0000 + i as u64);
        check_format(format, &values);
    }
}

#[test]
fn halfway_cases_at_every_precision_match_the_crt() {
    let mut rng = Rng::new(0xa1f);
    for sig in 1..=17u32 {
        let values = halfway_values(&mut rng, sig, 2000);
        check_format(&format!("%.{sig}g"), &values);
        check_format(&format!("%.{}e", sig - 1), &values);
    }
    for p in 0..=20u32 {
        let values = fixed_halfway_values(&mut rng, p, 2000);
        check_format(&format!("%.{p}f"), &values);
    }
}

#[test]
fn flags_widths_and_precisions_match_the_crt() {
    let mut rng = Rng::new(0xf1a9);
    let mut values = specials();
    for _ in 0..300 {
        values.push(f64::from_bits(rng.next_u64()));
        values.push(f64::from(f32::from_bits(rng.next_u64() as u32)));
    }
    values.extend(halfway_values(&mut rng, 3, 100));
    values.extend([
        0.5, 1.5, 2.5, 9.5, 99.5, 0.05, 1e-5, 1e-4, 123456.0, 1e15, 1e16, 1e17,
    ]);
    let flag_sets = [
        "", "-", "+", " ", "#", "0", "-+", "+0", " 0", "#0", "-#", "+#0", "- #",
    ];
    for conv in ["e", "E", "f", "F", "g", "G"] {
        for flags in flag_sets {
            for width in ["", "1", "8", "25"] {
                for precision in ["", ".0", ".1", ".3", ".12", ".40"] {
                    let format = format!("%{flags}{width}{precision}{conv}");
                    check_format(&format, &values);
                }
            }
        }
    }
}

#[test]
fn to_string_is_percent_f() {
    let mut rng = Rng::new(0x7057);
    for _ in 0..20_000 {
        let d = f64::from_bits(rng.next_u64());
        assert_eq!(
            cfmt::to_string_f64(Crt::NATIVE, d),
            crt::format_f64("%f", d)
        );
        let f = f32::from_bits(rng.next_u64() as u32);
        assert_eq!(
            cfmt::to_string_f32(Crt::NATIVE, f),
            crt::format_f64("%f", f64::from(f))
        );
    }
}

/// The stream's padding against printf's own width handling, where the two are defined to
/// agree: right adjustment with ' ' fill is `%W`, left is `%-W`, internal with '0' fill is
/// `%0W` for finite values; showpos is `+` and showpoint is `#`.
#[test]
fn stream_padding_matches_printf_width() {
    let mut rng = Rng::new(0x05f7);
    let mut values = Vec::new();
    for _ in 0..500 {
        values.push(f64::from_bits(rng.next_u64()));
        values.push(f64::from(f32::from_bits(rng.next_u64() as u32)));
    }
    values.extend(specials());
    let fields = [
        (FloatField::Default, "g"),
        (FloatField::Fixed, "f"),
        (FloatField::Scientific, "e"),
    ];
    for &(field, conv) in &fields {
        for precision in [0i64, 1, 5, 8, 15, 17] {
            for width in [0i64, 3, 11, 19, 30] {
                for &v in &values {
                    for (adjust, fill, flag) in [
                        (Adjust::Right, ' ', ""),
                        (Adjust::Left, ' ', "-"),
                        (Adjust::Internal, '0', "0"),
                    ] {
                        if adjust == Adjust::Internal && !v.is_finite() {
                            continue;
                        }
                        for (showpos, showpoint, extra) in
                            [(false, false, ""), (true, false, "+"), (true, true, "+#")]
                        {
                            let mut os = OStringStream::new(Crt::NATIVE);
                            os.precision = precision;
                            os.float_field = field;
                            os.width = width;
                            os.fill = fill;
                            os.adjust = adjust;
                            os.showpos = showpos;
                            os.showpoint = showpoint;
                            os.put_f64(v);
                            let format = format!("%{flag}{extra}{width}.{precision}{conv}");
                            assert_eq!(os.str(), crt::format_f64(&format, v), "{format} {v:e}");
                            assert_eq!(os.width, 0, "width resets");
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn stream_integers_match_printf() {
    let mut rng = Rng::new(0x1a7);
    for _ in 0..5000 {
        let u = rng.next_u64() >> (rng.next_u64() % 64);
        let mut os = OStringStream::new(Crt::NATIVE);
        os.base = Base::Hex;
        os.fill = '0';
        os.width = 16;
        os.put_u64(u);
        assert_eq!(os.str(), crt::format_u64("%016llx", u));

        let mut os = OStringStream::new(Crt::NATIVE);
        os.put_u64(u);
        os.put_str(" ");
        os.put_i64(u as i64);
        let expected = format!(
            "{} {}",
            crt::format_u64("%llu", u),
            crt::format_i64("%lld", u as i64)
        );
        assert_eq!(os.str(), expected);
    }
}
