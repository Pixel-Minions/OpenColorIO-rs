// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.8.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! Port of yaml-cpp 0.8.0's `YAML::Utils` (src/emitterutils.h, src/emitterutils.cpp): how a
//! string is written (plain, quoted, literal), escaping, comments, anchors and tags; plus
//! `IsNullString` (src/null.cpp) and `EncodeBase64` (src/binary.cpp).
//!
//! Strings are decoded as UTF-8 leniently, byte by byte, exactly as yaml-cpp does (a bad
//! lead byte becomes U+FFFD and is skipped; a bad trailing byte becomes U+FFFD and is read
//! again as a lead byte).

use std::sync::LazyLock;

use super::emitter_manip::EmitterManip;
use super::emitter_state::FlowType;
use super::exp;
use super::ostream_wrapper::OstreamWrapper;
use super::regex_yaml::{RegEx, Source, StringCharSource};

/// Port of `YAML::StringFormat::value` (emitterutils.h:17-19).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StringFormat {
    Plain,
    SingleQuoted,
    DoubleQuoted,
    Literal,
}

/// Port of `YAML::StringEscaping::value` (emitterutils.h:21-23).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StringEscaping {
    None,
    NonAscii,
    Json,
}

const REPLACEMENT_CHARACTER: i32 = 0xFFFD;

/// Port of `IsNullString` (null.cpp:5-8).
pub fn is_null_string(s: &[u8]) -> bool {
    s.is_empty() || s == b"~" || s == b"null" || s == b"Null" || s == b"NULL"
}

/// Port of `IsAnchorChar` (emitterutils.cpp:20-62): ns-anchor-char.
fn is_anchor_char(ch: i32) -> bool {
    match ch {
        0x2C | 0x5B | 0x5D | 0x7B | 0x7D | 0x20 | 0x09 | 0xFEFF | 0x0A | 0x0D => return false,
        0x85 => return true,
        _ => {}
    }
    if ch < 0x20 {
        return false;
    }
    if ch < 0x7E {
        return true;
    }
    if ch < 0xA0 {
        return false;
    }
    if (0xD800..=0xDFFF).contains(&ch) {
        return false;
    }
    if (ch & 0xFFFE) == 0xFFFE {
        return false;
    }
    if (0xFDD0..=0xFDEF).contains(&ch) {
        return false;
    }
    if ch > 0x10FFFF {
        return false;
    }
    true
}

/// Port of `Utf8BytesIndicated` (emitterutils.cpp:64-86).
fn utf8_bytes_indicated(ch: u8) -> i32 {
    match ch >> 4 {
        0..=7 => 1,
        12 | 13 => 2,
        14 => 3,
        15 => 4,
        _ => -1,
    }
}

/// Port of `IsTrailingByte` (emitterutils.cpp:88).
fn is_trailing_byte(ch: u8) -> bool {
    (ch & 0xC0) == 0x80
}

/// Port of `GetNextCodePointAndAdvance` (emitterutils.cpp:90-132).
fn next_code_point(s: &[u8], first: &mut usize) -> Option<i32> {
    if *first == s.len() {
        return None;
    }
    let mut n_bytes = utf8_bytes_indicated(s[*first]);
    if n_bytes < 1 {
        // Bad lead byte
        *first += 1;
        return Some(REPLACEMENT_CHARACTER);
    }
    if n_bytes == 1 {
        let code_point = i32::from(s[*first]);
        *first += 1;
        return Some(code_point);
    }

    // Gather bits from trailing bytes
    let mut code_point = i32::from(s[*first]) & !(0xFF << (7 - n_bytes));
    *first += 1;
    n_bytes -= 1;
    while n_bytes > 0 {
        if *first == s.len() || !is_trailing_byte(s[*first]) {
            code_point = REPLACEMENT_CHARACTER;
            break;
        }
        code_point <<= 6;
        code_point |= i32::from(s[*first]) & 0x3F;
        *first += 1;
        n_bytes -= 1;
    }

    // Check for illegal code points
    if code_point > 0x10FFFF
        || (0xD800..=0xDFFF).contains(&code_point)
        || (code_point & 0xFFFE) == 0xFFFE
        || (0xFDD0..=0xFDEF).contains(&code_point)
    {
        code_point = REPLACEMENT_CHARACTER;
    }
    Some(code_point)
}

