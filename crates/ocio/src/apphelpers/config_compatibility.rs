// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The config compatibility helpers: a port of `src/OpenColorIO/apphelpers/ConfigCompatibility.cpp`
//! and `ConfigCompatibility.h` @ v2.6.0, whose public API is
//! `include/OpenColorIO/OpenColorAppHelpers.h:278-289` (`namespace ConfigCompatibilityHelpers`).
//!
//! - [`check_compatibility`]: whether a config meets a requirement, as of an OCIO version
//!   ([`ConfigCompatibility`]); each unmet condition is logged at the debug level.

use ocio_ops::logging::log_debug;
use ocio_ops::open_color_types::{
    ColorSpaceVisibility, ConfigCompatibility, ReferenceSpaceType, SearchReferenceSpaceType,
};
use ocio_ops::parse_utils::ROLE_INTERCHANGE_DISPLAY;
use ocio_ops::utils::string_utils::c_str;

use crate::config::Config;

/// `msg` and the bytes of each name between its parts, as upstream concatenates
/// `std::string`s for `LogDebug`.
fn concat(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

/// Whether the config has at least one display color space (active or inactive) and all of
/// them have an interop ID and an encoding.
///
/// Port of `ConfigCompatibilityHelpers::DisplayColorSpacesHaveAttributes`
/// (src/OpenColorIO/apphelpers/ConfigCompatibility.cpp:19-60 @ v2.6.0).
pub(crate) fn display_color_spaces_have_attributes(config: &Config) -> bool {
    // Search all (active & inactive) display color spaces.

    let num_cs =
        config.num_color_spaces_with(SearchReferenceSpaceType::Display, ColorSpaceVisibility::All);
    if num_cs == 0 {
        log_debug("HDR Display Support (2.6): No display-referred color spaces found.");
        return false;
    }

    let mut all_have_interop_id = true;

    for i in 0..num_cs {
        let cs_name = config.color_space_name_by_index_with(
            SearchReferenceSpaceType::Display,
            ColorSpaceVisibility::All,
            i,
        );
        let cs = config.color_space(cs_name);

        // Having the interop ID allows applications to match up a display color space with
        // external color space definitions, such as operating system color space enums.

        if cs.is_none_or(|cs| c_str(cs.interop_id()).is_empty()) {
            log_debug(concat(&[
                b"HDR Display Support (2.6): Display color space '",
                c_str(cs_name),
                b"' has no interop ID.",
            ]));
            all_have_interop_id = false;
        }

        // The encoding attribute allows applications to know whether this color space is for
        // an SDR or HDR monitor and whether it is a linear or gamma-corrected encoding.

        if cs.is_none_or(|cs| c_str(cs.encoding()).is_empty()) {
            log_debug(concat(&[
                b"HDR Display Support (2.6): Display color space '",
                c_str(cs_name),
                b"' has no encoding.",
            ]));
            all_have_interop_id = false;
        }
    }

    all_have_interop_id
}

/// Whether the config has at least one active display, and each active display has a
/// display-referred color space of its name (found as `getColorSpace` finds one: by name,
/// alias or role).
///
/// Port of `ConfigCompatibilityHelpers::ActiveDisplaysHaveColorSpace`
/// (src/OpenColorIO/apphelpers/ConfigCompatibility.cpp:62-91 @ v2.6.0).
pub(crate) fn active_displays_have_color_space(config: &Config) -> bool {
    // Only check active displays.
    let num_displays = config.num_displays();
    if num_displays == 0 {
        log_debug("HDR Display Support (2.6): No active displays.");
        return false;
    }

    let mut all_have_color_space = true;

    // Having a display color space for each display allows applications to convert the output
    // from a DisplayViewTransform back to the display-referred reference space and from there
    // to any color space that may be required by the operating system.

    for d in 0..num_displays {
        let display = config.display(d);
        let cs = config.color_space(display);
        if cs.is_none_or(|cs| cs.reference_space_type() != ReferenceSpaceType::Display) {
            log_debug(concat(&[
                b"HDR Display Support (2.6): Display '",
                c_str(display),
                b"' has no matching display-referred color space.",
            ]));
            all_have_color_space = false;
        }
    }

    all_have_color_space
}

/// Whether the config has at least one active (display, view) pair using a view transform,
/// and every active view has a view transform, but for a view whose color space is a data
/// color space or names a named transform.
///
/// Port of `ConfigCompatibilityHelpers::ActiveViewsHaveViewTransform`
/// (src/OpenColorIO/apphelpers/ConfigCompatibility.cpp:93-168 @ v2.6.0).
pub(crate) fn active_views_have_view_transform(config: &Config) -> bool {
    // Only check active displays.
    let num_displays = config.num_displays();

    let mut num_views_total = 0;
    let mut all_have_view_transform = true;
    let mut has_view_transform = false;

    for d in 0..num_displays {
        let display = config.display(d);
        // Only check active views.
        let num_views = config.num_views(display);
        for v in 0..num_views {
            num_views_total += 1;

            // Being structured as a ViewTransform plus a display color space is the
            // best-practice for configs and structures the processing to give the application
            // the most flexibility to conform to the OS requirements.

            let view = config.view(display, v);
            let transform_name = config.display_view_transform_name(display, view);
            if !c_str(transform_name).is_empty() {
                has_view_transform = true;
                continue;
            }

            // However, sometimes a config will include a few "utility" views, such as a "Raw"
            // view, implemented as a color space where isData is true, and this should not
            // disqualify a config.

            let cs_name = c_str(config.display_view_color_space_name(display, view));
            let cs = if cs_name.is_empty() {
                None
            } else {
                config.color_space(cs_name)
            };
            if cs.is_some_and(|cs| cs.is_data()) {
                continue;
            }

            // Similarly, configs sometimes will structure "utility" views using a
            // NamedTransform as the color space. End-user expectations for such views is not
            // that they will provide exact colorimetry. As long as the application has a
            // display color space for the view, that should be sufficient context for how to
            // handle the resulting output.

            let nt = if cs_name.is_empty() {
                None
            } else {
                config.named_transform(cs_name)
            };
            if nt.is_some() {
                continue;
            }

            log_debug(concat(&[
                b"HDR Display Support (2.6): Active (display, view) pair ('",
                c_str(display),
                b"', '",
                c_str(view),
                b"') has no view transform and is not a data color space or a named transform.",
            ]));
            all_have_view_transform = false;
        }
    }

    if num_views_total == 0 {
        log_debug("HDR Display Support (2.6): No active (display, view) pairs.");
        return false;
    }

    if !has_view_transform {
        log_debug(
            "HDR Display Support (2.6): No active (display, view) pair references a view \
             transform.",
        );
    }

    all_have_view_transform && has_view_transform
}

/// Whether the config meets the HDR display requirements of OCIO 2.6: the
/// `cie_xyz_d65_interchange` role, the display color spaces' attributes
/// ([`display_color_spaces_have_attributes`]), a display color space for each active display
/// ([`active_displays_have_color_space`]) and view transforms for the active views
/// ([`active_views_have_view_transform`]). Every check runs, each logging what it misses.
///
/// Port of `ConfigCompatibilityHelpers::CheckHDRDisplaySupport26`
/// (src/OpenColorIO/apphelpers/ConfigCompatibility.cpp:170-208 @ v2.6.0).
pub(crate) fn check_hdr_display_support_26(config: &Config) -> bool {
    let mut compatible = true;

    // Ideally, an application will be able to use the interop IDs in the display color spaces
    // to identify one that corresponds to the color space required by the OS. An OCIO
    // ColorSpaceTransform may then be used to convert from the display color space produced by
    // the DisplayViewTransform into the required OS color space.
    //
    // However, if that is not available, the display-referred interchange role is a key piece
    // of information for applications, allowing them to invert the display color spaces back
    // to CIE XYZ D65 and then convert forward to whatever color space is required. For example,
    // the interchange role facilitates using GetProcessorFromConfigs to convert from a display
    // color space in a user's config to a display color space in one of the built-in configs.

    if !config.has_role(ROLE_INTERCHANGE_DISPLAY) {
        log_debug("HDR Display Support (2.6): Missing the 'cie_xyz_d65_interchange' role.");
        compatible = false;
    }

    if !display_color_spaces_have_attributes(config) {
        compatible = false;
    }

    if !active_displays_have_color_space(config) {
        compatible = false;
    }

    if !active_views_have_view_transform(config) {
        compatible = false;
    }

    compatible
}

/// Whether `config` meets the requirement `compatibility` names.
///
/// Port of `ConfigCompatibilityHelpers::CheckCompatibility`
/// (src/OpenColorIO/apphelpers/ConfigCompatibility.cpp:210-219 @ v2.6.0).
#[doc(alias = "CheckCompatibility")]
pub fn check_compatibility(config: &Config, compatibility: ConfigCompatibility) -> bool {
    match compatibility {
        ConfigCompatibility::HdrDisplaySupport26 => check_hdr_display_support_26(config),
    }
}

#[cfg(test)]
#[path = "config_compatibility_tests.rs"]
mod tests;
