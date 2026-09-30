// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.8.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! Port of yaml-cpp 0.8.0's `EmitterState` (src/emitterstate.h, src/emitterstate.cpp) and
//! its scoped settings (src/setting.h).
//!
//! A setting change records the old value; restoring a list of changes replays them in the
//! order they were made (`SettingChanges::restore`), so a setting changed twice in one scope
//! ends at its middle value, as in yaml-cpp.

use super::emitter_manip::{EmitterManip, EmitterNodeType};

/// Port of `YAML::FmtScope::value`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FmtScope {
    /// Until the next node.
    Local,
    /// From now on.
    Global,
}

/// Port of `YAML::GroupType::value`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupType {
    NoType,
    Seq,
    Map,
}

/// Port of `YAML::FlowType::value`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowType {
    NoType,
    Flow,
    Block,
}

/// yaml-cpp's `ErrorMsg` texts the emitter reports (include/yaml-cpp/exceptions.h:81-88).
pub mod error_msg {
    /// `ErrorMsg::UNMATCHED_GROUP_TAG`
    pub const UNMATCHED_GROUP_TAG: &str = "unmatched group tag";
    /// `ErrorMsg::UNEXPECTED_END_SEQ`
    pub const UNEXPECTED_END_SEQ: &str = "unexpected end sequence token";
    /// `ErrorMsg::UNEXPECTED_END_MAP`
    pub const UNEXPECTED_END_MAP: &str = "unexpected end map token";
    /// `ErrorMsg::INVALID_ANCHOR`
    pub const INVALID_ANCHOR: &str = "invalid anchor";
    /// `ErrorMsg::INVALID_ALIAS`
    pub const INVALID_ALIAS: &str = "invalid alias";
    /// `ErrorMsg::INVALID_TAG`
    pub const INVALID_TAG: &str = "invalid tag";
}

/// The `Setting<T>` members of `EmitterState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingKey {
    Charset,
    StrFmt,
    BoolFmt,
    BoolLengthFmt,
    BoolCaseFmt,
    NullFmt,
    IntFmt,
    Indent,
    PreCommentIndent,
    PostCommentIndent,
    SeqFmt,
    MapFmt,
    MapKeyFmt,
    FloatPrecision,
    DoublePrecision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingValue {
    Manip(EmitterManip),
    Size(usize),
}

/// The current values of the settings.
#[derive(Debug, Clone)]
struct Settings {
    charset: EmitterManip,
    str_fmt: EmitterManip,
    bool_fmt: EmitterManip,
    bool_length_fmt: EmitterManip,
    bool_case_fmt: EmitterManip,
    null_fmt: EmitterManip,
    int_fmt: EmitterManip,
    indent: usize,
    pre_comment_indent: usize,
    post_comment_indent: usize,
    seq_fmt: EmitterManip,
    map_fmt: EmitterManip,
    map_key_fmt: EmitterManip,
    float_precision: usize,
    double_precision: usize,
}

impl Settings {
    fn manip(&mut self, key: SettingKey) -> Option<&mut EmitterManip> {
        Some(match key {
            SettingKey::Charset => &mut self.charset,
            SettingKey::StrFmt => &mut self.str_fmt,
            SettingKey::BoolFmt => &mut self.bool_fmt,
            SettingKey::BoolLengthFmt => &mut self.bool_length_fmt,
            SettingKey::BoolCaseFmt => &mut self.bool_case_fmt,
            SettingKey::NullFmt => &mut self.null_fmt,
            SettingKey::IntFmt => &mut self.int_fmt,
            SettingKey::SeqFmt => &mut self.seq_fmt,
            SettingKey::MapFmt => &mut self.map_fmt,
            SettingKey::MapKeyFmt => &mut self.map_key_fmt,
            _ => return None,
        })
    }

    fn size(&mut self, key: SettingKey) -> Option<&mut usize> {
        Some(match key {
            SettingKey::Indent => &mut self.indent,
            SettingKey::PreCommentIndent => &mut self.pre_comment_indent,
            SettingKey::PostCommentIndent => &mut self.post_comment_indent,
            SettingKey::FloatPrecision => &mut self.float_precision,
            SettingKey::DoublePrecision => &mut self.double_precision,
            _ => return None,
        })
    }

