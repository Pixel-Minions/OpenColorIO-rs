// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.8.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! Port of yaml-cpp 0.8.0 `test/integration/handler_test.cpp`.

use super::super::event_handler::EmitterStyle;
use super::super::exceptions::error_msg;
use super::super::handler_test::*;

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerTest, NoEndOfMapFlow)`.
#[test]
fn no_end_of_map_flow() {
    expect_parser_exception(b"---{header: {id: 1", error_msg::END_OF_MAP_FLOW.as_bytes());
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerTest, PlainScalarStartingWithQuestionMark)`.
#[test]
fn plain_scalar_starting_with_question_mark() {
    expect_events(
        b"foo: ?bar",
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"foo"),
            scalar(b"?", 0, b"?bar"),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerTest, NullStringScalar)`.
#[test]
fn null_string_scalar() {
    expect_events(
        b"foo: null",
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"foo"),
            null(0),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerTest, CommentOnNewlineOfMapValueWithNoSpaces)`.
#[test]
fn comment_on_newline_of_map_value_with_no_spaces() {
    expect_events(
        b"key: value\n# comment",
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"key"),
            scalar(b"?", 0, b"value"),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerTest, CommentOnNewlineOfMapValueWithOneSpace)`.
#[test]
fn comment_on_newline_of_map_value_with_one_space() {
    expect_events(
        b"key: value\n # comment",
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"key"),
            scalar(b"?", 0, b"value"),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerTest, CommentOnNewlineOfMapValueWithManySpace)`.
#[test]
fn comment_on_newline_of_map_value_with_many_space() {
    expect_events(
        b"key: value\n    # comment",
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"key"),
            scalar(b"?", 0, b"value"),
            map_end(),
            doc_end(),
        ],
    );
}
