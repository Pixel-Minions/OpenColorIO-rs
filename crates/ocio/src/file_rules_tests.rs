// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of the tests of `tests/cpu/FileRules_tests.cpp` @ v2.5.2: those that read no YAML,
//! those that read configs (3.3k), those that write them (3.7b), and those that validate them
//! (3.8);
//! `crates/ocio/tests/file_rules_oracle.rs` checks the rules against the wheel.

use ocio_testkit::upstream::check_throw_what;

use super::*;
use crate::color_space::ColorSpace;
use crate::test_env::{
    EnvGuard, check_and_mute_aces_interchange_role_error, check_and_mute_color_timing_role_error,
    check_and_mute_compositing_log_role_error, check_and_mute_scene_linear_role_error,
};
use ocio_ops::utils::string_utils::split_by_lines;

/// Port of `OCIO_ADD_TEST(FileRules, config_read_only)` @ v2.5.2.
#[test]
fn config_read_only() {
    let _env = EnvGuard::new();
    let config = Config::create_raw().unwrap();
    let file_rules = config.file_rules().get();
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
    let fr = config.file_rules().get();
    let mut file_rules = (*fr).clone();
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
    let fr = config_raw.file_rules().get();
    let mut rules = (*fr).clone();

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
    let mut rules = (*config.file_rules().get()).clone();

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
    let fr = config_raw.file_rules().get();
    let mut rules = (*fr).clone();

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
    let mut file_rules = (*config.file_rules().get()).clone();
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
    assert!(config.file_rules().get().is_default());
}

// The tests that read configs (3.3k).

/// Upstream's `g_config` (`FileRules_tests.cpp:466-486`).
const G_CONFIG: &str = r#"ocio_profile_version: 2
environment:
  {}
strictparsing: true
roles:
  default: raw
  role1: cs1
  role2: cs2
displays:
  sRGB:
  - !<View> {name: Raw, colorspace: raw}
colorspaces:
  - !<ColorSpace>
      name: raw
  - !<ColorSpace>
      name: cs1
  - !<ColorSpace>
      name: cs2
  - !<ColorSpace>
      name: other_cs1
"#;

const G_NAME: &str = "rule1";

/// An editable copy of the config of [`G_CONFIG`].
fn g_config() -> Config {
    (*Config::create_from_stream(G_CONFIG.as_bytes()).unwrap()).clone()
}

/// The index of the rule that matches `path` (upstream's `rulePosition`).
fn rule_position(config: &Config, path: &str) -> usize {
    config.color_space_from_filepath_with_index(path).unwrap().1
}

/// The color space of the rule that matches `path`, and the rule's index.
fn color_space_and_position(config: &Config, path: &str) -> (Vec<u8>, usize) {
    config.color_space_from_filepath_with_index(path).unwrap()
}