    fn get(&mut self, key: SettingKey) -> SettingValue {
        if let Some(m) = self.manip(key) {
            SettingValue::Manip(*m)
        } else {
            SettingValue::Size(*self.size(key).expect("a size setting"))
        }
    }

    /// `Setting<T>::set`: stores the value and returns the change (the old value).
    fn set(&mut self, key: SettingKey, value: SettingValue) -> (SettingKey, SettingValue) {
        let old = self.get(key);
        match value {
            SettingValue::Manip(v) => *self.manip(key).expect("a manipulator setting") = v,
            SettingValue::Size(v) => *self.size(key).expect("a size setting") = v,
        }
        (key, old)
    }
}

/// Port of `YAML::SettingChanges` (setting.h:63-97): old values to restore, in order.
#[derive(Debug, Clone, Default)]
struct SettingChanges(Vec<(SettingKey, SettingValue)>);

impl SettingChanges {
    /// `SettingChanges::restore`: pops every change, first to last.
    fn restore(&self, settings: &mut Settings) {
        for &(key, old) in &self.0 {
            settings.set(key, old);
        }
    }

    /// `SettingChanges::clear`: restores, then forgets the changes.
    fn clear(&mut self, settings: &mut Settings) {
        self.restore(settings);
        self.0.clear();
    }

    fn push(&mut self, change: (SettingKey, SettingValue)) {
        self.0.push(change);
    }
}

/// Port of `EmitterState::Group` (emitterstate.h:153-187).
#[derive(Debug, Clone)]
struct Group {
    group_type: GroupType,
    flow_type: FlowType,
    indent: usize,
    child_count: usize,
    long_key: bool,
    modified_settings: SettingChanges,
}

impl Group {
    fn new(group_type: GroupType) -> Group {
        Group {
            group_type,
            flow_type: FlowType::NoType,
            indent: 0,
            child_count: 0,
            long_key: false,
            modified_settings: SettingChanges::default(),
        }
    }

    /// `Group::NodeType` (emitterstate.h:170-186).
    fn node_type(&self) -> EmitterNodeType {
        match (self.group_type, self.flow_type) {
            (GroupType::Seq, FlowType::Flow) => EmitterNodeType::FlowSeq,
            (GroupType::Seq, _) => EmitterNodeType::BlockSeq,
            (_, FlowType::Flow) => EmitterNodeType::FlowMap,
            _ => EmitterNodeType::BlockMap,
        }
    }
}

/// Port of `YAML::EmitterState` (emitterstate.h:31-196, emitterstate.cpp).
#[derive(Debug, Clone)]
pub struct EmitterState {
    is_good: bool,
    last_error: String,
    settings: Settings,
    modified_settings: SettingChanges,
    global_modified_settings: SettingChanges,
    groups: Vec<Group>,
    cur_indent: usize,
    has_anchor: bool,
    has_alias: bool,
    has_tag: bool,
    has_non_content: bool,
    doc_count: usize,
}

impl Default for EmitterState {
    fn default() -> Self {
        Self::new()
    }
}

impl EmitterState {
    /// Port of `EmitterState::EmitterState()` (emitterstate.cpp:7-35): the default global
    /// manipulators. The float and double precisions start at `max_digits10` (9 and 17).
    pub fn new() -> EmitterState {
        EmitterState {
            is_good: true,
            last_error: String::new(),
            settings: Settings {
                charset: EmitterManip::EmitNonAscii,
                str_fmt: EmitterManip::Auto,
                bool_fmt: EmitterManip::TrueFalseBool,
                bool_length_fmt: EmitterManip::LongBool,
                bool_case_fmt: EmitterManip::LowerCase,
                null_fmt: EmitterManip::TildeNull,
                int_fmt: EmitterManip::Dec,
                indent: 2,
                pre_comment_indent: 2,
                post_comment_indent: 1,
                seq_fmt: EmitterManip::Block,
                map_fmt: EmitterManip::Block,
                map_key_fmt: EmitterManip::Auto,
                float_precision: 9,
                double_precision: 17,
            },
            modified_settings: SettingChanges::default(),
            global_modified_settings: SettingChanges::default(),
            groups: Vec::new(),
            cur_indent: 0,
            has_anchor: false,
            has_alias: false,
            has_tag: false,
            has_non_content: false,
            doc_count: 0,
        }
    }

