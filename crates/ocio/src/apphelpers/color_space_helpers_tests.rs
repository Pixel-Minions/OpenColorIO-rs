// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/apphelpers/ColorSpaceHelpers_tests.cpp` @ v2.5.2.

use super::*;
use crate::apphelpers::configs_data::CATEGORY_TEST_CONFIG;

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
