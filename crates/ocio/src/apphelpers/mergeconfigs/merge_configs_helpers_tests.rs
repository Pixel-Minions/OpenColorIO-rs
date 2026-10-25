// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Port of `tests/cpu/apphelpers/MergeConfigsHelpers_tests.cpp` @ v2.5.2: the OCIOM tests
//! (9.2c), the section tests, each with its merger (9.2d-9.2i), and the merges (9.2j).

use super::*;
use crate::apphelpers::mergeconfigs::section_merger::{
    ColorspacesMerger, DisplayViewMerger, FileRulesMerger, GeneralMerger, LooksMerger,
    MergeHandlerOptions, NamedTransformsMerger, RolesMerger, Section, ViewTransformsMerger,
};
use crate::color_space::ColorSpace;
use crate::context::Context;
use crate::file_rules::FileRules;
use crate::fileformats::test_utils::get_test_files_dir;
use crate::named_transform::NamedTransform;
use crate::transform::{Transform, TransformType};
use crate::viewing_rules::ViewingRules;
use ocio_ops::logging::{get_logging_level, set_logging_level};
use ocio_ops::open_color_types::{
    ColorSpaceDirection, ColorSpaceVisibility, LoggingLevel, NamedTransformVisibility,
    SearchReferenceSpaceType, ViewTransformDirection, ViewType,
};
use ocio_ops::std_regex::{Regex, regex_replace};
use ocio_ops::utils::pystring::os_path;
use ocio_testkit::upstream::{check_close, check_throw_what};

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

/// Upstream's `setupBasics` of the view transforms section's test: a merger with new
/// parameters, whose view transforms strategy is `strategy`; the parameters.
fn setup_view_transforms_basics(
    merger: &mut ConfigMerger,
    strategy: MergeStrategies,
) -> ConfigMergingParameters {
    let params = ConfigMergingParameters::new().unwrap();
    merger.add_params(Some(params.clone()));
    merger.params(0).unwrap().set_view_transforms(strategy);
    params
}

/// The style of the built-in transform from the reference of the view transform `name`.
fn vt_from_ref_builtin_style(config: &Config, name: &str) -> &'static [u8] {
    let tf = config
        .view_transform(name)
        .unwrap()
        .transform(ViewTransformDirection::FromReference)
        .unwrap();
    let Transform::Builtin(bi) = tf else {
        panic!("not a BuiltinTransform: {tf}");
    };
    bi.style()
}

/// Port of `OCIO_ADD_TEST(MergeConfigs, view_transforms_section)` @ v2.5.2.
#[test]
fn view_transforms_section() {
    let base_config = get_base_config().unwrap();
    let input_config = get_input_config().unwrap();

    let setup_basics = setup_view_transforms_basics;

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
            ViewTransformsMerger,
            &base_config,
            &input_config,
            &params,
            &mut merged_config
        )
        .unwrap();

        assert_eq!(
            merged_config.default_view_transform_name(),
            b"Un-tone-mapped-2"
        );

        // Validate view_transforms

        assert_eq!(merged_config.num_view_transforms(), 3);
        assert_eq!(merged_config.view_transform_name_by_index(0), b"SDR Video");
        assert_eq!(
            merged_config.view_transform_name_by_index(1),
            b"Un-tone-mapped-2"
        );
        assert_eq!(merged_config.view_transform_name_by_index(2), b"Equal");
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
                    ViewTransformsMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
            },
            &[
                "The Input config contains a value that would override the Base config: view_transforms: SDR Video",
                "The Input config contains a value that would override the Base config: default_view_transform: Un-tone-mapped-2",
            ],
        );

        // Validate default_view_transform
        assert_eq!(
            merged_config.default_view_transform_name(),
            b"Un-tone-mapped-2"
        );

        // Validate view_transforms

        assert_eq!(merged_config.num_view_transforms(), 4);
        assert_eq!(merged_config.view_transform_name_by_index(0), b"SDR Video");
        // Note that the style of "SDR Video" differs between input and base, which is why a warning is
        // logged for that one, whereas the transform for "Equal" is the same in input and base.
        assert_eq!(
            vt_from_ref_builtin_style(&merged_config, "SDR Video"),
            b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-VIDEO-P3lim_1.1"
        );

        assert_eq!(
            merged_config.view_transform_name_by_index(1),
            b"Un-tone-mapped-2"
        );
        assert_eq!(merged_config.view_transform_name_by_index(2), b"Equal");
        assert_eq!(
            merged_config.view_transform_name_by_index(3),
            b"Un-tone-mapped"
        );
        assert_eq!(
            merged_config.view_transform("Equal").unwrap().description(),
            b"from input"
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
                    ViewTransformsMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
            },
            &[
                "The Input config contains a value that would override the Base config: view_transforms: SDR Video",
                "The Input config contains a value that would override the Base config: default_view_transform: Un-tone-mapped-2",
            ],
        );

        assert_eq!(
            merged_config.default_view_transform_name(),
            b"Un-tone-mapped-2"
        );

        // Validate view_transforms

        assert_eq!(merged_config.num_view_transforms(), 4);
        assert_eq!(merged_config.view_transform_name_by_index(0), b"SDR Video");
        assert_eq!(
            vt_from_ref_builtin_style(&merged_config, "SDR Video"),
            b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-VIDEO-P3lim_1.1"
        );

        assert_eq!(
            merged_config.view_transform_name_by_index(1),
            b"Un-tone-mapped"
        );
        assert_eq!(merged_config.view_transform_name_by_index(2), b"Equal");
        assert_eq!(
            merged_config.view_transform_name_by_index(3),
            b"Un-tone-mapped-2"
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
                    ViewTransformsMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
            },
            &[
                "The Input config contains a value that would override the Base config: view_transforms: SDR Video",
                "The Input config contains a value that would override the Base config: default_view_transform: Un-tone-mapped-2",
            ],
        );

        assert_eq!(merged_config.default_view_transform_name(), b"SDR Video");

        // Validate view_transforms

        assert_eq!(merged_config.num_view_transforms(), 4);
        assert_eq!(merged_config.view_transform_name_by_index(0), b"SDR Video");
        assert_eq!(
            vt_from_ref_builtin_style(&merged_config, "SDR Video"),
            b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-VIDEO_1.0"
        );

        assert_eq!(
            merged_config.view_transform_name_by_index(1),
            b"Un-tone-mapped-2"
        );
        assert_eq!(merged_config.view_transform_name_by_index(2), b"Equal");
        assert_eq!(
            merged_config.view_transform_name_by_index(3),
            b"Un-tone-mapped"
        );
        assert_eq!(
            merged_config.view_transform("Equal").unwrap().description(),
            b"from base"
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
                    ViewTransformsMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
            },
            &[
                "The Input config contains a value that would override the Base config: view_transforms: SDR Video",
                "The Input config contains a value that would override the Base config: default_view_transform: Un-tone-mapped-2",
            ],
        );

        assert_eq!(merged_config.default_view_transform_name(), b"SDR Video");

        // Validate view_transforms

        assert_eq!(merged_config.num_view_transforms(), 4);
        assert_eq!(merged_config.view_transform_name_by_index(0), b"SDR Video");
        assert_eq!(
            vt_from_ref_builtin_style(&merged_config, "SDR Video"),
            b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-VIDEO_1.0"
        );

        assert_eq!(
            merged_config.view_transform_name_by_index(1),
            b"Un-tone-mapped"
        );
        assert_eq!(merged_config.view_transform_name_by_index(2), b"Equal");
        assert_eq!(
            merged_config.view_transform_name_by_index(3),
            b"Un-tone-mapped-2"
        );
    }

    // Test display/views with strategy = BaseOnly.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::BaseOnly);
        let mut merged_config = (*base_config).clone();
        merge_section!(
            ViewTransformsMerger,
            &base_config,
            &input_config,
            &params,
            &mut merged_config
        )
        .unwrap();

        assert_eq!(merged_config.default_view_transform_name(), b"SDR Video");

        // Validate view_transforms

        assert_eq!(merged_config.num_view_transforms(), 3);
        assert_eq!(merged_config.view_transform_name_by_index(0), b"SDR Video");
        assert_eq!(
            merged_config.view_transform_name_by_index(1),
            b"Un-tone-mapped"
        );
        assert_eq!(merged_config.view_transform_name_by_index(2), b"Equal");
    }

    // Test display/views with strategy = InputOnly.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::InputOnly);
        let mut merged_config = (*base_config).clone();
        merge_section!(
            ViewTransformsMerger,
            &base_config,
            &input_config,
            &params,
            &mut merged_config
        )
        .unwrap();

        assert_eq!(
            merged_config.default_view_transform_name(),
            b"Un-tone-mapped-2"
        );

        // Validate view_transforms

        assert_eq!(merged_config.num_view_transforms(), 3);
        assert_eq!(merged_config.view_transform_name_by_index(0), b"SDR Video");
        assert_eq!(
            vt_from_ref_builtin_style(&merged_config, "SDR Video"),
            b"ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-VIDEO-P3lim_1.1"
        );

        assert_eq!(
            merged_config.view_transform_name_by_index(1),
            b"Un-tone-mapped-2"
        );
        assert_eq!(merged_config.view_transform_name_by_index(2), b"Equal");
    }

    // Test display/views with strategy = Remove
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::Remove);
        let mut merged_config = (*base_config).clone();
        merge_section!(
            ViewTransformsMerger,
            &base_config,
            &input_config,
            &params,
            &mut merged_config
        )
        .unwrap();

        // Note that the "SDR Video" view transform was removed, so the "SDR Video" value
        // of the default view transform was reset to empty (will use the first one by default).
        assert_eq!(merged_config.default_view_transform_name(), b"");

        // Validate view_transforms

        assert_eq!(merged_config.num_view_transforms(), 1);
        assert_eq!(
            merged_config.view_transform_name_by_index(0),
            b"Un-tone-mapped"
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
            .set_view_transforms(MergeStrategies::PreferInput);
        merger.params(0).unwrap().set_input_first(false);
        merger.params(0).unwrap().set_error_on_conflict(true);

        const PREFIX: &str =
            "The Input config contains a value that would override the Base config: ";

        // Test that an error is thrown when the the input values are different.
        {
            check_for_log_or_exception(
                LogType::Error,
                || {
                    merge_section!(
                        ViewTransformsMerger,
                        &base_config,
                        &input_config,
                        &params,
                        &mut merged_config
                    )
                },
                &[
                    &format!("{PREFIX}view_transforms: SDR Video"),
                    &format!("{PREFIX}default_view_transform: Un-tone-mapped-2"),
                ],
            );
        }
    }
}

/// Port of `getConfig` (MergeConfigsHelpers_tests.cpp:42-52 @ v2.5.2).
fn get_config(filename: &str) -> Result<Arc<Config>> {
    Config::create_from_file(merge_test_file(filename))
}

/// Port of `compareEnvironmentVar` (MergeConfigsHelpers_tests.cpp:54-67 @ v2.5.2).
#[track_caller]
fn compare_environment_var(
    merged_config: &Config,
    expected_names: &[&str],
    expected_values: &[&str],
) {
    for i in 0..merged_config.num_environment_vars() {
        let name = merged_config.environment_var_name_by_index(i);
        assert_eq!(name, expected_names[i as usize].as_bytes());
        assert_eq!(
            merged_config.environment_var_default(name),
            expected_values[i as usize].as_bytes()
        );
    }
}

/// Port of `checkColorSpace` (MergeConfigsHelpers_tests.cpp:69-81 @ v2.5.2).
#[track_caller]
fn check_color_space<'c>(
    merged_config: &'c Config,
    ref_name: &str,
    index: i32,
    ref_type: SearchReferenceSpaceType,
) -> &'c ColorSpace {
    let name =
        merged_config.color_space_name_by_index_with(ref_type, ColorSpaceVisibility::All, index);
    assert_eq!(name, ref_name.as_bytes());
    merged_config
        .color_space(ref_name)
        .unwrap_or_else(|| panic!("no color space {ref_name}"))
}

const SEARCH_DISPLAY: SearchReferenceSpaceType = SearchReferenceSpaceType::Display;
const SEARCH_SCENE: SearchReferenceSpaceType = SearchReferenceSpaceType::Scene;

/// The number of color spaces of `config`, all reference space types, inactive ones included.
fn num_all_color_spaces(config: &Config) -> i32 {
    config.num_color_spaces_with(SearchReferenceSpaceType::All, ColorSpaceVisibility::All)
}

/// Upstream's `setupBasics` of the overrides test: a merger with new parameters, whose
/// displays and views and color spaces strategies are `strategy`, with every override set.
fn setup_overrides_basics(
    merger: &mut ConfigMerger,
    strategy: MergeStrategies,
) -> ConfigMergingParameters {
    let params = ConfigMergingParameters::new().unwrap();
    merger.add_params(Some(params.clone()));
    let p = merger.params(0).unwrap();
    p.set_display_views(strategy);
    p.set_colorspaces(strategy);
    // Not looking for duplicates as this test does not test that.
    p.set_avoid_duplicates(false);

    // Set the overrides.
    p.set_name("OVR Name");
    p.set_description("OVR Desc");
    p.set_search_path("OVR1,OVR2");
    p.add_environment_var("OVR1", Some(b"VALUE1".as_slice()));
    p.add_environment_var("OVR2", Some(b"VALUE2".as_slice()));
    p.set_active_displays("OVR DISP 1,OVR DISP 2").unwrap();
    p.set_active_views("OVR VIEW 1,OVR VIEW 2").unwrap();
    p.set_inactive_color_spaces("view_1, ACES2065-1");

    params
}

/// Upstream's `doTests` of the overrides test.
#[track_caller]
fn check_overrides(merged_config: &Config) {
    assert_eq!(merged_config.name(), b"OVR Name");
    assert_eq!(merged_config.description(), b"OVR Desc");

    assert_eq!(merged_config.search_path(), b"OVR1,OVR2");

    let expected_names = ["OVR1", "OVR2"];
    let expected_values = ["VALUE1", "VALUE2"];
    assert_eq!(merged_config.num_environment_vars(), 2);
    compare_environment_var(merged_config, &expected_names, &expected_values);

    assert_eq!(merged_config.active_displays(), b"OVR DISP 1, OVR DISP 2");
    assert_eq!(merged_config.active_views(), b"OVR VIEW 1, OVR VIEW 2");

    assert_eq!(merged_config.inactive_color_spaces(), b"view_1, ACES2065-1");
}

/// The general, displays and views and color spaces mergers of the overrides test, in order.
fn merge_overrides_sections(
    base_config: &Config,
    input_config: &Config,
    params: &ConfigMergingParameters,
    merged_config: &mut Config,
) -> Result<()> {
    // Merge name and description.
    merge_section!(
        GeneralMerger,
        base_config,
        input_config,
        params,
        merged_config
    )?;
    // Merge active_display, active_views.
    merge_section!(
        DisplayViewMerger,
        base_config,
        input_config,
        params,
        merged_config
    )?;
    // Merge inactive_colorspaces, environment and search_path.
    merge_section!(
        ColorspacesMerger,
        base_config,
        input_config,
        params,
        merged_config
    )
}

/// Port of `OCIO_ADD_TEST(MergeConfigs, overrides)` @ v2.5.2.
#[test]
fn overrides() {
    let base_config = get_base_config().unwrap();
    let input_config = get_input_config().unwrap();

    // Test that the overrides options are taken into account in the merging process.

    let setup_basics = setup_overrides_basics;
    let do_tests = check_overrides;

    // Test sections with strategy = PreferInput
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
        let mut merged_config = (*base_config).clone();
        check_for_log_or_exception(
            LogType::Warning,
            || merge_overrides_sections(&base_config, &input_config, &params, &mut merged_config),
            &[
                "The Input config contains a value that would override the Base config: shared_views: SHARED_1",
                "The Input config contains a value that would override the Base config: display: DISP_1, view: VIEW_1",
                "The Input config contains a value that would override the Base config: viewing_rules: RULE_1",
                "Color space 'ACES2065-1' will replace a color space in the base config.",
                "Color space 'view_1' will replace a color space in the base config.",
            ],
        );
        do_tests(&merged_config);
    }

    // Test sections with strategy = PreferBase.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::PreferBase);
        let mut merged_config = (*base_config).clone();
        check_for_log_or_exception(
            LogType::Warning,
            || merge_overrides_sections(&base_config, &input_config, &params, &mut merged_config),
            &[
                "The Input config contains a value that would override the Base config: shared_views: SHARED_1",
                "The Input config contains a value that would override the Base config: display: DISP_1, view: VIEW_1",
                "The Input config contains a value that would override the Base config: viewing_rules: RULE_1",
                "Color space 'ACES2065-1' was not merged as it's already present in the base config.",
                "Color space 'view_1' was not merged as it's already present in the base config.",
            ],
        );
        do_tests(&merged_config);
    }

    // Test sections with strategy = InputOnly.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::InputOnly);
        let mut merged_config = (*base_config).clone();
        merge_overrides_sections(&base_config, &input_config, &params, &mut merged_config).unwrap();
        do_tests(&merged_config);
    }

    // Test sections with strategy = BaseOnly.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::BaseOnly);
        let mut merged_config = (*base_config).clone();
        merge_overrides_sections(&base_config, &input_config, &params, &mut merged_config).unwrap();
        do_tests(&merged_config);
    }

    // Strategy Remove is not tested as the overrides do not affect that strategy.
}

