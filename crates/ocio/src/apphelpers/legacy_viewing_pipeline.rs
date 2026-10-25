// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The legacy viewing pipeline: a port of `src/OpenColorIO/apphelpers/LegacyViewingPipeline.h`
//! and `LegacyViewingPipeline.cpp` @ v2.5.2, whose public API is
//! `include/OpenColorIO/OpenColorAppHelpers.h:350-432`.
//!
//! Around a display view transform, the pipeline adds the steps of a viewport: a color
//! correction in the scene-linear role's color space (linear CC), one in the color timing
//! role's (color timing CC), the looks (the view's, or an override), a channel view (RGBA
//! swizzling), the display view transform without its looks, and a color correction of the
//! display (display CC). The direction of the display view transform is the pipeline's.

use std::fmt;
use std::sync::Arc;

use ocio_ops::cfmt::{Crt, OStringStream};
use ocio_ops::exception::{Exception, Result};
use ocio_ops::open_color_types::TransformDirection;
use ocio_ops::parse_utils::{ROLE_COLOR_TIMING, ROLE_SCENE_LINEAR};
use ocio_ops::utils::string_utils::c_str;

use crate::config::Config;
use crate::context::Context;
use crate::display::OCIO_VIEW_USE_DISPLAY_NAME;
use crate::processor::Processor;
use crate::transform::Transform;
use crate::transforms::color_space_transform::ColorSpaceTransform;
use crate::transforms::display_view_transform::DisplayViewTransform;
use crate::transforms::group_transform::GroupTransform;
use crate::transforms::look_transform::LookTransform;

/// A viewing pipeline of the sort an application's viewport uses (the OCIO v1
/// `DisplayTransform`): the display view transform given, with these optional steps around
/// it, in order:
///
/// - from the display view transform's source color space to the scene-linear role's, then
///   the linear CC;
/// - to the color timing role's color space, then the color timing CC;
/// - the looks: the view's, or the override when it is enabled (applied even to data color
///   spaces then);
/// - the channel view;
/// - the display view transform, without its looks;
/// - the display CC.
///
/// The display view transform's direction is the pipeline's.
///
/// Port of `LegacyViewingPipeline` (include/OpenColorIO/OpenColorAppHelpers.h:350-432) and
/// `LegacyViewingPipelineImpl` (src/OpenColorIO/apphelpers/LegacyViewingPipeline.h:16-67 @
/// v2.5.2), its only implementation. Upstream deletes its copy constructor, and has no
/// `createEditableCopy`. The port's `Clone` is the Python binding's: it calls the library on a
/// copy, so that no lock of its own is held while the library runs Python code (a config's I/O
/// proxy, a logging function).
#[derive(Debug, Clone, Default)]
pub struct LegacyViewingPipeline {
    /// `m_linearCC`.
    linear_cc: Option<Transform>,
    /// `m_colorTimingCC`.
    color_timing_cc: Option<Transform>,
    /// `m_channelView`.
    channel_view: Option<Transform>,
    /// `m_displayCC`.
    display_cc: Option<Transform>,
    /// `m_displayViewTransform`: a copy of the transform given, with its looks bypassed (the
    /// pipeline applies them itself).
    display_view_transform: Option<DisplayViewTransform>,
    /// `m_dtOriginalLooksBypass`: the looks bypass of the transform given.
    dt_original_looks_bypass: bool,
    /// `m_looksOverrideEnabled`.
    looks_override_enabled: bool,
    /// `m_looksOverride`.
    looks_override: Vec<u8>,
}

/// A copy of `transform`, as upstream's `createEditableCopy` makes it: an invalid
/// FixedFunctionTransform, or curves that don't validate, are refused. Upstream's pipeline
/// setters are `noexcept`, so the exception of such a copy ends the wheel's process
/// (`std::terminate`); the port returns it (docs/improvements.md, U-151).
fn editable_copy(transform: Option<&Transform>) -> Result<Option<Transform>> {
    transform.map(Transform::create_editable_copy).transpose()
}

