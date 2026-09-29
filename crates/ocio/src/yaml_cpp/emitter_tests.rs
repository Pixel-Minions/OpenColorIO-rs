// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.8.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! Port of yaml-cpp 0.8.0 `test/integration/emitter_test.cpp`: every `EmitterTest` and
//! `EmitterErrorTest` that doesn't go through yaml-cpp's `Node` API, with the expected text
//! copied from the test. (`SimpleQuotedScalar`, `DumpAndSize`, `NullScalar`, `AliasScalar`,
//! `GlobalSettingStyleOnSeqNode`, `GlobalSettingStyleOnMapNode`, `NullFormattingOnNode` and
//! `AnchorEncoding` emit a loaded `Node`, which this port doesn't have.)

use std::collections::BTreeMap;

use super::super::emitter_manip::EmitterManip::*;
use super::super::emitter_manip::{
    Alias, Anchor, Binary, Comment, Indent, Null, double_precision, float_precision, local_tag,
    local_tag_with_prefix, verbatim_tag,
};
use super::super::emitter_state::error_msg;
use super::Emitter;

/// `EmitterTest::ExpectEmit`: the text, and no error.
#[track_caller]
fn expect_emit(out: &Emitter, expected: &str) {
    assert_eq!(out.c_str(), expected);
    assert!(out.good(), "Emitter raised: {}", out.last_error());
}

/// `EmitterErrorTest::ExpectEmitError`.
#[track_caller]
fn expect_emit_error(out: &Emitter, expected: &str) {
    assert!(!out.good(), "Emitter cleanly produced: {}", out.c_str());
    assert_eq!(out.last_error(), expected);
}

/// yaml-cpp 0.8.0 `EmitterTest.SimpleScalar`.
#[test]
fn simple_scalar() {
    let mut out = Emitter::new();
    out.put("Hello, World!");
    expect_emit(&out, "Hello, World!");
}

/// yaml-cpp 0.8.0 `EmitterTest.StringFormat`.
#[test]
fn string_format() {
    let mut out = Emitter::new();
    out.put(BeginSeq);
    out.set_string_format(SingleQuoted);
    out.put("string");
    out.set_string_format(DoubleQuoted);
    out.put("string");
    out.set_string_format(Literal);
    out.put("string");
    out.put(EndSeq);
    expect_emit(&out, "- 'string'\n- \"string\"\n- |\n  string");
}

/// yaml-cpp 0.8.0 `EmitterTest.IntBase`.
#[test]
fn int_base() {
    let mut out = Emitter::new();
    out.put(BeginSeq);
    out.set_int_base(Dec);
    out.put(1024);
    out.set_int_base(Hex);
    out.put(1024);
    out.set_int_base(Oct);
    out.put(1024);
    out.put(EndSeq);
    expect_emit(&out, "- 1024\n- 0x400\n- 02000");
}

/// yaml-cpp 0.8.0 `EmitterTest.NumberPrecision`.
#[test]
fn number_precision() {
    let mut out = Emitter::new();
    out.set_float_precision(3);
    out.set_double_precision(2);
    out.put(BeginSeq);
    out.put(3.1425926f32);
    out.put(53.5893f64);
    out.put(2384626.4338f64);
    out.put(EndSeq);
    expect_emit(&out, "- 3.14\n- 54\n- 2.4e+06");
}

/// yaml-cpp 0.8.0 `EmitterTest.SimpleSeq`.
#[test]
fn simple_seq() {
    let mut out = Emitter::new();
    out.put(BeginSeq)
        .put("eggs")
        .put("bread")
        .put("milk")
        .put(EndSeq);
    expect_emit(&out, "- eggs\n- bread\n- milk");
}

/// yaml-cpp 0.8.0 `EmitterTest.SimpleFlowSeq`.
#[test]
fn simple_flow_seq() {
    let mut out = Emitter::new();
    out.put(Flow)
        .put(BeginSeq)
        .put("Larry")
        .put("Curly")
        .put("Moe")
        .put(EndSeq);
    expect_emit(&out, "[Larry, Curly, Moe]");
}

/// yaml-cpp 0.8.0 `EmitterTest.EmptyFlowSeq`.
#[test]
fn empty_flow_seq() {
    let mut out = Emitter::new();
    out.put(Flow).put(BeginSeq).put(EndSeq);
    expect_emit(&out, "[]");
}

/// yaml-cpp 0.8.0 `EmitterTest.EmptyBlockSeqWithBegunContent`.
#[test]
fn empty_block_seq_with_begun_content() {
    let mut out = Emitter::new();
    out.put(BeginSeq);
    out.put(BeginSeq)
        .put(&Comment("comment".into()))
        .put(EndSeq);
    out.put(BeginSeq).put(Newline).put(EndSeq);
    out.put(EndSeq);
    expect_emit(&out, "-\n# comment\n  []\n-\n\n  []");
}

/// yaml-cpp 0.8.0 `EmitterTest.EmptyBlockMapWithBegunContent`.
#[test]
fn empty_block_map_with_begun_content() {
    let mut out = Emitter::new();
    out.put(BeginSeq);
    out.put(BeginMap)
        .put(&Comment("comment".into()))
        .put(EndMap);
    out.put(BeginMap).put(Newline).put(EndMap);
    out.put(EndSeq);
    expect_emit(&out, "-  # comment\n  {}\n-\n  {}");
}

/// yaml-cpp 0.8.0 `EmitterTest.EmptyFlowSeqWithBegunContent`.
#[test]
fn empty_flow_seq_with_begun_content() {
    let mut out = Emitter::new();
    out.put(Flow);
    out.put(BeginSeq);
    out.put(BeginSeq)
        .put(&Comment("comment".into()))
        .put(EndSeq);
    out.put(BeginSeq).put(Newline).put(EndSeq);
    out.put(EndSeq);
    expect_emit(&out, "[[  # comment\n  ], [\n  ]]");
}

/// yaml-cpp 0.8.0 `EmitterTest.EmptyFlowMapWithBegunContent`.
#[test]
fn empty_flow_map_with_begun_content() {
    let mut out = Emitter::new();
    out.put(Flow);
    out.put(BeginSeq);
    out.put(BeginMap)
        .put(&Comment("comment".into()))
        .put(EndMap);
    out.put(BeginMap).put(Newline).put(EndMap);
    out.put(EndSeq);
    expect_emit(&out, "[{  # comment\n  }, {\n  }]");
}

/// yaml-cpp 0.8.0 `EmitterTest.NestedBlockSeq`.
#[test]
fn nested_block_seq() {
    let mut out = Emitter::new();
    out.put(BeginSeq);
    out.put("item 1");
    out.put(BeginSeq)
        .put("subitem 1")
        .put("subitem 2")
        .put(EndSeq);
    out.put(EndSeq);
    expect_emit(&out, "- item 1\n-\n  - subitem 1\n  - subitem 2");
}

/// yaml-cpp 0.8.0 `EmitterTest.NestedFlowSeq`.
#[test]
fn nested_flow_seq() {
    let mut out = Emitter::new();
    out.put(BeginSeq);
    out.put("one");
    out.put(Flow)
        .put(BeginSeq)
        .put("two")
        .put("three")
        .put(EndSeq);
    out.put(EndSeq);
    expect_emit(&out, "- one\n- [two, three]");
}

/// yaml-cpp 0.8.0 `EmitterTest.SimpleMap`.
#[test]
fn simple_map() {
    let mut out = Emitter::new();
    out.put(BeginMap);
    out.put(Key).put("name");
    out.put(Value).put("Ryan Braun");
    out.put(Key).put("position");
    out.put(Value).put("3B");
    out.put(EndMap);
    expect_emit(&out, "name: Ryan Braun\nposition: 3B");
}

/// yaml-cpp 0.8.0 `EmitterTest.SimpleFlowMap`.
#[test]
fn simple_flow_map() {
    let mut out = Emitter::new();
    out.put(Flow);
    out.put(BeginMap);
    out.put(Key).put("shape");
    out.put(Value).put("square");
    out.put(Key).put("color");
    out.put(Value).put("blue");
    out.put(EndMap);
    expect_emit(&out, "{shape: square, color: blue}");
}

/// yaml-cpp 0.8.0 `EmitterTest.MapAndList`.
#[test]
fn map_and_list() {
    let mut out = Emitter::new();
    out.put(BeginMap);
    out.put(Key).put("name");
    out.put(Value).put("Barack Obama");
    out.put(Key).put("children");
    out.put(Value)
        .put(BeginSeq)
        .put("Sasha")
        .put("Malia")
        .put(EndSeq);
    out.put(EndMap);
    expect_emit(&out, "name: Barack Obama\nchildren:\n  - Sasha\n  - Malia");
}

/// yaml-cpp 0.8.0 `EmitterTest.ListAndMap`.
#[test]
fn list_and_map() {
    let mut out = Emitter::new();
    out.put(BeginSeq);
    out.put("item 1");
    out.put(BeginMap);
    out.put(Key).put("pens").put(Value).put(8);
    out.put(Key).put("pencils").put(Value).put(14);
    out.put(EndMap);
    out.put("item 2");
    out.put(EndSeq);
    expect_emit(&out, "- item 1\n- pens: 8\n  pencils: 14\n- item 2");
}

