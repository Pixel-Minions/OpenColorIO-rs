// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The color mixing helpers: a port of `src/OpenColorIO/apphelpers/MixingHelpers.h` and
//! `MixingHelpers.cpp` @ v2.5.2, whose public API is
//! `include/OpenColorIO/OpenColorAppHelpers.h:434-534`.
//!
//! A color picker mixes colors in a mixing space (the color picking role's color space, or
//! else the rendering space or a (display, view)'s space) and an encoding (RGB or HSV):
//!
//! - [`MixingColorSpaceManager`]: the mixing spaces and encodings, the selected ones, and the
//!   processor from the working space to them;
//! - [`MixingSlider`]: a UI slider's window onto the mixing space, which maps linear mixing
//!   spaces to an approximately perceptually uniform scale.

use std::fmt;
use std::sync::Arc;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::math_utils::{sse_add, sse_mul, std_max, std_min};
use ocio_ops::open_color_types::{FixedFunctionStyle, TransformDirection};
use ocio_ops::parse_utils::ROLE_COLOR_PICKING;
use ocio_ops::utils::string_utils::{StringVec, c_str};

use crate::apphelpers::color_space_helpers::ColorSpaceInfo;
use crate::config::Config;
use crate::processor::Processor;
use crate::transform::Transform;
use crate::transforms::display_view_transform::DisplayViewTransform;
use crate::transforms::fixed_function_transform::FixedFunctionTransform;
use crate::transforms::group_transform::GroupTransform;
use crate::transforms::matrix_transform::MatrixTransform;

// The slider's mapping of linear values to perceptual ones (MixingHelpers.cpp:21-27 @ v2.5.2).
const GAMMA: f32 = 2.0;
const LOGSLOPE: f32 = 0.55;
const BREAKPNT: f32 = 0.18;
const NEGSLOPE: f32 = 0.5;
/// 1/GAMMA.
const INVGAMMA: f32 = 0.5;
/// BREAKPNT^(1/GAMMA).
#[allow(clippy::excessive_precision)]
const INVBREAKPNT: f32 = 0.42426406871192851;
/// BREAKPNT^(1/GAMMA) - log10(BREAKPNT)*LOGSLOPE.
#[allow(clippy::excessive_precision)]
const LOGOFFSET: f32 = 0.83386419090511021;

/// C `powf(base, exponent)`: the C runtime's, as both wheels call it. LLVM rewrites `powf` of
/// a constant exponent (0.5 into a square root, 2 into a product) or base (10 into `exp10f`),
/// which differ from the C runtime's `powf` for many ordinary inputs: the C runtime's `powf`
/// is not correctly rounded, so it is off by one ulp where a square root or a product is
/// exact (Windows: `powf(0.010001217, 0.5)` is `0x3dcccffd`, `sqrt` gives `0x3dcccffe`;
/// glibc differs from `sqrtf` for 663,525 inputs in (0, 0.18]), and for a square root of
/// `-inf` or of a negative NaN; hiding both operands keeps the call (docs/improvements.md,
/// I-337).
#[inline]
fn crt_powf(base: f32, exponent: f32) -> f32 {
    std::hint::black_box(base).powf(std::hint::black_box(exponent))
}

/// A linear value on the slider's perceptual scale: half of it at or below 0, a square root up
/// to 0.18, a log above (and NaN through the square root, `powf(NaN, 0.5f)`, on both wheels).
///
/// Port of `linearToPerceptual` (MixingHelpers.cpp:29-37 @ v2.5.2).
fn linear_to_perceptual(linear: f32) -> f32 {
    if linear <= 0.0 {
        linear * NEGSLOPE
    } else if linear > BREAKPNT {
        LOGOFFSET + linear.log10() * LOGSLOPE
    } else {
        crt_powf(linear, INVGAMMA)
    }
}

