// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from expat 2.7.2 (MIT License, Copyright (c) 1998-2000 Thai Open Source Software Center
// Ltd and Clark Cooper, Copyright (c) 2001-2025 Expat maintainers); see mod.rs.

//! The tests of expat 2.7.2's own suite (`expat/tests/`) that reach only the ported parser:
//! those of `basic_tests.c` that set no handler but the element and character data ones, and
//! call no function the port leaves out, and of `acc_tests.c` the accounting of documents
//! without parameter or external entities.
//!
//! expat's runner runs the whole suite for each chunk size of `_XML_Parse_SINGLE_BYTES` from 0
//! to 5, with reparse deferral off and on (`runtests.c:101-110`); so does each test here
//! ([`each_run`]).
//!
//! Not ported, beyond those that need what the port leaves out: the tests that query the
//! parser from inside a handler (`test_line_and_column_numbers_inside_handlers`,
//! `test_attributes`, `test_deep_nested_attribute_entity`, `test_misc_expected_event_ptr_issue_980`):
//! the port's handlers can't reach their parser, and OCIO's don't; `test_negative_len_parse`:
//! the port's `parse` takes a slice, whose length can't be negative;
//! `test_buffer_can_grow_to_max`: it switches off the allocation tracker, which is not
//! ported, and grows buffers to 1 GiB; `test_misc_error_string`: a Rust enum has no
//! out-of-range values; `test_siphash_*`, `test_hash_collision`: the port's maps don't hash
//! with expat's SipHash.

use std::cell::RefCell;
use std::convert::Infallible;
use std::rc::Rc;

use super::*;

type P<'a> = Parser<'a, Infallible>;

/// One run of expat's suite: `g_chunkSize` and `g_reparseDeferralEnabledDefault`.
#[derive(Debug, Clone, Copy)]
struct Run {
    chunk_size: usize,
    deferral: bool,
}

/// Runs `test` for each chunk size from 0 to 5, with reparse deferral off and on
/// (`runtests.c:101-110`).
fn each_run(test: impl Fn(Run)) {
    for chunk_size in 0..=5 {
        for deferral in [false, true] {
            test(Run {
                chunk_size,
                deferral,
            });
        }
    }
}

impl Run {
    /// `XML_ParserCreate(NULL)` while `g_reparseDeferralEnabledDefault` is this run's
    /// (`basic_setup`, basic_tests.c:71-76).
    fn parser<'a>(self) -> P<'a> {
        Parser::with_deferral_default(self.deferral)
    }

    /// `_XML_Parse_SINGLE_BYTES` (common.c:194-219): `text` in chunks of `chunk_size` bytes,
    /// the last one with `is_final`.
    fn parse(self, parser: &mut P<'_>, mut text: &[u8], is_final: bool) -> XmlStatus {
        if self.chunk_size > 0 {
            // parse in chunks of `chunksize` bytes as long as not exhausting
            while text.len() > self.chunk_size {
                let res = parse(parser, &text[..self.chunk_size], false);
                if res != XmlStatus::Ok {
                    return res;
                }
                text = &text[self.chunk_size..];
            }
        }
        // parse the final chunk, the size of which will be <= chunksize
        parse(parser, text, is_final)
    }

    /// `_expect_failure` (common.c:221-231).
    fn expect_failure(self, parser: &mut P<'_>, text: &[u8], code: XmlError, message: &str) {
        if self.parse(parser, text, true) == XmlStatus::Ok {
            panic!("{self:?}: {message}");
        }
        if parser.error_code() != code {
            xml_failure(self, parser);
        }
    }

    /// `_run_character_check` (common.c:233-245).
    fn run_character_check(self, parser: &mut P<'_>, text: &[u8], expected: &[u8]) {
        let storage = CharData::new();
        parser.set_character_data_handler(Some(accumulate_characters(&storage)));
        if self.parse(parser, text, true) == XmlStatus::Error {
            xml_failure(self, parser);
        }
        storage.borrow().check(self, expected);
    }

    /// `_run_attribute_check` (common.c:247-259).
    fn run_attribute_check(self, parser: &mut P<'_>, text: &[u8], expected: &[u8]) {
        let storage = CharData::new();
        parser.set_start_element_handler(Some(accumulate_attribute(&storage)));
        if self.parse(parser, text, true) == XmlStatus::Error {
            xml_failure(self, parser);
        }
        storage.borrow().check(self, expected);
    }
}

/// `XML_Parse`, whose handlers never fail here.
fn parse(parser: &mut P<'_>, text: &[u8], is_final: bool) -> XmlStatus {
    match parser.parse(text, is_final) {
        Ok(status) => status,
        Err(ParseError::Handler(never)) => match never {},
    }
}

/// `XML_ParseBuffer`, whose handlers never fail here.
fn parse_buffer(parser: &mut P<'_>, len: i32, is_final: bool) -> XmlStatus {
    match parser.parse_buffer(len, is_final) {
        Ok(status) => status,
        Err(ParseError::Handler(never)) => match never {},
    }
}

/// `_xml_failure` (common.c:182-192): fails with the parser's error, line and column.
fn xml_failure(run: Run, parser: &mut P<'_>) -> ! {
    let err = parser.error_code();
    panic!(
        "{run:?}:     {}: {} (line {}, offset {})",
        err as i32,
        xml_error_string(err).unwrap_or("(null)"),
        parser.current_line_number(),
        parser.current_column_number()
    );
}

/// The parser's buffer from `at`, `len` bytes, for `XML_GetBuffer`'s callers.
fn buffer_at<'b>(parser: &'b mut P<'_>, at: usize, len: usize) -> &'b mut [u8] {
    &mut parser.buffer.as_mut().expect("a buffer")[at..at + len]
}

/// `CharData` (chardata.h:46-49, chardata.c): the characters handlers saw, up to 2048.
#[derive(Debug)]
struct CharData {
    count: i32,
    data: Vec<u8>,
}

impl CharData {
    /// `CharData_Init` (chardata.c:60-64).
    fn new() -> Rc<RefCell<CharData>> {
        Rc::new(RefCell::new(CharData {
            count: -1,
            data: Vec::new(),
        }))
    }

    /// `CharData_AppendXMLChars` (chardata.c:66-84).
    fn append(&mut self, s: &[u8]) {
        let maxchars = 2048i32;
        if self.count < 0 {
            self.count = 0;
        }
        let mut len = s.len() as i32;
        if len + self.count > maxchars {
            len = maxchars - self.count;
        }
        if len + self.count < 2048 {
            self.data.extend_from_slice(&s[..len as usize]);
            self.count += len;
        }
    }

    /// `CharData_CheckXMLChars` (chardata.c:86-106).
    fn check(&self, run: Run, expected: &[u8]) {
        let count = self.count.max(0) as usize;
        assert_eq!(
            count,
            expected.len(),
            "{run:?}: wrong number of data characters: got {count}, expected {}",
            expected.len()
        );
        assert_eq!(&self.data[..], expected, "{run:?}: got bad data bytes");
    }
}

/// `accumulate_characters` (handlers.c:1933-1937).
fn accumulate_characters(
    storage: &Rc<RefCell<CharData>>,
) -> CharacterDataHandler<'static, Infallible> {
    let storage = storage.clone();
    Box::new(move |s| {
        storage.borrow_mut().append(s);
        Ok(())
    })
}

/// `accumulate_attribute` (handlers.c:1939-1953): the value of the first attribute seen.
fn accumulate_attribute(
    storage: &Rc<RefCell<CharData>>,
) -> StartElementHandler<'static, Infallible> {
    let storage = storage.clone();
    Box::new(move |_name, mut atts| {
        let mut storage = storage.borrow_mut();
        while storage.count < 0 && !atts.is_empty() {
            // "accumulate" the value of the first attribute we see
            storage.append(atts[1]);
            atts = &atts[2..];
        }
        Ok(())
    })
}

/// `start_element_event_handler` and `record_element_start_handler` (handlers.c:73-78,
/// 1705-1710): the element's name.
fn record_start(storage: &Rc<RefCell<CharData>>) -> StartElementHandler<'static, Infallible> {
    let storage = storage.clone();
    Box::new(move |name, _atts| {
        storage.borrow_mut().append(name);
        Ok(())
    })
}

/// `end_element_event_handler` and `record_element_end_handler` (handlers.c:80-85,
/// 1712-1718): "/" and the element's name.
fn record_end(storage: &Rc<RefCell<CharData>>) -> EndElementHandler<'static, Infallible> {
    let storage = storage.clone();
    Box::new(move |name| {
        let mut storage = storage.borrow_mut();
        storage.append(b"/");
        storage.append(name);
        Ok(())
    })
}

/// `dummy_cdata_handler` (dummy.c:188-193): a character data handler that does nothing.
fn dummy_cdata_handler() -> CharacterDataHandler<'static, Infallible> {
    Box::new(|_| Ok(()))
}

// ---- Character & encoding tests ----

/// Port of expat 2.7.2 `START_TEST(test_nul_byte)` (tests/basic_tests.c:82-92).
#[test]
fn test_nul_byte() {
    each_run(|run| {
        let mut parser = run.parser();
        let text = b"<doc>\x00</doc>";

        // test that a NUL byte (in US-ASCII data) is an error
        if run.parse(&mut parser, text, true) == XmlStatus::Ok {
            panic!("{run:?}: Parser did not report error on NUL-byte.");
        }
        if parser.error_code() != XmlError::InvalidToken {
            xml_failure(run, &mut parser);
        }
    });
}

/// Port of expat 2.7.2 `START_TEST(test_u0000_char)` (tests/basic_tests.c:94-99).
#[test]
fn test_u0000_char() {
    each_run(|run| {
        // test that a NUL byte (in US-ASCII data) is an error
        run.expect_failure(
            &mut run.parser(),
            b"<doc>&#0;</doc>",
            XmlError::BadCharRef,
            "Parser did not report error on NUL-byte.",
        );
    });
}

/// Port of expat 2.7.2 `START_TEST(test_bom_utf8)` (tests/basic_tests.c:136-144).
#[test]
fn test_bom_utf8() {
    each_run(|run| {
        let mut parser = run.parser();
        // This test is really just making sure we don't core on a UTF-8 BOM.
        let text = b"\xef\xbb\xbf<e/>";

        if run.parse(&mut parser, text, true) == XmlStatus::Error {
            xml_failure(run, &mut parser);
        }
    });
}

/// Port of expat 2.7.2 `START_TEST(test_bom_utf16_be)` (tests/basic_tests.c:146-153).
#[test]
fn test_bom_utf16_be() {
    each_run(|run| {
        let mut parser = run.parser();
        let text = b"\xfe\xff\x00<\x00e\x00/\x00>";

        if run.parse(&mut parser, text, true) == XmlStatus::Error {
            xml_failure(run, &mut parser);
        }
    });
}

/// Port of expat 2.7.2 `START_TEST(test_bom_utf16_le)` (tests/basic_tests.c:155-162).
#[test]
fn test_bom_utf16_le() {
    each_run(|run| {
        let mut parser = run.parser();
        let text = b"\xff\xfe<\x00e\x00/\x00>\x00";

        if run.parse(&mut parser, text, true) == XmlStatus::Error {
            xml_failure(run, &mut parser);
        }
    });
}

/// Port of expat 2.7.2 `START_TEST(test_nobom_utf16_le)` (tests/basic_tests.c:164-177).
#[test]
fn test_nobom_utf16_le() {
    each_run(|run| {
        let mut parser = run.parser();
        let text = b" \x00<\x00e\x00/\x00>\x00";

        if run.chunk_size == 1 {
            // TODO: with just the first byte, we can't tell the difference between
            // UTF-16-LE and UTF-8. Avoid the failure for now.
            return;
        }

        if run.parse(&mut parser, text, true) == XmlStatus::Error {
            xml_failure(run, &mut parser);
        }
    });
}

/// Port of expat 2.7.2 `START_TEST(test_danish_latin1)` (tests/basic_tests.c:208-220).
#[test]
fn test_danish_latin1() {
    each_run(|run| {
        let text =
            b"<?xml version='1.0' encoding='iso-8859-1'?>\n<e>J\xf8rgen \xe6\xf8\xe5\xc6\xd8\xc5</e>";
        let expected = b"J\xc3\xb8rgen \xc3\xa6\xc3\xb8\xc3\xa5\xc3\x86\xc3\x98\xc3\x85";
        run.run_character_check(&mut run.parser(), text, expected);
    });
}

/// Port of expat 2.7.2 `START_TEST(test_french_charref_hexidecimal)`
/// (tests/basic_tests.c:223-234).
#[test]
fn test_french_charref_hexidecimal() {
    each_run(|run| {
        let text = b"<?xml version='1.0' encoding='iso-8859-1'?>\n\
                     <doc>&#xE9;&#xE8;&#xE0;&#xE7;&#xEA;&#xC8;</doc>";
        let expected = b"\xc3\xa9\xc3\xa8\xc3\xa0\xc3\xa7\xc3\xaa\xc3\x88";
        run.run_character_check(&mut run.parser(), text, expected);
    });
}

/// Port of expat 2.7.2 `START_TEST(test_french_charref_decimal)`
/// (tests/basic_tests.c:236-247).
#[test]
fn test_french_charref_decimal() {
    each_run(|run| {
        let text = b"<?xml version='1.0' encoding='iso-8859-1'?>\n\
                     <doc>&#233;&#232;&#224;&#231;&#234;&#200;</doc>";
        let expected = b"\xc3\xa9\xc3\xa8\xc3\xa0\xc3\xa7\xc3\xaa\xc3\x88";
        run.run_character_check(&mut run.parser(), text, expected);
    });
}

/// Port of expat 2.7.2 `START_TEST(test_french_latin1)` (tests/basic_tests.c:249-260).
#[test]
fn test_french_latin1() {
    each_run(|run| {
        let text =
            b"<?xml version='1.0' encoding='iso-8859-1'?>\n<doc>\xe9\xe8\xe0\xe7\xea\xc8</doc>";
        let expected = b"\xc3\xa9\xc3\xa8\xc3\xa0\xc3\xa7\xc3\xaa\xc3\x88";
        run.run_character_check(&mut run.parser(), text, expected);
    });
}

