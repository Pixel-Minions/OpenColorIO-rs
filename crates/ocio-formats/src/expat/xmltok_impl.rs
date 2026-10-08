// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from expat 2.7.2 (MIT License, Copyright (c) 1998-2000 Thai Open Source Software Center
// Ltd and Clark Cooper, Copyright (c) 2001-2025 Expat maintainers); see mod.rs.

//! The tokenizer's scanners: a port of `lib/xmltok_impl.c` and `xmltok_impl.h` (expat 2.7.2).
//!
//! expat compiles `xmltok_impl.c` three times, with macros that read one encoding's
//! characters: `normal_` (one byte per character: UTF-8, ISO-8859-1, US-ASCII), `little2_` and
//! `big2_` (UTF-16). Here each scanner is one generic function over [`Kind`], whose methods
//! are those macros (`MINBPC`, `BYTE_TYPE`, `BYTE_TO_ASCII`, `CHAR_MATCHES`, `IS_NAME_CHAR`,
//! `IS_NMSTRT_CHAR`, `IS_INVALID_CHAR` and their `_MINBPC` forms) as `xmltok.c` defines them
//! for a build without `XML_MIN_SIZE` (expat's default, and the wheels').
//!
//! A scanner reads `b[ptr..end]` and returns a token (`XML_TOK_*`); where upstream sets
//! `*nextTokPtr`, the port sets `*next`, an index into `b`. The ASCII constants of `ascii.h`
//! are the ASCII byte literals (`ASCII_x` is `b'x'`).
//!
//! The functions that read a token the scanners already accepted (`getAtts`, `nameLength`,
//! `skipS`, `charRefNumber`) read without bounds upstream, relying on its delimiters; here a
//! read past `b` panics, which a tokenized buffer never reaches.
//!
//! expat is built with `XML_NS` (namespace processing compiled in), so the scanners have
//! `BT_COLON` cases; the encodings without namespace processing, the only ones OCIO's parsers
//! use, never give `BT_COLON` (their tables map `:` to `BT_NMSTRT`), and those cases are not
//! ported.

use super::xmltok::{Attribute, Encoding, Position, check_char_ref_number};

// The byte types (xmltok_impl.h:33-71).
pub(super) const BT_NONXML: u8 = 0;
pub(super) const BT_MALFORM: u8 = 1;
pub(super) const BT_LT: u8 = 2;
pub(super) const BT_AMP: u8 = 3;
pub(super) const BT_RSQB: u8 = 4;
pub(super) const BT_LEAD2: u8 = 5;
pub(super) const BT_LEAD3: u8 = 6;
pub(super) const BT_LEAD4: u8 = 7;
pub(super) const BT_TRAIL: u8 = 8;
pub(super) const BT_CR: u8 = 9;
pub(super) const BT_LF: u8 = 10;
pub(super) const BT_GT: u8 = 11;
pub(super) const BT_QUOT: u8 = 12;
pub(super) const BT_APOS: u8 = 13;
pub(super) const BT_EQUALS: u8 = 14;
pub(super) const BT_QUEST: u8 = 15;
pub(super) const BT_EXCL: u8 = 16;
pub(super) const BT_SOL: u8 = 17;
pub(super) const BT_SEMI: u8 = 18;
pub(super) const BT_NUM: u8 = 19;
pub(super) const BT_LSQB: u8 = 20;
pub(super) const BT_S: u8 = 21;
pub(super) const BT_NMSTRT: u8 = 22;
/// `BT_COLON`: a colon in the encodings with namespace processing, which are not ported.
#[allow(dead_code)]
pub(super) const BT_COLON: u8 = 23;
pub(super) const BT_HEX: u8 = 24;
pub(super) const BT_DIGIT: u8 = 25;
pub(super) const BT_NAME: u8 = 26;
pub(super) const BT_MINUS: u8 = 27;
pub(super) const BT_OTHER: u8 = 28;
pub(super) const BT_NONASCII: u8 = 29;
pub(super) const BT_PERCNT: u8 = 30;
pub(super) const BT_LPAR: u8 = 31;
pub(super) const BT_RPAR: u8 = 32;
pub(super) const BT_AST: u8 = 33;
pub(super) const BT_PLUS: u8 = 34;
pub(super) const BT_COMMA: u8 = 35;
pub(super) const BT_VERBAR: u8 = 36;

// The tokens (xmltok.h:42-130).
pub const XML_TOK_TRAILING_RSQB: i32 = -5;
pub const XML_TOK_NONE: i32 = -4;
pub const XML_TOK_TRAILING_CR: i32 = -3;
pub const XML_TOK_PARTIAL_CHAR: i32 = -2;
pub const XML_TOK_PARTIAL: i32 = -1;
pub const XML_TOK_INVALID: i32 = 0;
pub const XML_TOK_START_TAG_WITH_ATTS: i32 = 1;
pub const XML_TOK_START_TAG_NO_ATTS: i32 = 2;
pub const XML_TOK_EMPTY_ELEMENT_WITH_ATTS: i32 = 3;
pub const XML_TOK_EMPTY_ELEMENT_NO_ATTS: i32 = 4;
pub const XML_TOK_END_TAG: i32 = 5;
pub const XML_TOK_DATA_CHARS: i32 = 6;
pub const XML_TOK_DATA_NEWLINE: i32 = 7;
pub const XML_TOK_CDATA_SECT_OPEN: i32 = 8;
pub const XML_TOK_ENTITY_REF: i32 = 9;
pub const XML_TOK_CHAR_REF: i32 = 10;
pub const XML_TOK_PI: i32 = 11;
pub const XML_TOK_XML_DECL: i32 = 12;
pub const XML_TOK_COMMENT: i32 = 13;
pub const XML_TOK_BOM: i32 = 14;
pub const XML_TOK_PROLOG_S: i32 = 15;
pub const XML_TOK_DECL_OPEN: i32 = 16;
pub const XML_TOK_DECL_CLOSE: i32 = 17;
pub const XML_TOK_NAME: i32 = 18;
pub const XML_TOK_NMTOKEN: i32 = 19;
pub const XML_TOK_POUND_NAME: i32 = 20;
pub const XML_TOK_OR: i32 = 21;
pub const XML_TOK_PERCENT: i32 = 22;
pub const XML_TOK_OPEN_PAREN: i32 = 23;
pub const XML_TOK_CLOSE_PAREN: i32 = 24;
pub const XML_TOK_OPEN_BRACKET: i32 = 25;
pub const XML_TOK_CLOSE_BRACKET: i32 = 26;
pub const XML_TOK_LITERAL: i32 = 27;
pub const XML_TOK_PARAM_ENTITY_REF: i32 = 28;
pub const XML_TOK_INSTANCE_START: i32 = 29;
pub const XML_TOK_NAME_QUESTION: i32 = 30;
pub const XML_TOK_NAME_ASTERISK: i32 = 31;
pub const XML_TOK_NAME_PLUS: i32 = 32;
pub const XML_TOK_COND_SECT_OPEN: i32 = 33;
pub const XML_TOK_COND_SECT_CLOSE: i32 = 34;
pub const XML_TOK_CLOSE_PAREN_QUESTION: i32 = 35;
pub const XML_TOK_CLOSE_PAREN_ASTERISK: i32 = 36;
pub const XML_TOK_CLOSE_PAREN_PLUS: i32 = 37;
pub const XML_TOK_COMMA: i32 = 38;
pub const XML_TOK_ATTRIBUTE_VALUE_S: i32 = 39;
pub const XML_TOK_CDATA_SECT_CLOSE: i32 = 40;
pub const XML_TOK_PREFIXED_NAME: i32 = 41;
pub const XML_TOK_IGNORE_SECT: i32 = 42;

/// The character macros of one compilation of `xmltok_impl.c` (xmltok.c:233-303, 738-812,
/// 871-945).
pub(super) trait Kind {
    /// `MINBPC(enc)`: the minimum bytes per character.
    const MINBPC: usize;
    /// `BYTE_TYPE(enc, p)`.
    fn byte_type(enc: &Encoding, b: &[u8], p: usize) -> u8;
    /// `BYTE_TO_ASCII(enc, p)`.
    fn byte_to_ascii(b: &[u8], p: usize) -> i32;
    /// `CHAR_MATCHES(enc, p, c)`, `c` an ASCII character.
    fn char_matches(b: &[u8], p: usize, c: u8) -> bool;
    /// `IS_NAME_CHAR(enc, p, n)`: the `n`-byte character at `p` is a name character.
    fn is_name_char(b: &[u8], p: usize, n: usize) -> bool;
    /// `IS_NMSTRT_CHAR(enc, p, n)`.
    fn is_nmstrt_char(b: &[u8], p: usize, n: usize) -> bool;
    /// `IS_INVALID_CHAR(enc, p, n)`.
    fn is_invalid_char(b: &[u8], p: usize, n: usize) -> bool;
    /// `IS_NAME_CHAR_MINBPC(enc, p)`.
    fn is_name_char_minbpc(b: &[u8], p: usize) -> bool;
    /// `IS_NMSTRT_CHAR_MINBPC(enc, p)`.
    fn is_nmstrt_char_minbpc(b: &[u8], p: usize) -> bool;
}