/// Port of `OCIO_ADD_TEST(FileRules, rules_filepattern)` @ v2.5.2.
#[test]
fn rules_filepattern() {
    let _env = EnvGuard::new();
    let mut config = g_config();
    let mut rules = (*config.file_rules().get()).clone();

    // Add pattern + extension rule.
    rules
        .insert_rule(0, G_NAME, "cs1", "*", "[eE][xX][r]")
        .unwrap();
    config.set_file_rules(&rules);

    assert_eq!(rule_position(&config, "/An/Arbitrary/Path/MyFile.exr"), 0);
    assert_eq!(rule_position(&config, "/An/Arbitrary/Path/MyFile.eXr"), 0);
    // Default rule. R must be lower case.
    assert_eq!(rule_position(&config, "/An/Arbitrary/Path/MyFile.EXR"), 1);
    assert_eq!(rule_position(&config, "/An/Arbitrary/Path/MyFileexr"), 1); // Default rule.
    assert_eq!(rule_position(&config, "/An/Arbitrary/Path/MyFile.jpeg"), 1); // Default rule.
    assert_eq!(
        rule_position(&config, "/An/Arbitrary.exr/Path/MyFileexr"),
        1
    ); // Default rule.
    assert_eq!(rule_position(&config, ""), 1); // Default rule.
    // Upstream's null pointer, which the config reads as "".
    assert_eq!(rule_position(&config, ""), 1); // Default rule.

    rules.set_pattern(0, "gamma").unwrap();
    config.set_file_rules(&rules);
    assert_eq!(
        rule_position(&config, "/An/gamma/Arbitrary/Path/MyFile.exr"),
        1
    ); // Default rule.

    rules.set_pattern(0, "*gamma").unwrap();
    config.set_file_rules(&rules);
    assert_eq!(
        rule_position(&config, "/An/gamma/Arbitrary/Path/MyFile.exr"),
        1
    ); // Default rule.

    rules.set_pattern(0, "gamma*").unwrap();
    config.set_file_rules(&rules);
    assert_eq!(
        rule_position(&config, "/An/gamma/Arbitrary/Path/MyFile.exr"),
        1
    ); // Default rule.

    rules.set_pattern(0, "*gamma*").unwrap();
    config.set_file_rules(&rules);
    assert_eq!(rule_position(&config, "/An/Arbitrary/Path/MyFile.exr"), 1); // Default rule.
    assert_eq!(
        rule_position(&config, "/An/GaMma/Arbitrary/Path/MyFile.exr"),
        1
    ); // Default rule.
    assert_eq!(
        rule_position(&config, "/An/gamma/Arbitrary/Path/MyFile.exr"),
        0
    );
    assert_eq!(
        rule_position(&config, "/An/gammaArbitrary/Path/MyFile.exr"),
        0
    );

    rules.set_pattern(0, "*ga?ma*").unwrap();
    config.set_file_rules(&rules);
    assert_eq!(rule_position(&config, "/An/Arbitrary/Path/MyFile.exr"), 1); // Default rule.
    assert_eq!(
        rule_position(&config, "/An/GaMma/Arbitrary/Path/MyFile.exr"),
        1
    ); // Default rule.
    assert_eq!(
        rule_position(&config, "/An/gamma/Arbitrary/Path/MyFile.exr"),
        0
    );
    assert_eq!(
        rule_position(&config, "/An/gammaArbitrary/Path/MyFile.exr"),
        0
    );
    assert_eq!(
        rule_position(&config, "/An/gatmaArbitrary/Path/MyFile.exr"),
        0
    );
    assert_eq!(
        rule_position(&config, "/An/gatmaArbitrary/Path/MyFile.exr"),
        0
    );
    // Default rule.
    assert_eq!(
        rule_position(&config, "/An/gatttttttmaArbitrary/Path/MyFile.exr"),
        1
    );
    assert_eq!(
        rule_position(&config, "/An/gamaArbitrary/Path/MyFile.exr"),
        1
    ); // Default rule.

    rules.set_pattern(0, "*ga*ma*").unwrap();
    config.set_file_rules(&rules);
    assert_eq!(rule_position(&config, "/An/Arbitrary/Path/MyFile.exr"), 1); // Default rule.
    assert_eq!(
        rule_position(&config, "/An/GaMma/Arbitrary/Path/MyFile.exr"),
        1
    ); // Default rule.
    assert_eq!(
        rule_position(&config, "/An/gamma/Arbitrary/Path/MyFile.exr"),
        0
    );
    assert_eq!(
        rule_position(&config, "/An/gammaArbitrary/Path/MyFile.exr"),
        0
    );
    assert_eq!(
        rule_position(&config, "/An/gatmaArbitrary/Path/MyFile.exr"),
        0
    );
    assert_eq!(
        rule_position(&config, "/An/gatmaArbitrary/Path/MyFile.exr"),
        0
    );
    assert_eq!(
        rule_position(&config, "/An/gatttttttmaArbitrary/Path/MyFile.exr"),
        0
    );
    assert_eq!(
        rule_position(&config, "/An/gamaArbitrary/Path/MyFile.exr"),
        0
    );

    rules.set_pattern(0, "*g?mm*").unwrap();
    config.set_file_rules(&rules);
    assert_eq!(
        rule_position(&config, "/An/gamma/Arbitrary/Path/MyFile.exr"),
        0
    );
    assert_eq!(
        rule_position(&config, "/An/gImma/Arbitrary/Path/MyFile.exr"),
        0
    );
    assert_eq!(
        rule_position(&config, "/An/gImmaaa/Arbitrary/Path/MyFile.exr"),
        0
    );

    rules.set_pattern(0, "*g*mm*").unwrap();
    config.set_file_rules(&rules);
    assert_eq!(
        rule_position(&config, "/An/gamma/Arbitrary/Path/MyFile.exr"),
        0
    );
    assert_eq!(
        rule_position(&config, "/An/gImma/Arbitrary/Path/MyFile.exr"),
        0
    );
    assert_eq!(
        rule_position(&config, "/An/gIIImmaaa/Arbitrary/Path/MyFile.exr"),
        0
    );
    assert_eq!(
        rule_position(&config, "/An/gmm/Arbitrary/Path/MyFile.exr"),
        0
    );

    rules.set_pattern(0, "*g?m?a*").unwrap();
    config.set_file_rules(&rules);
    assert_eq!(
        rule_position(&config, "/An/gamma/Arbitrary/Path/MyFile.exr"),
        0
    );
    assert_eq!(
        rule_position(&config, "/An/gImma/Arbitrary/Path/MyFile.exr"),
        0
    );
    assert_eq!(
        rule_position(&config, "/An/gImIa/Arbitrary/Path/MyFile.exr"),
        0
    );
    assert_eq!(
        rule_position(&config, "/An/gma/Arbitrary/Path/MyFile.exr"),
        1
    ); // Default rule.
    // Default rule.
    assert_eq!(
        rule_position(&config, "/An/gIIImmaaa/Arbitrary/Path/MyFile.exr"),
        1
    );

    rules.set_pattern(0, "*g[a]mma*").unwrap();
    config.set_file_rules(&rules);
    assert_eq!(
        rule_position(&config, "/An/gmma/Arbitrary/Path/MyFile.exr"),
        1
    ); // Default rule.
    assert_eq!(
        rule_position(&config, "/An/gamma/Arbitrary/Path/MyFile.exr"),
        0
    );
    assert_eq!(
        rule_position(&config, "/An/gbmma/Arbitrary/Path/MyFile.exr"),
        1
    ); // Default rule.

    rules.set_pattern(0, "*g[!a]mma*").unwrap();
    config.set_file_rules(&rules);
    assert_eq!(
        rule_position(&config, "/An/gmma/Arbitrary/Path/MyFile.exr"),
        1
    ); // Default rule.
    assert_eq!(
        rule_position(&config, "/An/gamma/Arbitrary/Path/MyFile.exr"),
        1
    ); // Default rule.
    assert_eq!(
        rule_position(&config, "/An/gbmma/Arbitrary/Path/MyFile.exr"),
        0
    );

    rules.set_pattern(0, "*g[abcd]mma*").unwrap();
    config.set_file_rules(&rules);
    assert_eq!(
        rule_position(&config, "/An/gamma/Arbitrary/Path/MyFile.exr"),
        0
    );
    assert_eq!(
        rule_position(&config, "/An/gbmma/Arbitrary/Path/MyFile.exr"),
        0
    );
    assert_eq!(
        rule_position(&config, "/An/gcmma/Arbitrary/Path/MyFile.exr"),
        0
    );
    assert_eq!(
        rule_position(&config, "/An/gdmma/Arbitrary/Path/MyFile.exr"),
        0
    );
    assert_eq!(
        rule_position(&config, "/An/gmma/Arbitrary/Path/MyFile.exr"),
        1
    ); // Default rule.
    assert_eq!(
        rule_position(&config, "/An/gemma/Arbitrary/Path/MyFile.exr"),
        1
    ); // Default rule.
    assert_eq!(
        rule_position(&config, "/An/gabmma/Arbitrary/Path/MyFile.exr"),
        1
    ); // Default rule.

    rules.set_pattern(0, "*g[!abcd]mma*").unwrap();
    config.set_file_rules(&rules);
    assert_eq!(
        rule_position(&config, "/An/gamma/Arbitrary/Path/MyFile.exr"),
        1
    ); // Default rule.
    assert_eq!(
        rule_position(&config, "/An/gbmma/Arbitrary/Path/MyFile.exr"),
        1
    ); // Default rule.
    assert_eq!(
        rule_position(&config, "/An/gcmma/Arbitrary/Path/MyFile.exr"),
        1
    ); // Default rule.
    assert_eq!(
        rule_position(&config, "/An/gdmma/Arbitrary/Path/MyFile.exr"),
        1
    ); // Default rule.
    assert_eq!(
        rule_position(&config, "/An/gmma/Arbitrary/Path/MyFile.exr"),
        1
    ); // Default rule.
    assert_eq!(
        rule_position(&config, "/An/gemma/Arbitrary/Path/MyFile.exr"),
        0
    );
    assert_eq!(
        rule_position(&config, "/An/gabmma/Arbitrary/Path/MyFile.exr"),
        1
    ); // Default rule.
    assert_eq!(
        rule_position(&config, "/An/gefmma/Arbitrary/Path/MyFile.exr"),
        1
    ); // Default rule.

    rules.set_pattern(0, "*g[a-d]mma*").unwrap();
    config.set_file_rules(&rules);
    assert_eq!(
        rule_position(&config, "/An/gamma/Arbitrary/Path/MyFile.exr"),
        0
    );
    assert_eq!(
        rule_position(&config, "/An/gbmma/Arbitrary/Path/MyFile.exr"),
        0
    );
    assert_eq!(
        rule_position(&config, "/An/gcmma/Arbitrary/Path/MyFile.exr"),
        0
    );
    assert_eq!(
        rule_position(&config, "/An/gdmma/Arbitrary/Path/MyFile.exr"),
        0
    );
    assert_eq!(
        rule_position(&config, "/An/gmma/Arbitrary/Path/MyFile.exr"),
        1
    ); // Default rule.
    assert_eq!(
        rule_position(&config, "/An/gemma/Arbitrary/Path/MyFile.exr"),
        1
    ); // Default rule.
    assert_eq!(
        rule_position(&config, "/An/gabmma/Arbitrary/Path/MyFile.exr"),
        1
    ); // Default rule.
    assert_eq!(
        rule_position(&config, "/An/gefmma/Arbitrary/Path/MyFile.exr"),
        1
    ); // Default rule.

    rules.set_pattern(0, "g[!a-d]mma*").unwrap();
    config.set_file_rules(&rules);
    assert_eq!(rule_position(&config, "gamma/Arbitrary/Path/MyFile.exr"), 1); // Default rule.
    assert_eq!(rule_position(&config, "gbmma/Arbitrary/Path/MyFile.exr"), 1); // Default rule.
    assert_eq!(rule_position(&config, "gcmma/Arbitrary/Path/MyFile.exr"), 1); // Default rule.
    assert_eq!(rule_position(&config, "gdmma/Arbitrary/Path/MyFile.exr"), 1); // Default rule.
    assert_eq!(rule_position(&config, "gmma/Arbitrary/Path/MyFile.exr"), 1); // Default rule.
    assert_eq!(rule_position(&config, "gemma/Arbitrary/Path/MyFile.exr"), 0);
    assert_eq!(
        rule_position(&config, "gabmma/Arbitrary/Path/MyFile.exr"),
        1
    ); // Default rule.
    assert_eq!(
        rule_position(&config, "gefmma/Arbitrary/Path/MyFile.exr"),
        1
    ); // Default rule.

    rules.set_pattern(0, r"g[!a-d][\*][e-g]mma").unwrap();
    config.set_file_rules(&rules);
    assert_eq!(rule_position(&config, "ge*fmma.exr"), 0);

    // Add pattern + extension rule.
    rules.insert_rule(0, "rule0", "cs1", "*", "jpg").unwrap();
    config.set_file_rules(&rules);
    assert_eq!(rule_position(&config, "test.jpg"), 0);
    assert_eq!(rule_position(&config, "test.Jpg"), 0);
    assert_eq!(rule_position(&config, "test.jpG"), 0);
    assert_eq!(rule_position(&config, "test.Jpeg"), 2); // Default rule.

    rules.set_extension(0, "jp[gG]").unwrap();
    config.set_file_rules(&rules);
    assert_eq!(rule_position(&config, "test.jpg"), 0);
    assert_eq!(rule_position(&config, "test.jpG"), 0);
    assert_eq!(rule_position(&config, "test.Jpg"), 2); // Default rule.

    rules.set_extension(0, "[Jj]pg").unwrap();
    config.set_file_rules(&rules);
    assert_eq!(rule_position(&config, "/mnt/media/image.Jpg"), 0);

    rules.set_extension(0, "?pg").unwrap();
    config.set_file_rules(&rules);
    assert_eq!(rule_position(&config, "/mnt/media/image.Jpg"), 0);

    rules.set_extension(0, "jpg").unwrap();
    config.set_file_rules(&rules);
    assert_eq!(rule_position(&config, "/mnt/media/image.Jpg"), 0);

    rules.set_extension(0, "JPG").unwrap();
    config.set_file_rules(&rules);
    assert_eq!(rule_position(&config, "/mnt/media/image.Jpg"), 0);

    rules.set_extension(0, "jp[gG]").unwrap();
    config.set_file_rules(&rules);
    assert_eq!(rule_position(&config, "/mnt/media/image.Jpg"), 2);

    rules.set_extension(0, "?PG").unwrap();
    config.set_file_rules(&rules);
    assert_eq!(rule_position(&config, "/mnt/media/image.Jpg"), 2);

    rules.set_extension(0, "jP*").unwrap();
    config.set_file_rules(&rules);
    assert_eq!(rule_position(&config, "/mnt/media/image.Jpg"), 2);

    rules.set_extension(0, "*g").unwrap();
    config.set_file_rules(&rules);
    assert_eq!(rule_position(&config, "/mnt/media/image.Jpg"), 0);

    rules.set_pattern(0, "*[^]*").unwrap();
    config.set_file_rules(&rules);
    assert_eq!(rule_position(&config, "/mnt/me^ia/image.Jpg"), 0);

    assert_eq!(rule_position(&config, "/mnt/media/image.Jpg"), 2);

    rules.set_pattern(0, "*(name)*").unwrap();
    config.set_file_rules(&rules);
    assert_eq!(rule_position(&config, "/mnt/(name)/image.Jpg"), 0);
}

