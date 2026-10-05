// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The C++ standard library's number extraction, `std::istream >> n` through `std::num_get`
//! in the "C" locale, as OCIO's YAML reader (yaml-cpp 0.8.0) uses it.
//!
//! The two wheels use different libraries: MSVC's STL on Windows (`num_get::_Getifld`, then
//! `_Stolx`/`_Stoulx`), libstdc++ on Linux (`num_get::_M_extract_int`). For integers they
//! follow the same rules of the C++ standard ([istream.formatted.arithmetic],
//! [facet.num.get.virtuals]):
//! - an optional `+` or `-`;
//! - with the base field cleared (`unsetf(std::ios::dec)`, as yaml-cpp does), a leading `0x`
//!   or `0X` reads hexadecimal and a leading `0` octal; otherwise, and with `dec`, decimal;
//! - then the digits of the base, as many as there are;
//! - no digit (`0x` alone counts as none) fails with the value 0, and a value outside the
//!   type fails with its nearest limit;
//! - the end of the text sets `eofbit`.
//!
//! The port follows those rules; it doesn't translate either library. The libraries differ
//! in what they do inside these rules (MSVC keeps at most 31 significant digits, libstdc++
//! all of them) only where both fail anyway.
//!
//! Floating-point numbers differ between the wheels ([`Library`]): MSVC's `num_get` gathers
//! a field that may be hexadecimal (`0x1p3`) and fails on a result the UCRT's `strtod`
//! reports out of range, a value rounded to zero included; libstdc++ reads only decimal, and
//! keeps a value rounded to zero. The MSVC side is a translation of its STL; the libstdc++
//! side follows the standard and the Linux wheel (libstdc++ is not translated).

use crate::utils::number_utils;

/// The `ios_base::basefield` setting of the stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Basefield {
    /// `dec` (a stream's default): decimal.
    Dec,
    /// No base flag (`unsetf(std::ios::dec)`): the prefix decides.
    Auto,
}

/// What an extraction did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Extracted<T> {
    /// The stored value: the number, 0 on a failed parse, or the nearest limit on overflow.
    pub value: T,
    /// `failbit` was set.
    pub fail: bool,
    /// `eofbit` was set: the extraction reached the end of the text.
    pub eof: bool,
    /// The number of bytes read.
    pub consumed: usize,
}

/// `std::istream >> std::noskipws >> n` for an integer type whose range is `min..=max`, at
/// the start of `input`. The libraries wrap a negative number for an unsigned type; the
/// callers reject a `-` before reading an unsigned type (as yaml-cpp does), and the port
/// fails it with 0.
pub fn get_integer(input: &[u8], basefield: Basefield, min: i128, max: i128) -> Extracted<i128> {
    let mut pos = 0usize;

    // the sign
    let mut negative = false;
    if let Some(&c) = input.first()
        && (c == b'+' || c == b'-')
    {
        negative = c == b'-';
        pos += 1;
    }

    // the base, from the prefix
    let mut base: u32 = 10;
    let mut seen_digit = false;
    if input.get(pos) == Some(&b'0') {
        seen_digit = true;
        pos += 1;
        match (basefield, input.get(pos)) {
            (Basefield::Auto, Some(b'x' | b'X')) => {
                base = 16;
                seen_digit = false;
                pos += 1;
            }
            (Basefield::Auto, _) => base = 8,
            (Basefield::Dec, _) => {}
        }
    }

    // the digits
    let mut magnitude: u128 = 0;
    let mut overflow = false;
    while let Some(&c) = input.get(pos) {
        let digit = match c {
            b'0'..=b'9' => u32::from(c - b'0'),
            b'a'..=b'f' => u32::from(c - b'a') + 10,
            b'A'..=b'F' => u32::from(c - b'A') + 10,
            _ => break,
        };
        if digit >= base {
            break;
        }
        seen_digit = true;
        magnitude = magnitude * u128::from(base) + u128::from(digit);
        if magnitude > u128::from(u64::MAX) {
            overflow = true;
            magnitude = u128::from(u64::MAX) + 1;
        }
        pos += 1;
    }
    let eof = pos >= input.len();

    if !seen_digit {
        return Extracted {
            value: 0,
            fail: true,
            eof,
            consumed: pos,
        };
    }

    let value = if negative {
        -(magnitude as i128)
    } else {
        magnitude as i128
    };
    if overflow || value > max || value < min {
        // the nearest limit
        return Extracted {
            value: if value < min { min } else { max },
            fail: true,
            eof,
            consumed: pos,
        };
    }
    Extracted {
        value,
        fail: false,
        eof,
        consumed: pos,
    }
}

/// `isspace` in the "C" locale: what `std::ws` and the formatted extractors skip.
pub fn is_c_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0B | 0x0C | b'\r')
}

