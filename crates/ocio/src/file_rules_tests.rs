// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of the tests of `tests/cpu/FileRules_tests.cpp` @ v2.5.2 that don't need
//! `Config::validate`: those that read no YAML, those that read configs (3.3k), and those that
//! write them (3.7b). Those that validate come with 3.8;
//! `crates/ocio/tests/file_rules_oracle.rs` checks the rules against the wheel.

use ocio_testkit::upstream::check_throw_what;

use super::*;
use crate::test_env::EnvGuard;

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