/// Port of `OCIO_ADD_TEST(FileRules, rules_regex)` @ v2.5.2.
#[test]
fn rules_regex() {
    let _env = EnvGuard::new();
    let mut config = g_config();
    let mut rules = (*config.file_rules().get()).clone();

    // Add pattern + extension rule.
    rules
        .insert_regex_rule(0, G_NAME, "cs1", r"(.*)(\bmine\b|\byours\b)(.*)")
        .unwrap();
    config.set_file_rules(&rules);

    assert_eq!(rule_position(&config, r"mnt/mine/media/image.jpg"), 0);

    assert_eq!(rule_position(&config, r"mnt/miner/media/image.jpg"), 1);

    assert_eq!(rule_position(&config, r"yours/mnt/media/image.jpg"), 0);

    assert_eq!(rule_position(&config, r"mnt\media\yours\image.jpg"), 0);

    assert_eq!(rule_position(&config, r"mine/media/image.jpg"), 0);

    // The error details may be different on each platform.
    check_throw_what(
        rules.insert_regex_rule(1, "invalid", "cs1", r"(.*)(\bmine\b|\byours\b(.*)"),
        "invalid regular expression",
    );
}

/// Port of `OCIO_ADD_TEST(FileRules, rules_long_filepattern)` @ v2.5.2.
#[test]
fn rules_long_filepattern() {
    let _env = EnvGuard::new();
    let mut config = g_config();
    let mut rules = (*config.file_rules().get()).clone();

    // Add pattern + extension rule.
    rules.insert_rule(0, G_NAME, "cs1", "*", "exr").unwrap();
    config.set_file_rules(&rules);

    // The file path existence is not tested
    const ARBITRARY_PATH: &str = concat!(
        "/Users/hodoulp/Documents/work/Color Management/ocio-images",
        ".1.0v4/spi-vfx/marci_512_srgb.exr"
    );

    assert_eq!(rule_position(&config, ARBITRARY_PATH), 0);

    rules.set_pattern(0, "*Col?r*").unwrap();
    config.set_file_rules(&rules);
    assert_eq!(rule_position(&config, ARBITRARY_PATH), 0);

    rules
        .set_pattern(
            0,
            concat!(
                "*****************************************************",
                "*******"
            ),
        )
        .unwrap();
    config.set_file_rules(&rules);
    assert_eq!(rule_position(&config, ARBITRARY_PATH), 0);

    rules.set_pattern(0, "*?").unwrap();
    config.set_file_rules(&rules);
    assert_eq!(rule_position(&config, ARBITRARY_PATH), 0);

    rules.set_pattern(0, "?*").unwrap();
    config.set_file_rules(&rules);
    assert_eq!(rule_position(&config, ARBITRARY_PATH), 0);

    rules.set_pattern(0, "?*").unwrap();
    config.set_file_rules(&rules);
    assert_eq!(rule_position(&config, ARBITRARY_PATH), 0);

    rules.set_pattern(0, "*?*").unwrap();
    config.set_file_rules(&rules);
    assert_eq!(rule_position(&config, ARBITRARY_PATH), 0);

    rules.set_pattern(0, "*.1.0v4*").unwrap();
    config.set_file_rules(&rules);
    assert_eq!(rule_position(&config, ARBITRARY_PATH), 0);

    rules.set_pattern(0, "*.1.*").unwrap();
    config.set_file_rules(&rules);
    assert_eq!(rule_position(&config, ARBITRARY_PATH), 0);
}

/// Port of `OCIO_ADD_TEST(FileRules, rules_test)` @ v2.5.2.
#[test]
fn rules_test() {
    let _env = EnvGuard::new();
    let mut config = g_config();
    let mut rules = (*config.file_rules().get()).clone();
    rules.insert_path_search_rule(0).unwrap();
    rules.insert_rule(1, "dpx file", "raw", "*", "dpx").unwrap();
    config.set_file_rules(&rules);

    let (color_space, rule_pos) = color_space_and_position(&config, "/mnt/user/show/img_cs1.dpx");
    assert_eq!(rule_pos, 0);
    assert_eq!(color_space, b"cs1");

    let (color_space, rule_pos) = color_space_and_position(&config, "show/cs2/img_cs1.exr");
    assert_eq!(rule_pos, 0);
    // The first color space name from the right.
    assert_eq!(color_space, b"cs1");

    let (color_space, rule_pos) = color_space_and_position(&config, "show/cs1/img_other_cs1.exr");
    assert_eq!(rule_pos, 0);
    // If there are 2 cs names ending the same position, the longest is used.
    assert_eq!(color_space, b"other_cs1");

    let (color_space, rule_pos) = color_space_and_position(&config, "show/other_cs1/img_cs1.exr");
    assert_eq!(rule_pos, 0);
    assert_eq!(color_space, b"cs1");

    let (color_space, rule_pos) = color_space_and_position(&config, "/mnt/user/unknown.dpx");
    assert_eq!(rule_pos, 1);
    assert_eq!(color_space, b"raw");

    let (color_space, rule_pos) = color_space_and_position(&config, "/mnt/user/unknown.jpg");
    assert_eq!(rule_pos, 2); // The default rule.
    assert_eq!(color_space, ROLE_DEFAULT.as_bytes());

    // Note that parseColorSpaceFromString (used by the pathSearch rule) is tested with aliases
    // and inactive color spaces in OCIO_ADD_TEST(Config, use_alias).
}

/// Port of `OCIO_ADD_TEST(FileRules, rules_priority)` @ v2.5.2.
#[test]
fn rules_priority() {
    let _env = EnvGuard::new();
    let mut config = g_config();
    let mut rules = (*config.file_rules().get()).clone();
    rules
        .insert_rule(0, "pattern dpx file", "raw", "*cs2*", "dpx")
        .unwrap();
    rules.insert_path_search_rule(1).unwrap();
    rules
        .insert_regex_rule(2, "regex rule", "cs5", ".*cs5.dpx")
        .unwrap();
    config.set_file_rules(&rules);

    let (color_space, rule_pos) = color_space_and_position(&config, "/mnt/media/cs2.dpx");
    assert_eq!(rule_pos, 0);
    assert_eq!(color_space, b"raw");

    let (color_space, rule_pos) = color_space_and_position(&config, "/mnt/media/cs2.exr");
    assert_eq!(rule_pos, 1);
    assert_eq!(color_space, b"cs2");

    let (color_space, rule_pos) = color_space_and_position(&config, "/mnt/media/cs5.dpx");
    assert_eq!(rule_pos, 2);
    assert_eq!(color_space, b"cs5");

    let (color_space, rule_pos) = color_space_and_position(&config, "/mnt/media/cs5.DPX");
    assert_eq!(rule_pos, 3);
    assert_eq!(color_space, ROLE_DEFAULT.as_bytes());
}

/// Port of `OCIO_ADD_TEST(FileRules, config_no_default)` @ v2.5.2.
#[test]
fn config_no_default() {
    let _env = EnvGuard::new();
    const CONFIG_NO_DEFAULT: &str = r#"ocio_profile_version: 2
strictparsing: true
roles:
  role1: cs1
  role2: cs2
displays:
  sRGB:
  - !<View> {name: Raw, colorspace: raw}
colorspaces:
  - !<ColorSpace>
      name: raw
  - !<ColorSpace>
      name: cs1
  - !<ColorSpace>
      name: cs2
"#;

    check_throw_what(
        Config::create_from_stream(CONFIG_NO_DEFAULT.as_bytes()),
        "must contain either a Default file rule or the 'default' role",
    );
}

