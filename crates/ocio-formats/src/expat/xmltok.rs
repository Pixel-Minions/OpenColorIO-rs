// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from expat 2.7.2 (MIT License, Copyright (c) 1998-2000 Thai Open Source Software Center
// Ltd and Clark Cooper, Copyright (c) 2001-2025 Expat maintainers); see mod.rs.

//! The tokenizer's encodings: a port of `lib/xmltok.c`, `xmltok_ns.c` and `xmltok.h` (expat
//! 2.7.2), for the parsers without namespace processing.
//!
//! - **The encodings** ([`Encoding`]): UTF-8, ISO-8859-1, US-ASCII, UTF-16BE and UTF-16LE, as
//!   documents declare or start with them, and the "internal" UTF-8 and UTF-16LE ones that
//!   expat reads its own entity values with. Each is a static, as upstream's
//!   `normal_encoding`s are, so the parser can compare them by address as expat does. A
//!   method dispatches to the scanner of the encoding's family ([`super::xmltok_impl`]).
//! - **The initial encoding** ([`InitEncoding`], upstream's `INIT_ENCODING`): the one a parser
//!   starts with, which detects the document's encoding from its first bytes (`initScan`).
//! - **The XML declaration** ([`xml_parse_xml_decl`]), the conversions to UTF-8, and
//!   `XmlUtf8Encode`.
//!
//! Not ported, as OCIO's parsers never reach them: the encodings with namespace processing
//! (`*_ns`), unknown encodings (`XmlInitUnknownEncoding`: OCIO sets no
//! `XML_SetUnknownEncodingHandler`, so the parser refuses them), and the conversions to UTF-16
//! (`XML_UNICODE` builds only). The internal UTF-16 encoding is the little-endian one, the
//! byte order of both wheels' machines.

use super::tables::{
    ASCII_TYPES, INTERNAL_LATIN1_TYPES, INTERNAL_UTF8_TYPES, LATIN1_TYPES, NAME_PAGES,
    NAMING_BITMAP, NMSTRT_PAGES, UTF8_TYPES,
};
use super::xmltok_impl::{self as imp, BT_LEAD4, BT_NONASCII, BT_NONXML, BT_TRAIL, Kind};
pub use super::xmltok_impl::{
    XML_TOK_BOM, XML_TOK_NONE, XML_TOK_PARTIAL, XML_TOK_PARTIAL_CHAR, XML_TOK_TRAILING_CR,
};

/// `XML_PROLOG_STATE` (xmltok.h:130).
pub const XML_PROLOG_STATE: i32 = 0;
/// `XML_CONTENT_STATE` (xmltok.h:131).
pub const XML_CONTENT_STATE: i32 = 1;
/// `XML_CDATA_SECTION_STATE` (xmltok.h:132).
pub const XML_CDATA_SECTION_STATE: i32 = 2;
/// `XML_IGNORE_SECTION_STATE` (xmltok.h:134), with `XML_DTD`, as the wheels are built.
pub const XML_IGNORE_SECTION_STATE: i32 = 3;

/// `XML_ATTRIBUTE_VALUE_LITERAL` (xmltok.h:138).
pub const XML_ATTRIBUTE_VALUE_LITERAL: i32 = 0;
/// `XML_ENTITY_VALUE_LITERAL` (xmltok.h:139).
pub const XML_ENTITY_VALUE_LITERAL: i32 = 1;

/// `XML_UTF8_ENCODE_MAX`: the most bytes `XmlUtf8Encode` writes (xmltok.h:142).
pub const XML_UTF8_ENCODE_MAX: usize = 4;

/// A position in a document: first line and first column are 0, not 1.
///
/// Port of `POSITION` (xmltok.h:146-150). `XML_Size` is `unsigned long` in the wheels' builds
/// (no `XML_LARGE_SIZE`); the port counts in `u64`, and only OCIO's own line count reaches its
/// messages.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Position {
    /// `lineNumber`.
    pub line_number: u64,
    /// `columnNumber`.
    pub column_number: u64,
}

/// An attribute of a start tag, as positions in the tokenized buffer.
///
/// Port of `ATTRIBUTE` (xmltok.h:159-164).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Attribute {
    /// `name`: where the name starts.
    pub name: usize,
    /// `valuePtr`: where the value starts, after its quote.
    pub value_ptr: usize,
    /// `valueEnd`: the value's closing quote.
    pub value_end: usize,
    /// `normalized`: the value needs no normalization (no reference, no white space but
    /// single spaces between other characters).
    pub normalized: bool,
}

/// The outcome of a conversion.
///
/// Port of `enum XML_Convert_Result` (xmltok.h:165-170).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConvertResult {
    /// `XML_CONVERT_COMPLETED`.
    Completed,
    /// `XML_CONVERT_INPUT_INCOMPLETE`.
    InputIncomplete,
    /// `XML_CONVERT_OUTPUT_EXHAUSTED`, and therefore potentially input remaining as well.
    OutputExhausted,
}

/// Which compilation of `xmltok_impl.c` an encoding uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    /// `normal_`: one byte per character.
    Normal,
    /// `little2_`: UTF-16LE.
    Little2,
    /// `big2_`: UTF-16BE.
    Big2,
}

/// An encoding's conversion to UTF-8 (`utf8Convert`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToUtf8 {
    /// `utf8_toUtf8`.
    Utf8,
    /// `latin1_toUtf8`.
    Latin1,
    /// `ascii_toUtf8`.
    Ascii,
    /// `little2_toUtf8`.
    Little2,
    /// `big2_toUtf8`.
    Big2,
}

/// One of expat's built-in encodings.
///
/// Port of `struct encoding` and `struct normal_encoding` (xmltok.h:172-200, xmltok.c:187-206)
/// for the encodings without namespace processing.
#[derive(Debug)]
pub struct Encoding {
    /// For diagnostics: upstream's variable name.
    name: &'static str,
    family: Family,
    /// `type`: the byte type of each byte (of each code unit below 256, for UTF-16).
    types: &'static [u8; 256],
    to_utf8: ToUtf8,
    /// `minBytesPerChar`.
    pub min_bytes_per_char: usize,
    /// `isUtf8`: the input needs no conversion to UTF-8.
    pub is_utf8: bool,
    /// `isUtf16`: the input is UTF-16 in the machine's byte order.
    pub is_utf16: bool,
}

