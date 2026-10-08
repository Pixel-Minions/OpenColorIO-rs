// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Text helpers of the XML readers: trimming, the number scanner and its tokenizer.
//!
//! Port of `src/OpenColorIO/fileformats/xmlutils/XMLReaderUtils.h` and `XMLReaderUtils.cpp`
//! (@ v2.5.2).
//!
//! **Bytes past the end.** Upstream's scanners take a `const char *` and a length, and some
//! read the byte at the length, or (`NumberUtils::from_chars` on Linux, `strtod_l`) on to the
//! string's terminating null: the readers pass `std::string`s, whose byte at the length is a
//! null, or a buffer whose byte after the length the caller provides. The port's scanners
//! take the bytes upstream may read, `s`, and the length apart; a byte past `s` reads as a
//! null.

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::utils::number_utils::{Errc, Flavor, from_chars_f64};
use ocio_ops::{Exception, Result};

// Strings used by CDL and CLF parsers or writers (XMLReaderUtils.h:24-38).

/// `ATTR_ID`.
pub const ATTR_ID: &[u8] = b"id";
/// `ATTR_NAME`.
pub const ATTR_NAME: &[u8] = b"name";

/// `CDL_TAG_COLOR_CORRECTION`.
pub const CDL_TAG_COLOR_CORRECTION: &[u8] = b"ColorCorrection";

/// `TAG_DESCRIPTION`.
pub const TAG_DESCRIPTION: &[u8] = b"Description";
/// `TAG_OFFSET`.
pub const TAG_OFFSET: &[u8] = b"Offset";
/// `TAG_POWER`.
pub const TAG_POWER: &[u8] = b"Power";
/// `TAG_SATNODE`.
pub const TAG_SATNODE: &[u8] = b"SatNode";
/// `TAG_SATNODEALT`.
pub const TAG_SATNODEALT: &[u8] = b"SATNode";
/// `TAG_SATURATION`.
pub const TAG_SATURATION: &[u8] = b"Saturation";
/// `TAG_SLOPE`.
pub const TAG_SLOPE: &[u8] = b"Slope";
/// `TAG_SOPNODE`.
pub const TAG_SOPNODE: &[u8] = b"SOPNode";

/// The byte at `i` of `s`, or the terminating null past its end.
fn at(s: &[u8], i: usize) -> u8 {
    s.get(i).copied().unwrap_or(0)
}

/// The first `len` bytes of `p_str`, at most `limit` (mainly used for display purpose).
///
/// Port of `TruncateString(const char *, size_t, size_t)` (XMLReaderUtils.h:40-46 @ v2.5.2).
pub fn truncate_string_to(p_str: &[u8], len: usize, limit: usize) -> Vec<u8> {
    let new_len = if limit < len { limit } else { len };
    (0..new_len).map(|i| at(p_str, i)).collect()
}

/// [`truncate_string_to`] to the default limit of 17 bytes.
///
/// Port of `TruncateString(const char *, size_t)` (XMLReaderUtils.h:48-53 @ v2.5.2).
pub fn truncate_string(p_str: &[u8], len: usize) -> Vec<u8> {
    const MAX_SIZE: usize = 17;
    truncate_string_to(p_str, len, MAX_SIZE)
}

/// Is `c` a 'space' character ( '\n', '\t', ' ' ... )? (Not `std::isspace`.)
///
/// Port of `IsSpace` (XMLReaderUtils.h:64-71 @ v2.5.2).
pub fn is_space(c: u8) -> bool {
    // Note: \n is unix while \r\n is windows line feed.
    c == b' ' || c == b'\n' || c == b'\t' || c == b'\r' || c == 0x0b || c == 0x0c
}

/// Is the character a valid number delimiter?
///
/// Port of `IsNumberDelimiter` (XMLReaderUtils.h:73-77 @ v2.5.2).
pub fn is_number_delimiter(c: u8) -> bool {
    is_space(c) || c == b','
}

/// Port of `IsNotSpace` (XMLReaderUtils.cpp:11-14 @ v2.5.2).
pub fn is_not_space(c: u8) -> bool {
    !is_space(c)
}

/// Trim from start.
///
/// Port of `LTrim` (XMLReaderUtils.cpp:16-21 @ v2.5.2).
pub(crate) fn l_trim(s: &mut Vec<u8>) {
    let first = s.iter().position(|&c| is_not_space(c)).unwrap_or(s.len());
    s.drain(..first);
}

/// Trim from end.
///
/// Port of `RTrim` (XMLReaderUtils.cpp:23-28 @ v2.5.2).
pub(crate) fn r_trim(s: &mut Vec<u8>) {
    let end = s
        .iter()
        .rposition(|&c| is_not_space(c))
        .map_or(0, |p| p + 1);
    s.truncate(end);
}

