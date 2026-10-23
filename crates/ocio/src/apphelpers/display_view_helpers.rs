// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The (display, view) helpers: a port of `src/OpenColorIO/apphelpers/DisplayViewHelpers.cpp` @
//! v2.5.2, whose public API is `include/OpenColorIO/OpenColorAppHelpers.h:295-348`
//! (`namespace DisplayViewHelpers`).
//!
//! - [`processor`]: the processor from a working color space to a (display, view) pair, with
//!   exposure, contrast and gamma controls as dynamic properties;
//! - [`identity_processor`]: those controls alone;
//! - [`add_display_view`]: a new (display, view) pair whose color space applies a LUT file;
//! - [`remove_display_view`]: removes a pair, and its color space when nothing uses it.

use std::sync::Arc;

use ocio_ops::exception::{Exception, Result};
use ocio_ops::open_color_types::{
    ColorSpaceDirection, ExposureContrastStyle, TransformDirection, combine_transform_directions,
};
use ocio_ops::platform::get_env_variable;
use ocio_ops::utils::string_utils::{self, StringVec, c_str};

use crate::apphelpers::category_helpers::{extract_items, find_color_space_names};
use crate::apphelpers::legacy_viewing_pipeline::LegacyViewingPipeline;
use crate::color_space::ColorSpace;
use crate::config::{Config, OCIO_ACTIVE_DISPLAYS_ENVVAR, OCIO_ACTIVE_VIEWS_ENVVAR};
use crate::context::Context;
use crate::processor::Processor;
use crate::transform::Transform;
use crate::transforms::display_view_transform::DisplayViewTransform;
use crate::transforms::exposure_contrast_transform::ExposureContrastTransform;
use crate::transforms::file_transform::FileTransform;
use crate::transforms::group_transform::GroupTransform;
use crate::transforms::matrix_transform::MatrixTransform;

/// The exposure and contrast control of the processors: a linear ExposureContrastTransform,
/// pivot 0.18, whose exposure and contrast are dynamic.
fn exposure_contrast() -> ExposureContrastTransform {
    let mut ex = ExposureContrastTransform::new();

    ex.set_style(ExposureContrastStyle::Linear);
    ex.set_pivot(0.18);

    ex.make_exposure_dynamic();
    ex.make_contrast_dynamic();
    ex
}

/// The gamma control of the processors: a video ExposureContrastTransform, pivot 1, whose gamma
/// is dynamic.
fn gamma() -> ExposureContrastTransform {
    let mut ex = ExposureContrastTransform::new();
    ex.set_style(ExposureContrastStyle::Video);
    ex.set_pivot(1.0);

    ex.make_gamma_dynamic();
    ex
}