impl LegacyViewingPipeline {
    /// A pipeline without any transform, and without a looks override.
    ///
    /// Port of `LegacyViewingPipeline::Create` (LegacyViewingPipeline.cpp:16-21 @ v2.5.2).
    #[doc(alias = "Create")]
    pub fn new() -> LegacyViewingPipeline {
        LegacyViewingPipeline::default()
    }

    /// The display view transform, with its looks bypassed.
    ///
    /// Port of `LegacyViewingPipelineImpl::getDisplayViewTransform`
    /// (LegacyViewingPipeline.cpp:28-31 @ v2.5.2).
    #[doc(alias = "getDisplayViewTransform")]
    pub fn display_view_transform(&self) -> Option<&DisplayViewTransform> {
        self.display_view_transform.as_ref()
    }

    /// Sets a copy of `dt`, whose looks bypass the pipeline keeps and then sets (the pipeline
    /// applies the looks itself), or none.
    ///
    /// Port of `LegacyViewingPipelineImpl::setDisplayViewTransform`
    /// (LegacyViewingPipeline.cpp:33-46 @ v2.5.2).
    #[doc(alias = "setDisplayViewTransform")]
    pub fn set_display_view_transform(&mut self, dt: Option<&DisplayViewTransform>) {
        if let Some(dt) = dt {
            let mut copy = dt.clone();
            self.dt_original_looks_bypass = copy.looks_bypass();
            copy.set_looks_bypass(true);
            self.display_view_transform = Some(copy);
        } else {
            self.display_view_transform = None;
        }
    }

    /// The linear CC.
    ///
    /// Port of `LegacyViewingPipelineImpl::getLinearCC` (LegacyViewingPipeline.cpp:48-51 @
    /// v2.5.2).
    #[doc(alias = "getLinearCC")]
    pub fn linear_cc(&self) -> Option<&Transform> {
        self.linear_cc.as_ref()
    }

    /// Sets a copy of `cc` (see [`Transform::create_editable_copy`]) as the linear CC, or
    /// none. A transform whose copy fails is refused, and the pipeline keeps its linear
    /// CC; the wheel's process ends there (docs/improvements.md, U-151).
    ///
    /// Port of `LegacyViewingPipelineImpl::setLinearCC` (LegacyViewingPipeline.cpp:53-63 @
    /// v2.5.2).
    #[doc(alias = "setLinearCC")]
    pub fn set_linear_cc(&mut self, cc: Option<&Transform>) -> Result<()> {
        self.linear_cc = editable_copy(cc)?;
        Ok(())
    }

    /// The color timing CC.
    ///
    /// Port of `LegacyViewingPipelineImpl::getColorTimingCC` (LegacyViewingPipeline.cpp:65-68
    /// @ v2.5.2).
    #[doc(alias = "getColorTimingCC")]
    pub fn color_timing_cc(&self) -> Option<&Transform> {
        self.color_timing_cc.as_ref()
    }

    /// Sets a copy of `cc` as the color timing CC, or none; refuses an invalid
    /// FixedFunctionTransform as [`LegacyViewingPipeline::set_linear_cc`] does.
    ///
    /// Port of `LegacyViewingPipelineImpl::setColorTimingCC` (LegacyViewingPipeline.cpp:70-80
    /// @ v2.5.2).
    #[doc(alias = "setColorTimingCC")]
    pub fn set_color_timing_cc(&mut self, cc: Option<&Transform>) -> Result<()> {
        self.color_timing_cc = editable_copy(cc)?;
        Ok(())
    }

    /// The channel view.
    ///
    /// Port of `LegacyViewingPipelineImpl::getChannelView` (LegacyViewingPipeline.cpp:82-85 @
    /// v2.5.2).
    #[doc(alias = "getChannelView")]
    pub fn channel_view(&self) -> Option<&Transform> {
        self.channel_view.as_ref()
    }

