// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.8.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! Port of yaml-cpp 0.8.0 `test/integration/node_spec_test.cpp`: each specification example
//! loaded and read through the node API, with the expectations copied from the test. Its four
//! `DISABLED_` tests are disabled upstream and are not ported. A C++ exception fails a test;
//! here a test returns it.

use super::Key;
use crate::yaml_cpp::convert::CChar;
use crate::yaml_cpp::exceptions::{Result, error_msg};
use crate::yaml_cpp::handler_test::Bytes;
use crate::yaml_cpp::parse::{load, load_all};
use crate::yaml_cpp::spec_examples::*;

/// `EXPECT_THROW_PARSER_EXCEPTION(Load(example), message)`: a `ParserException` with this
/// `msg`.
#[track_caller]
fn expect_load_parser_exception(example: &[u8], message: impl AsRef<[u8]>) {
    match load(example) {
        Ok(_) => panic!("expected a ParserException"),
        Err(e) => {
            assert!(e.is_parser_exception(), "not a ParserException: {e:?}");
            assert_eq!(Bytes(e.msg), Bytes::from(message.as_ref()));
        }
    }
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex2_1_SeqScalars)`.
#[test]
fn ex2_1_seq_scalars() -> Result<()> {
    let doc = load(EX2_1)?;
    assert!(doc.is_sequence()?);
    assert_eq!(3, doc.size()?);
    assert_eq!(
        Bytes::from(b"Mark McGwire"),
        Bytes::from(doc.get(0)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"Sammy Sosa"),
        Bytes::from(doc.get(1)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"Ken Griffey"),
        Bytes::from(doc.get(2)?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex2_2_MappingScalarsToScalars)`.
