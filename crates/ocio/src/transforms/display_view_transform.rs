// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The display view transform: a port of the class in
//! `src/OpenColorIO/transforms/DisplayViewTransform.cpp` @ v2.5.2. Its op builder
//! (`BuildDisplayOps`) and its `CollectContextVariables` come with WP 3.2.

use std::fmt;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::open_color_types::{TransformDirection, transform_direction_to_string};
use ocio_ops::utils::string_utils::c_str;

use crate::transform::{put_bool, put_c_str, validate_direction};

/// A conversion from a color space to one of a config's displays through one of its views,
/// by name, with the view's looks.
///
/// A copy is upstream's `createEditableCopy`. Upstream gives the class no `equals`.
///
/// Port of `DisplayViewTransform` and its `Impl` (include/OpenColorIO/OpenColorTransforms.h:
/// 376-425, src/OpenColorIO/transforms/DisplayViewTransform.cpp:18-152 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct DisplayViewTransform {
    /// `m_dir`.
    dir: TransformDirection,
    /// `m_src`.
    src: Vec<u8>,
    /// `m_display`.
    display: Vec<u8>,
    /// `m_view`.
    view: Vec<u8>,
    /// `m_looksBypass`.
    looks_bypass: bool,
    /// `m_dataBypass`.
    data_bypass: bool,
}

impl Default for DisplayViewTransform {
    fn default() -> Self {
        Self::new()
    }
}

impl DisplayViewTransform {
    /// A forward transform with empty names, which applies the view's looks and bypasses data
    /// color spaces.
    ///
    /// Port of `DisplayViewTransform::Create` and `Impl` (DisplayViewTransform.cpp:18-50 @
    /// v2.5.2).
    #[doc(alias = "Create")]
    pub fn new() -> DisplayViewTransform {
        DisplayViewTransform {
            dir: TransformDirection::Forward,
            src: Vec::new(),
            display: Vec::new(),
            view: Vec::new(),
            looks_bypass: false,
            data_bypass: true,
        }
    }

    /// Port of `DisplayViewTransform::getDirection` (DisplayViewTransform.cpp:65-68 @ v2.5.2).
    #[doc(alias = "getDirection")]
    pub fn direction(&self) -> TransformDirection {
        self.dir
    }

    /// Port of `DisplayViewTransform::setDirection` (DisplayViewTransform.cpp:70-73 @ v2.5.2).
    #[doc(alias = "setDirection")]
    pub fn set_direction(&mut self, dir: TransformDirection) {
        self.dir = dir;
    }

    /// Checks the direction ("DisplayViewTransform validation failed: " and its error), then
    /// that the source, the display and the view are set, in that order.
    ///
    /// Port of `DisplayViewTransform::validate` (DisplayViewTransform.cpp:75-102 @ v2.5.2).
    pub fn validate(&self) -> Result<()> {
        if let Err(ex) = validate_direction(self.dir) {
            return Err(Exception::new(format!(
                "DisplayViewTransform validation failed: {}",
                ex.message()
            )));
        }

        if self.src.is_empty() {
            return Err(Exception::new(
                "DisplayViewTransform: empty source color space name.",
            ));
        }

        if self.display.is_empty() {
            return Err(Exception::new("DisplayViewTransform: empty display name."));
        }

        if self.view.is_empty() {
            return Err(Exception::new("DisplayViewTransform: empty view name."));
        }
        Ok(())
    }

    /// Sets the source color space's name, up to its first NUL (a C string).
    ///
    /// Port of `DisplayViewTransform::setSrc` (DisplayViewTransform.cpp:104-107 @ v2.5.2). Its
    /// null pointer, which sets an empty name, is an empty slice here.
    #[doc(alias = "setSrc")]
    pub fn set_src(&mut self, name: impl AsRef<[u8]>) {
        self.src = c_str(name.as_ref()).to_vec();
    }

    /// The source color space's name.
    ///
    /// Port of `DisplayViewTransform::getSrc` (DisplayViewTransform.cpp:109-112 @ v2.5.2).
    #[doc(alias = "getSrc")]
    pub fn src(&self) -> &[u8] {
        &self.src
    }

