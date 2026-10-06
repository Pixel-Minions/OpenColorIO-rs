// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the config. Upstream's `Config version` and `Config family_separator`
//! (tests/cpu/Config_tests.cpp @ v2.5.2) also read and write configs as YAML (Phase 3); their
//! checks of the API alone are here, with upstream's expected messages, without the markers.

use ocio_testkit::upstream::check_throw_what;

use super::*;
use crate::test_env::EnvGuard;

/// `setMajorVersion` takes 1 and 2 and refuses other versions (Config_tests.cpp:2065-2067,
/// 2104-2107 @ v2.5.2); the minor version becomes the last one each major supports, which
/// upstream's tests read back as the serialized profile version (Config_tests.cpp:2069-2088).
#[test]
fn set_major_version() {
    let _env = EnvGuard::new();
    let mut config = Config::new().unwrap();

    config.set_major_version(1).unwrap();
    assert_eq!(config.major_version(), 1);
    check_throw_what(
        config.set_major_version(20000),
        "version is 20000 where supported versions start at 1 and end at 2",
    );
    assert_eq!(config.major_version(), 1);
    check_throw_what(
        config.set_major_version(3),
        "version is 3 where supported versions start at 1 and end at 2",
    );
    check_throw_what(
        config.set_major_version(0),
        "where supported versions start at 1 and end at 2",
    );

    config.set_major_version(2).unwrap();
    assert_eq!(config.major_version(), 2);
}

/// `Config version`'s checks of `setMinorVersion` and `setVersion` (Config_tests.cpp:2065-2107
/// @ v2.5.2), on a new config instead of one read from YAML.
#[test]
fn version_without_yaml() {
    let _env = EnvGuard::new();
    let mut config = Config::new().unwrap();

    config.set_major_version(1).unwrap();
    check_throw_what(
        config.set_major_version(20000),
        "version is 20000 where supported versions start at 1 and end at 2",
    );

    check_throw_what(
        config.set_minor_version(1),
        "The minor version 1 is not supported for major version 1. Maximum minor version is 0",
    );

    config.set_minor_version(0).unwrap();
    config.set_major_version(2).unwrap();

    check_throw_what(
        config.set_version(2, 9),
        "The minor version 9 is not supported for major version 2. Maximum minor version is 5",
    );

    config.set_major_version(2).unwrap();
    check_throw_what(
        config.set_minor_version(9),
        "The minor version 9 is not supported for major version 2. Maximum minor version is 5",
    );

    check_throw_what(
        config.set_version(3, 4),
        "version is 3 where supported versions start at 1 and end at 2",
    );
}

/// `Config family_separator`'s checks of the API (Config_tests.cpp:7486-7500 @ v2.5.2), on a
/// copy of the raw config.
#[test]
fn family_separator_without_yaml() {
    let _env = EnvGuard::new();
    let mut cfg = (*Config::create_raw().unwrap()).clone();

    assert_eq!(cfg.family_separator(), b'/');

    cfg.set_family_separator(b' ').unwrap();
    assert_eq!(cfg.family_separator(), b' ');

    cfg.set_family_separator(0).unwrap();
    assert_eq!(cfg.family_separator(), 0);

    // Reset to its default value.
    assert_eq!(Config::default_family_separator(), b'/');
    cfg.set_family_separator(Config::default_family_separator())
        .unwrap();
    assert_eq!(cfg.family_separator(), b'/');

    assert!(cfg.set_family_separator(127).is_err());
    assert!(cfg.set_family_separator(31).is_err());
}

/// `Config alias_validation`'s checks of `addColorSpace` (Config_tests.cpp:9415-9438 @ v2.5.2),
/// without its `validate()` calls (3.8a) and its named transforms (3.4j).
#[test]
fn alias_validation_of_color_spaces() {
    let _env = EnvGuard::new();
    // NB: This tests ColorSpaceSet::addColorSpace.

    let mut cfg = (*Config::create_raw().unwrap()).clone();
    let mut cs = ColorSpace::new();
    cs.set_name("colorspace1");
    cfg.add_color_space(&cs).unwrap();
    cs.set_name("colorspace2");
    cfg.add_color_space(&cs).unwrap();
    cs.set_name("colorspace3");
    cs.add_alias("colorspace1");
    check_throw_what(
        cfg.add_color_space(&cs),
        "Cannot add 'colorspace3' color space, it has 'colorspace1' alias and existing color \
         space, 'colorspace1' is using the same alias",
    );
    cs.remove_alias("colorspace1");

    cfg.set_role("alias", Some(b"colorspace2")).unwrap();
    cs.add_alias("alias");
    check_throw_what(
        cfg.add_color_space(&cs),
        "Cannot add 'colorspace3' color space, it has an alias 'alias' and there is already a \
         role with this name",
    );
    cs.remove_alias("alias");
    cs.add_alias("test%test");
    check_throw_what(
        cfg.add_color_space(&cs),
        "Cannot add 'colorspace3' color space, it has an alias 'test%test' that cannot contain \
         a context variable reserved token i.e. % or $",
    );

    cs.remove_alias("test%test");
    cs.add_alias("namedtransform");
    cfg.add_color_space(&cs).unwrap();
}
