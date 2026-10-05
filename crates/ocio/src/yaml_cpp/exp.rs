// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.8.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! Port of the `YAML::Exp` expressions (src/exp.h, yaml-cpp 0.8.0) and the escape sequences
//! of quoted scalars (src/exp.cpp). Each expression is built once, like the C++
//! function-local statics, with the same operator nesting (C++ `|` and `+` associate to the
//! left, and `+` binds tighter).

use std::sync::LazyLock;

use super::exceptions::{Exception, Result, error_msg};
use super::mark::Mark;
use super::regex_yaml::{RegEx, RegexOp};
use super::stream::Stream;

fn any_of(chars: &[u8]) -> RegEx {
    RegEx::string(chars, RegexOp::Or)
}

fn seq_of(chars: &[u8]) -> RegEx {
    RegEx::string(chars, RegexOp::Seq)
}

/// `Exp::Space()`
pub fn space() -> RegEx {
    RegEx::ch(b' ')
}

/// `Exp::Tab()`
pub fn tab() -> RegEx {
    RegEx::ch(b'\t')
}

/// `Exp::Blank()`: `Space() | Tab()`.
pub fn blank() -> RegEx {
    space() | tab()
}

/// `Exp::Break()`: `RegEx('\n') | RegEx("\r\n") | RegEx('\r')`.
pub fn line_break() -> RegEx {
    RegEx::ch(b'\n') | seq_of(b"\r\n") | RegEx::ch(b'\r')
}

/// `Exp::BlankOrBreak()`: `Blank() | Break()`.
pub fn blank_or_break() -> RegEx {
    blank() | line_break()
}

/// `Exp::Digit()`
pub fn digit() -> RegEx {
    RegEx::range(b'0', b'9')
}

/// `Exp::Alpha()`
pub fn alpha() -> RegEx {
    RegEx::range(b'a', b'z') | RegEx::range(b'A', b'Z')
}

/// `Exp::AlphaNumeric()`
pub fn alpha_numeric() -> RegEx {
    alpha() | digit()
}

/// `Exp::Word()`: `AlphaNumeric() | RegEx('-')`.
pub fn word() -> RegEx {
    alpha_numeric() | RegEx::ch(b'-')
}

/// `Exp::Hex()`
pub fn hex() -> RegEx {
    digit() | RegEx::range(b'A', b'F') | RegEx::range(b'a', b'f')
}

/// `Exp::NotPrintable()`: code points outside c-printable (YAML 1.2, 5.1).
pub fn not_printable() -> RegEx {
    RegEx::ch(0)
        | any_of(b"\x01\x02\x03\x04\x05\x06\x07\x08\x0B\x0C\x7F")
        | RegEx::range(0x0E, 0x1F)
        | (RegEx::ch(0xC2) + (RegEx::range(0x80, 0x84) | RegEx::range(0x86, 0x9F)))
}

/// `Exp::Utf8_ByteOrderMark()`
pub fn utf8_byte_order_mark() -> RegEx {
    seq_of(b"\xEF\xBB\xBF")
}

/// `Exp::Ampersand()`
pub fn ampersand() -> RegEx {
    RegEx::ch(b'&')
}

/// `Exp::Comment()`
pub fn comment() -> RegEx {
    RegEx::ch(b'#')
}

/// `Exp::URI()`: `Word() | RegEx("#;/?:@&=+$,_.!~*'()[]", REGEX_OR) | (RegEx('%') + Hex() +
/// Hex())`.
pub static URI: LazyLock<RegEx> =
    LazyLock::new(|| word() | any_of(b"#;/?:@&=+$,_.!~*'()[]") | (RegEx::ch(b'%') + hex() + hex()));

/// `Exp::Tag()`: `Word() | RegEx("#;/?:@&=+$_.~*'()", REGEX_OR) | (RegEx('%') + Hex() +
/// Hex())`.
pub static TAG: LazyLock<RegEx> =
    LazyLock::new(|| word() | any_of(b"#;/?:@&=+$_.~*'()") | (RegEx::ch(b'%') + hex() + hex()));