/// `std::pow(percept, GAMMA)` as each wheel computes it: a `powf` call on Windows (MSVC), and
/// a product `percept * percept` on Linux (GCC turns `powf(x, 2.0f)` into `mulss`;
/// `perceptualToLinear`, inlined into `MixingSliderImpl::sliderToMixing`, Windows
/// `0x180043fb9`-`0x180043fc4`, Linux `0x15b018`). The Windows `powf` is not correctly rounded, so
/// the two differ for ordinary values too (`powf(x, 2)` is not `x * x` for 3,535,522 inputs in
/// (0, 0.42426]; docs/improvements.md, I-337).
#[inline]
fn square(percept: f32) -> f32 {
    #[cfg(target_os = "windows")]
    {
        crt_powf(percept, GAMMA)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = GAMMA;
        percept * percept
    }
}

/// The inverse of [`linear_to_perceptual`]: double at or below 0, a square up to
/// 0.42426407 (and for NaN), a power of 10 above.
///
/// Port of `perceptualToLinear` (MixingHelpers.cpp:39-47 @ v2.5.2).
fn perceptual_to_linear(percept: f32) -> f32 {
    if percept <= 0.0 {
        percept / NEGSLOPE
    } else if percept > INVBREAKPNT {
        crt_powf(10.0, (percept - LOGOFFSET) / LOGSLOPE)
    } else {
        square(percept)
    }
}

/// A UI slider's window onto the mixing space: its edges and the conversions between distance
/// along it (0 at the minimum edge, 1 at the maximum) and mixing space units. Where the mixing
/// space isn't perceptually uniform, the edges and the values are mapped to an approximately
/// perceptually uniform scale (so that mid-grey falls near the middle), and the edges are kept
/// at least 0.01 apart.
///
/// The slider belongs to a [`MixingColorSpaceManager`], which hands it out by reference
/// ([`MixingColorSpaceManager::slider`]); it holds a copy of whether the manager's mixing space
/// is perceptually uniform, which the manager updates as its selection changes (upstream's
/// slider reads it from the manager it refers to).
///
/// Port of `MixingSlider` (include/OpenColorIO/OpenColorAppHelpers.h:454-483) and
/// `MixingSliderImpl` (src/OpenColorIO/apphelpers/MixingHelpers.h:15-40, MixingHelpers.cpp:
/// 52-114 @ v2.5.2), its only implementation (owner decision 2026-10-22, P5-3 d).
#[derive(Debug)]
pub struct MixingSlider {
    /// `m_mixing.isPerceptuallyUniform()`, kept by the manager.
    perceptually_uniform: bool,
    /// `m_sliderMinEdge`.
    slider_min_edge: f32,
    /// `m_sliderMaxEdge`.
    slider_max_edge: f32,
}

impl MixingSlider {
    /// The slider of a manager: edges 0 and 1.
    ///
    /// Port of `MixingSliderImpl::MixingSliderImpl` (MixingHelpers.h:38-39,
    /// MixingHelpers.cpp:52-56 @ v2.5.2).
    fn new(perceptually_uniform: bool) -> MixingSlider {
        MixingSlider {
            perceptually_uniform,
            slider_min_edge: 0.0,
            slider_max_edge: 1.0,
        }
    }

    /// The minimum edge of the slider, in mixing space units, unless the mixing space isn't
    /// perceptually uniform: then on the perceptual scale, of the smaller of the edge and the
    /// maximum edge less 0.01 (C++ `std::min`).
    ///
    /// Port of `MixingSliderImpl::getSliderMinEdge` (MixingHelpers.cpp:63-68 @ v2.5.2).
    #[doc(alias = "getSliderMinEdge")]
    pub fn slider_min_edge(&self) -> f32 {
        if !self.perceptually_uniform {
            linear_to_perceptual(std_min(self.slider_min_edge, self.slider_max_edge - 0.01))
        } else {
            self.slider_min_edge
        }
    }