/// Upstream's `setupBasics` of the color spaces section's test: a merger with new parameters,
/// whose color spaces and default strategies are `strategy`, with family prefixes, without the
/// reference space adjustment and the duplicate search.
fn setup_colorspaces_basics(
    merger: &mut ConfigMerger,
    strategy: MergeStrategies,
) -> ConfigMergingParameters {
    let params = ConfigMergingParameters::new().unwrap();
    merger.add_params(Some(params.clone()));
    let p = merger.params(0).unwrap();
    p.set_colorspaces(strategy);
    p.set_default_strategy(strategy);

    p.set_input_family_prefix("Input/");
    p.set_base_family_prefix("Base/");

    p.set_adjust_input_reference_space(false);
    p.set_avoid_duplicates(false);

    params
}

/// Port of `OCIO_ADD_TEST(MergeConfigs, colorspaces_section)` @ v2.5.2.
#[test]
fn colorspaces_section() {
    let base_config = get_config("base_colorspaces_config.yaml").unwrap();
    let input_config = get_config("input_colorspaces_config.yaml").unwrap();

    let setup_basics = setup_colorspaces_basics;

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
            ColorspacesMerger,
            &base_config,
            &input_config,
            &params,
            &mut merged_config
        )
        .unwrap();

        assert_eq!(merged_config.family_separator(), b'~');

        let expected_names = ["test", "test3"];
        let expected_values = ["differentValue", "value3"];
        compare_environment_var(&merged_config, &expected_names, &expected_values);

        assert_eq!(merged_config.search_path(), b".:def");
        assert_eq!(
            merged_config.inactive_color_spaces(),
            b"ACES2065-1, sRGB - Display"
        );
    }

    // Test Colorspaces with strategy = PreferInput, options InputFirst = true.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::PreferInput);

        let mut merged_config = (*base_config).clone();
        check_for_log_or_exception(
            LogType::Warning,
            || {
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
            },
            &[
                "Color space 'sRGB - Display' will replace a color space in the base config",
                "Color space 'look' will replace a color space in the base config",
                "Merged color space 'look' has a different reference space type than the color space it's replacing",
                "Merged color space 'ACES2065-1' has a conflict with alias 'aces' in color space 'ACEScg'",
                "The name of merged color space 'sRGB' has a conflict with an alias in color space 'sRGB - Texture'",
            ],
        );

        assert_eq!(merged_config.family_separator(), b'~');

        // Note that the environment vars are always written in alphabetical order,
        // so the InputFirst directive doesn't apply to this specific element.
        let expected_names = ["test", "test1", "test3"];
        let expected_values = ["differentValue", "value1", "value3"];
        compare_environment_var(&merged_config, &expected_names, &expected_values);

        assert_eq!(merged_config.search_path(), b".:def:abc");
        assert_eq!(
            merged_config.inactive_color_spaces(),
            b"ACES2065-1, sRGB - Display, sRGB - Texture, ACEScg"
        );

        assert_eq!(num_all_color_spaces(&merged_config), 6);

        // Display colorspaces.

        let cs = check_color_space(&merged_config, "sRGB - Display", 0, SEARCH_DISPLAY);
        assert_eq!(cs.num_aliases(), 1);
        assert_eq!(cs.alias(0), b"srgb_display");
        assert_eq!(cs.family(), b"Input~Display~Standard");
        assert_eq!(cs.description(), b"from input");

        let cs = check_color_space(&merged_config, "look", 1, SEARCH_DISPLAY);
        assert_eq!(cs.num_aliases(), 1);
        assert_eq!(cs.alias(0), b"look1");
        assert_eq!(cs.description(), b"from input");

        // Scene colorspaces.

        let cs = check_color_space(&merged_config, "ACES2065-1", 0, SEARCH_SCENE);
        assert_eq!(cs.num_aliases(), 1);
        assert_eq!(cs.alias(0), b"aces");
        assert_eq!(cs.family(), b"Input~ACES~Linear");

        let cs = check_color_space(&merged_config, "sRGB", 1, SEARCH_SCENE);
        assert_eq!(cs.num_aliases(), 1);
        assert_eq!(cs.alias(0), b"my_srgb");
        assert_eq!(cs.family(), b"Input~Texture~");

        let cs = check_color_space(&merged_config, "ACEScg", 2, SEARCH_SCENE);
        assert_eq!(cs.num_aliases(), 0);
        assert_eq!(cs.family(), b"Base~ACES~Linear");

        let cs = check_color_space(&merged_config, "sRGB - Texture", 3, SEARCH_SCENE);
        // Note "srgb" is removed as an alias since it is a color space name in the input config.
        assert_eq!(cs.num_aliases(), 1);
        assert_eq!(cs.alias(0), b"srgb_tx");
        assert_eq!(cs.family(), b"Base~Texture");
        assert_eq!(cs.description(), b"from base");

        // Note that the "look" scene color space is not merged since there is already a display color space
        // with that name.
    }

    // Test Colorspaces with strategy=PreferInput, options InputFirst = false.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
        merger.params(0).unwrap().set_input_first(false);

        let mut merged_config = (*base_config).clone();
        check_for_log_or_exception(
            LogType::Warning,
            || {
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
            },
            &[
                "Color space 'sRGB - Display' will replace a color space in the base config",
                "Color space 'look' will replace a color space in the base config",
                "Merged color space 'look' has a different reference space type than the color space it's replacing",
                "Merged color space 'ACES2065-1' has a conflict with alias 'aces' in color space 'ACEScg'",
                "The name of merged color space 'sRGB' has a conflict with an alias in color space 'sRGB - Texture'",
            ],
        );

        assert_eq!(merged_config.family_separator(), b'~');

        let expected_names = ["test", "test1", "test3"];
        let expected_values = ["differentValue", "value1", "value3"];
        compare_environment_var(&merged_config, &expected_names, &expected_values);

        // Note that the search path ignores InputFirst, it works based on the strategy only.
        assert_eq!(merged_config.search_path(), b".:def:abc");
        assert_eq!(
            merged_config.inactive_color_spaces(),
            b"sRGB - Texture, sRGB - Display, ACEScg, ACES2065-1"
        );

        assert_eq!(num_all_color_spaces(&merged_config), 6);

        // Display colorspaces.

        let cs = check_color_space(&merged_config, "sRGB - Display", 0, SEARCH_DISPLAY);
        assert_eq!(cs.alias(0), b"srgb_display");
        assert_eq!(cs.family(), b"Input~Display~Standard");
        assert_eq!(cs.description(), b"from input");

        let cs = check_color_space(&merged_config, "look", 1, SEARCH_DISPLAY);
        assert_eq!(cs.alias(0), b"look1");
        assert_eq!(cs.description(), b"from input");

        // Scene colorspaces.

        let cs = check_color_space(&merged_config, "ACEScg", 0, SEARCH_SCENE);
        assert_eq!(cs.num_aliases(), 0);
        assert_eq!(cs.family(), b"Base~ACES~Linear");

        let cs = check_color_space(&merged_config, "sRGB - Texture", 1, SEARCH_SCENE);
        assert_eq!(cs.num_aliases(), 1);
        assert_eq!(cs.alias(0), b"srgb_tx");
        assert_eq!(cs.family(), b"Base~Texture");
        assert_eq!(cs.description(), b"from base");

        let cs = check_color_space(&merged_config, "ACES2065-1", 2, SEARCH_SCENE);
        assert_eq!(cs.alias(0), b"aces");
        assert_eq!(cs.family(), b"Input~ACES~Linear");

        let cs = check_color_space(&merged_config, "sRGB", 3, SEARCH_SCENE);
        assert_eq!(cs.alias(0), b"my_srgb");
        assert_eq!(cs.family(), b"Input~Texture~");
    }

    // Test Colorspaces with strategy = PreferBase, options InputFirst = true.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::PreferBase);

        let mut merged_config = (*base_config).clone();
        check_for_log_or_exception(
            LogType::Warning,
            || {
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
            },
            &[
                "Color space 'sRGB - Display' was not merged as it's already present in the base config",
                "Color space 'look' was not merged as it's already present in the base config",
                "Merged color space 'ACES2065-1' has a conflict with alias 'aces' in color space 'ACEScg'",
                "Color space 'sRGB' was not merged as it conflicts with an alias in color space 'sRGB - Texture'",
            ],
        );

        assert_eq!(merged_config.family_separator(), b'#');

        let expected_names = ["test", "test1", "test3"];
        let expected_values = ["value", "value1", "value3"];
        compare_environment_var(&merged_config, &expected_names, &expected_values);

        // Note that the search path ignores InputFirst, it works based on the strategy only.
        assert_eq!(merged_config.search_path(), b".:abc:def");
        assert_eq!(
            merged_config.inactive_color_spaces(),
            b"ACES2065-1, sRGB - Display, sRGB - Texture, ACEScg"
        );

        assert_eq!(num_all_color_spaces(&merged_config), 5);

        // Display colorspaces.

        let cs = check_color_space(&merged_config, "sRGB - Display", 0, SEARCH_DISPLAY);
        assert_eq!(cs.alias(0), b"srgb_display");
        assert_eq!(cs.family(), b"Base#Display#Basic");
        assert_eq!(cs.description(), b"from base");

        // Scene colorspaces.

        let cs = check_color_space(&merged_config, "ACES2065-1", 0, SEARCH_SCENE);
        assert_eq!(cs.num_aliases(), 0);
        assert_eq!(cs.family(), b"Input#ACES#Linear");
        assert_eq!(cs.description(), b"from input");

        let cs = check_color_space(&merged_config, "ACEScg", 1, SEARCH_SCENE);
        assert_eq!(cs.num_aliases(), 1);
        assert_eq!(cs.alias(0), b"aces");
        assert_eq!(cs.family(), b"Base#ACES#Linear");

        let cs = check_color_space(&merged_config, "sRGB - Texture", 2, SEARCH_SCENE);
        assert_eq!(cs.num_aliases(), 2);
        assert_eq!(cs.alias(0), b"srgb");
        assert_eq!(cs.alias(1), b"srgb_tx");
        assert_eq!(cs.family(), b"Base#Texture");
        assert_eq!(cs.description(), b"from base");

        let cs = check_color_space(&merged_config, "look", 3, SEARCH_SCENE);
        assert_eq!(cs.num_aliases(), 0);
        assert_eq!(cs.description(), b"from base");
    }

    // Test Colorspaces with strategy = PreferBase, options InputFirst = false.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::PreferBase);
        merger.params(0).unwrap().set_input_first(false);

        let mut merged_config = (*base_config).clone();
        check_for_log_or_exception(
            LogType::Warning,
            || {
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
            },
            &[
                "Color space 'sRGB - Display' was not merged as it's already present in the base config",
                "Color space 'look' was not merged as it's already present in the base config",
                "Merged color space 'ACES2065-1' has a conflict with alias 'aces' in color space 'ACEScg'",
                "Color space 'sRGB' was not merged as it conflicts with an alias in color space 'sRGB - Texture'",
            ],
        );

        assert_eq!(merged_config.family_separator(), b'#');

        let expected_names = ["test", "test1", "test3"];
        let expected_values = ["value", "value1", "value3"];
        compare_environment_var(&merged_config, &expected_names, &expected_values);

        assert_eq!(merged_config.search_path(), b".:abc:def");
        assert_eq!(
            merged_config.inactive_color_spaces(),
            b"sRGB - Texture, sRGB - Display, ACEScg, ACES2065-1"
        );

        assert_eq!(num_all_color_spaces(&merged_config), 5);

        // Display colorspaces.

        let cs = check_color_space(&merged_config, "sRGB - Display", 0, SEARCH_DISPLAY);
        assert_eq!(cs.alias(0), b"srgb_display");
        assert_eq!(cs.family(), b"Base#Display#Basic");
        assert_eq!(cs.description(), b"from base");

        // Scene colorspaces.

        let cs = check_color_space(&merged_config, "ACEScg", 0, SEARCH_SCENE);
        assert_eq!(cs.alias(0), b"aces");

        let cs = check_color_space(&merged_config, "sRGB - Texture", 1, SEARCH_SCENE);
        assert_eq!(cs.num_aliases(), 2);
        assert_eq!(cs.alias(0), b"srgb");
        assert_eq!(cs.alias(1), b"srgb_tx");
        assert_eq!(cs.family(), b"Base#Texture");

        let cs = check_color_space(&merged_config, "look", 2, SEARCH_SCENE);
        assert_eq!(cs.num_aliases(), 0);
        assert_eq!(cs.description(), b"from base");

        let cs = check_color_space(&merged_config, "ACES2065-1", 3, SEARCH_SCENE);
        assert_eq!(cs.num_aliases(), 0);
    }

    // Test Colorspaces with strategy = BaseOnly.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::BaseOnly);

        let mut merged_config = (*base_config).clone();
        merge_section!(
            ColorspacesMerger,
            &base_config,
            &input_config,
            &params,
            &mut merged_config
        )
        .unwrap();

        assert_eq!(merged_config.family_separator(), b'#');

        let expected_names = ["test", "test1"];
        let expected_values = ["value", "value1"];
        compare_environment_var(&merged_config, &expected_names, &expected_values);

        assert_eq!(merged_config.search_path(), b".:abc");
        assert_eq!(
            merged_config.inactive_color_spaces(),
            b"sRGB - Texture, sRGB - Display, ACEScg"
        );

        assert_eq!(num_all_color_spaces(&merged_config), 4);

        let cs = check_color_space(&merged_config, "sRGB - Display", 0, SEARCH_DISPLAY);
        assert_eq!(cs.family(), b"Display#Basic");

        let cs = check_color_space(&merged_config, "ACEScg", 0, SEARCH_SCENE);
        assert_eq!(cs.family(), b"ACES#Linear");

        let cs = check_color_space(&merged_config, "sRGB - Texture", 1, SEARCH_SCENE);
        assert_eq!(cs.family(), b"Texture");

        let cs = check_color_space(&merged_config, "look", 2, SEARCH_SCENE);
        assert_eq!(cs.family(), b"");
    }

    // Test Colorspaces with strategy = InputOnly.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::InputOnly);

        let mut merged_config = (*base_config).clone();
        merge_section!(
            ColorspacesMerger,
            &base_config,
            &input_config,
            &params,
            &mut merged_config
        )
        .unwrap();

        assert_eq!(merged_config.family_separator(), b'~');

        let expected_names = ["test", "test3"];
        let expected_values = ["differentValue", "value3"];
        compare_environment_var(&merged_config, &expected_names, &expected_values);

        assert_eq!(merged_config.search_path(), b".:def");
        assert_eq!(
            merged_config.inactive_color_spaces(),
            b"ACES2065-1, sRGB - Display"
        );

        assert_eq!(num_all_color_spaces(&merged_config), 4);

        let cs = check_color_space(&merged_config, "sRGB - Display", 0, SEARCH_DISPLAY);
        assert_eq!(cs.family(), b"Display~Standard");

        let cs = check_color_space(&merged_config, "look", 1, SEARCH_DISPLAY);
        assert_eq!(cs.family(), b"Display~Standard");

        let cs = check_color_space(&merged_config, "ACES2065-1", 0, SEARCH_SCENE);
        assert_eq!(cs.family(), b"ACES~Linear");

        let cs = check_color_space(&merged_config, "sRGB", 1, SEARCH_SCENE);
        assert_eq!(cs.family(), b"Texture~");
    }

    // Test Colorspaces with strategy = Remove
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::Remove);

        let mut merged_config = (*base_config).clone();
        merge_section!(
            ColorspacesMerger,
            &base_config,
            &input_config,
            &params,
            &mut merged_config
        )
        .unwrap();

        assert_eq!(merged_config.family_separator(), b'#');

        let expected_names = ["test1"];
        let expected_values = ["value1"];
        compare_environment_var(&merged_config, &expected_names, &expected_values);

        assert_eq!(merged_config.search_path(), b"abc");
        assert_eq!(
            merged_config.inactive_color_spaces(),
            b"sRGB - Texture, ACEScg"
        );

        assert_eq!(num_all_color_spaces(&merged_config), 2);

        let cs = check_color_space(&merged_config, "ACEScg", 0, SEARCH_SCENE);
        assert_eq!(cs.family(), b"ACES#Linear");

        let cs = check_color_space(&merged_config, "sRGB - Texture", 1, SEARCH_SCENE);
        assert_eq!(cs.family(), b"Texture");
    }
}

/// Upstream's checks of a transform that must be a group of transforms of `types`, in order:
/// `getTransformType() == TRANSFORM_TYPE_GROUP`, the cast, `getNumTransforms()` and each child's
/// `getTransformType()`.
#[track_caller]
fn check_group_types(t: &Transform, types: &[TransformType]) {
    assert_eq!(t.transform_type(), TransformType::Group);
    let Transform::Group(gtx) = t else {
        panic!("not a GroupTransform");
    };
    assert_eq!(gtx.num_transforms() as usize, types.len());
    for (i, ty) in types.iter().enumerate() {
        assert_eq!(gtx.transform(i as i32).unwrap().transform_type(), *ty);
    }
}

