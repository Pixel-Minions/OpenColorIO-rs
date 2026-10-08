// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The group transform: a port of `src/OpenColorIO/transforms/GroupTransform.h` and
//! `GroupTransform.cpp` @ v2.5.2, without `write` and its format registry queries (the file
//! writers, Phase 4) or `CollectContextVariables` (the config, Phase 3).

use std::fmt;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::format_metadata::FormatMetadataImpl;
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{
    TransformDirection, combine_transform_directions, transform_direction_to_string,
};

use crate::config::Config;
use crate::context::Context;
use crate::transform::{Transform, build_ops, validate_direction};

/// A list of transforms, applied in order (in reverse order, each inverted, when the group is
/// inverse), with format metadata.
///
/// The group owns its children (the owner's API decision, 2026-10-01): a copy copies them,
/// where upstream's `createEditableCopy` shares them (docs/improvements.md, I-11).
///
/// Port of `GroupTransform` and `GroupTransformImpl` (include/OpenColorIO/
/// OpenColorTransforms.h:1530-1589, src/OpenColorIO/transforms/GroupTransform.h,
/// GroupTransform.cpp:17-112 @ v2.5.2).
#[derive(Debug)]
pub struct GroupTransform {
    /// `m_metadata`.
    metadata: FormatMetadataImpl,
    /// `m_dir`.
    dir: TransformDirection,
    /// `m_vec`.
    transforms: Vec<Transform>,
}

/// A copy of the group and of its children, nested groups included. It walks the nested groups
/// with a stack of its own: a group nested hundreds deep (a config's, U-60) copies without
/// overflowing the caller's stack, where a derived `clone` takes about 4 KiB of stack per
/// level at opt-level 0.
impl Clone for GroupTransform {
    fn clone(&self) -> Self {
        // Each level: the group being copied, its copy so far, and its next child.
        let mut stack = vec![(self, self.empty_copy(), 0)];
        loop {
            let (source, copy, next) = stack.last_mut().expect("a group");
            if let Some(child) = source.transforms.get(*next) {
                *next += 1;
                match child {
                    Transform::Group(group) => {
                        let level = (group, group.empty_copy(), 0);
                        stack.push(level);
                    }
                    other => copy.transforms.push(other.clone()),
                }
                continue;
            }
            let (_, done, _) = stack.pop().expect("a group");
            match stack.last_mut() {
                Some((_, parent, _)) => parent.transforms.push(Transform::Group(done)),
                None => return done,
            }
        }
    }
}

impl Default for GroupTransform {
    fn default() -> Self {
        Self::new()
    }
}

impl GroupTransform {
    /// An empty forward group with empty metadata.
    ///
    /// Port of `GroupTransform::Create` and `GroupTransformImpl::GroupTransformImpl`
    /// (GroupTransform.cpp:17-20, 30-34 @ v2.5.2).
    #[doc(alias = "Create")]
    pub fn new() -> GroupTransform {
        GroupTransform {
            metadata: FormatMetadataImpl::default(),
            dir: TransformDirection::Forward,
            transforms: Vec::new(),
        }
    }

    /// A copy of the group's metadata and direction, without its children.
    fn empty_copy(&self) -> GroupTransform {
        GroupTransform {
            metadata: self.metadata.clone(),
            dir: self.dir,
            transforms: Vec::with_capacity(self.transforms.len()),
        }
    }

    /// Port of `GroupTransformImpl::getDirection` (GroupTransform.cpp:46-49 @ v2.5.2).
    #[doc(alias = "getDirection")]
    pub fn direction(&self) -> TransformDirection {
        self.dir
    }

    /// Port of `GroupTransformImpl::setDirection` (GroupTransform.cpp:51-54 @ v2.5.2).
    #[doc(alias = "setDirection")]
    pub fn set_direction(&mut self, dir: TransformDirection) {
        self.dir = dir;
    }

    /// Checks the group's direction, then each child: the first child's error as it is.
    ///
    /// Port of `GroupTransformImpl::validate` (GroupTransform.cpp:56-73 @ v2.5.2).
    pub fn validate(&self) -> Result<()> {
        if let Err(ex) = validate_direction(self.dir) {
            return Err(Exception::new(
                [b"GroupTransform validation failed: ".as_slice(), ex.what()].concat(),
            ));
        }

        for val in &self.transforms {
            val.validate()?;
        }
        Ok(())
    }

    /// Port of `GroupTransformImpl::getFormatMetadata() const` (GroupTransform.h:40-43 @ v2.5.2).
    #[doc(alias = "getFormatMetadata")]
    pub fn format_metadata(&self) -> &FormatMetadataImpl {
        &self.metadata
    }