    /// Sets a copy of `transform` as the channel view, or none; refuses an invalid
    /// FixedFunctionTransform as [`LegacyViewingPipeline::set_linear_cc`] does.
    ///
    /// Port of `LegacyViewingPipelineImpl::setChannelView` (LegacyViewingPipeline.cpp:87-97 @
    /// v2.5.2).
    #[doc(alias = "setChannelView")]
    pub fn set_channel_view(&mut self, transform: Option<&Transform>) -> Result<()> {
        self.channel_view = editable_copy(transform)?;
        Ok(())
    }

    /// The display CC.
    ///
    /// Port of `LegacyViewingPipelineImpl::getDisplayCC` (LegacyViewingPipeline.cpp:99-102 @
    /// v2.5.2).
    #[doc(alias = "getDisplayCC")]
    pub fn display_cc(&self) -> Option<&Transform> {
        self.display_cc.as_ref()
    }

    /// Sets a copy of `cc` as the display CC, or none; refuses an invalid
    /// FixedFunctionTransform as [`LegacyViewingPipeline::set_linear_cc`] does.
    ///
    /// Port of `LegacyViewingPipelineImpl::setDisplayCC` (LegacyViewingPipeline.cpp:104-114 @
    /// v2.5.2).
    #[doc(alias = "setDisplayCC")]
    pub fn set_display_cc(&mut self, cc: Option<&Transform>) -> Result<()> {
        self.display_cc = editable_copy(cc)?;
        Ok(())
    }

    /// Holds these transforms as they are, without copying them (the display view transform's
    /// looks bypass as it is, and the bypass remembered by the last
    /// [`LegacyViewingPipeline::set_display_view_transform`] kept). For the Python binding only
    /// (`ocio-py`): upstream's getters return the pipeline's own copies, which Python changes in
    /// place; the binding keeps those copies and hands them to the pipeline as they are now
    /// before the pipeline reads them (`getProcessor`, `repr`). Behaviour-neutral for the Rust
    /// API, whose getters give references (coordinator's decision 2026-10-24, as
    /// `set_value_as_is` of the curve transforms).
    #[doc(hidden)]
    pub fn set_transforms_as_is(
        &mut self,
        display_view_transform: Option<DisplayViewTransform>,
        linear_cc: Option<Transform>,
        color_timing_cc: Option<Transform>,
        channel_view: Option<Transform>,
        display_cc: Option<Transform>,
    ) {
        self.display_view_transform = display_view_transform;
        self.linear_cc = linear_cc;
        self.color_timing_cc = color_timing_cc;
        self.channel_view = channel_view;
        self.display_cc = display_cc;
    }

    /// Whether the looks override replaces the view's looks (an empty override then means no
    /// looks).
    ///
    /// Port of `LegacyViewingPipelineImpl::setLooksOverrideEnabled`
    /// (LegacyViewingPipeline.cpp:117-120 @ v2.5.2).
    #[doc(alias = "setLooksOverrideEnabled")]
    pub fn set_looks_override_enabled(&mut self, enable: bool) {
        self.looks_override_enabled = enable;
    }

    /// Port of `LegacyViewingPipelineImpl::getLooksOverrideEnabled`
    /// (LegacyViewingPipeline.cpp:122-125 @ v2.5.2).
    #[doc(alias = "getLooksOverrideEnabled")]
    pub fn looks_override_enabled(&self) -> bool {
        self.looks_override_enabled
    }

    /// Sets the looks override, a look specification (`"+look_a, -look_b"`), up to its first
    /// NUL as the C string upstream takes.
    ///
    /// Port of `LegacyViewingPipelineImpl::setLooksOverride` (LegacyViewingPipeline.cpp:
    /// 127-130 @ v2.5.2).
    #[doc(alias = "setLooksOverride")]
    pub fn set_looks_override(&mut self, looks: impl AsRef<[u8]>) {
        self.looks_override = c_str(looks.as_ref()).to_vec();
    }

    /// Port of `LegacyViewingPipelineImpl::getLooksOverride` (LegacyViewingPipeline.cpp:
    /// 132-135 @ v2.5.2).
    #[doc(alias = "getLooksOverride")]
    pub fn looks_override(&self) -> &[u8] {
        &self.looks_override
    }