/// Upstream's `setupBasics` of the common reference and duplicates test: a merger with new
/// parameters, whose color spaces and default strategies are `strategy`, with family prefixes,
/// the reference space adjustment and the duplicate search.
fn setup_common_reference_basics(
    merger: &mut ConfigMerger,
    strategy: MergeStrategies,
) -> ConfigMergingParameters {
    let params = ConfigMergingParameters::new().unwrap();
    merger.add_params(Some(params.clone()));
    let p = merger.params(0).unwrap();
    p.set_colorspaces(strategy);
    p.set_default_strategy(strategy);

    p.set_input_family_prefix("Input/");
    p.set_base_family_prefix("Base/");

    p.set_adjust_input_reference_space(true);
    p.set_avoid_duplicates(true);

    params
}

/// The roles, displays and views, view transforms and color spaces mergers, in order.
fn merge_roles_to_colorspaces(
    base_config: &Config,
    input_config: &Config,
    params: &ConfigMergingParameters,
    merged_config: &mut Config,
) -> Result<()> {
    merge_section!(
        RolesMerger,
        base_config,
        input_config,
        params,
        merged_config
    )?;
    merge_section!(
        DisplayViewMerger,
        base_config,
        input_config,
        params,
        merged_config
    )?;
    merge_section!(
        ViewTransformsMerger,
        base_config,
        input_config,
        params,
        merged_config
    )?;
    merge_section!(
        ColorspacesMerger,
        base_config,
        input_config,
        params,
        merged_config
    )
}

/// Port of `OCIO_ADD_TEST(MergeConfigs, colorspaces_section_common_reference_and_duplicates)` @ v2.5.2.
#[test]
fn colorspaces_section_common_reference_and_duplicates() {
    // Base config display ref space: CIE-XYZ-D65, scene ref space: ACES2065-1.
    // Input config display ref space: linear Rec.709, scene ref space: linear Rec.709
    //
    // Both configs have the role: cie_xyz_d65_interchange: CIE-XYZ-D65
    // but not the aces_interchange role, so heuristics will be used for that.
    //
    // The merged configs will contain color spaces from the input config where
    // the reference space has been converted to that of the base config.
    // The base reference spaces are always used, regardless of strategy.
    //
    // Duplicates are removed, even though they use different reference spaces.

    let base_path = merge_test_file("merged1/base1.ocio");
    let input_path = merge_test_file("merged1/input1.ocio");

    let base_config = Config::create_from_file(&base_path).unwrap();
    let input_config = Config::create_from_file(&input_path).unwrap();

    let setup_basics = setup_common_reference_basics;

    // PreferInput, Input first.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
        merger.params(0).unwrap().set_input_first(true);

        let mut merged_config = (*base_config).clone();
        check_for_log_or_exception(
            LogType::Warning,
            || merge_roles_to_colorspaces(&base_config, &input_config, &params, &mut merged_config),
            &[
                "Equivalent input color space 'sRGB - Display' replaces 'sRGB - Display' in the base config, preserving aliases.",
                "Equivalent input color space 'CIE-XYZ-D65' replaces 'CIE-XYZ-D65' in the base config, preserving aliases.",
                "Equivalent input color space 'ACES2065-1' replaces 'ap0' in the base config, preserving aliases.",
                "Equivalent input color space 'sRGB' replaces 'sRGB - Texture' in the base config, preserving aliases.",
                "Merged color space 'ACES2065-1' has a conflict with alias 'aces' in color space 'ACEScg'",
            ],
        );

        assert_eq!(merged_config.num_roles(), 1);
        assert_eq!(
            merged_config.role_color_space("cie_xyz_d65_interchange"),
            b"CIE-XYZ-D65"
        );

        assert_eq!(num_all_color_spaces(&merged_config), 7);

        // Display-referred spaces.
        {
            // This is a duplicate.
            let cs = check_color_space(&merged_config, "sRGB - Display", 0, SEARCH_DISPLAY);
            assert_eq!(cs.num_aliases(), 1);
            // Check for alias srgb_display (added from base config).
            assert_eq!(cs.alias(0), b"srgb_display");
            assert_eq!(cs.description(), b"from input");
            assert_eq!(cs.encoding(), b"sdr-video");
            // Check categories were copied.
            assert_eq!(cs.num_categories(), 3);
            assert!(cs.has_category("file-io"));
            assert!(cs.has_category("texture"));
            assert!(cs.has_category("display"));

            // Check that the input config reference space was converted to the base reference space.
            // See ConfigUtils_tests.cpp for more detailed testing of the reference space conversion.
            {
                assert!(cs.transform(ColorSpaceDirection::ToReference).is_none());
                let t = cs.transform(ColorSpaceDirection::FromReference).unwrap();
                check_group_types(
                    t,
                    &[TransformType::Matrix, TransformType::ExponentWithLinear],
                );
            }

            let cs1 = check_color_space(&merged_config, "CIE-XYZ-D65", 1, SEARCH_DISPLAY);
            assert_eq!(cs1.num_aliases(), 1);
            assert_eq!(cs1.alias(0), b"cie_xyz_d65");
            {
                assert!(cs1.transform(ColorSpaceDirection::ToReference).is_none());
                let t = cs1.transform(ColorSpaceDirection::FromReference).unwrap();
                check_group_types(t, &[TransformType::Matrix, TransformType::Matrix]);
            }
        }

        // Scene-referred spaces.
        {
            // This is recognized as a duplicate, even though the name is different in the two configs.
            let cs = check_color_space(&merged_config, "ACES2065-1", 0, SEARCH_SCENE);
            assert_eq!(cs.num_aliases(), 2);
            assert_eq!(cs.alias(0), b"aces");
            // Check for alias ap0 (added from base config).
            assert_eq!(cs.alias(1), b"ap0");
            // Check categories were copied.
            assert_eq!(cs.num_categories(), 2);
            assert!(cs.has_category("file-io"));
            assert!(cs.has_category("texture"));
            assert_eq!(cs.encoding(), b"scene-linear");
            {
                assert!(cs.transform(ColorSpaceDirection::FromReference).is_none());
                let t = cs.transform(ColorSpaceDirection::ToReference).unwrap();
                check_group_types(t, &[TransformType::Matrix, TransformType::Matrix]);
            }

            // This is recognized as a duplicate, even though the name is different in the two configs.
            let cs1 = check_color_space(&merged_config, "sRGB", 1, SEARCH_SCENE);
            assert_eq!(cs1.num_aliases(), 2);
            // Check for alias sRGB - Texture (added from base config colorspace name).
            assert_eq!(cs1.alias(0), b"sRGB - Texture");
            // Check for alias srgb_tx (added from base config).
            assert_eq!(cs1.alias(1), b"srgb_tx");
            // Check categories were copied.
            assert_eq!(cs1.num_categories(), 2);
            assert!(cs1.has_category("file-io"));
            assert!(cs1.has_category("texture"));
            {
                assert!(cs1.transform(ColorSpaceDirection::FromReference).is_none());
                let t = cs1.transform(ColorSpaceDirection::ToReference).unwrap();
                check_group_types(
                    t,
                    &[TransformType::ExponentWithLinear, TransformType::Matrix],
                );
            }

            let cs2 = check_color_space(&merged_config, "rec709", 2, SEARCH_SCENE);
            assert_eq!(cs2.num_categories(), 1);
            assert!(cs2.has_category("texture"));
            {
                assert!(cs2.transform(ColorSpaceDirection::FromReference).is_none());
                let t = cs2.transform(ColorSpaceDirection::ToReference).unwrap();
                check_group_types(t, &[TransformType::Matrix]);
            }

            let cs3 = check_color_space(&merged_config, "Raw", 3, SEARCH_SCENE);
            assert_eq!(cs3.num_aliases(), 1);
            assert_eq!(cs3.alias(0), b"Utility - Raw");
            assert!(cs3.is_data());
            {
                assert!(cs3.transform(ColorSpaceDirection::ToReference).is_none());
                assert!(cs3.transform(ColorSpaceDirection::FromReference).is_none());
            }

            let cs4 = check_color_space(&merged_config, "ACEScg", 4, SEARCH_SCENE);
            assert_eq!(cs4.num_aliases(), 0);
            assert_eq!(cs4.num_categories(), 0);
            {
                assert!(cs4.transform(ColorSpaceDirection::FromReference).is_none());
                let t = cs4.transform(ColorSpaceDirection::ToReference).unwrap();
                assert_eq!(t.transform_type(), TransformType::Builtin);
            }
        }

        // View transforms.
        {
            assert_eq!(merged_config.num_view_transforms(), 2);
            assert_eq!(merged_config.view_transform_name_by_index(0), b"SDR Video");
            assert_eq!(
                merged_config
                    .view_transform("SDR Video")
                    .unwrap()
                    .description(),
                b"from input"
            );
            let tf = merged_config
                .view_transform("SDR Video")
                .unwrap()
                .transform(ViewTransformDirection::FromReference)
                .unwrap();

            // Validate the reference space conversion was added to the transform from the input config.
            check_group_types(
                tf,
                &[
                    TransformType::Matrix,
                    TransformType::Builtin,
                    TransformType::Matrix,
                ],
            );

            assert_eq!(merged_config.view_transform_name_by_index(1), b"vt2");
            let tf = merged_config
                .view_transform("vt2")
                .unwrap()
                .transform(ViewTransformDirection::ToReference)
                .unwrap();

            // Validate the reference space conversion was not added to the transform from the base config.
            assert_eq!(tf.transform_type(), TransformType::ExponentWithLinear);
        }
    }

    // PreferBase, Input first.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::PreferBase);
        merger.params(0).unwrap().set_input_first(true);

        let mut merged_config = (*base_config).clone();
        check_for_log_or_exception(
            LogType::Warning,
            || merge_roles_to_colorspaces(&base_config, &input_config, &params, &mut merged_config),
            &[
                "Equivalent base color space 'sRGB - Display' overrides 'sRGB - Display' in the input config, preserving aliases.",
                "Equivalent base color space 'CIE-XYZ-D65' overrides 'CIE-XYZ-D65' in the input config, preserving aliases.",
                "Equivalent base color space 'ap0' overrides 'ACES2065-1' in the input config, preserving aliases.",
                "Equivalent base color space 'sRGB - Texture' overrides 'sRGB' in the input config, preserving aliases.",
                concat!(
                    "Input color space 'ACES2065-1' is a duplicate of base color space 'ap0' but was ",
                    "unable to add alias 'aces' since it conflicts with base color space 'ACEScg'."
                ),
            ],
        );

        assert_eq!(merged_config.num_roles(), 1);
        assert_eq!(
            merged_config.role_color_space("cie_xyz_d65_interchange"),
            b"CIE-XYZ-D65"
        );

        assert_eq!(num_all_color_spaces(&merged_config), 7);

        // Display-referred spaces.
        {
            let cs = check_color_space(&merged_config, "sRGB - Display", 0, SEARCH_DISPLAY);
            assert_eq!(cs.num_aliases(), 1);
            assert_eq!(cs.alias(0), b"srgb_display");
            assert_eq!(cs.description(), b"from base");
            assert_eq!(cs.encoding(), b"");
            // Check categories were copied.
            assert_eq!(cs.num_categories(), 3);
            assert!(cs.has_category("file-io"));
            assert!(cs.has_category("texture"));
            assert!(cs.has_category("display"));
            {
                assert!(cs.transform(ColorSpaceDirection::ToReference).is_none());
                let t = cs.transform(ColorSpaceDirection::FromReference).unwrap();
                assert_eq!(t.transform_type(), TransformType::Builtin);
            }

            let cs1 = check_color_space(&merged_config, "CIE-XYZ-D65", 1, SEARCH_DISPLAY);
            assert_eq!(cs1.num_aliases(), 1);
            assert_eq!(cs1.alias(0), b"cie_xyz_d65");
            {
                assert!(cs1.transform(ColorSpaceDirection::ToReference).is_none());
                assert!(cs1.transform(ColorSpaceDirection::FromReference).is_none());
            }
        }

        // Scene-referred spaces.
        {
            let cs = check_color_space(&merged_config, "rec709", 0, SEARCH_SCENE);
            assert_eq!(cs.num_categories(), 1);
            assert!(cs.has_category("texture"));
            {
                assert!(cs.transform(ColorSpaceDirection::FromReference).is_none());
                let t = cs.transform(ColorSpaceDirection::ToReference).unwrap();
                check_group_types(t, &[TransformType::Matrix]);
            }

            let cs1 = check_color_space(&merged_config, "Raw", 1, SEARCH_SCENE);
            assert_eq!(cs1.num_aliases(), 1);
            assert_eq!(cs1.alias(0), b"Utility - Raw");
            assert!(cs1.is_data());
            {
                assert!(cs1.transform(ColorSpaceDirection::ToReference).is_none());
                assert!(cs1.transform(ColorSpaceDirection::FromReference).is_none());
            }

            let cs2 = check_color_space(&merged_config, "ACEScg", 2, SEARCH_SCENE);
            assert_eq!(cs2.num_aliases(), 1);
            assert_eq!(cs2.alias(0), b"aces");
            {
                assert!(cs2.transform(ColorSpaceDirection::FromReference).is_none());
                let t = cs2.transform(ColorSpaceDirection::ToReference).unwrap();
                assert_eq!(t.transform_type(), TransformType::Builtin);
            }

            let cs3 = check_color_space(&merged_config, "ap0", 3, SEARCH_SCENE);
            assert_eq!(cs3.num_aliases(), 1);
            assert_eq!(cs3.alias(0), b"ACES2065-1");
            assert!(!(cs3.is_data()));
            // Check categories were copied.
            assert_eq!(cs3.num_categories(), 2);
            assert!(cs3.has_category("file-io"));
            assert!(cs3.has_category("texture"));
            assert_eq!(cs3.encoding(), b"");
            {
                assert!(cs3.transform(ColorSpaceDirection::ToReference).is_none());
                assert!(cs3.transform(ColorSpaceDirection::FromReference).is_none());
            }

            let cs4 = check_color_space(&merged_config, "sRGB - Texture", 4, SEARCH_SCENE);
            assert_eq!(cs4.num_aliases(), 2);
            assert_eq!(cs4.alias(0), b"srgb");
            assert_eq!(cs4.alias(1), b"srgb_tx");
            assert_eq!(cs4.num_categories(), 2);
            {
                assert!(cs4.transform(ColorSpaceDirection::ToReference).is_none());
                let t = cs4.transform(ColorSpaceDirection::FromReference).unwrap();
                check_group_types(
                    t,
                    &[TransformType::Matrix, TransformType::ExponentWithLinear],
                );
            }
        }

        // View transforms.
        {
            assert_eq!(merged_config.num_view_transforms(), 2);
            assert_eq!(merged_config.view_transform_name_by_index(0), b"SDR Video");
            assert_eq!(
                merged_config
                    .view_transform("SDR Video")
                    .unwrap()
                    .description(),
                b"from base"
            );
            let tf = merged_config
                .view_transform("SDR Video")
                .unwrap()
                .transform(ViewTransformDirection::FromReference)
                .unwrap();

            // Validate that no reference space conversion was added, since the base transform was used.
            assert_eq!(tf.transform_type(), TransformType::Builtin);

            assert_eq!(merged_config.view_transform_name_by_index(1), b"vt2");
            let tf = merged_config
                .view_transform("vt2")
                .unwrap()
                .transform(ViewTransformDirection::ToReference)
                .unwrap();

            // Validate the reference space conversion was not added to the transform from the base config.
            assert_eq!(tf.transform_type(), TransformType::ExponentWithLinear);
        }
    }

    // Nothing special to test for Input only and Base only.
}

/// Upstream's `setupBasics` of the color spaces section errors' test: a merger with new
/// parameters, whose roles, color spaces, named transforms and default strategies are `strategy`,
/// with family prefixes, without the reference space adjustment and the duplicate search.
fn setup_colorspaces_errors_basics(
    merger: &mut ConfigMerger,
    strategy: MergeStrategies,
) -> ConfigMergingParameters {
    let params = ConfigMergingParameters::new().unwrap();
    merger.add_params(Some(params.clone()));
    let p = merger.params(0).unwrap();
    // Note that these tests run several of the mergers.
    p.set_roles(strategy);
    p.set_colorspaces(strategy);
    p.set_named_transforms(strategy);
    p.set_default_strategy(strategy);

    p.set_input_family_prefix("Input/");
    p.set_base_family_prefix("Base/");

    p.set_adjust_input_reference_space(false);
    p.set_avoid_duplicates(false);

    params
}

/// Port of `OCIO_ADD_TEST(MergeConfigs, colorspaces_section_errors)` @ v2.5.2.
#[test]
fn colorspaces_section_errors() {
    let setup_basics = setup_colorspaces_errors_basics;

    // Test ADD_CS_ERROR_NAME_IDENTICAL_TO_A_ROLE_NAME
    {
        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: A}

roles:
    b: colorspace_a

colorspaces:
- !<ColorSpace>
    name: colorspace_a
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: B}

colorspaces:
- !<ColorSpace>
    name: B
