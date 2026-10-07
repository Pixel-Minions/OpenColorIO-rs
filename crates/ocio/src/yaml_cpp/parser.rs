// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.8.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! Port of `YAML::Parser` (include/yaml-cpp/parser.h, src/parser.cpp, yaml-cpp 0.8.0): the
//! directives, then one document at a time through `SingleDocParser`.
//! `PrintTokens`, a debugging aid OCIO doesn't call, is not ported.

use super::event_handler::EventHandler;
use super::exceptions::{Exception, Result, error_msg};
use super::scanner::Scanner;
use super::single_doc_parser::SingleDocParser;
use super::tag::Directives;
use super::token::{Token, TokenType};
use ocio_ops::utils::num_get::{self, Basefield};

/// The stack a document is parsed on. `SingleDocParser` recurses once per nested node, up to
/// its `DepthGuard<500>`: at opt-level 0, where frames are largest, the deepest documents it
/// parses took at most 3.9 MiB (499 nested flow maps; 3.3 to 3.6 MiB for flow and block
/// sequences, block maps and compact maps). Twice that leaves room for the caller's own stack
/// not to matter, as the wheel parses on its caller's.
const PARSER_STACK: usize = 8 * 1024 * 1024;

/// Runs `parse` on a thread of [`PARSER_STACK`]: a document nested as deep as yaml-cpp allows
/// parses whatever the caller's stack (a program's main thread on Windows has 1 MiB). A thread
/// that can't be had is `std::bad_alloc`, what upstream throws when memory runs out.
fn on_parser_stack<T: Send>(parse: impl FnOnce() -> Result<T> + Send) -> Result<T> {
    std::thread::scope(|scope| {
        match std::thread::Builder::new()
            .stack_size(PARSER_STACK)
            .spawn_scoped(scope, parse)
        {
            Ok(handle) => handle
                .join()
                .unwrap_or_else(|panic| std::panic::resume_unwind(panic)),
            Err(_) => Err(Exception::bad_alloc()),
        }
    })
}

/// Port of `YAML::Parser` (parser.h:24-86).
#[derive(Debug, Default)]
pub struct Parser<'a> {
    scanner: Option<Scanner<'a>>,
    directives: Option<Directives>,
}

impl<'a> Parser<'a> {
    /// `Parser()` (parser.cpp:13): a parser with no input.
    pub fn empty() -> Parser<'a> {
        Parser {
            scanner: None,
            directives: None,
        }
    }

    /// `Parser(std::istream&)` (parser.cpp:15).
    pub fn new(input: &'a [u8]) -> Parser<'a> {
        let mut parser = Parser::empty();
        parser.load(input);
        parser
    }

    /// `operator bool` (parser.cpp:19-21): whether there is input with tokens left.
    pub fn is_valid(&mut self) -> Result<bool> {
        match &mut self.scanner {
            Some(scanner) => Ok(!scanner.empty()?),
            None => Ok(false),
        }
    }

    /// `Load(std::istream&)` (parser.cpp:23-26): resets the parser to new input.
    pub fn load(&mut self, input: &'a [u8]) {
        self.scanner = Some(Scanner::new(input));
        self.directives = Some(Directives::default());
    }

    /// `HandleNextDocument(EventHandler&)` (parser.cpp:28-41): reports the next document's
    /// events; false when there are no more documents.
    pub fn handle_next_document(&mut self, event_handler: &mut dyn EventHandler) -> Result<bool> {
        if self.scanner.is_none() {
            return Ok(false);
        }

        self.parse_directives()?;
        let (Some(scanner), Some(directives)) = (&mut self.scanner, &self.directives) else {
            return Ok(false);
        };
        if scanner.empty()? {
            return Ok(false);
        }

        let mut sdp = SingleDocParser::new(scanner, directives);
        on_parser_stack(|| sdp.handle_document(event_handler))?;
        Ok(true)
    }

    /// `ParseDirectives()` (parser.cpp:43-62): the directives before a document. Directives
    /// carry over from the last document unless the document has its own.
    fn parse_directives(&mut self) -> Result<()> {
        let mut read_directive = false;

        loop {
            let Some(scanner) = &mut self.scanner else {
                return Ok(());
            };
            if scanner.empty()? {
                break;
            }
            let token = scanner.peek()?;
            if token.ty != TokenType::Directive {
                break;
            }
            let token = token.clone();

            // we keep the directives from the last document if none are specified; but if any
            // directives are specific, then we reset them
            if !read_directive {
                self.directives = Some(Directives::default());
            }
            read_directive = true;
            self.handle_directive(&token)?;
            if let Some(scanner) = &mut self.scanner {
                scanner.pop()?;
            }
        }
        Ok(())
    }

    /// `HandleDirective(const Token&)` (parser.cpp:64-70): `%YAML` and `%TAG`; others are
    /// ignored.
    fn handle_directive(&mut self, token: &Token) -> Result<()> {
        if token.value == b"YAML" {
            self.handle_yaml_directive(token)
        } else if token.value == b"TAG" {
            self.handle_tag_directive(token)
        } else {
            Ok(())
        }
    }

    /// `HandleYamlDirective(const Token&)` (parser.cpp:72-95): `%YAML major.minor`, read with
    /// `std::stringstream >> int`, `get()`, `>> int`, which must reach the end.
    fn handle_yaml_directive(&mut self, token: &Token) -> Result<()> {
        if token.params.len() != 1 {
            return Err(Exception::parser(
                token.mark,
                error_msg::YAML_DIRECTIVE_ARGS,
            ));
        }

        let directives = self.directives.get_or_insert_with(Directives::default);
        if !directives.version.is_default {
            return Err(Exception::parser(
                token.mark,
                error_msg::REPEATED_YAML_DIRECTIVE,
            ));
        }

        let mut s = IntStream::new(&token.params[0]);
        directives.version.major = s.extract_int();
        s.get();
        directives.version.minor = s.extract_int();
        if s.fail || !s.peek_is_eof() {
            let msg = [error_msg::YAML_VERSION.as_bytes(), &token.params[0]].concat();
            return Err(Exception::parser(token.mark, msg));
        }

        if directives.version.major > 1 {
            return Err(Exception::parser(token.mark, error_msg::YAML_MAJOR_VERSION));
        }

        directives.version.is_default = false;
        // TODO: warning on major == 1, minor > 2?
        Ok(())
    }

    /// `HandleTagDirective(const Token&)` (parser.cpp:97-109): `%TAG handle prefix`.
    fn handle_tag_directive(&mut self, token: &Token) -> Result<()> {
        if token.params.len() != 2 {
            return Err(Exception::parser(token.mark, error_msg::TAG_DIRECTIVE_ARGS));
        }

        let handle = &token.params[0];
        let prefix = &token.params[1];
        let directives = self.directives.get_or_insert_with(Directives::default);
        if directives.tags.contains_key(handle) {
            return Err(Exception::parser(
                token.mark,
                error_msg::REPEATED_TAG_DIRECTIVE,
            ));
        }

        directives.tags.insert(handle.clone(), prefix.clone());
        Ok(())
    }
}

