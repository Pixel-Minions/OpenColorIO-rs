// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the built-in transform registry:
//! `tests/cpu/transforms/builtins/BuiltinTransformRegistry_tests.cpp` @ v2.5.2. `aces` builds
//! the ops of three entries and `read_write` every entry's processor: they come with the
//! builders (WP 3.2e-g, `p3-after-p2`). The `version_*_validation` tests read configs, which
//! `checkVersionConsistency` refuses (`p3-yaml-load-2`). The registry's styles are compared
//! with the wheel's in `tests/config_transforms_oracle.rs`.

use super::*;
use ocio_ops::platform::strcasecmp;
use ocio_testkit::upstream::check_throw_what;

/// Port of `OCIO_ADD_TEST(Builtins, basic)` @ v2.5.2.
#[test]
fn basic() {
    // Create an empty built-in transform registry.

    let mut registry = BuiltinTransformRegistry::new();
    assert_eq!(registry.num_builtins(), 0);
    check_throw_what(registry.builtin_style(0), "Invalid index.");

    let mut ops = OpVec::new();
    check_throw_what(registry.create_ops(0, &mut ops), "Invalid index.");

    // Add a built-in transform.

    let empty_functor: OpCreator = Arc::new(|_ops: &mut OpVec| Ok(()));
    registry.add_builtin(b"trans1", None, empty_functor.clone());

    assert_eq!(registry.num_builtins(), 1);
    assert!(strcasecmp(registry.builtin_style(0).unwrap(), "trans1").is_eq());

    // Add an existing built-in transform i.e. replace the existing one.

    registry.add_builtin(b"trans1", None, empty_functor);
    assert_eq!(registry.num_builtins(), 1);
    assert!(strcasecmp(registry.builtin_style(0).unwrap(), "trans1").is_eq());

    assert!(registry.create_ops(0, &mut ops).is_ok());
}

/// A style that differs only in case replaces the entry, which takes the new spelling and
/// description; a description stops at its first NUL, and a null one is empty.
#[test]
fn replacing_ignores_case() {
    let mut registry = BuiltinTransformRegistry::new();
    let creator: OpCreator = Arc::new(|_ops: &mut OpVec| Ok(()));
    registry.add_builtin(b"Trans1", Some(b"first"), creator.clone());
    registry.add_builtin(b"other", Some(b"two\0three"), creator.clone());
    registry.add_builtin(b"tRANS1", None, creator);
    assert_eq!(registry.num_builtins(), 2);
    assert_eq!(registry.builtin_style(0).unwrap(), b"tRANS1");
    assert_eq!(registry.builtin_description(0).unwrap(), b"");
    assert_eq!(registry.builtin_description(1).unwrap(), b"two");
    check_throw_what(registry.builtin_description(2), "Invalid index.");
}

/// The global registry's ops: an index past its entries is upstream's error, and an entry whose
/// ops are not ported yet says so, in both directions.
#[test]
fn ops_of_the_global_registry() {
    let registry = BuiltinTransformRegistry::get();
    let mut ops = OpVec::new();
    for dir in [TransformDirection::Forward, TransformDirection::Inverse] {
        check_throw_what(
            create_builtin_transform_ops(&mut ops, registry.num_builtins(), dir),
            "Invalid built-in transform name.",
        );
        check_throw_what(
            create_builtin_transform_ops(&mut ops, 0, dir),
            "BuiltinTransform: the ops of 'IDENTITY' are not ported yet.",
        );
    }
    assert!(ops.is_empty());
}

/// Port of `OCIO_ADD_TEST(Builtins, version_1_validation)` @ v2.5.2.
#[test]
fn version_1_validation() {
    let _env = crate::test_env::EnvGuard::new();
    // The unit test validates that the config reader throws for version 1 configs containing
    // a builtin transform.

    const CONFIG: &str = r#"ocio_profile_version: 1

search_path: ""
strictparsing: true
luma: [0.2126, 0.7152, 0.0722]

roles:
  default: ref

displays:
  Disp1:
    - !<View> {name: View1, colorspace: test}

colorspaces:
  - !<ColorSpace>
    name: ref

  - !<ColorSpace>
    name: test
    to_reference: !<BuiltinTransform> {style: ACEScct_to_ACES2065-1}"#;

    check_throw_what(
        crate::Config::create_from_stream(CONFIG.as_bytes()),
        "Only config version 2 (or higher) can have BuiltinInTransform.",
    );
}

/// Port of `OCIO_ADD_TEST(Builtins, version_2_validation)` @ v2.5.2.
#[test]
fn version_2_validation() {
    let _env = crate::test_env::EnvGuard::new();
    // The unit test validates that the config reader throws for version 2 configs containing
    // a builtin transform with the style 'ACES-LMT - ACES 1.3 Reference Gamut Compression'.

    const CONFIG: &str = r#"ocio_profile_version: 2

environment:
  {}
search_path: ""
strictparsing: true
luma: [0.2126, 0.7152, 0.0722]

roles:
  default: ref

file_rules:
  - !<Rule> {name: Default, colorspace: default}

displays:
  Disp1:
    - !<View> {name: View1, colorspace: test}

active_displays: []
active_views: []

colorspaces:
  - !<ColorSpace>
    name: ref

  - !<ColorSpace>
    name: test
    from_scene_reference: !<BuiltinTransform> {style: ACES-LMT - ACES 1.3 Reference Gamut Compression}"#;

    check_throw_what(
        crate::Config::create_from_stream(CONFIG.as_bytes()),
        "Only config version 2.1 (or higher) can have BuiltinTransform style 'ACES-LMT - ACES \
         1.3 Reference Gamut Compression'.",
    );
}

