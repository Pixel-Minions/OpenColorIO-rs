// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The C runtime's `sscanf` as OCIO's LUT readers call it: `sscanf_s` on Windows (the
//! Universal CRT) and `sscanf` on Linux (glibc), for the directives the readers use: `%d`,
//! `%Ns`, `%c`, `%*s`, literal characters and white space (spi1d, spi3d, Iridas cube,
//! Discreet 1DL, and the CTF reader's version numbers).
//!
//! The scans run in the "C" locale, as OCIO in a C++ application does: its white space is
//! space, `\t`, `\n`, `\v`, `\f` and `\r`. The wheels scan in their process's locale, and
//! under Python the Windows wheel runs in code page 1252's, where the UCRT also takes 0xA0 for
//! white space (in the format, and what `%d` and `%s` skip first), so `Length\xa03` reads as a
//! length of 3 there and as no length in the port and on Linux (deviation D-1).
//!
//! The readers' other extractions, `istream::getline` and `istream::read`, are the input
//! stream's (`ocio/src/fileformats/input_stream.rs`, with the spimtx and spi1d readers);
//! `istream >> std::string` comes with the Houdini reader (4.3c).
//!
//! The behaviour follows C17 7.21.6.2 and what each runtime does where the standard leaves
//! room, as `ocio-testkit`'s `crt.rs` shows it on each platform (`tests/cscan_crt.rs` compares
//! generated scans). No glibc or UCRT source is translated (owner decision D3).
//!
//! - White space in the format skips any white space of the input ("C" locale `isspace`).
//! - A literal character must match the input's; at the end of the input it is an input
//!   failure, otherwise a mismatch is a matching failure.
//! - `%d`, `%Ns` and `%*s` skip white space first; `%c` doesn't.
//! - `%d` reads an optional sign and decimal digits, saturates to the 64-bit range (as
//!   `strtoll`), and stores the low 32 bits: `2147483648` is stored as -2147483648, and
//!   anything past 2^63 - 1 as -1. A sign with nothing after it is an input failure in the UCRT
//!   and a matching failure in glibc.
//! - `%Ns` stores at most N non-white-space characters and a NUL; `%*s` reads them and stores
//!   nothing.
//! - The result is the number of conversions stored; an input failure before the first one
//!   stored (even after a `%*s`) gives `EOF` (-1).
//! - The UCRT's `%d` reads a 0xFF byte as `EOF` (a signed `char` of -1), and consumes it: where
//!   `%d` needs a sign or a digit, it fails there, an input failure if the byte was the input's
//!   last and a matching failure otherwise; after digits it ends the number, and is lost.
//!   Its other directives, and glibc's, read 0xFF as any other byte.

use crate::utils::num_get::is_c_space;

/// The C runtime whose scan to follow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanCrt {
    /// The Universal CRT's `sscanf_s`: the Windows wheel.
    Ucrt,
    /// glibc's `sscanf`: the Linux wheel.
    Glibc,
}

impl ScanCrt {
    /// The runtime of the wheel for the platform being compiled for.
    #[cfg(windows)]
    pub const NATIVE: ScanCrt = ScanCrt::Ucrt;
    /// The runtime of the wheel for the platform being compiled for.
    #[cfg(not(windows))]
    pub const NATIVE: ScanCrt = ScanCrt::Glibc;
}