/// Port of `OCIO_ADD_TEST(FileRules, config_default_missmatch)` @ v2.5.2.
#[test]
fn config_default_missmatch() {
    let _env = EnvGuard::new();
    const CONFIG_DEFAULT_MISSMATCH: &str = r#"ocio_profile_version: 2
environment:
  {}
strictparsing: true
roles:
  default: raw
  role1: cs1
  role2: cs2
displays:
  sRGB:
  - !<View> {name: Raw, colorspace: raw}
colorspaces:
  - !<ColorSpace>
      name: raw
  - !<ColorSpace>
      name: cs1
  - !<ColorSpace>
      name: cs2
file_rules:
  - !<Rule> {name: Default, colorspace: cs1}
"#;

    // As a warning message is expected, please mute it.
    let (config, log) = crate::test_env::capture_log(|| {
        Config::create_from_stream(CONFIG_DEFAULT_MISSMATCH.as_bytes())
    });
    let config = (*config.unwrap()).clone();

    let output = String::from_utf8(log.concat()).unwrap();
    assert!(output.contains("that does not match the default role"));

    let rules = config.file_rules().get();
    assert_eq!(rules.num_entries(), 1);
    assert_eq!(
        rules.name(0).unwrap(),
        FileRules::DEFAULT_RULE_NAME.as_bytes()
    );
    assert_eq!(rules.color_space(0).unwrap(), b"cs1");
    assert_eq!(
        config.color_space_from_filepath("anything").unwrap(),
        b"cs1"
    );

    // The color space of the default role is preserved.
    let cs = config.color_space(ROLE_DEFAULT).unwrap();
    assert_eq!(cs.name(), b"raw");
}

/// Port of `OCIO_ADD_TEST(FileRules, config_default_no_colorspace)` @ v2.5.2.
#[test]
fn config_default_no_colorspace() {
    let _env = EnvGuard::new();
    const CONFIG_DEFAULT_MISSMATCH: &str = r#"ocio_profile_version: 2
strictparsing: true
roles:
  default: raw
  role1: cs1
  role2: cs2
displays:
  sRGB:
  - !<View> {name: Raw, colorspace: raw}
colorspaces:
  - !<ColorSpace>
      name: raw
  - !<ColorSpace>
      name: cs1
  - !<ColorSpace>
      name: cs2
file_rules:
  - !<Rule> {name: Default}
"#;

    check_throw_what(
        Config::create_from_stream(CONFIG_DEFAULT_MISSMATCH.as_bytes()),
        "'Default' rule cannot have an empty color space name",
    );
}

/// Port of `OCIO_ADD_TEST(FileRules, config_no_default_rule)` @ v2.5.2.
#[test]
fn config_no_default_rule() {
    let _env = EnvGuard::new();
    const CONFIG_NO_DEFAULT_RULE: &str = r#"ocio_profile_version: 2
strictparsing: true
roles:
  default: raw
  role1: cs1
  role2: cs2
displays:
  sRGB:
  - !<View> {name: Raw, colorspace: raw}
colorspaces:
  - !<ColorSpace>
      name: raw
  - !<ColorSpace>
      name: cs1
  - !<ColorSpace>
      name: cs2
file_rules:
  - !<Rule> {name: Custom, pattern: "*", extension: jpg, colorspace: cs1}
"#;

    check_throw_what(
        Config::create_from_stream(CONFIG_NO_DEFAULT_RULE.as_bytes()),
        "'file_rules' does not contain a Default <Rule>",
    );
}

/// Port of `OCIO_ADD_TEST(FileRules, config_filerule_no_colorspace)` @ v2.5.2.
#[test]
fn config_filerule_no_colorspace() {
    let _env = EnvGuard::new();
    const CONFIG_NO_DEFAULT_RULE: &str = r#"ocio_profile_version: 2
strictparsing: true
roles:
  default: raw
  role1: cs1
  role2: cs2
displays:
  sRGB:
  - !<View> {name: Raw, colorspace: raw}
colorspaces:
  - !<ColorSpace>
      name: raw
  - !<ColorSpace>
      name: cs1
  - !<ColorSpace>
      name: cs2
file_rules:
  - !<Rule> {name: Custom, pattern: "*", extension: jpg}
  - !<Rule> {name: Default, colorspace: default}
"#;

    check_throw_what(
        Config::create_from_stream(CONFIG_NO_DEFAULT_RULE.as_bytes()),
        "File rule 'Custom' cannot have an empty color space name",
    );
}

/// Port of `OCIO_ADD_TEST(FileRules, config_v1_faulty)` @ v2.5.2.
#[test]
fn config_v1_faulty() {
    let _env = EnvGuard::new();
    const CONFIG_V1: &str = r#"ocio_profile_version: 1
strictparsing: true
roles:
  default: raw
  role1: cs1
  role2: cs2
displays:
  sRGB:
  - !<View> {name: Raw, colorspace: raw}
colorspaces:
  - !<ColorSpace>
      name: raw
  - !<ColorSpace>
      name: cs1
  - !<ColorSpace>
      name: cs2
file_rules:
  - !<Rule> {name: Default, colorspace: default}
"#;

    check_throw_what(
        Config::create_from_stream(CONFIG_V1.as_bytes()),
        "Config v1 can't use 'file_rules'",
    );
}

/// Port of `OCIO_ADD_TEST(FileRules, config_v2_wrong_rule)` @ v2.5.2.
#[test]
fn config_v2_wrong_rule() {
    let _env = EnvGuard::new();
    // 2 default rules.
    {
        let config_v2 = G_CONFIG.to_string()
            + r#"file_rules:
  - !<Rule> {name: Default, colorspace: default}
  - !<Rule> {name: Default, colorspace: cs1}
"#;
        check_throw_what(
            Config::create_from_stream(config_v2.as_bytes()),
            "Default rule has to be the last rule",
        );
    }
    // Default rule parameters.
    {
        let config_v2 = G_CONFIG.to_string()
            + r#"file_rules:
  - !<Rule> {name: Default, colorspace: cs2, regex: ".*\\.TIF?F$"}
"#;
        check_throw_what(
            Config::create_from_stream(config_v2.as_bytes()),
            "'Default' rule can't use pattern, extension or regex.",
        );
    }
    // Default should be at the end.
    {
        let config_v2 = G_CONFIG.to_string()
            + r#"file_rules:
  - !<Rule> {name: Default, colorspace: raw}
  - !<Rule> {name: Custom, colorspace: cs1, pattern: "*", extension: jpg}
"#;
        check_throw_what(
            Config::create_from_stream(config_v2.as_bytes()),
            "Default rule has to be the last rule",
        );
    }
    // 2 parse rules.
    {
        let config_v2 = G_CONFIG.to_string()
            + r#"file_rules:
  - !<Rule> {name: ColorSpaceNamePathSearch}
  - !<Rule> {name: ColorSpaceNamePathSearch}
  - !<Rule> {name: Default, colorspace: cs1}
"#;
        check_throw_what(
            Config::create_from_stream(config_v2.as_bytes()),
            "A rule named 'ColorSpaceNamePathSearch' already exists",
        );
    }
    // Rule with regex & glob.
    {
        let config_v2 = G_CONFIG.to_string()
            + r#"file_rules:
  - !<Rule> {name: Custom, colorspace: cs1, pattern: "*", extension: jpg, regex: ".*\\.TIF?F$"}
  - !<Rule> {name: Default, colorspace: cs1}
"#;
        check_throw_what(
            Config::create_from_stream(config_v2.as_bytes()),
            r"can't use regex '.*\.TIF?F$' and pattern & extension",
        );
    }
}

