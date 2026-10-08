// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.8.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! Port of yaml-cpp 0.8.0's exceptions (include/yaml-cpp/exceptions.h, src/exceptions.cpp,
//! include/yaml-cpp/depthguard.h, src/depthguard.cpp): the `ErrorMsg` texts and the
//! exception classes, whose `what()` is part of OCIO's error messages ("Error: Loading the OCIO
//! profile failed. yaml-cpp: error at line 3, column 5: ...").
//!
//! The C++ class hierarchy becomes one [`Exception`] with an [`ExceptionType`]; the message is
//! bytes, since it can embed the input's bytes (an anchor name, an escape character).

use std::fmt;

use super::mark::Mark;

/// Port of `namespace YAML::ErrorMsg` (exceptions.h:20-160): the message texts, verbatim.
pub mod error_msg {
    /// `ErrorMsg::YAML_DIRECTIVE_ARGS`
    pub const YAML_DIRECTIVE_ARGS: &str = "YAML directives must have exactly one argument";
    /// `ErrorMsg::YAML_VERSION`
    pub const YAML_VERSION: &str = "bad YAML version: ";
    /// `ErrorMsg::YAML_MAJOR_VERSION`
    pub const YAML_MAJOR_VERSION: &str = "YAML major version too large";
    /// `ErrorMsg::REPEATED_YAML_DIRECTIVE`
    pub const REPEATED_YAML_DIRECTIVE: &str = "repeated YAML directive";
    /// `ErrorMsg::TAG_DIRECTIVE_ARGS`
    pub const TAG_DIRECTIVE_ARGS: &str = "TAG directives must have exactly two arguments";
    /// `ErrorMsg::REPEATED_TAG_DIRECTIVE`
    pub const REPEATED_TAG_DIRECTIVE: &str = "repeated TAG directive";
    /// `ErrorMsg::CHAR_IN_TAG_HANDLE`
    pub const CHAR_IN_TAG_HANDLE: &str = "illegal character found while scanning tag handle";
    /// `ErrorMsg::TAG_WITH_NO_SUFFIX`
    pub const TAG_WITH_NO_SUFFIX: &str = "tag handle with no suffix";
    /// `ErrorMsg::END_OF_VERBATIM_TAG`
    pub const END_OF_VERBATIM_TAG: &str = "end of verbatim tag not found";
    /// `ErrorMsg::END_OF_MAP`
    pub const END_OF_MAP: &str = "end of map not found";
    /// `ErrorMsg::END_OF_MAP_FLOW`
    pub const END_OF_MAP_FLOW: &str = "end of map flow not found";
    /// `ErrorMsg::END_OF_SEQ`
    pub const END_OF_SEQ: &str = "end of sequence not found";
    /// `ErrorMsg::END_OF_SEQ_FLOW`
    pub const END_OF_SEQ_FLOW: &str = "end of sequence flow not found";
    /// `ErrorMsg::MULTIPLE_TAGS`
    pub const MULTIPLE_TAGS: &str = "cannot assign multiple tags to the same node";
    /// `ErrorMsg::MULTIPLE_ANCHORS`
    pub const MULTIPLE_ANCHORS: &str = "cannot assign multiple anchors to the same node";
    /// `ErrorMsg::MULTIPLE_ALIASES`
    pub const MULTIPLE_ALIASES: &str = "cannot assign multiple aliases to the same node";
    /// `ErrorMsg::ALIAS_CONTENT`
    pub const ALIAS_CONTENT: &str = "aliases can't have any content, *including* tags";
    /// `ErrorMsg::INVALID_HEX`
    pub const INVALID_HEX: &str = "bad character found while scanning hex number";
    /// `ErrorMsg::INVALID_UNICODE`
    pub const INVALID_UNICODE: &str = "invalid unicode: ";
    /// `ErrorMsg::INVALID_ESCAPE`
    pub const INVALID_ESCAPE: &str = "unknown escape character: ";
    /// `ErrorMsg::UNKNOWN_TOKEN`
    pub const UNKNOWN_TOKEN: &str = "unknown token";
    /// `ErrorMsg::DOC_IN_SCALAR`
    pub const DOC_IN_SCALAR: &str = "illegal document indicator in scalar";
    /// `ErrorMsg::EOF_IN_SCALAR`
    pub const EOF_IN_SCALAR: &str = "illegal EOF in scalar";
    /// `ErrorMsg::CHAR_IN_SCALAR`
    pub const CHAR_IN_SCALAR: &str = "illegal character in scalar";
    /// `ErrorMsg::TAB_IN_INDENTATION`
    pub const TAB_IN_INDENTATION: &str = "illegal tab when looking for indentation";
    /// `ErrorMsg::FLOW_END`
    pub const FLOW_END: &str = "illegal flow end";
    /// `ErrorMsg::BLOCK_ENTRY`
    pub const BLOCK_ENTRY: &str = "illegal block entry";
    /// `ErrorMsg::MAP_KEY`
    pub const MAP_KEY: &str = "illegal map key";
    /// `ErrorMsg::MAP_VALUE`
    pub const MAP_VALUE: &str = "illegal map value";
    /// `ErrorMsg::ALIAS_NOT_FOUND`
    pub const ALIAS_NOT_FOUND: &str = "alias not found after *";
    /// `ErrorMsg::ANCHOR_NOT_FOUND`
    pub const ANCHOR_NOT_FOUND: &str = "anchor not found after &";
    /// `ErrorMsg::CHAR_IN_ALIAS`
    pub const CHAR_IN_ALIAS: &str = "illegal character found while scanning alias";
    /// `ErrorMsg::CHAR_IN_ANCHOR`
    pub const CHAR_IN_ANCHOR: &str = "illegal character found while scanning anchor";
    /// `ErrorMsg::ZERO_INDENT_IN_BLOCK`
    pub const ZERO_INDENT_IN_BLOCK: &str = "cannot set zero indentation for a block scalar";
    /// `ErrorMsg::CHAR_IN_BLOCK`
    pub const CHAR_IN_BLOCK: &str = "unexpected character in block scalar";
    /// `ErrorMsg::AMBIGUOUS_ANCHOR`
    pub const AMBIGUOUS_ANCHOR: &str = "cannot assign the same alias to multiple nodes";
    /// `ErrorMsg::UNKNOWN_ANCHOR`
    pub const UNKNOWN_ANCHOR: &str = "the referenced anchor is not defined: ";
    /// `ErrorMsg::INVALID_NODE`
    pub const INVALID_NODE: &str = "invalid node; this may result from using a map iterator as a \
                                    sequence iterator, or vice-versa";
    /// `ErrorMsg::INVALID_SCALAR`
    pub const INVALID_SCALAR: &str = "invalid scalar";
    /// `ErrorMsg::KEY_NOT_FOUND`
    pub const KEY_NOT_FOUND: &str = "key not found";
    /// `ErrorMsg::BAD_CONVERSION`
    pub const BAD_CONVERSION: &str = "bad conversion";
    /// `ErrorMsg::BAD_DEREFERENCE`
    pub const BAD_DEREFERENCE: &str = "bad dereference";
    /// `ErrorMsg::BAD_SUBSCRIPT`
    pub const BAD_SUBSCRIPT: &str = "operator[] call on a scalar";
    /// `ErrorMsg::BAD_PUSHBACK`
    pub const BAD_PUSHBACK: &str = "appending to a non-sequence";
    /// `ErrorMsg::BAD_INSERT`
    pub const BAD_INSERT: &str = "inserting in a non-convertible-to-map";
    /// `ErrorMsg::UNMATCHED_GROUP_TAG`
    pub const UNMATCHED_GROUP_TAG: &str = "unmatched group tag";
    /// `ErrorMsg::UNEXPECTED_END_SEQ`
    pub const UNEXPECTED_END_SEQ: &str = "unexpected end sequence token";
    /// `ErrorMsg::UNEXPECTED_END_MAP`
    pub const UNEXPECTED_END_MAP: &str = "unexpected end map token";
    /// `ErrorMsg::SINGLE_QUOTED_CHAR`
    pub const SINGLE_QUOTED_CHAR: &str = "invalid character in single-quoted string";
    /// `ErrorMsg::INVALID_ANCHOR`
    pub const INVALID_ANCHOR: &str = "invalid anchor";
    /// `ErrorMsg::INVALID_ALIAS`
    pub const INVALID_ALIAS: &str = "invalid alias";
    /// `ErrorMsg::INVALID_TAG`
    pub const INVALID_TAG: &str = "invalid tag";
    /// `ErrorMsg::BAD_FILE`
    pub const BAD_FILE: &str = "bad file";
}

