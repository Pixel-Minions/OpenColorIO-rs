// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! The categories and encodings of color spaces and named transforms: a port of
//! `src/OpenColorIO/apphelpers/CategoryHelpers.h` and `CategoryHelpers.cpp` @ v2.5.2. These are
//! upstream's internals, which the menu helper (`ColorSpaceMenuHelper`) and
//! `ColorSpaceHelpers::AddColorSpace` use to find the items of a menu; none of them is public.
//!
//! Upstream keeps the color spaces and named transforms it finds as pointers into the config,
//! and tells two apart by their addresses; the port keeps references into the config and
//! compares them the same way ([`std::ptr::eq`]).

// The menu helper (5.7b) uses them.
#![allow(dead_code)]

use ocio_ops::logging::{get_logging_level, log_message};
use ocio_ops::open_color_types::{ColorSpaceVisibility, LoggingLevel, SearchReferenceSpaceType};
use ocio_ops::utils::string_utils::{self, StringVec, c_str};

use crate::apphelpers::color_space_helpers::ColorSpaceInfo;
use crate::color_space::ColorSpace;
use crate::config::Config;
use crate::named_transform::NamedTransform;

/// Port of `Categories` (CategoryHelpers.h:27 @ v2.5.2).
pub(crate) type Categories = StringVec;
/// Port of `Encodings` (CategoryHelpers.h:28 @ v2.5.2).
pub(crate) type Encodings = StringVec;
/// Port of `ColorSpaceNames` (CategoryHelpers.h:30 @ v2.5.2).
pub(crate) type ColorSpaceNames = StringVec;
/// Port of `Infos` (CategoryHelpers.h:34 @ v2.5.2): upstream's are shared pointers to
/// constant infos, which the port holds as values.
pub(crate) type Infos = Vec<ColorSpaceInfo>;

/// Port of `ColorSpaceVec` (CategoryHelpers.cpp:22 @ v2.5.2).
pub(crate) type ColorSpaceVec<'a> = Vec<&'a ColorSpace>;
/// Port of `NamedTransformVec` (CategoryHelpers.cpp:138 @ v2.5.2).
pub(crate) type NamedTransformVec<'a> = Vec<&'a NamedTransform>;

/// Appends `elt` unless the very same object (by address) is already there.
///
/// Port of `AddElement` (CategoryHelpers.cpp:24-36 @ v2.5.2).
fn add_element<'a, T>(vec: &mut Vec<&'a T>, elt: &'a T) {
    for entry in vec.iter() {
        if std::ptr::eq(*entry, elt) {
            // Already there.
            return;
        }
    }
    vec.push(elt);
}

/// Port of `HasCategory` (CategoryHelpers.cpp:38-42 @ v2.5.2), for a color space.
fn cs_has_category(elt: &ColorSpace, category: &[u8]) -> bool {
    elt.has_category(c_str(category))
}

/// Port of `HasCategory` (CategoryHelpers.cpp:38-42 @ v2.5.2), for a named transform.
fn nt_has_category(elt: &NamedTransform, category: &[u8]) -> bool {
    elt.has_category(c_str(category))
}

/// Whether `encoding` is the element's encoding, ignoring case.
///
/// Port of `HasEncoding` (CategoryHelpers.cpp:44-48 @ v2.5.2).
fn has_encoding(encoding: &[u8], elt_encoding: &[u8]) -> bool {
    string_utils::compare(encoding, elt_encoding)
}

/// The active color space at `idx` among those of `color_space_type`, looked up by its name,
/// as upstream does: `getColorSpace(getColorSpaceNameByIndex(...))`.
fn active_color_space(
    config: &Config,
    color_space_type: SearchReferenceSpaceType,
    idx: i32,
) -> &ColorSpace {
    config
        .color_space(config.color_space_name_by_index_with(
            color_space_type,
            ColorSpaceVisibility::Active,
            idx,
        ))
        .expect("the name of an active color space names a color space")
}

