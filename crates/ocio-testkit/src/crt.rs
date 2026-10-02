// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The platform C runtime, called through FFI: the reference that `ocio_ops::cfmt` (C and
//! iostream number formatting), `ocio_ops::utils::number_utils` (number parsing) and
//! `ocio_ops::platform`'s case-insensitive comparisons are checked against.
//!
//! On Windows this is the Universal CRT (`ucrtbase.dll`); on Linux it is glibc. These are the
//! libraries OCIO 2.5.2 itself calls: MSVC's `num_put` formats through UCRT `sprintf_s`, and
//! libstdc++'s through `vsnprintf`, while the Linux wheel parses numbers with `strtod_l`,
//! `strtof_l` and `strtol_l` in the "C" locale.
//!
//! This module is the only place outside the SIMD modules and `ocio-py` where `unsafe` is
//! allowed (PLAN.md D8, `cargo xtask guards`). Every wrapper is safe: formats are checked to
//! consume exactly one argument of the type passed, and inputs are copied into NUL-terminated
//! buffers.
#![allow(unsafe_code)]

// The declarations and conventions below (errno values, `long` width, the locale functions,
// legacy_stdio_definitions) are those of the two reference platforms (PLAN.md D11).
#[cfg(not(any(
    all(target_arch = "x86_64", target_os = "windows", target_env = "msvc"),
    all(target_arch = "x86_64", target_os = "linux", target_env = "gnu"),
)))]
compile_error!(
    "ocio-testkit's C runtime reference supports x86_64 Windows (MSVC, UCRT) and x86_64 \
     Linux (glibc) only"
);

use std::ffi::{c_char, c_int, c_long, c_void};
use std::sync::OnceLock;

/// `errno` value for a result out of range: 34 in both UCRT's `errno.h` (Windows SDK
/// 10.0.22000.0, ucrt/errno.h:79) and glibc's (asm-generic/errno-base.h:38).
pub const ERANGE: i32 = 34;

/// `errno` value for an invalid argument: 22 in both (ucrt/errno.h:78,
/// asm-generic/errno-base.h:26).
pub const EINVAL: i32 = 22;

/// The raw C declarations. The safe wrappers below are the only callers.
mod ffi {
    use std::ffi::{c_char, c_double, c_float, c_int, c_long, c_ulong, c_void};

    #[cfg_attr(windows, link(name = "legacy_stdio_definitions"))]
    unsafe extern "C" {
        // UCRT defines snprintf inline in <stdio.h>; `legacy_stdio_definitions.lib` provides
        // the out-of-line definition, which calls the same `__stdio_common_vsprintf`.
        pub(super) fn snprintf(buf: *mut c_char, size: usize, format: *const c_char, ...) -> c_int;
    }

    unsafe extern "C" {
        pub(super) fn strtod(s: *const c_char, end: *mut *mut c_char) -> c_double;
        pub(super) fn strtof(s: *const c_char, end: *mut *mut c_char) -> c_float;
        pub(super) fn strtol(s: *const c_char, end: *mut *mut c_char, base: c_int) -> c_long;
        pub(super) fn strtoul(s: *const c_char, end: *mut *mut c_char, base: c_int) -> c_ulong;
    }

    #[cfg(windows)]
    unsafe extern "C" {
        pub(super) fn _errno() -> *mut c_int;
        pub(super) fn _create_locale(category: c_int, locale: *const c_char) -> *mut c_void;
        pub(super) fn _strtod_l(
            s: *const c_char,
            end: *mut *mut c_char,
            loc: *mut c_void,
        ) -> c_double;
        pub(super) fn _strtof_l(
            s: *const c_char,
            end: *mut *mut c_char,
            loc: *mut c_void,
        ) -> c_float;
        pub(super) fn _strtol_l(
            s: *const c_char,
            end: *mut *mut c_char,
            base: c_int,
            loc: *mut c_void,
        ) -> c_long;
        pub(super) fn _stricmp(a: *const c_char, b: *const c_char) -> c_int;
        pub(super) fn _strnicmp(a: *const c_char, b: *const c_char, n: usize) -> c_int;
    }

