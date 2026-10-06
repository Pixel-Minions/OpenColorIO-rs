// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of the tests of `tests/cpu/FileRules_tests.cpp` @ v2.5.2 that read no YAML. Those
//! that read or write configs come with the YAML reader (3.3k) and writer (3.7b);
//! `crates/ocio/tests/file_rules_oracle.rs` checks the rules against the wheel.

use ocio_testkit::upstream::check_throw_what;

use super::*;
use crate::test_env::EnvGuard;

/// Port of `OCIO_ADD_TEST(FileRules, config_read_only)` @ v2.5.2.
#[test]
fn config_read_only() {
    let _env = EnvGuard::new();
    let config = Config::create_raw().unwrap();
    let file_rules = config.file_rules();
    assert_eq!(file_rules.num_entries(), 1);
    assert_eq!(
        file_rules.name(0).unwrap(),
        FileRules::DEFAULT_RULE_NAME.as_bytes()
    );
    assert_eq!(
        file_rules
            .index_for_rule(FileRules::DEFAULT_RULE_NAME)
            .unwrap(),
        0
    );
    assert_eq!(file_rules.pattern(0).unwrap(), b"");
    assert_eq!(file_rules.extension(0).unwrap(), b"");
    assert_eq!(file_rules.regex(0).unwrap(), b"");
    assert_eq!(file_rules.color_space(0).unwrap(), ROLE_DEFAULT.as_bytes());

    check_throw_what(
        file_rules.name(1),
        "rule index '1' invalid. There are only '1' rules.",
    );

    check_throw_what(
        file_rules.index_for_rule("toto"),
        "rule name 'toto' not found",
    );
}

/// Port of `OCIO_ADD_TEST(FileRules, config_insert_rule)` @ v2.5.2.
#[test]
fn config_insert_rule() {
    let _env = EnvGuard::new();
    let config_raw = Config::create_raw().unwrap();
    let config = (*config_raw).clone();
    let fr = config.file_rules();
    let mut file_rules = fr.clone();
    assert_eq!(file_rules.num_entries(), 1);
    file_rules.insert_rule(0, "rule", "raw", "*", "a").unwrap();
    assert_eq!(file_rules.num_entries(), 2);
    file_rules
        .insert_regex_rule(0, "TIFF rule", "raw", r".*\.TIF?F$")
        .unwrap();
    assert_eq!(file_rules.num_entries(), 3);
    check_throw_what(
        file_rules.insert_rule(0, "rule", "raw", "*", "b"),
        "A rule named 'rule' already exists",
    );
    check_throw_what(
        file_rules.insert_rule(4, "rule2", "raw", "*", "a"),
        "rule index '4' invalid",
    );
    check_throw_what(file_rules.remove_rule(3), "invalid");
    check_throw_what(file_rules.remove_rule(2), "is the default rule");
    file_rules.remove_rule(1).unwrap();
    file_rules.remove_rule(0).unwrap();
    assert_eq!(file_rules.num_entries(), 1);

    check_throw_what(
        file_rules.insert_rule(
            0,
            FileRules::FILE_PATH_SEARCH_RULE_NAME,
            "colorspace",
            "",
            "",
        ),
        "does not accept any color space",
    );
    check_throw_what(
        file_rules.insert_rule(0, FileRules::FILE_PATH_SEARCH_RULE_NAME, "", "pattern", ""),
        "do not accept any pattern",
    );
    check_throw_what(
        file_rules.insert_rule(
            0,
            FileRules::FILE_PATH_SEARCH_RULE_NAME,
            "",
            "",
            "extension",
        ),
        "do not accept any extension",
    );

    file_rules
        .insert_rule(0, FileRules::FILE_PATH_SEARCH_RULE_NAME, "", "", "")
        .unwrap();

    check_throw_what(
        file_rules.insert_rule(
            0,
            FileRules::FILE_PATH_SEARCH_RULE_NAME,
            "",
            "",
            "extension",
        ),
        "File rules: A rule named 'ColorSpaceNamePathSearch' already exists.",
    );

    check_throw_what(
        file_rules.insert_rule(0, "default", "", "", "extension"),
        "File rules: A rule named 'default' already exists.",
    );

    check_throw_what(
        file_rules.insert_rule(0, "defauLT", "", "", "extension"),
        "File rules: A rule named 'defauLT' already exists.",
    );

    check_throw_what(
        file_rules.insert_rule(0, "   Default   ", "", "", "extension"),
        "File rules: A rule named 'Default' already exists.",
    );

    file_rules.remove_rule(0).unwrap();
    file_rules.insert_path_search_rule(0).unwrap();

    // Adds a rule with empty name (upstream's null pointer).
    check_throw_what(
        file_rules.insert_rule(0, "", "raw", "*", "a"),
        "rule should have a non-empty name",
    );

    // Pattern and extension can't be null.
    check_throw_what(
        file_rules.insert_rule(0, "rule", "raw", "", "a"),
        "file name pattern is empty",
    );
    check_throw_what(
        file_rules.insert_rule(0, "rule", "raw", "*", ""),
        "file extension pattern is empty",
    );

    // Invalid glob pattern.
    check_throw_what(
        file_rules.insert_rule(0, "rule", "raw", "[", "a"),
        "invalid regular expression",
    );

    // Invalid regex (`\b` is a backspace in the C++ string).
    check_throw_what(
        file_rules.insert_regex_rule(0, "rule", "raw", "(.*)(\x08what"),
        "invalid regular expression",
    );
}

