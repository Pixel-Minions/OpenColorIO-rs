// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.8.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! Port of `YAML::ScanScalar` (src/scanscalar.h, src/scanscalar.cpp, yaml-cpp 0.8.0): the
//! scanning of plain, quoted, literal and folded scalars, with their line folding, escapes,
//! indentation and chomping.

use super::exceptions::{Exception, Result, error_msg};
use super::exp;
use super::regex_yaml::RegEx;
use super::stream::Stream;

/// `CHOMP` (scanscalar.h:15).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chomp {
    Strip,
    Clip,
    Keep,
}

/// `ACTION` (scanscalar.h:16).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    Break,
    Throw,
}

/// `FOLD` (scanscalar.h:17).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fold {
    DontFold,
    FoldBlock,
    FoldFlow,
}

/// Port of `ScanScalarParams` (scanscalar.h:19-58).
#[derive(Debug, Clone)]
pub struct ScanScalarParams<'r> {
    // input:
    /// What condition ends this scalar? (`Exp::Empty()` when unset.)
    pub end: Option<&'r RegEx>,
    /// Should we eat that condition when we see it?
    pub eat_end: bool,
    /// What level of indentation should be eaten and ignored?
    pub indent: i32,
    /// Should we try to autodetect the indent?
    pub detect_indent: bool,
    /// Should we continue eating this delicious indentation after `indent` spaces?
    pub eat_leading_whitespace: bool,
    /// What character do we escape on (a backslash or a single quote; 0 for none)?
    pub escape: u8,
    /// How do we fold line ends?
    pub fold: Fold,
    /// Do we remove all trailing spaces (at the very end)?
    pub trim_trailing_spaces: bool,
    /// Do we strip, clip, or keep trailing newlines (at the very end)?
    pub chomp: Chomp,
    /// What do we do if we see a document indicator?
    pub on_doc_indicator: Action,
    /// What do we do if we see a tab where we should be seeing indentation spaces?
    pub on_tab_in_indentation: Action,

    // output:
    pub leading_spaces: bool,
}

impl Default for ScanScalarParams<'_> {
    /// `ScanScalarParams()` (scanscalar.h:20-32).
    fn default() -> Self {
        ScanScalarParams {
            end: None,
            eat_end: false,
            indent: 0,
            detect_indent: false,
            eat_leading_whitespace: false,
            escape: 0,
            fold: Fold::DontFold,
            trim_trailing_spaces: false,
            chomp: Chomp::Clip,
            on_doc_indicator: Action::None,
            on_tab_in_indentation: Action::None,
            leading_spaces: false,
        }
    }
}

/// `std::string::find_last_not_of` over a set of bytes.
fn find_last_not_of(s: &[u8], set: &[u8]) -> Option<usize> {
    s.iter().rposition(|c| !set.contains(c))
}

