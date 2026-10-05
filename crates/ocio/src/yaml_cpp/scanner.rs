// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.8.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! Port of `YAML::Scanner` (src/scanner.h, src/scanner.cpp, yaml-cpp 0.8.0): turns the stream
//! of characters into a queue of tokens, on demand. The parser asks for the next token; the
//! scanner scans until the front of the queue is a token it is sure of. A token that may
//! start a simple key ("key: value" without a `?`) stays unverified, and keeps the tokens
//! behind it waiting, until the `:` shows up or the key becomes impossible.
//!
//! The token scanners are in `scan_token.rs` (scantoken.cpp) and the simple keys in
//! `simple_key.rs` (simplekey.cpp).
//!
//! yaml-cpp keeps pointers to queued tokens and to indentation markers. The port keeps
//! indices: a token's id counts every token ever queued (the queue only grows at the back and
//! shrinks at the front, as `std::queue`'s references stay valid), and the markers live in
//! `indent_refs` for the scanner's lifetime, as in C++.

use std::collections::VecDeque;

use super::exceptions::{Exception, Result, error_msg};
use super::exp::{self, keys};
use super::mark::Mark;
use super::regex_yaml::RegEx;
use super::stream::Stream;
use super::token::{Token, TokenStatus, TokenType};

/// `Scanner::IndentMarker::INDENT_TYPE` (scanner.h:43).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum IndentType {
    Map,
    Seq,
    None,
}

/// `Scanner::IndentMarker::STATUS` (scanner.h:44).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum IndentStatus {
    Valid,
    Invalid,
    Unknown,
}

/// `Scanner::IndentMarker` (scanner.h:42-52).
#[derive(Debug, Clone)]
pub(super) struct IndentMarker {
    pub(super) column: i32,
    pub(super) ty: IndentType,
    pub(super) status: IndentStatus,
    /// `pStartToken`: the id of the token that starts the collection.
    pub(super) start_token: Option<usize>,
}

impl IndentMarker {
    fn new(column: i32, ty: IndentType) -> IndentMarker {
        IndentMarker {
            column,
            ty,
            status: IndentStatus::Valid,
            start_token: None,
        }
    }
}

/// `Scanner::FLOW_MARKER` (scanner.h:54).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FlowMarker {
    FlowMap,
    FlowSeq,
}

/// `Scanner::SimpleKey` (scanner.h:144-154): a potential simple key, with the tokens it would
/// make valid.
#[derive(Debug, Clone)]
pub(super) struct SimpleKey {
    pub(super) mark: Mark,
    pub(super) flow_level: usize,
    /// `pIndent`: the index of the indentation marker in `indent_refs`.
    pub(super) indent: Option<usize>,
    /// `pMapStart`: the id of the map start token.
    pub(super) map_start: Option<usize>,
    /// `pKey`: the id of the key token.
    pub(super) key: Option<usize>,
}

/// Port of `YAML::Scanner` (scanner.h:24-186).
#[derive(Debug)]
pub struct Scanner<'a> {
    /// `INPUT`: the stream.
    pub(super) input: Stream<'a>,

    /// `m_tokens`: the output.
    tokens: VecDeque<Token>,
    /// How many tokens have left the queue: the id of the token at its front.
    popped: usize,

    // state info
    started_stream: bool,
    ended_stream: bool,
    pub(super) simple_key_allowed: bool,
    pub(super) can_be_json_flow: bool,
    pub(super) simple_keys: Vec<SimpleKey>,
    /// `m_indents`: indices into `indent_refs`.
    indents: Vec<usize>,
    /// `m_indentRefs`: every indentation marker, kept for the scanner's lifetime.
    pub(super) indent_refs: Vec<IndentMarker>,
    pub(super) flows: Vec<FlowMarker>,
}

