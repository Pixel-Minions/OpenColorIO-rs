// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The LUT baker: a config's transform from an input space to a target space (or to a display
//! and view), sampled into a LUT file of a format that bakes. A port of
//! `src/OpenColorIO/Baker.cpp` @ v2.5.2.

use std::sync::Arc;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::format_metadata::{FormatMetadataImpl, METADATA_ROOT};
use ocio_ops::open_color_types::ViewType;
use ocio_ops::utils::string_utils::c_str;

use crate::baking_utils::{
    get_input_to_shaper_processor, get_input_to_target_processor, get_shaper_to_input_processor,
};
use crate::config::Config;
use crate::transforms::file_format::{FormatBakeCapabilities, FormatCapabilities, FormatRegistry};

/// A LUT baker.
///
/// A clone is upstream's `createEditableCopy`: the same settings, sharing the config.
///
/// Port of `Baker` and `Baker::Impl` (src/OpenColorIO/Baker.cpp:17-91 @ v2.5.2).
#[derive(Debug, Clone)]
pub struct Baker {
    /// `m_config`: a copy of the config given to [`Baker::set_config`].
    config: Option<Arc<Config>>,
    /// `m_formatName`.
    format_name: Vec<u8>,
    /// `m_formatMetadata`, a root element without a value.
    format_metadata: FormatMetadataImpl,
    /// `m_inputSpace`.
    input_space: Vec<u8>,
    /// `m_shaperSpace`.
    shaper_space: Vec<u8>,
    /// `m_looks`.
    looks: Vec<u8>,
    /// `m_targetSpace`.
    target_space: Vec<u8>,
    /// `m_display`.
    display: Vec<u8>,
    /// `m_view`.
    view: Vec<u8>,
    /// `m_shapersize`: -1 until set.
    shaper_size: i32,
    /// `m_cubesize`: -1 until set.
    cube_size: i32,
}

impl Default for Baker {
    fn default() -> Self {
        Baker::new()
    }
}

impl Baker {
    /// A baker without config, format or spaces, and sizes of -1 (each format's default).
    ///
    /// Port of `Baker::Create` and `Baker::Impl::Impl` (Baker.cpp:17-20, 43-47, 75-78 @
    /// v2.5.2).
    #[doc(alias = "Create")]
    pub fn new() -> Baker {
        Baker {
            config: None,
            format_name: Vec::new(),
            format_metadata: FormatMetadataImpl::new(METADATA_ROOT, b"")
                .expect("the root element has a name"),
            input_space: Vec::new(),
            shaper_space: Vec::new(),
            looks: Vec::new(),
            target_space: Vec::new(),
            display: Vec::new(),
            view: Vec::new(),
            shaper_size: -1,
            cube_size: -1,
        }
    }

    /// A copy of the baker: the same settings, sharing the config.
    ///
    /// Port of `Baker::createEditableCopy` (Baker.cpp:86-91 @ v2.5.2).
    #[doc(alias = "createEditableCopy")]
    pub fn create_editable_copy(&self) -> Baker {
        self.clone()
    }

    /// Sets the config: the baker keeps its own copy.
    ///
    /// Port of `Baker::setConfig` (Baker.cpp:93-96 @ v2.5.2).
    #[doc(alias = "setConfig")]
    pub fn set_config(&mut self, config: &Config) {
        self.config = Some(Arc::new(config.clone()));
    }

    /// The baker's copy of its config, `None` until one is set.
    ///
    /// Port of `Baker::getConfig` (Baker.cpp:98-101 @ v2.5.2).
    #[doc(alias = "getConfig")]
    pub fn config(&self) -> Option<&Config> {
        self.config.as_deref()
    }

    /// The number of formats that bake.
    ///
    /// Port of `Baker::getNumFormats` (Baker.cpp:103-106 @ v2.5.2).
    #[doc(alias = "getNumFormats")]
    pub fn num_formats() -> i32 {
        FormatRegistry::instance().num_formats(FormatCapabilities::BAKE)
    }