/// Port of `WriteCodePoint` (emitterutils.cpp:134-153): UTF-8.
fn write_code_point(out: &mut OstreamWrapper, code_point: i32) {
    let cp = if !(0..=0x10FFFF).contains(&code_point) {
        REPLACEMENT_CHARACTER
    } else {
        code_point
    };
    if cp <= 0x7F {
        out.write_byte(cp as u8);
    } else if cp <= 0x7FF {
        out.write_bytes(&[(0xC0 | (cp >> 6)) as u8, (0x80 | (cp & 0x3F)) as u8]);
    } else if cp <= 0xFFFF {
        out.write_bytes(&[
            (0xE0 | (cp >> 12)) as u8,
            (0x80 | ((cp >> 6) & 0x3F)) as u8,
            (0x80 | (cp & 0x3F)) as u8,
        ]);
    } else {
        out.write_bytes(&[
            (0xF0 | (cp >> 18)) as u8,
            (0x80 | ((cp >> 12) & 0x3F)) as u8,
            (0x80 | ((cp >> 6) & 0x3F)) as u8,
            (0x80 | (cp & 0x3F)) as u8,
        ]);
    }
}

/// The characters that end a plain scalar (emitterutils.cpp:176-183).
fn disallowed(end_scalar: RegEx) -> RegEx {
    end_scalar
        | (exp::blank_or_break() + exp::comment())
        | exp::not_printable()
        | exp::utf8_byte_order_mark()
        | exp::line_break()
        | exp::tab()
        | exp::ampersand()
}

static DISALLOWED_FLOW: LazyLock<RegEx> = LazyLock::new(|| disallowed(exp::end_scalar_in_flow()));
static DISALLOWED_BLOCK: LazyLock<RegEx> = LazyLock::new(|| disallowed(exp::end_scalar()));

/// Port of `IsValidPlainScalar` (emitterutils.cpp:155-199).
fn is_valid_plain_scalar(s: &[u8], flow_type: FlowType, allow_only_ascii: bool) -> bool {
    // check against null
    if is_null_string(s) {
        return false;
    }

    // check the start
    let start: &RegEx = if flow_type == FlowType::Flow {
        &exp::PLAIN_SCALAR_IN_FLOW
    } else {
        &exp::PLAIN_SCALAR
    };
    if !start.matches_str(s) {
        return false;
    }

    // and check the end for plain whitespace (which can't be faithfully kept in a plain
    // scalar)
    if s.last() == Some(&b' ') {
        return false;
    }

    // then check until something is disallowed
    let disallowed: &RegEx = if flow_type == FlowType::Flow {
        &DISALLOWED_FLOW
    } else {
        &DISALLOWED_BLOCK
    };
    let mut buffer = StringCharSource::new(s);
    while buffer.is_valid() {
        if disallowed.matches(&buffer) {
            return false;
        }
        if allow_only_ascii && 0x80 <= buffer.at(0) {
            return false;
        }
        buffer.advance();
    }
    true
}

/// Port of `IsValidSingleQuotedScalar` (emitterutils.cpp:201-207).
fn is_valid_single_quoted_scalar(s: &[u8], escape_non_ascii: bool) -> bool {
    !s.iter()
        .any(|&ch| (escape_non_ascii && 0x80 <= ch) || ch == b'\n')
}