/// Port of `ScanScalar(Stream&, ScanScalarParams&)` (scanscalar.cpp:21-250). Scans in three
/// phases per line: until the line ending, the line ending, then the leading blanks of the
/// next line; the parameters decide where to stop and what to keep.
pub fn scan_scalar(input: &mut Stream<'_>, params: &mut ScanScalarParams<'_>) -> Result<Vec<u8>> {
    let mut found_non_empty_line = false;
    let mut past_opening_break = params.fold == Fold::FoldFlow;
    let mut empty_line = false;
    let mut more_indented = false;
    let mut folded_newline_count: i32 = 0;
    let mut folded_newline_started_more_indented = false;
    // `std::string::npos` is `None`.
    let mut last_escaped_char: Option<usize> = None;
    let mut scalar: Vec<u8> = Vec::new();
    params.leading_spaces = false;

    let end: &RegEx = params.end.unwrap_or(&exp::EMPTY);

    while input.is_valid() {
        // ********************************
        // Phase #1: scan until line ending

        let mut last_non_whitespace_char = scalar.len();
        let mut escaped_newline = false;
        while !end.matches_stream(input) && !exp::BREAK.matches_stream(input) {
            if !input.is_valid() {
                break;
            }

            // document indicator?
            if input.column() == 0 && exp::DOC_INDICATOR.matches_stream(input) {
                if params.on_doc_indicator == Action::Break {
                    break;
                }
                if params.on_doc_indicator == Action::Throw {
                    return Err(Exception::parser(input.mark(), error_msg::DOC_IN_SCALAR));
                }
            }

            found_non_empty_line = true;
            past_opening_break = true;

            // escaped newline? (only if we're escaping on slash)
            if params.escape == b'\\' && exp::ESC_BREAK.matches_stream(input) {
                // eat escape character and get out (but preserve trailing whitespace!)
                input.get();
                last_non_whitespace_char = scalar.len();
                last_escaped_char = Some(scalar.len());
                escaped_newline = true;
                break;
            }

            // escape this?
            if input.peek() == params.escape {
                scalar.extend_from_slice(&exp::escape(input)?);
                last_non_whitespace_char = scalar.len();
                last_escaped_char = Some(scalar.len());
                continue;
            }

            // otherwise, just add the damn character
            let ch = input.get();
            scalar.push(ch);
            if ch != b' ' && ch != b'\t' {
                last_non_whitespace_char = scalar.len();
            }
        }

        // eof? if we're looking to eat something, then we throw
        if !input.is_valid() {
            if params.eat_end {
                return Err(Exception::parser(input.mark(), error_msg::EOF_IN_SCALAR));
            }
            break;
        }

        // doc indicator?
        if params.on_doc_indicator == Action::Break
            && input.column() == 0
            && exp::DOC_INDICATOR.matches_stream(input)
        {
            break;
        }

        // are we done via character match?
        let n = end.match_stream(input);
        if n >= 0 {
            if params.eat_end {
                input.eat(n);
            }
            break;
        }

        // do we remove trailing whitespace?
        if params.fold == Fold::FoldFlow {
            scalar.truncate(last_non_whitespace_char);
        }

        // ********************************
        // Phase #2: eat line ending
        let n = exp::BREAK.match_stream(input);
        input.eat(n);

        // ********************************
        // Phase #3: scan initial spaces

        // first the required indentation
        while input.peek() == b' '
            && (input.column() < params.indent || (params.detect_indent && !found_non_empty_line))
            && !end.matches_stream(input)
        {
            input.eat(1);
        }

        // update indent if we're auto-detecting
        if params.detect_indent && !found_non_empty_line {
            params.indent = params.indent.max(input.column());
        }

        // and then the rest of the whitespace
        while exp::BLANK.matches_stream(input) {
            // we check for tabs that masquerade as indentation
            if input.peek() == b'\t'
                && input.column() < params.indent
                && params.on_tab_in_indentation == Action::Throw
            {
                return Err(Exception::parser(
                    input.mark(),
                    error_msg::TAB_IN_INDENTATION,
                ));
            }

            if !params.eat_leading_whitespace {
                break;
            }

            if end.matches_stream(input) {
                break;
            }

            input.eat(1);
        }

        // was this an empty line?
        let next_empty_line = exp::BREAK.matches_stream(input);
        let next_more_indented = exp::BLANK.matches_stream(input);
        if params.fold == Fold::FoldBlock && folded_newline_count == 0 && next_empty_line {
            folded_newline_started_more_indented = more_indented;
        }

        // for block scalars, we always start with a newline, so we should ignore it (not fold
        // or keep)
        if past_opening_break {
            match params.fold {
                Fold::DontFold => scalar.push(b'\n'),
                Fold::FoldBlock => {
                    if !empty_line
                        && !next_empty_line
                        && !more_indented
                        && !next_more_indented
                        && input.column() >= params.indent
                    {
                        scalar.push(b' ');
                    } else if next_empty_line {
                        folded_newline_count += 1;
                    } else {
                        scalar.push(b'\n');
                    }

                    if !next_empty_line && folded_newline_count > 0 {
                        scalar.extend(std::iter::repeat_n(
                            b'\n',
                            (folded_newline_count - 1) as usize,
                        ));
                        if folded_newline_started_more_indented
                            || next_more_indented
                            || !found_non_empty_line
                        {
                            scalar.push(b'\n');
                        }
                        folded_newline_count = 0;
                    }
                }
                Fold::FoldFlow => {
                    if next_empty_line {
                        scalar.push(b'\n');
                    } else if !empty_line && !escaped_newline {
                        scalar.push(b' ');
                    }
                }
            }
        }

        empty_line = next_empty_line;
        more_indented = next_more_indented;
        past_opening_break = true;

        // are we done via indentation?
        if !empty_line && input.column() < params.indent {
            params.leading_spaces = true;
            break;
        }
    }

    // post-processing
    if params.trim_trailing_spaces {
        let mut pos = find_last_not_of(&scalar, b" \t");
        if let Some(escaped) = last_escaped_char
            && pos.is_none_or(|p| p < escaped)
        {
            pos = Some(escaped);
        }
        if let Some(p) = pos
            && p < scalar.len()
        {
            scalar.truncate(p + 1);
        }
    }

    match params.chomp {
        Chomp::Clip => {
            let mut pos = find_last_not_of(&scalar, b"\n");
            if let Some(escaped) = last_escaped_char
                && pos.is_none_or(|p| p < escaped)
            {
                pos = Some(escaped);
            }
            match pos {
                None => scalar.clear(),
                Some(p) => {
                    if p + 1 < scalar.len() {
                        scalar.truncate(p + 2);
                    }
                }
            }
        }
        Chomp::Strip => {
            let mut pos = find_last_not_of(&scalar, b"\n");
            if let Some(escaped) = last_escaped_char
                && pos.is_none_or(|p| p < escaped)
            {
                pos = Some(escaped);
            }
            match pos {
                None => scalar.clear(),
                Some(p) => {
                    if p < scalar.len() {
                        scalar.truncate(p + 1);
                    }
                }
            }
        }
        Chomp::Keep => {}
    }

    Ok(scalar)
}
