// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/apphelpers/MergeConfigsHelpers_tests.cpp` @ v2.5.2: the OCIOM tests
//! (9.2c) and the section tests, each with its merger (9.2d-9.2i). The merges come with 9.2j.

use super::*;
use crate::apphelpers::mergeconfigs::section_merger::{
    DisplayViewMerger, FileRulesMerger, GeneralMerger, LooksMerger, MergeHandlerOptions,
    RolesMerger, Section,
};
use crate::file_rules::FileRules;
use crate::fileformats::test_utils::get_test_files_dir;
use crate::viewing_rules::ViewingRules;
use ocio_ops::logging::{get_logging_level, set_logging_level};
use ocio_ops::open_color_types::{LoggingLevel, ViewType};
use ocio_ops::std_regex::{Regex, regex_replace};
use ocio_ops::utils::pystring::os_path;
use ocio_testkit::upstream::check_close;

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

/// Port of `getBaseConfig` (tests/cpu/apphelpers/MergeConfigsHelpers_tests.cpp:18-28 @
/// v2.5.2).
fn get_base_config() -> Result<Arc<Config>> {
    Config::create_from_file(merge_test_file("base_config.yaml"))
}

/// Port of `getInputConfig` (MergeConfigsHelpers_tests.cpp:30-40 @ v2.5.2).
fn get_input_config() -> Result<Arc<Config>> {
    Config::create_from_file(merge_test_file("input_config.yaml"))
}

/// Port of `LogType` (MergeConfigsHelpers_tests.cpp:107-112 @ v2.5.2), without
/// `LOG_TYPE_INFO`, which no test uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LogType {
    Warning,
    Error,
}

/// Removes from `output` every match of the expression `pattern` (as the wheel's C++ library,
/// `std::regex`, reads it), searching again from the start after each removal; whether it found
/// one.
///
/// Port of `LogGuard::findAllAndRemove` (tests/cpu/UnitTestLogUtils.cpp:88-101 @ v2.5.2). Each
/// round removes every match `regex_replace` finds, left to right, and the rounds repeat until
/// none is left: the same text as upstream's one match per round.
fn find_all_and_remove(output: &mut Vec<u8>, pattern: &[u8]) -> bool {
    let re = Regex::new(pattern, crate::file_rules::LIBRARY).unwrap();
    let mut found = false;
    loop {
        let removed = regex_replace(output, &re, b"").unwrap();
        if removed == *output {
            return found;
        }
        found = true;
        *output = removed;
    }
}

/// Port of `checkAndMuteWarning` (tests/cpu/UnitTestLogUtils.cpp:162-166 @ v2.5.2): `s` is
/// part of the expression, unescaped.
fn check_and_mute_warning(output: &mut Vec<u8>, s: &str) -> bool {
    let pattern = [
        br"\[OpenColorIO Warning\]: ".as_slice(),
        s.as_bytes(),
        br"[\.\r\n]+",
    ]
    .concat();
    find_all_and_remove(output, &pattern)
}

/// Port of `checkAndMuteError` (tests/cpu/UnitTestLogUtils.cpp:168-172 @ v2.5.2).
fn check_and_mute_error(output: &mut Vec<u8>, s: &str) -> bool {
    let pattern = [
        br"\[OpenColorIO Error\]: ".as_slice(),
        s.as_bytes(),
        br"[\r\n]+",
    ]
    .concat();
    find_all_and_remove(output, &pattern)
}

/// Runs `setup` with the logging level at `Info`, the messages it logs kept. When it succeeds,
/// each of `strings` must be a message of `log_type` it logged, and no other message may be
/// left; when it fails, its error must be the first of `strings`.
///
/// Port of `checkForLogOrException` (MergeConfigsHelpers_tests.cpp:114-163 @ v2.5.2).
#[track_caller]
fn check_for_log_or_exception(
    log_type: LogType,
    setup: impl FnOnce() -> Result<()>,
    strings: &[&str],
) {
    // Use INFO rather than DEBUG for the guard to avoid a lot of OpOptimizers.cpp output.
    let (result, log) = crate::test_env::capture_log(|| {
        struct Level(LoggingLevel);
        impl Drop for Level {
            fn drop(&mut self) {
                set_logging_level(self.0);
            }
        }
        let _level = Level(get_logging_level());
        set_logging_level(LoggingLevel::Info);
        setup()
    });
    let mut log = log.concat();
    match result {
        Ok(()) => {
            for s in strings {
                if log_type == LogType::Error {
                    let error_found = check_and_mute_error(&mut log, s);
                    assert!(error_found, "This error was not found: {s}");
                } else if log_type == LogType::Warning {
                    let warning_found = check_and_mute_warning(&mut log, s);
                    assert!(warning_found, "This warning was not found: {s}");
                }
            }

            // If all messages have not been removed from the log at this point, it's
            // unexpected.
            assert!(
                log.is_empty(),
                "The following unexpected messages were encountered:\n{}",
                String::from_utf8_lossy(&log)
            );
        }
        Err(e) => {
            // Only checking the first string because only the first Exception gets out.
            assert_eq!(e.message(), strings[0]);
        }
    }
}

/// Upstream's `setupBasics` of the general section's test: a merger with new parameters, whose
/// default strategy is `strategy`; the parameters.
fn setup_general_basics(
    merger: &mut ConfigMerger,
    strategy: MergeStrategies,
) -> ConfigMergingParameters {
    let params = ConfigMergingParameters::new().unwrap();
    merger.add_params(Some(params.clone()));
    // The general strategy is determined by the default strategy.
    // However, note that the name and description may be set in the override options, tested above.
    merger.params(0).unwrap().set_default_strategy(strategy);
    params
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

/// Upstream's `OCIO::MergeHandlerOptions options = { baseConfig, inputConfig, params,
/// mergedConfig };` then `OCIO::Merger(options).merge();`: the merger's result.
macro_rules! merge_section {
    ($merger:ident, $base:expr, $input:expr, $params:expr, $merged:expr) => {{
        let mut options = MergeHandlerOptions {
            base_config: $base,
            input_config: $input,
            params: Some($params),
            merged_config: $merged,
        };
        $merger::new(&mut options).and_then(|mut m| m.merge())
    }};
}

/// Port of `OCIO_ADD_TEST(MergeConfigs, general_section)` @ v2.5.2.
#[test]
fn general_section() {
    let mut base_config = get_base_config().unwrap();
    let mut input_config = get_input_config().unwrap();

    let setup_basics = setup_general_basics;

    // Test that the default strategy is used as a fallback if the section strategy was not defined.
    {
        let mut merger = ConfigMerger::new();
        // Using STRATEGY_UNSPECIFIED as this simulates that the section
        // is missing from the OCIOM file.
        let params = setup_basics(&mut merger, MergeStrategies::Unspecified);
        // Simulate settings from OCIOM file.
        merger
            .params(0)
            .unwrap()
            .set_default_strategy(MergeStrategies::InputOnly);
        let mut merged_config = (*base_config).clone();
        merge_section!(
            GeneralMerger,
            &base_config,
            &input_config,
            &params,
            &mut merged_config
        )
        .unwrap();

        assert_eq!(merged_config.name(), b"input0");
        assert_eq!(merged_config.description(), b"My description 2");
        assert_eq!(merged_config.major_version(), 2);
        assert_eq!(merged_config.minor_version(), 1);
        let rgb = merged_config.default_luma_coefs();
        check_close(rgb[1], 0.677998, 1e-4);
    }

    // Test General section with strategy = PreferInput.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
        let mut merged_config = (*base_config).clone();
        merge_section!(
            GeneralMerger,
            &base_config,
            &input_config,
            &params,
            &mut merged_config
        )
        .unwrap();

        assert_eq!(merged_config.name(), b"input0");
        assert_eq!(merged_config.description(), b"My description 2");

        assert_eq!(merged_config.major_version(), 2);
        assert_eq!(merged_config.minor_version(), 1);

        let rgb = merged_config.default_luma_coefs();
        check_close(rgb[1], 0.677998, 1e-4);
    }

    // Test General section with strategy = PreferBase.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::PreferBase);
        let mut merged_config = (*base_config).clone();
        merge_section!(
            GeneralMerger,
            &base_config,
            &input_config,
            &params,
            &mut merged_config
        )
        .unwrap();

        assert_eq!(merged_config.name(), b"base0");
        assert_eq!(merged_config.description(), b"My description 1");

        assert_eq!(merged_config.major_version(), 2);
        // Config version is always highest of both configs, regardless of strategy.
        assert_eq!(merged_config.minor_version(), 1);

        let rgb = merged_config.default_luma_coefs();
        check_close(rgb[1], 0.7152, 1e-4);
    }

    // Test General section with strategy = InputOnly.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::InputOnly);
        let mut merged_config = (*base_config).clone();
        merge_section!(
            GeneralMerger,
            &base_config,
            &input_config,
            &params,
            &mut merged_config
        )
        .unwrap();

        assert_eq!(merged_config.name(), b"input0");
        assert_eq!(merged_config.description(), b"My description 2");

        assert_eq!(merged_config.major_version(), 2);
        assert_eq!(merged_config.minor_version(), 1);

        let rgb = merged_config.default_luma_coefs();
        check_close(rgb[1], 0.677998, 1e-4);
    }

    // Test General section with strategy = BaseOnly.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::BaseOnly);
        let mut merged_config = (*base_config).clone();
        merge_section!(
            GeneralMerger,
            &base_config,
            &input_config,
            &params,
            &mut merged_config
        )
        .unwrap();

        assert_eq!(merged_config.name(), b"base0");
        assert_eq!(merged_config.description(), b"My description 1");

        assert_eq!(merged_config.major_version(), 2);
        // Config version is always highest of both configs, regardless of strategy.
        assert_eq!(merged_config.minor_version(), 1);

        let rgb = merged_config.default_luma_coefs();
        check_close(rgb[1], 0.7152, 1e-4);
    }

    {
        const BASE: &str = r#"ocio_profile_version: 1

roles:
  default: colorspace_a
colorspaces:
- !<ColorSpace>
    name: colorspace_a
"#;

        const INPUT: &str = r#"ocio_profile_version: 2.1

luma: [0.262700, 0.677998, 0.059301]
name: input0
description: |
  My description 2

roles:
  default: colorspace_b
colorspaces:
- !<ColorSpace>
    name: colorspace_b
"#;

        base_config = Config::create_from_stream(BASE.as_bytes()).unwrap();
        input_config = Config::create_from_stream(INPUT.as_bytes()).unwrap();

        // Test General section with strategy = PreferInput.
        {
            let mut merger = ConfigMerger::new();
            let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
            let mut merged_config = (*base_config).clone();
            merge_section!(
                GeneralMerger,
                &base_config,
                &input_config,
                &params,
                &mut merged_config
            )
            .unwrap();

            assert_eq!(merged_config.name(), b"input0");
            assert_eq!(merged_config.description(), b"My description 2");

            assert_eq!(merged_config.major_version(), 2);
            assert_eq!(merged_config.minor_version(), 1);

            let rgb = merged_config.default_luma_coefs();
            check_close(rgb[1], 0.677998, 1e-4);
        }

        // Test General section with strategy = PreferBase.
        {
            let mut merger = ConfigMerger::new();
            let params = setup_basics(&mut merger, MergeStrategies::PreferBase);
            let mut merged_config = (*base_config).clone();
            merge_section!(
                GeneralMerger,
                &base_config,
                &input_config,
                &params,
                &mut merged_config
            )
            .unwrap();

            assert_eq!(merged_config.name(), b"");
            assert_eq!(merged_config.description(), b"");

            assert_eq!(merged_config.major_version(), 2);
            // Config version is always highest of both configs, regardless of strategy.
            assert_eq!(merged_config.minor_version(), 1);

            let rgb = merged_config.default_luma_coefs();
            check_close(rgb[1], 0.7152, 1e-4);
        }

        // Test General section with strategy = InputOnly.
        {
            let mut merger = ConfigMerger::new();
            let params = setup_basics(&mut merger, MergeStrategies::InputOnly);
            let mut merged_config = (*base_config).clone();
            merge_section!(
                GeneralMerger,
                &base_config,
                &input_config,
                &params,
                &mut merged_config
            )
            .unwrap();

            assert_eq!(merged_config.name(), b"input0");
            assert_eq!(merged_config.description(), b"My description 2");

            assert_eq!(merged_config.major_version(), 2);
            assert_eq!(merged_config.minor_version(), 1);

            let rgb = merged_config.default_luma_coefs();
            check_close(rgb[1], 0.677998, 1e-4);
        }

        // Test General section with strategy = BaseOnly.
        {
            let mut merger = ConfigMerger::new();
            let params = setup_basics(&mut merger, MergeStrategies::BaseOnly);
            let mut merged_config = (*base_config).clone();
            merge_section!(
                GeneralMerger,
                &base_config,
                &input_config,
                &params,
                &mut merged_config
            )
            .unwrap();

            assert_eq!(merged_config.name(), b"");
            assert_eq!(merged_config.description(), b"");

            assert_eq!(merged_config.major_version(), 2);
            // Config version is always highest of both configs, regardless of strategy.
            assert_eq!(merged_config.minor_version(), 1);

            let rgb = merged_config.default_luma_coefs();
            check_close(rgb[1], 0.7152, 1e-4);
        }
    }
}

