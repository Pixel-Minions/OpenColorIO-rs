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

/// Every string of up to 4 bytes over bytes of each UTF-8 class (ASCII, continuation bytes at
/// the edges of each lead's range, every kind of lead, bytes that never start a sequence).
#[cfg(windows)]
fn utf8_probes() -> Vec<Vec<u8>> {
    const CLASSES: [u8; 22] = [
        0x41, 0x00, 0x80, 0x8f, 0x90, 0x9f, 0xa0, 0xbf, 0xc0, 0xc1, 0xc2, 0xdf, 0xe0, 0xe1, 0xed,
        0xee, 0xef, 0xf0, 0xf1, 0xf4, 0xf5, 0xff,
    ];
    let mut out: Vec<Vec<u8>> = vec![Vec::new()];
    let mut level: Vec<Vec<u8>> = vec![Vec::new()];
    for _ in 0..4 {
        level = level
            .iter()
            .flat_map(|p| {
                CLASSES.iter().map(move |&c| {
                    let mut v = p.clone();
                    v.push(c);
                    v
                })
            })
            .collect();
        out.extend(level.iter().cloned());
    }
    out
}

/// `Utf8ToUtf16`'s lossy conversion against `MultiByteToWideChar(CP_UTF8, 0)`.
#[cfg(windows)]
#[test]
fn utf8_to_utf16_matches_the_system() {
    use ocio_ops::platform::utf8_to_utf16_lossy;
    for probe in utf8_probes() {
        assert_eq!(
            utf8_to_utf16_lossy(&probe),
            crt::utf8_to_utf16_system(&probe),
            "{probe:02x?}"
        );
    }
    let valid = "a\u{e9}\u{3053}\u{1f600}".as_bytes();
    assert_eq!(utf8_to_utf16_lossy(valid), crt::utf8_to_utf16_system(valid));
}

/// `Utf16ToUtf8`'s lossy conversion against `WideCharToMultiByte(CP_UTF8, 0)`: every string
/// of up to 4 units over ASCII, NUL, the surrogates' edges and others.
#[cfg(windows)]
#[test]
fn utf16_to_utf8_matches_the_system() {
    use ocio_ops::platform::utf16_to_utf8_lossy;
    const UNITS: [u16; 9] = [
        0x41, 0, 0xd800, 0xdbff, 0xdc00, 0xdfff, 0xe000, 0xfffd, 0x3053,
    ];
    let mut level: Vec<Vec<u16>> = vec![Vec::new()];
    for _ in 0..4 {
        level = level
            .iter()
            .flat_map(|p| {
                UNITS.iter().map(move |&c| {
                    let mut v = p.clone();
                    v.push(c);
                    v
                })
            })
            .collect();
        for probe in &level {
            assert_eq!(
                utf16_to_utf8_lossy(probe),
                crt::utf16_to_utf8_system(probe),
                "{probe:04x?}"
            );
        }
    }
}

/// `CreateFileContentHash` against `_wstat` (Windows) or `stat` (Linux): which paths it finds,
/// and their device (and inode on Linux). Absolute and relative paths, both separators,
/// directories with and without a trailing separator, wildcards, missing files, NUL. (Windows
/// device names such as `nul` are left out: `_wstat` finds some of them, the port none.)
#[test]
fn file_content_hash_matches_the_system() {
    use ocio_ops::platform::create_file_content_hash;
    let dir = std::env::temp_dir().join(format!("ocio_rs_hash_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("a.spi1d");
    std::fs::write(&file, b"x").unwrap();
    let dir_text = dir.to_str().unwrap().to_string();
    let file_text = file.to_str().unwrap().to_string();
    let mut probes: Vec<String> = vec![
        file_text.clone(),
        file_text.replace('\\', "/"),
        format!("{file_text}/"),
        format!("{file_text}\\"),
        dir_text.clone(),
        format!("{dir_text}/"),
        format!("{dir_text}\\"),
        format!("{dir_text}/*.spi1d"),
        format!("{dir_text}/a.spi1?"),
        format!("{dir_text}/missing"),
        format!("{dir_text}/./a.spi1d"),
        format!(
            "{dir_text}/../{}/a.spi1d",
            dir.file_name().unwrap().to_str().unwrap()
        ),
        "Cargo.toml".to_string(),
        "src".to_string(),
        "/dev/null".to_string(),
        String::new(),
        ".".to_string(),
    ];
    if cfg!(windows) {
        probes.push(file_text.to_lowercase());
        probes.push(file_text.to_uppercase());
    }
    for probe in &probes {
        let hash = create_file_content_hash(probe.as_bytes());
        #[cfg(windows)]
        {
            let wide: Vec<u16> = probe.encode_utf16().collect();
            let expected = crt::wstat_dev(&wide)
                .map(|dev| format!("{dev}:{}", ocio_testkit_fnv(probe.as_bytes())).into_bytes());
            assert_eq!(Some(hash).filter(|h| !h.is_empty()), expected, "{probe:?}");
        }
        #[cfg(target_os = "linux")]
        {
            let expected = crt::stat_dev_ino(probe.as_bytes())
                .map(|(dev, ino)| format!("{dev}:{ino}").into_bytes());
            assert_eq!(Some(hash).filter(|h| !h.is_empty()), expected, "{probe:?}");
        }
    }
    // A NUL ends the name, as in the C string.
    let with_nul = [file_text.as_bytes(), b"\0ignored"].concat();
    assert_eq!(
        create_file_content_hash(&with_nul),
        create_file_content_hash(file_text.as_bytes())
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

/// MSVC's `std::hash<std::string>` (FNV-1a), as the Windows wheel hashes the file name: the
/// constants of `<type_traits>` (`_FNV_offset_basis`, `_FNV_prime`).
#[cfg(windows)]
fn ocio_testkit_fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(14_695_981_039_346_656_037u64, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(1_099_511_628_211)
    })
}