    // basic state checking

    /// `good()`
    pub fn good(&self) -> bool {
        self.is_good
    }

    /// `GetLastError()`
    pub fn last_error(&self) -> &str {
        &self.last_error
    }

    /// `SetError`
    pub fn set_error(&mut self, error: &str) {
        self.is_good = false;
        self.last_error = error.to_string();
    }

    // node handling

    /// `SetAnchor`
    pub fn set_anchor(&mut self) {
        self.has_anchor = true;
    }

    /// `SetAlias`
    pub fn set_alias(&mut self) {
        self.has_alias = true;
    }

    /// `SetTag`
    pub fn set_tag(&mut self) {
        self.has_tag = true;
    }

    /// `SetNonContent`
    pub fn set_non_content(&mut self) {
        self.has_non_content = true;
    }

    /// `SetLongKey` (emitterstate.cpp:63-71): only in a map (asserted in C++).
    pub fn set_long_key(&mut self) {
        if let Some(group) = self.groups.last_mut() {
            group.long_key = true;
        }
    }

    /// `ForceFlow` (emitterstate.cpp:73-80).
    pub fn force_flow(&mut self) {
        if let Some(group) = self.groups.last_mut() {
            group.flow_type = FlowType::Flow;
        }
    }

    /// `StartedNode` (emitterstate.cpp:82-96).
    fn started_node(&mut self) {
        match self.groups.last_mut() {
            None => self.doc_count += 1,
            Some(group) => {
                group.child_count += 1;
                if group.child_count.is_multiple_of(2) {
                    group.long_key = false;
                }
            }
        }
        self.has_anchor = false;
        self.has_alias = false;
        self.has_tag = false;
        self.has_non_content = false;
    }

    /// `NextGroupType` (emitterstate.cpp:98-113).
    pub fn next_group_type(&self, group_type: GroupType) -> EmitterNodeType {
        if group_type == GroupType::Seq {
            if self.flow_type(group_type) == EmitterManip::Block {
                return EmitterNodeType::BlockSeq;
            }
            return EmitterNodeType::FlowSeq;
        }
        if self.flow_type(group_type) == EmitterManip::Block {
            return EmitterNodeType::BlockMap;
        }
        EmitterNodeType::FlowMap
    }

    /// `StartedDoc` (emitterstate.cpp:115-119).
    pub fn started_doc(&mut self) {
        self.has_anchor = false;
        self.has_tag = false;
        self.has_non_content = false;
    }

    /// `EndedDoc` (emitterstate.cpp:121-125).
    pub fn ended_doc(&mut self) {
        self.has_anchor = false;
        self.has_tag = false;
        self.has_non_content = false;
    }

    /// `StartedScalar` (emitterstate.cpp:127-130).
    pub fn started_scalar(&mut self) {
        self.started_node();
        self.clear_modified_settings();
    }

    /// `StartedGroup` (emitterstate.cpp:132-157): the local settings move to the group and
    /// last until it ends.
    pub fn started_group(&mut self, group_type: GroupType) {
        self.started_node();

        let last_group_indent = self.groups.last().map_or(0, |g| g.indent);
        self.cur_indent += last_group_indent;

        let mut group = Group::new(group_type);
        group.modified_settings = std::mem::take(&mut self.modified_settings);
        group.flow_type = if self.flow_type(group_type) == EmitterManip::Block {
            FlowType::Block
        } else {
            FlowType::Flow
        };
        group.indent = self.indent();
        self.groups.push(group);
    }

