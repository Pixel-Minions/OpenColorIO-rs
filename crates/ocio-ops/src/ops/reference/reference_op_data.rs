// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Reference op data: a port of `src/OpenColorIO/ops/reference/ReferenceOpData.h` and
//! `ReferenceOpData.cpp` @ v2.5.2.
//!
//! The CTF reader returns a reference for each `Reference` process node. The file transform
//! replaces it with the ops of the file it names, so no op holds one:
//! [`create_op_vec_from_op_data`](crate::op::create_op_vec_from_op_data) refuses it.

use crate::exception::{Exception, Result};
use crate::format_metadata::FormatMetadataImpl;
use crate::open_color_types::TransformDirection;

/// How a reference names what it stands for.
///
/// Port of `ReferenceStyle` (src/OpenColorIO/ops/reference/ReferenceOpData.h:16-23 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReferenceStyle {
    /// `REF_PATH`: a full or relative path.
    Path,
    /// `REF_ALIAS`: the name of a transform defined in a `synColorConfig.xml` file, which OCIO
    /// doesn't fully implement.
    Alias,
}

/// The data of a reference to the ops of another file.
///
/// Port of `ReferenceOpData` (src/OpenColorIO/ops/reference/ReferenceOpData.h:29-93,
/// ReferenceOpData.cpp:14-71 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct ReferenceOpData {
    /// The `OpData` base's `m_metadata`.
    metadata: FormatMetadataImpl,
    /// `m_referenceStyle`.
    reference_style: ReferenceStyle,
    /// `m_path`.
    path: Vec<u8>,
    /// `m_alias`.
    alias: Vec<u8>,
    /// `m_direction`.
    direction: TransformDirection,
}

impl Default for ReferenceOpData {
    fn default() -> Self {
        Self::new()
    }
}

impl ReferenceOpData {
    /// A forward reference to an empty path.
    ///
    /// Port of `ReferenceOpData::ReferenceOpData()` (ReferenceOpData.cpp:14-17, and the member
    /// initializers of ReferenceOpData.h:87-92 @ v2.5.2).
    pub fn new() -> Self {
        ReferenceOpData {
            metadata: FormatMetadataImpl::default(),
            reference_style: ReferenceStyle::Path,
            path: Vec::new(),
            alias: Vec::new(),
            direction: TransformDirection::Forward,
        }
    }

    /// Nothing to check.
    ///
    /// Port of `ReferenceOpData::validate` (ReferenceOpData.cpp:23-25 @ v2.5.2).
    pub fn validate(&self) {}

    /// Port of `ReferenceOpData::isNoOp` (ReferenceOpData.cpp:27-30 @ v2.5.2).
    pub fn is_no_op(&self) -> bool {
        false
    }

    /// Port of `ReferenceOpData::isIdentity` (ReferenceOpData.cpp:32-35 @ v2.5.2).
    pub fn is_identity(&self) -> bool {
        false
    }

    /// Port of `ReferenceOpData::hasChannelCrosstalk` (ReferenceOpData.cpp:37-40 @ v2.5.2).
    pub fn has_channel_crosstalk(&self) -> bool {
        true
    }

    /// Whether `other` has the same style, direction, and path or alias (whichever the style
    /// uses). The `OpData` base's type comparison is [`crate::op_data::OpData::equals`]'s.
    ///
    /// Port of `ReferenceOpData::equals` (ReferenceOpData.cpp:42-60 @ v2.5.2).
    pub fn equals(&self, other: &ReferenceOpData) -> bool {
        if self.reference_style != other.reference_style {
            return false;
        }
        if self.direction != other.direction {
            return false;
        }
        match self.reference_style {
            ReferenceStyle::Path => self.path == other.path,
            ReferenceStyle::Alias => self.alias == other.alias,
        }
    }

    /// An error: no op holds a reference, so it has no cache ID.
    ///
    /// Port of `ReferenceOpData::getCacheID` (ReferenceOpData.cpp:62-66 @ v2.5.2).
    pub fn get_cache_id(&self) -> Result<Vec<u8>> {
        Err(Exception::new(
            "ReferenceOpData::getCacheID should never be called. ReferenceOpData does not have \
             a corresponding Op",
        ))
    }

    /// Port of `ReferenceOpData::getReferenceStyle` (ReferenceOpData.h:49-52 @ v2.5.2).
    pub fn get_reference_style(&self) -> ReferenceStyle {
        self.reference_style
    }

    /// Port of `ReferenceOpData::getPath` (ReferenceOpData.h:54-57 @ v2.5.2).
    pub fn get_path(&self) -> &[u8] {
        &self.path
    }

    /// Sets the path, and makes the reference a path reference.
    ///
    /// Port of `ReferenceOpData::setPath` (ReferenceOpData.h:59-63 @ v2.5.2).
    pub fn set_path(&mut self, path: &[u8]) {
        self.reference_style = ReferenceStyle::Path;
        self.path = path.to_vec();
    }

    /// Port of `ReferenceOpData::getAlias` (ReferenceOpData.h:65-68 @ v2.5.2).
    pub fn get_alias(&self) -> &[u8] {
        &self.alias
    }

    /// Sets the alias, and makes the reference an alias reference.
    ///
    /// Port of `ReferenceOpData::setAlias` (ReferenceOpData.h:70-74 @ v2.5.2).
    pub fn set_alias(&mut self, alias: &[u8]) {
        self.reference_style = ReferenceStyle::Alias;
        self.alias = alias.to_vec();
    }

    /// Port of `ReferenceOpData::getDirection` (ReferenceOpData.h:76-79 @ v2.5.2).
    pub fn get_direction(&self) -> TransformDirection {
        self.direction
    }

    /// Port of `ReferenceOpData::setDirection` (ReferenceOpData.h:81-84 @ v2.5.2).
    pub fn set_direction(&mut self, dir: TransformDirection) {
        self.direction = dir;
    }

    /// Port of `OpData::getFormatMetadata() const` (src/OpenColorIO/Op.h:164 @ v2.5.2).
    pub fn get_format_metadata(&self) -> &FormatMetadataImpl {
        &self.metadata
    }

    /// Port of `OpData::getFormatMetadata()` (src/OpenColorIO/Op.h:163 @ v2.5.2).
    pub fn get_format_metadata_mut(&mut self) -> &mut FormatMetadataImpl {
        &mut self.metadata
    }
}

impl PartialEq for ReferenceOpData {
    /// Port of `operator==(const ReferenceOpData &, const ReferenceOpData &)`
    /// (ReferenceOpData.cpp:68-71 @ v2.5.2).
    fn eq(&self, other: &ReferenceOpData) -> bool {
        self.equals(other)
    }
}

#[cfg(test)]
#[path = "reference_op_data_tests.rs"]
mod tests;
