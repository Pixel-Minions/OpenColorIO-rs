// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/apphelpers/ColorSpaceHelpers_tests.cpp` @ v2.5.2.

use super::*;
use crate::apphelpers::configs_data::CATEGORY_TEST_CONFIG;
use ocio_ops::parse_utils::ROLE_RENDERING;
use ocio_testkit::upstream::check_throw_what;

/// Port of `OCIO_ADD_TEST(ColorSpaceInfo, read_values)` @ v2.5.2.
#[test]
fn color_space_info_read_values() {
    let config = Config::create_from_stream(CATEGORY_TEST_CONFIG.as_bytes()).unwrap();
    config.validate().unwrap();

    // Tests with 'in_1'.

    let mut cs = config.color_space("in_1").unwrap();

    let mut cs_info = ColorSpaceInfo::from_color_space(&config, cs);

    assert_eq!(cs.name(), b"in_1");
    assert_eq!(cs_info.name(), b"in_1");

    assert_eq!(cs.family(), b"Input / Camera/Acme");
    assert_eq!(cs_info.num_hierarchy_levels(), 3);
    assert_eq!(cs_info.hierarchy_level(0), b"Input");
    assert_eq!(cs_info.hierarchy_level(1), b"Camera");
    assert_eq!(cs_info.hierarchy_level(2), b"Acme");
    assert_eq!(cs_info.family(), cs.family());

    assert_eq!(
        cs.description(),
        b"An input color space.\nFor the Acme camera."
    );
    assert_eq!(cs_info.description(), cs.description());

    // Tests with 'lin_1'.

    cs = config.color_space("lin_1").unwrap();
    cs_info = ColorSpaceInfo::from_color_space(&config, cs);

    assert_eq!(cs.name(), b"lin_1");
    assert_eq!(cs_info.name(), b"lin_1");

    assert_eq!(cs.family(), b"");
    assert_eq!(cs_info.num_hierarchy_levels(), 0);
    assert_eq!(cs_info.family(), b"");

    assert_eq!(cs.description(), b"");
}

/// Port of `OCIO_ADD_TEST(ColorSpaceInfo, change_values)` @ v2.5.2.
#[test]
fn color_space_info_change_values() {
    let config = Config::create_raw().unwrap();
    config.validate().unwrap();

    let mut cs = config.color_space("raw").unwrap().clone();

    let mut cs_info = ColorSpaceInfo::from_color_space(&config, &cs);

    assert_eq!(cs.name(), b"raw");
    assert_eq!(cs_info.name(), b"raw");

    assert_eq!(cs.family(), b"raw");
    assert_eq!(cs_info.num_hierarchy_levels(), 1);
    assert_eq!(cs_info.hierarchy_level(0), b"raw");
    assert_eq!(cs_info.family(), cs.family());

    assert_eq!(
        cs.description(),
        b"A raw color space. Conversions to and from this space are no-ops."
    );
    assert_eq!(cs_info.description(), cs.description());

    // Change the family.

    cs.set_family("");
    assert_eq!(cs.family(), b"");

    cs_info = ColorSpaceInfo::from_color_space(&config, &cs);
    assert_eq!(cs_info.num_hierarchy_levels(), 0);
    assert_eq!(cs_info.family(), b"");

    cs.set_family("Acme     /   Camera");
    assert_eq!(cs.family(), b"Acme     /   Camera");

    let mut cfg = (*config).clone();

    // No family separator.

    cfg.set_family_separator(0).unwrap(); // i.e. that's the null character

    cs_info = ColorSpaceInfo::from_color_space(&cfg, &cs);
    assert_eq!(cs_info.num_hierarchy_levels(), 1);
    assert_eq!(cs_info.hierarchy_level(0), cs.family());

    // '/' is the new family separator.

    cfg.set_family_separator(b'/').unwrap();

    cs_info = ColorSpaceInfo::from_color_space(&cfg, &cs);
    assert_eq!(cs_info.num_hierarchy_levels(), 2);
    assert_eq!(cs_info.hierarchy_level(0), b"Acme");
    assert_eq!(cs_info.hierarchy_level(1), b"Camera");
    assert_eq!(cs_info.family(), cs.family());

    // '-' is the new family separator.

    cfg.set_family_separator(b'-').unwrap();

    cs_info = ColorSpaceInfo::from_color_space(&cfg, &cs);
    assert_eq!(cs_info.num_hierarchy_levels(), 1);
    assert_eq!(cs_info.hierarchy_level(0), cs.family());

    // Reset to the v2 default family separator i.e. default to '/'.

    cfg.set_family_separator(Config::default_family_separator())
        .unwrap();

    cs_info = ColorSpaceInfo::from_color_space(&cfg, &cs);
    assert_eq!(cs_info.num_hierarchy_levels(), 2);
    assert_eq!(cs_info.hierarchy_level(0), b"Acme");
    assert_eq!(cs_info.hierarchy_level(1), b"Camera");
    assert_eq!(cs_info.family(), cs.family());

    // Change the description.

    cs.set_description("desc 1\n\n\n desc 2");
    assert_eq!(cs.description(), b"desc 1\n\n\n desc 2");

    cs_info = ColorSpaceInfo::from_color_space(&cfg, &cs);
    assert_eq!(cs_info.description(), cs.description());
}