/// Upstream's `setupBasics` of the roles section's test: a merger with new parameters, whose
/// roles and default strategies are `strategy`; the parameters.
fn setup_roles_basics(
    merger: &mut ConfigMerger,
    strategy: MergeStrategies,
) -> ConfigMergingParameters {
    let params = ConfigMergingParameters::new().unwrap();
    merger.add_params(Some(params.clone()));
    merger.params(0).unwrap().set_roles(strategy);
    merger.params(0).unwrap().set_default_strategy(strategy);
    params
}

/// Port of `OCIO_ADD_TEST(MergeConfigs, roles_section)` @ v2.5.2.
#[test]
fn roles_section() {
    // Allowed strategies: PreferInput, PreferBase, InputOnly, BaseOnly, Remove
    // Allowed merge options: ErrorOnConflict.

    let base_config = get_base_config().unwrap();
    let input_config = get_input_config().unwrap();

    let setup_basics = setup_roles_basics;

    // Test that the default strategy is used as a fallback if the section strategy was not defined.
    {
        let mut merger = ConfigMerger::new();
        // Using STRATEGY_UNSPECIFIED as this simulates that the section
        // is missing from the OCIOM file.
        let params = setup_basics(&mut merger, MergeStrategies::Unspecified);
        // Simulate settings from OCIOM file.
        merger
            .params(0)
            .unwrap()
            .set_default_strategy(MergeStrategies::InputOnly);
        let mut merged_config = (*base_config).clone();
        check_for_log_or_exception(
            LogType::Warning,
            || {
                merge_section!(
                    RolesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
            },
            &[
                "The Input config contains a role 'g22_ap1_tx' that would override an alias of Base config color space 'Gamma 2.2 AP1 - Texture'",
                "The Input config contains a role 'nt_base' that would override Base config named transform: 'nt_base'",
            ],
        );

        assert_eq!(merged_config.num_roles(), 3);
        assert_eq!(
            merged_config.role_color_space("aces_interchange"),
            b"ACES2065-1"
        );
        assert_eq!(
            merged_config.role_color_space("texture_paint"),
            b"ACEScct - SomeOtherName"
        );
        assert_eq!(
            merged_config.role_color_space("matte_paint"),
            b"sRGB - Texture"
        );
    }

    // Test Roles section with strategy = PreferInput
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
        let mut merged_config = (*base_config).clone();
        check_for_log_or_exception(
            LogType::Warning,
            || {
                merge_section!(
                    RolesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
            },
            &[
                "The Input config contains a role 'g22_ap1_tx' that would override an alias of Base config color space 'Gamma 2.2 AP1 - Texture'",
                "The Input config contains a role that would override Base config role 'texture_paint'.",
                "The Input config contains a role 'nt_base' that would override Base config named transform: 'nt_base'",
            ],
        );

        assert_eq!(merged_config.num_roles(), 4);

        // Following three roles were overwritten by input config.
        assert_eq!(
            merged_config.role_color_space("aces_interchange"),
            b"ACES2065-1"
        );
        assert_eq!(
            merged_config.role_color_space("texture_paint"),
            b"ACEScct - SomeOtherName"
        );
        assert_eq!(
            merged_config.role_color_space("matte_paint"),
            b"sRGB - Texture"
        );

        // Following role come from base config.
        assert_eq!(merged_config.role_color_space("data"), b"Raw");
    }

    // Test Roles section with strategy = PreferBase
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::PreferBase);
        let mut merged_config = (*base_config).clone();
        check_for_log_or_exception(
            LogType::Warning,
            || {
                merge_section!(
                    RolesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
            },
            &[
                "The Input config contains a role 'g22_ap1_tx' that would override an alias of Base config color space 'Gamma 2.2 AP1 - Texture'",
                "The Input config contains a role that would override Base config role 'texture_paint'.",
                "The Input config contains a role 'nt_base' that would override Base config named transform: 'nt_base'",
            ],
        );

        assert_eq!(merged_config.num_roles(), 4);

        // Following three roles comes form the base config.
        assert_eq!(
            merged_config.role_color_space("aces_interchange"),
            b"ACES2065-1"
        );
        assert_eq!(merged_config.role_color_space("texture_paint"), b"ACEScct");
        assert_eq!(merged_config.role_color_space("data"), b"Raw");

        // Following role come from input config.
        assert_eq!(
            merged_config.role_color_space("matte_paint"),
            b"sRGB - Texture"
        );
    }

    // Test Roles section with strategy = InputOnly.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::InputOnly);
        let mut merged_config = (*base_config).clone();
        check_for_log_or_exception(
            LogType::Warning,
            || {
                merge_section!(
                    RolesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
            },
            &[
                "The Input config contains a role 'g22_ap1_tx' that would override an alias of Base config color space 'Gamma 2.2 AP1 - Texture'",
                "The Input config contains a role 'nt_base' that would override Base config named transform: 'nt_base'",
            ],
        );

        assert_eq!(merged_config.num_roles(), 3);

        // Following three roles comes form the input config.
        assert_eq!(
            merged_config.role_color_space("aces_interchange"),
            b"ACES2065-1"
        );
        assert_eq!(
            merged_config.role_color_space("texture_paint"),
            b"ACEScct - SomeOtherName"
        );
        assert_eq!(
            merged_config.role_color_space("matte_paint"),
            b"sRGB - Texture"
        );
    }

    // Test Roles section with strategy = BaseOnly.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::BaseOnly);
        let mut merged_config = (*base_config).clone();
        merge_section!(
            RolesMerger,
            &base_config,
            &input_config,
            &params,
            &mut merged_config
        )
        .unwrap();

        assert_eq!(merged_config.num_roles(), 3);

        // Following three roles comes form the base config.
        assert_eq!(
            merged_config.role_color_space("aces_interchange"),
            b"ACES2065-1"
        );
        assert_eq!(merged_config.role_color_space("texture_paint"), b"ACEScct");
        assert_eq!(merged_config.role_color_space("data"), b"Raw");
    }

    // Test Roles section with strategy = Remove
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::Remove);
        let mut merged_config = (*base_config).clone();
        merge_section!(
            RolesMerger,
            &base_config,
            &input_config,
            &params,
            &mut merged_config
        )
        .unwrap();

        assert_eq!(merged_config.num_roles(), 1);

        // This is the only role in base that is not in input.
        assert_eq!(merged_config.role_color_space("data"), b"Raw");
    }

    // Test Roles section with strategy = PreferInput and option ErrorOnConflict = true.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
        merger.params(0).unwrap().set_error_on_conflict(true);
        let mut merged_config = (*base_config).clone();
        check_for_log_or_exception(
            LogType::Error,
            || {
                merge_section!(
                    RolesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
            },
            &[
                "The Input config contains a role 'g22_ap1_tx' that would override an alias of Base config color space 'Gamma 2.2 AP1 - Texture'.",
            ],
        );
    }
}

/// Upstream's `setupBasics` of the file rules section's test: a merger with new parameters,
/// whose file rules strategy is `strategy`, without the reference space adjustment or the
/// duplicate search; the parameters.
fn setup_file_rules_basics(
    merger: &mut ConfigMerger,
    strategy: MergeStrategies,
) -> ConfigMergingParameters {
    let params = ConfigMergingParameters::new().unwrap();
    merger.add_params(Some(params.clone()));
    merger.params(0).unwrap().set_file_rules(strategy);
    merger
        .params(0)
        .unwrap()
        .set_adjust_input_reference_space(false);
    merger.params(0).unwrap().set_avoid_duplicates(false);

    params
}

/// `config->createEditableCopy()` with the file rules `rules`.
fn with_file_rules(config: &Config, rules: &FileRules) -> Config {
    let mut config = config.clone();
    config.set_file_rules(rules);
    config
}