    /// Checks the pipeline has a display view transform, and validates its transforms: an
    /// error of theirs comes back after "LegacyViewingPipeline is not valid: ".
    ///
    /// Port of `LegacyViewingPipelineImpl::validate` (LegacyViewingPipeline.cpp:137-173 @
    /// v2.5.2).
    fn validate(&self) -> Result<&DisplayViewTransform> {
        let Some(dvt) = &self.display_view_transform else {
            return Err(Exception::new(
                "LegacyViewingPipeline: can't create a processor without a display transform.",
            ));
        };

        let check = || -> Result<()> {
            dvt.validate()?;

            for transform in [
                &self.linear_cc,
                &self.color_timing_cc,
                &self.channel_view,
                &self.display_cc,
            ]
            .into_iter()
            .flatten()
            {
                transform.validate()?;
            }
            Ok(())
        };

        // `catch (Exception & e)`: an ExceptionMissingFile too, rethrown as an Exception.
        check().map_err(|e| {
            Exception::new(
                [
                    b"LegacyViewingPipeline is not valid: ".as_slice(),
                    c_str(e.what()),
                ]
                .concat(),
            )
        })?;
        Ok(dvt)
    }

    /// The processor of the pipeline in `context`, the config's current context for `None`
    /// (the overload without a context, and the Python binding's default), in the direction of
    /// its display view transform.
    ///
    /// The group it builds, forward: the conversion from the source color space to the
    /// scene-linear role then the linear CC (when it isn't a no-op), the same for the color
    /// timing role and CC, a look transform (from the color space reached so far to the result
    /// color space of the looks: the override when enabled, else the view's unless the display
    /// view transform bypassed them), the channel view, the display view transform (forward,
    /// from the color space reached, without looks), and the display CC.
    ///
    /// With the data bypass of the display view transform, and a data source or display color
    /// space (or none, a named transform), or a channel view matrix that reads the alpha into
    /// a color channel, the color space conversions are skipped: the CCs and channel view apply
    /// to the input as it is, a look transform skips its conversions (and its result color
    /// space is the input's), and the display view transform is left out unless the view's
    /// color space is no color space (a named transform).
    ///
    /// Port of `LegacyViewingPipelineImpl::getProcessor(const ConstConfigRcPtr &, const
    /// ConstContextRcPtr &)` (LegacyViewingPipeline.cpp:180-381 @ v2.5.2), and of
    /// `getProcessor(const ConstConfigRcPtr &)` (175-178) for `None`.
    #[doc(alias = "getProcessor")]
    pub fn processor(&self, config: &Config, context: Option<&Context>) -> Result<Arc<Processor>> {
        let current;
        let context = match context {
            Some(context) => context,
            None => {
                current = config.current_context().get();
                &*current
            }
        };

        let display_view_transform = self.validate()?;

        // Get direction from display transform.
        let dir = display_view_transform.direction();

        let input_color_space_name = display_view_transform.src().to_vec();
        let Some(input_color_space) = config.color_space(&input_color_space_name) else {
            let mut os = b"LegacyViewingPipeline error: ".to_vec();
            if input_color_space_name.is_empty() {
                os.extend_from_slice(b"InputColorSpaceName is unspecified.");
            } else {
                os.extend_from_slice(b"Cannot find inputColorSpace, named '");
                os.extend_from_slice(&input_color_space_name);
                os.extend_from_slice(b"'.");
            }
            return Err(Exception::new(os));
        };

        let display = display_view_transform.display().to_vec();
        let view = display_view_transform.view().to_vec();

        // Upstream looks the view transform up and never uses it.
        let view_transform_name = config.display_view_transform_name(&display, &view);
        if !view_transform_name.is_empty() {
            let _view_transform = config.view_transform(view_transform_name);
        }

        // NB: If the viewTransform is present, then displayColorSpace is a true display color
        // space rather than a traditional color space.
        let name = config
            .display_view_color_space_name(&display, &view)
            .to_vec();
        // A shared view containing a view transform may set the color space to
        // USE_DISPLAY_NAME, in which case we look for a display color space with the same name
        // as the display.
        let name_from_display = name == OCIO_VIEW_USE_DISPLAY_NAME.as_bytes();
        let display_color_space_name = if name_from_display { &display } else { &name };
        let display_color_space = config.color_space(display_color_space_name);
        // If this is not a color space it can be a named transform. Error handling (missing
        // color space or named transform) is handled by display view transform.

        let data_bypass = display_view_transform.data_bypass();
        let display_data = display_color_space.is_none_or(|cs| cs.is_data());
        let mut skip_color_space_conversions =
            data_bypass && (input_color_space.is_data() || display_data);

        if data_bypass {
            // If we're viewing alpha, also skip all color space conversions.
            // If the user does uses a different transform for the channel view, in place of a
            // simple matrix, they run the risk that when viewing alpha the colorspace
            // transforms will not be skipped. (I.e., filmlook will be applied to alpha.) If
            // this ever becomes an issue, additional engineering will be added at that time.
            if let Some(Transform::Matrix(typed_channel_view)) = &self.channel_view {
                let matrix44 = typed_channel_view.matrix();

                if (matrix44[3] > 0.0) || (matrix44[7] > 0.0) || (matrix44[11] > 0.0) {
                    skip_color_space_conversions = true;
                }
            }
        }

        let mut current_cs_name = input_color_space_name.clone();
        let mut dt_input_color_space = input_color_space;

        let mut group = GroupTransform::new();

        if let Some(linear_cc_transform) = &self.linear_cc {
            let linear_cc = config.processor_with_context(context, linear_cc_transform, dir)?;
            // If it is a no-op, dont bother doing the colorspace conversion.
            if !linear_cc.is_no_op()? {
                let Some(scene_linear_cs) = config.color_space(ROLE_SCENE_LINEAR) else {
                    return Err(Exception::new(
                        [
                            "DisplayViewTransform error:",
                            " LinearCC requires '",
                            ROLE_SCENE_LINEAR,
                            "' role to be defined.",
                        ]
                        .concat(),
                    ));
                };
                dt_input_color_space = scene_linear_cs;

                if !skip_color_space_conversions {
                    let mut cst = ColorSpaceTransform::new();
                    cst.set_src(&current_cs_name);
                    cst.set_dst(ROLE_SCENE_LINEAR);
                    current_cs_name = ROLE_SCENE_LINEAR.as_bytes().to_vec();
                    group.append_transform(Transform::from(cst));
                }

                group.append_transform(linear_cc_transform.clone());
            }
        }

        if let Some(color_timing_cc_transform) = &self.color_timing_cc {
            let color_timing_cc =
                config.processor_with_context(context, color_timing_cc_transform, dir)?;
            // If it is a no-op, dont bother doing the colorspace conversion.
            if !color_timing_cc.is_no_op()? {
                let Some(color_timing_cs) = config.color_space(ROLE_COLOR_TIMING) else {
                    return Err(Exception::new(
                        [
                            "DisplayViewTransform error:",
                            " ColorTimingCC requires '",
                            ROLE_COLOR_TIMING,
                            "' role to be defined.",
                        ]
                        .concat(),
                    ));
                };
                dt_input_color_space = color_timing_cs;

                if !skip_color_space_conversions {
                    let mut cst = ColorSpaceTransform::new();
                    cst.set_src(&current_cs_name);
                    cst.set_dst(ROLE_COLOR_TIMING);
                    current_cs_name = ROLE_COLOR_TIMING.as_bytes().to_vec();
                    group.append_transform(Transform::from(cst));
                }
                group.append_transform(color_timing_cc_transform.clone());
            }
        }

        let mut dt = display_view_transform.clone();
        dt.set_direction(TransformDirection::Forward);

        // Adjust display transform input color space.
        dt.set_src(&current_cs_name);

        // NB: If looksOverrideEnabled is true, always apply the look, even to data color
        // spaces. In other cases, follow what the DisplayViewTransform would do, except skip
        // color space conversions to the process space for Look transforms for data spaces
        // (DisplayViewTransform never skips).
        let mut looks: Vec<u8> = Vec::new();
        if self.looks_override_enabled {
            looks = self.looks_override.clone();
        } else if !self.dt_original_looks_bypass && !skip_color_space_conversions {
            looks = c_str(config.display_view_looks(&display, &view)).to_vec();
        }

        if !looks.is_empty() {
            let in_cs = dt_input_color_space.name();
            let out_cs = if skip_color_space_conversions {
                in_cs
            } else {
                LookTransform::looks_result_color_space(config, context, &looks)?
            };

            // Resulting color space could be null in case of a noop look.
            if !c_str(out_cs).is_empty() {
                let mut lt = LookTransform::new();
                lt.set_src(in_cs);
                lt.set_dst(out_cs);
                lt.set_looks(&looks);
                lt.set_skip_color_space_conversion(skip_color_space_conversions);

                group.append_transform(Transform::from(lt));

                // Adjust display transform input color space.
                dt.set_src(out_cs);
            }
        }

        if let Some(channel_view) = &self.channel_view {
            group.append_transform(channel_view.clone());
        }

        // If there is no displayColorSpace it should be a named transform and it has to be
        // applied.
        if !skip_color_space_conversions || display_color_space.is_none() {
            group.append_transform(Transform::from(dt));
        }

        if let Some(display_cc) = &self.display_cc {
            group.append_transform(display_cc.clone());
        }

        config.processor_with_context(context, &Transform::from(group), dir)
    }