/// Port of `IsValidLiteralScalar` (emitterutils.cpp:209-219).
fn is_valid_literal_scalar(s: &[u8], flow_type: FlowType, escape_non_ascii: bool) -> bool {
    if flow_type == FlowType::Flow {
        return false;
    }
    !s.iter().any(|&ch| escape_non_ascii && 0x80 <= ch)
}

/// Port of `EncodeUTF16SurrogatePair` (emitterutils.cpp:221-228).
fn encode_utf16_surrogate_pair(code_point: i32) -> (u16, u16) {
    let lead_offset: u32 = 0xD800 - (0x10000 >> 10);
    (
        (lead_offset | (code_point >> 10) as u32) as u16,
        (0xDC00 | (code_point & 0x3FF)) as u16,
    )
}

/// Port of `WriteDoubleQuoteEscapeSequence` (emitterutils.cpp:230-254): `\xXX` below 0xFF
/// (not for JSON), `\uXXXX` below 0xFFFF, else `\UXXXXXXXX` or a JSON surrogate pair. A
/// negative value (a C++ `char` above 0x7F) prints its low bits.
fn write_double_quote_escape_sequence(
    out: &mut OstreamWrapper,
    code_point: i32,
    escaping: StringEscaping,
) {
    const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

    out.write_byte(b'\\');
    let digits = if code_point < 0xFF && escaping != StringEscaping::Json {
        out.write_byte(b'x');
        2
    } else if code_point < 0xFFFF {
        out.write_byte(b'u');
        4
    } else if escaping != StringEscaping::Json {
        out.write_byte(b'U');
        8
    } else {
        let (lead, trail) = encode_utf16_surrogate_pair(code_point);
        write_double_quote_escape_sequence(out, i32::from(lead), escaping);
        write_double_quote_escape_sequence(out, i32::from(trail), escaping);
        return;
    };

    // Write digits into the escape sequence
    for d in (1..=digits).rev() {
        out.write_byte(HEX_DIGITS[((code_point >> (4 * (d - 1))) & 0xF) as usize]);
    }
}

/// Port of `WriteAliasName` (emitterutils.cpp:256-267).
fn write_alias_name(out: &mut OstreamWrapper, s: &[u8]) -> bool {
    let mut i = 0;
    while let Some(code_point) = next_code_point(s, &mut i) {
        if !is_anchor_char(code_point) {
            return false;
        }
        write_code_point(out, code_point);
    }
    true
}

/// Port of `Utils::ComputeStringFormat` (emitterutils.cpp:270-297).
pub fn compute_string_format(
    s: &[u8],
    str_format: EmitterManip,
    flow_type: FlowType,
    escape_non_ascii: bool,
) -> StringFormat {
    match str_format {
        EmitterManip::Auto => {
            if is_valid_plain_scalar(s, flow_type, escape_non_ascii) {
                return StringFormat::Plain;
            }
            StringFormat::DoubleQuoted
        }
        EmitterManip::SingleQuoted => {
            if is_valid_single_quoted_scalar(s, escape_non_ascii) {
                return StringFormat::SingleQuoted;
            }
            StringFormat::DoubleQuoted
        }
        EmitterManip::DoubleQuoted => StringFormat::DoubleQuoted,
        EmitterManip::Literal => {
            if is_valid_literal_scalar(s, flow_type, escape_non_ascii) {
                return StringFormat::Literal;
            }
            StringFormat::DoubleQuoted
        }
        _ => StringFormat::DoubleQuoted,
    }
}

/// Port of `Utils::WriteSingleQuotedString` (emitterutils.cpp:299-317). Stops (returning
/// `false`) at a newline, which the caller never passes.
pub fn write_single_quoted_string(out: &mut OstreamWrapper, s: &[u8]) -> bool {
    out.write_byte(b'\'');
    let mut i = 0;
    while let Some(code_point) = next_code_point(s, &mut i) {
        if code_point == i32::from(b'\n') {
            return false; // We can't handle a new line and the attendant indentation yet
        }
        if code_point == i32::from(b'\'') {
            out.write_bytes(b"''");
        } else {
            write_code_point(out, code_point);
        }
    }
    out.write_byte(b'\'');
    true
}

