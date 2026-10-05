// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! C++'s `std::regex` (ECMAScript grammar, `char`, the "C" locale), as each wheel's C++
//! library implements it (owner decision D2).
//!
//! OCIO 2.5.2 calls `std::regex` in four places: a file rule's regular expression is
//! validated by constructing it (`ValidateRegularExpression`, FileRules.cpp:263-282) and
//! matched with `regex_match` (FileRules.cpp:488-500); a glob rule is converted to an
//! expression first; `SanitizeRegularExpression` (FileRules.cpp:30-49) rewrites it with two
//! fixed `regex_replace` patterns; and `ocio://` URIs are recognized with `regex_search`
//! (Config.cpp:1162, BuiltinConfigRegistry.cpp:39). Each time it constructs
//! `std::regex(pattern)`: the ECMAScript grammar with no other flag. The messages of the
//! `std::regex_error`s it catches reach OCIO's own.
//!
//! The two wheels use different libraries ([`Library`]), which differ in the expressions they
//! accept, in what some of them mean, and in their error texts:
//! - the Windows wheel, Microsoft's STL (`<regex>`, header-only templates, compiled into the
//!   wheel with MSVC 14.44; its `regex_error` is thrown by `_Xregex_error` in `msvcp140.dll`).
//!   It is translated here from the headers of MSVC 14.44.35207 (Apache-2.0 WITH
//!   LLVM-exception, owner decision D3, notice in NOTICE): [`msvc`].
//! - the Linux wheel, GCC 14's libstdc++, which is not translated (it is GPL with the runtime
//!   exception): its behavior is reproduced from the C++ standard ([re.grammar], ECMA-262 3rd
//!   edition) and the Linux wheel, black-box: [`libstdcxx`].
//!
//! An expression is bytes, as C++ `char`s: no Unicode, no locale but the classic one.

pub mod libstdcxx;
pub mod msvc;

/// The C++ library whose `std::regex` the port reproduces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Library {
    /// Microsoft's STL, the Windows wheel's.
    Msvc,
    /// GCC's libstdc++, the Linux wheel's.
    Libstdcxx,
}

/// The kind of a `std::regex_error`.
///
/// Port of `regex_constants::error_type` (MSVC STL `<regex>`:126-142 @ 14.44.35207), the
/// standard's list ([re.err]) plus MSVC's `_Error_parse`, in MSVC's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorType {
    /// `error_collate`: an invalid collating element name.
    Collate,
    /// `error_ctype`: an invalid character class name.
    Ctype,
    /// `error_escape`: an invalid escaped character, or a trailing escape.
    Escape,
    /// `error_backref`: an invalid back reference.
    Backref,
    /// `error_brack`: mismatched `[` and `]`.
    Brack,
    /// `error_paren`: mismatched `(` and `)`.
    Paren,
    /// `error_brace`: mismatched `{` and `}`.
    Brace,
    /// `error_badbrace`: an invalid range in a `{}` expression.
    Badbrace,
    /// `error_range`: an invalid character range.
    Range,
    /// `error_space`: not enough memory to compile the expression.
    Space,
    /// `error_badrepeat`: one of `*?+{` not preceded by a valid expression.
    Badrepeat,
    /// `error_complexity`: a match exceeded the library's complexity limit.
    Complexity,
    /// `error_stack`: not enough memory to determine whether the expression matches.
    Stack,
    /// `error_syntax`: some other error in the expression.
    Syntax,
}

/// A `std::regex_error`: its kind and its `what()`, as the library writes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegexError {
    code: ErrorType,
    what: &'static str,
}

impl RegexError {
    /// `code()`.
    pub fn code(&self) -> ErrorType {
        self.code
    }

    /// `what()`: the library's text, which OCIO copies into its messages.
    pub fn what(&self) -> &'static str {
        self.what
    }
}

impl std::fmt::Display for RegexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.what)
    }
}

impl std::error::Error for RegexError {}

/// A compiled expression: `std::regex(pattern)`.
#[derive(Debug, Clone)]
pub struct Regex {
    program: Program,
}

/// The compiled form of each library.
#[derive(Debug, Clone)]
enum Program {
    Msvc(msvc::Program),
    Libstdcxx(libstdcxx::Program),
}

impl Regex {
    /// `std::regex(first, last)` with the ECMAScript grammar and no other flag, as `library`
    /// compiles it: the expression, or the `regex_error` the constructor throws. A
    /// `const char *` pattern is the bytes up to its first NUL; the caller cuts it.
    pub fn new(pattern: &[u8], library: Library) -> Result<Regex, RegexError> {
        match library {
            Library::Msvc => Ok(Regex {
                program: Program::Msvc(msvc::compile(pattern)?),
            }),
            Library::Libstdcxx => Ok(Regex {
                program: Program::Libstdcxx(libstdcxx::compile(pattern)?),
            }),
        }
    }

    /// The number of capture groups (`mark_count()`).
    pub fn mark_count(&self) -> usize {
        match &self.program {
            Program::Msvc(p) => p.mark_count() - 1,
            Program::Libstdcxx(p) => p.mark_count() - 1,
        }
    }
}