/// Port of expat 2.7.2 `START_TEST(test_french_utf8)` (tests/basic_tests.c:262-272).
#[test]
fn test_french_utf8() {
    each_run(|run| {
        let text = b"<?xml version='1.0' encoding='utf-8'?>\n<doc>\xc3\xa9</doc>";
        let expected = b"\xc3\xa9";
        run.run_character_check(&mut run.parser(), text, expected);
    });
}

/// Port of expat 2.7.2 `START_TEST(test_utf8_false_rejection)` (tests/basic_tests.c:279-288).
#[test]
fn test_utf8_false_rejection() {
    each_run(|run| {
        let text = b"<doc>\xef\xba\xbf</doc>";
        let expected = b"\xef\xba\xbf";
        run.run_character_check(&mut run.parser(), text, expected);
    });
}

/// Port of expat 2.7.2 `START_TEST(test_illegal_utf8)` (tests/basic_tests.c:295-313).
#[test]
fn test_illegal_utf8() {
    each_run(|run| {
        let mut parser = run.parser();
        for i in 128..=255u8 {
            let text = [b"<e>".as_slice(), &[i], b"cd</e>"].concat();
            if run.parse(&mut parser, &text, true) == XmlStatus::Ok {
                panic!("{run:?}: expected token error for (ordinal {i}) in UTF-8 text");
            } else if parser.error_code() != XmlError::InvalidToken {
                xml_failure(run, &mut parser);
            }
            // Reset the parser since we use the same parser repeatedly.
            parser.reset();
        }
    });
}

/// Port of expat 2.7.2 `START_TEST(test_utf16)` (tests/basic_tests.c:376-405).
#[test]
fn test_utf16() {
    each_run(|run| {
        let mut parser = run.parser();
        // <?xml version="1.0" encoding="UTF-16"?>
        //  <doc a='123'>some {A} text</doc>
        //
        // where {A} is U+FF21, FULLWIDTH LATIN CAPITAL LETTER A
        let text = b"\x00<\x00?\x00x\x00m\x00l\x00 \x00v\x00e\x00r\x00s\x00i\x00o\x00n\x00=\x00'\x001\x00.\x000\x00'\x00 \x00e\x00n\x00c\x00o\x00d\x00i\x00n\x00g\x00=\x00'\x00U\x00T\x00F\x00-\x001\x006\x00'\x00?\x00>\x00\n\x00<\x00d\x00o\x00c\x00 \x00a\x00=\x00'\x001\x002\x003\x00'\x00>\x00s\x00o\x00m\x00e\x00 \xff!\x00 \x00t\x00e\x00x\x00t\x00<\x00/\x00d\x00o\x00c\x00>";
        let expected = b"some \xef\xbc\xa1 text";
        let storage = CharData::new();
        parser.set_character_data_handler(Some(accumulate_characters(&storage)));
        if run.parse(&mut parser, text, true) == XmlStatus::Error {
            xml_failure(run, &mut parser);
        }
        storage.borrow().check(run, expected);
    });
}

/// Port of expat 2.7.2 `START_TEST(test_utf16_le_epilog_newline)` (tests/basic_tests.c:407-427).
#[test]
fn test_utf16_le_epilog_newline() {
    each_run(|run| {
        let mut parser = run.parser();
        let first_chunk_bytes = 17;
        let text = b"\xff\xfe<\x00e\x00/\x00>\x00\r\x00\n\x00\r\x00\n\x00";

        assert!(
            first_chunk_bytes < text.len(),
            "bad value of first_chunk_bytes"
        );
        if run.parse(&mut parser, &text[..first_chunk_bytes], false) == XmlStatus::Error {
            xml_failure(run, &mut parser);
        } else {
            let rc = run.parse(&mut parser, &text[first_chunk_bytes..], true);
            if rc == XmlStatus::Error {
                xml_failure(run, &mut parser);
            }
        }
    });
}

/// Port of expat 2.7.2 `START_TEST(test_long_utf8_character)` (tests/basic_tests.c:482-490).
#[test]
fn test_long_utf8_character() {
    each_run(|run| {
        let text = b"<?xml version='1.0' encoding='utf-8'?>\n\
                     <do\xf0\x90\x80\x80/>";
        run.expect_failure(
            &mut run.parser(),
            text,
            XmlError::InvalidToken,
            "4-byte UTF-8 character in element name not faulted",
        );
    });
}

/// "ABCDEFGHIJKLMNOP" four times: a line of the long attribute tests.
const A_TO_P_64: &[u8] = b"ABCDEFGHIJKLMNOPABCDEFGHIJKLMNOPABCDEFGHIJKLMNOPABCDEFGHIJKLMNOP";

/// Port of expat 2.7.2 `START_TEST(test_long_latin1_attribute)` (tests/basic_tests.c:495-547).
#[test]
fn test_long_latin1_attribute() {
    each_run(|run| {
        let mut text = b"<?xml version='1.0' encoding='iso-8859-1'?>\n<doc att='".to_vec();
        // 64 characters per line
        for _ in 0..15 {
            text.extend_from_slice(A_TO_P_64);
        }
        text.extend_from_slice(&A_TO_P_64[..63]);
        // Last character splits across a buffer boundary
        text.extend_from_slice(b"\xe4'>\n</doc>");

        let mut expected = Vec::new();
        for _ in 0..15 {
            expected.extend_from_slice(A_TO_P_64);
        }
        expected.extend_from_slice(&A_TO_P_64[..63]);
        expected.extend_from_slice(b"\xc3\xa4");

        run.run_attribute_check(&mut run.parser(), &text, &expected);
    });
}

/// Port of expat 2.7.2 `START_TEST(test_long_ascii_attribute)` (tests/basic_tests.c:552-598).
#[test]
fn test_long_ascii_attribute() {
    each_run(|run| {
        let mut text = b"<?xml version='1.0' encoding='us-ascii'?>\n<doc att='".to_vec();
        // 64 characters per line
        for _ in 0..16 {
            text.extend_from_slice(A_TO_P_64);
        }
        text.extend_from_slice(b"01234'>\n</doc>");
        let mut expected = Vec::new();
        for _ in 0..16 {
            expected.extend_from_slice(A_TO_P_64);
        }
        expected.extend_from_slice(b"01234");

        run.run_attribute_check(&mut run.parser(), &text, &expected);
    });
}

/// Port of expat 2.7.2 `START_TEST(test_line_number_after_parse)` (tests/basic_tests.c:601-618).
#[test]
fn test_line_number_after_parse() {
    each_run(|run| {
        let mut parser = run.parser();
        let text = b"<tag>\n\n\n</tag>";

        if run.parse(&mut parser, text, true) == XmlStatus::Error {
            xml_failure(run, &mut parser);
        }
        let lineno = parser.current_line_number();
        assert_eq!(lineno, 4, "{run:?}: expected 4 lines, saw {lineno}");
    });
}

/// Port of expat 2.7.2 `START_TEST(test_column_number_after_parse)`
/// (tests/basic_tests.c:621-636).
#[test]
fn test_column_number_after_parse() {
    each_run(|run| {
        let mut parser = run.parser();
        let text = b"<tag></tag>";

        if run.parse(&mut parser, text, true) == XmlStatus::Error {
            xml_failure(run, &mut parser);
        }
        let colno = parser.current_column_number();
        assert_eq!(colno, 11, "{run:?}: expected 11 columns, saw {colno}");
    });
}

/// Port of expat 2.7.2 `START_TEST(test_line_number_after_error)` (tests/basic_tests.c:671-688).
#[test]
fn test_line_number_after_error() {
    each_run(|run| {
        let mut parser = run.parser();
        let text = b"<a>\n  <b>\n  </a>"; // missing </b>
        if run.parse(&mut parser, text, true) != XmlStatus::Error {
            panic!("{run:?}: Expected a parse error");
        }

        let lineno = parser.current_line_number();
        assert_eq!(lineno, 3, "{run:?}: expected 3 lines, saw {lineno}");
    });
}

/// Port of expat 2.7.2 `START_TEST(test_column_number_after_error)`
/// (tests/basic_tests.c:691-708).
#[test]
fn test_column_number_after_error() {
    each_run(|run| {
        let mut parser = run.parser();
        let text = b"<a>\n  <b>\n  </a>"; // missing </b>
        if run.parse(&mut parser, text, true) != XmlStatus::Error {
            panic!("{run:?}: Expected a parse error");
        }

        let colno = parser.current_column_number();
        assert_eq!(colno, 4, "{run:?}: expected 4 columns, saw {colno}");
    });
}

/// "ABC...xyz0123456789-+": 64 characters, a line of the long line tests.
const LONG_LINE_64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-+";

/// Port of expat 2.7.2 `START_TEST(test_really_long_lines)` (tests/basic_tests.c:711-743).
#[test]
fn test_really_long_lines() {
    each_run(|run| {
        let mut parser = run.parser();
        // This parses an input line longer than INIT_DATA_BUF_SIZE characters long (defined
        // to be 1024 in xmlparse.c).
        let mut text = b"<e>".to_vec();
        // until we have at least 1024 characters on the line:
        for _ in 0..17 {
            text.extend_from_slice(LONG_LINE_64);
        }
        text.extend_from_slice(b"</e>");
        if run.parse(&mut parser, &text, true) == XmlStatus::Error {
            xml_failure(run, &mut parser);
        }
    });
}

/// Port of expat 2.7.2 `START_TEST(test_really_long_encoded_lines)`
/// (tests/basic_tests.c:746-787).
#[test]
fn test_really_long_encoded_lines() {
    each_run(|run| {
        let mut parser = run.parser();
        // As above, except that we want to provoke an output buffer overflow with a
        // non-trivial encoding.  For this we need to pass the whole cdata in one go, not
        // byte-by-byte.
        let mut text = b"<?xml version='1.0' encoding='iso-8859-1'?><e>".to_vec();
        for _ in 0..17 {
            text.extend_from_slice(LONG_LINE_64);
        }
        text.extend_from_slice(b"</e>");
        let parse_len = text.len();

        // Need a cdata handler to provoke the code path we want to test
        parser.set_character_data_handler(Some(dummy_cdata_handler()));
        let at = parser
            .get_buffer(parse_len as i32)
            .unwrap_or_else(|| panic!("{run:?}: Could not allocate parse buffer"));
        buffer_at(&mut parser, at, parse_len).copy_from_slice(&text);
        if parse_buffer(&mut parser, parse_len as i32, true) == XmlStatus::Error {
            xml_failure(run, &mut parser);
        }
    });
}

// ---- Element event tests ----

/// Port of expat 2.7.2 `START_TEST(test_end_element_events)` (tests/basic_tests.c:793-806).
#[test]
fn test_end_element_events() {
    each_run(|run| {
        let mut parser = run.parser();
        let text = b"<a><b><c/></b><d><f/></d></a>";
        let expected = b"/c/b/f/d/a";
        let storage = CharData::new();

        parser.set_end_element_handler(Some(record_end(&storage)));
        if run.parse(&mut parser, text, true) == XmlStatus::Error {
            xml_failure(run, &mut parser);
        }
        storage.borrow().check(run, expected);
    });
}

// ---- Attribute tests ----

/// Port of `is_whitespace_normalized` (tests/basic_tests.c:821-845): whether the attribute
/// value `s` is normalized as its type (CDATA or not) asks.
fn is_whitespace_normalized(s: &[u8], is_cdata: bool) -> bool {
    let mut blanks = 0;
    let mut at_start = true;
    for &c in s {
        if c == b' ' {
            blanks += 1;
        } else if c == b'\t' || c == b'\n' || c == b'\r' {
            return false;
        } else {
            if at_start {
                at_start = false;
                if blanks != 0 && !is_cdata {
                    // illegal leading blanks
                    return false;
                }
            } else if blanks > 1 && !is_cdata {
                return false;
            }
            blanks = 0;
        }
    }
    !(blanks != 0 && !is_cdata)
}

/// Port of expat 2.7.2 `START_TEST(test_helper_is_whitespace_normalized)`
/// (tests/basic_tests.c:848-869).
#[test]
fn test_helper_is_whitespace_normalized() {
    assert!(is_whitespace_normalized(b"abc", false));
    assert!(is_whitespace_normalized(b"abc", true));
    assert!(is_whitespace_normalized(b"abc def ghi", false));
    assert!(is_whitespace_normalized(b"abc def ghi", true));
    assert!(!is_whitespace_normalized(b" abc def ghi", false));
    assert!(is_whitespace_normalized(b" abc def ghi", true));
    assert!(!is_whitespace_normalized(b"abc  def ghi", false));
    assert!(is_whitespace_normalized(b"abc  def ghi", true));
    assert!(!is_whitespace_normalized(b"abc def ghi ", false));
    assert!(is_whitespace_normalized(b"abc def ghi ", true));
    assert!(!is_whitespace_normalized(b" ", false));
    assert!(is_whitespace_normalized(b" ", true));
    assert!(!is_whitespace_normalized(b"\t", false));
    assert!(!is_whitespace_normalized(b"\t", true));
    assert!(!is_whitespace_normalized(b"\n", false));
    assert!(!is_whitespace_normalized(b"\n", true));
    assert!(!is_whitespace_normalized(b"\r", false));
    assert!(!is_whitespace_normalized(b"\r", true));
    assert!(!is_whitespace_normalized(b"abc\t def", true));
}

/// Port of expat 2.7.2 `START_TEST(test_attr_whitespace_normalization)`
/// (tests/basic_tests.c:895-916), with `check_attr_contains_normalized_whitespace`
/// (871-893).
#[test]
fn test_attr_whitespace_normalization() {
    each_run(|run| {
        let mut parser = run.parser();
        let text = b"<!DOCTYPE doc [\n  <!ATTLIST doc\n            attr NMTOKENS #REQUIRED\n            ents ENTITIES #REQUIRED\n            refs IDREFS   #REQUIRED>\n]>\n<doc attr='    a  b c\t\td\te\t' refs=' id-1   \t  id-2\t\t'  \n     ents=' ent-1   \t\r\n            ent-2  ' >\n  <e id='id-1'/>\n  <e id='id-2'/>\n</doc>";

        parser.set_start_element_handler(Some(Box::new(move |_name, atts| {
            for pair in atts.chunks(2) {
                let (attrname, value) = (pair[0], pair[1]);
                if (attrname == b"attr" || attrname == b"ents" || attrname == b"refs")
                    && !is_whitespace_normalized(value, false)
                {
                    panic!(
                        "{run:?}: attribute value not normalized: {}='{}'",
                        String::from_utf8_lossy(attrname),
                        String::from_utf8_lossy(value)
                    );
                }
            }
            Ok(())
        })));
        if run.parse(&mut parser, text, true) == XmlStatus::Error {
            xml_failure(run, &mut parser);
        }
    });
}