/// yaml-cpp 0.8.0 `EmitterTest.NestedBlockMap`.
#[test]
fn nested_block_map() {
    let mut out = Emitter::new();
    out.put(BeginMap);
    out.put(Key).put("name");
    out.put(Value).put("Fred");
    out.put(Key).put("grades");
    out.put(Value);
    out.put(BeginMap);
    out.put(Key).put("algebra").put(Value).put("A");
    out.put(Key).put("physics").put(Value).put("C+");
    out.put(Key).put("literature").put(Value).put("B");
    out.put(EndMap);
    out.put(EndMap);
    expect_emit(
        &out,
        "name: Fred\ngrades:\n  algebra: A\n  physics: C+\n  literature: B",
    );
}

/// yaml-cpp 0.8.0 `EmitterTest.NestedFlowMap`.
#[test]
fn nested_flow_map() {
    let mut out = Emitter::new();
    out.put(Flow);
    out.put(BeginMap);
    out.put(Key).put("name");
    out.put(Value).put("Fred");
    out.put(Key).put("grades");
    out.put(Value);
    out.put(BeginMap);
    out.put(Key).put("algebra").put(Value).put("A");
    out.put(Key).put("physics").put(Value).put("C+");
    out.put(Key).put("literature").put(Value).put("B");
    out.put(EndMap);
    out.put(EndMap);
    expect_emit(
        &out,
        "{name: Fred, grades: {algebra: A, physics: C+, literature: B}}",
    );
}

/// yaml-cpp 0.8.0 `EmitterTest.MapListMix`.
#[test]
fn map_list_mix() {
    let mut out = Emitter::new();
    out.put(BeginMap);
    out.put(Key).put("name");
    out.put(Value).put("Bob");
    out.put(Key).put("position");
    out.put(Value);
    out.put(Flow).put(BeginSeq).put(2).put(4).put(EndSeq);
    out.put(Key)
        .put("invincible")
        .put(Value)
        .put(OnOffBool)
        .put(false);
    out.put(EndMap);
    expect_emit(&out, "name: Bob\nposition: [2, 4]\ninvincible: off");
}

/// yaml-cpp 0.8.0 `EmitterTest.SimpleLongKey`.
#[test]
fn simple_long_key() {
    let mut out = Emitter::new();
    out.put(LongKey);
    out.put(BeginMap);
    out.put(Key).put("height");
    out.put(Value).put("5'9\"");
    out.put(Key).put("weight");
    out.put(Value).put(145);
    out.put(EndMap);
    expect_emit(&out, "? height\n: 5'9\"\n? weight\n: 145");
}

/// yaml-cpp 0.8.0 `EmitterTest.SingleLongKey`.
#[test]
fn single_long_key() {
    let short_key = "a".repeat(1024);
    let long_key = "a".repeat(1025);
    let mut out = Emitter::new();
    out.put(BeginMap);
    out.put(Key).put("age");
    out.put(Value).put("24");
    out.put(LongKey).put(Key).put("height");
    out.put(Value).put("5'9\"");
    out.put(Key).put("weight");
    out.put(Value).put(145);
    out.put(Key).put(&short_key);
    out.put(Value).put("1");
    out.put(Key).put(&long_key);
    out.put(Value).put("1");
    out.put(EndMap);
    expect_emit(
        &out,
        &format!("age: 24\n? height\n: 5'9\"\nweight: 145\n{short_key}: 1\n? {long_key}\n: 1"),
    );
}

/// yaml-cpp 0.8.0 `EmitterTest.ComplexLongKey`.
#[test]
fn complex_long_key() {
    let mut out = Emitter::new();
    out.put(LongKey);
    out.put(BeginMap);
    out.put(Key).put(BeginSeq).put(1).put(3).put(EndSeq);
    out.put(Value).put("monster");
    out.put(Key)
        .put(Flow)
        .put(BeginSeq)
        .put(2)
        .put(0)
        .put(EndSeq);
    out.put(Value).put("demon");
    out.put(EndMap);
    expect_emit(&out, "? - 1\n  - 3\n: monster\n? [2, 0]\n: demon");
}

/// yaml-cpp 0.8.0 `EmitterTest.AutoLongKey`.
#[test]
fn auto_long_key() {
    let mut out = Emitter::new();
    out.put(BeginMap);
    out.put(Key).put(BeginSeq).put(1).put(3).put(EndSeq);
    out.put(Value).put("monster");
    out.put(Key)
        .put(Flow)
        .put(BeginSeq)
        .put(2)
        .put(0)
        .put(EndSeq);
    out.put(Value).put("demon");
    out.put(Key).put("the origin");
    out.put(Value).put("angel");
    out.put(EndMap);
    expect_emit(
        &out,
        "? - 1\n  - 3\n: monster\n[2, 0]: demon\nthe origin: angel",
    );
}

/// yaml-cpp 0.8.0 `EmitterTest.ScalarFormat`.
#[test]
fn scalar_format() {
    let mut out = Emitter::new();
    out.put(BeginSeq);
    out.put("simple scalar");
    out.put(SingleQuoted).put("explicit single-quoted scalar");
    out.put(DoubleQuoted).put("explicit double-quoted scalar");
    out.put("auto-detected\ndouble-quoted scalar");
    out.put("a non-\"auto-detected\" double-quoted scalar");
    out.put(Literal).put(
        "literal scalar\nthat may span\nmany, many\nlines \
         and have \"whatever\" crazy\tsymbols that we like",
    );
    out.put(EndSeq);
    expect_emit(
        &out,
        "- simple scalar\n- 'explicit single-quoted scalar'\n- \"explicit \
         double-quoted scalar\"\n- \"auto-detected\\ndouble-quoted \
         scalar\"\n- a \
         non-\"auto-detected\" double-quoted scalar\n- |\n  literal scalar\n \
         \x20\
         that may span\n  many, many\n  lines and have \"whatever\" \
         crazy\tsymbols that we like",
    );
}

/// yaml-cpp 0.8.0 `EmitterTest.LiteralWithoutTrailingSpaces`.
#[test]
fn literal_without_trailing_spaces() {
    let mut out = Emitter::new();
    out.put(BeginMap);
    out.put(Key).put("key");
    out.put(Value).put(Literal);
    out.put("expect that with two newlines\n\nno spaces are emitted in the empty line");
    out.put(EndMap);
    expect_emit(
        &out,
        "key: |\n  expect that with two newlines\n\n  no spaces are emitted in the empty line",
    );
}

/// yaml-cpp 0.8.0 `EmitterTest.AutoLongKeyScalar`.
#[test]
fn auto_long_key_scalar() {
    let mut out = Emitter::new();
    out.put(BeginMap);
    out.put(Key).put(Literal).put("multi-line\nscalar");
    out.put(Value).put("and its value");
    out.put(EndMap);
    expect_emit(&out, "? |\n  multi-line\n  scalar\n: and its value");
}

/// yaml-cpp 0.8.0 `EmitterTest.LongKeyFlowMap`.
#[test]
fn long_key_flow_map() {
    let mut out = Emitter::new();
    out.put(Flow);
    out.put(BeginMap);
    out.put(Key).put("simple key");
    out.put(Value).put("and value");
    out.put(LongKey).put(Key).put("long key");
    out.put(Value).put("and its value");
    out.put(EndMap);
    expect_emit(&out, "{simple key: and value, ? long key: and its value}");
}

/// yaml-cpp 0.8.0 `EmitterTest.BlockMapAsKey`.
#[test]
fn block_map_as_key() {
    let mut out = Emitter::new();
    out.put(BeginMap);
    out.put(Key);
    out.put(BeginMap);
    out.put(Key).put("key").put(Value).put("value");
    out.put(Key).put("next key").put(Value).put("next value");
    out.put(EndMap);
    out.put(Value);
    out.put("total value");
    out.put(EndMap);
    expect_emit(&out, "? key: value\n  next key: next value\n: total value");
}

/// yaml-cpp 0.8.0 `EmitterTest.TaggedBlockMapAsKey`.
#[test]
fn tagged_block_map_as_key() {
    let mut out = Emitter::new();
    out.put(BeginMap);
    out.put(Key);
    out.put(local_tag("innerMap"));
    out.put(BeginMap);
    out.put(Key).put("key").put(Value).put("value");
    out.put(EndMap);
    out.put(Value);
    out.put("outerValue");
    out.put(EndMap);
    expect_emit(&out, "? !innerMap\n  key: value\n: outerValue");
}

/// yaml-cpp 0.8.0 `EmitterTest.TaggedBlockListAsKey`.
#[test]
fn tagged_block_list_as_key() {
    let mut out = Emitter::new();
    out.put(BeginMap);
    out.put(Key);
    out.put(local_tag("innerList"));
    out.put(BeginSeq);
    out.put("listItem");
    out.put(EndSeq);
    out.put(Value);
    out.put("outerValue");
    out.put(EndMap);
    expect_emit(&out, "? !innerList\n  - listItem\n: outerValue");
}

