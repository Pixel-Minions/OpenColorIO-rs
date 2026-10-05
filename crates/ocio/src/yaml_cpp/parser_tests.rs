// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.8.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! Port of yaml-cpp 0.8.0 `test/parser_test.cpp`.

use super::Parser;
use crate::yaml_cpp::exceptions::ExceptionType;
use crate::yaml_cpp::handler_test::RecordingHandler;

/// `EXPECT_THROW(parser.HandleNextDocument(handler), YAML::DeepRecursion)` on `input`.
#[track_caller]
fn expect_deep_recursion(input: &[u8]) {
    let mut parser = Parser::new(input);
    let mut handler = RecordingHandler::default();
    match parser.handle_next_document(&mut handler) {
        Err(e) => assert!(
            matches!(e.kind, ExceptionType::DeepRecursion { .. }),
            "not a DeepRecursion: {e:?}"
        ),
        Ok(more) => panic!("no exception (more documents: {more})"),
    }
}

/// Port of yaml-cpp 0.8.0 `TEST(ParserTest, Empty)`.
#[test]
fn empty() {
    let mut parser = Parser::empty();
    assert!(!parser.is_valid().unwrap());

    let mut handler = RecordingHandler::default();
    assert!(!parser.handle_next_document(&mut handler).unwrap());
    assert!(handler.events.is_empty());
}

/// Port of yaml-cpp 0.8.0 `TEST(ParserTest, CVE_2017_5950)`.
#[test]
fn cve_2017_5950() {
    expect_deep_recursion(&[b'['; 16384]);
}

/// Port of yaml-cpp 0.8.0 `TEST(ParserTest, CVE_2018_20573)`.
#[test]
fn cve_2018_20573() {
    expect_deep_recursion(&[b'{'; 20535]);
}

/// Port of yaml-cpp 0.8.0 `TEST(ParserTest, CVE_2018_20574)`.
#[test]
fn cve_2018_20574() {
    expect_deep_recursion(&[b'{'; 21989]);
}

/// Port of yaml-cpp 0.8.0 `TEST(ParserTest, CVE_2019_6285)`.
#[test]
fn cve_2019_6285() {
    let mut excessive_recursion = vec![b'['; 23100];
    excessive_recursion.push(b'f');
    expect_deep_recursion(&excessive_recursion);
}
