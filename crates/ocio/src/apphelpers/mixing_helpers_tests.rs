// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/apphelpers/MixingHelpers_tests.cpp` @ v2.5.2.
//!
//! Upstream holds the slider's reference (`getSlider`) across calls that change the manager;
//! the port borrows it from the manager again after each ([`MixingColorSpaceManager::slider`]).

use super::*;
use crate::apphelpers::configs_data::CATEGORY_TEST_CONFIG;
use crate::transforms::group_transform::GroupTransform;
use ocio_testkit::upstream::{check_throw_what, mixing_float_check_equal as float_check_equal};

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

/// The group transform of `proc` (`createGroupTransform`), validated.
fn group_of(proc: &Processor) -> GroupTransform {
    let group = proc.create_group_transform().unwrap();
    group.validate().unwrap();
    group
}

fn category_config() -> Arc<Config> {
    let config = Config::create_from_stream(CATEGORY_TEST_CONFIG.as_bytes()).unwrap();
    config.validate().unwrap();
    config
}

/// Port of `OCIO_ADD_TEST(MixingColorSpaceManager, basic)` @ v2.5.2.
#[test]
fn mixing_color_space_manager_basic() {
    let config = category_config();

    let mut mixing_helper = MixingColorSpaceManager::new(config).unwrap();

    {
        let processor = mixing_helper
            .processor("lin_1", "DISP_1", "VIEW_1", TransformDirection::Forward)
            .unwrap();

        let group_transform = group_of(&processor);
        assert_eq!(group_transform.num_transforms(), 1);

        let tr = group_transform.transform(0).unwrap();

        typed!(tr, Matrix);
    }

    assert_eq!(mixing_helper.selected_mixing_encoding_idx(), 0);
    assert_eq!(mixing_helper.num_mixing_encodings(), 2);

    mixing_helper.set_selected_mixing_encoding("HSV").unwrap();
    assert_eq!(mixing_helper.selected_mixing_encoding_idx(), 1);
    mixing_helper.set_selected_mixing_encoding_idx(0).unwrap();
    assert_eq!(mixing_helper.selected_mixing_encoding_idx(), 0);

    assert!(mixing_helper.set_selected_mixing_encoding("HS").is_err());

    mixing_helper.set_selected_mixing_encoding_idx(1).unwrap(); // i.e. HSV

    {
        let processor = mixing_helper
            .processor("lin_1", "DISP_1", "VIEW_1", TransformDirection::Forward)
            .unwrap();

        let group_transform = group_of(&processor);
        assert_eq!(group_transform.num_transforms(), 2);

        {
            let tr = group_transform.transform(0).unwrap();
            typed!(tr, Matrix);

            let tr = group_transform.transform(1).unwrap();
            let ff = typed!(tr, FixedFunction);

            assert_eq!(ff.direction(), TransformDirection::Forward);
            assert_eq!(ff.style(), FixedFunctionStyle::RgbToHsv);
        }
    }

    assert_eq!(mixing_helper.selected_mixing_space_idx(), 0);
    assert_eq!(mixing_helper.num_mixing_spaces(), 2);

    mixing_helper
        .set_selected_mixing_space("Display Space")
        .unwrap();
    assert_eq!(mixing_helper.selected_mixing_space_idx(), 1);
    mixing_helper.set_selected_mixing_space_idx(0).unwrap();
    assert_eq!(mixing_helper.selected_mixing_space_idx(), 0);

    assert!(
        mixing_helper
            .set_selected_mixing_space("DisplaySpace")
            .is_err()
    );

    mixing_helper.set_selected_mixing_space_idx(1).unwrap(); // i.e. 'Display Space'

    {
        let processor = mixing_helper
            .processor("lin_1", "DISP_1", "VIEW_1", TransformDirection::Forward)
            .unwrap();

        let group_transform = group_of(&processor);
        assert_eq!(group_transform.num_transforms(), 2);

        {
            let tr = group_transform.transform(0).unwrap();

            let exp = typed!(tr, Exponent);

            assert_eq!(exp.direction(), TransformDirection::Inverse);

            let values = exp.value();

            assert_eq!(values[0], 2.6);
            assert_eq!(values[1], 2.6);
            assert_eq!(values[2], 2.6);
            assert_eq!(values[3], 1.);
        }

        {
            let tr = group_transform.transform(1).unwrap();

            let ff = typed!(tr, FixedFunction);

            assert_eq!(ff.direction(), TransformDirection::Forward);
            assert_eq!(ff.style(), FixedFunctionStyle::RgbToHsv);
        }
    }
}

