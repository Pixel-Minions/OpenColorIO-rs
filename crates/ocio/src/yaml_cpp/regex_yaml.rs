// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.8.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! Port of yaml-cpp 0.8.0's simplified regular expressions (src/regex_yaml.h,
//! src/regex_yaml.cpp, src/regeximpl.h) over `StringCharSource` (src/stringsource.h), the
//! only source the emitter matches against.
//!
//! yaml-cpp stores characters as `char`, which is signed on x86-64 with both MSVC and GCC,
//! and compares ranges as signed values; `RegEx` keeps them as `i8` for that reason.

use std::ops::{Add, BitAnd, BitOr, Not};

/// Port of `YAML::REGEX_OP` (regex_yaml.h:13-21).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegexOp {
    Empty,
    Match,
    Range,
    Or,
    And,
    Not,
    Seq,
}

/// Port of `YAML::StringCharSource` (stringsource.h): a position in a byte string. Reading
/// at the end gives the C string's NUL terminator.
#[derive(Debug, Clone, Copy)]
pub struct StringCharSource<'a> {
    s: &'a [u8],
    offset: usize,
}

impl<'a> StringCharSource<'a> {
    pub fn new(s: &'a [u8]) -> StringCharSource<'a> {
        StringCharSource { s, offset: 0 }
    }

    /// `operator bool`: characters remain.
    pub fn is_valid(&self) -> bool {
        self.offset < self.s.len()
    }

    /// `operator[](i)`: `m_str[m_offset + i]`. At the end this is the NUL terminator of
    /// `c_str()`; the matchers never read further.
    pub fn at(&self, i: usize) -> u8 {
        self.s.get(self.offset + i).copied().unwrap_or(0)
    }

    /// `operator+(int)` (stringsource.h:20-27).
    pub fn plus(&self, i: i32) -> StringCharSource<'a> {
        let mut source = *self;
        if self.offset as i64 + i64::from(i) >= 0 {
            source.offset = (self.offset as i64 + i64::from(i)) as usize;
        } else {
            source.offset = 0;
        }
        source
    }

    /// `operator++` (stringsource.h:29-32).
    pub fn advance(&mut self) {
        self.offset += 1;
    }
}

/// Port of `YAML::RegEx` (regex_yaml.h:25-77).
#[derive(Debug, Clone)]
pub struct RegEx {
    op: RegexOp,
    a: i8,
    z: i8,
    params: Vec<RegEx>,
}

impl RegEx {
    fn with_op(op: RegexOp) -> RegEx {
        RegEx {
            op,
            a: 0,
            z: 0,
            params: Vec::new(),
        }
    }

    /// `RegEx()`: matches only the empty string.
    pub fn empty() -> RegEx {
        RegEx::with_op(RegexOp::Empty)
    }

    /// `RegEx(char ch)`.
    pub fn ch(ch: u8) -> RegEx {
        RegEx {
            a: ch as i8,
            ..RegEx::with_op(RegexOp::Match)
        }
    }

    /// `RegEx(char a, char z)`: a signed range.
    pub fn range(a: u8, z: u8) -> RegEx {
        RegEx {
            a: a as i8,
            z: z as i8,
            ..RegEx::with_op(RegexOp::Range)
        }
    }

    /// `RegEx(const std::string &str, REGEX_OP op)`: one `RegEx(char)` per character, joined
    /// by `op` (`Seq` by default in C++).
    pub fn string(s: &[u8], op: RegexOp) -> RegEx {
        RegEx {
            params: s.iter().map(|&c| RegEx::ch(c)).collect(),
            ..RegEx::with_op(op)
        }
    }

    fn pair(op: RegexOp, a: RegEx, b: RegEx) -> RegEx {
        RegEx {
            params: vec![a, b],
            ..RegEx::with_op(op)
        }
    }

    /// `Matches(const std::string &)`.
    pub fn matches_str(&self, s: &[u8]) -> bool {
        self.match_str(s) >= 0
    }