/// Encodings compare by address, as expat compares `ENCODING` pointers: the internal UTF-8
/// encoding is not the document's UTF-8 encoding.
impl PartialEq for Encoding {
    fn eq(&self, other: &Encoding) -> bool {
        std::ptr::eq(self, other)
    }
}

impl Eq for Encoding {}

/// `utf8_encoding` (xmltok.c:471-479).
pub static UTF8_ENCODING: Encoding = Encoding {
    name: "utf8_encoding",
    family: Family::Normal,
    types: &UTF8_TYPES,
    to_utf8: ToUtf8::Utf8,
    min_bytes_per_char: 1,
    is_utf8: true,
    is_utf16: false,
};

/// `internal_utf8_encoding` (xmltok.c:493-501): returned by `XmlGetUtf8InternalEncoding`.
pub static INTERNAL_UTF8_ENCODING: Encoding = Encoding {
    name: "internal_utf8_encoding",
    family: Family::Normal,
    types: &INTERNAL_UTF8_TYPES,
    to_utf8: ToUtf8::Utf8,
    min_bytes_per_char: 1,
    is_utf8: true,
    is_utf16: false,
};

/// `latin1_encoding` (xmltok.c:551-559).
pub static LATIN1_ENCODING: Encoding = Encoding {
    name: "latin1_encoding",
    family: Family::Normal,
    types: &LATIN1_TYPES,
    to_utf8: ToUtf8::Latin1,
    min_bytes_per_char: 1,
    is_utf8: false,
    is_utf16: false,
};

/// `ascii_encoding` (xmltok.c:586-594): its input is UTF-8 as it is.
pub static ASCII_ENCODING: Encoding = Encoding {
    name: "ascii_encoding",
    family: Family::Normal,
    types: &ASCII_TYPES,
    to_utf8: ToUtf8::Ascii,
    min_bytes_per_char: 1,
    is_utf8: true,
    is_utf16: false,
};

/// `little2_encoding` (xmltok.c:829-842): UTF-16LE, the machine's byte order (`BYTEORDER ==
/// 1234`).
pub static LITTLE2_ENCODING: Encoding = Encoding {
    name: "little2_encoding",
    family: Family::Little2,
    types: &LATIN1_TYPES,
    to_utf8: ToUtf8::Little2,
    min_bytes_per_char: 2,
    is_utf8: false,
    is_utf16: true,
};

/// `internal_little2_encoding` (xmltok.c:859-868): returned by `XmlGetUtf16InternalEncoding`.
pub static INTERNAL_LITTLE2_ENCODING: Encoding = Encoding {
    name: "internal_little2_encoding",
    family: Family::Little2,
    types: &INTERNAL_LATIN1_TYPES,
    to_utf8: ToUtf8::Little2,
    min_bytes_per_char: 2,
    is_utf8: false,
    is_utf16: true,
};

/// `big2_encoding` (xmltok.c:962-975): UTF-16BE, not the machine's byte order.
pub static BIG2_ENCODING: Encoding = Encoding {
    name: "big2_encoding",
    family: Family::Big2,
    types: &LATIN1_TYPES,
    to_utf8: ToUtf8::Big2,
    min_bytes_per_char: 2,
    is_utf8: false,
    is_utf16: false,
};

/// `UCS2_GET_NAMING(pages, hi, lo)` (xmltok.c:80-81).
fn ucs2_get_naming(pages: &[u8; 256], hi: u8, lo: u8) -> bool {
    let index = (usize::from(pages[usize::from(hi)]) << 3) + (usize::from(lo) >> 5);
    NAMING_BITMAP[index] & (1u32 << (lo & 0x1F)) != 0
}

/// `UTF8_GET_NAMING2(pages, byte)` (xmltok.c:87-90).
fn utf8_get_naming2(pages: &[u8; 256], p: &[u8]) -> bool {
    let index = (usize::from(pages[usize::from((p[0] >> 2) & 7)]) << 3)
        + (usize::from(p[0] & 3) << 1)
        + usize::from((p[1] >> 5) & 1);
    NAMING_BITMAP[index] & (1u32 << (p[1] & 0x1F)) != 0
}

/// `UTF8_GET_NAMING3(pages, byte)` (xmltok.c:97-102).
fn utf8_get_naming3(pages: &[u8; 256], p: &[u8]) -> bool {
    let page = (usize::from(p[0] & 0xF) << 4) + usize::from((p[1] >> 2) & 0xF);
    let index = (usize::from(pages[page]) << 3)
        + (usize::from(p[1] & 3) << 1)
        + usize::from((p[2] >> 5) & 1);
    NAMING_BITMAP[index] & (1u32 << (p[2] & 0x1F)) != 0
}

/// `UTF8_INVALID2(p)` (xmltok.c:114-115).
fn utf8_invalid2(p: &[u8]) -> bool {
    p[0] < 0xC2 || (p[1] & 0x80) == 0 || (p[1] & 0xC0) == 0xC0
}

/// `UTF8_INVALID3(p)` (xmltok.c:117-124).
fn utf8_invalid3(p: &[u8]) -> bool {
    (p[2] & 0x80) == 0
        || (if p[0] == 0xEF && p[1] == 0xBF {
            p[2] > 0xBD
        } else {
            (p[2] & 0xC0) == 0xC0
        })
        || (if p[0] == 0xE0 {
            p[1] < 0xA0 || (p[1] & 0xC0) == 0xC0
        } else {
            (p[1] & 0x80) == 0
                || (if p[0] == 0xED {
                    p[1] > 0x9F
                } else {
                    (p[1] & 0xC0) == 0xC0
                })
        })
}

/// `UTF8_INVALID4(p)` (xmltok.c:126-132).
fn utf8_invalid4(p: &[u8]) -> bool {
    (p[3] & 0x80) == 0
        || (p[3] & 0xC0) == 0xC0
        || (p[2] & 0x80) == 0
        || (p[2] & 0xC0) == 0xC0
        || (if p[0] == 0xF0 {
            p[1] < 0x90 || (p[1] & 0xC0) == 0xC0
        } else {
            (p[1] & 0x80) == 0
                || (if p[0] == 0xF4 {
                    p[1] > 0x8F
                } else {
                    (p[1] & 0xC0) == 0xC0
                })
        })
}

/// The one-byte encodings' macros, without `XML_MIN_SIZE` (xmltok.c:233-303): `IS_NAME_CHAR`
/// and its siblings are UTF-8's (`utf8_isName2` and so on, xmltok.c:134-185), the only
/// one-byte encodings whose tables have lead bytes.
pub(super) struct Normal;

