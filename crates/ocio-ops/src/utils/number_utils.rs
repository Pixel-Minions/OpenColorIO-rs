// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/utils/NumberUtils.h` @ v2.5.2: locale-independent number parsing.
//!
//! `NumberUtils.h` has two implementations, chosen per translation unit by
//! `__cpp_lib_to_chars` (NumberUtils.h:18-21), and the two official 2.5.2 wheels compiled
//! different ones:
//! - **Windows (MSVC)**: `std::from_chars`. MSVC defines the feature macro in every STL header,
//!   and `OpenColorIO_2_5.dll` imports no `strtod`.
//! - **Linux (GCC 14.2.1, manylinux_2_28)**: `strtod_l`/`strtof_l`/`strtol_l` in the "C"
//!   locale. libstdc++ only defines `__cpp_lib_to_chars` in `<charconv>` and `<version>`,
//!   which NumberUtils.h includes after the check, so every file takes this branch
//!   (`libOpenColorIO.so` imports `strtod_l`, `strtof_l`, `strtol_l` and `newlocale`, and
//!   contains no `from_chars`).
//!
//! The branches agree on ordinary numbers and differ on edge cases, and the port reproduces
//! each on its platform (PLAN.md D12): a sign before `0x`, `0x` in a longer buffer, subnormal
//! floats, out-of-range values, NaN payloads, octal integers and the width of `long`.
//! [`Flavor::NATIVE`] is the branch of the platform being compiled for.
//!
//! Inputs are byte buffers that start at `first`: `last` is an offset into them, and the
//! `strtod` branch reads the buffer as a C string (up to its first NUL, or its end), which
//! may extend past `last`, exactly as the C++ does.

use std::cmp::Ordering;

use crate::cfmt::exact_digits;

/// Which implementation of `NumberUtils.h` to follow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavor {
    /// `std::from_chars` from the MSVC STL (`USE_CHARCONV_FROM_CHARS`): the Windows wheel.
    /// `long` is 32 bits.
    FromChars,
    /// `strtod_l`, `strtof_l` and `strtol_l` from glibc in the "C" locale: the Linux wheel.
    /// `long` is 64 bits.
    Strtod,
}

impl Flavor {
    /// The branch the wheel for the platform being compiled for uses.
    pub const NATIVE: Flavor = if cfg!(windows) {
        Flavor::FromChars
    } else {
        Flavor::Strtod
    };
}

/// The `std::errc` values NumberUtils returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Errc {
    /// `std::errc()`: success.
    Ok,
    /// `std::errc::invalid_argument`
    InvalidArgument,
    /// `std::errc::result_out_of_range`
    ResultOutOfRange,
    /// `std::errc::argument_out_of_domain`: the `strtod` branch parsed past `last`.
    ArgumentOutOfDomain,
}

/// `NumberUtils::from_chars_result`, with `ptr` as an offset from `first`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FromCharsResult {
    /// Where parsing stopped.
    pub ptr: usize,
    /// The error code.
    pub ec: Errc,
}

// ---------------------------------------------------------------------------------------
// Binary floating-point formats
// ---------------------------------------------------------------------------------------

/// The two formats NumberUtils parses into.
trait Float: Copy {
    /// Significand bits including the implicit one (`MANT_DIG`).
    const MANT_DIG: u32;
    /// Unbiased exponent of the smallest normal value.
    const MIN_EXP: i32;
    /// Unbiased exponent of the largest finite value.
    const MAX_EXP: i32;
    /// Decimal exponents (of `0.d * 10^e`) above which every value overflows...
    const DEC_OVERFLOW: i64;
    /// ...and below which every value rounds to zero.
    const DEC_UNDERFLOW: i64;
    /// The sign bit.
    const SIGN: u64;
    /// The exponent field, all ones.
    const EXP_MASK: u64;
    /// The quiet-NaN bit.
    const QUIET: u64;
    fn from_bits_u64(bits: u64) -> Self;
    fn to_bits_u64(self) -> u64;
    /// Correctly rounded decimal to binary (Rust's `str::parse`).
    fn parse_decimal(text: &str) -> Self;
    fn to_f64(self) -> f64;
    fn is_zero(self) -> bool;
    fn is_inf(self) -> bool;
    fn min_normal() -> Self;
}

