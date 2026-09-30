// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.8.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! Port of yaml-cpp 0.8.0's `YAML::Emitter` (include/yaml-cpp/emitter.h, src/emitter.cpp,
//! include/yaml-cpp/stlemitter.h).
//!
//! `Emitter::put` is C++'s `operator<<`: [`Emittable`] maps each argument type to the
//! overload C++ picks (byte strings `&[u8]`, and `&str`/`String` for convenience, to
//! `Write(std::string)`; `u8` to `Write(char)`; `bool`; the integer types to
//! `WriteIntegralType`; `f32`/`f64` to `WriteStreamable`; other slices and vectors to
//! `EmitSeq`; and the manipulators). yaml-cpp's `assert(false)` branches are no-ops, as in
//! the release build OCIO ships.
//!
//! Strings are C++ `std::string`s, i.e. bytes: OCIO hands the emitter whatever bytes it was
//! given (they need not be UTF-8), plain scalars pass them through, quoted and literal ones
//! decode them leniently (see `emitter_utils`), and the output is bytes too.

use ocio_ops::cfmt::{Base, Crt, OStringStream};

use super::emitter_manip::{
    Alias, Anchor, Binary, Comment, EmitterManip, EmitterNodeType, Indent, Null, Precision, Tag,
    TagType, local_tag, secondary_tag,
};
use super::emitter_state::{EmitterState, FlowType, FmtScope, GroupType, error_msg};
use super::emitter_utils::{self as utils, StringEscaping, StringFormat};
use super::ostream_wrapper::OstreamWrapper;

/// Port of `YAML::Emitter`, writing to its own buffer.
#[derive(Debug, Clone, Default)]
pub struct Emitter {
    state: EmitterState,
    stream: OstreamWrapper,
}