    /// Sets the minimum edge of the slider, in mixing space units.
    ///
    /// Port of `MixingSliderImpl::setSliderMinEdge` (MixingHelpers.cpp:70-73 @ v2.5.2).
    #[doc(alias = "setSliderMinEdge")]
    pub fn set_slider_min_edge(&mut self, slider_mixing_min_edge: f32) {
        self.slider_min_edge = slider_mixing_min_edge;
    }

    /// The maximum edge of the slider (see [`MixingSlider::slider_min_edge`]): on the
    /// perceptual scale, of the larger of the edge and the minimum edge plus 0.01 (C++
    /// `std::max`), where the mixing space isn't perceptually uniform.
    ///
    /// Port of `MixingSliderImpl::getSliderMaxEdge` (MixingHelpers.cpp:75-80 @ v2.5.2).
    #[doc(alias = "getSliderMaxEdge")]
    pub fn slider_max_edge(&self) -> f32 {
        if !self.perceptually_uniform {
            linear_to_perceptual(std_max(self.slider_max_edge, self.slider_min_edge + 0.01))
        } else {
            self.slider_max_edge
        }
    }

    /// Sets the maximum edge of the slider, in mixing space units.
    ///
    /// Port of `MixingSliderImpl::setSliderMaxEdge` (MixingHelpers.cpp:82-85 @ v2.5.2).
    #[doc(alias = "setSliderMaxEdge")]
    pub fn set_slider_max_edge(&mut self, slider_mixing_max_edge: f32) {
        self.slider_max_edge = slider_mixing_max_edge;
    }

    /// Distance along the slider to mixing space units.
    ///
    /// Where NaNs meet, the operand order is each wheel's: `min + (max - min) * units` on
    /// Windows, `units * (max - min) + min` on Linux (`MixingSliderImpl::sliderToMixing`,
    /// Windows `0x180043f62`-`0x180043f77`, Linux `0x15af83`-`0x15af98`;
    /// docs/improvements.md, I-337).
    ///
    /// Port of `MixingSliderImpl::sliderToMixing` (MixingHelpers.cpp:87-93 @ v2.5.2).
    #[doc(alias = "sliderToMixing")]
    pub fn slider_to_mixing(&self, slider_units: f32) -> f32 {
        let min = self.slider_min_edge();
        let max = self.slider_max_edge();
        #[cfg(target_os = "windows")]
        let percept = sse_add(min, sse_mul(max - min, slider_units));
        #[cfg(not(target_os = "windows"))]
        let percept = sse_add(sse_mul(slider_units, max - min), min);

        if !self.perceptually_uniform {
            perceptual_to_linear(percept)
        } else {
            percept
        }
    }

    /// Mixing space units to distance along the slider.
    ///
    /// Port of `MixingSliderImpl::mixingToSlider` (MixingHelpers.cpp:95-106 @ v2.5.2).
    #[doc(alias = "mixingToSlider")]
    pub fn mixing_to_slider(&self, mixing_units: f32) -> f32 {
        let percept = if !self.perceptually_uniform {
            linear_to_perceptual(mixing_units)
        } else {
            mixing_units
        };

        // The slider is a window onto the perceptual units.
        // Apply affine based on current left/right or min/max edges of the UI.
        (percept - self.slider_min_edge()) / (self.slider_max_edge() - self.slider_min_edge())
    }

    /// Writes the slider's text to `os`.
    ///
    /// Port of `operator<<(std::ostream &, const MixingSlider &)` (MixingHelpers.cpp:109-114 @
    /// v2.5.2).
    fn write_text(&self, os: &mut OStringStream) {
        os.put_str("minEdge: ");
        os.put_f32(self.slider_min_edge());
        os.put_str(", maxEdge: ");
        os.put_f32(self.slider_max_edge());
    }