"#;
            let base_config = Config::create_from_stream(BASE.as_bytes()).unwrap();
            let input_config = Config::create_from_stream(INPUT.as_bytes()).unwrap();

            // The role takes priority over the inbound colorspace.
            // The conflicting color space should not be added to the merged config (skipped).
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
                merger.params(0).unwrap().set_input_first(false);

                let mut merged_config = (*base_config).clone();
                merge_section!(
                    RolesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            ColorspacesMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &["Color space 'B' was not merged as it's identical to a role name"],
                );

                assert_eq!(merged_config.num_roles(), 1);
                assert_eq!(merged_config.role_name(0), b"b");

                // Colorspace A should not be added to the merged config (skipped)
                assert_eq!(num_all_color_spaces(&merged_config), 1);
                assert_eq!(merged_config.color_space_name_by_index(0), b"colorspace_a");
            }
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferBase);
                merger.params(0).unwrap().set_input_first(true);

                let mut merged_config = (*base_config).clone();
                merge_section!(
                    RolesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            ColorspacesMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &["Color space 'B' was not merged as it's identical to a role name"],
                );

                assert_eq!(merged_config.num_roles(), 1);
                assert_eq!(merged_config.role_name(0), b"b");

                // Colorspace A should not be added to the merged config (skipped)
                assert_eq!(num_all_color_spaces(&merged_config), 1);
                assert_eq!(merged_config.color_space_name_by_index(0), b"colorspace_a");
            }
            // Testing the error message when Error on conflict is enabled.
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
                merger.params(0).unwrap().set_input_first(false);
                merger.params(0).unwrap().set_error_on_conflict(true);

                let mut merged_config = (*base_config).clone();
                merge_section!(
                    RolesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_throw_what(
                    merge_section!(
                        ColorspacesMerger,
                        &base_config,
                        &input_config,
                        &params,
                        &mut merged_config
                    ),
                    "Color space 'B' was not merged as it's identical to a role name",
                );
            }
        }

        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: A}

colorspaces:
- !<ColorSpace>
    name: A
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: B}

roles:
    A: colorspace_b

colorspaces:
- !<ColorSpace>
    name: colorspace_b
"#;
            let base_config = Config::create_from_stream(BASE.as_bytes()).unwrap();
            let input_config = Config::create_from_stream(INPUT.as_bytes()).unwrap();

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
                        "The Input config contains a role 'a' that would override Base config color space 'A'",
                    ],
                );
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();

                assert_eq!(merged_config.num_roles(), 0);

                assert_eq!(num_all_color_spaces(&merged_config), 2);
                assert_eq!(merged_config.color_space_name_by_index(0), b"colorspace_b");
                assert_eq!(merged_config.color_space_name_by_index(1), b"A");
            }
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
                        "The Input config contains a role 'a' that would override Base config color space 'A'",
                    ],
                );
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();

                assert_eq!(merged_config.num_roles(), 0);

                assert_eq!(num_all_color_spaces(&merged_config), 2);
                assert_eq!(merged_config.color_space_name_by_index(0), b"colorspace_b");
                assert_eq!(merged_config.color_space_name_by_index(1), b"A");
            }
        }
    }

    {
        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
- !<Rule> {name: Default, colorspace: cs_base}

named_transforms:
- !<NamedTransform>
    name: nt_base
    encoding: log
    inverse_transform: !<MatrixTransform> {name: inverse, offset: [-0.2, -0.1, -0.1, 0]}
- !<NamedTransform>
    name: nt_base_extra
    aliases: [nt_base2]
    encoding: log
    inverse_transform: !<MatrixTransform> {name: inverse, offset: [-0.2, -0.1, -0.1, 0]}

colorspaces:
- !<ColorSpace>
    name: cs_base
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
- !<Rule> {name: Default, colorspace: nt_base}

colorspaces:
- !<ColorSpace>
    name: nt_base
- !<ColorSpace>
    name: nt_base2
"#;
            let base_config = Config::create_from_stream(BASE.as_bytes()).unwrap();
            let input_config = Config::create_from_stream(INPUT.as_bytes()).unwrap();
            {
                let mut merger = ConfigMerger::new();
                let mut merged_config = (*base_config).clone();
                let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
                merger.params(0).unwrap().set_input_first(false);

                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            NamedTransformsMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &[
                        "Named transform 'nt_base' was not merged as there's a color space with that name",
                        "Merged Base named transform 'nt_base_extra' has an alias 'nt_base2' that conflicts with color space 'nt_base2'",
                    ],
                );

                assert_eq!(
                    merged_config.num_named_transforms_with(NamedTransformVisibility::All),
                    1
                );
                let nt = check_named_transform(&merged_config, "nt_base_extra", 0);
                assert_eq!(nt.num_aliases(), 0);

                assert_eq!(num_all_color_spaces(&merged_config), 3);
                assert_eq!(merged_config.color_space_name_by_index(0), b"cs_base");
                assert_eq!(merged_config.color_space_name_by_index(1), b"nt_base");
                assert_eq!(merged_config.color_space_name_by_index(2), b"nt_base2");
            }
            {
                let mut merger = ConfigMerger::new();
                let mut merged_config = (*base_config).clone();
                let params = setup_basics(&mut merger, MergeStrategies::PreferBase);
                merger.params(0).unwrap().set_input_first(false);

                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            NamedTransformsMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &[
                        "Named transform 'nt_base' was not merged as there's a color space with that name",
                        "Merged Base named transform 'nt_base_extra' has an alias 'nt_base2' that conflicts with color space 'nt_base2'",
                    ],
                );

                assert_eq!(
                    merged_config.num_named_transforms_with(NamedTransformVisibility::All),
                    1
                );
                let nt = check_named_transform(&merged_config, "nt_base_extra", 0);
                assert_eq!(nt.num_aliases(), 0);

                assert_eq!(num_all_color_spaces(&merged_config), 3);
                assert_eq!(merged_config.color_space_name_by_index(0), b"cs_base");
                assert_eq!(merged_config.color_space_name_by_index(1), b"nt_base");
                assert_eq!(merged_config.color_space_name_by_index(2), b"nt_base2");
            }
            // Testing the error message when Error on conflict is enabled.
            {
                let mut merger = ConfigMerger::new();
                let mut merged_config = (*base_config).clone();

                let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
                merger.params(0).unwrap().set_input_first(false);
                merger.params(0).unwrap().set_error_on_conflict(true);

                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_throw_what(
                    merge_section!(
                        NamedTransformsMerger,
                        &base_config,
                        &input_config,
                        &params,
                        &mut merged_config
                    ),
                    "Named transform 'nt_base' was not merged as there's a color space with that name",
                );
            }
        }

        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: nt_input}

colorspaces:
- !<ColorSpace>
    name: nt_input
- !<ColorSpace>
    name: nt_input2
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: cs_input}

named_transforms:
  - !<NamedTransform>
    name: nt_input
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}

  - !<NamedTransform>
    name: nt_input_extra
    aliases: [nt_input2]
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}

colorspaces:
- !<ColorSpace>
    name: cs_input
"#;
            let base_config = Config::create_from_stream(BASE.as_bytes()).unwrap();
            let input_config = Config::create_from_stream(INPUT.as_bytes()).unwrap();
            {
                let mut merger = ConfigMerger::new();
                let mut merged_config = (*base_config).clone();
                let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
                merger.params(0).unwrap().set_input_first(true);

                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            NamedTransformsMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &[
                        "Named transform 'nt_input' was not merged as there's a color space with that name",
                        "Merged Input named transform 'nt_input_extra' has an alias 'nt_input2' that conflicts with color space 'nt_input2'",
                    ],
                );

                assert_eq!(
                    merged_config.num_named_transforms_with(NamedTransformVisibility::All),
                    1
                );
                let nt = check_named_transform(&merged_config, "nt_input_extra", 0);
                assert_eq!(nt.num_aliases(), 0);

                assert_eq!(num_all_color_spaces(&merged_config), 3);
                assert_eq!(merged_config.color_space_name_by_index(0), b"cs_input");
                assert_eq!(merged_config.color_space_name_by_index(1), b"nt_input");
                assert_eq!(merged_config.color_space_name_by_index(2), b"nt_input2");
            }

            {
                let mut merger = ConfigMerger::new();
                let mut merged_config = (*base_config).clone();
                let params = setup_basics(&mut merger, MergeStrategies::PreferBase);
                merger.params(0).unwrap().set_input_first(false);

                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            NamedTransformsMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &[
                        "Named transform 'nt_input' was not merged as there's a color space with that name",
                        "Merged Input named transform 'nt_input_extra' has an alias 'nt_input2' that conflicts with color space 'nt_input2'",
                    ],
                );

                assert_eq!(
                    merged_config.num_named_transforms_with(NamedTransformVisibility::All),
                    1
                );
                let nt = check_named_transform(&merged_config, "nt_input_extra", 0);
                assert_eq!(nt.num_aliases(), 0);

                assert_eq!(num_all_color_spaces(&merged_config), 3);
                assert_eq!(merged_config.color_space_name_by_index(0), b"nt_input");
                assert_eq!(merged_config.color_space_name_by_index(1), b"nt_input2");
                assert_eq!(merged_config.color_space_name_by_index(2), b"cs_input");
            }
        }
    }

    {
        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: csBase}

roles:
    role_base: csBase

colorspaces:
- !<ColorSpace>
    name: csBase

"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: csInput}

colorspaces:
- !<ColorSpace>
    name: csInput
    aliases: [role_base]
"#;
            let base_config = Config::create_from_stream(BASE.as_bytes()).unwrap();
            let input_config = Config::create_from_stream(INPUT.as_bytes()).unwrap();
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
                merger.params(0).unwrap().set_input_first(false);

                let mut merged_config = (*base_config).clone();
                merge_section!(
                    RolesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            ColorspacesMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &[
                        "Merged color space 'csInput' has an alias 'role_base' that conflicts with a role",
                    ],
                );

                assert_eq!(merged_config.num_roles(), 1);
                assert_eq!(merged_config.role_name(0), b"role_base");

                assert_eq!(num_all_color_spaces(&merged_config), 2);
                assert_eq!(merged_config.color_space_name_by_index(0), b"csBase");
                let name = merged_config.color_space_name_by_index(1);
                assert_eq!(name, b"csInput");
                assert_eq!(merged_config.color_space(name).unwrap().num_aliases(), 0);
            }
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferBase);
                merger.params(0).unwrap().set_input_first(false);

                let mut merged_config = (*base_config).clone();
                merge_section!(
                    RolesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            ColorspacesMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &[
                        "Merged color space 'csInput' has an alias 'role_base' that conflicts with a role",
                    ],
                );

                assert_eq!(merged_config.num_roles(), 1);
                assert_eq!(merged_config.role_name(0), b"role_base");

                assert_eq!(num_all_color_spaces(&merged_config), 2);
                assert_eq!(merged_config.color_space_name_by_index(0), b"csBase");
                let name = merged_config.color_space_name_by_index(1);
                assert_eq!(name, b"csInput");
                assert_eq!(merged_config.color_space(name).unwrap().num_aliases(), 0);
            }
            // Testing the error message when Error on conflict is enabled.
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
                merger.params(0).unwrap().set_error_on_conflict(true);

                let mut merged_config = (*base_config).clone();
                merge_section!(
                    RolesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_throw_what(
                    merge_section!(
                        ColorspacesMerger,
                        &base_config,
                        &input_config,
                        &params,
                        &mut merged_config
                    ),
                    "Merged color space 'csInput' has an alias 'role_base' that conflicts with a role",
                );
            }
        }

        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: csBase}

colorspaces:
- !<ColorSpace>
    name: csBase
    aliases: [role_input]
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: csInput}

roles:
    role_input: csInput

colorspaces:
- !<ColorSpace>
    name: csInput
"#;
            let base_config = Config::create_from_stream(BASE.as_bytes()).unwrap();
            let input_config = Config::create_from_stream(INPUT.as_bytes()).unwrap();
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
                        "The Input config contains a role 'role_input' that would override an alias of Base config color space 'csBase'",
                    ],
                );
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();

                assert_eq!(merged_config.num_roles(), 0);

                assert_eq!(num_all_color_spaces(&merged_config), 2);
                assert_eq!(merged_config.color_space_name_by_index(0), b"csInput");
                let name = merged_config.color_space_name_by_index(1);
                assert_eq!(name, b"csBase");
                assert_eq!(merged_config.color_space(name).unwrap().num_aliases(), 1);
                assert_eq!(
                    merged_config.color_space(name).unwrap().alias(0),
                    b"role_input"
                );
            }
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
                        "The Input config contains a role 'role_input' that would override an alias of Base config color space 'csBase'",
                    ],
                );
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();

                assert_eq!(merged_config.num_roles(), 0);

                assert_eq!(num_all_color_spaces(&merged_config), 2);
                assert_eq!(merged_config.color_space_name_by_index(0), b"csInput");
                let name = merged_config.color_space_name_by_index(1);
                assert_eq!(name, b"csBase");
                assert_eq!(merged_config.color_space(name).unwrap().num_aliases(), 1);
                assert_eq!(
                    merged_config.color_space(name).unwrap().alias(0),
                    b"role_input"
                );
            }
        }
    }

    {
        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: A}

named_transforms:
  - !<NamedTransform>
    name: nt_base
    encoding: log
    inverse_transform: !<MatrixTransform> {name: inverse, offset: [-0.2, -0.1, -0.1, 0]}

  - !<NamedTransform>
    name: nt_base_extra
    aliases: [nt_base2]
    encoding: log
    inverse_transform: !<MatrixTransform> {name: inverse, offset: [-0.2, -0.1, -0.1, 0]}
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: cs_input}

colorspaces:
- !<ColorSpace>
    name: cs_input
    aliases: [nt_base]
- !<ColorSpace>
    name: cs_input2
    aliases: [nt_base2]
"#;
            let base_config = Config::create_from_stream(BASE.as_bytes()).unwrap();
            let input_config = Config::create_from_stream(INPUT.as_bytes()).unwrap();
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
                merger.params(0).unwrap().set_input_first(false);

                let mut merged_config = (*base_config).clone();
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            NamedTransformsMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &[
                        "Named transform 'nt_base' was not merged as there's a color space alias with that name",
                        "Merged Base named transform 'nt_base_extra' has a conflict with alias 'nt_base2' in color space 'cs_input2'",
                    ],
                );

                assert_eq!(
                    merged_config.num_named_transforms_with(NamedTransformVisibility::All),
                    1
                );
                let nt = check_named_transform(&merged_config, "nt_base_extra", 0);
                assert_eq!(nt.num_aliases(), 0);

                assert_eq!(num_all_color_spaces(&merged_config), 2);

                let cs = check_color_space(&merged_config, "cs_input", 0, SEARCH_SCENE);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), b"nt_base");

                let cs = check_color_space(&merged_config, "cs_input2", 1, SEARCH_SCENE);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), b"nt_base2");
            }
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferBase);
                merger.params(0).unwrap().set_input_first(false);

                let mut merged_config = (*base_config).clone();
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            NamedTransformsMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &[
                        "Named transform 'nt_base' was not merged as there's a color space alias with that name",
                        "Merged Base named transform 'nt_base_extra' has a conflict with alias 'nt_base2' in color space 'cs_input2'",
                    ],
                );

                assert_eq!(
                    merged_config.num_named_transforms_with(NamedTransformVisibility::All),
                    1
                );
                let nt = check_named_transform(&merged_config, "nt_base_extra", 0);
                assert_eq!(nt.num_aliases(), 0);

                assert_eq!(num_all_color_spaces(&merged_config), 2);
                let cs = check_color_space(&merged_config, "cs_input", 0, SEARCH_SCENE);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), b"nt_base");

                let cs = check_color_space(&merged_config, "cs_input2", 1, SEARCH_SCENE);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), b"nt_base2");
            }
            // Testing the error message when Error on conflict is enabled.
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
                merger.params(0).unwrap().set_error_on_conflict(true);

                let mut merged_config = (*base_config).clone();
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_throw_what(
                    merge_section!(
                        NamedTransformsMerger,
                        &base_config,
                        &input_config,
                        &params,
                        &mut merged_config
                    ),
                    "Named transform 'nt_base' was not merged as there's a color space alias with that name",
                );
            }
        }

        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: cs_base}

colorspaces:
- !<ColorSpace>
    name: cs_base
    aliases: [nt_input]
- !<ColorSpace>
    name: cs_base2
    aliases: [nt_input2]
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: B}

named_transforms:
  - !<NamedTransform>
    name: nt_input
    encoding: log
    inverse_transform: !<MatrixTransform> {name: inverse, offset: [-0.2, -0.1, -0.1, 0]}
  - !<NamedTransform>
    name: nt_input_extra
    aliases: [nt_input2]
    encoding: log
    inverse_transform: !<MatrixTransform> {name: inverse, offset: [-0.2, -0.1, -0.1, 0]}
"#;
            let base_config = Config::create_from_stream(BASE.as_bytes()).unwrap();
            let input_config = Config::create_from_stream(INPUT.as_bytes()).unwrap();
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
                merger.params(0).unwrap().set_input_first(false);

                let mut merged_config = (*base_config).clone();
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            NamedTransformsMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &[
                        "Named transform 'nt_input' was not merged as there's a color space alias with that name",
                        "Merged Input named transform 'nt_input_extra' has a conflict with alias 'nt_input2' in color space 'cs_base2'",
                    ],
                );

                assert_eq!(
                    merged_config.num_named_transforms_with(NamedTransformVisibility::All),
                    1
                );
                let nt = check_named_transform(&merged_config, "nt_input_extra", 0);
                assert_eq!(nt.num_aliases(), 0);

                assert_eq!(num_all_color_spaces(&merged_config), 2);

                let cs = check_color_space(&merged_config, "cs_base", 0, SEARCH_SCENE);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), b"nt_input");

                let cs = check_color_space(&merged_config, "cs_base2", 1, SEARCH_SCENE);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), b"nt_input2");
            }
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferBase);
                merger.params(0).unwrap().set_input_first(false);

                let mut merged_config = (*base_config).clone();
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            NamedTransformsMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &[
                        "Named transform 'nt_input' was not merged as there's a color space alias with that name",
                        "Merged Input named transform 'nt_input_extra' has a conflict with alias 'nt_input2' in color space 'cs_base2'",
                    ],
                );

                assert_eq!(
                    merged_config.num_named_transforms_with(NamedTransformVisibility::All),
                    1
                );
                let nt = check_named_transform(&merged_config, "nt_input_extra", 0);
                assert_eq!(nt.num_aliases(), 0);

                assert_eq!(num_all_color_spaces(&merged_config), 2);

                let cs = check_color_space(&merged_config, "cs_base", 0, SEARCH_SCENE);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), b"nt_input");

                let cs = check_color_space(&merged_config, "cs_base2", 1, SEARCH_SCENE);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), b"nt_input2");
            }
        }
    }

    {
        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: cs_base}

