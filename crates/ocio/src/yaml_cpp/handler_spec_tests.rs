// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.8.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! Port of yaml-cpp 0.8.0 `test/integration/handler_spec_test.cpp`: the events of each
//! specification example, with the expectations copied from the test. Its four `DISABLED_`
//! tests (`Ex6_25_InvalidVerbatimTags`, `Ex6_27b_InvalidTagShorthands`,
//! `Ex8_5_ChompingTrailingLines`, `Ex8_21_BlockScalarNodes`) are disabled upstream and are
//! not ported.

use super::super::event_handler::EmitterStyle;
use super::super::exceptions::error_msg;
use super::super::handler_test::*;
use super::super::spec_examples::*;

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex2_1_SeqScalars)`.
#[test]
fn ex2_1_seq_scalars() {
    expect_events(
        EX2_1,
        &[
            doc_start(),
            seq_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"Mark McGwire"),
            scalar(b"?", 0, b"Sammy Sosa"),
            scalar(b"?", 0, b"Ken Griffey"),
            seq_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex2_2_MappingScalarsToScalars)`.
#[test]
fn ex2_2_mapping_scalars_to_scalars() {
    expect_events(
        EX2_2,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"hr"),
            scalar(b"?", 0, b"65"),
            scalar(b"?", 0, b"avg"),
            scalar(b"?", 0, b"0.278"),
            scalar(b"?", 0, b"rbi"),
            scalar(b"?", 0, b"147"),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex2_3_MappingScalarsToSequences)`.
#[test]
fn ex2_3_mapping_scalars_to_sequences() {
    expect_events(
        EX2_3,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"american"),
            seq_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"Boston Red Sox"),
            scalar(b"?", 0, b"Detroit Tigers"),
            scalar(b"?", 0, b"New York Yankees"),
            seq_end(),
            scalar(b"?", 0, b"national"),
            seq_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"New York Mets"),
            scalar(b"?", 0, b"Chicago Cubs"),
            scalar(b"?", 0, b"Atlanta Braves"),
            seq_end(),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex2_4_SequenceOfMappings)`.
#[test]
fn ex2_4_sequence_of_mappings() {
    expect_events(
        EX2_4,
        &[
            doc_start(),
            seq_start(b"?", 0, EmitterStyle::Block),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"name"),
            scalar(b"?", 0, b"Mark McGwire"),
            scalar(b"?", 0, b"hr"),
            scalar(b"?", 0, b"65"),
            scalar(b"?", 0, b"avg"),
            scalar(b"?", 0, b"0.278"),
            map_end(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"name"),
            scalar(b"?", 0, b"Sammy Sosa"),
            scalar(b"?", 0, b"hr"),
            scalar(b"?", 0, b"63"),
            scalar(b"?", 0, b"avg"),
            scalar(b"?", 0, b"0.288"),
            map_end(),
            seq_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex2_5_SequenceOfSequences)`.
#[test]
fn ex2_5_sequence_of_sequences() {
    expect_events(
        EX2_5,
        &[
            doc_start(),
            seq_start(b"?", 0, EmitterStyle::Block),
            seq_start(b"?", 0, EmitterStyle::Flow),
            scalar(b"?", 0, b"name"),
            scalar(b"?", 0, b"hr"),
            scalar(b"?", 0, b"avg"),
            seq_end(),
            seq_start(b"?", 0, EmitterStyle::Flow),
            scalar(b"?", 0, b"Mark McGwire"),
            scalar(b"?", 0, b"65"),
            scalar(b"?", 0, b"0.278"),
            seq_end(),
            seq_start(b"?", 0, EmitterStyle::Flow),
            scalar(b"?", 0, b"Sammy Sosa"),
            scalar(b"?", 0, b"63"),
            scalar(b"?", 0, b"0.288"),
            seq_end(),
            seq_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex2_6_MappingOfMappings)`.
#[test]
fn ex2_6_mapping_of_mappings() {
    expect_events(
        EX2_6,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"Mark McGwire"),
            map_start(b"?", 0, EmitterStyle::Flow),
            scalar(b"?", 0, b"hr"),
            scalar(b"?", 0, b"65"),
            scalar(b"?", 0, b"avg"),
            scalar(b"?", 0, b"0.278"),
            map_end(),
            scalar(b"?", 0, b"Sammy Sosa"),
            map_start(b"?", 0, EmitterStyle::Flow),
            scalar(b"?", 0, b"hr"),
            scalar(b"?", 0, b"63"),
            scalar(b"?", 0, b"avg"),
            scalar(b"?", 0, b"0.288"),
            map_end(),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex2_7_TwoDocumentsInAStream)`.
#[test]
fn ex2_7_two_documents_in_a_stream() {
    expect_events(
        EX2_7,
        &[
            doc_start(),
            seq_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"Mark McGwire"),
            scalar(b"?", 0, b"Sammy Sosa"),
            scalar(b"?", 0, b"Ken Griffey"),
            seq_end(),
            doc_end(),
            doc_start(),
            seq_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"Chicago Cubs"),
            scalar(b"?", 0, b"St Louis Cardinals"),
            seq_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex2_8_PlayByPlayFeed)`.
#[test]
fn ex2_8_play_by_play_feed() {
    expect_events(
        EX2_8,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"time"),
            scalar(b"?", 0, b"20:03:20"),
            scalar(b"?", 0, b"player"),
            scalar(b"?", 0, b"Sammy Sosa"),
            scalar(b"?", 0, b"action"),
            scalar(b"?", 0, b"strike (miss)"),
            map_end(),
            doc_end(),
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"time"),
            scalar(b"?", 0, b"20:03:47"),
            scalar(b"?", 0, b"player"),
            scalar(b"?", 0, b"Sammy Sosa"),
            scalar(b"?", 0, b"action"),
            scalar(b"?", 0, b"grand slam"),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex2_9_SingleDocumentWithTwoComments)`.
#[test]
fn ex2_9_single_document_with_two_comments() {
    expect_events(
        EX2_9,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"hr"),
            seq_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"Mark McGwire"),
            scalar(b"?", 0, b"Sammy Sosa"),
            seq_end(),
            scalar(b"?", 0, b"rbi"),
            seq_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"Sammy Sosa"),
            scalar(b"?", 0, b"Ken Griffey"),
            seq_end(),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex2_10_SimpleAnchor)`.