/// Port of `OCIO_ADD_TEST(FileRules, config_rule_customkeys)` @ v2.5.2.
#[test]
fn config_rule_customkeys() {
    let _env = EnvGuard::new();
    let config_raw = Config::create_raw().unwrap();
    let fr = config_raw.file_rules().get();
    let mut file_rules = (*fr).clone();
    assert_eq!(file_rules.num_entries(), 1);
    file_rules.insert_rule(0, "rule", "raw", "*", "a").unwrap();
    assert_eq!(file_rules.num_entries(), 2);
    assert_eq!(file_rules.num_custom_keys(0).unwrap(), 0);
    assert_eq!(file_rules.num_custom_keys(1).unwrap(), 0);
    check_throw_what(file_rules.num_custom_keys(2), "rule index '2' invalid");
    check_throw_what(file_rules.custom_key_name(0, 0), "Key index '0' is invalid");
    check_throw_what(file_rules.custom_key_name(1, 0), "Key index '0' is invalid");
    check_throw_what(
        file_rules.custom_key_value(0, 0),
        "Key index '0' is invalid",
    );
    check_throw_what(
        file_rules.custom_key_value(1, 0),
        "Key index '0' is invalid",
    );
    file_rules.set_custom_key(0, "key", "val").unwrap();
    file_rules.set_custom_key(1, "keyDef", "valDef").unwrap();
    // Upstream's null pointer, then the empty string.
    check_throw_what(
        file_rules.set_custom_key(0, "", "val"),
        "Key has to be a non-empty string",
    );
    check_throw_what(
        file_rules.set_custom_key(0, "", "val"),
        "Key has to be a non-empty string",
    );
    assert_eq!(file_rules.num_custom_keys(0).unwrap(), 1);
    assert_eq!(file_rules.num_custom_keys(1).unwrap(), 1);
    assert_eq!(file_rules.custom_key_name(0, 0).unwrap(), b"key");
    assert_eq!(file_rules.custom_key_value(0, 0).unwrap(), b"val");
    file_rules.set_custom_key(0, "key", "").unwrap();
    assert_eq!(file_rules.num_custom_keys(0).unwrap(), 0);
    file_rules.set_custom_key(0, "key", "val").unwrap();
    // Upstream's null pointer.
    file_rules.set_custom_key(0, "key", "").unwrap();
    assert_eq!(file_rules.num_custom_keys(0).unwrap(), 0);
    file_rules.set_custom_key(0, "key1", "val").unwrap();
    assert_eq!(file_rules.num_custom_keys(0).unwrap(), 1);
    file_rules.set_custom_key(0, "key1", "new val").unwrap();
    assert_eq!(file_rules.num_custom_keys(0).unwrap(), 1);
    assert_eq!(file_rules.custom_key_value(0, 0).unwrap(), b"new val");
    file_rules.set_custom_key(0, "key2", "val2").unwrap();
    assert_eq!(file_rules.num_custom_keys(0).unwrap(), 2);
    file_rules.set_custom_key(0, "key3", "3").unwrap();
    assert_eq!(file_rules.num_custom_keys(0).unwrap(), 3);
    file_rules.set_custom_key(0, "4", "val4").unwrap();
    assert_eq!(file_rules.num_custom_keys(0).unwrap(), 4);
    assert_eq!(file_rules.custom_key_name(0, 1).unwrap(), b"key1");
    assert_eq!(file_rules.custom_key_value(0, 1).unwrap(), b"new val");
    assert_eq!(file_rules.custom_key_name(0, 2).unwrap(), b"key2");
    assert_eq!(file_rules.custom_key_value(0, 2).unwrap(), b"val2");
    assert_eq!(file_rules.custom_key_name(0, 3).unwrap(), b"key3");
    assert_eq!(file_rules.custom_key_value(0, 3).unwrap(), b"3");
    assert_eq!(file_rules.custom_key_name(0, 0).unwrap(), b"4");
    assert_eq!(file_rules.custom_key_value(0, 0).unwrap(), b"val4");

    let mut config = (*config_raw).clone();
    config.set_file_rules(&file_rules);

    let os = config.serialize().unwrap();

    const EXPECTED: &str = r#"ocio_profile_version: 2

environment:
  {}
search_path: ""
strictparsing: false
luma: [0.2126, 0.7152, 0.0722]

roles:
  default: raw

file_rules:
  - !<Rule> {name: rule, colorspace: raw, pattern: "*", extension: a, custom: {4: val4, key1: new val, key2: val2, key3: 3}}
  - !<Rule> {name: Default, colorspace: default, custom: {keyDef: valDef}}

displays:
  sRGB:
    - !<View> {name: Raw, colorspace: raw}

active_displays: []
active_views: []

colorspaces:
  - !<ColorSpace>
    name: raw
    family: raw
    equalitygroup: ""
    bitdepth: 32f
    description: A raw color space. Conversions to and from this space are no-ops.
    isdata: true
    allocation: uniform
"#;

    assert_eq!(EXPECTED, String::from_utf8(os).unwrap());

    // Reload.
    let config_reloaded = Config::create_from_stream(EXPECTED.as_bytes()).unwrap();
    let rules_reloaded = config_reloaded.file_rules().get();

    assert_eq!(rules_reloaded.num_entries(), 2);

    assert_eq!(rules_reloaded.num_custom_keys(0).unwrap(), 4);
    assert_eq!(rules_reloaded.custom_key_name(0, 1).unwrap(), b"key1");
    assert_eq!(rules_reloaded.custom_key_value(0, 1).unwrap(), b"new val");
    assert_eq!(rules_reloaded.custom_key_name(0, 2).unwrap(), b"key2");
    assert_eq!(rules_reloaded.custom_key_value(0, 2).unwrap(), b"val2");
    assert_eq!(rules_reloaded.custom_key_name(0, 3).unwrap(), b"key3");
    assert_eq!(rules_reloaded.custom_key_value(0, 3).unwrap(), b"3");
    assert_eq!(rules_reloaded.custom_key_name(0, 0).unwrap(), b"4");
    assert_eq!(rules_reloaded.custom_key_value(0, 0).unwrap(), b"val4");

    assert_eq!(rules_reloaded.num_custom_keys(1).unwrap(), 1);
    assert_eq!(rules_reloaded.custom_key_name(1, 0).unwrap(), b"keyDef");
    assert_eq!(rules_reloaded.custom_key_value(1, 0).unwrap(), b"valDef");
}

/// Port of `OCIO_ADD_TEST(FileRules, config_rule_u8)` @ v2.5.2. Upstream's `U8` strings are
/// written as escapes.
#[test]
fn config_rule_u8() {
    let _env = EnvGuard::new();
    const NAME: &str = "\u{e9}\u{c0}\u{c2}\u{c7}\u{c9}\u{c8}\u{e7}$\u{20ac}";
    const KEY: &str = "key\u{a3}";
    const VALUE: &str = "val\u{20ac}";
    let config_raw = Config::create_raw().unwrap();
    let fr = config_raw.file_rules().get();
    let mut file_rules = (*fr).clone();
    assert_eq!(file_rules.num_entries(), 1);
    file_rules.insert_rule(0, NAME, "raw", "*", "a").unwrap();
    assert_eq!(file_rules.num_entries(), 2);
    file_rules.set_custom_key(0, KEY, VALUE).unwrap();
    assert_eq!(file_rules.custom_key_name(0, 0).unwrap(), KEY.as_bytes());
    assert_eq!(file_rules.custom_key_value(0, 0).unwrap(), VALUE.as_bytes());

    let mut config = (*config_raw).clone();
    config.set_file_rules(&file_rules);

    let os = config.serialize().unwrap();

    // Reload.
    let config_reloaded = Config::create_from_stream(&os).unwrap();
    let rules_reloaded = config_reloaded.file_rules().get();

    assert_eq!(rules_reloaded.num_entries(), 2);

    assert_eq!(file_rules.name(0).unwrap(), NAME.as_bytes());

    assert_eq!(rules_reloaded.num_custom_keys(0).unwrap(), 1);
    assert_eq!(file_rules.custom_key_name(0, 0).unwrap(), KEY.as_bytes());
    assert_eq!(file_rules.custom_key_value(0, 0).unwrap(), VALUE.as_bytes());
}

// The tests that validate configs (3.8).

/// `config.validate()`, and what it logged (upstream's `LogGuard` around it).
fn validate_logged(config: &Config) -> (Result<()>, Vec<u8>) {
    let (result, log) = crate::test_env::capture_log(|| config.validate());
    (result, log.concat())
}

/// Upstream's check that the roles version 2.2 asks for were reported (and muted): the scene
/// linear, compositing log, color timing and ACES interchange role errors.
fn check_and_mute_role_errors(log: &mut Vec<u8>) {
    assert!(check_and_mute_scene_linear_role_error(log));
    assert!(check_and_mute_compositing_log_role_error(log));
    assert!(check_and_mute_color_timing_role_error(log));
    assert!(check_and_mute_aces_interchange_role_error(log));
}