/// A key as yaml-cpp's key-dependent messages print it: the `ErrorMsg::*_WITH_KEY` overloads
/// (exceptions.h:93-158) and `key_to_string` (node/impl.h:318-321) print a string or a number
/// and leave out any other key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyText {
    /// A `std::string` or C-string key, printed as its bytes.
    Text(Vec<u8>),
    /// A key of another type: not printed.
    Unprintable,
}

impl KeyText {
    /// A numeric key, printed by `std::stringstream << key` (an integer in decimal).
    pub fn number(n: impl fmt::Display) -> KeyText {
        KeyText::Text(n.to_string().into_bytes())
    }
}

/// `ErrorMsg::KEY_NOT_FOUND_WITH_KEY` (exceptions.h:93-121).
pub fn key_not_found_with_key(key: &KeyText) -> Vec<u8> {
    match key {
        KeyText::Text(k) => [error_msg::KEY_NOT_FOUND.as_bytes(), b": ", k].concat(),
        KeyText::Unprintable => error_msg::KEY_NOT_FOUND.as_bytes().to_vec(),
    }
}

/// `ErrorMsg::BAD_SUBSCRIPT_WITH_KEY` (exceptions.h:123-149).
pub fn bad_subscript_with_key(key: &KeyText) -> Vec<u8> {
    match key {
        KeyText::Text(k) => [error_msg::BAD_SUBSCRIPT.as_bytes(), b" (key: \"", k, b"\")"].concat(),
        KeyText::Unprintable => error_msg::BAD_SUBSCRIPT.as_bytes().to_vec(),
    }
}

