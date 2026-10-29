// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! All four Config-backed tests from ColorSpaceSet_tests.cpp @ v2.5.2.
//! Literal assertions and call order follow upstream; no new API or oracle data.

use ocio::{ColorSpace, ColorSpaceSet, Config};

fn space(name: &str, categories: &[&str]) -> ColorSpace {
    let mut cs = ColorSpace::new();
    cs.set_name(name);
    for category in categories {
        cs.add_category(category);
    }
    cs
}

/// Port of `OCIO_ADD_TEST(ColorSpaceSet, basic)` @ v2.5.2.
#[test]
fn basic() {
    let mut config = Config::new().unwrap();
    // The existing Rust API represents the C++ null/empty category with "".
    let mut css1 = config.color_spaces("");
    assert_eq!(css1.num_color_spaces(), 0);
    let cs1 = space("cs1", &[]);
    assert!(!cs1.has_category("linear"));
    assert!(!cs1.has_category("rendering"));
    assert!(!cs1.has_category("log"));
    let mut cs2 = space("cs2", &["linear", "rendering"]);
    assert!(cs2.has_category("linear"));
    assert!(cs2.has_category("rendering"));
    assert!(!cs2.has_category("log"));
    cs2.add_category("log");
    assert!(cs2.has_category("log"));
    cs2.remove_category("log");
    assert!(!cs2.has_category("log"));
    config.add_color_space(&cs1).unwrap();
    config.add_color_space(&cs2).unwrap();
    css1 = config.color_spaces("");
    assert_eq!(css1.num_color_spaces(), 2);
    assert_eq!(config.num_color_spaces(), 2);
    css1 = config.color_spaces("");
    assert_eq!(css1.num_color_spaces(), 2);
    css1 = config.color_spaces("log");
    assert_eq!(css1.num_color_spaces(), 0);
    css1 = config.color_spaces("linear");
    assert_eq!(css1.num_color_spaces(), 1);
    assert_eq!(css1.color_space_name_by_index(0), Some(b"cs2".as_slice()));
    css1 = config.color_spaces("LinEar");
    assert_eq!(css1.num_color_spaces(), 1);
    assert_eq!(css1.color_space_name_by_index(0), Some(b"cs2".as_slice()));
    css1 = config.color_spaces(" LinEar ");
    assert_eq!(css1.num_color_spaces(), 1);
    assert_eq!(css1.color_space_name_by_index(0), Some(b"cs2".as_slice()));
    assert_eq!(css1.color_space_by_index(0).unwrap().name(), b"cs2");
    for request in ["lin ear", "[linear]", "linear log", "linearlog"] {
        css1 = config.color_spaces(request);
        assert_eq!(css1.num_color_spaces(), 0);
    }
    css1 = config.color_spaces("linear");
    assert_eq!(css1.num_color_spaces(), 1);
    config.clear_color_spaces();
    assert_eq!(config.num_color_spaces(), 0);
    assert_eq!(css1.num_color_spaces(), 1);
    let css2 = config.color_spaces("");
    assert_eq!(css2.num_color_spaces(), 0);
}

/// Port of `OCIO_ADD_TEST(ColorSpaceSet, decoupled_sets)` @ v2.5.2.
#[test]
fn decoupled_sets() {
    let mut config = Config::new().unwrap();
    let mut cs1 = space("cs1", &["linear"]);
    assert!(cs1.has_category("linear"));
    config.add_color_space(&cs1).unwrap();
    let css1 = config.color_spaces("");
    assert_eq!(css1.num_color_spaces(), 1);
    assert_eq!(css1.color_space_name_by_index(0), Some(b"cs1".as_slice()));
    let css2 = config.color_spaces("linear");
    assert_eq!(css2.num_color_spaces(), 1);
    assert_eq!(css2.color_space_name_by_index(0), Some(b"cs1".as_slice()));
    cs1.set_name("new_cs1");
    assert_eq!(config.color_space_name_by_index(0), b"cs1");
    assert_eq!(css1.num_color_spaces(), 1);
    assert_eq!(css1.color_space_name_by_index(0), Some(b"cs1".as_slice()));
    assert_eq!(css2.num_color_spaces(), 1);
    assert_eq!(css2.color_space_name_by_index(0), Some(b"cs1".as_slice()));
    assert!(!cs1.is_data());
    config.clear_color_spaces();
    config.add_color_space(&cs1).unwrap();
    cs1.set_is_data(true);
    assert_eq!(cs1.name(), b"new_cs1");
    assert!(cs1.is_data());
    assert_eq!(config.color_space_name_by_index(0), b"new_cs1");
    assert!(!config.color_space("new_cs1").unwrap().is_data());
    assert_eq!(css1.num_color_spaces(), 1);
    assert_eq!(css1.color_space_name_by_index(0), Some(b"cs1".as_slice()));
    assert!(!css1.color_space("cs1").unwrap().is_data());
    assert_eq!(css2.num_color_spaces(), 1);
    assert_eq!(css2.color_space_name_by_index(0), Some(b"cs1".as_slice()));
    assert!(!css2.color_space("cs1").unwrap().is_data());
}