    /// The slider's text, `minEdge: <min>, maxEdge: <max>` (the edges as
    /// [`MixingSlider::slider_min_edge`] and [`MixingSlider::slider_max_edge`] give them, as
    /// a C++ stream writes a float): the text of Python's `repr()`.
    ///
    /// Port of `operator<<(std::ostream &, const MixingSlider &)` (MixingHelpers.cpp:109-114 @
    /// v2.5.2), on a new stream.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_text(&mut os);
        os.into_bytes()
    }
}

impl fmt::Display for MixingSlider {
    /// See [`MixingSlider::to_bytes`].
    ///
    /// Port of `operator<<(std::ostream &, const MixingSlider &)` (MixingHelpers.cpp:109-114 @
    /// v2.5.2), on a new stream.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&String::from_utf8_lossy(&self.to_bytes()))
    }
}

/// The mixing spaces, the mixing encodings, the selected ones, and the slider of a color
/// picker for a config.
///
/// - The mixing spaces: the color picking role's color space alone (UI name
///   `color_picking (<color space>)`) when the config has that role, else `Rendering Space`
///   (the working space) and `Display Space` (a (display, view)'s).
/// - The mixing encodings: `RGB` and `HSV`.
///
/// Upstream deletes the copy constructor, and has no `createEditableCopy`.
///
/// Port of `MixingColorSpaceManager` (include/OpenColorIO/OpenColorAppHelpers.h:490-532) and
/// `MixingColorSpaceManagerImpl` (src/OpenColorIO/apphelpers/MixingHelpers.h:42-97,
/// MixingHelpers.cpp:116-395 @ v2.5.2), its only implementation (owner decision 2026-10-22,
/// P5-3 d).
pub struct MixingColorSpaceManager {
    /// `m_config`.
    config: Arc<Config>,
    /// `m_slider`.
    slider: MixingSlider,
    /// `m_mixingSpaces`.
    mixing_spaces: StringVec,
    /// `m_mixingEncodings`.
    mixing_encodings: StringVec,
    /// `m_selectedMixingSpaceIdx`.
    selected_mixing_space_idx: usize,
    /// `m_selectedMixingEncodingIdx`.
    selected_mixing_encoding_idx: usize,
    /// `m_colorPicker`.
    color_picker: Option<ColorSpaceInfo>,
}

impl fmt::Debug for MixingColorSpaceManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MixingColorSpaceManager")
            .field("slider", &self.slider)
            .field("mixing_spaces", &self.mixing_spaces)
            .field("selected_mixing_space_idx", &self.selected_mixing_space_idx)
            .field(
                "selected_mixing_encoding_idx",
                &self.selected_mixing_encoding_idx,
            )
            .field("color_picker", &self.color_picker)
            .finish_non_exhaustive()
    }
}

/// The color picking role's info ([`ColorSpaceInfo::create_from_single_role`]), `None` when
/// the config doesn't have the role. A role naming no color space (another role's name, or
/// none of the config's) is an error: upstream reads through the null color space
/// `getColorSpace` returns there, and the wheel's process dies (docs/improvements.md, U-152).
fn color_picker_of(config: &Config) -> Result<Option<ColorSpaceInfo>> {
    if config.has_role(ROLE_COLOR_PICKING) && config.color_space(ROLE_COLOR_PICKING).is_none() {
        return Err(Exception::new(
            [
                b"MixingColorSpaceManager: the role '".as_slice(),
                ROLE_COLOR_PICKING.as_bytes(),
                b"' refers to the color space '",
                config.role_color_space(ROLE_COLOR_PICKING),
                b"', which doesn't exist: OCIO 2.5.2 reads a null color space there, and its \
                  process dies (docs/improvements.md, U-152).",
            ]
            .concat(),
        ));
    }
    ColorSpaceInfo::create_from_single_role(config, ROLE_COLOR_PICKING.as_bytes())
}