// ---------------------------------------------------------------------------------------------
// ColorSpaceMenuHelper
//
// Upstream runs its tests one after another in one process; the port's run in parallel threads
// of one process, and share its menus (`ColorSpaceMenuHelper::new` keeps them by the text of
// their parameters, which holds the config's cache ID). So each test that reads
// `category_test_config` gives it a description of its own, which changes nothing but its cache
// ID: no test gets a menu another test made, and every menu logs as upstream's test expects.

/// `category_test_config` with the description `tag`.
fn tagged_config(tag: &str) -> Arc<Config> {
    let text = CATEGORY_TEST_CONFIG.replacen(
        "ocio_profile_version: 2\n",
        &format!("ocio_profile_version: 2\ndescription: {tag}\n"),
        1,
    );
    assert_ne!(text, CATEGORY_TEST_CONFIG);
    Config::create_from_stream(text.as_bytes()).unwrap()
}

/// Runs `f` as under upstream's `LogGuard` (the debug level, and a logging function that keeps
/// the messages): what it returns, and the messages logged on this thread, one after another.
fn log_guard<T>(f: impl FnOnce() -> T) -> (T, Vec<u8>) {
    use ocio_ops::logging::{get_logging_level, set_logging_level};
    use ocio_ops::open_color_types::LoggingLevel;

    let (out, log) = crate::test_env::capture_log(|| {
        struct Level(LoggingLevel);
        impl Drop for Level {
            fn drop(&mut self) {
                set_logging_level(self.0);
            }
        }
        let _level = Level(get_logging_level());
        set_logging_level(LoggingLevel::Debug);
        f()
    });
    (out, log.concat())
}

/// The menu of `params`, which upstream's test expects to be made.
fn menu(params: &ColorSpaceMenuParameters) -> Arc<ColorSpaceMenuHelper> {
    ColorSpaceMenuHelper::new(params).unwrap()
}

/// Port of `OCIO_ADD_TEST(ColorSpaceMenuHelper, no_color_spaces)` @ v2.5.2.
#[test]
fn color_space_menu_helper_no_color_spaces() {
    let is = r#"ocio_profile_version: 2

environment:
  {}

search_path: luts
strictparsing: true
family_separator: /
luma: [0.2126, 0.7152, 0.0722]

roles:
  rendering: test_1
  default: raw

view_transforms:
  - !<ViewTransform>
    name: view_transform
    from_scene_reference: !<MatrixTransform> {}

displays:
  DISP_1:
    - !<View> {name: VIEW_1, colorspace: test_1}
    - !<View> {name: VIEW_2, colorspace: test_2}

active_displays: []
active_views: []

colorspaces:
  - !<ColorSpace>
    name: raw
    family: Raw
    description: A raw color space. Conversions to and from this space are no-ops.
    isdata: true
    allocation: uniform

  - !<ColorSpace>
    name: test_1
    categories: [ working-space, basic-2d ]
    encoding: scene-linear

  - !<ColorSpace>
    name: test_2
    categories: [ working-space ]
    encoding: scene-linear
 "#;

    let config = Config::create_from_stream(is.as_bytes()).unwrap();
    config.validate().unwrap();

    // Use app-oriented categories with exact case.

    let mut params = ColorSpaceMenuParameters::new(config);

    let mut menu_helper = menu(&params);
    assert_eq!(menu_helper.num_color_spaces(), 3);

    params.set_include_color_spaces(false);
    menu_helper = menu(&params);
    assert_eq!(menu_helper.num_color_spaces(), 0);

    params.set_include_named_transforms(true);
    menu_helper = menu(&params);
    assert_eq!(menu_helper.num_color_spaces(), 0);

    params.set_include_color_spaces(true);
    params.set_search_reference_space_type(SearchReferenceSpaceType::Display);
    menu_helper = menu(&params);
    assert_eq!(menu_helper.num_color_spaces(), 0);

    params.set_include_color_spaces(true);
    params.set_app_categories("basic-2d");
    params.set_search_reference_space_type(SearchReferenceSpaceType::Scene);
    params.set_treat_no_category_as_any(true);
    menu_helper = menu(&params);
    assert_eq!(menu_helper.num_color_spaces(), 2);

    params.set_treat_no_category_as_any(false);
    menu_helper = menu(&params);
    assert_eq!(menu_helper.num_color_spaces(), 1);

    params.set_include_color_spaces(false);
    menu_helper = menu(&params);
    assert_eq!(menu_helper.num_color_spaces(), 0);

    params.set_include_color_spaces(true);
    params.set_search_reference_space_type(SearchReferenceSpaceType::Display);
    menu_helper = menu(&params);
    assert_eq!(menu_helper.num_color_spaces(), 0);
}