    /// `EndedGroup` (emitterstate.cpp:159-196).
    pub fn ended_group(&mut self, group_type: GroupType) {
        if self.groups.is_empty() {
            if group_type == GroupType::Seq {
                return self.set_error(error_msg::UNEXPECTED_END_SEQ);
            }
            return self.set_error(error_msg::UNEXPECTED_END_MAP);
        }

        if self.has_tag {
            self.set_error(error_msg::INVALID_TAG);
        }
        if self.has_anchor {
            self.set_error(error_msg::INVALID_ANCHOR);
        }

        // Get rid of the current group; destroying it restores its settings.
        {
            let mut finished = self.groups.pop().expect("a group");
            finished.modified_settings.clear(&mut self.settings);
            if finished.group_type != group_type {
                return self.set_error(error_msg::UNMATCHED_GROUP_TAG);
            }
        }

        // reset old settings
        let last_indent = self.groups.last().map_or(0, |g| g.indent);
        self.cur_indent -= last_indent;

        // Some global settings that we changed may have been overridden by a local setting
        // we just popped, so we need to restore them.
        self.global_modified_settings.restore(&mut self.settings);

        self.clear_modified_settings();
        self.has_anchor = false;
        self.has_tag = false;
        self.has_non_content = false;
    }

    /// `CurGroupNodeType` (emitterstate.cpp:198-204).
    pub fn cur_group_node_type(&self) -> EmitterNodeType {
        self.groups
            .last()
            .map_or(EmitterNodeType::NoType, Group::node_type)
    }

    /// `CurGroupType`
    pub fn cur_group_type(&self) -> GroupType {
        self.groups
            .last()
            .map_or(GroupType::NoType, |g| g.group_type)
    }

    /// `CurGroupFlowType`
    pub fn cur_group_flow_type(&self) -> FlowType {
        self.groups.last().map_or(FlowType::NoType, |g| g.flow_type)
    }

    /// `CurGroupIndent`
    pub fn cur_group_indent(&self) -> usize {
        self.groups.last().map_or(0, |g| g.indent)
    }

    /// `CurGroupChildCount`: the document count at the top level.
    pub fn cur_group_child_count(&self) -> usize {
        self.groups.last().map_or(self.doc_count, |g| g.child_count)
    }

    /// `CurGroupLongKey`
    pub fn cur_group_long_key(&self) -> bool {
        self.groups.last().is_some_and(|g| g.long_key)
    }

    /// `LastIndent` (emitterstate.cpp:226-232).
    pub fn last_indent(&self) -> usize {
        if self.groups.len() <= 1 {
            return 0;
        }
        self.cur_indent - self.groups[self.groups.len() - 2].indent
    }

    /// `CurIndent`
    pub fn cur_indent(&self) -> usize {
        self.cur_indent
    }

    /// `HasAnchor`
    pub fn has_anchor(&self) -> bool {
        self.has_anchor
    }

    /// `HasAlias`
    pub fn has_alias(&self) -> bool {
        self.has_alias
    }

    /// `HasTag`
    pub fn has_tag(&self) -> bool {
        self.has_tag
    }

    /// `HasBegunNode`: an anchor, a tag or non-content (a newline or comment) was written.
    pub fn has_begun_node(&self) -> bool {
        self.has_anchor || self.has_tag || self.has_non_content
    }

    /// `HasBegunContent`: an anchor or a tag was written.
    pub fn has_begun_content(&self) -> bool {
        self.has_anchor || self.has_tag
    }

    /// `ClearModifiedSettings` (emitterstate.cpp:234).
    pub fn clear_modified_settings(&mut self) {
        self.modified_settings.clear(&mut self.settings);
    }

    /// `RestoreGlobalModifiedSettings` (emitterstate.cpp:236-238).
    pub fn restore_global_modified_settings(&mut self) {
        self.global_modified_settings.restore(&mut self.settings);
    }

    /// `_Set` (emitterstate.h:198-213). A global change also records the new value, so that
    /// restoring the global changes returns to it.
    fn set(&mut self, key: SettingKey, value: SettingValue, scope: FmtScope) {
        match scope {
            FmtScope::Local => {
                let change = self.settings.set(key, value);
                self.modified_settings.push(change);
            }
            FmtScope::Global => {
                self.settings.set(key, value);
                let change = self.settings.set(key, value);
                self.global_modified_settings.push(change);
            }
        }
    }

    fn set_manip(&mut self, key: SettingKey, value: EmitterManip, scope: FmtScope) {
        self.set(key, SettingValue::Manip(value), scope);
    }