/// The processor from the working color space `working_name` (a color space or a role) to the
/// view `view_name` of the display `display_name` (forward), or back (inverse), in `context`
/// (the config's current context for `None`, the overload without a context).
///
/// Unless the display view transform's processor already has them (an ExposureContrastTransform
/// whose exposure, or gamma, is dynamic), the processor gets a dynamic exposure and contrast
/// in the scene-linear role's color space (linear, pivot 0.18) and a dynamic gamma at the end
/// (video, pivot 1), through a [`LegacyViewingPipeline`], with the channel view
/// `channel_view`. With both already there and no channel view, it is the display view
/// transform's processor.
///
/// Port of `DisplayViewHelpers::GetProcessor` (src/OpenColorIO/apphelpers/
/// DisplayViewHelpers.cpp:24-124 @ v2.5.2): both overloads, with the context of the Python
/// binding's (owner decision 2026-10-22, P5-3 b).
#[doc(alias = "GetProcessor")]
pub fn processor(
    config: &Config,
    context: Option<&Context>,
    working_name: impl AsRef<[u8]>,
    display_name: impl AsRef<[u8]>,
    view_name: impl AsRef<[u8]>,
    channel_view: Option<&MatrixTransform>,
    direction: TransformDirection,
) -> Result<Arc<Processor>> {
    let current;
    let context = match context {
        Some(context) => context,
        None => {
            current = config.current_context().get();
            &*current
        }
    };

    let mut display_transform = DisplayViewTransform::new();
    display_transform.set_direction(direction);
    display_transform.set_src(working_name);
    display_transform.set_display(display_name);
    display_transform.set_view(view_name);

    let processor = config.processor_with_context(
        context,
        &Transform::from(display_transform.clone()),
        TransformDirection::Forward,
    )?;

    let mut need_exposure = true;
    let mut need_gamma = true;

    if processor.is_dynamic() {
        let grp_transform = processor.create_group_transform()?;

        let max_transform = grp_transform.num_transforms();
        for idx in 0..max_transform {
            if let Transform::ExposureContrast(ex) = grp_transform.transform(idx)? {
                if ex.is_exposure_dynamic() {
                    need_exposure = false;
                }

                if ex.is_gamma_dynamic() {
                    need_gamma = false;
                }
            }
        }
    }

    if !need_exposure && !need_gamma && channel_view.is_none() {
        return Ok(processor);
    }

    let mut pipeline = LegacyViewingPipeline::new();
    pipeline.set_display_view_transform(Some(&display_transform));

    // TODO: Need to update to allow for apps that are viewing non-scene-linear images.
    if need_exposure {
        pipeline.set_linear_cc(Some(&Transform::from(exposure_contrast())))?;
    }

    if need_gamma {
        pipeline.set_display_cc(Some(&Transform::from(gamma())))?;
    }

    if let Some(channel_view) = channel_view {
        pipeline.set_channel_view(Some(&Transform::from(channel_view.clone())))?;
    }

    pipeline.processor(config, Some(context))
}

/// A processor of the exposure, contrast and gamma controls alone (see [`processor`]): an
/// identity until their dynamic properties change.
///
/// Port of `DisplayViewHelpers::GetIdentityProcessor` (DisplayViewHelpers.cpp:126-150 @
/// v2.5.2).
#[doc(alias = "GetIdentityProcessor")]
pub fn identity_processor(config: &Config) -> Result<Arc<Processor>> {
    let mut group = GroupTransform::new();
    group.append_transform(Transform::from(exposure_contrast()));
    group.append_transform(Transform::from(gamma()));
    config.processor(&Transform::from(group))
}

/// `envvar`'s list (split on `,`, trimmed), when the variable is set and not empty: it controls
/// the config's active list then.
fn env_list(envvar: &str) -> Option<StringVec> {
    let value = get_env_variable(envvar);
    if value.is_empty() {
        return None;
    }
    let mut list = string_utils::split(&value, b',');
    string_utils::trim_vec(&mut list);
    Some(list)
}

/// `list` (the config's active displays or views), split on `,` and trimmed, when it isn't
/// empty.
fn active_list(list: &[u8]) -> Option<StringVec> {
    let list = c_str(list);
    if list.is_empty() {
        return None;
    }
    let mut list = string_utils::split(list, b',');
    string_utils::trim_vec(&mut list);
    Some(list)
}

/// Whether `list` is the list of one empty name, which accepts every name.
fn accepts_all(list: &StringVec) -> bool {
    list.len() == 1 && list[0].is_empty()
}

/// Adds `display_name` to the config's active displays and `view_name` to its active views,
/// where the config has such a list (that doesn't accept every name) without them. Fails when
/// `OCIO_ACTIVE_DISPLAYS` (or `OCIO_ACTIVE_VIEWS`) controls the list.
///
/// Port of `DisplayViewHelpers::AddActiveDisplayView` (DisplayViewHelpers.cpp:152-231 @
/// v2.5.2). Its return for a null name can't happen with byte strings.
fn add_active_display_view(
    config: &mut Config,
    display_name: &[u8],
    view_name: &[u8],
) -> Result<()> {
    // Add the display to the active display list only if possible.

    if let Some(displays) = env_list(OCIO_ACTIVE_DISPLAYS_ENVVAR) {
        if !accepts_all(&displays) {
            return Err(Exception::new(format!(
                "Forbidden to add an active display as '{OCIO_ACTIVE_DISPLAYS_ENVVAR}' controls \
                 the active list."
            )));
        }
    } else if let Some(mut displays) = active_list(&config.active_displays())
        && !accepts_all(&displays)
        && !string_utils::contain(&displays, display_name)
    {
        displays.push(c_str(display_name).to_vec());
        let disp = string_utils::join(&displays, b',');
        config.set_active_displays(&disp)?;
    }

    // Add the view to the active view list, only if needed.

    if let Some(views) = env_list(OCIO_ACTIVE_VIEWS_ENVVAR) {
        if !accepts_all(&views) {
            return Err(Exception::new(format!(
                "Forbidden to add an active view as '{OCIO_ACTIVE_VIEWS_ENVVAR}' controls the \
                 active list."
            )));
        }
    } else if let Some(mut views) = active_list(&config.active_views())
        && !accepts_all(&views)
        && !string_utils::contain(&views, view_name)
    {
        views.push(c_str(view_name).to_vec());
        let v = string_utils::join(&views, b',');
        config.set_active_views(&v)?;
    }
    Ok(())
}

