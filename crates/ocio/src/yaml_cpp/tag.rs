// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.8.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! Port of `YAML::Tag` (src/tag.h, src/tag.cpp) and `YAML::Directives` (src/directives.h,
//! src/directives.cpp), yaml-cpp 0.8.0: a tag token, resolved against the `%TAG` directives.

use std::collections::BTreeMap;

use super::token::Token;

/// `Tag::TYPE` (tag.h:15-21), stored in the tag token's `data`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagType {
    Verbatim = 0,
    PrimaryHandle = 1,
    SecondaryHandle = 2,
    NamedHandle = 3,
    NonSpecific = 4,
}

/// Port of `YAML::Version` (directives.h:14-17).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version {
    pub is_default: bool,
    pub major: i32,
    pub minor: i32,
}

/// Port of `YAML::Directives` (directives.h:19-26).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Directives {
    pub version: Version,
    pub tags: BTreeMap<Vec<u8>, Vec<u8>>,
}

impl Default for Directives {
    /// `Directives()` (directives.cpp:4): YAML 1.2, no tag handles.
    fn default() -> Self {
        Directives {
            version: Version {
                is_default: true,
                major: 1,
                minor: 2,
            },
            tags: BTreeMap::new(),
        }
    }
}

impl Directives {
    /// `TranslateTagHandle(const std::string&)` (directives.cpp:6-16): the prefix a `%TAG`
    /// directive gave the handle; `!!` defaults to `tag:yaml.org,2002:`, others to themselves.
    pub fn translate_tag_handle(&self, handle: &[u8]) -> Vec<u8> {
        match self.tags.get(handle) {
            Some(prefix) => prefix.clone(),
            None => {
                if handle == b"!!" {
                    return b"tag:yaml.org,2002:".to_vec();
                }
                handle.to_vec()
            }
        }
    }
}

/// Port of `YAML::Tag` (tag.h:13-27).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub ty: TagType,
    pub handle: Vec<u8>,
    pub value: Vec<u8>,
}

impl Tag {
    /// `Tag(const Token&)` (tag.cpp:9-31). The scanner sets the token's `data` to one of the
    /// five types.
    pub fn new(token: &Token) -> Tag {
        let ty = match token.data {
            0 => TagType::Verbatim,
            1 => TagType::PrimaryHandle,
            2 => TagType::SecondaryHandle,
            3 => TagType::NamedHandle,
            _ => TagType::NonSpecific,
        };
        let mut tag = Tag {
            ty,
            handle: Vec::new(),
            value: Vec::new(),
        };
        match ty {
            TagType::Verbatim | TagType::PrimaryHandle | TagType::SecondaryHandle => {
                tag.value = token.value.clone();
            }
            TagType::NamedHandle => {
                tag.handle = token.value.clone();
                tag.value = token.params[0].clone();
            }
            TagType::NonSpecific => {}
        }
        tag
    }

    /// `Translate(const Directives&)` (tag.cpp:33-49): the full tag.
    pub fn translate(&self, directives: &Directives) -> Vec<u8> {
        match self.ty {
            TagType::Verbatim => self.value.clone(),
            TagType::PrimaryHandle => {
                [directives.translate_tag_handle(b"!"), self.value.clone()].concat()
            }
            TagType::SecondaryHandle => {
                [directives.translate_tag_handle(b"!!"), self.value.clone()].concat()
            }
            TagType::NamedHandle => {
                let handle = [b"!".as_slice(), &self.handle, b"!"].concat();
                [directives.translate_tag_handle(&handle), self.value.clone()].concat()
            }
            TagType::NonSpecific => b"!".to_vec(),
        }
    }
}