/// Port of `OCIO_ADD_TEST(ColorSpaceMenuHelper, categories)` @ v2.5.2.
#[test]
fn color_space_menu_helper_categories() {
    let config = tagged_config("categories");
    config.validate().unwrap();

    // Use app-oriented categories with exact case.

    let mut params = ColorSpaceMenuParameters::new(config);
    params.set_app_categories("file-io");

    // Note: The test config has two active color spaces and a NT without categories.
    params.set_treat_no_category_as_any(true);

    let mut menu_helper = menu(&params);

    assert_eq!(menu_helper.num_color_spaces(), 6);

    // Use app-oriented categories with other case.

    params.set_app_categories("FILE-IO");
    menu_helper = menu(&params);

    assert_eq!(menu_helper.num_color_spaces(), 6);

    // Use app-oriented categories, including named transforms.

    params.set_app_categories("file-io");
    params.set_include_named_transforms(true);
    menu_helper = menu(&params);

    assert_eq!(menu_helper.num_color_spaces(), 8);

    assert_eq!(menu_helper.name(0), b"raw");
    assert_eq!(menu_helper.name(1), b"in_1");
    assert_eq!(menu_helper.name(2), b"in_2");
    assert_eq!(menu_helper.name(3), b"in_3");
    assert_eq!(menu_helper.name(4), b"view_1");
    assert_eq!(menu_helper.name(5), b"lut_input_3");
    assert_eq!(menu_helper.name(6), b"nt2");
    assert_eq!(menu_helper.name(7), b"nt3");
    assert_eq!(menu_helper.name(8), b"");

    assert_eq!(menu_helper.ui_name(0), b"raw");
    assert_eq!(menu_helper.ui_name(1), b"in_1");
    assert_eq!(menu_helper.ui_name(2), b"in_2");
    assert_eq!(menu_helper.ui_name(3), b"in_3");
    assert_eq!(menu_helper.ui_name(4), b"view_1");
    assert_eq!(menu_helper.ui_name(5), b"lut_input_3");
    assert_eq!(menu_helper.ui_name(6), b"nt2");
    assert_eq!(menu_helper.ui_name(7), b"nt3");
    assert_eq!(menu_helper.ui_name(8), b"");

    assert_eq!(menu_helper.num_hierarchy_levels(0), 1);
    assert_eq!(menu_helper.num_hierarchy_levels(1), 3);
    assert_eq!(menu_helper.num_hierarchy_levels(2), 0);
    assert_eq!(menu_helper.num_hierarchy_levels(3), 0);
    assert_eq!(menu_helper.num_hierarchy_levels(4), 0);
    assert_eq!(menu_helper.num_hierarchy_levels(5), 0);
    assert_eq!(menu_helper.num_hierarchy_levels(6), 0);
    assert_eq!(menu_helper.num_hierarchy_levels(7), 1);
    assert_eq!(menu_helper.num_hierarchy_levels(8), 0);

    assert_eq!(menu_helper.hierarchy_level(1, 0), b"Input");
    assert_eq!(menu_helper.hierarchy_level(1, 1), b"Camera");
    assert_eq!(menu_helper.hierarchy_level(1, 2), b"Acme");
    assert_eq!(menu_helper.hierarchy_level(7, 0), b"NamedTransforms");

    // Use null categories.

    params.set_include_named_transforms(false);
    params.set_app_categories("");
    // All active color spaces (scene and display).
    menu_helper = menu(&params);
    assert_eq!(menu_helper.num_color_spaces(), 14);

    // Active non-display color spaces only.
    params.set_search_reference_space_type(SearchReferenceSpaceType::Scene);
    menu_helper = menu(&params);
    assert_eq!(menu_helper.num_color_spaces(), 11);

    // Active display color spaces only.
    params.set_search_reference_space_type(SearchReferenceSpaceType::Display);
    menu_helper = menu(&params);
    assert_eq!(menu_helper.num_color_spaces(), 3);

    // Use null categories, including named transforms.

    params.set_search_reference_space_type(SearchReferenceSpaceType::All);
    params.set_include_named_transforms(true);
    menu_helper = menu(&params);

    // All active color spaces and named transforms.
    assert_eq!(menu_helper.num_color_spaces(), 17);

    // Use app-oriented category, include roles.

    params.set_include_named_transforms(false);
    params.set_app_categories("look-process-space");
    params.set_include_roles(true);
    params.set_treat_no_category_as_any(false);
    menu_helper = menu(&params);

    assert_eq!(menu_helper.num_color_spaces(), 7);
    assert_eq!(menu_helper.name(0), b"lut_input_1");
    assert_eq!(menu_helper.name(1), b"lut_input_2");
    assert_eq!(menu_helper.name(2), b"lut_input_3");
    assert_eq!(menu_helper.name(3), b"default");
    assert_eq!(menu_helper.name(4), b"reference");
    assert_eq!(menu_helper.name(5), b"rendering");
    assert_eq!(menu_helper.name(6), b"scene_linear");
    assert_eq!(menu_helper.name(7), b"");

    assert_eq!(menu_helper.index_from_name("lut_input_1"), 0);
    assert_eq!(menu_helper.index_from_name("lut_input_2"), 1);
    assert_eq!(menu_helper.index_from_name("lut_input_3"), 2);
    assert_eq!(menu_helper.index_from_name("default"), 3);
    assert_eq!(menu_helper.index_from_name("reference"), 4);
    assert_eq!(menu_helper.index_from_name("rendering"), 5);
    assert_eq!(menu_helper.index_from_name("scene_linear"), 6);
    assert_eq!(menu_helper.index_from_name("default (lin_1)"), usize::MAX);

    assert_eq!(menu_helper.ui_name(0), b"lut_input_1");
    assert_eq!(menu_helper.ui_name(1), b"lut_input_2");
    assert_eq!(menu_helper.ui_name(2), b"lut_input_3");
    assert_eq!(menu_helper.ui_name(3), b"default (raw)");
    assert_eq!(menu_helper.ui_name(4), b"reference (lin_1)");
    assert_eq!(menu_helper.ui_name(5), b"rendering (lin_1)");
    assert_eq!(menu_helper.ui_name(6), b"scene_linear (lin_1)");
    assert_eq!(menu_helper.ui_name(7), b"");

    assert_eq!(menu_helper.index_from_ui_name("lut_input_1"), 0);
    assert_eq!(menu_helper.index_from_ui_name("lut_input_2"), 1);
    assert_eq!(menu_helper.index_from_ui_name("lut_input_3"), 2);
    assert_eq!(menu_helper.index_from_ui_name("default (raw)"), 3);
    assert_eq!(menu_helper.index_from_ui_name("reference (lin_1)"), 4);
    assert_eq!(menu_helper.index_from_ui_name("rendering (lin_1)"), 5);
    assert_eq!(menu_helper.index_from_ui_name("scene_linear (lin_1)"), 6);
    assert_eq!(menu_helper.index_from_ui_name("default"), usize::MAX);

    assert_eq!(menu_helper.num_hierarchy_levels(0), 0);
    assert_eq!(menu_helper.num_hierarchy_levels(1), 0);
    assert_eq!(menu_helper.num_hierarchy_levels(2), 0);
    assert_eq!(menu_helper.num_hierarchy_levels(3), 1);
    assert_eq!(menu_helper.num_hierarchy_levels(4), 1);
    assert_eq!(menu_helper.num_hierarchy_levels(5), 1);
    assert_eq!(menu_helper.num_hierarchy_levels(6), 1);
    assert_eq!(menu_helper.num_hierarchy_levels(7), 0);

    assert_eq!(menu_helper.hierarchy_level(3, 0), b"Roles");
    assert_eq!(menu_helper.hierarchy_level(4, 0), b"Roles");
    assert_eq!(menu_helper.hierarchy_level(5, 0), b"Roles");
    assert_eq!(menu_helper.hierarchy_level(6, 0), b"Roles");
    assert_eq!(menu_helper.hierarchy_level(7, 1), b"");
    assert_eq!(menu_helper.hierarchy_level(6, 1), b"");

    // Use an arbitrary (but existing) category only used by a named transform.

    {
        params.set_include_roles(false);
        params.set_include_named_transforms(true);
        params.set_app_categories("");
        params.set_user_categories("basic-3d");
        menu_helper = menu(&params);
        assert_eq!(menu_helper.num_color_spaces(), 1);
        assert_eq!(menu_helper.ui_name(0), b"nt1");

        // No color space is found, using all active color spaces and log a warning.
        params.set_include_named_transforms(false);
        let output;
        (menu_helper, output) = log_guard(|| menu(&params));
        assert_eq!(
            output,
            b"[OpenColorIO Info]: All parameters could not be used to create the menu: Found no \
              color space using user categories. Categories have been ignored since they \
              matched no color spaces.\n"
        );
        assert_eq!(menu_helper.num_color_spaces(), 14);
    }

    // Use a role.

    params.set_role(ROLE_RENDERING);
    params.set_app_categories("");
    params.set_include_roles(false);

    menu_helper = menu(&params);

    assert_eq!(menu_helper.num_color_spaces(), 1);

    assert_eq!(menu_helper.name(0), b"lin_1");
    assert_eq!(menu_helper.ui_name(0), b"rendering (lin_1)");
    assert_eq!(menu_helper.family(0), b"");

    // Use an existing role and app-oriented categories: categories are ignored.

    params.set_app_categories("file-io");
    menu_helper = menu(&params);

    assert_eq!(menu_helper.num_color_spaces(), 1);

    assert_eq!(menu_helper.name(0), b"lin_1");
    assert_eq!(menu_helper.ui_name(0), b"rendering (lin_1)");
    assert_eq!(menu_helper.family(0), b"");

    // Use an existing role and include roles: include roles is ignored.

    params.set_app_categories("");
    params.set_include_roles(true);

    menu_helper = menu(&params);

    assert_eq!(menu_helper.num_color_spaces(), 1);

    assert_eq!(menu_helper.name(0), b"lin_1");
    assert_eq!(menu_helper.ui_name(0), b"rendering (lin_1)");
    assert_eq!(menu_helper.family(0), b"");

    // Using an unknown category or role returns all the color spaces.
    {
        params.set_include_roles(false);
        params.set_role("");
        params.set_app_categories("unknown_category");

        // Return all the active color spaces.
        let output;
        (menu_helper, output) = log_guard(|| menu(&params));
        assert_eq!(
            output,
            b"[OpenColorIO Info]: All parameters could not be used to create the menu: Found no \
              color space using app categories. Found no color space using user categories. \
              Categories have been ignored since they matched no color spaces.\n"
        );
        assert_eq!(menu_helper.num_color_spaces(), 14);

        params.set_app_categories("");
        params.set_role("unknown_role");

        // Return all the active color spaces.
        let output;
        (menu_helper, output) = log_guard(|| menu(&params));
        assert_eq!(
            output,
            b"[OpenColorIO Info]: All parameters could not be used to create the menu: Found no \
              color space using user categories. Categories have been ignored since they \
              matched no color spaces.\n"
        );
        assert_eq!(menu_helper.num_color_spaces(), 14);
    }
}