    /// Sets the display's name, up to its first NUL.
    ///
    /// Port of `DisplayViewTransform::setDisplay` (DisplayViewTransform.cpp:114-117 @ v2.5.2).
    #[doc(alias = "setDisplay")]
    pub fn set_display(&mut self, display: impl AsRef<[u8]>) {
        self.display = c_str(display.as_ref()).to_vec();
    }

    /// The display's name.
    ///
    /// Port of `DisplayViewTransform::getDisplay` (DisplayViewTransform.cpp:119-122 @ v2.5.2).
    #[doc(alias = "getDisplay")]
    pub fn display(&self) -> &[u8] {
        &self.display
    }

    /// Sets the view's name, up to its first NUL.
    ///
    /// Port of `DisplayViewTransform::setView` (DisplayViewTransform.cpp:124-127 @ v2.5.2).
    #[doc(alias = "setView")]
    pub fn set_view(&mut self, view: impl AsRef<[u8]>) {
        self.view = c_str(view.as_ref()).to_vec();
    }

    /// The view's name.
    ///
    /// Port of `DisplayViewTransform::getView` (DisplayViewTransform.cpp:129-132 @ v2.5.2).
    #[doc(alias = "getView")]
    pub fn view(&self) -> &[u8] {
        &self.view
    }

    /// Whether the view's looks are skipped (not the default).
    ///
    /// Port of `DisplayViewTransform::setLooksBypass` (DisplayViewTransform.cpp:134-137 @
    /// v2.5.2).
    #[doc(alias = "setLooksBypass")]
    pub fn set_looks_bypass(&mut self, bypass: bool) {
        self.looks_bypass = bypass;
    }

    /// Port of `DisplayViewTransform::getLooksBypass` (DisplayViewTransform.cpp:139-142 @
    /// v2.5.2).
    #[doc(alias = "getLooksBypass")]
    pub fn looks_bypass(&self) -> bool {
        self.looks_bypass
    }

    /// Whether data color spaces are left unprocessed (the default).
    ///
    /// Port of `DisplayViewTransform::setDataBypass` (DisplayViewTransform.cpp:144-147 @
    /// v2.5.2).
    #[doc(alias = "setDataBypass")]
    pub fn set_data_bypass(&mut self, bypass: bool) {
        self.data_bypass = bypass;
    }

    /// Port of `DisplayViewTransform::getDataBypass` (DisplayViewTransform.cpp:149-152 @
    /// v2.5.2).
    #[doc(alias = "getDataBypass")]
    pub fn data_bypass(&self) -> bool {
        self.data_bypass
    }

    /// Writes the transform's text to `os`.
    ///
    /// Upstream ends the view with `", "` and starts each bypass with another `", "`, so the
    /// text has `", >"` at its end or `", , "` before a bypass (docs/improvements.md, I-121);
    /// a C++ `bool` prints as `0` or `1`.
    ///
    /// Port of `operator<<(std::ostream &, const DisplayViewTransform &)`
    /// (DisplayViewTransform.cpp:154-171 @ v2.5.2).
    pub(crate) fn write_text(&self, os: &mut OStringStream) {
        os.put_str("<DisplayViewTransform ");
        os.put_str("direction=");
        os.put_str(transform_direction_to_string(self.direction()));
        os.put_str(", ");
        os.put_str("src=");
        put_c_str(os, self.src());
        os.put_str(", ");
        os.put_str("display=");
        put_c_str(os, self.display());
        os.put_str(", ");
        os.put_str("view=");
        put_c_str(os, self.view());
        os.put_str(", ");
        if self.looks_bypass() {
            os.put_str(", looksBypass=");
            put_bool(os, self.looks_bypass());
        }
        if !self.data_bypass() {
            os.put_str(", dataBypass=");
            put_bool(os, self.data_bypass());
        }
        os.put_str(">");
    }
}

impl fmt::Display for DisplayViewTransform {
    /// `<DisplayViewTransform direction=<dir>, src=<src>, display=<display>, view=<view>, [,
    /// looksBypass=1][, dataBypass=0]>`.
    ///
    /// Port of `operator<<(std::ostream &, const DisplayViewTransform &)`
    /// (DisplayViewTransform.cpp:154-171 @ v2.5.2), on a new stream.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_text(&mut os);
        f.write_str(os.str())
    }
}

#[cfg(test)]
#[path = "display_view_transform_tests.rs"]
mod tests;