impl MixingColorSpaceManager {
    /// The manager of `config` (see [`MixingColorSpaceManager::refresh`]).
    ///
    /// Upstream's process dies where the config's color picking role names no color space;
    /// the port returns an error (docs/improvements.md, U-152).
    ///
    /// Port of `MixingColorSpaceManager::Create` and `MixingColorSpaceManagerImpl::
    /// MixingColorSpaceManagerImpl` (MixingHelpers.cpp:116-133 @ v2.5.2).
    #[doc(alias = "Create")]
    pub fn new(config: Arc<Config>) -> Result<MixingColorSpaceManager> {
        let mut manager = MixingColorSpaceManager {
            config: Arc::clone(&config),
            slider: MixingSlider::new(false),
            mixing_spaces: StringVec::new(),
            mixing_encodings: vec![b"RGB".to_vec(), b"HSV".to_vec()],
            selected_mixing_space_idx: 0,
            selected_mixing_encoding_idx: 0,
            color_picker: None,
        };
        manager.refresh(config)?;
        Ok(manager)
    }

    /// Makes the manager the manager of `config` (needed after a config changes): the mixing
    /// spaces are the config's, and the first mixing space and the first encoding are
    /// selected. The slider keeps its edges.
    ///
    /// Upstream's process dies where the config's color picking role names no color space;
    /// the port returns an error, and the manager is left as it was (docs/improvements.md,
    /// U-152).
    ///
    /// Port of `MixingColorSpaceManagerImpl::refresh` (both overloads, MixingHelpers.cpp:
    /// 135-167 @ v2.5.2).
    pub fn refresh(&mut self, config: Arc<Config>) -> Result<()> {
        let color_picker = color_picker_of(&config)?;
        self.config = config;

        // Add the mixing spaces.

        self.selected_mixing_space_idx = 0;
        self.mixing_spaces.clear();

        self.color_picker = color_picker;

        if let Some(color_picker) = &self.color_picker {
            self.mixing_spaces.push(color_picker.ui_name().to_vec());
        } else {
            // TODO: Replace the 'Display Space' entry (i.e. the color space of the monitor)
            // by the list of all the display color spaces from the configuration
            // when the feature is in.
            self.mixing_spaces.push(b"Rendering Space".to_vec());
            self.mixing_spaces.push(b"Display Space".to_vec());
        }

        // The mixing encodings.

        self.selected_mixing_encoding_idx = 0;
        self.sync_slider();
        Ok(())
    }

    /// Takes `config` as the manager's config, as it is: nothing else changes (the mixing
    /// spaces, the selections and the color picking role's color space stay those of the last
    /// [`MixingColorSpaceManager::refresh`]), as when the config upstream's manager points to
    /// is changed in place. For the Python binding only (`ocio-py`), whose configs are nodes
    /// the library's `Arc<Config>` can't follow: it hands the manager the config as Python
    /// holds it now before the manager reads it (`getProcessor`, `repr`). Behaviour-neutral for
    /// the Rust API, whose configs don't change in place (coordinator's decision 2026-10-24, as
    /// `set_value_as_is` of the curve transforms).
    #[doc(hidden)]
    pub fn set_config_as_is(&mut self, config: Arc<Config>) {
        self.config = config;
    }

    /// Updates the slider's copy of whether the mixing space is perceptually uniform.
    fn sync_slider(&mut self) {
        self.slider.perceptually_uniform = self.is_perceptually_uniform();
    }

    /// The number of mixing spaces.
    ///
    /// Port of `MixingColorSpaceManagerImpl::getNumMixingSpaces` (MixingHelpers.cpp:169-172 @
    /// v2.5.2).
    #[doc(alias = "getNumMixingSpaces")]
    pub fn num_mixing_spaces(&self) -> usize {
        self.mixing_spaces.len()
    }