/// yaml-cpp 0.8.0 `EmitterTest.AliasAndAnchor`.
#[test]
fn alias_and_anchor() {
    let mut out = Emitter::new();
    out.put(BeginSeq);
    out.put(&Anchor("fred".into()));
    out.put(BeginMap);
    out.put(Key).put("name").put(Value).put("Fred");
    out.put(Key).put("age").put(Value).put(42);
    out.put(EndMap);
    out.put(&Alias("fred".into()));
    out.put(EndSeq);
    expect_emit(&out, "- &fred\n  name: Fred\n  age: 42\n- *fred");
}

/// yaml-cpp 0.8.0 `EmitterTest.AliasOnKey`.
#[test]
fn alias_on_key() {
    let mut out = Emitter::new();
    out.put(BeginSeq);
    out.put(&Anchor("name".into())).put("Name");
    out.put(BeginMap);
    out.put(Key)
        .put(&Alias("name".into()))
        .put(Value)
        .put("Fred");
    out.put(EndMap);
    out.put(Flow).put(BeginMap);
    out.put(Key)
        .put(&Alias("name".into()))
        .put(Value)
        .put("Mike");
    out.put(EndMap);
    out.put(EndSeq);
    expect_emit(&out, "- &name Name\n- *name : Fred\n- {*name : Mike}");
}

/// yaml-cpp 0.8.0 `EmitterTest.AliasAndAnchorWithNull`.
#[test]
fn alias_and_anchor_with_null() {
    let mut out = Emitter::new();
    out.put(BeginSeq);
    out.put(&Anchor("fred".into())).put(Null);
    out.put(&Alias("fred".into()));
    out.put(EndSeq);
    expect_emit(&out, "- &fred ~\n- *fred");
}

/// yaml-cpp 0.8.0 `EmitterTest.AliasAndAnchorInFlow`.
#[test]
fn alias_and_anchor_in_flow() {
    let mut out = Emitter::new();
    out.put(Flow).put(BeginSeq);
    out.put(&Anchor("fred".into()));
    out.put(BeginMap);
    out.put(Key).put("name").put(Value).put("Fred");
    out.put(Key).put("age").put(Value).put(42);
    out.put(EndMap);
    out.put(&Alias("fred".into()));
    out.put(EndSeq);
    expect_emit(&out, "[&fred {name: Fred, age: 42}, *fred]");
}

/// yaml-cpp 0.8.0 `EmitterTest.SimpleVerbatimTag`.
#[test]
fn simple_verbatim_tag() {
    let mut out = Emitter::new();
    out.put(verbatim_tag("!foo")).put("bar");
    expect_emit(&out, "!<!foo> bar");
}

/// yaml-cpp 0.8.0 `EmitterTest.VerbatimTagInBlockSeq`.
#[test]
fn verbatim_tag_in_block_seq() {
    let mut out = Emitter::new();
    out.put(BeginSeq);
    out.put(verbatim_tag("!foo")).put("bar");
    out.put("baz");
    out.put(EndSeq);
    expect_emit(&out, "- !<!foo> bar\n- baz");
}

/// yaml-cpp 0.8.0 `EmitterTest.VerbatimTagInFlowSeq`.
#[test]
fn verbatim_tag_in_flow_seq() {
    let mut out = Emitter::new();
    out.put(Flow).put(BeginSeq);
    out.put(verbatim_tag("!foo")).put("bar");
    out.put("baz");
    out.put(EndSeq);
    expect_emit(&out, "[!<!foo> bar, baz]");
}

/// yaml-cpp 0.8.0 `EmitterTest.VerbatimTagInFlowSeqWithNull`.
#[test]
fn verbatim_tag_in_flow_seq_with_null() {
    let mut out = Emitter::new();
    out.put(Flow).put(BeginSeq);
    out.put(verbatim_tag("!foo")).put(Null);
    out.put("baz");
    out.put(EndSeq);
    expect_emit(&out, "[!<!foo> ~, baz]");
}

/// yaml-cpp 0.8.0 `EmitterTest.VerbatimTagInBlockMap`.
#[test]
fn verbatim_tag_in_block_map() {
    let mut out = Emitter::new();
    out.put(BeginMap);
    out.put(Key).put(verbatim_tag("!foo")).put("bar");
    out.put(Value).put(verbatim_tag("!waz")).put("baz");
    out.put(EndMap);
    expect_emit(&out, "? !<!foo> bar\n: !<!waz> baz");
}

/// yaml-cpp 0.8.0 `EmitterTest.VerbatimTagInFlowMap`.
#[test]
fn verbatim_tag_in_flow_map() {
    let mut out = Emitter::new();
    out.put(Flow).put(BeginMap);
    out.put(Key).put(verbatim_tag("!foo")).put("bar");
    out.put(Value).put("baz");
    out.put(EndMap);
    expect_emit(&out, "{!<!foo> bar: baz}");
}

/// yaml-cpp 0.8.0 `EmitterTest.VerbatimTagInFlowMapWithNull`.
#[test]
fn verbatim_tag_in_flow_map_with_null() {
    let mut out = Emitter::new();
    out.put(Flow).put(BeginMap);
    out.put(Key).put(verbatim_tag("!foo")).put(Null);
    out.put(Value).put("baz");
    out.put(EndMap);
    expect_emit(&out, "{!<!foo> ~: baz}");
}

/// yaml-cpp 0.8.0 `EmitterTest.VerbatimTagWithEmptySeq`.
#[test]
fn verbatim_tag_with_empty_seq() {
    let mut out = Emitter::new();
    out.put(verbatim_tag("!foo")).put(BeginSeq).put(EndSeq);
    expect_emit(&out, "!<!foo>\n[]");
}

/// yaml-cpp 0.8.0 `EmitterTest.VerbatimTagWithEmptyMap`.
#[test]
fn verbatim_tag_with_empty_map() {
    let mut out = Emitter::new();
    out.put(verbatim_tag("!bar")).put(BeginMap).put(EndMap);
    expect_emit(&out, "!<!bar>\n{}");
}

/// yaml-cpp 0.8.0 `EmitterTest.VerbatimTagWithEmptySeqAndMap`.
#[test]
fn verbatim_tag_with_empty_seq_and_map() {
    let mut out = Emitter::new();
    out.put(BeginSeq);
    out.put(verbatim_tag("!foo")).put(BeginSeq).put(EndSeq);
    out.put(verbatim_tag("!bar")).put(BeginMap).put(EndMap);
    out.put(EndSeq);
    expect_emit(&out, "- !<!foo>\n  []\n- !<!bar>\n  {}");
}

/// yaml-cpp 0.8.0 `EmitterTest.ByKindTagWithScalar`.
#[test]
fn by_kind_tag_with_scalar() {
    let mut out = Emitter::new();
    out.put(BeginSeq);
    out.put(DoubleQuoted).put("12");
    out.put("12");
    out.put(TagByKind).put("12");
    out.put(EndSeq);
    expect_emit(&out, "- \"12\"\n- 12\n- ! 12");
}

/// yaml-cpp 0.8.0 `EmitterTest.LocalTagInNameHandle`.
#[test]
fn local_tag_in_name_handle() {
    let mut out = Emitter::new();
    out.put(local_tag_with_prefix("a", "foo")).put("bar");
    expect_emit(&out, "!a!foo bar");
}

/// yaml-cpp 0.8.0 `EmitterTest.LocalTagWithScalar`.
#[test]
fn local_tag_with_scalar() {
    let mut out = Emitter::new();
    out.put(local_tag("foo")).put("bar");
    expect_emit(&out, "!foo bar");
}

/// yaml-cpp 0.8.0 `EmitterTest.ComplexDoc`.
#[test]
fn complex_doc() {
    let mut out = Emitter::new();
    out.put(BeginMap);
    out.put(Key).put("receipt");
    out.put(Value).put("Oz-Ware Purchase Invoice");
    out.put(Key).put("date");
    out.put(Value).put("2007-08-06");
    out.put(Key).put("customer");
    out.put(Value);
    out.put(BeginMap);
    out.put(Key).put("given");
    out.put(Value).put("Dorothy");
    out.put(Key).put("family");
    out.put(Value).put("Gale");
    out.put(EndMap);
    out.put(Key).put("items");
    out.put(Value);
    out.put(BeginSeq);
    out.put(BeginMap);
    out.put(Key).put("part_no");
    out.put(Value).put("A4786");
    out.put(Key).put("descrip");
    out.put(Value).put("Water Bucket (Filled)");
    out.put(Key).put("price");
    out.put(Value).put(1.47f64);
    out.put(Key).put("quantity");
    out.put(Value).put(4);
    out.put(EndMap);
    out.put(BeginMap);
    out.put(Key).put("part_no");
    out.put(Value).put("E1628");
    out.put(Key).put("descrip");
    out.put(Value).put("High Heeled \"Ruby\" Slippers");
    out.put(Key).put("price");
    out.put(Value).put(100.27f64);
    out.put(Key).put("quantity");
    out.put(Value).put(1);
    out.put(EndMap);
    out.put(EndSeq);
    out.put(Key).put("bill-to");
    out.put(Value).put(&Anchor("id001".into()));
    out.put(BeginMap);
    out.put(Key).put("street");
    out.put(Value)
        .put(Literal)
        .put("123 Tornado Alley\nSuite 16");
    out.put(Key).put("city");
    out.put(Value).put("East Westville");
    out.put(Key).put("state");
    out.put(Value).put("KS");
    out.put(EndMap);
    out.put(Key).put("ship-to");
    out.put(Value).put(&Alias("id001".into()));
    out.put(EndMap);
    expect_emit(
        &out,
        "receipt: Oz-Ware Purchase Invoice\ndate: 2007-08-06\ncustomer:\n  \
         given: Dorothy\n  family: Gale\nitems:\n  - part_no: A4786\n    \
         descrip: Water Bucket (Filled)\n    price: 1.47\n    quantity: 4\n  - \
         part_no: E1628\n    descrip: High Heeled \"Ruby\" Slippers\n    price: \
         100.27\n    quantity: 1\nbill-to: &id001\n  street: |\n    123 Tornado \
         Alley\n    Suite 16\n  city: East Westville\n  state: KS\nship-to: \
         *id001",
    );
}

