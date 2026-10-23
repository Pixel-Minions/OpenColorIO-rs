// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/apphelpers/DisplayViewHelpers_tests.cpp` @ v2.5.2.

use super::*;
use crate::apphelpers::color_space_helpers::{ColorSpaceMenuHelper, ColorSpaceMenuParameters};
use crate::apphelpers::configs_data::CATEGORY_TEST_CONFIG;
use crate::test_env::EnvGuard;
use crate::transform::TransformType;
use ocio_testkit::upstream::{check_close, check_throw_what};

/// The transform of `variant` that `tr` is (upstream's `DynamicPtrCast`, then
/// `OCIO_REQUIRE_ASSERT` on it).
macro_rules! typed {
    ($tr:expr, $variant:ident) => {
        match $tr {
            Transform::$variant(t) => t,
            other => panic!(
                "expected a {} transform, got {:?}",
                stringify!($variant),
                other.transform_type()
            ),
        }
    };
}

/// `ocioTestFilesDir + "/lut1d_green.ctf"` (OCIO_UNIT_TEST_FILES_DIR is tests/data/files).
fn lut1d_green() -> String {
    ocio_testkit::paths::upstream_dir()
        .join("tests/data/files/lut1d_green.ctf")
        .to_string_lossy()
        .into_owned()
}

