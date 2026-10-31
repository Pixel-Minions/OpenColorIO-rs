// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The C runtime's `atoi`, `atof` and `strtof`, in the "C" locale, as the tools call them on
//! their arguments, on each platform (PLAN.md D12). `tests/c_numbers_crt.rs` compares them with
//! the platform's own.
//!
//! - **Linux (glibc).** `atoi` is `(int) strtol(s, NULL, 10)`: a value past `long` clamps to it,
//!   then the cast keeps the low 32 bits. `atof` and `strtof` are `ocio_ops`' port of glibc's
//!   `strtod` and `strtof`.
//! - **Windows (UCRT).** `atoi` parses with `long`, 32 bits there, and clamps to it. `atof` and
//!   `strtof` skip white space and take a sign, then `inf`, `infinity` or `nan`, with
//!   `nan(snan)` a signaling NaN, `nan(ind)` the indeterminate NaN whatever the sign, and any
//!   other `nan(chars)` the quiet NaN with every payload bit set; the number itself is `ocio_ops`'
//!   port of UCRT's conversion.

/// `isspace` in the "C" locale.
fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

/// `atoi(s)`.
pub fn atoi(s: &[u8]) -> i32 {
    let mut i = 0;
    while s.get(i).copied().is_some_and(is_space) {
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
    let mut magnitude: i128 = 0;
    while let Some(digit) = s.get(i).filter(|c| c.is_ascii_digit()) {
        // Far past both limits is enough: it only has to stay there.
        magnitude = (magnitude * 10 + i128::from(digit - b'0')).min(1 << 80);
        i += 1;
    }
    let value = if negative { -magnitude } else { magnitude };
    if cfg!(windows) {
        value.clamp(i128::from(i32::MIN), i128::from(i32::MAX)) as i32
    } else {
        value.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64 as i32
    }
}

/// `atof(s)`.
pub fn atof(s: &[u8]) -> f64 {
    #[cfg(windows)]
    {
        use ocio_ops::utils::number_utils::ucrt::strtod_field;
        let special = |number: Special, negative: bool| {
            let sign = u64::from(negative) << 63;
            f64::from_bits(match number {
                Special::Infinity => sign | 0x7ff0_0000_0000_0000,
                Special::QuietNan => sign | 0x7fff_ffff_ffff_ffff,
                Special::SignalingNan => sign | 0x7ff0_0000_0000_0001,
                Special::Indeterminate => 0xfff8_0000_0000_0000,
            })
        };
        ucrt_strtox(s, |field| strtod_field(field).0, special)
    }
    #[cfg(not(windows))]
    {
        ocio_ops::utils::number_utils::glibc::strtod(s).0
    }
}

/// `strtof(s, nullptr)`.
pub fn strtof(s: &[u8]) -> f32 {
    #[cfg(windows)]
    {
        use ocio_ops::utils::number_utils::ucrt::strtof_field;
        let special = |number: Special, negative: bool| {
            let sign = u32::from(negative) << 31;
            f32::from_bits(match number {
                Special::Infinity => sign | 0x7f80_0000,
                Special::QuietNan => sign | 0x7fff_ffff,
                Special::SignalingNan => sign | 0x7f80_0001,
                Special::Indeterminate => 0xffc0_0000,
            })
        };
        ucrt_strtox(s, |field| strtof_field(field).0, special)
    }
    #[cfg(not(windows))]
    {
        ocio_ops::utils::number_utils::glibc::strtof(s).0
    }
}

/// The values UCRT's `strtod` and `strtof` spell in letters.
#[cfg(windows)]
#[derive(Clone, Copy)]
enum Special {
    Infinity,
    QuietNan,
    SignalingNan,
    Indeterminate,
}

/// Whether `s` starts with the lower-case `word`, in either case.
#[cfg(windows)]
fn starts_with_ignoring_case(s: &[u8], word: &[u8]) -> bool {
    s.len() >= word.len() && s[..word.len()].eq_ignore_ascii_case(word)
}

/// UCRT's `strtod` and `strtof`: what comes before the number (white space, a sign), and what
/// isn't a number. `number` converts the sign-less rest, `special` the spelled values.
#[cfg(windows)]
fn ucrt_strtox<F>(
    s: &[u8],
    number: impl Fn(&[u8]) -> F,
    special: impl Fn(Special, bool) -> F,
) -> F {
    let mut i = 0;
    while s.get(i).copied().is_some_and(is_space) {
        i += 1;
    }
    let start = i;
    let mut negative = false;
    match s.get(i) {
        Some(b'-') => {
            negative = true;
            i += 1;
        }
        Some(b'+') => i += 1,
        _ => {}
    }
    let rest = &s[i..];
    if starts_with_ignoring_case(rest, b"inf") {
        return special(Special::Infinity, negative);
    }
    if starts_with_ignoring_case(rest, b"nan") {
        let rest = &rest[3..];
        if let Some(inner) = rest.strip_prefix(b"(") {
            if starts_with_ignoring_case(inner, b"snan)") {
                return special(Special::SignalingNan, negative);
            }
            if starts_with_ignoring_case(inner, b"ind)") {
                return special(Special::Indeterminate, negative);
            }
        }
        return special(Special::QuietNan, negative);
    }
    number(&s[start..])
}
