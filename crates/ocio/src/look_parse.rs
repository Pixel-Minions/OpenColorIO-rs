// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The parsing of look lists: a port of `src/OpenColorIO/LookParse.h` and `LookParse.cpp` @
//! v2.5.2. It splits lists with `SplitStringEnvStyle` (`ocio_ops::parse_utils`).
//!
//! A list such as `" +cc,-onset | +cc "` parses to two options, `(+cc, -onset)` and `(+cc)`:
//! the first whose looks all exist is applied.

// The look transform's builder (WP 3.2b) and the config's look checks use it.
#![allow(dead_code)]

use ocio_ops::exception::Result;
use ocio_ops::open_color_types::{TransformDirection, get_inverse_transform_direction};
use ocio_ops::parse_utils::split_string_env_style;
use ocio_ops::utils::string_utils::{self, trim};

/// One look of an option: its name and direction.
///
/// Port of `LookParseResult::Token` (src/OpenColorIO/LookParse.h:22-32 @ v2.5.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Token {
    /// `name`.
    pub(crate) name: Vec<u8>,
    /// `dir`.
    pub(crate) dir: TransformDirection,
}

impl Default for Token {
    /// Port of `Token::Token` (LookParse.h:27-28 @ v2.5.2): an empty name, forward.
    fn default() -> Token {
        Token {
            name: Vec::new(),
            dir: TransformDirection::Forward,
        }
    }
}

impl Token {
    /// Reads a look: a leading `+` (forward) or `-` (inverse) sets the direction, and every
    /// leading `+` or `-` is removed from the name.
    ///
    /// Port of `LookParseResult::Token::parse` (LookParse.cpp:16-36 @ v2.5.2).
    pub(crate) fn parse(&mut self, str: &[u8]) {
        // Assert no commas, colons, or | in str.

        if string_utils::starts_with_char(str, b'+') {
            self.name = string_utils::left_trim_char(str, b'+').to_vec();
            self.dir = TransformDirection::Forward;
        }
        // TODO: Handle --
        else if string_utils::starts_with_char(str, b'-') {
            self.name = string_utils::left_trim_char(str, b'-').to_vec();
            self.dir = TransformDirection::Inverse;
        } else {
            self.name = str.to_vec();
            self.dir = TransformDirection::Forward;
        }
    }

    /// Writes the look: its name, after a `-` when inverse.
    ///
    /// Port of `LookParseResult::Token::serialize` (LookParse.cpp:38-49 @ v2.5.2).
    pub(crate) fn serialize(&self, os: &mut Vec<u8>) {
        match self.dir {
            TransformDirection::Forward => os.extend_from_slice(&self.name),
            TransformDirection::Inverse => {
                os.push(b'-');
                os.extend_from_slice(&self.name);
            }
        }
    }
}

/// The looks of one option, in order.
///
/// Port of `LookParseResult::Tokens` (LookParse.h:34 @ v2.5.2).
pub(crate) type Tokens = Vec<Token>;

/// The options of a look list, in order of precedence.
///
/// Port of `LookParseResult::Options` (LookParse.h:38 @ v2.5.2).
pub(crate) type Options = Vec<Tokens>;

/// A parsed look list: its options, each a list of looks.
///
/// Port of `LookParseResult` (src/OpenColorIO/LookParse.h:19-49 @ v2.5.2).
#[derive(Debug, Clone, Default)]
pub(crate) struct LookParseResult {
    /// `m_options`.
    options: Options,
}

impl LookParseResult {
    /// Writes the looks of one option, separated by `", "`.
    ///
    /// Port of `LookParseResult::serialize` (LookParse.cpp:51-58 @ v2.5.2).
    pub(crate) fn serialize(os: &mut Vec<u8>, tokens: &[Token]) {
        for (i, token) in tokens.iter().enumerate() {
            if i != 0 {
                os.extend_from_slice(b", ");
            }
            token.serialize(os);
        }
    }

    /// Parses `looksstr`: options separated by `|`, each a list of looks as
    /// [`split_string_env_style`] splits it. A blank string has no option.
    ///
    /// Port of `LookParseResult::parse` (LookParse.cpp:60-91 @ v2.5.2).
    pub(crate) fn parse(&mut self, looksstr: &[u8]) -> Result<&Options> {
        self.options.clear();

        let strippedlooks = trim(looksstr);
        if strippedlooks.is_empty() {
            return Ok(&self.options);
        }

        let options = string_utils::split(strippedlooks, b'|');

        for option in &options {
            let mut tokens = Tokens::new();

            for v in split_string_env_style(option)? {
                let mut t = Token::default();
                t.parse(&v);
                tokens.push(t);
            }

            self.options.push(tokens);
        }

        Ok(&self.options)
    }

    /// Port of `LookParseResult::getOptions` (LookParse.cpp:93-96 @ v2.5.2).
    pub(crate) fn options(&self) -> &Options {
        &self.options
    }

    /// Port of `LookParseResult::empty` (LookParse.cpp:98-101 @ v2.5.2).
    pub(crate) fn is_empty(&self) -> bool {
        self.options.is_empty()
    }

    /// Reverses each option, to apply it inverse: its looks in the reverse order, each in the
    /// other direction. The options keep their order of precedence.
    ///
    /// Port of `LookParseResult::reverse` (LookParse.cpp:103-120 @ v2.5.2).
    pub(crate) fn reverse(&mut self) {
        // m_options itself should NOT be reversed.
        // The individual looks
        // need to be applied in the inverse direction. But, the precedence
        // for which option to apply is to be maintained!

        for option in &mut self.options {
            option.reverse();

            for token in option.iter_mut() {
                token.dir = get_inverse_transform_direction(token.dir);
            }
        }
    }
}

#[cfg(test)]
#[path = "look_parse_tests.rs"]
mod tests;
