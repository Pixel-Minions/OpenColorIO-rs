// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.8.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! Port of yaml-cpp 0.8.0's `YAML::Emitter` (include/yaml-cpp/emitter.h, src/emitter.cpp):
//! the settings, the manipulators that begin and end documents and groups, and the
//! `PrepareNode` state machine that places each node. yaml-cpp's `assert(false)` branches
//! are no-ops, as in the release build OCIO ships.

use super::emitter_manip::{
    EmitterManip, EmitterNodeType, Indent, Precision, Tag, TagType, local_tag,
};
use super::emitter_state::{EmitterState, FlowType, FmtScope, GroupType, error_msg};
use super::emitter_utils as utils;
use super::ostream_wrapper::OstreamWrapper;

/// Port of `YAML::Emitter`, writing to its own buffer.
#[derive(Debug, Clone, Default)]
pub struct Emitter {
    state: EmitterState,
    stream: OstreamWrapper,
}

impl Emitter {
    /// Port of `Emitter::Emitter()` (emitter.cpp:14).
    pub fn new() -> Emitter {
        Emitter::default()
    }

    /// Port of `Emitter::c_str()` (emitter.cpp:21) read as a C string: the bytes up to the
    /// first NUL, which is what OCIO's `ostream << out.c_str()` writes (OCIOYaml.cpp:5447).
    /// A NUL can reach the output raw, e.g. from an overlong `C0 80` in a literal block.
    pub fn c_str(&self) -> &[u8] {
        self.stream.c_str()
    }

    /// Every byte written, including any NUL.
    pub fn as_bytes(&self) -> &[u8] {
        self.stream.as_bytes()
    }

    /// Port of `Emitter::size()` (emitter.cpp:23).
    pub fn size(&self) -> usize {
        self.stream.pos()
    }

    /// Port of `Emitter::good()` (emitter.cpp:26).
    pub fn good(&self) -> bool {
        self.state.good()
    }

    /// Port of `Emitter::GetLastError()` (emitter.cpp:28-30).
    pub fn last_error(&self) -> &str {
        self.state.last_error()
    }

    // global setters

    /// Port of `Emitter::SetOutputCharset` (emitter.cpp:33-35).
    pub fn set_output_charset(&mut self, value: EmitterManip) -> bool {
        self.state.set_output_charset(value, FmtScope::Global)
    }

    /// Port of `Emitter::SetStringFormat` (emitter.cpp:37-39).
    pub fn set_string_format(&mut self, value: EmitterManip) -> bool {
        self.state.set_string_format(value, FmtScope::Global)
    }

    /// Port of `Emitter::SetBoolFormat` (emitter.cpp:41-50).
    pub fn set_bool_format(&mut self, value: EmitterManip) -> bool {
        let mut ok = false;
        if self.state.set_bool_format(value, FmtScope::Global) {
            ok = true;
        }
        if self.state.set_bool_case_format(value, FmtScope::Global) {
            ok = true;
        }
        if self.state.set_bool_length_format(value, FmtScope::Global) {
            ok = true;
        }
        ok
    }

    /// Port of `Emitter::SetNullFormat` (emitter.cpp:52-54).
    pub fn set_null_format(&mut self, value: EmitterManip) -> bool {
        self.state.set_null_format(value, FmtScope::Global)
    }

    /// Port of `Emitter::SetIntBase` (emitter.cpp:56-58).
    pub fn set_int_base(&mut self, value: EmitterManip) -> bool {
        self.state.set_int_format(value, FmtScope::Global)
    }

    /// Port of `Emitter::SetSeqFormat` (emitter.cpp:60-62).
    pub fn set_seq_format(&mut self, value: EmitterManip) -> bool {
        self.state
            .set_flow_type(GroupType::Seq, value, FmtScope::Global)
    }

    /// Port of `Emitter::SetMapFormat` (emitter.cpp:64-71).
    pub fn set_map_format(&mut self, value: EmitterManip) -> bool {
        let mut ok = false;
        if self
            .state
            .set_flow_type(GroupType::Map, value, FmtScope::Global)
        {
            ok = true;
        }
        if self.state.set_map_key_format(value, FmtScope::Global) {
            ok = true;
        }
        ok
    }

