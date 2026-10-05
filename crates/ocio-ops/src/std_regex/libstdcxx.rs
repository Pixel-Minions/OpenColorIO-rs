// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! `std::regex` as the Linux wheel compiles it: GCC 14's libstdc++ (gcc-toolset-14, 14.2.1),
//! for `char` in the classic locale and the ECMAScript grammar with no other flag.
//!
//! libstdc++ is not translated (owner decision D3: it is GPL with the runtime exception). This
//! parser follows the ECMAScript grammar of the C++ standard ([re.grammar], ECMA-262 3rd
//! edition, 15.10) where libstdc++ follows it, and the Linux wheel where it doesn't; each rule
//! that departs from the standard says what the wheel does, as the oracle shows it
//! (`tests/std_regex_libstdcxx_*_oracle.rs`):
//! - a quantifier may follow a quantifier (`a**`, `a{2}{3}`), each applying to what precedes;
//! - `]` and `}` outside brackets and braces are ordinary characters;
//! - `\cX` is `X` itself, for any character `X`;
//! - `\0` is NUL, and the digits after it ordinary characters (`\01` is NUL then `1`);
//! - `\uNNNN` keeps the low byte of its value;
//! - a range compares its ends as signed `char`s (`[\x7f-\x81]` is refused);
//! - `^` and `$` match only at the ends of the text;
//! - a bracket expression's collating elements and equivalence classes take the POSIX names
//!   of the characters of the portable character set (`[[.space.]]`, `[[=tab=]]`).
//!
//! Its errors carry the wheel's texts: the specific ones of its scanner and bracket compiler,
//! and `regex_error` (the text of the system `libstdc++.so`'s `regex_error(code)`) for the
//! others.

use super::{ErrorType, RegexError};

// ---------------------------------------------------------------------------------------------
// Errors

/// The text of a `regex_error` thrown with a code only.
const GENERIC: &str = "regex_error";

const E_BRACKET: &str = "Unexpected character within '[...]' in regular expression";
const E_INCOMPLETE_BRACKET: &str = "Incomplete '[[' character class in regular expression";
const E_RANGE_START: &str = "Invalid start of '[x-x]' range in regular expression";
const E_RANGE_END: &str = "Invalid end of '[x-x]' range in regular expression";
const E_RANGE: &str = "Invalid range in bracket expression.";
const E_EQUIV: &str = "Invalid equivalence class.";
const E_COLLATE: &str = "Invalid collate element.";
const E_CTYPE: &str = "Invalid character class.";
const E_BACKREF_COUNT: &str = "Back-reference index exceeds current sub-expression count.";
const E_BACKREF_OPEN: &str = "Back-reference referred to an opened sub-expression.";
const E_ASSERTION: &str = "Invalid '(?...)' zero-width assertion in regular expression";
const E_ESCAPE_END: &str = "Invalid escape at end of regular expression";
const E_CONTROL: &str = "invalid '\\cX' control character in regular expression";
const E_HEX: &str = "Invalid '\\xNN' control character in regular expression";
const E_UNICODE: &str = "Invalid '\\uNNNN' control character in regular expression";
const E_INTEGER: &str = "invalid back reference";
/// `error_space` past `_GLIBCXX_REGEX_STATE_LIMIT` NFA states.
pub(super) const E_STATES: &str = "Number of NFA states exceeds limit. Please use shorter \
                                   regex string, or use smaller brace expression, or make \
                                   _GLIBCXX_REGEX_STATE_LIMIT larger.";

fn error(code: ErrorType, what: &'static str) -> RegexError {
    RegexError { code, what }
}

type ParseResult<T> = Result<T, RegexError>;

// ---------------------------------------------------------------------------------------------
// The classic locale

/// A character class of the classic locale: the bytes it holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Class {
    Alnum,
    Alpha,
    Blank,
    Cntrl,
    Digit,
    Graph,
    Lower,
    Print,
    Punct,
    Space,
    Upper,
    Xdigit,
    /// `w`: alphanumeric or `_`.
    Word,
}

impl Class {
    /// Whether byte `c` is in the class, in glibc's "C" locale: ASCII only.
    pub(super) fn contains(self, c: u8) -> bool {
        match self {
            Class::Alnum => c.is_ascii_alphanumeric(),
            Class::Alpha => c.is_ascii_alphabetic(),
            Class::Blank => c == b' ' || c == b'\t',
            Class::Cntrl => c.is_ascii_control(),
            Class::Digit => c.is_ascii_digit(),
            Class::Graph => c.is_ascii_graphic(),
            Class::Lower => c.is_ascii_lowercase(),
            Class::Print => c.is_ascii_graphic() || c == b' ',
            Class::Punct => c.is_ascii_punctuation(),
            Class::Space => matches!(c, b' ' | 0x09..=0x0D),
            Class::Upper => c.is_ascii_uppercase(),
            Class::Xdigit => c.is_ascii_hexdigit(),
            Class::Word => c.is_ascii_alphanumeric() || c == b'_',
        }
    }

