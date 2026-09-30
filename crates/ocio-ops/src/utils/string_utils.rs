// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `src/utils/StringUtils.h` @ v2.5.2: locale-independent string helpers.
//!
//! C++ `std::string` is a byte string, and OCIO passes these functions whatever bytes it
//! was given (names, descriptions and file content need not be UTF-8), so the port works on
//! byte strings: `&[u8]` in, `Vec<u8>` (or a sub-slice) out. Every function compares, cuts
//! or maps bytes exactly as the C++ does; separators are C++ `char`s, i.e. any byte.
//! Functions that take `const char *` in C++ (and so stop at a NUL) are marked `c_str`.

/// `StringUtils::StringVec`: a list of C++ strings.
pub type StringVec = Vec<Vec<u8>>;

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
/// including `' '`, whatever the locale. The C++ takes an `unsigned char`, so bytes of 0x80
/// and above are never spaces.
pub fn is_space(c: u8) -> bool {
    c <= b' '
}

/// The part of `s` a C++ `const char *` sees: up to the first NUL (all of `s` if it has none).
/// This is what `std::string::c_str()` gives a function that takes a `const char *`.
pub fn c_str(s: &[u8]) -> &[u8] {
    s.split(|&c| c == 0).next().unwrap_or(&[])
}

/// Port of `StringUtils::Lower(std::string)` (StringUtils.h:55-60 @ v2.5.2).
pub fn lower(s: &[u8]) -> Vec<u8> {
    s.iter().map(|&c| lower_char(c)).collect()
}

/// Port of `StringUtils::Lower(const char *)` (StringUtils.h:62-67 @ v2.5.2): `None` (a null
/// pointer) gives `""`.
pub fn lower_c_str(s: Option<&[u8]>) -> Vec<u8> {
    s.map_or_else(Vec::new, |s| lower(c_str(s)))
}

/// Port of `StringUtils::Upper(std::string)` (StringUtils.h:70-75 @ v2.5.2).
pub fn upper(s: &[u8]) -> Vec<u8> {
    s.iter().map(|&c| upper_char(c)).collect()
}

/// Port of `StringUtils::Upper(const char *)` (StringUtils.h:77-82 @ v2.5.2).
pub fn upper_c_str(s: Option<&[u8]>) -> Vec<u8> {
    s.map_or_else(Vec::new, |s| upper(c_str(s)))
}

/// Port of `StringUtils::Compare` (StringUtils.h:85-88 @ v2.5.2): equal ignoring ASCII case.
pub fn compare(left: &[u8], right: &[u8]) -> bool {
    lower(left) == lower(right)
}

/// Port of `StringUtils::EndsWith` (StringUtils.h:92-96 @ v2.5.2), case-sensitive.
pub fn ends_with(s: &[u8], suffix: &[u8]) -> bool {
    s.ends_with(suffix)
}

/// Port of `StringUtils::StartsWith(const std::string &, const std::string &)`
/// (StringUtils.h:100-103 @ v2.5.2), case-sensitive.
pub fn starts_with(s: &[u8], prefix: &[u8]) -> bool {
    s.starts_with(prefix)
}

/// Port of `StringUtils::StartsWith(const std::string &, char)` (StringUtils.h:107-110 @
/// v2.5.2).
pub fn starts_with_char(s: &[u8], prefix: u8) -> bool {
    s.first() == Some(&prefix)
}

/// Port of `StringUtils::LeftTrim(std::string, char)` (StringUtils.h:113-118 @ v2.5.2).
pub fn left_trim_char(s: &[u8], c: u8) -> &[u8] {
    let start = s.iter().position(|&ch| ch != c).unwrap_or(s.len());
    &s[start..]
}

/// Port of `StringUtils::LeftTrim(std::string)` (StringUtils.h:121-126 @ v2.5.2): drops
/// leading bytes up to `' '`.
pub fn left_trim(s: &[u8]) -> &[u8] {
    let start = s.iter().position(|&c| !is_space(c)).unwrap_or(s.len());
    &s[start..]
}

/// Port of `StringUtils::RightTrim(std::string, char)` (StringUtils.h:129-134 @ v2.5.2).
pub fn right_trim_char(s: &[u8], c: u8) -> &[u8] {
    let end = s.iter().rposition(|&ch| ch != c).map_or(0, |i| i + 1);
    &s[..end]
}

/// Port of `StringUtils::RightTrim(std::string)` (StringUtils.h:137-143 @ v2.5.2).
pub fn right_trim(s: &[u8]) -> &[u8] {
    let end = s.iter().rposition(|&c| !is_space(c)).map_or(0, |i| i + 1);
    &s[..end]
}

/// Port of `StringUtils::Trim(std::string, char)` (StringUtils.h:146-149 @ v2.5.2).
pub fn trim_char(s: &[u8], c: u8) -> &[u8] {
    left_trim_char(right_trim_char(s, c), c)
}

/// Port of `StringUtils::Trim(std::string)` (StringUtils.h:152-155 @ v2.5.2).
pub fn trim(s: &[u8]) -> &[u8] {
    left_trim(right_trim(s))
}

/// Port of `StringUtils::Trim(StringVec &)` (StringUtils.h:157-163 @ v2.5.2): trims every
/// entry in place.
pub fn trim_vec(list: &mut StringVec) {
    for entry in list.iter_mut() {
        *entry = trim(entry).to_vec();
    }
}