/// yaml-cpp 0.8.0 `EmitterTest.STLContainers`.
#[test]
fn stl_containers() {
    let mut out = Emitter::new();
    out.put(BeginSeq);
    let primes: Vec<i32> = vec![2, 3, 5, 7, 11, 13];
    out.put(Flow).put(&primes);
    let mut ages = BTreeMap::new();
    ages.insert(String::from("Daniel"), 26);
    ages.insert(String::from("Jesse"), 24);
    out.put(&ages);
    out.put(EndSeq);
    expect_emit(&out, "- [2, 3, 5, 7, 11, 13]\n- Daniel: 26\n  Jesse: 24");
}

/// yaml-cpp 0.8.0 `EmitterTest.CommentStyle`.
#[test]
fn comment_style() {
    let mut out = Emitter::new();
    out.set_pre_comment_indent(1);
    out.set_post_comment_indent(2);
    out.put(BeginMap);
    out.put(Key).put("method");
    out.put(Value)
        .put("least squares")
        .put(&Comment("should we change this method?".into()));
    out.put(EndMap);
    expect_emit(
        &out,
        "method: least squares #  should we change this method?",
    );
}

/// yaml-cpp 0.8.0 `EmitterTest.SimpleComment`.
#[test]
fn simple_comment() {
    let mut out = Emitter::new();
    out.put(BeginMap);
    out.put(Key).put("method");
    out.put(Value)
        .put("least squares")
        .put(&Comment("should we change this method?".into()));
    out.put(EndMap);
    expect_emit(
        &out,
        "method: least squares  # should we change this method?",
    );
}

/// yaml-cpp 0.8.0 `EmitterTest.MultiLineComment`.
#[test]
fn multi_line_comment() {
    let mut out = Emitter::new();
    out.put(BeginSeq);
    out.put("item 1").put(&Comment(
        "really really long\ncomment that couldn't possibly\nfit on one line".into(),
    ));
    out.put("item 2");
    out.put(EndSeq);
    expect_emit(
        &out,
        "- item 1  # really really long\n          # comment that couldn't \
         possibly\n          # fit on one line\n- item 2",
    );
}

/// yaml-cpp 0.8.0 `EmitterTest.ComplexComments`.
#[test]
fn complex_comments() {
    let mut out = Emitter::new();
    out.put(BeginMap);
    out.put(LongKey)
        .put(Key)
        .put("long key")
        .put(&Comment("long key".into()));
    out.put(Value).put("value");
    out.put(EndMap);
    expect_emit(&out, "? long key  # long key\n: value");
}

/// yaml-cpp 0.8.0 `EmitterTest.InitialComment`.
#[test]
fn initial_comment() {
    let mut out = Emitter::new();
    out.put(&Comment(
        "A comment describing the purpose of the file.".into(),
    ));
    out.put(BeginMap)
        .put(Key)
        .put("key")
        .put(Value)
        .put("value")
        .put(EndMap);
    expect_emit(
        &out,
        "# A comment describing the purpose of the file.\nkey: value",
    );
}

/// yaml-cpp 0.8.0 `EmitterTest.InitialCommentWithDocIndicator`.
#[test]
fn initial_comment_with_doc_indicator() {
    let mut out = Emitter::new();
    out.put(BeginDoc).put(&Comment(
        "A comment describing the purpose of the file.".into(),
    ));
    out.put(BeginMap)
        .put(Key)
        .put("key")
        .put(Value)
        .put("value")
        .put(EndMap);
    expect_emit(
        &out,
        "---\n# A comment describing the purpose of the file.\nkey: value",
    );
}

/// yaml-cpp 0.8.0 `EmitterTest.CommentInFlowSeq`.
#[test]
fn comment_in_flow_seq() {
    let mut out = Emitter::new();
    out.put(Flow)
        .put(BeginSeq)
        .put("foo")
        .put(&Comment("foo!".into()))
        .put("bar")
        .put(EndSeq);
    expect_emit(&out, "[foo,  # foo!\nbar]");
}

/// yaml-cpp 0.8.0 `EmitterTest.CommentInFlowMap`.
#[test]
fn comment_in_flow_map() {
    let mut out = Emitter::new();
    out.put(Flow).put(BeginMap);
    out.put(Key).put("foo").put(Value).put("foo value");
    out.put(Key)
        .put("bar")
        .put(Value)
        .put("bar value")
        .put(&Comment("bar!".into()));
    out.put(Key)
        .put("baz")
        .put(Value)
        .put("baz value")
        .put(&Comment("baz!".into()));
    out.put(EndMap);
    expect_emit(
        &out,
        "{foo: foo value, bar: bar value,  # bar!\nbaz: baz value,  # baz!\n}",
    );
}

/// yaml-cpp 0.8.0 `EmitterTest.Indentation`.
#[test]
fn indentation() {
    let mut out = Emitter::new();
    out.put(Indent(4));
    out.put(BeginSeq);
    out.put(BeginMap);
    out.put(Key).put("key 1").put(Value).put("value 1");
    out.put(Key)
        .put("key 2")
        .put(Value)
        .put(BeginSeq)
        .put("a")
        .put("b")
        .put("c")
        .put(EndSeq);
    out.put(EndMap);
    out.put(EndSeq);
    expect_emit(
        &out,
        "-   key 1: value 1\n    key 2:\n        -   a\n        -   b\n        - \
         \x20 c",
    );
}

/// yaml-cpp 0.8.0 `EmitterTest.SimpleGlobalSettings`.
#[test]
fn simple_global_settings() {
    let mut out = Emitter::new();
    out.set_indent(4);
    out.set_map_format(LongKey);

    out.put(BeginSeq);
    out.put(BeginMap);
    out.put(Key).put("key 1").put(Value).put("value 1");
    out.put(Key)
        .put("key 2")
        .put(Value)
        .put(Flow)
        .put(BeginSeq)
        .put("a")
        .put("b")
        .put("c")
        .put(EndSeq);
    out.put(EndMap);
    out.put(EndSeq);
    expect_emit(
        &out,
        "-   ? key 1\n    : value 1\n    ? key 2\n    : [a, b, c]",
    );
}

/// yaml-cpp 0.8.0 `EmitterTest.GlobalLongKeyOnSeq`.
#[test]
fn global_long_key_on_seq() {
    let mut out = Emitter::new();
    out.set_map_format(LongKey);

    out.put(BeginMap);
    out.put(Key).put(&Anchor("key".into()));
    out.put(BeginSeq).put("a").put("b").put(EndSeq);
    out.put(Value).put(&Anchor("value".into()));
    out.put(BeginSeq).put("c").put("d").put(EndSeq);
    out.put(Key)
        .put(&Alias("key".into()))
        .put(Value)
        .put(&Alias("value".into()));
    out.put(EndMap);
    expect_emit(
        &out,
        "? &key\n  - a\n  - b\n: &value\n  - c\n  - d\n? *key\n: *value",
    );
}

/// yaml-cpp 0.8.0 `EmitterTest.GlobalLongKeyOnMap`.
#[test]
fn global_long_key_on_map() {
    let mut out = Emitter::new();
    out.set_map_format(LongKey);

    out.put(BeginMap);
    out.put(Key).put(&Anchor("key".into()));
    out.put(BeginMap).put("a").put("b").put(EndMap);
    out.put(Value).put(&Anchor("value".into()));
    out.put(BeginMap).put("c").put("d").put(EndMap);
    out.put(Key)
        .put(&Alias("key".into()))
        .put(Value)
        .put(&Alias("value".into()));
    out.put(EndMap);
    expect_emit(
        &out,
        "? &key\n  ? a\n  : b\n: &value\n  ? c\n  : d\n? *key\n: *value",
    );
}