    /// The class a `[:name:]` names, ignoring case; `d`, `s` and `w` too (in either case, so
    /// `[:W:]` is `[:w:]`).
    fn lookup(name: &[u8]) -> Option<Class> {
        let lower: Vec<u8> = name.iter().map(u8::to_ascii_lowercase).collect();
        Some(match lower.as_slice() {
            b"alnum" => Class::Alnum,
            b"alpha" => Class::Alpha,
            b"blank" => Class::Blank,
            b"cntrl" => Class::Cntrl,
            b"digit" | b"d" => Class::Digit,
            b"graph" => Class::Graph,
            b"lower" => Class::Lower,
            b"print" => Class::Print,
            b"punct" => Class::Punct,
            b"space" | b"s" => Class::Space,
            b"upper" => Class::Upper,
            b"xdigit" => Class::Xdigit,
            b"w" => Class::Word,
            _ => return None,
        })
    }
}

/// The POSIX names of the portable character set that a collating element or an equivalence
/// class may give (`[[.space.]]`), by code: the names the Linux wheel accepts.
const COLLATE_NAMES: [&[u8]; 128] = [
    b"NUL",
    b"SOH",
    b"STX",
    b"ETX",
    b"EOT",
    b"ENQ",
    b"ACK",
    b"alert",
    b"backspace",
    b"tab",
    b"newline",
    b"vertical-tab",
    b"form-feed",
    b"carriage-return",
    b"SO",
    b"SI",
    b"DLE",
    b"DC1",
    b"DC2",
    b"DC3",
    b"DC4",
    b"NAK",
    b"SYN",
    b"ETB",
    b"CAN",
    b"EM",
    b"SUB",
    b"ESC",
    b"IS4",
    b"IS3",
    b"IS2",
    b"IS1",
    b"space",
    b"exclamation-mark",
    b"quotation-mark",
    b"number-sign",
    b"dollar-sign",
    b"percent-sign",
    b"ampersand",
    b"apostrophe",
    b"left-parenthesis",
    b"right-parenthesis",
    b"asterisk",
    b"plus-sign",
    b"comma",
    b"hyphen",
    b"period",
    b"slash",
    b"zero",
    b"one",
    b"two",
    b"three",
    b"four",
    b"five",
    b"six",
    b"seven",
    b"eight",
    b"nine",
    b"colon",
    b"semicolon",
    b"less-than-sign",
    b"equals-sign",
    b"greater-than-sign",
    b"question-mark",
    b"commercial-at",
    b"A",
    b"B",
    b"C",
    b"D",
    b"E",
    b"F",
    b"G",
    b"H",
    b"I",
    b"J",
    b"K",
    b"L",
    b"M",
    b"N",
    b"O",
    b"P",
    b"Q",
    b"R",
    b"S",
    b"T",
    b"U",
    b"V",
    b"W",
    b"X",
    b"Y",
    b"Z",
    b"left-square-bracket",
    b"backslash",
    b"right-square-bracket",
    b"circumflex",
    b"underscore",
    b"grave-accent",
    b"a",
    b"b",
    b"c",
    b"d",
    b"e",
    b"f",
    b"g",
    b"h",
    b"i",
    b"j",
    b"k",
    b"l",
    b"m",
    b"n",
    b"o",
    b"p",
    b"q",
    b"r",
    b"s",
    b"t",
    b"u",
    b"v",
    b"w",
    b"x",
    b"y",
    b"z",
    b"left-curly-bracket",
    b"vertical-line",
    b"right-curly-bracket",
    b"tilde",
    b"DEL",
];

/// The character a collating element names: one of the POSIX names (a letter names itself).
fn lookup_collate(name: &[u8]) -> Option<u8> {
    COLLATE_NAMES
        .iter()
        .position(|n| *n == name)
        .map(|i| i as u8)
}

// ---------------------------------------------------------------------------------------------
// The expression

/// A set of bytes: a bracket expression, a class escape, or `.`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ByteSet {
    bits: [u64; 4],
}

impl ByteSet {
    fn new() -> ByteSet {
        ByteSet { bits: [0; 4] }
    }

    fn insert(&mut self, c: u8) {
        self.bits[usize::from(c >> 6)] |= 1 << (c & 63);
    }

    pub(super) fn contains(&self, c: u8) -> bool {
        self.bits[usize::from(c >> 6)] & (1 << (c & 63)) != 0
    }

    fn insert_class(&mut self, class: Class, negated: bool) {
        for c in 0..=255u8 {
            if class.contains(c) != negated {
                self.insert(c);
            }
        }
    }