/// `Exp::PlainScalar()`: a plain scalar can't start with a blank, with any of
/// `,[]{}#&*!|>'"%@` and a backtick, or (in block context) with `-?:` followed by a blank.
pub static PLAIN_SCALAR: LazyLock<RegEx> = LazyLock::new(|| {
    !(blank_or_break()
        | any_of(b",[]{}#&*!|>'\"%@`")
        | (any_of(b"-?:") + (blank_or_break() | RegEx::empty())))
});

/// `Exp::PlainScalarInFlow()`: in flow context `?` is illegal, and `:` and `-` can't be
/// followed by a blank.
pub static PLAIN_SCALAR_IN_FLOW: LazyLock<RegEx> = LazyLock::new(|| {
    !(blank_or_break()
        | any_of(b"?,[]{}#&*!|>'\"%@`")
        | (any_of(b"-:") + (blank() | RegEx::empty())))
});

/// `Exp::EndScalar()`: `RegEx(':') + (BlankOrBreak() | RegEx())`.
pub fn end_scalar() -> RegEx {
    RegEx::ch(b':') + (blank_or_break() | RegEx::empty())
}

/// `Exp::EndScalarInFlow()`: `(RegEx(':') + (BlankOrBreak() | RegEx() | RegEx(",]}",
/// REGEX_OR))) | RegEx(",?[]{}", REGEX_OR)`.
pub fn end_scalar_in_flow() -> RegEx {
    (RegEx::ch(b':') + (blank_or_break() | RegEx::empty() | any_of(b",]}"))) | any_of(b",?[]{}")
}

// The expressions the scanner matches the stream against (exp.h:23-205), built once each,
// as the C++ function-local statics are.

/// `Exp::Empty()`
pub static EMPTY: LazyLock<RegEx> = LazyLock::new(RegEx::empty);

/// `Exp::Tab()`
pub static TAB: LazyLock<RegEx> = LazyLock::new(tab);

/// `Exp::Blank()`
pub static BLANK: LazyLock<RegEx> = LazyLock::new(blank);

/// `Exp::Break()`
pub static BREAK: LazyLock<RegEx> = LazyLock::new(line_break);

/// `Exp::BlankOrBreak()`
pub static BLANK_OR_BREAK: LazyLock<RegEx> = LazyLock::new(blank_or_break);

/// `Exp::Digit()`
pub static DIGIT: LazyLock<RegEx> = LazyLock::new(digit);

/// `Exp::Word()`
pub static WORD: LazyLock<RegEx> = LazyLock::new(word);

/// `Exp::Comment()`
pub static COMMENT: LazyLock<RegEx> = LazyLock::new(comment);

/// `Exp::DocStart()`: `RegEx("---") + (BlankOrBreak() | RegEx())`.
pub static DOC_START: LazyLock<RegEx> =
    LazyLock::new(|| seq_of(b"---") + (blank_or_break() | RegEx::empty()));

/// `Exp::DocEnd()`: `RegEx("...") + (BlankOrBreak() | RegEx())`.
pub static DOC_END: LazyLock<RegEx> =
    LazyLock::new(|| seq_of(b"...") + (blank_or_break() | RegEx::empty()));

/// `Exp::DocIndicator()`: `DocStart() | DocEnd()`.
pub static DOC_INDICATOR: LazyLock<RegEx> = LazyLock::new(|| DOC_START.clone() | DOC_END.clone());

/// `Exp::BlockEntry()`: `RegEx('-') + (BlankOrBreak() | RegEx())`.
pub static BLOCK_ENTRY: LazyLock<RegEx> =
    LazyLock::new(|| RegEx::ch(b'-') + (blank_or_break() | RegEx::empty()));

/// `Exp::Key()`: `RegEx('?') + BlankOrBreak()`.
pub static KEY: LazyLock<RegEx> = LazyLock::new(|| RegEx::ch(b'?') + blank_or_break());