/// Trim from both ends.
///
/// Port of `Trim` (XMLReaderUtils.cpp:30-35 @ v2.5.2).
pub fn trim(s: &mut Vec<u8>) {
    l_trim(s);
    r_trim(s);
}

/// The position of the first non-whitespace character of `s`'s first `len` bytes, or `len`.
/// Reads the byte at `len` too.
///
/// Port of `FindFirstNonWhiteSpace` (XMLReaderUtils.cpp:37-58 @ v2.5.2).
fn find_first_non_white_space(s: &[u8], len: usize) -> usize {
    let mut pos = 0usize;
    loop {
        if !is_space(at(s, pos)) {
            return pos;
        }
        if pos == len {
            return len;
        }
        pos += 1;
    }
}

/// The position of the last non-whitespace character of `s`'s first `len` bytes (`len` > 0),
/// or 0.
///
/// Port of `FindLastNonWhiteSpace` (XMLReaderUtils.cpp:60-82 @ v2.5.2).
fn find_last_non_white_space(s: &[u8], len: usize) -> usize {
    let mut pos = len - 1;
    loop {
        if !is_space(at(s, pos)) {
            return pos;
        }
        if pos == 0 {
            return 0;
        }
        pos -= 1;
    }
}

/// The start (first non space character) and end (just after the last non space character)
/// of `s`'s first `length` bytes; `(0, 0)` when there are none, or when `s` starts with a
/// null.
///
/// Port of `FindSubString` (XMLReaderUtils.cpp:84-114 @ v2.5.2).
pub fn find_sub_string(s: &[u8], length: usize) -> (usize, usize) {
    if at(s, 0) == 0 {
        return (0, 0); // nothing to Trim.
    }

    let start = find_first_non_white_space(s, length);
    if start == length {
        // str only contains spaces, tabs or newlines.
        // Return an empty string.
        return (0, 0);
    }

    // It is guaranteed here that end will not be 'npos'.
    // At worst, it will equal start.
    let mut end = find_last_non_white_space(s, length);

    // end-start should give the number of valid characters.
    if !is_space(at(s, end)) {
        end += 1;
    }
    (start, end)
}

/// The position of the next character to start scanning at: past the delimiters (spaces,
/// commas, tabs and newlines) from `pos`, or `len`.
///
/// Port of `FindNextTokenStart` (XMLReaderUtils.h:79-101 @ v2.5.2).
pub fn find_next_token_start(s: &[u8], len: usize, mut pos: usize) -> usize {
    if pos >= len {
        return len;
    }
    while is_number_delimiter(at(s, pos)) {
        pos += 1;
        if pos >= len {
            return len;
        }
    }
    pos
}

/// The position of the next delimiter (spaces and commas) from `pos`, or `len`.
///
/// Port of `FindDelim` (XMLReaderUtils.h:103-125 @ v2.5.2).
pub fn find_delim(s: &[u8], len: usize, mut pos: usize) -> usize {
    if pos >= len {
        return len;
    }
    while !is_number_delimiter(at(s, pos)) {
        pos += 1;
        if pos >= len {
            return len;
        }
    }
    pos
}

/// A number type the readers parse: `ParseNumber`'s `T` (`float`, `double` and `unsigned`
/// in OCIO).
pub trait XmlNumber: Copy {
    /// `std::is_floating_point<T>::value`.
    const IS_FLOATING_POINT: bool;
    /// `T(0)`.
    fn zero() -> Self;
    /// `(T)val`.
    fn from_f64(val: f64) -> Self;
    /// `static_cast<double>(value)`.
    fn to_f64(self) -> f64;
}

impl XmlNumber for f32 {
    const IS_FLOATING_POINT: bool = true;
    fn zero() -> Self {
        0.0
    }
    fn from_f64(val: f64) -> Self {
        val as f32
    }
    fn to_f64(self) -> f64 {
        f64::from(self)
    }
}

impl XmlNumber for f64 {
    const IS_FLOATING_POINT: bool = true;
    fn zero() -> Self {
        0.0
    }
    fn from_f64(val: f64) -> Self {
        val
    }
    fn to_f64(self) -> f64 {
        self
    }
}

impl XmlNumber for u32 {
    const IS_FLOATING_POINT: bool = false;
    fn zero() -> Self {
        0
    }
    /// `(unsigned)val`: both wheels convert through a 64-bit truncation (`cvttsd2si`) and
    /// keep the low 32 bits. A value out of that range is undefined in C++, and its result is
    /// only ever refused (`IsValid` compares it back with `val`), so the port's saturation
    /// there changes nothing.
    fn from_f64(val: f64) -> Self {
        val as i64 as u32
    }
    fn to_f64(self) -> f64 {
        f64::from(self)
    }
}