impl Kind for Normal {
    const MINBPC: usize = 1;
    #[inline]
    fn byte_type(enc: &Encoding, b: &[u8], p: usize) -> u8 {
        // SB_BYTE_TYPE
        enc.types[usize::from(b[p])]
    }
    #[inline]
    fn byte_to_ascii(b: &[u8], p: usize) -> i32 {
        // `*(p)`: a `char`, signed on both wheels' compilers.
        i32::from(b[p] as i8)
    }
    #[inline]
    fn char_matches(b: &[u8], p: usize, c: u8) -> bool {
        b[p] == c
    }
    fn is_name_char(b: &[u8], p: usize, n: usize) -> bool {
        match n {
            2 => utf8_get_naming2(&NAME_PAGES, &b[p..]),
            3 => utf8_get_naming3(&NAME_PAGES, &b[p..]),
            // utf8_isName4: isNever
            _ => false,
        }
    }
    fn is_nmstrt_char(b: &[u8], p: usize, n: usize) -> bool {
        match n {
            2 => utf8_get_naming2(&NMSTRT_PAGES, &b[p..]),
            3 => utf8_get_naming3(&NMSTRT_PAGES, &b[p..]),
            // utf8_isNmstrt4: isNever
            _ => false,
        }
    }
    fn is_invalid_char(b: &[u8], p: usize, n: usize) -> bool {
        match n {
            2 => utf8_invalid2(&b[p..]),
            3 => utf8_invalid3(&b[p..]),
            _ => utf8_invalid4(&b[p..]),
        }
    }
    #[inline]
    fn is_name_char_minbpc(_: &[u8], _: usize) -> bool {
        false
    }
    #[inline]
    fn is_nmstrt_char_minbpc(_: &[u8], _: usize) -> bool {
        false
    }
}

/// `unicode_byte_type(hi, lo)` (xmltok.c:596-620): the byte type of a UTF-16 code unit of 256
/// or more.
fn unicode_byte_type(hi: u8, lo: u8) -> u8 {
    match hi {
        // 0xD800-0xDBFF first 16-bit code unit or high surrogate (W1)
        0xD8..=0xDB => BT_LEAD4,
        // 0xDC00-0xDFFF second 16-bit code unit or low surrogate (W2)
        0xDC..=0xDF => BT_TRAIL,
        // noncharacter-FFFF, noncharacter-FFFE
        0xFF if lo == 0xFF || lo == 0xFE => BT_NONXML,
        _ => BT_NONASCII,
    }
}

/// UTF-16LE's macros (`LITTLE2_*`, xmltok.c:738-745 and 783-791).
pub(super) struct Little2;

impl Kind for Little2 {
    const MINBPC: usize = 2;
    #[inline]
    fn byte_type(enc: &Encoding, b: &[u8], p: usize) -> u8 {
        if b[p + 1] == 0 {
            enc.types[usize::from(b[p])]
        } else {
            unicode_byte_type(b[p + 1], b[p])
        }
    }
    #[inline]
    fn byte_to_ascii(b: &[u8], p: usize) -> i32 {
        if b[p + 1] == 0 {
            i32::from(b[p] as i8)
        } else {
            -1
        }
    }
    #[inline]
    fn char_matches(b: &[u8], p: usize, c: u8) -> bool {
        b[p + 1] == 0 && b[p] == c
    }
    fn is_name_char(_: &[u8], _: usize, _: usize) -> bool {
        false
    }
    fn is_nmstrt_char(_: &[u8], _: usize, _: usize) -> bool {
        false
    }
    fn is_invalid_char(_: &[u8], _: usize, _: usize) -> bool {
        false
    }
    fn is_name_char_minbpc(b: &[u8], p: usize) -> bool {
        ucs2_get_naming(&NAME_PAGES, b[p + 1], b[p])
    }
    fn is_nmstrt_char_minbpc(b: &[u8], p: usize) -> bool {
        ucs2_get_naming(&NMSTRT_PAGES, b[p + 1], b[p])
    }
}

/// UTF-16BE's macros (`BIG2_*`, xmltok.c:871-878 and 916-924).
pub(super) struct Big2;

impl Kind for Big2 {
    const MINBPC: usize = 2;
    #[inline]
    fn byte_type(enc: &Encoding, b: &[u8], p: usize) -> u8 {
        if b[p] == 0 {
            enc.types[usize::from(b[p + 1])]
        } else {
            unicode_byte_type(b[p], b[p + 1])
        }
    }
    #[inline]
    fn byte_to_ascii(b: &[u8], p: usize) -> i32 {
        if b[p] == 0 {
            i32::from(b[p + 1] as i8)
        } else {
            -1
        }
    }
    #[inline]
    fn char_matches(b: &[u8], p: usize, c: u8) -> bool {
        b[p] == 0 && b[p + 1] == c
    }
    fn is_name_char(_: &[u8], _: usize, _: usize) -> bool {
        false
    }
    fn is_nmstrt_char(_: &[u8], _: usize, _: usize) -> bool {
        false
    }
    fn is_invalid_char(_: &[u8], _: usize, _: usize) -> bool {
        false
    }
    fn is_name_char_minbpc(b: &[u8], p: usize) -> bool {
        ucs2_get_naming(&NAME_PAGES, b[p], b[p + 1])
    }
    fn is_nmstrt_char_minbpc(b: &[u8], p: usize) -> bool {
        ucs2_get_naming(&NMSTRT_PAGES, b[p], b[p + 1])
    }
}

/// Calls the scanner `$f` of `$enc`'s family.
macro_rules! dispatch {
    ($enc:expr, $f:ident ( $($arg:expr),* )) => {
        match $enc.family {
            Family::Normal => imp::$f::<Normal>($($arg),*),
            Family::Little2 => imp::$f::<Little2>($($arg),*),
            Family::Big2 => imp::$f::<Big2>($($arg),*),
        }
    };
}

impl Encoding {
    /// Upstream's name for the encoding, for diagnostics.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// `XmlTok(enc, state, ptr, end, nextTokPtr)`: the next token of `b[ptr..end]` in
    /// `state` (`scanners[state]`, xmltok.h:224-225).
    pub fn tok(&self, state: i32, b: &[u8], ptr: usize, end: usize, next: &mut usize) -> i32 {
        match state {
            XML_PROLOG_STATE => self.prolog_tok(b, ptr, end, next),
            XML_CONTENT_STATE => self.content_tok(b, ptr, end, next),
            XML_CDATA_SECTION_STATE => self.cdata_section_tok(b, ptr, end, next),
            _ => self.ignore_section_tok(b, ptr, end, next),
        }
    }