    /// The name of the baking format at `index`; `""` outside the list.
    ///
    /// Port of `Baker::getFormatNameByIndex` (Baker.cpp:108-111 @ v2.5.2).
    #[doc(alias = "getFormatNameByIndex")]
    pub fn format_name_by_index(index: i32) -> &'static [u8] {
        FormatRegistry::instance()
            .format_name_by_index(FormatCapabilities::BAKE, index)
            .as_bytes()
    }

    /// The extension of the baking format at `index`; `""` outside the list.
    ///
    /// Port of `Baker::getFormatExtensionByIndex` (Baker.cpp:113-116 @ v2.5.2).
    #[doc(alias = "getFormatExtensionByIndex")]
    pub fn format_extension_by_index(index: i32) -> &'static [u8] {
        FormatRegistry::instance()
            .format_extension_by_index(FormatCapabilities::BAKE, index)
            .as_bytes()
    }

    /// Sets the format, by name (ignoring case), up to its first NUL: "File format <name> does
    /// not support baking." unless one of the named format's infos bakes.
    ///
    /// Port of `Baker::setFormat` (Baker.cpp:118-139 @ v2.5.2). Its null pointer, which
    /// upstream can't look up, is out of reach here.
    #[doc(alias = "setFormat")]
    pub fn set_format(&mut self, format_name: impl AsRef<[u8]>) -> Result<()> {
        let format_name = c_str(format_name.as_ref());
        if let Some(fmt) = FormatRegistry::instance().file_format_by_name(format_name) {
            let format_info_vec = fmt.format_info();
            for info in &format_info_vec {
                if info.capabilities.contains(FormatCapabilities::BAKE) {
                    self.format_name = format_name.to_vec();
                    return Ok(());
                }
            }
        }

        let mut os = OStringStream::new(Crt::NATIVE);
        os.put_str("File format ");
        os.put_bytes(format_name);
        os.put_str(" does not support baking.");
        Err(Exception::new(os.into_bytes()))
    }

    /// The format's name, as it was set.
    ///
    /// Port of `Baker::getFormat` (Baker.cpp:141-144 @ v2.5.2).
    #[doc(alias = "getFormat")]
    pub fn format(&self) -> &[u8] {
        &self.format_name
    }

    /// The metadata the formats write (as comments, for some).
    ///
    /// Port of `Baker::getFormatMetadata() const` (Baker.cpp:146-149 @ v2.5.2).
    #[doc(alias = "getFormatMetadata")]
    pub fn format_metadata(&self) -> &FormatMetadataImpl {
        &self.format_metadata
    }

    /// The metadata the formats write, to edit.
    ///
    /// Port of `Baker::getFormatMetadata()` (Baker.cpp:151-154 @ v2.5.2).
    #[doc(alias = "getFormatMetadata")]
    pub fn format_metadata_mut(&mut self) -> &mut FormatMetadataImpl {
        &mut self.format_metadata
    }

    /// Sets the input space's name, up to its first NUL.
    ///
    /// Port of `Baker::setInputSpace` (Baker.cpp:156-159 @ v2.5.2).
    #[doc(alias = "setInputSpace")]
    pub fn set_input_space(&mut self, input_space: impl AsRef<[u8]>) {
        self.input_space = c_str(input_space.as_ref()).to_vec();
    }

    /// The input space's name.
    ///
    /// Port of `Baker::getInputSpace` (Baker.cpp:161-164 @ v2.5.2).
    #[doc(alias = "getInputSpace")]
    pub fn input_space(&self) -> &[u8] {
        &self.input_space
    }

    /// Sets the shaper space's name, up to its first NUL.
    ///
    /// Port of `Baker::setShaperSpace` (Baker.cpp:166-169 @ v2.5.2).
    #[doc(alias = "setShaperSpace")]
    pub fn set_shaper_space(&mut self, shaper_space: impl AsRef<[u8]>) {
        self.shaper_space = c_str(shaper_space.as_ref()).to_vec();
    }

    /// The shaper space's name.
    ///
    /// Port of `Baker::getShaperSpace` (Baker.cpp:171-174 @ v2.5.2).
    #[doc(alias = "getShaperSpace")]
    pub fn shaper_space(&self) -> &[u8] {
        &self.shaper_space
    }

    /// Sets the looks, up to the first NUL.
    ///
    /// Port of `Baker::setLooks` (Baker.cpp:176-179 @ v2.5.2).
    #[doc(alias = "setLooks")]
    pub fn set_looks(&mut self, looks: impl AsRef<[u8]>) {
        self.looks = c_str(looks.as_ref()).to_vec();
    }

    /// The looks, as they were set.
    ///
    /// Port of `Baker::getLooks` (Baker.cpp:181-184 @ v2.5.2).
    #[doc(alias = "getLooks")]
    pub fn looks(&self) -> &[u8] {
        &self.looks
    }

    /// Sets the target space's name, up to its first NUL.
    ///
    /// Port of `Baker::setTargetSpace` (Baker.cpp:186-189 @ v2.5.2).
    #[doc(alias = "setTargetSpace")]
    pub fn set_target_space(&mut self, target_space: impl AsRef<[u8]>) {
        self.target_space = c_str(target_space.as_ref()).to_vec();
    }

    /// The target space's name.
    ///
    /// Port of `Baker::getTargetSpace` (Baker.cpp:191-194 @ v2.5.2).
    #[doc(alias = "getTargetSpace")]
    pub fn target_space(&self) -> &[u8] {
        &self.target_space
    }

    /// The display's name.
    ///
    /// Port of `Baker::getDisplay` (Baker.cpp:196-199 @ v2.5.2).
    #[doc(alias = "getDisplay")]
    pub fn display(&self) -> &[u8] {
        &self.display
    }

    /// The view's name.
    ///
    /// Port of `Baker::getView` (Baker.cpp:201-204 @ v2.5.2).
    #[doc(alias = "getView")]
    pub fn view(&self) -> &[u8] {
        &self.view
    }

    /// Sets the display and the view, each up to its first NUL.
    ///
    /// Port of `Baker::setDisplayView` (Baker.cpp:206-215 @ v2.5.2). Its "Both display and
    /// view must be set." for a null pointer is out of reach here.
    #[doc(alias = "setDisplayView")]
    pub fn set_display_view(&mut self, display: impl AsRef<[u8]>, view: impl AsRef<[u8]>) {
        self.display = c_str(display.as_ref()).to_vec();
        self.view = c_str(view.as_ref()).to_vec();
    }

    /// Sets the shaper's size; -1 is the format's default.
    ///
    /// Port of `Baker::setShaperSize` (Baker.cpp:217-220 @ v2.5.2).
    #[doc(alias = "setShaperSize")]
    pub fn set_shaper_size(&mut self, shaper_size: i32) {
        self.shaper_size = shaper_size;
    }

    /// The shaper's size, -1 until set.
    ///
    /// Port of `Baker::getShaperSize` (Baker.cpp:222-225 @ v2.5.2).
    #[doc(alias = "getShaperSize")]
    pub fn shaper_size(&self) -> i32 {
        self.shaper_size
    }

    /// Sets the cube's size (or the 1D LUT's, for formats that bake one); -1 is the format's
    /// default.
    ///
    /// Port of `Baker::setCubeSize` (Baker.cpp:227-230 @ v2.5.2).
    #[doc(alias = "setCubeSize")]
    pub fn set_cube_size(&mut self, cube_size: i32) {
        self.cube_size = cube_size;
    }

    /// The cube's size, -1 until set.
    ///
    /// Port of `Baker::getCubeSize` (Baker.cpp:232-235 @ v2.5.2).
    #[doc(alias = "getCubeSize")]
    pub fn cube_size(&self) -> i32 {
        self.cube_size
    }

    /// Bakes the LUT into `os`, after checking the settings.
    ///
    /// What a format wrote before an error stays written to `os`, as on upstream's stream. A
    /// failure of `os` itself goes unreported, as a C++ stream keeps it in its state bits and
    /// throws nothing (docs/improvements.md, I-178).
    ///
    /// Port of `Baker::bake` (Baker.cpp:237-414 @ v2.5.2).
    pub fn bake(&self, os: &mut impl std::io::Write) -> Result<()> {
        let registry = FormatRegistry::instance();
        let Some(fmt) = registry.file_format_by_name(&self.format_name) else {
            let mut err = OStringStream::new(Crt::NATIVE);
            err.put_str("The format named '");
            err.put_bytes(&self.format_name);
            err.put_str("' could not be found. ");
            return Err(Exception::new(err.into_bytes()));
        };

        let fmt_info_vec = fmt.format_info();
        let fmt_info = fmt_info_vec[0];

        let input_space = &self.input_space;
        let target_space = &self.target_space;
        let shaper_space = &self.shaper_space;
        let display = &self.display;
        let view = &self.view;

        let display_view_mode = !display.is_empty() && !view.is_empty();
        let color_space_mode = !target_space.is_empty();

        // Settings validation.
        let Some(config) = self.config() else {
            return Err(Exception::new("No OCIO config has been set."));
        };

        if input_space.is_empty() {
            return Err(Exception::new("No input space has been set."));
        }

        if !display_view_mode && !color_space_mode {
            return Err(Exception::new(
                "No display / view or target colorspace has been set.",
            ));
        }

        if display_view_mode && color_space_mode {
            return Err(Exception::new(
                "Cannot use both display / view and target colorspace.",
            ));
        }

        if config.color_space(input_space).is_none() {
            let mut os = OStringStream::new(Crt::NATIVE);
            os.put_str("Could not find input colorspace '");
            os.put_bytes(input_space);
            os.put_str("'.");
            return Err(Exception::new(os.into_bytes()));
        }

        if color_space_mode && config.color_space(target_space).is_none() {
            let mut os = OStringStream::new(Crt::NATIVE);
            os.put_str("Could not find target colorspace '");
            os.put_bytes(target_space);
            os.put_str("'.");
            return Err(Exception::new(os.into_bytes()));
        }

        if display_view_mode {
            let mut found_display = false;
            let mut found_view = false;

            // Make sure we also search through inactive views.
            let has_view_by_type = |type_: ViewType, display: &[u8], view: &[u8]| -> bool {
                for i in 0..config.num_views_of_type(type_, display) {
                    if config.view_of_type(type_, display, i) == view {
                        return true;
                    }
                }
                false
            };

            for i in 0..config.num_displays_all() {
                let curr_display = config.display_all(i);
                if curr_display == display.as_slice() {
                    found_display = true;
                    found_view |= has_view_by_type(ViewType::DisplayDefined, curr_display, view);
                    found_view |= has_view_by_type(ViewType::Shared, curr_display, view);
                }
                if found_display && found_view {
                    break;
                }
            }

            if !found_display {
                let mut os = OStringStream::new(Crt::NATIVE);
                os.put_str("Could not find display '");
                os.put_bytes(display);
                os.put_str("'.");
                return Err(Exception::new(os.into_bytes()));
            } else if !found_view {
                let mut os = OStringStream::new(Crt::NATIVE);
                os.put_str("Could not find view '");
                os.put_bytes(view);
                os.put_str("'.");
                return Err(Exception::new(os.into_bytes()));
            }
        }

        let bake_1d = fmt_info.bake_capabilities == FormatBakeCapabilities::LUT_1D;
        if bake_1d && get_input_to_target_processor(self)?.has_channel_crosstalk() {
            let mut os = OStringStream::new(Crt::NATIVE);
            os.put_str("The format '");
            os.put_bytes(&self.format_name);
            os.put_str("' does not support");
            os.put_str(" transformations with channel crosstalk.");
            return Err(Exception::new(os.into_bytes()));
        }

        if self.cube_size() != -1 && self.cube_size() < 2 {
            return Err(Exception::new("Cube size must be at least 2 if set."));
        }

        let support_shaper = fmt_info.bake_capabilities.0 & FormatBakeCapabilities::LUT_1D_3D.0
            != 0
            || fmt_info.bake_capabilities.0 & FormatBakeCapabilities::LUT_1D.0 != 0;
        if !shaper_space.is_empty() && !support_shaper {
            let mut os = OStringStream::new(Crt::NATIVE);
            os.put_str("The format '");
            os.put_bytes(&self.format_name);
            os.put_str("' does not support shaper space.");
            return Err(Exception::new(os.into_bytes()));
        }

        if !shaper_space.is_empty() && self.shaper_size() != -1 && self.shaper_size() < 2 {
            let mut os = OStringStream::new(Crt::NATIVE);
            os.put_str("A shaper space '");
            os.put_bytes(self.shaper_space());
            os.put_str("' has");
            os.put_str(" been specified, so the shaper size must be 2 or larger.");
            return Err(Exception::new(os.into_bytes()));
        }

        if !shaper_space.is_empty() {
            let input_to_shaper = get_input_to_shaper_processor(self)?;
            let shaper_to_input = get_shaper_to_input_processor(self)?;

            if input_to_shaper.has_channel_crosstalk() || shaper_to_input.has_channel_crosstalk() {
                let mut os = OStringStream::new(Crt::NATIVE);
                os.put_str("The specified shaper space, '");
                os.put_bytes(self.shaper_space());
                os.put_str("' has channel crosstalk, which is not appropriate for");
                os.put_str(" shapers. Please select an alternate shaper space or");
                os.put_str(" omit this option.");
                return Err(Exception::new(os.into_bytes()));
            }
        }

        let mut stream = OStringStream::new(Crt::NATIVE);
        let baked = fmt.bake(self, &self.format_name, &mut stream);
        // What the format wrote before an error stays written, as on upstream's stream.
        let _ = os.write_all(stream.str());
        if let Err(e) = baked {
            let mut err = OStringStream::new(Crt::NATIVE);
            err.put_str("Error baking ");
            err.put_bytes(&self.format_name);
            err.put_str(":");
            err.put_c_str(e.what());
            return Err(Exception::new(err.into_bytes()));
        }

        //
        // TODO:
        //
        // - check limits of shaper and target, throw exception if we can't
        //   write that much data in x format
        // - add some checks to make sure we are monotonic
        // - do a compare between OCIO transform and output LUT transform
        //   throw error if we going beyond tolerance
        //
        Ok(())
    }
}
