// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/apphelpers/MergeConfigsHelpers_tests.cpp` @ v2.5.2: the OCIOM tests
//! (9.2c). The section tests come with their mergers (9.2d-9.2i), the merges with 9.2j.

use super::*;
use crate::fileformats::test_utils::get_test_files_dir;
use ocio_ops::utils::pystring::os_path;

/// `pystring::os::path::normpath(pystring::os::path::join(paths))` of the test files'
/// directory, `configs`, `mergeconfigs` and `file`.
fn merge_test_file(file: &str) -> Vec<u8> {
    let paths = [
        get_test_files_dir().into_bytes(),
        b"configs".to_vec(),
        b"mergeconfigs".to_vec(),
        file.as_bytes().to_vec(),
    ];
    let joined = paths[1..]
        .iter()
        .fold(paths[0].clone(), |acc, p| os_path::join(&acc, p));
    os_path::normpath(&joined)
}

/// Port of `OCIO_ADD_TEST(MergeConfigs, ociom_parser)` @ v2.5.2.
#[test]
fn ociom_parser() {
    let ociom_path = merge_test_file("parser_test.ociom");

    // Ensure version is initialized correctly.
    let merger = ConfigMerger::new();
    assert_eq!(merger.major_version(), 1);
    assert_eq!(merger.minor_version(), 0);

    // Test parsing an OCIOM file.
    let merger = ConfigMerger::from_file(&ociom_path).unwrap();

    assert_eq!(merger.major_version(), 1);
    assert_eq!(merger.minor_version(), 0);

    // Check the search path for finding the base and input configs.
    assert_eq!(merger.num_search_paths(), 2);
    assert_eq!(merger.search_path(0), b"/usr/local/configs");
    assert_eq!(merger.search_path(1), b".");

    // The parser_test.ociom contains only one merge.
    assert_eq!(merger.num_config_merging_parameters(), 1);
    let p = merger.params(0).unwrap();

    // Test that the all the options are loaded correctly.

    assert_eq!(p.base_config_name(), b"base0.ocio");
    assert_eq!(p.input_config_name(), b"input0.ocio");
    assert_eq!(p.output_name(), b"Merge1");

    assert_eq!(p.input_family_prefix(), b"abc");
    assert_eq!(p.base_family_prefix(), b"def");
    assert!(p.is_input_first());
    assert!(!p.is_error_on_conflict());

    assert_eq!(p.default_strategy(), MergeStrategies::InputOnly);
    assert!(p.is_avoid_duplicates());
    assert!(p.is_adjust_input_reference_space());

    assert_eq!(p.name(), b"my merge");
    assert_eq!(p.description(), b"my desc");
    assert_eq!(p.search_path(), b"abc");

    // Expecting two environment variables.
    assert_eq!(p.num_environment_vars(), 2);
    assert_eq!(p.environment_var(0), b"test");
    assert_eq!(p.environment_var_value(0), b"valueOther");
    assert_eq!(p.environment_var(1), b"test1");
    assert_eq!(p.environment_var_value(1), b"value123");

    assert_eq!(p.active_displays(), b"D1, D2");
    assert_eq!(p.active_views(), b"V1, V2");
    assert_eq!(p.inactive_color_spaces(), b"I1, I2");

    // PreferInput
    assert_eq!(p.roles(), MergeStrategies::PreferInput);
    // PreferBase
    assert_eq!(p.file_rules(), MergeStrategies::PreferBase);
    // InputOnly
    assert_eq!(p.display_views(), MergeStrategies::InputOnly);
    // BaseOnly
    assert_eq!(p.looks(), MergeStrategies::BaseOnly);
    // Remove
    assert_eq!(p.colorspaces(), MergeStrategies::Remove);
    // PreferBase
    assert_eq!(p.named_transforms(), MergeStrategies::PreferBase);
}

