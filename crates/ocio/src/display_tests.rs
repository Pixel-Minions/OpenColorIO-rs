// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the displays and views: `tests/cpu/Display_tests.cpp` @ v2.5.2. Its two `Config`
//! tests, which compare the views of two configs, are in `config_tests.rs`.

use ocio_ops::open_color_types::{ReferenceSpaceType, ViewTransformDirection, ViewType};
use ocio_testkit::upstream::check_throw_what;

use super::*;
use crate::color_space::ColorSpace;
use crate::config::Config;
use crate::look::Look;
use crate::test_env::EnvGuard;
use crate::transform::Transform;
use crate::transforms::matrix_transform::MatrixTransform;
use crate::view_transform::ViewTransform;
use crate::viewing_rules::ViewingRules;

/// Port of `OCIO_ADD_TEST(SharedViews, basic)` @ v2.5.2.
#[test]
fn shared_views_basic() {
    let _env = EnvGuard::new();
    // Shared views can not be used with v1 config.
    let mut config = Config::new().unwrap();
    config.set_major_version(1).unwrap();
    config
        .add_shared_view("shared1", "", "colorspace", "", "", "")
        .unwrap();
    check_throw_what(
        config.serialize(),
        "Only version 2 (or higher) can have shared views",
    );

    // Using a v2 config.
    let mut config = (*Config::create_raw().unwrap()).clone();
    config.validate().unwrap();

    // Shared views need to refer to existing colorspaces.
    config
        .add_shared_view("shared1", "", "colorspace1", "", "", "")
        .unwrap();
    check_throw_what(
        config.validate(),
        "color space or a named transform, 'colorspace1', which is not defined",
    );

    let mut cs = ColorSpace::new();
    cs.set_name("colorspace1");
    config.add_color_space(&cs).unwrap();
    config.validate().unwrap();

    // Shared views need to refer to existing looks.
    cs.set_name("colorspace2");
    config.add_color_space(&cs).unwrap();
    config
        .add_shared_view("shared2", "", "colorspace2", "look1", "", "")
        .unwrap();
    check_throw_what(
        config.validate(),
        "refers to a look, 'look1', which is not defined.",
    );

    let mut lk = Look::new();
    lk.set_name("look1");
    lk.set_process_space("look1_process");
    cs.set_name("look1_process");
    config.add_color_space(&cs).unwrap();
    config.add_look(&lk).unwrap();
    config.validate().unwrap();

    // Shared views need to refer to existing view transforms.
    let mut cs = ColorSpace::with_reference_space(ReferenceSpaceType::Display);
    cs.set_name("colorspace3");
    config.add_color_space(&cs).unwrap();
    config
        .add_shared_view(
            "shared3",
            "viewTransform1",
            "colorspace3",
            "",
            "",
            "shared view description",
        )
        .unwrap();
    check_throw_what(
        config.validate(),
        "refers to a view transform, 'viewTransform1', which is neither a view transform nor a \
         named transform",
    );

    let mut vt = ViewTransform::new(ReferenceSpaceType::Scene);
    vt.set_name("viewTransform1");
    vt.set_transform(
        Some(&Transform::from(MatrixTransform::new())),
        ViewTransformDirection::FromReference,
    )
    .unwrap();
    config.add_view_transform(&vt).unwrap();
    config.validate().unwrap();

    // Shared views need to refer to existing rules.
    config
        .add_shared_view("shared4", "", "colorspace1", "", "rule1", "")
        .unwrap();
    check_throw_what(
        config.validate(),
        "viewing rule, 'rule1', which is not defined",
    );

    let mut vrules = ViewingRules::new();
    vrules.insert_rule(0, "rule1").unwrap();
    vrules.add_color_space(0, "colorspace3").unwrap();

    config.set_viewing_rules(&vrules);
    config.validate().unwrap();

    // Add shared view with description.
    config
        .add_shared_view("shared5", "", "colorspace2", "", "", "Sample description")
        .unwrap();
    config.validate().unwrap();

    // Add another view to the sRGB display (CreateRaw creates an sRGB display with a Raw view).
    config
        .add_display_view("sRGB", "view1", "colorspace1", "")
        .unwrap();
    config.validate().unwrap();

    config.add_display_shared_view("sRGB", "shared2").unwrap();
    config.add_display_shared_view("sRGB", "shared3").unwrap();
    config.add_display_shared_view("sRGB", "shared4").unwrap();
    config.validate().unwrap();

    // Expecting five views: two DISPLAY_DEFINED views plus three SHARED views.
    assert_eq!(5, config.num_views("sRGB"));
    assert_eq!(b"Raw", config.view("sRGB", 0));
    assert_eq!(b"view1", config.view("sRGB", 1));
    assert_eq!(b"shared2", config.view("sRGB", 2));
    assert_eq!(b"shared3", config.view("sRGB", 3));
    assert_eq!(b"shared4", config.view("sRGB", 4));
    assert_eq!(
        2,
        config.num_views_of_type(ViewType::DisplayDefined, "sRGB")
    );
    assert_eq!(
        b"Raw",
        config.view_of_type(ViewType::DisplayDefined, "sRGB", 0)
    );
    assert_eq!(
        b"view1",
        config.view_of_type(ViewType::DisplayDefined, "sRGB", 1)
    );
    assert_eq!(3, config.num_views_of_type(ViewType::Shared, "sRGB"));
    assert_eq!(b"shared2", config.view_of_type(ViewType::Shared, "sRGB", 0));
    assert_eq!(b"shared3", config.view_of_type(ViewType::Shared, "sRGB", 1));
    assert_eq!(b"shared4", config.view_of_type(ViewType::Shared, "sRGB", 2));

    // Access view properties (either display-defined or shared views).
    assert_eq!(
        b"colorspace1",
        config.display_view_color_space_name("sRGB", "view1")
    );
    assert_eq!(
        b"colorspace2",
        config.display_view_color_space_name("sRGB", "shared2")
    );
    assert_eq!(
        b"viewTransform1",
        config.display_view_transform_name("sRGB", "shared3")
    );
    assert_eq!(b"look1", config.display_view_looks("sRGB", "shared2"));
    assert_eq!(b"rule1", config.display_view_rule("sRGB", "shared4"));
    assert_eq!(
        b"shared view description",
        config.display_view_description("sRGB", "shared3")
    );

    // A null or empty display name may be used to access shared views (regardless of whether
    // they are used in any displays). (Upstream's null pointer is the empty string.)
    assert_eq!(
        b"colorspace1",
        config.display_view_color_space_name("", "shared1")
    );
    assert_eq!(
        b"colorspace2",
        config.display_view_color_space_name("", "shared2")
    );
    assert_eq!(b"look1", config.display_view_looks("", "shared2"));
    assert_eq!(
        b"viewTransform1",
        config.display_view_transform_name("", "shared3")
    );
    assert_eq!(
        b"colorspace3",
        config.display_view_color_space_name("", "shared3")
    );
    assert_eq!(b"rule1", config.display_view_rule("", "shared4"));
    assert_eq!(
        b"Sample description",
        config.display_view_description("", "shared5")
    );

    // Use active views.
    config.set_active_views("view1, shared3").unwrap();
    assert_eq!(2, config.num_views("sRGB"));
    assert_eq!(b"view1", config.view("sRGB", 0));
    assert_eq!(b"shared3", config.view("sRGB", 1));

    // Even if not active, view properties can be queried.
    assert_eq!(b"look1", config.display_view_looks("sRGB", "shared2"));

    // These are not affected by active views.
    assert_eq!(
        2,
        config.num_views_of_type(ViewType::DisplayDefined, "sRGB")
    );
    assert_eq!(3, config.num_views_of_type(ViewType::Shared, "sRGB"));

    // Save and reload.
    let config_str = config.serialize().unwrap();
    let config_back = Config::create_from_stream(&config_str).unwrap();

    // Verify reloaded version of config.
    assert_eq!(
        config.num_views_of_type(ViewType::Shared, ""),
        config_back.num_views_of_type(ViewType::Shared, "")
    );
    assert_eq!(
        b"viewTransform1",
        config_back.display_view_transform_name("", "shared3")
    );
    assert_eq!(
        b"colorspace3",
        config_back.display_view_color_space_name("", "shared3")
    );
    assert_eq!(b"rule1", config_back.display_view_rule("", "shared4"));
    assert_eq!(
        b"Sample description",
        config_back.display_view_description("", "shared5")
    );

    // Add view to display with name of existing shared view will throw.
    check_throw_what(
        config.add_display_view("sRGB", "shared2", "colorspace1", ""),
        "There is already a shared view named 'shared2' in the display 'sRGB'",
    );

    // Add shared view to a display with name of existing view will throw.
    // Shared1 is a shared view, but it is not used by sRGB, so a view with that name
    // can be added as a display-defined view.
    config
        .add_display_view("sRGB", "shared1", "colorspace1", "")
        .unwrap();
    config.validate().unwrap();
    check_throw_what(
        config.add_display_shared_view("sRGB", "shared1"),
        "There is already a view named 'shared1' in the display 'sRGB'",
    );

    assert_eq!(3, config.num_views_of_type(ViewType::Shared, "sRGB"));
    config.validate().unwrap();

    // Add undefined shared view.
    config.add_display_shared_view("sRGB", "shared42").unwrap();
    check_throw_what(
        config.validate(),
        "contains a shared view 'shared42' that is not defined",
    );

    // Remove faulty view.
    config.remove_display_view("sRGB", "shared42").unwrap();
    config.validate().unwrap();

    // Remove unused shared view.
    config.remove_shared_view("shared1").unwrap();
    config.validate().unwrap();

    // Replace one of the existing shared views.  This time, it uses only a view transform and
    // special color space name. However, the config is missing a display color space having the
    // same name as the display.
    config
        .add_shared_view(
            "shared3",
            "viewTransform1",
            OCIO_VIEW_USE_DISPLAY_NAME,
            "",
            "",
            "shared view description",
        )
        .unwrap();

    check_throw_what(
        config.validate(),
        "The display 'sRGB' contains a shared view 'shared3' which does not define a color space \
         and there is no color space that matches the display name",
    );

    cs.set_name("sRGB");
    config.add_color_space(&cs).unwrap();
    config.validate().unwrap();

    // Verify that shared views with no color space are saved with a special display
    // color space name, and that they are properly loaded.
    let config_str = config.serialize().unwrap();

    assert!(
        config_str
            .windows(OCIO_VIEW_USE_DISPLAY_NAME.len())
            .any(|w| w == OCIO_VIEW_USE_DISPLAY_NAME.as_bytes())
    );

    let config_back = Config::create_from_stream(&config_str).unwrap();

    assert_eq!(
        OCIO_VIEW_USE_DISPLAY_NAME.as_bytes(),
        config_back.display_view_color_space_name("", "shared3")
    );

    // Remove all shared views
    assert_eq!(4, config.num_views_of_type(ViewType::Shared, ""));
    config.clear_shared_views();
    assert_eq!(0, config.num_views_of_type(ViewType::Shared, ""));
}