// ---- XML declaration tests ----

/// Port of expat 2.7.2 `START_TEST(test_xmldecl_misplaced)` (tests/basic_tests.c:922-929).
#[test]
fn test_xmldecl_misplaced() {
    each_run(|run| {
        run.expect_failure(
            &mut run.parser(),
            b"\n<?xml version='1.0'?>\n<a/>",
            XmlError::MisplacedXmlPi,
            "failed to report misplaced XML declaration",
        );
    });
}

/// Port of expat 2.7.2 `START_TEST(test_xmldecl_invalid)` (tests/basic_tests.c:931-935).
#[test]
fn test_xmldecl_invalid() {
    each_run(|run| {
        run.expect_failure(
            &mut run.parser(),
            b"<?xml version='1.0' \xc3\xa7?>\n<doc/>",
            XmlError::XmlDecl,
            "Failed to report invalid XML declaration",
        );
    });
}

/// Port of expat 2.7.2 `START_TEST(test_xmldecl_missing_attr)` (tests/basic_tests.c:937-941).
#[test]
fn test_xmldecl_missing_attr() {
    each_run(|run| {
        run.expect_failure(
            &mut run.parser(),
            b"<?xml ='1.0'?>\n<doc/>\n",
            XmlError::XmlDecl,
            "Failed to report missing XML declaration attribute",
        );
    });
}

/// Port of expat 2.7.2 `START_TEST(test_xmldecl_missing_value)` (tests/basic_tests.c:943-949).
#[test]
fn test_xmldecl_missing_value() {
    each_run(|run| {
        run.expect_failure(
            &mut run.parser(),
            b"<?xml version='1.0' encoding='us-ascii' standalone?>\n<doc/>",
            XmlError::XmlDecl,
            "Failed to report missing attribute value",
        );
    });
}

// ---- Entity tests ----

/// Port of expat 2.7.2 `START_TEST(test_wfc_undeclared_entity_unread_external_subset)`
/// (tests/basic_tests.c:1069-1077).
#[test]
fn test_wfc_undeclared_entity_unread_external_subset() {
    each_run(|run| {
        let mut parser = run.parser();
        let text = b"<!DOCTYPE doc SYSTEM 'foo'>\n<doc>&entity;</doc>";

        if run.parse(&mut parser, text, true) == XmlStatus::Error {
            xml_failure(run, &mut parser);
        }
    });
}

/// Port of expat 2.7.2 `START_TEST(test_wfc_undeclared_entity_no_external_subset)`
/// (tests/basic_tests.c:1082-1086).
#[test]
fn test_wfc_undeclared_entity_no_external_subset() {
    each_run(|run| {
        run.expect_failure(
            &mut run.parser(),
            b"<doc>&entity;</doc>",
            XmlError::UndefinedEntity,
            "Parser did not report undefined entity w/out a DTD.",
        );
    });
}

/// Port of expat 2.7.2 `START_TEST(test_wfc_undeclared_entity_standalone)`
/// (tests/basic_tests.c:1091-1100).
#[test]
fn test_wfc_undeclared_entity_standalone() {
    each_run(|run| {
        let text = b"<?xml version='1.0' encoding='us-ascii' standalone='yes'?>\n<!DOCTYPE doc SYSTEM 'foo'>\n<doc>&entity;</doc>";

        run.expect_failure(
            &mut run.parser(),
            text,
            XmlError::UndefinedEntity,
            "Parser did not report undefined entity (standalone).",
        );
    });
}

/// Port of expat 2.7.2 `START_TEST(test_entity_start_tag_level_greater_than_one)`
/// (tests/basic_tests.c:1195-1209).
#[test]
fn test_entity_start_tag_level_greater_than_one() {
    each_run(|run| {
        let text = b"<!DOCTYPE t1 [\n  <!ENTITY e1 'hello'>\n]>\n<t1>\n  <t2>&e1;</t2>\n</t1>\n";

        let mut parser = run.parser();
        assert_eq!(run.parse(&mut parser, text, true), XmlStatus::Ok, "{run:?}");
    });
}

/// Port of expat 2.7.2 `START_TEST(test_wfc_no_recursive_entity_refs)`
/// (tests/basic_tests.c:1211-1220).
#[test]
fn test_wfc_no_recursive_entity_refs() {
    each_run(|run| {
        let text = b"<!DOCTYPE doc [\n  <!ENTITY entity '&#38;entity;'>\n]>\n<doc>&entity;</doc>";

        run.expect_failure(
            &mut run.parser(),
            text,
            XmlError::RecursiveEntityRef,
            "Parser did not report recursive entity reference.",
        );
    });
}

/// Port of expat 2.7.2 `START_TEST(test_no_indirectly_recursive_entity_refs)`
/// (tests/basic_tests.c:1222-1307), the cases without parameter entities.
#[test]
fn test_no_indirectly_recursive_entity_refs() {
    each_run(|run| {
        let cases: [&[u8]; 2] = [
            // general entity + character data
            b"<!DOCTYPE a [\n  <!ENTITY e1 '&e2;'>\n  <!ENTITY e2 '&e1;'>\n]><a>&e2;</a>\n",
            // general entity + attribute value
            b"<!DOCTYPE a [\n  <!ENTITY e1 '&e2;'>\n  <!ENTITY e2 '&e1;'>\n]><a k1='&e2;' />\n",
        ];
        for (i, doc) in cases.iter().enumerate() {
            for reset_wanted in [true, false] {
                let mut parser = run.parser();
                let status = run.parse(&mut parser, doc, true);
                // XML_DTD: both GE and DTD
                assert_eq!(
                    status,
                    XmlStatus::Error,
                    "{run:?} [{i},reset={reset_wanted}]"
                );
                assert_eq!(
                    parser.error_code(),
                    XmlError::RecursiveEntityRef,
                    "{run:?} [{i}]"
                );
                if reset_wanted {
                    parser.reset();
                }
            }
        }
    });
}

/// Port of expat 2.7.2 `START_TEST(test_empty_ns_without_namespaces)`
/// (tests/basic_tests.c:1462-1471).
#[test]
fn test_empty_ns_without_namespaces() {
    each_run(|run| {
        let mut parser = run.parser();
        let text = b"<doc xmlns:prefix='http://example.org/'>\n  <e xmlns:prefix=''/>\n</doc>";

        if run.parse(&mut parser, text, true) == XmlStatus::Error {
            xml_failure(run, &mut parser);
        }
    });
}

/// Port of expat 2.7.2 `START_TEST(test_ns_in_attribute_default_without_namespaces)`
/// (tests/basic_tests.c:1477-1488).
#[test]
fn test_ns_in_attribute_default_without_namespaces() {
    each_run(|run| {
        let mut parser = run.parser();
        let text = b"<!DOCTYPE e:element [\n  <!ATTLIST e:element\n    xmlns:e CDATA 'http://example.org/'>\n      ]>\n<e:element/>";

        if run.parse(&mut parser, text, true) == XmlStatus::Error {
            xml_failure(run, &mut parser);
        }
    });
}

// ---- CDATA tests ----

/// `test_good_cdata_utf16` and `test_good_cdata_utf16_le`'s check.
fn check_good_cdata(run: Run, text: &[u8]) {
    let mut parser = run.parser();
    let expected = b"hello";
    let storage = CharData::new();
    parser.set_character_data_handler(Some(accumulate_characters(&storage)));

    if run.parse(&mut parser, text, true) == XmlStatus::Error {
        xml_failure(run, &mut parser);
    }
    storage.borrow().check(run, expected);
}

/// Port of expat 2.7.2 `START_TEST(test_good_cdata_utf16)` (tests/basic_tests.c:1602-1627).
#[test]
fn test_good_cdata_utf16() {
    each_run(|run| {
        // <?xml version='1.0' encoding='utf-16'?>
        // <a><![CDATA[hello]]></a>
        check_good_cdata(
            run,
            b"\x00<\x00?\x00x\x00m\x00l\x00 \x00v\x00e\x00r\x00s\x00i\x00o\x00n\x00=\x00'\x001\x00.\x000\x00'\x00 \x00e\x00n\x00c\x00o\x00d\x00i\x00n\x00g\x00=\x00'\x00u\x00t\x00f\x00-\x001\x006\x00'\x00?\x00>\x00\n\x00<\x00a\x00>\x00<\x00!\x00[\x00C\x00D\x00A\x00T\x00A\x00[\x00h\x00e\x00l\x00l\x00o\x00]\x00]\x00>\x00<\x00/\x00a\x00>",
        );
    });
}

/// Port of expat 2.7.2 `START_TEST(test_good_cdata_utf16_le)` (tests/basic_tests.c:1629-1654).
#[test]
fn test_good_cdata_utf16_le() {
    each_run(|run| {
        // <?xml version='1.0' encoding='utf-16'?>
        // <a><![CDATA[hello]]></a>
        check_good_cdata(
            run,
            b"<\x00?\x00x\x00m\x00l\x00 \x00v\x00e\x00r\x00s\x00i\x00o\x00n\x00=\x00'\x001\x00.\x000\x00'\x00 \x00e\x00n\x00c\x00o\x00d\x00i\x00n\x00g\x00=\x00'\x00u\x00t\x00f\x00-\x001\x006\x00'\x00?\x00>\x00\n\x00<\x00a\x00>\x00<\x00!\x00[\x00C\x00D\x00A\x00T\x00A\x00[\x00h\x00e\x00l\x00l\x00o\x00]\x00]\x00>\x00<\x00/\x00a\x00>\x00",
        );
    });
}

/// Port of expat 2.7.2 `START_TEST(test_long_cdata_utf16)` (tests/basic_tests.c:1661-1729).
#[test]
fn test_long_cdata_utf16() {
    each_run(|run| {
        let mut parser = run.parser();
        // <?xlm version='1.0' encoding='utf-16'?>
        // <a><![CDATA[
        // ABCDEFGHIJKLMNOP
        // ]]></a>
        // `A_TO_P_IN_UTF16`: 16 characters
        let a_to_p_in_utf16 =
            b"\x00A\x00B\x00C\x00D\x00E\x00F\x00G\x00H\x00I\x00J\x00K\x00L\x00M\x00N\x00O\x00P";
        let mut text = b"\x00<\x00?\x00x\x00m\x00l\x00 \x00v\x00e\x00r\x00s\x00i\x00o\x00n\x00=\x00'\x001\x00.\x000\x00'\x00 \x00e\x00n\x00c\x00o\x00d\x00i\x00n\x00g\x00=\x00'\x00u\x00t\x00f\x00-\x001\x006\x00'\x00?\x00>\x00<\x00a\x00>\x00<\x00!\x00[\x00C\x00D\x00A\x00T\x00A\x00[".to_vec();
        // 64 characters per line
        for _ in 0..65 {
            text.extend_from_slice(a_to_p_in_utf16);
        }
        text.extend_from_slice(b"\x00]\x00]\x00>\x00<\x00/\x00a\x00>");
        let mut expected = Vec::new();
        for _ in 0..16 {
            expected.extend_from_slice(A_TO_P_64);
        }
        expected.extend_from_slice(b"ABCDEFGHIJKLMNOP");
        let storage = CharData::new();

        parser.set_character_data_handler(Some(accumulate_characters(&storage)));
        let at = parser
            .get_buffer(text.len() as i32)
            .unwrap_or_else(|| panic!("{run:?}: Could not allocate parse buffer"));
        buffer_at(&mut parser, at, text.len()).copy_from_slice(&text);
        if parse_buffer(&mut parser, text.len() as i32, true) == XmlStatus::Error {
            xml_failure(run, &mut parser);
        }
        storage.borrow().check(run, &expected);
    });
}

/// Port of expat 2.7.2 `START_TEST(test_multichar_cdata_utf16)` (tests/basic_tests.c:1732-1769).
#[test]
fn test_multichar_cdata_utf16() {
    each_run(|run| {
        let mut parser = run.parser();
        // <?xml version='1.0' encoding='utf-16'?>
        // <a><![CDATA[{MINIM}{CROTCHET}]]></a>
        //
        // where {MINIM} is U+1d15e (a minim or half-note)
        //   UTF-16: 0xd834 0xdd5e
        //   UTF-8:  0xf0 0x9d 0x85 0x9e
        // and {CROTCHET} is U+1d15f (a crotchet or quarter-note)
        //   UTF-16: 0xd834 0xdd5f
        //   UTF-8:  0xf0 0x9d 0x85 0x9f
        let text = b"\x00<\x00?\x00x\x00m\x00l\x00 \x00v\x00e\x00r\x00s\x00i\x00o\x00n\x00=\x00'\x001\x00.\x000\x00'\x00 \x00e\x00n\x00c\x00o\x00d\x00i\x00n\x00g\x00=\x00'\x00u\x00t\x00f\x00-\x001\x006\x00'\x00?\x00>\x00\n\x00<\x00a\x00>\x00<\x00!\x00[\x00C\x00D\x00A\x00T\x00A\x00[\xd84\xdd^\xd84\xdd_\x00]\x00]\x00>\x00<\x00/\x00a\x00>";
        let expected = b"\xf0\x9d\x85\x9e\xf0\x9d\x85\x9f";
        let storage = CharData::new();

        parser.set_character_data_handler(Some(accumulate_characters(&storage)));
        if run.parse(&mut parser, text, true) == XmlStatus::Error {
            xml_failure(run, &mut parser);
        }
        storage.borrow().check(run, expected);
    });
}

