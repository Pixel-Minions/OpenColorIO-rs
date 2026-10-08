// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

//! Tests of the built-in configs: `tests/cpu/builtinconfigs/BuiltinConfig_tests.cpp` @ v2.5.2.
//! `create_builtin_config` reads them through `Config::CreateFromEnv` and
//! `Config::CreateFromFile` too, and comes with them (WP 3.10b). The registry's texts, names
//! and flags are compared with the wheel's in `tests/builtin_configs_oracle.rs`.

use ocio_testkit::upstream::check_throw_what;

use super::builtin_config_registry::{
    BuiltinConfigRegistry, OCIO_BUILTIN_URI_PREFIX, resolve_config_path,
};
use super::cg::{
    CG_CONFIG_V100_ACES_V13_OCIO_V21, CG_CONFIG_V210_ACES_V13_OCIO_V23,
    CG_CONFIG_V220_ACES_V13_OCIO_V24, CG_CONFIG_V400_ACES_V20_OCIO_V25,
};
use super::studio::{
    STUDIO_CONFIG_V100_ACES_V13_OCIO_V21, STUDIO_CONFIG_V210_ACES_V13_OCIO_V23,
    STUDIO_CONFIG_V220_ACES_V13_OCIO_V24, STUDIO_CONFIG_V400_ACES_V20_OCIO_V25,
};
use super::{embedded_len, embedded_text};

/// Port of `OCIO_ADD_TEST(BuiltinConfigs, basic)` @ v2.5.2.
#[test]
fn basic() {
    let registry = BuiltinConfigRegistry::get();

    assert_eq!(registry.num_builtin_configs(), 8);

    // Each built-in config: its index, name, name for user interfaces, text and whether it is
    // recommended, as upstream checks them one block each.
    let cases: [(usize, &str, &str, &[u8], bool); 8] = [
        // Test builtin config cg-config-v1.0.0_aces-v1.3_ocio-v2.1.
        (
            0,
            "cg-config-v1.0.0_aces-v1.3_ocio-v2.1",
            "Academy Color Encoding System - CG Config [COLORSPACES v1.0.0] [ACES v1.3] [OCIO v2.1]",
            CG_CONFIG_V100_ACES_V13_OCIO_V21,
            false,
        ),
        // Test builtin config cg-config-v2.1.0_aces-v1.3_ocio-v2.3.
        (
            1,
            "cg-config-v2.1.0_aces-v1.3_ocio-v2.3",
            "Academy Color Encoding System - CG Config [COLORSPACES v2.0.0] [ACES v1.3] [OCIO v2.3]",
            CG_CONFIG_V210_ACES_V13_OCIO_V23,
            false,
        ),
        // Test builtin config cg-config-v2.2.0_aces-v1.3_ocio-v2.4.
        (
            2,
            "cg-config-v2.2.0_aces-v1.3_ocio-v2.4",
            "Academy Color Encoding System - CG Config [COLORSPACES v2.2.0] [ACES v1.3] [OCIO v2.4]",
            CG_CONFIG_V220_ACES_V13_OCIO_V24,
            false,
        ),
        // Test builtin config cg-config-v4.0.0_aces-v2.0_ocio-v2.5.
        (
            3,
            "cg-config-v4.0.0_aces-v2.0_ocio-v2.5",
            "Academy Color Encoding System - CG Config [COLORSPACES v4.0.0] [ACES v2.0] [OCIO v2.5]",
            CG_CONFIG_V400_ACES_V20_OCIO_V25,
            true,
        ),
        // Test builtin config studio-config-v1.0.0_aces-v1.3_ocio-v2.1.
        (
            4,
            "studio-config-v1.0.0_aces-v1.3_ocio-v2.1",
            "Academy Color Encoding System - Studio Config [COLORSPACES v1.0.0] [ACES v1.3] [OCIO v2.1]",
            STUDIO_CONFIG_V100_ACES_V13_OCIO_V21,
            false,
        ),
        // Test builtin config studio-config-v2.1.0_aces-v1.3_ocio-v2.3.
        (
            5,
            "studio-config-v2.1.0_aces-v1.3_ocio-v2.3",
            "Academy Color Encoding System - Studio Config [COLORSPACES v2.0.0] [ACES v1.3] [OCIO v2.3]",
            STUDIO_CONFIG_V210_ACES_V13_OCIO_V23,
            false,
        ),
        // Test builtin config studio-config-v2.2.0_aces-v1.3_ocio-v2.4.
        (
            6,
            "studio-config-v2.2.0_aces-v1.3_ocio-v2.4",
            "Academy Color Encoding System - Studio Config [COLORSPACES v2.2.0] [ACES v1.3] [OCIO v2.4]",
            STUDIO_CONFIG_V220_ACES_V13_OCIO_V24,
            false,
        ),
        // Test builtin config studio-config-v4.0.0_aces-v2.0_ocio-v2.5.
        (
            7,
            "studio-config-v4.0.0_aces-v2.0_ocio-v2.5",
            "Academy Color Encoding System - Studio Config [COLORSPACES v4.0.0] [ACES v2.0] [OCIO v2.5]",
            STUDIO_CONFIG_V400_ACES_V20_OCIO_V25,
            true,
        ),
    ];

    for (cfgidx, config_name, ui_name, text, recommended) in cases {
        assert_eq!(
            registry.builtin_config_name(cfgidx).unwrap(),
            config_name.as_bytes()
        );

        assert_eq!(
            registry.builtin_config_ui_name(cfgidx).unwrap(),
            ui_name.as_bytes()
        );

        assert_eq!(registry.builtin_config(cfgidx).unwrap(), text);

        assert_eq!(
            registry
                .builtin_config_by_name(config_name.as_bytes())
                .unwrap(),
            text
        );

        assert_eq!(
            registry.is_builtin_config_recommended(cfgidx).unwrap(),
            recommended
        );
    }

    // ********************************
    // Testing some expected failures.
    // ********************************

    // Test isBuiltinConfigRecommended using an invalid config index.
    check_throw_what(
        registry.is_builtin_config_recommended(999),
        "Config index is out of range.",
    );

    // Test getBuiltinConfigName using an invalid config index.
    check_throw_what(
        registry.builtin_config_name(999),
        "Config index is out of range.",
    );

    // Test getBuiltinConfigUIName using an invalid config index.
    check_throw_what(
        registry.builtin_config_ui_name(999),
        "Config index is out of range.",
    );

    // Test getBuiltinConfig using an invalid config index.
    check_throw_what(
        registry.builtin_config(999),
        "Config index is out of range.",
    );

    // Test getBuiltinConfigByName using an unknown config name.
    check_throw_what(
        registry.builtin_config_by_name(b"I do not exist"),
        "Could not find 'I do not exist' in the built-in configurations.",
    );
}

