// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.8.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! Port of `YAML::EventHandler` (include/yaml-cpp/eventhandler.h), `anchor_t`
//! (include/yaml-cpp/anchor.h), `EmitterStyle` (include/yaml-cpp/emitterstyle.h) and
//! `IsNullString` (include/yaml-cpp/null.h, src/null.cpp), yaml-cpp 0.8.0: the events the
//! parser reports, one document at a time.

use super::mark::Mark;

/// `YAML::anchor_t` (anchor.h:11): anchors are numbered from 1 in each document.
pub type Anchor = usize;

/// `YAML::NullAnchor` (anchor.h:12).
pub const NULL_ANCHOR: Anchor = 0;

/// `EmitterStyle::value` (emitterstyle.h:9-11): how a collection was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EmitterStyle {
    #[default]
    Default,
    Block,
    Flow,
}

/// Port of `YAML::EventHandler` (eventhandler.h:17-40). Tags and scalars are bytes.
pub trait EventHandler {
    fn on_document_start(&mut self, mark: Mark);
    fn on_document_end(&mut self);

    fn on_null(&mut self, mark: Mark, anchor: Anchor);
    fn on_alias(&mut self, mark: Mark, anchor: Anchor);
    fn on_scalar(&mut self, mark: Mark, tag: &[u8], anchor: Anchor, value: &[u8]);

    fn on_sequence_start(&mut self, mark: Mark, tag: &[u8], anchor: Anchor, style: EmitterStyle);
    fn on_sequence_end(&mut self);

    fn on_map_start(&mut self, mark: Mark, tag: &[u8], anchor: Anchor, style: EmitterStyle);
    fn on_map_end(&mut self);

    /// `OnAnchor` (eventhandler.h:35-38): an empty default implementation, for compatibility.
    fn on_anchor(&mut self, _mark: Mark, _anchor_name: &[u8]) {}
}

/// `IsNullString(const std::string&)` (null.cpp:6-9): the plain scalars that mean null.
pub fn is_null_string(s: &[u8]) -> bool {
    s.is_empty() || s == b"~" || s == b"null" || s == b"Null" || s == b"NULL"
}