    fn negate(&mut self) {
        for b in &mut self.bits {
            *b = !*b;
        }
    }
}

/// A node of the parsed expression.
#[derive(Debug, Clone)]
pub(super) enum Node {
    /// Nothing: an empty alternative.
    Empty,
    /// A byte.
    Char(u8),
    /// A set of bytes (`.`, a class escape, a bracket expression).
    Set(Box<ByteSet>),
    /// `^`: the start of the text.
    LineBegin,
    /// `$`: the end of the text.
    LineEnd,
    /// `\b` (`negated`: `\B`).
    WordBoundary { negated: bool },
    /// `\n`: what capture group `n` matched.
    Backref(usize),
    /// A capture group (`index`), or a non-capture one (`None`).
    Group {
        index: Option<usize>,
        body: Box<Node>,
    },
    /// `(?=...)` or `(?!...)`.
    Lookahead { negated: bool, body: Box<Node> },
    /// A repetition: `min` to `max` (`None`: unbounded) times.
    Repeat {
        body: Box<Node>,
        min: u64,
        max: Option<u64>,
        greedy: bool,
        /// How it was written, which decides the NFA states it takes.
        form: RepeatForm,
    },
    /// Terms in sequence.
    Concat(Vec<Node>),
    /// Alternatives, the first preferred.
    Alternation(Vec<Node>),
}

/// How a repetition was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RepeatForm {
    /// `*`.
    Star,
    /// `+`.
    Plus,
    /// `?`.
    Question,
    /// `{n}`, `{n,}` or `{n,m}`.
    Brace,
}

/// A compiled expression: the tree, and the number of capture groups (group 0 included).
#[derive(Debug, Clone)]
pub struct Program {
    pub(super) root: Node,
    pub(super) groups: usize,
}

impl Program {
    pub(super) fn mark_count(&self) -> usize {
        self.groups
    }
}

// ---------------------------------------------------------------------------------------------
// The scanner

/// A token of the expression outside brackets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token {
    Eof,
    Char(u8),
    Any,
    LineBegin,
    LineEnd,
    WordBoundary {
        negated: bool,
    },
    /// `\d`, `\D`, `\s`, `\S`, `\w`, `\W`.
    ClassEscape(u8),
    Backref(usize),
    /// `*`, `+`, `?`.
    Star,
    Plus,
    Question,
    /// `{`.
    BraceOpen,
    /// `(`, `(?:`, `(?=`, `(?!`.
    GroupOpen,
    NonCaptureOpen,
    LookaheadOpen {
        negated: bool,
    },
    GroupClose,
    Or,
    /// `[` or `[^`.
    BracketOpen {
        negated: bool,
    },
}

/// A token inside brackets.
#[derive(Debug, Clone, PartialEq, Eq)]
enum BracketToken {
    Eof,
    Char(u8),
    Dash,
    Close,
    /// `[:name:]`.
    Class(Vec<u8>),
    /// `[=name=]`.
    Equiv(Vec<u8>),
    /// `[.name.]`.
    Collate(Vec<u8>),
    /// `\d` and the others.
    ClassEscape(u8),
    /// An escape a bracket expression can't hold (a back reference, `\B`).
    Invalid,
}

fn hex_value(c: u8) -> Option<u32> {
    (c as char).to_digit(16)
}

/// The parser: a recursive descent over the scanner's tokens, one token ahead.
struct Parser<'a> {
    pat: &'a [u8],
    pos: usize,
    /// The token ahead.
    token: Token,
    /// The capture groups opened so far.
    groups_opened: usize,
    /// The capture groups not yet closed.
    open_groups: Vec<usize>,
    /// The NFA states inserted so far.
    states: u64,
    /// The terms parsed so far.
    terms: u64,
    /// The groups open around the current term.
    nesting: u64,
}