/// Port of `OCIO_ADD_TEST(MixingColorSpaceManager, color_picker_role)` @ v2.5.2.
#[test]
fn mixing_color_space_manager_color_picker_role() {
    let config = category_config();

    let mut mixing_helper = MixingColorSpaceManager::new(Arc::clone(&config)).unwrap();
    assert_eq!(mixing_helper.num_mixing_spaces(), 2);

    // Add a color_picking role.
    let mut cfg = (*config).clone();
    assert!(!cfg.has_role(ROLE_COLOR_PICKING));
    cfg.set_role(ROLE_COLOR_PICKING, Some(&b"log_1"[..]))
        .unwrap();

    // The config changes so refresh the templates.
    mixing_helper.refresh(Arc::new(cfg)).unwrap();
    assert_eq!(mixing_helper.num_mixing_spaces(), 1);
    assert_eq!(
        mixing_helper.mixing_space_ui_name(0).unwrap(),
        b"color_picking (log_1)"
    );

    {
        let processor = mixing_helper
            .processor("lin_1", "DISP_1", "VIEW_1", TransformDirection::Forward)
            .unwrap();

        let group_transform = group_of(&processor);
        assert_eq!(group_transform.num_transforms(), 1);

        {
            let tr = group_transform.transform(0).unwrap();

            let log = typed!(tr, Log);

            assert_eq!(log.direction(), TransformDirection::Forward);
            assert_eq!(log.base(), 2.);
        }
    }

    mixing_helper.set_selected_mixing_encoding_idx(1).unwrap(); // i.e. HSV

    {
        let processor = mixing_helper
            .processor("lin_1", "DISP_1", "VIEW_1", TransformDirection::Forward)
            .unwrap();

        let group_transform = group_of(&processor);
        assert_eq!(group_transform.num_transforms(), 2);

        {
            let tr = group_transform.transform(0).unwrap();

            let log = typed!(tr, Log);

            assert_eq!(log.direction(), TransformDirection::Forward);
            assert_eq!(log.base(), 2.);
        }

        {
            let tr = group_transform.transform(1).unwrap();

            let ff = typed!(tr, FixedFunction);

            assert_eq!(ff.direction(), TransformDirection::Forward);
            assert_eq!(ff.style(), FixedFunctionStyle::RgbToHsv);
        }
    }

    check_throw_what(
        mixing_helper.set_selected_mixing_space_idx(1), // i.e. Display
        "Invalid idx for the mixing space index 1 where size is 1.",
    );
}