/// Removes `display_name` from the config's active displays when no (active) display of that
/// name is left, and `view_name` from its active views when no display, active or not, has a
/// view of that name left. Fails when `OCIO_ACTIVE_DISPLAYS` (or `OCIO_ACTIVE_VIEWS`) controls
/// the list.
///
/// Port of `DisplayViewHelpers::RemoveActiveDisplayView` (DisplayViewHelpers.cpp:233-376 @
/// v2.5.2). Its return for a null name can't happen with byte strings.
fn remove_active_display_view(
    config: &mut Config,
    display_name: &[u8],
    view_name: &[u8],
) -> Result<()> {
    // Remove the display from the active display list only if possible.

    if let Some(displays) = env_list(OCIO_ACTIVE_DISPLAYS_ENVVAR) {
        if !accepts_all(&displays) {
            return Err(Exception::new(format!(
                "Forbidden to remove an active display as '{OCIO_ACTIVE_DISPLAYS_ENVVAR}' \
                 controls the active list."
            )));
        }
    } else if let Some(mut displays) = active_list(&config.active_displays())
        && !accepts_all(&displays)
        && string_utils::contain(&displays, display_name)
    {
        // As the display is an active one, not finding it in the list means that it could
        // be removed from the 'active_displays' list.

        let mut to_remove = true;
        let num_displays = config.num_displays();
        let mut disp_idx = 0;
        while disp_idx < num_displays && to_remove {
            let disp_name = config.display(disp_idx);
            if string_utils::compare(disp_name, display_name) {
                to_remove = false;
            }
            disp_idx += 1;
        }

        if to_remove {
            string_utils::remove(&mut displays, display_name);
            let actives = string_utils::join(&displays, b',');
            config.set_active_displays(&actives)?;
        }
    }

    // Remove the view from the active view list only if possible.

    if let Some(views) = env_list(OCIO_ACTIVE_VIEWS_ENVVAR) {
        if !accepts_all(&views) {
            return Err(Exception::new(format!(
                "Forbidden to remove an active view as '{OCIO_ACTIVE_VIEWS_ENVVAR}' controls \
                 the active list."
            )));
        }
    } else if let Some(mut views) = active_list(&config.active_views())
        && !accepts_all(&views)
        && string_utils::contain(&views, view_name)
    {
        // As the view is an active one, not finding it in the list means that it could be
        // removed. But the code needs to loop over all the displays i.e. active and
        // inactive ones, before validating the removal.

        // Upstream's `EnableAllDisplays` guard: every display is active while the views
        // are searched, and the config's list comes back (from its text) at the end of
        // the block, after the views' list changed.
        let active_displays = config.active_displays();
        config.set_active_displays(b"")?;

        let mut to_remove = true;

        let num_displays = config.num_displays();
        let mut disp_idx = 0;
        while disp_idx < num_displays && to_remove {
            let disp_name = config.display(disp_idx).to_vec();

            let num_views = config.num_views(&disp_name);
            let mut view_idx = 0;
            while view_idx < num_views && to_remove {
                let vw_name = config.view(&disp_name, view_idx);
                if string_utils::compare(vw_name, view_name) {
                    to_remove = false;
                }
                view_idx += 1;
            }
            disp_idx += 1;
        }

        let changed = if to_remove {
            string_utils::remove(&mut views, view_name);
            let actives = string_utils::join(&views, b',');
            config.set_active_views(&actives)
        } else {
            Ok(())
        };

        config.set_active_displays(&active_displays)?;
        changed?;
    }
    Ok(())
}