    /// Port of `Emitter::SetIndent` (emitter.cpp:73-75).
    pub fn set_indent(&mut self, n: usize) -> bool {
        self.state.set_indent(n, FmtScope::Global)
    }

    /// Port of `Emitter::SetPreCommentIndent` (emitter.cpp:77-79).
    pub fn set_pre_comment_indent(&mut self, n: usize) -> bool {
        self.state.set_pre_comment_indent(n, FmtScope::Global)
    }

    /// Port of `Emitter::SetPostCommentIndent` (emitter.cpp:81-83).
    pub fn set_post_comment_indent(&mut self, n: usize) -> bool {
        self.state.set_post_comment_indent(n, FmtScope::Global)
    }

    /// Port of `Emitter::SetFloatPrecision` (emitter.cpp:85-87).
    pub fn set_float_precision(&mut self, n: usize) -> bool {
        self.state.set_float_precision(n, FmtScope::Global)
    }

    /// Port of `Emitter::SetDoublePrecision` (emitter.cpp:89-91).
    pub fn set_double_precision(&mut self, n: usize) -> bool {
        self.state.set_double_precision(n, FmtScope::Global)
    }

    /// Port of `Emitter::RestoreGlobalModifiedSettings` (emitter.cpp:93-95).
    pub fn restore_global_modified_settings(&mut self) {
        self.state.restore_global_modified_settings();
    }

    // local setters

    /// Port of `Emitter::SetLocalValue` (emitter.cpp:97-137): starts or ends a group or a
    /// document, or sets a formatter until the next node.
    pub fn set_local_value(&mut self, value: EmitterManip) -> &mut Self {
        if !self.good() {
            return self;
        }
        match value {
            EmitterManip::BeginDoc => self.emit_begin_doc(),
            EmitterManip::EndDoc => self.emit_end_doc(),
            EmitterManip::BeginSeq => self.emit_begin_seq(),
            EmitterManip::EndSeq => self.emit_end_seq(),
            EmitterManip::BeginMap => self.emit_begin_map(),
            EmitterManip::EndMap => self.emit_end_map(),
            // deprecated (these can be deduced by the parity of nodes in a map)
            EmitterManip::Key | EmitterManip::Value => {}
            EmitterManip::TagByKind => self.emit_kind_tag(),
            EmitterManip::Newline => self.emit_newline(),
            _ => self.state.set_local_value(value),
        }
        self
    }

    /// Port of `Emitter::SetLocalIndent` (emitter.cpp:139-142). The `int` converts to
    /// `size_t` as in C++: a negative value wraps to a huge indent, which
    /// `EmitterState::SetIndent` accepts (it refuses only 0 and 1, emitterstate.cpp:328-334),
    /// so the next indentation would write that many spaces. OCIO never sets an indent.
    pub fn set_local_indent(&mut self, indent: Indent) -> &mut Self {
        self.state.set_indent(indent.0 as usize, FmtScope::Local);
        self
    }

    /// Port of `Emitter::SetLocalPrecision` (emitter.cpp:144-150).
    pub fn set_local_precision(&mut self, precision: Precision) -> &mut Self {
        if precision.float_precision >= 0 {
            self.state
                .set_float_precision(precision.float_precision as usize, FmtScope::Local);
        }
        if precision.double_precision >= 0 {
            self.state
                .set_double_precision(precision.double_precision as usize, FmtScope::Local);
        }
        self
    }

    /// Port of `Emitter::EmitBeginDoc` (emitter.cpp:152-172).
    fn emit_begin_doc(&mut self) {
        if !self.good() {
            return;
        }
        if self.state.cur_group_type() != GroupType::NoType {
            return self.state.set_error("Unexpected begin document");
        }
        if self.state.has_anchor() || self.state.has_tag() {
            return self.state.set_error("Unexpected begin document");
        }
        if self.stream.col() > 0 {
            self.stream.write_byte(b'\n');
        }
        self.stream.write_bytes(b"---\n");
        self.state.started_doc();
    }