/// Port of `OCIO_ADD_TEST(ColorSpaceMenuHelper, user_categories)` @ v2.5.2.
#[test]
fn color_space_menu_helper_user_categories() {
    let config = tagged_config("user_categories");
    config.validate().unwrap();

    let mut params = ColorSpaceMenuParameters::new(config);
    // Note: The test config has two active color spaces and a NT without categories.
    params.set_treat_no_category_as_any(true);

    // User categories can be used instead of app-oriented categories.

    params.set_user_categories("basic-2d");
    let mut menu_helper = menu(&params);
    assert_eq!(menu_helper.num_color_spaces(), 5);

    params.set_user_categories("advanced-2d");
    menu_helper = menu(&params);
    assert_eq!(menu_helper.num_color_spaces(), 6);

    params.set_user_categories("basic-2d, advanced-2d");
    params.set_include_named_transforms(true);
    menu_helper = menu(&params);
    assert_eq!(menu_helper.num_color_spaces(), 12);

    // Env. variable overrides parameter.

    {
        // Upstream's Platform::Setenv and Unsetenv, on this thread's environment.
        let env = crate::test_env::EnvGuard::new();
        env.set(&[(OCIO_USER_CATEGORIES_ENVVAR, "basic-3d")]);
        menu_helper = menu(&params);
        assert_eq!(menu_helper.num_color_spaces(), 4);
    }

    //
    // Using both app-oriented categories and user categories.
    //

    // Intersection is used if non-empty.

    params.set_include_named_transforms(false);
    params.set_app_categories("file-io, working-space");
    params.set_user_categories("advanced-2d");
    menu_helper = menu(&params);
    assert_eq!(menu_helper.num_color_spaces(), 5);
    assert_eq!(menu_helper.name(1), b"in_2");

    // Intersection is used if non-empty, named transforms can be included.

    params.set_include_named_transforms(true);
    params.set_app_categories("working-space");
    params.set_user_categories("basic-3d");
    menu_helper = menu(&params);
    assert_eq!(menu_helper.num_color_spaces(), 4);
    assert_eq!(menu_helper.name(2), b"nt1");
    params.set_include_named_transforms(false);

    // Intersection is empty. App-oriented categories are used as the fall-back.

    params.set_app_categories("look-process-space");
    params.set_user_categories("advanced-3d");
    params.set_treat_no_category_as_any(false);
    let output;
    (menu_helper, output) = log_guard(|| menu(&params));
    assert_eq!(
        output,
        b"[OpenColorIO Info]: All parameters could not be used to create the menu: Intersection \
          of color spaces with app categories and color spaces with user categories is empty. \
          User categories have been ignored.\n"
    );
    assert_eq!(menu_helper.num_color_spaces(), 3);

    // Intersection leads to no results and there are no app-oriented category results.  Fall
    // back to user categories.

    params.set_app_categories("not a category, not used");
    params.set_user_categories("basic-2d, not used");
    params.set_treat_no_category_as_any(false);
    let output;
    (menu_helper, output) = log_guard(|| menu(&params));
    assert_eq!(
        output,
        b"[OpenColorIO Info]: All parameters could not be used to create the menu: Found no \
          color space using app categories.\n"
    );
    assert_eq!(menu_helper.num_color_spaces(), 3);
}