    /// `Match(const std::string &)`: the number of bytes matched at the start, or -1.
    pub fn match_str(&self, s: &[u8]) -> i32 {
        self.match_source(&StringCharSource::new(s))
    }

    /// `Matches(const Source &)`.
    pub fn matches(&self, source: &StringCharSource<'_>) -> bool {
        self.match_source(source) >= 0
    }

    /// `Match(const Source &)` (regeximpl.h:67-70).
    pub fn match_source(&self, source: &StringCharSource<'_>) -> i32 {
        if self.is_valid_source(source) {
            self.match_unchecked(source)
        } else {
            -1
        }
    }

    /// `IsValidSource<StringCharSource>` (regeximpl.h:55-65).
    fn is_valid_source(&self, source: &StringCharSource<'_>) -> bool {
        match self.op {
            RegexOp::Match | RegexOp::Range => source.is_valid(),
            _ => true,
        }
    }

    /// `MatchUnchecked` and the `MatchOp*` operators (regeximpl.h:72-182).
    fn match_unchecked(&self, source: &StringCharSource<'_>) -> i32 {
        match self.op {
            // MatchOpEmpty<StringCharSource>: only the empty string.
            RegexOp::Empty => {
                if source.is_valid() {
                    -1
                } else {
                    0
                }
            }
            RegexOp::Match => {
                if source.at(0) as i8 != self.a {
                    -1
                } else {
                    1
                }
            }
            RegexOp::Range => {
                let c = source.at(0) as i8;
                if self.a > c || self.z < c { -1 } else { 1 }
            }
            RegexOp::Or => {
                for param in &self.params {
                    let n = param.match_unchecked(source);
                    if n >= 0 {
                        return n;
                    }
                }
                -1
            }
            RegexOp::And => {
                // The length of the FIRST entry, if all match.
                let mut first = -1;
                for (i, param) in self.params.iter().enumerate() {
                    let n = param.match_unchecked(source);
                    if n == -1 {
                        return -1;
                    }
                    if i == 0 {
                        first = n;
                    }
                }
                first
            }
            RegexOp::Not => {
                if self.params.is_empty() || self.params[0].match_unchecked(source) >= 0 {
                    -1
                } else {
                    1
                }
            }
            RegexOp::Seq => {
                let mut offset = 0;
                for param in &self.params {
                    // Match, not MatchUnchecked: validity is checked after the offset.
                    let n = param.match_source(&source.plus(offset));
                    if n == -1 {
                        return -1;
                    }
                    offset += n;
                }
                offset
            }
        }
    }
}

/// `operator!(const RegEx &)` (regex_yaml.cpp:15-19).
impl Not for RegEx {
    type Output = RegEx;
    fn not(self) -> RegEx {
        RegEx {
            params: vec![self],
            ..RegEx::with_op(RegexOp::Not)
        }
    }
}

/// `operator|` (regex_yaml.cpp:21-26).
impl BitOr for RegEx {
    type Output = RegEx;
    fn bitor(self, rhs: RegEx) -> RegEx {
        RegEx::pair(RegexOp::Or, self, rhs)
    }
}

/// `operator&` (regex_yaml.cpp:28-33).
impl BitAnd for RegEx {
    type Output = RegEx;
    fn bitand(self, rhs: RegEx) -> RegEx {
        RegEx::pair(RegexOp::And, self, rhs)
    }
}

/// `operator+` (regex_yaml.cpp:35-40).
impl Add for RegEx {
    type Output = RegEx;
    fn add(self, rhs: RegEx) -> RegEx {
        RegEx::pair(RegexOp::Seq, self, rhs)
    }
}

#[cfg(test)]
mod tests {
    //! Port of yaml-cpp 0.8.0 `test/regex_test.cpp`. `MIN_CHAR` is `Stream::eof() + 1`, i.e.
    //! `0x04 + 1`.
    use super::*;