    /// Port of `Emitter::EmitEndDoc` (emitter.cpp:174-192).
    fn emit_end_doc(&mut self) {
        if !self.good() {
            return;
        }
        if self.state.cur_group_type() != GroupType::NoType {
            return self.state.set_error("Unexpected begin document");
        }
        if self.state.has_anchor() || self.state.has_tag() {
            return self.state.set_error("Unexpected begin document");
        }
        if self.stream.col() > 0 {
            self.stream.write_byte(b'\n');
        }
        self.stream.write_bytes(b"...\n");
    }

    /// Port of `Emitter::EmitBeginSeq` (emitter.cpp:194-202).
    fn emit_begin_seq(&mut self) {
        if !self.good() {
            return;
        }
        self.prepare_node(self.state.next_group_type(GroupType::Seq));
        self.state.started_group(GroupType::Seq);
    }

    /// Port of `Emitter::EmitEndSeq` (emitter.cpp:204-227): an empty sequence is written in
    /// flow style, `[]`.
    fn emit_end_seq(&mut self) {
        self.emit_end_group(GroupType::Seq, b'[', b']');
    }

    /// Port of `Emitter::EmitBeginMap` (emitter.cpp:229-237).
    fn emit_begin_map(&mut self) {
        if !self.good() {
            return;
        }
        self.prepare_node(self.state.next_group_type(GroupType::Map));
        self.state.started_group(GroupType::Map);
    }

    /// Port of `Emitter::EmitEndMap` (emitter.cpp:239-262): an empty map is written `{}`.
    fn emit_end_map(&mut self) {
        self.emit_end_group(GroupType::Map, b'{', b'}');
    }

    /// The shared body of `EmitEndSeq` and `EmitEndMap`.
    fn emit_end_group(&mut self, group_type: GroupType, open: u8, close: u8) {
        if !self.good() {
            return;
        }
        let original_type = self.state.cur_group_flow_type();

        if self.state.cur_group_child_count() == 0 {
            self.state.force_flow();
        }

        if self.state.cur_group_flow_type() == FlowType::Flow {
            if self.stream.comment() {
                self.stream.write_byte(b'\n');
            }
            self.stream.indent_to(self.state.cur_indent());
            // An empty group that was block-style, or a flow group that never wrote its
            // opening bracket, opens here.
            if original_type == FlowType::Block
                || (self.state.cur_group_child_count() == 0 && !self.state.has_begun_node())
            {
                self.stream.write_byte(open);
            }
            self.stream.write_byte(close);
        }

        self.state.ended_group(group_type);
    }

    /// Port of `Emitter::EmitNewline` (emitter.cpp:264-272).
    fn emit_newline(&mut self) {
        if !self.good() {
            return;
        }
        self.prepare_node(EmitterNodeType::NoType);
        self.stream.write_byte(b'\n');
        self.state.set_non_content();
    }

    /// Port of `Emitter::PrepareNode` (emitter.cpp:276-300): puts the stream in a state to
    /// write the next node (e.g. writes a sequence's `- `).
    fn prepare_node(&mut self, child: EmitterNodeType) {
        match self.state.cur_group_node_type() {
            EmitterNodeType::NoType => self.prepare_top_node(child),
            EmitterNodeType::FlowSeq => self.flow_seq_prepare_node(child),
            EmitterNodeType::BlockSeq => self.block_seq_prepare_node(child),
            EmitterNodeType::FlowMap => self.flow_map_prepare_node(child),
            EmitterNodeType::BlockMap => self.block_map_prepare_node(child),
            // assert(false) in yaml-cpp
            EmitterNodeType::Property | EmitterNodeType::Scalar => {}
        }
    }