/// Port of expat 2.7.2 `START_TEST(test_utf16_bad_surrogate_pair)`
/// (tests/basic_tests.c:1772-1797).
#[test]
fn test_utf16_bad_surrogate_pair() {
    each_run(|run| {
        let mut parser = run.parser();
        // <?xml version='1.0' encoding='utf-16'?>
        // <a><![CDATA[{BADLINB}]]></a>
        //
        // where {BADLINB} is U+10000 (the first Linear B character) with the UTF-16
        // surrogate pair in the wrong order, i.e. 0xdc00 0xd800
        let text = b"\x00<\x00?\x00x\x00m\x00l\x00 \x00v\x00e\x00r\x00s\x00i\x00o\x00n\x00=\x00'\x001\x00.\x000\x00'\x00 \x00e\x00n\x00c\x00o\x00d\x00i\x00n\x00g\x00=\x00'\x00u\x00t\x00f\x00-\x001\x006\x00'\x00?\x00>\x00\n\x00<\x00a\x00>\x00<\x00!\x00[\x00C\x00D\x00A\x00T\x00A\x00[\xdc\x00\xd8\x00\x00]\x00]\x00>\x00<\x00/\x00a\x00>";

        if run.parse(&mut parser, text, true) != XmlStatus::Error {
            panic!("{run:?}: Reversed UTF-16 surrogate pair not faulted");
        }
        if parser.error_code() != XmlError::InvalidToken {
            xml_failure(run, &mut parser);
        }
    });
}

/// Port of expat 2.7.2 `START_TEST(test_bad_cdata)` (tests/basic_tests.c:1799-1852).
#[test]
fn test_bad_cdata() {
    each_run(|run| {
        let mut parser = run.parser();
        let cases: [(&[u8], XmlError); 21] = [
            (b"<a><", XmlError::UnclosedToken),
            (b"<a><!", XmlError::UnclosedToken),
            (b"<a><![", XmlError::UnclosedToken),
            (b"<a><![C", XmlError::UnclosedToken),
            (b"<a><![CD", XmlError::UnclosedToken),
            (b"<a><![CDA", XmlError::UnclosedToken),
            (b"<a><![CDAT", XmlError::UnclosedToken),
            (b"<a><![CDATA", XmlError::UnclosedToken),
            (b"<a><![CDATA[", XmlError::UnclosedCdataSection),
            (b"<a><![CDATA[]", XmlError::UnclosedCdataSection),
            (b"<a><![CDATA[]]", XmlError::UnclosedCdataSection),
            (b"<a><!<a/>", XmlError::InvalidToken),
            (b"<a><![<a/>", XmlError::UnclosedToken),  // ?!
            (b"<a><![C<a/>", XmlError::UnclosedToken), // ?!
            (b"<a><![CD<a/>", XmlError::InvalidToken),
            (b"<a><![CDA<a/>", XmlError::InvalidToken),
            (b"<a><![CDAT<a/>", XmlError::InvalidToken),
            (b"<a><![CDATA<a/>", XmlError::InvalidToken),
            (b"<a><![CDATA[<a/>", XmlError::UnclosedCdataSection),
            (b"<a><![CDATA[]<a/>", XmlError::UnclosedCdataSection),
            (b"<a><![CDATA[]]<a/>", XmlError::UnclosedCdataSection),
        ];

        for (i, (text, expected_error)) in cases.iter().enumerate() {
            let actual_status = run.parse(&mut parser, text, true);
            let actual_error = parser.error_code();

            assert_eq!(actual_status, XmlStatus::Error, "{run:?} case {}", i + 1);
            assert_eq!(
                actual_error,
                *expected_error,
                "{run:?}: Expected error {:?} but got error {:?} for case {}: \"{}\"",
                expected_error,
                actual_error,
                i + 1,
                String::from_utf8_lossy(text)
            );

            parser.reset();
        }
    });
}

/// Port of expat 2.7.2 `START_TEST(test_bad_cdata_utf16)` (tests/basic_tests.c:1855-1927).
#[test]
fn test_bad_cdata_utf16() {
    each_run(|run| {
        let mut parser = run.parser();
        let prolog = b"\x00<\x00?\x00x\x00m\x00l\x00 \x00v\x00e\x00r\x00s\x00i\x00o\x00n\x00=\x00'\x001\x00.\x000\x00'\x00 \x00e\x00n\x00c\x00o\x00d\x00i\x00n\x00g\x00=\x00'\x00u\x00t\x00f\x00-\x001\x006\x00'\x00?\x00>\x00\n\x00<\x00a\x00>";
        let cases: [(usize, &[u8], XmlError); 24] = [
            (1, b"\x00", XmlError::UnclosedToken),
            (2, b"\x00<", XmlError::UnclosedToken),
            (3, b"\x00<\x00", XmlError::UnclosedToken),
            (4, b"\x00<\x00!", XmlError::UnclosedToken),
            (5, b"\x00<\x00!\x00", XmlError::UnclosedToken),
            (6, b"\x00<\x00!\x00[", XmlError::UnclosedToken),
            (7, b"\x00<\x00!\x00[\x00", XmlError::UnclosedToken),
            (8, b"\x00<\x00!\x00[\x00C", XmlError::UnclosedToken),
            (9, b"\x00<\x00!\x00[\x00C\x00", XmlError::UnclosedToken),
            (10, b"\x00<\x00!\x00[\x00C\x00D", XmlError::UnclosedToken),
            (
                11,
                b"\x00<\x00!\x00[\x00C\x00D\x00",
                XmlError::UnclosedToken,
            ),
            (
                12,
                b"\x00<\x00!\x00[\x00C\x00D\x00A",
                XmlError::UnclosedToken,
            ),
            (
                13,
                b"\x00<\x00!\x00[\x00C\x00D\x00A\x00",
                XmlError::UnclosedToken,
            ),
            (
                14,
                b"\x00<\x00!\x00[\x00C\x00D\x00A\x00T",
                XmlError::UnclosedToken,
            ),
            (
                15,
                b"\x00<\x00!\x00[\x00C\x00D\x00A\x00T\x00",
                XmlError::UnclosedToken,
            ),
            (
                16,
                b"\x00<\x00!\x00[\x00C\x00D\x00A\x00T\x00A",
                XmlError::UnclosedToken,
            ),
            (
                17,
                b"\x00<\x00!\x00[\x00C\x00D\x00A\x00T\x00A\x00",
                XmlError::UnclosedToken,
            ),
            (
                18,
                b"\x00<\x00!\x00[\x00C\x00D\x00A\x00T\x00A\x00[",
                XmlError::UnclosedCdataSection,
            ),
            (
                19,
                b"\x00<\x00!\x00[\x00C\x00D\x00A\x00T\x00A\x00[\x00",
                XmlError::UnclosedCdataSection,
            ),
            (
                20,
                b"\x00<\x00!\x00[\x00C\x00D\x00A\x00T\x00A\x00[\x00Z",
                XmlError::UnclosedCdataSection,
            ),
            // Now add a four-byte UTF-16 character
            (
                21,
                b"\x00<\x00!\x00[\x00C\x00D\x00A\x00T\x00A\x00[\x00Z\xd8",
                XmlError::UnclosedCdataSection,
            ),
            (
                22,
                b"\x00<\x00!\x00[\x00C\x00D\x00A\x00T\x00A\x00[\x00Z\xd84",
                XmlError::PartialChar,
            ),
            (
                23,
                b"\x00<\x00!\x00[\x00C\x00D\x00A\x00T\x00A\x00[\x00Z\xd84\xdd",
                XmlError::PartialChar,
            ),
            (
                24,
                b"\x00<\x00!\x00[\x00C\x00D\x00A\x00T\x00A\x00[\x00Z\xd84\xdd^",
                XmlError::UnclosedCdataSection,
            ),
        ];

        for (i, (text_bytes, text, expected_error)) in cases.iter().enumerate() {
            if run.parse(&mut parser, prolog, false) == XmlStatus::Error {
                xml_failure(run, &mut parser);
            }
            let actual_status = run.parse(&mut parser, &text[..*text_bytes], true);
            assert_eq!(actual_status, XmlStatus::Error, "{run:?} case {}", i + 1);
            let actual_error = parser.error_code();
            assert_eq!(
                actual_error,
                *expected_error,
                "{run:?}: Expected error {:?} ({:?}), got {:?} ({:?}) for case {}",
                expected_error,
                xml_error_string(*expected_error),
                actual_error,
                xml_error_string(actual_error),
                i + 1
            );
            parser.reset();
        }
    });
}

// ---- Parse calls and buffers ----

/// Port of expat 2.7.2 `START_TEST(test_empty_parse)` (tests/basic_tests.c:2908-2936).
#[test]
fn test_empty_parse() {
    each_run(|run| {
        let mut parser = run.parser();
        let text = b"<doc></doc>";
        let partial = b"<doc>";

        if parse(&mut parser, b"", false) == XmlStatus::Error {
            panic!("{run:?}: Parsing empty string faulted");
        }
        if parse(&mut parser, b"", true) != XmlStatus::Error {
            panic!("{run:?}: Parsing final empty string not faulted");
        }
        if parser.error_code() != XmlError::NoElements {
            panic!("{run:?}: Parsing final empty string faulted for wrong reason");
        }

        // Now try with valid text before the empty end
        parser.reset();
        if run.parse(&mut parser, text, false) == XmlStatus::Error {
            xml_failure(run, &mut parser);
        }
        if parse(&mut parser, b"", true) == XmlStatus::Error {
            panic!("{run:?}: Parsing final empty string faulted");
        }

        // Now try with invalid text before the empty end
        parser.reset();
        if run.parse(&mut parser, partial, false) == XmlStatus::Error {
            xml_failure(run, &mut parser);
        }
        if parse(&mut parser, b"", true) != XmlStatus::Error {
            panic!("{run:?}: Parsing final incomplete empty string not faulted");
        }
    });
}

/// Port of expat 2.7.2 `START_TEST(test_negative_len_parse_buffer)`
/// (tests/basic_tests.c:2963-2991).
#[test]
fn test_negative_len_parse_buffer() {
    each_run(|run| {
        let doc = b"<root/>";
        for is_final in [false, true] {
            let mut parser = run.parser();

            assert_eq!(
                parser.error_code(),
                XmlError::None,
                "{run:?}: There was not supposed to be any initial parse error."
            );

            let at = parser
                .get_buffer(doc.len() as i32)
                .unwrap_or_else(|| panic!("{run:?}: XML_GetBuffer failed."));
            buffer_at(&mut parser, at, doc.len()).copy_from_slice(doc);

            let status = parse_buffer(&mut parser, -1, is_final);

            assert_eq!(
                status,
                XmlStatus::Error,
                "{run:?}: Negative len was expected to fail the parse but did not."
            );
            assert_eq!(
                parser.error_code(),
                XmlError::InvalidArgument,
                "{run:?}: Parse error does not match XML_ERROR_INVALID_ARGUMENT."
            );
        }
    });
}

/// `get_buffer_test_text` (tests/common.c:113-134): an element name longer than 1024
/// characters, which exercises some of the pool allocation code. The count at the end of
/// the line is the number of characters (bytes) in the element name by that point.
const GET_BUFFER_TEST_TEXT: &[u8] = concat!(
    "<documentwitharidiculouslylongelementnametotease", // 0x030
    "aparticularcorneroftheallocationinXML_GetBuffers", // 0x060
    "othatwecanimprovethecoverageyetagain012345678901", // 0x090
    "123456789abcdef0123456789abcdef0123456789abcdef0", // 0x0c0
    "123456789abcdef0123456789abcdef0123456789abcdef0", // 0x0f0
    "123456789abcdef0123456789abcdef0123456789abcdef0", // 0x120
    "123456789abcdef0123456789abcdef0123456789abcdef0", // 0x150
    "123456789abcdef0123456789abcdef0123456789abcdef0", // 0x180
    "123456789abcdef0123456789abcdef0123456789abcdef0", // 0x1b0
    "123456789abcdef0123456789abcdef0123456789abcdef0", // 0x1e0
    "123456789abcdef0123456789abcdef0123456789abcdef0", // 0x210
    "123456789abcdef0123456789abcdef0123456789abcdef0", // 0x240
    "123456789abcdef0123456789abcdef0123456789abcdef0", // 0x270
    "123456789abcdef0123456789abcdef0123456789abcdef0", // 0x2a0
    "123456789abcdef0123456789abcdef0123456789abcdef0", // 0x2d0
    "123456789abcdef0123456789abcdef0123456789abcdef0", // 0x300
    "123456789abcdef0123456789abcdef0123456789abcdef0", // 0x330
    "123456789abcdef0123456789abcdef0123456789abcdef0", // 0x360
    "123456789abcdef0123456789abcdef0123456789abcdef0", // 0x390
    "123456789abcdef0123456789abcdef0123456789abcdef0", // 0x3c0
    "123456789abcdef0123456789abcdef0123456789abcdef0", // 0x3f0
    "123456789abcdef0123456789abcdef0123456789>\n<ef0", // 0x420
)
.as_bytes();

/// Port of expat 2.7.2 `START_TEST(test_get_buffer_1)` (tests/basic_tests.c:3010-3050), with
/// `XML_FEATURE_CONTEXT_BYTES` 1024.
#[test]
fn test_get_buffer_1() {
    each_run(|run| {
        let mut parser = run.parser();
        let text = GET_BUFFER_TEST_TEXT;
        let context_bytes = 1024;

        // Attempt to allocate a negative length buffer
        assert!(
            parser.get_buffer(-12).is_none(),
            "{run:?}: Negative length buffer not failed"
        );

        // Now get a small buffer and extend it past valid length
        let at = parser
            .get_buffer(1536)
            .unwrap_or_else(|| panic!("{run:?}: 1.5K buffer failed"));
        buffer_at(&mut parser, at, text.len()).copy_from_slice(text);
        if parse_buffer(&mut parser, text.len() as i32, false) == XmlStatus::Error {
            xml_failure(run, &mut parser);
        }
        assert!(
            parser.get_buffer(i32::MAX).is_none(),
            "{run:?}: INT_MAX buffer not failed"
        );

        // Now try extending it a more reasonable but still too large amount.  The allocator
        // in XML_GetBuffer() doubles the buffer size until it exceeds the requested amount or
        // INT_MAX.  If it exceeds INT_MAX, it rejects the request, so we want a request
        // between INT_MAX and INT_MAX/2.  A gap of 1K seems comfortable, with an extra byte
        // just to ensure that the request is off any boundary.  The request will be inflated
        // internally by XML_CONTEXT_BYTES (if >=1), so we subtract that from our request.
        assert!(
            parser
                .get_buffer(i32::MAX - (context_bytes + 1025))
                .is_none(),
            "{run:?}: INT_MAX- buffer not failed"
        );

        // Now try extending it a carefully crafted amount
        assert!(
            parser.get_buffer(1000).is_some(),
            "{run:?}: 1000 buffer failed"
        );
    });
}

