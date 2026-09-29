// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/utils/StringUtils.h` @ v2.5.2: locale-independent string helpers.
//!
//! C++ `std::string` is a byte string. Every function here only compares, cuts or maps at
//! ASCII bytes, so on UTF-8 text the `&str` versions give the same bytes as the C++; the
//! `*_bytes` trims serve raw bytes. Separators are C++ `char`s and must be ASCII here.
//! Functions that take `const char *` in C++ (and so stop at a NUL) are marked `c_str`.

/// `StringUtils::StringVec`.
pub type StringVec = Vec<String>;

/// Port of `StringUtils::Lower(unsigned char)` (StringUtils.h:26-34 @ v2.5.2): ASCII only,
/// avoiding the locale's "Turkish I".
pub fn lower_char(c: u8) -> u8 {
    if c.is_ascii_uppercase() {
        c + (b'a' - b'A')
    } else {
        c
    }
}

/// Port of `StringUtils::Upper(unsigned char)` (StringUtils.h:37-45 @ v2.5.2).
pub fn upper_char(c: u8) -> u8 {
    if c.is_ascii_lowercase() {
        c - (b'a' - b'A')
    } else {
        c
    }
}

/// Port of `StringUtils::IsSpace` (StringUtils.h:49-52 @ v2.5.2): any byte up to and
/// including `' '`, whatever the locale.
pub fn is_space(c: u8) -> bool {
    c <= b' '
}

fn map_ascii(s: &str, f: fn(u8) -> u8) -> String {
    let bytes: Vec<u8> = s.bytes().map(f).collect();
    // Only ASCII letters change, so the bytes stay UTF-8.
    String::from_utf8(bytes).expect("ASCII mapping keeps UTF-8")
}

/// The part of `s` a C++ `const char *` sees: up to the first NUL.
fn c_str(s: &str) -> &str {
    s.split('\0').next().unwrap_or("")
}

/// Port of `StringUtils::Lower(std::string)` (StringUtils.h:55-60 @ v2.5.2).
pub fn lower(s: &str) -> String {
    map_ascii(s, lower_char)
}

/// Port of `StringUtils::Lower(const char *)` (StringUtils.h:62-67 @ v2.5.2): `None` (a null
/// pointer) gives `""`.
pub fn lower_c_str(s: Option<&str>) -> String {
    s.map_or_else(String::new, |s| lower(c_str(s)))
}

/// Port of `StringUtils::Upper(std::string)` (StringUtils.h:70-75 @ v2.5.2).
pub fn upper(s: &str) -> String {
    map_ascii(s, upper_char)
}

/// Port of `StringUtils::Upper(const char *)` (StringUtils.h:77-82 @ v2.5.2).
pub fn upper_c_str(s: Option<&str>) -> String {
    s.map_or_else(String::new, |s| upper(c_str(s)))
}

/// Port of `StringUtils::Compare` (StringUtils.h:85-88 @ v2.5.2): equal ignoring ASCII case.
pub fn compare(left: &str, right: &str) -> bool {
    lower(left) == lower(right)
}

/// Port of `StringUtils::EndsWith` (StringUtils.h:92-96 @ v2.5.2), case-sensitive.
pub fn ends_with(s: &str, suffix: &str) -> bool {
    s.as_bytes().ends_with(suffix.as_bytes())
}

/// Port of `StringUtils::StartsWith(const std::string &, const std::string &)`
/// (StringUtils.h:100-103 @ v2.5.2), case-sensitive.
pub fn starts_with(s: &str, prefix: &str) -> bool {
    s.as_bytes().starts_with(prefix.as_bytes())
}

/// Port of `StringUtils::StartsWith(const std::string &, char)` (StringUtils.h:107-110 @
/// v2.5.2).
pub fn starts_with_char(s: &str, prefix: u8) -> bool {
    s.as_bytes().first() == Some(&prefix)
}