    /// Port of `Emitter::PrepareTopNode` (emitter.cpp:302-326).
    fn prepare_top_node(&mut self, child: EmitterNodeType) {
        if child == EmitterNodeType::NoType {
            return;
        }
        if self.state.cur_group_child_count() > 0 && self.stream.col() > 0 {
            self.emit_begin_doc();
        }
        match child {
            EmitterNodeType::NoType => {}
            EmitterNodeType::Property
            | EmitterNodeType::Scalar
            | EmitterNodeType::FlowSeq
            | EmitterNodeType::FlowMap => {
                // TODO (yaml-cpp): if we were writing null, and we wanted it blank, we
                // wouldn't want a space
                self.space_or_indent_to(self.state.has_begun_content(), 0);
            }
            EmitterNodeType::BlockSeq | EmitterNodeType::BlockMap => {
                if self.state.has_begun_node() {
                    self.stream.write_byte(b'\n');
                }
            }
        }
    }

    /// The `SpaceOrIndentTo` call shared by the flow `Prepare*` functions for scalar and
    /// flow children; block children can't occur in a flow group (assert(false)).
    fn flow_child(&mut self, child: EmitterNodeType, last_indent: usize) {
        match child {
            EmitterNodeType::Property
            | EmitterNodeType::Scalar
            | EmitterNodeType::FlowSeq
            | EmitterNodeType::FlowMap => {
                let require_space =
                    self.state.has_begun_content() || self.state.cur_group_child_count() > 0;
                self.space_or_indent_to(require_space, last_indent);
            }
            EmitterNodeType::NoType | EmitterNodeType::BlockSeq | EmitterNodeType::BlockMap => {}
        }
    }

    /// Port of `Emitter::FlowSeqPrepareNode` (emitter.cpp:328-357).
    fn flow_seq_prepare_node(&mut self, child: EmitterNodeType) {
        let last_indent = self.state.last_indent();

        if !self.state.has_begun_node() {
            if self.stream.comment() {
                self.stream.write_byte(b'\n');
            }
            self.stream.indent_to(last_indent);
            if self.state.cur_group_child_count() == 0 {
                self.stream.write_byte(b'[');
            } else {
                self.stream.write_byte(b',');
            }
        }

        self.flow_child(child, last_indent);
    }

    /// Port of `Emitter::BlockSeqPrepareNode` (emitter.cpp:359-391).
    fn block_seq_prepare_node(&mut self, child: EmitterNodeType) {
        let cur_indent = self.state.cur_indent();
        let next_indent = cur_indent + self.state.cur_group_indent();

        if child == EmitterNodeType::NoType {
            return;
        }

        if !self.state.has_begun_content() {
            if self.state.cur_group_child_count() > 0 || self.stream.comment() {
                self.stream.write_byte(b'\n');
            }
            self.stream.indent_to(cur_indent);
            self.stream.write_byte(b'-');
        }

        match child {
            EmitterNodeType::NoType => {}
            EmitterNodeType::Property
            | EmitterNodeType::Scalar
            | EmitterNodeType::FlowSeq
            | EmitterNodeType::FlowMap => {
                self.space_or_indent_to(self.state.has_begun_content(), next_indent);
            }
            EmitterNodeType::BlockSeq => self.stream.write_byte(b'\n'),
            EmitterNodeType::BlockMap => {
                if self.state.has_begun_content() || self.stream.comment() {
                    self.stream.write_byte(b'\n');
                }
            }
        }
    }

    /// Port of `Emitter::FlowMapPrepareNode` (emitter.cpp:393-408).
    fn flow_map_prepare_node(&mut self, child: EmitterNodeType) {
        if self.state.cur_group_child_count().is_multiple_of(2) {
            if self.state.map_key_format() == EmitterManip::LongKey {
                self.state.set_long_key();
            }
            if self.state.cur_group_long_key() {
                self.flow_map_prepare_long_key(child);
            } else {
                self.flow_map_prepare_simple_key(child);
            }
        } else if self.state.cur_group_long_key() {
            self.flow_map_prepare_long_key_value(child);
        } else {
            self.flow_map_prepare_simple_key_value(child);
        }
    }

    /// Port of `Emitter::FlowMapPrepareLongKey` (emitter.cpp:410-439).
    fn flow_map_prepare_long_key(&mut self, child: EmitterNodeType) {
        let last_indent = self.state.last_indent();

        if !self.state.has_begun_node() {
            if self.stream.comment() {
                self.stream.write_byte(b'\n');
            }
            self.stream.indent_to(last_indent);
            if self.state.cur_group_child_count() == 0 {
                self.stream.write_bytes(b"{ ?");
            } else {
                self.stream.write_bytes(b", ?");
            }
        }

        self.flow_child(child, last_indent);
    }