/// Port of expat 2.7.2 `START_TEST(test_get_buffer_2)` (tests/basic_tests.c:3053-3071).
#[test]
fn test_get_buffer_2() {
    each_run(|run| {
        let mut parser = run.parser();
        let text = GET_BUFFER_TEST_TEXT;

        // Now get a decent buffer
        let at = parser
            .get_buffer(1536)
            .unwrap_or_else(|| panic!("{run:?}: 1.5K buffer failed"));
        buffer_at(&mut parser, at, text.len()).copy_from_slice(text);
        if parse_buffer(&mut parser, text.len() as i32, false) == XmlStatus::Error {
            xml_failure(run, &mut parser);
        }

        // Extend it, to catch a different code path
        assert!(
            parser.get_buffer(1024).is_some(),
            "{run:?}: 1024 buffer failed"
        );
    });
}

/// Port of expat 2.7.2 `START_TEST(test_get_buffer_3_overflow)` (tests/basic_tests.c:3075-3095).
#[test]
fn test_get_buffer_3_overflow() {
    each_run(|run| {
        let mut parser = run.parser();

        let text = b"\n";
        let expected_keep_value = text.len() as i32;

        // After this call, variable "keep" in XML_GetBuffer will have value
        // expectedKeepValue
        if run.parse(&mut parser, text, false) == XmlStatus::Error {
            xml_failure(run, &mut parser);
        }

        assert!(expected_keep_value > 0);
        assert!(
            parser
                .get_buffer(i32::MAX - expected_keep_value + 1)
                .is_none(),
            "{run:?}: enlarging buffer not failed"
        );
    });
}

/// Port of expat 2.7.2 `START_TEST(test_getbuffer_allocates_on_zero_len)`
/// (tests/basic_tests.c:3147-3159).
#[test]
fn test_getbuffer_allocates_on_zero_len() {
    each_run(|run| {
        for first_len in [1, 0] {
            let mut parser = run.parser();
            assert!(
                parser.get_buffer(first_len).is_some(),
                "{run:?} len={first_len}"
            );
            assert!(parser.get_buffer(0).is_some(), "{run:?} len={first_len}");
            if parse_buffer(&mut parser, 0, false) != XmlStatus::Ok {
                xml_failure(run, &mut parser);
            }
        }
    });
}

/// Port of expat 2.7.2 `START_TEST(test_byte_info_at_end)` (tests/basic_tests.c:3162-3177).
#[test]
fn test_byte_info_at_end() {
    each_run(|run| {
        let mut parser = run.parser();
        let text = b"<doc></doc>";

        assert!(
            parser.current_byte_index() == -1 && parser.current_byte_count() == 0,
            "{run:?}: Byte index/count incorrect at start of parse"
        );
        if run.parse(&mut parser, text, true) == XmlStatus::Error {
            xml_failure(run, &mut parser);
        }
        // At end, the count will be zero and the index the end of string
        assert_eq!(
            parser.current_byte_count(),
            0,
            "{run:?}: Terminal byte count incorrect"
        );
        assert_eq!(
            parser.current_byte_index(),
            text.len() as i64,
            "{run:?}: Terminal byte index incorrect"
        );
    });
}

/// Port of expat 2.7.2 `START_TEST(test_byte_info_at_error)` (tests/basic_tests.c:3182-3193).
#[test]
fn test_byte_info_at_error() {
    each_run(|run| {
        let mut parser = run.parser();
        let pre_error_str = b"<doc></";
        let text = b"<doc></wombat></doc>";

        if run.parse(&mut parser, text, true) == XmlStatus::Ok {
            panic!("{run:?}: Syntax error not faulted");
        }
        assert_eq!(
            parser.current_byte_count(),
            0,
            "{run:?}: Error byte count incorrect"
        );
        assert_eq!(
            parser.current_byte_index(),
            pre_error_str.len() as i64,
            "{run:?}: Error byte index incorrect"
        );
    });
}

/// Port of expat 2.7.2 `START_TEST(test_not_predefined_entities)` (tests/basic_tests.c:3268-3280).
#[test]
fn test_not_predefined_entities() {
    each_run(|run| {
        let mut parser = run.parser();
        let text: [&[u8]; 4] = [
            b"<doc>&pt;</doc>",
            b"<doc>&amo;</doc>",
            b"<doc>&quid;</doc>",
            b"<doc>&apod;</doc>",
        ];

        for text in text {
            run.expect_failure(
                &mut parser,
                text,
                XmlError::UndefinedEntity,
                "Undefined entity not rejected",
            );
            parser.reset();
        }
    });
}

/// Port of expat 2.7.2 `START_TEST(test_predefined_entity_redefinition)`
/// (tests/basic_tests.c:3576-3583).
#[test]
fn test_predefined_entity_redefinition() {
    each_run(|run| {
        let text = b"<!DOCTYPE doc [\n<!ENTITY apos 'foo'>\n]>\n<doc>&apos;</doc>";
        run.run_character_check(&mut run.parser(), text, b"'");
    });
}

/// Port of expat 2.7.2 `START_TEST(test_unfinished_epilog)` (tests/basic_tests.c:3840-3846).
#[test]
fn test_unfinished_epilog() {
    each_run(|run| {
        let text = b"<doc></doc><";

        run.expect_failure(
            &mut run.parser(),
            text,
            XmlError::UnclosedToken,
            "Incomplete epilog entry not faulted",
        );
    });
}

/// Port of expat 2.7.2 `START_TEST(test_partial_char_in_epilog)` (tests/basic_tests.c:3848-3861).
#[test]
fn test_partial_char_in_epilog() {
    each_run(|run| {
        let mut parser = run.parser();
        let text = b"<doc></doc>\xe2\x82";

        // First check that no fault is raised if the parse is not finished
        if run.parse(&mut parser, text, false) == XmlStatus::Error {
            xml_failure(run, &mut parser);
        }
        // Now check that it is faulted once we finish
        if parse_buffer(&mut parser, 0, true) != XmlStatus::Error {
            panic!("{run:?}: Partial character in epilog not faulted");
        }
        if parser.error_code() != XmlError::PartialChar {
            xml_failure(run, &mut parser);
        }
    });
}

/// Port of expat 2.7.2 `START_TEST(test_restart_on_error)` (tests/basic_tests.c:4010-4023).
#[test]
fn test_restart_on_error() {
    each_run(|run| {
        let mut parser = run.parser();
        let text = b"<$doc><doc></doc>";

        if run.parse(&mut parser, text, true) != XmlStatus::Error {
            panic!("{run:?}: Invalid tag name not faulted");
        }
        if parser.error_code() != XmlError::InvalidToken {
            xml_failure(run, &mut parser);
        }
        if parse(&mut parser, b"", true) != XmlStatus::Error {
            panic!("{run:?}: Restarting invalid parse not faulted");
        }
        if parser.error_code() != XmlError::InvalidToken {
            xml_failure(run, &mut parser);
        }
    });
}

/// Port of expat 2.7.2 `START_TEST(test_reject_lt_in_attribute_value)`
/// (tests/basic_tests.c:4026-4033).
#[test]
fn test_reject_lt_in_attribute_value() {
    each_run(|run| {
        let text = b"<!DOCTYPE doc [<!ATTLIST doc a CDATA '<bar>'>]>\n<doc></doc>";

        run.expect_failure(
            &mut run.parser(),
            text,
            XmlError::InvalidToken,
            "Bad attribute default not faulted",
        );
    });
}

/// Port of expat 2.7.2 `START_TEST(test_reject_unfinished_param_in_att_value)`
/// (tests/basic_tests.c:4035-4042).
#[test]
fn test_reject_unfinished_param_in_att_value() {
    each_run(|run| {
        let text = b"<!DOCTYPE doc [<!ATTLIST doc a CDATA '&foo'>]>\n<doc></doc>";

        run.expect_failure(
            &mut run.parser(),
            text,
            XmlError::InvalidToken,
            "Bad attribute default not faulted",
        );
    });
}

/// Port of expat 2.7.2 `START_TEST(test_trailing_cr_in_att_value)` (tests/basic_tests.c:4044-4051).
#[test]
fn test_trailing_cr_in_att_value() {
    each_run(|run| {
        let mut parser = run.parser();
        let text = b"<doc a='value\r'/>";

        if run.parse(&mut parser, text, true) == XmlStatus::Error {
            xml_failure(run, &mut parser);
        }
    });
}

/// Port of expat 2.7.2 `START_TEST(test_invalid_character_entity)` (tests/basic_tests.c:4159-4168).
#[test]
fn test_invalid_character_entity() {
    each_run(|run| {
        let text = b"<!DOCTYPE doc [\n  <!ENTITY entity '&#x110000;'>\n]>\n<doc>&entity;</doc>";

        run.expect_failure(
            &mut run.parser(),
            text,
            XmlError::BadCharRef,
            "Out of range character reference not faulted",
        );
    });
}

/// Port of expat 2.7.2 `START_TEST(test_invalid_character_entity_2)`
/// (tests/basic_tests.c:4170-4179).
#[test]
fn test_invalid_character_entity_2() {
    each_run(|run| {
        let text = b"<!DOCTYPE doc [\n  <!ENTITY entity '&#xg0;'>\n]>\n<doc>&entity;</doc>";

        run.expect_failure(
            &mut run.parser(),
            text,
            XmlError::InvalidToken,
            "Out of range character reference not faulted",
        );
    });
}

/// Port of expat 2.7.2 `START_TEST(test_invalid_character_entity_3)`
/// (tests/basic_tests.c:4181-4201).
#[test]
fn test_invalid_character_entity_3() {
    each_run(|run| {
        let mut parser = run.parser();
        // <!DOCTYPE doc [\n
        // <!ENTITY entity '&คจ;'>\n (U+0E04 = KHO KHWAI, U+0E08 = CHO CHAN)
        // ]>\n
        // <doc>&entity;</doc>
        let text = b"\x00<\x00!\x00D\x00O\x00C\x00T\x00Y\x00P\x00E\x00 \x00d\x00o\x00c\x00 \x00[\x00\n\x00<\x00!\x00E\x00N\x00T\x00I\x00T\x00Y\x00 \x00e\x00n\x00t\x00i\x00t\x00y\x00 \x00'\x00&\x0e\x04\x0e\x08\x00;\x00'\x00>\x00\n\x00]\x00>\x00\n\x00<\x00d\x00o\x00c\x00>\x00&\x00e\x00n\x00t\x00i\x00t\x00y\x00;\x00<\x00/\x00d\x00o\x00c\x00>";

        if run.parse(&mut parser, text, true) != XmlStatus::Error {
            panic!("{run:?}: Invalid start of entity name not faulted");
        }
        if parser.error_code() != XmlError::UndefinedEntity {
            xml_failure(run, &mut parser);
        }
    });
}

/// Port of expat 2.7.2 `START_TEST(test_invalid_character_entity_4)`
/// (tests/basic_tests.c:4203-4212).
#[test]
fn test_invalid_character_entity_4() {
    each_run(|run| {
        let text = b"<!DOCTYPE doc [\n  <!ENTITY entity '&#1114112;'>\n]>\n<doc>&entity;</doc>"; // = &#x110000

        run.expect_failure(
            &mut run.parser(),
            text,
            XmlError::BadCharRef,
            "Out of range character reference not faulted",
        );
    });
}

// ---- Names, attributes and doctypes ----

/// Port of expat 2.7.2 `START_TEST(test_utf8_in_cdata_section)` (tests/basic_tests.c:4815-4825).
#[test]
fn test_utf8_in_cdata_section() {
    each_run(|run| {
        let text = b"<doc><![CDATA[one \xc3\xa9 two]]></doc>";
        let expected = b"one \xc3\xa9 two";

        run.run_character_check(&mut run.parser(), text, expected);
    });
}

/// Port of expat 2.7.2 `START_TEST(test_utf8_in_cdata_section_2)` (tests/basic_tests.c:4828-4838).
#[test]
fn test_utf8_in_cdata_section_2() {
    each_run(|run| {
        let text = b"<doc><![CDATA[\xc3\xa9]\xc3\xa9two]]></doc>";
        let expected = b"\xc3\xa9]\xc3\xa9two";

        run.run_character_check(&mut run.parser(), text, expected);
    });
}