/// `HAS_CHARS(enc, ptr, end, count)`.
#[inline]
fn has_chars<K: Kind>(ptr: usize, end: usize, count: usize) -> bool {
    end >= ptr && end - ptr >= count * K::MINBPC
}

/// `HAS_CHAR(enc, ptr, end)`.
#[inline]
fn has_char<K: Kind>(ptr: usize, end: usize) -> bool {
    has_chars::<K>(ptr, end, 1)
}

/// What a case macro did with the character at `ptr`: moved past it, or returned a token
/// (having set `*next` where upstream does).
type Step = Result<usize, i32>;

/// `INVALID_LEAD_CASE` and `INVALID_CASES` (xmltok_impl.c:48-67): `None` for the other byte
/// types.
fn invalid_cases<K: Kind>(
    bt: u8,
    b: &[u8],
    ptr: usize,
    end: usize,
    next: &mut usize,
) -> Option<Step> {
    let n = match bt {
        BT_LEAD2 => 2,
        BT_LEAD3 => 3,
        BT_LEAD4 => 4,
        BT_NONXML | BT_MALFORM | BT_TRAIL => {
            *next = ptr;
            return Some(Err(XML_TOK_INVALID));
        }
        _ => return None,
    };
    if end - ptr < n {
        return Some(Err(XML_TOK_PARTIAL_CHAR));
    }
    if K::is_invalid_char(b, ptr, n) {
        *next = ptr;
        return Some(Err(XML_TOK_INVALID));
    }
    Some(Ok(ptr + n))
}

/// `CHECK_NAME_CASE` and `CHECK_NAME_CASES` (xmltok_impl.c:69-96): `None` for the other byte
/// types.
fn check_name_cases<K: Kind>(
    bt: u8,
    b: &[u8],
    ptr: usize,
    end: usize,
    next: &mut usize,
) -> Option<Step> {
    let n = match bt {
        BT_NONASCII => {
            if !K::is_name_char_minbpc(b, ptr) {
                *next = ptr;
                return Some(Err(XML_TOK_INVALID));
            }
            return Some(Ok(ptr + K::MINBPC));
        }
        BT_NMSTRT | BT_HEX | BT_DIGIT | BT_NAME | BT_MINUS => return Some(Ok(ptr + K::MINBPC)),
        BT_LEAD2 => 2,
        BT_LEAD3 => 3,
        BT_LEAD4 => 4,
        _ => return None,
    };
    if end - ptr < n {
        return Some(Err(XML_TOK_PARTIAL_CHAR));
    }
    if K::is_invalid_char(b, ptr, n) || !K::is_name_char(b, ptr, n) {
        *next = ptr;
        return Some(Err(XML_TOK_INVALID));
    }
    Some(Ok(ptr + n))
}

/// `CHECK_NMSTRT_CASE` and `CHECK_NMSTRT_CASES` (xmltok_impl.c:98-122): `None` for the other
/// byte types.
fn check_nmstrt_cases<K: Kind>(
    bt: u8,
    b: &[u8],
    ptr: usize,
    end: usize,
    next: &mut usize,
) -> Option<Step> {
    let n = match bt {
        BT_NONASCII => {
            if !K::is_nmstrt_char_minbpc(b, ptr) {
                *next = ptr;
                return Some(Err(XML_TOK_INVALID));
            }
            return Some(Ok(ptr + K::MINBPC));
        }
        BT_NMSTRT | BT_HEX => return Some(Ok(ptr + K::MINBPC)),
        BT_LEAD2 => 2,
        BT_LEAD3 => 3,
        BT_LEAD4 => 4,
        _ => return None,
    };
    if end - ptr < n {
        return Some(Err(XML_TOK_PARTIAL_CHAR));
    }
    if K::is_invalid_char(b, ptr, n) || !K::is_nmstrt_char(b, ptr, n) {
        *next = ptr;
        return Some(Err(XML_TOK_INVALID));
    }
    Some(Ok(ptr + n))
}

/// Applies a case macro's step: `ptr` moves on, or the scanner returns.
macro_rules! step {
    ($ptr:ident, $s:expr) => {
        match $s {
            Ok(p) => $ptr = p,
            Err(tok) => return tok,
        }
    };
}

/// `REQUIRE_CHARS(enc, ptr, end, count)`.
macro_rules! require_chars {
    ($k:ty, $ptr:expr, $end:expr, $count:expr) => {
        if !has_chars::<$k>($ptr, $end, $count) {
            return XML_TOK_PARTIAL;
        }
    };
}

/// `end` shrunk to whole characters, as the scanners' heads do for `MINBPC > 1`; `None` when
/// no whole character is left.
#[inline]
fn whole_chars<K: Kind>(ptr: usize, end: usize) -> Option<usize> {
    if K::MINBPC > 1 {
        let mut n = end - ptr;
        if n & (K::MINBPC - 1) != 0 {
            n &= !(K::MINBPC - 1);
            if n == 0 {
                return None;
            }
            return Some(ptr + n);
        }
    }
    Some(end)
}

/// After "<!-": a comment.
///
/// Port of `PREFIX(scanComment)` (xmltok_impl.c:144-179).
fn scan_comment<K: Kind>(
    enc: &Encoding,
    b: &[u8],
    mut ptr: usize,
    end: usize,
    next: &mut usize,
) -> i32 {
    if has_char::<K>(ptr, end) {
        if !K::char_matches(b, ptr, b'-') {
            *next = ptr;
            return XML_TOK_INVALID;
        }
        ptr += K::MINBPC;
        while has_char::<K>(ptr, end) {
            let bt = K::byte_type(enc, b, ptr);
            if let Some(s) = invalid_cases::<K>(bt, b, ptr, end, next) {
                step!(ptr, s);
                continue;
            }
            match bt {
                BT_MINUS => {
                    ptr += K::MINBPC;
                    require_chars!(K, ptr, end, 1);
                    if K::char_matches(b, ptr, b'-') {
                        ptr += K::MINBPC;
                        require_chars!(K, ptr, end, 1);
                        if !K::char_matches(b, ptr, b'>') {
                            *next = ptr;
                            return XML_TOK_INVALID;
                        }
                        *next = ptr + K::MINBPC;
                        return XML_TOK_COMMENT;
                    }
                }
                _ => ptr += K::MINBPC,
            }
        }
    }
    XML_TOK_PARTIAL
}

/// After "<!": a declaration, a comment or a conditional section.
///
/// Port of `PREFIX(scanDecl)` (xmltok_impl.c:181-228).
fn scan_decl<K: Kind>(
    enc: &Encoding,
    b: &[u8],
    mut ptr: usize,
    end: usize,
    next: &mut usize,
) -> i32 {
    require_chars!(K, ptr, end, 1);
    match K::byte_type(enc, b, ptr) {
        BT_MINUS => return scan_comment::<K>(enc, b, ptr + K::MINBPC, end, next),
        BT_LSQB => {
            *next = ptr + K::MINBPC;
            return XML_TOK_COND_SECT_OPEN;
        }
        BT_NMSTRT | BT_HEX => ptr += K::MINBPC,
        _ => {
            *next = ptr;
            return XML_TOK_INVALID;
        }
    }
    while has_char::<K>(ptr, end) {
        match K::byte_type(enc, b, ptr) {
            bt @ (BT_PERCNT | BT_S | BT_CR | BT_LF) => {
                if bt == BT_PERCNT {
                    require_chars!(K, ptr, end, 2);
                    // don't allow <!ENTITY% foo "whatever">
                    if let BT_S | BT_CR | BT_LF | BT_PERCNT = K::byte_type(enc, b, ptr + K::MINBPC)
                    {
                        *next = ptr;
                        return XML_TOK_INVALID;
                    }
                }
                *next = ptr;
                return XML_TOK_DECL_OPEN;
            }
            BT_NMSTRT | BT_HEX => ptr += K::MINBPC,
            _ => {
                *next = ptr;
                return XML_TOK_INVALID;
            }
        }
    }
    XML_TOK_PARTIAL
}

