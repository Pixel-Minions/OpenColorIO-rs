// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/apphelpers/CategoryHelpers_tests.cpp` @ v2.5.2.

use super::*;
use crate::apphelpers::configs_data::CATEGORY_TEST_CONFIG;

/// The names of `items`, as `&str` for the comparisons.
fn names<T: Named>(items: &[&T]) -> Vec<String> {
    items
        .iter()
        .map(|i| String::from_utf8(i.item_name().to_vec()).unwrap())
        .collect()
}

trait Named {
    fn item_name(&self) -> &[u8];
}

impl Named for ColorSpace {
    fn item_name(&self) -> &[u8] {
        self.name()
    }
}

impl Named for NamedTransform {
    fn item_name(&self) -> &[u8] {
        self.name()
    }
}

/// `StringVec` of these strings.
fn vec_of(items: &[&str]) -> StringVec {
    items.iter().map(|s| s.as_bytes().to_vec()).collect()
}

/// Port of `OCIO_ADD_TEST(CategoryHelpers, categories)` @ v2.5.2.
#[test]
fn categories() {
    {
        let categories = b"iNpuT";

        let cats = extract_items(Some(categories));
        assert_eq!(cats.len(), 1);
        assert_eq!(cats[0], b"input");
    }

    {
        let categories = b"    iNpuT     ";

        let cats = extract_items(Some(categories));
        assert_eq!(cats.len(), 1);
        assert_eq!(cats[0], b"input");
    }

    {
        let categories = b",,iNpuT,    ,,";

        let cats = extract_items(Some(categories));
        assert_eq!(cats.len(), 1);
        assert_eq!(cats[0], b"input");
    }

    {
        let categories = b",,iNpuT,    ,,lut_input_SPACE";

        let cats = extract_items(Some(categories));
        assert_eq!(cats.len(), 2);
        assert_eq!(cats[0], b"input");
        assert_eq!(cats[1], b"lut_input_space");
    }
}

