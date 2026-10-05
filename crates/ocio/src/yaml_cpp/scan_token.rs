// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.8.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! Port of the scanner's token scanners (src/scantoken.cpp, yaml-cpp 0.8.0).

use super::exceptions::{Exception, Result, error_msg};
use super::exp::{self, keys};
use super::regex_yaml::RegEx;
use super::scan_scalar::{Action, Chomp, Fold, ScanScalarParams, scan_scalar};
use super::scan_tag::{scan_tag_handle, scan_tag_suffix, scan_verbatim_tag};
use super::scanner::{FlowMarker, IndentType, Scanner};
use super::tag::TagType;
use super::token::{Token, TokenType};

impl Scanner<'_> {
    /// `ScanDirective()` (scantoken.cpp:20-58): `%NAME param...`, with no semantic checking
    /// (that's for the parser).
    pub(super) fn scan_directive(&mut self) {
        // pop indents and simple keys
        self.pop_all_indents();
        self.pop_all_simple_keys();

        self.simple_key_allowed = false;
        self.can_be_json_flow = false;

        // store pos and eat indicator
        let mut token = Token::new(TokenType::Directive, self.input.mark());
        self.input.eat(1);

        // read name
        while self.input.is_valid() && !exp::BLANK_OR_BREAK.matches_stream(&self.input) {
            token.value.push(self.input.get());
        }

        // read parameters
        loop {
            // first get rid of whitespace
            while exp::BLANK.matches_stream(&self.input) {
                self.input.eat(1);
            }

            // break on newline or comment
            if !self.input.is_valid()
                || exp::BREAK.matches_stream(&self.input)
                || exp::COMMENT.matches_stream(&self.input)
            {
                break;
            }

            // now read parameter
            let mut param = Vec::new();
            while self.input.is_valid() && !exp::BLANK_OR_BREAK.matches_stream(&self.input) {
                param.push(self.input.get());
            }

            token.params.push(param);
        }

        self.push(token);
    }

    /// `ScanDocStart()` (scantoken.cpp:60-71): `---`.
    pub(super) fn scan_doc_start(&mut self) {
        self.pop_all_indents();
        self.pop_all_simple_keys();
        self.simple_key_allowed = false;
        self.can_be_json_flow = false;

        // eat
        let mark = self.input.mark();
        self.input.eat(3);
        self.push(Token::new(TokenType::DocStart, mark));
    }

    /// `ScanDocEnd()` (scantoken.cpp:73-84): `...`.
    pub(super) fn scan_doc_end(&mut self) {
        self.pop_all_indents();
        self.pop_all_simple_keys();
        self.simple_key_allowed = false;
        self.can_be_json_flow = false;

        // eat
        let mark = self.input.mark();
        self.input.eat(3);
        self.push(Token::new(TokenType::DocEnd, mark));
    }

    /// `ScanFlowStart()` (scantoken.cpp:86-101): `[` or `{`, which can start a simple key.
    pub(super) fn scan_flow_start(&mut self) {
        // flows can be simple keys
        self.insert_potential_simple_key();
        self.simple_key_allowed = true;
        self.can_be_json_flow = false;

        // eat
        let mark = self.input.mark();
        let ch = self.input.get();
        let flow_type = if ch == keys::FLOW_SEQ_START {
            FlowMarker::FlowSeq
        } else {
            FlowMarker::FlowMap
        };
        self.flows.push(flow_type);
        let ty = if flow_type == FlowMarker::FlowSeq {
            TokenType::FlowSeqStart
        } else {
            TokenType::FlowMapStart
        };
        self.push(Token::new(ty, mark));
    }

    /// `ScanFlowEnd()` (scantoken.cpp:103-131): `]` or `}`, which must close the innermost
    /// flow of the same kind.
    pub(super) fn scan_flow_end(&mut self) -> Result<()> {
        if self.in_block_context() {
            return Err(Exception::parser(self.input.mark(), error_msg::FLOW_END));
        }

        // we might have a solo entry in the flow context
        if self.in_flow_context() {
            if self.flows.last() == Some(&FlowMarker::FlowMap) && self.verify_simple_key() {
                let mark = self.input.mark();
                self.push(Token::new(TokenType::Value, mark));
            } else if self.flows.last() == Some(&FlowMarker::FlowSeq) {
                self.invalidate_simple_key();
            }
        }

        self.simple_key_allowed = false;
        self.can_be_json_flow = true;

        // eat
        let mark = self.input.mark();
        let ch = self.input.get();

        // check that it matches the start
        let flow_type = if ch == keys::FLOW_SEQ_END {
            FlowMarker::FlowSeq
        } else {
            FlowMarker::FlowMap
        };
        if self.flows.last() != Some(&flow_type) {
            return Err(Exception::parser(mark, error_msg::FLOW_END));
        }
        self.flows.pop();

        // `flowType ? FLOW_SEQ_END : FLOW_MAP_END`, with FLOW_MAP = 0 and FLOW_SEQ = 1.
        let ty = if flow_type == FlowMarker::FlowSeq {
            TokenType::FlowSeqEnd
        } else {
            TokenType::FlowMapEnd
        };
        self.push(Token::new(ty, mark));
        Ok(())
    }

    /// `ScanFlowEntry()` (scantoken.cpp:133-150): `,`.
    pub(super) fn scan_flow_entry(&mut self) {
        // we might have a solo entry in the flow context
        if self.in_flow_context() {
            if self.flows.last() == Some(&FlowMarker::FlowMap) && self.verify_simple_key() {
                let mark = self.input.mark();
                self.push(Token::new(TokenType::Value, mark));
            } else if self.flows.last() == Some(&FlowMarker::FlowSeq) {
                self.invalidate_simple_key();
            }
        }

        self.simple_key_allowed = true;
        self.can_be_json_flow = false;

        // eat
        let mark = self.input.mark();
        self.input.eat(1);
        self.push(Token::new(TokenType::FlowEntry, mark));
    }

    /// `ScanBlockEntry()` (scantoken.cpp:152-170): `- `, which opens a block sequence at its
    /// column.
    pub(super) fn scan_block_entry(&mut self) -> Result<()> {
        // we better be in the block context!
        if self.in_flow_context() {
            return Err(Exception::parser(self.input.mark(), error_msg::BLOCK_ENTRY));
        }

        // can we put it here?
        if !self.simple_key_allowed {
            return Err(Exception::parser(self.input.mark(), error_msg::BLOCK_ENTRY));
        }

        self.push_indent_to(self.input.column(), IndentType::Seq);
        self.simple_key_allowed = true;
        self.can_be_json_flow = false;

        // eat
        let mark = self.input.mark();
        self.input.eat(1);
        self.push(Token::new(TokenType::BlockEntry, mark));
        Ok(())
    }

    /// `ScanKey()` (scantoken.cpp:172-189): `? `.
    pub(super) fn scan_key(&mut self) -> Result<()> {
        // handle keys differently in the block context (and manage indents)
        if self.in_block_context() {
            if !self.simple_key_allowed {
                return Err(Exception::parser(self.input.mark(), error_msg::MAP_KEY));
            }

            self.push_indent_to(self.input.column(), IndentType::Map);
        }

        // can only put a simple key here if we're in block context
        self.simple_key_allowed = self.in_block_context();

        // eat
        let mark = self.input.mark();
        self.input.eat(1);
        self.push(Token::new(TokenType::Key, mark));
        Ok(())
    }

    /// `ScanValue()` (scantoken.cpp:191-218): `:`, which validates the pending simple key, or
    /// stands alone.
    pub(super) fn scan_value(&mut self) -> Result<()> {
        // and check that simple key
        let is_simple_key = self.verify_simple_key();
        self.can_be_json_flow = false;

        if is_simple_key {
            // can't follow a simple key with another simple key (dunno why, though - it
            // seems fine)
            self.simple_key_allowed = false;
        } else {
            // handle values differently in the block context (and manage indents)
            if self.in_block_context() {
                if !self.simple_key_allowed {
                    return Err(Exception::parser(self.input.mark(), error_msg::MAP_VALUE));
                }

                self.push_indent_to(self.input.column(), IndentType::Map);
            }

            // can only put a simple key here if we're in block context
            self.simple_key_allowed = self.in_block_context();
        }

        // eat
        let mark = self.input.mark();
        self.input.eat(1);
        self.push(Token::new(TokenType::Value, mark));
        Ok(())
    }

    /// `ScanAnchorOrAlias()` (scantoken.cpp:220-253): `&name` or `*name`.
    pub(super) fn scan_anchor_or_alias(&mut self) -> Result<()> {
        // insert a potential simple key
        self.insert_potential_simple_key();
        self.simple_key_allowed = false;
        self.can_be_json_flow = false;

        // eat the indicator
        let mark = self.input.mark();
        let indicator = self.input.get();
        let alias = indicator == keys::ALIAS;

        // now eat the content
        let mut name = Vec::new();
        while self.input.is_valid() && exp::ANCHOR.matches_stream(&self.input) {
            name.push(self.input.get());
        }

        // we need to have read SOMETHING!
        if name.is_empty() {
            let msg = if alias {
                error_msg::ALIAS_NOT_FOUND
            } else {
                error_msg::ANCHOR_NOT_FOUND
            };
            return Err(Exception::parser(self.input.mark(), msg));
        }

        // and needs to end correctly
        if self.input.is_valid() && !exp::ANCHOR_END.matches_stream(&self.input) {
            let msg = if alias {
                error_msg::CHAR_IN_ALIAS
            } else {
                error_msg::CHAR_IN_ANCHOR
            };
            return Err(Exception::parser(self.input.mark(), msg));
        }

        // and we're done
        let mut token = Token::new(
            if alias {
                TokenType::Alias
            } else {
                TokenType::Anchor
            },
            mark,
        );
        token.value = name;
        self.push(token);
        Ok(())
    }

    /// `ScanTag()` (scantoken.cpp:255-292): `!<verbatim>`, `!suffix`, `!!suffix`,
    /// `!handle!suffix` or a lone `!`.
    pub(super) fn scan_tag(&mut self) -> Result<()> {
        // insert a potential simple key
        self.insert_potential_simple_key();
        self.simple_key_allowed = false;
        self.can_be_json_flow = false;

        let mut token = Token::new(TokenType::Tag, self.input.mark());

        // eat the indicator
        self.input.get();

        if self.input.is_valid() && self.input.peek() == keys::VERBATIM_TAG_START {
            let tag = scan_verbatim_tag(&mut self.input)?;

            token.value = tag;
            token.data = TagType::Verbatim as i32;
        } else {
            let mut can_be_handle = false;
            token.value = scan_tag_handle(&mut self.input, &mut can_be_handle)?;
            if !can_be_handle && token.value.is_empty() {
                token.data = TagType::NonSpecific as i32;
            } else if token.value.is_empty() {
                token.data = TagType::SecondaryHandle as i32;
            } else {
                token.data = TagType::PrimaryHandle as i32;
            }

            // is there a suffix?
            if can_be_handle && self.input.peek() == keys::TAG {
                // eat the indicator
                self.input.get();
                token.params.push(scan_tag_suffix(&mut self.input)?);
                token.data = TagType::NamedHandle as i32;
            }
        }

        self.push(token);
        Ok(())
    }

    /// `ScanPlainScalar()` (scantoken.cpp:294-328).
    pub(super) fn scan_plain_scalar(&mut self) -> Result<()> {
        // set up the scanning parameters
        let end: &RegEx = if self.in_flow_context() {
            &exp::SCAN_SCALAR_END_IN_FLOW
        } else {
            &exp::SCAN_SCALAR_END
        };
        let mut params = ScanScalarParams {
            end: Some(end),
            eat_end: false,
            indent: if self.in_flow_context() {
                0
            } else {
                self.get_top_indent().wrapping_add(1)
            },
            fold: Fold::FoldFlow,
            eat_leading_whitespace: true,
            trim_trailing_spaces: true,
            chomp: Chomp::Strip,
            on_doc_indicator: Action::Break,
            on_tab_in_indentation: Action::Throw,
            ..ScanScalarParams::default()
        };

        // insert a potential simple key
        self.insert_potential_simple_key();

        let mark = self.input.mark();
        let scalar = scan_scalar(&mut self.input, &mut params)?;

        // can have a simple key only if we ended the scalar by starting a new line
        self.simple_key_allowed = params.leading_spaces;
        self.can_be_json_flow = false;

        // finally, check and see if we ended on an illegal character
        // if(Exp::IllegalCharInScalar.Matches(INPUT))
        //	throw ParserException(INPUT.mark(), ErrorMsg::CHAR_IN_SCALAR);

        let mut token = Token::new(TokenType::PlainScalar, mark);
        token.value = scalar;
        self.push(token);
        Ok(())
    }

    /// `ScanQuotedScalar()` (scantoken.cpp:330-368): single or double quoted.
    pub(super) fn scan_quoted_scalar(&mut self) -> Result<()> {
        // peek at single or double quote (don't eat because we need to preserve (for the
        // time being) the input position)
        let quote = self.input.peek();
        let single = quote == b'\'';

        // setup the scanning parameters
        let end = if single {
            RegEx::ch(quote) & !exp::ESC_SINGLE_QUOTE.clone()
        } else {
            RegEx::ch(quote)
        };
        let mut params = ScanScalarParams {
            end: Some(&end),
            eat_end: true,
            escape: if single { b'\'' } else { b'\\' },
            indent: 0,
            fold: Fold::FoldFlow,
            eat_leading_whitespace: true,
            trim_trailing_spaces: false,
            chomp: Chomp::Clip,
            on_doc_indicator: Action::Throw,
            ..ScanScalarParams::default()
        };

        // insert a potential simple key
        self.insert_potential_simple_key();

        let mark = self.input.mark();

        // now eat that opening quote
        self.input.get();

        // and scan
        let scalar = scan_scalar(&mut self.input, &mut params)?;
        self.simple_key_allowed = false;
        self.can_be_json_flow = true;

        let mut token = Token::new(TokenType::NonPlainScalar, mark);
        token.value = scalar;
        self.push(token);
        Ok(())
    }

    /// `ScanBlockScalar()` (scantoken.cpp:370-436): `|` or `>`, the chomping and indentation
    /// indicators, then the scalar on the following lines.
    pub(super) fn scan_block_scalar(&mut self) -> Result<()> {
        let mut params = ScanScalarParams {
            indent: 1,
            detect_indent: true,
            ..ScanScalarParams::default()
        };

        // eat block indicator ('|' or '>')
        let mark = self.input.mark();
        let indicator = self.input.get();
        params.fold = if indicator == keys::FOLDED_SCALAR {
            Fold::FoldBlock
        } else {
            Fold::DontFold
        };

        // eat chomping/indentation indicators
        params.chomp = Chomp::Clip;
        let n = exp::CHOMP.match_stream(&self.input);
        for _ in 0..n {
            let ch = self.input.get();
            if ch == b'+' {
                params.chomp = Chomp::Keep;
            } else if ch == b'-' {
                params.chomp = Chomp::Strip;
            } else if exp::DIGIT.matches_char(ch) {
                if ch == b'0' {
                    return Err(Exception::parser(
                        self.input.mark(),
                        error_msg::ZERO_INDENT_IN_BLOCK,
                    ));
                }

                params.indent = i32::from(ch - b'0');
                params.detect_indent = false;
            }
        }

        // now eat whitespace
        while exp::BLANK.matches_stream(&self.input) {
            self.input.eat(1);
        }

        // and comments to the end of the line
        if exp::COMMENT.matches_stream(&self.input) {
            while self.input.is_valid() && !exp::BREAK.matches_stream(&self.input) {
                self.input.eat(1);
            }
        }

        // if it's not a line break, then we ran into a bad character inline
        if self.input.is_valid() && !exp::BREAK.matches_stream(&self.input) {
            return Err(Exception::parser(
                self.input.mark(),
                error_msg::CHAR_IN_BLOCK,
            ));
        }

        // set the initial indentation
        if self.get_top_indent() >= 0 {
            params.indent = params.indent.wrapping_add(self.get_top_indent());
        }

        params.eat_leading_whitespace = false;
        params.trim_trailing_spaces = false;
        params.on_tab_in_indentation = Action::Throw;

        let scalar = scan_scalar(&mut self.input, &mut params)?;

        // simple keys always ok after block scalars (since we're gonna start a new line
        // anyways)
        self.simple_key_allowed = true;
        self.can_be_json_flow = false;

        let mut token = Token::new(TokenType::NonPlainScalar, mark);
        token.value = scalar;
        self.push(token);
        Ok(())
    }
}