/// Port of `OCIO_ADD_TEST(FileRules, pattern_error)` @ v2.5.2.
#[test]
fn pattern_error() {
    let _env = EnvGuard::new();
    let config_raw = Config::create_raw().unwrap();
    let fr = config_raw.file_rules();
    let mut rules = fr.clone();

    rules
        .insert_rule(0, FileRules::FILE_PATH_SEARCH_RULE_NAME, "", "", "")
        .unwrap();
    rules.insert_rule(0, "new rule", "raw", "*", "a").unwrap();
    assert_eq!(rules.num_entries(), 3);

    // Upstream's null pointer and empty string.
    check_throw_what(rules.set_pattern(0, ""), "file name pattern is empty");
    check_throw_what(rules.set_pattern(0, ""), "file name pattern is empty");

    check_throw_what(rules.set_pattern(0, "[]"), "invalid regular expression");
    check_throw_what(rules.set_pattern(0, "[!]"), "invalid regular expression");
    check_throw_what(rules.set_pattern(0, "[a-b"), "invalid regular expression");
    check_throw_what(rules.set_pattern(0, "[a-b]]"), "invalid regular expression");
    check_throw_what(
        rules.set_pattern(0, "[[a-b]]"),
        "invalid regular expression",
    );
    check_throw_what(rules.set_pattern(0, "[*]"), "invalid regular expression");
}

/// Port of `OCIO_ADD_TEST(FileRules, with_defaults)` @ v2.5.2.
#[test]
fn with_defaults() {
    let _env = EnvGuard::new();
    // Validate some default behaviours.

    let config = (*Config::create_raw().unwrap()).clone();
    let mut rules = config.file_rules().clone();

    rules
        .insert_rule(0, FileRules::FILE_PATH_SEARCH_RULE_NAME, "", "", "")
        .unwrap();
    assert_eq!(rules.num_entries(), 2);

    // Null or empty pattern and/or extension throw.

    assert!(rules.insert_rule(0, "new rule2", "raw", "", "a").is_err());
    assert!(rules.insert_rule(0, "new rule2", "raw", "", "a").is_err());

    assert!(rules.insert_rule(0, "new rule3", "raw", "a", "").is_err());
    assert!(rules.insert_rule(0, "new rule3", "raw", "a", "").is_err());

    // Null or empty regex throws.

    assert!(rules.insert_regex_rule(0, "new rule2", "raw", "").is_err());
    assert!(rules.insert_regex_rule(0, "new rule2", "raw", "").is_err());
}

/// Port of `OCIO_ADD_TEST(FileRules, extension_error)` @ v2.5.2.
#[test]
fn extension_error() {
    let _env = EnvGuard::new();
    let config_raw = Config::create_raw().unwrap();
    let fr = config_raw.file_rules();
    let mut rules = fr.clone();

    rules
        .insert_rule(0, FileRules::FILE_PATH_SEARCH_RULE_NAME, "", "", "")
        .unwrap();
    rules.insert_rule(0, "new rule", "raw", "*", "a").unwrap();
    assert_eq!(rules.num_entries(), 3);

    check_throw_what(
        rules.set_extension(0, ""),
        "file extension pattern is empty",
    );
    check_throw_what(
        rules.set_extension(0, ""),
        "file extension pattern is empty",
    );
}

/// Port of `OCIO_ADD_TEST(FileRules, clone)` @ v2.5.2.
#[test]
fn clone() {
    let _env = EnvGuard::new();
    // Validate that 'FileRules::createEditableCopy()' does not share FileRule instances.

    let config = (*Config::create_raw().unwrap()).clone();
    let mut file_rules = config.file_rules().clone();
    file_rules
        .insert_rule(0, FileRules::FILE_PATH_SEARCH_RULE_NAME, "", "", "")
        .unwrap();
    file_rules.insert_rule(0, "rule", "raw", "*", "a").unwrap();
    assert_eq!(file_rules.num_entries(), 3);

    let mut new_file_rules = file_rules.clone();
    assert_eq!(new_file_rules.num_entries(), 3);

    assert_eq!(
        new_file_rules.pattern(0).unwrap(),
        file_rules.pattern(0).unwrap()
    );

    new_file_rules.set_pattern(0, "*A").unwrap();
    assert_ne!(
        new_file_rules.pattern(0).unwrap(),
        file_rules.pattern(0).unwrap()
    );

    file_rules.set_pattern(0, "*B").unwrap();
    assert_ne!(
        new_file_rules.pattern(0).unwrap(),
        file_rules.pattern(0).unwrap()
    );

    new_file_rules.set_pattern(0, "*").unwrap();
    file_rules.set_pattern(0, "*").unwrap();
    assert_eq!(
        new_file_rules.pattern(0).unwrap(),
        file_rules.pattern(0).unwrap()
    );
}

/// Port of `OCIO_ADD_TEST(FileRules, isDefault)` @ v2.5.2.
#[test]
fn is_default() {
    let _env = EnvGuard::new();
    let mut file_rules = FileRules::new();
    assert!(file_rules.is_default());
    file_rules.set_color_space(0, "DEFault").unwrap();
    assert!(file_rules.is_default());
    file_rules.set_color_space(0, "raw").unwrap();
    assert!(!file_rules.is_default());
    file_rules.set_color_space(0, "default").unwrap();
    assert!(file_rules.is_default());

    file_rules.set_custom_key(0, "key", "val").unwrap();
    assert!(!file_rules.is_default());
    file_rules.set_custom_key(0, "key", "").unwrap();
    assert!(file_rules.is_default());

    file_rules.insert_rule(0, "rule", "raw", "*", "a").unwrap();
    assert!(!file_rules.is_default());

    let config = Config::create_raw().unwrap();
    assert!(config.file_rules().is_default());
}
