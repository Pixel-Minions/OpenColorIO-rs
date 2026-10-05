// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.8.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! Port of yaml-cpp 0.8.0's `HandlerTest` fixture (test/handler_test.h) and
//! `MockEventHandler` (test/mock_event_handler.h). The C++ tests set gmock expectations in
//! sequence on a `StrictMock` handler, every mark matched by `_`; here the handler records
//! the events without their marks, and the test compares the whole list.

use std::fmt;

use super::event_handler::{Anchor, EmitterStyle, EventHandler};
use super::exceptions::Result;
use super::mark::Mark;
use super::parser::Parser;

/// Bytes, shown as a byte string.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Bytes(pub(crate) Vec<u8>);

impl fmt::Debug for Bytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "b\"{}\"", self.0.escape_ascii())
    }
}

impl From<&[u8]> for Bytes {
    fn from(b: &[u8]) -> Bytes {
        Bytes(b.to_vec())
    }
}

impl<const N: usize> From<&[u8; N]> for Bytes {
    fn from(b: &[u8; N]) -> Bytes {
        Bytes(b.to_vec())
    }
}

impl From<Vec<u8>> for Bytes {
    fn from(b: Vec<u8>) -> Bytes {
        Bytes(b)
    }
}

/// One `MockEventHandler` call, without its mark.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Event {
    DocumentStart,
    DocumentEnd,
    Null(Anchor),
    Alias(Anchor),
    Scalar(Bytes, Anchor, Bytes),
    SequenceStart(Bytes, Anchor, EmitterStyle),
    SequenceEnd,
    MapStart(Bytes, Anchor, EmitterStyle),
    MapEnd,
    Anchor(Bytes),
}

/// `OnDocumentStart(_)`
pub(crate) fn doc_start() -> Event {
    Event::DocumentStart
}

/// `OnDocumentEnd()`
pub(crate) fn doc_end() -> Event {
    Event::DocumentEnd
}

/// `OnNull(_, anchor)`
pub(crate) fn null(anchor: Anchor) -> Event {
    Event::Null(anchor)
}

/// `OnAlias(_, anchor)`
pub(crate) fn alias(anchor: Anchor) -> Event {
    Event::Alias(anchor)
}

/// `OnScalar(_, tag, anchor, value)`
pub(crate) fn scalar(tag: impl AsRef<[u8]>, anchor: Anchor, value: impl AsRef<[u8]>) -> Event {
    Event::Scalar(
        Bytes(tag.as_ref().to_vec()),
        anchor,
        Bytes(value.as_ref().to_vec()),
    )
}

/// `OnSequenceStart(_, tag, anchor, style)`
pub(crate) fn seq_start(tag: impl AsRef<[u8]>, anchor: Anchor, style: EmitterStyle) -> Event {
    Event::SequenceStart(Bytes(tag.as_ref().to_vec()), anchor, style)
}

/// `OnSequenceEnd()`
pub(crate) fn seq_end() -> Event {
    Event::SequenceEnd
}

/// `OnMapStart(_, tag, anchor, style)`
pub(crate) fn map_start(tag: impl AsRef<[u8]>, anchor: Anchor, style: EmitterStyle) -> Event {
    Event::MapStart(Bytes(tag.as_ref().to_vec()), anchor, style)
}

/// `OnMapEnd()`
pub(crate) fn map_end() -> Event {
    Event::MapEnd
}

/// `OnAnchor(_, name)`
pub(crate) fn anchor(name: impl AsRef<[u8]>) -> Event {
    Event::Anchor(Bytes(name.as_ref().to_vec()))
}

/// A handler that records the events (the `StrictMock` and `NiceMock` handlers).
#[derive(Debug, Default)]
pub(crate) struct RecordingHandler {
    pub(crate) events: Vec<Event>,
}

impl EventHandler for RecordingHandler {
    fn on_document_start(&mut self, _mark: Mark) {
        self.events.push(Event::DocumentStart);
    }

    fn on_document_end(&mut self) {
        self.events.push(Event::DocumentEnd);
    }

    fn on_null(&mut self, _mark: Mark, anchor: Anchor) {
        self.events.push(Event::Null(anchor));
    }

    fn on_alias(&mut self, _mark: Mark, anchor: Anchor) {
        self.events.push(Event::Alias(anchor));
    }

    fn on_scalar(&mut self, _mark: Mark, tag: &[u8], anchor: Anchor, value: &[u8]) {
        self.events.push(scalar(tag, anchor, value));
    }

    fn on_sequence_start(&mut self, _mark: Mark, tag: &[u8], anchor: Anchor, style: EmitterStyle) {
        self.events.push(seq_start(tag, anchor, style));
    }

    fn on_sequence_end(&mut self) {
        self.events.push(Event::SequenceEnd);
    }

    fn on_map_start(&mut self, _mark: Mark, tag: &[u8], anchor: Anchor, style: EmitterStyle) {
        self.events.push(map_start(tag, anchor, style));
    }

    fn on_map_end(&mut self) {
        self.events.push(Event::MapEnd);
    }

    fn on_anchor(&mut self, _mark: Mark, anchor_name: &[u8]) {
        self.events.push(anchor(anchor_name));
    }
}

/// `HandlerTest::Parse` and `IgnoreParse` (handler_test.h:17-31): every document, in order.
pub(crate) fn parse(example: &[u8], handler: &mut dyn EventHandler) -> Result<()> {
    let mut parser = Parser::new(example);
    while parser.handle_next_document(handler)? {}
    Ok(())
}

/// The expectations of a `HandlerTest` (`EXPECT_CALL`s `InSequence` on the `StrictMock`
/// handler), then `Parse(example)`: exactly these events, in this order, and no exception.
#[track_caller]
pub(crate) fn expect_events(example: &[u8], expected: &[Event]) {
    let mut handler = RecordingHandler::default();
    if let Err(e) = parse(example, &mut handler) {
        panic!("Parse threw: {e}; events so far: {:#?}", handler.events);
    }
    assert_eq!(handler.events, expected);
}

/// `EXPECT_THROW_PARSER_EXCEPTION(IgnoreParse(example), message)`: a `ParserException` with
/// this `msg`.
#[track_caller]
pub(crate) fn expect_parser_exception(example: &[u8], message: impl AsRef<[u8]>) {
    let mut handler = RecordingHandler::default();
    match parse(example, &mut handler) {
        Ok(()) => panic!("expected a ParserException, parsed: {:#?}", handler.events),
        Err(e) => {
            assert!(e.is_parser_exception(), "not a ParserException: {e:?}");
            assert_eq!(Bytes(e.msg), Bytes(message.as_ref().to_vec()));
        }
    }
}