/// Port of `OCIO_ADD_TEST(DisplayViewHelpers, basic)` @ v2.5.2.
#[test]
fn basic() {
    let mut cfg = Config::create_from_stream(CATEGORY_TEST_CONFIG.as_bytes()).unwrap();
    cfg.validate().unwrap();

    //
    // Step 1 - Validate the selected working color spaces.
    //

    let mut params = ColorSpaceMenuParameters::new(Arc::clone(&cfg));
    params.set_treat_no_category_as_any(false);
    params.set_app_categories("working-space");
    let working_menu_helper = ColorSpaceMenuHelper::new(&params).unwrap();

    assert_eq!(working_menu_helper.num_color_spaces(), 7);

    assert_eq!(working_menu_helper.name(0), b"lin_1");
    assert_eq!(working_menu_helper.name(1), b"lin_2");
    assert_eq!(working_menu_helper.name(2), b"log_1");
    assert_eq!(working_menu_helper.name(3), b"in_3");
    assert_eq!(working_menu_helper.name(4), b"display_lin_1");
    assert_eq!(working_menu_helper.name(5), b"display_lin_2");
    assert_eq!(working_menu_helper.name(6), b"display_log_1");

    //
    // Step 2 - Validate the selected connection color spaces.
    //

    params.set_app_categories("LUT-connection-space");
    let connection_menu_helper = ColorSpaceMenuHelper::new(&params).unwrap();

    assert_eq!(connection_menu_helper.num_color_spaces(), 1);

    assert_eq!(connection_menu_helper.name(0), b"lut_input_1");

    //
    // Step 3 - Create a (display, view) pair.
    //

    let mut config = (*cfg).clone();

    let file_path = lut1d_green();

    add_display_view(
        &mut config,
        "DISP_1",
        "VIEW_5",
        "look_3",
        "view_5",
        "",
        "",
        "cat1, cat2",
        &file_path,
        "lut_input_1",
    )
    .unwrap();
    // Keep in sync. the configs.
    cfg = Arc::new(config.clone());

    //
    // Step 4 - Validate the new (display, view) pair.
    //

    assert_eq!(config.view("DISP_1", 3), b"VIEW_5");

    assert_eq!(
        config.display_view_color_space_name("DISP_1", "VIEW_5"),
        b"view_5"
    );

    assert_eq!(config.display_view_looks("DISP_1", "VIEW_5"), b"look_3");

    //
    // Step 5 - Check the newly created color space.
    //
    {
        let cs = config.color_space("view_5").unwrap();
        // These categories were not already used in the config, so AddDisplayView ignores them.
        assert!(!cs.has_category("cat1"));
        assert!(!cs.has_category("cat2"));
        assert_eq!(cs.family(), b"");
        assert_eq!(cs.description(), b"");
    }

    //
    // Step 6 - Create a processor for the new (display, view) pair.
    //
    let processor = processor(
        &cfg,
        None,
        "lin_1",
        "DISP_1",
        "VIEW_5",
        None,
        TransformDirection::Forward,
    )
    .unwrap();

    let group_transform = processor.create_group_transform().unwrap();

    group_transform.validate().unwrap();

    assert_eq!(group_transform.num_transforms(), 7);

    // The E/C op.
    {
        let ex = typed!(group_transform.transform(0).unwrap(), ExposureContrast);

        assert_eq!(ex.direction(), TransformDirection::Forward);
        assert_eq!(ex.style(), ExposureContrastStyle::Linear);
        assert_eq!(ex.pivot(), 0.18);

        assert_eq!(ex.exposure(), 0.);
        assert!(ex.is_exposure_dynamic());

        assert_eq!(ex.contrast(), 1.);
        assert!(ex.is_contrast_dynamic());

        assert_eq!(ex.gamma(), 1.);
        assert!(!ex.is_gamma_dynamic());
    }

    // Working color space (i.e. lin_1) to the 'look' process color space (i.e. log_1).
    {
        let log = typed!(group_transform.transform(1).unwrap(), Log);

        assert_eq!(log.direction(), TransformDirection::Forward);
        assert_eq!(log.base(), 2.);
    }

    // 'look' color processing i.e. look_3.
    {
        let cdl = typed!(group_transform.transform(2).unwrap(), Cdl);

        assert_eq!(cdl.direction(), TransformDirection::Forward);

        let rgb = cdl.slope();
        assert_eq!(rgb[0], 1.);
        assert_eq!(rgb[1], 2.);
        assert_eq!(rgb[2], 1.);

        let rgb = cdl.power();
        assert_eq!(rgb[0], 1.);
        assert_eq!(rgb[1], 1.);
        assert_eq!(rgb[2], 1.);

        assert_eq!(cdl.sat(), 1.);
    }

    // 'look' process color space (i.e. log_1) to 'reference'.
    {
        let log = typed!(group_transform.transform(3).unwrap(), Log);

        assert_eq!(log.direction(), TransformDirection::Inverse);
        assert_eq!(log.base(), 2.);
    }

    // 'reference' to the display color space (i.e. view_3).
    {
        // The 'view_3' color space is a group transform containing:
        //  1. 'reference' to connection color space i.e. lut_1.
        //  2. The user 1D LUT.

        let exp = typed!(group_transform.transform(4).unwrap(), Exponent);

        assert_eq!(exp.direction(), TransformDirection::Inverse);

        let values = exp.value();

        assert_eq!(values[0], 2.6);
        assert_eq!(values[1], 2.6);
        assert_eq!(values[2], 2.6);
        assert_eq!(values[3], 1.);

        let lut = typed!(group_transform.transform(5).unwrap(), Lut1D);
        assert_eq!(lut.direction(), TransformDirection::Forward);

        let [r, g, b] = lut.value(0).unwrap();
        assert_eq!(r, 0.);
        assert_eq!(g, 0.);
        assert_eq!(b, 0.);

        let [r, g, b] = lut.value(1).unwrap();
        assert_eq!(r, 0.);
        check_close(g, 33.0f32 / 1023.0f32, 1e-8f32);
        assert_eq!(b, 0.);

        let [r, g, b] = lut.value(2).unwrap();
        assert_eq!(r, 0.);
        check_close(g, 66.0f32 / 1023.0f32, 1e-8f32);
        assert_eq!(b, 0.);
    }

    // The E/C op.
    {
        let ex = typed!(group_transform.transform(6).unwrap(), ExposureContrast);

        assert_eq!(ex.direction(), TransformDirection::Forward);
        assert_eq!(ex.style(), ExposureContrastStyle::Video);
        assert_eq!(ex.pivot(), 1.);

        assert_eq!(ex.exposure(), 0.);
        assert!(!ex.is_exposure_dynamic());

        assert_eq!(ex.contrast(), 1.);
        assert!(!ex.is_contrast_dynamic());

        assert_eq!(ex.gamma(), 1.);
        assert!(ex.is_gamma_dynamic());
    }

    //
    // Step 7 - Some faulty scenarios.
    //

    {
        // Color space already exists.
        check_throw_what(
            add_display_view(
                &mut config,
                "",
                "VIEW_4",
                "look_3",
                "view_5",
                "",
                "",
                "cat1, cat2",
                &file_path,
                "lut_input_1",
            ),
            "Color space name 'view_5' already exists.",
        );
    }

    {
        // Display is null.
        check_throw_what(
            add_display_view(
                &mut config,
                "",
                "VIEW_4",
                "look_3",
                "view_51",
                "",
                "",
                "cat1, cat2",
                &file_path,
                "lut_input_1",
            ),
            "Invalid display name.",
        );
    }

    {
        // View is null.
        check_throw_what(
            add_display_view(
                &mut config,
                "DISP_1",
                "",
                "look_3",
                "view_51",
                "",
                "",
                "cat1, cat2",
                &file_path,
                "lut_input_1",
            ),
            "Invalid view name.",
        );
    }

    {
        // Connection CS does not exist.
        check_throw_what(
            add_display_view(
                &mut config,
                "DISP_1",
                "VIEW_4",
                "look_3",
                "view_51",
                "",
                "",
                "cat1, cat2",
                &file_path,
                "lut_unknown",
            ),
            "Connection color space name 'lut_unknown' does not exist.",
        );
    }

    //
    // Step 8 - Remove the display/view.
    //

    let _ = config.view("DISP_1", 3);

    remove_display_view(&mut config, "DISP_1", "VIEW_5").unwrap();

    assert!(config.color_space("view_5").is_none());
    let _ = config.view("DISP_1", 2);
}

