// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.8.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! Port of yaml-cpp 0.8.0 `test/specexamples.h`: the YAML 1.2 specification's examples, as
//! the tests use them. `ex6_25`, `ex6_27b`, `ex8_5` and `ex8_21`, which only the tests disabled
//! upstream use, are left out.

/// `ex2_1` (test/specexamples.h).
pub(crate) const EX2_1: &[u8] = b"- Mark McGwire\n\
    - Sammy Sosa\n\
    - Ken Griffey";

/// `ex2_2` (test/specexamples.h).
pub(crate) const EX2_2: &[u8] = b"hr:  65    # Home runs\n\
    avg: 0.278 # Batting average\n\
    rbi: 147   # Runs Batted In";

/// `ex2_3` (test/specexamples.h).
pub(crate) const EX2_3: &[u8] = b"american:\n\
    - Boston Red Sox\n\
    - Detroit Tigers\n\
    - New York Yankees\n\
    national:\n\
    - New York Mets\n\
    - Chicago Cubs\n\
    - Atlanta Braves";

/// `ex2_4` (test/specexamples.h).
pub(crate) const EX2_4: &[u8] = b"-\n\
    \x20\x20name: Mark McGwire\n\
    \x20\x20hr:   65\n\
    \x20\x20avg:  0.278\n\
    -\n\
    \x20\x20name: Sammy Sosa\n\
    \x20\x20hr:   63\n\
    \x20\x20avg:  0.288";

/// `ex2_5` (test/specexamples.h).
pub(crate) const EX2_5: &[u8] = b"- [name        , hr, avg  ]\n\
    - [Mark McGwire, 65, 0.278]\n\
    - [Sammy Sosa  , 63, 0.288]";

/// `ex2_6` (test/specexamples.h).
pub(crate) const EX2_6: &[u8] = b"Mark McGwire: {hr: 65, avg: 0.278}\n\
    Sammy Sosa: {\n\
    \x20\x20\x20\x20hr: 63,\n\
    \x20\x20\x20\x20avg: 0.288\n\
    \x20\x20}";

/// `ex2_7` (test/specexamples.h).
pub(crate) const EX2_7: &[u8] = b"# Ranking of 1998 home runs\n\
    ---\n\
    - Mark McGwire\n\
    - Sammy Sosa\n\
    - Ken Griffey\n\
    \n\
    # Team ranking\n\
    ---\n\
    - Chicago Cubs\n\
    - St Louis Cardinals";

/// `ex2_8` (test/specexamples.h).
pub(crate) const EX2_8: &[u8] = b"---\n\
    time: 20:03:20\n\
    player: Sammy Sosa\n\
    action: strike (miss)\n\
    ...\n\
    ---\n\
    time: 20:03:47\n\
    player: Sammy Sosa\n\
    action: grand slam\n\
    ...";

/// `ex2_9` (test/specexamples.h).
pub(crate) const EX2_9: &[u8] = b"---\n\
    hr: # 1998 hr ranking\n\
    \x20\x20- Mark McGwire\n\
    \x20\x20- Sammy Sosa\n\
    rbi:\n\
    \x20\x20# 1998 rbi ranking\n\
    \x20\x20- Sammy Sosa\n\
    \x20\x20- Ken Griffey";

/// `ex2_10` (test/specexamples.h).
pub(crate) const EX2_10: &[u8] = b"---\n\
    hr:\n\
    \x20\x20- Mark McGwire\n\
    \x20\x20# Following node labeled SS\n\
    \x20\x20- &SS Sammy Sosa\n\
    rbi:\n\
    \x20\x20- *SS # Subsequent occurrence\n\
    \x20\x20- Ken Griffey";

/// `ex2_11` (test/specexamples.h).
pub(crate) const EX2_11: &[u8] = b"? - Detroit Tigers\n\
    \x20\x20- Chicago cubs\n\
    :\n\
    \x20\x20- 2001-07-23\n\
    \n\
    ? [ New York Yankees,\n\
    \x20\x20\x20\x20Atlanta Braves ]\n\
    : [ 2001-07-02, 2001-08-12,\n\
    \x20\x20\x20\x202001-08-14 ]";

