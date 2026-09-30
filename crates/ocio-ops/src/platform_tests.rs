// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of `platform.rs`, with the ported tests of `tests/cpu/Platform_tests.cpp` @ v2.5.2
//! that cover what is ported so far. `tests/platform_crt.rs` checks the case-insensitive
//! comparisons against the C runtime.
//!
//! Upstream's `Strcasecmp` returns an `int` that its test compares with 0; here that is
//! `Ordering::Equal`.

use super::*;

#[test]
fn injected_environment() {
    // The provider is global: hold it against the other tests that replace it.
    let _environment = crate::unit_test_log_utils::environment_lock();
    let mut vars = BTreeMap::new();
    vars.insert("OCIO_TEST_EMPTY".to_string(), String::new());
    set_env_provider(Some(Arc::new(MapEnv(vars))));
    assert_eq!(getenv("OCIO_TEST_EMPTY"), Some(String::new()));
    assert!(is_env_present("OCIO_TEST_EMPTY"));
    assert_eq!(getenv("OCIO_TEST_MISSING"), None);
    assert_eq!(getenv(""), None);
    set_env_provider(None);
}

#[test]
fn case_insensitive_compare() {
    assert_eq!(strcasecmp("ProcessList", "processlist"), Ordering::Equal);
    assert_eq!(strcasecmp("a", "B"), Ordering::Less);
    assert_eq!(strncasecmp("Info", "INFORMATION", 4), Ordering::Equal);
}

/// Port of `OCIO_ADD_TEST(Platform, string_compare)` @ v2.5.2.
#[test]
fn string_compare() {
    assert_eq!(strcasecmp("TtOoPp", "TtOoPp"), Ordering::Equal);
    assert_eq!(strcasecmp("TtOoPp", "ttOoPp"), Ordering::Equal);
    assert_ne!(strcasecmp("TtOoPp", "tOoPp"), Ordering::Equal);
    assert_ne!(strcasecmp("TtOoPp", "TtOoPp1"), Ordering::Equal);

    assert_eq!(strncasecmp("TtOoPp", "TtOoPp", 2), Ordering::Equal);
    assert_eq!(strncasecmp("TtOoPp", "ttOoPp", 2), Ordering::Equal);
    assert_eq!(strncasecmp("TtOoPp", "ttOOOO", 2), Ordering::Equal);
    assert_ne!(strcasecmp("TtOoPp", "tOoPp"), Ordering::Equal);
    assert_ne!(strcasecmp("TtOoPp", "TOoPp"), Ordering::Equal);
}