    #[cfg(target_os = "linux")]
    unsafe extern "C" {
        pub(super) fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;
        pub(super) fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    }

    #[cfg(target_os = "linux")]
    unsafe extern "C" {
        pub(super) fn strcasecmp(a: *const c_char, b: *const c_char) -> c_int;
        pub(super) fn strncasecmp(a: *const c_char, b: *const c_char, n: usize) -> c_int;
        pub(super) fn __errno_location() -> *mut c_int;
        pub(super) fn newlocale(
            mask: c_int,
            locale: *const c_char,
            base: *mut c_void,
        ) -> *mut c_void;
        pub(super) fn strtod_l(
            s: *const c_char,
            end: *mut *mut c_char,
            loc: *mut c_void,
        ) -> c_double;
        pub(super) fn strtof_l(
            s: *const c_char,
            end: *mut *mut c_char,
            loc: *mut c_void,
        ) -> c_float;
        pub(super) fn strtol_l(
            s: *const c_char,
            end: *mut *mut c_char,
            base: c_int,
            loc: *mut c_void,
        ) -> c_long;
    }
}

fn errno_ptr() -> *mut c_int {
    // SAFETY: both functions return the calling thread's errno location, valid for the
    // thread's lifetime.
    #[cfg(windows)]
    unsafe {
        ffi::_errno()
    }
    #[cfg(target_os = "linux")]
    unsafe {
        ffi::__errno_location()
    }
}

fn set_errno(value: i32) {
    // SAFETY: errno_ptr() is valid for this thread.
    unsafe { *errno_ptr() = value }
}

fn errno() -> i32 {
    // SAFETY: errno_ptr() is valid for this thread.
    unsafe { *errno_ptr() }
}

/// The argument type a printf conversion consumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArgKind {
    Double,
    LongLong,
    UnsignedLongLong,
}

/// Checks that `format` has exactly one conversion (plus any number of `%%`), with no `*`
/// width or precision, and returns the argument type it consumes. Panics otherwise, so the
/// variadic call below can never read an argument that was not passed.
fn check_format(format: &str) -> ArgKind {
    let bytes = format.as_bytes();
    assert!(
        !bytes.contains(&0),
        "format contains a NUL byte: {format:?}"
    );
    let mut kind = None;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            i += 1;
            continue;
        }
        i += 1;
        if bytes.get(i) == Some(&b'%') {
            i += 1;
            continue;
        }
        while i < bytes.len() && b"-+ #0".contains(&bytes[i]) {
            i += 1;
        }
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if bytes.get(i) == Some(&b'.') {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
        }
        let long_long = bytes[i..].starts_with(b"ll");
        if long_long {
            i += 2;
        }
        let conv = *bytes
            .get(i)
            .unwrap_or_else(|| panic!("truncated conversion in {format:?}"));
        let this = match (conv, long_long) {
            (b'e' | b'E' | b'f' | b'F' | b'g' | b'G' | b'a' | b'A', false) => ArgKind::Double,
            (b'd' | b'i', true) => ArgKind::LongLong,
            (b'u' | b'x' | b'X' | b'o', true) => ArgKind::UnsignedLongLong,
            _ => panic!("unsupported conversion in {format:?}"),
        };
        assert!(kind.is_none(), "more than one conversion in {format:?}");
        kind = Some(this);
        i += 1;
    }
    kind.unwrap_or_else(|| panic!("no conversion in {format:?}"))
}

fn c_string(text: &str) -> Vec<u8> {
    let mut v = Vec::with_capacity(text.len() + 1);
    v.extend_from_slice(text.as_bytes());
    v.push(0);
    v
}

/// Formats with `snprintf`, growing the buffer until the output fits.
fn snprintf_with(
    format: &str,
    call: impl Fn(*mut c_char, usize, *const c_char) -> c_int,
) -> String {
    let fmt = c_string(format);
    let mut buf = vec![0u8; 64];
    loop {
        let n = call(buf.as_mut_ptr().cast(), buf.len(), fmt.as_ptr().cast());
        let n = usize::try_from(n).unwrap_or_else(|_| panic!("snprintf({format:?}) failed"));
        if n < buf.len() {
            buf.truncate(n);
            return String::from_utf8(buf).expect("printf output is ASCII");
        }
        buf = vec![0u8; n + 1];
    }
}

