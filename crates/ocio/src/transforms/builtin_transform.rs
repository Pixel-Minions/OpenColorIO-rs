// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The built-in transform: a port of `src/OpenColorIO/transforms/BuiltinTransform.h` and
//! `BuiltinTransform.cpp` @ v2.5.2, with its op builder `BuildBuiltinOps`.

use std::fmt;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::op::OpVec;
use ocio_ops::open_color_types::{
    TransformDirection, combine_transform_directions, transform_direction_to_string,
};
use ocio_ops::platform::strcasecmp;
use ocio_ops::utils::string_utils::c_str;

use crate::transform::{put_c_str, validate_direction};
use crate::transforms::builtins::builtin_transform_registry::{
    BuiltinTransformRegistry, create_builtin_transform_ops,
};

/// One of the transforms OCIO knows how to build, chosen by its style in the
/// [`BuiltinTransformRegistry`]: a FileTransform without the file.
///
/// A copy is upstream's `createEditableCopy`. Upstream gives the class no `equals`.
///
/// Port of `BuiltinTransform` (include/OpenColorIO/OpenColorTransforms.h:198-224 @ v2.5.2) and
/// `BuiltinTransformImpl` (src/OpenColorIO/transforms/BuiltinTransform.h:15-40,
/// BuiltinTransform.cpp:18-78 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct BuiltinTransform {
    /// `m_direction`.
    direction: TransformDirection,
    /// `m_transformIndex`: index of the built-in transform in the registry.
    transform_index: usize,
}

impl Default for BuiltinTransform {
    fn default() -> Self {
        Self::new()
    }
}

impl BuiltinTransform {
    /// The registry's first built-in transform, `IDENTITY`, forward.
    ///
    /// Port of `BuiltinTransform::Create` (BuiltinTransform.cpp:18-21 @ v2.5.2) and the
    /// members' defaults (BuiltinTransform.h:38-39).
    #[doc(alias = "Create")]
    pub fn new() -> BuiltinTransform {
        BuiltinTransform {
            direction: TransformDirection::Forward,
            transform_index: 0,
        }
    }

    /// Port of `BuiltinTransformImpl::getDirection` (BuiltinTransform.cpp:38-41 @ v2.5.2).
    #[doc(alias = "getDirection")]
    pub fn direction(&self) -> TransformDirection {
        self.direction
    }

    /// Port of `BuiltinTransformImpl::setDirection` (BuiltinTransform.cpp:43-46 @ v2.5.2).
    #[doc(alias = "setDirection")]
    pub fn set_direction(&mut self, dir: TransformDirection) {
        self.direction = dir;
    }

    /// Checks the direction. The class has no other check: a style is checked when it is set.
    ///
    /// Port of `Transform::validate` (src/OpenColorIO/Transform.cpp:30-40 @ v2.5.2), which the
    /// class doesn't override.
    pub fn validate(&self) -> Result<()> {
        validate_direction(self.direction)
    }

    /// The style: the registry's spelling of it.
    ///
    /// Port of `BuiltinTransformImpl::getStyle` (BuiltinTransform.cpp:48-51 @ v2.5.2).
    #[doc(alias = "getStyle")]
    pub fn style(&self) -> &'static [u8] {
        BuiltinTransformRegistry::get()
            .builtin_style(self.transform_index)
            .expect("the index of a registered style")
    }

    /// The description of the style, possibly empty.
    ///
    /// Port of `BuiltinTransformImpl::getDescription` (BuiltinTransform.cpp:53-56 @ v2.5.2).
    #[doc(alias = "getDescription")]
    pub fn description(&self) -> &'static [u8] {
        BuiltinTransformRegistry::get()
            .builtin_description(self.transform_index)
            .expect("the index of a registered style")
    }

    /// The style's index in the registry.
    ///
    /// Port of `BuiltinTransformImpl::getTransformIndex` (BuiltinTransform.cpp:58-61 @ v2.5.2).
    pub(crate) fn transform_index(&self) -> usize {
        self.transform_index
    }

    /// Selects the registry's first built-in transform whose style matches `style` (up to its
    /// first NUL), ignoring case; or "BuiltinTransform: invalid built-in transform style
    /// '<style>'.", and the transform is unchanged.
    ///
    /// Port of `BuiltinTransformImpl::setStyle` (BuiltinTransform.cpp:63-78 @ v2.5.2).
    #[doc(alias = "setStyle")]
    pub fn set_style(&mut self, style: impl AsRef<[u8]>) -> Result<()> {
        let style = c_str(style.as_ref());
        let registry = BuiltinTransformRegistry::get();
        for index in 0..registry.num_builtins() {
            if strcasecmp(style, registry.builtin_style(index)?).is_eq() {
                self.transform_index = index;
                return Ok(());
            }
        }

        Err(Exception::new(format!(
            "BuiltinTransform: invalid built-in transform style '{}'.",
            String::from_utf8_lossy(style)
        )))
    }

    /// Writes the transform's text to `os`.
    ///
    /// Port of `operator<<(std::ostream &, const BuiltinTransform &)` (BuiltinTransform.cpp:
    /// 93-100 @ v2.5.2).
    pub(crate) fn write_text(&self, os: &mut OStringStream) {
        os.put_str("<BuiltinTransform");
        os.put_str(" direction = ");
        os.put_str(transform_direction_to_string(self.direction()));
        os.put_str(", style = ");
        put_c_str(os, self.style());
        os.put_str(">");
    }
}

impl fmt::Display for BuiltinTransform {
    /// `<BuiltinTransform direction = <dir>, style = <style>>`.
    ///
    /// Port of `operator<<(std::ostream &, const BuiltinTransform &)` (BuiltinTransform.cpp:
    /// 93-100 @ v2.5.2), on a new stream.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_text(&mut os);
        f.write_str(os.str())
    }
}

/// Appends the ops of the built-in transform `transform` in the direction `dir`, combined with
/// its own.
///
/// Port of `BuildBuiltinOps` (BuiltinTransform.cpp:81-90 @ v2.5.2).
pub(crate) fn build_builtin_ops(
    ops: &mut OpVec,
    transform: &BuiltinTransform,
    dir: TransformDirection,
) -> Result<()> {
    let combined_dir = combine_transform_directions(dir, transform.direction());

    create_builtin_transform_ops(ops, transform.transform_index(), combined_dir)
}

#[cfg(test)]
#[path = "builtin_transform_tests.rs"]
mod tests;