    /// `SetLocalValue` (emitterstate.cpp:39-53): tries every setter; the ones the value
    /// means something to accept it.
    pub fn set_local_value(&mut self, value: EmitterManip) {
        self.set_output_charset(value, FmtScope::Local);
        self.set_string_format(value, FmtScope::Local);
        self.set_bool_format(value, FmtScope::Local);
        self.set_bool_case_format(value, FmtScope::Local);
        self.set_bool_length_format(value, FmtScope::Local);
        self.set_null_format(value, FmtScope::Local);
        self.set_int_format(value, FmtScope::Local);
        self.set_flow_type(GroupType::Seq, value, FmtScope::Local);
        self.set_flow_type(GroupType::Map, value, FmtScope::Local);
        self.set_map_key_format(value, FmtScope::Local);
    }

    /// `SetOutputCharset` (emitterstate.cpp:240-251).
    pub fn set_output_charset(&mut self, value: EmitterManip, scope: FmtScope) -> bool {
        use EmitterManip::*;
        match value {
            EmitNonAscii | EscapeNonAscii | EscapeAsJson => {
                self.set_manip(SettingKey::Charset, value, scope);
                true
            }
            _ => false,
        }
    }

    /// `GetOutputCharset`
    pub fn output_charset(&self) -> EmitterManip {
        self.settings.charset
    }

    /// `SetStringFormat` (emitterstate.cpp:253-264).
    pub fn set_string_format(&mut self, value: EmitterManip, scope: FmtScope) -> bool {
        use EmitterManip::*;
        match value {
            Auto | SingleQuoted | DoubleQuoted | Literal => {
                self.set_manip(SettingKey::StrFmt, value, scope);
                true
            }
            _ => false,
        }
    }

    /// `GetStringFormat`
    pub fn string_format(&self) -> EmitterManip {
        self.settings.str_fmt
    }

    /// `SetBoolFormat` (emitterstate.cpp:266-276).
    pub fn set_bool_format(&mut self, value: EmitterManip, scope: FmtScope) -> bool {
        use EmitterManip::*;
        match value {
            OnOffBool | TrueFalseBool | YesNoBool => {
                self.set_manip(SettingKey::BoolFmt, value, scope);
                true
            }
            _ => false,
        }
    }

    /// `GetBoolFormat`
    pub fn bool_format(&self) -> EmitterManip {
        self.settings.bool_fmt
    }

    /// `SetBoolLengthFormat` (emitterstate.cpp:278-288).
    pub fn set_bool_length_format(&mut self, value: EmitterManip, scope: FmtScope) -> bool {
        use EmitterManip::*;
        match value {
            LongBool | ShortBool => {
                self.set_manip(SettingKey::BoolLengthFmt, value, scope);
                true
            }
            _ => false,
        }
    }

    /// `GetBoolLengthFormat`
    pub fn bool_length_format(&self) -> EmitterManip {
        self.settings.bool_length_fmt
    }

    /// `SetBoolCaseFormat` (emitterstate.cpp:290-301).
    pub fn set_bool_case_format(&mut self, value: EmitterManip, scope: FmtScope) -> bool {
        use EmitterManip::*;
        match value {
            UpperCase | LowerCase | CamelCase => {
                self.set_manip(SettingKey::BoolCaseFmt, value, scope);
                true
            }
            _ => false,
        }
    }

    /// `GetBoolCaseFormat`
    pub fn bool_case_format(&self) -> EmitterManip {
        self.settings.bool_case_fmt
    }

    /// `SetNullFormat` (emitterstate.cpp:303-314).
    pub fn set_null_format(&mut self, value: EmitterManip, scope: FmtScope) -> bool {
        use EmitterManip::*;
        match value {
            LowerNull | UpperNull | CamelNull | TildeNull => {
                self.set_manip(SettingKey::NullFmt, value, scope);
                true
            }
            _ => false,
        }
    }

    /// `GetNullFormat`
    pub fn null_format(&self) -> EmitterManip {
        self.settings.null_fmt
    }

