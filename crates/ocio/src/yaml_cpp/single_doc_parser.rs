// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.8.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! Port of `YAML::SingleDocParser` (src/singledocparser.h, src/singledocparser.cpp), with
//! `CollectionStack` (src/collectionstack.h) and `DepthGuard<500>` (include/yaml-cpp/
//! depthguard.h), yaml-cpp 0.8.0: turns the tokens of one document into events.

use std::collections::BTreeMap;

use super::event_handler::{Anchor, EmitterStyle, EventHandler, NULL_ANCHOR, is_null_string};
use super::exceptions::{Exception, Result, error_msg};
use super::mark::Mark;
use super::scanner::Scanner;
use super::tag::{Directives, Tag};
use super::token::TokenType;

/// `CollectionType::value` (collectionstack.h:14-16).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CollectionType {
    NoCollection,
    BlockMap,
    BlockSeq,
    FlowMap,
    FlowSeq,
    CompactMap,
}

/// `DepthGuard<500>`'s limit (singledocparser.cpp:51): `HandleNode` may nest 499 deep.
const MAX_DEPTH: i32 = 500;

/// Port of `YAML::SingleDocParser` (singledocparser.h:27-63).
#[derive(Debug)]
pub struct SingleDocParser<'s, 'a> {
    depth: i32,
    scanner: &'s mut Scanner<'a>,
    directives: &'s Directives,
    /// `m_pCollectionStack`
    collection_stack: Vec<CollectionType>,
    /// `m_anchors`: anchor names and their numbers.
    anchors: BTreeMap<Vec<u8>, Anchor>,
    cur_anchor: Anchor,
}

impl<'s, 'a> SingleDocParser<'s, 'a> {
    /// `SingleDocParser(Scanner&, const Directives&)` (singledocparser.cpp:18-23).
    pub fn new(scanner: &'s mut Scanner<'a>, directives: &'s Directives) -> Self {
        SingleDocParser {
            depth: 0,
            scanner,
            directives,
            collection_stack: Vec::new(),
            anchors: BTreeMap::new(),
            cur_anchor: 0,
        }
    }

    /// `HandleDocument(EventHandler&)` (singledocparser.cpp:30-48): the next document. The
    /// caller has checked that tokens remain.
    pub fn handle_document(&mut self, event_handler: &mut dyn EventHandler) -> Result<()> {
        let mark = self.scanner.peek()?.mark;
        event_handler.on_document_start(mark);

        // eat doc start
        if self.scanner.peek()?.ty == TokenType::DocStart {
            self.scanner.pop()?;
        }

        // recurse!
        self.handle_node(event_handler)?;

        event_handler.on_document_end();

        // and finally eat any doc ends we see
        while !self.scanner.empty()? && self.scanner.peek()?.ty == TokenType::DocEnd {
            self.scanner.pop()?;
        }
        Ok(())
    }

    /// `HandleNode(EventHandler&)` (singledocparser.cpp:50-149) under its `DepthGuard<500>`:
    /// "bad file" at the scanner's mark when nodes nest 500 deep.
    fn handle_node(&mut self, event_handler: &mut dyn EventHandler) -> Result<()> {
        let mark = self.scanner.mark();
        self.depth += 1;
        if MAX_DEPTH <= self.depth {
            return Err(Exception::deep_recursion(
                self.depth,
                mark,
                error_msg::BAD_FILE,
            ));
        }
        let result = self.handle_node_body(event_handler);
        self.depth -= 1;
        result
    }