impl<'a> Parser<'a> {
    fn new(pat: &'a [u8]) -> ParseResult<Parser<'a>> {
        let mut parser = Parser {
            pat,
            pos: 0,
            token: Token::Eof,
            groups_opened: 0,
            open_groups: Vec::new(),
            states: 0,
            terms: 0,
            nesting: 0,
        };
        parser.advance()?;
        Ok(parser)
    }

    /// Inserts `n` NFA states; past the limit, `error_space`.
    fn add_states(&mut self, n: u64) -> ParseResult<()> {
        self.states = self.states.saturating_add(n);
        if self.states > STATE_LIMIT {
            return Err(error(ErrorType::Space, E_STATES));
        }
        Ok(())
    }

    fn peek_byte(&self) -> Option<u8> {
        self.pat.get(self.pos).copied()
    }

    fn take_byte(&mut self) -> Option<u8> {
        let c = self.peek_byte()?;
        self.pos += 1;
        Some(c)
    }

    /// Scans the next token outside brackets.
    fn advance(&mut self) -> ParseResult<()> {
        self.token = self.scan()?;
        Ok(())
    }

    fn scan(&mut self) -> ParseResult<Token> {
        let Some(c) = self.take_byte() else {
            return Ok(Token::Eof);
        };
        Ok(match c {
            b'\\' => self.scan_escape()?,
            b'(' => {
                if self.peek_byte() != Some(b'?') {
                    Token::GroupOpen
                } else {
                    self.pos += 1;
                    match self.take_byte() {
                        Some(b':') => Token::NonCaptureOpen,
                        Some(b'=') => Token::LookaheadOpen { negated: false },
                        Some(b'!') => Token::LookaheadOpen { negated: true },
                        None => return Err(error(ErrorType::Paren, GENERIC)),
                        Some(_) => return Err(error(ErrorType::Paren, E_ASSERTION)),
                    }
                }
            }
            b')' => Token::GroupClose,
            b'[' => {
                if self.peek_byte() == Some(b'^') {
                    self.pos += 1;
                    Token::BracketOpen { negated: true }
                } else {
                    Token::BracketOpen { negated: false }
                }
            }
            b'{' => Token::BraceOpen,
            b'|' => Token::Or,
            b'^' => Token::LineBegin,
            b'$' => Token::LineEnd,
            b'.' => Token::Any,
            b'*' => Token::Star,
            b'+' => Token::Plus,
            b'?' => Token::Question,
            _ => Token::Char(c),
        })
    }

    /// The value of the escapes that give a character, after the `\` and the letter `c`, or
    /// `None` when `c` starts none of them.
    fn scan_char_escape(&mut self, c: u8) -> ParseResult<Option<u8>> {
        Ok(Some(match c {
            b'f' => 0x0C,
            b'n' => b'\n',
            b'r' => b'\r',
            b't' => b'\t',
            b'v' => 0x0B,
            b'c' => match self.take_byte() {
                Some(x) => x,
                None => return Err(error(ErrorType::Escape, E_CONTROL)),
            },
            b'x' => self.scan_hex(2, E_HEX)?,
            b'u' => self.scan_hex(4, E_UNICODE)?,
            _ => return Ok(None),
        }))
    }

    /// `count` hexadecimal digits; the low byte of their value.
    fn scan_hex(&mut self, count: usize, what: &'static str) -> ParseResult<u8> {
        let mut value = 0u32;
        for _ in 0..count {
            match self.take_byte().and_then(hex_value) {
                Some(d) => value = value * 16 + d,
                None => return Err(error(ErrorType::Escape, what)),
            }
        }
        Ok(value as u8)
    }

    /// A decimal number from its first digit `first`: the back reference's index, or a
    /// brace's count. A value past `int` is refused.
    fn scan_integer(&mut self, first: u8) -> ParseResult<u64> {
        let mut value = u64::from(first - b'0');
        while let Some(c @ b'0'..=b'9') = self.peek_byte() {
            self.pos += 1;
            value = value * 10 + u64::from(c - b'0');
            if value > i32::MAX as u64 {
                // Keep consuming the digits; the value is refused.
                while let Some(b'0'..=b'9') = self.peek_byte() {
                    self.pos += 1;
                }
                return Err(error(ErrorType::Backref, E_INTEGER));
            }
        }
        Ok(value)
    }

    /// An escape outside brackets, after its `\`.
    fn scan_escape(&mut self) -> ParseResult<Token> {
        let Some(c) = self.take_byte() else {
            return Err(error(ErrorType::Escape, E_ESCAPE_END));
        };
        if let Some(v) = self.scan_char_escape(c)? {
            return Ok(Token::Char(v));
        }
        Ok(match c {
            b'b' => Token::WordBoundary { negated: false },
            b'B' => Token::WordBoundary { negated: true },
            b'd' | b'D' | b's' | b'S' | b'w' | b'W' => Token::ClassEscape(c),
            b'0' => Token::Char(0),
            b'1'..=b'9' => Token::Backref(self.scan_integer(c)? as usize),
            _ => Token::Char(c),
        })
    }

    /// The next token inside brackets.
    fn scan_bracket(&mut self) -> ParseResult<BracketToken> {
        let Some(c) = self.take_byte() else {
            return Ok(BracketToken::Eof);
        };
        Ok(match c {
            b']' => BracketToken::Close,
            b'-' => BracketToken::Dash,
            b'[' => match self.peek_byte() {
                None => return Err(error(ErrorType::Brack, E_INCOMPLETE_BRACKET)),
                Some(d @ (b':' | b'=' | b'.')) => {
                    self.pos += 1;
                    let name = self.scan_bracket_name(d)?;
                    match d {
                        b':' => BracketToken::Class(name),
                        b'=' => BracketToken::Equiv(name),
                        _ => BracketToken::Collate(name),
                    }
                }
                Some(_) => BracketToken::Char(b'['),
            },
            b'\\' => {
                let Some(e) = self.take_byte() else {
                    return Err(error(ErrorType::Escape, GENERIC));
                };
                if let Some(v) = self.scan_char_escape(e)? {
                    return Ok(BracketToken::Char(v));
                }
                match e {
                    b'b' => BracketToken::Char(0x08),
                    b'd' | b'D' | b's' | b'S' | b'w' | b'W' => BracketToken::ClassEscape(e),
                    b'0' => BracketToken::Char(0),
                    b'1'..=b'9' | b'B' => BracketToken::Invalid,
                    _ => BracketToken::Char(e),
                }
            }
            _ => BracketToken::Char(c),
        })
    }

    /// The name of `[:name:]`, `[=name=]` or `[.name.]` after its opening: up to the first
    /// `d`, which a `]` must follow.
    fn scan_bracket_name(&mut self, d: u8) -> ParseResult<Vec<u8>> {
        let start = self.pos;
        loop {
            match self.take_byte() {
                None => return Err(error(ErrorType::Brack, GENERIC)),
                Some(c) if c == d => {
                    let name = self.pat[start..self.pos - 1].to_vec();
                    if self.take_byte() != Some(b']') {
                        return Err(error(ErrorType::Brack, GENERIC));
                    }
                    return Ok(name);
                }
                Some(_) => {}
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // The grammar

    /// A group's disjunction, one level of nesting deeper.
    fn nested_disjunction(&mut self) -> ParseResult<Node> {
        self.nesting += 1;
        if self.nesting > MAX_NESTING {
            // The wheel's compiler recurses per group (U-54).
            return Err(error(ErrorType::Stack, GENERIC));
        }
        let body = self.disjunction()?;
        self.nesting -= 1;
        Ok(body)
    }

    /// Disjunction: alternatives separated by `|`.
    fn disjunction(&mut self) -> ParseResult<Node> {
        let mut alternatives = vec![self.alternative()?];
        while self.token == Token::Or {
            self.advance()?;
            alternatives.push(self.alternative()?);
        }
        self.add_states(OR_STATES * (alternatives.len() as u64 - 1))?;
        Ok(if alternatives.len() == 1 {
            alternatives.pop().expect("an alternative")
        } else {
            Node::Alternation(alternatives)
        })
    }

    /// Alternative: terms up to `|`, `)` or the end.
    fn alternative(&mut self) -> ParseResult<Node> {
        let mut terms = Vec::new();
        while let Some(term) = self.term()? {
            terms.push(term);
        }
        self.add_states(ALTERNATIVE_END_STATES)?;
        Ok(match terms.len() {
            0 => Node::Empty,
            1 => terms.pop().expect("a term"),
            _ => Node::Concat(terms),
        })
    }

    /// Term: an assertion, or an atom and its quantifiers; `None` at the end of an
    /// alternative.
    fn term(&mut self) -> ParseResult<Option<Node>> {
        if !matches!(self.token, Token::Eof | Token::Or | Token::GroupClose) {
            self.terms += 1;
            if self.terms > MAX_TERMS {
                // The wheel's compiler recurses per term (U-54).
                return Err(error(ErrorType::Space, E_STATES));
            }
        }
        let node = match self.token {
            Token::Eof | Token::Or | Token::GroupClose => return Ok(None),
            Token::LineBegin => {
                self.advance()?;
                self.add_states(1)?;
                return self.no_quantifier(Node::LineBegin);
            }
            Token::LineEnd => {
                self.advance()?;
                self.add_states(1)?;
                return self.no_quantifier(Node::LineEnd);
            }
            Token::WordBoundary { negated } => {
                self.advance()?;
                self.add_states(1)?;
                return self.no_quantifier(Node::WordBoundary { negated });
            }
            Token::LookaheadOpen { negated } => {
                self.advance()?;
                let body = self.nested_disjunction()?;
                self.expect_close()?;
                self.add_states(LOOKAHEAD_CLOSE_STATES)?;
                return self.no_quantifier(Node::Lookahead {
                    negated,
                    body: Box::new(body),
                });
            }
            Token::Star | Token::Plus | Token::Question | Token::BraceOpen => {
                return Err(error(ErrorType::Badrepeat, GENERIC));
            }
            Token::Char(c) => {
                self.advance()?;
                self.add_states(1)?;
                Node::Char(c)
            }
            Token::Any => {
                self.advance()?;
                self.add_states(1)?;
                let mut set = ByteSet::new();
                set.negate();
                for c in *b"\n\r" {
                    remove(&mut set, c);
                }
                Node::Set(Box::new(set))
            }
            Token::ClassEscape(c) => {
                self.advance()?;
                self.add_states(1)?;
                Node::Set(Box::new(class_escape_set(c)))
            }
            Token::Backref(n) => {
                if n > self.groups_opened {
                    return Err(error(ErrorType::Backref, E_BACKREF_COUNT));
                }
                if self.open_groups.contains(&n) {
                    return Err(error(ErrorType::Backref, E_BACKREF_OPEN));
                }
                self.advance()?;
                self.add_states(1)?;
                Node::Backref(n)
            }
            Token::GroupOpen => {
                self.groups_opened += 1;
                let index = self.groups_opened;
                self.open_groups.push(index);
                self.advance()?;
                self.add_states(CAPTURE_OPEN_STATES)?;
                let body = self.nested_disjunction()?;
                self.expect_close()?;
                self.add_states(CAPTURE_CLOSE_STATES)?;
                self.open_groups.pop();
                Node::Group {
                    index: Some(index),
                    body: Box::new(body),
                }
            }
            Token::NonCaptureOpen => {
                self.advance()?;
                self.add_states(NON_CAPTURE_OPEN_STATES)?;
                let body = self.nested_disjunction()?;
                self.expect_close()?;
                self.add_states(NON_CAPTURE_CLOSE_STATES)?;
                Node::Group {
                    index: None,
                    body: Box::new(body),
                }
            }
            Token::BracketOpen { negated } => {
                let set = self.bracket(negated)?;
                self.advance()?;
                self.add_states(1)?;
                Node::Set(Box::new(set))
            }
        };
        self.quantifiers(node).map(Some)
    }

    /// An assertion, which takes no quantifier.
    fn no_quantifier(&mut self, node: Node) -> ParseResult<Option<Node>> {
        if matches!(
            self.token,
            Token::Star | Token::Plus | Token::Question | Token::BraceOpen
        ) {
            return Err(error(ErrorType::Badrepeat, GENERIC));
        }
        Ok(Some(node))
    }

    /// The `)` of a group.
    fn expect_close(&mut self) -> ParseResult<()> {
        if self.token != Token::GroupClose {
            return Err(error(ErrorType::Paren, GENERIC));
        }
        self.advance()
    }

    /// The quantifiers after an atom, each applying to all that precedes it.
    fn quantifiers(&mut self, mut node: Node) -> ParseResult<Node> {
        loop {
            let (min, max, form) = match self.token {
                Token::Star => {
                    self.advance()?;
                    (0, None, RepeatForm::Star)
                }
                Token::Plus => {
                    self.advance()?;
                    (1, None, RepeatForm::Plus)
                }
                Token::Question => {
                    self.advance()?;
                    (0, Some(1), RepeatForm::Question)
                }
                Token::BraceOpen => {
                    let (min, max) = self.brace()?;
                    if let Some(max) = max
                        && max < min
                    {
                        // The wheel copies the body `min` times before it checks the range.
                        self.add_states(min.saturating_mul(states(&node).1))?;
                        return Err(error(ErrorType::Badbrace, GENERIC));
                    }
                    (min, max, RepeatForm::Brace)
                }
                _ => return Ok(node),
            };
            let greedy = if self.token == Token::Question {
                self.advance()?;
                false
            } else {
                true
            };
            node = Node::Repeat {
                body: Box::new(node),
                min,
                max,
                greedy,
                form,
            };
            let (total, _) = states(&node);
            let (body_total, _) = match &node {
                Node::Repeat { body, .. } => states(body),
                _ => unreachable!("a repetition"),
            };
            self.add_states(total - body_total)?;
        }
    }

    /// `{n}`, `{n,}` or `{n,m}` after the `{`; the token after the `}` is scanned.
    fn brace(&mut self) -> ParseResult<(u64, Option<u64>)> {
        // Once a count is read, the wheel inserts an NFA state before it refuses a brace
        // that doesn't close, except for a character other than `,` and `}` right after the
        // first count.
        let min = match self.take_byte() {
            Some(c @ b'0'..=b'9') => self.scan_integer(c)?,
            _ => return Err(error(ErrorType::Badbrace, GENERIC)),
        };
        let max = match self.take_byte() {
            Some(b'}') => Some(min),
            Some(b',') => match self.take_byte() {
                Some(b'}') => None,
                Some(c @ b'0'..=b'9') => {
                    let max = self.scan_integer(c)?;
                    if self.take_byte() != Some(b'}') {
                        return self.unclosed_brace();
                    }
                    Some(max)
                }
                _ => return self.unclosed_brace(),
            },
            None => return self.unclosed_brace(),
            Some(_) => return Err(error(ErrorType::Badbrace, GENERIC)),
        };
        self.advance()?;
        Ok((min, max))
    }

    /// The error of a brace that doesn't close after its count.
    fn unclosed_brace<T>(&mut self) -> ParseResult<T> {
        self.add_states(1)?;
        Err(error(ErrorType::Badbrace, GENERIC))
    }

    /// A bracket expression after its `[` (and `^`), up to its `]`.
    fn bracket(&mut self, negated: bool) -> ParseResult<ByteSet> {
        /// The item before a `-`.
        enum Last {
            None,
            Char(u8),
            Set,
        }
        let mut set = ByteSet::new();
        let mut last = Last::None;
        loop {
            match self.scan_bracket()? {
                BracketToken::Close => break,
                BracketToken::Eof | BracketToken::Invalid => {
                    return Err(error(ErrorType::Brack, E_BRACKET));
                }
                BracketToken::Dash => match last {
                    Last::None => {
                        set.insert(b'-');
                        last = Last::Char(b'-');
                    }
                    Last::Set => {
                        if self.peek_byte() == Some(b']') {
                            set.insert(b'-');
                            last = Last::None;
                        } else {
                            return Err(error(ErrorType::Range, E_RANGE_START));
                        }
                    }
                    Last::Char(start) => {
                        let end = match self.scan_bracket()? {
                            BracketToken::Close => {
                                set.insert(b'-');
                                break;
                            }
                            BracketToken::Char(c) => c,
                            BracketToken::Dash => b'-',
                            _ => return Err(error(ErrorType::Range, E_RANGE_END)),
                        };
                        if (end as i8) < (start as i8) {
                            return Err(error(ErrorType::Range, E_RANGE));
                        }
                        for c in (start as i8)..=(end as i8) {
                            set.insert(c as u8);
                        }
                        last = Last::None;
                    }
                },
                BracketToken::Char(c) => {
                    set.insert(c);
                    last = Last::Char(c);
                }
                BracketToken::Collate(name) => {
                    let Some(c) = lookup_collate(&name) else {
                        return Err(error(ErrorType::Collate, E_COLLATE));
                    };
                    set.insert(c);
                    last = Last::Char(c);
                }
                BracketToken::Equiv(name) => {
                    let Some(c) = lookup_collate(&name) else {
                        return Err(error(ErrorType::Collate, E_EQUIV));
                    };
                    let key = c.to_ascii_lowercase();
                    for x in 0..=255u8 {
                        if x.to_ascii_lowercase() == key {
                            set.insert(x);
                        }
                    }
                    last = Last::Set;
                }
                BracketToken::Class(name) => {
                    let Some(class) = Class::lookup(&name) else {
                        return Err(error(ErrorType::Ctype, E_CTYPE));
                    };
                    set.insert_class(class, false);
                    last = Last::Set;
                }
                BracketToken::ClassEscape(c) => {
                    let escape = class_escape_set(c);
                    for x in 0..=255u8 {
                        if escape.contains(x) {
                            set.insert(x);
                        }
                    }
                    last = Last::Set;
                }
            }
        }
        if negated {
            set.negate();
        }
        Ok(set)
    }
}

fn remove(set: &mut ByteSet, c: u8) {
    set.bits[usize::from(c >> 6)] &= !(1 << (c & 63));
}

/// The bytes of `\d`, `\s`, `\w` and their negations (`\D`...).
fn class_escape_set(c: u8) -> ByteSet {
    let class = match c.to_ascii_lowercase() {
        b'd' => Class::Digit,
        b's' => Class::Space,
        _ => Class::Word,
    };
    let mut set = ByteSet::new();
    set.insert_class(class, c.is_ascii_uppercase());
    set
}

/// The most terms an expression may hold: the wheel's compiler recurses once per term of an
/// alternative, and its stack overflows (the process ends) at about 75,000 (U-54). Past it,
/// the port refuses the expression with the state limit's error.
const MAX_TERMS: u64 = 50_000;

/// The deepest nesting of groups: the wheel's compiler recurses once per group, and its stack
/// overflows at about 14,100 to 14,500 levels (U-54). Past it, the port refuses the expression
/// with `error_stack` (as `regex_error`, the text of a code without a message).
const MAX_NESTING: u64 = 5_000;

/// `_GLIBCXX_REGEX_STATE_LIMIT`: the most NFA states an expression may take.
const STATE_LIMIT: u64 = 100_000;

/// The NFA states inserted before the expression, and after it.
const START_STATES: u64 = 1;
const END_STATES: u64 = 2;
/// The NFA state that ends each alternative.
const ALTERNATIVE_END_STATES: u64 = 1;
/// The NFA states each `|` adds, once its disjunction is complete.
const OR_STATES: u64 = 2;
/// The NFA states a group inserts at its opening and at its closing.
const CAPTURE_OPEN_STATES: u64 = 1;
const CAPTURE_CLOSE_STATES: u64 = 1;
const NON_CAPTURE_OPEN_STATES: u64 = 1;
const NON_CAPTURE_CLOSE_STATES: u64 = 0;
const LOOKAHEAD_CLOSE_STATES: u64 = 2;

/// The NFA states `node` takes, and those of them a copy of it takes (a brace leaves its
/// original body unused, and copies only what is used), in the Linux wheel: an empirical
/// model, calibrated against the wheel's state limit (`tests/std_regex_libstdcxx_*`).
fn states(node: &Node) -> (u64, u64) {
    let both = |n: u64| (n, n);
    match node {
        Node::Empty => both(0),
        Node::Char(_)
        | Node::Set(_)
        | Node::LineBegin
        | Node::LineEnd
        | Node::WordBoundary { .. }
        | Node::Backref(_) => both(1),
        Node::Concat(nodes) => nodes.iter().map(states).fold((0, 0), |(t, r), (nt, nr)| {
            (t.saturating_add(nt), r.saturating_add(nr))
        }),
        // As a disjunction's body, less the end of its last alternative, which the
        // disjunction counts (`disjunction_states`).
        Node::Alternation(nodes) => {
            let n = nodes.len() as u64;
            let extra = (n - 1) * ALTERNATIVE_END_STATES + (n - 1) * OR_STATES;
            nodes
                .iter()
                .map(states)
                .fold((extra, extra), |(t, r), (nt, nr)| {
                    (t.saturating_add(nt), r.saturating_add(nr))
                })
        }
        Node::Group { index, body } => {
            let (t, r) = disjunction_states(body);
            let extra = if index.is_some() {
                CAPTURE_OPEN_STATES + CAPTURE_CLOSE_STATES
            } else {
                NON_CAPTURE_OPEN_STATES + NON_CAPTURE_CLOSE_STATES
            };
            (t.saturating_add(extra), r.saturating_add(extra))
        }
        Node::Lookahead { body, .. } => {
            let (t, r) = disjunction_states(body);
            (
                t.saturating_add(LOOKAHEAD_CLOSE_STATES),
                r.saturating_add(LOOKAHEAD_CLOSE_STATES),
            )
        }
        Node::Repeat {
            body,
            min,
            max,
            form,
            ..
        } => {
            let (t, r) = states(body);
            match form {
                RepeatForm::Star | RepeatForm::Plus => (t + 1, r + 1),
                RepeatForm::Question => (t + 2, r + 2),
                RepeatForm::Brace => {
                    // The original body is left unused: `n` copies, then a copy for the
                    // loop or `m - n` optional copies, and two more states.
                    let copies = match max {
                        None => min.saturating_add(1).saturating_mul(r),
                        Some(max) => min
                            .saturating_mul(r)
                            .saturating_add((max - min).saturating_mul(r.saturating_add(1))),
                    };
                    let used = copies.saturating_add(2);
                    (t.saturating_add(used), used)
                }
            }
        }
    }
}

/// The NFA states of a disjunction whose alternatives `body` holds.
fn disjunction_states(body: &Node) -> (u64, u64) {
    let (t, r) = states(body);
    (
        t.saturating_add(ALTERNATIVE_END_STATES),
        r.saturating_add(ALTERNATIVE_END_STATES),
    )
}

/// Up to this many `(` the compilation runs on the caller's thread.
const INLINE_GROUPS: usize = 64;

/// The stack a compilation needs per level of nesting, with a margin.
const STACK_PER_LEVEL: usize = 8192;

/// `std::regex(pattern)` as the Linux wheel compiles it. An expression with many `(`
/// compiles on a thread with the stack its nesting may need.
pub(super) fn compile(pattern: &[u8]) -> Result<Program, RegexError> {
    let groups = pattern.iter().filter(|&&c| c == b'(').count();
    if groups <= INLINE_GROUPS {
        return compile_here(pattern);
    }
    let levels = groups.min(MAX_NESTING as usize + 1);
    let stack = 256 * 1024 + levels * STACK_PER_LEVEL;
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(stack)
            .spawn_scoped(scope, || compile_here(pattern))
            .expect("a thread to compile the expression")
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
    })
}

fn compile_here(pattern: &[u8]) -> Result<Program, RegexError> {
    let mut parser = Parser::new(pattern)?;
    parser.add_states(START_STATES)?;
    let root = parser.disjunction()?;
    if parser.token != Token::Eof {
        // A `)` that closes nothing.
        return Err(error(ErrorType::Paren, GENERIC));
    }
    parser.add_states(END_STATES)?;
    debug_assert_eq!(
        parser.states,
        START_STATES + END_STATES + disjunction_states(&root).0
    );
    Ok(Program {
        root: Node::Group {
            index: Some(0),
            body: Box::new(root),
        },
        groups: parser.groups_opened + 1,
    })
}