/// Port of expat 2.7.2 `START_TEST(test_utf8_in_start_tags)` (tests/basic_tests.c:4840-4944).
#[test]
fn test_utf8_in_start_tags() {
    each_run(|run| {
        // The idea with the tests below is this: we want to cover 1-, 2- and 3-byte
        // sequences, 4-byte sequences go to isNever and are hence not a concern.
        //
        // We start with a character that is a valid name character (or even name-start
        // character, see XML 1.0r4 spec) and then we flip single bits at places where (1)
        // the result leaves the UTF-8 encoding space and (2) we stay in the same n-byte
        // sequence family.
        //
        // (good name, good name start, tag name)
        let cases: [(bool, bool, &[u8]); 24] = [
            // 1-byte UTF-8: [0xxx xxxx]
            (true, true, b"\x3A"),   // [0011 1010] = ASCII colon ':'
            (false, false, b"\xBA"), // [<1>011 1010]
            (true, false, b"\x39"),  // [0011 1001] = ASCII nine '9'
            (false, false, b"\xB9"), // [<1>011 1001]
            // 2-byte UTF-8: [110x xxxx] [10xx xxxx]
            (true, true, b"\xDB\xA5"), // [1101 1011] [1010 0101] = Arabic small waw U+06E5
            (false, false, b"\x9B\xA5"), // [1<0>01 1011] [1010 0101]
            (false, false, b"\xDB\x25"), // [1101 1011] [<0>010 0101]
            (false, false, b"\xDB\xE5"), // [1101 1011] [1<1>10 0101]
            (true, false, b"\xCC\x81"), // [1100 1100] [1000 0001] = combining char U+0301
            (false, false, b"\x8C\x81"), // [1<0>00 1100] [1000 0001]
            (false, false, b"\xCC\x01"), // [1100 1100] [<0>000 0001]
            (false, false, b"\xCC\xC1"), // [1100 1100] [1<1>00 0001]
            // 3-byte UTF-8: [1110 xxxx] [10xx xxxx] [10xxxxxx]
            (true, true, b"\xE0\xA4\x85"), // [1110 0000] [1010 0100] [1000 0101] =
            // Devanagari Letter A U+0905
            (false, false, b"\xA0\xA4\x85"), // [1<0>10 0000] [1010 0100] [1000 0101]
            (false, false, b"\xE0\x24\x85"), // [1110 0000] [<0>010 0100] [1000 0101]
            (false, false, b"\xE0\xE4\x85"), // [1110 0000] [1<1>10 0100] [1000 0101]
            (false, false, b"\xE0\xA4\x05"), // [1110 0000] [1010 0100] [<0>000 0101]
            (false, false, b"\xE0\xA4\xC5"), // [1110 0000] [1010 0100] [1<1>00 0101]
            (true, false, b"\xE0\xA4\x81"),  // [1110 0000] [1010 0100] [1000 0001] =
            // combining char U+0901
            (false, false, b"\xA0\xA4\x81"), // [1<0>10 0000] [1010 0100] [1000 0001]
            (false, false, b"\xE0\x24\x81"), // [1110 0000] [<0>010 0100] [1000 0001]
            (false, false, b"\xE0\xE4\x81"), // [1110 0000] [1<1>10 0100] [1000 0001]
            (false, false, b"\xE0\xA4\x01"), // [1110 0000] [1010 0100] [<0>000 0001]
            (false, false, b"\xE0\xA4\xC1"), // [1110 0000] [1010 0100] [1<1>00 0001]
        ];
        let at_name_start = [true, false];

        let mut fail_count = 0;

        // we need all the bytes to be parsed, but we don't want the errors that can trigger
        // on isFinal=XML_TRUE, so we skip the test if the heuristic is on.
        if run.deferral {
            return;
        }

        for (i, (good_name, good_name_start, tag_name)) in cases.iter().enumerate() {
            for &start in &at_name_start {
                let expected_success = if start { *good_name_start } else { *good_name };
                let doc = [
                    b"<".as_slice(),
                    if start { b"" } else { b"a" },
                    tag_name,
                    b"><!--",
                ]
                .concat();
                let mut parser = run.parser();

                let status = run.parse(&mut parser, &doc, false);

                let mut success = true;
                if (status == XmlStatus::Ok) != expected_success {
                    success = false;
                }
                if status == XmlStatus::Error && parser.error_code() != XmlError::InvalidToken {
                    success = false;
                }

                if !success {
                    eprintln!(
                        "{run:?}: FAIL case {:2} ({}at name start, {}-byte sequence, error code {})",
                        i + 1,
                        if start { "    " } else { "not " },
                        tag_name.len(),
                        parser.error_code() as i32
                    );
                    fail_count += 1;
                }
            }
        }

        assert!(fail_count == 0, "{run:?}: UTF-8 regression detected");
    });
}

/// Port of expat 2.7.2 `START_TEST(test_trailing_spaces_in_elements)`
/// (tests/basic_tests.c:4947-4961).
#[test]
fn test_trailing_spaces_in_elements() {
    each_run(|run| {
        let mut parser = run.parser();
        let text = b"<doc   >Hi</doc >";
        let expected = b"doc/doc";
        let storage = CharData::new();

        parser.set_element_handler(Some(record_start(&storage)), Some(record_end(&storage)));
        if run.parse(&mut parser, text, true) == XmlStatus::Error {
            xml_failure(run, &mut parser);
        }
        storage.borrow().check(run, expected);
    });
}

/// `accumulate_attribute` over `text`, checked against `expected`.
fn check_first_attribute(run: Run, text: &[u8], expected: &[u8]) {
    let mut parser = run.parser();
    let storage = CharData::new();

    parser.set_start_element_handler(Some(accumulate_attribute(&storage)));
    if run.parse(&mut parser, text, true) == XmlStatus::Error {
        xml_failure(run, &mut parser);
    }
    storage.borrow().check(run, expected);
}

/// Port of expat 2.7.2 `START_TEST(test_utf16_attribute)` (tests/basic_tests.c:4963-4981).
#[test]
fn test_utf16_attribute() {
    each_run(|run| {
        // <d {KHO KHWAI}{CHO CHAN}='a'/>
        // where {KHO KHWAI} = U+0E04 = 0xe0 0xb8 0x84 in UTF-8
        // and   {CHO CHAN}  = U+0E08 = 0xe0 0xb8 0x88 in UTF-8
        let text = b"<\x00d\x00 \x00\x04\x0e\x08\x0e=\x00'\x00a\x00'\x00/\x00>\x00";
        check_first_attribute(run, text, b"a");
    });
}

/// Port of expat 2.7.2 `START_TEST(test_utf16_second_attr)` (tests/basic_tests.c:4983-5001).
#[test]
fn test_utf16_second_attr() {
    each_run(|run| {
        // <d a='1' {KHO KHWAI}{CHO CHAN}='2'/>
        // where {KHO KHWAI} = U+0E04 = 0xe0 0xb8 0x84 in UTF-8
        // and   {CHO CHAN}  = U+0E08 = 0xe0 0xb8 0x88 in UTF-8
        let text =
            b"<\x00d\x00 \x00a\x00=\x00'\x001\x00'\x00 \x00\x04\x0e\x08\x0e=\x00'\x002\x00'\x00/\x00>\x00";
        check_first_attribute(run, text, b"1");
    });
}

/// Port of expat 2.7.2 `START_TEST(test_attr_after_solidus)` (tests/basic_tests.c:5003-5008).
#[test]
fn test_attr_after_solidus() {
    each_run(|run| {
        let text = b"<doc attr1='a' / attr2='b'>";

        run.expect_failure(
            &mut run.parser(),
            text,
            XmlError::InvalidToken,
            "Misplaced / not faulted",
        );
    });
}

/// Port of expat 2.7.2 `START_TEST(test_bad_attr_desc_keyword)` (tests/basic_tests.c:5046-5055).
#[test]
fn test_bad_attr_desc_keyword() {
    each_run(|run| {
        let text = b"<!DOCTYPE doc [\n  <!ATTLIST doc attr CDATA #!IMPLIED>\n]>\n<doc />";

        run.expect_failure(
            &mut run.parser(),
            text,
            XmlError::InvalidToken,
            "Bad keyword !IMPLIED not faulted",
        );
    });
}

/// Port of expat 2.7.2 `START_TEST(test_bad_attr_desc_keyword_utf16)`
/// (tests/basic_tests.c:5061-5081).
#[test]
fn test_bad_attr_desc_keyword_utf16() {
    each_run(|run| {
        let mut parser = run.parser();
        // <!DOCTYPE d [
        // <!ATTLIST d a CDATA #{KHO KHWAI}{CHO CHAN}>
        // ]><d/>
        //
        // where {KHO KHWAI} = U+0E04 = 0xe0 0xb8 0x84 in UTF-8
        // and   {CHO CHAN}  = U+0E08 = 0xe0 0xb8 0x88 in UTF-8
        let text = b"\x00<\x00!\x00D\x00O\x00C\x00T\x00Y\x00P\x00E\x00 \x00d\x00 \x00[\x00\n\x00<\x00!\x00A\x00T\x00T\x00L\x00I\x00S\x00T\x00 \x00d\x00 \x00a\x00 \x00C\x00D\x00A\x00T\x00A\x00 \x00#\x0e\x04\x0e\x08\x00>\x00\n\x00]\x00>\x00<\x00d\x00/\x00>";

        if run.parse(&mut parser, text, true) != XmlStatus::Error {
            panic!("{run:?}: Invalid UTF16 attribute keyword not faulted");
        }
        if parser.error_code() != XmlError::Syntax {
            xml_failure(run, &mut parser);
        }
    });
}

/// Port of expat 2.7.2 `START_TEST(test_bad_doctype_utf8)` (tests/basic_tests.c:5096-5102).
#[test]
fn test_bad_doctype_utf8() {
    each_run(|run| {
        let text = b"<!DOCTYPE \xDB\x25doc><doc/>"; // [1101 1011] [<0>010 0101]
        run.expect_failure(
            &mut run.parser(),
            text,
            XmlError::InvalidToken,
            "Invalid UTF-8 in DOCTYPE not faulted",
        );
    });
}

/// Port of expat 2.7.2 `START_TEST(test_bad_doctype_utf16)` (tests/basic_tests.c:5104-5121).
#[test]
fn test_bad_doctype_utf16() {
    each_run(|run| {
        let mut parser = run.parser();
        // <!DOCTYPE doc [ \x06f2 ]><doc/>
        //
        // U+06F2 = EXTENDED ARABIC-INDIC DIGIT TWO, a valid number (name character) but not a
        // valid letter (name start character)
        let text = b"\x00<\x00!\x00D\x00O\x00C\x00T\x00Y\x00P\x00E\x00 \x00d\x00o\x00c\x00 \x00[\x00 \x06\xf2\x00 \x00]\x00>\x00<\x00d\x00o\x00c\x00/\x00>";

        if run.parse(&mut parser, text, true) != XmlStatus::Error {
            panic!("{run:?}: Invalid bytes in DOCTYPE not faulted");
        }
        if parser.error_code() != XmlError::Syntax {
            xml_failure(run, &mut parser);
        }
    });
}

/// Port of expat 2.7.2 `START_TEST(test_bad_doctype_plus)` (tests/basic_tests.c:5123-5130).
#[test]
fn test_bad_doctype_plus() {
    each_run(|run| {
        let text = b"<!DOCTYPE 1+ [ <!ENTITY foo 'bar'> ]>\n<1+>&foo;</1+>";

        run.expect_failure(
            &mut run.parser(),
            text,
            XmlError::InvalidToken,
            "'+' in document name not faulted",
        );
    });
}

/// Port of expat 2.7.2 `START_TEST(test_bad_doctype_star)` (tests/basic_tests.c:5132-5139).
#[test]
fn test_bad_doctype_star() {
    each_run(|run| {
        let text = b"<!DOCTYPE 1* [ <!ENTITY foo 'bar'> ]>\n<1*>&foo;</1*>";

        run.expect_failure(
            &mut run.parser(),
            text,
            XmlError::InvalidToken,
            "'*' in document name not faulted",
        );
    });
}

/// Port of expat 2.7.2 `START_TEST(test_bad_doctype_query)` (tests/basic_tests.c:5141-5148).
#[test]
fn test_bad_doctype_query() {
    each_run(|run| {
        let text = b"<!DOCTYPE 1? [ <!ENTITY foo 'bar'> ]>\n<1?>&foo;</1?>";

        run.expect_failure(
            &mut run.parser(),
            text,
            XmlError::InvalidToken,
            "'?' in document name not faulted",
        );
    });
}

/// Port of expat 2.7.2 `START_TEST(test_entity_in_utf16_be_attr)`
/// (tests/basic_tests.c:5167-5187).
#[test]
fn test_entity_in_utf16_be_attr() {
    each_run(|run| {
        // <e a='&#228; &#x00E4;'></e>
        let text = b"\x00<\x00e\x00 \x00a\x00=\x00'\x00&\x00#\x002\x002\x008\x00;\x00 \x00&\x00#\x00x\x000\x000\x00E\x004\x00;\x00'\x00>\x00<\x00/\x00e\x00>";
        let expected = b"\xc3\xa4 \xc3\xa4";
        check_first_attribute(run, text, expected);
    });
}

/// Port of expat 2.7.2 `START_TEST(test_entity_in_utf16_le_attr)`
/// (tests/basic_tests.c:5189-5209).
#[test]
fn test_entity_in_utf16_le_attr() {
    each_run(|run| {
        // <e a='&#228; &#x00E4;'></e>
        let text = b"<\x00e\x00 \x00a\x00=\x00'\x00&\x00#\x002\x002\x008\x00;\x00 \x00&\x00#\x00x\x000\x000\x00E\x004\x00;\x00'\x00>\x00<\x00/\x00e\x00>\x00";
        let expected = b"\xc3\xa4 \xc3\xa4";
        check_first_attribute(run, text, expected);
    });
}

/// The doctype and declaration failures: `text`, the error, the message.
fn expect_failures(cases: &[(&[u8], XmlError, &str)]) {
    each_run(|run| {
        for (text, code, message) in cases {
            run.expect_failure(&mut run.parser(), text, *code, message);
        }
    });
}

/// Port of expat 2.7.2 `START_TEST(test_short_doctype)` (tests/basic_tests.c:5278-5283).
#[test]
fn test_short_doctype() {
    expect_failures(&[(
        b"<!DOCTYPE doc></doc>",
        XmlError::InvalidToken,
        "DOCTYPE without subset not rejected",
    )]);
}

/// Port of expat 2.7.2 `START_TEST(test_short_doctype_2)` (tests/basic_tests.c:5285-5290).
#[test]
fn test_short_doctype_2() {
    expect_failures(&[(
        b"<!DOCTYPE doc PUBLIC></doc>",
        XmlError::Syntax,
        "DOCTYPE without Public ID not rejected",
    )]);
}

/// Port of expat 2.7.2 `START_TEST(test_short_doctype_3)` (tests/basic_tests.c:5292-5297).
#[test]
fn test_short_doctype_3() {
    expect_failures(&[(
        b"<!DOCTYPE doc SYSTEM></doc>",
        XmlError::Syntax,
        "DOCTYPE without System ID not rejected",
    )]);
}