/// Port of `Utils::WriteDoubleQuotedString` (emitterutils.cpp:319-364).
pub fn write_double_quoted_string(
    out: &mut OstreamWrapper,
    s: &[u8],
    escaping: StringEscaping,
) -> bool {
    out.write_byte(b'"');
    let mut i = 0;
    while let Some(code_point) = next_code_point(s, &mut i) {
        match code_point {
            0x22 => out.write_bytes(b"\\\""),
            0x5C => out.write_bytes(b"\\\\"),
            0x0A => out.write_bytes(b"\\n"),
            0x09 => out.write_bytes(b"\\t"),
            0x0D => out.write_bytes(b"\\r"),
            0x08 => out.write_bytes(b"\\b"),
            0x0C => out.write_bytes(b"\\f"),
            _ => {
                if code_point < 0x20 || (0x80..=0xA0).contains(&code_point) {
                    // Control characters and non-breaking space
                    write_double_quote_escape_sequence(out, code_point, escaping);
                } else if code_point == 0xFEFF {
                    // Byte order marks (ZWNS) should be escaped (YAML 1.2, sec. 5.2)
                    write_double_quote_escape_sequence(out, code_point, escaping);
                } else if escaping == StringEscaping::NonAscii && code_point > 0x7E {
                    write_double_quote_escape_sequence(out, code_point, escaping);
                } else {
                    write_code_point(out, code_point);
                }
            }
        }
    }
    out.write_byte(b'"');
    true
}

/// Port of `Utils::WriteLiteralString` (emitterutils.cpp:366-380): `|`, then each line at
/// the indent; empty lines stay empty.
pub fn write_literal_string(out: &mut OstreamWrapper, s: &[u8], indent: usize) -> bool {
    out.write_bytes(b"|\n");
    let mut i = 0;
    while let Some(code_point) = next_code_point(s, &mut i) {
        if code_point == i32::from(b'\n') {
            out.write_byte(b'\n');
        } else {
            out.indent_to(indent);
            write_code_point(out, code_point);
        }
    }
    true
}