impl Float for f64 {
    const MANT_DIG: u32 = 53;
    const MIN_EXP: i32 = -1022;
    const MAX_EXP: i32 = 1023;
    const DEC_OVERFLOW: i64 = 310;
    const DEC_UNDERFLOW: i64 = -326;
    const SIGN: u64 = 1 << 63;
    const EXP_MASK: u64 = 0x7ff0_0000_0000_0000;
    const QUIET: u64 = 1 << 51;
    fn from_bits_u64(bits: u64) -> f64 {
        f64::from_bits(bits)
    }
    fn to_bits_u64(self) -> u64 {
        self.to_bits()
    }
    fn parse_decimal(text: &str) -> f64 {
        text.parse().expect("a normalized decimal")
    }
    fn to_f64(self) -> f64 {
        self
    }
    fn is_zero(self) -> bool {
        self == 0.0
    }
    fn is_inf(self) -> bool {
        self.is_infinite()
    }
    fn min_normal() -> f64 {
        f64::MIN_POSITIVE
    }
}

impl Float for f32 {
    const MANT_DIG: u32 = 24;
    const MIN_EXP: i32 = -126;
    const MAX_EXP: i32 = 127;
    const DEC_OVERFLOW: i64 = 40;
    const DEC_UNDERFLOW: i64 = -46;
    const SIGN: u64 = 1 << 31;
    const EXP_MASK: u64 = 0x7f80_0000;
    const QUIET: u64 = 1 << 22;
    fn from_bits_u64(bits: u64) -> f32 {
        f32::from_bits(bits as u32)
    }
    fn to_bits_u64(self) -> u64 {
        u64::from(self.to_bits())
    }
    fn parse_decimal(text: &str) -> f32 {
        text.parse().expect("a normalized decimal")
    }
    fn to_f64(self) -> f64 {
        f64::from(self)
    }
    fn is_zero(self) -> bool {
        self == 0.0
    }
    fn is_inf(self) -> bool {
        self.is_infinite()
    }
    fn min_normal() -> f32 {
        f32::MIN_POSITIVE
    }
}

fn negate<F: Float>(v: F, negative: bool) -> F {
    if negative {
        F::from_bits_u64(v.to_bits_u64() ^ F::SIGN)
    } else {
        v
    }
}

/// A magnitude converted to `F` (correctly rounded, half to even), with the facts both
/// libraries report on.
struct Converted<F> {
    value: F,
    /// The value was nonzero and rounded to zero, or rounded to infinity.
    out_of_range: bool,
    /// glibc's underflow: tiny (after rounding, x86 `TININESS_AFTER_ROUNDING`) and inexact,
    /// or overflow (strtod_l.c `round_and_return`).
    glibc_erange: bool,
}

/// A parsed decimal significand: `0.d[0] d[1] ... * 10^exp10`, with no leading or trailing
/// zero digits. Empty `digits` means zero.
struct Decimal {
    digits: Vec<u8>,
    exp10: i64,
}

/// Compares two normalized nonzero decimals.
fn cmp_decimal(a: &[u8], a_exp: i64, b: &[u8], b_exp: i64) -> Ordering {
    a_exp.cmp(&b_exp).then_with(|| {
        let n = a.len().max(b.len());
        (0..n)
            .map(|i| {
                a.get(i)
                    .copied()
                    .unwrap_or(0)
                    .cmp(&b.get(i).copied().unwrap_or(0))
            })
            .find(|o| o.is_ne())
            .unwrap_or(Ordering::Equal)
    })
}

/// The exact decimal expansion of `m * 2^e`, as (digits, exp10).
fn exact(m: u64, e: i32) -> (Vec<u8>, i64) {
    let (digits, exp10) = exact_digits(m, e);
    (digits, i64::from(exp10))
}

/// The exact decimal expansion of a positive finite float.
fn exact_of<F: Float>(v: F) -> (Vec<u8>, i64) {
    let bits = v.to_f64().to_bits();
    let biased = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1 << 52) - 1);
    if biased == 0 {
        exact(fraction, -1074)
    } else {
        exact(fraction | (1 << 52), biased - 1075)
    }
}