impl<'a> Scanner<'a> {
    /// `Scanner(std::istream&)` (scanner.cpp:10-20).
    pub fn new(input: &'a [u8]) -> Scanner<'a> {
        Scanner {
            input: Stream::new(input),
            tokens: VecDeque::new(),
            popped: 0,
            started_stream: false,
            ended_stream: false,
            simple_key_allowed: false,
            can_be_json_flow: false,
            simple_keys: Vec::new(),
            indents: Vec::new(),
            indent_refs: Vec::new(),
            flows: Vec::new(),
        }
    }

    /// `empty()` (scanner.cpp:24-27): whether no token is left.
    pub fn empty(&mut self) -> Result<bool> {
        self.ensure_tokens_in_queue()?;
        Ok(self.tokens.is_empty())
    }

    /// `pop()` (scanner.cpp:29-33): removes the next token.
    pub fn pop(&mut self) -> Result<()> {
        self.ensure_tokens_in_queue()?;
        if self.tokens.pop_front().is_some() {
            self.popped += 1;
        }
        Ok(())
    }

    /// `peek()` (scanner.cpp:35-49): the next token. yaml-cpp asserts that there is one; the
    /// parser always checks `empty()` first.
    pub fn peek(&mut self) -> Result<&Token> {
        self.ensure_tokens_in_queue()?;
        Ok(self
            .tokens
            .front()
            .expect("Scanner::peek: the parser checks empty() first"))
    }

    /// `mark()` (scanner.cpp:51): the stream's current position.
    pub fn mark(&self) -> Mark {
        self.input.mark()
    }

    /// Sets the status of the queued token with this id (yaml-cpp writes through a `Token*`).
    /// A simple key's tokens are unverified until the key is resolved, and the queue never
    /// gives up an unverified token, so the token is always still queued.
    pub(super) fn set_token_status(&mut self, id: usize, status: TokenStatus) {
        if let Some(token) = id
            .checked_sub(self.popped)
            .and_then(|i| self.tokens.get_mut(i))
        {
            token.status = status;
        }
    }

    /// `EnsureTokensInQueue()` (scanner.cpp:53-80): scans until the front token is valid, or
    /// the stream has ended. Invalid tokens at the front are dropped.
    fn ensure_tokens_in_queue(&mut self) -> Result<()> {
        loop {
            if let Some(token) = self.tokens.front() {
                // if this guy's valid, then we're done
                if token.status == TokenStatus::Valid {
                    return Ok(());
                }

                // here's where we clean up the impossible tokens
                if token.status == TokenStatus::Invalid {
                    self.tokens.pop_front();
                    self.popped += 1;
                    continue;
                }

                // note: what's left are the unverified tokens
            }

            // no token? maybe we've actually finished
            if self.ended_stream {
                return Ok(());
            }

            // no? then scan...
            self.scan_next_token()?;
        }
    }

    /// `ScanNextToken()` (scanner.cpp:82-174): scans one token, branching on the next few
    /// characters.
    fn scan_next_token(&mut self) -> Result<()> {
        if self.ended_stream {
            return Ok(());
        }

        if !self.started_stream {
            self.start_stream();
            return Ok(());
        }

        // get rid of whitespace, etc. (in between tokens it should be irrelevant)
        self.scan_to_next_token();

        // maybe need to end some blocks
        self.pop_indent_to_here();

        // *****
        // And now branch based on the next few characters!
        // *****

        // end of stream
        if !self.input.is_valid() {
            self.end_stream();
            return Ok(());
        }

        if self.input.column() == 0 && self.input.peek() == keys::DIRECTIVE {
            self.scan_directive();
            return Ok(());
        }

        // document token
        if self.input.column() == 0 && exp::DOC_START.matches_stream(&self.input) {
            self.scan_doc_start();
            return Ok(());
        }

        if self.input.column() == 0 && exp::DOC_END.matches_stream(&self.input) {
            self.scan_doc_end();
            return Ok(());
        }

        // flow start/end/entry
        if self.input.peek() == keys::FLOW_SEQ_START || self.input.peek() == keys::FLOW_MAP_START {
            self.scan_flow_start();
            return Ok(());
        }

        if self.input.peek() == keys::FLOW_SEQ_END || self.input.peek() == keys::FLOW_MAP_END {
            return self.scan_flow_end();
        }

        if self.input.peek() == keys::FLOW_ENTRY {
            self.scan_flow_entry();
            return Ok(());
        }

        // block/map stuff
        if exp::BLOCK_ENTRY.matches_stream(&self.input) {
            return self.scan_block_entry();
        }

        let key_regex: &RegEx = if self.in_block_context() {
            &exp::KEY
        } else {
            &exp::KEY_IN_FLOW
        };
        if key_regex.matches_stream(&self.input) {
            return self.scan_key();
        }

        if self.get_value_regex().matches_stream(&self.input) {
            return self.scan_value();
        }

        // alias/anchor
        if self.input.peek() == keys::ALIAS || self.input.peek() == keys::ANCHOR {
            return self.scan_anchor_or_alias();
        }

        // tag
        if self.input.peek() == keys::TAG {
            return self.scan_tag();
        }

        // special scalars
        if self.in_block_context()
            && (self.input.peek() == keys::LITERAL_SCALAR
                || self.input.peek() == keys::FOLDED_SCALAR)
        {
            return self.scan_block_scalar();
        }

        if self.input.peek() == b'\'' || self.input.peek() == b'"' {
            return self.scan_quoted_scalar();
        }

        // plain scalars
        let plain_regex: &RegEx = if self.in_block_context() {
            &exp::PLAIN_SCALAR
        } else {
            &exp::PLAIN_SCALAR_IN_FLOW
        };
        if plain_regex.matches_stream(&self.input) {
            return self.scan_plain_scalar();
        }

        // don't know what it is!
        Err(Exception::parser(
            self.input.mark(),
            error_msg::UNKNOWN_TOKEN,
        ))
    }

    /// `ScanToNextToken()` (scanner.cpp:176-211): eats blanks, comments and line breaks. A
    /// tab in block context forbids a simple key; a line break invalidates the pending one.
    fn scan_to_next_token(&mut self) {
        loop {
            // first eat whitespace
            while self.input.is_valid() && Self::is_whitespace_to_be_eaten(self.input.peek()) {
                if self.in_block_context() && exp::TAB.matches_stream(&self.input) {
                    self.simple_key_allowed = false;
                }
                self.input.eat(1);
            }

            // then eat a comment
            if exp::COMMENT.matches_stream(&self.input) {
                // eat until line break
                while self.input.is_valid() && !exp::BREAK.matches_stream(&self.input) {
                    self.input.eat(1);
                }
            }

            // if it's NOT a line break, then we're done!
            if !exp::BREAK.matches_stream(&self.input) {
                break;
            }

            // otherwise, let's eat the line break and keep going
            let n = exp::BREAK.match_stream(&self.input);
            self.input.eat(n);

            // oh yeah, and let's get rid of that simple key
            self.invalidate_simple_key();

            // new line - we may be able to accept a simple key now
            if self.in_block_context() {
                self.simple_key_allowed = true;
            }
        }
    }

    /// `IsWhitespaceToBeEaten(char)` (scanner.cpp:225-235): a space or a tab.
    fn is_whitespace_to_be_eaten(ch: u8) -> bool {
        ch == b' ' || ch == b'\t'
    }

    /// `GetValueRegex()` (scanner.cpp:237-243): the expression a value indicator matches here.
    fn get_value_regex(&self) -> &'static RegEx {
        if self.in_block_context() {
            return &exp::VALUE;
        }
        if self.can_be_json_flow {
            &exp::VALUE_IN_JSON_FLOW
        } else {
            &exp::VALUE_IN_FLOW
        }
    }