/// When using an integer `ParseNumber` template, it is an error if the string actually
/// contains a number with a decimal part.
///
/// Port of `IsValid` (XMLReaderUtils.h:128-138 @ v2.5.2).
fn is_valid<T: XmlNumber>(value: T, val: f64) -> bool {
    // Returns true, if T is the type float, double or long double.
    if T::IS_FLOATING_POINT {
        return true;
    }
    value.to_f64() == val
}

/// The error message `"<prefix><parsedStr><middle><fullStr, at most 100 bytes>'."`.
fn parse_number_error(
    s: &[u8],
    start_pos: usize,
    end_pos: usize,
    prefix: &str,
    middle: &str,
) -> Exception {
    let full_str: Vec<u8> = (0..end_pos).map(|i| at(s, i)).collect();
    let parsed_str: Vec<u8> = (start_pos..end_pos).map(|i| at(s, i)).collect();
    let mut oss = OStringStream::new(Crt::NATIVE);
    oss.put_str(prefix);
    oss.put_bytes(&parsed_str);
    oss.put_str(middle);
    oss.put_bytes(&truncate_string_to(&full_str, end_pos, 100));
    oss.put_str("'.");
    Exception::new(oss.into_bytes())
}

/// The first number of `s[start_pos..end_pos]`, which must hold nothing else but white space;
/// the byte at `end_pos` (or the null past `s`) must not continue the number.
///
/// Port of `ParseNumber` (XMLReaderUtils.h:140-208 @ v2.5.2), with this platform's
/// `NumberUtils::from_chars`.
pub fn parse_number<T: XmlNumber>(
    s: &[u8],
    start_pos: usize,
    end_pos: usize,
    value: &mut T,
) -> Result<()> {
    if end_pos == start_pos {
        return Err(Exception::new("ParseNumber: nothing to parse."));
    }

    let start_parse = &s[start_pos.min(s.len())..];

    let mut val = 0.0f64;

    let (adjusted_start_pos, adjusted_end_pos) = find_sub_string(start_parse, end_pos - start_pos);

    let first = &start_parse[adjusted_start_pos.min(start_parse.len())..];
    let result = from_chars_f64(
        Flavor::NATIVE,
        first,
        (adjusted_end_pos - adjusted_start_pos).min(first.len()),
        &mut val,
    );

    *value = T::from_f64(val);

    if result.ec == Errc::InvalidArgument {
        return Err(parse_number_error(
            s,
            start_pos,
            end_pos,
            "ParserNumber: Characters '",
            "' can not be parsed to numbers in '",
        ));
    } else if !is_valid(*value, val) {
        return Err(parse_number_error(
            s,
            start_pos,
            end_pos,
            "ParserNumber: Characters '",
            "' are illegal in '",
        ));
    } else if start_pos + adjusted_start_pos + result.ptr != end_pos {
        // Number is followed by something.
        return Err(parse_number_error(
            s,
            start_pos,
            end_pos,
            "ParserNumber: '",
            "' number is followed by unexpected characters in '",
        ));
    }
    Ok(())
}

/// Extracts the next number of `s`'s first `len` bytes from `*pos`; `*pos` moves to the start
/// of the following one, or `len`.
///
/// Port of `GetNextNumber` (XMLReaderUtils.h:210-228 @ v2.5.2).
pub fn get_next_number<T: XmlNumber>(
    s: &[u8],
    len: usize,
    pos: &mut usize,
    num: &mut T,
) -> Result<()> {
    *pos = find_next_token_start(s, len, *pos);

    if *pos != len {
        let next_pos = find_delim(s, len, *pos);
        parse_number(s, *pos, next_pos, num)?;
        *pos = next_pos;

        if *pos != len {
            *pos = find_next_token_start(s, len, next_pos);
        }
    }
    Ok(())
}

/// The numbers of a string like "0 1 2" (`s`'s first `len` bytes).
///
/// Port of `GetNumbers` (XMLReaderUtils.h:230-246 @ v2.5.2).
pub fn get_numbers<T: XmlNumber>(s: &[u8], len: usize) -> Result<Vec<T>> {
    let mut numbers = Vec::new();

    let mut pos = find_next_token_start(s, len, 0);
    while pos != len {
        let mut num = T::zero();
        get_next_number(s, len, &mut pos, &mut num)?;
        numbers.push(num);
    }

    Ok(numbers)
}

#[cfg(test)]
#[path = "xml_reader_utils_tests.rs"]
mod tests;