/// Whether the target `b[ptr..end]` of a processing instruction may stand (any case of "xml"
/// but "xml" itself may not), and its token in `*tok`: `XML_TOK_XML_DECL` for "xml",
/// `XML_TOK_PI` otherwise.
///
/// Port of `PREFIX(checkPiTarget)` (xmltok_impl.c:230-272).
fn check_pi_target<K: Kind>(b: &[u8], mut ptr: usize, end: usize, tok: &mut i32) -> bool {
    let mut upper = false;
    *tok = XML_TOK_PI;
    if end - ptr != K::MINBPC * 3 {
        return true;
    }
    for (lower, upper_case) in [(b'x', b'X'), (b'm', b'M'), (b'l', b'L')] {
        let c = K::byte_to_ascii(b, ptr);
        if c == i32::from(lower) {
        } else if c == i32::from(upper_case) {
            upper = true;
        } else {
            return true;
        }
        ptr += K::MINBPC;
    }
    if upper {
        return false;
    }
    *tok = XML_TOK_XML_DECL;
    true
}

/// After "<?": a processing instruction or an XML declaration.
///
/// Port of `PREFIX(scanPi)` (xmltok_impl.c:275-333).
fn scan_pi<K: Kind>(enc: &Encoding, b: &[u8], mut ptr: usize, end: usize, next: &mut usize) -> i32 {
    let mut tok = 0;
    let target = ptr;
    require_chars!(K, ptr, end, 1);
    let bt = K::byte_type(enc, b, ptr);
    match check_nmstrt_cases::<K>(bt, b, ptr, end, next) {
        Some(s) => step!(ptr, s),
        None => {
            *next = ptr;
            return XML_TOK_INVALID;
        }
    }
    while has_char::<K>(ptr, end) {
        let bt = K::byte_type(enc, b, ptr);
        if let Some(s) = check_name_cases::<K>(bt, b, ptr, end, next) {
            step!(ptr, s);
            continue;
        }
        match bt {
            BT_S | BT_CR | BT_LF => {
                if !check_pi_target::<K>(b, target, ptr, &mut tok) {
                    *next = ptr;
                    return XML_TOK_INVALID;
                }
                ptr += K::MINBPC;
                while has_char::<K>(ptr, end) {
                    let bt = K::byte_type(enc, b, ptr);
                    if let Some(s) = invalid_cases::<K>(bt, b, ptr, end, next) {
                        step!(ptr, s);
                        continue;
                    }
                    match bt {
                        BT_QUEST => {
                            ptr += K::MINBPC;
                            require_chars!(K, ptr, end, 1);
                            if K::char_matches(b, ptr, b'>') {
                                *next = ptr + K::MINBPC;
                                return tok;
                            }
                        }
                        _ => ptr += K::MINBPC,
                    }
                }
                return XML_TOK_PARTIAL;
            }
            BT_QUEST => {
                if !check_pi_target::<K>(b, target, ptr, &mut tok) {
                    *next = ptr;
                    return XML_TOK_INVALID;
                }
                ptr += K::MINBPC;
                require_chars!(K, ptr, end, 1);
                if K::char_matches(b, ptr, b'>') {
                    *next = ptr + K::MINBPC;
                    return tok;
                }
                *next = ptr;
                return XML_TOK_INVALID;
            }
            _ => {
                *next = ptr;
                return XML_TOK_INVALID;
            }
        }
    }
    XML_TOK_PARTIAL
}

/// After "<![": "CDATA[".
///
/// Port of `PREFIX(scanCdataSection)` (xmltok_impl.c:335-352).
fn scan_cdata_section<K: Kind>(b: &[u8], mut ptr: usize, end: usize, next: &mut usize) -> i32 {
    require_chars!(K, ptr, end, 6);
    for c in *b"CDATA[" {
        if !K::char_matches(b, ptr, c) {
            *next = ptr;
            return XML_TOK_INVALID;
        }
        ptr += K::MINBPC;
    }
    *next = ptr;
    XML_TOK_CDATA_SECT_OPEN
}

/// A token in a CDATA section: its end, a newline, or characters.
///
/// Port of `PREFIX(cdataSectionTok)` (xmltok_impl.c:354-428).
pub(super) fn cdata_section_tok<K: Kind>(
    enc: &Encoding,
    b: &[u8],
    mut ptr: usize,
    end: usize,
    next: &mut usize,
) -> i32 {
    if ptr >= end {
        return XML_TOK_NONE;
    }
    let Some(end) = whole_chars::<K>(ptr, end) else {
        return XML_TOK_PARTIAL;
    };
    let bt = K::byte_type(enc, b, ptr);
    match bt {
        BT_RSQB => {
            ptr += K::MINBPC;
            require_chars!(K, ptr, end, 1);
            if K::char_matches(b, ptr, b']') {
                ptr += K::MINBPC;
                require_chars!(K, ptr, end, 1);
                if !K::char_matches(b, ptr, b'>') {
                    ptr -= K::MINBPC;
                } else {
                    *next = ptr + K::MINBPC;
                    return XML_TOK_CDATA_SECT_CLOSE;
                }
            }
        }
        BT_CR => {
            ptr += K::MINBPC;
            require_chars!(K, ptr, end, 1);
            if K::byte_type(enc, b, ptr) == BT_LF {
                ptr += K::MINBPC;
            }
            *next = ptr;
            return XML_TOK_DATA_NEWLINE;
        }
        BT_LF => {
            *next = ptr + K::MINBPC;
            return XML_TOK_DATA_NEWLINE;
        }
        _ => match invalid_cases::<K>(bt, b, ptr, end, next) {
            Some(s) => step!(ptr, s),
            None => ptr += K::MINBPC,
        },
    }
    while has_char::<K>(ptr, end) {
        let bt = K::byte_type(enc, b, ptr);
        match bt {
            BT_LEAD2 | BT_LEAD3 | BT_LEAD4 => {
                let n = usize::from(bt - BT_LEAD2) + 2;
                if end - ptr < n || K::is_invalid_char(b, ptr, n) {
                    *next = ptr;
                    return XML_TOK_DATA_CHARS;
                }
                ptr += n;
            }
            BT_NONXML | BT_MALFORM | BT_TRAIL | BT_CR | BT_LF | BT_RSQB => {
                *next = ptr;
                return XML_TOK_DATA_CHARS;
            }
            _ => ptr += K::MINBPC,
        }
    }
    *next = ptr;
    XML_TOK_DATA_CHARS
}

/// After "</": an end tag.
///
/// Port of `PREFIX(scanEndTag)` (xmltok_impl.c:430-479).
fn scan_end_tag<K: Kind>(
    enc: &Encoding,
    b: &[u8],
    mut ptr: usize,
    end: usize,
    next: &mut usize,
) -> i32 {
    require_chars!(K, ptr, end, 1);
    let bt = K::byte_type(enc, b, ptr);
    match check_nmstrt_cases::<K>(bt, b, ptr, end, next) {
        Some(s) => step!(ptr, s),
        None => {
            *next = ptr;
            return XML_TOK_INVALID;
        }
    }
    while has_char::<K>(ptr, end) {
        let bt = K::byte_type(enc, b, ptr);
        if let Some(s) = check_name_cases::<K>(bt, b, ptr, end, next) {
            step!(ptr, s);
            continue;
        }
        match bt {
            BT_S | BT_CR | BT_LF => {
                ptr += K::MINBPC;
                while has_char::<K>(ptr, end) {
                    match K::byte_type(enc, b, ptr) {
                        BT_S | BT_CR | BT_LF => {}
                        BT_GT => {
                            *next = ptr + K::MINBPC;
                            return XML_TOK_END_TAG;
                        }
                        _ => {
                            *next = ptr;
                            return XML_TOK_INVALID;
                        }
                    }
                    ptr += K::MINBPC;
                }
                return XML_TOK_PARTIAL;
            }
            BT_GT => {
                *next = ptr + K::MINBPC;
                return XML_TOK_END_TAG;
            }
            _ => {
                *next = ptr;
                return XML_TOK_INVALID;
            }
        }
    }
    XML_TOK_PARTIAL
}