/// `Exp::KeyInFlow()`: `RegEx('?') + BlankOrBreak()`.
pub static KEY_IN_FLOW: LazyLock<RegEx> = LazyLock::new(|| RegEx::ch(b'?') + blank_or_break());

/// `Exp::Value()`: `RegEx(':') + (BlankOrBreak() | RegEx())`.
pub static VALUE: LazyLock<RegEx> =
    LazyLock::new(|| RegEx::ch(b':') + (blank_or_break() | RegEx::empty()));

/// `Exp::ValueInFlow()`: `RegEx(':') + (BlankOrBreak() | RegEx(",]}", REGEX_OR))`.
pub static VALUE_IN_FLOW: LazyLock<RegEx> =
    LazyLock::new(|| RegEx::ch(b':') + (blank_or_break() | any_of(b",]}")));

/// `Exp::ValueInJSONFlow()`: `RegEx(':')`.
pub static VALUE_IN_JSON_FLOW: LazyLock<RegEx> = LazyLock::new(|| RegEx::ch(b':'));

/// `Exp::Anchor()`: `!(RegEx("[]{},", REGEX_OR) | BlankOrBreak())`.
pub static ANCHOR: LazyLock<RegEx> = LazyLock::new(|| !(any_of(b"[]{},") | blank_or_break()));

/// `Exp::AnchorEnd()`: `RegEx("?:,]}%@`", REGEX_OR) | BlankOrBreak()`.
pub static ANCHOR_END: LazyLock<RegEx> = LazyLock::new(|| any_of(b"?:,]}%@`") | blank_or_break());

/// `Exp::ScanScalarEndInFlow()`: `EndScalarInFlow() | (BlankOrBreak() + Comment())`.
pub static SCAN_SCALAR_END_IN_FLOW: LazyLock<RegEx> =
    LazyLock::new(|| end_scalar_in_flow() | (blank_or_break() + comment()));

/// `Exp::ScanScalarEnd()`: `EndScalar() | (BlankOrBreak() + Comment())`.
pub static SCAN_SCALAR_END: LazyLock<RegEx> =
    LazyLock::new(|| end_scalar() | (blank_or_break() + comment()));

/// `Exp::EscSingleQuote()`: `RegEx("\'\'")`.
pub static ESC_SINGLE_QUOTE: LazyLock<RegEx> = LazyLock::new(|| seq_of(b"''"));

/// `Exp::EscBreak()`: `RegEx('\\') + Break()`.
pub static ESC_BREAK: LazyLock<RegEx> = LazyLock::new(|| RegEx::ch(b'\\') + line_break());

/// `Exp::ChompIndicator()`: `RegEx("+-", REGEX_OR)`.
fn chomp_indicator() -> RegEx {
    any_of(b"+-")
}

/// `Exp::Chomp()`: `(ChompIndicator() + Digit()) | (Digit() + ChompIndicator()) |
/// ChompIndicator() | Digit()`.
pub static CHOMP: LazyLock<RegEx> = LazyLock::new(|| {
    (chomp_indicator() + digit()) | (digit() + chomp_indicator()) | chomp_indicator() | digit()
});

/// The `YAML::Keys` characters (exp.h:208-222).
pub mod keys {
    pub const DIRECTIVE: u8 = b'%';
    pub const FLOW_SEQ_START: u8 = b'[';
    pub const FLOW_SEQ_END: u8 = b']';
    pub const FLOW_MAP_START: u8 = b'{';
    pub const FLOW_MAP_END: u8 = b'}';
    pub const FLOW_ENTRY: u8 = b',';
    pub const ALIAS: u8 = b'*';
    pub const ANCHOR: u8 = b'&';
    pub const TAG: u8 = b'!';
    pub const LITERAL_SCALAR: u8 = b'|';
    pub const FOLDED_SCALAR: u8 = b'>';
    pub const VERBATIM_TAG_START: u8 = b'<';
    pub const VERBATIM_TAG_END: u8 = b'>';
}