/// `ErrorMsg::INVALID_NODE_WITH_KEY` (exceptions.h:151-158).
pub fn invalid_node_with_key(key: &[u8]) -> Vec<u8> {
    if key.is_empty() {
        return error_msg::INVALID_NODE.as_bytes().to_vec();
    }
    [b"invalid node; first invalid key: \"", key, b"\""].concat()
}

/// The C++ class of a yaml-cpp exception (exceptions.h:162-301, depthguard.h:20-34).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExceptionType {
    /// `ParserException`
    Parser,
    /// `DeepRecursion`, a `ParserException`, with the depth it was thrown at.
    DeepRecursion { depth: i32 },
    /// `InvalidScalar`, a `RepresentationException`.
    InvalidScalar,
    /// `KeyNotFound`, a `RepresentationException`.
    KeyNotFound,
    /// `InvalidNode`, a `RepresentationException`.
    InvalidNode,
    /// `BadConversion` (thrown as `TypedBadConversion<T>`), a `RepresentationException`.
    BadConversion,
    /// `BadDereference`, a `RepresentationException`.
    BadDereference,
    /// `BadSubscript`, a `RepresentationException`.
    BadSubscript,
    /// `BadPushback`, a `RepresentationException`.
    BadPushback,
    /// `BadInsert`, a `RepresentationException`.
    BadInsert,
    /// `EmitterException`
    Emitter,
    /// `BadFile`
    BadFile,
    /// `std::bad_alloc`, which yaml-cpp lets through when memory runs out: the port's parser
    /// when it can't have the thread it parses a document on.
    BadAlloc,
}