/// Port of `OCIO_ADD_TEST(ColorSpaceMenuHelper, encodings)` @ v2.5.2.
#[test]
fn color_space_menu_helper_encodings() {
    let config = tagged_config("encodings");
    config.validate().unwrap();

    let mut params = ColorSpaceMenuParameters::new(config);
    params.set_app_categories("file-io");
    params.set_encodings("sdr-video");
    // Note: The test config has two active color spaces without categories
    // but they don't have encodings and so are not included. However, there
    // is a named transform with no category but an encoding.
    params.set_treat_no_category_as_any(true);
    let mut menu_helper = menu(&params);
    assert_eq!(menu_helper.num_color_spaces(), 3);
    assert_eq!(menu_helper.name(0), b"in_1");
    assert_eq!(menu_helper.name(1), b"in_2");
    assert_eq!(menu_helper.name(2), b"view_1");

    params.set_include_named_transforms(true);
    menu_helper = menu(&params);
    assert_eq!(menu_helper.num_color_spaces(), 4);
    assert_eq!(menu_helper.name(3), b"nt3");
    params.set_include_named_transforms(false);

    params.set_encodings("not found encoding");
    let output;
    (menu_helper, output) = log_guard(|| menu(&params));
    assert_eq!(
        output,
        b"[OpenColorIO Info]: All parameters could not be used to create the menu: Encodings \
          have been ignored since they matched no color spaces.\n"
    );
    // The encoding doesn't match, so it's only the 4 with "file-io" and the two without
    // categories.
    assert_eq!(menu_helper.num_color_spaces(), 6);

    // If intersection is empty, encodings are ignored.

    params.set_include_named_transforms(true);
    params.set_treat_no_category_as_any(false);
    params.set_app_categories("working-space");
    params.set_user_categories("basic-3d");
    params.set_encodings("log");
    let output;
    (menu_helper, output) = log_guard(|| menu(&params));
    assert_eq!(
        output,
        b"[OpenColorIO Info]: All parameters could not be used to create the menu: Encodings have \
          been ignored since they matched no color spaces.\n"
    );
    assert_eq!(menu_helper.num_color_spaces(), 1);
    assert_eq!(menu_helper.name(0), b"nt1");

    // If intersection is empty (with and without encoding), user categories are ignored and
    // encodings are used.

    params.set_include_named_transforms(true);
    params.set_treat_no_category_as_any(false);
    params.set_app_categories("file-io");
    params.set_user_categories("basic-3d");
    params.set_encodings("sdr-video");
    let output;
    (menu_helper, output) = log_guard(|| menu(&params));
    assert_eq!(
        output,
        b"[OpenColorIO Info]: All parameters could not be used to create the menu: Intersection \
          of color spaces with app categories and color spaces with user categories is empty. \
          User categories have been ignored.\n"
    );
    assert_eq!(menu_helper.num_color_spaces(), 3);
    assert_eq!(menu_helper.name(0), b"in_1");
    assert_eq!(menu_helper.name(1), b"in_2");
    assert_eq!(menu_helper.name(2), b"nt3");

    // Categories give no color space, all categories are ignored but encodings are used.

    params.set_include_named_transforms(true);
    params.set_treat_no_category_as_any(false);
    params.set_app_categories("no");
    params.set_user_categories("no");
    params.set_encodings("sdr-video");
    let output;
    (menu_helper, output) = log_guard(|| menu(&params));
    assert_eq!(
        output,
        b"[OpenColorIO Info]: All parameters could not be used to create the menu: Found no color \
          space using app categories. Found no color space using user categories. Categories \
          have been ignored since they matched no color spaces.\n"
    );
    assert_eq!(menu_helper.num_color_spaces(), 6);
    assert_eq!(menu_helper.name(0), b"in_1");
    assert_eq!(menu_helper.name(1), b"in_2");
    assert_eq!(menu_helper.name(2), b"view_1");
    assert_eq!(menu_helper.name(3), b"display_lin_2");
    assert_eq!(menu_helper.name(4), b"nt1");
    assert_eq!(menu_helper.name(5), b"nt3");

    // App-oriented categories is empty, but encodings are used. Intersection with user
    // categories.

    params.set_treat_no_category_as_any(false);
    params.set_app_categories("");
    params.set_encodings("sdr-video");
    params.set_user_categories("advanced-2d");
    let output;
    (menu_helper, output) = log_guard(|| menu(&params));
    assert_eq!(output, b"");
    assert_eq!(menu_helper.num_color_spaces(), 2);
}