    /// The pipeline's text, in bytes (see the `Display` implementation): names and paths in
    /// it pass through unchanged, where `Display` replaces what isn't UTF-8.
    ///
    /// Port of `operator<<(std::ostream &, const LegacyViewingPipeline &)`
    /// (LegacyViewingPipeline.cpp:383-447 @ v2.5.2), on a new stream.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut os = OStringStream::new(Crt::NATIVE);
        self.write_text(&mut os);
        os.into_bytes()
    }

    /// Writes the pipeline's text to `os`: one stream for every transform, so what a transform
    /// leaves in its state stays for the next (a MatrixTransform's precision,
    /// docs/improvements.md, I-73).
    ///
    /// Port of `operator<<(std::ostream &, const LegacyViewingPipeline &)`
    /// (LegacyViewingPipeline.cpp:383-447 @ v2.5.2).
    fn write_text(&self, os: &mut OStringStream) {
        let mut first = true;
        if let Some(dvt) = &self.display_view_transform {
            os.put_str("DisplayViewTransform: ");
            dvt.write_text(os);
            first = false;
        }
        for (label, transform) in [
            ("LinearCC: ", &self.linear_cc),
            ("ColorTimingCC: ", &self.color_timing_cc),
            ("ChannelView: ", &self.channel_view),
            ("DisplayCC: ", &self.display_cc),
        ] {
            if let Some(transform) = transform {
                if !first {
                    os.put_str(", ");
                }
                os.put_str(label);
                transform.write_text(os);
                first = false;
            }
        }
        if self.looks_override_enabled {
            if !first {
                os.put_str(", ");
            }
            os.put_str("LooksOverrideEnabled");
            first = false;
        }
        let lo = &self.looks_override;
        if !lo.is_empty() {
            if !first {
                os.put_str(", ");
            }
            os.put_str("LooksOverride: ");
            os.put_bytes(lo);
        }
    }
}

impl fmt::Display for LegacyViewingPipeline {
    /// `DisplayViewTransform: <...>, LinearCC: <...>, ColorTimingCC: <...>, ChannelView: <...>,
    /// DisplayCC: <...>, LooksOverrideEnabled, LooksOverride: <looks>`, each part only when
    /// set: the text of Python's `repr()`.
    ///
    /// Port of `operator<<(std::ostream &, const LegacyViewingPipeline &)`
    /// (LegacyViewingPipeline.cpp:383-447 @ v2.5.2), on a new stream.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&String::from_utf8_lossy(&self.to_bytes()))
    }
}

#[cfg(test)]
#[path = "legacy_viewing_pipeline_tests.rs"]
mod tests;
