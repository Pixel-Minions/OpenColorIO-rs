// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The look transform: a port of the class in `src/OpenColorIO/transforms/LookTransform.cpp`
//! @ v2.5.2. Its op builder (`BuildLookOps`), `GetLooksResultColorSpace` and its
//! `CollectContextVariables` come with WP 3.2.

use std::fmt;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::open_color_types::{TransformDirection, transform_direction_to_string};
use ocio_ops::utils::string_utils::c_str;

use crate::transform::{put_c_str, validate_direction};

/// Looks applied between two of a config's color spaces, by name: a list of look names, each
/// optionally prefixed with `+` (forward) or `-` (inverse), separated by commas or colons.
///
/// A copy is upstream's `createEditableCopy`. Upstream gives the class no `equals`.
///
/// Port of `LookTransform` and its `Impl` (include/OpenColorIO/OpenColorTransforms.h:
/// 1740-1798, src/OpenColorIO/transforms/LookTransform.cpp:18-147 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct LookTransform {
    /// `m_dir`.
    dir: TransformDirection,
    /// `m_skipColorSpaceConversion`.
    skip_color_space_conversion: bool,
    /// `m_src`.
    src: Vec<u8>,
    /// `m_dst`.
    dst: Vec<u8>,
    /// `m_looks`.
    looks: Vec<u8>,
}

impl Default for LookTransform {
    fn default() -> Self {
        Self::new()
    }
}

impl LookTransform {
    /// A forward transform with empty names and no looks, which converts between the color
    /// spaces.
    ///
    /// Port of `LookTransform::Create` and `Impl` (LookTransform.cpp:18-60 @ v2.5.2).
    #[doc(alias = "Create")]
    pub fn new() -> LookTransform {
        LookTransform {
            dir: TransformDirection::Forward,
            skip_color_space_conversion: false,
            src: Vec::new(),
            dst: Vec::new(),
            looks: Vec::new(),
        }
    }

    /// Port of `LookTransform::getDirection` (LookTransform.cpp:75-78 @ v2.5.2).
    #[doc(alias = "getDirection")]
    pub fn direction(&self) -> TransformDirection {
        self.dir
    }

    /// Port of `LookTransform::setDirection` (LookTransform.cpp:80-83 @ v2.5.2).
    #[doc(alias = "setDirection")]
    pub fn set_direction(&mut self, dir: TransformDirection) {
        self.dir = dir;
    }

    /// Checks the direction ("LookTransform validation failed: " and its error), then that both
    /// color spaces are set. The looks may be empty.
    ///
    /// Port of `LookTransform::validate` (LookTransform.cpp:85-107 @ v2.5.2).
    pub fn validate(&self) -> Result<()> {
        if let Err(ex) = validate_direction(self.dir) {
            return Err(Exception::new(format!(
                "LookTransform validation failed: {}",
                ex.message()
            )));
        }

        if self.src.is_empty() {
            return Err(Exception::new(
                "LookTransform: empty source color space name.",
            ));
        }

        if self.dst.is_empty() {
            return Err(Exception::new(
                "LookTransform: empty destination color space name.",
            ));
        }
        Ok(())
    }

    /// The source color space's name.
    ///
    /// Port of `LookTransform::getSrc` (LookTransform.cpp:109-112 @ v2.5.2).
    #[doc(alias = "getSrc")]
    pub fn src(&self) -> &[u8] {
        &self.src
    }

    /// Sets the source color space's name, up to its first NUL (a C string).
    ///
    /// Port of `LookTransform::setSrc` (LookTransform.cpp:114-117 @ v2.5.2). Its null pointer,
    /// which sets an empty name, is an empty slice here.
    #[doc(alias = "setSrc")]
    pub fn set_src(&mut self, src: impl AsRef<[u8]>) {
        self.src = c_str(src.as_ref()).to_vec();
    }

    /// The destination color space's name.
    ///
    /// Port of `LookTransform::getDst` (LookTransform.cpp:119-122 @ v2.5.2).
    #[doc(alias = "getDst")]
    pub fn dst(&self) -> &[u8] {
        &self.dst
    }

    /// Sets the destination color space's name, up to its first NUL.
    ///
    /// Port of `LookTransform::setDst` (LookTransform.cpp:124-127 @ v2.5.2).
    #[doc(alias = "setDst")]
    pub fn set_dst(&mut self, dst: impl AsRef<[u8]>) {
        self.dst = c_str(dst.as_ref()).to_vec();
    }

    /// Sets the looks, up to the first NUL: look names separated by commas or colons, each
    /// optionally prefixed with `+` or `-`.
    ///
    /// Port of `LookTransform::setLooks` (LookTransform.cpp:129-132 @ v2.5.2).
    #[doc(alias = "setLooks")]
    pub fn set_looks(&mut self, looks: impl AsRef<[u8]>) {
        self.looks = c_str(looks.as_ref()).to_vec();
    }

    /// The looks, as they were set.
    ///
    /// Port of `LookTransform::getLooks` (LookTransform.cpp:134-137 @ v2.5.2).
    #[doc(alias = "getLooks")]
    pub fn looks(&self) -> &[u8] {
        &self.looks
    }

    /// Whether only the looks are applied, without converting from the source color space and
    /// to the destination.
    ///
    /// Port of `LookTransform::setSkipColorSpaceConversion` (LookTransform.cpp:139-142 @
    /// v2.5.2).
    #[doc(alias = "setSkipColorSpaceConversion")]
    pub fn set_skip_color_space_conversion(&mut self, skip: bool) {
        self.skip_color_space_conversion = skip;
    }

    /// Port of `LookTransform::getSkipColorSpaceConversion` (LookTransform.cpp:144-147 @
    /// v2.5.2).
    #[doc(alias = "getSkipColorSpaceConversion")]
    pub fn skip_color_space_conversion(&self) -> bool {
        self.skip_color_space_conversion
    }

    /// Writes the transform's text to `os`.
    ///
    /// Port of `operator<<(std::ostream &, const LookTransform &)` (LookTransform.cpp:181-191 @
    /// v2.5.2).
    pub(crate) fn write_text(&self, os: &mut OStringStream) {
        os.put_str("<LookTransform");
        os.put_str(" direction=");
        os.put_str(transform_direction_to_string(self.direction()));
        os.put_str(", src=");
        put_c_str(os, self.src());
        os.put_str(", dst=");
        put_c_str(os, self.dst());
        os.put_str(", looks=");
        put_c_str(os, self.looks());
        if self.skip_color_space_conversion() {
            os.put_str(", skipCSConversion");
        }
        os.put_str(">");
    }
}

impl fmt::Display for LookTransform {
    /// `<LookTransform direction=<dir>, src=<src>, dst=<dst>, looks=<looks>[,
    /// skipCSConversion]>`.
    ///
    /// Port of `operator<<(std::ostream &, const LookTransform &)` (LookTransform.cpp:181-191 @
    /// v2.5.2), on a new stream.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_text(&mut os);
        f.write_str(os.str())
    }
}

#[cfg(test)]
#[path = "look_transform_tests.rs"]
mod tests;