/// Port of `OCIO_ADD_TEST(FileRules, config_v1)` @ v2.5.2.
#[test]
fn config_v1() {
    let _env = EnvGuard::new();
    // From a v1 config create valid file rules.

    {
        const CONFIG: &str = concat!(
            "ocio_profile_version: 1\n",
            "\n",
            "search_path: \"\"\n",
            "strictparsing: false\n",
            "luma: [0.2126, 0.7152, 0.0722]\n",
            "\n",
            "roles:\n",
            "  default: raw\n",
            "\n",
            "displays:\n",
            "  sRGB:\n",
            "    - !<View> {name: Raw, colorspace: raw}\n",
            "\n",
            "active_displays: []\n",
            "active_views: []\n",
            "\n",
            "colorspaces:\n",
            "  - !<ColorSpace>\n",
            "    name: raw\n",
            "    family: \"\"\n",
            "    equalitygroup: \"\"\n",
            "    bitdepth: unknown\n",
            "    isdata: false\n",
            "    allocation: uniform\n",
        );

        let config = Config::create_from_stream(CONFIG.as_bytes()).unwrap();
        config.validate().unwrap();

        let rules = config.file_rules().get();
        assert_eq!(rules.num_entries(), 2);
        assert_eq!(
            rules.name(0).unwrap(),
            FileRules::FILE_PATH_SEARCH_RULE_NAME.as_bytes()
        );
        assert_eq!(
            rules.name(1).unwrap(),
            FileRules::DEFAULT_RULE_NAME.as_bytes()
        );

        assert_eq!(rules.color_space(1).unwrap(), b"default");

        // Check that the file rules are not saved in a v1 config.
        assert_eq!(config.serialize().unwrap(), CONFIG.as_bytes());
    }

    // Test fallback 1: The default role is missing and there is a data color space named 'raw'.

    {
        const CONFIG: &str = concat!(
            "ocio_profile_version: 1\n",
            "displays:\n",
            "  sRGB:\n",
            "    - !<View> {name: Raw, colorspace: raw}\n",
            "colorspaces:\n",
            "  - !<ColorSpace>\n",
            "    name: cs2\n",
            "  - !<ColorSpace>\n",
            "    name: raw\n",
            "    isdata: true\n",
        );

        let config = Config::create_from_stream(CONFIG.as_bytes()).unwrap();
        config.validate().unwrap();

        let rules = config.file_rules().get();
        assert_eq!(rules.num_entries(), 2);
        assert_eq!(
            rules.name(0).unwrap(),
            FileRules::FILE_PATH_SEARCH_RULE_NAME.as_bytes()
        );
        assert_eq!(
            rules.name(1).unwrap(),
            FileRules::DEFAULT_RULE_NAME.as_bytes()
        );

        assert_eq!(rules.color_space(1).unwrap(), b"raw");
    }

    // Test fallback 2: The default role is missing and there is a data color space.
    // But 'raw' is not a data color space.

    {
        const CONFIG: &str = concat!(
            "ocio_profile_version: 1\n",
            "displays:\n",
            "  sRGB:\n",
            "    - !<View> {name: Raw, colorspace: raw}\n",
            "colorspaces:\n",
            "  - !<ColorSpace>\n",
            "    name: cs2\n",
            "  - !<ColorSpace>\n",
            "    name: raw\n",
            "  - !<ColorSpace>\n",
            "    name: cs3\n",
            "    isdata: true\n",
        );

        let config = Config::create_from_stream(CONFIG.as_bytes()).unwrap();
        config.validate().unwrap();

        let rules = config.file_rules().get();
        assert_eq!(rules.num_entries(), 2);
        assert_eq!(
            rules.name(0).unwrap(),
            FileRules::FILE_PATH_SEARCH_RULE_NAME.as_bytes()
        );
        assert_eq!(
            rules.name(1).unwrap(),
            FileRules::DEFAULT_RULE_NAME.as_bytes()
        );

        assert_eq!(rules.color_space(1).unwrap(), b"cs3");
    }

    // Test fallback 3: The default role is missing and there is no data color space but there is
    // an active color space.

    {
        const CONFIG: &str = concat!(
            "ocio_profile_version: 1\n",
            "displays:\n",
            "  sRGB:\n",
            "    - !<View> {name: Raw, colorspace: raw}\n",
            "colorspaces:\n",
            "  - !<ColorSpace>\n",
            "    name: cs2\n",
            "  - !<ColorSpace>\n",
            "    name: raw\n",
        );

        let config = Config::create_from_stream(CONFIG.as_bytes()).unwrap();
        config.validate().unwrap();

        let rules = config.file_rules().get();
        assert_eq!(rules.num_entries(), 2);
        assert_eq!(
            rules.name(0).unwrap(),
            FileRules::FILE_PATH_SEARCH_RULE_NAME.as_bytes()
        );
        assert_eq!(
            rules.name(1).unwrap(),
            FileRules::DEFAULT_RULE_NAME.as_bytes()
        );

        assert_eq!(rules.color_space(1).unwrap(), b"cs2");
    }

    // Test that getColorSpaceFromFilePath works even with a v1 config (that pre-dates the
    // introduction of file rules).

    {
        const CONFIG: &str = concat!(
            "ocio_profile_version: 1\n",
            "roles:\n",
            "  default: raw\n",
            "displays:\n",
            "  sRGB:\n",
            "    - !<View> {name: Raw, colorspace: raw}\n",
            "colorspaces:\n",
            "  - !<ColorSpace>\n",
            "    name: cs2\n",
            "  - !<ColorSpace>\n",
            "    name: raw\n",
            "  - !<ColorSpace>\n",
            "    name: cs3\n",
            "    isdata: true\n",
        );

        let config = Config::create_from_stream(CONFIG.as_bytes()).unwrap();
        config.validate().unwrap();

        let rules = config.file_rules().get();
        assert_eq!(rules.num_entries(), 2);
        assert_eq!(
            rules.name(0).unwrap(),
            FileRules::FILE_PATH_SEARCH_RULE_NAME.as_bytes()
        );
        assert_eq!(
            rules.name(1).unwrap(),
            FileRules::DEFAULT_RULE_NAME.as_bytes()
        );

        assert_eq!(rules.color_space(1).unwrap(), b"default");

        // Test the file path search rule i.e. implemented using Config::parseColorSpaceFromString()

        assert_eq!(
            color_space_and_position(&config, "/usr/cs2_file.exr"),
            (b"cs2".to_vec(), 0)
        );
        assert!(
            !config
                .filepath_only_matches_default_rule("/usr/cs2_file.exr")
                .unwrap()
        );

        assert_eq!(
            color_space_and_position(&config, "/usr/cs3/file.exr"),
            (b"cs3".to_vec(), 0)
        );
        assert!(
            !config
                .filepath_only_matches_default_rule("/usr/cs3/file.exr")
                .unwrap()
        );

        assert_eq!(
            color_space_and_position(&config, "/usr/cs3/cs2_file.exr"),
            (b"cs2".to_vec(), 0)
        );
        assert!(
            !config
                .filepath_only_matches_default_rule("/usr/cs3/cs2_file.exr")
                .unwrap()
        );

        // Test that it fallbacks to the default rule when nothing found.

        assert_eq!(
            color_space_and_position(&config, "/usr/file.exr"),
            (b"default".to_vec(), 1)
        );
        assert!(
            config
                .filepath_only_matches_default_rule("/usr/file.exr")
                .unwrap()
        );
    }
}

/// Port of `OCIO_ADD_TEST(FileRules, rule_invalid)` @ v2.5.2.
#[test]
fn rule_invalid() {
    let _env = EnvGuard::new();
    let mut config = g_config();

    config.validate().unwrap();
    let mut rules = (*config.file_rules().get()).clone();
    assert_eq!(rules.num_entries(), 1);

    rules.insert_rule(0, G_NAME, "cs1", "*", "exr").unwrap();
    config.set_file_rules(&rules);
    config.validate().unwrap();

    rules.set_color_space(0, "role1").unwrap();
    config.set_file_rules(&rules);
    config.validate().unwrap();

    rules.set_color_space(0, "invalid_color_space").unwrap();
    config.set_file_rules(&rules);
    check_throw_what(
        config.validate(),
        "rule named 'rule1' is referencing 'invalid_color_space' that is neither a color space \
         nor a named transform",
    );
}

/// Port of `OCIO_ADD_TEST(FileRules, multiple_rules)` @ v2.5.2.
#[test]
fn multiple_rules() {
    let _env = EnvGuard::new();
    let mut config = g_config();

    config.validate().unwrap();
    let mut rules = (*config.file_rules().get()).clone();

    // Create multiple rules.
    let nb_rules_to_create = 42;
    let nb_default_rules = rules.num_entries();
    let mut nb_rules_created = 0;
    while nb_rules_created < nb_rules_to_create {
        let rule_name = format!("rule{nb_rules_created}");
        rules.insert_rule(0, &rule_name, "cs1", "*", "exr").unwrap();
        nb_rules_created += 1;
        assert_eq!(rules.num_entries(), nb_rules_created + nb_default_rules);
    }

    config.set_file_rules(&rules);

    // Serialize the config.
    let oss = config.serialize().unwrap();

    // Reload config.
    let config_reloaded = Config::create_from_stream(&oss).unwrap();
    let rules_reloaded = config_reloaded.file_rules().get();

    // Validate that we have the correct number of rules.
    assert_eq!(
        rules_reloaded.num_entries(),
        nb_rules_created + nb_default_rules
    );
}

/// Port of `OCIO_ADD_TEST(FileRules, config_no_default_role)` @ v2.5.2.
#[test]
fn config_no_default_role() {
    let _env = EnvGuard::new();
    // Test with a config that does not have a default role, nor a default color space.
    // Default rule points to an existing color space.
    const CONFIG_NO_DEFAULT: &str = r#"ocio_profile_version: 2
environment:
  {}
strictparsing: true
roles:
  role1: cs1
  role2: cs2
displays:
  sRGB:
  - !<View> {name: Raw, colorspace: raw}
colorspaces:
  - !<ColorSpace>
      name: raw
  - !<ColorSpace>
      name: cs1
  - !<ColorSpace>
      name: cs2
file_rules:
  - !<Rule> {name: Default, colorspace: cs1}
"#;

    // As a warning message is expected, please mute it.
    let (config, log) =
        crate::test_env::capture_log(|| Config::create_from_stream(CONFIG_NO_DEFAULT.as_bytes()));
    let config = config.unwrap();

    assert!(log.is_empty());

    config.validate().unwrap();
}