colorspaces:
- !<ColorSpace>
    name: cs_base
    aliases: [my_colorspace]
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: cs_input}

colorspaces:
- !<ColorSpace>
    name: cs_input
    aliases: [my_colorspace]
"#;
            let base_config = Config::create_from_stream(BASE.as_bytes()).unwrap();
            let input_config = Config::create_from_stream(INPUT.as_bytes()).unwrap();

            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
                merger.params(0).unwrap().set_input_first(false);

                let mut merged_config = (*base_config).clone();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            ColorspacesMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &[
                        "Merged color space 'cs_input' has a conflict with alias 'my_colorspace' in color space 'cs_base'",
                    ],
                );

                assert_eq!(num_all_color_spaces(&merged_config), 2);

                let cs = check_color_space(&merged_config, "cs_base", 0, SEARCH_SCENE);
                assert_eq!(cs.num_aliases(), 0);

                let cs = check_color_space(&merged_config, "cs_input", 1, SEARCH_SCENE);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), b"my_colorspace");
            }
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferBase);
                merger.params(0).unwrap().set_input_first(false);

                let mut merged_config = (*base_config).clone();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            ColorspacesMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &[
                        "Merged color space 'cs_input' has a conflict with alias 'my_colorspace' in color space 'cs_base'",
                    ],
                );

                assert_eq!(num_all_color_spaces(&merged_config), 2);

                let cs = check_color_space(&merged_config, "cs_base", 0, SEARCH_SCENE);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), b"my_colorspace");

                let cs = check_color_space(&merged_config, "cs_input", 1, SEARCH_SCENE);
                assert_eq!(cs.num_aliases(), 0);
            }
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferInput);

                let mut merged_config = (*base_config).clone();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            ColorspacesMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &[
                        "Merged color space 'cs_input' has a conflict with alias 'my_colorspace' in color space 'cs_base'",
                    ],
                );

                assert_eq!(num_all_color_spaces(&merged_config), 2);

                let cs = check_color_space(&merged_config, "cs_input", 0, SEARCH_SCENE);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), b"my_colorspace");

                let cs = check_color_space(&merged_config, "cs_base", 1, SEARCH_SCENE);
                assert_eq!(cs.num_aliases(), 0);
            }
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferBase);

                let mut merged_config = (*base_config).clone();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            ColorspacesMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &[
                        "Merged color space 'cs_input' has a conflict with alias 'my_colorspace' in color space 'cs_base'",
                    ],
                );

                assert_eq!(num_all_color_spaces(&merged_config), 2);

                let cs = check_color_space(&merged_config, "cs_input", 0, SEARCH_SCENE);
                assert_eq!(cs.num_aliases(), 0);

                let cs = check_color_space(&merged_config, "cs_base", 1, SEARCH_SCENE);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), b"my_colorspace");
            }
            // Testing the error message when Error on conflict is enabled.
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
                merger.params(0).unwrap().set_error_on_conflict(true);

                let mut merged_config = (*base_config).clone();
                check_throw_what(
                    merge_section!(
                        ColorspacesMerger,
                        &base_config,
                        &input_config,
                        &params,
                        &mut merged_config
                    ),
                    "Merged color space 'cs_input' has a conflict with alias 'my_colorspace' in color space 'cs_base'",
                );
            }
        }
    }

    {
        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: A}

colorspaces:
- !<ColorSpace>
    name: A
    aliases: [B]
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: B}

colorspaces:
- !<ColorSpace>
    name: B
"#;
            let base_config = Config::create_from_stream(BASE.as_bytes()).unwrap();
            let input_config = Config::create_from_stream(INPUT.as_bytes()).unwrap();
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
                merger.params(0).unwrap().set_input_first(false);

                let mut merged_config = (*base_config).clone();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            ColorspacesMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &[
                        "The name of merged color space 'B' has a conflict with an alias in color space 'A'",
                    ],
                );

                assert_eq!(num_all_color_spaces(&merged_config), 2);

                let cs = check_color_space(&merged_config, "A", 0, SEARCH_SCENE);
                assert_eq!(cs.num_aliases(), 0);

                let cs = check_color_space(&merged_config, "B", 1, SEARCH_SCENE);
                assert_eq!(cs.num_aliases(), 0);
            }
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferBase);
                merger.params(0).unwrap().set_input_first(false);

                let mut merged_config = (*base_config).clone();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            ColorspacesMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &[
                        "Color space 'B' was not merged as it conflicts with an alias in color space 'A'",
                    ],
                );

                assert_eq!(num_all_color_spaces(&merged_config), 1);

                let cs = check_color_space(&merged_config, "A", 0, SEARCH_SCENE);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), b"B");
            }
            // Testing the error message when Error on conflict is enabled.
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
                merger.params(0).unwrap().set_error_on_conflict(true);

                let mut merged_config = (*base_config).clone();
                check_throw_what(
                    merge_section!(
                        ColorspacesMerger,
                        &base_config,
                        &input_config,
                        &params,
                        &mut merged_config
                    ),
                    "The name of merged color space 'B' has a conflict with an alias in color space 'A'",
                );
            }
        }

        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: A}

colorspaces:
- !<ColorSpace>
    name: A
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: B}

colorspaces:
- !<ColorSpace>
    name: B
    aliases: [A]
"#;
            let base_config = Config::create_from_stream(BASE.as_bytes()).unwrap();
            let input_config = Config::create_from_stream(INPUT.as_bytes()).unwrap();
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferInput);

                let mut merged_config = (*base_config).clone();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            ColorspacesMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &[
                        "Merged color space 'B' has an alias 'A' that conflicts with color space 'A'",
                    ],
                );

                assert_eq!(num_all_color_spaces(&merged_config), 1);

                let cs = check_color_space(&merged_config, "B", 0, SEARCH_SCENE);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), b"A");
            }
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
                merger.params(0).unwrap().set_input_first(false);

                let mut merged_config = (*base_config).clone();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            ColorspacesMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &[
                        "Merged color space 'B' has an alias 'A' that conflicts with color space 'A'",
                    ],
                );

                assert_eq!(num_all_color_spaces(&merged_config), 1);

                let cs = check_color_space(&merged_config, "B", 0, SEARCH_SCENE);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), b"A");
            }
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferBase);

                let mut merged_config = (*base_config).clone();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            ColorspacesMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &[
                        "Merged color space 'B' has an alias 'A' that conflicts with color space 'A'",
                    ],
                );

                assert_eq!(num_all_color_spaces(&merged_config), 2);

                let cs = check_color_space(&merged_config, "B", 0, SEARCH_SCENE);
                assert_eq!(cs.num_aliases(), 0);

                let cs = check_color_space(&merged_config, "A", 1, SEARCH_SCENE);
                assert_eq!(cs.num_aliases(), 0);
            }
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferBase);
                merger.params(0).unwrap().set_input_first(false);

                let mut merged_config = (*base_config).clone();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            ColorspacesMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &[
                        "Merged color space 'B' has an alias 'A' that conflicts with color space 'A'",
                    ],
                );

                assert_eq!(num_all_color_spaces(&merged_config), 2);

                let cs = check_color_space(&merged_config, "A", 0, SEARCH_SCENE);
                assert_eq!(cs.num_aliases(), 0);

                let cs = check_color_space(&merged_config, "B", 1, SEARCH_SCENE);
                assert_eq!(cs.num_aliases(), 0);
            }
            // Testing the error message when Error on conflict is enabled.
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
                merger.params(0).unwrap().set_error_on_conflict(true);

                let mut merged_config = (*base_config).clone();
                check_throw_what(
                    merge_section!(
                        ColorspacesMerger,
                        &base_config,
                        &input_config,
                        &params,
                        &mut merged_config
                    ),
                    "Merged color space 'B' has an alias 'A' that conflicts with color space 'A'",
                );
            }
        }
    }
}

/// Port of `checkNamedTransform` (MergeConfigsHelpers_tests.cpp:83-93 @ v2.5.2).
#[track_caller]
fn check_named_transform<'c>(
    merged_config: &'c Config,
    ref_name: &str,
    index: i32,
) -> &'c NamedTransform {
    let name =
        merged_config.named_transform_name_by_index_with(NamedTransformVisibility::All, index);
    assert_eq!(name, ref_name.as_bytes());
    merged_config
        .named_transform(ref_name)
        .unwrap_or_else(|| panic!("no named transform {ref_name}"))
}

/// Upstream's `setupBasics` of the named transforms section's test: a merger with new
/// parameters, whose roles, color spaces, named transforms and default strategies are
/// `strategy`, with family prefixes, without the reference space adjustment, with the duplicate
/// search, input first.
fn setup_named_transforms_basics(
    merger: &mut ConfigMerger,
    strategy: MergeStrategies,
) -> ConfigMergingParameters {
    let params = ConfigMergingParameters::new().unwrap();
    merger.add_params(Some(params.clone()));
    let p = merger.params(0).unwrap();
    // Note that these tests run several of the mergers.
    // Need to run the color space merger too, since that affects how the named transform
    // merger will work (in terms of avoiding conflicts with color space names).
    p.set_roles(strategy);
    p.set_colorspaces(strategy);
    p.set_named_transforms(strategy);
    p.set_default_strategy(strategy);

    p.set_input_family_prefix("Input/");
    p.set_base_family_prefix("Base/");

    p.set_adjust_input_reference_space(false);
    p.set_avoid_duplicates(true);
    p.set_input_first(true);

    params
}