/// Port of `OCIO_ADD_TEST(MergeConfigs, file_rules_section)` @ v2.5.2.
#[test]
fn file_rules_section() {
    let base_config = get_base_config().unwrap();
    let input_config = get_input_config().unwrap();

    let setup_basics = setup_file_rules_basics;

    // Allowed strategies: All
    // Allowed merge options: All

    // Test that the default strategy is used as a fallback if the section strategy was not defined.
    {
        let mut merger = ConfigMerger::new();
        // Using STRATEGY_UNSPECIFIED as this simulates that the section is missing from the OCIOM file.
        let params = setup_basics(&mut merger, MergeStrategies::Unspecified);
        // Simulate settings from OCIOM file.
        merger
            .params(0)
            .unwrap()
            .set_default_strategy(MergeStrategies::InputOnly);

        let mut merged_config = (*base_config).clone();
        merge_section!(
            FileRulesMerger,
            &base_config,
            &input_config,
            &params,
            &mut merged_config
        )
        .unwrap();

        assert!(!merged_config.is_strict_parsing_enabled());

        let fr = merged_config.file_rules().get();

        assert_eq!(fr.num_entries(), 5);

        assert_eq!(fr.name(0).unwrap(), b"LogC");
        assert_eq!(fr.color_space(0).unwrap(), b"ACES2065-1");

        assert_eq!(fr.name(1).unwrap(), b"TIFF");
        assert_eq!(fr.color_space(1).unwrap(), b"sRGB - Texture");

        assert_eq!(fr.name(2).unwrap(), b"JPEG");
        assert_eq!(fr.color_space(2).unwrap(), b"Linear Rec.2020");
        assert_eq!(fr.regex(2).unwrap(), br".*\.jpeg$");

        assert_eq!(fr.name(3).unwrap(), b"ColorSpaceNamePathSearch");

        assert_eq!(fr.name(4).unwrap(), b"Default");
        assert_eq!(fr.color_space(4).unwrap(), b"ACEScct - SomeOtherName");
    }

    // Test FileRules section with strategy = PreferInput.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
        merger.params(0).unwrap().set_input_first(true);

        let mut merged_config = (*base_config).clone();
        check_for_log_or_exception(
            LogType::Warning,
            || {
                merge_section!(
                    FileRulesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
            },
            &[
                "The Input config contains a value that would override the Base config: file_rules: TIFF",
                "The Input config contains a value that would override the Base config: file_rules: Default",
            ],
        );

        let fr = merged_config.file_rules().get();

        assert_eq!(fr.num_entries(), 6);

        assert_eq!(fr.name(0).unwrap(), b"LogC");
        assert_eq!(fr.color_space(0).unwrap(), b"ACES2065-1");

        assert_eq!(fr.name(1).unwrap(), b"TIFF");
        assert_eq!(fr.color_space(1).unwrap(), b"sRGB - Texture");
        assert_eq!(fr.regex(1).unwrap(), br".*\.TIF?F$");
        // Verify that the custom keys are merged.
        assert_eq!(fr.custom_key_name(1, 0).unwrap(), b"key1");
        assert_eq!(fr.custom_key_value(1, 0).unwrap(), b"value1");
        assert_eq!(fr.custom_key_name(1, 1).unwrap(), b"key2");
        assert_eq!(fr.custom_key_value(1, 1).unwrap(), b"value2");

        assert_eq!(fr.name(2).unwrap(), b"JPEG");
        assert_eq!(fr.color_space(2).unwrap(), b"Linear Rec.2020");
        assert_eq!(fr.regex(2).unwrap(), br".*\.jpeg$");

        assert_eq!(fr.name(3).unwrap(), b"ColorSpaceNamePathSearch");

        assert_eq!(fr.name(4).unwrap(), b"OpenEXR");
        assert_eq!(fr.color_space(4).unwrap(), b"ACEScct");
        assert_eq!(fr.pattern(4).unwrap(), b"*");
        assert_eq!(fr.extension(4).unwrap(), b"exr");

        assert_eq!(fr.name(5).unwrap(), b"Default");
        assert_eq!(fr.color_space(5).unwrap(), b"ACEScct - SomeOtherName");
    }

    // Test FileRules section with strategy = PreferInput, options InputFirst = false.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
        merger.params(0).unwrap().set_input_first(false);

        let mut merged_config = (*base_config).clone();
        check_for_log_or_exception(
            LogType::Warning,
            || {
                merge_section!(
                    FileRulesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
            },
            &[
                "The Input config contains a value that would override the Base config: file_rules: TIFF",
                "The Input config contains a value that would override the Base config: file_rules: Default",
            ],
        );

        assert!(!merged_config.is_strict_parsing_enabled());

        let fr = merged_config.file_rules().get();

        assert_eq!(fr.num_entries(), 6);

        assert_eq!(fr.name(0).unwrap(), b"LogC");
        assert_eq!(fr.color_space(0).unwrap(), b"ACES2065-1");

        assert_eq!(fr.name(1).unwrap(), b"TIFF");
        assert_eq!(fr.color_space(1).unwrap(), b"sRGB - Texture");
        // Verify that the custom keys are merged.
        assert_eq!(fr.custom_key_name(1, 0).unwrap(), b"key1");
        assert_eq!(fr.custom_key_value(1, 0).unwrap(), b"value1");
        assert_eq!(fr.custom_key_name(1, 1).unwrap(), b"key2");
        assert_eq!(fr.custom_key_value(1, 1).unwrap(), b"value2");

        assert_eq!(fr.name(2).unwrap(), b"OpenEXR");
        assert_eq!(fr.color_space(2).unwrap(), b"ACEScct");
        assert_eq!(fr.pattern(2).unwrap(), b"*");
        assert_eq!(fr.extension(2).unwrap(), b"exr");

        assert_eq!(fr.name(3).unwrap(), b"ColorSpaceNamePathSearch");

        assert_eq!(fr.name(4).unwrap(), b"JPEG");
        assert_eq!(fr.color_space(4).unwrap(), b"Linear Rec.2020");
        assert_eq!(fr.regex(4).unwrap(), br".*\.jpeg$");

        assert_eq!(fr.name(5).unwrap(), b"Default");
        assert_eq!(fr.color_space(5).unwrap(), b"ACEScct - SomeOtherName");
    }

    // Test FileRules section with strategy = PreferBase.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::PreferBase);
        merger.params(0).unwrap().set_input_first(true);

        let mut merged_config = (*base_config).clone();
        check_for_log_or_exception(
            LogType::Warning,
            || {
                merge_section!(
                    FileRulesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
            },
            &[
                "The Input config contains a value that would override the Base config: file_rules: TIFF",
                "The Input config contains a value that would override the Base config: file_rules: Default",
            ],
        );

        assert!(merged_config.is_strict_parsing_enabled());

        let fr = merged_config.file_rules().get();

        assert_eq!(fr.num_entries(), 6);

        assert_eq!(fr.name(0).unwrap(), b"LogC");
        assert_eq!(fr.color_space(0).unwrap(), b"ACES2065-1");

        assert_eq!(fr.name(1).unwrap(), b"TIFF");
        assert_eq!(fr.color_space(1).unwrap(), b"Gamma 2.2 AP1 - Texture");
        assert_eq!(fr.num_custom_keys(1).unwrap(), 0);

        assert_eq!(fr.name(2).unwrap(), b"JPEG");
        assert_eq!(fr.color_space(2).unwrap(), b"Linear Rec.2020");
        assert_eq!(fr.regex(2).unwrap(), br".*\.jpeg$");

        assert_eq!(fr.name(3).unwrap(), b"ColorSpaceNamePathSearch");

        assert_eq!(fr.name(4).unwrap(), b"OpenEXR");
        assert_eq!(fr.color_space(4).unwrap(), b"ACEScct");
        assert_eq!(fr.pattern(4).unwrap(), b"*");
        assert_eq!(fr.extension(4).unwrap(), b"exr");

        assert_eq!(fr.name(5).unwrap(), b"Default");
        assert_eq!(fr.color_space(5).unwrap(), b"Raw");
    }

    // Test FileRules section with strategy = PreferBase, options InputFirst = false.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::PreferBase);
        merger.params(0).unwrap().set_input_first(false);

        let mut merged_config = (*base_config).clone();
        check_for_log_or_exception(
            LogType::Warning,
            || {
                merge_section!(
                    FileRulesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
            },
            &[
                "The Input config contains a value that would override the Base config: file_rules: TIFF",
                "The Input config contains a value that would override the Base config: file_rules: Default",
            ],
        );

        assert!(merged_config.is_strict_parsing_enabled());

        let fr = merged_config.file_rules().get();

        assert_eq!(fr.num_entries(), 6);

        assert_eq!(fr.name(0).unwrap(), b"LogC");
        assert_eq!(fr.color_space(0).unwrap(), b"ACES2065-1");

        assert_eq!(fr.name(1).unwrap(), b"TIFF");
        assert_eq!(fr.color_space(1).unwrap(), b"Gamma 2.2 AP1 - Texture");

        assert_eq!(fr.name(2).unwrap(), b"OpenEXR");
        assert_eq!(fr.color_space(2).unwrap(), b"ACEScct");
        assert_eq!(fr.pattern(2).unwrap(), b"*");
        assert_eq!(fr.extension(2).unwrap(), b"exr");

        assert_eq!(fr.name(3).unwrap(), b"ColorSpaceNamePathSearch");

        assert_eq!(fr.name(4).unwrap(), b"JPEG");
        assert_eq!(fr.color_space(4).unwrap(), b"Linear Rec.2020");
        assert_eq!(fr.regex(4).unwrap(), br".*\.jpeg$");

        assert_eq!(fr.name(5).unwrap(), b"Default");
        assert_eq!(fr.color_space(5).unwrap(), b"Raw");
    }

    // Test FileRules section with strategy = InputOnly.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::InputOnly);

        let mut merged_config = (*base_config).clone();
        merge_section!(
            FileRulesMerger,
            &base_config,
            &input_config,
            &params,
            &mut merged_config
        )
        .unwrap();

        assert!(!merged_config.is_strict_parsing_enabled());

        let fr = merged_config.file_rules().get();

        assert_eq!(fr.num_entries(), 5);

        assert_eq!(fr.name(0).unwrap(), b"LogC");
        assert_eq!(fr.color_space(0).unwrap(), b"ACES2065-1");

        assert_eq!(fr.name(1).unwrap(), b"TIFF");
        assert_eq!(fr.color_space(1).unwrap(), b"sRGB - Texture");

        assert_eq!(fr.name(2).unwrap(), b"JPEG");
        assert_eq!(fr.color_space(2).unwrap(), b"Linear Rec.2020");
        assert_eq!(fr.regex(2).unwrap(), br".*\.jpeg$");

        assert_eq!(fr.name(3).unwrap(), b"ColorSpaceNamePathSearch");

        assert_eq!(fr.name(4).unwrap(), b"Default");
        assert_eq!(fr.color_space(4).unwrap(), b"ACEScct - SomeOtherName");
    }

    // Test FileRules section with strategy = BaseOnly.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::BaseOnly);

        let mut merged_config = (*base_config).clone();
        merge_section!(
            FileRulesMerger,
            &base_config,
            &input_config,
            &params,
            &mut merged_config
        )
        .unwrap();

        assert!(merged_config.is_strict_parsing_enabled());

        let fr = merged_config.file_rules().get();

        assert_eq!(fr.name(0).unwrap(), b"LogC");
        assert_eq!(fr.color_space(0).unwrap(), b"ACES2065-1");

        assert_eq!(fr.name(1).unwrap(), b"TIFF");
        assert_eq!(fr.color_space(1).unwrap(), b"Gamma 2.2 AP1 - Texture");

        assert_eq!(fr.name(2).unwrap(), b"OpenEXR");
        assert_eq!(fr.color_space(2).unwrap(), b"ACEScct");
        assert_eq!(fr.pattern(2).unwrap(), b"*");
        assert_eq!(fr.extension(2).unwrap(), b"exr");

        assert_eq!(fr.name(3).unwrap(), b"ColorSpaceNamePathSearch");

        assert_eq!(fr.name(4).unwrap(), b"Default");
        assert_eq!(fr.color_space(4).unwrap(), b"Raw");
    }

    // Test FileRules section with strategy = Remove.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::Remove);

        let mut merged_config = (*base_config).clone();
        merge_section!(
            FileRulesMerger,
            &base_config,
            &input_config,
            &params,
            &mut merged_config
        )
        .unwrap();

        assert!(merged_config.is_strict_parsing_enabled());

        let fr = merged_config.file_rules().get();

        assert_eq!(fr.num_entries(), 2);

        assert_eq!(fr.name(0).unwrap(), b"OpenEXR");
        assert_eq!(fr.color_space(0).unwrap(), b"ACEScct");
        assert_eq!(fr.pattern(0).unwrap(), b"*");
        assert_eq!(fr.extension(0).unwrap(), b"exr");

        assert_eq!(fr.name(1).unwrap(), b"Default");
        assert_eq!(fr.color_space(1).unwrap(), b"Raw");
    }

    // Test FileRules section with strategy = PreferInput and copying ColorSpaceNamePathSearch.
    {
        let mut merger = ConfigMerger::new();
        let mut merged_config = (*base_config).clone();

        let params = ConfigMergingParameters::new().unwrap();
        merger.add_params(Some(params.clone()));
        merger
            .params(0)
            .unwrap()
            .set_file_rules(MergeStrategies::PreferInput);

        {
            let mut input_fr = (*input_config.file_rules().get()).clone();
            // Delete ColorSpaceNamePathSearch, so it is only in the base and must be copied over.
            input_fr.remove_rule(3).unwrap();
            let editable_input_config = with_file_rules(&input_config, &input_fr);

            check_for_log_or_exception(
                LogType::Warning,
                || {
                    merge_section!(
                        FileRulesMerger,
                        &base_config,
                        &editable_input_config,
                        &params,
                        &mut merged_config
                    )
                },
                &[
                    "The Input config contains a value that would override the Base config: file_rules: TIFF",
                    "The Input config contains a value that would override the Base config: file_rules: Default",
                ],
            );

            let fr = merged_config.file_rules().get();

            assert_eq!(fr.num_entries(), 6);

            assert_eq!(fr.name(0).unwrap(), b"LogC");
            assert_eq!(fr.color_space(0).unwrap(), b"ACES2065-1");

            assert_eq!(fr.name(1).unwrap(), b"TIFF");
            assert_eq!(fr.color_space(1).unwrap(), b"sRGB - Texture");
            assert_eq!(fr.regex(1).unwrap(), br".*\.TIF?F$");
            // Verify that the custom keys are merged.
            assert_eq!(fr.custom_key_name(1, 0).unwrap(), b"key1");
            assert_eq!(fr.custom_key_value(1, 0).unwrap(), b"value1");
            assert_eq!(fr.custom_key_name(1, 1).unwrap(), b"key2");
            assert_eq!(fr.custom_key_value(1, 1).unwrap(), b"value2");

            assert_eq!(fr.name(2).unwrap(), b"JPEG");
            assert_eq!(fr.color_space(2).unwrap(), b"Linear Rec.2020");
            assert_eq!(fr.regex(2).unwrap(), br".*\.jpeg$");

            assert_eq!(fr.name(3).unwrap(), b"OpenEXR");
            assert_eq!(fr.color_space(3).unwrap(), b"ACEScct");
            assert_eq!(fr.pattern(3).unwrap(), b"*");
            assert_eq!(fr.extension(3).unwrap(), b"exr");

            assert_eq!(fr.name(4).unwrap(), b"ColorSpaceNamePathSearch");

            assert_eq!(fr.name(5).unwrap(), b"Default");
            assert_eq!(fr.color_space(5).unwrap(), b"ACEScct - SomeOtherName");
        }
    }

    const PREFIX: &str = "The Input config contains a value that would override the Base config: ";

    // Test that error_on_conflicts is processed correctly.
    // strategy = PreferInput, InputFirst = true
    {
        let mut merger = ConfigMerger::new();
        let mut merged_config = (*base_config).clone();

        let params = ConfigMergingParameters::new().unwrap();
        merger.add_params(Some(params.clone()));
        merger
            .params(0)
            .unwrap()
            .set_file_rules(MergeStrategies::PreferInput);
        merger.params(0).unwrap().set_error_on_conflict(true);
        merger
            .params(0)
            .unwrap()
            .set_default_strategy(MergeStrategies::PreferInput);

        // Test that an error is thrown when the input config's COLORSPACE is different.
        {
            let mut base_fr = FileRules::new();
            base_fr
                .insert_rule(0, "ruleTestColorspace", "colorspace1", "*abc*", "*")
                .unwrap();
            let editable_base_config = with_file_rules(&base_config, &base_fr);

            let mut input_fr = FileRules::new();
            input_fr
                .insert_rule(0, "ruleTestColorspace", "colorspace2", "*abc*", "*")
                .unwrap();
            let editable_input_config = with_file_rules(&input_config, &input_fr);

            check_for_log_or_exception(
                LogType::Error,
                || {
                    merge_section!(
                        FileRulesMerger,
                        &editable_base_config,
                        &editable_input_config,
                        &params,
                        &mut merged_config
                    )
                },
                &[&format!("{PREFIX}file_rules: ruleTestColorspace")],
            );
        }

        // Test that an error is thrown when the input config's REGEX is different.
        {
            let mut base_fr = FileRules::new();
            base_fr
                .insert_regex_rule(0, "ruleTestColorspace", "colorspace1", r".*\.TIF?F$")
                .unwrap();
            let editable_base_config = with_file_rules(&base_config, &base_fr);

            let mut input_fr = FileRules::new();
            input_fr
                .insert_regex_rule(0, "ruleTestColorspace", "colorspace1", r".*\.TIF?F")
                .unwrap();
            let editable_input_config = with_file_rules(&input_config, &input_fr);

            check_for_log_or_exception(
                LogType::Error,
                || {
                    merge_section!(
                        FileRulesMerger,
                        &editable_base_config,
                        &editable_input_config,
                        &params,
                        &mut merged_config
                    )
                },
                &[&format!("{PREFIX}file_rules: ruleTestColorspace")],
            );
        }

        // Test that an error is thrown when the input config's PATTERN is different.
        {
            let mut base_fr = FileRules::new();
            base_fr
                .insert_rule(0, "ruleTestPattern", "colorspace1", "*abc*", "*")
                .unwrap();
            let editable_base_config = with_file_rules(&base_config, &base_fr);

            let mut input_fr = FileRules::new();
            input_fr
                .insert_rule(0, "ruleTestPattern", "colorspace1", "*abcd*", "*")
                .unwrap();
            let editable_input_config = with_file_rules(&input_config, &input_fr);

            check_for_log_or_exception(
                LogType::Error,
                || {
                    merge_section!(
                        FileRulesMerger,
                        &editable_base_config,
                        &editable_input_config,
                        &params,
                        &mut merged_config
                    )
                },
                &[&format!("{PREFIX}file_rules: ruleTestPattern")],
            );
        }

        // Test that an error is thrown when the input config's EXTENSION is different.
        {
            let mut base_fr = FileRules::new();
            base_fr
                .insert_rule(0, "ruleTestExtension", "colorspace1", "*abc*", "*")
                .unwrap();
            let editable_base_config = with_file_rules(&base_config, &base_fr);

            let mut input_fr = FileRules::new();
            input_fr
                .insert_rule(0, "ruleTestExtension", "colorspace1", "*abc*", "*a")
                .unwrap();
            let editable_input_config = with_file_rules(&input_config, &input_fr);

            check_for_log_or_exception(
                LogType::Error,
                || {
                    merge_section!(
                        FileRulesMerger,
                        &editable_base_config,
                        &editable_input_config,
                        &params,
                        &mut merged_config
                    )
                },
                &[&format!("{PREFIX}file_rules: ruleTestExtension")],
            );
        }

        // Test that an error is thrown when the input config's CUSTOM KEYS are different.
        {
            let mut base_fr = FileRules::new();
            base_fr
                .insert_rule(0, "ruleTestCustomKeys", "colorspace1", "*abc*", "*")
                .unwrap();
            base_fr.set_custom_key(0, "key1", "value1").unwrap();
            base_fr.set_custom_key(0, "key2", "value2").unwrap();
            let editable_base_config = with_file_rules(&base_config, &base_fr);

            let mut input_fr = FileRules::new();
            input_fr
                .insert_rule(0, "ruleTestCustomKeys", "colorspace1", "*abc*", "*")
                .unwrap();
            input_fr.set_custom_key(0, "key1", "value1").unwrap();
            input_fr.set_custom_key(0, "key2", "value22").unwrap();
            let editable_input_config = with_file_rules(&input_config, &input_fr);

            check_for_log_or_exception(
                LogType::Error,
                || {
                    merge_section!(
                        FileRulesMerger,
                        &editable_base_config,
                        &editable_input_config,
                        &params,
                        &mut merged_config
                    )
                },
                &[&format!("{PREFIX}file_rules: ruleTestCustomKeys")],
            );
        }

        // Test that no error is thrown when the input config's CUSTOM KEYS are the same.
        {
            let mut base_fr = FileRules::new();
            base_fr
                .insert_rule(0, "ruleTestCustomKeys", "colorspace1", "*abc*", "*")
                .unwrap();
            base_fr.set_custom_key(0, "key1", "value1").unwrap();
            base_fr.set_custom_key(0, "key2", "value2").unwrap();
            let editable_base_config = with_file_rules(&base_config, &base_fr);

            let mut input_fr = FileRules::new();
            input_fr
                .insert_rule(0, "ruleTestCustomKeys", "colorspace1", "*abc*", "*")
                .unwrap();
            input_fr.set_custom_key(0, "key2", "value2").unwrap();
            input_fr.set_custom_key(0, "key1", "value1").unwrap(); // must be equal even in a different order
            let editable_input_config = with_file_rules(&input_config, &input_fr);

            merge_section!(
                FileRulesMerger,
                &editable_base_config,
                &editable_input_config,
                &params,
                &mut merged_config
            )
            .unwrap();
        }
    }
}