/// Port of `GetStringEscapingStyle` (emitter.cpp:706-716).
fn string_escaping_style(manip: EmitterManip) -> StringEscaping {
    match manip {
        EmitterManip::EscapeNonAscii => StringEscaping::NonAscii,
        EmitterManip::EscapeAsJson => StringEscaping::Json,
        _ => StringEscaping::None,
    }
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

    /// Port of `Emitter::StartedScalar` (emitter.cpp:701).
    fn started_scalar(&mut self) {
        self.state.started_scalar();
    }

    // overloads of Write

    /// Port of `Emitter::Write(const std::string &)` (emitter.cpp:718-752) on the string's
    /// bytes: plain when valid (the bytes as they are), else double-quoted (or the requested
    /// format), both of which decode the bytes leniently. Literal strings and strings over
    /// 1024 bytes make a map key a long key.
    pub fn write_bytes(&mut self, s: &[u8]) -> &mut Self {
        if !self.good() {
            return self;
        }

        let escaping = string_escaping_style(self.state.output_charset());
        let format = utils::compute_string_format(
            s,
            self.state.string_format(),
            self.state.cur_group_flow_type(),
            escaping == StringEscaping::NonAscii,
        );

        if format == StringFormat::Literal || s.len() > 1024 {
            self.state
                .set_map_key_format(EmitterManip::LongKey, FmtScope::Local);
        }

        self.prepare_node(EmitterNodeType::Scalar);

        match format {
            StringFormat::Plain => self.stream.write_bytes(s),
            StringFormat::SingleQuoted => {
                utils::write_single_quoted_string(&mut self.stream, s);
            }
            StringFormat::DoubleQuoted => {
                utils::write_double_quoted_string(&mut self.stream, s, escaping);
            }
            StringFormat::Literal => {
                let indent = self.state.cur_indent() + self.state.indent();
                utils::write_literal_string(&mut self.stream, s, indent);
            }
        }

        self.started_scalar();
        self
    }

    /// [`Emitter::write_bytes`] for UTF-8 text.
    pub fn write_str(&mut self, s: &str) -> &mut Self {
        self.write_bytes(s.as_bytes())
    }

    /// Port of `Emitter::ComputeFullBoolName` (emitter.cpp:762-809).
    fn compute_full_bool_name(&self, b: bool) -> &'static str {
        use EmitterManip::*;
        let main_fmt = if self.state.bool_length_format() == ShortBool {
            YesNoBool
        } else {
            self.state.bool_format()
        };
        let case_fmt = self.state.bool_case_format();
        let pick = |t: &'static str, f: &'static str| if b { t } else { f };
        match (main_fmt, case_fmt) {
            (YesNoBool, UpperCase) => pick("YES", "NO"),
            (YesNoBool, CamelCase) => pick("Yes", "No"),
            (YesNoBool, LowerCase) => pick("yes", "no"),
            (OnOffBool, UpperCase) => pick("ON", "OFF"),
            (OnOffBool, CamelCase) => pick("On", "Off"),
            (OnOffBool, LowerCase) => pick("on", "off"),
            (TrueFalseBool, UpperCase) => pick("TRUE", "FALSE"),
            (TrueFalseBool, CamelCase) => pick("True", "False"),
            (TrueFalseBool, LowerCase) => pick("true", "false"),
            // should never get here, but it can't hurt to give these answers
            _ => pick("y", "n"),
        }
    }

    /// Port of `Emitter::ComputeNullName` (emitter.cpp:811-824).
    fn compute_null_name(&self) -> &'static str {
        match self.state.null_format() {
            EmitterManip::LowerNull => "null",
            EmitterManip::UpperNull => "NULL",
            EmitterManip::CamelNull => "Null",
            _ => "~",
        }
    }

    /// Port of `Emitter::Write(bool)` (emitter.cpp:826-841).
    pub fn write_bool(&mut self, b: bool) -> &mut Self {
        if !self.good() {
            return self;
        }

        self.prepare_node(EmitterNodeType::Scalar);

        let name = self.compute_full_bool_name(b);
        if self.state.bool_length_format() == EmitterManip::ShortBool {
            self.stream.write_byte(name.as_bytes()[0]);
        } else {
            self.stream.write_str(name);
        }

        self.started_scalar();
        self
    }

    /// Port of `Emitter::Write(char)` (emitter.cpp:843-854): `ch` is a C++ `char`.
    pub fn write_char(&mut self, ch: u8) -> &mut Self {
        if !self.good() {
            return self;
        }

        self.prepare_node(EmitterNodeType::Scalar);
        let escaping = string_escaping_style(self.state.output_charset());
        utils::write_char(&mut self.stream, ch, escaping);
        self.started_scalar();
        self
    }

    /// Port of `Emitter::Write(const _Alias &)` (emitter.cpp:856-877).
    pub fn write_alias(&mut self, alias: &Alias) -> &mut Self {
        if !self.good() {
            return self;
        }

        if self.state.has_anchor() || self.state.has_tag() {
            self.state.set_error(error_msg::INVALID_ALIAS);
            return self;
        }

        self.prepare_node(EmitterNodeType::Scalar);

        if !utils::write_alias(&mut self.stream, &alias.0) {
            self.state.set_error(error_msg::INVALID_ALIAS);
            return self;
        }

        self.started_scalar();
        self.state.set_alias();
        self
    }

    /// Port of `Emitter::Write(const _Anchor &)` (emitter.cpp:879-898).
    pub fn write_anchor(&mut self, anchor: &Anchor) -> &mut Self {
        if !self.good() {
            return self;
        }

        if self.state.has_anchor() {
            self.state.set_error(error_msg::INVALID_ANCHOR);
            return self;
        }

        self.prepare_node(EmitterNodeType::Property);

        if !utils::write_anchor(&mut self.stream, &anchor.0) {
            self.state.set_error(error_msg::INVALID_ANCHOR);
            return self;
        }

        self.state.set_anchor();
        self
    }

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

    /// Port of `Emitter::Write(const _Comment &)` (emitter.cpp:931-945).
    pub fn write_comment(&mut self, comment: &Comment) -> &mut Self {
        if !self.good() {
            return self;
        }

        self.prepare_node(EmitterNodeType::NoType);

        if self.stream.col() > 0 {
            self.stream.indentation(self.state.pre_comment_indent());
        }
        utils::write_comment(
            &mut self.stream,
            &comment.0,
            self.state.post_comment_indent(),
        );

        self.state.set_non_content();
        self
    }

    /// Port of `Emitter::Write(const _Null &)` (emitter.cpp:947-958).
    pub fn write_null(&mut self) -> &mut Self {
        if !self.good() {
            return self;
        }

        self.prepare_node(EmitterNodeType::Scalar);
        let name = self.compute_null_name();
        self.stream.write_str(name);
        self.started_scalar();
        self
    }

    /// Port of `Emitter::Write(const Binary &)` (emitter.cpp:960-971).
    pub fn write_binary(&mut self, binary: Binary<'_>) -> &mut Self {
        self.write_tag(&secondary_tag("binary"));

        if !self.good() {
            return self;
        }

        self.prepare_node(EmitterNodeType::Scalar);
        utils::write_binary(&mut self.stream, binary.0);
        self.started_scalar();
        self
    }

    /// Port of `Emitter::WriteIntegralType` (emitter.h:136-151) with
    /// `PrepareIntegralStream` (emitter.cpp:682-699): a `std::stringstream` in `std::dec`,
    /// `"0x" << std::hex` or `"0" << std::oct`. `bits` is the C++ type's width, which is
    /// what hex and octal print of a negative value.
    fn write_integral(&mut self, value: i128, bits: u32, signed: bool) -> &mut Self {
        if !self.good() {
            return self;
        }

        self.prepare_node(EmitterNodeType::Scalar);

        let mut stream = OStringStream::new(Crt::NATIVE);
        match self.state.int_format() {
            EmitterManip::Dec => stream.base = Base::Dec,
            EmitterManip::Hex => {
                stream.put_str("0x");
                stream.base = Base::Hex;
            }
            EmitterManip::Oct => {
                stream.put_str("0");
                stream.base = Base::Oct;
            }
            // assert(false) in yaml-cpp
            _ => {}
        }
        if signed && stream.base == Base::Dec {
            stream.put_i64(value as i64);
        } else {
            let mask = if bits == 64 {
                u64::MAX
            } else {
                (1u64 << bits) - 1
            };
            stream.put_u64(value as u64 & mask);
        }
        self.stream.write_str(stream.str());

        self.started_scalar();
        self
    }

    /// Port of `Emitter::WriteStreamable` (emitter.h:153-188): `.nan`, `.inf`, `-.inf`, or
    /// `stream << value` with the float or double precision.
    fn write_streamable(&mut self, value: f64, is_nan: bool, is_inf: bool, precision: usize) {
        if !self.good() {
            return;
        }

        self.prepare_node(EmitterNodeType::Scalar);

        let mut stream = OStringStream::new(Crt::NATIVE);
        stream.precision = precision as i64;

        if is_nan {
            stream.put_str(".nan");
        } else if is_inf {
            if value.is_sign_negative() {
                stream.put_str("-.inf");
            } else {
                stream.put_str(".inf");
            }
        } else {
            stream.put_f64(value);
        }
        self.stream.write_str(stream.str());

        self.started_scalar();
    }

    /// `operator<<(Emitter &, float)`: `WriteStreamable<float>`, at the float precision.
    pub fn write_f32(&mut self, value: f32) -> &mut Self {
        let precision = self.state.float_precision();
        self.write_streamable(
            f64::from(value),
            value.is_nan(),
            value.is_infinite(),
            precision,
        );
        self
    }

    /// `operator<<(Emitter &, double)`: `WriteStreamable<double>`, at the double precision.
    pub fn write_f64(&mut self, value: f64) -> &mut Self {
        let precision = self.state.double_precision();
        self.write_streamable(value, value.is_nan(), value.is_infinite(), precision);
        self
    }

    /// C++ `emitter << value`.
    pub fn put<T: Emittable>(&mut self, value: T) -> &mut Self {
        value.emit(self);
        self
    }
}