/// Port of `OCIO_ADD_TEST(DisplayViewHelpers, display_view_without_look)` @ v2.5.2.
#[test]
fn display_view_without_look() {
    let cfg = Config::create_from_stream(CATEGORY_TEST_CONFIG.as_bytes()).unwrap();
    cfg.validate().unwrap();

    // Forward direction.

    let proc = processor(
        &cfg,
        None,
        "lin_1",
        "DISP_1",
        "VIEW_1",
        None,
        TransformDirection::Forward,
    )
    .unwrap();

    let group_transform = proc.create_group_transform().unwrap();
    group_transform.validate().unwrap();
    assert_eq!(group_transform.num_transforms(), 3);

    let exp = typed!(group_transform.transform(1).unwrap(), Exponent);
    assert_eq!(exp.direction(), TransformDirection::Inverse);

    // Inverse direction.

    let proc = processor(
        &cfg,
        None,
        "lin_1",
        "DISP_1",
        "VIEW_1",
        None,
        TransformDirection::Inverse,
    )
    .unwrap();

    let group_transform = proc.create_group_transform().unwrap();
    group_transform.validate().unwrap();
    assert_eq!(group_transform.num_transforms(), 3);

    let exp = typed!(group_transform.transform(1).unwrap(), Exponent);
    assert_eq!(exp.direction(), TransformDirection::Forward);

    // Forward with a channel view matrix.

    let mut cv = MatrixTransform::new();
    let mat: [f64; 16] = [
        1.0, 0.0, 0.0, 0.0, //
        0.0, 0.0, 0.0, 0.0, //
        0.0, 0.0, 0.0, 0.0, //
        0.0, 0.0, 0.0, 0.0,
    ];
    cv.set_matrix(&mat);

    let proc = processor(
        &cfg,
        None,
        "lin_1",
        "DISP_1",
        "VIEW_1",
        Some(&cv),
        TransformDirection::Forward,
    )
    .unwrap();

    let group_transform = proc.create_group_transform().unwrap();
    group_transform.validate().unwrap();
    assert_eq!(group_transform.num_transforms(), 4);

    let mt = typed!(group_transform.transform(1).unwrap(), Matrix);
    assert_eq!(mt.direction(), TransformDirection::Forward);
    let mat = mt.matrix();
    assert_eq!(mat[0], 1.);
    assert_eq!(mat[5], 0.);

    // Inverse test with a channel view matrix can't be done because channel view matrix is
    // singular and inversion will fail.
}