/// Port of `OCIO_ADD_TEST(ColorSpaceMenuHelper, usability_issues)` @ v2.5.2.
#[test]
fn color_space_menu_helper_usability_issues() {
    // Adding a color space without categories to a config (and app) that uses them
    // results in that color space not showing up. This is fixed by treating color
    // spaces without categories as having all categories.
    {
        let is = r#"ocio_profile_version: 2
roles:
  default: raw

displays:
  DISP_1:
    - !<View> {name: VIEW_1, colorspace: test_1}

colorspaces:
  - !<ColorSpace>
    name: raw
    categories: [ file-io ]

  - !<ColorSpace>
    name: test_1
    categories: [ file-io, working-space ]

  - !<ColorSpace>
    name: test_2
    categories: [ file-io ]

    # A color space is then added with no categories present.

  - !<ColorSpace>
    name: test_3
 "#;

        let config = Config::create_from_stream(is.as_bytes()).unwrap();
        config.validate().unwrap();

        let mut params = ColorSpaceMenuParameters::new(config);
        params.set_app_categories("file-io");
        params.set_treat_no_category_as_any(false);

        let mut menu_helper = menu(&params);
        // The last color space doesn't show up, which may cause end-user confusion.
        assert_eq!(menu_helper.num_color_spaces(), 3);
        assert_eq!(menu_helper.name(0), b"raw");
        assert_eq!(menu_helper.name(1), b"test_1");
        assert_eq!(menu_helper.name(2), b"test_2");

        // Treating color spaces with no categories as having all categories solves the issue.
        params.set_treat_no_category_as_any(true);
        menu_helper = menu(&params);
        assert_eq!(menu_helper.num_color_spaces(), 4);
        assert_eq!(menu_helper.name(3), b"test_3");
    }

    // Adding a color space with categories to a config that does not use them results
    // in the original color spaces disappearing (for apps that use categories). This
    // is fixed by treating color spaces without categories as having all categories.
    {
        let is = r#"ocio_profile_version: 2
roles:
  default: raw

displays:
  DISP_1:
    - !<View> {name: VIEW_1, colorspace: test_1}

colorspaces:
  - !<ColorSpace>
    name: raw

  - !<ColorSpace>
    name: test_1

  - !<ColorSpace>
    name: test_2

    # A color space is then added that uses categories.

  - !<ColorSpace>
    name: test_3
    categories: [ file-io ]
 "#;

        let mut config = (*Config::create_from_stream(is.as_bytes()).unwrap()).clone();
        config.validate().unwrap();

        // Make test_3 inactive to simulate it not being present.
        config.set_inactive_color_spaces("test_3");

        // Upstream's parameters share the config, which the test changes below; the port's
        // hold an `Arc<Config>`, given again once the config has changed.
        let mut params = ColorSpaceMenuParameters::new(Arc::new(config.clone()));
        params.set_app_categories("file-io");
        params.set_treat_no_category_as_any(false);

        let mut menu_helper;
        {
            let output;
            (menu_helper, output) = log_guard(|| menu(&params));
            assert_eq!(
                output,
                b"[OpenColorIO Info]: All parameters could not be used to create the menu: Found \
                  no color space using app categories. Categories have been ignored since they \
                  matched no color spaces.\n"
            );
        }
        // All three expected color spaces show up, as desired.
        assert_eq!(menu_helper.num_color_spaces(), 3);
        assert_eq!(menu_helper.name(0), b"raw");
        assert_eq!(menu_helper.name(1), b"test_1");
        assert_eq!(menu_helper.name(2), b"test_2");

        // Add test_3, which contains a category.
        config.set_inactive_color_spaces("");
        params.set_config(Arc::new(config.clone()));
        menu_helper = menu(&params);
        // The original three color spaces disappear, which may cause end-user confusion.
        assert_eq!(menu_helper.num_color_spaces(), 1);

        // Treating color spaces with no categories as having all categories solves the issue.
        params.set_treat_no_category_as_any(true);
        menu_helper = menu(&params);
        assert_eq!(menu_helper.num_color_spaces(), 4);
        assert_eq!(menu_helper.name(3), b"test_3");
    }
}

