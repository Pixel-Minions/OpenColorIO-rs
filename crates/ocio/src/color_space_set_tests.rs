// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the color space set. Upstream's (`tests/cpu/ColorSpaceSet_tests.cpp` @ v2.5.2)
//! build their sets with `Config::getColorSpaces`, so they come with the config's color spaces
//! (WP 3.4f). The sets are compared with the wheel's in `tests/model_objects_oracle.rs`.

use super::*;
use ocio_testkit::upstream::check_throw_what;

fn named(name: &str, aliases: &[&str]) -> ColorSpace {
    let mut cs = ColorSpace::new();
    cs.set_name(name);
    for alias in aliases {
        cs.add_alias(alias);
    }
    cs
}

fn names(css: &ColorSpaceSet) -> Vec<&[u8]> {
    (0..css.num_color_spaces())
        .map(|i| css.color_space_name_by_index(i).unwrap())
        .collect()
}

/// Lookups by name or alias ignore case; out of range is `None` or -1; a set holds copies.
#[test]
fn lookups_and_copies() {
    let mut cs1 = named("cs1", &["one", "First"]);
    let mut css = ColorSpaceSet::new();
    css.add_color_space(&cs1).unwrap();
    css.add_color_space(&named("cs2", &[])).unwrap();
    cs1.set_name("changed");

    assert_eq!(names(&css), [&b"cs1"[..], b"cs2"]);
    assert_eq!(css.color_space_index("CS2"), 1);
    assert_eq!(css.color_space_index("first"), 0);
    assert_eq!(css.color_space_index(""), -1);
    assert_eq!(css.color_space_index("changed"), -1);
    assert_eq!(css.color_space("ONE").unwrap().name(), b"cs1");
    assert!(css.color_space_by_index(2).is_none());
    assert!(css.color_space_name_by_index(-1).is_none());
    assert!(css.has_color_space("cs1\0junk"));
}

/// The refusals of `add_color_space`, with upstream's messages; a color space of the same name
/// replaces the existing one, in place.
#[test]
fn add_refuses_conflicts_and_replaces_by_name() {
    let mut css = ColorSpaceSet::new();
    check_throw_what(
        css.add_color_space(&ColorSpace::new()),
        "Cannot add a color space with an empty name.",
    );
    css.add_color_space(&named("cs1", &["alias1"])).unwrap();
    css.add_color_space(&named("cs2", &[])).unwrap();
    check_throw_what(
        css.add_color_space(&named("ALIAS1", &[])),
        "Cannot add 'ALIAS1' color space, existing color space, 'cs1' is using this name as \
         an alias.",
    );
    check_throw_what(
        css.add_color_space(&named("cs3", &["CS2"])),
        "Cannot add 'cs3' color space, it has 'CS2' alias and existing color space, 'cs2' is \
         using the same alias.",
    );
    let mut replacement = named("CS1", &["alias1", "other"]);
    replacement.set_is_data(true);
    css.add_color_space(&replacement).unwrap();
    assert!(css.color_space("other").unwrap().is_data());
}

/// Removal goes by name only; equality compares sizes and names. The orders the set operations
/// give are checked against the wheel in `tests/model_objects_oracle.rs`.
#[test]
fn remove_and_equality() {
    let mut css1 = ColorSpaceSet::new();
    for cs in [named("cs1", &["a1"]), named("cs2", &[]), named("cs3", &[])] {
        css1.add_color_space(&cs).unwrap();
    }
    let mut css2 = ColorSpaceSet::new();
    css2.add_color_space(&named("cs3", &[])).unwrap();
    css2.add_color_space(&named("cs2", &[])).unwrap();

    let mut copy = css1.clone();
    assert!(copy == css1);
    copy.remove_color_space("a1");
    assert_eq!(copy.num_color_spaces(), 3);
    copy.remove_color_space("CS1");
    assert!(copy != css1);
    copy.remove_color_spaces(&css2);
    assert_eq!(copy.num_color_spaces(), 0);
    copy.add_color_spaces(&css2).unwrap();
    assert_eq!(copy.num_color_spaces(), 2);
    copy.clear_color_spaces();
    assert_eq!(copy.num_color_spaces(), 0);
}