/// Port of `OCIO_ADD_TEST(DisplayViewHelpers, active_display_view)` @ v2.5.2.
#[test]
fn active_display_view() {
    let env = EnvGuard::new();

    let mut cfg = (*Config::create_from_stream(CATEGORY_TEST_CONFIG.as_bytes()).unwrap()).clone();
    cfg.validate().unwrap();

    // Step 1 - Check the current status.

    assert_eq!(cfg.num_displays(), 2);
    assert_eq!(cfg.num_views("DISP_1"), 3);
    assert_eq!(cfg.num_views("DISP_2"), 4);

    // Step 2 - Add some active displays & views.

    cfg.set_active_displays("DISP_1").unwrap();
    cfg.set_active_views("VIEW_3, VIEW_2").unwrap();

    assert_eq!(cfg.num_displays(), 1);
    assert_eq!(cfg.display(0), b"DISP_1");

    assert_eq!(cfg.num_views("DISP_1"), 2);
    assert_eq!(cfg.view("DISP_1", 0), b"VIEW_3");
    assert_eq!(cfg.view("DISP_1", 1), b"VIEW_2");

    // Step 3 - Create a (display, view) pair.

    let file_path = lut1d_green();

    let mut user_transform = FileTransform::new();
    user_transform.set_src(&file_path);

    add_display_view(
        &mut cfg,
        "DISP_1",
        "VIEW_5",
        "",
        "VIEW_5",
        "",
        "",
        "cat1, cat2",
        &file_path,
        "lut_input_1",
    )
    .unwrap();

    // The active displays & views were correctly updated.
    assert_eq!(cfg.active_displays(), b"DISP_1");
    assert_eq!(cfg.active_views(), b"VIEW_3, VIEW_2, VIEW_5");

    assert_eq!(cfg.num_displays(), 1);
    assert_eq!(cfg.display(0), b"DISP_1");

    assert_eq!(cfg.num_views("DISP_1"), 3);
    assert_eq!(cfg.view("DISP_1", 0), b"VIEW_3");
    assert_eq!(cfg.view("DISP_1", 1), b"VIEW_2");
    assert_eq!(cfg.view("DISP_1", 2), b"VIEW_5");

    // Step 4 - Remove a (display, view) pair.

    remove_display_view(&mut cfg, "DISP_1", "VIEW_5").unwrap();

    assert_eq!(cfg.active_displays(), b"DISP_1");
    assert_eq!(cfg.active_views(), b"VIEW_3, VIEW_2");

    assert_eq!(cfg.num_displays(), 1);
    assert_eq!(cfg.display(0), b"DISP_1");

    assert_eq!(cfg.num_views("DISP_1"), 2);
    assert_eq!(cfg.view("DISP_1", 0), b"VIEW_3");
    assert_eq!(cfg.view("DISP_1", 1), b"VIEW_2");

    // Step 5 - Reset active displays & views.

    cfg.set_active_displays("").unwrap();
    cfg.set_active_views("").unwrap();

    assert_eq!(cfg.num_displays(), 2);
    assert_eq!(cfg.display(0), b"DISP_1");
    assert_eq!(cfg.display(1), b"DISP_2");

    assert_eq!(cfg.num_views("DISP_1"), 3);
    assert_eq!(cfg.view("DISP_1", 0), b"VIEW_1");
    assert_eq!(cfg.view("DISP_1", 1), b"VIEW_2");
    assert_eq!(cfg.view("DISP_1", 2), b"VIEW_3");

    // Step 6 - Add some active displays.

    {
        env.set(&[("OCIO_ACTIVE_DISPLAYS", "DISP_1")]);

        // Grab the envvar value.
        cfg = (*Config::create_from_stream(CATEGORY_TEST_CONFIG.as_bytes()).unwrap()).clone();

        assert_eq!(cfg.num_displays(), 1);
        assert_eq!(cfg.display(0), b"DISP_1");

        assert_eq!(cfg.num_views("DISP_1"), 3);
        assert_eq!(cfg.view("DISP_1", 0), b"VIEW_1");
        assert_eq!(cfg.view("DISP_1", 1), b"VIEW_2");
        assert_eq!(cfg.view("DISP_1", 2), b"VIEW_3");

        check_throw_what(
            add_display_view(
                &mut cfg,
                "DISP_5",
                "VIEW_5",
                "",
                "VIEW_5",
                "",
                "",
                "cat1, cat2",
                &file_path,
                "lut_input_1",
            ),
            "Forbidden to add an active display as 'OCIO_ACTIVE_DISPLAYS' controls the active \
             list.",
        );
        // `ActiveGuard`'s destructor sets the variable to "".
        env.set(&[("OCIO_ACTIVE_DISPLAYS", "")]);
    }

    // Step 7 - Add some active views.

    {
        env.set(&[("OCIO_ACTIVE_VIEWS", "VIEW_3, VIEW_2")]);

        // Grab the envvar value.
        cfg = (*Config::create_from_stream(CATEGORY_TEST_CONFIG.as_bytes()).unwrap()).clone();

        assert_eq!(cfg.num_displays(), 2);
        assert_eq!(cfg.display(0), b"DISP_1");
        assert_eq!(cfg.display(1), b"DISP_2");

        assert_eq!(cfg.num_views("DISP_1"), 2);
        assert_eq!(cfg.view("DISP_1", 0), b"VIEW_3");
        assert_eq!(cfg.view("DISP_1", 1), b"VIEW_2");

        check_throw_what(
            add_display_view(
                &mut cfg,
                "DISP_1",
                "VIEW_5",
                "",
                "VIEW_5",
                "",
                "",
                "cat1, cat2",
                &file_path,
                "lut_input_1",
            ),
            "Forbidden to add an active view as 'OCIO_ACTIVE_VIEWS' controls the active list.",
        );
        env.set(&[("OCIO_ACTIVE_VIEWS", "")]);
    }
}