    /// The body of `HandleNode` (singledocparser.cpp:53-148).
    fn handle_node_body(&mut self, event_handler: &mut dyn EventHandler) -> Result<()> {
        // an empty node *is* a possibility
        if self.scanner.empty()? {
            event_handler.on_null(self.scanner.mark(), NULL_ANCHOR);
            return Ok(());
        }

        // save location
        let mark = self.scanner.peek()?.mark;

        // special case: a value node by itself must be a map, with no header
        if self.scanner.peek()?.ty == TokenType::Value {
            event_handler.on_map_start(mark, b"?", NULL_ANCHOR, EmitterStyle::Default);
            self.handle_map(event_handler)?;
            event_handler.on_map_end();
            return Ok(());
        }

        // special case: an alias node
        if self.scanner.peek()?.ty == TokenType::Alias {
            let name = self.scanner.peek()?.value.clone();
            event_handler.on_alias(mark, self.lookup_anchor(mark, &name)?);
            self.scanner.pop()?;
            return Ok(());
        }

        let mut tag = Vec::new();
        let mut anchor_name = Vec::new();
        let mut anchor = NULL_ANCHOR;
        self.parse_properties(&mut tag, &mut anchor, &mut anchor_name)?;

        if !anchor_name.is_empty() {
            event_handler.on_anchor(mark, &anchor_name);
        }

        // after parsing properties, an empty node is again a possibility
        if self.scanner.empty()? {
            event_handler.on_null(mark, anchor);
            return Ok(());
        }

        let token = self.scanner.peek()?;
        let token_type = token.ty;

        // add non-specific tags
        if tag.is_empty() {
            tag = if token_type == TokenType::NonPlainScalar {
                b"!".to_vec()
            } else {
                b"?".to_vec()
            };
        }

        if token_type == TokenType::PlainScalar && tag == b"?" && is_null_string(&token.value) {
            event_handler.on_null(mark, anchor);
            self.scanner.pop()?;
            return Ok(());
        }

        // now split based on what kind of node we should be
        match token_type {
            TokenType::PlainScalar | TokenType::NonPlainScalar => {
                event_handler.on_scalar(mark, &tag, anchor, &token.value);
                self.scanner.pop()?;
                return Ok(());
            }
            TokenType::FlowSeqStart => {
                event_handler.on_sequence_start(mark, &tag, anchor, EmitterStyle::Flow);
                self.handle_sequence(event_handler)?;
                event_handler.on_sequence_end();
                return Ok(());
            }
            TokenType::BlockSeqStart => {
                event_handler.on_sequence_start(mark, &tag, anchor, EmitterStyle::Block);
                self.handle_sequence(event_handler)?;
                event_handler.on_sequence_end();
                return Ok(());
            }
            TokenType::FlowMapStart => {
                event_handler.on_map_start(mark, &tag, anchor, EmitterStyle::Flow);
                self.handle_map(event_handler)?;
                event_handler.on_map_end();
                return Ok(());
            }
            TokenType::BlockMapStart => {
                event_handler.on_map_start(mark, &tag, anchor, EmitterStyle::Block);
                self.handle_map(event_handler)?;
                event_handler.on_map_end();
                return Ok(());
            }
            // compact maps can only go in a flow sequence
            TokenType::Key if self.get_cur_collection_type() == CollectionType::FlowSeq => {
                event_handler.on_map_start(mark, &tag, anchor, EmitterStyle::Flow);
                self.handle_map(event_handler)?;
                event_handler.on_map_end();
                return Ok(());
            }
            _ => {}
        }

        if tag == b"?" {
            event_handler.on_null(mark, anchor);
        } else {
            event_handler.on_scalar(mark, &tag, anchor, b"");
        }
        Ok(())
    }

    /// `HandleSequence(EventHandler&)` (singledocparser.cpp:151-163).
    fn handle_sequence(&mut self, event_handler: &mut dyn EventHandler) -> Result<()> {
        // split based on start token
        match self.scanner.peek()?.ty {
            TokenType::BlockSeqStart => self.handle_block_sequence(event_handler),
            TokenType::FlowSeqStart => self.handle_flow_sequence(event_handler),
            _ => Ok(()),
        }
    }