/// Port of `StringUtils::LeftTrim(std::string, char)` (StringUtils.h:113-118 @ v2.5.2).
pub fn left_trim_char(s: &str, c: u8) -> String {
    assert!(c.is_ascii(), "separator must be ASCII");
    let start = s.bytes().position(|ch| ch != c).unwrap_or(s.len());
    s[start..].to_string()
}

/// Port of `StringUtils::LeftTrim(std::string)` (StringUtils.h:121-126 @ v2.5.2): drops
/// leading bytes up to `' '`.
pub fn left_trim(s: &str) -> String {
    String::from_utf8(left_trim_bytes(s.as_bytes()).to_vec()).expect("cut at an ASCII byte")
}

/// Port of `StringUtils::RightTrim(std::string, char)` (StringUtils.h:129-134 @ v2.5.2).
pub fn right_trim_char(s: &str, c: u8) -> String {
    assert!(c.is_ascii(), "separator must be ASCII");
    let end = s.bytes().rposition(|ch| ch != c).map_or(0, |i| i + 1);
    s[..end].to_string()
}

/// Port of `StringUtils::RightTrim(std::string)` (StringUtils.h:137-143 @ v2.5.2).
pub fn right_trim(s: &str) -> String {
    String::from_utf8(right_trim_bytes(s.as_bytes()).to_vec()).expect("cut at an ASCII byte")
}

/// Port of `StringUtils::Trim(std::string, char)` (StringUtils.h:146-149 @ v2.5.2).
pub fn trim_char(s: &str, c: u8) -> String {
    left_trim_char(&right_trim_char(s, c), c)
}

/// Port of `StringUtils::Trim(std::string)` (StringUtils.h:152-155 @ v2.5.2).
pub fn trim(s: &str) -> String {
    left_trim(&right_trim(s))
}

/// Port of `StringUtils::Trim(StringVec &)` (StringUtils.h:157-163 @ v2.5.2): trims every
/// entry in place.
pub fn trim_vec(list: &mut StringVec) {
    for entry in list.iter_mut() {
        *entry = trim(entry);
    }
}

/// `StringUtils::LeftTrim(std::string)` on raw bytes. The C++ passes each `char` to
/// `IsSpace(unsigned char)`, so bytes of 0x80 and above are never spaces.
pub fn left_trim_bytes(s: &[u8]) -> &[u8] {
    let start = s.iter().position(|&c| !is_space(c)).unwrap_or(s.len());
    &s[start..]
}

/// `StringUtils::RightTrim(std::string)` on raw bytes.
pub fn right_trim_bytes(s: &[u8]) -> &[u8] {
    let end = s.iter().rposition(|&c| !is_space(c)).map_or(0, |i| i + 1);
    &s[..end]
}

/// `StringUtils::Trim(std::string)` on raw bytes.
pub fn trim_bytes(s: &[u8]) -> &[u8] {
    left_trim_bytes(right_trim_bytes(s))
}

/// Port of `StringUtils::IsEmptyOrWhiteSpace` (StringUtils.h:166-169 @ v2.5.2).
pub fn is_empty_or_white_space(s: &str) -> bool {
    s.bytes().all(is_space)
}

/// Port of `StringUtils::Split` (StringUtils.h:172-191 @ v2.5.2): the `std::getline` loop,
/// plus an empty last entry when the string ends with the separator. `""` gives `[""]`.
pub fn split(s: &str, separator: u8) -> StringVec {
    assert!(separator.is_ascii(), "separator must be ASCII");
    if s.is_empty() {
        return vec![String::new()];
    }
    let bytes = s.as_bytes();
    let mut results = StringVec::new();
    // std::getline: each call extracts up to and including the next separator, and fails
    // only when it extracts nothing at all (end of input).
    let mut pos = 0;
    while pos < bytes.len() {
        let end = bytes[pos..]
            .iter()
            .position(|&c| c == separator)
            .map_or(bytes.len(), |i| pos + i);
        results.push(s[pos..end].to_string());
        pos = end + 1;
    }
    if bytes.last() == Some(&separator) {
        results.push(String::new());
    }
    results
}

