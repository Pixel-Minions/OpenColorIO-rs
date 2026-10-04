// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the config's version. Upstream's `Config version` and `Config version_validation`
//! (tests/cpu/Config_tests.cpp @ v2.5.2) need configs read from YAML (Phase 3); their checks of
//! `setMajorVersion` are here, with upstream's expected messages.

use std::sync::Arc;

use ocio_testkit::upstream::check_throw_what;

use super::*;

/// `setMajorVersion` takes 1 and 2 and refuses other versions (Config_tests.cpp:2065-2067,
/// 2104-2107 @ v2.5.2); the minor version becomes the last one each major supports, which
/// upstream's tests read back as the serialized profile version (Config_tests.cpp:2069-2088).
#[test]
fn set_major_version() {
    let mut config = Config::create_raw();
    let config = Arc::get_mut(&mut config).unwrap();

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
