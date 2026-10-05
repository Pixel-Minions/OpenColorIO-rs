// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.8.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! Port of yaml-cpp 0.8.0 `test/integration/error_messages_test.cpp`. A `std::vector<int>`
//! key, which no message prints, is `Key::StrSeq(&[])` here (the port's keys that aren't
//! strings or numbers); the `Node()` key of `BadSubscriptErrorMessage` is left out, as the
//! port has no node keys.

use super::Key;
use crate::yaml_cpp::exceptions::{ExceptionType, Result};
use crate::yaml_cpp::handler_test::Bytes;
use crate::yaml_cpp::parse::load;

/// `EXPECT_THROW_EXCEPTION(exception_type, statement, message)`.
#[track_caller]
fn expect_throw_exception<T: std::fmt::Debug>(
    kind: ExceptionType,
    result: Result<T>,
    message: &str,
) {
    match result {
        Err(e) => {
            assert_eq!(e.kind, kind, "{e:?}");
            assert_eq!(Bytes(e.msg), Bytes::from(message.as_bytes()));
        }
        Ok(v) => panic!("no exception: {v:?}"),
    }
}

/// Port of yaml-cpp 0.8.0 `TEST(ErrorMessageTest, BadSubscriptErrorMessage)`.
#[test]
fn bad_subscript_error_message() -> Result<()> {
    let example_yaml = b"first:\n   second: 1\n   third: 2\n";
    let doc = load(example_yaml)?;

    // Test that printable key is part of error message
    expect_throw_exception(
        ExceptionType::BadSubscript,
        doc.get(b"first")?.get(b"second")?.get(b"fourth"),
        "operator[] call on a scalar (key: \"fourth\")",
    );

    expect_throw_exception(
        ExceptionType::BadSubscript,
        doc.get(b"first")?.get(b"second")?.get(37),
        "operator[] call on a scalar (key: \"37\")",
    );

    // Non-printable key is not included in error message
    expect_throw_exception(
        ExceptionType::BadSubscript,
        doc.get(b"first")?.get(b"second")?.get(Key::StrSeq(&[])),
        "operator[] call on a scalar",
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(ErrorMessageTest, Ex9_1_InvalidNodeErrorMessage)`.
#[test]
fn ex9_1_invalid_node_error_message() -> Result<()> {
    let example_yaml = b"first:\n   second: 1\n   third: 2\n";
    let doc = load(example_yaml)?;

    // Test that printable key is part of error message
    expect_throw_exception(
        ExceptionType::InvalidNode,
        doc.get(b"first")?.get(b"fourth")?.as_::<i32>(),
        "invalid node; first invalid key: \"fourth\"",
    );

    expect_throw_exception(
        ExceptionType::InvalidNode,
        doc.get(b"first")?.get(37)?.as_::<i32>(),
        "invalid node; first invalid key: \"37\"",
    );

    // Non-printable key is not included in error message
    expect_throw_exception(
        ExceptionType::InvalidNode,
        doc.get(b"first")?.get(Key::StrSeq(&[]))?.as_::<i32>(),
        "invalid node; this may result from using a map iterator as a sequence iterator, or \
         vice-versa",
    );
    Ok(())
}