/// An argument of a scan: where a conversion stores its value.
#[derive(Debug)]
pub enum ScanArg<'a> {
    /// `%d`: an `int`.
    Int(&'a mut i32),
    /// `%Ns`: a buffer of at least N + 1 bytes.
    Str(&'a mut [u8]),
    /// `%c`: one `char`.
    Char(&'a mut u8),
}

/// How a scan stopped early.
enum Failure {
    /// The input ended where a directive needed more.
    Input,
    /// The input didn't match a directive.
    Matching,
}

/// `sscanf(input, format, args...)` (`sscanf_s` with each buffer's size on Windows): `input`
/// as a C string (up to its first NUL). Panics on a format the readers don't use, or on
/// arguments that don't match its conversions, as the C call would be undefined.
pub fn sscanf(crt: ScanCrt, input: &[u8], format: &[u8], args: &mut [ScanArg<'_>]) -> i32 {
    let input = &input[..input.iter().position(|&b| b == 0).unwrap_or(input.len())];
    let mut pos = 0;
    let mut stored = 0;
    let mut next_arg = 0;
    let mut f = 0;
    let stop = |failure: Failure, stored: i32| match failure {
        Failure::Input if stored == 0 => -1,
        _ => stored,
    };
    let skip_space = |pos: &mut usize| {
        while *pos < input.len() && is_c_space(input[*pos]) {
            *pos += 1;
        }
    };
    while f < format.len() {
        let c = format[f];
        if is_c_space(c) {
            while f < format.len() && is_c_space(format[f]) {
                f += 1;
            }
            skip_space(&mut pos);
            continue;
        }
        if c != b'%' {
            if pos == input.len() {
                return stop(Failure::Input, stored);
            }
            if input[pos] != c {
                return stop(Failure::Matching, stored);
            }
            pos += 1;
            f += 1;
            continue;
        }
        f += 1;
        let suppressed = format.get(f) == Some(&b'*');
        if suppressed {
            f += 1;
        }
        let start = f;
        while f < format.len() && format[f].is_ascii_digit() {
            f += 1;
        }
        let width: Option<usize> = std::str::from_utf8(&format[start..f])
            .ok()
            .and_then(|w| w.parse().ok());
        let conversion = *format.get(f).expect("a conversion after %");
        f += 1;
        match (conversion, suppressed, width) {
            (b'd', false, None) => {
                // The UCRT's `%d` reads 0xFF as `EOF`, consumed: an input failure at the end of
                // the input, else a matching failure.
                let ucrt_eof = |pos: &mut usize| -> Option<Failure> {
                    if crt == ScanCrt::Ucrt && *pos < input.len() && input[*pos] == 0xFF {
                        *pos += 1;
                        Some(if *pos == input.len() {
                            Failure::Input
                        } else {
                            Failure::Matching
                        })
                    } else {
                        None
                    }
                };
                skip_space(&mut pos);
                if pos == input.len() {
                    return stop(Failure::Input, stored);
                }
                if let Some(failure) = ucrt_eof(&mut pos) {
                    return stop(failure, stored);
                }
                let negative = input[pos] == b'-';
                if negative || input[pos] == b'+' {
                    pos += 1;
                    if pos == input.len() {
                        let failure = match crt {
                            ScanCrt::Ucrt => Failure::Input,
                            ScanCrt::Glibc => Failure::Matching,
                        };
                        return stop(failure, stored);
                    }
                    if let Some(failure) = ucrt_eof(&mut pos) {
                        return stop(failure, stored);
                    }
                }
                if !input[pos].is_ascii_digit() {
                    return stop(Failure::Matching, stored);
                }
                // The magnitude, saturated past 2^63 (enough for either sign's limit).
                let mut magnitude: u64 = 0;
                while pos < input.len() && input[pos].is_ascii_digit() {
                    magnitude = magnitude
                        .saturating_mul(10)
                        .saturating_add(u64::from(input[pos] - b'0'))
                        .min(1 << 63);
                    pos += 1;
                }
                // A 0xFF after the digits ends the number in the UCRT, and is lost.
                let _ = ucrt_eof(&mut pos);
                let value: i64 = if negative {
                    // -2^63 at most.
                    (magnitude as i128).wrapping_neg() as i64
                } else {
                    magnitude.min(i64::MAX as u64) as i64
                };
                match args.get_mut(next_arg) {
                    Some(ScanArg::Int(out)) => **out = value as i32,
                    _ => panic!("%d needs an int argument"),
                }
                next_arg += 1;
                stored += 1;
            }
            (b's', _, _) if suppressed || width.is_some_and(|w| w > 0) => {
                skip_space(&mut pos);
                if pos == input.len() {
                    return stop(Failure::Input, stored);
                }
                let limit = width.unwrap_or(usize::MAX);
                let begin = pos;
                while pos < input.len() && pos - begin < limit && !is_c_space(input[pos]) {
                    pos += 1;
                }
                if suppressed {
                    continue;
                }
                match args.get_mut(next_arg) {
                    Some(ScanArg::Str(buf)) => {
                        let n = pos - begin;
                        assert!(buf.len() > n, "a buffer of at least N + 1 bytes");
                        buf[..n].copy_from_slice(&input[begin..pos]);
                        buf[n] = 0;
                    }
                    _ => panic!("%s needs a buffer argument"),
                }
                next_arg += 1;
                stored += 1;
            }
            (b'c', false, None) => {
                if pos == input.len() {
                    return stop(Failure::Input, stored);
                }
                match args.get_mut(next_arg) {
                    Some(ScanArg::Char(out)) => **out = input[pos],
                    _ => panic!("%c needs a char argument"),
                }
                pos += 1;
                next_arg += 1;
                stored += 1;
            }
            _ => panic!(
                "unsupported scan directive in {:?}",
                String::from_utf8_lossy(format)
            ),
        }
    }
    stored
}