/// `snprintf(buf, n, format, value)` for a format with one floating-point conversion
/// (`%e %E %f %F %g %G %a %A` with flags, width and precision written in the format).
pub fn format_f64(format: &str, value: f64) -> String {
    assert_eq!(check_format(format), ArgKind::Double, "{format:?}");
    snprintf_with(format, |buf, size, fmt| {
        // SAFETY: `fmt` is NUL-terminated and consumes exactly one double (checked above);
        // `buf` has `size` writable bytes. `f64` is C `double`.
        unsafe { ffi::snprintf(buf, size, fmt, value) }
    })
}

/// `snprintf` for a format with one `long long` conversion (`%lld`, `%lli`).
pub fn format_i64(format: &str, value: i64) -> String {
    assert_eq!(check_format(format), ArgKind::LongLong, "{format:?}");
    snprintf_with(format, |buf, size, fmt| {
        // SAFETY: as in `format_f64`, for one long long (`i64`).
        unsafe { ffi::snprintf(buf, size, fmt, value) }
    })
}

/// `snprintf` for a format with one `unsigned long long` conversion (`%llu %llx %llX %llo`).
pub fn format_u64(format: &str, value: u64) -> String {
    assert_eq!(
        check_format(format),
        ArgKind::UnsignedLongLong,
        "{format:?}"
    );
    snprintf_with(format, |buf, size, fmt| {
        // SAFETY: as in `format_f64`, for one unsigned long long (`u64`).
        unsafe { ffi::snprintf(buf, size, fmt, value) }
    })
}

/// The result of a `strto*` call.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Strto<T> {
    /// The returned value.
    pub value: T,
    /// Offset of `*endptr` from the start of the input.
    pub end: usize,
    /// `errno` after the call (it is cleared before).
    pub errno: i32,
}

/// Runs a `strto*` function on `input` as a C string: the bytes up to the first NUL (or all
/// of them), followed by a NUL terminator.
fn strto<T>(input: &[u8], call: impl Fn(*const c_char, *mut *mut c_char) -> T) -> Strto<T> {
    let len = input.iter().position(|&b| b == 0).unwrap_or(input.len());
    let mut buf = Vec::with_capacity(len + 1);
    buf.extend_from_slice(&input[..len]);
    buf.push(0);
    let start = buf.as_ptr().cast::<c_char>();
    let mut end: *mut c_char = std::ptr::null_mut();
    set_errno(0);
    let value = call(start, &mut end);
    let errno = errno();
    // SAFETY: the strto* functions set *endptr to a position inside the buffer.
    let offset = unsafe { end.cast_const().offset_from(start) };
    Strto {
        value,
        end: usize::try_from(offset).expect("endptr inside the input"),
        errno,
    }
}

/// `strtod(input, &end)` in the process's locale, which is "C" (Rust never calls
/// `setlocale`).
pub fn strtod_c(input: &[u8]) -> Strto<f64> {
    // SAFETY: `s` is NUL-terminated and `end` is a valid out-pointer.
    strto(input, |s, end| unsafe { ffi::strtod(s, end) })
}

/// `strtof(input, &end)` in the "C" locale.
pub fn strtof_c(input: &[u8]) -> Strto<f32> {
    // SAFETY: as in `strtod_c`.
    strto(input, |s, end| unsafe { ffi::strtof(s, end) })
}

/// `strtol(input, &end, base)` in the "C" locale. `long` is 32 bits on Windows and 64 bits
/// on Linux; the value is widened to `i64`.
pub fn strtol_c(input: &[u8], base: i32) -> Strto<i64> {
    // SAFETY: as in `strtod_c`.
    strto(input, |s, end| {
        widen_long(unsafe { ffi::strtol(s, end, base) })
    })
}

/// `strtoul(input, &end, base)` in the "C" locale. `unsigned long` is 32 bits on Windows and
/// 64 bits on Linux; the value is widened to `u64`.
// `unsigned long` is already `u64` on Linux.
#[allow(clippy::useless_conversion)]
pub fn strtoul_c(input: &[u8], base: i32) -> Strto<u64> {
    // SAFETY: as in `strtod_c`.
    strto(input, |s, end| {
        u64::from(unsafe { ffi::strtoul(s, end, base) })
    })
}