/// Port of `OCIO_ADD_TEST(ColorSpaceMenuHelper, no_category)` @ v2.5.2.
#[test]
fn color_space_menu_helper_no_category() {
    let is = r#"ocio_profile_version: 1

environment:
  {}

search_path: luts
strictparsing: true

roles:
  rendering: test_1
  default: raw

displays:
  DISP_1:
    - !<View> {name: VIEW_1, colorspace: test_1}
    - !<View> {name: VIEW_2, colorspace: test_2}

active_displays: []
active_views: []

colorspaces:
  - !<ColorSpace>
    name: raw
    family: Raw
    description: A raw color space. Conversions to and from this space are no-ops.
    isdata: true
    allocation: uniform

  - !<ColorSpace>
    name: test_1

  - !<ColorSpace>
    name: test_2
 "#;

    let config = Config::create_from_stream(is.as_bytes()).unwrap();
    config.validate().unwrap();

    let mut params = ColorSpaceMenuParameters::new(config);

    // Categories are ignored when config is version 1 and no message is logged.
    {
        params.set_app_categories("file-io");

        // Return all the color spaces.
        let (menu_helper, output) = log_guard(|| menu(&params));
        assert_eq!(output, b"");
        assert_eq!(menu_helper.num_color_spaces(), 3);
    }
}

/// Port of `OCIO_ADD_TEST(ColorSpaceMenuHelper, input_color_transformation)` @ v2.5.2.
#[test]
fn color_space_menu_helper_input_color_transformation() {
    use crate::transform::Transform;
    use ocio_ops::open_color_types::TransformDirection;

    let config = tagged_config("input_color_transformation");
    config.validate().unwrap();

    //
    // Step 1 - Validate the selected input color spaces.
    //

    let mut params = ColorSpaceMenuParameters::new(Arc::clone(&config));

    params.set_app_categories("file-io");

    params.set_treat_no_category_as_any(false);

    let input_menu_helper = menu(&params);

    assert_eq!(input_menu_helper.num_color_spaces(), 4);

    assert_eq!(input_menu_helper.name(0), b"in_1");
    assert_eq!(input_menu_helper.name(1), b"in_2");
    assert_eq!(input_menu_helper.name(2), b"in_3");
    assert_eq!(input_menu_helper.name(3), b"lut_input_3");

    // Some extra validation.

    {
        assert_eq!(input_menu_helper.num_hierarchy_levels(0), 3);
        assert_eq!(input_menu_helper.hierarchy_level(0, 0), b"Input");
        assert_eq!(input_menu_helper.hierarchy_level(0, 1), b"Camera");
        assert_eq!(input_menu_helper.hierarchy_level(0, 2), b"Acme");

        assert_eq!(
            input_menu_helper.description(0),
            b"An input color space.\nFor the Acme camera."
        );
    }

    {
        assert_eq!(input_menu_helper.num_hierarchy_levels(1), 0);
        assert_eq!(input_menu_helper.description(1), b"");
    }

    //
    // Step 2 - Validate the selected working color spaces.
    //

    params.set_app_categories("working-space");
    let working_menu_helper = menu(&params);

    assert_eq!(working_menu_helper.num_color_spaces(), 7);

    assert_eq!(working_menu_helper.name(0), b"lin_1");
    assert_eq!(working_menu_helper.name(1), b"lin_2");
    assert_eq!(working_menu_helper.name(2), b"log_1");
    assert_eq!(working_menu_helper.name(3), b"in_3");
    assert_eq!(working_menu_helper.name(4), b"display_lin_1");
    assert_eq!(working_menu_helper.name(5), b"display_lin_2");
    assert_eq!(working_menu_helper.name(6), b"display_log_1");

    //
    // Step 3 - Validate the color transformation from in_1 to lin_2.
    //

    let processor = config
        .processor_between(input_menu_helper.name(0), working_menu_helper.name(1))
        .unwrap();

    let group_transform = processor.create_group_transform().unwrap();

    group_transform.validate().unwrap();

    assert_eq!(group_transform.num_transforms(), 1);

    {
        let tr = group_transform.transform(0).unwrap();

        let Transform::Exponent(exp) = tr else {
            panic!("not an ExponentTransform: {tr:?}");
        };

        assert_eq!(exp.direction(), TransformDirection::Forward);

        let values = exp.value();

        assert_eq!(values[0], 2.6);
        assert_eq!(values[1], 2.6);
        assert_eq!(values[2], 2.6);
        assert_eq!(values[3], 1.);
    }
}