    /// `XmlPrologTok` (xmltok.h:227-228).
    pub fn prolog_tok(&self, b: &[u8], ptr: usize, end: usize, next: &mut usize) -> i32 {
        dispatch!(self, prolog_tok(self, b, ptr, end, next))
    }

    /// `XmlContentTok` (xmltok.h:230-231).
    pub fn content_tok(&self, b: &[u8], ptr: usize, end: usize, next: &mut usize) -> i32 {
        dispatch!(self, content_tok(self, b, ptr, end, next))
    }

    /// `XmlCdataSectionTok` (xmltok.h:233-234).
    pub fn cdata_section_tok(&self, b: &[u8], ptr: usize, end: usize, next: &mut usize) -> i32 {
        dispatch!(self, cdata_section_tok(self, b, ptr, end, next))
    }

    /// `XmlIgnoreSectionTok` (xmltok.h:238-239).
    pub fn ignore_section_tok(&self, b: &[u8], ptr: usize, end: usize, next: &mut usize) -> i32 {
        dispatch!(self, ignore_section_tok(self, b, ptr, end, next))
    }

    /// `XmlAttributeValueTok` (xmltok.h:249-250).
    pub fn attribute_value_tok(&self, b: &[u8], ptr: usize, end: usize, next: &mut usize) -> i32 {
        dispatch!(self, attribute_value_tok(self, b, ptr, end, next))
    }

    /// `XmlEntityValueTok` (xmltok.h:252-253).
    pub fn entity_value_tok(&self, b: &[u8], ptr: usize, end: usize, next: &mut usize) -> i32 {
        dispatch!(self, entity_value_tok(self, b, ptr, end, next))
    }

    /// `XmlNameMatchesAscii` (xmltok.h:255-256).
    pub fn name_matches_ascii(&self, b: &[u8], ptr1: usize, end1: usize, ptr2: &[u8]) -> bool {
        dispatch!(self, name_matches_ascii(b, ptr1, end1, ptr2))
    }

    /// `XmlNameLength` (xmltok.h:258).
    pub fn name_length(&self, b: &[u8], ptr: usize) -> usize {
        dispatch!(self, name_length(self, b, ptr))
    }

    /// `XmlSkipS` (xmltok.h:260).
    pub fn skip_s(&self, b: &[u8], ptr: usize) -> usize {
        dispatch!(self, skip_s(self, b, ptr))
    }

    /// `XmlGetAttributes` (xmltok.h:262-263): the attribute count of the tag at `ptr`; the
    /// first `atts.len()` go into `atts`.
    pub fn get_atts(&self, b: &[u8], ptr: usize, atts: &mut [Attribute]) -> usize {
        dispatch!(self, get_atts(self, b, ptr, atts))
    }

    /// `XmlCharRefNumber` (xmltok.h:265).
    pub fn char_ref_number(&self, b: &[u8], ptr: usize) -> i32 {
        dispatch!(self, char_ref_number(b, ptr))
    }

    /// `XmlPredefinedEntityName` (xmltok.h:267-268).
    pub fn predefined_entity_name(&self, b: &[u8], ptr: usize, end: usize) -> i32 {
        dispatch!(self, predefined_entity_name(b, ptr, end))
    }

    /// `XmlUpdatePosition` (xmltok.h:270-271).
    pub fn update_position(&self, b: &[u8], ptr: usize, end: usize, pos: &mut Position) {
        dispatch!(self, update_position(self, b, ptr, end, pos))
    }

    /// `XmlIsPublicId` (xmltok.h:273-274).
    pub fn is_public_id(&self, b: &[u8], ptr: usize, end: usize, bad: &mut usize) -> bool {
        dispatch!(self, is_public_id(self, b, ptr, end, bad))
    }

    /// `XmlUtf8Convert(enc, fromP, fromLim, toP, toLim)` (xmltok.h:276-277): converts
    /// `from[*from_p..from_lim]` to UTF-8 into `to[*to_p..to_lim]`, moving both positions.
    pub fn utf8_convert(
        &self,
        from: &[u8],
        from_p: &mut usize,
        from_lim: usize,
        to: &mut [u8],
        to_p: &mut usize,
        to_lim: usize,
    ) -> ConvertResult {
        match self.to_utf8 {
            ToUtf8::Utf8 => utf8_to_utf8(from, from_p, from_lim, to, to_p, to_lim),
            ToUtf8::Latin1 => latin1_to_utf8(from, from_p, from_lim, to, to_p, to_lim),
            ToUtf8::Ascii => ascii_to_utf8(from, from_p, from_lim, to, to_p, to_lim),
            ToUtf8::Little2 => utf16_to_utf8::<false>(from, from_p, from_lim, to, to_p, to_lim),
            ToUtf8::Big2 => utf16_to_utf8::<true>(from, from_p, from_lim, to, to_p, to_lim),
        }
    }
}

/// `UTF8_cval2`, `UTF8_cval3`, `UTF8_cval4`: the masked first bytes of 2, 3 and 4-byte
/// sequences (xmltok.c:320-325).
const UTF8_CVAL2: u8 = 0xc0;
const UTF8_CVAL3: u8 = 0xe0;
const UTF8_CVAL4: u8 = 0xf0;

/// `from_lim` moved back so that `from[from..from_lim]` ends with a complete UTF-8 character.
///
/// Port of `_INTERNAL_trim_to_complete_utf8_characters` (xmltok.c:327-364).
pub fn trim_to_complete_utf8_characters(b: &[u8], from: usize, mut from_lim: usize) -> usize {
    let mut walked = 0usize;
    while from_lim > from {
        let prev = b[from_lim - 1];
        if (prev & 0xf8) == 0xf0 {
            // 4-byte character, lead by 0b11110xxx byte
            if walked + 1 >= 4 {
                from_lim += 4 - 1;
                break;
            } else {
                walked = 0;
            }
        } else if (prev & 0xf0) == 0xe0 {
            // 3-byte character, lead by 0b1110xxxx byte
            if walked + 1 >= 3 {
                from_lim += 3 - 1;
                break;
            } else {
                walked = 0;
            }
        } else if (prev & 0xe0) == 0xc0 {
            // 2-byte character, lead by 0b110xxxxx byte
            if walked + 1 >= 2 {
                from_lim += 2 - 1;
                break;
            } else {
                walked = 0;
            }
        } else if (prev & 0x80) == 0x00 {
            // 1-byte character, matching 0b0xxxxxxx
            break;
        }
        from_lim -= 1;
        walked += 1;
    }
    from_lim
}

