// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.8.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! The stream's part of yaml-cpp 0.8.0 `test/integration/encoding_test.cpp`: the test's
//! documents, in each of its 10 encodings, read through the stream, give the UTF-8 text the
//! test builds with its own `EncodeToUtf8`. (The test itself checks the scalars the parser
//! reads from them; it is ported with the parser.)

use super::{STREAM_EOF, Stream};

type EncodingFn = fn(&mut Vec<u8>, i32);

/// `Byte` (encoding_test.cpp:16-19).
fn byte(ch: i32) -> u8 {
    ch as u32 as u8
}

/// `EncodeToUtf8` (encoding_test.cpp:21-37).
fn encode_to_utf8(stream: &mut Vec<u8>, ch: i32) {
    if ch <= 0x7F {
        stream.push(byte(ch));
    } else if ch <= 0x7FF {
        stream.push(byte(0xC0 | (ch >> 6)));
        stream.push(byte(0x80 | (ch & 0x3F)));
    } else if ch <= 0xFFFF {
        stream.push(byte(0xE0 | (ch >> 12)));
        stream.push(byte(0x80 | ((ch >> 6) & 0x3F)));
        stream.push(byte(0x80 | (ch & 0x3F)));
    } else if ch <= 0x1FFFFF {
        stream.push(byte(0xF0 | (ch >> 18)));
        stream.push(byte(0x80 | ((ch >> 12) & 0x3F)));
        stream.push(byte(0x80 | ((ch >> 6) & 0x3F)));
        stream.push(byte(0x80 | (ch & 0x3F)));
    }
}

/// `SplitUtf16HighChar` (encoding_test.cpp:39-49).
fn split_utf16_high_char(stream: &mut Vec<u8>, encoding: EncodingFn, ch: i32) -> bool {
    let biased_value = ch - 0x10000;
    if biased_value < 0 {
        return false;
    }
    let high = 0xD800 | (biased_value >> 10);
    let low = 0xDC00 | (biased_value & 0x3FF);
    encoding(stream, high);
    encoding(stream, low);
    true
}

/// `EncodeToUtf16LE` (encoding_test.cpp:51-55).
fn encode_to_utf16le(stream: &mut Vec<u8>, ch: i32) {
    if !split_utf16_high_char(stream, encode_to_utf16le, ch) {
        stream.push(byte(ch & 0xFF));
        stream.push(byte(ch >> 8));
    }
}

/// `EncodeToUtf16BE` (encoding_test.cpp:57-61).
fn encode_to_utf16be(stream: &mut Vec<u8>, ch: i32) {
    if !split_utf16_high_char(stream, encode_to_utf16be, ch) {
        stream.push(byte(ch >> 8));
        stream.push(byte(ch & 0xFF));
    }
}

/// `EncodeToUtf32LE` (encoding_test.cpp:63-66).
fn encode_to_utf32le(stream: &mut Vec<u8>, ch: i32) {
    stream.extend_from_slice(&[
        byte(ch & 0xFF),
        byte((ch >> 8) & 0xFF),
        byte((ch >> 16) & 0xFF),
        byte((ch >> 24) & 0xFF),
    ]);
}

/// `EncodeToUtf32BE` (encoding_test.cpp:68-71).
fn encode_to_utf32be(stream: &mut Vec<u8>, ch: i32) {
    stream.extend_from_slice(&[
        byte((ch >> 24) & 0xFF),
        byte((ch >> 16) & 0xFF),
        byte((ch >> 8) & 0xFF),
        byte(ch & 0xFF),
    ]);
}

