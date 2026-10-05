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
