// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.
// Ported from yaml-cpp 0.8.0 (MIT License, Copyright (c) 2008-2015 Jesse Beder); see mod.rs.

//! Port of yaml-cpp 0.8.0's manipulators and node kinds (include/yaml-cpp/emittermanip.h,
//! emitterdef.h, binary.h's `Binary`, null.h's `_Null`).

/// Port of `YAML::EMITTER_MANIP` (emittermanip.h:13-70).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmitterManip {
    // general manipulators
    Auto,
    TagByKind,
    Newline,

    // output character set
    EmitNonAscii,
    EscapeNonAscii,
    EscapeAsJson,

    // string manipulators (and Auto)
    SingleQuoted,
    DoubleQuoted,
    Literal,

    // null manipulators
    LowerNull,
    UpperNull,
    CamelNull,
    TildeNull,

    // bool manipulators
    YesNoBool,
    TrueFalseBool,
    OnOffBool,
    UpperCase,
    LowerCase,
    CamelCase,
    LongBool,
    ShortBool,

    // int manipulators
    Dec,
    Hex,
    Oct,

    // document manipulators
    BeginDoc,
    EndDoc,

    // sequence manipulators
    BeginSeq,
    EndSeq,
    Flow,
    Block,

    // map manipulators (and Flow, Block, Auto)
    BeginMap,
    EndMap,
    Key,
    Value,
    LongKey,
}

/// Port of `YAML::_Indent` (emittermanip.h:72-75).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Indent(pub i32);

/// Port of `YAML::_Alias` (emittermanip.h:79-82). The name is a C++ `std::string`: bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alias(pub Vec<u8>);

/// Port of `YAML::_Anchor` (emittermanip.h:86-89). The name is bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anchor(pub Vec<u8>);

/// Port of `YAML::_Tag::Type` (emittermanip.h:94-96).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagType {
    /// `!<content>`
    Verbatim,
    /// `!content`
    PrimaryHandle,
    /// `!prefix!content`
    NamedHandle,
}

/// Port of `YAML::_Tag` (emittermanip.h:93-104). Prefix and content are C++ `std::string`s:
/// bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    /// The handle prefix (named handles only).
    pub prefix: Vec<u8>,
    /// The tag.
    pub content: Vec<u8>,
    /// How it is written.
    pub tag_type: TagType,
}

/// Port of `YAML::VerbatimTag` (emittermanip.h:106-108).
pub fn verbatim_tag(content: impl AsRef<[u8]>) -> Tag {
    Tag {
        prefix: Vec::new(),
        content: content.as_ref().to_vec(),
        tag_type: TagType::Verbatim,
    }
}

/// Port of `YAML::LocalTag(const std::string &)` (emittermanip.h:110-112).
pub fn local_tag(content: impl AsRef<[u8]>) -> Tag {
    Tag {
        prefix: Vec::new(),
        content: content.as_ref().to_vec(),
        tag_type: TagType::PrimaryHandle,
    }
}

/// Port of `YAML::LocalTag(const std::string &, const std::string)` (emittermanip.h:
/// 114-116).
pub fn local_tag_with_prefix(prefix: impl AsRef<[u8]>, content: impl AsRef<[u8]>) -> Tag {
    Tag {
        prefix: prefix.as_ref().to_vec(),
        content: content.as_ref().to_vec(),
        tag_type: TagType::NamedHandle,
    }
}

/// Port of `YAML::SecondaryTag` (emittermanip.h:118-120).
pub fn secondary_tag(content: impl AsRef<[u8]>) -> Tag {
    Tag {
        prefix: Vec::new(),
        content: content.as_ref().to_vec(),
        tag_type: TagType::NamedHandle,
    }
}

/// Port of `YAML::_Comment` (emittermanip.h:122-125). The text is bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment(pub Vec<u8>);

/// Port of `YAML::_Precision` (emittermanip.h:129-135); a negative value leaves that
/// precision alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Precision {
    /// Digits for `float`.
    pub float_precision: i32,
    /// Digits for `double`.
    pub double_precision: i32,
}

/// Port of `YAML::FloatPrecision` (emittermanip.h:137).
pub fn float_precision(n: i32) -> Precision {
    Precision {
        float_precision: n,
        double_precision: -1,
    }
}

/// Port of `YAML::DoublePrecision` (emittermanip.h:139).
pub fn double_precision(n: i32) -> Precision {
    Precision {
        float_precision: -1,
        double_precision: n,
    }
}

/// Port of `YAML::Precision` (emittermanip.h:141).
pub fn precision(n: i32) -> Precision {
    Precision {
        float_precision: n,
        double_precision: n,
    }
}

/// Port of `YAML::_Null` (null.h): written as the null format's name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Null;

/// Port of `YAML::Binary` (binary.h) as a borrowed byte string: written as a base64
/// `!!binary` scalar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Binary<'a>(pub &'a [u8]);

/// Port of `YAML::EmitterNodeType::value` (emitterdef.h:11-13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmitterNodeType {
    NoType,
    Property,
    Scalar,
    FlowSeq,
    BlockSeq,
    FlowMap,
    BlockMap,
}