/// `ex2_12` (test/specexamples.h).
pub(crate) const EX2_12: &[u8] = b"---\n\
    # Products purchased\n\
    - item    : Super Hoop\n\
    \x20\x20quantity: 1\n\
    - item    : Basketball\n\
    \x20\x20quantity: 4\n\
    - item    : Big Shoes\n\
    \x20\x20quantity: 1";

/// `ex2_13` (test/specexamples.h).
pub(crate) const EX2_13: &[u8] = b"# ASCII Art\n\
    --- |\n\
    \x20\x20\\//||\\/||\n\
    \x20\x20// ||  ||__";

/// `ex2_14` (test/specexamples.h).
pub(crate) const EX2_14: &[u8] = b"--- >\n\
    \x20\x20Mark McGwire's\n\
    \x20\x20year was crippled\n\
    \x20\x20by a knee injury.";

/// `ex2_15` (test/specexamples.h).
pub(crate) const EX2_15: &[u8] = b">\n\
    \x20Sammy Sosa completed another\n\
    \x20fine season with great stats.\n\
    \x20\n\
    \x20\x20\x2063 Home Runs\n\
    \x20\x20\x200.288 Batting Average\n\
    \x20\n\
    \x20What a year!";

/// `ex2_16` (test/specexamples.h).
pub(crate) const EX2_16: &[u8] = b"name: Mark McGwire\n\
    accomplishment: >\n\
    \x20\x20Mark set a major league\n\
    \x20\x20home run record in 1998.\n\
    stats: |\n\
    \x20\x2065 Home Runs\n\
    \x20\x200.278 Batting Average\n";

/// `ex2_17` (test/specexamples.h).
pub(crate) const EX2_17: &[u8] = b"unicode: \"Sosa did fine.\\u263A\"\n\
    control: \"\\b1998\\t1999\\t2000\\n\"\n\
    hex esc: \"\\x0d\\x0a is \\r\\n\"\n\
    \n\
    single: '\"Howdy!\" he cried.'\n\
    quoted: ' # Not a ''comment''.'\n\
    tie-fighter: '|\\-*-/|'";

/// `ex2_18` (test/specexamples.h).
pub(crate) const EX2_18: &[u8] = b"plain:\n\
    \x20\x20This unquoted scalar\n\
    \x20\x20spans many lines.\n\
    \n\
    quoted: \"So does this\n\
    \x20\x20quoted scalar.\\n\"";

/// `ex2_19` (test/specexamples.h).
pub(crate) const EX2_19: &[u8] = b"canonical: 12345\n\
    decimal: +12345\n\
    octal: 0o14\n\
    hexadecimal: 0xC\n";

/// `ex2_20` (test/specexamples.h).
pub(crate) const EX2_20: &[u8] = b"canonical: 1.23015e+3\n\
    exponential: 12.3015e+02\n\
    fixed: 1230.15\n\
    negative infinity: -.inf\n\
    not a number: .NaN\n";

/// `ex2_21` (test/specexamples.h).
pub(crate) const EX2_21: &[u8] = b"null:\n\
    booleans: [ true, false ]\n\
    string: '012345'\n";

/// `ex2_22` (test/specexamples.h).
pub(crate) const EX2_22: &[u8] = b"canonical: 2001-12-15T02:59:43.1Z\n\
    iso8601: 2001-12-14t21:59:43.10-05:00\n\
    spaced: 2001-12-14 21:59:43.10 -5\n\
    date: 2002-12-14\n";

/// `ex2_23` (test/specexamples.h).
pub(crate) const EX2_23: &[u8] = b"---\n\
    not-date: !!str 2002-04-28\n\
    \n\
    picture: !!binary |\n\
    \x20R0lGODlhDAAMAIQAAP//9/X\n\
    \x2017unp5WZmZgAAAOfn515eXv\n\
    \x20Pz7Y6OjuDg4J+fn5OTk6enp\n\
    \x2056enmleECcgggoBADs=\n\
    \n\
    application specific tag: !something |\n\
    \x20The semantics of the tag\n\
    \x20above may be different for\n\
    \x20different documents.";