/// `Exp::ParseHex` (exp.cpp:13-31): the hex digits' value, wrapping past 32 bits as C++
/// `unsigned` does.
fn parse_hex(s: &[u8], mark: Mark) -> Result<u32> {
    let mut value: u32 = 0;
    for &ch in s {
        let digit = match ch {
            b'a'..=b'f' => ch - b'a' + 10,
            b'A'..=b'F' => ch - b'A' + 10,
            b'0'..=b'9' => ch - b'0',
            _ => return Err(Exception::parser(mark, error_msg::INVALID_HEX)),
        };
        value = (value << 4).wrapping_add(u32::from(digit));
    }
    Ok(value)
}

/// `Exp::Str` (exp.cpp:33): one `char`, the low byte of `ch`.
fn str_of(ch: u32) -> u8 {
    ch as u8
}

/// `Exp::Escape(Stream&, int codeLength)` (exp.cpp:35-64): the next `code_length` characters
/// as a hex code point, encoded in UTF-8.
fn escape_code(input: &mut Stream<'_>, code_length: i32) -> Result<Vec<u8>> {
    // grab string
    let mut s = Vec::new();
    for _ in 0..code_length {
        s.push(input.get());
    }

    // get the value
    let value = parse_hex(&s, input.mark())?;

    // legal unicode?
    if (0xD800..=0xDFFF).contains(&value) || value > 0x10FFFF {
        let msg = format!("{}{}", error_msg::INVALID_UNICODE, value);
        return Err(Exception::parser(input.mark(), msg));
    }

    // now break it up into chars
    Ok(if value <= 0x7F {
        vec![str_of(value)]
    } else if value <= 0x7FF {
        vec![str_of(0xC0 + (value >> 6)), str_of(0x80 + (value & 0x3F))]
    } else if value <= 0xFFFF {
        vec![
            str_of(0xE0 + (value >> 12)),
            str_of(0x80 + ((value >> 6) & 0x3F)),
            str_of(0x80 + (value & 0x3F)),
        ]
    } else {
        vec![
            str_of(0xF0 + (value >> 18)),
            str_of(0x80 + ((value >> 12) & 0x3F)),
            str_of(0x80 + ((value >> 6) & 0x3F)),
            str_of(0x80 + (value & 0x3F)),
        ]
    })
}

/// `Exp::Escape(Stream&)` (exp.cpp:66-134): the escape sequence that starts at the stream (a
/// backslash or a single quote), translated. `\N` and `\_` give one byte each (0x85 and 0xA0),
/// not their UTF-8 encodings, as in yaml-cpp.
pub fn escape(input: &mut Stream<'_>) -> Result<Vec<u8>> {
    // eat slash
    let escape = input.get();

    // switch on escape character
    let ch = input.get();

    // first do single quote, since it's easier
    if escape == b'\'' && ch == b'\'' {
        return Ok(b"'".to_vec());
    }

    // now do the slash (we're not gonna check if it's a slash - you better pass one!)
    let out: &[u8] = match ch {
        b'0' => b"\x00",
        b'a' => b"\x07",
        b'b' => b"\x08",
        b't' | b'\t' => b"\x09",
        b'n' => b"\x0A",
        b'v' => b"\x0B",
        b'f' => b"\x0C",
        b'r' => b"\x0D",
        b'e' => b"\x1B",
        b' ' => b" ",
        b'"' => b"\"",
        b'\'' => b"'",
        b'\\' => b"\\",
        b'/' => b"/",
        b'N' => b"\x85",
        b'_' => b"\xA0",
        b'L' => b"\xE2\x80\xA8", // LS (#x2028)
        b'P' => b"\xE2\x80\xA9", // PS (#x2029)
        b'x' => return escape_code(input, 2),
        b'u' => return escape_code(input, 4),
        b'U' => return escape_code(input, 8),
        _ => {
            let mut msg = error_msg::INVALID_ESCAPE.as_bytes().to_vec();
            msg.push(ch);
            return Err(Exception::parser(input.mark(), msg));
        }
    };
    Ok(out.to_vec())
}