    /// The UI name of the mixing space `idx`; an error past the last one.
    ///
    /// Port of `MixingColorSpaceManagerImpl::getMixingSpaceUIName` (MixingHelpers.cpp:174-186
    /// @ v2.5.2).
    #[doc(alias = "getMixingSpaceUIName")]
    pub fn mixing_space_ui_name(&self, idx: usize) -> Result<&[u8]> {
        if let Some(name) = self.mixing_spaces.get(idx) {
            return Ok(name);
        }

        Err(Exception::new(format!(
            "Invalid mixing space index {idx} where size is {}.",
            self.mixing_spaces.len()
        )))
    }

    /// The index of the selected mixing space.
    ///
    /// Port of `MixingColorSpaceManagerImpl::getSelectedMixingSpaceIdx` (MixingHelpers.cpp:
    /// 188-191 @ v2.5.2).
    #[doc(alias = "getSelectedMixingSpaceIdx")]
    pub fn selected_mixing_space_idx(&self) -> usize {
        self.selected_mixing_space_idx
    }

    /// Selects the mixing space `idx`; an error past the last one.
    ///
    /// Port of `MixingColorSpaceManagerImpl::setSelectedMixingSpaceIdx` (MixingHelpers.cpp:
    /// 193-204 @ v2.5.2).
    #[doc(alias = "setSelectedMixingSpaceIdx")]
    pub fn set_selected_mixing_space_idx(&mut self, idx: usize) -> Result<()> {
        if idx >= self.mixing_spaces.len() {
            return Err(Exception::new(format!(
                "Invalid idx for the mixing space index {idx} where size is {}.",
                self.mixing_spaces.len()
            )));
        }

        self.selected_mixing_space_idx = idx;
        self.sync_slider();
        Ok(())
    }

    /// Selects the mixing space of the UI name `mixing_space` (up to its first NUL, as the C
    /// string upstream takes); an error when there is none. Its error for a null name can't
    /// happen with byte strings.
    ///
    /// Port of `MixingColorSpaceManagerImpl::setSelectedMixingSpace` (MixingHelpers.cpp:
    /// 206-225 @ v2.5.2).
    #[doc(alias = "setSelectedMixingSpace")]
    pub fn set_selected_mixing_space(&mut self, mixing_space: impl AsRef<[u8]>) -> Result<()> {
        let mixing_space = c_str(mixing_space.as_ref());
        if let Some(idx) = self
            .mixing_spaces
            .iter()
            .position(|s| s.as_slice() == mixing_space)
        {
            self.selected_mixing_space_idx = idx;
            self.sync_slider();
            return Ok(());
        }

        Err(Exception::new(
            [
                b"Invalid mixing space name: '".as_slice(),
                mixing_space,
                b"'.",
            ]
            .concat(),
        ))
    }

    /// Whether the selected mixing space is perceptually uniform: the color picking role's
    /// color space and the display space are taken to be, the rendering space isn't.
    ///
    /// Port of `MixingColorSpaceManagerImpl::isPerceptuallyUniform` (MixingHelpers.cpp:
    /// 227-236 @ v2.5.2).
    #[doc(alias = "isPerceptuallyUniform")]
    pub fn is_perceptually_uniform(&self) -> bool {
        // TODO: This response should vary as a function of the mixing space.
        // (The limited options above allow us to hard-code for now.)

        // Only Display color spaces are perceptually linear.
        // TODO: It's probably not always reasonable to assume the color_picking role is
        // perceptually uniform.
        if self.color_picker.is_none() {
            self.selected_mixing_space_idx() != 0
        } else {
            true
        }
    }

    /// The number of mixing encodings.
    ///
    /// Port of `MixingColorSpaceManagerImpl::getNumMixingEncodings` (MixingHelpers.cpp:
    /// 238-241 @ v2.5.2).
    #[doc(alias = "getNumMixingEncodings")]
    pub fn num_mixing_encodings(&self) -> usize {
        self.mixing_encodings.len()
    }