/// `ex2_24` (test/specexamples.h).
pub(crate) const EX2_24: &[u8] = b"%TAG ! tag:clarkevans.com,2002:\n\
    --- !shape\n\
    \x20\x20# Use the ! handle for presenting\n\
    \x20\x20# tag:clarkevans.com,2002:circle\n\
    - !circle\n\
    \x20\x20center: &ORIGIN {x: 73, y: 129}\n\
    \x20\x20radius: 7\n\
    - !line\n\
    \x20\x20start: *ORIGIN\n\
    \x20\x20finish: { x: 89, y: 102 }\n\
    - !label\n\
    \x20\x20start: *ORIGIN\n\
    \x20\x20color: 0xFFEEBB\n\
    \x20\x20text: Pretty vector drawing.";

/// `ex2_25` (test/specexamples.h).
pub(crate) const EX2_25: &[u8] = b"# Sets are represented as a\n\
    # Mapping where each key is\n\
    # associated with a null value\n\
    --- !!set\n\
    ? Mark McGwire\n\
    ? Sammy Sosa\n\
    ? Ken Griffey";

/// `ex2_26` (test/specexamples.h).
pub(crate) const EX2_26: &[u8] = b"# Ordered maps are represented as\n\
    # A sequence of mappings, with\n\
    # each mapping having one key\n\
    --- !!omap\n\
    - Mark McGwire: 65\n\
    - Sammy Sosa: 63\n\
    - Ken Griffey: 58";

/// `ex2_27` (test/specexamples.h).
pub(crate) const EX2_27: &[u8] = b"--- !<tag:clarkevans.com,2002:invoice>\n\
    invoice: 34843\n\
    date   : 2001-01-23\n\
    bill-to: &id001\n\
    \x20\x20\x20\x20given  : Chris\n\
    \x20\x20\x20\x20family : Dumars\n\
    \x20\x20\x20\x20address:\n\
    \x20\x20\x20\x20\x20\x20\x20\x20lines: |\n\
    \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20458 Walkman Dr.\n\
    \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20Suite #292\n\
    \x20\x20\x20\x20\x20\x20\x20\x20city    : Royal Oak\n\
    \x20\x20\x20\x20\x20\x20\x20\x20state   : MI\n\
    \x20\x20\x20\x20\x20\x20\x20\x20postal  : 48046\n\
    ship-to: *id001\n\
    product:\n\
    \x20\x20\x20\x20- sku         : BL394D\n\
    \x20\x20\x20\x20\x20\x20quantity    : 4\n\
    \x20\x20\x20\x20\x20\x20description : Basketball\n\
    \x20\x20\x20\x20\x20\x20price       : 450.00\n\
    \x20\x20\x20\x20- sku         : BL4438H\n\
    \x20\x20\x20\x20\x20\x20quantity    : 1\n\
    \x20\x20\x20\x20\x20\x20description : Super Hoop\n\
    \x20\x20\x20\x20\x20\x20price       : 2392.00\n\
    tax  : 251.42\n\
    total: 4443.52\n\
    comments:\n\
    \x20\x20\x20\x20Late afternoon is best.\n\
    \x20\x20\x20\x20Backup contact is Nancy\n\
    \x20\x20\x20\x20Billsmer @ 338-4338.";

/// `ex2_28` (test/specexamples.h).
pub(crate) const EX2_28: &[u8] = b"---\n\
    Time: 2001-11-23 15:01:42 -5\n\
    User: ed\n\
    Warning:\n\
    \x20\x20This is an error message\n\
    \x20\x20for the log file\n\
    ---\n\
    Time: 2001-11-23 15:02:31 -5\n\
    User: ed\n\
    Warning:\n\
    \x20\x20A slightly different error\n\
    \x20\x20message.\n\
    ---\n\
    Date: 2001-11-23 15:03:17 -5\n\
    User: ed\n\
    Fatal:\n\
    \x20\x20Unknown variable \"bar\"\n\
    Stack:\n\
    \x20\x20- file: TopClass.py\n\
    \x20\x20\x20\x20line: 23\n\
    \x20\x20\x20\x20code: |\n\
    \x20\x20\x20\x20\x20\x20x = MoreObject(\"345\\n\")\n\
    \x20\x20- file: MoreClass.py\n\
    \x20\x20\x20\x20line: 58\n\
    \x20\x20\x20\x20code: |-\n\
    \x20\x20\x20\x20\x20\x20foo = bar";

