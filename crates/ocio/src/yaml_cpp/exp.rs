// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.8.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! Port of the `YAML::Exp` expressions the emitter uses (src/exp.h, yaml-cpp 0.8.0). Each is
//! built once, like the C++ function-local statics, with the same operator nesting (C++ `|`
//! and `+` associate to the left, and `+` binds tighter).

use std::sync::LazyLock;

use super::regex_yaml::{RegEx, RegexOp};

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