/// The active named transform at `idx`, looked up by its name, as upstream does:
/// `getNamedTransform(getNamedTransformNameByIndex(idx))`.
fn active_named_transform(config: &Config, idx: i32) -> &NamedTransform {
    config
        .named_transform(config.named_transform_name_by_index(idx))
        .expect("the name of an active named transform names a named transform")
}

/// The active color spaces of `color_space_type` that have one of `categories` (or no category
/// at all, with `treat_no_category_as_any`) and one of `encodings`, in config order; none if
/// `include_color_spaces` is false or either list is empty.
///
/// Port of `GetColorSpaces(config, includeColorSpaces, treatNoCategoryAsAny, colorSpaceType,
/// categories, encodings)` (CategoryHelpers.cpp:50-81 @ v2.5.2).
pub(crate) fn get_color_spaces_with_encodings<'a>(
    config: &'a Config,
    include_color_spaces: bool,
    treat_no_category_as_any: bool,
    color_space_type: SearchReferenceSpaceType,
    categories: &Categories,
    encodings: &Encodings,
) -> ColorSpaceVec<'a> {
    let mut css = ColorSpaceVec::new();
    if include_color_spaces && !categories.is_empty() && !encodings.is_empty() {
        let num_cs = config.num_color_spaces_with(color_space_type, ColorSpaceVisibility::Active);
        for idx in 0..num_cs {
            let cs = active_color_space(config, color_space_type, idx);

            let ignore_category = treat_no_category_as_any && cs.num_categories() == 0;
            for cat in categories {
                for enc in encodings {
                    if (ignore_category || cs_has_category(cs, cat))
                        && has_encoding(enc, cs.encoding())
                    {
                        add_element(&mut css, cs);
                    }
                }
            }
        }
    }
    css
}

/// The active color spaces of `color_space_type` that have one of `categories` (or no category
/// at all, with `treat_no_category_as_any`), in config order; none if `include_color_spaces`
/// is false or `categories` is empty.
///
/// Port of `GetColorSpaces(config, includeColorSpaces, treatNoCategoryAsAny, colorSpaceType,
/// categories)` (CategoryHelpers.cpp:83-110 @ v2.5.2).
pub(crate) fn get_color_spaces<'a>(
    config: &'a Config,
    include_color_spaces: bool,
    treat_no_category_as_any: bool,
    color_space_type: SearchReferenceSpaceType,
    categories: &Categories,
) -> ColorSpaceVec<'a> {
    let mut css = ColorSpaceVec::new();
    if include_color_spaces && !categories.is_empty() {
        let num_cs = config.num_color_spaces_with(color_space_type, ColorSpaceVisibility::Active);
        for idx in 0..num_cs {
            let cs = active_color_space(config, color_space_type, idx);

            let ignore_category = treat_no_category_as_any && cs.num_categories() == 0;
            for cat in categories {
                if ignore_category || cs_has_category(cs, cat) {
                    add_element(&mut css, cs);
                }
            }
        }
    }
    css
}

/// The active color spaces of `color_space_type` that have one of `encodings`, in config
/// order; none if `include_color_spaces` is false or `encodings` is empty.
///
/// Port of `GetColorSpacesFromEncodings` (CategoryHelpers.cpp:112-136 @ v2.5.2).
pub(crate) fn get_color_spaces_from_encodings<'a>(
    config: &'a Config,
    include_color_spaces: bool,
    color_space_type: SearchReferenceSpaceType,
    encodings: &Encodings,
) -> ColorSpaceVec<'a> {
    let mut css = ColorSpaceVec::new();
    if include_color_spaces && !encodings.is_empty() {
        let num_cs = config.num_color_spaces_with(color_space_type, ColorSpaceVisibility::Active);
        for idx in 0..num_cs {
            let cs = active_color_space(config, color_space_type, idx);
            for enc in encodings {
                if has_encoding(enc, cs.encoding()) {
                    add_element(&mut css, cs);
                }
            }
        }
    }
    css
}