/// Adds the color space `color_space`, whose transform from its reference is the connection
/// color space's from the reference (or the inverse of its transform to the reference) then
/// `user_transform`, and the view `view_name` of `display_name` for it with the looks
/// `look_definition`, made active where the config's active lists need it.
///
/// Port of `DisplayViewHelpers::AddDisplayView(ConfigRcPtr &, const char *, const char *, const
/// char *, const ColorSpaceRcPtr &, FileTransformRcPtr &, const char *)`
/// (DisplayViewHelpers.cpp:378-449 @ v2.5.2). Its "(null)" for a null connection name can't
/// happen with byte strings.
fn add_display_view_with_color_space(
    config: &mut Config,
    display_name: &[u8],
    view_name: &[u8],
    look_definition: &[u8],
    color_space: &mut ColorSpace,
    user_transform: FileTransform,
    connection_color_space_name: &[u8],
) -> Result<()> {
    let display_name = c_str(display_name);
    let view_name = c_str(view_name);
    if display_name.is_empty() {
        return Err(Exception::new("Invalid display name."));
    }

    if view_name.is_empty() {
        return Err(Exception::new("Invalid view name."));
    }

    // Step 1 - Create the color transformation.

    let mut grp = GroupTransform::new();

    // Add the 'reference' to connection color space.
    {
        // Check for an active or inactive color space.
        let Some(connection_cs) = config.color_space(connection_color_space_name) else {
            return Err(Exception::new(
                [
                    b"Connection color space name '".as_slice(),
                    c_str(connection_color_space_name),
                    b"' does not exist.",
                ]
                .concat(),
            ));
        };

        if let Some(tr) = connection_cs.transform(ColorSpaceDirection::FromReference) {
            grp.append_transform(tr.create_editable_copy()?);
        } else if let Some(tr) = connection_cs.transform(ColorSpaceDirection::ToReference) {
            let mut t = tr.create_editable_copy()?;
            t.set_direction(combine_transform_directions(
                tr.direction(),
                TransformDirection::Inverse,
            ));
            grp.append_transform(t);
        }
    }

    // Add the 'LUT' transform.
    grp.append_transform(Transform::from(user_transform));

    grp.validate()?;

    // Step 2 - Add active display and view.

    add_active_display_view(config, display_name, view_name)?;

    // Step 3 - Add the color space to the config.

    color_space.set_transform(
        Some(&Transform::from(grp)),
        ColorSpaceDirection::FromReference,
    )?;
    config.add_color_space(color_space)?;

    // Step 4 - Add a new active (display, view) pair.

    config.add_display_view(display_name, view_name, color_space.name(), look_definition)
}

