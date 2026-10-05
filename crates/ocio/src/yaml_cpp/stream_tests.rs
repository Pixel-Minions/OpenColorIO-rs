// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.8.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! The stream's part of yaml-cpp 0.8.0 `test/integration/encoding_test.cpp`: the test's
//! document, in each of its 10 encodings, reads through the stream as the same document
//! built with the test's own `EncodeToUtf8`. (The test itself checks the scalars the parser
//! reads from it: `encoding_tests.rs`, whose encoders these tests share.)

use super::{STREAM_EOF, Stream};
use crate::yaml_cpp::parser::encoding_tests::{
    EncodingFn, encode_to_utf8, encode_to_utf16be, encode_to_utf16le, encode_to_utf32be,
    encode_to_utf32le, set_up_encoding,
};

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
    let (yaml, _) = set_up_encoding(encoding, declare_encoding);
    let (utf8, _) = set_up_encoding(encode_to_utf8, false);
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