    /// `StartStream()` (scanner.cpp:245-252): the base indentation, at column -1.
    fn start_stream(&mut self) {
        self.started_stream = true;
        self.simple_key_allowed = true;
        self.indent_refs
            .push(IndentMarker::new(-1, IndentType::None));
        self.indents.push(self.indent_refs.len() - 1);
    }

    /// `EndStream()` (scanner.cpp:254-265): closes every block.
    fn end_stream(&mut self) {
        // force newline
        if self.input.column() > 0 {
            self.input.reset_column();
        }

        self.pop_all_indents();
        self.pop_all_simple_keys();

        self.simple_key_allowed = false;
        self.ended_stream = true;
    }

    /// Queues a token (`m_tokens.push`) and gives its id.
    pub(super) fn push(&mut self, token: Token) -> usize {
        self.tokens.push_back(token);
        self.popped + self.tokens.len() - 1
    }

    /// `PushToken(Token::TYPE)` (scanner.cpp:267-270): a token at the current mark.
    fn push_token(&mut self, ty: TokenType) -> usize {
        let mark = self.input.mark();
        self.push(Token::new(ty, mark))
    }

    /// `InFlowContext()`
    pub(super) fn in_flow_context(&self) -> bool {
        !self.flows.is_empty()
    }

    /// `InBlockContext()`
    pub(super) fn in_block_context(&self) -> bool {
        self.flows.is_empty()
    }