/// Port of expat 2.7.2 `START_TEST(test_long_doctype)` (tests/basic_tests.c:5299-5303).
#[test]
fn test_long_doctype() {
    expect_failures(&[(
        b"<!DOCTYPE doc PUBLIC 'foo' 'bar' 'baz'></doc>",
        XmlError::Syntax,
        "DOCTYPE with extra ID not rejected",
    )]);
}

/// Port of expat 2.7.2 `START_TEST(test_bad_entity)` (tests/basic_tests.c:5305-5313).
#[test]
fn test_bad_entity() {
    expect_failures(&[(
        b"<!DOCTYPE doc [\n  <!ENTITY foo PUBLIC>\n]>\n<doc/>",
        XmlError::Syntax,
        "ENTITY without Public ID is not rejected",
    )]);
}

/// Port of expat 2.7.2 `START_TEST(test_bad_entity_2)` (tests/basic_tests.c:5316-5324).
#[test]
fn test_bad_entity_2() {
    expect_failures(&[(
        b"<!DOCTYPE doc [\n  <!ENTITY % foo bar>\n]>\n<doc/>",
        XmlError::Syntax,
        "ENTITY without Public ID is not rejected",
    )]);
}

/// Port of expat 2.7.2 `START_TEST(test_bad_entity_3)` (tests/basic_tests.c:5326-5334).
#[test]
fn test_bad_entity_3() {
    expect_failures(&[(
        b"<!DOCTYPE doc [\n  <!ENTITY % foo PUBLIC>\n]>\n<doc/>",
        XmlError::Syntax,
        "Parameter ENTITY without Public ID is not rejected",
    )]);
}

/// Port of expat 2.7.2 `START_TEST(test_bad_entity_4)` (tests/basic_tests.c:5336-5344).
#[test]
fn test_bad_entity_4() {
    expect_failures(&[(
        b"<!DOCTYPE doc [\n  <!ENTITY % foo SYSTEM>\n]>\n<doc/>",
        XmlError::Syntax,
        "Parameter ENTITY without Public ID is not rejected",
    )]);
}

/// Port of expat 2.7.2 `START_TEST(test_bad_notation)` (tests/basic_tests.c:5346-5354).
#[test]
fn test_bad_notation() {
    expect_failures(&[(
        b"<!DOCTYPE doc [\n  <!NOTATION n SYSTEM>\n]>\n<doc/>",
        XmlError::Syntax,
        "Notation without System ID is not rejected",
    )]);
}

/// Port of expat 2.7.2 `START_TEST(test_entity_ref_no_elements)` (tests/basic_tests.c:5417-5428).
#[test]
fn test_entity_ref_no_elements() {
    each_run(|run| {
        let text = b"<!DOCTYPE foo [\n<!ENTITY e1 \"test\">\n]> <foo>&e1;"; // intentionally missing newline

        let mut parser = run.parser();
        assert_eq!(
            run.parse(&mut parser, text, true),
            XmlStatus::Error,
            "{run:?}"
        );
        assert_eq!(parser.error_code(), XmlError::NoElements, "{run:?}");
    });
}

/// Port of expat 2.7.2 `START_TEST(test_deep_nested_entity)` (tests/basic_tests.c:5431-5473).
#[test]
fn test_deep_nested_entity() {
    each_run(|run| {
        const N_LINES: usize = 60000;

        // Create the XML
        let mut text = String::from("<!DOCTYPE foo [\n\t<!ENTITY s0 'deepText'>\n");
        for i in 1..N_LINES {
            text += &format!("  <!ENTITY s{i} '&s{};'>\n", i - 1);
        }
        text += &format!("]> <foo>&s{};</foo>\n", N_LINES - 1);

        let expected = b"deepText";

        let storage = CharData::new();
        let mut parser = run.parser();
        parser.set_character_data_handler(Some(accumulate_characters(&storage)));

        if run.parse(&mut parser, text.as_bytes(), true) == XmlStatus::Error {
            xml_failure(run, &mut parser);
        }

        storage.borrow().check(run, expected);
    });
}

// ---- Reparse deferral ----

/// Port of expat 2.7.2 `START_TEST(test_big_tokens_scale_linearly)`
/// (tests/basic_tests.c:5621-5704).
#[test]
fn test_big_tokens_scale_linearly() {
    each_run(|run| {
        let text: [(&[u8], &[u8]); 5] = [
            (b"<a>", b"</a>"),                      // assumed good, used as baseline
            (b"<b><![CDATA[ value: ", b" ]]></b>"), // CDATA, performed OK before patch
            (b"<c attr='", b"'></c>"),              // big attribute, used to be O(N^2)
            (b"<d><!-- ", b" --></d>"),             // long comment, used to be O(N^2)
            (b"<e><", b"/></e>"),                   // big elem name, used to be O(N^2)
        ];
        let aaaaaa = [b'a'; 4096];
        let fillsize = aaaaaa.len();
        let fillcount = 100;
        let approx_bytes = (fillsize * fillcount) as u32; // ignore pre/post.
        let max_factor = 4;
        let max_scanned = max_factor * approx_bytes;

        if !run.deferral {
            return; // heuristic is disabled; we would get O(n^2) and fail.
        }

        for (pre, post) in text {
            let mut parser = run.parser();

            // parse the start text
            parser.bytes_scanned = 0;
            if run.parse(&mut parser, pre, false) != XmlStatus::Ok {
                xml_failure(run, &mut parser);
            }

            // parse lots of 'a', failing the test early if it takes too long
            let mut past_max_count = 0;
            for f in 0..fillcount {
                if run.parse(&mut parser, &aaaaaa, false) != XmlStatus::Ok {
                    xml_failure(run, &mut parser);
                }
                if parser.bytes_scanned > max_scanned {
                    // We're not done, and have already passed the limit -- the test will
                    // definitely fail. This block allows us to save time by failing early.
                    let pushed = pre.len() + (f + 1) * fillsize;
                    eprintln!(
                        "{run:?}: after {}/{fillcount} loops: pushed={pushed} scanned={} \
                         max_scanned: {max_scanned}",
                        f + 1,
                        parser.bytes_scanned
                    );
                    past_max_count += 1;
                    // We are failing, but allow a few log prints first. If we don't reach a
                    // count of five, the test will fail after the loop instead.
                    assert!(past_max_count < 5, "{run:?}");
                }
            }

            // parse the end text
            if run.parse(&mut parser, post, true) != XmlStatus::Ok {
                xml_failure(run, &mut parser);
            }

            // or the counter isn't working
            assert!(parser.bytes_scanned > approx_bytes, "{run:?}");
            assert!(
                parser.bytes_scanned <= max_scanned,
                "{run:?}: scanned too many bytes: {} (max {max_scanned})",
                parser.bytes_scanned
            );
        }
    });
}

/// Port of expat 2.7.2 `START_TEST(test_set_reparse_deferral)` (tests/basic_tests.c:5706-5775).
#[test]
fn test_set_reparse_deferral() {
    each_run(|run| {
        let pre = b"<d>";
        let start = b"<x attr='";
        let end = b"'></x>";
        let eeeeee = [b'e'; 100];
        let fillsize = eeeeee.len();

        for enabled in [0u8, 1] {
            let mut parser = run.parser();
            assert!(parser.set_reparse_deferral_enabled(enabled), "{run:?}");
            // pre-grow the buffer to avoid reparsing due to almost-fullness
            assert!(
                parser.get_buffer((fillsize * 10103) as i32).is_some(),
                "{run:?}"
            );

            let storage = CharData::new();
            parser.set_start_element_handler(Some(record_start(&storage)));

            // parse the start text
            if parse(&mut parser, pre, false) != XmlStatus::Ok {
                xml_failure(run, &mut parser);
            }
            storage.borrow().check(run, b"d"); // first element should be done

            // ..and the start of the token
            if parse(&mut parser, start, false) != XmlStatus::Ok {
                xml_failure(run, &mut parser);
            }
            storage.borrow().check(run, b"d"); // still just the first one

            // try to parse lots of 'e', but the token isn't finished
            for _ in 0..100 {
                if parse(&mut parser, &eeeeee, false) != XmlStatus::Ok {
                    xml_failure(run, &mut parser);
                }
            }
            storage.borrow().check(run, b"d"); // *still* just the first one

            // end the <x> token.
            if parse(&mut parser, end, false) != XmlStatus::Ok {
                xml_failure(run, &mut parser);
            }

            if enabled == 1 {
                // In general, we may need to push more data to trigger a reparse attempt,
                // but in this test, the data is constructed to always require it.
                storage.borrow().check(run, b"d"); // or the test is incorrect
                // 2x the token length should suffice; the +1 covers the start and end.
                for _ in 0..101 {
                    if parse(&mut parser, &eeeeee, false) != XmlStatus::Ok {
                        xml_failure(run, &mut parser);
                    }
                }
            }
            storage.borrow().check(run, b"dx"); // the <x> should be done
        }
    });
}

/// Port of expat 2.7.2 `START_TEST(test_set_reparse_deferral_on_the_fly)`
/// (tests/basic_tests.c:5909-5958).
#[test]
fn test_set_reparse_deferral_on_the_fly() {
    each_run(|run| {
        let pre = b"<d><x attr='";
        let end = b"'></x>";
        let iiiiii = [b'i'; 100];

        let mut parser = run.parser();
        assert!(parser.set_reparse_deferral_enabled(1), "{run:?}");

        let storage = CharData::new();
        parser.set_start_element_handler(Some(record_start(&storage)));

        // parse the start text
        if parse(&mut parser, pre, false) != XmlStatus::Ok {
            xml_failure(run, &mut parser);
        }
        storage.borrow().check(run, b"d"); // first element should be done

        // try to parse some 'i', but the token isn't finished
        if parse(&mut parser, &iiiiii, false) != XmlStatus::Ok {
            xml_failure(run, &mut parser);
        }
        storage.borrow().check(run, b"d"); // *still* just the first one

        // end the <x> token.
        if parse(&mut parser, end, false) != XmlStatus::Ok {
            xml_failure(run, &mut parser);
        }
        storage.borrow().check(run, b"d"); // not yet.

        // now change the heuristic setting and add *no* data
        assert!(parser.set_reparse_deferral_enabled(0), "{run:?}");
        // we avoid isFinal=XML_TRUE, because that would force-bypass the heuristic.
        if parse(&mut parser, b"", false) != XmlStatus::Ok {
            xml_failure(run, &mut parser);
        }
        storage.borrow().check(run, b"dx");
    });
}

/// Port of expat 2.7.2 `START_TEST(test_set_bad_reparse_option)` (tests/basic_tests.c:5960-5973).
#[test]
fn test_set_bad_reparse_option() {
    each_run(|run| {
        let mut parser = run.parser();
        assert!(!parser.set_reparse_deferral_enabled(2));
        assert!(!parser.set_reparse_deferral_enabled(3));
        assert!(!parser.set_reparse_deferral_enabled(99));
        assert!(!parser.set_reparse_deferral_enabled(127));
        assert!(!parser.set_reparse_deferral_enabled(128));
        assert!(!parser.set_reparse_deferral_enabled(129));
        assert!(!parser.set_reparse_deferral_enabled(255));
        assert!(parser.set_reparse_deferral_enabled(0));
        assert!(parser.set_reparse_deferral_enabled(1));
    });
}

