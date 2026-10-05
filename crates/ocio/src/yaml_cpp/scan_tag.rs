// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.8.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! Port of yaml-cpp 0.8.0's tag scanning (src/scantag.h, src/scantag.cpp): verbatim tags
//! (`!<ColorSpace>`, as OCIO writes them), tag handles and tag suffixes.

use super::exceptions::{Exception, Result, error_msg};
use super::exp::{self, keys};
use super::mark::Mark;
use super::stream::Stream;

/// `ScanVerbatimTag(Stream&)` (scantag.cpp:8-29): the URI between `<` and `>`.
pub fn scan_verbatim_tag(input: &mut Stream<'_>) -> Result<Vec<u8>> {
    let mut tag = Vec::new();

    // eat the start character
    input.get();

    while input.is_valid() {
        if input.peek() == keys::VERBATIM_TAG_END {
            // eat the end character
            input.get();
            return Ok(tag);
        }

        let n = exp::URI.match_stream(input);
        if n <= 0 {
            break;
        }

        tag.extend_from_slice(&input.get_n(n));
    }

    Err(Exception::parser(
        input.mark(),
        error_msg::END_OF_VERBATIM_TAG,
    ))
}

/// `ScanTagHandle(Stream&, bool& canBeHandle)` (scantag.cpp:31-63): the word after `!`, and
/// whether it can be a handle (`!name!`): only word characters. A `!` after other tag
/// characters is an error at the first of them.
pub fn scan_tag_handle(input: &mut Stream<'_>, can_be_handle: &mut bool) -> Result<Vec<u8>> {
    let mut tag = Vec::new();
    *can_be_handle = true;
    let mut first_non_word_char = Mark::default();

    while input.is_valid() {
        if input.peek() == keys::TAG {
            if !*can_be_handle {
                return Err(Exception::parser(
                    first_non_word_char,
                    error_msg::CHAR_IN_TAG_HANDLE,
                ));
            }
            break;
        }

        let mut n = 0;
        if *can_be_handle {
            n = exp::WORD.match_stream(input);
            if n <= 0 {
                *can_be_handle = false;
                first_non_word_char = input.mark();
            }
        }

        if !*can_be_handle {
            n = exp::TAG.match_stream(input);
        }

        if n <= 0 {
            break;
        }

        tag.extend_from_slice(&input.get_n(n));
    }

    Ok(tag)
}

/// `ScanTagSuffix(Stream&)` (scantag.cpp:65-79): the tag characters after a handle.
pub fn scan_tag_suffix(input: &mut Stream<'_>) -> Result<Vec<u8>> {
    let mut tag = Vec::new();

    while input.is_valid() {
        let n = exp::TAG.match_stream(input);
        if n <= 0 {
            break;
        }

        tag.extend_from_slice(&input.get_n(n));
    }

    if tag.is_empty() {
        return Err(Exception::parser(
            input.mark(),
            error_msg::TAG_WITH_NO_SUFFIX,
        ));
    }

    Ok(tag)
}