    /// `SetIntFormat` (emitterstate.cpp:316-326).
    pub fn set_int_format(&mut self, value: EmitterManip, scope: FmtScope) -> bool {
        use EmitterManip::*;
        match value {
            Dec | Hex | Oct => {
                self.set_manip(SettingKey::IntFmt, value, scope);
                true
            }
            _ => false,
        }
    }

    /// `GetIntFormat`
    pub fn int_format(&self) -> EmitterManip {
        self.settings.int_fmt
    }

    /// `SetIndent` (emitterstate.cpp:328-334): at least 2.
    pub fn set_indent(&mut self, value: usize, scope: FmtScope) -> bool {
        if value <= 1 {
            return false;
        }
        self.set(SettingKey::Indent, SettingValue::Size(value), scope);
        true
    }

    /// `GetIndent`
    pub fn indent(&self) -> usize {
        self.settings.indent
    }

    /// `SetPreCommentIndent` (emitterstate.cpp:336-343).
    pub fn set_pre_comment_indent(&mut self, value: usize, scope: FmtScope) -> bool {
        if value == 0 {
            return false;
        }
        self.set(
            SettingKey::PreCommentIndent,
            SettingValue::Size(value),
            scope,
        );
        true
    }

    /// `GetPreCommentIndent`
    pub fn pre_comment_indent(&self) -> usize {
        self.settings.pre_comment_indent
    }

    /// `SetPostCommentIndent` (emitterstate.cpp:345-352).
    pub fn set_post_comment_indent(&mut self, value: usize, scope: FmtScope) -> bool {
        if value == 0 {
            return false;
        }
        self.set(
            SettingKey::PostCommentIndent,
            SettingValue::Size(value),
            scope,
        );
        true
    }

    /// `GetPostCommentIndent`
    pub fn post_comment_indent(&self) -> usize {
        self.settings.post_comment_indent
    }

    /// `SetFlowType` (emitterstate.cpp:354-364).
    pub fn set_flow_type(
        &mut self,
        group_type: GroupType,
        value: EmitterManip,
        scope: FmtScope,
    ) -> bool {
        match value {
            EmitterManip::Block | EmitterManip::Flow => {
                let key = if group_type == GroupType::Seq {
                    SettingKey::SeqFmt
                } else {
                    SettingKey::MapFmt
                };
                self.set_manip(key, value, scope);
                true
            }
            _ => false,
        }
    }

    /// `GetFlowType` (emitterstate.cpp:366-373): always `Flow` inside a flow group.
    pub fn flow_type(&self, group_type: GroupType) -> EmitterManip {
        if self.cur_group_flow_type() == FlowType::Flow {
            return EmitterManip::Flow;
        }
        if group_type == GroupType::Seq {
            self.settings.seq_fmt
        } else {
            self.settings.map_fmt
        }
    }

    /// `SetMapKeyFormat` (emitterstate.cpp:375-384).
    pub fn set_map_key_format(&mut self, value: EmitterManip, scope: FmtScope) -> bool {
        match value {
            EmitterManip::Auto | EmitterManip::LongKey => {
                self.set_manip(SettingKey::MapKeyFmt, value, scope);
                true
            }
            _ => false,
        }
    }

    /// `GetMapKeyFormat`
    pub fn map_key_format(&self) -> EmitterManip {
        self.settings.map_key_fmt
    }

    /// `SetFloatPrecision` (emitterstate.cpp:386-391): at most `max_digits10` (9).
    pub fn set_float_precision(&mut self, value: usize, scope: FmtScope) -> bool {
        if value > 9 {
            return false;
        }
        self.set(SettingKey::FloatPrecision, SettingValue::Size(value), scope);
        true
    }

    /// `GetFloatPrecision`
    pub fn float_precision(&self) -> usize {
        self.settings.float_precision
    }

    /// `SetDoublePrecision` (emitterstate.cpp:393-399): at most `max_digits10` (17).
    pub fn set_double_precision(&mut self, value: usize, scope: FmtScope) -> bool {
        if value > 17 {
            return false;
        }
        self.set(
            SettingKey::DoublePrecision,
            SettingValue::Size(value),
            scope,
        );
        true
    }

    /// `GetDoublePrecision`
    pub fn double_precision(&self) -> usize {
        self.settings.double_precision
    }
}