fn decimal_to_float<F: Float>(d: &Decimal) -> Converted<F> {
    let zero = F::from_bits_u64(0);
    let inf = F::from_bits_u64(F::EXP_MASK);
    if d.digits.is_empty() {
        return Converted {
            value: zero,
            out_of_range: false,
            glibc_erange: false,
        };
    }
    if d.exp10 > F::DEC_OVERFLOW {
        return Converted {
            value: inf,
            out_of_range: true,
            glibc_erange: true,
        };
    }
    if d.exp10 < F::DEC_UNDERFLOW {
        return Converted {
            value: zero,
            out_of_range: true,
            glibc_erange: true,
        };
    }
    let mut text = String::with_capacity(d.digits.len() + 24);
    text.push_str("0.");
    text.extend(d.digits.iter().map(|&x| char::from(b'0' + x)));
    text.push('e');
    text.push_str(&d.exp10.to_string());
    let value = F::parse_decimal(&text);
    if value.is_inf() {
        return Converted {
            value,
            out_of_range: true,
            glibc_erange: true,
        };
    }
    if value.is_zero() {
        return Converted {
            value,
            out_of_range: true,
            glibc_erange: true,
        };
    }
    let min_normal = F::min_normal();
    let glibc_erange = if value.to_f64() < min_normal.to_f64() {
        // Subnormal: tiny; ERANGE when inexact.
        let (vd, ve) = exact_of(value);
        cmp_decimal(&d.digits, d.exp10, &vd, ve).is_ne()
    } else if value.to_bits_u64() == min_normal.to_bits_u64() {
        // Rounded up to the smallest normal: tiny only if rounding to MANT_DIG bits with an
        // unbounded exponent stays below it, i.e. x < 2^MIN_EXP - 2^(MIN_EXP - MANT_DIG - 1).
        let (nd, ne) = exact_of(min_normal);
        let below = cmp_decimal(&d.digits, d.exp10, &nd, ne).is_lt();
        let m = (1u64 << (F::MANT_DIG + 1)) - 1;
        let (md, me) = exact(m, F::MIN_EXP - F::MANT_DIG as i32 - 1);
        below && cmp_decimal(&d.digits, d.exp10, &md, me).is_lt()
    } else {
        false
    };
    Converted {
        value,
        out_of_range: false,
        glibc_erange,
    }
}

/// A parsed hexadecimal significand: `(mant + sticky) * 2^exp2`, where `sticky` stands for
/// nonzero bits below `mant` (set only once `mant` holds 64 bits).
struct Hex {
    mant: u64,
    exp2: i64,
    sticky: bool,
}

/// Rounds `mant * 2^exp2` (plus sticky) to `MANT_DIG` bits at `lsb`, half to even.
/// Returns the integer significand and whether anything was dropped.
fn round_at(mant: u64, exp2: i64, sticky: bool, lsb: i64) -> (u64, bool) {
    let shift = lsb - exp2;
    if shift <= 0 {
        debug_assert!(!sticky && shift > -64);
        return (mant << (-shift), false);
    }
    let (q, round, rest) = if shift > 64 {
        (0, false, mant != 0 || sticky)
    } else if shift == 64 {
        (0, mant >> 63 == 1, (mant << 1) != 0 || sticky)
    } else {
        let q = mant >> shift;
        let round = (mant >> (shift - 1)) & 1 == 1;
        let rest = (mant & ((1u64 << (shift - 1)) - 1)) != 0 || sticky;
        (q, round, rest)
    };
    let up = round && (rest || q & 1 == 1);
    (q + u64::from(up), round || rest)
}

fn hex_to_float<F: Float>(h: &Hex) -> Converted<F> {
    let zero = F::from_bits_u64(0);
    let inf = F::from_bits_u64(F::EXP_MASK);
    if h.mant == 0 {
        return Converted {
            value: zero,
            out_of_range: false,
            glibc_erange: false,
        };
    }
    let top = i64::from(63 - h.mant.leading_zeros());
    // The value is in [2^e, 2^(e+1)).
    let e = h.exp2 + top;
    let mant_dig = i64::from(F::MANT_DIG);
    let min_exp = i64::from(F::MIN_EXP);
    let max_exp = i64::from(F::MAX_EXP);
    if e > max_exp {
        return Converted {
            value: inf,
            out_of_range: true,
            glibc_erange: true,
        };
    }
    // Tininess after rounding: round to MANT_DIG bits with an unbounded exponent.
    let (qu, _) = round_at(h.mant, h.exp2, h.sticky, e - (mant_dig - 1));
    let e_unbounded = if qu >> F::MANT_DIG != 0 { e + 1 } else { e };
    let tiny = e_unbounded < min_exp;

    let lsb = (e - (mant_dig - 1)).max(min_exp - (mant_dig - 1));
    let (mut q, inexact) = round_at(h.mant, h.exp2, h.sticky, lsb);
    let mut lsb = lsb;
    if q >> F::MANT_DIG != 0 {
        q >>= 1;
        lsb += 1;
    }
    let msb_exp = lsb + mant_dig - 1;
    if q != 0 && q >> (F::MANT_DIG - 1) != 0 && msb_exp > max_exp {
        return Converted {
            value: inf,
            out_of_range: true,
            glibc_erange: true,
        };
    }
    let bits = if q == 0 {
        0
    } else if q >> (F::MANT_DIG - 1) == 0 {
        // Subnormal: the exponent field is zero.
        q
    } else {
        let biased = (msb_exp - min_exp + 1) as u64;
        (biased << (F::MANT_DIG - 1)) | (q & ((1u64 << (F::MANT_DIG - 1)) - 1))
    };
    Converted {
        value: F::from_bits_u64(bits),
        out_of_range: q == 0,
        glibc_erange: tiny && inexact,
    }
}