/// `ex5_3` (test/specexamples.h).
pub(crate) const EX5_3: &[u8] = b"sequence:\n\
    - one\n\
    - two\n\
    mapping:\n\
    \x20\x20? sky\n\
    \x20\x20: blue\n\
    \x20\x20sea : green";

/// `ex5_4` (test/specexamples.h).
pub(crate) const EX5_4: &[u8] = b"sequence: [ one, two, ]\n\
    mapping: { sky: blue, sea: green }";

/// `ex5_5` (test/specexamples.h).
pub(crate) const EX5_5: &[u8] = b"# Comment only.";

/// `ex5_6` (test/specexamples.h).
pub(crate) const EX5_6: &[u8] = b"anchored: !local &anchor value\n\
    alias: *anchor";

/// `ex5_7` (test/specexamples.h).
pub(crate) const EX5_7: &[u8] = b"literal: |\n\
    \x20\x20some\n\
    \x20\x20text\n\
    folded: >\n\
    \x20\x20some\n\
    \x20\x20text\n";

/// `ex5_8` (test/specexamples.h).
pub(crate) const EX5_8: &[u8] = b"single: 'text'\n\
    double: \"text\"";

/// `ex5_11` (test/specexamples.h).
pub(crate) const EX5_11: &[u8] = b"|\n\
    \x20\x20Line break (no glyph)\n\
    \x20\x20Line break (glyphed)\n";

/// `ex5_12` (test/specexamples.h).
pub(crate) const EX5_12: &[u8] = b"# Tabs and spaces\n\
    quoted: \"Quoted\t\"\n\
    block:\t|\n\
    \x20\x20void main() {\n\
    \x20\x20\tprintf(\"Hello, world!\\n\");\n\
    \x20\x20}";

/// `ex5_13` (test/specexamples.h).
pub(crate) const EX5_13: &[u8] = b"\"Fun with \\\\\n\
    \\\" \\a \\b \\e \\f \\\n\
    \\n \\r \\t \\v \\0 \\\n\
    \\  \\_ \\N \\L \\P \\\n\
    \\x41 \\u0041 \\U00000041\"";

/// `ex5_14` (test/specexamples.h).
pub(crate) const EX5_14: &[u8] = b"Bad escapes:\n\
    \x20\x20\"\\c\n\
    \x20\x20\\xq-\"";

/// `ex6_1` (test/specexamples.h).
pub(crate) const EX6_1: &[u8] = b"  # Leading comment line spaces are\n\
    \x20\x20\x20# neither content nor indentation.\n\
    \x20\x20\x20\x20\n\
    Not indented:\n\
    \x20By one space: |\n\
    \x20\x20\x20\x20By four\n\
    \x20\x20\x20\x20\x20\x20spaces\n\
    \x20Flow style: [    # Leading spaces\n\
    \x20\x20\x20By two,        # in flow style\n\
    \x20\x20Also by two,    # are neither\n\
    \x20\x20\tStill by two   # content nor\n\
    \x20\x20\x20\x20]             # indentation.";

/// `ex6_2` (test/specexamples.h).
pub(crate) const EX6_2: &[u8] = b"? a\n\
    : -\tb\n\
    \x20\x20-  -\tc\n\
    \x20\x20\x20\x20\x20- d";

/// `ex6_3` (test/specexamples.h).
pub(crate) const EX6_3: &[u8] = b"- foo:\t bar\n\
    - - baz\n\
    \x20\x20-\tbaz";

/// `ex6_4` (test/specexamples.h).
pub(crate) const EX6_4: &[u8] = b"plain: text\n\
    \x20\x20lines\n\
    quoted: \"text\n\
    \x20\x20\tlines\"\n\
    block: |\n\
    \x20\x20text\n\
    \x20\x20\x20\tlines\n";