/// Port of `Utils::WriteChar` (emitterutils.cpp:382-407): letters plain, other printable
/// ASCII quoted, the rest escaped. `ch` is a C++ `char`, signed.
pub fn write_char(out: &mut OstreamWrapper, ch: u8, escaping: StringEscaping) -> bool {
    let signed = i32::from(ch as i8);
    if ch.is_ascii_alphabetic() {
        out.write_byte(ch);
    } else if ch == b'"' {
        out.write_bytes(br#""\"""#);
    } else if ch == b'\t' {
        out.write_bytes(br#""\t""#);
    } else if ch == b'\n' {
        out.write_bytes(br#""\n""#);
    } else if ch == 0x08 {
        out.write_bytes(br#""\b""#);
    } else if ch == b'\r' {
        out.write_bytes(br#""\r""#);
    } else if ch == 0x0C {
        out.write_bytes(br#""\f""#);
    } else if ch == b'\\' {
        out.write_bytes(br#""\\""#);
    } else if (0x20..=0x7e).contains(&signed) {
        out.write_byte(b'"');
        out.write_byte(ch);
        out.write_byte(b'"');
    } else {
        out.write_byte(b'"');
        write_double_quote_escape_sequence(out, signed, escaping);
        out.write_byte(b'"');
    }
    true
}

/// Port of `Utils::WriteComment` (emitterutils.cpp:409-426): `#`, then each line continued
/// at the same column.
pub fn write_comment(out: &mut OstreamWrapper, s: &[u8], post_comment_indent: usize) -> bool {
    let cur_indent = out.col();
    out.write_byte(b'#');
    out.indentation(post_comment_indent);
    out.set_comment();
    let mut i = 0;
    while let Some(code_point) = next_code_point(s, &mut i) {
        if code_point == i32::from(b'\n') {
            out.write_byte(b'\n');
            out.indent_to(cur_indent);
            out.write_byte(b'#');
            out.indentation(post_comment_indent);
            out.set_comment();
        } else {
            write_code_point(out, code_point);
        }
    }
    true
}

/// Port of `Utils::WriteAlias` (emitterutils.cpp:428-431).
pub fn write_alias(out: &mut OstreamWrapper, s: &[u8]) -> bool {
    out.write_byte(b'*');
    write_alias_name(out, s)
}

/// Port of `Utils::WriteAnchor` (emitterutils.cpp:433-436).
pub fn write_anchor(out: &mut OstreamWrapper, s: &[u8]) -> bool {
    out.write_byte(b'&');
    write_alias_name(out, s)
}

/// Copies what `re` matches, repeatedly, until the source ends; `false` if it stops matching.
fn write_matching(out: &mut OstreamWrapper, s: &[u8], re: &RegEx) -> bool {
    let mut buffer = StringCharSource::new(s);
    while buffer.is_valid() {
        let n = re.match_source(&buffer);
        if n <= 0 {
            return false;
        }
        for _ in 0..n {
            out.write_byte(buffer.at(0));
            buffer.advance();
        }
    }
    true
}

/// Port of `Utils::WriteTag` (emitterutils.cpp:438-457): `!<uri>` (verbatim) or `!tag`.
pub fn write_tag(out: &mut OstreamWrapper, s: &[u8], verbatim: bool) -> bool {
    out.write_bytes(if verbatim { b"!<" } else { b"!" });
    let re: &RegEx = if verbatim { &exp::URI } else { &exp::TAG };
    if !write_matching(out, s, re) {
        return false;
    }
    if verbatim {
        out.write_byte(b'>');
    }
    true
}

/// Port of `Utils::WriteTagWithPrefix` (emitterutils.cpp:459-489): `!prefix!tag`.
pub fn write_tag_with_prefix(out: &mut OstreamWrapper, prefix: &[u8], tag: &[u8]) -> bool {
    out.write_byte(b'!');
    if !write_matching(out, prefix, &exp::URI) {
        return false;
    }
    out.write_byte(b'!');
    write_matching(out, tag, &exp::TAG)
}

/// Port of `EncodeBase64` (binary.cpp:6-42).
pub fn encode_base64(data: &[u8]) -> String {
    const ENCODING: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::with_capacity(4 * data.len() / 3 + 3);
    let (chunks, rest) = data.as_chunks::<3>();
    for c in chunks {
        out.push(ENCODING[usize::from(c[0] >> 2)]);
        out.push(ENCODING[usize::from(((c[0] & 0x3) << 4) | (c[1] >> 4))]);
        out.push(ENCODING[usize::from(((c[1] & 0xf) << 2) | (c[2] >> 6))]);
        out.push(ENCODING[usize::from(c[2] & 0x3f)]);
    }
    match rest {
        [a] => {
            out.push(ENCODING[usize::from(a >> 2)]);
            out.push(ENCODING[usize::from((a & 0x3) << 4)]);
            out.extend_from_slice(b"==");
        }
        [a, b] => {
            out.push(ENCODING[usize::from(a >> 2)]);
            out.push(ENCODING[usize::from(((a & 0x3) << 4) | (b >> 4))]);
            out.push(ENCODING[usize::from((b & 0xf) << 2)]);
            out.push(b'=');
        }
        _ => {}
    }
    String::from_utf8(out).expect("base64 is ASCII")
}

/// Port of `Utils::WriteBinary` (emitterutils.cpp:491-495).
pub fn write_binary(out: &mut OstreamWrapper, data: &[u8]) -> bool {
    write_double_quoted_string(out, encode_base64(data).as_bytes(), StringEscaping::None);
    true
}