/// yaml-cpp 0.8.0 `EmitterTest.ComplexGlobalSettings`.
#[test]
fn complex_global_settings() {
    let mut out = Emitter::new();
    out.put(BeginSeq);
    out.put(Block);
    out.put(BeginMap);
    out.put(Key).put("key 1").put(Value).put("value 1");
    out.put(Key).put("key 2").put(Value);
    out.set_seq_format(Flow);
    out.put(BeginSeq).put("a").put("b").put("c").put(EndSeq);
    out.put(EndMap);
    out.put(BeginMap);
    out.put(Key).put(BeginSeq).put(1).put(2).put(EndSeq);
    out.put(Value)
        .put(BeginMap)
        .put(Key)
        .put("a")
        .put(Value)
        .put("b")
        .put(EndMap);
    out.put(EndMap);
    out.put(EndSeq);
    expect_emit(
        &out,
        "- key 1: value 1\n  key 2: [a, b, c]\n- [1, 2]:\n    a: b",
    );
}

/// yaml-cpp 0.8.0 `EmitterTest.Null`.
#[test]
fn null() {
    let mut out = Emitter::new();
    out.put(BeginSeq);
    out.put(Null);
    out.put(BeginMap);
    out.put(Key).put("null value").put(Value).put(Null);
    out.put(Key).put(Null).put(Value).put("null key");
    out.put(EndMap);
    out.put(EndSeq);
    expect_emit(&out, "- ~\n- null value: ~\n  ~: null key");
}

/// yaml-cpp 0.8.0 `EmitterTest.OutputCharset`.
#[test]
fn output_charset() {
    let mut out = Emitter::new();
    out.put(BeginSeq);
    out.set_output_charset(EmitNonAscii);
    out.put("\x24 \u{a2} \u{20ac}");
    out.set_output_charset(EscapeNonAscii);
    out.put("\x24 \u{a2} \u{20ac}");
    out.put(EndSeq);
    expect_emit(&out, "- \x24 \u{a2} \u{20ac}\n- \"\x24 \\xa2 \\u20ac\"");
}

/// yaml-cpp 0.8.0 `EmitterTest.EscapedUnicode`.
#[test]
fn escaped_unicode() {
    let mut out = Emitter::new();
    out.put(EscapeNonAscii)
        .put("\x24 \u{a2} \u{20ac} \u{24b62}");
    expect_emit(&out, "\"$ \\xa2 \\u20ac \\U00024b62\"");
}

/// yaml-cpp 0.8.0 `EmitterTest.Unicode`.
#[test]
fn unicode() {
    let mut out = Emitter::new();
    out.put("\x24 \u{a2} \u{20ac} \u{24b62}");
    expect_emit(&out, "\x24 \u{a2} \u{20ac} \u{24b62}");
}

/// yaml-cpp 0.8.0 `EmitterTest.DoubleQuotedUnicode`.
#[test]
fn double_quoted_unicode() {
    let mut out = Emitter::new();
    out.put(DoubleQuoted).put("\x24 \u{a2} \u{20ac} \u{24b62}");
    expect_emit(&out, "\"\x24 \u{a2} \u{20ac} \u{24b62}\"");
}

/// yaml-cpp 0.8.0 `EmitterTest.EscapedJsonString`.
#[test]
fn escaped_json_string() {
    let mut out = Emitter::new();
    out.set_string_format(DoubleQuoted);
    out.set_output_charset(EscapeAsJson);
    out.put(
        "\" \\ \
         \x01 \x02 \x03 \x04 \x05 \x06 \x07 \x08 \x09 \x0A \x0B \x0C \x0D \x0E \x0F \
         \x10 \x11 \x12 \x13 \x14 \x15 \x16 \x17 \x18 \x19 \x1A \x1B \x1C \x1D \x1E \x1F \
         \x24 \u{a2} \u{20ac} \u{24b62}",
    );
    expect_emit(
        &out,
        concat!(
            r#""\" \\ \u0001 \u0002 \u0003 \u0004 \u0005 \u0006 \u0007 \b \t "#,
            r#"\n \u000b \f \r \u000e \u000f \u0010 \u0011 \u0012 \u0013 "#,
            r#"\u0014 \u0015 \u0016 \u0017 \u0018 \u0019 \u001a \u001b "#,
            r#"\u001c \u001d \u001e \u001f "#,
            "$ \u{a2} \u{20ac} \u{24b62}\""
        ),
    );
}

/// yaml-cpp 0.8.0 `EmitterTest.EscapedCharacters`.
#[test]
fn escaped_characters() {
    let mut out = Emitter::new();
    out.put(BeginSeq)
        .put(0x00u8)
        .put(0x0Cu8)
        .put(0x0Du8)
        .put(EndSeq);
    expect_emit(&out, "- \"\\x00\"\n- \"\\f\"\n- \"\\r\"");
}

/// yaml-cpp 0.8.0 `EmitterTest.CharactersEscapedAsJson`.
#[test]
fn characters_escaped_as_json() {
    let mut out = Emitter::new();
    out.set_output_charset(EscapeAsJson);
    out.put(BeginSeq)
        .put(0x00u8)
        .put(0x0Cu8)
        .put(0x0Du8)
        .put(EndSeq);
    expect_emit(&out, "- \"\\u0000\"\n- \"\\f\"\n- \"\\r\"");
}

/// yaml-cpp 0.8.0 `EmitterTest.DoubleQuotedString`.
#[test]
fn double_quoted_string() {
    let mut out = Emitter::new();
    out.put(DoubleQuoted)
        .put("\" \\ \n \t \r \x08 \x15 \u{feff} \x24");
    expect_emit(&out, "\"\\\" \\\\ \\n \\t \\r \\b \\x15 \\ufeff $\"");
}

/// The user type of the `UserType` tests, and its `operator<<`.
struct Foo {
    x: i32,
    bar: String,
}

impl super::Emittable for &Foo {
    fn emit(self, out: &mut Emitter) {
        out.put(BeginMap);
        out.put(Key).put("x").put(Value).put(self.x);
        out.put(Key).put("bar").put(Value).put(&self.bar);
        out.put(EndMap);
    }
}

fn foo(x: i32, bar: &str) -> Foo {
    Foo {
        x,
        bar: bar.to_string(),
    }
}

/// yaml-cpp 0.8.0 `EmitterTest.UserType`.
#[test]
fn user_type() {
    let mut out = Emitter::new();
    out.put(BeginSeq);
    out.put(&foo(5, "hello"));
    out.put(&foo(3, "goodbye"));
    out.put(EndSeq);
    expect_emit(&out, "- x: 5\n  bar: hello\n- x: 3\n  bar: goodbye");
}

/// yaml-cpp 0.8.0 `EmitterTest.UserType2`.
#[test]
fn user_type2() {
    let mut out = Emitter::new();
    out.put(BeginSeq);
    out.put(&foo(5, "\r"));
    out.put(EndSeq);
    expect_emit(&out, "- x: 5\n  bar: \"\\r\"");
}

/// yaml-cpp 0.8.0 `EmitterTest.UserTypeInContainer`.
#[test]
fn user_type_in_container() {
    let fv = vec![foo(5, "hello"), foo(3, "goodbye")];
    let mut out = Emitter::new();
    out.put(&fv);
    expect_emit(&out, "- x: 5\n  bar: hello\n- x: 3\n  bar: goodbye");
}

/// The test's `operator<<(Emitter &, const T *)`: the value, or `Null`.
fn put_pointer<'a, T>(out: &mut Emitter, v: Option<&'a T>)
where
    &'a T: super::Emittable,
{
    match v {
        Some(v) => {
            out.put(v);
        }
        None => {
            out.put(Null);
        }
    }
}

/// yaml-cpp 0.8.0 `EmitterTest.PointerToInt`.
#[test]
fn pointer_to_int() {
    let value = 5;
    let mut out = Emitter::new();
    out.put(BeginSeq);
    put_pointer::<i32>(&mut out, Some(&value));
    put_pointer::<i32>(&mut out, None);
    out.put(EndSeq);
    expect_emit(&out, "- 5\n- ~");
}

/// yaml-cpp 0.8.0 `EmitterTest.PointerToUserType`.
#[test]
fn pointer_to_user_type() {
    let value = foo(5, "hello");
    let mut out = Emitter::new();
    out.put(BeginSeq);
    put_pointer::<Foo>(&mut out, Some(&value));
    put_pointer::<Foo>(&mut out, None);
    out.put(EndSeq);
    expect_emit(&out, "- x: 5\n  bar: hello\n- ~");
}

/// yaml-cpp 0.8.0 `EmitterTest.NewlineAtEnd`.
#[test]
fn newline_at_end() {
    let mut out = Emitter::new();
    out.put("Hello").put(Newline).put(Newline);
    expect_emit(&out, "Hello\n\n");
}

/// yaml-cpp 0.8.0 `EmitterTest.NewlineInBlockSequence`.
#[test]
fn newline_in_block_sequence() {
    let mut out = Emitter::new();
    out.put(BeginSeq);
    out.put("a")
        .put(Newline)
        .put("b")
        .put("c")
        .put(Newline)
        .put("d");
    out.put(EndSeq);
    expect_emit(&out, "- a\n\n- b\n- c\n\n- d");
}