/// Port of `OCIO_ADD_TEST(ColorSpaceMenuHelper, additional_color_space)` @ v2.5.2.
#[test]
fn color_space_menu_helper_additional_color_space() {
    // The unit test validates that a custom color transformation (i.e. an inactive one
    // or a newly created one not in the config instance) are correctly handled.

    let config = tagged_config("additional_color_space");
    config.validate().unwrap();

    // Use an arbitrary menu helper.

    let mut params = ColorSpaceMenuParameters::new(Arc::clone(&config));

    params.set_app_categories("file-io");
    params.set_treat_no_category_as_any(false);
    let mut menu_helper = menu(&params);

    assert_eq!(menu_helper.num_color_spaces(), 4);

    assert_eq!(menu_helper.name(0), b"in_1");
    assert_eq!(menu_helper.name(1), b"in_2");
    assert_eq!(menu_helper.name(2), b"in_3");
    assert_eq!(menu_helper.name(3), b"lut_input_3");

    //
    // Add an additional color space to the menu.
    //

    // Note that it could be an inactive color space or an active color space not having one
    // of the selected categories.

    params.add_color_space("lin_1");
    menu_helper = menu(&params);
    assert_eq!(menu_helper.num_color_spaces(), 5);

    assert_eq!(menu_helper.name(0), b"in_1");
    assert_eq!(menu_helper.name(1), b"in_2");
    assert_eq!(menu_helper.name(2), b"in_3");
    assert_eq!(menu_helper.name(3), b"lut_input_3");
    assert_eq!(menu_helper.name(4), b"lin_1");

    //
    // Add an additional color space that is already there: nothing gets added.
    //

    params.add_color_space("in_2");
    menu_helper = menu(&params);
    assert_eq!(menu_helper.num_color_spaces(), 5);

    //
    // Delete a color space and recreate the menu helper.
    //

    let mut cfg = (*config).clone();
    cfg.remove_color_space("in_1");
    // Upstream's parameters share `cfg`, which the test changes below; the port's hold an
    // `Arc<Config>`, given again once `cfg` has changed.
    params.set_config(Arc::new(cfg.clone()));
    menu_helper = menu(&params);

    assert_eq!(menu_helper.num_color_spaces(), 4);

    assert_eq!(menu_helper.name(0), b"in_2");
    assert_eq!(menu_helper.name(1), b"in_3");
    assert_eq!(menu_helper.name(2), b"lut_input_3");
    // And the additional color space is still present.
    assert_eq!(menu_helper.name(3), b"lin_1");

    // Additional color space are case insensitive.
    params.clear_added_color_spaces();
    assert_eq!(params.num_added_color_spaces(), 0);
    params.add_color_space("LIN_1");
    menu_helper = menu(&params);
    // Still get 4 items.
    assert_eq!(menu_helper.num_color_spaces(), 4);

    //
    // Same color space can't be added twice.
    //

    params.add_color_space("lin_1");
    assert_eq!(params.num_added_color_spaces(), 1);
    params.add_color_space("LIN_1");
    assert_eq!(params.num_added_color_spaces(), 1);
    params.clear_added_color_spaces();

    //
    // Add a named transform.
    //

    params.add_color_space("lin_1");
    params.add_color_space("nt1");
    assert_eq!(params.num_added_color_spaces(), 2);
    menu_helper = menu(&params);
    assert_eq!(menu_helper.num_color_spaces(), 5);
    assert_eq!(menu_helper.name(4), b"nt1");

    //
    // Add a role (first that refers to color space already there or not).
    //

    params.add_color_space(ROLE_RENDERING);
    assert_eq!(params.num_added_color_spaces(), 3);
    menu_helper = menu(&params);
    // Color space is already there: nothing is added.
    assert_eq!(menu_helper.num_color_spaces(), 5);

    params.add_color_space("default");
    menu_helper = menu(&params);
    assert_eq!(menu_helper.num_color_spaces(), 6);
    assert_eq!(menu_helper.name(5), b"raw");

    //
    // Add inactive color space.
    //

    params.clear_added_color_spaces();
    params.set_app_categories("file-io");
    menu_helper = menu(&params);
    assert_eq!(menu_helper.num_color_spaces(), 3);
    assert_eq!(menu_helper.name(0), b"in_2");
    assert_eq!(menu_helper.name(1), b"in_3");
    assert_eq!(menu_helper.name(2), b"lut_input_3");
    cfg.set_inactive_color_spaces("in_3");
    params.set_config(Arc::new(cfg.clone()));
    menu_helper = menu(&params);
    assert_eq!(menu_helper.num_color_spaces(), 2);
    assert_eq!(menu_helper.name(0), b"in_2");
    assert_eq!(menu_helper.name(1), b"lut_input_3");
    params.add_color_space("in_3");
    menu_helper = menu(&params);
    assert_eq!(menu_helper.num_color_spaces(), 3);
    assert_eq!(menu_helper.name(0), b"in_2");
    assert_eq!(menu_helper.name(1), b"lut_input_3");
    assert_eq!(menu_helper.name(2), b"in_3");

    //
    // Add a color space that does not exist.
    //

    params.add_color_space("doesNotExist");
    check_throw_what(
        ColorSpaceMenuHelper::new(&params),
        "Element 'doesNotExist' is neither a color space not a named transform",
    );
}