    /// `GetFlowLevel()`
    pub(super) fn get_flow_level(&self) -> usize {
        self.flows.len()
    }

    /// `GetStartTokenFor(INDENT_TYPE)` (scanner.cpp:272-284). Never called with `NONE`.
    fn get_start_token_for(ty: IndentType) -> TokenType {
        match ty {
            IndentType::Seq => TokenType::BlockSeqStart,
            IndentType::Map | IndentType::None => TokenType::BlockMapStart,
        }
    }

    /// `PushIndentTo(int column, INDENT_TYPE)` (scanner.cpp:286-314): opens a block
    /// collection at `column` if that is an indentation, and queues its start token. Gives
    /// the marker's index.
    pub(super) fn push_indent_to(&mut self, column: i32, ty: IndentType) -> Option<usize> {
        // are we in flow?
        if self.in_flow_context() {
            return None;
        }

        let mut indent = IndentMarker::new(column, ty);
        let last_indent = &self.indent_refs[*self.indents.last()?];

        // is this actually an indentation?
        if indent.column < last_indent.column {
            return None;
        }
        if indent.column == last_indent.column
            && !(indent.ty == IndentType::Seq && last_indent.ty == IndentType::Map)
        {
            return None;
        }

        // push a start token
        indent.start_token = Some(self.push_token(Self::get_start_token_for(ty)));

        // and then the indent
        self.indent_refs.push(indent);
        let index = self.indent_refs.len() - 1;
        self.indents.push(index);
        Some(index)
    }

    /// `PopIndentToHere()` (scanner.cpp:316-341): closes the blocks the current column ends,
    /// then drops the invalid markers on top.
    fn pop_indent_to_here(&mut self) {
        // are we in flow?
        if self.in_flow_context() {
            return;
        }

        // now pop away
        while let Some(&top) = self.indents.last() {
            let indent = &self.indent_refs[top];
            if indent.column < self.input.column() {
                break;
            }
            if indent.column == self.input.column()
                && !(indent.ty == IndentType::Seq && !exp::BLOCK_ENTRY.matches_stream(&self.input))
            {
                break;
            }

            self.pop_indent();
        }

        while let Some(&top) = self.indents.last() {
            if self.indent_refs[top].status != IndentStatus::Invalid {
                break;
            }
            self.pop_indent();
        }
    }

    /// `PopAllIndents()` (scanner.cpp:343-358): closes every block but the base.
    pub(super) fn pop_all_indents(&mut self) {
        // are we in flow?
        if self.in_flow_context() {
            return;
        }

        // now pop away
        while let Some(&top) = self.indents.last() {
            if self.indent_refs[top].ty == IndentType::None {
                break;
            }
            self.pop_indent();
        }
    }

    /// `PopIndent()` (scanner.cpp:360-374): closes the top block, queueing its end token if
    /// the block was valid, or invalidating the pending simple key if not.
    fn pop_indent(&mut self) {
        let Some(top) = self.indents.pop() else {
            return;
        };
        let IndentMarker { status, ty, .. } = self.indent_refs[top];

        if status != IndentStatus::Valid {
            self.invalidate_simple_key();
            return;
        }

        if ty == IndentType::Seq {
            self.push_token(TokenType::BlockSeqEnd);
        } else if ty == IndentType::Map {
            self.push_token(TokenType::BlockMapEnd);
        }
    }

    /// `GetTopIndent()` (scanner.cpp:376-381).
    pub(super) fn get_top_indent(&self) -> i32 {
        match self.indents.last() {
            Some(&top) => self.indent_refs[top].column,
            None => 0,
        }
    }
}