/// The checks of `config_v1_to_v2_from_file` after an upgrade: the version, and the two file
/// rules with the default rule's color space `default_cs`.
fn check_upgraded_rules(config: &Config, default_cs: &[u8]) {
    // Check the new version.

    assert_eq!(config.major_version(), 2);

    // Check the new file rules.

    let rules = config.file_rules().get();
    assert_eq!(rules.num_entries(), 2);
    assert_eq!(
        rules.name(0).unwrap(),
        FileRules::FILE_PATH_SEARCH_RULE_NAME.as_bytes()
    );
    assert_eq!(
        rules.name(1).unwrap(),
        FileRules::DEFAULT_RULE_NAME.as_bytes()
    );
    assert_eq!(rules.color_space(1).unwrap(), default_cs);
}

/// Port of `OCIO_ADD_TEST(FileRules, config_v1_to_v2_from_file)` @ v2.5.2.
#[test]
fn config_v1_to_v2_from_file() {
    let _env = EnvGuard::new();
    // The unit test checks the file rules when loading a v1 config, the upgrade from v1 to v2
    // and finally, the use of file rules with the upgraded v2 in-memory config.
    //
    // Note: For now, only the file rules and the versions are impacted by the upgrade.

    {
        // Test the common use case i.e. read a v1 config file and upgrade it to v2.

        const CONFIG_V1: &str = r#"ocio_profile_version: 1
strictparsing: true
roles:
  default: raw
  role1: cs1
  role2: cs2
displays:
  sRGB:
  - !<View> {name: Raw, colorspace: raw}
colorspaces:
  - !<ColorSpace>
      name: raw
  - !<ColorSpace>
      name: cs1
  - !<ColorSpace>
      name: cs2
"#;

        let mut config = (*Config::create_from_stream(CONFIG_V1.as_bytes()).unwrap()).clone();
        config.validate().unwrap();

        // Check the version.

        assert_eq!(config.major_version(), 1);

        // Check the file rules.

        let rules = config.file_rules().get();
        assert_eq!(rules.num_entries(), 2);
        assert_eq!(
            rules.name(0).unwrap(),
            FileRules::FILE_PATH_SEARCH_RULE_NAME.as_bytes()
        );
        assert_eq!(
            rules.name(1).unwrap(),
            FileRules::DEFAULT_RULE_NAME.as_bytes()
        );
        assert_eq!(rules.color_space(1).unwrap(), b"default");

        // Check the v1 in-memory file rules are working.

        // It checks that the rule 'FileRules::FilePathSearchRuleName' exists.
        assert_eq!(
            config
                .color_space_from_filepath("/usr/cs2_file.exr")
                .unwrap(),
            b"cs2"
        );
        // It checks that the rule 'Default' exists.
        assert_eq!(
            config.color_space_from_filepath("/usr/file.exr").unwrap(),
            b"default"
        );

        // Upgrading is making sure to build a valid v2 config.

        config.upgrade_to_latest_version().unwrap();

        {
            let (result, mut log) = validate_logged(&config);
            result.unwrap();
            // Check that the log contains the expected error messages for the missing roles and
            // mute them so that (only) those messages don't appear in the test output.
            check_and_mute_role_errors(&mut log);
        }

        check_upgraded_rules(&config, b"default");

        // Check the v1 in-memory file rules are working.

        assert_eq!(
            config
                .color_space_from_filepath("/usr/cs2_file.exr")
                .unwrap(),
            b"cs2"
        );
        assert_eq!(
            config.color_space_from_filepath("/usr/file.exr").unwrap(),
            b"default"
        );
    }

    {
        // The default role is missing and there is a 'data' color space named rAw.

        const CONFIG_V1: &str = r#"ocio_profile_version: 1
strictparsing: true
roles:
  role1: cs1
  role2: cs2
displays:
  sRGB:
  - !<View> {name: Raw, colorspace: rAw}
colorspaces:
  - !<ColorSpace>
      name: rAw
      isdata: true
  - !<ColorSpace>
      name: cs1
  - !<ColorSpace>
      name: cs2
"#;

        let mut config = (*Config::create_from_stream(CONFIG_V1.as_bytes()).unwrap()).clone();
        config.validate().unwrap();

        // Check the version.

        assert_eq!(config.major_version(), 1);

        // Check the file rules.

        let rules = config.file_rules().get();
        assert_eq!(rules.num_entries(), 2);
        assert_eq!(
            rules.name(0).unwrap(),
            FileRules::FILE_PATH_SEARCH_RULE_NAME.as_bytes()
        );
        assert_eq!(
            rules.name(1).unwrap(),
            FileRules::DEFAULT_RULE_NAME.as_bytes()
        );
        assert_eq!(rules.color_space(1).unwrap(), b"rAw");

        // Check the v1 in-memory file rules are working.

        assert_eq!(
            config
                .color_space_from_filepath("/usr/cs2_file.exr")
                .unwrap(),
            b"cs2"
        );
        assert_eq!(
            config.color_space_from_filepath("/usr/file.exr").unwrap(),
            b"rAw"
        );

        // Upgrading is making sure to build a valid v2 config.

        config.upgrade_to_latest_version().unwrap();

        {
            let (result, mut log) = validate_logged(&config);
            result.unwrap();
            // Ignore (only) the errors logged regarding the missing roles that are required in
            // configs with version >= 2.2.
            check_and_mute_role_errors(&mut log);
        }

        check_upgraded_rules(&config, b"rAw");

        assert_eq!(
            config
                .color_space_from_filepath("/usr/cs2_file.exr")
                .unwrap(),
            b"cs2"
        );
        assert_eq!(
            config.color_space_from_filepath("/usr/file.exr").unwrap(),
            b"rAw"
        );
    }

    {
        // The default role is missing and there is no 'data' color space so, the first
        // color space is used in v1, and the first active color space is used in v2.

        // Note that inactive color spaces do not exist in v1 explaining why the first color
        // space is used.

        const CONFIG_V1: &str = r#"ocio_profile_version: 1
strictparsing: true
roles:
  role1: cs1
  role2: cs2
displays:
  sRGB:
  - !<View> {name: Raw, colorspace: rAw}
colorspaces:
  - !<ColorSpace>
      name: cs1
  - !<ColorSpace>
      name: cs2
  - !<ColorSpace>
      name: rAw
"#;

        let config = Config::create_from_stream(CONFIG_V1.as_bytes()).unwrap();
        config.validate().unwrap();

        // Check the version.

        assert_eq!(config.major_version(), 1);

        // Check the file rules.

        let rules = config.file_rules().get();
        assert_eq!(rules.num_entries(), 2);
        assert_eq!(
            rules.name(0).unwrap(),
            FileRules::FILE_PATH_SEARCH_RULE_NAME.as_bytes()
        );
        assert_eq!(
            rules.name(1).unwrap(),
            FileRules::DEFAULT_RULE_NAME.as_bytes()
        );
        assert_eq!(rules.color_space(1).unwrap(), b"cs1");

        // Check the v1 in-memory file rules are working.

        assert_eq!(
            config
                .color_space_from_filepath("/usr/cs2_file.exr")
                .unwrap(),
            b"cs2"
        );
        assert_eq!(
            config.color_space_from_filepath("/usr/file.exr").unwrap(),
            b"cs1"
        );

        {
            // In v2, the first active color space is then used for the 'Default' rule.

            let mut cfg = (*config).clone();

            // Upgrading is making sure to build a valid v2 config.

            cfg.set_inactive_color_spaces("cs1");
            cfg.upgrade_to_latest_version().unwrap();

            {
                let (result, mut log) = validate_logged(&cfg);
                result.unwrap();
                // Ignore (only) the errors logged regarding the missing roles that are required
                // in configs with version >= 2.2.
                check_and_mute_role_errors(&mut log);
            }

            check_upgraded_rules(&cfg, b"cs2");

            assert_eq!(
                cfg.color_space_from_filepath("/usr/cs1_file.exr").unwrap(),
                b"cs1"
            );
            assert_eq!(
                cfg.color_space_from_filepath("/usr/file.exr").unwrap(),
                b"cs2"
            );
        }

        {
            // In v2, the first color space is used for the 'Default' rule because there no
            // active color spaces.

            let mut cfg = (*config).clone();

            // Upgrading is making sure to build a valid v2 config.

            cfg.set_inactive_color_spaces("cs1, cs2, raw");

            {
                let (upgraded, l) =
                    crate::test_env::capture_log(|| cfg.upgrade_to_latest_version());
                upgraded.unwrap();

                assert_eq!(
                    String::from_utf8(l.concat()).unwrap(),
                    "[OpenColorIO Warning]: The default rule creation falls back to the first \
                     color space because no suitable color space exists.\n"
                );
            }

            {
                let (result, mut log) = validate_logged(&cfg);
                result.unwrap();
                // Ignore (only) the errors logged regarding the missing roles that are required
                // in configs with version >= 2.2.
                check_and_mute_role_errors(&mut log);
            }

            check_upgraded_rules(&cfg, b"cs1");

            assert_eq!(
                cfg.color_space_from_filepath("/usr/raw_file.exr").unwrap(),
                b"rAw"
            );
            assert_eq!(
                cfg.color_space_from_filepath("/usr/file.exr").unwrap(),
                b"cs1"
            );
        }
    }
}