#[test]
fn ex2_10_simple_anchor() {
    expect_events(
        EX2_10,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"hr"),
            seq_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"Mark McGwire"),
            anchor(b"SS"),
            scalar(b"?", 1, b"Sammy Sosa"),
            seq_end(),
            scalar(b"?", 0, b"rbi"),
            seq_start(b"?", 0, EmitterStyle::Block),
            alias(1),
            scalar(b"?", 0, b"Ken Griffey"),
            seq_end(),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex2_11_MappingBetweenSequences)`.
#[test]
fn ex2_11_mapping_between_sequences() {
    expect_events(
        EX2_11,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            seq_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"Detroit Tigers"),
            scalar(b"?", 0, b"Chicago cubs"),
            seq_end(),
            seq_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"2001-07-23"),
            seq_end(),
            seq_start(b"?", 0, EmitterStyle::Flow),
            scalar(b"?", 0, b"New York Yankees"),
            scalar(b"?", 0, b"Atlanta Braves"),
            seq_end(),
            seq_start(b"?", 0, EmitterStyle::Flow),
            scalar(b"?", 0, b"2001-07-02"),
            scalar(b"?", 0, b"2001-08-12"),
            scalar(b"?", 0, b"2001-08-14"),
            seq_end(),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex2_12_CompactNestedMapping)`.
