// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! A list of tokens, such as a color space's categories: a port of
//! `src/OpenColorIO/TokensManager.h` @ v2.5.2.

use ocio_ops::utils::string_utils::{c_str, lower, trim};

/// A list of tokens that ignores case and the whitespace around a token when it looks one up,
/// and keeps each token trimmed, in the order it was added.
///
/// Port of `TokensManager` (src/OpenColorIO/TokensManager.h:20-97 @ v2.5.2).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TokensManager {
    /// `m_tokens`.
    tokens: Vec<Vec<u8>>,
}

/// `StringUtils::Trim(StringUtils::Lower(s))`: how tokens compare.
fn key(s: &[u8]) -> Vec<u8> {
    trim(&lower(s)).to_vec()
}

impl TokensManager {
    /// The index of the token that matches `token` (up to its first NUL) ignoring case and
    /// the surrounding whitespace; `None` for an empty token.
    ///
    /// Port of `TokensManager::findToken` (TokensManager.h:29-45 @ v2.5.2).
    fn find_token(&self, token: &[u8]) -> Option<usize> {
        let token = c_str(token);
        if token.is_empty() {
            return None;
        }

        // NB: Categories are not case-sensitive and whitespace is stripped.
        let reference = key(token);

        self.tokens.iter().position(|t| key(t) == reference)
    }

    /// Port of `TokensManager::hasToken` (TokensManager.h:47-50 @ v2.5.2).
    pub(crate) fn has_token(&self, token: &[u8]) -> bool {
        self.find_token(token).is_some()
    }

    /// Adds `token` (up to its first NUL), trimmed, unless it is empty or already there.
    ///
    /// Port of `TokensManager::addToken` (TokensManager.h:52-59 @ v2.5.2).
    pub(crate) fn add_token(&mut self, token: &[u8]) {
        let token = c_str(token);
        if token.is_empty() {
            return;
        }
        if self.find_token(token).is_none() {
            self.tokens.push(trim(token).to_vec());
        }
    }

    /// Removes the first token that matches `token`.
    ///
    /// Port of `TokensManager::removeToken` (TokensManager.h:61-78 @ v2.5.2).
    pub(crate) fn remove_token(&mut self, token: &[u8]) {
        let token = c_str(token);
        if token.is_empty() {
            return;
        }

        // NB: Categories are not case-sensitive and whitespace is stripped.
        let reference = key(token);

        if let Some(index) = self.tokens.iter().position(|t| key(t) == reference) {
            self.tokens.remove(index);
        }
    }

    /// Port of `TokensManager::getNumTokens` (TokensManager.h:80-83 @ v2.5.2).
    pub(crate) fn num_tokens(&self) -> i32 {
        self.tokens.len() as i32
    }

    /// The token at `index`, or `None` (upstream's null pointer) outside the list.
    ///
    /// Port of `TokensManager::getToken` (TokensManager.h:85-90 @ v2.5.2).
    pub(crate) fn token(&self, index: i32) -> Option<&[u8]> {
        if index < 0 || index >= self.tokens.len() as i32 {
            return None;
        }

        Some(&self.tokens[index as usize])
    }

    /// Port of `TokensManager::clearTokens` (TokensManager.h:92-95 @ v2.5.2).
    pub(crate) fn clear_tokens(&mut self) {
        self.tokens.clear();
    }
}