/// Port of `YAML::Exception` and its subclasses (exceptions.h:162-301): the mark, the message
/// (`msg`) and the class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exception {
    pub kind: ExceptionType,
    pub mark: Mark,
    pub msg: Vec<u8>,
}

impl Exception {
    /// `Exception(mark, msg)` of the given class.
    pub fn new(kind: ExceptionType, mark: Mark, msg: impl Into<Vec<u8>>) -> Exception {
        Exception {
            kind,
            mark,
            msg: msg.into(),
        }
    }

    /// `ParserException(mark, msg)` (exceptions.h:182-188).
    pub fn parser(mark: Mark, msg: impl Into<Vec<u8>>) -> Exception {
        Exception::new(ExceptionType::Parser, mark, msg)
    }

    /// `DeepRecursion(depth, mark, msg)` (depthguard.h:20-34, depthguard.cpp:5-7).
    pub fn deep_recursion(depth: i32, mark: Mark, msg: impl Into<Vec<u8>>) -> Exception {
        Exception::new(ExceptionType::DeepRecursion { depth }, mark, msg)
    }

    /// `InvalidNode(key)` (exceptions.h:229-236): the null mark.
    pub fn invalid_node(key: &[u8]) -> Exception {
        Exception::new(
            ExceptionType::InvalidNode,
            Mark::null_mark(),
            invalid_node_with_key(key),
        )
    }

    /// `TypedBadConversion<T>(mark)` (exceptions.h:238-251).
    pub fn bad_conversion(mark: Mark) -> Exception {
        Exception::new(
            ExceptionType::BadConversion,
            mark,
            error_msg::BAD_CONVERSION,
        )
    }

    /// `BadSubscript(mark, key)` (exceptions.h:261-268).
    pub fn bad_subscript(mark: Mark, key: &KeyText) -> Exception {
        Exception::new(
            ExceptionType::BadSubscript,
            mark,
            bad_subscript_with_key(key),
        )
    }

    /// A `std::bad_alloc`: its `what()` is the C++ library's text, with no mark.
    pub fn bad_alloc() -> Exception {
        let what = ocio_ops::exception::Exception::bad_alloc().what().to_vec();
        Exception::new(ExceptionType::BadAlloc, Mark::null_mark(), what)
    }

    /// Whether the exception is a `ParserException` (a `DeepRecursion` is one).
    pub fn is_parser_exception(&self) -> bool {
        matches!(
            self.kind,
            ExceptionType::Parser | ExceptionType::DeepRecursion { .. }
        )
    }

    /// Whether the exception is a `RepresentationException`.
    pub fn is_representation_exception(&self) -> bool {
        matches!(
            self.kind,
            ExceptionType::InvalidScalar
                | ExceptionType::KeyNotFound
                | ExceptionType::InvalidNode
                | ExceptionType::BadConversion
                | ExceptionType::BadDereference
                | ExceptionType::BadSubscript
                | ExceptionType::BadPushback
                | ExceptionType::BadInsert
        )
    }

    /// `what()`: `Exception::build_what` (exceptions.h:173-181). With a null mark, the
    /// message alone; otherwise "yaml-cpp: error at line L, column C: " and the message, the
    /// line and column counted from 1 (`mark.line + 1`, an `int` addition).
    pub fn what(&self) -> Vec<u8> {
        if self.mark.is_null() {
            return self.msg.clone();
        }
        let mut out = format!(
            "yaml-cpp: error at line {}, column {}: ",
            self.mark.line.wrapping_add(1),
            self.mark.column.wrapping_add(1)
        )
        .into_bytes();
        out.extend_from_slice(&self.msg);
        out
    }
}

impl fmt::Display for Exception {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&String::from_utf8_lossy(&self.what()))
    }
}

impl std::error::Error for Exception {}

/// A yaml-cpp result.
pub type Result<T> = std::result::Result<T, Exception>;