/// Port of `OCIO_ADD_TEST(ColorSpaceSet, order_validation)` @ v2.5.2.
#[test]
fn order_validation() {
    let mut config = Config::new().unwrap();
    let mut css1 = config.color_spaces("");
    assert_eq!(css1.num_color_spaces(), 0);
    let cs1 = space("cs1", &["linear", "rendering"]);
    let cs2 = space("cs2", &["rendering", "linear"]);
    let cs3 = space("cs3", &["rendering"]);
    config.add_color_space(&cs1).unwrap();
    config.add_color_space(&cs2).unwrap();
    config.add_color_space(&cs3).unwrap();
    css1 = config.color_spaces("linear");
    assert_eq!(css1.num_color_spaces(), 2);
    assert_eq!(css1.color_space_name_by_index(0), Some(b"cs1".as_slice()));
    assert_eq!(css1.color_space_name_by_index(1), Some(b"cs2".as_slice()));
    css1 = config.color_spaces("rendering");
    assert_eq!(css1.num_color_spaces(), 3);
    assert_eq!(css1.color_space_name_by_index(0), Some(b"cs1".as_slice()));
    assert_eq!(css1.color_space_name_by_index(1), Some(b"cs2".as_slice()));
    assert_eq!(css1.color_space_name_by_index(2), Some(b"cs3".as_slice()));
}

/// Port of `OCIO_ADD_TEST(ColorSpaceSet, operations_on_set)` @ v2.5.2.
#[test]
fn operations_on_set() {
    let mut config = Config::new().unwrap();
    let cs1 = space("cs1", &[]);
    config.add_color_space(&cs1).unwrap();
    let cs2 = space("cs2", &["linear", "rendering"]);
    config.add_color_space(&cs2).unwrap();
    let cs3 = space("cs3", &["log", "rendering"]);
    config.add_color_space(&cs3).unwrap();
    let css1 = config.color_spaces("");
    assert_eq!(css1.num_color_spaces(), 3);
    let css2 = config.color_spaces("linear");
    assert_eq!(css2.num_color_spaces(), 1);
    assert_eq!(css2.color_space_name_by_index(0), Some(b"cs2".as_slice()));
    let css3 = config.color_spaces("log");
    assert_eq!(css3.num_color_spaces(), 1);
    assert_eq!(css3.color_space_name_by_index(0), Some(b"cs3".as_slice()));
    let mut css4 = ColorSpaceSet::union(&css2, &css3).unwrap();
    assert_eq!(css4.num_color_spaces(), 2);
    css4 = ColorSpaceSet::union(&css1, &css2).unwrap();
    assert_eq!(css4.num_color_spaces(), 3);
    css4 = ColorSpaceSet::intersection(&css2, &css3).unwrap();
    assert_eq!(css4.num_color_spaces(), 0);
    css4 = ColorSpaceSet::intersection(&css2, &css1).unwrap();
    assert_eq!(css4.num_color_spaces(), 1);
    assert_eq!(css4.color_space_name_by_index(0), Some(b"cs2".as_slice()));
    assert_eq!(css4.color_space_by_index(0).unwrap().name(), b"cs2");
    css4 = ColorSpaceSet::difference(&css1, &css3).unwrap();
    assert_eq!(css4.num_color_spaces(), 2);
    assert_eq!(css4.color_space_name_by_index(0), Some(b"cs1".as_slice()));
    assert_eq!(css4.color_space_name_by_index(1), Some(b"cs2".as_slice()));
    css4 = ColorSpaceSet::difference(&css1, &css2).unwrap();
    assert_eq!(css4.num_color_spaces(), 2);
    assert_eq!(css4.color_space_name_by_index(0), Some(b"cs1".as_slice()));
    assert_eq!(css4.color_space_name_by_index(1), Some(b"cs3".as_slice()));
    css4 = ColorSpaceSet::difference(&css1, &ColorSpaceSet::union(&css2, &css3).unwrap()).unwrap();
    assert_eq!(css4.num_color_spaces(), 1);
    assert_eq!(css4.color_space_name_by_index(0), Some(b"cs1".as_slice()));
    let mut css5 = config.color_spaces("rendering");
    assert_eq!(css5.num_color_spaces(), 2);
    css5.add_color_space(&cs1).unwrap();
    assert_eq!(css5.num_color_spaces(), 3);
    css5.remove_color_space("cs2");
    css5.remove_color_space("cs1");
    assert_eq!(css5.num_color_spaces(), 1);
    assert_eq!(css5.color_space_name_by_index(0), Some(b"cs3".as_slice()));
    css5.clear_color_spaces();
    assert_eq!(css5.num_color_spaces(), 0);
    css5 = config.color_spaces("rendering");
    assert_eq!(css5.num_color_spaces(), 2);
    assert_eq!(css5.color_space_name_by_index(0), Some(b"cs2".as_slice()));
    assert_eq!(css5.color_space_name_by_index(1), Some(b"cs3".as_slice()));
    css4 = ColorSpaceSet::intersection(
        &ColorSpaceSet::difference(&css1, &css5).unwrap(),
        &ColorSpaceSet::union(&css2, &css3).unwrap(),
    )
    .unwrap();
    assert_eq!(css4.num_color_spaces(), 0);
}