/// After "&#x": a hexadecimal character reference.
///
/// Port of `PREFIX(scanHexCharRef)` (xmltok_impl.c:481-510).
fn scan_hex_char_ref<K: Kind>(
    enc: &Encoding,
    b: &[u8],
    mut ptr: usize,
    end: usize,
    next: &mut usize,
) -> i32 {
    if has_char::<K>(ptr, end) {
        match K::byte_type(enc, b, ptr) {
            BT_DIGIT | BT_HEX => {}
            _ => {
                *next = ptr;
                return XML_TOK_INVALID;
            }
        }
        ptr += K::MINBPC;
        while has_char::<K>(ptr, end) {
            match K::byte_type(enc, b, ptr) {
                BT_DIGIT | BT_HEX => {}
                BT_SEMI => {
                    *next = ptr + K::MINBPC;
                    return XML_TOK_CHAR_REF;
                }
                _ => {
                    *next = ptr;
                    return XML_TOK_INVALID;
                }
            }
            ptr += K::MINBPC;
        }
    }
    XML_TOK_PARTIAL
}

/// After "&#": a character reference.
///
/// Port of `PREFIX(scanCharRef)` (xmltok_impl.c:512-541).
fn scan_char_ref<K: Kind>(
    enc: &Encoding,
    b: &[u8],
    mut ptr: usize,
    end: usize,
    next: &mut usize,
) -> i32 {
    if has_char::<K>(ptr, end) {
        if K::char_matches(b, ptr, b'x') {
            return scan_hex_char_ref::<K>(enc, b, ptr + K::MINBPC, end, next);
        }
        match K::byte_type(enc, b, ptr) {
            BT_DIGIT => {}
            _ => {
                *next = ptr;
                return XML_TOK_INVALID;
            }
        }
        ptr += K::MINBPC;
        while has_char::<K>(ptr, end) {
            match K::byte_type(enc, b, ptr) {
                BT_DIGIT => {}
                BT_SEMI => {
                    *next = ptr + K::MINBPC;
                    return XML_TOK_CHAR_REF;
                }
                _ => {
                    *next = ptr;
                    return XML_TOK_INVALID;
                }
            }
            ptr += K::MINBPC;
        }
    }
    XML_TOK_PARTIAL
}

/// After "&": an entity or character reference.
///
/// Port of `PREFIX(scanRef)` (xmltok_impl.c:543-569).
fn scan_ref<K: Kind>(
    enc: &Encoding,
    b: &[u8],
    mut ptr: usize,
    end: usize,
    next: &mut usize,
) -> i32 {
    require_chars!(K, ptr, end, 1);
    let bt = K::byte_type(enc, b, ptr);
    match check_nmstrt_cases::<K>(bt, b, ptr, end, next) {
        Some(s) => step!(ptr, s),
        None => {
            if bt == BT_NUM {
                return scan_char_ref::<K>(enc, b, ptr + K::MINBPC, end, next);
            }
            *next = ptr;
            return XML_TOK_INVALID;
        }
    }
    while has_char::<K>(ptr, end) {
        let bt = K::byte_type(enc, b, ptr);
        if let Some(s) = check_name_cases::<K>(bt, b, ptr, end, next) {
            step!(ptr, s);
            continue;
        }
        if bt == BT_SEMI {
            *next = ptr + K::MINBPC;
            return XML_TOK_ENTITY_REF;
        }
        *next = ptr;
        return XML_TOK_INVALID;
    }
    XML_TOK_PARTIAL
}

/// After the first character of an attribute's name: the attributes, to the end of the tag.
///
/// Port of `PREFIX(scanAtts)` (xmltok_impl.c:571-722). The jumps to `gt` and `sol` from the
/// value's end into the loop after it are the `Close` steps.
fn scan_atts<K: Kind>(
    enc: &Encoding,
    b: &[u8],
    mut ptr: usize,
    end: usize,
    next: &mut usize,
) -> i32 {
    // How a tag closes after an attribute value: upstream's `gt` and `sol` labels.
    enum Close {
        Gt,
        Sol,
    }
    let close_tag = |ptr: usize, close: Close, next: &mut usize| -> i32 {
        match close {
            Close::Gt => {
                *next = ptr + K::MINBPC;
                XML_TOK_START_TAG_WITH_ATTS
            }
            Close::Sol => {
                let ptr = ptr + K::MINBPC;
                if !has_char::<K>(ptr, end) {
                    return XML_TOK_PARTIAL;
                }
                if !K::char_matches(b, ptr, b'>') {
                    *next = ptr;
                    return XML_TOK_INVALID;
                }
                *next = ptr + K::MINBPC;
                XML_TOK_EMPTY_ELEMENT_WITH_ATTS
            }
        }
    };
    while has_char::<K>(ptr, end) {
        let bt = K::byte_type(enc, b, ptr);
        if let Some(s) = check_name_cases::<K>(bt, b, ptr, end, next) {
            step!(ptr, s);
            continue;
        }
        match bt {
            BT_S | BT_CR | BT_LF | BT_EQUALS => {
                if bt != BT_EQUALS {
                    loop {
                        ptr += K::MINBPC;
                        require_chars!(K, ptr, end, 1);
                        let t = K::byte_type(enc, b, ptr);
                        if t == BT_EQUALS {
                            break;
                        }
                        match t {
                            BT_S | BT_LF | BT_CR => {}
                            _ => {
                                *next = ptr;
                                return XML_TOK_INVALID;
                            }
                        }
                    }
                }
                // BT_EQUALS
                let open;
                loop {
                    ptr += K::MINBPC;
                    require_chars!(K, ptr, end, 1);
                    let t = K::byte_type(enc, b, ptr);
                    if t == BT_QUOT || t == BT_APOS {
                        open = t;
                        break;
                    }
                    match t {
                        BT_S | BT_LF | BT_CR => {}
                        _ => {
                            *next = ptr;
                            return XML_TOK_INVALID;
                        }
                    }
                }
                ptr += K::MINBPC;
                // in attribute value
                loop {
                    require_chars!(K, ptr, end, 1);
                    let t = K::byte_type(enc, b, ptr);
                    if t == open {
                        break;
                    }
                    if let Some(s) = invalid_cases::<K>(t, b, ptr, end, next) {
                        step!(ptr, s);
                        continue;
                    }
                    match t {
                        BT_AMP => {
                            let tok = scan_ref::<K>(enc, b, ptr + K::MINBPC, end, &mut ptr);
                            if tok <= 0 {
                                if tok == XML_TOK_INVALID {
                                    *next = ptr;
                                }
                                return tok;
                            }
                        }
                        BT_LT => {
                            *next = ptr;
                            return XML_TOK_INVALID;
                        }
                        _ => ptr += K::MINBPC,
                    }
                }
                ptr += K::MINBPC;
                require_chars!(K, ptr, end, 1);
                match K::byte_type(enc, b, ptr) {
                    BT_S | BT_CR | BT_LF => {}
                    BT_SOL => return close_tag(ptr, Close::Sol, next),
                    BT_GT => return close_tag(ptr, Close::Gt, next),
                    _ => {
                        *next = ptr;
                        return XML_TOK_INVALID;
                    }
                }
                // ptr points to closing quote
                loop {
                    ptr += K::MINBPC;
                    require_chars!(K, ptr, end, 1);
                    let bt = K::byte_type(enc, b, ptr);
                    if let Some(s) = check_nmstrt_cases::<K>(bt, b, ptr, end, next) {
                        step!(ptr, s);
                        break;
                    }
                    match bt {
                        BT_S | BT_CR | BT_LF => continue,
                        BT_GT => return close_tag(ptr, Close::Gt, next),
                        BT_SOL => return close_tag(ptr, Close::Sol, next),
                        _ => {
                            *next = ptr;
                            return XML_TOK_INVALID;
                        }
                    }
                }
            }
            _ => {
                *next = ptr;
                return XML_TOK_INVALID;
            }
        }
    }
    XML_TOK_PARTIAL
}