/// libstdc++'s `std::_Hash_bytes(ptr, len, seed)` (`_ZSt11_Hash_bytesPKvmm` in
/// `libstdc++.so.6`), which `std::hash<std::string>` calls with the seed `0xc70f6907`: the
/// reference for the Linux wheel's hashes of strings. Loaded at run time, as the oracle's
/// wheel needs the library anyway; Linux only.
#[cfg(target_os = "linux")]
pub fn libstdcxx_hash_bytes(bytes: &[u8], seed: u64) -> u64 {
    type HashBytes = unsafe extern "C" fn(*const c_void, usize, usize) -> usize;
    static FUNC: OnceLock<usize> = OnceLock::new();
    let f = *FUNC.get_or_init(|| {
        // SAFETY: valid C strings; RTLD_NOW is 2 in glibc.
        let handle = unsafe { ffi::dlopen(c"libstdc++.so.6".as_ptr(), 2) };
        assert!(!handle.is_null(), "could not load libstdc++.so.6");
        // SAFETY: a handle dlopen returned and a valid C string.
        let sym = unsafe { ffi::dlsym(handle, c"_ZSt11_Hash_bytesPKvmm".as_ptr()) };
        assert!(!sym.is_null(), "no std::_Hash_bytes in libstdc++.so.6");
        sym as usize
    });
    // SAFETY: the symbol is `std::size_t std::_Hash_bytes(const void*, std::size_t,
    // std::size_t)`, which reads `len` bytes at `ptr`.
    let func: HashBytes = unsafe { std::mem::transmute::<usize, HashBytes>(f) };
    // SAFETY: as above.
    unsafe { func(bytes.as_ptr().cast(), bytes.len(), seed as usize) as u64 }
}

/// The "C" locale object for the `*_l` functions, created once, as OCIO's
/// `NumberUtils::Locale` does (`newlocale(LC_ALL_MASK, "C", NULL)` / `_create_locale`).
fn c_locale() -> *mut c_void {
    struct Loc(*mut c_void);
    // SAFETY: a locale object is immutable once created and may be used from any thread.
    unsafe impl Send for Loc {}
    // SAFETY: as above.
    unsafe impl Sync for Loc {}
    static LOC: OnceLock<Loc> = OnceLock::new();
    LOC.get_or_init(|| {
        let name = c"C";
        #[cfg(target_os = "linux")]
        // SAFETY: LC_ALL_MASK (glibc: every category bit except LC_ALL's) with a valid name
        // and no base locale.
        let loc = unsafe { ffi::newlocale(0x1FBF, name.as_ptr(), std::ptr::null_mut()) };
        #[cfg(windows)]
        // SAFETY: LC_ALL is 0 in UCRT; the name is a valid C string.
        let loc = unsafe { ffi::_create_locale(0, name.as_ptr()) };
        assert!(!loc.is_null(), "could not create the C locale");
        Loc(loc)
    })
    .0
}

/// `strtod_l(input, &end, C)`: the call the Linux wheel's `NumberUtils::from_chars(double)`
/// makes (`_strtod_l` on Windows, which the Windows wheel does not use).
pub fn strtod_l(input: &[u8]) -> Strto<f64> {
    let loc = c_locale();
    #[cfg(target_os = "linux")]
    // SAFETY: as in `strtod_c`, with a valid locale object.
    let r = strto(input, |s, end| unsafe { ffi::strtod_l(s, end, loc) });
    #[cfg(windows)]
    // SAFETY: as above.
    let r = strto(input, |s, end| unsafe { ffi::_strtod_l(s, end, loc) });
    r
}

/// `strtof_l(input, &end, C)`: the Linux wheel's `NumberUtils::from_chars(float)`.
pub fn strtof_l(input: &[u8]) -> Strto<f32> {
    let loc = c_locale();
    #[cfg(target_os = "linux")]
    // SAFETY: as in `strtod_l`.
    let r = strto(input, |s, end| unsafe { ffi::strtof_l(s, end, loc) });
    #[cfg(windows)]
    // SAFETY: as above.
    let r = strto(input, |s, end| unsafe { ffi::_strtof_l(s, end, loc) });
    r
}

