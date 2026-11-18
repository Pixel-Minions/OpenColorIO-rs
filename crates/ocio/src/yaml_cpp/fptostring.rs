// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.9.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! Port of yaml-cpp 0.9.0's `FpToString` (`include/yaml-cpp/fptostring.h`,
//! `src/fptostring.cpp`): how the emitter writes a `float` or a `double` since 0.9.0
//! (`Emitter::WriteStreamable`, `include/yaml-cpp/emitter.h:161-197`).
//!
//! It takes dragonbox's shortest decimal of the value in its own type, cuts it to `precision`
//! significant digits, rounding up when the next digit is `'5'` or more, and writes it in
//! scientific notation when the decimal exponent is at least `precision` (6 for a precision
//! of 0) or at most -5. Zeros left by the cut are kept (I-950). Zero, infinities and NaNs go
//! through a classic-locale `std::stringstream` instead.

use ocio_ops::cfmt::{Crt, OStringStream};

use super::contrib::dragonbox::{self, DecimalFp};

/// The size of `ConvertToChars`'s digit buffers (`std::array<char, 20>`, the digits of
/// 2^64).
const DIGITS: usize = 20;

/// The size of the output buffer (`std::array<char, 28> output_buffer`,
/// src/fptostring.cpp:134).
const OUTPUT_BUFFER: usize = 28;

/// Writes the decimal digits of `value` at the end of `buf`, left-padded with `'0'` to `width`
/// digits, moves them to the front, and returns how many there are; -1 when the preconditions
/// fail.
///
/// Port of `detail::fp_formatting::ConvertToChars` (src/fptostring.cpp:31-69), on a buffer of
/// [`DIGITS`] characters (the only size its callers pass).
fn convert_to_chars(buf: &mut [u8; DIGITS], mut value: u64, width: i32) -> i32 {
    let len = buf.len() as i32;
    // defensive programming, abort if precondition are not met (will trigger in release build)
    if width < 1 {
        return -1;
    }
    if len < width {
        return -1;
    }
    if len < 20 {
        return -1;
    }

    // count number of digits, and fill digits array accordingly
    let mut digits_ct: i32 = 0;
    while value > 0 {
        let c = (value % 10) as u8 + b'0';
        value /= 10;
        digits_ct += 1;
        buf[(len - digits_ct) as usize] = c;
    }
    while digits_ct < width {
        debug_assert!(digits_ct < 64);
        digits_ct += 1;
        buf[(len - digits_ct) as usize] = b'0';
    }
    // move data to the front of the array
    buf.copy_within((len - digits_ct) as usize.., 0);
    digits_ct
}

/// A value `FpToString` formats: `float` or `double`, each through its own dragonbox path.
#[derive(Debug, Clone, Copy)]
pub enum Fp {
    /// A `float`: `FpToString(float, size_t)`.
    F32(f32),
    /// A `double`: `FpToString(double, size_t)`.
    F64(f64),
}

impl Fp {
    /// Whether the sign bit is set (an infinity is written `-.inf` then).
    pub fn is_sign_negative(self) -> bool {
        match self {
            Fp::F32(v) => v.is_sign_negative(),
            Fp::F64(v) => v.is_sign_negative(),
        }
    }

    fn is_zero(self) -> bool {
        match self {
            Fp::F32(v) => v == 0.0,
            Fp::F64(v) => v == 0.0,
        }
    }

    fn is_inf_or_nan(self) -> bool {
        match self {
            Fp::F32(v) => v.is_infinite() || v.is_nan(),
            Fp::F64(v) => v.is_infinite() || v.is_nan(),
        }
    }

    /// `std::stringstream ss; ss.imbue(std::locale::classic()); ss << v; return ss.str();`
    /// (src/fptostring.cpp:87-90, 100-103, 168-171): `operator<<` formats a `float` as the
    /// `double` it converts to.
    fn stream(self) -> String {
        let mut ss = OStringStream::new(Crt::NATIVE);
        match self {
            Fp::F32(v) => ss.put_f64(f64::from(v)),
            Fp::F64(v) => ss.put_f64(v),
        }
        String::from_utf8_lossy(ss.str()).into_owned()
    }

    /// `jkj::dragonbox::to_decimal(v)` as significand, exponent and sign.
    fn to_decimal(self) -> (u64, i32, bool) {
        match self {
            Fp::F32(v) => {
                let DecimalFp {
                    significand,
                    exponent,
                    is_negative,
                } = dragonbox::to_decimal_f32(v);
                (u64::from(significand), exponent, is_negative)
            }
            Fp::F64(v) => {
                let DecimalFp {
                    significand,
                    exponent,
                    is_negative,
                } = dragonbox::to_decimal_f64(v);
                (significand, exponent, is_negative)
            }
        }
    }
}

/// `v` as yaml-cpp 0.9.0 writes it at `precision` significant digits; a `precision` of 0 is
/// the shortest representation.
///
/// Port of `FpToString(float, size_t)` and `FpToString(double, size_t)`
/// (src/fptostring.cpp:233-239) over `detail::fp_formatting::FpToString<T>(T, int)` (76-228):
/// the `size_t` precision converts to `int` as C++ converts it.
pub fn fp_to_string(v: Fp, precision: usize) -> String {
    fp_to_string_impl(v, precision as i32)
}