/// Port of `OCIO_ADD_TEST(MergeConfigs, params_serialization)` @ v2.5.2.
#[test]
fn params_serialization() {
    let ociom_path = merge_test_file("parser_test.ociom");

    let merger = ConfigMerger::from_file(&ociom_path).unwrap();
    let p = merger.params(0).unwrap();

    const REF: &str = concat!(
        "<base: base0.ocio, input: input0.ocio, output_name: Merge1, input_family_prefix: abc, ",
        "base_family_prefix: def, input_first: true, error_on_conflict: false, default_strategy: InputOnly, ",
        "avoid_duplicates: true, adjust_input_reference_space: true, name: my merge, description: my desc, ",
        "search_path: abc, active_displays: D1, D2, active_views: V1, V2, inactive_colorspaces: I1, I2, ",
        "roles: PreferInput, file_rules: PreferBase, display-views: InputOnly, view_transforms: PreferBase, ",
        "looks: BaseOnly, colorspaces: Remove, named_transforms: PreferBase, ",
        "environment: [test=valueOther, test1=value123]>"
    );

    let oss = p.to_bytes();

    assert_eq!(String::from_utf8(oss).unwrap(), REF);
}

/// Port of `OCIO_ADD_TEST(MergeConfigs, ociom_serialization)` @ v2.5.2.
#[test]
fn ociom_serialization() {
    let ociom_path = merge_test_file("parser_test.ociom");
    let merger = ConfigMerger::from_file(&ociom_path).unwrap();

    let oss = merger.serialize().unwrap();

    const REF: &str = r#"ociom_version: 1.0
search_path:
  - /usr/local/configs
  - .
merge:
  Merge1:
    base: base0.ocio
    input: input0.ocio
    options:
      input_family_prefix: abc
      base_family_prefix: def
      input_first: true
      error_on_conflict: false
      default_strategy: InputOnly
      avoid_duplicates: true
      adjust_input_reference_space: true
    overrides:
      name: my merge
      description: my desc
      search_path: abc
      environment:
        test: valueOther
        test1: value123
      active_displays: [D1, D2]
      active_views: [V1, V2]
      inactive_colorspaces: [I1, I2]
    params:
      roles:
        strategy: PreferInput
      file_rules:
        strategy: PreferBase
      display-views:
        strategy: InputOnly
      view_transforms:
        strategy: PreferBase
      looks:
        strategy: BaseOnly
      colorspaces:
        strategy: Remove
      named_transforms:
        strategy: PreferBase"#;

    let rss = REF;
    assert_eq!(String::from_utf8(oss).unwrap(), rss);

    // `oss2 << *merger`: the same text.
    let oss2 = merger.serialize().unwrap();

    assert_eq!(String::from_utf8(oss2).unwrap(), rss);
}

/// Port of `OCIO_ADD_TEST(MergeConfigs, ociom_parser_no_overrides)` @ v2.5.2.
#[test]
fn ociom_parser_no_overrides() {
    let ociom_path = merge_test_file("parser_test_no_overrides.ociom");

    let merger = ConfigMerger::from_file(&ociom_path).unwrap();

    // The parser_test.ociom contains only one merge.
    let p = merger.params(0).unwrap();

    // Test that the all the options are loaded correctly.
    // Note that is does not test all possibilities.
    // e.g. it does not test all the strategies for all sections.

    assert_eq!(p.base_config_name(), b"input0.ocio");
    assert_eq!(p.input_config_name(), b"input2.ocio");

    assert_eq!(p.input_family_prefix(), b"abc");
    assert_eq!(p.base_family_prefix(), b"def");
    assert!(p.is_input_first());
    assert!(!p.is_error_on_conflict());
    // PreferInput
    assert_eq!(p.default_strategy(), MergeStrategies::InputOnly);
    assert!(p.is_avoid_duplicates());
    assert!(p.is_adjust_input_reference_space());

    assert_eq!(p.name(), b"");
    assert_eq!(p.description(), b"");
    assert_eq!(p.search_path(), b"");

    // Expecting 0 environment variables.
    assert_eq!(p.num_environment_vars(), 0);

    assert_eq!(p.active_displays(), b"");
    assert_eq!(p.active_views(), b"");
    assert_eq!(p.inactive_color_spaces(), b"");

    assert_eq!(p.roles(), MergeStrategies::PreferInput);
    assert_eq!(p.file_rules(), MergeStrategies::PreferBase);
    assert_eq!(p.display_views(), MergeStrategies::InputOnly);
    assert_eq!(p.view_transforms(), MergeStrategies::PreferBase);
    assert_eq!(p.looks(), MergeStrategies::BaseOnly);
    assert_eq!(p.colorspaces(), MergeStrategies::Remove);
    assert_eq!(p.named_transforms(), MergeStrategies::PreferBase);
}
