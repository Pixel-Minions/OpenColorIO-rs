// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The color space transform: a port of the class in
//! `src/OpenColorIO/transforms/ColorSpaceTransform.cpp` @ v2.5.2. Its op builder
//! (`BuildColorSpaceOps`, the reference-space conversions) and its `CollectContextVariables`
//! come with WP 3.2.

use std::fmt;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::open_color_types::{TransformDirection, transform_direction_to_string};
use ocio_ops::utils::string_utils::c_str;

use crate::transform::{put_bool, put_c_str, validate_direction};

/// A conversion from one of a config's color spaces to another, by name.
///
/// A copy is upstream's `createEditableCopy`. Upstream gives the class no `equals`.
///
/// Port of `ColorSpaceTransform` and its `Impl` (include/OpenColorIO/OpenColorTransforms.h:
/// 333-371, src/OpenColorIO/transforms/ColorSpaceTransform.cpp:18-136 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct ColorSpaceTransform {
    /// `m_dir`.
    dir: TransformDirection,
    /// `m_src`.
    src: Vec<u8>,
    /// `m_dst`.
    dst: Vec<u8>,
    /// `m_dataBypass`.
    data_bypass: bool,
}

impl Default for ColorSpaceTransform {
    fn default() -> Self {
        Self::new()
    }
}

impl ColorSpaceTransform {
    /// A forward transform with empty names, which bypasses data color spaces.
    ///
    /// Port of `ColorSpaceTransform::Create` and `Impl` (ColorSpaceTransform.cpp:18-59 @
    /// v2.5.2).
    #[doc(alias = "Create")]
    pub fn new() -> ColorSpaceTransform {
        ColorSpaceTransform {
            dir: TransformDirection::Forward,
            src: Vec::new(),
            dst: Vec::new(),
            data_bypass: true,
        }
    }

    /// Port of `ColorSpaceTransform::getDirection` (ColorSpaceTransform.cpp:74-77 @ v2.5.2).
    #[doc(alias = "getDirection")]
    pub fn direction(&self) -> TransformDirection {
        self.dir
    }

    /// Port of `ColorSpaceTransform::setDirection` (ColorSpaceTransform.cpp:79-82 @ v2.5.2).
    #[doc(alias = "setDirection")]
    pub fn set_direction(&mut self, dir: TransformDirection) {
        self.dir = dir;
    }

    /// Checks the direction ("ColorSpaceTransform validation failed: " and its error), then
    /// that both names are set.
    ///
    /// Port of `ColorSpaceTransform::validate` (ColorSpaceTransform.cpp:84-106 @ v2.5.2).
    pub fn validate(&self) -> Result<()> {
        if let Err(ex) = validate_direction(self.dir) {
            return Err(Exception::new(format!(
                "ColorSpaceTransform validation failed: {}",
                ex.message()
            )));
        }

        if self.src.is_empty() {
            return Err(Exception::new(
                "ColorSpaceTransform: empty source color space name.",
            ));
        }

        if self.dst.is_empty() {
            return Err(Exception::new(
                "ColorSpaceTransform: empty destination color space name.",
            ));
        }
        Ok(())
    }

    /// The source color space's name.
    ///
    /// Port of `ColorSpaceTransform::getSrc` (ColorSpaceTransform.cpp:108-111 @ v2.5.2).
    #[doc(alias = "getSrc")]
    pub fn src(&self) -> &[u8] {
        &self.src
    }

    /// Sets the source color space's name, up to its first NUL (a C string).
    ///
    /// Port of `ColorSpaceTransform::setSrc` (ColorSpaceTransform.cpp:113-116 @ v2.5.2). Its
    /// null pointer, which sets an empty name, is an empty slice here.
    #[doc(alias = "setSrc")]
    pub fn set_src(&mut self, src: impl AsRef<[u8]>) {
        self.src = c_str(src.as_ref()).to_vec();
    }

    /// The destination color space's name.
    ///
    /// Port of `ColorSpaceTransform::getDst` (ColorSpaceTransform.cpp:118-121 @ v2.5.2).
    #[doc(alias = "getDst")]
    pub fn dst(&self) -> &[u8] {
        &self.dst
    }

    /// Sets the destination color space's name, up to its first NUL (a C string).
    ///
    /// Port of `ColorSpaceTransform::setDst` (ColorSpaceTransform.cpp:123-126 @ v2.5.2).
    #[doc(alias = "setDst")]
    pub fn set_dst(&mut self, dst: impl AsRef<[u8]>) {
        self.dst = c_str(dst.as_ref()).to_vec();
    }

    /// Whether data color spaces are left unprocessed (the default).
    ///
    /// Port of `ColorSpaceTransform::getDataBypass` (ColorSpaceTransform.cpp:128-131 @ v2.5.2).
    #[doc(alias = "getDataBypass")]
    pub fn data_bypass(&self) -> bool {
        self.data_bypass
    }

    /// Port of `ColorSpaceTransform::setDataBypass` (ColorSpaceTransform.cpp:133-136 @ v2.5.2).
    #[doc(alias = "setDataBypass")]
    pub fn set_data_bypass(&mut self, bypass: bool) {
        self.data_bypass = bypass;
    }

    /// Writes the transform's text to `os`.
    ///
    /// Upstream prints `dataBypass=0` straight after the destination's name, without a
    /// separator (docs/improvements.md, I-120), and a C++ `bool` as `0` or `1`.
    ///
    /// Port of `operator<<(std::ostream &, const ColorSpaceTransform &)`
    /// (ColorSpaceTransform.cpp:138-151 @ v2.5.2).
    pub(crate) fn write_text(&self, os: &mut OStringStream) {
        os.put_str("<ColorSpaceTransform ");
        os.put_str("direction=");
        os.put_str(transform_direction_to_string(self.direction()));
        os.put_str(", ");
        os.put_str("src=");
        put_c_str(os, self.src());
        os.put_str(", ");
        os.put_str("dst=");
        put_c_str(os, self.dst());
        let bypass = self.data_bypass();
        if !bypass {
            os.put_str("dataBypass=");
            put_bool(os, bypass);
        }
        os.put_str(">");
    }
}

impl fmt::Display for ColorSpaceTransform {
    /// `<ColorSpaceTransform direction=<dir>, src=<src>, dst=<dst>[dataBypass=0]>`.
    ///
    /// Port of `operator<<(std::ostream &, const ColorSpaceTransform &)`
    /// (ColorSpaceTransform.cpp:138-151 @ v2.5.2), on a new stream.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_text(&mut os);
        f.write_str(os.str())
    }
}

#[cfg(test)]
#[path = "color_space_transform_tests.rs"]
mod tests;
