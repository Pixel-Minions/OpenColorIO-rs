// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Displays and views: a port of `src/OpenColorIO/Display.h` and `Display.cpp` @ v2.5.2. A
//! config keeps its displays in their order, each with its own views and the names of the
//! shared views it uses, and the shared views themselves.
//!
//! Strings are bytes, as upstream's C strings: each ends at its first NUL.

use ocio_ops::parse_utils::intersect_string_vecs_case_ignore;
use ocio_ops::platform::strcasecmp;
use ocio_ops::utils::string_utils::{StringVec, c_str};

/// A shared view using this for the color space name will use a display color space that has
/// the same name as the display the shared view is used by.
///
/// Port of `OCIO_VIEW_USE_DISPLAY_NAME` (src/OpenColorIO/Config.cpp:59 @ v2.5.2).
pub const OCIO_VIEW_USE_DISPLAY_NAME: &str = "<USE_DISPLAY_NAME>";

/// A view: of a display, or one of the config's shared views.
///
/// Port of `View` (src/OpenColorIO/Display.h:22-57 @ v2.5.2).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct View {
    /// `m_name`.
    pub(crate) name: Vec<u8>,
    /// `m_viewTransform`: might be empty.
    pub(crate) view_transform: Vec<u8>,
    /// `m_colorspace`.
    pub(crate) colorspace: Vec<u8>,
    /// `m_looks`: might be empty.
    pub(crate) looks: Vec<u8>,
    /// `m_rule`: might be empty.
    pub(crate) rule: Vec<u8>,
    /// `m_description`: might be empty.
    pub(crate) description: Vec<u8>,
}

impl View {
    /// A view of these strings, each up to its first NUL (upstream's null pointers are empty
    /// strings).
    ///
    /// Port of `View::View(const char *, ...)` (src/OpenColorIO/Display.h:33-46 @ v2.5.2).
    pub(crate) fn new(
        name: &[u8],
        view_transform: &[u8],
        colorspace: &[u8],
        looks: &[u8],
        rule: &[u8],
        description: &[u8],
    ) -> View {
        View {
            name: c_str(name).to_vec(),
            view_transform: c_str(view_transform).to_vec(),
            colorspace: c_str(colorspace).to_vec(),
            looks: c_str(looks).to_vec(),
            rule: c_str(rule).to_vec(),
            description: c_str(description).to_vec(),
        }
    }

    /// Whether `csname` is `<USE_DISPLAY_NAME>`, ignoring case.
    ///
    /// Port of `View::UseDisplayName` (src/OpenColorIO/Display.h:48-52 @ v2.5.2).
    pub(crate) fn use_display_name(csname: &[u8]) -> bool {
        strcasecmp(csname, OCIO_VIEW_USE_DISPLAY_NAME).is_eq()
    }

    /// Port of `View::useDisplayNameForColorspace` (src/OpenColorIO/Display.h:53-56 @ v2.5.2).
    pub(crate) fn use_display_name_for_colorspace(&self) -> bool {
        View::use_display_name(&self.colorspace)
    }
}

/// `ViewVec`.
pub(crate) type ViewVec = Vec<View>;

/// The index of the view named `name` (ignoring case) in `vec`.
///
/// Port of `FindView` (src/OpenColorIO/Display.cpp:33-49 @ v2.5.2).
pub(crate) fn find_view(vec: &[View], name: &[u8]) -> Option<usize> {
    vec.iter()
        .position(|view| strcasecmp(name, &view.name).is_eq())
}

/// Adds a view to `views`, or replaces everything but the name of the view of that name
/// (ignoring case). A color space spelled `<USE_DISPLAY_NAME>` in any case is stored in upper
/// case.
///
/// Port of `AddView` (src/OpenColorIO/Display.cpp:51-71 @ v2.5.2).
pub(crate) fn add_view(
    views: &mut ViewVec,
    name: &[u8],
    view_transform: &[u8],
    display_color_space: &[u8],
    looks: &[u8],
    rule: &[u8],
    description: &[u8],
) {
    let mut display_color_space = c_str(display_color_space);
    if View::use_display_name(display_color_space) {
        display_color_space = OCIO_VIEW_USE_DISPLAY_NAME.as_bytes();
    }
    match find_view(views, c_str(name)) {
        None => views.push(View::new(
            name,
            view_transform,
            display_color_space,
            looks,
            rule,
            description,
        )),
        Some(i) => {
            let view = &mut views[i];
            view.view_transform = c_str(view_transform).to_vec();
            view.colorspace = display_color_space.to_vec();
            view.looks = c_str(looks).to_vec();
            view.rule = c_str(rule).to_vec();
            view.description = c_str(description).to_vec();
        }
    }
}

/// A display of a config.
///
/// Port of `Display` (src/OpenColorIO/Display.h:67-77 @ v2.5.2).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Display {
    /// `m_temporary`: used to not save displays that originate by instantiating a virtual
    /// display.
    pub(crate) temporary: bool,
    /// `m_views`: the views defined by the display.
    pub(crate) views: ViewVec,
    /// `m_sharedViews`: references to shared views defined by the config.
    pub(crate) shared_views: StringVec,
}

/// The displays in config order, each with its name.
///
/// Port of `DisplayMap` (src/OpenColorIO/Display.h:79-83 @ v2.5.2).
pub(crate) type DisplayMap = Vec<(Vec<u8>, Display)>;

/// The index of the display named `name` (ignoring case).
///
/// Port of `FindDisplay` (src/OpenColorIO/Display.cpp:15-31 @ v2.5.2).
pub(crate) fn find_display(displays: &DisplayMap, name: &[u8]) -> Option<usize> {
    displays
        .iter()
        .position(|display| strcasecmp(name, &display.0).is_eq())
}

/// The active displays: the displays of the environment's list that exist (in its order), else
/// those of the config's active list, else all the displays.
///
/// Port of `ComputeDisplays` (src/OpenColorIO/Display.cpp:73-101 @ v2.5.2).
pub(crate) fn compute_displays(
    display_cache: &mut StringVec,
    displays: &DisplayMap,
    active_displays: &[Vec<u8>],
    active_displays_env_override: &[Vec<u8>],
) {
    display_cache.clear();

    let display_master_list: StringVec = displays.iter().map(|d| d.0.clone()).collect();

    // Apply the env override if it's not empty.
    if !active_displays_env_override.is_empty() {
        *display_cache =
            intersect_string_vecs_case_ignore(active_displays_env_override, &display_master_list);
        if !display_cache.is_empty() {
            return;
        }
    }
    // Otherwise, apply the active displays if it's not empty.
    else if !active_displays.is_empty() {
        *display_cache = intersect_string_vecs_case_ignore(active_displays, &display_master_list);
        if !display_cache.is_empty() {
            return;
        }
    }

    *display_cache = display_master_list;
}

#[cfg(test)]
#[path = "display_tests.rs"]
mod tests;