/// A value `Emitter::put` accepts: the C++ `operator<<` overloads (emitter.h:200-278,
/// stlemitter.h).
pub trait Emittable {
    /// Writes `self` the way the matching `operator<<` does.
    fn emit(self, out: &mut Emitter);
}

impl Emittable for EmitterManip {
    fn emit(self, out: &mut Emitter) {
        out.set_local_value(self);
    }
}

impl Emittable for Indent {
    fn emit(self, out: &mut Emitter) {
        out.set_local_indent(self);
    }
}

impl Emittable for Precision {
    fn emit(self, out: &mut Emitter) {
        out.set_local_precision(self);
    }
}

/// A C++ `std::string` (or `const char *`): bytes, not a sequence of `char`s.
impl Emittable for &[u8] {
    fn emit(self, out: &mut Emitter) {
        out.write_bytes(self);
    }
}

/// A C++ string literal: bytes.
impl<const N: usize> Emittable for &[u8; N] {
    fn emit(self, out: &mut Emitter) {
        out.write_bytes(self);
    }
}

impl Emittable for &str {
    fn emit(self, out: &mut Emitter) {
        out.write_str(self);
    }
}

impl Emittable for &String {
    fn emit(self, out: &mut Emitter) {
        out.write_str(self);
    }
}

impl Emittable for String {
    fn emit(self, out: &mut Emitter) {
        out.write_str(&self);
    }
}