    /// Port of `Emitter::FlowMapPrepareLongKeyValue` (emitter.cpp:441-467).
    fn flow_map_prepare_long_key_value(&mut self, child: EmitterNodeType) {
        let last_indent = self.state.last_indent();

        if !self.state.has_begun_node() {
            if self.stream.comment() {
                self.stream.write_byte(b'\n');
            }
            self.stream.indent_to(last_indent);
            self.stream.write_byte(b':');
        }

        self.flow_child(child, last_indent);
    }

    /// Port of `Emitter::FlowMapPrepareSimpleKey` (emitter.cpp:469-498).
    fn flow_map_prepare_simple_key(&mut self, child: EmitterNodeType) {
        let last_indent = self.state.last_indent();

        if !self.state.has_begun_node() {
            if self.stream.comment() {
                self.stream.write_byte(b'\n');
            }
            self.stream.indent_to(last_indent);
            if self.state.cur_group_child_count() == 0 {
                self.stream.write_byte(b'{');
            } else {
                self.stream.write_byte(b',');
            }
        }

        self.flow_child(child, last_indent);
    }

    /// Port of `Emitter::FlowMapPrepareSimpleKeyValue` (emitter.cpp:500-529).
    fn flow_map_prepare_simple_key_value(&mut self, child: EmitterNodeType) {
        let last_indent = self.state.last_indent();

        if !self.state.has_begun_node() {
            if self.stream.comment() {
                self.stream.write_byte(b'\n');
            }
            self.stream.indent_to(last_indent);
            if self.state.has_alias() {
                self.stream.write_byte(b' ');
            }
            self.stream.write_byte(b':');
        }

        self.flow_child(child, last_indent);
    }

    /// Port of `Emitter::BlockMapPrepareNode` (emitter.cpp:531-550): block children, and
    /// keys with properties, are written as long keys (`? key`).
    fn block_map_prepare_node(&mut self, child: EmitterNodeType) {
        if self.state.cur_group_child_count().is_multiple_of(2) {
            if self.state.map_key_format() == EmitterManip::LongKey {
                self.state.set_long_key();
            }
            if matches!(
                child,
                EmitterNodeType::BlockSeq | EmitterNodeType::BlockMap | EmitterNodeType::Property
            ) {
                self.state.set_long_key();
            }

            if self.state.cur_group_long_key() {
                self.block_map_prepare_long_key(child);
            } else {
                self.block_map_prepare_simple_key(child);
            }
        } else if self.state.cur_group_long_key() {
            self.block_map_prepare_long_key_value(child);
        } else {
            self.block_map_prepare_simple_key_value(child);
        }
    }

    /// Port of `Emitter::BlockMapPrepareLongKey` (emitter.cpp:552-585).
    fn block_map_prepare_long_key(&mut self, child: EmitterNodeType) {
        let cur_indent = self.state.cur_indent();
        let child_count = self.state.cur_group_child_count();

        if child == EmitterNodeType::NoType {
            return;
        }

        if !self.state.has_begun_content() {
            if child_count > 0 {
                self.stream.write_byte(b'\n');
            }
            if self.stream.comment() {
                self.stream.write_byte(b'\n');
            }
            self.stream.indent_to(cur_indent);
            self.stream.write_byte(b'?');
        }

        match child {
            EmitterNodeType::NoType => {}
            EmitterNodeType::Property
            | EmitterNodeType::Scalar
            | EmitterNodeType::FlowSeq
            | EmitterNodeType::FlowMap => self.space_or_indent_to(true, cur_indent + 1),
            EmitterNodeType::BlockSeq | EmitterNodeType::BlockMap => {
                if self.state.has_begun_content() {
                    self.stream.write_byte(b'\n');
                }
            }
        }
    }

