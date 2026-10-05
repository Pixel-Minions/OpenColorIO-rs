// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.8.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! Port of yaml-cpp 0.8.0 `test/integration/load_node_test.cpp`: the tests that read loaded
//! nodes. Not ported: those of the APIs the port leaves out (`Reassign` and
//! `ForceInsertIntoMap` mutate nodes; `CloneScalar`, `CloneSeq`, `CloneMap`, `CloneAlias`
//! need `Clone`; `Binary` and `BinaryWithWhitespaces` need `Binary`; `EmitEmptyNode` and
//! `SpecialFlow` emit nodes), and `ForEach` and `ForEachMap`, which need Boost.
//! `FallbackValues` and `NumericConversion` read floating-point numbers and come with them.

use super::load;
use crate::yaml_cpp::convert::CChar;
use crate::yaml_cpp::event_handler::EmitterStyle;
use crate::yaml_cpp::exceptions::{ExceptionType, Result, error_msg};
use crate::yaml_cpp::handler_test::Bytes;
use crate::yaml_cpp::node::Node;

/// `EXPECT_THROW(statement, InvalidNode)`.
#[track_caller]
fn expect_invalid_node<T: std::fmt::Debug>(result: Result<T>) {
    match result {
        Err(e) => assert_eq!(e.kind, ExceptionType::InvalidNode, "{e:?}"),
        Ok(v) => panic!("no exception: {v:?}"),
    }
}

/// The `try { Load(input); FAIL() } catch (const ParserException& e)` loops of
/// `IncompleteJson` and `IncorrectFlow`.
#[track_caller]
fn expect_parser_exceptions(tests: &[(&str, &[u8], &str)]) {
    for &(name, input, expected_exception) in tests {
        match load(input) {
            Ok(_) => panic!(
                "Expected exception {expected_exception} for {name}, input: {}",
                input.escape_ascii()
            ),
            Err(e) => {
                assert!(e.is_parser_exception(), "{name}: {e:?}");
                assert_eq!(
                    Bytes::from(expected_exception.as_bytes()),
                    Bytes(e.msg),
                    "{name}"
                );
            }
        }
    }
}