/// The active named transforms that have one of `categories` (or no category at all, with
/// `treat_no_category_as_any`) and one of `encodings`, in config order; none if
/// `include_named_transforms` is false or either list is empty.
///
/// Port of `GetNamedTransforms(config, includeNamedTransforms, treatNoCategoryAsAny,
/// categories, encodings)` (CategoryHelpers.cpp:140-167 @ v2.5.2).
pub(crate) fn get_named_transforms_with_encodings<'a>(
    config: &'a Config,
    include_named_transforms: bool,
    treat_no_category_as_any: bool,
    categories: &Categories,
    encodings: &Encodings,
) -> NamedTransformVec<'a> {
    let mut nts = NamedTransformVec::new();
    if include_named_transforms && !categories.is_empty() && !encodings.is_empty() {
        for idx in 0..config.num_named_transforms() {
            let nt = active_named_transform(config, idx);

            let ignore_category = treat_no_category_as_any && nt.num_categories() == 0;
            for cat in categories {
                for enc in encodings {
                    if (ignore_category || nt_has_category(nt, cat))
                        && has_encoding(enc, nt.encoding())
                    {
                        add_element(&mut nts, nt);
                    }
                }
            }
        }
    }
    nts
}

/// The active named transforms that have one of `categories` (or no category at all, with
/// `treat_no_category_as_any`), in config order; none if `include_named_transforms` is false
/// or `categories` is empty.
///
/// Port of `GetNamedTransforms(config, includeNamedTransforms, treatNoCategoryAsAny,
/// categories)` (CategoryHelpers.cpp:169-192 @ v2.5.2).
pub(crate) fn get_named_transforms<'a>(
    config: &'a Config,
    include_named_transforms: bool,
    treat_no_category_as_any: bool,
    categories: &Categories,
) -> NamedTransformVec<'a> {
    let mut nts = NamedTransformVec::new();
    if include_named_transforms && !categories.is_empty() {
        for idx in 0..config.num_named_transforms() {
            let nt = active_named_transform(config, idx);

            let ignore_category = treat_no_category_as_any && nt.num_categories() == 0;
            for cat in categories {
                if ignore_category || nt_has_category(nt, cat) {
                    add_element(&mut nts, nt);
                }
            }
        }
    }
    nts
}

/// The active named transforms that have one of `encodings`, in config order; none if
/// `include_named_transforms` is false or `encodings` is empty.
///
/// Port of `GetNamedTransformsFromEncodings` (CategoryHelpers.cpp:194-214 @ v2.5.2).
pub(crate) fn get_named_transforms_from_encodings<'a>(
    config: &'a Config,
    include_named_transforms: bool,
    encodings: &Encodings,
) -> NamedTransformVec<'a> {
    let mut nts = NamedTransformVec::new();
    if include_named_transforms && !encodings.is_empty() {
        for idx in 0..config.num_named_transforms() {
            let nt = active_named_transform(config, idx);
            for enc in encodings {
                if has_encoding(enc, nt.encoding()) {
                    add_element(&mut nts, nt);
                }
            }
        }
    }
    nts
}

/// The infos of the color spaces, then of the named transforms.
///
/// Port of `GetInfos` (CategoryHelpers.cpp:216-230 @ v2.5.2).
fn get_infos(config: &Config, css: &ColorSpaceVec<'_>, nts: &NamedTransformVec<'_>) -> Infos {
    let mut all_infos = Infos::new();
    for cs in css {
        all_infos.push(ColorSpaceInfo::from_color_space(config, cs));
    }
    for nt in nts {
        all_infos.push(ColorSpaceInfo::from_named_transform(config, nt));
    }
    all_infos
}

/// The names of the color spaces.
///
/// Port of `GetNames` (CategoryHelpers.cpp:232-243 @ v2.5.2), for a `ColorSpaceVec`.
fn get_names(list: &ColorSpaceVec<'_>) -> ColorSpaceNames {
    list.iter()
        .map(|item| c_str(item.name()).to_vec())
        .collect()
}