    /// Port of `GroupTransformImpl::getFormatMetadata()` (GroupTransform.h:35-38 @ v2.5.2).
    #[doc(alias = "getFormatMetadata")]
    pub fn format_metadata_mut(&mut self) -> &mut FormatMetadataImpl {
        &mut self.metadata
    }

    /// The number of children.
    ///
    /// Port of `GroupTransformImpl::getNumTransforms` (GroupTransform.cpp:75-78 @ v2.5.2).
    #[doc(alias = "getNumTransforms")]
    pub fn num_transforms(&self) -> i32 {
        self.transforms.len() as i32
    }

    /// The error of an index past the children (GroupTransform.cpp:82-87, 94-99 @ v2.5.2).
    fn invalid_index(index: i32) -> Exception {
        Exception::new(format!("Invalid transform index {index}."))
    }

    /// Child `index`: "Invalid transform index <index>." outside the children.
    ///
    /// Port of `GroupTransformImpl::getTransform() const` (GroupTransform.cpp:80-90 @ v2.5.2).
    #[doc(alias = "getTransform")]
    pub fn transform(&self, index: i32) -> Result<&Transform> {
        if index < 0 || index >= self.transforms.len() as i32 {
            return Err(Self::invalid_index(index));
        }
        Ok(&self.transforms[index as usize])
    }

    /// Child `index`, to change: "Invalid transform index <index>." outside the children.
    ///
    /// Port of `GroupTransformImpl::getTransform()` (GroupTransform.cpp:92-102 @ v2.5.2).
    #[doc(alias = "getTransform")]
    pub fn transform_mut(&mut self, index: i32) -> Result<&mut Transform> {
        if index < 0 || index >= self.transforms.len() as i32 {
            return Err(Self::invalid_index(index));
        }
        Ok(&mut self.transforms[index as usize])
    }

    /// Adds `transform` after the children.
    ///
    /// Port of `GroupTransformImpl::appendTransform` (GroupTransform.cpp:104-107 @ v2.5.2).
    #[doc(alias = "appendTransform")]
    pub fn append_transform(&mut self, transform: Transform) {
        self.transforms.push(transform);
    }

    /// Adds `transform` before the children.
    ///
    /// Port of `GroupTransformImpl::prependTransform` (GroupTransform.cpp:109-112 @ v2.5.2).
    #[doc(alias = "prependTransform")]
    pub fn prepend_transform(&mut self, transform: Transform) {
        self.transforms.insert(0, transform);
    }
}

impl GroupTransform {
    /// Writes the group's text to `os`, its children's included, on the same stream: a child
    /// that changes the stream's state changes it for the children after it (a
    /// MatrixTransform's precision, docs/improvements.md, I-73).
    ///
    /// Port of `operator<<(std::ostream &, const GroupTransform &)` (GroupTransform.cpp:
    /// 156-168 @ v2.5.2).
    pub(crate) fn write_text(&self, os: &mut OStringStream) {
        os.put_str("<GroupTransform ");
        os.put_str("direction=");
        os.put_str(transform_direction_to_string(self.direction()));
        os.put_str(", ");
        os.put_str("transforms=");
        for transform in &self.transforms {
            os.put_str("\n        ");
            transform.write_text(os);
        }
        os.put_str(">");
    }
}

impl fmt::Display for GroupTransform {
    /// `<GroupTransform direction=<dir>, transforms=`, each child on its own line after eight
    /// spaces, then `>`.
    ///
    /// Port of `operator<<(std::ostream &, const GroupTransform &)` (GroupTransform.cpp:
    /// 156-168 @ v2.5.2), on a new stream.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_text(&mut os);
        f.write_str(&os.to_string_lossy())
    }
}

/// Appends the ops of `group_transform`'s children in the direction `dir` combined with the
/// group's: in order forward, in reverse order inverted. The first group's metadata becomes the
/// ops' metadata.
///
/// Port of `BuildGroupOps` (src/OpenColorIO/transforms/GroupTransform.cpp:172-204 @ v2.5.2).
pub(crate) fn build_group_ops(
    ops: &mut OpVec,
    config: &Config,
    context: &Context,
    group_transform: &GroupTransform,
    dir: TransformDirection,
) -> Result<()> {
    if ops.is_empty() {
        // If group is the first transform, copy the group metadata.
        *ops.get_format_metadata_mut() = group_transform.format_metadata().clone();
    }

    let combined_dir = combine_transform_directions(dir, group_transform.direction());
    match combined_dir {
        TransformDirection::Forward => {
            for child in &group_transform.transforms {
                build_ops(ops, config, context, child, TransformDirection::Forward)?;
            }
        }
        TransformDirection::Inverse => {
            for child in group_transform.transforms.iter().rev() {
                build_ops(ops, config, context, child, TransformDirection::Inverse)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "group_transform_tests.rs"]
mod tests;