/// `strtol_l(input, &end, base, C)`: the Linux wheel's `NumberUtils::from_chars(long)` uses
/// base 0. `long` is widened to `i64`.
pub fn strtol_l(input: &[u8], base: i32) -> Strto<i64> {
    let loc = c_locale();
    #[cfg(target_os = "linux")]
    // SAFETY: as in `strtod_l`.
    let r = strto(input, |s, end| {
        widen_long(unsafe { ffi::strtol_l(s, end, base, loc) })
    });
    #[cfg(windows)]
    // SAFETY: as above.
    let r = strto(input, |s, end| {
        widen_long(unsafe { ffi::_strtol_l(s, end, base, loc) })
    });
    r
}

/// C `long` widened to `i64` (a no-op on Linux, where `long` is already 64 bits).
#[allow(clippy::useless_conversion)]
fn widen_long(v: c_long) -> i64 {
    i64::from(v)
}

/// `bytes` with a NUL appended: a C string that the C function reads up to the first NUL,
/// which may be inside `bytes`.
fn nul_terminated(bytes: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(bytes.len() + 1);
    v.extend_from_slice(bytes);
    v.push(0);
    v
}

/// `_stricmp(a, b)` (UCRT) or `strcasecmp(a, b)` (glibc) in the process's locale, which is
/// "C" (Rust never calls `setlocale`): `Platform::Strcasecmp` as OCIO calls it. Each side is
/// the C string `bytes` + NUL, so it ends at its first NUL. Only the sign of the result is
/// specified.
pub fn strcasecmp_c(a: &[u8], b: &[u8]) -> i32 {
    let (a, b) = (nul_terminated(a), nul_terminated(b));
    #[cfg(windows)]
    // SAFETY: both arguments are NUL-terminated buffers that outlive the call.
    let r = unsafe { ffi::_stricmp(a.as_ptr().cast(), b.as_ptr().cast()) };
    #[cfg(target_os = "linux")]
    // SAFETY: as above.
    let r = unsafe { ffi::strcasecmp(a.as_ptr().cast(), b.as_ptr().cast()) };
    r
}

/// `_strnicmp(a, b, n)` (UCRT) or `strncasecmp(a, b, n)` (glibc) in the "C" locale, as
/// [`strcasecmp_c`]: `Platform::Strncasecmp` as OCIO calls it. `n` must be at most
/// `i32::MAX`, which UCRT requires.
pub fn strncasecmp_c(a: &[u8], b: &[u8], n: usize) -> i32 {
    assert!(
        n <= i32::MAX as usize,
        "UCRT's _strnicmp refuses a count of {n}"
    );
    let (a, b) = (nul_terminated(a), nul_terminated(b));
    #[cfg(windows)]
    // SAFETY: both arguments are NUL-terminated buffers that outlive the call; the functions
    // stop at a NUL, so `n` may exceed their lengths.
    let r = unsafe { ffi::_strnicmp(a.as_ptr().cast(), b.as_ptr().cast(), n) };
    #[cfg(target_os = "linux")]
    // SAFETY: as above.
    let r = unsafe { ffi::strncasecmp(a.as_ptr().cast(), b.as_ptr().cast(), n) };
    r
}

/// The number of bits in C `long` on this platform (32 on Windows, 64 on Linux).
pub const LONG_BITS: u32 = c_long::BITS;

#[cfg(test)]
mod tests {
    //! Self-checks of the FFI plumbing. They compare the C runtime with itself (round trips,
    //! the locale and plain variants, the C standard's error conventions); the port is
    //! compared with the C runtime in `ocio-ops`.
    use super::*;

