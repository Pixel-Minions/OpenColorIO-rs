// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `ocio_ops::platform::strcasecmp` and `strncasecmp` against the platform C runtime through
//! `ocio_testkit::crt`: `_stricmp`/`_strnicmp` (UCRT) or `strcasecmp`/`strncasecmp` (glibc),
//! the functions `Platform::Strcasecmp`/`Strncasecmp` call. Expected values come from the C
//! runtime, in this process's locale, which is the classic "C" locale: Rust never calls
//! `setlocale`. Deviation D-4 (`docs/deviations.md`): the port always compares as the "C"
//! locale does, whatever locale the host has set.
//!
//! The C functions specify only the sign of their result, so the port's `Ordering` is
//! compared with that sign. Covered: every pair of bytes, alone and between a prefix and a
//! suffix that differ only in case; NUL termination; and strings of different lengths.

use std::cmp::Ordering;

use ocio_ops::platform::{strcasecmp, strncasecmp};
use ocio_testkit::crt;

/// The C result's sign as an `Ordering`.
fn sign(c_result: i32) -> Ordering {
    c_result.cmp(&0)
}

#[track_caller]
fn check(a: &[u8], b: &[u8]) {
    assert_eq!(
        strcasecmp(a, b),
        sign(crt::strcasecmp_c(a, b)),
        "strcasecmp({a:?}, {b:?})"
    );
}

#[track_caller]
fn check_n(a: &[u8], b: &[u8], n: usize) {
    assert_eq!(
        strncasecmp(a, b, n),
        sign(crt::strncasecmp_c(a, b, n)),
        "strncasecmp({a:?}, {b:?}, {n})"
    );
}

/// Each byte against each byte: the case folding of every byte, and the order of the folded
/// bytes (compared as unsigned). Byte 0 is an empty C string.
#[test]
fn every_byte_pair() {
    for a in 0..=255u8 {
        for b in 0..=255u8 {
            check(&[a], &[b]);
            check_n(&[a], &[b], 1);

            // After a prefix and before a suffix that differ only in case: the comparison
            // goes past equal bytes, and the pair decides unless it folds to equal bytes.
            let x = [b'o', b'C', a, b'i', b'O'];
            let y = [b'O', b'c', b, b'I', b'o'];
            check(&x, &y);
            for n in 0..=6 {
                check_n(&x, &y, n);
            }
        }
    }
}

/// Each side ends at its first NUL.
#[test]
fn nul_ends_each_side() {
    let cases: &[(&[u8], &[u8])] = &[
        (b"ab\0cd", b"AB\0xy"),
        (b"ab\0", b"ab"),
        (b"AB\0", b"ab\0zz"),
        (b"\0a", b"\0b"),
        (b"\0a", b""),
        (b"a\0", b"ab"),
        (b"ab", b"a\0b"),
        (b"a\0\xff", b"A\0\x01"),
    ];
    for &(a, b) in cases {
        check(a, b);
        check(b, a);
        for n in 0..=6 {
            check_n(a, b, n);
            check_n(b, a, n);
        }
    }
}

/// Strings of different lengths, and counts shorter than, equal to and longer than them.
#[test]
fn lengths_and_counts() {
    let strings: &[&[u8]] = &[
        b"",
        b"a",
        b"A",
        b"ab",
        b"AB",
        b"abc",
        b"ABD",
        b"Z",
        b"_",
        b"[",
        b"`",
        b"{",
        b"ProcessList",
        b"processlist",
        b"PROCESSLISTS",
        b"\x80",
        b"\xc4\x8a",
        b"\xc4\x9a",
        b"\xff\xfe",
    ];
    for &a in strings {
        for &b in strings {
            check(a, b);
            for n in [0, 1, 2, 3, 4, 11, 12, 13, 100, i32::MAX as usize] {
                check_n(a, b, n);
            }
        }
    }
}