/// The entries of `list0` (in its order) that are also in `list1`, by address.
///
/// Port of `Intersection` (CategoryHelpers.cpp:245-259 @ v2.5.2).
fn intersection<'a, T>(list0: &[&'a T], list1: &[&'a T]) -> Vec<&'a T> {
    let mut result = Vec::new();
    for &i0 in list0 {
        if list1.iter().any(|&i1| std::ptr::eq(i1, i0)) {
            result.push(i0);
        }
    }
    result
}

/// The tokens of a comma-separated list, in lower case (ASCII only) and trimmed, without the
/// empty ones; none for `None` (a null pointer). The list ends at its first NUL.
///
/// Port of `ExtractItems` (CategoryHelpers.cpp:263-279 @ v2.5.2).
pub(crate) fn extract_items(strings: Option<&[u8]>) -> StringVec {
    let Some(strings) = strings else {
        return StringVec::new();
    };
    let tmp = string_utils::split(&string_utils::lower_c_str(Some(strings)), b',');
    let mut all = StringVec::new();

    for val in &tmp {
        let v = string_utils::trim(val);
        if !v.is_empty() {
            all.push(v.to_vec());
        }
    }

    all
}

/// The names of the active color spaces (of every reference space) that have one of
/// `categories`.
///
/// Port of `FindColorSpaceNames` (CategoryHelpers.cpp:281-285 @ v2.5.2).
pub(crate) fn find_color_space_names(config: &Config, categories: &Categories) -> ColorSpaceNames {
    let all_cs = get_color_spaces(
        config,
        true,
        false,
        SearchReferenceSpaceType::All,
        categories,
    );
    get_names(&all_cs)
}

/// How `FindColorSpaceInfos` used a list of categories, to say so in its log.
///
/// Port of `CategoryUsage` (CategoryHelpers.cpp:290-296 @ v2.5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CategoryUsage {
    NotUsed,
    ShouldBeUsed,
    Ignored,
    NoneFound,
}

/// What `FindColorSpaceInfos` had to give up, logged as information when it returns.
///
/// Port of `LogMessageHelper` (CategoryHelpers.cpp:297-346 @ v2.5.2): upstream logs from its
/// destructor, as the port does from `Drop`, after the function's result is made.
#[derive(Debug)]
struct LogMessageHelper {
    ignore_encodings: bool,
    ignore_categories: bool,
    empty_intersection: bool,
    app_cats: CategoryUsage,
    user_cats: CategoryUsage,
}

impl LogMessageHelper {
    fn new() -> LogMessageHelper {
        LogMessageHelper {
            ignore_encodings: false,
            ignore_categories: false,
            empty_intersection: false,
            app_cats: CategoryUsage::NotUsed,
            user_cats: CategoryUsage::NotUsed,
        }
    }
}

impl Drop for LogMessageHelper {
    /// Port of `LogMessageHelper::~LogMessageHelper` (CategoryHelpers.cpp:304-345 @ v2.5.2).
    fn drop(&mut self) {
        if (get_logging_level() as i32) >= (LoggingLevel::Info as i32)
            && (self.empty_intersection
                || self.ignore_encodings
                || self.ignore_categories
                || self.app_cats == CategoryUsage::NoneFound
                || self.user_cats == CategoryUsage::NoneFound
                || self.user_cats == CategoryUsage::Ignored)
        {
            let mut os = b"All parameters could not be used to create the menu:".to_vec();
            if self.empty_intersection {
                os.extend_from_slice(
                    b" Intersection of color spaces with app categories and color spaces with \
                      user categories is empty.",
                );
            }
            if self.app_cats == CategoryUsage::NoneFound {
                os.extend_from_slice(b" Found no color space using app categories.");
                if self.user_cats == CategoryUsage::Ignored
                    || self.user_cats == CategoryUsage::NoneFound
                {
                    self.ignore_categories = true;
                }
            }
            if self.user_cats == CategoryUsage::NoneFound {
                os.extend_from_slice(b" Found no color space using user categories.");
            } else if self.user_cats == CategoryUsage::Ignored {
                os.extend_from_slice(b" User categories have been ignored.");
            }
            if self.ignore_encodings {
                os.extend_from_slice(
                    b" Encodings have been ignored since they matched no color spaces.",
                );
            }
            if self.ignore_categories {
                os.extend_from_slice(
                    b" Categories have been ignored since they matched no color spaces.",
                );
            }

            // The information level is a supported one: LogMessage can't fail.
            let _ = log_message(LoggingLevel::Info, &os);
        }
    }
}