    /// The name of the mixing encoding `idx`; an error past the last one.
    ///
    /// Port of `MixingColorSpaceManagerImpl::getMixingEncodingName` (MixingHelpers.cpp:
    /// 243-255 @ v2.5.2).
    #[doc(alias = "getMixingEncodingName")]
    pub fn mixing_encoding_name(&self, idx: usize) -> Result<&[u8]> {
        if let Some(name) = self.mixing_encodings.get(idx) {
            return Ok(name);
        }

        Err(Exception::new(format!(
            "Invalid mixing encoding index {idx} where size is {}.",
            self.mixing_encodings.len()
        )))
    }

    /// The index of the selected mixing encoding.
    ///
    /// Port of `MixingColorSpaceManagerImpl::getSelectedMixingEncodingIdx`
    /// (MixingHelpers.cpp:257-260 @ v2.5.2).
    #[doc(alias = "getSelectedMixingEncodingIdx")]
    pub fn selected_mixing_encoding_idx(&self) -> usize {
        self.selected_mixing_encoding_idx
    }

    /// Selects the mixing encoding `idx`; an error past the last one.
    ///
    /// Port of `MixingColorSpaceManagerImpl::setSelectedMixingEncodingIdx`
    /// (MixingHelpers.cpp:262-273 @ v2.5.2).
    #[doc(alias = "setSelectedMixingEncodingIdx")]
    pub fn set_selected_mixing_encoding_idx(&mut self, idx: usize) -> Result<()> {
        if idx >= self.mixing_encodings.len() {
            return Err(Exception::new(format!(
                "Invalid idx for the mixing encoding index {idx} where size is {}.",
                self.mixing_encodings.len()
            )));
        }

        self.selected_mixing_encoding_idx = idx;
        Ok(())
    }

    /// Selects the mixing encoding named `mixing_encoding` (up to its first NUL, as the C
    /// string upstream takes); an error when there is none. Its error for a null name can't
    /// happen with byte strings.
    ///
    /// Port of `MixingColorSpaceManagerImpl::setSelectedMixingEncoding` (MixingHelpers.cpp:
    /// 275-294 @ v2.5.2).
    #[doc(alias = "setSelectedMixingEncoding")]
    pub fn set_selected_mixing_encoding(
        &mut self,
        mixing_encoding: impl AsRef<[u8]>,
    ) -> Result<()> {
        let mixing_encoding = c_str(mixing_encoding.as_ref());
        if let Some(idx) = self
            .mixing_encodings
            .iter()
            .position(|s| s.as_slice() == mixing_encoding)
        {
            self.selected_mixing_encoding_idx = idx;
            return Ok(());
        }

        Err(Exception::new(
            [
                b"Invalid mixing encoding: '".as_slice(),
                mixing_encoding,
                b"'.",
            ]
            .concat(),
        ))
    }

    /// The processor from the working space to the mixing space, in RGB: to the color picking
    /// role's color space, else to the view `view_name` of the display `display_name` (the
    /// display space) or an identity (the rendering space).
    ///
    /// Port of `MixingColorSpaceManagerImpl::getProcessorWithoutEncoding` (MixingHelpers.cpp:
    /// 296-325 @ v2.5.2).
    fn processor_without_encoding(
        &self,
        working_name: &[u8],
        display_name: &[u8],
        view_name: &[u8],
    ) -> Result<Arc<Processor>> {
        if let Some(color_picker) = &self.color_picker {
            // Mix colors in the color_picker role color space.
            self.config
                .processor_between(working_name, color_picker.name())
        } else if self.selected_mixing_space_idx() > 0 {
            // Mix colors in the selected (display, view) space.
            let mut dt = DisplayViewTransform::new();
            dt.set_display(display_name);
            dt.set_view(view_name);
            dt.set_src(working_name);
            self.config.processor(&Transform::from(dt))
        } else {
            // Mix colors directly in the working/rendering space.
            let tr = MatrixTransform::new();
            self.config.processor(&Transform::from(tr))
        }
    }