/// Port of `OCIO_ADD_TEST(CategoryHelpers, basic)` @ v2.5.2.
#[test]
fn basic() {
    // This is testing internals that are using vectors of pointers.
    // These do not include the various fallbacks that are included at theColorSpaceHelpers
    // level.

    let config = Config::create_from_stream(CATEGORY_TEST_CONFIG.as_bytes()).unwrap();
    config.validate().unwrap();

    {
        let categories = vec_of(&["file-io", "working-space"]);
        let encodings = vec_of(&["sdr-video", "log"]);
        let mut css = get_color_spaces_with_encodings(
            &config,
            true, // includeColorSpaces
            true, // treatNoCategoryAsAny
            SearchReferenceSpaceType::Scene,
            &categories,
            &encodings,
        );
        assert_eq!(css.len(), 4);
        assert_eq!(names(&css), ["log_1", "in_1", "in_2", "view_1"]);

        css = get_color_spaces_with_encodings(
            &config,
            true,
            false,
            SearchReferenceSpaceType::Scene,
            &categories,
            &encodings,
        );
        assert_eq!(css.len(), 3);
        assert_eq!(names(&css), ["log_1", "in_1", "in_2"]);

        css = get_color_spaces_with_encodings(
            &config,
            false,
            true,
            SearchReferenceSpaceType::Scene,
            &categories,
            &encodings,
        );
        assert_eq!(css.len(), 0);
    }
    {
        let categories = vec_of(&[]);
        let encodings = vec_of(&["sdr-video", "log"]);
        let mut css = get_color_spaces_with_encodings(
            &config,
            true,
            false,
            SearchReferenceSpaceType::Scene,
            &categories,
            &encodings,
        );
        assert_eq!(css.len(), 0);
        css = get_color_spaces_from_encodings(
            &config,
            true, // includeColorSpaces
            SearchReferenceSpaceType::Scene,
            &encodings,
        );
        assert_eq!(css.len(), 4);
    }
    {
        let categories = vec_of(&["file-io", "working-space"]);
        let encodings = vec_of(&[]);
        let mut css = get_color_spaces_with_encodings(
            &config,
            true,
            false,
            SearchReferenceSpaceType::Scene,
            &categories,
            &encodings,
        );
        assert_eq!(css.len(), 0);

        css = get_color_spaces(
            &config,
            true,
            false,
            SearchReferenceSpaceType::Scene,
            &categories,
        );
        assert_eq!(css.len(), 7);
        css = get_color_spaces(
            &config,
            true,
            true,
            SearchReferenceSpaceType::Scene,
            &categories,
        );
        assert_eq!(css.len(), 9);
    }
    {
        let categories = vec_of(&["file-io", "working-space"]);
        let encodings = vec_of(&["sdr-video", "log"]);
        let css = get_color_spaces_with_encodings(
            &config,
            true,
            true,
            SearchReferenceSpaceType::Display,
            &categories,
            &encodings,
        );
        assert_eq!(css.len(), 2);
        assert_eq!(names(&css), ["display_lin_2", "display_log_1"]);
    }
    {
        let categories = vec_of(&["file-io", "working-space"]);
        let encodings = vec_of(&["sdr-video", "log"]);
        let css = get_color_spaces_with_encodings(
            &config,
            true,
            false,
            SearchReferenceSpaceType::All,
            &categories,
            &encodings,
        );
        assert_eq!(css.len(), 5);
        assert_eq!(
            names(&css),
            ["log_1", "in_1", "in_2", "display_lin_2", "display_log_1"]
        );
    }
    {
        let categories = vec_of(&["file-io", "working-space"]);
        let mut css = get_color_spaces(
            &config,
            true,  // includeColorSpaces
            false, // treatNoCategoryAsAny
            SearchReferenceSpaceType::All,
            &categories,
        );
        assert_eq!(css.len(), 10);
        assert_eq!(
            names(&css),
            [
                "lin_1",
                "lin_2",
                "log_1",
                "in_1",
                "in_2",
                "in_3",
                "lut_input_3",
                "display_lin_1",
                "display_lin_2",
                "display_log_1"
            ]
        );

        css = get_color_spaces(
            &config,
            true,
            true,
            SearchReferenceSpaceType::All,
            &categories,
        );
        assert_eq!(css.len(), 12);

        css = get_color_spaces(
            &config,
            false,
            true,
            SearchReferenceSpaceType::All,
            &categories,
        );
        assert_eq!(css.len(), 0);
    }
    {
        let encodings = vec_of(&["sdr-video", "log"]);
        let mut css = get_color_spaces_from_encodings(
            &config,
            true, // includeColorSpaces
            SearchReferenceSpaceType::All,
            &encodings,
        );
        assert_eq!(css.len(), 6);
        assert_eq!(
            names(&css),
            [
                "log_1",
                "in_1",
                "in_2",
                "view_1",
                "display_lin_2",
                "display_log_1"
            ]
        );

        css = get_color_spaces_from_encodings(
            &config,
            false,
            SearchReferenceSpaceType::All,
            &encodings,
        );
        assert_eq!(css.len(), 0);
    }

    // Named Transforms

    {
        let categories = vec_of(&["file-io", "working-space"]);
        let encodings = vec_of(&["sdr-video", "log"]);
        let mut nts = get_named_transforms_with_encodings(
            &config,
            true, // includeNamedTransforms
            true, // treatNoCategoryAsAny
            &categories,
            &encodings,
        );
        assert_eq!(nts.len(), 3);
        assert_eq!(names(&nts), ["nt1", "nt2", "nt3"]);

        nts = get_named_transforms_with_encodings(&config, true, false, &categories, &encodings);
        assert_eq!(nts.len(), 2);
        assert_eq!(names(&nts), ["nt1", "nt3"]);

        nts = get_named_transforms_with_encodings(&config, false, true, &categories, &encodings);
        assert_eq!(nts.len(), 0);
    }
    {
        let categories = vec_of(&[]);
        let encodings = vec_of(&["sdr-video", "log"]);
        let nts = get_named_transforms_with_encodings(&config, true, true, &categories, &encodings);
        assert_eq!(nts.len(), 0);
    }
    {
        let categories = vec_of(&["file-io", "working-space"]);
        let encodings = vec_of(&[]);
        let nts = get_named_transforms_with_encodings(&config, true, true, &categories, &encodings);
        assert_eq!(nts.len(), 0);
    }
    {
        let categories = vec_of(&["file-io"]);
        let mut nts = get_named_transforms(
            &config,
            true, // includeNamedTransforms
            true, // treatNoCategoryAsAny
            &categories,
        );
        assert_eq!(nts.len(), 2);
        assert_eq!(names(&nts), ["nt2", "nt3"]);

        nts = get_named_transforms(&config, true, false, &categories);
        assert_eq!(nts.len(), 1);
        assert_eq!(names(&nts), ["nt3"]);

        nts = get_named_transforms(&config, false, true, &categories);
        assert_eq!(nts.len(), 0);
    }
    {
        let encodings = vec_of(&["log"]);
        let mut nts = get_named_transforms_from_encodings(
            &config, true, // includeNamedTransforms
            &encodings,
        );
        assert_eq!(nts.len(), 1);
        assert_eq!(names(&nts), ["nt2"]);

        nts = get_named_transforms_from_encodings(&config, false, &encodings);
        assert_eq!(nts.len(), 0);
    }
}