/// Port of `OCIO_ADD_TEST(FileRules, config_v1_to_v2_from_memory)` @ v2.5.2.
#[test]
fn config_v1_to_v2_from_memory() {
    let _env = EnvGuard::new();
    // The unit test checks the file rules from an in-memory v1 config, the upgrade from v1 to
    // v2, and finally, the file rules in the upgraded v2 in-memory config.
    //
    // Note: For now, only the file rules and the versions are impacted by the upgrade.

    // The following tests manually create an in-memory v1 config with faulty file rules. As the
    // config file read (which automatically updates in-memory v1 file rules like in previous
    // tests) is not used, only an explicit upgrade to the latest version, can fix the file
    // rules.

    // The 'Default' rule refers to the 'default' role, which doesn't exist: the role errors are
    // logged, then validate throws.
    let check_default_rule_fails = |config: &Config| {
        let (result, mut log) = validate_logged(config);
        check_throw_what(
            result,
            "rule named 'Default' is referencing 'default' that is neither a color space nor a \
             named transform",
        );
        // Ignore (only) the errors logged regarding the missing roles that are required in
        // configs with version >= 2.2.
        check_and_mute_role_errors(&mut log);
    };
    let check_validates = |config: &Config| {
        let (result, mut log) = validate_logged(config);
        result.unwrap();
        // Ignore (only) the errors logged regarding the missing roles that are required in
        // configs with version >= 2.2.
        check_and_mute_role_errors(&mut log);
    };

    {
        // The default role is missing but there is an active 'data' color space.

        let mut config = Config::new().unwrap();
        config.set_major_version(1).unwrap();
        config
            .add_display_view("disp1", "view1", "cs1", "")
            .unwrap();
        let mut cs1 = ColorSpace::new();
        cs1.set_name("cs1");
        cs1.set_is_data(true);
        config.add_color_space(&cs1).unwrap();
        let mut raw = ColorSpace::new();
        raw.set_name("rAw");
        config.add_color_space(&raw).unwrap();
        config.validate().unwrap(); // (does not fail since the major version is 1)

        // Default rule is using 'Default' role that does not exist.
        config.set_major_version(2).unwrap();

        check_default_rule_fails(&config);

        // Upgrading is making sure to build a valid v2 config.
        config.set_major_version(1).unwrap();
        config.upgrade_to_latest_version().unwrap();

        check_validates(&config);

        // 'cs1' is an active & 'data' color space.

        check_upgraded_rules(&config, b"cs1");
    }

    // The default role is missing and there is no 'data' color space.

    {
        let mut config = Config::new().unwrap();
        config.set_major_version(1).unwrap();
        config
            .add_display_view("disp1", "view1", "cs1", "")
            .unwrap();
        let mut cs1 = ColorSpace::new();
        cs1.set_name("cs1");
        config.add_color_space(&cs1).unwrap();
        let mut raw = ColorSpace::new();
        raw.set_name("rAw");
        config.add_color_space(&raw).unwrap();
        config.validate().unwrap(); // (does not fail since the major version is 1)

        // Default rule is using 'Default' role but the associated color space does not exist.
        config.set_major_version(2).unwrap();

        check_default_rule_fails(&config);

        // Upgrading is making sure to build a valid v2 config.
        config.set_major_version(1).unwrap();
        config.upgrade_to_latest_version().unwrap();

        check_validates(&config);

        // 'Default' role does not exist, 'Raw' is not a data color-space, so use the first
        // active color space.

        check_upgraded_rules(&config, b"cs1");
    }

    // The default role is missing and there is no 'data' & active color space. The algorithm
    // then fallbacks to the first available color space and logs a warning.

    {
        let mut config = Config::new().unwrap();
        config.set_major_version(1).unwrap();
        config
            .add_display_view("disp1", "view1", "cs1", "")
            .unwrap();
        let mut cs1 = ColorSpace::new();
        cs1.set_name("cs1");
        config.add_color_space(&cs1).unwrap();
        config.validate().unwrap(); // (does not fail since the major version is 1)

        // Default rule is using 'Default' role but the associated color space does not exist.
        config.set_inactive_color_spaces("cs1");
        config.set_major_version(2).unwrap();

        check_default_rule_fails(&config);

        config.set_major_version(1).unwrap();

        {
            let (upgraded, log) =
                crate::test_env::capture_log(|| config.upgrade_to_latest_version());
            upgraded.unwrap();
            let svec = split_by_lines(&log.concat());
            assert!(svec.iter().any(|l| {
                l.as_slice()
                    == b"[OpenColorIO Warning]: The default rule creation falls back to the first \
                     color space because no suitable color space exists."
                        .as_slice()
            }));
        }

        check_validates(&config);

        // Check the 'default' rule. As there is not 'data' or active color space, the default
        // rule is using an inactive color space.

        check_upgraded_rules(&config, b"cs1");
    }
}

/// Port of `OCIO_ADD_TEST(FileRules, read_write_incomplete_configs)` @ v2.5.2.
#[test]
fn read_write_incomplete_configs() {
    let _env = EnvGuard::new();
    // It should be possible to read and write configs where that are syntactically valid
    // but which are incomplete and hence would not pass validation.

    // The default role references a color space that has not been defined yet.
    {
        const CONFIG: &str = r#"ocio_profile_version: 2
roles:
  default: cs2

colorspaces:
  - !<ColorSpace>
      name: raw
"#;

        // Test read works.
        let cfg = Config::create_from_stream(CONFIG.as_bytes()).unwrap();

        // Test write works.
        cfg.serialize().unwrap();

        // Test that validate catches the problem.
        check_throw_what(
            cfg.validate(),
            "Config failed role validation. The role 'default' refers to a color space, 'cs2', \
             which is not defined.",
        );
    }

    // FileRules Default rule references a color space that has not been defined yet.
    {
        const CONFIG: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: cs2}

displays:
  sRGB:
  - !<View> {name: Raw, colorspace: raw}
colorspaces:
  - !<ColorSpace>
      name: raw
"#;

        // Test read works.
        let cfg = Config::create_from_stream(CONFIG.as_bytes()).unwrap();

        // Test write works.
        cfg.serialize().unwrap();

        // Test that validate catches the problem.
        check_throw_what(
            cfg.validate(),
            "File rules: rule named 'Default' is referencing 'cs2' that is neither a color space \
             nor a named transform.",
        );
    }
}

/// Port of `OCIO_ADD_TEST(FileRules, rule_move)` @ v2.5.2.
#[test]
fn rule_move() {
    let _env = EnvGuard::new();
    let config = g_config();

    config.validate().unwrap();
    let mut rules = (*config.file_rules().get()).clone();

    rules.insert_rule(0, "rule0", "cs1", "*", "exr").unwrap();
    rules.insert_rule(1, "rule1", "cs1", "*", "exr").unwrap();
    rules.insert_rule(2, "rule2", "cs1", "*", "exr").unwrap();
    rules.insert_rule(3, "rule3", "cs1", "*", "exr").unwrap();
    rules.insert_rule(4, "rule4", "cs1", "*", "exr").unwrap();
    assert_eq!(rules.num_entries(), 6);

    check_throw_what(
        rules.increase_rule_priority(0),
        "may not be moved to index '-1'",
    );
    check_throw_what(
        rules.decrease_rule_priority(4),
        "may not be moved to index '5'",
    );

    check_throw_what(rules.increase_rule_priority(5), "is the default rule");
    check_throw_what(rules.decrease_rule_priority(5), "is the default rule");

    rules.decrease_rule_priority(2).unwrap();
    assert_eq!(rules.num_entries(), 6);
    assert_eq!(rules.name(2).unwrap(), b"rule3");
    assert_eq!(rules.name(3).unwrap(), b"rule2");

    rules.increase_rule_priority(3).unwrap();
    assert_eq!(rules.num_entries(), 6);
    assert_eq!(rules.name(2).unwrap(), b"rule2");
    assert_eq!(rules.name(3).unwrap(), b"rule3");

    rules.decrease_rule_priority(2).unwrap();
    rules.decrease_rule_priority(3).unwrap();
    assert_eq!(rules.num_entries(), 6);
    assert_eq!(rules.name(2).unwrap(), b"rule3");
    assert_eq!(rules.name(3).unwrap(), b"rule4");
    assert_eq!(rules.name(4).unwrap(), b"rule2");

    rules.increase_rule_priority(4).unwrap();
    rules.increase_rule_priority(3).unwrap();
    assert_eq!(rules.num_entries(), 6);
    assert_eq!(rules.name(2).unwrap(), b"rule2");
    assert_eq!(rules.name(3).unwrap(), b"rule3");
    assert_eq!(rules.name(4).unwrap(), b"rule4");
}