    const MIN_CHAR: u8 = 0x05;

    fn one(c: u8) -> Vec<u8> {
        vec![c]
    }

    /// Port of yaml-cpp 0.8.0 `TEST(RegExTest, Empty)`.
    #[test]
    fn empty() {
        let empty = RegEx::empty();
        assert!(empty.matches_str(b""));
        assert_eq!(0, empty.match_str(b""));
        for i in MIN_CHAR..128 {
            let s = one(i);
            assert!(!empty.matches_str(&s));
            assert_eq!(-1, empty.match_str(&s));
        }
    }

    /// Port of yaml-cpp 0.8.0 `TEST(RegExTest, Range)`.
    #[test]
    fn range() {
        for i in MIN_CHAR..128 {
            for j in MIN_CHAR..128 {
                let ex = RegEx::range(i, j);
                for k in MIN_CHAR..128 {
                    let s = one(k);
                    if i <= k && k <= j {
                        assert!(ex.matches_str(&s));
                        assert_eq!(1, ex.match_str(&s));
                    } else {
                        assert!(!ex.matches_str(&s));
                        assert_eq!(-1, ex.match_str(&s));
                    }
                }
            }
        }
    }

    /// Port of yaml-cpp 0.8.0 `TEST(RegExTest, EmptyString)`.
    #[test]
    fn empty_string() {
        let ex = RegEx::string(b"", RegexOp::Seq);
        assert!(ex.matches_str(b""));
        assert_eq!(0, ex.match_str(b""));

        // Matches anything, unlike RegEx()!
        assert!(ex.matches_str(b"hello"));
        assert_eq!(0, ex.match_str(b"hello"));
    }

    /// Port of yaml-cpp 0.8.0 `TEST(RegExTest, SingleCharacterString)`.
    #[test]
    fn single_character_string() {
        for i in MIN_CHAR..128 {
            let ex = RegEx::string(&one(i), RegexOp::Seq);
            for j in MIN_CHAR..128 {
                let s = one(j);
                if j == i {
                    assert!(ex.matches_str(&s));
                    assert_eq!(1, ex.match_str(&s));
                    // Match at start of string only!
                    let mut prefixed = vec![i + 1];
                    prefixed.extend_from_slice(b"prefix: ");
                    prefixed.extend_from_slice(&s);
                    assert!(!ex.matches_str(&prefixed));
                    assert_eq!(-1, ex.match_str(&prefixed));
                } else {
                    assert!(!ex.matches_str(&s));
                    assert_eq!(-1, ex.match_str(&s));
                }
            }
        }
    }

    /// Port of yaml-cpp 0.8.0 `TEST(RegExTest, MultiCharacterString)`.
    #[test]
    fn multi_character_string() {
        let ex = RegEx::string(b"ab", RegexOp::Seq);

        assert!(!ex.matches_str(b"a"));
        assert_eq!(-1, ex.match_str(b"a"));

        assert!(ex.matches_str(b"ab"));
        assert_eq!(2, ex.match_str(b"ab"));
        assert!(ex.matches_str(b"abba"));
        assert_eq!(2, ex.match_str(b"abba"));

        // match at start of string only!
        assert!(!ex.matches_str(b"baab"));
        assert_eq!(-1, ex.match_str(b"baab"));
    }

    /// Port of yaml-cpp 0.8.0 `TEST(RegExTest, OperatorNot)`.
    #[test]
    fn operator_not() {
        let ex = !RegEx::string(b"ab", RegexOp::Seq);

        assert!(ex.matches_str(b"a"));
        assert_eq!(1, ex.match_str(b"a"));

        assert!(!ex.matches_str(b"ab"));
        assert_eq!(-1, ex.match_str(b"ab"));
        assert!(!ex.matches_str(b"abba"));
        assert_eq!(-1, ex.match_str(b"abba"));

        // match at start of string only!
        assert!(ex.matches_str(b"baab"));
        // Operator not causes only one character to be matched.
        assert_eq!(1, ex.match_str(b"baab"));
    }