    /// The processor from the working space `working_name` to the selected mixing space, in
    /// the selected encoding (forward), or back (inverse); see
    /// [`MixingColorSpaceManager::refresh`] for the mixing spaces. The display space is the
    /// view `view_name` of the display `display_name`. Uses the config's current context.
    ///
    /// Port of `MixingColorSpaceManagerImpl::getProcessor` (MixingHelpers.cpp:327-347 @
    /// v2.5.2).
    #[doc(alias = "getProcessor")]
    pub fn processor(
        &self,
        working_name: impl AsRef<[u8]>,
        display_name: impl AsRef<[u8]>,
        view_name: impl AsRef<[u8]>,
        direction: TransformDirection,
    ) -> Result<Arc<Processor>> {
        let mut group = GroupTransform::new();

        let processor = self.processor_without_encoding(
            working_name.as_ref(),
            display_name.as_ref(),
            view_name.as_ref(),
        )?;

        group.append_transform(Transform::from(processor.create_group_transform()?));

        if self.selected_mixing_encoding_idx() == 1 {
            // i.e. HSV
            let tr = FixedFunctionTransform::new(FixedFunctionStyle::RgbToHsv, &[])?;

            group.append_transform(Transform::from(tr));
        }

        self.config
            .processor_in_direction(&Transform::from(group), direction)
    }

    /// The slider.
    ///
    /// Port of `MixingColorSpaceManagerImpl::getSlider()` (MixingHelpers.cpp:349-352 @
    /// v2.5.2).
    #[doc(alias = "getSlider")]
    pub fn slider(&mut self) -> &mut MixingSlider {
        &mut self.slider
    }

    /// The slider, with its edges set to `slider_mixing_min_edge` and `slider_mixing_max_edge`
    /// (in mixing space units).
    ///
    /// Port of `MixingColorSpaceManagerImpl::getSlider(float, float)` (MixingHelpers.cpp:
    /// 354-360 @ v2.5.2).
    #[doc(alias = "getSlider")]
    pub fn slider_with_range(
        &mut self,
        slider_mixing_min_edge: f32,
        slider_mixing_max_edge: f32,
    ) -> &mut MixingSlider {
        self.slider.set_slider_min_edge(slider_mixing_min_edge);
        self.slider.set_slider_max_edge(slider_mixing_max_edge);
        &mut self.slider
    }

    /// The manager's text: `config: <the config's cache ID>, slider: [<the slider's text>]`,
    /// then `, mixingSpaces: [<UI names, comma-separated>]` (when there are any),
    /// `, selectedMixingSpaceIdx: <n>, selectedMixingEncodingIdx: <n>` and, with the color
    /// picking role, `, colorPicking`: the text of Python's `repr()`. The config's cache ID
    /// can fail.
    ///
    /// Port of `MixingColorSpaceManagerImpl::serialize` and `operator<<(std::ostream &, const
    /// MixingColorSpaceManager &)` (MixingHelpers.cpp:362-395 @ v2.5.2), on a new stream.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut os = OStringStream::new(Crt::NATIVE);
        os.put_str("config: ");
        os.put_str(&self.config.cache_id()?);
        os.put_str(", slider: [");
        self.slider.write_text(&mut os);
        os.put_str("]");
        if !self.mixing_spaces.is_empty() {
            os.put_str(", mixingSpaces: [");
            let mut first = true;
            for cs in &self.mixing_spaces {
                if !first {
                    os.put_str(", ");
                }
                os.put_bytes(cs);
                first = false;
            }
            os.put_str("]");
        }
        os.put_str(", selectedMixingSpaceIdx: ");
        os.put_u64(self.selected_mixing_space_idx as u64);
        os.put_str(", selectedMixingEncodingIdx: ");
        os.put_u64(self.selected_mixing_encoding_idx as u64);
        if self.color_picker.is_some() {
            os.put_str(", colorPicking");
        }
        Ok(os.into_bytes())
    }
}

#[cfg(test)]
#[path = "mixing_helpers_tests.rs"]
mod tests;
