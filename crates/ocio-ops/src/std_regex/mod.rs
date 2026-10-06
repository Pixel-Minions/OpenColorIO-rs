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
mod libstdcxx_match;
pub mod msvc;
mod msvc_match;

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

/// `regex_match(text, re)`: whether the whole text matches. A `const char *` text is the
/// bytes up to its first NUL; the caller cuts it. Matching can throw: MSVC's matcher gives up
/// with `error_stack` or `error_complexity` past its limits, and the port refuses what the
/// Linux wheel's matcher can't do without overflowing its stack (U-54).
///
/// Port of `regex_match(const char *, const regex &)` (MSVC STL `<regex>`:2196-2203); for
/// libstdc++, its behavior (`libstdcxx_match`).
pub fn regex_match(text: &[u8], re: &Regex) -> Result<bool, RegexError> {
    match &re.program {
        Program::Msvc(p) => {
            // MSVC's matcher recurses up to 600 levels (its own limit), with frames that are
            // large at opt-level 0: on a thread of its own, with the stack that needs.
            on_stack(MSVC_MATCH_STACK, || msvc_match::regex_match(p, text))
                .unwrap_or_else(|| Err(msvc::error(ErrorType::Space)))
        }
        Program::Libstdcxx(p) => libstdcxx_match::regex_match(p, text),
    }
}

/// The stack MSVC's matcher needs at its depth limit (600 nested matches): at most 1.43 MiB
/// measured at opt-level 0, where frames are largest (`(a|b|c)*` on 298 `c`, `(?:(?=a|b).)*`
/// on 299 `a`), with a margin.
const MSVC_MATCH_STACK: usize = 8 * 1024 * 1024;

/// Runs `f` on a new thread with a stack of `size` bytes, or `None` when the system can't
/// create the thread. A panic in `f` is resumed on the caller's thread.
fn on_stack<T: Send>(size: usize, f: impl FnOnce() -> T + Send) -> Option<T> {
    std::thread::scope(|scope| {
        let handle = std::thread::Builder::new()
            .stack_size(size)
            .spawn_scoped(scope, f)
            .ok()?;
        Some(
            handle
                .join()
                .unwrap_or_else(|panic| std::panic::resume_unwind(panic)),
        )
    })
}