/// Port of `OCIO_ADD_TEST(MergeConfigs, named_transform_section)` @ v2.5.2.
#[test]
fn named_transform_section() {
    let base_config = get_base_config().unwrap();

    let input_config = get_input_config().unwrap();

    let setup_basics = setup_named_transforms_basics;

    // Test that the default strategy is used as a fallback if the section strategy was not defined.
    {
        let mut merger = ConfigMerger::new();
        // Using STRATEGY_UNSPECIFIED as this simulate that the section
        // is missing from the OCIOM file.
        let params = setup_basics(&mut merger, MergeStrategies::Unspecified);
        // Simulate settings from OCIOM file.
        merger
            .params(0)
            .unwrap()
            .set_default_strategy(MergeStrategies::InputOnly);

        let mut merged_config = (*base_config).clone();
        merge_section!(
            ColorspacesMerger,
            &base_config,
            &input_config,
            &params,
            &mut merged_config
        )
        .unwrap();
        merge_section!(
            NamedTransformsMerger,
            &base_config,
            &input_config,
            &params,
            &mut merged_config
        )
        .unwrap();

        assert_eq!(
            merged_config.num_named_transforms_with(NamedTransformVisibility::All),
            3
        );

        let nt = check_named_transform(&merged_config, "nt_both", 0);
        assert_eq!(nt.num_aliases(), 2);
        assert_eq!(nt.alias(0), b"Utility - Raw");
        assert_eq!(nt.alias(1), b"nametr");
        assert_eq!(nt.family(), b"");
        assert_eq!(nt.description(), b"from input");

        let nt = check_named_transform(&merged_config, "nt_input", 1);
        assert_eq!(nt.num_aliases(), 2);
        assert_eq!(nt.alias(0), b"Raw");
        assert_eq!(nt.alias(1), b"in nt");
        assert_eq!(nt.family(), b"Raw");
        assert_eq!(nt.description(), b"from input");

        let nt = check_named_transform(&merged_config, "view_2", 2);
        assert_eq!(nt.num_aliases(), 1);
        assert_eq!(nt.alias(0), b"g22_ap1");
        assert_eq!(nt.family(), b"Raw");
        assert_eq!(nt.description(), b"from input");
    }

    // Test NamedTransform with strategy = PreferInput, options InputFirst = true.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::PreferInput);

        let mut merged_config = (*base_config).clone();
        check_for_log_or_exception(
            LogType::Warning,
            || {
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
            },
            &[
                "Equivalent input color space 'ACES2065-1' replaces 'ACES2065-1' in the base config, preserving aliases.",
                "Equivalent input color space 'ACEScct - SomeOtherName' replaces 'ACEScct' in the base config, preserving aliases.",
                "Equivalent input color space 'view_1' replaces 'view_1' in the base config, preserving aliases.",
                "Equivalent input color space 'view_1B' replaces 'view_1' in the base config, preserving aliases.",
                "Equivalent input color space 'view_3' replaces 'view_2' in the base config, preserving aliases.",
                "Equivalent input color space 'log_3' replaces 'log_1' in the base config, preserving aliases.",
                "Equivalent input color space 'lin_3' replaces 'ACES2065-1' in the base config, preserving aliases.",
            ],
        );
        check_for_log_or_exception(
            LogType::Warning,
            || {
                merge_section!(
                    NamedTransformsMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
            },
            &[
                "Named transform 'nt_both' will replace a named transform in the base config",
                "Merged Base named transform 'nt_both' has a conflict with alias 'srgb_tx' in color space 'sRGB - Texture'",
                "Merged Base named transform 'nt_base' has an alias 'view_3' that conflicts with color space 'view_3'",
                "Merged Input named transform 'nt_both' has a conflict with alias 'Utility - Raw' in color space 'Raw'",
                "The name of merged named transform 'nt_input' has a conflict with an alias in named transform 'nt_base'",
                "Merged Input named transform 'nt_input' has an alias 'Raw' that conflicts with color space 'Raw'",
                "Named transform 'view_2' was not merged as there's a color space alias with that name.",
            ],
        );

        assert_eq!(
            merged_config.num_named_transforms_with(NamedTransformVisibility::All),
            3
        );

        let nt = check_named_transform(&merged_config, "nt_both", 0);
        assert_eq!(nt.num_aliases(), 1);
        assert_eq!(nt.alias(0), b"nametr");
        assert_eq!(nt.family(), b"Input@");
        assert_eq!(nt.description(), b"from input");

        let nt = check_named_transform(&merged_config, "nt_input", 1);
        assert_eq!(nt.num_aliases(), 1);
        assert_eq!(nt.alias(0), b"in nt");
        assert_eq!(nt.family(), b"Input@Raw");
        assert_eq!(nt.description(), b"from input");

        let nt = check_named_transform(&merged_config, "nt_base", 2);
        assert_eq!(nt.num_aliases(), 0);
        assert_eq!(nt.family(), b"Base@nt");
        assert_eq!(nt.description(), b"from base");
    }

    // Test NamedTransform with strategy=PreferInput, options InputFirst = false.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
        merger.params(0).unwrap().set_input_first(false);

        let mut merged_config = (*base_config).clone();
        check_for_log_or_exception(
            LogType::Warning,
            || {
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
            },
            &[
                "Equivalent input color space 'ACES2065-1' replaces 'ACES2065-1' in the base config, preserving aliases.",
                "Equivalent input color space 'ACEScct - SomeOtherName' replaces 'ACEScct' in the base config, preserving aliases.",
                "Equivalent input color space 'view_1' replaces 'view_1' in the base config, preserving aliases.",
                "Equivalent input color space 'view_1B' replaces 'view_1' in the base config, preserving aliases.",
                "Equivalent input color space 'view_3' replaces 'view_2' in the base config, preserving aliases.",
                "Equivalent input color space 'log_3' replaces 'log_1' in the base config, preserving aliases.",
                "Equivalent input color space 'lin_3' replaces 'ACES2065-1' in the base config, preserving aliases.",
            ],
        );
        check_for_log_or_exception(
            LogType::Warning,
            || {
                merge_section!(
                    NamedTransformsMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
            },
            &[
                "Named transform 'nt_both' will replace a named transform in the base config",
                "Merged Base named transform 'nt_both' has a conflict with alias 'srgb_tx' in color space 'sRGB - Texture'",
                "Merged Base named transform 'nt_base' has an alias 'view_3' that conflicts with color space 'view_3'",
                "Merged Input named transform 'nt_both' has a conflict with alias 'Utility - Raw' in color space 'Raw'",
                "The name of merged named transform 'nt_input' has a conflict with an alias in named transform 'nt_base'",
                "Merged Input named transform 'nt_input' has an alias 'Raw' that conflicts with color space 'Raw'",
                "Named transform 'view_2' was not merged as there's a color space alias with that name.",
            ],
        );

        assert_eq!(
            merged_config.num_named_transforms_with(NamedTransformVisibility::All),
            3
        );

        let nt = check_named_transform(&merged_config, "nt_base", 0);
        assert_eq!(nt.num_aliases(), 0);
        assert_eq!(nt.family(), b"Base@nt");
        assert_eq!(nt.description(), b"from base");

        let nt = check_named_transform(&merged_config, "nt_both", 1);
        assert_eq!(nt.num_aliases(), 1);
        assert_eq!(nt.alias(0), b"nametr");
        assert_eq!(nt.family(), b"Input@");
        assert_eq!(nt.description(), b"from input");

        let nt = check_named_transform(&merged_config, "nt_input", 2);
        assert_eq!(nt.num_aliases(), 1);
        assert_eq!(nt.alias(0), b"in nt");
        assert_eq!(nt.family(), b"Input@Raw");
        assert_eq!(nt.description(), b"from input");

        assert_eq!(
            merged_config.inactive_color_spaces(),
            b"Gamma 2.2 AP1 - Texture, Linear Rec.2020, nt_both, nt_input"
        );
    }

    // Test NamedTransform with strategy = PreferBase, options InputFirst = true.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::PreferBase);

        let mut merged_config = (*base_config).clone();
        check_for_log_or_exception(
            LogType::Warning,
            || {
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
            },
            &[
                "Equivalent base color space 'ACES2065-1' overrides 'ACES2065-1' in the input config, preserving aliases.",
                "Equivalent base color space 'ACEScct' overrides 'ACEScct - SomeOtherName' in the input config, preserving aliases.",
                "Equivalent base color space 'view_1' overrides 'view_1' in the input config, preserving aliases.",
                "Equivalent base color space 'view_1' overrides 'view_1B' in the input config, preserving aliases.",
                "Equivalent base color space 'view_2' overrides 'view_3' in the input config, preserving aliases.",
                "Equivalent base color space 'log_1' overrides 'log_3' in the input config, preserving aliases.",
                "Equivalent base color space 'ACES2065-1' overrides 'lin_3' in the input config, preserving aliases.",
            ],
        );
        check_for_log_or_exception(
            LogType::Warning,
            || {
                merge_section!(
                    NamedTransformsMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
            },
            &[
                "Merged Base named transform 'nt_both' has a conflict with alias 'srgb_tx' in color space 'sRGB - Texture'",
                "Merged Base named transform 'nt_base' has a conflict with alias 'view_3' in color space 'view_2'.",
                "Named transform 'nt_both' was not merged as it's already present in the base config",
                "Named transform 'nt_input' was not merged as it conflicts with an alias in named transform 'nt_base'",
                "Named transform 'view_2' was not merged as there's a color space with that name",
            ],
        );

        assert_eq!(
            merged_config.num_named_transforms_with(NamedTransformVisibility::All),
            2
        );

        let nt = check_named_transform(&merged_config, "nt_both", 0);
        assert_eq!(nt.num_aliases(), 1);
        assert_eq!(nt.alias(0), b"namet2");
        assert_eq!(nt.family(), b"Base#");
        assert_eq!(nt.description(), b"from base");

        let nt = check_named_transform(&merged_config, "nt_base", 1);
        assert_eq!(nt.num_aliases(), 1);
        assert_eq!(nt.alias(0), b"nt_input");
        assert_eq!(nt.family(), b"Base#nt");
        assert_eq!(nt.description(), b"from base");

        // NB: The nt_input is included referring to the alias in the base config, not the input config.
        assert_eq!(
            merged_config.inactive_color_spaces(),
            b"Linear Rec.2020, nt_both, view_2, Gamma 2.2 AP1 - Texture"
        );
    }

    // Test NamedTransform with strategy = PreferBase, options InputFirst = false.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::PreferBase);
        merger.params(0).unwrap().set_input_first(false);

        let mut merged_config = (*base_config).clone();
        check_for_log_or_exception(
            LogType::Warning,
            || {
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
            },
            &[
                "Equivalent base color space 'ACES2065-1' overrides 'ACES2065-1' in the input config, preserving aliases.",
                "Equivalent base color space 'ACEScct' overrides 'ACEScct - SomeOtherName' in the input config, preserving aliases.",
                "Equivalent base color space 'view_1' overrides 'view_1' in the input config, preserving aliases.",
                "Equivalent base color space 'view_1' overrides 'view_1B' in the input config, preserving aliases.",
                "Equivalent base color space 'view_2' overrides 'view_3' in the input config, preserving aliases.",
                "Equivalent base color space 'log_1' overrides 'log_3' in the input config, preserving aliases.",
                "Equivalent base color space 'ACES2065-1' overrides 'lin_3' in the input config, preserving aliases.",
            ],
        );
        check_for_log_or_exception(
            LogType::Warning,
            || {
                merge_section!(
                    NamedTransformsMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
            },
            &[
                "Merged Base named transform 'nt_both' has a conflict with alias 'srgb_tx' in color space 'sRGB - Texture'",
                "Merged Base named transform 'nt_base' has a conflict with alias 'view_3' in color space 'view_2'.",
                "Named transform 'nt_both' was not merged as it's already present in the base config",
                "Named transform 'nt_input' was not merged as it conflicts with an alias in named transform 'nt_base'",
                "Named transform 'view_2' was not merged as there's a color space with that name",
            ],
        );

        assert_eq!(
            merged_config.num_named_transforms_with(NamedTransformVisibility::All),
            2
        );

        let nt = check_named_transform(&merged_config, "nt_both", 0);
        assert_eq!(nt.num_aliases(), 1);
        assert_eq!(nt.alias(0), b"namet2");
        assert_eq!(nt.family(), b"Base#");
        assert_eq!(nt.description(), b"from base");

        let nt = check_named_transform(&merged_config, "nt_base", 1);
        assert_eq!(nt.num_aliases(), 1);
        assert_eq!(nt.alias(0), b"nt_input");
        assert_eq!(nt.family(), b"Base#nt");
        assert_eq!(nt.description(), b"from base");
    }

    // Test NamedTransform with strategy = BaseOnly.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::BaseOnly);

        let mut merged_config = (*base_config).clone();
        merge_section!(
            ColorspacesMerger,
            &base_config,
            &input_config,
            &params,
            &mut merged_config
        )
        .unwrap();
        merge_section!(
            NamedTransformsMerger,
            &base_config,
            &input_config,
            &params,
            &mut merged_config
        )
        .unwrap();

        assert_eq!(
            merged_config.num_named_transforms_with(NamedTransformVisibility::All),
            2
        );

        let nt = check_named_transform(&merged_config, "nt_both", 0);
        assert_eq!(nt.num_aliases(), 2);
        assert_eq!(nt.alias(0), b"srgb_tx");
        assert_eq!(nt.alias(1), b"namet2");
        assert_eq!(nt.family(), b"");
        assert_eq!(nt.description(), b"from base");

        let nt = check_named_transform(&merged_config, "nt_base", 1);
        assert_eq!(nt.num_aliases(), 2);
        assert_eq!(nt.alias(0), b"view_3");
        assert_eq!(nt.alias(1), b"nt_input");
        assert_eq!(nt.family(), b"nt");
        assert_eq!(nt.description(), b"from base");
    }

    // Test NamedTransform with strategy = InputOnly.
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::InputOnly);

        let mut merged_config = (*base_config).clone();
        merge_section!(
            ColorspacesMerger,
            &base_config,
            &input_config,
            &params,
            &mut merged_config
        )
        .unwrap();
        merge_section!(
            NamedTransformsMerger,
            &base_config,
            &input_config,
            &params,
            &mut merged_config
        )
        .unwrap();

        assert_eq!(
            merged_config.num_named_transforms_with(NamedTransformVisibility::All),
            3
        );

        let nt = check_named_transform(&merged_config, "nt_both", 0);
        assert_eq!(nt.num_aliases(), 2);
        assert_eq!(nt.alias(0), b"Utility - Raw");
        assert_eq!(nt.alias(1), b"nametr");
        assert_eq!(nt.family(), b"");
        assert_eq!(nt.description(), b"from input");

        let nt = check_named_transform(&merged_config, "nt_input", 1);
        assert_eq!(nt.num_aliases(), 2);
        assert_eq!(nt.alias(0), b"Raw");
        assert_eq!(nt.alias(1), b"in nt");
        assert_eq!(nt.family(), b"Raw");
        assert_eq!(nt.description(), b"from input");

        let nt = check_named_transform(&merged_config, "view_2", 2);
        assert_eq!(nt.num_aliases(), 1);
        assert_eq!(nt.alias(0), b"g22_ap1");
        assert_eq!(nt.family(), b"Raw");
        assert_eq!(nt.description(), b"from input");
    }

    // Test NamedTransform with strategy = Remove
    {
        let mut merger = ConfigMerger::new();
        let params = setup_basics(&mut merger, MergeStrategies::Remove);

        let mut merged_config = (*base_config).clone();
        merge_section!(
            ColorspacesMerger,
            &base_config,
            &input_config,
            &params,
            &mut merged_config
        )
        .unwrap();
        merge_section!(
            NamedTransformsMerger,
            &base_config,
            &input_config,
            &params,
            &mut merged_config
        )
        .unwrap();

        assert_eq!(
            merged_config.num_named_transforms_with(NamedTransformVisibility::All),
            1
        );

        let nt = check_named_transform(&merged_config, "nt_base", 0);
        assert_eq!(nt.num_aliases(), 2);
        assert_eq!(nt.alias(0), b"view_3");
        assert_eq!(nt.alias(1), b"nt_input");
        assert_eq!(nt.family(), b"nt");
        assert_eq!(nt.description(), b"from base");
    }
}

/// Upstream's `setupBasics` of the named transforms section errors' test: a merger with new
/// parameters, whose roles, color spaces, named transforms and default strategies are `strategy`,
/// with family prefixes, without the reference space adjustment and the duplicate search.
fn setup_named_transforms_errors_basics(
    merger: &mut ConfigMerger,
    strategy: MergeStrategies,
) -> ConfigMergingParameters {
    let params = ConfigMergingParameters::new().unwrap();
    merger.add_params(Some(params.clone()));
    let p = merger.params(0).unwrap();
    // Note that these tests run several of the mergers.
    p.set_roles(strategy);
    p.set_colorspaces(strategy);
    p.set_named_transforms(strategy);
    p.set_default_strategy(strategy);

    p.set_input_family_prefix("Input/");
    p.set_base_family_prefix("Base/");

    p.set_adjust_input_reference_space(false);
    p.set_avoid_duplicates(false);

    params
}

/// Port of `OCIO_ADD_TEST(MergeConfigs, named_transform_section_errors)` @ v2.5.2.
#[test]
fn named_transform_section_errors() {
    let setup_basics = setup_named_transforms_errors_basics;

    // NT name matches a role name.
    {
        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: A}

roles:
    b: Raw

colorspaces:
- !<ColorSpace>
    name: Raw

named_transforms:
  - !<NamedTransform>
    name: A
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: B}

named_transforms:
  - !<NamedTransform>
    name: B
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}
"#;
            let base_config = Config::create_from_stream(BASE.as_bytes()).unwrap();
            let input_config = Config::create_from_stream(INPUT.as_bytes()).unwrap();

            // The role takes priority.
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
                merger.params(0).unwrap().set_input_first(false);

                let mut merged_config = (*base_config).clone();
                merge_section!(
                    RolesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            NamedTransformsMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &["Named transform 'B' was not merged as it's identical to a role name"],
                );

                assert_eq!(merged_config.num_roles(), 1);
                assert_eq!(merged_config.role_name(0), b"b");

                // NamedTransform B should not be added to the merged config (skipped).
                assert_eq!(
                    merged_config.num_named_transforms_with(NamedTransformVisibility::All),
                    1
                );
                assert_eq!(merged_config.named_transform_name_by_index(0), b"A");
            }
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferBase);
                merger.params(0).unwrap().set_input_first(false);

                let mut merged_config = (*base_config).clone();
                merge_section!(
                    RolesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            NamedTransformsMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &["Named transform 'B' was not merged as it's identical to a role name"],
                );

                assert_eq!(merged_config.num_roles(), 1);
                assert_eq!(merged_config.role_name(0), b"b");

                // NamedTransform B should not be added to the merged config (skipped)
                assert_eq!(
                    merged_config.num_named_transforms_with(NamedTransformVisibility::All),
                    1
                );
                assert_eq!(merged_config.named_transform_name_by_index(0), b"A");
            }
            // Testing the error message when Error on conflict is enabled.
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
                merger.params(0).unwrap().set_error_on_conflict(true);

                let mut merged_config = (*base_config).clone();
                check_throw_what(
                    merge_section!(
                        NamedTransformsMerger,
                        &base_config,
                        &input_config,
                        &params,
                        &mut merged_config
                    ),
                    "Named transform 'B' was not merged as it's identical to a role name",
                );
            }
        }

        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: A}

named_transforms:
  - !<NamedTransform>
    name: A
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: B}

roles:
    a: B

colorspaces:
- !<ColorSpace>
    name: B
"#;
            let base_config = Config::create_from_stream(BASE.as_bytes()).unwrap();
            let input_config = Config::create_from_stream(INPUT.as_bytes()).unwrap();

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
                        "The Input config contains a role 'a' that would override Base config named transform: 'A'",
                    ],
                );
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                merge_section!(
                    NamedTransformsMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();

                assert_eq!(merged_config.num_roles(), 0);

                assert_eq!(
                    merged_config.num_named_transforms_with(NamedTransformVisibility::All),
                    1
                );
                check_named_transform(&merged_config, "A", 0);
            }
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
                        "The Input config contains a role 'a' that would override Base config named transform: 'A'",
                    ],
                );
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                merge_section!(
                    NamedTransformsMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();

                assert_eq!(merged_config.num_roles(), 0);

                assert_eq!(
                    merged_config.num_named_transforms_with(NamedTransformVisibility::All),
                    1
                );
                check_named_transform(&merged_config, "A", 0);
            }
        }
    }

    // NT name matches a color space name.
    {
        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: B}

colorspaces:
- !<ColorSpace>
    name: B
- !<ColorSpace>
    name: myB
    aliases: [B1]
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: csB}

colorspaces:
- !<ColorSpace>
    name: csB

named_transforms:
  - !<NamedTransform>
    name: B
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}

  - !<NamedTransform>
    name: B1
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}
"#;
            let base_config = Config::create_from_stream(BASE.as_bytes()).unwrap();
            let input_config = Config::create_from_stream(INPUT.as_bytes()).unwrap();

            // The role take priority over the inbound colorspace.
            // The conflicting colorspace should not be added to the merged config (skipped).
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
                merger.params(0).unwrap().set_input_first(false);

                let mut merged_config = (*base_config).clone();
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            NamedTransformsMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &[
                        "Named transform 'B' was not merged as there's a color space with that name",
                        "Named transform 'B1' was not merged as there's a color space alias with that name",
                    ],
                );

                assert_eq!(
                    merged_config.num_named_transforms_with(NamedTransformVisibility::All),
                    0
                );

                assert_eq!(num_all_color_spaces(&merged_config), 3);

                check_color_space(&merged_config, "B", 0, SEARCH_SCENE);
                let cs = check_color_space(&merged_config, "myB", 1, SEARCH_SCENE);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), b"B1");
                check_color_space(&merged_config, "csB", 2, SEARCH_SCENE);
            }
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferBase);
                merger.params(0).unwrap().set_input_first(false);

                let mut merged_config = (*base_config).clone();
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            NamedTransformsMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &[
                        "Named transform 'B' was not merged as there's a color space with that name",
                        "Named transform 'B1' was not merged as there's a color space alias with that name",
                    ],
                );

                assert_eq!(
                    merged_config.num_named_transforms_with(NamedTransformVisibility::All),
                    0
                );

                assert_eq!(num_all_color_spaces(&merged_config), 3);

                check_color_space(&merged_config, "B", 0, SEARCH_SCENE);

                let cs = check_color_space(&merged_config, "myB", 1, SEARCH_SCENE);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), b"B1");

                check_color_space(&merged_config, "csB", 2, SEARCH_SCENE);
            }
            // Testing the error message when Error on conflict is enabled.
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
                merger.params(0).unwrap().set_error_on_conflict(true);

                let mut merged_config = (*base_config).clone();
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_throw_what(
                    merge_section!(
                        NamedTransformsMerger,
                        &base_config,
                        &input_config,
                        &params,
                        &mut merged_config
                    ),
                    "Named transform 'B' was not merged as there's a color space with that name",
                );
            }
        }

        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: csA}

colorspaces:
- !<ColorSpace>
    name: csA

named_transforms:
  - !<NamedTransform>
    name: A
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}

  - !<NamedTransform>
    name: A1
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: A}

colorspaces:
- !<ColorSpace>
    name: A
- !<ColorSpace>
    name: myA
    aliases: [A1]
"#;
            let base_config = Config::create_from_stream(BASE.as_bytes()).unwrap();
            let input_config = Config::create_from_stream(INPUT.as_bytes()).unwrap();
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferInput);

                let mut merged_config = (*base_config).clone();
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            NamedTransformsMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &[
                        "Named transform 'A' was not merged as there's a color space with that name",
                        "Named transform 'A1' was not merged as there's a color space alias with that name",
                    ],
                );

                assert_eq!(
                    merged_config.num_named_transforms_with(NamedTransformVisibility::All),
                    0
                );

                assert_eq!(num_all_color_spaces(&merged_config), 3);

                check_color_space(&merged_config, "A", 0, SEARCH_SCENE);
                let cs = check_color_space(&merged_config, "myA", 1, SEARCH_SCENE);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), b"A1");
                check_color_space(&merged_config, "csA", 2, SEARCH_SCENE);
            }
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferBase);

                let mut merged_config = (*base_config).clone();
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            NamedTransformsMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &[
                        "Named transform 'A' was not merged as there's a color space with that name",
                        "Named transform 'A1' was not merged as there's a color space alias with that name",
                    ],
                );

                assert_eq!(
                    merged_config.num_named_transforms_with(NamedTransformVisibility::All),
                    0
                );

                assert_eq!(num_all_color_spaces(&merged_config), 3);

                check_color_space(&merged_config, "A", 0, SEARCH_SCENE);
                let cs = check_color_space(&merged_config, "myA", 1, SEARCH_SCENE);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), b"A1");
                check_color_space(&merged_config, "csA", 2, SEARCH_SCENE);
            }
        }
    }

    // NT name matches an NT alias.
    {
        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: csA}