/// After "<": a tag, a comment, a CDATA section or a processing instruction.
///
/// Port of `PREFIX(scanLt)` (xmltok_impl.c:724-820).
fn scan_lt<K: Kind>(enc: &Encoding, b: &[u8], mut ptr: usize, end: usize, next: &mut usize) -> i32 {
    require_chars!(K, ptr, end, 1);
    let bt = K::byte_type(enc, b, ptr);
    match check_nmstrt_cases::<K>(bt, b, ptr, end, next) {
        Some(s) => step!(ptr, s),
        None => match bt {
            BT_EXCL => {
                ptr += K::MINBPC;
                require_chars!(K, ptr, end, 1);
                match K::byte_type(enc, b, ptr) {
                    BT_MINUS => return scan_comment::<K>(enc, b, ptr + K::MINBPC, end, next),
                    BT_LSQB => return scan_cdata_section::<K>(b, ptr + K::MINBPC, end, next),
                    _ => {}
                }
                *next = ptr;
                return XML_TOK_INVALID;
            }
            BT_QUEST => return scan_pi::<K>(enc, b, ptr + K::MINBPC, end, next),
            BT_SOL => return scan_end_tag::<K>(enc, b, ptr + K::MINBPC, end, next),
            _ => {
                *next = ptr;
                return XML_TOK_INVALID;
            }
        },
    }
    // we have a start-tag
    while has_char::<K>(ptr, end) {
        let bt = K::byte_type(enc, b, ptr);
        if let Some(s) = check_name_cases::<K>(bt, b, ptr, end, next) {
            step!(ptr, s);
            continue;
        }
        match bt {
            BT_S | BT_CR | BT_LF => {
                ptr += K::MINBPC;
                while has_char::<K>(ptr, end) {
                    let bt = K::byte_type(enc, b, ptr);
                    if let Some(s) = check_nmstrt_cases::<K>(bt, b, ptr, end, next) {
                        step!(ptr, s);
                        return scan_atts::<K>(enc, b, ptr, end, next);
                    }
                    match bt {
                        BT_GT => {
                            *next = ptr + K::MINBPC;
                            return XML_TOK_START_TAG_NO_ATTS;
                        }
                        BT_SOL => return lt_sol::<K>(b, ptr, end, next),
                        BT_S | BT_CR | BT_LF => {
                            ptr += K::MINBPC;
                            continue;
                        }
                        _ => {
                            *next = ptr;
                            return XML_TOK_INVALID;
                        }
                    }
                }
                return XML_TOK_PARTIAL;
            }
            BT_GT => {
                *next = ptr + K::MINBPC;
                return XML_TOK_START_TAG_NO_ATTS;
            }
            BT_SOL => return lt_sol::<K>(b, ptr, end, next),
            _ => {
                *next = ptr;
                return XML_TOK_INVALID;
            }
        }
    }
    XML_TOK_PARTIAL
}

/// `scanLt`'s `sol` label: "/" then ">" ends an empty element.
fn lt_sol<K: Kind>(b: &[u8], ptr: usize, end: usize, next: &mut usize) -> i32 {
    let ptr = ptr + K::MINBPC;
    require_chars!(K, ptr, end, 1);
    if !K::char_matches(b, ptr, b'>') {
        *next = ptr;
        return XML_TOK_INVALID;
    }
    *next = ptr + K::MINBPC;
    XML_TOK_EMPTY_ELEMENT_NO_ATTS
}

/// A token of content: a tag, a reference, a newline or characters.
///
/// Port of `PREFIX(contentTok)` (xmltok_impl.c:822-920).
pub(super) fn content_tok<K: Kind>(
    enc: &Encoding,
    b: &[u8],
    mut ptr: usize,
    end: usize,
    next: &mut usize,
) -> i32 {
    if ptr >= end {
        return XML_TOK_NONE;
    }
    let Some(end) = whole_chars::<K>(ptr, end) else {
        return XML_TOK_PARTIAL;
    };
    let bt = K::byte_type(enc, b, ptr);
    match bt {
        BT_LT => return scan_lt::<K>(enc, b, ptr + K::MINBPC, end, next),
        BT_AMP => return scan_ref::<K>(enc, b, ptr + K::MINBPC, end, next),
        BT_CR => {
            ptr += K::MINBPC;
            if !has_char::<K>(ptr, end) {
                return XML_TOK_TRAILING_CR;
            }
            if K::byte_type(enc, b, ptr) == BT_LF {
                ptr += K::MINBPC;
            }
            *next = ptr;
            return XML_TOK_DATA_NEWLINE;
        }
        BT_LF => {
            *next = ptr + K::MINBPC;
            return XML_TOK_DATA_NEWLINE;
        }
        BT_RSQB => {
            ptr += K::MINBPC;
            if !has_char::<K>(ptr, end) {
                return XML_TOK_TRAILING_RSQB;
            }
            if K::char_matches(b, ptr, b']') {
                ptr += K::MINBPC;
                if !has_char::<K>(ptr, end) {
                    return XML_TOK_TRAILING_RSQB;
                }
                if !K::char_matches(b, ptr, b'>') {
                    ptr -= K::MINBPC;
                } else {
                    *next = ptr;
                    return XML_TOK_INVALID;
                }
            }
        }
        _ => match invalid_cases::<K>(bt, b, ptr, end, next) {
            Some(s) => step!(ptr, s),
            None => ptr += K::MINBPC,
        },
    }
    while has_char::<K>(ptr, end) {
        let bt = K::byte_type(enc, b, ptr);
        match bt {
            BT_LEAD2 | BT_LEAD3 | BT_LEAD4 => {
                let n = usize::from(bt - BT_LEAD2) + 2;
                if end - ptr < n || K::is_invalid_char(b, ptr, n) {
                    *next = ptr;
                    return XML_TOK_DATA_CHARS;
                }
                ptr += n;
            }
            BT_RSQB => {
                if has_chars::<K>(ptr, end, 2) {
                    if !K::char_matches(b, ptr + K::MINBPC, b']') {
                        ptr += K::MINBPC;
                        continue;
                    }
                    if has_chars::<K>(ptr, end, 3) {
                        if !K::char_matches(b, ptr + 2 * K::MINBPC, b'>') {
                            ptr += K::MINBPC;
                            continue;
                        }
                        *next = ptr + 2 * K::MINBPC;
                        return XML_TOK_INVALID;
                    }
                }
                *next = ptr;
                return XML_TOK_DATA_CHARS;
            }
            BT_AMP | BT_LT | BT_NONXML | BT_MALFORM | BT_TRAIL | BT_CR | BT_LF => {
                *next = ptr;
                return XML_TOK_DATA_CHARS;
            }
            _ => ptr += K::MINBPC,
        }
    }
    *next = ptr;
    XML_TOK_DATA_CHARS
}

/// After "%": a parameter entity reference, or a lone "%".
///
/// Port of `PREFIX(scanPercent)` (xmltok_impl.c:922-950).
fn scan_percent<K: Kind>(
    enc: &Encoding,
    b: &[u8],
    mut ptr: usize,
    end: usize,
    next: &mut usize,
) -> i32 {
    require_chars!(K, ptr, end, 1);
    let bt = K::byte_type(enc, b, ptr);
    match check_nmstrt_cases::<K>(bt, b, ptr, end, next) {
        Some(s) => step!(ptr, s),
        None => {
            *next = ptr;
            return match bt {
                BT_S | BT_LF | BT_CR | BT_PERCNT => XML_TOK_PERCENT,
                _ => XML_TOK_INVALID,
            };
        }
    }
    while has_char::<K>(ptr, end) {
        let bt = K::byte_type(enc, b, ptr);
        if let Some(s) = check_name_cases::<K>(bt, b, ptr, end, next) {
            step!(ptr, s);
            continue;
        }
        if bt == BT_SEMI {
            *next = ptr + K::MINBPC;
            return XML_TOK_PARAM_ENTITY_REF;
        }
        *next = ptr;
        return XML_TOK_INVALID;
    }
    XML_TOK_PARTIAL
}

/// After "#": a name such as `#PCDATA`.
///
/// Port of `PREFIX(scanPoundName)` (xmltok_impl.c:952-980).
fn scan_pound_name<K: Kind>(
    enc: &Encoding,
    b: &[u8],
    mut ptr: usize,
    end: usize,
    next: &mut usize,
) -> i32 {
    require_chars!(K, ptr, end, 1);
    let bt = K::byte_type(enc, b, ptr);
    match check_nmstrt_cases::<K>(bt, b, ptr, end, next) {
        Some(s) => step!(ptr, s),
        None => {
            *next = ptr;
            return XML_TOK_INVALID;
        }
    }
    while has_char::<K>(ptr, end) {
        let bt = K::byte_type(enc, b, ptr);
        if let Some(s) = check_name_cases::<K>(bt, b, ptr, end, next) {
            step!(ptr, s);
            continue;
        }
        *next = ptr;
        return match bt {
            BT_CR | BT_LF | BT_S | BT_RPAR | BT_GT | BT_PERCNT | BT_VERBAR => XML_TOK_POUND_NAME,
            _ => XML_TOK_INVALID,
        };
    }
    -XML_TOK_POUND_NAME
}