/// Adds the view `view_name` of the display `display_name` (made if needed), with the looks
/// `look_definition` (may be empty), and its color space: named `color_space_name`, with the
/// family `color_space_family` and the description `color_space_description` (each may be
/// empty), the categories `categories` (a comma-separated list; added only when an active color
/// space of the config already has one of them), and the transform from the reference that goes
/// to the connection color space `connection_color_space_name` and then applies the file
/// `transform_file_path` (a FileTransform). The display and view become active where the
/// config's active lists need it.
///
/// Fails with "Color space name '<name>' already exists." for a color space (or alias, or role)
/// of that name, "Invalid display name.", "Invalid view name.", "Connection color space name
/// '<name>' does not exist.", "Forbidden to add an active display as 'OCIO_ACTIVE_DISPLAYS'
/// controls the active list." (or view, `OCIO_ACTIVE_VIEWS`), and what validating the
/// transforms, `Config::add_color_space` and `Config::add_display_view` refuse. As upstream's,
/// the config keeps what was done before an error (the active lists, then the color space).
///
/// Port of `DisplayViewHelpers::AddDisplayView(ConfigRcPtr &, const char *, const char *, const
/// char *, const char *, const char *, const char *, const char *, const char *, const char *)`
/// (include/OpenColorIO/OpenColorAppHelpers.h:325-334, DisplayViewHelpers.cpp:451-501 @
/// v2.5.2). Its null names, which upstream reads as empty ones, are empty byte strings.
#[doc(alias = "AddDisplayView")]
#[allow(clippy::too_many_arguments)]
pub fn add_display_view(
    config: &mut Config,
    display_name: impl AsRef<[u8]>,
    view_name: impl AsRef<[u8]>,
    look_definition: impl AsRef<[u8]>,
    color_space_name: impl AsRef<[u8]>,
    color_space_family: impl AsRef<[u8]>,
    color_space_description: impl AsRef<[u8]>,
    categories: impl AsRef<[u8]>,
    transform_file_path: impl AsRef<[u8]>,
    connection_color_space_name: impl AsRef<[u8]>,
) -> Result<()> {
    let mut color_space = ColorSpace::new();
    color_space.set_name(c_str(color_space_name.as_ref()));
    color_space.set_family(c_str(color_space_family.as_ref()));
    color_space.set_description(c_str(color_space_description.as_ref()));

    // Check if the name is already a color space or a role name.
    if config.color_space(color_space.name()).is_some() {
        return Err(Exception::new(
            [
                b"Color space name '".as_slice(),
                color_space.name(),
                b"' already exists.",
            ]
            .concat(),
        ));
    }

    // Add categories if any.
    let categories = c_str(categories.as_ref());
    if !categories.is_empty() {
        let cats = extract_items(Some(categories));

        // Only add the categories if already used.
        let names = find_color_space_names(config, &cats);
        if !names.is_empty() {
            for cat in &cats {
                color_space.add_category(cat);
            }
        }
    }

    let mut file = FileTransform::new();
    file.set_src(transform_file_path);

    add_display_view_with_color_space(
        config,
        display_name.as_ref(),
        view_name.as_ref(),
        look_definition.as_ref(),
        &mut color_space,
        file,
        connection_color_space_name.as_ref(),
    )
}

/// Removes the view `view_name` of the display `display_name` (and the display when it has no
/// view left), takes them out of the config's active lists where nothing else of that name is
/// left, and removes the view's color space when nothing uses it anymore.
///
/// The color space is the view's, or, for a view the display doesn't have, the color space
/// named like the display: `Config::remove_display_view` then refuses the missing view.
/// "Missing color space for '<display>' and '<view>'." for an empty display name (upstream
/// reaches it only for a view without a color space of a display named "").
///
/// Port of `DisplayViewHelpers::RemoveDisplayView` (include/OpenColorIO/
/// OpenColorAppHelpers.h:336-342, DisplayViewHelpers.cpp:503-533 @ v2.5.2). Its return for a
/// null name can't happen with byte strings.
#[doc(alias = "RemoveDisplayView")]
pub fn remove_display_view(
    config: &mut Config,
    display_name: impl AsRef<[u8]>,
    view_name: impl AsRef<[u8]>,
) -> Result<()> {
    let display_name = c_str(display_name.as_ref());
    let view_name = c_str(view_name.as_ref());

    let name = c_str(config.display_view_color_space_name(display_name, view_name)).to_vec();
    let cs_name = if name.is_empty() {
        display_name.to_vec()
    } else {
        name
    };
    if cs_name.is_empty() {
        return Err(Exception::new(
            [
                b"Missing color space for '".as_slice(),
                display_name,
                b"' and '",
                view_name,
                b"'.",
            ]
            .concat(),
        ));
    }

    // Step 1 - Remove the (display, view) pair.

    config.remove_display_view(display_name, view_name)?;

    // Setp 2 - Remove the (display, view) pair from active lists if possible.

    remove_active_display_view(config, display_name, view_name)?;

    // Step 3 - Remove the associated color space if not used.

    if !config.is_color_space_used(&cs_name) {
        config.remove_color_space(&cs_name);
    }
    Ok(())
}

#[cfg(test)]
#[path = "display_view_helpers_tests.rs"]
mod tests;