    /// `HandleBlockSequence(EventHandler&)` (singledocparser.cpp:165-196).
    fn handle_block_sequence(&mut self, event_handler: &mut dyn EventHandler) -> Result<()> {
        // eat start token
        self.scanner.pop()?;
        self.push_collection_type(CollectionType::BlockSeq);

        loop {
            if self.scanner.empty()? {
                return Err(Exception::parser(
                    self.scanner.mark(),
                    error_msg::END_OF_SEQ,
                ));
            }

            let token = self.scanner.peek()?;
            let (token_type, token_mark) = (token.ty, token.mark);
            if token_type != TokenType::BlockEntry && token_type != TokenType::BlockSeqEnd {
                return Err(Exception::parser(token_mark, error_msg::END_OF_SEQ));
            }

            self.scanner.pop()?;
            if token_type == TokenType::BlockSeqEnd {
                break;
            }

            // check for null
            if !self.scanner.empty()? {
                let next_token = self.scanner.peek()?;
                if next_token.ty == TokenType::BlockEntry || next_token.ty == TokenType::BlockSeqEnd
                {
                    event_handler.on_null(next_token.mark, NULL_ANCHOR);
                    continue;
                }
            }

            self.handle_node(event_handler)?;
        }

        self.pop_collection_type(CollectionType::BlockSeq);
        Ok(())
    }

    /// `HandleFlowSequence(EventHandler&)` (singledocparser.cpp:198-229).
    fn handle_flow_sequence(&mut self, event_handler: &mut dyn EventHandler) -> Result<()> {
        // eat start token
        self.scanner.pop()?;
        self.push_collection_type(CollectionType::FlowSeq);

        loop {
            if self.scanner.empty()? {
                return Err(Exception::parser(
                    self.scanner.mark(),
                    error_msg::END_OF_SEQ_FLOW,
                ));
            }

            // first check for end
            if self.scanner.peek()?.ty == TokenType::FlowSeqEnd {
                self.scanner.pop()?;
                break;
            }

            // then read the node
            self.handle_node(event_handler)?;

            if self.scanner.empty()? {
                return Err(Exception::parser(
                    self.scanner.mark(),
                    error_msg::END_OF_SEQ_FLOW,
                ));
            }

            // now eat the separator (or could be a sequence end, which we ignore - but if it's
            // neither, then it's a bad node)
            let token = self.scanner.peek()?;
            if token.ty == TokenType::FlowEntry {
                self.scanner.pop()?;
            } else if token.ty != TokenType::FlowSeqEnd {
                return Err(Exception::parser(token.mark, error_msg::END_OF_SEQ_FLOW));
            }
        }

        self.pop_collection_type(CollectionType::FlowSeq);
        Ok(())
    }

    /// `HandleMap(EventHandler&)` (singledocparser.cpp:231-249).
    fn handle_map(&mut self, event_handler: &mut dyn EventHandler) -> Result<()> {
        // split based on start token
        match self.scanner.peek()?.ty {
            TokenType::BlockMapStart => self.handle_block_map(event_handler),
            TokenType::FlowMapStart => self.handle_flow_map(event_handler),
            TokenType::Key => self.handle_compact_map(event_handler),
            TokenType::Value => self.handle_compact_map_with_no_key(event_handler),
            _ => Ok(()),
        }
    }