/// After an opening quote `open`: a literal.
///
/// Port of `PREFIX(scanLit)` (xmltok_impl.c:982-1014).
fn scan_lit<K: Kind>(
    open: u8,
    enc: &Encoding,
    b: &[u8],
    mut ptr: usize,
    end: usize,
    next: &mut usize,
) -> i32 {
    while has_char::<K>(ptr, end) {
        let t = K::byte_type(enc, b, ptr);
        if let Some(s) = invalid_cases::<K>(t, b, ptr, end, next) {
            step!(ptr, s);
            continue;
        }
        match t {
            BT_QUOT | BT_APOS => {
                ptr += K::MINBPC;
                if t != open {
                    continue;
                }
                if !has_char::<K>(ptr, end) {
                    return -XML_TOK_LITERAL;
                }
                *next = ptr;
                return match K::byte_type(enc, b, ptr) {
                    BT_S | BT_CR | BT_LF | BT_GT | BT_PERCNT | BT_LSQB => XML_TOK_LITERAL,
                    _ => XML_TOK_INVALID,
                };
            }
            _ => ptr += K::MINBPC,
        }
    }
    XML_TOK_PARTIAL
}

/// A token of the prolog (and of the DTD).
///
/// Port of `PREFIX(prologTok)` (xmltok_impl.c:1016-1258).
pub(super) fn prolog_tok<K: Kind>(
    enc: &Encoding,
    b: &[u8],
    mut ptr: usize,
    end: usize,
    next: &mut usize,
) -> i32 {
    if ptr >= end {
        return XML_TOK_NONE;
    }
    let Some(end) = whole_chars::<K>(ptr, end) else {
        return XML_TOK_PARTIAL;
    };
    let mut tok;
    let bt = K::byte_type(enc, b, ptr);
    match bt {
        BT_QUOT => return scan_lit::<K>(BT_QUOT, enc, b, ptr + K::MINBPC, end, next),
        BT_APOS => return scan_lit::<K>(BT_APOS, enc, b, ptr + K::MINBPC, end, next),
        BT_LT => {
            ptr += K::MINBPC;
            require_chars!(K, ptr, end, 1);
            match K::byte_type(enc, b, ptr) {
                BT_EXCL => return scan_decl::<K>(enc, b, ptr + K::MINBPC, end, next),
                BT_QUEST => return scan_pi::<K>(enc, b, ptr + K::MINBPC, end, next),
                BT_NMSTRT | BT_HEX | BT_NONASCII | BT_LEAD2 | BT_LEAD3 | BT_LEAD4 => {
                    *next = ptr - K::MINBPC;
                    return XML_TOK_INSTANCE_START;
                }
                _ => {}
            }
            *next = ptr;
            return XML_TOK_INVALID;
        }
        BT_CR | BT_S | BT_LF => {
            if bt == BT_CR && ptr + K::MINBPC == end {
                *next = end;
                // indicate that this might be part of a CR/LF pair
                return -XML_TOK_PROLOG_S;
            }
            loop {
                ptr += K::MINBPC;
                if !has_char::<K>(ptr, end) {
                    break;
                }
                match K::byte_type(enc, b, ptr) {
                    BT_S | BT_LF => {}
                    // don't split CR/LF pair
                    BT_CR if ptr + K::MINBPC != end => {}
                    _ => {
                        *next = ptr;
                        return XML_TOK_PROLOG_S;
                    }
                }
            }
            *next = ptr;
            return XML_TOK_PROLOG_S;
        }
        BT_PERCNT => return scan_percent::<K>(enc, b, ptr + K::MINBPC, end, next),
        BT_COMMA => {
            *next = ptr + K::MINBPC;
            return XML_TOK_COMMA;
        }
        BT_LSQB => {
            *next = ptr + K::MINBPC;
            return XML_TOK_OPEN_BRACKET;
        }
        BT_RSQB => {
            ptr += K::MINBPC;
            if !has_char::<K>(ptr, end) {
                return -XML_TOK_CLOSE_BRACKET;
            }
            if K::char_matches(b, ptr, b']') {
                require_chars!(K, ptr, end, 2);
                if K::char_matches(b, ptr + K::MINBPC, b'>') {
                    *next = ptr + 2 * K::MINBPC;
                    return XML_TOK_COND_SECT_CLOSE;
                }
            }
            *next = ptr;
            return XML_TOK_CLOSE_BRACKET;
        }
        BT_LPAR => {
            *next = ptr + K::MINBPC;
            return XML_TOK_OPEN_PAREN;
        }
        BT_RPAR => {
            ptr += K::MINBPC;
            if !has_char::<K>(ptr, end) {
                return -XML_TOK_CLOSE_PAREN;
            }
            match K::byte_type(enc, b, ptr) {
                BT_AST => {
                    *next = ptr + K::MINBPC;
                    return XML_TOK_CLOSE_PAREN_ASTERISK;
                }
                BT_QUEST => {
                    *next = ptr + K::MINBPC;
                    return XML_TOK_CLOSE_PAREN_QUESTION;
                }
                BT_PLUS => {
                    *next = ptr + K::MINBPC;
                    return XML_TOK_CLOSE_PAREN_PLUS;
                }
                BT_CR | BT_LF | BT_S | BT_GT | BT_COMMA | BT_VERBAR | BT_RPAR => {
                    *next = ptr;
                    return XML_TOK_CLOSE_PAREN;
                }
                _ => {}
            }
            *next = ptr;
            return XML_TOK_INVALID;
        }
        BT_VERBAR => {
            *next = ptr + K::MINBPC;
            return XML_TOK_OR;
        }
        BT_GT => {
            *next = ptr + K::MINBPC;
            return XML_TOK_DECL_CLOSE;
        }
        BT_NUM => return scan_pound_name::<K>(enc, b, ptr + K::MINBPC, end, next),
        BT_LEAD2 | BT_LEAD3 | BT_LEAD4 => {
            let n = usize::from(bt - BT_LEAD2) + 2;
            if end - ptr < n {
                return XML_TOK_PARTIAL_CHAR;
            }
            if K::is_invalid_char(b, ptr, n) {
                *next = ptr;
                return XML_TOK_INVALID;
            }
            if K::is_nmstrt_char(b, ptr, n) {
                ptr += n;
                tok = XML_TOK_NAME;
            } else if K::is_name_char(b, ptr, n) {
                ptr += n;
                tok = XML_TOK_NMTOKEN;
            } else {
                *next = ptr;
                return XML_TOK_INVALID;
            }
        }
        BT_NMSTRT | BT_HEX => {
            tok = XML_TOK_NAME;
            ptr += K::MINBPC;
        }
        BT_DIGIT | BT_NAME | BT_MINUS => {
            tok = XML_TOK_NMTOKEN;
            ptr += K::MINBPC;
        }
        BT_NONASCII if K::is_nmstrt_char_minbpc(b, ptr) => {
            ptr += K::MINBPC;
            tok = XML_TOK_NAME;
        }
        BT_NONASCII if K::is_name_char_minbpc(b, ptr) => {
            ptr += K::MINBPC;
            tok = XML_TOK_NMTOKEN;
        }
        _ => {
            *next = ptr;
            return XML_TOK_INVALID;
        }
    }
    while has_char::<K>(ptr, end) {
        let bt = K::byte_type(enc, b, ptr);
        if let Some(s) = check_name_cases::<K>(bt, b, ptr, end, next) {
            step!(ptr, s);
            continue;
        }
        match bt {
            BT_GT | BT_RPAR | BT_COMMA | BT_VERBAR | BT_LSQB | BT_PERCNT | BT_S | BT_CR | BT_LF => {
                *next = ptr;
                return tok;
            }
            BT_PLUS | BT_AST | BT_QUEST => {
                if tok == XML_TOK_NMTOKEN {
                    *next = ptr;
                    return XML_TOK_INVALID;
                }
                *next = ptr + K::MINBPC;
                return match bt {
                    BT_PLUS => XML_TOK_NAME_PLUS,
                    BT_AST => XML_TOK_NAME_ASTERISK,
                    _ => XML_TOK_NAME_QUESTION,
                };
            }
            _ => {
                *next = ptr;
                return XML_TOK_INVALID;
            }
        }
    }
    tok = -tok;
    tok
}