/// Port of yaml-cpp 0.8.0 `TEST(LoadNodeTest, IterateSequence)`.
#[test]
fn iterate_sequence() -> Result<()> {
    let node = load(b"[1, 3, 5, 7]")?;
    let seq = [1, 3, 5, 7];
    let mut i = 0;
    for it in node.iter() {
        assert!(i < 4);
        let x = seq[i];
        i += 1;
        assert_eq!(x, it.node.as_::<i32>()?);
    }
    assert_eq!(4, i);
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(LoadNodeTest, IterateMap)`.
#[test]
fn iterate_map() -> Result<()> {
    let node = load(b"{a: A, b: B, c: C}")?;
    let mut i = 0;
    for it in node.iter() {
        assert!(i < 3);
        i += 1;
        assert_eq!(
            i32::from(it.second.as_::<CChar>()?.0),
            i32::from(it.first.as_::<CChar>()?.0) + i32::from(b'A') - i32::from(b'a')
        );
    }
    assert_eq!(3, i);
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(LoadNodeTest, ResetNode)`.
#[test]
fn reset_node() -> Result<()> {
    let mut node = load(b"[1, 2, 3]")?;
    assert!(!node.is_null()?);
    let other = node.clone();
    node.reset(&Node::new())?;
    assert!(node.is_null()?);
    assert!(!other.is_null()?);
    node.reset(&other)?;
    assert!(!node.is_null()?);
    assert_eq!(node, other);
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(LoadNodeTest, EmptyString)`.
#[test]
fn empty_string() -> Result<()> {
    let node = load(b"\"\"")?;
    assert!(!node.is_null()?);
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(LoadNodeTest, DereferenceIteratorError)`.
#[test]
fn dereference_iterator_error() -> Result<()> {
    let node = load(b"[{a: b}, 1, 2]")?;
    expect_invalid_node(node.iter().next().unwrap().first.as_::<i32>());
    assert!(node.iter().next().unwrap().node.is_map()?);
    assert!(node.iter().next().unwrap().node.is_map()?);
    expect_invalid_node(
        node.iter()
            .next()
            .unwrap()
            .node
            .iter()
            .next()
            .unwrap()
            .node
            .node_type(),
    );
    expect_invalid_node(
        node.iter()
            .next()
            .unwrap()
            .node
            .iter()
            .next()
            .unwrap()
            .node
            .node_type(),
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeTest, ParseNodeStyle)`.
#[test]
fn parse_node_style() -> Result<()> {
    assert_eq!(EmitterStyle::Flow, load(b"[1, 2, 3]")?.style()?);
    assert_eq!(EmitterStyle::Flow, load(b"{foo: bar}")?.style()?);
    assert_eq!(EmitterStyle::Block, load(b"- foo\n- bar")?.style()?);
    assert_eq!(EmitterStyle::Block, load(b"foo: bar")?.style()?);
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeTest, IncompleteJson)`.
#[test]
fn incomplete_json() {
    expect_parser_exceptions(&[
        (
            "JSON map without value",
            b"{\"access\"",
            error_msg::END_OF_MAP_FLOW,
        ),
        (
            "JSON map with colon but no value",
            b"{\"access\":",
            error_msg::END_OF_MAP_FLOW,
        ),
        (
            "JSON map with unclosed value quote",
            b"{\"access\":\"",
            error_msg::END_OF_MAP_FLOW,
        ),
        (
            "JSON map without end brace",
            b"{\"access\":\"abc\"",
            error_msg::END_OF_MAP_FLOW,
        ),
    ]);
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeTest, IncorrectFlow)`.
#[test]
fn incorrect_flow() {
    expect_parser_exceptions(&[
        ("Incorrect yaml: \"{:]\"", b"{:]", error_msg::FLOW_END),
        ("Incorrect yaml: \"[:}\"", b"[:}", error_msg::FLOW_END),
    ]);
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeTest, LoadTildeAsNull)`.
#[test]
fn load_tilde_as_null() -> Result<()> {
    let node = load(b"~")?;
    assert!(node.is_null()?);
    assert_eq!(Bytes::from(node.as_::<Vec<u8>>()?), Bytes::from(b"null"));
    assert_eq!(
        Bytes::from(node.as_or::<Vec<u8>>(b"~".to_vec())?),
        Bytes::from(b"null")
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeTest, LoadNullWithStrTag)`.
#[test]
fn load_null_with_str_tag() -> Result<()> {
    let node = load(b"!!str null")?;
    assert_eq!(
        Bytes::from(node.tag()?),
        Bytes::from(b"tag:yaml.org,2002:str")
    );
    assert_eq!(Bytes::from(node.as_::<Vec<u8>>()?), Bytes::from(b"null"));
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeTest, LoadQuotedNull)`.
#[test]
fn load_quoted_null() -> Result<()> {
    let node = load(b"\"null\"")?;
    assert_eq!(Bytes::from(node.as_::<Vec<u8>>()?), Bytes::from(b"null"));
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeTest, LoadTagWithParenthesis)`.
#[test]
fn load_tag_with_parenthesis() -> Result<()> {
    let node = load(b"!Complex(Tag) foo")?;
    assert_eq!(Bytes::from(node.tag()?), Bytes::from(b"!Complex(Tag)"));
    assert_eq!(Bytes::from(node.as_::<Vec<u8>>()?), Bytes::from(b"foo"));
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeTest, LoadTagWithNullScalar)`.
#[test]
fn load_tag_with_null_scalar() -> Result<()> {
    let node = load(b"!2")?;
    assert!(node.is_null()?);
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(LoadNodeTest, BlockCRNLEncoded)`.
#[test]
fn block_crnl_encoded() -> Result<()> {
    let node = load(
        b"blockText: |\r\n  some arbitrary text \r\n  spanning some \r\n  lines, that are split \r\n  \
          by CR and NL\r\nfollowup: 1",
    )?;
    assert_eq!(
        Bytes::from(
            b"some arbitrary text \nspanning some \nlines, that are split \nby CR and NL\n"
        ),
        Bytes::from(node.get(b"blockText")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(1, node.get(b"followup")?.as_::<i32>()?);
    Ok(())
}