/// yaml-cpp 0.8.0 `EmitterTest.NewlineInFlowSequence`.
#[test]
fn newline_in_flow_sequence() {
    let mut out = Emitter::new();
    out.put(Flow).put(BeginSeq);
    out.put("a")
        .put(Newline)
        .put("b")
        .put("c")
        .put(Newline)
        .put("d");
    out.put(EndSeq);
    expect_emit(&out, "[a,\nb, c,\nd]");
}

/// yaml-cpp 0.8.0 `EmitterTest.NewlineInBlockMap`.
#[test]
fn newline_in_block_map() {
    let mut out = Emitter::new();
    out.put(BeginMap);
    out.put(Key).put("a").put(Value).put("foo").put(Newline);
    out.put(Key).put("b").put(Newline).put(Value).put("bar");
    out.put(LongKey)
        .put(Key)
        .put("c")
        .put(Newline)
        .put(Value)
        .put("car");
    out.put(EndMap);
    expect_emit(&out, "a: foo\nb:\n  bar\n? c\n\n: car");
}

/// yaml-cpp 0.8.0 `EmitterTest.NewlineInFlowMap`.
#[test]
fn newline_in_flow_map() {
    let mut out = Emitter::new();
    out.put(Flow).put(BeginMap);
    out.put(Key).put("a").put(Value).put("foo").put(Newline);
    out.put(Key).put("b").put(Value).put("bar");
    out.put(EndMap);
    expect_emit(&out, "{a: foo,\nb: bar}");
}

/// yaml-cpp 0.8.0 `EmitterTest.LotsOfNewlines`.
#[test]
fn lots_of_newlines() {
    let mut out = Emitter::new();
    out.put(BeginSeq);
    out.put("a").put(Newline);
    out.put(BeginSeq);
    out.put("b").put("c").put(Newline);
    out.put(EndSeq);
    out.put(Newline);
    out.put(BeginMap);
    out.put(Newline)
        .put(Key)
        .put("d")
        .put(Value)
        .put(Newline)
        .put("e");
    out.put(LongKey)
        .put(Key)
        .put("f")
        .put(Newline)
        .put(Value)
        .put("foo");
    out.put(EndMap);
    out.put(EndSeq);
    expect_emit(
        &out,
        "- a\n\n-\n  - b\n  - c\n\n\n-\n  d:\n    e\n  ? f\n\n  : foo",
    );
}

/// yaml-cpp 0.8.0 `EmitterTest.Binary`.
#[test]
fn binary() {
    let mut out = Emitter::new();
    out.put(Binary(b"Hello, World!"));
    expect_emit(&out, "!!binary \"SGVsbG8sIFdvcmxkIQ==\"");
}

/// yaml-cpp 0.8.0 `EmitterTest.LongBinary`.
#[test]
fn long_binary() {
    let mut out = Emitter::new();
    out.put(Binary(
        b"Man is distinguished, not only by his reason, but by this \
          singular passion from other animals, which is a lust of the \
          mind, that by a perseverance of delight in the continued and \
          indefatigable generation of knowledge, exceeds the short \
          vehemence of any carnal pleasure.\n",
    ));
    expect_emit(
        &out,
        "!!binary \
         \"TWFuIGlzIGRpc3Rpbmd1aXNoZWQsIG5vdCBvbmx5IGJ5IGhpcyByZWFzb24sIGJ1dCBieS\
         B0aGlzIHNpbmd1bGFyIHBhc3Npb24gZnJvbSBvdGhlciBhbmltYWxzLCB3aGljaCBpcyBhIG\
         x1c3Qgb2YgdGhlIG1pbmQsIHRoYXQgYnkgYSBwZXJzZXZlcmFuY2Ugb2YgZGVsaWdodCBpbi\
         B0aGUgY29udGludWVkIGFuZCBpbmRlZmF0aWdhYmxlIGdlbmVyYXRpb24gb2Yga25vd2xlZG\
         dlLCBleGNlZWRzIHRoZSBzaG9ydCB2ZWhlbWVuY2Ugb2YgYW55IGNhcm5hbCBwbGVhc3VyZS\
         4K\"",
    );
}

/// yaml-cpp 0.8.0 `EmitterTest.EmptyBinary`.
#[test]
fn empty_binary() {
    let mut out = Emitter::new();
    out.put(Binary(b""));
    expect_emit(&out, "!!binary \"\"");
}

/// yaml-cpp 0.8.0 `EmitterTest.ColonAtEndOfScalar`.
#[test]
fn colon_at_end_of_scalar() {
    let mut out = Emitter::new();
    out.put("a:");
    expect_emit(&out, "\"a:\"");
}

/// yaml-cpp 0.8.0 `EmitterTest.ColonAsScalar`.
#[test]
fn colon_as_scalar() {
    let mut out = Emitter::new();
    out.put(BeginMap);
    out.put(Key).put("apple").put(Value).put(":");
    out.put(Key).put("banana").put(Value).put(":");
    out.put(EndMap);
    expect_emit(&out, "apple: \":\"\nbanana: \":\"");
}

/// yaml-cpp 0.8.0 `EmitterTest.ColonAtEndOfScalarInFlow`.
#[test]
fn colon_at_end_of_scalar_in_flow() {
    let mut out = Emitter::new();
    out.put(Flow)
        .put(BeginMap)
        .put(Key)
        .put("C:")
        .put(Value)
        .put("C:")
        .put(EndMap);
    expect_emit(&out, "{\"C:\": \"C:\"}");
}

/// yaml-cpp 0.8.0 `EmitterTest.GlobalBoolFormatting`.
#[test]
fn global_bool_formatting() {
    let mut out = Emitter::new();
    out.put(BeginSeq);
    for case in [UpperCase, LowerCase, CamelCase] {
        out.set_bool_format(case);
        for format in [YesNoBool, TrueFalseBool, OnOffBool] {
            out.set_bool_format(format);
            out.put(true);
            out.put(false);
        }
    }
    out.set_bool_format(ShortBool);
    out.set_bool_format(UpperCase);
    for format in [YesNoBool, TrueFalseBool, OnOffBool] {
        out.set_bool_format(format);
        out.put(true);
        out.put(false);
    }
    out.put(EndSeq);
    expect_emit(
        &out,
        "- YES\n- NO\n- TRUE\n- FALSE\n- ON\n- OFF\n\
         - yes\n- no\n- true\n- false\n- on\n- off\n\
         - Yes\n- No\n- True\n- False\n- On\n- Off\n\
         - Y\n- N\n- Y\n- N\n- Y\n- N",
    );
}

/// yaml-cpp 0.8.0 `EmitterTest.BoolFormatting`.
#[test]
fn bool_formatting() {
    let mut out = Emitter::new();
    out.put(BeginSeq);
    for format in [TrueFalseBool, YesNoBool, OnOffBool, ShortBool] {
        for value in [true, false] {
            for case in [UpperCase, CamelCase, LowerCase] {
                out.put(format).put(case).put(value);
            }
        }
    }
    out.put(EndSeq);
    expect_emit(
        &out,
        "- TRUE\n- True\n- true\n- FALSE\n- False\n- false\n\
         - YES\n- Yes\n- yes\n- NO\n- No\n- no\n\
         - ON\n- On\n- on\n- OFF\n- Off\n- off\n\
         - Y\n- Y\n- y\n- N\n- N\n- n",
    );
}

/// yaml-cpp 0.8.0 `EmitterTest.GlobalNullFormatting`.
#[test]
fn global_null_formatting() {
    let mut out = Emitter::new();
    out.put(Flow).put(BeginSeq);
    for format in [LowerNull, UpperNull, CamelNull, TildeNull] {
        out.set_null_format(format);
        out.put(Null);
    }
    out.put(EndSeq);
    expect_emit(&out, "[null, NULL, Null, ~]");
}

/// yaml-cpp 0.8.0 `EmitterTest.NullFormatting`.
#[test]
fn null_formatting() {
    let mut out = Emitter::new();
    out.put(Flow).put(BeginSeq);
    out.put(LowerNull).put(Null);
    out.put(UpperNull).put(Null);
    out.put(CamelNull).put(Null);
    out.put(TildeNull).put(Null);
    out.put(EndSeq);
    expect_emit(&out, "[null, NULL, Null, ~]");
}

/// yaml-cpp 0.8.0 `EmitterTest.ImplicitDocStart`.
#[test]
fn implicit_doc_start() {
    let mut out = Emitter::new();
    out.put("Hi");
    out.put("Bye");
    out.put("Oops");
    expect_emit(&out, "Hi\n---\nBye\n---\nOops");
}

/// yaml-cpp 0.8.0 `EmitterTest.EmptyString`.
#[test]
fn empty_string() {
    let mut out = Emitter::new();
    out.put(BeginMap);
    out.put(Key).put("key").put(Value).put("");
    out.put(EndMap);
    expect_emit(&out, "key: \"\"");
}