#[test]
fn ex2_12_compact_nested_mapping() {
    expect_events(
        EX2_12,
        &[
            doc_start(),
            seq_start(b"?", 0, EmitterStyle::Block),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"item"),
            scalar(b"?", 0, b"Super Hoop"),
            scalar(b"?", 0, b"quantity"),
            scalar(b"?", 0, b"1"),
            map_end(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"item"),
            scalar(b"?", 0, b"Basketball"),
            scalar(b"?", 0, b"quantity"),
            scalar(b"?", 0, b"4"),
            map_end(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"item"),
            scalar(b"?", 0, b"Big Shoes"),
            scalar(b"?", 0, b"quantity"),
            scalar(b"?", 0, b"1"),
            map_end(),
            seq_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex2_13_InLiteralsNewlinesArePreserved)`.
#[test]
fn ex2_13_in_literals_newlines_are_preserved() {
    expect_events(
        EX2_13,
        &[
            doc_start(),
            scalar(b"!", 0, b"\\//||\\/||\n// ||  ||__"),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex2_14_InFoldedScalarsNewlinesBecomeSpaces)`.
#[test]
fn ex2_14_in_folded_scalars_newlines_become_spaces() {
    expect_events(
        EX2_14,
        &[
            doc_start(),
            scalar(
                b"!",
                0,
                b"Mark McGwire's year was crippled by a knee injury.",
            ),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex2_15_FoldedNewlinesArePreservedForMoreIndentedAndBlankLines)`.
#[test]
fn ex2_15_folded_newlines_are_preserved_for_more_indented_and_blank_lines() {
    expect_events(
        EX2_15,
        &[
            doc_start(),
            scalar(b"!", 0, b"Sammy Sosa completed another fine season with great stats.\n\n  63 Home Runs\n  0.288 Batting Average\n\nWhat a year!"),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex2_16_IndentationDeterminesScope)`.
#[test]
fn ex2_16_indentation_determines_scope() {
    expect_events(
        EX2_16,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"name"),
            scalar(b"?", 0, b"Mark McGwire"),
            scalar(b"?", 0, b"accomplishment"),
            scalar(
                b"!",
                0,
                b"Mark set a major league home run record in 1998.\n",
            ),
            scalar(b"?", 0, b"stats"),
            scalar(b"!", 0, b"65 Home Runs\n0.278 Batting Average\n"),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex2_17_QuotedScalars)`.
#[test]
fn ex2_17_quoted_scalars() {
    expect_events(
        EX2_17,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"unicode"),
            scalar(b"!", 0, b"Sosa did fine.\xE2\x98\xBA"),
            scalar(b"?", 0, b"control"),
            scalar(b"!", 0, b"\x081998\t1999\t2000\n"),
            scalar(b"?", 0, b"hex esc"),
            scalar(b"!", 0, b"\r\n is \r\n"),
            scalar(b"?", 0, b"single"),
            scalar(b"!", 0, b"\"Howdy!\" he cried."),
            scalar(b"?", 0, b"quoted"),
            scalar(b"!", 0, b" # Not a 'comment'."),
            scalar(b"?", 0, b"tie-fighter"),
            scalar(b"!", 0, b"|\\-*-/|"),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex2_18_MultiLineFlowScalars)`.
#[test]
fn ex2_18_multi_line_flow_scalars() {
    expect_events(
        EX2_18,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"plain"),
            scalar(b"?", 0, b"This unquoted scalar spans many lines."),
            scalar(b"?", 0, b"quoted"),
            scalar(b"!", 0, b"So does this quoted scalar.\n"),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex2_19_Integers)`.
#[test]
fn ex2_19_integers() {
    expect_events(
        EX2_19,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"canonical"),
            scalar(b"?", 0, b"12345"),
            scalar(b"?", 0, b"decimal"),
            scalar(b"?", 0, b"+12345"),
            scalar(b"?", 0, b"octal"),
            scalar(b"?", 0, b"0o14"),
            scalar(b"?", 0, b"hexadecimal"),
            scalar(b"?", 0, b"0xC"),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex2_20_FloatingPoint)`.
#[test]
fn ex2_20_floating_point() {
    expect_events(
        EX2_20,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"canonical"),
            scalar(b"?", 0, b"1.23015e+3"),
            scalar(b"?", 0, b"exponential"),
            scalar(b"?", 0, b"12.3015e+02"),
            scalar(b"?", 0, b"fixed"),
            scalar(b"?", 0, b"1230.15"),
            scalar(b"?", 0, b"negative infinity"),
            scalar(b"?", 0, b"-.inf"),
            scalar(b"?", 0, b"not a number"),
            scalar(b"?", 0, b".NaN"),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex2_21_Miscellaneous)`.
#[test]
fn ex2_21_miscellaneous() {
    expect_events(
        EX2_21,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            null(0),
            null(0),
            scalar(b"?", 0, b"booleans"),
            seq_start(b"?", 0, EmitterStyle::Flow),
            scalar(b"?", 0, b"true"),
            scalar(b"?", 0, b"false"),
            seq_end(),
            scalar(b"?", 0, b"string"),
            scalar(b"!", 0, b"012345"),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex2_22_Timestamps)`.
#[test]
fn ex2_22_timestamps() {
    expect_events(
        EX2_22,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"canonical"),
            scalar(b"?", 0, b"2001-12-15T02:59:43.1Z"),
            scalar(b"?", 0, b"iso8601"),
            scalar(b"?", 0, b"2001-12-14t21:59:43.10-05:00"),
            scalar(b"?", 0, b"spaced"),
            scalar(b"?", 0, b"2001-12-14 21:59:43.10 -5"),
            scalar(b"?", 0, b"date"),
            scalar(b"?", 0, b"2002-12-14"),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex2_23_VariousExplicitTags)`.
#[test]
fn ex2_23_various_explicit_tags() {
    expect_events(
        EX2_23,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"not-date"),
            scalar(b"tag:yaml.org,2002:str", 0, b"2002-04-28"),
            scalar(b"?", 0, b"picture"),
            scalar(b"tag:yaml.org,2002:binary", 0, b"R0lGODlhDAAMAIQAAP//9/X\n17unp5WZmZgAAAOfn515eXv\nPz7Y6OjuDg4J+fn5OTk6enp\n56enmleECcgggoBADs=\n"),
            scalar(b"?", 0, b"application specific tag"),
            scalar(b"!something", 0, b"The semantics of the tag\nabove may be different for\ndifferent documents."),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex2_24_GlobalTags)`.
#[test]
fn ex2_24_global_tags() {
    expect_events(
        EX2_24,
        &[
            doc_start(),
            seq_start(b"tag:clarkevans.com,2002:shape", 0, EmitterStyle::Block),
            map_start(b"tag:clarkevans.com,2002:circle", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"center"),
            anchor(b"ORIGIN"),
            map_start(b"?", 1, EmitterStyle::Flow),
            scalar(b"?", 0, b"x"),
            scalar(b"?", 0, b"73"),
            scalar(b"?", 0, b"y"),
            scalar(b"?", 0, b"129"),
            map_end(),
            scalar(b"?", 0, b"radius"),
            scalar(b"?", 0, b"7"),
            map_end(),
            map_start(b"tag:clarkevans.com,2002:line", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"start"),
            alias(1),
            scalar(b"?", 0, b"finish"),
            map_start(b"?", 0, EmitterStyle::Flow),
            scalar(b"?", 0, b"x"),
            scalar(b"?", 0, b"89"),
            scalar(b"?", 0, b"y"),
            scalar(b"?", 0, b"102"),
            map_end(),
            map_end(),
            map_start(b"tag:clarkevans.com,2002:label", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"start"),
            alias(1),
            scalar(b"?", 0, b"color"),
            scalar(b"?", 0, b"0xFFEEBB"),
            scalar(b"?", 0, b"text"),
            scalar(b"?", 0, b"Pretty vector drawing."),
            map_end(),
            seq_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex2_25_UnorderedSets)`.
#[test]
fn ex2_25_unordered_sets() {
    expect_events(
        EX2_25,
        &[
            doc_start(),
            map_start(b"tag:yaml.org,2002:set", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"Mark McGwire"),
            null(0),
            scalar(b"?", 0, b"Sammy Sosa"),
            null(0),
            scalar(b"?", 0, b"Ken Griffey"),
            null(0),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex2_26_OrderedMappings)`.
#[test]
fn ex2_26_ordered_mappings() {
    expect_events(
        EX2_26,
        &[
            doc_start(),
            seq_start(b"tag:yaml.org,2002:omap", 0, EmitterStyle::Block),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"Mark McGwire"),
            scalar(b"?", 0, b"65"),
            map_end(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"Sammy Sosa"),
            scalar(b"?", 0, b"63"),
            map_end(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"Ken Griffey"),
            scalar(b"?", 0, b"58"),
            map_end(),
            seq_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex2_27_Invoice)`.
#[test]
fn ex2_27_invoice() {
    expect_events(
        EX2_27,
        &[
            doc_start(),
            map_start(b"tag:clarkevans.com,2002:invoice", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"invoice"),
            scalar(b"?", 0, b"34843"),
            scalar(b"?", 0, b"date"),
            scalar(b"?", 0, b"2001-01-23"),
            scalar(b"?", 0, b"bill-to"),
            anchor(b"id001"),
            map_start(b"?", 1, EmitterStyle::Block),
            scalar(b"?", 0, b"given"),
            scalar(b"?", 0, b"Chris"),
            scalar(b"?", 0, b"family"),
            scalar(b"?", 0, b"Dumars"),
            scalar(b"?", 0, b"address"),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"lines"),
            scalar(b"!", 0, b"458 Walkman Dr.\nSuite #292\n"),
            scalar(b"?", 0, b"city"),
            scalar(b"?", 0, b"Royal Oak"),
            scalar(b"?", 0, b"state"),
            scalar(b"?", 0, b"MI"),
            scalar(b"?", 0, b"postal"),
            scalar(b"?", 0, b"48046"),
            map_end(),
            map_end(),
            scalar(b"?", 0, b"ship-to"),
            alias(1),
            scalar(b"?", 0, b"product"),
            seq_start(b"?", 0, EmitterStyle::Block),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"sku"),
            scalar(b"?", 0, b"BL394D"),
            scalar(b"?", 0, b"quantity"),
            scalar(b"?", 0, b"4"),
            scalar(b"?", 0, b"description"),
            scalar(b"?", 0, b"Basketball"),
            scalar(b"?", 0, b"price"),
            scalar(b"?", 0, b"450.00"),
            map_end(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"sku"),
            scalar(b"?", 0, b"BL4438H"),
            scalar(b"?", 0, b"quantity"),
            scalar(b"?", 0, b"1"),
            scalar(b"?", 0, b"description"),
            scalar(b"?", 0, b"Super Hoop"),
            scalar(b"?", 0, b"price"),
            scalar(b"?", 0, b"2392.00"),
            map_end(),
            seq_end(),
            scalar(b"?", 0, b"tax"),
            scalar(b"?", 0, b"251.42"),
            scalar(b"?", 0, b"total"),
            scalar(b"?", 0, b"4443.52"),
            scalar(b"?", 0, b"comments"),
            scalar(
                b"?",
                0,
                b"Late afternoon is best. Backup contact is Nancy Billsmer @ 338-4338.",
            ),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex2_28_LogFile)`.
#[test]
fn ex2_28_log_file() {
    expect_events(
        EX2_28,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"Time"),
            scalar(b"?", 0, b"2001-11-23 15:01:42 -5"),
            scalar(b"?", 0, b"User"),
            scalar(b"?", 0, b"ed"),
            scalar(b"?", 0, b"Warning"),
            scalar(b"?", 0, b"This is an error message for the log file"),
            map_end(),
            doc_end(),
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"Time"),
            scalar(b"?", 0, b"2001-11-23 15:02:31 -5"),
            scalar(b"?", 0, b"User"),
            scalar(b"?", 0, b"ed"),
            scalar(b"?", 0, b"Warning"),
            scalar(b"?", 0, b"A slightly different error message."),
            map_end(),
            doc_end(),
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"Date"),
            scalar(b"?", 0, b"2001-11-23 15:03:17 -5"),
            scalar(b"?", 0, b"User"),
            scalar(b"?", 0, b"ed"),
            scalar(b"?", 0, b"Fatal"),
            scalar(b"?", 0, b"Unknown variable \"bar\""),
            scalar(b"?", 0, b"Stack"),
            seq_start(b"?", 0, EmitterStyle::Block),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"file"),
            scalar(b"?", 0, b"TopClass.py"),
            scalar(b"?", 0, b"line"),
            scalar(b"?", 0, b"23"),
            scalar(b"?", 0, b"code"),
            scalar(b"!", 0, b"x = MoreObject(\"345\\n\")\n"),
            map_end(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"file"),
            scalar(b"?", 0, b"MoreClass.py"),
            scalar(b"?", 0, b"line"),
            scalar(b"?", 0, b"58"),
            scalar(b"?", 0, b"code"),
            scalar(b"!", 0, b"foo = bar"),
            map_end(),
            seq_end(),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex5_3_BlockStructureIndicators)`.
#[test]
fn ex5_3_block_structure_indicators() {
    expect_events(
        EX5_3,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"sequence"),
            seq_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"one"),
            scalar(b"?", 0, b"two"),
            seq_end(),
            scalar(b"?", 0, b"mapping"),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"sky"),
            scalar(b"?", 0, b"blue"),
            scalar(b"?", 0, b"sea"),
            scalar(b"?", 0, b"green"),
            map_end(),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex5_4_FlowStructureIndicators)`.
#[test]
fn ex5_4_flow_structure_indicators() {
    expect_events(
        EX5_4,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"sequence"),
            seq_start(b"?", 0, EmitterStyle::Flow),
            scalar(b"?", 0, b"one"),
            scalar(b"?", 0, b"two"),
            seq_end(),
            scalar(b"?", 0, b"mapping"),
            map_start(b"?", 0, EmitterStyle::Flow),
            scalar(b"?", 0, b"sky"),
            scalar(b"?", 0, b"blue"),
            scalar(b"?", 0, b"sea"),
            scalar(b"?", 0, b"green"),
            map_end(),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex5_5_CommentIndicator)`.
#[test]
fn ex5_5_comment_indicator() {
    expect_events(EX5_5, &[]);
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex5_6_NodePropertyIndicators)`.
#[test]
fn ex5_6_node_property_indicators() {
    expect_events(
        EX5_6,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"anchored"),
            anchor(b"anchor"),
            scalar(b"!local", 1, b"value"),
            scalar(b"?", 0, b"alias"),
            alias(1),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex5_7_BlockScalarIndicators)`.
#[test]
fn ex5_7_block_scalar_indicators() {
    expect_events(
        EX5_7,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"literal"),
            scalar(b"!", 0, b"some\ntext\n"),
            scalar(b"?", 0, b"folded"),
            scalar(b"!", 0, b"some text\n"),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex5_8_QuotedScalarIndicators)`.
#[test]
fn ex5_8_quoted_scalar_indicators() {
    expect_events(
        EX5_8,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"single"),
            scalar(b"!", 0, b"text"),
            scalar(b"?", 0, b"double"),
            scalar(b"!", 0, b"text"),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex5_11_LineBreakCharacters)`.
#[test]
fn ex5_11_line_break_characters() {
    expect_events(
        EX5_11,
        &[
            doc_start(),
            scalar(b"!", 0, b"Line break (no glyph)\nLine break (glyphed)\n"),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex5_12_TabsAndSpaces)`.
#[test]
fn ex5_12_tabs_and_spaces() {
    expect_events(
        EX5_12,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"quoted"),
            scalar(b"!", 0, b"Quoted\t"),
            scalar(b"?", 0, b"block"),
            scalar(
                b"!",
                0,
                b"void main() {\n\tprintf(\"Hello, world!\\n\");\n}",
            ),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex5_13_EscapedCharacters)`.
#[test]
fn ex5_13_escaped_characters() {
    expect_events(
        EX5_13,
        &[
            doc_start(),
            scalar(
                b"!",
                0,
                [
                    b"Fun with \\ \" \x07 \x08 \x1B \x0C \n \r \t \x0B ".as_slice(),
                    b"\x00".as_slice(),
                    b"   \xA0 \x85 \xE2\x80\xA8 \xE2\x80\xA9 A A A".as_slice(),
                ]
                .concat(),
            ),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex5_14_InvalidEscapedCharacters)`.
#[test]
fn ex5_14_invalid_escaped_characters() {
    expect_parser_exception(
        EX5_14,
        [error_msg::INVALID_ESCAPE.as_bytes(), b"c".as_slice()].concat(),
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex6_1_IndentationSpaces)`.
#[test]
fn ex6_1_indentation_spaces() {
    expect_events(
        EX6_1,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"Not indented"),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"By one space"),
            scalar(b"!", 0, b"By four\n  spaces\n"),
            scalar(b"?", 0, b"Flow style"),
            seq_start(b"?", 0, EmitterStyle::Flow),
            scalar(b"?", 0, b"By two"),
            scalar(b"?", 0, b"Also by two"),
            scalar(b"?", 0, b"Still by two"),
            seq_end(),
            map_end(),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex6_2_IndentationIndicators)`.
#[test]
fn ex6_2_indentation_indicators() {
    expect_events(
        EX6_2,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"a"),
            seq_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"b"),
            seq_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"c"),
            scalar(b"?", 0, b"d"),
            seq_end(),
            seq_end(),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex6_3_SeparationSpaces)`.
#[test]
fn ex6_3_separation_spaces() {
    expect_events(
        EX6_3,
        &[
            doc_start(),
            seq_start(b"?", 0, EmitterStyle::Block),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"foo"),
            scalar(b"?", 0, b"bar"),
            map_end(),
            seq_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"baz"),
            scalar(b"?", 0, b"baz"),
            seq_end(),
            seq_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex6_4_LinePrefixes)`.
#[test]
fn ex6_4_line_prefixes() {
    expect_events(
        EX6_4,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"plain"),
            scalar(b"?", 0, b"text lines"),
            scalar(b"?", 0, b"quoted"),
            scalar(b"!", 0, b"text lines"),
            scalar(b"?", 0, b"block"),
            scalar(b"!", 0, b"text\n \tlines\n"),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex6_5_EmptyLines)`.
#[test]
fn ex6_5_empty_lines() {
    expect_events(
        EX6_5,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"Folding"),
            scalar(b"!", 0, b"Empty line\nas a line feed"),
            scalar(b"?", 0, b"Chomping"),
            scalar(b"!", 0, b"Clipped empty lines\n"),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex6_6_LineFolding)`.
#[test]
fn ex6_6_line_folding() {
    expect_events(
        EX6_6,
        &[
            doc_start(),
            scalar(b"!", 0, b"trimmed\n\n\nas space"),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex6_7_BlockFolding)`.
#[test]
fn ex6_7_block_folding() {
    expect_events(
        EX6_7,
        &[
            doc_start(),
            scalar(b"!", 0, b"foo \n\n\t bar\n\nbaz\n"),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex6_8_FlowFolding)`.
#[test]
fn ex6_8_flow_folding() {
    expect_events(
        EX6_8,
        &[doc_start(), scalar(b"!", 0, b" foo\nbar\nbaz "), doc_end()],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex6_9_SeparatedComment)`.
#[test]
fn ex6_9_separated_comment() {
    expect_events(
        EX6_9,
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

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex6_10_CommentLines)`.
#[test]
fn ex6_10_comment_lines() {
    expect_events(EX6_10, &[]);
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, _MultiLineComments)`.
#[test]
fn _multi_line_comments() {
    expect_events(
        EX6_11,
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

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex6_12_SeparationSpacesII)`.
#[test]
fn ex6_12_separation_spaces_ii() {
    expect_events(
        EX6_12,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            map_start(b"?", 0, EmitterStyle::Flow),
            scalar(b"?", 0, b"first"),
            scalar(b"?", 0, b"Sammy"),
            scalar(b"?", 0, b"last"),
            scalar(b"?", 0, b"Sosa"),
            map_end(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"hr"),
            scalar(b"?", 0, b"65"),
            scalar(b"?", 0, b"avg"),
            scalar(b"?", 0, b"0.278"),
            map_end(),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex6_13_ReservedDirectives)`.
#[test]
fn ex6_13_reserved_directives() {
    expect_events(EX6_13, &[doc_start(), scalar(b"!", 0, b"foo"), doc_end()]);
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex6_14_YAMLDirective)`.
#[test]
fn ex6_14_yaml_directive() {
    expect_events(EX6_14, &[doc_start(), scalar(b"!", 0, b"foo"), doc_end()]);
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex6_15_InvalidRepeatedYAMLDirective)`.
#[test]
fn ex6_15_invalid_repeated_yaml_directive() {
    expect_parser_exception(EX6_15, error_msg::REPEATED_YAML_DIRECTIVE.as_bytes());
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex6_16_TagDirective)`.
#[test]
fn ex6_16_tag_directive() {
    expect_events(
        EX6_16,
        &[
            doc_start(),
            scalar(b"tag:yaml.org,2002:str", 0, b"foo"),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex6_17_InvalidRepeatedTagDirective)`.
#[test]
fn ex6_17_invalid_repeated_tag_directive() {
    expect_parser_exception(EX6_17, error_msg::REPEATED_TAG_DIRECTIVE.as_bytes());
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex6_18_PrimaryTagHandle)`.
#[test]
fn ex6_18_primary_tag_handle() {
    expect_events(
        EX6_18,
        &[
            doc_start(),
            scalar(b"!foo", 0, b"bar"),
            doc_end(),
            doc_start(),
            scalar(b"tag:example.com,2000:app/foo", 0, b"bar"),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex6_19_SecondaryTagHandle)`.
#[test]
fn ex6_19_secondary_tag_handle() {
    expect_events(
        EX6_19,
        &[
            doc_start(),
            scalar(b"tag:example.com,2000:app/int", 0, b"1 - 3"),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex6_20_TagHandles)`.
#[test]
fn ex6_20_tag_handles() {
    expect_events(
        EX6_20,
        &[
            doc_start(),
            scalar(b"tag:example.com,2000:app/foo", 0, b"bar"),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex6_21_LocalTagPrefix)`.
#[test]
fn ex6_21_local_tag_prefix() {
    expect_events(
        EX6_21,
        &[
            doc_start(),
            scalar(b"!my-light", 0, b"fluorescent"),
            doc_end(),
            doc_start(),
            scalar(b"!my-light", 0, b"green"),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex6_22_GlobalTagPrefix)`.
#[test]
fn ex6_22_global_tag_prefix() {
    expect_events(
        EX6_22,
        &[
            doc_start(),
            seq_start(b"?", 0, EmitterStyle::Block),
            scalar(b"tag:example.com,2000:app/foo", 0, b"bar"),
            seq_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex6_23_NodeProperties)`.
#[test]
fn ex6_23_node_properties() {
    expect_events(
        EX6_23,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            anchor(b"a1"),
            scalar(b"tag:yaml.org,2002:str", 1, b"foo"),
            scalar(b"tag:yaml.org,2002:str", 0, b"bar"),
            anchor(b"a2"),
            scalar(b"?", 2, b"baz"),
            alias(1),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex6_24_VerbatimTags)`.
#[test]
fn ex6_24_verbatim_tags() {
    expect_events(
        EX6_24,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"tag:yaml.org,2002:str", 0, b"foo"),
            scalar(b"!bar", 0, b"baz"),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex6_26_TagShorthands)`.
#[test]
fn ex6_26_tag_shorthands() {
    expect_events(
        EX6_26,
        &[
            doc_start(),
            seq_start(b"?", 0, EmitterStyle::Block),
            scalar(b"!local", 0, b"foo"),
            scalar(b"tag:yaml.org,2002:str", 0, b"bar"),
            scalar(b"tag:example.com,2000:app/tag%21", 0, b"baz"),
            seq_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex6_27a_InvalidTagShorthands)`.
#[test]
fn ex6_27a_invalid_tag_shorthands() {
    expect_parser_exception(EX6_27A, error_msg::TAG_WITH_NO_SUFFIX.as_bytes());
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex6_28_NonSpecificTags)`.
#[test]
fn ex6_28_non_specific_tags() {
    expect_events(
        EX6_28,
        &[
            doc_start(),
            seq_start(b"?", 0, EmitterStyle::Block),
            scalar(b"!", 0, b"12"),
            scalar(b"?", 0, b"12"),
            scalar(b"!", 0, b"12"),
            seq_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex6_29_NodeAnchors)`.
#[test]
fn ex6_29_node_anchors() {
    expect_events(
        EX6_29,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"First occurrence"),
            anchor(b"anchor"),
            scalar(b"?", 1, b"Value"),
            scalar(b"?", 0, b"Second occurrence"),
            alias(1),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex7_1_AliasNodes)`.
#[test]
fn ex7_1_alias_nodes() {
    expect_events(
        EX7_1,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"First occurrence"),
            anchor(b"anchor"),
            scalar(b"?", 1, b"Foo"),
            scalar(b"?", 0, b"Second occurrence"),
            alias(1),
            scalar(b"?", 0, b"Override anchor"),
            anchor(b"anchor"),
            scalar(b"?", 2, b"Bar"),
            scalar(b"?", 0, b"Reuse anchor"),
            alias(2),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex7_2_EmptyNodes)`.
#[test]
fn ex7_2_empty_nodes() {
    expect_events(
        EX7_2,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Flow),
            scalar(b"?", 0, b"foo"),
            scalar(b"tag:yaml.org,2002:str", 0, b""),
            scalar(b"tag:yaml.org,2002:str", 0, b""),
            scalar(b"?", 0, b"bar"),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex7_3_CompletelyEmptyNodes)`.
#[test]
fn ex7_3_completely_empty_nodes() {
    expect_events(
        EX7_3,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Flow),
            scalar(b"?", 0, b"foo"),
            null(0),
            null(0),
            scalar(b"?", 0, b"bar"),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex7_4_DoubleQuotedImplicitKeys)`.
#[test]
fn ex7_4_double_quoted_implicit_keys() {
    expect_events(
        EX7_4,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"!", 0, b"implicit block key"),
            seq_start(b"?", 0, EmitterStyle::Flow),
            map_start(b"?", 0, EmitterStyle::Flow),
            scalar(b"!", 0, b"implicit flow key"),
            scalar(b"?", 0, b"value"),
            map_end(),
            seq_end(),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex7_5_DoubleQuotedLineBreaks)`.
#[test]
fn ex7_5_double_quoted_line_breaks() {
    expect_events(
        EX7_5,
        &[
            doc_start(),
            scalar(
                b"!",
                0,
                b"folded to a space,\nto a line feed, or \t \tnon-content",
            ),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex7_6_DoubleQuotedLines)`.
#[test]
fn ex7_6_double_quoted_lines() {
    expect_events(
        EX7_6,
        &[
            doc_start(),
            scalar(b"!", 0, b" 1st non-empty\n2nd non-empty 3rd non-empty "),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex7_7_SingleQuotedCharacters)`.
#[test]
fn ex7_7_single_quoted_characters() {
    expect_events(
        EX7_7,
        &[
            doc_start(),
            scalar(b"!", 0, b"here's to \"quotes\""),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex7_8_SingleQuotedImplicitKeys)`.
#[test]
fn ex7_8_single_quoted_implicit_keys() {
    expect_events(
        EX7_8,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"!", 0, b"implicit block key"),
            seq_start(b"?", 0, EmitterStyle::Flow),
            map_start(b"?", 0, EmitterStyle::Flow),
            scalar(b"!", 0, b"implicit flow key"),
            scalar(b"?", 0, b"value"),
            map_end(),
            seq_end(),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex7_9_SingleQuotedLines)`.
#[test]
fn ex7_9_single_quoted_lines() {
    expect_events(
        EX7_9,
        &[
            doc_start(),
            scalar(b"!", 0, b" 1st non-empty\n2nd non-empty 3rd non-empty "),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex7_10_PlainCharacters)`.
#[test]
fn ex7_10_plain_characters() {
    expect_events(
        EX7_10,
        &[
            doc_start(),
            seq_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"::vector"),
            scalar(b"!", 0, b": - ()"),
            scalar(b"?", 0, b"Up, up, and away!"),
            scalar(b"?", 0, b"-123"),
            scalar(b"?", 0, b"http://example.com/foo#bar"),
            seq_start(b"?", 0, EmitterStyle::Flow),
            scalar(b"?", 0, b"::vector"),
            scalar(b"!", 0, b": - ()"),
            scalar(b"!", 0, b"Up, up, and away!"),
            scalar(b"?", 0, b"-123"),
            scalar(b"?", 0, b"http://example.com/foo#bar"),
            seq_end(),
            seq_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex7_11_PlainImplicitKeys)`.
#[test]
fn ex7_11_plain_implicit_keys() {
    expect_events(
        EX7_11,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"implicit block key"),
            seq_start(b"?", 0, EmitterStyle::Flow),
            map_start(b"?", 0, EmitterStyle::Flow),
            scalar(b"?", 0, b"implicit flow key"),
            scalar(b"?", 0, b"value"),
            map_end(),
            seq_end(),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex7_12_PlainLines)`.
#[test]
fn ex7_12_plain_lines() {
    expect_events(
        EX7_12,
        &[
            doc_start(),
            scalar(b"?", 0, b"1st non-empty\n2nd non-empty 3rd non-empty"),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex7_13_FlowSequence)`.
#[test]
fn ex7_13_flow_sequence() {
    expect_events(
        EX7_13,
        &[
            doc_start(),
            seq_start(b"?", 0, EmitterStyle::Block),
            seq_start(b"?", 0, EmitterStyle::Flow),
            scalar(b"?", 0, b"one"),
            scalar(b"?", 0, b"two"),
            seq_end(),
            seq_start(b"?", 0, EmitterStyle::Flow),
            scalar(b"?", 0, b"three"),
            scalar(b"?", 0, b"four"),
            seq_end(),
            seq_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex7_14_FlowSequenceEntries)`.
#[test]
fn ex7_14_flow_sequence_entries() {
    expect_events(
        EX7_14,
        &[
            doc_start(),
            seq_start(b"?", 0, EmitterStyle::Flow),
            scalar(b"!", 0, b"double quoted"),
            scalar(b"!", 0, b"single quoted"),
            scalar(b"?", 0, b"plain text"),
            seq_start(b"?", 0, EmitterStyle::Flow),
            scalar(b"?", 0, b"nested"),
            seq_end(),
            map_start(b"?", 0, EmitterStyle::Flow),
            scalar(b"?", 0, b"single"),
            scalar(b"?", 0, b"pair"),
            map_end(),
            seq_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex7_15_FlowMappings)`.
#[test]
fn ex7_15_flow_mappings() {
    expect_events(
        EX7_15,
        &[
            doc_start(),
            seq_start(b"?", 0, EmitterStyle::Block),
            map_start(b"?", 0, EmitterStyle::Flow),
            scalar(b"?", 0, b"one"),
            scalar(b"?", 0, b"two"),
            scalar(b"?", 0, b"three"),
            scalar(b"?", 0, b"four"),
            map_end(),
            map_start(b"?", 0, EmitterStyle::Flow),
            scalar(b"?", 0, b"five"),
            scalar(b"?", 0, b"six"),
            scalar(b"?", 0, b"seven"),
            scalar(b"?", 0, b"eight"),
            map_end(),
            seq_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex7_16_FlowMappingEntries)`.
#[test]
fn ex7_16_flow_mapping_entries() {
    expect_events(
        EX7_16,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Flow),
            scalar(b"?", 0, b"explicit"),
            scalar(b"?", 0, b"entry"),
            scalar(b"?", 0, b"implicit"),
            scalar(b"?", 0, b"entry"),
            null(0),
            null(0),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex7_17_FlowMappingSeparateValues)`.
#[test]
fn ex7_17_flow_mapping_separate_values() {
    expect_events(
        EX7_17,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Flow),
            scalar(b"?", 0, b"unquoted"),
            scalar(b"!", 0, b"separate"),
            scalar(b"?", 0, b"http://foo.com"),
            null(0),
            scalar(b"?", 0, b"omitted value"),
            null(0),
            null(0),
            scalar(b"?", 0, b"omitted key"),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex7_18_FlowMappingAdjacentValues)`.
#[test]
fn ex7_18_flow_mapping_adjacent_values() {
    expect_events(
        EX7_18,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Flow),
            scalar(b"!", 0, b"adjacent"),
            scalar(b"?", 0, b"value"),
            scalar(b"!", 0, b"readable"),
            scalar(b"?", 0, b"value"),
            scalar(b"!", 0, b"empty"),
            null(0),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex7_19_SinglePairFlowMappings)`.
#[test]
fn ex7_19_single_pair_flow_mappings() {
    expect_events(
        EX7_19,
        &[
            doc_start(),
            seq_start(b"?", 0, EmitterStyle::Flow),
            map_start(b"?", 0, EmitterStyle::Flow),
            scalar(b"?", 0, b"foo"),
            scalar(b"?", 0, b"bar"),
            map_end(),
            seq_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex7_20_SinglePairExplicitEntry)`.
#[test]
fn ex7_20_single_pair_explicit_entry() {
    expect_events(
        EX7_20,
        &[
            doc_start(),
            seq_start(b"?", 0, EmitterStyle::Flow),
            map_start(b"?", 0, EmitterStyle::Flow),
            scalar(b"?", 0, b"foo bar"),
            scalar(b"?", 0, b"baz"),
            map_end(),
            seq_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex7_21_SinglePairImplicitEntries)`.
#[test]
fn ex7_21_single_pair_implicit_entries() {
    expect_events(
        EX7_21,
        &[
            doc_start(),
            seq_start(b"?", 0, EmitterStyle::Block),
            seq_start(b"?", 0, EmitterStyle::Flow),
            map_start(b"?", 0, EmitterStyle::Flow),
            scalar(b"?", 0, b"YAML"),
            scalar(b"?", 0, b"separate"),
            map_end(),
            seq_end(),
            seq_start(b"?", 0, EmitterStyle::Flow),
            map_start(b"?", 0, EmitterStyle::Default),
            null(0),
            scalar(b"?", 0, b"empty key entry"),
            map_end(),
            seq_end(),
            seq_start(b"?", 0, EmitterStyle::Flow),
            map_start(b"?", 0, EmitterStyle::Flow),
            map_start(b"?", 0, EmitterStyle::Flow),
            scalar(b"?", 0, b"JSON"),
            scalar(b"?", 0, b"like"),
            map_end(),
            scalar(b"?", 0, b"adjacent"),
            map_end(),
            seq_end(),
            seq_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex7_22_InvalidImplicitKeys)`.
#[test]
fn ex7_22_invalid_implicit_keys() {
    expect_parser_exception(EX7_22, error_msg::END_OF_SEQ_FLOW.as_bytes());
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex7_23_FlowContent)`.
#[test]
fn ex7_23_flow_content() {
    expect_events(
        EX7_23,
        &[
            doc_start(),
            seq_start(b"?", 0, EmitterStyle::Block),
            seq_start(b"?", 0, EmitterStyle::Flow),
            scalar(b"?", 0, b"a"),
            scalar(b"?", 0, b"b"),
            seq_end(),
            map_start(b"?", 0, EmitterStyle::Flow),
            scalar(b"?", 0, b"a"),
            scalar(b"?", 0, b"b"),
            map_end(),
            scalar(b"!", 0, b"a"),
            scalar(b"!", 0, b"b"),
            scalar(b"?", 0, b"c"),
            seq_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex7_24_FlowNodes)`.
#[test]
fn ex7_24_flow_nodes() {
    expect_events(
        EX7_24,
        &[
            doc_start(),
            seq_start(b"?", 0, EmitterStyle::Block),
            scalar(b"tag:yaml.org,2002:str", 0, b"a"),
            scalar(b"!", 0, b"b"),
            anchor(b"anchor"),
            scalar(b"!", 1, b"c"),
            alias(1),
            scalar(b"tag:yaml.org,2002:str", 0, b""),
            seq_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex8_1_BlockScalarHeader)`.
#[test]
fn ex8_1_block_scalar_header() {
    expect_events(
        EX8_1,
        &[
            doc_start(),
            seq_start(b"?", 0, EmitterStyle::Block),
            scalar(b"!", 0, b"literal\n"),
            scalar(b"!", 0, b" folded\n"),
            scalar(b"!", 0, b"keep\n\n"),
            scalar(b"!", 0, b" strip"),
            seq_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex8_2_BlockIndentationHeader)`.
#[test]
fn ex8_2_block_indentation_header() {
    expect_events(
        EX8_2,
        &[
            doc_start(),
            seq_start(b"?", 0, EmitterStyle::Block),
            scalar(b"!", 0, b"detected\n"),
            scalar(b"!", 0, b"\n\n# detected\n"),
            scalar(b"!", 0, b" explicit\n"),
            scalar(b"!", 0, b"\t\ndetected\n"),
            seq_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex8_3a_InvalidBlockScalarIndentationIndicators)`.
#[test]
fn ex8_3a_invalid_block_scalar_indentation_indicators() {
    expect_parser_exception(EX8_3A, error_msg::END_OF_SEQ.as_bytes());
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex8_3b_InvalidBlockScalarIndentationIndicators)`.
#[test]
fn ex8_3b_invalid_block_scalar_indentation_indicators() {
    expect_parser_exception(EX8_3B, error_msg::END_OF_SEQ.as_bytes());
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex8_3c_InvalidBlockScalarIndentationIndicators)`.
#[test]
fn ex8_3c_invalid_block_scalar_indentation_indicators() {
    expect_parser_exception(EX8_3C, error_msg::END_OF_SEQ.as_bytes());
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex8_4_ChompingFinalLineBreak)`.
#[test]
fn ex8_4_chomping_final_line_break() {
    expect_events(
        EX8_4,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"strip"),
            scalar(b"!", 0, b"text"),
            scalar(b"?", 0, b"clip"),
            scalar(b"!", 0, b"text\n"),
            scalar(b"?", 0, b"keep"),
            scalar(b"!", 0, b"text\n"),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex8_6_EmptyScalarChomping)`.
#[test]
fn ex8_6_empty_scalar_chomping() {
    expect_events(
        EX8_6,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"strip"),
            scalar(b"!", 0, b""),
            scalar(b"?", 0, b"clip"),
            scalar(b"!", 0, b""),
            scalar(b"?", 0, b"keep"),
            scalar(b"!", 0, b"\n"),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex8_7_LiteralScalar)`.
#[test]
fn ex8_7_literal_scalar() {
    expect_events(
        EX8_7,
        &[
            doc_start(),
            scalar(b"!", 0, b"literal\n\ttext\n"),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex8_8_LiteralContent)`.
#[test]
fn ex8_8_literal_content() {
    expect_events(
        EX8_8,
        &[
            doc_start(),
            scalar(b"!", 0, b"\n\nliteral\n \n\ntext\n"),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex8_9_FoldedScalar)`.
#[test]
fn ex8_9_folded_scalar() {
    expect_events(
        EX8_9,
        &[doc_start(), scalar(b"!", 0, b"folded text\n"), doc_end()],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex8_10_FoldedLines)`.
#[test]
fn ex8_10_folded_lines() {
    expect_events(
        EX8_10,
        &[
            doc_start(),
            scalar(
                b"!",
                0,
                b"\nfolded line\nnext line\n  * bullet\n\n  * list\n  * lines\n\nlast line\n",
            ),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex8_11_MoreIndentedLines)`.
#[test]
fn ex8_11_more_indented_lines() {
    expect_events(
        EX8_11,
        &[
            doc_start(),
            scalar(
                b"!",
                0,
                b"\nfolded line\nnext line\n  * bullet\n\n  * list\n  * lines\n\nlast line\n",
            ),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex8_12_EmptySeparationLines)`.
#[test]
fn ex8_12_empty_separation_lines() {
    expect_events(
        EX8_12,
        &[
            doc_start(),
            scalar(
                b"!",
                0,
                b"\nfolded line\nnext line\n  * bullet\n\n  * list\n  * lines\n\nlast line\n",
            ),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex8_13_FinalEmptyLines)`.
#[test]
fn ex8_13_final_empty_lines() {
    expect_events(
        EX8_13,
        &[
            doc_start(),
            scalar(
                b"!",
                0,
                b"\nfolded line\nnext line\n  * bullet\n\n  * list\n  * lines\n\nlast line\n",
            ),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex8_14_BlockSequence)`.
#[test]
fn ex8_14_block_sequence() {
    expect_events(
        EX8_14,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"block sequence"),
            seq_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"one"),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"two"),
            scalar(b"?", 0, b"three"),
            map_end(),
            seq_end(),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex8_15_BlockSequenceEntryTypes)`.
#[test]
fn ex8_15_block_sequence_entry_types() {
    expect_events(
        EX8_15,
        &[
            doc_start(),
            seq_start(b"?", 0, EmitterStyle::Block),
            null(0),
            scalar(b"!", 0, b"block node\n"),
            seq_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"one"),
            scalar(b"?", 0, b"two"),
            seq_end(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"one"),
            scalar(b"?", 0, b"two"),
            map_end(),
            seq_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex8_16_BlockMappings)`.
#[test]
fn ex8_16_block_mappings() {
    expect_events(
        EX8_16,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"block mapping"),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"key"),
            scalar(b"?", 0, b"value"),
            map_end(),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex8_17_ExplicitBlockMappingEntries)`.
#[test]
fn ex8_17_explicit_block_mapping_entries() {
    expect_events(
        EX8_17,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"explicit key"),
            null(0),
            scalar(b"!", 0, b"block key\n"),
            seq_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"one"),
            scalar(b"?", 0, b"two"),
            seq_end(),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex8_18_ImplicitBlockMappingEntries)`.
#[test]
fn ex8_18_implicit_block_mapping_entries() {
    expect_events(
        EX8_18,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"plain key"),
            scalar(b"?", 0, b"in-line value"),
            null(0),
            null(0),
            scalar(b"!", 0, b"quoted key"),
            seq_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"entry"),
            seq_end(),
            map_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex8_19_CompactBlockMappings)`.
#[test]
fn ex8_19_compact_block_mappings() {
    expect_events(
        EX8_19,
        &[
            doc_start(),
            seq_start(b"?", 0, EmitterStyle::Block),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"sun"),
            scalar(b"?", 0, b"yellow"),
            map_end(),
            map_start(b"?", 0, EmitterStyle::Block),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"earth"),
            scalar(b"?", 0, b"blue"),
            map_end(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"moon"),
            scalar(b"?", 0, b"white"),
            map_end(),
            map_end(),
            seq_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex8_20_BlockNodeTypes)`.
#[test]
fn ex8_20_block_node_types() {
    expect_events(
        EX8_20,
        &[
            doc_start(),
            seq_start(b"?", 0, EmitterStyle::Block),
            scalar(b"!", 0, b"flow in block"),
            scalar(b"!", 0, b"Block scalar\n"),
            map_start(b"tag:yaml.org,2002:map", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"foo"),
            scalar(b"?", 0, b"bar"),
            map_end(),
            seq_end(),
            doc_end(),
        ],
    );
}

/// Port of yaml-cpp 0.8.0 `TEST_F(HandlerSpecTest, Ex8_22_BlockCollectionNodes)`.
#[test]
fn ex8_22_block_collection_nodes() {
    expect_events(
        EX8_22,
        &[
            doc_start(),
            map_start(b"?", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"sequence"),
            seq_start(b"tag:yaml.org,2002:seq", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"entry"),
            seq_start(b"tag:yaml.org,2002:seq", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"nested"),
            seq_end(),
            seq_end(),
            scalar(b"?", 0, b"mapping"),
            map_start(b"tag:yaml.org,2002:map", 0, EmitterStyle::Block),
            scalar(b"?", 0, b"foo"),
            scalar(b"?", 0, b"bar"),
            map_end(),
            map_end(),
            doc_end(),
        ],
    );
}