    /// `HandleBlockMap(EventHandler&)` (singledocparser.cpp:251-288).
    fn handle_block_map(&mut self, event_handler: &mut dyn EventHandler) -> Result<()> {
        // eat start token
        self.scanner.pop()?;
        self.push_collection_type(CollectionType::BlockMap);

        loop {
            if self.scanner.empty()? {
                return Err(Exception::parser(
                    self.scanner.mark(),
                    error_msg::END_OF_MAP,
                ));
            }

            let token = self.scanner.peek()?;
            let (token_type, token_mark) = (token.ty, token.mark);
            if token_type != TokenType::Key
                && token_type != TokenType::Value
                && token_type != TokenType::BlockMapEnd
            {
                return Err(Exception::parser(token_mark, error_msg::END_OF_MAP));
            }

            if token_type == TokenType::BlockMapEnd {
                self.scanner.pop()?;
                break;
            }

            // grab key (if non-null)
            if token_type == TokenType::Key {
                self.scanner.pop()?;
                self.handle_node(event_handler)?;
            } else {
                event_handler.on_null(token_mark, NULL_ANCHOR);
            }

            // now grab value (optional)
            if !self.scanner.empty()? && self.scanner.peek()?.ty == TokenType::Value {
                self.scanner.pop()?;
                self.handle_node(event_handler)?;
            } else {
                event_handler.on_null(token_mark, NULL_ANCHOR);
            }
        }

        self.pop_collection_type(CollectionType::BlockMap);
        Ok(())
    }

    /// `HandleFlowMap(EventHandler&)` (singledocparser.cpp:290-336).
    fn handle_flow_map(&mut self, event_handler: &mut dyn EventHandler) -> Result<()> {
        // eat start token
        self.scanner.pop()?;
        self.push_collection_type(CollectionType::FlowMap);

        loop {
            if self.scanner.empty()? {
                return Err(Exception::parser(
                    self.scanner.mark(),
                    error_msg::END_OF_MAP_FLOW,
                ));
            }

            let token = self.scanner.peek()?;
            let (token_type, mark) = (token.ty, token.mark);
            // first check for end
            if token_type == TokenType::FlowMapEnd {
                self.scanner.pop()?;
                break;
            }

            // grab key (if non-null)
            if token_type == TokenType::Key {
                self.scanner.pop()?;
                self.handle_node(event_handler)?;
            } else {
                event_handler.on_null(mark, NULL_ANCHOR);
            }

            // now grab value (optional)
            if !self.scanner.empty()? && self.scanner.peek()?.ty == TokenType::Value {
                self.scanner.pop()?;
                self.handle_node(event_handler)?;
            } else {
                event_handler.on_null(mark, NULL_ANCHOR);
            }

            if self.scanner.empty()? {
                return Err(Exception::parser(
                    self.scanner.mark(),
                    error_msg::END_OF_MAP_FLOW,
                ));
            }

            // now eat the separator (or could be a map end, which we ignore - but if it's
            // neither, then it's a bad node)
            let next_token = self.scanner.peek()?;
            if next_token.ty == TokenType::FlowEntry {
                self.scanner.pop()?;
            } else if next_token.ty != TokenType::FlowMapEnd {
                return Err(Exception::parser(
                    next_token.mark,
                    error_msg::END_OF_MAP_FLOW,
                ));
            }
        }

        self.pop_collection_type(CollectionType::FlowMap);
        Ok(())
    }

    /// `HandleCompactMap(EventHandler&)` (singledocparser.cpp:338-356): a single
    /// "key: value" pair in a flow sequence.
    fn handle_compact_map(&mut self, event_handler: &mut dyn EventHandler) -> Result<()> {
        self.push_collection_type(CollectionType::CompactMap);

        // grab key
        let mark = self.scanner.peek()?.mark;
        self.scanner.pop()?;
        self.handle_node(event_handler)?;

        // now grab value (optional)
        if !self.scanner.empty()? && self.scanner.peek()?.ty == TokenType::Value {
            self.scanner.pop()?;
            self.handle_node(event_handler)?;
        } else {
            event_handler.on_null(mark, NULL_ANCHOR);
        }

        self.pop_collection_type(CollectionType::CompactMap);
        Ok(())
    }

    /// `HandleCompactMapWithNoKey(EventHandler&)` (singledocparser.cpp:358-370): a single
    /// ": value" pair in a flow sequence.
    fn handle_compact_map_with_no_key(
        &mut self,
        event_handler: &mut dyn EventHandler,
    ) -> Result<()> {
        self.push_collection_type(CollectionType::CompactMap);

        // null key
        let mark = self.scanner.peek()?.mark;
        event_handler.on_null(mark, NULL_ANCHOR);

        // grab value
        self.scanner.pop()?;
        self.handle_node(event_handler)?;

        self.pop_collection_type(CollectionType::CompactMap);
        Ok(())
    }