// ---------------------------------------------------------------------------------------
// Scanning helpers
// ---------------------------------------------------------------------------------------

/// `isspace` in the "C" locale, and NumberUtils' `from_chars_is_space`.
fn is_c_space(c: u8) -> bool {
    matches!(c, b' ' | b'\n' | b'\t' | b'\r' | 0x0b | 0x0c)
}

fn hex_value(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

fn starts_with_ci(s: &[u8], lower: &[u8]) -> bool {
    s.len() >= lower.len()
        && s.iter()
            .zip(lower)
            .all(|(a, b)| a.to_ascii_lowercase() == *b)
}

/// Saturating accumulation of a decimal exponent (either library treats any exponent past
/// its limits as overflow or underflow; far beyond every representable value either way).
const EXP_CLAMP: i64 = 1_000_000_000_000;

/// Scans `digits [. digits] [e [sign] digits]` at `i` (decimal) or the hexadecimal form with
/// a `p` exponent. `require_hex_digit_after_point` is glibc's rule that `0x.` alone is not a
/// number. Returns the end and the parsed significand, or `None` without digits.
fn scan_number(s: &[u8], i: usize, hex: bool) -> Option<(usize, Decimal, Hex)> {
    let base_digit = |c: u8| {
        if hex {
            hex_value(c)
        } else {
            c.is_ascii_digit().then(|| c - b'0')
        }
    };
    let mut j = i;
    let mut int_digits = Vec::new();
    while let Some(d) = s.get(j).copied().and_then(base_digit) {
        int_digits.push(d);
        j += 1;
    }
    let mut frac_digits = Vec::new();
    if s.get(j) == Some(&b'.') {
        let mut k = j + 1;
        while let Some(d) = s.get(k).copied().and_then(base_digit) {
            frac_digits.push(d);
            k += 1;
        }
        if !int_digits.is_empty() || !frac_digits.is_empty() {
            j = k;
        }
    }
    if int_digits.is_empty() && frac_digits.is_empty() {
        return None;
    }
    // Exponent: e/E (decimal) or p/P (hex), optional sign, at least one decimal digit.
    let mut exponent: i64 = 0;
    let marker = if hex { b'p' } else { b'e' };
    if s.get(j).map(u8::to_ascii_lowercase) == Some(marker) {
        let mut k = j + 1;
        let mut negative = false;
        if let Some(&c) = s.get(k)
            && (c == b'+' || c == b'-')
        {
            negative = c == b'-';
            k += 1;
        }
        if s.get(k).is_some_and(u8::is_ascii_digit) {
            while let Some(&c) = s.get(k).filter(|c| c.is_ascii_digit()) {
                exponent = (exponent * 10 + i64::from(c - b'0')).min(EXP_CLAMP);
                k += 1;
            }
            if negative {
                exponent = -exponent;
            }
            j = k;
        }
    }

    let mut decimal = Decimal {
        digits: Vec::new(),
        exp10: 0,
    };
    let mut hexv = Hex {
        mant: 0,
        exp2: 0,
        sticky: false,
    };
    if hex {
        // Value = (hex digits as an integer) * 2^(exponent - 4 * fraction digits).
        let mut exp2 = exponent - 4 * frac_digits.len() as i64;
        let mut mant: u64 = 0;
        let mut sticky = false;
        let mut significant = 0usize;
        for d in int_digits.iter().chain(&frac_digits).copied() {
            if significant == 0 && d == 0 {
                continue;
            }
            if significant < 16 {
                mant = (mant << 4) | u64::from(d);
                significant += 1;
            } else {
                sticky |= d != 0;
                exp2 += 4;
            }
        }
        hexv = Hex {
            mant,
            exp2: exp2.clamp(-EXP_CLAMP, EXP_CLAMP),
            sticky,
        };
    } else {
        let point = int_digits.len() as i64;
        let mut all = int_digits;
        all.extend_from_slice(&frac_digits);
        let lead = all.iter().take_while(|&&d| d == 0).count();
        let mut digits = all.split_off(lead);
        while digits.last() == Some(&0) {
            digits.pop();
        }
        decimal = Decimal {
            exp10: (point - lead as i64 + exponent).clamp(-EXP_CLAMP, EXP_CLAMP),
            digits,
        };
    }
    Some((j, decimal, hexv))
}

// ---------------------------------------------------------------------------------------
// glibc strtod / strtof / strtol ("C" locale)
// ---------------------------------------------------------------------------------------

/// The result of a glibc `strto*` call.
struct Strto<T> {
    value: T,
    end: usize,
    erange: bool,
}

/// glibc `strtoull(s, &end, 0)` for a NaN payload: `[0-9A-Za-z_]*` input, so no whitespace
/// or sign. Returns (value, end, overflowed).
fn glibc_strtoull_base0(s: &[u8]) -> (u64, usize, bool) {
    let (base, start) = if s.first() == Some(&b'0') {
        if s.get(1).map(u8::to_ascii_lowercase) == Some(b'x')
            && s.get(2).copied().and_then(hex_value).is_some()
        {
            (16u64, 2)
        } else {
            (8, 0)
        }
    } else {
        (10, 0)
    };
    let mut value: u64 = 0;
    let mut overflow = false;
    let mut i = start;
    while let Some(d) = s.get(i).copied().and_then(hex_value) {
        if u64::from(d) >= base {
            break;
        }
        match value
            .checked_mul(base)
            .and_then(|v| v.checked_add(u64::from(d)))
        {
            Some(v) => value = v,
            None => overflow = true,
        }
        i += 1;
    }
    if i == start {
        // No digits: "0x" without hex digits consumed only the "0"; nothing otherwise.
        return (0, if start == 2 { 1 } else { 0 }, false);
    }
    if overflow {
        value = u64::MAX;
    }
    (value, i, overflow)
}

/// glibc `strtod_l(s, &end, C)` / `strtof_l` (stdlib/strtod_l.c, glibc 2.34) on the C string
/// `s` (no NUL inside): the C17 7.22.1.3 subject sequence, correctly rounded, with glibc's
/// `ERANGE` on overflow and on tiny inexact results, its `nan(n-char-sequence)` payloads and
/// its return values when nothing is converted.
fn glibc_strtof<F: Float>(s: &[u8]) -> Strto<F> {
    let zero = F::from_bits_u64(0);
    let mut i = 0;
    while s.get(i).copied().is_some_and(is_c_space) {
        i += 1;
    }
    let mut negative = false;
    match s.get(i) {
        Some(b'-') => {
            negative = true;
            i += 1;
        }
        Some(b'+') => i += 1,
        _ => {}
    }
    let no_conversion = Strto {
        value: zero,
        end: 0,
        erange: false,
    };
    let rest = &s[i..];
    if starts_with_ci(rest, b"inf") {
        let end = i + if starts_with_ci(&rest[3..], b"inity") {
            8
        } else {
            3
        };
        return Strto {
            value: negate(F::from_bits_u64(F::EXP_MASK), negative),
            end,
            erange: false,
        };
    }
    if starts_with_ci(rest, b"nan") {
        let mut end = i + 3;
        let mut value = F::from_bits_u64(F::EXP_MASK | F::QUIET);
        if s.get(end) == Some(&b'(') {
            let seq_start = end + 1;
            let mut k = seq_start;
            while s
                .get(k)
                .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
            {
                k += 1;
            }
            if s.get(k) == Some(&b')') {
                // An overflowing payload saturates, and glibc 2.34 leaves errno alone
                // (checked against strtod_l in tests/number_utils_crt.rs).
                let (mant, used, _overflow) = glibc_strtoull_base0(&s[seq_start..k]);
                if seq_start + used == k {
                    // SET_NAN_PAYLOAD: the payload fills the bits below the quiet bit.
                    let payload = mant & (F::QUIET - 1);
                    value = F::from_bits_u64(F::EXP_MASK | F::QUIET | payload);
                }
                end = k + 1;
            }
        }
        return Strto {
            value: negate(value, negative),
            end,
            erange: false,
        };
    }
    // Hexadecimal: "0x" followed by a valid hex significand; otherwise just the "0".
    if rest.first() == Some(&b'0') && rest.get(1).map(u8::to_ascii_lowercase) == Some(b'x') {
        return match scan_number(s, i + 2, true) {
            Some((end, _, h)) => {
                let c = hex_to_float::<F>(&h);
                Strto {
                    value: negate(c.value, negative),
                    end,
                    erange: c.glibc_erange,
                }
            }
            None => Strto {
                value: negate(zero, negative),
                end: i + 1,
                erange: false,
            },
        };
    }
    match scan_number(s, i, false) {
        Some((end, d, _)) => {
            let c = decimal_to_float::<F>(&d);
            Strto {
                value: negate(c.value, negative),
                end,
                erange: c.glibc_erange,
            }
        }
        None => no_conversion,
    }
}

/// glibc `strtol_l(s, &end, 0, C)` with a 64-bit `long` (LP64).
fn glibc_strtol_base0(s: &[u8]) -> Strto<i64> {
    let mut i = 0;
    while s.get(i).copied().is_some_and(is_c_space) {
        i += 1;
    }
    let mut negative = false;
    match s.get(i) {
        Some(b'-') => {
            negative = true;
            i += 1;
        }
        Some(b'+') => i += 1,
        _ => {}
    }
    let (value, used, overflow) = glibc_strtoull_base0(&s[i..]);
    if used == 0 {
        return Strto {
            value: 0,
            end: 0,
            erange: false,
        };
    }
    let end = i + used;
    let limit = if negative {
        i64::MIN.unsigned_abs()
    } else {
        i64::MAX as u64
    };
    if overflow || value > limit {
        return Strto {
            value: if negative { i64::MIN } else { i64::MAX },
            end,
            erange: true,
        };
    }
    let value = if negative {
        (value as i64).wrapping_neg()
    } else {
        value as i64
    };
    Strto {
        value,
        end,
        erange: false,
    }
}

// ---------------------------------------------------------------------------------------
// MSVC std::from_chars
// ---------------------------------------------------------------------------------------

/// MSVC `std::from_chars(first, last, value, fmt)` for floating point (`<charconv>`
/// `_Floating_from_chars`, `_Ordinary_floating_from_chars`, `_Infinity_from_chars`,
/// `_Nan_from_chars`; MSVC 14.44). Returns the value to store (MSVC stores infinity or zero
/// on `result_out_of_range` too) and the result.
fn msvc_from_chars_float<F: Float>(s: &[u8], hex: bool) -> (Option<F>, FromCharsResult) {
    let invalid = (
        None,
        FromCharsResult {
            ptr: 0,
            ec: Errc::InvalidArgument,
        },
    );
    let mut i = 0;
    let mut negative = false;
    if s.first() == Some(&b'-') {
        negative = true;
        i += 1;
    }
    let Some(&start) = s.get(i) else {
        return invalid;
    };
    let folded = start | 0x20;
    if folded == b'i' {
        if !starts_with_ci(&s[i + 1..], b"nf") {
            return invalid;
        }
        let mut end = i + 3;
        if starts_with_ci(&s[end..], b"inity") {
            end += 5;
        }
        let value = negate(F::from_bits_u64(F::EXP_MASK), negative);
        return (
            Some(value),
            FromCharsResult {
                ptr: end,
                ec: Errc::Ok,
            },
        );
    }
    if folded == b'n' {
        if !starts_with_ci(&s[i + 1..], b"an") {
            return invalid;
        }
        let mut end = i + 3;
        let mut quiet = true;
        if s.get(end) == Some(&b'(') {
            let seq = end + 1;
            let mut k = seq;
            while k < s.len() {
                let c = s[k];
                if c == b')' {
                    end = k + 1;
                    let body = &s[seq..k];
                    if body.len() == 3 && starts_with_ci(body, b"ind") {
                        // UCRT's "indeterminate": negative quiet NaN, parsed with or
                        // without a '-'.
                        negative = true;
                    } else if body.len() == 4 && starts_with_ci(body, b"snan") {
                        quiet = false;
                    }
                    break;
                } else if c == b'_' || c.is_ascii_alphanumeric() {
                    k += 1;
                } else {
                    break;
                }
            }
        }
        let mut bits = F::EXP_MASK | if quiet { F::QUIET } else { 1 };
        if negative {
            bits |= F::SIGN;
        }
        return (
            Some(F::from_bits_u64(bits)),
            FromCharsResult {
                ptr: end,
                ec: Errc::Ok,
            },
        );
    }
    if folded > b'f' {
        return invalid;
    }
    let Some((end, d, h)) = scan_number(s, i, hex) else {
        return invalid;
    };
    let c = if hex {
        hex_to_float::<F>(&h)
    } else {
        decimal_to_float::<F>(&d)
    };
    let ec = if c.out_of_range {
        Errc::ResultOutOfRange
    } else {
        Errc::Ok
    };
    (
        Some(negate(c.value, negative)),
        FromCharsResult { ptr: end, ec },
    )
}

/// MSVC `std::from_chars(first, last, long&, base)` (`_Integer_from_chars`) with a 32-bit
/// `long` (LLP64). The value is stored only on success.
fn msvc_from_chars_long(s: &[u8], base: u32) -> (Option<i32>, FromCharsResult) {
    let mut i = 0;
    let negative = s.first() == Some(&b'-');
    if negative {
        i += 1;
    }
    let limit: u64 = if negative { 1 << 31 } else { (1 << 31) - 1 };
    let mut value: u64 = 0;
    let mut overflow = false;
    let digits_start = i;
    while let Some(d) = s.get(i).copied().and_then(hex_value) {
        let d = u64::from(d);
        if d >= u64::from(base) {
            break;
        }
        let next = value * u64::from(base) + d;
        if next > limit {
            overflow = true;
        } else {
            value = next;
        }
        i += 1;
    }
    if i == digits_start {
        return (
            None,
            FromCharsResult {
                ptr: 0,
                ec: Errc::InvalidArgument,
            },
        );
    }
    if overflow {
        return (
            None,
            FromCharsResult {
                ptr: i,
                ec: Errc::ResultOutOfRange,
            },
        );
    }
    let v = if negative {
        (value as i64).wrapping_neg() as i32
    } else {
        value as i32
    };
    (
        Some(v),
        FromCharsResult {
            ptr: i,
            ec: Errc::Ok,
        },
    )
}

// ---------------------------------------------------------------------------------------
// NumberUtils::from_chars
// ---------------------------------------------------------------------------------------

/// `from_chars_skip_prefix` and `from_chars_hex_prefix` (NumberUtils.h:79-101): the offset
/// where `std::from_chars` starts and whether it parses hexadecimal.
fn skip_prefix(buf: &[u8], last: usize) -> (usize, bool) {
    let mut first = 0;
    while first < last && is_c_space(buf[first]) {
        first += 1;
    }
    if first < last && buf[first] == b'+' {
        first += 1;
    }
    if first + 2 < last && buf[first] == b'0' && (buf[first + 1] == b'x' || buf[first + 1] == b'X')
    {
        (first + 2, true)
    } else {
        (first, false)
    }
}

/// The C string starting at `first`: up to its first NUL, or the whole buffer.
fn c_string(buf: &[u8]) -> &[u8] {
    buf.iter().position(|&b| b == 0).map_or(buf, |n| &buf[..n])
}

fn from_chars_float<F: Float>(
    flavor: Flavor,
    buf: &[u8],
    last: usize,
    value: &mut F,
) -> FromCharsResult {
    assert!(last <= buf.len(), "last is past the buffer");
    if last == 0 {
        return FromCharsResult {
            ptr: 0,
            ec: Errc::InvalidArgument,
        };
    }
    match flavor {
        Flavor::FromChars => {
            let (first, hex) = skip_prefix(buf, last);
            let (stored, r) = msvc_from_chars_float::<F>(&buf[first..last], hex);
            if let Some(v) = stored {
                *value = v;
            }
            FromCharsResult {
                ptr: first + r.ptr,
                ec: r.ec,
            }
        }
        Flavor::Strtod => {
            let r = glibc_strtof::<F>(c_string(buf));
            // NumberUtils.h:132 (double) checks `errno != 0 && errno != EINVAL`, and :188
            // (float) `errno != 0`; glibc's strtod never sets EINVAL, so both are ERANGE.
            if r.erange {
                FromCharsResult {
                    ptr: r.end,
                    ec: Errc::ResultOutOfRange,
                }
            } else if r.end == 0 {
                FromCharsResult {
                    ptr: 0,
                    ec: Errc::InvalidArgument,
                }
            } else if r.end <= last {
                *value = r.value;
                FromCharsResult {
                    ptr: r.end,
                    ec: Errc::Ok,
                }
            } else {
                FromCharsResult {
                    ptr: 0,
                    ec: Errc::ArgumentOutOfDomain,
                }
            }
        }
    }
}

/// Port of `NumberUtils::from_chars(const char*, const char*, double&)`
/// (src/utils/NumberUtils.h:104-150 @ v2.5.2). `buf` starts at `first`; `last` is an offset
/// into it. On error the value is left unchanged, except that the MSVC branch stores
/// infinity or zero with `ResultOutOfRange`, as MSVC's `std::from_chars` does.
pub fn from_chars_f64(flavor: Flavor, buf: &[u8], last: usize, value: &mut f64) -> FromCharsResult {
    from_chars_float(flavor, buf, last, value)
}

/// Port of `NumberUtils::from_chars(const char*, const char*, float&)`
/// (src/utils/NumberUtils.h:152-206 @ v2.5.2). See [`from_chars_f64`].
pub fn from_chars_f32(flavor: Flavor, buf: &[u8], last: usize, value: &mut f32) -> FromCharsResult {
    from_chars_float(flavor, buf, last, value)
}

/// Port of `NumberUtils::from_chars(const char*, const char*, long int&)`
/// (src/utils/NumberUtils.h:208-256 @ v2.5.2). `long` is 32 bits for
/// [`Flavor::FromChars`] (Windows) and 64 bits for [`Flavor::Strtod`] (Linux); the value
/// is stored widened to `i64`. The `strtol_l` branch uses base 0, so a leading `0` means
/// octal there.
pub fn from_chars_long(
    flavor: Flavor,
    buf: &[u8],
    last: usize,
    value: &mut i64,
) -> FromCharsResult {
    assert!(last <= buf.len(), "last is past the buffer");
    if last == 0 {
        return FromCharsResult {
            ptr: 0,
            ec: Errc::InvalidArgument,
        };
    }
    match flavor {
        Flavor::FromChars => {
            let (first, hex) = skip_prefix(buf, last);
            let (stored, r) = msvc_from_chars_long(&buf[first..last], if hex { 16 } else { 10 });
            if let Some(v) = stored {
                *value = i64::from(v);
            }
            FromCharsResult {
                ptr: first + r.ptr,
                ec: r.ec,
            }
        }
        Flavor::Strtod => {
            let r = glibc_strtol_base0(c_string(buf));
            if r.erange {
                FromCharsResult {
                    ptr: r.end,
                    ec: Errc::ResultOutOfRange,
                }
            } else if r.end == 0 {
                FromCharsResult {
                    ptr: 0,
                    ec: Errc::InvalidArgument,
                }
            } else if r.end <= last {
                *value = r.value;
                FromCharsResult {
                    ptr: r.end,
                    ec: Errc::Ok,
                }
            } else {
                FromCharsResult {
                    ptr: 0,
                    ec: Errc::ArgumentOutOfDomain,
                }
            }
        }
    }
}

/// glibc's `strtod`, `strtof` and `strtol` (base 0) in the "C" locale, as this port
/// implements them for [`Flavor::Strtod`]: (value, end offset, `errno == ERANGE`). For
/// comparing with the C library.
pub mod glibc {
    /// `strtod(s, &end)` on the C string `s`.
    pub fn strtod(s: &[u8]) -> (f64, usize, bool) {
        let r = super::glibc_strtof::<f64>(super::c_string(s));
        (r.value, r.end, r.erange)
    }

    /// `strtof(s, &end)` on the C string `s`.
    pub fn strtof(s: &[u8]) -> (f32, usize, bool) {
        let r = super::glibc_strtof::<f32>(super::c_string(s));
        (r.value, r.end, r.erange)
    }

    /// `strtol(s, &end, 0)` on the C string `s`, with a 64-bit `long`.
    pub fn strtol_base0(s: &[u8]) -> (i64, usize, bool) {
        let r = super::glibc_strtol_base0(super::c_string(s));
        (r.value, r.end, r.erange)
    }
}

#[cfg(test)]
#[path = "number_utils_tests.rs"]
mod tests;