/// Port of `StringUtils::Join` (StringUtils.h:194-211 @ v2.5.2): entries separated by the
/// separator and a space.
pub fn join(strings: &[String], separator: u8) -> String {
    assert!(separator.is_ascii(), "separator must be ASCII");
    match strings {
        [] => String::new(),
        [only] => only.clone(),
        [first, rest @ ..] => {
            let mut result = first.clone();
            for s in rest {
                result.push(char::from(separator));
                result.push(' ');
                result.push_str(s);
            }
            result
        }
    }
}

/// Port of `StringUtils::SplitByLines` (StringUtils.h:214-228 @ v2.5.2): `std::getline` on
/// `'\n'`, so a final `'\n'` ends the last line instead of starting an empty one, and `'\r'`
/// stays. `""` gives `[""]`.
pub fn split_by_lines(s: &str) -> StringVec {
    if s.is_empty() {
        return vec![String::new()];
    }
    let mut lines: StringVec = s.split('\n').map(str::to_string).collect();
    if s.ends_with('\n') {
        lines.pop();
    }
    lines
}

/// `isspace` in the classic locale, which `std::istream >> std::string` uses.
fn is_classic_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\x0b' | '\x0c' | '\r')
}

/// Port of `StringUtils::SplitByWhiteSpaces` (StringUtils.h:231-236 @ v2.5.2):
/// `std::istream_iterator<std::string>`, i.e. the words between runs of classic-locale
/// whitespace (space, `\t`, `\n`, `\v`, `\f`, `\r`; not every byte up to `' '`).
pub fn split_by_white_spaces(s: &str) -> StringVec {
    s.split(is_classic_space)
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// Port of `StringUtils::Find` (StringUtils.h:240-243 @ v2.5.2): `std::string::find`;
/// `None` is `std::string::npos`.
pub fn find(subject: &str, search: &str) -> Option<usize> {
    subject.find(search)
}

/// Port of `StringUtils::ReverseFind` (StringUtils.h:247-250 @ v2.5.2): `std::string::rfind`.
pub fn reverse_find(subject: &str, search: &str) -> Option<usize> {
    subject.rfind(search)
}

/// Port of `StringUtils::ReplaceInPlace` (StringUtils.h:253-268 @ v2.5.2): replaces every
/// occurrence, scanning on after each replacement; `false` if nothing changed.
pub fn replace_in_place(subject: &mut String, search: &str, replace: &str) -> bool {
    if search.is_empty() {
        return false;
    }
    let mut changed = false;
    let mut pos = 0;
    while let Some(found) = subject[pos..].find(search) {
        let at = pos + found;
        subject.replace_range(at..at + search.len(), replace);
        pos = at + replace.len();
        changed = true;
    }
    changed
}

/// Port of `StringUtils::Replace` (StringUtils.h:271-276 @ v2.5.2).
pub fn replace(subject: &str, search: &str, replace: &str) -> String {
    let mut s = subject.to_string();
    replace_in_place(&mut s, search, replace);
    s
}

/// Port of `StringUtils::Contain` (StringUtils.h:279-287 @ v2.5.2): case-insensitive, whole
/// entries. The C++ compares `c_str()`s, so each side stops at a NUL.
pub fn contain(list: &[String], entry: &str) -> bool {
    list.iter().any(|e| compare(c_str(e), c_str(entry)))
}

/// Port of `StringUtils::Remove` (StringUtils.h:291-305 @ v2.5.2): removes the first
/// case-insensitive match; `true` if one was found.
pub fn remove(list: &mut StringVec, entry: &str) -> bool {
    match list.iter().position(|e| compare(c_str(e), c_str(entry))) {
        Some(i) => {
            list.remove(i);
            true
        }
        None => false,
    }
}

#[cfg(test)]
#[path = "string_utils_tests.rs"]
mod tests;