/// yaml-cpp 0.8.0 `EmitterTest.SingleChar`.
#[test]
fn single_char() {
    let mut out = Emitter::new();
    out.put(BeginSeq);
    out.put(b'a');
    out.put(b':');
    out.put(0x10u8);
    out.put(b'\n');
    out.put(b' ');
    out.put(b'\t');
    out.put(EndSeq);
    expect_emit(
        &out,
        "- a\n- \":\"\n- \"\\x10\"\n- \"\\n\"\n- \" \"\n- \"\\t\"",
    );
}

/// yaml-cpp 0.8.0 `EmitterTest.DefaultPrecision`.
#[test]
fn default_precision() {
    let mut out = Emitter::new();
    out.put(BeginSeq);
    out.put(1.3125f32);
    out.put(1.23455810546875f64);
    out.put(EndSeq);
    expect_emit(&out, "- 1.3125\n- 1.23455810546875");
}

/// yaml-cpp 0.8.0 `EmitterTest.SetPrecision`.
#[test]
fn set_precision() {
    let mut out = Emitter::new();
    out.put(BeginSeq);
    out.put(float_precision(3)).put(1.3125f32);
    out.put(double_precision(6)).put(1.23455810546875f64);
    out.put(EndSeq);
    expect_emit(&out, "- 1.31\n- 1.23456");
}

/// yaml-cpp 0.8.0 `EmitterTest.DashInBlockContext`.
#[test]
fn dash_in_block_context() {
    let mut out = Emitter::new();
    out.put(BeginMap);
    out.put(Key).put("key").put(Value).put("-");
    out.put(EndMap);
    expect_emit(&out, "key: \"-\"");
}

/// yaml-cpp 0.8.0 `EmitterTest.HexAndOct`.
#[test]
fn hex_and_oct() {
    let mut out = Emitter::new();
    out.put(Flow).put(BeginSeq);
    out.put(31);
    out.put(Hex).put(31);
    out.put(Oct).put(31);
    out.put(EndSeq);
    expect_emit(&out, "[31, 0x1f, 037]");
}

/// yaml-cpp 0.8.0 `EmitterTest.CompactMapWithNewline`.
#[test]
fn compact_map_with_newline() {
    let mut out = Emitter::new();
    out.put(&Comment("Characteristics".into()));
    out.put(BeginSeq);
    out.put(BeginMap);
    out.put(Key).put("color").put(Value).put("blue");
    out.put(Key).put("height").put(Value).put(120);
    out.put(EndMap);
    out.put(Newline).put(Newline);
    out.put(&Comment("Skills".into()));
    out.put(BeginMap);
    out.put(Key).put("attack").put(Value).put(23);
    out.put(Key).put("intelligence").put(Value).put(56);
    out.put(EndMap);
    out.put(EndSeq);
    expect_emit(
        &out,
        "# Characteristics\n- color: blue\n  height: 120\n\n# Skills\n- attack: 23\n  \
         intelligence: 56",
    );
}

/// yaml-cpp 0.8.0 `EmitterTest.ForceSingleQuotedToDouble`.
#[test]
fn force_single_quoted_to_double() {
    let mut out = Emitter::new();
    out.put(SingleQuoted).put("Hello\nWorld");
    expect_emit(&out, "\"Hello\\nWorld\"");
}

/// yaml-cpp 0.8.0 `EmitterTest.QuoteNull`.
#[test]
fn quote_null() {
    let mut out = Emitter::new();
    out.put("null");
    expect_emit(&out, "\"null\"");
}

/// yaml-cpp 0.8.0 `EmitterTest.ValueOfDoubleQuote`.
#[test]
fn value_of_double_quote() {
    let mut out = Emitter::new();
    out.put(BeginMap);
    out.put(Key).put("foo").put(Value).put(b'"');
    out.put(EndMap);
    expect_emit(&out, "foo: \"\\\"\"");
}

/// yaml-cpp 0.8.0 `EmitterTest.ValueOfBackslash`.
#[test]
fn value_of_backslash() {
    let mut out = Emitter::new();
    out.put(BeginMap);
    out.put(Key).put("foo").put(Value).put(b'\\');
    out.put(EndMap);
    expect_emit(&out, "foo: \"\\\\\"");
}

/// yaml-cpp 0.8.0 `EmitterTest.Infinity`.
#[test]
fn infinity() {
    let mut out = Emitter::new();
    out.put(BeginMap);
    out.put(Key).put("foo").put(Value).put(f32::INFINITY);
    out.put(Key).put("bar").put(Value).put(f64::INFINITY);
    out.put(EndMap);
    expect_emit(&out, "foo: .inf\nbar: .inf");
}

/// yaml-cpp 0.8.0 `EmitterTest.NegInfinity`.
#[test]
fn neg_infinity() {
    let mut out = Emitter::new();
    out.put(BeginMap);
    out.put(Key).put("foo").put(Value).put(-f32::INFINITY);
    out.put(Key).put("bar").put(Value).put(-f64::INFINITY);
    out.put(EndMap);
    expect_emit(&out, "foo: -.inf\nbar: -.inf");
}

/// yaml-cpp 0.8.0 `EmitterTest.NaN`.
#[test]
fn nan() {
    let mut out = Emitter::new();
    out.put(BeginMap);
    out.put(Key).put("foo").put(Value).put(f32::NAN);
    out.put(Key).put("bar").put(Value).put(f64::NAN);
    out.put(EndMap);
    expect_emit(&out, "foo: .nan\nbar: .nan");
}

/// yaml-cpp 0.8.0 `EmitterTest.ComplexFlowSeqEmbeddingAMapWithNewLine`.
#[test]
fn complex_flow_seq_embedding_a_map_with_new_line() {
    let mut out = Emitter::new();
    out.put(BeginMap);
    for node in ["NodeA", "NodeB"] {
        out.put(Key).put(node).put(Value).put(BeginMap);
        out.put(Key).put("k").put(Value).put(Flow).put(BeginSeq);
        for i in 0..2 {
            out.put(BeginMap)
                .put(Key)
                .put("i")
                .put(Value)
                .put(i)
                .put(EndMap)
                .put(Newline);
        }
        out.put(EndSeq);
        out.put(EndMap);
    }
    out.put(EndMap);
    expect_emit(
        &out,
        "NodeA:\n  k: [{i: 0},\n    {i: 1},\n    ]\nNodeB:\n  k: [{i: 0},\n    {i: 1},\n    ]",
    );
}

/// yaml-cpp 0.8.0 `EmitterTest.ComplexFlowSeqEmbeddingAMapWithNewLineUsingAliases`.
#[test]
fn complex_flow_seq_embedding_a_map_with_new_line_using_aliases() {
    let mut out = Emitter::new();
    out.put(BeginMap);
    out.put(Key)
        .put("Node")
        .put(&Anchor("Node".into()))
        .put(Value)
        .put(BeginMap);
    out.put(Key).put("k").put(Value).put(Flow).put(BeginSeq);
    out.put(BeginMap)
        .put(Key)
        .put("i")
        .put(Value)
        .put(0)
        .put(EndMap);
    out.put(Newline);
    out.put(BeginMap)
        .put(Key)
        .put("i")
        .put(Value)
        .put(1)
        .put(EndMap);
    out.put(Newline);
    out.put(EndSeq).put(EndMap);
    out.put(Key).put("NodeA").put(&Alias("Node".into()));
    out.put(Key).put("NodeB").put(&Alias("Node".into()));
    out.put(EndMap);
    expect_emit(
        &out,
        "Node: &Node\n  k: [{i: 0},\n    {i: 1},\n    ]\nNodeA: *Node\nNodeB: *Node",
    );
}

/// yaml-cpp 0.8.0 `EmitterTest.ComplexFlowSeqEmbeddingAMapUsingAliases`.
#[test]
fn complex_flow_seq_embedding_a_map_using_aliases() {
    let mut out = Emitter::new();
    out.put(BeginMap);
    out.put(Key)
        .put("Node")
        .put(&Anchor("Node".into()))
        .put(Value)
        .put(BeginMap);
    out.put(Key).put("k").put(Value).put(Flow).put(BeginSeq);
    out.put(BeginMap)
        .put(Key)
        .put("i")
        .put(Value)
        .put(0)
        .put(EndMap);
    out.put(BeginMap)
        .put(Key)
        .put("i")
        .put(Value)
        .put(1)
        .put(EndMap);
    out.put(EndSeq).put(EndMap);
    out.put(Key).put("NodeA").put(&Alias("Node".into()));
    out.put(Key).put("NodeB").put(&Alias("Node".into()));
    out.put(EndMap);
    expect_emit(
        &out,
        "Node: &Node\n  k: [{i: 0}, {i: 1}]\nNodeA: *Node\nNodeB: *Node",
    );
}