/// A token of an attribute value already tokenized: characters, a reference, a newline or a
/// space.
///
/// Port of `PREFIX(attributeValueTok)` (xmltok_impl.c:1260-1327).
pub(super) fn attribute_value_tok<K: Kind>(
    enc: &Encoding,
    b: &[u8],
    mut ptr: usize,
    end: usize,
    next: &mut usize,
) -> i32 {
    if ptr >= end {
        return XML_TOK_NONE;
    } else if !has_char::<K>(ptr, end) {
        // This line cannot be executed. The incoming data has already been tokenized once.
        return XML_TOK_PARTIAL;
    }
    let start = ptr;
    while has_char::<K>(ptr, end) {
        let bt = K::byte_type(enc, b, ptr);
        match bt {
            // NOTE: The encoding has already been validated.
            BT_LEAD2 | BT_LEAD3 | BT_LEAD4 => ptr += usize::from(bt - BT_LEAD2) + 2,
            BT_AMP => {
                if ptr == start {
                    return scan_ref::<K>(enc, b, ptr + K::MINBPC, end, next);
                }
                *next = ptr;
                return XML_TOK_DATA_CHARS;
            }
            BT_LT => {
                // this is for inside entity references
                *next = ptr;
                return XML_TOK_INVALID;
            }
            BT_LF => {
                if ptr == start {
                    *next = ptr + K::MINBPC;
                    return XML_TOK_DATA_NEWLINE;
                }
                *next = ptr;
                return XML_TOK_DATA_CHARS;
            }
            BT_CR => {
                if ptr == start {
                    ptr += K::MINBPC;
                    if !has_char::<K>(ptr, end) {
                        return XML_TOK_TRAILING_CR;
                    }
                    if K::byte_type(enc, b, ptr) == BT_LF {
                        ptr += K::MINBPC;
                    }
                    *next = ptr;
                    return XML_TOK_DATA_NEWLINE;
                }
                *next = ptr;
                return XML_TOK_DATA_CHARS;
            }
            BT_S => {
                if ptr == start {
                    *next = ptr + K::MINBPC;
                    return XML_TOK_ATTRIBUTE_VALUE_S;
                }
                *next = ptr;
                return XML_TOK_DATA_CHARS;
            }
            _ => ptr += K::MINBPC,
        }
    }
    *next = ptr;
    XML_TOK_DATA_CHARS
}

/// A token of an entity value already tokenized: characters, a reference, a parameter entity
/// reference or a newline.
///
/// Port of `PREFIX(entityValueTok)` (xmltok_impl.c:1329-1394).
pub(super) fn entity_value_tok<K: Kind>(
    enc: &Encoding,
    b: &[u8],
    mut ptr: usize,
    end: usize,
    next: &mut usize,
) -> i32 {
    if ptr >= end {
        return XML_TOK_NONE;
    } else if !has_char::<K>(ptr, end) {
        // This line cannot be executed. The incoming data has already been tokenized once.
        return XML_TOK_PARTIAL;
    }
    let start = ptr;
    while has_char::<K>(ptr, end) {
        let bt = K::byte_type(enc, b, ptr);
        match bt {
            // NOTE: The encoding has already been validated.
            BT_LEAD2 | BT_LEAD3 | BT_LEAD4 => ptr += usize::from(bt - BT_LEAD2) + 2,
            BT_AMP => {
                if ptr == start {
                    return scan_ref::<K>(enc, b, ptr + K::MINBPC, end, next);
                }
                *next = ptr;
                return XML_TOK_DATA_CHARS;
            }
            BT_PERCNT => {
                if ptr == start {
                    let tok = scan_percent::<K>(enc, b, ptr + K::MINBPC, end, next);
                    return if tok == XML_TOK_PERCENT {
                        XML_TOK_INVALID
                    } else {
                        tok
                    };
                }
                *next = ptr;
                return XML_TOK_DATA_CHARS;
            }
            BT_LF => {
                if ptr == start {
                    *next = ptr + K::MINBPC;
                    return XML_TOK_DATA_NEWLINE;
                }
                *next = ptr;
                return XML_TOK_DATA_CHARS;
            }
            BT_CR => {
                if ptr == start {
                    ptr += K::MINBPC;
                    if !has_char::<K>(ptr, end) {
                        return XML_TOK_TRAILING_CR;
                    }
                    if K::byte_type(enc, b, ptr) == BT_LF {
                        ptr += K::MINBPC;
                    }
                    *next = ptr;
                    return XML_TOK_DATA_NEWLINE;
                }
                *next = ptr;
                return XML_TOK_DATA_CHARS;
            }
            _ => ptr += K::MINBPC,
        }
    }
    *next = ptr;
    XML_TOK_DATA_CHARS
}

/// The rest of an ignored conditional section, nested ones included.
///
/// Port of `PREFIX(ignoreSectionTok)` (xmltok_impl.c:1396-1444).
pub(super) fn ignore_section_tok<K: Kind>(
    enc: &Encoding,
    b: &[u8],
    mut ptr: usize,
    mut end: usize,
    next: &mut usize,
) -> i32 {
    let mut level = 0;
    if K::MINBPC > 1 {
        let mut n = end - ptr;
        if n & (K::MINBPC - 1) != 0 {
            n &= !(K::MINBPC - 1);
            end = ptr + n;
        }
    }
    while has_char::<K>(ptr, end) {
        let bt = K::byte_type(enc, b, ptr);
        if let Some(s) = invalid_cases::<K>(bt, b, ptr, end, next) {
            step!(ptr, s);
            continue;
        }
        match bt {
            BT_LT => {
                ptr += K::MINBPC;
                require_chars!(K, ptr, end, 1);
                if K::char_matches(b, ptr, b'!') {
                    ptr += K::MINBPC;
                    require_chars!(K, ptr, end, 1);
                    if K::char_matches(b, ptr, b'[') {
                        level += 1;
                        ptr += K::MINBPC;
                    }
                }
            }
            BT_RSQB => {
                ptr += K::MINBPC;
                require_chars!(K, ptr, end, 1);
                if K::char_matches(b, ptr, b']') {
                    ptr += K::MINBPC;
                    require_chars!(K, ptr, end, 1);
                    if K::char_matches(b, ptr, b'>') {
                        ptr += K::MINBPC;
                        if level == 0 {
                            *next = ptr;
                            return XML_TOK_IGNORE_SECT;
                        }
                        level -= 1;
                    }
                }
            }
            _ => ptr += K::MINBPC,
        }
    }
    XML_TOK_PARTIAL
}

/// Whether the literal `b[ptr..end]` (quotes included) is a valid public identifier; if not,
/// `*bad` is where it stops being one.
///
/// Port of `PREFIX(isPublicId)` (xmltok_impl.c:1448-1501).
pub(super) fn is_public_id<K: Kind>(
    enc: &Encoding,
    b: &[u8],
    mut ptr: usize,
    mut end: usize,
    bad: &mut usize,
) -> bool {
    ptr += K::MINBPC;
    end -= K::MINBPC;
    while has_char::<K>(ptr, end) {
        let bt = K::byte_type(enc, b, ptr);
        let check_ascii = match bt {
            BT_DIGIT | BT_HEX | BT_MINUS | BT_APOS | BT_LPAR | BT_RPAR | BT_PLUS | BT_COMMA
            | BT_SOL | BT_EQUALS | BT_QUEST | BT_CR | BT_LF | BT_SEMI | BT_EXCL | BT_AST
            | BT_PERCNT | BT_NUM => false,
            BT_S => {
                if K::char_matches(b, ptr, b'\t') {
                    *bad = ptr;
                    return false;
                }
                false
            }
            BT_NAME | BT_NMSTRT => K::byte_to_ascii(b, ptr) & !0x7f != 0,
            _ => true,
        };
        if check_ascii {
            match K::byte_to_ascii(b, ptr) {
                0x24 | 0x40 => {} // $ @
                _ => {
                    *bad = ptr;
                    return false;
                }
            }
        }
        ptr += K::MINBPC;
    }
    true
}