/// Port of `StringUtils::IsEmptyOrWhiteSpace` (StringUtils.h:166-169 @ v2.5.2).
pub fn is_empty_or_white_space(s: &[u8]) -> bool {
    s.iter().all(|&c| is_space(c))
}

/// Port of `StringUtils::Split` (StringUtils.h:172-191 @ v2.5.2): the `std::getline` loop,
/// plus an empty last entry when the string ends with the separator. `""` gives `[""]`.
pub fn split(s: &[u8], separator: u8) -> StringVec {
    if s.is_empty() {
        return vec![Vec::new()];
    }
    let mut results = StringVec::new();
    // std::getline: each call extracts up to and including the next separator, and fails
    // only when it extracts nothing at all (end of input).
    let mut pos = 0;
    while pos < s.len() {
        let end = s[pos..]
            .iter()
            .position(|&c| c == separator)
            .map_or(s.len(), |i| pos + i);
        results.push(s[pos..end].to_vec());
        pos = end + 1;
    }
    if s.last() == Some(&separator) {
        results.push(Vec::new());
    }
    results
}

/// Port of `StringUtils::Join` (StringUtils.h:194-211 @ v2.5.2): entries separated by the
/// separator and a space.
pub fn join(strings: &[Vec<u8>], separator: u8) -> Vec<u8> {
    match strings {
        [] => Vec::new(),
        [only] => only.clone(),
        [first, rest @ ..] => {
            let mut result = first.clone();
            for s in rest {
                result.push(separator);
                result.push(b' ');
                result.extend_from_slice(s);
            }
            result
        }
    }
}

/// Port of `StringUtils::SplitByLines` (StringUtils.h:214-228 @ v2.5.2): `std::getline` on
/// `'\n'`, so a final `'\n'` ends the last line instead of starting an empty one, and `'\r'`
/// stays. `""` gives `[""]`.
pub fn split_by_lines(s: &[u8]) -> StringVec {
    if s.is_empty() {
        return vec![Vec::new()];
    }
    let mut lines: StringVec = s.split(|&c| c == b'\n').map(<[u8]>::to_vec).collect();
    if s.ends_with(b"\n") {
        lines.pop();
    }
    lines
}

/// `isspace` in the classic locale, which `std::istream >> std::string` uses: space, `\t`,
/// `\n`, `\v`, `\f` and `\r`, and no byte of 0x80 and above.
fn is_classic_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

/// Port of `StringUtils::SplitByWhiteSpaces` (StringUtils.h:231-236 @ v2.5.2):
/// `std::istream_iterator<std::string>`, i.e. the words between runs of classic-locale
/// whitespace (not every byte up to `' '`).
pub fn split_by_white_spaces(s: &[u8]) -> StringVec {
    s.split(|&c| is_classic_space(c))
        .filter(|w| !w.is_empty())
        .map(<[u8]>::to_vec)
        .collect()
}

/// Port of `StringUtils::Find` (StringUtils.h:240-243 @ v2.5.2): `std::string::find`;
/// `None` is `std::string::npos`, and an empty `search` is found at 0.
pub fn find(subject: &[u8], search: &[u8]) -> Option<usize> {
    find_from(subject, search, 0)
}

/// `std::string::find(search, pos)`.
fn find_from(subject: &[u8], search: &[u8], pos: usize) -> Option<usize> {
    if pos > subject.len() {
        return None;
    }
    if search.is_empty() {
        return Some(pos);
    }
    subject[pos..]
        .windows(search.len())
        .position(|w| w == search)
        .map(|i| pos + i)
}

/// Port of `StringUtils::ReverseFind` (StringUtils.h:247-250 @ v2.5.2): `std::string::rfind`;
/// an empty `search` is found at the end.
pub fn reverse_find(subject: &[u8], search: &[u8]) -> Option<usize> {
    if search.is_empty() {
        return Some(subject.len());
    }
    subject.windows(search.len()).rposition(|w| w == search)
}

/// Port of `StringUtils::ReplaceInPlace` (StringUtils.h:253-268 @ v2.5.2): replaces every
/// occurrence, scanning on after each replacement; `false` if nothing changed.
pub fn replace_in_place(subject: &mut Vec<u8>, search: &[u8], replace: &[u8]) -> bool {
    if search.is_empty() {
        return false;
    }
    let mut changed = false;
    let mut pos = 0;
    while let Some(at) = find_from(subject, search, pos) {
        subject.splice(at..at + search.len(), replace.iter().copied());
        pos = at + replace.len();
        changed = true;
    }
    changed
}

/// Port of `StringUtils::Replace` (StringUtils.h:271-276 @ v2.5.2).
pub fn replace(subject: &[u8], search: &[u8], replace: &[u8]) -> Vec<u8> {
    let mut s = subject.to_vec();
    replace_in_place(&mut s, search, replace);
    s
}

/// Port of `StringUtils::Contain` (StringUtils.h:279-287 @ v2.5.2): case-insensitive, whole
/// entries. The C++ compares `c_str()`s, so each side stops at a NUL.
pub fn contain(list: &[Vec<u8>], entry: &[u8]) -> bool {
    list.iter().any(|e| compare(c_str(e), c_str(entry)))
}

/// Port of `StringUtils::Remove` (StringUtils.h:291-305 @ v2.5.2): removes the first
/// case-insensitive match; `true` if one was found.
pub fn remove(list: &mut StringVec, entry: &[u8]) -> bool {
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