/// `ex6_5` (test/specexamples.h).
pub(crate) const EX6_5: &[u8] = b"Folding:\n\
    \x20\x20\"Empty line\n\
    \x20\x20\x20\t\n\
    \x20\x20as a line feed\"\n\
    Chomping: |\n\
    \x20\x20Clipped empty lines\n\
    \x20";

/// `ex6_6` (test/specexamples.h).
pub(crate) const EX6_6: &[u8] = b">-\n\
    \x20\x20trimmed\n\
    \x20\x20\n\
    \x20\n\
    \n\
    \x20\x20as\n\
    \x20\x20space";

/// `ex6_7` (test/specexamples.h).
pub(crate) const EX6_7: &[u8] = b">\n\
    \x20\x20foo \n\
    \x20\n\
    \x20\x20\t bar\n\
    \n\
    \x20\x20baz\n";

/// `ex6_8` (test/specexamples.h).
pub(crate) const EX6_8: &[u8] = b"\"\n\
    \x20\x20foo \n\
    \x20\n\
    \x20\x20\t bar\n\
    \n\
    \x20\x20baz\n\
    \"";

/// `ex6_9` (test/specexamples.h).
pub(crate) const EX6_9: &[u8] = b"key:    # Comment\n\
    \x20\x20value";

/// `ex6_10` (test/specexamples.h).
pub(crate) const EX6_10: &[u8] = b"  # Comment\n\
    \x20\x20\x20\n\
    \n";

/// `ex6_11` (test/specexamples.h).
pub(crate) const EX6_11: &[u8] = b"key:    # Comment\n\
    \x20\x20\x20\x20\x20\x20\x20\x20# lines\n\
    \x20\x20value\n\
    \n";

/// `ex6_12` (test/specexamples.h).
pub(crate) const EX6_12: &[u8] = b"{ first: Sammy, last: Sosa }:\n\
    # Statistics:\n\
    \x20\x20hr:  # Home runs\n\
    \x20\x20\x20\x20\x2065\n\
    \x20\x20avg: # Average\n\
    \x20\x20\x200.278";

/// `ex6_13` (test/specexamples.h).
pub(crate) const EX6_13: &[u8] = b"%FOO  bar baz # Should be ignored\n\
    \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20# with a warning.\n\
    --- \"foo\"";

/// `ex6_14` (test/specexamples.h).
pub(crate) const EX6_14: &[u8] = b"%YAML 1.3 # Attempt parsing\n\
    \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20# with a warning\n\
    ---\n\
    \"foo\"";

/// `ex6_15` (test/specexamples.h).
pub(crate) const EX6_15: &[u8] = b"%YAML 1.2\n\
    %YAML 1.1\n\
    foo";

/// `ex6_16` (test/specexamples.h).
pub(crate) const EX6_16: &[u8] = b"%TAG !yaml! tag:yaml.org,2002:\n\
    ---\n\
    !yaml!str \"foo\"";

/// `ex6_17` (test/specexamples.h).
pub(crate) const EX6_17: &[u8] = b"%TAG ! !foo\n\
    %TAG ! !foo\n\
    bar";

/// `ex6_18` (test/specexamples.h).
pub(crate) const EX6_18: &[u8] = b"# Private\n\
    !foo \"bar\"\n\
    ...\n\
    # Global\n\
    %TAG ! tag:example.com,2000:app/\n\
    ---\n\
    !foo \"bar\"";

/// `ex6_19` (test/specexamples.h).
pub(crate) const EX6_19: &[u8] = b"%TAG !! tag:example.com,2000:app/\n\
    ---\n\
    !!int 1 - 3 # Interval, not integer";

/// `ex6_20` (test/specexamples.h).
pub(crate) const EX6_20: &[u8] = b"%TAG !e! tag:example.com,2000:app/\n\
    ---\n\
    !e!foo \"bar\"";