#[test]
fn ex2_2_mapping_scalars_to_scalars() -> Result<()> {
    let doc = load(EX2_2)?;
    assert!(doc.is_map()?);
    assert_eq!(3, doc.size()?);
    assert_eq!(
        Bytes::from(b"65"),
        Bytes::from(doc.get(b"hr")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"0.278"),
        Bytes::from(doc.get(b"avg")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"147"),
        Bytes::from(doc.get(b"rbi")?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex2_3_MappingScalarsToSequences)`.
#[test]
fn ex2_3_mapping_scalars_to_sequences() -> Result<()> {
    let doc = load(EX2_3)?;
    assert!(doc.is_map()?);
    assert_eq!(2, doc.size()?);
    assert_eq!(3, doc.get(b"american")?.size()?);
    assert_eq!(
        Bytes::from(b"Boston Red Sox"),
        Bytes::from(doc.get(b"american")?.get(0)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"Detroit Tigers"),
        Bytes::from(doc.get(b"american")?.get(1)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"New York Yankees"),
        Bytes::from(doc.get(b"american")?.get(2)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(3, doc.get(b"national")?.size()?);
    assert_eq!(
        Bytes::from(b"New York Mets"),
        Bytes::from(doc.get(b"national")?.get(0)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"Chicago Cubs"),
        Bytes::from(doc.get(b"national")?.get(1)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"Atlanta Braves"),
        Bytes::from(doc.get(b"national")?.get(2)?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex2_4_SequenceOfMappings)`.
#[test]
fn ex2_4_sequence_of_mappings() -> Result<()> {
    let doc = load(EX2_4)?;
    assert_eq!(2, doc.size()?);
    assert_eq!(3, doc.get(0)?.size()?);
    assert_eq!(
        Bytes::from(b"Mark McGwire"),
        Bytes::from(doc.get(0)?.get(b"name")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"65"),
        Bytes::from(doc.get(0)?.get(b"hr")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"0.278"),
        Bytes::from(doc.get(0)?.get(b"avg")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(3, doc.get(1)?.size()?);
    assert_eq!(
        Bytes::from(b"Sammy Sosa"),
        Bytes::from(doc.get(1)?.get(b"name")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"63"),
        Bytes::from(doc.get(1)?.get(b"hr")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"0.288"),
        Bytes::from(doc.get(1)?.get(b"avg")?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex2_5_SequenceOfSequences)`.
#[test]
fn ex2_5_sequence_of_sequences() -> Result<()> {
    let doc = load(EX2_5)?;
    assert_eq!(3, doc.size()?);
    assert_eq!(3, doc.get(0)?.size()?);
    assert_eq!(
        Bytes::from(b"name"),
        Bytes::from(doc.get(0)?.get(0)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"hr"),
        Bytes::from(doc.get(0)?.get(1)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"avg"),
        Bytes::from(doc.get(0)?.get(2)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(3, doc.get(1)?.size()?);
    assert_eq!(
        Bytes::from(b"Mark McGwire"),
        Bytes::from(doc.get(1)?.get(0)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"65"),
        Bytes::from(doc.get(1)?.get(1)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"0.278"),
        Bytes::from(doc.get(1)?.get(2)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(3, doc.get(2)?.size()?);
    assert_eq!(
        Bytes::from(b"Sammy Sosa"),
        Bytes::from(doc.get(2)?.get(0)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"63"),
        Bytes::from(doc.get(2)?.get(1)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"0.288"),
        Bytes::from(doc.get(2)?.get(2)?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex2_6_MappingOfMappings)`.
#[test]
fn ex2_6_mapping_of_mappings() -> Result<()> {
    let doc = load(EX2_6)?;
    assert_eq!(2, doc.size()?);
    assert_eq!(2, doc.get(b"Mark McGwire")?.size()?);
    assert_eq!(
        Bytes::from(b"65"),
        Bytes::from(doc.get(b"Mark McGwire")?.get(b"hr")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"0.278"),
        Bytes::from(doc.get(b"Mark McGwire")?.get(b"avg")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(2, doc.get(b"Sammy Sosa")?.size()?);
    assert_eq!(
        Bytes::from(b"63"),
        Bytes::from(doc.get(b"Sammy Sosa")?.get(b"hr")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"0.288"),
        Bytes::from(doc.get(b"Sammy Sosa")?.get(b"avg")?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex2_7_TwoDocumentsInAStream)`.
#[test]
fn ex2_7_two_documents_in_a_stream() -> Result<()> {
    let docs = load_all(EX2_7)?;
    assert_eq!(2, docs.len());
    {
        let doc = docs[0].clone();
        assert_eq!(3, doc.size()?);
        assert_eq!(
            Bytes::from(b"Mark McGwire"),
            Bytes::from(doc.get(0)?.as_::<Vec<u8>>()?)
        );
        assert_eq!(
            Bytes::from(b"Sammy Sosa"),
            Bytes::from(doc.get(1)?.as_::<Vec<u8>>()?)
        );
        assert_eq!(
            Bytes::from(b"Ken Griffey"),
            Bytes::from(doc.get(2)?.as_::<Vec<u8>>()?)
        );
    }
    {
        let doc = docs[1].clone();
        assert_eq!(2, doc.size()?);
        assert_eq!(
            Bytes::from(b"Chicago Cubs"),
            Bytes::from(doc.get(0)?.as_::<Vec<u8>>()?)
        );
        assert_eq!(
            Bytes::from(b"St Louis Cardinals"),
            Bytes::from(doc.get(1)?.as_::<Vec<u8>>()?)
        );
    }
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex2_8_PlayByPlayFeed)`.
#[test]
fn ex2_8_play_by_play_feed() -> Result<()> {
    let docs = load_all(EX2_8)?;
    assert_eq!(2, docs.len());
    {
        let doc = docs[0].clone();
        assert_eq!(3, doc.size()?);
        assert_eq!(
            Bytes::from(b"20:03:20"),
            Bytes::from(doc.get(b"time")?.as_::<Vec<u8>>()?)
        );
        assert_eq!(
            Bytes::from(b"Sammy Sosa"),
            Bytes::from(doc.get(b"player")?.as_::<Vec<u8>>()?)
        );
        assert_eq!(
            Bytes::from(b"strike (miss)"),
            Bytes::from(doc.get(b"action")?.as_::<Vec<u8>>()?)
        );
    }
    {
        let doc = docs[1].clone();
        assert_eq!(3, doc.size()?);
        assert_eq!(
            Bytes::from(b"20:03:47"),
            Bytes::from(doc.get(b"time")?.as_::<Vec<u8>>()?)
        );
        assert_eq!(
            Bytes::from(b"Sammy Sosa"),
            Bytes::from(doc.get(b"player")?.as_::<Vec<u8>>()?)
        );
        assert_eq!(
            Bytes::from(b"grand slam"),
            Bytes::from(doc.get(b"action")?.as_::<Vec<u8>>()?)
        );
    }
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex2_9_SingleDocumentWithTwoComments)`.
#[test]
fn ex2_9_single_document_with_two_comments() -> Result<()> {
    let doc = load(EX2_9)?;
    assert_eq!(2, doc.size()?);
    assert_eq!(2, doc.get(b"hr")?.size()?);
    assert_eq!(
        Bytes::from(b"Mark McGwire"),
        Bytes::from(doc.get(b"hr")?.get(0)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"Sammy Sosa"),
        Bytes::from(doc.get(b"hr")?.get(1)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(2, doc.get(b"rbi")?.size()?);
    assert_eq!(
        Bytes::from(b"Sammy Sosa"),
        Bytes::from(doc.get(b"rbi")?.get(0)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"Ken Griffey"),
        Bytes::from(doc.get(b"rbi")?.get(1)?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex2_10_SimpleAnchor)`.
#[test]
fn ex2_10_simple_anchor() -> Result<()> {
    let doc = load(EX2_10)?;
    assert_eq!(2, doc.size()?);
    assert_eq!(2, doc.get(b"hr")?.size()?);
    assert_eq!(
        Bytes::from(b"Mark McGwire"),
        Bytes::from(doc.get(b"hr")?.get(0)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"Sammy Sosa"),
        Bytes::from(doc.get(b"hr")?.get(1)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(2, doc.get(b"rbi")?.size()?);
    assert_eq!(
        Bytes::from(b"Sammy Sosa"),
        Bytes::from(doc.get(b"rbi")?.get(0)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"Ken Griffey"),
        Bytes::from(doc.get(b"rbi")?.get(1)?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex2_11_MappingBetweenSequences)`.
#[test]
fn ex2_11_mapping_between_sequences() -> Result<()> {
    let doc = load(EX2_11)?;

    let tigers_cubs: &[&[u8]] = &[b"Detroit Tigers", b"Chicago cubs"];

    let yankees_braves: &[&[u8]] = &[b"New York Yankees", b"Atlanta Braves"];

    assert_eq!(2, doc.size()?);
    assert_eq!(1, doc.get(Key::StrSeq(tigers_cubs))?.size()?);
    assert_eq!(
        Bytes::from(b"2001-07-23"),
        Bytes::from(
            doc.get(Key::StrSeq(tigers_cubs))?
                .get(0)?
                .as_::<Vec<u8>>()?
        )
    );
    assert_eq!(3, doc.get(Key::StrSeq(yankees_braves))?.size()?);
    assert_eq!(
        Bytes::from(b"2001-07-02"),
        Bytes::from(
            doc.get(Key::StrSeq(yankees_braves))?
                .get(0)?
                .as_::<Vec<u8>>()?
        )
    );
    assert_eq!(
        Bytes::from(b"2001-08-12"),
        Bytes::from(
            doc.get(Key::StrSeq(yankees_braves))?
                .get(1)?
                .as_::<Vec<u8>>()?
        )
    );
    assert_eq!(
        Bytes::from(b"2001-08-14"),
        Bytes::from(
            doc.get(Key::StrSeq(yankees_braves))?
                .get(2)?
                .as_::<Vec<u8>>()?
        )
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex2_12_CompactNestedMapping)`.
#[test]
fn ex2_12_compact_nested_mapping() -> Result<()> {
    let doc = load(EX2_12)?;
    assert_eq!(3, doc.size()?);
    assert_eq!(2, doc.get(0)?.size()?);
    assert_eq!(
        Bytes::from(b"Super Hoop"),
        Bytes::from(doc.get(0)?.get(b"item")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(1, doc.get(0)?.get(b"quantity")?.as_::<i32>()?);
    assert_eq!(2, doc.get(1)?.size()?);
    assert_eq!(
        Bytes::from(b"Basketball"),
        Bytes::from(doc.get(1)?.get(b"item")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(4, doc.get(1)?.get(b"quantity")?.as_::<i32>()?);
    assert_eq!(2, doc.get(2)?.size()?);
    assert_eq!(
        Bytes::from(b"Big Shoes"),
        Bytes::from(doc.get(2)?.get(b"item")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(1, doc.get(2)?.get(b"quantity")?.as_::<i32>()?);
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex2_13_InLiteralsNewlinesArePreserved)`.
#[test]
fn ex2_13_in_literals_newlines_are_preserved() -> Result<()> {
    let doc = load(EX2_13)?;
    assert!(Bytes::from(doc.as_::<Vec<u8>>()?) == Bytes::from(b"\\//||\\/||\n// ||  ||__"));
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex2_14_InFoldedScalarsNewlinesBecomeSpaces)`.
#[test]
fn ex2_14_in_folded_scalars_newlines_become_spaces() -> Result<()> {
    let doc = load(EX2_14)?;
    assert!(
        Bytes::from(doc.as_::<Vec<u8>>()?)
            == Bytes::from(b"Mark McGwire's year was crippled by a knee injury.")
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex2_15_FoldedNewlinesArePreservedForMoreIndentedAndBlankLines)`.
#[test]
fn ex2_15_folded_newlines_are_preserved_for_more_indented_and_blank_lines() -> Result<()> {
    let doc = load(EX2_15)?;
    assert!(Bytes::from(doc.as_::<Vec<u8>>()?) == Bytes::from(b"Sammy Sosa completed another fine season with great stats.\n\n  63 Home Runs\n  0.288 Batting Average\n\nWhat a year!"));
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex2_16_IndentationDeterminesScope)`.
#[test]
fn ex2_16_indentation_determines_scope() -> Result<()> {
    let doc = load(EX2_16)?;
    assert_eq!(3, doc.size()?);
    assert_eq!(
        Bytes::from(b"Mark McGwire"),
        Bytes::from(doc.get(b"name")?.as_::<Vec<u8>>()?)
    );
    assert!(
        Bytes::from(doc.get(b"accomplishment")?.as_::<Vec<u8>>()?)
            == Bytes::from(b"Mark set a major league home run record in 1998.\n")
    );
    assert!(
        Bytes::from(doc.get(b"stats")?.as_::<Vec<u8>>()?)
            == Bytes::from(b"65 Home Runs\n0.278 Batting Average\n")
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex2_17_QuotedScalars)`.
#[test]
fn ex2_17_quoted_scalars() -> Result<()> {
    let doc = load(EX2_17)?;
    assert_eq!(6, doc.size()?);
    assert_eq!(
        Bytes::from(b"Sosa did fine.\xE2\x98\xBA"),
        Bytes::from(doc.get(b"unicode")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"\x081998\t1999\t2000\n"),
        Bytes::from(doc.get(b"control")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"\r\n is \r\n"),
        Bytes::from(doc.get(b"hex esc")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"\"Howdy!\" he cried."),
        Bytes::from(doc.get(b"single")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b" # Not a 'comment'."),
        Bytes::from(doc.get(b"quoted")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"|\\-*-/|"),
        Bytes::from(doc.get(b"tie-fighter")?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex2_18_MultiLineFlowScalars)`.
#[test]
fn ex2_18_multi_line_flow_scalars() -> Result<()> {
    let doc = load(EX2_18)?;
    assert_eq!(2, doc.size()?);
    assert!(
        Bytes::from(doc.get(b"plain")?.as_::<Vec<u8>>()?)
            == Bytes::from(b"This unquoted scalar spans many lines.")
    );
    assert!(
        Bytes::from(doc.get(b"quoted")?.as_::<Vec<u8>>()?)
            == Bytes::from(b"So does this quoted scalar.\n")
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex2_23_VariousExplicitTags)`.
#[test]
fn ex2_23_various_explicit_tags() -> Result<()> {
    let doc = load(EX2_23)?;
    assert_eq!(3, doc.size()?);
    assert_eq!(
        Bytes::from(b"tag:yaml.org,2002:str"),
        Bytes::from(doc.get(b"not-date")?.tag()?)
    );
    assert_eq!(
        Bytes::from(b"2002-04-28"),
        Bytes::from(doc.get(b"not-date")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"tag:yaml.org,2002:binary"),
        Bytes::from(doc.get(b"picture")?.tag()?)
    );
    assert!(Bytes::from(doc.get(b"picture")?.as_::<Vec<u8>>()?) == Bytes::from(b"R0lGODlhDAAMAIQAAP//9/X\n17unp5WZmZgAAAOfn515eXv\nPz7Y6OjuDg4J+fn5OTk6enp\n56enmleECcgggoBADs=\n"));
    assert_eq!(
        Bytes::from(b"!something"),
        Bytes::from(doc.get(b"application specific tag")?.tag()?)
    );
    assert!(
        Bytes::from(doc.get(b"application specific tag")?.as_::<Vec<u8>>()?)
            == Bytes::from(
                b"The semantics of the tag\nabove may be different for\ndifferent documents."
            )
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex2_24_GlobalTags)`.
#[test]
fn ex2_24_global_tags() -> Result<()> {
    let doc = load(EX2_24)?;
    assert_eq!(
        Bytes::from(b"tag:clarkevans.com,2002:shape"),
        Bytes::from(doc.tag()?)
    );
    assert_eq!(3, doc.size()?);
    assert_eq!(
        Bytes::from(b"tag:clarkevans.com,2002:circle"),
        Bytes::from(doc.get(0)?.tag()?)
    );
    assert_eq!(2, doc.get(0)?.size()?);
    assert_eq!(2, doc.get(0)?.get(b"center")?.size()?);
    assert_eq!(73, doc.get(0)?.get(b"center")?.get(b"x")?.as_::<i32>()?);
    assert_eq!(129, doc.get(0)?.get(b"center")?.get(b"y")?.as_::<i32>()?);
    assert_eq!(7, doc.get(0)?.get(b"radius")?.as_::<i32>()?);
    assert_eq!(
        Bytes::from(b"tag:clarkevans.com,2002:line"),
        Bytes::from(doc.get(1)?.tag()?)
    );
    assert_eq!(2, doc.get(1)?.size()?);
    assert_eq!(2, doc.get(1)?.get(b"start")?.size()?);
    assert_eq!(73, doc.get(1)?.get(b"start")?.get(b"x")?.as_::<i32>()?);
    assert_eq!(129, doc.get(1)?.get(b"start")?.get(b"y")?.as_::<i32>()?);
    assert_eq!(2, doc.get(1)?.get(b"finish")?.size()?);
    assert_eq!(89, doc.get(1)?.get(b"finish")?.get(b"x")?.as_::<i32>()?);
    assert_eq!(102, doc.get(1)?.get(b"finish")?.get(b"y")?.as_::<i32>()?);
    assert_eq!(
        Bytes::from(b"tag:clarkevans.com,2002:label"),
        Bytes::from(doc.get(2)?.tag()?)
    );
    assert_eq!(3, doc.get(2)?.size()?);
    assert_eq!(2, doc.get(2)?.get(b"start")?.size()?);
    assert_eq!(73, doc.get(2)?.get(b"start")?.get(b"x")?.as_::<i32>()?);
    assert_eq!(129, doc.get(2)?.get(b"start")?.get(b"y")?.as_::<i32>()?);
    assert_eq!(
        Bytes::from(b"0xFFEEBB"),
        Bytes::from(doc.get(2)?.get(b"color")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"Pretty vector drawing."),
        Bytes::from(doc.get(2)?.get(b"text")?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex2_25_UnorderedSets)`.
#[test]
fn ex2_25_unordered_sets() -> Result<()> {
    let doc = load(EX2_25)?;
    assert_eq!(
        Bytes::from(b"tag:yaml.org,2002:set"),
        Bytes::from(doc.tag()?)
    );
    assert_eq!(3, doc.size()?);
    assert!(doc.get(b"Mark McGwire")?.is_null()?);
    assert!(doc.get(b"Sammy Sosa")?.is_null()?);
    assert!(doc.get(b"Ken Griffey")?.is_null()?);
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex2_16_OrderedMappings)`.
#[test]
fn ex2_16_ordered_mappings() -> Result<()> {
    let doc = load(EX2_26)?;
    assert_eq!(
        Bytes::from(b"tag:yaml.org,2002:omap"),
        Bytes::from(doc.tag()?)
    );
    assert_eq!(3, doc.size()?);
    assert_eq!(1, doc.get(0)?.size()?);
    assert_eq!(65, doc.get(0)?.get(b"Mark McGwire")?.as_::<i32>()?);
    assert_eq!(1, doc.get(1)?.size()?);
    assert_eq!(63, doc.get(1)?.get(b"Sammy Sosa")?.as_::<i32>()?);
    assert_eq!(1, doc.get(2)?.size()?);
    assert_eq!(58, doc.get(2)?.get(b"Ken Griffey")?.as_::<i32>()?);
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex2_27_Invoice)`.
#[test]
fn ex2_27_invoice() -> Result<()> {
    let doc = load(EX2_27)?;
    assert_eq!(
        Bytes::from(b"tag:clarkevans.com,2002:invoice"),
        Bytes::from(doc.tag()?)
    );
    assert_eq!(8, doc.size()?);
    assert_eq!(34843, doc.get(b"invoice")?.as_::<i32>()?);
    assert_eq!(
        Bytes::from(b"2001-01-23"),
        Bytes::from(doc.get(b"date")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(3, doc.get(b"bill-to")?.size()?);
    assert_eq!(
        Bytes::from(b"Chris"),
        Bytes::from(doc.get(b"bill-to")?.get(b"given")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"Dumars"),
        Bytes::from(doc.get(b"bill-to")?.get(b"family")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(4, doc.get(b"bill-to")?.get(b"address")?.size()?);
    assert!(
        Bytes::from(
            doc.get(b"bill-to")?
                .get(b"address")?
                .get(b"lines")?
                .as_::<Vec<u8>>()?
        ) == Bytes::from(b"458 Walkman Dr.\nSuite #292\n")
    );
    assert!(
        Bytes::from(
            doc.get(b"bill-to")?
                .get(b"address")?
                .get(b"city")?
                .as_::<Vec<u8>>()?
        ) == Bytes::from(b"Royal Oak")
    );
    assert_eq!(
        Bytes::from(b"MI"),
        Bytes::from(
            doc.get(b"bill-to")?
                .get(b"address")?
                .get(b"state")?
                .as_::<Vec<u8>>()?
        )
    );
    assert_eq!(
        Bytes::from(b"48046"),
        Bytes::from(
            doc.get(b"bill-to")?
                .get(b"address")?
                .get(b"postal")?
                .as_::<Vec<u8>>()?
        )
    );
    assert_eq!(3, doc.get(b"ship-to")?.size()?);
    assert_eq!(
        Bytes::from(b"Chris"),
        Bytes::from(doc.get(b"ship-to")?.get(b"given")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"Dumars"),
        Bytes::from(doc.get(b"ship-to")?.get(b"family")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(4, doc.get(b"ship-to")?.get(b"address")?.size()?);
    assert!(
        Bytes::from(
            doc.get(b"ship-to")?
                .get(b"address")?
                .get(b"lines")?
                .as_::<Vec<u8>>()?
        ) == Bytes::from(b"458 Walkman Dr.\nSuite #292\n")
    );
    assert!(
        Bytes::from(
            doc.get(b"ship-to")?
                .get(b"address")?
                .get(b"city")?
                .as_::<Vec<u8>>()?
        ) == Bytes::from(b"Royal Oak")
    );
    assert_eq!(
        Bytes::from(b"MI"),
        Bytes::from(
            doc.get(b"ship-to")?
                .get(b"address")?
                .get(b"state")?
                .as_::<Vec<u8>>()?
        )
    );
    assert_eq!(
        Bytes::from(b"48046"),
        Bytes::from(
            doc.get(b"ship-to")?
                .get(b"address")?
                .get(b"postal")?
                .as_::<Vec<u8>>()?
        )
    );
    assert_eq!(2, doc.get(b"product")?.size()?);
    assert_eq!(4, doc.get(b"product")?.get(0)?.size()?);
    assert_eq!(
        Bytes::from(b"BL394D"),
        Bytes::from(doc.get(b"product")?.get(0)?.get(b"sku")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        4,
        doc.get(b"product")?
            .get(0)?
            .get(b"quantity")?
            .as_::<i32>()?
    );
    assert!(
        Bytes::from(
            doc.get(b"product")?
                .get(0)?
                .get(b"description")?
                .as_::<Vec<u8>>()?
        ) == Bytes::from(b"Basketball")
    );
    assert_eq!(
        Bytes::from(b"450.00"),
        Bytes::from(
            doc.get(b"product")?
                .get(0)?
                .get(b"price")?
                .as_::<Vec<u8>>()?
        )
    );
    assert_eq!(4, doc.get(b"product")?.get(1)?.size()?);
    assert_eq!(
        Bytes::from(b"BL4438H"),
        Bytes::from(doc.get(b"product")?.get(1)?.get(b"sku")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        1,
        doc.get(b"product")?
            .get(1)?
            .get(b"quantity")?
            .as_::<i32>()?
    );
    assert!(
        Bytes::from(
            doc.get(b"product")?
                .get(1)?
                .get(b"description")?
                .as_::<Vec<u8>>()?
        ) == Bytes::from(b"Super Hoop")
    );
    assert_eq!(
        Bytes::from(b"2392.00"),
        Bytes::from(
            doc.get(b"product")?
                .get(1)?
                .get(b"price")?
                .as_::<Vec<u8>>()?
        )
    );
    assert_eq!(
        Bytes::from(b"251.42"),
        Bytes::from(doc.get(b"tax")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"4443.52"),
        Bytes::from(doc.get(b"total")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"Late afternoon is best. Backup contact is Nancy Billsmer @ 338-4338."),
        Bytes::from(doc.get(b"comments")?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex2_28_LogFile)`.
#[test]
fn ex2_28_log_file() -> Result<()> {
    let docs = load_all(EX2_28)?;
    assert_eq!(3, docs.len());
    {
        let doc = docs[0].clone();
        assert_eq!(3, doc.size()?);
        assert_eq!(
            Bytes::from(b"2001-11-23 15:01:42 -5"),
            Bytes::from(doc.get(b"Time")?.as_::<Vec<u8>>()?)
        );
        assert_eq!(
            Bytes::from(b"ed"),
            Bytes::from(doc.get(b"User")?.as_::<Vec<u8>>()?)
        );
        assert!(
            Bytes::from(doc.get(b"Warning")?.as_::<Vec<u8>>()?)
                == Bytes::from(b"This is an error message for the log file")
        );
    }
    {
        let doc = docs[1].clone();
        assert_eq!(3, doc.size()?);
        assert_eq!(
            Bytes::from(b"2001-11-23 15:02:31 -5"),
            Bytes::from(doc.get(b"Time")?.as_::<Vec<u8>>()?)
        );
        assert_eq!(
            Bytes::from(b"ed"),
            Bytes::from(doc.get(b"User")?.as_::<Vec<u8>>()?)
        );
        assert!(
            Bytes::from(doc.get(b"Warning")?.as_::<Vec<u8>>()?)
                == Bytes::from(b"A slightly different error message.")
        );
    }
    {
        let doc = docs[2].clone();
        assert_eq!(4, doc.size()?);
        assert_eq!(
            Bytes::from(b"2001-11-23 15:03:17 -5"),
            Bytes::from(doc.get(b"Date")?.as_::<Vec<u8>>()?)
        );
        assert_eq!(
            Bytes::from(b"ed"),
            Bytes::from(doc.get(b"User")?.as_::<Vec<u8>>()?)
        );
        assert_eq!(
            Bytes::from(b"Unknown variable \"bar\""),
            Bytes::from(doc.get(b"Fatal")?.as_::<Vec<u8>>()?)
        );
        assert_eq!(2, doc.get(b"Stack")?.size()?);
        assert_eq!(3, doc.get(b"Stack")?.get(0)?.size()?);
        assert_eq!(
            Bytes::from(b"TopClass.py"),
            Bytes::from(doc.get(b"Stack")?.get(0)?.get(b"file")?.as_::<Vec<u8>>()?)
        );
        assert_eq!(
            Bytes::from(b"23"),
            Bytes::from(doc.get(b"Stack")?.get(0)?.get(b"line")?.as_::<Vec<u8>>()?)
        );
        assert!(
            Bytes::from(doc.get(b"Stack")?.get(0)?.get(b"code")?.as_::<Vec<u8>>()?)
                == Bytes::from(b"x = MoreObject(\"345\\n\")\n")
        );
        assert_eq!(3, doc.get(b"Stack")?.get(1)?.size()?);
        assert_eq!(
            Bytes::from(b"MoreClass.py"),
            Bytes::from(doc.get(b"Stack")?.get(1)?.get(b"file")?.as_::<Vec<u8>>()?)
        );
        assert_eq!(
            Bytes::from(b"58"),
            Bytes::from(doc.get(b"Stack")?.get(1)?.get(b"line")?.as_::<Vec<u8>>()?)
        );
        assert_eq!(
            Bytes::from(b"foo = bar"),
            Bytes::from(doc.get(b"Stack")?.get(1)?.get(b"code")?.as_::<Vec<u8>>()?)
        );
    }
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex5_3_BlockStructureIndicators)`.
#[test]
fn ex5_3_block_structure_indicators() -> Result<()> {
    let doc = load(EX5_3)?;
    assert_eq!(2, doc.size()?);
    assert_eq!(2, doc.get(b"sequence")?.size()?);
    assert_eq!(
        Bytes::from(b"one"),
        Bytes::from(doc.get(b"sequence")?.get(0)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"two"),
        Bytes::from(doc.get(b"sequence")?.get(1)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(2, doc.get(b"mapping")?.size()?);
    assert_eq!(
        Bytes::from(b"blue"),
        Bytes::from(doc.get(b"mapping")?.get(b"sky")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"green"),
        Bytes::from(doc.get(b"mapping")?.get(b"sea")?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex5_4_FlowStructureIndicators)`.
#[test]
fn ex5_4_flow_structure_indicators() -> Result<()> {
    let doc = load(EX5_4)?;
    assert_eq!(2, doc.size()?);
    assert_eq!(2, doc.get(b"sequence")?.size()?);
    assert_eq!(
        Bytes::from(b"one"),
        Bytes::from(doc.get(b"sequence")?.get(0)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"two"),
        Bytes::from(doc.get(b"sequence")?.get(1)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(2, doc.get(b"mapping")?.size()?);
    assert_eq!(
        Bytes::from(b"blue"),
        Bytes::from(doc.get(b"mapping")?.get(b"sky")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"green"),
        Bytes::from(doc.get(b"mapping")?.get(b"sea")?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex5_5_CommentIndicator)`.
#[test]
fn ex5_5_comment_indicator() -> Result<()> {
    let doc = load(EX5_5)?;
    assert!(doc.is_null()?);
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex5_6_NodePropertyIndicators)`.
#[test]
fn ex5_6_node_property_indicators() -> Result<()> {
    let doc = load(EX5_6)?;
    assert_eq!(2, doc.size()?);
    assert!(Bytes::from(doc.get(b"anchored")?.as_::<Vec<u8>>()?) == Bytes::from(b"value"));
    assert_eq!(
        Bytes::from(b"value"),
        Bytes::from(doc.get(b"alias")?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex5_7_BlockScalarIndicators)`.
#[test]
fn ex5_7_block_scalar_indicators() -> Result<()> {
    let doc = load(EX5_7)?;
    assert_eq!(2, doc.size()?);
    assert_eq!(
        Bytes::from(b"some\ntext\n"),
        Bytes::from(doc.get(b"literal")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"some text\n"),
        Bytes::from(doc.get(b"folded")?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex5_8_QuotedScalarIndicators)`.
#[test]
fn ex5_8_quoted_scalar_indicators() -> Result<()> {
    let doc = load(EX5_8)?;
    assert_eq!(2, doc.size()?);
    assert_eq!(
        Bytes::from(b"text"),
        Bytes::from(doc.get(b"single")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"text"),
        Bytes::from(doc.get(b"double")?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex5_11_LineBreakCharacters)`.
#[test]
fn ex5_11_line_break_characters() -> Result<()> {
    let doc = load(EX5_11)?;
    assert!(
        Bytes::from(doc.as_::<Vec<u8>>()?)
            == Bytes::from(b"Line break (no glyph)\nLine break (glyphed)\n")
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex5_12_TabsAndSpaces)`.
#[test]
fn ex5_12_tabs_and_spaces() -> Result<()> {
    let doc = load(EX5_12)?;
    assert_eq!(2, doc.size()?);
    assert_eq!(
        Bytes::from(b"Quoted\t"),
        Bytes::from(doc.get(b"quoted")?.as_::<Vec<u8>>()?)
    );
    assert!(
        Bytes::from(doc.get(b"block")?.as_::<Vec<u8>>()?)
            == Bytes::from(b"void main() {\n\tprintf(\"Hello, world!\\n\");\n}")
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex5_13_EscapedCharacters)`.
#[test]
fn ex5_13_escaped_characters() -> Result<()> {
    let doc = load(EX5_13)?;
    assert!(
        Bytes::from(doc.as_::<Vec<u8>>()?)
            == Bytes::from(
                [
                    b"Fun with \\ \" \x07 \x08 \x1B \x0C \n \r \t \x0B ".as_slice(),
                    b"\x00".as_slice(),
                    b"   \xA0 \x85 \xE2\x80\xA8 \xE2\x80\xA9 A A A".as_slice()
                ]
                .concat()
            )
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex5_14_InvalidEscapedCharacters)`.
#[test]
fn ex5_14_invalid_escaped_characters() -> Result<()> {
    expect_load_parser_exception(
        EX5_14,
        [error_msg::INVALID_ESCAPE.as_bytes(), b"c".as_slice()].concat(),
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex6_1_IndentationSpaces)`.
#[test]
fn ex6_1_indentation_spaces() -> Result<()> {
    let doc = load(EX6_1)?;
    assert_eq!(1, doc.size()?);
    assert_eq!(2, doc.get(b"Not indented")?.size()?);
    assert!(
        Bytes::from(
            doc.get(b"Not indented")?
                .get(b"By one space")?
                .as_::<Vec<u8>>()?
        ) == Bytes::from(b"By four\n  spaces\n")
    );
    assert_eq!(3, doc.get(b"Not indented")?.get(b"Flow style")?.size()?);
    assert!(
        Bytes::from(
            doc.get(b"Not indented")?
                .get(b"Flow style")?
                .get(0)?
                .as_::<Vec<u8>>()?
        ) == Bytes::from(b"By two")
    );
    assert!(
        Bytes::from(
            doc.get(b"Not indented")?
                .get(b"Flow style")?
                .get(1)?
                .as_::<Vec<u8>>()?
        ) == Bytes::from(b"Also by two")
    );
    assert!(
        Bytes::from(
            doc.get(b"Not indented")?
                .get(b"Flow style")?
                .get(2)?
                .as_::<Vec<u8>>()?
        ) == Bytes::from(b"Still by two")
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex6_2_IndentationIndicators)`.
#[test]
fn ex6_2_indentation_indicators() -> Result<()> {
    let doc = load(EX6_2)?;
    assert_eq!(1, doc.size()?);
    assert_eq!(2, doc.get(b"a")?.size()?);
    assert_eq!(
        Bytes::from(b"b"),
        Bytes::from(doc.get(b"a")?.get(0)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(2, doc.get(b"a")?.get(1)?.size()?);
    assert_eq!(
        Bytes::from(b"c"),
        Bytes::from(doc.get(b"a")?.get(1)?.get(0)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"d"),
        Bytes::from(doc.get(b"a")?.get(1)?.get(1)?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex6_3_SeparationSpaces)`.
#[test]
fn ex6_3_separation_spaces() -> Result<()> {
    let doc = load(EX6_3)?;
    assert_eq!(2, doc.size()?);
    assert_eq!(1, doc.get(0)?.size()?);
    assert_eq!(
        Bytes::from(b"bar"),
        Bytes::from(doc.get(0)?.get(b"foo")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(2, doc.get(1)?.size()?);
    assert_eq!(
        Bytes::from(b"baz"),
        Bytes::from(doc.get(1)?.get(0)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"baz"),
        Bytes::from(doc.get(1)?.get(1)?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex6_4_LinePrefixes)`.
#[test]
fn ex6_4_line_prefixes() -> Result<()> {
    let doc = load(EX6_4)?;
    assert_eq!(3, doc.size()?);
    assert_eq!(
        Bytes::from(b"text lines"),
        Bytes::from(doc.get(b"plain")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"text lines"),
        Bytes::from(doc.get(b"quoted")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"text\n \tlines\n"),
        Bytes::from(doc.get(b"block")?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex6_5_EmptyLines)`.
#[test]
fn ex6_5_empty_lines() -> Result<()> {
    let doc = load(EX6_5)?;
    assert_eq!(2, doc.size()?);
    assert_eq!(
        Bytes::from(b"Empty line\nas a line feed"),
        Bytes::from(doc.get(b"Folding")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"Clipped empty lines\n"),
        Bytes::from(doc.get(b"Chomping")?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex6_6_LineFolding)`.
#[test]
fn ex6_6_line_folding() -> Result<()> {
    let doc = load(EX6_6)?;
    assert_eq!(
        Bytes::from(b"trimmed\n\n\nas space"),
        Bytes::from(doc.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex6_7_BlockFolding)`.
#[test]
fn ex6_7_block_folding() -> Result<()> {
    let doc = load(EX6_7)?;
    assert_eq!(
        Bytes::from(b"foo \n\n\t bar\n\nbaz\n"),
        Bytes::from(doc.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex6_8_FlowFolding)`.
#[test]
fn ex6_8_flow_folding() -> Result<()> {
    let doc = load(EX6_8)?;
    assert_eq!(
        Bytes::from(b" foo\nbar\nbaz "),
        Bytes::from(doc.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex6_9_SeparatedComment)`.
#[test]
fn ex6_9_separated_comment() -> Result<()> {
    let doc = load(EX6_9)?;
    assert_eq!(1, doc.size()?);
    assert_eq!(
        Bytes::from(b"value"),
        Bytes::from(doc.get(b"key")?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex6_10_CommentLines)`.
#[test]
fn ex6_10_comment_lines() -> Result<()> {
    let doc = load(EX6_10)?;
    assert!(doc.is_null()?);
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex6_11_MultiLineComments)`.
#[test]
fn ex6_11_multi_line_comments() -> Result<()> {
    let doc = load(EX6_11)?;
    assert_eq!(1, doc.size()?);
    assert_eq!(
        Bytes::from(b"value"),
        Bytes::from(doc.get(b"key")?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex6_12_SeparationSpacesII)`.
#[test]
fn ex6_12_separation_spaces_ii() -> Result<()> {
    let doc = load(EX6_12)?;

    let sammy: &[(&[u8], &[u8])] = &[(b"first", b"Sammy"), (b"last", b"Sosa")];

    assert_eq!(1, doc.size()?);
    assert_eq!(2, doc.get(Key::StrMap(sammy))?.size()?);
    assert_eq!(65, doc.get(Key::StrMap(sammy))?.get(b"hr")?.as_::<i32>()?);
    assert_eq!(
        Bytes::from(b"0.278"),
        Bytes::from(doc.get(Key::StrMap(sammy))?.get(b"avg")?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex6_13_ReservedDirectives)`.
#[test]
fn ex6_13_reserved_directives() -> Result<()> {
    let doc = load(EX6_13)?;
    assert_eq!(Bytes::from(b"foo"), Bytes::from(doc.as_::<Vec<u8>>()?));
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex6_14_YAMLDirective)`.
#[test]
fn ex6_14_yaml_directive() -> Result<()> {
    let doc = load(EX6_14)?;
    assert_eq!(Bytes::from(b"foo"), Bytes::from(doc.as_::<Vec<u8>>()?));
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex6_15_InvalidRepeatedYAMLDirective)`.
#[test]
fn ex6_15_invalid_repeated_yaml_directive() -> Result<()> {
    expect_load_parser_exception(EX6_15, error_msg::REPEATED_YAML_DIRECTIVE.as_bytes());
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex6_16_TagDirective)`.
#[test]
fn ex6_16_tag_directive() -> Result<()> {
    let doc = load(EX6_16)?;
    assert_eq!(
        Bytes::from(b"tag:yaml.org,2002:str"),
        Bytes::from(doc.tag()?)
    );
    assert_eq!(Bytes::from(b"foo"), Bytes::from(doc.as_::<Vec<u8>>()?));
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex6_17_InvalidRepeatedTagDirective)`.
#[test]
fn ex6_17_invalid_repeated_tag_directive() -> Result<()> {
    expect_load_parser_exception(EX6_17, error_msg::REPEATED_TAG_DIRECTIVE.as_bytes());
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex6_18_PrimaryTagHandle)`.
#[test]
fn ex6_18_primary_tag_handle() -> Result<()> {
    let docs = load_all(EX6_18)?;
    assert_eq!(2, docs.len());
    {
        let doc = docs[0].clone();
        assert_eq!(Bytes::from(b"!foo"), Bytes::from(doc.tag()?));
        assert_eq!(Bytes::from(b"bar"), Bytes::from(doc.as_::<Vec<u8>>()?));
    }
    {
        let doc = docs[1].clone();
        assert_eq!(
            Bytes::from(b"tag:example.com,2000:app/foo"),
            Bytes::from(doc.tag()?)
        );
        assert_eq!(Bytes::from(b"bar"), Bytes::from(doc.as_::<Vec<u8>>()?));
    }
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex6_19_SecondaryTagHandle)`.
#[test]
fn ex6_19_secondary_tag_handle() -> Result<()> {
    let doc = load(EX6_19)?;
    assert_eq!(
        Bytes::from(b"tag:example.com,2000:app/int"),
        Bytes::from(doc.tag()?)
    );
    assert_eq!(Bytes::from(b"1 - 3"), Bytes::from(doc.as_::<Vec<u8>>()?));
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex6_20_TagHandles)`.
#[test]
fn ex6_20_tag_handles() -> Result<()> {
    let doc = load(EX6_20)?;
    assert_eq!(
        Bytes::from(b"tag:example.com,2000:app/foo"),
        Bytes::from(doc.tag()?)
    );
    assert_eq!(Bytes::from(b"bar"), Bytes::from(doc.as_::<Vec<u8>>()?));
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex6_21_LocalTagPrefix)`.
#[test]
fn ex6_21_local_tag_prefix() -> Result<()> {
    let docs = load_all(EX6_21)?;
    assert_eq!(2, docs.len());
    {
        let doc = docs[0].clone();
        assert_eq!(Bytes::from(b"!my-light"), Bytes::from(doc.tag()?));
        assert_eq!(
            Bytes::from(b"fluorescent"),
            Bytes::from(doc.as_::<Vec<u8>>()?)
        );
    }
    {
        let doc = docs[1].clone();
        assert_eq!(Bytes::from(b"!my-light"), Bytes::from(doc.tag()?));
        assert_eq!(Bytes::from(b"green"), Bytes::from(doc.as_::<Vec<u8>>()?));
    }
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex6_22_GlobalTagPrefix)`.
#[test]
fn ex6_22_global_tag_prefix() -> Result<()> {
    let doc = load(EX6_22)?;
    assert_eq!(1, doc.size()?);
    assert_eq!(
        Bytes::from(b"tag:example.com,2000:app/foo"),
        Bytes::from(doc.get(0)?.tag()?)
    );
    assert_eq!(
        Bytes::from(b"bar"),
        Bytes::from(doc.get(0)?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex6_23_NodeProperties)`.
#[test]
fn ex6_23_node_properties() -> Result<()> {
    let doc = load(EX6_23)?;
    assert_eq!(2, doc.size()?);
    for it in doc.iter() {
        if Bytes::from(it.first.as_::<Vec<u8>>()?) == Bytes::from(b"foo") {
            assert_eq!(
                Bytes::from(b"tag:yaml.org,2002:str"),
                Bytes::from(it.first.tag()?)
            );
            assert_eq!(
                Bytes::from(b"tag:yaml.org,2002:str"),
                Bytes::from(it.second.tag()?)
            );
            assert_eq!(
                Bytes::from(b"bar"),
                Bytes::from(it.second.as_::<Vec<u8>>()?)
            );
        } else if Bytes::from(it.first.as_::<Vec<u8>>()?) == Bytes::from(b"baz") {
            assert_eq!(
                Bytes::from(b"foo"),
                Bytes::from(it.second.as_::<Vec<u8>>()?)
            );
        } else {
            panic!("unknown key");
        }
    }
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex6_24_VerbatimTags)`.
#[test]
fn ex6_24_verbatim_tags() -> Result<()> {
    let doc = load(EX6_24)?;
    assert_eq!(1, doc.size()?);
    for it in doc.iter() {
        assert_eq!(
            Bytes::from(b"tag:yaml.org,2002:str"),
            Bytes::from(it.first.tag()?)
        );
        assert_eq!(Bytes::from(b"foo"), Bytes::from(it.first.as_::<Vec<u8>>()?));
        assert_eq!(Bytes::from(b"!bar"), Bytes::from(it.second.tag()?));
        assert_eq!(
            Bytes::from(b"baz"),
            Bytes::from(it.second.as_::<Vec<u8>>()?)
        );
    }
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex6_26_TagShorthands)`.
#[test]
fn ex6_26_tag_shorthands() -> Result<()> {
    let doc = load(EX6_26)?;
    assert_eq!(3, doc.size()?);
    assert_eq!(Bytes::from(b"!local"), Bytes::from(doc.get(0)?.tag()?));
    assert_eq!(
        Bytes::from(b"foo"),
        Bytes::from(doc.get(0)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"tag:yaml.org,2002:str"),
        Bytes::from(doc.get(1)?.tag()?)
    );
    assert_eq!(
        Bytes::from(b"bar"),
        Bytes::from(doc.get(1)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"tag:example.com,2000:app/tag%21"),
        Bytes::from(doc.get(2)?.tag()?)
    );
    assert_eq!(
        Bytes::from(b"baz"),
        Bytes::from(doc.get(2)?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex6_27a_InvalidTagShorthands)`.
#[test]
fn ex6_27a_invalid_tag_shorthands() -> Result<()> {
    expect_load_parser_exception(EX6_27A, error_msg::TAG_WITH_NO_SUFFIX.as_bytes());
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex6_28_NonSpecificTags)`.
#[test]
fn ex6_28_non_specific_tags() -> Result<()> {
    let doc = load(EX6_28)?;
    assert_eq!(3, doc.size()?);
    assert_eq!(
        Bytes::from(b"12"),
        Bytes::from(doc.get(0)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(12, doc.get(1)?.as_::<i32>()?);
    assert_eq!(
        Bytes::from(b"12"),
        Bytes::from(doc.get(2)?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex6_29_NodeAnchors)`.
#[test]
fn ex6_29_node_anchors() -> Result<()> {
    let doc = load(EX6_29)?;
    assert_eq!(2, doc.size()?);
    assert_eq!(
        Bytes::from(b"Value"),
        Bytes::from(doc.get(b"First occurrence")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"Value"),
        Bytes::from(doc.get(b"Second occurrence")?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex7_1_AliasNodes)`.
#[test]
fn ex7_1_alias_nodes() -> Result<()> {
    let doc = load(EX7_1)?;
    assert_eq!(4, doc.size()?);
    assert_eq!(
        Bytes::from(b"Foo"),
        Bytes::from(doc.get(b"First occurrence")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"Foo"),
        Bytes::from(doc.get(b"Second occurrence")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"Bar"),
        Bytes::from(doc.get(b"Override anchor")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"Bar"),
        Bytes::from(doc.get(b"Reuse anchor")?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex7_2_EmptyNodes)`.
#[test]
fn ex7_2_empty_nodes() -> Result<()> {
    let doc = load(EX7_2)?;
    assert_eq!(2, doc.size()?);
    for it in doc.iter() {
        if Bytes::from(it.first.as_::<Vec<u8>>()?) == Bytes::from(b"foo") {
            assert_eq!(
                Bytes::from(b"tag:yaml.org,2002:str"),
                Bytes::from(it.second.tag()?)
            );
            assert_eq!(Bytes::from(b""), Bytes::from(it.second.as_::<Vec<u8>>()?));
        } else if Bytes::from(it.first.as_::<Vec<u8>>()?) == Bytes::from(b"") {
            assert_eq!(
                Bytes::from(b"tag:yaml.org,2002:str"),
                Bytes::from(it.first.tag()?)
            );
            assert_eq!(
                Bytes::from(b"bar"),
                Bytes::from(it.second.as_::<Vec<u8>>()?)
            );
        } else {
            panic!("unexpected key");
        }
    }
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex7_3_CompletelyEmptyNodes)`.
#[test]
fn ex7_3_completely_empty_nodes() -> Result<()> {
    let doc = load(EX7_3)?;
    assert_eq!(2, doc.size()?);
    assert!(doc.get(b"foo")?.is_null()?);
    assert_eq!(
        Bytes::from(b"bar"),
        Bytes::from(doc.get(Key::Null)?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex7_4_DoubleQuotedImplicitKeys)`.
#[test]
fn ex7_4_double_quoted_implicit_keys() -> Result<()> {
    let doc = load(EX7_4)?;
    assert_eq!(1, doc.size()?);
    assert_eq!(1, doc.get(b"implicit block key")?.size()?);
    assert_eq!(1, doc.get(b"implicit block key")?.get(0)?.size()?);
    assert_eq!(
        Bytes::from(b"value"),
        Bytes::from(
            doc.get(b"implicit block key")?
                .get(0)?
                .get(b"implicit flow key")?
                .as_::<Vec<u8>>()?
        )
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex7_5_DoubleQuotedLineBreaks)`.
#[test]
fn ex7_5_double_quoted_line_breaks() -> Result<()> {
    let doc = load(EX7_5)?;
    assert!(
        Bytes::from(doc.as_::<Vec<u8>>()?)
            == Bytes::from(b"folded to a space,\nto a line feed, or \t \tnon-content")
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex7_6_DoubleQuotedLines)`.
#[test]
fn ex7_6_double_quoted_lines() -> Result<()> {
    let doc = load(EX7_6)?;
    assert!(
        Bytes::from(doc.as_::<Vec<u8>>()?)
            == Bytes::from(b" 1st non-empty\n2nd non-empty 3rd non-empty ")
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex7_7_SingleQuotedCharacters)`.
#[test]
fn ex7_7_single_quoted_characters() -> Result<()> {
    let doc = load(EX7_7)?;
    assert_eq!(
        Bytes::from(b"here's to \"quotes\""),
        Bytes::from(doc.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex7_8_SingleQuotedImplicitKeys)`.
#[test]
fn ex7_8_single_quoted_implicit_keys() -> Result<()> {
    let doc = load(EX7_8)?;
    assert_eq!(1, doc.size()?);
    assert_eq!(1, doc.get(b"implicit block key")?.size()?);
    assert_eq!(1, doc.get(b"implicit block key")?.get(0)?.size()?);
    assert_eq!(
        Bytes::from(b"value"),
        Bytes::from(
            doc.get(b"implicit block key")?
                .get(0)?
                .get(b"implicit flow key")?
                .as_::<Vec<u8>>()?
        )
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex7_9_SingleQuotedLines)`.
#[test]
fn ex7_9_single_quoted_lines() -> Result<()> {
    let doc = load(EX7_9)?;
    assert!(
        Bytes::from(doc.as_::<Vec<u8>>()?)
            == Bytes::from(b" 1st non-empty\n2nd non-empty 3rd non-empty ")
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex7_10_PlainCharacters)`.
#[test]
fn ex7_10_plain_characters() -> Result<()> {
    let doc = load(EX7_10)?;
    assert_eq!(6, doc.size()?);
    assert_eq!(
        Bytes::from(b"::vector"),
        Bytes::from(doc.get(0)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b": - ()"),
        Bytes::from(doc.get(1)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"Up, up, and away!"),
        Bytes::from(doc.get(2)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(-123, doc.get(3)?.as_::<i32>()?);
    assert_eq!(
        Bytes::from(b"http://example.com/foo#bar"),
        Bytes::from(doc.get(4)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(5, doc.get(5)?.size()?);
    assert_eq!(
        Bytes::from(b"::vector"),
        Bytes::from(doc.get(5)?.get(0)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b": - ()"),
        Bytes::from(doc.get(5)?.get(1)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"Up, up, and away!"),
        Bytes::from(doc.get(5)?.get(2)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(-123, doc.get(5)?.get(3)?.as_::<i32>()?);
    assert_eq!(
        Bytes::from(b"http://example.com/foo#bar"),
        Bytes::from(doc.get(5)?.get(4)?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex7_11_PlainImplicitKeys)`.
#[test]
fn ex7_11_plain_implicit_keys() -> Result<()> {
    let doc = load(EX7_11)?;
    assert_eq!(1, doc.size()?);
    assert_eq!(1, doc.get(b"implicit block key")?.size()?);
    assert_eq!(1, doc.get(b"implicit block key")?.get(0)?.size()?);
    assert_eq!(
        Bytes::from(b"value"),
        Bytes::from(
            doc.get(b"implicit block key")?
                .get(0)?
                .get(b"implicit flow key")?
                .as_::<Vec<u8>>()?
        )
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex7_12_PlainLines)`.
#[test]
fn ex7_12_plain_lines() -> Result<()> {
    let doc = load(EX7_12)?;
    assert!(
        Bytes::from(doc.as_::<Vec<u8>>()?)
            == Bytes::from(b"1st non-empty\n2nd non-empty 3rd non-empty")
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex7_13_FlowSequence)`.
#[test]
fn ex7_13_flow_sequence() -> Result<()> {
    let doc = load(EX7_13)?;
    assert_eq!(2, doc.size()?);
    assert_eq!(2, doc.get(0)?.size()?);
    assert_eq!(
        Bytes::from(b"one"),
        Bytes::from(doc.get(0)?.get(0)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"two"),
        Bytes::from(doc.get(0)?.get(1)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(2, doc.get(1)?.size()?);
    assert_eq!(
        Bytes::from(b"three"),
        Bytes::from(doc.get(1)?.get(0)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"four"),
        Bytes::from(doc.get(1)?.get(1)?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex7_14_FlowSequenceEntries)`.
#[test]
fn ex7_14_flow_sequence_entries() -> Result<()> {
    let doc = load(EX7_14)?;
    assert_eq!(5, doc.size()?);
    assert_eq!(
        Bytes::from(b"double quoted"),
        Bytes::from(doc.get(0)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"single quoted"),
        Bytes::from(doc.get(1)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"plain text"),
        Bytes::from(doc.get(2)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(1, doc.get(3)?.size()?);
    assert_eq!(
        Bytes::from(b"nested"),
        Bytes::from(doc.get(3)?.get(0)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(1, doc.get(4)?.size()?);
    assert_eq!(
        Bytes::from(b"pair"),
        Bytes::from(doc.get(4)?.get(b"single")?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex7_15_FlowMappings)`.
#[test]
fn ex7_15_flow_mappings() -> Result<()> {
    let doc = load(EX7_15)?;
    assert_eq!(2, doc.size()?);
    assert_eq!(2, doc.get(0)?.size()?);
    assert_eq!(
        Bytes::from(b"two"),
        Bytes::from(doc.get(0)?.get(b"one")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"four"),
        Bytes::from(doc.get(0)?.get(b"three")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(2, doc.get(1)?.size()?);
    assert_eq!(
        Bytes::from(b"six"),
        Bytes::from(doc.get(1)?.get(b"five")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"eight"),
        Bytes::from(doc.get(1)?.get(b"seven")?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex7_16_FlowMappingEntries)`.
#[test]
fn ex7_16_flow_mapping_entries() -> Result<()> {
    let doc = load(EX7_16)?;
    assert_eq!(3, doc.size()?);
    assert_eq!(
        Bytes::from(b"entry"),
        Bytes::from(doc.get(b"explicit")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"entry"),
        Bytes::from(doc.get(b"implicit")?.as_::<Vec<u8>>()?)
    );
    assert!(doc.get(Key::Null)?.is_null()?);
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex7_17_FlowMappingSeparateValues)`.
#[test]
fn ex7_17_flow_mapping_separate_values() -> Result<()> {
    let doc = load(EX7_17)?;
    assert_eq!(4, doc.size()?);
    assert_eq!(
        Bytes::from(b"separate"),
        Bytes::from(doc.get(b"unquoted")?.as_::<Vec<u8>>()?)
    );
    assert!(doc.get(b"http://foo.com")?.is_null()?);
    assert!(doc.get(b"omitted value")?.is_null()?);
    assert_eq!(
        Bytes::from(b"omitted key"),
        Bytes::from(doc.get(Key::Null)?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex7_18_FlowMappingAdjacentValues)`.
#[test]
fn ex7_18_flow_mapping_adjacent_values() -> Result<()> {
    let doc = load(EX7_18)?;
    assert_eq!(3, doc.size()?);
    assert_eq!(
        Bytes::from(b"value"),
        Bytes::from(doc.get(b"adjacent")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"value"),
        Bytes::from(doc.get(b"readable")?.as_::<Vec<u8>>()?)
    );
    assert!(doc.get(b"empty")?.is_null()?);
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex7_19_SinglePairFlowMappings)`.
#[test]
fn ex7_19_single_pair_flow_mappings() -> Result<()> {
    let doc = load(EX7_19)?;
    assert_eq!(1, doc.size()?);
    assert_eq!(1, doc.get(0)?.size()?);
    assert_eq!(
        Bytes::from(b"bar"),
        Bytes::from(doc.get(0)?.get(b"foo")?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex7_20_SinglePairExplicitEntry)`.
#[test]
fn ex7_20_single_pair_explicit_entry() -> Result<()> {
    let doc = load(EX7_20)?;
    assert_eq!(1, doc.size()?);
    assert_eq!(1, doc.get(0)?.size()?);
    assert_eq!(
        Bytes::from(b"baz"),
        Bytes::from(doc.get(0)?.get(b"foo bar")?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex7_21_SinglePairImplicitEntries)`.
#[test]
fn ex7_21_single_pair_implicit_entries() -> Result<()> {
    let doc = load(EX7_21)?;
    assert_eq!(3, doc.size()?);
    assert_eq!(1, doc.get(0)?.size()?);
    assert_eq!(1, doc.get(0)?.get(0)?.size()?);
    assert_eq!(
        Bytes::from(b"separate"),
        Bytes::from(doc.get(0)?.get(0)?.get(b"YAML")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(1, doc.get(1)?.size()?);
    assert_eq!(1, doc.get(1)?.get(0)?.size()?);
    assert_eq!(
        Bytes::from(b"empty key entry"),
        Bytes::from(doc.get(1)?.get(0)?.get(Key::Null)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(1, doc.get(2)?.size()?);
    assert_eq!(1, doc.get(2)?.get(0)?.size()?);

    let key: &[(&[u8], &[u8])] = &[(b"JSON", b"like")];
    assert_eq!(
        Bytes::from(b"adjacent"),
        Bytes::from(
            doc.get(2)?
                .get(0)?
                .get(Key::StrMap(key))?
                .as_::<Vec<u8>>()?
        )
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex7_22_InvalidImplicitKeys)`.
#[test]
fn ex7_22_invalid_implicit_keys() -> Result<()> {
    expect_load_parser_exception(EX7_22, error_msg::END_OF_SEQ_FLOW.as_bytes());
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex7_23_FlowContent)`.
#[test]
fn ex7_23_flow_content() -> Result<()> {
    let doc = load(EX7_23)?;
    assert_eq!(5, doc.size()?);
    assert_eq!(2, doc.get(0)?.size()?);
    assert_eq!(
        Bytes::from(b"a"),
        Bytes::from(doc.get(0)?.get(0)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"b"),
        Bytes::from(doc.get(0)?.get(1)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(1, doc.get(1)?.size()?);
    assert_eq!(
        Bytes::from(b"b"),
        Bytes::from(doc.get(1)?.get(b"a")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"a"),
        Bytes::from(doc.get(2)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(b'b', doc.get(3)?.as_::<CChar>()?.0);
    assert_eq!(
        Bytes::from(b"c"),
        Bytes::from(doc.get(4)?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex7_24_FlowNodes)`.
#[test]
fn ex7_24_flow_nodes() -> Result<()> {
    let doc = load(EX7_24)?;
    assert_eq!(5, doc.size()?);
    assert_eq!(
        Bytes::from(b"tag:yaml.org,2002:str"),
        Bytes::from(doc.get(0)?.tag()?)
    );
    assert_eq!(
        Bytes::from(b"a"),
        Bytes::from(doc.get(0)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(b'b', doc.get(1)?.as_::<CChar>()?.0);
    assert_eq!(
        Bytes::from(b"c"),
        Bytes::from(doc.get(2)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"c"),
        Bytes::from(doc.get(3)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"tag:yaml.org,2002:str"),
        Bytes::from(doc.get(4)?.tag()?)
    );
    assert_eq!(Bytes::from(b""), Bytes::from(doc.get(4)?.as_::<Vec<u8>>()?));
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex8_1_BlockScalarHeader)`.
#[test]
fn ex8_1_block_scalar_header() -> Result<()> {
    let doc = load(EX8_1)?;
    assert_eq!(4, doc.size()?);
    assert_eq!(
        Bytes::from(b"literal\n"),
        Bytes::from(doc.get(0)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b" folded\n"),
        Bytes::from(doc.get(1)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"keep\n\n"),
        Bytes::from(doc.get(2)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b" strip"),
        Bytes::from(doc.get(3)?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex8_2_BlockIndentationHeader)`.
#[test]
fn ex8_2_block_indentation_header() -> Result<()> {
    let doc = load(EX8_2)?;
    assert_eq!(4, doc.size()?);
    assert_eq!(
        Bytes::from(b"detected\n"),
        Bytes::from(doc.get(0)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"\n\n# detected\n"),
        Bytes::from(doc.get(1)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b" explicit\n"),
        Bytes::from(doc.get(2)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"\t\ndetected\n"),
        Bytes::from(doc.get(3)?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex8_3a_InvalidBlockScalarIndentationIndicators)`.
#[test]
fn ex8_3a_invalid_block_scalar_indentation_indicators() -> Result<()> {
    expect_load_parser_exception(EX8_3A, error_msg::END_OF_SEQ.as_bytes());
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex8_3b_InvalidBlockScalarIndentationIndicators)`.
#[test]
fn ex8_3b_invalid_block_scalar_indentation_indicators() -> Result<()> {
    expect_load_parser_exception(EX8_3B, error_msg::END_OF_SEQ.as_bytes());
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex8_3c_InvalidBlockScalarIndentationIndicators)`.
#[test]
fn ex8_3c_invalid_block_scalar_indentation_indicators() -> Result<()> {
    expect_load_parser_exception(EX8_3C, error_msg::END_OF_SEQ.as_bytes());
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex8_4_ChompingFinalLineBreak)`.
#[test]
fn ex8_4_chomping_final_line_break() -> Result<()> {
    let doc = load(EX8_4)?;
    assert_eq!(3, doc.size()?);
    assert_eq!(
        Bytes::from(b"text"),
        Bytes::from(doc.get(b"strip")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"text\n"),
        Bytes::from(doc.get(b"clip")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"text\n"),
        Bytes::from(doc.get(b"keep")?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex8_6_EmptyScalarChomping)`.
#[test]
fn ex8_6_empty_scalar_chomping() -> Result<()> {
    let doc = load(EX8_6)?;
    assert_eq!(3, doc.size()?);
    assert_eq!(
        Bytes::from(b""),
        Bytes::from(doc.get(b"strip")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b""),
        Bytes::from(doc.get(b"clip")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"\n"),
        Bytes::from(doc.get(b"keep")?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex8_7_LiteralScalar)`.
#[test]
fn ex8_7_literal_scalar() -> Result<()> {
    let doc = load(EX8_7)?;
    assert_eq!(
        Bytes::from(b"literal\n\ttext\n"),
        Bytes::from(doc.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex8_8_LiteralContent)`.
#[test]
fn ex8_8_literal_content() -> Result<()> {
    let doc = load(EX8_8)?;
    assert_eq!(
        Bytes::from(b"\n\nliteral\n \n\ntext\n"),
        Bytes::from(doc.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex8_9_FoldedScalar)`.
#[test]
fn ex8_9_folded_scalar() -> Result<()> {
    let doc = load(EX8_9)?;
    assert_eq!(
        Bytes::from(b"folded text\n"),
        Bytes::from(doc.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex8_10_FoldedLines)`.
#[test]
fn ex8_10_folded_lines() -> Result<()> {
    let doc = load(EX8_10)?;
    assert!(
        Bytes::from(doc.as_::<Vec<u8>>()?)
            == Bytes::from(
                b"\nfolded line\nnext line\n  * bullet\n\n  * list\n  * lines\n\nlast line\n"
            )
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex8_11_MoreIndentedLines)`.
#[test]
fn ex8_11_more_indented_lines() -> Result<()> {
    let doc = load(EX8_11)?;
    assert!(
        Bytes::from(doc.as_::<Vec<u8>>()?)
            == Bytes::from(
                b"\nfolded line\nnext line\n  * bullet\n\n  * list\n  * lines\n\nlast line\n"
            )
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex8_12_EmptySeparationLines)`.
#[test]
fn ex8_12_empty_separation_lines() -> Result<()> {
    let doc = load(EX8_12)?;
    assert!(
        Bytes::from(doc.as_::<Vec<u8>>()?)
            == Bytes::from(
                b"\nfolded line\nnext line\n  * bullet\n\n  * list\n  * lines\n\nlast line\n"
            )
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex8_13_FinalEmptyLines)`.
#[test]
fn ex8_13_final_empty_lines() -> Result<()> {
    let doc = load(EX8_13)?;
    assert!(
        Bytes::from(doc.as_::<Vec<u8>>()?)
            == Bytes::from(
                b"\nfolded line\nnext line\n  * bullet\n\n  * list\n  * lines\n\nlast line\n"
            )
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex8_14_BlockSequence)`.
#[test]
fn ex8_14_block_sequence() -> Result<()> {
    let doc = load(EX8_14)?;
    assert_eq!(1, doc.size()?);
    assert_eq!(2, doc.get(b"block sequence")?.size()?);
    assert_eq!(
        Bytes::from(b"one"),
        Bytes::from(doc.get(b"block sequence")?.get(0)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(1, doc.get(b"block sequence")?.get(1)?.size()?);
    assert_eq!(
        Bytes::from(b"three"),
        Bytes::from(
            doc.get(b"block sequence")?
                .get(1)?
                .get(b"two")?
                .as_::<Vec<u8>>()?
        )
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex8_15_BlockSequenceEntryTypes)`.
#[test]
fn ex8_15_block_sequence_entry_types() -> Result<()> {
    let doc = load(EX8_15)?;
    assert_eq!(4, doc.size()?);
    assert!(doc.get(0)?.is_null()?);
    assert_eq!(
        Bytes::from(b"block node\n"),
        Bytes::from(doc.get(1)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(2, doc.get(2)?.size()?);
    assert_eq!(
        Bytes::from(b"one"),
        Bytes::from(doc.get(2)?.get(0)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"two"),
        Bytes::from(doc.get(2)?.get(1)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(1, doc.get(3)?.size()?);
    assert_eq!(
        Bytes::from(b"two"),
        Bytes::from(doc.get(3)?.get(b"one")?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex8_16_BlockMappings)`.
#[test]
fn ex8_16_block_mappings() -> Result<()> {
    let doc = load(EX8_16)?;
    assert_eq!(1, doc.size()?);
    assert_eq!(1, doc.get(b"block mapping")?.size()?);
    assert_eq!(
        Bytes::from(b"value"),
        Bytes::from(doc.get(b"block mapping")?.get(b"key")?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex8_17_ExplicitBlockMappingEntries)`.
#[test]
fn ex8_17_explicit_block_mapping_entries() -> Result<()> {
    let doc = load(EX8_17)?;
    assert_eq!(2, doc.size()?);
    assert!(doc.get(b"explicit key")?.is_null()?);
    assert_eq!(2, doc.get(b"block key\n")?.size()?);
    assert_eq!(
        Bytes::from(b"one"),
        Bytes::from(doc.get(b"block key\n")?.get(0)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"two"),
        Bytes::from(doc.get(b"block key\n")?.get(1)?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex8_18_ImplicitBlockMappingEntries)`.
#[test]
fn ex8_18_implicit_block_mapping_entries() -> Result<()> {
    let doc = load(EX8_18)?;
    assert_eq!(3, doc.size()?);
    assert_eq!(
        Bytes::from(b"in-line value"),
        Bytes::from(doc.get(b"plain key")?.as_::<Vec<u8>>()?)
    );
    assert!(doc.get(Key::Null)?.is_null()?);
    assert_eq!(1, doc.get(b"quoted key")?.size()?);
    assert_eq!(
        Bytes::from(b"entry"),
        Bytes::from(doc.get(b"quoted key")?.get(0)?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex8_19_CompactBlockMappings)`.
#[test]
fn ex8_19_compact_block_mappings() -> Result<()> {
    let doc = load(EX8_19)?;
    assert_eq!(2, doc.size()?);
    assert_eq!(1, doc.get(0)?.size()?);
    assert_eq!(
        Bytes::from(b"yellow"),
        Bytes::from(doc.get(0)?.get(b"sun")?.as_::<Vec<u8>>()?)
    );
    assert_eq!(1, doc.get(1)?.size()?);
    let key: &[(&[u8], &[u8])] = &[(b"earth", b"blue")];
    assert_eq!(1, doc.get(1)?.get(Key::StrMap(key))?.size()?);
    assert_eq!(
        Bytes::from(b"white"),
        Bytes::from(
            doc.get(1)?
                .get(Key::StrMap(key))?
                .get(b"moon")?
                .as_::<Vec<u8>>()?
        )
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex8_20_BlockNodeTypes)`.
#[test]
fn ex8_20_block_node_types() -> Result<()> {
    let doc = load(EX8_20)?;
    assert_eq!(3, doc.size()?);
    assert_eq!(
        Bytes::from(b"flow in block"),
        Bytes::from(doc.get(0)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(
        Bytes::from(b"Block scalar\n"),
        Bytes::from(doc.get(1)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(1, doc.get(2)?.size()?);
    assert_eq!(
        Bytes::from(b"bar"),
        Bytes::from(doc.get(2)?.get(b"foo")?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, Ex8_22_BlockCollectionNodes)`.
#[test]
fn ex8_22_block_collection_nodes() -> Result<()> {
    let doc = load(EX8_22)?;
    assert_eq!(2, doc.size()?);
    assert_eq!(2, doc.get(b"sequence")?.size()?);
    assert_eq!(
        Bytes::from(b"entry"),
        Bytes::from(doc.get(b"sequence")?.get(0)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(1, doc.get(b"sequence")?.get(1)?.size()?);
    assert_eq!(
        Bytes::from(b"nested"),
        Bytes::from(doc.get(b"sequence")?.get(1)?.get(0)?.as_::<Vec<u8>>()?)
    );
    assert_eq!(1, doc.get(b"mapping")?.size()?);
    assert_eq!(
        Bytes::from(b"bar"),
        Bytes::from(doc.get(b"mapping")?.get(b"foo")?.as_::<Vec<u8>>()?)
    );
    Ok(())
}

/// Port of yaml-cpp 0.8.0 `TEST(NodeSpecTest, FlowMapNotClosed)`.
#[test]
fn flow_map_not_closed() -> Result<()> {
    expect_load_parser_exception(b"{x:", error_msg::UNKNOWN_TOKEN.as_bytes());
    Ok(())
}