/// Port of `utf8_toUtf8` (xmltok.c:366-406).
fn utf8_to_utf8(
    from: &[u8],
    from_p: &mut usize,
    mut from_lim: usize,
    to: &mut [u8],
    to_p: &mut usize,
    to_lim: usize,
) -> ConvertResult {
    let mut input_incomplete = false;
    let mut output_exhausted = false;

    // Avoid copying partial characters (due to limited space).
    let bytes_available = from_lim - *from_p;
    let bytes_storable = to_lim - *to_p;
    if bytes_available > bytes_storable {
        from_lim = *from_p + bytes_storable;
        output_exhausted = true;
    }

    // Avoid copying partial characters (from incomplete input).
    {
        let from_lim_before = from_lim;
        from_lim = trim_to_complete_utf8_characters(from, *from_p, from_lim);
        if from_lim < from_lim_before {
            input_incomplete = true;
        }
    }

    {
        let bytes_to_copy = from_lim - *from_p;
        to[*to_p..*to_p + bytes_to_copy].copy_from_slice(&from[*from_p..from_lim]);
        *from_p += bytes_to_copy;
        *to_p += bytes_to_copy;
    }

    if output_exhausted {
        // needs to go first
        ConvertResult::OutputExhausted
    } else if input_incomplete {
        ConvertResult::InputIncomplete
    } else {
        ConvertResult::Completed
    }
}

/// Port of `latin1_toUtf8` (xmltok.c:503-525).
fn latin1_to_utf8(
    from: &[u8],
    from_p: &mut usize,
    from_lim: usize,
    to: &mut [u8],
    to_p: &mut usize,
    to_lim: usize,
) -> ConvertResult {
    loop {
        if *from_p == from_lim {
            return ConvertResult::Completed;
        }
        let c = from[*from_p];
        if c & 0x80 != 0 {
            if to_lim - *to_p < 2 {
                return ConvertResult::OutputExhausted;
            }
            to[*to_p] = (c >> 6) | UTF8_CVAL2;
            to[*to_p + 1] = (c & 0x3f) | 0x80;
            *to_p += 2;
            *from_p += 1;
        } else {
            if *to_p == to_lim {
                return ConvertResult::OutputExhausted;
            }
            to[*to_p] = c;
            *to_p += 1;
            *from_p += 1;
        }
    }
}

/// Port of `ascii_toUtf8` (xmltok.c:561-571).
fn ascii_to_utf8(
    from: &[u8],
    from_p: &mut usize,
    from_lim: usize,
    to: &mut [u8],
    to_p: &mut usize,
    to_lim: usize,
) -> ConvertResult {
    while *from_p < from_lim && *to_p < to_lim {
        to[*to_p] = from[*from_p];
        *to_p += 1;
        *from_p += 1;
    }
    if *to_p == to_lim && *from_p < from_lim {
        ConvertResult::OutputExhausted
    } else {
        ConvertResult::Completed
    }
}

/// Port of `little2_toUtf8` and `big2_toUtf8` (`DEFINE_UTF16_TO_UTF8`, xmltok.c:622-697,
/// with `GET_LO` and `GET_HI` of xmltok.c:720-730), `BIG` for big-endian.
fn utf16_to_utf8<const BIG: bool>(
    from_buf: &[u8],
    from_p: &mut usize,
    from_lim: usize,
    to: &mut [u8],
    to_p: &mut usize,
    to_lim: usize,
) -> ConvertResult {
    let get_lo = |p: usize| if BIG { from_buf[p + 1] } else { from_buf[p] };
    let get_hi = |p: usize| if BIG { from_buf[p] } else { from_buf[p + 1] };
    let mut from = *from_p;
    let from_lim = from + (((from_lim - from) >> 1) << 1); // shrink to even
    while from < from_lim {
        let lo = get_lo(from);
        let hi = get_hi(from);
        match hi {
            0 if lo < 0x80 => {
                if *to_p == to_lim {
                    *from_p = from;
                    return ConvertResult::OutputExhausted;
                }
                to[*to_p] = lo;
                *to_p += 1;
            }
            0..=0x7 => {
                if to_lim - *to_p < 2 {
                    *from_p = from;
                    return ConvertResult::OutputExhausted;
                }
                to[*to_p] = (lo >> 6) | (hi << 2) | UTF8_CVAL2;
                to[*to_p + 1] = (lo & 0x3f) | 0x80;
                *to_p += 2;
            }
            0xD8..=0xDB => {
                if to_lim - *to_p < 4 {
                    *from_p = from;
                    return ConvertResult::OutputExhausted;
                }
                if from_lim - from < 4 {
                    *from_p = from;
                    return ConvertResult::InputIncomplete;
                }
                let plane = (((hi & 0x3) << 2) | ((lo >> 6) & 0x3)) + 1;
                to[*to_p] = (plane >> 2) | UTF8_CVAL4;
                to[*to_p + 1] = ((lo >> 2) & 0xF) | ((plane & 0x3) << 4) | 0x80;
                from += 2;
                let lo2 = get_lo(from);
                to[*to_p + 2] = ((lo & 0x3) << 4) | ((get_hi(from) & 0x3) << 2) | (lo2 >> 6) | 0x80;
                to[*to_p + 3] = (lo2 & 0x3f) | 0x80;
                *to_p += 4;
            }
            _ => {
                if to_lim - *to_p < 3 {
                    *from_p = from;
                    return ConvertResult::OutputExhausted;
                }
                // 16 bits divided 4, 6, 6 amongst 3 bytes
                to[*to_p] = (hi >> 4) | UTF8_CVAL3;
                to[*to_p + 1] = ((hi & 0xf) << 2) | (lo >> 6) | 0x80;
                to[*to_p + 2] = (lo & 0x3f) | 0x80;
                *to_p += 3;
            }
        }
        from += 2;
    }
    *from_p = from;
    if from < from_lim {
        ConvertResult::InputIncomplete
    } else {
        ConvertResult::Completed
    }
}

/// Whether two ASCII strings are equal ignoring case; `s2` is upper case.
///
/// Port of `streqci` (xmltok.c:1006-1025).
fn streqci(s1: &[u8], s2: &[u8]) -> bool {
    s1.len() == s2.len()
        && s1
            .iter()
            .zip(s2)
            .all(|(&c1, &c2)| c1.to_ascii_uppercase() == c2)
}