/// Port of `detail::fp_formatting::FpToString<T>(T v, int precision)`
/// (src/fptostring.cpp:76-228).
fn fp_to_string_impl(v: Fp, mut precision: i32) -> String {
    // hard coded constant, at which exponent should switch to a scientific notation
    let lower_exponent_threshold: i32 = -5;
    let upper_exponent_threshold: i32 = if precision == 0 { 6 } else { precision };
    if precision == 0 {
        precision = 6;
    }

    // dragonbox/to_decimal does not handle value 0, inf, NaN
    if v.is_zero() || v.is_inf_or_nan() {
        return v.stream();
    }

    let (significand, mut r_exponent, is_negative) = v.to_decimal();

    let mut digits = [0u8; DIGITS]; // max digits of size_t is 20.
    let mut digits_ct = convert_to_chars(&mut digits, significand, 1);

    // defensive programming, ConvertToChars arguments are invalid
    if digits_ct == -1 {
        return v.stream();
    }

    // check if requested precision is lower than
    // required digits for exact representation
    if digits_ct > precision {
        let diff = digits_ct - precision;
        r_exponent += diff;
        digits_ct = precision;

        // round numbers if required
        if digits[digits_ct as usize] >= b'5' {
            let mut i = digits_ct - 1;
            digits[i as usize] += 1;
            while digits[i as usize] == b'9' + 1 {
                digits_ct -= 1;
                r_exponent += 1;
                if i > 0 {
                    digits[(i - 1) as usize] += 1;
                    i -= 1;
                } else {
                    digits_ct = 1;
                    digits[0] = b'1';
                    break;
                }
            }
        }
    }

    // Case 1 - scientific notation: max digits of size_t plus sign, a dot and 2 letters for
    // 'e+' or 'e-' and 4 letters for the exponent
    // Case 2 - default notation: require up to precision number of digits and one for a
    // potential sign
    let mut output: Vec<u8> = Vec::with_capacity(OUTPUT_BUFFER);

    // Helper variable that in Case 2 counts the overflowing number of zeros that do not fit
    // into the buffer.
    let mut overflow_zeros: i32 = 0;

    // print '-' symbol for negative numbers
    if is_negative {
        output.push(b'-');
    }

    // exponent if only a single non-zero digit is before the decimal point
    let exponent = r_exponent + digits_ct - 1;

    // case 1: scientific notation
    if exponent >= upper_exponent_threshold || exponent <= lower_exponent_threshold {
        // print first digit
        output.push(digits[0]);

        // print digits after decimal point
        if digits_ct > 1 {
            output.push(b'.');
            // print significant numbers after decimal point
            for &digit in &digits[1..digits_ct as usize] {
                output.push(digit);
            }
        }
        output.push(b'e');
        output.push(if exponent >= 0 { b'+' } else { b'-' });
        let mut exp_digits = [0u8; DIGITS];
        let exp_digits_ct =
            convert_to_chars(&mut exp_digits, u64::from(exponent.unsigned_abs()), 2);

        // defensive programming, ConvertToChars arguments are invalid
        if exp_digits_ct == -1 {
            return v.stream();
        }

        output.extend_from_slice(&exp_digits[..exp_digits_ct as usize]);

    // case 2: default notation
    } else {
        let digits_end = digits_ct as usize;
        let mut digits_iter = 0usize;

        // print digits before point
        let before_decimal_digits = digits_ct + r_exponent;
        if before_decimal_digits > 0 {
            // print non-zero digits before point
            for _ in 0..before_decimal_digits.min(digits_ct) {
                output.push(digits[digits_iter]);
                digits_iter += 1;
            }

            // number of digits that have to be zero
            let zero_digits_ct = before_decimal_digits - digits_ct;

            // space left in the output_buffer (-1 because we need it for null-termination)
            let buffer_empty_space = OUTPUT_BUFFER as i32 - output.len() as i32 - 1;

            // print all zeros not fitting into the buffer at the end of the function
            overflow_zeros = 0.max(zero_digits_ct - buffer_empty_space);

            // print trailing zeros before point
            output.extend(std::iter::repeat_n(
                b'0',
                (zero_digits_ct - overflow_zeros).max(0) as usize,
            ));

        // print 0 before point if none where printed before
        } else {
            output.push(b'0');
        }

        if digits_iter != digits_end {
            output.push(b'.');
            // print 0 after decimal point, to fill until first digits
            let after_decimal_zeros = -digits_ct - r_exponent;
            output.extend(std::iter::repeat_n(
                b'0',
                after_decimal_zeros.max(0) as usize,
            ));

            // print significant numbers after decimal point
            output.extend_from_slice(&digits[digits_iter..digits_end]);
        }
    }
    let mut ret_value = String::from_utf8(output).expect("ASCII digits");
    ret_value.extend(std::iter::repeat_n('0', overflow_zeros as usize));
    ret_value
}

#[cfg(test)]
#[path = "fptostring_tests.rs"]
mod tests;