/// Port of `OCIO_ADD_TEST(DisplayViewHelpers, remove_display_view)` @ v2.5.2.
#[test]
fn remove_display_view_test() {
    // Validate that a color space is removed or not depending of its usage i.e. color spaces
    // used by a ColorSpaceTransform for example. When removing a (display, view) pair the
    // associated color space is then removed only if not used.

    const CONFIG: &str = "ocio_profile_version: 2\n\
        \n\
        environment:\n\
        \x20 {}\n\
        \n\
        search_path: luts\n\
        strictparsing: true\n\
        luma: [0.2126, 0.7152, 0.0722]\n\
        \n\
        roles:\n\
        \x20 default: cs1\n\
        \n\
        displays:\n\
        \x20 disp1:\n\
        \x20   - !<View> {name: view1, colorspace: cs1}\n\
        \x20   - !<View> {name: view2, colorspace: cs2}\n\
        \x20   - !<View> {name: view3, colorspace: cs3}\n\
        \x20   - !<View> {name: view4, colorspace: cs2}\n\
        \n\
        colorspaces:\n\
        \x20 - !<ColorSpace>\n\
        \x20   name: cs1\n\
        \n\
        \x20 - !<ColorSpace>\n\
        \x20   name: cs2\n\
        \n\
        \x20 - !<ColorSpace>\n\
        \x20   name: cs3\n\
        \x20   from_reference: !<ColorSpaceTransform> {src: cs2, dst: cs2}\n";

    let mut config = (*Config::create_from_stream(CONFIG.as_bytes()).unwrap()).clone();
    config.validate().unwrap();
    assert_eq!(config.num_views("disp1"), 4);

    // Remove a (display, view) pair.

    remove_display_view(&mut config, "disp1", "view2").unwrap();
    assert_eq!(config.num_views("disp1"), 3);
    // 'cs2' still exists because it's used by 'cs3' and the (disp1, view4) pair.
    assert!(config.color_space("cs2").is_some());

    remove_display_view(&mut config, "disp1", "view3").unwrap();
    assert_eq!(config.num_views("disp1"), 2);
    // 'cs3' is removed because it was not used.
    assert!(config.color_space("cs3").is_none());

    remove_display_view(&mut config, "disp1", "view4").unwrap();
    assert_eq!(config.num_views("disp1"), 1);
    // 'cs2' is removed because it was not anymore used (i.e. 'cs3' is now removed).
    assert!(config.color_space("cs2").is_none());
}

/// Port of `OCIO_ADD_TEST(DisplayViewHelpers, identity_processor)` @ v2.5.2.
#[test]
fn identity_processor_test() {
    let config = Config::new().unwrap();
    let identity = identity_processor(&config).unwrap();
    let grp = identity.create_group_transform().unwrap();
    assert_eq!(grp.num_transforms(), 2);
    assert_eq!(
        grp.transform(0).unwrap().transform_type(),
        TransformType::ExposureContrast
    );
    assert_eq!(
        grp.transform(1).unwrap().transform_type(),
        TransformType::ExposureContrast
    );
    let ec0 = typed!(grp.transform(0).unwrap(), ExposureContrast);
    let ec1 = typed!(grp.transform(1).unwrap(), ExposureContrast);
    assert!(ec0.is_contrast_dynamic());
    assert!(ec0.is_exposure_dynamic());
    assert!(!ec0.is_gamma_dynamic());
    assert!(!ec1.is_contrast_dynamic());
    assert!(!ec1.is_exposure_dynamic());
    assert!(ec1.is_gamma_dynamic());
}