/// yaml-cpp 0.8.0 `EmitterTest.ComplexFlowSeqEmbeddingAMapWithNewLineUsingAliases2`.
#[test]
fn complex_flow_seq_embedding_a_map_with_new_line_using_aliases2() {
    let mut out = Emitter::new();
    out.put(BeginMap);
    out.put(Key)
        .put("Seq")
        .put(&Anchor("Seq".into()))
        .put(Flow)
        .put(BeginSeq);
    out.put(BeginMap)
        .put(Key)
        .put("i")
        .put(Value)
        .put(0)
        .put(EndMap);
    out.put(Newline);
    out.put(BeginMap)
        .put(Key)
        .put("i")
        .put(Value)
        .put(1)
        .put(EndMap);
    out.put(Newline);
    out.put(EndSeq);
    for node in ["NodeA", "NodeB"] {
        out.put(Key).put(node).put(Value).put(BeginMap);
        out.put(Key)
            .put("k")
            .put(Value)
            .put(&Alias("Seq".into()))
            .put(EndMap);
    }
    out.put(EndMap);
    expect_emit(
        &out,
        "Seq: &Seq [{i: 0},\n  {i: 1},\n  ]\nNodeA:\n  k: *Seq\nNodeB:\n  k: *Seq",
    );
}

/// yaml-cpp 0.8.0 `EmitterTest.ComplexFlowSeqEmbeddingAMapUsingAliases2`.
#[test]
fn complex_flow_seq_embedding_a_map_using_aliases2() {
    let mut out = Emitter::new();
    out.put(BeginMap);
    out.put(Key)
        .put("Seq")
        .put(&Anchor("Seq".into()))
        .put(Value)
        .put(Flow)
        .put(BeginSeq);
    out.put(BeginMap)
        .put(Key)
        .put("i")
        .put(Value)
        .put(0)
        .put(EndMap);
    out.put(BeginMap)
        .put(Key)
        .put("i")
        .put(Value)
        .put(1)
        .put(EndMap);
    out.put(EndSeq);
    for node in ["NodeA", "NodeB"] {
        out.put(Key).put(node).put(Value).put(BeginMap);
        out.put(Key)
            .put("k")
            .put(Value)
            .put(&Alias("Seq".into()))
            .put(EndMap);
    }
    out.put(EndMap);
    expect_emit(
        &out,
        "Seq: &Seq [{i: 0}, {i: 1}]\nNodeA:\n  k: *Seq\nNodeB:\n  k: *Seq",
    );
}

/// yaml-cpp 0.8.0 `EmitterTest.ComplexFlowSeqEmbeddingAMapWithNewLineUsingAliases3`.
#[test]
fn complex_flow_seq_embedding_a_map_with_new_line_using_aliases3() {
    let mut out = Emitter::new();
    out.put(BeginMap);
    out.put(Key).put("Keys").put(Value).put(Flow).put(BeginSeq);
    for (anchor, i) in [("k0", 0), ("k1", 1)] {
        out.put(&Anchor(anchor.into()))
            .put(BeginMap)
            .put(Key)
            .put("i")
            .put(Value)
            .put(i)
            .put(EndMap)
            .put(Newline);
    }
    out.put(EndSeq);
    for node in ["NodeA", "NodeB"] {
        out.put(Key).put(node).put(Value).put(BeginMap);
        out.put(Key).put("k").put(Value).put(Flow).put(BeginSeq);
        out.put(&Alias("k0".into()))
            .put(Newline)
            .put(&Alias("k1".into()))
            .put(Newline);
        out.put(EndSeq).put(EndMap);
    }
    out.put(EndMap);
    expect_emit(
        &out,
        "Keys: [&k0 {i: 0},\n&k1 {i: 1},\n  ]\nNodeA:\n  k: [*k0,\n  *k1,\n    ]\nNodeB:\n  \
         k: [*k0,\n  *k1,\n    ]",
    );
}

/// yaml-cpp 0.8.0 `EmitterTest.ComplexFlowSeqEmbeddingAMapUsingAliases3a`.
#[test]
fn complex_flow_seq_embedding_a_map_using_aliases3a() {
    let mut out = Emitter::new();
    out.put(BeginMap);
    out.put(Key).put("Keys").put(Value).put(BeginSeq);
    for (anchor, i) in [("k0", 0), ("k1", 1)] {
        out.put(&Anchor(anchor.into()))
            .put(BeginMap)
            .put(Key)
            .put("i")
            .put(Value)
            .put(i)
            .put(EndMap);
    }
    out.put(EndSeq);
    for node in ["NodeA", "NodeB"] {
        out.put(Key).put(node).put(Value).put(BeginMap);
        out.put(Key).put("k").put(Value).put(Flow).put(BeginSeq);
        out.put(&Alias("k0".into())).put(&Alias("k1".into()));
        out.put(EndSeq).put(EndMap);
    }
    out.put(EndMap);
    expect_emit(
        &out,
        "Keys:\n  - &k0\n    i: 0\n  - &k1\n    i: 1\nNodeA:\n  k: [*k0, *k1]\nNodeB:\n  \
         k: [*k0, *k1]",
    );
}

/// yaml-cpp 0.8.0 `EmitterTest.ComplexFlowSeqEmbeddingAMapUsingAliases3b`.
#[test]
fn complex_flow_seq_embedding_a_map_using_aliases3b() {
    let mut out = Emitter::new();
    out.put(BeginMap);
    out.put(Key).put("Keys").put(Value).put(Flow).put(BeginSeq);
    for (anchor, i) in [("k0", 0), ("k1", 1)] {
        out.put(&Anchor(anchor.into()))
            .put(BeginMap)
            .put(Key)
            .put("i")
            .put(Value)
            .put(i)
            .put(EndMap);
    }
    out.put(EndSeq);
    for node in ["NodeA", "NodeB"] {
        out.put(Key).put(node).put(Value).put(BeginMap);
        out.put(Key).put("k").put(Value).put(Flow).put(BeginSeq);
        out.put(&Alias("k0".into())).put(&Alias("k1".into()));
        out.put(EndSeq).put(EndMap);
    }
    out.put(EndMap);
    expect_emit(
        &out,
        "Keys: [&k0 {i: 0}, &k1 {i: 1}]\nNodeA:\n  k: [*k0, *k1]\nNodeB:\n  k: [*k0, *k1]",
    );
}

/// yaml-cpp 0.8.0 `EmitterErrorTest.BadLocalTag`.
#[test]
fn bad_local_tag() {
    let mut out = Emitter::new();
    out.put(local_tag("e!far")).put("bar");
    expect_emit_error(&out, "invalid tag");
}

/// yaml-cpp 0.8.0 `EmitterErrorTest.BadTagAndTag`.
#[test]
fn bad_tag_and_tag() {
    let mut out = Emitter::new();
    out.put(verbatim_tag("!far"))
        .put(verbatim_tag("!foo"))
        .put("bar");
    expect_emit_error(&out, error_msg::INVALID_TAG);
}

/// yaml-cpp 0.8.0 `EmitterErrorTest.BadAnchorAndAnchor`.
#[test]
fn bad_anchor_and_anchor() {
    let mut out = Emitter::new();
    out.put(&Anchor("far".into()))
        .put(&Anchor("foo".into()))
        .put("bar");
    expect_emit_error(&out, error_msg::INVALID_ANCHOR);
}

/// yaml-cpp 0.8.0 `EmitterErrorTest.BadEmptyAnchorOnGroup`.
#[test]
fn bad_empty_anchor_on_group() {
    let mut out = Emitter::new();
    out.put(BeginSeq)
        .put("bar")
        .put(&Anchor("foo".into()))
        .put(EndSeq);
    expect_emit_error(&out, error_msg::INVALID_ANCHOR);
}

/// yaml-cpp 0.8.0 `EmitterErrorTest.BadEmptyTagOnGroup`.
#[test]
fn bad_empty_tag_on_group() {
    let mut out = Emitter::new();
    out.put(BeginSeq)
        .put("bar")
        .put(verbatim_tag("!foo"))
        .put(EndSeq);
    expect_emit_error(&out, error_msg::INVALID_TAG);
}

/// yaml-cpp 0.8.0 `EmitterErrorTest.ExtraEndSeq`.
#[test]
fn extra_end_seq() {
    let mut out = Emitter::new();
    out.put(BeginSeq);
    out.put("Hello");
    out.put("World");
    out.put(EndSeq);
    out.put(EndSeq);
    expect_emit_error(&out, error_msg::UNEXPECTED_END_SEQ);
}

/// yaml-cpp 0.8.0 `EmitterErrorTest.ExtraEndMap`.
#[test]
fn extra_end_map() {
    let mut out = Emitter::new();
    out.put(BeginMap);
    out.put(Key).put("Hello").put(Value).put("World");
    out.put(EndMap);
    out.put(EndMap);
    expect_emit_error(&out, error_msg::UNEXPECTED_END_MAP);
}

/// yaml-cpp 0.8.0 `EmitterErrorTest.InvalidAnchor`.
#[test]
fn invalid_anchor() {
    let mut out = Emitter::new();
    out.put(BeginSeq);
    out.put(&Anchor("new\nline".into())).put("Test");
    out.put(EndSeq);
    expect_emit_error(&out, error_msg::INVALID_ANCHOR);
}

/// yaml-cpp 0.8.0 `EmitterErrorTest.InvalidAlias`.
#[test]
fn invalid_alias() {
    let mut out = Emitter::new();
    out.put(BeginSeq);
    out.put(&Alias("new\nline".into()));
    out.put(EndSeq);
    expect_emit_error(&out, error_msg::INVALID_ALIAS);
}