/// `ex6_21` (test/specexamples.h).
pub(crate) const EX6_21: &[u8] = b"%TAG !m! !my-\n\
    --- # Bulb here\n\
    !m!light fluorescent\n\
    ...\n\
    %TAG !m! !my-\n\
    --- # Color here\n\
    !m!light green";

/// `ex6_22` (test/specexamples.h).
pub(crate) const EX6_22: &[u8] = b"%TAG !e! tag:example.com,2000:app/\n\
    ---\n\
    - !e!foo \"bar\"";

/// `ex6_23` (test/specexamples.h).
pub(crate) const EX6_23: &[u8] = b"!!str &a1 \"foo\":\n\
    \x20\x20!!str bar\n\
    &a2 baz : *a1";

/// `ex6_24` (test/specexamples.h).
pub(crate) const EX6_24: &[u8] = b"!<tag:yaml.org,2002:str> foo :\n\
    \x20\x20!<!bar> baz";

/// `ex6_26` (test/specexamples.h).
pub(crate) const EX6_26: &[u8] = b"%TAG !e! tag:example.com,2000:app/\n\
    ---\n\
    - !local foo\n\
    - !!str bar\n\
    - !e!tag%21 baz\n";

/// `ex6_27a` (test/specexamples.h).
pub(crate) const EX6_27A: &[u8] = b"%TAG !e! tag:example,2000:app/\n\
    ---\n\
    - !e! foo";

/// `ex6_28` (test/specexamples.h).
pub(crate) const EX6_28: &[u8] = b"# Assuming conventional resolution:\n\
    - \"12\"\n\
    - 12\n\
    - ! 12";

/// `ex6_29` (test/specexamples.h).
pub(crate) const EX6_29: &[u8] = b"First occurrence: &anchor Value\n\
    Second occurrence: *anchor";

/// `ex7_1` (test/specexamples.h).
pub(crate) const EX7_1: &[u8] = b"First occurrence: &anchor Foo\n\
    Second occurrence: *anchor\n\
    Override anchor: &anchor Bar\n\
    Reuse anchor: *anchor";

/// `ex7_2` (test/specexamples.h).
pub(crate) const EX7_2: &[u8] = b"{\n\
    \x20\x20foo : !!str,\n\
    \x20\x20!!str : bar,\n\
    }";

/// `ex7_3` (test/specexamples.h).
pub(crate) const EX7_3: &[u8] = b"{\n\
    \x20\x20? foo :,\n\
    \x20\x20: bar,\n\
    }\n";

/// `ex7_4` (test/specexamples.h).
pub(crate) const EX7_4: &[u8] = b"\"implicit block key\" : [\n\
    \x20\x20\"implicit flow key\" : value,\n\
    \x20]";

/// `ex7_5` (test/specexamples.h).
pub(crate) const EX7_5: &[u8] = b"\"folded \n\
    to a space,\t\n\
    \x20\n\
    to a line feed, or \t\\\n\
    \x20\\ \tnon-content\"";

/// `ex7_6` (test/specexamples.h).
pub(crate) const EX7_6: &[u8] = b"\" 1st non-empty\n\
    \n\
    \x202nd non-empty \n\
    \t3rd non-empty \"";

/// `ex7_7` (test/specexamples.h).
pub(crate) const EX7_7: &[u8] = b" 'here''s to \"quotes\"'";

/// `ex7_8` (test/specexamples.h).
pub(crate) const EX7_8: &[u8] = b"'implicit block key' : [\n\
    \x20\x20'implicit flow key' : value,\n\
    \x20]";

/// `ex7_9` (test/specexamples.h).
pub(crate) const EX7_9: &[u8] = b"' 1st non-empty\n\
    \n\
    \x202nd non-empty \n\
    \t3rd non-empty '";

/// `ex7_10` (test/specexamples.h).
pub(crate) const EX7_10: &[u8] = b"# Outside flow collection:\n\
    - ::vector\n\
    - \": - ()\"\n\
    - Up, up, and away!\n\
    - -123\n\
    - http://example.com/foo#bar\n\
    # Inside flow collection:\n\
    - [ ::vector,\n\
    \x20\x20\": - ()\",\n\
    \x20\x20\"Up, up, and away!\",\n\
    \x20\x20-123,\n\
    \x20\x20http://example.com/foo#bar ]";