impl Emittable for bool {
    fn emit(self, out: &mut Emitter) {
        out.write_bool(self);
    }
}

/// `char` and `unsigned char` (a C++ byte).
impl Emittable for u8 {
    fn emit(self, out: &mut Emitter) {
        out.write_char(self);
    }
}

impl Emittable for &Alias {
    fn emit(self, out: &mut Emitter) {
        out.write_alias(self);
    }
}

impl Emittable for &Anchor {
    fn emit(self, out: &mut Emitter) {
        out.write_anchor(self);
    }
}

impl Emittable for &Tag {
    fn emit(self, out: &mut Emitter) {
        out.write_tag(self);
    }
}

impl Emittable for Tag {
    fn emit(self, out: &mut Emitter) {
        out.write_tag(&self);
    }
}

impl Emittable for &Comment {
    fn emit(self, out: &mut Emitter) {
        out.write_comment(self);
    }
}

impl Emittable for Null {
    fn emit(self, out: &mut Emitter) {
        out.write_null();
    }
}

impl Emittable for Binary<'_> {
    fn emit(self, out: &mut Emitter) {
        out.write_binary(self);
    }
}

impl Emittable for f32 {
    fn emit(self, out: &mut Emitter) {
        out.write_f32(self);
    }
}

impl Emittable for f64 {
    fn emit(self, out: &mut Emitter) {
        out.write_f64(self);
    }
}

impl Emittable for &f32 {
    fn emit(self, out: &mut Emitter) {
        out.write_f32(*self);
    }
}

impl Emittable for &f64 {
    fn emit(self, out: &mut Emitter) {
        out.write_f64(*self);
    }
}

macro_rules! emit_integral {
    ($($t:ty => $bits:expr, $signed:expr;)*) => {$(
        impl Emittable for $t {
            fn emit(self, out: &mut Emitter) {
                out.write_integral(i128::from(self), $bits, $signed);
            }
        }
        impl Emittable for &$t {
            fn emit(self, out: &mut Emitter) {
                out.write_integral(i128::from(*self), $bits, $signed);
            }
        }
    )*};
}

// short, unsigned short, int, unsigned int, long long, unsigned long long.
emit_integral! {
    i16 => 16, true;
    u16 => 16, false;
    i32 => 32, true;
    u32 => 32, false;
    i64 => 64, true;
    u64 => 64, false;
}

/// `operator<<(Emitter &, const std::vector<T> &)` (stlemitter.h:16-28): `EmitSeq`.
impl<'a, T> Emittable for &'a [T]
where
    &'a T: Emittable,
{
    fn emit(self, out: &mut Emitter) {
        out.set_local_value(EmitterManip::BeginSeq);
        for v in self {
            v.emit(out);
        }
        out.set_local_value(EmitterManip::EndSeq);
    }
}

impl<'a, T> Emittable for &'a Vec<T>
where
    &'a T: Emittable,
{
    fn emit(self, out: &mut Emitter) {
        self.as_slice().emit(out);
    }
}

/// `operator<<(Emitter &, const std::map<K, V> &)` (stlemitter.h:40-47): a map in key order.
impl<'a, K, V> Emittable for &'a std::collections::BTreeMap<K, V>
where
    &'a K: Emittable,
    &'a V: Emittable,
{
    fn emit(self, out: &mut Emitter) {
        out.set_local_value(EmitterManip::BeginMap);
        for (k, v) in self {
            out.set_local_value(EmitterManip::Key);
            k.emit(out);
            out.set_local_value(EmitterManip::Value);
            v.emit(out);
        }
        out.set_local_value(EmitterManip::EndMap);
    }
}

#[cfg(test)]
#[path = "emitter_tests.rs"]
mod tests;