/// The first character of `b[ptr..end]` if it converts to one byte of UTF-8, or -1.
///
/// Port of `toAscii` (xmltok.c:1034-1043).
fn to_ascii(enc: &Encoding, b: &[u8], ptr: usize, end: usize) -> i32 {
    let mut buf = [0u8; 1];
    let mut p = 0usize;
    let mut from = ptr;
    enc.utf8_convert(b, &mut from, end, &mut buf, &mut p, 1);
    if p == 0 {
        -1
    } else {
        // `buf[0]`, a `char`.
        i32::from(buf[0] as i8)
    }
}

/// Port of `isSpace` (xmltok.c:1045-1058).
fn is_space(c: i32) -> bool {
    matches!(c, 0x20 | 0xD | 0xA | 0x9)
}

/// A pseudo-attribute of an XML declaration: just optional white space (`*name` is `None`),
/// or white space then `name = 'value'`. `None` where the text isn't either, with `*next` at
/// the bad character.
///
/// Port of `parsePseudoAttribute` (xmltok.c:1060-1135): returns `false` where upstream
/// returns 0.
#[allow(clippy::too_many_arguments)]
fn parse_pseudo_attribute(
    enc: &Encoding,
    b: &[u8],
    mut ptr: usize,
    end: usize,
    name: &mut Option<usize>,
    name_end: &mut usize,
    val: &mut usize,
    next: &mut usize,
) -> bool {
    let mbpc = enc.min_bytes_per_char;
    if ptr == end {
        *name = None;
        return true;
    }
    if !is_space(to_ascii(enc, b, ptr, end)) {
        *next = ptr;
        return false;
    }
    loop {
        ptr += mbpc;
        if !is_space(to_ascii(enc, b, ptr, end)) {
            break;
        }
    }
    if ptr == end {
        *name = None;
        return true;
    }
    *name = Some(ptr);
    loop {
        let mut c = to_ascii(enc, b, ptr, end);
        if c == -1 {
            *next = ptr;
            return false;
        }
        if c == i32::from(b'=') {
            *name_end = ptr;
            break;
        }
        if is_space(c) {
            *name_end = ptr;
            loop {
                ptr += mbpc;
                c = to_ascii(enc, b, ptr, end);
                if !is_space(c) {
                    break;
                }
            }
            if c != i32::from(b'=') {
                *next = ptr;
                return false;
            }
            break;
        }
        ptr += mbpc;
    }
    if Some(ptr) == *name {
        *next = ptr;
        return false;
    }
    ptr += mbpc;
    let mut c = to_ascii(enc, b, ptr, end);
    while is_space(c) {
        ptr += mbpc;
        c = to_ascii(enc, b, ptr, end);
    }
    if c != i32::from(b'"') && c != i32::from(b'\'') {
        *next = ptr;
        return false;
    }
    let open = c;
    ptr += mbpc;
    *val = ptr;
    loop {
        c = to_ascii(enc, b, ptr, end);
        if c == open {
            break;
        }
        if !(i32::from(b'a')..=i32::from(b'z')).contains(&c)
            && !(i32::from(b'A')..=i32::from(b'Z')).contains(&c)
            && !(i32::from(b'0')..=i32::from(b'9')).contains(&c)
            && c != i32::from(b'.')
            && c != i32::from(b'-')
            && c != i32::from(b'_')
        {
            *next = ptr;
            return false;
        }
        ptr += mbpc;
    }
    *next = ptr + mbpc;
    true
}

/// The encoding names expat knows, in the order of their indices (xmltok.c:1474-1500): the
/// encoding index of `encodings[]`.
const ISO_8859_1_ENC: i32 = 0;
#[allow(dead_code)] // "US-ASCII" is found by its place in `ENCODING_NAMES`.
const US_ASCII_ENC: i32 = 1;
const UTF_8_ENC: i32 = 2;
const UTF_16_ENC: i32 = 3;
const UTF_16BE_ENC: i32 = 4;
const UTF_16LE_ENC: i32 = 5;
const NO_ENC: i32 = 6;
const UNKNOWN_ENC: i32 = -1;

/// `encodingNames[]` (xmltok.c:1486-1500, 1505-1507).
const ENCODING_NAMES: [&[u8]; 6] = [
    b"ISO-8859-1",
    b"US-ASCII",
    b"UTF-8",
    b"UTF-16",
    b"UTF-16BE",
    b"UTF-16LE",
];

/// The index of the encoding `name` (ignoring case): `NO_ENC` for none, `UNKNOWN_ENC` for one
/// expat doesn't know.
///
/// Port of `getEncodingIndex` (xmltok.c:1503-1515).
fn get_encoding_index(name: Option<&[u8]>) -> i32 {
    let Some(name) = name else {
        return NO_ENC;
    };
    for (i, known) in ENCODING_NAMES.iter().enumerate() {
        if streqci(name, known) {
            return i as i32;
        }
    }
    UNKNOWN_ENC
}

/// `encodings[]` (xmltok_ns.c:58-63): the encoding of each index; "UTF-16" alone is
/// big-endian, and no encoding (`NO_ENC`) is UTF-8.
static ENCODINGS: [&Encoding; 7] = [
    &LATIN1_ENCODING,
    &ASCII_ENCODING,
    &UTF8_ENCODING,
    &BIG2_ENCODING,
    &BIG2_ENCODING,
    &LITTLE2_ENCODING,
    &UTF8_ENCODING, // NO_ENC
];

/// The encoding a parser starts with, which detects the document's.
///
/// Port of `INIT_ENCODING` (xmltok.h:281-284) and the initial encoding `XmlInitEncoding`
/// makes (xmltok_ns.c:77-90): `INIT_ENC_INDEX`, the index of the encoding given to the parser
/// (its "protocol" encoding), is the only state it has. Its scanners set the parser's
/// encoding (`*encPtr`); here they return it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InitEncoding {
    /// `INIT_ENC_INDEX(enc)`.
    index: i32,
}

impl InitEncoding {
    /// The initial encoding of a parser given the encoding `name` (none: `None`); `None` for
    /// a name expat doesn't know.
    ///
    /// Port of `XmlInitEncoding` (xmltok_ns.c:77-90).
    pub fn new(name: Option<&[u8]>) -> Option<InitEncoding> {
        let i = get_encoding_index(name);
        if i == UNKNOWN_ENC {
            return None;
        }
        Some(InitEncoding { index: i })
    }

