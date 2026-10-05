// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.8.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! Port of `YAML::Token` (src/token.h, yaml-cpp 0.8.0): what the scanner hands the parser.
//! Its `operator<<`, which only `Parser::PrintTokens` uses (OCIO calls neither), is not ported.

use super::mark::Mark;

/// `Token::STATUS` (token.h:23).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenStatus {
    Valid,
    Invalid,
    Unverified,
}

/// `Token::TYPE` (token.h:24-46).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenType {
    Directive,
    DocStart,
    DocEnd,
    BlockSeqStart,
    BlockMapStart,
    BlockSeqEnd,
    BlockMapEnd,
    BlockEntry,
    FlowSeqStart,
    FlowMapStart,
    FlowSeqEnd,
    FlowMapEnd,
    FlowMapCompact,
    FlowEntry,
    Key,
    Value,
    Anchor,
    Alias,
    Tag,
    PlainScalar,
    NonPlainScalar,
}

/// Port of `YAML::Token` (token.h:22-68).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub status: TokenStatus,
    pub ty: TokenType,
    pub mark: Mark,
    pub value: Vec<u8>,
    pub params: Vec<Vec<u8>>,
    pub data: i32,
}

impl Token {
    /// `Token(TYPE, const Mark&)` (token.h:49-50).
    pub fn new(ty: TokenType, mark: Mark) -> Token {
        Token {
            status: TokenStatus::Valid,
            ty,
            mark,
            value: Vec::new(),
            params: Vec::new(),
            data: 0,
        }
    }
}
