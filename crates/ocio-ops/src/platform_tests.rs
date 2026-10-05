// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of `platform.rs`, with the ported tests of `tests/cpu/Platform_tests.cpp` @ v2.5.2
//! that cover what is ported so far. `tests/platform_crt.rs` checks the case-insensitive
//! comparisons against the C runtime.
//!
//! Upstream's `Strcasecmp` returns an `int` that its test compares with 0; here that is
//! `Ordering::Equal`.
//!
//! The environment tests run on the process environment, as upstream's do: what they set
//! lands in OCIO's overlay ([`ProcessEnv`]), not in the process's own environment. Upstream's
//! checks through the Windows API (`GetEnvironmentVariable`, `SetEnvironmentVariable`) read and
//! write the process's environment, which the port doesn't change; they are checked through
//! [`ProcessEnv`] instead.

use super::*;

#[test]
fn injected_environment() {
    // The provider is global: hold it against the other tests that replace it.
    let _environment = crate::unit_test_log_utils::environment_lock();
    let mut vars = BTreeMap::new();
    vars.insert("OCIO_TEST_EMPTY".to_string(), String::new());
    set_env_provider(Some(Arc::new(MapEnv::from(vars))));
    assert_eq!(getenv("OCIO_TEST_EMPTY"), Some(Vec::new()));
    assert!(is_env_present("OCIO_TEST_EMPTY"));
    assert_eq!(getenv("OCIO_TEST_MISSING"), None);
    assert_eq!(getenv(""), None);
    set_env_provider(None);
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

/// Holds the environment lock with OCIO reading the process environment.
fn process_environment() -> std::sync::MutexGuard<'static, ()> {
    let lock = crate::unit_test_log_utils::environment_lock();
    set_env_provider(None);
    lock
}

/// Port of `OCIO_ADD_TEST(Platform, envVariable)` @ v2.5.2.
#[test]
fn env_variable() {
    let _environment = process_environment();

    // Only validates the public API.
    let path = get_env_variable("PATH");
    assert!(!path.is_empty());

    set_env_variable("MY_DUMMY_ENV", Some(b"SomeValue")).expect("no error");
    let value = get_env_variable("MY_DUMMY_ENV");
    assert!(!value.is_empty());
    assert_eq!(value, b"SomeValue");

    unset_env_variable("MY_DUMMY_ENV").expect("no error");
    let value = get_env_variable("MY_DUMMY_ENV");
    assert!(value.is_empty());
}

/// Port of `OCIO_ADD_TEST(Platform, getenv)` @ v2.5.2.
#[test]
fn getenv_test() {
    let _environment = process_environment();

    assert_eq!(getenv("NotExistingEnvVariable"), None);

    let env = getenv("PATH");
    assert!(env.as_ref().is_some_and(|e| !e.is_empty()));

    // Test a not existing env. variable.
    assert!(!is_env_present("NotExistingEnvVariable"));
    assert_eq!(getenv("NotExistingEnvVariable"), None);

    // Test an existing env. variable.
    assert!(is_env_present("PATH"));
    assert!(getenv("PATH").is_some_and(|e| !e.is_empty()));

    // Create a variable and test that it's retrievable.
    setenv("MY_WINDOWS_DUMMY_ENV", "SomeValue").expect("no error");
    assert_eq!(
        ProcessEnv.var(b"MY_WINDOWS_DUMMY_ENV"),
        Some(b"SomeValue".to_vec())
    );
    unsetenv("MY_WINDOWS_DUMMY_ENV").expect("no error");
    assert_eq!(ProcessEnv.var(b"MY_WINDOWS_DUMMY_ENV"), None);
}

/// Port of `OCIO_ADD_TEST(Platform, setenv)` @ v2.5.2.
#[test]
fn setenv_test() {
    let _environment = process_environment();

    // Guard to automatically unset the env. variable.
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            unsetenv("MY_DUMMY_ENV").expect("no error");
            unsetenv("MY_WINDOWS_DUMMY_ENV").expect("no error");
        }
    }
    let _guard = Guard;

    {
        setenv("MY_DUMMY_ENV", "SomeValue").expect("no error");
        let env = getenv("MY_DUMMY_ENV").expect("set");
        assert!(!env.is_empty());
        assert_eq!(env, b"SomeValue");
        assert_eq!(env.len(), "SomeValue".len());
    }
    {
        setenv("MY_DUMMY_ENV", " ").expect("no error");
        let env = getenv("MY_DUMMY_ENV").expect("set");
        assert!(!env.is_empty());
        assert_eq!(env, b" ");
        assert_eq!(env.len(), 1);
    }
    {
        unsetenv("MY_DUMMY_ENV").expect("no error");
        assert_eq!(getenv("MY_DUMMY_ENV"), None);
    }
}

/// Port of `OCIO_ADD_TEST(Platform, utf8_utf16_convert)` @ v2.5.2.
#[test]
fn utf8_utf16_convert() {
    if cfg!(windows) {
        // Define the same string in both UTF-8 and UTF-16LE encoding:
        // - Hiragana letter KO:        xe3, x81, x93       x3053
        // - Hiragana letter N:         xe3, x82, x93       x3093
        // - Hiragana letter NI:        xe3, x81, xab       x306b
        // - Hiragana letter CHI:       xe3, x81, xa1       x3061
        // - Hiragana letter HA/WA:     xe3, x81, xaf       x306f
        let utf8_str = b"\xe3\x81\x93\xe3\x82\x93\xe3\x81\xab\xe3\x81\xa1\xe3\x81\xaf";
        let utf16_str: [u16; 5] = [0x3053, 0x3093, 0x306b, 0x3061, 0x306f];

        // Convert each string to the other encoding and assert that the result matches the
        // other.
        let utf16_to_utf8 = utf16_to_utf8(&utf16_str).expect("Windows");
        let utf8_to_utf16 = utf8_to_utf16(utf8_str).expect("Windows");

        assert_eq!(utf16_to_utf8, utf8_str);
        assert_eq!(utf8_to_utf16, utf16_str);
    }
}

/// Elsewhere than Windows, the conversions throw for a non-empty string, with upstream's text
/// (Platform.cpp:304, 320 @ v2.5.2), and give an empty string for an empty one.
#[cfg(not(windows))]
#[test]
fn utf_conversions_are_windows_only() {
    for result in [
        utf8_to_utf16(b"a").map(|_| ()),
        utf16_to_utf8(&[0x61]).map(|_| ()),
    ] {
        assert_eq!(
            result.unwrap_err().what(),
            b"Only supported by the Windows platform."
        );
    }
    assert_eq!(utf8_to_utf16(b""), Ok(Vec::new()));
    assert_eq!(utf16_to_utf8(&[]), Ok(Vec::new()));
}

/// Port of `OCIO_ADD_TEST(Platform, create_temp_filename)` @ v2.5.2.
#[test]
fn create_temp_filename_test() {
    const TEST_MAX: usize = 20;

    let mut uids = std::collections::BTreeSet::new();
    for _ in 0..TEST_MAX {
        uids.insert(create_temp_filename(b""));
    }

    // Check that it only generates unique random strings.
    assert_eq!(uids.len(), TEST_MAX);
}