/// Upstream's `setupLooks` of the looks section's test: a merger with new parameters, whose
/// looks strategy is `strategy` and family prefixes `Input/` and `Base/`, changed by `cb`, then
/// the looks merged into `merged_config`.
fn setup_looks(
    base_config: &Config,
    input_config: &Config,
    merger: &mut ConfigMerger,
    merged_config: &mut Config,
    strategy: MergeStrategies,
    cb: Option<&dyn Fn(&mut ConfigMerger)>,
) {
    let params = ConfigMergingParameters::new().unwrap();
    merger.add_params(Some(params.clone()));
    merger.params(0).unwrap().set_looks(strategy);

    merger.params(0).unwrap().set_input_family_prefix("Input/");
    merger.params(0).unwrap().set_base_family_prefix("Base/");

    if let Some(cb) = cb {
        cb(merger);
    }

    merge_section!(
        LooksMerger,
        base_config,
        input_config,
        &params,
        merged_config
    )
    .unwrap();
}

/// The process space of the merged config's look at `index`.
fn look_process_space(config: &Config, index: i32) -> &[u8] {
    config
        .look(config.look_name_by_index(index))
        .unwrap()
        .process_space()
}

/// Port of `OCIO_ADD_TEST(MergeConfigs, looks_section)` @ v2.5.2.
#[test]
fn looks_section() {
    let base_config = get_base_config().unwrap();
    let input_config = get_input_config().unwrap();

    // Test that the default strategy is used as a fallback if the section strategy was not defined.
    {
        let mut merger = ConfigMerger::new();
        let mut merged_config = (*base_config).clone();
        setup_looks(
            &base_config,
            &input_config,
            &mut merger,
            &mut merged_config,
            // Using STRATEGY_UNSPECIFIED as this simulate that the section
            // is missing from the OCIOM file.
            MergeStrategies::Unspecified,
            Some(&|merger: &mut ConfigMerger| {
                // Simulate settings from OCIOM file.
                merger
                    .params(0)
                    .unwrap()
                    .set_default_strategy(MergeStrategies::InputOnly);
            }),
        );

        assert_eq!(merged_config.num_looks(), 2);
        assert_eq!(merged_config.look_name_by_index(0), b"look_both");
        assert_eq!(merged_config.look_name_by_index(1), b"look_input");

        assert_eq!(
            look_process_space(&merged_config, 0),
            b"ACEScct - SomeOtherName"
        );
        assert_eq!(look_process_space(&merged_config, 1), b"log_3");
    }

    // Test Looks with strategy = PreferInput, options InputFirst = true.
    {
        let mut merger = ConfigMerger::new();
        let mut merged_config = (*base_config).clone();

        setup_looks(
            &base_config,
            &input_config,
            &mut merger,
            &mut merged_config,
            MergeStrategies::PreferInput,
            Some(&|merger: &mut ConfigMerger| {
                merger
                    .params(0)
                    .unwrap()
                    .set_adjust_input_reference_space(false);
            }),
        );

        assert_eq!(merged_config.num_looks(), 3);
        assert_eq!(merged_config.look_name_by_index(0), b"look_both");
        assert_eq!(merged_config.look_name_by_index(1), b"look_input");
        assert_eq!(merged_config.look_name_by_index(2), b"look_base");

        assert_eq!(
            look_process_space(&merged_config, 0),
            b"ACEScct - SomeOtherName"
        );
        assert_eq!(look_process_space(&merged_config, 1), b"log_3");
        assert_eq!(look_process_space(&merged_config, 2), b"log_1");
    }

    // Test Looks with strategy=PreferInput, options InputFirst = false.
    {
        let mut merger = ConfigMerger::new();
        let mut merged_config = (*base_config).clone();

        setup_looks(
            &base_config,
            &input_config,
            &mut merger,
            &mut merged_config,
            MergeStrategies::PreferInput,
            Some(&|merger: &mut ConfigMerger| {
                merger.params(0).unwrap().set_input_first(false);
                merger
                    .params(0)
                    .unwrap()
                    .set_adjust_input_reference_space(false);
            }),
        );

        assert_eq!(merged_config.num_looks(), 3);
        assert_eq!(merged_config.look_name_by_index(0), b"look_both");
        assert_eq!(merged_config.look_name_by_index(1), b"look_base");
        assert_eq!(merged_config.look_name_by_index(2), b"look_input");

        assert_eq!(
            look_process_space(&merged_config, 0),
            b"ACEScct - SomeOtherName"
        );
        assert_eq!(look_process_space(&merged_config, 1), b"log_1");
        assert_eq!(look_process_space(&merged_config, 2), b"log_3");
    }

    // Test Looks with strategy = PreferBase, options InputFirst = true.
    {
        let mut merger = ConfigMerger::new();
        let mut merged_config = (*base_config).clone();
        setup_looks(
            &base_config,
            &input_config,
            &mut merger,
            &mut merged_config,
            MergeStrategies::PreferBase,
            Some(&|merger: &mut ConfigMerger| {
                merger.params(0).unwrap().set_input_first(true);
                merger
                    .params(0)
                    .unwrap()
                    .set_adjust_input_reference_space(false);
            }),
        );

        assert_eq!(merged_config.num_looks(), 3);
        assert_eq!(merged_config.look_name_by_index(0), b"look_both");
        assert_eq!(merged_config.look_name_by_index(1), b"look_input");
        assert_eq!(merged_config.look_name_by_index(2), b"look_base");

        assert_eq!(look_process_space(&merged_config, 0), b"ACES2065-1");
        assert_eq!(look_process_space(&merged_config, 1), b"log_3");
        assert_eq!(look_process_space(&merged_config, 2), b"log_1");
    }

    // Test Looks with strategy = PreferBase, options InputFirst = false.
    {
        let mut merger = ConfigMerger::new();
        let mut merged_config = (*base_config).clone();
        setup_looks(
            &base_config,
            &input_config,
            &mut merger,
            &mut merged_config,
            MergeStrategies::PreferBase,
            Some(&|merger: &mut ConfigMerger| {
                merger.params(0).unwrap().set_input_first(false);
                merger
                    .params(0)
                    .unwrap()
                    .set_adjust_input_reference_space(false);
            }),
        );

        assert_eq!(merged_config.num_looks(), 3);
        assert_eq!(merged_config.look_name_by_index(0), b"look_both");
        assert_eq!(merged_config.look_name_by_index(1), b"look_base");
        assert_eq!(merged_config.look_name_by_index(2), b"look_input");

        assert_eq!(look_process_space(&merged_config, 0), b"ACES2065-1");
        assert_eq!(look_process_space(&merged_config, 1), b"log_1");
        assert_eq!(look_process_space(&merged_config, 2), b"log_3");
    }

    // Test Looks with strategy = BaseOnly.
    {
        let mut merger = ConfigMerger::new();
        let mut merged_config = (*base_config).clone();
        setup_looks(
            &base_config,
            &input_config,
            &mut merger,
            &mut merged_config,
            MergeStrategies::BaseOnly,
            None,
        );

        assert_eq!(merged_config.num_looks(), 2);
        assert_eq!(merged_config.look_name_by_index(0), b"look_both");
        assert_eq!(merged_config.look_name_by_index(1), b"look_base");

        assert_eq!(look_process_space(&merged_config, 0), b"ACES2065-1");
        assert_eq!(look_process_space(&merged_config, 1), b"log_1");
    }

    // Test Looks with strategy = InputOnly.
    {
        let mut merger = ConfigMerger::new();
        let mut merged_config = (*base_config).clone();
        setup_looks(
            &base_config,
            &input_config,
            &mut merger,
            &mut merged_config,
            MergeStrategies::InputOnly,
            None,
        );

        assert_eq!(merged_config.num_looks(), 2);
        assert_eq!(merged_config.look_name_by_index(0), b"look_both");
        assert_eq!(merged_config.look_name_by_index(1), b"look_input");

        assert_eq!(
            look_process_space(&merged_config, 0),
            b"ACEScct - SomeOtherName"
        );
        assert_eq!(look_process_space(&merged_config, 1), b"log_3");
    }

    // Test Looks with strategy = Remove
    {
        let mut merger = ConfigMerger::new();
        let mut merged_config = (*base_config).clone();
        setup_looks(
            &base_config,
            &input_config,
            &mut merger,
            &mut merged_config,
            MergeStrategies::Remove,
            None,
        );

        assert_eq!(merged_config.num_looks(), 1);
        assert_eq!(merged_config.look_name_by_index(0), b"look_base");
        assert_eq!(look_process_space(&merged_config, 0), b"log_1");
    }
}

