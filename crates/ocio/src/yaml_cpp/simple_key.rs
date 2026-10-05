// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.8.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! Port of the scanner's simple keys (src/simplekey.cpp, yaml-cpp 0.8.0). A token that could
//! start "key: value" queues an unverified KEY token (and a block map start, if it opens a
//! map); the `:` validates them, and anything else that rules the key out invalidates them.

use super::scanner::{IndentStatus, IndentType, Scanner, SimpleKey};
use super::token::{Token, TokenStatus, TokenType};

impl SimpleKey {
    /// `SimpleKey(const Mark&, std::size_t flowLevel)` (simplekey.cpp:7-12).
    fn new(mark: super::mark::Mark, flow_level: usize) -> SimpleKey {
        SimpleKey {
            mark,
            flow_level,
            indent: None,
            map_start: None,
            key: None,
        }
    }
}

impl Scanner<'_> {
    /// `SimpleKey::Validate()` (simplekey.cpp:14-24).
    fn validate(&mut self, key: &SimpleKey) {
        // Note: pIndent will *not* be garbage here; we "garbage collect" them so we can always
        // refer to them
        if let Some(indent) = key.indent {
            self.indent_refs[indent].status = IndentStatus::Valid;
        }
        if let Some(id) = key.map_start {
            self.set_token_status(id, TokenStatus::Valid);
        }
        if let Some(id) = key.key {
            self.set_token_status(id, TokenStatus::Valid);
        }
    }

    /// `SimpleKey::Invalidate()` (simplekey.cpp:26-33).
    fn invalidate(&mut self, key: &SimpleKey) {
        if let Some(indent) = key.indent {
            self.indent_refs[indent].status = IndentStatus::Invalid;
        }
        if let Some(id) = key.map_start {
            self.set_token_status(id, TokenStatus::Invalid);
        }
        if let Some(id) = key.key {
            self.set_token_status(id, TokenStatus::Invalid);
        }
    }

    /// `CanInsertPotentialSimpleKey()` (simplekey.cpp:35-41).
    fn can_insert_potential_simple_key(&self) -> bool {
        if !self.simple_key_allowed {
            return false;
        }
        !self.exists_active_simple_key()
    }

    /// `ExistsActiveSimpleKey()` (simplekey.cpp:43-53): whether there's a potential simple
    /// key at our flow level (at most one per flow level is allowed).
    fn exists_active_simple_key(&self) -> bool {
        match self.simple_keys.last() {
            Some(key) => key.flow_level == self.get_flow_level(),
            None => false,
        }
    }

    /// `InsertPotentialSimpleKey()` (simplekey.cpp:55-80): if we can, queues a potential
    /// simple key (with a map start in block context) and saves it on the stack.
    pub(super) fn insert_potential_simple_key(&mut self) {
        if !self.can_insert_potential_simple_key() {
            return;
        }

        let mut key = SimpleKey::new(self.input.mark(), self.get_flow_level());

        // first add a map start, if necessary
        if self.in_block_context() {
            key.indent = self.push_indent_to(self.input.column(), IndentType::Map);
            if let Some(indent) = key.indent {
                self.indent_refs[indent].status = IndentStatus::Unknown;
                key.map_start = self.indent_refs[indent].start_token;
                if let Some(id) = key.map_start {
                    self.set_token_status(id, TokenStatus::Unverified);
                }
            }
        }

        // then add the (now unverified) key
        let mark = self.input.mark();
        let id = self.push(Token::new(TokenType::Key, mark));
        key.key = Some(id);
        self.set_token_status(id, TokenStatus::Unverified);

        self.simple_keys.push(key);
    }

    /// `InvalidateSimpleKey()` (simplekey.cpp:82-95): invalidates the simple key at our flow
    /// level, if any.
    pub(super) fn invalidate_simple_key(&mut self) {
        let Some(key) = self.simple_keys.last() else {
            return;
        };

        // grab top key
        if key.flow_level != self.get_flow_level() {
            return;
        }

        let key = key.clone();
        self.invalidate(&key);
        self.simple_keys.pop();
    }

    /// `VerifySimpleKey()` (simplekey.cpp:97-126): validates the latest simple key if it is
    /// at our flow level, on the same line and less than 1024 characters back; invalidates it
    /// otherwise.
    pub(super) fn verify_simple_key(&mut self) -> bool {
        // grab top key
        let Some(key) = self.simple_keys.last().cloned() else {
            return false;
        };

        // only validate if we're in the correct flow level
        if key.flow_level != self.get_flow_level() {
            return false;
        }

        self.simple_keys.pop();

        let mut is_valid = true;

        // needs to be less than 1024 characters and inline
        if self.input.line() != key.mark.line || self.input.pos().wrapping_sub(key.mark.pos) > 1024
        {
            is_valid = false;
        }

        // invalidate key
        if is_valid {
            self.validate(&key);
        } else {
            self.invalidate(&key);
        }

        is_valid
    }

    /// `PopAllSimpleKeys()` (simplekey.cpp:128-131).
    pub(super) fn pop_all_simple_keys(&mut self) {
        self.simple_keys.clear();
    }
}