colorspaces:
- !<ColorSpace>
    name: csA

named_transforms:
  - !<NamedTransform>
    name: A
    aliases: [B]
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: csB}

colorspaces:
- !<ColorSpace>
    name: csB

named_transforms:
  - !<NamedTransform>
    name: B
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}
"#;
            let base_config = Config::create_from_stream(BASE.as_bytes()).unwrap();
            let input_config = Config::create_from_stream(INPUT.as_bytes()).unwrap();
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
                merger.params(0).unwrap().set_input_first(false);

                let mut merged_config = (*base_config).clone();
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            NamedTransformsMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &[
                        "The name of merged named transform 'B' has a conflict with an alias in named transform 'A'",
                    ],
                );

                assert_eq!(num_all_color_spaces(&merged_config), 2);

                assert_eq!(
                    merged_config.num_named_transforms_with(NamedTransformVisibility::All),
                    2
                );

                let nt = check_named_transform(&merged_config, "A", 0);
                assert_eq!(nt.num_aliases(), 0);
                check_named_transform(&merged_config, "B", 1);
            }
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
                merger.params(0).unwrap().set_input_first(false);

                let mut merged_config = (*base_config).clone();
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            NamedTransformsMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &[
                        "The name of merged named transform 'B' has a conflict with an alias in named transform 'A'",
                    ],
                );

                assert_eq!(num_all_color_spaces(&merged_config), 2);

                assert_eq!(
                    merged_config.num_named_transforms_with(NamedTransformVisibility::All),
                    2
                );

                let nt = check_named_transform(&merged_config, "A", 0);
                assert_eq!(nt.num_aliases(), 0);
                check_named_transform(&merged_config, "B", 1);
            }
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
                merger.params(0).unwrap().set_error_on_conflict(true);

                let mut merged_config = (*base_config).clone();
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_throw_what(
                    merge_section!(
                        NamedTransformsMerger,
                        &base_config,
                        &input_config,
                        &params,
                        &mut merged_config
                    ),
                    "The name of merged named transform 'B' has a conflict with an alias in named transform 'A'",
                );
            }
        }

        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: csA}

colorspaces:
- !<ColorSpace>
    name: csA

named_transforms:
  - !<NamedTransform>
    name: A
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: B}

colorspaces:
- !<ColorSpace>
    name: csB

named_transforms:
  - !<NamedTransform>
    name: B
    aliases: [A]
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}
"#;
            let base_config = Config::create_from_stream(BASE.as_bytes()).unwrap();
            let input_config = Config::create_from_stream(INPUT.as_bytes()).unwrap();
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferInput);

                let mut merged_config = (*base_config).clone();
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            NamedTransformsMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &[
                        "Merged Input named transform 'B' has an alias 'A' that conflicts with named transform 'A'",
                    ],
                );

                assert_eq!(num_all_color_spaces(&merged_config), 2);

                assert_eq!(
                    merged_config.num_named_transforms_with(NamedTransformVisibility::All),
                    1
                );

                let nt = check_named_transform(&merged_config, "B", 0);
                assert_eq!(nt.num_aliases(), 1);
                assert_eq!(nt.alias(0), b"A");
            }
            {
                let mut merger = ConfigMerger::new();
                let mut merged_config = (*base_config).clone();

                let params = setup_basics(&mut merger, MergeStrategies::PreferBase);

                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            NamedTransformsMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &[
                        "Merged Input named transform 'B' has an alias 'A' that conflicts with named transform 'A'",
                    ],
                );

                assert_eq!(num_all_color_spaces(&merged_config), 2);

                assert_eq!(
                    merged_config.num_named_transforms_with(NamedTransformVisibility::All),
                    2
                );

                let nt = check_named_transform(&merged_config, "B", 0);
                assert_eq!(nt.num_aliases(), 0);
                let nt = check_named_transform(&merged_config, "A", 1);
                assert_eq!(nt.num_aliases(), 0);
            }
        }
    }

    // NT alias matches a role name.
    {
        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: csA}

roles:
    b1: csA

colorspaces:
- !<ColorSpace>
    name: csA
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: csB}

colorspaces:
- !<ColorSpace>
    name: csB

named_transforms:
  - !<NamedTransform>
    name: B
    aliases: [B1]
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}
"#;
            let base_config = Config::create_from_stream(BASE.as_bytes()).unwrap();
            let input_config = Config::create_from_stream(INPUT.as_bytes()).unwrap();
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
                merger.params(0).unwrap().set_input_first(false);

                let mut merged_config = (*base_config).clone();
                merge_section!(
                    RolesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            NamedTransformsMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &[
                        "Merged Input named transform 'B' has an alias 'B1' that conflicts with a role",
                    ],
                );

                assert_eq!(merged_config.num_roles(), 1);
                assert_eq!(merged_config.role_name(0), b"b1");

                assert_eq!(num_all_color_spaces(&merged_config), 2);

                assert_eq!(
                    merged_config.num_named_transforms_with(NamedTransformVisibility::All),
                    1
                );

                let nt = check_named_transform(&merged_config, "B", 0);
                assert_eq!(nt.num_aliases(), 0);
            }
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferBase);
                merger.params(0).unwrap().set_input_first(false);

                let mut merged_config = (*base_config).clone();
                merge_section!(
                    RolesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            NamedTransformsMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &[
                        "Merged Input named transform 'B' has an alias 'B1' that conflicts with a role",
                    ],
                );

                assert_eq!(merged_config.num_roles(), 1);
                assert_eq!(merged_config.role_name(0), b"b1");

                assert_eq!(num_all_color_spaces(&merged_config), 2);

                assert_eq!(
                    merged_config.num_named_transforms_with(NamedTransformVisibility::All),
                    1
                );

                let nt = check_named_transform(&merged_config, "B", 0);
                assert_eq!(nt.num_aliases(), 0);
            }
            // Testing the error message when Error on conflict is enabled.
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
                merger.params(0).unwrap().set_error_on_conflict(true);

                let mut merged_config = (*base_config).clone();
                merge_section!(
                    RolesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_throw_what(
                    merge_section!(
                        NamedTransformsMerger,
                        &base_config,
                        &input_config,
                        &params,
                        &mut merged_config
                    ),
                    "Merged Input named transform 'B' has an alias 'B1' that conflicts with a role",
                );
            }
        }

        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: csA}

colorspaces:
- !<ColorSpace>
    name: csA

named_transforms:
  - !<NamedTransform>
    name: B
    aliases: [A1]
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: csB}

roles:
    a1: csB

colorspaces:
- !<ColorSpace>
    name: csB
"#;
            let base_config = Config::create_from_stream(BASE.as_bytes()).unwrap();
            let input_config = Config::create_from_stream(INPUT.as_bytes()).unwrap();
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
                merger.params(0).unwrap().set_input_first(false);

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
                        "The Input config contains a role 'a1' that would override an alias of Base config named transform: 'B'",
                    ],
                );
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                merge_section!(
                    NamedTransformsMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();

                assert_eq!(merged_config.num_roles(), 0);

                assert_eq!(num_all_color_spaces(&merged_config), 2);

                assert_eq!(
                    merged_config.num_named_transforms_with(NamedTransformVisibility::All),
                    1
                );

                let nt = check_named_transform(&merged_config, "B", 0);
                assert_eq!(nt.num_aliases(), 1);
                assert_eq!(nt.alias(0), b"A1");
            }
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
                merger.params(0).unwrap().set_input_first(false);

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
                        "The Input config contains a role 'a1' that would override an alias of Base config named transform: 'B'",
                    ],
                );
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                merge_section!(
                    NamedTransformsMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();

                assert_eq!(merged_config.num_roles(), 0);

                assert_eq!(num_all_color_spaces(&merged_config), 2);

                assert_eq!(
                    merged_config.num_named_transforms_with(NamedTransformVisibility::All),
                    1
                );

                let nt = check_named_transform(&merged_config, "B", 0);
                assert_eq!(nt.num_aliases(), 1);
                assert_eq!(nt.alias(0), b"A1");
            }
        }
    }

    // NT name matches color space alias.
    {
        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: csA}

colorspaces:
- !<ColorSpace>
    name: csA
- !<ColorSpace>
    name: B
- !<ColorSpace>
    name: myB
    aliases: [B1]
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: csB}

colorspaces:
- !<ColorSpace>
    name: csB

named_transforms:
  - !<NamedTransform>
    name: B
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}

  - !<NamedTransform>
    name: B1
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}
"#;
            let base_config = Config::create_from_stream(BASE.as_bytes()).unwrap();
            let input_config = Config::create_from_stream(INPUT.as_bytes()).unwrap();

            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
                merger.params(0).unwrap().set_input_first(false);

                let mut merged_config = (*base_config).clone();
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            NamedTransformsMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &[
                        "Named transform 'B' was not merged as there's a color space with that name",
                        "Named transform 'B1' was not merged as there's a color space alias with that name",
                    ],
                );

                assert_eq!(
                    merged_config.num_named_transforms_with(NamedTransformVisibility::All),
                    0
                );

                assert_eq!(num_all_color_spaces(&merged_config), 4);

                check_color_space(&merged_config, "csA", 0, SEARCH_SCENE);
                check_color_space(&merged_config, "B", 1, SEARCH_SCENE);
                let cs = check_color_space(&merged_config, "myB", 2, SEARCH_SCENE);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), b"B1");
                check_color_space(&merged_config, "csB", 3, SEARCH_SCENE);
            }
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferBase);
                merger.params(0).unwrap().set_input_first(false);

                let mut merged_config = (*base_config).clone();
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            NamedTransformsMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &[
                        "Named transform 'B' was not merged as there's a color space with that name",
                        "Named transform 'B1' was not merged as there's a color space alias with that name",
                    ],
                );

                assert_eq!(
                    merged_config.num_named_transforms_with(NamedTransformVisibility::All),
                    0
                );

                assert_eq!(num_all_color_spaces(&merged_config), 4);

                check_color_space(&merged_config, "csA", 0, SEARCH_SCENE);
                check_color_space(&merged_config, "B", 1, SEARCH_SCENE);
                let cs = check_color_space(&merged_config, "myB", 2, SEARCH_SCENE);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), b"B1");
                check_color_space(&merged_config, "csB", 3, SEARCH_SCENE);
            }
            // Testing the error message when Error on conflict is enabled.
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
                merger.params(0).unwrap().set_error_on_conflict(true);

                let mut merged_config = (*base_config).clone();
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_throw_what(
                    merge_section!(
                        NamedTransformsMerger,
                        &base_config,
                        &input_config,
                        &params,
                        &mut merged_config
                    ),
                    "Named transform 'B' was not merged as there's a color space with that name",
                );
            }
        }

        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: csA}

colorspaces:
- !<ColorSpace>
    name: csA

named_transforms:
  - !<NamedTransform>
    name: A
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}

  - !<NamedTransform>
    name: A1
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: csB}

colorspaces:
- !<ColorSpace>
    name: csB
- !<ColorSpace>
    name: A
- !<ColorSpace>
    name: myA
    aliases: [A1]
"#;
            let base_config = Config::create_from_stream(BASE.as_bytes()).unwrap();
            let input_config = Config::create_from_stream(INPUT.as_bytes()).unwrap();
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
                merger.params(0).unwrap().set_input_first(false);

                let mut merged_config = (*base_config).clone();
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            NamedTransformsMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &[
                        "Named transform 'A' was not merged as there's a color space with that name",
                        "Named transform 'A1' was not merged as there's a color space alias with that name",
                    ],
                );

                assert_eq!(
                    merged_config.num_named_transforms_with(NamedTransformVisibility::All),
                    0
                );

                assert_eq!(num_all_color_spaces(&merged_config), 4);

                check_color_space(&merged_config, "csA", 0, SEARCH_SCENE);
                check_color_space(&merged_config, "csB", 1, SEARCH_SCENE);
                check_color_space(&merged_config, "A", 2, SEARCH_SCENE);
                let cs = check_color_space(&merged_config, "myA", 3, SEARCH_SCENE);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), b"A1");
            }
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferBase);
                merger.params(0).unwrap().set_input_first(false);

                let mut merged_config = (*base_config).clone();
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            NamedTransformsMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &[
                        "Named transform 'A' was not merged as there's a color space with that name",
                        "Named transform 'A1' was not merged as there's a color space alias with that name",
                    ],
                );

                assert_eq!(
                    merged_config.num_named_transforms_with(NamedTransformVisibility::All),
                    0
                );

                assert_eq!(num_all_color_spaces(&merged_config), 4);

                check_color_space(&merged_config, "csA", 0, SEARCH_SCENE);
                check_color_space(&merged_config, "csB", 1, SEARCH_SCENE);
                check_color_space(&merged_config, "A", 2, SEARCH_SCENE);
                let cs = check_color_space(&merged_config, "myA", 3, SEARCH_SCENE);
                assert_eq!(cs.num_aliases(), 1);
                assert_eq!(cs.alias(0), b"A1");
            }
        }
    }

    // NT alias matches existing NT alias.
    {
        {
            const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: csA}

colorspaces:
- !<ColorSpace>
    name: csA

named_transforms:
  - !<NamedTransform>
    name: A
    aliases: [my_colorspace]
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}
"#;

            const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: csB}

colorspaces:
- !<ColorSpace>
    name: csB

named_transforms:
  - !<NamedTransform>
    name: B
    aliases: [my_colorspace]
    categories: [ working-space, basic-3d, advanced-2d ]
    encoding: sdr-video
    transform: !<MatrixTransform> {name: forwardBase, offset: [0.1, 0.2, 0.3, 0.4]}
"#;
            let base_config = Config::create_from_stream(BASE.as_bytes()).unwrap();
            let input_config = Config::create_from_stream(INPUT.as_bytes()).unwrap();

            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
                merger.params(0).unwrap().set_input_first(false);

                let mut merged_config = (*base_config).clone();
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            NamedTransformsMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &[
                        "Merged Input named transform 'B' has a conflict with alias 'my_colorspace' in named transform 'A'",
                    ],
                );

                assert_eq!(num_all_color_spaces(&merged_config), 2);

                assert_eq!(
                    merged_config.num_named_transforms_with(NamedTransformVisibility::All),
                    2
                );

                let nt = check_named_transform(&merged_config, "A", 0);
                assert_eq!(nt.num_aliases(), 0);
                let nt = check_named_transform(&merged_config, "B", 1);
                assert_eq!(nt.num_aliases(), 1);
                assert_eq!(nt.alias(0), b"my_colorspace");
            }
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferInput);

                let mut merged_config = (*base_config).clone();
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            NamedTransformsMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &[
                        "Merged Input named transform 'B' has a conflict with alias 'my_colorspace' in named transform 'A'",
                    ],
                );

                assert_eq!(num_all_color_spaces(&merged_config), 2);

                assert_eq!(
                    merged_config.num_named_transforms_with(NamedTransformVisibility::All),
                    2
                );

                let nt = check_named_transform(&merged_config, "B", 0);
                assert_eq!(nt.num_aliases(), 1);
                assert_eq!(nt.alias(0), b"my_colorspace");
                let nt = check_named_transform(&merged_config, "A", 1);
                assert_eq!(nt.num_aliases(), 0);
            }
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferBase);
                merger.params(0).unwrap().set_input_first(false);

                let mut merged_config = (*base_config).clone();
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            NamedTransformsMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &[
                        "Merged Input named transform 'B' has a conflict with alias 'my_colorspace' in named transform 'A'",
                    ],
                );

                assert_eq!(num_all_color_spaces(&merged_config), 2);

                assert_eq!(
                    merged_config.num_named_transforms_with(NamedTransformVisibility::All),
                    2
                );

                let nt = check_named_transform(&merged_config, "A", 0);
                assert_eq!(nt.num_aliases(), 1);
                assert_eq!(nt.alias(0), b"my_colorspace");
                let nt = check_named_transform(&merged_config, "B", 1);
                assert_eq!(nt.num_aliases(), 0);
            }
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferBase);

                let mut merged_config = (*base_config).clone();
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_for_log_or_exception(
                    LogType::Warning,
                    || {
                        merge_section!(
                            NamedTransformsMerger,
                            &base_config,
                            &input_config,
                            &params,
                            &mut merged_config
                        )
                    },
                    &[
                        "Merged Input named transform 'B' has a conflict with alias 'my_colorspace' in named transform 'A'",
                    ],
                );

                assert_eq!(num_all_color_spaces(&merged_config), 2);

                assert_eq!(
                    merged_config.num_named_transforms_with(NamedTransformVisibility::All),
                    2
                );

                let nt = check_named_transform(&merged_config, "B", 0);
                assert_eq!(nt.num_aliases(), 0);
                let nt = check_named_transform(&merged_config, "A", 1);
                assert_eq!(nt.num_aliases(), 1);
                assert_eq!(nt.alias(0), b"my_colorspace");
            }
            // Testing the error message when Error on conflict is enabled.
            {
                let mut merger = ConfigMerger::new();
                let params = setup_basics(&mut merger, MergeStrategies::PreferInput);
                merger.params(0).unwrap().set_error_on_conflict(true);

                let mut merged_config = (*base_config).clone();
                merge_section!(
                    ColorspacesMerger,
                    &base_config,
                    &input_config,
                    &params,
                    &mut merged_config
                )
                .unwrap();
                check_throw_what(
                    merge_section!(
                        NamedTransformsMerger,
                        &base_config,
                        &input_config,
                        &params,
                        &mut merged_config
                    ),
                    "Merged Input named transform 'B' has a conflict with alias 'my_colorspace' in named transform 'A'",
                );
            }
        }
    }
}