/// Upstream's `setupBasics` of the displays and views section's test: a merger with new
/// parameters, whose displays and views strategy is `strategy`; the parameters.
fn setup_display_views_basics(
    merger: &mut ConfigMerger,
    strategy: MergeStrategies,
) -> ConfigMergingParameters {
    let params = ConfigMergingParameters::new().unwrap();
    merger.add_params(Some(params.clone()));
    merger.params(0).unwrap().set_display_views(strategy);
    params
}

/// `rules->getColorSpace(rule, index)` of a color space the rule has.
fn rule_color_space(rules: &ViewingRules, rule: usize, index: usize) -> &[u8] {
    rules.color_space(rule, index).unwrap().unwrap()
}

/// `rules->getEncoding(rule, index)` of an encoding the rule has.
fn rule_encoding(rules: &ViewingRules, rule: usize, index: usize) -> &[u8] {
    rules.encoding(rule, index).unwrap().unwrap()
}

/// Port of `OCIO_ADD_TEST(MergeConfigs, displays_views_section)` @ v2.5.2.
#[test]
fn displays_views_section() {
    const VIEW_DISPLAY_DEFINED: ViewType = ViewType::DisplayDefined;
    const VIEW_SHARED: ViewType = ViewType::Shared;

    let base_config = get_base_config().unwrap();
    let input_config = get_input_config().unwrap();

    let setup_basics = setup_display_views_basics;

    // Allowed strategies: All
    // Allowed merge options: All

    // Test that the default strategy is used as a fallback if the section strategy was not defined.
    {
        let mut merger = ConfigMerger::new();

        // Using STRATEGY_UNSPECIFIED as this simulates that the section
        // is missing from the OCIOM file.
        let params = setup_basics(&mut merger, MergeStrategies::Unspecified);
        // Simulate settings from OCIOM file.
        merger
            .params(0)
            .unwrap()
            .set_default_strategy(MergeStrategies::InputOnly);
        merger.params(0).unwrap().set_input_first(true);

        let mut merged_config = (*base_config).clone();
        merge_section!(
            DisplayViewMerger,
            &base_config,
            &input_config,
            &params,
            &mut merged_config
        )
        .unwrap();

        assert_eq!(merged_config.active_displays(), b"DISP_1, DISP_3");
        assert_eq!(
            merged_config.active_views(),
            b"SHARED_1, SHARED_3, VIEW_1, VIEW_3"
        );

        // Validate shared_views
        assert_eq!(merged_config.num_views_of_type(VIEW_SHARED, b""), 2);
        assert_eq!(merged_config.view_of_type(VIEW_SHARED, b"", 0), b"SHARED_1");
        assert_eq!(merged_config.view_of_type(VIEW_SHARED, b"", 1), b"SHARED_3");

        // Validate displays
        assert_eq!(merged_config.num_displays_all(), 2);
        assert_eq!(merged_config.display(0), b"DISP_1");
        assert_eq!(merged_config.display(1), b"DISP_3");

        // Validate display/views

        assert_eq!(
            merged_config.num_views_of_type(VIEW_DISPLAY_DEFINED, "DISP_1"),
            1
        );
        assert_eq!(merged_config.num_views_of_type(VIEW_SHARED, "DISP_1"), 2);
        assert_eq!(
            merged_config.view_of_type(VIEW_DISPLAY_DEFINED, "DISP_1", 0),
            b"VIEW_1"
        );
        // Make sure this is the right VIEW_1 by checking the colorspace.
        assert_eq!(
            merged_config.display_view_color_space_name("DISP_1", "VIEW_1"),
            b"view_1B"
        );

        assert_eq!(
            merged_config.view_of_type(VIEW_SHARED, "DISP_1", 0),
            b"SHARED_3"
        );
        assert_eq!(
            merged_config.display_view_color_space_name("DISP_1", "SHARED_3"),
            b"log_3"
        );

        assert_eq!(
            merged_config.view_of_type(VIEW_SHARED, "DISP_1", 1),
            b"SHARED_1"
        );
        assert_eq!(
            merged_config.display_view_color_space_name("DISP_1", "SHARED_1"),
            b"lin_3"
        );

        assert_eq!(
            merged_config.num_views_of_type(VIEW_DISPLAY_DEFINED, "DISP_3"),
            2
        );
        assert_eq!(
            merged_config.view_of_type(VIEW_DISPLAY_DEFINED, "DISP_3", 0),
            b"VIEW_1"
        );
        assert_eq!(
            merged_config.view_of_type(VIEW_DISPLAY_DEFINED, "DISP_3", 1),
            b"VIEW_3"
        );

        // Validate viewing_rules
        let rules = merged_config.viewing_rules().get();

        assert_eq!(rules.num_entries(), 2);

        assert_eq!(rules.name(0).unwrap(), b"RULE_1");
        assert_eq!(rules.num_color_spaces(0).unwrap(), 1);
        assert_eq!(rule_color_space(&rules, 0, 0), b"sRGB - Texture");

        assert_eq!(rules.name(1).unwrap(), b"RULE_3");
        assert_eq!(rules.num_color_spaces(1).unwrap(), 2);
        assert_eq!(rule_color_space(&rules, 1, 0), b"Linear Rec.2020");
        assert_eq!(rule_color_space(&rules, 1, 1), b"ACEScct - SomeOtherName");

        // Validate virtual_display
        assert_eq!(
            merged_config.virtual_display_num_views(VIEW_DISPLAY_DEFINED),
            2
        );
        assert_eq!(merged_config.virtual_display_num_views(VIEW_SHARED), 1);

        assert_eq!(
            merged_config.virtual_display_view(VIEW_DISPLAY_DEFINED, 0),
            b"ACES"
        );
        assert_eq!(
            merged_config.virtual_display_view(VIEW_DISPLAY_DEFINED, 1),
            b"Lin"
        );

        assert_eq!(
            merged_config.virtual_display_view(VIEW_SHARED, 0),
            b"SHARED_3"
        );
    }

    // Test display/views with strategy = PreferInput, options InputFirst = true.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
        merger.params(0).unwrap().set_input_first(true);
        let mut merged_config = (*base_config).clone();
        check_for_log_or_exception(
            LogType::Warning,
            || {
                merge_section!(
                    DisplayViewMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
            },
            &[
                "The Input config contains a value that would override the Base config: shared_views: SHARED_1",
                "The Input config contains a value that would override the Base config: display: DISP_1, view: VIEW_1",
                "The Input config contains a value that would override the Base config: viewing_rules: RULE_1",
            ],
        );

        assert_eq!(merged_config.active_displays(), b"DISP_1, DISP_3, DISP_2");
        assert_eq!(
            merged_config.active_views(),
            b"SHARED_1, SHARED_3, VIEW_1, VIEW_3, SHARED_2, VIEW_2"
        );

        // Validate shared_views
        assert_eq!(merged_config.num_views_of_type(VIEW_SHARED, b""), 3);
        assert_eq!(merged_config.view_of_type(VIEW_SHARED, b"", 0), b"SHARED_1");
        assert_eq!(merged_config.view_of_type(VIEW_SHARED, b"", 1), b"SHARED_3");
        assert_eq!(merged_config.view_of_type(VIEW_SHARED, b"", 2), b"SHARED_2");

        // Validate displays
        assert_eq!(merged_config.num_displays_all(), 3);
        assert_eq!(merged_config.display(0), b"DISP_1");
        assert_eq!(merged_config.display(1), b"DISP_3");
        assert_eq!(merged_config.display(2), b"DISP_2");

        // Validate display/views
        assert_eq!(
            merged_config.num_views_of_type(VIEW_DISPLAY_DEFINED, "DISP_1"),
            1
        );
        assert_eq!(merged_config.num_views_of_type(VIEW_SHARED, "DISP_1"), 3);
        assert_eq!(
            merged_config.view_of_type(VIEW_DISPLAY_DEFINED, "DISP_1", 0),
            b"VIEW_1"
        );
        // Make sure this is the right VIEW_1 by checking the colorspace.
        assert_eq!(
            merged_config.display_view_color_space_name("DISP_1", "VIEW_1"),
            b"view_1B"
        );
        assert_eq!(
            merged_config.display_view_rule("DISP_1", "VIEW_1"),
            b"RULE_3"
        );

        assert_eq!(
            merged_config.view_of_type(VIEW_SHARED, "DISP_1", 0),
            b"SHARED_3"
        );
        assert_eq!(
            merged_config.display_view_color_space_name("DISP_1", "SHARED_3"),
            b"log_3"
        );

        assert_eq!(
            merged_config.view_of_type(VIEW_SHARED, "DISP_1", 1),
            b"SHARED_1"
        );
        assert_eq!(
            merged_config.display_view_color_space_name("DISP_1", "SHARED_1"),
            b"lin_3"
        );

        assert_eq!(
            merged_config.view_of_type(VIEW_SHARED, "DISP_1", 2),
            b"SHARED_2"
        );
        assert_eq!(
            merged_config.display_view_color_space_name("DISP_1", "SHARED_2"),
            b"<USE_DISPLAY_NAME>"
        );
        assert_eq!(
            merged_config.display_view_transform_name("DISP_1", "SHARED_2"),
            b"SDR Video"
        );

        assert_eq!(
            merged_config.num_views_of_type(VIEW_DISPLAY_DEFINED, "DISP_3"),
            2
        );
        assert_eq!(
            merged_config.view_of_type(VIEW_DISPLAY_DEFINED, "DISP_3", 0),
            b"VIEW_1"
        );
        assert_eq!(
            merged_config.view_of_type(VIEW_DISPLAY_DEFINED, "DISP_3", 1),
            b"VIEW_3"
        );
        assert_eq!(
            merged_config.display_view_looks("DISP_3", "VIEW_3"),
            b"look_input"
        );

        assert_eq!(
            merged_config.num_views_of_type(VIEW_DISPLAY_DEFINED, "DISP_2"),
            2
        );
        assert_eq!(
            merged_config.view_of_type(VIEW_DISPLAY_DEFINED, "DISP_2", 0),
            b"VIEW_1"
        );
        assert_eq!(
            merged_config.view_of_type(VIEW_DISPLAY_DEFINED, "DISP_2", 1),
            b"VIEW_2"
        );

        // Validate viewing_rules
        let rules = merged_config.viewing_rules().get();

        assert_eq!(rules.num_entries(), 3);

        assert_eq!(rules.name(0).unwrap(), b"RULE_1");
        assert_eq!(rules.num_color_spaces(0).unwrap(), 1);
        assert_eq!(rule_color_space(&rules, 0, 0), b"sRGB - Texture");

        assert_eq!(rules.name(1).unwrap(), b"RULE_3");
        assert_eq!(rules.num_color_spaces(1).unwrap(), 2);
        assert_eq!(rule_color_space(&rules, 1, 0), b"Linear Rec.2020");
        assert_eq!(rule_color_space(&rules, 1, 1), b"ACEScct - SomeOtherName");

        assert_eq!(rules.name(2).unwrap(), b"RULE_2");
        assert_eq!(rules.num_encodings(2).unwrap(), 1);
        assert_eq!(rule_encoding(&rules, 2, 0), b"scene-linear");

        // Validate virtual_display
        assert_eq!(
            merged_config.virtual_display_num_views(VIEW_DISPLAY_DEFINED),
            3
        );
        assert_eq!(merged_config.virtual_display_num_views(VIEW_SHARED), 2);

        assert_eq!(
            merged_config.virtual_display_view(VIEW_DISPLAY_DEFINED, 0),
            b"ACES"
        );
        assert_eq!(
            merged_config.virtual_display_view(VIEW_DISPLAY_DEFINED, 1),
            b"Lin"
        );
        assert_eq!(
            merged_config.virtual_display_view(VIEW_DISPLAY_DEFINED, 2),
            b"Log"
        );

        assert_eq!(
            merged_config.virtual_display_view(VIEW_SHARED, 0),
            b"SHARED_3"
        );
        assert_eq!(
            merged_config.virtual_display_view(VIEW_SHARED, 1),
            b"SHARED_1"
        );
    }

    // Test display/views with strategy=PreferInput, options InputFirst = false.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
        merger.params(0).unwrap().set_input_first(false);
        let mut merged_config = (*base_config).clone();
        check_for_log_or_exception(
            LogType::Warning,
            || {
                merge_section!(
                    DisplayViewMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
            },
            &[
                "The Input config contains a value that would override the Base config: shared_views: SHARED_1",
                "The Input config contains a value that would override the Base config: display: DISP_1, view: VIEW_1",
                "The Input config contains a value that would override the Base config: viewing_rules: RULE_1",
            ],
        );

        assert_eq!(merged_config.active_displays(), b"DISP_1, DISP_2, DISP_3");
        assert_eq!(
            merged_config.active_views(),
            b"SHARED_1, SHARED_2, VIEW_1, VIEW_2, SHARED_3, VIEW_3"
        );

        // Validate shared_views
        assert_eq!(merged_config.num_views_of_type(VIEW_SHARED, b""), 3);
        assert_eq!(merged_config.view_of_type(VIEW_SHARED, b"", 0), b"SHARED_1");
        assert_eq!(merged_config.view_of_type(VIEW_SHARED, b"", 1), b"SHARED_2");
        assert_eq!(merged_config.view_of_type(VIEW_SHARED, b"", 2), b"SHARED_3");

        // Validate displays
        assert_eq!(merged_config.num_displays_all(), 3);
        assert_eq!(merged_config.display(0), b"DISP_1");
        assert_eq!(merged_config.display(1), b"DISP_2");
        assert_eq!(merged_config.display(2), b"DISP_3");

        // Validate display/views

        assert_eq!(
            merged_config.num_views_of_type(VIEW_DISPLAY_DEFINED, "DISP_1"),
            1
        );
        assert_eq!(merged_config.num_views_of_type(VIEW_SHARED, "DISP_1"), 3);
        assert_eq!(
            merged_config.view_of_type(VIEW_DISPLAY_DEFINED, "DISP_1", 0),
            b"VIEW_1"
        );
        // Make sure this is the right VIEW_1 by checking the colorspace.
        assert_eq!(
            merged_config.display_view_color_space_name("DISP_1", "VIEW_1"),
            b"view_1B"
        );
        assert_eq!(
            merged_config.display_view_rule("DISP_1", "VIEW_1"),
            b"RULE_3"
        );

        assert_eq!(
            merged_config.view_of_type(VIEW_SHARED, "DISP_1", 0),
            b"SHARED_1"
        );
        assert_eq!(
            merged_config.display_view_color_space_name("DISP_1", "SHARED_1"),
            b"lin_3"
        );

        assert_eq!(
            merged_config.view_of_type(VIEW_SHARED, "DISP_1", 1),
            b"SHARED_2"
        );
        assert_eq!(
            merged_config.display_view_color_space_name("DISP_1", "SHARED_2"),
            b"<USE_DISPLAY_NAME>"
        );
        assert_eq!(
            merged_config.display_view_transform_name("DISP_1", "SHARED_2"),
            b"SDR Video"
        );

        assert_eq!(
            merged_config.view_of_type(VIEW_SHARED, "DISP_1", 2),
            b"SHARED_3"
        );
        assert_eq!(
            merged_config.display_view_color_space_name("DISP_1", "SHARED_3"),
            b"log_3"
        );

        assert_eq!(
            merged_config.num_views_of_type(VIEW_DISPLAY_DEFINED, "DISP_2"),
            2
        );
        assert_eq!(
            merged_config.view_of_type(VIEW_DISPLAY_DEFINED, "DISP_2", 0),
            b"VIEW_1"
        );
        assert_eq!(
            merged_config.view_of_type(VIEW_DISPLAY_DEFINED, "DISP_2", 1),
            b"VIEW_2"
        );

        assert_eq!(
            merged_config.num_views_of_type(VIEW_DISPLAY_DEFINED, "DISP_3"),
            2
        );
        assert_eq!(
            merged_config.view_of_type(VIEW_DISPLAY_DEFINED, "DISP_3", 0),
            b"VIEW_1"
        );
        assert_eq!(
            merged_config.view_of_type(VIEW_DISPLAY_DEFINED, "DISP_3", 1),
            b"VIEW_3"
        );
        assert_eq!(
            merged_config.display_view_looks("DISP_3", "VIEW_3"),
            b"look_input"
        );

        // Validate viewing_rules
        let rules = merged_config.viewing_rules().get();

        assert_eq!(rules.num_entries(), 3);

        assert_eq!(rules.name(0).unwrap(), b"RULE_1");
        assert_eq!(rules.num_color_spaces(0).unwrap(), 1);
        assert_eq!(rule_color_space(&rules, 0, 0), b"sRGB - Texture");

        assert_eq!(rules.name(1).unwrap(), b"RULE_2");
        assert_eq!(rules.num_encodings(1).unwrap(), 1);
        assert_eq!(rule_encoding(&rules, 1, 0), b"scene-linear");

        assert_eq!(rules.name(2).unwrap(), b"RULE_3");
        assert_eq!(rules.num_color_spaces(2).unwrap(), 2);
        assert_eq!(rule_color_space(&rules, 2, 0), b"Linear Rec.2020");
        assert_eq!(rule_color_space(&rules, 2, 1), b"ACEScct - SomeOtherName");

        // Validate virtual_display
        assert_eq!(
            merged_config.virtual_display_num_views(VIEW_DISPLAY_DEFINED),
            3
        );
        assert_eq!(merged_config.virtual_display_num_views(VIEW_SHARED), 2);

        assert_eq!(
            merged_config.virtual_display_view(VIEW_DISPLAY_DEFINED, 0),
            b"ACES"
        );
        assert_eq!(
            merged_config.virtual_display_view(VIEW_DISPLAY_DEFINED, 1),
            b"Log"
        );
        assert_eq!(
            merged_config.virtual_display_view(VIEW_DISPLAY_DEFINED, 2),
            b"Lin"
        );

        assert_eq!(
            merged_config.virtual_display_view(VIEW_SHARED, 0),
            b"SHARED_1"
        );
        assert_eq!(
            merged_config.virtual_display_view(VIEW_SHARED, 1),
            b"SHARED_3"
        );
    }

    // Test display/views with strategy = PreferBase, options InputFirst = true.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::PreferBase);
        merger.params(0).unwrap().set_input_first(true);
        let mut merged_config = (*base_config).clone();
        check_for_log_or_exception(
            LogType::Warning,
            || {
                merge_section!(
                    DisplayViewMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
            },
            &[
                "The Input config contains a value that would override the Base config: shared_views: SHARED_1",
                "The Input config contains a value that would override the Base config: display: DISP_1, view: VIEW_1",
                "The Input config contains a value that would override the Base config: viewing_rules: RULE_1",
            ],
        );

        assert_eq!(merged_config.active_displays(), b"DISP_1, DISP_3, DISP_2");
        assert_eq!(
            merged_config.active_views(),
            b"SHARED_1, SHARED_3, VIEW_1, VIEW_3, SHARED_2, VIEW_2"
        );

        // Validate shared_views
        assert_eq!(merged_config.num_views_of_type(VIEW_SHARED, b""), 3);
        assert_eq!(merged_config.view_of_type(VIEW_SHARED, b"", 0), b"SHARED_1");
        assert_eq!(merged_config.view_of_type(VIEW_SHARED, b"", 1), b"SHARED_3");
        assert_eq!(merged_config.view_of_type(VIEW_SHARED, b"", 2), b"SHARED_2");

        // Validate displays
        assert_eq!(merged_config.num_displays_all(), 3);
        assert_eq!(merged_config.display(0), b"DISP_1");
        assert_eq!(merged_config.display(1), b"DISP_3");
        assert_eq!(merged_config.display(2), b"DISP_2");

        // Validate display/views

        assert_eq!(
            merged_config.num_views_of_type(VIEW_DISPLAY_DEFINED, "DISP_1"),
            1
        );
        assert_eq!(merged_config.num_views_of_type(VIEW_SHARED, "DISP_1"), 3);
        assert_eq!(
            merged_config.view_of_type(VIEW_DISPLAY_DEFINED, "DISP_1", 0),
            b"VIEW_1"
        );
        // Make sure this is the right VIEW_1 by checking the colorspace.
        assert_eq!(
            merged_config.display_view_color_space_name("DISP_1", "VIEW_1"),
            b"view_1"
        );
        assert_eq!(
            merged_config.display_view_rule("DISP_1", "VIEW_1"),
            b"RULE_1"
        );

        assert_eq!(
            merged_config.view_of_type(VIEW_SHARED, "DISP_1", 0),
            b"SHARED_3"
        );
        assert_eq!(
            merged_config.display_view_color_space_name("DISP_1", "SHARED_3"),
            b"log_3"
        );

        assert_eq!(
            merged_config.view_of_type(VIEW_SHARED, "DISP_1", 1),
            b"SHARED_1"
        );
        assert_eq!(
            merged_config.display_view_color_space_name("DISP_1", "SHARED_1"),
            b"lin_1"
        );

        assert_eq!(
            merged_config.view_of_type(VIEW_SHARED, "DISP_1", 2),
            b"SHARED_2"
        );
        assert_eq!(
            merged_config.display_view_color_space_name("DISP_1", "SHARED_2"),
            b"<USE_DISPLAY_NAME>"
        );
        assert_eq!(
            merged_config.display_view_transform_name("DISP_1", "SHARED_2"),
            b"SDR Video"
        );

        assert_eq!(
            merged_config.num_views_of_type(VIEW_DISPLAY_DEFINED, "DISP_3"),
            2
        );
        assert_eq!(
            merged_config.view_of_type(VIEW_DISPLAY_DEFINED, "DISP_3", 0),
            b"VIEW_1"
        );
        assert_eq!(
            merged_config.view_of_type(VIEW_DISPLAY_DEFINED, "DISP_3", 1),
            b"VIEW_3"
        );
        assert_eq!(
            merged_config.display_view_looks("DISP_3", "VIEW_3"),
            b"look_input"
        );

        assert_eq!(
            merged_config.num_views_of_type(VIEW_DISPLAY_DEFINED, "DISP_2"),
            2
        );
        assert_eq!(
            merged_config.view_of_type(VIEW_DISPLAY_DEFINED, "DISP_2", 0),
            b"VIEW_1"
        );
        assert_eq!(
            merged_config.display_view_rule("DISP_2", "VIEW_1"),
            b"RULE_2"
        );
        assert_eq!(
            merged_config.view_of_type(VIEW_DISPLAY_DEFINED, "DISP_2", 1),
            b"VIEW_2"
        );
        assert_eq!(
            merged_config.display_view_looks("DISP_2", "VIEW_2"),
            b"look_base"
        );

        // Validate viewing_rules
        let rules = merged_config.viewing_rules().get();

        assert_eq!(rules.num_entries(), 3);

        assert_eq!(rules.name(0).unwrap(), b"RULE_1");
        assert_eq!(rules.num_color_spaces(0).unwrap(), 1);
        assert_eq!(rule_color_space(&rules, 0, 0), b"Gamma 2.2 AP1 - Texture");

        assert_eq!(rules.name(1).unwrap(), b"RULE_3");
        assert_eq!(rules.num_color_spaces(1).unwrap(), 2);
        assert_eq!(rule_color_space(&rules, 1, 0), b"Linear Rec.2020");
        assert_eq!(rule_color_space(&rules, 1, 1), b"ACEScct - SomeOtherName");

        assert_eq!(rules.name(2).unwrap(), b"RULE_2");
        assert_eq!(rules.num_encodings(2).unwrap(), 1);
        assert_eq!(rule_encoding(&rules, 2, 0), b"scene-linear");

        // Validate virtual_display
        assert_eq!(
            merged_config.virtual_display_num_views(VIEW_DISPLAY_DEFINED),
            3
        );
        assert_eq!(merged_config.virtual_display_num_views(VIEW_SHARED), 2);

        assert_eq!(
            merged_config.virtual_display_view(VIEW_DISPLAY_DEFINED, 0),
            b"ACES"
        );
        assert_eq!(
            merged_config.virtual_display_view(VIEW_DISPLAY_DEFINED, 1),
            b"Lin"
        );
        assert_eq!(
            merged_config.virtual_display_view(VIEW_DISPLAY_DEFINED, 2),
            b"Log"
        );

        assert_eq!(
            merged_config.virtual_display_view(VIEW_SHARED, 0),
            b"SHARED_3"
        );
        assert_eq!(
            merged_config.virtual_display_view(VIEW_SHARED, 1),
            b"SHARED_1"
        );
    }

    // Test display/views with strategy = PreferBase, options InputFirst = false.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::PreferBase);
        merger.params(0).unwrap().set_input_first(false);
        let mut merged_config = (*base_config).clone();
        check_for_log_or_exception(
            LogType::Warning,
            || {
                merge_section!(
                    DisplayViewMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
            },
            &[
                "The Input config contains a value that would override the Base config: shared_views: SHARED_1",
                "The Input config contains a value that would override the Base config: display: DISP_1, view: VIEW_1",
                "The Input config contains a value that would override the Base config: viewing_rules: RULE_1",
            ],
        );

        assert_eq!(merged_config.active_displays(), b"DISP_1, DISP_2, DISP_3");
        assert_eq!(
            merged_config.active_views(),
            b"SHARED_1, SHARED_2, VIEW_1, VIEW_2, SHARED_3, VIEW_3"
        );

        // Validate shared_views
        assert_eq!(merged_config.num_views_of_type(VIEW_SHARED, b""), 3);
        assert_eq!(merged_config.view_of_type(VIEW_SHARED, b"", 0), b"SHARED_1");
        assert_eq!(merged_config.view_of_type(VIEW_SHARED, b"", 1), b"SHARED_2");
        assert_eq!(merged_config.view_of_type(VIEW_SHARED, b"", 2), b"SHARED_3");

        // Validate displays
        assert_eq!(merged_config.num_displays_all(), 3);
        assert_eq!(merged_config.display(0), b"DISP_1");
        assert_eq!(merged_config.display(1), b"DISP_2");
        assert_eq!(merged_config.display(2), b"DISP_3");

        // Validate display/views

        assert_eq!(
            merged_config.num_views_of_type(VIEW_DISPLAY_DEFINED, "DISP_1"),
            1
        );
        assert_eq!(merged_config.num_views_of_type(VIEW_SHARED, "DISP_1"), 3);
        assert_eq!(
            merged_config.view_of_type(VIEW_DISPLAY_DEFINED, "DISP_1", 0),
            b"VIEW_1"
        );
        // Make sure this is the right VIEW_1 by checking the colorspace.
        assert_eq!(
            merged_config.display_view_color_space_name("DISP_1", "VIEW_1"),
            b"view_1"
        );
        assert_eq!(
            merged_config.display_view_rule("DISP_1", "VIEW_1"),
            b"RULE_1"
        );

        assert_eq!(
            merged_config.view_of_type(VIEW_SHARED, "DISP_1", 0),
            b"SHARED_1"
        );
        assert_eq!(
            merged_config.display_view_color_space_name("DISP_1", "SHARED_1"),
            b"lin_1"
        );

        assert_eq!(
            merged_config.view_of_type(VIEW_SHARED, "DISP_1", 1),
            b"SHARED_2"
        );
        assert_eq!(
            merged_config.display_view_color_space_name("DISP_1", "SHARED_2"),
            b"<USE_DISPLAY_NAME>"
        );
        assert_eq!(
            merged_config.display_view_transform_name("DISP_1", "SHARED_2"),
            b"SDR Video"
        );

        assert_eq!(
            merged_config.view_of_type(VIEW_SHARED, "DISP_1", 2),
            b"SHARED_3"
        );
        assert_eq!(
            merged_config.display_view_color_space_name("DISP_1", "SHARED_3"),
            b"log_3"
        );

        assert_eq!(
            merged_config.num_views_of_type(VIEW_DISPLAY_DEFINED, "DISP_2"),
            2
        );
        assert_eq!(
            merged_config.view_of_type(VIEW_DISPLAY_DEFINED, "DISP_2", 0),
            b"VIEW_1"
        );
        assert_eq!(
            merged_config.display_view_rule("DISP_2", "VIEW_1"),
            b"RULE_2"
        );
        assert_eq!(
            merged_config.view_of_type(VIEW_DISPLAY_DEFINED, "DISP_2", 1),
            b"VIEW_2"
        );
        assert_eq!(
            merged_config.display_view_looks("DISP_2", "VIEW_2"),
            b"look_base"
        );

        assert_eq!(
            merged_config.num_views_of_type(VIEW_DISPLAY_DEFINED, "DISP_3"),
            2
        );
        assert_eq!(
            merged_config.view_of_type(VIEW_DISPLAY_DEFINED, "DISP_3", 0),
            b"VIEW_1"
        );
        assert_eq!(
            merged_config.view_of_type(VIEW_DISPLAY_DEFINED, "DISP_3", 1),
            b"VIEW_3"
        );
        assert_eq!(
            merged_config.display_view_looks("DISP_3", "VIEW_3"),
            b"look_input"
        );

        // Validate viewing_rules
        let rules = merged_config.viewing_rules().get();

        assert_eq!(rules.num_entries(), 3);

        assert_eq!(rules.name(0).unwrap(), b"RULE_1");
        assert_eq!(rules.num_color_spaces(0).unwrap(), 1);
        assert_eq!(rule_color_space(&rules, 0, 0), b"Gamma 2.2 AP1 - Texture");

        assert_eq!(rules.name(1).unwrap(), b"RULE_2");
        assert_eq!(rules.num_encodings(1).unwrap(), 1);
        assert_eq!(rule_encoding(&rules, 1, 0), b"scene-linear");

        assert_eq!(rules.name(2).unwrap(), b"RULE_3");
        assert_eq!(rules.num_color_spaces(2).unwrap(), 2);
        assert_eq!(rule_color_space(&rules, 2, 0), b"Linear Rec.2020");
        assert_eq!(rule_color_space(&rules, 2, 1), b"ACEScct - SomeOtherName");

        // Validate virtual_display
        assert_eq!(
            merged_config.virtual_display_num_views(VIEW_DISPLAY_DEFINED),
            3
        );
        assert_eq!(merged_config.virtual_display_num_views(VIEW_SHARED), 2);

        assert_eq!(
            merged_config.virtual_display_view(VIEW_DISPLAY_DEFINED, 0),
            b"ACES"
        );
        assert_eq!(
            merged_config.virtual_display_view(VIEW_DISPLAY_DEFINED, 1),
            b"Log"
        );
        assert_eq!(
            merged_config.virtual_display_view(VIEW_DISPLAY_DEFINED, 2),
            b"Lin"
        );

        assert_eq!(
            merged_config.virtual_display_view(VIEW_SHARED, 0),
            b"SHARED_1"
        );
        assert_eq!(
            merged_config.virtual_display_view(VIEW_SHARED, 1),
            b"SHARED_3"
        );
    }

    // Test display/views with strategy = BaseOnly.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::BaseOnly);
        let mut merged_config = (*base_config).clone();
        merge_section!(
            DisplayViewMerger,
            &base_config,
            &input_config,
            &params,
            &mut merged_config
        )
        .unwrap();

        assert_eq!(merged_config.active_displays(), b"DISP_1, DISP_2");
        assert_eq!(
            merged_config.active_views(),
            b"SHARED_1, SHARED_2, VIEW_1, VIEW_2"
        );

        // Validate shared_views
        assert_eq!(merged_config.num_views_of_type(VIEW_SHARED, b""), 2);
        assert_eq!(merged_config.view_of_type(VIEW_SHARED, b"", 0), b"SHARED_1");
        assert_eq!(merged_config.view_of_type(VIEW_SHARED, b"", 1), b"SHARED_2");

        // Validate displays
        assert_eq!(merged_config.num_displays_all(), 2);
        assert_eq!(merged_config.display(0), b"DISP_1");
        assert_eq!(merged_config.display(1), b"DISP_2");

        // Validate display/views

        assert_eq!(
            merged_config.num_views_of_type(VIEW_DISPLAY_DEFINED, "DISP_1"),
            1
        );
        assert_eq!(merged_config.num_views_of_type(VIEW_SHARED, "DISP_1"), 2);
        assert_eq!(
            merged_config.view_of_type(VIEW_DISPLAY_DEFINED, "DISP_1", 0),
            b"VIEW_1"
        );
        // Make sure this is the right VIEW_1 by checking the colorspace.
        assert_eq!(
            merged_config.display_view_color_space_name("DISP_1", "VIEW_1"),
            b"view_1"
        );
        assert_eq!(
            merged_config.display_view_rule("DISP_1", "VIEW_1"),
            b"RULE_1"
        );

        assert_eq!(
            merged_config.view_of_type(VIEW_SHARED, "DISP_1", 0),
            b"SHARED_1"
        );
        assert_eq!(
            merged_config.display_view_color_space_name("DISP_1", "SHARED_1"),
            b"lin_1"
        );

        assert_eq!(
            merged_config.view_of_type(VIEW_SHARED, "DISP_1", 1),
            b"SHARED_2"
        );
        assert_eq!(
            merged_config.display_view_color_space_name("DISP_1", "SHARED_2"),
            b"<USE_DISPLAY_NAME>"
        );
        assert_eq!(
            merged_config.display_view_transform_name("DISP_1", "SHARED_2"),
            b"SDR Video"
        );

        assert_eq!(
            merged_config.num_views_of_type(VIEW_DISPLAY_DEFINED, "DISP_2"),
            2
        );
        assert_eq!(
            merged_config.view_of_type(VIEW_DISPLAY_DEFINED, "DISP_2", 0),
            b"VIEW_1"
        );
        assert_eq!(
            merged_config.display_view_rule("DISP_2", "VIEW_1"),
            b"RULE_2"
        );
        assert_eq!(
            merged_config.view_of_type(VIEW_DISPLAY_DEFINED, "DISP_2", 1),
            b"VIEW_2"
        );
        assert_eq!(
            merged_config.display_view_looks("DISP_2", "VIEW_2"),
            b"look_base"
        );

        // Validate viewing_rules
        let rules = merged_config.viewing_rules().get();

        assert_eq!(rules.num_entries(), 2);

        assert_eq!(rules.name(0).unwrap(), b"RULE_1");
        assert_eq!(rules.num_color_spaces(0).unwrap(), 1);
        assert_eq!(rule_color_space(&rules, 0, 0), b"Gamma 2.2 AP1 - Texture");

        assert_eq!(rules.name(1).unwrap(), b"RULE_2");
        assert_eq!(rules.num_encodings(1).unwrap(), 1);
        assert_eq!(rule_encoding(&rules, 1, 0), b"scene-linear");

        // Validate virtual_display
        assert_eq!(
            merged_config.virtual_display_num_views(VIEW_DISPLAY_DEFINED),
            2
        );
        assert_eq!(merged_config.virtual_display_num_views(VIEW_SHARED), 1);

        assert_eq!(
            merged_config.virtual_display_view(VIEW_DISPLAY_DEFINED, 0),
            b"ACES"
        );
        assert_eq!(
            merged_config.virtual_display_view(VIEW_DISPLAY_DEFINED, 1),
            b"Log"
        );

        assert_eq!(
            merged_config.virtual_display_view(VIEW_SHARED, 0),
            b"SHARED_1"
        );
    }

    // Test display/views with strategy = InputOnly.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::InputOnly);
        let mut merged_config = (*base_config).clone();
        merge_section!(
            DisplayViewMerger,
            &base_config,
            &input_config,
            &params,
            &mut merged_config
        )
        .unwrap();

        assert_eq!(merged_config.active_displays(), b"DISP_1, DISP_3");
        assert_eq!(
            merged_config.active_views(),
            b"SHARED_1, SHARED_3, VIEW_1, VIEW_3"
        );

        // Validate shared_views
        assert_eq!(merged_config.num_views_of_type(VIEW_SHARED, b""), 2);
        assert_eq!(merged_config.view_of_type(VIEW_SHARED, b"", 0), b"SHARED_1");
        assert_eq!(merged_config.view_of_type(VIEW_SHARED, b"", 1), b"SHARED_3");

        // Validate displays
        assert_eq!(merged_config.num_displays_all(), 2);
        assert_eq!(merged_config.display(0), b"DISP_1");
        assert_eq!(merged_config.display(1), b"DISP_3");

        // Validate display/views

        assert_eq!(
            merged_config.num_views_of_type(VIEW_DISPLAY_DEFINED, "DISP_1"),
            1
        );
        assert_eq!(merged_config.num_views_of_type(VIEW_SHARED, "DISP_1"), 2);
        assert_eq!(
            merged_config.view_of_type(VIEW_DISPLAY_DEFINED, "DISP_1", 0),
            b"VIEW_1"
        );
        // Make sure this is the right VIEW_1 by checking the colorspace.
        assert_eq!(
            merged_config.display_view_color_space_name("DISP_1", "VIEW_1"),
            b"view_1B"
        );
        assert_eq!(
            merged_config.display_view_rule("DISP_1", "VIEW_1"),
            b"RULE_3"
        );

        assert_eq!(
            merged_config.view_of_type(VIEW_SHARED, "DISP_1", 0),
            b"SHARED_3"
        );
        assert_eq!(
            merged_config.display_view_color_space_name("DISP_1", "SHARED_3"),
            b"log_3"
        );

        assert_eq!(
            merged_config.view_of_type(VIEW_SHARED, "DISP_1", 1),
            b"SHARED_1"
        );
        assert_eq!(
            merged_config.display_view_color_space_name("DISP_1", "SHARED_1"),
            b"lin_3"
        );

        assert_eq!(
            merged_config.num_views_of_type(VIEW_DISPLAY_DEFINED, "DISP_3"),
            2
        );
        assert_eq!(
            merged_config.view_of_type(VIEW_DISPLAY_DEFINED, "DISP_3", 0),
            b"VIEW_1"
        );
        assert_eq!(
            merged_config.view_of_type(VIEW_DISPLAY_DEFINED, "DISP_3", 1),
            b"VIEW_3"
        );
        assert_eq!(
            merged_config.display_view_looks("DISP_3", "VIEW_3"),
            b"look_input"
        );

        // Validate viewing_rules
        let rules = merged_config.viewing_rules().get();

        assert_eq!(rules.num_entries(), 2);

        assert_eq!(rules.name(0).unwrap(), b"RULE_1");
        assert_eq!(rules.num_color_spaces(0).unwrap(), 1);
        assert_eq!(rule_color_space(&rules, 0, 0), b"sRGB - Texture");

        assert_eq!(rules.name(1).unwrap(), b"RULE_3");
        assert_eq!(rules.num_color_spaces(1).unwrap(), 2);
        assert_eq!(rule_color_space(&rules, 1, 0), b"Linear Rec.2020");
        assert_eq!(rule_color_space(&rules, 1, 1), b"ACEScct - SomeOtherName");

        // Validate virtual_display
        assert_eq!(
            merged_config.virtual_display_num_views(VIEW_DISPLAY_DEFINED),
            2
        );
        assert_eq!(merged_config.virtual_display_num_views(VIEW_SHARED), 1);

        assert_eq!(
            merged_config.virtual_display_view(VIEW_DISPLAY_DEFINED, 0),
            b"ACES"
        );
        assert_eq!(
            merged_config.virtual_display_view(VIEW_DISPLAY_DEFINED, 1),
            b"Lin"
        );

        assert_eq!(
            merged_config.virtual_display_view(VIEW_SHARED, 0),
            b"SHARED_3"
        );
    }

    // Test display/views with strategy = Remove
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::Remove);
        let mut merged_config = (*base_config).clone();
        merge_section!(
            DisplayViewMerger,
            &base_config,
            &input_config,
            &params,
            &mut merged_config
        )
        .unwrap();

        assert_eq!(merged_config.active_displays(), b"DISP_2");
        assert_eq!(merged_config.active_views(), b"SHARED_2, VIEW_2");

        // Validate shared_views
        assert_eq!(merged_config.num_views_of_type(VIEW_SHARED, b""), 1);
        assert_eq!(merged_config.view_of_type(VIEW_SHARED, b"", 0), b"SHARED_2");

        // Validate displays
        assert_eq!(merged_config.num_displays_all(), 2);
        assert_eq!(merged_config.display_all(0), b"DISP_1");
        assert_eq!(merged_config.display_all(1), b"DISP_2");

        // Validate display/views
        assert_eq!(
            merged_config.num_views_of_type(VIEW_DISPLAY_DEFINED, "DISP_1"),
            0
        );
        assert_eq!(merged_config.num_views_of_type(VIEW_SHARED, "DISP_1"), 1);
        assert_eq!(
            merged_config.view_of_type(VIEW_SHARED, "DISP_1", 0),
            b"SHARED_2"
        );
        assert_eq!(
            merged_config.display_view_color_space_name("DISP_1", "SHARED_2"),
            b"<USE_DISPLAY_NAME>"
        );
        assert_eq!(
            merged_config.display_view_transform_name("DISP_1", "SHARED_2"),
            b"SDR Video"
        );

        assert_eq!(
            merged_config.num_views_of_type(VIEW_DISPLAY_DEFINED, "DISP_2"),
            2
        );
        assert_eq!(
            merged_config.view_of_type(VIEW_DISPLAY_DEFINED, "DISP_2", 0),
            b"VIEW_1"
        );
        assert_eq!(
            merged_config.display_view_rule("DISP_2", "VIEW_1"),
            b"RULE_2"
        );
        assert_eq!(
            merged_config.view_of_type(VIEW_DISPLAY_DEFINED, "DISP_2", 1),
            b"VIEW_2"
        );
        assert_eq!(
            merged_config.display_view_looks("DISP_2", "VIEW_2"),
            b"look_base"
        );

        // Validate viewing_rules
        let rules = merged_config.viewing_rules().get();
        assert_eq!(rules.num_entries(), 1);
        assert_eq!(rules.name(0).unwrap(), b"RULE_2");
        assert_eq!(rules.num_encodings(0).unwrap(), 1);
        assert_eq!(rule_encoding(&rules, 0, 0), b"scene-linear");

        // Validate virtual_display
        assert_eq!(
            merged_config.virtual_display_num_views(VIEW_DISPLAY_DEFINED),
            1
        );
        assert_eq!(merged_config.virtual_display_num_views(VIEW_SHARED), 1);

        assert_eq!(
            merged_config.virtual_display_view(VIEW_DISPLAY_DEFINED, 0),
            b"Log"
        );
        assert_eq!(
            merged_config.virtual_display_view(VIEW_SHARED, 0),
            b"SHARED_1"
        );
    }

    // Test that error_on_conflicts is processed correctly.
    // strategy = PreferInput, InputFirst = false
    {
        let mut merger = ConfigMerger::new();
        let mut merged_config = (*base_config).clone();

        let params = ConfigMergingParameters::new().unwrap();
        merger.add_params(Some(params.clone()));
        merger
            .params(0)
            .unwrap()
            .set_display_views(MergeStrategies::PreferInput);
        merger.params(0).unwrap().set_input_first(false);
        merger.params(0).unwrap().set_error_on_conflict(true);

        const PREFIX: &str =
            "The Input config contains a value that would override the Base config: ";

        // Test that an error is thrown when the input config values are different.
        {
            check_for_log_or_exception(
                LogType::Error,
                || {
                    merge_section!(
                        DisplayViewMerger,
                        &base_config,
                        &input_config,
                        &params,
                        &mut merged_config
                    )
                },
                &[
                    &format!("{PREFIX}shared_views: SHARED_1"),
                    &format!("{PREFIX}display: DISP_1, views: VIEW_1"),
                    &format!("{PREFIX}viewing_rules: RULE_1"),
                    &format!("{PREFIX}virtual_display: ACES"),
                ],
            );
        }
    }
}