/// Port of `OCIO_ADD_TEST(MixingSlider, basic)` @ v2.5.2.
#[test]
fn mixing_slider_basic() {
    let config = category_config();

    let mut mixing_helper = MixingColorSpaceManager::new(config).unwrap();

    mixing_helper.slider_with_range(0.0, 1.0);

    mixing_helper.set_selected_mixing_encoding_idx(1).unwrap(); // i.e. HSV
    {
        // Needs linear to perceptually linear adjustment.

        mixing_helper.set_selected_mixing_space_idx(0).unwrap(); // i.e. Rendering Space
        assert_eq!(mixing_helper.selected_mixing_space_idx(), 0);
        let slider = mixing_helper.slider();

        slider.set_slider_min_edge(0.0);
        slider.set_slider_max_edge(1.0);

        float_check_equal(0, slider.slider_min_edge());
        float_check_equal(83386, slider.slider_max_edge());

        float_check_equal(37923, slider.mixing_to_slider(0.1));
        float_check_equal(80144, slider.mixing_to_slider(0.5));

        float_check_equal(10000, slider.slider_to_mixing(0.379232));
        float_check_equal(50000, slider.slider_to_mixing(0.801448));

        slider.set_slider_min_edge(-0.2);
        slider.set_slider_max_edge(5.);

        float_check_equal(3792, slider.mixing_to_slider(-0.1));
        float_check_equal(31573, slider.mixing_to_slider(0.1));
        float_check_equal(58279, slider.mixing_to_slider(0.5));
        float_check_equal(90744, slider.mixing_to_slider(3.0));

        float_check_equal(-10000, slider.slider_to_mixing(0.037927));
        float_check_equal(10000, slider.slider_to_mixing(0.315733));
        float_check_equal(50000, slider.slider_to_mixing(0.582797));
        float_check_equal(300000, slider.slider_to_mixing(0.907444));

        // Does not need any linear to perceptually linear adjustment.

        mixing_helper.set_selected_mixing_space_idx(1).unwrap(); // i.e. Display Space
        assert_eq!(mixing_helper.selected_mixing_space_idx(), 1);
        let slider = mixing_helper.slider();

        slider.set_slider_min_edge(0.0);
        slider.set_slider_max_edge(1.0);

        float_check_equal(0, slider.slider_min_edge());
        float_check_equal(100000, slider.slider_max_edge());

        float_check_equal(10000, slider.mixing_to_slider(0.1));
        float_check_equal(50000, slider.mixing_to_slider(0.5));

        float_check_equal(37923, slider.slider_to_mixing(0.379232));
        float_check_equal(80144, slider.slider_to_mixing(0.801448));

        slider.set_slider_min_edge(-0.2);
        slider.set_slider_max_edge(5.);

        float_check_equal(0, slider.mixing_to_slider(slider.slider_min_edge()));
        float_check_equal(100000, slider.mixing_to_slider(slider.slider_max_edge()));

        float_check_equal(1923, slider.mixing_to_slider(-0.1));
        float_check_equal(5769, slider.mixing_to_slider(0.1));
        float_check_equal(13461, slider.mixing_to_slider(0.5));
        float_check_equal(61538, slider.mixing_to_slider(3.0));

        float_check_equal(-277, slider.slider_to_mixing(0.037927));
        float_check_equal(144181, slider.slider_to_mixing(0.315733));
        float_check_equal(283054, slider.slider_to_mixing(0.582797));
        float_check_equal(451870, slider.slider_to_mixing(0.907444));
    }

    mixing_helper.set_selected_mixing_encoding_idx(0).unwrap(); // i.e. RGB
    {
        // Needs linear to perceptually linear adjustment.

        mixing_helper.set_selected_mixing_space_idx(0).unwrap(); // i.e. Rendering Space
        assert_eq!(mixing_helper.selected_mixing_space_idx(), 0);
        let slider = mixing_helper.slider();

        slider.set_slider_min_edge(0.0);
        slider.set_slider_max_edge(1.0);

        float_check_equal(0, slider.slider_min_edge());
        float_check_equal(83386, slider.slider_max_edge());

        float_check_equal(37923, slider.mixing_to_slider(0.1));
        float_check_equal(80144, slider.mixing_to_slider(0.5));

        float_check_equal(10000, slider.slider_to_mixing(0.379232));
        float_check_equal(50000, slider.slider_to_mixing(0.801448));

        slider.set_slider_min_edge(-0.2);
        slider.set_slider_max_edge(5.);

        float_check_equal(3792, slider.mixing_to_slider(-0.1));
        float_check_equal(31573, slider.mixing_to_slider(0.1));
        float_check_equal(58279, slider.mixing_to_slider(0.5));
        float_check_equal(90744, slider.mixing_to_slider(3.0));

        float_check_equal(-10000, slider.slider_to_mixing(0.037927));
        float_check_equal(10000, slider.slider_to_mixing(0.315733));
        float_check_equal(50000, slider.slider_to_mixing(0.582797));
        float_check_equal(300000, slider.slider_to_mixing(0.907444));

        // Does not need any linear to perceptually linear adjustment.

        mixing_helper.set_selected_mixing_space_idx(1).unwrap(); // i.e. Display Space
        assert_eq!(mixing_helper.selected_mixing_space_idx(), 1);
        let slider = mixing_helper.slider();

        slider.set_slider_min_edge(0.0);
        slider.set_slider_max_edge(1.0);

        float_check_equal(0, slider.slider_min_edge());
        float_check_equal(100000, slider.slider_max_edge());

        float_check_equal(10000, slider.mixing_to_slider(0.1));
        float_check_equal(50000, slider.mixing_to_slider(0.5));

        float_check_equal(37923, slider.slider_to_mixing(0.379232));
        float_check_equal(80144, slider.slider_to_mixing(0.801448));

        slider.set_slider_min_edge(-0.2);
        slider.set_slider_max_edge(5.);

        float_check_equal(1923, slider.mixing_to_slider(-0.1));
        float_check_equal(5769, slider.mixing_to_slider(0.1));
        float_check_equal(13461, slider.mixing_to_slider(0.5));
        float_check_equal(61538, slider.mixing_to_slider(3.0));

        float_check_equal(-277, slider.slider_to_mixing(0.037927));
        float_check_equal(144181, slider.slider_to_mixing(0.315733));
        float_check_equal(283054, slider.slider_to_mixing(0.582797));
        float_check_equal(451870, slider.slider_to_mixing(0.907444));
    }
}

/// Port of `OCIO_ADD_TEST(MixingSlider, color_picker_role)` @ v2.5.2.
#[test]
fn mixing_slider_color_picker_role() {
    let config = category_config();

    let mut mixing_helper = MixingColorSpaceManager::new(Arc::clone(&config)).unwrap();

    // Add the color_picking role.
    let mut cfg = (*config).clone();
    assert!(!cfg.has_role(ROLE_COLOR_PICKING));
    cfg.set_role(ROLE_COLOR_PICKING, Some(&b"lin_1"[..]))
        .unwrap();

    // Refresh the templates as the config changed.
    mixing_helper.refresh(Arc::new(cfg)).unwrap();

    assert_eq!(mixing_helper.num_mixing_spaces(), 1);
    assert_eq!(
        mixing_helper.mixing_space_ui_name(0).unwrap(),
        b"color_picking (lin_1)"
    );

    check_throw_what(
        mixing_helper.set_selected_mixing_space_idx(1),
        "Invalid idx for the mixing space index 1 where size is 1.",
    );

    mixing_helper.set_selected_mixing_encoding_idx(1).unwrap(); // i.e. HSV
    mixing_helper.set_selected_mixing_space_idx(0).unwrap(); // i.e. Color Picker role

    let slider = mixing_helper.slider_with_range(0.0, 1.0);
    float_check_equal(50501, slider.mixing_to_slider(0.50501));
    float_check_equal(50501, slider.slider_to_mixing(0.50501));

    mixing_helper.set_selected_mixing_encoding_idx(0).unwrap(); // i.e. RGB
    mixing_helper.set_selected_mixing_space_idx(0).unwrap(); // i.e. Color Picker role

    let slider = mixing_helper.slider();
    float_check_equal(50501, slider.mixing_to_slider(0.50501));
    float_check_equal(50501, slider.slider_to_mixing(0.50501));
}