    /// `ParseProperties(std::string& tag, anchor_t& anchor, std::string& anchor_name)`
    /// (singledocparser.cpp:372-395): any tag and anchor tokens before a node.
    fn parse_properties(
        &mut self,
        tag: &mut Vec<u8>,
        anchor: &mut Anchor,
        anchor_name: &mut Vec<u8>,
    ) -> Result<()> {
        tag.clear();
        anchor_name.clear();
        *anchor = NULL_ANCHOR;

        loop {
            if self.scanner.empty()? {
                return Ok(());
            }

            match self.scanner.peek()?.ty {
                TokenType::Tag => self.parse_tag(tag)?,
                TokenType::Anchor => self.parse_anchor(anchor, anchor_name)?,
                _ => return Ok(()),
            }
        }
    }

    /// `ParseTag(std::string& tag)` (singledocparser.cpp:397-405).
    fn parse_tag(&mut self, tag: &mut Vec<u8>) -> Result<()> {
        let token = self.scanner.peek()?;
        if !tag.is_empty() {
            return Err(Exception::parser(token.mark, error_msg::MULTIPLE_TAGS));
        }

        let tag_info = Tag::new(token);
        *tag = tag_info.translate(self.directives);
        self.scanner.pop()
    }

    /// `ParseAnchor(anchor_t& anchor, std::string& anchor_name)` (singledocparser.cpp:
    /// 407-415).
    fn parse_anchor(&mut self, anchor: &mut Anchor, anchor_name: &mut Vec<u8>) -> Result<()> {
        let token = self.scanner.peek()?;
        if *anchor != NULL_ANCHOR {
            return Err(Exception::parser(token.mark, error_msg::MULTIPLE_ANCHORS));
        }

        *anchor_name = token.value.clone();
        *anchor = self.register_anchor(anchor_name);
        self.scanner.pop()
    }

    /// `RegisterAnchor(const std::string&)` (singledocparser.cpp:417-422): numbers the anchor
    /// (a name defined again gets a new number).
    fn register_anchor(&mut self, name: &[u8]) -> Anchor {
        if name.is_empty() {
            return NULL_ANCHOR;
        }

        self.cur_anchor += 1;
        self.anchors.insert(name.to_vec(), self.cur_anchor);
        self.cur_anchor
    }

    /// `LookupAnchor(const Mark&, const std::string&)` (singledocparser.cpp:424-434).
    fn lookup_anchor(&self, mark: Mark, name: &[u8]) -> Result<Anchor> {
        match self.anchors.get(name) {
            Some(&anchor) => Ok(anchor),
            None => {
                let msg = [error_msg::UNKNOWN_ANCHOR.as_bytes(), name].concat();
                Err(Exception::parser(mark, msg))
            }
        }
    }

    /// `CollectionStack::GetCurCollectionType()` (collectionstack.h:21-25).
    fn get_cur_collection_type(&self) -> CollectionType {
        self.collection_stack
            .last()
            .copied()
            .unwrap_or(CollectionType::NoCollection)
    }

    /// `CollectionStack::PushCollectionType` (collectionstack.h:26-28).
    fn push_collection_type(&mut self, ty: CollectionType) {
        self.collection_stack.push(ty);
    }

    /// `CollectionStack::PopCollectionType` (collectionstack.h:29-33). yaml-cpp asserts that
    /// the type is the top one, which the parser's structure guarantees.
    fn pop_collection_type(&mut self, ty: CollectionType) {
        debug_assert_eq!(ty, self.get_cur_collection_type());
        self.collection_stack.pop();
    }
}