/// The attributes of a well-formed start tag or empty element tag at `ptr`: returns how many
/// there are, and stores the first `atts.len()` in `atts`.
///
/// Port of `PREFIX(getAtts)` (xmltok_impl.c:1503-1599).
pub(super) fn get_atts<K: Kind>(
    enc: &Encoding,
    b: &[u8],
    mut ptr: usize,
    atts: &mut [Attribute],
) -> usize {
    #[derive(PartialEq)]
    enum State {
        Other,
        InName,
        InValue,
    }
    let atts_max = atts.len();
    let mut state = State::InName;
    let mut n_atts = 0usize;
    // defined when state == InValue
    let mut open = 0u8;

    ptr += K::MINBPC;
    loop {
        let bt = K::byte_type(enc, b, ptr);
        match bt {
            BT_LEAD2 | BT_LEAD3 | BT_LEAD4 | BT_NONASCII | BT_NMSTRT | BT_HEX => {
                // START_NAME
                if state == State::Other {
                    if n_atts < atts_max {
                        atts[n_atts].name = ptr;
                        atts[n_atts].normalized = true;
                    }
                    state = State::InName;
                }
                if let BT_LEAD2 | BT_LEAD3 | BT_LEAD4 = bt {
                    // NOTE: The encoding has already been validated.
                    ptr += usize::from(bt - BT_LEAD2) + 2 - K::MINBPC;
                }
            }
            BT_QUOT | BT_APOS => {
                if state != State::InValue {
                    if n_atts < atts_max {
                        atts[n_atts].value_ptr = ptr + K::MINBPC;
                    }
                    state = State::InValue;
                    open = bt;
                } else if open == bt {
                    state = State::Other;
                    if n_atts < atts_max {
                        atts[n_atts].value_end = ptr;
                    }
                    n_atts += 1;
                }
            }
            BT_AMP => {
                if n_atts < atts_max {
                    atts[n_atts].normalized = false;
                }
            }
            BT_S => {
                if state == State::InName {
                    state = State::Other;
                } else if state == State::InValue
                    && n_atts < atts_max
                    && atts[n_atts].normalized
                    && (ptr == atts[n_atts].value_ptr
                        || K::byte_to_ascii(b, ptr) != i32::from(b' ')
                        || K::byte_to_ascii(b, ptr + K::MINBPC) == i32::from(b' ')
                        || K::byte_type(enc, b, ptr + K::MINBPC) == open)
                {
                    atts[n_atts].normalized = false;
                }
            }
            BT_CR | BT_LF => {
                // This case ensures that the first attribute name is counted. Apart from that
                // we could just change state on the quote.
                if state == State::InName {
                    state = State::Other;
                } else if state == State::InValue && n_atts < atts_max {
                    atts[n_atts].normalized = false;
                }
            }
            BT_GT | BT_SOL if state != State::InValue => return n_atts,
            _ => {}
        }
        ptr += K::MINBPC;
    }
}

/// The value of the character reference at `ptr` ("&#...;"), or -1 if it isn't a character
/// XML allows.
///
/// Port of `PREFIX(charRefNumber)` (xmltok_impl.c:1601-1657).
pub(super) fn char_ref_number<K: Kind>(b: &[u8], mut ptr: usize) -> i32 {
    let mut result: i32 = 0;
    // skip &#
    ptr += 2 * K::MINBPC;
    if K::char_matches(b, ptr, b'x') {
        ptr += K::MINBPC;
        while !K::char_matches(b, ptr, b';') {
            let c = K::byte_to_ascii(b, ptr);
            match c {
                0x30..=0x39 => {
                    result <<= 4;
                    result |= c - 0x30;
                }
                0x41..=0x46 => {
                    result <<= 4;
                    result += 10 + (c - 0x41);
                }
                0x61..=0x66 => {
                    result <<= 4;
                    result += 10 + (c - 0x61);
                }
                _ => {}
            }
            if result >= 0x110000 {
                return -1;
            }
            ptr += K::MINBPC;
        }
    } else {
        while !K::char_matches(b, ptr, b';') {
            let c = K::byte_to_ascii(b, ptr);
            result *= 10;
            result += c - 0x30;
            if result >= 0x110000 {
                return -1;
            }
            ptr += K::MINBPC;
        }
    }
    check_char_ref_number(result)
}

/// The character `b[ptr..end]` names if it is one of the predefined entities (`lt`, `gt`,
/// `amp`, `quot`, `apos`), or 0.
///
/// Port of `PREFIX(predefinedEntityName)` (xmltok_impl.c:1659-1711).
pub(super) fn predefined_entity_name<K: Kind>(b: &[u8], mut ptr: usize, end: usize) -> i32 {
    match (end - ptr) / K::MINBPC {
        2 => {
            if K::char_matches(b, ptr + K::MINBPC, b't') {
                match K::byte_to_ascii(b, ptr) {
                    0x6C => return i32::from(b'<'), // l
                    0x67 => return i32::from(b'>'), // g
                    _ => {}
                }
            }
        }
        3 => {
            if K::char_matches(b, ptr, b'a') {
                ptr += K::MINBPC;
                if K::char_matches(b, ptr, b'm') {
                    ptr += K::MINBPC;
                    if K::char_matches(b, ptr, b'p') {
                        return i32::from(b'&');
                    }
                }
            }
        }
        4 => match K::byte_to_ascii(b, ptr) {
            0x71 => {
                // q
                ptr += K::MINBPC;
                if K::char_matches(b, ptr, b'u') {
                    ptr += K::MINBPC;
                    if K::char_matches(b, ptr, b'o') {
                        ptr += K::MINBPC;
                        if K::char_matches(b, ptr, b't') {
                            return i32::from(b'"');
                        }
                    }
                }
            }
            0x61 => {
                // a
                ptr += K::MINBPC;
                if K::char_matches(b, ptr, b'p') {
                    ptr += K::MINBPC;
                    if K::char_matches(b, ptr, b'o') {
                        ptr += K::MINBPC;
                        if K::char_matches(b, ptr, b's') {
                            return i32::from(b'\'');
                        }
                    }
                }
            }
            _ => {}
        },
        _ => {}
    }
    0
}

/// Whether the name `b[ptr1..end1]` is the ASCII string `ptr2`.
///
/// Port of `PREFIX(nameMatchesAscii)` (xmltok_impl.c:1713-1730).
pub(super) fn name_matches_ascii<K: Kind>(
    b: &[u8],
    mut ptr1: usize,
    end1: usize,
    ptr2: &[u8],
) -> bool {
    for &c in ptr2 {
        if end1 < ptr1 || end1 - ptr1 < K::MINBPC {
            // This line cannot be executed. The incoming data has already been tokenized once.
            return false;
        }
        if !K::char_matches(b, ptr1, c) {
            return false;
        }
        ptr1 += K::MINBPC;
    }
    ptr1 == end1
}

/// The length in bytes of the name at `ptr`.
///
/// Port of `PREFIX(nameLength)` (xmltok_impl.c:1732-1760).
pub(super) fn name_length<K: Kind>(enc: &Encoding, b: &[u8], mut ptr: usize) -> usize {
    let start = ptr;
    loop {
        let bt = K::byte_type(enc, b, ptr);
        match bt {
            // NOTE: The encoding has already been validated.
            BT_LEAD2 | BT_LEAD3 | BT_LEAD4 => ptr += usize::from(bt - BT_LEAD2) + 2,
            BT_NONASCII | BT_NMSTRT | BT_HEX | BT_DIGIT | BT_NAME | BT_MINUS => {
                ptr += K::MINBPC;
            }
            _ => return ptr - start,
        }
    }
}

/// The first character at or after `ptr` that isn't white space.
///
/// Port of `PREFIX(skipS)` (xmltok_impl.c:1762-1775).
pub(super) fn skip_s<K: Kind>(enc: &Encoding, b: &[u8], mut ptr: usize) -> usize {
    loop {
        match K::byte_type(enc, b, ptr) {
            BT_LF | BT_CR | BT_S => ptr += K::MINBPC,
            _ => return ptr,
        }
    }
}

/// Moves `pos` over `b[ptr..end]`: lines (LF, CR or CR LF) and columns (characters).
///
/// Port of `PREFIX(updatePosition)` (xmltok_impl.c:1777-1809).
pub(super) fn update_position<K: Kind>(
    enc: &Encoding,
    b: &[u8],
    mut ptr: usize,
    end: usize,
    pos: &mut Position,
) {
    while has_char::<K>(ptr, end) {
        let bt = K::byte_type(enc, b, ptr);
        match bt {
            // NOTE: The encoding has already been validated.
            BT_LEAD2 | BT_LEAD3 | BT_LEAD4 => {
                ptr += usize::from(bt - BT_LEAD2) + 2;
                pos.column_number = pos.column_number.wrapping_add(1);
            }
            BT_LF => {
                pos.column_number = 0;
                pos.line_number = pos.line_number.wrapping_add(1);
                ptr += K::MINBPC;
            }
            BT_CR => {
                pos.line_number = pos.line_number.wrapping_add(1);
                ptr += K::MINBPC;
                if has_char::<K>(ptr, end) && K::byte_type(enc, b, ptr) == BT_LF {
                    ptr += K::MINBPC;
                }
                pos.column_number = 0;
            }
            _ => {
                ptr += K::MINBPC;
                pos.column_number = pos.column_number.wrapping_add(1);
            }
        }
    }
}