// ---------------------------------------------------------------------------------------
// Floating point
// ---------------------------------------------------------------------------------------

/// The C++ library that reads a floating-point number: the wheels differ here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Library {
    /// MSVC's STL (`msvcp140.dll`) and the UCRT's `strtod`: the Windows wheel.
    Msvc,
    /// libstdc++ and glibc's `strtod_l`: the Linux wheel.
    Libstdcxx,
}

impl Library {
    /// The library of the platform being compiled for: MSVC's on x86_64 Windows.
    #[cfg(all(target_arch = "x86_64", target_os = "windows", target_env = "msvc"))]
    pub const NATIVE: Library = Library::Msvc;

    /// The library of the platform being compiled for: libstdc++ on x86_64 Linux.
    #[cfg(all(target_arch = "x86_64", target_os = "linux", target_env = "gnu"))]
    pub const NATIVE: Library = Library::Libstdcxx;
}

/// `std::istream >> std::noskipws >> d` for a `double`, at the start of `input`.
pub fn get_f64(input: &[u8], library: Library) -> Extracted<f64> {
    match library {
        Library::Msvc => {
            let (field, consumed) = msvc_parse_fp(input);
            let eof = consumed >= input.len();
            match field {
                None => failed(0.0, eof, consumed),
                Some(field) => {
                    let (value, _, erange) = number_utils::ucrt::strtod_field(&field);
                    Extracted {
                        value,
                        fail: erange,
                        eof,
                        consumed,
                    }
                }
            }
        }
        Library::Libstdcxx => {
            let (field, consumed) = libstdcxx_extract_float(input);
            let eof = consumed >= input.len();
            let (value, end, _) = number_utils::glibc::strtod(&field);
            libstdcxx_convert(
                value,
                end == field.len() && !field.is_empty(),
                f64::MAX,
                eof,
                consumed,
            )
        }
    }
}

/// `std::istream >> std::noskipws >> f` for a `float`, at the start of `input`.
pub fn get_f32(input: &[u8], library: Library) -> Extracted<f32> {
    match library {
        Library::Msvc => {
            let (field, consumed) = msvc_parse_fp(input);
            let eof = consumed >= input.len();
            match field {
                None => failed(0.0, eof, consumed),
                Some(field) => {
                    let (value, _, erange) = number_utils::ucrt::strtof_field(&field);
                    Extracted {
                        value,
                        fail: erange,
                        eof,
                        consumed,
                    }
                }
            }
        }
        Library::Libstdcxx => {
            let (field, consumed) = libstdcxx_extract_float(input);
            let eof = consumed >= input.len();
            let (value, end, _) = number_utils::glibc::strtof(&field);
            libstdcxx_convert(
                value,
                end == field.len() && !field.is_empty(),
                f32::MAX,
                eof,
                consumed,
            )
        }
    }
}

fn failed<T>(value: T, eof: bool, consumed: usize) -> Extracted<T> {
    Extracted {
        value,
        fail: true,
        eof,
        consumed,
    }
}

/// libstdc++'s conversion of the gathered characters (`__convert_to_v` with `strtod_l` or
/// `strtof_l` in the "C" locale), as the Linux wheel shows it: the whole field must convert,
/// or the extraction fails with 0; an infinite result fails with the largest finite value of
/// its sign; a result too small for the type is kept (0 or a subnormal).
fn libstdcxx_convert<F>(value: F, whole: bool, max: F, eof: bool, consumed: usize) -> Extracted<F>
where
    F: Copy + PartialOrd + std::ops::Neg<Output = F> + From<i8>,
{
    let zero = F::from(0);
    if !whole {
        return failed(zero, eof, consumed);
    }
    if value > max {
        return failed(max, eof, consumed);
    }
    if value < -max {
        return failed(-max, eof, consumed);
    }
    Extracted {
        value,
        fail: false,
        eof,
        consumed,
    }
}

/// libstdc++'s stage 2 for a floating-point number in the "C" locale, as the standard
/// describes it ([facet.num.get.virtuals]) and the Linux wheel shows it: the characters it
/// gathers, and how many it read. An optional sign; digits, at most one `.`, then an
/// exponent: `e` or `E` after at least one digit of the significand (a leading zero counts),
/// an optional sign and digits. Reading stops at the first character that doesn't fit.
/// Leading zeros gather as one `0`.
pub fn libstdcxx_extract_float(input: &[u8]) -> (Vec<u8>, usize) {
    let mut field = Vec::new();
    let mut pos = 0usize;

    if let Some(&c) = input.first()
        && (c == b'+' || c == b'-')
    {
        field.push(c);
        pos += 1;
    }

    let mut found_mantissa = false;
    while input.get(pos) == Some(&b'0') {
        if !found_mantissa {
            field.push(b'0');
            found_mantissa = true;
        }
        pos += 1;
    }

    let mut found_dec = false;
    let mut found_sci = false;
    while let Some(&c) = input.get(pos) {
        if c.is_ascii_digit() {
            field.push(c);
            found_mantissa = true;
        } else if c == b'.' && !found_dec && !found_sci {
            field.push(b'.');
            found_dec = true;
        } else if (c == b'e' || c == b'E') && !found_sci && found_mantissa {
            field.push(b'e');
            found_sci = true;
            pos += 1;
            match input.get(pos) {
                Some(&s) if s == b'+' || s == b'-' => field.push(s),
                // the character after the `e` is read again at the top of the loop
                Some(_) => continue,
                None => break,
            }
        } else {
            break;
        }
        pos += 1;
    }
    (field, pos)
}