/// The `std::stringstream` `HandleYamlDirective` reads the version from, in the "C" locale:
/// the `eofbit` and `failbit` states and `>> int`, `get()` and `peek()`.
#[derive(Debug)]
struct IntStream<'s> {
    s: &'s [u8],
    pos: usize,
    eof: bool,
    fail: bool,
}

impl<'s> IntStream<'s> {
    fn new(s: &'s [u8]) -> IntStream<'s> {
        IntStream {
            s,
            pos: 0,
            eof: false,
            fail: false,
        }
    }

    fn good(&self) -> bool {
        !self.eof && !self.fail
    }

    /// `operator>>(int&)` ([istream.formatted.arithmetic]): the sentry skips white space,
    /// then `num_get` reads a decimal integer ([`num_get::get_integer`]). Both wheels read
    /// through `long` and check the range of `int`.
    fn extract_int(&mut self) -> i32 {
        // the sentry
        if !self.good() {
            self.fail = true;
            return 0;
        }
        while self
            .s
            .get(self.pos)
            .is_some_and(|&c| num_get::is_c_space(c))
        {
            self.pos += 1;
        }
        if self.pos >= self.s.len() {
            self.eof = true;
            self.fail = true;
            return 0;
        }

        let extracted = num_get::get_integer(
            &self.s[self.pos..],
            Basefield::Dec,
            i128::from(i32::MIN),
            i128::from(i32::MAX),
        );
        self.pos += extracted.consumed;
        self.eof |= extracted.eof;
        self.fail |= extracted.fail;
        extracted.value as i32
    }

    /// `get()`: one character; at the end, `eofbit` and `failbit`.
    fn get(&mut self) {
        if !self.good() {
            self.fail = true;
            return;
        }
        if self.pos < self.s.len() {
            self.pos += 1;
        } else {
            self.eof = true;
            self.fail = true;
        }
    }

    /// `peek() == EOF`: a stream that isn't good, or has no character left, peeks EOF.
    fn peek_is_eof(&mut self) -> bool {
        if !self.good() {
            self.fail = true;
            return true;
        }
        if self.pos >= self.s.len() {
            self.eof = true;
            return true;
        }
        false
    }
}

#[cfg(test)]
#[path = "parser_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "handler_tests.rs"]
mod handler_tests;

#[cfg(test)]
#[path = "handler_spec_tests.rs"]
mod handler_spec_tests;

#[cfg(test)]
#[path = "encoding_tests.rs"]
pub(crate) mod encoding_tests;