/// The items of a menu (step 2 and 3 of `ColorSpaceMenuParameters`' algorithm): the active
/// color spaces of `color_space_type` and the active named transforms (as the include flags
/// ask) that have one of the app categories and one of the encodings, intersected with those
/// that have one of the user categories; with the fall-backs that keep the result from being
/// empty, logged as information. A version 1 config has no categories: all its items.
///
/// The last fall-back (3g), all the items, lists the color spaces even when
/// `include_color_spaces` is false (docs/improvements.md, I-316).
///
/// Port of `FindColorSpaceInfos` (CategoryHelpers.cpp:349-555 @ v2.5.2).
pub(crate) fn find_color_space_infos(
    config: &Config,
    app_categories: &Categories,
    user_categories: &Categories,
    include_color_spaces: bool,
    include_named_transforms: bool,
    treat_no_category_as_any: bool,
    encodings: &Encodings,
    color_space_type: SearchReferenceSpaceType,
) -> Infos {
    // At least one of include flags is true.

    let mut log = LogMessageHelper::new();

    // V1 does not have categories and encodings, skip them.
    if config.major_version() >= 2 {
        let mut app_cs = ColorSpaceVec::new();
        let mut app_nt = NamedTransformVec::new();
        let mut app_cs_no_encodings = ColorSpaceVec::new();
        let mut app_nt_no_encodings = NamedTransformVec::new();
        let mut app_no_encodings_computed = false;

        let mut app_size: usize = 0;

        let mut encs_ignored = encodings.is_empty();

        if !app_categories.is_empty() {
            // 3a) Use categories and encodings, fallback to only categories, fallback to only
            //     encodings.

            log.app_cats = CategoryUsage::ShouldBeUsed;

            // Use categories and encodings.

            if !encs_ignored {
                app_cs = get_color_spaces_with_encodings(
                    config,
                    include_color_spaces,
                    treat_no_category_as_any,
                    color_space_type,
                    app_categories,
                    encodings,
                );
                app_nt = get_named_transforms_with_encodings(
                    config,
                    include_named_transforms,
                    treat_no_category_as_any,
                    app_categories,
                    encodings,
                );
                app_size = app_cs.len() + app_nt.len();
            }

            // Do not use encodings if empty or drop them if no result is found with them.
            if app_size == 0 {
                encs_ignored = true;
                log.ignore_encodings = !encodings.is_empty();
                app_cs = get_color_spaces(
                    config,
                    include_color_spaces,
                    treat_no_category_as_any,
                    color_space_type,
                    app_categories,
                );

                app_nt = get_named_transforms(
                    config,
                    include_named_transforms,
                    treat_no_category_as_any,
                    app_categories,
                );
                app_size = app_cs.len() + app_nt.len();

                // Keep these results in case we need them later.
                app_no_encodings_computed = true;
                app_cs_no_encodings = app_cs.clone();
                app_nt_no_encodings = app_nt.clone();
            }

            // Drop app categories and use encoding if no results.
            if app_size == 0 && !encodings.is_empty() {
                encs_ignored = false;
                log.ignore_encodings = false;
                log.app_cats = CategoryUsage::NoneFound;
                app_cs = get_color_spaces_from_encodings(
                    config,
                    include_color_spaces,
                    color_space_type,
                    encodings,
                );
                app_nt = get_named_transforms_from_encodings(
                    config,
                    include_named_transforms,
                    encodings,
                );
                app_size = app_cs.len() + app_nt.len();
            }

            if app_size == 0 {
                log.app_cats = CategoryUsage::NoneFound;
            }
        } else if !encs_ignored {
            app_cs = get_color_spaces_from_encodings(
                config,
                include_color_spaces,
                color_space_type,
                encodings,
            );
            app_nt =
                get_named_transforms_from_encodings(config, include_named_transforms, encodings);
            app_size = app_cs.len() + app_nt.len();
        }

        let mut user_cs = ColorSpaceVec::new();
        let mut user_nt = NamedTransformVec::new();
        let mut user_size: usize = 0;

        if !user_categories.is_empty() {
            // 3b) Items using user categories.

            user_cs = get_color_spaces(
                config,
                include_color_spaces,
                treat_no_category_as_any,
                color_space_type,
                user_categories,
            );
            user_nt = get_named_transforms(
                config,
                include_named_transforms,
                treat_no_category_as_any,
                user_categories,
            );
            user_size = user_cs.len() + user_nt.len();
            if user_size == 0 {
                log.user_cats = CategoryUsage::NoneFound;
            }
        }

        if app_size != 0 && user_size != 0 {
            // 3c) and 3d) Use intersection of app and user categories.

            let mut app_cs_no_encodings_test = false;
            let encs_ignored_back = encs_ignored;
            let ignore_encodings_back = log.ignore_encodings;

            // Allow to run twice, with and without encodings.
            loop {
                let (app_cs_test, app_nt_test) = if app_cs_no_encodings_test {
                    (&app_cs_no_encodings, &app_nt_no_encodings)
                } else {
                    (&app_cs, &app_nt)
                };
                let css = intersection(app_cs_test, &user_cs);
                let nts = intersection(app_nt_test, &user_nt);

                if !css.is_empty() || !nts.is_empty() {
                    // 3c) or 3d) Intersection is not empty.
                    return get_infos(config, &css, &nts);
                }

                if !encs_ignored && !encodings.is_empty() {
                    // Intersection is empty, but encodings can be dropped if they were not
                    // dropped already.
                    encs_ignored = true;
                    log.ignore_encodings = true;
                    if !app_no_encodings_computed {
                        // If not already computed, compute list with app categories and no
                        // encodings.
                        app_cs_no_encodings = get_color_spaces(
                            config,
                            include_color_spaces,
                            treat_no_category_as_any,
                            color_space_type,
                            app_categories,
                        );
                        app_nt_no_encodings = get_named_transforms(
                            config,
                            include_named_transforms,
                            treat_no_category_as_any,
                            app_categories,
                        );
                    }
                    app_cs_no_encodings_test = true;
                } else {
                    break;
                }
            }
            log.empty_intersection = true;
            encs_ignored = encs_ignored_back;
            log.ignore_encodings = ignore_encodings_back;
        }
        // Upstream restores `encsIgnored` and reads it no more.
        let _ = encs_ignored;

        if app_size != 0 {
            // 3e) Only use app categories. Use the result of 3a).
            if !user_categories.is_empty() && log.user_cats != CategoryUsage::NoneFound {
                log.user_cats = CategoryUsage::Ignored;
            }
            return get_infos(config, &app_cs, &app_nt);
        }

        if user_size != 0 {
            // 3f) Only use user categories.
            return get_infos(config, &user_cs, &user_nt);
        }

        // Fallback to ignoring categories and encodings.
        log.ignore_categories = !app_categories.is_empty() || !user_categories.is_empty();
    }

    // 3g) Ignore all categories and encodings and return all items.

    let mut all_infos = Infos::new();
    let num_cs = config.num_color_spaces_with(color_space_type, ColorSpaceVisibility::Active);
    for idx in 0..num_cs {
        let cs = active_color_space(config, color_space_type, idx);
        all_infos.push(ColorSpaceInfo::from_color_space(config, cs));
    }

    if include_named_transforms {
        for idx in 0..config.num_named_transforms() {
            let nt = active_named_transform(config, idx);
            all_infos.push(ColorSpaceInfo::from_named_transform(config, nt));
        }
    }

    // Nothing is found, no need to log anything.
    if all_infos.is_empty() {
        log.app_cats = CategoryUsage::NotUsed;
        log.user_cats = CategoryUsage::NotUsed;
        log.empty_intersection = false;
        log.ignore_categories = false;
        log.ignore_encodings = false;
    }
    all_infos
}

#[cfg(test)]
#[path = "category_helpers_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "category_helpers_oracle_tests.rs"]
mod oracle_tests;