    /// `%.17g` keeps DBL_DECIMAL_DIG (17 for IEEE double) significant digits, enough for any
    /// double to come back unchanged (C17 5.2.4.2.2), and strtod rounds text of up to
    /// DECIMAL_DIG digits correctly (C17 7.22.1.3p9; 17 on UCRT, 21 on glibc x86-64); the
    /// whole text is read (p5).
    #[test]
    fn format_round_trips_through_strtod() {
        let mut rng = crate::probe::Rng::new(1);
        for _ in 0..10_000 {
            let x = f64::from_bits(rng.next_u64());
            if !x.is_finite() {
                continue;
            }
            let text = format_f64("%.17g", x);
            let parsed = strtod_c(text.as_bytes());
            assert_eq!(parsed.value.to_bits(), x.to_bits(), "{text}");
            assert_eq!(parsed.end, text.len(), "{text}");
        }
    }

    /// A test process never calls `setlocale`, so the "C" locale is in effect (C17
    /// 7.11.1.1p4), and the plain functions must agree with the "C"-locale variants.
    #[test]
    fn locale_variants_agree_with_plain_ones() {
        for text in [
            "1.5",
            "  -2.25e3xyz",
            "0x1p-3",
            "inf",
            "nan",
            "1e999",
            "junk",
            "",
        ] {
            let a = strtod_c(text.as_bytes());
            let b = strtod_l(text.as_bytes());
            assert_eq!(
                (a.value.to_bits(), a.end),
                (b.value.to_bits(), b.end),
                "{text}"
            );
            let a = strtof_c(text.as_bytes());
            let b = strtof_l(text.as_bytes());
            assert_eq!(
                (a.value.to_bits(), a.end),
                (b.value.to_bits(), b.end),
                "{text}"
            );
        }
        for text in ["12", "-0x1F", "017", "9999999999999999999999"] {
            assert_eq!(strtol_c(text.as_bytes(), 0), strtol_l(text.as_bytes(), 0));
        }
    }

    /// C17 7.22.1.3: all of "1e999" is the subject sequence (p2-p4), so the end is after it
    /// (p5); the value overflows, so strtod returns HUGE_VAL and stores ERANGE in errno
    /// (p10); HUGE_VAL is positive infinity on IEC 60559 platforms (C17 F.10p2).
    #[test]
    fn overflow_sets_erange() {
        let input = b"1e999";
        let r = strtod_c(input);
        assert_eq!(r.value.to_bits(), f64::INFINITY.to_bits());
        assert_eq!(r.errno, ERANGE);
        assert_eq!(r.end, input.len());
    }

    /// C17 7.22.1.3: "x1" does not begin with a subject sequence (p3-p4), so no conversion is
    /// performed and the end is the start of the input (p7), and zero is returned (p10).
    #[test]
    fn no_conversion_leaves_end_at_start() {
        let r = strtod_c(b"x1");
        assert_eq!((r.value.to_bits(), r.end), (0.0f64.to_bits(), 0));
    }

    /// The input is a C string, which ends at its first null character (C17 7.1.1p1): what
    /// follows a NUL is never read, so the result is that of the text before it.
    #[test]
    fn input_stops_at_nul() {
        let with_tail = strtod_c(b"12\x003");
        let alone = strtod_c(b"12");
        assert_eq!(
            (with_tail.value.to_bits(), with_tail.end, with_tail.errno),
            (alone.value.to_bits(), alone.end, alone.errno)
        );
    }

    /// C17 7.21.6.1p8: `u` and `x` write an unsigned value in decimal and lowercase
    /// hexadecimal, `d` a signed value in decimal; `0` pads with leading zeros to the width
    /// (p6, p4). Rust's `Display` and `LowerHex` define the same digits.
    #[test]
    fn integer_formats_agree_with_rust() {
        let mut rng = crate::probe::Rng::new(2);
        for _ in 0..1000 {
            let v = rng.next_u64();
            assert_eq!(format_u64("%llu", v), v.to_string());
            assert_eq!(format_u64("%016llx", v), format!("{v:016x}"));
            assert_eq!(format_i64("%lld", v as i64), (v as i64).to_string());
        }
    }

    #[test]
    #[should_panic(expected = "more than one conversion")]
    fn two_conversions_are_rejected() {
        format_f64("%g %g", 1.0);
    }

    #[test]
    #[should_panic(expected = "unsupported conversion")]
    fn star_precision_is_rejected() {
        format_f64("%.*g", 1.0);
    }
}