/// Port of `OCIO_ADD_TEST(Builtins, version_2_1_validation)` @ v2.5.2.
#[test]
fn version_2_1_validation() {
    let _env = crate::test_env::EnvGuard::new();
    // The unit test validates that the config reader checkVersionConsistency check throws for
    // version 2.1 configs containing a Builtin Transform with the 2.2 style for ARRI LogC4.

    const CONFIG: &str = r#"ocio_profile_version: 2.1

environment:
  {}
search_path: ""
strictparsing: true
luma: [0.2126, 0.7152, 0.0722]

roles:
  default: ref

file_rules:
  - !<Rule> {name: Default, colorspace: default}

displays:
  Disp1:
    - !<View> {name: View1, colorspace: test}

active_displays: []
active_views: []

colorspaces:
  - !<ColorSpace>
    name: ref

  - !<ColorSpace>
    name: test
    from_scene_reference: !<BuiltinTransform> {style: ARRI_LOGC4_to_ACES2065-1}"#;

    check_throw_what(
        crate::Config::create_from_stream(CONFIG.as_bytes()),
        "Only config version 2.2 (or higher) can have BuiltinTransform style \
         'ARRI_LOGC4_to_ACES2065-1'.",
    );
}

/// Upstream's `TestStyle` (BuiltinTransformRegistry_tests.cpp:315-360 @ v2.5.2): a version 2.3
/// config with a built-in transform of `style` is refused.
fn test_style(style: &str) {
    const BASE: &str = r#"ocio_profile_version: 2.3

environment:
  {}
search_path: ""
strictparsing: true
luma: [0.2126, 0.7152, 0.0722]

roles:
  default: ref

file_rules:
  - !<Rule> {name: Default, colorspace: default}

displays:
  Disp1:
    - !<View> {name: View1, colorspace: test}

active_displays: []
active_views: []

colorspaces:
  - !<ColorSpace>
    name: ref

  - !<ColorSpace>
    name: test
    from_scene_reference: !<BuiltinTransform> {style: "#;

    let config = format!("{BASE}{style}}}");

    let err_msg =
        format!("Only config version 2.4 (or higher) can have BuiltinTransform style '{style}'.");

    check_throw_what(
        crate::Config::create_from_stream(config.as_bytes()),
        &err_msg,
    );
}

/// Port of `OCIO_ADD_TEST(Builtins, version_2_3_validation)` @ v2.5.2.
#[test]
fn version_2_3_validation() {
    let _env = crate::test_env::EnvGuard::new();
    // The unit test validates that the config reader checkVersionConsistency check throws for
    // version 2.3 configs containing a Builtin Transform with the new 2.4 styles.

    test_style("APPLE_LOG_to_ACES2065-1");
    test_style("CURVE - APPLE_LOG_to_LINEAR");
    test_style("CURVE - HLG-OETF");
    test_style("CURVE - HLG-OETF-INVERSE");
    test_style("DISPLAY - CIE-XYZ-D65_to_DCDM-D65");
    test_style("DISPLAY - CIE-XYZ-D65_to_ST2084-DCDM-D65");
    test_style("ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-REC709_2.0");
    test_style("ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-P3-D65_2.0");
    test_style("ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-108nit-P3-D65_2.0");
    test_style("ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-300nit-P3-D65_2.0");
    test_style("ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-500nit-P3-D65_2.0");
    test_style("ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-P3-D65_2.0");
    test_style("ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-2000nit-P3-D65_2.0");
    test_style("ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-P3-D65_2.0");
    test_style("ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-500nit-REC2020_2.0");
    test_style("ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-REC2020_2.0");
    test_style("ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-2000nit-REC2020_2.0");
    test_style("ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-REC2020_2.0");
    test_style("ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-REC709-D60-in-REC709-D65_2.0");
    test_style("ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-REC709-D60-in-P3-D65_2.0");
    test_style(
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-REC709-D60-in-REC2020-D65_2.0",
    );
    test_style("ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-P3-D60-in-P3-D65_2.0");
    test_style("ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - SDR-100nit-P3-D60-in-XYZ-E_2.0");
    test_style("ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-108nit-P3-D60-in-P3-D65_2.0");
    test_style("ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-300nit-P3-D60-in-XYZ-E_2.0");
    test_style("ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-500nit-P3-D60-in-P3-D65_2.0");
    test_style("ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-P3-D60-in-P3-D65_2.0");
    test_style("ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-2000nit-P3-D60-in-P3-D65_2.0");
    test_style("ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-P3-D60-in-P3-D65_2.0");
    test_style("ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-500nit-P3-D60-in-REC2020-D65_2.0");
    test_style("ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-P3-D60-in-REC2020-D65_2.0");
    test_style("ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-2000nit-P3-D60-in-REC2020-D65_2.0");
    test_style("ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-P3-D60-in-REC2020-D65_2.0");
    test_style(
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-500nit-REC2020-D60-in-REC2020-D65_2.0",
    );
    test_style(
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-1000nit-REC2020-D60-in-REC2020-D65_2.0",
    );
    test_style(
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-2000nit-REC2020-D60-in-REC2020-D65_2.0",
    );
    test_style(
        "ACES-OUTPUT - ACES2065-1_to_CIE-XYZ-D65 - HDR-4000nit-REC2020-D60-in-REC2020-D65_2.0",
    );
}