/// `ex7_11` (test/specexamples.h).
pub(crate) const EX7_11: &[u8] = b"implicit block key : [\n\
    \x20\x20implicit flow key : value,\n\
    \x20]";

/// `ex7_12` (test/specexamples.h).
pub(crate) const EX7_12: &[u8] = b"1st non-empty\n\
    \n\
    \x202nd non-empty \n\
    \t3rd non-empty";

/// `ex7_13` (test/specexamples.h).
pub(crate) const EX7_13: &[u8] = b"- [ one, two, ]\n\
    - [three ,four]";

/// `ex7_14` (test/specexamples.h).
pub(crate) const EX7_14: &[u8] = b"[\n\
    \"double\n\
    \x20quoted\", 'single\n\
    \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20quoted',\n\
    plain\n\
    \x20text, [ nested ],\n\
    single: pair,\n\
    ]";

/// `ex7_15` (test/specexamples.h).
pub(crate) const EX7_15: &[u8] = b"- { one : two , three: four , }\n\
    - {five: six,seven : eight}";

/// `ex7_16` (test/specexamples.h).
pub(crate) const EX7_16: &[u8] = b"{\n\
    ? explicit: entry,\n\
    implicit: entry,\n\
    ?\n\
    }";

/// `ex7_17` (test/specexamples.h).
pub(crate) const EX7_17: &[u8] = b"{\n\
    unquoted : \"separate\",\n\
    http://foo.com,\n\
    omitted value:,\n\
    : omitted key,\n\
    }";

/// `ex7_18` (test/specexamples.h).
pub(crate) const EX7_18: &[u8] = b"{\n\
    \"adjacent\":value,\n\
    \"readable\":value,\n\
    \"empty\":\n\
    }";

/// `ex7_19` (test/specexamples.h).
pub(crate) const EX7_19: &[u8] = b"[\n\
    foo: bar\n\
    ]";

/// `ex7_20` (test/specexamples.h).
pub(crate) const EX7_20: &[u8] = b"[\n\
    ? foo\n\
    \x20bar : baz\n\
    ]";

/// `ex7_21` (test/specexamples.h).
pub(crate) const EX7_21: &[u8] = b"- [ YAML : separate ]\n\
    - [ : empty key entry ]\n\
    - [ {JSON: like}:adjacent ]";

/// `ex7_22` (test/specexamples.h).
pub(crate) const EX7_22: &[u8] = b"[ foo\n\
    \x20bar: invalid,";

/// `ex7_23` (test/specexamples.h).
pub(crate) const EX7_23: &[u8] = b"- [ a, b ]\n\
    - { a: b }\n\
    - \"a\"\n\
    - 'b'\n\
    - c";

/// `ex7_24` (test/specexamples.h).
pub(crate) const EX7_24: &[u8] = b"- !!str \"a\"\n\
    - 'b'\n\
    - &anchor \"c\"\n\
    - *anchor\n\
    - !!str";

/// `ex8_1` (test/specexamples.h).
pub(crate) const EX8_1: &[u8] = b"- | # Empty header\n\
    \x20literal\n\
    - >1 # Indentation indicator\n\
    \x20\x20folded\n\
    - |+ # Chomping indicator\n\
    \x20keep\n\
    \n\
    - >1- # Both indicators\n\
    \x20\x20strip\n";

/// `ex8_2` (test/specexamples.h).
pub(crate) const EX8_2: &[u8] = b"- |\n\
    \x20detected\n\
    - >\n\
    \x20\n\
    \x20\x20\n\
    \x20\x20# detected\n\
    - |1\n\
    \x20\x20explicit\n\
    - >\n\
    \x20\t\n\
    \x20detected\n";

/// `ex8_3a` (test/specexamples.h).
pub(crate) const EX8_3A: &[u8] = b"- |\n\
    \x20\x20\n\
    \x20text";

/// `ex8_3b` (test/specexamples.h).
pub(crate) const EX8_3B: &[u8] = b"- >\n\
    \x20\x20text\n\
    \x20text";