    /// Port of yaml-cpp 0.8.0 `TEST(RegExTest, OperatorOr)`.
    #[test]
    fn operator_or() {
        for i in MIN_CHAR..127 {
            for j in i + 1..128 {
                let ex1 =
                    RegEx::string(&one(i), RegexOp::Seq) | RegEx::string(&one(j), RegexOp::Seq);
                let ex2 =
                    RegEx::string(&one(j), RegexOp::Seq) | RegEx::string(&one(i), RegexOp::Seq);
                for k in MIN_CHAR..128 {
                    let s = one(k);
                    if i == k || j == k {
                        assert!(ex1.matches_str(&s));
                        assert!(ex2.matches_str(&s));
                        assert_eq!(1, ex1.match_str(&s));
                        assert_eq!(1, ex2.match_str(&s));
                    } else {
                        assert!(!ex1.matches_str(&s));
                        assert!(!ex2.matches_str(&s));
                        assert_eq!(-1, ex1.match_str(&s));
                        assert_eq!(-1, ex2.match_str(&s));
                    }
                }
            }
        }
    }

    /// Port of yaml-cpp 0.8.0 `TEST(RegExTest, OperatorOrShortCircuits)`.
    #[test]
    fn operator_or_short_circuits() {
        let ex1 = RegEx::string(b"aaaa", RegexOp::Seq) | RegEx::string(b"aa", RegexOp::Seq);
        let ex2 = RegEx::string(b"aa", RegexOp::Seq) | RegEx::string(b"aaaa", RegexOp::Seq);

        assert!(ex1.matches_str(b"aaaaa"));
        assert_eq!(4, ex1.match_str(b"aaaaa"));

        assert!(ex2.matches_str(b"aaaaa"));
        assert_eq!(2, ex2.match_str(b"aaaaa"));
    }

    /// Port of yaml-cpp 0.8.0 `TEST(RegExTest, OperatorAnd)`.
    #[test]
    fn operator_and() {
        let empty_set = RegEx::ch(b'a') & RegEx::empty();
        assert!(!empty_set.matches_str(b"a"));
    }

    /// Port of yaml-cpp 0.8.0 `TEST(RegExTest, OperatorAndShortCircuits)`.
    #[test]
    fn operator_and_short_circuits() {
        let ex1 = RegEx::string(b"aaaa", RegexOp::Seq) & RegEx::string(b"aa", RegexOp::Seq);
        let ex2 = RegEx::string(b"aa", RegexOp::Seq) & RegEx::string(b"aaaa", RegexOp::Seq);

        assert!(ex1.matches_str(b"aaaaa"));
        assert_eq!(4, ex1.match_str(b"aaaaa"));

        assert!(ex2.matches_str(b"aaaaa"));
        assert_eq!(2, ex2.match_str(b"aaaaa"));
    }

    /// Port of yaml-cpp 0.8.0 `TEST(RegExTest, OperatorPlus)`.
    #[test]
    fn operator_plus() {
        let ex = RegEx::string(b"hello ", RegexOp::Seq) + RegEx::string(b"there", RegexOp::Seq);

        assert!(ex.matches_str(b"hello there"));
        assert!(!ex.matches_str(b"hello "));
        assert!(!ex.matches_str(b"there"));
        assert_eq!(11, ex.match_str(b"hello there"));
    }

    /// Port of yaml-cpp 0.8.0 `TEST(RegExTest, StringOr)`.
    #[test]
    fn string_or() {
        let s = b"abcde";
        let ex = RegEx::string(s, RegexOp::Or);

        for i in 0..s.len() {
            assert!(ex.matches_str(&s[i..i + 1]));
            assert_eq!(1, ex.match_str(&s[i..i + 1]));
        }

        assert_eq!(1, ex.match_str(s));
    }
}