// The rest of this section translates MSVC's STL (Microsoft STL, `stl/inc/xlocnum`, as in
// MSVC 14.44), Copyright (c) Microsoft Corporation, licensed under the Apache License v2.0
// with LLVM Exception (SPDX: Apache-2.0 WITH LLVM-exception); see NOTICE. The wheel runs the
// `num_get<char>` of its msvcp140.dll; the translation reads every spelling of
// `crates/ocio/tests/yaml_cpp_convert_oracle.rs` as the Windows wheel does.

/// `_MAX_SIG_DIG_V2` (xlocnum:552): the significant digits `num_get` keeps.
const MSVC_MAX_SIG_DIG: i64 = 768;

/// Port of `num_get::_Parse_fp_with_locale` (xlocnum:803-1101, MSVC 14.44) in the "C" locale
/// (no grouping, `.` as the decimal point): the field `num_get` hands `strtod` (`None` for
/// `_Base == 0`, no digit), and how many characters it read. It keeps 768 significant
/// digits (rounding the last one up past dropped nonzero digits) and folds the rest into the
/// exponent, which it clamps.
pub fn msvc_parse_fp(input: &[u8]) -> (Option<Vec<u8>>, usize) {
    // `_Src`: the digits, then the signs, `X`/`x` and `P`/`p`.
    const SRC: &[u8] = b"0123456789ABCDEFabcdef-+XxPp";
    const OFFSET_DEC_DIGIT_END: usize = 10;
    const OFFSET_HEX_DIGIT_END: usize = 22;
    let find_elem = |c: u8| SRC.iter().position(|&s| s == c).unwrap_or(SRC.len());

    let mut buf: Vec<u8> = Vec::new();
    let mut pos = 0usize;

    if let Some(&c) = input.first() {
        if c == b'+' {
            // gather plus sign
            buf.push(b'+');
            pos += 1;
        } else if c == b'-' {
            // gather minus sign
            buf.push(b'-');
            pos += 1;
        }
    }

    buf.push(b'0'); // backstop carries from sticky bit

    let mut parse_hex = false;
    let mut seendigit = false; // seen a digit in input
    if input.get(pos) == Some(&b'0') {
        pos += 1;
        if pos >= input.len() {
            // "0" only
            return (Some(buf), pos);
        }

        if input[pos] == b'x' || input[pos] == b'X' {
            // 0x or 0X
            parse_hex = true;
            pos += 1; // discard 0x or 0X for further parsing
            buf.push(b'x');
        } else {
            seendigit = true;
        }
    }

    let mut has_unaccumulated_digits = false;
    let mut significant: i64 = 0; // number of significant digits
    let mut power_of_rep_base: i64 = 0; // power of 10 or 16

    let offset_digit_end = if parse_hex {
        OFFSET_HEX_DIGIT_END
    } else {
        OFFSET_DEC_DIGIT_END
    };
    while let Some(&c) = input.get(pos) {
        let idx = find_elem(c);
        if idx >= offset_digit_end {
            break;
        }
        if significant >= MSVC_MAX_SIG_DIG {
            power_of_rep_base += 1; // just scale by 10 or 16
            if idx > 0 {
                has_unaccumulated_digits = true;
            }
        } else if idx != 0 || significant != 0 {
            // save a significant digit
            buf.push(SRC[idx]);
            significant += 1;
        }
        seendigit = true;
        pos += 1;
    }

    if parse_hex && seendigit && significant == 0 {
        // all of the digits after the 'x' and before the decimal point are zero
        buf.push(b'0'); // save at least one leading digit for hex
    }

    if input.get(pos) == Some(&b'.') {
        // add .
        buf.push(b'.');
        pos += 1;
    }

    if significant == 0 {
        // 0000. so far: count leading fraction zeros without storing them
        while input.get(pos) == Some(&b'0') {
            power_of_rep_base -= 1;
            seendigit = true;
            pos += 1;
        }
    }

    while let Some(&c) = input.get(pos) {
        let idx = find_elem(c);
        if idx >= offset_digit_end {
            break;
        }
        if significant < MSVC_MAX_SIG_DIG {
            // save a significant fraction digit
            buf.push(SRC[idx]);
            significant += 1;
        } else if idx > 0 {
            has_unaccumulated_digits = true;
        }
        seendigit = true;
        pos += 1;
    }

    if has_unaccumulated_digits {
        // increment last digit in memory of those lost
        let mut last = buf.len() - 1;
        if buf[last] == b'.' {
            last -= 1;
        }
        let round_digit = if parse_hex { b'8' } else { b'5' };
        if buf[last] == b'0' || buf[last] == round_digit {
            buf[last] += 1;
        }
    }

    // 'e' for dec, 'p' for hex
    let (lower_exp, upper_exp) = if parse_hex {
        (b'p', b'P')
    } else {
        (b'e', b'E')
    };

    let mut exponent_part: i64 = 0;
    if seendigit
        && input
            .get(pos)
            .is_some_and(|&c| c == lower_exp || c == upper_exp)
    {
        // collect exponent
        pos += 1;
        seendigit = false;

        let mut exponent_part_negative = false;
        match input.get(pos) {
            Some(b'+') => pos += 1,
            Some(b'-') => {
                exponent_part_negative = true;
                pos += 1;
            }
            _ => {}
        }

        while input.get(pos) == Some(&b'0') {
            // strip leading zeros
            seendigit = true;
            pos += 1;
        }

        while let Some(&c) = input.get(pos) {
            let idx = find_elem(c);
            if idx >= OFFSET_DEC_DIGIT_END {
                break;
            }
            let idx = idx as i64;
            if exponent_part < i64::MAX / 10
                || (exponent_part == i64::MAX / 10 && idx <= i64::MAX % 10)
            {
                // save a significant exponent digit
                exponent_part = exponent_part * 10 + idx;
            } else {
                exponent_part = i64::MAX; // saturated
            }
            seendigit = true;
            pos += 1;
        }

        if exponent_part_negative {
            exponent_part = -exponent_part;
        }
    }

    if !seendigit {
        return (None, pos);
    }

    const DEC_EXP_ABS_BOUND: i64 = 1100; // slightly greater than 324 + 768
    const HEX_EXP_ABS_BOUND: i64 = 4200; // slightly greater than 1074 + 768 * 4

    let exp_abs_bound = if parse_hex {
        HEX_EXP_ABS_BOUND
    } else {
        DEC_EXP_ABS_BOUND
    };
    let exp_rep_abs_bound = if parse_hex {
        HEX_EXP_ABS_BOUND / 4
    } else {
        DEC_EXP_ABS_BOUND
    };
    let scale = |n: i64| if parse_hex { n * 4 } else { n };

    // basically clamp(-bound, exponent + power of the base, bound), without overflowing
    let mut power_of_rep_adjusted = power_of_rep_base;
    loop {
        if exponent_part >= 0
            && power_of_rep_adjusted >= 0
            && (exponent_part >= exp_abs_bound || power_of_rep_adjusted >= exp_rep_abs_bound)
        {
            exponent_part = exp_abs_bound;
            break;
        } else if exponent_part <= 0
            && power_of_rep_adjusted <= 0
            && (exponent_part <= -exp_abs_bound || power_of_rep_adjusted <= -exp_rep_abs_bound)
        {
            exponent_part = -exp_abs_bound;
            break;
        } else if exponent_part.abs() <= exp_abs_bound
            && power_of_rep_adjusted.abs() <= exp_rep_abs_bound
        {
            // of different signs, both small enough
            exponent_part += scale(power_of_rep_adjusted);
            exponent_part = exponent_part.clamp(-exp_abs_bound, exp_abs_bound);
            break;
        } else {
            // only enters once: of different signs, but at least one is large
            let preadjustment_round_up = if parse_hex {
                (exponent_part.abs() - 1) / 4 + 1
            } else {
                exponent_part.abs()
            };
            let exp_rep_adjustment = preadjustment_round_up.min(power_of_rep_base.abs());

            if exponent_part >= 0 {
                exponent_part -= scale(exp_rep_adjustment);
                power_of_rep_adjusted += exp_rep_adjustment;
            } else {
                exponent_part += scale(exp_rep_adjustment);
                power_of_rep_adjusted -= exp_rep_adjustment;
            }
        }
    }

    if exponent_part != 0 {
        buf.push(if parse_hex { b'p' } else { b'e' });
        if exponent_part < 0 {
            buf.push(b'-');
        }
        buf.extend_from_slice(exponent_part.unsigned_abs().to_string().as_bytes());
    }

    (Some(buf), pos)
}
