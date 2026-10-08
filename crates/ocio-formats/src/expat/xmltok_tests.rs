// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from expat 2.7.2 (MIT License, Copyright (c) 1998-2000 Thai Open Source Software Center
// Ltd and Clark Cooper, Copyright (c) 2001-2025 Expat maintainers); see mod.rs.

//! The tests of expat 2.7.2's own suite (`expat/tests/`) that reach the tokenizer directly.
//! Its other tests go through `XML_Parse`, and come with the parser.

use super::*;

// Examples, not masks (basic_tests.c:315-319).
const UTF8_LEAD_1: u8 = 0x7f; // 0b01111111
const UTF8_LEAD_2: u8 = 0xdf; // 0b11011111
const UTF8_LEAD_3: u8 = 0xef; // 0b11101111
const UTF8_LEAD_4: u8 = 0xf7; // 0b11110111
const UTF8_FOLLOW: u8 = 0xbf; // 0b10111111

/// Port of expat 2.7.2 `START_TEST(test_utf8_auto_align)` (tests/basic_tests.c:321-371).
#[test]
fn test_utf8_auto_align() {
    let cases: [(isize, &[u8]); 11] = [
        (0, b""),
        (0, &[UTF8_LEAD_1]),
        (-1, &[UTF8_LEAD_2]),
        (0, &[UTF8_LEAD_2, UTF8_FOLLOW]),
        (-1, &[UTF8_LEAD_3]),
        (-2, &[UTF8_LEAD_3, UTF8_FOLLOW]),
        (0, &[UTF8_LEAD_3, UTF8_FOLLOW, UTF8_FOLLOW]),
        (-1, &[UTF8_LEAD_4]),
        (-2, &[UTF8_LEAD_4, UTF8_FOLLOW]),
        (-3, &[UTF8_LEAD_4, UTF8_FOLLOW, UTF8_FOLLOW]),
        (0, &[UTF8_LEAD_4, UTF8_FOLLOW, UTF8_FOLLOW, UTF8_FOLLOW]),
    ];

    let mut success = true;
    for (i, (expected_movement_in_chars, input)) in cases.iter().enumerate() {
        let from_lim_initially = input.len();
        let from_lim = trim_to_complete_utf8_characters(input, 0, from_lim_initially);
        let actual_movement_in_chars = from_lim as isize - from_lim_initially as isize;
        if actual_movement_in_chars != *expected_movement_in_chars {
            success = false;
            println!(
                "[-] UTF-8 case {:2}: Expected movement by {:2} chars, actually moved by {:2} \
                 chars: \"{}\"",
                i + 1,
                expected_movement_in_chars,
                actual_movement_in_chars,
                input
                    .iter()
                    .map(|b| format!("\\x{b:02x}"))
                    .collect::<String>()
            );
        }
    }

    assert!(success, "UTF-8 auto-alignment is not bullet-proof");
}