/// `pystring::os::path::normpath(pystring::os::path::join(paths))` of the test files'
/// directory, `configs` and `parts`.
fn configs_test_file(parts: &[&str]) -> Vec<u8> {
    let joined = std::iter::once("configs")
        .chain(parts.iter().copied())
        .fold(get_test_files_dir().into_bytes(), |acc, p| {
            os_path::join(&acc, p.as_bytes())
        });
    os_path::normpath(&joined)
}

/// The first transform of the group of a processor between two color spaces, as a matrix: its
/// first value.
#[track_caller]
fn first_matrix_value(config: &Config, ctx: &Context, src: &str, dst: &str) -> f64 {
    let processor = config
        .processor_between_with_context(ctx, src, dst)
        .unwrap();
    let group = processor.create_group_transform().unwrap();
    let tr = group.transform(0).unwrap();
    let Transform::Matrix(mtx) = tr else {
        panic!("not a MatrixTransform: {tr}");
    };
    mtx.matrix()[0]
}

/// Port of `OCIO_ADD_TEST(MergeConfigs, merges_with_ociom_file)` @ v2.5.2.
#[test]
fn merges_with_ociom_file() {
    {
        let ociom_path = configs_test_file(&["mergeconfigs", "merged1", "merged1.ociom"]);

        // PreferInput, Input first
        {
            let merger = ConfigMerger::from_file(&ociom_path).unwrap();
            let mut new_merger = None;
            check_for_log_or_exception(
                LogType::Warning,
                || {
                    new_merger = Some(merger.merge_configs()?);
                    Ok(())
                },
                &[
                    "The Input config contains a value that would override the Base config: file_rules: Default",
                    "Merged color space 'ACES2065-1' has a conflict with alias 'aces' in color space 'ACEScg'",
                    "Equivalent input color space 'sRGB - Display' replaces 'sRGB - Display' in the base config, preserving aliases.",
                    "Equivalent input color space 'CIE-XYZ-D65' replaces 'CIE-XYZ-D65' in the base config, preserving aliases.",
                    "Equivalent input color space 'ACES2065-1' replaces 'ap0' in the base config, preserving aliases.",
                    "Equivalent input color space 'sRGB' replaces 'sRGB - Texture' in the base config, preserving aliases.",
                ],
            );
            let merged_config = new_merger.unwrap().last_merged_config().unwrap();

            // This test is essentially the same as the first sub-test in the above test
            // colorspaces_section_common_reference_and_duplicates.  So just do a quick
            // sanity test here rather than redoing all the same checks.

            assert_eq!(merged_config.name(), b"Merged1");
            assert_eq!(
                merged_config.description(),
                b"Basic merge with default strategy"
            );
            assert_eq!(num_all_color_spaces(&merged_config), 7);
        }
    }

    // Test is similar to the previous one but it has two merges in the
    // OCIOM file and it is using the output of the first merged config
    // as the input for the second merge.
    {
        let ociom_path = configs_test_file(&["mergeconfigs", "merged2", "merged.ociom"]);

        {
            let merger = ConfigMerger::from_file(&ociom_path).unwrap();
            let mut new_merger = None;
            check_for_log_or_exception(
                LogType::Warning,
                || {
                    new_merger = Some(merger.merge_configs()?);
                    Ok(())
                },
                &[
                    "The Input config contains a value that would override the Base config: file_rules: Default",
                    "The Input config contains a role that would override Base config role 'cie_xyz_d65_interchange'",
                    "Color space 'sRGB - Display' was not merged as it's already present in the base config",
                    "Color space 'CIE-XYZ-D65' was not merged as it's already present in the base config",
                    "Merged color space 'ACES2065-1' has a conflict with alias 'aces' in color space 'ACEScg'",
                    "Color space 'sRGB' was not merged as it conflicts with an alias in color space 'sRGB - Texture'",
                    "Equivalent base color space 'ap0' overrides 'rec709' in the input config, preserving aliases",
                ],
            );
            let new_merger = new_merger.unwrap();

            assert_eq!(new_merger.num_merged_configs(), 2);

            let merged_config = new_merger.last_merged_config().unwrap();

            merged_config.validate().unwrap();

            assert_eq!(merged_config.name(), b"Merged2");
            assert_eq!(merged_config.description(), b"Description override");
            assert_eq!(num_all_color_spaces(&merged_config), 7);

            // The second merge replaces the file rules of the first merge with the input config.
            let fr = merged_config.file_rules().get();
            assert_eq!(fr.num_entries(), 1);
            assert_eq!(fr.name(0).unwrap(), b"Default");
            assert_eq!(fr.color_space(0).unwrap(), b"sRGB");

            // The second merge replaces the roles of the first merge with the input config.
            assert_eq!(merged_config.num_roles(), 1);
            assert_eq!(
                merged_config.role_color_space("cie_xyz_d65_interchange"),
                b"CIE-XYZ-D65"
            );

            // The rest of the config should be the result of the first merge.

            assert_eq!(merged_config.num_displays_all(), 1);
            assert_eq!(merged_config.display(0), b"sRGB - Display");
            assert_eq!(
                merged_config.num_views_of_type(ViewType::DisplayDefined, "sRGB - Display"),
                2
            );

            let cs = check_color_space(&merged_config, "sRGB - Display", 0, SEARCH_DISPLAY);
            assert_eq!(cs.num_aliases(), 1);
            assert_eq!(cs.alias(0), b"srgb_display");
            assert_eq!(cs.family(), b"Display-Basic");
            assert_eq!(cs.description(), b"from base");

            let cs = check_color_space(&merged_config, "ACES2065-1", 0, SEARCH_SCENE);
            assert_eq!(cs.num_aliases(), 0);
            assert_eq!(cs.family(), b"ACES~Linear");
            assert_eq!(cs.description(), b"from input");

            assert_eq!(merged_config.inactive_color_spaces(), b"ACES2065-1");
        }
    }

    // Test with external LUT files.
    {
        let ociom_path = configs_test_file(&["mergeconfigs", "merged3", "merged.ociom"]);

        // PreferInput, Input first
        {
            let merger = ConfigMerger::from_file(&ociom_path).unwrap();
            let mut new_merger = None;
            check_for_log_or_exception(
                LogType::Warning,
                || {
                    new_merger = Some(merger.merge_configs()?);
                    Ok(())
                },
                &["Color space 'raw' will replace a color space in the base config."],
            );
            let merged_config = new_merger.unwrap().last_merged_config().unwrap();
            merged_config.validate().unwrap();

            assert_eq!(merged_config.search_path(), b"./$SHOT:./shot1:shot2:.");
            let cs = merged_config.color_space("shot1_lut1_cs").unwrap();
            let tf = cs.transform(ColorSpaceDirection::ToReference).unwrap();
            let Transform::File(ftf) = tf else {
                panic!("not a FileTransform: {tf}");
            };
            assert_eq!(ftf.src(), b"shot1/lut1.clf");
            merged_config.processor(tf).unwrap();

            let look = merged_config.look("shot_look").unwrap();
            let ltf = look.transform().unwrap();
            merged_config.processor(ltf).unwrap();
        }
    }

    // Test that a merge could go wrong if the search_paths are merged with a different strategy
    // than the other sections.
    {
        let ociom_path = configs_test_file(&["mergeconfigs", "merged3", "merged.ociom"]);

        {
            let merger = ConfigMerger::from_file(&ociom_path).unwrap();
            // Changing the strategy for colorspace merger to INPUT_ONLY.
            // This will break the looks "shot_look" (from base) as it needs the search paths
            // from the base config (search_paths are managed by the colorspace merger).
            merger
                .params(0)
                .unwrap()
                .set_colorspaces(MergeStrategies::InputOnly);
            // The rest of the merges uses PreferInput strategy.

            let new_merger = merger.merge_configs().unwrap();
            let merged_config = new_merger.last_merged_config().unwrap();
            assert!(merged_config.config_io_proxy().is_none());

            let look = merged_config.look("shot_look").unwrap();
            let ltf = look.transform().unwrap();

            // Expected to throw as the search_paths were merged following the InputOnly strategy
            // and the looks were merged following the PreferInput (see OCIOM file default strategy).
            // Therefore, the look's FileTransform can not find "look.cdl" and throws an exception.
            assert!(merged_config.processor(ltf).is_err());

            // It can happen with any section that uses the search_paths such as looks,
            // named transforms, and colorspaces.
        }
    }

    // Test with a built-in config.
    {
        let ociom_path = configs_test_file(&["mergeconfigs", "merged4", "merged.ociom"]);

        // InputOnly
        {
            let merger = ConfigMerger::from_file(&ociom_path).unwrap();
            let new_merger = merger.merge_configs().unwrap();
            let merged_config = new_merger.last_merged_config().unwrap();

            merged_config.validate().unwrap();

            assert_eq!(
                merged_config.name(),
                b"cg-config-v1.0.0_aces-v1.3_ocio-v2.1"
            );
            assert_eq!(num_all_color_spaces(&merged_config), 20);
        }
    }
}

/// Upstream's merge parameters of the in-memory merge tests: not input first, `PreferInput`
/// for the roles, color spaces, named transforms and the default, with family prefixes,
/// without the reference space adjustment and the duplicate search.
fn in_memory_params() -> ConfigMergingParameters {
    let params = ConfigMergingParameters::new().unwrap();
    params.set_input_first(false);
    let strategy = MergeStrategies::PreferInput;
    params.set_roles(strategy);
    params.set_colorspaces(strategy);
    params.set_named_transforms(strategy);
    params.set_default_strategy(strategy);
    params.set_input_family_prefix("Input/");
    params.set_base_family_prefix("Base/");
    params.set_adjust_input_reference_space(false);
    params.set_avoid_duplicates(false);
    params
}

/// Port of `OCIO_ADD_TEST(MergeConfigs, merges_with_ocioz_file)` @ v2.5.2.
#[test]
fn merges_with_ocioz_file() {
    let archive_path_linux = configs_test_file(&["context_test1", "context_test1_linux.ocioz"]);
    let base_config = Config::create_from_file(&archive_path_linux).unwrap();

    let config_path_input = configs_test_file(&["mergeconfigs", "merged3", "input.ocio"]);
    let input_config = Config::create_from_file(&config_path_input).unwrap();

    let params = in_memory_params();

    {
        let mut merged_config = None;
        check_for_log_or_exception(
            LogType::Warning,
            || {
                merged_config = Some(config_merging_helpers::merge_configs(
                    Some(&params),
                    Some(&base_config),
                    Some(&input_config),
                )?);
                Ok(())
            },
            &[
                "Color space 'reference' will replace a color space in the base config.",
                "Color space 'raw' will replace a color space in the base config.",
                "Color space 'plain_lut1_cs' will replace a color space in the base config.",
                "Color space 'shot1_lut1_cs' will replace a color space in the base config.",
            ],
        );
        let merged_config = merged_config.unwrap();

        // The working dir is empty.
        assert_eq!(merged_config.working_dir(), b"");
        // The configIOProxy is not NULL.
        assert!(merged_config.config_io_proxy().is_some());

        let mut ctx = (*merged_config.current_context().get()).clone();

        // This is resolved in the OCIOZ base.
        {
            // Note: the $SHOT = shot4 search path takes precedence for this color space.
            let mat0 = first_matrix_value(&merged_config, &ctx, "plain_lut1_cs", "reference");
            assert_eq!(mat0, 40.); // is 40 in the base config, 20 in input
        }
        {
            let mat0 = first_matrix_value(&merged_config, &ctx, "shot1_lut1_cs", "reference");
            assert_eq!(mat0, 10.); // is 10 in the base config, 100 in input
        }
        {
            // Will try to resolve it using relative paths and won't find it.
            assert!(
                merged_config
                    .processor_between_with_context(&ctx, "shot1_lut2_cs", "reference")
                    .is_err()
            );
        }

        // Add an absolute search path for the input config.
        let search_path_input = configs_test_file(&["mergeconfigs", "merged3"]);

        ctx.clear_search_paths();
        ctx.add_search_path(&search_path_input);
        for i in 0..merged_config.num_search_paths() {
            ctx.add_search_path(merged_config.search_path_with_index(i));
        }

        {
            let mat0 = first_matrix_value(&merged_config, &ctx, "shot1_lut2_cs", "reference");
            assert_eq!(mat0, 42.); // doesn't exist in the base config
        }

        // TODO: The next test won't work without this.  Is that expected that you need to clear
        // the cache after adding a search path, or is there a bug involving the cacheID of processors
        // when OCIOZ is being used?
        merged_config.clear_processor_cache();

        {
            let mat0 = first_matrix_value(&merged_config, &ctx, "shot1_lut1_cs", "reference");
            assert_eq!(mat0, 100.); // is 10 in the base config, 100 in input
        }
    }
}

/// Port of `OCIO_ADD_TEST(MergeConfigs, merge_in_memory_configs)` @ v2.5.2.
#[test]
fn merge_in_memory_configs() {
    const BASE: &str = r#"ocio_profile_version: 2.1

file_rules:
  - !<Rule> {name: Default, colorspace: A}

roles:
    a: colorspace_a

colorspaces:
- !<ColorSpace>
    name: colorspace_a
    family: utility
"#;

    const INPUT: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: B}

colorspaces:
- !<ColorSpace>
    name: B
    family: aces
"#;

    const RESULT: &str = r#"ocio_profile_version: 2.1

roles:
  a: colorspace_a

file_rules:
  - !<Rule> {name: Default, colorspace: B}

colorspaces:
  - !<ColorSpace>
    name: colorspace_a
    family: Base/utility
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform

  - !<ColorSpace>
    name: B
    family: Input/aces
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform"#;

    let base_config = Config::create_from_stream(BASE.as_bytes()).unwrap();
    let input_config = Config::create_from_stream(INPUT.as_bytes()).unwrap();

    let params = in_memory_params();

    let mut merged_config = None;
    check_for_log_or_exception(
        LogType::Warning,
        || {
            merged_config = Some(config_merging_helpers::merge_configs(
                Some(&params),
                Some(&base_config),
                Some(&input_config),
            )?);
            Ok(())
        },
        &[
            "The Input config contains a value that would override the Base config: file_rules: Default",
        ],
    );

    let oss = merged_config.unwrap().serialize().unwrap();

    let result_config = Config::create_from_stream(RESULT.as_bytes()).unwrap();
    let oss_result = result_config.serialize().unwrap();

    assert_eq!(
        String::from_utf8_lossy(&oss),
        String::from_utf8_lossy(&oss_result)
    );
}

/// Port of `OCIO_ADD_TEST(MergeConfigs, merge_single_colorspace)` @ v2.5.2.
#[test]
fn merge_single_colorspace() {
    const BASE: &str = r#"ocio_profile_version: 2

file_rules:
  - !<Rule> {name: Default, colorspace: A}

roles:
    a: colorspace_a

colorspaces:
- !<ColorSpace>
    name: colorspace_a
    family: utility
"#;

    const INPUT: &str = r#"ocio_profile_version: 2.1

file_rules:
  - !<Rule> {name: Default, colorspace: B}

colorspaces:
- !<ColorSpace>
    name: B
    family: aces
"#;

    const RESULT: &str = r#"ocio_profile_version: 2

environment:
  {}
search_path: ""
strictparsing: true
luma: [0.2126, 0.7152, 0.0722]

roles:
  a: colorspace_a

file_rules:
  - !<Rule> {name: Default, colorspace: A}

displays:
  {}

active_displays: []
active_views: []

colorspaces:
  - !<ColorSpace>
    name: colorspace_a
    family: Base/utility
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform

  - !<ColorSpace>
    name: B
    family: Input/aces
    equalitygroup: ""
    bitdepth: unknown
    isdata: false
    allocation: uniform"#;

    let base_config = Config::create_from_stream(BASE.as_bytes()).unwrap();
    let input_config = Config::create_from_stream(INPUT.as_bytes()).unwrap();
    let colorspace = input_config.color_space("B");

    let params = in_memory_params();

    let merged_config =
        config_merging_helpers::merge_color_space(&params, Some(&base_config), colorspace).unwrap();

    let oss = merged_config.serialize().unwrap();

    let result_config = Config::create_from_stream(RESULT.as_bytes()).unwrap();
    let oss_result = result_config.serialize().unwrap();

    assert_eq!(
        String::from_utf8_lossy(&oss),
        String::from_utf8_lossy(&oss_result)
    );
}