    /// The next token in `state` (`XML_PROLOG_STATE` or `XML_CONTENT_STATE`), detecting the
    /// encoding from the first bytes: a byte order mark, or the first bytes of "<", or the
    /// given encoding. Returns the token and, once detected, the encoding (upstream sets
    /// `*encPtr`).
    ///
    /// Port of `initScan` (xmltok.c:1531-1645), and of `initScanProlog` and
    /// `initScanContent` (xmltok_ns.c:65-75).
    pub fn scan(
        &self,
        state: i32,
        b: &[u8],
        ptr: usize,
        end: usize,
        next: &mut usize,
    ) -> (i32, Option<&'static Encoding>) {
        if ptr >= end {
            return (XML_TOK_NONE, None);
        }
        let init_index = self.index;
        if ptr + 1 == end {
            // only a single byte available for auto-detection
            // (`#ifndef XML_DTD`: the wheels' expat is built with `XML_DTD`.)
            // so we're parsing an external text entity...
            // if UTF-16 was externally specified, then we need at least 2 bytes
            if let UTF_16_ENC | UTF_16LE_ENC | UTF_16BE_ENC = init_index {
                return (XML_TOK_PARTIAL, None);
            }
            match b[ptr] {
                0xFE | 0xFF | 0xEF
                    if !(init_index == ISO_8859_1_ENC && state == XML_CONTENT_STATE) =>
                {
                    return (XML_TOK_PARTIAL, None);
                }
                0x00 | 0x3C => return (XML_TOK_PARTIAL, None),
                _ => {}
            }
        } else {
            match (u16::from(b[ptr]) << 8) | u16::from(b[ptr + 1]) {
                0xFEFF => {
                    if !(init_index == ISO_8859_1_ENC && state == XML_CONTENT_STATE) {
                        *next = ptr + 2;
                        return (XML_TOK_BOM, Some(ENCODINGS[UTF_16BE_ENC as usize]));
                    }
                }
                // 00 3C is handled in the default case
                0x3C00 => {
                    if !((init_index == UTF_16BE_ENC || init_index == UTF_16_ENC)
                        && state == XML_CONTENT_STATE)
                    {
                        let enc = ENCODINGS[UTF_16LE_ENC as usize];
                        return (enc.tok(state, b, ptr, end, next), Some(enc));
                    }
                }
                0xFFFE => {
                    if !(init_index == ISO_8859_1_ENC && state == XML_CONTENT_STATE) {
                        *next = ptr + 2;
                        return (XML_TOK_BOM, Some(ENCODINGS[UTF_16LE_ENC as usize]));
                    }
                }
                0xEFBB => {
                    // Maybe a UTF-8 BOM (EF BB BF). If there's an explicitly specified
                    // (external) encoding of ISO-8859-1 or some flavour of UTF-16 and this is
                    // an external text entity, don't look for the BOM, because it might be a
                    // legal data.
                    let skip = state == XML_CONTENT_STATE
                        && matches!(
                            init_index,
                            ISO_8859_1_ENC | UTF_16BE_ENC | UTF_16LE_ENC | UTF_16_ENC
                        );
                    if !skip {
                        if ptr + 2 == end {
                            return (XML_TOK_PARTIAL, None);
                        }
                        if b[ptr + 2] == 0xBF {
                            *next = ptr + 3;
                            return (XML_TOK_BOM, Some(ENCODINGS[UTF_8_ENC as usize]));
                        }
                    }
                }
                _ => {
                    if b[ptr] == 0 {
                        // 0 isn't a legal data character. Furthermore a document entity can
                        // only start with ASCII characters. So the only way this can fail to
                        // be big-endian UTF-16 if it it's an external parsed general entity
                        // that's labelled as UTF-16LE.
                        if !(state == XML_CONTENT_STATE && init_index == UTF_16LE_ENC) {
                            let enc = ENCODINGS[UTF_16BE_ENC as usize];
                            return (enc.tok(state, b, ptr, end, next), Some(enc));
                        }
                    } else if b[ptr + 1] == 0 {
                        // We could recover here in the case of an external entity whose
                        // second byte is 0, by assuming UTF-16LE. But we don't, because
                        // this would mean when presented just with a single byte, we
                        // couldn't reliably determine whether we needed further bytes.
                        if state != XML_CONTENT_STATE {
                            let enc = ENCODINGS[UTF_16LE_ENC as usize];
                            return (enc.tok(state, b, ptr, end, next), Some(enc));
                        }
                    }
                }
            }
        }
        let enc = ENCODINGS[init_index as usize];
        (enc.tok(state, b, ptr, end, next), Some(enc))
    }

    /// The initial encoding's `updatePosition`: UTF-8's.
    ///
    /// Port of `initUpdatePosition` (xmltok.c:1027-1032).
    pub fn update_position(&self, b: &[u8], ptr: usize, end: usize, pos: &mut Position) {
        UTF8_ENCODING.update_position(b, ptr, end, pos);
    }
}

/// `XmlGetUtf8InternalEncoding` (xmltok_ns.c:39-42).
pub fn xml_get_utf8_internal_encoding() -> &'static Encoding {
    &INTERNAL_UTF8_ENCODING
}

/// `XmlGetUtf16InternalEncoding` (xmltok_ns.c:44-56), for a little-endian machine.
pub fn xml_get_utf16_internal_encoding() -> &'static Encoding {
    &INTERNAL_LITTLE2_ENCODING
}

/// The encoding an XML declaration names in `b[ptr..end]`: `enc` itself for "UTF-16" in a
/// two-byte encoding, or one of `encodings[]`; `None` for a name expat doesn't know, or one
/// longer than 127 bytes in UTF-8.
///
/// Port of `findEncoding` (xmltok_ns.c:92-110).
fn find_encoding(
    enc: &'static Encoding,
    b: &[u8],
    ptr: usize,
    end: usize,
) -> Option<&'static Encoding> {
    const ENCODING_MAX: usize = 128;
    let mut buf = [0u8; ENCODING_MAX];
    let mut p = 0usize;
    let mut from = ptr;
    enc.utf8_convert(b, &mut from, end, &mut buf, &mut p, ENCODING_MAX - 1);
    if from != end {
        return None;
    }
    let name = &buf[..p];
    if streqci(name, b"UTF-16") && enc.min_bytes_per_char == 2 {
        return Some(enc);
    }
    let i = get_encoding_index(Some(name));
    if i == UNKNOWN_ENC {
        return None;
    }
    Some(ENCODINGS[i as usize])
}