/// `EncodingTest::SetUpEncoding` and `AddEntry` (encoding_test.cpp:73-131): the document in
/// `encoding`, and the same document's characters (without the byte order mark) through
/// `EncodeToUtf8`.
fn set_up_encoding(encoding: EncodingFn, declare_encoding: bool) -> (Vec<u8>, Vec<u8>) {
    let mut yaml = Vec::new();
    let mut utf8 = Vec::new();
    if declare_encoding {
        encoding(&mut yaml, 0xFEFF);
    }

    let mut add_entry = |start_ch: i32, end_ch: i32| {
        for ch in [
            i32::from(b'-'),
            i32::from(b' '),
            i32::from(b'|'),
            0x0A,
            0x20,
            0x20,
        ] {
            encoding(&mut yaml, ch);
            encode_to_utf8(&mut utf8, ch);
        }
        for ch in start_ch..=end_ch {
            encoding(&mut yaml, ch);
            encode_to_utf8(&mut utf8, ch);
        }
        encoding(&mut yaml, 0x0A);
        encode_to_utf8(&mut utf8, 0x0A);
    };

    add_entry(0x0021, 0x007E); // Basic Latin
    add_entry(0x00A1, 0x00FF); // Latin-1 Supplement
    add_entry(0x0660, 0x06FF); // Arabic (largest contiguous block)

    // CJK unified ideographs (multiple lines)
    add_entry(0x4E00, 0x4EFF);
    add_entry(0x4F00, 0x4FFF);
    add_entry(0x5000, 0x51FF); // 512 character line
    add_entry(0x5200, 0x54FF); // 768 character line
    add_entry(0x5500, 0x58FF); // 1024 character line

    add_entry(0x103A0, 0x103C3); // Old Persian

    (yaml, utf8)
}

/// Every character the stream gives, up to `Stream::eof()`.
fn read_all(yaml: &[u8]) -> Vec<u8> {
    let mut stream = Stream::new(yaml);
    let mut out = Vec::new();
    while stream.peek() != STREAM_EOF {
        out.push(stream.get());
    }
    out
}

#[track_caller]
fn check(encoding: EncodingFn, declare_encoding: bool) {
    let (yaml, utf8) = set_up_encoding(encoding, declare_encoding);
    let read = read_all(&yaml);
    assert!(
        read == utf8,
        "the stream decoded {} bytes, not the test's {}",
        read.len(),
        utf8.len()
    );
}

/// The stream's part of yaml-cpp 0.8.0 `TEST_F(EncodingTest, UTF8_noBOM)`.
#[test]
fn utf8_no_bom() {
    check(encode_to_utf8, false);
}

/// The stream's part of yaml-cpp 0.8.0 `TEST_F(EncodingTest, UTF8_BOM)`.
#[test]
fn utf8_bom() {
    check(encode_to_utf8, true);
}

/// The stream's part of yaml-cpp 0.8.0 `TEST_F(EncodingTest, UTF16LE_noBOM)`.
#[test]
fn utf16le_no_bom() {
    check(encode_to_utf16le, false);
}

/// The stream's part of yaml-cpp 0.8.0 `TEST_F(EncodingTest, UTF16LE_BOM)`.
#[test]
fn utf16le_bom() {
    check(encode_to_utf16le, true);
}

/// The stream's part of yaml-cpp 0.8.0 `TEST_F(EncodingTest, UTF16BE_noBOM)`.
#[test]
fn utf16be_no_bom() {
    check(encode_to_utf16be, false);
}

/// The stream's part of yaml-cpp 0.8.0 `TEST_F(EncodingTest, UTF16BE_BOM)`.
#[test]
fn utf16be_bom() {
    check(encode_to_utf16be, true);
}

/// The stream's part of yaml-cpp 0.8.0 `TEST_F(EncodingTest, UTF32LE_noBOM)`.
#[test]
fn utf32le_no_bom() {
    check(encode_to_utf32le, false);
}

/// The stream's part of yaml-cpp 0.8.0 `TEST_F(EncodingTest, UTF32LE_BOM)`.
#[test]
fn utf32le_bom() {
    check(encode_to_utf32le, true);
}

/// The stream's part of yaml-cpp 0.8.0 `TEST_F(EncodingTest, UTF32BE_noBOM)`.
#[test]
fn utf32be_no_bom() {
    check(encode_to_utf32be, false);
}

/// The stream's part of yaml-cpp 0.8.0 `TEST_F(EncodingTest, UTF32BE_BOM)`.
#[test]
fn utf32be_bom() {
    check(encode_to_utf32be, true);
}