/// Port of expat 2.7.2 `START_TEST(test_varying_buffer_fills)` (tests/basic_tests.c:6103-6214).
#[test]
fn test_varying_buffer_fills() {
    each_run(|run| {
        const KIB: i64 = 1024;
        const MIB: i64 = 1024 * KIB;
        let document_length = (16 * MIB) as usize;
        let big = 7654321; // arbitrarily chosen between 4 and 8 MiB

        if run.chunk_size != 0 {
            return; // this test is slow, and doesn't use _XML_Parse_SINGLE_BYTES().
        }

        let mut document = vec![b'x'; document_length];
        document[0] = b'<';
        document[1] = b't';
        document[2..big - 1].fill(b' '); // a very spacy token
        document[big - 1] = b'>';

        // Each testcase is a list of buffer fill sizes, terminated by a value < 0. When reparse
        // deferral is enabled, the final (negated) value is the expected maximum number of
        // bytes scanned in parse attempts.
        let testcases: [&[i64]; 11] = [
            &[8 * MIB, -8 * MIB],
            &[4 * MIB, 4 * MIB, -12 * MIB], // try at 4MB, then 8MB = 12 MB total
            // zero-size fills shouldn't trigger the bypass
            &[4 * MIB, 0, 4 * MIB, -12 * MIB],
            &[4 * MIB, 0, 0, 4 * MIB, -12 * MIB],
            &[4 * MIB, 0, MIB, 0, 3 * MIB, -12 * MIB],
            // try to hit the buffer ceiling only once (at the end)
            &[
                4 * MIB,
                2 * MIB,
                MIB,
                512 * KIB,
                256 * KIB,
                256 * KIB,
                -12 * MIB,
            ],
            // try to hit the same buffer ceiling multiple times
            &[4 * MIB + 1, 2 * MIB, MIB, 512 * KIB, -25 * MIB],
            // try to hit every ceiling, by always landing 1K shy of the buffer size
            &[
                KIB,
                2 * KIB,
                4 * KIB,
                8 * KIB,
                16 * KIB,
                32 * KIB,
                64 * KIB,
                128 * KIB,
                256 * KIB,
                512 * KIB,
                MIB,
                2 * MIB,
                4 * MIB,
                -16 * MIB,
            ],
            // try to avoid every ceiling, by always landing 1B past the buffer size the normal
            // 2x heuristic threshold still forces parse attempts.
            &[
                2 * KIB + 1, // will attempt 2KiB + 1 ==> total 2KiB + 1
                2 * KIB,
                4 * KIB, // will attempt 8KiB + 1 ==> total 10KiB + 2
                8 * KIB,
                16 * KIB, // will attempt 32KiB + 1 ==> total 42KiB + 3
                32 * KIB,
                64 * KIB, // will attempt 128KiB + 1 ==> total 170KiB + 4
                128 * KIB,
                256 * KIB, // will attempt 512KiB + 1 ==> total 682KiB + 5
                512 * KIB,
                MIB, // will attempt 2MiB + 1 ==> total 2M + 682K + 6
                2 * MIB,
                4 * MIB, // will attempt 8MiB + 1 ==> total 10M + 682K + 7
                -(10 * MIB + 682 * KIB + 7),
            ],
            // try to avoid every ceiling again, except on our last fill.
            &[
                2 * KIB + 1, // will attempt 2KiB + 1 ==> total 2KiB + 1
                2 * KIB,
                4 * KIB, // will attempt 8KiB + 1 ==> total 10KiB + 2
                8 * KIB,
                16 * KIB, // will attempt 32KiB + 1 ==> total 42KiB + 3
                32 * KIB,
                64 * KIB, // will attempt 128KiB + 1 ==> total 170KiB + 4
                128 * KIB,
                256 * KIB, // will attempt 512KiB + 1 ==> total 682KiB + 5
                512 * KIB,
                MIB, // will attempt 2MiB + 1 ==> total 2M + 682K + 6
                2 * MIB,
                4 * MIB - 1, // will attempt 8MiB ==> total 10M + 682K + 6
                -(10 * MIB + 682 * KIB + 6),
            ],
            // try to hit ceilings on the way multiple times
            &[
                512 * KIB + 1,
                256 * KIB,
                128 * KIB,
                128 * KIB - 1, // 1 MiB buffer
                512 * KIB + 1,
                256 * KIB,
                128 * KIB,
                128 * KIB - 1, // 2 MiB buffer
                MIB + 1,
                512 * KIB,
                256 * KIB,
                256 * KIB - 1, // 4 MiB buffer
                2 * MIB + 1,
                MIB,
                512 * KIB, // 8 MiB buffer
                // we'll make a parse attempt at every parse call
                -(45 * MIB + 12),
            ],
        ];
        for (test_i, fills) in testcases.iter().enumerate() {
            let mut parser = run.parser();

            let storage = CharData::new();
            parser.set_start_element_handler(Some(record_start(&storage)));

            parser.bytes_scanned = 0;
            let mut worstcase_bytes: i64 = 0; // sum of (buffered bytes at each XML_Parse call)
            let mut offset: usize = 0;
            let mut fillsize = fills.iter();
            let mut fill = *fillsize.next().expect("a fill");
            while fill >= 0 {
                let fill_len = fill as usize;
                assert!(offset + fill_len <= document_length); // or test is invalid
                if parse(&mut parser, &document[offset..offset + fill_len], false) != XmlStatus::Ok
                {
                    xml_failure(run, &mut parser);
                }
                offset += fill_len;
                fill = *fillsize.next().expect("a fill");
                assert!(offset as i64 <= i32::MAX as i64 - worstcase_bytes); // avoid overflow
                worstcase_bytes += offset as i64; // we might've tried to parse all pending bytes
            }
            // the big token should've been parsed
            assert_eq!(storage.borrow().count, 1, "{run:?} #{test_i}");
            // test-the-test: does our counter work?
            assert!(parser.bytes_scanned > 0, "{run:?} #{test_i}");
            if run.deferral {
                // heuristic is enabled; some XML_Parse calls may have deferred reparsing
                let max_bytes_scanned = (-fill) as u32;
                assert!(
                    parser.bytes_scanned <= max_bytes_scanned,
                    "{run:?} #{test_i}: bytes scanned in parse attempts: actual={} limit={}",
                    parser.bytes_scanned,
                    max_bytes_scanned
                );
            }
            assert!(
                i64::from(parser.bytes_scanned) <= worstcase_bytes,
                "{run:?} #{test_i}"
            );
        }
    });
}

// ---- Accounting ----

/// Port of expat 2.7.2 `START_TEST(test_accounting_precision)` (tests/acc_tests.c:59-294),
/// the cases without parameter or external entities: the direct bytes are the document's,
/// the indirect bytes those the test expects. `sizeof(XML_Char)` is 1.
#[test]
fn test_accounting_precision() {
    each_run(|run| {
        let cases: &[(&[u8], u64)] = &[
            (b"<e/>", 0),
            (b"<e></e>", 0),
            // Attributes
            (b"<e k1=\"v2\" k2=\"v2\"/>", 0),
            (b"<e k1=\"v2\" k2=\"v2\"></e>", 0),
            (b"<p:e xmlns:p=\"https://domain.invalid/\" />", 0),
            (b"<e k=\"&amp;&apos;&gt;&lt;&quot;\" />", 5 /* number of predefined entities */),
            (b"<e1 xmlns='https://example.org/'>\n  <e2 xmlns=''/>\n</e1>", 0),
            // Text
            (b"<e>text</e>", 0),
            (b"<e1><e2>text1<e3/>text2</e2></e1>", 0),
            (b"<e>&amp;&apos;&gt;&lt;&quot;</e>", 5 /* number of predefined entities */),
            (b"<e>&#65;&#41;</e>", 0),
            // Prolog
            (b"<?xml version=\"1.0\"?><root/>", 0),
            // Whitespace
            (b"  <e1>  <e2>  </e2>  </e1>  ", 0),
            (b"<e1  ><e2  /></e1  >", 0),
            (b"<e1><e2 k = \"v\"/><e3 k = 'v'/></e1>", 0),
            // Comments
            (b"<!-- Comment --><e><!-- Comment --></e>", 0),
            // Processing instructions
            (
                b"<?xml-stylesheet type=\"text/xsl\" href=\"https://domain.invalid/\" media=\"all\"?><e/>",
                0,
            ),
            (b"<?pi0?><?pi1 ?><?pi2  ?><r/><?pi4?>", 0),
            // CDATA
            (b"<e><![CDATA[one two three]]></e>", 0),
            // The following is the essence of this OSS-Fuzz finding:
            // https://bugs.chromium.org/p/oss-fuzz/issues/detail?id=34302
            // https://oss-fuzz.com/testcase-detail/4860575394955264
            (
                b"<!DOCTYPE r [\n<!ENTITY e \"111<![CDATA[2 <= 2]]>333\">\n]>\n<r>&e;</r>\n",
                b"111<![CDATA[2 <= 2]]>333".len() as u64,
            ),
            // General entities
            (
                b"<!DOCTYPE root [\n<!ENTITY nine \"123456789\">\n]>\n<root>&nine;</root>",
                b"123456789".len() as u64,
            ),
            (
                b"<!DOCTYPE root [\n<!ENTITY nine \"123456789\">\n]>\n<root k1=\"&nine;\"/>",
                b"123456789".len() as u64,
            ),
            (
                b"<!DOCTYPE root [\n<!ENTITY nine \"123456789\">\n<!ENTITY nine2 \"&nine;&nine;\">\n]>\n<root>&nine2;&nine2;&nine2;</root>",
                3 /* calls to &nine2; */ * 2 /* calls to &nine; */
                    * (b"&nine;".len() + b"123456789".len()) as u64,
            ),
        ];

        for (u, (primary_text, extra)) in cases.iter().enumerate() {
            let expected_count_bytes_direct = primary_text.len() as u64;
            let expected_count_bytes_indirect = *extra;

            let mut parser = run.parser();
            // XML_SetParamEntityParsing(parser, XML_PARAM_ENTITY_PARSING_ALWAYS) changes
            // nothing for documents without parameter entities.

            if run.parse(&mut parser, primary_text, true) != XmlStatus::Ok {
                xml_failure(run, &mut parser);
            }

            let actual_count_bytes_direct = parser.accounting.count_bytes_direct;
            let actual_count_bytes_indirect = parser.accounting.count_bytes_indirect;

            assert_eq!(
                actual_count_bytes_direct,
                expected_count_bytes_direct,
                "{run:?}: Document {} of {}: Count of direct bytes is off",
                u + 1,
                cases.len()
            );
            assert_eq!(
                actual_count_bytes_indirect,
                expected_count_bytes_indirect,
                "{run:?}: Document {} of {}: Count of indirect bytes is off",
                u + 1,
                cases.len()
            );
        }
    });
}

// ---- The allocation tracker ----

/// Port of expat 2.7.2 `START_TEST(test_alloc_tracker_size_recorded)`
/// (tests/alloc_tests.c:2094-2126): the port counts blocks without allocating them, so the
/// memory suite makes no difference, and `Block::size` is the recorded size.
#[test]
fn test_alloc_tracker_size_recorded() {
    each_run(|run| {
        for use_mem_suite in [true, false] {
            let mut parser = run.parser();

            let mut ptr = parser.expat_malloc(10).expect("a block");
            assert_eq!(ptr.size, 10, "{run:?} useMemSuite={use_mem_suite}");

            assert!(!parser.expat_realloc(&mut ptr, usize::MAX / 2));

            assert_eq!(ptr.size, 10); // i.e. unchanged

            assert!(parser.expat_realloc(&mut ptr, 20));
            assert_eq!(ptr.size, 20);

            parser.expat_free(ptr);
        }
    });
}

/// Port of expat 2.7.2 `START_TEST(test_alloc_tracker_maximum_amplification)`
/// (tests/alloc_tests.c:2128-2160).
#[test]
fn test_alloc_tracker_maximum_amplification() {
    each_run(|run| {
        if run.deferral {
            return;
        }

        let mut parser = run.parser();

        // Get .m_accounting.countBytesDirect from 0 to 3
        let chunk = b"<e>";
        assert_eq!(
            run.parse(&mut parser, chunk, false),
            XmlStatus::Ok,
            "{run:?}"
        );

        // Stop activation threshold from interfering
        assert!(parser.set_alloc_tracker_activation_threshold(0));

        // Exceed maximum amplification: should be rejected.
        assert!(parser.expat_malloc(1000).is_none(), "{run:?}");

        // Increase maximum amplification, and try the same amount once more: should work.
        assert!(parser.set_alloc_tracker_maximum_amplification(3000.0f32));

        let ptr = parser.expat_malloc(1000);
        assert!(ptr.is_some(), "{run:?}");
        parser.expat_free(ptr.expect("a block"));
    });
}

/// Port of expat 2.7.2 `START_TEST(test_alloc_tracker_threshold)` (tests/alloc_tests.c:2162-2178).
#[test]
fn test_alloc_tracker_threshold() {
    each_run(|run| {
        let mut parser = run.parser();

        // Exceed maximum amplification *before* (default) threshold: should work.
        let ptr = parser.expat_malloc(1000);
        assert!(ptr.is_some(), "{run:?}");
        parser.expat_free(ptr.expect("a block"));

        // Exceed maximum amplification *after* threshold: should be rejected.
        assert!(parser.set_alloc_tracker_activation_threshold(999));
        assert!(parser.expat_malloc(1000).is_none(), "{run:?}");
    });
}

/// Port of expat 2.7.2 `START_TEST(test_alloc_tracker_getbuffer_unlimited)`
/// (tests/alloc_tests.c:2180-2195).
#[test]
fn test_alloc_tracker_getbuffer_unlimited() {
    each_run(|run| {
        let mut parser = run.parser();

        // Artificially lower threshold
        assert!(parser.set_alloc_tracker_activation_threshold(0));

        // Self-test: Prove that threshold is as rejecting as expected
        assert!(parser.expat_malloc(1000).is_none(), "{run:?}");
        // XML_GetBuffer should be allowed to pass, though
        assert!(parser.get_buffer(1000).is_some(), "{run:?}");
    });
}

/// Port of expat 2.7.2 `START_TEST(test_alloc_tracker_api)` (tests/alloc_tests.c:2197-2249),
/// without the calls on a NULL parser or a parser with a parent, which the port has not.
#[test]
fn test_alloc_tracker_api() {
    each_run(|run| {
        let mut parser_without_parent = run.parser();

        // XML_SetAllocTrackerMaximumAmplification, error cases
        assert!(
            !parser_without_parent.set_alloc_tracker_maximum_amplification(f32::NAN),
            "Call with NaN limit is NOT supposed to succeed"
        );
        assert!(
            !parser_without_parent.set_alloc_tracker_maximum_amplification(-1.0f32),
            "Call with negative limit is NOT supposed to succeed"
        );
        assert!(
            !parser_without_parent.set_alloc_tracker_maximum_amplification(0.9f32),
            "Call with positive limit <1.0 is NOT supposed to succeed"
        );

        // XML_SetAllocTrackerMaximumAmplification, success cases
        assert!(
            parser_without_parent.set_alloc_tracker_maximum_amplification(1.0f32),
            "Call with positive limit >=1.0 is supposed to succeed"
        );
        assert!(
            parser_without_parent.set_alloc_tracker_maximum_amplification(123456.789f32),
            "Call with positive limit >=1.0 is supposed to succeed"
        );
        assert!(
            parser_without_parent.set_alloc_tracker_maximum_amplification(f32::INFINITY),
            "Call with positive limit >=1.0 is supposed to succeed"
        );

        // XML_SetAllocTrackerActivationThreshold, success cases
        assert!(
            parser_without_parent.set_alloc_tracker_activation_threshold(123),
            "Call with non-NULL parentless parser is supposed to succeed"
        );
    });
}

/// The port's own memory limit (owner decision 2026-10-08, I-171): with the tracker's
/// threshold at 0, the first structure a document makes the parser charge is refused, and
/// the parse fails with `XML_ERROR_NO_MEMORY`, where expat's `MALLOC` would have failed.
#[test]
fn documents_past_the_memory_limit_are_refused() {
    each_run(|run| {
        let docs: [&[u8]; 4] = [
            b"<e/>",
            b"<e a='1'>text</e>",
            b"<!DOCTYPE e [<!ENTITY x 'y'>]><e/>",
            b"<!DOCTYPE e [<!ATTLIST e a CDATA 'v'>]><e/>",
        ];
        for doc in docs {
            let mut parser = run.parser();
            assert!(parser.set_alloc_tracker_activation_threshold(0));
            assert_eq!(
                run.parse(&mut parser, doc, true),
                XmlStatus::Error,
                "{run:?}"
            );
            assert_eq!(parser.error_code(), XmlError::NoMemory, "{run:?}");
            assert_eq!(xml_error_string(parser.error_code()), Some("out of memory"));
        }
    });
}