/// What an XML declaration (or a text declaration) says.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct XmlDecl {
    /// `*versionPtr`, `*versionEndPtr`: the version's value.
    pub version: Option<(usize, usize)>,
    /// `*encodingName`: where the encoding's name starts.
    pub encoding_name: Option<usize>,
    /// `*namedEncodingPtr`: whether an encoding was named (`Some`), and which (`None` inside
    /// for a name expat doesn't know).
    pub encoding: Option<Option<&'static Encoding>>,
    /// `*standalonePtr`: 1 for "yes", 0 for "no", unchanged (-1 here) without one.
    pub standalone: i32,
}

/// Parses the XML declaration (or, for an external general entity, the text declaration)
/// `b[ptr..end]` (from "<?xml" to "?>"): `Err(bad)` with the position of the bad character
/// when it isn't well-formed.
///
/// Port of `XmlParseXmlDecl` (xmltok_ns.c:112-120) and `doParseXmlDecl` (xmltok.c:1151-1230).
/// Upstream writes each output through a pointer as it reads it, and some before it finds an
/// error; the parser reads them only after a success.
pub fn xml_parse_xml_decl(
    is_general_text_entity: bool,
    enc: &'static Encoding,
    b: &[u8],
    mut ptr: usize,
    mut end: usize,
) -> Result<XmlDecl, usize> {
    let mbpc = enc.min_bytes_per_char;
    let mut decl = XmlDecl {
        standalone: -1,
        ..XmlDecl::default()
    };
    let mut val = 0usize;
    let mut name: Option<usize> = None;
    let mut name_end = 0usize;
    ptr += 5 * mbpc;
    end -= 2 * mbpc;
    let mut next = ptr;
    if !parse_pseudo_attribute(
        enc,
        b,
        ptr,
        end,
        &mut name,
        &mut name_end,
        &mut val,
        &mut next,
    ) {
        return Err(next);
    }
    ptr = next;
    let Some(mut name_pos) = name else {
        return Err(ptr);
    };
    if !enc.name_matches_ascii(b, name_pos, name_end, b"version") {
        if !is_general_text_entity {
            return Err(name_pos);
        }
    } else {
        decl.version = Some((val, ptr));
        if !parse_pseudo_attribute(
            enc,
            b,
            ptr,
            end,
            &mut name,
            &mut name_end,
            &mut val,
            &mut next,
        ) {
            return Err(next);
        }
        ptr = next;
        match name {
            None => {
                if is_general_text_entity {
                    // a TextDecl must have an EncodingDecl
                    return Err(ptr);
                }
                return Ok(decl);
            }
            Some(n) => name_pos = n,
        }
    }
    if enc.name_matches_ascii(b, name_pos, name_end, b"encoding") {
        let c = to_ascii(enc, b, val, end);
        if !(i32::from(b'a')..=i32::from(b'z')).contains(&c)
            && !(i32::from(b'A')..=i32::from(b'Z')).contains(&c)
        {
            return Err(val);
        }
        decl.encoding_name = Some(val);
        decl.encoding = Some(find_encoding(enc, b, val, ptr - mbpc));
        if !parse_pseudo_attribute(
            enc,
            b,
            ptr,
            end,
            &mut name,
            &mut name_end,
            &mut val,
            &mut next,
        ) {
            return Err(next);
        }
        ptr = next;
        match name {
            None => return Ok(decl),
            Some(n) => name_pos = n,
        }
    }
    if !enc.name_matches_ascii(b, name_pos, name_end, b"standalone") || is_general_text_entity {
        return Err(name_pos);
    }
    if enc.name_matches_ascii(b, val, ptr - mbpc, b"yes") {
        decl.standalone = 1;
    } else if enc.name_matches_ascii(b, val, ptr - mbpc, b"no") {
        decl.standalone = 0;
    } else {
        return Err(val);
    }
    while is_space(to_ascii(enc, b, ptr, end)) {
        ptr += mbpc;
    }
    if ptr != end {
        return Err(ptr);
    }
    Ok(decl)
}

/// `result` if it is a character XML allows in a character reference, or -1: not a
/// surrogate, U+FFFE or U+FFFF, nor a control character other than tab, LF and CR.
///
/// Port of `checkCharRefNumber` (xmltok.c:1232-1254).
pub(super) fn check_char_ref_number(result: i32) -> i32 {
    match result >> 8 {
        0xD8..=0xDF => return -1,
        0 if LATIN1_TYPES[result as usize] == BT_NONXML => return -1,
        0xFF if result == 0xFFFE || result == 0xFFFF => return -1,
        _ => {}
    }
    result
}

/// Writes the UTF-8 of the character `c` into `buf` and returns its length (0 for a value
/// that isn't a character).
///
/// Port of `XmlUtf8Encode` (xmltok.c:1256-1290).
pub fn xml_utf8_encode(c: i32, buf: &mut [u8; XML_UTF8_ENCODE_MAX]) -> usize {
    // minN is minimum legal resulting value for N byte sequence
    const MIN2: i32 = 0x80;
    const MIN3: i32 = 0x800;
    const MIN4: i32 = 0x10000;

    if c < 0 {
        return 0; // LCOV_EXCL_LINE: this case is always eliminated beforehand
    }
    if c < MIN2 {
        buf[0] = c as u8; // `| UTF8_cval1`, 0
        return 1;
    }
    if c < MIN3 {
        buf[0] = ((c >> 6) as u8) | UTF8_CVAL2;
        buf[1] = ((c & 0x3f) as u8) | 0x80;
        return 2;
    }
    if c < MIN4 {
        buf[0] = ((c >> 12) as u8) | UTF8_CVAL3;
        buf[1] = (((c >> 6) & 0x3f) as u8) | 0x80;
        buf[2] = ((c & 0x3f) as u8) | 0x80;
        return 3;
    }
    if c < 0x110000 {
        buf[0] = ((c >> 18) as u8) | UTF8_CVAL4;
        buf[1] = (((c >> 12) & 0x3f) as u8) | 0x80;
        buf[2] = (((c >> 6) & 0x3f) as u8) | 0x80;
        buf[3] = ((c & 0x3f) as u8) | 0x80;
        return 4;
    }
    0 // LCOV_EXCL_LINE: this case too is eliminated before calling
}

#[cfg(test)]
#[path = "xmltok_tests.rs"]
mod tests;