    /// Port of `Emitter::BlockMapPrepareLongKeyValue` (emitter.cpp:587-615).
    fn block_map_prepare_long_key_value(&mut self, child: EmitterNodeType) {
        let cur_indent = self.state.cur_indent();

        if child == EmitterNodeType::NoType {
            return;
        }

        if !self.state.has_begun_content() {
            self.stream.write_byte(b'\n');
            self.stream.indent_to(cur_indent);
            self.stream.write_byte(b':');
        }

        match child {
            EmitterNodeType::NoType => {}
            EmitterNodeType::Property
            | EmitterNodeType::Scalar
            | EmitterNodeType::FlowSeq
            | EmitterNodeType::FlowMap => self.space_or_indent_to(true, cur_indent + 1),
            EmitterNodeType::BlockSeq | EmitterNodeType::BlockMap => {
                if self.state.has_begun_content() {
                    self.stream.write_byte(b'\n');
                }
                self.space_or_indent_to(true, cur_indent + 1);
            }
        }
    }

    /// Port of `Emitter::BlockMapPrepareSimpleKey` (emitter.cpp:617-643).
    fn block_map_prepare_simple_key(&mut self, child: EmitterNodeType) {
        let cur_indent = self.state.cur_indent();
        let child_count = self.state.cur_group_child_count();

        if child == EmitterNodeType::NoType {
            return;
        }

        if !self.state.has_begun_node() && child_count > 0 {
            self.stream.write_byte(b'\n');
        }

        match child {
            EmitterNodeType::Property
            | EmitterNodeType::Scalar
            | EmitterNodeType::FlowSeq
            | EmitterNodeType::FlowMap => {
                self.space_or_indent_to(self.state.has_begun_content(), cur_indent);
            }
            EmitterNodeType::NoType | EmitterNodeType::BlockSeq | EmitterNodeType::BlockMap => {}
        }
    }

    /// Port of `Emitter::BlockMapPrepareSimpleKeyValue` (emitter.cpp:645-670).
    fn block_map_prepare_simple_key_value(&mut self, child: EmitterNodeType) {
        let cur_indent = self.state.cur_indent();
        let next_indent = cur_indent + self.state.cur_group_indent();

        if !self.state.has_begun_node() {
            if self.state.has_alias() {
                self.stream.write_byte(b' ');
            }
            self.stream.write_byte(b':');
        }

        match child {
            EmitterNodeType::NoType => {}
            EmitterNodeType::Property
            | EmitterNodeType::Scalar
            | EmitterNodeType::FlowSeq
            | EmitterNodeType::FlowMap => self.space_or_indent_to(true, next_indent),
            EmitterNodeType::BlockSeq | EmitterNodeType::BlockMap => {
                self.stream.write_byte(b'\n');
            }
        }
    }

    /// Port of `Emitter::SpaceOrIndentTo` (emitter.cpp:672-680).
    fn space_or_indent_to(&mut self, require_space: bool, indent: usize) {
        if self.stream.comment() {
            self.stream.write_byte(b'\n');
        }
        if self.stream.col() > 0 && require_space {
            self.stream.write_byte(b' ');
        }
        self.stream.indent_to(indent);
    }

    // overloads of Write

    /// Port of `Emitter::Write(const _Tag &)` (emitter.cpp:900-927).
    pub fn write_tag(&mut self, tag: &Tag) -> &mut Self {
        if !self.good() {
            return self;
        }

        if self.state.has_tag() {
            self.state.set_error(error_msg::INVALID_TAG);
            return self;
        }

        self.prepare_node(EmitterNodeType::Property);

        let success = match tag.tag_type {
            TagType::Verbatim => utils::write_tag(&mut self.stream, &tag.content, true),
            TagType::PrimaryHandle => utils::write_tag(&mut self.stream, &tag.content, false),
            TagType::NamedHandle => {
                utils::write_tag_with_prefix(&mut self.stream, &tag.prefix, &tag.content)
            }
        };

        if !success {
            self.state.set_error(error_msg::INVALID_TAG);
            return self;
        }

        self.state.set_tag();
        self
    }

    /// Port of `Emitter::EmitKindTag` (emitter.cpp:929).
    fn emit_kind_tag(&mut self) {
        self.write_tag(&local_tag(""));
    }
}