/// Port of `OCIO_ADD_TEST(BuiltinConfigs, basic_impl)` @ v2.5.2.
#[test]
fn basic_impl() {
    {
        // Test the addBuiltin method.

        let mut registry = BuiltinConfigRegistry::new();

        // Add configs into the built-ins config registry
        const SIMPLE_CONFIG: &str = concat!(
            "ocio_profile_version: 1\n",
            "colorspaces:\n",
            "  - !<ColorSpace>\n",
            "      name: raw\n",
            "  - !<ColorSpace>\n",
            "      name: linear\n",
            "roles:\n",
            "  default: raw\n",
            "displays:\n",
            "  sRGB:\n",
            "  - !<View> {name: Raw, colorspace: raw}\n",
            "\n",
        );

        // Add first config.
        registry.add_builtin(
            b"simple_config_1",
            b"My simple config display name #1",
            SIMPLE_CONFIG.as_bytes(),
            false,
        );
        // Add second config.
        registry.add_builtin(
            b"simple_config_2",
            b"My simple config display name #2",
            SIMPLE_CONFIG.as_bytes(),
            true,
        );

        assert_eq!(registry.num_builtin_configs(), 2);

        // Tests to check if the config #1 was added correctly.
        assert_eq!(registry.builtin_config_name(0).unwrap(), b"simple_config_1");
        assert_eq!(
            registry.builtin_config_ui_name(0).unwrap(),
            b"My simple config display name #1"
        );

        // Tests to check if the config #2 was added correctly.
        assert_eq!(registry.builtin_config_name(1).unwrap(), b"simple_config_2");
        assert_eq!(
            registry.builtin_config_ui_name(1).unwrap(),
            b"My simple config display name #2"
        );
    }
}

/// Port of `OCIO_ADD_TEST(BuiltinConfigs, resolve_config_path)` @ v2.5.2.
#[test]
fn resolve_config_path_test() {
    assert_eq!(
        resolve_config_path(b"ocio://default"),
        b"ocio://cg-config-v4.0.0_aces-v2.0_ocio-v2.5"
    );

    assert_eq!(
        resolve_config_path(b"ocio://cg-config-latest"),
        b"ocio://cg-config-v4.0.0_aces-v2.0_ocio-v2.5"
    );

    assert_eq!(
        resolve_config_path(b"ocio://studio-config-latest"),
        b"ocio://studio-config-v4.0.0_aces-v2.0_ocio-v2.5"
    );

    // ******************************************************************************
    // Paths that are not starting with "ocio://" are simply returned unmodified.
    // ******************************************************************************

    assert_eq!(
        resolve_config_path(b"studio-config-latest"),
        b"studio-config-latest"
    );

    assert_eq!(
        resolve_config_path(b"studio-config-latest.ocio"),
        b"studio-config-latest.ocio"
    );

    assert_eq!(
        resolve_config_path(b"/usr/local/share/aces.ocio"),
        b"/usr/local/share/aces.ocio"
    );

    assert_eq!(
        resolve_config_path(b"C:\\myconfig\\config.ocio"),
        b"C:\\myconfig\\config.ocio"
    );

    assert_eq!(resolve_config_path(b""), b"");

    // *****************************************************
    // The function does not try to validate to catch
    // mistakes in URI usage. That's up to the application.
    // *****************************************************

    // Unknown built-in config.
    assert_eq!(
        resolve_config_path(b"ocio://not-a-builtin"),
        b"ocio://not-a-builtin"
    );

    // Missing "//".
    assert_eq!(resolve_config_path(b"ocio:default"), b"ocio:default");

    assert_eq!(OCIO_BUILTIN_URI_PREFIX, "ocio://");
}

/// The embedded text of a file with LF or CR LF line ends is the same, with the line ends of
/// this platform's wheel: CR LF on Windows, LF elsewhere (I-148).
#[test]
fn embedded_texts_end_lines_as_the_wheel_of_the_platform() {
    const LF: &[u8] = b"a: 1\n\nb: [x, y]\n";
    const CRLF: &[u8] = b"a: 1\r\n\r\nb: [x, y]\r\n";
    let from_lf = embedded_text::<{ embedded_len(LF) }>(LF);
    let from_crlf = embedded_text::<{ embedded_len(CRLF) }>(CRLF);
    let expected: &[u8] = if cfg!(windows) { CRLF } else { LF };
    assert_eq!(from_lf, expected);
    assert_eq!(from_crlf, expected);
}