/// `ex8_3c` (test/specexamples.h).
pub(crate) const EX8_3C: &[u8] = b"- |2\n\
    \x20text";

/// `ex8_4` (test/specexamples.h).
pub(crate) const EX8_4: &[u8] = b"strip: |-\n\
    \x20\x20text\n\
    clip: |\n\
    \x20\x20text\n\
    keep: |+\n\
    \x20\x20text\n";

/// `ex8_6` (test/specexamples.h).
pub(crate) const EX8_6: &[u8] = b"strip: >-\n\
    \n\
    clip: >\n\
    \n\
    keep: |+\n\
    \n";

/// `ex8_7` (test/specexamples.h).
pub(crate) const EX8_7: &[u8] = b"|\n\
    \x20literal\n\
    \x20\ttext\n\
    \n";

/// `ex8_8` (test/specexamples.h).
pub(crate) const EX8_8: &[u8] = b"|\n\
    \x20\n\
    \x20\x20\n\
    \x20\x20literal\n\
    \x20\x20\x20\n\
    \x20\x20\n\
    \x20\x20text\n\
    \n\
    \x20# Comment\n";

/// `ex8_9` (test/specexamples.h).
pub(crate) const EX8_9: &[u8] = b">\n\
    \x20folded\n\
    \x20text\n\
    \n";

/// `ex8_10` (test/specexamples.h).
pub(crate) const EX8_10: &[u8] = b">\n\
    \n\
    \x20folded\n\
    \x20line\n\
    \n\
    \x20next\n\
    \x20line\n\
    \x20\x20\x20* bullet\n\
    \n\
    \x20\x20\x20* list\n\
    \x20\x20\x20* lines\n\
    \n\
    \x20last\n\
    \x20line\n\
    \n\
    # Comment\n";

/// `ex8_11` (test/specexamples.h).
pub(crate) const EX8_11: &[u8] = EX8_10;

/// `ex8_12` (test/specexamples.h).
pub(crate) const EX8_12: &[u8] = EX8_10;

/// `ex8_13` (test/specexamples.h).
pub(crate) const EX8_13: &[u8] = EX8_10;

/// `ex8_14` (test/specexamples.h).
pub(crate) const EX8_14: &[u8] = b"block sequence:\n\
    \x20\x20- one\n\
    \x20\x20- two : three\n";

/// `ex8_15` (test/specexamples.h).
pub(crate) const EX8_15: &[u8] = b"- # Empty\n\
    - |\n\
    \x20block node\n\
    - - one # Compact\n\
    \x20\x20- two # sequence\n\
    - one: two # Compact mapping\n";

/// `ex8_16` (test/specexamples.h).
pub(crate) const EX8_16: &[u8] = b"block mapping:\n\
    \x20key: value\n";

/// `ex8_17` (test/specexamples.h).
pub(crate) const EX8_17: &[u8] = b"? explicit key # Empty value\n\
    ? |\n\
    \x20\x20block key\n\
    : - one # Explicit compact\n\
    \x20\x20- two # block value\n";

/// `ex8_18` (test/specexamples.h).
pub(crate) const EX8_18: &[u8] = b"plain key: in-line value\n\
    :  # Both empty\n\
    \"quoted key\":\n\
    - entry\n";

/// `ex8_19` (test/specexamples.h).
pub(crate) const EX8_19: &[u8] = b"- sun: yellow\n\
    - ? earth: blue\n\
    \x20\x20: moon: white\n";

/// `ex8_20` (test/specexamples.h).
pub(crate) const EX8_20: &[u8] = b"-\n\
    \x20\x20\"flow in block\"\n\
    - >\n\
    \x20Block scalar\n\
    - !!map # Block collection\n\
    \x20\x20foo : bar\n";

/// `ex8_22` (test/specexamples.h).
pub(crate) const EX8_22: &[u8] = b"sequence: !!seq\n\
    - entry\n\
    - !!seq\n\
    \x20- nested\n\
    mapping: !!map\n\
    \x20foo: bar\n";
